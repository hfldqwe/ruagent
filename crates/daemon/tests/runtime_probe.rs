//! t68: the runtime PROBE, as an integration target so CI can actually run it.
//!
//! These two tests started life as unit tests in `api.rs` behind `#[ignore]` because they need the
//! mock ACP agent binary; CI never runs daemon-lib ignored tests, so they produced no reading and
//! made the ignored-instrument manifest red. `.github/workflows/ci.yml` now runs THIS target with
//! `--include-ignored`, which is the same shape `injection_e2e` already uses.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

/// What to do when the mock binary is missing (the shape injection_e2e uses: with `--include-ignored`
/// a missing binary is a FAILURE, not a skip -- that is the whole point of moving this test here).
const NEEDS_MOCK: &str =
    "the mock ACP agent is missing: cargo build -p ruagent-mock-agent --bins (CI builds it)";

/// One test mutates PATH (the `.cmd` shim case), so the three take turns. An ASYNC mutex, because
/// these are async tests: holding a std guard across an await is what clippy (rightly) refuses.
static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn turn() -> tokio::sync::MutexGuard<'static, ()> {
    TURN.lock().await
}

/// The mock agent beside this test binary: tests live in `<target>/<profile>/deps/`.
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

const AGENTS: &str = "[agent.alpha]\nharness = \"mock\"\ncommand = \"cargo\"\ndescription = \"alpha\"\nprompt = \"alpha\"\n";

/// A daemon on an ephemeral port over a PRIVATE root: the shape the other in-test daemons use.
async fn boot(tag: &str) -> (String, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("ruagent-probe-{tag}-{}", std::process::id()));
    // A stale same-named root from an earlier run: this test owns the name (process id + tag), and a
    // failure to remove it is not this test's business -- the fresh `create_dir_all` below is.
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(root.join("data")).unwrap();
    std::fs::write(config_dir.join("agents.toml"), AGENTS).unwrap();

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
    (format!("http://{addr}"), root)
}

async fn make_runtime(base: &str, name: &str, command: &str) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/v1/runtimes"))
        .json(&serde_json::json!({ "name": name, "harness": "mock", "command": command }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "creating runtime `{name}`");
}

async fn probe(base: &str, name: &str) -> (u16, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/v1/runtimes/{name}/probe"))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let body = resp.json::<serde_json::Value>().await.unwrap();
    (status, body)
}

/// The mock agent handshakes, so a probe of it succeeds and names the binary it found.
#[tokio::test]
#[ignore = "needs ruagent-mock-agent next to this test binary: cargo build -p ruagent-mock-agent --bins"]
async fn probing_the_mock_agent_reports_ok_and_the_command_path() {
    let _turn = turn().await;
    let mock = mock_bin().expect(NEEDS_MOCK);
    let (base, _root) = boot("ok").await;
    make_runtime(&base, "mock-ok", &format!("{mock} --behavior echo")).await;

    let (status, body) = probe(&base, "mock-ok").await;
    // a COMPLETED probe is 200 even when it fails; this one succeeds
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], true, "{body}");
    assert_eq!(body["stage"], "ok", "{body}");
    assert!(
        body["command_path"].as_str().is_some_and(|p| !p.is_empty()),
        "the probe names the binary it resolved: {body}"
    );
}

/// A program that starts and dies at once is reported WITH its exit code and the tail of its stderr.
#[tokio::test]
#[ignore = "needs ruagent-mock-agent next to this test binary: cargo build -p ruagent-mock-agent --bins"]
async fn a_program_that_exits_immediately_is_reported_with_its_code_and_stderr() {
    let _turn = turn().await;
    let mock = mock_bin().expect(NEEDS_MOCK);
    let (base, _root) = boot("dies").await;
    make_runtime(
        &base,
        "mock-dies",
        &format!("{mock} --definitely-not-a-behaviour"),
    )
    .await;

    let (status, body) = probe(&base, "mock-dies").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], false, "{body}");
    assert_eq!(body["stage"], "spawn-failed", "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("exited immediately"), "{detail}");
    assert!(
        detail.contains("code:") || detail.contains("exit status"),
        "the detail carries the exit code/status: {detail}"
    );
}

/// t68: on Windows a `.cmd` shim is launched through `cmd /c <shim>`, and the probe used to report
/// that SHELL as `command_path` -- a path no user can find or search for (`"cmd"`). It must report
/// the shim itself, and say in the detail that the launch goes through a shell.
#[tokio::test]
async fn a_cmd_shim_is_reported_by_its_own_path_not_as_the_shell() {
    let _turn = turn().await;
    if !cfg!(windows) {
        return; // a `.cmd` shim only exists on Windows; `resolve_program` never wraps elsewhere
    }
    let (base, root) = boot("shim").await;
    let shim_dir = root.join("shims");
    std::fs::create_dir_all(&shim_dir).unwrap();
    let shim = shim_dir.join("fake-agent.cmd");
    std::fs::write(&shim, "@echo off\r\nexit /b 0\r\n").unwrap();

    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![shim_dir.clone()];
    dirs.extend(std::env::split_paths(&old_path));
    let joined = std::env::join_paths(dirs).unwrap();
    // SAFETY: PATH is mutated only while this test holds `TURN`, and restored before it returns.
    unsafe { std::env::set_var("PATH", &joined) };

    make_runtime(&base, "shimmy", "fake-agent").await;
    let (status, body) = probe(&base, "shimmy").await;
    unsafe { std::env::set_var("PATH", &old_path) };

    println!("READING t68 cmd-shim probe: {body}");
    assert_eq!(status, 200, "{body}");
    let path = body["command_path"].as_str().unwrap_or_default();
    assert_ne!(path, "cmd", "the shell is not the command: {body}");
    assert!(
        path.ends_with("fake-agent.cmd"),
        "command_path must be the shim itself: {body}"
    );
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("launched through")
            || body["stage"] == "spawn-failed",
        "a shell launch must be stated, not hidden: {body}"
    );
}
