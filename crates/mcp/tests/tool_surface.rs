//! The MCP capability surface AS A CLIENT SEES IT (ruagent-tunable-capabilities
//! t17).
//!
//! Everything here is read from the published tool list of a served
//! `PlatformTools` — the JSON a client actually receives — rather than from the
//! crate's internals, because that is the part of the surface that can go stale
//! with nobody noticing: a field name, a field description and `additionalProperties`
//! are metadata that no parser in this repository reads.
//!
//! No daemon is started. `list_tools` is answered by the server itself, and the one
//! tool call below is refused at parameter extraction, before any HTTP request is
//! built (the second test proves that by using a dead address and checking the
//! message is not the transport one).

use std::sync::Arc;

use rmcp::model::CallToolRequestParams;
use rmcp::{ClientHandler, ServiceExt};
use ruagent_mcp::{BridgeConfig, PlatformTools};

type ClientHandle = rmcp::service::RunningService<rmcp::service::RoleClient, TestClient>;
type ServerHandle = tokio::task::JoinHandle<
    rmcp::service::RunningService<rmcp::service::RoleServer, PlatformTools>,
>;

struct TestClient;

impl ClientHandler for TestClient {
    fn get_info(&self) -> rmcp::model::ClientInfo {
        rmcp::model::ClientInfo::default()
    }
}

/// A dead address ON PURPOSE: listing tools must not dial anything, and a call
/// refused for an unsendable field must not dial either. The second test below
/// checks the refusal message is not the transport error, which is what proves it.
const NO_DAEMON: &str = "http://127.0.0.1:1";

