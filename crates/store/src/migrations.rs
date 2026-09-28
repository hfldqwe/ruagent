//! Schema migrations: embedded SQL applied in order, tracked in
//! `schema_migrations`. Idempotent and safe to run on every boot.

use crate::sqlite::DbError;

/// Embedded migration scripts, in order. The index in the array is the
/// schema version.
pub const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_agents.sql"),
    include_str!("migrations/0003_m2_results.sql"),
    include_str!("migrations/0004_memory.sql"),
    include_str!("migrations/0005_graph.sql"),
    include_str!("migrations/0006_knowledge.sql"),
    include_str!("migrations/0007_sessions.sql"),
    include_str!("migrations/0008_memory_embeddings.sql"),
    include_str!("migrations/0009_knowledge_files.sql"),
    include_str!("migrations/0010_wiki.sql"),
    include_str!("migrations/0011_recall_log.sql"),
    include_str!("migrations/0012_task_selected_by.sql"),
    include_str!("migrations/0013_chats.sql"),
    include_str!("migrations/0014_agent_options.sql"),
    include_str!("migrations/0015_chats_cwd.sql"),
    include_str!("migrations/0016_session_archives.sql"),
    include_str!("migrations/0017_session_deletions.sql"),
    include_str!("migrations/0018_memory_lifecycle.sql"),
    include_str!("migrations/0019_recall_quality.sql"),
    include_str!("migrations/0020_memory_semantics.sql"),
    include_str!("migrations/0021_graph_evidence.sql"),
    include_str!("migrations/0022_wiki_gen2.sql"),
    include_str!("migrations/0023_recall_telemetry.sql"),
    include_str!("migrations/0024_distill_attempts.sql"),
    include_str!("migrations/0025_query_eval_gold_unique.sql"),
];

/// The highest version any database can be at: `MIGRATIONS.len()`.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Apply all pending migrations.
///
/// ONE TRANSACTION PER MIGRATION (t6). Before this, `execute_batch` ran each
/// statement of a migration in its own implicit transaction, so a migration that
/// failed halfway left a PARTIALLY APPLIED schema and no version row -- the next
/// boot would re-run it from the top against a schema that had already changed.
/// Migrations 0019-0022 are the first ones here that ALTER existing tables,
/// backfill rows and install triggers, so the window was real: a failure between
/// the `wiki_builds` backfill and its triggers would leave a database whose rows
/// had been rewritten and whose constraints were missing.
///
/// The SQLite `foreign_keys` pragma cannot be toggled inside a transaction, so a
/// migration that needs a table rebuild (the 12-step procedure) is limited to the
/// case where NOTHING references the table being replaced: then `DROP TABLE`
/// cannot abort on a dependent object and the rebuild needs no pragma at all.
/// 0022 is that limitation (a table with 41 child rows), which is why the wiki
/// invariant is enforced by triggers rather than a CHECK; **0024 is the rebuild
/// that IS expressible** (0 foreign keys, 0 views, 0 triggers reference
/// `distill_log` -- measured on the live database before it was written), and it
/// is the first migration here to replace a table outright.
pub fn apply(conn: &mut rusqlite::Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL
         );",
    )?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;

    for (i, script) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version <= current {
            continue;
        }
        apply_one(conn, version, script)?;
        tracing::info!(version, "applied schema migration");
    }
    Ok(())
}

