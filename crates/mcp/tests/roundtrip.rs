//! MCP roundtrip: a real rmcp client speaks the MCP protocol over an
//! in-process duplex pipe to `PlatformTools`, which calls a real in-test
//! daemon over HTTP — the whole §6.5 seam, protocol-complete.

use std::sync::Arc;

use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};
use ruagent_daemon::api::AppState;
use ruagent_daemon::config::DaemonConfig;
use ruagent_daemon::runs::RunManager;
use ruagent_mcp::{BridgeConfig, PlatformTools};
use ruagent_store::Db;

struct TestClient;

impl ClientHandler for TestClient {
    fn get_info(&self) -> rmcp::model::ClientInfo {
        rmcp::model::ClientInfo::default()
    }
}

/// Extract the text of a CallToolResult.
fn text_of(out: &rmcp::model::CallToolResult) -> String {
    out.content
        .iter()
        .filter_map(|c| match c {
            rmcp::model::ContentBlock::Text(t) => Some(t.text.to_string()),
            _ => None,
        })
        .collect()
}

fn call(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    let obj = args.as_object().cloned().unwrap_or_default();
    CallToolRequestParams::new(name.to_string()).with_arguments(obj)
}

async fn start_test_daemon() -> String {
    start_test_daemon_with_policy("[permissions]\ndefault = \"ask\"\n")
        .await
        .0
}

/// The daemon PLUS the config root it serves, so a test can read `policy.toml`
/// itself: for a capability write the FILE is the witness, not the tool's reply.
async fn start_test_daemon_with_policy(policy_toml: &str) -> (String, std::path::PathBuf) {
    static DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("ruagent-mcp-e2e-{}-{seq}", std::process::id()));
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
        ruagent_daemon::distill::AutoDistill::default(),
        None,
        ruagent_daemon::distill::AgentRegistry::default(),
    );
    // t10: the LIVE-PLANE INSTALL the real `serve` performs (`daemon/src/lib.rs`).
    // Without it this harness's plane is `legacy()` no matter what `policy.toml`
    // says, so `/api/v1/capabilities` would describe a config nobody has: a
    // `[capabilities]` table seeded into the file below would read as ABSENT.
    chats.set_capabilities(cfg.capabilities.clone());
    let mgr = Arc::new(RunManager::new(
        db.clone(),
        root.clone(),
        agents,
        cfg.policy.to_policy(),
        cfg.mcp.clone(),
    ));
    mgr.set_capabilities(chats.capabilities_handle());
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

