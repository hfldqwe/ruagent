//! Consolidation: a memory that names what it was derived from (R-B C4, E.4).
//!
//! WHAT WAS MISSING. Session distillation produced memories (85 written over 33
//! sessions, live DB 2026-09-27) and nothing could ever SYNTHESISE them: there
//! was no second pipeline, and `memories` had no way to say "this row is a
//! conclusion drawn from those rows". A reflection product without provenance is
//! exactly the failure R-B B.4 warns about — it would raise the marked-row
//! coverage again after t347 spent a migration lowering it (§7.140).
//!
//! WHAT THIS MODULE IS. The memory-side half of consolidation: store the
//! conclusions' source links (0020's `memory_sources`), idempotently. It does NOT
//! call an LLM, does not decide what to consolidate, and does not touch
//! `distill.rs` (I-C/t9 owns the producing side). That split is registered in the
//! report as jointly-delivered.
//!
//! The reverse index direction matters for erasure: "given an episode, which
//! memories claim it" is what the residue scan (E.5) could not answer before.

use chrono::Utc;
use ruagent_store::Db;

use crate::write::DbError;

/// What a consolidation conclusion was derived from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Memory,
    Episode,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Memory => "memory",
            SourceKind::Episode => "episode",
        }
    }

    pub fn try_parse(s: &str) -> Option<Self> {
        match s {
            "memory" => Some(SourceKind::Memory),
            "episode" => Some(SourceKind::Episode),
            _ => None,
        }
    }
}

/// One consolidation: the new row plus the sources it claims.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationWrite {
    pub memory_id: i64,
    pub sources: Vec<(SourceKind, i64)>,
}

/// The idempotency key for one conclusion: the content plus the SORTED source set.
///
/// Why a key at all: re-running a session-close job must not add the same
/// conclusion twice, and "same content, same sources" is the only definition of
/// the same conclusion that does not need an LLM to re-judge it. Canonicalising
/// the source order is what makes it stable across runs.
pub fn consolidation_key(content: &str, sources: &[(SourceKind, i64)]) -> String {
    let mut s: Vec<(String, i64)> = sources
        .iter()
        .map(|(k, id)| (k.as_str().to_string(), *id))
        .collect();
    s.sort();
    s.dedup();
    let mut joined = String::new();
    for (k, id) in s {
        joined.push_str(&format!("{k}:{id},"));
    }
    crate::write::content_hash(&format!("{content}\u{1f}{joined}"))
}

