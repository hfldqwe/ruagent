//! MCP roundtrip: a real rmcp client speaks the MCP protocol over an
//! in-process duplex pipe to `PlatformTools`, which calls a real in-test
//! daemon over HTTP — the whole §6.5 seam, protocol-complete.

use std::sync::Arc;

use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};
use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_mcp::{BridgeConfig, PlatformTools};
use ruagent_store::Db;

struct TestClient;

impl ClientHandler for TestClient {
    fn get_info(&self) -> rmcp::model::ClientInfo {
        rmcp::model::ClientInfo::default()
    }
}

/// Extract the text of a CallToolResult.
fn text_of(out: &rmcp::model::CallToolResult) -> String {
    out.content
        .iter()
        .filter_map(|c| match c {
            rmcp::model::ContentBlock::Text(t) => Some(t.text.to_string()),
            _ => None,
        })
        .collect()
}

fn call(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    let obj = args.as_object().cloned().unwrap_or_default();
    CallToolRequestParams::new(name.to_string()).with_arguments(obj)
}

async fn start_test_daemon() -> String {
    start_test_daemon_with_policy("[permissions]\ndefault = \"ask\"\n")
        .await
        .0
}

/// The daemon PLUS the config root it serves, so a test can read `policy.toml`
/// itself: for a capability write the FILE is the witness, not the tool's reply.
async fn start_test_daemon_with_policy(policy_toml: &str) -> (String, std::path::PathBuf) {
    static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-mcp-e2e-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("agents.toml"),
        "[agent.mock]\nharness = \"mock\"\ncommand = \"mock-agent\"\ndescription = \"x\"\nenabled = false\n",
    )
    .unwrap();
    std::fs::write(
        config_dir.join("mcp.toml"),
        "[profile.default]\nservers = []\n",
    )
    .unwrap();
    std::fs::write(config_dir.join("policy.toml"), policy_toml).unwrap();

    let cfg = DaemonConfig::load(&root).unwrap();
    let db = Db::open(root.join("data").join("ruagent.db")).unwrap();
    let mut agents = cfg.agents.clone();
    for card in &mut agents {
        if let Some(id) = db.agent_id_by_name(&card.name).await.unwrap() {
            card.id = id;
        }
        db.upsert_agent(card).await.unwrap();
    }
    let knowledge = ruagent_knowledge::Knowledge::open(&root, db.clone())
        .await
        .unwrap();
    let chats = ruagent_daemon::chat::ChatManager::new(
        db.clone(),
        root.clone(),
        Arc::new(|_, _| {}),
        cfg.mcp.clone(),
        ruagent_daemon::distill::AutoDistill::default(),
        None,
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    // t10: the LIVE-PLANE INSTALL the real `serve` performs (`daemon/src/lib.rs`).
    // Without it this harness's plane is `legacy()` no matter what `policy.toml`
    // says, so `/api/v1/capabilities` would describe a config nobody has: a
    // `[capabilities]` table seeded into the file below would read as ABSENT.
    chats.set_capabilities(cfg.capabilities.clone());
    let mgr = Arc::new(RunManager::new(
        db.clone(),
        root.clone(),
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));
    mgr.set_capabilities(chats.capabilities_handle());
    let app = ruagent_daemon::api::router(AppState {
        mgr,
        config: Arc::new(cfg),
        knowledge: Arc::new(knowledge),
        chats,
        sessions: Arc::new(ruagent_daemon::sessions::SessionIndexer::new(
            db.clone(),
            std::env::temp_dir(),
        )),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://{addr}"), root)
}

#[tokio::test]
async fn mcp_tools_reach_the_daemon_memory_and_knowledge() {
    let daemon_url = start_test_daemon().await;

    // Wire an rmcp client to the platform server over a duplex pipe.
    // Both sides are spawned (their service loops drive themselves).
    let url_for_server = daemon_url.clone();
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        PlatformTools::new(BridgeConfig {
            daemon_url: url_for_server,
        })
        .serve(server_transport)
        .await
        .expect("mcp server starts")
    });

    let client = TestClient.serve(client_transport).await.unwrap();

    // List tools: the platform surface.
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    // t93 / audit M5: this used to be `any()` over 9 of the 14 names, so deleting
    // any of the five tools this generation added (memory_forget_report,
    // graph_search, graph_retrieve, wiki_pages, wiki_links) stayed green. Set
    // EQUALITY (both directions) plus a count is what makes removal loud.
    // t6 (wave C) added the capability plane's four tools. This vector is the
    // ONLY place the surface is counted: set equality (both directions) plus the
    // count is what makes adding, renaming or removing a tool loud.
    let mut expected = vec![
        "memory_search",
        "memory_write",
        "memory_recall",
        "memory_get",
        "knowledge_search",
        "knowledge_ingest",
        "knowledge_expand",
        "graph_entity",
        "memory_forget_report",
        "graph_search",
        "graph_retrieve",
        "wiki_pages",
        "wiki_links",
        "list_tasks",
        "capabilities_list",
        "capability_set",
        "distill_session",
        "knowledge_graph_ingest",
    ];
    assert_eq!(
        expected.len(),
        18,
        "the declared consumption surface is 18 tools"
    );
    let mut got = names.clone();
    got.sort();
    expected.sort();
    assert_eq!(
        got, expected,
        "the MCP surface changed: missing or extra tools. got={names:?}"
    );

    // Write a memory through the MCP tool.
    let out = client
        .call_tool(call(
            "memory_write",
            serde_json::json!({
                "store": "observation",
                "namespace": "user",
                "content": "the user prefers rust over javascript"
            }),
        ))
        .await
        .unwrap();
    assert!(text_of(&out).contains("Inserted"), "{out:?}");

    // Ingest a document into the knowledge base.
    let out = client
        .call_tool(call(
            "knowledge_ingest",
            serde_json::json!({
                "name": "design-notes",
                "content": "The deploy script lives in scripts/release.sh. Run it from the repository root."
            }),
        ))
        .await
        .unwrap();
    assert!(text_of(&out).contains("1"), "one chunk ingested: {out:?}");

    // Search memories via MCP.
    let out = client
        .call_tool(call(
            "memory_search",
            serde_json::json!({ "query": "rust" }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("rust over javascript"),
        "{}",
        text_of(&out)
    );

    // Search knowledge via MCP.
    let out = client
        .call_tool(call(
            "knowledge_search",
            serde_json::json!({ "query": "release script" }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("scripts/release.sh"),
        "{}",
        text_of(&out)
    );

    // Expand a hit into its parent section via MCP: the chunk id comes
    // off the daemon's HTTP surface.
    let http = reqwest::Client::new();
    let docs: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/documents"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let doc_id = docs["documents"]
        .as_array()
        .and_then(|d| d.first())
        .and_then(|d| d["id"].as_i64())
        .expect("ingested document listed");
    let chunks: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/documents/{doc_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let chunk_id = chunks["chunks"]
        .as_array()
        .and_then(|c| c.first())
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|id| id.as_i64())
        .expect("document has chunks");
    let out = client
        .call_tool(call(
            "knowledge_expand",
            serde_json::json!({ "chunk_id": chunk_id }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("release.sh"),
        "expansion carries the section: {}",
        text_of(&out)
    );

    // Platform ops: create a task via REST and list it via MCP.
    let http = reqwest::Client::new();
    http.post(format!("{daemon_url}/api/v1/tasks"))
        .json(&serde_json::json!({"title": "mcp roundtrip", "intent": "x"}))
        .send()
        .await
        .unwrap();
    let out = client
        .call_tool(call("list_tasks", serde_json::json!({})))
        .await
        .unwrap();
    assert!(text_of(&out).contains("mcp roundtrip"), "{}", text_of(&out));

    client.cancel().await.unwrap();
    let server = server_task.await.unwrap();
    server.cancel().await.unwrap();
}

/// The message of a REFUSED tool call. A refusal that reads as a success is the
/// failure this helper exists to make impossible: `ErrorData` reaches the
/// client as a JSON-RPC error, so `Ok(..)` here is a broken contract, not a
/// softer assertion. A failure raised BEFORE the handler runs (rmcp's parameter
/// extraction, t17) arrives as `Ok(..)` with `is_error: true` instead — that is
/// [`failure`]'s shape, and the difference is the layer, not the severity.
fn refusal(out: Result<rmcp::model::CallToolResult, rmcp::ServiceError>) -> String {
    match out {
        Err(e) => e.to_string(),
        Ok(r) => panic!(
            "expected a refusal, got a successful result: {} / {r:?}",
            text_of(&r)
        ),
    }
}

/// A tool call that FAILED as the client sees it. Two shapes are legitimate and
/// both are failures a caller acts on: a JSON-RPC error (`Err`, what a
/// handler-level `Err(ErrorData)` produces) and a tool result with
/// `is_error: true` (what rmcp's own parameter extraction produces for a field the
/// tool cannot send — t17). A plain `Ok` that is not an error is the false success
/// this helper exists to make impossible.
fn failure(out: Result<rmcp::model::CallToolResult, rmcp::ServiceError>) -> String {
    match out {
        Err(e) => e.to_string(),
        Ok(r) if r.is_error == Some(true) => text_of(&r),
        Ok(r) => panic!(
            "expected a failure, got a successful result: {} / {r:?}",
            text_of(&r)
        ),
    }
}

type ClientHandle = rmcp::service::RunningService<rmcp::service::RoleClient, TestClient>;
type ServerHandle = tokio::task::JoinHandle<
    rmcp::service::RunningService<rmcp::service::RoleServer, PlatformTools>,
>;

/// One platform MCP server and one client over an in-process duplex pipe: the
/// whole §6.5 seam, protocol and HTTP included.
async fn mcp_pair(daemon_url: &str) -> (ClientHandle, ServerHandle) {
    let url_for_server = daemon_url.to_string();
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        PlatformTools::new(BridgeConfig {
            daemon_url: url_for_server,
        })
        .serve(server_transport)
        .await
        .expect("mcp server starts")
    });
    let client = TestClient.serve(client_transport).await.unwrap();
    (client, server_task)
}

async fn shutdown_pair(client: ClientHandle, server_task: ServerHandle) {
    client.cancel().await.unwrap();
    let server = server_task.await.unwrap();
    server.cancel().await.unwrap();
}

/// t6 (wave C), part 1: the capability-plane tools against the REAL daemon
/// routes.
///
/// The request/response SHAPING is unit-tested inside `ruagent-mcp`
/// (crates/mcp/src/lib.rs: the PUT body, the extractor default, the ingest
/// query, the renderings, the refusal text). This test is the other half: the
/// shapes are the ones the daemon actually serves, the ids are the ones
/// `policy.toml` accepts, and the refusals carry the daemon's own words.
#[tokio::test]
async fn mcp_capability_plane_tools_reach_the_daemon() {
    let daemon_url = start_test_daemon().await;
    let (client, server_task) = mcp_pair(&daemon_url).await;

    // -- capabilities_list: the plane is readable, with tier and enable state.
    let out = client
        .call_tool(call("capabilities_list", serde_json::json!({})))
        .await
        .unwrap();
    let text = text_of(&out);
    // the ids are the ones the config file and the HTTP API use — verbatim
    assert!(text.contains("recall_leg_wiki"), "{text}");
    assert!(text.contains("session_extract_rules"), "{text}");
    assert!(text.contains("knowledge_ingest_graph"), "{text}");
    assert!(text.contains("distill_session"), "{text}");
    // a reader must be able to see the tier and the state per row
    assert!(
        text.contains("[llm] distill_session: enabled=false"),
        "{text}"
    );
    assert!(
        text.contains("knowledge_ingest_graph: enabled=false"),
        "the new graph capability is off by default: {text}"
    );
    assert!(text.contains("configured: legacy"), "{text}");

    // A tier filter an agent may pass, and a bogus one that must be refused
    // rather than silently read as "no capabilities".
    let out = client
        .call_tool(call(
            "capabilities_list",
            serde_json::json!({ "tier": "llm" }),
        ))
        .await
        .unwrap();
    let llm = text_of(&out);
    assert!(llm.contains("distill_session"), "{llm}");
    assert!(!llm.contains("recall_leg_wiki"), "{llm}");
    let err = refusal(
        client
            .call_tool(call(
                "capabilities_list",
                serde_json::json!({ "tier": "cheap" }),
            ))
            .await,
    );
    assert!(err.contains("cheap"), "{err}");
    assert!(err.contains("free") && err.contains("llm"), "{err}");

    // -- capability_set: an unknown id is refused BY NAME, with the known ids.
    let err = refusal(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({ "id": "recall_leg_wik", "enabled": false }),
            ))
            .await,
    );
    assert!(err.contains("recall_leg_wik"), "{err}");
    assert!(err.contains("known ids"), "{err}");
    assert!(err.contains("capability_set"), "{err}");

    // -- the llm cost gate: enabling distill_session without confirm_cost is a
    // refusal that names the flag — never a silent spend.
    let err = refusal(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({ "id": "distill_session", "enabled": true }),
            ))
            .await,
    );
    assert!(err.contains("distill_session"), "{err}");
    assert!(err.contains("confirm_cost"), "{err}");

    // -- knowledge_graph_ingest is exercised end to end in the second test
    // below; here the point is that enabling a FREE capability needs no
    // confirm_cost, and the reply says the value now comes from the file.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({ "id": "knowledge_ingest_graph", "enabled": true }),
        ))
        .await
        .unwrap();
    let enabled_text = text_of(&out);
    assert!(
        enabled_text.contains("`knowledge_ingest_graph` is now enabled=true"),
        "{enabled_text}"
    );

    // -- the toggle did NOT silently reset the rest of the plane: the daemon's
    // PUT replaces the whole table, so this is the assertion that the bridge's
    // read-modify-write kept every other capability where it was.
    let out = client
        .call_tool(call("capabilities_list", serde_json::json!({})))
        .await
        .unwrap();
    let after = text_of(&out);
    assert!(
        after.contains("knowledge_ingest_graph: enabled=true"),
        "{after}"
    );
    assert!(
        after.contains("knowledge_ingest_graph: enabled=true (configured: file"),
        "the write landed in policy.toml: {after}"
    );
    assert!(after.contains("recall_leg_wiki: enabled=true"), "{after}");
    assert!(
        after.contains("[llm] distill_session: enabled=false"),
        "{after}"
    );

    // -- distill_session: the free rules path reaches the daemon's distill
    // handler (HTTP 400 about the session, not a 404 route miss). A successful
    // run needs an enabled agent and a real transcript; that live path belongs
    // to the operator's probe, not to a fixture this test fabricates.
    let err = refusal(
        client
            .call_tool(call(
                "distill_session",
                serde_json::json!({
                    "session_key": "ruagent:0f3a9c7e5b1d2a44",
                    "extractor": "rules",
                    "dry_run": true
                }),
            ))
            .await,
    );
    assert!(err.contains("distill_session"), "{err}");
    assert!(err.contains("HTTP 400"), "{err}");

    // A bad extractor is refused before any request leaves the bridge.
    let err = refusal(
        client
            .call_tool(call(
                "distill_session",
                serde_json::json!({ "session_key": "ruagent:0f3a", "extractor": "llm" }),
            ))
            .await,
    );
    assert!(err.contains("model tokens"), "{err}");

    shutdown_pair(client, server_task).await;
}

