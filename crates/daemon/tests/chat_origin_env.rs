//! P6 (`docs/plans/2026-11-23-reports-never-lost.md` §3) ON REAL PROCESSES: the daemon puts the
//! conversation a run belongs to into that run's AGENT PROCESS environment, and leaves it out when
//! the run has no conversation.
//!
//! WHY THE MOCK AGENT AND NOT A UNIT TEST. The claim is "the spawned process can read it", and only
//! a spawned process can say that. `crates/mock-agent --behavior envdump` replies with three
//! readings — the value it sees, its own pid, and what a CHILD of it sees — so this test reads them
//! off the run's own `result` after the real daemon has spawned it through the real ACP client.
//! Nothing here asserts on a function being called.
//!
//! WHAT THIS FILE DOES **NOT** PROVE, said plainly: the hop from the agent to the platform MCP
//! server is not measured here (that needs the agent to spawn the server and speak MCP to it). It is
//! OS inheritance — the same mechanism `RUAGENT_URL` already depends on in production — and the
//! child reading below is the evidence that inheritance works from this process at least one level
//! down. The reader half (`task_dispatch` taking the value from its environment) is measured in
//! `crates/mcp/tests/dispatch_origin.rs`.

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
    /// The private root, kept because the daemon's own logs and the runs' transcripts live here: a
    /// failing reading is diagnosable after the fact, which is why this file does not delete it.
    #[allow(dead_code)]
    root: std::path::PathBuf,
}

