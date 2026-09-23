//! RunManager: drives ACP runs, persists state, streams events, and
//! applies the M1 permission policy. One supervisor task per run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use ruagent_acp::adapter::HarnessAdapter as _;
use ruagent_acp::permission::{PermissionAnswer, PermissionAsk};
use ruagent_acp::{RunOptions, adapter_for, run_once};
use ruagent_core::{
    AgentCard, EdgeKind, PermissionKind, PermissionResolution, RouteSource, RoutingDecision, Run,
    RunEvent, RunId, RunParams, RunStatus, StopReason, Task, TaskCreator, TaskEdge, TaskStatus,
};
use ruagent_orchestrator::{self, PipelineStep, Topology};
use ruagent_policy::PermissionPolicy;
use ruagent_store::{Db, TranscriptLine, TranscriptWriter, transcript_path};
use tokio::sync::{broadcast, mpsc};

/// Does this failure look like MCP injection / session startup?
///
/// Deliberately permissive: a false positive only costs one extra handshake on a
/// run that has already failed, and the diagnosis is informative either way.
fn looks_like_mcp_failure(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("mcp") || message.contains("会话建立失败") || m.contains("session/new")
}

/// Do the MCP handshake ourselves and report which injected server is broken.
///
/// The harness says only `mcp-client(mcp): initial connection or tool
/// synchronization failed` — no server, no command, no reason. Since the run has
/// already failed, paying for a real initialize + tools/list (10s cap, servers in
/// parallel) is worth it: the result names the command and the actual error.
async fn diagnose_mcp_servers(servers: &[agent_client_protocol::schema::v1::McpServer]) -> String {
    use agent_client_protocol::schema::v1::McpServer;
    use ruagent_mcp::health::{PingTarget, describe, ping};

    let mut handles = Vec::new();
    for server in servers {
        let (name, target) = match server {
            McpServer::Stdio(x) => (
                x.name.clone(),
                PingTarget {
                    command: Some(x.command.display().to_string()),
                    args: x.args.clone(),
                    url: None,
                },
            ),
            McpServer::Http(x) => (
                x.name.clone(),
                PingTarget {
                    command: None,
                    args: Vec::new(),
                    url: Some(x.url.clone()),
                },
            ),
            McpServer::Sse(x) => (
                x.name.clone(),
                PingTarget {
                    command: None,
                    args: Vec::new(),
                    url: Some(x.url.clone()),
                },
            ),
            _ => continue,
        };
        handles.push(tokio::spawn(async move {
            let described = describe(&target);
            let outcome = ping(&target).await;
            (name, described, outcome)
        }));
    }
    if handles.is_empty() {
        return String::new();
    }
    let mut lines = vec![format!(
        "\n\n[MCP 自检] 本会话注入了 {} 个 MCP 服务器，逐个做了 initialize + tools/list 握手：",
        handles.len()
    )];
    for handle in handles {
        match handle.await {
            Ok((name, described, outcome)) if outcome.ok => lines.push(format!(
                "  · {}（{}）：握手成功，{} 个工具，{}ms",
                name,
                described,
                outcome.tools.unwrap_or(0),
                outcome.latency_ms
            )),
            Ok((name, described, outcome)) => lines.push(format!(
                "  · {}（{}）：**握手失败** —— {}（{}ms）",
                name,
                described,
                outcome.error.unwrap_or_else(|| "未知原因".to_string()),
                outcome.latency_ms
            )),
            Err(e) => lines.push(format!("  · 自检任务失败：{e}")),
        }
    }
    lines.push(
        "  提示：若失败的是 ruagent 自己的 mcp-serve，检查该路径是否存在、是否可执行、\
         是否正在被 cargo 重建（CARGO_TARGET_DIR 指向的 exe 会被每次构建覆盖）。"
            .to_string(),
    );
    lines.join("\n")
}

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

/// Where a run executes (design SS8.2: per-run isolation).
#[derive(Debug, Clone)]
pub enum WorkspaceSpec {
    /// Fresh empty dir under `<root>/workspaces/run-<id>` (default).
    Fresh,
    /// Explicit working directory.
    Cwd(PathBuf),
    /// A git worktree of `repo` on a per-run branch - fan-out agents
    /// never step on each other (design SS5.2/SS8.2).
    Worktree { repo: PathBuf },
}

/// How to launch one run beyond task/agent/prompt/workspace: routing
/// provenance and canonical session-option defaults (issue #36).
#[derive(Default)]
pub struct RunLaunch {
    pub routed: Option<RoutingDecision>,
    /// Canonical option defaults applied after `session/new`:
    /// `mode` (permission mode) / `effort` (thinking level).
    pub options: std::collections::BTreeMap<String, String>,
    /// The prompt to RECORD as the original ask (defaults to the
    /// incoming one). A retry composes context onto the original but
    /// must still record the original — chained retries would grow
    /// the record monotonically otherwise.
    pub original_prompt: Option<String>,
    /// Context prepended to the injection render (visible as
    /// ContextInjected, not as user speech): a retry's crash snapshot
    /// rides here, keeping session previews and distillation clean.
    pub context_prefix: Option<String>,
}

/// One harness's concurrency gate (design §8.3): a counting semaphore
/// plus a waiting counter so an overloaded harness can reject new
/// launches with a clear error instead of queueing forever.
struct HarnessGate {
    sem: std::sync::Arc<tokio::sync::Semaphore>,
    waiting: std::sync::atomic::AtomicUsize,
}

/// A run's hold on its harness gate: either a permit taken on the spot
/// (fast path), or a reserved waiting slot the supervisor task settles.
enum GateHold {
    Free(tokio::sync::OwnedSemaphorePermit),
    Waiting(std::sync::Arc<HarnessGate>),
}

/// Reserve a waiting slot on a busy gate (design §8.3). Increments the
/// waiting count — the caller MUST decrement it once it acquires. A
/// full queue rejects with a clear error.
fn reserve_slot(
    gate: &HarnessGate,
    kind: ruagent_core::HarnessKind,
    queue_cap: usize,
    running: usize,
) -> Result<()> {
    let waiting = gate.waiting.fetch_add(1, Ordering::SeqCst) + 1;
    if waiting > queue_cap {
        gate.waiting.fetch_sub(1, Ordering::SeqCst);
        anyhow::bail!(
            "harness {kind:?} is saturated: {queue_cap} runs already waiting \
             ({running} running) — retry when the current runs finish"
        );
    }
    Ok(())
}

/// Messages broadcast to live subscribers (SSE/WS).
#[derive(Debug, Clone)]
pub enum StreamMsg {
    Event { run_id: RunId, line: TranscriptLine },
    Finished { run_id: RunId, status: RunStatus },
}

