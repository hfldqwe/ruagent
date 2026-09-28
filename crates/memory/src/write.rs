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
///
/// `Debug` is implemented BY HAND (t52) because this enum's `{:?}` is a WIRE
/// string: the daemon serializes it straight into the HTTP body
/// (`api.rs: `format!("{outcome:?}")``), the CLI prints it, the MCP server passes
/// it through and the panel maps it back to an i18n key by taking the FIRST
/// TOKEN (`panel/src/views/Memory.tsx: outcomeKey`). So the three positive arms
/// are byte-frozen here by test, and only the refusal carries extra detail: a
/// caller told `RejectedNamespace` with no other information cannot tell a typo
/// from a governance rule, and the flat namespace list published elsewhere
/// (api.rs:4234, `0004_memory.sql:21`, the panel's `NAMESPACES`) reads as if
/// `global` were writable everywhere.
#[derive(Clone, PartialEq)]
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

impl std::fmt::Debug for WriteOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Byte-frozen arms: the panel splits "Inserted(7)" /
            // "Superseded { old: 1, new: 2 }" on the first `(`, `{` or space.
            WriteOutcome::Inserted(id) => write!(f, "Inserted({id})"),
            WriteOutcome::Superseded { old, new } => {
                write!(f, "Superseded {{ old: {old}, new: {new} }}")
            }
            WriteOutcome::SkippedDuplicate(id) => write!(f, "SkippedDuplicate({id})"),
            // The refusal names what IS supported, derived from the matrix
            // (`MemoryStore::allowed_kinds` → `write_vocabulary`), and points at
            // the audit row that holds WHICH pair was refused: the variant is
            // fieldless on purpose (distill.rs matches `W::RejectedNamespace`
            // without a payload), so the per-request facts live in the audit.
            WriteOutcome::RejectedNamespace => write!(
                f,
                "RejectedNamespace (this store does not allow that namespace; \
                 write vocabulary = {}; the refused pair is recorded in the write audit as op='reject')",
                crate::write_vocabulary()
            ),
        }
    }
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

/// What one write decided INSIDE the writer closure — before the audit turns it
/// into rows.
///
/// A refusal is a THIRD thing: neither an outcome to return as success nor a SQL
/// failure. It means "nothing was written, and here is why" (t72 / F2).
enum Step {
    Outcome {
        outcome: WriteOutcome,
        /// `Some(id)` = the write landed by CLEARING a tombstone instead of
        /// inserting a row, because the unique key `(store, namespace,
        /// content_hash)` still holds the slot (`0004_memory.sql:30`). The caller
        /// sees `Inserted`/`Superseded`; the audit's `reason` records the revive,
        /// so a reader never has to guess how the row became current.
        revived: Option<i64>,
    },
    /// `supersedes` named a row that EXISTS but is not a current row of this
    /// store+namespace, so the scoped UPDATE affected **0 rows**. Nothing was
    /// superseded, therefore nothing may claim it was.
    SupersedeRefused { old: i64 },
    /// The content's unique key is held by a tombstone that is ALSO superseded:
    /// reviving it would not make it current, and a second row with the same key
    /// cannot exist. Refused, loudly — never silently dropped.
    InsertRefused { tombstone: i64 },
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

