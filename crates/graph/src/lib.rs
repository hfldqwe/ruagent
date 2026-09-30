//! ruagent-graph: the entity property graph with bi-temporal edges
//! (design §6.6 #1 — "the single highest-value idea in the whole survey").
//!
//! Edges carry two timelines: event time (`valid_at`/`invalid_at` — when
//! the fact was true in the world) and transaction time
//! (`created_at`/`expired_at` — when we recorded it). Contradictions
//! between the same entity pair + relation invalidate the old edge at the
//! new edge's valid_at; nothing is ever deleted.

use chrono::{DateTime, Utc};

use ruagent_store::Db;

pub use ruagent_store::DbError;

mod community;
mod retrieve;

pub use community::{
    Community, CommunityBuild, build_communities, communities, communities_of, community_coverage,
    set_community_summary,
};
pub use retrieve::{
    DAMPING, DEFAULT_BEAM, DEFAULT_HOPS, DEFAULT_MAX_FACTS, DEFAULT_MAX_PATHS, EdgeSource,
    EmptyReason, EvidenceEdge, EvidencePath, GraphEvidence, GraphQuery, MAX_HOPS, PathRationale,
    RetrievalStats, SeedHit, SeedLeg, TemporalStatus, TruncationBudget, parse_ts, resolve_seeds,
    retrieve,
};

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

pub(crate) fn norm(name: &str) -> String {
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
    let name = name.to_string();
    let kind = kind.map(str::to_string);
    let summary = summary.map(str::to_string);
    db.call_flat(move |conn| {
        let tx = conn.transaction()?;
        let id = upsert_entity_in(&tx, &name, kind.as_deref(), summary.as_deref())?;
        tx.commit()?;
        Ok(id)
    })
    .await
}

