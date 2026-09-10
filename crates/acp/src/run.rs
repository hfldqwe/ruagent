//! Connection-per-run driver: spawn an agent process, initialize, run one
//! prompt to completion, stream normalized events. See design §4.3.

use std::path::PathBuf;

use agent_client_protocol::schema::v1::{
    ContentBlock, InitializeRequest, McpServer, NewSessionRequest, PromptRequest,
    ReadTextFileRequest, ReadTextFileResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionNotification, TextContent,
    WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo, LineDirection};
use ruagent_core::RunEvent;
use tokio::sync::mpsc;

use crate::AcpError;
use crate::permission::{PermissionAnswer, PermissionAsk};

/// Options for a single run against a freshly spawned agent process.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Executable to spawn, e.g. `dsh` or an absolute path.
    pub program: String,
    /// Arguments passed to the program, e.g. `["--profile", "acp"]`.
    pub args: Vec<String>,
    /// Working directory for the session (absolute).
    pub cwd: PathBuf,
    /// MCP servers injected via `session/new` (design §7.1).
    pub mcp_servers: Vec<McpServer>,
    /// The prompt text.
    pub prompt: String,
}

/// Split a command line into program + args on whitespace.
///
/// M1 keeps this deliberately dumb (no quoting); harness adapters with
/// special needs pass structured [`RunOptions`] fields instead.
pub fn split_command_line(command: &str) -> (String, Vec<String>) {
    let mut parts = command.split_whitespace();
    let program = parts.next().unwrap_or_default().to_string();
    let args = parts.map(str::to_string).collect();
    (program, args)
}

/// Terminal summary of a completed run.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub session_id: String,
    pub stop_reason: ruagent_core::StopReason,
}

