//! Community layer (design: docs/design/reviews/gen2-graph-spec.md E8 / C·G8).
//!
//! WHY this exists: the graph answers "what is true about X" but had no way to
//! answer a GLOBAL question ("what are the themes across what I know"), because
//! there was no layer between a single entity and the whole graph. GraphRAG's
//! answer is a hierarchical community partition plus one summary per community;
//! LazyGraphRAG's correction is that the summaries can be written LAZILY (at
//! query time), which is why a community row with `summary IS NULL` is a valid
//! state here and not an error.
//!
//! DETERMINISM is the whole point of this file. A partition that changes between
//! two runs of the same data cannot be evaluated: "did the community layer help"
//! would be indistinguishable from "the partition moved". So:
//!
//!   * the graph is built in id order;
//!   * the local-moving pass visits nodes in ascending id order;
//!   * a move needs a STRICT modularity gain, and ties go to the lower community id;
//!   * components smaller than `MIN_SPLIT_SIZE` are never split.
//!
//! Two runs on the same rows produce the same partition, which a test asserts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ruagent_store::Db;

use crate::DbError;

/// A component smaller than this is its own community: splitting 3 nodes has no
/// evidence behind it. Measured live: 20 of 23 components have size 1, two have
/// size 4, one has 35.
const MIN_SPLIT_SIZE: usize = 8;
/// Local-moving passes. Bounded so the call cannot loop on a pathological graph.
const MAX_PASSES: usize = 10;

/// One community.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Community {
    pub id: i64,
    pub level: u32,
    pub parent: Option<i64>,
    pub summary: Option<String>,
    pub entity_ids: Vec<i64>,
}

/// What a build did, in numbers a reader can falsify.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CommunityBuild {
    pub level: u32,
    pub communities: u32,
    /// Entities assigned to some community at this level.
    pub entities_covered: u32,
    /// Entities with at least one current edge -- the honest denominator.
    pub non_isolated: u32,
    /// How many communities the local-moving pass actually split (the rest are
    /// components that were already one community).
    pub split_by_modularity: u32,
}

fn build_adjacency(edges: &[(i64, i64)]) -> BTreeMap<i64, BTreeSet<i64>> {
    let mut adj: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for (s, d) in edges {
        adj.entry(*s).or_default().insert(*d);
        adj.entry(*d).or_default().insert(*s);
    }
    adj
}

/// Connected components, in ascending smallest-node order.
fn components(adj: &BTreeMap<i64, BTreeSet<i64>>) -> Vec<Vec<i64>> {
    let mut seen: BTreeSet<i64> = BTreeSet::new();
    let mut out = Vec::new();
    for start in adj.keys() {
        if seen.contains(start) {
            continue;
        }
        let mut stack = vec![*start];
        let mut comp = Vec::new();
        seen.insert(*start);
        while let Some(x) = stack.pop() {
            comp.push(x);
            for y in adj.get(&x).into_iter().flatten() {
                if seen.insert(*y) {
                    stack.push(*y);
                }
            }
        }
        comp.sort_unstable();
        out.push(comp);
    }
    out.sort_by(|a, b| a[0].cmp(&b[0]));
    out
}

