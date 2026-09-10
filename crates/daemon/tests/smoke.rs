//! Real-harness smoke tests — LOCAL, MANUAL, NEVER IN CI (design §12.5).
//!
//! Requires the real CLIs installed and authenticated:
//! `claude` (npm), `opencode`, `dsh`.
//!
//! Run:
//! ```text
//! cargo test -p ruagent-daemon --features smoke --test smoke -- --nocapture --test-threads=1
//! ```
//!
//! Costs real tokens; prompts are deliberately tiny.

#![cfg(feature = "smoke")]

use std::sync::Arc;
use std::time::Duration;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

const REAL_AGENTS_TOML: &str = r#"
[agent.claude]
harness = "claude-code"
command = "npx @agentclientprotocol/claude-agent-acp"
description = "Claude Code via the official ACP adapter"

[agent.opencode]
harness = "opencode"
command = "opencode acp"
description = "OpenCode (native ACP)"

[agent.dsh]
harness = "dsh"
command = "dsh --profile acp"
description = "DeepSeek Harness (native ACP)"
"#;

async fn start_daemon() -> String {
    static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-smoke-{}-{}", std::process::id(), seq));
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("agents.toml"), REAL_AGENTS_TOML).unwrap();
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
    let mgr = Arc::new(RunManager::new(
        db,
        root,
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));
    let app = ruagent_daemon::api::router(AppState {
        mgr,
        config: Arc::new(cfg),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

/// Drive one tiny prompt through a real harness and return the agent text.
async fn smoke(url: &str, agent: &str) -> String {
    let http = reqwest::Client::new();
    let task: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks"))
        .json(&serde_json::json!({ "title": "smoke", "intent": "say ok" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap();
    let run: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks/{task_id}/runs"))
        .json(&serde_json::json!({ "agent": agent, "prompt": "Reply with exactly: ok" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();

    let sse = http
        .get(format!("{url}/api/v1/runs/{run_id}/events"))
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let mut text = String::new();
    for line in sse.lines() {
        if let Some(data) = line.strip_prefix("data: ")
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(data)
            && v["event"]["type"] == "agent_message_chunk"
            && let Some(t) = v["event"]["content"][0]["text"].as_str()
        {
            text.push_str(t);
        }
    }
    text
}

#[tokio::test]
async fn smoke_opencode() {
    let url = start_daemon().await;
    let text = smoke(&url, "opencode").await;
    assert!(text.to_lowercase().contains("ok"), "got: {text:?}");
}

#[tokio::test]
async fn smoke_dsh() {
    let url = start_daemon().await;
    let text = smoke(&url, "dsh").await;
    assert!(text.to_lowercase().contains("ok"), "got: {text:?}");
}

#[tokio::test]
async fn smoke_claude() {
    let url = start_daemon().await;
    let text = smoke(&url, "claude").await;
    assert!(text.to_lowercase().contains("ok"), "got: {text:?}");
}
