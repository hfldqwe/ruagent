//! t6: a `recall_log` row that could NOT be written is ANNOUNCED, not dropped in
//! silence.
//!
//! The recall RESPONSE must not change — recording is fire-and-forget by design, and a
//! recall must not fail because its evidence row could not be stored. So the property
//! these tests guard is the ANNOUNCEMENT: if the log line disappears again (the old
//! `let _ = db.call(...)`, whose outer layer only reports a dead writer), the drop goes
//! back to being invisible and these tests fail rather than continuing to pass on the
//! response alone.
//!
//! WHY THIS FAILURE INJECTION. The measured failure is `SQLITE_BUSY` under a write lock
//! held from a second process, which costs a 5s `busy_timeout` wait per round and needs
//! a second process — exercised against a private daemon in the task's evidence. Here an
//! `BEFORE INSERT` trigger that `RAISE(ABORT)`s is the SAME code path with the SAME fix
//! in it (the closure's statement returns `Err`, so the announcement runs), while also
//! letting the test assert that the row count did not move. Deterministic, fast, and no
//! second process.
//!
//! SCOPE NOTE: these tests capture with `tracing::subscriber::set_default`, which is
//! thread-local. The dropped-row line is emitted on the request path (the test's own
//! runtime thread) and is captured. The retention-sweep line is emitted inside the
//! closure, i.e. on the database writer thread, so what is asserted for it here is the
//! NEGATIVE — a failed sweep must not be reported as a lost evidence row — and the line
//! itself is verified against a real daemon, whose subscriber is global.

use std::sync::{Arc, Mutex};

use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_store::Db;

/// Capture `tracing` output so a test can assert that a loss was LOUD.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = CaptureWriter;
    fn make_writer(&'a self) -> Self::Writer {
        CaptureWriter(self.0.clone())
    }
}

impl Capture {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// The daemon's own boot shape (`crates/daemon/tests/knowledge_api.rs`), with the `Db`
/// handle kept so a test can inject a statement failure and read `recall_log` back.
async fn start_test_daemon() -> (String, std::path::PathBuf, Db) {
    static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-t6recall-{}-{seq}", std::process::id()));
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
    (format!("http://{addr}"), root, db)
}

async fn install_trigger(db: &Db, sql: &str) {
    let sql = sql.to_string();
    db.call_flat(move |conn| conn.execute_batch(&sql))
        .await
        .expect("install the injected statement failure");
}

async fn recall_log_count(db: &Db) -> i64 {
    db.call_flat(|conn| conn.query_row("SELECT COUNT(*) FROM recall_log", [], |r| r.get(0)))
        .await
        .expect("count recall_log")
}

/// One recall over HTTP with `tracing` captured for the whole request.
async fn recall_with_capture(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
    cap: &Capture,
) -> (u16, serde_json::Value) {
    let guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(cap.clone())
            .with_ansi(false)
            .finish(),
    );
    let resp = http
        .get(format!("{url}/api/v1/recall"))
        .query(query)
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap();
    drop(guard);
    (status, body)
}

