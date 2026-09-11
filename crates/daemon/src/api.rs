//! HTTP API (`/api/v1`) + SSE event streaming.

use std::convert::Infallible;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use ruagent_core::{Run, RunId, RunStatus, Task, TaskCreator, TaskStatus};
use ruagent_store::{TranscriptLine, transcript_path};
use serde::Deserialize;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::config::DaemonConfig;
use crate::runs::{PendingPermission, RunManager, WorkspaceSpec};

#[derive(Clone)]
pub struct AppState {
    pub mgr: std::sync::Arc<RunManager>,
    pub config: std::sync::Arc<DaemonConfig>,
    pub knowledge: std::sync::Arc<ruagent_knowledge::Knowledge>,
}

/// Build the API router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/stats", get(stats))
        .route("/api/v1/mcp", get(mcp_registry))
        .route("/api/v1/skills", get(list_skills))
        .route("/api/v1/skills/sync", post(sync_skills))
        .route("/api/v1/memory/write", post(memory_write))
        .route("/api/v1/memory/search", get(memory_search))
        .route("/api/v1/memory/list", get(memory_list))
        .route("/api/v1/knowledge/ingest", post(knowledge_ingest))
        .route("/api/v1/knowledge/search", get(knowledge_search))
        .route("/api/v1/tasks", post(create_task).get(list_tasks))
        .route("/api/v1/tasks/{id}", get(get_task))
        .route("/api/v1/tasks/{id}/runs", post(start_run))
        .route("/api/v1/tasks/{id}/fanout", post(start_fanout))
        .route("/api/v1/tasks/{id}/pipeline", post(start_pipeline))
        .route("/api/v1/runs/{id}", get(get_run))
        .route("/api/v1/runs/{id}/select", post(select_run))
        .route("/api/v1/runs/{id}/events", get(run_events))
        .route("/api/v1/permissions", get(list_permissions))
        .route("/api/v1/permissions/{key}", post(resolve_permission))
        .with_state(state)
        .fallback_service(panel_service())
}

/// Serve the built web panel (SPA) when `panel/dist` exists; the API
/// works fine without it. Override with `RUAGENT_PANEL_DIST`.
fn panel_service() -> tower_http::services::ServeDir<tower_http::services::ServeFile> {
    use tower_http::services::{ServeDir, ServeFile};
    let dist = std::env::var("RUAGENT_PANEL_DIST")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("panel/dist"));
    if !dist.join("index.html").is_file() {
        tracing::info!(
            dist = %dist.display(),
            "panel not built — serving API only (cd panel && npm run build)"
        );
    }
    ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html")))
}

/// Error type that renders as (status, message).
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }

    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
}

impl From<ruagent_store::DbError> for ApiError {
    fn from(e: ruagent_store::DbError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("{e}"),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("{e:#}"),
        }
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, self.message).into_response()
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "ruagent" }))
}

async fn stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let stats = state.mgr.db().agent_stats().await?;
    Ok(Json(serde_json::json!({ "agents": stats })))
}

/// Discovered skills (platform library + the daemon's working-dir
/// project skills).
async fn list_skills(State(state): State<AppState>) -> Json<serde_json::Value> {
    let root = state.mgr.root();
    let platform = root.join("skills");
    let project = std::path::PathBuf::from(".ruagent").join("skills");
    let skills = crate::skills::discover(&platform, Some(&project));
    Json(serde_json::json!({ "skills": skills }))
}

/// Sync skills into every enabled harness's skill directories under the
/// daemon's working directory (design SS7.2).
async fn sync_skills(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let root = state.mgr.root();
    let platform = root.join("skills");
    let project = std::path::PathBuf::from(".ruagent").join("skills");
    let skills = crate::skills::discover(&platform, Some(&project));
    let harnesses: Vec<String> = state
        .mgr
        .agents()
        .into_iter()
        .filter(|a| a.enabled)
        .map(|a| format!("{:?}", a.harness).to_lowercase())
        .collect();
    let (installed, skipped) = crate::skills::sync(&skills, std::path::Path::new("."), &harnesses)
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(
        serde_json::json!({ "installed": installed, "skipped": skipped }),
    ))
}

