//! Registry editing API: create/update/delete runtimes and roles over
//! HTTP against an in-process daemon started with an EMPTY agents.toml —
//! every card in this test is created through the API, which exercises
//! the full write → re-parse → hot-reload loop without a restart.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

async fn start_daemon() -> (String, std::path::PathBuf) {
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-regapi-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    // Empty registry: everything gets created through the API.
    std::fs::write(config_dir.join("agents.toml"), "").unwrap();
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

async fn agent_names(http: &reqwest::Client, url: &str) -> Vec<(String, String, bool)> {
    let v: serde_json::Value = http
        .get(format!("{url}/api/v1/agents"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["name"].as_str().unwrap().to_string(),
                a["kind"].as_str().unwrap().to_string(),
                a["enabled"].as_bool().unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn runtime_and_role_crud_with_hot_reload() {
    let (url, root) = start_daemon().await;
    let http = reqwest::Client::new();
    let toml_path = root.join("config").join("agents.toml");

    // 1. Create a runtime from nothing (empty agents.toml).
    let resp = http
        .post(format!("{url}/api/v1/runtimes"))
        .json(&serde_json::json!({
            "name": "test-rt",
            "harness": "mock",
            "command": "ruagent-mock-agent --behavior echo",
            "description": "created by the registry test",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let card: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(card["name"], "test-rt");
    assert_eq!(card["kind"], "runtime");

    // Visible in the registry WITHOUT a restart.
    let names = agent_names(&http, &url).await;
    assert_eq!(names, vec![("test-rt".into(), "runtime".into(), true)]);

    // The file on disk is valid TOML a human could have written.
    let text = std::fs::read_to_string(&toml_path).unwrap();
    assert!(text.contains("[runtime.test-rt]"));
    assert!(text.contains("ruagent-mock-agent --behavior echo"));

    // 2. Create a role on it.
    let resp = http
        .post(format!("{url}/api/v1/agents"))
        .json(&serde_json::json!({
            "name": "tester",
            "prompt": "You test things.",
            "description": "the registry test role",
            "runtimes": ["test-rt"],
            "runtime": "test-rt",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let card: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(card["kind"], "role");
    assert_eq!(card["runtime"], "test-rt");

    let names = agent_names(&http, &url).await;
    assert!(names.contains(&("tester".into(), "role".into(), true)));

    // 3. Update the role's prompt.
    let resp = http
        .patch(format!("{url}/api/v1/agents/tester"))
        .json(&serde_json::json!({ "prompt": "You test things harder." }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let card: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(card["prompt"], "You test things harder.");

    // 4. The runtime is in use — deletion is a conflict.
    let resp = http
        .delete(format!("{url}/api/v1/runtimes/test-rt"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    assert!(resp.text().await.unwrap().contains("tester"));

    // 5. A role referencing an unknown runtime never touches the file.
    let before = std::fs::read_to_string(&toml_path).unwrap();
    let resp = http
        .post(format!("{url}/api/v1/agents"))
        .json(&serde_json::json!({
            "name": "bad",
            "prompt": "x",
            "runtimes": ["nope"],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    assert_eq!(std::fs::read_to_string(&toml_path).unwrap(), before);

    // 6. Clean deletes: role first, then the runtime.
    let resp = http
        .delete(format!("{url}/api/v1/agents/tester"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    let resp = http
        .delete(format!("{url}/api/v1/runtimes/test-rt"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    assert!(agent_names(&http, &url).await.is_empty());

    // Deleting something that is already gone is a 404.
    let resp = http
        .delete(format!("{url}/api/v1/runtimes/test-rt"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn runtime_validation_rejects_bad_input() {
    let (url, _root) = start_daemon().await;
    let http = reqwest::Client::new();

    // Unknown harness.
    let resp = http
        .post(format!("{url}/api/v1/runtimes"))
        .json(&serde_json::json!({
            "name": "bad-harness",
            "harness": "does-not-exist",
            "command": "x",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Missing command.
    let resp = http
        .post(format!("{url}/api/v1/runtimes"))
        .json(&serde_json::json!({ "name": "no-command", "harness": "mock" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Bad name (TOML key / URL path safety).
    let resp = http
        .post(format!("{url}/api/v1/runtimes"))
        .json(&serde_json::json!({ "name": "a.b", "command": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // A role without a prompt is not a role.
    let resp = http
        .post(format!("{url}/api/v1/agents"))
        .json(&serde_json::json!({ "name": "no-prompt" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