#[tokio::test]
async fn mcp_tools_reach_the_daemon_memory_and_knowledge() {
    let daemon_url = start_test_daemon().await;

    // Wire an rmcp client to the platform server over a duplex pipe.
    // Both sides are spawned (their service loops drive themselves).
    let url_for_server = daemon_url.clone();
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        PlatformTools::new(BridgeConfig {
            daemon_url: url_for_server,
        })
        .serve(server_transport)
        .await
        .expect("mcp server starts")
    });

    let client = TestClient.serve(client_transport).await.unwrap();

    // List tools: the platform surface.
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    // t93 / audit M5: this used to be `any()` over 9 of the 14 names, so deleting
    // any of the five tools this generation added (memory_forget_report,
    // graph_search, graph_retrieve, wiki_pages, wiki_links) stayed green. Set
    // EQUALITY (both directions) plus a count is what makes removal loud.
    // t6 (wave C) added the capability plane's four tools. This vector is the
    // ONLY place the surface is counted: set equality (both directions) plus the
    // count is what makes adding, renaming or removing a tool loud.
    let mut expected = vec![
        "memory_search",
        "memory_write",
        "memory_recall",
        "memory_get",
        "knowledge_search",
        "knowledge_ingest",
        "knowledge_expand",
        "graph_entity",
        "memory_forget_report",
        "graph_search",
        "graph_retrieve",
        "wiki_pages",
        "wiki_links",
        "list_tasks",
        "capabilities_list",
        "capability_set",
        "distill_session",
        "knowledge_graph_ingest",
        // t12 (issues #51, #54): the fact write, the ledger read, and the pending
        // queue's decision sheet.
        "graph_fact_add",
        "distill_status",
        "graph_resolution_pending",
    ];
    assert_eq!(
        expected.len(),
        21,
        "the declared consumption surface is 21 tools"
    );
    let mut got = names.clone();
    got.sort();
    expected.sort();
    assert_eq!(
        got, expected,
        "the MCP surface changed: missing or extra tools. got={names:?}"
    );

    // Write a memory through the MCP tool.
    let out = client
        .call_tool(call(
            "memory_write",
            serde_json::json!({
                "store": "observation",
                "namespace": "user",
                "content": "the user prefers rust over javascript"
            }),
        ))
        .await
        .unwrap();
    assert!(text_of(&out).contains("Inserted"), "{out:?}");

    // Ingest a document into the knowledge base.
    let out = client
        .call_tool(call(
            "knowledge_ingest",
            serde_json::json!({
                "name": "design-notes",
                "content": "The deploy script lives in scripts/release.sh. Run it from the repository root."
            }),
        ))
        .await
        .unwrap();
    assert!(text_of(&out).contains("1"), "one chunk ingested: {out:?}");

    // Search memories via MCP.
    let out = client
        .call_tool(call(
            "memory_search",
            serde_json::json!({ "query": "rust" }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("rust over javascript"),
        "{}",
        text_of(&out)
    );

    // Search knowledge via MCP.
    let out = client
        .call_tool(call(
            "knowledge_search",
            serde_json::json!({ "query": "release script" }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("scripts/release.sh"),
        "{}",
        text_of(&out)
    );

    // Expand a hit into its parent section via MCP: the chunk id comes
    // off the daemon's HTTP surface.
    let http = reqwest::Client::new();
    let docs: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/documents"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let doc_id = docs["documents"]
        .as_array()
        .and_then(|d| d.first())
        .and_then(|d| d["id"].as_i64())
        .expect("ingested document listed");
    let chunks: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/knowledge/documents/{doc_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let chunk_id = chunks["chunks"]
        .as_array()
        .and_then(|c| c.first())
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|id| id.as_i64())
        .expect("document has chunks");
    let out = client
        .call_tool(call(
            "knowledge_expand",
            serde_json::json!({ "chunk_id": chunk_id }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("release.sh"),
        "expansion carries the section: {}",
        text_of(&out)
    );

    // Platform ops: create a task via REST and list it via MCP.
    let http = reqwest::Client::new();
    http.post(format!("{daemon_url}/api/v1/tasks"))
        .json(&serde_json::json!({"title": "mcp roundtrip", "intent": "x"}))
        .send()
        .await
        .unwrap();
    let out = client
        .call_tool(call("list_tasks", serde_json::json!({})))
        .await
        .unwrap();
    assert!(text_of(&out).contains("mcp roundtrip"), "{}", text_of(&out));

    client.cancel().await.unwrap();
    let server = server_task.await.unwrap();
    server.cancel().await.unwrap();
}

/// issue #56 / t2: a REFUSED store×namespace pair is an ERROR to the MCP caller,
/// carrying the daemon's own sentence — never a success envelope — and NOTHING is
/// written. The allowed pairs keep the receipt they had.
///
/// BOTH branches are measured through the real seam: the daemon's 409 body is read
/// off HTTP in the same test so the tool's failure can be compared against it
/// VERBATIM (a paraphrase would pass a substring test written only from the tool
/// side), and the tool runs over an in-process rmcp pipe against the real routes.
/// `profile` × `global` is outside the store's write vocabulary
/// (`MemoryStore::allowed_kinds`), while `observation` × `user` is inside it.
#[tokio::test]
async fn a_refused_memory_write_fails_with_the_daemons_own_words_through_mcp() {
    let (daemon_url, root) =
        start_test_daemon_with_policy("[permissions]\ndefault = \"ask\"\n").await;
    let http = reqwest::Client::new();
    let (client, server_task) = mcp_pair(&daemon_url).await;

    let refused_args = serde_json::json!({
        "store": "profile",
        "namespace": "global",
        "content": "t2 the owner prefers tea to coffee",
    });
    let before = count_rows(&root, "SELECT COUNT(*) FROM memories").await;

    // -- (1) the daemon's OWN answer, read off the HTTP route the tool calls: the
    //        status that makes the refusal a failure, and the words it must carry.
    let raw = http
        .post(format!("{daemon_url}/api/v1/memory/write"))
        .json(&refused_args)
        .send()
        .await
        .unwrap();
    assert_eq!(
        raw.status().as_u16(),
        409,
        "a store×namespace refusal must not answer 2xx (issue #56)"
    );
    let body: serde_json::Value = raw.json().await.unwrap();
    let daemon_sentence = body["error"]
        .as_str()
        .expect("the refusal body carries `error`")
        .to_string();
    assert_eq!(body["store"], "profile", "{body}");
    assert_eq!(body["namespace"], "global", "{body}");
    assert_eq!(
        body["allowed_namespaces"],
        serde_json::json!(["user"]),
        "the refusal publishes the store's own vocabulary: {body}"
    );

    // -- (2) the same call through the MCP tool: an ERROR, carrying that sentence.
    let seen = failure(client.call_tool(call("memory_write", refused_args)).await);
    assert!(
        seen.contains(&daemon_sentence),
        "the daemon's own sentence must reach the caller verbatim: {seen}"
    );
    assert!(seen.contains("HTTP 409"), "{seen}");
    assert!(
        seen.contains("`profile`") && seen.contains("`global`"),
        "the refusal names the store and the namespace: {seen}"
    );
    assert!(
        seen.contains("user"),
        "and what that store MAY write, so the caller can retry: {seen}"
    );
    // The false success this task exists to remove.
    assert!(
        !seen.starts_with("memory write: "),
        "a refusal must not wear the receipt: {seen}"
    );
    assert!(
        !seen.contains("Inserted") && !seen.contains("SkippedDuplicate"),
        "{seen}"
    );

    // -- (3) NOTHING WAS WRITTEN. The table is the witness, not the sentence.
    assert_eq!(
        count_rows(&root, "SELECT COUNT(*) FROM memories").await,
        before,
        "a refused write must not insert a row"
    );

    // -- (4) the ALLOWED branch: same tool, same receipt key and shape as before.
    let allowed = serde_json::json!({
        "store": "observation",
        "namespace": "user",
        "content": "t2 the owner prefers tea to coffee",
    });
    let ok = text_of(
        &client
            .call_tool(call("memory_write", allowed.clone()))
            .await
            .unwrap(),
    );
    assert!(
        ok.starts_with("memory write: Inserted("),
        "the receipt shape and key are unchanged: {ok}"
    );
    assert_eq!(
        count_rows(&root, "SELECT COUNT(*) FROM memories").await,
        before + 1,
        "an allowed pair writes exactly one row: {ok}"
    );
    let dup = text_of(
        &client
            .call_tool(call("memory_write", allowed))
            .await
            .unwrap(),
    );
    assert!(
        dup.starts_with("memory write: SkippedDuplicate("),
        "a duplicate is still a success, and it says which row it is: {dup}"
    );

    // -- (5) the tool description no longer claims a vocabulary the daemon owns: it
    //        used to say "Namespaces: user, global, project:<name>, agent:<name>",
    //        which reads as `global` being writable everywhere.
    let tools = client.list_all_tools().await.unwrap();
    let described = tools
        .iter()
        .find(|t| t.name == "memory_write")
        .map(|t| t.description.clone().unwrap_or_default())
        .expect("memory_write is on the surface");
    assert!(
        !described.contains("Namespaces:"),
        "the description must not restate the write matrix: {described}"
    );
    assert!(
        described.contains("allowed_namespaces"),
        "it must say the REFUSAL names what the store may write: {described}"
    );

    shutdown_pair(client, server_task).await;
}

/// THE NEGATIVE CONTROL for the test above (issue #56) — and nothing else.
///
/// It is not there to bless a behaviour: it exists so that "the refusal reaches the
/// caller as an error" cannot be a vacuous claim. The pre-#56 daemon answered a
/// refused pair `200 {"outcome":"RejectedNamespace (...)"}`, and under THAT body no
/// bridge can tell a refusal from a write — the caller sees a receipt. So this test
/// serves the old body from a stub and shows the receipt, which is the distinction
/// the fix bought: the tool's honesty is bounded by the status the daemon sends, and
/// the status is what changed (`crates/daemon/src/api.rs`, t1).
///
/// The control is what makes the assertions above load-bearing: on THIS result
/// [`failure`] panics instead of passing (its own `Ok(r) => panic!(...)` arm), so the
/// test above can only be green because the daemon now answers 409 — not because the
/// check it uses is lenient.
#[tokio::test]
async fn the_pre_56_200_refusal_body_would_have_read_as_a_receipt() {
    let (url, stub) = stub_http_server(
        "200 OK",
        &serde_json::json!({
            "outcome": "RejectedNamespace (this store does not allow that namespace; write \
                        vocabulary = profile: user; observation: user, project:<x>, agent:<x>; \
                        procedure: project:<x>, global; lesson: project:<x>, global)",
        })
        .to_string(),
    )
    .await;
    let (client, server_task) = mcp_pair(&url).await;

    let out = client
        .call_tool(call(
            "memory_write",
            serde_json::json!({
                "store": "profile",
                "namespace": "global",
                "content": "t2 control: this pair is refused by the daemon",
            }),
        ))
        .await;
    // 200 + that body -> a SUCCESS to the caller. Not endorsed: this is the shape the
    // bridge cannot see through, and the reason #56 is a write-side issue.
    let seen = text_of(&out.expect("a 200 is a success as far as the bridge can tell"));
    assert!(
        seen.starts_with("memory write: RejectedNamespace"),
        "the old body arrives as a receipt, which is the false success the test above \
         must catch: {seen}"
    );
    assert!(!seen.contains("refused by the ruagent daemon"), "{seen}");

    shutdown_pair(client, server_task).await;
    stub.abort();
}

/// A stub HTTP server answering EVERY request with `status` and `body` (one
/// connection per request), so a tool can be pointed at a "daemon" that answers
/// exactly what the test needs and nothing else. Returns the base URL and the
/// accept loop's handle.
async fn stub_http_server(status: &str, body: &str) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let status = status.to_string();
    let body = body.to_string();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let status = status.clone();
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 8192];
                // The request body is irrelevant: every call gets the same answer.
                let _ = sock.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(body.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (format!("http://{addr}"), handle)
}

/// Seed SQL through a SECOND connection to the test daemon's own database.
///
/// WHY A DIRECT INSERT IS LEGITIMATE HERE (t12): the two facts the tests below need
/// -- "this session's ledger holds these attempts" and "the queue holds this pair" --
/// are written by paths this crate does not drive (distill.rs's ledger writes, and the
/// mechanical resolution pass that queues pairs), and both live in crates/daemon,
/// outside this task. What IS under test is the READ: the tool, the route and the
/// table. A row that failed to land would show up as a wrong reading, not as a silent
/// pass, because every assertion below names a value it expects to read back.
async fn seed(root: &std::path::Path, sql: &str) {
    let db = Db::open(root.join("data").join("ruagent.db")).unwrap();
    let sql = sql.to_string();
    db.call_flat(move |conn| conn.execute_batch(&sql))
        .await
        .unwrap();
}

/// ONE count from the test daemon's own database, through a second connection.
///
/// The witness for "nothing was written" has to be the TABLE: a refusal's own
/// sentence says NOTHING WAS WRITTEN, and a test that only reads that sentence
/// would pass on a daemon that wrote the row and said otherwise.
async fn count_rows(root: &std::path::Path, sql: &str) -> i64 {
    let db = Db::open(root.join("data").join("ruagent.db")).unwrap();
    let sql = sql.to_string();
    db.call_flat(move |conn| conn.query_row(&sql, [], |r| r.get(0)))
        .await
        .unwrap()
}

async fn create_entity(http: &reqwest::Client, daemon_url: &str, name: &str) -> i64 {
    let v: serde_json::Value = http
        .post(format!("{daemon_url}/api/v1/graph/entity"))
        .json(&serde_json::json!({ "name": name, "kind": "concept" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v["id"].as_i64().expect("the entity route returns an id")
}

/// t12 / #51, acceptance 1: a fact written through MCP is readable BY THE SAME
/// SESSION through graph_entity.
///
/// The entities are created through the daemon's own route on purpose: the fact tool
/// never invents one (t1), so a fixture that pre-seeded them would hide the very
/// behaviour this asserts.
#[tokio::test]
async fn a_fact_written_through_mcp_is_readable_by_graph_entity_in_the_same_session() {
    let daemon_url = start_test_daemon().await;
    let (client, server_task) = mcp_pair(&daemon_url).await;
    let http = reqwest::Client::new();
    let src = create_entity(&http, &daemon_url, "t12-atlas-release").await;
    let dst = create_entity(&http, &daemon_url, "t12-friday-shipment").await;

    let out = client
        .call_tool(call(
            "graph_fact_add",
            serde_json::json!({
                "src_name": "t12-atlas-release",
                "dst_name": "t12-friday-shipment",
                "relation": "ships_on",
                "fact_text": "the atlas release train ships on Fridays"
            }),
        ))
        .await
        .unwrap();
    let written = text_of(&out);
    assert!(
        written.contains("event_time_source=recorded"),
        "the write must report the event-time source the route recorded: {written}"
    );
    assert!(
        written.contains(&format!("READ IT BACK with graph_entity(id: {src})")),
        "the reply must name the id to read back: {written}"
    );

    // The SAME session reads it back, by id, through the read tool.
    let out = client
        .call_tool(call("graph_entity", serde_json::json!({ "id": src })))
        .await
        .unwrap();
    let read = text_of(&out);
    assert!(
        read.contains("the atlas release train ships on Fridays"),
        "the fact written through MCP must be readable by graph_entity: {read}"
    );

    // A name that resolves to nothing is REFUSED, naming the string (t1): this route
    // never creates an entity, so a typo cannot invent a node.
    let refused = failure(
        client
            .call_tool(call(
                "graph_fact_add",
                serde_json::json!({
                    "src": src,
                    "dst_name": "t12-no-such-entity-anywhere",
                    "relation": "runs_on",
                    "fact_text": "x"
                }),
            ))
            .await,
    );
    assert!(
        refused.contains("t12-no-such-entity-anywhere"),
        "the refusal must quote the name that resolved to nothing: {refused}"
    );

    // The same end addressed BOTH ways is refused rather than silently resolved to
    // one of them.
    let refused = failure(
        client
            .call_tool(call(
                "graph_fact_add",
                serde_json::json!({
                    "src": src, "src_name": "t12-atlas-release",
                    "dst": dst, "relation": "ships_on", "fact_text": "x"
                }),
            ))
            .await,
    );
    assert!(
        refused.contains("exactly one of"),
        "an ambiguous end must be refused, not guessed: {refused}"
    );

    shutdown_pair(client, server_task).await;
}

/// t12 / acceptance 2: the distillation ledger, read for ONE session.
///
/// Every reading the contract names for the ledger is asserted here against the
/// DAEMON'S OWN ROUTE (not a stub): the verdict over the whole history, the newest
/// attempt (which may be a later failure), the NULL-is-not-zero rule for every
/// nullable column, and the window/truncation pair.
#[tokio::test]
async fn distill_status_reads_the_ledger_the_daemon_serves() {
    let (daemon_url, root) =
        start_test_daemon_with_policy("[permissions]\ndefault = \"ask\"\n").await;
    seed(
        &root,
        r#"
INSERT INTO distill_log (session_key, distilled_at, status, agent, memories_written,
        entities_written, relations_written, tokens_used, context_size, cost_usd, retries,
        windows, truncated)
VALUES ('ruagent:t12-ok', '2026-10-01T00:00:00Z', 'ok', 'mock', 3, 1, 2, 1234, 200000,
        0.0012, 0, 1, 0);
INSERT INTO distill_log (session_key, distilled_at, status, failure_reason, agent)
VALUES ('ruagent:t12-ok', '2026-10-02T00:00:00Z', 'failed',
        'no content to distil: the session has no user messages', '');
INSERT INTO distill_log (session_key, distilled_at) VALUES ('ruagent:t12-legacy',
        '2026-09-01T00:00:00Z');
INSERT INTO distill_log (session_key, distilled_at, status, windows, truncated)
VALUES ('ruagent:t12-cut', '2026-10-03T00:00:00Z', 'ok', 3, 1);
"#,
    )
    .await;
    let mut many = String::new();
    for i in 0..12 {
        many.push_str(&format!(
            "INSERT INTO distill_log (session_key, distilled_at, status) VALUES \
             ('ruagent:t12-many', '2026-10-{i:02}T00:00:00Z', 'ok');\n"
        ));
    }
    seed(&root, &many).await;

    let (client, server_task) = mcp_pair(&daemon_url).await;

    // 1. A session with a successful attempt AND a later failure: DISTILLED (the
    //    verdict is over the whole history), newest attempt = the failure, and the
    //    cumulative totals span BOTH rows.
    let out = client
        .call_tool(call(
            "distill_status",
            serde_json::json!({ "session_key": "ruagent:t12-ok" }),
        ))
        .await
        .unwrap();
    let ok = text_of(&out);
    assert!(ok.contains("DISTILLED"), "{ok}");
    assert!(
        ok.contains("attempts recorded for this session: 2") && ok.contains("failed=1"),
        "the three-state distribution must span the history: {ok}"
    );
    assert!(
        ok.contains("status=failed"),
        "the newest attempt is the LATER failure, and the verdict is not: {ok}"
    );
    assert!(
        ok.contains("failure reason: no content to distil"),
        "the failure reason must be readable: {ok}"
    );
    assert!(
        ok.contains("tokens 1234"),
        "the cumulative sum spans the whole history, not the page: {ok}"
    );
    assert!(ok.contains("$0.0012"), "{ok}");
    assert!(
        ok.contains("1 of 2 attempt(s) did not record one"),
        "the count of unrecorded rows is the other half of the sum: {ok}"
    );

    // 2. A pre-0028 row: NULL status is the ledger's own word, and every nullable
    //    column says NOT RECORDED instead of 0.
    let out = client
        .call_tool(call(
            "distill_status",
            serde_json::json!({ "session_key": "ruagent:t12-legacy" }),
        ))
        .await
        .unwrap();
    let legacy = text_of(&out);
    assert!(legacy.contains("NOT DISTILLED"), "{legacy}");
    assert!(
        legacy.contains("status=not_recorded (NULL status"),
        "a NULL status is the ledger's fourth state, not a fourth outcome: {legacy}"
    );
    assert!(
        legacy.contains("tokens NOT RECORDED"),
        "an unrecorded column must not print as 0: {legacy}"
    );
    assert!(
        legacy.contains("windows/truncated NOT RECORDED"),
        "neither window nor truncation was measured: {legacy}"
    );

    // 3. A LOSSY transcript: windows and truncated are read together, so "3 windows"
    //    never reads as "nothing lost".
    let out = client
        .call_tool(call(
            "distill_status",
            serde_json::json!({ "session_key": "ruagent:t12-cut" }),
        ))
        .await
        .unwrap();
    let cut = text_of(&out);
    assert!(cut.contains("windows=3, truncated=YES"), "{cut}");
    assert!(
        cut.contains("text was DROPPED"),
        "a cut that lost text must say so: {cut}"
    );

    // 4. A session the ledger has never heard of is NOT DISTILLED, and the reply
    //    says the ledger holds no row rather than inventing a reason.
    let out = client
        .call_tool(call(
            "distill_status",
            serde_json::json!({ "session_key": "ruagent:t12-none" }),
        ))
        .await
        .unwrap();
    let none = text_of(&out);
    assert!(none.contains("NOT DISTILLED"), "{none}");
    assert!(
        none.contains("no attempt recorded for this session"),
        "{none}"
    );
    assert!(
        none.contains("distill_session"),
        "the reply must name the free way to distil it: {none}"
    );

    // 5. A session with more attempts than the page: the remainder is STATED (the
    //    limit is this tool's, and it says what it left out).
    let out = client
        .call_tool(call(
            "distill_status",
            serde_json::json!({ "session_key": "ruagent:t12-many", "limit": 2 }),
        ))
        .await
        .unwrap();
    let many = text_of(&out);
    assert!(many.contains("shown 2 of 12"), "{many}");
    assert!(many.contains("还有 10 条未显示"), "{many}");

    shutdown_pair(client, server_task).await;
}

/// t12 / acceptance 3: the pending queue is listed AND both decisions are recorded,
/// with the SAME semantics the HTTP route has (t10 / issue #50).
#[tokio::test]
async fn the_pending_queue_is_listed_and_decided_through_mcp() {
    let (daemon_url, root) =
        start_test_daemon_with_policy("[permissions]\ndefault = \"ask\"\n").await;
    let http = reqwest::Client::new();
    let a1 = create_entity(&http, &daemon_url, "t12-kubernetes").await;
    let b1 = create_entity(&http, &daemon_url, "t12-k8s").await;
    let a2 = create_entity(&http, &daemon_url, "t12-postgres").await;
    let b2 = create_entity(&http, &daemon_url, "t12-postgresql").await;
    // The queue is filled by the mechanical pass (crates/daemon), which this crate
    // does not drive; the rows are seeded with the schema's own columns.
    seed(
        &root,
        &format!(
            "INSERT INTO resolution_pending (entity_a, entity_b, reason, created_at) \
             VALUES ({a1}, {b1}, 'names differ by a suffix', '2026-10-01T00:00:00Z');\n\
             INSERT INTO resolution_pending (entity_a, entity_b, reason, created_at) \
             VALUES ({a2}, {b2}, 'same product, two spellings', '2026-10-01T00:00:00Z');\n"
        ),
    )
    .await;

    let (client, server_task) = mcp_pair(&daemon_url).await;

    // 1. The decision sheet: both pairs, both entities, both fact counts, and the
    //    exact call that records each decision.
    let out = client
        .call_tool(call("graph_resolution_pending", serde_json::json!({})))
        .await
        .unwrap();
    let listed = text_of(&out);
    assert!(
        listed.contains("merge decisions waiting: 2 pair(s)"),
        "{listed}"
    );
    assert!(
        listed.contains("t12-kubernetes") && listed.contains("t12-k8s"),
        "{listed}"
    );
    assert!(
        listed.contains("queued because: names differ by a suffix"),
        "{listed}"
    );
    assert!(
        listed.contains(&format!("keeper: {a1}, absorbed: {b1}")),
        "the reply must carry the exact merge call: {listed}"
    );
    assert!(
        listed.contains(&format!("a: {a1}, b: {b1}")),
        "the reply must carry the exact dismiss call: {listed}"
    );
    assert!(
        listed.contains("NO LIMIT WAS APPLIED"),
        "the whole queue is returned, and the reply says so: {listed}"
    );

    // 2. Keep this pair apart, twice: the repeat is the same decision, not an error.
    let out = client
        .call_tool(call(
            "graph_resolution_pending",
            serde_json::json!({ "action": "dismiss", "a": a1, "b": b1 }),
        ))
        .await
        .unwrap();
    let dismissed = text_of(&out);
    assert!(dismissed.contains("STAY SEPARATE"), "{dismissed}");
    let out = client
        .call_tool(call(
            "graph_resolution_pending",
            serde_json::json!({ "action": "dismiss", "a": b1, "b": a1 }),
        ))
        .await
        .unwrap();
    let again = text_of(&out);
    assert!(
        again.contains("already recorded"),
        "the unordered pair repeats idempotently: {again}"
    );

    // 3. The queue is now one pair smaller, and the answered pair is COUNTED rather
    //    than merely gone: "empty" and "worked through" are different readings.
    let out = client
        .call_tool(call("graph_resolution_pending", serde_json::json!({})))
        .await
        .unwrap();
    let after_dismiss = text_of(&out);
    assert!(
        after_dismiss.contains("merge decisions waiting: 1 pair(s)"),
        "{after_dismiss}"
    );
    assert!(
        after_dismiss.contains("1 pair(s) already"),
        "{after_dismiss}"
    );

    // 4. Merge the other pair: the keeper absorbs it, the absorbed name becomes an
    //    alias (so the next pass cannot rebuild it), and the pair leaves the queue.
    let out = client
        .call_tool(call(
            "graph_resolution_pending",
            serde_json::json!({ "action": "merge", "keeper": a2, "absorbed": b2 }),
        ))
        .await
        .unwrap();
    let merged = text_of(&out);
    assert!(merged.contains("was absorbed into"), "{merged}");
    assert!(
        merged.contains("is now an ALIAS of the keeper"),
        "the absorbed name must become an alias of the keeper: {merged}"
    );
    // alias_written reports the SECOND write, not whether the alias exists: the merge
    // records it inside its own transaction, so the honest claim is checked against
    // the entity the daemon serves, not against that flag.
    let keeper: serde_json::Value = http
        .get(format!("{daemon_url}/api/v1/graph/entity/{a2}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let aliases: Vec<&str> = keeper["aliases"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert!(
        aliases.contains(&"t12-postgresql"),
        "the absorbed name must resolve to the keeper: {keeper}"
    );
    assert!(
        merged.contains("alias_written="),
        "the reply must still report what the second write did: {merged}"
    );
    let out = client
        .call_tool(call("graph_resolution_pending", serde_json::json!({})))
        .await
        .unwrap();
    let empty = text_of(&out);
    assert!(
        empty.contains("merge decisions waiting: 0 pair(s)"),
        "{empty}"
    );
    assert!(
        empty.contains("nothing to decide"),
        "an empty queue says why it is empty: {empty}"
    );

    // 5. The refusals are the DAEMON'S (one rule, one message) except for the
    //    argument check this tool owns.
    let refused = failure(
        client
            .call_tool(call(
                "graph_resolution_pending",
                serde_json::json!({ "action": "merge", "keeper": a2 }),
            ))
            .await,
    );
    assert!(refused.contains("absorbed"), "{refused}");
    assert!(refused.contains("needs BOTH"), "{refused}");

    let refused = failure(
        client
            .call_tool(call(
                "graph_resolution_pending",
                serde_json::json!({ "action": "reconcile" }),
            ))
            .await,
    );
    assert!(refused.contains("reconcile"), "{refused}");
    assert!(refused.contains("dismiss"), "{refused}");

    let refused = failure(
        client
            .call_tool(call(
                "graph_resolution_pending",
                serde_json::json!({ "action": "dismiss", "a": a1, "b": a1 }),
            ))
            .await,
    );
    assert!(
        refused.contains("same entity"),
        "the daemon's own refusal must reach the caller: {refused}"
    );

    shutdown_pair(client, server_task).await;
}

/// t12 / acceptance 4: a list this tool cut SAYS SO. The old tool printed the first
/// 20 rows of the response and nothing else, so a board with more rows than the page
/// read exactly like a complete one.
#[tokio::test]
async fn a_cut_task_list_says_how_many_rows_it_left_out() {
    let daemon_url = start_test_daemon().await;
    let http = reqwest::Client::new();
    for i in 0..3 {
        http.post(format!("{daemon_url}/api/v1/tasks"))
            .json(&serde_json::json!({
                "title": format!("t12 board row {i}"),
                "intent": "x"
            }))
            .send()
            .await
            .unwrap();
    }
    let (client, server_task) = mcp_pair(&daemon_url).await;

    let out = client
        .call_tool(call("list_tasks", serde_json::json!({ "limit": 2 })))
        .await
        .unwrap();
    let page = text_of(&out);
    assert!(page.contains("t12 board row"), "{page}");
    assert!(
        page.contains("shown 2 of 3 (limit=2, offset=0)"),
        "the envelope's own counts must be printed: {page}"
    );
    assert!(
        page.contains("还有 1 条未显示"),
        "the remainder must be stated, in the acceptance's own words: {page}"
    );

    // The last page is complete, and says THAT as well -- so "no warning" is never
    // the only evidence that nothing was left out.
    let out = client
        .call_tool(call(
            "list_tasks",
            serde_json::json!({ "limit": 2, "offset": 2 }),
        ))
        .await
        .unwrap();
    let last = text_of(&out);
    assert!(last.contains("shown 3 of 3 (limit=2, offset=2)"), "{last}");
    assert!(!last.contains("还有"), "nothing is left out: {last}");
    assert!(
        last.contains("2 earlier row(s) not shown"),
        "a page that starts late says what is behind it: {last}"
    );

    shutdown_pair(client, server_task).await;
}

/// t27 / #54: the MCP search tools carry the DAEMON's remainder over the REAL seam.
///
/// WHY THIS EXISTS (t26's declared residual): t26 fixed the routes -- /graph/search now
/// honours limit, and both it and /memory/search publish matched / matched_exact /
/// truncated -- and pressed the tool end over a stub, but the chain "real daemon reading
/// -> real MCP tool text" was never driven end to end. #54's acceptance is stated at the
/// MCP boundary ("MCP 输出包含剩余条数"), so it can only be proved here: five matching
/// rows per store, one real daemon, one real stdio MCP session.
#[tokio::test]
async fn a_cut_search_page_reports_the_real_remainder_through_mcp() {
    let daemon_url = start_test_daemon().await;
    let http = reqwest::Client::new();
    for i in 0..5 {
        // Through the daemon's OWN write routes: the corpus is real rows, not fixtures
        // poked into the tables. observation x user is one of the allowed pairs (the
        // write vocabulary refuses observation x global).
        let resp = http
            .post(format!("{daemon_url}/api/v1/memory/write"))
            .json(&serde_json::json!({
                "store": "observation",
                "namespace": "user",
                "content": format!("t27 probe memory {i}"),
            }))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success(), "{}", resp.text().await.unwrap());
        create_entity(&http, &daemon_url, &format!("t27 probe entity {i}")).await;
    }

    let (client, server_task) = mcp_pair(&daemon_url).await;

    // 1. REACHED: two rows of five matched, so the remainder is a REAL number.
    for tool in ["graph_search", "memory_search"] {
        let out = client
            .call_tool(call(
                tool,
                serde_json::json!({ "query": "t27 probe", "limit": 2 }),
            ))
            .await
            .unwrap();
        let text = text_of(&out);
        println!("READING t27 {tool} limit=2 -> {text}");
        assert!(
            text.contains("matched 5, this reply carries 2"),
            "{tool} must print the daemon's pre-limit count: {text}"
        );
        assert!(
            text.contains("还有 3 条未显示"),
            "{tool} must print the remainder #54 is about: {text}"
        );
        assert!(
            !text.contains("UNKNOWN"),
            "{tool} must not hedge about a count the daemon publishes: {text}"
        );
    }

    // 2. EXACTLY EQUAL (5 of 5) and 3. UNTOUCHED (a page wider than the corpus): neither
    //    is a cut, so not one word about a remainder.
    for limit in [5, 10] {
        for tool in ["graph_search", "memory_search"] {
            let out = client
                .call_tool(call(
                    tool,
                    serde_json::json!({ "query": "t27 probe", "limit": limit }),
                ))
                .await
                .unwrap();
            let text = text_of(&out);
            assert!(
                text.contains("matched 5, this reply carries 5"),
                "{tool} limit={limit}: {text}"
            );
            assert!(
                !text.contains("还有"),
                "{tool} limit={limit} must not claim a remainder it does not have: {text}"
            );
        }
    }

    shutdown_pair(client, server_task).await;
}

/// t27 / #54: when the widening scan hits its ceiling, the MCP tool says AT LEAST -- a
/// floor, never dressed as an exact count (t25's rule, pressed end to end).
///
/// The corpus must be larger than the daemon's scan ceiling (api.rs:
/// SEARCH_MATCH_CEILING = 5000), so it is seeded in ONE statement rather than 5005
/// route calls. Both FTS indexes are trigger-maintained (0005_graph.sql /
/// 0004_memory.sql), so a direct insert is indexed exactly like the route's own write.
#[tokio::test]
async fn the_widening_ceiling_is_reported_as_a_lower_bound_through_mcp() {
    let (daemon_url, root) = start_test_daemon_with_policy(
        r#"[permissions]
default = "ask"
"#,
    )
    .await;
    seed(
        &root,
        "WITH RECURSIVE seq(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM seq WHERE i < 5005)
         INSERT INTO entities (name, norm_name, kind, summary, created_at, updated_at)
         SELECT 't27 ceiling entity ' || i, 't27 ceiling entity ' || i, 'probe', NULL,
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z' FROM seq;
         WITH RECURSIVE seq(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM seq WHERE i < 5005)
         INSERT INTO memories (store, namespace, content, content_hash, confidence,
                               created_at, updated_at)
         SELECT 'observation', 'user', 't27 ceiling memory ' || i, 't27c' || i, 0.9,
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z' FROM seq;",
    )
    .await;

    let (client, server_task) = mcp_pair(&daemon_url).await;
    for (tool, query) in [
        ("graph_search", "t27 ceiling entity"),
        ("memory_search", "t27 ceiling memory"),
    ] {
        let out = client
            .call_tool(call(
                tool,
                serde_json::json!({ "query": query, "limit": 2 }),
            ))
            .await
            .unwrap();
        let text = text_of(&out);
        println!("READING t27 ceiling {tool} limit=2 -> {text}");
        assert!(
            text.contains("matched AT LEAST 5000"),
            "{tool} must report the floor as a floor: {text}"
        );
        assert!(
            text.contains("至少还有 4998 条未显示"),
            "{tool} must state the remainder as a lower bound: {text}"
        );
        assert!(
            text.contains("exact match count is unknown"),
            "{tool} must say WHY it cannot be exact: {text}"
        );
        assert!(
            !text.contains("matched 5000, this reply carries 2"),
            "{tool} must not print the scan's ceiling as an exact count: {text}"
        );
    }

    shutdown_pair(client, server_task).await;
}

/// The message of a REFUSED tool call. A refusal that reads as a success is the
/// failure this helper exists to make impossible: `ErrorData` reaches the
/// client as a JSON-RPC error, so `Ok(..)` here is a broken contract, not a
/// softer assertion. A failure raised BEFORE the handler runs (rmcp's parameter
/// extraction, t17) arrives as `Ok(..)` with `is_error: true` instead — that is
/// [`failure`]'s shape, and the difference is the layer, not the severity.
fn refusal(out: Result<rmcp::model::CallToolResult, rmcp::ServiceError>) -> String {
    match out {
        Err(e) => e.to_string(),
        Ok(r) => panic!(
            "expected a refusal, got a successful result: {} / {r:?}",
            text_of(&r)
        ),
    }
}

/// A tool call that FAILED as the client sees it. Two shapes are legitimate and
/// both are failures a caller acts on: a JSON-RPC error (`Err`, what a
/// handler-level `Err(ErrorData)` produces) and a tool result with
/// `is_error: true` (what rmcp's own parameter extraction produces for a field the
/// tool cannot send — t17). A plain `Ok` that is not an error is the false success
/// this helper exists to make impossible.
fn failure(out: Result<rmcp::model::CallToolResult, rmcp::ServiceError>) -> String {
    match out {
        Err(e) => e.to_string(),
        Ok(r) if r.is_error == Some(true) => text_of(&r),
        Ok(r) => panic!(
            "expected a failure, got a successful result: {} / {r:?}",
            text_of(&r)
        ),
    }
}

type ClientHandle = rmcp::service::RunningService<rmcp::service::RoleClient, TestClient>;
type ServerHandle = tokio::task::JoinHandle<
    rmcp::service::RunningService<rmcp::service::RoleServer, PlatformTools>,
>;

/// One platform MCP server and one client over an in-process duplex pipe: the
/// whole §6.5 seam, protocol and HTTP included.
async fn mcp_pair(daemon_url: &str) -> (ClientHandle, ServerHandle) {
    let url_for_server = daemon_url.to_string();
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        PlatformTools::new(BridgeConfig {
            daemon_url: url_for_server,
        })
        .serve(server_transport)
        .await
        .expect("mcp server starts")
    });
    let client = TestClient.serve(client_transport).await.unwrap();
    (client, server_task)
}

async fn shutdown_pair(client: ClientHandle, server_task: ServerHandle) {
    client.cancel().await.unwrap();
    let server = server_task.await.unwrap();
    server.cancel().await.unwrap();
}

/// t6 (wave C), part 1: the capability-plane tools against the REAL daemon
/// routes.
///
/// The request/response SHAPING is unit-tested inside `ruagent-mcp`
/// (crates/mcp/src/lib.rs: the PUT body, the extractor default, the ingest
/// query, the renderings, the refusal text). This test is the other half: the
/// shapes are the ones the daemon actually serves, the ids are the ones
/// `policy.toml` accepts, and the refusals carry the daemon's own words.
#[tokio::test]
async fn mcp_capability_plane_tools_reach_the_daemon() {
    let daemon_url = start_test_daemon().await;
    let (client, server_task) = mcp_pair(&daemon_url).await;

    // -- capabilities_list: the plane is readable, with tier and enable state.
    let out = client
        .call_tool(call("capabilities_list", serde_json::json!({})))
        .await
        .unwrap();
    let text = text_of(&out);
    // the ids are the ones the config file and the HTTP API use — verbatim
    assert!(text.contains("recall_leg_wiki"), "{text}");
    assert!(text.contains("session_extract_rules"), "{text}");
    assert!(text.contains("knowledge_ingest_graph"), "{text}");
    assert!(text.contains("distill_session"), "{text}");
    // a reader must be able to see the tier and the state per row
    assert!(
        text.contains("[llm] distill_session: enabled=false"),
        "{text}"
    );
    assert!(
        text.contains("knowledge_ingest_graph: enabled=false"),
        "the new graph capability is off by default: {text}"
    );
    assert!(text.contains("configured: legacy"), "{text}");

    // A tier filter an agent may pass, and a bogus one that must be refused
    // rather than silently read as "no capabilities".
    let out = client
        .call_tool(call(
            "capabilities_list",
            serde_json::json!({ "tier": "llm" }),
        ))
        .await
        .unwrap();
    let llm = text_of(&out);
    assert!(llm.contains("distill_session"), "{llm}");
    assert!(!llm.contains("recall_leg_wiki"), "{llm}");
    let err = refusal(
        client
            .call_tool(call(
                "capabilities_list",
                serde_json::json!({ "tier": "cheap" }),
            ))
            .await,
    );
    assert!(err.contains("cheap"), "{err}");
    assert!(err.contains("free") && err.contains("llm"), "{err}");

    // -- capability_set: an unknown id is refused BY NAME, with the known ids.
    let err = refusal(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({ "id": "recall_leg_wik", "enabled": false }),
            ))
            .await,
    );
    assert!(err.contains("recall_leg_wik"), "{err}");
    assert!(err.contains("known ids"), "{err}");
    assert!(err.contains("capability_set"), "{err}");

    // -- the llm cost gate: enabling distill_session without confirm_cost is a
    // refusal that names the flag — never a silent spend.
    let err = refusal(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({ "id": "distill_session", "enabled": true }),
            ))
            .await,
    );
    assert!(err.contains("distill_session"), "{err}");
    assert!(err.contains("confirm_cost"), "{err}");

    // -- knowledge_graph_ingest is exercised end to end in the second test
    // below; here the point is that enabling a FREE capability needs no
    // confirm_cost, and the reply says the value now comes from the file.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({ "id": "knowledge_ingest_graph", "enabled": true }),
        ))
        .await
        .unwrap();
    let enabled_text = text_of(&out);
    assert!(
        enabled_text.contains("`knowledge_ingest_graph` is now enabled=true"),
        "{enabled_text}"
    );

    // -- the toggle did NOT silently reset the rest of the plane: the daemon's
    // PUT replaces the whole table, so this is the assertion that the bridge's
    // read-modify-write kept every other capability where it was.
    let out = client
        .call_tool(call("capabilities_list", serde_json::json!({})))
        .await
        .unwrap();
    let after = text_of(&out);
    assert!(
        after.contains("knowledge_ingest_graph: enabled=true"),
        "{after}"
    );
    assert!(
        after.contains("knowledge_ingest_graph: enabled=true (configured: file"),
        "the write landed in policy.toml: {after}"
    );
    assert!(after.contains("recall_leg_wiki: enabled=true"), "{after}");
    assert!(
        after.contains("[llm] distill_session: enabled=false"),
        "{after}"
    );

    // -- distill_session: the free rules path reaches the daemon's distill
    // handler (HTTP 400 about the session, not a 404 route miss). A successful
    // run needs an enabled agent and a real transcript; that live path belongs
    // to the operator's probe, not to a fixture this test fabricates.
    let err = refusal(
        client
            .call_tool(call(
                "distill_session",
                serde_json::json!({
                    "session_key": "ruagent:0f3a9c7e5b1d2a44",
                    "extractor": "rules",
                    "dry_run": true
                }),
            ))
            .await,
    );
    assert!(err.contains("distill_session"), "{err}");
    assert!(err.contains("HTTP 400"), "{err}");

    // A bad extractor is refused before any request leaves the bridge.
    let err = refusal(
        client
            .call_tool(call(
                "distill_session",
                serde_json::json!({ "session_key": "ruagent:0f3a", "extractor": "llm" }),
            ))
            .await,
    );
    assert!(err.contains("model tokens"), "{err}");

    shutdown_pair(client, server_task).await;
}