async fn mcp_pair() -> (ClientHandle, ServerHandle) {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        PlatformTools::new(BridgeConfig {
            daemon_url: NO_DAEMON.to_string(),
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

/// The message of a call the client can SEE FAILED. Both shapes are legitimate and
/// both are failures a caller acts on: a JSON-RPC error (`Err`, what a handler-level
/// `Err(ErrorData)` produces) and a tool result with `is_error: true` (what rmcp's
/// own parameter extraction produces for a field the tool cannot send). A plain
/// `Ok` is the FALSE SUCCESS this file exists to remove, so it panics.
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

async fn capability_set_tool() -> (rmcp::model::Tool, Vec<rmcp::model::Tool>) {
    let (client, server_task) = mcp_pair().await;
    let tools = client.list_all_tools().await.unwrap();
    let tool = tools
        .iter()
        .find(|t| t.name.as_ref() == "capability_set")
        .expect("capability_set is served")
        .clone();
    shutdown_pair(client, server_task).await;
    (tool, tools)
}

/// The message for a divergence between the tool's option fields and the registry's
/// declared keys, or `None` when they agree — true in BOTH directions, which is the F4
/// fix (ruagent-tunable-capabilities t21).
///
/// A divergence has two shapes and they are reported on DIFFERENT lines: a sixth
/// registry key means the schema is MISSING a property (the key is unreachable from
/// MCP), while a renamed or stale field means the schema carries one the registry does
/// not declare. The previous shape checked list LENGTHS first and described both
/// directions as "the schema carries an EXTRA property", so for the direction that
/// matters most the report pointed at the wrong side. Extracted as a function so the
/// guard and the F4 control below exercise the SAME wording.
fn divergence_message(tool_keys: &[String], registry: &[String]) -> Option<String> {
    let missing: Vec<&str> = registry
        .iter()
        .map(String::as_str)
        .filter(|k| !tool_keys.iter().any(|t| t == k))
        .collect();
    let extra: Vec<&str> = tool_keys
        .iter()
        .map(String::as_str)
        .filter(|k| !registry.iter().any(|r| r == k))
        .collect();
    if missing.is_empty() && extra.is_empty() {
        return None;
    }
    Some(format!(
        "the tool's option fields and the registry's declared keys have diverged:\n  \
         the registry declares but this schema does NOT carry (a key unreachable from MCP): \
         {missing:?}\n  \
         this schema carries but the registry does NOT declare (a rename or a stale field): \
         {extra:?}\n  \
         fix `CapabilitySetParams` (crates/mcp/src/lib.rs) to match OptionKey::ALL."
    ))
}

/// t17 GUARD: the tool's option-field names must EQUAL the registry's declared keys
/// (`OptionKey::ALL`), and nothing in the published metadata may spell a bound.
///
/// WHAT IT PROTECTS. `capability_set`'s fields are the only way an option key can
/// reach `policy.toml` through MCP, while `OptionKey::ALL` is the registry's own
/// list of the keys that exist. If the registry declares a sixth key, this tool
/// cannot send it and it becomes unreachable from every MCP client — silently,
/// because nothing else in the tree ever compares the two. Today they coincide
/// exactly (measured by the design task), so this test is a RECOVERY PATH rather
/// than a bug report: the day they diverge it fails loudly and prints both sides,
/// instead of a caller meeting a key it cannot set.
///
/// WHY NOT A CONVENTION. The alternative was to trust that whoever adds a registry
/// key also remembers this struct — a comment, a review habit, a checklist. A
/// convention has no failure signal at all: the divergence stays invisible until a
/// user cannot set the new key, and by then the code that should have changed is
/// elsewhere. Both lists are in this workspace and both are machine-readable, so the
/// reliable guard is a comparison, the same reasoning as every other pinned list in
/// this crate.
///
/// The names are read from the PUBLISHED `inputSchema` (not from the Rust struct),
/// so a `#[serde(rename)]` that changes what a client must send is caught too.
///
/// MEASURED, THEN LEFT ALONE: schemars still publishes `minimum: 0` on the two u32
/// fields (`max_per_input`, `max_docs_per_pass`) and nothing numeric on the three
/// f64 ones (`weight`, `min_score`, `min_confidence`). Those come from the FIELD
/// TYPES, not from the registry, so they are not restated anywhere by hand and are
/// asserted here only where the t17 decision depends on them.
#[tokio::test]
async fn the_option_field_list_equals_the_registrys_declared_keys() {
    let (tool, _) = capability_set_tool().await;
    let props = tool
        .input_schema
        .get("properties")
        .and_then(|v| v.as_object())
        .expect("the tool publishes an object schema with properties");

    // The three arguments that are not option keys.
    let arguments = ["id", "enabled", "confirm_cost"];
    for a in arguments {
        assert!(
            props.contains_key(a),
            "capability_set lost its `{a}` argument: {:?}",
            props.keys().collect::<Vec<_>>()
        );
    }
    let mut tool_keys: Vec<String> = props
        .keys()
        .filter(|k| !arguments.contains(&k.as_str()))
        .cloned()
        .collect();
    tool_keys.sort();
    let mut registry: Vec<String> = ruagent_daemon::capability::OptionKey::ALL
        .iter()
        .map(|k| k.as_str().to_string())
        .collect();
    registry.sort();
    // Non-vacuity first: an empty registry compared against an over-eagerly filtered
    // property list would "pass" while comparing nothing.
    assert!(
        !registry.is_empty(),
        "OptionKey::ALL is empty — this guard would compare nothing"
    );
    // F4 (ruagent-tunable-capabilities t21): one assertion, and the message it builds
    // names the side that is actually wrong. The old shape compared lengths first and
    // called both directions "the schema carries an EXTRA property", so for a SIXTH
    // registry key — the direction this guard exists for — the accurate line was
    // unreachable and the report pointed at the wrong side.
    if let Some(msg) = divergence_message(&tool_keys, &registry) {
        panic!("{msg}");
    }

    // F4 CONTROL, on the same wording the guard just used: a sixth registry key lands on
    // the MISSING line and nowhere else; a stale field lands on the EXTRA line. Without
    // this, the F4 fix would be a claim rather than a property.
    let six_keys = {
        let mut r = registry.clone();
        r.push("decay_half_life".to_string());
        r
    };
    let msg = divergence_message(&tool_keys, &six_keys)
        .expect("a sixth registry key must be reported as a divergence");
    let missing_line = msg
        .lines()
        .find(|l| l.contains("does NOT carry"))
        .unwrap_or_else(|| panic!("the message does not name the missing side: {msg}"));
    assert!(
        missing_line.contains("decay_half_life"),
        "a sixth registry key must be named on the MISSING side: {msg}"
    );
    let extra_line = msg
        .lines()
        .find(|l| l.contains("does NOT declare"))
        .unwrap_or_else(|| panic!("the message does not name the extra side: {msg}"));
    assert!(
        !extra_line.contains("decay_half_life"),
        "F4 regression — the report names the WRONG side for a missing key: {msg}"
    );
    let stale = {
        let mut t = tool_keys.clone();
        t.push("stale_field".to_string());
        t
    };
    let msg = divergence_message(&stale, &registry).expect("a stale field must be reported");
    assert!(
        msg.lines()
            .find(|l| l.contains("does NOT declare"))
            .is_some_and(|l| l.contains("stale_field")),
        "a stale field must be named on the EXTRA side: {msg}"
    );
    assert!(
        divergence_message(&tool_keys, &registry).is_none(),
        "the control cannot observe agreement, so it would not detect anything"
    );

    // SENSITIVITY, so this guard cannot pass by comparing the wrong things: one name
    // added (a sixth registry key nobody wired) or one missing (a registry key this
    // tool cannot send) is exactly what the assertion above must reject. This is a
    // permanent control instead of a mutation window, because reddening shared source
    // to prove a test can fail is what this repository forbids.
    let mut with_extra = registry.clone();
    with_extra.push("decay_half_life".to_string());
    with_extra.sort();
    assert_ne!(
        tool_keys, with_extra,
        "the comparison ignores an extra key — it would not catch a sixth registry key"
    );
    let mut missing_one = registry.clone();
    missing_one.remove(0);
    assert_ne!(
        tool_keys, missing_one,
        "the comparison ignores a missing key — it would not catch a key this tool cannot send"
    );

    // t17 item 1's schema-level consequence: the extractor refuses a field that is
    // not in that list, and the published schema now SAYS so (schemars derives
    // `additionalProperties: false` from `#[serde(deny_unknown_fields)]`). Asserted
    // because it is the half of the fix a client can read without being refused
    // first.
    assert_eq!(
        tool.input_schema.get("additionalProperties"),
        Some(&serde_json::json!(false)),
        "the published schema must forbid properties the tool cannot send: {}",
        serde_json::to_string(&tool.input_schema).unwrap_or_default()
    );

    // t17 item 2: the five option-field descriptions point at the daemon's schema
    // and spell NO range. `..=` is the range syntax the old strings carried, and
    // `OptionKey::bounds()` is the only place a bound may live.
    for key in &tool_keys {
        let d = props[key.as_str()]
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert!(
            d.contains("options_schema"),
            "`{key}` does not point at the daemon's declared schema: {d}"
        );
        assert!(
            !d.contains("..="),
            "`{key}` spells a range that can go stale when OptionKey::bounds() moves: {d}"
        );
    }
}

/// t17 item 1: a field this tool cannot SEND is refused instead of silently
/// discarded — the measured defect was `capability_set(id=…, enabled=true,
/// decay_half_life=5.0)` answering SUCCESS while the key appeared neither in the
/// reply nor in `policy.toml`.
///
/// Two things this adds over the guard above: the refusal REACHES THE CALLER in the
/// shape rmcp uses (a tool result with `is_error: true` whose text names the
/// offending field AND every field the tool can send), and it happens BEFORE any
/// HTTP request is built — the daemon here is a dead address, and the control call
/// below shows what reaching the daemon looks like, so the refusal cannot be the
/// transport error wearing a disguise.
#[tokio::test]
async fn a_field_the_tool_cannot_send_is_refused_before_any_request() {
    let (client, server_task) = mcp_pair().await;

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
    assert!(
        err.contains("unknown field `decay_half_life`"),
        "the caller must be told WHICH key was not sent: {err}"
    );
    // ... and which fields CAN be sent, so the fix is visible in the message.
    for key in [
        "weight",
        "min_score",
        "max_per_input",
        "min_confidence",
        "max_docs_per_pass",
    ] {
        assert!(
            err.contains(key),
            "the refusal does not name `{key}`: {err}"
        );
    }
    assert!(
        !err.contains("cannot reach the ruagent daemon"),
        "the refusal must happen before any request, not after a failed one: {err}"
    );

    // CONTROL: the same call WITHOUT the unsendable field does reach the request
    // path (the address is dead, so THAT is the message a caller gets), which is
    // what makes the assertion above about the field rather than about `NO_DAEMON`.
    let transport = failure(
        client
            .call_tool(call(
                "capability_set",
                serde_json::json!({
                    "id": "recall_leg_memory_semantic",
                    "enabled": true,
                    "min_score": 0.4
                }),
            ))
            .await,
    );
    assert!(
        transport.contains("cannot reach the ruagent daemon"),
        "a sendable field must still travel to the daemon: {transport}"
    );

    shutdown_pair(client, server_task).await;
}

/// The schema-wide guard's WALKER, as a function over a tool LIST rather than an
/// assertion loop inside one test. That shape is the point: the sensitivity control
/// below runs this exact predicate against a synthetic tool that lacks the attribute,
/// which is what a tool added tomorrow would look like — no shared source is mutated to
/// demonstrate that.
///
/// The predicate is about the PUBLISHED schemas, not about the Rust structs: a struct
/// can grow `#[serde(rename)]`, or a tool can be added with no params type at all (the
/// shape `wiki_pages`/`wiki_links` had, whose empty-object schema accepts any field),
/// and only the schema shows it.
fn tools_that_accept_undeclared_fields(tools: &[rmcp::model::Tool]) -> Vec<String> {
    tools
        .iter()
        .filter(|t| t.input_schema.get("additionalProperties") != Some(&serde_json::json!(false)))
        .map(|t| t.name.to_string())
        .collect()
}

/// t21 (finding F1): NO tool may report success for a field it ignored — the same
/// caller-visible false success t17 fixed for `capability_set`, which a review then
/// measured on one tool after another: `capabilities_list`, `graph_search`,
/// `knowledge_search`, `list_tasks`, `memory_recall`, `memory_search` each returned
/// SUCCESS for a field they do not declare, and 17 of the 18 published schemas left
/// `additionalProperties` unset.
///
/// WHY ONE GUARD OVER EVERY SCHEMA AND NOT FOURTEEN ASSERTIONS: fourteen silent structs
/// is a PATTERN, not fourteen accidents — the attribute is per-struct and nothing else
/// in the tree notices when one is missing, so a per-tool test written alongside the
/// tools cannot catch the next one. This walks the whole published surface once, so the
/// nineteenth tool cannot skip the attribute unnoticed.
///
/// WHAT IT COVERS, AND WHAT IT DOES NOT (ruagent-tunable-capabilities t27, recording the
/// increment-6 review's measurement so the boundary is not mistaken for full coverage).
/// It reads the TOP-LEVEL `additionalProperties` of every published tool schema — the
/// property `#[serde(deny_unknown_fields)]` sets on the struct it is written on. A nested
/// OBJECT-TYPED field is NOT covered: unknown keys inside it are governed by the NESTED
/// type's own derive, not by the outer attribute, so a tool that grows one would pass
/// this guard while that field silently accepted anything.
///
/// MEASURED TODAY, so the caveat is a fact rather than a worry: no tool declares an
/// object-typed or array-of-object field. Across the 18 published schemas there are 39
/// properties and every one is a string, integer, number or boolean — or the nullable
/// union of one — with no array-typed property at all. The boundary is not being crossed
/// now; it is the next reader's to cross knowingly.
///
/// WHEN IT IS: extend this walker to descend into nested object schemas (`properties`,
/// and `items` for arrays of objects) and assert the same `additionalProperties: false`
/// at every level, keeping the tool name in the failure message.
///
/// MEASURED FIX, for the record: 15 params structs, of which only `CapabilitySetParams`
/// carried `#[serde(deny_unknown_fields)]`; the 14 others now carry it too, and
/// `wiki_pages`/`wiki_links` — which had NO params type, so their schema was an empty
/// object that discarded any field — now take `NoParams`, an empty struct with the same
/// attribute. Declared fields are unchanged everywhere (the two wiki tools declared
/// none before and declare none now).
#[tokio::test]
async fn every_tool_refuses_fields_it_does_not_declare() {
    let (client, server_task) = mcp_pair().await;
    let tools = client.list_all_tools().await.unwrap();
    shutdown_pair(client, server_task).await;

    // Non-vacuity: an empty list (a broken walker, an unserved tool set) would "pass".
    assert_eq!(
        tools.len(),
        18,
        "the walker ran over the wrong surface: {:?}",
        tools.iter().map(|t| t.name.to_string()).collect::<Vec<_>>()
    );

    // THE GUARD: the whole surface at once, and the failure names every silent tool.
    let silent = tools_that_accept_undeclared_fields(&tools);
    assert!(
        silent.is_empty(),
        "these tools would answer SUCCESS for a field they do not declare (add \
         #[serde(deny_unknown_fields)] to their params struct): {silent:?}"
    );

    // SENSITIVITY, through the SAME walker: a tool added tomorrow without the attribute,
    // and one whose schema allows everything, must both be flagged; today's schemas must
    // not be. This is the permanent control instead of an isolated worktree — reddening
    // shared source to prove a test can fail is what this repository forbids.
    let real = tools
        .iter()
        .find(|t| t.name.as_ref() == "capability_set")
        .expect("capability_set is served")
        .clone();
    let mut stripped = real.clone();
    stripped.name = "future_tool_without_the_attribute".into();
    Arc::make_mut(&mut stripped.input_schema).remove("additionalProperties");
    let mut permissive = real.clone();
    permissive.name = "future_tool_that_allows_anything".into();
    Arc::make_mut(&mut permissive.input_schema)
        .insert("additionalProperties".into(), serde_json::json!(true));
    assert_eq!(
        tools_that_accept_undeclared_fields(&[stripped]),
        vec!["future_tool_without_the_attribute".to_string()],
        "the walker would not catch a tool added later WITHOUT the attribute"
    );
    assert_eq!(
        tools_that_accept_undeclared_fields(&[permissive]),
        vec!["future_tool_that_allows_anything".to_string()],
        "the walker would not catch a schema whose additionalProperties is not `false`"
    );
    assert!(
        tools_that_accept_undeclared_fields(&[real]).is_empty(),
        "the walker flags a tool that DOES refuse undeclared fields"
    );

    // ... and the attribute is not decoration: the measured live example from the review
    // is now refused. `capabilities_list` is a READ tool, so a dropped field there was a
    // lying success; the refusal happens before any HTTP request (dead address), so the
    // message is serde's, not the transport one.
    let (client, server_task) = mcp_pair().await;
    let err = failure(
        client
            .call_tool(call(
                "capabilities_list",
                serde_json::json!({ "tier": "free", "skip_semantic": true }),
            ))
            .await,
    );
    assert!(
        err.contains("unknown field `skip_semantic`"),
        "a read tool must name the field it refused: {err}"
    );
    assert!(
        !err.contains("cannot reach the ruagent daemon"),
        "the refusal must happen before any request: {err}"
    );

    // The two tools whose SIGNATURE changed (`wiki_pages`, `wiki_links`: no params type
    // -> `NoParams`) must still accept an EMPTY argument set — they now refuse UNKNOWN
    // fields, not all fields. A dead address means reaching the request path is exactly
    // what a passing call looks like here.
    for name in ["wiki_pages", "wiki_links"] {
        let err = failure(client.call_tool(call(name, serde_json::json!({}))).await);
        assert!(
            err.contains("cannot reach the ruagent daemon"),
            "`{name}` must still accept an empty argument set and reach the request path: {err}"
        );
    }
    shutdown_pair(client, server_task).await;
}
