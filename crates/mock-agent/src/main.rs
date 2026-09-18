//! Mock ACP agent binary — see `ruagent_mock_agent` lib docs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, Cost, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionId,
    PermissionOptionKind, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus, PromptRequest,
    PromptResponse, RequestPermissionOutcome, RequestPermissionRequest, SessionId,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, StopReason, TextContent, ToolCall, ToolCallId, ToolCallUpdate,
    ToolCallUpdateFields, UsageUpdate,
};
use agent_client_protocol::{Agent, Result, Stdio};

use ruagent_mock_agent::{Behavior, MockArgs, advertise, default_current, prompt_text, set_option};

#[tokio::main]
async fn main() -> Result<()> {
    let args = MockArgs::from_args();
    let behavior = args.behavior;
    let scripted_replies = Arc::new(args.replies);
    eprintln!("mock-agent starting, behavior: {behavior:?}");
    let session_counter = Arc::new(AtomicU64::new(0));
    // Current session-option values (model / mode / reasoning effort).
    let current = Arc::new(std::sync::Mutex::new(default_current()));

    Agent
        .builder()
        .name("ruagent-mock-agent")
        .on_receive_request(
            async |req: InitializeRequest, responder, _conn| {
                responder.respond(
                    InitializeResponse::new(req.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let counter = session_counter.clone();
                let current = current.clone();
                async move |req: NewSessionRequest, responder, _conn| {
                    let _ = &req.cwd;
                    let n = counter.fetch_add(1, Ordering::SeqCst);
                    let options = advertise(&current.lock().expect("options lock"));
                    responder.respond(
                        NewSessionResponse::new(SessionId::new(format!("mock-session-{n}")))
                            .config_options(options),
                    )
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let current = current.clone();
                async move |req: SetSessionConfigOptionRequest, responder, _conn| {
                    let Some(value) = req.value.as_value_id() else {
                        responder.respond_with_error(
                            agent_client_protocol::Error::invalid_params()
                                .data("mock options are select-style"),
                        )?;
                        return Ok(());
                    };
                    let mut cur = current.lock().expect("options lock");
                    match set_option(&mut cur, &req.config_id.to_string(), &value.to_string()) {
                        Ok(()) => {
                            responder.respond(SetSessionConfigOptionResponse::new(advertise(&cur)))
                        }
                        Err(e) => responder.respond_with_error(
                            agent_client_protocol::Error::invalid_params().data(e),
                        ),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let scripted_replies = scripted_replies.clone();
                async move |req: PromptRequest, responder, conn| {
                    let sid = req.session_id.clone();
                    let text = prompt_text(&req.prompt);
                    let notify = |update: SessionUpdate| -> Result<()> {
                        conn.send_notification(SessionNotification::new(sid.clone(), update))
                    };

                    match behavior {
                        Behavior::Scripted => {
                            // First marker contained in the prompt wins;
                            // the script is keyed by content because every
                            // ruagent one-shot call spawns a fresh process.
                            let reply = scripted_replies
                                .iter()
                                .find(|r| text.contains(&r.marker))
                                .map(|r| r.reply.clone())
                                .unwrap_or_else(|| {
                                    format!(
                                        "no scripted reply for this prompt: {}",
                                        &text[..text.len().min(80)]
                                    )
                                });
                            notify(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(reply)),
                            )))?;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        }
                        Behavior::Echo => {
                            notify(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(format!("echo: {text}"))),
                            )))?;
                            notify(SessionUpdate::UsageUpdate(
                                UsageUpdate::new(120, 200_000).cost(Cost::new(0.001, "USD")),
                            ))?;
                            // Real agents stream chunks for a while before
                            // their final response; an instant response
                            // races the client's dispatch and loses chunks.
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        }
                        Behavior::ToolCall => {
                            notify(SessionUpdate::ToolCall(
                                ToolCall::new(ToolCallId::new("tc-1"), "Read file")
                                    .raw_input(serde_json::json!({"path": "notes.txt"})),
                            ))?;
                            notify(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                                ToolCallId::new("tc-1"),
                                ToolCallUpdateFields::new()
                                    .raw_output(serde_json::json!({"lines_read": 42})),
                            )))?;
                            notify(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(format!(
                                    "done after tool call (prompt was: {text})"
                                ))),
                            )))?;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        }
                        Behavior::Permission => {
                            notify(SessionUpdate::ToolCall(
                                ToolCall::new(ToolCallId::new("tc-perm"), "Write file").raw_input(
                                    serde_json::json!({"path": "out.txt", "content": "hi"}),
                                ),
                            ))?;
                            let ask = RequestPermissionRequest::new(
                                sid.clone(),
                                ToolCallUpdate::new(
                                    ToolCallId::new("tc-perm"),
                                    ToolCallUpdateFields::new()
                                        .title("Write file")
                                        .raw_input(serde_json::json!({"path": "out.txt"})),
                                ),
                                vec![
                                    PermissionOption::new(
                                        PermissionOptionId::new("allow-once"),
                                        "Allow",
                                        PermissionOptionKind::AllowOnce,
                                    ),
                                    PermissionOption::new(
                                        PermissionOptionId::new("reject-once"),
                                        "Reject",
                                        PermissionOptionKind::RejectOnce,
                                    ),
                                ],
                            );
                            // The permission round-trip must NOT block the
                            // dispatch loop (SDK ordering docs: a handler
                            // awaiting its own request's response deadlocks).
                            // Answer the prompt from the response callback.
                            let sid_cb = sid.clone();
                            let conn_cb = conn.clone();
                            conn.send_request(ask)
                                .on_receiving_result(async move |result| {
                                    let answer = result?;
                                    let verdict = match answer.outcome {
                                        RequestPermissionOutcome::Selected(sel) => {
                                            let id: String = sel.option_id.to_string();
                                            if id == "allow-once" {
                                                "allowed"
                                            } else {
                                                "rejected"
                                            }
                                        }
                                        _ => "cancelled",
                                    };
                                    conn_cb.send_notification(SessionNotification::new(
                                        sid_cb,
                                        SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                            ContentBlock::Text(TextContent::new(format!(
                                                "permission {verdict}"
                                            ))),
                                        )),
                                    ))?;
                                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                                })?;
                            Ok(())
                        }
                        Behavior::Plan => {
                            notify(SessionUpdate::Plan(Plan::new(vec![
                                PlanEntry::new(
                                    "analyze the request",
                                    PlanEntryPriority::High,
                                    PlanEntryStatus::Completed,
                                ),
                                PlanEntry::new(
                                    "write the answer",
                                    PlanEntryPriority::Medium,
                                    PlanEntryStatus::InProgress,
                                ),
                                PlanEntry::new(
                                    "clean up",
                                    PlanEntryPriority::Low,
                                    PlanEntryStatus::Pending,
                                ),
                            ])))?;
                            notify(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new("planned work")),
                            )))?;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        }
                        Behavior::Approve => {
                            notify(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new("ALLOW")),
                            )))?;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        }
                        Behavior::Judge => {
                            // Pick the first [RUN <id>] candidate: the
                            // daemon's judge prompt lists them in run order,
                            // so tests get a deterministic winner.
                            let reply = match ruagent_mock_agent::judge_candidates(&text).first() {
                                Some(id) => {
                                    format!(
                                        "RUN: {id}\nWHY: mock judge prefers the first candidate"
                                    )
                                }
                                None => "no candidates in prompt".to_string(),
                            };
                            notify(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(reply)),
                            )))?;
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        }
                        Behavior::Crash => {
                            notify(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new("about to crash")),
                            )))?;
                            // Give the writer a moment to flush, then die.
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            std::process::exit(1);
                        }
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(Stdio::new())
        .await
}