pub struct RunManager {
    db: Db,
    root: PathBuf,
    /// Live registry, swappable without a daemon restart (registry
    /// edits hot-reload it; see api.rs and registry.rs).
    agents: std::sync::RwLock<HashMap<String, AgentCard>>,
    policy: PermissionPolicy,
    mcp: crate::config::McpConfig,
    broadcast: broadcast::Sender<StreamMsg>,
    pending: PendingMap,
    /// Resolved id of the approver agent, if configured (tier 2).
    approver_id: Option<ruagent_core::AgentId>,
    /// Asks waiting for the approver agent.
    approver_tx: mpsc::UnboundedSender<PendingPermission>,
    approver_rx: std::sync::Mutex<Option<mpsc::UnboundedReceiver<PendingPermission>>>,
    /// Live cancellation tokens per run (POST /runs/{id}/cancel).
    cancellations: Arc<Mutex<HashMap<RunId, tokio_util::sync::CancellationToken>>>,
    /// Round-trip editor over config/agents.toml (registry API).
    registry: crate::registry::Editor,
    /// Per-harness concurrency gates (design §8.3): fanning out N
    /// agents on one runtime queues them instead of spawning N
    /// children at once.
    gates: HashMap<ruagent_core::HarnessKind, std::sync::Arc<HarnessGate>>,
}

impl RunManager {
    pub fn new(
        db: Db,
        root: PathBuf,
        agents: Vec<AgentCard>,
        policy: PermissionPolicy,
        mcp: crate::config::McpConfig,
    ) -> Self {
        let registry_path = root.join("config").join("agents.toml");
        let (broadcast, _) = broadcast::channel(1024);
        let (approver_tx, approver_rx) = mpsc::unbounded_channel();
        // Resolve the approver name into its stable id (if registered).
        let approver_id = policy
            .approver
            .as_ref()
            .and_then(|a| agents.iter().find(|c| c.name == a.agent).map(|c| c.id));
        if let Some(approver) = &policy.approver
            && approver_id.is_none()
        {
            tracing::warn!(
                agent = %approver.agent,
                "approver agent configured but not registered; tier 2 disabled"
            );
        }
        let limits = policy.concurrency;
        Self {
            db,
            root,
            agents: std::sync::RwLock::new(
                agents.into_iter().map(|a| (a.name.clone(), a)).collect(),
            ),
            policy,
            mcp,
            broadcast,
            pending: Arc::new(Mutex::new(HashMap::new())),
            approver_id,
            approver_tx,
            approver_rx: std::sync::Mutex::new(Some(approver_rx)),
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            registry: crate::registry::Editor::new(registry_path),
            gates: ruagent_core::HarnessKind::ALL
                .into_iter()
                .map(|k| {
                    (
                        k,
                        std::sync::Arc::new(HarnessGate {
                            sem: std::sync::Arc::new(tokio::sync::Semaphore::new(
                                limits.per_harness,
                            )),
                            waiting: std::sync::atomic::AtomicUsize::new(0),
                        }),
                    )
                })
                .collect(),
        }
    }

    /// The gate for one harness (design §8.3). Every harness kind has
    /// one (see `HarnessKind::ALL`).
    fn gate(&self, kind: ruagent_core::HarnessKind) -> std::sync::Arc<HarnessGate> {
        self.gates
            .get(&kind)
            .expect("every harness kind has a gate")
            .clone()
    }

    /// The agents.toml editor (registry write API).
    pub fn registry(&self) -> &crate::registry::Editor {
        &self.registry
    }

    /// Start the approver loop (call once after wrapping in Arc).
    pub fn start_approver_loop(self: &Arc<Self>) {
        let Some(mut rx) = self.approver_rx.lock().expect("approver rx").take() else {
            return;
        };
        let mgr = Arc::clone(self);
        tokio::spawn(async move {
            while let Some(ask) = rx.recv().await {
                let mgr = Arc::clone(&mgr);
                tokio::spawn(async move {
                    if let Err(e) = mgr.run_approver(ask).await {
                        tracing::warn!(error = %e, "approver run failed; ask stays with the human");
                    }
                });
            }
        });
    }

    /// Cancel a live run: triggers the supervisor's cancellation branch —
    /// the ACP driver is dropped (killing the child process group), the
    /// run is marked cancelled and finalized like any other terminal
    /// state. Returns false when the run is not live.
    pub fn cancel_run(&self, run_id: RunId) -> bool {
        self.cancellations
            .lock()
            .expect("cancellations lock")
            .get(&run_id)
            .map(|t| t.cancel())
            .is_some()
    }

    /// Tier 2: let the approver agent decide a parked ask (design SS9.2).
    /// Fail-closed: any failure leaves the ask parked for the human.
    async fn run_approver(&self, ask: PendingPermission) -> Result<()> {
        let approver = self
            .policy
            .approver
            .as_ref()
            .context("approver not configured")?;
        let card = self
            .agent(&approver.agent)
            .with_context(|| format!("approver agent `{}` missing", approver.agent))?;

        let prompt = format!(
            "An agent requests permission to run: {}\nInput: {}\n\nReply with exactly: ALLOW or REJECT",
            ask.title, ask.raw_input
        );
        let mut task = Task::new(
            format!("approve: {}", ask.title),
            prompt.clone(),
            TaskCreator::Rule {
                name: "approver".into(),
            },
        );
        task.project = Some("ruagent-internal".into());
        self.db.insert_task(&task).await?;

        let mcp = self.mcp_for(&card);
        let run = self
            .start_run(
                &task,
                &approver.agent,
                prompt,
                mcp,
                WorkspaceSpec::Fresh,
                RunLaunch {
                    routed: Some(RoutingDecision::explicit(card.id)),
                    ..Default::default()
                },
            )
            .await?;
        let final_run = wait_terminal(&self.db, run.id).await?;
        if final_run.status != RunStatus::Completed {
            anyhow::bail!("approver run {:?}: no verdict", final_run.status);
        }
        let text = final_run.result.unwrap_or_default().to_lowercase();
        // REJECT first: an ambiguous "allow or reject" echo must not allow.
        let want = if text.contains("reject") {
            [PermissionKind::RejectOnce, PermissionKind::RejectAlways]
        } else if text.contains("allow") {
            [PermissionKind::AllowOnce, PermissionKind::AllowAlways]
        } else {
            anyhow::bail!("approver verdict unclear: {text:?}");
        };
        let Some(choice) = ask.choices.iter().find(|c| want.contains(&c.kind)) else {
            anyhow::bail!("no matching option offered for the verdict");
        };

        let key = format!("{}:{}", ask.run_id, ask.tool_call_id);
        self.resolve_internal(
            &key,
            PermissionAnswer::Select(choice.option_id.clone()),
            PermissionResolution::ApproverAgent { agent: card.id },
        )
    }

