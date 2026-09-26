//! End-to-end: what an AGENT actually receives (t292).
//!
//! WHY THIS FILE EXISTS: t260 put knowledge and wiki hits into the injection and
//! proved it with a temporary probe. After the probe was deleted, the only
//! remaining guard was a unit test on the renderer (crates/memory/src/inject.rs)
//! plus one e2e that asserted the MEMORY block alone -- so the user-visible half
//! ("the knowledge base now reaches the agent") had nothing watching it.
//!
//! WHAT IT DRIVES: a real in-process daemon over a real TCP socket, a real
//! RunManager, a real SQLite + knowledge base in a throwaway root, and the real
//! mock-agent binary over real ACP. BOTH producers are covered -- the run path
//! (t292) and the chat path (t302), which had no test touching its injection at
//! all until then, even though t259 called it the MAIN path.
//!
//! THREE LEVELS, in the order they can fail: the render proves the CONSTRUCTION,
//! the context_injected event proves it was SENT, and the mock agent's echoed
//! prompt proves it was RECEIVED. A test that stops at the first level stays
//! green while the send is truncated.
//!
//! The assertion is on the run's FIRST-CLASS
//! transcript event (context_injected), which is what the daemon itself
//! records -- never on /api/v1/recall, which would append a recall_log row and
//! make the test a producer of the data it inspects.
//!
//! BOTH DIRECTIONS ARE ASSERTED, and that is the point: a test that only checks
//! "the knowledge block is there when documents exist" cannot tell "the block is
//! correctly empty" from "the block was never wired up". So the same drive runs
//! twice -- with documents and without.
//!
//! PRECONDITION: the mock agent binary. cargo test --workspace builds it; a bare
//! cargo test -p ruagent-daemon may not, and then this test PRINTS a skip and
//! returns -- it never passes silently.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

/// The memory that must reach the agent in BOTH directions: without it the
/// "no knowledge block" case could pass by having no blocks at all.
const MEMORY: &str = "t292-memory-the-user-prefers-concise-answers";
/// A profile memory, so the two paths emit the same block SET and their block
/// headers can be compared directly (the drift check).
const PROFILE: &str = "t302-profile-the-user-is-a-rust-developer";
const PROMPT: &str = "where does the deploy script live";
const SOURCE_TEXT: &str = "t292-source-the-deploy-script-lives-in-scripts-release-sh";
const WIKI_TEXT: &str = "t292-wiki-generated-summary-of-the-release-process";

const SOURCE_DOC: &str = r#"# Deploy guide

t292-source-the-deploy-script-lives-in-scripts-release-sh and it runs from the repository root.
"#;
const WIKI_DOC: &str = r#"# Deploy notes

t292-wiki-generated-summary-of-the-release-process, to be verified against the deploy guide.
"#;

/// The mock binary, derived from THIS test binary's own location:
/// target/profile/deps/test.exe -> target/profile/ruagent-mock-agent[.exe].
/// CARGO_BIN_EXE_ruagent-mock-agent is only defined for the mock-agent package's
/// own tests, so an in-scope daemon test cannot use it.
fn mock_bin() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let profile_dir = exe.parent()?.parent()?;
    let name = if cfg!(windows) {
        "ruagent-mock-agent.exe"
    } else {
        "ruagent-mock-agent"
    };
    let path = profile_dir.join(name);
    path.is_file().then(|| {
        path.display()
            .to_string()
            .replace(std::path::MAIN_SEPARATOR, "/")
    })
}

/// A precondition this test cannot satisfy from the inside.
///
/// PRINTED, never silent -- and that matters more than it looks: an early return
/// is a PASS in cargo's summary, so a skipped run and a real run both print
/// "test result: ok". The printed line is the only difference, which is exactly
/// the ambiguity the repo's write-guard specs avoid the same way.
///
/// An unattended run can refuse the ambiguity instead of reading it:
/// RUAGENT_REQUIRE_MOCK=1 turns the missing binary into a FAILURE.
fn skip_missing_mock() {
    let msg = "no ruagent-mock-agent binary next to this test binary.                Build it (cargo build -p ruagent-mock-agent) or run cargo test --workspace.";
    if std::env::var("RUAGENT_REQUIRE_MOCK").is_ok() {
        panic!("RUAGENT_REQUIRE_MOCK is set and there is {msg}");
    }
    println!("SKIP t292: {msg} THIS TEST DID NOT RUN.");
}

