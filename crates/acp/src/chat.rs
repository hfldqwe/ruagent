//! Persistent chat sessions: one spawned agent process, one ACP session,
//! many prompts — terminal-like conversation. Model switching is best-effort
//! via `session/set_config_option` (agents that don't support it keep their
//! default; the caller surfaces the warning).

use std::path::PathBuf;

use agent_client_protocol::schema::v1::{
    ContentBlock, InitializeRequest, McpServer, NewSessionRequest, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionConfigSelectOptions, SessionNotification,
    SetSessionConfigOptionRequest, TextContent,
};
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo, LineDirection};
use ruagent_core::RunEvent;
use tokio::sync::mpsc;

use crate::AcpError;
use crate::permission::{PermissionAnswer, PermissionAsk};

/// One selectable model advertised by the agent (ACP session config,
/// option `model`). `value` is what `set_config_option` expects — agents
/// use their own scheme (dsh: `provider/model`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ModelChoice {
    pub value: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// Live model state of a session: the advertised choices and the current
/// selection. `None`-valued watch = the agent advertises no model option.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct ModelsState {
    pub choices: Vec<ModelChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

/// Extract the model option from an ACP config-option list. Prefers the
/// option with category `model`, falling back to id `model`.
fn extract_models(options: &[SessionConfigOption]) -> Option<ModelsState> {
    let opt = options
        .iter()
        .find(|o| matches!(o.category, Some(SessionConfigOptionCategory::Model)))
        .or_else(|| options.iter().find(|o| o.id.to_string() == "model"))?;
    let SessionConfigKind::Select(select) = &opt.kind else {
        return None;
    };
    let mut choices = Vec::new();
    match &select.options {
        SessionConfigSelectOptions::Ungrouped(entries) => {
            for o in entries {
                choices.push(ModelChoice {
                    value: o.value.to_string(),
                    name: o.name.clone(),
                    description: o.description.clone(),
                    group: None,
                });
            }
        }
        SessionConfigSelectOptions::Grouped(groups) => {
            for g in groups {
                for o in &g.options {
                    choices.push(ModelChoice {
                        value: o.value.to_string(),
                        name: o.name.clone(),
                        description: o.description.clone(),
                        group: Some(g.name.clone()),
                    });
                }
            }
        }
        _ => return None,
    }
    if choices.is_empty() {
        return None;
    }
    Some(ModelsState {
        choices,
        current: Some(select.current_value.to_string()),
    })
}

/// Options for a chat session.
#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub mcp_servers: Vec<McpServer>,
    /// Best-effort initial model selection (`session/set_config_option`,
    /// config id `model`). Must be a value the agent advertises.
    pub model: Option<String>,
}

/// Commands into a live chat session.
#[derive(Debug)]
pub enum ChatCommand {
    /// Send another user message on the same session.
    Prompt { text: String },
    /// Switch model mid-session (no restart, context preserved). Replies
    /// with the new current value, or the agent's rejection message.
    SetModel {
        model: String,
        reply: tokio::sync::oneshot::Sender<Result<Option<String>, String>>,
    },
    /// Close the chat (user action or idle timeout). The connection drops,
    /// which kills the child process group.
    Shutdown,
}

/// A live chat session: cheap to clone; all clones share the underlying
/// supervisor. The event stream is exposed via a tokio broadcast channel;
/// the live model state via a watch channel (filled after `session/new`).
#[derive(Clone)]
pub struct ChatSession {
    cmd_tx: mpsc::UnboundedSender<ChatCommand>,
    events: tokio::sync::broadcast::Sender<RunEvent>,
    models: std::sync::Arc<tokio::sync::watch::Sender<Option<ModelsState>>>,
    models_rx: tokio::sync::watch::Receiver<Option<ModelsState>>,
}