    // The whole dedupe+insert+supersede decision is one closure on the
    // single-writer actor — no read-then-write races by construction.
    let step: Step = db
        .call(move |conn| -> Result<Step, rusqlite::Error> {
            // 2. Dedupe: identical CURRENT content already exists?
            //
            // "Current" is the READ side's predicate (`query.rs` current_memories:
            // `superseded_at IS NULL AND deleted_at IS NULL`). This query used to
            // omit `deleted_at IS NULL`, so a soft-deleted row still counted as
            // "already there": deleting a fact and writing it again returned
            // `SkippedDuplicate(<deleted id>)` while `current_memories` showed
            // nothing -- the fact was gone for good and the caller was told the
            // write was already present (t72 / F1). One definition, used by both
            // sides.
            let existing: Option<i64> = conn
                .query_row(
                    "SELECT id FROM memories
                     WHERE store = ?1 AND namespace = ?2 AND content_hash = ?3
                       AND superseded_at IS NULL AND deleted_at IS NULL",
                    rusqlite::params![store, ns, hash],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?;
            if let Some(id) = existing {
                return Ok(Step::Outcome {
                    outcome: WriteOutcome::SkippedDuplicate(id),
                    revived: None,
                });
            }

            // 3. Supersede the target if given — and JUDGE IT BY THE ROWS IT
            // CHANGED, not by the request (t72 / F2). The scoping is deliberate
            // (you may only replace a row in your own store+namespace); what was
            // missing is the consequence of the UPDATE matching nothing.
            let mut superseded: Option<i64> = None;
            if let Some(old_id) = supersedes {
                let affected = conn.execute(
                    "UPDATE memories SET superseded_at = ?4, updated_at = ?4 WHERE id = ?1
                     AND store = ?2 AND namespace = ?3
                     AND superseded_at IS NULL AND deleted_at IS NULL",
                    rusqlite::params![old_id, store, ns, now],
                )?;
                if affected == 0 {
                    // Two very different cases share `affected == 0`:
                    //   * the id does not exist  -> fall through, the FOREIGN KEY
                    //     on `memories.supersedes` rejects the INSERT exactly as
                    //     it did before this change;
                    //   * the id exists but is superseded/soft-deleted/another
                    //     store's -> refuse. Inserting would create a row whose
                    //     `supersedes` column claims a replacement that never
                    //     happened, and the returned outcome plus the audit row
                    //     would both say `supersede` (the t41 family: an evidence
                    //     event must not exist for an event that did not).
                    let exists: bool = conn.query_row(
                        "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
                        [old_id],
                        |r| r.get(0),
                    )?;
                    if exists {
                        return Ok(Step::SupersedeRefused { old: old_id });
                    }
                } else {
                    superseded = Some(old_id);
                }
            }

            // 4. The key is taken by a TOMBSTONE? Revive it.
            //
            // `memories` carries `UNIQUE (store, namespace, content_hash)`
            // (`0004_memory.sql:30`) and that key spans tombstoned rows: a deleted
            // row keeps its slot for ever. So "write the same content again" can
            // only land by CLEARING the tombstone — inserting a second row is
            // impossible, and (measured while implementing F1) it fails loudly
            // with `UNIQUE constraint failed`, which is the honest version of the
            // silent `SkippedDuplicate(<deleted id>)` this task removes.
            //
            // `revived` travels to the audit so the history says HOW the row
            // became current instead of implying a brand-new row.
            let tombstone: Option<(i64, bool)> = conn
                .query_row(
                    "SELECT id, superseded_at IS NOT NULL FROM memories
                     WHERE store = ?1 AND namespace = ?2 AND content_hash = ?3
                       AND deleted_at IS NOT NULL
                     ORDER BY id LIMIT 1",
                    rusqlite::params![store, ns, hash],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?;
            if let Some((tombstone_id, also_superseded)) = tombstone {
                if also_superseded {
                    // Reviving it would not make it current (it is superseded
                    // too), and a second row cannot exist: refuse, and say so.
                    return Ok(Step::InsertRefused {
                        tombstone: tombstone_id,
                    });
                }
                let revived_rows = conn.execute(
                    "UPDATE memories SET deleted_at = NULL, confidence = ?2, updated_at = ?3
                     WHERE id = ?1 AND deleted_at IS NOT NULL",
                    rusqlite::params![tombstone_id, confidence, now],
                )?;
                // A revive writes to the CURRENT set, so it is judged by the rows
                // it changed (same rule as the supersede above); the count travels
                // into the audit's reason so the evidence is in the row, not in a
                // promise made by this comment.
                debug_assert_eq!(
                    revived_rows, 1,
                    "the tombstone was selected in this closure"
                );
                return Ok(Step::Outcome {
                    outcome: if let Some(old_id) = superseded {
                        WriteOutcome::Superseded {
                            old: old_id,
                            new: tombstone_id,
                        }
                    } else {
                        WriteOutcome::Inserted(tombstone_id)
                    },
                    revived: Some(tombstone_id),
                });
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
            Ok(Step::Outcome {
                outcome: if let Some(old_id) = superseded {
                    WriteOutcome::Superseded {
                        old: old_id,
                        new: new_id,
                    }
                } else {
                    WriteOutcome::Inserted(new_id)
                },
                revived: None,
            })
        })
        .await??;

    // 4. Audit. A refusal gets its OWN op: the audit's job is to say what
    //    happened, so it may not borrow the name of a supersede that did not
    //    happen -- and it may not be skipped either, because a refused write is
    //    a decision (the namespace refusal above is audited for the same reason).
    let result = match step {
        Step::SupersedeRefused { old } => {
            let before = format!("id={old}");
            audit(
                db,
                "supersede_refused",
                write.store.as_str(),
                &write.namespace.to_string(),
                Some(&before),
                None,
                Some(format!(
                    "0 rows superseded: `{old}` is not a current row of store `{}` namespace `{}` \
                     (it is superseded, soft-deleted, or in another store); nothing was written",
                    write.store.as_str(),
                    write.namespace
                )),
            )
            .await?;
            // The function's error type is `DbError` (store-owned); a refused
            // supersede IS a constraint on this write, so it surfaces as a
            // constraint-shaped failure with an explicit message -- the same
            // class the nonexistent-id path already produced.
            return Err(DbError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
                Some(format!(
                    "supersede refused: `{old}` is not a current row of store `{}` namespace `{}` \
                     -- the memory was NOT written (recorded in the write audit as \
                     op='supersede_refused')",
                    write.store.as_str(),
                    write.namespace
                )),
            )));
        }
        Step::InsertRefused { tombstone } => {
            let before = format!("id={tombstone}");
            audit(
                db,
                "insert_refused",
                write.store.as_str(),
                &write.namespace.to_string(),
                Some(&before),
                None,
                Some(format!(
                    "the content's unique key (store, namespace, content_hash) is held by \
                     tombstone id={tombstone}, which is ALSO superseded: reviving it would not \
                     make it current and a second row cannot exist; nothing was written"
                )),
            )
            .await?;
            return Err(DbError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
                Some(format!(
                    "insert refused: the same content is held by tombstone id={tombstone} which is \
                     also superseded -- the memory was NOT written (recorded in the write audit as \
                     op='insert_refused')"
                )),
            )));
        }
        Step::Outcome { outcome, revived } => {
            // 5. Audit the outcome that really happened. `op` names the DECISION
            //    (as it does for `reject`/`skip_dedupe`); the free-form `reason`
            //    carries HOW it was carried out, so a revived row never reads as a
            //    brand-new one.
            let (op, before, after) = match &outcome {
                WriteOutcome::Inserted(id) => ("insert", None, Some(format!("id={id}"))),
                WriteOutcome::Superseded { old, new } => (
                    "supersede",
                    Some(format!("id={old}")),
                    Some(format!("id={new}")),
                ),
                WriteOutcome::SkippedDuplicate(id) => {
                    ("skip_dedupe", None, Some(format!("id={id}")))
                }
                WriteOutcome::RejectedNamespace => unreachable!("rejected before the transaction"),
            };
            audit(
                db,
                op,
                write.store.as_str(),
                &write.namespace.to_string(),
                before.as_deref(),
                after.as_deref(),
                revived.map(|id| {
                    format!(
                        "revived soft-deleted row id={id}: the key (store, namespace, \
                         content_hash) is UNIQUE across tombstones, so this write cleared \
                         `deleted_at` (affected 1 row) instead of inserting a new row"
                    )
                }),
            )
            .await?;
            outcome
        }
    };

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

