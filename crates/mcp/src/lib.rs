//! ruagent-mcp: the platform's own MCP server (design §6.5).
//!
//! A thin stdio-to-HTTP bridge: agents spawn `ruagent mcp-serve` (it is
//! registered in mcp.toml and injected into every session); the tools
//! call the daemon's local REST API. This is the ONLY seam external
//! harnesses need to reach the central memory, knowledge, and task
//! state — no SDK adapters (the Agno lesson).

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};

pub mod health;

/// Configuration for the bridge.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Daemon base URL (e.g. http://127.0.0.1:8787).
    pub daemon_url: String,
}

impl BridgeConfig {
    pub fn from_env() -> Self {
        Self {
            daemon_url: std::env::var("RUAGENT_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8787".into()),
        }
    }
}

/// The platform tool set.
#[derive(Debug, Clone)]
pub struct PlatformTools {
    config: BridgeConfig,
    http: reqwest::Client,
}

impl PlatformTools {
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }
}

#[tool_router]
impl PlatformTools {
    // -- memory ---------------------------------------------------------

    #[tool(
        description = "Search the user's long-term memories (facts, preferences, lessons learned across sessions). Use this before asking the user something they may have already told you."
    )]
    async fn memory_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/search", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[("q", &params.query)])
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(hits_to_text(&resp["hits"]))
    }

    #[tool(
        description = "Store a long-term memory about the user, a project, or a lesson learned. Stores: profile (user facts), observation (general), procedure (how-to, project/global), lesson (project/global). Namespaces: user, global, project:<name>, agent:<name>. Optionally pass supersedes=<memory id> to replace it."
    )]
    async fn memory_write(
        &self,
        Parameters(params): Parameters<WriteParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/write", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "store": params.store,
                "namespace": params.namespace,
                "content": params.content,
                "supersedes": params.supersedes,
            }))
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(format!(
            "memory write: {}",
            resp["outcome"].as_str().unwrap_or("?")
        ))
    }

    #[tool(
        description = "Unified recall across long-term memories, knowledge base, generated wiki pages and the knowledge graph. Two strategies: aggressive (default) returns full content above a relevance threshold — use when you need ready-to-use context; conservative returns only stubs (titles, entity names, relation one-liners) — cheap to scan, then fetch what you actually need via memory_get / graph_entity. Prefer conservative when context budget matters. The `wiki` section holds AGENT-GENERATED pages (stale flag marks outdated ones) — treat as leads to verify against sources, not ground truth."
    )]
    async fn memory_recall(
        &self,
        Parameters(params): Parameters<RecallParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/recall", self.config.daemon_url);
        let strategy = if params.conservative.unwrap_or(false) {
            "conservative".to_string()
        } else {
            "aggressive".to_string()
        };
        let top_n = params.top_n.unwrap_or(5).to_string();
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[
                ("q", &params.query),
                ("strategy", &strategy),
                ("top_n", &top_n),
            ])
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(recall_to_text(&resp))
    }

    #[tool(
        description = "Fetch one memory's full content by id (the follow-up call for memory_recall's conservative strategy)."
    )]
    async fn memory_get(
        &self,
        Parameters(params): Parameters<GetParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/{}", self.config.daemon_url, params.id);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        let m = &resp["memory"];
        Ok(format!(
            "[{}/{}] {}",
            m["store"].as_str().unwrap_or("?"),
            m["namespace"].as_str().unwrap_or("?"),
            m["content"].as_str().unwrap_or("?")
        ))
    }

    // -- knowledge ------------------------------------------------------

    #[tool(
        description = "Search the knowledge base of ingested documents (hybrid semantic + keyword search)."
    )]
    async fn knowledge_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/search", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[("q", &params.query)])
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(hits_to_text(&resp["hits"]))
    }

    #[tool(
        description = "Ingest a document (markdown, notes, code) into the knowledge base so it becomes searchable."
    )]
    async fn knowledge_ingest(
        &self,
        Parameters(params): Parameters<IngestParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/ingest", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "name": params.name,
                "content": params.content,
            }))
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(format!(
            "ingested {} chunks",
            resp["chunks"].as_i64().unwrap_or(0)
        ))
    }

    #[tool(
        description = "Expand a knowledge-base hit into its full parent section: the complete context (heading + surrounding paragraphs) around the chunk. Pass the chunk_id from memory_recall or knowledge_search results."
    )]
    async fn knowledge_expand(
        &self,
        Parameters(params): Parameters<ChunkParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!(
            "{}/api/v1/knowledge/expand/{}",
            self.config.daemon_url, params.chunk_id
        );
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(format!(
            "[{}] {}\n\n--- hit chunk ---\n{}",
            resp["document"].as_str().unwrap_or("?"),
            resp["section"].as_str().unwrap_or("?"),
            resp["chunk"].as_str().unwrap_or("?"),
        ))
    }

    #[tool(
        description = "Fetch one knowledge-graph entity by id: its currently-valid facts (relations to other entities). The follow-up call for memory_recall's entity stubs."
    )]
    async fn graph_entity(
        &self,
        Parameters(params): Parameters<EntityParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!(
            "{}/api/v1/graph/entity/{}",
            self.config.daemon_url, params.id
        );
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        // t93 / audit M1 (second half): `resp["facts"].as_array().cloned()
        // .unwrap_or_default()` was a protocol-drift absorber -- rename the field
        // and EVERY entity reports "has no current facts", with no test to catch
        // it. A missing or mistyped field is an error, not an empty result.
        //
        // M1 (first half): "this entity does not exist" must not read the same as
        // "this entity exists with no current facts". The daemon's
        // `GET /graph/entity/{id}` (api.rs:1207-1216) returns `{"facts": []}` for
        // BOTH today -- it carries no existence field and no 404 -- so this tool
        // cannot invent the distinction on its own. The branch below already
        // honours an existence field (`entity: null` or `exists: false`), so the
        // daemon side is a one-line change; see the finding in the report.
        let entity_missing = resp
            .get("entity")
            .map(|e| e.is_null())
            .or_else(|| resp.get("exists").and_then(|e| e.as_bool()).map(|b| !b))
            .unwrap_or(false);
        if entity_missing {
            return Err(rmcp::ErrorData::internal_error(
                format!(
                    "entity #{} not found: the graph has no entity with that id",
                    params.id
                ),
                None,
            ));
        }
        let facts =
            facts_of(&resp).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        if facts.is_empty() {
            return Ok(format!("entity #{} has no current facts", params.id));
        }
        let mut out = String::new();
        for f in facts {
            out.push_str(&format!(
                "{} {} {}: {}\n",
                f["src"].as_i64().unwrap_or(0),
                f["relation"].as_str().unwrap_or("?"),
                f["dst"].as_i64().unwrap_or(0),
                f["fact_text"].as_str().unwrap_or("?"),
            ));
        }
        Ok(out)
    }

    // -- gen2 consumption surface (DEP-4 + R-B U-6; t46) ------------------
    //
    // Five READ-ONLY tools. Each one is a thin proxy to the HTTP route of the
    // same name -- ONE consumption surface, not a second implementation: the
    // JSON the daemon returns is the JSON the agent sees (pretty-printed only
    // where a table is unreadable otherwise). No write tool is added here: the
    // graph's merge/community writes stay human/agent decisions made through the
    // HTTP API with an explicit body.

    #[tool(
        description = "Audit trail for a forgotten memory: where the content still exists (the local index/file copies) and which conclusions were derived from it. Needs the memory's content_hash (64 hex chars, from memory_get)."
    )]
    async fn memory_forget_report(
        &self,
        Parameters(params): Parameters<HashParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/forget-report", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .query(&[("content_hash", &params.content_hash)])
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    #[tool(
        description = "Search the knowledge graph's entities by name/summary (a flat name search, NOT multi-hop). Use graph_retrieve when the question needs a path between things."
    )]
    async fn graph_search(
        &self,
        Parameters(params): Parameters<GraphSearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/graph/search", self.config.daemon_url);
        let mut req = self.http.get(&url).query(&[("q", params.query.clone())]);
        if let Some(limit) = params.limit {
            req = req.query(&[("limit", limit.to_string())]);
        }
        let resp: serde_json::Value = req
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    #[tool(
        description = "Multi-hop retrieval over the knowledge graph: seeds (entities matching the text) + evidence paths (edges with their temporal status and their source episode/session). Optional as_of=<RFC3339> answers 'what was true at that instant'; three spellings of one instant give identical evidence. `stats.graph_edges` is the graph's SIZE, not the edge count at as_of. Empty results carry stats.empty_reason -- read it instead of guessing."
    )]
    async fn graph_retrieve(
        &self,
        Parameters(params): Parameters<GraphRetrieveParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/graph/retrieve", self.config.daemon_url);
        let mut req = self.http.get(&url).query(&[("q", params.query.clone())]);
        if let Some(hops) = params.hops {
            req = req.query(&[("hops", hops.to_string())]);
        }
        if let Some(as_of) = &params.as_of {
            req = req.query(&[("as_of", as_of)]);
        }
        if let Some(sup) = params.include_superseded {
            req = req.query(&[("include_superseded", sup.to_string())]);
        }
        let resp: serde_json::Value = req
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    #[tool(
        description = "The generated wiki's pages: slug, status, freshness (fresh|stale|unknown), the recorded cite_coverage and has_anchors. `cite_coverage: null` means NO BUILD RECORDED a reading for that page -- it is not 0.0 and not 1.0: read it as unknown."
    )]
    async fn wiki_pages(&self) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/wiki/pages", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    #[tool(
        description = "The wiki's link graph: links_out per page (counts broken links, excludes self-links) and links_in, plus the pages that wanted-but-missing pages demand."
    )]
    async fn wiki_links(&self) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/wiki/links", self.config.daemon_url);
        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    // -- platform ops (the symmetric design, §7.2) -----------------------

    #[tool(
        description = "List the platform's tasks (durable work items) with their status. Optionally filter by status: pending|in_progress|blocked|done|cancelled."
    )]
    async fn list_tasks(
        &self,
        Parameters(params): Parameters<TaskFilterParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/tasks", self.config.daemon_url);
        let mut req = self.http.get(&url);
        if let Some(status) = &params.status {
            req = req.query(&[("status", status)]);
        }
        let resp: serde_json::Value = req
            .send()
            .await
            .map_err(rpc_error)?
            .error_for_status()
            .map_err(rpc_error)?
            .json()
            .await
            .map_err(rpc_error)?;
        let tasks = resp["tasks"].as_array().cloned().unwrap_or_default();
        let mut out = String::new();
        for t in tasks.iter().take(20) {
            out.push_str(&format!(
                "[{}] {} ({})\n",
                t["status"].as_str().unwrap_or("?"),
                t["title"].as_str().unwrap_or("?"),
                t["id"].as_str().unwrap_or("?")
            ));
        }
        if out.is_empty() {
            out.push_str("no tasks");
        }
        Ok(out)
    }
}