/// t6 (wave C), part 2: the free knowledge→graph ingestion path against the
/// REAL route, including the fact that it is OFF by default.
///
/// The route is `POST /api/v1/knowledge/graph/ingest?document=<name>&dry_run=…`
/// (query parameters, no body — t4's frozen shape), and the capability refusal
/// is a 409 whose body names the capability and both remedies.
#[tokio::test]
async fn mcp_knowledge_graph_ingest_reaches_the_daemon() {
    let daemon_url = start_test_daemon().await;
    let (client, server_task) = mcp_pair(&daemon_url).await;

    // A document must exist before the graph ingest has anything to read.
    let out = client
        .call_tool(call(
            "knowledge_ingest",
            serde_json::json!({
                "name": "graph-notes",
                "content": "Kubernetes runs on the cluster. The release script lives in scripts/release.sh and runs from the repository root."
            }),
        ))
        .await
        .unwrap();
    assert!(text_of(&out).contains("ingested"), "{out:?}");

    // The pricing path works while the capability is OFF: counts, no writes.
    let out = client
        .call_tool(call(
            "knowledge_graph_ingest",
            serde_json::json!({ "document": "graph-notes", "dry_run": true }),
        ))
        .await
        .unwrap();
    let priced = text_of(&out);
    assert!(priced.contains("DRY RUN"), "{priced}");
    assert!(priced.contains("documents"), "{priced}");

    // A real write while the capability is off is REFUSED by name, with the
    // remedy in the message — never an opaque transport error.
    let err = refusal(
        client
            .call_tool(call(
                "knowledge_graph_ingest",
                serde_json::json!({ "document": "graph-notes", "dry_run": false }),
            ))
            .await,
    );
    assert!(err.contains("knowledge_ingest_graph"), "{err}");
    assert!(
        err.contains("enabled = true"),
        "the refusal names the exact enable action: {err}"
    );
    assert!(err.contains("capability_set"), "{err}");

    // Enable it through the MCP tool (free tier: no confirm_cost), then the real
    // write succeeds.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({ "id": "knowledge_ingest_graph", "enabled": true }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("`knowledge_ingest_graph` is now enabled=true"),
        "{out:?}"
    );
    let out = client
        .call_tool(call(
            "knowledge_graph_ingest",
            serde_json::json!({ "document": "graph-notes" }),
        ))
        .await
        .unwrap();
    let ingested = text_of(&out);
    assert!(ingested.contains("documents"), "{ingested}");
    assert!(!ingested.contains("DRY RUN"), "{ingested}");
    // The ledger's idempotence is t4's own contract (its counts on a re-run);
    // this test asserts the MCP seam, not the extractor's bookkeeping.

    shutdown_pair(client, server_task).await;
}

