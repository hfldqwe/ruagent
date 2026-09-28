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
    /// What the purge reads about the doomed row: `(store, namespace, deleted_at,
    /// rows_that_point_at_it)`. Named instead of written out three times, which
    /// is also what clippy's `type_complexity` was pointing at.
    type Doomed = (String, String, Option<String>, usize);
    let found: Option<Doomed> = db
        .call(move |conn| -> Result<Option<Doomed>, rusqlite::Error> {
            let mut stmt =
                conn.prepare("SELECT store, namespace, deleted_at FROM memories WHERE id = ?1")?;
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
        })
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
pub const LEGACY_EPISODE_TEXT: &str = "legacy distilled memories: the [distilled] body prefix was removed by the t347 migration;      the session these came from was not recorded when they were written";
pub const LEGACY_EPISODE_KEY: &str = "ruagent:legacy-distilled-prefix";

// ── Erasure, made answerable (R-B C5 / E.5) ────────────────────────────────
//
// WHAT A PURGE COULD NOT ANSWER BEFORE. `purge_memory` really deletes the row and
// the FTS trigger follows it, so the memory-side disappearance was already true.
// What nobody could answer was the OTHER direction: "that content you asked me to
// forget — where else does it still live?" The raw transcript is in
// `episodes.content`, the same text may have been indexed in the knowledge base,
// and a wiki page may carry it. Reporting that is `forget_report`.
//
// THREE STATES, NOT A BOOLEAN (the design that recall and wiki both asked for):
// a surface with 0 hits, a surface that was never asked, and a surface whose
// owner promised a query that does not exist yet are three different facts, and
// collapsing them is how "nobody looked" is rendered as "nothing there".

/// One derivation surface. The knowledge and wiki entries are the shapes the two
/// sibling specs (R-A A.8, R-D A.2) named; this crate never queries them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidualSurface {
    /// `memories` rows, ANY state: superseded and tombstoned rows still hold text.
    Memories,
    /// The `memories_fts` index (trigger-maintained; a purge follows it).
    MemoriesFts,
    /// Raw transcripts in `episodes.content`.
    Episodes,
    /// Which memories claim a source (`memory_sources`), i.e. derived conclusions.
    Derived,
    /// External surfaces, reported by their owners; never queried from here.
    KnowledgeFile,
    KnowledgeDocument,
    KnowledgeChunk,
    KnowledgeChunkFts,
    KnowledgeVectors,
    WikiChunk,
    WikiPageHash,
    WikiPlan,
    WikiBuildPage,
    /// A file on disk that holds memory bytes IN THE CLEAR: the `data/backups/*`
    /// dumps `strip_distilled_prefix` writes before it rewrites rows, and whole
    /// database copies named `data/*.before-*.db`.
    ///
    /// WHY THIS IS A FORGETTING SURFACE AT ALL (t75 / A-1): the other twelve are
    /// the live database and its derivations. A backup is neither — it is the
    /// SAME BYTES, kept outside every surface the database can answer about, so a
    /// forget that cleaned the live rows and left the dump would read as
    /// "provable" while the plaintext is still on the disk. Measured on the
    /// machine this was written on: the live `memories` table held 0 rows with the
    /// `[distilled]` marker and 0 tombstones, while
    /// `data/backups/memories-distilled-prefix-20260926T210910Z.txt`
    /// (35843 B) still carried the pre-cleanup bytes in the clear.
    BackupFile,
}

/// Where a hit lives: DB, disk, or both. The `Db|File|Both` value set is the one
/// `ResidualOrigin` in `crates/knowledge` uses (recall's spec D.2); the two types
/// are deliberately separate (this crate does not depend on that one) and a drift
/// check over the value set is registered as I-A's t7 test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidualOrigin {
    Db,
    File,
    Both,
}

/// One residual hit: an IDENTIFIER, never the forgotten text.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ResidualHit {
    pub surface: ResidualSurface,
    pub key: String,
    pub source: &'static str,
    pub origin: ResidualOrigin,
    pub path: Option<String>,
}

/// A count with its truncation and its total kept apart (R-B C5-D, semantics 4):
/// `total: None` means "not measured", NOT zero.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ResidualCount {
    pub hits: usize,
    pub truncated: bool,
    pub total: Option<usize>,
}

/// A surface's answer: a reading, or a reason it could not be taken.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResidualStatus {
    Readout(ResidualCount),
    NotAvailable(&'static str),
}

/// One surface this crate can answer itself.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LocalResidual {
    pub surface: ResidualSurface,
    pub status: ResidualStatus,
    pub sample: Vec<ResidualHit>,
    pub source: &'static str,
}

/// A surface another owner must answer. Present as a REQUEST, never as a reading
/// this crate invented (§7.130).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ExternalResidual {
    pub surface: ResidualSurface,
    pub owner: &'static str,
    pub status: ResidualStatus,
    pub source: &'static str,
}

