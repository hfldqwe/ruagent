//! Fan-out judge integration (design §5.2/§5.3): two scripted members
//! produce different answers, the `judge` mock reviews them and picks the
//! first `[RUN <id>]` candidate — the verdict must land on the task as
//! `selected_run_id` with `agent:judge` provenance, plus a Reviews edge
//! and the parsed rationale in the task detail view.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A daemon with three mock agents: two scripted fan-out members (same
/// prompt marker, different replies files) and the judge.
async fn start_judge_daemon() -> (String, std::path::PathBuf) {
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-judge-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Windows paths must use forward slashes here: TOML basic strings
    // treat backslashes as escapes.
    let bin = env!("CARGO_BIN_EXE_ruagent-mock-agent").replace('\\', "/");
    let mut toml = String::new();
    for name in ["alpha", "beta"] {
        let replies_path = root.join(format!("{name}-replies.json"));
        std::fs::write(
            &replies_path,
            format!(
                "[{{\"marker\":\"FANOUT MARKER\",\"reply\":\"answer from {name}\"}}]"
            ),
        )
        .unwrap();
        let replies = replies_path.to_string_lossy().replace('\\', "/");
        toml.push_str(&format!(
            "[agent.{name}]\nharness = \"mock\"\ncommand = \"{bin} --behavior scripted --replies {replies}\"\ndescription = \"{name} member\"\n\n"
        ));
    }
    toml.push_str(&format!(
        "[agent.judge]\nharness = \"mock\"\ncommand = \"{bin} --behavior judge\"\ndescription = \"fan-out judge\"\n"
    ));
    std::fs::write(config_dir.join("agents.toml"), toml).unwrap();
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

/// Poll a predicate over the task detail until it holds (10s ceiling).
async fn wait_task<F>(http: &reqwest::Client, url: &str, id: &str, mut ok: F) -> serde_json::Value
where
    F: FnMut(&serde_json::Value) -> bool,
{
    for _ in 0..80 {
        let resp = http
            .get(format!("{url}/api/v1/tasks/{id}"))
            .send()
            .await
            .unwrap();
        let status = resp.status();
        let body = resp.text().await.unwrap();
        let t: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| {
            panic!("task detail GET returned {status}: {e}\nbody: {body}")
        });
        if ok(&t) {
            return t;
        }
        tokio::time::sleep(std::time::Duration::from_millis(125)).await;
    }
    panic!("task condition not reached in time");
}

#[tokio::test]
async fn fanout_judge_picks_winner_and_records_provenance() {
    let (url, _root) = start_judge_daemon().await;
    let http = reqwest::Client::new();

    // 1. Task + fan-out to two members with different answers.
    let task: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks"))
        .json(&serde_json::json!({
            "title": "Widget design",
            "intent": "FANOUT MARKER: design the widget",
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    let _: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks/{task_id}/fanout"))
        .json(&serde_json::json!({ "agents": ["alpha", "beta"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // 2. Wait for both members to complete; the first run in the listing
    //    is the first `[RUN ...]` candidate the mock judge will pick.
    let detail = wait_task(&http, &url, &task_id, |t| {
        let runs = t["runs"].as_array().unwrap();
        runs.len() == 2
            && runs
                .iter()
                .all(|r| r["status"] == serde_json::json!("completed"))
    })
    .await;
    let expected_winner = detail["runs"][0]["id"].as_str().unwrap().to_string();
    let other = detail["runs"][1]["id"].as_str().unwrap().to_string();
    assert_ne!(expected_winner, other);
    assert_eq!(detail["runs"][0]["result"], "answer from alpha");
    assert_eq!(detail["runs"][1]["result"], "answer from beta");

    // 3. Judge it.
    let judged: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks/{task_id}/judge"))
        .json(&serde_json::json!({ "agent": "judge" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(judged["judge_run"]["status"], "spawning");

    // 4. The verdict lands: selection with agent provenance, the parsed
    //    rationale, and the Reviews-edge-backed judgement view. The
    //    selection is written by a background driver that polls the run —
    //    wait for it, not just for the run completing.
    let detail = wait_task(&http, &url, &task_id, |t| {
        t["judgement"].is_object()
            && t["judgement"]["judge_run_status"] == serde_json::json!("completed")
            && !t["selected_run_id"].is_null()
    })
    .await;
    assert_eq!(detail["selected_run_id"], expected_winner.as_str());
    assert_eq!(detail["selected_by"], "agent:judge");
    assert_eq!(detail["judgement"]["winner_run_id"], expected_winner.as_str());
    assert_eq!(
        detail["judgement"]["rationale"],
        "mock judge prefers the first candidate"
    );

    // 5. A human override re-marks the selection as human (design §5.2:
    //    the human pick stays the default authority).
    let resp = http
        .post(format!("{url}/api/v1/runs/{other}/select"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);
    let detail: serde_json::Value = http
        .get(format!("{url}/api/v1/tasks/{task_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["selected_run_id"], other.as_str());
    assert_eq!(detail["selected_by"], "human");
    // The judge's own verdict survives the override.
    assert_eq!(
        detail["judgement"]["winner_run_id"],
        expected_winner.as_str()
    );
}

#[tokio::test]
async fn judge_needs_two_completed_results() {
    let (url, _root) = start_judge_daemon().await;
    let http = reqwest::Client::new();

    let task: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks"))
        .json(&serde_json::json!({
            "title": "Solo",
            "intent": "FANOUT MARKER: only one run",
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    let _: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks/{task_id}/runs"))
        .json(&serde_json::json!({ "agent": "alpha" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    wait_task(&http, &url, &task_id, |t| {
        t["runs"].as_array().unwrap().len() == 1
            && t["runs"][0]["status"] == serde_json::json!("completed")
    })
    .await;

    let resp = http
        .post(format!("{url}/api/v1/tasks/{task_id}/judge"))
        .json(&serde_json::json!({ "agent": "judge" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    // ApiError bodies are plain text, not JSON.
    let body = resp.text().await.unwrap();
    assert!(body.contains("at least two completed runs"));
}
