//! RunManager: drives ACP runs, persists state, streams events, and
//! applies the M1 permission policy. One supervisor task per run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use ruagent_acp::adapter::HarnessAdapter as _;
use ruagent_acp::permission::{PermissionAnswer, PermissionAsk};
use ruagent_acp::{RunOptions, adapter_for, run_once};
use ruagent_core::{
    AgentCard, PermissionKind, PermissionResolution, Run, RunEvent, RunId, RunParams, RunStatus,
    StopReason, Task, TaskStatus,
};
use ruagent_policy::{PermissionAction, PermissionPolicy};
use ruagent_store::{Db, TranscriptLine, TranscriptWriter, transcript_path};
use tokio::sync::{broadcast, mpsc};

/// A pending permission ask parked for a human decision.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingPermission {
    pub run_id: RunId,
    pub tool_call_id: String,
    pub title: String,
    pub raw_input: serde_json::Value,
    pub choices: Vec<ruagent_core::PermissionChoice>,
}

/// A parked permission ask with its answer channel and event tap.
type PendingEntry = (
    PendingPermission,
    mpsc::UnboundedSender<RunEvent>,
    tokio::sync::oneshot::Sender<PermissionAnswer>,
);

/// Map key: `<run_id>:<tool_call_id>`.
type PendingMap = Arc<Mutex<HashMap<String, PendingEntry>>>;

/// Messages broadcast to live subscribers (SSE/WS).
#[derive(Debug, Clone)]
pub enum StreamMsg {
    Event { run_id: RunId, line: TranscriptLine },
    Finished { run_id: RunId, status: RunStatus },
}

pub struct RunManager {
    db: Db,
    root: PathBuf,
    agents: HashMap<String, AgentCard>,
    policy: PermissionPolicy,
    broadcast: broadcast::Sender<StreamMsg>,
    pending: PendingMap,
}