/// t6 (wave C), part 2: the free knowledge→graph ingestion path against the
/// REAL route, including the fact that it is OFF by default.
///
/// The route is `POST /api/v1/knowledge/graph/ingest?document=<name>&dry_run=…`
/// (query parameters, no body — t4's frozen shape), and the capability refusal
/// is a 409 whose body names the capability and both remedies.
#[tokio::test]
async fn mcp_knowledge_graph_ingest_reaches_the_daemon() {
    let daemon_url = start_test_daemon().await;
    let (client, server_task) = mcp_pair(&daemon_url).await;

    // A document must exist before the graph ingest has anything to read.
    let out = client
        .call_tool(call(
            "knowledge_ingest",
            serde_json::json!({
                "name": "graph-notes",
                "content": "Kubernetes runs on the cluster. The release script lives in scripts/release.sh and runs from the repository root."
            }),
        ))
        .await
        .unwrap();
    assert!(text_of(&out).contains("ingested"), "{out:?}");

    // The pricing path works while the capability is OFF: counts, no writes.
    let out = client
        .call_tool(call(
            "knowledge_graph_ingest",
            serde_json::json!({ "document": "graph-notes", "dry_run": true }),
        ))
        .await
        .unwrap();
    let priced = text_of(&out);
    assert!(priced.contains("DRY RUN"), "{priced}");
    assert!(priced.contains("documents"), "{priced}");

    // A real write while the capability is off is REFUSED by name, with the
    // remedy in the message — never an opaque transport error.
    let err = refusal(
        client
            .call_tool(call(
                "knowledge_graph_ingest",
                serde_json::json!({ "document": "graph-notes", "dry_run": false }),
            ))
            .await,
    );
    assert!(err.contains("knowledge_ingest_graph"), "{err}");
    assert!(
        err.contains("enabled = true"),
        "the refusal names the exact enable action: {err}"
    );
    assert!(err.contains("capability_set"), "{err}");

    // Enable it through the MCP tool (free tier: no confirm_cost), then the real
    // write succeeds.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({ "id": "knowledge_ingest_graph", "enabled": true }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("`knowledge_ingest_graph` is now enabled=true"),
        "{out:?}"
    );
    let out = client
        .call_tool(call(
            "knowledge_graph_ingest",
            serde_json::json!({ "document": "graph-notes" }),
        ))
        .await
        .unwrap();
    let ingested = text_of(&out);
    assert!(ingested.contains("documents"), "{ingested}");
    assert!(!ingested.contains("DRY RUN"), "{ingested}");
    // The ledger's idempotence is t4's own contract (its counts on a re-run);
    // this test asserts the MCP seam, not the extractor's bookkeeping.

    shutdown_pair(client, server_task).await;
}

