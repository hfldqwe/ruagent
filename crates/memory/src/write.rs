//! The governed write pipeline (design §6.3): dedupe → namespace check →
//! insert/supersede → audit. Never loads all memories (Agno's token trap
//! is a design non-goal here — writes are hash-keyed row operations).

use chrono::Utc;

use ruagent_store::Db;

use crate::MemoryStore;
use crate::namespace::Namespace;

pub use ruagent_store::DbError;

/// One governed write request.
#[derive(Debug, Clone)]
pub struct MemoryWrite {
    pub store: MemoryStore,
    pub namespace: Namespace,
    pub content: String,
    /// Confidence 0..=1; explicit writes are 1.0, auto-extractions lower.
    pub confidence: f64,
    /// Optional episode id for provenance.
    pub source_episode: Option<i64>,
    /// Content that this memory replaces (same store+namespace); the old
    /// row is superseded, never deleted (design §6.6 #1 spirit).
    pub supersedes: Option<i64>,
}

/// What the pipeline did.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteOutcome {
    /// New memory inserted.
    Inserted(i64),
    /// Old memory superseded by a new one.
    Superseded { old: i64, new: i64 },
    /// Identical content already current — skipped, audited.
    SkippedDuplicate(i64),
    /// Namespace not allowed for this store — rejected, audited.
    RejectedNamespace,
}

