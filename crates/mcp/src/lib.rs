//! ruagent-mcp: the platform's own MCP server (design §6.5).
//!
//! A thin stdio-to-HTTP bridge: agents spawn `ruagent mcp-serve` (it is
//! registered in mcp.toml and injected into every session); the tools
//! call the daemon's local REST API. This is the ONLY seam external
//! harnesses need to reach the central memory, knowledge, and task
//! state — no SDK adapters (the Agno lesson).
//!
//! The capability plane (docs/plans/capability-plugins-design.md §14) rides the
//! same seam: `capabilities_list` / `capability_set` read and write the SAME
//! registry ids the HTTP API and `policy.toml` use, `distill_session` triggers
//! the llm-tier extraction, `knowledge_graph_ingest` the free-tier one. Every
//! description states WHEN to call the tool and its COST tier, so an agent can
//! pick the free path deliberately.

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
        description = "Search the user's long-term memories (facts, preferences, lessons learned across sessions). WHEN: before asking the user something they may have already told you, at the start of a task, or when a preference would change your approach. COST: free (zero model tokens). For ONE pass across memory + knowledge + wiki + graph, use memory_recall instead."
    )]
    async fn memory_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/search", self.config.daemon_url);
        let resp = json_of(
            self.http.get(&url).query(&[("q", &params.query)]),
            "memory_search",
            &self.config.daemon_url,
        )
        .await?;
        Ok(hits_to_text(&resp["hits"]))
    }

    #[tool(
        description = "Store a long-term memory about the user, a project, or a lesson learned. Stores: profile (user facts), observation (general), procedure (how-to, project/global), lesson (project/global). Namespaces: user, global, project:<name>, agent:<name>. Optionally pass supersedes=<memory id> to replace it. WHEN: the user asks you to remember something, or states a durable preference, decision or correction worth keeping beyond this session. COST: free (zero model tokens)."
    )]
    async fn memory_write(
        &self,
        Parameters(params): Parameters<WriteParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/write", self.config.daemon_url);
        let resp = json_of(
            self.http.post(&url).json(&serde_json::json!({
                "store": params.store,
                "namespace": params.namespace,
                "content": params.content,
                "supersedes": params.supersedes,
            })),
            "memory_write",
            &self.config.daemon_url,
        )
        .await?;
        Ok(format!(
            "memory write: {}",
            resp["outcome"].as_str().unwrap_or("?")
        ))
    }

    #[tool(
        description = "Unified recall across long-term memories, knowledge base, generated wiki pages and the knowledge graph — ONE pass over all four, so prefer it over four separate searches when the question spans the user's facts and their documents. WHEN: at the start of a session or task that needs context, or when a question may be answered by memories, documents or entity relations. COST: free (zero model tokens; every leg is a local index query, no model is called). STRATEGY: strategy=aggressive (default) returns full content above the relevance threshold; strategy=conservative returns stubs only (titles, entity names, relation one-liners) — cheap to scan, then fetch what you need with memory_get / knowledge_expand / graph_entity. conservative=true is the older spelling of strategy=conservative; passing both with disagreeing values is refused, not silently resolved. PER-LEG CONTROL: the six retrieval legs are switched and weighted through the capability plane, not through this tool — capability_set(id=recall_leg_wiki, enabled=false) drops a leg. Their ids are recall_leg_memory_semantic, recall_leg_memory_fts, recall_leg_knowledge_semantic, recall_leg_knowledge_fts, recall_leg_wiki, recall_leg_graph (capabilities_list shows which are off). The reply carries legs_disabled, naming the legs this call skipped, so a thin result is explained rather than guessed at. Wiki pages are AGENT-GENERATED: verify against sources before trusting them."
    )]
    async fn memory_recall(
        &self,
        Parameters(params): Parameters<RecallParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/recall", self.config.daemon_url);
        let strategy = recall_strategy(params.conservative, params.strategy.as_deref())?;
        let top_n = params.top_n.unwrap_or(5).to_string();
        let resp = json_of(
            self.http.get(&url).query(&[
                ("q", &params.query),
                ("strategy", &strategy.to_string()),
                ("top_n", &top_n),
            ]),
            "memory_recall",
            &self.config.daemon_url,
        )
        .await?;
        Ok(recall_to_text(&resp))
    }

    #[tool(
        description = "Fetch one memory's full content by id. WHEN: memory_search or memory_recall returned a hit whose full text you need (conservative recall returns stubs on purpose), or you already hold an id. COST: free (zero model tokens)."
    )]
    async fn memory_get(
        &self,
        Parameters(params): Parameters<GetParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/{}", self.config.daemon_url, params.id);
        let resp = json_of(self.http.get(&url), "memory_get", &self.config.daemon_url).await?;
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
        description = "Search the knowledge base of ingested documents (hybrid semantic + keyword search). WHEN: the answer should come from the user's documents, notes or code — memory holds facts, knowledge holds documents. COST: free (zero model tokens)."
    )]
    async fn knowledge_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/search", self.config.daemon_url);
        let resp = json_of(
            self.http.get(&url).query(&[("q", &params.query)]),
            "knowledge_search",
            &self.config.daemon_url,
        )
        .await?;
        Ok(hits_to_text(&resp["hits"]))
    }

    #[tool(
        description = "Ingest a document (markdown, notes, code) into the knowledge base so it is searchable from every later session. WHEN: the user wants a document remembered or searchable, or hands you content that should outlive this conversation. COST: free (zero model tokens). NOTE: ingesting does NOT by itself put entities and relations into the knowledge graph — the automatic knowledge→graph ingestion (capability knowledge_ingest_graph) is OFF by default. Call capabilities_list to check it, and knowledge_graph_ingest to put an already-ingested document into the graph (also zero tokens)."
    )]
    async fn knowledge_ingest(
        &self,
        Parameters(params): Parameters<IngestParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/ingest", self.config.daemon_url);
        let resp = json_of(
            self.http.post(&url).json(&serde_json::json!({
                "name": params.name,
                "content": params.content,
            })),
            "knowledge_ingest",
            &self.config.daemon_url,
        )
        .await?;
        Ok(format!(
            "ingested {} chunks",
            resp["chunks"].as_i64().unwrap_or(0)
        ))
    }

    #[tool(
        description = "Expand a knowledge-base hit into its full parent section: the complete context (heading + surrounding paragraphs) around the chunk. Pass the chunk_id from memory_recall or knowledge_search results. WHEN: a hit looks relevant but is too short to act on, or you need the surrounding section before quoting it. COST: free (zero model tokens)."
    )]
    async fn knowledge_expand(
        &self,
        Parameters(params): Parameters<ChunkParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!(
            "{}/api/v1/knowledge/expand/{}",
            self.config.daemon_url, params.chunk_id
        );
        let resp = json_of(
            self.http.get(&url),
            "knowledge_expand",
            &self.config.daemon_url,
        )
        .await?;
        Ok(format!(
            "[{}] {}\n\n--- hit chunk ---\n{}",
            resp["document"].as_str().unwrap_or("?"),
            resp["section"].as_str().unwrap_or("?"),
            resp["chunk"].as_str().unwrap_or("?"),
        ))
    }

    #[tool(
        description = "Fetch one knowledge-graph entity by id: its currently-valid facts (relations to other entities). WHEN: memory_recall returned an entity stub (&id) and you need the facts, or you are about to rely on a relation and want its current text. COST: free (zero model tokens)."
    )]
    async fn graph_entity(
        &self,
        Parameters(params): Parameters<EntityParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!(
            "{}/api/v1/graph/entity/{}",
            self.config.daemon_url, params.id
        );
        let resp = json_of(self.http.get(&url), "graph_entity", &self.config.daemon_url).await?;
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
        description = "Audit trail for a forgotten memory: where the content still exists (the local index/file copies) and which conclusions were derived from it. Needs the memory's content_hash (64 hex chars, from memory_get). WHEN: the user asks what is left of a memory they asked you to forget, or before claiming a deletion is complete. COST: free (zero model tokens)."
    )]
    async fn memory_forget_report(
        &self,
        Parameters(params): Parameters<HashParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/forget-report", self.config.daemon_url);
        let resp = json_of(
            self.http
                .get(&url)
                .query(&[("content_hash", &params.content_hash)]),
            "memory_forget_report",
            &self.config.daemon_url,
        )
        .await?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    #[tool(
        description = "Search the knowledge graph's entities by name/summary (a flat name search, NOT multi-hop). WHEN: you want candidate entities by name; use graph_retrieve when the question needs a path between things. COST: free (zero model tokens)."
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
        let resp = json_of(req, "graph_search", &self.config.daemon_url).await?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    #[tool(
        description = "Multi-hop retrieval over the knowledge graph: seeds (entities matching the text) + evidence paths (edges with their temporal status and their source episode/session). Optional as_of=<RFC3339> answers 'what was true at that instant'; three spellings of one instant give identical evidence. stats.graph_edges is the graph's SIZE, not the edge count at as_of. Empty results carry stats.empty_reason -- read it instead of guessing. WHEN: the question needs multi-hop evidence, or a time-bounded answer. COST: free (zero model tokens)."
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
        let resp = json_of(req, "graph_retrieve", &self.config.daemon_url).await?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    #[tool(
        description = "The generated wiki's pages: slug, status, freshness (fresh|stale|unknown), the recorded cite_coverage and has_anchors. cite_coverage: null means NO BUILD RECORDED a reading for that page -- it is not 0.0 and not 1.0: read it as unknown. WHEN: you need to know which generated pages exist and how stale they are before citing one. COST: free (zero model tokens)."
    )]
    async fn wiki_pages(
        &self,
        Parameters(_params): Parameters<NoParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/wiki/pages", self.config.daemon_url);
        let resp = json_of(self.http.get(&url), "wiki_pages", &self.config.daemon_url).await?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    #[tool(
        description = "The wiki's link graph: links_out per page (counts broken links, excludes self-links) and links_in, plus the pages that wanted-but-missing pages demand. WHEN: you are checking the wiki's coherence (broken links, wanted pages) or deciding what to generate next. COST: free (zero model tokens)."
    )]
    async fn wiki_links(
        &self,
        Parameters(_params): Parameters<NoParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/wiki/links", self.config.daemon_url);
        let resp = json_of(self.http.get(&url), "wiki_links", &self.config.daemon_url).await?;
        Ok(serde_json::to_string_pretty(&resp).unwrap_or_else(|_| resp.to_string()))
    }

    // -- the capability plane (docs/plans/capability-plugins-design.md §14) --

    #[tool(
        description = "List the platform's switchable pipelines (the capability plane), one row per capability: its id, token tier, whether it is enabled NOW, whether that came from policy.toml or from the registry default, its option values, and what it gates. WHEN: BEFORE you rely on recall, session distillation, memory injection or knowledge→graph ingestion; when the user asks what is on or off; or when a pipeline did nothing and you need to know whether it is simply switched off. COST: free (zero model tokens) -- this is a config read, so read it instead of guessing. Optional tier=free (zero tokens, deterministic) or tier=llm (spends the user's model tokens) filters the list. The ids here are the SAME strings used by capability_set, by the HTTP API and as [capabilities.<id>] sections in policy.toml -- use them verbatim, never a paraphrase."
    )]
    async fn capabilities_list(
        &self,
        Parameters(params): Parameters<CapabilitiesParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/capabilities", self.config.daemon_url);
        let resp = json_of(
            self.http.get(&url),
            "capabilities_list",
            &self.config.daemon_url,
        )
        .await?;
        capabilities_to_text(&resp, params.tier.as_deref())
    }

    // DELIBERATE TOOL-METADATA CHANGE (ruagent-tunable-capabilities t13, the
    // follow-up to t10). This description no longer ENUMERATES the option keys:
    // which keys a capability declares is DATA (`options_schema` on every
    // `capabilities_list` row), and a prose copy of that set has no parser and no
    // test, so it would go stale silently the day the registry declares a different
    // set — the fourth spelling of one fact and the only unguarded one.
    //
    // WHY IT DOES NOT BREAK THE "TOOL SURFACE STAYS STABLE" RULE: the PARAMETER
    // schema — the fields a client sends (`CapabilitySetParams`, below) — is
    // untouched, so no caller breaks; nothing parses a description, and no tool was
    // added or removed. This removes a staleness source and changes no behaviour.
    #[tool(
        description = "Enable, disable or re-weight ONE capability by id: id=distill_session, enabled=false stops unattended distillation; id=knowledge_ingest_graph, enabled=true lets knowledge documents feed the knowledge graph; id=recall_leg_wiki, enabled=false drops that recall leg. WHEN: the user asks to turn a pipeline on or off, or complains about the behaviour or the cost of one. COST: this call is free, but ENABLING an llm-tier capability (distill_session) makes the platform spend the user's model tokens from then on: say so out loud, and only then pass confirm_cost=true -- without it the daemon refuses the write (HTTP 409) and nothing changes. That check is on the CONFIGURATION, not on what is effectively running, so it is required even when [distill].auto is already true. The write goes to policy.toml and takes effect immediately; the reply is the full post-change state, so read it instead of calling capabilities_list again. OPTION KEYS ARE NOT LISTED HERE ON PURPOSE: which keys a capability accepts is declared per row by capabilities_list's options_schema (one entry per declared key, with its kind, its bounds and the daemon's own accepted-range phrase) -- read the row for the id you are about to change before setting an option; the parameter list below is what this tool can send, and a field this tool does not declare is REFUSED rather than ignored (so a typo cannot look like a successful write). An option you omit keeps its current value, and an unknown id or undeclared key is refused with the known ids and the declared keys named. This tool changes exactly one capability and KEEPS every capability policy.toml already configures, because the daemon's PUT replaces the whole [capabilities] table: a bare one-id write would silently reset every other capability to its registry default (and an llm-tier default is OFF). The first write on a root with no [capabilities] table creates one, which puts every capability at its registry default -- read the reply's conflicts[] list: it names any capability whose legacy switch is still on while the capability is now off, and prints the exact [capabilities.<id>] line that restores it."
    )]
    async fn capability_set(
        &self,
        Parameters(params): Parameters<CapabilitySetParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let what = "capability_set";
        let url = format!("{}/api/v1/capabilities", self.config.daemon_url);
        // READ-MODIFY-WRITE. The daemon's PUT REPLACES the whole
        // `[capabilities]` table, so a bare one-id write would silently reset
        // every other capability to its registry default (and an llm-tier
        // default is OFF). Read the rows, re-emit every capability policy.toml
        // already configures, then apply the one change on top.
        let current = json_of(self.http.get(&url), what, &self.config.daemon_url).await?;
        let rows = current
            .get("capabilities")
            .and_then(|v| v.as_array())
            .ok_or_else(|| drift(what, "capabilities", &current))?;
        let table = merged_table(rows, &params)?;
        let body = capability_set_body(table, params.confirm_cost.unwrap_or(false));
        let resp = json_of(
            self.http.put(&url).json(&body),
            what,
            &self.config.daemon_url,
        )
        .await?;
        capability_set_to_text(&resp, &params)
    }

    // -- the two trigger surfaces (§14.3) --------------------------------

    #[tool(
        description = "Distill ONE session now: extract durable memories (and, when the distiller is configured for it, entities/relations) from that session's transcript. WHEN: the user says to remember a session or distill a conversation, or a session ended with something worth keeping that is not in memory yet. COST: extractor=rules is FREE -- zero model tokens, deterministic, no agent is spawned; extractor=acp (the default) SPENDS MODEL TOKENS -- it runs a full extraction-agent turn over the transcript; extractor=both runs rules and then acp. Pass dry_run=true first: it reports what would be written and writes nothing. This manual call is an explicit request and is never blocked by the distill_session capability -- that capability gates only the unattended session-close path."
    )]
    async fn distill_session(
        &self,
        Parameters(params): Parameters<DistillSessionParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let what = "distill_session";
        if params.session_key.is_empty() || params.session_key.contains('/') {
            return Err(rmcp::ErrorData::internal_error(
                format!(
                    "distill_session: session_key `{}` is not a session key -- it must be non-empty and contain no `/`; the keys look like `ruagent:<hex>` and come from the sessions listing",
                    params.session_key
                ),
                None,
            ));
        }
        let extractor = extractor_of(params.extractor.as_deref())?;
        let dry_run = params.dry_run.unwrap_or(false);
        let url = format!(
            "{}/api/v1/sessions/{}/distill",
            self.config.daemon_url, params.session_key
        );
        let resp = json_of(
            self.http.post(&url).json(&distill_body(extractor, dry_run)),
            what,
            &self.config.daemon_url,
        )
        .await?;
        distill_to_text(&resp, dry_run)
    }

    #[tool(
        description = "Put knowledge-base documents into the knowledge graph: extract entities, aliases and relations deterministically and write them, so graph_search and graph_retrieve can answer from your documents. WHEN: the user wants their documents reachable as entities/relations, or asks why a document's entities are missing from the graph. COST: free (zero model tokens -- no model is called), but it WRITES entities and relations, and the AUTOMATIC path is OFF by default: the capability knowledge_ingest_graph gates the 60-second background sweep, and this explicit call is the immediate path. dry_run=true always works (capability on or off) and prices the work with counts only; dry_run=false while the capability is off is refused (HTTP 409) naming the capability and the remedy. With document=<name> it ingests that one document (idempotent per content hash); with no document it sweeps up to the configured max_docs_per_pass documents the ledger has not seen. Read the reply's ledger_hits: it is why a re-run writes nothing."
    )]
    async fn knowledge_graph_ingest(
        &self,
        Parameters(params): Parameters<KnowledgeGraphIngestParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/graph/ingest", self.config.daemon_url);
        let query = ingest_query(params.document.as_deref(), params.dry_run.unwrap_or(false));
        let resp = json_of(
            self.http.post(&url).query(&query),
            "knowledge_graph_ingest",
            &self.config.daemon_url,
        )
        .await?;
        ingest_to_text(&resp)
    }

    // -- platform ops (the symmetric design, §7.2) -----------------------

    #[tool(
        description = "List the platform's tasks (durable work items) with their status. Optionally filter by status: pending|in_progress|blocked|done|cancelled. WHEN: you need the shared board before creating or picking up durable work, or the user asks what is in flight. COST: free (zero model tokens)."
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
        let resp = json_of(req, "list_tasks", &self.config.daemon_url).await?;
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

