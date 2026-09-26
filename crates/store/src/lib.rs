//! ruagent-store: persistence for the daemon.
//!
//! Design §10: queryable state lives in SQLite (WAL mode, single-writer
//! actor, group commits); high-volume run events are append-only JSONL
//! transcripts; vectors (M3) go to embedded LanceDB. Postgres is a
//! productization-time swap behind these traits.

pub mod fts;
pub mod migrations;
pub mod sqlite;
pub mod transcript;

pub use sqlite::{Db, DbError};
pub use transcript::{TranscriptWriter, read_transcript, transcript_path};

use chrono::{DateTime, Utc};
use ruagent_core::{Run, RunStatus, StopReason, Task, TaskEdge, TaskStatus};

/// Envelope of a stored run event.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TranscriptLine {
    pub ts: DateTime<Utc>,
    pub seq: u64,
    pub event: ruagent_core::RunEvent,
}

// ---------------------------------------------------------------------------
// Task repository
// ---------------------------------------------------------------------------

impl Db {
    /// Insert a new task.
    pub async fn insert_task(&self, task: &Task) -> Result<(), DbError> {
        let t = task.clone();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO tasks (id, title, intent, status, creator, project, pinned_agent, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    t.id.to_string(),
                    t.title,
                    t.intent,
                    task_status_to_str(t.status),
                    serde_json::to_string(&t.creator).expect("TaskCreator serializes"),
                    t.project,
                    t.pinned_agent.map(|a| a.to_string()),
                    t.created_at.to_rfc3339(),
                    t.updated_at.to_rfc3339(),
                ],
            )
        })
        .await??;
        Ok(())
    }

    /// Update task status (and timestamp).
    pub async fn update_task_status(
        &self,
        id: ruagent_core::TaskId,
        status: TaskStatus,
    ) -> Result<(), DbError> {
        self.call(move |conn| {
            conn.execute(
                "UPDATE tasks SET status = ?2, updated_at = ?3 WHERE id = ?1",
                rusqlite::params![
                    id.to_string(),
                    task_status_to_str(status),
                    Utc::now().to_rfc3339(),
                ],
            )
        })
        .await??;
        Ok(())
    }

    /// Fetch one task by id.
    pub async fn get_task(&self, id: ruagent_core::TaskId) -> Result<Option<Task>, DbError> {
        self.call(move |conn| -> Result<Option<Task>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT id, title, intent, status, creator, project, pinned_agent, created_at, updated_at
                 FROM tasks WHERE id = ?1",
            )?;
            let mut rows = stmt.query([id.to_string()])?;
            match rows.next()? {
                Some(row) => Ok(Some(task_from_row(row)?)),
                None => Ok(None),
            }
        })
        .await?
        .map_err(DbError::from)
    }

    /// List tasks by status (all statuses when `None`), newest first.
    pub async fn list_tasks(&self, status: Option<TaskStatus>) -> Result<Vec<Task>, DbError> {
        self.call(move |conn| -> Result<Vec<Task>, rusqlite::Error> {
            let (sql, param): (&str, Vec<String>) = match status {
                Some(s) => (
                    "SELECT id, title, intent, status, creator, project, pinned_agent, created_at, updated_at
                     FROM tasks WHERE status = ?1 ORDER BY created_at DESC",
                    vec![task_status_to_str(s).to_string()],
                ),
                None => (
                    "SELECT id, title, intent, status, creator, project, pinned_agent, created_at, updated_at
                     FROM tasks ORDER BY created_at DESC",
                    vec![],
                ),
            };
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(param), |row| {
                    task_from_row(row)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
        .map_err(DbError::from)
    }

    /// Delete a task: its runs and edges (transcript files stay on disk —
    /// they are append-only evidence).
    pub async fn delete_task(&self, id: ruagent_core::TaskId) -> Result<(), DbError> {
        self.call(move |conn| -> Result<(), rusqlite::Error> {
            conn.execute("DELETE FROM runs WHERE task_id = ?1", [id.to_string()])?;
            conn.execute(
                "DELETE FROM task_edges WHERE from_id = ?1 OR to_id = ?1",
                [id.to_string()],
            )?;
            conn.execute("DELETE FROM tasks WHERE id = ?1", [id.to_string()])?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    /// Mark the winning run of a task (fan-out selection, design §5.2).
    /// `by` records the selection provenance: `"human"` or `"agent:<name>"`
    /// (a judge run, design §5.3).
    pub async fn set_selected_run(
        &self,
        task_id: ruagent_core::TaskId,
        run_id: ruagent_core::RunId,
        by: &str,
    ) -> Result<(), DbError> {
        let by = by.to_string();
        self.call(move |conn| {
            conn.execute(
                "UPDATE tasks SET selected_run_id = ?2, selected_by = ?3, updated_at = ?4 WHERE id = ?1",
                rusqlite::params![
                    task_id.to_string(),
                    run_id.to_string(),
                    by,
                    Utc::now().to_rfc3339(),
                ],
            )
        })
        .await??;
        Ok(())
    }

    /// The selected winning run of a task and who selected it, if any.
    /// Returns `(run_id, by)`; `by` is `"human"` or `"agent:<name>"`.
    pub async fn selected_run(
        &self,
        task_id: ruagent_core::TaskId,
    ) -> Result<Option<(ruagent_core::RunId, String)>, DbError> {
        type SelectionRow = Option<(Option<String>, Option<String>)>;
        let row: SelectionRow = self
            .call(move |conn| -> Result<SelectionRow, rusqlite::Error> {
                let mut stmt =
                    conn.prepare("SELECT selected_run_id, selected_by FROM tasks WHERE id = ?1")?;
                let mut rows = stmt.query([task_id.to_string()])?;
                match rows.next()? {
                    // get::<Option<String>>: the columns are NULL-able, and
                    // a bare `String` get rejects NULL rows.
                    Some(row) => {
                        let run: Option<String> = row.get(0)?;
                        let by: Option<String> = row.get(1)?;
                        Ok(Some((run, by)))
                    }
                    None => Ok(None),
                }
            })
            .await??;
        match row {
            // NULL selected_by = a selection made before migration 0012.
            Some((Some(run), by)) => Ok(Some((
                run.parse().map_err(conv)?,
                by.unwrap_or_else(|| "human".into()),
            ))),
            Some((None, _)) | None => Ok(None),
        }
    }

    /// Insert a typed task edge (idempotent).
    pub async fn insert_edge(&self, edge: TaskEdge) -> Result<(), DbError> {
        self.call(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO task_edges (from_id, to_id, kind) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    edge.from.to_string(),
                    edge.to.to_string(),
                    edge_kind_to_str(edge.kind),
                ],
            )
        })
        .await??;
        Ok(())
    }

    /// Edges leaving a task.
    pub async fn edges_from(&self, from: ruagent_core::TaskId) -> Result<Vec<TaskEdge>, DbError> {
        self.call(move |conn| -> Result<Vec<TaskEdge>, rusqlite::Error> {
            let mut stmt =
                conn.prepare("SELECT from_id, to_id, kind FROM task_edges WHERE from_id = ?1")?;
            let rows = stmt
                .query_map([from.to_string()], edge_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
        .map_err(DbError::from)
    }
}

// ---------------------------------------------------------------------------
// Run repository
// ---------------------------------------------------------------------------

impl Db {
    /// Insert a new run.
    pub async fn insert_run(&self, run: &Run) -> Result<(), DbError> {
        let r = run.clone();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO runs (id, task_id, agent, params, status, acp_session_id, workspace,
                                   context_used, context_size, cost_usd, error, result, stop_reason,
                                   created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                rusqlite::params![
                    r.id.to_string(),
                    r.task_id.to_string(),
                    r.params.agent.to_string(),
                    serde_json::to_string(&r.params).expect("RunParams serializes"),
                    run_status_to_str(r.status),
                    r.acp_session_id,
                    r.workspace,
                    r.context_usage.map(|u| u.used),
                    r.context_usage.map(|u| u.size),
                    r.cost_usd,
                    r.error,
                    r.result,
                    r.stop_reason.map(stop_reason_to_str),
                    r.created_at.to_rfc3339(),
                    r.updated_at.to_rfc3339(),
                ],
            )
        })
        .await??;
        Ok(())
    }

    /// Persist the full mutable state of a run (the orchestrator owns the
    /// `Run` struct and writes it back on every transition).
    pub async fn update_run(&self, run: &Run) -> Result<(), DbError> {
        let r = run.clone();
        self.call(move |conn| {
            conn.execute(
                "UPDATE runs SET status = ?2, acp_session_id = ?3, workspace = ?4,
                                 context_used = ?5, context_size = ?6, cost_usd = ?7,
                                 error = ?8, result = ?9, stop_reason = ?10, updated_at = ?11
                 WHERE id = ?1",
                rusqlite::params![
                    r.id.to_string(),
                    run_status_to_str(r.status),
                    r.acp_session_id,
                    r.workspace,
                    r.context_usage.map(|u| u.used),
                    r.context_usage.map(|u| u.size),
                    r.cost_usd,
                    r.error,
                    r.result,
                    r.stop_reason.map(stop_reason_to_str),
                    r.updated_at.to_rfc3339(),
                ],
            )
        })
        .await??;
        Ok(())
    }

    /// Fetch one run by id.
    pub async fn get_run(&self, id: ruagent_core::RunId) -> Result<Option<Run>, DbError> {
        self.call(move |conn| -> Result<Option<Run>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT id, task_id, agent, params, status, acp_session_id, workspace,
                        context_used, context_size, cost_usd, error, result, stop_reason, created_at, updated_at
                 FROM runs WHERE id = ?1",
            )?;
            let mut rows = stmt.query([id.to_string()])?;
            match rows.next()? {
                Some(row) => Ok(Some(run_from_row(row)?)),
                None => Ok(None),
            }
        })
        .await?
        .map_err(DbError::from)
    }

    /// All runs of a task, oldest first.
    pub async fn list_runs_for_task(
        &self,
        task_id: ruagent_core::TaskId,
    ) -> Result<Vec<Run>, DbError> {
        self.call(move |conn| -> Result<Vec<Run>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT id, task_id, agent, params, status, acp_session_id, workspace,
                        context_used, context_size, cost_usd, error, result, stop_reason, created_at, updated_at
                 FROM runs WHERE task_id = ?1 ORDER BY created_at ASC",
            )?;
            let rows = stmt
                .query_map([task_id.to_string()], run_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
        .map_err(DbError::from)
    }

    /// All runs in a given status (daemon recovery: find interrupted).
    pub async fn list_runs_by_status(&self, status: RunStatus) -> Result<Vec<Run>, DbError> {
        self.call(move |conn| -> Result<Vec<Run>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT id, task_id, agent, params, status, acp_session_id, workspace,
                        context_used, context_size, cost_usd, error, result, stop_reason, created_at, updated_at
                 FROM runs WHERE status = ?1 ORDER BY created_at ASC",
            )?;
            let rows = stmt
                .query_map([run_status_to_str(status)], run_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
        .map_err(DbError::from)
    }
}

// ---------------------------------------------------------------------------
// Observability aggregations (design §8.1)
// ---------------------------------------------------------------------------

/// Cost/usage totals grouped by agent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentStats {
    pub agent: String,
    pub runs: u64,
    pub completed: u64,
    pub failed: u64,
    pub total_cost_usd: f64,
    pub last_run_at: Option<String>,
}

impl Db {
    /// Aggregate run outcomes and costs per agent.
    pub async fn agent_stats(&self) -> Result<Vec<AgentStats>, DbError> {
        self.call(move |conn| -> Result<Vec<AgentStats>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT a.name AS name,
                        COUNT(*) AS runs,
                        SUM(CASE WHEN r.status = 'completed' THEN 1 ELSE 0 END) AS completed,
                        SUM(CASE WHEN r.status = 'failed' THEN 1 ELSE 0 END) AS failed,
                        COALESCE(SUM(r.cost_usd), 0.0) AS total_cost,
                        MAX(r.created_at) AS last_run
                 FROM runs r JOIN agents a ON a.id = r.agent
                 GROUP BY a.name ORDER BY total_cost DESC",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(AgentStats {
                        agent: row.get("name")?,
                        runs: row.get("runs")?,
                        completed: row.get("completed")?,
                        failed: row.get("failed")?,
                        total_cost_usd: row.get("total_cost")?,
                        last_run_at: row.get("last_run")?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?
        .map_err(DbError::from)
    }
}

// ---------------------------------------------------------------------------
// Agent registry
// ---------------------------------------------------------------------------

impl Db {
    /// Insert or replace an agent by name (id preserved if the name exists).
    pub async fn upsert_agent(&self, card: &ruagent_core::AgentCard) -> Result<(), DbError> {
        let c = card.clone();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO agents (name, id, harness, card) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(name) DO UPDATE SET harness = ?3, card = ?4",
                rusqlite::params![
                    c.name,
                    c.id.to_string(),
                    format!("{:?}", c.harness),
                    serde_json::to_string(&c).expect("AgentCard serializes"),
                ],
            )
        })
        .await??;
        Ok(())
    }

    /// The stable id for an agent name, if registered.
    pub async fn agent_id_by_name(
        &self,
        name: &str,
    ) -> Result<Option<ruagent_core::AgentId>, DbError> {
        let name = name.to_string();
        let row: Option<String> = self
            .call(move |conn| -> Result<Option<String>, rusqlite::Error> {
                let mut stmt = conn.prepare("SELECT id FROM agents WHERE name = ?1")?;
                let mut rows = stmt.query([&name])?;
                match rows.next()? {
                    Some(row) => Ok(Some(row.get(0)?)),
                    None => Ok(None),
                }
            })
            .await??;
        match row {
            Some(s) => Ok(Some(s.parse().map_err(conv)?)),
            None => Ok(None),
        }
    }

    /// All registered agents.
    pub async fn list_agents(&self) -> Result<Vec<ruagent_core::AgentCard>, DbError> {
        self.call(
            move |conn| -> Result<Vec<ruagent_core::AgentCard>, rusqlite::Error> {
                let mut stmt = conn.prepare("SELECT card FROM agents ORDER BY name")?;
                let rows = stmt
                    .query_map([], |row| {
                        let card: String = row.get(0)?;
                        serde_json::from_str(&card).map_err(conv)
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            },
        )
        .await?
        .map_err(DbError::from)
    }
}

// ---------------------------------------------------------------------------
// Row mapping
// ---------------------------------------------------------------------------

/// Adapt a domain parse error into a sqlite conversion error.
fn conv(e: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let id: String = row.get("id")?;
    let creator: String = row.get("creator")?;
    let pinned: Option<String> = row.get("pinned_agent")?;
    let created: String = row.get("created_at")?;
    let updated: String = row.get("updated_at")?;
    let status: String = row.get("status")?;
    Ok(Task {
        id: id.parse().map_err(conv)?,
        title: row.get("title")?,
        intent: row.get("intent")?,
        status: task_status_from_str(&status),
        creator: serde_json::from_str(&creator).map_err(conv)?,
        project: row.get("project")?,
        pinned_agent: pinned.map(|s| s.parse().map_err(conv)).transpose()?,
        created_at: DateTime::parse_from_rfc3339(&created)
            .map_err(conv)?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated)
            .map_err(conv)?
            .with_timezone(&Utc),
    })
}

fn edge_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskEdge> {
    let from: String = row.get("from_id")?;
    let to: String = row.get("to_id")?;
    let kind: String = row.get("kind")?;
    Ok(TaskEdge {
        from: from.parse().map_err(conv)?,
        to: to.parse().map_err(conv)?,
        kind: edge_kind_from_str(&kind),
    })
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Run> {
    let id: String = row.get("id")?;
    let task_id: String = row.get("task_id")?;
    let params: String = row.get("params")?;
    let status: String = row.get("status")?;
    let created: String = row.get("created_at")?;
    let updated: String = row.get("updated_at")?;
    let context_used: Option<u64> = row.get("context_used")?;
    let context_size: Option<u64> = row.get("context_size")?;
    Ok(Run {
        id: id.parse().map_err(conv)?,
        task_id: task_id.parse().map_err(conv)?,
        params: serde_json::from_str(&params).map_err(conv)?,
        status: run_status_from_str(&status),
        acp_session_id: row.get("acp_session_id")?,
        workspace: row.get("workspace")?,
        context_usage: match (context_used, context_size) {
            (Some(used), Some(size)) => Some(ruagent_core::ContextUsage {
                used,
                size,
                cost_usd: None,
            }),
            _ => None,
        },
        cost_usd: row.get("cost_usd")?,
        error: row.get("error")?,
        result: row.get("result")?,
        stop_reason: row
            .get::<_, Option<String>>("stop_reason")?
            .map(|s| stop_reason_from_str(&s)),
        created_at: DateTime::parse_from_rfc3339(&created)
            .map_err(conv)?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated)
            .map_err(conv)?
            .with_timezone(&Utc),
    })
}

// ---------------------------------------------------------------------------
// Enum <-> string mappings (SQLite stores TEXT)
// ---------------------------------------------------------------------------

fn task_status_to_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Pending => "pending",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn task_status_from_str(s: &str) -> TaskStatus {
    match s {
        "in_progress" => TaskStatus::InProgress,
        "blocked" => TaskStatus::Blocked,
        "done" => TaskStatus::Done,
        "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::Pending,
    }
}

fn edge_kind_to_str(k: ruagent_core::EdgeKind) -> &'static str {
    match k {
        ruagent_core::EdgeKind::DependsOn => "depends_on",
        ruagent_core::EdgeKind::SpawnedBy => "spawned_by",
        ruagent_core::EdgeKind::Reviews => "reviews",
        ruagent_core::EdgeKind::FanoutOf => "fanout_of",
    }
}