/// The MCP registry as configured (design SS7.1). Live health pinging
/// arrives with the M3 platform MCP server.
async fn mcp_registry(State(state): State<AppState>) -> Json<serde_json::Value> {
    let servers: Vec<serde_json::Value> = state
        .config
        .mcp
        .servers
        .iter()
        .map(|(name, e)| {
            serde_json::json!({
                "name": name,
                "command": e.command,
                "url": e.url,
                "inject_for": e.inject_for,
            })
        })
        .collect();
    let profiles: Vec<serde_json::Value> = state
        .config
        .mcp
        .profiles
        .iter()
        .map(|(name, servers)| serde_json::json!({ "name": name, "servers": servers }))
        .collect();
    Json(serde_json::json!({ "servers": servers, "profiles": profiles }))
}

async fn list_agents(State(state): State<AppState>) -> Json<serde_json::Value> {
    let agents: Vec<serde_json::Value> = state
        .mgr
        .agents()
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id.to_string(),
                "name": a.name,
                "harness": format!("{:?}", a.harness),
                "description": a.description,
                "model": a.model,
                "enabled": a.enabled,
            })
        })
        .collect();
    Json(serde_json::json!({ "agents": agents }))
}

#[derive(Deserialize)]
struct CreateTaskRequest {
    title: String,
    intent: String,
    #[serde(default)]
    project: Option<String>,
}

async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<Task>, ApiError> {
    let mut task = Task::new(req.title, req.intent, TaskCreator::Human);
    task.project = req.project;
    state.mgr.db().insert_task(&task).await?;
    Ok(Json(task))
}

#[derive(Deserialize)]
struct ListTasksQuery {
    status: Option<String>,
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(q): Query<ListTasksQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let status = match q.status.as_deref() {
        None => None,
        Some(s) => Some(parse_task_status(s).ok_or_else(|| {
            ApiError::bad_request(format!(
                "unknown status `{s}` (pending|in_progress|blocked|done|cancelled)"
            ))
        })?),
    };
    let tasks = state.mgr.db().list_tasks(status).await?;
    Ok(Json(serde_json::json!({ "tasks": tasks })))
}

async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: ruagent_core::TaskId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid task id"))?;
    let task = state
        .mgr
        .db()
        .get_task(id)
        .await?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    let runs = state.mgr.db().list_runs_for_task(id).await?;
    let selected_run_id = state.mgr.db().selected_run(id).await?;
    Ok(Json(
        serde_json::json!({ "task": task, "runs": runs, "selected_run_id": selected_run_id }),
    ))
}

#[derive(Deserialize)]
struct StartRunRequest {
    agent: Option<String>,
    prompt: Option<String>,
    cwd: Option<String>,
    /// Git repo to isolate this run in (worktree on a per-run branch).
    repo: Option<String>,
}

async fn start_run(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(req): Json<StartRunRequest>,
) -> Result<Json<Run>, ApiError> {
    let task_id: ruagent_core::TaskId = task_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid task id"))?;
    let task = state
        .mgr
        .db()
        .get_task(task_id)
        .await?
        .ok_or_else(|| ApiError::not_found("task not found"))?;

    // Routing provenance (design §5.4): explicit when named, cascade
    // otherwise — the cascade DECIDES the agent, not just records it.
    let (agent, decision) = if let Some(name) = req.agent.as_deref() {
        let card = state
            .mgr
            .agent(name)
            .ok_or_else(|| ApiError::bad_request(format!("unknown agent `{name}`")))?;
        (
            name.to_string(),
            ruagent_core::RoutingDecision::explicit(card.id),
        )
    } else {
        let mut t = task.clone();
        t.pinned_agent = None; // the API pin is `agent`, not the task's
        if let Some(decision) = routing_decision(&state, &t) {
            let id = decision.primary();
            let card = state
                .mgr
                .agents()
                .into_iter()
                .find(|c| c.id == id)
                .ok_or_else(|| ApiError::bad_request("routing decision named an unknown agent"))?;
            (card.name.clone(), decision)
        } else {
            let card = state.config.default_agent().ok_or_else(|| {
                ApiError::bad_request("no agent specified and no default configured")
            })?;
            (
                card.name.clone(),
                ruagent_core::RoutingDecision::default_agent(card.id),
            )
        }
    };
    let card = state
        .mgr
        .agent(&agent)
        .ok_or_else(|| ApiError::bad_request(format!("unknown agent `{agent}`")))?;

    let prompt = req.prompt.unwrap_or_else(|| task.intent.clone());
    let mcp = state
        .config
        .mcp
        .expand_profile(card.mcp_profile.as_deref(), &card.name);

    let workspace_spec = match (req.cwd.as_deref(), req.repo.as_deref()) {
        (Some(cwd), _) => WorkspaceSpec::Cwd(std::path::PathBuf::from(cwd)),
        (None, Some(repo)) => WorkspaceSpec::Worktree {
            repo: std::path::PathBuf::from(repo),
        },
        (None, None) => WorkspaceSpec::Fresh,
    };
    let run = state
        .mgr
        .start_run(&task, &agent, prompt, mcp, workspace_spec, Some(decision))
        .await?;
    Ok(Json(run))
}