/// `upsert_entity`'s statements, against a borrowed connection (t81).
///
/// WHY THIS EXISTS AS A SEPARATE FUNCTION: a multi-step write must run inside
/// ONE transaction, and a transaction can only be opened on a connection the
/// caller owns. Every public entry point below is therefore a thin wrapper
/// (`let tx = conn.transaction()?; …; tx.commit()?`) around one of these `*_in`
/// functions, and the aggregation entry point (`apply_extraction`) drives the
/// same functions inside a single transaction. Same shape as
/// `crates/store/src/lib.rs:838-845`.
fn upsert_entity_in(
    conn: &rusqlite::Connection,
    name: &str,
    kind: Option<&str>,
    summary: Option<&str>,
) -> rusqlite::Result<i64> {
    let norm_name = norm(name);
    let name = name.trim().to_string();
    let now = Utc::now().to_rfc3339();
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
    db.call(
        move |conn| -> Result<EntityDeleteOutcome, rusqlite::Error> {
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
        },
    )
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
///
/// t82 (audit #3, RVC-1's mirror image): the instant is **parsed**, never
/// compared as text. Until this, `at` and the two time columns met inside SQL
/// with `<=`/`>`, so ONE instant written three ways answered differently on the
/// live table — `…18:38:15Z` → 38 edges, `…18:38:15+00:00` → 30,
/// `…02:38:15+08:00` → 43 (the parse-correct answer is 30, i.e. 16/0/21 wrong),
/// with 6 of 63 entities getting a different count per spelling. The HTTP side
/// looked fine only because `api.rs:1290` pre-canonicalises with
/// `parse_ts`+`to_rfc3339` — and this is a PUBLIC API, so the bug belonged here,
/// not in the endpoint. RVC-1 fixed exactly this on the *retrieval* side and the
/// write/API side kept it; that is the "same problem, fixed in one place" trap.
///
/// A text that is not an instant is refused loudly (the same shape
/// `retrieve::retrieve`'s `as_of` uses, retrieve.rs:737-751) rather than being
/// silently string-compared. A *stored* value that is not an instant still falls
/// back to a raw-text compare against the instant's **canonical** form, so an
/// unparsable row cannot vanish from the answer and the fallback itself is
/// spelling-independent.
pub async fn facts_as_of(db: &Db, entity: i64, at: &str) -> Result<Vec<Edge>, DbError> {
    let at = parse_ts(at).ok_or_else(|| {
        DbError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "at is not an instant: {at:?} (accepted: RFC3339 with Z or an offset, \
                 with or without nanoseconds; YYYY-MM-DD; %Y-%m-%d %H:%M:%S)"
            ),
        ))
    })?;
    db.call_flat(move |conn| -> Result<Vec<Edge>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, src, dst, relation, fact_text, valid_at, invalid_at, source_episode
             FROM entity_edges
             WHERE (src = ?1 OR dst = ?1)",
        )?;
        // Filter in Rust: SQLite's text comparison cannot see that
        // `…18:38:15Z` and `…02:38:15+08:00` are the same moment, and that is
        // the whole defect.
        let mut rows = stmt
            .query_map([entity], edge_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.retain(|e| in_force_at(&e.valid_at, e.invalid_at.as_deref(), at));
        // Order by INSTANT, not by text, and break ties on `id` so the order is
        // total rather than plan-dependent (RVC-4's lesson). An unparsable
        // `valid_at` sorts last instead of shuffling the visible answer.
        rows.sort_by(|a, b| {
            parse_ts(&b.valid_at)
                .cmp(&parse_ts(&a.valid_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(rows)
    })
    .await
}

/// Is this edge in force at `at`? — instants, not text.
///
/// Same rule as the retrieval side's `true_then_at` (`retrieve.rs:670`, RVC-1);
/// it lives here as well because `retrieve.rs` is outside t82's scope, so the
/// two copies must not drift (a follow-up should make that one `pub(crate)` and
/// delete this — see the report's finding).
fn in_force_at(valid_at: &str, invalid_at: Option<&str>, at: DateTime<Utc>) -> bool {
    let at_raw = at.to_rfc3339();
    let starts = match parse_ts(valid_at) {
        Some(v) => v <= at, // inclusive start
        None => valid_at.trim() <= at_raw.as_str(),
    };
    if !starts {
        return false;
    }
    match invalid_at {
        None => true,
        Some(inv) => match parse_ts(inv) {
            Some(i) => i > at, // exclusive end
            None => inv.trim() > at_raw.as_str(),
        },
    }
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

/// One entity by id, or `None` when the id is not in the graph.
///
/// The detail route needs the name and kind BESIDE the facts: `/graph/entity/{id}`
/// used to answer `{"facts": […]}` alone, so a caller holding an id could not read
/// back what it was looking at (ruagent-close-the-gaps t22).
pub async fn entity_by_id(db: &Db, id: i64) -> Result<Option<Entity>, DbError> {
    db.call(move |conn| -> Result<Option<Entity>, rusqlite::Error> {
        conn.query_row(
            "SELECT id, name, kind, summary FROM entities WHERE id = ?1",
            [id],
            |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    summary: row.get(3)?,
                })
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
    })
    .await?
    .map_err(DbError::from)
}

/// The aliases recorded for one entity, oldest first.
///
/// This table is where a folded name goes when resolution merges it onto an entity
/// it did not create (`resolve_entity_in`'s `MergeVerdict::SameObject` arm calls
/// `add_alias_in`). Measured on the ingested corpus copy (ruagent-close-the-gaps
/// t18): of 257 distinct parenthetical names, **116 are an `entities.name` and 141
/// live ONLY here** — 103 that no strict search returns at all, and 38 whose
/// `exact` verdict is a FALSE POSITIVE on a DIFFERENT entity, because
/// `entities_fts` indexes `name, summary` and never this table. Before this read
/// existed, the alias text appeared in no response at all, so a search for a term
/// the graph knows answered with a candidate list nobody could verify.
///
/// READ-ONLY. Indexing aliases in the FTS table is a SEPARATE change: it alters
/// every existing query's results and needs its own before/after over a corpus.
pub async fn aliases_of(db: &Db, entity: i64) -> Result<Vec<String>, DbError> {
    db.call(move |conn| -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt =
            conn.prepare("SELECT alias FROM entity_aliases WHERE entity_id = ?1 ORDER BY id")?;
        let rows = stmt.query_map([entity], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()
    })
    .await?
    .map_err(DbError::from)
}

/// The aliases of MANY entities in ONE query, keyed by entity id.
///
/// A list route that opted into aliases must not fan out one query per row: the
/// single-writer actor is the bottleneck, so the whole page is read with one
/// statement (the same reasoning `list_edges` follows for the canvas). Entities
/// with no alias are ABSENT from the map rather than present-and-empty, so the
/// caller can tell "no aliases" from "not in this page".
pub async fn aliases_for(
    db: &Db,
    ids: &[i64],
) -> Result<std::collections::BTreeMap<i64, Vec<String>>, DbError> {
    let ids: Vec<i64> = ids.to_vec();
    if ids.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    db.call(
        move |conn| -> Result<std::collections::BTreeMap<i64, Vec<String>>, rusqlite::Error> {
            let marks = vec!["?"; ids.len()].join(",");
            let sql = format!(
                "SELECT entity_id, alias FROM entity_aliases
                  WHERE entity_id IN ({marks}) ORDER BY entity_id, id"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut out: std::collections::BTreeMap<i64, Vec<String>> =
                std::collections::BTreeMap::new();
            for r in rows {
                let (id, alias) = r?;
                out.entry(id).or_default().push(alias);
            }
            Ok(out)
        },
    )
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
        // An empty expression is not a query: FTS5 rejects MATCH '' with
        // "fts5: syntax error near \"\"". The recall form comes out empty
        // whenever EVERY term is below the recall floor (see
        // ruagent_store::fts::MIN_RECALL_ASCII), so a two-letter query such as
        // "v4" or "A2" would otherwise become a hard error instead of degrading
        // to the LIKE stage below. The precision form is empty only for an
        // empty term list, which the caller already rejected -- skipped anyway,
        // because the rule is "never hand MATCH an empty pattern".
        if !all.is_empty() {
            fts(&all, &mut out, &mut seen)?;
        }
        if out.is_empty() && !any.is_empty() {
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

// ---------------------------------------------------------------------------
// gen2: provenance, event-time semantics, alias resolution (spec E3-E5, C·G3/G4/G6)
// ---------------------------------------------------------------------------

/// Normalised fact text: lowercase with runs of whitespace collapsed. This is
/// the identity of a FACT, so `Alice  works at Acme.` and `alice works at acme.`
/// are one fact and not two edges.
pub fn normalize_fact(fact: &str) -> String {
    fact.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Stable 64-bit FNV-1a hex over the normalised fact text.
///
/// WHY not `DefaultHasher`: that is SipHash with an unspecified seed policy, and
/// this value is PERSISTED (`entity_edges.fact_hash`). A stored hash that can
/// stop matching after a toolchain change is not an identity. FNV-1a is fully
/// specified here, so any reader can recompute it.
pub fn fact_hash(fact: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in normalize_fact(fact).as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Where an edge's `valid_at` came from. Exactly the vocabulary
/// `0021_graph_evidence.sql` documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTimeSource {
    /// The extraction returned an event time for this fact.
    Extracted,
    /// There was no event time: `valid_at` is the write clock.
    Recorded,
}

impl EventTimeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            EventTimeSource::Extracted => "extracted",
            EventTimeSource::Recorded => "recorded",
        }
    }
}

/// The extraction-side writer: records the fact's identity, where `valid_at`
/// came from, and which episode the fact came from; supersedes the currently
/// valid `(src, dst, relation)` predecessor exactly like `add_fact`.
///
/// WHY this is a SECOND function instead of a parameter on `add_fact`:
/// `add_fact` is a frozen interface (spec §D.3) with live call sites in
/// `api.rs` and in this crate's own tests, and it must keep writing NULL for
/// `event_time_source` -- a panel/CLI write did not go through extraction, so
/// that writer genuinely does not know where the timestamp came from.
///
/// `valid_at: None` forces `Recorded`: a NULL event time means the clock
/// stamped the value, so claiming `Extracted` would be a lie.
///
/// The eight parameters are the eight fields of the row, named one by one on
/// purpose: a struct here would let a caller forget `event_time_source` (the
/// column whose absence is the whole finding), and the extraction writer is the
/// only caller.
#[allow(clippy::too_many_arguments)]
pub async fn add_fact_with_source(
    db: &Db,
    src: i64,
    dst: i64,
    relation: &str,
    fact_text: &str,
    valid_at: Option<&str>,
    event_time_source: EventTimeSource,
    source_episode: Option<i64>,
) -> Result<i64, DbError> {
    let relation = relation.to_string();
    let fact_text = fact_text.to_string();
    let valid_at = valid_at.map(str::to_string);
    db.call_flat(move |conn| {
        let tx = conn.transaction()?;
        let id = insert_fact_in(
            &tx,
            src,
            dst,
            &relation,
            &fact_text,
            valid_at.as_deref(),
            event_time_source,
            source_episode,
        )?;
        tx.commit()?;
        Ok(id)
    })
    .await
}

/// The two statements a fact INSERT needs, against a borrowed connection (t81).
/// Both the single-fact path and the whole-extraction transaction call THIS, so
/// they cannot drift into two different SQL shapes.
#[allow(clippy::too_many_arguments)]
fn insert_fact_in(
    conn: &rusqlite::Connection,
    src: i64,
    dst: i64,
    relation: &str,
    fact_text: &str,
    valid_at: Option<&str>,
    event_time_source: EventTimeSource,
    source_episode: Option<i64>,
) -> rusqlite::Result<i64> {
    let hash = fact_hash(fact_text);
    // `valid_at: None` forces `Recorded`: a NULL event time means the clock
    // stamped the value, so claiming `Extracted` would be a lie.
    let source = if valid_at.is_none() {
        EventTimeSource::Recorded
    } else {
        event_time_source
    }
    .as_str();
    let valid_at = valid_at
        .map(str::to_string)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE entity_edges
         SET invalid_at = ?5, expired_at = ?6
         WHERE src = ?1 AND dst = ?2 AND relation = ?3 AND invalid_at IS NULL",
        rusqlite::params![src, dst, relation, fact_text, valid_at, now],
    )?;
    conn.execute(
        "INSERT INTO entity_edges
             (src, dst, relation, fact_text, valid_at, created_at,
              source_episode, event_time_source, fact_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            src,
            dst,
            relation,
            fact_text,
            valid_at,
            now,
            source_episode,
            source,
            hash
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// The relation vocabulary (G7, R-1a/R-1c).
///
/// WHY A VOCABULARY AT ALL: the extraction is free to invent a relation name, so
/// the same claim came back under several names and each name became its own
/// edge. Measured on the frozen 60 current edges: 11 of them stated a fact that
/// another edge already stated (49/52/53/55 = "the harness drives model
/// v4-flash" under `drives_model` / `runs_model` / `powers` / `powered_by`;
/// 50/54/56 = the permission gatekeeper under three names; 14/39/62 and 15/40/63
/// = "ruagent integrates runtime X" under `integrates_runtime` / `supports` /
/// `orchestrates`). Relation-level precision was 0.7500 because of them.
///
/// A FAMILY is a set of names that make the same KIND of claim. Two edges with
/// the same endpoint pair and relations of the same family are the same claim,
/// so only the first is written (see `upsert_fact`).
pub fn relation_family(name: &str) -> Option<&'static str> {
    const MODEL: &[&str] = &["drives_model", "runs_model", "powers", "powered_by"];
    const RUNTIME: &[&str] = &["integrates_runtime", "orchestrates", "supports"];
    const GATEKEEPER: &[&str] = &[
        "uses_model_for_permission_gatekeeping",
        "uses_as_permission_gatekeeper",
        "uses_for_permission_gatekeeping",
        "hosts",
    ];
    const PROTOCOL: &[&str] = &["uses_protocol", "uses"];
    let n = name.trim().to_lowercase();
    let n = n.as_str();
    if MODEL.contains(&n) {
        Some("model_inference")
    } else if RUNTIME.contains(&n) {
        Some("runtime_integration")
    } else if GATEKEEPER.contains(&n) {
        Some("permission_gatekeeper")
    } else if PROTOCOL.contains(&n) {
        Some("protocol_use")
    } else {
        None
    }
}

/// What a relation name is: a claim about the two endpoints, or something else.
///
/// The second arm exists because some names are not relations at all: a
/// constraint (`must_never_drop`) or a plan (`planned_queryable_store`) says
/// something about ONE object, and storing it as an edge between two objects
/// asserts a relationship the transcript never stated. Those are refused at write
/// time (with the reason kept) instead of becoming graph facts. The remaining
/// object-specific names (`uses_as_production_database`,
/// `uses_for_vector_storage`, `uses_model_for_permission_gatekeeping`) ARE
/// relations -- they name a role, but they do state how the two endpoints relate
/// -- so they are accepted; the vocabulary does not pretend to be a closed
/// whitelist, and a new general name is written as-is (the alternative, refusing
/// everything unknown, silently drops real facts).
pub enum RelationVerdict {
    Relation { family: Option<&'static str> },
    NotARelation(&'static str),
}

pub fn relation_verdict(name: &str) -> RelationVerdict {
    let n = name.trim().to_lowercase();
    let not_a_relation: Option<&'static str> = if n.is_empty() {
        Some("empty relation name")
    } else if n.starts_with("must_")
        || n.starts_with("should_")
        || n.starts_with("never_")
        || n.starts_with("planned_")
        || n.ends_with("_is_planned")
    {
        Some("a constraint/plan about ONE object, not a relation between two")
    } else {
        None
    };
    match not_a_relation {
        Some(reason) => RelationVerdict::NotARelation(reason),
        None => RelationVerdict::Relation {
            family: relation_family(&n),
        },
    }
}

/// What happened to one extracted relation.
#[derive(Debug, Clone, PartialEq)]
pub enum FactOutcome {
    Written {
        id: i64,
    },
    /// This pair already carries this claim under another name of the same
    /// family; the edge that exists wins. Nothing written, nothing destroyed, so
    /// re-extracting the same session is idempotent.
    Duplicate {
        of: i64,
        of_relation: String,
    },
    /// The name asserts a constraint/plan, not a relation: nothing written.
    RefusedNotARelation(&'static str),
}

/// Write one extracted relation, DEDUPED at the write side (G7 / R-1a).
///
/// The rule, in order:
///   1. a name that is not a relation is refused (never stored as a fact);
///   2. if the same endpoint pair already carries a CURRENT edge of the same
///      FAMILY under a different name, this is the same claim: skip it (the first
///      spelling wins and the existing edge is left untouched);
///   3. a same-named current edge between the same endpoints is superseded and a
///      new row inserted (`add_fact_with_source` keeps the old row as history),
///      so a corrected fact still wins; an exact re-statement (same fact hash) is
///      skipped as idempotent.
///
/// WHY STEP 2 DOES NOT COMPARE FACTS: the duplicates are REWORDINGS ("DeepSeek
/// Harness 驱动 deepseek-v4-flash 模型运行。" vs "DeepSeek Harness 在该会话中运行
/// deepseek-v4-flash 模型。"), so a text or `fact_hash` comparison cannot see them
/// -- measured: the hash differs for all 11. The relation FAMILY is what makes
/// them the same claim.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_fact(
    db: &Db,
    src: i64,
    dst: i64,
    relation: &str,
    fact_text: &str,
    valid_at: Option<&str>,
    event_time_source: EventTimeSource,
    source_episode: Option<i64>,
) -> Result<FactOutcome, DbError> {
    let relation = relation.to_string();
    let fact_text = fact_text.to_string();
    let valid_at = valid_at.map(str::to_string);
    db.call_flat(move |conn| {
        let tx = conn.transaction()?;
        let outcome = upsert_fact_in(
            &tx,
            src,
            dst,
            &relation,
            &fact_text,
            valid_at.as_deref(),
            event_time_source,
            source_episode,
        )?;
        tx.commit()?;
        Ok(outcome)
    })
    .await
}

/// `upsert_fact`'s decision plus its two statements, against a borrowed
/// connection (t81) — so the write-side dedupe (G7) applies inside the
/// extraction transaction too, instead of being bypassed by it.
#[allow(clippy::too_many_arguments)]
fn upsert_fact_in(
    conn: &rusqlite::Connection,
    src: i64,
    dst: i64,
    relation: &str,
    fact_text: &str,
    valid_at: Option<&str>,
    event_time_source: EventTimeSource,
    source_episode: Option<i64>,
) -> rusqlite::Result<FactOutcome> {
    let family = match relation_verdict(relation) {
        RelationVerdict::NotARelation(reason) => {
            return Ok(FactOutcome::RefusedNotARelation(reason));
        }
        RelationVerdict::Relation { family } => family,
    };
    if let Some(fam) = family {
        let same_name = relation.trim().to_lowercase();
        let new_hash = fact_hash(fact_text);
        let existing: Vec<(i64, String, Option<String>)> = {
            let mut st = conn.prepare(
                "SELECT id, relation, fact_hash FROM entity_edges
                  WHERE invalid_at IS NULL
                    AND ((src = ?1 AND dst = ?2) OR (src = ?2 AND dst = ?1))",
            )?;
            st.query_map(rusqlite::params![src, dst], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        if let Some((id, _, hash)) = existing
            .iter()
            .find(|(_, rel, _)| rel.trim().to_lowercase() == same_name)
        {
            if hash.as_deref() == Some(new_hash.as_str()) {
                return Ok(FactOutcome::Duplicate {
                    of: *id,
                    of_relation: relation.to_string(),
                });
            }
        } else if let Some((id, rel, _)) = existing
            .iter()
            .find(|(_, rel, _)| relation_family(rel) == Some(fam))
        {
            return Ok(FactOutcome::Duplicate {
                of: *id,
                of_relation: rel.clone(),
            });
        }
    }
    let id = insert_fact_in(
        conn,
        src,
        dst,
        relation,
        fact_text,
        valid_at,
        event_time_source,
        source_episode,
    )?;
    Ok(FactOutcome::Written { id })
}

/// The resolution judge's normal form: lowercase, trimmed, with ONE trailing
/// parenthetical group removed. `Agent Client Protocol (ACP)` and
/// `Agent Client Protocol` share a base; that is the whole point.
pub fn base_name(name: &str) -> String {
    variants(name).first().cloned().unwrap_or_default()
}

/// The last of `ascii` / `wide` in `s`, with the character that matched, so a
/// caller can advance past the delimiter by ITS OWN byte length.
fn last_delimiter(s: &str, ascii: char, wide: char) -> Option<(usize, char)> {
    match (s.rfind(ascii), s.rfind(wide)) {
        (Some(i), Some(j)) => Some(if i > j { (i, ascii) } else { (j, wide) }),
        (Some(i), None) => Some((i, ascii)),
        (None, Some(j)) => Some((j, wide)),
        (None, None) => None,
    }
}

/// Every spelling a name carries: the base, plus the content of ONE trailing
/// parenthetical group.
///
/// WHY the parenthetical is kept rather than thrown away: the live graph links
/// objects THROUGH it. `DeepSeek Harness (dsh)` is the same object as `dsh`, and
/// that relation is only visible if the abbreviation inside the parentheses is
/// still available to the judge. Dropping it (which `base_name` alone does) is
/// what made the first version of this judge miss 4 of the 8 gold pairs.
///
/// # Why the inner slice is not `open + 1`
///
/// It used to be, and `（` is THREE bytes. A name carrying a full-width group —
/// `特质（trait）`, `栈帧（jvm stacks 虚拟机栈）`, ordinary text in a Chinese corpus —
/// panicked the caller inside this function: "start byte index 7 is not a char
/// boundary; it is inside '（' (bytes 6..9) of `特质（trait）`" (lib.rs:916). That
/// panic lands on the store's single-writer actor, because `apply_extraction`
/// resolves names inside the write closure, so ONE such name in ONE document took
/// the daemon's whole data plane down (`/api/v1/sessions` then answers 500 while
/// `/api/v1/health` still says ok). Measured on the real corpus: 113 of 936
/// knowledge documents carry at least one such name, and with
/// `knowledge_ingest_graph` enabled the boot sweep dies on the first one with no
/// HTTP request involved.
///
/// Now every index is either produced by `rfind` (a character boundary) or
/// advanced by the matched delimiter's own `len_utf8()` (1 for `(`/`)`, 3 for
/// `（`/`）`), so no byte-width assumption remains.
///
/// # Mixed delimiters: paired by POSITION
///
/// `矩阵（matrix)` and `Matrix (矩阵）` each yield one group, which is the SAME rule
/// `ruagent_extract::text::trailing_parenthetical` chose (crates/extract/src/
/// text.rs, the sibling fix of ruagent-close-the-gaps t8): the extractor hands
/// this function the pair it derived, so the two crates must not disagree about
/// what `（x)` means. Width is a keyboard/encoding artifact, not a semantic one;
/// refusing to pair across widths would silently drop a real alias.
pub fn variants(name: &str) -> Vec<String> {
    let n = name.trim().to_lowercase();
    let mut out = vec![n.clone()];
    if let Some((close, _)) = last_delimiter(&n, ')', '）')
        && let Some((open, opener)) = last_delimiter(&n[..close], '(', '（')
    {
        let outer = n[..open].trim().to_string();
        let inner = n[open + opener.len_utf8()..close].trim().to_string();
        if !outer.is_empty() {
            out.insert(0, outer);
        }
        if !inner.is_empty() {
            out.push(inner);
        }
    }
    out.dedup();
    out
}

/// The initials of a multi-word name: `agent client protocol` -> `acp`.
/// One word (or a single Han run) has no acronym, and returns None.
pub fn acronym(name: &str) -> Option<String> {
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    let mut s = String::new();
    for w in words {
        let c = w.chars().next()?;
        if !c.is_alphanumeric() {
            return None;
        }
        s.push(c);
    }
    Some(s)
}

/// The mechanical resolution judge.
///
/// Criterion (named here so a reader can recompute it, and so the gold set in
/// `tests/gold/resolution.json` can disagree with it):
///   `variants(x)` = [`variants`]: the base plus the trailing parenthetical.
///   SAME    = any pair of variants is EQUAL, or the tokens of the shorter are a
///             subset of the longer's (both >= 3 chars), or one variant is the
///             ACRONYM of the other's multi-word form (`ACP` of `Agent Client
///             Protocol`, `dsh` of `DeepSeek Harness` when the parentheses say
///             so).
///   PENDING = the first token of the store's own tokenizer matches but no SAME
///             rule fires -- the `dsh` / `dsh-kanban` family, where a human must
///             decide.
///   DIFFERENT = everything else.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeVerdict {
    SameObject(i64),
    Pending(i64),
    Different,
}

/// Does variant `a` denote the same object as variant `b`?
fn same_variant(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let ta: Vec<&str> = a.split_whitespace().collect();
    let tb: Vec<&str> = b.split_whitespace().collect();
    let (short, long) = if ta.len() <= tb.len() {
        (&ta, &tb)
    } else {
        (&tb, &ta)
    };
    let (sname, lname) = if ta.len() <= tb.len() { (a, b) } else { (b, a) };
    if !short.is_empty()
        && sname.chars().count() >= 3
        && lname.chars().count() >= 3
        && short.iter().all(|t| long.contains(t))
    {
        return true;
    }
    // Acronym <-> expansion, in either direction.
    if acronym(lname).as_deref() == Some(sname) || acronym(sname).as_deref() == Some(lname) {
        return true;
    }
    false
}

pub fn judge_against(existing: &[(i64, String)], name: &str) -> MergeVerdict {
    let target_variants = variants(name);
    let target_terms = ruagent_store::fts::terms(name);
    let target_first = target_terms.first().cloned();
    let mut pending: Option<i64> = None;
    for (id, other) in existing {
        let other_variants = variants(other);
        for a in &target_variants {
            for b in &other_variants {
                if same_variant(a, b) {
                    return MergeVerdict::SameObject(*id);
                }
            }
        }
        if pending.is_none()
            && let Some(tf) = &target_first
        {
            // The store's tokenizer, so "dsh-kanban" and "dsh-graph-view" both
            // lead with "dsh" -- the family a human has to rule on.
            let of = ruagent_store::fts::terms(other);
            if of.first() == Some(tf) {
                pending = Some(*id);
            }
        }
    }
    match pending {
        Some(id) => MergeVerdict::Pending(id),
        None => MergeVerdict::Different,
    }
}

/// What an alias-aware upsert did.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveOutcome {
    /// The name/alias resolved to an EXISTING entity; aliases were attached to it.
    Merged { id: i64, aliases_added: u32 },
    /// A new entity was created (nothing matched).
    Created { id: i64 },
    /// A new entity was created AND the pair is in the review queue: the judge
    /// would not merge on its own, and merging on a guess is worse than asking.
    PendingReview { id: i64, other: i64 },
}

/// Record an alias so a later query for it resolves to `entity_id`.
pub async fn add_alias(
    db: &Db,
    entity_id: i64,
    alias: &str,
    source: &str,
) -> Result<bool, DbError> {
    let alias = alias.to_string();
    let source = source.to_string();
    db.call_flat(move |conn| {
        let tx = conn.transaction()?;
        let n = add_alias_in(&tx, entity_id, &alias, &source)?;
        tx.commit()?;
        Ok(n)
    })
    .await
}

/// `add_alias`'s statement against a borrowed connection (t81) — so a set of
/// aliases can be written in the SAME transaction as the entity it belongs to.
fn add_alias_in(
    conn: &rusqlite::Connection,
    entity_id: i64,
    alias: &str,
    source: &str,
) -> rusqlite::Result<bool> {
    let alias = alias.trim().to_string();
    if alias.is_empty() {
        return Ok(false);
    }
    let norm_alias = norm(&alias);
    let now = Utc::now().to_rfc3339();
    let n = conn.execute(
        "INSERT INTO entity_aliases (entity_id, alias, norm_alias, source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(norm_alias) DO NOTHING",
        rusqlite::params![entity_id, alias, norm_alias, source, now],
    )?;
    Ok(n > 0)
}

/// Find-or-create an entity THROUGH the named judge, attaching aliases.
///
/// The difference from `upsert_entity` (frozen): that one resolves on
/// `norm_name` equality only, so `Agent Client Protocol (ACP)` and
/// `Agent Client Protocol` became two rows with two separate edge sets (4 such
/// pairs existed live, 2026-09-27). This one consults the alias table and then
/// the judge, merges on a named criterion, and QUEUES what it will not decide.
pub async fn upsert_entity_with_aliases(
    db: &Db,
    name: &str,
    kind: Option<&str>,
    summary: Option<&str>,
    aliases: &[String],
    source: &str,
) -> Result<ResolveOutcome, DbError> {
    if name.trim().is_empty() {
        return Err(DbError::from(rusqlite::Error::QueryReturnedNoRows));
    }
    let name = name.to_string();
    let kind = kind.map(str::to_string);
    let summary = summary.map(str::to_string);
    let aliases = aliases.to_vec();
    let source = source.to_string();
    db.call_flat(move |conn| {
        let tx = conn.transaction()?;
        let (outcome, _added) = resolve_entity_in(
            &tx,
            &name,
            kind.as_deref(),
            summary.as_deref(),
            &aliases,
            &source,
        )?;
        tx.commit()?;
        Ok(outcome)
    })
    .await
}

/// `upsert_entity_with_aliases`' decision and writes, against a borrowed
/// connection (t81). The entity row, its aliases and the review-queue row are
/// now ONE transaction: before, they were five separate round trips through the
/// actor, so a failure in the middle left an entity whose alias set was only
/// partly written. Returns the outcome and how many alias rows were created.
fn resolve_entity_in(
    conn: &rusqlite::Connection,
    name: &str,
    kind: Option<&str>,
    summary: Option<&str>,
    aliases: &[String],
    source: &str,
) -> rusqlite::Result<(ResolveOutcome, u32)> {
    let name_trimmed = name.trim().to_string();
    if name_trimmed.is_empty() {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    // 1. Exact norm_name, or 2. the alias table.
    let mut probes = vec![norm(&name_trimmed)];
    for a in aliases {
        let na = norm(a);
        if !na.is_empty() {
            probes.push(na);
        }
    }
    probes.sort();
    probes.dedup();
    let mut hit: Option<i64> = None;
    for p in &probes {
        if let Ok(id) = conn.query_row("SELECT id FROM entities WHERE norm_name = ?1", [p], |r| {
            r.get::<_, i64>(0)
        }) {
            hit = Some(id);
            break;
        }
        if let Ok(id) = conn.query_row(
            "SELECT entity_id FROM entity_aliases WHERE norm_alias = ?1",
            [p],
            |r| r.get::<_, i64>(0),
        ) {
            hit = Some(id);
            break;
        }
    }

    // 3. The judge, against every existing entity.
    let existing: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, name FROM entities ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let verdict = match hit {
        Some(id) => MergeVerdict::SameObject(id),
        None => judge_against(&existing, &name_trimmed),
    };

    match verdict {
        MergeVerdict::SameObject(id) => {
            let mut added = 0;
            for a in aliases {
                if add_alias_in(conn, id, a, source)? {
                    added += 1;
                }
            }
            let existing_name = existing
                .iter()
                .find(|(e, _)| *e == id)
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            if norm(&existing_name) != norm(&name_trimmed)
                && add_alias_in(conn, id, &name_trimmed, source)?
            {
                added += 1;
            }
            // Refresh kind/summary without asserting anything new about identity.
            conn.execute(
                "UPDATE entities SET kind = COALESCE(?2, kind),
                                     summary = COALESCE(?3, summary),
                                     updated_at = ?4
                 WHERE id = ?1",
                rusqlite::params![id, kind, summary, Utc::now().to_rfc3339()],
            )?;
            Ok((
                ResolveOutcome::Merged {
                    id,
                    aliases_added: added,
                },
                added,
            ))
        }
        MergeVerdict::Pending(other) => {
            let (id, added) =
                create_and_alias_in(conn, &name_trimmed, kind, summary, aliases, source)?;
            queue_pending_in(conn, id, other, "first-token match, judge did not merge")?;
            Ok((ResolveOutcome::PendingReview { id, other }, added))
        }
        MergeVerdict::Different => {
            let (id, added) =
                create_and_alias_in(conn, &name_trimmed, kind, summary, aliases, source)?;
            Ok((ResolveOutcome::Created { id }, added))
        }
    }
}

fn create_and_alias_in(
    conn: &rusqlite::Connection,
    name: &str,
    kind: Option<&str>,
    summary: Option<&str>,
    aliases: &[String],
    source: &str,
) -> rusqlite::Result<(i64, u32)> {
    let id = upsert_entity_in(conn, name, kind, summary)?;
    let mut added = 0;
    for a in aliases {
        if add_alias_in(conn, id, a, source)? {
            added += 1;
        }
    }
    Ok((id, added))
}

/// Record that a pair needs a human/LLM decision. Kept (not dropped) so "we did
/// not merge these" is a recorded decision rather than a silent one.
pub async fn queue_pending(db: &Db, a: i64, b: i64, reason: &str) -> Result<(), DbError> {
    let reason = reason.to_string();
    db.call_flat(move |conn| {
        let tx = conn.transaction()?;
        queue_pending_in(&tx, a, b, &reason)?;
        tx.commit()?;
        Ok(())
    })
    .await
}

/// `queue_pending`'s statement against a borrowed connection (t81).
fn queue_pending_in(
    conn: &rusqlite::Connection,
    a: i64,
    b: i64,
    reason: &str,
) -> rusqlite::Result<()> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO resolution_pending (entity_a, entity_b, reason, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(entity_a, entity_b) DO UPDATE SET reason = excluded.reason",
        rusqlite::params![lo, hi, reason, now],
    )?;
    Ok(())
}

/// The undecided queue, for a report or a review UI.
pub async fn pending_pairs(db: &Db) -> Result<Vec<(i64, i64, String)>, DbError> {
    let out = db
        .call_flat(|conn| -> Result<Vec<(i64, i64, String)>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT entity_a, entity_b, COALESCE(reason, '') FROM resolution_pending
                 ORDER BY entity_a, entity_b",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<Result<Vec<_>, _>>()
        })
        .await?;
    Ok(out)
}

/// The pairs the judge says are the SAME object but that still exist as two
/// rows -- the G6 metric, computable without reading any test fixture.
pub async fn redundant_pairs(db: &Db) -> Result<Vec<(i64, i64, String)>, DbError> {
    let entities: Vec<(i64, String)> = {
        let db2 = db.clone();
        db2.call_flat(|conn| -> Result<Vec<(i64, String)>, rusqlite::Error> {
            let mut stmt = conn.prepare("SELECT id, name FROM entities ORDER BY id")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<Vec<_>, _>>()
        })
        .await?
    };
    let mut out = Vec::new();
    for (i, (id_a, name_a)) in entities.iter().enumerate() {
        for (id_b, name_b) in entities.iter().skip(i + 1) {
            if let MergeVerdict::SameObject(_) = judge_against(&[(*id_b, name_b.clone())], name_a) {
                out.push((*id_a, *id_b, format!("{name_a} == {name_b}")));
            }
        }
    }
    Ok(out)
}

/// Merge `absorbed` into `keeper`: every incident edge follows, the absorbed
/// name becomes an alias of the keeper, and the absorbed row is deleted.
///
/// WHY this exists at all: the resolution judge can only report the 4 redundant
/// pairs (G6); "冗余 0" is not reachable by judging alone. Nothing calls this
/// automatically -- merging is a decision, so the caller is a human/agent path.
///
/// ATOMIC (t81): the EIGHT statements below run in ONE transaction. Before this
/// they ran in autocommit, and the old comment claimed the FK ordering made a
/// half-apply impossible -- measured with an injected failure at the last
/// statement, the edges and the alias were already moved while the absorbed row
/// survived. Ordering prevents an FK ERROR; only a transaction prevents a
/// HALF-MERGE.
pub async fn merge_entities(db: &Db, keeper: i64, absorbed: i64) -> Result<u32, DbError> {
    if keeper == absorbed {
        return Ok(0);
    }
    let now = Utc::now().to_rfc3339();
    let moved = db
        .call_flat(move |conn| -> Result<u32, rusqlite::Error> {
            let tx = conn.transaction()?;
            let name: String =
                tx.query_row("SELECT name FROM entities WHERE id = ?1", [absorbed], |r| {
                    r.get(0)
                })?;
            let mut moved = 0u32;
            moved += tx.execute(
                "UPDATE entity_edges SET src = ?1 WHERE src = ?2",
                rusqlite::params![keeper, absorbed],
            )? as u32;
            moved += tx.execute(
                "UPDATE entity_edges SET dst = ?1 WHERE dst = ?2",
                rusqlite::params![keeper, absorbed],
            )? as u32;
            tx.execute(
                "INSERT INTO entity_aliases (entity_id, alias, norm_alias, source, created_at)
                 VALUES (?1, ?2, ?3, 'merge', ?4)
                 ON CONFLICT(norm_alias) DO NOTHING",
                rusqlite::params![keeper, name, crate::norm(&name), now],
            )?;
            // Aliases follow their entity. With the whole group in one
            // transaction this is no longer load-bearing for atomicity -- it is
            // here so the intermediate states inside the transaction are legal
            // (foreign_keys=ON).
            tx.execute(
                "UPDATE entity_aliases SET entity_id = ?1 WHERE entity_id = ?2",
                rusqlite::params![keeper, absorbed],
            )?;
            tx.execute(
                "DELETE FROM community_entities WHERE entity_id = ?1",
                [absorbed],
            )?;
            tx.execute(
                "DELETE FROM resolution_pending WHERE entity_a = ?1 OR entity_b = ?1
                   OR entity_a = ?2 OR entity_b = ?2",
                rusqlite::params![keeper, absorbed],
            )?;
            tx.execute("DELETE FROM entities WHERE id = ?1", [absorbed])?;
            tx.commit()?;
            Ok(moved)
        })
        .await?;
    Ok(moved)
}

/// One entity as the extraction write path sees it (t81).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractEntity {
    pub name: String,
    pub kind: Option<String>,
    pub summary: Option<String>,
    pub aliases: Vec<String>,
}

/// One relation as the extraction write path sees it (t81). `src`/`dst` are
/// entity NAMES, resolved against the entities written in the same call.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractFact {
    pub src: String,
    pub dst: String,
    pub relation: String,
    pub fact_text: String,
    /// `None` = the extraction did not state an event time (forces `Recorded`).
    pub valid_at: Option<String>,
    pub event_time_source: EventTimeSource,
}

/// What one atomic extraction write did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractionWrite {
    /// Distinct entity names resolved (created, merged or queued).
    pub entities: u32,
    /// Alias rows created across all entities.
    pub aliases: u32,
    /// Relations written.
    pub relations: u32,
    /// Relations the write-side dedupe (G7) recognised as already stated.
    pub duplicates: u32,
    /// Relations refused because the name does not state a relation.
    pub refused: u32,
    /// Entities queued for review instead of merged.
    pub pending: u32,
}

/// Write a WHOLE extraction in ONE transaction (t81, audit #1).
///
/// WHY THIS ENTRY POINT EXISTS: distillation writes N entities, their aliases
/// and M relations. Each of those used to be its own closure through the
/// single-writer actor, i.e. its own autocommit -- so a failure at relation 7
/// left the first 6 entities and their aliases COMMITTED, and the audit
/// measured exactly that: a failed attempt whose episode was marked
/// `run_turn_failed` still added `entities 2 … aliases 1` to the graph. The
/// invariant "a failed attempt changes nothing" cannot be expressed with N
/// separate transactions, so the aggregation lives here: one closure, one
/// transaction, all-or-nothing.
///
/// The per-item rules are NOT reimplemented: this drives the same
/// `resolve_entity_in` / `upsert_fact_in` the single-item entry points drive, so
/// the write-side dedupe (G7) and the alias/judge resolution apply identically.
pub async fn apply_extraction(
    db: &Db,
    entities: &[ExtractEntity],
    facts: &[ExtractFact],
    source: &str,
    source_episode: Option<i64>,
) -> Result<ExtractionWrite, DbError> {
    let entities = entities.to_vec();
    let facts = facts.to_vec();
    let source = source.to_string();
    db.call_flat(move |conn| {
        let tx = conn.transaction()?;
        let mut report = ExtractionWrite::default();
        let mut ids: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for e in &entities {
            let name = e.name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            let (outcome, added) = resolve_entity_in(
                &tx,
                &name,
                e.kind.as_deref(),
                e.summary.as_deref(),
                &e.aliases,
                &source,
            )?;
            match outcome {
                ResolveOutcome::Merged { id, .. } | ResolveOutcome::Created { id } => {
                    ids.insert(name, id);
                }
                ResolveOutcome::PendingReview { id, .. } => {
                    report.pending += 1;
                    ids.insert(name, id);
                }
            }
            report.aliases += added;
        }
        report.entities = ids.len() as u32;
        for f in &facts {
            let (Some(src), Some(dst)) = (ids.get(f.src.trim()), ids.get(f.dst.trim())) else {
                continue; // relation to an unlisted entity — skip
            };
            match upsert_fact_in(
                &tx,
                *src,
                *dst,
                f.relation.trim(),
                f.fact_text.trim(),
                f.valid_at.as_deref(),
                f.event_time_source,
                source_episode,
            )? {
                FactOutcome::Written { .. } => report.relations += 1,
                FactOutcome::Duplicate { .. } => report.duplicates += 1,
                FactOutcome::RefusedNotARelation(_) => report.refused += 1,
            }
        }
        tx.commit()?;
        Ok(report)
    })
    .await
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

    /// t81 / audit #6: `merge_entities` runs EIGHT statements. Injecting a
    /// failure at the LAST one used to leave the edges retargeted and the alias
    /// moved while the absorbed row survived -- the "half-apply" the old comment
    /// said the FK ordering prevented.
    #[tokio::test]
    async fn a_failed_merge_rolls_back_the_whole_group() {
        let db = Db::open_in_memory().unwrap();
        let keeper = upsert_entity(&db, "ruagent", Some("project"), None)
            .await
            .unwrap();
        let absorbed = upsert_entity(&db, "ruagent (old)", Some("project"), None)
            .await
            .unwrap();
        add_fact(
            &db,
            absorbed,
            keeper,
            "uses_model",
            "old ruagent uses it",
            None,
            None,
        )
        .await
        .unwrap();
        add_alias(&db, absorbed, "legacy-ruagent", "test")
            .await
            .unwrap();

        let before = merge_snapshot(&db).await;
        assert_eq!(before.0, 2, "two entities before the merge");
        assert_eq!(before.2, 1, "one alias before the merge");

        // Poison the LAST statement of the group (the entity delete).
        let poison = format!(
            "CREATE TRIGGER t81_boom BEFORE DELETE ON entities
             WHEN OLD.id = {absorbed}
             BEGIN SELECT RAISE(ABORT, 't81 injected failure'); END;"
        );
        db.call(move |conn| conn.execute_batch(&poison))
            .await
            .unwrap()
            .unwrap();

        let err = merge_entities(&db, keeper, absorbed)
            .await
            .expect_err("the poisoned merge must fail");
        let after = merge_snapshot(&db).await;
        println!(
            "READING t81 #6: injected failure at the last statement -> {err} | \
             entities {}/{} | edges {}->{} | aliases {}/{} (before/after)",
            before.0, after.0, before.1, after.1, before.2, after.2
        );
        assert_eq!(
            after, before,
            "a failed merge must change NOTHING: no retargeted edge, no moved alias, no deleted row"
        );

        // Negative control: with the injection gone the same merge lands whole.
        db.call(|conn| conn.execute_batch("DROP TRIGGER t81_boom"))
            .await
            .unwrap()
            .unwrap();
        let moved = merge_entities(&db, keeper, absorbed).await.unwrap();
        let done = merge_snapshot(&db).await;
        println!(
            "READING t81 #6 (control): merge succeeded -> moved {moved} edge(s) | \
             entities {} | edges {} | aliases {} | keeper incident edges {}",
            done.0,
            done.1,
            done.2,
            current_facts(&db, keeper).await.unwrap().len()
        );
        assert_eq!(done.0, 1, "the absorbed row is gone");
        assert_eq!(
            done.2, 2,
            "its name and its own alias are now aliases of the keeper"
        );
        assert_eq!(
            current_facts(&db, keeper).await.unwrap().len(),
            1,
            "the edge followed the keeper"
        );
        assert!(
            pending_pairs(&db).await.unwrap().is_empty(),
            "the merge emptied the review queue of this pair"
        );
    }

    /// (entities, current edges, aliases) — the three tables a merge touches,
    /// plus the queue, read in one place so "0 changes" is one comparison.
    async fn merge_snapshot(db: &Db) -> (i64, i64, i64) {
        let e = db
            .call_flat(|conn| conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0)))
            .await
            .unwrap();
        let x = db
            .call_flat(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM entity_edges WHERE invalid_at IS NULL",
                    [],
                    |r| r.get(0),
                )
            })
            .await
            .unwrap();
        let a = db
            .call_flat(|conn| {
                conn.query_row("SELECT COUNT(*) FROM entity_aliases", [], |r| r.get(0))
            })
            .await
            .unwrap();
        (e, x, a)
    }

    /// t81 / audit #1: one extraction write (entities + aliases + relations) is
    /// all-or-nothing. Injecting a failure on the edge INSERT used to leave the
    /// entities and their aliases COMMITTED -- measured in the audit as
    /// `entities 2 … aliases 1` next to a `run_turn_failed` episode.
    #[tokio::test]
    async fn a_failed_extraction_write_leaves_no_partial_graph() {
        let db = Db::open_in_memory().unwrap();
        let entities = vec![
            ExtractEntity {
                name: "ruagent".into(),
                kind: Some("project".into()),
                summary: None,
                aliases: vec!["ruagent-rs".into()],
            },
            ExtractEntity {
                name: "麒麟 V10".into(),
                kind: None,
                summary: None,
                aliases: vec!["银河麒麟".into()],
            },
        ];
        let facts = vec![ExtractFact {
            src: "ruagent".into(),
            dst: "麒麟 V10".into(),
            relation: "runs_on".into(),
            fact_text: "ruagent runs on 麒麟".into(),
            valid_at: None,
            event_time_source: EventTimeSource::Recorded,
        }];

        let before = merge_snapshot(&db).await;
        assert_eq!(before, (0, 0, 0), "an empty graph to start from");

        db.call(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER t81_boom BEFORE INSERT ON entity_edges
                 BEGIN SELECT RAISE(ABORT, 't81 injected failure'); END;",
            )
        })
        .await
        .unwrap()
        .unwrap();

        let err = apply_extraction(&db, &entities, &facts, "extraction", None)
            .await
            .expect_err("the poisoned extraction write must fail");
        let after = merge_snapshot(&db).await;
        println!(
            "READING t81 #1: injected failure -> {err} | entities {}/{} | edges {}/{} | aliases {}/{} \
             (before/after: 0 residue expected)",
            before.0, after.0, before.1, after.1, before.2, after.2
        );
        assert_eq!(
            after, before,
            "a failed extraction write must leave the graph exactly as it was (0 residue)"
        );

        // Negative control: without the injection the same call lands whole.
        db.call(|conn| conn.execute_batch("DROP TRIGGER t81_boom"))
            .await
            .unwrap()
            .unwrap();
        let report = apply_extraction(&db, &entities, &facts, "extraction", None)
            .await
            .unwrap();
        let done = merge_snapshot(&db).await;
        println!(
            "READING t81 #1 (control): success -> entities {} aliases {} relations {} | tables: \
             entities {} edges {} aliases {}",
            report.entities, report.aliases, report.relations, done.0, done.1, done.2
        );
        assert_eq!(
            (report.entities, report.aliases, report.relations),
            (2, 2, 1)
        );
        assert_eq!(
            done,
            (2, 1, 2),
            "both entities, their aliases and the edge landed"
        );
    }

    /// t82 / audit #3: one instant, three spellings, one answer — ROW FOR ROW.
    ///
    /// The fixture holds the shapes the live table actually has: `Z`, a `+00:00`
    /// offset, a tenant offset (`+08:00`), 9-digit nanoseconds, and a date-only
    /// row (1 of 67 live edges). Two rows are chosen so that the OLD string
    /// comparison had to answer differently for the same moment.
    #[tokio::test]
    async fn facts_as_of_answers_the_same_for_one_instant_written_three_ways() {
        let db = Db::open_in_memory().unwrap();
        let a = upsert_entity(&db, "ruagent", None, None).await.unwrap();
        let b = upsert_entity(&db, "麒麟 V10", None, None).await.unwrap();
        let at = "2026-09-13T18:38:15Z";
        let rows = [
            // (relation, valid_at, invalid_at) — R = 2026-09-13T18:38:15Z
            ("starts_before", "2026-09-13T10:00:00+00:00", None),
            ("starts_exactly_at_R", at, None),
            (
                "starts_one_second_after_R",
                "2026-09-14T02:38:16+08:00",
                None,
            ),
            ("date_only_midnight", "2026-09-13", None),
            (
                "ends_exactly_at_R",
                "2026-09-13T10:00:00+00:00",
                Some("2026-09-14T02:38:15+08:00"),
            ),
            (
                "ends_after_R",
                "2026-09-13T10:00:00+00:00",
                Some("2026-09-13T20:00:00Z"),
            ),
        ];
        for (rel, v, iv) in rows {
            let id = add_fact_with_source(
                &db,
                a,
                b,
                rel,
                "fixture fact",
                Some(v),
                EventTimeSource::Extracted,
                None,
            )
            .await
            .unwrap();
            if let Some(iv) = iv {
                db.call(move |conn| {
                    conn.execute(
                        "UPDATE entity_edges SET invalid_at = ?1 WHERE id = ?2",
                        rusqlite::params![iv, id],
                    )
                })
                .await
                .unwrap()
                .unwrap();
            }
        }

        // The SAME instant, three spellings (R == 2026-09-14T02:38:15+08:00).
        let spellings = [
            "2026-09-13T18:38:15Z",
            "2026-09-13T18:38:15+00:00",
            "2026-09-14T02:38:15+08:00",
        ];
        let mut answers = Vec::new();
        for s in spellings {
            let got: Vec<String> = facts_as_of(&db, a, s)
                .await
                .unwrap()
                .into_iter()
                .map(|e| format!("{}#{}", e.id, e.relation))
                .collect();
            answers.push((s, got));
        }
        println!(
            "READING t82: one instant written three ways -> {}",
            answers
                .iter()
                .map(|(s, v)| format!("{s} = {} edges {v:?}", v.len()))
                .collect::<Vec<_>>()
                .join(" | ")
        );
        assert_eq!(
            answers[0].1, answers[1].1,
            "Z and +00:00 must be row-for-row identical"
        );
        assert_eq!(
            answers[1].1, answers[2].1,
            "+00:00 and +08:00 must be row-for-row identical"
        );
        assert_eq!(
            answers[0].1.len(),
            4,
            "in force at R: starts_before, starts_exactly_at_R, date_only_midnight, ends_after_R \
             (the one that starts after R and the one that ends exactly at R are out): {:?}",
            answers[0].1
        );
        // Instant order, not text order: an instant built from a +08:00 wall clock
        // must NOT land "later" than a Z one that is the same moment.
        assert_eq!(
            answers[0].1,
            vec![
                "2#starts_exactly_at_R".to_string(),
                "1#starts_before".to_string(),
                "6#ends_after_R".to_string(),
                "4#date_only_midnight".to_string(),
            ],
            "descending by instant, ties by id"
        );

        // NEGATIVE CONTROL: a DIFFERENT instant must give a DIFFERENT answer --
        // this is what proves the comparison was not switched off, shortened to a
        // constant, or turned into "everything".
        let earlier: Vec<String> = facts_as_of(&db, a, "2026-09-13T05:00:00Z")
            .await
            .unwrap()
            .into_iter()
            .map(|e| format!("{}#{}", e.id, e.relation))
            .collect();
        println!(
            "READING t82 (control): a different instant -> {} edges {earlier:?} vs {} edges at R",
            earlier.len(),
            answers[0].1.len()
        );
        assert_ne!(
            earlier, answers[0].1,
            "a different instant must not answer the same"
        );
        assert_eq!(
            earlier,
            vec!["4#date_only_midnight".to_string()],
            "only the date-only (midnight) row has started by 05:00"
        );
    }

    /// t82: a text that is not an instant is REFUSED, not string-compared. The
    /// old code answered `AND valid_at <= 'not-a-time'`, i.e. an empty list that
    /// is indistinguishable from a legitimate "nothing was true then".
    #[tokio::test]
    async fn facts_as_of_refuses_a_text_that_is_not_an_instant() {
        let db = Db::open_in_memory().unwrap();
        let a = upsert_entity(&db, "ruagent", None, None).await.unwrap();
        for bad in ["not-a-time", "", "2026-13-45T99:99:99Z", "now"] {
            let err = facts_as_of(&db, a, bad).await.expect_err("must refuse");
            println!("READING t82 (refusal): {bad:?} -> {err}");
            assert!(
                err.to_string().contains("is not an instant"),
                "the refusal must say why: {err}"
            );
        }
    }
}
