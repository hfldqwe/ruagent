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
    std::fs::write(
        config_dir.join("policy.toml"),
        "[permissions]\ndefault = \"ask\"\n",
    )
    .unwrap();

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
    let mgr = Arc::new(RunManager::new(
        db.clone(),
        root.clone(),
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));
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
    format!("http://{addr}")
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
    for expected in [
        "memory_search",
        "memory_write",
        "memory_recall",
        "memory_get",
        "knowledge_search",
        "knowledge_ingest",
        "knowledge_expand",
        "graph_entity",
        "list_tasks",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing {expected}: {names:?}"
        );
    }

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
