//! HTTP API (`/api/v1`) + SSE event streaming.

use std::convert::Infallible;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use ruagent_acp::chat::ChatCommand;
use ruagent_core::{Run, RunId, RunStatus, Task, TaskCreator, TaskStatus};
use ruagent_knowledge::rrf::WeightError;
use ruagent_store::{DeleteSession, TranscriptLine, transcript_path};
use serde::Deserialize;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::config::DaemonConfig;
use crate::runs::{PendingPermission, RunManager, WorkspaceSpec};

#[derive(Clone)]
pub struct AppState {
    pub mgr: std::sync::Arc<RunManager>,
    pub config: std::sync::Arc<DaemonConfig>,
    pub knowledge: std::sync::Arc<ruagent_knowledge::Knowledge>,
    pub chats: std::sync::Arc<crate::chat::ChatManager>,
    pub sessions: std::sync::Arc<crate::sessions::SessionIndexer>,
}

/// Build the API router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/agents", get(list_agents).post(create_agent_card))
        .route(
            "/api/v1/agents/{name}",
            axum::routing::patch(update_agent_card).delete(delete_agent_card),
        )
        .route("/api/v1/agents/{name}/options", get(agent_options))
        .route("/api/v1/runtimes", post(create_runtime_card))
        .route(
            "/api/v1/runtimes/{name}",
            axum::routing::patch(update_runtime_card).delete(delete_runtime_card),
        )
        .route("/api/v1/chat/{id}/options", post(chat_option))
        .route("/api/v1/stats", get(stats))
        .route("/api/v1/mcp", get(mcp_registry))
        .route("/api/v1/skills", get(list_skills))
        .route("/api/v1/skills/sync", post(sync_skills))
        .route("/api/v1/graph/entities", get(graph_entities))
        // Batch edges: the whole graph in ONE request (row 39: K=1 ⇒ R<=4).
        .route("/api/v1/graph/edges", get(graph_edges))
        .route("/api/v1/graph/search", get(graph_search))
        .route("/api/v1/graph/entity", post(graph_create_entity))
        .route("/api/v1/graph/fact", post(graph_add_fact))
        .route(
            "/api/v1/graph/entity/{id}",
            get(graph_entity).delete(graph_entity_delete),
        )
        .route("/api/v1/graph/entity/{id}/neighbors", get(graph_neighbors))
        .route("/api/v1/graph/entity/{id}/facts", get(graph_facts))
        // DEP-4 (graph spec, wired in t19): the six multi-hop / community /
        // resolution entry points. Everything here is READ-ONLY except the three
        // explicit writes, and no write ever happens implicitly (a merge is a
        // human/agent decision, never a background side effect).
        .route("/api/v1/graph/retrieve", get(graph_retrieve))
        .route("/api/v1/graph/communities", get(graph_communities))
        .route(
            "/api/v1/graph/communities/build",
            post(graph_communities_build),
        )
        .route(
            "/api/v1/graph/community/{id}/summary",
            axum::routing::put(graph_community_summary),
        )
        .route(
            "/api/v1/graph/resolution/pending",
            get(graph_resolution_pending),
        )
        .route(
            "/api/v1/graph/resolution/merge",
            post(graph_resolution_merge),
        )
        .route("/api/v1/memory/write", post(memory_write))
        .route("/api/v1/memory/supersede", post(memory_supersede))
        .route("/api/v1/memory/diffs", get(memory_diffs))
        .route("/api/v1/memory/{id}", get(memory_get).delete(memory_delete))
        .route("/api/v1/memory/{id}/restore", post(memory_restore))
        .route("/api/v1/memory/search", get(memory_search))
        .route("/api/v1/memory/list", get(memory_list))
        .route("/api/v1/knowledge/ingest", post(knowledge_ingest))
        // KB -> graph ingestion (t4): a REAL ingest needs the free
        // `knowledge_ingest_graph` capability (OFF by default); `dry_run` prices
        // it and is always allowed.
        .route(
            "/api/v1/knowledge/graph/ingest",
            post(knowledge_graph_ingest),
        )
        .route(
            "/api/v1/knowledge/graph/ingest/status",
            get(knowledge_graph_ingest_status),
        )
        .route("/api/v1/knowledge/search", get(knowledge_search))
        .route("/api/v1/knowledge/documents", get(knowledge_documents))
        .route(
            "/api/v1/knowledge/documents/{id}",
            axum::routing::get(knowledge_document_chunks).delete(knowledge_document_delete),
        )
        .route(
            // Wildcard: document names are `/`-separated paths into the
            // knowledge tree (wiki mode M0) — `raw/wiki/deploy-guide`.
            "/api/v1/knowledge/raw/{*name}",
            get(knowledge_raw).put(knowledge_raw_put),
        )
        .route("/api/v1/knowledge/rebuild", post(knowledge_rebuild))
        .route(
            "/api/v1/knowledge/chunks/{id}",
            axum::routing::patch(knowledge_chunk_edit),
        )
        .route(
            "/api/v1/knowledge/chunks/{id}/revisions",
            get(knowledge_chunk_revisions),
        )
        .route(
            "/api/v1/knowledge/revisions/{id}/rollback",
            post(knowledge_revision_rollback),
        )
        .route("/api/v1/knowledge/expand/{chunk_id}", get(knowledge_expand))
        .route("/api/v1/knowledge/wiki/build", post(wiki_build))
        .route("/api/v1/knowledge/wiki/builds", get(wiki_builds))
        .route("/api/v1/knowledge/wiki/builds/{id}", get(wiki_build_get))
        .route("/api/v1/knowledge/wiki/pages", get(wiki_pages))
        .route(
            // The correction loop's WRITE side (R-D D.6). `wiki.rs` owns the
            // semantics (empty reason/author is refused); the endpoint owns the
            // status codes, so a refusal is a 400 that says which field.
            "/api/v1/knowledge/wiki/pages/{slug}/corrections",
            get(wiki_page_corrections).post(wiki_add_correction),
        )
        .route("/api/v1/knowledge/wiki/links", get(wiki_links))
        .route("/api/v1/tasks", post(create_task).get(list_tasks))
        .route("/api/v1/tasks/{id}", get(get_task))
        .route(
            "/api/v1/tasks/{id}",
            axum::routing::patch(update_task).delete(delete_task),
        )
        .route("/api/v1/tasks/{id}/runs", post(start_run))
        .route("/api/v1/tasks/{id}/fanout", post(start_fanout))
        .route("/api/v1/tasks/{id}/pipeline", post(start_pipeline))
        .route("/api/v1/tasks/{id}/judge", post(judge_task))
        .route("/api/v1/tasks/{id}/land", post(land_task))
        .route("/api/v1/runs/{id}", get(get_run))
        .route("/api/v1/runs/{id}/select", post(select_run))
        .route("/api/v1/runs/{id}/cancel", post(cancel_run))
        .route("/api/v1/runs/{id}/retry", post(retry_run))
        .route("/api/v1/runs/{id}/events", get(run_events))
        .route(
            "/api/v1/distill",
            get(distill_policy_get).put(distill_policy_put),
        )
        // The capability plane (docs/plans/capability-plugins-design.md §14).
        // The handlers live in `crate::capability` so this file keeps only the
        // route lines; `GET` lists every capability, `PUT` replaces the whole
        // `[capabilities]` table and swaps the live plane.
        .route(
            "/api/v1/capabilities",
            get(crate::capability::list_capabilities).put(crate::capability::update_capabilities),
        )
        .route("/api/v1/directory/pick", post(pick_directory))
        .route("/api/v1/chat", post(chat_start).get(chat_list))
        .route("/api/v1/chats", get(chats_history))
        .route("/api/v1/chat/{id}/messages", post(chat_message))
        .route("/api/v1/chat/{id}/stop", post(chat_stop))
        .route("/api/v1/chat/{id}/handoff", post(chat_handoff))
        .route("/api/v1/chat/{id}/resume", post(chat_resume))
        .route("/api/v1/chat/{id}/events", get(chat_events))
        .route(
            "/api/v1/chat/{id}",
            axum::routing::patch(chat_model).delete(chat_close),
        )
        // The history resource (plural), distinct from the live-session
        // resource (singular) above. DELETE on the plural one removes the
        // row; DELETE on the singular one only closes the session and
        // deliberately leaves the row -- which is why a closed chat stayed
        // in the list.
        .route("/api/v1/chats/{id}", axum::routing::delete(chat_delete))
        .route("/api/v1/sessions", get(sessions_list))
        .route(
            "/api/v1/sessions/{key}",
            get(sessions_messages).delete(session_delete),
        )
        // Archive = a ruagent-side hide. POST hides, DELETE unhides; the
        // indexed row and the source file it points at are never touched.
        .route(
            "/api/v1/sessions/{key}/archive",
            post(session_archive).delete(session_unarchive),
        )
        .route("/api/v1/sessions/{key}/distill", post(session_distill))
        .route("/api/v1/recall", get(recall))
        .route("/api/v1/recall/log", get(recall_log))
        // INT46-1 (t47): the route the MCP tool `memory_forget_report` proxies.
        // It did not exist -- the capability lived only in `crates/memory`'s
        // report function -- so the tool registered fine and failed on every call.
        // A consumption surface is the test of the contract: the tool found a
        // missing route on the day it was added.
        .route("/api/v1/forget-report", get(forget_report))
        .route(
            "/api/v1/memory/backfill-embeddings",
            post(memory_backfill_embeddings),
        )
        .route(
            "/api/v1/memory/migrate-distilled-prefix",
            post(memory_migrate_distilled_prefix),
        )
        .route("/api/v1/permissions", get(list_permissions))
        .route("/api/v1/permissions/{key}", post(resolve_permission))
        .with_state(state)
        .fallback_service(panel_service())
}

/// Resolve the built web panel directory.
///
/// This used to be a bare `panel/dist`, i.e. relative to the process'
/// **working directory** — so `ruagent serve` started from anywhere but
/// the repo root silently served the API with no panel (a 404 on `/`),
/// which is indistinguishable from "the panel is broken". Resolution is
/// now independent of the CWD: the first candidate that actually holds
/// an `index.html` wins.
///
///   1. `RUAGENT_PANEL_DIST` — the explicit override, as before.
///   2. next to the executable (`<exe dir>/panel/dist`) — a deployed
///      layout, where the panel ships beside the binary.
///   3. the source tree the binary was built from
///      (`<crates/daemon>/../../panel/dist`) — what a `cargo run` /
///      `cargo build` binary needs, and the case that used to break.
///   4. `./panel/dist` — the old behaviour, kept last so that a daemon
///      started from the repo root resolves exactly as it did before.
fn panel_dist_dir() -> std::path::PathBuf {
    use std::path::{Path, PathBuf};
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(explicit) = std::env::var("RUAGENT_PANEL_DIST") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("panel").join("dist"));
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    if let Some(repo) = manifest.parent().and_then(Path::parent) {
        candidates.push(repo.join("panel").join("dist"));
    }
    candidates.push(PathBuf::from("panel/dist"));
    for candidate in &candidates {
        if candidate.join("index.html").is_file() {
            return candidate.clone();
        }
    }
    // Nothing is built: report the first candidate so the log still says
    // something actionable, and let ServeDir 404 as it did before.
    candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from("panel/dist"))
}

/// Serve the built web panel (SPA) when it exists; the API works fine
/// without it. See `panel_dist_dir` for how the directory is found.
///
/// Cache policy: hashed `/assets/*` are immutable; the HTML shell is
/// `no-cache` so a rebuild always lands (a stale index.html referencing
/// a dead bundle renders a blank page).
fn panel_service() -> CacheDir {
    use tower_http::services::{ServeDir, ServeFile};
    let dist = panel_dist_dir();
    if !dist.join("index.html").is_file() {
        tracing::info!(
            dist = %dist.display(),
            cwd = %std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
            "panel not built — serving API only (cd panel && npm run build)"
        );
    } else {
        tracing::info!(dist = %dist.display(), "serving the web panel");
    }
    let inner = ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html")));
    CacheDir { inner }
}

/// Wraps the static service to add cache headers. Only reaches
/// non-API paths (the router's fallback).
#[derive(Clone)]
struct CacheDir {
    inner: tower_http::services::ServeDir<tower_http::services::ServeFile>,
}

impl tower::Service<axum::http::Request<axum::body::Body>> for CacheDir {
    type Response = axum::response::Response;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        tower::Service::<axum::http::Request<axum::body::Body>>::poll_ready(&mut self.inner, cx)
    }

    fn call(&mut self, req: axum::http::Request<axum::body::Body>) -> Self::Future {
        use tower::ServiceExt as _;
        let is_asset = req.uri().path().starts_with("/assets/");
        let fut = self.inner.clone().oneshot(req);
        Box::pin(async move {
            let resp = fut.await.map_err(|e| match e {})?;
            // into_parts + from_parts preserves headers (Content-Type!) —
            // Response::new(body) drops them, which breaks module-script
            // MIME checks and renders a blank panel.
            let (mut parts, body) = resp.into_parts();
            let cache = if is_asset {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            parts.headers.insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static(cache),
            );
            Ok(axum::response::Response::from_parts(
                parts,
                axum::body::Body::new(body),
            ))
        })
    }
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

    /// 400 with a message that names the field, id or key that was refused.
    /// `pub(crate)` because the capability handlers live in their own module
    /// (`crate::capability`) and must answer with this daemon's error shape.
    pub(crate) fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    /// The request is understood and the target exists, but this daemon
    /// refuses to act on it (the session belongs to another tool).
    fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: msg.into(),
        }
    }

    /// 409: understood and permitted, but refused on a cost/consent ground
    /// (the llm-tier capability gate, design §14.2).
    pub(crate) fn conflict(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: msg.into(),
        }
    }

    fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
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

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
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

/// THE daemon's ONE sentence for a refused recall-leg configuration (design
/// §11.5): `recall leg configuration is invalid: <WeightError>`.
///
/// `pub(crate)` because TWO doors can meet this invariant and they must answer with
/// the same words: the recall handler (read time) and the capability `PUT`
/// (`crate::capability::zero_weight_leg`, write time). The message is
/// [`WeightError`]'s own `Display`, so the leg name and the rule — including
/// "(disable the leg instead of zeroing it)" — are spelled in one place, the
/// knowledge-side weight rule, and neither door can paraphrase them.
pub(crate) fn leg_config_error(e: WeightError) -> String {
    format!("recall leg configuration is invalid: {e}")
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Liveness of the process AND of its data plane (ruagent-close-the-gaps t16).
///
/// `health` used to be a CONSTANT: `{status: ok}` whatever the store was doing.
/// When the single-writer actor died — one panicking store operation was enough —
/// every store-backed route (sessions, tasks, stats, memory, ingest) failed while
/// this endpoint kept answering ok, so a supervisor, the watchdog installed by
/// `scripts/ruagent-daemon.ps1` (at logon, every 5 minutes) and any human all
/// believed the machine was fine. That answer was worse than a crash, because a
/// crash is visible.
///
/// WHAT IT CHECKS NOW, and what a caller learns:
///   * `status: "ok"` / 200 — the writer actor is Running, so store-backed routes
///     can be served.
///   * `status: "degraded"` / **503** — the writer is Stopped: there is nothing to
///     serve store-backed routes. A supervisor should restart the daemon; a human
///     should read the log. This is the state that used to be reported as ok.
///   * `db.writer_panics` / `db.last_panic` — always present, and NON-ZERO even
///     while status is ok. The actor survives a panic raised inside a caller's
///     closure (that is the design), so `ok` stays honest — but the operation that
///     panicked did NOT happen, and this is where that is visible. Reported rather
///     than hidden: a silent recovery is the same defect one level up.
async fn health(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let db = state.mgr.db();
    let running = matches!(db.writer_state(), ruagent_store::WriterState::Running);
    let (code, status, writer) = if running {
        (StatusCode::OK, "ok", "running")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "degraded", "stopped")
    };
    (
        code,
        Json(serde_json::json!({
            "status": status,
            "service": "ruagent",
            "db": {
                "writer": writer,
                "writer_panics": db.writer_panics(),
                "last_panic": db.last_writer_panic(),
            },
        })),
    )
}

async fn stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let stats = state.mgr.db().agent_stats().await?;
    // INT-F4 (t127, additive): the two calibration versions the rest of the
    // wire already reports, so a reader of this payload can tell "worse" from
    // "embedded or scored by a different version". BOTH come from their single
    // source of truth — a literal here would be a second, silently drifting
    // copy of the value it is supposed to report. Cite the SYMBOL, not the line:
    //   * `state.knowledge.embedder_name()` — the live embedder's own name,
    //     defined by `Knowledge::embedder_name` (`crates/knowledge/src/
    //     store.rs`, `pub fn embedder_name(&self) -> &'static str`);
    //     `/api/v1/knowledge/documents` already reads the same accessor.
    //   * `ruagent_knowledge::SCORING_VERSION` — the fusion/calibration
    //     constant (`crates/knowledge/src/store.rs`, re-exported by
    //     `crates/knowledge/src/lib.rs`).
    // (The line numbers these sat on moved when this comment was inserted --
    // that is why the symbols are the citation.)
    // The pre-existing `agents` key is first and untouched.
    Ok(Json(serde_json::json!({
        "agents": stats,
        "embedder": state.knowledge.embedder_name(),
        "scoring_version": ruagent_knowledge::SCORING_VERSION,
    })))
}

// ---------------------------------------------------------------------------
// Run + task mutations (M4: Multica-parity board operations)
// ---------------------------------------------------------------------------

async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let run_id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid run id"))?;
    if state.mgr.cancel_run(run_id) {
        Ok(StatusCode::ACCEPTED)
    } else {
        // Not live: either already terminal or unknown.
        let run = state
            .mgr
            .db()
            .get_run(run_id)
            .await?
            .ok_or_else(|| ApiError::not_found("run not found"))?;
        if run.status.is_terminal() {
            Err(ApiError::bad_request(format!(
                "run already {}",
                run.status.status_str()
            )))
        } else {
            Err(ApiError::not_found("run not live"))
        }
    }
}

/// One-click retry of a dead run (design §8.3 crash row): new run,
/// same task/agent/options, workspace reused, crash context injected.
async fn retry_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Run>), ApiError> {
    let run_id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid run id"))?;
    let run = state.mgr.retry_run(run_id).await.map_err(|e| {
        let msg = format!("{e:#}");
        if msg.contains("not found") {
            ApiError::not_found(msg)
        } else {
            ApiError::bad_request(msg)
        }
    })?;
    Ok((StatusCode::CREATED, Json(run)))
}

#[derive(Deserialize)]
struct UpdateTaskRequest {
    status: String,
}

async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<StatusCode, ApiError> {
    let id: ruagent_core::TaskId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid task id"))?;
    let status = parse_task_status(&req.status).ok_or_else(|| {
        ApiError::bad_request("unknown status (pending|in_progress|blocked|done|cancelled)")
    })?;
    state.mgr.db().update_task_status(id, status).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: ruagent_core::TaskId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid task id"))?;
    // Deleting a task discards its outputs: the run workspaces and
    // worktrees go with the rows (issue #43).
    state.mgr.cleanup_task_workspaces(id).await;
    state.mgr.db().delete_task(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Memory management (M4: OpenViking-parity audit + supersede)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SupersedeRequest {
    id: i64,
    new_content: String,
}

async fn memory_supersede(
    State(state): State<AppState>,
    Json(req): Json<SupersedeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let old = ruagent_memory::query::get_memory(state.mgr.db(), req.id)
        .await?
        .ok_or_else(|| ApiError::not_found("memory not found"))?;
    if old.superseded_at.is_some() {
        return Err(ApiError::bad_request("memory already superseded"));
    }
    let outcome = ruagent_memory::write_memory(
        state.mgr.db(),
        &ruagent_memory::MemoryWrite {
            store: old.store,
            namespace: ruagent_memory::Namespace::parse(&old.namespace)
                .ok_or_else(|| ApiError::bad_request("stored namespace unparseable"))?,
            content: req.new_content,
            confidence: old.confidence,
            source_episode: None,
            supersedes: Some(old.id),
        },
    )
    .await?;
    Ok(Json(
        serde_json::json!({ "outcome": format!("{outcome:?}") }),
    ))
}

async fn memory_diffs(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let diffs = ruagent_memory::query::list_diffs(state.mgr.db(), q.limit.unwrap_or(100)).await?;
    Ok(Json(serde_json::json!({ "diffs": diffs })))
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<u32>,
}

async fn memory_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid memory id"))?;
    let m = ruagent_memory::query::get_memory(state.mgr.db(), id)
        .await?
        .ok_or_else(|| ApiError::not_found("memory not found"))?;
    Ok(Json(serde_json::json!({ "memory": m })))
}

// ---------------------------------------------------------------------------
// Knowledge documents (M4)
// ---------------------------------------------------------------------------

async fn knowledge_documents(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let docs = state
        .knowledge
        .list_documents()
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(
        serde_json::json!({ "documents": docs, "embedder": state.knowledge.embedder_name() }),
    ))
}

async fn knowledge_document_chunks(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid document id"))?;
    let chunks = state
        .knowledge
        .document_chunks(id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::json!({ "chunks": chunks })))
}

async fn knowledge_document_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid document id"))?;
    // File-backed documents take their `.md` with them — the file is
    // the source of truth, a row-only delete would be resurrected by
    // the next scan.
    state
        .knowledge
        .delete_document_with_file(id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Knowledge markdown files (source of truth) + chunk curation
// (design-study memsearch/EverOS/WeKnora)
// ---------------------------------------------------------------------------

/// The raw markdown of one document — the source of truth itself.
async fn knowledge_raw(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    match state.knowledge.read_raw(&name) {
        Ok(Some(content)) => Ok((
            [(
                axum::http::header::CONTENT_TYPE,
                "text/markdown; charset=utf-8",
            )],
            content,
        )
            .into_response()),
        Ok(None) => Err(ApiError::not_found("no such knowledge document")),
        Err(e) => Err(ApiError::bad_request(format!("{e}"))),
    }
}

#[derive(Deserialize)]
struct KnowledgeRawPut {
    content: String,
}

/// Write a document's markdown: the file lands under
/// `<root>/knowledge/<name>.md` (human-editable, git-friendly) and the
/// index follows immediately.
async fn knowledge_raw_put(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<KnowledgeRawPut>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let chunks = state
        .knowledge
        .save(&name, &req.content)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::json!({
        "chunks": chunks,
        "file": format!("{name}.md"),
    })))
}

/// Rebuild the shadow index from the markdown files (proof the index
/// is disposable).
async fn knowledge_rebuild(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let report = state
        .knowledge
        .rebuild()
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::json!({ "rebuild": report })))
}

#[derive(Deserialize)]
struct ChunkEditRequest {
    content: String,
}

/// Edit one chunk (WeKnora chunk editing): the revision is recorded,
/// the source `.md` is rewritten through the chunk's span, and the
/// document is reindexed.
async fn knowledge_chunk_edit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ChunkEditRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chunk id"))?;
    let out = state
        .knowledge
        .edit_chunk(id, &req.content)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::to_value(out).unwrap_or_default()))
}

/// Revision history of one chunk.
async fn knowledge_chunk_revisions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chunk id"))?;
    let revisions = state
        .knowledge
        .chunk_revisions(id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::json!({ "revisions": revisions })))
}

/// Roll one revision back (the rollback itself is recorded as a new
/// revision).
async fn knowledge_revision_rollback(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid revision id"))?;
    let out = state
        .knowledge
        .rollback_revision(id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::to_value(out).unwrap_or_default()))
}

