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
        description = "Search the user's long-term memories (facts, preferences, lessons learned across sessions). WHEN: before asking the user something they may have already told you, at the start of a task, or when a preference would change your approach. COST: free (zero model tokens). For ONE pass across memory + knowledge + wiki + graph, use memory_recall instead. A CUT IS COUNTED AND NAMED: memory search is a page (limit, default 8, max 200) and the daemon reports how many rows matched BEFORE the page, so the reply ends with the remainder (还有 N 条未显示) -- and when that count is a floor rather than the whole answer it says AT LEAST instead of dressing a lower bound as a total."
    )]
    async fn memory_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/memory/search", self.config.daemon_url);
        // THE WINDOW IS SENT EXPLICITLY, with the daemon's own default (8) as this
        // tool's default: a request that names its page size is a reading, and the
        // reply's own counts then say whether that page was cut (issue #54).
        let window = params.limit.unwrap_or(8);
        let resp = json_of(
            self.http
                .get(&url)
                .query(&[("q", &params.query), ("limit", &window.to_string())]),
            "memory_search",
            &self.config.daemon_url,
        )
        .await?;
        memory_search_to_text(&resp)
    }

    #[tool(
        // THE DESCRIPTION DOES NOT RESTATE THE WRITE VOCABULARY (issue #56, t2). It
        // used to say "Namespaces: user, global, project:<name>, agent:<name>", which
        // reads as *every* namespace being writable everywhere -- but each store has
        // its own write vocabulary (`MemoryStore::allowed_kinds`, design 6.2), so that
        // sentence was a SECOND copy of a matrix the daemon already owns, and it was
        // wrong. The tool now states the RULE (the daemon adjudicates, a pair outside
        // the vocabulary FAILS) and points at the refusal, which names the store, the
        // namespace and the `allowed_namespaces` that store may write.
        description = "Store a long-term memory about the user, a project, or a lesson learned. Stores: profile (user facts), observation (general), procedure (how-to), lesson (corrections and lessons worth keeping). THE DAEMON ADJUDICATES THE store x namespace PAIR: each store writes only some of user | global | project:<name> | agent:<name>, and a pair outside that store's write vocabulary FAILS -- nothing is written, and the refusal names the store, the namespace and the allowed_namespaces that store may write. A write is stored only when THIS call returns a receipt; never report a refused call as remembered. Optionally pass supersedes=<memory id> to replace it. WHEN: the user asks you to remember something, or states a durable preference, decision or correction worth keeping beyond this session. COST: free (zero model tokens)."
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
        // A REFUSED PAIR NEVER REACHES THIS LINE (issue #56, t1): the daemon answers
        // 409 with a JSON body, and `json_of` turns any non-2xx into an `Err` whose
        // message IS the daemon's own sentence -- so a refusal is what the caller
        // sees, never "memory write: RejectedNamespace(...)" dressed as a receipt.
        //
        // A 2xx, on the other hand, IS a write, and that envelope always carries
        // `outcome` (the route's branch table, crates/daemon/src/api.rs). A success
        // reply WITHOUT it is protocol DRIFT, not an empty reading: `"?"` here would
        // read as a stored memory, which is the false success this tool must never
        // produce (the `drift` discipline, t93).
        Ok(format!(
            "memory write: {}",
            field_str("memory_write", &resp, "outcome")?
        ))
    }

    #[tool(
        description = "Unified recall across long-term memories, knowledge base, generated wiki pages and the knowledge graph — ONE pass over all four, so prefer it over four separate searches when the question spans the user's facts and their documents. WHEN: at the start of a session or task that needs context, or when a question may be answered by memories, documents or entity relations. COST: free (zero model tokens; every leg is a local index query, no model is called). STRATEGY: strategy=aggressive (default) returns full content above the relevance threshold; strategy=conservative returns stubs only (titles, entity names, relation one-liners) — cheap to scan, then fetch what you need with memory_get / knowledge_expand / graph_entity. conservative=true is the older spelling of strategy=conservative; passing both with disagreeing values is refused, not silently resolved. PER-LEG CONTROL: the six retrieval legs are switched and weighted through the capability plane, not through this tool — capability_set(id=recall_leg_wiki, enabled=false) drops a leg. Their ids are recall_leg_memory_semantic, recall_leg_memory_fts, recall_leg_knowledge_semantic, recall_leg_knowledge_fts, recall_leg_wiki, recall_leg_graph (capabilities_list shows which are off). The reply carries legs_disabled, naming the legs this call skipped, so a thin result is explained rather than guessed at. Wiki pages are AGENT-GENERATED: verify against sources before trusting them. IT SAYS WHEN IT WAS CUT: every section here is a WINDOW (top_n, no offset), and a reply that reached one ends with an INCOMPLETE line naming which section was cut and -- for memories -- 还有 N 条未显示, so a thin answer is never mistaken for a complete one."
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
        recall_to_text(&resp)
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
        description = "Search the knowledge base of ingested documents (hybrid semantic + keyword search). WHEN: the answer should come from the user's documents, notes or code — memory holds facts, knowledge holds documents. COST: free (zero model tokens). THE CUT IS COUNTED: this route publishes how many hits matched before its window (matched), and the reply ends with 'matched M, this reply carries R' plus 还有 N 条未显示 when rows were left out."
    )]
    async fn knowledge_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let url = format!("{}/api/v1/knowledge/search", self.config.daemon_url);
        let limit = params.limit.unwrap_or(8);
        let resp = json_of(
            self.http
                .get(&url)
                .query(&[("q", &params.query), ("limit", &limit.to_string())]),
            "knowledge_search",
            &self.config.daemon_url,
        )
        .await?;
        knowledge_hits_to_text(&resp)
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
        // M1 (first half, repaired in t118): "this id is not in the graph" must not
        // read the same as "this entity exists with no current facts". The daemon
        // DOES carry that distinction -- `name: null` means the id is absent -- and
        // always did; the signal, its coordinates and the daemon test that pins it
        // are documented on [`entity_exists`]. What used to be here read
        // `resp["entity"]` / `resp["exists"]`, two keys the daemon has never emitted,
        // so the missing branch could not fire and a fabricated id came back as a
        // successful "has no current facts" -- a false success the caller acts on.
        if !entity_exists(&resp).map_err(|e| rmcp::ErrorData::internal_error(e, None))? {
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
        description = "Search the knowledge graph's entities by name/summary (a flat name search, NOT multi-hop). WHEN: you want candidate entities by name; use graph_retrieve when the question needs a path between things. COST: free (zero model tokens). PAGED AND COUNTED: limit is honoured (default 10, max 200) and the reply states how many entities matched before the page, with 还有 N 条未显示 when rows were left out (AT LEAST N when the route could not count to the end). Read match FIRST: candidate means the LOOSE widening pass answered, which can match a name that does not exist."
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
        graph_search_to_text(&resp)
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
        description = "Distill ONE session now: extract durable memories (and, when the distiller is configured for it, entities/relations) from that session's transcript. WHEN: the user says to remember a session or distill a conversation, or a session ended with something worth keeping that is not in memory yet. COST: extractor=rules is FREE -- zero model tokens, deterministic, no agent is spawned; extractor=acp (the default) SPENDS MODEL TOKENS -- it runs a full extraction-agent turn over the transcript; extractor=both runs rules and then acp. Pass dry_run=true first: it reports what would be written and writes nothing. This manual call is an explicit request and is never blocked by the distill_session capability -- that capability gates only the unattended session-close path. This tool only TRIGGERS; to ask whether a session is ALREADY distilled and what its last attempt did, call distill_status (free -- it reads the ledger and never calls a model)."
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

    // -- facts, the distill ledger, and the pending queue (t12: #51, #54) ---
    //
    // THREE tools, each a thin proxy to an EXISTING daemon route (this file's own
    // rule: do not add a write path, add a tool). What is deliberately NOT here is
    // written down once, in the non-goals section of
    // docs/plans/capability-plugins-design.md §14.4: community discovery and
    // community-summary writes, and any LLM adjudication of a merge, are NOT
    // exposed through MCP.

    #[tool(
        description = "Write ONE fact between two EXISTING graph entities: src --relation--> dst, with the fact's own text. WHEN: the user states a durable relation the graph should hold (\"atlas ships on Fridays\", \"Dana owns the rollback\"), or you are completing/correcting the graph by hand -- knowledge_graph_ingest is how a DOCUMENT feeds the graph, this is how ONE fact does. COST: free (zero model tokens), but it WRITES one edge. Address each end by id (src/dst) or by name (src_name/dst_name), exactly ONE form per end: a name that resolves to nothing is REFUSED with the name quoted (this route never invents an entity -- create it first, or pass the id), and sending both forms for one end is refused too. THE TIME CONTRACT (#49): every write through this seam is event_time_source=recorded -- a tool call is not a transcript, so it can never claim the event time came out of the text; valid_at omitted means now. A refusal names the field and the value, so a typo cannot look like a successful write."
    )]
    async fn graph_fact_add(
        &self,
        Parameters(params): Parameters<FactAddParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let what = "graph_fact_add";
        let url = format!("{}/api/v1/graph/fact", self.config.daemon_url);
        // ONLY THE KEYS THE CALLER SET. An absent end or an absent valid_at must
        // reach the daemon as ABSENT: the route owns the "exactly one of id/name"
        // rule and its own message for a name that resolves to nothing, and a
        // second copy of that rule here would be the duplicate implementation this
        // bridge exists to avoid.
        let mut body = serde_json::Map::new();
        body.insert("relation".into(), serde_json::json!(params.relation));
        body.insert("fact_text".into(), serde_json::json!(params.fact_text));
        for (key, value) in [
            ("src", params.src.map(serde_json::Value::from)),
            ("dst", params.dst.map(serde_json::Value::from)),
            (
                "src_name",
                params.src_name.clone().map(serde_json::Value::from),
            ),
            (
                "dst_name",
                params.dst_name.clone().map(serde_json::Value::from),
            ),
            (
                "valid_at",
                params.valid_at.clone().map(serde_json::Value::from),
            ),
        ] {
            if let Some(value) = value {
                body.insert(key.to_string(), value);
            }
        }
        let resp = json_of(
            self.http.post(&url).json(&serde_json::Value::Object(body)),
            what,
            &self.config.daemon_url,
        )
        .await?;
        fact_added_to_text(&resp, &params.relation, params.valid_at.as_deref())
    }

    #[tool(
        description = "Is THIS session distilled, and what did its LAST attempt do? Reads the distillation ledger (one row per attempt: outcome, failure reason, what it wrote, how the transcript was cut, what it cost). WHEN: the user asks whether a conversation has been distilled or why it produced nothing; BEFORE re-distilling a session (with extractor=acp a repeat is another model call); or when a 'distilled' badge disagrees with what you expect. COST: free (zero model tokens) -- this only reads the ledger, it never distils. TWO SEPARATE QUESTIONS, both answered: `distilled` is YES/NO over the session's WHOLE history (any attempt whose outcome was ok), while the newest-attempt block is the LATEST row, which may be a later failure. NULL MEANS NOT RECORDED, NEVER 0: tokens_used / cost_usd / windows / truncated / retries are null when nobody measured them (every pre-0028 row), and the last two must be read TOGETHER -- truncated=true with windows=1 is a LOSSY cut that dropped the transcript's tail, while windows=3 with truncated=false lost nothing. To distil it now call distill_session (extractor=rules is free)."
    )]
    async fn distill_status(
        &self,
        Parameters(params): Parameters<DistillStatusParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let what = "distill_status";
        // The same argument check `distill_session` makes, for the same reason: a
        // `/` cannot be in a session key, and a URL built from one would ask the
        // daemon about a session that cannot exist.
        if params.session_key.is_empty() || params.session_key.contains('/') {
            return Err(rmcp::ErrorData::internal_error(
                format!(
                    "distill_status: session_key `{}` is not a session key -- it must be non-empty and contain no `/`; the keys look like `ruagent:<hex>` and come from the sessions listing",
                    params.session_key
                ),
                None,
            ));
        }
        let limit = params.limit.unwrap_or(10).clamp(1, 100);
        let url = format!("{}/api/v1/distill/log", self.config.daemon_url);
        let resp = json_of(
            self.http.get(&url).query(&[
                ("session", &params.session_key),
                ("limit", &limit.to_string()),
            ]),
            what,
            &self.config.daemon_url,
        )
        .await?;
        distill_status_to_text(&resp, &params.session_key)
    }

    #[tool(
        description = "The entity pairs waiting for a MERGE DECISION -- and the place the decision is recorded. A mechanical pass queues pairs that look like the same object under two rows; each pair carries both entities and how many CURRENT facts each has, so the decision can be made from this reply. WHEN: action=list (the default) when the user asks whether two entities should be merged, or the graph looks like one object under two names; action=merge when the user decides the two ARE the same entity (the keeper absorbs the other: its edges move over and its name becomes an alias of the keeper, so the next pass does not rebuild it); action=dismiss when the user decides they STAY SEPARATE -- remembered, so a later mechanical pass cannot re-queue the pair. COST: free (zero model tokens) for all three; merge and dismiss WRITE. MERGING IS EXPLICIT AND NEVER AUTOMATIC -- no model decides it on its own, and a dismissal is not a lock (merging a dismissed pair is still allowed). Decisions are IDEMPOTENT: repeating a dismissal reports already_dismissed instead of an error, and a pair that was never queued is refused rather than answered 'done'. The list is the WHOLE queue (that route applies no limit), so nothing is hidden here; pending_count is the queue's size and dismissed_count says how many decisions are already remembered."
    )]
    async fn graph_resolution_pending(
        &self,
        Parameters(params): Parameters<ResolutionParams>,
    ) -> Result<String, rmcp::ErrorData> {
        let what = "graph_resolution_pending";
        let base = &self.config.daemon_url;
        match params.action.as_deref() {
            None | Some("list") => {
                let url = format!("{base}/api/v1/graph/resolution/pending");
                let resp = json_of(self.http.get(&url), what, base).await?;
                pending_to_text(&resp, params.show_redundant.unwrap_or(false))
            }
            Some("merge") => {
                // REFUSED HERE, not sent as a body the daemon's type cannot
                // deserialize: the ids ARE what the action means, so a merge without
                // them is an argument error and gets a sentence naming the action.
                // (The daemon still owns every SEMANTIC rule: the same id twice, a
                // missing entity, how many edges move.)
                let (Some(keeper), Some(absorbed)) = (params.keeper, params.absorbed) else {
                    return Err(rmcp::ErrorData::internal_error(
                        "graph_resolution_pending: action=\"merge\" needs BOTH `keeper` (the entity that survives) and `absorbed` (the one merged away), as ids from the pending list"
                            .to_string(),
                        None,
                    ));
                };
                let url = format!("{base}/api/v1/graph/resolution/merge");
                let resp = json_of(
                    self.http
                        .post(&url)
                        .json(&serde_json::json!({ "keeper": keeper, "absorbed": absorbed })),
                    what,
                    base,
                )
                .await?;
                merge_result_to_text(&resp)
            }
            Some("dismiss") => {
                let (Some(a), Some(b)) = (params.a, params.b) else {
                    return Err(rmcp::ErrorData::internal_error(
                        "graph_resolution_pending: action=\"dismiss\" needs BOTH `a` and `b` (the two entity ids from the pending list)"
                            .to_string(),
                        None,
                    ));
                };
                let url = format!("{base}/api/v1/graph/resolution/dismiss");
                let resp = json_of(
                    self.http
                        .post(&url)
                        .json(&serde_json::json!({ "a": a, "b": b })),
                    what,
                    base,
                )
                .await?;
                dismiss_result_to_text(&resp)
            }
            Some(other) => Err(rmcp::ErrorData::internal_error(
                format!(
                    "graph_resolution_pending: action `{other}` is not an action: use \"list\" (the default -- read the queue), \"merge\" (keeper absorbs absorbed) or \"dismiss\" (these two stay separate)"
                ),
                None,
            )),
        }
    }

    // -- platform ops (the symmetric design, §7.2) -----------------------

    #[tool(
        description = "List the platform's tasks (durable work items) with their status. Optionally filter by status: pending|in_progress|blocked|done|cancelled. WHEN: you need the shared board before creating or picking up durable work, or the user asks what is in flight. COST: free (zero model tokens). PAGED, AND ITS CUT IS STATED: limit (default 20) and offset walk the daemon's list envelope (total/limit/offset/truncated), and the reply ENDS with the counts -- shown N of T (limit, offset) plus 还有 N 条未显示 -- so a page that was cut can never read as a complete board."
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
        // THE PAGE SIZE IS SENT AND NAMED (t12 / issue #54). This used to print the
        // first 20 rows of whatever came back and say nothing about the rest, so a
        // board with 40 rows and one with 20 were the same reading. limit keeps
        // today's 20 as the default; the envelope reports the value the daemon
        // actually applied, and tasks_to_text prints the remainder.
        let limit = params.limit.unwrap_or(20);
        let offset = params.offset.unwrap_or(0);
        let req = req
            .query(&[("limit", limit.to_string())])
            .query(&[("offset", offset.to_string())]);
        let resp = json_of(req, "list_tasks", &self.config.daemon_url).await?;
        tasks_to_text(&resp)
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