/// SHA-256 hex of the content — the dedupe key.
pub fn content_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Run one write through the pipeline.
pub async fn write_memory(db: &Db, write: &MemoryWrite) -> Result<WriteOutcome, DbError> {
    // 1. Namespace governance — enforced here, in code (design §6.2).
    if !write.store.allows_namespace(&write.namespace) {
        audit(
            db,
            "reject",
            write.store.as_str(),
            &write.namespace.to_string(),
            None,
            None,
            Some(format!(
                "store `{}` cannot write namespace `{}`",
                write.store.as_str(),
                write.namespace
            )),
        )
        .await?;
        return Ok(WriteOutcome::RejectedNamespace);
    }

    let hash = content_hash(&write.content);
    let store = write.store.as_str();
    let ns = write.namespace.to_string();
    let now = Utc::now().to_rfc3339();
    let content = write.content.clone();
    let confidence = write.confidence.clamp(0.0, 1.0);
    let source_episode = write.source_episode;
    let supersedes = write.supersedes;

    // The whole dedupe+insert+supersede decision is one transaction on
    // the single-writer actor — no read-then-write races by construction.
    let result: WriteOutcome = db
        .call(move |conn| -> Result<WriteOutcome, rusqlite::Error> {
            // 2. Dedupe: identical current content already exists?
            let existing: Option<i64> = conn
                .query_row(
                    "SELECT id FROM memories
                     WHERE store = ?1 AND namespace = ?2 AND content_hash = ?3
                       AND superseded_at IS NULL",
                    rusqlite::params![store, ns, hash],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?;
            if let Some(id) = existing {
                return Ok(WriteOutcome::SkippedDuplicate(id));
            }

            // 3. Supersede the target if given.
            if let Some(old_id) = supersedes {
                conn.execute(
                    "UPDATE memories SET superseded_at = ?4, updated_at = ?4 WHERE id = ?1
                     AND store = ?2 AND namespace = ?3 AND superseded_at IS NULL",
                    rusqlite::params![old_id, store, ns, now],
                )?;
            }

            conn.execute(
                "INSERT INTO memories (store, namespace, content, content_hash, confidence,
                                       source_episode, supersedes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                rusqlite::params![
                    store,
                    ns,
                    content,
                    hash,
                    confidence,
                    source_episode,
                    supersedes,
                    now
                ],
            )?;
            let new_id = conn.last_insert_rowid();
            Ok(if let Some(old_id) = supersedes {
                WriteOutcome::Superseded {
                    old: old_id,
                    new: new_id,
                }
            } else {
                WriteOutcome::Inserted(new_id)
            })
        })
        .await??;

    // 4. Audit.
    let (op, before, after) = match &result {
        WriteOutcome::Inserted(id) => ("insert", None, Some(format!("id={id}"))),
        WriteOutcome::Superseded { old, new } => (
            "supersede",
            Some(format!("id={old}")),
            Some(format!("id={new}")),
        ),
        WriteOutcome::SkippedDuplicate(id) => ("skip_dedupe", None, Some(format!("id={id}"))),
        WriteOutcome::RejectedNamespace => unreachable!("rejected before the transaction"),
    };
    audit(
        db,
        op,
        write.store.as_str(),
        &write.namespace.to_string(),
        before.as_deref(),
        after.as_deref(),
        None,
    )
    .await?;

    Ok(result)
}

/// Append to the audit log. `pub(crate)` so the lifecycle path (t251)
/// writes the same table through the same statement — one audit writer, not
/// two that can drift.
pub(crate) async fn audit(
    db: &Db,
    op: &str,
    mem_store: &str,
    namespace: &str,
    before: Option<&str>,
    after: Option<&str>,
    reason: Option<String>,
) -> Result<(), DbError> {
    let op = op.to_string();
    let mem_store = mem_store.to_string();
    let namespace = namespace.to_string();
    let before = before.map(str::to_string);
    let after = after.map(str::to_string);
    let now = Utc::now().to_rfc3339();
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO memory_diffs (ts, op, mem_store, namespace, before, after, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![now, op, mem_store, namespace, before, after, reason],
        )
    })
    .await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::Namespace;

    fn write(store: MemoryStore, ns: &str, content: &str) -> MemoryWrite {
        MemoryWrite {
            store,
            namespace: Namespace::parse(ns).unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        }
    }

    #[tokio::test]
    async fn dedupe_supersede_reject_and_audit() {
        let db = Db::open_in_memory().unwrap();

        // Insert.
        let out = write_memory(&db, &write(MemoryStore::Observation, "user", "likes tea"))
            .await
            .unwrap();
        let WriteOutcome::Inserted(id1) = out else {
            panic!("insert")
        };

        // Same content again: skipped.
        let out = write_memory(&db, &write(MemoryStore::Observation, "user", "likes tea"))
            .await
            .unwrap();
        assert_eq!(out, WriteOutcome::SkippedDuplicate(id1));

        // Updated content supersedes the old one.
        let mut w = write(MemoryStore::Observation, "user", "prefers coffee");
        w.supersedes = Some(id1);
        let out = write_memory(&db, &w).await.unwrap();
        let WriteOutcome::Superseded { old, new } = out else {
            panic!("supersede")
        };
        assert_eq!(old, id1);
        assert_ne!(new, id1);

        // Old memory still exists (superseded, not deleted).
        let old_still_there: i64 = db
            .call(move |conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM memories WHERE id = ?1 AND superseded_at IS NOT NULL",
                    [old],
                    |r| r.get(0),
                )
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(old_still_there, 1);

        // Namespace governance: profile must be user-scoped.
        let out = write_memory(&db, &write(MemoryStore::Profile, "project:x", "structured"))
            .await
            .unwrap();
        assert_eq!(out, WriteOutcome::RejectedNamespace);
        let out = write_memory(&db, &write(MemoryStore::Procedure, "user", "how-to"))
            .await
            .unwrap();
        assert_eq!(out, WriteOutcome::RejectedNamespace);

        // Audit rows exist for every decision.
        let audits: i64 = db
            .call(|conn| conn.query_row("SELECT COUNT(*) FROM memory_diffs", [], |r| r.get(0)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(audits, 5, "insert + skip + supersede + 2 rejects");
    }

    #[tokio::test]
    async fn hash_is_stable() {
        assert_eq!(content_hash("abc"), content_hash("abc"));
        assert_ne!(content_hash("abc"), content_hash("abd"));
        assert_eq!(content_hash("").len(), 64);
    }
}
