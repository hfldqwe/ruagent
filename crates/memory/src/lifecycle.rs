//! Memory lifecycle (t251): soft delete and restore.
//!
//! A deletion is a TOMBSTONE on the row (deleted_at), never a DELETE: the
//! audit log has to stay answerable ("what was retracted, and when"), and a
//! hard delete would take the row out of memory_diffs reach as well. The row
//! keeps its content, its superseded_at chain and its id, so a restore is an
//! UPDATE and a deletion is reversible by construction.
//!
//! WHY NOT A SIDE TABLE (contrast 0017_session_deletions): that table exists
//! because the session indexer re-writes sessions with INSERT OR REPLACE every
//! 60s and would resurrect a flag on the row. Nothing rescans memories --
//! every writer goes through this module or write -- so the flag cannot be
//! undone behind the reader's back.
//!
//! WHY NOT REUSE superseded_at: supersession is a correction ("this was
//! replaced by that"), deletion is a retraction. One column for both makes
//! "was this replaced, or removed?" undecidable from the data.

use chrono::Utc;

use ruagent_store::Db;

use crate::write::audit;

pub use ruagent_store::DbError;

/// What a delete did. Every case is distinguishable: "not found" and "already
/// deleted" are different facts, and collapsing them into one boolean is how a
/// delete silently becomes a no-op.
#[derive(Debug, Clone, PartialEq)]
pub enum DeleteOutcome {
    Deleted { id: i64, deleted_at: String },
    NotFound,
    AlreadyDeleted { id: i64, deleted_at: String },
}

/// What a restore did.
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreOutcome {
    Restored { id: i64 },
    NotFound,
    NotDeleted { id: i64 },
}

/// Soft-delete one memory. A second call is REPORTED, not performed:
/// AlreadyDeleted carries the original timestamp, so the caller can say when
/// it happened and the tombstone never moves.
pub async fn delete_memory(db: &Db, id: i64) -> Result<DeleteOutcome, DbError> {
    let now = Utc::now().to_rfc3339();
    let stamp = now.clone();
    // Read and write in ONE statement on the single-writer actor: no
    // read-then-write race, and the update only fires when the row was live.
    let found: Option<(String, String, Option<String>)> = db
        .call(
            move |conn| -> Result<Option<(String, String, Option<String>)>, rusqlite::Error> {
                let mut stmt = conn
                    .prepare("SELECT store, namespace, deleted_at FROM memories WHERE id = ?1")?;
                let mut rows = stmt.query([id])?;
                let Some(row) = rows.next()? else {
                    return Ok(None);
                };
                let store: String = row.get(0)?;
                let ns: String = row.get(1)?;
                let already: Option<String> = row.get(2)?;
                if already.is_none() {
                    conn.execute(
                        "UPDATE memories SET deleted_at = ?2, updated_at = ?2
                          WHERE id = ?1 AND deleted_at IS NULL",
                        rusqlite::params![id, stamp],
                    )?;
                }
                Ok(Some((store, ns, already)))
            },
        )
        .await??;

    let Some((store, namespace, already)) = found else {
        return Ok(DeleteOutcome::NotFound);
    };
    let outcome = match already {
        Some(ts) => DeleteOutcome::AlreadyDeleted { id, deleted_at: ts },
        None => DeleteOutcome::Deleted {
            id,
            deleted_at: now,
        },
    };
    // Audit through the SAME writer the write pipeline uses (write::audit):
    // one statement, one place where an op name can be invented. The op set
    // gains "delete"/"restore" beside insert/supersede/skip_dedupe/reject.
    let reason = match &outcome {
        DeleteOutcome::Deleted { .. } => "soft delete".to_string(),
        DeleteOutcome::AlreadyDeleted { deleted_at, .. } => {
            format!("already deleted at {}", deleted_at)
        }
        DeleteOutcome::NotFound => unreachable!("returned above"),
    };
    audit(
        db,
        "delete",
        &store,
        &namespace,
        Some(&format!("id={}", id)),
        None,
        Some(reason),
    )
    .await?;
    Ok(outcome)
}

