//! Knowledge base → knowledge graph ingestion (t4,
//! docs/plans/capability-plugins-design.md §13).
//!
//! Mock-driven by construction — no harness, no API key, no network. The
//! in-process half opens `Knowledge` on a temp root with the offline hash
//! embedder (`Knowledge::open`, files.rs:415) over `Db::open_in_memory`. The
//! HTTP half drives a real in-test daemon on 127.0.0.1 with a single DISABLED
//! mock agent card, exactly like `knowledge_api.rs` does.
//!
//! What is pinned, in the order of the t4 acceptance items:
//!   * OFF BY DEFAULT: `POST /api/v1/knowledge/ingest` writes the file and its
//!     chunks and NOTHING else (no entity, no relation, no ledger row, no
//!     `distill_log` row — no model or agent is invoked from this path);
//!   * with the capability off, a REAL ingest is refused (409) naming the
//!     capability, while `dry_run` still prices it;
//!   * with the capability enabled the document's entities/relations land in the
//!     graph through `ruagent_extract::graph_candidates` (zero tokens);
//!   * re-ingesting an unchanged document writes nothing;
//!   * a changed document retracts its previous revision and leaves no orphan;
//!   * a document that loses its content leaves nothing behind;
//!   * another document's claim survives someone else's revision;
//!   * a sweep is bounded by `max_docs_per_pass` and skips what it cannot read.

use std::sync::Arc;

use ruagent_daemon::api::AppState;
use ruagent_daemon::capability::{CapabilityId, CapabilityPlane};
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::knowledge_graph::{self as kg, IngestOptions};
use ruagent_daemon::runs::RunManager;
use ruagent_knowledge::Knowledge;
use ruagent_store::Db;

// ---------------------------------------------------------------------------
// In-process half: the ingestion itself.
// ---------------------------------------------------------------------------

const DOC_A: &str = "# Alpha\n\n# Beta\n\nAlpha is a Beta.\n";
const DOC_B: &str = "# Alpha\n\n# Gamma\n\nAlpha is a Gamma.\n";
const DOC_HEADLESS: &str = "Nothing but prose, and no headings at all.\n";

struct Fixture {
    db: Db,
    kb: Knowledge,
}

async fn fixture(tag: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "ruagent-t4-graphingest-{}-{}-{tag}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Db::open_in_memory().unwrap();
    let kb = Knowledge::open(&dir, db.clone()).await.unwrap();
    Fixture { db, kb }
}

async fn scalar(db: &Db, sql: &'static str) -> i64 {
    db.call_flat(move |conn| conn.query_row(sql, [], |r| r.get(0)))
        .await
        .unwrap()
}

async fn entity_names(db: &Db) -> Vec<String> {
    db.call_flat(|conn| {
        let mut stmt = conn.prepare("SELECT norm_name FROM entities ORDER BY norm_name")?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()
    })
    .await
    .unwrap()
}

