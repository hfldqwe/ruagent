//! HTTP API (`/api/v1`) + SSE event streaming.

use std::convert::Infallible;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
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
    pub chats: std::sync::Arc<crate::chat::ChatManager>,
    pub sessions: std::sync::Arc<crate::sessions::SessionIndexer>,
}

/// Build the API router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/agents/{name}/options", get(agent_options))
        .route("/api/v1/chat/{id}/options", post(chat_option))
        .route("/api/v1/stats", get(stats))
        .route("/api/v1/mcp", get(mcp_registry))
        .route("/api/v1/skills", get(list_skills))
        .route("/api/v1/skills/sync", post(sync_skills))
        .route("/api/v1/graph/entities", get(graph_entities))
        .route("/api/v1/graph/search", get(graph_search))
        .route("/api/v1/graph/entity", post(graph_create_entity))
        .route("/api/v1/graph/fact", post(graph_add_fact))
        .route("/api/v1/graph/entity/{id}", get(graph_entity))
        .route("/api/v1/graph/entity/{id}/neighbors", get(graph_neighbors))
        .route("/api/v1/graph/entity/{id}/facts", get(graph_facts))
        .route("/api/v1/memory/write", post(memory_write))
        .route("/api/v1/memory/supersede", post(memory_supersede))
        .route("/api/v1/memory/diffs", get(memory_diffs))
        .route("/api/v1/memory/{id}", get(memory_get))
        .route("/api/v1/memory/search", get(memory_search))
        .route("/api/v1/memory/list", get(memory_list))
        .route("/api/v1/knowledge/ingest", post(knowledge_ingest))
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
        .route("/api/v1/tasks", post(create_task).get(list_tasks))
        .route("/api/v1/tasks/{id}", get(get_task))
        .route(
            "/api/v1/tasks/{id}",
            axum::routing::patch(update_task).delete(delete_task),
        )
        .route("/api/v1/tasks/{id}/runs", post(start_run))
        .route("/api/v1/tasks/{id}/fanout", post(start_fanout))
        .route("/api/v1/tasks/{id}/pipeline", post(start_pipeline))
        .route("/api/v1/runs/{id}", get(get_run))
        .route("/api/v1/runs/{id}/select", post(select_run))
        .route("/api/v1/runs/{id}/cancel", post(cancel_run))
        .route("/api/v1/runs/{id}/events", get(run_events))
        .route("/api/v1/chat", post(chat_start).get(chat_list))
        .route("/api/v1/chat/{id}/messages", post(chat_message))
        .route("/api/v1/chat/{id}/events", get(chat_events))
        .route(
            "/api/v1/chat/{id}",
            axum::routing::patch(chat_model).delete(chat_close),
        )
        .route("/api/v1/sessions", get(sessions_list))
        .route("/api/v1/sessions/{key}", get(sessions_messages))
        .route("/api/v1/sessions/{key}/distill", post(session_distill))
        .route("/api/v1/recall", get(recall))
        .route(
            "/api/v1/memory/backfill-embeddings",
            post(memory_backfill_embeddings),
        )
        .route("/api/v1/permissions", get(list_permissions))
        .route("/api/v1/permissions/{key}", post(resolve_permission))
        .with_state(state)
        .fallback_service(panel_service())
}

