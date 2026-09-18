//! Chat experience round (2026-09-18): option-catalog persistence,
//! chat history with role attribution, runtime-switch identity, and
//! role option defaults — all against the mock agent over real ACP.

use std::sync::Arc;

/// Absolute path to the built mock binary (the CARGO_BIN_EXE pattern —
/// a bare name would only resolve where ~/.cargo/bin happens to have
/// it installed, which is exactly nowhere in CI).
const MOCK_BIN: &str = env!("CARGO_BIN_EXE_ruagent-mock-agent");

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

async fn start_daemon() -> (String, std::path::PathBuf) {
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-chatexp-{}-{seq}", std::process::id()));
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

async fn create_runtime(http: &reqwest::Client, url: &str, name: &str, cmd: &str) {
    let resp = http
        .post(format!("{url}/api/v1/runtimes"))
        .json(&serde_json::json!({
            "name": name,
            "harness": "mock",
            "command": cmd,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "runtime create failed");
}

async fn create_role(http: &reqwest::Client, url: &str, body: serde_json::Value) {
    let resp = http
        .post(format!("{url}/api/v1/agents"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "role create failed: {body}");
}

/// Poll until a condition on a JSON value holds (bounded).
async fn poll_json<F>(http: &reqwest::Client, url: &str, f: F, what: &str) -> serde_json::Value
where
    F: Fn(&serde_json::Value) -> bool,
{
    for _ in 0..100 {
        let v: serde_json::Value = http.get(url).send().await.unwrap().json().await.unwrap();
        if f(&v) {
            return v;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for: {what}");
}

/// The mock runtime's option catalog is probed once, then served from
/// cache; the persisted copy survives a daemon restart (fresh
/// ChatManager seeds its memory cache from `agent_options`).
#[tokio::test]
async fn option_catalog_probes_once_then_persists() {
    let (url, root) = start_daemon().await;
    let http = reqwest::Client::new();
    create_runtime(&http, &url, "rt", &format!("{MOCK_BIN} --behavior echo")).await;

    // First read probes (a real mock-agent process spawns).
    let v = poll_json(
        &http,
        &format!("{url}/api/v1/agents/rt/options"),
        |v| v["options"].as_array().is_some_and(|a| !a.is_empty()),
        "options probe",
    )
    .await;
    assert_eq!(v["cached"], false);
    assert_eq!(v["options"].as_array().unwrap().len(), 3);
    let model = v["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == "model")
        .unwrap();
    assert_eq!(model["current"], "mock-pro");

    // Second read: cached, instant.
    let v: serde_json::Value = http
        .get(format!("{url}/api/v1/agents/rt/options"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["cached"], true);

    // The persisted row exists.
    let db = Db::open(root.join("data").join("ruagent.db")).unwrap();
    let n: Result<Result<i64, ruagent_store::DbError>, _> = db
        .call(|conn| -> Result<i64, ruagent_store::DbError> {
            conn.query_row(
                "SELECT COUNT(*) FROM agent_options WHERE runtime = 'rt'",
                [],
                |r| r.get(0),
            )
            .map_err(ruagent_store::DbError::from)
        })
        .await;
    assert_eq!(n.unwrap().unwrap(), 1);

    // A fresh ChatManager (daemon restart) seeds from the table and
    // serves cached without spawning anything.
    let chats = ruagent_daemon::chat::ChatManager::new(
        db,
        root.clone(),
        Arc::new(|_, _| {}),
        ruagent_daemon::config::DaemonConfig::load(&root)
            .unwrap()
            .mcp,
        ruagent_daemon::distill::AutoDistill::default(),
        None,
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    chats.load_option_cache().await;
    let cfg = DaemonConfig::load(&root).unwrap();
    let card = cfg.agents.iter().find(|c| c.name == "rt").unwrap().clone();
    let (entry, cached) = chats.agent_options(&card).await.unwrap();
    assert!(cached, "restart must serve the persisted catalog");
    assert!(entry.options.iter().any(|o| o.id == "model"));
}

/// Chats are recorded with their role identity, title from the first
/// prompt, and current engine; runtime switches keep the agent label,
/// and a model-only switch after one stays on the CURRENT engine (it
/// must not fall back to the role's default runtime).
#[tokio::test]
async fn chat_history_and_runtime_switch_identity() {
    let (url, _root) = start_daemon().await;
    let http = reqwest::Client::new();
    create_runtime(&http, &url, "rt-a", &format!("{MOCK_BIN} --behavior echo")).await;
    create_runtime(&http, &url, "rt-b", &format!("{MOCK_BIN} --behavior echo")).await;
    create_role(
        &http,
        &url,
        serde_json::json!({
            "name": "tester",
            "prompt": "you are the tester",
            "runtimes": ["rt-a", "rt-b"],
            "runtime": "rt-a",
            "model": "mock-max",
        }),
    )
    .await;

    // Default model: the role's configured one, no explicit pick needed.
    let chat: serde_json::Value = http
        .post(format!("{url}/api/v1/chat"))
        .json(&serde_json::json!({ "agent": "tester" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = chat["id"].as_str().unwrap().to_string();
    assert_eq!(chat["agent"], "tester");
    assert_eq!(chat["runtime"], "rt-a");
    assert_eq!(chat["model"], "mock-max");

    // First prompt becomes the history title.
    http.post(format!("{url}/api/v1/chat/{id}/messages"))
        .json(&serde_json::json!({ "text": "hello history" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let history = poll_json(
        &http,
        &format!("{url}/api/v1/chats?agent=tester"),
        |v| v["chats"].as_array().is_some_and(|a| !a.is_empty()),
        "history row",
    )
    .await;
    let row = &history["chats"][0];
    assert_eq!(row["agent"], "tester");
    assert_eq!(row["runtime"], "rt-a");
    assert_eq!(row["title"], "hello history");
    assert_eq!(row["active"], true);

    // Runtime switch: same agent identity, new engine.
    let switched: serde_json::Value = http
        .patch(format!("{url}/api/v1/chat/{id}"))
        .json(&serde_json::json!({ "runtime": "rt-b" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(switched["switched"], "restarted");
    assert_eq!(switched["agent"], "tester");
    assert_eq!(switched["runtime"], "rt-b");
    let id2 = switched["id"].as_str().unwrap().to_string();

    // Model-only switch on the switched chat: stays on rt-b (the
    // current engine), not the role default rt-a.
    let switched2: serde_json::Value = http
        .patch(format!("{url}/api/v1/chat/{id2}"))
        .json(&serde_json::json!({ "model": "mock-pro" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        switched2["runtime"], "rt-b",
        "model switch must keep the current engine"
    );
    assert_eq!(switched2["agent"], "tester");

    // Options can be read for a DIFFERENT engine than the role's
    // default (issue #37): the runtime override addresses a runtime
    // card; roles and unknown names are rejected.
    let resp = http
        .get(format!("{url}/api/v1/agents/tester/options?runtime=rt-b"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: serde_json::Value = resp.json().await.unwrap();
    assert!(
        v["options"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["id"] == "model")
    );
    for bad in ["nope", "tester"] {
        let resp = http
            .get(format!("{url}/api/v1/agents/tester/options?runtime={bad}"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "runtime={bad} must be rejected");
    }

    // History: both rows attributed to the role.
    let history = poll_json(
        &http,
        &format!("{url}/api/v1/chats?agent=tester"),
        |v| v["chats"].as_array().is_some_and(|a| a.len() >= 2),
        "two history rows",
    )
    .await;
    let rows = history["chats"].as_array().unwrap();
    assert!(rows.iter().all(|r| r["agent"] == "tester"));
    assert!(rows.iter().any(|r| r["runtime"] == "rt-b"));

    // Cleanup the live chats so the runtime processes exit.
    for cid in [&id, &id2] {
        let _ = http.delete(format!("{url}/api/v1/chat/{cid}")).send().await;
    }
    let _ = id;
}

/// Role option defaults (`[agent.X.options]`, canonical keys) are
/// applied onto the runtime's advertised options at chat start.
#[tokio::test]
async fn role_option_defaults_apply() {
    let (url, _root) = start_daemon().await;
    let http = reqwest::Client::new();
    create_runtime(&http, &url, "rt", &format!("{MOCK_BIN} --behavior echo")).await;
    create_role(
        &http,
        &url,
        serde_json::json!({
            "name": "planner",
            "prompt": "you plan",
            "runtimes": ["rt"],
            "runtime": "rt",
            "options": { "mode": "auto", "effort": "high" },
        }),
    )
    .await;

    let chat: serde_json::Value = http
        .post(format!("{url}/api/v1/chat"))
        .json(&serde_json::json!({ "agent": "planner" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = chat["id"].as_str().unwrap().to_string();

    // The defaults land on the live chat's advertised state (refresh
    // reads from the live session). mode → "auto", effort → "high".
    let v = poll_json(
        &http,
        &format!("{url}/api/v1/agents/rt/options?refresh=1"),
        |v| {
            v["options"].as_array().is_some_and(|opts| {
                opts.iter()
                    .find(|o| o["id"] == "mode")
                    .is_some_and(|o| o["current"] == "auto")
                    && opts
                        .iter()
                        .find(|o| o["id"] == "reasoning_effort")
                        .is_some_and(|o| o["current"] == "high")
            })
        },
        "role defaults applied",
    )
    .await;
    let options = v["options"].as_array().unwrap();
    let mode = options.iter().find(|o| o["id"] == "mode").unwrap();
    assert_eq!(mode["current"], "auto");
    let effort = options
        .iter()
        .find(|o| o["id"] == "reasoning_effort")
        .unwrap();
    assert_eq!(effort["current"], "high");

    let _ = http.delete(format!("{url}/api/v1/chat/{id}")).send().await;
}