#[derive(Deserialize)]
struct FanOutRequest {
    agents: Vec<String>,
    prompt: Option<String>,
    /// Git repo: every fan-out member gets its own worktree (design SS8.2).
    repo: Option<String>,
}

/// Fan-out compare (design §5.2): same prompt to N agents in parallel.
async fn start_fanout(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(req): Json<FanOutRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let task_id: ruagent_core::TaskId = task_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid task id"))?;
    let task = state
        .mgr
        .db()
        .get_task(task_id)
        .await?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    let prompt = req.prompt.unwrap_or_else(|| task.intent.clone());
    let runs = state
        .mgr
        .start_fanout(
            &task,
            &req.agents,
            prompt,
            req.repo.map(std::path::PathBuf::from),
        )
        .await?;
    Ok(Json(serde_json::json!({ "runs": runs })))
}

#[derive(Deserialize)]
struct PipelineRequest {
    steps: Vec<PipelineStepDto>,
    prompt: Option<String>,
}

#[derive(Deserialize)]
struct PipelineStepDto {
    agent: String,
    prompt: Option<String>,
}

/// Pipeline (design §5.2): sequential sub-tasks with bounded handoff.
async fn start_pipeline(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(req): Json<PipelineRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let task_id: ruagent_core::TaskId = task_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid task id"))?;
    let task = state
        .mgr
        .db()
        .get_task(task_id)
        .await?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    let base_prompt = req.prompt.unwrap_or_else(|| task.intent.clone());
    let steps: Vec<ruagent_orchestrator::PipelineStep> = req
        .steps
        .into_iter()
        .map(|s| ruagent_orchestrator::PipelineStep {
            agent: s.agent,
            prompt: s.prompt,
        })
        .collect();
    let task_ids = state.mgr.start_pipeline(&task, steps, base_prompt).await?;
    Ok(Json(serde_json::json!({ "tasks": task_ids })))
}

/// Select the winning run of a task (fan-out comparison outcome).
async fn select_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let run_id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid run id"))?;
    let run = state
        .mgr
        .db()
        .get_run(run_id)
        .await?
        .ok_or_else(|| ApiError::not_found("run not found"))?;
    if run.status != RunStatus::Completed {
        return Err(ApiError::bad_request("only completed runs can be selected"));
    }
    state.mgr.db().set_selected_run(run.task_id, run_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Run>, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid run id"))?;
    let run = state
        .mgr
        .db()
        .get_run(id)
        .await?
        .ok_or_else(|| ApiError::not_found("run not found"))?;
    Ok(Json(run))
}

