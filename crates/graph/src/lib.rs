//! ruagent-graph: the entity property graph with bi-temporal edges
//! (design §6.6 #1 — "the single highest-value idea in the whole survey").
//!
//! Edges carry two timelines: event time (`valid_at`/`invalid_at` — when
//! the fact was true in the world) and transaction time
//! (`created_at`/`expired_at` — when we recorded it). Contradictions
//! between the same entity pair + relation invalidate the old edge at the
//! new edge's valid_at; nothing is ever deleted.

use chrono::Utc;

use ruagent_store::Db;

pub use ruagent_store::DbError;

/// One entity node.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Entity {
    pub id: i64,
    pub name: String,
    pub kind: Option<String>,
    pub summary: Option<String>,
}

/// One edge (a dated fact).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Edge {
    pub id: i64,
    pub src: i64,
    pub dst: i64,
    pub relation: String,
    pub fact_text: String,
    pub valid_at: String,
    pub invalid_at: Option<String>,
    pub source_episode: Option<i64>,
}

fn norm(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Insert or resolve an entity by normalized name; returns its id.
/// Updating an existing entity refreshes kind/summary (cheap resolution:
/// exact normalized match; LLM-assisted merging is a deliberate non-goal
/// at single-user scale — design §6.6 #8).
pub async fn upsert_entity(
    db: &Db,
    name: &str,
    kind: Option<&str>,
    summary: Option<&str>,
) -> Result<i64, DbError> {
    let norm_name = norm(name);
    let name = name.trim().to_string();
    let kind = kind.map(str::to_string);
    let summary = summary.map(str::to_string);
    let now = Utc::now().to_rfc3339();
    db.call(move |conn| -> Result<i64, rusqlite::Error> {
        conn.execute(
            "INSERT INTO entities (name, norm_name, kind, summary, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(norm_name) DO UPDATE SET
                 name = excluded.name,
                 kind = COALESCE(excluded.kind, entities.kind),
                 summary = COALESCE(excluded.summary, entities.summary),
                 updated_at = excluded.updated_at",
            rusqlite::params![name, norm_name, kind, summary, now],
        )?;
        conn.query_row(
            "SELECT id FROM entities WHERE norm_name = ?1",
            [&norm_name],
            |r| r.get(0),
        )
    })
    .await?
    .map_err(DbError::from)
}

/// Add a fact. If a currently-valid edge with the same (src, dst,
/// relation) exists, it is invalidated at this fact's `valid_at`
/// (deterministic supersession — the same entity pair only, Graphiti's
/// own complexity cut).
pub async fn add_fact(
    db: &Db,
    src: i64,
    dst: i64,
    relation: &str,
    fact_text: &str,
    valid_at: Option<&str>, // None = now
    source_episode: Option<i64>,
) -> Result<i64, DbError> {
    let relation = relation.to_string();
    let fact_text = fact_text.to_string();
    let valid_at = valid_at
        .map(str::to_string)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let now = Utc::now().to_rfc3339();
    db.call(move |conn| -> Result<i64, rusqlite::Error> {
        // Invalidate the currently-valid predecessor.
        conn.execute(
            "UPDATE entity_edges
             SET invalid_at = ?5, expired_at = ?6
             WHERE src = ?1 AND dst = ?2 AND relation = ?3 AND invalid_at IS NULL",
            rusqlite::params![src, dst, relation, fact_text, valid_at, now],
        )?;
        conn.execute(
            "INSERT INTO entity_edges (src, dst, relation, fact_text, valid_at, created_at, source_episode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![src, dst, relation, fact_text, valid_at, now, source_episode],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await?
    .map_err(DbError::from)
}

/// What a hard entity delete did. `edges_removed` counts EVERY incident
/// `entity_edges` row (either direction, current or superseded);
/// `facts_removed` counts the currently-valid ones (`invalid_at IS NULL`) —
/// the facts the graph page renders. Both are reported because a row-only
/// delete leaves the page drawing dangling edges (t276).
#[derive(Debug, Clone, PartialEq)]
pub enum EntityDeleteOutcome {
    Deleted {
        id: i64,
        edges_removed: i64,
        facts_removed: i64,
    },
    NotFound,
}

/// Hard-delete an entity together with every edge and fact that touches it,
/// in either direction. An absent id reports NotFound instead of a silent
/// success (t276): "nothing was there" and "it is gone now" are different
/// facts. The FTS index follows via the `entities_ad` trigger.
pub async fn delete_entity(db: &Db, id: i64) -> Result<EntityDeleteOutcome, DbError> {
    db.call(move |conn| -> Result<EntityDeleteOutcome, rusqlite::Error> {
        let exists: i64 =
            conn.query_row("SELECT COUNT(*) FROM entities WHERE id = ?1", [id], |r| {
                r.get(0)
            })?;
        if exists == 0 {
            return Ok(EntityDeleteOutcome::NotFound);
        }
        let facts_removed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entity_edges
              WHERE (src = ?1 OR dst = ?1) AND invalid_at IS NULL",
            [id],
            |r| r.get(0),
        )?;
        let edges_removed =
            conn.execute("DELETE FROM entity_edges WHERE src = ?1 OR dst = ?1", [id])? as i64;
        conn.execute("DELETE FROM entities WHERE id = ?1", [id])?;
        Ok(EntityDeleteOutcome::Deleted {
            id,
            edges_removed,
            facts_removed,
        })
    })
    .await?
    .map_err(DbError::from)
}

/// Currently-valid facts about an entity (either direction).
pub async fn current_facts(db: &Db, entity: i64) -> Result<Vec<Edge>, DbError> {
    db.call(move |conn| -> Result<Vec<Edge>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, src, dst, relation, fact_text, valid_at, invalid_at, source_episode
             FROM entity_edges
             WHERE (src = ?1 OR dst = ?1) AND invalid_at IS NULL
             ORDER BY valid_at DESC",
        )?;
        let rows = stmt
            .query_map([entity], edge_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// Every currently-valid edge, in one request.
///
/// The per-entity queries above answer "what do I know about X" one entity at a
/// time; a canvas that draws the graph needs all of them at once, and asking per
/// node is N+1 (design §12 row 39: K=1 ⇒ R<=4). Bounded by `limit`/`offset` and
/// returning the total, so a large graph is PAGED rather than pulled whole --
/// the reason this is not just `SELECT *`.
pub async fn list_edges(db: &Db, limit: u32, offset: u32) -> Result<(Vec<Edge>, i64), DbError> {
    let limit = limit as i64;
    let offset = offset as i64;
    db.call(move |conn| -> Result<(Vec<Edge>, i64), rusqlite::Error> {
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM entity_edges WHERE invalid_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT id, src, dst, relation, fact_text, valid_at, invalid_at, source_episode
             FROM entity_edges
             WHERE invalid_at IS NULL
             ORDER BY id
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit, offset], edge_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((rows, total))
    })
    .await?
    .map_err(DbError::from)
}

/// "What was true as of X" — the bi-temporal payoff (design §6.6 #1).
pub async fn facts_as_of(db: &Db, entity: i64, at: &str) -> Result<Vec<Edge>, DbError> {
    let at = at.to_string();
    db.call(move |conn| -> Result<Vec<Edge>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, src, dst, relation, fact_text, valid_at, invalid_at, source_episode
             FROM entity_edges
             WHERE (src = ?1 OR dst = ?1)
               AND valid_at <= ?2
               AND (invalid_at IS NULL OR invalid_at > ?2)
             ORDER BY valid_at DESC",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![entity, at], edge_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// Multi-hop neighborhood via a recursive CTE (design §6.6 #3's graph
/// walk leg): entities reachable within `hops` of `entity` through
/// currently-valid edges, with the path length.
pub async fn neighbors(db: &Db, entity: i64, hops: u32) -> Result<Vec<(Entity, u32)>, DbError> {
    let max_hops = hops as i64;
    db.call(move |conn| -> Result<Vec<(Entity, u32)>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "WITH RECURSIVE walk(entity, depth) AS (
                 SELECT ?1, 0
                 UNION
                 SELECT CASE WHEN e.src = walk.entity THEN e.dst ELSE e.src END,
                        walk.depth + 1
                 FROM entity_edges e
                 JOIN walk ON (e.src = walk.entity OR e.dst = walk.entity)
                 WHERE e.invalid_at IS NULL AND walk.depth < ?2
             )
             SELECT DISTINCT ent.id, ent.name, ent.kind, ent.summary, MIN(walk.depth) AS d
             FROM walk JOIN entities ent ON ent.id = walk.entity
             WHERE walk.entity != ?1
             GROUP BY ent.id ORDER BY d, ent.name",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![entity, max_hops], |row| {
                Ok((
                    Entity {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        kind: row.get(2)?,
                        summary: row.get(3)?,
                    },
                    row.get::<_, i64>(4)? as u32,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// List entities (id desc) with their current fact counts — the graph
/// explorer's landing view.
pub async fn list_entities(db: &Db, limit: u32) -> Result<Vec<(Entity, i64)>, DbError> {
    db.call(move |conn| -> Result<Vec<(Entity, i64)>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT e.id, e.name, e.kind, e.summary,
                    (SELECT COUNT(*) FROM entity_edges x
                      WHERE (x.src = e.id OR x.dst = e.id) AND x.invalid_at IS NULL)
             FROM entities e ORDER BY e.id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit], |row| {
                Ok((
                    Entity {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        kind: row.get(2)?,
                        summary: row.get(3)?,
                    },
                    row.get(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// FTS over entity names/summaries (the keyword candidate leg of
/// resolution and retrieval). Punctuated tokens (scripts/release.sh,
/// node.js) are FTS5 syntax errors as raw input — quote each token
/// into a literal phrase.
pub async fn search_entities(db: &Db, query: &str, limit: u32) -> Result<Vec<Entity>, DbError> {
    let fts_query = query
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    db.call(move |conn| -> Result<Vec<Entity>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT e.id, e.name, e.kind, e.summary
             FROM entities_fts f JOIN entities e ON e.id = f.rowid
             WHERE entities_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![fts_query, limit], |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    summary: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// Entity search for the RECALL leg (t250): a query shaped like what a user
/// types, not like what is stored.
///
/// search_entities is unchanged (frozen interface): it quotes each whitespace
/// token into a phrase and ANDs them. Measured against the live graph (t247)
/// that misses two whole classes -- a hyphenated model whose parts are not
/// adjacent in the stored text, and any substring of a Han run. This entry
/// point degrades instead of failing:
///
///   1. precision: every term (split the way the tokenizer splits) ANDed;
///   2. recall:    the same terms as prefixes, ORed -- only if step 1 was empty;
///   3. LIKE:      the one path FTS5 cannot express, for substrings of a term.
///
/// It is a recall leg, not a ranker: FTS rank order first, then the LIKE hits
/// FTS missed, de-duplicated by id.
pub async fn search_entities_loose(
    db: &Db,
    query: &str,
    limit: u32,
) -> Result<Vec<Entity>, DbError> {
    let terms = ruagent_store::fts::terms(query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let all = ruagent_store::fts::match_all(&terms);
    let any = ruagent_store::fts::match_any_prefix(&terms);
    let likes = ruagent_store::fts::like_patterns(&terms);
    db.call(move |conn| -> Result<Vec<Entity>, rusqlite::Error> {
        let mut out: Vec<Entity> = Vec::new();
        let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let fts = |expr: &str,
                   out: &mut Vec<Entity>,
                   seen: &mut std::collections::HashSet<i64>|
         -> Result<(), rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT e.id, e.name, e.kind, e.summary
                     FROM entities_fts f JOIN entities e ON e.id = f.rowid
                     WHERE entities_fts MATCH ?1 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![expr, limit], |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    summary: row.get(3)?,
                })
            })?;
            for r in rows {
                let e = r?;
                if seen.insert(e.id) {
                    out.push(e);
                }
            }
            Ok(())
        };
        fts(&all, &mut out, &mut seen)?;
        if out.is_empty() {
            fts(&any, &mut out, &mut seen)?;
        }
        for pat in likes {
            if out.len() >= limit as usize {
                break;
            }
            let mut stmt = conn.prepare(
                "SELECT id, name, kind, summary FROM entities
                 WHERE name LIKE ?1 ESCAPE '\\' OR summary LIKE ?1 ESCAPE '\\'
                 ORDER BY id LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![pat, limit], |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    summary: row.get(3)?,
                })
            })?;
            for r in rows {
                let e = r?;
                if seen.insert(e.id) {
                    out.push(e);
                }
            }
        }
        Ok(out)
    })
    .await?
    .map_err(DbError::from)
}

fn edge_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Edge> {
    Ok(Edge {
        id: row.get(0)?,
        src: row.get(1)?,
        dst: row.get(2)?,
        relation: row.get(3)?,
        fact_text: row.get(4)?,
        valid_at: row.get(5)?,
        invalid_at: row.get(6)?,
        source_episode: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bi_temporal_supersession_and_as_of_queries() {
        let db = Db::open_in_memory().unwrap();
        let alice = upsert_entity(&db, "Alice", Some("person"), None)
            .await
            .unwrap();
        let acme = upsert_entity(&db, "Acme", Some("org"), None).await.unwrap();
        // Same entity resolves (case/space-insensitive).
        let alice2 = upsert_entity(&db, "  alice ", None, Some("engineer"))
            .await
            .unwrap();
        assert_eq!(alice, alice2);

        // Alice joins Acme in January.
        let jan = "2026-01-15T00:00:00Z";
        add_fact(
            &db,
            alice,
            acme,
            "works_at",
            "Alice works at Acme",
            Some(jan),
            None,
        )
        .await
        .unwrap();

        // Current: the January fact.
        let now_facts = current_facts(&db, alice).await.unwrap();
        assert_eq!(now_facts.len(), 1);
        assert!(now_facts[0].fact_text.contains("Acme"));

        // As of before January: nothing.
        let before = facts_as_of(&db, alice, "2025-12-01T00:00:00Z")
            .await
            .unwrap();
        assert!(before.is_empty());

        // In March she moves on — same pair+relation invalidates.
        let mar = "2026-03-01T00:00:00Z";
        add_fact(
            &db,
            alice,
            acme,
            "works_at",
            "Alice LEFT Acme",
            Some(mar),
            None,
        )
        .await
        .unwrap();

        // Current: only the March fact is valid.
        let now_facts = current_facts(&db, alice).await.unwrap();
        assert_eq!(now_facts.len(), 1);
        assert!(now_facts[0].fact_text.contains("LEFT"));

        // As of February: the January fact was true then.
        let feb = facts_as_of(&db, alice, "2026-02-01T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(feb.len(), 1);
        assert!(feb[0].fact_text.contains("works at Acme"));

        // As of April: the January fact is gone, March fact is true.
        let apr = facts_as_of(&db, alice, "2026-04-01T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(apr.len(), 1);
        assert!(apr[0].fact_text.contains("LEFT"));

        // History is never deleted.
        let total: i64 = db
            .call(|conn| conn.query_row("SELECT COUNT(*) FROM entity_edges", [], |r| r.get(0)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(total, 2);
    }

    #[tokio::test]
    async fn multi_hop_traversal() {
        let db = Db::open_in_memory().unwrap();
        let a = upsert_entity(&db, "a", None, None).await.unwrap();
        let b = upsert_entity(&db, "b", None, None).await.unwrap();
        let c = upsert_entity(&db, "c", None, None).await.unwrap();
        let d = upsert_entity(&db, "d", None, None).await.unwrap();
        add_fact(&db, a, b, "knows", "a-b", None, None)
            .await
            .unwrap();
        add_fact(&db, b, c, "knows", "b-c", None, None)
            .await
            .unwrap();
        add_fact(&db, c, d, "knows", "c-d", None, None)
            .await
            .unwrap();

        let one = neighbors(&db, a, 1).await.unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].0.name, "b");

        let two = neighbors(&db, a, 2).await.unwrap();
        assert_eq!(two.len(), 2);
        assert!(two.iter().any(|(e, d)| e.name == "c" && *d == 2));

        let three = neighbors(&db, a, 3).await.unwrap();
        assert_eq!(three.len(), 3);

        // Invalidated edges drop out of traversal.
        let _ = add_fact(&db, b, c, "knows", "b-c-broken", None, None)
            .await
            .unwrap();
        // (same pair+relation superseded; still one edge b-c current)
        let two_again = neighbors(&db, a, 2).await.unwrap();
        assert_eq!(two_again.len(), 2);
    }

    #[tokio::test]
    async fn entity_search() {
        let db = Db::open_in_memory().unwrap();
        upsert_entity(&db, "ruagent", Some("project"), Some("agent platform"))
            .await
            .unwrap();
        let hits = search_entities(&db, "agent", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "ruagent");
    }
}
