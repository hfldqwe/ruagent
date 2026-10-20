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

// ---------------------------------------------------------------------------
// Temp-root cleanup (t37, the shape fixed by t27/t30/t31 -- issue #69)
//
// THIS SUITE IS THE ONE NOBODY COUNTED: %TEMP% held ~110 ruagent-mcphealth-* directories,
// one per start_daemon() call whose root was never reclaimed, because the only cleanup was
// a `let _ = remove_dir_all` at creation and nothing at all at the end. The in-process
// daemon holds the root's database for the whole round, so the root of the CURRENT run
// cannot be removed from the test that owns it -- what must not happen is that NOTHING ever
// removes it. Hence the same four parts the other suites use:
//
//   1. a bounded retry that RELEASES the handles it can and RETURNS the reason;
//   2. a REPORTING entry point for a root this process is holding (visible, never swallowed);
//   3. an ASSERTING entry point for a root the caller owns (a cleanup that cannot happen is
//      a red test, not a directory nobody counts);
//   4. a once-per-process pruner for roots whose pid is DEAD, so the pile is bounded by the
//      next run instead of growing forever.
// ---------------------------------------------------------------------------

/// Every temp-root prefix THIS suite creates, in one place, so the pruner cannot be given a
/// list that has drifted from the roots the suite actually writes.
const MCP_HEALTH_ROOT_PREFIXES: &[&str] = &["ruagent-mcphealth-"];

/// How long a removal waits for another task to drop its handle. The in-process daemon is
/// what holds the root, so this is a bounded poll rather than a hope.
const CLEANUP_BUDGET: std::time::Duration = std::time::Duration::from_millis(500);