/// Serve the built web panel (SPA) when `panel/dist` exists; the API
/// works fine without it. Override with `RUAGENT_PANEL_DIST`.
///
/// Cache policy: hashed `/assets/*` are immutable; the HTML shell is
/// `no-cache` so a rebuild always lands (a stale index.html referencing
/// a dead bundle renders a blank page).
fn panel_service() -> CacheDir {
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

async fn graph_entities(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entities = ruagent_graph::list_entities(state.mgr.db(), q.limit.unwrap_or(50)).await?;
    Ok(Json(serde_json::json!({ "entities": entities })))
}

async fn graph_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let hits = ruagent_graph::search_entities(state.mgr.db(), &q.q, 10).await?;
    Ok(Json(serde_json::json!({ "entities": hits })))
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

/// Compile source documents into wiki pages. `dry_run` plans and
/// returns the page set for review (the plan-level human gate);
/// `confirm_plan` executes a reviewed plan.
async fn wiki_build(
    State(state): State<AppState>,
    Json(req): Json<WikiBuildRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let scope = parse_wiki_scope(req.scope)?;
    let distiller = crate::distill::Distiller {
        db: state.mgr.db().clone(),
        root: state.config.root.clone(),
        embedder: None,
        registry: state.mgr.registry_view(),
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

async fn graph_entity(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: i64 = id
        .parse()
        .map_err(|_| ApiError::bad_request("invalid entity id"))?;
    let facts = ruagent_graph::current_facts(state.mgr.db(), id).await?;
    Ok(Json(serde_json::json!({ "facts": facts })))
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
    let facts = match &q.at {
        Some(at) => ruagent_graph::facts_as_of(state.mgr.db(), id, at).await?,
        None => ruagent_graph::current_facts(state.mgr.db(), id).await?,
    };
    Ok(Json(serde_json::json!({ "facts": facts })))
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
                "models": a.models,
                "reasoning_effort": a.reasoning_effort,
                "description": a.description,
                "model": a.model,
                "enabled": a.enabled,
                "runtime": a.runtime,
                "runtimes": a.runtimes,
                "prompt": a.prompt.as_deref().map(|p| p.chars().take(160).collect::<String>()),
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

// ---------------------------------------------------------------------------
// Session history (auto-synced from claude-code / dsh / ruagent stores)
// ---------------------------------------------------------------------------

async fn sessions_list(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sessions = state
        .sessions
        .list(q.limit.unwrap_or(200))
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    Ok(Json(serde_json::json!({ "sessions": sessions })))
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
async fn memory_backfill_embeddings(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let db = state.mgr.db().clone();
    let rows: Vec<(i64, String)> = db
        .call(|conn| -> Result<_, ruagent_store::DbError> {
            let mut stmt = conn
                .prepare("SELECT id, content FROM memories WHERE embedding IS NULL")
                .map_err(ruagent_store::DbError::from)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(ruagent_store::DbError::from)?;
            Ok(rows.filter_map(|r| r.ok()).collect())
        })
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let total = rows.len();
    let embedder = state.knowledge.embedder();
    for (id, content) in rows {
        crate::memembed::embed_row(&db, embedder.clone(), id, &content).await;
    }
    Ok(Json(serde_json::json!({ "embedded": total })))
}

// ---------------------------------------------------------------------------
// Distillation: session → memories + graph (agent-run extraction)
// ---------------------------------------------------------------------------

async fn session_distill(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Any enabled agent can serve as the distiller; prefer dsh (cheap,
    // local) then the first enabled.
    let agents = state.mgr.agents();
    let card = agents
        .iter()
        .find(|a| a.enabled && a.name == "dsh")
        .or_else(|| agents.iter().find(|a| a.enabled))
        .ok_or_else(|| ApiError::bad_request("no enabled agent to distill with"))?;
    let distiller = crate::distill::Distiller {
        db: state.mgr.db().clone(),
        root: state.config.root.clone(),
        embedder: Some(state.knowledge.embedder()),
        registry: state.mgr.registry_view(),
    };
    let out = distiller
        .distill(&key, card)
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
}

async fn recall(
    State(state): State<AppState>,
    Query(q): Query<RecallQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let conservative = q.strategy.as_deref() == Some("conservative");
    let top_n = q.top_n.unwrap_or(5).clamp(1, 20);

    // Memories — semantic leg first (same embedder as the knowledge
    // base), FTS as the keyword fallback.
    let semantic = crate::memembed::semantic_search(
        state.mgr.db(),
        state.knowledge.embedder(),
        &q.q,
        top_n,
        if conservative { 0.30 } else { 0.25 },
    )
    .await;
    let query_text = q.q.clone();
    let memories: Vec<(i64, String, String, String)> = state
        .mgr
        .db()
        .call(
            move |conn| -> Result<Vec<(i64, String, String, String)>, ruagent_store::DbError> {
                let pattern = format!("\"{}\"", query_text.replace('"', "\"\""));
                let mut stmt = conn
                    .prepare(
                        "SELECT m.id, m.store, m.namespace, m.content
                       FROM memories_fts f JOIN memories m ON m.id = f.rowid
                      WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL
                      ORDER BY rank LIMIT ?2",
                    )
                    .map_err(ruagent_store::DbError::from)?;
                let rows = stmt
                    .query_map(rusqlite::params![pattern, top_n], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                    })
                    .map_err(ruagent_store::DbError::from)?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            },
        )
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;

    // Knowledge chunks (hybrid semantic + keyword). Parent-child
    // retrieval (WeKnora): the hit is the precise unit, the aggressive
    // strategy returns the parent SECTION for complete context.
    let hits = state
        .knowledge
        .search(&q.q, top_n)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let parents = state
        .knowledge
        .parents_for(&hits.iter().map(|h| h.chunk_id).collect::<Vec<_>>())
        .await;

    // Graph entities (name/summary match) + their currently-valid facts
    // (the relations half of the conservative strategy's stubs).
    let entities = ruagent_graph::search_entities(state.mgr.db(), &q.q, top_n)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e}")))?;
    let mut entity_facts: std::collections::HashMap<i64, Vec<(String, String, String)>> =
        std::collections::HashMap::new();
    {
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
    let mut seen_ids: std::collections::HashSet<i64> = semantic.iter().map(|m| m.0).collect();
    let mut out_memories: Vec<serde_json::Value> = Vec::new();
    for (id, store, ns, content, score) in &semantic {
        out_memories.push(if conservative {
            serde_json::json!({
                "kind": "memory", "id": id, "store": store, "namespace": ns,
                "title": truncate_chars(content, 60),
                "score": (score * 100.0).round() / 100.0,
                "hint": "call memory_get(id) for full content",
            })
        } else {
            serde_json::json!({
                "kind": "memory", "id": id, "store": store, "namespace": ns,
                "content": content, "score": (score * 100.0).round() / 100.0,
            })
        });
    }
    for (id, store, ns, content) in memories {
        if seen_ids.contains(&id) {
            continue;
        }
        seen_ids.insert(id);
        out_memories.push(if conservative {
            serde_json::json!({
                "kind": "memory", "id": id, "store": store, "namespace": ns,
                "title": truncate_chars(&content, 60),
                "hint": "call memory_get(id) for full content",
            })
        } else {
            serde_json::json!({
                "kind": "memory", "id": id, "store": store, "namespace": ns,
                "content": content,
            })
        });
    }
    let mut out_chunks: Vec<serde_json::Value> = Vec::new();
    for h in hits {
        if (h.score as f64) < min_score as f64 {
            continue;
        }
        out_chunks.push(if conservative {
            serde_json::json!({
                "kind": "knowledge", "chunk_id": h.chunk_id, "document": h.document,
                "excerpt": truncate_chars(&h.content, 80),
                "hint": "call knowledge_expand(chunk_id) for the full section",
            })
        } else {
            serde_json::json!({
                "kind": "knowledge", "chunk_id": h.chunk_id, "document": h.document,
                // The parent section (capped): the hit's full context.
                "content": parents
                    .get(&h.chunk_id)
                    .map(|p| truncate_chars(p, PARENT_CONTEXT_CAP))
                    .unwrap_or_else(|| h.content.clone()),
                "excerpt": truncate_chars(&h.content, 160),
                "score": h.score,
            })
        });
    }
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
        let related_memories: Vec<serde_json::Value> = semantic
            .iter()
            .chain(fts_related(&state, &e.name).await.iter())
            .filter(|(_, _, _, c, _)| c.contains(&e.name))
            .take(2)
            .map(|(id, store, ns, content, _)| {
                serde_json::json!({
                    "id": id, "store": store, "namespace": ns,
                    "title": truncate_chars(content, 60),
                })
            })
            .collect();
        let related = if related_chunks.is_empty() && related_memories.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::json!({ "chunks": related_chunks, "memories": related_memories })
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

    Ok(Json(serde_json::json!({
        "strategy": if conservative { "conservative" } else { "aggressive" },
        "memories": out_memories,
        "knowledge": out_chunks,
        "entities": out_entities,
    })))
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
// Chat: terminal-like persistent sessions (M5)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChatStartRequest {
    agent: String,
    #[serde(default)]
    model: Option<String>,
}

async fn chat_start(
    State(state): State<AppState>,
    Json(req): Json<ChatStartRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let card = state
        .mgr
        .agent(&req.agent)
        .ok_or_else(|| ApiError::bad_request(format!("unknown agent `{}`", req.agent)))?;
    let chat = state
        .chats
        .start(card, req.model)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(serde_json::json!({
        "id": chat.id.to_string(),
        "agent": chat.agent,
        "model": chat.model,
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

/// Chat SSE: replay the chat transcript, then tail live events (single
/// event per line, no seq). `StateChanged{completed}` = end of chat.
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
    let replay = ruagent_store::read_transcript(&path).unwrap_or_default();

    let mut live = chat.subscribe();
    let (tx, rx_stream) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        for line in replay {
            let _ = tx.send(sse_data(&line));
        }
        loop {
            match live.recv().await {
                Ok(event) => {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    let _ = tx.send(Ok(Event::default().data(json)));
                    if let ruagent_core::RunEvent::StateChanged {
                        status: ruagent_core::RunStatus::Completed,
                    } = &event
                    {
                        let _ = tx.send(sse_end(ruagent_core::RunStatus::Completed));
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => {
                    let _ = tx.send(sse_end(ruagent_core::RunStatus::Completed));
                    return;
                }
            }
        }
    });

    Ok(Sse::new(UnboundedReceiverStream::new(rx_stream)).keep_alive(KeepAlive::default()))
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
    let card = state
        .mgr
        .agent(&chat.agent)
        .ok_or_else(|| ApiError::bad_request("agent vanished"))?;
    // Runtime switch: spawn the same agent (same role prompt) on a
    // different engine. The engine card is the registry entry named by
    // the runtime; the role prompt travels along.
    let (new_chat, restarted) = if let Some(runtime) = &req.runtime {
        let engine = state
            .mgr
            .agents()
            .into_iter()
            .find(|a| &a.name == runtime)
            .ok_or_else(|| ApiError::bad_request(format!("unknown runtime `{runtime}`")))?;
        let mut role = engine.clone();
        // carry the role prompt + agent identity
        if let Some(p) = &card.prompt {
            role.prompt = Some(p.clone());
        }
        state
            .chats
            .switch_model(id, req.model.clone(), &role)
            .await
            .map_err(|e| ApiError::bad_request(format!("{e:#}")))?
    } else {
        state
            .chats
            .switch_model(id, req.model, card)
            .await
            .map_err(|e| ApiError::bad_request(format!("{e:#}")))?
    };
    Ok(Json(serde_json::json!({
        "id": new_chat.id.to_string(),
        "agent": new_chat.agent,
        "model": new_chat.model,
        // "live" = same session, context preserved; "restarted" = new one.
        "switched": if restarted { "restarted" } else { "live" },
    })))
}

/// The session options an agent advertises (model, reasoning effort,
/// permission mode, …): cached, read from a live chat, or probed with a
/// throwaway session.
async fn agent_options(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let card = state
        .mgr
        .agent(&name)
        .filter(|a| a.enabled)
        .ok_or_else(|| ApiError::not_found("agent not found"))?;
    let options = state
        .chats
        .agent_options(card)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(Json(
        serde_json::json!({ "agent": name, "options": options }),
    ))
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
