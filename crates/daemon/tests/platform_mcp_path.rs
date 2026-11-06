//! t69: THE SHIPPED INSTALL RUNS FROM AN UNZIPPED FOLDER, WHERE `ruagent` IS NOT ON PATH.
//!
//! WHAT THE USER SAW: on a machine where the daemon was never installed (the zip was unpacked into a
//! folder and run from there), every runtime answered 400 on session options with
//! "MCP 服务器 [ruagent] 无法启动：PATH 上找不到 [ruagent]（配置的命令 = ruagent mcp-serve）" -- because
//! the shipped `mcp.toml` seeds the platform's own MCP server as the bare name ruagent and the
//! pre-flight refuses the whole profile when that name does not resolve. On a developer machine with
//! ruagent on PATH the same build is green, which is why nothing caught it.
//!
//! WHAT THIS FILE PROVES: with a PATH that does NOT contain ruagent, and with an OLD root whose
//! mcp.toml was written by the previous seed (the shape that shipped), the daemon (a) resolves the
//! platform's own server to its OWN executable, and (b) answers
//! `GET /api/v1/agents/{name}/options` with 200 instead of 400.
//!
//! WHY THIS TEST EDITS PATH, AND WHY THAT IS NOT A RACE: std::env::set_var is process-wide and cargo
//! runs one target's tests as THREADS in one process, so a test that edits PATH races every other
//! test in the same binary. THIS FILE HOLDS EXACTLY ONE TEST for that reason, and it restores PATH
//! before it asserts. The alternative -- an injected resolver -- is what the crate-internal tests in
//! crates/daemon/src/config.rs already do, so doing it again here would prove nothing about the real
//! code path; a third option (spawn the daemon as a child with a scrubbed PATH) needs a built daemon
//! binary and a port, i.e. a different suite.
//!
//! PRECONDITION: the mock agent binary. `cargo test --workspace` builds it; a bare
//! `cargo test -p ruagent-daemon` may not, so this test is #[ignore]d with the reason and PANICS
//! (never silently passes) when the binary is missing and the test is run anyway.

use std::path::PathBuf;
use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

/// THE OLD SEED, VERBATIM (the shape that shipped and that users already have on disk). This is the
/// "existing root" half of the task: the fix must work for a file the CURRENT seed writer never
/// touched, not only for a fresh install.
const OLD_SEEDED_MCP_TOML: &str = r#"# ruagent MCP registry. See design §7.1.
# The platform's own MCP server: memory, knowledge and task tools for
# every agent (design SS6.5). Requires the ruagent binary reachable as
# ruagent on PATH and a running daemon (ruagent serve).
[mcp.ruagent]
command = "ruagent"
args = ["mcp-serve"]

[profile.default]
servers = ["ruagent"]
"#;

const NEEDS_MOCK: &str = "needs ruagent-mock-agent next to this test binary: build it with `cargo build -p ruagent-mock-agent --bins`, then run this test with `-- --ignored`";

/// `target/<profile>/deps/<this test>` -> `target/<profile>/ruagent-mock-agent[.exe]`. Copied (not
/// re-derived) from crates/daemon/tests/injection_e2e.rs, which carries the reasoning for why a
/// cross-package suite has to do this arithmetic itself; the copies stay honest because the failure
/// message names every candidate.
fn mock_bin_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(profile_dir) = exe.parent().and_then(|d| d.parent())
    {
        for name in ["ruagent-mock-agent.exe", "ruagent-mock-agent"] {
            out.push(profile_dir.join(name));
        }
    }
    out
}

fn mock_bin() -> Option<String> {
    let candidates = mock_bin_candidates();
    if let Some(path) = candidates.iter().find(|p| p.is_file()) {
        return Some(path.display().to_string().replace(char::from(92), "/"));
    }
    eprintln!("mock binary MISSING -- {NEEDS_MOCK}");
    for path in &candidates {
        eprintln!("  looked for: {}", path.display());
    }
    None
}