/// A daemon over a PRIVATE root with two agents: `envdump` (reports its environment) and `talker`
/// (an ordinary answerer, used to create a real conversation to attribute work to).
async fn boot(tag: &str, mock: &str) -> Harness {
    let root = std::env::temp_dir().join(format!(
        "ruagent-origin-env-{tag}-{}-{}",
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
            "[agent.envdump]\nharness = \"mock\"\ncommand = '{mock} --behavior envdump'\n\
             description = \"reports the environment of the process the daemon spawned\"\n\n\
             [agent.talker]\nharness = \"mock\"\ncommand = '{mock} --behavior echo'\n\
             description = \"a conversation to attribute work to\"\n"
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

/// Dispatch one run and wait for it to END, returning its final row.
///
/// Waiting on the RUN's own terminal state (not on a sleep) keeps the reading about the agent
/// process that was spawned; a run that never ends still fails, as this poll's timeout.
async fn run_to_end(
    http: &reqwest::Client,
    base: &str,
    agent: &str,
    origin: Option<&str>,
    prompt: &str,
) -> serde_json::Value {
    let task = post_json(
        http,
        format!("{base}/api/v1/tasks"),
        serde_json::json!({ "title": format!("P6 origin env: {prompt}"), "intent": prompt }),
        200,
    )
    .await;
    let task_id = task["id"].as_str().expect("task id").to_string();
    let mut body = serde_json::json!({ "agent": agent, "prompt": prompt });
    if let Some(origin) = origin {
        body["origin_chat_id"] = serde_json::json!(origin);
    }
    let started = post_json(
        http,
        format!("{base}/api/v1/tasks/{task_id}/runs"),
        body,
        200,
    )
    .await;
    let run_id = started["id"].as_str().expect("run id").to_string();
    for _ in 0..300 {
        let run = get_json(http, format!("{base}/api/v1/runs/{run_id}")).await;
        if matches!(
            run["status"].as_str(),
            Some("completed") | Some("failed") | Some("cancelled") | Some("interrupted")
        ) {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the envdump run never reached a terminal state");
}

/// The reading the instrument prints, e.g. `env RUAGENT_CHAT_ID=<abc> pid=42 child=<abc>`.
fn field(line: &str, key: &str) -> String {
    line.split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")).map(str::to_string))
        .unwrap_or_else(|| panic!("the env dump has no `{key}=` token: {line}"))
}

#[tokio::test]
async fn a_chat_dispatched_run_carries_its_conversation_and_an_unattributed_one_carries_nothing() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = boot("p6", &mock).await;
    let http = reqwest::Client::new();

    // A real conversation, because the id must be one the daemon can actually report into.
    let chat = post_json(
        &http,
        format!("{}/api/v1/chat", h.base),
        serde_json::json!({ "agent": "talker" }),
        200,
    )
    .await;
    let chat_id = chat["id"].as_str().expect("chat id").to_string();

    // (1) A CHAT-DISPATCHED RUN: the variable is there, its value is the conversation, and a CHILD
    // of the agent inherits it — which is the mechanism the platform MCP server relies on.
    let attributed = run_to_end(&http, &h.base, "envdump", Some(&chat_id), "attributed").await;
    let text = attributed["result"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    println!("READING P6 attributed run result:\n{text}");
    assert_eq!(attributed["status"], "completed", "{attributed}");
    assert_eq!(
        field(&text, "RUAGENT_CHAT_ID"),
        format!("<{chat_id}>"),
        "the agent process of a chat-dispatched run must carry the conversation: {text}"
    );
    assert_eq!(
        field(&text, "child"),
        format!("<{chat_id}>"),
        "a child of the agent inherits it — that is how the MCP server learns it: {text}"
    );

    // (2) A RUN WITH NO CONVERSATION (the panel, the CLI, a script): the variable is ABSENT — not
    // empty. `<>` would make "nobody is waiting" indistinguishable from "a chat with an empty id".
    let unattributed = run_to_end(&http, &h.base, "envdump", None, "unattributed").await;
    let text = unattributed["result"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    println!("READING P6 unattributed run result:\n{text}");
    assert_eq!(unattributed["status"], "completed", "{unattributed}");
    assert_eq!(
        field(&text, "RUAGENT_CHAT_ID"),
        "<unset>",
        "a run with no origin must NOT get the variable at all: {text}"
    );
    assert_eq!(
        field(&text, "child"),
        "<unset>",
        "and nothing to inherit: {text}"
    );

    // (3) ONE RUN, ONE PROCESS — the assumption P6 rests on ("if one agent process served several
    // conversations, one variable would not be enough"). Two dispatches produced two pids, so a
    // process serves exactly one run here. Reads the SAME two results as above.
    let attributed_pid = field(attributed["result"].as_str().unwrap_or_default(), "pid");
    let unattributed_pid = field(unattributed["result"].as_str().unwrap_or_default(), "pid");
    println!("READING P6 process ids: attributed={attributed_pid} unattributed={unattributed_pid}");
    assert_ne!(
        attributed_pid, unattributed_pid,
        "two runs must be two agent processes (the daemon spawns one per run: crates/acp/src/lib.rs \
         'connection-per-run'), otherwise one RUAGENT_CHAT_ID per process would be ambiguous"
    );
}

/// P6, the half that actually makes the feature work for a person: **the agent the user is
/// talking to** must know which conversation it is in.
///
/// WHY THIS IS A SEPARATE TEST FROM THE ONE ABOVE. The run-side injection above covers work the
/// daemon dispatches. But the entity that CALLS `task_dispatch` is the conversation's own agent
/// process — a DIFFERENT process, spawned by `ChatManager`, not by `RunManager`. Without this,
/// every dispatch the chat agent makes has no origin, so its report goes to nobody: the supervisor
/// looks wired up and answers into the void. Measured, not assumed — the value is read out of the
/// conversation's own transcript, which is the only place a spawned process can put it.
#[tokio::test]
async fn the_conversations_own_agent_process_carries_its_conversation() {
    let mock = mock_bin().expect(NEEDS_MOCK);
    let h = boot("p6chat", &mock).await;
    let http = reqwest::Client::new();

    // The conversation is started on `envdump`, so the first reply will be the process reporting
    // its own environment.
    let chat = post_json(
        &http,
        format!("{}/api/v1/chat", h.base),
        serde_json::json!({ "agent": "envdump" }),
        200,
    )
    .await;
    let chat_id = chat["id"].as_str().expect("chat id").to_string();

    // 202: the turn is QUEUED, not answered inline (the same asynchrony the whole supervisor work
    // rests on), so the reply has to be waited for below.
    post_json(
        &http,
        format!("{}/api/v1/chat/{chat_id}/messages", h.base),
        serde_json::json!({ "text": "report your environment" }),
        202,
    )
    .await;

    // Read the conversation's transcript until the agent has spoken. The envdump reply is plain
    // text, so the transcript is where the reading has to come from — asserting on a function
    // being called would prove nothing about a spawned process's environment.
    let id: ruagent_core::RunId = chat_id.parse().expect("chat ids are run ids");
    let path = ruagent_store::transcript_path(h.root.join("data").join("transcripts"), &id);
    let mut raw = String::new();
    for _ in 0..200 {
        raw = std::fs::read_to_string(&path).unwrap_or_default();
        if raw.contains("RUAGENT_CHAT_ID") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    println!("READING P6 conversation transcript ({chat_id}):\n{raw}");

    assert!(
        raw.contains(&format!("RUAGENT_CHAT_ID=<{chat_id}>")),
        "the conversation's own agent process must carry its chat id (this is what makes its \
         task_dispatch attributable): {raw}"
    );
    assert!(
        raw.contains(&format!("child=<{chat_id}>")),
        "and its children, which is how the platform MCP server learns it: {raw}"
    );
}