/// The JSON type of a value, for drift messages that name what actually arrived.
fn json_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Whether the daemon's `/api/v1/graph/entity/{id}` response says the entity
/// EXISTS. `Ok(false)` means "that id is not in the graph" -- `Ok(true)` means the
/// row is there, whether or not it has any facts.
///
/// THE WHOLE SIGNAL IS ONE KEY: **`name`, and the value that means absent is
/// `null`.** The daemon builds it in `async fn graph_entity` as
/// `entity.as_ref().map(|e| e.name.clone())` (read 2026-10-04: `api.rs:1361`), where
/// `entity` is `ruagent_graph::entity_by_id(db, id).await?` (same read: `api.rs:1355`)
/// -- so `null` is exactly "no row for this id", while any string (an empty one
/// included) is a row that is there. The daemon does that on purpose and says so at
/// `api.rs:1358-1365`: an id that is not in the graph keeps a **200 with empty
/// collections, never a 404**, so the status code carries no existence information
/// and this key is the only carrier. It is pinned on the daemon side by the test
/// `the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive` (same
/// read: `api.rs:6711`) -- HTTP 200 for an id that is not in the graph (`api.rs:6783`),
/// `name.is_null()` for it (`api.rs:6784`), `facts == []` for it (`api.rs:6785-6788`).
/// THESE NUMBERS DRIFT -- a concurrent unit edits that file, and during this very
/// unit they moved by 19 lines. Resolve by SYMBOL (`graph_entity`, `entity_by_id`,
/// the four-key response literal, that test name); each number above is a hint with
/// the date it was read -- all seven were re-read on 2026-10-04, which is where the
/// two that had drifted by 19 (`1341-1346`, `1342`) were corrected to
/// `1360-1365` / `1361`.
///
/// STRICT ON PURPOSE, in the same spirit as [`facts_of`]: a response with no `name`
/// key, or a `name` that is neither a string nor `null`, is an ERROR. A renamed or
/// retyped field must not silently read as "the entity does not exist" *or* as "it
/// exists" -- that silent absorption is the very thing this replaces.
///
/// WHAT IT REPLACES (ruagent-tunable-capabilities t118, the first half of t93's M1):
/// the branch here used to read `resp["entity"]` and fall back to `resp["exists"]`.
/// The daemon has never emitted either key (the response literal is four keys:
/// `name`, `kind`, `aliases`, `facts` -- `api.rs:1360-1365`), so the predicate was
/// `false` for EVERY response, including one served for a fabricated id. Both
/// branches are deleted rather than kept unreachable: a branch that can never fire
/// is not a safety net, it is a false green waiting to be cited.
pub fn entity_exists(resp: &serde_json::Value) -> Result<bool, String> {
    match resp.get("name") {
        Some(v) if v.is_null() => Ok(false),
        Some(v) if v.is_string() => Ok(true),
        Some(other) => Err(format!(
            "daemon response has `name` of type {} (expected a string, or null for an \
             id that is not in the graph), so the entity-existence signal is gone: {resp}",
            json_kind(other)
        )),
        None => Err(format!(
            "daemon response carries no `name` key, and `name` is the entity-existence \
             signal (`name: null` = that id is not in the graph): {resp}"
        )),
    }
}

// ---------------------------------------------------------------------------
// t12 renderers: the fact write, the distill ledger, the pending queue, and the
// "this list was cut" note every list tool ends with
// ---------------------------------------------------------------------------

/// The currency mark, spelled once: a money reading that omitted it (or spelled it
/// twice) would be a second answer to "what does this number mean".
const MONEY: char = '$';

fn field_i64(what: &str, obj: &serde_json::Value, key: &str) -> Result<i64, rmcp::ErrorData> {
    obj.get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| drift(what, key, obj))
}

fn field_array<'a>(
    what: &str,
    obj: &'a serde_json::Value,
    key: &str,
) -> Result<&'a Vec<serde_json::Value>, rmcp::ErrorData> {
    obj.get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| drift(what, key, obj))
}

/// A RECORDED number, or the honest alternative (the 0028 rule, issue #55).
///
/// NULL MEANS NOBODY RECORDED IT, and printing 0 for that would turn "we did not
/// measure this" into "this was zero" -- the one reading 0028's columns exist to
/// prevent. Every nullable ledger column goes through here or through
/// [`reading_cost`], so the rule is applied once instead of once per field.
fn reading_int(value: &serde_json::Value, absent: &str) -> String {
    value
        .as_i64()
        .map(|n| n.to_string())
        .unwrap_or_else(|| absent.to_string())
}

fn reading_cost(value: &serde_json::Value, absent: &str) -> String {
    value
        .as_f64()
        .map(|c| format!("{MONEY}{c:.4}"))
        .unwrap_or_else(|| absent.to_string())
}

