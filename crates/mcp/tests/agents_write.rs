//! The v3 tools: create, change and delete the agents themselves (ruagent-issues-56-59 t54).
//!
//! WHY A STATEFUL MOCK. Two of the acceptance points are about what this tool SENDS (does an
//! update carry only the keys the caller gave?) and two are about what it REFUSES BEFORE writing
//! (the front door, a runtime roles still list) -- so the server has to both remember requests and
//! hold a registry the tool can read. The mock therefore owns a small agents list plus the front
//! door, refuses a role without a prompt with the daemon's OWN sentence, and records every request.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};
use ruagent_mcp::{BridgeConfig, PlatformTools};

const DOOR: &str = "alpha";

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    body: serde_json::Value,
}

#[derive(Clone)]
struct Mock {
    seen: Arc<Mutex<Vec<Seen>>>,
    cards: Arc<Mutex<Vec<serde_json::Value>>>,
    door: Arc<Mutex<Option<String>>>,
}

fn card(name: &str, runtimes: &[&str], primary: bool) -> serde_json::Value {
    serde_json::json!({
        "id": format!("01a30000-0000-7000-8000-0000000000{n}",
            n = match name { "alpha" => "a1", "beta" => "b2", "local" => "l3", _ => "zz" }),
        "name": name,
        "kind": if runtimes.is_empty() { "runtime" } else { "role" },
        "enabled": true,
        "primary": primary,
        "model": null,
        "description": null,
        "runtimes": runtimes,
        "prompt": null,
    })
}

impl Mock {
    fn new(cards: Vec<serde_json::Value>, door: Option<&str>) -> Self {
        Self {
            seen: Arc::new(Mutex::new(Vec::new())),
            cards: Arc::new(Mutex::new(cards)),
            door: Arc::new(Mutex::new(door.map(str::to_string))),
        }
    }

    fn record(&self, method: &str, path: String, body: serde_json::Value) {
        self.seen.lock().expect("mock state").push(Seen {
            method: method.to_string(),
            path,
            body,
        });
    }

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
}

async fn list_agents(State(mock): State<Mock>) -> Json<serde_json::Value> {
    let cards = mock.cards.lock().expect("mock state").clone();
    Json(serde_json::json!({ "agents": cards }))
}

async fn primary(State(mock): State<Mock>) -> Json<serde_json::Value> {
    let name = mock.door.lock().expect("mock state").clone();
    Json(serde_json::json!({
        "name": name,
        "id": name.as_deref().map(|n| format!("id-of-{n}")),
        "source": "routing-default",
        "why": "the mock says so"
    }))
}

async fn create_agent(
    State(mock): State<Mock>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    mock.record("POST", "/api/v1/agents".to_string(), body.clone());
    let prompt = body.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    if prompt.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "a role needs a prompt \u{2014} that is what makes it a role"
            })),
        );
    }
    let runtimes: Vec<serde_json::Value> = body
        .get("runtimes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if runtimes.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "a role needs at least one runtime \u{2014} create one on the runtimes page first"
            })),
        );
    }
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    let new = serde_json::json!({
        "id": "01a30000-0000-7000-8000-0000000000ff",
        "name": name,
        "kind": "role",
        "enabled": true,
        "primary": false,
        "model": body.get("model").cloned().unwrap_or(serde_json::Value::Null),
        "description": body.get("description").cloned().unwrap_or(serde_json::Value::Null),
        "runtimes": runtimes,
        "prompt": prompt,
    });
    mock.cards.lock().expect("mock state").push(new.clone());
    (StatusCode::CREATED, Json(new))
}

async fn update_agent(
    State(mock): State<Mock>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    mock.record("PUT", format!("/api/v1/agents/{name}"), body.clone());
    let mut cards = mock.cards.lock().expect("mock state");
    let Some(row) = cards
        .iter_mut()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name.as_str()))
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("agent {name} not found") })),
        );
    };
    if let (Some(target), Some(given)) = (row.as_object_mut(), body.as_object()) {
        for (key, value) in given {
            target.insert(key.clone(), value.clone());
        }
    }
    let out = row.clone();
    (StatusCode::OK, Json(out))
}

async fn delete_agent(
    State(mock): State<Mock>,
    Path(name): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    mock.record(
        "DELETE",
        format!("/api/v1/agents/{name}"),
        serde_json::Value::Null,
    );
    let mut cards = mock.cards.lock().expect("mock state");
    let before = cards.len();
    cards.retain(|c| c.get("name").and_then(|v| v.as_str()) != Some(name.as_str()));
    if cards.len() == before {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("agent {name} not found") })),
        );
    }
    (StatusCode::NO_CONTENT, Json(serde_json::json!(null)))
}

async fn start_mock(cards: Vec<serde_json::Value>, door: Option<&str>) -> (String, Mock) {
    let mock = Mock::new(cards, door);
    let app = Router::new()
        .route("/api/v1/agents", get(list_agents).post(create_agent))
        .route(
            "/api/v1/agents/{name}",
            axum::routing::put(update_agent).delete(delete_agent),
        )
        .route("/api/v1/primary", get(primary))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://{addr}"), mock)
}