// ---------------------------------------------------------------------------
// HTTP + error shaping
//
// Every tool goes through `json_of`, so a refusal reaches the agent as the
// DAEMON'S OWN sentence (it names the capability id, the key, the range or the
// cost gate) instead of reqwest's "HTTP status client error (400 Bad Request)".
// ---------------------------------------------------------------------------

/// The message for a non-2xx daemon reply: the daemon's reason, verbatim, plus
/// the concrete way out when it is a capability refusal.
///
/// The daemon's `ApiError` renders as (status, plain text) — so the JSON probe
/// below is a courtesy for handlers that answer `{"error": ...}`, and the text
/// fallback is the common path. Pure, so the shaping is tested without a daemon.
fn refusal_message(what: &str, status: u16, body: &str) -> String {
    let text = body.trim();
    let detail = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.as_str())
                .or_else(|| v.get("message").and_then(|m| m.as_str()))
                .map(str::to_string)
        })
        .unwrap_or_else(|| text.to_string());
    let mut msg = format!("{what} refused by the ruagent daemon (HTTP {status}): {detail}");
    // A capability refusal must name the action, not just the refusal (t6
    // acceptance: "never an opaque transport error"). The cost gate's own
    // message already carries `confirm_cost`, and an unknown-id message already
    // lists the known ids — do not repeat either.
    let names_a_capability = detail.contains("capability `")
        || detail.contains("[capabilities.")
        || detail.contains("capabilities.");
    if names_a_capability && !detail.contains("confirm_cost") && !detail.contains("known ids") {
        msg.push_str(
            " Fix: capabilities_list names every id and its tier; then \
             capability_set(id: \"<id>\", enabled: true) for a free-tier id, or \
             capability_set(id: \"<id>\", enabled: true, confirm_cost: true) for an \
             llm-tier id (which spends model tokens).",
        );
    }
    msg
}

fn transport_message(what: &str, base: &str, e: &reqwest::Error) -> String {
    format!(
        "{what}: cannot reach the ruagent daemon at {base} ({e}). Fix: start it \
         (`ruagent serve`, or scripts/ruagent-daemon.ps1 start) and call again."
    )
}

/// One HTTP call: send, read the body, decode JSON. Both failure modes name the
/// caller and the reason.
async fn json_of(
    req: reqwest::RequestBuilder,
    what: &str,
    base: &str,
) -> Result<serde_json::Value, rmcp::ErrorData> {
    let resp = req
        .send()
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(transport_message(what, base, &e), None))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| {
        rmcp::ErrorData::internal_error(
            format!("{what}: the daemon's reply could not be read: {e}"),
            None,
        )
    })?;
    if !status.is_success() {
        return Err(rmcp::ErrorData::internal_error(
            refusal_message(what, status.as_u16(), &body),
            None,
        ));
    }
    serde_json::from_str(&body).map_err(|e| {
        rmcp::ErrorData::internal_error(
            format!(
                "{what}: the daemon answered HTTP {status} with a body that is not JSON ({e}): {}",
                clip(&body, 400)
            ),
            None,
        )
    })
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let total = s.chars().count();
    let head: String = s.chars().take(max).collect();
    format!("{head}… ({total} chars total)")
}