/// The raw body of one `[capabilities.<id>]` section of `policy.toml` — the FILE,
/// not the daemon's reply and not the tool's rendering of it.
fn capability_section(text: &str, id: &str) -> String {
    let header = format!("[capabilities.{id}]");
    let mut body = String::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == header;
            continue;
        }
        if inside && !trimmed.is_empty() {
            body.push_str(trimmed);
            body.push('\n');
        }
    }
    body
}

/// The KEY SET of one `[capabilities.<id>]` section, sorted (the editor's
/// insertion order is not the file's original order, so only the set is stable).
fn capability_section_keys(text: &str, id: &str) -> Vec<String> {
    let mut keys: Vec<String> = capability_section(text, id)
        .lines()
        .filter_map(|l| l.split('=').next())
        .map(|k| k.trim().to_string())
        .collect();
    keys.sort();
    keys
}

/// t10: the MCP capability write preserves the file's OWN key set.
///
/// Two properties are on trial here, and both were broken once elsewhere:
///
/// * the READ-MODIFY-WRITE — the daemon's PUT REPLACES the whole `[capabilities]`
///   table and removes every row and key an entry omits, so a one-id write must
///   carry every other configured row back (`CapabilitiesEditor::update`);
/// * NO RE-MATERIALIZATION — re-emitting the RESOLVED values instead persists
///   registry defaults into rows the user never set, which is the defect measured
///   on the panel side (`enabled = true` alone became `enabled = true` +
///   `weight = 1`).
///
/// The witness is `policy.toml`, read before and after each write: the file is what
/// the user owns, and a tool reply cannot testify about it.
#[tokio::test]
async fn capability_set_preserves_the_files_own_key_set() {
    let (daemon_url, root) = start_test_daemon_with_policy(
        "[permissions]\ndefault = \"ask\"\n\
         \n\
         [capabilities.recall_leg_memory_fts]\n\
         enabled = true\n\
         \n\
         [capabilities.recall_leg_memory_semantic]\n\
         weight = 3.5\n\
         \n\
         [capabilities.recall_leg_wiki]\n",
    )
    .await;
    let policy = root.join("config").join("policy.toml");
    let before = std::fs::read_to_string(&policy).unwrap();
    // The seed is the shape that exposes both defects: one row carries `enabled`
    // only (so `weight` resolves to its registry default 1.0), one carries
    // `weight` only (so `enabled` resolves to its default true), one is empty.
    assert_eq!(
        capability_section_keys(&before, "recall_leg_memory_fts"),
        vec!["enabled"]
    );
    assert_eq!(
        capability_section_keys(&before, "recall_leg_memory_semantic"),
        vec!["weight"]
    );

    let (client, server_task) = mcp_pair(&daemon_url).await;

    // The DECLARED key set is visible before anything is written. `min_score` on
    // the memory semantic leg declares no registry default, so there is no
    // resolved value to render — and the tool still names it, with the daemon's own
    // accepted-range phrase, which is what keeps it reachable from this surface.
    let out = client
        .call_tool(call("capabilities_list", serde_json::json!({})))
        .await
        .unwrap();
    let listed = text_of(&out);
    assert!(
        listed.contains(
            "min_score=- (declared float; the file does not carry it and the registry \
             declares no default; accepted finite and 0.0..=1.0)"
        ),
        "a declared key with no default must be nameable: {listed}"
    );
    assert!(
        listed.contains("weight=1.0 (float default, the file does not)"),
        "a defaulted value must not read as the file's: {listed}"
    );
    assert!(
        listed.contains("weight=3.5 (the file carries it)"),
        "a value the file carries must read as the file's: {listed}"
    );

    // Write about a capability the file does not mention at all.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({ "id": "knowledge_ingest_graph", "enabled": true }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("`knowledge_ingest_graph` is now enabled=true"),
        "{out:?}"
    );

    let after = std::fs::read_to_string(&policy).unwrap();
    assert_eq!(
        capability_section_keys(&after, "knowledge_ingest_graph"),
        vec!["enabled"],
        "the requested row landed: {after}"
    );
    for id in [
        "recall_leg_memory_fts",
        "recall_leg_memory_semantic",
        "recall_leg_wiki",
    ] {
        assert_eq!(
            capability_section(&after, id),
            capability_section(&before, id),
            "row `{id}` was rewritten by a write that did not name it: {after}"
        );
        assert_eq!(
            capability_section_keys(&after, id),
            capability_section_keys(&before, id),
            "row `{id}` changed KEY SET: {after}"
        );
    }
    assert!(
        !capability_section(&after, "recall_leg_memory_fts").contains("weight"),
        "a RESOLVED default (weight = 1.0) was materialized into the user's file: {after}"
    );

    // A SECOND write, this time with values, to the semantic leg: the declared key
    // with no default becomes settable through MCP, the file's own `weight` is
    // updated, and NOTHING else appears — `enabled` is written only because the
    // caller set it (it is a required tool argument), never as a default.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({
                "id": "recall_leg_memory_semantic",
                "enabled": true,
                "weight": 2.5,
                "min_score": 0.4
            }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("`recall_leg_memory_semantic` is now enabled=true"),
        "{out:?}"
    );
    let after_two = std::fs::read_to_string(&policy).unwrap();
    assert_eq!(
        capability_section_keys(&after_two, "recall_leg_memory_semantic"),
        vec!["enabled", "min_score", "weight"],
        "only the keys the caller asked for: {after_two}"
    );
    let semantic = capability_section(&after_two, "recall_leg_memory_semantic");
    assert!(semantic.contains("min_score = 0.4"), "{after_two}");
    assert!(semantic.contains("weight = 2.5"), "{after_two}");
    for id in ["recall_leg_memory_fts", "recall_leg_wiki"] {
        assert_eq!(
            capability_section(&after_two, id),
            capability_section(&before, id),
            "row `{id}` survived a second write untouched: {after_two}"
        );
    }
    assert_eq!(
        capability_section(&after_two, "knowledge_ingest_graph"),
        capability_section(&after, "knowledge_ingest_graph"),
        "the first write's row survived the second: {after_two}"
    );

    // The DAEMON's own refusal still reaches the caller verbatim: a key this
    // capability does not declare is refused with the daemon's sentence and its
    // accepted-key list, not with a locally invented message.
    let err = refusal(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({
                    "id": "recall_leg_wiki",
                    "enabled": true,
                    "weight": 2.0
                }),
            ))
            .await,
    );
    assert!(
        err.contains("capability `recall_leg_wiki` does not accept the option `weight`"),
        "the daemon's own words: {err}"
    );
    assert!(err.contains("it accepts: none"), "{err}");

    // A VALUE the daemon refuses is likewise its own 400 phrase, verbatim: this
    // bridge pre-validates nothing (a second range check here could drift from the
    // registry's), so the range the agent reads is the range the daemon enforces.
    let err = refusal(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({
                    "id": "session_extract_rules",
                    "enabled": true,
                    "max_per_input": 0
                }),
            ))
            .await,
    );
    assert!(
        err.contains(
            "capability `session_extract_rules`: `max_per_input = 0` is out of range (1..=10000)"
        ),
        "the daemon's own range phrase: {err}"
    );

    // ... and the refused write left the file exactly as it was.
    assert_eq!(
        std::fs::read_to_string(&policy).unwrap(),
        after_two,
        "a refused write must not touch the file"
    );

    shutdown_pair(client, server_task).await;
}

