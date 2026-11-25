//! P6 (`docs/plans/2026-11-23-reports-never-lost.md` §3): `task_dispatch` attributes the work to
//! the conversation this process is working for, without the agent having to know its own chat id.
//!
//! WHY A MOCK DAEMON. The claim is about the REQUEST body: does `origin_chat_id` travel when the
//! caller did not send one, does an explicit one win, and does an ABSENT conversation stay absent
//! rather than becoming an empty string. A request body is only observable at a server boundary, so
//! the daemon here records what arrived — the same shape `dispatch.rs` uses for `agent`.
//!
//! WHY THE ENVIRONMENT IS SET HERE, IN A FILE OF ITS OWN. `RUAGENT_CHAT_ID` is process-global, and an
//! integration-test file is its own process: the three states (absent / set / set-empty) are walked
//! in ONE test function, in order, so no two tests can observe each other's variable. This is the
//! READER half of P6; the producer half — the daemon really putting the variable on the agent
//! process — is measured on real spawned processes in
//! `crates/daemon/tests/chat_origin_env.rs`. Neither test claims the other's reading.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};
use ruagent_mcp::{BridgeConfig, PlatformTools};

/// The variable, spelled out rather than imported: the name IS the contract between the daemon
/// (which sets it) and this server (which reads it), and a test that imported it from one side could
/// not notice the two sides drifting apart.
const CHAT_ENV: &str = "RUAGENT_CHAT_ID";

/// The conversation the daemon "exported" (stands in for the chat a run was dispatched from).
const CHAT_ID: &str = "01a20000-0000-7000-8000-00000000c001";
/// A DIFFERENT conversation, for the explicit-argument-wins case.
const OTHER_CHAT: &str = "01a20000-0000-7000-8000-00000000c002";

const TASK_ID: &str = "01a20000-0000-7000-8000-0000000000t1";
const RUN_ID: &str = "01a20000-0000-7000-8000-0000000000r1";
const AGENT_ID: &str = "01a20000-0000-7000-8000-0000000000aa";

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    body: serde_json::Value,
}

#[derive(Clone)]
struct Mock {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Mock {
    fn new() -> Self {
        Self {
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn record(&self, method: &str, path: String, body: serde_json::Value) {
        self.seen.lock().expect("mock state").push(Seen {
            method: method.to_string(),
            path,
            body,
        });
    }

    /// Every dispatch body the tool sent, in order.
    fn dispatch_bodies(&self) -> Vec<serde_json::Value> {
        self.seen
            .lock()
            .expect("mock state")
            .iter()
            .filter(|s| s.method == "POST" && s.path.ends_with("/runs"))
            .map(|s| s.body.clone())
            .collect()
    }
}

async fn dispatch(
    State(mock): State<Mock>,
    Path(task): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    mock.record("POST", format!("/api/v1/tasks/{task}/runs"), body.clone());
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "id": RUN_ID,
            "task_id": TASK_ID,
            // The daemon echoes what the REQUEST carried -- that is what the reply's attribution
            // line is derived from, and it keeps this mock honest about the wire.
            "origin_chat_id": body.get("origin_chat_id").cloned().unwrap_or(serde_json::Value::Null),
            "params": { "agent": AGENT_ID, "prompt": body.get("prompt").cloned() },
            "status": "queued",
            "acp_session_id": null,
            "workspace": null,
            "context_usage": null,
            "cost_usd": null,
            "error": null,
            "result": null,
            "stop_reason": null,
            "created_at": "2026-11-23T00:00:00Z",
            "updated_at": "2026-11-23T00:00:00Z"
        })),
    )
}

async fn agents(State(mock): State<Mock>) -> Json<serde_json::Value> {
    mock.record("GET", "/api/v1/agents".to_string(), serde_json::Value::Null);
    Json(serde_json::json!({
        "agents": [{
            "id": AGENT_ID,
            "name": "alpha",
            "kind": "role",
            "enabled": true,
            "primary": false,
            "model": null,
            "description": "the card the run names"
        }]
    }))
}

async fn start_mock() -> (String, Mock) {
    let mock = Mock::new();
    let app = Router::new()
        .route("/api/v1/tasks/{id}/runs", post(dispatch))
        .route("/api/v1/agents", get(agents))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://{addr}"), mock)
}

struct TestClient;

impl ClientHandler for TestClient {
    fn get_info(&self) -> rmcp::model::ClientInfo {
        rmcp::model::ClientInfo::default()
    }
}

type ClientHandle = rmcp::service::RunningService<rmcp::service::RoleClient, TestClient>;
type ServerHandle = tokio::task::JoinHandle<
    rmcp::service::RunningService<rmcp::service::RoleServer, PlatformTools>,
>;

async fn served_tools(daemon_url: String) -> (ClientHandle, ServerHandle) {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        PlatformTools::new(BridgeConfig { daemon_url })
            .serve(server_transport)
            .await
            .expect("mcp server starts")
    });
    let client = TestClient.serve(client_transport).await.unwrap();
    (client, server_task)
}

async fn shutdown_pair(client: ClientHandle, server_task: ServerHandle) {
    client.cancel().await.unwrap();
    server_task.await.unwrap().cancel().await.unwrap();
}

fn call(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    let obj = args.as_object().cloned().unwrap_or_default();
    CallToolRequestParams::new(name.to_string()).with_arguments(obj)
}

fn text_of(out: &rmcp::model::CallToolResult) -> String {
    out.content
        .iter()
        .filter_map(|c| match c {
            rmcp::model::ContentBlock::Text(t) => Some(t.text.to_string()),
            _ => None,
        })
        .collect()
}