/// Store the source links of one conclusion. `INSERT OR IGNORE`, so a second call
/// adds nothing and says so by returning 0.
///
/// The memory row itself is written by the normal governed pipeline; this only
/// records where it came from — one direction, one table, no second write path
/// for memory content.
///
/// AUDIT (t31 / RV-B-2, captain's ruling). `memory_sources` is a table, not a
/// decision: before this, `op='consolidate'` existed nowhere in the repo, so a
/// consolidation left a provenance row and no record that a consolidation
/// HAPPENED. One `memory_diffs` row is written **only when this call actually
/// inserted something**, with the idempotency key and both counts in the reason
/// (`consolidate key=<64hex> sources=<given> inserted=<n>`). The consequence is
/// measurable and is the reading the ruling asks for: the FIRST call adds 1 audit
/// row, a repeat adds 0 — the audit is exactly as idempotent as `memory_sources`.
pub async fn record_consolidation(db: &Db, w: &ConsolidationWrite) -> Result<usize, DbError> {
    let memory_id = w.memory_id;
    let rows: Vec<(String, i64)> = w
        .sources
        .iter()
        .map(|(k, id)| (k.as_str().to_string(), *id))
        .collect();
    let now = Utc::now().to_rfc3339();
    let n = db
        .call(move |conn| -> Result<usize, rusqlite::Error> {
            let mut n = 0usize;
            for (kind, source_id) in &rows {
                n += conn.execute(
                    "INSERT OR IGNORE INTO memory_sources (memory_id, source_kind, source_id, created_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![memory_id, kind, source_id, now],
                )?;
            }
            Ok(n)
        })
        .await?
        .map_err(DbError::from)?;

    // A no-op re-run must not add an audit row: the audit is a record of an
    // ACTION, and nothing happened.
    if n == 0 {
        return Ok(0);
    }

    // The carrier facts the reason needs (content ⇒ key; store/namespace ⇒ the
    // audit row's own columns). Read here rather than added to
    // `ConsolidationWrite`, because that struct is public and used by callers
    // outside this module.
    let carrier: Option<(String, String, String)> = db
        .call(
            move |conn| -> Result<Option<(String, String, String)>, rusqlite::Error> {
                conn.query_row(
                    "SELECT content, store, namespace FROM memories WHERE id = ?1",
                    [memory_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            },
        )
        .await?
        .map_err(DbError::from)?;
    let (content, store, namespace) = carrier.unwrap_or_else(|| {
        (
            String::new(),
            crate::MemoryStore::Observation.as_str().to_string(),
            crate::Namespace::User.to_string(),
        )
    });
    let key = consolidation_key(&content, &w.sources);
    let reason = format!(
        "consolidate key={key} sources={} inserted={n}",
        w.sources.len()
    );
    crate::write::audit(
        db,
        "consolidate",
        &store,
        &namespace,
        None,
        Some(&format!("id={memory_id}")),
        Some(reason),
    )
    .await?;
    Ok(n)
}

/// What one memory claims as its sources (newest table, stable order).
pub async fn sources_of(db: &Db, memory_id: i64) -> Result<Vec<(String, i64)>, DbError> {
    db.call(move |conn| -> Result<Vec<(String, i64)>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT source_kind, source_id FROM memory_sources
              WHERE memory_id = ?1 ORDER BY source_kind, source_id",
        )?;
        let rows = stmt
            .query_map([memory_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// The reverse direction: which memories claim this source. This is the reading
/// the erasure path needs ("this episode is gone — who was built from it?").
pub async fn derived_from(db: &Db, kind: SourceKind, source_id: i64) -> Result<Vec<i64>, DbError> {
    let kind = kind.as_str();
    db.call(move |conn| -> Result<Vec<i64>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT memory_id FROM memory_sources
              WHERE source_kind = ?1 AND source_id = ?2 ORDER BY memory_id",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![kind, source_id], |r| r.get(0))?
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

    async fn seed(db: &Db, content: &str) -> i64 {
        let w = MemoryWrite {
            store: crate::MemoryStore::Observation,
            namespace: Namespace::parse("user").unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        };
        match write_memory(db, &w).await.unwrap() {
            WriteOutcome::Inserted(id) => id,
            other => panic!("expected Inserted, got {other:?}"),
        }
    }

    /// C4 memory half: a conclusion names its sources, the link is idempotent, and
    /// the reverse index answers "who was built from this".
    #[tokio::test]
    async fn consolidation_links_are_stored_once_and_queryable_both_ways() {
        let db = Db::open_in_memory().unwrap();
        let a = seed(&db, "fact A").await;
        let b = seed(&db, "fact B").await;
        let conclusion = seed(&db, "conclusion from A and B").await;
        let w = ConsolidationWrite {
            memory_id: conclusion,
            sources: vec![(SourceKind::Memory, a), (SourceKind::Memory, b)],
        };
        let first = record_consolidation(&db, &w).await.unwrap();
        let second = record_consolidation(&db, &w).await.unwrap();
        println!("READING C4 first={first} second={second}");
        assert_eq!(first, 2);
        assert_eq!(second, 0, "re-running must add nothing");
        // C4 / RV-B-2: the ACTION is audited too, and the audit is exactly as
        // idempotent as the source table — one row for the first call, none after.
        let audit_rows = |db: &Db| {
            let d = db.clone();
            async move {
                d.call(
                    |conn| -> Result<Vec<(String, Option<String>)>, rusqlite::Error> {
                        let mut stmt = conn.prepare(
                        "SELECT op, reason FROM memory_diffs WHERE op = 'consolidate' ORDER BY id",
                    )?;
                        let rows = stmt
                            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(rows)
                    },
                )
                .await
                .unwrap()
                .unwrap()
            }
        };
        let after_first = audit_rows(&db).await;
        println!("READING C4 audit after first call: {after_first:?}");
        assert_eq!(
            after_first.len(),
            1,
            "the first call records exactly one row"
        );
        let reason = after_first[0].1.clone().unwrap_or_default();
        assert!(reason.starts_with("consolidate key="), "{reason}");
        assert!(reason.contains("sources=2"), "{reason}");
        assert!(reason.contains("inserted=2"), "{reason}");
        let expected_key = consolidation_key("conclusion from A and B", &w.sources);
        assert!(
            reason.contains(&expected_key),
            "the reason must carry the idempotency key: {reason}"
        );
        // The reading the ruling asks for: the repeat adds ZERO audit rows.
        let _ = record_consolidation(&db, &w).await.unwrap();
        let after_second = audit_rows(&db).await;
        println!(
            "READING C4 audit rows: first call={} after a repeat={}",
            after_first.len(),
            after_second.len()
        );
        assert_eq!(
            after_second.len(),
            1,
            "a no-op re-run must not add an audit row"
        );
        assert_eq!(
            sources_of(&db, conclusion).await.unwrap(),
            vec![("memory".to_string(), a), ("memory".to_string(), b)]
        );
        assert_eq!(
            derived_from(&db, SourceKind::Memory, a).await.unwrap(),
            vec![conclusion]
        );
    }

    /// The key is order-insensitive but content-sensitive: the same conclusion
    /// over the same sources is one conclusion; a different one is not.
    #[test]
    fn the_key_canonicalises_the_source_set() {
        let k1 = consolidation_key("x", &[(SourceKind::Memory, 2), (SourceKind::Episode, 1)]);
        let k2 = consolidation_key("x", &[(SourceKind::Episode, 1), (SourceKind::Memory, 2)]);
        let k3 = consolidation_key("y", &[(SourceKind::Episode, 1), (SourceKind::Memory, 2)]);
        println!(
            "READING C4 key(k1)={k1:.16} k1==k2 {} k1==k3 {}",
            k1 == k2,
            k1 == k3
        );
        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
        assert_eq!(k1.len(), 64);
    }
}