/// A response that does not carry a field this bridge depends on is PROTOCOL
/// DRIFT, not an empty reading — the `facts_of` discipline (t93).
fn drift(what: &str, field: &str, resp: &serde_json::Value) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(
        format!("{what}: the daemon response carries no `{field}`: {resp}"),
        None,
    )
}

fn field_str<'a>(
    what: &str,
    obj: &'a serde_json::Value,
    key: &str,
) -> Result<&'a str, rmcp::ErrorData> {
    obj.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| drift(what, key, obj))
}

fn field_bool(what: &str, obj: &serde_json::Value, key: &str) -> Result<bool, rmcp::ErrorData> {
    obj.get(key)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| drift(what, key, obj))
}

/// [`field_bool`] for a key inside an object the caller already extracted (the
/// daemon's `options_set`), so the drift message names WHICH object was
/// incomplete rather than dumping the whole row under the key's name.
fn map_bool(
    what: &str,
    obj_name: &str,
    map: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<bool, rmcp::ErrorData> {
    map.get(key).and_then(|v| v.as_bool()).ok_or_else(|| {
        rmcp::ErrorData::internal_error(
            format!(
                "{what}: the daemon response carries no `{key}` in `{obj_name}`: {}",
                serde_json::Value::Object(map.clone())
            ),
            None,
        )
    })
}

fn field_u64(what: &str, obj: &serde_json::Value, key: &str) -> Result<u64, rmcp::ErrorData> {
    obj.get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| drift(what, key, obj))
}

// ---------------------------------------------------------------------------
// The capability plane
// ---------------------------------------------------------------------------

/// One entry of the daemon's `options_schema`: a key the REGISTRY DECLARES for
/// this capability, the kind of number it is, the daemon's own accepted-range
/// phrase, the value the config currently resolves for it, and whether the FILE
/// carries the key at all.
///
/// `value` and `carried` are deliberately kept apart: `value` is what `options`
/// resolved (the file's value, else the registry default) while `carried` is what
/// `options_set` reports (the file's own key set). A renderer that walks only
/// resolved values cannot see a declared key with NO registry default, and a
/// writer that re-emits resolved values writes registry defaults into the user's
/// file (design §11.5).
struct DeclaredOption<'a> {
    key: &'a str,
    kind: &'a str,
    expectation: &'a str,
    value: Option<&'a serde_json::Value>,
    carried: bool,
}

/// Read one row's declared options FROM THE DAEMON.
///
/// This is the ONE place the key set comes from. It replaces the local
/// `const OPTION_KEYS: [&str; 5]` this bridge used to keep — the THIRD copy of
/// that list in the tree, after the registry's own table and the panel's copy
/// (design §11.5, the follow-up this increment closes). A local list cannot see a
/// key the registry adds, and it cannot tell "the file carries this key" from "the
/// config resolves a registry default for it"; the daemon reports both
/// (`options_schema` / `options_set`) and this bridge no longer guesses either.
///
/// Missing either field is protocol drift, refused loudly like every other field
/// this bridge depends on — reporting no options is exactly the failure mode
/// (`facts_of`, t93).
fn declared_options<'a>(
    what: &str,
    row: &'a serde_json::Value,
) -> Result<Vec<DeclaredOption<'a>>, rmcp::ErrorData> {
    let schema = row
        .get("options_schema")
        .and_then(|v| v.as_array())
        .ok_or_else(|| drift(what, "options_schema", row))?;
    let set = row
        .get("options_set")
        .and_then(|v| v.as_object())
        .ok_or_else(|| drift(what, "options_set", row))?;
    let mut out = Vec::with_capacity(schema.len());
    for entry in schema {
        let key = field_str(what, entry, "key")?;
        out.push(DeclaredOption {
            key,
            kind: field_str(what, entry, "kind")?,
            expectation: field_str(what, entry, "expectation")?,
            value: row
                .get("options")
                .and_then(|o| o.get(key))
                .filter(|v| !v.is_null()),
            carried: map_bool(what, "options_set", set, key)?,
        });
    }
    Ok(out)
}

/// Render one row's options: every key the REGISTRY DECLARES, in registry order,
/// with its current value and where that value comes from — so a declared key with
/// no default (no resolved value to render) is visible AND actionable instead of
/// absent, and so an agent can tell a value the user set from a default. A row
/// that declares nothing says so rather than printing dead keys.
fn options_to_text(row: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "capabilities_list";
    let declared = declared_options(WHAT, row)?;
    if declared.is_empty() {
        return Ok("none declared (this capability accepts no option key)".to_string());
    }
    let mut parts = Vec::with_capacity(declared.len());
    for o in &declared {
        parts.push(match (o.value, o.carried) {
            (Some(v), true) => format!("{}={v} (the file carries it)", o.key),
            (Some(v), false) => format!("{}={v} ({} default, the file does not)", o.key, o.kind),
            (None, false) => format!(
                "{}=- (declared {}; the file does not carry it and the registry declares no \
                 default; accepted {})",
                o.key, o.kind, o.expectation
            ),
            // The daemon reports the file carries this key but resolved NO value
            // for it. That is drift, and inventing a value here would write a
            // wrong one into the user's config.
            (None, true) => return Err(drift(WHAT, &format!("options.{}", o.key), row)),
        });
    }
    Ok(parts.join(" "))
}

/// Render `GET /api/v1/capabilities` for an agent: what is on, what tier it
/// costs, where the value came from, and what it gates.
fn capabilities_to_text(
    resp: &serde_json::Value,
    tier_filter: Option<&str>,
) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "capabilities_list";
    if let Some(tier) = tier_filter
        && tier != "free"
        && tier != "llm"
    {
        return Err(rmcp::ErrorData::internal_error(
            format!(
                "capabilities_list: tier `{tier}` is not a token tier: use \"free\" (zero model \
                 tokens, deterministic) or \"llm\" (spends the user's model tokens)"
            ),
            None,
        ));
    }
    let rows = resp
        .get("capabilities")
        .and_then(|v| v.as_array())
        .ok_or_else(|| drift(WHAT, "capabilities", resp))?;
    let present = field_bool(WHAT, resp, "table_present")?;
    let file = resp
        .get("config_file")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let selected: Vec<&serde_json::Value> = match tier_filter {
        None => rows.iter().collect(),
        Some(tier) => rows
            .iter()
            .filter(|r| r.get("tier").and_then(|v| v.as_str()) == Some(tier))
            .collect(),
    };
    let selected_count = selected.len();

    let mut out = format!(
        "capability plane: {selected_count} of {} capabilities{}; [capabilities] table {} in {file}\n\
         (tier free = zero model tokens, deterministic; tier llm = spends the user's model tokens)\n",
        rows.len(),
        match tier_filter {
            Some(t) => format!(" with tier={t}"),
            None => String::new(),
        },
        if present {
            "PRESENT"
        } else {
            "ABSENT — today's behaviour, every gate passes through"
        },
    );

    for row in selected {
        let id = field_str(WHAT, row, "id")?;
        let tier = field_str(WHAT, row, "tier")?;
        let enabled = field_bool(WHAT, row, "enabled")?;
        let configured = field_str(WHAT, row, "configured")?;
        let new = row.get("new").and_then(|v| v.as_bool()).unwrap_or(false);
        out.push_str(&format!(
            "[{tier}] {id}: enabled={enabled} (configured: {configured}{})\n",
            if new { ", new" } else { "" },
        ));
        if let Some(d) = row.get("description").and_then(|v| v.as_str()) {
            out.push_str(&format!("    {d}\n"));
        }
        if let Some(g) = row.get("gates").and_then(|v| v.as_str()) {
            out.push_str(&format!("    gates: {g}\n"));
        }
        out.push_str(&format!("    options: {}\n", options_to_text(row)?));
    }
    if selected_count == 0
        && let Some(tier) = tier_filter
    {
        out.push_str(&format!("(no capability has tier={tier} right now)\n"));
    }

    let conflicts = resp
        .get("conflicts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if conflicts.is_empty() {
        out.push_str("conflicts: none — no legacy switch is being narrowed\n");
    } else {
        out.push_str(
            "conflicts (a legacy switch is ON while the capability is off — the platform is \
             narrower than the file suggests):\n",
        );
        for c in &conflicts {
            out.push_str(&format!(
                "  {} (legacy `{}`): {}\n",
                field_str(WHAT, c, "id")?,
                field_str(WHAT, c, "legacy_key")?,
                field_str(WHAT, c, "reason")?,
            ));
        }
    }
    out.push_str(
        "Change one with capability_set(id: \"<id>\", enabled: true|false); an llm-tier id \
         additionally needs confirm_cost: true.\n",
    );
    Ok(out)
}

