//! t77: `GET /api/v1/mcp` must carry the command that will ACTUALLY be spawned, not only the
//! configured one, or a checker goes red on the healthiest possible machine: the platform's own MCP
//! server is started from the daemon's OWN executable when the bare name `ruagent` is not on PATH, so
//! `health ok / tools 21` and `command = "ruagent"` are true at the same time.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

const AGENTS: &str = "[agent.alpha]\nharness = \"mock\"\ncommand = \"cargo\"\ndescription = \"alpha\"\nprompt = \"alpha\"\n";

/// The platform's own server (bare `ruagent`, `mcp-serve`) PLUS a user's own server, in one profile.
const MCP_TOML: &str = r#"
[mcp.ruagent]
command = "ruagent"
args = ["mcp-serve"]

[mcp.mine]
command = "some-user-mcp-binary-9f3a"
args = ["--stdio"]

[profile.default]
servers = ["ruagent", "mine"]
"#;

async fn boot(tag: &str) -> String {
    let root = std::env::temp_dir().join(format!("ruagent-mcpeff-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(root.join("data")).unwrap();
    std::fs::write(config_dir.join("agents.toml"), AGENTS).unwrap();
    std::fs::write(config_dir.join("mcp.toml"), MCP_TOML).unwrap();

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

/// The substitution under test only happens when the bare name does NOT resolve, so the test must MAKE
/// that true instead of hoping the machine has no `ruagent` on PATH (CI may have one).
struct NoRuagentOnPath(Option<std::ffi::OsString>);

impl NoRuagentOnPath {
    fn install() -> Self {
        let old = std::env::var_os("PATH");
        let empty = std::env::temp_dir().join(format!("ruagent-emptypath-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&empty);
        // SAFETY: this test binary runs one test at a time here (the only test), and the original is
        // restored before it returns.
        unsafe { std::env::set_var("PATH", &empty) };
        Self(old)
    }
}

impl Drop for NoRuagentOnPath {
    fn drop(&mut self) {
        unsafe { std::env::set_var("PATH", self.0.clone().unwrap_or_default()) };
    }
}

#[tokio::test]
async fn the_report_carries_the_command_that_will_actually_be_spawned() {
    let base = boot("default").await;
    let _no_ruagent = NoRuagentOnPath::install();

    let resp = reqwest::Client::new()
        .get(format!("{base}/api/v1/mcp"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    println!("READING t77 GET /api/v1/mcp: {body}");

    let server = |name: &str| {
        body["servers"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["name"] == name))
            .cloned()
            .unwrap_or_else(|| panic!("no server `{name}` in {body}"))
    };

    // the PLATFORM's own server: configured name unchanged, effective command is the daemon's own exe
    let own = server("ruagent");
    assert_eq!(
        own["command"], "ruagent",
        "`command` must stay the CONFIGURED value -- that is the semantics this task protects: {own}"
    );
    let effective = own["command_effective"]
        .as_str()
        .unwrap_or_else(|| panic!("no command_effective for the platform server: {own}"));
    assert_ne!(
        effective, "ruagent",
        "it must not just echo the name: {own}"
    );
    let path = std::path::Path::new(effective);
    assert!(
        path.is_absolute(),
        "the effective command is the daemon's own executable, i.e. absolute: {own}"
    );
    assert!(path.is_file(), "and it exists: {own}");

    // a USER's own server is never rewritten
    let mine = server("mine");
    assert_eq!(mine["command"], "some-user-mcp-binary-9f3a", "{mine}");
    assert_eq!(
        mine["command_effective"], "some-user-mcp-binary-9f3a",
        "a user's command is reported as configured -- the daemon may not guess what it meant: {mine}"
    );
}