/// The whole answer for one content hash.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForgetReport {
    /// The object asked about: a hash, never the content.
    pub content_hash: String,
    pub local: Vec<LocalResidual>,
    pub external: Vec<ExternalResidual>,
}

/// How many identifiers a local sample carries. ONE place (the report's readers
/// use the same bound the report used).
pub const RESIDUAL_SAMPLE_LIMIT: u32 = 20;

/// Everything the memory side can say about one content hash.
///
/// READ-ONLY: it deletes nothing, calls nothing external, and writes no audit
/// row. The `external` entries are the REQUEST shapes for the knowledge and wiki
/// surfaces, with their owners' promises recorded as `NotAvailable` until those
/// queries exist (I-A: t7+, `residual_scan`; I-D: t10).
///
/// NO ROOT ⇒ the backup surface is `NotAvailable`, never absent (t75): this
/// overload keeps its old signature, so callers that cannot name a filesystem
/// root still get the surface NAMED with the reason it could not be read.
pub async fn forget_report(db: &Db, content_hash: &str) -> Result<ForgetReport, DbError> {
    forget_report_at(db, None, content_hash).await
}

/// `forget_report` for a caller that knows the ruagent root, which is what the
/// backup surface needs: `data/backups/*` and `data/*.before-*.db` live under it
/// (t75 / A-1). Passing `None` is legal and means "nobody told me where the files
/// are" — the same three-state discipline as the rest of this report.
pub async fn forget_report_at(
    db: &Db,
    root: Option<&std::path::Path>,
    content_hash: &str,
) -> Result<ForgetReport, DbError> {
    let hash = content_hash.to_string();
    let rows: Vec<(i64, String)> = db
        .call(move |conn| -> Result<Vec<(i64, String)>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT id, namespace FROM memories WHERE content_hash = ?1 ORDER BY id",
            )?;
            let rows = stmt
                .query_map([hash], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await??;

    let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
    let total = ids.len();
    let truncated = total > RESIDUAL_SAMPLE_LIMIT as usize;
    let count = ResidualCount {
        hits: total,
        truncated,
        total: Some(total),
    };
    let sample: Vec<ResidualHit> = rows
        .iter()
        .take(RESIDUAL_SAMPLE_LIMIT as usize)
        .map(|(id, ns)| ResidualHit {
            surface: ResidualSurface::Memories,
            key: format!("memory/{id} ({ns})"),
            source: "memories.content_hash",
            origin: ResidualOrigin::Db,
            path: None,
        })
        .collect();

    // The FTS index is external-content, keyed by rowid: ask about the ids we
    // found rather than about the text (this crate never needs the text).
    let fts = if ids.is_empty() {
        0usize
    } else {
        let probe = ids.clone();
        db.call(move |conn| -> Result<usize, rusqlite::Error> {
            let mut n = 0usize;
            for id in &probe {
                n += conn.query_row(
                    "SELECT COUNT(*) FROM memories_fts WHERE rowid = ?1",
                    [id],
                    |r| r.get::<_, i64>(0),
                )? as usize;
            }
            Ok(n)
        })
        .await??
    };

    // Derived conclusions: which OTHER memories claim one of these as a source.
    let derived: Vec<i64> = if ids.is_empty() {
        Vec::new()
    } else {
        let probe = ids.clone();
        db.call(move |conn| -> Result<Vec<i64>, rusqlite::Error> {
            let mut out: Vec<i64> = Vec::new();
            for id in &probe {
                let mut stmt = conn.prepare(
                    "SELECT memory_id FROM memory_sources
                      WHERE source_kind = 'memory' AND source_id = ?1 ORDER BY memory_id",
                )?;
                let rows = stmt.query_map([id], |r| r.get::<_, i64>(0))?;
                for r in rows {
                    let v = r?;
                    if !out.contains(&v) {
                        out.push(v);
                    }
                }
            }
            Ok(out)
        })
        .await??
    };

    let local = vec![
        LocalResidual {
            surface: ResidualSurface::Memories,
            status: ResidualStatus::Readout(count),
            sample,
            source: "SELECT id, namespace FROM memories WHERE content_hash = ?1 (any state)",
        },
        LocalResidual {
            surface: ResidualSurface::MemoriesFts,
            status: ResidualStatus::Readout(ResidualCount {
                hits: fts,
                truncated: false,
                total: Some(fts),
            }),
            sample: Vec::new(),
            source: "SELECT COUNT(*) FROM memories_fts WHERE rowid = ?1",
        },
        LocalResidual {
            surface: ResidualSurface::Derived,
            status: ResidualStatus::Readout(ResidualCount {
                hits: derived.len(),
                truncated: false,
                total: Some(derived.len()),
            }),
            sample: derived
                .iter()
                .map(|id| ResidualHit {
                    surface: ResidualSurface::Derived,
                    key: format!("memory/{id}"),
                    source: "memory_sources(source_kind='memory')",
                    origin: ResidualOrigin::Db,
                    path: None,
                })
                .collect(),
            source: "SELECT memory_id FROM memory_sources WHERE source_kind='memory' AND source_id=?1",
        },
        LocalResidual {
            surface: ResidualSurface::Episodes,
            status: ResidualStatus::NotAvailable(
                "episodes has no column linking a memory's hash to a transcript; \
                 only an episode-id lookup is possible (R-B U-1, waits on I-SCHEMA)",
            ),
            sample: Vec::new(),
            source: "n/a",
        },
        // The backup surface (t75 / A-1). Answered by READING FILES, which is why
        // it needs a root: no SQL here can see `data/backups/*`.
        backup_surface(root, content_hash),
    ];

    let external = vec![
        ExternalResidual {
            surface: ResidualSurface::KnowledgeFile,
            owner: "I-A/recall",
            status: ResidualStatus::NotAvailable(
                "I-A promised residual_scan after t7; the raw SQL exists but is private",
            ),
            source: "gen2-recall-spec D.2 residual_scan / document_path",
        },
        ExternalResidual {
            surface: ResidualSurface::KnowledgeDocument,
            owner: "I-A/recall",
            status: ResidualStatus::NotAvailable("waits on I-A t7 (residual_scan)"),
            source: "gen2-recall-spec D.2",
        },
        ExternalResidual {
            surface: ResidualSurface::KnowledgeChunk,
            owner: "I-A/recall",
            status: ResidualStatus::NotAvailable("waits on I-A t7 (residual_scan)"),
            source: "gen2-recall-spec D.2",
        },
        ExternalResidual {
            surface: ResidualSurface::WikiChunk,
            owner: "I-D/wiki",
            status: ResidualStatus::NotAvailable("waits on I-D t10"),
            source: "gen2-wiki-spec A.2 (three read-only legs)",
        },
        ExternalResidual {
            surface: ResidualSurface::WikiPageHash,
            owner: "I-D/wiki",
            status: ResidualStatus::NotAvailable(
                "hash-only: answers no substring question, so it is not a content reading",
            ),
            source: "gen2-wiki-spec A.2",
        },
        ExternalResidual {
            surface: ResidualSurface::WikiPlan,
            owner: "I-D/wiki",
            status: ResidualStatus::NotAvailable("waits on I-D t10"),
            source: "gen2-wiki-spec A.2",
        },
        // The OTHER backup class (t75): the wiki keeps its own pre-write copies
        // under `wiki-backups/{slug}.md`, written by the daemon. This crate cannot
        // reach them (different root, different owner), but "a plaintext copy of
        // forgotten bytes" is the same surface, so it is REQUESTED here rather
        // than left invisible.
        ExternalResidual {
            surface: ResidualSurface::BackupFile,
            owner: "daemon/wiki",
            status: ResidualStatus::NotAvailable(
                "wiki backups are `wiki-backups/{slug}.md` under the daemon's wiki root: \
                 same surface, different owner; no query exists from here",
            ),
            source: "crates/daemon/src/wiki.rs:2317-2323",
        },
    ];

    Ok(ForgetReport {
        content_hash: content_hash.to_string(),
        local,
        external,
    })
}

/// One candidate file the backup scan examined.
struct BackupFileReading {
    name: String,
    bytes: u64,
    /// `Some(n)` = checked, and it holds `n` records with this hash; `None` = the
    /// file could not be checked, with the reason.
    verdict: Result<usize, String>,
}

/// What one backup scan found. `inventory` is evidence (printed by tests and
/// probes): the API carries `hits` plus, for unreadable candidates, one sample
/// entry whose key says why.
struct BackupScan {
    hits: Vec<ResidualHit>,
    inventory: Vec<String>,
    unreadable: usize,
}

/// The `data/backups/*` + `data/*.before-*.db` half of a forget report.
///
/// READ-ONLY: `read_dir`, `metadata`, `read`, and a SQLite connection opened with
/// `SQLITE_OPEN_READ_ONLY`. Nothing here creates, moves or deletes a file.
fn backup_surface(root: Option<&std::path::Path>, content_hash: &str) -> LocalResidual {
    let Some(root) = root else {
        return LocalResidual {
            surface: ResidualSurface::BackupFile,
            status: ResidualStatus::NotAvailable(
                "no filesystem root was given: `data/backups/*` and `data/*.before-*.db` cannot \
                 be reached by SQL; call forget_report_at(db, Some(root), hash)",
            ),
            sample: Vec::new(),
            source: "n/a (no root supplied)",
        };
    };
    let scan = scan_backups(root, content_hash);
    // The per-file inventory (name, size, verdict) is evidence, not a per-hash
    // reading, so it goes to the log where an operator can see WHICH files were
    // examined and which were unreadable — the same text the tests print.
    if !scan.inventory.is_empty() {
        tracing::debug!(
            target: "ruagent_memory::forget",
            files = scan.inventory.len(),
            inventory = ?scan.inventory,
            "backup surface inventory (read-only)"
        );
    }
    let hits = scan.hits.len();
    // `total: None` = "not fully measured" (the crate's existing three-state
    // convention), and it is set when a candidate could not be read: the byte
    // may be there and this report cannot say. Confirmed carriers stay in `hits`.
    let total = if scan.unreadable == 0 {
        Some(hits)
    } else {
        None
    };
    LocalResidual {
        surface: ResidualSurface::BackupFile,
        status: ResidualStatus::Readout(ResidualCount {
            hits,
            truncated: false,
            total,
        }),
        sample: scan.hits,
        source: if scan.unreadable == 0 {
            "read_dir + read (dump) / SQLITE_OPEN_READ_ONLY (db copy), root known"
        } else {
            "read_dir + read (dump) / SQLITE_OPEN_READ_ONLY (db copy); \
             at least one candidate unreadable, so the total is not measured"
        },
    }
}

/// Every backup-class candidate under `root`, with a per-hash verdict.
fn scan_backups(root: &std::path::Path, content_hash: &str) -> BackupScan {
    let data = root.join("data");
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    push_files(&data.join("backups"), &mut candidates);
    if let Ok(dir) = std::fs::read_dir(&data) {
        for entry in dir.flatten() {
            let path = entry.path();
            let name = file_name_of(&path);
            // Whole-database copies: `data/*.before-*` — note the name does NOT
            // have to END in `.db` (the machine this was written on has
            // `ruagent.db.before-t229-cleanup-20260926-150802`), so the file KIND
            // is decided by sniffing its bytes, not by its extension.
            if path.is_file() && name.contains(".before-") {
                candidates.push(path);
            }
        }
    }
    candidates.sort();

    let mut hits: Vec<ResidualHit> = Vec::new();
    let mut inventory: Vec<String> = Vec::new();
    let mut unreadable = 0usize;
    for path in candidates {
        let reading = read_backup_candidate(&path, content_hash);
        let path_text = path.to_string_lossy().to_string();
        match &reading.verdict {
            Ok(n) => {
                if *n > 0 {
                    hits.push(ResidualHit {
                        surface: ResidualSurface::BackupFile,
                        key: reading.name.clone(),
                        source: "backup file holding this content_hash",
                        origin: ResidualOrigin::File,
                        path: Some(path_text.clone()),
                    });
                    inventory.push(format!(
                        "{}: {} B, {} record(s) with this hash",
                        reading.name, reading.bytes, n
                    ));
                } else {
                    inventory.push(format!(
                        "{}: {} B, checked, this hash is NOT in it",
                        reading.name, reading.bytes
                    ));
                }
            }
            Err(reason) => {
                unreadable += 1;
                // The candidate is visible in the sample so a reader can see WHICH
                // file is unmeasured; it is not counted as a hit.
                hits.push(ResidualHit {
                    surface: ResidualSurface::BackupFile,
                    key: format!("{} (could not be checked: {reason})", reading.name),
                    source: "backup file that could not be read",
                    origin: ResidualOrigin::File,
                    path: Some(path_text.clone()),
                });
                inventory.push(format!(
                    "{}: {} B, COULD NOT BE CHECKED ({reason})",
                    reading.name, reading.bytes
                ));
            }
        }
    }
    BackupScan {
        hits,
        inventory,
        unreadable,
    }
}

fn push_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                out.push(path);
            }
        }
    }
}

