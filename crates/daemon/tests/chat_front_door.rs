//! t74: `/chat` with no `agent` must land on the FRONT DOOR, and a runtime that cannot start must
//! produce an actionable sentence (which runtime, which command is missing, who CAN answer, and what
//! to do) instead of `invalid agent command `program `npx` ... not found on PATH`.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

const NEEDS_MOCK: &str =
    "the mock ACP agent is missing: cargo build -p ruagent-mock-agent --bins (CI builds it)";

fn mock_bin() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let profile_dir = exe.parent()?.parent()?;
    for name in ["ruagent-mock-agent.exe", "ruagent-mock-agent"] {
        let path = profile_dir.join(name);
        if path.is_file() {
            return Some(path.display().to_string());
        }
    }
    None
}

/// A daemon over a PRIVATE root whose `agents.toml` is the caller's text.
async fn boot(tag: &str, agents_toml: &str) -> String {
    let root = std::env::temp_dir().join(format!("ruagent-chatfd-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(root.join("data")).unwrap();
    std::fs::write(config_dir.join("agents.toml"), agents_toml).unwrap();

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
        ruagent_daemon::distill::AutoDistill {
            auto: cfg.policy.distill.auto,
            agent: cfg.policy.distill.agent.clone(),
            language: cfg.policy.distill.language.clone(),
            prompt: cfg.policy.distill.prompt.clone(),
            graph: cfg.policy.distill.graph.unwrap_or(true),
        },
        None,
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    chats.set_capabilities(cfg.capabilities.clone());
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

async fn post_chat(base: &str, body: serde_json::Value) -> (u16, String) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/v1/chat"))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.text().await.unwrap())
}

/// (A) no `agent`: the FRONT DOOR answers, and the response says WHO answered.
#[tokio::test]
async fn omitting_the_agent_lands_on_the_front_door_and_names_the_agent() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let base = boot(
        "front",
        &format!("[runtime.front]\nharness = \"mock\"\ncommand = '{mock} --behavior echo'\ndescription = \"front\"\nprimary = true\n"),
    )
    .await;

    let (status, body) = post_chat(&base, serde_json::json!({})).await;
    println!("READING t74 chat without agent: {status} {body}");
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json["agent"], "front",
        "the response names the agent that answered: {body}"
    );

    // an EXPLICIT name still means exactly what it meant before
    let (status, body) = post_chat(&base, serde_json::json!({ "agent": "front" })).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = post_chat(&base, serde_json::json!({ "agent": "nope" })).await;
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("unknown agent"), "{body}");
}

/// (B) and the ACCEPTANCE shape: a front door that cannot start names what is missing, lists the
/// runtimes that CAN start, gives the two edits, and has no nested backticks.
#[tokio::test]
async fn an_unstartable_front_door_names_the_missing_command_and_who_can_answer() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let base = boot(
        "partial",
        &format!(
            "[runtime.ghost]\nharness = \"mock\"\ncommand = 'definitely-not-installed-xyz'\ndescription = \"ghost\"\nprimary = true\n\n\
             [runtime.helper]\nharness = \"mock\"\ncommand = '{mock} --behavior echo'\ndescription = \"helper\"\n"
        ),
    )
    .await;

    // omitted: the front door IS the ghost, so this is the front-door-unusable case
    let (status, body) = post_chat(&base, serde_json::json!({})).await;
    println!("READING t74 front door unusable: {status} {body}");
    assert_eq!(
        status, 400,
        "a configuration fault is a 4xx, not a 500: {body}"
    );
    assert!(
        body.contains("ghost"),
        "the message names the runtime that cannot start: {body}"
    );
    assert!(
        body.contains("definitely-not-installed-xyz"),
        "and the program that is missing: {body}"
    );
    assert!(
        body.contains("helper"),
        "and a runtime that CAN start right now: {body}"
    );
    assert!(
        body.contains("PUT /api/v1/primary"),
        "and the action that would switch to it: {body}"
    );
    assert!(
        body.contains("command"),
        "and the other action (edit the command): {body}"
    );
    assert!(
        !body.contains("``"),
        "no empty/nested backticks, which is what the old text had: {body}"
    );
    assert!(
        !body.contains("invalid agent command"),
        "the old raw text is gone: {body}"
    );
}

/// (B, all unusable) when NOTHING on this machine can start, the message has to say so and point at
/// the installation step instead of blaming a field name.
#[tokio::test]
async fn when_no_runtime_can_start_the_message_says_so() {
    let base = boot(
        "none",
        "[runtime.ghost]\nharness = \"mock\"\ncommand = 'definitely-not-installed-xyz'\ndescription = \"ghost\"\nprimary = true\n\n\
         [runtime.also-ghost]\nharness = \"mock\"\ncommand = 'also-not-installed-abc'\ndescription = \"also\"\n",
    )
    .await;

    let (status, body) = post_chat(&base, serde_json::json!({})).await;
    println!("READING t74 nothing usable: {status} {body}");
    assert_eq!(status, 400, "{body}");
    assert!(
        body.contains("没有任何运行时能启动"),
        "it says plainly that nothing can start: {body}"
    );
    assert!(
        !body.contains("此刻能启动的运行时"),
        "and does not claim some runtime works: {body}"
    );
    assert!(body.contains("ruagent doctor"), "{body}");
    assert!(!body.contains("``"), "{body}");
}