/// Restore a soft-deleted memory (the reverse UPDATE). Restoring a live row
/// reports NotDeleted instead of pretending something happened.
pub async fn restore_memory(db: &Db, id: i64) -> Result<RestoreOutcome, DbError> {
    let found: Option<(String, String, Option<String>)> = db
        .call(
            move |conn| -> Result<Option<(String, String, Option<String>)>, rusqlite::Error> {
                let mut stmt = conn
                    .prepare("SELECT store, namespace, deleted_at FROM memories WHERE id = ?1")?;
                let mut rows = stmt.query([id])?;
                let Some(row) = rows.next()? else {
                    return Ok(None);
                };
                let store: String = row.get(0)?;
                let ns: String = row.get(1)?;
                let was_deleted: Option<String> = row.get(2)?;
                if was_deleted.is_some() {
                    conn.execute(
                        "UPDATE memories SET deleted_at = NULL, updated_at = ?2 WHERE id = ?1",
                        rusqlite::params![id, Utc::now().to_rfc3339()],
                    )?;
                }
                Ok(Some((store, ns, was_deleted)))
            },
        )
        .await??;

    let Some((store, namespace, was_deleted)) = found else {
        return Ok(RestoreOutcome::NotFound);
    };
    let outcome = match was_deleted {
        Some(_) => RestoreOutcome::Restored { id },
        None => RestoreOutcome::NotDeleted { id },
    };
    if matches!(outcome, RestoreOutcome::Restored { .. }) {
        audit(
            db,
            "restore",
            &store,
            &namespace,
            Some(&format!("id={}", id)),
            None,
            Some("restored".to_string()),
        )
        .await?;
    }
    Ok(outcome)
}

/// What a purge did. Deliberately NOT a DeleteOutcome: a purge leaves no
/// tombstone to report, and "there was nothing there" must not read as
/// success (t276).
#[derive(Debug, Clone, PartialEq)]
pub enum PurgeOutcome {
    /// The row is GONE. `was_deleted` says whether it had been tombstoned.
    Purged {
        id: i64,
        was_deleted: bool,
        /// How many rows pointed at this one through their supersedes column
        /// and had that link detached so the hard delete could proceed (t317).
        /// Zero is the common case; a non-zero count is the operation SAYING it
        /// also cleared a pointer instead of failing with a 500.
        links_cleared: usize,
    },
    NotFound,
}

