//! t79: a fresh install's seed defines runtimes as `[agent.<name>]`, while the read paths (panel
//! Runtimes page, `ruagent doctor`, `GET /api/v1/agents`, `POST /api/v1/runtimes/{name}/probe`) all
//! present them as runtimes. The write path must reach the SAME cards the read path shows -- before
//! this, `PATCH /api/v1/runtimes/dsh` answered 404 on a brand-new root, so the panel's Edit button and
//! the README's `curl -X PATCH` recipe were dead exactly where the user needed them.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

/// The shape a FRESH install ships: runtimes as `[agent.<name>]` (no `[runtime.*]` at all).
const SEED_SHAPE: &str = "
[agent.dsh]
harness = \"dsh\"
command = \"dsh --profile acp\"
description = \"dsh\"

[agent.mock]
harness = \"mock\"
command = \"cargo\"
description = \"mock\"
prompt = \"mock\"
primary = true
\n";

/// The LEGACY shape: an explicit `[runtime.<name>]` layer plus one agent.
const RUNTIME_SHAPE: &str = "
[runtime.legacy]
harness = \"mock\"
command = \"cargo\"
description = \"legacy\"

[agent.alpha]
harness = \"mock\"
command = \"cargo\"
description = \"alpha\"
prompt = \"alpha\"
primary = true
\n";

struct Booted {
    base: String,
    agents_toml: std::path::PathBuf,
}

