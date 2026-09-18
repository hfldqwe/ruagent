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
    let bin = env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/");
    format!(
        "[agent.mock]
harness = \"mock\"
command = \"{bin} --behavior {behavior}\"
description = \"mock\"
"
    )
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

#[tokio::test]
async fn fanout_with_repo_gives_each_run_its_own_worktree() {
    // A real git repo to isolate in.
    static REPO_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = REPO_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let repo = std::env::temp_dir().join(format!("ruagent-repo-{}-{}", std::process::id(), seq));
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("hello.txt"), "base").unwrap();
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-q", "-b", "main"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .args(["commit", "-q", "-m", "base"])
            .status()
            .unwrap()
            .success()
    );

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