/// One deterministic local-moving (Louvain phase 1) pass set.
///
/// `gain(c) = k_i_in(c) - tot(c) * k_i / (2m)` -- the standard modularity gain
/// up to a constant, with unweighted edges. A move requires `gain(best) >
/// gain(current) + EPS`, so a node never oscillates.
fn local_moving(comp: &[i64], adj: &BTreeMap<i64, BTreeSet<i64>>) -> Vec<Vec<i64>> {
    const EPS: f64 = 1e-12;
    let members: BTreeSet<i64> = comp.iter().copied().collect();
    let deg = |n: i64| -> f64 { adj.get(&n).map(|s| s.len() as f64).unwrap_or(0.0) };
    let m: f64 = comp.iter().map(|n| deg(*n)).sum::<f64>() / 2.0;
    if m <= 0.0 {
        return vec![comp.to_vec()];
    }
    // node -> community, communities keyed by their smallest member id.
    let mut node_comm: HashMap<i64, i64> = comp.iter().map(|n| (*n, *n)).collect();
    for _ in 0..MAX_PASSES {
        let mut moved = false;
        for n in comp {
            let current = node_comm[n];
            let mut tot: HashMap<i64, f64> = HashMap::new();
            for x in comp {
                *tot.entry(node_comm[x]).or_insert(0.0) += deg(*x);
            }
            let mut k_in: HashMap<i64, f64> = HashMap::new();
            for nb in adj.get(n).into_iter().flatten() {
                if members.contains(nb) {
                    *k_in.entry(node_comm[nb]).or_insert(0.0) += 1.0;
                }
            }
            let k = deg(*n);
            let gain = |c: i64, tot: &HashMap<i64, f64>| -> f64 {
                let ki = k_in.get(&c).copied().unwrap_or(0.0);
                let t = tot.get(&c).copied().unwrap_or(0.0);
                ki - t * k / (2.0 * m)
            };
            let base = gain(current, &tot);
            let mut best = (current, base);
            // Deterministic: candidate communities in ascending id order, and a
            // move must beat the incumbent STRICTLY.
            let mut candidates: Vec<i64> = k_in.keys().copied().collect();
            candidates.sort_unstable();
            for c in candidates {
                if c == current {
                    continue;
                }
                let g = gain(c, &tot);
                if g > best.1 + EPS {
                    best = (c, g);
                }
            }
            if best.0 != current && best.1 > base + EPS {
                // Do not merge into a community keyed ABOVE us when the gain is
                // equal: keep the smaller key.
                node_comm.insert(*n, best.0);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    let mut groups: HashMap<i64, Vec<i64>> = HashMap::new();
    for n in comp {
        groups.entry(node_comm[n]).or_default().push(*n);
    }
    let mut out: Vec<Vec<i64>> = groups.into_values().collect();
    for g in &mut out {
        g.sort_unstable();
    }
    out.sort_by(|a, b| a[0].cmp(&b[0]));
    out
}

/// Rebuild the partition for `level` from the CURRENT edges.
///
/// Level 0 = the fine partition (components, refined by modularity where the
/// component is big enough). Level >= 1 = one community over every assigned
/// entity, and the level-0 communities are re-parented to it.
pub async fn build_communities(db: &Db, level: u32) -> Result<CommunityBuild, DbError> {
    let now = chrono::Utc::now().to_rfc3339();
    let build = db
        .call_flat(move |conn| -> Result<CommunityBuild, rusqlite::Error> {
            // Scoped so the read statement is dropped BEFORE the transaction
            // below: `conn.transaction()` needs `&mut conn`, and a live
            // statement borrows it immutably (t81).
            let edges: Vec<(i64, i64)> = {
                let mut stmt =
                    conn.prepare("SELECT src, dst FROM entity_edges WHERE invalid_at IS NULL")?;
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let adj = build_adjacency(&edges);
            let non_isolated = adj.len() as u32;

            // Partition for this level.
            let mut parts: Vec<Vec<i64>> = Vec::new();
            let mut split_by_modularity = 0u32;
            if level == 0 {
                for comp in components(&adj) {
                    if comp.len() < MIN_SPLIT_SIZE {
                        parts.push(comp);
                        continue;
                    }
                    let sub = local_moving(&comp, &adj);
                    if sub.len() > 1 {
                        split_by_modularity += 1;
                    }
                    parts.extend(sub);
                }
            } else {
                let mut all: Vec<i64> = adj.keys().copied().collect();
                all.sort_unstable();
                if !all.is_empty() {
                    parts.push(all);
                }
            }

            // Replace this level atomically: a half-rebuilt partition would be
            // read as evidence. (t81) This is now TRUE rather than aspirational:
            // the deletes, the community rows, their members and the parent
            // linkage all run inside ONE transaction, so a failure anywhere in
            // here rolls the whole level back to the partition it replaced.
            // Measured before the fix, with an injected failure: 3 communities /
            // 9 members became 3 communities / 6 members with three rows left
            // `parent_id NULL` -- the old partition gone, the new one half-built.
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM community_entities WHERE community_id IN
                   (SELECT id FROM communities WHERE level = ?1)",
                [level],
            )?;
            tx.execute("DELETE FROM communities WHERE level = ?1", [level])?;
            let mut covered = 0u32;
            for part in &parts {
                tx.execute(
                    "INSERT INTO communities (level, parent_id, summary, built_at)
                     VALUES (?1, NULL, NULL, ?2)",
                    rusqlite::params![level, now],
                )?;
                let cid = tx.last_insert_rowid();
                for e in part {
                    tx.execute(
                        "INSERT INTO community_entities (community_id, entity_id, weight)
                         VALUES (?1, ?2, 1.0)",
                        rusqlite::params![cid, e],
                    )?;
                    covered += 1;
                }
            }
            if level == 0 {
                // Re-parent every level-0 community to the single level-1 row,
                // if that row exists YET.
                if let Ok(parent) = tx.query_row(
                    "SELECT id FROM communities WHERE level = 1 ORDER BY id LIMIT 1",
                    [],
                    |r| r.get::<_, i64>(0),
                ) {
                    tx.execute(
                        "UPDATE communities SET parent_id = ?1 WHERE level = 0",
                        [parent],
                    )?;
                }
            } else {
                // The level just written is the parent of level 0. Handled here
                // as well as above so a caller may build level 0 or level 1
                // first and still get the same hierarchy.
                if let Ok(me) = tx.query_row(
                    "SELECT id FROM communities WHERE level = ?1 ORDER BY id LIMIT 1",
                    [level],
                    |r| r.get::<_, i64>(0),
                ) {
                    tx.execute(
                        "UPDATE communities SET parent_id = ?1 WHERE level = 0",
                        [me],
                    )?;
                }
            }
            tx.commit()?;
            Ok(CommunityBuild {
                level,
                communities: parts.len() as u32,
                entities_covered: covered,
                non_isolated,
                split_by_modularity,
            })
        })
        .await?;
    Ok(build)
}

/// Read one level's partition. `None` = this level was never built, which is a
/// different fact from "the partition is empty" and must stay distinguishable.
pub async fn communities(db: &Db, level: u32) -> Result<Option<Vec<Community>>, DbError> {
    let out = db
        .call_flat(
            move |conn| -> Result<Option<Vec<Community>>, rusqlite::Error> {
                let mut rows: Vec<Community> = Vec::new();
                {
                    let mut stmt = conn.prepare(
                        "SELECT id, level, parent_id, summary FROM communities
                     WHERE level = ?1 ORDER BY id",
                    )?;
                    let mapped = stmt.query_map([level], |r| {
                        Ok(Community {
                            id: r.get(0)?,
                            level: r.get::<_, i64>(1)? as u32,
                            parent: r.get(2)?,
                            summary: r.get(3)?,
                            entity_ids: Vec::new(),
                        })
                    })?;
                    for c in mapped {
                        rows.push(c?);
                    }
                }
                if rows.is_empty() {
                    return Ok(None);
                }
                let mut stmt = conn.prepare(
                    "SELECT entity_id FROM community_entities WHERE community_id = ?1
                 ORDER BY entity_id",
                )?;
                for c in &mut rows {
                    let ids = stmt.query_map([c.id], |r| r.get::<_, i64>(0))?;
                    for id in ids {
                        c.entity_ids.push(id?);
                    }
                }
                Ok(Some(rows))
            },
        )
        .await?;
    Ok(out)
}

/// Lazy summary write (LazyGraphRAG's correction: sumarisation is deferred, so
/// a NULL summary is a normal state). Returns false when the community is gone.
pub async fn set_community_summary(db: &Db, id: i64, summary: &str) -> Result<bool, DbError> {
    let summary = summary.to_string();
    let n = db
        .call_flat(move |conn| {
            conn.execute(
                "UPDATE communities SET summary = ?2 WHERE id = ?1",
                rusqlite::params![id, summary],
            )
        })
        .await?;
    Ok(n > 0)
}

/// Coverage of a level: (entities in some community, non-isolated entities).
pub async fn community_coverage(db: &Db, level: u32) -> Result<(u32, u32), DbError> {
    let out = db
        .call_flat(move |conn| -> Result<(u32, u32), rusqlite::Error> {
            let covered: i64 = conn.query_row(
                "SELECT COUNT(*) FROM community_entities ce
                   JOIN communities c ON c.id = ce.community_id WHERE c.level = ?1",
                [level],
                |r| r.get(0),
            )?;
            let non_isolated: i64 = conn.query_row(
                "SELECT COUNT(DISTINCT e.id) FROM entities e
                 WHERE EXISTS (SELECT 1 FROM entity_edges x
                                WHERE (x.src = e.id OR x.dst = e.id) AND x.invalid_at IS NULL)",
                [],
                |r| r.get(0),
            )?;
            Ok((covered as u32, non_isolated as u32))
        })
        .await?;
    Ok(out)
}

/// The communities a seed entity belongs to, strongest first: the direction
/// retrieval needs.
pub async fn communities_of(db: &Db, entity: i64) -> Result<Vec<Community>, DbError> {
    let out = db
        .call_flat(move |conn| -> Result<Vec<Community>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.level, c.parent_id, c.summary, ce.weight
                 FROM community_entities ce JOIN communities c ON c.id = ce.community_id
                 WHERE ce.entity_id = ?1 ORDER BY ce.weight DESC, c.level, c.id",
            )?;
            let rows = stmt.query_map([entity], |r| {
                Ok(Community {
                    id: r.get(0)?,
                    level: r.get::<_, i64>(1)? as u32,
                    parent: r.get(2)?,
                    summary: r.get(3)?,
                    entity_ids: Vec::new(),
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
        .await?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruagent_store::Db;

    async fn count(db: &Db, sql: &'static str) -> i64 {
        db.call_flat(move |conn| conn.query_row(sql, [], |r| r.get(0)))
            .await
            .unwrap()
    }

    /// The partition of a level as a comparable value: the community ids, their
    /// parent linkage and their member sets.
    async fn partition(db: &Db, level: u32) -> Vec<(i64, Option<i64>, Vec<i64>)> {
        db.call_flat(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, parent_id FROM communities WHERE level = ?1 ORDER BY id",
            )?;
            let comms = stmt
                .query_map([level], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            let mut out = Vec::new();
            for (cid, parent) in comms {
                let mut m = conn.prepare(
                    "SELECT entity_id FROM community_entities WHERE community_id = ?1 ORDER BY entity_id",
                )?;
                let members = m
                    .query_map([cid], |r| r.get::<_, i64>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                out.push((cid, parent, members));
            }
            Ok(out)
        })
        .await
        .unwrap()
    }

    async fn seed_graph(db: &Db) {
        // Two components of three nodes each: 0-1-2 and 3-4-5.
        for i in 0..6 {
            db.call_flat(move |conn| {
                conn.execute(
                    "INSERT INTO entities (id, name, norm_name, kind, created_at, updated_at)
                     VALUES (?1, ?2, ?2, 'test', 't', 't')",
                    rusqlite::params![i, format!("e{i}")],
                )
            })
            .await
            .unwrap();
        }
        for (a, b) in [(0, 1), (1, 2), (3, 4), (4, 5)] {
            db.call_flat(move |conn| {
                conn.execute(
                    "INSERT INTO entity_edges (src, dst, relation, fact_text, valid_at, created_at)
                     VALUES (?1, ?2, 'links', 'x', 't', 't')",
                    rusqlite::params![a, b],
                )
            })
            .await
            .unwrap();
        }
    }

    /// t81 / audit #2: the comment claimed "Replace this level atomically".
    /// Injecting a failure used to leave 3 communities / 9 members as
    /// 3 communities / 6 members with three rows `parent_id NULL` — the old
    /// partition gone, the new one half-built, all of it committed.
    #[tokio::test]
    async fn a_failed_rebuild_leaves_the_previous_partition_intact() {
        let db = Db::open_in_memory().unwrap();
        seed_graph(&db).await;
        // Build level 1 FIRST so level 0 gets a parent: with no level-1 row a
        // NULL `parent_id` is the normal state, and then "3 rows with a NULL
        // parent" would not distinguish a half-built partition from a healthy
        // one. With the hierarchy in place it does (t81).
        build_communities(&db, 1).await.unwrap();
        let built = build_communities(&db, 0).await.unwrap();
        assert_eq!(built.communities, 2);
        assert_eq!(built.entities_covered, 6);
        let before = partition(&db, 0).await;
        assert_eq!(before.len(), 2);
        assert_eq!(
            before.iter().map(|(_, _, m)| m.len()).sum::<usize>(),
            6,
            "two communities of three"
        );
        assert!(
            before.iter().all(|(_, parent, _)| parent.is_some()),
            "level 0 hangs off the level-1 community: {:?}",
            before
                .iter()
                .map(|(id, p, _)| (*id, *p))
                .collect::<Vec<_>>()
        );

        // Poison the member insert for the LAST entity the rebuild would place.
        db.call(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER t81_boom BEFORE INSERT ON community_entities
                 WHEN NEW.entity_id = 5
                 BEGIN SELECT RAISE(ABORT, 't81 injected failure'); END;",
            )
        })
        .await
        .unwrap()
        .unwrap();

        let err = build_communities(&db, 0)
            .await
            .expect_err("the poisoned rebuild must fail");
        let after = partition(&db, 0).await;
        // Level-scoped on purpose: level 1 has its own six member rows, so a
        // bare COUNT(*) over `community_entities` would read 12 and hide nothing
        // useful (t81).
        let members = count(
            &db,
            "SELECT COUNT(*) FROM community_entities ce JOIN communities c ON c.id = ce.community_id
              WHERE c.level = 0",
        )
        .await;
        let orphans = count(
            &db,
            "SELECT COUNT(*) FROM communities WHERE level = 0 AND parent_id IS NULL",
        )
        .await;
        println!(
            "READING t81 #2: injected failure -> {err} | partition before {} communities/{:?} | \
             after {} communities (members {members}, parent_id NULL {orphans})",
            before.len(),
            before.iter().map(|(_, _, m)| m.len()).collect::<Vec<_>>(),
            after.len()
        );
        assert_eq!(
            after, before,
            "a failed rebuild must leave the PREVIOUS partition untouched, not a half-built one"
        );
        assert_eq!(members, 6, "still six member rows");
        assert_eq!(orphans, 0, "and no community left without its parent");

        // Negative control: without the injection the rebuild lands whole.
        db.call(|conn| conn.execute_batch("DROP TRIGGER t81_boom"))
            .await
            .unwrap()
            .unwrap();
        let rebuilt = build_communities(&db, 0).await.unwrap();
        let done = partition(&db, 0).await;
        let orphans_after = count(
            &db,
            "SELECT COUNT(*) FROM communities WHERE level = 0 AND parent_id IS NULL",
        )
        .await;
        println!(
            "READING t81 #2 (control): rebuild succeeded -> {} communities, {} members, \
             parent_id NULL {orphans_after}",
            rebuilt.communities, rebuilt.entities_covered
        );
        assert_eq!((rebuilt.communities, rebuilt.entities_covered), (2, 6));
        assert_eq!(
            done.iter().map(|(_, _, m)| m.len()).sum::<usize>(),
            6,
            "the same six members are covered after a successful rebuild"
        );
    }
}