async fn boot(tag: &str, agents_toml: &str) -> Booted {
    let root = std::env::temp_dir().join(format!("ruagent-t79-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(root.join("data")).unwrap();
    let path = config_dir.join("agents.toml");
    std::fs::write(&path, agents_toml).unwrap();

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
    Booted {
        base: format!("http://{addr}"),
        agents_toml: path,
    }
}

async fn patch(base: &str, name: &str, body: serde_json::Value) -> (u16, String) {
    let resp = reqwest::Client::new()
        .patch(format!("{base}/api/v1/runtimes/{name}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    (resp.status().as_u16(), resp.text().await.unwrap())
}

async fn delete(base: &str, name: &str) -> (u16, String) {
    let resp = reqwest::Client::new()
        .delete(format!("{base}/api/v1/runtimes/{name}"))
        .send()
        .await
        .unwrap();
    (resp.status().as_u16(), resp.text().await.unwrap())
}

async fn agents_json(base: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{base}/api/v1/agents"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// The bug this task exists for: a FRESH seed has no `[runtime.dsh]`, and PATCH used to answer 404.
#[tokio::test]
async fn a_fresh_seed_edits_the_agent_card_and_says_so() {
    let b = boot("seed", SEED_SHAPE).await;
    let before = std::fs::read_to_string(&b.agents_toml).unwrap();

    let (status, body) = patch(
        &b.base,
        "dsh",
        serde_json::json!({ "command": "definitely-not-installed-xyz --acp" }),
    )
    .await;
    println!("READING t79 PATCH on a fresh seed: {status} {body}");
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        json["edited"], "agent",
        "it must say WHICH card it edited: {body}"
    );
    assert!(
        json["note"]
            .as_str()
            .unwrap_or_default()
            .contains("[agent.<name>]"),
        "and explain that this root defines runtimes in the agent layer: {body}"
    );

    let after = std::fs::read_to_string(&b.agents_toml).unwrap();
    assert_ne!(after, before, "the file must have changed");
    let dsh = after.split("[agent.mock]").next().unwrap_or_default();
    assert!(
        dsh.contains("command = \"definitely-not-installed-xyz --acp\""),
        "[agent.dsh].command is the new value: {after}"
    );
    assert!(
        !after.contains("[runtime."),
        "and no [runtime.*] layer was invented: {after}"
    );

    // the READ path must reflect the write, with the same cheap resolution reading
    let agents = agents_json(&b.base).await;
    let row = agents["agents"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["name"] == "dsh"))
        .cloned()
        .unwrap_or_else(|| panic!("no dsh card: {agents}"));
    assert_eq!(
        row["command"], "definitely-not-installed-xyz --acp",
        "GET /api/v1/agents shows the new command: {row}"
    );
    assert_eq!(
        row["command_resolved"], false,
        "and it does not resolve: {row}"
    );
}

/// The LEGACY shape must behave exactly as before: `[runtime.*]` wins, and the reply says so.
#[tokio::test]
async fn a_runtime_layer_root_is_unchanged() {
    let b = boot("legacy", RUNTIME_SHAPE).await;
    let before = std::fs::read_to_string(&b.agents_toml).unwrap();

    let (status, body) = patch(
        &b.base,
        "legacy",
        serde_json::json!({ "description": "edited through the runtime layer" }),
    )
    .await;
    println!("READING t79 PATCH on a [runtime.*] root: {status} {body}");
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["edited"], "runtime", "{body}");
    assert!(json.get("note").is_none(), "no fallback note: {body}");

    let after = std::fs::read_to_string(&b.agents_toml).unwrap();
    let legacy = after.split("[agent.alpha]").next().unwrap_or_default();
    assert!(legacy.contains("[runtime.legacy]"), "{after}");
    assert!(
        legacy.contains("edited through the runtime layer"),
        "the runtime layer got the field: {after}"
    );
    assert_eq!(
        after.split("[agent.alpha]").nth(1),
        before.split("[agent.alpha]").nth(1),
        "the agent table is untouched: {after}"
    );
}

/// An unknown name is still 404 -- and now says which runtimes this machine actually has.
#[tokio::test]
async fn an_unknown_name_lists_the_runtimes_that_exist() {
    let b = boot("unknown", SEED_SHAPE).await;
    let (status, body) = patch(&b.base, "nope", serde_json::json!({ "description": "x" })).await;
    println!("READING t79 PATCH unknown: {status} {body}");
    assert_eq!(status, 404, "{body}");
    assert!(
        body.contains("dsh"),
        "the message lists a runtime that IS here: {body}"
    );
    assert!(body.contains("mock"), "{body}");
    assert!(body.contains("not found"), "{body}");
}

/// DELETE follows the same fallback, keeps the "used by a role" conflict, and never answers a silent
/// 204 about an agent card.
#[tokio::test]
async fn delete_falls_back_to_the_agent_card_and_keeps_the_conflict_invariant() {
    let seed_with_role = format!(
        // a single-layer ("[agent.*]" only) file needs the runtime fields on EVERY entry, roles included
        "{SEED_SHAPE}[agent.some-role]\nharness = \"mock\"\ncommand = \"cargo\"\nprompt = \"do things\"\nruntimes = [\"dsh\"]\ndescription = \"role\"\n"
    );
    let b = boot("delete", &seed_with_role).await;

    // 1) a role references `dsh`: deleting it is a CONFLICT, not a success
    let (status, body) = delete(&b.base, "dsh").await;
    println!("READING t79 DELETE referenced: {status} {body}");
    assert_eq!(status, 409, "a referenced runtime is a conflict: {body}");
    assert!(
        body.contains("some-role"),
        "the message names the user: {body}"
    );
    assert!(
        std::fs::read_to_string(&b.agents_toml)
            .unwrap()
            .contains("[agent.dsh]"),
        "and nothing was removed"
    );

    // 2) an unreferenced card deletes, and SAYS it deleted the agent card
    let (status, body) = delete(&b.base, "mock").await;
    println!("READING t79 DELETE agent card: {status} {body}");
    assert_eq!(status, 200, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["edited"], "agent", "{body}");
    assert_eq!(json["deleted"], "mock", "{body}");
    assert!(
        !std::fs::read_to_string(&b.agents_toml)
            .unwrap()
            .contains("[agent.mock]"),
        "the card is gone"
    );
}

/// The documented `[runtime.*]` delete keeps its 204 (no body), so existing clients are unaffected.
#[tokio::test]
async fn deleting_a_runtime_layer_card_is_still_204() {
    let b = boot("delete-legacy", RUNTIME_SHAPE).await;
    let (status, body) = delete(&b.base, "legacy").await;
    println!("READING t79 DELETE runtime card: {status} {body:?}");
    assert_eq!(status, 204, "{body}");
    assert!(body.is_empty(), "204 carries no body: {body}");
    assert!(
        !std::fs::read_to_string(&b.agents_toml)
            .unwrap()
            .contains("[runtime.legacy]"),
        "the runtime layer entry is gone"
    );
}

/// The PRE-CHANGE path, still present: this is the function the endpoint called before t79, and on an
/// `[agent.*]`-only file it answers exactly what the captain measured through HTTP (`404 runtime `dsh`
/// not found`). Kept as a function and asserted here, so the "before" reading is reproducible without
/// reverting the tree.
#[tokio::test]
async fn the_legacy_write_path_still_refuses_an_agent_layer_only_file() {
    let root = std::env::temp_dir().join(format!("ruagent-t79-legacy-path-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("agents.toml");
    std::fs::write(&path, SEED_SHAPE).unwrap();

    let editor = ruagent_daemon::registry::Editor::new(&path);
    let err = editor
        .update_runtime(
            "dsh",
            &ruagent_daemon::registry::RuntimePatch {
                command: Some("definitely-not-installed-xyz --acp".to_string()),
                ..Default::default()
            },
        )
        .expect_err("the pre-t79 path only knew [runtime.*]");
    println!("READING t79 legacy path: {err}");
    assert!(
        format!("{err}").contains("runtime `dsh` not found"),
        "{err}"
    );

    // and it did NOT touch the file
    assert_eq!(std::fs::read_to_string(&path).unwrap(), SEED_SHAPE);
}