/// Walk the three states of the daemon's export, one after the other, against one mock daemon:
/// absent (a panel/CLI/script context), present (a chat-dispatched run), and an explicit argument
/// that must beat it. One function, because the variable is process-global and the ORDER is the
/// evidence: the same tool, same process, three readings that differ only by the environment.
#[tokio::test]
async fn task_dispatch_attributes_work_to_the_conversation_the_daemon_exported() {
    // (0) The process must not start with a stale value: the absence reading below would be
    // meaningless otherwise. `remove_var` is unsafe in edition 2024 because the environment is
    // shared with every thread; this file is its own process and only this test touches it.
    unsafe { std::env::remove_var(CHAT_ENV) };

    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    // (1) NO CONVERSATION. The panel, the CLI and a script all look like this, and the field must be
    // ABSENT from the request -- not an empty string, which the daemon would try to attribute.
    let out = text_of(
        &client
            .call_tool(call(
                "task_dispatch",
                serde_json::json!({ "task_id": TASK_ID, "agent": "alpha" }),
            ))
            .await
            .unwrap(),
    );
    println!("READING P6 task_dispatch with no conversation:\n{out}");
    let bodies = mock.dispatch_bodies();
    assert_eq!(bodies.len(), 1, "one dispatch request");
    assert!(
        bodies[0].get("origin_chat_id").is_none(),
        "with no daemon export and no argument, the origin must be ABSENT from the request: {}",
        bodies[0]
    );
    assert!(
        out.contains("belongs to NO conversation"),
        "the reply must say the work is attributed to nobody, because an agent that believes it \
         reported when nobody is waiting is the failure this change exists to prevent: {out}"
    );

    // (2) THE DAEMON EXPORTED A CONVERSATION (what happens inside a chat-dispatched run: the agent
    // process carries it, the MCP server it spawned inherited it). No argument is passed here --
    // that is the whole point, because an agent does not know its own chat id.
    unsafe { std::env::set_var(CHAT_ENV, CHAT_ID) };
    let out = text_of(
        &client
            .call_tool(call(
                "task_dispatch",
                serde_json::json!({ "task_id": TASK_ID, "agent": "alpha" }),
            ))
            .await
            .unwrap(),
    );
    println!("READING P6 task_dispatch inside a conversation:\n{out}");
    let bodies = mock.dispatch_bodies();
    assert_eq!(bodies.len(), 2, "a second dispatch request");
    assert_eq!(
        bodies[1]["origin_chat_id"], CHAT_ID,
        "the run must be attributed to the conversation this process was told it works for: {}",
        bodies[1]
    );
    assert!(
        out.contains(CHAT_ID) && out.contains("belongs to conversation"),
        "the reply must report the attribution back to the caller: {out}"
    );

    // (3) AN EXPLICIT ARGUMENT WINS. Handing work to a DIFFERENT conversation is a real use
    // (a table of follow-ups for another thread), and the caller's word outranks the export.
    let out = text_of(
        &client
            .call_tool(call(
                "task_dispatch",
                serde_json::json!({ "task_id": TASK_ID, "origin_chat_id": OTHER_CHAT }),
            ))
            .await
            .unwrap(),
    );
    println!("READING P6 task_dispatch with an explicit conversation:\n{out}");
    let bodies = mock.dispatch_bodies();
    assert_eq!(bodies.len(), 3);
    assert_eq!(
        bodies[2]["origin_chat_id"], OTHER_CHAT,
        "an explicit origin_chat_id must win over the daemon's export: {}",
        bodies[2]
    );
    assert!(out.contains(OTHER_CHAT), "{out}");

    // (4) AN EMPTY EXPORT IS NOT A CONVERSATION. The daemon never writes one, but a shell can, and
    // forwarding "" would ask the daemon to attribute work to a chat with no id.
    unsafe { std::env::set_var(CHAT_ENV, "") };
    client
        .call_tool(call(
            "task_dispatch",
            serde_json::json!({ "task_id": TASK_ID, "agent": "alpha" }),
        ))
        .await
        .unwrap();
    let bodies = mock.dispatch_bodies();
    assert_eq!(bodies.len(), 4);
    assert!(
        bodies[3].get("origin_chat_id").is_none(),
        "an empty export must be treated as ABSENT, never forwarded as \"\": {}",
        bodies[3]
    );
    unsafe { std::env::remove_var(CHAT_ENV) };

    shutdown_pair(client, server_task).await;
}

/// The TOOL DESCRIPTION is what a model reads before it decides to pass `origin_chat_id` at all, so
/// the P6 sentence has to be in it: automatic by default, explicit wins, and honest about the case
/// where nobody is waiting. A description that still says "you fill it in" would recreate the
/// original bug (a field nobody fills) no matter what the code does.
#[tokio::test]
async fn the_dispatch_description_says_the_conversation_is_automatic() {
    let (url, _mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;
    let tools = client.list_all_tools().await.unwrap();
    let dispatch = tools
        .iter()
        .find(|t| t.name.as_ref() == "task_dispatch")
        .expect("task_dispatch is served")
        .description
        .clone()
        .unwrap_or_default();
    println!("READING P6 task_dispatch description:\n{dispatch}");

    assert!(
        dispatch.contains(CHAT_ENV),
        "the description must name the mechanism the daemon actually uses: {dispatch}"
    );
    assert!(
        dispatch.contains("AUTOMATICALLY")
            && dispatch.contains("do not have to know your own chat id"),
        "the description must say the attribution is automatic, because THAT is what the agent \
         could not do for itself: {dispatch}"
    );
    assert!(
        dispatch.contains("explicit origin_chat_id you DO pass wins"),
        "an explicit value must be stated to win: {dispatch}"
    );
    assert!(
        dispatch.contains("reported to nobody"),
        "the case with no conversation behind the run must be stated, not left to be discovered: \
         {dispatch}"
    );

    shutdown_pair(client, server_task).await;
}