struct TestDaemon {
    url: String,
    root: PathBuf,
    #[allow(dead_code)]
    db: Db,
}

async fn boot(tag: &str, with_knowledge: bool) -> Option<TestDaemon> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let bin = mock_bin()?;
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root =
        std::env::temp_dir().join(format!("ruagent-t292-{tag}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    // Forward slashes in the command: TOML basic strings treat a backslash as an
    // escape, and a Windows path would silently corrupt.
    std::fs::write(
        config_dir.join("agents.toml"),
        format!(
            r#"[agent.mock]
harness = "mock"
command = "{bin} --behavior echo"
description = "t292"

# t326: a run that DIES, so the retry path (and its crash snapshot in the
# ContextInjected render) can be driven end to end.
[agent.mockcrash]
harness = "mock"
command = "{bin} --behavior crash"
description = "t326"
"#
        ),
    )
    .unwrap();
    std::fs::write(
        config_dir.join("mcp.toml"),
        r#"[profile.default]
servers = []
"#,
    )
    .unwrap();
    std::fs::write(
        config_dir.join("policy.toml"),
        r#"[permissions]
default = "ask"
"#,
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
    let knowledge = Arc::new(
        ruagent_knowledge::Knowledge::open(&root, db.clone())
            .await
            .unwrap(),
    );
    // A memory in BOTH directions, so "no knowledge block" cannot pass by having
    // no context at all.
    let outcome = ruagent_memory::write_memory(
        &db,
        &ruagent_memory::MemoryWrite {
            store: ruagent_memory::MemoryStore::Observation,
            namespace: ruagent_memory::Namespace::parse("user").unwrap(),
            content: MEMORY.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ruagent_memory::WriteOutcome::Inserted(_)),
        "seeding the memory failed: {outcome:?}"
    );
    // A profile memory too: both paths read profile/user, so this makes their
    // block sets comparable in the wording test.
    let profile = ruagent_memory::write_memory(
        &db,
        &ruagent_memory::MemoryWrite {
            store: ruagent_memory::MemoryStore::Profile,
            namespace: ruagent_memory::Namespace::parse("user").unwrap(),
            content: PROFILE.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(profile, ruagent_memory::WriteOutcome::Inserted(_)),
        "seeding the profile failed: {profile:?}"
    );
    if with_knowledge {
        knowledge.save("t292-runbook", SOURCE_DOC).await.unwrap();
        knowledge.save("wiki/t292-notes", WIKI_DOC).await.unwrap();
    }

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
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));
    // The wiring the daemon does at boot (lib.rs). Without it the run path has no
    // knowledge handle and this test would be asserting the unwired shape.
    chats.set_knowledge(Arc::clone(&knowledge));
    mgr.set_knowledge(Arc::clone(&knowledge));

    let app = ruagent_daemon::api::router(AppState {
        mgr,
        config: Arc::new(cfg),
        knowledge: Arc::clone(&knowledge),
        chats,
        sessions: Arc::new(ruagent_daemon::sessions::SessionIndexer::new(
            db.clone(),
            std::env::temp_dir(),
        )),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Some(TestDaemon {
        url: format!("http://{addr}"),
        root,
        db,
    })
}

/// Drive one real run and return (the context_injected render, the agent's own
/// view of its prompt). The mock agent echoes its full prompt, so the second
/// value is the strongest available statement of "the agent RECEIVED this" --
/// the event alone only says the daemon emitted it.
async fn drive_run(d: &TestDaemon, prompt: &str) -> (String, String) {
    let http = reqwest::Client::new();
    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "t292", "intent": prompt }))
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
        .post(format!("{}/api/v1/tasks/{task_id}/runs", d.url))
        .json(&serde_json::json!({ "agent": "mock", "prompt": prompt }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = run["id"].as_str().unwrap().to_string();
    let path = d
        .root
        .join("data")
        .join("transcripts")
        .join(format!("run-{run_id}.jsonl"));

    // Poll the transcript for the first-class event, then for the agent's echo.
    // Never /api/v1/recall: that would write a recall_log row and make this test
    // a producer of the data it inspects.
    let mut render = None;
    let mut echoed = String::new();
    for _ in 0..600 {
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                match v["event"]["type"].as_str() {
                    Some("context_injected") => {
                        render = v["event"]["render"].as_str().map(|s| s.to_string());
                    }
                    Some("agent_message_chunk") => {
                        if let Some(t) = v["event"]["content"][0]["text"].as_str() {
                            echoed.push_str(t);
                        }
                    }
                    _ => {}
                }
            }
            if render.is_some() && !echoed.is_empty() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let render = render.unwrap_or_else(|| {
        panic!(
            "no context_injected event in {} (the run produced no injection at all)",
            path.display()
        )
    });
    assert!(
        !echoed.is_empty(),
        "the agent echoed nothing -- the run never reached the model, so this test          would be asserting a prompt that was never sent: {}",
        path.display()
    );
    (render, echoed)
}

/// DIRECTION 1: with a source document and a wiki page, a real run's injected
/// context carries memory AND knowledge AND wiki -- and the agent received them.
#[tokio::test]
async fn run_injects_memory_knowledge_and_wiki() {
    let Some(d) = boot("withkb", true).await else {
        skip_missing_mock();
        return;
    };
    let (render, echoed) = drive_run(&d, PROMPT).await;
    println!("T292 WITH render>>>{render}<<<");

    // The memory half (the only half the pre-existing e2e asserted).
    assert!(
        render.contains("<relevant_memories>"),
        "no memory block: {render}"
    );
    assert!(
        render.contains(MEMORY),
        "the seeded memory is missing: {render}"
    );
    // The half that had NO test before this one.
    assert!(
        render.contains("<knowledge>"),
        "a real run's context has no knowledge block: {render}"
    );
    assert!(
        render.contains(SOURCE_TEXT),
        "the source document did not reach the context: {render}"
    );
    assert!(
        render.contains("<wiki>"),
        "a real run's context has no wiki block: {render}"
    );
    assert!(
        render.contains(WIKI_TEXT),
        "the wiki page did not reach the context: {render}"
    );
    // And the user-visible end of the chain: the agent's own prompt carried it.
    assert!(
        echoed.contains(SOURCE_TEXT) && echoed.contains(WIKI_TEXT),
        "the agent's echoed prompt does not contain the knowledge: {echoed}"
    );
    assert!(
        echoed.contains(MEMORY),
        "the agent's echoed prompt does not contain the memory: {echoed}"
    );
}

/// DIRECTION 2: the same drive with NO documents in the knowledge base. The
/// knowledge and wiki blocks must be ABSENT -- not empty, not a placeholder.
/// Without this direction, direction 1 could pass on a renderer that emits the
/// tags unconditionally and the test would never notice.
#[tokio::test]
async fn run_without_knowledge_documents_has_no_knowledge_block() {
    let Some(d) = boot("nokb", false).await else {
        skip_missing_mock();
        return;
    };
    let (render, echoed) = drive_run(&d, PROMPT).await;
    println!("T292 WITHOUT render>>>{render}<<<");

    // The memory block is STILL there: this is what makes the absence below
    // meaningful rather than "nothing was injected at all".
    assert!(
        render.contains("<relevant_memories>"),
        "the memory block must survive the missing knowledge base: {render}"
    );
    assert!(render.contains(MEMORY), "{render}");
    assert!(
        !render.contains("<knowledge>"),
        "an empty knowledge base must produce NO knowledge block (never a          placeholder): {render}"
    );
    assert!(
        !render.contains("<wiki>"),
        "an empty knowledge base must produce NO wiki block: {render}"
    );
    assert!(
        !echoed.contains("<knowledge>"),
        "the agent saw a knowledge block that should not exist: {echoed}"
    );
}

/// Drive one real CHAT and return (the context_injected render, the agent's own
/// view of its prompt). Same three-level reading as drive_run: the render proves
/// the construction, the event proves it was sent, the echo proves the agent
/// received it.
async fn drive_chat(d: &TestDaemon, prompt: &str) -> (String, String) {
    let http = reqwest::Client::new();
    let chat: serde_json::Value = http
        .post(format!("{}/api/v1/chat", d.url))
        .json(&serde_json::json!({ "agent": "mock" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let chat_id = chat["id"].as_str().unwrap().to_string();
    http.post(format!("{}/api/v1/chat/{chat_id}/messages", d.url))
        .json(&serde_json::json!({ "text": prompt }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let path = d
        .root
        .join("data")
        .join("transcripts")
        .join(format!("run-{chat_id}.jsonl"));

    let mut render = None;
    let mut echoed = String::new();
    for _ in 0..600 {
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                match v["event"]["type"].as_str() {
                    Some("context_injected") => {
                        render = v["event"]["render"].as_str().map(|s| s.to_string());
                    }
                    Some("agent_message_chunk") => {
                        if let Some(t) = v["event"]["content"][0]["text"].as_str() {
                            echoed.push_str(t);
                        }
                    }
                    _ => {}
                }
            }
            if render.is_some() && !echoed.is_empty() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let render = render.unwrap_or_else(|| {
        panic!(
            "no context_injected event in the chat transcript {} -- the first prompt              carried no context at all",
            path.display()
        )
    });
    assert!(
        !echoed.is_empty(),
        "the agent echoed nothing, so this test would be asserting a prompt that          was never sent: {}",
        path.display()
    );
    (render, echoed)
}

/// The block headers of a render, in the order they appear. This is the thing
/// that DRIFTED once (a097c5e): the wording a producer wraps its blocks in.
fn headers_of(render: &str) -> Vec<String> {
    render
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with('<') && l.ends_with('>') && !l.starts_with("</"))
        .map(|l| l.to_string())
        .collect()
}

/// THE CHAT PATH, three levels deep. t292 covered the run path; this is the path
/// t259 called the MAIN one and the one that had already drifted once, and until
/// now it had no test touching its injection at all (injection_context had two
/// references: its definition and its one call site).
#[tokio::test]
async fn chat_injects_memory_knowledge_and_wiki() {
    let Some(d) = boot("chatwithkb", true).await else {
        skip_missing_mock();
        return;
    };
    let (render, echoed) = drive_chat(&d, PROMPT).await;
    println!("T302 CHAT WITH render>>>{render}<<<");

    assert!(
        render.contains("<relevant_memories>"),
        "the chat path injects no memory block: {render}"
    );
    assert!(render.contains(MEMORY), "{render}");
    assert!(
        render.contains("<knowledge>"),
        "a real chat's first prompt carries no knowledge block: {render}"
    );
    assert!(
        render.contains(SOURCE_TEXT),
        "the source document did not reach the chat context: {render}"
    );
    assert!(
        render.contains("<wiki>"),
        "a real chat's first prompt carries no wiki block: {render}"
    );
    assert!(
        render.contains(WIKI_TEXT),
        "the wiki page did not reach the chat context: {render}"
    );
    // Level 3: the agent RECEIVED it (the mock echoes its prompt).
    assert!(
        echoed.contains(SOURCE_TEXT) && echoed.contains(WIKI_TEXT) && echoed.contains(MEMORY),
        "the agent's echoed prompt does not contain the injected context: {echoed}"
    );
}

/// The same drive with an empty knowledge base: the knowledge and wiki blocks
/// must be ABSENT, and the memory block must still be there -- otherwise this
/// test cannot tell "correctly empty" from "never wired up".
#[tokio::test]
async fn chat_without_knowledge_documents_has_no_knowledge_block() {
    let Some(d) = boot("chatnokb", false).await else {
        skip_missing_mock();
        return;
    };
    let (render, echoed) = drive_chat(&d, PROMPT).await;
    println!("T302 CHAT WITHOUT render>>>{render}<<<");

    assert!(
        render.contains("<relevant_memories>") && render.contains(MEMORY),
        "the memory block must survive the missing knowledge base: {render}"
    );
    assert!(
        !render.contains("<knowledge>"),
        "an empty knowledge base must produce NO knowledge block in a chat: {render}"
    );
    assert!(
        !render.contains("<wiki>"),
        "an empty knowledge base must produce NO wiki block in a chat: {render}"
    );
    assert!(
        !echoed.contains("<knowledge>"),
        "the agent saw a knowledge block that should not exist: {echoed}"
    );
}

/// THE DRIFT CHECK (the direct answer to "the two paths have drifted once
/// already", a097c5e). Both producers now render through the same contract, so
/// their block headers must be IDENTICAL -- and the old chat-only header must be
/// gone from both.
#[tokio::test]
async fn chat_and_run_use_the_same_block_wording() {
    let Some(d) = boot("wording", true).await else {
        skip_missing_mock();
        return;
    };
    let (chat_render, _) = drive_chat(&d, PROMPT).await;
    let (run_render, _) = drive_run(&d, PROMPT).await;

    let chat_headers = headers_of(&chat_render);
    let run_headers = headers_of(&run_render);
    let old_header = ruagent_daemon::chat::HDR_MEMORY;
    println!("T302 chat_headers={chat_headers:?}");
    println!("T302 run_headers={run_headers:?}");
    println!("T302 old_chat_only_header={old_header:?}");
    println!(
        "T302 old_header_present chat={} run={}",
        chat_render.contains(old_header),
        run_render.contains(old_header)
    );

    assert_eq!(
        chat_headers, run_headers,
        "the two injection producers emit different block headers -- that is the          drift a097c5e recorded, and this is the test that would catch it again"
    );
    assert!(
        !chat_render.contains(old_header) && !run_render.contains(old_header),
        "the retired chat-only header is back: chat={} run={}",
        chat_render.contains(old_header),
        run_render.contains(old_header)
    );
    assert!(
        chat_headers.contains(&"<knowledge>".to_string())
            && chat_headers.contains(&"<wiki>".to_string()),
        "the wording test is only meaningful while both paths actually carry the          knowledge blocks: {chat_headers:?}"
    );
}

/// Every visible-truncation marker in a render, verbatim. t302's drift check
/// compared BLOCK HEADERS; this is the second half of the same idea -- the
/// inline marker VOCABULARY, which is where "one concept, two byte sequences"
/// actually lived (the contract rendered "… [+N chars truncated]" while the
/// retry context rendered "…[+N chars truncated]").
fn markers_of(render: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = render;
    while let Some(i) = rest.find("… [+") {
        let tail = &rest[i..];
        match tail.find(']') {
            Some(j) => {
                out.push(tail[..=j].to_string());
                rest = &tail[j..];
            }
            None => break,
        }
    }
    out
}

/// Seed one long observation: without it the per_block bound never binds, and a
/// test that only ever sees untruncated renders cannot tell WHICH bytes a
/// producer would emit.
async fn seed_long_observation(d: &TestDaemon, content: &str) {
    let outcome = ruagent_memory::write_memory(
        &d.db,
        &ruagent_memory::MemoryWrite {
            store: ruagent_memory::MemoryStore::Observation,
            namespace: ruagent_memory::Namespace::parse("user").unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, ruagent_memory::WriteOutcome::Inserted(_)),
        "seeding the long memory failed: {outcome:?}"
    );
}

/// t309: the truncation marker is ONE vocabulary, so both producers must render
/// the contract's exact bytes -- compared against the contract's own
/// constructor, not against a shape that a lookalike could also satisfy.
#[tokio::test]
async fn both_paths_emit_the_contract_truncation_marker() {
    let Some(d) = boot("markers", true).await else {
        skip_missing_mock();
        return;
    };
    seed_long_observation(&d, &"t309-long-memory ".repeat(90)).await;

    let (chat_render, _) = drive_chat(&d, PROMPT).await;
    let (run_render, _) = drive_run(&d, PROMPT).await;
    let chat_markers = markers_of(&chat_render);
    let run_markers = markers_of(&run_render);
    println!("T309 chat_markers={chat_markers:?}");
    println!("T309 run_markers={run_markers:?}");

    assert!(
        !chat_markers.is_empty() && !run_markers.is_empty(),
        "no marker to compare -- the bound never bound: chat={} run={}",
        chat_render.chars().count(),
        run_render.chars().count()
    );
    assert_eq!(
        chat_markers, run_markers,
        "the two producers emit different truncation markers"
    );
    for m in chat_markers.iter().chain(run_markers.iter()) {
        let n: usize = m
            .trim_start_matches("… [+")
            .trim_end_matches(" chars truncated]")
            .parse()
            .unwrap_or_else(|_| panic!("marker is not the vocabulary's shape: {m}"));
        assert_eq!(
            m,
            &ruagent_memory::inject::tail_truncated(n),
            "the render's marker is not what the contract's vocabulary produces"
        );
    }
}

/// t326, the render half: a REAL run retry. The crash snapshot rides as context
/// (ContextInjected), so the retried run's own render must open with the SHARED
/// retry prefix -- byte-equal to chat.rs's retry_head(), which runs.rs calls.
#[tokio::test]
async fn a_retried_run_carries_the_shared_retry_prefix() {
    let Some(d) = boot("retry", false).await else {
        skip_missing_mock();
        return;
    };
    let http = reqwest::Client::new();

    // 1. A run that DIES: the mock's crash behavior, as its own agent.
    let task: serde_json::Value = http
        .post(format!("{}/api/v1/tasks", d.url))
        .json(&serde_json::json!({ "title": "t326", "intent": "crash please" }))
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
        .post(format!("{}/api/v1/tasks/{task_id}/runs", d.url))
        .json(&serde_json::json!({ "agent": "mockcrash", "prompt": "crash please" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let dead_id = run["id"].as_str().unwrap().to_string();

    // 2. It must reach a terminal, retryable state.
    let mut status = String::new();
    for _ in 0..80 {
        let r: serde_json::Value = http
            .get(format!("{}/api/v1/runs/{dead_id}", d.url))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        status = r["status"].as_str().unwrap_or("").to_string();
        if matches!(status.as_str(), "failed" | "cancelled" | "interrupted") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    println!("T326 the crashed run ended as {status:?}");
    assert!(
        matches!(status.as_str(), "failed" | "cancelled" | "interrupted"),
        "the crash run must end retryable, got {status:?}"
    );

    // 3. Retry it -- the new run inherits the crash snapshot as CONTEXT.
    let retried: serde_json::Value = http
        .post(format!("{}/api/v1/runs/{dead_id}/retry", d.url))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let new_id = retried["id"].as_str().unwrap().to_string();

    // 4. Its ContextInjected render must open with the shared prefix.
    let path = d
        .root
        .join("data")
        .join("transcripts")
        .join(format!("run-{new_id}.jsonl"));
    let mut render = None;
    for _ in 0..80 {
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if v["event"]["type"] == "context_injected" {
                        render = v["event"]["render"].as_str().map(str::to_string);
                    }
                }
            }
        }
        if render.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let render = render.expect("the retried run must emit context_injected");
    let shown = &render[..render.len().min(200)];
    println!("T326 retried run render starts: {shown:?}");
    let head = render.split(" — ").next().unwrap_or("");
    assert_eq!(
        head,
        ruagent_daemon::chat::retry_head(),
        "the retried run's prefix is not the shared bytes: {render}"
    );
}
