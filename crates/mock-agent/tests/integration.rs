//! End-to-end integration tests: the ruagent ACP client drives the mock
//! agent binary over real stdio JSON-RPC. No network, no API keys — this
//! is the CI backbone described in design §12.

use std::time::Duration;

use ruagent_acp::permission::{PermissionAnswer, PermissionAsk};
use ruagent_acp::{RunOptions, run_once};
use ruagent_core::{RunEvent, StopReason};
use tokio::sync::mpsc;

fn mock_opts(behavior: &str) -> RunOptions {
    RunOptions {
        program: env!("CARGO_BIN_EXE_ruagent-mock-agent").to_string(),
        args: vec!["--behavior".into(), behavior.into()],
        cwd: std::env::temp_dir(),
        mcp_servers: vec![],
        prompt: "hello mock".into(),
        options: vec![],
    }
}

struct Driven {
    result: Result<ruagent_acp::RunOutcome, ruagent_acp::AcpError>,
    events: Vec<RunEvent>,
}

/// Drive `opts` to completion, auto-answering permission asks per
/// `answer`, and collect the full normalized event stream.
async fn drive(opts: RunOptions, answer: Option<PermissionAnswer>) -> Driven {
    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel();
    let (ask_tx, mut ask_rx) = mpsc::unbounded_channel();

    // The decider owns PermissionResolved attribution (mirrors the
    // daemon's policy center).
    let decider_tx = ev_tx.clone();
    let answerer = tokio::spawn(async move {
        while let Some(ask) = ask_rx.recv().await {
            let PermissionAsk {
                tool_call_id,
                choices,
                answer: tx,
                ..
            } = ask;
            let ans = answer.clone().unwrap_or(PermissionAnswer::Cancel);
            if let PermissionAnswer::Select(id) = &ans
                && let Some(choice) = choices.iter().find(|c| &c.option_id == id)
            {
                let _ = decider_tx.send(RunEvent::PermissionResolved {
                    tool_call_id,
                    outcome: choice.kind,
                    resolution: ruagent_core::PermissionResolution::Rule {
                        rule_id: "test-policy".into(),
                    },
                });
            }
            let _ = tx.send(ans);
        }
    });

    let result = run_once(opts, ev_tx, ask_tx).await;

    // Drain the stream: events are unbounded-channel sends that may still
    // be in flight right after the run future resolves.
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(100), ev_rx.recv()).await {
            Ok(Some(ev)) => events.push(ev),
            Ok(None) => break,  // all senders dropped
            Err(_) => continue, // timeout, keep draining until deadline
        }
    }
    answerer.abort();
    Driven { result, events }
}

fn texts(events: &[RunEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            RunEvent::AgentMessageChunk { content } => Some(
                content
                    .iter()
                    .filter_map(|b| b.as_text())
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn echo_behavior_streams_events_and_completes() {
    let driven = drive(mock_opts("echo"), None).await;
    let outcome = driven.result.expect("echo run should succeed");
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
    assert_eq!(outcome.session_id, "mock-session-0");

    let texts = texts(&driven.events);
    assert_eq!(texts, vec!["echo: hello mock".to_string()]);

    let usage = driven.events.iter().find_map(|e| match e {
        RunEvent::UsageUpdate { usage } => Some(*usage),
        _ => None,
    });
    let usage = usage.expect("usage update expected");
    assert_eq!(usage.used, 120);
    assert_eq!(usage.size, 200_000);
    assert_eq!(usage.cost_usd, Some(0.001));
}

#[tokio::test]
async fn toolcall_behavior_reports_tool_and_output() {
    let driven = drive(mock_opts("toolcall"), None).await;
    let outcome = driven.result.expect("toolcall run should succeed");
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);

    let call = driven.events.iter().find_map(|e| match e {
        RunEvent::ToolCall {
            tool_call_id,
            title,
            raw_input,
        } => Some((tool_call_id.clone(), title.clone(), raw_input.clone())),
        _ => None,
    });
    let (id, title, input) = call.expect("tool call event expected");
    assert_eq!(id, "tc-1");
    assert_eq!(title, "Read file");
    assert_eq!(input["path"], "notes.txt");

    let update = driven.events.iter().find_map(|e| match e {
        RunEvent::ToolCallUpdate {
            tool_call_id,
            raw_output,
            ..
        } if tool_call_id == "tc-1" => raw_output.clone(),
        _ => None,
    });
    let output = update.expect("tool call update expected");
    assert_eq!(output["lines_read"], 42);
}

#[tokio::test]
async fn permission_allow_flow_round_trips() {
    let driven = drive(
        mock_opts("permission"),
        Some(PermissionAnswer::Select("allow-once".into())),
    )
    .await;
    driven.result.expect("permission run should succeed");

    let requested = driven
        .events
        .iter()
        .any(|e| matches!(e, RunEvent::PermissionRequested { tool_call_id, .. } if tool_call_id == "tc-perm"));
    assert!(requested, "PermissionRequested event expected");

    let resolved = driven.events.iter().find_map(|e| match e {
        RunEvent::PermissionResolved {
            tool_call_id,
            outcome,
            ..
        } if tool_call_id == "tc-perm" => Some(*outcome),
        _ => None,
    });
    assert_eq!(
        resolved,
        Some(ruagent_core::PermissionKind::AllowOnce),
        "PermissionResolved with allow expected"
    );

    let texts = texts(&driven.events);
    assert_eq!(texts, vec!["permission allowed".to_string()]);
}

#[tokio::test]
async fn permission_reject_flow_round_trips() {
    let driven = drive(
        mock_opts("permission"),
        Some(PermissionAnswer::Select("reject-once".into())),
    )
    .await;
    driven.result.expect("run should still complete");
    let texts = texts(&driven.events);
    assert_eq!(texts, vec!["permission rejected".to_string()]);
}

#[tokio::test]
async fn permission_cancel_fails_closed() {
    // No answer policy at all: the ask channel receiver drops immediately
    // in `drive` (answer=None still answers Cancel), but test the raw
    // semantics: answering Cancel keeps the run alive and the mock
    // reports "cancelled".
    let driven = drive(mock_opts("permission"), Some(PermissionAnswer::Cancel)).await;
    driven.result.expect("run should complete after cancel");
    let texts = texts(&driven.events);
    assert_eq!(texts, vec!["permission cancelled".to_string()]);
}

#[tokio::test]
async fn plan_behavior_publishes_plan() {
    let driven = drive(mock_opts("plan"), None).await;
    driven.result.expect("plan run should succeed");
    let plan = driven.events.iter().find_map(|e| match e {
        RunEvent::Plan { entries } => Some(entries.clone()),
        _ => None,
    });
    let plan = plan.expect("plan event expected");
    assert_eq!(plan.len(), 3);
    assert_eq!(plan[0].content, "analyze the request");
    assert_eq!(plan[0].status, ruagent_core::PlanEntryStatus::Completed);
    assert_eq!(plan[2].status, ruagent_core::PlanEntryStatus::Pending);
}

#[tokio::test]
async fn crash_behavior_surfaces_as_error() {
    let driven = drive(mock_opts("crash"), None).await;
    assert!(
        driven.result.is_err(),
        "crashed agent must surface as an error, not success"
    );
    // We should still have seen the first chunk before the crash.
    assert!(!texts(&driven.events).is_empty());
}

#[tokio::test]
async fn split_command_line_parses() {
    let (program, args) = ruagent_acp::split_command_line("dsh --profile acp");
    assert_eq!(program, "dsh");
    assert_eq!(args, vec!["--profile".to_string(), "acp".to_string()]);
}
