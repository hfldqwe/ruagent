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
    let mgr = Arc::new(RunManager::new(
        db,
        root.clone(),
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));

    let app = ruagent_daemon::api::router(AppState {
        mgr,
        config: Arc::new(cfg),
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

    // Answer: allow.
    let status = http
        .post(format!("{}/api/v1/permissions/{key}", d.url))
        .json(&serde_json::json!({ "action": "allow" }))
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