/// Hard-delete one memory: the row leaves `memories` (the FTS index follows
/// via `memories_ad`). This is the ONLY path that removes a secret someone
/// pasted into a memory — a soft delete keeps the content on the row for the
/// audit log. Purging a tombstone is allowed on purpose: that is how a
/// retracted secret stops being stored at all. An absent id reports NotFound
/// instead of a silent 200.
pub async fn purge_memory(db: &Db, id: i64) -> Result<PurgeOutcome, DbError> {
    let found: Option<(String, String, Option<String>, usize)> =
        db
            .call(
                move |conn| -> Result<
                    Option<(String, String, Option<String>, usize)>,
                    rusqlite::Error,
                > {
                    let mut stmt = conn.prepare(
                        "SELECT store, namespace, deleted_at FROM memories WHERE id = ?1",
                    )?;
                    let mut rows = stmt.query([id])?;
                    let Some(row) = rows.next()? else {
                        return Ok(None);
                    };
                    let store: String = row.get(0)?;
                    let ns: String = row.get(1)?;
                    let was_deleted: Option<String> = row.get(2)?;
                    // t317 (F-310a): supersedes is a SELF-REFERENCING foreign key,
                    // so a hard delete used to depend on the order of the purges:
                    // the row a live child still pointed at came back as
                    // "FOREIGN KEY constraint failed" (SQLite 787) and the daemon
                    // turned that into a 500. The link is internal bookkeeping, not
                    // a user-level precondition -- detach it first, and REPORT how
                    // many were detached rather than doing it silently.
                    let links_cleared = conn.execute(
                        "UPDATE memories SET supersedes = NULL WHERE supersedes = ?1",
                        [id],
                    )?;
                    conn.execute("DELETE FROM memories WHERE id = ?1", [id])?;
                    Ok(Some((store, ns, was_deleted, links_cleared)))
                },
            )
            .await??;
    let Some((store, namespace, was_deleted, links_cleared)) = found else {
        return Ok(PurgeOutcome::NotFound);
    };
    // Same audit channel as delete/restore (write::audit): the op set gains
    // "purge" beside insert/supersede/skip_dedupe/reject/delete/restore.
    // The audit says what the row WAS and what the purge also had to do.
    let what = if was_deleted.is_some() {
        "purged a tombstone (hard delete)"
    } else {
        "purged a live row (hard delete)"
    };
    let reason = if links_cleared == 0 {
        what.to_string()
    } else {
        format!("{what}; detached {links_cleared} supersede link(s)")
    };
    audit(
        db,
        "purge",
        &store,
        &namespace,
        Some(&format!("id={}", id)),
        None,
        Some(reason),
    )
    .await?;
    Ok(PurgeOutcome::Purged {
        id,
        was_deleted: was_deleted.is_some(),
        links_cleared,
    })
}

// ── t347: the "[distilled] " prefix migration ───────────────────────────────

/// The marker distillation used to write into the body.
pub const DISTILLED_PREFIX: &str = "[distilled] ";

/// The episode the migrated rows are pointed at, for the rows whose body said
/// "distilled" but whose session was never recorded.
///
/// A REAL episode row, not a sentinel number: `source_episode` carries a
/// FOREIGN KEY to `episodes(id)`, and -1 is rejected by SQLite (MEASURED on
/// this migration's first version: SqliteFailure extended_code 787). The
/// episode's own text says what it is, so a reader who follows the reference
/// learns "distilled, session not recorded" instead of a magic number whose
/// meaning lives in a comment.
pub const LEGACY_EPISODE_TEXT: &str =
    "legacy distilled memories: the [distilled] body prefix was removed by the t347 migration;      the session these came from was not recorded when they were written";
pub const LEGACY_EPISODE_KEY: &str = "ruagent:legacy-distilled-prefix";

/// One row, before and after. Returned so the caller can show the change
/// verbatim instead of trusting a count.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PrefixSample {
    pub id: i64,
    pub before: String,
    pub after: String,
}

/// What the migration did. Every field is a count a reader can check.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PrefixMigrationOutcome {
    /// Rows whose body started with the marker (the ones this rewrites).
    pub scanned: usize,
    /// Rows actually rewritten (== scanned on the first run, 0 afterwards).
    pub stripped: usize,
    /// Rows whose body CONTAINS the marker somewhere else: counted, NOT
    /// touched. This is the reverse case, and the reason the operation is a
    /// prefix strip and not a replace.
    pub marker_inside_only: usize,
    /// Rows that had no episode id and got `LEGACY_DISTILLED_EPISODE`.
    pub backfilled: usize,
    /// Where the BEFORE contents were written (length-prefixed: id + byte length + bytes).
    pub backup: String,
    pub samples: Vec<PrefixSample>,
}