/// t17: a field this tool cannot SEND is refused, and nothing is written.
///
/// The measured defect this closes: `capability_set(id=recall_leg_memory_semantic,
/// enabled=true, decay_half_life=5.0)` answered SUCCESS while serde dropped the key
/// before the bridge saw it — the caller was told the update happened, and the key
/// appeared neither in the reply nor in `policy.toml`. The FILE is the witness here:
/// a refusal must leave it byte-identical, and the control call through the SAME
/// daemon shows the tool still writes when every field is one it can send.
#[tokio::test]
async fn capability_set_refuses_a_field_it_cannot_send() {
    let (daemon_url, root) = start_test_daemon_with_policy(
        "[permissions]\ndefault = \"ask\"\n\n[capabilities.recall_leg_memory_semantic]\nweight = 3.5\n",
    )
    .await;
    let policy = root.join("config").join("policy.toml");
    let before = std::fs::read_to_string(&policy).unwrap();
    let (client, server_task) = mcp_pair(&daemon_url).await;

    // The refused call: no success result exists, so nothing can have claimed the
    // update happened.
    let err = failure(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({
                    "id": "recall_leg_memory_semantic",
                    "enabled": true,
                    "decay_half_life": 5.0
                }),
            ))
            .await,
    );
    assert!(err.contains("unknown field `decay_half_life`"), "{err}");
    assert!(
        !err.contains("is now enabled"),
        "the caller must not be told the change happened: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(&policy).unwrap(),
        before,
        "the refused call must leave policy.toml untouched"
    );

    // CONTROL through the same daemon: with only fields the tool can send, the write
    // lands — the refusal above is about the field, not about the tool being broken.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({
                "id": "recall_leg_memory_semantic",
                "enabled": true,
                "min_score": 0.4
            }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("`recall_leg_memory_semantic` is now enabled=true"),
        "{out:?}"
    );
    let after = std::fs::read_to_string(&policy).unwrap();
    assert_ne!(after, before, "the control write must land in the file");
    assert_eq!(
        capability_section_keys(&after, "recall_leg_memory_semantic"),
        vec!["enabled", "min_score", "weight"],
        "the control write adds exactly what the caller set: {after}"
    );

    shutdown_pair(client, server_task).await;
}
