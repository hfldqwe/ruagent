//! P5 (`docs/plans/2026-11-23-reports-never-lost.md` §2): the self-heal budget belongs to a HUMAN
//! DISPATCH, not to the task.
//!
//! THE BUG THIS PINS. `healed_already` used to be «does ANY attempt of this task carry
//! `self_heal_of`?», which stays true for ever after the first heal — so a task a human
//! re-dispatched a second and third time got no automatic retry at all, exactly when re-dispatching
//! was the sign they wanted it done. Now each dispatch is a GENERATION (`params.dispatch_epoch`) and
//! each generation gets one heal.
//!
//! Both halves are asserted, because either alone would hide the bug: a re-dispatch MUST heal
//! (otherwise P5 did nothing), and one generation MUST NOT heal twice (otherwise the criterion was
//! merely loosened into "heal for ever"). The legacy half — rows written before the field existed —
//! asserts the conservative direction: an old heal attempt still counts, so old clusters behave
//! exactly as they did before this change.
//!
//! The failure is real: `crates/mock-agent --behavior crash` (chunks, waits 150 ms, exits 1), so the
//! whole chain runs with no API keys and no real harness. What is asserted is the CLUSTER'S ROWS
//! (`GET /tasks/{id}` -> runs), not the chat's delivery: whether a report reaches a chat is P2/P4's
//! subject, and an assertion about it here would make this test break when that work lands.

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

struct Harness {
    base: String,
    #[allow(dead_code)]
    root: std::path::PathBuf,
    /// The daemon's OWN handle: the legacy test writes pre-versioning rows, and the rule is that
    /// SQLite writes go through the one single-writer actor this process already opened.
    db: Db,
}

