//! The v2 tools: create a task, dispatch it to an agent, read the run back (ruagent-issues-56-59
//! t53).
//!
//! WHY A MOCK DAEMON, WHEN THE OTHER FILE DRIVES THE REAL ROUTER. Two of these assertions are
//! about the REQUEST this tool SENDS -- does an omitted agent stay omitted, does an explicit one
//! travel under the key the daemon reads -- and a request body is only observable at a server
//! boundary. The same server lets one test hand back a run whose `params.agent` is a registry ID
//! and check that the TOOL turns it into a NAME, which is what a caller and a user understand.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};
use ruagent_mcp::{BridgeConfig, PlatformTools};

/// The registry id the mock hands back inside a run. Deliberately NOT a name: the tool must
/// translate it.
const AGENT_ID: &str = "01a20000-0000-7000-8000-0000000000aa";
const TASK_ID: &str = "01a20000-0000-7000-8000-0000000000t1";
const RUN_ID: &str = "01a20000-0000-7000-8000-0000000000r1";

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    body: serde_json::Value,
}

/// The observable state: every request the tool made, in order.
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

    /// Every body sent to a path ENDING with `suffix` (the task id varies between tests).
    fn bodies_at(&self, method: &str, suffix: &str) -> Vec<serde_json::Value> {
        self.seen
            .lock()
            .expect("mock state")
            .iter()
            .filter(|s| s.method == method && s.path.ends_with(suffix))
            .map(|s| s.body.clone())
            .collect()
    }

    fn count_at(&self, method: &str, suffix: &str) -> usize {
        self.bodies_at(method, suffix).len()
    }

    /// Requests to a path PREFIX (the run id makes an exact match awkward).
    fn count_prefix(&self, method: &str, prefix: &str) -> usize {
        self.seen
            .lock()
            .expect("mock state")
            .iter()
            .filter(|s| s.method == method && s.path.starts_with(prefix))
            .count()
    }
}

fn run_json(status: &str, prompt: Option<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "id": RUN_ID,
        "task_id": TASK_ID,
        // THE ID, NOT THE NAME -- exactly what the daemon stores, and what the tool has to translate.
        "params": { "agent": AGENT_ID, "prompt": prompt },
        "status": status,
        "acp_session_id": null,
        "workspace": null,
        "context_usage": null,
        "cost_usd": 0.0042,
        "error": null,
        "result": null,
        "stop_reason": null,
        "created_at": "2026-10-31T00:00:00Z",
        "updated_at": "2026-10-31T00:00:00Z"
    })
}

async fn create_task(
    State(mock): State<Mock>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    mock.record("POST", "/api/v1/tasks".to_string(), body.clone());
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "id": TASK_ID,
            "title": body.get("title").cloned().unwrap_or(serde_json::Value::Null),
            "intent": body.get("intent").cloned().unwrap_or(serde_json::Value::Null),
            "project": body.get("project").cloned().unwrap_or(serde_json::Value::Null),
            "status": "pending",
        })),
    )
}

async fn dispatch(
    State(mock): State<Mock>,
    Path(task): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    mock.record("POST", format!("/api/v1/tasks/{task}/runs"), body.clone());
    if task == "missing" {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "task not found" })),
        );
    }
    if body.get("agent").and_then(|v| v.as_str()) == Some("nope") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unknown agent `nope` -- known agents: alpha"
            })),
        );
    }
    (
        StatusCode::OK,
        Json(run_json("queued", body.get("prompt").cloned())),
    )
}

async fn read_run(
    State(mock): State<Mock>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    mock.record("GET", format!("/api/v1/runs/{id}"), serde_json::Value::Null);
    if id == "missing" {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "run not found" })),
        );
    }
    (StatusCode::OK, Json(run_json("running", None)))
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

