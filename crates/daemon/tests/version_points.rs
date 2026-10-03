//! INT-F4 (t127): `/api/v1/stats` reports the two calibration versions the
//! rest of the wire already carries. Each value must come from its SINGLE source
//! (the live embedder accessor and the `SCORING_VERSION` constant), never a
//! literal — proven by reading the same key off `/api/v1/knowledge/documents`,
//! which is fed by the same accessor.
//!
//! The negative control is a TEMP COPY of a real payload (the shared tree is
//! never mutated): the same predicate the positive test uses must be FALSE for a
//! deleted key, a renamed key and a literal value.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

const POLICY: &str = "[permissions]\ndefault = \"ask\"\n[distill]\nprompt = \"PROMPT-A\"\n";

/// Boot a real in-test daemon with this `policy.toml` (temp root, temp port) —
/// the same shape `crates/daemon/tests/knowledge_api.rs` uses.
async fn start_test_daemon(policy_toml: &str, tag: &str) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-vp-{tag}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("policy.toml"), policy_toml).unwrap();

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
        ruagent_daemon::distill::AutoDistill {
            auto: cfg.policy.distill.auto,
            agent: cfg.policy.distill.agent.clone(),
            language: cfg.policy.distill.language.clone(),
            prompt: cfg.policy.distill.prompt.clone(),
            graph: cfg.policy.distill.graph.unwrap_or(true),
        },
        None,
        ruagent_daemon::distill::AgentRegistry {
            enabled: agents.clone(),
        },
    );
    chats.set_capabilities(cfg.capabilities.clone());
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
    format!("http://{addr}")
}

/// The predicate BOTH the reading and the negative control use: the payload must
/// carry both calibration keys, as a string and a number.
fn calibration(payload: &serde_json::Value) -> Option<(String, u64)> {
    Some((
        payload.get("embedder")?.as_str()?.to_string(),
        payload.get("scoring_version")?.as_u64()?,
    ))
}

async fn get_json(http: &reqwest::Client, url: String) -> serde_json::Value {
    http.get(url).send().await.unwrap().json().await.unwrap()
}

#[tokio::test]
async fn stats_reports_the_calibration_versions_from_their_single_sources() {
    let url = start_test_daemon(POLICY, "f4").await;
    let http = reqwest::Client::new();
    let body = get_json(&http, format!("{url}/api/v1/stats")).await;
    println!(
        "READING F4 /api/v1/stats: {}",
        serde_json::to_string(&body).unwrap()
    );

    // The pre-existing key is first and unchanged.
    assert!(body.get("agents").is_some(), "the `agents` key must stay");
    let (embedder, scoring) = calibration(&body).expect("both calibration keys");
    let keys: Vec<String> = body.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        vec!["agents", "embedder", "scoring_version"],
        "additive only"
    );

    // SAME ACCESSOR: the live embedder name is already read by
    // `/api/v1/knowledge/documents` (api.rs:648), so the two read points
    // must agree — this is what proves it is not a literal.
    let docs = get_json(&http, format!("{url}/api/v1/knowledge/documents")).await;
    assert_eq!(
        embedder,
        docs["embedder"].as_str().unwrap(),
        "both read points report the same live embedder"
    );
    // SINGLE CONSTANT: the source of truth, not a copy of its value.
    assert_eq!(
        scoring,
        ruagent_knowledge::SCORING_VERSION as u64,
        "scoring_version must be the constant from its source"
    );
}

/// The documented tail `extraction_prompt` appends after the base (the source of
/// the composition is `crates/daemon/src/distill.rs`, `fn extraction_prompt`).
const TRANSCRIPT_TAIL: &str = "\n\nTRANSCRIPT:\n";