/// `(src, dst, relation, fact)` of every CURRENT edge.
async fn live_edges(db: &Db) -> Vec<(String, String, String, String)> {
    db.call_flat(|conn| {
        let mut stmt = conn.prepare(
            "SELECT s.norm_name, d.norm_name, e.relation, e.fact_text
               FROM entity_edges e
               JOIN entities s ON s.id = e.src
               JOIN entities d ON d.id = e.dst
              WHERE e.invalid_at IS NULL
              ORDER BY s.norm_name, e.relation, d.norm_name",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<Vec<_>, _>>()
    })
    .await
    .unwrap()
}

async fn ledger_rows(db: &Db) -> i64 {
    scalar(db, "SELECT COUNT(*) FROM knowledge_graph_ingest").await
}

#[tokio::test]
async fn an_ingest_puts_the_documents_entities_and_relations_in_the_graph() {
    let f = fixture("ingest").await;
    f.kb.save("doc-a", DOC_A).await.unwrap();

    let report = kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();

    assert_eq!(report.documents, 1, "{report:?}");
    assert_eq!(report.ledger_hits, 0, "{report:?}");
    assert!(report.entities >= 2, "Alpha and Beta: {report:?}");
    assert!(report.relations >= 1, "Alpha is_a Beta: {report:?}");
    assert!(entity_names(&f.db).await.contains(&"alpha".to_string()));
    assert!(entity_names(&f.db).await.contains(&"beta".to_string()));
    assert!(
        live_edges(&f.db)
            .await
            .iter()
            .any(|(s, d, r, _)| s == "alpha" && d == "beta" && r == "is_a"),
        "{:?}",
        live_edges(&f.db).await
    );
    assert_eq!(ledger_rows(&f.db).await, 1);

    // The ledger answers "is my knowledge base in the graph?".
    let status = kg::ledger_status(&f.db, 10).await.unwrap();
    assert_eq!(status.documents, 1);
    assert_eq!(status.relations, report.relations as i64);
    assert_eq!(status.last.len(), 1);
    assert_eq!(status.last[0].document_name, "doc-a");
    assert_eq!(
        status.last[0].content_hash,
        ruagent_knowledge::sha256_hex(DOC_A.as_bytes())
    );
}

#[tokio::test]
async fn re_ingesting_an_unchanged_document_touches_nothing() {
    let f = fixture("idempotent").await;
    f.kb.save("doc-a", DOC_A).await.unwrap();
    let first = kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();
    assert_eq!(first.documents, 1);
    let entities = scalar(&f.db, "SELECT COUNT(*) FROM entities").await;
    let edges = scalar(&f.db, "SELECT COUNT(*) FROM entity_edges").await;

    let second = kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();

    assert_eq!(second.ledger_hits, 1, "{second:?}");
    assert_eq!(second.documents, 0, "{second:?}");
    assert_eq!(second.entities, 0, "{second:?}");
    assert_eq!(second.relations, 0, "{second:?}");
    assert_eq!(
        scalar(&f.db, "SELECT COUNT(*) FROM entities").await,
        entities
    );
    assert_eq!(
        scalar(&f.db, "SELECT COUNT(*) FROM entity_edges").await,
        edges
    );
    assert_eq!(
        ledger_rows(&f.db).await,
        1,
        "one ledger row per document, not per run"
    );
}

#[tokio::test]
async fn a_changed_document_retracts_its_previous_revision() {
    let f = fixture("revised").await;
    f.kb.save("doc-a", DOC_A).await.unwrap();
    kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();

    f.kb.save("doc-a", DOC_B).await.unwrap();
    let second = kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();

    assert_eq!(second.documents, 1, "{second:?}");
    assert_eq!(second.ledger_hits, 0, "{second:?}");
    let names = entity_names(&f.db).await;
    assert!(names.contains(&"alpha".to_string()), "{names:?}");
    assert!(names.contains(&"gamma".to_string()), "{names:?}");
    assert!(
        !names.contains(&"beta".to_string()),
        "the dropped entity is gone, not orphaned: {names:?}"
    );
    let live = live_edges(&f.db).await;
    assert_eq!(live.len(), 1, "{live:?}");
    assert_eq!(live[0].0, "alpha");
    assert_eq!(live[0].1, "gamma");
    // The previous revision's fact is retracted, and — because the entity it
    // pointed at is the one this revision dropped — its history row goes with
    // that entity (an edge cannot outlive its endpoint: the FK is real, and the
    // rule only ever removes history that THIS revision stated). Nothing of the
    // old claim is live, and one claim is live: the new one.
    assert_eq!(
        scalar(&f.db, "SELECT COUNT(*) FROM entity_edges").await,
        1,
        "only the current revision's claim is left"
    );
    assert_eq!(ledger_rows(&f.db).await, 1, "the row follows the revision");
    let status = kg::ledger_status(&f.db, 10).await.unwrap();
    assert_eq!(
        status.last[0].content_hash,
        ruagent_knowledge::sha256_hex(DOC_B.as_bytes()),
        "the ledger remembers the revision it extracted"
    );
}

#[tokio::test]
async fn a_document_that_loses_its_content_leaves_no_orphans() {
    let f = fixture("emptied").await;
    f.kb.save("doc-a", DOC_A).await.unwrap();
    kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();

    f.kb.save("doc-a", DOC_HEADLESS).await.unwrap();
    let second = kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();

    assert_eq!(second.documents, 1, "{second:?}");
    assert_eq!(second.candidates, 0, "{second:?}");
    assert!(
        entity_names(&f.db).await.is_empty(),
        "no orphaned nodes: {:?}",
        entity_names(&f.db).await
    );
    assert!(live_edges(&f.db).await.is_empty());
    assert_eq!(
        scalar(&f.db, "SELECT COUNT(*) FROM entity_edges").await,
        0,
        "nothing of the previous revision is left, not even history"
    );
    assert_eq!(ledger_rows(&f.db).await, 1);
}

#[tokio::test]
async fn another_documents_claim_survives_a_revision_change() {
    let f = fixture("shared-entity").await;
    f.kb.save("doc-a", DOC_A).await.unwrap();
    f.kb.save("keeper", "# Beta\n").await.unwrap();
    kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();
    kg::ingest_document(&f.db, &f.kb, "keeper", &IngestOptions::default())
        .await
        .unwrap();

    // doc-a stops mentioning Beta entirely; `keeper` still does.
    f.kb.save("doc-a", "# Alpha\n").await.unwrap();
    kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();

    let names = entity_names(&f.db).await;
    assert!(
        names.contains(&"beta".to_string()),
        "another document still claims Beta: {names:?}"
    );
    assert!(names.contains(&"alpha".to_string()), "{names:?}");
    // doc-a's fact (alpha is_a beta) is retracted — nothing else states it — and
    // because BOTH its endpoints survive, the history row is KEPT and closed on
    // the record axis: the retraction rule deletes nothing another source could
    // still need.
    assert!(live_edges(&f.db).await.is_empty());
    assert_eq!(
        scalar(&f.db, "SELECT COUNT(*) FROM entity_edges").await,
        1,
        "the retracted fact keeps its history row here"
    );
    assert_eq!(
        scalar(
            &f.db,
            "SELECT COUNT(*) FROM entity_edges WHERE invalid_at IS NOT NULL"
        )
        .await,
        1,
        "the retraction closed invalid_at"
    );
}

#[tokio::test]
async fn a_dry_run_prices_the_ingest_and_writes_nothing() {
    let f = fixture("dry-run").await;
    f.kb.save("doc-a", DOC_A).await.unwrap();

    let report = kg::ingest_document(
        &f.db,
        &f.kb,
        "doc-a",
        &IngestOptions {
            dry_run: true,
            ..IngestOptions::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(report.documents, 0, "{report:?}");
    assert!(report.candidates >= 3, "the price: {report:?}");
    assert_eq!(scalar(&f.db, "SELECT COUNT(*) FROM entities").await, 0);
    assert_eq!(scalar(&f.db, "SELECT COUNT(*) FROM entity_edges").await, 0);
    assert_eq!(ledger_rows(&f.db).await, 0, "a price is not a purchase");

    // …and the priced document is then ingested for real, unchanged.
    let real = kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();
    assert_eq!(real.documents, 1, "{real:?}");
}

#[tokio::test]
async fn a_sweep_is_bounded_by_max_docs_per_pass() {
    let f = fixture("bounded").await;
    f.kb.save("one", "# One\n").await.unwrap();
    f.kb.save("two", "# Two\n").await.unwrap();
    f.kb.save("three", "# Three\n").await.unwrap();
    let opts = IngestOptions {
        max_docs_per_pass: 2,
        ..IngestOptions::default()
    };

    let first = kg::sweep(&f.db, &f.kb, &opts).await.unwrap();
    assert_eq!(first.documents, 2, "{first:?}");
    assert_eq!(scalar(&f.db, "SELECT COUNT(*) FROM entities").await, 2);

    let second = kg::sweep(&f.db, &f.kb, &opts).await.unwrap();
    assert_eq!(second.documents, 1, "{second:?}");
    assert_eq!(second.ledger_hits, 2, "{second:?}");
    assert_eq!(scalar(&f.db, "SELECT COUNT(*) FROM entities").await, 3);

    let third = kg::sweep(&f.db, &f.kb, &opts).await.unwrap();
    assert_eq!(third.documents, 0, "{third:?}");
    assert_eq!(third.ledger_hits, 3, "{third:?}");
    assert_eq!(scalar(&f.db, "SELECT COUNT(*) FROM entities").await, 3);
}

#[tokio::test]
async fn a_sweep_skips_a_document_row_whose_file_is_gone() {
    let f = fixture("ghost").await;
    f.db.call_flat(|conn| {
        conn.execute(
            "INSERT INTO documents (name, source, content_hash, chunk_count, created_at)
             VALUES ('ghost', NULL, 'deadbeef', 0, '2026-01-01T00:00:00Z')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let report = kg::sweep(&f.db, &f.kb, &IngestOptions::default())
        .await
        .unwrap();

    assert_eq!(report.documents, 0, "{report:?}");
    assert_eq!(report.skipped, 1, "named, counted, not silent: {report:?}");
    assert_eq!(ledger_rows(&f.db).await, 0);
}

#[tokio::test]
async fn a_reverted_revision_does_not_duplicate_facts_or_entities() {
    let f = fixture("reverted").await;
    f.kb.save("doc-a", DOC_A).await.unwrap();
    kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();
    f.kb.save("doc-a", DOC_B).await.unwrap();
    kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();
    f.kb.save("doc-a", DOC_A).await.unwrap();
    let back = kg::ingest_document(&f.db, &f.kb, "doc-a", &IngestOptions::default())
        .await
        .unwrap();

    assert_eq!(back.documents, 1, "{back:?}");
    let names = entity_names(&f.db).await;
    assert_eq!(names, vec!["alpha", "beta"], "{names:?}");
    let live = live_edges(&f.db).await;
    assert_eq!(live.len(), 1, "one live claim, not two: {live:?}");
    assert!(
        live[0].3.contains("Beta"),
        "the reverted text is the live claim: {live:?}"
    );
}

#[tokio::test]
async fn both_name_spellings_resolve_to_one_document() {
    let f = fixture("names").await;
    f.kb.save("notes", DOC_A).await.unwrap();

    let by_stem = kg::ingest_document(&f.db, &f.kb, "notes", &IngestOptions::default())
        .await
        .unwrap();
    assert_eq!(by_stem.documents, 1, "{by_stem:?}");
    let by_file = kg::ingest_document(&f.db, &f.kb, "notes.md", &IngestOptions::default())
        .await
        .unwrap();
    assert_eq!(by_file.ledger_hits, 1, "{by_file:?}");
    assert_eq!(ledger_rows(&f.db).await, 1);
}

#[tokio::test]
async fn an_unknown_document_is_an_error_that_names_it() {
    let f = fixture("unknown").await;
    let err = kg::ingest_document(&f.db, &f.kb, "nope", &IngestOptions::default())
        .await
        .expect_err("an unknown document is not a silent success");
    let msg = format!("{err:#}");
    assert!(msg.contains("nope"), "{msg}");
    assert_eq!(ledger_rows(&f.db).await, 0);
}

// ---------------------------------------------------------------------------
// HTTP half: the capability gate on the route.
// ---------------------------------------------------------------------------

struct TestDaemon {
    url: String,
    root: std::path::PathBuf,
    state: AppState,
}

/// Build a `[capabilities]` plane from policy TOML text — the same door the
/// config loader uses (`CapabilityPlane::from_policy`), constructed EXPLICITLY
/// because the boot install of the live plane belongs to the integration task
/// (§17.8).
fn plane(text: &str) -> CapabilityPlane {
    let policy = ruagent_policy::PolicyConfig::parse(text).expect("policy text parses");
    CapabilityPlane::from_policy(&policy).expect("plane builds")
}

/// A real in-test daemon on loopback, with ONE disabled mock agent card: no
/// harness can run even if something tried to spawn one.
async fn start_test_daemon(tag: &str) -> TestDaemon {
    let root = std::env::temp_dir().join(format!(
        "ruagent-t4-graphingest-http-{}-{tag}",
        std::process::id()
    ));
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
    let knowledge = Knowledge::open(&root, db.clone()).await.unwrap();
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
    let state = AppState {
        mgr,
        config: Arc::new(cfg),
        knowledge: Arc::new(knowledge),
        chats,
        sessions: Arc::new(ruagent_daemon::sessions::SessionIndexer::new(
            db.clone(),
            std::env::temp_dir(),
        )),
    };
    let app = ruagent_daemon::api::router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    TestDaemon {
        url: format!("http://{addr}"),
        root,
        state,
    }
}

async fn db_of(state: &AppState) -> Db {
    state.mgr.db().clone()
}

async fn http_json(
    http: &reqwest::Client,
    method: reqwest::Method,
    url: String,
    body: Option<serde_json::Value>,
) -> (reqwest::StatusCode, serde_json::Value) {
    let mut req = http.request(method, url);
    if let Some(body) = body {
        req = req.json(&body);
    }
    let resp = req.send().await.unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    let value = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, value)
}

/// ACCEPTANCE #1: with the default configuration — no `[capabilities]` table,
/// so `knowledge_ingest_graph` is off — ingesting a knowledge document writes
/// the markdown file and its chunks and stops. No entity, no relation, no
/// ledger row, and no `distill_log` row: this path cannot have invoked a model
/// or spawned an agent.
#[tokio::test]
async fn with_the_capability_off_the_knowledge_base_behaves_exactly_as_today() {
    let td = start_test_daemon("off").await;
    let db = db_of(&td.state).await;
    let http = reqwest::Client::new();

    let (status, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/ingest", td.url),
        Some(serde_json::json!({ "name": "doc-a", "content": DOC_A })),
    )
    .await;
    assert_eq!(status, 200, "{body:?}");
    assert!(
        body["chunks"].as_i64().unwrap_or(0) >= 1,
        "the file is indexed as today: {body:?}"
    );
    assert_eq!(body["file"].as_str().unwrap(), "doc-a.md");
    assert!(td.root.join("knowledge").join("doc-a.md").is_file());

    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entities").await, 0);
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entity_edges").await, 0);
    assert_eq!(ledger_rows(&db).await, 0, "the ledger is untouched");
    assert_eq!(
        scalar(&db, "SELECT COUNT(*) FROM distill_log").await,
        0,
        "no distillation attempt was recorded -- nothing was invoked"
    );

    // A REAL ingest through the explicit route is refused, loudly, naming the
    // capability and both remedies.
    let (status, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/graph/ingest?document=doc-a", td.url),
        None,
    )
    .await;
    assert_eq!(status, 409, "{body:?}");
    let msg = body.to_string();
    assert!(msg.contains("knowledge_ingest_graph"), "{msg}");
    assert!(msg.contains("dry_run"), "names the pricing remedy: {msg}");
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entities").await, 0);
    assert_eq!(ledger_rows(&db).await, 0);
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM distill_log").await, 0);

    // dry_run is ALWAYS allowed: it prices the ingestion without writing.
    let (status, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!(
            "{}/api/v1/knowledge/graph/ingest?document=doc-a&dry_run=true",
            td.url
        ),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body:?}");
    assert_eq!(body["ingest"]["dry_run"], serde_json::Value::Bool(true));
    assert_eq!(body["ingest"]["documents"].as_i64(), Some(0));
    assert!(
        body["ingest"]["candidates"].as_i64().unwrap_or(0) >= 3,
        "the price: {body:?}"
    );
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entities").await, 0);
    assert_eq!(ledger_rows(&db).await, 0);

    // The status route answers the question and writes nothing.
    let (status, body) = http_json(
        &http,
        reqwest::Method::GET,
        format!("{}/api/v1/knowledge/graph/ingest/status", td.url),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body:?}");
    assert_eq!(body["ledger"]["documents"].as_i64(), Some(0));
}

/// ACCEPTANCE #2: with the capability ENABLED the same document's entities and
/// relations land in the graph, and re-ingesting it writes nothing.
#[tokio::test]
async fn with_the_capability_on_the_route_ingests_and_is_idempotent() {
    let td = start_test_daemon("on").await;
    td.state.chats.set_capabilities(plane(
        "[capabilities.knowledge_ingest_graph]\nenabled = true\n",
    ));
    let db = db_of(&td.state).await;
    let http = reqwest::Client::new();

    let (_, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/ingest", td.url),
        Some(serde_json::json!({ "name": "doc-a", "content": DOC_A })),
    )
    .await;
    assert!(body["chunks"].as_i64().unwrap_or(0) >= 1, "{body:?}");

    let (status, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/graph/ingest?document=doc-a", td.url),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body:?}");
    assert_eq!(body["ingest"]["dry_run"], serde_json::Value::Bool(false));
    assert_eq!(body["ingest"]["documents"].as_i64(), Some(1));
    assert!(
        body["ingest"]["entities"].as_i64().unwrap_or(0) >= 2,
        "{body:?}"
    );
    let entities = scalar(&db, "SELECT COUNT(*) FROM entities").await;
    let edges = scalar(&db, "SELECT COUNT(*) FROM entity_edges").await;
    assert!(entities >= 2, "{entities}");
    assert!(edges >= 1, "{edges}");
    assert_eq!(ledger_rows(&db).await, 1);
    assert_eq!(
        scalar(&db, "SELECT COUNT(*) FROM distill_log").await,
        0,
        "the ingest path spends no tokens: it never runs a distillation"
    );

    // Re-ingest the unchanged document: nothing is written, and the route says
    // so through `ledger_hits`.
    let (status, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/graph/ingest?document=doc-a", td.url),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body:?}");
    assert_eq!(body["ingest"]["documents"].as_i64(), Some(0));
    assert_eq!(body["ingest"]["ledger_hits"].as_i64(), Some(1));
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entities").await, entities);
    assert_eq!(
        scalar(&db, "SELECT COUNT(*) FROM entity_edges").await,
        edges
    );

    // The status route now reports the document as ingested.
    let (_, body) = http_json(
        &http,
        reqwest::Method::GET,
        format!("{}/api/v1/knowledge/graph/ingest/status?limit=5", td.url),
        None,
    )
    .await;
    assert_eq!(body["ledger"]["documents"].as_i64(), Some(1));
    assert_eq!(body["ledger"]["last"][0]["document_name"], "doc-a");
}

/// The sweep is the route's no-`document` form, and it is bounded by the
/// capability's `max_docs_per_pass` — the "cost-controllable catch-up" of §13.2.
#[tokio::test]
async fn a_sweep_through_the_route_honours_max_docs_per_pass() {
    let td = start_test_daemon("sweep").await;
    td.state.chats.set_capabilities(plane(
        "[capabilities.knowledge_ingest_graph]\nenabled = true\nmax_docs_per_pass = 1\n",
    ));
    let db = db_of(&td.state).await;
    let http = reqwest::Client::new();

    for name in ["one", "two"] {
        let (_, body) = http_json(
            &http,
            reqwest::Method::POST,
            format!("{}/api/v1/knowledge/ingest", td.url),
            Some(serde_json::json!({ "name": name, "content": format!("# {name}\n") })),
        )
        .await;
        assert!(body["chunks"].as_i64().unwrap_or(0) >= 1, "{body:?}");
    }

    let (status, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/graph/ingest", td.url),
        None,
    )
    .await;
    assert_eq!(status, 200, "{body:?}");
    assert_eq!(body["ingest"]["documents"].as_i64(), Some(1), "{body:?}");
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entities").await, 1);

    let (_, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/graph/ingest", td.url),
        None,
    )
    .await;
    assert_eq!(body["ingest"]["documents"].as_i64(), Some(1), "{body:?}");
    assert_eq!(scalar(&db, "SELECT COUNT(*) FROM entities").await, 2);

    let (_, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/graph/ingest", td.url),
        None,
    )
    .await;
    assert_eq!(body["ingest"]["documents"].as_i64(), Some(0), "{body:?}");
    assert_eq!(body["ingest"]["ledger_hits"].as_i64(), Some(2), "{body:?}");
}

/// A document that does not exist is a 400 that names it — never a silent
/// success (the run-cost of §13 is not an excuse for a silent default).
#[tokio::test]
async fn an_unknown_document_is_refused_by_the_route() {
    let td = start_test_daemon("unknown").await;
    td.state.chats.set_capabilities(plane(
        "[capabilities.knowledge_ingest_graph]\nenabled = true\n",
    ));
    let http = reqwest::Client::new();

    let (status, body) = http_json(
        &http,
        reqwest::Method::POST,
        format!("{}/api/v1/knowledge/graph/ingest?document=nope", td.url),
        None,
    )
    .await;

    assert_eq!(status, 400, "{body:?}");
    assert!(body.to_string().contains("nope"), "{body:?}");
}

/// `CapabilityId::KnowledgeIngestGraph` is the id both the refusal and the
/// success path name; pinned so a rename cannot silently orphan the route.
#[test]
fn the_gate_is_the_id_the_route_names() {
    assert_eq!(
        CapabilityId::KnowledgeIngestGraph.as_str(),
        "knowledge_ingest_graph"
    );
}