/// alpha is the front door and a role on "local"; beta is a role on "local" too; "local" is the
/// runtime both of them list.
fn seed() -> Vec<serde_json::Value> {
    vec![
        card("alpha", &["local"], true),
        card("beta", &["local"], false),
        card("local", &[], false),
    ]
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
async fn agent_create_returns_the_new_id_and_sends_only_what_was_given() {
    let (url, mock) = start_mock(seed(), Some(DOOR)).await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call(
                "agent_create",
                serde_json::json!({
                    "name": "reviewer",
                    "prompt": "review diffs for correctness",
                    "runtimes": ["local"],
                    "model": "model-x"
                }),
            ))
            .await
            .unwrap(),
    );
    println!("READING t54 agent_create:\n{out}");
    assert!(
        out.contains("id: 01a30000-0000-7000-8000-0000000000ff"),
        "the new card's registry id must be in the reply: {out}"
    );
    assert!(out.contains("created `reviewer`"), "{out}");

    let bodies = mock.bodies_at("POST", "/api/v1/agents");
    assert_eq!(bodies.len(), 1, "one create request");
    assert_eq!(bodies[0]["name"], "reviewer");
    assert_eq!(bodies[0]["prompt"], "review diffs for correctness");
    assert_eq!(bodies[0]["model"], "model-x");
    assert!(
        bodies[0].get("description").is_none(),
        "an omitted description must stay omitted: {}",
        bodies[0]
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn agent_create_without_a_prompt_keeps_the_daemons_own_sentence() {
    let (url, mock) = start_mock(seed(), Some(DOOR)).await;
    let (client, server_task) = served_tools(url).await;

    let msg = failure(
        client
            .call_tool(call(
                "agent_create",
                serde_json::json!({ "name": "noprompt", "runtimes": ["local"] }),
            ))
            .await,
    );
    println!("READING t54 agent_create(no prompt):\n{msg}");
    assert!(
        msg.contains("a role needs a prompt"),
        "the daemon's own reason must survive: {msg}"
    );
    assert!(
        msg.contains("that is what makes it a role"),
        "verbatim, not paraphrased: {msg}"
    );

    // The second refusal the daemon owns, for the same reason: nothing was created.
    let no_runtime = failure(
        client
            .call_tool(call(
                "agent_create",
                serde_json::json!({ "name": "noruntime", "prompt": "x" }),
            ))
            .await,
    );
    println!("READING t54 agent_create(no runtime):\n{no_runtime}");
    assert!(no_runtime.contains("at least one runtime"), "{no_runtime}");
    assert_eq!(
        mock.count_at("POST", "/api/v1/agents"),
        2,
        "two attempts, no writes"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn agent_update_sends_only_the_keys_the_caller_gave() {
    let (url, mock) = start_mock(seed(), Some(DOOR)).await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call(
                "agent_update",
                serde_json::json!({ "name": "beta", "model": "model-cheap" }),
            ))
            .await
            .unwrap(),
    );
    println!("READING t54 agent_update:\n{out}");
    assert!(out.contains("updated `beta`"), "{out}");
    assert!(out.contains("fields sent: model"), "{out}");

    let bodies = mock.bodies_at("PUT", "/api/v1/agents/beta");
    assert_eq!(bodies.len(), 1, "one update request");
    let keys: Vec<&String> = bodies[0]
        .as_object()
        .expect("an object body")
        .keys()
        .collect();
    assert_eq!(
        keys,
        vec!["model"],
        "the request must carry ONLY the key the caller gave -- a null for the rest would clear          the card instead of editing one field: {}",
        bodies[0]
    );
    assert_eq!(bodies[0]["model"], "model-cheap");

    // A two-field change sends two keys, and still nothing else.
    let _ = text_of(
        &client
            .call_tool(call(
                "agent_update",
                serde_json::json!({ "name": "beta", "prompt": "be brief", "enabled": false }),
            ))
            .await
            .unwrap(),
    );
    let bodies = mock.bodies_at("PUT", "/api/v1/agents/beta");
    let mut keys: Vec<String> = bodies[1]
        .as_object()
        .expect("an object body")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["enabled".to_string(), "prompt".to_string()],
        "{}",
        bodies[1]
    );

    // And an update that changes nothing is refused rather than sent as an empty rewrite.
    let empty = failure(
        client
            .call_tool(call("agent_update", serde_json::json!({ "name": "beta" })))
            .await,
    );
    println!("READING t54 agent_update(empty):\n{empty}");
    assert!(empty.contains("nothing to change"), "{empty}");
    assert_eq!(
        mock.count_at("PUT", "/api/v1/agents/beta"),
        2,
        "the empty update never went out"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn agent_delete_refuses_the_front_door_and_names_the_next_step() {
    let (url, mock) = start_mock(seed(), Some(DOOR)).await;
    let (client, server_task) = served_tools(url).await;

    let msg = failure(
        client
            .call_tool(call("agent_delete", serde_json::json!({ "name": "alpha" })))
            .await,
    );
    println!("READING t54 agent_delete(front door):\n{msg}");
    assert!(msg.contains("CURRENT FRONT DOOR"), "{msg}");
    assert!(
        msg.contains("agent_set_primary"),
        "the refusal must say HOW to make it deletable: {msg}"
    );
    assert!(
        msg.contains("answering them"),
        "and WHY it refuses: deleting it would remove the agent the user is talking to: {msg}"
    );
    assert_eq!(
        mock.count_at("DELETE", "/api/v1/agents/alpha"),
        0,
        "nothing may be deleted when the pre-check refuses"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn agent_delete_refuses_a_runtime_that_roles_still_list() {
    let (url, mock) = start_mock(seed(), Some(DOOR)).await;
    let (client, server_task) = served_tools(url).await;

    let msg = failure(
        client
            .call_tool(call("agent_delete", serde_json::json!({ "name": "local" })))
            .await,
    );
    println!("READING t54 agent_delete(referenced runtime):\n{msg}");
    assert!(msg.contains("RUNTIME"), "{msg}");
    assert!(
        msg.contains("alpha") && msg.contains("beta"),
        "every dependent role must be named: {msg}"
    );
    assert!(
        msg.contains("agent_update"),
        "the refusal must say HOW to free it: {msg}"
    );
    assert_eq!(
        mock.count_at("DELETE", "/api/v1/agents/local"),
        0,
        "nothing may be deleted when the pre-check refuses"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn agent_delete_removes_an_unused_agent_and_reports_what_is_left() {
    let (url, mock) = start_mock(seed(), Some(DOOR)).await;
    let (client, server_task) = served_tools(url).await;

    let out = text_of(
        &client
            .call_tool(call("agent_delete", serde_json::json!({ "name": "beta" })))
            .await
            .unwrap(),
    );
    println!("READING t54 agent_delete(beta):\n{out}");
    assert!(out.contains("deleted `beta`"), "{out}");
    assert!(
        out.contains("still registered (2)") && out.contains("alpha") && out.contains("local"),
        "the post-state must be read back, not assumed: {out}"
    );
    assert!(
        !out.contains("beta,"),
        "the deleted agent must not be listed as remaining: {out}"
    );
    assert_eq!(
        mock.count_at("DELETE", "/api/v1/agents/beta"),
        1,
        "one delete request"
    );

    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn an_unreachable_daemon_is_reported_by_all_three_agent_tools() {
    let (client, server_task) = served_tools("http://127.0.0.1:1".to_string()).await;
    for (tool, args) in [
        (
            "agent_create",
            serde_json::json!({ "name": "x", "prompt": "p", "runtimes": ["r"] }),
        ),
        (
            "agent_update",
            serde_json::json!({ "name": "x", "model": "m" }),
        ),
        ("agent_delete", serde_json::json!({ "name": "x" })),
    ] {
        let msg = failure(client.call_tool(call(tool, args)).await);
        println!("READING t54 {tool} against a dead daemon: {msg}");
        assert!(
            msg.contains("cannot reach the ruagent daemon"),
            "{tool} must report the transport failure in words a caller can act on: {msg}"
        );
    }
    shutdown_pair(client, server_task).await;
}

#[tokio::test]
async fn the_agent_tools_say_what_they_cost_and_what_they_change() {
    let (url, _mock) = start_mock(seed(), Some(DOOR)).await;
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

    for name in ["agent_create", "agent_update", "agent_delete"] {
        let d = description(name);
        assert!(d.contains("WHEN:"), "{name} must say when: {d}");
        assert!(d.contains("COST:"), "{name} must name its cost tier: {d}");
        assert!(
            d.contains("zero model tokens"),
            "{name} spends no model tokens and must say so: {d}"
        );
    }
    let create = description("agent_create");
    assert!(
        create.contains("A ROLE NEEDS BOTH A PROMPT AND AT LEAST ONE RUNTIME"),
        "the two refusals must be predictable BEFORE the call: {create}"
    );
    assert!(
        create.contains("DOES NOT MAKE IT THE FRONT DOOR"),
        "creating must not be mistaken for choosing who answers: {create}"
    );
    let update = description("agent_update");
    assert!(
        update
            .to_lowercase()
            .contains("say what you are changing out loud"),
        "changing an agent can change the user's answers, so announce it: {update}"
    );
    assert!(
        update.contains("ONLY THE FIELDS YOU PASS ARE SENT"),
        "the patch semantics must be stated: {update}"
    );
    let delete = description("agent_delete");
    assert!(
        delete.to_lowercase().contains("say it out loud"),
        "deleting is destructive, so announce it: {delete}"
    );

    shutdown_pair(client, server_task).await;
}
