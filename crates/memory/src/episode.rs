//! Episodes: the non-lossy base layer (design §6.6 #2). Raw content lands
//! here first, hash-deduped; derived memories reference their episode.

use chrono::Utc;
use ruagent_store::Db;

use crate::write::DbError;

/// What kind of raw content an episode captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeKind {
    RunTurn,
    Document,
    Manual,
    McpWrite,
}

impl EpisodeKind {
    fn as_str(self) -> &'static str {
        match self {
            EpisodeKind::RunTurn => "run_turn",
            EpisodeKind::Document => "document",
            EpisodeKind::Manual => "manual",
            EpisodeKind::McpWrite => "mcp_write",
        }
    }
}

/// Record an episode. Idempotent by content hash: a repeated ingestion
/// returns the existing id (the pipeline's dedup key).
pub async fn record_episode(
    db: &Db,
    kind: EpisodeKind,
    content: &str,
    source_run: Option<&str>,
) -> Result<i64, DbError> {
    let hash = crate::write::content_hash(content);
    let kind = kind.as_str();
    let content = content.to_string();
    let run = source_run.map(str::to_string);
    let now = Utc::now().to_rfc3339();
    let hash_for_lookup = hash.clone();
    let row: Option<i64> = db
        .call(move |conn| -> Result<Option<i64>, rusqlite::Error> {
            let existing: Option<i64> = conn
                .query_row(
                    "SELECT id FROM episodes WHERE content_hash = ?1",
                    [&hash],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?;
            if let Some(id) = existing {
                return Ok(Some(id));
            }
            conn.execute(
                "INSERT INTO episodes (kind, content, content_hash, ref_time, ingested_at, source_run)
                 VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
                rusqlite::params![kind, content, hash, now, run],
            )?;
            Ok(None)
        })
        .await??;
    match row {
        Some(id) => Ok(id),
        None => {
            let hash2 = hash_for_lookup;
            let id: i64 = db
                .call(move |conn| {
                    conn.query_row(
                        "SELECT id FROM episodes WHERE content_hash = ?1",
                        [&hash2],
                        |r| r.get(0),
                    )
                })
                .await??;
            Ok(id)
        }
    }
}

/// Total episode count (used by the `ruagent://` root listing).
pub async fn episode_count(db: &Db) -> Result<i64, DbError> {
    db.call(|conn| conn.query_row("SELECT COUNT(*) FROM episodes", [], |r| r.get(0)))
        .await?
        .map_err(DbError::from)
}