/// The `facts` array of a graph-entity response, or an error that NAMES the
/// drift (t93 / audit M1). Public so the roundtrip test can press the branch
/// without a live daemon: `unwrap_or_default()` here is what made a renamed
/// field indistinguishable from "no facts".
pub fn facts_of(resp: &serde_json::Value) -> Result<Vec<serde_json::Value>, String> {
    resp.get("facts")
        .and_then(|v| v.as_array())
        .cloned()
        .ok_or_else(|| format!("daemon response carries no `facts` array: {resp}"))
}

#[test]
fn t93_facts_of_rejects_a_renamed_or_mistyped_field() {
    let normal = serde_json::json!({ "facts": [] });
    assert_eq!(
        facts_of(&normal)
            .expect("an empty array is a reading")
            .len(),
        0
    );

    let one = serde_json::json!({ "facts": [{ "src": 1, "relation": "runs_on", "dst": 2 }] });
    assert_eq!(facts_of(&one).unwrap().len(), 1);

    let renamed = serde_json::json!({ "edges": [] });
    let err = facts_of(&renamed).expect_err("a renamed field must be an error, not []");
    assert!(err.contains("no `facts` array"), "{err}");

    let mistyped = serde_json::json!({ "facts": "no" });
    assert!(
        facts_of(&mistyped).is_err(),
        "a non-array `facts` must be an error"
    );
}

