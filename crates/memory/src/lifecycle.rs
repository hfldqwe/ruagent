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