/// Expand one hit into its parent section: complete context around the
/// chunk (the `expand` layer of progressive recall).
async fn knowledge_expand(
    State(state): State<AppState>,
    Path(chunk_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = chunk_id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chunk id"))?;
    let expansion = state
        .knowledge
        .expand(id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::to_value(expansion).unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Graph (M4: the graph crate finally gets its REST surface)
// ---------------------------------------------------------------------------

/// `/graph/entities` query — this route's OWN struct, so the opt-in below cannot
/// leak into the many routes that share `LimitQuery`.
///
/// `aliases=true` adds a top-level `"aliases"` map (`{ "<entity id>": ["…"] }`) for
/// the rows in THIS page. ADDITIVE, and the row shape is untouched: a consumer that
/// never sends the flag — or that ignores the field — sees byte-identical rows, and
/// the rows stay the `[entity, fact_count]` arrays they have always been.
#[derive(Deserialize)]
struct GraphEntitiesQuery {
    #[serde(default)]
    limit: Option<u32>,
    /// Absent = off. Named for what it turns on, not for the mechanism.
    #[serde(default)]
    aliases: Option<bool>,
}

async fn graph_entities(
    State(state): State<AppState>,
    Query(q): Query<GraphEntitiesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let db = state.mgr.db();
    let entities = ruagent_graph::list_entities(db, q.limit.unwrap_or(50)).await?;
    let mut body = serde_json::json!({ "entities": entities });
    if q.aliases == Some(true) {
        let ids: Vec<i64> = body["entities"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r.get(0).and_then(|e| e.get("id")).and_then(|i| i.as_i64()))
                    .collect()
            })
            .unwrap_or_default();
        body["aliases"] = serde_json::json!(ruagent_graph::aliases_for(db, &ids).await?);
    }
    Ok(Json(body))
}

/// The whole edge list in ONE request. A canvas that draws the graph must not
/// fan out one request per node (design §12 row 39: K=1 ⇒ R<=4), so the edges
/// come back together -- bounded by limit/offset with the total, so a large
/// graph is paged instead of pulled whole.
async fn graph_edges(
    State(state): State<AppState>,
    Query(q): Query<EdgeQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(500).min(5_000);
    let offset = q.offset.unwrap_or(0);
    let (edges, total) = ruagent_graph::list_edges(state.mgr.db(), limit, offset).await?;
    Ok(Json(serde_json::json!({
        "edges": edges,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

#[derive(Deserialize)]
struct EdgeQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

/// Entity search for the panel and the CLI: two passes, and the answer says
/// which one produced it.
///
/// The strict pass is the phrase pass (`search_entities`, the interface t250
/// froze): it quotes the whole query into FTS5 phrases, and a phrase requires
/// its tokens to be adjacent IN ONE COLUMN. `autohotkey-v2` therefore cannot
/// reach the entity `AutoHotkey` whose summary says `v2.0.28` -- name and
/// summary are different columns -- so the panel answered "no such entity" for
/// an entity that was right there (t312 measured it; t320 fixes it here).
///
/// When the strict pass finds nothing, the loose pass runs, and its results are
/// reported as `candidate` rather than as hits: loose is a WIDENING pass (AND
/// over tokens, then a single-token prefix OR, then LIKE), so it also answers
/// for a name that does not exist at all (`zzz-not-a-real-name` matches on
/// `"name"*`). Presenting those as plain hits would trade a silent miss for
/// a silent false positive; `match` says which leg spoke.
async fn graph_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let db = state.mgr.db();
    let strict = ruagent_graph::search_entities(db, &q.q, 10).await?;
    let (entities, matched) = if !strict.is_empty() {
        (strict, "exact")
    } else {
        // An empty query has nothing to widen to: it is "none", not a fallback.
        let loose = if q.q.trim().is_empty() {
            Vec::new()
        } else {
            ruagent_graph::search_entities_loose(db, &q.q, 10).await?
        };
        if loose.is_empty() {
            (loose, "none")
        } else {
            (loose, "candidate")
        }
    };
    Ok(Json(
        serde_json::json!({ "entities": entities, "match": matched }),
    ))
}

#[derive(Deserialize)]
struct CreateEntityRequest {
    name: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

// ---------------------------------------------------------------------------
// Wiki mode (design docs/plans/2026-09-15-wiki-mode-design.md)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct WikiBuildRequest {
    /// "all" | "changed" | an array of document names. Default: changed.
    #[serde(default)]
    scope: Option<serde_json::Value>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    agent: Option<String>,
    /// Reuse the stored plan of a dry-run build (the confirm step).
    #[serde(default)]
    confirm_plan: Option<i64>,
}

/// Query parameters on the wiki build endpoint. There are none: every field of
/// `WikiBuildRequest` is a BODY field, and a parameter of that name in the query
/// string is REFUSED rather than ignored (t300).
///
/// The failure this replaces is measured, not imagined: `POST
/// /api/v1/knowledge/wiki/build?dry_run=true` with body `{}` answered 202 with a
/// body describing a plan, while the row it wrote was `dry_run=0,
/// status='done'` (build 6 in the live store) — a success code over a
/// production build, from a caller that had asked for a plan. An ignored
/// parameter is worse than a refused one: nothing in the response tells the
/// caller that half of its request never arrived.
#[derive(Deserialize)]
struct WikiBuildQuery {
    #[serde(default)]
    dry_run: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    confirm_plan: Option<String>,
}

fn parse_wiki_scope(v: Option<serde_json::Value>) -> Result<crate::wiki::Scope, ApiError> {
    match v {
        None => Ok(crate::wiki::Scope::Changed),
        Some(serde_json::Value::String(s)) => match s.as_str() {
            "all" => Ok(crate::wiki::Scope::All),
            "changed" => Ok(crate::wiki::Scope::Changed),
            other => Err(ApiError::bad_request(format!(
                "unknown scope `{other}` (all | changed | [names])"
            ))),
        },
        Some(serde_json::Value::Array(items)) => {
            let mut names = Vec::new();
            for item in items {
                let Some(name) = item.as_str() else {
                    return Err(ApiError::bad_request(
                        "scope array must contain document names",
                    ));
                };
                names.push(name.to_string());
            }
            if names.is_empty() {
                return Err(ApiError::bad_request("scope array is empty"));
            }
            Ok(crate::wiki::Scope::Names(names))
        }
        Some(_) => Err(ApiError::bad_request(
            "scope must be \"all\" | \"changed\" | [names]",
        )),
    }
}

/// Compile source documents into wiki pages.
///
/// BODY fields — all of them: `scope` (`"all"` | `"changed"` | `[names]`, default
/// `"changed"`) · `dry_run` (plan and return the page set for review: the
/// plan-level human gate) · `agent` · `confirm_plan` (execute a reviewed plan).
///
/// The QUERY string is not a second way to say any of those: a query parameter
/// named `dry_run`/`scope`/`agent`/`confirm_plan` is a 400 that names it (t300). It used
/// to be accepted and silently ignored, so `?dry_run=true` returned a success
/// code and wrote a production row.
///
/// A body without `scope` is legal (it means "changed"), but a scope that
/// selects no documents is not: the build fails with `no source documents in
/// the knowledge base`, or `scope selects no source documents` when the scope
/// itself names none.
async fn wiki_build(
    State(state): State<AppState>,
    Query(q): Query<WikiBuildQuery>,
    Json(req): Json<WikiBuildRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let mut ignored: Vec<&str> = Vec::new();
    if q.dry_run.is_some() {
        ignored.push("dry_run");
    }
    if q.scope.is_some() {
        ignored.push("scope");
    }
    if q.agent.is_some() {
        ignored.push("agent");
    }
    if q.confirm_plan.is_some() {
        ignored.push("confirm_plan");
    }
    if !ignored.is_empty() {
        return Err(ApiError::bad_request(format!(
            "these query parameters are not read: {}. Send them as JSON body fields — the query string used to be accepted and silently ignored, which is how `?dry_run=true` returned a success code over a production build (t300)",
            ignored.join(", ")
        )));
    }
    let scope = parse_wiki_scope(req.scope)?;
    let distiller = crate::distill::Distiller {
        db: state.mgr.db().clone(),
        root: state.config.root.clone(),
        embedder: None,
        registry: state.mgr.registry_view(),
        // The wiki pipeline has its own prompts; the distill language
        // override is about memories, not wiki pages. It never calls
        // `distill`, so the graph flag is just the default.
        language: None,
        prompt_override: None,
        graph: true,
    };
    let builder = crate::wiki::WikiBuilder::new(distiller, state.knowledge.as_ref().clone());
    let out = builder
        .start_build(crate::wiki::BuildRequest {
            scope,
            dry_run: req.dry_run.unwrap_or(false),
            agent: req.agent,
            confirm_plan: req.confirm_plan,
        })
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    let status = if out.status == "running" {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(serde_json::to_value(&out).unwrap_or_default())))
}

async fn wiki_builds(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let builds = state
        .mgr
        .db()
        .call(
            move |conn| -> Result<Vec<serde_json::Value>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT id, scope, status, dry_run, agent, pages_planned, pages_written,
                        pages_failed, error, started_at, finished_at
                   FROM wiki_builds ORDER BY id DESC LIMIT ?1",
                )?;
                let rows = stmt
                    .query_map([limit], |r| {
                        Ok(serde_json::json!({
                            "id": r.get::<_, i64>(0)?,
                            "scope": r.get::<_, String>(1)?,
                            "status": r.get::<_, String>(2)?,
                            "dry_run": r.get::<_, i64>(3)? != 0,
                            "agent": r.get::<_, String>(4)?,
                            "pages_planned": r.get::<_, i64>(5)?,
                            "pages_written": r.get::<_, i64>(6)?,
                            "pages_failed": r.get::<_, i64>(7)?,
                            "error": r.get::<_, Option<String>>(8)?,
                            "started_at": r.get::<_, String>(9)?,
                            "finished_at": r.get::<_, Option<String>>(10)?,
                        }))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            },
        )
        .await??;
    Ok(Json(serde_json::json!({ "builds": builds })))
}

async fn wiki_build_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid build id"))?;
    let build = state
        .mgr
        .db()
        .call(
            move |conn| -> Result<Option<serde_json::Value>, rusqlite::Error> {
                conn.query_row(
                    "SELECT id, scope, status, dry_run, agent, pages_planned, pages_written,
                        pages_failed, plan_json, error, started_at, finished_at
                   FROM wiki_builds WHERE id = ?1",
                    [id],
                    |r| {
                        Ok(serde_json::json!({
                            "id": r.get::<_, i64>(0)?,
                            "scope": r.get::<_, String>(1)?,
                            "status": r.get::<_, String>(2)?,
                            "dry_run": r.get::<_, i64>(3)? != 0,
                            "agent": r.get::<_, String>(4)?,
                            "pages_planned": r.get::<_, i64>(5)?,
                            "pages_written": r.get::<_, i64>(6)?,
                            "pages_failed": r.get::<_, i64>(7)?,
                            "plan": serde_json::from_str::<serde_json::Value>(
                                &r.get::<_, Option<String>>(8)?.unwrap_or_default()
                            ).ok(),
                            "error": r.get::<_, Option<String>>(9)?,
                            "started_at": r.get::<_, String>(10)?,
                            "finished_at": r.get::<_, Option<String>>(11)?,
                        }))
                    },
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })
            },
        )
        .await??;
    let Some(build) = build else {
        return Err(ApiError::not_found("no such wiki build"));
    };
    let pages = state
        .mgr
        .db()
        .call(
            move |conn| -> Result<Vec<serde_json::Value>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT slug, action, status, error FROM wiki_build_pages
                  WHERE build_id = ?1 ORDER BY slug",
                )?;
                let rows = stmt
                    .query_map([id], |r| {
                        Ok(serde_json::json!({
                            "slug": r.get::<_, String>(0)?,
                            "action": r.get::<_, String>(1)?,
                            "status": r.get::<_, String>(2)?,
                            "error": r.get::<_, Option<String>>(3)?,
                        }))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            },
        )
        .await??;
    Ok(Json(serde_json::json!({ "build": build, "pages": pages })))
}

/// The wiki page inventory (design §9.1): slug/title/aliases/entities/
/// sources with stale + edited markers and link counts.
async fn wiki_pages(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let pages = crate::wiki::pages(state.mgr.db(), state.knowledge.as_ref()).await;
    Ok(Json(serde_json::json!({ "pages": pages })))
}

/// The wiki link graph: nodes, edges, broken (wanted pages) and orphans.
async fn wiki_links(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        serde_json::to_value(crate::wiki::links(state.knowledge.as_ref())).unwrap_or_default(),
    ))
}

/// The corrections recorded for one page, newest first (R-D D.6). Reading is
/// unconditional: an empty list is a reading, not a 404 — "no correction" and
/// "no such page" are different facts and this endpoint only knows the first.
async fn wiki_page_corrections(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rows = crate::wiki::corrections(state.mgr.db(), &slug).await;
    Ok(Json(
        serde_json::json!({ "slug": slug, "corrections": rows }),
    ))
}

#[derive(Deserialize)]
struct AddCorrectionRequest {
    /// `pin` | `release` | `note`.
    kind: String,
    reason: String,
    /// Who is recording it. `None`/empty is refused (`wiki::add_correction`).
    #[serde(default)]
    author: Option<String>,
}

/// Record a correction (pin / release / note). The refusals keep their identity:
/// an unknown kind names the accepted vocabulary, and an empty reason/author is a
/// 400 carrying the storage layer's own sentence — never a silent insert with a
/// blank "why", which is the whole reason this table exists.
async fn wiki_add_correction(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<AddCorrectionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(kind) = crate::wiki::CorrectionKind::parse(req.kind.trim()) else {
        let accepted: Vec<&str> = crate::wiki::CorrectionKind::ALL
            .iter()
            .map(|k| k.as_str())
            .collect();
        return Err(ApiError::bad_request(format!(
            "unknown correction kind `{}`; accepted: {}",
            req.kind,
            accepted.join(", ")
        )));
    };
    let c = crate::wiki::Correction {
        slug: slug.clone(),
        kind,
        reason: req.reason,
        author: req.author.unwrap_or_default(),
        at: String::new(), // the writer stamps it
    };
    match crate::wiki::add_correction(state.mgr.db(), &c).await {
        Ok(id) => Ok(Json(serde_json::json!({
            "id": id,
            "correction": {
                "slug": c.slug,
                "kind": kind.as_str(),
                "reason": c.reason.trim(),
                "author": c.author,
            }
        }))),
        Err(e) => Err(ApiError::bad_request(e.to_string())),
    }
}

async fn graph_create_entity(
    State(state): State<AppState>,
    Json(req): Json<CreateEntityRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = ruagent_graph::upsert_entity(
        state.mgr.db(),
        &req.name,
        req.kind.as_deref(),
        req.summary.as_deref(),
    )
    .await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

#[derive(Deserialize)]
struct AddFactRequest {
    src: i64,
    dst: i64,
    relation: String,
    fact_text: String,
    /// RFC3339 or ISO date; None = now.
    #[serde(default)]
    valid_at: Option<String>,
}

async fn graph_add_fact(
    State(state): State<AppState>,
    Json(req): Json<AddFactRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = ruagent_graph::add_fact(
        state.mgr.db(),
        req.src,
        req.dst,
        &req.relation,
        &req.fact_text,
        req.valid_at.as_deref(),
        None,
    )
    .await?;
    Ok(Json(serde_json::json!({ "id": id })))
}

/// The entity, its aliases and its facts.
///
/// `name`, `kind` and `aliases` are ADDITIVE (ruagent-close-the-gaps t22): a caller
/// that reads only `facts` keeps working, and a caller holding an id can now read
/// back what it is looking at. `aliases` is the ONLY response that carries the alias
/// text, and that text is what makes a search answer verifiable rather than a guess.
///
/// THE COUNT THIS EXISTS FOR, measured on the ingested corpus copy (t18), because
/// the earlier premise said something else: of 257 distinct parenthetical entity
/// names, **116 are an `entities.name`, 141 are alias-only** — 103 that no strict
/// `/graph/search` returns at all, and 38 whose `match="exact"` is a FALSE POSITIVE
/// on a DIFFERENT entity (the strict pass matches each token against
/// `entities_fts(name, summary)`, so a name can "hit" through another entity's
/// summary). All 141 are rows in `entity_aliases`; none are lost. A verification
/// judged against "116 visible + 38 found" would be measuring a different quantity.
async fn graph_entity(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid entity id"))?;
    let db = state.mgr.db();
    let entity = ruagent_graph::entity_by_id(db, id).await?;
    let aliases = ruagent_graph::aliases_of(db, id).await?;
    let facts = ruagent_graph::current_facts(db, id).await?;
    // An id that is not in the graph keeps today's answer (HTTP 200 with empty
    // collections) rather than becoming a 404: this response only gained fields.
    Ok(Json(serde_json::json!({
        "name": entity.as_ref().map(|e| e.name.clone()),
        "kind": entity.as_ref().and_then(|e| e.kind.clone()),
        "aliases": aliases,
        "facts": facts,
    })))
}

/// Hard-delete an entity, its edges and its facts (t276). The graph page has
/// no other way to correct a wrong entity, and a row-only delete would leave
/// it drawing dangling edges. 404 for an id that is not there — never a
/// silent 200.
async fn graph_entity_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid entity id"))?;
    match ruagent_graph::delete_entity(state.mgr.db(), id).await? {
        ruagent_graph::EntityDeleteOutcome::Deleted {
            id,
            edges_removed,
            facts_removed,
        } => Ok(Json(serde_json::json!({
            "outcome": "deleted",
            "id": id,
            "edges_removed": edges_removed,
            "facts_removed": facts_removed,
        }))),
        ruagent_graph::EntityDeleteOutcome::NotFound => {
            Err(ApiError::not_found("entity not found"))
        }
    }
}

#[derive(Deserialize)]
struct NeighborsQuery {
    #[serde(default)]
    hops: Option<u32>,
}

async fn graph_neighbors(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<NeighborsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid entity id"))?;
    let hops = q.hops.unwrap_or(2).min(4);
    let neighbors = ruagent_graph::neighbors(state.mgr.db(), id, hops).await?;
    Ok(Json(
        serde_json::json!({ "neighbors": neighbors, "hops": hops }),
    ))
}

#[derive(Deserialize)]
struct FactsQuery {
    /// "What was true as of X" (RFC3339/ISO date); absent = current.
    #[serde(default)]
    at: Option<String>,
}

async fn graph_facts(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<FactsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid entity id"))?;
    // t57 (F2; V-INT's residual of V-C F-1 / RVC-1): normalize `at` at the ENTRY,
    // exactly as `retrieve?as_of=` has since t27. `facts_as_of` compares
    // timestamps as text, so an unnormalized spelling silently selects a
    // different edge set -- and a non-instant used to be accepted with a 200
    // (measured t57: `not-a-time` / `2026-13-45T99:99:99Z` / `now` => 200).
    // Now a non-instant is a 400; it is never compared as a string.
    let facts = match &q.at {
        Some(at) => {
            let dt = ruagent_graph::parse_ts(at)
                .ok_or_else(|| ApiError::bad_request("at must be an RFC3339 instant"))?;
            ruagent_graph::facts_as_of(state.mgr.db(), id, &dt.to_rfc3339()).await?
        }
        None => ruagent_graph::current_facts(state.mgr.db(), id).await?,
    };
    Ok(Json(serde_json::json!({ "facts": facts })))
}

// ---------------------------------------------------------------------------
// DEP-4 (graph spec; wired in t19): multi-hop retrieval, communities, resolution.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GraphRetrieveQuery {
    q: String,
    #[serde(default)]
    hops: Option<u32>,
    #[serde(default)]
    beam: Option<u32>,
    #[serde(default)]
    max_paths: Option<u32>,
    #[serde(default)]
    max_facts: Option<u32>,
    /// "What was true at this instant". Normalised at THIS entrance (RVC-1's
    /// rule, restated for the HTTP face): three spellings of one instant must
    /// produce byte-identical evidence, so the string is parsed once and
    /// re-rendered in one canonical form before it reaches the graph. A string
    /// that is not an instant is a 400, never a silent string comparison.
    #[serde(default)]
    as_of: Option<String>,
    #[serde(default)]
    include_superseded: Option<bool>,
}

/// `GET /api/v1/graph/retrieve?q=&hops=` — the ONLY multi-hop entry point.
async fn graph_retrieve(
    State(state): State<AppState>,
    Query(q): Query<GraphRetrieveQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut query = ruagent_graph::GraphQuery::for_text(q.q.clone());
    if let Some(h) = q.hops {
        query.hops = h;
    }
    if let Some(b) = q.beam {
        query.beam = b;
    }
    if let Some(m) = q.max_paths {
        query.max_paths = m;
    }
    if let Some(m) = q.max_facts {
        query.max_facts = m;
    }
    if let Some(s) = q.include_superseded {
        query.include_superseded = s;
    }
    query.as_of = match q.as_of.as_deref() {
        None => None,
        Some(raw) => match ruagent_graph::parse_ts(raw) {
            Some(dt) => Some(dt.to_rfc3339()),
            None => {
                return Err(ApiError::bad_request(format!(
                    "as_of `{raw}` is not an RFC3339 instant"
                )));
            }
        },
    };
    let evidence = ruagent_graph::retrieve(state.mgr.db(), &query).await?;
    Ok(Json(serde_json::json!({
        "seeds": evidence.seeds,
        "paths": evidence.paths,
        "stats": evidence.stats,
        // Stated where it can be read: `graph_edges` is the graph's SIZE (it does
        // not move with `as_of`) — contract §1.5, RV-C's reminder.
        "stats_note": "graph_edges is the graph's total size, not the edge count at as_of",
    })))
}

#[derive(Deserialize)]
struct LevelQuery {
    #[serde(default)]
    level: Option<u32>,
}

/// `GET /api/v1/graph/communities?level=` — `null` means NEVER BUILT.
async fn graph_communities(
    State(state): State<AppState>,
    Query(q): Query<LevelQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let level = q.level.unwrap_or(0);
    let found = ruagent_graph::communities(state.mgr.db(), level).await?;
    let built = found.is_some();
    let (entities_covered, non_isolated) =
        ruagent_graph::community_coverage(state.mgr.db(), level).await?;
    Ok(Json(serde_json::json!({
        // `null` (never built) is NOT `[]` (built, no communities): a consumer
        // that renders both as "0 communities" invents a partition.
        "communities": found,
        "built": built,
        "coverage": {
            "entities_covered": entities_covered,
            "non_isolated": non_isolated,
        },
    })))
}

/// `POST /api/v1/graph/communities/build` — one explicit (re)build of one level.
async fn graph_communities_build(
    State(state): State<AppState>,
    Query(q): Query<LevelQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let level = q.level.unwrap_or(0);
    let build = ruagent_graph::build_communities(state.mgr.db(), level).await?;
    Ok(Json(serde_json::json!(build)))
}

#[derive(Deserialize)]
struct SummaryBody {
    summary: Option<String>,
}

/// `PUT /api/v1/graph/community/{id}/summary` — an empty summary is a 400; a
/// summary for a community that does not exist is a 404 (`updated: false` must
/// not be reported as a successful 200).
async fn graph_community_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SummaryBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid community id"))?;
    let summary = body.summary.unwrap_or_default();
    if summary.trim().is_empty() {
        return Err(ApiError::bad_request(
            "summary must not be empty (clearing a summary is not a summary)",
        ));
    }
    let updated = ruagent_graph::set_community_summary(state.mgr.db(), id, &summary).await?;
    if !updated {
        return Err(ApiError::not_found(format!("no community {id}")));
    }
    Ok(Json(serde_json::json!({ "id": id, "updated": updated })))
}

/// `GET /api/v1/graph/resolution/pending` — two SEPARATE readings: the queued
/// pairs (a decision is waiting) and the pairs that are already the same object
/// under two rows (a merge was missed).
async fn graph_resolution_pending(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pending = ruagent_graph::pending_pairs(state.mgr.db()).await?;
    let redundant = ruagent_graph::redundant_pairs(state.mgr.db()).await?;
    Ok(Json(serde_json::json!({
        "pending": pending,
        "redundant": redundant,
        "pending_count": pending.len(),
        "redundant_count": redundant.len(),
    })))
}

#[derive(Deserialize)]
struct MergeBody {
    keeper: i64,
    absorbed: i64,
}

/// `POST /api/v1/graph/resolution/merge` — EXPLICIT, never automatic.
async fn graph_resolution_merge(
    State(state): State<AppState>,
    Json(body): Json<MergeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.keeper == body.absorbed {
        return Err(ApiError::bad_request(
            "keeper and absorbed are the same entity",
        ));
    }
    // Existence is checked HERE, so "one of them is gone" is a 404 rather than a
    // `moved: 0` that reads like a no-op merge.
    let names: std::collections::HashMap<i64, String> = {
        let ids = vec![body.keeper, body.absorbed];
        state
            .mgr
            .db()
            .call(
                move |conn| -> Result<std::collections::HashMap<i64, String>, ruagent_store::DbError> {
                    let mut out = std::collections::HashMap::new();
                    for id in ids {
                        let name: Option<String> = conn
                            .query_row("SELECT name FROM entities WHERE id = ?1", [id], |r| r.get(0))
                            .ok();
                        if let Some(n) = name {
                            out.insert(id, n);
                        }
                    }
                    Ok(out)
                },
            )
            .await??
    };
    if !names.contains_key(&body.keeper) {
        return Err(ApiError::not_found(format!("no entity {}", body.keeper)));
    }
    let absorbed_name = match names.get(&body.absorbed) {
        Some(n) => n.clone(),
        None => return Err(ApiError::not_found(format!("no entity {}", body.absorbed))),
    };
    let moved = ruagent_graph::merge_entities(state.mgr.db(), body.keeper, body.absorbed).await?;
    // The absorbed name must become an alias of the keeper, or the next scan
    // rebuilds the entity we just merged away.
    let alias_written =
        ruagent_graph::add_alias(state.mgr.db(), body.keeper, &absorbed_name, "merge").await?;
    Ok(Json(serde_json::json!({
        "keeper": body.keeper,
        "absorbed": body.absorbed,
        "moved": moved,
        "alias": absorbed_name,
        "alias_written": alias_written,
    })))
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
    // Live health per server (issue #24); null = never checked.
    let health = state.config.mcp.health.snapshot();
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
                "health": health.get(name),
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

// ---------------------------------------------------------------------------
// Registry editing (runtimes + roles): agents.toml via toml_edit, then a
// hot reload — the daemon never needs a restart for a registry change.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RuntimeCardRequest {
    #[serde(default)]
    harness: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mcp_profile: Option<String>,
    #[serde(default)]
    models: Option<Vec<String>>,
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct CreateRuntimeRequest {
    name: String,
    #[serde(flatten)]
    fields: RuntimeCardRequest,
}

impl RuntimeCardRequest {
    fn patch(&self) -> crate::registry::RuntimePatch {
        crate::registry::RuntimePatch {
            harness: self.harness.clone(),
            command: self.command.clone(),
            description: self.description.clone(),
            mcp_profile: self.mcp_profile.clone(),
            models: self.models.clone(),
            enabled: self.enabled,
        }
    }
}

/// Re-parse agents.toml, re-resolve stable DB ids, swap the live
/// registry. Every successful edit runs through this.
async fn reload_registry(state: &AppState) -> Result<Vec<ruagent_core::AgentCard>, ApiError> {
    let text = std::fs::read_to_string(state.mgr.registry().path())
        .map_err(|e| ApiError::internal(format!("re-reading agents.toml: {e}")))?;
    let mut cards = crate::config::parse_agents(&text)
        .map_err(|e| ApiError::internal(format!("re-parsing agents.toml: {e}")))?;
    for card in &mut cards {
        if let Some(id) = state.mgr.db().agent_id_by_name(&card.name).await? {
            card.id = id;
        }
        state.mgr.db().upsert_agent(card).await?;
    }
    state.mgr.reload_agents(cards.clone());
    Ok(cards)
}

async fn create_runtime_card(
    State(state): State<AppState>,
    Json(req): Json<CreateRuntimeRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let command = req
        .fields
        .command
        .clone()
        .filter(|c| !c.trim().is_empty())
        .ok_or_else(|| ApiError::bad_request("a runtime needs a spawn command"))?;
    // Validate the harness BEFORE writing anything to the file.
    crate::config::harness_of(&req.name, req.fields.harness.as_deref())
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    state
        .mgr
        .registry()
        .create_runtime(&req.name, &req.fields.patch())
        .map_err(registry_error)?;
    let _ = command;
    let cards = reload_registry(&state).await?;
    let card = cards
        .iter()
        .find(|c| c.name == req.name)
        .ok_or_else(|| ApiError::internal("runtime vanished after create"))?;
    Ok((StatusCode::CREATED, Json(card_json(card))))
}

async fn update_runtime_card(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<RuntimeCardRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(h) = req.harness.as_deref() {
        crate::config::harness_of(&name, Some(h))
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
    }
    state
        .mgr
        .registry()
        .update_runtime(&name, &req.patch())
        .map_err(registry_error)?;
    let cards = reload_registry(&state).await?;
    let card = cards
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| ApiError::internal("runtime vanished after update"))?;
    Ok(Json(card_json(card)))
}

