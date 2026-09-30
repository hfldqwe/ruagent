//! The capability plane over a real in-test daemon: the `GET`/`PUT
//! `/api/v1/capabilities` surface, the `policy.toml` it writes, and the hard
//! errors a bad configuration raises at load
//! (docs/plans/capability-plugins-design.md §4, §5, §14, §18).

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::capability::{CapabilityId, CapabilityPlane};
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

/// Boot the daemon with `policy_toml` as its `policy.toml`, exactly as
/// `ruagent serve` does for the parts this surface touches.
async fn start_test_daemon(policy_toml: &str) -> (String, std::path::PathBuf) {
    static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-capapi-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let config_dir = root.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("agents.toml"),
        "[agent.mock]\nharness = \"mock\"\ncommand = \"mock-agent\"\ndescription = \"x\"\nenabled = false\n",
    )
    .unwrap();
    std::fs::write(
        config_dir.join("mcp.toml"),
        "[profile.default]\nservers = []\n",
    )
    .unwrap();
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
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    // Boot installs the plane the file defines into the ONE shared handle
    // (design §17.8, the RunManager gets the same Arc). Until that integration
    // row lands the handle holds `legacy()`, which is L1; the test installs it
    // here so the HTTP surface is exercised exactly as it will run.
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
    (format!("http://{addr}"), root)
}

fn row<'a>(body: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    body["capabilities"]
        .as_array()
        .unwrap_or_else(|| panic!("capabilities is not an array: {body}"))
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no row for `{id}` in {body}"))
}

async fn get_capabilities(url: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{url}/api/v1/capabilities"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn policy_text(root: &std::path::Path) -> String {
    std::fs::read_to_string(root.join("config").join("policy.toml")).unwrap()
}

#[tokio::test]
async fn a_fresh_root_is_legacy_mode_and_lists_every_capability() {
    let (url, root) = start_test_daemon("[permissions]\ndefault = \"ask\"\n").await;
    let v = get_capabilities(&url).await;
    assert_eq!(v["table_present"], false, "{v}");
    assert_eq!(
        v["config_file"].as_str().unwrap(),
        root.join("config")
            .join("policy.toml")
            .display()
            .to_string()
    );
    assert_eq!(v["conflicts"].as_array().unwrap().len(), 0);
    let rows = v["capabilities"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        CapabilityId::ALL.len(),
        "one row per registry id"
    );
    for (i, id) in CapabilityId::ALL.iter().enumerate() {
        assert_eq!(rows[i]["id"], id.as_str(), "registry order");
        // With the table absent every row is legacy, and its own state is the
        // registry default (today's behaviour, L1).
        assert_eq!(rows[i]["configured"], "legacy", "{}", id.as_str());
        assert_eq!(
            rows[i]["enabled"],
            rows[i]["default_enabled"],
            "{}",
            id.as_str()
        );
        assert!(
            rows[i]["description"]
                .as_str()
                .is_some_and(|d| !d.is_empty())
        );
        assert!(rows[i]["gates"].as_str().is_some_and(|g| !g.is_empty()));
    }
    let llm = row(&v, "distill_session");
    assert_eq!(llm["tier"], "llm");
    assert_eq!(
        llm["enabled"], false,
        "the llm tier costs tokens: off by default"
    );
    let semantic = row(&v, "recall_leg_memory_semantic");
    assert_eq!(semantic["tier"], "free");
    assert_eq!(semantic["options"]["weight"], 1.0);
    assert_eq!(semantic["options"]["min_score"], serde_json::Value::Null);
}

#[tokio::test]
async fn a_file_table_overrides_only_the_keys_it_names() {
    let (url, _root) = start_test_daemon(
        "[permissions]\ndefault = \"ask\"\n\n[capabilities.recall_leg_graph]\nenabled = false\n",
    )
    .await;
    let v = get_capabilities(&url).await;
    assert_eq!(v["table_present"], true);
    let graph = row(&v, "recall_leg_graph");
    assert_eq!(graph["enabled"], false);
    assert_eq!(graph["configured"], "file");
    for r in v["capabilities"].as_array().unwrap() {
        if r["id"] == "recall_leg_graph" {
            continue;
        }
        assert_eq!(r["configured"], "default", "nothing else was named: {r}");
        assert_eq!(r["enabled"], r["default_enabled"], "{r}");
    }
}