/// WHAT A WINDOW MEASUREMENT SAYS, with `windows` and `truncated` READ TOGETHER.
///
/// They are two halves of one reading and are useless apart: a lossy single-window
/// cut (windows=1, truncated=true) dropped the transcript's tail, while three
/// windows with truncated=false lost nothing at all. A row that measured neither
/// says so instead of reading as "one window, nothing lost".
fn window_reading(row: &serde_json::Value) -> String {
    let windows = row.get("windows").and_then(|v| v.as_i64());
    let truncated = row.get("truncated").and_then(|v| v.as_bool());
    match (windows, truncated) {
        (Some(1), Some(true)) => {
            "windows=1, truncated=YES -- the transcript was cut to one window and its TAIL WAS \
             DROPPED"
                .to_string()
        }
        (Some(1), Some(false)) => {
            "windows=1, truncated=no -- the whole transcript fitted one window, nothing was lost"
                .to_string()
        }
        (Some(n), Some(true)) => format!(
            "windows={n}, truncated=YES -- one of the {n} windows was cut, so text was DROPPED"
        ),
        (Some(n), Some(false)) => {
            format!("windows={n}, truncated=no -- {n} windows at turn boundaries, nothing was lost")
        }
        (Some(n), None) => format!(
            "windows={n}, truncated=NOT RECORDED -- the window count is there but whether a cut \
             lost text is not"
        ),
        (None, Some(true)) => "windows=NOT RECORDED, truncated=YES -- text was dropped, but how \
                               many windows it took is not recorded"
            .to_string(),
        (None, Some(false)) => "windows=NOT RECORDED, truncated=no".to_string(),
        (None, None) => "windows/truncated NOT RECORDED -- this attempt measured neither (every \
                         pre-0028 row does)"
            .to_string(),
    }
}

/// HOW MANY ROWS AN ANSWER LEFT OUT (issue #54's third acceptance).
///
/// EVERY list tool ends with this line, because a list that was cut and a list that
/// ended are otherwise the same reading: `shown` is where this page started plus what
/// it carried, and the remainder is `total - shown`. The count is printed even when
/// the list is complete ("shown 25 of 25"), so "nothing is missing" is a fact in the
/// reply rather than an inference from the absence of a warning.
fn unshown_note(shown: u64, total: u64, limit: Option<u64>, offset: Option<u64>) -> String {
    let after = total.saturating_sub(shown);
    // THE LABELS COME FROM WHERE THE NUMBERS CAME FROM, and the arithmetic is this
    // one subtraction either way: an envelope route publishes total/limit/offset,
    // while a search leg publishes its own match count and no page bounds.
    let mut line = match (limit, offset) {
        (Some(limit), Some(offset)) => {
            format!("shown {shown} of {total} (limit={limit}, offset={offset})")
        }
        _ => format!("matched {total}, this reply carries {shown}"),
    };
    if after > 0 {
        // THE HINT NAMES ONLY WHAT THE CALLER CAN DO (t26): a search leg has no offset,
        // so "ask for offset=N" would be advice its reader cannot follow.
        line.push_str(&format!(
            "; 还有 {after} 条未显示 ({after} row(s) left -- raise limit{})",
            match offset {
                Some(offset) => format!(" or ask for offset={}", offset + shown),
                None => String::new(),
            }
        ));
    }
    if let Some(offset) = offset.filter(|o| *o > 0) {
        line.push_str(&format!(
            "; {offset} earlier row(s) not shown (this page starts at offset {offset})"
        ));
    }
    format!("\n{line}\n")
}

/// The report of ONE fact write, plus the two things an agent needs next: where to
/// read it back, and what the write's event time MEANS (#49).
fn fact_added_to_text(
    resp: &serde_json::Value,
    relation: &str,
    valid_at: Option<&str>,
) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "graph_fact_add";
    let id = field_i64(WHAT, resp, "id")?;
    let src = field_i64(WHAT, resp, "src")?;
    let dst = field_i64(WHAT, resp, "dst")?;
    let source = field_str(WHAT, resp, "event_time_source")?;
    let when = match valid_at {
        Some(v) => format!("valid_at={v}, as you passed it (the route does not echo it back)"),
        None => "valid_at omitted, so the daemon recorded NOW as the event time".to_string(),
    };
    let mut out = format!(
        "fact #{id} written: entity {src} -[{relation}]-> entity {dst}\n\
         READ IT BACK with graph_entity(id: {src}) (or {dst}) -- this session sees the write \
         immediately.\n\
         EVENT TIME CONTRACT: event_time_source={source} -- a write through this tool is a \
         RECORDED time ({when}). A tool call is not a transcript, so it cannot claim the time \
         came OUT of the text; only the extraction path writes \"extracted\", and only when the \
         transcript itself carries the time. A null in that column would mean the writer did not \
         record one.\n"
    );
    if src == dst {
        out.push_str(
            "NOTE: both ends resolved to the SAME entity -- the edge is a self-relation. If you \
             meant two entities, one of the names matched the wrong row.\n",
        );
    }
    Ok(out)
}

/// WHAT THE LEDGER KNOWS ABOUT ONE SESSION (issue #46), from
/// `GET /api/v1/distill/log?session=`.
///
/// TWO QUESTIONS, TWO ANSWERS, and they are not the same question: `distilled` is
/// "did ANY attempt on this session end ok", taken from the route's `by_status`
/// distribution -- which counts the session's WHOLE history, not the page -- while the
/// "newest attempt" block describes the LATEST row, which may well be a later failure.
/// A reader that took the newest row for the verdict would call a session undistilled
/// because its last retry failed (or distilled because its older attempt succeeded).
fn distill_status_to_text(
    resp: &serde_json::Value,
    session_key: &str,
) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "distill_status";
    let entries = field_array(WHAT, resp, "entries")?;
    let total = field_u64(WHAT, resp, "total")?;
    let limit = field_u64(WHAT, resp, "limit")?;
    let offset = field_u64(WHAT, resp, "offset")?;
    let by_status = resp
        .get("by_status")
        .and_then(|v| v.as_object())
        .ok_or_else(|| drift(WHAT, "by_status", resp))?;
    let totals = resp
        .get("totals")
        .and_then(|v| v.as_object())
        .ok_or_else(|| drift(WHAT, "totals", resp))?;
    // A bucket the daemon did not send is drift: the route always emits all four, and
    // a default of 0 here would turn "the shape changed" into "no successful attempt".
    let bucket = |name: &str| -> Result<u64, rmcp::ErrorData> {
        by_status
            .get(name)
            .and_then(|v| v.as_u64())
            .ok_or_else(|| drift(WHAT, &format!("by_status.{name}"), resp))
    };
    let ok = bucket("ok")?;
    let empty = bucket("empty")?;
    let failed = bucket("failed")?;
    let not_recorded = bucket("not_recorded")?;

    let mut out = if ok > 0 {
        format!("session {session_key}: DISTILLED ({ok} attempt(s) recorded an ok outcome).\n")
    } else {
        format!("session {session_key}: NOT DISTILLED (no attempt recorded an ok outcome).\n")
    };

    match entries.first() {
        None => out.push_str(
            "no attempt recorded for this session: the ledger holds NO row, so nothing recorded \
             a failure reason either. A refusal or an empty transcript DOES write a row (t4), so \
             'no row' means no attempt ran here at all -- or the session is older than the \
             ledger.\n",
        ),
        Some(latest) => out.push_str(&attempt_block(latest)?),
    }
    // The pointer rides EVERY undistilled answer, the "no row at all" case included:
    // that is the case where an agent most needs to know how to get one, and it is
    // how distill_status covers the trigger half of this surface without a second,
    // redundant trigger tool.
    if ok == 0 {
        out.push_str(
            "To distil it now: distill_session(session_key: \"<key>\", extractor: \"rules\") is \
             FREE (zero model tokens); extractor=\"acp\" spends one model call per transcript \
             window.\n",
        );
    }

    out.push_str(&format!(
        "attempts recorded for this session: {total} (ok={ok} empty={empty} failed={failed} \
         not_recorded={not_recorded})\n"
    ));
    // READ BEFORE THE EMPTY-HISTORY BRANCH: whether the totals carry their unrecorded
    // COUNTS is a shape question, and a check that only ran for a session with rows
    // would let a renamed key through on the empty case -- the case where the reader
    // has the least to cross-check it against.
    let tokens_missing = totals
        .get("tokens_used_not_recorded")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| drift(WHAT, "totals.tokens_used_not_recorded", resp))?;
    let cost_missing = totals
        .get("cost_usd_not_recorded")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| drift(WHAT, "totals.cost_usd_not_recorded", resp))?;
    out.push_str("cumulative over the WHOLE history (not only the page above):\n");
    if total == 0 {
        // A sum with nothing to add is not a zero. "0 of 0 attempts did not record
        // one" would be arithmetic over an empty set dressed as a reading.
        out.push_str(
            "  no attempt is recorded, so there is nothing to sum: tokens and cost are \
             unanswerable for this session (not 0).\n",
        );
        return Ok(format!(
            "{out}{}",
            unshown_note(
                offset + entries.len() as u64,
                total,
                Some(limit),
                Some(offset)
            )
        ));
    }
    out.push_str(&format!(
        "  tokens {} ({} of {total} attempt(s) did not record one)\n",
        reading_int(
            totals
                .get("tokens_used")
                .unwrap_or(&serde_json::Value::Null),
            "not recorded (nobody measured it)"
        ),
        tokens_missing
    ));
    out.push_str(&format!(
        "  cost   {} ({} of {total} attempt(s) did not record one)\n",
        reading_cost(
            totals.get("cost_usd").unwrap_or(&serde_json::Value::Null),
            "not recorded (no provider priced these attempts)"
        ),
        cost_missing
    ));
    out.push_str(&unshown_note(
        offset + entries.len() as u64,
        total,
        Some(limit),
        Some(offset),
    ));
    Ok(out)
}

/// ONE attempt, read from its ledger row. Every nullable field says "not recorded"
/// rather than 0, and the transcript reading keeps `windows` beside `truncated`.
fn attempt_block(row: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "distill_status";
    let id = field_i64(WHAT, row, "id")?;
    let at = field_str(WHAT, row, "distilled_at")?;
    let status = row
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("not_recorded (NULL status: nobody recorded an outcome for this attempt)");
    let agent = row
        .get("agent")
        .and_then(|v| v.as_str())
        .filter(|a| !a.trim().is_empty())
        .unwrap_or("none (no agent was spawned)");
    let mut out = format!(
        "  newest attempt #{id} at {at}: status={status} (agent {agent})\n  wrote: {m} memories, {e} entities, {r} relations (already stated {dup}, refused {ref})\n  usage: tokens {tok} (context {ctx}), cost {cost}, retries {retries}\n  transcript: {windows}\n",
        m = reading_int(row.get("memories_written").unwrap_or(&serde_json::Value::Null), "not recorded"),
        e = reading_int(row.get("entities_written").unwrap_or(&serde_json::Value::Null), "not recorded"),
        r = reading_int(row.get("relations_written").unwrap_or(&serde_json::Value::Null), "not recorded"),
        dup = reading_int(row.get("relations_duplicate").unwrap_or(&serde_json::Value::Null), "not reported"),
        ref = reading_int(row.get("relations_refused").unwrap_or(&serde_json::Value::Null), "not reported"),
        tok = reading_int(row.get("tokens_used").unwrap_or(&serde_json::Value::Null), "NOT RECORDED"),
        ctx = reading_int(row.get("context_size").unwrap_or(&serde_json::Value::Null), "not recorded"),
        cost = reading_cost(row.get("cost_usd").unwrap_or(&serde_json::Value::Null), "NOT RECORDED"),
        retries = reading_int(row.get("retries").unwrap_or(&serde_json::Value::Null), "NOT RECORDED (predates the retry path)"),
        windows = window_reading(row),
    );
    if let Some(reason) = row
        .get("failure_reason")
        .and_then(|v| v.as_str())
        .filter(|r| !r.trim().is_empty())
    {
        out.push_str(&format!("  failure reason: {reason}\n"));
    }
    Ok(out)
}