fn file_name_of(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Check ONE candidate for this hash, read-only. The KIND of file is decided by
/// its first bytes (a SQLite header vs this crate's length-prefixed dump), not by
/// its name: the machine's own database copy ends in `-150802`, not `.db`.
fn read_backup_candidate(path: &std::path::Path, content_hash: &str) -> BackupFileReading {
    let name = file_name_of(path);
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let verdict = sniff_kind(path).and_then(|kind| match kind {
        BackupKind::Sqlite => check_db_copy(path, content_hash),
        BackupKind::Dump => check_dump(path, content_hash),
        BackupKind::Unknown => {
            Err("neither a SQLite database nor a length-prefixed dump".to_string())
        }
    });
    BackupFileReading {
        name,
        bytes,
        verdict,
    }
}

enum BackupKind {
    Sqlite,
    Dump,
    Unknown,
}

const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

fn sniff_kind(path: &std::path::Path) -> Result<BackupKind, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("cannot open read-only: {e}"))?;
    let mut head = [0u8; 16];
    let n = file
        .read(&mut head)
        .map_err(|e| format!("cannot read the first bytes: {e}"))?;
    if n >= SQLITE_HEADER.len() && &head[..SQLITE_HEADER.len()] == SQLITE_HEADER {
        return Ok(BackupKind::Sqlite);
    }
    // A dump starts with "<id> <byte_len>\n"; anything else is not checkable HERE
    // (it may still hold the bytes — that is what `total: None` says).
    match std::str::from_utf8(&head[..n]) {
        Ok(text) if text.starts_with(|c: char| c.is_ascii_digit()) => Ok(BackupKind::Dump),
        _ => Ok(BackupKind::Unknown),
    }
}