impl ChatSession {
    pub fn send(&self, cmd: ChatCommand) -> Result<(), AcpError> {
        self.cmd_tx
            .send(cmd)
            .map_err(|_| AcpError::Command("chat closed".into()))
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<RunEvent> {
        self.events.subscribe()
    }

    /// The last known model state (None until the agent reports it).
    pub fn models_now(&self) -> Option<ModelsState> {
        self.models_rx.borrow().clone()
    }

    /// A receiver tracking model-state updates for this session.
    pub fn models_watch(&self) -> tokio::sync::watch::Receiver<Option<ModelsState>> {
        self.models_rx.clone()
    }
}

/// Start a chat session: spawns the agent, initializes, opens an ACP
/// session, applies the model if given, then parks the supervisor loop on
/// the command channel. Each prompt's events flow on the broadcast channel;
/// a `Stopped` RunEvent marks each completed reply.
pub fn start_chat(
    opts: ChatOptions,
    ask_tx: mpsc::UnboundedSender<PermissionAsk>,
) -> Result<ChatSession, AcpError> {
    let agent =
        AcpAgent::from_args(std::iter::once(opts.program.clone()).chain(opts.args.iter().cloned()))
            .map_err(|e| AcpError::Command(format!("{}: {e}", opts.program)))?;
    let agent = agent.with_debug(|line, dir| {
        if dir == LineDirection::Stderr {
            tracing::debug!(target: "ruagent::acp::stderr", "{line}");
        }
    });

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ChatCommand>();
    let (events_tx, _events_rx) = tokio::sync::broadcast::channel::<RunEvent>(1024);
    let (models_tx, models_rx) = tokio::sync::watch::channel::<Option<ModelsState>>(None);
    let models_tx = std::sync::Arc::new(models_tx);

    let supervisor_events = events_tx.clone();
    let ask = ask_tx.clone();
    let return_events = events_tx.clone();
    let supervisor_models = models_tx.clone();

    tokio::spawn(async move {
        if let Err(err) = supervise_chat(
            agent,
            opts,
            cmd_rx,
            supervisor_events,
            supervisor_models,
            ask,
        )
        .await
        {
            let _ = events_tx.send(RunEvent::Error {
                message: format!("chat session ended: {err}"),
            });
        }
    });

    Ok(ChatSession {
        cmd_tx,
        events: return_events,
        models: models_tx,
        models_rx,
    })
}

async fn supervise_chat(
    agent: AcpAgent,
    opts: ChatOptions,
    mut cmd_rx: mpsc::UnboundedReceiver<ChatCommand>,
    events: tokio::sync::broadcast::Sender<RunEvent>,
    models_tx: std::sync::Arc<tokio::sync::watch::Sender<Option<ModelsState>>>,
    ask_tx: mpsc::UnboundedSender<PermissionAsk>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ev_notification = events.clone();
    let ev_permission = events.clone();
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
                async move |req: RequestPermissionRequest, responder, _cx| {
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
                        tool_call_id: tool.tool_call_id.to_string(),
                        title: title.clone(),
                        raw_input: raw_input.clone(),
                        choices: choices.clone(),
                    });

                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let ask_msg = PermissionAsk {
                        tool_call_id: tool.tool_call_id.to_string(),
                        title,
                        raw_input,
                        choices,
                        answer: tx,
                    };
                    let answer = match ask.send(ask_msg) {
                        Ok(()) => rx.await.unwrap_or(PermissionAnswer::Cancel),
                        Err(_) => PermissionAnswer::Cancel,
                    };
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
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let root = read_root;
                async move |req: agent_client_protocol::schema::v1::ReadTextFileRequest,
                            responder,
                            _cx| {
                    match crate::fs_tools::resolve_existing_under(&root, &req.path) {
                        Ok(path) => match tokio::fs::read_to_string(&path).await {
                            Ok(content) => responder.respond(
                                agent_client_protocol::schema::v1::ReadTextFileResponse::new(
                                    crate::fs_tools::apply_line_window(
                                        &content, req.line, req.limit,
                                    ),
                                ),
                            ),
                            Err(e) => responder.respond_with_internal_error(format!("{e}")),
                        },
                        Err(e) => responder.respond_with_internal_error(format!("{e}")),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let root = write_root;
                async move |req: agent_client_protocol::schema::v1::WriteTextFileRequest,
                            responder,
                            _cx| {
                    match crate::fs_tools::resolve_new_under(&root, &req.path) {
                        Ok(path) => {
                            if let Some(parent) = path.parent()
                                && let Err(e) = tokio::fs::create_dir_all(parent).await
                            {
                                return responder.respond_with_internal_error(format!("{e}"));
                            }
                            match tokio::fs::write(&path, &req.content).await {
                                Ok(()) => responder.respond(
                                    agent_client_protocol::schema::v1::WriteTextFileResponse::new(),
                                ),
                                Err(e) => responder.respond_with_internal_error(format!("{e}")),
                            }
                        }
                        Err(e) => responder.respond_with_internal_error(format!("{e}")),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |connection: ConnectionTo<Agent>| {
            // Initialize (protocol v1).
            connection
                .send_request(InitializeRequest::new(
                    agent_client_protocol::schema::ProtocolVersion::V1,
                ))
                .block_task()
                .await?;

            // One session for the whole chat.
            let mut new_session = NewSessionRequest::new(&opts.cwd);
            new_session.mcp_servers = opts.mcp_servers.clone();
            let session = connection.send_request(new_session).block_task().await?;
            let session_id = session.session_id;

            // The agent advertises its model catalog here (dsh: grouped
            // provider/model pairs; agents without the option report none).
            let advertised = extract_models(session.config_options.as_deref().unwrap_or(&[]));
            if let Some(state) = &advertised {
                models_tx.send_replace(Some(state.clone()));
            }
            // Which config id addresses the model option (usually "model").
            let model_config_id = session
                .config_options
                .as_deref()
                .and_then(|opts| {
                    opts.iter()
                        .find(|o| {
                            matches!(o.category, Some(SessionConfigOptionCategory::Model))
                                || o.id.to_string() == "model"
                        })
                        .map(|o| o.id.clone())
                })
                .unwrap_or_else(|| "model".into());

            // Best-effort initial model selection.
            if let Some(model) = &opts.model {
                let set = SetSessionConfigOptionRequest::new(
                    session_id.clone(),
                    model_config_id.clone(),
                    SessionConfigOptionValue::value_id(model.clone()),
                );
                match connection.send_request(set).block_task().await {
                    Ok(resp) => {
                        if let Some(state) = extract_models(&resp.config_options) {
                            models_tx.send_replace(Some(state));
                        }
                    }
                    Err(e) => {
                        // Not all agents support config options; the chat
                        // still works on the harness default.
                        let _ = events.send(RunEvent::Error {
                            message: format!(
                                "model `{model}` not applied (agent default in use): {e}"
                            ),
                        });
                    }
                }
            }

            let _ = events.send(RunEvent::StateChanged {
                status: ruagent_core::RunStatus::Running,
            });

            // The conversation loop: each Prompt keeps the same session —
            // that's the whole point (terminal-like multi-turn).
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    ChatCommand::Prompt { text } => {
                        let prompt = PromptRequest::new(
                            session_id.clone(),
                            vec![ContentBlock::Text(TextContent::new(text))],
                        );
                        let resp = connection.send_request(prompt).block_task().await;
                        match resp {
                            Ok(r) => {
                                let _ = events.send(RunEvent::Stopped {
                                    stop_reason: crate::map::stop_reason(r.stop_reason),
                                });
                            }
                            Err(e) => {
                                let _ = events.send(RunEvent::Error {
                                    message: format!("prompt failed: {e}"),
                                });
                                break;
                            }
                        }
                    }
                    ChatCommand::SetModel { model, reply } => {
                        let set = SetSessionConfigOptionRequest::new(
                            session_id.clone(),
                            model_config_id.clone(),
                            SessionConfigOptionValue::value_id(model.clone()),
                        );
                        let answer = match connection.send_request(set).block_task().await {
                            Ok(resp) => {
                                let state = extract_models(&resp.config_options);
                                if let Some(s) = &state {
                                    models_tx.send_replace(Some(s.clone()));
                                }
                                Ok(state.and_then(|s| s.current))
                            }
                            Err(e) => Err(format!("{e}")),
                        };
                        let _ = reply.send(answer);
                    }
                    ChatCommand::Shutdown => break,
                }
            }
            let _ = events.send(RunEvent::StateChanged {
                status: ruagent_core::RunStatus::Completed,
            });
            let _ = ask;
            Ok::<(), agent_client_protocol::Error>(())
        })
        .await?;
    Ok(())
}