/// The measured loss, made loud: the evidence row cannot be written, the recall still
/// answers 200 with its normal body, and the dropped row is ANNOUNCED with the query it
/// belonged to and the SQL error that stopped it.
#[tokio::test]
async fn a_recall_log_row_that_cannot_be_written_is_announced() {
    let (url, root, db) = start_test_daemon().await;
    let http = reqwest::Client::new();
    install_trigger(
        &db,
        "CREATE TRIGGER t6_refuse_recall_log BEFORE INSERT ON recall_log
         BEGIN SELECT RAISE(ABORT, 't6 injected: recall_log write refused'); END;",
    )
    .await;

    let before = recall_log_count(&db).await;
    let cap = Capture::default();
    let (status, body) = recall_with_capture(
        &http,
        &url,
        &[("q", "t6-announce-me"), ("top_n", "5")],
        &cap,
    )
    .await;
    let after = recall_log_count(&db).await;
    let logs = cap.text();

    // 1. The response is what it has always been: 200, with a normal body.
    assert_eq!(
        status, 200,
        "recording must never break recall (fire-and-forget by design)"
    );
    assert!(body["scoring"]["fusion"].is_string(), "{body:?}");
    assert!(body["memories"].is_array(), "{body:?}");
    assert!(body["knowledge"].is_array(), "{body:?}");

    // 2. The evidence row is GONE: the count does not move.
    assert_eq!(before, 0, "a fresh root starts with an empty log");
    assert_eq!(
        after, before,
        "the row could not land, so recall_log must not grow"
    );

    // 3. ...and the loss is LOUD, naming the call and the real SQL error.
    assert!(
        logs.contains("recall evidence row was NOT recorded"),
        "a dropped evidence row must be announced: {logs}"
    );
    assert!(
        logs.contains("t6 injected: recall_log write refused"),
        "the announcement must carry the SQL error, not just that something failed: {logs}"
    );
    assert!(
        logs.contains("t6-announce-me"),
        "the announcement must identify WHICH recall lost its row: {logs}"
    );
    // 4. The two losses are not conflated.
    assert!(
        !logs.contains("retention sweep failed"),
        "a refused INSERT is not a failed sweep: {logs}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// The success path is unchanged: a writable log writes the same row with the same
/// fields, and says nothing.
#[tokio::test]
async fn a_written_recall_log_row_is_not_announced() {
    let (url, root, db) = start_test_daemon().await;
    let http = reqwest::Client::new();

    let cap = Capture::default();
    let (status, body) = recall_with_capture(
        &http,
        &url,
        &[
            ("q", "t6-happy-path"),
            ("strategy", "conservative"),
            ("top_n", "7"),
            ("source", "probe"),
        ],
        &cap,
    )
    .await;
    let logs = cap.text();

    assert_eq!(status, 200);
    assert!(body["scoring"]["fusion"].is_string(), "{body:?}");
    assert_eq!(
        recall_log_count(&db).await,
        1,
        "a writable log must still record exactly one row per recall"
    );
    assert!(
        !logs.contains("was NOT recorded"),
        "a row that landed must not be announced as lost: {logs}"
    );
    assert!(
        !logs.contains("retention sweep failed"),
        "an uncontended sweep is a no-op, not a failure: {logs}"
    );

    // The row's fields, unchanged: what the caller declared is what the row carries.
    let row: (String, String, i64, Option<String>) = db
        .call_flat(|conn| {
            conn.query_row(
                "SELECT query, strategy, top_n, source FROM recall_log ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
        })
        .await
        .expect("read the row back");
    assert_eq!(row.0, "t6-happy-path");
    assert_eq!(row.1, "conservative");
    assert_eq!(row.2, 7);
    assert_eq!(row.3.as_deref(), Some("probe"));

    let _ = std::fs::remove_dir_all(&root);
}

/// Precision guard: a failed RETENTION SWEEP is not a dropped evidence row. The row
/// landed; only the cap was not enforced. Reporting the two with one message would
/// overstate the hole — and this test fails if the announcement is ever wired to the
/// wrong statement (the sweep's own line is on the writer thread; see the file header).
#[tokio::test]
async fn a_failed_retention_sweep_is_not_reported_as_a_lost_row() {
    let (url, root, db) = start_test_daemon().await;
    let http = reqwest::Client::new();
    install_trigger(
        &db,
        "CREATE TRIGGER t6_refuse_sweep BEFORE DELETE ON recall_log
         BEGIN SELECT RAISE(ABORT, 't6 injected: sweep refused'); END;",
    )
    .await;

    let cap = Capture::default();
    let (status, _body) = recall_with_capture(&http, &url, &[("q", "t6-sweep-only")], &cap).await;
    let logs = cap.text();

    assert_eq!(status, 200);
    assert_eq!(
        recall_log_count(&db).await,
        1,
        "the evidence row landed; only the sweep failed"
    );
    assert!(
        !logs.contains("recall evidence row was NOT recorded"),
        "a landed row must not be announced as lost just because the sweep failed: {logs}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