/// One `[capabilities.<id>]` entry rebuilt from an API row: EXACTLY the keys the
/// FILE already carries — `enabled` when the file carries it, plus every declared
/// option key `options_set` marks present, valued from `options` (for a key the
/// file carries, the value `options` resolves IS the file's own value).
///
/// Two properties ride on this, and both have been broken once already:
///
/// * THE READ-MODIFY-WRITE. The daemon's PUT REPLACES the whole `[capabilities]`
///   table and removes every row and every key an entry omits
///   (`CapabilitiesEditor::update` in `crates/daemon/src/config.rs`), so re-emitting
///   each `configured == "file"` row is the ONLY reason one toggle does not delete
///   every other configured capability.
/// * NO RE-MATERIALIZATION. Re-emitting the RESOLVED values instead persists
///   registry DEFAULTS into rows the user never set (an `enabled = true`-only row
///   became `enabled = true` + `weight = 1`), which is why a Reset was undone by
///   the next unrelated write. `options_set` — the daemon's answer to "which keys
///   does the file carry" — is the only source used here, so a key the file does
///   not carry is not written, `enabled` included.
fn entry_of_row(
    row: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, rmcp::ErrorData> {
    const WHAT: &str = "capability_set";
    let set = row
        .get("options_set")
        .and_then(|v| v.as_object())
        .ok_or_else(|| drift(WHAT, "options_set", row))?;
    let mut entry = serde_json::Map::new();
    if map_bool(WHAT, "options_set", set, "enabled")? {
        entry.insert(
            "enabled".into(),
            serde_json::Value::Bool(field_bool(WHAT, row, "enabled")?),
        );
    }
    for o in declared_options(WHAT, row)? {
        if !o.carried {
            continue;
        }
        let Some(v) = o.value else {
            return Err(drift(WHAT, &format!("options.{}", o.key), row));
        };
        entry.insert(o.key.to_string(), v.clone());
    }
    Ok(entry)
}

/// The fields the caller actually set. An omitted option is "leave the current
/// value", never "reset it".
///
/// This is the typed-argument half of the tool's INPUT SCHEMA and not a key list:
/// a static MCP tool schema has one fixed field per option, so the arm per field is
/// unavoidable and is not a second source of truth — it decides nothing. Whether a
/// key is legal for the target capability, and whether its value is in range, is
/// decided by the DAEMON, whose refusal this bridge surfaces verbatim
/// ([`refusal_message`]) rather than inventing a local message that could drift.
fn apply_params(
    entry: &mut serde_json::Map<String, serde_json::Value>,
    params: &CapabilitySetParams,
) {
    entry.insert("enabled".into(), serde_json::Value::Bool(params.enabled));
    if let Some(v) = params.weight {
        entry.insert("weight".into(), serde_json::json!(v));
    }
    if let Some(v) = params.min_score {
        entry.insert("min_score".into(), serde_json::json!(v));
    }
    if let Some(v) = params.max_per_input {
        entry.insert("max_per_input".into(), serde_json::json!(v));
    }
    if let Some(v) = params.min_confidence {
        entry.insert("min_confidence".into(), serde_json::json!(v));
    }
    if let Some(v) = params.max_docs_per_pass {
        entry.insert("max_docs_per_pass".into(), serde_json::json!(v));
    }
}

/// The `[capabilities]` table to PUT: every capability policy.toml already
/// configures (`configured == "file"`), then the one requested row on top. Each
/// re-emitted row carries its OWN key set, never the values the config resolves
/// for it ([`entry_of_row`]).
///
/// `legacy` (no table at all) and `default` (table present, id absent) rows are
/// deliberately NOT re-emitted: they already resolve to the registry default, so
/// writing them would turn a default into a frozen `file` value the user never
/// asked for.
fn merged_table(
    rows: &[serde_json::Value],
    params: &CapabilitySetParams,
) -> Result<serde_json::Map<String, serde_json::Value>, rmcp::ErrorData> {
    const WHAT: &str = "capability_set";
    let mut table = serde_json::Map::new();
    for row in rows {
        let id = field_str(WHAT, row, "id")?;
        let configured = field_str(WHAT, row, "configured")?;
        if configured != "file" {
            continue;
        }
        let mut entry = entry_of_row(row)?;
        if id == params.id {
            apply_params(&mut entry, params);
        }
        table.insert(id.to_string(), serde_json::Value::Object(entry));
    }
    if !table.contains_key(&params.id) {
        let mut entry = serde_json::Map::new();
        apply_params(&mut entry, params);
        table.insert(params.id.clone(), serde_json::Value::Object(entry));
    }
    Ok(table)
}

/// The PUT body, exactly the shape §14.2 documents.
fn capability_set_body(
    table: serde_json::Map<String, serde_json::Value>,
    confirm_cost: bool,
) -> serde_json::Value {
    serde_json::json!({
        "confirm_cost": confirm_cost,
        "capabilities": serde_json::Value::Object(table),
    })
}

/// The post-change state, led by what actually changed and any narrowing the
/// write caused.
fn capability_set_to_text(
    resp: &serde_json::Value,
    params: &CapabilitySetParams,
) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "capability_set";
    let rows = resp
        .get("capabilities")
        .and_then(|v| v.as_array())
        .ok_or_else(|| drift(WHAT, "capabilities", resp))?;
    let written = rows
        .iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(params.id.as_str()));
    let mut out = match written {
        Some(row) => format!(
            "capability_set: `{}` is now enabled={} (tier {}, configured: {}). policy.toml and \
             the live plane were updated together.\n",
            params.id,
            field_bool(WHAT, row, "enabled")?,
            field_str(WHAT, row, "tier")?,
            field_str(WHAT, row, "configured")?,
        ),
        None => format!(
            "capability_set: `{}` was written, but the daemon's reply does not list it — read the \
             payload below.\n",
            params.id
        ),
    };
    let conflicts = resp
        .get("conflicts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if !conflicts.is_empty() {
        out.push_str(
            "WARNING — the resulting configuration is narrower than the legacy switches still \
             say:\n",
        );
        for c in &conflicts {
            out.push_str(&format!(
                "  {}: {}\n",
                field_str(WHAT, c, "id")?,
                field_str(WHAT, c, "reason")?,
            ));
        }
        out.push_str("The fix is the [capabilities.<id>] line the reason names.\n");
    }
    out.push_str(&serde_json::to_string_pretty(resp).unwrap_or_else(|_| resp.to_string()));
    out.push('\n');
    Ok(out)
}

// ---------------------------------------------------------------------------
// The two trigger surfaces
// ---------------------------------------------------------------------------

/// The extractor, defaulted and validated HERE: an unknown value must be a
/// refusal that names the free path, not an empty success or a silent "acp".
fn extractor_of(v: Option<&str>) -> Result<&'static str, rmcp::ErrorData> {
    match v {
        None | Some("acp") => Ok("acp"),
        Some("rules") => Ok("rules"),
        Some("both") => Ok("both"),
        Some(other) => Err(rmcp::ErrorData::internal_error(
            format!(
                "distill_session: extractor `{other}` is not an extractor: use \"rules\" (FREE, \
                 zero model tokens, deterministic), \"acp\" (SPENDS model tokens -- an \
                 extraction-agent turn over the transcript) or \"both\""
            ),
            None,
        )),
    }
}

fn distill_body(extractor: &str, dry_run: bool) -> serde_json::Value {
    serde_json::json!({ "extractor": extractor, "dry_run": dry_run })
}

fn distill_to_text(
    resp: &serde_json::Value,
    requested_dry_run: bool,
) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "distill_session";
    let d = resp
        .get("distilled")
        .ok_or_else(|| drift(WHAT, "distilled", resp))?;
    if !d.is_object() {
        return Err(drift(WHAT, "distilled (an object)", resp));
    }
    let dry = d
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(requested_dry_run);
    let source = match d.get("source").and_then(|v| v.as_str()) {
        Some(s) => format!(" (extractor: {s})"),
        None => String::new(),
    };
    let mut out = format!(
        "distilled `{}` via {}{source}: {} memories written, {} skipped, {} entities, {} \
         relations{}",
        field_str(WHAT, d, "session_key")?,
        field_str(WHAT, d, "agent")?,
        field_u64(WHAT, d, "memories_written")?,
        field_u64(WHAT, d, "memories_skipped")?,
        field_u64(WHAT, d, "entities_written")?,
        field_u64(WHAT, d, "relations_written")?,
        if dry {
            " [DRY RUN — nothing was written]"
        } else {
            ""
        },
    );
    if d.get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        out.push_str(" (the transcript was truncated to the prompt budget)");
    }
    out.push('\n');
    Ok(out)
}

/// The ingest route takes QUERY parameters and no body (t4's frozen shape).
/// `dry_run` is always sent explicitly, because the daemon refuses any other
/// spelling and an absent one would be a silent default.
fn ingest_query(document: Option<&str>, dry_run: bool) -> Vec<(String, String)> {
    let mut query = vec![("dry_run".to_string(), dry_run.to_string())];
    if let Some(doc) = document
        && !doc.is_empty()
    {
        query.push(("document".to_string(), doc.to_string()));
    }
    query
}

fn ingest_to_text(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "knowledge_graph_ingest";
    let g = resp
        .get("ingest")
        .ok_or_else(|| drift(WHAT, "ingest", resp))?;
    if !g.is_object() {
        return Err(drift(WHAT, "ingest (an object)", resp));
    }
    let dry = g.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut out = format!(
        "knowledge -> graph ingest{}: {} documents, {} entities, {} relations, {} candidates",
        if dry {
            " [DRY RUN — nothing written, no ledger row]"
        } else {
            ""
        },
        field_u64(WHAT, g, "documents")?,
        field_u64(WHAT, g, "entities")?,
        field_u64(WHAT, g, "relations")?,
        field_u64(WHAT, g, "candidates")?,
    );
    for key in [
        "skipped",
        "duplicates",
        "refused",
        "truncated",
        "ledger_hits",
    ] {
        match g.get(key) {
            Some(v) => out.push_str(&format!(", {key}={v}")),
            None => out.push_str(&format!(", {key}=not reported")),
        }
    }
    out.push('\n');
    Ok(out)
}

/// The strategy: one value, two spellings, and a refusal when they disagree —
/// never a silent pick (L3). `strategy` is the daemon's own vocabulary.
fn recall_strategy(
    conservative: Option<bool>,
    strategy: Option<&str>,
) -> Result<&'static str, rmcp::ErrorData> {
    let from_flag = conservative.map(|c| if c { "conservative" } else { "aggressive" });
    let asked = match strategy {
        None => None,
        Some("aggressive") => Some("aggressive"),
        Some("conservative") => Some("conservative"),
        Some(other) => {
            return Err(rmcp::ErrorData::internal_error(
                format!(
                    "memory_recall: strategy `{other}` is not a recall strategy: use \
                     \"aggressive\" (full content above the relevance threshold, the default) or \
                     \"conservative\" (stubs only, then fetch what you need)"
                ),
                None,
            ));
        }
    };
    match (from_flag, asked) {
        (Some(a), Some(b)) if a != b => Err(rmcp::ErrorData::internal_error(
            format!(
                "memory_recall: `conservative` and `strategy` disagree (conservative={a} says \
                 `{a}` while strategy=\"{b}\") — pass one of them, not both"
            ),
            None,
        )),
        (_, Some(b)) => Ok(b),
        (Some(a), None) => Ok(a),
        (None, None) => Ok("aggressive"),
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

#[tool_handler]
impl ServerHandler for PlatformTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "ruagent platform tools: search/write the user's long-term memory, \
             search/ingest the knowledge base (expand hits into their full parent \
             sections with knowledge_expand), fetch knowledge-graph entities, and \
             list platform tasks. Prefer memory_search before asking the user for \
             known facts. The platform's pipelines are individually switchable \
             (the capability plane): call capabilities_list BEFORE depending on \
             recall, distillation or knowledge ingestion — it names each \
             capability's token tier (free = zero model tokens, deterministic; \
             llm = spends the user's model tokens) and whether it is on. \
             capability_set changes exactly one of them (enabling an llm-tier \
             capability needs confirm_cost: true and spends the user's tokens), \
             distill_session distils one session (extractor=rules is free, \
             extractor=acp spends tokens, dry_run prices either), and \
             knowledge_graph_ingest fills the knowledge graph from the knowledge \
             base (free, and off by default).",
        )
    }
}