/// Apply exactly one migration and record its version ATOMICALLY.
///
/// Split out of `apply` so the rollback property is testable without a
/// deliberately broken entry in `MIGRATIONS`.
fn apply_one(conn: &mut rusqlite::Connection, version: i64, script: &str) -> Result<(), DbError> {
    let tx = conn.transaction()?;
    tx.execute_batch(script)?;
    tx.execute(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
        rusqlite::params![version, chrono::Utc::now().to_rfc3339()],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        conn
    }

    /// The infallible `count_where` above unwraps, which a `Db::call` closure
    /// cannot use; this is the same query with the error passed through.
    fn count_ok(conn: &rusqlite::Connection, sql: &str) -> Result<i64, rusqlite::Error> {
        conn.query_row(sql, [], |r| r.get(0))
    }

    /// Column names of a table, in declaration order.
    fn columns(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn count_where(conn: &rusqlite::Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn migrations_apply_idempotently() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap(); // second run must be a no-op
        let v: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
        assert_eq!(v, SCHEMA_VERSION);
        // The M1 tables exist.
        for table in ["tasks", "task_edges", "runs"] {
            let n: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {table} missing");
        }
    }

    /// t6: a migration is all-or-nothing. The failure has to be INSIDE the
    /// migration, after a statement that already changed the schema -- that is
    /// the window the old runner left open.
    #[test]
    fn a_failing_migration_rolls_back_completely() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        let before = count_where(&conn, "SELECT COUNT(*) FROM schema_migrations");

        let err = apply_one(
            &mut conn,
            999,
            "CREATE TABLE t6_atomicity_probe (x INTEGER);\n\
             INSERT INTO no_such_table_6 VALUES (1);",
        );
        assert!(err.is_err(), "the bad migration must report failure");

        let probe = count_where(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE name='t6_atomicity_probe'",
        );
        assert_eq!(probe, 0, "the partial table must have been rolled back");
        assert_eq!(
            count_where(&conn, "SELECT COUNT(*) FROM schema_migrations"),
            before,
            "no version row may survive a failed migration"
        );
    }

    /// t6: the additive surface, read back from a database built ONLY from the
    /// migration files (no live history, no defaults to hide behind).
    #[test]
    fn new_objects_exist_on_a_fresh_database() {
        let conn = fresh();

        for table in [
            "query_eval_sets",
            "query_eval_gold",
            "query_eval_runs",
            "memory_sources",
            "entity_aliases",
            "resolution_pending",
            "communities",
            "community_entities",
            "wiki_pages",
            "wiki_citations",
            "wiki_graph_readings",
            "wiki_corrections",
        ] {
            assert_eq!(
                count_where(
                    &conn,
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
                    )
                ),
                1,
                "missing table {table}"
            );
        }
        assert_eq!(
            count_where(
                &conn,
                "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='wiki_builds_unfinished_plans'"
            ),
            1,
            "missing the plan-only reading view"
        );

        // The ALTERed columns, and the NULL-default rule: every one of them must
        // be NULLABLE and must have NO default, or `ALTER TABLE` would have
        // backfilled history with an assertion (0020's rule).
        //
        // The full column list of two tables is read back too, rather than only
        // probing for the new names: a migration that appended a column in the
        // wrong table, or that replaced one, has to fail here and not in a
        // consumer. (`chunks` order matters to a reader: grams is appended last.)
        assert_eq!(
            columns(&conn, "chunks"),
            vec![
                "id",
                "document_id",
                "idx",
                "content",
                "span_start",
                "span_end",
                "section_id",
                "grams"
            ],
        );
        assert_eq!(
            columns(&conn, "recall_log"),
            vec![
                "id",
                "ts",
                "query",
                "strategy",
                "top_n",
                "memories",
                "knowledge",
                "wiki",
                "entities",
                "top_memory_score",
                "top_knowledge_score",
                "source",
                "top_knowledge_relevance",
                "top_knowledge_relevance_kind",
                "knowledge_leg_window",
                "scoring_version",
                "graph_entities",
                "graph_paths",
                "score_kind",
                "fusion",
                "top_legs_json",
                "candidates_json",
                "selected_json",
                "rejected_json",
            ],
        );
        // t25: `distill_log` is one row per ATTEMPT, so the identity column comes
        // first and `session_key` is a plain, indexed column. The order is
        // asserted as well as the set: a reader that still assumes session_key is
        // the key has to notice.
        assert_eq!(
            columns(&conn, "distill_log"),
            vec![
                "id",
                "session_key",
                "distilled_at",
                "memories_written",
                "entities_written",
                "relations_written",
                "agent",
                "status",
                "failure_reason",
                "prompt_hash"
            ],
        );
        let pk: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(distill_log)").unwrap();
            stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .filter(|(_, is_pk)| *is_pk == 1)
                .map(|(name, _)| name)
                .collect()
        };
        assert_eq!(
            pk,
            vec!["id"],
            "the per-attempt key must be `id`; `session_key` was the defect"
        );
        assert_eq!(
            count_where(
                &conn,
                "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','view') \
                 AND name IN ('distill_recorded_outcomes')"
            ),
            1,
            "the reading rule must be queryable, not just documented"
        );
        let added: &[(&str, &str)] = &[
            ("recall_log", "top_knowledge_relevance"),
            ("recall_log", "top_knowledge_relevance_kind"),
            ("recall_log", "knowledge_leg_window"),
            ("recall_log", "scoring_version"),
            ("recall_log", "graph_entities"),
            ("recall_log", "graph_paths"),
            ("recall_log", "score_kind"),
            ("recall_log", "fusion"),
            ("recall_log", "top_legs_json"),
            ("recall_log", "candidates_json"),
            ("recall_log", "selected_json"),
            ("recall_log", "rejected_json"),
            ("chunks", "grams"),
            ("memories", "access_count"),
            ("memories", "last_used_at"),
            ("memories", "valid_from"),
            ("memories", "valid_to"),
            ("distill_log", "status"),
            ("distill_log", "failure_reason"),
            ("distill_log", "prompt_hash"),
            ("entity_edges", "event_time_source"),
            ("entity_edges", "fact_hash"),
            ("entities", "embedding"),
            ("entities", "embedder"),
        ];
        for (table, column) in added {
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let row = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(3)?,            // notnull
                        r.get::<_, Option<String>>(4)?, // dflt_value
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .into_iter()
                .find(|(name, _, _)| name == column);
            let (_, notnull, dflt) = row.unwrap_or_else(|| panic!("{table}.{column} missing"));
            assert_eq!(
                notnull, 0,
                "{table}.{column} must be NULLABLE: NULL is how a pre-existing row says \
                 'unknown', and a NOT NULL would have backfilled it"
            );
            assert!(
                dflt.is_none(),
                "{table}.{column} must have NO default, got {dflt:?}"
            );
        }
    }

    /// t25 / R-C DEP-3: one row per ATTEMPT.
    ///
    /// The live evidence for needing it: a window with 22 attempts and 21
    /// failures produced ONE row in the database, because `session_key` was the
    /// primary key and every attempt for a session hit the same key. "How often
    /// does distillation fail?" could only be answered from the daemon's log.
    ///
    /// The same test pins the READING RULE: the denominator of that rate is
    /// `distill_recorded_outcomes`, so an attempt whose outcome nobody recorded
    /// is not silently counted as a success.
    #[test]
    fn distill_log_keeps_one_row_per_attempt_and_legacy_rows_stay_unknown() {
        let conn = fresh();
        let session_rows = |conn: &rusqlite::Connection| -> i64 {
            count_where(
                conn,
                "SELECT COUNT(*) FROM distill_log WHERE session_key = 's1'",
            )
        };
        let ins =
            |at: &str, status: Option<&str>, reason: Option<&str>, hash: Option<&str>, mem: i64| {
                conn.execute(
                    "INSERT INTO distill_log
                     (session_key, distilled_at, memories_written, entities_written,
                      relations_written, agent, status, failure_reason, prompt_hash)
                 VALUES ('s1', ?1, ?2, 0, 0, 'agent', ?3, ?4, ?5)",
                    rusqlite::params![at, mem, status, reason, hash],
                )
                .unwrap()
            };

        // Attempt 1 succeeds, attempt 2 fails ON THE SAME SESSION. Before this
        // migration the second write replaced the first (INSERT OR REPLACE).
        ins("2026-01-01T00:00:00Z", Some("ok"), None, Some("ph1"), 3);
        ins(
            "2026-01-01T00:10:00Z",
            Some("failed"),
            Some("prompt longer than the context window"),
            Some("ph2"),
            0,
        );
        assert_eq!(
            session_rows(&conn),
            2,
            "a failed attempt must not replace the successful one"
        );
        // ... and the reverse direction: a later SUCCESS must not erase the
        // failure's record either. Both directions are stated because the old
        // single-row design lost whichever came first.
        ins("2026-01-01T00:20:00Z", Some("ok"), None, Some("ph1"), 2);
        assert_eq!(session_rows(&conn), 3);
        assert_eq!(
            count_where(
                &conn,
                "SELECT COUNT(*) FROM distill_log WHERE status='failed'"
            ),
            1,
            "the failure record must survive a later success"
        );

        // Two attempts in the SAME SECOND coexist: there is no time component in
        // the key any more, so nothing has to be unique but `id`.
        let same = "2026-01-01T00:30:00Z";
        ins(same, Some("empty"), None, Some("ph1"), 0);
        ins(same, Some("empty"), None, Some("ph1"), 0);
        assert_eq!(session_rows(&conn), 5);

        // The failure row carries what an operator needs to act on.
        let (reason, hash): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT failure_reason, prompt_hash FROM distill_log WHERE status='failed'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            reason.as_deref(),
            Some("prompt longer than the context window")
        );
        assert_eq!(
            hash.as_deref(),
            Some("ph2"),
            "the fingerprint says WHICH prompt failed"
        );

        // ids are unique and assigned in insertion order.
        let ids: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT id FROM distill_log WHERE session_key='s1' ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);

        // A row the writer did not annotate: the shape a pre-0024 row has. It is
        // NOT a success and NOT a failure -- it is an attempt whose outcome
        // nobody recorded (closure §6.4: NULL is not a value).
        conn.execute(
            "INSERT INTO distill_log (session_key, distilled_at, memories_written,
                                      entities_written, relations_written, agent)
             VALUES ('s1', '2026-01-01T00:40:00Z', 0, 0, 0, 'pre-0024-writer')",
            [],
        )
        .unwrap();
        assert_eq!(session_rows(&conn), 6);
        assert_eq!(
            count_where(
                &conn,
                "SELECT COUNT(*) FROM distill_log WHERE status IS NULL"
            ),
            1
        );
        assert_eq!(
            count_where(&conn, "SELECT COUNT(*) FROM distill_recorded_outcomes"),
            5,
            "the rate's denominator excludes attempts whose outcome was not recorded"
        );
        let by_status: Vec<(String, i64)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT status, COUNT(*) FROM distill_recorded_outcomes GROUP BY 1 ORDER BY 1",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(
            by_status,
            vec![
                ("empty".to_string(), 2),
                ("failed".to_string(), 1),
                ("ok".to_string(), 2)
            ],
            "the database can now answer the question the log used to own"
        );

        // A per-attempt row with no session cannot be attributed, so the column
        // is NOT NULL (the old `TEXT PRIMARY KEY` accepted NULLs -- a SQLite
        // quirk -- and would have let such a row in).
        assert!(
            conn.execute(
                "INSERT INTO distill_log (session_key, distilled_at) VALUES (NULL, 'x')",
                []
            )
            .is_err(),
            "session_key must be NOT NULL now that it is not a key"
        );

        // THE HAZARD, pinned: the pre-0024 writer's single statement must fail
        // LOUDLY here rather than silently collapsing attempts again. This is
        // what `crates/daemon/src/distill.rs:327-343` must stop using (graph's
        // I-C task); the exact error is registered in 0024's header.
        let old_writer = conn.prepare(
            "INSERT INTO distill_log (session_key, distilled_at) VALUES ('s1', 'x')
             ON CONFLICT(session_key) DO UPDATE SET distilled_at = excluded.distilled_at",
        );
        match old_writer {
            Ok(_) => panic!(
                "the pre-0024 upsert prepared successfully -- it would collapse attempts again"
            ),
            Err(e) => assert!(
                format!("{e}").contains("ON CONFLICT"),
                "unexpected error from the stale statement: {e}"
            ),
        }
    }

    /// t6: the CJK bigram index must follow `chunks.grams` through the ONE path
    /// that actually populates it -- the NULL -> value backfill update. The
    /// three-trigger draft would have run FTS5's 'delete' for a row that was
    /// never inserted on exactly this path.
    ///
    /// Every assertion goes through MATCH: `chunks_fts_cjk` is an
    /// external-content table, so a plain `SELECT COUNT(*)` reads `chunks` and
    /// would report 1 whatever the index holds.
    #[test]
    fn cjk_bigram_index_follows_a_null_to_value_backfill() {
        let conn = fresh();
        let matches = |pattern: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM chunks_fts_cjk WHERE chunks_fts_cjk MATCH ?1",
                [pattern],
                |r| r.get(0),
            )
            .unwrap()
        };
        conn.execute(
            "INSERT INTO documents (id, name, content_hash, chunk_count, created_at)
             VALUES (1, 'd', 'h', 1, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // The write path today: no grams yet.
        let text = "咖啡研磨度决定萃取速度";
        conn.execute(
            "INSERT INTO chunks (id, document_id, idx, content) VALUES (1, 1, 0, ?1)",
            [text],
        )
        .unwrap();
        assert_eq!(
            matches("\"研磨\""),
            0,
            "a NULL grams row must not be in the index"
        );

        // The backfill: exactly the statement `Db::backfill_chunk_grams` issues.
        conn.execute(
            "UPDATE chunks SET grams = ?1 WHERE id = 1",
            [crate::fts::han_bigrams(text)],
        )
        .unwrap();
        assert_eq!(matches("\"研磨\""), 1, "the backfill must index the row");
        assert_eq!(matches("\"咖啡\""), 1);
        assert_eq!(matches("\"啡研\""), 1);
        // The query form is the pair to the indexed form.
        assert_eq!(
            matches(&crate::fts::match_bigrams(&crate::fts::terms("研磨度"))),
            1,
            "the 2-character substring the LIKE stage exists for must be indexed"
        );

        // A real change of an already-indexed value: replaced, not duplicated --
        // a duplicate posting would make MATCH return the same rowid twice.
        conn.execute("UPDATE chunks SET grams = '咖啡 啡研' WHERE id = 1", [])
            .unwrap();
        assert_eq!(matches("\"研磨\""), 0, "the old value must be gone");
        assert_eq!(matches("\"咖啡\""), 1, "the new value must be there once");

        // Delete.
        conn.execute("DELETE FROM chunks WHERE id = 1", []).unwrap();
        assert_eq!(matches("\"咖啡\""), 0);
    }

    /// t6: the two halves of the bigram feature -- the text stored in
    /// `chunks.grams` and the query pattern -- are one pair, checked over the
    /// migration-built schema rather than in isolation.
    #[test]
    fn the_bigram_pair_round_trips_over_a_long_han_run() {
        let conn = fresh();
        conn.execute(
            "INSERT INTO documents (id, name, content_hash, chunk_count, created_at)
             VALUES (1, 'd', 'h', 1, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let text = "泡茶的水温很重要。反射的访问权限问题。";
        conn.execute(
            "INSERT INTO chunks (id, document_id, idx, content, grams) VALUES (1, 1, 0, ?1, ?2)",
            rusqlite::params![text, crate::fts::han_bigrams(text)],
        )
        .unwrap();
        let matches = |q: &str| -> i64 {
            let pattern = crate::fts::match_bigrams(&crate::fts::terms(q));
            conn.query_row(
                "SELECT COUNT(*) FROM chunks_fts_cjk WHERE chunks_fts_cjk MATCH ?1",
                [&pattern],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(matches("水温"), 1);
        assert_eq!(matches("访问权限"), 1);
        assert_eq!(matches("权限访问"), 0, "a phrase of bigrams is ordered");
        assert_eq!(matches("潜艇"), 0);
    }

    /// t6: the wiki_builds invariant, replayed against the EXACT statements the
    /// product issues (crates/daemon/src/wiki.rs), because a constraint that
    /// rejects a legitimate write is a product outage, not a safety net.
    #[test]
    fn wiki_builds_invariant_accepts_every_product_write_and_rejects_bad_pairs() {
        let conn = fresh();
        let ins = |status: &str, dry: i64| {
            conn.execute(
                "INSERT INTO wiki_builds (scope, status, dry_run, agent, pages_planned, \
                                          plan_json, started_at) \
                 VALUES ('all', ?1, ?2, 'agent', 0, NULL, '2026-01-01T00:00:00Z')",
                rusqlite::params![status, dry],
            )
        };

        // wiki.rs:1642 -- the dry-run insert, which is UNFINISHED at this instant.
        let dry_id = {
            assert!(
                ins("planned_only", 1).is_ok(),
                "the live dry-run insert (status='planned_only', dry_run=1) must still work"
            );
            conn.last_insert_rowid()
        };
        // wiki.rs:805 (confirm_plan) and wiki.rs:848 -- real builds say 'running'.
        let run_id = {
            assert!(ins("running", 0).is_ok());
            conn.last_insert_rowid()
        };

        // wiki.rs:1845 -- finish_dry_run stamps finished_at only.
        assert!(
            conn.execute(
                "UPDATE wiki_builds SET finished_at = '2026-01-01T00:00:01Z' WHERE id = ?1",
                rusqlite::params![dry_id],
            )
            .is_ok()
        );
        // wiki.rs:1716 -- finish_build.
        assert!(
            conn.execute(
                "UPDATE wiki_builds SET status = 'done', pages_written = 1, \
                        pages_failed = 0, finished_at = '2026-01-01T00:01:00Z' WHERE id = ?1",
                rusqlite::params![run_id],
            )
            .is_ok()
        );
        // wiki.rs:1762 -- fail_build on a real build.
        let run2 = {
            assert!(ins("running", 0).is_ok());
            conn.last_insert_rowid()
        };
        assert!(
            conn.execute(
                "UPDATE wiki_builds SET status = 'failed', error = 'boom', \
                        finished_at = '2026-01-01T00:02:00Z' WHERE id = ?1",
                rusqlite::params![run2],
            )
            .is_ok()
        );
        // wiki.rs:1750 -- update_build's generic field write.
        assert!(
            conn.execute(
                "UPDATE wiki_builds SET pages_written = 2 WHERE id = ?1",
                rusqlite::params![run_id],
            )
            .is_ok()
        );

        // The defect the constraint exists for: the plan-only vocabulary and the
        // dry_run flag must not be able to disagree.
        assert!(
            ins("done", 1).is_err(),
            "a dry_run=1 row with an execution status must be rejected"
        );
        assert!(
            ins("planned", 0).is_err(),
            "a dry_run=0 row with a plan-only status must be rejected"
        );
        assert!(
            conn.execute(
                "UPDATE wiki_builds SET status = 'done' WHERE id = ?1",
                rusqlite::params![dry_id],
            )
            .is_err(),
            "a conforming row must not be made to violate the pairing"
        );
        // t10 will write 'planned' for dry runs; that must already be accepted.
        let planned_id = {
            assert!(ins("planned", 1).is_ok());
            conn.last_insert_rowid()
        };

        // The reading that stands in for the unimplementable CHECK, and its
        // first assertion is the REASON the CHECK cannot exist: the row inserted
        // one statement ago is a plan-only row whose `finish_dry_run` has not run
        // yet (wiki.rs:1845 is a separate statement), so at this instant the
        // invariant "plan-only => finished_at IS NOT NULL" is LEGITIMATELY
        // FALSE. A CHECK would have rejected that INSERT and broken every
        // dry-run build.
        assert_eq!(
            count_where(&conn, "SELECT COUNT(*) FROM wiki_builds_unfinished_plans"),
            1,
            "a just-inserted plan-only row is unfinished until finish_dry_run runs"
        );
        // ... and the single statement the product issues next closes it.
        conn.execute(
            "UPDATE wiki_builds SET finished_at = '2026-01-01T00:00:02Z' WHERE id = ?1",
            rusqlite::params![planned_id],
        )
        .unwrap();
        assert_eq!(
            count_where(&conn, "SELECT COUNT(*) FROM wiki_builds_unfinished_plans"),
            0
        );
    }

    /// t6: the clause the acceptance asks about -- a NEW column on rows that
    /// already existed must stay NULL, and NULL must not read as a value.
    ///
    /// A fresh database cannot show this (there is no history), so the reading is
    /// taken on a COPY of the live database.
    ///
    /// t33 SHAPE (the fifth family of false green, closed on the store side): this
    /// test used to print `NOT MEASURED` and `return` without the env var, so the
    /// harness counted it as a PASS while it had measured nothing -- "N passed"
    /// then mixed measurements with non-attempts. Now it is `#[ignore]`d (the
    /// count field says `ignored`, which cannot be mistaken for a reading), running
    /// it explicitly without the env var FAILS at the `expect`, and the reading
    /// itself is asserted non-vacuous below (an empty copy is not a measurement).
    #[ignore = "measures the history-NULL reading: needs RUAGENT_T6_LIVE_COPY=<a migrated COPY of \
                the live db> (the test APPLIES migrations, i.e. it WRITES) and runs with \
                `-- --ignored`"]
    #[test]
    fn live_copy_keeps_history_nullable() {
        let path = std::env::var("RUAGENT_T6_LIVE_COPY").expect(
            "this instrument has no state in which it passes without measuring: set \
             RUAGENT_T6_LIVE_COPY=<a migrated COPY of the live db> (the test APPLIES migrations, \
             i.e. it WRITES -- never point it at ~/.ruagent) and run with `-- --ignored`",
        );
        let mut conn = rusqlite::Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        apply(&mut conn).unwrap();
        assert_eq!(
            count_where(&conn, "SELECT MAX(version) FROM schema_migrations"),
            SCHEMA_VERSION
        );
        // THE READING MUST BE ABOUT SOMETHING. A copy with no rows would satisfy
        // every count-based assertion below (0 unfinished, 0 non-NULL history)
        // while proving nothing at all -- the vacuous pass this family is about.
        let wiki_rows = count_where(&conn, "SELECT COUNT(*) FROM wiki_builds");
        let chunks = count_where(&conn, "SELECT COUNT(*) FROM chunks");
        assert!(
            wiki_rows > 0 || chunks > 0,
            "the copy holds neither wiki_builds nor chunks rows: there is no history to read, so \
             this run measures nothing and must not be reported as a pass"
        );
        println!("[t33] live copy object set: wiki_builds={wiki_rows} chunks={chunks}");

        // t36 FINDING (measured, not reasoned): T6 and T25 must point at SEPARATE
        // copies. The t25 instrument writes two ANNOTATED rows into `distill_log`
        // for its probe session, so running the two with ONE copy leaves this test
        // reading another test's writes -- and the history assertion further down
        // ("every historical row must be NULL") then fails for a reason that has
        // nothing to do with history. It took a shared copy to see that; the
        // failure it produces is confusing, so the trap now announces itself.
        let probe_rows = count_where(
            &conn,
            "SELECT COUNT(*) FROM distill_log WHERE session_key = 't25:schema-side-probe'",
        );
        assert_eq!(
            probe_rows, 0,
            "this copy carries the t25 instrument's probe rows: point \
             RUAGENT_T6_LIVE_COPY and RUAGENT_T25_LIVE_COPY at SEPARATE copies, otherwise this \
             test reads another instrument's writes"
        );

        // (a) the wiki invariant holds on real history after the backfill.
        let unfinished = count_where(&conn, "SELECT COUNT(*) FROM wiki_builds_unfinished_plans");
        let rows = count_where(&conn, "SELECT COUNT(*) FROM wiki_builds");
        println!("[t6] live copy: wiki_builds rows={rows} unfinished_plans={unfinished}");
        assert_eq!(
            unfinished, 0,
            "the 0022 backfill must leave no plan-only row without finished_at"
        );
        assert_eq!(
            count_where(
                &conn,
                "SELECT COUNT(*) FROM wiki_builds \
                  WHERE (status IN ('planned','planned_only')) <> (dry_run = 1)"
            ),
            0,
            "no historical row may still disagree with itself"
        );

        // (b) every added column is NULL on every pre-0020/21/22 row.
        //
        // These five are asserted as an invariant: nothing writes them yet, so a
        // non-NULL value here would mean the migration BACKFILLED history with a
        // value it cannot support (the defect the NULL rule exists to prevent).
        for (table, column) in [
            ("memories", "access_count"),
            ("memories", "last_used_at"),
            ("memories", "valid_from"),
            ("distill_log", "status"),
            ("entity_edges", "event_time_source"),
        ] {
            let total = count_where(&conn, &format!("SELECT COUNT(*) FROM {table}"));
            let nulls = count_where(
                &conn,
                &format!("SELECT COUNT(*) FROM {table} WHERE {column} IS NULL"),
            );
            println!("[t6] live copy: {table}.{column} NULL {nulls}/{total}");
            assert_eq!(
                nulls, total,
                "{table}.{column}: every historical row must be NULL (unknown), not a \
                 backfilled value"
            );
        }

        // `chunks.grams` is the one exception, and it is a READING rather than an
        // invariant: 0022's sibling is designed to fill it, so on a freshly
        // migrated copy it is all-NULL and after a backfill it is none-NULL.
        // Both are legitimate; a PARTIAL fill would mean a batch was torn, which
        // each batch's own transaction makes impossible -- that is what is
        // asserted here, and the state itself is printed so neither reading can
        // be reported as the other.
        let total = count_where(&conn, "SELECT COUNT(*) FROM chunks");
        let nulls = count_where(&conn, "SELECT COUNT(*) FROM chunks WHERE grams IS NULL");
        println!(
            "[t6] live copy: chunks.grams NULL {nulls}/{total} (all-NULL = freshly migrated, none-NULL = backfilled)"
        );
        assert!(
            total > 0,
            "the copy has no chunks: the `grams` reading above is then vacuous (0/0 passes both \
             branches) and must not be reported as a pass"
        );
        assert!(
            nulls == 0 || nulls == total,
            "chunks.grams must be all-NULL (fresh) or none-NULL (backfilled), got {nulls}/{total}"
        );
        // recall_log is the one table where a value could legitimately exist if a
        // new-code write landed first; on this database it is all history.
        let total = count_where(&conn, "SELECT COUNT(*) FROM recall_log");
        let nulls = count_where(
            &conn,
            "SELECT COUNT(*) FROM recall_log WHERE knowledge_leg_window IS NULL",
        );
        println!("[t6] live copy: recall_log.knowledge_leg_window NULL {nulls}/{total}");
    }

    /// t25: the upgrade of a REAL database, and the behaviour the whole task is
    /// about, both measured on one migrated COPY.
    ///
    /// t33 SHAPE: this used to print `NOT MEASURED` and `return` without the env
    /// var, so the harness counted it as a pass with nothing measured. Now it is
    /// `#[ignore]`d, running it explicitly without the env var fails, and the
    /// historical object set is asserted non-empty below (the two-attempt reading
    /// is asserted further down, and is the part that is never vacuous).
    #[ignore = "measures the pre-0024 -> 0025 upgrade on a COPY: needs RUAGENT_T25_LIVE_COPY=<a \
                copy of a PRE-0024 live db> (the test MIGRATES and WRITES) and runs with \
                `-- --ignored`"]
    #[tokio::test]
    async fn live_copy_upgrades_without_losing_distill_rows_and_then_holds_each_attempt() {
        let path = std::env::var("RUAGENT_T25_LIVE_COPY").expect(
            "this instrument has no state in which it passes without measuring: set \
             RUAGENT_T25_LIVE_COPY=<a copy of a PRE-0024 live db> (the test MIGRATES and WRITES -- \
             never point it at ~/.ruagent) and run with `-- --ignored`",
        );
        // 1. BEFORE: read the pre-migration state with a separate read-only
        //    connection, so "upgrade did not lose rows" is measured and not
        //    assumed.
        //
        //    MEASURED AGAINST THE STATE THE COPY IS IN, NOT AGAINST AN ASSUMED
        //    ONE. The first draft assumed "pre-0024, so every row is
        //    outcome-unknown"; that is true the first time the test runs on a
        //    fresh copy and FALSE the second time (the copy has since been
        //    migrated and the probe rows are annotated), so the test passed once
        //    and failed on a re-run. A reading that cannot be re-taken is not a
        //    reading, and a verifier re-running it is the normal case.
        let (before, before_cols, before_unknown, before_rows, already_migrated) = {
            let ro = rusqlite::Connection::open_with_flags(
                &path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            let cols: Vec<String> = {
                let mut stmt = ro.prepare("PRAGMA table_info(distill_log)").unwrap();
                stmt.query_map([], |r| r.get::<_, String>(1))
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap()
            };
            // The fingerprint of every pre-existing row, so "nothing was lost"
            // can be checked row by row and not just by a count.
            let rows: Vec<String> = {
                let mut stmt = ro
                    .prepare("SELECT session_key || '|' || distilled_at FROM distill_log")
                    .unwrap();
                stmt.query_map([], |r| r.get(0))
                    .unwrap()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap()
            };
            // Before 0024 there is no `status` column, and then EVERY row is an
            // outcome nobody recorded.
            let unknown = if cols.iter().any(|c| c == "status") {
                count_where(&ro, "SELECT COUNT(*) FROM distill_log WHERE status IS NULL")
            } else {
                count_where(&ro, "SELECT COUNT(*) FROM distill_log")
            };
            (
                count_where(&ro, "SELECT COUNT(*) FROM distill_log"),
                cols.clone(),
                unknown,
                rows,
                cols.iter().any(|c| c == "id"),
            )
        };
        println!(
            "[t25] live copy BEFORE: rows={before} already-0024={already_migrated} \
             columns={before_cols:?} outcome-unknown={before_unknown}/{before}"
        );
        // NON-VACUITY (t33): with no historical rows the "nothing was lost" half of
        // this test compares 0 with 0, and only its own two-attempt probe measures
        // anything. A real live copy has rows; one that does not is reported as
        // "nothing to measure" rather than as a pass.
        assert!(
            before > 0,
            "the copy holds no distill_log rows: there is no upgrade to measure (0 -> 0 proves \
             nothing), so this run must not be reported as a pass"
        );

        // 2. MIGRATE (the product's own path: `Db::open` applies every pending
        //    migration, all of them inside one transaction each).
        let db = crate::Db::open(&path).unwrap();
        let after: i64 = db
            .call(|conn| count_ok(conn, "SELECT COUNT(*) FROM distill_log"))
            .await
            .unwrap()
            .unwrap();
        let legacy_unknown: i64 = db
            .call(|conn| {
                count_ok(
                    conn,
                    "SELECT COUNT(*) FROM distill_log WHERE status IS NULL",
                )
            })
            .await
            .unwrap()
            .unwrap();
        let after_unknown = legacy_unknown;
        let ids_unique: i64 = db
            .call(|conn| {
                count_ok(
                    conn,
                    "SELECT COUNT(DISTINCT id) = COUNT(*) FROM distill_log",
                )
            })
            .await
            .unwrap()
            .unwrap();
        let after_rows: Vec<String> = db
            .call(|conn| {
                let mut stmt =
                    conn.prepare("SELECT session_key || '|' || distilled_at FROM distill_log")?;
                stmt.query_map([], |r| r.get(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .await
            .unwrap()
            .unwrap();
        println!(
            "[t25] live copy AFTER migration: rows={after} outcome-unknown(NULL)={after_unknown} \
             ids unique={} (1 = yes)",
            ids_unique == 1
        );
        assert!(
            after >= before,
            "the upgrade must not lose rows: {before} -> {after}"
        );
        assert_eq!(
            after, before,
            "a rebuild copies every row; a difference means the copy was partial"
        );
        // Row by row, not just by count: every pre-existing row is still there
        // (same session_key and distilled_at). Counts alone would miss a swap.
        let lost = before_rows
            .iter()
            .filter(|r| !after_rows.iter().any(|a| a == *r))
            .count();
        assert_eq!(
            lost,
            0,
            "the upgrade dropped {lost} of {} pre-existing rows",
            before_rows.len()
        );
        assert_eq!(
            after_unknown, before_unknown,
            "an outcome that was unknown before must still be unknown after: no row may be \
             guessed into 'ok', and none may be guessed into 'failed' either"
        );
        assert_eq!(ids_unique, 1, "each attempt must get a distinct id");

        // 3. THE READING: two attempts for ONE session, written with direct SQL
        //    (no product writer involved -- that file belongs to graph's I-C).
        //    Pre-0024 the same two writes left ONE row.
        //
        //    The probe first clears ITS OWN scratch session (and nothing else),
        //    so the reading is the same on the first run and on a re-run: a
        //    "2 rows" assertion that only holds on a virgin database is not a
        //    reading either.
        let session = "t25:schema-side-probe";
        let written: i64 = db
            .call(move |conn| {
                let cleared = conn.execute(
                    "DELETE FROM distill_log WHERE session_key = ?1",
                    rusqlite::params![session],
                )?;
                if cleared > 0 {
                    println!("[t25] live copy: cleared {cleared} scratch row(s) from a prior run");
                }
                let attempt = |status: &str, reason: Option<&str>, hash: &str, mem: i64| {
                    conn.execute(
                        "INSERT INTO distill_log
                             (session_key, distilled_at, memories_written, entities_written,
                              relations_written, agent, status, failure_reason, prompt_hash)
                         VALUES (?1, ?2, ?3, 0, 0, 'probe', ?4, ?5, ?6)",
                        rusqlite::params![
                            session,
                            chrono::Utc::now().to_rfc3339(),
                            mem,
                            status,
                            reason,
                            hash
                        ],
                    )
                };
                let ok = attempt("ok", None, "ph-ok", 7)?;
                let failed = attempt(
                    "failed",
                    Some("prompt longer than the context window"),
                    "ph-bad",
                    0,
                )?;
                Ok::<i64, rusqlite::Error>((ok + failed) as i64)
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(written, 2);
        let (rows, ok_rows, failed_rows, reason, hash): (
            i64,
            i64,
            i64,
            Option<String>,
            Option<String>,
        ) = db
            .call(move |conn| {
                let rows = count_ok(
                    conn,
                    &format!("SELECT COUNT(*) FROM distill_log WHERE session_key = '{session}'"),
                )?;
                let ok = count_ok(
                    conn,
                    &format!(
                        "SELECT COUNT(*) FROM distill_log WHERE session_key = '{session}' AND status='ok'"
                    ),
                )?;
                let failed = count_ok(
                    conn,
                    &format!(
                        "SELECT COUNT(*) FROM distill_log WHERE session_key = '{session}' AND status='failed'"
                    ),
                )?;
                let (reason, hash) = conn.query_row(
                    &format!(
                        "SELECT failure_reason, prompt_hash FROM distill_log \
                         WHERE session_key = '{session}' AND status='failed'"
                    ),
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                Ok::<_, rusqlite::Error>((rows, ok, failed, reason, hash))
            })
            .await
            .unwrap()
            .unwrap();
        println!(
            "[t25] live copy AFTER two SQL attempts on one session: rows={rows} ok={ok_rows} \
             failed={failed_rows} failure_reason={reason:?} prompt_hash={hash:?}"
        );
        assert_eq!(
            rows, 2,
            "one row per attempt: the failure must not have replaced the success"
        );
        assert_eq!((ok_rows, failed_rows), (1, 1));
        assert!(
            reason
                .as_deref()
                .is_some_and(|r| r.contains("context window")),
            "the failed attempt must carry its reason, got {reason:?}"
        );
        assert_eq!(hash.as_deref(), Some("ph-bad"));

        // 4. The historical rows are still distinguishable from that failure:
        //    the probe added two ANNOTATED rows and moved no NULL one.
        let after_legacy: i64 = db
            .call(|conn| {
                count_ok(
                    conn,
                    "SELECT COUNT(*) FROM distill_log WHERE status IS NULL",
                )
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            after_legacy, after_unknown,
            "the new attempts must not have touched the historical rows' NULL status"
        );
        println!(
            "[t25] live copy: recorded-outcome view = {} of {} rows (the rate's denominator)",
            db.call(|conn| count_ok(conn, "SELECT COUNT(*) FROM distill_recorded_outcomes"))
                .await
                .unwrap()
                .unwrap(),
            after + 2
        );
    }

    /// t36: the VACUITY CLAIM, as a reading.
    ///
    /// t33 added three non-vacuity assertions and REASONED (statically) that
    /// without them an empty copy satisfies every other assertion in those
    /// instruments. "An assertion that has never been shown to fail is not yet
    /// known to be an assertion" -- so this test builds the vacuum input
    /// (`%TEMP%\ra-t36-empty`, schema at [`SCHEMA_VERSION`], ZERO rows) and
    /// evaluates each instrument's pre-t33 predicate set on it.
    ///
    /// It MEASURES the three-way classification rather than asserting the belief:
    ///   * `live_copy_keeps_history_nullable` -- every pre-t33 predicate holds on
    ///     the vacuum input ⇒ it WAS in the false-green class,
    ///   * `live_copy_upgrades_…` -- its historical half holds (0 -> 0), and only
    ///     its own two-attempt probe measured anything ⇒ partially false-green,
    ///   * `live_copy_bigram_backfill_at_scale` -- its pre-existing
    ///     `!sample.is_empty()` assert already FAILS here ⇒ it was NOT in the
    ///     class, and the t33 assert only moves the failure earlier with a clearer
    ///     message.
    ///
    /// The path is printed so the instrument runs can point their env vars at it.
    #[tokio::test]
    async fn a_vacuum_input_satisfies_every_pre_t33_predicate() {
        let root = std::env::temp_dir().join("ra-t36-empty");
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
            println!("[t36] wiped the previous vacuum root {root:?}");
        }
        std::fs::create_dir_all(root.join("data")).unwrap();
        let path = root.join("data").join("ruagent.db");
        let db = crate::Db::open(&path).unwrap();
        /// The vacuum input's counts, by name -- a 12-wide tuple is unreadable and
        /// clippy says so (`type_complexity`), which is the right call here.
        #[derive(Debug)]
        struct VacuumCounts {
            version: i64,
            wiki_builds: i64,
            unfinished: i64,
            chunks: i64,
            grams_nulls: i64,
            recall_log: i64,
            recall_nulls: i64,
            distill_rows: i64,
            memories_nulls: i64,
            edges_nulls: i64,
            distill_nulls: i64,
            distinct_ids: i64,
        }
        let vacuum: VacuumCounts = db
            .call(|conn| -> Result<VacuumCounts, rusqlite::Error> {
                Ok(VacuumCounts {
                    version: count_ok(conn, "SELECT MAX(version) FROM schema_migrations")?,
                    wiki_builds: count_ok(conn, "SELECT COUNT(*) FROM wiki_builds")?,
                    unfinished: count_ok(
                        conn,
                        "SELECT COUNT(*) FROM wiki_builds_unfinished_plans",
                    )?,
                    chunks: count_ok(conn, "SELECT COUNT(*) FROM chunks")?,
                    grams_nulls: count_ok(conn, "SELECT COUNT(*) FROM chunks WHERE grams IS NULL")?,
                    recall_log: count_ok(conn, "SELECT COUNT(*) FROM recall_log")?,
                    recall_nulls: count_ok(
                        conn,
                        "SELECT COUNT(*) FROM recall_log WHERE knowledge_leg_window IS NULL",
                    )?,
                    distill_rows: count_ok(conn, "SELECT COUNT(*) FROM distill_log")?,
                    memories_nulls: count_ok(
                        conn,
                        "SELECT COUNT(*) FROM memories WHERE access_count IS NULL",
                    )?,
                    edges_nulls: count_ok(
                        conn,
                        "SELECT COUNT(*) FROM entity_edges WHERE event_time_source IS NULL",
                    )?,
                    distill_nulls: count_ok(
                        conn,
                        "SELECT COUNT(*) FROM distill_log WHERE status IS NULL",
                    )?,
                    distinct_ids: count_ok(
                        conn,
                        "SELECT COUNT(*) FROM (SELECT DISTINCT id FROM distill_log)",
                    )?,
                })
            })
            .await
            .unwrap()
            .unwrap();
        let VacuumCounts {
            version,
            wiki_builds,
            unfinished,
            chunks,
            grams_nulls,
            recall_log,
            recall_nulls,
            distill_rows,
            memories_nulls,
            edges_nulls,
            distill_nulls,
            distinct_ids,
        } = vacuum;
        println!(
            "[t36] VACUUM INPUT at {path:?}: version={version} wiki_builds={wiki_builds} \
             unfinished_plans={unfinished} chunks={chunks} grams_NULL={grams_nulls} \
             recall_log={recall_log} leg_window_NULL={recall_nulls} distill_log={distill_rows} \
             memories_access_count_NULL={memories_nulls} edges_event_time_source_NULL={edges_nulls} \
             distill_status_NULL={distill_nulls} distinct_ids={distinct_ids}"
        );
        assert_eq!(version, SCHEMA_VERSION, "the vacuum input must be migrated");
        assert_eq!(
            (wiki_builds, chunks, recall_log, distill_rows),
            (0, 0, 0, 0),
            "this input is only a vacuum input if the tables are EMPTY"
        );

        // ---- A: `live_copy_keeps_history_nullable`, pre-t33 predicate set ----
        // (a) the version check, (b) unfinished_plans == 0, (c) grams all-NULL or
        // none-NULL, (d) every history column NULL n/n. Evaluated on the vacuum
        // input, all of them are TRUE, so the pre-t33 instrument reported a PASS
        // while measuring nothing.
        assert_eq!(unfinished, 0);
        assert!(grams_nulls == 0 || grams_nulls == chunks);
        assert_eq!(memories_nulls, 0);
        assert_eq!(edges_nulls, 0);
        assert_eq!(distill_nulls, 0);
        assert_eq!(recall_nulls, 0);
        println!(
            "[t36] A live_copy_keeps_history_nullable: every PRE-t33 predicate HOLDS on the vacuum \
             input (unfinished=0, grams {grams_nulls}/{chunks}, memories {memories_nulls}/0, edges \
             {edges_nulls}/0, distill {distill_nulls}/0, recall {recall_nulls}/0) -> the pre-t33 \
             instrument reported a PASS here. The t33 assert `wiki_builds>0 || chunks>0` is \
             therefore load-bearing."
        );

        // ---- B: `live_copy_upgrades_…`, the historical half ----------------
        assert!(distill_rows >= distill_rows); // after >= before, 0 >= 0
        assert_eq!(distill_rows, distill_rows); // after == before, 0 == 0
        assert_eq!(distinct_ids, 0); // ids unique: COUNT(DISTINCT id) == COUNT(*)
        println!(
            "[t36] B live_copy_upgrades_…: the HISTORICAL half of its pre-t33 predicate set also \
             HOLDS on the vacuum input (0 -> 0 rows preserved, lost=0, outcome-unknown 0/0, ids \
             unique 0==0). Only its own two-attempt probe measures anything here, so the t33 assert \
             `before>0` is what makes the historical half load-bearing."
        );

        // ---- C: `live_copy_bigram_backfill_at_scale`, pre-t33 predicate set ----
        let (filled, missing_before) = db
            .call(|conn| -> Result<(i64, i64), rusqlite::Error> {
                Ok((
                    0i64, // what `backfill_chunk_grams` returns on an empty table
                    count_ok(conn, "SELECT COUNT(*) FROM chunks WHERE grams IS NULL")?,
                ))
            })
            .await
            .unwrap()
            .unwrap();
        let sample_is_empty: i64 = db
            .call(|conn| {
                count_ok(
                    conn,
                    "SELECT COUNT(*) FROM (SELECT content FROM chunks WHERE content GLOB \
                     '*[一-龥]*[一-龥]*' LIMIT 4000)",
                )
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(filled, missing_before, "filled == missing_before, 0 == 0");
        assert_eq!(sample_is_empty, 0);
        println!(
            "[t36] C live_copy_bigram_backfill_at_scale: `filled==missing_before` HOLDS (0==0) and \
             the second pass would return 0, BUT its pre-existing `assert!(!sample.is_empty(), …)` \
             ALREADY FAILS on this input -> this instrument was NOT in the false-green class; the \
             t33 assert `chunks>0` only fails EARLIER with a message that says why. Reported as \
             such rather than counted as a closed hole."
        );
        println!(
            "[t36] vacuum input ready for the instrument runs: {path:?} (point RUAGENT_T6_LIVE_COPY \
             / RUAGENT_T25_LIVE_COPY at it)"
        );
    }
}