async fn delete_runtime_card(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    // A runtime in use by any role is a conflict, not a bad request.
    state
        .mgr
        .registry()
        .delete_runtime(&name)
        .map_err(registry_error)?;
    reload_registry(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct AgentCardRequest {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    mcp_profile: Option<String>,
    #[serde(default)]
    runtimes: Option<Vec<String>>,
    #[serde(default)]
    runtime: Option<String>,
    /// Canonical session-option defaults (`options = { mode = "plan" }`).
    /// `Some(empty)` clears them.
    #[serde(default)]
    options: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct CreateAgentRequest {
    name: String,
    #[serde(flatten)]
    fields: AgentCardRequest,
}

impl AgentCardRequest {
    fn patch(&self) -> crate::registry::AgentPatch {
        crate::registry::AgentPatch {
            prompt: self.prompt.clone(),
            description: self.description.clone(),
            model: self.model.clone(),
            mcp_profile: self.mcp_profile.clone(),
            runtimes: self.runtimes.clone(),
            runtime: self.runtime.clone(),
            options: self.options.clone(),
            enabled: self.enabled,
        }
    }
}

async fn create_agent_card(
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if req
        .fields
        .prompt
        .as_deref()
        .is_none_or(|p| p.trim().is_empty())
    {
        return Err(ApiError::bad_request(
            "a role needs a prompt — that is what makes it a role",
        ));
    }
    // runtimes = the full list when given; else the single default.
    let mut runtimes = req.fields.runtimes.clone().unwrap_or_default();
    if let Some(default) = req.fields.runtime.as_deref() {
        let owned = default.to_string();
        if !runtimes.contains(&owned) {
            runtimes.push(owned);
        }
    }
    if runtimes.is_empty() {
        return Err(ApiError::bad_request(
            "a role needs at least one runtime — create one on the runtimes page first",
        ));
    }
    state
        .mgr
        .registry()
        .create_agent(&req.name, &req.fields.patch())
        .map_err(registry_error)?;
    let cards = reload_registry(&state).await?;
    let card = cards
        .iter()
        .find(|c| c.name == req.name)
        .ok_or_else(|| ApiError::internal("agent vanished after create"))?;
    Ok((StatusCode::CREATED, Json(card_json(card))))
}

async fn update_agent_card(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<AgentCardRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .mgr
        .registry()
        .update_agent(&name, &req.patch())
        .map_err(registry_error)?;
    let cards = reload_registry(&state).await?;
    let card = cards
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| ApiError::internal("agent vanished after update"))?;
    Ok(Json(card_json(card)))
}

async fn delete_agent_card(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .mgr
        .registry()
        .delete_agent(&name)
        .map_err(registry_error)?;
    reload_registry(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Registry edits fail as `anyhow` errors — "not found" and "is used by"
/// read as 404/409, everything else as 400.
fn registry_error(e: anyhow::Error) -> ApiError {
    let msg = format!("{e:#}");
    if msg.contains("not found") {
        ApiError::not_found(msg)
    } else if msg.contains("is used by") {
        ApiError::conflict(msg)
    } else {
        ApiError::bad_request(msg)
    }
}

async fn list_agents(State(state): State<AppState>) -> Json<serde_json::Value> {
    let agents: Vec<serde_json::Value> = state.mgr.agents().iter().map(card_json).collect();
    Json(serde_json::json!({ "agents": agents }))
}

/// The agent-card JSON shape (list + registry create/update replies).
fn card_json(a: &ruagent_core::AgentCard) -> serde_json::Value {
    serde_json::json!({
        "id": a.id.to_string(),
        "name": a.name,
        "harness": format!("{:?}", a.harness),
        "models": a.models,
        "reasoning_effort": a.reasoning_effort,
        "description": a.description,
        "model": a.model,
        "enabled": a.enabled,
        "runtime": a.runtime,
        "runtimes": a.runtimes,
        "options": a.options,
        // Full prompt, no truncation: the edit modal round-trips this
        // value — a truncated copy would destroy the real prompt on
        // save (found via the user's "description and prompt incomplete"
        // report: every role showed exactly 160 chars).
        "prompt": a.prompt,
        // Two-layer model (design §4.1, user ruling 2026-09-17):
        // a card with a role prompt or runtime references is a
        // ROLE; a bare harness instance (legacy single-layer
        // entry, or a [runtime.*] card) IS a runtime.
        "kind": if a.prompt.is_some() || a.runtime.is_some() || !a.runtimes.is_empty() {
            "role"
        } else {
            "runtime"
        },
        "command": a.command,
    })
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
    let (selected_run_id, selected_by) = state
        .mgr
        .db()
        .selected_run(id)
        .await?
        .map(|(run_id, by)| (Some(run_id), Some(by)))
        .unwrap_or((None, None));
    let judgement = judge_view(state.mgr.db(), id, &runs).await?;
    // Landable (design §5.2): the winner still has its worktree — the
    // branch can be merged back into the main repo. A fresh/cwd
    // workspace or an already-landed (swept) worktree is not.
    let worktrees = state.mgr.root().join("worktrees");
    let landable = selected_run_id.is_some_and(|rid| {
        runs.iter().any(|r| {
            r.id == rid
                && r.workspace.as_deref().is_some_and(|ws| {
                    let p = std::path::Path::new(ws);
                    p.starts_with(&worktrees) && p.is_dir()
                })
        })
    });
    // A run parked on the inbox shows it here: the task view must say
    // "waiting for permission", not look hung (issue #34).
    let pending = state.mgr.pending_permissions();
    let runs_json: Vec<serde_json::Value> = runs
        .iter()
        .map(|r| {
            let mut v = serde_json::to_value(r).unwrap_or_default();
            if let Some(p) = pending.iter().find(|p| p.run_id == r.id) {
                v["waiting_permission"] = serde_json::json!({
                    "tool_call_id": p.tool_call_id,
                    "title": p.title,
                });
            } else {
                v["waiting_permission"] = serde_json::Value::Null;
            }
            v
        })
        .collect();
    Ok(Json(serde_json::json!({
        "task": task,
        "runs": runs_json,
        "selected_run_id": selected_run_id,
        "selected_by": selected_by,
        "landable": landable,
        "judgement": judgement,
    })))
}

#[derive(Deserialize)]
struct StartRunRequest {
    agent: Option<String>,
    prompt: Option<String>,
    cwd: Option<String>,
    /// Git repo to isolate this run in (worktree on a per-run branch).
    repo: Option<String>,
    /// Canonical session-option defaults (issue #36): `mode` (permission
    /// mode) / `effort` (thinking level), applied after session/new.
    #[serde(default)]
    options: std::collections::BTreeMap<String, String>,
}

/// The agent for the no-cascade fallback: the first ENABLED card of the REGISTRY the run resolves
/// against (t38) -- never the config-file view's card, whose `AgentId` is minted fresh on every
/// load (`config.rs:200/225/265/307`).
///
/// MEASURED, and why this exists: with `routing.toml` commented out (this machine's live shape)
/// every run takes this fallback. The old body read `state.config.default_agent()` and recorded
/// ITS id, so a run's `routed` event named an agent that exists in no registry and in no table --
/// `01a0f580-b2ac-7048-a136-6bbe33d5f13b` in the live transcript, while the same run's
/// `params.agent` was `01a0b915-8273-73c4-8fe9-0e35a34d4ded` (approver, resolved by name through
/// the manager a few lines below). The two views are both legitimate; only the registry's id is
/// resolvable, so that is the one to record.
///
/// The RULE is still "the first enabled card", the rule the config-file view's `default_agent()` used
/// -- which t43 deleted, this being its only replacement. It changes which ID is recorded for the
/// default agent, not which agent is the default -- except for the t43 guard below.
///
/// t43: the last-resort fallback must NOT be the card the permission cascade designates as its
/// APPROVER. `approver` is that card's ID, resolved from `policy.toml`'s
/// `[permissions.approver].agent` by NAME against this same registry -- the resolution
/// `RunManager::new` computes for tier 2 (`runs.rs:253-256`, exposed as `RunManager::approver_id`) --
/// and NOT a `kind` test (a derived presentation attribute), a name convention, or a prompt substring.
/// Only the last-resort fallback consults it: a rule naming the adjudicator, a pin naming it, or a
/// `routing.toml` `default` naming it are the operator's deliberate choices and never reach here.
fn default_route_agent(
    cards: &[ruagent_core::AgentCard],
    approver: Option<ruagent_core::AgentId>,
) -> Option<ruagent_core::AgentCard> {
    cards
        .iter()
        .find(|c| c.enabled && Some(c.id) != approver)
        .cloned()
}

/// t41 (from the t40 measurement): a default-level decision must SAY which agent it chose and why.
///
/// WHY, measured: a run's JSON carries no agent name at all -- a substring search for `approver` over
/// the whole object is False, its only trace is `params.agent`, a bare ULID -- and the routing
/// decision said `source.level = "default"` with `rationale: null`. So a CONFIGURED default and the
/// no-default accident were indistinguishable, and the user whose real intent came back `ALLOW`
/// (`run.result = ALLOW`, 13 s, `status=completed`, `used=14422` tokens) had nothing that connected
/// the one word to its cause. The panel already renders this field next to `路由：{level}`, so this is
/// text, not UI.
///
/// TWO CASES, kept apart because their remedies differ:
///   * `configured = true` -- `routing.toml` names a `default`: an operator chose this agent, so the
///     text says "configured default" and names the file, which is where to choose another one;
///   * `configured = false` -- nothing is configured, so the FIRST ENABLED CARD was used: the text
///     says so and names the key to set, so the reader learns the agent was picked by ordering rather
///     than by anyone's decision.
///
/// The agent's NAME, never its id: `params.agent` already carries the id, and an id is exactly what a
/// reader cannot use without a second lookup.
fn default_rationale(agent: &str, configured: bool, approver: Option<&str>) -> String {
    if configured {
        let mut text = format!(
            "no agent pinned and no route matched; configured default `{agent}` (routing.toml)"
        );
        if approver == Some(agent) {
            // t43: an operator may deliberately want the adjudicator as the fallback, and that is a
            // legitimate state -- but the text has to say what they have asked for, or the one-word
            // answer looks like a bug.
            text.push_str(
                " -- this is the permission approver policy.toml designates: it answers ALLOW/REJECT, \
                 so a task sent here is adjudicated, not performed",
            );
        }
        text
    } else {
        match approver {
            // t43: the fallback skipped the adjudicator to get here. Say so, and say how to choose it
            // deliberately, because "the agent I wanted was silently not used" is the failure mode
            // this whole guard exists to avoid replacing with another silence.
            Some(skipped) => format!(
                "no agent pinned, no route matched and no default configured; using the first enabled \
                 card that is NOT the permission approver `{agent}` -- `{skipped}` is the approver \
                 policy.toml designates, and it adjudicates instead of performing the task (set \
                 routing.toml's `default` to pick either deliberately)"
            ),
            None => format!(
                "no agent pinned, no route matched and no default configured; using the first enabled card `{agent}` (set routing.toml's `default` to choose)"
            ),
        }
    }
}

/// Fill in the rationale of a DEFAULT-level decision that has none (t41), and name the permission
/// approver when it is what the decision is about (t43): either a configured `default` that names the
/// adjudicator, or a fallback that had to skip it. Explicit and rule decisions are left alone: they
/// already carry their own provenance (`RouteSource::Rule { rule_id }`, which the panel prefers over
/// the rationale).
///
/// `approver` is the NAME of the card `policy.toml`'s `[permissions.approver].agent` designates
/// (`designated_approver_name`), so both branches talk about the same identity the run path uses.
fn with_default_rationale(
    mut decision: ruagent_core::RoutingDecision,
    agent: &str,
    configured: bool,
    approver: Option<&str>,
) -> ruagent_core::RoutingDecision {
    if decision.source == ruagent_core::RouteSource::Default && decision.rationale.is_none() {
        decision.rationale = Some(default_rationale(agent, configured, approver));
    }
    decision
}

/// The NAME of the card `policy.toml` designates as the permission approver (t43), resolved through
/// the SAME id the run path uses (`RunManager::approver_id`, which `RunManager::new` computes from
/// `policy.approver.agent` by name against this registry). `None` when no approver is configured or
/// it is not registered -- in which case the fallback has nothing to exclude.
fn designated_approver_name(state: &AppState) -> Option<String> {
    let id = state.mgr.approver_id()?;
    state
        .mgr
        .agents()
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.name)
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
            // t41: the cascade yields `Default` only when `routing.toml` named a `default`, so this is
            // the CONFIGURED case -- name the agent and say where the choice lives. t43: if that name is
            // the permission approver, say what that means -- the operator asked for it on purpose, and
            // the guard below deliberately does not stand in their way.
            let designated = designated_approver_name(&state);
            (
                card.name.clone(),
                with_default_rationale(decision, &card.name, true, designated.as_deref()),
            )
        } else {
            // t38: build the fallback from the SAME registry the run resolves against, below.
            // `state.config.default_agent()` is the CONFIG-FILE view (`config.rs:106`), and the
            // cards that view builds carry a fresh `AgentId::generate()` on every load
            // (`config.rs:200/225/265/307`; the last one's own comment says the stable database id
            // replaces it). Recording that id put an agent into the `routed` event that no registry
            // contains -- measured: the live transcript carried `01a0f580-b2ac-7048-a136-6bbe33d5f13b`
            // while the same run's `params.agent` was `01a0b915-8273-73c4-8fe9-0e35a34d4ded`
            // (approver) -- and the emission site had to reconcile it. Two views are not the bug:
            // the file describes what is CONFIGURED, the registry what is RUNNABLE. Recording the
            // unmaterialized one was.
            // t43: the LAST-RESORT fallback must not be the permission approver, and where it cannot
            // proceed it must say why instead of running the adjudicator. The key is the id `policy.toml`
            // designates, resolved exactly as tier 2 resolves it (`RunManager::approver_id`).
            let approver_id = state.mgr.approver_id();
            let approver_name = designated_approver_name(&state);
            let cards = state.mgr.agents();
            let card = default_route_agent(&cards, approver_id).ok_or_else(|| {
                match cards
                    .iter()
                    .find(|c| c.enabled && approver_id == Some(c.id))
                    .map(|c| c.name.as_str())
                {
                    // The adjudicator is the ONLY enabled agent. Running it would report success while
                    // adjudicating a task, so refuse -- and say what to do instead (t43). This text is
                    // NOT the old "no agent specified and no default configured": that one means
                    // "nothing is enabled", which is the `None` arm below and stays unchanged.
                    Some(adjudicator) => ApiError::bad_request(format!(
                        "no agent specified, no route matched and no default configured, and the only \
                         enabled agent is `{adjudicator}` -- the permission approver policy.toml \
                         designates, which answers ALLOW/REJECT instead of performing tasks; enable a \
                         work-capable agent, name one in routing.toml's `default`, or pin one on this \
                         run (`agent`)"
                    )),
                    None => ApiError::bad_request("no agent specified and no default configured"),
                }
            })?;
            // t41 + t43: nothing is configured, so this is the ACCIDENT case -- and when the approver had
            // to be skipped to get here (it is the first ENABLED card), the text says so and says how to
            // choose it deliberately. A skip that did not happen passes `None`.
            let skipped = approver_name.as_deref().filter(|name| {
                cards
                    .iter()
                    .find(|c| c.enabled)
                    .is_some_and(|first| first.name == *name)
            });
            (
                card.name.clone(),
                with_default_rationale(
                    ruagent_core::RoutingDecision::default_agent(card.id),
                    &card.name,
                    false,
                    skipped,
                ),
            )
        }
    };
    let card = state
        .mgr
        .agent(&agent)
        .ok_or_else(|| ApiError::bad_request(format!("unknown agent `{agent}`")))?;

    let prompt = req.prompt.unwrap_or_else(|| task.intent.clone());
    // Pre-flight the injected MCP servers. A command that cannot be resolved is
    // a configuration error the user can fix — say so here, instead of letting
    // the harness refuse session/new with an opaque "mcp-client(mcp): initial
    // connection or tool synchronization failed".
    let mcp = state
        .config
        .mcp
        .expand_profile_preflighted(card.mcp_profile.as_deref(), &card.name)
        .map_err(ApiError::bad_request)?;

    let workspace_spec = match (req.cwd.as_deref(), req.repo.as_deref()) {
        (Some(cwd), _) => WorkspaceSpec::Cwd(std::path::PathBuf::from(cwd)),
        (None, Some(repo)) => WorkspaceSpec::Worktree {
            repo: std::path::PathBuf::from(repo),
        },
        (None, None) => WorkspaceSpec::Fresh,
    };
    let run = state
        .mgr
        .start_run(
            &task,
            &agent,
            prompt,
            mcp,
            workspace_spec,
            crate::runs::RunLaunch {
                routed: Some(decision),
                options: req.options,
                ..Default::default()
            },
        )
        .await?;
    Ok(Json(run))
}

#[derive(Deserialize)]
struct FanOutRequest {
    agents: Vec<String>,
    prompt: Option<String>,
    /// Git repo: every fan-out member gets its own worktree (design SS8.2).
    repo: Option<String>,
    /// Canonical session-option defaults applied to every member
    /// (issue #36): mode / effort.
    #[serde(default)]
    options: std::collections::BTreeMap<String, String>,
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
            req.options,
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

#[derive(Deserialize)]
struct JudgeRequest {
    agent: String,
    /// Restrict judging to these runs; default = every completed run
    /// with a result.
    run_ids: Option<Vec<String>>,
}

/// Fan-out judge (design §5.2/§5.3): a NORMAL run that reviews the
/// completed runs of a task and picks the winner. Returns the live
/// judge run; the verdict lands on the task in the background.
async fn judge_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(req): Json<JudgeRequest>,
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
    let runs = state.mgr.db().list_runs_for_task(task_id).await?;

    let wanted: Option<Vec<RunId>> = match req.run_ids {
        Some(ids) => {
            let mut out = Vec::new();
            for raw in ids {
                let id: RunId = raw
                    .parse()
                    .map_err(|_| ApiError::bad_request(format!("invalid run id `{raw}`")))?;
                if !runs.iter().any(|r| r.id == id) {
                    return Err(ApiError::bad_request(format!(
                        "run `{raw}` does not belong to this task"
                    )));
                }
                out.push(id);
            }
            Some(out)
        }
        None => None,
    };

    let agent_cards = state.mgr.agents();
    let mut candidates: Vec<crate::runs::JudgeCandidate> = Vec::new();
    for r in &runs {
        if let Some(ids) = &wanted
            && !ids.contains(&r.id)
        {
            continue;
        }
        if r.status != RunStatus::Completed {
            continue;
        }
        let Some(result) = r.result.as_deref().filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        let agent_name = agent_cards
            .iter()
            .find(|c| c.id == r.params.agent)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| r.params.agent.to_string());
        candidates.push(crate::runs::JudgeCandidate {
            run_id: r.id,
            agent_name,
            cost_usd: r.cost_usd,
            result: result.to_string(),
        });
    }
    if candidates.len() < 2 {
        return Err(ApiError::bad_request(
            "judging needs at least two completed runs with results",
        ));
    }
    let run = state
        .mgr
        .start_judge(&task, &req.agent, &candidates)
        .await?;
    Ok(Json(serde_json::json!({ "judge_run": run })))
}

/// The newest judge run for a task and its parsed verdict, for display.
/// The authoritative selection lives on the task (`selected_run_id` /
/// `selected_by`); this re-derives the judge's own pick + rationale from
/// its reply, so it survives a later human override.
async fn judge_view(
    db: &ruagent_store::Db,
    task_id: ruagent_core::TaskId,
    parent_runs: &[Run],
) -> Result<Option<serde_json::Value>, ApiError> {
    let edges = db.edges_from(task_id).await?;
    let mut judge_tasks = Vec::new();
    for e in edges
        .iter()
        .filter(|e| e.kind == ruagent_core::EdgeKind::Reviews)
    {
        if let Some(t) = db.get_task(e.to).await? {
            judge_tasks.push(t);
        }
    }
    let Some(judge_task) = judge_tasks
        .into_iter()
        .max_by(|a, b| a.created_at.cmp(&b.created_at))
    else {
        return Ok(None);
    };
    let mut runs = db.list_runs_for_task(judge_task.id).await?;
    let Some(run) = runs.pop() else {
        return Ok(None);
    };
    let candidate_ids: Vec<RunId> = parent_runs.iter().map(|r| r.id).collect();
    let (winner, rationale) = run
        .result
        .as_deref()
        .and_then(|reply| crate::runs::parse_judge_verdict(reply, &candidate_ids))
        .map(|(w, r)| (Some(w), r))
        .unwrap_or((None, None));
    Ok(Some(serde_json::json!({
        "judge_run_id": run.id,
        "judge_run_status": run.status,
        "winner_run_id": winner,
        "rationale": rationale,
    })))
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
    state
        .mgr
        .db()
        .set_selected_run(run.task_id, run_id, "human")
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Land the fan-out winner (design §5.2): merge the selected run's
/// worktree branch into the main repo, then sweep the task's
/// worktrees. A failed merge reports git's own message.
async fn land_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: ruagent_core::TaskId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid task id"))?;
    state
        .mgr
        .db()
        .get_task(id)
        .await?
        .ok_or_else(|| ApiError::not_found("task not found"))?;
    let hash = state
        .mgr
        .land_selected(id)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({ "landed": hash })))
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

// ---------------------------------------------------------------------------
// Session history (auto-synced from claude-code / dsh / ruagent stores)
// ---------------------------------------------------------------------------

/// Which sessions the list should return.
#[derive(Deserialize)]
struct SessionsQuery {
    limit: Option<u32>,
    /// exclude (default) | include | only
    archived: Option<String>,
}

/// A session is only deletable by the tool that owns its history file.
/// Everything else here is an index of a file we must not remove.
const DELETABLE_SOURCE: &str = "ruagent";

async fn sessions_list(
    State(state): State<AppState>,
    Query(q): Query<SessionsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(200);
    let mode = q.archived.as_deref().unwrap_or("exclude");
    if !matches!(mode, "exclude" | "include" | "only") {
        return Err(ApiError::bad_request(format!(
            "archived must be one of exclude|include|only, got '{mode}'"
        )));
    }
    // Deleted sessions are gone in *every* mode: the tombstone exists so that
    // the indexer's next rescan (INSERT OR REPLACE, every 60s) cannot bring
    // the row back, and a deleted key must not reappear as "archived" either.
    let deleted: std::collections::HashSet<String> = state
        .mgr
        .db()
        .deleted_session_keys()
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .collect();
    let hidden: std::collections::HashSet<String> = state
        .mgr
        .db()
        .archived_session_keys()
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .filter(|k| !deleted.contains(k))
        .collect();
    let mut sessions = state
        .sessions
        .list(limit)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    // ruagent conversations carry the agent identity from the chats
    // table (which role/runtime the user was talking to).
    if sessions.iter().any(|s| s.source == "ruagent") {
        let by_key = state.chats.session_keys_by_agent(limit).await;
        for s in &mut sessions {
            if s.source == "ruagent"
                && let Some(agent) = by_key.get(s.key.as_str())
            {
                s.agent = Some(agent.clone());
            }
        }
    }
    // The archived flag is not part of the index (see the session_archives
    // migration for why it cannot be a column), so it is added here rather
    // than in the indexer's record.
    let mut out = Vec::with_capacity(sessions.len());
    for s in sessions {
        if deleted.contains(&s.key) {
            continue;
        }
        let is_hidden = hidden.contains(&s.key);
        if (mode == "exclude" && is_hidden) || (mode == "only" && !is_hidden) {
            continue;
        }
        let mut v = serde_json::to_value(&s)
            .map_err(|e| ApiError::internal(format!("session encode: {e}")))?;
        if let Some(obj) = v.as_object_mut() {
            obj.insert("archived".into(), serde_json::Value::Bool(is_hidden));
            obj.insert(
                "deletable".into(),
                serde_json::Value::Bool(s.source == DELETABLE_SOURCE),
            );
        }
        out.push(v);
    }
    Ok(Json(serde_json::json!({
        "sessions": out,
        "archived_count": hidden.len(),
    })))
}

/// Hide a session from the default list. ruagent-side only: no file is
/// read, written or moved.
async fn session_archive(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state
        .mgr
        .db()
        .session_exists(&key)
        .await
        .map_err(ApiError::from)?
    {
        return Err(ApiError::not_found(format!("no such session: {key}")));
    }
    let now = chrono::Utc::now().timestamp_millis();
    state
        .mgr
        .db()
        .archive_session(&key, now)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "key": key, "archived": true })))
}

/// Un-hide. Idempotent: unarchiving something that was not archived is a
/// no-op, not an error, so a double-click cannot fail.
async fn session_unarchive(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let was = state
        .mgr
        .db()
        .unarchive_session(&key)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(
        serde_json::json!({ "key": key, "archived": false, "changed": was }),
    ))
}

/// Delete a session from ruagent's index.
///
/// Only sessions ruagent produced itself (source = ruagent) can be deleted.
/// Every other row points at another tool's history file — ruagent merely
/// indexes it, so removing it is refused with 403 and an explanation the UI
/// can show verbatim.
async fn session_delete(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state
        .mgr
        .db()
        .delete_session(
            &key,
            DELETABLE_SOURCE,
            chrono::Utc::now().timestamp_millis(),
        )
        .await
        .map_err(ApiError::from)?
    {
        DeleteSession::Deleted => Ok(Json(serde_json::json!({ "key": key, "deleted": true }))),
        DeleteSession::NotFound => Err(ApiError::not_found(format!("no such session: {key}"))),
        DeleteSession::SourceNotAllowed(source) => Err(ApiError::forbidden(format!(
            "this session belongs to '{source}': ruagent only indexes that tool's \
             history file and will not delete it. Remove it from {source} itself."
        ))),
    }
}

async fn sessions_messages(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let messages = state
        .sessions
        .messages(&key)
        .await
        .map_err(|e| ApiError::not_found(format!("{e}")))?;
    Ok(Json(serde_json::json!({ "messages": messages })))
}

/// Backfill embeddings for existing memory rows (after an embedder
/// change, or rows written before the semantic leg existed).
/// POST /api/v1/memory/migrate-distilled-prefix
///
/// One-time data migration (t347). Distillation used to write "`[distilled] `"
/// into the memory BODY; the marker is provenance, and provenance belongs in
/// `source_episode`, so the prefix is stripped from the stored bytes once and the
/// field carries the fact from then on.
///
/// WHY AN ENDPOINT AND NOT A STARTUP STEP: this rewrites user data (156 rows on
/// this machine). A daemon that silently rewrites memories on boot is a
/// surprise; a named, idempotent, callable migration that leaves a backup file
/// is auditable, and a second call is a measured no-op.
///
/// SINGLE WRITER: the rewrite goes through `state.mgr.db()` -- the store's
/// writer actor the daemon already owns. No second write connection is opened
/// for this, which is the whole reason it is an endpoint rather than a script.
async fn memory_migrate_distilled_prefix(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let backup_dir = state.mgr.root().join("data").join("backups");
    let out =
        ruagent_memory::lifecycle::strip_distilled_prefix(state.mgr.db(), &backup_dir).await?;
    Ok(Json(serde_json::json!({
        "scanned": out.scanned,
        "stripped": out.stripped,
        "marker_inside_only": out.marker_inside_only,
        "backfilled": out.backfilled,
        "backup": out.backup,
        "samples": out.samples,
    })))
}

async fn memory_backfill_embeddings(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // NULL embeddings AND rows written by a different model (the
    // memory-side half of an embedder switch)
    let total = crate::memembed::reembed_stale(state.mgr.db(), state.knowledge.embedder()).await;
    Ok(Json(serde_json::json!({ "embedded": total })))
}

// ---------------------------------------------------------------------------
// Distillation: session → memories + graph (agent-run extraction)
// ---------------------------------------------------------------------------

/// Open the system directory picker and return the chosen path (null
/// when cancelled). The browser cannot learn absolute local paths, but
/// the daemon is a local process — the workspace picker goes through
/// here. Interactive by design: nothing automated should call it.
async fn pick_directory() -> Json<serde_json::Value> {
    // AsyncFileDialog, not the sync API: rfd's sync pick_folder runs
    // CoInitializeEx(APARTMENTTHREADED) on the CALLING thread, which
    // fails silently (RPC_E_CHANGED_MODE → None, no dialog) on a
    // thread whose COM apartment differs. The async variant opens the
    // dialog on a dedicated fresh thread — the documented fix.
    #[cfg(windows)]
    let path = {
        let handle = rfd::AsyncFileDialog::new()
            .set_title("选择项目目录")
            .pick_folder()
            .await;
        handle.map(|h| h.path().to_path_buf())
    };
    // No native picker off-Windows (rfd's Linux backends all need
    // compile-time wayland/GTK) — the panel treats null as cancelled.
    #[cfg(not(windows))]
    let path: Option<std::path::PathBuf> = None;
    Json(serde_json::json!({ "path": path }))
}

/// The live distillation policy (the settings card's source) plus the
/// built-in extraction prompt for reference.
async fn distill_policy_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let p = state.chats.distill_policy_now();
    // INT-F3 (t127, additive): `prompt_hash` is the SAME origin as the hash
    // the real distillation records — `Distiller::distill_plan` computes
    // `ruagent_memory::write::content_hash(&self.compose_prompt())`
    // (`crates/daemon/src/distill.rs`). `compose_prompt` is `pub(crate)`, so
    // this handler builds the SAME struct out of the SAME policy the six keys
    // above already report — `Distiller` has no private fields, so no accessor
    // was needed. The field mapping is the one the auto-distill path installs at
    // boot (`crates/daemon/src/lib.rs`): `[distill] prompt -> prompt_override`,
    // `language -> language`, `graph -> graph` (policy -> field).
    let distiller = crate::distill::Distiller {
        db: state.mgr.db().clone(),
        root: state.config.root.clone(),
        embedder: Some(state.knowledge.embedder()),
        registry: state.mgr.registry_view(),
        language: p.language.clone(),
        prompt_override: p.prompt.clone(),
        graph: p.graph,
    };
    Json(serde_json::json!({
        "auto": p.auto,
        "agent": p.agent,
        "language": p.language,
        "prompt": p.prompt,
        "graph": p.graph,
        "builtin_prompt": crate::distill::builtin_extraction_prompt(),
        "prompt_hash": ruagent_memory::write::content_hash(&distiller.compose_prompt()),
    }))
}

#[derive(Deserialize)]
struct DistillPutRequest {
    #[serde(default)]
    auto: bool,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    /// true|false; null/absent clears the key (back to the default).
    /// Taken as a raw value so a non-boolean 400s here, not as an
    /// extractor rejection.
    #[serde(default)]
    graph: Option<serde_json::Value>,
}

