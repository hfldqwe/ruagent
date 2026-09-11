# M3 #21 wiring — run in repo root with: python .patch_m3_daemon.py
import re

# ---- 1. daemon deps ----
p = 'crates/daemon/Cargo.toml'
s = open(p, encoding='utf-8').read()
if 'ruagent-knowledge' not in s:
    s = s.replace('ruagent-orchestrator.workspace = true',
                  'ruagent-orchestrator.workspace = true\nruagent-memory.workspace = true\nruagent-knowledge.workspace = true')
if 'ruagent-graph' not in s:
    s = s.replace('ruagent-knowledge.workspace = true',
                  'ruagent-knowledge.workspace = true\nruagent-graph.workspace = true')
open(p, 'w', encoding='utf-8').write(s)
print("daemon deps set")

# ---- 2. AppState gains Knowledge ----
p = 'crates/daemon/src/api.rs'
s = open(p, encoding='utf-8').read()
s = s.replace('''#[derive(Clone)]
pub struct AppState {
    pub mgr: std::sync::Arc<RunManager>,
    pub config: std::sync::Arc<DaemonConfig>,
}''',
'''#[derive(Clone)]
pub struct AppState {
    pub mgr: std::sync::Arc<RunManager>,
    pub config: std::sync::Arc<DaemonConfig>,
    pub knowledge: std::sync::Arc<ruagent_knowledge::Knowledge>,
}''')

# routes
s = s.replace('        .route("/api/v1/mcp", get(mcp_registry))',
'''        .route("/api/v1/mcp", get(mcp_registry))
        .route("/api/v1/memory/write", post(memory_write))
        .route("/api/v1/memory/search", get(memory_search))
        .route("/api/v1/memory/list", get(memory_list))
        .route("/api/v1/knowledge/ingest", post(knowledge_ingest))
        .route("/api/v1/knowledge/search", get(knowledge_search))''')

# handlers — append before the helpers section
anchor = '''/// Resolve the routing file's agent NAMES into ids and run the cascade.'''
handlers = '''// ---------------------------------------------------------------------------
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
    Ok(Json(serde_json::json!({ "outcome": format!("{outcome:?}") })))
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
    let hits = ruagent_memory::query::search_fts(state.mgr.db(), &q.q, q.limit.unwrap_or(8)).await?;
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
    let hits =
        ruagent_memory::query::current_memories(state.mgr.db(), store, &namespace, q.limit.unwrap_or(50))
            .await?;
    let counts = ruagent_memory::query::store_counts(state.mgr.db()).await?;
    Ok(Json(serde_json::json!({ "memories": hits, "counts": counts })))
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

''' + anchor
assert anchor in s, "helper anchor"
s = s.replace(anchor, handlers)
open(p, 'w', encoding='utf-8').write(s)
print("api endpoints added")

# ---- 3. serve(): construct Knowledge ----
p = 'crates/daemon/src/lib.rs'
s = open(p, encoding='utf-8').read()
s = s.replace('''    let state = api::AppState {
        mgr,
        config: Arc::new(config),
    };''',
'''    // Knowledge base: hash embedder by default (offline boot); set
    // RUAGENT_EMBEDDER=fastembed for real semantics (model downloads on
    // first use — the daemon never blocks on it by default).
    let knowledge = match std::env::var("RUAGENT_EMBEDDER").as_deref() {
        Ok("fastembed") => {
            match ruagent_knowledge::FastEmbedder::try_new().await {
                Ok(fe) => ruagent_knowledge::Knowledge::with_embedder(
                    &root,
                    db.clone(),
                    std::sync::Arc::new(fe),
                )
                .await,
                Err(e) => {
                    tracing::warn!(error = %e, "fastembed unavailable; falling back to hash embedder");
                    ruagent_knowledge::Knowledge::open(&root, db.clone()).await
                }
            }
        }
        _ => ruagent_knowledge::Knowledge::open(&root, db.clone()).await,
    }
    .with_context(|| "opening knowledge base")?;

    let state = api::AppState {
        mgr,
        config: Arc::new(config),
        knowledge: Arc::new(knowledge),
    };''')
open(p, 'w', encoding='utf-8').write(s)
print("serve constructs Knowledge")