fn edge_kind_from_str(s: &str) -> ruagent_core::EdgeKind {
    match s {
        "spawned_by" => ruagent_core::EdgeKind::SpawnedBy,
        "reviews" => ruagent_core::EdgeKind::Reviews,
        "fanout_of" => ruagent_core::EdgeKind::FanoutOf,
        _ => ruagent_core::EdgeKind::DependsOn,
    }
}

fn run_status_to_str(s: RunStatus) -> &'static str {
    match s {
        RunStatus::Queued => "queued",
        RunStatus::Spawning => "spawning",
        RunStatus::Running => "running",
        RunStatus::WaitingPermission => "waiting_permission",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Interrupted => "interrupted",
    }
}

fn run_status_from_str(s: &str) -> RunStatus {
    match s {
        "spawning" => RunStatus::Spawning,
        "running" => RunStatus::Running,
        "waiting_permission" => RunStatus::WaitingPermission,
        "completed" => RunStatus::Completed,
        "failed" => RunStatus::Failed,
        "cancelled" => RunStatus::Cancelled,
        "interrupted" => RunStatus::Interrupted,
        _ => RunStatus::Queued,
    }
}

fn stop_reason_to_str(s: StopReason) -> &'static str {
    match s {
        StopReason::EndTurn => "end_turn",
        StopReason::Cancelled => "cancelled",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurns => "max_turns",
        StopReason::Refusal => "refusal",
        StopReason::Error => "error",
    }
}