/// The raw body of one `[capabilities.<id>]` section of `policy.toml` — the FILE,
/// not the daemon's reply and not the tool's rendering of it.
fn capability_section(text: &str, id: &str) -> String {
    let header = format!("[capabilities.{id}]");
    let mut body = String::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == header;
            continue;
        }
        if inside && !trimmed.is_empty() {
            body.push_str(trimmed);
            body.push('\n');
        }
    }
    body
}

/// The KEY SET of one `[capabilities.<id>]` section, sorted (the editor's
/// insertion order is not the file's original order, so only the set is stable).
fn capability_section_keys(text: &str, id: &str) -> Vec<String> {
    let mut keys: Vec<String> = capability_section(text, id)
        .lines()
        .filter_map(|l| l.split('=').next())
        .map(|k| k.trim().to_string())
        .collect();
    keys.sort();
    keys
}

/// t10: the MCP capability write preserves the file's OWN key set.
///
/// Two properties are on trial here, and both were broken once elsewhere:
///
/// * the READ-MODIFY-WRITE — the daemon's PUT REPLACES the whole `[capabilities]`
///   table and removes every row and key an entry omits, so a one-id write must
///   carry every other configured row back (`CapabilitiesEditor::update`);
/// * NO RE-MATERIALIZATION — re-emitting the RESOLVED values instead persists
///   registry defaults into rows the user never set, which is the defect measured
///   on the panel side (`enabled = true` alone became `enabled = true` +
///   `weight = 1`).
///
/// The witness is `policy.toml`, read before and after each write: the file is what
/// the user owns, and a tool reply cannot testify about it.
#[tokio::test]
async fn capability_set_preserves_the_files_own_key_set() {
    let (daemon_url, root) = start_test_daemon_with_policy(
        "[permissions]\ndefault = \"ask\"\n\
         \n\
         [capabilities.recall_leg_memory_fts]\n\
         enabled = true\n\
         \n\
         [capabilities.recall_leg_memory_semantic]\n\
         weight = 3.5\n\
         \n\
         [capabilities.recall_leg_wiki]\n",
    )
    .await;
    let policy = root.join("config").join("policy.toml");
    let before = std::fs::read_to_string(&policy).unwrap();
    // The seed is the shape that exposes both defects: one row carries `enabled`
    // only (so `weight` resolves to its registry default 1.0), one carries
    // `weight` only (so `enabled` resolves to its default true), one is empty.
    assert_eq!(
        capability_section_keys(&before, "recall_leg_memory_fts"),
        vec!["enabled"]
    );
    assert_eq!(
        capability_section_keys(&before, "recall_leg_memory_semantic"),
        vec!["weight"]
    );

    let (client, server_task) = mcp_pair(&daemon_url).await;

    // The DECLARED key set is visible before anything is written. `min_score` on
    // the memory semantic leg declares no registry default, so there is no
    // resolved value to render — and the tool still names it, with the daemon's own
    // accepted-range phrase, which is what keeps it reachable from this surface.
    let out = client
        .call_tool(call("capabilities_list", serde_json::json!({})))
        .await
        .unwrap();
    let listed = text_of(&out);
    assert!(
        listed.contains(
            "min_score=- (declared float; the file does not carry it and the registry \
             declares no default; accepted finite and 0.0..=1.0)"
        ),
        "a declared key with no default must be nameable: {listed}"
    );
    assert!(
        listed.contains("weight=1.0 (float default, the file does not)"),
        "a defaulted value must not read as the file's: {listed}"
    );
    assert!(
        listed.contains("weight=3.5 (the file carries it)"),
        "a value the file carries must read as the file's: {listed}"
    );

    // Write about a capability the file does not mention at all.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({ "id": "knowledge_ingest_graph", "enabled": true }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("`knowledge_ingest_graph` is now enabled=true"),
        "{out:?}"
    );

    let after = std::fs::read_to_string(&policy).unwrap();
    assert_eq!(
        capability_section_keys(&after, "knowledge_ingest_graph"),
        vec!["enabled"],
        "the requested row landed: {after}"
    );
    for id in [
        "recall_leg_memory_fts",
        "recall_leg_memory_semantic",
        "recall_leg_wiki",
    ] {
        assert_eq!(
            capability_section(&after, id),
            capability_section(&before, id),
            "row `{id}` was rewritten by a write that did not name it: {after}"
        );
        assert_eq!(
            capability_section_keys(&after, id),
            capability_section_keys(&before, id),
            "row `{id}` changed KEY SET: {after}"
        );
    }
    assert!(
        !capability_section(&after, "recall_leg_memory_fts").contains("weight"),
        "a RESOLVED default (weight = 1.0) was materialized into the user's file: {after}"
    );

    // A SECOND write, this time with values, to the semantic leg: the declared key
    // with no default becomes settable through MCP, the file's own `weight` is
    // updated, and NOTHING else appears — `enabled` is written only because the
    // caller set it (it is a required tool argument), never as a default.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({
                "id": "recall_leg_memory_semantic",
                "enabled": true,
                "weight": 2.5,
                "min_score": 0.4
            }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("`recall_leg_memory_semantic` is now enabled=true"),
        "{out:?}"
    );
    let after_two = std::fs::read_to_string(&policy).unwrap();
    assert_eq!(
        capability_section_keys(&after_two, "recall_leg_memory_semantic"),
        vec!["enabled", "min_score", "weight"],
        "only the keys the caller asked for: {after_two}"
    );
    let semantic = capability_section(&after_two, "recall_leg_memory_semantic");
    assert!(semantic.contains("min_score = 0.4"), "{after_two}");
    assert!(semantic.contains("weight = 2.5"), "{after_two}");
    for id in ["recall_leg_memory_fts", "recall_leg_wiki"] {
        assert_eq!(
            capability_section(&after_two, id),
            capability_section(&before, id),
            "row `{id}` survived a second write untouched: {after_two}"
        );
    }
    assert_eq!(
        capability_section(&after_two, "knowledge_ingest_graph"),
        capability_section(&after, "knowledge_ingest_graph"),
        "the first write's row survived the second: {after_two}"
    );

    // The DAEMON's own refusal still reaches the caller verbatim: a key this
    // capability does not declare is refused with the daemon's sentence and its
    // accepted-key list, not with a locally invented message.
    let err = refusal(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({
                    "id": "recall_leg_wiki",
                    "enabled": true,
                    "weight": 2.0
                }),
            ))
            .await,
    );
    assert!(
        err.contains("capability `recall_leg_wiki` does not accept the option `weight`"),
        "the daemon's own words: {err}"
    );
    assert!(err.contains("it accepts: none"), "{err}");

    // A VALUE the daemon refuses is likewise its own 400 phrase, verbatim: this
    // bridge pre-validates nothing (a second range check here could drift from the
    // registry's), so the range the agent reads is the range the daemon enforces.
    let err = refusal(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({
                    "id": "session_extract_rules",
                    "enabled": true,
                    "max_per_input": 0
                }),
            ))
            .await,
    );
    assert!(
        err.contains(
            "capability `session_extract_rules`: `max_per_input = 0` is out of range (1..=10000)"
        ),
        "the daemon's own range phrase: {err}"
    );

    // ... and the refused write left the file exactly as it was.
    assert_eq!(
        std::fs::read_to_string(&policy).unwrap(),
        after_two,
        "a refused write must not touch the file"
    );

    shutdown_pair(client, server_task).await;
}

