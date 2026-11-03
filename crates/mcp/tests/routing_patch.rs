//! routing_set_default sends ONE key (ruagent-issues-56-59 t62).
//!
//! WHY A MOCK: the acceptance is about the REQUEST BODY -- that this tool no longer re-sends the
//! user's rules, and that clearing sends an explicit empty value instead of dropping the key -- and
//! a body is only observable at a server boundary. The semantics the mock implements are the
//! daemon's new ones (PUT /api/v1/routing is a patch: an omitted key keeps its value, an empty
//! string clears it), so a tool that still sent the whole view would be caught here.

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};
use ruagent_mcp::{BridgeConfig, PlatformTools};

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    body: serde_json::Value,
}

#[derive(Clone)]
struct Mock {
    seen: Arc<Mutex<Vec<Seen>>>,
    /// The daemon's current view: one rule on "beta", default "beta".
    default: Arc<Mutex<Option<String>>>,
}

impl Mock {
    fn new() -> Self {
        Self {
            seen: Arc::new(Mutex::new(Vec::new())),
            default: Arc::new(Mutex::new(Some("beta".to_string()))),
        }
    }

    fn record(&self, method: &str, body: serde_json::Value) {
        self.seen.lock().expect("mock state").push(Seen {
            method: method.to_string(),
            body,
        });
    }

    fn writes(&self) -> Vec<serde_json::Value> {
        self.seen
            .lock()
            .expect("mock state")
            .iter()
            .filter(|s| s.method == "PUT")
            .map(|s| s.body.clone())
            .collect()
    }
}

fn rule() -> serde_json::Value {
    serde_json::json!({
        "project": "ruagent",
        "title_contains": "release",
        "agent": "beta"
    })
}

async fn read_routing(State(mock): State<Mock>) -> Json<serde_json::Value> {
    let default = mock.default.lock().expect("mock state").clone();
    Json(serde_json::json!({
        "default": default,
        "routes": [rule()],
    }))
}

/// The daemon's patch semantics, implemented in eleven lines: an omitted key is untouched, an empty
/// string clears.
async fn write_routing(
    State(mock): State<Mock>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    mock.record("PUT", body.clone());
    let mut current = mock.default.lock().expect("mock state");
    if let Some(value) = body.get("default") {
        *current = match value.as_str() {
            Some("") | None => None,
            Some(name) => Some(name.to_string()),
        };
    }
    let default = current.clone();
    Json(serde_json::json!({
        "default": default,
        "routes": [rule()],
        "takes_effect": "next run",
    }))
}

async fn primary(State(mock): State<Mock>) -> Json<serde_json::Value> {
    let name = mock.default.lock().expect("mock state").clone();
    Json(serde_json::json!({
        "name": name,
        "id": name.as_deref().map(|n| format!("id-of-{n}")),
        "source": "routing-default",
        "why": "the mock says so"
    }))
}

async fn start_mock() -> (String, Mock) {
    let mock = Mock::new();
    let app = Router::new()
        .route("/api/v1/routing", get(read_routing).put(write_routing))
        .route("/api/v1/primary", get(primary))
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

#[tokio::test]
async fn setting_the_default_sends_the_default_and_nothing_else() {
    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call(
                "routing_set_default",
                serde_json::json!({ "name": "alpha" }),
            ))
            .await
            .unwrap(),
    );
    println!("READING t62 routing_set_default(alpha):\n{out}");
    assert!(out.contains("wrote default = `alpha`"), "{out}");

    let writes = mock.writes();
    assert_eq!(writes.len(), 1, "exactly one write");
    let keys: Vec<&String> = writes[0]
        .as_object()
        .expect("an object body")
        .keys()
        .collect();
    assert_eq!(
        keys,
        vec!["default"],
        "the body must carry ONLY default -- re-sending the user's rules is a rewrite nobody asked          for: {}",
        writes[0]
    );
    assert!(
        writes[0].get("routes").is_none(),
        "routes must not be in the body at all: {}",
        writes[0]
    );
    assert_eq!(writes[0]["default"], "alpha");

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn clearing_sends_an_explicit_empty_value_instead_of_an_omitted_key() {
    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call(
                "routing_set_default",
                serde_json::json!({ "name": "none" }),
            ))
            .await
            .unwrap(),
    );
    println!("READING t62 routing_set_default(none):\n{out}");

    let writes = mock.writes();
    assert_eq!(writes.len(), 1, "exactly one write");
    let body = writes[0].as_object().expect("an object body");
    assert!(
        body.contains_key("default"),
        "the clear must be an EXPLICIT empty value -- an omitted key means leave it alone, so \
         dropping it would be a silent no-op: {}",
        writes[0]
    );
    assert_eq!(
        body["default"], "",
        "the clear travels as the empty string: {}",
        writes[0]
    );
    assert!(body.get("routes").is_none(), "{}", writes[0]);
    // And the clear really cleared in the daemon's own view, so the reply is not a lie.
    assert!(out.contains("default = <none>"), "{out}");

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn an_empty_name_clears_too_and_sends_the_same_explicit_empty() {
    let (url, mock) = start_mock().await;
    let (client, server_task) = served_tools(url).await;

    let _ = text_of(
        &client
            .call_tool(call(
                "routing_set_default",
                serde_json::json!({ "name": "" }),
            ))
            .await
            .unwrap(),
    );
    let writes = mock.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0]["default"], "", "{}", writes[0]);

    shutdown_pair(client, server_task).await;
}