/// Update the distillation policy: write `[distill]` in policy.toml
/// (round-tripped — comments and other sections survive), then swap the
/// live value. Empty strings clear optional keys.
async fn distill_policy_put(
    State(state): State<AppState>,
    Json(req): Json<DistillPutRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Normalize: blank optional fields mean "not set".
    let clean = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let agent = clean(&req.agent);
    let language = clean(&req.language);
    let prompt = clean(&req.prompt);
    // graph: a bool or nothing. Anything else 400s instead of silently
    // distilling with the default.
    let graph = match req.graph {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Bool(b)) => Some(b),
        Some(_) => return Err(ApiError::bad_request("`graph` must be true or false")),
    };
    // A named extractor must exist — a typo would silently fall back to
    // dsh otherwise.
    if let Some(name) = agent.as_deref() {
        let known = state.mgr.agents();
        if !known.iter().any(|a| a.enabled && a.name == name) {
            return Err(ApiError::bad_request(format!(
                "unknown or disabled agent `{name}`"
            )));
        }
    }
    let cfg = ruagent_policy::DistillConfig {
        auto: req.auto,
        agent,
        language,
        prompt,
        graph,
    };
    crate::config::DistillEditor::new(state.config.root.join("config").join("policy.toml"))
        .update(&cfg)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    state.chats.set_distill_policy(crate::distill::AutoDistill {
        auto: cfg.auto,
        agent: cfg.agent.clone(),
        language: cfg.language.clone(),
        prompt: cfg.prompt.clone(),
        graph: cfg.graph.unwrap_or(true),
    });
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// The optional body of `POST /api/v1/sessions/{key}/distill` (t4, §14.3).
#[derive(Deserialize)]
struct DistillBody {
    #[serde(default)]
    extractor: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

/// Read the distill request's body EXPLICITLY.
///
/// WHY NOT `Option<Json<DistillBody>>`: that shape swallows every rejection, so
/// a malformed body would silently become "no body" — a silent default, which is
/// exactly what the capability contract forbids. An empty body still means
/// today's behaviour (one ACP turn, no dry run), and an unknown `extractor`
/// value is a 400 naming the value and the accepted ones.
fn distill_request(body: &[u8]) -> Result<(crate::extract_plane::ExtractPlan, bool), ApiError> {
    if body.is_empty() {
        return Ok((
            crate::extract_plane::ExtractPlan::explicit(crate::extract_plane::Extractor::Acp),
            false,
        ));
    }
    let parsed: DistillBody = serde_json::from_slice(body)
        .map_err(|e| ApiError::bad_request(format!("distill body is not JSON: {e}")))?;
    let extractor = match parsed.extractor.as_deref() {
        None => crate::extract_plane::Extractor::Acp,
        Some(value) => crate::extract_plane::Extractor::parse(value)
            .map_err(|e| ApiError::bad_request(format!("{e}")))?,
    };
    // MANUAL distillation is NOT capability-gated (§3.7): it is an explicit
    // human/agent instruction, so `ExtractPlan::explicit` runs what was asked
    // for. `extractor: "rules"` is the zero-token choice.
    Ok((
        crate::extract_plane::ExtractPlan::explicit(extractor),
        parsed.dry_run,
    ))
}

/// `POST /api/v1/sessions/{key}/distill` with the optional body of §14.3 (t4).
///
/// The body is OPTIONAL and read as raw bytes: an empty body is today's
/// behaviour — one ACP chat turn, through the extractor seam with
/// `ExtractPlan::explicit(Acp)` — and `{"extractor":"rules"|"acp"|"both",
/// "dry_run":bool}` names the implementation. `dry_run` extracts and reports
/// without writing anything at all.
async fn session_distill(
    State(state): State<AppState>,
    Path(key): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (plan, dry_run) = distill_request(&body)?;
    // The live policy decides: its agent, else dsh, else the first
    // enabled; its language/prompt ride the extraction prompt.
    let policy = state.chats.distill_policy_now();
    let distiller = crate::distill::Distiller {
        db: state.mgr.db().clone(),
        root: state.config.root.clone(),
        embedder: Some(state.knowledge.embedder()),
        registry: state.mgr.registry_view(),
        language: policy.language,
        prompt_override: policy.prompt,
        graph: policy.graph,
    };
    // THE AGENT CARD IS RESOLVED LAZILY (t14): it is the ACP tier's business, so a
    // plan the caller built from `{"extractor":"rules"}` must NOT need one. Until
    // this, the free tier was unreachable on a machine with no enabled agent —
    // the exact machine it is for — and answered 400 to a request that was never
    // going to spawn anything. An ACP plan with no enabled agent still fails
    // here, visibly, naming the reason; that requirement is unchanged.
    let card = if plan.acp {
        let agents = state.mgr.agents();
        let enabled: Vec<_> = agents.iter().filter(|a| a.enabled).cloned().collect();
        Some(
            crate::distill::select_agent(&enabled, policy.agent.as_deref())
                .map_err(|e| ApiError::bad_request(format!("{e:#}")))?
                .clone(),
        )
    } else {
        None
    };
    let out = distiller
        .distill_plan(&key, card.as_ref(), plan, dry_run)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({ "distilled": out })))
}

// ---------------------------------------------------------------------------
// Recall — two strategies (aggressive: full content above threshold;
// conservative: stubs only, agent pulls details on demand)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RecallQuery {
    q: String,
    #[serde(default)]
    strategy: Option<String>, // aggressive | conservative
    #[serde(default)]
    min_score: Option<f64>,
    #[serde(default)]
    top_n: Option<u32>,
    /// WHO IS CALLING, declared by the caller (?source= wins, then the
    /// x-ruagent-recall-source header). Omitted = unknown, recorded as NULL.
    #[serde(default)]
    source: Option<String>,
}

/// One recall leg's capability id, for the response and the log (t5).
///
/// THE ONE MAPPING from the engine's typed legs to the registry's ids, and the only
/// place the ids are spelled in this flow — `CapabilityId::as_str()` supplies the
/// strings, so a rename in the registry cannot leave a stale literal behind here
/// (a second hardcoded list is what t10's "no second list shadowing the registry"
/// item rejects). The engine deliberately names no id at all.
fn recall_leg_id(leg: crate::memembed::RecallLeg) -> &'static str {
    use crate::capability::CapabilityId;
    use crate::memembed::RecallLeg;
    match leg {
        RecallLeg::MemorySemantic => CapabilityId::RecallLegMemorySemantic,
        RecallLeg::MemoryFts => CapabilityId::RecallLegMemoryFts,
        RecallLeg::KnowledgeSemantic => CapabilityId::RecallLegKnowledgeSemantic,
        RecallLeg::KnowledgeFts => CapabilityId::RecallLegKnowledgeFts,
        RecallLeg::Wiki => CapabilityId::RecallLegWiki,
        RecallLeg::Graph => CapabilityId::RecallLegGraph,
    }
    .as_str()
}

/// THE ONE PLACE the six recall legs are read off the capability plane: the runtime's
/// own `(enabled, configured weight)` per leg, handed to the runtime's own resolver.
///
/// Two callers need exactly this question asked of exactly this table:
///
/// * the recall handler below, which must not let two call sites resolve a leg
///   differently (`legacy = true` for all six: every one of them is today's behaviour,
///   design L1/L2, so the plane can only NARROW what the handler does);
/// * the capability write door's per-leg probe (`crate::capability::runtime_refusal`),
///   which asks whether a given plane would put ONE leg in a state the runtime refuses.
///   Without this function that probe would have to re-read the plane and re-apply
///   `Option<f64> -> f32` itself, i.e. carry a second copy of the reading — and the
///   copy is what drifts.
///
/// `RecallLegConfig::resolve` (crates/daemon/src/memembed.rs) is the runtime's own
/// composition over `MemoryLegs::resolve` and `LegConfig::resolve`; this function only
/// supplies its arguments, so which legs carry a weight (and what a leg's default is
/// when the file names none) is decided there and nowhere else.
pub(crate) fn recall_legs(
    plane: &crate::capability::CapabilityPlane,
) -> Result<crate::memembed::RecallLegConfig, WeightError> {
    use crate::capability::CapabilityId;
    use crate::memembed::RecallLegConfig;
    let leg = |id: CapabilityId| (plane.gate(id, true), plane.options(id).weight);
    RecallLegConfig::resolve(
        leg(CapabilityId::RecallLegMemorySemantic),
        leg(CapabilityId::RecallLegMemoryFts),
        leg(CapabilityId::RecallLegKnowledgeSemantic),
        leg(CapabilityId::RecallLegKnowledgeFts),
        plane.gate(CapabilityId::RecallLegWiki, true),
        plane.gate(CapabilityId::RecallLegGraph, true),
    )
}

/// `GET /api/v1/recall?q=...&top_n=N[&strategy=...][&min_score=...][&source=...]`
///
/// The knowledge hits in this response are the CROSS-KIND fused top-N: memories,
/// knowledge and entities compete for the same `top_n` slots, so a query whose
/// memories fill the budget returns no knowledge hits at all.
/// `/api/v1/knowledge/search` answers the narrower question (knowledge-only
/// top-N), so the two hit SETS differ by design — do not read them as the same
/// list (t308).
///
/// The per-leg fields of a knowledge hit are identical on the INTERSECTION of
/// the two responses: the same `compute_legs`, the same leg window, and both
/// endpoints build the object with the same `knowledge_hit_json`. Measured
/// live: `q='deploy'` — this endpoint [43,41] vs knowledge/search
/// [43,62,58,64,70], intersection [43], differing leg keys NONE; `q='wiki'` —
/// this endpoint returned no knowledge hit (nothing to compare).
async fn recall(
    State(state): State<AppState>,
    Query(q): Query<RecallQuery>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conservative = q.strategy.as_deref() == Some("conservative");
    let top_n = q.top_n.unwrap_or(5).clamp(1, 20);

    // WHO CALLED THIS (t251). The caller declares its own source -- query
    // parameter first, header second -- and a caller that declares nothing is
    // recorded as NULL. The daemon never infers it: a guess from the query text
    // would put a second, wrong answer into a column the writer can answer
    // correctly, and "probe" would stop meaning anything.
    let source = declared_source(&q.source, &headers)?;

    // t5: THE RECALL LEG CONFIGURATION, read from the live plane ONCE.
    //
    // One read, one value, passed into every leg producer below: a leg cannot be
    // half-off because two call sites resolved it differently, and a leg that is off
    // is never queried (`recall_memories_with` / `search_page_with` gate the CALL,
    // not the results — the embedding and the SQL are the costs).
    //
    // The reading lives in [`recall_legs`], because the capability write door asks the
    // SAME question of the SAME table (§11.5, increment 5) and must not be able to
    // disagree with this handler about what the plane says.
    let plane = state.chats.capabilities();
    let legs = match recall_legs(&plane) {
        Ok(legs) => legs,
        Err(e) => {
            return Err(ApiError::bad_request(leg_config_error(e)));
        }
    };
    // The capability ids of the legs this call did NOT query, sorted — the ONE
    // reason a response can be empty for a reason other than "nothing matched".
    // The ids come from the registry (`CapabilityId::as_str`), never from a second
    // list spelled here or in the engine.
    let legs_disabled: Vec<&'static str> = legs
        .disabled_legs()
        .into_iter()
        .map(recall_leg_id)
        .collect();
    // t2 (design §20.2, the MEMORY half): the memory fusion's EFFECTIVE weights, in
    // the same shape the knowledge side already publishes (`scoring.fusion` /
    // `candidates_json.fusion`). The knowledge half was auditable; the memory half
    // recorded only hit COUNTS, so a `recall_leg_memory_semantic.weight` change left
    // no trace at all — "the weight changed" and "the corpus changed" were the same
    // record. Read from the SAME `MemoryLegs` handed to the fusion below, through
    // the same normalizer that feeds it (`MemoryLegs::effective`), never from the raw
    // configured values: a weight the ranking did not use is not evidence about the
    // ranking.
    let memory_fusion_label = legs.memory.fusion_label();
    // `recall_leg_memory_semantic.min_score` (design §11.2): unset ⇒ today's
    // per-strategy floor; set ⇒ it overrides BOTH strategies, because a caller who
    // names a number means it.
    let mem_min_score = plane
        .options(crate::capability::CapabilityId::RecallLegMemorySemantic)
        .min_score
        .map(|v| v as f32)
        .unwrap_or(if conservative { 0.30 } else { 0.25 });

    // Memories: BOTH legs, fused on one scale, bounded by top_n. Before t251
    // the handler took the semantic leg, seeded a seen-set from it and APPENDED
    // keyword rows that were not in that set -- a concatenation, bounded by
    // 2 * top_n, with no score at all on the keyword rows.
    let mem_legs = crate::memembed::recall_memories_with(
        state.mgr.db(),
        state.knowledge.embedder(),
        &q.q,
        top_n,
        mem_min_score,
        &legs.memory,
    )
    .await;

    // Knowledge chunks (hybrid semantic + keyword). Parent-child
    // retrieval (WeKnora): the hit is the precise unit, the aggressive
    // strategy returns the parent SECTION for complete context.
    // Search WIDER than top_n: wiki pages are compilations of their
    // sources — more keyword density, more chunks — and crowd the
    // sources out of a shared budget (live finding 2026-09-16:
    // "autohotkey 改键" returned 5 wiki chunks, 0 source docs). Each
    // section gets its own top_n instead.
    let search_n = (top_n.saturating_mul(3)).min(30);
    // H-1/H-2 (t19): ONE paged entry point. `search_page` returns every hit WITH
    // its scale (`score_kind`), its per-leg evidence (rank + raw + kind) and the
    // calibrated relevance, plus the page's own provenance (`leg_window`,
    // `fusion`, candidate count). The response, the telemetry row and the panel
    // all read THESE values: the old shape called `search` AND `search_legs`
    // (two passes that could disagree) and had no scale to report at all.
    let page = state
        .knowledge
        .search_page_with(&q.q, search_n, &legs.knowledge)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let leg_window = page.evidence.leg_window as i64;
    let candidates = page.evidence.candidates as i64;
    // WHY THIS IS SPELLED OUT HERE INSTEAD OF CALLING `FusionKind::label()`
    // (the memory-label alignment):
    // the knowledge type's own `label()` renders a DIFFERENT string for the same
    // information — `"rrf(k=60,w_sem=2,w_kw=1)"` (crates/knowledge/src/store.rs:151,
    // pinned by that type's own test). `scoring.fusion` / `recall_log.fusion` have
    // emitted THIS spelling (`rrf:k=...,w_semantic=...,w_keyword=...`) since the key
    // was introduced, and they are EXISTING wire values: unifying the pre-existing
    // pair would be a non-additive change to a key readers already parse, so it is
    // deliberately NOT done here. What IS done: the memory label added by t2
    // (`MemoryLegs::fusion_label`, returned as `memory_fusion`) is built with THIS
    // spelling, so one payload never carries two shapes for one concept — a reader
    // that can parse `scoring.fusion` can parse `memory_fusion` with the same code.
    let fusion_label = match page.evidence.fusion {
        ruagent_knowledge::store::FusionKind::Rrf {
            k,
            w_semantic,
            w_keyword,
        } => format!("rrf:k={k},w_semantic={w_semantic},w_keyword={w_keyword}"),
    };
    // The stage label is composed from the page's producer state AND the leg
    // configuration this call passed: with the keyword leg off the label is
    // "disabled", never "empty" (t12 / design §11.3).
    let kw_stage = Some(keyword_stage_label(
        &page.evidence.keyword_stage,
        legs.knowledge.keyword,
    ));
    let ranked: Vec<ruagent_knowledge::store::RankedHit> = page.hits;
    // The scale of the top score, its window and the calibration version, taken
    // from the SAME page the score came from (H-2: a score without its scale is
    // not comparable to the pre-0019 rows, whose window was `limit.max(10)`).
    let top_score_kind = ranked
        .first()
        .map(|r| r.score_kind.as_str())
        .unwrap_or("rrf_rank");
    let scoring_version = ranked
        .first()
        .and_then(|r| r.relevance.as_ref())
        .map(|r| r.version);
    let top_knowledge_relevance = ranked
        .first()
        .and_then(|r| r.relevance.as_ref())
        .map(|r| r.value as f64);
    let top_knowledge_relevance_kind = ranked
        .first()
        .and_then(|r| r.relevance.as_ref())
        .map(|r| r.kind.as_str());
    let all_hits: Vec<ruagent_knowledge::store::SearchHit> =
        ranked.iter().map(|r| r.hit.clone()).collect();
    // raw best score BEFORE any filtering — logged for threshold
    // tuning (what the filters dropped)
    let top_knowledge_score = all_hits.first().map(|h| h.score as f64);
    let (wiki_hits, hits): (Vec<_>, Vec<_>) = all_hits
        .into_iter()
        .partition(|h| h.document.starts_with("wiki/"));
    let wiki_hits: Vec<_> = wiki_hits.into_iter().take(top_n as usize).collect();
    let hits: Vec<_> = hits.into_iter().take(top_n as usize).collect();
    // The wiki leg (§11.3): OFF ⇒ `"wiki": []` and `wiki::recall_stubs` is never
    // called. The partition above still keeps `wiki/…` documents out of
    // `knowledge` (that is a filter over hits already retrieved, not a query).
    let out_wiki = if legs.wiki {
        crate::wiki::recall_stubs(state.knowledge.as_ref(), &wiki_hits)
    } else {
        Vec::new()
    };
    let parents = state
        .knowledge
        .parents_for(&hits.iter().map(|h| h.chunk_id).collect::<Vec<_>>())
        .await;

    // Graph seeds + one multi-hop evidence pass.
    //
    // DEP-1 (graph spec, wired in t19): this leg used to be the STRICT name/summary
    // FTS match (`search_entities`), which matched 1 of 23 real queries. It is now
    // the graph's own seed resolver (semantic + name + alias + summary + fact-text
    // legs), followed by one retrieval so the counts below are the production
    // path's own reading — and they are what lands in `recall_log.graph_entities`
    // / `graph_paths` (DEP-3).
    //
    // t5: the whole block is gated. OFF ⇒ `resolve_seeds`, `retrieve` and the
    // per-entity `current_facts` pass are NOT called (four SQL paths per entity,
    // and the `search`/`fts_related` probes under them), and the response reports
    // the NEW literal `"leg disabled"` — deliberately not one of `EmptyReason`'s
    // values, which mean "the walk ran and found nothing".
    let (graph_edges_total, graph_paths, graph_empty_reason, graph_truncated_by, entities) = if legs
        .graph
    {
        let seeds = ruagent_graph::resolve_seeds(state.mgr.db(), &q.q, top_n)
            .await
            .map_err(|e| ApiError::bad_request(format!("{e}")))?;
        let graph_evidence = ruagent_graph::retrieve(
            state.mgr.db(),
            &ruagent_graph::GraphQuery::for_text(q.q.clone()),
        )
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
        // NOT "the edge count at the instant": the graph's total size, constant
        // across `as_of` (RV-C). Recorded for provenance, never read as temporal.
        let edges_total = graph_evidence.stats.graph_edges as i64;
        let paths = graph_evidence.stats.paths_emitted as i64;
        let truncated_by = graph_evidence
            .stats
            .truncated_by
            .map(|t| format!("{t:?}").to_lowercase());
        let empty_reason = graph_evidence
            .stats
            .empty_reason
            .map(|r| format!("{r:?}").to_lowercase());
        let mut seen_entities: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let entities: Vec<ruagent_graph::Entity> = seeds
            .into_iter()
            .filter(|s| seen_entities.insert(s.entity.id))
            .map(|s| s.entity)
            .collect();
        (edges_total, paths, empty_reason, truncated_by, entities)
    } else {
        (0, 0, Some("leg disabled".to_string()), None, Vec::new())
    };
    // The telemetry closure is `move`, so the response keeps its own copies of
    // these two (they are reported in BOTH places on purpose).
    let graph_empty_resp = graph_empty_reason.clone();
    let graph_truncated_resp = graph_truncated_by.clone();
    let graph_entities = entities.len() as i64;
    let mut entity_facts: std::collections::HashMap<i64, Vec<(String, String, String)>> =
        std::collections::HashMap::new();
    if legs.graph {
        // id -> name for rendering edge endpoints.
        let names: std::collections::HashMap<i64, String> = state
            .mgr
            .db()
            .call(|conn| -> Result<_, ruagent_store::DbError> {
                let mut stmt = conn
                    .prepare("SELECT id, name FROM entities")
                    .map_err(ruagent_store::DbError::from)?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                    .map_err(ruagent_store::DbError::from)?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        for e in &entities {
            if let Ok(facts) = ruagent_graph::current_facts(state.mgr.db(), e.id).await {
                let rels = facts
                    .iter()
                    .map(|f| {
                        let other = if f.src == e.id { f.dst } else { f.src };
                        let other_name = names
                            .get(&other)
                            .cloned()
                            .unwrap_or_else(|| format!("#{other}"));
                        (f.relation.clone(), other_name, f.fact_text.clone())
                    })
                    .collect();
                entity_facts.insert(e.id, rels);
            }
        }
    }

    let min_score = q.min_score.unwrap_or(0.0) as f32;
    // One entry per FUSED result: every row carries the same score scale and
    // names the legs that found it, plus each leg's own raw score. The fused
    // score is a RANK score (RRF), so it is labelled -- the panel showed the
    // knowledge leg's RRF score next to the memory leg's cosine in one column
    // and invited exactly that misreading (t247).
    let mut out_memories: Vec<serde_json::Value> = Vec::new();
    for hit in &mem_legs.hits {
        out_memories.push(if conservative {
            serde_json::json!({
                "kind": "memory", "id": hit.id, "store": hit.store,
                "namespace": hit.namespace,
                "title": truncate_chars(&hit.content, 60),
                "score": round_to(hit.score, 6),
                "score_kind": "rrf_rank",
                "legs": hit.legs,
                "semantic_score": hit.semantic_score.map(|v| round_to(v, 4)),
                "keyword_score": hit.keyword_score.map(|v| round_to(v, 4)),
                "hint": "call memory_get(id) for full content",
            })
        } else {
            serde_json::json!({
                "kind": "memory", "id": hit.id, "store": hit.store,
                "namespace": hit.namespace,
                "content": hit.content,
                "score": round_to(hit.score, 6),
                "score_kind": "rrf_rank",
                "legs": hit.legs,
                "semantic_score": hit.semantic_score.map(|v| round_to(v, 4)),
                "keyword_score": hit.keyword_score.map(|v| round_to(v, 4)),
            })
        });
    }
    // t250's per-leg evidence, at the HTTP layer. The fused score alone cannot
    // say WHICH leg found a chunk or how well it scored in that leg, so every
    // knowledge hit carries its legs, ranks and raw scores (semantic: LanceDB
    // distance, lower is closer; keyword: bm25, more negative is better).
    //
    // COST, stated rather than hidden: this is a second pass over the same two
    // legs. crates/knowledge is frozen by t250, so search() and search_legs()
    // cannot be merged into one call from here; both use the same leg_k, so the
    // Per-leg evidence comes from the SAME page the ranking came from (t19):
    // one pass, one leg window, no second `search_legs` call that could disagree.
    let ranked_of = |id: i64| ranked.iter().find(|r| r.hit.chunk_id == id);
    let mut out_chunks: Vec<serde_json::Value> = Vec::new();
    for hit in hits {
        if (hit.score as f64) < min_score as f64 {
            continue;
        }
        let Some(ranked_hit) = ranked_of(hit.chunk_id) else {
            continue;
        };
        let content = parents
            .get(&hit.chunk_id)
            .map(|p| truncate_chars(p, PARENT_CONTEXT_CAP))
            .unwrap_or_else(|| hit.content.clone());
        out_chunks.push(knowledge_hit_json(
            ranked_hit,
            kw_stage.as_deref(),
            conservative,
            content,
        ));
    }
    // §12-2: the entity→wiki soft link, computed once for all hits
    // (one pass over the wiki dir, not one per entity). With the graph leg OFF
    // there are no entities to link, so the pass is skipped rather than run over
    // an empty list — the cost of a leg that is off must be zero, and it reads the
    // same either way.
    let related_wiki = if legs.graph {
        crate::wiki::entity_related_pages(
            state.knowledge.as_ref(),
            &entities.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
        )
    } else {
        std::collections::HashMap::new()
    };
    let mut out_entities: Vec<serde_json::Value> = Vec::new();
    for e in entities {
        let facts = entity_facts
            .get(&e.id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(relation, other, fact)| {
                serde_json::json!({ "relation": relation, "with": other, "fact": fact })
            })
            .collect::<Vec<_>>();
        // Graph-guided navigation: what references this entity in the
        // knowledge base and the memories (ids + one-line excerpts only
        // — the agent pulls full content on demand).
        let related_chunks = state
            .knowledge
            .search(&e.name, 4)
            .await
            .unwrap_or_default()
            .into_iter()
            // Precision first: either the chunk names the entity, or the
            // semantic score is strongly above the noise floor.
            .filter(|h| h.content.contains(&e.name) || h.score >= 0.45)
            .take(2)
            .map(|h| {
                serde_json::json!({
                    "chunk_id": h.chunk_id, "document": h.document,
                    "excerpt": truncate_chars(&h.content, 80),
                })
            })
            .collect::<Vec<_>>();
        // Two sources, one pool: the fused recall hits (both legs) plus the
        // entity-name keyword leg. The fused hits replaced the old raw
        // `semantic` vector; fts_related still searches by the ENTITY name,
        // which the query-keyed hits cannot do.
        let mut related_pool: Vec<(i64, String, String, String)> = mem_legs
            .hits
            .iter()
            .map(|m| {
                (
                    m.id,
                    m.store.clone(),
                    m.namespace.clone(),
                    m.content.clone(),
                )
            })
            .collect();
        related_pool.extend(
            fts_related(&state, &e.name)
                .await
                .into_iter()
                .map(|(id, store, ns, content, _)| (id, store, ns, content)),
        );
        let related_memories: Vec<serde_json::Value> = related_pool
            .into_iter()
            .filter(|(_, _, _, c)| c.contains(&e.name))
            .take(2)
            .map(|(id, store, ns, content)| {
                serde_json::json!({
                    "id": id, "store": store, "namespace": ns,
                    "title": truncate_chars(&content, 60),
                })
            })
            .collect();
        // map keys are lowercased entity names (the match is too)
        let wiki_pages = related_wiki
            .get(&e.name.trim().to_lowercase())
            .cloned()
            .unwrap_or_default();
        let related =
            if related_chunks.is_empty() && related_memories.is_empty() && wiki_pages.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::json!({
                    "chunks": related_chunks, "memories": related_memories,
                    "wiki": wiki_pages,
                })
            };
        out_entities.push(if conservative {
            serde_json::json!({
                "kind": "entity", "id": e.id, "name": e.name, "entity_kind": e.kind,
                "facts": facts, "related": related,
                "hint": "call graph_entity(id) for facts and neighbors",
            })
        } else {
            serde_json::json!({
                "kind": "entity", "id": e.id, "name": e.name, "entity_kind": e.kind,
                "summary": e.summary, "facts": facts, "related": related,
            })
        });
    }

    // M6: usage log for threshold tuning -- one row per call with the
    // per-section counts and raw top scores (what the filters kept vs
    // dropped). Fire-and-forget; local sqlite, sub-millisecond.
    //
    // source (t251) records WHO DECLARED this call: what the caller said, or
    // NULL. NULL means unknown -- and it is what every row written before this
    // column means, so a reader must render NULL as "unknown (pre-0018)"
    // instead of defaulting it to a real caller.
    {
        let db = state.mgr.db().clone();
        let query: String = q.q.chars().take(200).collect();
        let strategy = if conservative {
            "conservative"
        } else {
            "aggressive"
        };
        let (nm, nk, nw, ne) = (
            out_memories.len() as i64,
            out_chunks.len() as i64,
            out_wiki.len() as i64,
            out_entities.len() as i64,
        );
        let tm = mem_legs.top_semantic_score;
        let declared = source.clone();
        // t19 (contract §3.1): the row records the SCALE of every score it
        // carries, the page's provenance, and the graph legs. "A column exists" is
        // not "a column is filled"; NULL still means UNKNOWN and the pre-0019 rows
        // are never rewritten.
        let selected_ids: Vec<i64> = out_chunks
            .iter()
            .filter_map(|c| c.get("chunk_id").and_then(|v| v.as_i64()))
            .collect();
        let top_legs_json = serde_json::json!(
            ranked
                .iter()
                .map(|r| serde_json::json!({
                    "chunk_id": r.hit.chunk_id,
                    "score": r.hit.score,
                    "score_kind": r.score_kind.as_str(),
                    "semantic": r.semantic.as_ref().map(|e| serde_json::json!({
                        "rank": e.rank, "raw": e.raw_score, "kind": e.kind.as_str(),
                    })),
                    "keyword": r.keyword.as_ref().map(|e| serde_json::json!({
                        "rank": e.rank, "raw": e.raw_score, "kind": e.kind.as_str(),
                    })),
                    "relevance": r.relevance.as_ref().map(|v| serde_json::json!({
                        "value": v.value, "kind": v.kind.as_str(), "version": v.version,
                    })),
                }))
                .collect::<Vec<_>>()
        )
        .to_string();
        let rejected_ids: Vec<i64> = ranked
            .iter()
            .map(|r| r.hit.chunk_id)
            .filter(|id| !selected_ids.contains(id))
            .collect();
        let candidates_json = serde_json::json!({
            "candidates": candidates,
            "leg_window": leg_window,
            "fusion": fusion_label.clone(),
            "ranked_page": ranked.len(),
            // t5: which legs this call did NOT query. NOT on `top_legs_json`: that
            // column is a JSON ARRAY (one entry per ranked hit), so a key would
            // change its shape for every existing reader — a default-config wire
            // change, which is what this increment forbids. The object it lands in
            // is the log's candidate-provenance row, which grows with the fusion.
            "disabled_legs": legs_disabled.clone(),
            // t2: the memory fusion's effective weights, beside the knowledge
            // `fusion` label above. Same object, same reason the `disabled_legs`
            // key above lives here: the recorder's additive keys belong in an
            // OBJECT, not in the per-hit ARRAY. The key is prefixed because `fusion`
            // is already the knowledge label in this row — a reader gets both
            // fusions, neither shadowing the other.
            "memory_fusion": memory_fusion_label.clone(),
        })
        .to_string();
        let selected_json = serde_json::json!({
            "memories": nm,
            "knowledge": nk,
            "wiki": nw,
            "entities": ne,
            "knowledge_chunk_ids": selected_ids,
            "graph_entities": graph_entities,
            "graph_paths": graph_paths,
        })
        .to_string();
        let rejected_json = serde_json::json!({
            "below_min_score_or_outside_top_n": rejected_ids,
            "min_score": min_score,
            "graph_empty_reason": graph_empty_reason.clone(),
            "graph_truncated_by": graph_truncated_by.clone(),
        })
        .to_string();
        // The closure is `move`: keep the response's own copy of the fusion label.
        let fusion_for_row = fusion_label.clone();
        // ...and the log's own copy of the query, which the closure takes by move.
        // Same 200-char cap the row itself would have stored, so the announcement
        // below and the row describe the same call.
        let evidence_query = query.clone();
        // FIRE-AND-FORGET BY DESIGN, AND ANNOUNCED WHEN IT FAILS (t6).
        //
        // Recording must never break recall: the response is returned whether or not
        // this row lands, and this is deliberately NOT a propagation point (the task
        // path answers 500 for the same contention — a different decision for a
        // different write). That choice is kept.
        //
        // What was NOT deliberate is the SILENCE. This call used to be
        // `let _ = db.call(...)`, which discards the nested `Result` the store's own
        // docs warn about (t324/t327): its outer layer only reports a dead writer, so
        // a failed statement and a successful call were the same reading. Measured
        // under a write lock the 5s `busy_timeout` cannot absorb: recall answered 200
        // with a normal body while the `recall_log` row was dropped and NOTHING was
        // written to the daemon log — a hole in exactly the row increment 3 made the
        // fusion weights auditable through. `call_flat` collapses both layers into the
        // real error, which is announced below.
        if let Err(e) = db
            .call_flat(move |conn| -> Result<(), rusqlite::Error> {
                conn.execute(
                    "INSERT INTO recall_log
                        (ts, query, strategy, top_n, memories, knowledge, wiki, entities,
                         top_memory_score, top_knowledge_score, source,
                         top_knowledge_relevance, top_knowledge_relevance_kind,
                         knowledge_leg_window, scoring_version,
                         graph_entities, graph_paths,
                         score_kind, fusion, top_legs_json, candidates_json,
                         selected_json, rejected_json)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,
                             ?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",
                    rusqlite::params![
                        chrono::Utc::now().to_rfc3339(),
                        query,
                        strategy,
                        top_n as i64,
                        nm,
                        nk,
                        nw,
                        ne,
                        tm,
                        top_knowledge_score,
                        declared,
                        top_knowledge_relevance,
                        top_knowledge_relevance_kind,
                        leg_window,
                        scoring_version,
                        graph_entities,
                        graph_paths,
                        top_score_kind,
                        fusion_for_row,
                        top_legs_json,
                        candidates_json,
                        selected_json,
                        rejected_json,
                    ],
                )?;
                // Retention (t251): the log is a tuning SAMPLE, not a ledger.
                // The self-check probes alone wrote 554 of the 574 live rows
                // (t247), and unbounded growth turns "the recent calls" into
                // "every call ever". Keep the newest RECALL_LOG_KEEP; a no-op
                // while the table is smaller than that.
                if let Err(e) = conn.execute(
                    "DELETE FROM recall_log WHERE id <= (SELECT MAX(id) FROM recall_log) - ?1",
                    [RECALL_LOG_KEEP],
                ) {
                    // A failed SWEEP is a DIFFERENT loss from a failed INSERT: the
                    // evidence row landed, only the cap was not enforced. Announcing
                    // it with the dropped-row wording below would overstate the hole,
                    // so it gets its own line instead of propagating.
                    tracing::warn!(
                        error = %e,
                        "recall_log retention sweep failed (rows kept past the cap)",
                    );
                }
                Ok(())
            })
            .await
        {
            // ONE line per dropped row, at WARN: the request was still served, so this
            // is a degraded audit trail rather than a request failure. No collapsing
            // and no rate limit — the attempt happens once per recall request, so the
            // line rate is bounded by the request rate, and the NUMBER of recalls that
            // lost their evidence is the property being announced (deduplicating
            // repeats would hide exactly that count). The fields are what identifies
            // the call, plus the SQLite error that stopped the row.
            tracing::warn!(
                error = %e,
                query = %evidence_query,
                strategy = %strategy,
                top_n = top_n,
                source = ?source,
                "recall evidence row was NOT recorded (recall still answered)",
            );
        }
    }
    Ok(Json(serde_json::json!({
        "strategy": if conservative { "conservative" } else { "aggressive" },
        // The caller's declared source, echoed back: a caller can see that its
        // declaration was recorded, and a caller that declared nothing sees
        // null rather than a guessed label.
        "source": source,
        // H-2 (t19): a score is only readable together with its scale, the window
        // it was ranked in and the calibration version — emitted next to each
        // other, never separately (the pre-0019 rows' window was `limit.max(10)`).
        "scoring": {
            "score_kind": top_score_kind,
            "fusion": fusion_label,
            "leg_window": leg_window,
            "scoring_version": scoring_version,
            "candidates": candidates,
            "top_knowledge_score": top_knowledge_score,
            "top_knowledge_relevance": top_knowledge_relevance,
            "top_knowledge_relevance_kind": top_knowledge_relevance_kind,
        },
        // DEP-1/DEP-3 (t19): the graph leg's own counts, from the seed resolver and
        // one retrieval — these are the numbers the telemetry row records.
        // `graph_edges_total` is the graph's SIZE, not "edges at the instant"
        // (RV-C): it does not move with `as_of`, and reading it as temporal is the
        // mistake this key name exists to prevent.
        "graph": {
            "entities": graph_entities,
            "paths": graph_paths,
            "graph_edges_total": graph_edges_total,
            "empty_reason": graph_empty_resp,
            "truncated_by": graph_truncated_resp,
        },
        // t5: the legs this call did not query, sorted, config-derived. A reader can
        // now tell "nothing matched" from "nothing was asked for" — without it an
        // all-off recall is an empty success that looks like a retrieval miss
        // (design §11.3/§11.4).
        "legs_disabled": legs_disabled,
        // t2 (§20.2, memory half): the weights that produced the `memories` section
        // above, as the knowledge side's `scoring.fusion` publishes its own. A
        // top-level, additive key next to `legs_disabled` — no existing key changes
        // type, meaning or shape. Without it the memory half of a tuning change was
        // unauditable: the response carried hit COUNTS, so a reader could not tell a
        // different weight from a different corpus. A disabled leg reads 0 here
        // (and is named in `legs_disabled`), so "off" is never confused with a
        // weight the fusion used.
        "memory_fusion": memory_fusion_label,
        "memories": out_memories,
        // What each memory leg did. Before t251 the response could not
        // distinguish "the keyword leg added nothing" from "the keyword leg was
        // discarded" -- both looked like memories.len() == top_n.
        "memory_legs": {
            "semantic": mem_legs.semantic,
            "keyword": mem_legs.keyword,
            "keyword_new": mem_legs.keyword_new,
            "returned": out_memories.len(),
            "dropped_by_top_n": mem_legs.dropped_by_top_n,
            "top_semantic_score": mem_legs.top_semantic_score,
        },
        "knowledge": out_chunks,
        // SS13-2: wiki hits are a separate section -- never merged into
        // knowledge -- so consumers can distinguish generated pages and
        // downweight or verify them.
        "wiki": out_wiki,
        "entities": out_entities,
    })))
}

/// `GET /api/v1/forget-report?content_hash=<64hex>` (INT46-1, t47).
///
/// What is still there for a hash, in two SEPARATE lists: the local residuals
/// (rows/files that still carry the content) and the external ones, exactly as
/// `crates/memory`'s report computed them -- this handler adds no second opinion.
///
/// An unknown hash is an EMPTY READING, NOT a 404 and NOT a silent fallback: the
/// report answers "what is left for this hash", it never claims the hash was
/// written. Read `local[].status.readout.hits` -- every surface answers 0 and
/// `sample` is empty. **`total` is NOT a residual count**: it is the number of
/// surface readings the report carries (10 in this generation), which is the same
/// for a written and an unwritten hash, so it cannot distinguish them. (Measured:
/// a never-written `aaaa...` hash answers 200 with `total: 10` and all hits 0.)
/// A malformed hash is a 400 instead, so the two answers differ by shape.
#[derive(Deserialize)]
struct ForgetReportQuery {
    content_hash: String,
}

async fn forget_report(
    State(state): State<AppState>,
    Query(q): Query<ForgetReportQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let hash = q.content_hash.trim();
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request(
            "content_hash must be 64 hex characters",
        ));
    }
    let report = ruagent_memory::lifecycle::forget_report(state.mgr.db(), hash).await?;
    Ok(Json(serde_json::json!({
        "content_hash": report.content_hash,
        "local": report.local,
        "external": report.external,
        // Derived here, once: a reader that adds the lists up itself would be a
        // second answer to the same question.
        "total": report.local.len() + report.external.len(),
    })))
}