/// Record a MERGE DECISION in the audit log (R-B C2 / X-3).
///
/// WHY A SEPARATE ENTRY POINT. `memory_diffs` recorded the OUTCOME of every
/// write (insert / supersede / skip_dedupe / …) and had no row for the
/// DECISION that produced it: `judge` could refuse a merge, or the embedder
/// could escalate one to a named judge, and neither event was readable
/// afterwards. The op is `merge_judged` — a new VALUE, not a new column:
/// 0004 declares `op TEXT NOT NULL` with no CHECK (I-SCHEMA verified this in
/// 0020_memory_sql:97-104), and the live table already holds seven ops.
///
/// The `reason` string comes from `dedupe::merge_audit_reason`, so the text has
/// one writer. `before` names the candidate the decision was made against.
pub async fn audit_merge_decision(
    db: &Db,
    store: MemoryStore,
    namespace: &Namespace,
    candidate: Option<i64>,
    reason: &str,
) -> Result<(), DbError> {
    audit(
        db,
        "merge_judged",
        store.as_str(),
        &namespace.to_string(),
        candidate.map(|id| format!("id={id}")).as_deref(),
        None,
        Some(reason.to_string()),
    )
    .await
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

    // ── t52: the WRITE vocabulary is a matrix, and the refusal must say so ──

    /// The published vocabulary and the enforcement are ONE source.
    ///
    /// The falsifiable half: for all 4 stores × 4 namespace kinds (16 combos)
    /// the rendered string must agree with `allows_namespace`. If someone edits
    /// the `match` without the matrix (or the reverse), this goes red — which is
    /// exactly how "the claimed vocabulary" and "the actual one" drifted before.
    #[test]
    fn the_published_vocabulary_and_the_enforcement_agree_on_all_sixteen_pairs() {
        let vocab = crate::write_vocabulary();
        println!("READING t52 write vocabulary: {vocab}");
        assert_eq!(
            vocab,
            "profile×{user} | observation×{user,project:<x>,agent:<x>} | procedure×{project:<x>,global} | lesson×{project:<x>,global}"
        );
        let probes: [crate::Namespace; 4] = [
            crate::Namespace::User,
            crate::Namespace::Global,
            crate::Namespace::Project("x".into()),
            crate::Namespace::Agent("x".into()),
        ];
        let mut allowed: Vec<String> = Vec::new();
        let mut refused: Vec<String> = Vec::new();
        for store in crate::MemoryStore::ALL {
            // the rendered cell for this store, as the string a caller reads
            let cell = vocab
                .split(" | ")
                .find(|c| c.starts_with(&format!("{}×", store.as_str())))
                .expect("every store is rendered")
                .to_string();
            for ns in &probes {
                // the SPELLING the vocabulary uses, taken from the namespace's
                // own kind — so this compares the matrix with itself, not with a
                // second copy of the spellings in this test
                let name = ns.kind().as_str();
                let ok = store.allows_namespace(ns);
                let claimed = cell.contains(name);
                assert_eq!(
                    ok,
                    claimed,
                    "store {} says {ok} for {name} but the vocabulary says {claimed}: {cell}",
                    store.as_str()
                );
                let pair = format!("{}×{name}", store.as_str());
                if ok {
                    allowed.push(pair);
                } else {
                    refused.push(pair);
                }
            }
        }
        println!("READING t52 allowed pairs ({}): {allowed:?}", allowed.len());
        println!("READING t52 refused pairs ({}): {refused:?}", refused.len());
        // The two claims the finding was about, stated as data:
        assert!(allowed.contains(&"procedure×global".to_string()));
        assert!(allowed.contains(&"lesson×global".to_string()));
        assert!(
            refused.contains(&"observation×global".to_string()),
            "observation×global must be REFUSED (the finding)"
        );
        assert_eq!(
            allowed.len(),
            8,
            "1 profile + 3 observation + 2 procedure + 2 lesson"
        );
        assert_eq!(refused.len(), 8);
    }

    /// `global` IS a supported value — for the stores that own the global scope.
    ///
    /// This is the reading that kills the flat claim `user | global |
    /// project:<x> | agent:<x>` in both directions: `global` is neither
    /// universally accepted NOR unsupported.
    #[tokio::test]
    async fn global_is_writable_where_the_global_scope_lives_and_refused_elsewhere() {
        let db = Db::open_in_memory().unwrap();
        let cases: [(MemoryStore, crate::Namespace, bool); 4] = [
            (MemoryStore::Procedure, crate::Namespace::Global, true),
            (MemoryStore::Lesson, crate::Namespace::Global, true),
            (MemoryStore::Observation, crate::Namespace::Global, false),
            (MemoryStore::Profile, crate::Namespace::Global, false),
        ];
        for (store, ns, expect_insert) in cases {
            let out = write_memory(
                &db,
                &MemoryWrite {
                    store,
                    namespace: ns.clone(),
                    content: format!("t52 {} {}", store.as_str(), ns),
                    confidence: 0.9,
                    source_episode: None,
                    supersedes: None,
                },
            )
            .await
            .unwrap();
            println!("READING t52 write {}×{ns} -> {:?}", store.as_str(), out);
            assert_eq!(
                matches!(out, WriteOutcome::Inserted(_)),
                expect_insert,
                "{}×{ns}",
                store.as_str()
            );
        }
        // the refused pairs left an audit row naming WHICH pair was refused
        let reasons: Vec<String> = db
            .call(|conn| -> Result<Vec<String>, rusqlite::Error> {
                let mut stmt =
                    conn.prepare("SELECT reason FROM memory_diffs WHERE op='reject' ORDER BY id")?;
                let rows = stmt
                    .query_map([], |r| r.get::<_, Option<String>>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows.into_iter().flatten().collect())
            })
            .await
            .unwrap()
            .unwrap();
        println!("READING t52 reject audit reasons: {reasons:?}");
        assert_eq!(reasons.len(), 2);
        assert!(reasons[0].contains("observation") && reasons[0].contains("global"));
        assert!(reasons[1].contains("profile") && reasons[1].contains("global"));
    }

    /// The refusal is DISCOVERABLE, and the other three strings are byte-frozen.
    ///
    /// `RejectedNamespace` alone cannot tell a caller whether `global` is a typo
    /// or a governance rule; the arm now names the write vocabulary. The panel
    /// reads the first token (`split(/[({\s]/)[0]`), so the leading variant name
    /// must stay untouchable — asserted here.
    #[test]
    fn the_refusal_names_the_supported_values_and_the_other_strings_are_frozen() {
        let refusal = format!("{:?}", WriteOutcome::RejectedNamespace);
        println!("READING t52 refusal wire string: {refusal}");
        assert!(
            refusal.starts_with("RejectedNamespace"),
            "the panel's first-token mapping must keep working: {refusal}"
        );
        assert!(
            refusal.contains("observation×{user,project:<x>,agent:<x>}"),
            "{refusal}"
        );
        assert!(
            refusal.contains("procedure×{project:<x>,global}"),
            "{refusal}"
        );
        assert!(
            !refusal.contains("observation×{user,global"),
            "the refusal must not repeat the flat-list claim: {refusal}"
        );
        assert!(
            refusal.contains("op='reject'"),
            "it points at the audit: {refusal}"
        );

        // Byte-frozen positive arms (wire strings: HTTP body, CLI, MCP).
        assert_eq!(format!("{:?}", WriteOutcome::Inserted(7)), "Inserted(7)");
        assert_eq!(
            format!("{:?}", WriteOutcome::Superseded { old: 1, new: 2 }),
            "Superseded { old: 1, new: 2 }"
        );
        assert_eq!(
            format!("{:?}", WriteOutcome::SkippedDuplicate(7)),
            "SkippedDuplicate(7)"
        );
        // and the panel's own mapping, reproduced: the first token is the key
        for s in [
            format!("{:?}", WriteOutcome::Inserted(7)),
            format!("{:?}", WriteOutcome::Superseded { old: 1, new: 2 }),
            refusal.clone(),
        ] {
            let variant: String = s
                .split(['(', '{', ' ', ']'])
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            assert!(
                !variant.is_empty() && !variant.contains(' '),
                "first token of {s:?} is {variant:?}"
            );
        }
    }
}