/// Remove a root with a bounded retry, RETURNING the reason (t31's shape).
///
/// It returns instead of printing so a caller can assert on it, and it polls instead of
/// sleeping once because what it waits for is a handle being released.
fn try_remove_test_root(root: &std::path::Path, budget: std::time::Duration) -> Result<(), String> {
    let started = std::time::Instant::now();
    loop {
        match std::fs::remove_dir_all(root) {
            Ok(()) => {
                if started.elapsed() > std::time::Duration::from_millis(250) {
                    println!(
                        "READING t37 cleanup of {} waited {:.1} s for the handles to be released",
                        root.display(),
                        started.elapsed().as_secs_f64()
                    );
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                if started.elapsed() >= budget {
                    return Err(format!(
                        "{} could NOT be removed within {:.2} s: {e}",
                        root.display(),
                        budget.as_secs_f64()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }
}

/// Remove a root test code OWNS, or PANIC naming it (t31): a cleanup that cannot happen is a
/// red test, not a directory nobody counts.
fn remove_test_root(root: &std::path::Path, what: &str, budget: std::time::Duration) {
    if let Err(why) = try_remove_test_root(root, budget) {
        panic!("{what}: {why}");
    }
}

/// Try once, then REPORT -- never silently drop -- a root this process is still holding (t31).
///
/// WHY NOT AN ASSERTION HERE: the holder is THIS test process (the in-process daemon keeps a
/// handle inside the root for the whole round), so the removal cannot succeed however long it
/// waits, and a panic would red a test for a fact that is not its own. The failure is made
/// VISIBLE instead, and the pile is bounded by the pruner below -- which this call also
/// guarantees has run, so a suite that only ever cleans up still prunes.
fn report_test_root_left_behind(root: &std::path::Path) {
    prune_stale_mcp_health_roots();
    if let Err(why) = try_remove_test_root(root, CLEANUP_BUDGET) {
        eprintln!(
            "READING t37 cleanup: {why} -- the root is held by THIS test process, so the              failure is REPORTED rather than swallowed; the next run's pruner removes it"
        );
    }
}

/// Is a process with this pid alive? The daemon's own primitive, START TIME included, so a
/// RECYCLED pid is not mistaken for the process that made the root.
fn pid_is_alive(pid: u32) -> bool {
    ruagent_daemon::orphans::process_started_at_ms(pid).is_some()
}

/// Remove the roots of DEAD test processes for this suite's prefixes, once per process.
///
/// SAFETY RULE, deliberately conservative: a root is pruned only when it carries at least one
/// number AND none of its numbers belongs to a live process. Root names put the pid in
/// different positions, so the question is asked of EVERY number in the name; a number that
/// happens to be a live process keeps the root, which costs disk and never a run.
fn prune_stale_mcp_health_roots() {
    static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if DONE.set(()).is_err() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let me = std::process::id();
    let (mut pruned, mut failed) = (0usize, 0usize);
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !MCP_HEALTH_ROOT_PREFIXES.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let path = e.path();
        if !path.is_dir() || !all_numeric_fields_are_dead(&name, me) {
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => pruned += 1,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                failed += 1;
                eprintln!(
                    "READING t37 stale mcp-health root {} could NOT be removed: {err}",
                    path.display()
                );
            }
        }
    }
    if pruned > 0 || failed > 0 {
        eprintln!(
            "READING t37 pruned {pruned} stale mcp_health root(s) of dead test processes;              {failed} could not be removed"
        );
    }
}

/// True when every number in a root name is safe to prune on: no live process owns it, and it
/// is not OUR pid. A name with no number at all is NOT prunable (nothing identifies its run).
fn all_numeric_fields_are_dead(name: &str, me: u32) -> bool {
    all_numeric_fields_are_dead_with(name, me, pid_is_alive)
}

/// The rule with the liveness question passed IN, so a test can answer it deterministically
/// instead of depending on which pids this machine happens to have live: Windows REUSES pids,
/// and a reused one made the dead-pid case vacuous-red.
fn all_numeric_fields_are_dead_with(name: &str, me: u32, alive: impl Fn(u32) -> bool) -> bool {
    let mut seen = false;
    for field in name.split(|c: char| !c.is_ascii_digit()) {
        if field.is_empty() {
            continue;
        }
        let Ok(n) = field.parse::<u32>() else {
            continue;
        };
        // A number outside this range is NOT evidence about a run: below 1000 it is almost
        // always part of a NAME, and above 100_000_000 a nanosecond stamp cannot be a pid.
        // A name with no pid in it is never pruned, however many of these numbers it carries.
        if !(1000..=100_000_000).contains(&n) {
            continue;
        }
        seen = true;
        if n == me || alive(n) {
            return false;
        }
    }
    seen
}

static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

async fn start_daemon() -> (String, std::path::PathBuf) {
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-mcphealth-{}-{seq}", std::process::id()));
    // REPORT (never swallow) a pre-clean that could not happen, and run the once-per-process
    // pruner while we are here -- the name carries OUR pid, so nothing here is this call's to
    // red (t37's shape; the previous `let _ = remove_dir_all` was the silent half of the pile).
    report_test_root_left_behind(&root);
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

    // The root of THIS run cannot be removed from here: the in-process daemon spawned above
    // still holds its database, so a panic would red the test for a fact that is not its own
    // (t37). Report it -- and bound the pile with the pruner -- so the next run reclaims it.
    report_test_root_left_behind(&root);
}

// ---------------------------------------------------------------------------
// The cleanup shape's OWN proof (t37): a cleanup that cannot happen must be visible.
// ---------------------------------------------------------------------------

/// The bounded retry RETURNS a reason that NAMES the path, and the asserting half PANICS on
/// it -- so a root nobody removed can never go quiet again.
///
/// HOW THE FAILURE IS MADE DETERMINISTIC: a root that holds an OPEN SQLite database cannot be
/// removed on Windows (this machine), which is exactly the state every live suite leaves
/// behind. On unix an open file does NOT block rmdir, so the same fact is expressed the only
/// other way available -- a read-only directory (reasoned there, measured here).
///
/// The root is released and removed at the end: a test written about the leftover pile must
/// not join it.
#[test]
fn a_failed_cleanup_is_reported_and_the_asserting_half_panics() {
    let root = std::env::temp_dir().join(format!(
        "ruagent-mcphealth-{}-t37proof-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    // TEMPORARY t37 NEGATIVE CONTROL (env-gated, so the tree is never red by default). With
    // the flag set the root is NOT held, the removal therefore SUCCEEDS, and this test MUST
    // fail -- which is what proves the assertions below are about the failure and not vacuous.
    let hold = std::env::var_os("RUAGENT_T37_CONTROL_NO_HOLD")
        .is_none()
        .then(|| ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&root).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&root, perms).unwrap();
    }

    // 1. The failure is RETURNED, and it NAMES the root it could not remove.
    let why = try_remove_test_root(&root, std::time::Duration::from_millis(100))
        .expect_err("a root holding an open database must NOT come back as removed");
    println!("READING t37 deliberate cleanup failure: {why}");
    assert!(
        why.contains(&root.display().to_string()),
        "the report must name the root it could not remove: {why}"
    );

    // 2. The call a test makes for a root it OWNS PANICS on it.
    let panicked = std::panic::catch_unwind(|| {
        remove_test_root(
            &root,
            "the t37 proof root",
            std::time::Duration::from_millis(100),
        )
    })
    .is_err();
    assert!(
        panicked,
        "remove_test_root must PANIC on a root it could not remove, or the leftover pile is          back in the dark"
    );

    // Release the handle (and the unix permission trick), then remove it for real.
    drop(hold);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&root).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&root, perms).unwrap();
    }
    remove_test_root(
        &root,
        "the t37 proof root, after releasing its handle",
        std::time::Duration::from_millis(500),
    );
    println!("READING t37 after releasing the handle: the proof root was removed");
}

/// The pruner's rule, unit-tested WITHOUT touching %TEMP% and without reading this machine's
/// pid table: the liveness question is passed in, so the reading is the same on every machine.
#[test]
fn only_names_whose_numbers_are_all_dead_are_prunable() {
    let me = std::process::id();
    let never = |_pid: u32| false;
    let pid_4242 = |pid: u32| pid == 4242;
    // NO pid in the name: nothing identifies its run, so it is never pruned -- even though it
    // carries digits (the 37 of t37).
    assert!(!all_numeric_fields_are_dead_with(
        "ruagent-mcphealth-mock-bins",
        me,
        never
    ));
    assert!(!all_numeric_fields_are_dead_with(
        "ruagent-mcphealth-t37-proof",
        me,
        never
    ));
    // A number too large to be a pid is not evidence either way.
    assert!(!all_numeric_fields_are_dead_with(
        "ruagent-mcphealth-4294967280",
        me,
        never
    ));
    // A plausible pid that is GONE: prunable. This is the reading the pruner acts on.
    assert!(all_numeric_fields_are_dead_with(
        "ruagent-mcphealth-4242-3",
        me,
        never
    ));
    // ... and the same name with that pid ALIVE is kept, in whatever position the pid sits,
    // which is what makes the rule safe.
    assert!(!all_numeric_fields_are_dead_with(
        "ruagent-mcphealth-4242-3",
        me,
        pid_4242
    ));
    assert!(!all_numeric_fields_are_dead_with(
        "ruagent-mcphealth-3-4242",
        me,
        pid_4242
    ));
    // Our OWN pid is live by definition, so a root of this run is never pruned.
    assert!(!all_numeric_fields_are_dead_with(
        &format!("ruagent-mcphealth-{me}-0"),
        me,
        never
    ));
}