/// SSE: replay the transcript from seq 0, then tail live until the run
/// finishes. Terminal marker: `event: end`.
async fn run_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let run_id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid run id"))?;
    let run = state
        .mgr
        .db()
        .get_run(run_id)
        .await?
        .ok_or_else(|| ApiError::not_found("run not found"))?;

    // Subscribe BEFORE reading the file: any event not yet in the file is
    // buffered in the broadcast receiver, and the seq check below removes
    // duplicates that are in both.
    let mut rx = state.mgr.broadcast();
    let path = transcript_path(state.mgr.transcripts_dir(), &run_id);
    let replay = ruagent_store::read_transcript(&path).unwrap_or_default();
    let terminal_at_start = run.status.is_terminal();

    let (tx, rx_stream) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut last_seq = 0u64;
        for line in replay {
            last_seq = line.seq;
            let _ = tx.send(sse_data(&line));
        }
        if terminal_at_start {
            let _ = tx.send(sse_end(run.status));
            return;
        }
        loop {
            match rx.recv().await {
                Ok(crate::runs::StreamMsg::Event { run_id: r, line }) if r == run_id => {
                    if line.seq > last_seq {
                        last_seq = line.seq;
                        let _ = tx.send(sse_data(&line));
                    }
                }
                Ok(crate::runs::StreamMsg::Finished { run_id: r, status }) if r == run_id => {
                    let _ = tx.send(sse_end(status));
                    return;
                }
                Ok(_) => {}
                Err(_) => return, // daemon shutting down
            }
        }
    });

    Ok(Sse::new(UnboundedReceiverStream::new(rx_stream)).keep_alive(KeepAlive::default()))
}

async fn list_permissions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pending: Vec<PendingPermission> = state.mgr.pending_permissions();
    Json(serde_json::json!({ "pending": pending }))
}

#[derive(Deserialize)]
struct ResolvePermissionRequest {
    /// "allow" | "reject" | "cancel"
    action: String,
}

async fn resolve_permission(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<ResolvePermissionRequest>,
) -> Result<StatusCode, ApiError> {
    let pending = state
        .mgr
        .pending_permissions()
        .into_iter()
        .find(|p| format!("{}:{}", p.run_id, p.tool_call_id) == key)
        .ok_or_else(|| ApiError::not_found("no such pending permission"))?;

    let answer = match req.action.as_str() {
        "allow" => {
            let id = pending
                .choices
                .iter()
                .find(|c| {
                    matches!(
                        c.kind,
                        ruagent_core::PermissionKind::AllowOnce
                            | ruagent_core::PermissionKind::AllowAlways
                    )
                })
                .map(|c| c.option_id.clone())
                .ok_or_else(|| ApiError::bad_request("no allow option offered"))?;
            ruagent_acp::permission::PermissionAnswer::Select(id)
        }
        "reject" => {
            let id = pending
                .choices
                .iter()
                .find(|c| {
                    matches!(
                        c.kind,
                        ruagent_core::PermissionKind::RejectOnce
                            | ruagent_core::PermissionKind::RejectAlways
                    )
                })
                .map(|c| c.option_id.clone())
                .ok_or_else(|| ApiError::bad_request("no reject option offered"))?;
            ruagent_acp::permission::PermissionAnswer::Select(id)
        }
        "cancel" => ruagent_acp::permission::PermissionAnswer::Cancel,
        other => return Err(ApiError::bad_request(format!("unknown action `{other}`"))),
    };

    state.mgr.resolve_permission(&key, answer)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Memory + knowledge (design SS6.5: the only seam external CLIs need)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct MemoryWriteRequest {
    /// profile | observation | procedure | lesson
    store: String,
    /// user | global | project:<x> | agent:<x>
    namespace: String,
    content: String,
    #[serde(default)]
    supersedes: Option<i64>,
}

async fn memory_write(
    State(state): State<AppState>,
    Json(req): Json<MemoryWriteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = match req.store.as_str() {
        "profile" => ruagent_memory::MemoryStore::Profile,
        "procedure" => ruagent_memory::MemoryStore::Procedure,
        "lesson" => ruagent_memory::MemoryStore::Lesson,
        _ => ruagent_memory::MemoryStore::Observation,
    };
    let Some(namespace) = ruagent_memory::Namespace::parse(&req.namespace) else {
        return Err(ApiError::bad_request(format!(
            "invalid namespace `{}` (user | global | project:<x> | agent:<x>)",
            req.namespace
        )));
    };
    let episode = ruagent_memory::episode::record_episode(
        state.mgr.db(),
        ruagent_memory::episode::EpisodeKind::McpWrite,
        &req.content,
        None,
    )
    .await?;
    let outcome = ruagent_memory::write_memory(
        state.mgr.db(),
        &ruagent_memory::MemoryWrite {
            store,
            namespace,
            content: req.content,
            confidence: 0.9,
            source_episode: Some(episode),
            supersedes: req.supersedes,
        },
    )
    .await?;
    Ok(Json(
        serde_json::json!({ "outcome": format!("{outcome:?}") }),
    ))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default)]
    limit: Option<u32>,
}

