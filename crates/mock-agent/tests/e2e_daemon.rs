//! Full-stack daemon e2e: axum API + RunManager + store + ACP client
//! driving the real mock-agent binary. This is M1's integration proof —
//! no real harnesses, no API keys (design §12).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

struct TestDaemon {
    url: String,
    _root: PathBuf,
}

async fn start_daemon(agents_toml: &str, policy_toml: &str) -> TestDaemon {
    start_daemon_with_routing(agents_toml, policy_toml, "").await
}

async fn start_daemon_with_routing(
    agents_toml: &str,
    policy_toml: &str,
    routing_toml: &str,
) -> TestDaemon {
    let root = std::env::temp_dir().join(format!(
        "ruagent-e2e-{}-{}",
        std::process::id(),
        chrono_tag()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("agents.toml"), agents_toml).unwrap();
    std::fs::write(
        config_dir.join("mcp.toml"),
        "[profile.default]\nservers = []\n",
    )
    .unwrap();
    std::fs::write(config_dir.join("policy.toml"), policy_toml).unwrap();
    if !routing_toml.is_empty() {
        std::fs::write(config_dir.join("routing.toml"), routing_toml).unwrap();
    }

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
        ruagent_daemon::distill::AutoDistill::default(),
        None,
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    let mgr = Arc::new(RunManager::new(
        db.clone(),
        root.clone(),
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));
    mgr.start_approver_loop();

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
    TestDaemon {
        url: format!("http://{addr}"),
        _root: root,
    }
}

/// Unique dir component. Windows SystemTime granularity (~0.5-15ms) makes
/// nanosecond stamps collide between parallel tests — hence a counter.
static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn chrono_tag() -> u64 {
    DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

fn mock_agent_toml(behavior: &str) -> String {
    // Windows paths must use forward slashes here: TOML basic strings
    // treat backslashes as escapes (a lesson for users too).
    let bin = mock_bin();
    format!(
        "[agent.mock]
harness = \"mock\"
command = \"{bin} --behavior {behavior}\"
description = \"mock\"
"
    )
}

/// Forward-slash mock binary path, TOML-safe on Windows.
fn mock_bin() -> String {
    env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/")
}

/// Run a prompt on the mock agent and return (final text, sse dump).
async fn drive_run(http: &reqwest::Client, url: &str, prompt: &str) -> (String, String) {
    let task: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks"))
        .json(&serde_json::json!({ "title": "e2e", "intent": prompt }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    let run: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks/{task_id}/runs"))
        .json(&serde_json::json!({ "agent": "mock", "prompt": prompt }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(run["status"], "spawning", "run should start");
    let run_id = run["id"].as_str().unwrap().to_string();

    // Stream SSE to the end marker (server closes the stream there).
    let sse = http
        .get(format!("{url}/api/v1/runs/{run_id}/events"))
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .text()
        .await
        .unwrap();

    // Extract agent text from the transcript lines.
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
    (text, sse)
}

#[tokio::test]
async fn echo_run_end_to_end() {
    let d = start_daemon(&mock_agent_toml("echo"), "default = \"ask\"\n").await;
    let http = reqwest::Client::new();

    // health
    let health: serde_json::Value = http
        .get(format!("{}/api/v1/health", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["status"], "ok");

    // agents
    let agents: serde_json::Value = http
        .get(format!("{}/api/v1/agents", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(agents["agents"][0]["name"], "mock");

    let (text, sse) = drive_run(&http, &d.url, "hello daemon").await;
    assert_eq!(text, "echo: hello daemon");
    assert!(
        sse.contains("\"type\":\"usage_update\""),
        "usage in transcript"
    );
    assert!(sse.contains("event: end"), "end marker present");

    // task is done
    let tasks: serde_json::Value = http
        .get(format!("{}/api/v1/tasks", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tasks["tasks"][0]["status"], "done");
}

#[tokio::test]
async fn permission_parks_for_human_then_resolves() {
    let d = start_daemon(&mock_agent_toml("permission"), "default = \"ask\"\n").await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "perm", "intent": "write something" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    let run: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/runs", d.url))
        .json(&serde_json::json!({ "agent": "mock", "prompt": "write something" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();

    // The ask parks in the inbox (policy default = ask).
    let pending = poll_until(&http, &format!("{}/api/v1/permissions", d.url), |v| {
        !v["pending"].as_array().unwrap().is_empty()
    })
    .await;
    let p = &pending["pending"][0];
    assert_eq!(p["title"], "Write file");
    let key = format!(
        "{}:{}",
        p["run_id"].as_str().unwrap(),
        p["tool_call_id"].as_str().unwrap()
    );

    // While parked: the task view says so (issue #34) and the run is
    // `running` (the session went live), not stuck in `spawning`.
    let detail = poll_until(&http, &format!("{}/api/v1/tasks/{task_id}", d.url), |v| {
        v["runs"]
            .as_array()
            .is_some_and(|rs| rs.iter().any(|r| r["waiting_permission"].is_object()))
    })
    .await;
    let parked = detail["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["waiting_permission"].is_object())
        .unwrap();
    assert_eq!(parked["status"], "running");
    assert_eq!(parked["waiting_permission"]["title"], "Write file");

    // An unknown option_id is rejected without consuming the ask.
    let bad = http
        .post(format!("{}/api/v1/permissions/{key}", d.url))
        .json(&serde_json::json!({ "option_id": "allow-everything-forever" }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);

    // Answer with the agent's exact option id (issue #35) — the only
    // path that can express allow-always style options.
    let status = http
        .post(format!("{}/api/v1/permissions/{key}", d.url))
        .json(&serde_json::json!({ "option_id": "allow-once" }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success());

    // Stream the (already running) run to its end and check the verdict.
    let sse = http
        .get(format!("{}/api/v1/runs/{run_id}/events", d.url))
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        sse.contains("permission allowed"),
        "verdict in stream:\n{sse}"
    );
    assert!(
        sse.contains("\"resolution\":{\"source\":\"human\"}"),
        "human attribution recorded:\n{sse}"
    );
}

#[tokio::test]
async fn permission_rule_auto_allows() {
    let d = start_daemon(
        &mock_agent_toml("permission"),
        r#"
[permissions]
default = "ask"

[[permissions.rules]]
title_contains = "write"
action = "allow"
"#,
    )
    .await;
    let http = reqwest::Client::new();

    let (text, sse) = drive_run(&http, &d.url, "write something").await;
    assert_eq!(text, "permission allowed");
    assert!(
        sse.contains("\"source\":\"rule\""),
        "rule attribution recorded"
    );

    // Nothing should be parked in the inbox afterwards.
    let pending: serde_json::Value = http
        .get(format!("{}/api/v1/permissions", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(pending["pending"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn crashed_agent_marks_run_failed() {
    let d = start_daemon(&mock_agent_toml("crash"), "default = \"ask\"\n").await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "crash", "intent": "go boom" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();
    let run: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/runs", d.url))
        .json(&serde_json::json!({ "agent": "mock", "prompt": "go boom" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();

    // Wait for the run to settle into failed.
    let final_run = poll_until(&http, &format!("{}/api/v1/runs/{run_id}", d.url), |v| {
        v["status"] == "failed"
    })
    .await;
    assert_eq!(final_run["status"], "failed");
    assert_eq!(final_run["stop_reason"], "error");
    // The SDK reports crashes as either "Incoming transport closed" or
    // "Process exited with exit code: 1" depending on which watcher fires
    // first — assert on semantics, not wording.
    assert!(!final_run["error"].as_str().unwrap_or("").is_empty());
}

/// Poll a JSON GET until `pred` holds (10s ceiling).
async fn poll_until(
    http: &reqwest::Client,
    url: &str,
    pred: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let v: serde_json::Value = http.get(url).send().await.unwrap().json().await.unwrap();
        if pred(&v) {
            return v;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {url}, last: {v}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ---------------------------------------------------------------------------
// M2 topology tests
// ---------------------------------------------------------------------------

fn two_mock_agents_toml() -> String {
    let bin = env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/");
    format!(
        "[agent.mocka]\nharness = \"mock\"\ncommand = \"{bin} --behavior echo\"\ndescription = \"echo mock\"\n\n[agent.mockb]\nharness = \"mock\"\ncommand = \"{bin} --behavior plan\"\ndescription = \"plan mock\"\n"
    )
}

async fn wait_task_done(http: &reqwest::Client, url: &str, task_id: &str) -> serde_json::Value {
    poll_until(http, &format!("{url}/api/v1/tasks/{task_id}"), |v| {
        v["task"]["status"] == "done"
    })
    .await
}

#[tokio::test]
async fn fanout_compares_and_selects() {
    let d = start_daemon(&two_mock_agents_toml(), "default = \"ask\"\n").await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "compare", "intent": "hello fanout" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap();

    let fan: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/fanout", d.url))
        .json(&serde_json::json!({ "agents": ["mocka", "mockb"] }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let runs = fan["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "two parallel runs");

    // Both complete with distinct results.
    let mut results = Vec::new();
    for r in runs {
        let rid = r["id"].as_str().unwrap();
        let final_run = poll_until(&http, &format!("{}/api/v1/runs/{rid}", d.url), |v| {
            v["status"] == "completed"
        })
        .await;
        results.push(final_run["result"].as_str().unwrap_or("").to_string());
    }
    assert!(results.iter().any(|r| r.contains("echo: hello fanout")));
    assert!(results.iter().any(|r| r.contains("planned work")));

    // Select the winner: the echo run.
    let detail = wait_task_done(&http, &d.url, task_id).await;
    let all_runs = detail["runs"].as_array().unwrap();
    let winner = all_runs
        .iter()
        .find(|r| r["result"].as_str().unwrap_or("").contains("echo:"))
        .unwrap();
    let winner_id = winner["id"].as_str().unwrap();
    let status = http
        .post(format!("{}/api/v1/runs/{winner_id}/select", d.url))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success());

    let detail: serde_json::Value = http
        .get(format!("{}/api/v1/tasks/{task_id}", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["selected_run_id"].as_str(), Some(winner_id));
}

#[tokio::test]
async fn pipeline_hands_off_upstream_result() {
    let bin = env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/");
    let agents = format!(
        "[agent.mock]\nharness = \"mock\"\ncommand = \"{bin} --behavior echo\"\ndescription = \"echo mock\"\n"
    );
    let d = start_daemon(&agents, "default = \"ask\"\n").await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "pipe", "intent": "say alpha" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap();

    let pipe: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/pipeline", d.url))
        .json(&serde_json::json!({
            "steps": [
                { "agent": "mock" },
                { "agent": "mock" }
            ]
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let sub_tasks = pipe["tasks"].as_array().unwrap();
    assert_eq!(sub_tasks.len(), 2, "one sub-task per step");

    // Both steps complete; the second one's prompt carried the first one's
    // result (the mock echoes its prompt, so "alpha" must appear again).
    let first = wait_task_done(&http, &d.url, sub_tasks[0].as_str().unwrap()).await;
    let second = wait_task_done(&http, &d.url, sub_tasks[1].as_str().unwrap()).await;

    let r1 = first["runs"][0]["result"].as_str().unwrap_or("");
    let r2 = second["runs"][0]["result"].as_str().unwrap_or("");
    assert!(r1.contains("echo: say alpha"), "step 1: {r1}");
    assert!(
        r2.contains("alpha") && r2.contains("Upstream result"),
        "step 2 must receive the handoff: {r2}"
    );
}

#[tokio::test]
async fn routing_rule_picks_agent_without_explicit_pin() {
    let bin = env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/");
    let agents = format!(
        "[agent.mocka]\nharness = \"mock\"\ncommand = \"{bin} --behavior echo\"\ndescription = \"a\"\n\n[agent.mockb]\nharness = \"mock\"\ncommand = \"{bin} --behavior plan\"\ndescription = \"b\"\n"
    );
    let routing = "[[routes]]\nproject = \"proj-x\"\nagent = \"mockb\"\n\ndefault = \"mocka\"\n";
    let d = start_daemon_with_routing(&agents, "default = \"ask\"\n", routing).await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(
            &serde_json::json!({ "title": "routed", "intent": "hello route", "project": "proj-x" }),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap();

    // No agent named: the rule must pick mockb (plan behavior).
    let run: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/runs", d.url))
        .json(&serde_json::json!({ "prompt": "hello route" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();
    let final_run = poll_until(&http, &format!("{}/api/v1/runs/{run_id}", d.url), |v| {
        v["status"] == "completed"
    })
    .await;
    let result = final_run["result"].as_str().unwrap_or("");
    assert!(
        result.contains("planned work"),
        "rule must route to mockb; run = {final_run}"
    );

    // And the Routed event with rule provenance is on the stream.
    let sse = http
        .get(format!("{}/api/v1/runs/{run_id}/events", d.url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        sse.contains("\"type\":\"routed\"") && sse.contains("rule"),
        "routed event with rule provenance expected"
    );
}

// ---------------------------------------------------------------------------
// M2 worktree isolation (design SS8.2)
// ---------------------------------------------------------------------------

/// A temp git repo with one commit on `main` — the isolation target for
/// worktree fan-outs, and the merge target when a winner lands.
fn temp_git_repo(tag: &str) -> PathBuf {
    // Windows SystemTime granularity makes timestamp tags collide
    // between parallel tests — hence a counter (see DIR_SEQ).
    static REPO_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = REPO_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let repo =
        std::env::temp_dir().join(format!("ruagent-repo-{tag}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("hello.txt"), "base").unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .args(args)
            .status()
            .unwrap()
            .success()
    };
    assert!(git(&["init", "-q", "-b", "main"]));
    assert!(git(&["add", "."]));
    assert!(git(&["commit", "-q", "-m", "base"]));
    repo
}

#[tokio::test]
async fn fanout_with_repo_gives_each_run_its_own_worktree() {
    // A real git repo to isolate in.
    let repo = temp_git_repo("wt");

    let d = start_daemon(
        &two_mock_agents_toml(),
        "default = \"ask\"
",
    )
    .await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "wt", "intent": "hello wt" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap();

    let fan: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/fanout", d.url))
        .json(&serde_json::json!({
            "agents": ["mocka", "mockb"],
            "repo": repo.to_string_lossy().replace('\\', "/")
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let runs = fan["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);

    let mut workspaces = Vec::new();
    for r in runs {
        let rid = r["id"].as_str().unwrap();
        let final_run = poll_until(&http, &format!("{}/api/v1/runs/{rid}", d.url), |v| {
            v["status"] == "completed"
        })
        .await;
        let ws = final_run["workspace"].as_str().unwrap().to_string();
        // The worktree contains the repo's file and is under worktrees/.
        assert!(ws.replace('\\', "/").contains("/worktrees/"), "ws = {ws}");
        assert!(
            std::path::Path::new(&ws).join("hello.txt").is_file(),
            "repo content in {ws}"
        );
        // And it is a real worktree on its own branch.
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&ws)
            .args(["branch", "--show-current"])
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert!(branch.starts_with("ruagent/run-"), "branch = {branch}");
        workspaces.push(ws);
    }
    assert_ne!(
        workspaces[0], workspaces[1],
        "runs must not share a worktree"
    );

    // Issue #43: deleting the task discards its outputs — the
    // worktrees and their branches go with the rows.
    let resp = http
        .delete(format!("{}/api/v1/tasks/{task_id}", d.url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::NO_CONTENT);
    for ws in &workspaces {
        assert!(!std::path::Path::new(ws).exists(), "worktree must go: {ws}");
    }
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["worktree", "list"])
        .output()
        .unwrap();
    let listed = String::from_utf8_lossy(&out.stdout);
    assert!(
        !listed.contains("run-"),
        "admin entries must be pruned: {listed}"
    );
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["branch", "--list", "ruagent/run-*"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "branches must be deleted"
    );

    let _ = std::fs::remove_dir_all(&repo);
}

// ---------------------------------------------------------------------------
// Fan-out winner landing (design §5.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn land_selected_worktree() {
    // The winner's work must reach the main repo, not die on a branch:
    // land commits the selected run's uncommitted work, merges its
    // branch, and sweeps the task's worktrees (design §5.2).
    let repo = temp_git_repo("land");
    // Two write mocks in the two-layer form: every run leaves out.txt
    // in its own worktree.
    let bin = mock_bin();
    let d = start_daemon(
        &format!(
            "[runtime.wr1]\nharness = \"mock\"\ncommand = \"{bin} --behavior write\"\n\n\
             [runtime.wr2]\nharness = \"mock\"\ncommand = \"{bin} --behavior write\"\n\n\
             [agent.writer1]\nruntimes = [\"wr1\"]\nruntime = \"wr1\"\n\n\
             [agent.writer2]\nruntimes = [\"wr2\"]\nruntime = \"wr2\"\n"
        ),
        "default = \"ask\"\n",
    )
    .await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "land me", "intent": "write the file" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    let fan: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/fanout", d.url))
        .json(&serde_json::json!({
            "agents": ["writer1", "writer2"],
            "repo": repo.to_string_lossy().replace('\\', "/")
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let runs = fan["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    let run1 = runs[0]["id"].as_str().unwrap().to_string();

    // Both runs complete, each with out.txt in its own worktree.
    for r in runs {
        let rid = r["id"].as_str().unwrap();
        let final_run = poll_until(&http, &format!("{}/api/v1/runs/{rid}", d.url), |v| {
            v["status"] == "completed"
        })
        .await;
        assert_eq!(final_run["result"], "wrote out.txt");
        let ws = final_run["workspace"].as_str().unwrap();
        assert!(
            std::path::Path::new(ws).join("out.txt").is_file(),
            "the mock wrote into its worktree: {ws}"
        );
    }

    // Human pick, then land.
    let status = http
        .post(format!("{}/api/v1/runs/{run1}/select", d.url))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success());
    let detail: serde_json::Value = http
        .get(format!("{}/api/v1/tasks/{task_id}", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        detail["landable"], true,
        "a selected winner with its worktree is landable: {detail}"
    );

    let resp = http
        .post(format!("{}/api/v1/tasks/{task_id}/land", d.url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let landed: serde_json::Value = resp.json().await.unwrap();
    assert!(
        !landed["landed"].as_str().unwrap_or("").is_empty(),
        "the land reply carries the new HEAD: {landed}"
    );

    // The main repo carries the winner's file, committed by the land.
    assert_eq!(
        std::fs::read_to_string(repo.join("out.txt")).unwrap(),
        "written by mock"
    );
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["log", "-1", "--format=%s"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("ruagent: land run {run1} (land me)"),
        "HEAD is the land commit"
    );

    // The task's worktrees and branches are swept...
    let worktrees = d._root.join("worktrees");
    let swept = std::fs::read_dir(&worktrees)
        .map(|mut it| it.next().is_none())
        .unwrap_or(true); // a removed dir is swept too
    assert!(
        swept,
        "the task's worktrees must be gone: {}",
        worktrees.display()
    );
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["branch", "--list", "ruagent/run-*"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "run branches must be deleted"
    );

    // ...so the task is no longer landable and a second land is a 400.
    let detail: serde_json::Value = http
        .get(format!("{}/api/v1/tasks/{task_id}", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["landable"], false);
    let resp = http
        .post(format!("{}/api/v1/tasks/{task_id}/land", d.url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);

    let _ = std::fs::remove_dir_all(&repo);
}

// ---------------------------------------------------------------------------
// M2 approver agent (design SS9.2, tier 2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn approver_agent_answers_ask_unattended() {
    let bin = env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/");
    // worker: permission behavior (asks before writing)
    // approver: approve behavior (replies exactly ALLOW)
    let agents = format!(
        "[agent.worker]\nharness = \"mock\"\ncommand = \"{bin} --behavior permission\"\ndescription = \"asks\"\n\n[agent.approver]\nharness = \"mock\"\ncommand = \"{bin} --behavior approve\"\ndescription = \"approves\"\n"
    );
    let policy = "[permissions]\ndefault = \"ask\"\n\n[permissions.approver]\nagent = \"approver\"\nhigh_risk = [\"delete\"]\n";
    let d = start_daemon(&agents, policy).await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "unattended", "intent": "write something" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap();

    let run: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/runs", d.url))
        .json(&serde_json::json!({ "agent": "worker", "prompt": "write something" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();

    // The run completes WITHOUT any human action: the approver agent
    // answered the parked ask.
    let sse = http
        .get(format!("{}/api/v1/runs/{run_id}/events", d.url))
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        sse.contains("permission allowed"),
        "worker proceeds after approval"
    );
    assert!(
        sse.contains("\"source\":\"approver_agent\""),
        "approver attribution recorded"
    );

    // The inbox is empty afterwards (the approver consumed the ask).
    let pending: serde_json::Value = http
        .get(format!("{}/api/v1/permissions", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(pending["pending"].as_array().unwrap().is_empty());

    // And the approver's decision run exists as a real, traced task.
    let tasks: serde_json::Value = http
        .get(format!("{}/api/v1/tasks", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let approve_task = tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"].as_str().unwrap_or("").starts_with("approve:"))
        .expect("approver run recorded as a task");
    assert_eq!(approve_task["status"], "done");
}

// ---------------------------------------------------------------------------
// M3: memory injection into run prompts (design SS6.4 push path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memories_are_injected_into_run_prompts() {
    let d = start_daemon(&mock_agent_toml("echo"), "default = \"ask\"\n").await;
    let http = reqwest::Client::new();

    // Write a user observation through the API (the MCP tool's backend).
    let w: serde_json::Value = http
        .post(format!("{}/api/v1/memory/write", d.url))
        .json(&serde_json::json!({
            "store": "observation",
            "namespace": "user",
            "content": "the user prefers concise answers"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(w["outcome"].as_str().unwrap().contains("Inserted"), "{w}");

    // Search finds it.
    let hits: serde_json::Value = http
        .get(format!("{}/api/v1/memory/search?q=concise", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hits["hits"].as_array().unwrap().len(), 1);

    // The echo mock echoes its FULL prompt — so the injected memory block
    // must appear in the run's result.
    let (text, sse) = drive_run(&http, &d.url, "hello injection").await;
    assert!(text.contains("hello injection"), "{text}");
    assert!(
        text.contains("the user prefers concise answers"),
        "injected memory visible in the agent's prompt echo: {text}"
    );
    assert!(
        sse.contains("\"type\":\"context_injected\""),
        "ContextInjected event on the stream"
    );
    assert!(sse.contains("<relevant_memories>"), "tagged block rendered");
}

// ---------------------------------------------------------------------------
// M4: run cancellation (#25)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancel_parks_then_cancels() {
    // The permission mock parks mid-run — the perfect cancellation target.
    let d = start_daemon(&mock_agent_toml("permission"), "default = \"ask\"\n").await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "cancelme", "intent": "write something" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();
    let run: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/runs", d.url))
        .json(&serde_json::json!({ "agent": "mock", "prompt": "write something" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();

    // Parked (waiting permission).
    poll_until(&http, &format!("{}/api/v1/permissions", d.url), |v| {
        !v["pending"].as_array().unwrap().is_empty()
    })
    .await;

    // Cancel.
    let status = http
        .post(format!("{}/api/v1/runs/{run_id}/cancel", d.url))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, axum::http::StatusCode::ACCEPTED);

    let final_run = poll_until(&http, &format!("{}/api/v1/runs/{run_id}", d.url), |v| {
        v["status"] == "cancelled"
    })
    .await;
    assert_eq!(final_run["stop_reason"], "cancelled");

    // The ask is gone from the inbox (cleaned up at finalize).
    let pending: serde_json::Value = http
        .get(format!("{}/api/v1/permissions", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(pending["pending"].as_array().unwrap().is_empty());

    // Cancelling again: rejected with a clear message.
    let resp = http
        .post(format!("{}/api/v1/runs/{run_id}/cancel", d.url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// M4: graph REST surface (#28)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graph_endpoints_roundtrip() {
    let d = start_daemon(&mock_agent_toml("echo"), "default = \"ask\"\n").await;
    let http = reqwest::Client::new();

    // Create entities.
    let alice: serde_json::Value = http
        .post(format!("{}/api/v1/graph/entity", d.url))
        .json(&serde_json::json!({ "name": "Alice", "kind": "person" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let acme: serde_json::Value = http
        .post(format!("{}/api/v1/graph/entity", d.url))
        .json(&serde_json::json!({ "name": "Acme", "kind": "org" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let alice_id = alice["id"].as_i64().unwrap();
    let acme_id = acme["id"].as_i64().unwrap();

    // Add a fact valid from January.
    let fact: serde_json::Value = http
        .post(format!("{}/api/v1/graph/fact", d.url))
        .json(&serde_json::json!({
            "src": alice_id, "dst": acme_id,
            "relation": "works_at",
            "fact_text": "Alice works at Acme",
            "valid_at": "2026-01-15T00:00:00Z"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // Current facts.
    let facts: serde_json::Value = http
        .get(format!("{}/api/v1/graph/entity/{alice_id}", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(facts["facts"].as_array().unwrap().len(), 1);

    // As-of before January: nothing.
    let before: serde_json::Value = http
        .get(format!(
            "{}/api/v1/graph/entity/{alice_id}/facts?at=2025-12-01T00:00:00Z",
            d.url
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(before["facts"].as_array().unwrap().is_empty());

    // Neighbors: Alice sees Acme at hop 1.
    let nbrs: serde_json::Value = http
        .get(format!(
            "{}/api/v1/graph/entity/{alice_id}/neighbors",
            d.url
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(nbrs["neighbors"][0][0]["name"] == "Acme");

    // Search finds Alice by name (the FTS index covers name + summary).
    let hits: serde_json::Value = http
        .get(format!("{}/api/v1/graph/search?q=alice", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!hits["entities"].as_array().unwrap().is_empty());

    // Entity list.
    let list: serde_json::Value = http
        .get(format!("{}/api/v1/graph/entities", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(list["entities"].as_array().unwrap().len() >= 2);
    let _ = fact;
}

#[tokio::test]
async fn harness_concurrency_gates_and_queues() {
    // per_harness = 1 (design §8.3): the second run on the same harness
    // must queue (the permission mock parks the first indefinitely),
    // and beyond the queue cap launches are rejected with a clear
    // error instead of queueing forever.
    let d = start_daemon(
        &mock_agent_toml("permission"),
        "[permissions]\ndefault = \"ask\"\n\n[concurrency]\nper_harness = 1\nqueue_per_harness = 1\n",
    )
    .await;
    let http = reqwest::Client::new();

    let mk_task = |n: &str| {
        let http = http.clone();
        let url = d.url.clone();
        let title = format!("gate {n}");
        async move {
            let t: serde_json::Value = http
                .post(format!("{url}/api/v1/tasks"))
                .json(&serde_json::json!({ "title": title, "intent": "write something" }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            t["id"].as_str().unwrap().to_string()
        }
    };
    // POST a run; the caller decides how to read the response because
    // a saturated launch is an error status, not a run row.
    let start = |task: String| {
        let http = http.clone();
        let url = d.url.clone();
        async move {
            http.post(format!("{url}/api/v1/tasks/{task}/runs"))
                .json(&serde_json::json!({ "agent": "mock", "prompt": "write something" }))
                .send()
                .await
                .unwrap()
        }
    };
    // Wait for a pending permission belonging to one specific run.
    let wait_parked = |run_id: String| {
        let http = http.clone();
        let url = d.url.clone();
        async move {
            poll_until(&http, &format!("{url}/api/v1/permissions"), |v| {
                v["pending"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|p| p["run_id"].as_str() == Some(&run_id)))
            })
            .await
        }
    };
    let get_run = |run_id: String| {
        let http = http.clone();
        let url = d.url.clone();
        async move {
            http.get(format!("{url}/api/v1/runs/{run_id}"))
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap()
        }
    };
    let resolve_all = || {
        let http = http.clone();
        let url = d.url.clone();
        async move {
            let pending: serde_json::Value = http
                .get(format!("{url}/api/v1/permissions"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            for p in pending["pending"]
                .as_array()
                .map(|a| a.as_slice())
                .unwrap_or(&[])
            {
                let key = format!(
                    "{}:{}",
                    p["run_id"].as_str().unwrap(),
                    p["tool_call_id"].as_str().unwrap()
                );
                let _ = http
                    .post(format!("{url}/api/v1/permissions/{key}"))
                    .json(&serde_json::json!({ "action": "allow" }))
                    .send()
                    .await;
            }
        }
    };

    // A acquires the only slot and parks on its permission.
    let a: serde_json::Value = start(mk_task("a").await).await.json().await.unwrap();
    let a_id = a["id"].as_str().unwrap().to_string();
    assert_eq!(a["status"], "spawning", "free slot spawns immediately");
    wait_parked(a_id.clone()).await;
    let a_run = get_run(a_id.clone()).await;
    assert_eq!(a_run["status"], "running");

    // B queues behind A: row present, no spawn, no workspace yet.
    let b: serde_json::Value = start(mk_task("b").await).await.json().await.unwrap();
    let b_id = b["id"].as_str().unwrap().to_string();
    assert_eq!(b["status"], "queued", "second run queues: {b}");
    let b_run = get_run(b_id.clone()).await;
    assert_eq!(b_run["status"], "queued");
    assert!(
        b_run["workspace"].is_null(),
        "a queued run must not consume resources: {b_run}"
    );

    // C: the queue cap (1 waiting) rejects the launch; the run row
    // records why, so the failure is observable, not just a 500.
    let c_task = mk_task("c").await;
    let c_resp = start(c_task.clone()).await;
    assert!(
        !c_resp.status().is_success(),
        "saturated launch must be rejected, got {}",
        c_resp.status()
    );
    let c_detail: serde_json::Value = http
        .get(format!("{}/api/v1/tasks/{c_task}", d.url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let c_run = &c_detail["runs"][0];
    assert_eq!(c_run["status"], "failed", "rejected run row: {c_run}");
    assert!(
        c_run["error"]
            .as_str()
            .unwrap_or_default()
            .contains("saturated"),
        "queue-full must say so: {c_run}"
    );

    // Resolve A's permission → A completes → B takes the freed slot
    // and parks on its own permission.
    resolve_all().await;
    let _ = poll_until(&http, &format!("{}/api/v1/runs/{a_id}", d.url), |v| {
        v["status"] == "completed"
    })
    .await;
    wait_parked(b_id.clone()).await;
    let b_run = poll_until(&http, &format!("{}/api/v1/runs/{b_id}", d.url), |v| {
        v["status"] == "running"
    })
    .await;
    assert!(b_run["workspace"].is_string(), "B spawned after the gate");

    // Release B so the mock process exits cleanly.
    resolve_all().await;
    let _ = poll_until(&http, &format!("{}/api/v1/runs/{b_id}", d.url), |v| {
        v["status"] == "completed"
    })
    .await;
}

// ---------------------------------------------------------------------------
// Boot-time orphan sweep (#44)
// ---------------------------------------------------------------------------

#[test]
fn orphan_sweep_finds_and_kills_children() {
    // A parked mock agent is a real child of this test process: the
    // sweep must identify it as a child of its parent and the kill
    // primitive must take down the process tree.
    // Piped stdin: an inherited one hits EOF (no test stdin) and the
    // mock exits instead of parking — the whole point of the test.
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_ruagent-mock-agent"))
        .args(["--behavior", "permission"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    // A freshly created Windows process reports exit code 0 until it
    // initializes (then STILL_ACTIVE) — poll for true liveness.
    let mut alive = false;
    for _ in 0..50 {
        if ruagent_daemon::orphans::process_started_at_ms(pid).is_some() {
            alive = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(alive, "spawned mock never showed up as alive");

    // Identification: it shows up as a child of this process.
    let children = ruagent_daemon::orphans::child_pids_of(std::process::id());
    assert!(children.contains(&pid), "child not found in {children:?}");

    // The recorded start time matches a fresh read (alive check basis).
    let start = ruagent_daemon::orphans::process_started_at_ms(pid).unwrap();
    assert!(start > 0);

    // Kill: the parked process must actually die.
    assert!(ruagent_daemon::orphans::kill_tree(pid));
    let status = child.wait().unwrap();
    assert!(
        !status.success(),
        "a killed process cannot exit cleanly: {status:?}"
    );
    assert!(ruagent_daemon::orphans::process_started_at_ms(pid).is_none());
}

#[test]
fn prior_daemon_record_roundtrips() {
    let root = std::env::temp_dir().join(format!(
        "ruagent-orphan-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    let _ = std::fs::remove_dir_all(&root);
    // First boot: no record.
    assert!(
        ruagent_daemon::orphans::read_prior(&root)
            .unwrap()
            .is_none()
    );
    ruagent_daemon::orphans::write_current(&root).unwrap();
    // Our own pid round-trips with its start time.
    let prior = ruagent_daemon::orphans::read_prior(&root).unwrap().unwrap();
    assert_eq!(prior.pid, std::process::id());
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Failed-run one-click retry (§8.3 crash row)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn failed_run_one_click_retry() {
    // The crash mock dies mid-prompt. The retry is a NEW run on the
    // same task with the same agent + options, reusing the dead
    // attempt's workspace, with its last output injected as context.
    let bin = mock_bin();
    let d = start_daemon(
        &format!(
            "[agent.crasher]\nharness = \"mock\"\ncommand = \"{bin} --behavior crash\"\n\n\
             [agent.echoer]\nharness = \"mock\"\ncommand = \"{bin} --behavior echo\"\n"
        ),
        "default = \"ask\"\n",
    )
    .await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "retryable", "intent": "do the thing" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();
    let run: serde_json::Value = http
        .post(format!("{}/api/v1/tasks/{task_id}/runs", d.url))
        .json(&serde_json::json!({
            "agent": "crasher",
            "prompt": "custom launch prompt",
            "options": { "mode": "auto" },
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();
    let failed = poll_until(&http, &format!("{}/api/v1/runs/{run_id}", d.url), |v| {
        v["status"] == "failed"
    })
    .await;
    assert_eq!(failed["params"]["prompt"], "custom launch prompt");
    let old_ws = failed["workspace"].as_str().unwrap().to_string();
    assert!(old_ws.contains("run-"), "fresh workspace: {old_ws}");

    // Retry: a new run row on the same task, params preserved.
    let resp = http
        .post(format!("{}/api/v1/runs/{run_id}/retry", d.url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::CREATED);
    let retry: serde_json::Value = resp.json().await.unwrap();
    let retry_id = retry["id"].as_str().unwrap().to_string();
    assert_ne!(retry_id, run_id);
    assert_eq!(retry["task_id"], task_id);

    // The crash mock dies again (faithful retry) — but on the SAME
    // workspace, with the dead attempt's last words as context.
    let retried = poll_until(&http, &format!("{}/api/v1/runs/{retry_id}", d.url), |v| {
        v["status"] == "failed"
    })
    .await;
    assert_eq!(
        retried["workspace"], old_ws,
        "the retry continues in the dead attempt's workspace"
    );
    assert_eq!(retried["params"]["options"]["mode"], "auto");
    // The original ask is preserved verbatim — chained retries must
    // not grow the record.
    assert_eq!(
        retried["params"]["prompt"], "custom launch prompt",
        "params.prompt must stay the original: {}",
        retried["params"]["prompt"]
    );
    let root = std::path::Path::new(&old_ws)
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let transcript = std::fs::read_to_string(
        root.join("data")
            .join("transcripts")
            .join(format!("run-{retry_id}.jsonl")),
    )
    .unwrap_or_else(|e| panic!("retry transcript unreadable: {e} (root {})", root.display()));
    // The crash snapshot rides as CONTEXT (context_injected), the
    // user_message stays the clean original ask — session previews
    // and distillation never see injected text as user speech.
    let lines: Vec<serde_json::Value> = transcript
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let ctx = lines
        .iter()
        .find(|l| l["event"]["type"] == "context_injected");
    assert!(
        ctx.is_some_and(|l| {
            let r = l["event"]["render"].as_str().unwrap_or_default();
            r.contains("retry context") && r.contains("about to crash")
        }),
        "the crash snapshot rides the ContextInjected render"
    );
    let user = lines.iter().find(|l| l["event"]["type"] == "user_message");
    assert!(
        user.is_some_and(|l| l["event"]["text"] == "custom launch prompt"),
        "user_message must be the clean original ask, not the composed prompt"
    );

    // A completed run is not retryable — "run again" is a different
    // gesture.
    let task2: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "done", "intent": "say hi" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run2: serde_json::Value = http
        .post(format!(
            "{}/api/v1/tasks/{}/runs",
            d.url,
            task2["id"].as_str().unwrap()
        ))
        .json(&serde_json::json!({ "agent": "echoer", "prompt": "say hi" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let _ = poll_until(
        &http,
        &format!("{}/api/v1/runs/{}", d.url, run2["id"].as_str().unwrap()),
        |v| v["status"] == "completed",
    )
    .await;
    let resp = http
        .post(format!(
            "{}/api/v1/runs/{}/retry",
            d.url,
            run2["id"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Role identity in runs (the specialist model)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn role_prompt_rides_runs() {
    // A run against a ROLE carries the role's identity ahead of the
    // prompt — the same contract chats use. The specialist team model
    // depends on it: without the role block, a run is just a runtime.
    let bin = mock_bin();
    let d = start_daemon(
        &format!(
            "[runtime.mockrt]\nharness = \"mock\"\ncommand = \"{bin} --behavior echo\"\n\n\
             [agent.specialist]\nprompt = \"you are the gatekeeper of truth\"\nruntimes = [\"mockrt\"]\nruntime = \"mockrt\"\n"
        ),
        "default = \"ask\"\n",
    )
    .await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "role", "intent": "hello" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run: serde_json::Value = http
        .post(format!(
            "{}/api/v1/tasks/{}/runs",
            d.url,
            task["id"].as_str().unwrap()
        ))
        .json(&serde_json::json!({ "agent": "specialist", "prompt": "hello role" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();
    let final_run = poll_until(&http, &format!("{}/api/v1/runs/{run_id}", d.url), |v| {
        v["status"] == "completed"
    })
    .await;
    let result = final_run["result"].as_str().unwrap_or_default();
    assert!(
        result.contains("[role — you are]") && result.contains("you are the gatekeeper of truth"),
        "the role block must ride the run prompt: {result}"
    );
    assert!(
        result.contains("hello role"),
        "the prompt itself survives: {result}"
    );
}

#[tokio::test]
async fn role_option_defaults_apply_to_runs() {
    // [agent.X.options] defaults ride RUNS too, not just chats — a
    // specialist registered with mode=auto must not ask for every
    // edit in run mode. The mock reports applied options back.
    let bin = mock_bin();
    let d = start_daemon(
        &format!(
            "[runtime.mockrt]\nharness = \"mock\"\ncommand = \"{bin} --behavior configdump\"\n\n\
             [agent.specialist]\nprompt = \"you are the tester\"\nruntimes = [\"mockrt\"]\nruntime = \"mockrt\"\n\n\
             [agent.plainer]\nprompt = \"no defaults\"\nruntimes = [\"mockrt\"]\nruntime = \"mockrt\"\n\n\
             [agent.specialist.options]\nmode = \"auto\"\n"
        ),
        "default = \"ask\"\n",
    )
    .await;
    let http = reqwest::Client::new();

    let drive = |agent: &str, extra: serde_json::Value| {
        let http = http.clone();
        let url = d.url.clone();
        let agent = agent.to_string();
        async move {
            let task: serde_json::Value = http
                .post(format!("{url}/api/v1/tasks"))
                .json(&serde_json::json!({ "title": "opts", "intent": "dump" }))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let mut body = serde_json::json!({ "agent": agent, "prompt": "dump" });
            if let (Some(dst), Some(src)) = (body.as_object_mut(), extra.as_object()) {
                for (k, v) in src {
                    dst.insert(k.clone(), v.clone());
                }
            }
            let run: serde_json::Value = http
                .post(format!(
                    "{}/api/v1/tasks/{}/runs",
                    url,
                    task["id"].as_str().unwrap()
                ))
                .json(&body)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let run_id = run["id"].as_str().unwrap().to_string();
            poll_until(&http, &format!("{url}/api/v1/runs/{run_id}"), |v| {
                v["status"] == "completed"
            })
            .await
        }
    };

    // Role default applies.
    let r = drive("specialist", serde_json::json!({})).await;
    assert!(
        (r["result"].as_str().unwrap_or_default()).contains("mode=auto"),
        "role default options must ride runs: {}",
        r["result"]
    );
    assert_eq!(r["params"]["options"]["mode"], "auto");

    // A role WITHOUT defaults stays clean.
    let r = drive("plainer", serde_json::json!({})).await;
    assert_eq!(r["params"]["options"].as_object().map(|m| m.len()), Some(0));

    // The request's explicit option overrides the role default per key
    // (the mock's mode choices are ask/auto).
    let r = drive(
        "specialist",
        serde_json::json!({ "options": { "mode": "ask" } }),
    )
    .await;
    assert!(
        (r["result"].as_str().unwrap_or_default()).contains("mode=ask"),
        "request options win: {}",
        r["result"]
    );
    assert_eq!(r["params"]["options"]["mode"], "ask");
}
