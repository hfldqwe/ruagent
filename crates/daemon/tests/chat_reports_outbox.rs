//! P4 END TO END: a platform report survives a conversation that has no live session, and the
//! user gets it back when the conversation comes back.
//!
//! Spec: `docs/plans/2026-11-23-reports-never-lost.md` §1 (P4.1–P4.5), acceptance §5 rows 1–4.
//!
//! WHAT WAS BROKEN, in one sentence: `ChatManager::notify` could only deliver into a LIVE session,
//! so a report for a chat that was closed, idle-reaped, or gone because the daemon had restarted was
//! written to the log as `platform event dropped` and lost — while the work it reported on had
//! actually finished. Every test here therefore reads the CHAT'S OWN TRANSCRIPT (the file
//! `chat_events` replays and the panel renders), never "some function was called".
//!
//! The failing agent is `crates/mock-agent` (`--behavior crash`: a chunk, 150 ms, exit 1); the
//! conversation runs on `--behavior echo`. No API keys, no real harness.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

const NEEDS_MOCK: &str =
    "the mock ACP agent is missing: cargo build -p ruagent-mock-agent --bins (CI builds it)";

fn mock_bin() -> Option<String> {
    // The same resolution the other daemon integration tests use: the workspace build puts the
    // binary next to the test executable.
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

/// A private root, its config, and an open database — everything a daemon needs EXCEPT the
/// managers, so a test can write rows (the state a killed daemon leaves) before the daemon boots.
struct Prepared {
    root: std::path::PathBuf,
    cfg: DaemonConfig,
    db: Db,
    knowledge: ruagent_knowledge::Knowledge,
}

struct Harness {
    base: String,
    root: std::path::PathBuf,
    db: Db,
    mgr: Arc<RunManager>,
}

async fn prepare(tag: &str, mock: &str) -> Prepared {
    let root = std::env::temp_dir().join(format!(
        "ruagent-outbox-{tag}-{}-{}",
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
             description = \"the self-heal target\"\n\n\
             [agent.talker]\nharness = \"mock\"\ncommand = '{mock} --behavior echo'\n\
             description = \"the conversation the report goes into\"\n"
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
    Prepared {
        root,
        cfg,
        db,
        knowledge,
    }
}

/// Wire the daemon the way `lib.rs` wires it (run manager, chat manager, HTTP router) over a
/// prepared root.
async fn serve(p: Prepared) -> Harness {
    let Prepared {
        root,
        cfg,
        db,
        knowledge,
    } = p;
    let mut agents = cfg.agents.clone();
    for card in &mut agents {
        if let Some(id) = db.agent_id_by_name(&card.name).await.unwrap() {
            card.id = id;
        }
    }
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
        // An embedder: a report becomes a PROMPT, and a manager without one cannot deliver it.
        Some(knowledge.embedder()),
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    chats.set_ask_dropper({
        let mgr = Arc::clone(&mgr);
        Arc::new(move |id| mgr.drop_pending_for(id))
    });
    let app = ruagent_daemon::api::router(AppState {
        mgr: Arc::clone(&mgr),
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
        mgr,
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
    let resp = http.get(&url).send().await.unwrap();
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    assert_eq!(status, 200, "GET {url} -> {status}: {text}");
    serde_json::from_str(&text).unwrap()
}

/// Every message the chat has, as the panel/SSE reads it: the chat's own transcript lines.
fn chat_messages(root: &std::path::Path, chat_id: &str) -> Vec<ruagent_core::RunEvent> {
    let id: ruagent_core::RunId = chat_id.parse().expect("chat ids are run ids");
    let path = ruagent_store::transcript_path(root.join("data").join("transcripts"), &id);
    ruagent_store::read_transcript(&path)
        .unwrap_or_default()
        .into_iter()
        .map(|line| line.event)
        .collect()
}

/// The platform events this conversation holds, verbatim.
fn platform_events(root: &std::path::Path, chat_id: &str) -> Vec<String> {
    chat_messages(root, chat_id)
        .into_iter()
        .filter_map(|event| match event {
            ruagent_core::RunEvent::UserMessage { text } if text.contains("平台事件") => {
                Some(text)
            }
            _ => None,
        })
        .collect()
}

/// Wait for the conversation to hold a platform event, and return them all.
///
/// WHY A WAIT AND NOT A READ AFTER THE RESUME RETURNS: the transcript line is written by the chat's
/// own forwarder task, which subscribes to the session's event stream. The HTTP response is sent when
/// `send_prompt` has ACCEPTED the prompt, not when the session has replayed the event back — so one
/// read right after the response measures the scheduler, not the daemon (measured: it missed the
/// event when this test was first written). Bounded at 10 s, and it fails with the messages that ARE
/// present, so a delivery that never happens stays a loud failure.
async fn wait_for_platform_events(root: &std::path::Path, chat_id: &str) -> Vec<String> {
    for _ in 0..200 {
        let events = platform_events(root, chat_id);
        if !events.is_empty() {
            return events;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!(
        "NO PLATFORM EVENT ARRIVED IN 10 s. Messages present: {:#?}",
        chat_messages(root, chat_id)
    );
}

/// Create a conversation through the API and then CLOSE it, leaving exactly the shape acceptance
/// §5.1 names: a `chats` row that exists, and no live session.
async fn a_closed_conversation(http: &reqwest::Client, base: &str) -> String {
    let chat = post_json(
        http,
        format!("{base}/api/v1/chat"),
        serde_json::json!({ "agent": "talker" }),
        200,
    )
    .await;
    let chat_id = chat["id"].as_str().expect("chat id").to_string();
    let resp = http
        .delete(format!("{base}/api/v1/chat/{chat_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        204,
        "closing a live chat is 204 (the history row stays)"
    );
    chat_id
}

/// The reports a conversation is still owed, as the panel's read sees them.
async fn owed(http: &reqwest::Client, base: &str, chat_id: &str) -> serde_json::Value {
    get_json(
        http,
        format!("{base}/api/v1/chat/{chat_id}/reports?pending=1&limit=200"),
    )
    .await
}

/// The row's pending count on the conversation list (`GET /api/v1/chats`), which is what the rail's
/// badge shows.
async fn unread_for(http: &reqwest::Client, base: &str, chat_id: &str) -> serde_json::Value {
    let chats = get_json(http, format!("{base}/api/v1/chats?limit=200")).await;
    chats["chats"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == chat_id))
        .map(|row| row["pending_reports"].clone())
        .unwrap_or_else(|| panic!("chat {chat_id} must be in GET /api/v1/chats: {chats}"))
}

/// ACCEPTANCE §5.1 (the P4 main criterion) + §5.2's visibility half.
///
/// A conversation with NO live session gets a platform event: the event must be IN THE OUTBOX (not
/// dropped), the rail must show it as unread, and `POST /chat/{id}/resume` must put it in front of
/// the user with `delivered_at` set and `attempts >= 1`.
#[tokio::test]
async fn a_report_for_a_chat_with_no_session_is_stored_and_delivered_on_resume() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = serve(prepare("no-session", &mock).await).await;
    let http = reqwest::Client::new();
    let chat_id = a_closed_conversation(&http, &h.base).await;

    assert!(
        platform_events(&h.root, &chat_id).is_empty(),
        "the conversation must start with no platform event, or the assertion below is vacuous"
    );

    // THE EVENT A KILLED DAEMON OR A CLOSED CHAT USED TO LOSE.
    ruagent_daemon::chat::notify_chat(
        &h.root,
        &chat_id,
        "【平台事件】你派出去的活完成了：三份报告合成了一页纸".to_string(),
    )
    .await
    .expect("a report for a chat with no session is NOT an error any more: it is stored");

    // ---- it is IN THE OUTBOX (acceptance §5.1's first half) -----------------
    let pending = owed(&http, &h.base, &chat_id).await;
    let rows = pending["reports"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        rows.len(),
        1,
        "the report must be in the outbox, not dropped: {pending}"
    );
    assert_eq!(
        rows[0]["text"], "【平台事件】你派出去的活完成了：三份报告合成了一页纸",
        "the stored body is the report AS WRITTEN — no platform marker baked in"
    );
    assert!(
        !rows[0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains('\u{2063}'),
        "the sentinel must NOT be stored: it is applied when the prompt is built"
    );
    assert_eq!(rows[0]["delivered_at"], serde_json::Value::Null);
    assert_eq!(rows[0]["attempts"], 0, "nothing has been attempted yet");
    assert_eq!(pending["pending"], 1);

    // ---- visibility: the rail's number (P4.4) -------------------------------
    assert_eq!(
        unread_for(&http, &h.base, &chat_id).await,
        serde_json::json!(1),
        "GET /api/v1/chats must report the owed report on this conversation's row"
    );

    // ---- resume: THE MOMENT IT COMES BACK (acceptance §5.1's second half) ---
    let resumed = post_json(
        &http,
        format!("{}/api/v1/chat/{chat_id}/resume", h.base),
        serde_json::Value::Null,
        200,
    )
    .await;
    assert_eq!(
        resumed["pending_reports"],
        serde_json::json!(0),
        "a resume delivers the one report it owed, and says how many are left: {resumed}"
    );

    let events = wait_for_platform_events(&h.root, &chat_id).await;
    assert_eq!(
        events.len(),
        1,
        "the resume must have put the report into the conversation as a platform event, got \
         {events:?}"
    );
    assert!(
        events[0].contains("三份报告合成了一页纸"),
        "the delivered turn is the report itself: {:?}",
        events[0]
    );
    // The marker exists and nothing follows it — how the daemon's own reader (`sessions::user_text`)
    // distinguishes "the platform spoke" from "the user spoke".
    let sentinel = ruagent_daemon::chat::USER_TEXT_SENTINEL;
    let at = events[0]
        .rfind(sentinel)
        .unwrap_or_else(|| panic!("the delivered report must carry the platform marker"));
    assert!(
        events[0][at + sentinel.len()..].trim().is_empty(),
        "nothing may follow the platform marker, or the platform's words become the user's"
    );

    // ---- and the row now says so -------------------------------------------
    let after = get_json(
        &http,
        format!("{}/api/v1/chat/{chat_id}/reports?limit=200", h.base),
    )
    .await;
    let rows = after["reports"].as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 1, "the row survives delivery (it is a record)");
    assert!(
        rows[0]["delivered_at"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "delivered_at is the ONE delivered criterion and it must be set: {after}"
    );
    assert!(
        rows[0]["attempts"].as_i64().unwrap_or(0) >= 1,
        "the delivery was an attempt and must be counted: {after}"
    );
    assert_eq!(
        owed(&http, &h.base, &chat_id).await["pending"],
        0,
        "nothing is owed any more"
    );
    assert_eq!(
        unread_for(&http, &h.base, &chat_id).await,
        serde_json::json!(0),
        "the rail's number must read 0 (a reading), not 'unknown'"
    );
}

/// ACCEPTANCE §5.2: the per-chat cap. N + 3 reports into one conversation leave N, and the OLDEST
/// are the ones that go (a silent drop is the failure mode this table exists to remove, so the
/// store hands the dropped ids back and the daemon WARNs them by name).
///
/// The WARN itself is asserted at the layer that can read it: `the_report_outbox_...` in
/// `crates/store/src/lib.rs` asserts the returned ids ARE the oldest pending rows.
#[tokio::test]
async fn the_outbox_keeps_at_most_the_cap_and_drops_the_oldest_reports() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = serve(prepare("cap", &mock).await).await;
    let http = reqwest::Client::new();
    let chat_id = a_closed_conversation(&http, &h.base).await;

    let cap = ruagent_store::CHAT_REPORTS_CAP;
    for i in 0..cap + 3 {
        ruagent_daemon::chat::notify_chat(&h.root, &chat_id, format!("【平台事件】report {i}"))
            .await
            .expect(
                "every report is accepted; the cap drops the oldest, it does not refuse the new",
            );
    }

    let pending = owed(&http, &h.base, &chat_id).await;
    let texts: Vec<String> = pending["reports"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| r["text"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        texts.len() as i64,
        cap,
        "the outbox holds the CAP, not one more: {texts:?}"
    );
    assert_eq!(
        texts.first().map(String::as_str),
        Some("【平台事件】report 3"),
        "the three OLDEST reports were the ones dropped: {texts:?}"
    );
    let newest = format!("【平台事件】report {}", cap + 2);
    assert_eq!(
        texts.last().map(String::as_str),
        Some(newest.as_str()),
        "the newest report is always kept: {texts:?}"
    );
    assert_eq!(
        unread_for(&http, &h.base, &chat_id).await,
        serde_json::json!(cap),
        "the rail shows the capped number"
    );
}

/// ACCEPTANCE §5.3: deleting a conversation takes its outbox rows with it (the `DELETE /tasks/{id}`
/// shape — the row goes, and the things that answer to it go).
#[tokio::test]
async fn deleting_a_chat_removes_its_outbox_rows() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = serve(prepare("delete", &mock).await).await;
    let http = reqwest::Client::new();
    let chat_id = a_closed_conversation(&http, &h.base).await;

    for i in 0..2 {
        ruagent_daemon::chat::notify_chat(&h.root, &chat_id, format!("【平台事件】owed {i}"))
            .await
            .expect("stored");
    }
    assert_eq!(owed(&http, &h.base, &chat_id).await["pending"], 2);

    let resp = http
        .delete(format!("{}/api/v1/chats/{chat_id}", h.base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 204, "the history row is removed");

    let after = get_json(
        &http,
        format!("{}/api/v1/chat/{chat_id}/reports?limit=200", h.base),
    )
    .await;
    assert_eq!(
        after["reports"].as_array().map(|r| r.len()),
        Some(0),
        "a deleted conversation's reports must not outlive it: {after}"
    );
    assert_eq!(after["pending"], 0);
    let rows = get_json(&http, format!("{}/api/v1/chats?limit=200", h.base)).await;
    assert!(
        !rows["chats"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|r| r["id"] == chat_id.as_str())),
        "the conversation is gone from the list too: {rows}"
    );
}

/// ACCEPTANCE §5.4 (P4.5): RESTART → ONE SELF-HEAL → THE REPORT IS REALLY THERE WHEN THE
/// CONVERSATION COMES BACK.
///
/// The restart is represented by the DATABASE STATE a killed daemon leaves — a task, and a run that
/// says `running` with the `origin_chat_id` of the conversation that dispatched it — and then by the
/// boot's OWN recovery halves (`interrupt_live_runs` → `finalize_interrupted_clusters`), which is
/// the pair `lib.rs` calls at boot in that order. The self-heal that follows is REAL (a second mock
/// agent process is spawned), and so is the resume (HTTP).
///
/// WHY THIS TEST IS THE ONE THAT AUTHORIZES `allow_self_heal = true` (spec §P4.5): before P4, the
/// boot sweep had to pass `false`, because the heal it would start spent a round of tokens and then
/// its report was dropped (`WARN platform event dropped`) — paying and not being told. This asserts
/// the three links of the repaired chain at once: the heal HAPPENS (two runs), the report is PARKED
/// (outbox, because the chat has no session yet), and it ARRIVES on resume.
#[tokio::test]
async fn a_restart_heals_the_cluster_and_the_report_arrives_when_the_conversation_does() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let p = prepare("restart", &mock).await;
    let db = p.db.clone();

    // ---- the state a killed daemon leaves -----------------------------------
    let chat_id = ruagent_core::RunId::generate().to_string();
    {
        let id = chat_id.clone();
        db.call_flat(move |conn| {
            conn.execute(
                "INSERT INTO chats (id, agent, runtime, model, title, created_at, updated_at, cwd)
                 VALUES (?1, 'talker', 'mock', NULL, 'a conversation that was open', 1, 1, NULL)",
                [&id],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    let task = ruagent_core::Task::new(
        "把三份报告合成一页纸",
        "summarise three reports into one page",
        ruagent_core::TaskCreator::Human,
    );
    db.insert_task(&task).await.unwrap();
    let agent_id = db
        .agent_id_by_name("boom-a")
        .await
        .unwrap()
        .expect("boom-a is registered");
    let mut dead = ruagent_core::Run::new(task.id, ruagent_core::RunParams::for_agent(agent_id));
    dead.origin_chat_id = Some(chat_id.clone());
    dead.status = ruagent_core::RunStatus::Running;
    db.insert_run(&dead).await.unwrap();

    // ---- the daemon comes up over that root --------------------------------
    let h = serve(p).await;
    let http = reqwest::Client::new();

    // THE BOOT'S OWN RECOVERY, in the boot's own order.
    let interrupted = ruagent_daemon::interrupt_live_runs(&h.db)
        .await
        .expect("the sweep");
    assert_eq!(
        interrupted.len(),
        1,
        "one live run was interrupted by the restart"
    );
    ruagent_daemon::finalize_interrupted_clusters(&h.mgr, &interrupted).await;

    // ---- the heal really happens (2 runs), and the report is PARKED ---------
    let mut runs = Vec::new();
    for _ in 0..300 {
        let view = get_json(&http, format!("{}/api/v1/tasks/{}", h.base, task.id)).await;
        runs = view["runs"].as_array().cloned().unwrap_or_default();
        let pending = owed(&http, &h.base, &chat_id).await;
        if runs.len() == 2
            && runs.iter().all(|r| {
                matches!(
                    r["status"].as_str(),
                    Some("completed") | Some("failed") | Some("cancelled") | Some("interrupted")
                )
            })
            && pending["pending"] == 1
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        runs.len(),
        2,
        "the boot sweep must have started the cluster's ONE self-heal (this is the line P4.5 \
         changes: with `allow_self_heal = false` there is exactly one run). runs={runs:?}"
    );
    assert!(
        runs.iter()
            .any(|r| r["params"]["self_heal_of"].as_str().is_some()),
        "the second run must record that it heals the first: {runs:?}"
    );
    let pending = owed(&http, &h.base, &chat_id).await;
    let rows = pending["reports"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        rows.len(),
        1,
        "the healed cluster's report must be PARKED in the outbox (the chat has no live session \
         after a restart, which is exactly the case that used to drop it): {pending}"
    );
    assert!(
        rows[0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("把三份报告合成一页纸"),
        "the parked report names the task: {pending}"
    );
    assert!(
        platform_events(&h.root, &chat_id).is_empty(),
        "nothing can have been delivered: the conversation was never resumed"
    );

    // ---- THE ACCEPTANCE: the conversation comes back, the report is there --
    post_json(
        &http,
        format!("{}/api/v1/chat/{chat_id}/resume", h.base),
        serde_json::Value::Null,
        200,
    )
    .await;
    let events = wait_for_platform_events(&h.root, &chat_id).await;
    assert_eq!(
        events.len(),
        1,
        "after the restart the user must get exactly the report the platform owed them, got \
         {events:?}"
    );
    assert!(
        events[0].contains("把三份报告合成一页纸"),
        "the sentence that arrives is the report: {:?}",
        events[0]
    );
    assert!(
        events[0].contains("自愈") || events[0].contains("重试"),
        "the report must say the automatic retry happened (that is what the token spend bought): \
         {:?}",
        events[0]
    );
    assert_eq!(
        owed(&http, &h.base, &chat_id).await["pending"],
        0,
        "the debt is settled"
    );
}

/// THE LOOP THAT MUST NOT EXIST (independent verification, 2026-11-25).
///
/// The first version ALSO replayed the outbox from `drain_one_notification`, i.e. when a turn ENDED.
/// Every delivery is itself a turn, so each delivery triggered the next one: with three parked
/// reports, `resume` delivered one and the conversation then drained itself to `pending=0` with
/// nobody watching — up to the 50-row cap, i.e. up to 50 paid model turns nobody asked for.
///
/// What this test pins: a resume delivers ONE, and a later ordinary turn does NOT touch the rest.
/// Replaying them belongs to the person opening the conversation again, one report per resume.
#[tokio::test]
async fn a_resume_delivers_one_report_and_a_later_turn_does_not_drain_the_rest() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = serve(prepare("no-loop", &mock).await).await;
    let http = reqwest::Client::new();
    let chat_id = a_closed_conversation(&http, &h.base).await;

    // Three reports parked while the conversation has no session — the shape a restart leaves.
    for i in 1..=3 {
        ruagent_daemon::chat::notify_chat(&h.root, &chat_id, format!("【平台事件】第 {i} 条"))
            .await
            .expect("a report for a chat with no session is stored, not an error");
    }
    assert_eq!(
        owed(&http, &h.base, &chat_id).await["pending"],
        3,
        "all three must be parked"
    );

    // (1) A resume delivers exactly one and says two remain.
    let resumed = post_json(
        &http,
        format!("{}/api/v1/chat/{chat_id}/resume", h.base),
        serde_json::Value::Null,
        200,
    )
    .await;
    assert_eq!(
        resumed["pending_reports"],
        serde_json::json!(2),
        "a resume pays for ONE report, never a batch: {resumed}"
    );
    let events = wait_for_platform_events(&h.root, &chat_id).await;
    assert_eq!(events.len(), 1, "one report arrives: {events:?}");
    assert_eq!(
        owed(&http, &h.base, &chat_id).await["pending"],
        2,
        "two are still owed — and they stay owed"
    );

    // (2) THE REGRESSION GUARD: an ordinary turn goes by. When it ends the in-memory queue is empty,
    // and the outbox must NOT be drained into it. Wait for that turn to actually finish (its reply
    // is in the transcript), then read the outbox again.
    post_json(
        &http,
        format!("{}/api/v1/chat/{chat_id}/messages", h.base),
        serde_json::json!({ "text": "还在吗" }),
        202,
    )
    .await;
    for _ in 0..200 {
        let line = chat_messages(&h.root, &chat_id);
        if line
            .iter()
            .any(|e| matches!(e, ruagent_core::RunEvent::AgentMessageChunk { .. }))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    // The turn is over; give a possible (wrong) drain a moment to happen, so its absence is a
    // reading rather than a race.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let events_after = platform_events(&h.root, &chat_id);
    assert_eq!(
        events_after.len(),
        1,
        "a turn ending must NOT replay the outbox: that is the self-draining loop, and it spends one \
         paid turn per parked report with nobody watching. Got {events_after:?}"
    );
    assert_eq!(
        owed(&http, &h.base, &chat_id).await["pending"],
        2,
        "the two owed reports must still be owed after an ordinary turn"
    );

    // (3) And the NEXT time the person actually reopens the conversation is what settles the next
    // one — the intended pace. A live chat cannot be "resumed" (the daemon answers 400
    // "chat is live — attach instead"), so closing it first is part of the real shape: you close
    // the window, later you open it again.
    let closed = http
        .delete(format!("{}/api/v1/chat/{chat_id}", h.base))
        .send()
        .await
        .unwrap();
    assert_eq!(closed.status().as_u16(), 204, "closing a live chat is 204");
    post_json(
        &http,
        format!("{}/api/v1/chat/{chat_id}/resume", h.base),
        serde_json::Value::Null,
        200,
    )
    .await;
    for _ in 0..200 {
        if platform_events(&h.root, &chat_id).len() == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        owed(&http, &h.base, &chat_id).await["pending"],
        1,
        "reopening the conversation settles exactly one more report"
    );
}