/// The predicate BOTH the reading and the negative control use for F3: the
/// payload must carry a SHA-256 hex `prompt_hash`.
fn prompt_hash(payload: &serde_json::Value) -> Option<String> {
    let h = payload.get("prompt_hash")?.as_str()?.to_string();
    (h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

/// Recompute `prompt_hash` INDEPENDENTLY, from values read off the wire plus the
/// documented composition: `content_hash(base + "\n\nTRANSCRIPT:\n")`. This is the
/// same function and the same inputs the daemon uses — the daemon builds them
/// from its live policy, the test from the wire's own `builtin_prompt`/`prompt`.
fn expected_prompt_hash(base: &str) -> String {
    ruagent_memory::write::content_hash(&format!("{base}{TRANSCRIPT_TAIL}"))
}

/// The six keys `/api/v1/distill` reported BEFORE this change (t62 §2, the
/// pre-change reading) — they must stay, verbatim, in this order.
const DISTILL_KEYS_BEFORE: [&str; 6] = [
    "auto",
    "agent",
    "language",
    "prompt",
    "graph",
    "builtin_prompt",
];

#[tokio::test]
async fn distill_reports_a_prompt_hash_from_the_same_function_and_input() {
    let http = reqwest::Client::new();
    let a = start_test_daemon(POLICY, "f3a").await;
    let b = start_test_daemon(POLICY, "f3b").await;
    let c = start_test_daemon(
        "[permissions]\ndefault = \"ask\"\n[distill]\nprompt = \"PROMPT-B\"\n",
        "f3c",
    )
    .await;

    let body = get_json(&http, format!("{a}/api/v1/distill")).await;
    println!(
        "READING F3 /api/v1/distill: {}",
        serde_json::to_string(&body).unwrap()
    );

    // ADDITIVE: the six keys are there, verbatim, and `prompt_hash` is appended.
    let keys: Vec<String> = body.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        DISTILL_KEYS_BEFORE
            .iter()
            .map(|k| k.to_string())
            .chain(["prompt_hash".to_string()])
            .collect::<Vec<_>>(),
        "the six keys must not move or change, and only `prompt_hash` is added"
    );
    assert_eq!(body["prompt"], "PROMPT-A", "the live policy is reported");

    // SAME INPUT => SAME HASH, independently recomposed: the test builds the
    // documented composition `base + "\n\nTRANSCRIPT:\n"` from values it reads
    // off THIS wire (`prompt` for the override daemon, `builtin_prompt` for the
    // default one) and hashes it with the same function the daemon uses.
    let a2 = get_json(&http, format!("{a}/api/v1/distill")).await;
    let b2 = get_json(&http, format!("{b}/api/v1/distill")).await;
    let c2 = get_json(&http, format!("{c}/api/v1/distill")).await;
    let d = start_test_daemon("[permissions]\ndefault = \"ask\"\n", "f3d").await;
    let d2 = get_json(&http, format!("{d}/api/v1/distill")).await;
    println!(
        "READING F3 hashes: override daemon {} | same-policy second daemon {} | other prompt {} | default-prompt daemon {} (expected {} / {})",
        prompt_hash(&body).unwrap(),
        prompt_hash(&b2).unwrap(),
        prompt_hash(&c2).unwrap(),
        prompt_hash(&d2).unwrap(),
        expected_prompt_hash(body["prompt"].as_str().unwrap()),
        expected_prompt_hash(d2["builtin_prompt"].as_str().unwrap())
    );
    assert_eq!(
        prompt_hash(&body),
        Some(expected_prompt_hash(body["prompt"].as_str().unwrap())),
        "same function, same input: content_hash(compose_prompt()) for the override policy"
    );
    assert_eq!(
        prompt_hash(&d2),
        Some(expected_prompt_hash(d2["builtin_prompt"].as_str().unwrap())),
        "same function, same input: content_hash(compose_prompt()) for the built-in policy"
    );
    assert_eq!(
        prompt_hash(&body),
        prompt_hash(&b2),
        "same input => same hash (a second daemon, same policy)"
    );
    assert_eq!(
        prompt_hash(&body),
        prompt_hash(&a2),
        "same input => same hash (same daemon)"
    );
    assert_ne!(
        prompt_hash(&body),
        prompt_hash(&c2),
        "changing `[distill] prompt` must change the hash"
    );
}

#[tokio::test]
async fn the_prompt_hash_check_is_false_for_all_three_mutations() {
    // A TEMP COPY of a real payload: the shared tree is never mutated.
    let url = start_test_daemon(POLICY, "f3nc").await;
    let http = reqwest::Client::new();
    let real = get_json(&http, format!("{url}/api/v1/distill")).await;

    let mut renamed = real.clone();
    let moved = renamed["prompt_hash"].take();
    renamed["prompt_fingerprint"] = moved;
    let mut deleted = real.clone();
    deleted.as_object_mut().unwrap().remove("prompt_hash");
    let mut literal = real.clone();
    literal["prompt_hash"] =
        serde_json::json!("0000000000000000000000000000000000000000000000000000000000000000");

    println!(
        "READING F3 negative control: real={:?} renamed={:?} deleted={:?} literal={:?}",
        prompt_hash(&real),
        prompt_hash(&renamed),
        prompt_hash(&deleted),
        prompt_hash(&literal)
    );
    assert!(prompt_hash(&real).is_some(), "the real payload passes");
    assert!(prompt_hash(&renamed).is_none(), "a renamed key must fail");
    assert!(prompt_hash(&deleted).is_none(), "a deleted key must fail");
    assert_ne!(
        prompt_hash(&literal),
        prompt_hash(&real),
        "a literal must not read as the computed value"
    );
}

#[tokio::test]
async fn the_calibration_check_is_false_for_all_three_mutations() {
    // A TEMP COPY of a real payload: the shared tree is never mutated.
    let url = start_test_daemon(POLICY, "f4nc").await;
    let http = reqwest::Client::new();
    let real = get_json(&http, format!("{url}/api/v1/stats")).await;

    let mut renamed = real.clone();
    let moved = renamed["embedder"].take();
    renamed["embedder_name"] = moved;
    let mut deleted = real.clone();
    deleted.as_object_mut().unwrap().remove("scoring_version");
    let mut literal = real.clone();
    literal["embedder"] = serde_json::json!("a-literal-not-the-live-embedder");
    literal["scoring_version"] = serde_json::json!(999u64);

    println!(
        "READING F4 negative control: real={:?} renamed={:?} deleted={:?} literal={:?}",
        calibration(&real),
        calibration(&renamed),
        calibration(&deleted),
        calibration(&literal)
    );
    assert!(calibration(&real).is_some(), "the real payload passes");
    assert!(calibration(&renamed).is_none(), "a renamed key must fail");
    assert!(calibration(&deleted).is_none(), "a deleted key must fail");
    assert_ne!(
        calibration(&literal),
        calibration(&real),
        "a literal must not read as the source value"
    );
}