#[tool_handler]
impl ServerHandler for PlatformTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "ruagent platform tools: search/write the user's long-term memory, \
             search/ingest the knowledge base (expand hits into their full parent \
             sections with knowledge_expand), fetch knowledge-graph entities, and \
             list platform tasks. Prefer memory_search before asking the user for \
             known facts.",
        )
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "What to look for")]
    pub query: String,
    #[schemars(description = "Max results (default 8)")]
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RecallParams {
    #[schemars(description = "What to look for")]
    pub query: String,
    #[schemars(
        description = "true = stubs only (cheap, fetch details with memory_get); false = full content (default)"
    )]
    pub conservative: Option<bool>,
    #[schemars(description = "Max results per section (default 5)")]
    pub top_n: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct HashParams {
    #[schemars(description = "The memory's content_hash (64 hex chars)")]
    pub content_hash: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GraphSearchParams {
    #[schemars(description = "Name/summary text to look for")]
    pub query: String,
    #[schemars(description = "Max results (default 8)")]
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GraphRetrieveParams {
    #[schemars(description = "The question, in words")]
    pub query: String,
    #[schemars(description = "Hops to walk (default 2, max 3)")]
    pub hops: Option<u32>,
    #[schemars(description = "RFC3339 instant: answer 'what was true then'")]
    pub as_of: Option<String>,
    #[schemars(description = "true = include edges that were superseded")]
    pub include_superseded: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetParams {
    #[schemars(description = "Memory id from recall/search results")]
    pub id: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WriteParams {
    #[schemars(description = "profile | observation | procedure | lesson")]
    pub store: String,
    #[schemars(description = "user | global | project:<name> | agent:<name>")]
    pub namespace: String,
    #[schemars(description = "The memory content, one concise fact")]
    pub content: String,
    #[schemars(description = "Memory id this one replaces (optional)")]
    pub supersedes: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IngestParams {
    #[schemars(description = "Document name")]
    pub name: String,
    #[schemars(description = "Document content")]
    pub content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ChunkParams {
    #[schemars(description = "Chunk id from recall/search results")]
    pub chunk_id: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EntityParams {
    #[schemars(description = "Entity id from recall results")]
    pub id: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TaskFilterParams {
    #[schemars(description = "Optional status filter")]
    pub status: Option<String>,
}

/// Render a /recall response for an agent: sectioned, ids preserved
/// (conservative stubs point at memory_get / knowledge_expand /
/// graph_entity).
fn recall_to_text(resp: &serde_json::Value) -> String {
    let mut out = Vec::new();
    for section in ["memories", "knowledge", "wiki", "entities"] {
        let items = resp[section].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            continue;
        }
        out.push(format!("## {section}"));
        for it in items.iter() {
            let line = match it["kind"].as_str().unwrap_or("?") {
                "memory" => {
                    let id = it["id"].clone();
                    let body = it["content"]
                        .as_str()
                        .or_else(|| it["title"].as_str())
                        .unwrap_or("?");
                    let score = it["score"]
                        .as_f64()
                        .map(|s| format!(" ({s:.2})"))
                        .unwrap_or_default();
                    format!("  #{id}{score} {body}")
                }
                "knowledge" => {
                    let doc = it["document"].as_str().unwrap_or("?");
                    let body = it["content"]
                        .as_str()
                        .or_else(|| it["excerpt"].as_str())
                        .unwrap_or("?");
                    let cid = it["chunk_id"].clone();
                    format!("  [{doc}] {body} (expand: {cid})")
                }
                "wiki" => {
                    // §13-2: generated pages — labeled and marked so the
                    // agent can verify against sources before trusting.
                    let slug = it["slug"].as_str().unwrap_or("?");
                    let title = it["title"].as_str().unwrap_or("?");
                    let stale = if it["stale"].as_bool().unwrap_or(false) {
                        " [sources updated since generation]"
                    } else {
                        ""
                    };
                    let cid = it["chunk_id"].clone();
                    format!(
                        "  [wiki/{slug}] {title}{stale} (expand: {cid}; GENERATED — verify against sources)"
                    )
                }
                "entity" => {
                    let id = it["id"].clone();
                    let name = it["name"].as_str().unwrap_or("?");
                    // §12-2: related wiki pages on the stub — labeled
                    // generated so the agent verifies before trusting
                    let wiki = it["related"]["wiki"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|w| w["slug"].as_str())
                                .collect::<Vec<_>>()
                        })
                        .filter(|v| !v.is_empty())
                        .map(|v| format!(" [wiki: {} (generated)]", v.join(", ")))
                        .unwrap_or_default();
                    match it["summary"].as_str() {
                        Some(s) => format!("  &{id} {name} — {s}{wiki}"),
                        None => format!("  &{id} {name} (call graph_entity for facts){wiki}"),
                    }
                }
                _ => continue,
            };
            out.push(line);
        }
    }
    if out.is_empty() {
        "no results".into()
    } else {
        format!(
            "{}\n(ids: #n = memory_get(n), &n = graph_entity(n), expand: n = knowledge_expand(n))",
            out.join("\n")
        )
    }
}

fn hits_to_text(hits: &serde_json::Value) -> String {
    let hits = hits.as_array().cloned().unwrap_or_default();
    if hits.is_empty() {
        return "no results".into();
    }
    hits.iter()
        .map(|h| {
            let content = h["content"]
                .as_str()
                .or_else(|| h["result"].as_str())
                .unwrap_or("?");
            let source = h["document"].as_str().unwrap_or("memory");
            format!("[{source}] {content}")
        })
        .collect::<Vec<_>>()
        .join("\n---\n")
}

fn rpc_error(e: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(format!("daemon call failed: {e}"), None)
}

/// Run the MCP server over stdio (the `ruagent mcp-serve` subcommand).
pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let handler = PlatformTools::new(BridgeConfig::from_env());
    let transport = rmcp::transport::stdio();
    let server = rmcp::service::serve_server(handler, transport).await?;
    let reason = server.waiting().await?;
    tracing::info!(?reason, "mcp server stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::recall_to_text;

    #[test]
    fn recall_to_text_renders_sections_and_entity_wiki() {
        let resp = serde_json::json!({
            "memories": [{ "kind": "memory", "id": 7, "title": "use sqlx", "hint": "call memory_get" }],
            "knowledge": [{ "kind": "knowledge", "chunk_id": 3, "document": "ops-handbook", "excerpt": "release.sh" }],
            "wiki": [{ "kind": "wiki", "slug": "deploy-pipeline", "title": "Deploy pipeline",
                        "stale": true, "chunk_id": 9, "hint": "verify" }],
            "entities": [{ "kind": "entity", "id": 1, "name": "Kubernetes",
                           "related": { "wiki": [{ "slug": "deploy-pipeline" }] } }],
        });
        let text = recall_to_text(&resp);
        assert!(text.contains("## memories"), "{text}");
        assert!(text.contains("#7 use sqlx"), "{text}");
        assert!(text.contains("## knowledge"), "{text}");
        assert!(
            text.contains("[ops-handbook] release.sh (expand: 3)"),
            "{text}"
        );
        // wiki section: labeled generated, stale flag present
        assert!(text.contains("## wiki"), "{text}");
        assert!(
            text.contains("[wiki/deploy-pipeline] Deploy pipeline [sources updated since generation] (expand: 9; GENERATED"),
            "{text}"
        );
        // entity line carries the soft-linked wiki pages
        assert!(
            text.contains(
                "&1 Kubernetes (call graph_entity for facts) [wiki: deploy-pipeline (generated)]"
            ),
            "{text}"
        );
    }

    #[test]
    fn recall_to_text_skips_empty_sections() {
        let resp =
            serde_json::json!({ "memories": [], "knowledge": [], "wiki": [], "entities": [] });
        assert_eq!(recall_to_text(&resp), "no results");
    }
}