// ---------------------------------------------------------------------------
// Tool parameters. EVERY struct here carries `deny_unknown_fields`.
// ---------------------------------------------------------------------------
//
// WHY EVERY ONE (ruagent-tunable-capabilities t21, finding F1 — MEASURED: 15 params
// structs, only `CapabilitySetParams` carried the attribute, and 17 of the 18 PUBLISHED
// schemas left `additionalProperties` unset). Without the attribute serde silently
// DISCARDS a field the struct does not declare, so `capabilities_list(tier=…,
// skip_semantic=true)` answers SUCCESS with the extra field dropped and the caller's
// intent ignored — the same caller-visible false success t17 fixed for
// `capability_set`, repeated one tool over. For a READ tool that is a lying success;
// for a WRITE tool it is worse, because a mistyped flag quietly means "the default", so
// the write runs for real when the caller meant to price it first.
//
// COMPATIBILITY CONSEQUENCE, stated once and plainly: a client that sends a field the
// tool does not declare now gets a HARD FAILURE — a tool result with `is_error: true`
// carrying serde's `unknown field …` sentence — instead of silence. Nothing else
// changes: no declared field was added, removed, renamed, retyped or re-defaulted, and
// the 18 tool names are untouched. A tool that ships WITHOUT the attribute fails
// `crates/mcp/tests/tool_surface.rs::every_tool_refuses_fields_it_does_not_declare`,
// which walks every published schema rather than naming the tools one by one.
//
// `wiki_pages` and `wiki_links` take no argument, and they now carry [`NoParams`]
// rather than no parameter type at all: without a params type rmcp publishes
// `{"type":"object","properties":{}}`, whose `additionalProperties` is unset, so those
// two silently accepted and discarded ANY field. `NoParams` declares no field either —
// the declared surface is unchanged — and it makes the refusal uniform across all 18.

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchParams {
    #[schemars(description = "What to look for")]
    pub query: String,
    #[schemars(description = "Max results (default 8)")]
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecallParams {
    #[schemars(description = "What to look for")]
    pub query: String,
    #[schemars(
        description = "true = stubs only (cheap, fetch details with memory_get); false = full content (default). The older spelling of strategy=conservative|aggressive; passing both with disagreeing values is refused."
    )]
    pub conservative: Option<bool>,
    #[schemars(
        description = "aggressive (default) = full content above the relevance threshold; conservative = stubs only. Prefer this to `conservative`. Per-leg switches live in the capability plane (recall_leg_* ids), not here."
    )]
    pub strategy: Option<String>,
    #[schemars(description = "Max results per section (default 5, max 20)")]
    pub top_n: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HashParams {
    #[schemars(description = "The memory's content_hash (64 hex chars)")]
    pub content_hash: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphSearchParams {
    #[schemars(description = "Name/summary text to look for")]
    pub query: String,
    #[schemars(description = "Max results (default 8)")]
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct GetParams {
    #[schemars(description = "Memory id from recall/search results")]
    pub id: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct IngestParams {
    #[schemars(description = "Document name")]
    pub name: String,
    #[schemars(description = "Document content")]
    pub content: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChunkParams {
    #[schemars(description = "Chunk id from recall/search results")]
    pub chunk_id: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntityParams {
    #[schemars(description = "Entity id from recall results")]
    pub id: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskFilterParams {
    #[schemars(description = "Optional status filter")]
    pub status: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesParams {
    #[schemars(
        description = "Optional token-tier filter: free (zero model tokens, deterministic) or llm (spends the user's model tokens). Anything else is refused."
    )]
    pub tier: Option<String>,
}

/// The parameters of `capability_set`, as a CLIENT sends them.
///
/// `deny_unknown_fields` IS THE FIX FOR A MEASURED FALSE SUCCESS
/// (ruagent-tunable-capabilities t17): `capability_set(id=…, enabled=true,
/// decay_half_life=5.0)` used to answer SUCCESS while serde silently dropped that
/// key before this bridge ever saw it — the caller was told the update happened and
/// the field was never sent. With this attribute rmcp's own parameter extraction
/// (`FromContextPart for Parameters<P>`) fails FIRST, and MEASURED the client sees a
/// tool result with `is_error: true` whose text is `failed to deserialize
/// parameters: unknown field `decay_half_life`, expected one of `id`, `enabled`, …`
/// — the key that was not sent, and every field this tool can send instead (the
/// `ErrorData` behind it is `invalid_params`, -32602). See
/// `crates/mcp/tests/tool_surface.rs`.
///
/// CHOICE, AND THE ALTERNATIVE REJECTED. The recorded alternative was a catch-all
/// field (`#[serde(flatten)] extra: serde_json::Map<…>`) refused by hand in the
/// handler. Rejected: it needs a phantom field, it adds a hand-written validation
/// path that nothing keeps in step with the field list, and its generated schema
/// advertises the OPPOSITE of the truth — a flattened map tells every client that
/// arbitrary extra properties are allowed, which is the false promise being removed
/// here. The attribute states wire-ability declaratively, at the earliest possible
/// point, with no code.
///
/// COMPATIBILITY CONSEQUENCE, stated rather than slipped in: a client that sends a
/// field this struct does not declare now gets a hard failure — `is_error: true`
/// carrying serde's `unknown field …` sentence — instead of a silent success. That
/// is the intended change and the ONLY one: no field was added or removed, no name
/// changed, and every field a client legitimately sends still deserializes exactly
/// as before. The one schema-level difference is measured, not guessed:
/// `additionalProperties: false` appears in this tool's `inputSchema`, derived by
/// schemars from this attribute, so the schema now says what the extractor enforces.
///
/// The DAEMON remains the authority on whether a SENDABLE key is legal for the
/// target capability (an undeclared key is a 400 naming the declared set); this
/// struct decides only what can be put on the wire.
///
/// DELIBERATELY LEFT ALONE (finding F3, ruagent-tunable-capabilities t17):
/// schemars still publishes `minimum: 0` on the two u32 fields (`max_per_input`,
/// `max_docs_per_pass`) and nothing numeric on the three f64 ones (`weight`,
/// `min_score`, `min_confidence`). Those come from the FIELD TYPES, not from the
/// registry, so they cannot go stale when `OptionKey::bounds()` moves. The ranges
/// that COULD drift are no longer written in ANY string this tool publishes — they
/// were the five descriptions' range clauses — and each description now sends the
/// reader to `options_schema` instead.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySetParams {
    #[schemars(
        description = "Capability id, spelled exactly as capabilities_list, the HTTP API and policy.toml spell it (e.g. recall_leg_wiki, knowledge_ingest_graph, distill_session)"
    )]
    pub id: String,
    #[schemars(description = "true = turn it on, false = turn it off")]
    pub enabled: bool,
    #[schemars(
        description = "Optional fusion weight, for a capability whose row declares it; omitted = keep the current value. WHICH capabilities declare it, and its accepted range, are DATA: capabilities_list's options_schema for the target row (each entry carries the kind, the bounds and the daemon's own accepted-range phrase)."
    )]
    pub weight: Option<f64>,
    #[schemars(
        description = "Optional relevance-floor override, for a row whose options_schema declares it; omitted = keep the current value. Read that row's options_schema for the accepted range — never a range copied into this string."
    )]
    pub min_score: Option<f64>,
    #[schemars(
        description = "Optional cap on candidates per input, for a row whose options_schema declares it; omitted = keep the current value. Read that row's options_schema for the accepted range."
    )]
    pub max_per_input: Option<u32>,
    #[schemars(
        description = "Optional confidence floor, for a row whose options_schema declares it; omitted = keep the current value. Read that row's options_schema for the accepted range."
    )]
    pub min_confidence: Option<f64>,
    #[schemars(
        description = "Optional cap on documents per sweep, for a row whose options_schema declares it; omitted = keep the current value. Read that row's options_schema for the accepted range."
    )]
    pub max_docs_per_pass: Option<u32>,
    #[schemars(
        description = "Required (true) to enable an llm-tier capability, which spends the user's model tokens"
    )]
    pub confirm_cost: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DistillSessionParams {
    #[schemars(description = "Session key as the sessions listing spells it (e.g. ruagent:<hex>)")]
    pub session_key: String,
    #[schemars(
        description = "rules = FREE, zero model tokens, deterministic; acp (default) = SPENDS model tokens (an extraction-agent turn); both = rules then acp. Anything else is refused."
    )]
    pub extractor: Option<String>,
    #[schemars(
        description = "true = report what would be written and write nothing (the cheap way to price an acp run)"
    )]
    pub dry_run: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeGraphIngestParams {
    #[schemars(
        description = "Document name to ingest; omitted or empty = sweep the documents the ledger has not seen, up to the configured max_docs_per_pass"
    )]
    pub document: Option<String>,
    #[schemars(
        description = "true = extract and report the counts without writing to the graph (works even while the capability is off)"
    )]
    pub dry_run: Option<bool>,
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
    // §11.4: the endpoint reports the legs the capability plane switched off.
    // A thin result must be EXPLAINED ("the configuration asked for nothing")
    // rather than left for the agent to guess at.
    let disabled: Vec<&str> = resp["legs_disabled"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    if !disabled.is_empty() {
        out.push(format!(
            "(legs disabled by the capability plane: {} — capabilities_list shows the rest)",
            disabled.join(", ")
        ));
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

/// Run the MCP server over stdio (the `ruagent mcp-serve` subcommand).
pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let handler = PlatformTools::new(BridgeConfig::from_env());
    let transport = rmcp::transport::stdio();
    let server = rmcp::service::serve_server(handler, transport).await?;
    let reason = server.waiting().await?;
    tracing::info!(?reason, "mcp server stopped");
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn recall_to_text_explains_the_legs_the_plane_switched_off() {
        // §11.4: `legs_disabled` is the additive field that turns a thin result
        // into a reading. Dropping it must not silently look like "no hits".
        let resp = serde_json::json!({
            "memories": [],
            "knowledge": [],
            "wiki": [],
            "entities": [],
            "legs_disabled": ["recall_leg_graph", "recall_leg_wiki"],
        });
        let text = recall_to_text(&resp);
        assert!(
            text.contains("legs disabled by the capability plane"),
            "{text}"
        );
        assert!(text.contains("recall_leg_graph, recall_leg_wiki"), "{text}");
    }

    // -- memory_recall strategy control ---------------------------------

    #[test]
    fn recall_strategy_defaults_to_aggressive_and_accepts_both_spellings() {
        assert_eq!(recall_strategy(None, None).unwrap(), "aggressive");
        assert_eq!(recall_strategy(Some(false), None).unwrap(), "aggressive");
        assert_eq!(recall_strategy(Some(true), None).unwrap(), "conservative");
        assert_eq!(
            recall_strategy(None, Some("conservative")).unwrap(),
            "conservative"
        );
        assert_eq!(
            recall_strategy(None, Some("aggressive")).unwrap(),
            "aggressive"
        );
        // agreeing spellings are not a conflict
        assert_eq!(
            recall_strategy(Some(true), Some("conservative")).unwrap(),
            "conservative"
        );
        assert_eq!(
            recall_strategy(Some(false), Some("aggressive")).unwrap(),
            "aggressive"
        );
    }

    #[test]
    fn recall_strategy_refuses_an_unknown_value_and_a_disagreement() {
        let err = recall_strategy(None, Some("stubs")).expect_err("unknown strategy");
        let msg = err.to_string();
        assert!(msg.contains("stubs"), "{msg}");
        assert!(
            msg.contains("aggressive") && msg.contains("conservative"),
            "{msg}"
        );

        let err = recall_strategy(Some(true), Some("aggressive")).expect_err("disagreement");
        let msg = err.to_string();
        assert!(msg.contains("disagree"), "{msg}");
        assert!(
            msg.contains("conservative") && msg.contains("aggressive"),
            "{msg}"
        );
    }

    // -- capabilities_list ----------------------------------------------

    /// A row of `GET /api/v1/capabilities`, in the shape the daemon actually
    /// serves: `options` RESOLVED, `options_schema` what the registry declares
    /// (registry order) and `options_set` the keys the FILE carries. The four rows
    /// mirror the real registry: one that declares nothing, one whose declared
    /// `min_score` has NO registry default, one whose declared keys are all
    /// defaulted, and the llm-tier one.
    fn plane_response() -> serde_json::Value {
        serde_json::json!({
            "table_present": false,
            "config_file": "/home/u/.ruagent/config/policy.toml",
            "capabilities": [
                { "id": "recall_leg_wiki", "tier": "free", "description": "Recall leg: agent-generated wiki pages (leads, not ground truth).", "gates": "the wiki/ partition of the knowledge hits", "default_enabled": true, "enabled": true, "configured": "legacy", "new": false,
                  "options": { "weight": null, "min_score": null, "max_per_input": null, "min_confidence": null, "max_docs_per_pass": null },
                  "options_schema": [],
                  "options_set": { "enabled": false } },
                { "id": "recall_leg_memory_semantic", "tier": "free", "description": "Recall leg: cosine similarity over memory embeddings.", "gates": "the cosine leg of GET /api/v1/recall", "default_enabled": true, "enabled": true, "configured": "legacy", "new": false,
                  "options": { "weight": 1.0, "min_score": null, "max_per_input": null, "min_confidence": null, "max_docs_per_pass": null },
                  "options_schema": [
                    { "key": "weight", "kind": "float", "min": 0.0, "max": 100.0, "default": 1.0, "expectation": "finite and 0.0..=100.0" },
                    { "key": "min_score", "kind": "float", "min": 0.0, "max": 1.0, "default": null, "expectation": "finite and 0.0..=1.0" }
                  ],
                  "options_set": { "enabled": false, "weight": false, "min_score": false } },
                { "id": "session_extract_rules", "tier": "free", "description": "Zero-token deterministic extraction of a closed session into memory candidates.", "gates": "the unattended transcript -> memory extractor", "default_enabled": false, "enabled": false, "configured": "default", "new": true,
                  "options": { "weight": null, "min_score": null, "max_per_input": 32, "min_confidence": 0.0, "max_docs_per_pass": null },
                  "options_schema": [
                    { "key": "max_per_input", "kind": "uint", "min": 1.0, "max": 10000.0, "default": 32.0, "expectation": "1..=10000" },
                    { "key": "min_confidence", "kind": "float", "min": 0.0, "max": 1.0, "default": 0.0, "expectation": "finite and 0.0..=1.0" }
                  ],
                  "options_set": { "enabled": false, "max_per_input": false, "min_confidence": false } },
                { "id": "distill_session", "tier": "llm", "description": "ACP-agent distillation when a session closes (spends model tokens).", "gates": "auto-distill on session close", "default_enabled": false, "enabled": false, "configured": "legacy", "new": false,
                  "options": { "weight": null, "min_score": null, "max_per_input": null, "min_confidence": null, "max_docs_per_pass": null },
                  "options_schema": [],
                  "options_set": { "enabled": false } }
            ],
            "conflicts": [],
        })
    }

    /// The fixture row with `id`, mutably, the way the daemon would report it once
    /// the file names the row.
    fn row_mut<'a>(resp: &'a mut serde_json::Value, id: &str) -> &'a mut serde_json::Value {
        resp["capabilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == serde_json::json!(id))
            .expect("the fixture carries this id")
    }

    #[test]
    fn capabilities_to_text_names_id_tier_enabled_state_and_options() {
        let text = capabilities_to_text(&plane_response(), None).unwrap();
        // The id, the tier and the enabled state must be readable without the raw JSON.
        assert!(
            text.contains("[llm] distill_session: enabled=false"),
            "{text}"
        );
        assert!(
            text.contains("[free] recall_leg_wiki: enabled=true"),
            "{text}"
        );
        assert!(text.contains("(configured: legacy"), "{text}");
        assert!(text.contains("configured: default, new"), "{text}");
        assert!(text.contains("max_per_input=32"), "{text}");
        // A resolved, defaulted key says so — the file does not carry it.
        assert!(
            text.contains("max_per_input=32 (uint default, the file does not)"),
            "{text}"
        );
        // A row that declares NOTHING says so instead of printing five dead keys:
        // the keys are the registry's, not this bridge's.
        assert!(
            text.contains("options: none declared (this capability accepts no option key)"),
            "{text}"
        );
        assert!(
            text.contains("gates: auto-distill on session close"),
            "{text}"
        );
        // absent table = today's behaviour, and the footer names the next call
        assert!(
            text.contains("table_present") || text.contains("ABSENT"),
            "{text}"
        );
        assert!(text.contains("capability_set(id:"), "{text}");
        assert!(text.contains("conflicts: none"), "{text}");
    }

    #[test]
    fn capabilities_to_text_filters_by_tier_and_refuses_a_bogus_tier() {
        let text = capabilities_to_text(&plane_response(), Some("llm")).unwrap();
        assert!(text.contains("distill_session"), "{text}");
        assert!(!text.contains("recall_leg_wiki"), "{text}");
        assert!(text.contains("1 of 4 capabilities"), "{text}");

        let err = capabilities_to_text(&plane_response(), Some("cheap")).expect_err("bogus tier");
        let msg = err.to_string();
        assert!(msg.contains("cheap"), "{msg}");
        assert!(msg.contains("free") && msg.contains("llm"), "{msg}");
    }

    #[test]
    fn capabilities_to_text_refuses_protocol_drift_instead_of_reporting_nothing() {
        // A row without `configured` cannot be told apart from "the file never
        // mentions it", and guessing would make capability_set write the wrong
        // table. Loud, naming the field (the `facts_of` discipline).
        let resp = serde_json::json!({
            "table_present": false,
            "config_file": "policy.toml",
            "capabilities": [{ "id": "recall_leg_wiki", "tier": "free", "enabled": true }],
            "conflicts": [],
        });
        let err = capabilities_to_text(&resp, None).expect_err("missing configured");
        let msg = err.to_string();
        assert!(msg.contains("configured"), "{msg}");

        let no_rows = serde_json::json!({ "table_present": false, "conflicts": [] });
        let err = capabilities_to_text(&no_rows, None).expect_err("missing capabilities");
        assert!(err.to_string().contains("capabilities"), "{err}");

        let no_present = serde_json::json!({ "capabilities": [], "conflicts": [] });
        let err = capabilities_to_text(&no_present, None).expect_err("missing table_present");
        assert!(err.to_string().contains("table_present"), "{err}");

        // The declared key set is the DAEMON's, so a row that does not report it is
        // drift too: rendering nothing would read as "this capability has no
        // options", which is a different (and wrong) statement.
        let mut no_schema = plane_response();
        row_mut(&mut no_schema, "recall_leg_wiki")
            .as_object_mut()
            .unwrap()
            .remove("options_schema");
        let err = capabilities_to_text(&no_schema, None).expect_err("missing options_schema");
        assert!(err.to_string().contains("options_schema"), "{err}");

        let mut no_set = plane_response();
        row_mut(&mut no_set, "recall_leg_wiki")
            .as_object_mut()
            .unwrap()
            .remove("options_set");
        let err = capabilities_to_text(&no_set, None).expect_err("missing options_set");
        assert!(err.to_string().contains("options_set"), "{err}");
    }

    /// The key set comes from `options_schema`, NOT from a list this bridge keeps:
    /// a declared key the fixture never asked for is rendered, and one whose value
    /// the config does not resolve is still rendered WITH the daemon's own
    /// accepted-range phrase — which is how a declared key with no registry default
    /// stays reachable (the hole this increment closes, design §11.5).
    #[test]
    fn capabilities_to_text_renders_every_key_the_daemon_declares() {
        let mut resp = plane_response();
        row_mut(&mut resp, "recall_leg_wiki")["options_schema"] = serde_json::json!([
            { "key": "decay_half_life", "kind": "float", "min": 0.0, "max": 50.0,
              "default": null, "expectation": "finite and 0.0..=50.0" }
        ]);
        row_mut(&mut resp, "recall_leg_wiki")["options_set"] =
            serde_json::json!({ "enabled": false, "decay_half_life": false });
        let text = capabilities_to_text(&resp, None).unwrap();
        assert!(text.contains("decay_half_life"), "{text}");
        assert!(text.contains("finite and 0.0..=50.0"), "{text}");
        assert!(
            text.contains("decay_half_life=- (declared float; the file does not carry it"),
            "a declared key with no value is NAMED, not omitted: {text}"
        );

        // The live instance of that hole: `min_score` on the memory semantic leg
        // has no registry default, and its declared phrase is the daemon's.
        let text = capabilities_to_text(&plane_response(), None).unwrap();
        assert!(
            text.contains(
                "min_score=- (declared float; the file does not carry it and the registry \
                 declares no default; accepted finite and 0.0..=1.0)"
            ),
            "{text}"
        );
        assert!(
            text.contains("weight=1.0 (float default, the file does not)"),
            "{text}"
        );

        // ... and a key the FILE carries is reported as the file's, not as a default.
        let mut carried = plane_response();
        row_mut(&mut carried, "recall_leg_memory_semantic")["options"]["weight"] =
            serde_json::json!(3.5);
        row_mut(&mut carried, "recall_leg_memory_semantic")["options_set"] =
            serde_json::json!({ "enabled": false, "weight": true, "min_score": false });
        let text = capabilities_to_text(&carried, None).unwrap();
        assert!(text.contains("weight=3.5 (the file carries it)"), "{text}");
    }

    /// `options_set` saying the file carries a key must come with a resolved value:
    /// a renderer that invented one would write it back on the next toggle.
    #[test]
    fn capabilities_to_text_refuses_a_carried_key_with_no_value() {
        let mut resp = plane_response();
        {
            let row = row_mut(&mut resp, "recall_leg_memory_semantic");
            row["options"]["weight"] = serde_json::json!(null);
            row["options_set"] =
                serde_json::json!({ "enabled": false, "weight": true, "min_score": false });
        }
        let err = capabilities_to_text(&resp, None).expect_err("carried but no value");
        let msg = err.to_string();
        assert!(msg.contains("options.weight"), "{msg}");
    }

    #[test]
    fn capabilities_to_text_reports_a_conflict_verbatim() {
        let mut resp = plane_response();
        resp["conflicts"] = serde_json::json!([{
            "id": "distill_session",
            "legacy_key": "distill.auto",
            "reason": "`[distill] auto = true` but capability `distill_session` is off. Add [capabilities.distill_session] enabled = true to restore it.",
        }]);
        let text = capabilities_to_text(&resp, None).unwrap();
        assert!(text.contains("distill.auto"), "{text}");
        assert!(
            text.contains("[capabilities.distill_session] enabled = true"),
            "{text}"
        );
    }

    // -- capability_set --------------------------------------------------

    fn set_params(id: &str, enabled: bool) -> CapabilitySetParams {
        CapabilitySetParams {
            id: id.to_string(),
            enabled,
            weight: None,
            min_score: None,
            max_per_input: None,
            min_confidence: None,
            max_docs_per_pass: None,
            confirm_cost: None,
        }
    }

    #[test]
    fn capability_set_body_is_the_documented_put_shape() {
        let params = set_params("recall_leg_graph", false);
        let table = merged_table(
            &plane_response()["capabilities"].as_array().unwrap().clone(),
            &params,
        )
        .unwrap();
        let body = capability_set_body(table, false);
        assert_eq!(body["confirm_cost"], serde_json::json!(false));
        assert_eq!(
            body["capabilities"]["recall_leg_graph"]["enabled"],
            serde_json::json!(false)
        );
        // The same id string the API and policy.toml use — no parallel vocabulary.
        assert!(body["capabilities"].get("recall_leg_graph").is_some());
        assert!(body.to_string().contains("recall_leg_graph"));
    }

    /// THE READ-MODIFY-WRITE, with the KEY SET preserved. The daemon's PUT
    /// replaces the whole table, so a one-id write must carry every other
    /// `configured: "file"` row back — carrying the keys THAT ROW's file has, not
    /// the values the config resolves for it. Re-emitting resolved values is the
    /// measured defect (`enabled = true` alone became `enabled = true` +
    /// `weight = 1`), and the third row here is the witness: its file carries
    /// `weight` and NO `enabled`, so the entry must carry `weight` and no
    /// `enabled`.
    #[test]
    fn capability_set_preserves_every_other_rows_key_set_exactly() {
        let mut resp = plane_response();
        {
            let row = row_mut(&mut resp, "recall_leg_wiki");
            row["configured"] = serde_json::json!("file");
            row["options_set"] = serde_json::json!({ "enabled": true });
        }
        {
            // The file carries `enabled` only: `weight`/`min_confidence` resolve to
            // registry defaults and must NOT be written.
            let row = row_mut(&mut resp, "session_extract_rules");
            row["configured"] = serde_json::json!("file");
            row["enabled"] = serde_json::json!(true);
            row["options_set"] = serde_json::json!({
                "enabled": true, "max_per_input": false, "min_confidence": false
            });
        }
        {
            // The file carries `weight` only: no `enabled` key may be added.
            let row = row_mut(&mut resp, "recall_leg_memory_semantic");
            row["configured"] = serde_json::json!("file");
            row["options"]["weight"] = serde_json::json!(3.5);
            row["options_set"] =
                serde_json::json!({ "enabled": false, "weight": true, "min_score": false });
        }
        let rows = resp["capabilities"].as_array().unwrap().clone();
        let params = set_params("recall_leg_wiki", false);
        let table = merged_table(&rows, &params).unwrap();

        assert_eq!(
            table["recall_leg_wiki"]["enabled"],
            serde_json::json!(false)
        );
        // the row the write did not name keeps its file's key set, `enabled` and
        // nothing else
        assert_eq!(
            table["session_extract_rules"],
            serde_json::json!({ "enabled": true })
        );
        // ... and this one keeps `weight` and NO `enabled`
        assert_eq!(
            table["recall_leg_memory_semantic"],
            serde_json::json!({ "weight": 3.5 })
        );
        // `legacy`/`default` rows are not frozen into the file
        assert!(!table.contains_key("distill_session"));
        assert!(!table.contains_key("recall_leg_memory_fts"));
    }

    /// A `configured == "file"` row whose `options_set` is missing cannot be told
    /// apart from one whose file carries nothing, and guessing would DELETE the
    /// user's weights on the next toggle. Loud, naming the field.
    #[test]
    fn capability_set_refuses_to_guess_which_keys_the_file_carries() {
        let mut resp = plane_response();
        {
            let row = row_mut(&mut resp, "recall_leg_memory_semantic");
            row["configured"] = serde_json::json!("file");
            row.as_object_mut().unwrap().remove("options_set");
        }
        let rows = resp["capabilities"].as_array().unwrap().clone();
        let err = merged_table(&rows, &set_params("recall_leg_wiki", false))
            .expect_err("drift must be loud");
        assert!(err.to_string().contains("options_set"), "{err}");
    }

    /// t17 item 1, at the level where it is enforced: the struct itself refuses a
    /// field it cannot send, so no handler code can be tempted to "handle" an
    /// unsendable key. rmcp's parameter extraction
    /// (`FromContextPart for Parameters<P>`) runs this — measured: the client sees a
    /// tool result with `is_error: true` whose text is
    /// `failed to deserialize parameters: unknown field \`decay_half_life\`,
    /// expected one of \`id\`, \`enabled\`, …`.
    #[test]
    fn an_option_field_this_tool_cannot_send_is_refused_at_the_wire() {
        let ok = serde_json::from_value::<CapabilitySetParams>(serde_json::json!({
            "id": "recall_leg_memory_semantic",
            "enabled": true,
            "min_score": 0.4
        }))
        .expect("every declared field still deserializes");
        assert_eq!(ok.min_score, Some(0.4));

        let err = serde_json::from_value::<CapabilitySetParams>(serde_json::json!({
            "id": "recall_leg_memory_semantic",
            "enabled": true,
            "decay_half_life": 5.0
        }))
        .expect_err("a field this tool cannot send must not deserialize silently");
        let msg = err.to_string();
        assert!(msg.contains("unknown field `decay_half_life`"), "{msg}");
        // ... and the message names the fields that ARE sendable, so a caller can fix
        // the call without reading this source.
        for key in [
            "weight",
            "min_score",
            "max_per_input",
            "min_confidence",
            "max_docs_per_pass",
        ] {
            assert!(
                msg.contains(key),
                "the refusal does not name `{key}`: {msg}"
            );
        }
    }

    #[test]
    fn capability_set_writes_only_what_the_caller_set_for_an_unconfigured_id() {
        let rows = plane_response()["capabilities"].as_array().unwrap().clone();
        let mut params = set_params("knowledge_ingest_graph", true);
        params.max_docs_per_pass = Some(5);
        let table = merged_table(&rows, &params).unwrap();
        let entry = &table["knowledge_ingest_graph"];
        assert_eq!(entry["enabled"], serde_json::json!(true));
        assert_eq!(entry["max_docs_per_pass"], serde_json::json!(5));
        // an omitted option is "leave the current value", never "reset it"
        assert!(entry.get("weight").is_none());
        assert!(entry.get("max_per_input").is_none());
    }

    #[test]
    fn capability_set_keeps_an_existing_weight_when_only_enabled_is_toggled() {
        let mut resp = plane_response();
        {
            let row = row_mut(&mut resp, "recall_leg_memory_semantic");
            row["configured"] = serde_json::json!("file");
            row["enabled"] = serde_json::json!(false);
            row["options"]["weight"] = serde_json::json!(3.5);
            row["options_set"] =
                serde_json::json!({ "enabled": true, "weight": true, "min_score": false });
        }
        let rows = resp["capabilities"].as_array().unwrap().clone();
        let table = merged_table(&rows, &set_params("recall_leg_memory_semantic", false)).unwrap();
        assert_eq!(
            table["recall_leg_memory_semantic"]["weight"],
            serde_json::json!(3.5)
        );
        assert_eq!(
            table["recall_leg_memory_semantic"]["enabled"],
            serde_json::json!(false)
        );
        // only the keys the file carries are re-emitted; the write added none
        assert_eq!(
            table["recall_leg_memory_semantic"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            vec!["enabled", "weight"]
        );
    }

    /// The declared key with NO registry default is expressible: a caller that
    /// wants it sets it like any other, and the entry carries it.
    #[test]
    fn capability_set_can_set_a_declared_key_the_file_does_not_carry() {
        let rows = plane_response()["capabilities"].as_array().unwrap().clone();
        let mut params = set_params("recall_leg_memory_semantic", true);
        params.min_score = Some(0.4);
        let table = merged_table(&rows, &params).unwrap();
        assert_eq!(
            table["recall_leg_memory_semantic"],
            serde_json::json!({ "enabled": true, "min_score": 0.4 })
        );
    }

    #[test]
    fn capability_set_refuses_to_guess_when_a_row_has_no_configured_key() {
        let rows = serde_json::json!([{ "id": "recall_leg_wiki", "enabled": true }]);
        let err = merged_table(
            rows.as_array().unwrap(),
            &set_params("recall_leg_wiki", false),
        )
        .expect_err("drift must be loud");
        assert!(err.to_string().contains("configured"), "{err}");
    }

    #[test]
    fn capability_set_carries_confirm_cost_and_names_the_change() {
        let params = CapabilitySetParams {
            confirm_cost: Some(true),
            ..set_params("distill_session", true)
        };
        let table = merged_table(&[], &params).unwrap();
        let body = capability_set_body(table, params.confirm_cost.unwrap_or(false));
        assert_eq!(body["confirm_cost"], serde_json::json!(true));
        assert_eq!(
            body["capabilities"]["distill_session"]["enabled"],
            serde_json::json!(true)
        );

        let mut resp = plane_response();
        {
            let row = row_mut(&mut resp, "distill_session");
            row["configured"] = serde_json::json!("file");
            row["enabled"] = serde_json::json!(true);
        }
        let text = capability_set_to_text(&resp, &params).unwrap();
        assert!(
            text.contains("`distill_session` is now enabled=true (tier llm"),
            "{text}"
        );
        assert!(text.contains("policy.toml"), "{text}");
    }

    #[test]
    fn capability_set_warns_loudly_about_a_conflict() {
        let mut resp = plane_response();
        resp["conflicts"] = serde_json::json!([{
            "id": "distill_session",
            "legacy_key": "distill.auto",
            "reason": "Add [capabilities.distill_session] enabled = true to restore it.",
        }]);
        let text = capability_set_to_text(&resp, &set_params("recall_leg_graph", false)).unwrap();
        assert!(text.contains("WARNING"), "{text}");
        assert!(
            text.contains("distill.auto") || text.contains("distill_session"),
            "{text}"
        );
        assert!(text.contains("enabled = true to restore it"), "{text}");
    }

    // -- distill_session -------------------------------------------------

    #[test]
    fn distill_defaults_to_acp_and_validates_the_extractor() {
        assert_eq!(extractor_of(None).unwrap(), "acp");
        assert_eq!(extractor_of(Some("acp")).unwrap(), "acp");
        assert_eq!(extractor_of(Some("rules")).unwrap(), "rules");
        assert_eq!(extractor_of(Some("both")).unwrap(), "both");
        assert_eq!(
            distill_body("acp", false),
            serde_json::json!({ "extractor": "acp", "dry_run": false })
        );

        let err = extractor_of(Some("llm")).expect_err("unknown extractor");
        let msg = err.to_string();
        assert!(msg.contains("llm"), "{msg}");
        assert!(msg.contains("rules"), "{msg}");
        assert!(msg.contains("acp"), "{msg}");
        assert!(msg.contains("model tokens"), "{msg}");
    }

    #[test]
    fn distill_to_text_reports_the_counts_and_the_dry_run_marker() {
        let resp = serde_json::json!({
            "distilled": {
                "session_key": "ruagent:0f3a", "memories_written": 4, "memories_skipped": 1,
                "entities_written": 2, "relations_written": 3, "agent": "mock",
                "source": "rules", "truncated": true, "dry_run": true
            }
        });
        let text = distill_to_text(&resp, true).unwrap();
        assert!(text.contains("`ruagent:0f3a` via mock"), "{text}");
        assert!(text.contains("(extractor: rules)"), "{text}");
        assert!(
            text.contains("4 memories written, 1 skipped, 2 entities, 3 relations"),
            "{text}"
        );
        assert!(text.contains("DRY RUN"), "{text}");
        assert!(text.contains("truncated"), "{text}");
    }

    #[test]
    fn distill_to_text_refuses_a_response_without_the_load_bearing_keys() {
        // Everything today's shape carries except the relations count: a
        // renamed or missing field must be a refusal that NAMES it, never "0".
        let missing = serde_json::json!({ "distilled": {
            "session_key": "ruagent:0f3a", "agent": "mock", "memories_written": 1,
            "memories_skipped": 0, "entities_written": 0
        } });
        let err = distill_to_text(&missing, false).expect_err("drift must be loud");
        assert!(err.to_string().contains("relations_written"), "{err}");

        // A response with no counts at all names the first field it lacks.
        let bare = serde_json::json!({ "distilled": { "session_key": "ruagent:0f3a" } });
        let err = distill_to_text(&bare, false).expect_err("drift must be loud");
        assert!(err.to_string().contains("agent"), "{err}");

        let no_object = serde_json::json!({ "distilled": "ok" });
        let err = distill_to_text(&no_object, false).expect_err("drift must be loud");
        assert!(err.to_string().contains("distilled"), "{err}");

        let nothing = serde_json::json!({});
        let err = distill_to_text(&nothing, false).expect_err("drift must be loud");
        assert!(err.to_string().contains("distilled"), "{err}");
    }

    // -- knowledge_graph_ingest ------------------------------------------

    #[test]
    fn ingest_query_is_explicit_and_lowercase() {
        assert_eq!(
            ingest_query(None, false),
            vec![("dry_run".to_string(), "false".to_string())]
        );
        assert_eq!(
            ingest_query(Some("notes.md"), true),
            vec![
                ("dry_run".to_string(), "true".to_string()),
                ("document".to_string(), "notes.md".to_string()),
            ]
        );
        // an empty document means "sweep", not "a document named ''"
        assert_eq!(ingest_query(Some(""), false).len(), 1);
    }

    #[test]
    fn ingest_to_text_reports_the_frozen_counts_and_flags_missing_ones() {
        let resp = serde_json::json!({
            "ingest": {
                "documents": 2, "skipped": 0, "entities": 7, "relations": 5, "duplicates": 1,
                "refused": 0, "truncated": 0, "candidates": 12, "ledger_hits": 3, "dry_run": true
            }
        });
        let text = ingest_to_text(&resp).unwrap();
        assert!(
            text.contains("2 documents, 7 entities, 5 relations, 12 candidates"),
            "{text}"
        );
        assert!(text.contains("ledger_hits=3"), "{text}");
        assert!(text.contains("DRY RUN"), "{text}");

        // a count the caller acts on must exist: a missing one is drift, not 0
        let partial = serde_json::json!({ "ingest": { "documents": 2, "entities": 7 } });
        let err = ingest_to_text(&partial).expect_err("drift must be loud");
        assert!(err.to_string().contains("relations"), "{err}");

        // the diagnostic extras degrade VISIBLY instead of reading as zero
        let mut thin = resp.clone();
        thin["ingest"]
            .as_object_mut()
            .unwrap()
            .remove("ledger_hits");
        let text = ingest_to_text(&thin).unwrap();
        assert!(text.contains("ledger_hits=not reported"), "{text}");
    }

    // -- actionable errors -----------------------------------------------

    #[test]
    fn a_capability_refusal_names_the_id_and_the_exact_enable_action() {
        // The daemon's ApiError renders the message as PLAIN TEXT (that is what
        // reqwest hands back), so the fallback path is the common one.
        let body = "capability `knowledge_ingest_graph` is off: a real ingest writes entities \
                    and relations for the knowledge documents. Add \
                    [capabilities.knowledge_ingest_graph] enabled = true (free tier, zero \
                    tokens), or resend with dry_run=true to price it first.";
        let msg = refusal_message("knowledge_graph_ingest", 409, body);
        assert!(msg.contains("knowledge_ingest_graph"), "{msg}");
        assert!(msg.contains("HTTP 409"), "{msg}");
        assert!(msg.contains("capability_set(id:"), "{msg}");
        assert!(msg.contains("enabled: true"), "{msg}");
        // never an opaque transport error
        assert!(!msg.contains("error_for_status"), "{msg}");
    }

    #[test]
    fn the_llm_cost_gate_refusal_is_reported_without_a_second_hint() {
        let body = "capability `distill_session` is llm-tier: enabling it spends model tokens. \
                    Resend with \"confirm_cost\": true.";
        let msg = refusal_message("capability_set", 409, body);
        assert!(msg.contains("distill_session"), "{msg}");
        assert!(msg.contains("confirm_cost"), "{msg}");
        assert!(!msg.contains("Fix: capabilities_list"), "{msg}");
    }

    #[test]
    fn an_unknown_id_refusal_carries_the_known_ids() {
        let body = "unknown capability `recall_leg_wik` in [capabilities]: known ids are \
                    memory_inject_chat, recall_leg_wiki, distill_session";
        let msg = refusal_message("capability_set", 400, body);
        assert!(msg.contains("recall_leg_wik"), "{msg}");
        assert!(msg.contains("known ids are"), "{msg}");
    }

    #[test]
    fn a_json_error_body_is_unwrapped_and_a_plain_body_is_passed_through() {
        let json = refusal_message(
            "capability_set",
            409,
            "{\"error\":\"capability `distill_session` is llm-tier: enabling it spends model tokens. Resend with \\\"confirm_cost\\\": true.\"}",
        );
        assert!(json.contains("distill_session"), "{json}");
        assert!(!json.contains("{\"error\""), "{json}");

        let plain = refusal_message("memory_search", 500, "database is locked");
        assert!(plain.contains("database is locked"), "{plain}");
        assert!(plain.contains("HTTP 500"), "{plain}");
    }

    #[test]
    fn a_non_json_2xx_body_is_reported_with_its_head() {
        let long = "x".repeat(500);
        let clipped = clip(&long, 400);
        assert!(clipped.starts_with("xxxx"), "{clipped}");
        assert!(clipped.contains("500 chars total"), "{clipped}");
        assert!(clip("short", 400) == "short");
    }

    // -- the descriptions are the contract --------------------------------

    /// An agent reading ONLY the tool list must be able to choose correctly:
    /// every tool says WHEN to call it, and every tool names its cost tier.
    /// This is the one place that checks all of them, so a new tool cannot
    /// arrive with a description that only says what it does.
    #[test]
    fn every_tool_description_states_when_to_call_it_and_its_cost_tier() {
        let tools = PlatformTools::tool_router().list_all();
        assert!(!tools.is_empty(), "the router serves no tools");
        for tool in &tools {
            let d = tool.description.as_deref().unwrap_or_default();
            assert!(
                d.contains("WHEN:"),
                "tool `{}` does not say WHEN to call it: {d}",
                tool.name
            );
            assert!(
                d.contains("COST:"),
                "tool `{}` does not name its cost tier: {d}",
                tool.name
            );
        }
    }

    /// ruagent-tunable-capabilities t13: after t10 deleted the CODE copy of the
    /// option key list, this tool's description held the last spelling of it — and
    /// it is the only one nothing parses, so it would go stale the day the registry
    /// declares a different set. It now points at the authority instead
    /// (`options_schema`, served by `capabilities_list`). This test is what makes a
    /// list coming back loud; nothing else parses a description.
    #[test]
    fn the_capability_set_description_points_at_the_schema_instead_of_listing_keys() {
        let tools = PlatformTools::tool_router().list_all();
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == "capability_set")
            .expect("capability_set is served");
        let d = tool.description.as_deref().unwrap_or_default();

        // It names where the answer lives.
        assert!(d.contains("options_schema"), "{d}");
        assert!(d.contains("capabilities_list"), "{d}");

        // ... and it no longer ENUMERATES the keys. `re-weight` is the action verb
        // for the fusion weight, not a key name, so the four names with no prose
        // homograph are checked with `contains`, plus the old list's opening.
        assert!(!d.contains("(weight"), "a key list is back: {d}");
        for key in [
            "min_score",
            "max_per_input",
            "min_confidence",
            "max_docs_per_pass",
        ] {
            assert!(!d.contains(key), "the description enumerates `{key}`: {d}");
        }

        // The metadata change is surgical: the WHEN/COST gate, the cost flag and the
        // read-modify-write paragraph are all still there.
        for kept in [
            "WHEN:",
            "COST:",
            "confirm_cost",
            "policy.toml",
            "replaces the whole [capabilities] table",
            "conflicts[]",
        ] {
            assert!(d.contains(kept), "the description lost `{kept}`: {d}");
        }
    }

    /// The cheap/expensive choice must be visible from the server's own
    /// instructions too, not only from the per-tool descriptions.
    #[test]
    fn the_server_instructions_point_at_the_capability_plane() {
        let info = PlatformTools::new(BridgeConfig {
            daemon_url: "http://127.0.0.1:8787".into(),
        })
        .get_info();
        let instructions = info.instructions.unwrap_or_default();
        assert!(instructions.contains("capabilities_list"), "{instructions}");
        assert!(instructions.contains("capability_set"), "{instructions}");
        assert!(instructions.contains("confirm_cost"), "{instructions}");
        assert!(instructions.contains("free"), "{instructions}");
        assert!(instructions.contains("llm"), "{instructions}");
    }
}