/// Remove a root with a bounded retry, REPORTING a failure instead of swallowing it (the t31 shape;
/// the canonical copy with the asserting half is crates/daemon/src/lib.rs::test_cleanup, which an
/// integration test cannot reach).
fn remove_test_root(root: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match std::fs::remove_dir_all(root) {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                if std::time::Instant::now() >= deadline {
                    eprintln!(
                        "t69: the private root {} could NOT be removed within 5 s: {e}",
                        root.display()
                    );
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }
}

#[tokio::test]
#[ignore = "needs ruagent-mock-agent next to this test binary: build it with `cargo build -p ruagent-mock-agent --bins`, then run with `-- --ignored`"]
async fn options_are_not_400_when_ruagent_is_not_on_path() {
    use agent_client_protocol::schema::v1::McpServer;
    use tower::ServiceExt as _;

    let mock = mock_bin().expect(NEEDS_MOCK);
    // THE PREFIX IS PART OF THE CLEANUP CONTRACT: "ruagent-cfg-" is already in the daemon's
    // TEST_ROOT_PREFIXES (crates/daemon/src/lib.rs), so a handle that outlives this test -- and one
    // does: an in-process daemon keeps a handle inside its root for the whole round, which is why
    // the removal below is REPORTED rather than asserted -- is swept by the next run's pruner.
    // Inventing a new prefix would have meant editing lib.rs, which this task does not own.
    let root = std::env::temp_dir().join(format!("ruagent-cfg-pathless-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a private root");
    // The two files the load must NOT reseed are written FIRST: mcp.toml in the OLD shipped shape,
    // agents.toml naming the mock binary by ABSOLUTE path (a bare name would need the PATH this test
    // is about to scrub).
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(config_dir.join("mcp.toml"), OLD_SEEDED_MCP_TOML).expect("mcp.toml");
    std::fs::write(
        config_dir.join("agents.toml"),
        format!(
            "# t69 fixture: one enabled mock runtime, spawned by absolute path.\n\
             [agent.mock]\n\
             harness = \"mock\"\n\
             command = '{mock} --behavior echo'\n\
             description = \"t69 pathless-PATH fixture\"\n\
             enabled = true\n"
        ),
    )
    .expect("agents.toml");

    let cfg = DaemonConfig::load(&root).expect("the config loads (and seeds what is missing)");
    assert!(
        cfg.mcp.servers.contains_key("ruagent"),
        "the fixture must carry the platform's own server: {:?}",
        cfg.mcp.servers.keys().collect::<Vec<_>>()
    );

    // A directory that exists but holds no ruagent binary: the shipped-install shape.
    let pathless = root.join("path-without-ruagent");
    std::fs::create_dir_all(&pathless).expect("empty PATH dir");
    let saved_path = std::env::var_os("PATH");
    // SAFETY: this file holds exactly ONE test, so no other test in this process reads PATH
    // concurrently (the file's doc comment carries the whole argument).
    unsafe { std::env::set_var("PATH", &pathless) };
    let ruagent_still_findable = ruagent_mcp::health::resolve_command("ruagent").is_ok();

    // (a) the config-level answer, with the REAL resolver and the scrubbed PATH.
    let injected = cfg
        .mcp
        .expand_profile_preflighted(Some("default"), "mock")
        .unwrap_or_else(|e| panic!("the platform's own server must not be refused here: {e}"));
    let injected_command = match injected.as_slice() {
        [McpServer::Stdio(stdio)] => stdio.command.display().to_string(),
        other => panic!("expected exactly one stdio server, got {other:?}"),
    };
    let own_exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    // (b) the route the user actually hits. IN A BLOCK, so every handle (the SQLite connection
    // the knowledge base and the session indexer share) is DROPPED before the root is removed --
    // measured: removing first left "另一个程序正在使用此文件" and a littered temp root on every run.
    let (status, text) = {
        let db = Db::open(root.join("data").join("ruagent.db")).expect("db");
        let knowledge = Arc::new(
            ruagent_knowledge::Knowledge::open(&root, db.clone())
                .await
                .expect("knowledge"),
        );
        let chats = ruagent_daemon::chat::ChatManager::new(
            db.clone(),
            root.clone(),
            Arc::new(|_, _| {}),
            cfg.mcp.clone(),
            ruagent_daemon::distill::AutoDistill::default(),
            Some(knowledge.embedder()),
            ruagent_daemon::distill::AgentRegistry::default(),
        );
        let mgr = Arc::new(RunManager::new(
            db.clone(),
            root.clone(),
            cfg.agents.clone(),
            cfg.policy.to_policy(),
            cfg.mcp.clone(),
        ));
        chats.set_knowledge(Arc::clone(&knowledge));
        mgr.set_knowledge(Arc::clone(&knowledge));
        let app = ruagent_daemon::api::router(AppState {
            mgr,
            config: Arc::new(cfg),
            knowledge: Arc::clone(&knowledge),
            chats,
            sessions: Arc::new(ruagent_daemon::sessions::SessionIndexer::new(
                db,
                std::env::temp_dir(),
            )),
        });
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/v1/agents/mock/options")
                    .body(axum::body::Body::empty())
                    .expect("request"),
            )
            .await
            .expect("router");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8_lossy(&bytes).to_string();
        (status, text)
    };

    // Restore PATH BEFORE asserting: a panic must not leave the process with a scrubbed PATH.
    // SAFETY: same single-test argument as above.
    unsafe {
        match saved_path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }

    println!(
        "READING t69 PATH={} (ruagent findable = {ruagent_still_findable}), old-seed mcp.toml: injected command = {injected_command} (this test binary = {own_exe}); GET /api/v1/agents/mock/options -> {status}, body = {}",
        pathless.display(),
        &text[..text.len().min(300)]
    );

    assert!(
        !ruagent_still_findable,
        "the precondition of this test: ruagent must NOT resolve from {}",
        pathless.display()
    );
    assert_eq!(
        injected_command, own_exe,
        "the platform's own MCP server is started from THIS executable when the bare name does not resolve"
    );
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "an unzipped install must not have every runtime refused at the MCP pre-flight: {text}"
    );
    assert!(
        !text.contains("PATH 上找不到") && !text.contains("MCP 服务器"),
        "the MCP pre-flight refusal must not appear at all: {text}"
    );

    remove_test_root(&root);
}