#[tokio::test]
async fn a_file_table_activates_the_plane_and_narrowing_is_reported() {
    let (url, _root) = start_test_daemon(
        "[permissions]\ndefault = \"ask\"\n\n[distill]\nauto = true\n\
         \n[capabilities.recall_leg_wiki]\nenabled = false\n",
    )
    .await;
    let v = get_capabilities(&url).await;
    // The llm tier now defaults OFF because the table EXISTS (not because
    // anything narrowed it yet).
    assert_eq!(row(&v, "distill_session")["enabled"], false);
    // ...and that is exactly the loss the operator must be told about.
    let conflicts = v["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{v}");
    assert_eq!(conflicts[0]["id"], "distill_session");
    assert_eq!(conflicts[0]["legacy_key"], "distill.auto");
    assert!(
        conflicts[0]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("unattended distillation")),
        "{v}"
    );
}

#[tokio::test]
async fn put_persists_the_table_and_get_reflects_it() {
    let (url, root) = start_test_daemon(
        "# top comment\n[permissions]\ndefault = \"ask\"\n\n# distillation\n[distill]\nauto = false\n",
    )
    .await;
    let http = reqwest::Client::new();
    let resp = http
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({
            "capabilities": {
                "recall_leg_wiki": { "enabled": false },
                "knowledge_ingest_graph": { "enabled": true, "max_per_input": 7 }
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(v["table_present"], true);
    assert_eq!(row(&v, "recall_leg_wiki")["enabled"], false);
    assert_eq!(row(&v, "recall_leg_wiki")["configured"], "file");
    let kg = row(&v, "knowledge_ingest_graph");
    assert_eq!(
        kg["enabled"], true,
        "a free capability needs no cost confirmation"
    );
    assert_eq!(kg["configured"], "file");
    assert_eq!(kg["options"]["max_per_input"], 7);
    assert_eq!(
        kg["options"]["max_docs_per_pass"], 20,
        "an option key the request did not name keeps its declared default"
    );
    // GET answers from the SAME live plane the PUT swapped: no second shape.
    assert_eq!(get_capabilities(&url).await, v);
    // The file is the source of truth, round-tripped: comments and the other
    // sections survive, and the table is readable TOML.
    let text = policy_text(&root);
    assert!(text.contains("# top comment"), "{text}");
    assert!(text.contains("# distillation"), "{text}");
    assert!(text.contains("[distill]"), "{text}");
    assert!(text.contains("[capabilities.recall_leg_wiki]"), "{text}");
    assert!(text.contains("max_per_input = 7"), "{text}");
    // ...and a reload yields the same plane, so the write is durable.
    let cfg = DaemonConfig::load(&root).unwrap();
    assert!(cfg.capabilities.table_present());
    assert!(!cfg.capabilities.gate(CapabilityId::RecallLegWiki, true));
    assert!(
        cfg.capabilities
            .gate(CapabilityId::KnowledgeIngestGraph, true)
    );
    assert_eq!(
        cfg.capabilities
            .options(CapabilityId::KnowledgeIngestGraph)
            .max_per_input,
        Some(7)
    );
}

#[tokio::test]
async fn put_refuses_an_unknown_id_and_leaves_the_file_alone() {
    let (url, root) = start_test_daemon("[permissions]\ndefault = \"ask\"\n").await;
    let before = policy_text(&root);
    let resp = reqwest::Client::new()
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({
            "capabilities": { "session_extract_rule": { "enabled": true } }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = resp.text().await.unwrap();
    assert!(body.contains("session_extract_rule"), "{body}");
    assert!(
        body.contains("session_extract_rules"),
        "the message lists the known ids: {body}"
    );
    assert_eq!(
        policy_text(&root),
        before,
        "a refused update writes nothing"
    );
}

#[tokio::test]
async fn put_refuses_an_undeclared_option_key_and_an_out_of_range_value() {
    let (url, root) = start_test_daemon("[permissions]\ndefault = \"ask\"\n").await;
    let before = policy_text(&root);
    let http = reqwest::Client::new();
    let resp = http
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({
            "capabilities": { "distill_session": { "enabled": true, "weight": 1.0 } }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = resp.text().await.unwrap();
    assert!(body.contains("distill_session"), "{body}");
    assert!(body.contains("weight"), "{body}");
    let resp = http
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({
            "capabilities": { "session_extract_rules": { "max_per_input": 0 } }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = resp.text().await.unwrap();
    assert!(body.contains("max_per_input"), "{body}");
    assert!(body.contains("1..=10000"), "{body}");
    assert_eq!(
        policy_text(&root),
        before,
        "a refused update writes nothing"
    );
}

#[tokio::test]
async fn put_refuses_an_llm_enable_without_confirm_cost() {
    let (url, root) = start_test_daemon("[permissions]\ndefault = \"ask\"\n").await;
    let before = policy_text(&root);
    let http = reqwest::Client::new();
    let resp = http
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({
            "capabilities": { "distill_session": { "enabled": true } }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409, "an unconfirmed spend is refused loudly");
    let body = resp.text().await.unwrap();
    assert!(body.contains("distill_session"), "{body}");
    assert!(body.contains("llm-tier"), "{body}");
    assert!(body.contains("confirm_cost"), "{body}");
    assert_eq!(
        policy_text(&root),
        before,
        "the refused request wrote nothing"
    );
    assert_eq!(
        get_capabilities(&url).await["table_present"],
        false,
        "and the live plane did not move"
    );
    // With the confirmation the same request lands.
    let resp = http
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({
            "confirm_cost": true,
            "capabilities": { "distill_session": { "enabled": true } }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(row(&v, "distill_session")["enabled"], true);
    assert_eq!(row(&v, "distill_session")["configured"], "file");
    // Now that it is on, re-sending it without the confirmation is a no-op,
    // new spend is what needs consent.
    let resp = http
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({
            "capabilities": { "distill_session": { "enabled": true } }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn an_empty_map_restores_legacy_mode() {
    let (url, root) = start_test_daemon(
        "[permissions]\ndefault = \"ask\"\n\n[capabilities.recall_leg_wiki]\nenabled = false\n",
    )
    .await;
    assert_eq!(get_capabilities(&url).await["table_present"], true);
    let resp = reqwest::Client::new()
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({ "capabilities": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        v["table_present"], false,
        "an empty map REMOVES the table: back to exactly today's behaviour"
    );
    for r in v["capabilities"].as_array().unwrap() {
        assert_eq!(r["configured"], "legacy", "{r}");
    }
    assert!(
        !policy_text(&root).contains("[capabilities"),
        "the table is gone from the file too"
    );
    assert_eq!(get_capabilities(&url).await["table_present"], false);
}

/// THE LIVE SWAP (t8): a `PUT` must reach the OTHER consumers of the plane, not
/// only the capabilities endpoint. The recall route is the one a probe can read
/// without any data: a disabled leg is reported in `legs_disabled` and its
/// section is empty, on the SAME running daemon, with no restart — and removing
/// the table puts it back.
#[tokio::test]
async fn a_put_swaps_the_plane_the_recall_route_reads() {
    let (url, _root) = start_test_daemon("[permissions]\ndefault = \"ask\"\n").await;
    let http = reqwest::Client::new();
    let recall = |url: String| async move {
        reqwest::Client::new()
            .get(format!("{url}/api/v1/recall"))
            .query(&[("q", "deploy")])
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()
    };
    // Legacy mode: no leg is disabled, so the wiki leg runs.
    let before = recall(url.clone()).await;
    assert!(
        before["legs_disabled"].as_array().unwrap().is_empty(),
        "{before}"
    );
    // Narrow one leg through the HTTP face.
    let resp = http
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({
            "capabilities": { "recall_leg_wiki": { "enabled": false } }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    // The consumer sees it at once: same daemon, no restart.
    let after = recall(url.clone()).await;
    assert_eq!(
        after["legs_disabled"],
        serde_json::json!(["recall_leg_wiki"]),
        "{after}"
    );
    assert!(
        after["wiki"].as_array().unwrap().is_empty(),
        "the disabled leg's section is empty, not a silent miss: {after}"
    );
    // Removing the table restores today's behaviour everywhere.
    let resp = http
        .put(format!("{url}/api/v1/capabilities"))
        .json(&serde_json::json!({ "capabilities": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let back = recall(url.clone()).await;
    assert!(
        back["legs_disabled"].as_array().unwrap().is_empty(),
        "{back}"
    );
}

#[tokio::test]
async fn an_unknown_id_in_the_file_fails_the_boot_naming_it() {
    static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-capbad-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::write(
        root.join("config").join("policy.toml"),
        "[capabilities.distill_sesion]\nenabled = true\n",
    )
    .unwrap();
    let err = DaemonConfig::load(&root).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("distill_sesion"), "{msg}");
    assert!(
        msg.contains("distill_session"),
        "lists the known ids: {msg}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_plane_can_only_narrow() {
    // The L2 law, stated once outside the crate's own unit tests: a capability
    // that is off cannot turn work on that the legacy flag did not ask for.
    let plane = CapabilityPlane::legacy();
    for id in CapabilityId::ALL {
        assert!(plane.gate(*id, true));
        assert!(!plane.gate(*id, false));
    }
}