    pub fn broadcast(&self) -> broadcast::Receiver<StreamMsg> {
        self.broadcast.subscribe()
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Park an external ask (from the ChatManager) in the permission
    /// inbox, scoped to the given context id (the chat id). The ask lands
    /// in the same inbox the panel/approver loop serve, keyed
    /// `<context_id>:<tool_call_id>`. Chat asks always reach a human
    /// unless a rule auto-answers (no approver delegation for chats).
    pub fn park_external_ask(self: &Arc<Self>, ask: PermissionAsk, context_id: RunId) {
        let PermissionAsk {
            tool_call_id,
            title,
            raw_input,
            choices,
            answer,
        } = ask;
        let info = PendingPermission {
            run_id: context_id,
            tool_call_id: tool_call_id.clone(),
            title,
            raw_input,
            choices,
        };
        let key = format!("{}:{}", context_id, tool_call_id);
        // Event tap: a dead channel — chat event streams come from the chat
        // broadcast, not the pending entry. (PermissionResolved attribution
        // events still flow through the chat session's own channel.)
        let (dead_tx, _dead_rx) = mpsc::unbounded_channel::<RunEvent>();
        drop(_dead_rx);
        self.pending
            .lock()
            .expect("pending lock")
            .insert(key, (info, dead_tx, answer));
    }

    /// Drop every ask parked for one chat. Called when a chat closes
    /// (explicit close, idle reaper, engine-restart): the dropped
    /// oneshot senders make the acp side fail closed (Cancel) and the
    /// inbox stops listing dead entries (issue #40).
    pub fn drop_pending_for(&self, context_id: RunId) {
        let prefix = format!("{context_id}:");
        self.pending
            .lock()
            .expect("pending lock")
            .retain(|k, _| !k.starts_with(&prefix));
    }

    /// Remove the per-run isolated workspaces of one task. Task
    /// deletion discards the task and its outputs, so the run
    /// worktrees (and their branches) must not accumulate (issue #43);
    /// runs that merely finished keep theirs — a worktree can hold the
    /// deliverable of a code-writing run.
    pub async fn cleanup_task_workspaces(&self, task_id: ruagent_core::TaskId) {
        let runs = match self.db.list_runs_for_task(task_id).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "listing runs for workspace cleanup");
                return;
            }
        };
        let worktrees = self.root.join("worktrees");
        let workspaces = self.root.join("workspaces");
        for run in runs {
            let Some(ws) = run.workspace else { continue };
            let dir = std::path::PathBuf::from(&ws);
            if dir.starts_with(&worktrees) {
                // A registered worktree: remove it through git (which
                // also prunes the admin entry), then the branch. A
                // vanished parent repo degrades to deleting the dir.
                let parent = std::process::Command::new("git")
                    .arg("-C")
                    .arg(&dir)
                    .args(["rev-parse", "--git-common-dir"])
                    .output();
                let repo = parent
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| std::path::PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
                if let Some(repo) = repo.filter(|r| r.is_dir()) {
                    let _ = std::process::Command::new("git")
                        .arg("-C")
                        .arg(&repo)
                        .args(["worktree", "remove", "--force"])
                        .arg(&dir)
                        .output();
                    let _ = std::process::Command::new("git")
                        .arg("-C")
                        .arg(&repo)
                        .args(["branch", "-D", &format!("ruagent/run-{}", run.id)])
                        .output();
                    tracing::info!(run_id = %run.id, "worktree removed with task");
                } else {
                    let _ = std::fs::remove_dir_all(&dir);
                }
            } else if dir.starts_with(&workspaces) {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
    }

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn transcripts_dir(&self) -> PathBuf {
        self.root.join("data").join("transcripts")
    }

    pub fn agent(&self, name: &str) -> Option<AgentCard> {
        self.agents.read().expect("agents lock").get(name).cloned()
    }

    /// Swap the live registry (registry edits call this after writing
    /// agents.toml). Cards keep their stable DB ids; new ones are
    /// resolved by the caller.
    pub fn reload_agents(&self, agents: Vec<AgentCard>) {
        let mut guard = self.agents.write().expect("agents lock");
        *guard = agents.into_iter().map(|a| (a.name.clone(), a)).collect();
        tracing::info!(count = guard.len(), "agent registry reloaded");
    }

    pub fn registry_view(&self) -> crate::distill::AgentRegistry {
        crate::distill::AgentRegistry {
            enabled: self
                .agents
                .read()
                .expect("agents lock")
                .values()
                .filter(|a| a.enabled)
                .cloned()
                .collect(),
        }
    }

    pub fn agents(&self) -> Vec<AgentCard> {
        let mut v: Vec<_> = self
            .agents
            .read()
            .expect("agents lock")
            .values()
            .cloned()
            .collect();
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
        self.resolve_internal(key, answer, PermissionResolution::Human)
    }

    fn resolve_internal(
        &self,
        key: &str,
        answer: PermissionAnswer,
        resolution: PermissionResolution,
    ) -> Result<()> {
        let entry = self
            .pending
            .lock()
            .expect("pending lock")
            .remove(key)
            .with_context(|| format!("no pending permission `{key}` (already resolved?)"))?;
        let (pending, ev_tx, answer_tx) = entry;

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
            resolution,
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
        workspace_spec: WorkspaceSpec,
        launch: RunLaunch,
    ) -> Result<Run> {
        let card = self
            .agent(agent_name)
            .with_context(|| format!("unknown agent `{agent_name}`"))?;
        let spec = adapter_for(card.harness)
            .spawn_spec(&card)
            .with_context(|| format!("resolving spawn command for `{agent_name}`"))?;
        // The role's identity rides every run (the same contract chats
        // use): a run against a ROLE must carry the role prompt, not
        // just the runtime behind it.
        let role = card.prompt.clone();
        let context_prefix = launch.context_prefix.clone();

        let mut run = Run::new(task.id, RunParams::for_agent(card.id));
        // Canonical session-option defaults (issue #36): mode / effort.
        // The ROLE's defaults apply to runs too — the same contract
        // chats have. Without this, a specialist registered with
        // `options = { mode = "auto" }` still asked for every edit in
        // run mode (request options win per key).
        let mut options = card.options.clone();
        for (k, v) in &launch.options {
            options.insert(k.clone(), v.clone());
        }
        run.params.options = options;
        // The original prompt (pre-injection): a retry replays it
        // faithfully (design §8.3 crash row).
        run.params.prompt = Some(
            launch
                .original_prompt
                .clone()
                .unwrap_or_else(|| prompt.clone()),
        );

        // Concurrency gate (design §8.3): a free slot spawns now; a
        // busy one queues the run — the row lands as Queued so the
        // fan-out reply lists every member — and a full queue rejects
        // the launch with a clear error.
        let gate = self.gate(card.harness);
        let hold = match gate.sem.clone().try_acquire_owned() {
            Ok(permit) => GateHold::Free(permit),
            Err(_) => {
                if let Err(e) = reserve_slot(
                    &gate,
                    card.harness,
                    self.policy.concurrency.queue_per_harness,
                    self.policy.concurrency.per_harness,
                ) {
                    run.status = RunStatus::Failed;
                    run.error = Some(e.to_string());
                    run.stop_reason = Some(StopReason::Error);
                    run.updated_at = chrono::Utc::now();
                    self.db.insert_run(&run).await?;
                    return Err(e);
                }
                run.status = RunStatus::Queued;
                run.updated_at = chrono::Utc::now();
                self.db.insert_run(&run).await?;
                GateHold::Waiting(gate)
            }
        };
        if let GateHold::Free(_) = &hold {
            run.status = RunStatus::Spawning;
            run.updated_at = chrono::Utc::now();
            self.db.insert_run(&run).await?;
        }

        let db = self.db.clone();
        let root = self.root.clone();
        let broadcast = self.broadcast.clone();
        let pending = self.pending.clone();
        let policy = self.policy.clone();
        let approver_id = self.approver_id;
        let approver_tx = self.approver_tx.clone();
        let cancel_token = tokio_util::sync::CancellationToken::new();
        self.cancellations
            .lock()
            .expect("cancellations lock")
            .insert(run.id, cancel_token.clone());
        let cancellations = self.cancellations.clone();
        let task_id = task.id;
        let run_id = run.id;
        let task = task.clone();

        let returned = run.clone();
        // The gate permit rides the supervisor: it releases when the
        // run reaches any terminal state (design §8.3). A queued run
        // waits for the gate here — cancellable — before it consumes
        // any resources.
        tokio::spawn(async move {
            let _permit = match hold {
                GateHold::Free(permit) => permit,
                GateHold::Waiting(g) => {
                    let permit = tokio::select! {
                        p = g.sem.clone().acquire_owned() => p.ok(),
                        _ = cancel_token.cancelled() => None,
                    };
                    g.waiting.fetch_sub(1, Ordering::SeqCst);
                    let Some(permit) = permit else {
                        if cancel_token.is_cancelled() {
                            run.status = RunStatus::Cancelled;
                            run.stop_reason = Some(StopReason::Cancelled);
                        } else {
                            run.status = RunStatus::Failed;
                            run.stop_reason = Some(StopReason::Error);
                            run.error = Some("harness gate closed".into());
                        }
                        run.updated_at = chrono::Utc::now();
                        let _ = db.update_run(&run).await;
                        cancellations
                            .lock()
                            .expect("cancellations lock")
                            .remove(&run_id);
                        return;
                    };
                    permit
                }
            };

            // Per-run isolated workspace (design §8.2) and the push path
            // of the injection contract (design SS6.4): bounded tagged
            // memory blocks prepended to the prompt; the render is also
            // emitted as a ContextInjected event for observability.
            // Prepared after the gate so a queued run touches nothing
            // while it waits.
            let cwd = match create_workspace(&root, workspace_spec, run_id) {
                Ok(dir) => dir,
                Err(err) => {
                    tracing::error!(run_id = %run_id, error = %err, "workspace prep failed");
                    run.status = RunStatus::Failed;
                    run.error = Some(err.to_string());
                    run.stop_reason = Some(StopReason::Error);
                    run.updated_at = chrono::Utc::now();
                    let _ = db.update_run(&run).await;
                    cancellations
                        .lock()
                        .expect("cancellations lock")
                        .remove(&run_id);
                    return;
                }
            };
            run.workspace = Some(cwd.to_string_lossy().into_owned());

            let mut injection = render_run_injection(&db, &task).await;
            // Attempt-history context (a retry's crash snapshot) rides
            // as context — visible in the ContextInjected render, never
            // as user speech.
            if let Some(prefix) = context_prefix.as_deref().filter(|p| !p.trim().is_empty()) {
                injection = if injection.is_empty() {
                    prefix.to_string()
                } else {
                    format!("{prefix}\n---\n{injection}")
                };
            }
            // Role identity first, then memory context (the chat
            // contract, mirrored for runs).
            if let Some(role) = role.as_deref().filter(|r| !r.trim().is_empty()) {
                injection = if injection.is_empty() {
                    format!("{}]\n{role}", crate::chat::HDR_ROLE)
                } else {
                    format!("{}]\n{role}\n---\n{injection}", crate::chat::HDR_ROLE)
                };
            }
            let mut prompt = prompt;
            if !injection.is_empty() {
                prompt = format!("{injection}\n---\n{prompt}");
            }

            if run.status == RunStatus::Queued {
                run.status = RunStatus::Spawning;
            }
            run.updated_at = chrono::Utc::now();
            let _ = db.update_run(&run).await;
            // A task goes (back) to in_progress whenever a run of it is
            // live — a retry on an already-Done task must not run under
            // a Done status.
            if matches!(task.status, TaskStatus::Pending | TaskStatus::Done) {
                let _ = db.update_task_status(task.id, TaskStatus::InProgress).await;
            }

            let result = supervise(
                db.clone(),
                root,
                broadcast,
                pending,
                policy,
                approver_id,
                approver_tx,
                run.clone(),
                task_id,
                spec,
                prompt,
                mcp_servers.clone(),
                cwd,
                launch.routed,
                injection,
                cancel_token,
            )
            .await;
            if let Err(err) = result {
                tracing::error!(run_id = %run_id, error = %err, "run supervisor failed");
                let mut run = run;
                run.status = RunStatus::Failed;
                let mut message = err.to_string();
                if !mcp_servers.is_empty() && looks_like_mcp_failure(&message) {
                    message.push_str(&diagnose_mcp_servers(&mcp_servers).await);
                }
                run.error = Some(message);
                run.stop_reason = Some(StopReason::Error);
                run.updated_at = chrono::Utc::now();
                let _ = db.update_run(&run).await;
            }
            cancellations
                .lock()
                .expect("cancellations lock")
                .remove(&run_id);
        });

        Ok(returned)
    }

    /// One-click retry of a dead run (design §8.3 crash row): a new
    /// run on the same task with the same agent and options, reusing
    /// the crashed attempt's workspace (half-written work survives),
    /// and a context snapshot from its transcript so the agent knows
    /// where it died instead of starting from zero.
    pub async fn retry_run(&self, run_id: RunId) -> Result<Run> {
        let old = self
            .db
            .get_run(run_id)
            .await?
            .with_context(|| format!("run {run_id} not found"))?;
        anyhow::ensure!(
            old.status.is_retryable(),
            "only failed, interrupted or cancelled runs can be retried (this one is {:?})",
            old.status
        );
        let task = self
            .db
            .get_task(old.task_id)
            .await?
            .with_context(|| format!("task {} for retry not found", old.task_id))?;
        let card = self
            .agents()
            .into_iter()
            .find(|c| c.id == old.params.agent)
            .with_context(|| "the retried run's agent is no longer registered")?;

        // The original ask: recorded as-is even though the retry
        // composes context onto it (chained retries must not grow it).
        let original = old
            .params
            .prompt
            .clone()
            .unwrap_or_else(|| task.intent.clone());
        // The crash snapshot rides as CONTEXT (ContextInjected), not as
        // user speech — session previews and distillation stay clean.
        let context_prefix = retry_context(&self.transcripts_dir(), &old);
        // The workspace carries the crashed attempt's half-written
        // work: the retry continues in place.
        let workspace = old
            .workspace
            .as_deref()
            .map(std::path::PathBuf::from)
            .map(WorkspaceSpec::Cwd)
            .unwrap_or(WorkspaceSpec::Fresh);
        self.start_run(
            &task,
            &card.name,
            original.clone(),
            self.mcp_for(&card),
            workspace,
            RunLaunch {
                routed: Some(RoutingDecision {
                    agents: vec![card.id],
                    source: RouteSource::Explicit,
                    rationale: Some(format!("retry of run {run_id}")),
                }),
                options: old.params.options.clone(),
                original_prompt: Some(original),
                context_prefix,
            },
        )
        .await
    }

    /// Fan out: same prompt to N agents in parallel on one task
    /// (design §5.2). Every run records its routing provenance.
    pub async fn start_fanout(
        &self,
        task: &Task,
        agents: &[String],
        prompt: String,
        repo: Option<PathBuf>,
        options: std::collections::BTreeMap<String, String>,
    ) -> Result<Vec<Run>> {
        let topology = Topology::FanOut {
            agents: agents.to_vec(),
        };
        let plan = ruagent_orchestrator::plan(task, &topology)?;
        let mut runs = Vec::new();
        for planned in &plan.runs {
            let card = self
                .agent(&planned.agent)
                .with_context(|| format!("unknown agent `{}`", planned.agent))?;
            let decision = RoutingDecision {
                agents: vec![card.id],
                source: RouteSource::Explicit,
                rationale: Some("fan-out member".into()),
            };
            let mcp = self.mcp_for(&card);
            let spec = match &repo {
                Some(r) => WorkspaceSpec::Worktree { repo: r.clone() },
                None => WorkspaceSpec::Fresh,
            };
            let run = self
                .start_run(
                    task,
                    &planned.agent,
                    prompt.clone(),
                    mcp,
                    spec,
                    RunLaunch {
                        routed: Some(decision),
                        options: options.clone(),
                        ..Default::default()
                    },
                )
                .await?;
            runs.push(run);
        }
        Ok(runs)
    }

    /// Pipeline: sequential sub-tasks with handoff (design §5.2). Returns
    /// the sub-task ids; the driver runs in the background.
    pub async fn start_pipeline(
        self: &Arc<Self>,
        task: &Task,
        steps: Vec<PipelineStep>,
        base_prompt: String,
    ) -> Result<Vec<ruagent_core::TaskId>> {
        let topology = Topology::Pipeline { steps };
        let plan = ruagent_orchestrator::plan(task, &topology)?;
        for planned in &plan.runs {
            if planned.task.id != task.id {
                self.db.insert_task(&planned.task).await?;
            }
        }
        for edge in &plan.edges {
            self.db.insert_edge(*edge).await?;
        }
        let task_ids: Vec<ruagent_core::TaskId> = plan.runs.iter().map(|r| r.task.id).collect();

        let mgr = Arc::clone(self);
        let base = task.clone();
        tokio::spawn(async move {
            let mut handoff: Option<String> = None;
            for (i, planned) in plan.runs.iter().enumerate() {
                let prompt = match (&planned.prompt, &handoff) {
                    (Some(p), _) => p.clone(),
                    (None, Some(up)) => compose_handoff(&base.intent, up),
                    (None, None) => base_prompt.clone(),
                };
                let card = match mgr.agent(&planned.agent) {
                    Some(c) => c,
                    None => {
                        tracing::error!(agent = %planned.agent, "pipeline step agent missing");
                        cancel_unstarted(&mgr.db, &plan.runs[i..]).await;
                        break;
                    }
                };
                let decision = RoutingDecision {
                    agents: vec![card.id],
                    source: RouteSource::Explicit,
                    rationale: Some(format!("pipeline step (upstream: {})", handoff.is_some())),
                };
                let mcp = mgr.mcp_for(&card);
                let run = match mgr
                    .start_run(
                        &planned.task,
                        &planned.agent,
                        prompt,
                        mcp,
                        WorkspaceSpec::Fresh,
                        RunLaunch {
                            routed: Some(decision),
                            ..Default::default()
                        },
                    )
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!(error = %e, "pipeline step failed to start");
                        // start_run failed before insert_run: step i's task
                        // is still Pending, so cancel from it (inclusive).
                        cancel_unstarted(&mgr.db, &plan.runs[i..]).await;
                        break;
                    }
                };
                let final_run = match wait_terminal(&mgr.db, run.id).await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!(error = %e, "pipeline wait failed");
                        cancel_unstarted(&mgr.db, &plan.runs[i + 1..]).await;
                        break;
                    }
                };
                if final_run.status != RunStatus::Completed {
                    tracing::warn!(run_id = %run.id, status = ?final_run.status, "pipeline step did not complete; stopping chain");
                    cancel_unstarted(&mgr.db, &plan.runs[i + 1..]).await;
                    break;
                }
                handoff = final_run.result;
            }
        });
        Ok(task_ids)
    }

    /// Fan-out judge (design §5.2 winner selection, §5.3 everything-is-a-run):
    /// a NORMAL run on a judge sub-task that reviews the completed
    /// candidate runs and picks one. Its verdict — a `RUN:` line naming a
    /// candidate — becomes `task.selected_run_id` with `agent:<name>`
    /// provenance; the Reviews edge links the judge task to the parent.
    /// The judge completes in the background; the returned run is live.
    pub async fn start_judge(
        self: &Arc<Self>,
        task: &Task,
        judge_agent: &str,
        candidates: &[JudgeCandidate],
    ) -> Result<Run> {
        let card = self
            .agent(judge_agent)
            .with_context(|| format!("unknown agent `{judge_agent}`"))?;
        if candidates.len() < 2 {
            anyhow::bail!("judging needs at least two completed runs with results");
        }
        let prompt = compose_judge_prompt(task, candidates);

        let mut judge_task = Task::new(
            format!("judge: {}", task.title),
            prompt.clone(),
            TaskCreator::Rule {
                name: "judge".into(),
            },
        );
        judge_task.project = Some("ruagent-internal".into());
        self.db.insert_task(&judge_task).await?;
        self.db
            .insert_edge(TaskEdge {
                from: task.id,
                to: judge_task.id,
                kind: EdgeKind::Reviews,
            })
            .await?;

        let decision = RoutingDecision {
            agents: vec![card.id],
            source: RouteSource::Explicit,
            rationale: Some("fan-out judge".into()),
        };
        let mcp = self.mcp_for(&card);
        let run = self
            .start_run(
                &judge_task,
                judge_agent,
                prompt,
                mcp,
                WorkspaceSpec::Fresh,
                RunLaunch {
                    routed: Some(decision),
                    ..Default::default()
                },
            )
            .await?;

        let mgr = Arc::clone(self);
        let task_id = task.id;
        let by = format!("agent:{judge_agent}");
        let candidate_ids: Vec<RunId> = candidates.iter().map(|c| c.run_id).collect();
        tokio::spawn(async move {
            let final_run = match wait_terminal(&mgr.db, run.id).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = %e, "judge run wait failed");
                    return;
                }
            };
            let Some(reply) = final_run.result.as_deref() else {
                tracing::warn!(status = ?final_run.status, "judge run produced no result");
                return;
            };
            match parse_judge_verdict(reply, &candidate_ids) {
                Some((winner, why)) => {
                    tracing::info!(winner = %winner, rationale = why.as_deref().unwrap_or("-"), "judge verdict recorded");
                    if let Err(e) = mgr.db.set_selected_run(task_id, winner, &by).await {
                        tracing::warn!(error = %e, "recording judge selection failed");
                    }
                }
                None => tracing::warn!(
                    reply = &reply[..reply.len().min(200)],
                    "judge reply carried no valid RUN: verdict; selection unchanged"
                ),
            }
        });
        Ok(run)
    }

    /// Land the fan-out winner (design §5.2): commit any uncommitted
    /// work in the selected run's worktree, merge its branch into the
    /// main repo, then sweep the task's worktrees and branches (issue
    /// #43's rule — the deliverable is safe in the main checkout now).
    /// Returns the post-merge HEAD short hash. A failed merge returns
    /// git's own message and cleans nothing: a conflict is a human
    /// decision, not ours to discard.
    pub async fn land_selected(&self, task_id: ruagent_core::TaskId) -> Result<String> {
        let task = self
            .db
            .get_task(task_id)
            .await?
            .with_context(|| format!("task {task_id} not found"))?;
        let (run_id, _by) = self
            .db
            .selected_run(task_id)
            .await?
            .context("no winner selected")?;
        let run = self
            .db
            .get_run(run_id)
            .await?
            .with_context(|| format!("selected run {run_id} not found"))?;
        // Only a run's own worktree can land — a fresh or explicit-cwd
        // workspace has no branch to merge.
        let worktrees = self.root.join("worktrees");
        let ws = run
            .workspace
            .as_deref()
            .map(std::path::Path::new)
            .filter(|d| d.starts_with(&worktrees) && d.is_dir())
            .context("winner has no worktree to land")?;
        let branch = format!("ruagent/run-{run_id}");

        // The main checkout the worktree was cut from.
        let common = git(
            ws,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let main = std::path::Path::new(&common)
            .parent()
            .context("git common dir has no parent")?
            .to_path_buf();

        // Uncommitted work in the worktree is part of the deliverable:
        // land it on the run branch so the merge carries it.
        if !git(ws, &["status", "--porcelain"])?.is_empty() {
            git(ws, &["add", "-A"])?;
            let msg = format!("ruagent: land run {run_id} ({})", task.title);
            git(
                ws,
                &[
                    "-c",
                    "user.name=ruagent",
                    "-c",
                    "user.email=ruagent@localhost",
                    "commit",
                    "-m",
                    &msg,
                ],
            )?;
        } else {
            // Clean worktree: nothing beyond main's HEAD on the branch
            // means the merge would be a no-op.
            let range = format!("HEAD..{branch}");
            if git(&main, &["rev-list", "--count", &range])? == "0" {
                anyhow::bail!("winner produced nothing to land");
            }
        }

        git(&main, &["merge", "--no-edit", &branch])?;
        self.cleanup_task_workspaces(task_id).await;
        let head = git(&main, &["rev-parse", "--short", "HEAD"])?;
        tracing::info!(run_id = %run_id, hash = %head, "winner landed");
        Ok(head)
    }

    /// MCP servers for an agent's configured profile (design §7.1).
    fn mcp_for(&self, card: &AgentCard) -> Vec<agent_client_protocol::schema::v1::McpServer> {
        self.mcp
            .expand_profile(card.mcp_profile.as_deref(), &card.name)
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
    approver_id: Option<ruagent_core::AgentId>,
    approver_tx: mpsc::UnboundedSender<PendingPermission>,
    mut run: Run,
    task_id: ruagent_core::TaskId,
    spec: ruagent_acp::SpawnSpec,
    prompt: String,
    mcp_servers: Vec<agent_client_protocol::schema::v1::McpServer>,
    cwd: PathBuf,
    routed: Option<RoutingDecision>,
    injection: String,
    cancel_token: tokio_util::sync::CancellationToken,
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

    if let Some(decision) = routed {
        emit(
            &mut transcript,
            &broadcast,
            &run,
            RunEvent::Routed { decision },
        );
    }
    if !injection.is_empty() {
        emit(
            &mut transcript,
            &broadcast,
            &run,
            RunEvent::ContextInjected {
                render: injection.clone(),
            },
        );
    }
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
        prompt: prompt.clone(),
        options: run
            .params
            .options
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };
    // The user_message event carries the ORIGINAL ask only — injected
    // memory and retry context ride separately (ContextInjected / the
    // dead attempt's transcript), the same contract chats use. Putting
    // the composed prompt here leaked injection text into session
    // previews and fed it back into distillation as user speech.
    let asked = run.params.prompt.clone().unwrap_or_else(|| prompt.clone());
    emit(
        &mut transcript,
        &broadcast,
        &run,
        RunEvent::UserMessage { text: asked },
    );
    let mut driver = tokio::spawn(run_once(opts, ev_tx.clone(), ask_tx));

    let mut ev_open = true;
    let mut ask_open = true;
    let mut done = false;
    let mut done_at: Option<tokio::time::Instant> = None;
    let mut result_text = String::new();

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
                    if let RunEvent::AgentMessageChunk { content } = &event {
                        for block in content {
                            if let Some(text) = block.as_text() {
                                result_text.push_str(text);
                            }
                        }
                    }
                    // The ACP session went live: leave "spawning" (a
                    // run waiting on a permission mid-research must not
                    // read as still-spawning — issue #34).
                    if let RunEvent::StateChanged {
                        status: ruagent_core::RunStatus::Running,
                    } = &event
                    {
                        run.status = ruagent_core::RunStatus::Running;
                        run.updated_at = chrono::Utc::now();
                        let r = run.clone();
                        let db = db.clone();
                        tokio::spawn(async move {
                            let _ = db.update_run(&r).await;
                        });
                    }
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
                    let asking = run.params.agent.to_string();
                    let approver = approver_id.map(|i| i.to_string());
                    match policy.path(&title, Some(&asking), approver.as_deref()) {
                        ruagent_policy::PermissionPath::Auto(action) => {
                            let wanted = if matches!(action, ruagent_policy::PermissionAction::Allow) {
                                [PermissionKind::AllowOnce, PermissionKind::AllowAlways]
                            } else {
                                [PermissionKind::RejectOnce, PermissionKind::RejectAlways]
                            };
                            match choices.iter().find(|c| wanted.contains(&c.kind)) {
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
                        ruagent_policy::PermissionPath::Delegate => {
                            // Park for the human (override still possible)
                            // AND hand to the approver agent (tier 2).
                            let key = format!("{}:{}", info.run_id, info.tool_call_id);
                            pending
                                .lock()
                                .expect("pending lock")
                                .insert(key, (info.clone(), ev_tx.clone(), answer));
                            let _ = approver_tx.send(info);
                        }
                        ruagent_policy::PermissionPath::Human => {
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
            _ = cancel_token.cancelled(), if !done => {
                // Cancellation: abort the driver task — the abort drops
                // the run_once future at its await point, dropping the
                // connection and tearing down the child process group
                // (the SDK's ChildGuard). Merely detaching it would let
                // the agent run on, burning tokens under a row already
                // marked Cancelled (issue #39). Parked asks fail closed:
                // their oneshot senders drop, the acp side reads Cancel.
                driver.abort();
                done = true;
                run.status = RunStatus::Cancelled;
                run.stop_reason = Some(StopReason::Cancelled);
                run.updated_at = chrono::Utc::now();
                emit(
                    &mut transcript,
                    &broadcast,
                    &run,
                    RunEvent::Stopped {
                        stop_reason: StopReason::Cancelled,
                    },
                );
                emit(
                    &mut transcript,
                    &broadcast,
                    &run,
                    RunEvent::StateChanged {
                        status: RunStatus::Cancelled,
                    },
                );
            }
            res = &mut driver, if !done => {
                done = true;
                run.updated_at = chrono::Utc::now();
                match res {
                    Ok(Ok(outcome)) => {
                        run.status = RunStatus::Completed;
                        run.stop_reason = Some(outcome.stop_reason);
                        run.acp_session_id = Some(outcome.session_id);
                        if !result_text.is_empty() {
                            run.result = Some(result_text.clone());
                        }
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
            }
        }
    }

    // Dead asks for a finished/cancelled run can never be answered
    // usefully; drop them so the inbox stays honest.
    {
        let prefix = format!("{}:", run.id);
        let mut p = pending.lock().expect("pending lock");
        p.retain(|k, _| !k.starts_with(&prefix));
    }

    // Finalize only AFTER the drain: a run that is terminal in the DB or
    // announced as Finished must have every event (including stragglers
    // that raced the prompt response) already appended and flushed.
    // Straggler chunks appended after the driver arm snapshotted
    // `result` must land in the persisted result too — the review round
    // lost its entire final report to this ordering once.
    if !result_text.is_empty() {
        run.result = Some(result_text);
    }
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

/// Compose a handoff prompt: bounded upstream result (design §5.2 —
/// bounded summary until the M3 injection contract lands).
fn compose_handoff(intent: &str, upstream: &str) -> String {
    const MAX_UPSTREAM_CHARS: usize = 4000;
    let bounded: String = if upstream.chars().count() > MAX_UPSTREAM_CHARS {
        let cut: String = upstream.chars().take(MAX_UPSTREAM_CHARS).collect();
        format!(
            "{cut}
…[upstream result truncated at {MAX_UPSTREAM_CHARS} chars]"
        )
    } else {
        upstream.to_string()
    };
    format!(
        "{intent}

--- Upstream result (handoff) ---
{bounded}
--- End upstream result ---

Continue from the upstream result."
    )
}

/// One candidate shown to a fan-out judge: the run, the agent's display
/// name, and its (bounded) result.
#[derive(Debug, Clone)]
pub struct JudgeCandidate {
    pub run_id: RunId,
    pub agent_name: String,
    pub cost_usd: Option<f64>,
    pub result: String,
}

/// Build the judge prompt. The `[RUN <id>]` markers are part of the
/// contract — the judge reply references them, and the mock agent's judge
/// behavior keys on them.
fn compose_judge_prompt(task: &Task, candidates: &[JudgeCandidate]) -> String {
    const MAX_RESULT_CHARS: usize = 3000;
    let mut out = format!(
        "You are judging fan-out results for a task. Compare the candidate runs and pick the single best one.\n\nTask: {}\nIntent: {}\n\nCandidates (in run order):\n",
        task.title, task.intent
    );
    for (i, c) in candidates.iter().enumerate() {
        let bounded: String = if c.result.chars().count() > MAX_RESULT_CHARS {
            let cut: String = c.result.chars().take(MAX_RESULT_CHARS).collect();
            format!("{cut}\n…[truncated at {MAX_RESULT_CHARS} chars]")
        } else {
            c.result.clone()
        };
        let cost = c
            .cost_usd
            .map(|v| format!("${v:.4}"))
            .unwrap_or_else(|| "-".into());
        out.push_str(&format!(
            "\n[{}] [RUN {}] agent: {} · cost: {}\n{}\n",
            i + 1,
            c.run_id,
            c.agent_name,
            cost,
            bounded
        ));
    }
    out.push_str(
        "\nReply with exactly two lines:\nRUN: <the run id of the best candidate>\nWHY: <one short paragraph justifying the choice>\n",
    );
    out
}

/// Parse a judge reply. The first `RUN:` line must name a candidate (full
/// id, or an unambiguous prefix of at least 8 chars); the first `WHY:`
/// line is the rationale (optional). `None` = no usable verdict.
pub(crate) fn parse_judge_verdict(
    reply: &str,
    candidates: &[RunId],
) -> Option<(RunId, Option<String>)> {
    let line_value = |key: &str| -> Option<String> {
        reply.lines().find_map(|l| {
            let t = l.trim();
            t.to_lowercase()
                .strip_prefix(key)
                .map(|rest| rest.trim().to_string())
                .filter(|v| !v.is_empty())
        })
    };
    let chosen = line_value("run:")?;
    let winner = match candidates
        .iter()
        .copied()
        .find(|id| id.to_string() == chosen)
    {
        Some(id) => Some(id),
        // Tolerate shortened ids — but only when unambiguous.
        // (`then_some` would index eagerly — len 0 must not panic.)
        None if chosen.len() >= 8 => {
            let matches: Vec<RunId> = candidates
                .iter()
                .copied()
                .filter(|id| id.to_string().starts_with(&chosen))
                .collect();
            if matches.len() == 1 {
                Some(matches[0])
            } else {
                None
            }
        }
        None => None,
    }?;
    Some((winner, line_value("why:")))
}

/// Mark never-started pipeline sub-tasks cancelled so a broken chain does
/// not leave them dangling in Pending forever (design §5.1 task states).
async fn cancel_unstarted(db: &Db, planned: &[ruagent_orchestrator::PlannedRun]) {
    for p in planned {
        if let Err(e) = db
            .update_task_status(p.task.id, TaskStatus::Cancelled)
            .await
        {
            tracing::warn!(error = %e, "cancelling downstream pipeline task failed");
        }
    }
}

/// Wait for a run to reach a terminal state (poll; 30-minute ceiling).
async fn wait_terminal(db: &Db, run_id: RunId) -> Result<Run> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1800);
    loop {
        let run = db
            .get_run(run_id)
            .await?
            .with_context(|| "run vanished while waiting")?;
        if run.status.is_terminal() {
            return Ok(run);
        }
        // bail, not assert: this runs in background drivers where a panic
        // would kill the driver silently.
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("run did not reach a terminal state within 30 minutes");
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// The crash snapshot for a retry (design §8.3): what the dead attempt
/// last said, so the replacement run continues instead of restarting
/// from zero. Bounded — the tail, not the whole transcript.
fn retry_context(transcripts: &std::path::Path, old: &Run) -> Option<String> {
    let path = transcript_path(transcripts, &old.id);
    let lines = ruagent_store::read_transcript(&path).ok()?;
    let mut text = String::new();
    let mut last_tool = None;
    for line in &lines {
        match &line.event {
            ruagent_core::RunEvent::AgentMessageChunk { content } => {
                for block in content {
                    if let Some(t) = block.as_text() {
                        text.push_str(t);
                    }
                }
            }
            // Where a run died often shows in its last tool call, not
            // its last words.
            ruagent_core::RunEvent::ToolCall { title, .. } => last_tool = Some(title.clone()),
            _ => {}
        }
    }
    let why = old
        .error
        .as_deref()
        .map(|e| format!(" — {e}"))
        .unwrap_or_default();
    if text.is_empty() {
        // Died before saying anything: the death reason still rides
        // (an early-death retry must not lose it).
        return old.error.as_deref().map(|_| {
            format!(
                "{} — the previous attempt (run {}) died{why} before producing any output.]",
                crate::chat::retry_head(),
                old.id
            )
        });
    }
    // Keep the tail (the last thing the agent was doing), bounded in
    // CHARACTERS with a visible truncation marker — the repo's
    // injection convention (memory/inject.rs, compose_handoff, judge).
    const BOUND: usize = 1500;
    let total = text.chars().count();
    let tail = if total > BOUND {
        let skipped = total - BOUND;
        let kept: String = text.chars().skip(skipped).collect();
        format!("…[+{skipped} chars truncated] {kept}")
    } else {
        text
    };
    let tool = last_tool
        .map(|t| format!(" Last tool call: {t}."))
        .unwrap_or_default();
    Some(format!(
        "{} — the previous attempt (run {}) died{why}.{tool} Its last output before dying:] {tail}",
        crate::chat::retry_head(),
        old.id
    ))
}

/// Run git in `dir`, returning trimmed stdout. A non-zero exit carries
/// git's own output verbatim — stdout first, then stderr: a merge
/// conflict reports on stdout while most errors use stderr, and the
/// caller (the land API) must relay what git actually said.
fn git(dir: &std::path::Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .with_context(|| format!("spawning git in {}", dir.display()))?;
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let mut msg = String::new();
        if !stdout.trim().is_empty() {
            msg.push_str(stdout.trim());
            msg.push('\n');
        }
        msg.push_str(stderr.trim());
        anyhow::bail!("{msg}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Per-run isolated workspace (design §8.2): a fresh dir under the
/// data root, an explicit cwd, or a git worktree branched for this run.
fn create_workspace(root: &std::path::Path, spec: WorkspaceSpec, run_id: RunId) -> Result<PathBuf> {
    Ok(match spec {
        WorkspaceSpec::Fresh => {
            let dir = root.join("workspaces").join(format!("run-{}", run_id));
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("creating workspace {}", dir.display()))?;
            dir
        }
        WorkspaceSpec::Cwd(dir) => dir,
        WorkspaceSpec::Worktree { repo } => {
            let dir = root.join("worktrees").join(format!("run-{}", run_id));
            let branch = format!("ruagent/run-{}", run_id);
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["worktree", "add", "-b", &branch])
                .arg(&dir)
                .output()
                .with_context(|| "spawning git for worktree add")?;
            if !out.status.success() {
                anyhow::bail!(
                    "git worktree add failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            tracing::info!(branch = %branch, worktree = %dir.display(), "worktree created");
            dir
        }
    })
}

/// Assemble the push-path injection for a run (design SS6.4): user
/// profile + user observations, bounded by the default budget.
async fn render_run_injection(db: &Db, task: &Task) -> String {
    use ruagent_memory::query::current_memories;
    use ruagent_memory::{InjectionBudget, MemoryForInjection, MemoryStore, render_injection};
    let mut mems: Vec<MemoryForInjection> = Vec::new();
    if let Ok(profile) = current_memories(db, MemoryStore::Profile, "user", 5).await {
        mems.extend(profile.into_iter().map(|m| MemoryForInjection {
            tag: "user_profile",
            content: m.content,
            updated_at: m.updated_at,
        }));
    }
    if let Ok(obs) = current_memories(db, MemoryStore::Observation, "user", 8).await {
        mems.extend(obs.into_iter().map(|m| MemoryForInjection {
            tag: "relevant_memories",
            content: m.content,
            updated_at: m.updated_at,
        }));
    }
    if let Some(project) = &task.project
        && let Ok(obs) = current_memories(
            db,
            MemoryStore::Observation,
            &format!("project:{project}"),
            8,
        )
        .await
    {
        mems.extend(obs.into_iter().map(|m| MemoryForInjection {
            tag: "project_context",
            content: m.content,
            updated_at: m.updated_at,
        }));
    }
    if mems.is_empty() {
        return String::new();
    }
    render_injection(&mems, &InjectionBudget::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_is_bounded() {
        let long = "x".repeat(10_000);
        let p = compose_handoff("do things", &long);
        assert!(p.chars().count() < 5_000, "handoff must be bounded");
        assert!(p.contains("truncated"));
        let p = compose_handoff("intent", "short");
        assert!(p.contains("short"));
        assert!(p.contains("Upstream result"));
    }

    fn candidate(id: &str, result: &str) -> JudgeCandidate {
        JudgeCandidate {
            run_id: id.parse().unwrap(),
            agent_name: format!("agent-{}", &id[..8]),
            cost_usd: Some(0.01),
            result: result.into(),
        }
    }

    #[test]
    fn judge_prompt_lists_candidates_with_run_markers() {
        let task = Task::new("Pick", "pick the best", TaskCreator::Human);
        let prompt = compose_judge_prompt(
            &task,
            &[
                candidate("11111111-1111-1111-1111-111111111111", "first result"),
                candidate("22222222-2222-2222-2222-222222222222", &"x".repeat(4_000)),
            ],
        );
        assert!(prompt.contains("[RUN 11111111-1111-1111-1111-111111111111]"));
        assert!(prompt.contains("agent-11111111"));
        assert!(prompt.contains("$0.0100"));
        assert!(prompt.contains("truncated at 3000 chars"));
        assert!(prompt.contains("RUN: <the run id"));
    }

    #[test]
    fn judge_verdict_parses_exact_prefix_and_why() {
        let ids: Vec<RunId> = [
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();

        // Exact id + rationale.
        let (w, why) = parse_judge_verdict(
            "RUN: 22222222-2222-2222-2222-222222222222\nWHY: cheaper",
            &ids,
        )
        .unwrap();
        assert_eq!(w, ids[1]);
        assert_eq!(why.as_deref(), Some("cheaper"));

        // Unambiguous 8-char prefix, no WHY, noisy casing/spacing.
        let (w, why) = parse_judge_verdict("  run:   11111111  \nother lines", &ids).unwrap();
        assert_eq!(w, ids[0]);
        assert!(why.is_none());

        // Unknown id, and an ambiguous prefix: both reject.
        assert!(parse_judge_verdict("RUN: 33333333-3333", &ids).is_none());
        let same_prefix: Vec<RunId> = [
            "11111111-1111-1111-1111-111111111111",
            "11111111-2222-2222-2222-222222222222",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
        assert!(parse_judge_verdict("RUN: 11111111", &same_prefix).is_none());
        // No RUN: line at all.
        assert!(parse_judge_verdict("I liked the second one", &ids).is_none());
    }
}