/// Start the mock and return its URL plus the recorded-request handle.
async fn start_mock() -> (String, Mock) {
    let mock = Mock::new();
    let app = Router::new()
        .route("/api/v1/tasks", post(create_task))
        .route("/api/v1/tasks/{id}/runs", post(dispatch))
        .route("/api/v1/runs/{id}", get(read_run))
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

fn failure(out: Result<rmcp::model::CallToolResult, rmcp::ServiceError>) -> String {
    match out {
        Err(e) => e.to_string(),
        Ok(r) if r.is_error == Some(true) => text_of(&r),
        Ok(r) => panic!("expected a failure, got: {}", text_of(&r)),
    }
}

#[tokio::test]
async fn task_create_sends_the_intent_and_reports_the_id() {
    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call(
                "task_create",
                serde_json::json!({
                    "title": "ship the release notes",
                    "intent": "write notes for v0.2.0 and open the PR",
                    "project": "ruagent"
                }),
            ))
            .await
            .unwrap(),
    );
    println!("READING t53 task_create:\n{out}");
    assert!(out.contains(TASK_ID), "the id must be in the reply: {out}");
    assert!(out.contains("status: pending"), "{out}");
    assert!(
        out.contains("spends no tokens"),
        "the free-half of the pair must be said: {out}"
    );

    let bodies = mock.bodies_at("POST", "/api/v1/tasks");
    assert_eq!(bodies.len(), 1, "one create request");
    assert_eq!(
        bodies[0]["intent"],
        "write notes for v0.2.0 and open the PR"
    );
    assert_eq!(bodies[0]["project"], "ruagent");

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn task_dispatch_names_the_agent_and_leaves_an_omitted_one_out_of_the_request() {
    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    // NO AGENT: the cascade's decision, so the REQUEST must not carry the field at all.
    let out = text_of(
        &client
            .call_tool(call(
                "task_dispatch",
                serde_json::json!({ "task_id": TASK_ID, "prompt": "do the thing" }),
            ))
            .await
            .unwrap(),
    );
    println!("READING t53 task_dispatch (no agent):\n{out}");
    assert!(
        out.contains(RUN_ID),
        "the run id must be in the reply: {out}"
    );
    assert!(
        out.contains("agent: alpha"),
        "the reply must name the agent the daemon resolved: {out}"
    );
    assert!(
        !out.contains(AGENT_ID),
        "the reply must NOT show the registry id: {out}"
    );
    assert!(out.contains("status: queued"), "{out}");
    assert!(
        out.contains("ASYNCHRONOUS"),
        "the reply must say the work is still going: {out}"
    );

    let bodies = mock.bodies_at("POST", "/runs");
    assert_eq!(bodies.len(), 1, "one dispatch request");
    assert!(
        bodies[0].get("agent").is_none(),
        "an omitted agent must be ABSENT from the request (the daemon's cascade decides, not this tool): {}",
        bodies[0]
    );
    assert_eq!(bodies[0]["prompt"], "do the thing");
    // The translation itself happened against the daemon, not against a local table.
    assert_eq!(
        mock.count_at("GET", "/api/v1/agents"),
        1,
        "the id was resolved by the daemon"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn task_dispatch_sends_an_explicit_agent_under_its_own_key() {
    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call(
                "task_dispatch",
                serde_json::json!({ "task_id": TASK_ID, "agent": "alpha" }),
            ))
            .await
            .unwrap(),
    );
    println!("READING t53 task_dispatch (agent=alpha):\n{out}");
    let bodies = mock.bodies_at("POST", "/runs");
    assert_eq!(bodies.len(), 1);
    assert_eq!(
        bodies[0]["agent"], "alpha",
        "the name travels as-is: {}",
        bodies[0]
    );
    assert!(
        bodies[0].get("prompt").is_none(),
        "an omitted prompt must stay omitted too: {}",
        bodies[0]
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn a_missing_task_and_an_unknown_agent_keep_the_daemons_own_words() {
    let (url, _mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let missing = failure(
        client
            .call_tool(call(
                "task_dispatch",
                serde_json::json!({ "task_id": "missing" }),
            ))
            .await,
    );
    println!("READING t53 task_dispatch(missing task):\n{missing}");
    assert!(missing.contains("task not found"), "{missing}");

    let unknown = failure(
        client
            .call_tool(call(
                "task_dispatch",
                serde_json::json!({ "task_id": TASK_ID, "agent": "nope" }),
            ))
            .await,
    );
    println!("READING t53 task_dispatch(unknown agent):\n{unknown}");
    assert!(unknown.contains("unknown agent"), "{unknown}");
    assert!(
        unknown.contains("known agents: alpha"),
        "the daemon's list of what DOES exist must survive: {unknown}"
    );

    let run_missing = failure(
        client
            .call_tool(call("run_get", serde_json::json!({ "run_id": "missing" })))
            .await,
    );
    println!("READING t53 run_get(missing run):\n{run_missing}");
    assert!(run_missing.contains("run not found"), "{run_missing}");

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn an_unreachable_daemon_is_reported_by_all_three_tools() {
    let (client, server_task) = served_tools("http://127.0.0.1:1".to_string()).await;
    for (tool, args) in [
        (
            "task_create",
            serde_json::json!({ "title": "t", "intent": "i" }),
        ),
        ("task_dispatch", serde_json::json!({ "task_id": TASK_ID })),
        ("run_get", serde_json::json!({ "run_id": RUN_ID })),
    ] {
        let msg = failure(client.call_tool(call(tool, args)).await);
        println!("READING t53 {tool} against a dead daemon: {msg}");
        assert!(
            msg.contains("cannot reach the ruagent daemon"),
            "{tool} must report the transport failure in words a caller can act on: {msg}"
        );
    }
    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn run_get_reports_a_running_state_without_waiting_for_it() {
    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let started = std::time::Instant::now();
    let out = text_of(
        &client
            .call_tool(call("run_get", serde_json::json!({ "run_id": RUN_ID })))
            .await
            .unwrap(),
    );
    let elapsed = started.elapsed();
    println!("READING t53 run_get:\n{out}");
    println!("READING t53 run_get took {elapsed:?}");
    assert!(out.contains("status: running"), "{out}");
    assert!(
        out.contains("NOT finished yet"),
        "an unfinished run must say so: {out}"
    );
    assert!(
        out.contains("agent: alpha"),
        "the agent is named here too: {out}"
    );
    assert!(
        out.contains("result: <none yet"),
        "no final text yet must be stated, not left out: {out}"
    );
    // THE NON-BLOCKING ASSERTION: a real wait would be a poll loop, and a poll loop would be MORE
    // THAN ONE request to the same run. One request, one reading, and the call returns immediately.
    assert_eq!(
        mock.count_prefix("GET", "/api/v1/runs/"),
        1,
        "run_get must read the run ONCE (no polling): {:?}",
        mock.bodies_at("GET", "/api/v1/runs/")
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "run_get must not block: took {elapsed:?}"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn the_dispatch_tools_say_what_they_cost_and_that_they_do_not_wait() {
    let (url, _mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;
    let tools = client.list_all_tools().await.unwrap();
    let description = |name: &str| {
        tools
            .iter()
            .find(|t| t.name.as_ref() == name)
            .unwrap_or_else(|| panic!("{name} is not served"))
            .description
            .clone()
            .unwrap_or_default()
    };

    // THE THREE ARE SERVED (the surface lists in the other test files pin the names and the count;
    // this pins what a model reading them is told).
    let create = description("task_create");
    assert!(
        create.contains("zero model tokens"),
        "creating a task is free and must say so: {create}"
    );
    let dispatch = description("task_dispatch");
    assert!(
        dispatch.contains("SPENDS THE USER'S MODEL TOKENS"),
        "the one call that starts a run must say what it costs: {dispatch}"
    );
    assert!(
        dispatch.to_lowercase().contains("say it out loud"),
        "and that the user must be told before it happens: {dispatch}"
    );
    assert!(
        dispatch.contains("NAME") && dispatch.contains("does not guess a default"),
        "the cascade rule must be stated: {dispatch}"
    );
    let read = description("run_get");
    assert!(
        read.contains("zero model tokens") && read.contains("DOES NOT WAIT"),
        "run_get is free and never blocks, and must say both: {read}"
    );

    shutdown_pair(client, server_task).await;
}