/// THE PENDING QUEUE, AS A DECISION SHEET (issue #50, read through MCP).
///
/// Each pair prints BOTH entities and both CURRENT-fact counts, so the decision can
/// be made from the reply alone -- the daemon's own counts_note says what those
/// counts mean, and it is printed rather than paraphrased (a paraphrase would be a
/// second definition of the same number).
fn pending_to_text(
    resp: &serde_json::Value,
    show_redundant: bool,
) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "graph_resolution_pending";
    let pending = field_array(WHAT, resp, "pending")?;
    let redundant = field_array(WHAT, resp, "redundant")?;
    let pending_count = field_u64(WHAT, resp, "pending_count")?;
    let redundant_count = field_u64(WHAT, resp, "redundant_count")?;
    let dismissed_count = field_u64(WHAT, resp, "dismissed_count")?;
    let counts_note = field_str(WHAT, resp, "counts_note")?;

    let mut out = format!(
        "merge decisions waiting: {pending_count} pair(s); {dismissed_count} pair(s) already \
         answered (these two stay separate, remembered); {redundant_count} pair(s) are \
         ALREADY the same object under two rows (no decision left to make -- merge them).\n\
         counts: {counts_note}\n"
    );
    if pending.is_empty() {
        out.push_str(if dismissed_count > 0 {
            "nothing to decide: the queue is empty because every pair was worked through (see \
             dismissed_count).\n"
        } else {
            "nothing to decide: nothing is queued and nothing has been dismissed.\n"
        });
    }

    for (i, pair) in pending.iter().enumerate() {
        let a = pair
            .get("a")
            .ok_or_else(|| drift(WHAT, "pending[].a", pair))?;
        let b = pair
            .get("b")
            .ok_or_else(|| drift(WHAT, "pending[].b", pair))?;
        let a_id = field_i64(WHAT, a, "id")?;
        let b_id = field_i64(WHAT, b, "id")?;
        let a_name = field_str(WHAT, a, "name")?;
        let b_name = field_str(WHAT, b, "name")?;
        let a_kind = a
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("kind not recorded");
        let b_kind = b
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("kind not recorded");
        let a_facts = reading_int(pair.get("a_facts").unwrap_or(&serde_json::Value::Null), "?");
        let b_facts = reading_int(pair.get("b_facts").unwrap_or(&serde_json::Value::Null), "?");
        let reason = pair
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("(no reason recorded)");
        let since = pair
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("an unrecorded time");
        out.push_str(&format!(
            "{n}. #{a_id} {a_name:?} ({a_kind}) has {a_facts} current fact(s)   <->   #{b_id} {b_name:?} ({b_kind}) has {b_facts} current fact(s)\n   queued because: {reason} (since {since})\n   merge: graph_resolution_pending(action: \"merge\", keeper: {a_id}, absorbed: {b_id})\n   apart: graph_resolution_pending(action: \"dismiss\", a: {a_id}, b: {b_id})\n",
            n = i + 1,
        ));
    }

    if show_redundant && !redundant.is_empty() {
        out.push_str("already the same object (merge these -- no judgement left):\n");
        for row in redundant {
            let ids: Vec<String> = row
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_i64().map(|i| i.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let reason = row
                .as_array()
                .and_then(|a| a.get(2))
                .and_then(|v| v.as_str())
                .unwrap_or("(no reason recorded)");
            out.push_str(&format!(
                "  #{} and #{} -- {reason}\n",
                ids.first().map(String::as_str).unwrap_or("?"),
                ids.get(1).map(String::as_str).unwrap_or("?"),
            ));
        }
    } else if !redundant.is_empty() {
        out.push_str(
            "(the redundant pairs are counted above; pass show_redundant: true to print their \
             ids)\n",
        );
    }
    out.push_str(
        "NO LIMIT WAS APPLIED: this route returns the WHOLE queue, so nothing is hidden here \
         (pending_count is the queue's size).\n",
    );
    Ok(out)
}

/// The result of an EXPLICIT merge, from the daemon's own report.
fn merge_result_to_text(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "graph_resolution_pending";
    let keeper = field_i64(WHAT, resp, "keeper")?;
    let absorbed = field_i64(WHAT, resp, "absorbed")?;
    let moved = field_u64(WHAT, resp, "moved")?;
    let alias = field_str(WHAT, resp, "alias")?;
    let alias_written = field_bool(WHAT, resp, "alias_written")?;
    Ok(format!(
        "merged: #{absorbed} was absorbed into #{keeper}; {moved} edge(s) moved over.\n\
         the absorbed name {alias:?} is now an ALIAS of the keeper, which is what stops the next \
         mechanical pass from rebuilding the entity just merged away. alias_written={alias_written} \
         reports THIS second write: the merge itself records the alias in its own transaction, so \
         false means the row was already there (read graph_entity(id: {keeper}) to see the alias \
         list) -- it does NOT mean the alias is missing.\n\
         the pair is gone from the pending queue; MERGING IS EXPLICIT -- nothing merged these two \
         on its own.\n"
    ))
}

/// The result of a dismissal, INCLUDING the idempotent repeat (#50).
fn dismiss_result_to_text(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "graph_resolution_pending";
    let a = field_i64(WHAT, resp, "a")?;
    let b = field_i64(WHAT, resp, "b")?;
    let dismissed = field_bool(WHAT, resp, "dismissed")?;
    let already = field_bool(WHAT, resp, "already_dismissed")?;
    if dismissed {
        Ok(format!(
            "recorded: entities #{a} and #{b} STAY SEPARATE. The queue row is stamped, not \
             deleted, so a later mechanical pass cannot re-queue the pair. Merging them later is \
             still allowed -- this is not a lock.\n"
        ))
    } else if already {
        Ok(format!(
            "already recorded: #{a} and #{b} were dismissed before. The repeat is the same \
             decision twice, not an error (dismissed={dismissed}, already_dismissed={already}).\n"
        ))
    } else {
        Err(drift(
            WHAT,
            "dismissed/already_dismissed (one of them must be true)",
            resp,
        ))
    }
}

/// The task list, PAGED through the daemon's envelope (issue #54).
///
/// WHAT CHANGED AND WHY: this tool used to print only the first 20 rows of the
/// response -- a silent cut, so a board with 40 rows and one with 20 read
/// identically, and the agent could not tell "there are no more" from "I was not
/// shown them". The page size is now NAMED and sent, the envelope's
/// total/limit/offset are read from the daemon, and the reply ends with how many
/// rows are not shown.
fn tasks_to_text(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "list_tasks";
    let tasks = field_array(WHAT, resp, "tasks")?;
    let total = field_u64(WHAT, resp, "total")?;
    let limit = field_u64(WHAT, resp, "limit")?;
    let offset = field_u64(WHAT, resp, "offset")?;
    let truncated = field_bool(WHAT, resp, "truncated")?;
    let mut out = String::new();
    for t in tasks {
        out.push_str(&format!(
            "[{}] {} ({})\n",
            t.get("status").and_then(|v| v.as_str()).unwrap_or("?"),
            t.get("title").and_then(|v| v.as_str()).unwrap_or("?"),
            t.get("id").and_then(|v| v.as_str()).unwrap_or("?"),
        ));
    }
    if out.is_empty() {
        out.push_str("no tasks");
    }
    let shown = offset + tasks.len() as u64;
    out.push_str(&unshown_note(shown, total, Some(limit), Some(offset)));
    // The daemon computes its own flag from the same two numbers this note does
    // (offset + rows returned < total). A disagreement means the rule moved, and the
    // reader must see that rather than a note that quietly contradicts the flag.
    if truncated != (total > shown) {
        out.push_str(&format!(
            "NOTE: the daemon reports truncated={truncated} while the counts say {}. One of the \
             two changed meaning.\n",
            total > shown
        ));
    }
    Ok(out)
}

/// The knowledge hits plus the match count the route publishes (issue #54).
fn knowledge_hits_to_text(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "knowledge_search";
    let hits = field_array(WHAT, resp, "hits")?;
    let matched = field_u64(WHAT, resp, "matched")?;
    let mut out = hits_to_text(resp.get("hits").unwrap_or(&serde_json::Value::Null));
    if !hits.is_empty() {
        out.push_str(&unshown_note(hits.len() as u64, matched, None, None));
    }
    Ok(out)
}

/// The memory-search hits, plus the remainder the route now publishes (#54).
///
/// WHY THIS IS NOT A GUESS ANY MORE (t26): `GET /api/v1/memory/search` used to answer
/// `{hits}` and nothing else -- no match count, no total -- so the bridge could only
/// say that a full page MEANT the remainder was UNKNOWN (t12 read the handler and said
/// exactly that). The route now publishes the pre-limit count (`matched`), whether that
/// count is complete (`matched_exact`) and the cut in t22's vocabulary, so the number
/// printed here is the daemon's reading; `search_remainder_note` owns the wording.
fn memory_search_to_text(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "memory_search";
    let hits = field_array(WHAT, resp, "hits")?;
    let mut out = hits_to_text(resp.get("hits").unwrap_or(&serde_json::Value::Null));
    if !hits.is_empty() {
        out.push_str(&search_remainder_note(WHAT, resp, hits.len() as u64)?);
    }
    Ok(out)
}

/// The graph search's entities, with the route's own counts beside them (#54).
///
/// THE OLD CAVEAT IS GONE, MEASURED RATHER THAN ASSUMED (t26): t12 read `graph_search`
/// in `crates/daemon/src/api.rs` and found a FIXED window of 10 with `q.limit` parsed
/// and never read, plus no match count -- and said so in the tool description rather
/// than pretend. The route now honours `limit` (bounded by its own
/// `SEARCH_PAGE_MAX`) and publishes `matched` / `matched_exact` / `truncated` /
/// `truncated_sections`, so what used to be a hard-coded warning is now the daemon's
/// numbers. `match` still says which pass answered, which is the part a caller must
/// read first: `candidate` comes from the loose widening pass.
fn graph_search_to_text(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "graph_search";
    let entities = field_array(WHAT, resp, "entities")?;
    let matched_kind = field_str(WHAT, resp, "match")?;
    let mut out = serde_json::to_string_pretty(resp).unwrap_or_else(|_| resp.to_string());
    out.push_str(&format!(
        "\nmatch={matched_kind}, {} entity/entities returned.",
        entities.len()
    ));
    if !entities.is_empty() {
        out.push_str(&search_remainder_note(WHAT, resp, entities.len() as u64)?);
    }
    out.push('\n');
    Ok(out)
}