/// Strip the leading `[distilled] ` from every row that has it, once.
///
/// WHY A MIGRATION AND NOT A READ-SIDE TRIM: the prefix lives in the STORED
/// bytes, so every reader -- injection, recall, near-duplicate detection, the
/// panel -- had to know about it. Trimming on read leaves that knowledge in five
/// places; rewriting the row leaves it in none.
///
/// WHAT IT TOUCHES, EXACTLY: `content.strip_prefix(DISTILLED_PREFIX)` and
/// nothing else. A row whose body mentions the marker anywhere but the start is
/// counted in `marker_inside_only` and left byte-identical.
///
/// WHY A BACKUP FILE: 156 rows change. The file holds id + the BEFORE content,
/// so the rollback is mechanical:
/// `UPDATE memories SET content = <before> WHERE id = <id>;`
/// It is also this rewrite's audit trail: `memory_diffs` records semantic
/// changes and this is not one -- the memory means the same thing afterwards.
///
/// NOT BUMPED: `updated_at`. The row did not change meaning, and bumping it
/// would move 156 rows to the top of every "recently updated" list.
/// BUMPED: `content_hash`, because the bytes changed and the exact-duplicate
/// check reads that column.
pub async fn strip_distilled_prefix(
    db: &Db,
    backup_dir: &std::path::Path,
) -> Result<PrefixMigrationOutcome, DbError> {
    let all: Vec<(i64, String, Option<i64>)> = db
        .call(|conn| -> Result<Vec<(i64, String, Option<i64>)>, rusqlite::Error> {
            let mut stmt = conn.prepare("SELECT id, content, source_episode FROM memories ORDER BY id")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
        .map_err(DbError::from)?;

    let mut candidates: Vec<(i64, String, Option<i64>, String)> = Vec::new();
    let mut marker_inside_only = 0usize;
    for (id, content, episode) in &all {
        match content.strip_prefix(DISTILLED_PREFIX) {
            Some(rest) => candidates.push((*id, content.clone(), *episode, rest.to_string())),
            None => {
                if content.contains("[distilled]") {
                    marker_inside_only += 1;
                }
            }
        }
    }

    std::fs::create_dir_all(backup_dir)?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let backup = backup_dir.join(format!("memories-distilled-prefix-{stamp}.txt"));
    // LENGTH-PREFIXED, not JSON: memory content can contain quotes and newlines,
    // and this crate has no JSON dependency. One record = "<id> <byte_len>" then
    // exactly that many bytes of the BEFORE content then a newline, so the file
    // round-trips byte for byte and the rollback is mechanical.
    let mut lines: Vec<u8> = Vec::new();
    for (id, before, _, _) in &candidates {
        lines.extend_from_slice(format!("{id} {}", before.len()).as_bytes());
        lines.push(b'\n');
        lines.extend_from_slice(before.as_bytes());
        lines.push(b'\n');
    }
    std::fs::write(&backup, lines)?;

    // A row that never recorded its session still needs a reference the FOREIGN
    // KEY accepts, so the migration records ONE episode that says what it is
    // (idempotent by content hash, so a second run reuses it).
    let needs_backfill = candidates.iter().any(|(_, _, ep, _)| ep.is_none());
    let legacy_episode: i64 = if needs_backfill {
        crate::episode::record_episode(
            db,
            crate::episode::EpisodeKind::Manual,
            LEGACY_EPISODE_TEXT,
            Some(LEGACY_EPISODE_KEY),
        )
        .await?
    } else {
        // Never used: with no legacy episode there is no candidate whose
        // source_episode is NULL, so COALESCE keeps the value it finds. If that
        // ever stopped being true, the FOREIGN KEY would reject 0 loudly.
        0
    };

    let mut stripped = 0usize;
    let mut backfilled = 0usize;
    let mut samples: Vec<PrefixSample> = Vec::new();
    for (id, before, episode, after) in &candidates {
        let hash = crate::write::content_hash(after);
        let set_episode = episode.is_none();
        // Owned copies: the writer closure must be 'static (Db::call).
        let after_owned = after.clone();
        let id_owned = *id;
        db.call(move |conn| -> Result<(), rusqlite::Error> {
            conn.execute(
                "UPDATE memories SET content = ?1, content_hash = ?2,
                        source_episode = COALESCE(source_episode, ?3)
                 WHERE id = ?4",
                rusqlite::params![after_owned, hash, legacy_episode, id_owned],
            )?;
            Ok(())
        })
        .await?
        .map_err(DbError::from)?;
        stripped += 1;
        if set_episode {
            backfilled += 1;
        }
        if samples.len() < 5 {
            samples.push(PrefixSample { id: *id, before: before.clone(), after: after.clone() });
        }
    }

    Ok(PrefixMigrationOutcome {
        scanned: candidates.len(),
        stripped,
        marker_inside_only,
        backfilled,
        backup: backup.display().to_string(),
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryStore;
    use crate::namespace::Namespace;
    use crate::query::{
        all_memories, count_memories, current_memories, get_memory, list_diffs, search_fts,
    };
    use crate::write::{MemoryWrite, WriteOutcome, write_memory};

    fn obs(content: &str, ns: &str) -> MemoryWrite {
        MemoryWrite {
            store: MemoryStore::Observation,
            namespace: Namespace::parse(ns).unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        }
    }

    async fn seed(db: &Db, content: &str) -> i64 {
        let WriteOutcome::Inserted(id) = write_memory(db, &obs(content, "user")).await.unwrap()
        else {
            panic!("seed insert")
        };
        id
    }

    /// t317 (F-310a): supersedes is a SELF-REFERENCING foreign key, so a hard
    /// delete used to depend on the ORDER of the purges -- the row a live child
    /// still pointed at could not be purged at all (SQLite 787, "FOREIGN KEY
    /// constraint failed"), which the daemon turns into a 500.
    ///
    /// MEASURED BEFORE THE FIX, crate level (in-memory DB):
    ///   Err(Sqlite(SqliteFailure(Error { code: ConstraintViolation,
    ///       extended_code: 787 }, Some("FOREIGN KEY constraint failed"))))
    /// and on the live daemon the same call answered
    ///   HTTP 500  "sqlite error: FOREIGN KEY constraint failed".
    #[tokio::test]
    async fn purge_detaches_the_supersede_links_instead_of_failing() {
        let db = Db::open_in_memory().unwrap();
        let a = seed(&db, "t317 A the superseded parent").await;
        let mut w = obs("t317 B the superseding child", "user");
        w.supersedes = Some(a);
        let b = match write_memory(&db, &w).await.unwrap() {
            WriteOutcome::Superseded { old, new } => {
                assert_eq!(old, a, "the child must supersede the parent we named");
                new
            }
            other => panic!("expected Superseded, got {other:?}"),
        };
        assert!(
            get_memory(&db, a)
                .await
                .unwrap()
                .unwrap()
                .superseded_at
                .is_some(),
            "the parent must be marked superseded, or this is not the t317 case"
        );

        // THE ORDER THAT USED TO 500: the parent first, its child still alive.
        let parent = purge_memory(&db, a).await;
        println!("T317 purge parent={a} while child={b} is alive: {parent:?}");
        match parent.unwrap() {
            PurgeOutcome::Purged {
                id,
                was_deleted,
                links_cleared,
            } => {
                assert_eq!(id, a);
                assert!(!was_deleted, "it was live, not a tombstone");
                assert_eq!(
                    links_cleared, 1,
                    "the detached link must be REPORTED, not done silently"
                );
            }
            other => panic!("expected Purged, got {other:?}"),
        }
        // The child is still purgeable afterwards, with nothing left dangling.
        assert!(purge_memory(&db, b).await.is_ok());

        // The order that always worked, on a fresh pair (it must keep working).
        let c = seed(&db, "t317 C the parent").await;
        let mut w2 = obs("t317 D the child", "user");
        w2.supersedes = Some(c);
        let d = match write_memory(&db, &w2).await.unwrap() {
            WriteOutcome::Superseded { new, .. } => new,
            other => panic!("expected Superseded, got {other:?}"),
        };
        assert!(purge_memory(&db, d).await.is_ok());
        let second = purge_memory(&db, c).await.unwrap();
        match second {
            PurgeOutcome::Purged { links_cleared, .. } => {
                assert_eq!(
                    links_cleared, 0,
                    "the child is gone, so there is no link to detach"
                )
            }
            other => panic!("expected Purged, got {other:?}"),
        }
    }

    /// The OTHER reading of F-310a: a LEGACY row whose supersedes already points
    /// at a row that is gone. Reachable when the FK was off (an older build, or
    /// a restore that inserted the child first), and the task title names this
    /// shape -- so it is measured rather than assumed. Measured live too: the
    /// child answered 200 while the parent was the one that 500'd.
    #[tokio::test]
    async fn purge_a_row_whose_supersedes_already_dangles() {
        let db = Db::open_in_memory().unwrap();
        let orphan = db
            .call(|conn| -> Result<i64, rusqlite::Error> {
                conn.execute_batch("PRAGMA foreign_keys=OFF")?;
                conn.execute(
                    "INSERT INTO memories (store, namespace, content, content_hash, confidence,
                                           supersedes, created_at, updated_at)
                     VALUES ('observation','user','t317 legacy orphan','t317-legacy-hash',0.5,
                             999999,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
                    [],
                )?;
                let id = conn.last_insert_rowid();
                conn.execute_batch("PRAGMA foreign_keys=ON")?;
                Ok(id)
            })
            .await
            .unwrap()
            .unwrap();
        let out = purge_memory(&db, orphan).await;
        println!("T317 purge a row whose supersedes points at 999999 (gone): {out:?}");
        assert!(
            out.is_ok(),
            "a dangling supersedes must not block the purge: {out:?}"
        );
        match out.unwrap() {
            PurgeOutcome::Purged { links_cleared, .. } => assert_eq!(links_cleared, 0),
            other => panic!("expected Purged, got {other:?}"),
        }
    }

    /// The reverse of the fix: a REAL internal failure must still be an error.
    /// The daemon maps this error class to 500 (measured live for the FK case
    /// before the fix: "sqlite error: FOREIGN KEY constraint failed" with status
    /// 500), so swallowing it here would hide a genuine fault behind a 200.
    #[tokio::test]
    async fn a_real_internal_failure_is_still_an_error() {
        let db = Db::open_in_memory().unwrap();
        let id = seed(&db, "t317 E a row whose audit table is missing").await;
        db.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch("DROP TABLE memory_diffs")?;
            Ok(())
        })
        .await
        .unwrap()
        .unwrap();
        let out = purge_memory(&db, id).await;
        println!("T317 purge with the audit table dropped: {out:?}");
        assert!(
            out.is_err(),
            "a genuine internal failure must not be reported as success: {out:?}"
        );
    }    /// t347 acceptance 2 + 5: the migration strips ONLY the leading marker, and a
    /// row whose body mentions the marker somewhere else is left byte-identical.
    /// The control row (never had the marker) is asserted too, so a migration that
    /// rewrote everything would fail here.
    #[tokio::test]
    async fn the_prefix_migration_strips_only_the_leading_marker() {
        let db = Db::open_in_memory().unwrap();
        let legacy = seed(&db, "[distilled] 用户偏好使用简体中文交流。").await;
        let control = seed(&db, "the deploy script lives in scripts/deploy.sh").await;
        let inside = seed(&db, "the agent said [distilled] in its own words").await;
        let dir = std::env::temp_dir().join(format!("ruagent-t347-{}", std::process::id()));
        let out = strip_distilled_prefix(&db, &dir).await.unwrap();
        println!("READING t347 migration: {out:?}");
        assert_eq!(out.scanned, 1, "only the row that STARTS with the marker");
        assert_eq!(out.stripped, 1);
        assert_eq!(out.marker_inside_only, 1);
        assert_eq!(out.backfilled, 1, "the legacy row had no episode id");

        let a = get_memory(&db, legacy).await.unwrap().unwrap();
        assert_eq!(a.content, "用户偏好使用简体中文交流。");
        let legacy_id = a.source_episode.expect("a real episode id, not a sentinel");
        let kind: String = db
            .call(move |conn| {
                conn.query_row("SELECT kind FROM episodes WHERE id = ?1", [legacy_id], |r| r.get(0))
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kind, "manual", "the migration episode says what it is");

        let b = get_memory(&db, control).await.unwrap().unwrap();
        assert_eq!(b.content, "the deploy script lives in scripts/deploy.sh");
        assert_eq!(b.source_episode, None, "a row without the marker gains nothing");

        let c = get_memory(&db, inside).await.unwrap().unwrap();
        assert_eq!(
            c.content, "the agent said [distilled] in its own words",
            "the marker INSIDE the body is not provenance and must not be touched"
        );

        // The backup holds the BEFORE bytes, so the rewrite is reversible.
        let backup = std::fs::read_to_string(&out.backup).unwrap();
        assert!(
            backup.contains("[distilled] 用户偏好使用简体中文交流。"),
            "backup must carry the before content: {backup:?}"
        );

        // Idempotent: a second run changes nothing, and says so.
        let again = strip_distilled_prefix(&db, &dir).await.unwrap();
        assert_eq!(again.scanned, 0);
        assert_eq!(again.stripped, 0);
        assert_eq!(again.marker_inside_only, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole contract in one test: the row survives, the audit records it,
    /// and every read path stops returning it.
    #[tokio::test]
    async fn soft_delete_hides_from_every_read_path_and_audits() {
        let db = Db::open_in_memory().unwrap();
        let keep = seed(&db, "the deploy script lives in scripts/deploy.sh").await;
        let doomed = seed(&db, "an obsolete note about the kettle").await;

        let stamp = match delete_memory(&db, doomed).await.unwrap() {
            DeleteOutcome::Deleted { deleted_at, .. } => deleted_at,
            other => panic!("expected Deleted, got {other:?}"),
        };

        // 1. The row is still there, with deleted_at set (soft, not gone).
        let row = get_memory(&db, doomed).await.unwrap().unwrap();
        assert_eq!(row.deleted_at.as_deref(), Some(stamp.as_str()));
        assert_eq!(row.content, "an obsolete note about the kettle");

        // 2. Gone from list / count / search / current.
        let listed = all_memories(&db, None, None, 50).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, keep);
        assert_eq!(count_memories(&db, None, None).await.unwrap(), 1);
        assert!(search_fts(&db, "kettle", 10).await.unwrap().is_empty());
        assert_eq!(
            current_memories(&db, MemoryStore::Observation, "user", 10)
                .await
                .unwrap()
                .len(),
            1
        );

        // 3. The audit has the delete, with the store and namespace.
        let diffs = list_diffs(&db, 50).await.unwrap();
        let d = diffs.iter().find(|d| d.op == "delete").expect("audited");
        assert_eq!(d.before.as_deref(), Some(format!("id={}", doomed).as_str()));
        assert_eq!(d.after, None);
        assert_eq!(d.mem_store.as_deref(), Some("observation"));
        assert_eq!(d.namespace.as_deref(), Some("user"));

        // 4. Deleting again reports, and does not move the timestamp.
        match delete_memory(&db, doomed).await.unwrap() {
            DeleteOutcome::AlreadyDeleted { deleted_at, .. } => assert_eq!(deleted_at, stamp),
            other => panic!("expected AlreadyDeleted, got {other:?}"),
        }
        assert_eq!(
            get_memory(&db, doomed)
                .await
                .unwrap()
                .unwrap()
                .deleted_at
                .as_deref(),
            Some(stamp.as_str()),
            "a second delete must not move the tombstone"
        );

        // 5. Restore puts it back.
        assert_eq!(
            restore_memory(&db, doomed).await.unwrap(),
            RestoreOutcome::Restored { id: doomed }
        );
        assert!(
            get_memory(&db, doomed)
                .await
                .unwrap()
                .unwrap()
                .deleted_at
                .is_none()
        );
        assert_eq!(all_memories(&db, None, None, 50).await.unwrap().len(), 2);
        assert_eq!(search_fts(&db, "kettle", 10).await.unwrap().len(), 1);
        assert!(
            list_diffs(&db, 50)
                .await
                .unwrap()
                .iter()
                .any(|d| d.op == "restore")
        );
    }

    #[tokio::test]
    async fn unknown_id_is_reported_not_ignored() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(
            delete_memory(&db, 404).await.unwrap(),
            DeleteOutcome::NotFound
        );
        assert_eq!(
            restore_memory(&db, 404).await.unwrap(),
            RestoreOutcome::NotFound
        );
        let id = seed(&db, "still live").await;
        assert_eq!(
            restore_memory(&db, id).await.unwrap(),
            RestoreOutcome::NotDeleted { id }
        );
    }

    /// Deleting a superseded row must not resurrect it, and must not touch the
    /// row that superseded it.
    #[tokio::test]
    async fn delete_is_independent_of_supersession() {
        let db = Db::open_in_memory().unwrap();
        let old = seed(&db, "v1 of the note").await;
        let mut w = obs("v2 of the note", "user");
        w.supersedes = Some(old);
        let WriteOutcome::Superseded { new, .. } = write_memory(&db, &w).await.unwrap() else {
            panic!("supersede")
        };
        assert!(matches!(
            delete_memory(&db, old).await.unwrap(),
            DeleteOutcome::Deleted { .. }
        ));
        let row = get_memory(&db, old).await.unwrap().unwrap();
        assert!(row.superseded_at.is_some() && row.deleted_at.is_some());
        assert_eq!(all_memories(&db, None, None, 50).await.unwrap().len(), 1);
        assert!(
            get_memory(&db, new)
                .await
                .unwrap()
                .unwrap()
                .deleted_at
                .is_none()
        );
    }

    /// The store/namespace filter used by /api/v1/memory/list: no filter means
    /// no filter, and a store filter must not silently become observation.
    #[tokio::test]
    async fn filters_narrow_exactly() {
        let db = Db::open_in_memory().unwrap();
        write_memory(
            &db,
            &MemoryWrite {
                store: MemoryStore::Lesson,
                namespace: Namespace::parse("global").unwrap(),
                content: "a lesson".into(),
                confidence: 0.9,
                source_episode: None,
                supersedes: None,
            },
        )
        .await
        .unwrap();
        write_memory(
            &db,
            &MemoryWrite {
                store: MemoryStore::Procedure,
                namespace: Namespace::parse("global").unwrap(),
                content: "a procedure".into(),
                confidence: 0.9,
                source_episode: None,
                supersedes: None,
            },
        )
        .await
        .unwrap();
        seed(&db, "an observation").await;

        assert_eq!(all_memories(&db, None, None, 50).await.unwrap().len(), 3);
        assert_eq!(
            all_memories(&db, Some(MemoryStore::Lesson), None, 50)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            all_memories(&db, Some(MemoryStore::Lesson), None, 50)
                .await
                .unwrap()[0]
                .namespace,
            "global"
        );
        assert_eq!(
            count_memories(&db, Some(MemoryStore::Procedure), None)
                .await
                .unwrap(),
            1
        );
        // The old behaviour: namespace defaulted to "user", so a lesson query
        // returned 0 rows while the store held one. No filter must mean all.
        assert_eq!(
            all_memories(&db, Some(MemoryStore::Lesson), None, 50)
                .await
                .unwrap()
                .len(),
            1,
            "a store filter alone must not add a namespace filter"
        );
        assert_eq!(
            all_memories(&db, None, Some("global".to_string()), 50)
                .await
                .unwrap()
                .len(),
            2
        );
    }
}