/// Recent recall calls with per-section counts and raw top scores -- the M6
/// tuning dataset (which sections came back empty, what the filters dropped),
/// plus the declared source and the retention state.
#[derive(Deserialize)]
struct RecallLogQuery {
    #[serde(default)]
    limit: Option<u32>,
    /// Optional source filter. The literal "unknown" selects the rows that
    /// predate the column (source IS NULL) -- those rows are NOT filtered out
    /// by default, because dropping 574 rows of history would look tidier than
    /// it is.
    #[serde(default)]
    source: Option<String>,
}

async fn recall_log(
    State(state): State<AppState>,
    Query(q): Query<RecallLogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let filter = q
        .source
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());
    let filter_tx = filter.clone();
    let rows = state
        .mgr
        .db()
        .call(
            move |conn| -> Result<Vec<serde_json::Value>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT ts, query, strategy, top_n, memories, knowledge, wiki, entities,
                            top_memory_score, top_knowledge_score, source,
                            CAST(score_kind AS TEXT), CAST(knowledge_leg_window AS INTEGER),
                            CAST(scoring_version AS INTEGER), CAST(fusion AS TEXT),
                            CAST(top_knowledge_relevance AS REAL),
                            CAST(top_knowledge_relevance_kind AS TEXT),
                            CAST(top_legs_json AS TEXT), CAST(candidates_json AS TEXT),
                            CAST(selected_json AS TEXT), CAST(rejected_json AS TEXT),
                            CAST(graph_entities AS INTEGER), CAST(graph_paths AS INTEGER)
                       FROM recall_log
                      WHERE (?2 IS NULL
                             OR (?2 = 'unknown' AND source IS NULL)
                             OR source = ?2)
                      ORDER BY id DESC LIMIT ?1",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![limit, filter_tx], |r| {
                        let source: Option<String> = r.get(10)?;
                        Ok(serde_json::json!({
                            "ts": r.get::<_, String>(0)?,
                            "query": r.get::<_, String>(1)?,
                            "strategy": r.get::<_, String>(2)?,
                            "top_n": r.get::<_, i64>(3)?,
                            "memories": r.get::<_, i64>(4)?,
                            "knowledge": r.get::<_, i64>(5)?,
                            "wiki": r.get::<_, i64>(6)?,
                            "entities": r.get::<_, i64>(7)?,
                            "top_memory_score": r.get::<_, Option<f64>>(8)?,
                            "top_knowledge_score": r.get::<_, Option<f64>>(9)?,
                            // null is preserved as null: it is a fact about the
                            // row (written before the column existed), not a
                            // missing value to be papered over.
                            "source": source,
                            // t59 (F1): every column the writer stores is read
                            // back here (contract §1.2 "新列一律透出"; its L52 row
                            // lists them). The CASTs make this independent of the
                            // migrations' declared types -- SQLite is dynamically
                            // typed, so reading an INTEGER column as String would
                            // fail at runtime. NULL stays null: a row written
                            // before the column existed is a fact about the row,
                            // not a gap to paper over (same rule as above).
                            "score_kind": r.get::<_, Option<String>>(11)?,
                            "knowledge_leg_window": r.get::<_, Option<i64>>(12)?,
                            "scoring_version": r.get::<_, Option<i64>>(13)?,
                            "fusion": r.get::<_, Option<String>>(14)?,
                            "top_knowledge_relevance": r.get::<_, Option<f64>>(15)?,
                            "top_knowledge_relevance_kind": r.get::<_, Option<String>>(16)?,
                            "top_legs_json": r.get::<_, Option<String>>(17)?,
                            "candidates_json": r.get::<_, Option<String>>(18)?,
                            "selected_json": r.get::<_, Option<String>>(19)?,
                            "rejected_json": r.get::<_, Option<String>>(20)?,
                            "graph_entities": r.get::<_, Option<i64>>(21)?,
                            "graph_paths": r.get::<_, Option<i64>>(22)?,
                            "source_label": match &source {
                                Some(s) => s.clone(),
                                None => "unknown (pre-0018)".to_string(),
                            },
                        }))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            },
        )
        .await??;
    let retention = state
        .mgr
        .db()
        .call(|conn| -> Result<serde_json::Value, rusqlite::Error> {
            conn.query_row(
                "SELECT COUNT(*), MIN(ts), MAX(ts), COALESCE(SUM(source IS NULL), 0)
                   FROM recall_log",
                [],
                |r| {
                    Ok(serde_json::json!({
                        "policy": "keep the newest rows; older rows are pruned on write",
                        "max_rows": RECALL_LOG_KEEP,
                        "rows": r.get::<_, i64>(0)?,
                        "oldest_ts": r.get::<_, Option<String>>(1)?,
                        "newest_ts": r.get::<_, Option<String>>(2)?,
                        "rows_without_source": r.get::<_, i64>(3)?,
                    }))
                },
            )
        })
        .await??;
    Ok(Json(serde_json::json!({
        // t83 HANDED BACK (kept as `log`): captain adjudicated that this key moves
        // to the contract's `rows`, and the daemon half was made to work --
        // `test --workspace` went green (56 ok, `recall_calls_are_logged ... ok`)
        // with `api.rs` + `crates/daemon/tests/knowledge_api.rs` renamed together.
        // The PANEL half cannot be finished from `panel/src/views/Memory.tsx`
        // alone: `Awaited<ReturnType<typeof api.recallLog>>["rows"]` has no type
        // (TS7006 at Memory.tsx:344) because the response type of `api.recallLog`
        // lives in the panel's api client, outside this unit's inScope -- and
        // typing the state as `MemoryRow[]` instead made three render sites fail
        // (TS2339 on `top_knowledge_score`/`ts`). So the wire keeps `log` until a
        // unit that owns the whole panel can move all consumers at once.
        "log": rows,
        "retention": retention,
        "source_filter": filter,
    })))
}

/// Rows kept in recall_log (t251). The log is a tuning SAMPLE, not a ledger:
/// the self-check probes alone wrote 554 of the 574 live rows (t247), and
/// unbounded growth turns "the recent calls" into "every call ever". 5000 is
/// roughly a month of current usage and a no-op while the table is smaller.
const RECALL_LOG_KEEP: i64 = 5000;

/// The declared recall source: the query parameter wins, then the
/// x-ruagent-recall-source header. None means the caller declared nothing,
/// which is recorded as NULL -- never inferred from the request.
///
/// WHY NOT INFER IT: the only signal that separates the self-check probe from
/// a real user is the query text ("kettle material"), and a column filled by
/// pattern-matching query text is a second, wrong answer to a question the
/// writer can answer correctly. The caller declares; the daemon records.
fn declared_source(
    param: &Option<String>,
    headers: &HeaderMap,
) -> Result<Option<String>, ApiError> {
    let raw = param.clone().or_else(|| {
        headers
            .get("x-ruagent-recall-source")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });
    let Some(raw) = raw else { return Ok(None) };
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        // An explicitly empty value declares nothing; it is not an error.
        return Ok(None);
    }
    if s.len() > 32
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(ApiError::bad_request(
            "source must be 1..=32 chars of [a-z0-9_-] (probe, user, distill, mcp, cli, ...)",
        ));
    }
    Ok(Some(s))
}

/// Round for display without inventing precision: RRF scores live around 1/61,
/// so two decimals would collapse every fused row onto the same number.
/// The keyword leg's construction, as a stable lower-case label
/// (t261: precision -> prefix -> bigram -> substring; "empty" = the leg RAN and
/// found nothing).
///
/// t12: `keyword_leg_enabled` is the CONFIGURATION the endpoint holds, not a
/// producer state. A deliberately switched-off leg reports `"disabled"`, so "you
/// turned this off" cannot be read as "this ran and found nothing" — the whole
/// point of a switch being visibly a switch (design §11.3).
///
/// WHY THE OVERRIDE LIVES HERE AND NOT IN THE ENUM: `KeywordStage` is the
/// producer's own state, and `crates/knowledge` never sees a configuration — it
/// only knows what the legs it ran produced. Adding a `Disabled` variant would put
/// a CONSUMER's concern into a producer's state enum, so the label is composed at
/// the endpoint, from the leg config it passed to `search_page_with`.
fn keyword_stage_label(
    stage: &ruagent_knowledge::store::KeywordStage,
    keyword_leg_enabled: bool,
) -> String {
    if !keyword_leg_enabled {
        return "disabled".to_string();
    }
    format!("{stage:?}").to_lowercase()
}

/// One knowledge hit as JSON. ONE constructor, because the recall endpoint and
/// the knowledge-search endpoint must carry the SAME key names: a second copy
/// is exactly how the two would drift (t290).
///
/// Per-leg evidence (t250/t261): `legs` names the legs that found the chunk;
/// `semantic_rank` / `semantic_score` and `keyword_rank` / `keyword_score`
/// carry that leg's OWN raw score (semantic: LanceDB distance, lower is closer;
/// keyword: bm25, more negative is better — 0.0 in the substring stage, where
/// FTS never matched and there is no bm25 to report); query_keyword_stage names
/// the construction that produced the keyword leg. A leg that did NOT find the
/// chunk is `null`, never 0: 0 is a legal score, `null` is "this leg missed"
/// (t290 — the panel would otherwise read a miss as a perfect match).
fn knowledge_hit_json(
    ranked: &ruagent_knowledge::store::RankedHit,
    stage: Option<&str>,
    conservative: bool,
    content: String,
) -> serde_json::Value {
    let hit = &ranked.hit;
    // Each score is emitted NEXT TO its scale (H-1): `semantic_score` without
    // `semantic_score_kind` reads as a similarity when it is a distance, and
    // `relevance` without `relevance_kind` reads as an RRF rank when it is a
    // calibrated [0,1]. The kinds come from the leg/relevance objects themselves,
    // never from a second literal that could drift (F-5).
    let leg = |l: &Option<ruagent_knowledge::store::LegEvidence>| match l {
        Some(e) => (
            Some(e.rank as i64),
            Some(round_to(e.raw_score as f64, 4)),
            Some(e.kind.as_str()),
        ),
        None => (None, None, None),
    };
    let (sem_rank, sem_score, sem_kind) = leg(&ranked.semantic);
    let (kw_rank, kw_score, kw_kind) = leg(&ranked.keyword);
    let mut found_in: Vec<&str> = Vec::new();
    if ranked.semantic.is_some() {
        found_in.push("semantic");
    }
    if ranked.keyword.is_some() {
        found_in.push("keyword");
    }
    let mut obj = serde_json::json!({
        "kind": "knowledge",
        "chunk_id": hit.chunk_id,
        "document": hit.document,
        "score": hit.score,
        // The scale of `score`. From the value itself, not a copy of the literal.
        "score_kind": ranked.score_kind.as_str(),
        "legs": found_in,
        "semantic_rank": sem_rank,
        "semantic_score": sem_score,
        "semantic_score_kind": sem_kind,
        "keyword_rank": kw_rank,
        "keyword_score": kw_score,
        "keyword_score_kind": kw_kind,
        "query_keyword_stage": stage,
        // The calibrated display relevance (a DIFFERENT quantity from `score`:
        // [0,1] cosine-derived, not an RRF rank). `null` when the query had no
        // semantic leg to calibrate against.
        "relevance": ranked.relevance.as_ref().map(|r| round_to(r.value as f64, 4)),
        "relevance_kind": ranked.relevance.as_ref().map(|r| r.kind.as_str()),
        "relevance_version": ranked.relevance.as_ref().map(|r| r.version),
        "relevance_query_background": ranked
            .relevance
            .as_ref()
            .map(|r| round_to(r.query_background as f64, 4)),
    });
    if conservative {
        obj["excerpt"] = serde_json::json!(truncate_chars(&hit.content, 80));
        obj["hint"] = serde_json::json!("call knowledge_expand(chunk_id) for the full section");
    } else {
        obj["content"] = serde_json::json!(content);
        obj["excerpt"] = serde_json::json!(truncate_chars(&hit.content, 160));
    }
    obj
}

fn round_to(v: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (v * f).round() / f
}

/// One of the four stores, or a 400 naming the valid set. The old handler
/// mapped EVERY unknown value to Observation, so store=observations (plural),
/// store=OBSERVATION and store=bogus all returned 200 with observation rows.
fn parse_store(s: &str) -> Result<ruagent_memory::MemoryStore, ApiError> {
    match s {
        "profile" => Ok(ruagent_memory::MemoryStore::Profile),
        "observation" => Ok(ruagent_memory::MemoryStore::Observation),
        "procedure" => Ok(ruagent_memory::MemoryStore::Procedure),
        "lesson" => Ok(ruagent_memory::MemoryStore::Lesson),
        other => Err(ApiError::bad_request(format!(
            "unknown store {other}: expected one of profile, observation, procedure, lesson"
        ))),
    }
}

/// FTS memories matching `term` (the keyword leg for entity navigation).
#[allow(clippy::type_complexity)]
async fn fts_related(state: &AppState, term: &str) -> Vec<(i64, String, String, String, f32)> {
    let term = term.to_string();
    state
        .mgr
        .db()
        .call(move |conn| -> Result<_, ruagent_store::DbError> {
            let pattern = format!("\"{}\"", term.replace('"', "\"\""));
            let mut stmt = conn
                .prepare(
                    "SELECT m.id, m.store, m.namespace, m.content
                       FROM memories_fts f JOIN memories m ON m.id = f.rowid
                      WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL
                        AND m.deleted_at IS NULL
                      ORDER BY rank LIMIT 3",
                )
                .map_err(ruagent_store::DbError::from)?;
            let rows = stmt
                .query_map([&pattern], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                    ))
                })
                .map_err(ruagent_store::DbError::from)?;
            Ok(rows
                .filter_map(|r| r.ok())
                .map(|(id, s, n, c)| (id, s, n, c, 0.0))
                .collect())
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default()
}

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

/// Cap on the parent section returned by aggressive recall: sections
/// can be long; context budgets cannot.
const PARENT_CONTEXT_CAP: usize = 2000;

async fn list_permissions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pending: Vec<PendingPermission> = state.mgr.pending_permissions();
    Json(serde_json::json!({ "pending": pending }))
}

#[derive(Deserialize)]
struct ResolvePermissionRequest {
    /// "allow" | "reject" | "cancel" — semantic actions; `allow` picks
    /// the first allow-kind option the agent offered.
    #[serde(default)]
    action: Option<String>,
    /// The exact option id from the inbox listing (e.g.
    /// `allow-with-updates`) — the only way to grant an allow-always
    /// style option (issue #35).
    #[serde(default)]
    option_id: Option<String>,
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

    let answer = if let Some(id) = &req.option_id {
        if !pending.choices.iter().any(|c| &c.option_id == id) {
            return Err(ApiError::bad_request(format!(
                "option `{id}` was not offered (choices: {})",
                pending
                    .choices
                    .iter()
                    .map(|c| c.option_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        ruagent_acp::permission::PermissionAnswer::Select(id.clone())
    } else {
        match req.action.as_deref() {
            Some("allow") => {
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
            Some("reject") => {
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
            Some("cancel") => ruagent_acp::permission::PermissionAnswer::Cancel,
            other => {
                return Err(ApiError::bad_request(format!(
                    "pass `option_id` (the inbox listing's exact id) or `action` (allow|reject|cancel), got {other:?}"
                )));
            }
        }
    };

    state.mgr.resolve_permission(&key, answer)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Chat: terminal-like persistent sessions (M5)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChatStartRequest {
    agent: String,
    #[serde(default)]
    model: Option<String>,
    /// Project directory the session works in; empty/absent = the
    /// daemon's per-chat scratch workspace.
    #[serde(default)]
    cwd: Option<String>,
}

async fn chat_start(
    State(state): State<AppState>,
    Json(req): Json<ChatStartRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let card = state
        .mgr
        .agent(&req.agent)
        .ok_or_else(|| ApiError::bad_request(format!("unknown agent `{}`", req.agent)))?;
    // The user's pick wins; otherwise the card's configured default
    // model ([agent.X].model / [runtime.X].model) — nobody should be
    // asked to choose a model on every chat.
    let model = req
        .model
        .filter(|m| !m.trim().is_empty())
        .or_else(|| card.model.clone());
    let cwd = req
        .cwd
        .filter(|c| !c.trim().is_empty())
        .map(std::path::PathBuf::from);
    let chat = state
        .chats
        .start(&card, model, cwd)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({
        "id": chat.id.to_string(),
        "agent": chat.agent,
        "runtime": chat.runtime,
        "model": chat.model,
        "cwd": chat.cwd.as_ref().map(|p| p.display().to_string()),
    })))
}

async fn chat_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "chats": state.chats.list() }))
}

#[derive(Deserialize)]
struct ChatMessageRequest {
    text: String,
}

async fn chat_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ChatMessageRequest>,
) -> Result<StatusCode, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    let chat = state
        .chats
        .chat(id)
        .ok_or_else(|| ApiError::not_found("chat not found"))?;
    if req.text.trim().is_empty() {
        return Err(ApiError::bad_request("empty message"));
    }
    chat.send_prompt(state.mgr.db(), state.knowledge.embedder(), req.text)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(StatusCode::ACCEPTED)
}

/// Stop the chat's in-flight reply: the outstanding prompt is cancelled
/// (`$/cancel_request`) and a `Stopped{cancelled}` event follows on the
/// stream; the session stays alive for the next prompt. Idempotent —
/// with no prompt in flight it does nothing.
async fn chat_stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    let chat = state
        .chats
        .chat(id)
        .ok_or_else(|| ApiError::not_found("chat not found"))?;
    chat.send(ChatCommand::Stop)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(StatusCode::OK)
}

/// Hand the conversation to another agent: the session restarts on the
/// target card with the prior conversation (bounded tail) as handoff
/// context on the next prompt. Returns the NEW chat's identity.
async fn chat_handoff(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: ruagent_core::RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    let name = req
        .get("agent")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("missing `agent`"))?;
    let card = state
        .mgr
        .agent(name)
        .ok_or_else(|| ApiError::bad_request(format!("unknown agent `{name}`")))?
        .clone();
    let chat = state
        .chats
        .switch_agent(id, &card)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({
        "id": chat.id.to_string(),
        "agent": chat.agent,
        "runtime": chat.runtime,
        "model": chat.model,
        "cwd": chat.cwd.as_ref().map(|p| p.display().to_string()),
    })))
}

/// Resume a closed conversation: same RunId (transcript appends,
/// history row survives), session restarted on the chat's agent with
/// the prior conversation as handoff context.
async fn chat_resume(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: ruagent_core::RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    // The chats row names the agent; resolve it to a live card.
    let label: String = state
        .mgr
        .db()
        .call({
            let key = id.to_string();
            move |conn| {
                conn.query_row("SELECT agent FROM chats WHERE id = ?1", [&key], |r| {
                    r.get(0)
                })
            }
        })
        .await
        .map_err(|_| ApiError::not_found("chat not found in history"))?
        .map_err(|_| ApiError::not_found("chat not found in history"))?;
    let card = state
        .mgr
        .agent(&label)
        .ok_or_else(|| ApiError::bad_request(format!("agent `{label}` no longer exists")))?
        .clone();
    let chat = state
        .chats
        .resume_chat(id, &card)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({
        "id": chat.id.to_string(),
        "agent": chat.agent,
        "runtime": chat.runtime,
        "model": chat.model,
        "cwd": chat.cwd.as_ref().map(|p| p.display().to_string()),
    })))
}

