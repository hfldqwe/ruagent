//! P0 + P2 END TO END: a chat dispatches work, the work fails, the platform heals it once, it
//! still fails, and THE CHAT REALLY GETS A NEW MESSAGE.
//!
//! This is the acceptance the design demands (`docs/plans/2026-11-20-primary-supervisor-design.md`
//! §5): "POST /tasks/{id}/runs 带 origin_chat_id → run 失败 → 自愈一次 → 仍失败 → chat 里真的多了
//! 一条平台事件". It therefore asserts on the CHAT'S OWN MESSAGE LIST — the transcript the panel and
//! the SSE route read — never on "some function was called". A unit test that observed the hook
//! firing would prove nothing about whether a user sees a report.
//!
//! The failing agent is `crates/mock-agent` (a first-class test asset: `--behavior crash` emits a
//! chunk, waits 150 ms, then exits 1), so this runs with no API keys and no real harness. TWO crash
//! agents are registered on purpose: the self-heal must be able to move to a DIFFERENT one, and the
//! test asserts from the rows that it did.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

const NEEDS_MOCK: &str =
    "the mock ACP agent is missing: cargo build -p ruagent-mock-agent --bins (CI builds it)";

fn mock_bin() -> Option<String> {
    // The same resolution the other daemon integration tests use (`chat_front_door.rs`,
    // `capabilities.rs`): the workspace build puts the binary next to the test executable.
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
    root: std::path::PathBuf,
}

