//! runtime_probe: a failed probe is a RESULT, not an HTTP error (ruagent-issues-56-59 t67).
//!
//! The daemon answers a completed probe with HTTP 200 and `ok:false` plus `stage`+`detail`, and
//! those two values are the whole point: they are what a user can act on. So the tests below drive a
//! mock that speaks exactly that contract -- one name per stage, plus a genuinely missing name that
//! answers 404 -- and check that the tool reports the first kind and keeps the daemon's words, while
//! still failing loudly on the second.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};
use ruagent_mcp::{BridgeConfig, PlatformTools};

const GHOST_DETAIL: &str = "`definitely-not-installed-xyz` is not on PATH. Install it, or set this runtime's `command` to the binary's full path (the PATH this daemon sees is the one it was started with).";
const MOCK_PATH: &str = "D:\\rust_cache\\debug\\ruagent-mock-agent.exe";
const SLOW_DETAIL: &str = "the ACP handshake did not finish within 3s (the process was alive and stayed alive) -- a harness that needs a login, or a machine too slow for the ceiling";

#[derive(Clone, Default)]
struct Mock {
    probes: Arc<Mutex<Vec<String>>>,
}

async fn probe(
    State(mock): State<Mock>,
    Path(name): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    mock.probes.lock().expect("mock state").push(name.clone());
    match name.as_str() {
        "ghost" => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": false,
                "command_path": null,
                "stage": "not-found",
                "detail": GHOST_DETAIL,
            })),
        ),
        "mock" => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "command_path": MOCK_PATH,
                "stage": "ok",
                "detail": "spawned, stayed alive and finished the ACP handshake",
            })),
        ),
        "slow" => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": false,
                "command_path": MOCK_PATH,
                "stage": "handshake-failed",
                "detail": SLOW_DETAIL,
            })),
        ),
        other => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("no agent or runtime named `{other}`")
            })),
        ),
    }
}

async fn list_agents() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "agents": [
            {
                "id": "01a40000-0000-7000-8000-0000000000aa",
                "name": "mock",
                "kind": "runtime",
                "enabled": true,
                "primary": false,
                "model": null,
                "description": "the scriptable test agent",
                "command_resolved": true,
                "command_path": MOCK_PATH,
            },
            {
                "id": "01a40000-0000-7000-8000-0000000000bb",
                "name": "ghost",
                "kind": "runtime",
                "enabled": true,
                "primary": false,
                "model": null,
                "description": "a runtime whose binary is not installed",
                "command_resolved": false,
                "command_path": null,
            },
        ]
    }))
}

async fn start_mock() -> (String, Mock) {
    let mock = Mock::default();
    let app = Router::new()
        .route("/api/v1/runtimes/{name}/probe", post(probe))
        .route("/api/v1/agents", get(list_agents))
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
async fn not_found_keeps_the_stage_and_the_daemons_detail() {
    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call(
                "runtime_probe",
                serde_json::json!({ "name": "ghost" }),
            ))
            .await
            .expect("a failed probe is a RESULT, not an MCP error"),
    );
    println!("READING t67 runtime_probe(ghost):\n{out}");
    assert!(
        out.contains("stage: not-found."),
        "the stage verbatim: {out}"
    );
    assert!(
        out.contains("definitely-not-installed-xyz"),
        "the daemon's detail must survive: {out}"
    );
    assert!(
        out.contains("can be searched") || out.contains("is not on PATH"),
        "{out}"
    );
    assert!(
        out.contains("Next:") && out.contains("install"),
        "a diagnosis without a next step is half an answer: {out}"
    );
    assert_eq!(
        mock.probes.lock().unwrap().as_slice(),
        ["ghost"],
        "one probe, one request"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn a_working_runtime_is_a_short_confirmation_with_the_path() {
    let (url, _mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call("runtime_probe", serde_json::json!({ "name": "mock" })))
            .await
            .unwrap(),
    );
    println!("READING t67 runtime_probe(mock):\n{out}");
    assert!(out.contains("RUNS"), "{out}");
    assert!(
        out.contains(MOCK_PATH),
        "the resolved path is the useful half: {out}"
    );
    assert!(!out.contains("CANNOT RUN"), "{out}");

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn handshake_failed_is_reported_verbatim_and_never_as_an_error() {
    let (url, _mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    // The daemon answered HTTP 200 with ok:false; if the tool treated that as a failure this call
    // would come back as Err and the stage would be lost.
    let out = text_of(
        &client
            .call_tool(call("runtime_probe", serde_json::json!({ "name": "slow" })))
            .await
            .expect("HTTP 200 + ok:false is a completed probe, not a failed request"),
    );
    println!("READING t67 runtime_probe(slow):\n{out}");
    assert!(out.contains("stage: handshake-failed."), "{out}");
    assert!(
        out.contains("the ACP handshake did not finish within 3s"),
        "the detail fragment must be the daemon's own sentence: {out}"
    );
    assert!(
        out.contains("RUAGENT_PROBE_TIMEOUT_MS"),
        "a timeout has a knob, and the next step must name it: {out}"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn a_name_that_does_not_exist_is_still_a_real_error() {
    let (url, _mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let msg = failure(
        client
            .call_tool(call("runtime_probe", serde_json::json!({ "name": "nope" })))
            .await,
    );
    println!("READING t67 runtime_probe(nope):\n{msg}");
    assert!(
        msg.contains("refused by the ruagent daemon (HTTP 404)"),
        "a request that did not work IS an error, and says why: {msg}"
    );
    assert!(
        msg.contains("no agent or runtime named"),
        "with the daemon's own words: {msg}"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn agents_list_says_which_runtime_cannot_be_resolved() {
    let (url, _mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call("agents_list", serde_json::json!({})))
            .await
            .unwrap(),
    );
    println!("READING t67 agents_list:\n{out}");
    assert!(
        out.contains(&format!("command_resolved=true  path={MOCK_PATH}")),
        "a runtime that resolves says so, with its path: {out}"
    );
    assert!(
        out.contains("command_resolved=FALSE"),
        "and the one that cannot run is the row a user needs: {out}"
    );
    assert!(
        out.contains("runtime_probe"),
        "the false row must point at the tool that explains it: {out}"
    );

    shutdown_pair(client, server_task).await;
}