/// THE REMAINDER FOR A SEARCH PAGE, taken from the daemon's own reading (t26 / #54).
///
/// WHY IT IS SHARED: `memory_search` and `graph_search` now publish the same four
/// keys, and two renderers would be two spellings of "there is more" -- the duplication
/// this effort exists to remove. The arithmetic stays in `unshown_note` (the single
/// subtraction); what this adds is the one thing a search page needs beyond it: a count
/// that is only a FLOOR is printed as a floor. That is t25's rule, applied to the one
/// case where the route cannot count to the end of its match set.
fn search_remainder_note(
    what: &'static str,
    resp: &serde_json::Value,
    returned: u64,
) -> Result<String, rmcp::ErrorData> {
    let matched = field_u64(what, resp, "matched")?;
    let exact = field_bool(what, resp, "matched_exact")?;
    let truncated = field_bool(what, resp, "truncated")?;
    // The daemon's flag and its counts answer ONE question, so a disagreement is drift
    // rather than a warning to print: one of them has changed meaning.
    let left = matched.saturating_sub(returned);
    if truncated != (left > 0) {
        return Err(drift(
            what,
            "truncated against matched/returned (they must agree)",
            resp,
        ));
    }
    if exact {
        return Ok(unshown_note(returned, matched, None, None));
    }
    // The floor case: `matched` is what the route's scan REACHED, so the sentence says
    // "at least" in both places and never turns a lower bound into a total.
    Ok(format!(
        "\nmatched AT LEAST {matched}, this reply carries {returned}; 至少还有 {left} 条未显示 ({left} row(s) left, and the route's own scan stopped at its ceiling -- the exact match count is unknown)\n"
    ))
}

/// THE INCOMPLETENESS READING (issue #54) for a WINDOW-shaped answer.
///
/// memory_recall's sections are windows, not pages: they take top_n and no offset,
/// so they cannot carry the envelope the paged routes share (the daemon says the same,
/// api.rs's page doc). What they DO publish is truncated plus truncated_sections --
/// which section was cut -- and the MEMORIES section additionally publishes how many
/// rows the window dropped (memory_legs.dropped_by_top_n). That count is the
/// per-section remainder; for the other sections the reply names the window's own
/// numbers rather than inventing one, and says so.
fn recall_cut_note(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
    const WHAT: &str = "memory_recall";
    let truncated = field_bool(WHAT, resp, "truncated")?;
    let sections: Vec<&str> = resp
        .get("truncated_sections")
        .and_then(|v| v.as_array())
        .ok_or_else(|| drift(WHAT, "truncated_sections", resp))?
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    // NOT TRUNCATED IS A READING TOO, and it is the one this route can make exactly:
    // the daemon sets the flag only when the window was REACHED and rows were left
    // over, so false never has to be read as "maybe".
    if !truncated {
        return Ok(String::new());
    }
    let mut out = format!(
        "INCOMPLETE: the top_n window cut these section(s): {}. That is a WINDOW, not a page (no offset), so the reply cannot say how many more exist beyond what each section publishes:\n",
        sections.join(", ")
    );
    let returned_knowledge = resp
        .get("knowledge")
        .and_then(|v| v.as_array())
        .map(|a| a.len() as u64)
        .unwrap_or(0);
    for section in &sections {
        match *section {
            "memories" => {
                let dropped = resp
                    .get("memory_legs")
                    .and_then(|m| m.get("dropped_by_top_n"))
                    .and_then(|v| v.as_u64());
                match dropped {
                    Some(n) => out.push_str(&format!(
                        " memories: 还有 {n} 条未显示 ({n} row(s) the window dropped). Raise top_n, or narrow the query.\n"
                    )),
                    None => out.push_str(
                        " memories: the window was reached, but how many rows it dropped is not reported by this reply (memory_legs.dropped_by_top_n is absent).\n",
                    ),
                }
            }
            "knowledge" => {
                let candidates = resp
                    .get("scoring")
                    .and_then(|s| s.get("candidates"))
                    .and_then(|v| v.as_u64());
                let window = resp
                    .get("scoring")
                    .and_then(|s| s.get("leg_window"))
                    .and_then(|v| v.as_u64());
                out.push_str(&format!(
                    " knowledge: this reply carries {returned_knowledge} hit(s); the leg scored {} candidate(s) in a window of {} (scoring.candidates / scoring.leg_window). The candidates not returned are what the window or the relevance floor left out; raise top_n to see more.\n",
                    candidates
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "an unreported number of".to_string()),
                    window
                        .map(|w| w.to_string())
                        .unwrap_or_else(|| "an unreported".to_string()),
                ));
            }
            other => out.push_str(&format!(
                " {other}: cut by the window; this reply publishes no count for how many rows were left out.\n"
            )),
        }
    }
    Ok(out)
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
             base (free, and off by default). graph_fact_add writes ONE relation \
             between two existing entities and reports its event time as RECORDED \
             (only the extraction path can claim a time out of the text); \
             distill_status reads the distillation ledger for one session (free, \
             no model call: whether it is distilled and what its last attempt \
             did); and graph_resolution_pending lists the pairs waiting for a \
             merge decision and records either one (merge, or keep them \
             separate) — explicit, never automatic. Lists STATE their cut: a \
             reply that was limited ends with how many rows it did not show.",
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
// the tool names are untouched (18 in the t21 generation this note is about; 21 since
// t12 added graph_fact_add, distill_status and graph_resolution_pending). A tool that
// ships WITHOUT the attribute fails
// `crates/mcp/tests/tool_surface.rs::every_tool_refuses_fields_it_does_not_declare`,
// which walks every published schema rather than naming the tools one by one.
//
// `wiki_pages` and `wiki_links` take no argument, and they now carry [`NoParams`]
// rather than no parameter type at all: without a params type rmcp publishes
// `{"type":"object","properties":{}}`, whose `additionalProperties` is unset, so those
// two silently accepted and discarded ANY field. `NoParams` declares no field either —
// the declared surface is unchanged — and it makes the refusal uniform across every
// tool, whatever the surface's size.

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchParams {
    #[schemars(description = "What to look for")]
    pub query: String,
    #[schemars(
        description = "Rows to show (default 8, the daemon's maximum 200). Honoured (clamped); the reply states how many rows matched before this page and whether it was cut."
    )]
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
    #[schemars(
        description = "Rows to ask for (default 10, the daemon's maximum 200). Honoured: the reply states how many entities matched before this page and what the page left out."
    )]
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
    // Not a promise that every store accepts every one of these: the pair is
    // adjudicated by the daemon, and a refused pair comes back as an error naming
    // the `allowed_namespaces` for that store (issue #56).
    #[schemars(
        description = "user | global | project:<name> | agent:<name> -- but each store writes only some of these; the daemon decides and a refused pair names the allowed set"
    )]
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
    #[schemars(
        description = "Rows to show (default 20). The daemon clamps it; the reply reports the value it applied and how many rows are left."
    )]
    pub limit: Option<u32>,
    #[schemars(
        description = "Rows to skip before this page (default 0) -- how to reach the rows a limit left out"
    )]
    pub offset: Option<u32>,
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactAddParams {
    #[schemars(description = "Source entity id. Give this OR src_name, never both.")]
    pub src: Option<i64>,
    #[schemars(
        description = "Source entity NAME (matched against names and recorded aliases, case/whitespace-insensitive). Give this OR src, never both. A name that resolves to nothing is REFUSED with the name quoted -- this tool never creates an entity."
    )]
    pub src_name: Option<String>,
    #[schemars(description = "Target entity id. Give this OR dst_name, never both.")]
    pub dst: Option<i64>,
    #[schemars(description = "Target entity NAME. Give this OR dst, never both.")]
    pub dst_name: Option<String>,
    #[schemars(description = "The relation, as a short verb phrase (e.g. \"owns\", \"ships_on\")")]
    pub relation: String,
    #[schemars(
        description = "The fact in words, as it should read back (e.g. \"Dana owns the rollback procedure\")"
    )]
    pub fact_text: String,
    #[schemars(
        description = "RFC3339 date/time the fact became true; omitted = now. The write is always recorded as event_time_source=recorded: a tool call is not a transcript."
    )]
    pub valid_at: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DistillStatusParams {
    #[schemars(description = "Session key as the sessions listing spells it (e.g. ruagent:<hex>)")]
    pub session_key: String,
    #[schemars(
        description = "How many of the NEWEST attempts to print (default 10, max 100). The distilled verdict and the cumulative totals cover the session's WHOLE history either way, and the reply says how many attempts it did not show."
    )]
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolutionParams {
    #[schemars(
        description = "list (the default) = read the pending queue; merge = the keeper absorbs the absorbed entity; dismiss = these two stay separate. Anything else is refused."
    )]
    pub action: Option<String>,
    #[schemars(description = "action=merge: the entity id that SURVIVES")]
    pub keeper: Option<i64>,
    #[schemars(
        description = "action=merge: the entity id that is merged away (its edges move to the keeper, and its name becomes an alias of it)"
    )]
    pub absorbed: Option<i64>,
    #[schemars(description = "action=dismiss: one end's entity id")]
    pub a: Option<i64>,
    #[schemars(
        description = "action=dismiss: the other end's entity id (the pair is unordered -- the stored key is the ordered pair)"
    )]
    pub b: Option<i64>,
    #[schemars(
        description = "action=list: also print the pairs that are ALREADY the same object under two rows (default false; they are always counted)"
    )]
    pub show_redundant: Option<bool>,
}