/// Chat SSE: replay the chat transcript, then tail live events. `StateChanged{completed}`
/// = end of chat.
async fn chat_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    let chat = state
        .chats
        .chat(id)
        .ok_or_else(|| ApiError::not_found("chat not found"))?;
    let path = state.chats.transcript_path(id);

    // Subscribe BEFORE reading the transcript (issue #42, the run_events
    // lesson): events broadcast in the read window sit in the receiver
    // instead of being lost. Both this receiver and the transcript
    // forwarder see the same ordered stream, so the buffered events are
    // a suffix of it and the replay a prefix — whatever prefix of the
    // buffer the file already contains gets skipped below.
    let mut live = chat.subscribe();
    let mut buffered = Vec::new();
    let mut lagged = false;
    loop {
        match live.try_recv() {
            Ok(event) => buffered.push(event),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => lagged = true,
            Err(_) => break,
        }
    }
    let replay = ruagent_store::read_transcript(&path).unwrap_or_default();
    let (tx, rx_stream) = tokio::sync::mpsc::unbounded_channel();
    // Teardown watch (t95). A broadcast receiver only reports Closed once EVERY
    // sender is gone -- and THIS handler holds a Chat clone, which holds the
    // session, which holds the sender. So waiting for the channel alone means
    // an attached client would never learn that its chat was deleted or
    // closed: the stream would just sit there until the client hung up.
    // The registry is therefore polled as a second, independent liveness
    // signal; a deleted/closed chat ends the stream with the same terminal
    // end event the Closed arm sends, within one tick.
    let registry = state.chats.clone();
    tokio::spawn(async move {
        for line in &replay {
            let _ = tx.send(sse_data(line));
        }
        // The overlap: the largest k where the transcript's last k
        // events equal the buffer's first k. On receiver lag the buffer
        // is incomplete and matching could skip real events — accept
        // duplicates instead (degrade, never lose).
        let mut skip = 0;
        if !lagged {
            let max_k = buffered.len().min(replay.len());
            'k: for k in (1..=max_k).rev() {
                for i in 0..k {
                    if replay[replay.len() - k + i].event != buffered[i] {
                        continue 'k;
                    }
                }
                skip = k;
                break;
            }
        }
        for event in buffered.into_iter().skip(skip) {
            if chat_sse_event(&tx, &event) {
                return;
            }
        }
        loop {
            tokio::select! {
                got = live.recv() => match got {
                    Ok(event) => {
                        if chat_sse_event(&tx, &event) {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => {
                        let _ = tx.send(sse_end(ruagent_core::RunStatus::Completed));
                        return;
                    }
                },
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                    if registry.chat(id).is_none() {
                        let _ = tx.send(sse_end(ruagent_core::RunStatus::Completed));
                        return;
                    }
                }
            }
        }
    });

    Ok(Sse::new(UnboundedReceiverStream::new(rx_stream)).keep_alive(KeepAlive::default()))
}

/// Send one live chat event over SSE; true when it terminates the stream.
fn chat_sse_event(
    tx: &tokio::sync::mpsc::UnboundedSender<Result<Event, Infallible>>,
    event: &ruagent_core::RunEvent,
) -> bool {
    let json = serde_json::to_string(event).unwrap_or_default();
    let _ = tx.send(Ok(Event::default().data(json)));
    matches!(
        event,
        ruagent_core::RunEvent::StateChanged {
            status: ruagent_core::RunStatus::Completed
        }
    )
}

#[derive(Deserialize)]
struct ChatModelRequest {
    model: Option<String>,
    /// Switch the chat to another runtime (a [runtime.X] name). The
    /// agent (and its role prompt) stays; the engine restarts.
    runtime: Option<String>,
}

async fn chat_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ChatModelRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    let chat = state
        .chats
        .chat(id)
        .ok_or_else(|| ApiError::not_found("chat not found"))?;
    // The original card (role identity + prompt + role option defaults).
    // A runtime chat names its own engine; a role chat keeps its prompt
    // regardless of which engine it currently runs on.
    let original = state
        .mgr
        .agent(&chat.agent)
        .ok_or_else(|| ApiError::bad_request("agent vanished"))?;
    // Engine switch: spawn the same agent (same role prompt) on a
    // different runtime. No runtime given: stay on the CURRENT engine
    // (a chat switched to another runtime must not fall back to the
    // role's default).
    let engine = match &req.runtime {
        Some(runtime) => state
            .mgr
            .agents()
            .into_iter()
            .find(|a| &a.name == runtime)
            .ok_or_else(|| ApiError::bad_request(format!("unknown runtime `{runtime}`")))?,
        None => state
            .mgr
            .agent(&chat.runtime)
            .unwrap_or_else(|| original.clone()),
    };
    // carry the role prompt, defaults and identity onto the engine card
    let mut target = engine.clone();
    if let Some(p) = &original.prompt {
        target.prompt = Some(p.clone());
    }
    target.options = original.options.clone();
    let (new_chat, restarted) = state
        .chats
        .switch_model(id, req.model, &target, &chat.agent)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({
        "id": new_chat.id.to_string(),
        "agent": new_chat.agent,
        "runtime": new_chat.runtime,
        "model": new_chat.model,
        "cwd": new_chat.cwd.as_ref().map(|p| p.display().to_string()),
        // "live" = same session, context preserved; "restarted" = new one.
        "switched": if restarted { "restarted" } else { "live" },
    })))
}

#[derive(Deserialize)]
struct AgentOptionsQuery {
    /// Force a fresh probe (the manual sync button) instead of serving
    /// the cached catalog. "1" and "true" both count.
    #[serde(default)]
    refresh: Option<String>,
    /// Read the catalog of a DIFFERENT runtime than the card's default
    /// (issue #37): a role chat switched to another engine needs that
    /// engine's models, not the default's.
    #[serde(default)]
    runtime: Option<String>,
}

/// The session options a runtime advertises (model, reasoning effort,
/// permission mode, …): served from the persisted catalog (instant),
/// unless `?refresh=1` forces a fresh probe. `?runtime=` overrides the
/// engine whose catalog is read.
async fn agent_options(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<AgentOptionsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let card = state
        .mgr
        .agent(&name)
        .filter(|a| a.enabled)
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    // Runtime override: read (and probe) the named engine instead of
    // the card's default one. Model catalogs are engine-specific.
    let card = match &q.runtime {
        Some(rt) if *rt != crate::chat::ChatManager::runtime_of(&card) => {
            let engine = state
                .mgr
                .agent(rt)
                .ok_or_else(|| ApiError::bad_request(format!("unknown runtime `{rt}`")))?;
            if engine.prompt.is_some() {
                return Err(ApiError::bad_request(format!(
                    "`{rt}` is a role, not a runtime"
                )));
            }
            engine
        }
        _ => card,
    };
    let (entry, cached) = if matches!(q.refresh.as_deref(), Some("1") | Some("true")) {
        let entry = state
            .chats
            .refresh_agent_options(&card)
            .await
            .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
        (entry, false)
    } else {
        state
            .chats
            .agent_options(&card)
            .await
            .map_err(|e| ApiError::bad_request(format!("{e:#}")))?
    };
    Ok(Json(serde_json::json!({
        "agent": name,
        "options": entry.options,
        "cached": cached,
        "updated_at": entry.updated_at,
    })))
}

#[derive(Deserialize)]
struct ChatsHistoryQuery {
    /// Only conversations with this agent (role or runtime name).
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

/// Chat history: every conversation the daemon ever hosted (the chats
/// table), newest first, joined with the sessions index for previews
/// and counts — the panel's history drawer.
async fn chats_history(
    State(state): State<AppState>,
    Query(q): Query<ChatsHistoryQuery>,
) -> Json<serde_json::Value> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let chats = state.chats.history(q.agent.as_deref(), limit).await;
    Json(serde_json::json!({ "chats": chats }))
}

#[derive(Deserialize)]
struct ChatOptionRequest {
    id: String,
    value: String,
}

/// Set one advertised session option live on a chat (reasoning effort,
/// permission mode, …; model goes through PATCH /chat/{id}).
async fn chat_option(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ChatOptionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    if state.chats.chat(id).is_none() {
        return Err(ApiError::not_found("chat not found"));
    }
    match state.chats.set_option(id, req.id, req.value).await {
        Ok(options) => Ok(Json(serde_json::json!({ "options": options }))),
        Err(e) => Err(ApiError::bad_request(e)),
    }
}

async fn chat_close(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    if state.chats.close(id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("chat not found"))
    }
}

/// Delete one chat-history row (the panel's forget-this-conversation).
///
/// NOT the same endpoint as DELETE /api/v1/chat/{id}:
///
/// * /chat/{id}  closes the live session and KEEPS the history row
///   (unchanged -- the panel and history-reopen use it).
/// * /chats/{id} removes the row, and stops the session first if one is
///   live so nothing keeps writing into a deleted chat.
///
/// 204 when a row was removed, 404 when the id was never there. A repeat
/// delete is a 404 no-op -- idempotent in effect, honest in the status.
async fn chat_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: RunId = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid chat id"))?;
    match state.chats.delete(id).await {
        Ok(crate::chat::ChatDelete::Deleted { .. }) => Ok(StatusCode::NO_CONTENT),
        Ok(crate::chat::ChatDelete::NotFound) => Err(ApiError::not_found("chat not found")),
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

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
    // C-INT (R-B spec, delivered in t19): an UNKNOWN store used to fall through
    // to `Observation` — the caller asked for something that does not exist and
    // got a row in a store it never named. Unknown values are now named in a 400;
    // `observation` is spelled out so the old fallback's legitimate uses still
    // work (the fallback was doing two jobs: a default AND a typo sink).
    let store = match req.store.as_str() {
        "observation" => ruagent_memory::MemoryStore::Observation,
        "profile" => ruagent_memory::MemoryStore::Profile,
        "procedure" => ruagent_memory::MemoryStore::Procedure,
        "lesson" => ruagent_memory::MemoryStore::Lesson,
        other => {
            return Err(ApiError::bad_request(format!(
                "unknown store `{other}` (observation | profile | procedure | lesson)"
            )));
        }
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
            content: req.content.clone(),
            confidence: 0.9,
            source_episode: Some(episode),
            supersedes: req.supersedes,
        },
    )
    .await?;
    // Semantic leg: embed the row with the shared embedder.
    use ruagent_memory::write::WriteOutcome as W;
    let row_id = match &outcome {
        W::Inserted(id) => Some(*id),
        W::Superseded { new, .. } => Some(*new),
        _ => None,
    };
    if let Some(id) = row_id {
        crate::memembed::embed_row(state.mgr.db(), state.knowledge.embedder(), id, &req.content)
            .await;
    }
    Ok(Json(
        serde_json::json!({ "outcome": format!("{outcome:?}") }),
    ))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default)]
    limit: Option<u32>,
    /// `?legs=false` skips the per-leg evidence pass (t290). Absent = on:
    /// the panel's #knowledge page needs the legs by default.
    #[serde(default)]
    legs: Option<bool>,
}

async fn memory_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let hits =
        ruagent_memory::query::search_fts(state.mgr.db(), &q.q, q.limit.unwrap_or(8)).await?;
    // Same derivation as the list: the recall tab renders the same rows.
    let ids: Vec<i64> = hits.iter().map(|m| m.id).collect();
    let distilled = distilled_ids(state.mgr.db(), &ids).await?;
    let hits = with_distilled(&hits, &distilled)?;
    Ok(Json(serde_json::json!({ "hits": hits })))
}

#[derive(Deserialize)]
struct MemoryListQuery {
    /// Omitted or empty = every store. An unknown value is a 400 naming the
    /// valid set, not a silent fallback to observation (t251: store=bogus used
    /// to return 200 with 13 observation rows).
    #[serde(default)]
    store: Option<String>,
    /// Omitted or empty = every namespace. The old handler defaulted to
    /// "user", so store=lesson returned 0 rows while the store held 38
    /// (t246 measured exactly that).
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

/// The episode kind a session distillation records. The panel's distilled badge
/// is derived from THIS, not from "source_episode is set" (t350).
const DISTILLED_EPISODE_KIND: &str = "run_turn";

/// Which of these memory ids came out of a session distillation.
///
/// WHY THE JOIN IS HERE AND NOT IN THE BROWSER: the rule is a fact about our
/// schema (which episode kind distillation writes), and t347 measured what a
/// coarser rule does. The prefix migration backfilled 156 rows, so
/// "source_episode is not null" marked 162 of 163 rows -- a badge on every row
/// says nothing, which is the same failure t347 removed from the body text. One
/// place owns the rule, and the panel renders a boolean.
async fn distilled_ids(
    db: &ruagent_store::Db,
    ids: &[i64],
) -> Result<std::collections::HashSet<i64>, ApiError> {
    if ids.is_empty() {
        return Ok(std::collections::HashSet::new());
    }
    let ids: Vec<i64> = ids.to_vec();
    let found = db
        .call(move |conn| -> Result<Vec<i64>, rusqlite::Error> {
            let marks = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT m.id FROM memories m
                 JOIN episodes e ON e.id = m.source_episode
                 WHERE e.kind = ?1 AND m.id IN ({marks})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() + 1);
            params.push(&DISTILLED_EPISODE_KIND);
            for id in &ids {
                params.push(id);
            }
            let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| r.get(0))?;
            rows.collect()
        })
        .await??;
    Ok(found.into_iter().collect())
}

/// The rows as the panel receives them: the stored row PLUS the derived
/// boolean. The panel never sees the join, and it never sees a raw episode id
/// it would have to interpret.
fn with_distilled(
    rows: &[ruagent_memory::MemoryRow],
    distilled: &std::collections::HashSet<i64>,
) -> Result<Vec<serde_json::Value>, ApiError> {
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let mut v = serde_json::to_value(r).map_err(anyhow::Error::from)?;
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "distilled".to_string(),
                serde_json::Value::Bool(distilled.contains(&r.id)),
            );
        }
        out.push(v);
    }
    Ok(out)
}

async fn memory_list(
    State(state): State<AppState>,
    Query(q): Query<MemoryListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = match q.store.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s) => Some(parse_store(s)?),
    };
    let namespace = match q.namespace.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(ns) => Some(ns.to_string()),
    };
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);
    let memories =
        ruagent_memory::query::all_memories(state.mgr.db(), store, namespace.clone(), limit)
            .await?;
    let matched = ruagent_memory::query::count_memories(state.mgr.db(), store, namespace).await?;
    let counts = ruagent_memory::query::store_counts(state.mgr.db()).await?;
    // The marker is DERIVED HERE (see distilled_ids): the panel gets a boolean.
    let ids: Vec<i64> = memories.iter().map(|m| m.id).collect();
    let distilled = distilled_ids(state.mgr.db(), &ids).await?;
    let memories = with_distilled(&memories, &distilled)?;
    Ok(Json(serde_json::json!({
        "memories": memories,
        // total = the rows IN THIS RESPONSE; matched = how many rows the filter
        // matches before the limit. Both are reported because the response is a
        // page: a caller that reads only one of them must not conclude the
        // store is empty (the failure this replaces: store=lesson returned 0
        // rows while counts reported 38).
        "total": memories.len(),
        "matched": matched,
        "limit": limit,
        "store": q.store,
        "namespace": q.namespace,
        "counts": counts,
    })))
}

/// Soft-delete one memory: DELETE /api/v1/memory/{id}. The row stays (with
/// deleted_at set) and memory_diffs records the retraction, so the audit can
/// still answer "what was removed, and when".
#[derive(Deserialize)]
struct MemoryDeleteQuery {
    /// `?purge=true` hard-deletes the row instead of tombstoning it (t276):
    /// the only path that removes a secret pasted into a memory. Absent keeps
    /// the t251 contract untouched — a soft delete, row still there.
    #[serde(default)]
    purge: bool,
}

async fn memory_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<MemoryDeleteQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid memory id"))?;
    if q.purge {
        return match ruagent_memory::purge_memory(state.mgr.db(), id).await? {
            ruagent_memory::PurgeOutcome::Purged { id, .. } => Ok(Json(serde_json::json!({
                "outcome": "purged",
                "id": id,
            }))),
            ruagent_memory::PurgeOutcome::NotFound => Err(ApiError::not_found("memory not found")),
        };
    }
    match ruagent_memory::delete_memory(state.mgr.db(), id).await? {
        ruagent_memory::DeleteOutcome::Deleted { id, deleted_at } => Ok(Json(serde_json::json!({
            "outcome": "deleted", "id": id, "deleted_at": deleted_at, "soft": true,
        }))),
        ruagent_memory::DeleteOutcome::AlreadyDeleted { id, deleted_at } => Err(
            ApiError::conflict(format!("memory {id} was already deleted at {deleted_at}")),
        ),
        ruagent_memory::DeleteOutcome::NotFound => Err(ApiError::not_found("memory not found")),
    }
}

/// Undo a soft delete: POST /api/v1/memory/{id}/restore.
async fn memory_restore(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid memory id"))?;
    match ruagent_memory::restore_memory(state.mgr.db(), id).await? {
        ruagent_memory::RestoreOutcome::Restored { id } => {
            Ok(Json(serde_json::json!({ "outcome": "restored", "id": id })))
        }
        ruagent_memory::RestoreOutcome::NotDeleted { id } => {
            Err(ApiError::conflict(format!("memory {id} is not deleted")))
        }
        ruagent_memory::RestoreOutcome::NotFound => Err(ApiError::not_found("memory not found")),
    }
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
    // Markdown is the source of truth: ingest writes the document file
    // and indexes it (the index is a rebuildable shadow).
    let chunks = state
        .knowledge
        .save(&req.name, &req.content)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(
        serde_json::json!({ "chunks": chunks, "file": format!("{}.md", req.name.trim().trim_end_matches(".md")) }),
    ))
}

/// `POST /api/v1/knowledge/graph/ingest?document=NAME[&dry_run=true]`
///
/// Knowledge base → knowledge graph ingestion (docs/plans §13), behind the free
/// `knowledge_ingest_graph` capability. THE CAPABILITY IS OFF BY DEFAULT, so
/// with the shipped configuration a REAL ingest is REFUSED (409, naming the id
/// and both remedies) instead of writing entities behind the user's back; the
/// knowledge base keeps behaving exactly as it does today (file + chunks).
///
/// * no `document` → a sweep, bounded by the capability's `max_docs_per_pass`
///   (default 20), in `(created_at DESC, id ASC)` order;
/// * `document=NAME` → that one document; `NAME` is the markdown path under
///   `<root>/knowledge` with the `.md` suffix removed, and either spelling works;
/// * `dry_run=true` → extract and count, write nothing, record nothing. ALWAYS
///   allowed, capability or not: pricing the ingestion before turning it on is
///   what `dry_run` exists for (§13.4).
///
/// NOTHING ON THIS PATH CALLS A MODEL. The capability is free tier and the
/// extraction is `ruagent_extract::graph_candidates` — a pure function. There is
/// no ACP run, no agent spawn and no network call to be found here; the writes
/// are SQLite and `crates/graph` only.
#[derive(Deserialize)]
struct GraphIngestQuery {
    #[serde(default)]
    document: Option<String>,
    #[serde(default)]
    dry_run: Option<bool>,
}

async fn knowledge_graph_ingest(
    State(state): State<AppState>,
    Query(q): Query<GraphIngestQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let plane = state.chats.capabilities();
    let dry_run = q.dry_run.unwrap_or(false);
    let opts = crate::knowledge_graph::IngestOptions {
        dry_run,
        ..crate::knowledge_graph::options_of(&plane)
    };
    if !dry_run && !plane.enabled(crate::capability::CapabilityId::KnowledgeIngestGraph) {
        let id = crate::capability::CapabilityId::KnowledgeIngestGraph.as_str();
        return Err(ApiError::conflict(format!(
            "capability `{id}` is off: a real ingest writes entities and relations for the \
             knowledge documents. Add `[capabilities.{id}] enabled = true` (free tier, zero \
             tokens), or resend with `dry_run=true` to price it first."
        )));
    }
    let db = state.mgr.db().clone();
    let report = match q
        .document
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        Some(name) => crate::knowledge_graph::ingest_document(&db, &state.knowledge, name, &opts)
            .await
            .map_err(|e| ApiError::bad_request(format!("{e:#}")))?,
        None => crate::knowledge_graph::sweep(&db, &state.knowledge, &opts)
            .await
            .map_err(|e| ApiError::internal(format!("{e:#}")))?,
    };
    let mut body = serde_json::to_value(&report).unwrap_or_default();
    if let Some(obj) = body.as_object_mut() {
        obj.insert("dry_run".into(), serde_json::Value::Bool(dry_run));
    }
    Ok(Json(serde_json::json!({ "ingest": body })))
}

/// `GET /api/v1/knowledge/graph/ingest/status?limit=N`
///
/// "Is my knowledge base in the graph?" — the ledger's totals plus its newest
/// rows (one per document, so this is also the list of documents already
/// ingested). Read-only: it works whether or not the capability is enabled and
/// never triggers extraction.
#[derive(Deserialize)]
struct GraphIngestStatusQuery {
    #[serde(default)]
    limit: Option<u32>,
}

async fn knowledge_graph_ingest_status(
    State(state): State<AppState>,
    Query(q): Query<GraphIngestStatusQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let db = state.mgr.db().clone();
    let limit = q.limit.unwrap_or(20).clamp(1, 500);
    let ledger = crate::knowledge_graph::ledger_status(&db, limit)
        .await
        .map_err(|e| ApiError::internal(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({ "ledger": ledger })))
}

/// `GET /api/v1/knowledge/search?q=...&limit=N[&legs=false]`
///
/// Every hit carries the per-leg evidence (t290): `legs`, `semantic_rank`,
/// `semantic_score`, `keyword_rank`, `keyword_score`, `query_keyword_stage` — the
/// SAME keys `/api/v1/recall` emits for its knowledge hits, built by the same
/// `knowledge_hit_json`, so the panel's #knowledge page can show the legs
/// without intercepting a prompt (which is all t263 could do before this).
///
/// SET vs SET, stated because a reader — and my own first probe — got this
/// wrong (t308): this endpoint returns the KNOWLEDGE-ONLY top-N, while recall's
/// knowledge hits are the CROSS-KIND fused top-N — memories, knowledge and
/// entities compete for the same `top_n` slots, so a query whose memories fill
/// the budget returns NO knowledge hits at all. The two sets therefore differ
/// by design; the per-leg fields agree key for key ON THE INTERSECTION (same
/// `compute_legs`, same leg window). Measured live: `q='deploy'` — this endpoint
/// [43,62,58,64,70] vs recall [43,41], intersection [43], differing leg keys
/// NONE; `q='wiki'` — recall's knowledge hits were [] (nothing to compare).
///
/// PAGINATION, stated because the two numbers are easy to conflate:
/// `total` is how many hits are IN THIS RESPONSE (at most `limit`); `matched`
/// is how many the query produced BEFORE the limit (the fused candidate set),
/// so with `limit=5` on a 30-chunk corpus the response reads `total: 5`,
/// `matched: 30` — equal only when the limit is not binding. `matched` is
/// bounded by the retrieval's leg window, so on a corpus larger than that
/// window it is a LOWER BOUND, and with `?legs=false` it falls back to
/// `total` (no second pass, no candidate count).
///
/// COST, stated rather than hidden: the evidence is a second pass over the same
/// two legs (ANN + bm25) — the same shape t251 documented for recall, and for
/// the same reason: `crates/knowledge` offers `search` and `search_legs` as two
/// calls, both using the same leg window, so the leg sets are identical by
/// construction. `?legs=false` is the opt-out for hot callers (the panel
/// refreshes this endpoint while the user types). A panel-side cache was
/// rejected: the cost belongs to the caller's call pattern, not to a cache the
/// daemon cannot invalidate.
async fn knowledge_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(8).clamp(1, 50);
    // H-1/H-3 (t19): the same paged entry point the recall endpoint uses, so the
    // leg fields here and there come from ONE implementation (the doc comment
    // above promises key-for-key agreement on the intersection; this is what makes
    // that promise structural rather than a coincidence of two code paths).
    let page = state
        .knowledge
        .search_page(&q.q, limit)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let leg_window = page.evidence.leg_window as i64;
    let candidates = page.evidence.candidates as i64;
    // The same re-spelling as the recall endpoint's (so both endpoints emit one
    // shape, and the recall response's `memory_fusion` — see the note at the recall
    // site — is parsed by the same code as this label). `FusionKind::label()` is
    // deliberately not called: its string is a different one, and these keys are
    // existing wire values.
    let fusion_label = match page.evidence.fusion {
        ruagent_knowledge::store::FusionKind::Rrf {
            k,
            w_semantic,
            w_keyword,
        } => format!("rrf:k={k},w_semantic={w_semantic},w_keyword={w_keyword}"),
    };
    // This endpoint runs the DEFAULT leg configuration (`search_page`), so the
    // keyword leg always ran: `true` here is not an assumption but the config this
    // request used. Only the recall endpoint can report "disabled" (t12).
    let kw_stage = Some(keyword_stage_label(&page.evidence.keyword_stage, true));
    let scoring_version = page
        .hits
        .first()
        .and_then(|r| r.relevance.as_ref())
        .map(|r| r.version);
    let total = page.hits.len();
    let matched = page.evidence.fused.len();
    // `?legs=false` keeps its old meaning: the leg keys are present but NULL (a
    // cheaper response for a caller that only wants chunk ids), never a second
    // ranking path.
    let want_legs = q.legs.unwrap_or(true);
    let mut out: Vec<serde_json::Value> = Vec::new();
    for ranked in &page.hits {
        let mut obj = knowledge_hit_json(
            ranked,
            kw_stage.as_deref(),
            false,
            ranked.hit.content.clone(),
        );
        if !want_legs {
            // `legs` stays an EMPTY ARRAY (a hit found by no recorded leg), not
            // null: the key's type must not change with the opt-out, and `[]` is
            // what "no leg evidence" has always meant here.
            obj["legs"] = serde_json::json!([]);
            for key in [
                "semantic_rank",
                "semantic_score",
                "semantic_score_kind",
                "keyword_rank",
                "keyword_score",
                "keyword_score_kind",
                "query_keyword_stage",
                "relevance",
                "relevance_kind",
                "relevance_version",
                "relevance_query_background",
            ] {
                obj[key] = serde_json::Value::Null;
            }
        }
        out.push(obj);
    }
    Ok(Json(serde_json::json!({
        "hits": out,
        "total": total,
        "matched": matched,
        // H-2: every score above is an RRF rank computed in THIS window at THIS
        // calibration version. Emitted together so a reader never has to guess
        // which scale a number is on (the pre-0019 rows used `limit.max(10)`).
        "scoring": {
            "score_kind": page
                .hits
                .first()
                .map(|r| r.score_kind.as_str())
                .unwrap_or("rrf_rank"),
            "fusion": fusion_label,
            "leg_window": leg_window,
            "scoring_version": scoring_version,
            "candidates": candidates,
        },
    })))
}