/// A daemon over a PRIVATE root: two failing agents (`boom-a`, `boom-b`), one chat agent
/// (`talker`), and a real ChatManager wired the way `lib.rs` wires it (the park closure, the
/// ask-dropper and an embedder, which a prompt — and therefore a report — needs).
async fn boot(tag: &str, mock: &str) -> Harness {
    let root = std::env::temp_dir().join(format!(
        "ruagent-supervise-{tag}-{}-{}",
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
             description = \"the conversation the work was dispatched from\"\n\n\
             [agent.slow]\nharness = \"mock\"\ncommand = '{mock} --behavior slowreply'\n\
             description = \"stays live for 10 s (the slow member of a cluster)\"\n"
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
    // The wiring `lib.rs` does: chat asks land in the shared inbox, and a closed chat drops them.
    let mgr_for_chats = Arc::clone(&mgr);
    let chats = ruagent_daemon::chat::ChatManager::new(
        db.clone(),
        root.clone(),
        Arc::new(move |ask, context_id| mgr_for_chats.park_external_ask(ask, context_id)),
        cfg.mcp.clone(),
        ruagent_daemon::distill::AutoDistill::default(),
        // An embedder: a prompt cannot be built without one, and the platform report IS a prompt.
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

/// Every message the chat has, as the panel/SSE reads it: the chat's own transcript lines.
///
/// WHY THE TRANSCRIPT AND NOT AN HTTP LIST: a chat's message list IS this file — `chat_events`
/// replays it from seq 0 and the panel renders what it replays. There is no `GET /chat/{id}/
/// messages` route to call, and inventing one for a test would test the test.
fn chat_messages(root: &std::path::Path, chat_id: &str) -> Vec<ruagent_core::RunEvent> {
    let id: ruagent_core::RunId = chat_id.parse().expect("chat ids are run ids");
    let path = ruagent_store::transcript_path(root.join("data").join("transcripts"), &id);
    ruagent_store::read_transcript(&path)
        .unwrap_or_default()
        .into_iter()
        .map(|line| line.event)
        .collect()
}

/// Bounded wait for a condition on `GET /tasks/{id}` (100 × 100 ms would race a crash-and-heal
/// cycle that includes two process spawns; this polls up to ~30 s and returns the LAST reading, so
/// a condition that never holds still fails on the assertion below with the real value).
async fn poll_task<F>(http: &reqwest::Client, url: String, f: F, what: &str) -> serde_json::Value
where
    F: Fn(&serde_json::Value) -> bool,
{
    let mut last = serde_json::Value::Null;
    for _ in 0..300 {
        last = get_json(http, url.clone()).await;
        if f(&last) {
            return last;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for: {what}; last reading: {last}");
}

/// THE ACCEPTANCE, in one test, on the real wire.
#[tokio::test]
async fn a_failed_chat_dispatched_cluster_reports_into_the_chat_after_one_self_heal() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = boot("wake", &mock).await;
    let http = reqwest::Client::new();

    // The conversation that will be woken. It must be LIVE (a real session), because a report is a
    // prompt: `ChatManager::notify` refuses to invent a turn for a chat that is gone.
    let chat = post_json(
        &http,
        format!("{}/api/v1/chat", h.base),
        serde_json::json!({ "agent": "talker" }),
        200,
    )
    .await;
    let chat_id = chat["id"].as_str().expect("chat id").to_string();
    assert_eq!(chat["agent"], "talker");
    assert!(
        chat_messages(&h.root, &chat_id).is_empty()
            || !chat_messages(&h.root, &chat_id).iter().any(|e| matches!(
                e,
                ruagent_core::RunEvent::UserMessage { text } if text.contains("平台事件")
            )),
        "the chat must start with no platform report in it, or the assertion below is vacuous"
    );

    // The work item, then ONE dispatch that names the conversation it answers to.
    let task = post_json(
        &http,
        format!("{}/api/v1/tasks", h.base),
        serde_json::json!({
            "title": "把 report.md 的结论整理成一页纸",
            "intent": "summarise the report into one page",
        }),
        200,
    )
    .await;
    let task_id = task["id"].as_str().expect("task id").to_string();
    let started = post_json(
        &http,
        format!("{}/api/v1/tasks/{task_id}/runs", h.base),
        serde_json::json!({ "agent": "boom-a", "origin_chat_id": chat_id }),
        200,
    )
    .await;
    assert_eq!(
        started["origin_chat_id"].as_str(),
        Some(chat_id.as_str()),
        "P2.1: the run must RECORD the conversation that dispatched it — if this is null, the \
         dispatch site in api.rs (`start_run`) is not passing `origin_chat_id` into `RunLaunch`. \
         got: {started}"
    );
    let first_run_id = started["id"].as_str().expect("run id").to_string();

    // WAIT FOR THE DECISION, NOT FOR A SNAPSHOT (measured: 1/10 red before this).
    //
    // "every run that exists right now is terminal" is TRUE in the window between an attempt
    // ending and the self-heal retry being inserted, so a reading taken then describes a cluster
    // that has not finished deciding — the task is still `in_progress` and a third row is about to
    // appear. The PLATFORM REPORT is the load-bearing signal instead: the completion hook writes
    // the task status back BEFORE it sends anything, so a report in the chat means the write-back
    // already happened and (with the heal budget spent) no new attempt is coming.
    let report = wait_for_report(&h.root, &chat_id).await;
    let task_view = get_json(&http, format!("{}/api/v1/tasks/{task_id}", h.base)).await;

    let runs = task_view["runs"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        runs.len(),
        2,
        "exactly ONE self-heal retry (the budget is 1), never a chain: {task_view}"
    );
    // ---- P2.3: the retry moved to a DIFFERENT agent -------------------------
    let agents: Vec<&str> = runs
        .iter()
        .map(|r| r["params"]["agent"].as_str().unwrap_or("<none>"))
        .collect();
    assert_ne!(
        agents[0], agents[1],
        "the self-heal must try a DIFFERENT agent when the registry has one: {task_view}"
    );
    let heal = runs
        .iter()
        .find(|r| r["params"]["self_heal_of"].as_str().is_some())
        .expect("the retry must record WHICH run it heals (params.self_heal_of)");
    assert_eq!(
        heal["params"]["self_heal_of"].as_str(),
        Some(first_run_id.as_str()),
        "the heal names the attempt it heals"
    );
    assert_eq!(
        heal["origin_chat_id"].as_str(),
        Some(chat_id.as_str()),
        "the heal retry answers to the SAME conversation"
    );
    // ---- P0: the board must not show 进行中 about a dead cluster -------------
    assert_eq!(
        task_view["task"]["status"], "pending",
        "a failed cluster returns the task to pending (re-dispatchable), NOT in_progress: {task_view}"
    );
    for run in &runs {
        assert!(
            run["error"].as_str().is_some_and(|e| !e.is_empty()),
            "every failed attempt must say why: {run}"
        );
    }

    // ---- P2.3's trace: the self-heal is visible in the run's own event stream
    let heal_id: ruagent_core::RunId = heal["id"].as_str().unwrap().parse().unwrap();
    let heal_events = ruagent_store::read_transcript(ruagent_store::transcript_path(
        h.root.join("data").join("transcripts"),
        &heal_id,
    ))
    .unwrap_or_default();
    let heal_trace = heal_events
        .iter()
        .find_map(|line| match &line.event {
            ruagent_core::RunEvent::Routed { decision } => decision.rationale.clone(),
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        heal_trace.contains("self-heal"),
        "a self-heal must leave a trace in the run's event stream, not only in a log: {heal_trace:?}"
    );

    // ---- THE ACCEPTANCE: the chat really has a NEW platform event -----------
    println!("READING the platform event the chat received:\n{report}\n");
    assert!(
        report.contains("把 report.md 的结论整理成一页纸"),
        "the report names the task: {report}"
    );
    assert!(
        report.contains("boom-a") && report.contains("boom-b"),
        "the report names the agents by NAME (both attempts): {report}"
    );
    assert!(
        report.contains("我试了 2 次"),
        "the report says how many attempts were made, including the self-heal: {report}"
    );
    assert!(
        report.contains("自愈"),
        "the report says the automatic retry happened: {report}"
    );
    // The marker that keeps this out of the user's own words (P2.2), and what FOLLOWS the last
    // marker must be empty — which is exactly how the daemon's reader (`sessions::user_text`)
    // splits "the user's own words" out of a prompt. The claim is asserted here rather than by
    // widening that function's visibility: the marker's MEANING is the contract, and a test that
    // could only see it through a `pub` shim would be testing the shim.
    let sentinel = ruagent_daemon::chat::USER_TEXT_SENTINEL;
    let at = report
        .rfind(sentinel)
        .unwrap_or_else(|| panic!("the report must carry the platform marker: {report:?}"));
    let after = &report[at + sentinel.len()..];
    assert!(
        after.trim().is_empty(),
        "nothing may follow the platform marker, or the platform's words become the user's: \
         {after:?}"
    );

    // And the task row the user's board reads agrees with the report.
    let tasks = get_json(&http, format!("{}/api/v1/tasks", h.base)).await;
    assert!(
        tasks["tasks"].as_array().is_some_and(|rows| rows
            .iter()
            .any(|t| t["id"] == task_id.as_str() && t["status"] == "pending")),
        "the task list must show the failure, not 进行中: {tasks}"
    );
}

/// Bounded wait for the chat's FIRST platform event, and the reason every "did the cluster
/// report?" question waits on THIS instead of on a list of runs.
///
/// The completion hook writes the task status back BEFORE it sends the report (one function, in
/// that order), so "a report exists" implies "the write-back already happened". A poll over
/// `GET /tasks/{id}` does NOT imply that: in the window between an attempt ending and a self-heal
/// retry being inserted, every run that exists IS terminal while the cluster has not finished
/// deciding — measured 1/10 red on exactly that window (`runs.len() >= 2 && all ended` was true,
/// the task was still `in_progress`, and the report had not been composed yet).
///
/// The alternative the design forbids — loosening the ASSERTIONS — is not used here: what changed
/// is WHAT WE WAIT FOR, and both acceptance criteria (`mid_flight == 0`, exactly one report) are
/// untouched.
async fn wait_for_report(root: &std::path::Path, chat_id: &str) -> String {
    const TURNS: usize = 300;
    for _ in 0..TURNS {
        let found = chat_messages(root, chat_id)
            .into_iter()
            .find_map(|event| match event {
                ruagent_core::RunEvent::UserMessage { text } if text.contains("平台事件") => {
                    Some(text)
                }
                _ => None,
            });
        if let Some(text) = found {
            return text;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!(
        "THE CHAT RECEIVED NO PLATFORM EVENT ({TURNS} × 100 ms). Messages present: {:#?}",
        chat_messages(root, chat_id)
    );
}

fn ended(run: &serde_json::Value) -> bool {
    // The terminal set, spelled out: WaitingPermission/Queued/Spawning/Running are NOT ended (the
    // whole point of P0's exhaustive mapping).
    matches!(
        run["status"].as_str(),
        Some("completed") | Some("failed") | Some("cancelled") | Some("interrupted")
    )
}

/// B1's NEGATIVE CONTROL (acceptance script §B1 反例): the same failing dispatch WITHOUT an
/// `origin_chat_id` must add NOTHING to any chat — a panel/script dispatch is somebody already
/// looking at the answer.
///
/// P0 still applies (the board must not lie about a run nobody attributed to a conversation), and
/// that half is asserted too, so this test cannot pass by the run never happening.
#[tokio::test]
async fn a_dispatch_without_an_origin_reports_nothing_but_still_fixes_the_board() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = boot("no-origin", &mock).await;
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
        serde_json::json!({ "title": "面板派的活", "intent": "nobody is waiting to be woken" }),
        200,
    )
    .await;
    let task_id = task["id"].as_str().expect("task id").to_string();
    // NO origin_chat_id: exactly what the panel sends.
    post_json(
        &http,
        format!("{}/api/v1/tasks/{task_id}/runs", h.base),
        serde_json::json!({ "agent": "boom-a" }),
        200,
    )
    .await;

    // WAIT FOR THE WRITE-BACK ITSELF, not for a run row (measured: red once under full-suite load).
    //
    // The order inside the daemon is: persist the run's terminal status (`update_run`) → then the
    // completion hook writes the task status. So a reading taken when the RUN says `failed` can
    // legitimately still see `in_progress` — the hook has not run yet. With no origin chat there is
    // no report to wait for (that is the whole point of this test), so the write-back itself is the
    // signal: the same assertion, WAITED FOR instead of raced. A write-back that never happens
    // still fails, loudly, as this poll's timeout.
    poll_task(
        &http,
        format!("{}/api/v1/tasks/{task_id}", h.base),
        |v| {
            v["runs"]
                .as_array()
                .is_some_and(|runs| runs.iter().all(ended))
                && v["task"]["status"] != "in_progress"
        },
        "the run's terminal state to be written back onto the TASK (the run is already failed and \
         the task is still in_progress)",
    )
    .await;
    let task_view = get_json(&http, format!("{}/api/v1/tasks/{task_id}", h.base)).await;
    assert_eq!(
        task_view["runs"][0]["origin_chat_id"],
        serde_json::Value::Null,
        "an unattributed dispatch must stay unattributed: {task_view}"
    );
    // P0 is not gated on attribution.
    assert_eq!(
        task_view["task"]["status"], "pending",
        "the board must still be told the run failed: {task_view}"
    );

    // THE NEGATIVE CONTROL: give any delivery a moment to happen, then require silence.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let leaked: Vec<String> = chat_messages(&h.root, &chat_id)
        .into_iter()
        .filter_map(|e| match e {
            ruagent_core::RunEvent::UserMessage { text } if text.contains("平台事件") => {
                Some(text)
            }
            _ => None,
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "a dispatch with no origin_chat_id must wake NOBODY, got {leaked:?}"
    );
}

/// B2 (acceptance script §B2, the one that catches "wake once per run"): two attempts of one
/// cluster, one FAST and one SLOW. The fast one's terminal state is an INTERMEDIATE result, so the
/// chat must still be empty; only when the slow one ends does exactly ONE report appear.
///
/// This is the property that makes a timer unnecessary — the aggregation is the whole criterion.
#[tokio::test]
async fn an_intermediate_terminal_attempt_does_not_report_until_the_cluster_ends() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = boot("cluster", &mock).await;
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
        serde_json::json!({ "title": "两个尝试的同一件事", "intent": "one fast, one slow" }),
        200,
    )
    .await;
    let task_id = task["id"].as_str().expect("task id").to_string();

    // The SLOW attempt first, so it is certainly live when the fast one ends.
    post_json(
        &http,
        format!("{}/api/v1/tasks/{task_id}/runs", h.base),
        serde_json::json!({ "agent": "slow", "origin_chat_id": chat_id }),
        200,
    )
    .await;
    // The FAST one (crashes in ~150 ms).
    post_json(
        &http,
        format!("{}/api/v1/tasks/{task_id}/runs", h.base),
        serde_json::json!({ "agent": "boom-a", "origin_chat_id": chat_id }),
        200,
    )
    .await;

    // (a) the fast attempt ends while the slow one is still running.
    poll_task(
        &http,
        format!("{}/api/v1/tasks/{task_id}", h.base),
        |v| {
            v["runs"].as_array().is_some_and(|runs| {
                runs.iter().any(|r| r["status"] == "failed")
                    && runs.iter().any(|r| !ended(r) && r["status"] != "queued")
            })
        },
        "the fast attempt to fail while the slow one is still running",
    )
    .await;
    let mid_flight = chat_messages(&h.root, &chat_id)
        .into_iter()
        .filter(|e| matches!(e, ruagent_core::RunEvent::UserMessage { text } if text.contains("平台事件")))
        .count();
    assert_eq!(
        mid_flight, 0,
        "an intermediate terminal attempt must NOT report: the cluster still has a live run"
    );

    // (b) THE CLUSTER'S DECISION, waited for by the SIGNAL rather than by a snapshot.
    //
    // WAIT FOR THE REPORT, NOT FOR "every run that exists is terminal" (measured: 1/10 red before
    // this). The two differ in the window between the slow attempt ending and its self-heal retry
    // being inserted: at that instant every existing run IS terminal, the task is still
    // `in_progress`, and the hook has already decided to spend its one heal — a reading taken
    // there describes a cluster that has not finished deciding. The report is the signal because
    // the SAME hook writes the task status back BEFORE sending it, and because after a report the
    // heal budget is spent, so no further attempt can appear. Both acceptance criteria
    // (`mid_flight == 0`, exactly one report) are unchanged — what changed is what we WAIT for.
    let _report = wait_for_report(&h.root, &chat_id).await;
    let finished = get_json(&http, format!("{}/api/v1/tasks/{task_id}", h.base)).await;
    let reports = chat_messages(&h.root, &chat_id)
        .into_iter()
        .filter(|e| {
            matches!(e, ruagent_core::RunEvent::UserMessage { text } if text.contains("平台事件"))
        })
        .count();
    assert_eq!(
        reports, 1,
        "the cluster must report EXACTLY ONCE, never per run: {finished}"
    );
    assert!(
        finished["runs"]
            .as_array()
            .is_some_and(|runs| runs.iter().all(ended)),
        "no attempt may still be live once the cluster has reported: {finished}"
    );
    assert_ne!(
        finished["task"]["status"], "in_progress",
        "the board must not keep saying 进行中: {finished}"
    );
}

/// P2.4, the case that must never be silent: a report for a chat that is GONE (never existed, or
/// deleted) is recorded and swallowed — no panic, no retry, and the run path is unaffected.
#[tokio::test]
async fn a_report_for_a_deleted_chat_is_swallowed_and_says_so() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = boot("gone", &mock).await;
    let chat_id = ruagent_core::RunId::generate().to_string();
    // No chat was created: this id names nothing at all.
    let outcome =
        ruagent_daemon::chat::notify_chat(&h.root, &chat_id, "【平台事件】测试".to_string()).await;
    assert!(
        outcome.is_ok(),
        "a report for a chat that does not exist is a recorded loss, not an error the run path \
         has to survive: {outcome:?}"
    );

    // A root with no daemon at all is the OTHER way delivery fails, and it must be reported.
    let nowhere = std::env::temp_dir().join("ruagent-supervise-nowhere-root");
    let err = ruagent_daemon::chat::notify_chat(&nowhere, &chat_id, "x".to_string()).await;
    assert!(
        err.is_err(),
        "with no live manager the caller must GET an error it can log, never a silent no-op"
    );

    // Nothing may have been written for a chat that does not exist: the swallow is a log line, not
    // a fabricated session.
    assert!(
        chat_messages(&h.root, &chat_id).is_empty(),
        "no transcript may be created for a chat that never existed"
    );
}