async fn memory_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let hits =
        ruagent_memory::query::search_fts(state.mgr.db(), &q.q, q.limit.unwrap_or(8)).await?;
    Ok(Json(serde_json::json!({ "hits": hits })))
}

async fn memory_list(
    State(state): State<AppState>,
    Query(q): Query<MemoryListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = match q.store.as_str() {
        "profile" => ruagent_memory::MemoryStore::Profile,
        "procedure" => ruagent_memory::MemoryStore::Procedure,
        "lesson" => ruagent_memory::MemoryStore::Lesson,
        _ => ruagent_memory::MemoryStore::Observation,
    };
    let namespace = q.namespace.clone().unwrap_or_else(|| "user".into());
    let hits = ruagent_memory::query::current_memories(
        state.mgr.db(),
        store,
        &namespace,
        q.limit.unwrap_or(50),
    )
    .await?;
    let counts = ruagent_memory::query::store_counts(state.mgr.db()).await?;
    Ok(Json(
        serde_json::json!({ "memories": hits, "counts": counts }),
    ))
}

#[derive(Deserialize)]
struct MemoryListQuery {
    store: String,
    namespace: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct KnowledgeIngestRequest {
    name: String,
    content: String,
}

async fn knowledge_ingest(
    State(state): State<AppState>,
    Json(req): Json<KnowledgeIngestRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let chunks = state
        .knowledge
        .ingest(&req.name, &req.content)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::json!({ "chunks": chunks })))
}

async fn knowledge_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let hits = state
        .knowledge
        .search(&q.q, q.limit.unwrap_or(8))
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::json!({ "hits": hits })))
}

/// Resolve the routing file's agent NAMES into ids and run the cascade.
fn routing_decision(state: &AppState, task: &Task) -> Option<ruagent_core::RoutingDecision> {
    let by_name: std::collections::HashMap<String, &ruagent_core::AgentCard> = state
        .mgr
        .agents()
        .into_iter()
        .map(|a| (a.name.clone(), a))
        .collect();
    let resolve = |name: &str| by_name.get(name).map(|c| c.id.to_string());
    let config = ruagent_orchestrator::RoutingConfig {
        routes: state
            .config
            .routing
            .routes
            .iter()
            .filter_map(|r| {
                resolve(&r.agent).map(|id| ruagent_orchestrator::RoutingRule {
                    project: r.project.clone(),
                    title_contains: r.title_contains.clone(),
                    agent: id,
                })
            })
            .collect(),
        default: state.config.routing.default.as_deref().and_then(resolve),
    };
    ruagent_orchestrator::route(task, &config)
}

fn parse_task_status(s: &str) -> Option<TaskStatus> {
    match s {
        "pending" => Some(TaskStatus::Pending),
        "in_progress" => Some(TaskStatus::InProgress),
        "blocked" => Some(TaskStatus::Blocked),
        "done" => Some(TaskStatus::Done),
        "cancelled" => Some(TaskStatus::Cancelled),
        _ => None,
    }
}

fn sse_data(line: &TranscriptLine) -> Result<Event, Infallible> {
    let json = serde_json::to_string(line).unwrap_or_default();
    Ok(Event::default().data(json))
}

fn sse_end(status: RunStatus) -> Result<Event, Infallible> {
    let json = serde_json::json!({ "status": status });
    Ok(Event::default().event("end").data(json.to_string()))
}