/// Drive one prompt through a freshly spawned agent.
///
/// Events stream out on `event_tx` as they arrive; permission requests are
/// surfaced on `ask_tx` and the run parks until the daemon answers. The
/// returned future resolves when the prompt completes or the connection
/// fails.
pub async fn run_once(
    opts: RunOptions,
    event_tx: mpsc::UnboundedSender<RunEvent>,
    ask_tx: mpsc::UnboundedSender<PermissionAsk>,
) -> Result<RunOutcome, AcpError> {
    let agent =
        AcpAgent::from_args(std::iter::once(opts.program.clone()).chain(opts.args.iter().cloned()))
            .map_err(|e| AcpError::Command(format!("{}: {e}", opts.program)))?;
    // Surface agent stderr in our logs — invaluable for diagnosing
    // harness startup failures (npx downloads, auth, config).
    let agent = agent.with_debug(|line, dir| {
        if dir == LineDirection::Stderr {
            tracing::debug!(target: "ruagent::acp::stderr", "{line}");
        }
    });

    let ev_notification = event_tx.clone();
    let ev_permission = event_tx.clone();
    let ask = ask_tx.clone();
    let read_root = opts.cwd.clone();
    let write_root = opts.cwd.clone();

    Client
        .builder()
        .name("ruagent")
        .on_receive_notification(
            {
                let ev = ev_notification.clone();
                async move |n: SessionNotification, _cx| {
                    if let Some(event) = crate::map::session_update(n.update) {
                        let _ = ev.send(event);
                    }
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            {
                let ask = ask.clone();
                let ev = ev_permission.clone();
                async move |req: RequestPermissionRequest, responder, cx| {
                    let tool = &req.tool_call;
                    let title = tool
                        .fields
                        .title
                        .clone()
                        .unwrap_or_else(|| "unnamed tool".into());
                    let raw_input = tool
                        .fields
                        .raw_input
                        .clone()
                        .unwrap_or(serde_json::Value::Null);
                    let tool_call_id = tool.tool_call_id.to_string();
                    let choices: Vec<ruagent_core::PermissionChoice> = req
                        .options
                        .iter()
                        .map(|o| ruagent_core::PermissionChoice {
                            option_id: o.option_id.to_string(),
                            name: o.name.clone(),
                            kind: crate::map::permission_kind(o.kind),
                        })
                        .collect();
                    let _ = ev.send(ruagent_core::RunEvent::PermissionRequested {
                        tool_call_id: tool_call_id.clone(),
                        title: title.clone(),
                        raw_input: raw_input.clone(),
                        choices: choices.clone(),
                    });

                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let ask_msg = PermissionAsk {
                        tool_call_id: tool_call_id.clone(),
                        title,
                        raw_input,
                        choices: choices.clone(),
                        answer: tx,
                    };
                    // Never hold the dispatch loop while waiting for the
                    // policy center (a human may take minutes): forward the
                    // ask, then answer from a spawned task so the loop keeps
                    // processing messages (SDK ordering docs — deadlock risk).
                    if ask.send(ask_msg).is_err() {
                        // Daemon is gone: fail closed.
                        return responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Cancelled,
                        ));
                    }
                    let ev_spawn = ev.clone();
                    cx.spawn(async move {
                        let answer = rx.await.unwrap_or(PermissionAnswer::Cancel);
                        let _ = ev_spawn.send(ruagent_core::RunEvent::PermissionResolved {
                            tool_call_id: tool_call_id.clone(),
                            outcome: match &answer {
                                PermissionAnswer::Select(id) => choices
                                    .iter()
                                    .find(|c| &c.option_id == id)
                                    .map(|c| c.kind)
                                    .unwrap_or(ruagent_core::PermissionKind::RejectOnce),
                                PermissionAnswer::Cancel => {
                                    ruagent_core::PermissionKind::RejectOnce
                                }
                            },
                            resolution: ruagent_core::PermissionResolution::Rule {
                                rule_id: "m1-policy".into(),
                            },
                        });
                        match answer {
                            PermissionAnswer::Select(id) => responder.respond(
                                RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
                                    SelectedPermissionOutcome::new(id),
                                )),
                            ),
                            PermissionAnswer::Cancel => responder.respond(
                                RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled),
                            ),
                        }
                    })
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let root = read_root;
                async move |req: ReadTextFileRequest, responder, _cx| {
                    match crate::fs_tools::resolve_existing_under(&root, &req.path) {
                        Ok(path) => match tokio::fs::read_to_string(&path).await {
                            Ok(content) => responder.respond(ReadTextFileResponse::new(
                                crate::fs_tools::apply_line_window(&content, req.line, req.limit),
                            )),
                            Err(e) => {
                                tracing::warn!(target: "ruagent::acp::fs", "read failed: {e}");
                                responder.respond_with_internal_error(format!("{e}"))
                            }
                        },
                        Err(e) => {
                            tracing::warn!(target: "ruagent::acp::fs", "read rejected: {e}");
                            responder.respond_with_internal_error(format!("{e}"))
                        }
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let root = write_root;
                async move |req: WriteTextFileRequest, responder, _cx| {
                    match crate::fs_tools::resolve_new_under(&root, &req.path) {
                        Ok(path) => {
                            if let Some(parent) = path.parent()
                                && let Err(e) = tokio::fs::create_dir_all(parent).await
                            {
                                return responder.respond_with_internal_error(format!("{e}"));
                            }
                            match tokio::fs::write(&path, &req.content).await {
                                Ok(()) => responder.respond(WriteTextFileResponse::new()),
                                Err(e) => {
                                    tracing::warn!(target: "ruagent::acp::fs", "write failed: {e}");
                                    responder.respond_with_internal_error(format!("{e}"))
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(target: "ruagent::acp::fs", "write rejected: {e}");
                            responder.respond_with_internal_error(format!("{e}"))
                        }
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            // 1. Initialize (protocol v1).
            connection
                .send_request(InitializeRequest::new(
                    agent_client_protocol::schema::ProtocolVersion::V1,
                ))
                .block_task()
                .await?;

            // 2. New session with MCP injection (design §7.1).
            let mut new_session = NewSessionRequest::new(&opts.cwd);
            new_session.mcp_servers = opts.mcp_servers.clone();
            let session = connection.send_request(new_session).block_task().await?;
            let session_id = session.session_id;

            // 3. Prompt to completion.
            let prompt = PromptRequest::new(
                session_id.clone(),
                vec![ContentBlock::Text(TextContent::new(opts.prompt.clone()))],
            );
            let response = connection.send_request(prompt).block_task().await?;

            Ok(RunOutcome {
                session_id: session_id.to_string(),
                stop_reason: crate::map::stop_reason(response.stop_reason),
            })
        })
        .await
        .map_err(AcpError::from)
}