/// Render a /recall response for an agent: sectioned, ids preserved
/// (conservative stubs point at memory_get / knowledge_expand /
/// graph_entity).
fn recall_to_text(resp: &serde_json::Value) -> Result<String, rmcp::ErrorData> {
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
    let body = if out.is_empty() {
        "no results".to_string()
    } else {
        format!(
            "{}\n(ids: #n = memory_get(n), &n = graph_entity(n), expand: n = knowledge_expand(n))",
            out.join("\n")
        )
    };
    // THE COMPLETENESS READING (issue #54) rides the answer, and it is REQUIRED
    // rather than optional: a response whose truncation fields vanished would
    // otherwise read as a complete one, which is the failure mode this note exists
    // to remove. The daemon always emits both keys (t22).
    Ok(format!("{body}{}", recall_cut_note(resp)?))
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

    /// A one-route stand-in for the daemon's `GET /api/v1/graph/entity/{id}`, serving
    /// the TWO bodies the REAL route serves -- captured from it in this unit (report
    /// §1, after rebuilding the stale shared binary):
    ///   id not in the graph -> {"name":null,"kind":null,"aliases":[],"facts":[]}
    ///   id 1 (exists, no facts) -> {"name":"t118-probe-without-facts",...
    /// The happy path is the daemon's own test
    /// `the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive`
    /// (re-read 2026-10-04: `api.rs:6711`; the earlier hint `6673` pointed into
    /// `the_manual_rules_route_runs_with_every_agent_disabled`); this
    /// stub exists so the TOOL's two directions can be pressed without a database.
    async fn stub_daemon() -> String {
        use axum::{Router, routing::get};
        let app = Router::new().route(
            "/api/v1/graph/entity/{id}",
            get(
                |axum::extract::Path(id): axum::extract::Path<String>| async move {
                    if id == "987654321" {
                        axum::Json(serde_json::json!({
                            "name": serde_json::Value::Null, "kind": serde_json::Value::Null,
                            "aliases": [], "facts": []
                        }))
                    } else {
                        axum::Json(serde_json::json!({
                            "name": "t118-probe-without-facts", "kind": "concept",
                            "aliases": [], "facts": []
                        }))
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    struct ToolClient;
    impl rmcp::ClientHandler for ToolClient {
        fn get_info(&self) -> rmcp::model::ClientInfo {
            Default::default()
        }
    }

    /// Call `graph_entity` over the real MCP seam (duplex pipe) and return what the
    /// CALLER sees: `Err(text)` for a refusal (JSON-RPC error or `is_error: true`),
    /// `Ok(text)` for a successful answer.
    async fn graph_entity_as_a_caller_sees_it(daemon_url: &str, id: i64) -> Result<String, String> {
        tool_as_a_caller_sees_it(daemon_url, "graph_entity", serde_json::json!({ "id": id })).await
    }

    /// t118, direction 1 of 2: an id that is NOT in the graph is a REFUSAL that names
    /// it. Before t118 this came back `Ok("entity #987654321 has no current facts")`
    /// with `is_error: Some(false)` -- the caller-visible false success this pins.
    #[tokio::test]
    async fn a_fabricated_entity_id_is_refused_not_answered_with_no_facts() {
        let url = stub_daemon().await;
        let seen = graph_entity_as_a_caller_sees_it(&url, 987_654_321).await;
        let message = seen.expect_err("an id that is not in the graph must be refused");
        assert!(
            message.contains("987654321"),
            "the refusal must NAME the id the caller sent: {message}"
        );
        assert!(message.contains("not found"), "{message}");
        assert!(
            !message.contains("has no current facts"),
            "an absent id must not be reported as an entity without facts: {message}"
        );
    }

    /// t118, direction 2 of 2: an entity that EXISTS with no current facts stays a
    /// success with today's text -- the two cases must be distinguishable.
    #[tokio::test]
    async fn an_entity_with_no_current_facts_is_still_a_success() {
        let url = stub_daemon().await;
        let text = graph_entity_as_a_caller_sees_it(&url, 1)
            .await
            .expect("an entity that exists must not be refused");
        assert!(text.contains("has no current facts"), "{text}");
    }

    /// t118 / t93-M1 (first half): the existence reading, pressed on all four
    /// shapes. The two `Ok` cases are the two facts the tool must tell apart; the
    /// two `Err` cases are the drift the reading must NOT absorb (a missing key used
    /// to read as "not missing"). The protocol half lives above, over a stub serving
    /// the same two bodies the REAL route serves; driving the real route
    /// (`crates/mcp/tests/roundtrip.rs`: `start_test_daemon_with_policy` + `mcp_pair`)
    /// is delegated to the independent verification -- see the report, section 9.
    #[test]
    fn entity_existence_is_read_from_name_and_drift_is_not_absorbed() {
        // An entity that exists: `name` is its name -- a string, even when empty.
        let present = serde_json::json!({ "name": "Kubernetes", "kind": "concept", "aliases": [], "facts": [] });
        assert_eq!(entity_exists(&present), Ok(true), "{present}");

        // An empty string is still a row that is there: `null` is the absent value,
        // not "empty" (the daemon maps a row through `entity.as_ref().map(...)`).
        let present_empty_name =
            serde_json::json!({ "name": "", "kind": null, "aliases": [], "facts": [] });
        assert_eq!(entity_exists(&present_empty_name), Ok(true));

        // The id is not in the graph: `name: null` (daemon api.rs:1361) -- this is
        // the case that used to come back as a SUCCESS with "has no current facts".
        let absent = serde_json::json!({ "name": null, "kind": null, "aliases": [], "facts": [] });
        assert_eq!(entity_exists(&absent), Ok(false));

        // Drift, direction 1: the signal key is gone -- an error, not "missing".
        let renamed = serde_json::json!({ "entity_name": "Kubernetes", "facts": [] });
        let e = entity_exists(&renamed).expect_err("a missing `name` must be an error");
        assert!(e.contains("no `name` key"), "{e}");

        // Drift, direction 2: the signal key was retyped.
        let retyped = serde_json::json!({ "name": 7, "facts": [] });
        let e = entity_exists(&retyped).expect_err("a non-string `name` must be an error");
        assert!(e.contains("type number"), "{e}");
    }

    #[test]
    fn recall_to_text_renders_sections_and_entity_wiki() {
        let resp = serde_json::json!({
            "memories": [{ "kind": "memory", "id": 7, "title": "use sqlx", "hint": "call memory_get" }],
            "knowledge": [{ "kind": "knowledge", "chunk_id": 3, "document": "ops-handbook", "excerpt": "release.sh" }],
            "wiki": [{ "kind": "wiki", "slug": "deploy-pipeline", "title": "Deploy pipeline",
                        "stale": true, "chunk_id": 9, "hint": "verify" }],
            "entities": [{ "kind": "entity", "id": 1, "name": "Kubernetes",
                           "related": { "wiki": [{ "slug": "deploy-pipeline" }] } }],
            // The completeness reading is REQUIRED by the renderer (t22 emits both
            // keys on every recall), so a fixture without it is a shape the daemon
            // cannot send.
            "truncated": false,
            "truncated_sections": [],
        });
        let text = recall_to_text(&resp).expect("a complete answer renders");
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
        let resp = serde_json::json!({ "memories": [], "knowledge": [], "wiki": [], "entities": [],
                                       "truncated": false, "truncated_sections": [] });
        assert_eq!(
            recall_to_text(&resp).expect("a complete answer renders"),
            "no results"
        );
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
            "truncated": false,
            "truncated_sections": [],
        });
        let text = recall_to_text(&resp).expect("a complete answer renders");
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

    /// issue #56 (t2): the daemon's REFUSED-PAIR body is JSON with
    /// `error` + `outcome` + `store` + `namespace` + `reason` + `allowed_namespaces`
    /// (crates/daemon/src/api.rs, `refused_store_namespace`). The bridge must hand
    /// the caller the daemon's own sentence — not the raw JSON, and not a
    /// paraphrase that drops which namespaces the store MAY write.
    #[test]
    fn a_refused_pair_reaches_the_caller_as_the_daemons_own_sentence() {
        // The body is the daemon's, byte for byte in shape; `error` is the sentence.
        let sentence = "namespace governance (design §6.2): store `profile` writes only {user}, \
                        and `global` is not one of them. NOTHING WAS WRITTEN — the refusal is \
                        recorded in the write audit as op='reject'";
        let body = serde_json::json!({
            "error": sentence,
            "outcome": "RejectedNamespace (this store does not allow that namespace; write vocabulary \
                        = ...)",
            "store": "profile",
            "namespace": "global",
            "reason": "namespace governance (design §6.2): store `profile` writes only {user}",
            "allowed_namespaces": ["user"],
        })
        .to_string();
        let msg = refusal_message("memory_write", 409, &body);
        assert!(
            msg.contains(sentence),
            "the daemon's sentence must survive verbatim: {msg}"
        );
        assert!(
            !msg.contains("{\"error\""),
            "the JSON wrapper must be gone: {msg}"
        );
        assert!(msg.contains("HTTP 409"), "{msg}");
        // store, namespace and what the store MAY write are all readable.
        assert!(
            msg.contains("`profile`") && msg.contains("`global`"),
            "{msg}"
        );
        assert!(msg.contains("only {user}"), "{msg}");
        assert!(
            msg.contains("RejectedNamespace") || msg.contains("NOTHING WAS WRITTEN"),
            "{msg}"
        );
        // A refusal is not a receipt, and this stays true of every refusal shape.
        assert!(!msg.contains("memory write: Inserted"), "{msg}");

        // The OTHER routes still answer plain text (t1: `body: None` by default) --
        // the JSON probe is a courtesy, not an assumption about all errors.
        let plain = refusal_message(
            "memory_write",
            400,
            "unknown store `profil` (observation | profile | procedure | lesson)",
        );
        assert!(plain.contains("unknown store `profil`"), "{plain}");
        assert!(plain.contains("HTTP 400"), "{plain}");
    }

    /// A 2xx WITHOUT `outcome` is drift, not a write: `memory_write` used to print
    /// "memory write: ?", which reads exactly like a receipt (issue #56 / t93).
    #[test]
    fn a_success_envelope_without_outcome_is_drift_not_a_receipt() {
        let ok = serde_json::json!({ "outcome": "Inserted(7)" });
        assert_eq!(
            format!(
                "memory write: {}",
                field_str("memory_write", &ok, "outcome").unwrap()
            ),
            "memory write: Inserted(7)",
            "the receipt key and shape are unchanged"
        );
        let empty = serde_json::json!({ "note": "ok" });
        let err = field_str("memory_write", &empty, "outcome")
            .expect_err("a 2xx without `outcome` is drift");
        assert!(err.to_string().contains("outcome"), "{err}");
        assert!(err.to_string().contains("memory_write"), "{err}");
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

    // -- t12: the fact write, the ledger reading, the pending queue ----------

    /// t12 / #51: the write report names the id, the event-time SOURCE, and the way
    /// to read the fact back -- and a caller-supplied valid_at is reported as the
    /// CALLER's value, never as something the transcript said.
    #[test]
    fn a_fact_write_reports_its_source_and_how_to_read_it_back() {
        let resp = serde_json::json!({
            "id": 7, "src": 1, "dst": 2, "event_time_source": "recorded"
        });
        let text = fact_added_to_text(&resp, "ships_on", None).unwrap();
        assert!(
            text.contains("fact #7 written: entity 1 -[ships_on]-> entity 2"),
            "{text}"
        );
        assert!(text.contains("event_time_source=recorded"), "{text}");
        assert!(text.contains("graph_entity(id: 1)"), "{text}");
        assert!(
            text.contains("valid_at omitted, so the daemon recorded NOW"),
            "{text}"
        );
        assert!(
            text.contains("cannot claim the time") && text.contains("extraction path"),
            "the time contract must be stated on the write that follows it: {text}"
        );

        let text = fact_added_to_text(&resp, "ships_on", Some("2026-01-01T00:00:00Z")).unwrap();
        assert!(text.contains("valid_at=2026-01-01T00:00:00Z"), "{text}");

        // A response without the id cannot be reported as a successful write.
        let bad = serde_json::json!({ "src": 1, "dst": 2, "event_time_source": "recorded" });
        let err = fact_added_to_text(&bad, "r", None).expect_err("drift must be loud");
        assert!(err.to_string().contains("id"), "{err}");

        // Both ends resolving to one entity is flagged: it is a self-relation, not
        // the two-entity fact the caller asked for.
        let same = serde_json::json!({
            "id": 9, "src": 4, "dst": 4, "event_time_source": "recorded"
        });
        let text = fact_added_to_text(&same, "relates_to", None).unwrap();
        assert!(text.contains("SAME entity"), "{text}");
    }

    /// t12 / #46: the ledger reading -- verdict over the WHOLE history, newest
    /// attempt from the newest row, and NULL-is-not-zero for every column.
    #[test]
    fn distill_status_reads_its_verdict_and_its_newest_attempt_apart() {
        let resp = serde_json::json!({
            "entries": [
                { "id": 2, "distilled_at": "2026-10-02T00:00:00Z", "status": "failed",
                  "failure_reason": "no content to distil", "agent": "",
                  "memories_written": 0, "entities_written": 0, "relations_written": 0,
                  "relations_duplicate": serde_json::Value::Null,
                  "relations_refused": serde_json::Value::Null,
                  "tokens_used": serde_json::Value::Null,
                  "context_size": serde_json::Value::Null,
                  "cost_usd": serde_json::Value::Null,
                  "retries": 2, "windows": 1, "truncated": true },
                { "id": 1, "distilled_at": "2026-10-01T00:00:00Z", "status": "ok",
                  "agent": "mock", "memories_written": 3, "entities_written": 1,
                  "relations_written": 2, "tokens_used": 1234, "context_size": 200000,
                  "cost_usd": 0.0012, "retries": 0, "windows": 1, "truncated": false },
            ],
            "total": 2, "limit": 10, "offset": 0, "truncated": false,
            "by_status": { "ok": 1, "empty": 0, "failed": 1, "not_recorded": 0 },
            "totals": { "tokens_used": 1234, "cost_usd": 0.0012,
                        "tokens_used_not_recorded": 1, "cost_usd_not_recorded": 1 },
        });
        let text = distill_status_to_text(&resp, "ruagent:abc").unwrap();
        assert!(text.contains("DISTILLED (1 attempt(s)"), "{text}");
        assert!(text.contains("newest attempt #2"), "{text}");
        assert!(text.contains("status=failed"), "{text}");
        assert!(
            text.contains("failure reason: no content to distil"),
            "{text}"
        );
        assert!(
            text.contains(
                "attempts recorded for this session: 2 (ok=1 empty=0 failed=1 not_recorded=0)"
            ),
            "{text}"
        );
        // The newest attempt measured a LOSSY single window: said in one breath.
        assert!(
            text.contains("windows=1, truncated=YES -- the transcript was cut to one window and its TAIL WAS DROPPED"),
            "{text}"
        );
        assert!(text.contains("tokens NOT RECORDED"), "{text}");
        assert!(text.contains(", cost NOT RECORDED,"), "{text}");
        assert!(text.contains("retries 2"), "{text}");
        // The totals span the history, with the unrecorded count beside them.
        assert!(
            text.contains("tokens 1234 (1 of 2 attempt(s) did not record one)"),
            "{text}"
        );
        assert!(
            text.contains("$0.0012 (1 of 2 attempt(s) did not record one)"),
            "{text}"
        );
        // never a fabricated zero for a column nobody measured
        assert!(!text.contains("tokens 0 "), "{text}");
        assert!(!text.contains("$0.0000"), "{text}");
    }

    /// t12: a session the ledger has no row for is NOT DISTILLED, says why the ledger
    /// is silent, and points at the free way to distil it.
    #[test]
    fn distill_status_says_what_an_empty_ledger_means() {
        let resp = serde_json::json!({
            "entries": [], "total": 0, "limit": 10, "offset": 0, "truncated": false,
            "by_status": { "ok": 0, "empty": 0, "failed": 0, "not_recorded": 0 },
            "totals": { "tokens_used": serde_json::Value::Null,
                        "cost_usd": serde_json::Value::Null,
                        "tokens_used_not_recorded": 0, "cost_usd_not_recorded": 0 },
        });
        let text = distill_status_to_text(&resp, "ruagent:none").unwrap();
        assert!(text.contains("NOT DISTILLED"), "{text}");
        assert!(
            text.contains("no attempt recorded for this session"),
            "{text}"
        );
        assert!(
            text.contains("distill_session"),
            "the free way to distil it must be named: {text}"
        );
        assert!(
            text.contains("tokens and cost are unanswerable for this session (not 0)"),
            "an empty history has nothing to sum: {text}"
        );
    }

    /// t12: the ledger's REQUIRED keys, and the drift rule that makes a renamed one
    /// loud instead of silently "no successful attempt".
    #[test]
    fn distill_status_refuses_a_ledger_response_without_its_load_bearing_keys() {
        let base = serde_json::json!({
            "entries": [], "total": 0, "limit": 10, "offset": 0,
            "by_status": { "ok": 0, "empty": 0, "failed": 0, "not_recorded": 0 },
            "totals": { "tokens_used": serde_json::Value::Null,
                        "cost_usd": serde_json::Value::Null,
                        "tokens_used_not_recorded": 0, "cost_usd_not_recorded": 0 },
        });
        assert!(distill_status_to_text(&base, "k").is_ok());

        for key in ["entries", "total", "limit", "offset", "by_status", "totals"] {
            let mut thin = base.clone();
            thin.as_object_mut().unwrap().remove(key);
            let err = distill_status_to_text(&thin, "k")
                .expect_err("a missing load-bearing key must be drift");
            assert!(err.to_string().contains(key), "{key}: {err}");
        }
        // A MISSING BUCKET is drift too: defaulting it to 0 would turn "the shape
        // changed" into "this session was never distilled".
        let mut no_bucket = base.clone();
        no_bucket["by_status"] = serde_json::json!({ "ok": 0, "empty": 0, "failed": 0 });
        let err = distill_status_to_text(&no_bucket, "k").expect_err("missing bucket");
        assert!(err.to_string().contains("by_status.not_recorded"), "{err}");
        let mut no_missing = base.clone();
        no_missing["totals"] = serde_json::json!({ "tokens_used": 1, "cost_usd": 0.1 });
        let err = distill_status_to_text(&no_missing, "k").expect_err("missing count");
        assert!(
            err.to_string().contains("tokens_used_not_recorded"),
            "{err}"
        );
    }

    /// t12 / #50: the decision sheet -- both ends, both counts, and the exact calls.
    #[test]
    fn pending_to_text_prints_the_decision_sheet() {
        let resp = serde_json::json!({
            "pending": [{
                "a": { "id": 12, "name": "Kubernetes", "kind": "concept", "summary": null },
                "b": { "id": 31, "name": "k8s", "kind": "concept", "summary": null },
                "a_facts": 4, "b_facts": 2, "reason": "one is an abbreviation of the other",
                "created_at": "2026-10-01T00:00:00Z",
            }],
            "redundant": [[12, 40, "identical names"]],
            "pending_count": 1, "redundant_count": 1, "dismissed_count": 2,
            "counts_note": "a_facts/b_facts are each entity's current facts",
        });
        let text = pending_to_text(&resp, false).unwrap();
        assert!(
            text.contains("merge decisions waiting: 1 pair(s)"),
            "{text}"
        );
        assert!(text.contains("2 pair(s) already"), "{text}");
        assert!(
            text.contains("1 pair(s) are ALREADY the same object"),
            "{text}"
        );
        assert!(
            text.contains("counts: a_facts/b_facts are each entity's current facts"),
            "the daemon's own definition ships with the numbers: {text}"
        );
        assert!(
            text.contains("#12 \"Kubernetes\" (concept) has 4 current fact(s)"),
            "{text}"
        );
        assert!(
            text.contains("#31 \"k8s\" (concept) has 2 current fact(s)"),
            "{text}"
        );
        assert!(
            text.contains("one is an abbreviation of the other"),
            "{text}"
        );
        assert!(
            text.contains(
                "merge: graph_resolution_pending(action: \"merge\", keeper: 12, absorbed: 31)"
            ),
            "{text}"
        );
        assert!(
            text.contains("apart: graph_resolution_pending(action: \"dismiss\", a: 12, b: 31)"),
            "{text}"
        );
        assert!(
            text.contains("NO LIMIT WAS APPLIED"),
            "the whole queue is returned, and the reply says so: {text}"
        );
        // The redundant rows are COUNTED but only printed on request.
        assert!(!text.contains("#12 and #40"), "{text}");
        assert!(text.contains("show_redundant: true"), "{text}");
        let shown = pending_to_text(&resp, true).unwrap();
        assert!(shown.contains("#12 and #40"), "{shown}");

        // Drift: a queue response without its counts cannot be rendered.
        let mut thin = resp.clone();
        thin.as_object_mut().unwrap().remove("dismissed_count");
        let err = pending_to_text(&thin, false).expect_err("drift must be loud");
        assert!(err.to_string().contains("dismissed_count"), "{err}");
    }

    /// t12: the two decisions, including the idempotent repeat and the drift case.
    #[test]
    fn the_two_decisions_are_reported_from_the_daemons_own_answer() {
        let merged = serde_json::json!({
            "keeper": 12, "absorbed": 31, "moved": 5, "alias": "k8s", "alias_written": false
        });
        let text = merge_result_to_text(&merged).unwrap();
        assert!(text.contains("#31 was absorbed into #12"), "{text}");
        assert!(text.contains("5 edge(s) moved over"), "{text}");
        assert!(
            text.contains("alias_written=false reports THIS second write"),
            "false must not read as 'the alias is missing': {text}"
        );
        assert!(text.contains("MERGING IS EXPLICIT"), "{text}");

        let dismissed = serde_json::json!({
            "a": 12, "b": 31, "dismissed": true, "already_dismissed": false
        });
        let text = dismiss_result_to_text(&dismissed).unwrap();
        assert!(text.contains("#12 and #31 STAY SEPARATE"), "{text}");
        assert!(text.contains("not a lock"), "{text}");

        let again = serde_json::json!({
            "a": 12, "b": 31, "dismissed": false, "already_dismissed": true
        });
        let text = dismiss_result_to_text(&again).unwrap();
        assert!(text.contains("already recorded"), "{text}");

        // Neither flag true cannot be reported as a decision.
        let neither = serde_json::json!({
            "a": 12, "b": 31, "dismissed": false, "already_dismissed": false
        });
        assert!(dismiss_result_to_text(&neither).is_err());
    }

    /// t12 / #54: the ONE remainder line, in both of its shapes -- and the acceptance
    /// sentence (还有 N 条未显示) is what it prints.
    #[test]
    fn the_remainder_line_counts_what_a_cut_list_left_out() {
        // The envelope shape: a page that was cut.
        let cut = unshown_note(2, 3, Some(2), Some(0));
        assert!(cut.contains("shown 2 of 3 (limit=2, offset=0)"), "{cut}");
        assert!(cut.contains("还有 1 条未显示"), "{cut}");
        assert!(cut.contains("raise limit or ask for offset=2"), "{cut}");

        // A complete page still says it is complete, and what is behind it.
        let whole = unshown_note(3, 3, Some(2), Some(2));
        assert!(
            whole.contains("shown 3 of 3 (limit=2, offset=2)"),
            "{whole}"
        );
        assert!(!whole.contains("还有"), "{whole}");
        assert!(whole.contains("2 earlier row(s) not shown"), "{whole}");

        // The search-leg shape: a match count and no page bounds.
        let search = unshown_note(8, 20, None, None);
        assert!(
            search.contains("matched 20, this reply carries 8"),
            "{search}"
        );
        assert!(search.contains("还有 12 条未显示"), "{search}");
    }

    /// t26 / #54: a search page carries the route's OWN reading of what it left out,
    /// in the three boundary shapes t22/t25 fixed for every other list -- reached (cut),
    /// exactly equal (NOT a cut), untouched (not a cut) -- plus the one shape only a
    /// search has: a count the route could not complete.
    #[test]
    fn a_search_page_prints_the_remainder_the_route_measured() {
        // REACHED: two rows out of five matched, so three are left.
        let cut = serde_json::json!({
            "hits": [{"content": "a"}, {"content": "b"}],
            "limit": 2, "matched": 5, "matched_exact": true,
            "truncated": true, "truncated_sections": ["memories"],
        });
        let text = memory_search_to_text(&cut).unwrap();
        assert!(text.contains("matched 5, this reply carries 2"), "{text}");
        assert!(text.contains("还有 3 条未显示"), "{text}");

        // EXACTLY EQUAL: the page holds every match, so it is NOT a cut -- the case a
        // "the page looks full" rule gets wrong.
        let exact = serde_json::json!({
            "hits": [{"content": "a"}, {"content": "b"}],
            "limit": 2, "matched": 2, "matched_exact": true,
            "truncated": false, "truncated_sections": [],
        });
        let text = memory_search_to_text(&exact).unwrap();
        assert!(text.contains("matched 2, this reply carries 2"), "{text}");
        assert!(!text.contains("还有"), "{text}");

        // UNTOUCHED: the same reading, and no warning noise.
        let roomy = serde_json::json!({
            "hits": [{"content": "a"}],
            "limit": 8, "matched": 1, "matched_exact": true,
            "truncated": false, "truncated_sections": [],
        });
        let text = memory_search_to_text(&roomy).unwrap();
        assert!(text.contains("matched 1, this reply carries 1"), "{text}");
        assert!(!text.contains("还有"), "{text}");

        // A FLOOR IS PRINTED AS A FLOOR (t25): the route's scan stopped at its ceiling,
        // so both the count and the remainder are lower bounds and say so.
        let floored = serde_json::json!({
            "hits": [{"content": "a"}],
            "limit": 1, "matched": 5000, "matched_exact": false,
            "truncated": true, "truncated_sections": ["memories"],
        });
        let text = memory_search_to_text(&floored).unwrap();
        assert!(text.contains("matched AT LEAST 5000"), "{text}");
        assert!(text.contains("至少还有 4999 条未显示"), "{text}");
        assert!(text.contains("exact match count is unknown"), "{text}");

        // The graph search renders the SAME reading from its own keys, and keeps saying
        // which pass answered.
        let entities: Vec<serde_json::Value> =
            (0..2).map(|i| serde_json::json!({ "id": i })).collect();
        let gs = serde_json::json!({
            "entities": entities, "match": "candidate",
            "limit": 2, "matched": 7, "matched_exact": true,
            "truncated": true, "truncated_sections": ["entities"],
        });
        let text = graph_search_to_text(&gs).unwrap();
        assert!(
            text.contains("match=candidate, 2 entity/entities returned"),
            "{text}"
        );
        assert!(text.contains("还有 5 条未显示"), "{text}");
        // An empty answer is not a cut, and prints no remainder line.
        let none = serde_json::json!({
            "entities": [], "match": "none",
            "limit": 10, "matched": 0, "matched_exact": true,
            "truncated": false, "truncated_sections": [],
        });
        let text = graph_search_to_text(&none).unwrap();
        assert!(!text.contains("还有"), "{text}");

        // DRIFT: the daemon's flag and its counts answer ONE question, so a response
        // where they disagree is not renderable as a single reading.
        let disagreeing = serde_json::json!({
            "hits": [{"content": "a"}],
            "limit": 8, "matched": 4, "matched_exact": true,
            "truncated": false, "truncated_sections": [],
        });
        let err = memory_search_to_text(&disagreeing).expect_err("a disagreement is drift");
        assert!(err.to_string().contains("truncated"), "{err}");

        // And the reading is REQUIRED: a route that stops publishing it cannot be
        // reported as a complete answer.
        let thin = serde_json::json!({ "hits": [{"content": "a"}] });
        let err = memory_search_to_text(&thin).expect_err("a missing reading is drift");
        assert!(err.to_string().contains("matched"), "{err}");

        // The knowledge leg keeps its own engine-published count (unchanged by t26).
        let kn = serde_json::json!({
            "hits": [{"document": "d", "content": "c"}], "total": 1, "matched": 4
        });
        let text = knowledge_hits_to_text(&kn).unwrap();
        assert!(text.contains("matched 4, this reply carries 1"), "{text}");
        assert!(text.contains("还有 3 条未显示"), "{text}");
        let no_hits = serde_json::json!({ "hits": [], "total": 0, "matched": 0 });
        assert_eq!(knowledge_hits_to_text(&no_hits).unwrap(), "no results");
    }

    /// t12 / #54 for a WINDOW-shaped answer: the cut is named, per section, with the
    /// count the daemon publishes and no invented number where it publishes none.
    #[test]
    fn the_recall_cut_note_names_the_section_and_what_it_dropped() {
        let cut = serde_json::json!({
            "memories": [], "knowledge": [], "wiki": [], "entities": [],
            "truncated": true, "truncated_sections": ["memories", "wiki"],
            "memory_legs": { "dropped_by_top_n": 7 },
            "scoring": { "candidates": 12, "leg_window": 5 },
        });
        let note = recall_cut_note(&cut).unwrap();
        assert!(note.contains("INCOMPLETE"), "{note}");
        assert!(note.contains("memories, wiki"), "{note}");
        assert!(note.contains("还有 7 条未显示"), "{note}");
        assert!(
            note.contains("this reply publishes no count"),
            "an unpublished count is admitted, not invented: {note}"
        );

        let knowledge_cut = serde_json::json!({
            "memories": [], "knowledge": [{"kind": "knowledge"}], "wiki": [], "entities": [],
            "truncated": true, "truncated_sections": ["knowledge"],
            "scoring": { "candidates": 12, "leg_window": 5 },
        });
        let note = recall_cut_note(&knowledge_cut).unwrap();
        assert!(note.contains("12 candidate(s) in a window of 5"), "{note}");

        // Not truncated is a reading, and it prints nothing extra.
        let whole = serde_json::json!({
            "truncated": false, "truncated_sections": [],
        });
        assert_eq!(recall_cut_note(&whole).unwrap(), "");

        // The keys are load-bearing: without them the answer cannot be called
        // complete OR cut, which is exactly the ambiguity #54 removes.
        let no_flag = serde_json::json!({ "truncated_sections": [] });
        assert!(recall_cut_note(&no_flag).is_err());
        let no_sections = serde_json::json!({ "truncated": false });
        assert!(recall_cut_note(&no_sections).is_err());
    }

    /// Call ONE tool over the real MCP seam (a duplex pipe) and return what the CALLER
    /// sees: `Err(text)` for a refusal (JSON-RPC error or `is_error: true`), `Ok(text)`
    /// for a successful answer.
    async fn tool_as_a_caller_sees_it(
        daemon_url: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<String, String> {
        use rmcp::ServiceExt;
        let (server_t, client_t) = tokio::io::duplex(65536);
        let url = daemon_url.to_string();
        let server_task = tokio::spawn(async move {
            PlatformTools::new(BridgeConfig { daemon_url: url })
                .serve(server_t)
                .await
                .expect("mcp server starts")
        });
        let client = ToolClient.serve(client_t).await.unwrap();
        let out = client
            .call_tool(
                rmcp::model::CallToolRequestParams::new(tool.to_string())
                    .with_arguments(args.as_object().cloned().unwrap_or_default()),
            )
            .await;
        let seen = match out {
            Err(e) => Err(e.to_string()),
            Ok(r) => {
                let text: String = r
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        rmcp::model::ContentBlock::Text(t) => Some(t.text.to_string()),
                        _ => None,
                    })
                    .collect();
                if r.is_error == Some(true) {
                    Err(text)
                } else {
                    Ok(text)
                }
            }
        };
        let _ = client.cancel().await;
        let _ = server_task.await;
        seen
    }

    /// A stub serving the two search routes with the payload
    /// `crates/daemon/src/api.rs` now produces: a corpus of five matches, a page of
    /// `limit` rows, and the reading beside it (`matched`, `matched_exact`,
    /// `truncated`, `truncated_sections`).
    ///
    /// WHY A STUB, AND WHERE THE REAL ROUTE IS PRESSED INSTEAD: the route's own readings
    /// are asserted against a real database and real rows by the daemon's test
    /// `search_pages_honour_their_limit_and_state_their_cut` (three boundaries plus the
    /// empty case). What this presses is the TOOL end over the wire: that the number a
    /// caller reads comes from those keys, rather than from a constant mirrored in this
    /// crate -- which is what t17-O1 measured (a `ROUTE_WINDOW = 10` that guessed at the
    /// daemon's window and drifted from it).
    async fn search_stub_daemon() -> String {
        use axum::{Router, routing::get};
        /// A five-match corpus served `limit` rows to a page.
        fn body(rows_key: &str, section: &str, limit: u64) -> serde_json::Value {
            const MATCHED: u64 = 5;
            let rows: Vec<serde_json::Value> = (0..limit.min(MATCHED))
                .map(|i| {
                    serde_json::json!({
                        "id": i + 1,
                        "content": format!("t26-stub-{i}"),
                        "name": format!("t26-stub-{i}"),
                    })
                })
                .collect();
            let cut = limit < MATCHED;
            let mut body = serde_json::Map::new();
            body.insert(rows_key.to_string(), serde_json::json!(rows));
            body.insert("limit".to_string(), serde_json::json!(limit));
            body.insert("matched".to_string(), serde_json::json!(MATCHED));
            body.insert("matched_exact".to_string(), serde_json::json!(true));
            body.insert("truncated".to_string(), serde_json::json!(cut));
            body.insert(
                "truncated_sections".to_string(),
                serde_json::json!(if cut {
                    vec![section]
                } else {
                    Vec::<&str>::new()
                }),
            );
            serde_json::Value::Object(body)
        }
        type Q = std::collections::HashMap<String, String>;
        let graph = get(
            |axum::extract::Query(q): axum::extract::Query<Q>| async move {
                let limit: u64 = q.get("limit").and_then(|l| l.parse().ok()).unwrap_or(10);
                let mut v = body("entities", "entities", limit);
                v.as_object_mut()
                    .expect("an object")
                    .insert("match".to_string(), serde_json::json!("exact"));
                axum::Json(v)
            },
        );
        let memory = get(
            |axum::extract::Query(q): axum::extract::Query<Q>| async move {
                let limit: u64 = q.get("limit").and_then(|l| l.parse().ok()).unwrap_or(8);
                axum::Json(body("hits", "memories", limit))
            },
        );
        let app = Router::new()
            .route("/api/v1/graph/search", graph)
            .route("/api/v1/memory/search", memory);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    /// t26 / #54 at the CALLER's end of the wire: a cut page carries the REAL remainder,
    /// a complete page carries none, and neither hedges with UNKNOWN any more.
    #[tokio::test]
    async fn a_cut_search_page_carries_the_real_remainder_over_the_mcp_seam() {
        let url = search_stub_daemon().await;

        // CUT: two rows out of five matched.
        for tool in ["graph_search", "memory_search"] {
            let text = tool_as_a_caller_sees_it(
                &url,
                tool,
                serde_json::json!({ "query": "t26", "limit": 2 }),
            )
            .await
            .unwrap_or_else(|e| panic!("{tool} must succeed: {e}"));
            println!("READING t26 {tool} limit=2 -> {text}");
            assert!(
                text.contains("matched 5, this reply carries 2"),
                "{tool}: {text}"
            );
            assert!(text.contains("还有 3 条未显示"), "{tool}: {text}");
            assert!(
                !text.contains("UNKNOWN"),
                "{tool} must not hedge about the remainder any more: {text}"
            );
        }

        // COMPLETE: five rows out of five -- no remainder is claimed.
        for tool in ["graph_search", "memory_search"] {
            let text = tool_as_a_caller_sees_it(
                &url,
                tool,
                serde_json::json!({ "query": "t26", "limit": 5 }),
            )
            .await
            .unwrap_or_else(|e| panic!("{tool} must succeed: {e}"));
            assert!(
                text.contains("matched 5, this reply carries 5"),
                "{tool}: {text}"
            );
            assert!(
                !text.contains("还有"),
                "{tool} must not claim a remainder it does not have: {text}"
            );
        }
    }
}