async fn boot(tag: &str, mock: &str) -> Harness {
    let root = std::env::temp_dir().join(format!(
        "ruagent-heal-epoch-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(root.join("data")).unwrap();
    std::fs::write(
        config_dir.join("agents.toml"),
        format!(
            "[agent.boom-a]\nharness = \"mock\"\ncommand = '{mock} --behavior crash'\n\
             description = \"fails on every prompt\"\n\n\
             [agent.boom-b]\nharness = \"mock\"\ncommand = '{mock} --behavior crash'\n\
             description = \"fails on every prompt too (the self-heal target)\"\n\n\
             [agent.talker]\nharness = \"mock\"\ncommand = '{mock} --behavior echo'\n\
             description = \"the conversation the work was dispatched from\"\n"
        ),
    )
    .unwrap();
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
    let mgr = Arc::new(RunManager::new(
        db.clone(),
        root.clone(),
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));
    mgr.start_approver_loop();
    let mgr_for_chats = Arc::clone(&mgr);
    let chats = ruagent_daemon::chat::ChatManager::new(
        db.clone(),
        root.clone(),
        Arc::new(move |ask, context_id| mgr_for_chats.park_external_ask(ask, context_id)),
        cfg.mcp.clone(),
        ruagent_daemon::distill::AutoDistill::default(),
        Some(knowledge.embedder()),
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    chats.set_ask_dropper({
        let mgr = Arc::clone(&mgr);
        Arc::new(move |id| mgr.drop_pending_for(id))
    });
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
    Harness {
        base: format!("http://{addr}"),
        root,
        db,
    }
}

async fn post_json(
    http: &reqwest::Client,
    url: String,
    body: serde_json::Value,
    want: u16,
) -> serde_json::Value {
    let resp = http.post(url).json(&body).send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    assert_eq!(status, want, "POST {body} -> {status}: {text}");
    serde_json::from_str(&text).unwrap_or(serde_json::Value::Null)
}

async fn get_json(http: &reqwest::Client, url: String) -> serde_json::Value {
    let resp = http.get(url).send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    assert_eq!(status, 200, "GET -> {status}: {text}");
    serde_json::from_str(&text).unwrap()
}

fn ended(run: &serde_json::Value) -> bool {
    matches!(
        run["status"].as_str(),
        Some("completed") | Some("failed") | Some("cancelled") | Some("interrupted")
    )
}

/// Wait until the cluster has `want` attempts, every one of them terminal, and the task is no longer
/// `in_progress` — then hold one settle window and re-read, so a run that is ABOUT to be inserted by
/// the completion hook shows up instead of being missed.
///
/// WHY THE SETTLE WINDOW: the hook writes the task status back BEFORE it decides about the heal, so
/// "all attempts terminal + task not in_progress" is also true in the instant before a heal is
/// inserted. Waiting for it and asserting immediately would be a race that passes for the wrong
/// reason (measured shape: the same window is why `supervise_wakeup.rs` waits on the report).
/// What this returns is the reading AFTER the window, so "N attempts" really means "N and no more".
async fn settle(
    http: &reqwest::Client,
    task_url: &str,
    want: usize,
    what: &str,
) -> serde_json::Value {
    for attempt in 0..300 {
        let reading = get_json(http, task_url.to_string()).await;
        let all_ended = reading["runs"]
            .as_array()
            .is_some_and(|runs| runs.len() == want && runs.iter().all(ended));
        if all_ended && reading["task"]["status"] != "in_progress" {
            break;
        }
        assert!(
            attempt < 299,
            "timed out waiting for {want} ended attempts ({what}); last reading: {reading}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // The settle window: long enough for a heal to be STARTED and inserted (a crash run lives
    // ~150 ms, and a heal is inserted in the same hook), short enough to keep the suite quick.
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    let after = get_json(http, task_url.to_string()).await;
    assert!(
        after["runs"]
            .as_array()
            .is_some_and(|runs| runs.len() == want),
        "expected {want} attempts and no more ({what}): {after} \
         (a cluster that keeps adding attempts is the 'heal for ever' failure)"
    );
    after
}

fn epochs(task_view: &serde_json::Value) -> Vec<serde_json::Value> {
    task_view["runs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| r["params"]["dispatch_epoch"].clone())
        .collect()
}

fn heal_rows(task_view: &serde_json::Value) -> Vec<serde_json::Value> {
    task_view["runs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r["params"]["self_heal_of"].as_str().is_some())
        .collect()
}

/// P5's main claim: the SAME task dispatched TWICE, failing twice, heals TWICE — once per dispatch —
/// and each generation heals exactly once.
#[tokio::test]
async fn a_second_dispatch_of_the_same_failing_task_gets_its_own_self_heal() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = boot("twice", &mock).await;
    let http = reqwest::Client::new();

    // A live conversation: the completion hook only heals a cluster that somebody is waiting for
    // (P2.1), which is unrelated to P5 and shared by both dispatches.
    let chat = post_json(
        &http,
        format!("{}/api/v1/chat", h.base),
        serde_json::json!({ "agent": "talker" }),
        200,
    )
    .await;
    let chat_id = chat["id"].as_str().expect("chat id").to_string();

    let task = post_json(
        &http,
        format!("{}/api/v1/tasks", h.base),
        serde_json::json!({
            "title": "同一个任务派两次都得自愈",
            "intent": "fail, be healed, fail again; then be re-dispatched and get the same treatment"
        }),
        200,
    )
    .await;
    let task_id = task["id"].as_str().expect("task id").to_string();
    let task_url = format!("{}/api/v1/tasks/{task_id}", h.base);

    // ---- DISPATCH 1 (generation 0) --------------------------------------
    let first = post_json(
        &http,
        format!("{}/api/v1/tasks/{task_id}/runs", h.base),
        serde_json::json!({ "agent": "boom-a", "origin_chat_id": chat_id }),
        200,
    )
    .await;
    let first_run_id = first["id"].as_str().expect("run id").to_string();
    let after_first = settle(&http, &task_url, 2, "dispatch 1: one attempt + one heal").await;
    println!("READING P5 after dispatch 1: {after_first:#}");
    let heals = heal_rows(&after_first);
    assert_eq!(
        heals.len(),
        1,
        "the first dispatch gets exactly one automatic retry: {after_first}"
    );
    assert_eq!(
        heals[0]["params"]["self_heal_of"].as_str(),
        Some(first_run_id.as_str()),
        "the heal names the attempt it heals"
    );
    assert_eq!(
        epochs(&after_first),
        vec![serde_json::json!(0), serde_json::json!(0)],
        "the heal belongs to the SAME generation as the attempt it heals: {after_first}"
    );
    assert_eq!(
        after_first["task"]["status"], "pending",
        "a failed cluster returns the task to pending (re-dispatchable): {after_first}"
    );

    // ---- DISPATCH 2 (the human tries again -> generation 1) -------------
    let second = post_json(
        &http,
        format!("{}/api/v1/tasks/{task_id}/runs", h.base),
        serde_json::json!({ "agent": "boom-a", "origin_chat_id": chat_id }),
        200,
    )
    .await;
    let second_run_id = second["id"].as_str().expect("run id").to_string();
    assert_eq!(
        second["params"]["dispatch_epoch"],
        serde_json::json!(1),
        "the second human dispatch is its own generation: {second}"
    );
    let after_second = settle(&http, &task_url, 4, "dispatch 2: the same shape again").await;
    println!("READING P5 after dispatch 2: {after_second:#}");
    let heals = heal_rows(&after_second);
    assert_eq!(
        heals.len(),
        2,
        "EACH dispatch heals once, so the re-dispatched task has TWO heals (this is P5: before the \
         fix the second dispatch was told the budget was spent): {after_second}"
    );
    let second_heal = heals
        .iter()
        .find(|r| r["params"]["self_heal_of"].as_str() == Some(second_run_id.as_str()))
        .unwrap_or_else(|| {
            panic!("the second dispatch's own heal must name its attempt: {after_second}")
        });
    assert_eq!(
        second_heal["params"]["dispatch_epoch"],
        serde_json::json!(1),
        "and it belongs to generation 1, not to generation 0: {after_second}"
    );
    assert_eq!(
        epochs(&after_second),
        vec![
            serde_json::json!(0),
            serde_json::json!(0),
            serde_json::json!(1),
            serde_json::json!(1)
        ],
        "two dispatches, two generations, two attempts each (never a third attempt inside one \
         generation): {after_second}"
    );

    // ---- AND IT DOES NOT GO ON FOR EVER ---------------------------------
    // One generation, one heal. The settle() calls above already required the cluster to stop
    // growing at 2 and at 4; this states the criterion directly on the rows, so a future change that
    // drops `heal_budget_spent` to "any heal counts" fails HERE with the right sentence.
    for generation in [0, 1] {
        let heals_in_generation = heal_rows(&after_second)
            .iter()
            .filter(|r| r["params"]["dispatch_epoch"] == serde_json::json!(generation))
            .count();
        assert_eq!(
            heals_in_generation, 1,
            "generation {generation} must heal EXACTLY once: {after_second}"
        );
    }
    assert_eq!(
        after_second["task"]["status"], "pending",
        "the board still tells the truth after the second failure: {after_second}"
    );
}

/// The COMPATIBILITY rule, measured: a cluster whose heal attempt was written BEFORE
/// `params.dispatch_epoch` existed (`None`) must not be granted a fresh budget. The old row counts
/// as "this generation" — the conservative direction, because the expensive way to be wrong is a
/// silent extra round of the user's tokens.
#[tokio::test]
async fn a_pre_versioning_heal_row_still_counts_as_this_generation() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = boot("legacy", &mock).await;
    let http = reqwest::Client::new();

    let chat = post_json(
        &http,
        format!("{}/api/v1/chat", h.base),
        serde_json::json!({ "agent": "talker" }),
        200,
    )
    .await;
    let chat_id = chat["id"].as_str().expect("chat id").to_string();

    let task = post_json(
        &http,
        format!("{}/api/v1/tasks", h.base),
        serde_json::json!({ "title": "旧数据不该多给预算", "intent": "a cluster written before P5" }),
        200,
    )
    .await;
    let task_id: ruagent_core::TaskId = task["id"].as_str().expect("task id").parse().unwrap();
    let task_url = format!("{}/api/v1/tasks/{task_id}", h.base);

    // Two attempts as they exist on disk BEFORE this change: no `dispatch_epoch` at all, one of them
    // the cluster's heal. Inserted through the daemon's own handle (the single writer), because the
    // API has no way to write that shape — which is the point: it is not a shape this code creates.
    let agent_id =
        h.db.agent_id_by_name("boom-a")
            .await
            .unwrap()
            .expect("boom-a is registered");
    let mut plain = ruagent_core::Run::new(task_id, ruagent_core::RunParams::for_agent(agent_id));
    plain.status = ruagent_core::RunStatus::Failed;
    plain.error = Some("old failure".into());
    plain.stop_reason = Some(ruagent_core::StopReason::Error);
    h.db.insert_run(&plain).await.unwrap();
    let mut heal = ruagent_core::Run::new(task_id, ruagent_core::RunParams::for_agent(agent_id));
    heal.params.self_heal_of = Some(plain.id);
    heal.status = ruagent_core::RunStatus::Failed;
    heal.error = Some("old heal failure".into());
    heal.stop_reason = Some(ruagent_core::StopReason::Error);
    h.db.insert_run(&heal).await.unwrap();
    // The reading that makes this test mean what it says: the rows really are un-versioned.
    let written = h.db.list_runs_for_task(task_id).await.unwrap();
    assert!(
        written.iter().all(|r| r.params.dispatch_epoch.is_none()),
        "the fixture must be the pre-versioning shape: {written:#?}"
    );

    // A human dispatches the task again — this IS a new generation, with a budget in principle.
    let started = post_json(
        &http,
        format!("{}/api/v1/tasks/{task_id}/runs", h.base),
        serde_json::json!({ "agent": "boom-a", "origin_chat_id": chat_id }),
        200,
    )
    .await;
    assert!(
        started["params"]["dispatch_epoch"].as_u64().is_some(),
        "the NEW attempt is versioned: {started}"
    );

    let settled = settle(&http, &task_url, 3, "legacy cluster: no third attempt").await;
    println!("READING P5 legacy cluster after one more dispatch: {settled:#}");
    assert_eq!(
        heal_rows(&settled).len(),
        1,
        "the un-versioned heal already spent the budget, so this cluster gets NO new one (the \
         conservative direction: an old cluster behaves exactly as it did before P5): {settled}"
    );
    assert_eq!(
        settled["runs"]
            .as_array()
            .map(|r| r.len())
            .unwrap_or_default(),
        3,
        "the two legacy attempts plus the new one, and nothing else: {settled}"
    );
}