/// t17: a field this tool cannot SEND is refused, and nothing is written.
///
/// The measured defect this closes: `capability_set(id=recall_leg_memory_semantic,
/// enabled=true, decay_half_life=5.0)` answered SUCCESS while serde dropped the key
/// before the bridge saw it — the caller was told the update happened, and the key
/// appeared neither in the reply nor in `policy.toml`. The FILE is the witness here:
/// a refusal must leave it byte-identical, and the control call through the SAME
/// daemon shows the tool still writes when every field is one it can send.
#[tokio::test]
async fn capability_set_refuses_a_field_it_cannot_send() {
    let (daemon_url, root) = start_test_daemon_with_policy(
        "[permissions]\ndefault = \"ask\"\n\n[capabilities.recall_leg_memory_semantic]\nweight = 3.5\n",
    )
    .await;
    let policy = root.join("config").join("policy.toml");
    let before = std::fs::read_to_string(&policy).unwrap();
    let (client, server_task) = mcp_pair(&daemon_url).await;

    // The refused call: no success result exists, so nothing can have claimed the
    // update happened.
    let err = failure(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({
                    "id": "recall_leg_memory_semantic",
                    "enabled": true,
                    "decay_half_life": 5.0
                }),
            ))
            .await,
    );
    assert!(err.contains("unknown field `decay_half_life`"), "{err}");
    assert!(
        !err.contains("is now enabled"),
        "the caller must not be told the change happened: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(&policy).unwrap(),
        before,
        "the refused call must leave policy.toml untouched"
    );

    // CONTROL through the same daemon: with only fields the tool can send, the write
    // lands — the refusal above is about the field, not about the tool being broken.
    let out = client
        .call_tool(call(
            "capability_set",
            serde_json::json!({
                "id": "recall_leg_memory_semantic",
                "enabled": true,
                "min_score": 0.4
            }),
        ))
        .await
        .unwrap();
    assert!(
        text_of(&out).contains("`recall_leg_memory_semantic` is now enabled=true"),
        "{out:?}"
    );
    let after = std::fs::read_to_string(&policy).unwrap();
    assert_ne!(after, before, "the control write must land in the file");
    assert_eq!(
        capability_section_keys(&after, "recall_leg_memory_semantic"),
        vec!["enabled", "min_score", "weight"],
        "the control write adds exactly what the caller set: {after}"
    );

    shutdown_pair(client, server_task).await;
}
