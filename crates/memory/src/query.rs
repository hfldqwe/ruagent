//! Read path: current memories by store/namespace + FTS search over them.

use ruagent_store::Db;

use crate::{MemoryRow, MemoryStore};

pub use ruagent_store::DbError;

/// A row whose `store` is not one of the four stores is an ERROR, not an
/// observation.
///
/// This used to be `match store { "profile" => …, "procedure" => …, "lesson" =>
/// …, _ => Observation }`: a silent downgrade, and the same defect the HTTP list
/// endpoint had before `parse_store` (R-B D-4). The governed write path cannot
/// produce such a row today; the read path refuses to invent a value for one
/// anyway, because "cannot happen" is not a reason for a reader to lie.
fn unknown_store_error(name: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        1,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown memory store `{name}`"),
        )),
    )
}

fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRow> {
    let store_name: String = row.get("store")?;
    let store =
        MemoryStore::try_parse(&store_name).ok_or_else(|| unknown_store_error(&store_name))?;
    Ok(MemoryRow {
        id: row.get("id")?,
        store,
        namespace: row.get("namespace")?,
        content: row.get("content")?,
        confidence: row.get("confidence")?,
        supersedes: row.get("supersedes")?,
        superseded_at: row.get("superseded_at")?,
        deleted_at: row.get("deleted_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        source_episode: row.get("source_episode")?,
    })
}

/// Current (non-superseded) memories for one store in one namespace,
/// newest first.
pub async fn current_memories(
    db: &Db,
    store: MemoryStore,
    namespace: &str,
    limit: u32,
) -> Result<Vec<MemoryRow>, DbError> {
    let store = store.as_str();
    let namespace = namespace.to_string();
    db.call(move |conn| -> Result<Vec<MemoryRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, store, namespace, content, confidence, supersedes, superseded_at,
                    deleted_at, created_at, updated_at, source_episode
             FROM memories
             WHERE store = ?1 AND namespace = ?2
               AND superseded_at IS NULL AND deleted_at IS NULL
             ORDER BY updated_at DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![store, namespace, limit], row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// Turn a user-supplied string into a legal FTS5 query — the ONE place that
/// knows the rule (t73 / F3).
///
/// WHY IT LIVES HERE (t73): the rule was written in the daemon
/// (`memembed.rs` `keyword_pattern`) and only the recall path knew it, so the
/// public `search_fts` handed raw user text to `MATCH` and ordinary queries
/// failed: measured (t69 C9, read-only probe) **8 of 11** strings error —
/// `"` (unterminated string) · `AND` / `NOT` / `NEAR(` / `-` / `deploy OR`
/// (fts5 syntax error) · `*` (unknown special query) · `a"b` (unterminated
/// string) — while the SAME 11 strings sanitized by this rule are **11/11 ok**.
/// Two implementations, one of which knew the rule, is the defect; moving the
/// rule into the shared layer leaves exactly one.
///
/// THE RULE: quote the whole query as an FTS5 **phrase** and double any inner
/// `"` (FTS5's escape inside a phrase). A phrase cannot start an operator, so
/// every special-syntax input becomes a literal search for those characters.
///
/// IDEMPOTENT BY CONSTRUCTION, and that is load-bearing: the recall path
/// (`memembed.rs`) ALREADY wraps its query in a phrase before calling
/// `search_fts_scored`, and it is not in this task's scope to change. If this
/// function re-escaped an already-formed phrase, the keyword leg would silently
/// start searching for literal quote characters and return nothing. So a string
/// that is already exactly one well-formed phrase is returned unchanged, and
/// `fts_pattern(fts_pattern(x)) == fts_pattern(x)` (tested).
pub fn fts_pattern(query: &str) -> String {
    if is_single_phrase(query) {
        return query.to_string();
    }
    format!("\"{}\"", query.replace('"', "\"\""))
}

/// Is `s` already exactly one well-formed FTS5 phrase (outer quotes with every
/// inner `"` doubled)? See `fts_pattern` for why this matters.
fn is_single_phrase(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return false;
    }
    // The outer bytes are ASCII quotes, so this slice is on char boundaries.
    let inner = &bytes[1..bytes.len() - 1];
    let mut i = 0;
    while i < inner.len() {
        if inner[i] == b'"' {
            // Every `"` inside must be the first half of a `""` pair.
            if i + 1 >= inner.len() || inner[i + 1] != b'"' {
                return false;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    true
}

/// Full-text search over current memories, all namespaces (the hybrid
/// retrieval's keyword leg; vectors land in the knowledge crate).
///
/// The caller passes RAW text: this is a public entry point, so the
/// sanitization happens HERE and not at each call site (t73 / F3).
pub async fn search_fts(db: &Db, query: &str, limit: u32) -> Result<Vec<MemoryRow>, DbError> {
    let query = fts_pattern(query);
    db.call(move |conn| -> Result<Vec<MemoryRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT m.id, m.store, m.namespace, m.content, m.confidence, m.supersedes,
                    m.superseded_at, m.deleted_at, m.created_at, m.updated_at, m.source_episode
             FROM memories_fts f
             JOIN memories m ON m.id = f.rowid
             WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL AND m.deleted_at IS NULL
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![query, limit], row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// The keyword leg WITH its own score: SQLite FTS5 `bm25()`, where a more
/// negative value is a better match. `search_fts` stays as the score-less
/// form for callers that only want the rows.
///
/// WHY A SCORE AT ALL (t251): the recall merge used to append this leg's rows
/// with no score, so the only ranked leg was the semantic one and the merged
/// array was a concatenation, not a ranking. A leg that reports its own score
/// can be fused on one scale and inspected afterwards.
///
/// Takes RAW text for the same reason as `search_fts` (t73 / F3); a caller that
/// already built a phrase (the recall path) is unaffected because
/// `fts_pattern` is idempotent.
pub async fn search_fts_scored(
    db: &Db,
    query: &str,
    limit: u32,
) -> Result<Vec<(MemoryRow, f64)>, DbError> {
    let query = fts_pattern(query);
    db.call(
        move |conn| -> Result<Vec<(MemoryRow, f64)>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT m.id, m.store, m.namespace, m.content, m.confidence, m.supersedes,
                    m.superseded_at, m.deleted_at, m.created_at, m.updated_at, m.source_episode,
                    bm25(memories_fts)
             FROM memories_fts f
             JOIN memories m ON m.id = f.rowid
             WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL AND m.deleted_at IS NULL
             ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![query, limit], |r| {
                    Ok((row_to_memory(r)?, r.get::<_, f64>(10)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        },
    )
    .await?
    .map_err(DbError::from)
}

/// One memory by id (any state: superseded and soft-deleted included — this
/// is the row the audit and the restore path need to see).
pub async fn get_memory(db: &Db, id: i64) -> Result<Option<MemoryRow>, DbError> {
    db.call(move |conn| -> Result<Option<MemoryRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, store, namespace, content, confidence, supersedes, superseded_at,
                    deleted_at, created_at, updated_at, source_episode
             FROM memories WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_memory(row)?)),
            None => Ok(None),
        }
    })
    .await?
    .map_err(DbError::from)
}

/// The audit log, newest first (design SS6.3: every write decision).
#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryDiff {
    pub id: i64,
    pub ts: String,
    pub op: String,
    pub mem_store: Option<String>,
    pub namespace: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub reason: Option<String>,
}

pub async fn list_diffs(db: &Db, limit: u32) -> Result<Vec<MemoryDiff>, DbError> {
    db.call(move |conn| -> Result<Vec<MemoryDiff>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, ts, op, mem_store, namespace, before, after, reason
             FROM memory_diffs ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit], |row| {
                Ok(MemoryDiff {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    op: row.get(2)?,
                    mem_store: row.get(3)?,
                    namespace: row.get(4)?,
                    before: row.get(5)?,
                    after: row.get(6)?,
                    reason: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// The `ruagent://` root listing: per-store counts of current memories.
pub async fn store_counts(db: &Db) -> Result<Vec<(String, String, i64)>, DbError> {
    db.call(
        |conn| -> Result<Vec<(String, String, i64)>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT store, namespace, COUNT(*) FROM memories
             WHERE superseded_at IS NULL AND deleted_at IS NULL
             GROUP BY store, namespace ORDER BY store, namespace",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        },
    )
    .await?
    .map_err(DbError::from)
}

/// Every LIVE memory, optionally narrowed by store and/or namespace.
///
/// WHY THE `?1 IS NULL OR ...` SHAPE: the API has to express "all stores" and
/// "all namespaces" without building SQL from strings, and a NULL parameter is
/// the one form where the filter set and the statement cannot drift apart.
pub async fn all_memories(
    db: &Db,
    store: Option<MemoryStore>,
    namespace: Option<String>,
    limit: u32,
) -> Result<Vec<MemoryRow>, DbError> {
    let store = store.map(|s| s.as_str().to_string());
    db.call(move |conn| -> Result<Vec<MemoryRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, store, namespace, content, confidence, supersedes, superseded_at,
                    deleted_at, created_at, updated_at, source_episode
             FROM memories
             WHERE superseded_at IS NULL AND deleted_at IS NULL
               AND (?1 IS NULL OR store = ?1)
               AND (?2 IS NULL OR namespace = ?2)
             ORDER BY store, namespace, updated_at DESC, id DESC
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![store, namespace, limit], row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// How many live memories match the same filter `all_memories` takes,
/// IGNORING the limit — the number the caller gets back as `matched`.
pub async fn count_memories(
    db: &Db,
    store: Option<MemoryStore>,
    namespace: Option<String>,
) -> Result<i64, DbError> {
    let store = store.map(|s| s.as_str().to_string());
    db.call(move |conn| -> Result<i64, rusqlite::Error> {
        conn.query_row(
            "SELECT COUNT(*) FROM memories
             WHERE superseded_at IS NULL AND deleted_at IS NULL
               AND (?1 IS NULL OR store = ?1)
               AND (?2 IS NULL OR namespace = ?2)",
            rusqlite::params![store, namespace],
            |r| r.get(0),
        )
    })
    .await?
    .map_err(DbError::from)
}

/// Every LIVE row in one (store, namespace) scope, as `(id, content)`.
///
/// This is the candidate面 a merge decision is allowed to look at (R-B E.2):
/// before it, `distill::mergeable_target` read the same scope with its own SQL
/// and no bound at all. The bound belongs to the policy (`MergeConfig::top_k`),
/// so this function returns the scope and the decision bounds it.
pub async fn scope_rows(
    db: &Db,
    store: MemoryStore,
    namespace: &str,
) -> Result<Vec<(i64, String)>, DbError> {
    let store = store.as_str();
    let namespace = namespace.to_string();
    db.call(move |conn| -> Result<Vec<(i64, String)>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, content FROM memories
              WHERE store = ?1 AND namespace = ?2
                AND superseded_at IS NULL AND deleted_at IS NULL
              ORDER BY id",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![store, namespace], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// Memories derived from one session's episode (R-B C4).
///
/// The link is the one distillation writes: `memories.source_episode` →
/// `episodes.id`, and the episode carries the session key in `source_run`.
/// Before this, "what did this session produce" had no read path at all — the
/// panel's per-session view could only show the episode count.
pub async fn session_memories(db: &Db, session_key: &str) -> Result<Vec<MemoryRow>, DbError> {
    let key = session_key.to_string();
    db.call(move |conn| -> Result<Vec<MemoryRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT m.id, m.store, m.namespace, m.content, m.confidence, m.supersedes,
                    m.superseded_at, m.deleted_at, m.created_at, m.updated_at, m.source_episode
               FROM memories m JOIN episodes e ON e.id = m.source_episode
              WHERE e.source_run = ?1
                AND m.superseded_at IS NULL AND m.deleted_at IS NULL
              ORDER BY m.id",
        )?;
        let rows = stmt
            .query_map([key], row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::Namespace;
    use crate::write::{MemoryWrite, WriteOutcome, write_memory};

    #[tokio::test]
    async fn search_finds_current_only() {
        let db = Db::open_in_memory().unwrap();
        let w = |content: &str| MemoryWrite {
            store: MemoryStore::Observation,
            namespace: Namespace::parse("project:demo").unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        };
        let first = write_memory(&db, &w("the deploy script lives in scripts/deploy.sh"))
            .await
            .unwrap();
        let WriteOutcome::Inserted(old) = first else {
            panic!()
        };
        let mut updated = w("the deploy script lives in scripts/release.sh");
        updated.supersedes = Some(old);
        write_memory(&db, &updated).await.unwrap();
        write_memory(&db, &w("unrelated note about tea"))
            .await
            .unwrap();

        let hits = search_fts(&db, "deploy", 10).await.unwrap();
        assert_eq!(hits.len(), 1, "superseded memory must not match");
        assert!(hits[0].content.contains("release.sh"));

        let current = current_memories(&db, MemoryStore::Observation, "project:demo", 10)
            .await
            .unwrap();
        assert_eq!(current.len(), 2);

        let counts = store_counts(&db).await.unwrap();
        assert_eq!(
            counts,
            vec![("observation".into(), "project:demo".into(), 2)]
        );
    }

    /// C7 (mem-core half): a row naming an unknown store must make the read path
    /// FAIL, not silently become an observation.
    ///
    /// The row is inserted with raw SQL on purpose: the governed write path
    /// refuses such a store, so a row like this can only arrive from a hand-edit,
    /// an older build, or a future migration — which is exactly the case the old
    /// `_ => Observation` arm answered with a lie.
    #[tokio::test]
    async fn an_unknown_store_is_an_error_not_an_observation() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .call(|conn| -> Result<i64, rusqlite::Error> {
                conn.execute(
                    "INSERT INTO memories (store, namespace, content, content_hash, confidence,
                                           created_at, updated_at)
                     VALUES ('bogus','user','a row from nowhere','hash-bogus-store',0.9,
                             '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                    [],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING C7 inserted the bogus-store row id={id}");

        let listed = all_memories(&db, None, None, 50).await;
        println!("READING C7 all_memories on a bogus-store row: {listed:?}");
        assert!(
            listed.is_err(),
            "an unknown store must surface as an error, got {listed:?}"
        );
        let msg = format!("{}", listed.unwrap_err());
        assert!(
            msg.contains("unknown memory store `bogus`"),
            "the error must name the value, got: {msg}"
        );

        // And the named-store reads keep working on the same table.
        assert_eq!(
            all_memories(&db, Some(MemoryStore::Observation), None, 50)
                .await
                .unwrap()
                .len(),
            0
        );
    }

    /// C4 read half: which memories a session produced, through the episode link.
    #[tokio::test]
    async fn session_memories_follows_the_episode_link() {
        let db = Db::open_in_memory().unwrap();
        let ep = crate::episode::record_episode(
            &db,
            crate::episode::EpisodeKind::RunTurn,
            "a transcript",
            Some("ruagent:session-key"),
        )
        .await
        .unwrap();
        let w = |content: &str| MemoryWrite {
            store: MemoryStore::Observation,
            namespace: Namespace::parse("project:demo").unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: Some(ep),
            supersedes: None,
        };
        write_memory(&db, &w("session fact one")).await.unwrap();
        write_memory(&db, &w("session fact two")).await.unwrap();
        write_memory(
            &db,
            &MemoryWrite {
                source_episode: None,
                ..w("unrelated")
            },
        )
        .await
        .unwrap();

        let rows = session_memories(&db, "ruagent:session-key").await.unwrap();
        println!("READING C4 session_memories = {:?}", rows.len());
        assert_eq!(rows.len(), 2, "only the two rows that name the episode");
        assert!(
            session_memories(&db, "ruagent:nope")
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// t69's C9 eleven inputs, verbatim — the same eleven the audit measured
    /// against the OLD code (raw text into MATCH).
    const C9_INPUTS: [&str; 11] = [
        "deploy",
        "\"",
        "AND",
        "NEAR(",
        "*",
        "-",
        "deploy OR",
        "NOT",
        "a\"b",
        "café",
        "🦀",
    ];

    /// The pre-fix code path, emulated verbatim: hand the RAW string to MATCH.
    /// Used as the "before" half of the before/after comparison, so both halves
    /// are measured at the same carrier instead of one of them being quoted
    /// from the audit report.
    async fn raw_match(db: &Db, query: &str, limit: u32) -> Result<Vec<i64>, DbError> {
        let query = query.to_string();
        db.call(move |conn| -> Result<Vec<i64>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT m.id FROM memories_fts f JOIN memories m ON m.id = f.rowid
                 WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL AND m.deleted_at IS NULL
                 ORDER BY rank LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![query, limit], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
        .map_err(DbError::from)
    }

    /// t73 / F3: every one of the eleven strings must reach FTS as a phrase, so
    /// none of them can be a syntax error any more. The `before` column is
    /// reproduced in-test (raw MATCH) so the 8 errors are shown, not asserted
    /// from the audit's text.
    #[tokio::test]
    async fn no_user_text_reaches_match_unquoted() {
        let db = Db::open_in_memory().unwrap();
        let w = |content: &str| MemoryWrite {
            store: MemoryStore::Observation,
            namespace: Namespace::parse("project:demo").unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        };
        write_memory(&db, &w("the deploy script lives in scripts/deploy.sh"))
            .await
            .unwrap();

        let mut raw_errors = 0;
        let mut after_ok = 0;
        for q in C9_INPUTS {
            let before = raw_match(&db, q, 10).await;
            let after = search_fts(&db, q, 10).await;
            let before_label = match &before {
                Ok(rows) => format!("ok rows={}", rows.len()),
                Err(e) => {
                    raw_errors += 1;
                    format!("ERR {e}")
                }
            };
            let after_label = match &after {
                Ok(rows) => {
                    after_ok += 1;
                    format!("ok rows={}", rows.len())
                }
                Err(e) => format!("ERR {e}"),
            };
            println!(
                "READING t73 F3 search_fts({q:?}) -> before(raw)={before_label} | \
                 after(shared fts_pattern)={after_label}"
            );
            assert!(
                after.is_ok(),
                "a public FTS entry point must not fail on user text: {q:?} -> {after:?}"
            );
        }
        println!(
            "READING t73 F3 totals: before(raw) errors={raw_errors}/11 | after ok={after_ok}/11"
        );
        assert_eq!(raw_errors, 8, "the audit's 8/11 errors, reproduced in-test");
        assert_eq!(
            after_ok, 11,
            "11/11 after the rule moved into the shared layer"
        );
    }

    /// The rule is idempotent, which is what keeps the recall path
    /// (`memembed.rs`, already sanitizing, out of this task's scope) unchanged.
    #[test]
    fn the_shared_rule_is_idempotent() {
        for q in C9_INPUTS {
            let once = fts_pattern(q);
            let twice = fts_pattern(&once);
            println!("READING t73 idempotence {q:?} -> {once:?} -> {twice:?}");
            assert_eq!(
                once, twice,
                "re-sanitizing must not change the query: {q:?}"
            );
        }
        // and the recall path's own phrase survives verbatim
        assert_eq!(fts_pattern("\"deploy\""), "\"deploy\"");
        assert_eq!(fts_pattern("\"deploy script\""), "\"deploy script\"");
        // while raw text that merely LOOKS quoted is escaped
        assert_eq!(fts_pattern("a\"b"), "\"a\"\"b\"");
    }

    /// NEGATIVE CONTROL (the key one): normalization must not change what the
    /// legitimate queries return. For each input that was ALREADY legal raw, the
    /// row ids before and after must be identical — measured on one carrier,
    /// both paths in this test.
    #[tokio::test]
    async fn legitimate_queries_return_exactly_the_same_rows() {
        let db = Db::open_in_memory().unwrap();
        let w = |content: &str| MemoryWrite {
            store: MemoryStore::Observation,
            namespace: Namespace::parse("project:demo").unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        };
        let adjacent = write_memory(&db, &w("deploy script lives in scripts/release.sh"))
            .await
            .unwrap();
        let separate = write_memory(&db, &w("deploy notes, then later a shell script"))
            .await
            .unwrap();
        write_memory(&db, &w("release 部署 script")).await.unwrap();
        write_memory(&db, &w("café notes about tea")).await.unwrap();
        let ids = |o: WriteOutcome| match o {
            WriteOutcome::Inserted(id) => id,
            other => panic!("{other:?}"),
        };
        let adjacent = ids(adjacent);
        let separate = ids(separate);

        // (a) ordinary keyword, (b) CJK, (c) a phrase the user quoted legally,
        // (d) a phrase in bare text. All four are legal raw AND after.
        for q in ["deploy", "部署", "\"deploy script\"", "café"] {
            let before = raw_match(&db, q, 10).await.unwrap();
            let after: Vec<i64> = search_fts(&db, q, 10)
                .await
                .unwrap()
                .into_iter()
                .map(|r| r.id)
                .collect();
            let mut b = before.clone();
            let mut a = after.clone();
            b.sort_unstable();
            a.sort_unstable();
            println!("READING t73 negative control {q:?} -> before(raw)={b:?} after={a:?}");
            assert_eq!(
                b, a,
                "result set changed for a previously legal query: {q:?}"
            );
        }

        // (e) the ONE semantic difference, stated instead of hidden: a multi-term
        // query used to be an implicit AND of terms and is now a phrase. Measured:
        let before = raw_match(&db, "deploy script", 10).await.unwrap();
        let after: Vec<i64> = search_fts(&db, "deploy script", 10)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        println!(
            "READING t73 difference \"deploy script\" -> before(raw AND)={before:?} \
             after(phrase)={after:?} (adjacent={adjacent} separate={separate})"
        );
        assert!(before.contains(&adjacent) && before.contains(&separate));
        assert_eq!(after, vec![adjacent], "phrase = adjacent terms only");
    }
}