/// A whole-database copy: ask it the same question the live database answers,
/// through a READ-ONLY connection.
fn check_db_copy(path: &std::path::Path, content_hash: &str) -> Result<usize, String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("cannot open read-only: {e}"))?;
    conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE content_hash = ?1",
        [content_hash],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n as usize)
    .map_err(|e| format!("not a memories database: {e}"))
}

/// The dump `strip_distilled_prefix` writes: `"<id> <byte_len>\n"`, then exactly
/// that many bytes of the BEFORE content, then a newline — repeated. The hash is
/// recomputed with the crate's own function, so there is one definition of what a
/// content hash is.
fn check_dump(path: &std::path::Path, content_hash: &str) -> Result<usize, String> {
    let buf = std::fs::read(path).map_err(|e| format!("cannot read: {e}"))?;
    let records = parse_dump(&buf)?;
    let hits = records
        .iter()
        .filter(|(_, content)| crate::write::content_hash(content) == content_hash)
        .count();
    Ok(hits)
}

/// Parse a length-prefixed dump into `(id, content)` pairs. Returns the reason it
/// is not a dump this crate wrote instead of pretending it holds nothing.
fn parse_dump(buf: &[u8]) -> Result<Vec<(i64, String)>, String> {
    let mut out: Vec<(i64, String)> = Vec::new();
    let mut pos = 0usize;
    while pos < buf.len() {
        let header_end = buf[pos..]
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| format!("unterminated record header at byte {pos}"))?;
        let header = std::str::from_utf8(&buf[pos..pos + header_end])
            .map_err(|e| format!("record header is not utf-8: {e}"))?;
        let mut parts = header.split(' ');
        let id: i64 = parts
            .next()
            .ok_or_else(|| format!("empty record header at byte {pos}"))?
            .parse()
            .map_err(|e| format!("record id is not a number: {e}"))?;
        let len: usize = parts
            .next()
            .ok_or_else(|| format!("record header at byte {pos} has no byte length"))?
            .parse()
            .map_err(|e| format!("record byte length is not a number: {e}"))?;
        let start = pos + header_end + 1;
        let end = start
            .checked_add(len)
            .filter(|end| *end <= buf.len())
            .ok_or_else(|| format!("record {id} claims {len} bytes past the end of the file"))?;
        let content = std::str::from_utf8(&buf[start..end])
            .map_err(|e| format!("record {id} is not utf-8: {e}"))?;
        out.push((id, content.to_string()));
        // the record is terminated by a newline (absent only at a truncated tail)
        pos = end + 1;
    }
    Ok(out)
}

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
        .call(
            |conn| -> Result<Vec<(i64, String, Option<i64>)>, rusqlite::Error> {
                let mut stmt =
                    conn.prepare("SELECT id, content, source_episode FROM memories ORDER BY id")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            },
        )
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
            samples.push(PrefixSample {
                id: *id,
                before: before.clone(),
                after: after.clone(),
            });
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
    }
    /// t347 acceptance 2 + 5: the migration strips ONLY the leading marker, and a
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
                conn.query_row(
                    "SELECT kind FROM episodes WHERE id = ?1",
                    [legacy_id],
                    |r| r.get(0),
                )
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kind, "manual", "the migration episode says what it is");

        let b = get_memory(&db, control).await.unwrap().unwrap();
        assert_eq!(b.content, "the deploy script lives in scripts/deploy.sh");
        assert_eq!(
            b.source_episode, None,
            "a row without the marker gains nothing"
        );

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

    // ── C5: erasure, made answerable ───────────────────────────────────────

    /// The report answers the memory side, refuses to invent the other two sides,
    /// and the memory-side answer MOVES when the row is purged. That movement is
    /// the reading: before the purge there is a hit, after there is none, and the
    /// surfaces that cannot be answered say so instead of reporting 0.
    #[tokio::test]
    async fn forget_report_reads_the_memory_side_and_refuses_to_invent_the_rest() {
        let db = Db::open_in_memory().unwrap();
        let content = "the secret is in this sentence";
        let hash = crate::write::content_hash(content);
        let inserted = crate::write::write_memory(
            &db,
            &crate::write::MemoryWrite {
                store: MemoryStore::Observation,
                namespace: Namespace::parse("user").unwrap(),
                content: content.into(),
                confidence: 0.9,
                source_episode: None,
                supersedes: None,
            },
        )
        .await
        .unwrap();
        let crate::write::WriteOutcome::Inserted(id) = inserted else {
            panic!("insert")
        };

        let before = forget_report(&db, &hash).await.unwrap();
        let before_mem = before
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::Memories)
            .unwrap();
        let before_fts = before
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::MemoriesFts)
            .unwrap();
        println!(
            "READING C5 before purge: memories={:?} fts={:?} external={} episodes={:?}",
            before_mem.status,
            before_fts.status,
            before.external.len(),
            before
                .local
                .iter()
                .find(|l| l.surface == ResidualSurface::Episodes)
                .map(|l| l.status.clone())
        );
        assert_eq!(
            before_mem.status,
            ResidualStatus::Readout(ResidualCount {
                hits: 1,
                truncated: false,
                total: Some(1)
            })
        );
        assert_eq!(
            before_fts.status,
            ResidualStatus::Readout(ResidualCount {
                hits: 1,
                truncated: false,
                total: Some(1)
            })
        );
        // The two surfaces this crate cannot answer are NotAvailable, not 0.
        assert!(matches!(
            before
                .local
                .iter()
                .find(|l| l.surface == ResidualSurface::Episodes)
                .unwrap()
                .status,
            ResidualStatus::NotAvailable(_)
        ));
        assert!(
            before
                .external
                .iter()
                .all(|e| matches!(e.status, ResidualStatus::NotAvailable(_))),
            "an external surface must be a request, never an invented reading"
        );
        assert!(
            before
                .external
                .iter()
                .any(|e| e.surface == ResidualSurface::WikiChunk && e.owner == "I-D/wiki")
        );

        // The report never carries the forgotten text.
        let dumped = format!("{before:?}");
        println!("READING C5 report identifiers only: {dumped}");
        assert!(!dumped.contains("the secret is in this sentence"));
        assert!(dumped.contains("memory/"));

        // Purge, then read again: the memory surfaces must fall to zero.
        assert!(matches!(
            purge_memory(&db, id).await.unwrap(),
            PurgeOutcome::Purged { .. }
        ));
        let after = forget_report(&db, &hash).await.unwrap();
        let after_mem = after
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::Memories)
            .unwrap();
        let after_fts = after
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::MemoriesFts)
            .unwrap();
        println!(
            "READING C5 after purge: memories={:?} fts={:?}",
            after_mem.status, after_fts.status
        );
        assert_eq!(
            after_mem.status,
            ResidualStatus::Readout(ResidualCount {
                hits: 0,
                truncated: false,
                total: Some(0)
            })
        );
        assert_eq!(
            after_fts.status,
            ResidualStatus::Readout(ResidualCount {
                hits: 0,
                truncated: false,
                total: Some(0)
            })
        );
    }

    /// The derived-conclusion surface is a real reading: a memory that claims a
    /// source shows up when that source is asked about (the `memory_sources`
    /// reverse index, 0020).
    #[tokio::test]
    async fn forget_report_lists_the_conclusions_derived_from_a_source() {
        let db = Db::open_in_memory().unwrap();
        let mut ids = Vec::new();
        for content in ["source fact", "derived conclusion"] {
            let out = crate::write::write_memory(
                &db,
                &crate::write::MemoryWrite {
                    store: MemoryStore::Observation,
                    namespace: Namespace::parse("user").unwrap(),
                    content: content.into(),
                    confidence: 0.9,
                    source_episode: None,
                    supersedes: None,
                },
            )
            .await
            .unwrap();
            let crate::write::WriteOutcome::Inserted(id) = out else {
                panic!("insert")
            };
            ids.push(id);
        }
        crate::consolidate::record_consolidation(
            &db,
            &crate::consolidate::ConsolidationWrite {
                memory_id: ids[1],
                sources: vec![(crate::consolidate::SourceKind::Memory, ids[0])],
            },
        )
        .await
        .unwrap();
        let hash = crate::write::content_hash("source fact");
        let report = forget_report(&db, &hash).await.unwrap();
        let derived = report
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::Derived)
            .unwrap();
        println!(
            "READING C5 derived surface: {:?} {:?}",
            derived.status, derived.sample
        );
        assert_eq!(
            derived
                .sample
                .iter()
                .map(|h| h.key.clone())
                .collect::<Vec<_>>(),
            vec![format!("memory/{}", ids[1])]
        );
    }

    /// t75 / A-1: the surface list, as the report actually emits it. Two things
    /// this pins down: (a) the backup class is present on BOTH sides (memory reads
    /// the memory backups, the wiki's copies are requested from their owner), and
    /// (b) which enum variants no report mentions — the shape of the original
    /// defect, where "not in the list" reads as "not there".
    #[tokio::test]
    async fn the_report_names_every_surface_it_answers_or_requests() {
        let db = Db::open_in_memory().unwrap();
        let report = forget_report(&db, "0123456789abcdef").await.unwrap();
        let local: Vec<ResidualSurface> = report.local.iter().map(|l| l.surface).collect();
        let external: Vec<ResidualSurface> = report.external.iter().map(|e| e.surface).collect();
        println!("READING t75 local surfaces  = {local:?}");
        println!("READING t75 external surfaces = {external:?}");
        assert_eq!(
            local,
            vec![
                ResidualSurface::Memories,
                ResidualSurface::MemoriesFts,
                ResidualSurface::Derived,
                ResidualSurface::Episodes,
                ResidualSurface::BackupFile,
            ]
        );
        assert_eq!(
            external,
            vec![
                ResidualSurface::KnowledgeFile,
                ResidualSurface::KnowledgeDocument,
                ResidualSurface::KnowledgeChunk,
                ResidualSurface::WikiChunk,
                ResidualSurface::WikiPageHash,
                ResidualSurface::WikiPlan,
                ResidualSurface::BackupFile,
            ]
        );
        // The enum carries three more variants that NO surface of this report
        // mentions: `KnowledgeChunkFts`, `KnowledgeVectors`, `WikiBuildPage`. They
        // are named here rather than left implicit — and reported (t75 finding A-2)
        // instead of being added to the report without their owners' queries.
        let mentioned: Vec<ResidualSurface> =
            local.iter().chain(external.iter()).copied().collect();
        for surface in [
            ResidualSurface::KnowledgeChunkFts,
            ResidualSurface::KnowledgeVectors,
            ResidualSurface::WikiBuildPage,
        ] {
            println!("READING t75 unmentioned enum variant: {surface:?}");
            assert!(!mentioned.contains(&surface));
        }
        // ...and the one that IS mentioned on both sides is the new backup class.
        assert!(
            mentioned
                .iter()
                .filter(|s| **s == ResidualSurface::BackupFile)
                .count()
                == 2
        );
    }

    /// The three-state discipline, applied to the new surface (t75): an unrooted
    /// call must NAME the surface and say why it could not be read — never drop it.
    #[tokio::test]
    async fn an_unrooted_report_still_names_the_backup_surface() {
        let db = Db::open_in_memory().unwrap();
        let report = forget_report(&db, "0123456789abcdef").await.unwrap();
        let backup = report
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::BackupFile)
            .expect("the backup surface must be in the list even without a root");
        println!(
            "READING t75 unrooted backup surface: {:?} source={}",
            backup.status, backup.source
        );
        match &backup.status {
            ResidualStatus::NotAvailable(reason) => {
                assert!(
                    reason.contains("forget_report_at"),
                    "the reason must name the call that can answer: {reason}"
                );
            }
            other => panic!("unrooted must be NotAvailable, got {other:?}"),
        }
        assert!(backup.sample.is_empty());
    }

    /// A dump file's inventory line, without needing a root on this machine.
    fn dump_line(name: &str, bytes: u64, verdict: &str) -> String {
        format!("{name}: {bytes} B, {verdict}")
    }

    /// Flush a temp database into its main file BEFORE copying it. Without this
    /// the copy is a database whose schema still sits in the `-wal` file, and the
    /// detector says so ("could not be checked: no such table: memories") — which
    /// is why a database copy is reported with `total: None` rather than a false
    /// zero when it cannot be read. Both layers are unwrapped on purpose: a
    /// checkpoint that fails must be a visible test failure, never a silent no-op.
    async fn flush_for_copy(db: &Db) {
        db.call(|conn| conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)"))
            .await
            .expect("the single-writer actor is open")
            .expect("PRAGMA wal_checkpoint must succeed on a temp database");
    }

    /// t75 / A-1, the whole scenario in one test: a row that carried the
    /// `[distilled]` prefix is cleaned from the live database, and the PRE-CLEANUP
    /// bytes are still readable on disk. `forget_report_at` must say so — both for
    /// the dump `strip_distilled_prefix` wrote and for a whole-database copy.
    #[tokio::test]
    async fn the_pre_cleanup_bytes_are_reported_as_a_residual() {
        let dir = std::env::temp_dir().join(format!("ruagent-t75-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data/backups")).unwrap();
        let db_path = dir.join("data/ruagent.db");
        let db = Db::open(&db_path).unwrap();

        let before = format!("{DISTILLED_PREFIX}the fact that was cleaned up");
        let before_hash = crate::write::content_hash(&before);
        write_memory(
            &db,
            &MemoryWrite {
                store: MemoryStore::Observation,
                namespace: crate::namespace::Namespace::parse("project:t75").unwrap(),
                content: before.clone(),
                confidence: 0.9,
                source_episode: None,
                supersedes: None,
            },
        )
        .await
        .unwrap();

        // a whole-database copy, taken the way the machine has one
        flush_for_copy(&db).await;
        std::fs::copy(&db_path, dir.join("data/ruagent.db.before-t75.db")).unwrap();

        // the cleanup, which writes its own dump first
        let outcome = strip_distilled_prefix(&db, &dir.join("data/backups"))
            .await
            .unwrap();
        println!("READING t75 strip outcome: {outcome:?}");

        let live = forget_report_at(&db, Some(&dir), &before_hash)
            .await
            .unwrap();
        let memories = live
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::Memories)
            .unwrap();
        let backup = live
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::BackupFile)
            .unwrap();
        println!(
            "READING t75 after cleanup: live memories surface={:?} backup surface={:?}",
            memories.status, backup.status
        );
        for hit in &backup.sample {
            println!(
                "READING t75 backup carrier: key={} path={:?}",
                hit.key, hit.path
            );
        }
        // the live database no longer holds the hash ...
        match &memories.status {
            ResidualStatus::Readout(c) => assert_eq!(c.hits, 0, "the live row's bytes changed"),
            other => panic!("expected a readout, got {other:?}"),
        }
        // ... and the plaintext copy is still there, on BOTH candidate kinds
        match &backup.status {
            ResidualStatus::Readout(c) => {
                assert_eq!(c.hits, 2, "the dump AND the database copy carry the bytes");
                assert_eq!(c.total, Some(2), "both candidates were readable");
            }
            other => panic!("expected a readout, got {other:?}"),
        }
        let names: Vec<String> = backup.sample.iter().map(|h| h.key.clone()).collect();
        println!("READING t75 carriers = {names:?}");
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("memories-distilled-prefix-"))
        );
        assert!(names.iter().any(|n| n == "ruagent.db.before-t75.db"));

        // the inventory, printed as evidence (name, size, verdict)
        let scan = scan_backups(&dir, &before_hash);
        for line in &scan.inventory {
            println!("READING t75 inventory {line}");
        }
        assert_eq!(scan.inventory.len(), 2);
        assert!(scan.inventory[0].contains("record(s) with this hash"));
    }

    /// NEGATIVE CONTROL (t75): the surface must report "checked, not there" rather
    /// than treating the mere existence of a backup as a residual.
    #[tokio::test]
    async fn a_backup_that_does_not_hold_the_hash_is_not_a_hit() {
        let dir = std::env::temp_dir().join(format!("ruagent-t75-neg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data/backups")).unwrap();
        let db = Db::open(dir.join("data/ruagent.db")).unwrap();
        write_memory(
            &db,
            &MemoryWrite {
                store: MemoryStore::Observation,
                namespace: crate::namespace::Namespace::parse("project:t75").unwrap(),
                content: format!("{DISTILLED_PREFIX}a different fact"),
                confidence: 0.9,
                source_episode: None,
                supersedes: None,
            },
        )
        .await
        .unwrap();
        flush_for_copy(&db).await;
        std::fs::copy(
            dir.join("data/ruagent.db"),
            dir.join("data/ruagent.db.before-t75-neg.db"),
        )
        .unwrap();
        strip_distilled_prefix(&db, &dir.join("data/backups"))
            .await
            .unwrap();

        let nowhere = crate::write::content_hash("content that exists nowhere at all");
        let report = forget_report_at(&db, Some(&dir), &nowhere).await.unwrap();
        let backup = report
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::BackupFile)
            .unwrap();
        println!(
            "READING t75 negative control: {:?} sample={:?}",
            backup.status, backup.sample
        );
        match &backup.status {
            ResidualStatus::Readout(c) => {
                assert_eq!(c.hits, 0, "no carrier for a hash nothing ever held");
                assert_eq!(c.total, Some(0), "and it WAS measured: not `None`");
            }
            other => panic!("expected a measured zero, got {other:?}"),
        }
        assert!(backup.sample.is_empty());
        assert_eq!(dump_line("x.txt", 1, "checked"), "x.txt: 1 B, checked");
        for line in scan_backups(&dir, &nowhere).inventory {
            println!("READING t75 negative inventory {line}");
            assert!(line.contains("NOT in it"));
        }
    }

    /// The REAL machine, on request only: `RUAGENT_T75_REAL_ROOT` names a ruagent
    /// root, and this test prints each backup file with its size and how it was
    /// judged. Read-only, and loud when it is not asked for (no silent skip).
    #[tokio::test]
    async fn the_real_root_inventory_is_read_when_it_is_named() {
        let Ok(root) = std::env::var("RUAGENT_T75_REAL_ROOT") else {
            println!(
                "READING t75 real-root inventory SKIPPED: RUAGENT_T75_REAL_ROOT is not set \
                 (set it to a ruagent root to read that machine's backups; nothing is written)"
            );
            return;
        };
        let root = std::path::PathBuf::from(root);
        let db = Db::open_in_memory().unwrap();
        let report = forget_report_at(&db, Some(&root), "no-such-hash-on-purpose")
            .await
            .unwrap();
        let backup = report
            .local
            .iter()
            .find(|l| l.surface == ResidualSurface::BackupFile)
            .unwrap();
        println!(
            "READING t75 real root {root:?} backup surface = {:?}",
            backup.status
        );
        let scan = scan_backups(&root, "no-such-hash-on-purpose");
        println!("READING t75 real candidates = {}", scan.inventory.len());
        for line in &scan.inventory {
            println!("READING t75 real inventory {line}");
        }
    }
}