fn stop_reason_from_str(s: &str) -> StopReason {
    match s {
        "cancelled" => StopReason::Cancelled,
        "max_tokens" => StopReason::MaxTokens,
        "max_turns" => StopReason::MaxTurns,
        "refusal" => StopReason::Refusal,
        "error" => StopReason::Error,
        _ => StopReason::EndTurn,
    }
}

// ---------------------------------------------------------------------------
// Session lifecycle: archive (a ruagent-side hide) + delete (own sessions only)
// ---------------------------------------------------------------------------

/// What a delete attempt did. The source guard is evaluated inside the SAME
/// writer closure as the DELETE, so a row can never be removed by a caller
/// that did not first see (and accept) the row's source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteSession {
    Deleted,
    NotFound,
    /// The session is another tool's history file. ruagent only indexes it,
    /// so it must not remove it — the actual source is reported back because
    /// the caller owes the user that explanation.
    SourceNotAllowed(String),
}

impl Db {
    /// Hide a session in ruagent's own list.
    ///
    /// Writes exactly one row in session_archives — the sessions index and the
    /// source file it points at are never touched, so this is reversible and
    /// works for every source (including other tools', whose files we have no
    /// business editing).
    pub async fn archive_session(&self, key: &str, now_ms: i64) -> Result<(), DbError> {
        let key = key.to_string();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO session_archives (key, archived_at) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET archived_at = excluded.archived_at",
                rusqlite::params![key, now_ms],
            )
        })
        .await??;
        Ok(())
    }

    /// Undo archive_session. false = it was not archived.
    pub async fn unarchive_session(&self, key: &str) -> Result<bool, DbError> {
        let key = key.to_string();
        let n = self
            .call(move |conn| conn.execute("DELETE FROM session_archives WHERE key = ?1", [&key]))
            .await??;
        Ok(n > 0)
    }

    /// Keys hidden from the default list.
    pub async fn archived_session_keys(&self) -> Result<Vec<String>, DbError> {
        self.call(|conn| {
            let mut stmt = conn
                .prepare("SELECT key FROM session_archives")
                .map_err(DbError::from)?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for k in rows.flatten() {
                out.push(k);
            }
            Ok(out)
        })
        .await?
    }

    /// Keys the user deleted from ruagent's index. Unlike a missing row these
    /// survive the indexer's next INSERT OR REPLACE, which is what makes the
    /// delete stick (measured: without a tombstone the row returned in <70s).
    pub async fn deleted_session_keys(&self) -> Result<Vec<String>, DbError> {
        self.call(|conn| {
            let mut stmt = conn
                .prepare("SELECT key FROM session_deletions")
                .map_err(DbError::from)?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for k in rows.flatten() {
                out.push(k);
            }
            Ok(out)
        })
        .await?
    }

    /// Is this key in the index at all? A hide for an unknown key would linger
    /// and could hide an unrelated future row that reuses the key.
    pub async fn session_exists(&self, key: &str) -> Result<bool, DbError> {
        let key = key.to_string();
        self.call(move |conn| {
            conn.query_row("SELECT 1 FROM sessions WHERE key = ?1", [&key], |_| Ok(()))
                .is_ok()
        })
        .await
    }

    /// Remove one chat-history row (the panel's forget-this-conversation).
    ///
    /// Deletes the `chats` row ONLY. The transcript JSONL, the
    /// `sessions` index row and anything memory ingestion produced are
    /// untouched -- retracting a conversation's content is a separate
    /// decision with its own markers (0016_session_archives /
    /// 0017_session_deletions).
    ///
    /// NO TOMBSTONE IS NEEDED HERE, unlike `sessions`: nothing rebuilds
    /// the `chats` table. Its only writers are ChatManager::start
    /// (INSERT ... ON CONFLICT, at chat start/resume) and the first-prompt
    /// title UPDATE; there is no periodic re-index of it. See
    /// ChatManager::delete for the full check.
    ///
    /// Returns true when a row was removed, false when the id was absent --
    /// so a repeated delete is an idempotent no-op.
    pub async fn delete_chat(&self, id: &str) -> Result<bool, DbError> {
        let id = id.to_string();
        self.call(move |conn| {
            let n = conn
                .execute("DELETE FROM chats WHERE id = ?1", [&id])
                .unwrap_or(0);
            n > 0
        })
        .await
    }

    /// Remove a session from ruagent's index.
    ///
    /// allowed_source is the only source this caller may delete from; every
    /// other source comes back as DeleteSession::SourceNotAllowed. The hide
    /// marker is dropped with the row, so a later session that happens to
    /// reuse the key is not born invisible.
    pub async fn delete_session(
        &self,
        key: &str,
        allowed_source: &str,
        now_ms: i64,
    ) -> Result<DeleteSession, DbError> {
        let key = key.to_string();
        let allowed = allowed_source.to_string();
        self.call(move |conn| {
            let source: Option<String> = conn
                .query_row("SELECT source FROM sessions WHERE key = ?1", [&key], |r| {
                    r.get(0)
                })
                .ok();
            let Some(source) = source else {
                return DeleteSession::NotFound;
            };
            if source != allowed {
                return DeleteSession::SourceNotAllowed(source);
            }
            // Tombstone first: if the process dies between the two statements
            // the session is still hidden, whereas the reverse order would
            // leave a row that the indexer happily re-creates.
            let _ = conn.execute(
                "INSERT OR REPLACE INTO session_deletions (key, deleted_at) VALUES (?1, ?2)",
                rusqlite::params![&key, now_ms],
            );
            let n = conn
                .execute("DELETE FROM sessions WHERE key = ?1", [&key])
                .unwrap_or(0);
            if n == 0 {
                return DeleteSession::NotFound;
            }
            let _ = conn.execute("DELETE FROM session_archives WHERE key = ?1", [&key]);
            DeleteSession::Deleted
        })
        .await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ruagent_core::{RunParams, TaskCreator};

    #[tokio::test]
    async fn task_roundtrip_and_status_update() {
        let db = Db::open_in_memory().unwrap();
        let mut t = Task::new("title", "intent", TaskCreator::Human);
        db.insert_task(&t).await.unwrap();

        let got = db.get_task(t.id).await.unwrap().expect("task present");
        assert_eq!(got, t);

        db.update_task_status(t.id, TaskStatus::Done).await.unwrap();
        let got = db.get_task(t.id).await.unwrap().unwrap();
        assert_eq!(got.status, TaskStatus::Done);
        assert!(got.updated_at >= t.updated_at);
        t.status = TaskStatus::Done;
        t.updated_at = got.updated_at;

        // list by status
        let done = db.list_tasks(Some(TaskStatus::Done)).await.unwrap();
        assert_eq!(done, vec![t.clone()]);
        let pending = db.list_tasks(Some(TaskStatus::Pending)).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn run_roundtrip_and_update() {
        let db = Db::open_in_memory().unwrap();
        let task = Task::new("t", "i", TaskCreator::Human);
        db.insert_task(&task).await.unwrap();

        let mut run = Run::new(
            task.id,
            RunParams::for_agent(ruagent_core::AgentId::generate()),
        );
        db.insert_run(&run).await.unwrap();

        run.status = RunStatus::Running;
        run.acp_session_id = Some("sess-1".into());
        run.context_usage = Some(ruagent_core::ContextUsage {
            used: 10,
            size: 100,
            cost_usd: None,
        });
        run.cost_usd = Some(0.02);
        run.error = None;
        run.updated_at = Utc::now();
        db.update_run(&run).await.unwrap();

        let got = db.get_run(run.id).await.unwrap().expect("run present");
        assert_eq!(got.status, RunStatus::Running);
        assert_eq!(got.acp_session_id.as_deref(), Some("sess-1"));
        assert_eq!(got.cost_usd, Some(0.02));
        assert_eq!(got.context_usage.map(|u| u.used), Some(10));

        let runs = db.list_runs_for_task(task.id).await.unwrap();
        assert_eq!(runs.len(), 1);
    }

    #[tokio::test]
    async fn agent_stats_aggregate_costs() {
        let db = Db::open_in_memory().unwrap();
        let card = ruagent_core::AgentCard {
            id: ruagent_core::AgentId::generate(),
            name: "claude".into(),
            harness: ruagent_core::HarnessKind::ClaudeCode,
            command: None,
            description: String::new(),
            model: None,
            reasoning_effort: None,
            context_window: None,
            mcp_profile: None,
            prompt: None,
            runtime: None,
            runtimes: Vec::new(),
            options: Default::default(),
            tags: vec![],
            models: vec![],
            enabled: true,
        };
        db.upsert_agent(&card).await.unwrap();
        let task = Task::new("t", "i", TaskCreator::Human);
        db.insert_task(&task).await.unwrap();
        for (status, cost) in [
            (RunStatus::Completed, 0.5),
            (RunStatus::Completed, 0.25),
            (RunStatus::Failed, 0.1),
        ] {
            let mut run = Run::new(task.id, RunParams::for_agent(card.id));
            run.status = status;
            run.cost_usd = Some(cost);
            db.insert_run(&run).await.unwrap();
        }
        let stats = db.agent_stats().await.unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].agent, "claude");
        assert_eq!(stats[0].runs, 3);
        assert_eq!(stats[0].completed, 2);
        assert_eq!(stats[0].failed, 1);
        assert!((stats[0].total_cost_usd - 0.85).abs() < 1e-9);
    }

    #[tokio::test]
    async fn agent_upsert_preserves_id() {
        let db = Db::open_in_memory().unwrap();
        let mut card = ruagent_core::AgentCard {
            id: ruagent_core::AgentId::generate(),
            name: "claude".into(),
            harness: ruagent_core::HarnessKind::ClaudeCode,
            command: None,
            description: "v1".into(),
            model: None,
            reasoning_effort: None,
            context_window: None,
            mcp_profile: None,
            prompt: None,
            runtime: None,
            runtimes: Vec::new(),
            options: Default::default(),
            tags: vec![],
            models: vec![],
            enabled: true,
        };
        db.upsert_agent(&card).await.unwrap();

        // Same name, different content: id stays, card updates.
        card.description = "v2".into();
        card.id = ruagent_core::AgentId::generate();
        db.upsert_agent(&card).await.unwrap();

        let id = db.agent_id_by_name("claude").await.unwrap().unwrap();
        let all = db.list_agents().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].description, "v2");
        assert_ne!(id, card.id, "stored id must be the first one");
    }

    #[tokio::test]
    async fn edges_are_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let a = Task::new("a", "i", TaskCreator::Human);
        let b = Task::new("b", "i", TaskCreator::Human);
        db.insert_task(&a).await.unwrap();
        db.insert_task(&b).await.unwrap();

        let edge = TaskEdge {
            from: a.id,
            to: b.id,
            kind: ruagent_core::EdgeKind::Reviews,
        };
        db.insert_edge(edge).await.unwrap();
        db.insert_edge(edge).await.unwrap(); // ignored

        let edges = db.edges_from(a.id).await.unwrap();
        assert_eq!(edges, vec![edge]);
    }

    // -- session lifecycle -------------------------------------------------

    /// Insert an index row the way the indexer does (INSERT OR REPLACE, no
    /// archive column) so the tests exercise the real write shape.
    async fn seed_session(db: &Db, key: &str, source: &str) {
        let key = key.to_string();
        let source = source.to_string();
        db.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO sessions
                     (key, source, title, project, ref_path, started_at, updated_at,
                      mtime_ms, size_bytes, message_count, preview)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                rusqlite::params![
                    key, source, "t", "/w", "/f.jsonl", 1i64, 2i64, 2i64, 3i64, 4i64, "p"
                ],
            )
        })
        .await
        .unwrap()
        .unwrap();
    }

    #[tokio::test]
    async fn archive_hides_and_unarchive_restores() {
        let db = Db::open_in_memory().unwrap();
        seed_session(&db, "dsh:aa", "dsh").await;

        assert!(db.archived_session_keys().await.unwrap().is_empty());
        db.archive_session("dsh:aa", 1_700_000_000_000)
            .await
            .unwrap();
        assert_eq!(db.archived_session_keys().await.unwrap(), vec!["dsh:aa"]);
        // Idempotent: hiding twice is not an error and does not duplicate.
        db.archive_session("dsh:aa", 1_700_000_000_001)
            .await
            .unwrap();
        assert_eq!(db.archived_session_keys().await.unwrap().len(), 1);
        assert!(db.unarchive_session("dsh:aa").await.unwrap());
        assert!(db.archived_session_keys().await.unwrap().is_empty());
        assert!(!db.unarchive_session("dsh:aa").await.unwrap());
        // The index row itself is untouched by either direction.
        assert!(db.session_exists("dsh:aa").await.unwrap());
    }

    /// The reason the marker is a side table: the indexer re-writes the row
    /// with INSERT OR REPLACE on every rescan, which would blank a column.
    #[tokio::test]
    async fn archive_survives_a_reindex() {
        let db = Db::open_in_memory().unwrap();
        seed_session(&db, "claude-code:bb", "claude-code").await;
        db.archive_session("claude-code:bb", 1).await.unwrap();

        seed_session(&db, "claude-code:bb", "claude-code").await; // rescan
        assert_eq!(
            db.archived_session_keys().await.unwrap(),
            vec!["claude-code:bb"]
        );
    }

    #[tokio::test]
    async fn delete_is_limited_to_the_allowed_source() {
        let db = Db::open_in_memory().unwrap();
        seed_session(&db, "ruagent:cc", "ruagent").await;
        seed_session(&db, "claude-code:dd", "claude-code").await;

        // Another tool's history: refused, and the row is still there.
        assert_eq!(
            db.delete_session("claude-code:dd", "ruagent", 9)
                .await
                .unwrap(),
            DeleteSession::SourceNotAllowed("claude-code".into())
        );
        assert!(db.session_exists("claude-code:dd").await.unwrap());

        // Our own: deleted.
        assert_eq!(
            db.delete_session("ruagent:cc", "ruagent", 9).await.unwrap(),
            DeleteSession::Deleted
        );
        assert!(!db.session_exists("ruagent:cc").await.unwrap());

        // Unknown key.
        assert_eq!(
            db.delete_session("ruagent:nope", "ruagent", 9)
                .await
                .unwrap(),
            DeleteSession::NotFound
        );
    }

    #[tokio::test]
    async fn delete_drops_the_hide_marker_with_the_row() {
        let db = Db::open_in_memory().unwrap();
        seed_session(&db, "ruagent:ee", "ruagent").await;
        db.archive_session("ruagent:ee", 7).await.unwrap();
        assert_eq!(
            db.delete_session("ruagent:ee", "ruagent", 9).await.unwrap(),
            DeleteSession::Deleted
        );
        // A future session reusing the key must not be born invisible.
        assert!(db.archived_session_keys().await.unwrap().is_empty());
    }

    /// The whole reason the tombstone exists: the indexer re-inserts every
    /// session it finds on disk with INSERT OR REPLACE, so a deletion that
    /// lived only in the missing row was undone by the next 60s rescan
    /// (measured live before this test was written: the row was back in <70s).
    #[tokio::test]
    async fn delete_survives_a_reindex() {
        let db = Db::open_in_memory().unwrap();
        seed_session(&db, "ruagent:ff", "ruagent").await;
        assert_eq!(
            db.delete_session("ruagent:ff", "ruagent", 9).await.unwrap(),
            DeleteSession::Deleted
        );
        assert_eq!(db.deleted_session_keys().await.unwrap(), vec!["ruagent:ff"]);

        // The rescan puts the row back (the source file is still on disk) ...
        seed_session(&db, "ruagent:ff", "ruagent").await;
        assert!(db.session_exists("ruagent:ff").await.unwrap());
        // ... but the tombstone still marks it, so the list can hide it.
        assert_eq!(db.deleted_session_keys().await.unwrap(), vec!["ruagent:ff"]);

        // A second delete of the resurrected row stays a single tombstone.
        assert_eq!(
            db.delete_session("ruagent:ff", "ruagent", 11)
                .await
                .unwrap(),
            DeleteSession::Deleted
        );
        assert_eq!(db.deleted_session_keys().await.unwrap().len(), 1);
    }
}