impl RunManager {
    pub fn new(db: Db, root: PathBuf, agents: Vec<AgentCard>, policy: PermissionPolicy) -> Self {
        let (broadcast, _) = broadcast::channel(1024);
        Self {
            db,
            root,
            agents: agents.into_iter().map(|a| (a.name.clone(), a)).collect(),
            policy,
            broadcast,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn broadcast(&self) -> broadcast::Receiver<StreamMsg> {
        self.broadcast.subscribe()
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn transcripts_dir(&self) -> PathBuf {
        self.root.join("data").join("transcripts")
    }

    pub fn agent(&self, name: &str) -> Option<&AgentCard> {
        self.agents.get(name)
    }

    pub fn agents(&self) -> Vec<&AgentCard> {
        let mut v: Vec<_> = self.agents.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// All currently parked permission asks.
    pub fn pending_permissions(&self) -> Vec<PendingPermission> {
        self.pending
            .lock()
            .expect("pending lock")
            .values()
            .map(|(p, _, _)| p.clone())
            .collect()
    }

    /// Answer a parked permission ask (the human inbox path). `key` is
    /// `<run_id>:<tool_call_id>` as listed by `pending_permissions`.
    pub fn resolve_permission(&self, key: &str, answer: PermissionAnswer) -> Result<()> {
        let entry = self
            .pending
            .lock()
            .expect("pending lock")
            .remove(key)
            .with_context(|| format!("no pending permission `{key}`"))?;
        let (pending, ev_tx, answer_tx) = entry;

        // Emit the resolution event into the run's stream (attribution:
        // human, since rules would have auto-answered at arrival).
        let outcome = match &answer {
            PermissionAnswer::Select(id) => pending
                .choices
                .iter()
                .find(|c| &c.option_id == id)
                .map(|c| c.kind)
                .unwrap_or(PermissionKind::RejectOnce),
            PermissionAnswer::Cancel => PermissionKind::RejectOnce,
        };
        let _ = ev_tx.send(RunEvent::PermissionResolved {
            tool_call_id: pending.tool_call_id.clone(),
            outcome,
            resolution: PermissionResolution::Human,
        });
        answer_tx
            .send(answer)
            .map_err(|_| anyhow::anyhow!("permission receiver already gone"))?;
        Ok(())
    }

    /// Create and start a run of `task` on the named agent.
    pub async fn start_run(
        &self,
        task: &Task,
        agent_name: &str,
        prompt: String,
        mcp_servers: Vec<agent_client_protocol::schema::v1::McpServer>,
        cwd_override: Option<PathBuf>,
    ) -> Result<Run> {
        let card = self
            .agent(agent_name)
            .with_context(|| format!("unknown agent `{agent_name}`"))?;
        let spec = adapter_for(card.harness)
            .spawn_spec(card)
            .with_context(|| format!("resolving spawn command for `{agent_name}`"))?;

        let mut run = Run::new(task.id, RunParams::for_agent(card.id));

        // Per-run isolated workspace (design §8.2; git worktrees in M2).
        let workspace = cwd_override
            .unwrap_or_else(|| self.root.join("workspaces").join(format!("run-{}", run.id)));
        std::fs::create_dir_all(&workspace)
            .with_context(|| format!("creating workspace {}", workspace.display()))?;
        run.workspace = Some(workspace.to_string_lossy().into_owned());

        run.status = RunStatus::Spawning;
        run.updated_at = chrono::Utc::now();
        self.db.insert_run(&run).await?;
        if task.status == TaskStatus::Pending {
            self.db
                .update_task_status(task.id, TaskStatus::InProgress)
                .await?;
        }

        let db = self.db.clone();
        let root = self.root.clone();
        let broadcast = self.broadcast.clone();
        let pending = self.pending.clone();
        let policy = self.policy.clone();
        let task_id = task.id;
        let run_id = run.id;
        let cwd = workspace.clone();

        let returned = run.clone();
        tokio::spawn(async move {
            if let Err(err) = supervise(
                db.clone(),
                root,
                broadcast,
                pending,
                policy,
                run.clone(),
                task_id,
                spec,
                prompt,
                mcp_servers,
                cwd,
            )
            .await
            {
                tracing::error!(run_id = %run_id, error = %err, "run supervisor failed");
                let mut run = run;
                run.status = RunStatus::Failed;
                run.error = Some(err.to_string());
                run.stop_reason = Some(StopReason::Error);
                run.updated_at = chrono::Utc::now();
                let _ = db.update_run(&run).await;
            }
        });

        Ok(returned)
    }
}

/// The per-run supervisor: drives the ACP connection, writes the
/// transcript, applies the permission policy, and finalizes state.
#[allow(clippy::too_many_arguments)]
async fn supervise(
    db: Db,
    root: PathBuf,
    broadcast: broadcast::Sender<StreamMsg>,
    pending: PendingMap,
    policy: PermissionPolicy,
    mut run: Run,
    task_id: ruagent_core::TaskId,
    spec: ruagent_acp::SpawnSpec,
    prompt: String,
    mcp_servers: Vec<agent_client_protocol::schema::v1::McpServer>,
    cwd: PathBuf,
) -> Result<()> {
    let transcripts_dir = root.join("data").join("transcripts");
    let mut transcript = TranscriptWriter::create(transcript_path(&transcripts_dir, &run.id))
        .with_context(|| "creating transcript")?;

    let emit = |transcript: &mut TranscriptWriter,
                broadcast: &broadcast::Sender<StreamMsg>,
                run: &Run,
                event: RunEvent| {
        let line = append_event(transcript, run, &event);
        let _ = broadcast.send(StreamMsg::Event {
            run_id: run.id,
            line,
        });
    };

    emit(
        &mut transcript,
        &broadcast,
        &run,
        RunEvent::StateChanged {
            status: RunStatus::Spawning,
        },
    );

    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<RunEvent>();
    let (ask_tx, mut ask_rx) = mpsc::unbounded_channel::<PermissionAsk>();

    let opts = RunOptions {
        program: spec.program.clone(),
        args: spec.args.clone(),
        cwd,
        mcp_servers,
        prompt,
    };
    let mut driver = tokio::spawn(run_once(opts, ev_tx.clone(), ask_tx));

    let mut ev_open = true;
    let mut ask_open = true;
    let mut done = false;
    let mut done_at: Option<tokio::time::Instant> = None;

    loop {
        // The prompt response can beat late notifications through the
        // client's dispatch (responses bypass the notification loop), so
        // after the driver finishes we drain for a grace period instead of
        // breaking on the first empty poll.
        if done {
            let at = *done_at.get_or_insert_with(tokio::time::Instant::now);
            if ev_rx.is_empty()
                && ask_rx.is_empty()
                && at.elapsed() > std::time::Duration::from_millis(300)
            {
                break;
            }
        }
        tokio::select! {
            maybe_ev = ev_rx.recv(), if ev_open => match maybe_ev {
                Some(event) => {
                    if let RunEvent::UsageUpdate { usage } = &event {
                        run.context_usage = Some(*usage);
                        if usage.cost_usd.is_some() {
                            run.cost_usd = usage.cost_usd;
                        }
                        run.updated_at = chrono::Utc::now();
                        let r = run.clone();
                        let db = db.clone();
                        tokio::spawn(async move {
                            let _ = db.update_run(&r).await;
                        });
                    }
                    emit(&mut transcript, &broadcast, &run, event);
                }
                None => ev_open = false,
            },
            maybe_ask = ask_rx.recv(), if ask_open => match maybe_ask {
                Some(ask) => {
                    let PermissionAsk { tool_call_id, title, raw_input, choices, answer } = ask;
                    let info = PendingPermission {
                        run_id: run.id,
                        tool_call_id: tool_call_id.clone(),
                        title: title.clone(),
                        raw_input: raw_input.clone(),
                        choices: choices.clone(),
                    };
                    match policy.decide(&title) {
                        action @ (PermissionAction::Allow | PermissionAction::Reject) => {
                            let wanted = if matches!(action, PermissionAction::Allow) {
                                [PermissionKind::AllowOnce, PermissionKind::AllowAlways]
                            } else {
                                [PermissionKind::RejectOnce, PermissionKind::RejectAlways]
                            };
                            let choice = choices.iter().find(|c| wanted.contains(&c.kind));
                            match choice {
                                Some(c) => {
                                    emit(&mut transcript, &broadcast, &run, RunEvent::PermissionResolved {
                                        tool_call_id: tool_call_id.clone(),
                                        outcome: c.kind,
                                        resolution: PermissionResolution::Rule {
                                            rule_id: format!("title~={}", title.to_lowercase()),
                                        },
                                    });
                                    let _ = answer.send(PermissionAnswer::Select(c.option_id.clone()));
                                }
                                None => {
                                    // No matching option: fail closed.
                                    let _ = answer.send(PermissionAnswer::Cancel);
                                }
                            }
                        }
                        PermissionAction::Ask => {
                            // Park for the human inbox (fail-closed default).
                            let key = format!("{}:{}", info.run_id, info.tool_call_id);
                            pending
                                .lock()
                                .expect("pending lock")
                                .insert(key, (info, ev_tx.clone(), answer));
                        }
                    }
                }
                None => ask_open = false,
            },
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)), if done => {
                // grace tick: re-check the drain condition above
            }
            res = &mut driver, if !done => {
                done = true;
                run.updated_at = chrono::Utc::now();
                match res {
                    Ok(Ok(outcome)) => {
                        run.status = RunStatus::Completed;
                        run.stop_reason = Some(outcome.stop_reason);
                        run.acp_session_id = Some(outcome.session_id);
                        emit(&mut transcript, &broadcast, &run, RunEvent::Stopped {
                            stop_reason: outcome.stop_reason,
                        });
                    }
                    Ok(Err(err)) => {
                        run.status = RunStatus::Failed;
                        run.stop_reason = Some(StopReason::Error);
                        run.error = Some(err.to_string());
                        emit(&mut transcript, &broadcast, &run, RunEvent::Error {
                            message: err.to_string(),
                        });
                    }
                    Err(join_err) => {
                        run.status = RunStatus::Failed;
                        run.stop_reason = Some(StopReason::Error);
                        run.error = Some(format!("supervisor task panicked: {join_err}"));
                    }
                }
                emit(&mut transcript, &broadcast, &run, RunEvent::StateChanged {
                    status: run.status,
                });
                // Dead asks for a finished run can never be answered
                // usefully; drop them so the inbox stays honest.
                let prefix = format!("{run_id}:", run_id = run.id);
                let mut p = pending.lock().expect("pending lock");
                p.retain(|k, _| !k.starts_with(&prefix));
                drop(p);
            }
        }
    }

    // Finalize only AFTER the drain: a run that is terminal in the DB or
    // announced as Finished must have every event (including stragglers
    // that raced the prompt response) already appended and flushed.
    transcript.flush()?;
    db.update_run(&run).await?;
    if run.status == RunStatus::Completed {
        db.update_task_status(task_id, TaskStatus::Done).await?;
    }
    let _ = broadcast.send(StreamMsg::Finished {
        run_id: run.id,
        status: run.status,
    });
    Ok(())
}

/// Append to the transcript and produce the envelope line for broadcast.
fn append_event(transcript: &mut TranscriptWriter, run: &Run, event: &RunEvent) -> TranscriptLine {
    // The writer assigns ts+seq; we re-read by constructing the same shape
    // for broadcast (seq is the writer's counter, mirrored here).
    let seq = transcript.next_seq();
    let line = TranscriptLine {
        ts: chrono::Utc::now(),
        seq,
        event: event.clone(),
    };
    if let Err(e) = transcript.append(event) {
        tracing::warn!(run_id = %run.id, error = %e, "transcript append failed");
    }
    line
}