/// Resolve the routing file's agent NAMES into ids and run the cascade.
fn routing_decision(state: &AppState, task: &Task) -> Option<ruagent_core::RoutingDecision> {
    let by_name: std::collections::HashMap<String, ruagent_core::AgentCard> = state
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::extract_plane::{ExtractPlan, Extractor};

    /// A minimal `AgentCard` for the routing tests (t38/t41/t43). Nothing here reads a prompt, a
    /// `kind` or a description, which is the point: the fallback must key on neither.
    fn card(id: &str, name: &str, enabled: bool) -> ruagent_core::AgentCard {
        ruagent_core::AgentCard {
            id: id.parse().expect("an agent id"),
            name: name.to_string(),
            harness: ruagent_core::HarnessKind::Dsh,
            command: None,
            description: String::new(),
            model: None,
            reasoning_effort: None,
            context_window: None,
            mcp_profile: None,
            models: Vec::new(),
            prompt: None,
            runtime: None,
            runtimes: Vec::new(),
            options: std::collections::BTreeMap::new(),
            tags: Vec::new(),
            enabled,
        }
    }

    /// t38: the no-cascade fallback must take its agent from the SAME registry the run resolves
    /// against, so the id in the `routed` decision and the id in `params.agent` agree BY
    /// CONSTRUCTION instead of being reconciled at the emission site afterwards.
    ///
    /// The two cards below are the measured shape of ONE agent seen through two views: the registry
    /// holds the materialized card (the id the run uses), while the config file's view mints a fresh
    /// `AgentId::generate()` per load, so `GET /api/v1/agents` contains the first and not the
    /// second. Standing of this test against the PRE-CHANGE code, stated plainly: the old body was
    /// inline in the `start_run` handler (`state.config.default_agent()`), so this test could not
    /// run against it at all -- the pre-change evidence is the private-daemon reading (the emission
    /// site's reconciliation fired on EVERY run, `routed rationale = "router named <config-view id>
    /// (not the registry id of the agent that ran)"`). What this test pins is the resolution itself:
    /// point `default_route_agent` back at a config-view card and `chosen.id == registry_card.id`
    /// fails on the very first assertion.
    ///
    /// The fixture is `impl`, deliberately NOT `approver` (t43): the fallback now keys on the card
    /// `policy.toml` designates as the permission approver, so a fixture named `approver` would make a
    /// reader unable to tell whether a pass came from this rule or from that guard. The adjudicator is
    /// the subject of exactly one test, `the_fallback_skips_the_designated_permission_approver`.
    #[test]
    fn the_default_route_agent_comes_from_the_registry_view() {
        // The registry's id and the config view's per-load id, as measured live (the latter is the
        // shape of the id the config view mints, kept only to show it is not the one recorded).
        let registry_card = card("01a0b8bb-f251-739a-a5ed-1dd1c1635302", "impl", true);
        let config_view_card = card("01a0f580-b2ac-7048-a136-6bbe33d5f13b", "impl", true);
        assert_ne!(
            registry_card.id, config_view_card.id,
            "one agent, two views, two ids -- that is the trap this test guards"
        );

        let chosen = default_route_agent(std::slice::from_ref(&registry_card), None)
            .expect("a default agent");
        assert_eq!(
            chosen.id, registry_card.id,
            "the fallback records the REGISTRY's id -- the one `params.agent` will carry"
        );
        assert_ne!(
            chosen.id, config_view_card.id,
            "and never the config view's id, which no registry on this machine resolves"
        );

        // The RULE is still "the first ENABLED card", so a disabled card in front changes nothing.
        let disabled_first = card("01a0f580-0000-0000-0000-000000000000", "aaa", false);
        let both = vec![disabled_first, registry_card.clone()];
        assert_eq!(
            default_route_agent(&both, None)
                .expect("a default agent")
                .id,
            registry_card.id,
            "the disabled card is skipped, as the rule has always done"
        );
    }

    /// t43: the last-resort fallback must skip the card `policy.toml` designates as the permission
    /// approver -- keyed on that card's ID, which is what the handler passes in -- and must come back
    /// `None` (so the handler can refuse loudly) when the adjudicator is the only enabled card.
    ///
    /// The adjudicator's real name is used here on purpose: this is the one test where it is the
    /// subject, so a reader cannot mistake a pass for the fixture. Every other test in this module uses
    /// non-adjudicator cards for exactly that reason.
    ///
    /// Standing against the PRE-CHANGE bytes: `default_route_agent` took no approver argument there, so
    /// these assertions cannot run; the behavioural pre-change reading is the private-daemon
    /// reproduction in the task output (`run.result = ALLOW` on a real intent).
    #[test]
    fn the_fallback_skips_the_designated_permission_approver() {
        let adjudicator = card("01a0b915-8273-73c4-8fe9-0e35a34d4ded", "approver", true);
        let worker = card("01a0b8bb-f251-739a-a5ed-1dd1c1635302", "impl", true);
        let both = vec![adjudicator.clone(), worker.clone()];

        // Without a designated approver the rule is unchanged: the first enabled card, approver
        // included -- which is what makes the guard a guard and not a rename of the rule.
        assert_eq!(
            default_route_agent(&both, None).expect("a default").id,
            adjudicator.id,
            "no designated approver -> the rule is exactly what it was"
        );
        // With one, the card is skipped rather than chosen.
        let chosen = default_route_agent(&both, Some(adjudicator.id)).expect("a default");
        assert_eq!(
            chosen.id, worker.id,
            "the adjudicator is skipped, not selected"
        );
        assert_ne!(chosen.id, adjudicator.id);

        // Only the adjudicator enabled: this is the handler's loud-refusal state. Measured on a private
        // daemon (a config whose only enabled card is the approver): the run is refused before it spawns.
        assert!(
            default_route_agent(std::slice::from_ref(&adjudicator), Some(adjudicator.id)).is_none(),
            "only the adjudicator enabled -> None -> start_run refuses instead of adjudicating a task"
        );

        // A disabled card in front is skipped by the pre-existing rule, and the guard does not
        // resurrect it.
        let disabled = card("01a0f580-0000-0000-0000-000000000000", "aaa", false);
        let with_disabled = vec![disabled, adjudicator.clone(), worker.clone()];
        assert_eq!(
            default_route_agent(&with_disabled, Some(adjudicator.id))
                .expect("a default")
                .id,
            worker.id
        );

        // The two rationales this path can produce, in the exact words the panel will render.
        let skipped = default_rationale("impl", false, Some("approver"));
        assert!(skipped.contains("NOT the permission approver"), "{skipped}");
        assert!(
            skipped.contains("`impl`") && skipped.contains("`approver`"),
            "both names, so the reader sees what was chosen and what was skipped: {skipped}"
        );
        assert!(skipped.contains("adjudicates"), "{skipped}");
        // An operator who deliberately configures the adjudicator as the default is NOT blocked -- the
        // guard is on the fallback -- and the rationale says what they asked for.
        let deliberate = default_rationale("approver", true, Some("approver"));
        assert!(
            deliberate.contains("configured default `approver`"),
            "{deliberate}"
        );
        assert!(deliberate.contains("permission approver"), "{deliberate}");
        assert!(deliberate.contains("ALLOW/REJECT"), "{deliberate}");
    }

    /// t41: the rationale must name the AGENT and the REASON, and must keep the two default cases
    /// apart -- one is a configuration (`routing.toml`), the other is an accident of ordering.
    ///
    /// STANDING AGAINST THE PRE-CHANGE BYTES, plainly: these assertions cannot run there because
    /// neither helper existed. What the pre-change bytes DO show is the behaviour they pin, MEASURED
    /// on a private daemon minutes before this change: an unpinned run's decision was
    /// `{"source":{"level":"default"},"rationale":null}` on BOTH paths -- agent `approver` with
    /// `routing.toml` commented out, and agent `impl` with `default = "impl"` -- i.e. byte-identical
    /// text for a configuration and an accident. A reviewer should read the property as the pair
    /// `assert!(no_default.contains("approver"))` plus `assert_ne!(no_default, configured)`.
    #[test]
    fn the_default_rationale_names_the_agent_and_the_reason() {
        let no_default = default_rationale("architect", false, None);
        assert!(
            no_default.contains("architect"),
            "the NAME is what a run JSON lacks (its agent-name search is False): {no_default}"
        );
        assert!(
            no_default.contains("no default configured"),
            "the accidental case must say so, or it reads like a decision: {no_default}"
        );
        assert!(
            no_default.contains("routing.toml"),
            "and it must name where to choose, so the text is actionable: {no_default}"
        );
        assert!(
            !no_default.contains("01a0") && !no_default.contains("d4ded"),
            "an id is NOT the answer -- `params.agent` already carries it: {no_default}"
        );

        let configured = default_rationale("impl", true, Some("approver"));
        assert!(configured.contains("impl"), "{configured}");
        assert!(
            configured.contains("configured default"),
            "the configured case must be labelled as a configuration: {configured}"
        );
        assert!(
            !configured.contains("no default configured"),
            "the two cases must not read alike: {configured}"
        );

        // Only a default-level decision with an EMPTY rationale is touched. A rule decision keeps its
        // own provenance (the panel prefers `rule_id` over the rationale), and nothing is overwritten.
        let rule = ruagent_core::RoutingDecision {
            agents: vec!["01a09d69-498f-77a1-9bcd-b84a93ff4122".parse().unwrap()],
            source: ruagent_core::RouteSource::Rule {
                rule_id: "project~None+title~Some(\"x\")".into(),
            },
            rationale: None,
        };
        assert!(
            with_default_rationale(rule, "architect", true, None)
                .rationale
                .is_none()
        );
        let retry = ruagent_core::RoutingDecision {
            agents: vec!["01a0b915-8273-73c4-8fe9-0e35a34d4ded".parse().unwrap()],
            source: ruagent_core::RouteSource::Default,
            rationale: Some("retry of run x".into()),
        };
        assert_eq!(
            with_default_rationale(retry, "approver", false, None)
                .rationale
                .as_deref(),
            Some("retry of run x"),
            "an existing rationale is never overwritten"
        );

        // A silent default-level decision IS filled, and neither the level nor the chosen agent moves.
        let silent = ruagent_core::RoutingDecision {
            agents: vec!["01a0b915-8273-73c4-8fe9-0e35a34d4ded".parse().unwrap()],
            source: ruagent_core::RouteSource::Default,
            rationale: None,
        };
        let named = with_default_rationale(silent, "approver", false, None);
        let why = named.rationale.clone().unwrap_or_default();
        assert!(why.contains("approver"), "{why}");
        assert_eq!(
            named.source,
            ruagent_core::RouteSource::Default,
            "the level is unchanged"
        );
        assert_eq!(
            named.primary(),
            "01a0b915-8273-73c4-8fe9-0e35a34d4ded".parse().unwrap(),
            "which agent is chosen is NOT this task's business"
        );
    }

    /// t4: the manual distill route's optional body (§14.3). An EMPTY body is
    /// today's behaviour — one ACP turn, no dry run — and a malformed or unknown
    /// value is refused by name, never swallowed into "no body".
    #[test]
    fn the_distill_body_names_the_extractor_and_refuses_what_it_does_not_know() {
        // `ApiError` is not `Debug` (it is rendered by `IntoResponse`), so a
        // refused body is unwrapped through its message.
        fn ok(body: &[u8]) -> (ExtractPlan, bool) {
            match distill_request(body) {
                Ok(parsed) => parsed,
                Err(e) => panic!("expected a parsed distill body, got: {}", e.message),
            }
        }

        assert_eq!(ok(b""), (ExtractPlan::explicit(Extractor::Acp), false));
        assert_eq!(
            ok(br#"{"extractor":"rules"}"#),
            (ExtractPlan::explicit(Extractor::Rules), false)
        );
        assert_eq!(
            ok(br#"{"extractor":"both","dry_run":true}"#),
            (ExtractPlan::explicit(Extractor::Both), true)
        );
        assert_eq!(
            ok(br#"{"dry_run":true}"#),
            (ExtractPlan::explicit(Extractor::Acp), true),
            "an omitted extractor is the documented default, not a silent one"
        );

        let err = distill_request(br#"{"extractor":"llm"}"#).unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("llm"), "{}", err.message);
        assert!(
            err.message.contains("rules"),
            "lists the values: {}",
            err.message
        );

        let err = distill_request(b"{not json").unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("JSON"), "{}", err.message);
    }

    /// t350: the badge is derived from the episode KIND. The two kinds that
    /// exist on this machine are the two that matter: a session distillation
    /// (RunTurn) and the episode the t347 prefix migration created (Manual,
    /// whose text says the session is unknown). Marking both is what made the
    /// badge meaningless.
    #[tokio::test]
    async fn the_distilled_marker_comes_from_the_episode_kind() {
        let db = ruagent_store::Db::open_in_memory().unwrap();
        let turn = ruagent_memory::episode::record_episode(
            &db,
            ruagent_memory::episode::EpisodeKind::RunTurn,
            "a distilled session transcript",
            Some("session:1"),
        )
        .await
        .unwrap();
        let manual = ruagent_memory::episode::record_episode(
            &db,
            ruagent_memory::episode::EpisodeKind::Manual,
            "the t347 migration episode",
            Some("ruagent:legacy-distilled-prefix"),
        )
        .await
        .unwrap();
        let w = |content: &str, ep: i64| ruagent_memory::write::MemoryWrite {
            store: ruagent_memory::MemoryStore::Profile,
            namespace: ruagent_memory::namespace::Namespace::parse("user").unwrap(),
            content: content.to_string(),
            confidence: 0.9,
            source_episode: Some(ep),
            supersedes: None,
        };
        let distilled_row = match ruagent_memory::write::write_memory(
            &db,
            &w("came out of a session distillation", turn),
        )
        .await
        .unwrap()
        {
            ruagent_memory::write::WriteOutcome::Inserted(id) => id,
            other => panic!("expected Inserted, got {other:?}"),
        };
        let migrated_row = match ruagent_memory::write::write_memory(
            &db,
            &w("backfilled by the prefix migration", manual),
        )
        .await
        .unwrap()
        {
            ruagent_memory::write::WriteOutcome::Inserted(id) => id,
            other => panic!("expected Inserted, got {other:?}"),
        };
        // ApiError has no Debug impl (it is a status + message), so unwrap is not
        // available here; the test module can read the private message.
        let marks = match distilled_ids(&db, &[distilled_row, migrated_row]).await {
            Ok(m) => m,
            Err(e) => panic!("distilled_ids failed: {}", e.message),
        };
        println!(
            "READING t350: run_turn row {distilled_row} marked={} | manual row {migrated_row} marked={}",
            marks.contains(&distilled_row),
            marks.contains(&migrated_row)
        );
        assert!(
            marks.contains(&distilled_row),
            "a run_turn (session distillation) row MUST be marked"
        );
        assert!(
            !marks.contains(&migrated_row),
            "a manual (migrated, session unknown) row must NOT be marked"
        );
        // And what the panel actually receives is a BOOLEAN, present on every
        // row -- false, not missing, for the migrated one.
        let rows = ruagent_memory::query::all_memories(&db, None, None, 50)
            .await
            .unwrap();
        let json = match with_distilled(&rows, &marks) {
            Ok(v) => v,
            Err(e) => panic!("with_distilled failed: {}", e.message),
        };
        let migrated_json = json.iter().find(|v| v["id"] == migrated_row).unwrap();
        assert_eq!(migrated_json["distilled"], serde_json::Value::Bool(false));
        let distilled_json = json.iter().find(|v| v["id"] == distilled_row).unwrap();
        assert_eq!(distilled_json["distilled"], serde_json::Value::Bool(true));
    }

    /// t188: the panel directory must not be resolved against the working
    /// directory. `cargo test` runs with the CWD set to this package's root
    /// (`crates/daemon`), where a bare `panel/dist` does not exist — so a
    /// resolution that still finds an `index.html` has to have come from a
    /// CWD-independent candidate (the build tree this binary came from).
    #[test]
    fn panel_dist_does_not_depend_on_the_working_directory() {
        let cwd = std::env::current_dir().expect("cwd");
        let found = panel_dist_dir();
        assert!(
            !found.join("index.html").is_file() || found.is_absolute(),
            "resolved to the relative path {} — still CWD-dependent",
            found.display()
        );
        if found.join("index.html").is_file() {
            assert_ne!(found, std::path::PathBuf::from("panel/dist"));
            eprintln!("cwd={} -> panel dist={}", cwd.display(), found.display());
            // A second working directory, far from the repo: the whole point of
            // the fix is that the answer does not move. Process-wide chdir, so
            // run this with --test-threads=1.
            let second = std::path::PathBuf::from("C:\\");
            if std::env::set_current_dir(&second).is_ok() {
                let from_root = panel_dist_dir();
                let _ = std::env::set_current_dir(&cwd);
                eprintln!(
                    "cwd={} -> panel dist={}",
                    second.display(),
                    from_root.display()
                );
                assert_eq!(
                    from_root, found,
                    "the panel resolved differently from a different working directory"
                );
            }
        } else {
            eprintln!("panel/dist not built in this checkout — nothing to assert");
        }
    }

    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// A router over a throwaway root: real handlers, real SQLite, no daemon
    /// process. A second *process* would either collide with the running
    /// daemon's port or, worse, open the user's data root — so the session
    /// lifecycle is exercised in-process instead.
    /// A root no other test can pick. The clock alone is not enough: two
    /// parallel tests can read the same `SystemTime::now()`, land on the same
    /// path, and then fight over one SQLite file — which is how this suite
    /// produced `Sqlite(DatabaseBusy("database is locked"))` in a test that had
    /// done nothing wrong (t313). The counter cannot repeat inside a process,
    /// the pid separates processes, and the timestamp separates two processes
    /// that started in the same tick.
    fn harness_root() -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "ruagent-api-test-{}-{}-{}",
            std::process::id(),
            n,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    async fn harness() -> (Router, ruagent_store::Db, std::path::PathBuf) {
        harness_with_agents(None).await
    }

    /// `agents_toml`: written BEFORE the config is loaded, so the registry the
    /// state is built from is exactly this text (t14 drives a registry whose
    /// every agent is `enabled = false`).
    async fn harness_with_agents(
        agents_toml: Option<&str>,
    ) -> (Router, ruagent_store::Db, std::path::PathBuf) {
        let root = harness_root();
        std::fs::create_dir_all(root.join("config")).unwrap();
        if let Some(text) = agents_toml {
            std::fs::write(root.join("config").join("agents.toml"), text).unwrap();
        }
        let cfg = DaemonConfig::load(&root).unwrap();
        let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
        let knowledge = ruagent_knowledge::Knowledge::open(&root, db.clone())
            .await
            .unwrap();
        let chats = crate::chat::ChatManager::new(
            db.clone(),
            root.clone(),
            std::sync::Arc::new(|_, _| {}),
            cfg.mcp.clone(),
            crate::distill::AutoDistill::default(),
            None,
            crate::distill::AgentRegistry::default(),
        );
        let mgr = std::sync::Arc::new(RunManager::new(
            db.clone(),
            root.clone(),
            cfg.agents.clone(),
            cfg.policy.to_policy(),
            cfg.mcp.clone(),
        ));
        let state = AppState {
            mgr,
            config: std::sync::Arc::new(cfg),
            knowledge: std::sync::Arc::new(knowledge),
            chats,
            sessions: std::sync::Arc::new(crate::sessions::SessionIndexer::new(
                db.clone(),
                std::env::temp_dir(),
            )),
        };
        (router(state), db, root)
    }

    /// Insert an index row exactly the way the indexer does.
    async fn seed(db: &ruagent_store::Db, key: &str, source: &str) {
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

    async fn hit(app: &Router, method: &str, uri: &str) -> (StatusCode, serde_json::Value, String) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        (status, json, text)
    }

    fn find(v: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
        v["sessions"]
            .as_array()?
            .iter()
            .find(|s| s["key"] == key)
            .cloned()
    }

    /// Index one session whose transcript is a REAL file, the way the indexer
    /// does (`seed` above points at a path that does not exist, which is fine for
    /// a listing test and useless for a route that reads the file).
    async fn seed_transcript(db: &ruagent_store::Db, key: &str, path: &std::path::Path) {
        let key = key.to_string();
        let path = path.to_string_lossy().into_owned();
        db.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO sessions
                     (key, source, title, project, ref_path, started_at, updated_at,
                      mtime_ms, size_bytes, message_count, preview)
                 VALUES (?1,'ruagent',?2,'/w',?3,1,2,2,3,4,'p')",
                rusqlite::params![key, "t14", path],
            )
        })
        .await
        .unwrap()
        .unwrap();
    }

    /// The `distill_log` status of every attempt for one session, in write order.
    async fn distill_statuses(db: &ruagent_store::Db, key: &str) -> Vec<String> {
        let key = key.to_string();
        db.call_flat(move |conn| {
            let mut stmt =
                conn.prepare("SELECT status FROM distill_log WHERE session_key = ?1 ORDER BY id")?;
            let rows = stmt.query_map([&key], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()
        })
        .await
        .unwrap()
    }

    async fn memories_written_count(db: &ruagent_store::Db) -> i64 {
        db.call_flat(|conn| conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)))
            .await
            .unwrap()
    }

    /// t14: THE MANUAL ROUTE'S FREE TIER MUST NOT NEED AN AGENT.
    ///
    /// `{"extractor":"rules"}` names the zero-token implementation, which never
    /// touches an agent card; the machine it exists for is the one with NO agent
    /// enabled and no API key. Until the card was made lazy, this route selected
    /// an agent BEFORE consulting the plan and answered 400 `no enabled agent
    /// available` to a request that would have spawned nothing — the free tier
    /// was unreachable exactly where it was meant to work.
    ///
    /// The other half is asserted in the same drive: `{"extractor":"acp"}` on the
    /// SAME registry still refuses, by name. Lazy resolution must not become a
    /// silent success for a plan that does need a model.
    #[tokio::test]
    async fn the_manual_rules_route_runs_with_every_agent_disabled() {
        // One agent exists and is DISABLED: the registry the daemon builds from
        // `enabled = false` is empty, which is the shape this test is about.
        let (app, db, root) = harness_with_agents(Some(
            "[agent.t14none]\nharness = \"mock\"\ncommand = \"definitely-not-a-real-t14-binary\"\ndescription = \"t14: exists, disabled\"\nenabled = false\n",
        ))
        .await;

        let key = "ruagent:t14-no-agent";
        let file = root.join("data").join("transcripts").join("run-t14.jsonl");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        // A PREF marker (`记住`/`以后`) is what the deterministic tier extracts;
        // without one the pass would be `empty` and prove nothing.
        std::fs::write(
            &file,
            "{\"ts\":\"2026-09-26T14:00:01Z\",\"seq\":1,\"event\":{\"type\":\"user_message\",\"text\":\"记住：以后回答都要简洁，不要长篇大论。\"}}\n\
             {\"ts\":\"2026-09-26T14:00:02Z\",\"seq\":2,\"event\":{\"type\":\"agent_message_chunk\",\"text\":\"understood\"}}\n",
        )
        .unwrap();
        seed_transcript(&db, key, &file).await;

        let (st, json, raw) = send_json(
            &app,
            "POST",
            &format!("/api/v1/sessions/{key}/distill"),
            Some(serde_json::json!({ "extractor": "rules" })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        assert_eq!(json["distilled"]["source"], "rules", "{raw}");
        assert_eq!(
            json["distilled"]["agent"], "",
            "a rules pass names no agent: {raw}"
        );
        assert!(
            json["distilled"]["memories_written"].as_u64().unwrap_or(0) > 0,
            "the free tier must WRITE, not just answer 200: {raw}"
        );
        assert!(
            memories_written_count(&db).await > 0,
            "a memory row must exist after the rules pass"
        );
        let statuses = distill_statuses(&db, key).await;
        assert_eq!(statuses, vec!["ok".to_string()], "{statuses:?}");

        // THE CONTROL: the ACP tier on the same empty registry still refuses, and
        // says why. It must NOT turn into a success just because the free tier now
        // runs cardless.
        let (st, _json, raw) = send_json(
            &app,
            "POST",
            &format!("/api/v1/sessions/{key}/distill"),
            Some(serde_json::json!({ "extractor": "acp" })),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{raw}");
        assert!(
            raw.contains("no enabled agent available"),
            "the refusal must name the reason: {raw}"
        );
        assert_eq!(
            distill_statuses(&db, key).await,
            vec!["ok".to_string()],
            "a refused request writes no log row"
        );
    }

    /// POST/GET with a JSON body, for routes that take one.
    async fn send_json(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value, String) {
        let mut b = Request::builder().method(method).uri(uri);
        if body.is_some() {
            b = b.header("content-type", "application/json");
        }
        let resp = app
            .clone()
            .oneshot(
                b.body(match &body {
                    Some(v) => Body::from(v.to_string()),
                    None => Body::empty(),
                })
                .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        (status, json, text)
    }

    /// t290: knowledge/search must carry the SAME per-leg evidence as recall's
    /// knowledge hits, hit for hit; and a leg that missed must be `null`, not 0.
    #[tokio::test]
    async fn knowledge_search_legs_match_recall_and_absent_legs_are_null() {
        let (app, _db, _root) = harness().await;
        let doc = "# Deploy guide\n\nThe deploy pipeline runs on Tuesdays from the release branch.\n\n# Tea notes\n\nEarl grey tastes best with a slice of lemon.\n";
        let (st, _, raw) = send_json(
            &app,
            "POST",
            "/api/v1/knowledge/ingest",
            Some(serde_json::json!({ "name": "t290-doc", "content": doc })),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{raw}");

        let q = "deploy%20pipeline%20tuesdays";
        let (s1, v1, raw1) = send_json(
            &app,
            "GET",
            &format!("/api/v1/knowledge/search?q={q}&limit=5"),
            None,
        )
        .await;
        assert_eq!(s1, StatusCode::OK, "{raw1}");
        let hits = v1["hits"].as_array().cloned().unwrap_or_default();
        assert!(!hits.is_empty(), "expected hits: {raw1}");
        println!(
            "READING knowledge/search: total={} matched={} first hit keys={:?}",
            v1["total"],
            v1["matched"],
            hits[0]
                .as_object()
                .map(|o| o.keys().cloned().collect::<Vec<_>>())
        );

        let (s2, v2, raw2) = send_json(
            &app,
            "GET",
            &format!("/api/v1/recall?q={q}&top_n=5&source=t290-test"),
            None,
        )
        .await;
        assert_eq!(s2, StatusCode::OK, "{raw2}");
        let rec_knowledge = v2["knowledge"].as_array().cloned().unwrap_or_default();
        assert!(!rec_knowledge.is_empty(), "recall knowledge empty: {raw2}");

        // Hit for hit: the same chunk must carry the same leg fields.
        for h in &hits {
            let id = h["chunk_id"].as_i64().unwrap();
            let other = rec_knowledge
                .iter()
                .find(|k| k["chunk_id"].as_i64() == Some(id))
                .unwrap_or_else(|| panic!("chunk {id} missing from recall: {raw2}"));
            for key in [
                "legs",
                "semantic_rank",
                "semantic_score",
                "keyword_rank",
                "keyword_score",
                "query_keyword_stage",
            ] {
                assert_eq!(h[key], other[key], "key {key} differs on chunk {id}");
            }
        }
        // The null invariant, on whatever the corpus produced: a leg that did
        // not find the chunk reports null, never 0.
        let mut null_cases = 0;
        for h in &hits {
            let legs: Vec<String> = h["legs"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            if !legs.iter().any(|l| l == "keyword") {
                assert!(h["keyword_score"].is_null(), "{h}");
                assert!(h["keyword_rank"].is_null(), "{h}");
                null_cases += 1;
            }
            if !legs.iter().any(|l| l == "semantic") {
                assert!(h["semantic_score"].is_null(), "{h}");
            }
            assert!(h["query_keyword_stage"].is_string() || h["query_keyword_stage"].is_null());
        }
        println!("READING null-leg cases in this query: {null_cases}");

        // total vs matched: equal while the limit is not binding, different
        // once it bites (the acceptance's 5/161 shape).
        let (s3, v3, raw3) = send_json(
            &app,
            "GET",
            &format!("/api/v1/knowledge/search?q={q}&limit=1"),
            None,
        )
        .await;
        assert_eq!(s3, StatusCode::OK, "{raw3}");
        println!(
            "READING pagination: limit=1 -> total={} matched={} (hits={})",
            v3["total"],
            v3["matched"],
            v3["hits"].as_array().map(|a| a.len()).unwrap_or(0)
        );
        assert_eq!(v3["total"], 1, "{raw3}");
        assert!(
            v3["matched"].as_u64().unwrap_or(0) > 1,
            "matched must count before the limit: {raw3}"
        );

        // ?legs=false is the documented opt-out: no evidence pass, and the
        // hits keep the same shape minus the leg fields' values.
        let (s4, v4, raw4) = send_json(
            &app,
            "GET",
            &format!("/api/v1/knowledge/search?q={q}&limit=1&legs=false"),
            None,
        )
        .await;
        assert_eq!(s4, StatusCode::OK, "{raw4}");
        assert!(v4["hits"][0]["keyword_score"].is_null(), "{raw4}");
        assert!(
            v4["hits"][0]["legs"]
                .as_array()
                .map(|a| a.is_empty())
                .unwrap_or(false),
            "{raw4}"
        );
        println!(
            "READING legs=false: legs={} query_keyword_stage={}",
            v4["hits"][0]["legs"], v4["hits"][0]["query_keyword_stage"]
        );
    }

    /// t300: `?dry_run=true` used to be accepted and ignored, so the caller got a
    /// 202 and a production row. Now the query string is refused, and the body
    /// is the only place those fields are read.
    #[tokio::test]
    async fn wiki_build_refuses_query_parameters_it_does_not_read() {
        let (app, db, _root) = harness().await;
        let before = count(&db, "select count(*) from wiki_builds").await;
        let (st, _v, raw) = send_json(
            &app,
            "POST",
            "/api/v1/knowledge/wiki/build?dry_run=true",
            Some(serde_json::json!({})),
        )
        .await;
        println!("READING query form: HTTP {st} body={raw}");
        assert_eq!(st, StatusCode::BAD_REQUEST, "{raw}");
        assert!(raw.contains("dry_run"), "{raw}");
        let after = count(&db, "select count(*) from wiki_builds").await;
        println!("READING wiki_builds rows: before={before} after={after}");
        assert_eq!(
            before, after,
            "a refused request must not write a build row"
        );

        // The body is the way to say it; a scope that selects nothing is the
        // documented failure, not a silent empty build.
        let (st2, _v2, raw2) = send_json(
            &app,
            "POST",
            "/api/v1/knowledge/wiki/build",
            Some(serde_json::json!({ "dry_run": true, "scope": "all" })),
        )
        .await;
        println!("READING body form: HTTP {st2} body={raw2}");
        assert_eq!(st2, StatusCode::BAD_REQUEST, "{raw2}");
        assert!(
            raw2.contains("no source documents") || raw2.contains("selects no source"),
            "{raw2}"
        );
    }

    /// The correction loop's write side (R-D D.6): `POST` records, `GET` reads,
    /// and a refusal keeps its identity (unknown kind names the vocabulary; an
    /// empty reason/author is a 400 that says which field). Wiring is asserted,
    /// not assumed: a `pin` must reach the SEMANTICS (`active_correction`), not
    /// just the table, or the panel's freeze button would store a row that
    /// changes nothing.
    #[tokio::test]
    async fn wiki_corrections_records_reads_and_refuses_by_name() {
        let (app, db, _root) = harness().await;
        let path = "/api/v1/knowledge/wiki/pages/engine-notes/corrections";

        // Nothing recorded yet: an empty list is a reading, not a 404.
        let (st0, v0, raw0) = send_json(&app, "GET", path, None).await;
        println!("READING GET empty: HTTP {st0} body={raw0}");
        assert_eq!(st0, StatusCode::OK, "{raw0}");
        assert_eq!(v0["slug"], "engine-notes", "{raw0}");
        assert_eq!(v0["corrections"], serde_json::json!([]), "{raw0}");

        let (st, v, raw) = send_json(
            &app,
            "POST",
            path,
            Some(serde_json::json!({
                "kind": "pin",
                "reason": "hand-checked against the vendor page",
                "author": "operator",
            })),
        )
        .await;
        println!("READING POST pin: HTTP {st} body={raw}");
        assert_eq!(st, StatusCode::OK, "{raw}");
        assert!(v["id"].as_i64().unwrap_or(0) > 0, "{raw}");
        assert_eq!(v["correction"]["kind"], "pin", "{raw}");
        assert_eq!(v["correction"]["author"], "operator", "{raw}");

        // ONE vocabulary: the value this endpoint ECHOES must be a value it
        // accepts. Before the fix the enum serialised as `Pin` while `parse()`
        // only knew `pin`, so a client echoing the reading back got a 400.
        let echoed = v["correction"]["kind"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let (echo_s, _echo_v, echo_raw) = send_json(
            &app,
            "POST",
            path,
            Some(serde_json::json!({
                "kind": echoed,
                "reason": "round-trip of the value the endpoint just returned",
                "author": "operator",
            })),
        )
        .await;
        println!("READING round-trip: echoed={echoed} HTTP {echo_s}");
        assert_eq!(echo_s, StatusCode::OK, "{echo_raw}");

        let (st1, v1, raw1) = send_json(&app, "GET", path, None).await;
        assert_eq!(st1, StatusCode::OK, "{raw1}");
        let rows1 = v1["corrections"].as_array().cloned().unwrap_or_default();
        let kinds1: Vec<&str> = rows1
            .iter()
            .map(|c| c["kind"].as_str().unwrap_or(""))
            .collect();
        println!("READING GET after pin+round-trip: kinds={kinds1:?} body={raw1}");
        // Order-independent on purpose: `at DESC, id DESC` is the storage order,
        // not part of this endpoint's contract, and a test that reads index 0 is
        // testing the clock. BOTH rows are pins because the round-trip re-sent the
        // value the endpoint had just echoed — which is the point of that step.
        assert_eq!(
            kinds1,
            vec!["pin", "pin"],
            "the pin and the round-tripped pin: {raw1}"
        );
        let reasons: Vec<&str> = rows1
            .iter()
            .map(|c| c["reason"].as_str().unwrap_or(""))
            .collect();
        assert!(
            reasons.contains(&v["correction"]["reason"].as_str().unwrap_or("")),
            "the reason travels with its row: {raw1}"
        );
        assert!(
            reasons.contains(&"round-trip of the value the endpoint just returned"),
            "{raw1}"
        );

        // The pin reaches the freeze semantics the build reads.
        let active = crate::wiki::active_correction(&db, "engine-notes").await;
        println!("READING active_correction after pin: {active:?}");
        assert_eq!(
            active.map(|c| c.kind.as_str()),
            Some("pin"),
            "a pin must be the active correction the build consults"
        );

        // `release` unfreezes; the history is kept (nothing is deleted).
        let (st2, _v2, raw2) = send_json(
            &app,
            "POST",
            path,
            Some(serde_json::json!({
                "kind": "release",
                "reason": "source rewritten upstream",
                "author": "operator",
            })),
        )
        .await;
        assert_eq!(st2, StatusCode::OK, "{raw2}");
        assert!(
            crate::wiki::active_correction(&db, "engine-notes")
                .await
                .is_none(),
            "a release must clear the freeze"
        );
        let (st3, v3, raw3) = send_json(&app, "GET", path, None).await;
        assert_eq!(st3, StatusCode::OK, "{raw3}");
        assert_eq!(
            v3["corrections"].as_array().map(|a| a.len()),
            Some(3),
            "history is kept: pin + note + release: {raw3}"
        );

        // Refusals, each naming what is wrong.
        let (bad_kind_s, _bk, bad_kind) = send_json(
            &app,
            "POST",
            path,
            Some(serde_json::json!({"kind": "freeze", "reason": "x", "author": "a"})),
        )
        .await;
        println!("READING POST unknown kind: HTTP {bad_kind_s} body={bad_kind}");
        assert_eq!(bad_kind_s, StatusCode::BAD_REQUEST, "{bad_kind}");
        assert!(
            bad_kind.contains("pin") && bad_kind.contains("note"),
            "{bad_kind}"
        );

        let (bad_reason_s, _br, bad_reason) = send_json(
            &app,
            "POST",
            path,
            Some(serde_json::json!({"kind": "note", "reason": "  ", "author": "a"})),
        )
        .await;
        assert_eq!(bad_reason_s, StatusCode::BAD_REQUEST, "{bad_reason}");
        assert!(bad_reason.contains("reason"), "{bad_reason}");

        let (bad_author_s, _ba, bad_author) = send_json(
            &app,
            "POST",
            path,
            Some(serde_json::json!({"kind": "note", "reason": "why", "author": ""})),
        )
        .await;
        assert_eq!(bad_author_s, StatusCode::BAD_REQUEST, "{bad_author}");
        assert!(bad_author.contains("author"), "{bad_author}");

        // Three refusals wrote nothing: the table still holds exactly the two
        // rows we recorded on purpose.
        let rows = count(&db, "select count(*) from wiki_corrections").await;
        println!("READING wiki_corrections rows after refusals: {rows}");
        assert_eq!(rows, 3, "a refused correction must not be stored");
    }

    /// t320: the panel's entity search must not answer "nothing" for an entity
    /// that is there, and must not pass a widened guess off as a hit.
    ///
    /// Object set: one entity whose NAME is `AutoHotkey` and whose SUMMARY says
    /// `v2.0.28` (the cross-column shape) and nothing else. Sampling surface:
    /// the real router over a throwaway root. Falsifiers: (a) `q=autohotkey-v2`
    /// returning nothing, or returning it unmarked; (b) an exact query being
    /// marked `candidate`; (c) `q=zzz-not-a-real-name` returning an UNMARKED
    /// entity.
    #[tokio::test]
    async fn entity_search_falls_back_across_columns_and_marks_the_fallback() {
        let (app, db, _root) = harness().await;
        // Name and summary copied from the live row (id 4) that t312 read: the
        // name carries `AutoHotkey`, the summary carries `v2` -- different
        // columns, which is exactly why the phrase pass cannot see them
        // together. (My first fixture wrote "AutoHotkey v2.0.28" in the summary
        // and the premise assertion FAILED: both tokens were adjacent in ONE
        // column, so the phrase matched. The shape has to be the real one.)
        let id = ruagent_graph::upsert_entity(
            &db,
            "AutoHotkey",
            Some("tool"),
            Some("Windows 键盘/鼠标热键自动化工具，v1 与 v2 语法不兼容；本机为 v2.0.28 解压版。"),
        )
        .await
        .unwrap();
        // A second entity that has NOTHING to do with the query but DOES carry
        // the token `name` -- the live store has one too, which is why
        // `zzz-not-a-real-name` came back with 1 loose hit (t312). Without it
        // the control would pass vacuously, by finding nothing at all.
        ruagent_graph::upsert_entity(
            &db,
            "SKILL.md",
            Some("doc"),
            Some("Agent Skills：每个 skill 有一个 name 字段和一段 description。"),
        )
        .await
        .unwrap();

        // BEFORE, on the same tree: the strict pass alone -- what this endpoint
        // used to call, and therefore what the panel used to get.
        let strict = ruagent_graph::search_entities(&db, "autohotkey-v2", 10)
            .await
            .unwrap();
        println!(
            "READING t320 strict(autohotkey-v2) = {} hit(s)  [before: the panel answered nothing]",
            strict.len()
        );
        assert!(
            strict.is_empty(),
            "the strict phrase pass must miss across columns: {strict:?}"
        );

        // AFTER: the endpoint falls back, and says so.
        let (st, v, raw) = hit(&app, "GET", "/api/v1/graph/search?q=autohotkey-v2").await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        println!(
            "READING t320 search(autohotkey-v2) -> HTTP {} match={} entities={}  {raw}",
            st.as_u16(),
            v["match"],
            v["entities"].as_array().map(|a| a.len()).unwrap_or(0)
        );
        assert_eq!(v["match"], "candidate", "{v}");
        let ents = v["entities"].as_array().expect("entities array");
        assert!(
            ents.iter()
                .any(|e| e["id"] == id && e["name"] == "AutoHotkey"),
            "the fallback must reach the entity that was there: {v}"
        );

        // The fallback is not simply always on: an exact query stays exact.
        let (st, v, raw) = hit(&app, "GET", "/api/v1/graph/search?q=AutoHotkey").await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        assert_eq!(v["match"], "exact", "{v}");
        assert!(!v["entities"].as_array().unwrap().is_empty(), "{v}");

        // The control: a name that does not exist. Loose widens, so a hit is
        // allowed -- an UNMARKED hit is not.
        let (st, v, raw) = hit(&app, "GET", "/api/v1/graph/search?q=zzz-not-a-real-name").await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        let n = v["entities"].as_array().map(|a| a.len()).unwrap_or(0);
        println!(
            "READING t320 search(zzz-not-a-real-name) -> HTTP {} match={} entities={}  {raw}",
            st.as_u16(),
            v["match"],
            n
        );
        assert!(
            n > 0,
            "the control must actually widen (the `name` token is in the store), \
             otherwise this branch proves nothing: {v}"
        );
        assert_eq!(
            v["match"], "candidate",
            "a widened hit must never be presented as a hit: {v}"
        );
        assert!(
            v["entities"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["name"] != "zzz-not-a-real-name"),
            "and it is not an entity that exists: {v}"
        );
    }

    /// One integer out of one statement — the raw counts the readings quote.
    async fn count(db: &ruagent_store::Db, sql: &str) -> i64 {
        let sql = sql.to_string();
        db.call(move |conn| conn.query_row(&sql, [], |r| r.get::<_, i64>(0)))
            .await
            .unwrap()
            .unwrap()
    }

    /// t276: a hard entity delete takes its edges and facts with it (both
    /// directions), reports how many, and 404s on an id that is not there.
    /// Object set = one probe entity + one other entity + 2 facts; sampling
    /// surface = the real router over a throwaway root; falsifier = any edge
    /// row left behind, or a 200 for the second delete.
    #[tokio::test]
    async fn entity_delete_removes_edges_and_facts_and_404s_missing() {
        let (app, db, _root) = harness().await;
        let probe = ruagent_graph::upsert_entity(&db, "__probe__doctor-node", Some("tool"), None)
            .await
            .unwrap();
        let other = ruagent_graph::upsert_entity(&db, "t276-other", Some("tool"), None)
            .await
            .unwrap();
        ruagent_graph::add_fact(&db, probe, other, "touches", "probe fact", None, None)
            .await
            .unwrap();
        ruagent_graph::add_fact(&db, other, probe, "touched-by", "reverse fact", None, None)
            .await
            .unwrap();
        let before_edges = count(&db, "SELECT COUNT(*) FROM entity_edges").await;
        let before_probe_entities = count(
            &db,
            "SELECT COUNT(*) FROM entities WHERE name LIKE '__probe__%'",
        )
        .await;
        assert_eq!(before_edges, 2);
        assert_eq!(before_probe_entities, 1);
        let (st, v, raw) = hit(&app, "DELETE", &format!("/api/v1/graph/entity/{probe}")).await;
        println!(
            "READING entity delete: before entities(probe)={} edges={} | DELETE -> HTTP {} {}",
            before_probe_entities,
            before_edges,
            st.as_u16(),
            raw.trim()
        );
        assert_eq!(st, StatusCode::OK, "{raw}");
        assert_eq!(v["outcome"], "deleted", "{raw}");
        assert_eq!(v["id"], probe, "{raw}");
        assert_eq!(v["edges_removed"], 2, "{raw}");
        assert_eq!(v["facts_removed"], 2, "{raw}");
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM entities WHERE name LIKE '__probe__%'"
            )
            .await,
            0
        );
        assert_eq!(count(&db, "SELECT COUNT(*) FROM entity_edges").await, 0);
        let (lst, lv, lraw) = hit(&app, "GET", "/api/v1/graph/entities?limit=50").await;
        assert_eq!(lst, StatusCode::OK, "{lraw}");
        assert!(!lv.to_string().contains("__probe__doctor-node"), "{lv}");
        let (sst, sv, sraw) = hit(&app, "GET", "/api/v1/graph/search?q=__probe__doctor-node").await;
        assert_eq!(sst, StatusCode::OK, "{sraw}");
        assert!(!sv.to_string().contains("__probe__doctor-node"), "{sv}");
        let (st2, _, raw2) = hit(&app, "DELETE", &format!("/api/v1/graph/entity/{probe}")).await;
        println!(
            "READING entity negative: second DELETE -> HTTP {} {}",
            st2.as_u16(),
            raw2.trim()
        );
        println!(
            "READING entity after: entities(probe)={} edges={} list/search contain it = {} / {}",
            count(
                &db,
                "SELECT COUNT(*) FROM entities WHERE name LIKE '__probe__%'"
            )
            .await,
            count(&db, "SELECT COUNT(*) FROM entity_edges").await,
            lv.to_string().contains("__probe__doctor-node"),
            sv.to_string().contains("__probe__doctor-node")
        );
        assert_eq!(st2, StatusCode::NOT_FOUND, "{raw2}");
    }

    /// ruagent-close-the-gaps t22: the alias text is reachable, and only additively.
    ///
    /// WHAT THIS PINS, and what fails if the exposure is removed: the `aliases`
    /// assertion on the entity route and the opt-in assertion on the list route.
    /// Without them the folded name goes back to being a row in `entity_aliases`
    /// that no response contains — the state t18 measured, where a search for a term
    /// the graph knew answered `match="candidate"` with a list nobody could verify
    /// (for the entity used here the owning id IS among the candidates, and the
    /// reader still cannot tell, because the alias text is absent).
    #[tokio::test]
    async fn the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive() {
        let (app, db, root) = harness().await;
        let keeper = ruagent_graph::upsert_entity(&db, "AMBIGUOUS", Some("concept"), None)
            .await
            .unwrap();
        assert!(
            ruagent_graph::add_alias(&db, keeper, "AMBIGUOUS（模糊）", "knowledge")
                .await
                .unwrap()
        );

        // 1. the detail route: name/kind/aliases BESIDE the facts it already had.
        let (st, v, raw) = hit(&app, "GET", &format!("/api/v1/graph/entity/{keeper}")).await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        let aliases: Vec<&str> = v["aliases"]
            .as_array()
            .expect("aliases array")
            .iter()
            .filter_map(|a| a.as_str())
            .collect();
        let facts_len = v["facts"].as_array().map(Vec::len).unwrap_or(usize::MAX);
        println!(
            "READING t22 /graph/entity/{keeper} -> HTTP {} name={} kind={} aliases={aliases:?} facts={facts_len}",
            st.as_u16(),
            v["name"],
            v["kind"]
        );
        assert_eq!(v["name"], "AMBIGUOUS", "{raw}");
        assert_eq!(v["kind"], "concept", "{raw}");
        assert!(v["facts"].is_array(), "the old field is still there: {raw}");
        assert!(
            aliases.contains(&"AMBIGUOUS（模糊）"),
            "the merged name must be in the response: {raw}"
        );

        // 2. the list route: same rows, and the alias map only when asked for.
        let (st, plain, raw) = hit(&app, "GET", "/api/v1/graph/entities?limit=10").await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        assert!(
            plain.get("aliases").is_none(),
            "no flag, no field — the opt-in must stay opt-in: {raw}"
        );
        assert!(
            plain["entities"][0].is_array(),
            "rows keep the [entity, fact_count] shape a consumer already parses: {raw}"
        );
        let (st, opted, raw) =
            hit(&app, "GET", "/api/v1/graph/entities?limit=10&aliases=true").await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        let got = opted["aliases"]
            .as_object()
            .and_then(|m| m.get(&keeper.to_string()))
            .and_then(|a| a.as_array())
            .expect("the entity's aliases in the opt-in map");
        println!(
            "READING t22 /graph/entities?aliases=true -> HTTP {} field present={} aliases={:?}",
            st.as_u16(),
            opted.get("aliases").is_some(),
            got
        );
        assert!(
            got.iter().any(|a| a == "AMBIGUOUS（模糊）"),
            "the opt-in map carries the alias text: {raw}"
        );

        // 3. an id that is not in the graph keeps today's 200-with-empty answer.
        let (st, missing, raw) = hit(
            &app,
            "GET",
            &format!("/api/v1/graph/entity/{}", keeper + 999),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        assert!(missing["name"].is_null(), "{raw}");
        assert!(
            missing["facts"].as_array().is_some_and(Vec::is_empty),
            "{raw}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// t276: `?purge=true` really removes the row; the default stays the t251
    /// soft delete (row still there, second plain DELETE 409).
    #[tokio::test]
    async fn memory_purge_removes_the_row_and_keeps_the_soft_default() {
        let (app, db, _root) = harness().await;
        let outcome = ruagent_memory::write_memory(
            &db,
            &ruagent_memory::MemoryWrite {
                store: ruagent_memory::MemoryStore::Observation,
                namespace: ruagent_memory::Namespace::Agent("__probe__".to_string()),
                content: "t276 probe secret".to_string(),
                confidence: 1.0,
                source_episode: None,
                supersedes: None,
            },
        )
        .await
        .unwrap();
        let id = match outcome {
            ruagent_memory::WriteOutcome::Inserted(id) => id,
            other => panic!("expected Inserted, got {other:?}"),
        };
        let probe_rows = "SELECT COUNT(*) FROM memories WHERE namespace LIKE 'agent:__probe__%'";
        let before_rows = count(&db, probe_rows).await;
        assert_eq!(before_rows, 1);

        // Default semantics unchanged: soft delete, row still there.
        let (st, v, raw) = hit(&app, "DELETE", &format!("/api/v1/memory/{id}")).await;
        assert_eq!(st, StatusCode::OK, "{raw}");
        assert_eq!(v["outcome"], "deleted", "{raw}");
        assert_eq!(v["soft"], true, "{raw}");
        let rows_after_soft = count(&db, probe_rows).await;
        assert_eq!(rows_after_soft, 1, "soft delete keeps the row");
        // Second plain DELETE is 409, not a silent 200.
        let (st2, _, raw2) = hit(&app, "DELETE", &format!("/api/v1/memory/{id}")).await;
        assert_eq!(st2, StatusCode::CONFLICT, "{raw2}");

        // Purge removes the row (and with it the only copy of the secret).
        let (st3, v3, raw3) = hit(&app, "DELETE", &format!("/api/v1/memory/{id}?purge=true")).await;
        assert_eq!(st3, StatusCode::OK, "{raw3}");
        assert_eq!(v3["outcome"], "purged", "{raw3}");
        assert_eq!(v3["id"], id, "{raw3}");
        println!(
            "READING memory: before rows={} | soft DELETE -> HTTP {} {} | rows after soft={} | second plain DELETE -> HTTP {} | purge -> HTTP {} {} | rows after purge={}",
            before_rows,
            st.as_u16(),
            raw.trim(),
            rows_after_soft,
            st2.as_u16(),
            st3.as_u16(),
            raw3.trim(),
            count(&db, probe_rows).await
        );
        assert_eq!(count(&db, probe_rows).await, 0);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM memories WHERE content LIKE '%t276 probe secret%'"
            )
            .await,
            0
        );
        // Read paths: list, search and recall no longer see it.
        for uri in [
            "/api/v1/memory/list?store=observation&limit=50",
            "/api/v1/memory/search?q=t276%20probe",
            "/api/v1/recall?q=t276%20probe&top_n=5",
        ] {
            let (s, _, body) = hit(&app, "GET", uri).await;
            assert_eq!(s, StatusCode::OK, "{uri}: {body}");
            assert!(!body.contains("t276 probe secret"), "{uri}: {body}");
        }
        // Negative: purging an id that is gone is 404, not a silent 200.
        let (st4, _, raw4) = hit(&app, "DELETE", &format!("/api/v1/memory/{id}?purge=true")).await;
        assert_eq!(st4, StatusCode::NOT_FOUND, "{raw4}");
    }

    /// The whole lifecycle through the real router: the routes, the query
    /// parameter, the source guard and the index writes.
    #[tokio::test]
    async fn session_archive_delete_over_the_router() {
        let (app, db, root) = harness().await;
        seed(&db, "ruagent:aa", "ruagent").await;
        seed(&db, "claude-code:bb", "claude-code").await;

        // Default list: everything, nothing archived, per-row deletability.
        let (st, v, _) = hit(&app, "GET", "/api/v1/sessions").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["archived_count"], 0);
        assert_eq!(v["sessions"].as_array().unwrap().len(), 2);
        assert_eq!(find(&v, "ruagent:aa").unwrap()["deletable"], true);
        assert_eq!(find(&v, "claude-code:bb").unwrap()["deletable"], false);
        assert_eq!(find(&v, "ruagent:aa").unwrap()["archived"], false);

        // Archive another tool's session: allowed (it is only a hide).
        let (st, v, _) = hit(&app, "POST", "/api/v1/sessions/claude-code:bb/archive").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["archived"], true);

        let (_, v, _) = hit(&app, "GET", "/api/v1/sessions").await;
        assert!(find(&v, "claude-code:bb").is_none(), "hidden by default");
        assert_eq!(v["archived_count"], 1);
        let (_, v, _) = hit(&app, "GET", "/api/v1/sessions?archived=include").await;
        assert_eq!(find(&v, "claude-code:bb").unwrap()["archived"], true);
        let (_, v, _) = hit(&app, "GET", "/api/v1/sessions?archived=only").await;
        assert_eq!(v["sessions"].as_array().unwrap().len(), 1);
        assert_eq!(v["sessions"][0]["key"], "claude-code:bb");

        let (st, _, _) = hit(&app, "GET", "/api/v1/sessions?archived=nonsense").await;
        assert_eq!(st, StatusCode::BAD_REQUEST);

        // Unarchive: back in the default list, and idempotent.
        let (st, v, _) = hit(&app, "DELETE", "/api/v1/sessions/claude-code:bb/archive").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["changed"], true);
        let (_, v, _) = hit(&app, "GET", "/api/v1/sessions").await;
        assert!(find(&v, "claude-code:bb").is_some());
        assert_eq!(v["archived_count"], 0);
        let (st, v, _) = hit(&app, "DELETE", "/api/v1/sessions/claude-code:bb/archive").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["changed"], false);

        // Hiding an unknown key is refused rather than stored.
        let (st, _, _) = hit(&app, "POST", "/api/v1/sessions/ruagent:nope/archive").await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // Delete another tool's history: 403 naming the owner, row intact.
        let (st, _, body) = hit(&app, "DELETE", "/api/v1/sessions/claude-code:bb").await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert!(body.contains("claude-code"), "body was {body}");
        let (_, v, _) = hit(&app, "GET", "/api/v1/sessions").await;
        assert!(find(&v, "claude-code:bb").is_some());

        // Delete our own: gone, and gone means gone.
        let (st, _, _) = hit(&app, "DELETE", "/api/v1/sessions/ruagent:aa").await;
        assert_eq!(st, StatusCode::OK);
        let (_, v, _) = hit(&app, "GET", "/api/v1/sessions").await;
        assert!(find(&v, "ruagent:aa").is_none());
        let (st, _, _) = hit(&app, "DELETE", "/api/v1/sessions/ruagent:aa").await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // The indexer re-inserts every session whose file is still on disk, so
        // simulate that 60s rescan: the tombstone must keep it gone in every
        // mode, and it must not reappear as an "archived" row either.
        seed(&db, "ruagent:aa", "ruagent").await;
        for mode in ["exclude", "include", "only"] {
            let uri = format!("/api/v1/sessions?archived={mode}");
            let (st, v, _) = hit(&app, "GET", &uri).await;
            assert_eq!(st, StatusCode::OK);
            assert!(
                find(&v, "ruagent:aa").is_none(),
                "resurrected by a re-index in mode {mode}"
            );
            assert_eq!(v["archived_count"], 0);
        }
        assert_eq!(db.deleted_session_keys().await.unwrap(), vec!["ruagent:aa"]);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Deleting a row must not leave a hide marker behind: a later session
    /// that reuses the key must not be born invisible.
    #[tokio::test]
    async fn deleting_a_hidden_session_clears_its_marker() {
        let (app, db, root) = harness().await;
        seed(&db, "ruagent:cc", "ruagent").await;
        let (st, _, _) = hit(&app, "POST", "/api/v1/sessions/ruagent:cc/archive").await;
        assert_eq!(st, StatusCode::OK);
        let (st, _, _) = hit(&app, "DELETE", "/api/v1/sessions/ruagent:cc").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(
            db.archived_session_keys().await.unwrap(),
            Vec::<String>::new()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `health` must NOT answer ok while the data plane is dead
    /// (ruagent-close-the-gaps t16).
    ///
    /// WHAT THIS WOULD HAVE CAUGHT. Before this task, `health` was
    /// `Json(json!({"status":"ok","service":"ruagent"}))` — a constant that never
    /// touched the store. Measured on a private daemon: after ONE store-backed
    /// operation panicked (which used to unwind the single-writer thread), the
    /// route set was `health` 200 ok / `/api/v1/sessions` 500 / `/api/v1/tasks`
    /// 500 / `/api/v1/stats` 500 / `/api/v1/memory/list` 500, all with body
    /// `database writer is shut down`, and `daemon.out` gained nothing. A
    /// supervisor watching `health` saw a healthy daemon for ever. The positive
    /// half of this test pins the CONTRACT (a live actor MUST report ok); the
    /// negative half pins the DEFECT directly and would have failed loudly on the
    /// old body, which was literally `{"status":"ok"}` with no `db` field at all.
    #[tokio::test]
    async fn health_reports_the_state_of_the_data_plane_not_a_constant() {
        let (app, db, root) = harness().await;

        let (status, body, _raw) = hit(&app, "GET", "/api/v1/health").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a daemon whose writer is running must be ok"
        );
        assert_eq!(body["status"], "ok");
        assert_eq!(
            body["db"]["writer"], "running",
            "the caller must be able to see WHAT was checked, not just the verdict"
        );
        assert_eq!(
            body["db"]["writer_panics"], 0,
            "a fresh daemon has caught no panics"
        );

        // THE DEFECT, PINNED: the old handler answered exactly `{"status":"ok"}`
        // for a store that could not serve. That document must not be what a
        // caller gets any more.
        assert_ne!(
            body,
            serde_json::json!({ "status": "ok", "service": "ruagent" }),
            "health must no longer be the constant it was: the old body answered ok while every \
             store-backed route failed, which is worse than a crash because a crash is visible"
        );

        // A panic inside a store-backed closure is ANNOUNCED by health while the
        // actor survives -- the reading that used to be lost entirely.
        let _ = db
            .call(|_conn| -> i64 { panic!("t16: route-level panic") })
            .await;
        let (status, body, _raw) = hit(&app, "GET", "/api/v1/health").await;
        assert_eq!(status, StatusCode::OK, "the actor survived by design");
        assert_eq!(
            body["db"]["writer_panics"], 1,
            "a panic the actor survived must still be visible to a health check"
        );
        let last = body["db"]["last_panic"].as_str().unwrap_or_default();
        assert!(
            last.contains("t16: route-level panic"),
            "health must carry the panic text, got {last:?}"
        );

        // And the route set still answers, which is the whole point of surviving.
        let (st, _h, _b) = hit(&app, "GET", "/api/v1/sessions").await;
        assert_eq!(
            st,
            StatusCode::OK,
            "store-backed routes must keep working after a caught panic"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
