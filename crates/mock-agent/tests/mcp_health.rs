//! MCP health checks (issue #24): a real rmcp client roundtrips against
//! the hand-rolled mock-mcp server, a broken server reports down, the
//! registry API exposes both, and injection excludes the down server.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

/// Binary paths with forward slashes — safe to embed in TOML strings.
fn mock_mcp() -> String {
    env!("CARGO_BIN_EXE_ruagent-mock-mcp").replace('\\', "/")
}
fn mock_agent() -> String {
    env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/")
}

static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

async fn start_daemon() -> (String, std::path::PathBuf) {
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-mcphealth-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("agents.toml"),
        format!(
            r#"[agent.probe]
harness = "mock"
command = "{mock_cmd} --behavior configdump"
description = "reports injected config"
mcp_profile = "default"
"#,
            mock_cmd = mock_agent()
        ),
    )
    .unwrap();
    // One healthy server (the hand-rolled MCP stdio server) and one
    // whose command cannot exist — the profile injects both.
    std::fs::write(
        config_dir.join("mcp.toml"),
        format!(
            r#"[mcp.good]
command = "{mcp_cmd}"

[mcp.bad]
command = "definitely-missing-mcp-binary-xyz"

[profile.default]
servers = ["good", "bad"]
"#,
            mcp_cmd = mock_mcp()
        ),
    )
    .unwrap();
    std::fs::write(
        config_dir.join("policy.toml"),
        "[permissions]\ndefault = \"ask\"\n",
    )
    .unwrap();

    let cfg = DaemonConfig::load(&root).unwrap();
    // The in-process daemon does not run lib.rs's boot loop — run the
    // first health pass synchronously, exactly as the loop would.
    cfg.mcp.health.refresh(&cfg.mcp).await;

    let db = Db::open(root.join("data").join("ruagent.db")).unwrap();
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
        cfg.agents.clone(),
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
    (format!("http://{addr}"), root)
}

#[tokio::test]
async fn health_check_reports_and_gates_injection() {
    let (url, root) = start_daemon().await;
    let http = reqwest::Client::new();

    // The registry API carries the live state: good answers with its
    // two tools, bad is down with the spawn error.
    let v: serde_json::Value = http
        .get(format!("{url}/api/v1/mcp"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let servers: Vec<_> = v["servers"].as_array().unwrap().clone();
    let good = servers.iter().find(|s| s["name"] == "good").unwrap();
    assert_eq!(good["health"]["state"], "ok", "good server: {good}");
    assert_eq!(good["health"]["tools"], 2, "mock-mcp serves two tools");
    assert!(good["health"]["latency_ms"].as_u64().is_some());
    let bad = servers.iter().find(|s| s["name"] == "bad").unwrap();
    assert_eq!(bad["health"]["state"], "down", "bad server: {bad}");
    assert!(
        bad["health"]["error"].as_str().unwrap().contains("spawn"),
        "down reason should name the spawn failure: {bad}"
    );

    // Injection gate: a chat on the probe agent reports the injected
    // servers — only `good` makes it through.
    let chat: serde_json::Value = http
        .post(format!("{url}/api/v1/chat"))
        .json(&serde_json::json!({ "agent": "probe" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = chat["id"].as_str().unwrap().to_string();
    http.post(format!("{url}/api/v1/chat/{id}/messages"))
        .json(&serde_json::json!({ "text": "report" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // The configdump reply lands in the chat transcript (the live SSE
    // stream never ends while the chat is open — poll the file).
    let transcript = root
        .join("data")
        .join("transcripts")
        .join(format!("run-{id}.jsonl"));
    let mut report = String::new();
    for _ in 0..50 {
        if let Ok(text) = std::fs::read_to_string(&transcript) {
            report = text
                .lines()
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .filter_map(|v| {
                    let ev = v.get("event")?;
                    (ev["type"] == "agent_message_chunk")
                        .then(|| ev["content"][0]["text"].as_str().unwrap_or("").to_string())
                })
                .collect::<Vec<_>>()
                .join("");
            if report.contains("mcp=") {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(
        report.contains("mcp=good"),
        "only the healthy server may be injected, got: {report}"
    );
    assert!(
        !report.contains("bad"),
        "the down server must be excluded from injection, got: {report}"
    );

    let _ = http.delete(format!("{url}/api/v1/chat/{id}")).send().await;
}
