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

/// Which HUMAN DISPATCH of a task a launch belongs to (P5 of
/// `docs/plans/2026-11-23-reports-never-lost.md`).
///
/// The value that lands on the attempt is `RunParams::dispatch_epoch`; this type is only about WHO
/// decides it, and the two cases must not be conflated:
///
/// * [`Self::New`] — a dispatch a human (or the panel, or `task_dispatch`) just made. `start_run`
///   numbers it from the cluster: the count of non-self-heal attempts already on the task. That
///   number is what gives the dispatch its ONE automatic retry.
/// * [`Self::Of`] — this attempt belongs to a generation that already exists, so the caller carries
///   its number forward. Two things use it: a self-heal (same generation as the attempt it heals,
///   otherwise its own failure would look like a fresh dispatch and heal again, and again) and every
///   member of one fan-out (one dispatch, one generation — numbering each branch separately would
///   hand the Nth branch a budget of its own, which is the bug P5 fixes wearing a new costume).
///
/// `Of(None)` is a legacy, un-versioned generation: rows written before `RunParams::dispatch_epoch`
/// existed. See that field's comment for why `None` is the conservative direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DispatchEpoch {
    /// A NEW dispatch: `start_run` numbers it from the cluster.
    #[default]
    New,
    /// An EXISTING generation, carried forward (`None` = legacy/un-versioned).
    Of(Option<u32>),
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
    /// WHICH CONVERSATION ASKED FOR THIS ATTEMPT (P2.1 of
    /// `docs/plans/2026-11-20-primary-supervisor-design.md` §3).
    ///
    /// `None` = the panel, the CLI or an external client: whoever asked is looking at the answer
    /// already, so the completion hook wakes nobody. `Some(chat id)` = a chat dispatched it, and
    /// that chat is told once when the whole CLUSTER (same `task_id`) is terminal.
    ///
    /// It is a field of the LAUNCH rather than of the task because attribution belongs to the
    /// dispatch, not to the work item: one task can be dispatched by a chat and later re-dispatched
    /// from the board, and the second dispatch has nobody to wake.
    pub origin_chat_id: Option<String>,
    /// P2.3: set when THIS attempt is the cluster's ONE self-heal retry, naming the attempt it
    /// heals. Read back by the completion hook to prove the budget is spent (see
    /// `RunParams::self_heal_of`).
    pub self_heal_of: Option<RunId>,
    /// P5: which human dispatch this attempt belongs to (see [`DispatchEpoch`]). `New` is the
    /// default because a launch that says nothing is a fresh dispatch — the panel, the API, the
    /// watchdog and `task_dispatch` all reach here through it.
    pub dispatch_epoch: DispatchEpoch,
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
    /// The daemon's ONE knowledge handle (t260). Set by lib.rs at boot; None
    /// in tests that do not need retrieval, which yields no knowledge block
    /// rather than a second embedder decision.
    knowledge: Mutex<Option<Arc<ruagent_knowledge::Knowledge>>>,
    /// The daemon's ONE capability-plane handle (t8), shared with the
    /// ChatManager: `lib.rs` installs `chats.capabilities_handle()` here, so a
    /// runtime `PUT /api/v1/capabilities` narrows the next run prompt too and
    /// there is still exactly one plane in the process.
    ///
    /// `None` — a test harness that never installs one — is LEGACY: every gate
    /// answers with today's behaviour (law L1). Optional by construction, the
    /// same reason `knowledge` is.
    capabilities: Mutex<Option<Arc<std::sync::RwLock<crate::capability::CapabilityPlane>>>>,
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
            knowledge: Mutex::new(None),
            capabilities: Mutex::new(None),
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

    /// Hand the manager the ONE knowledge handle the daemon built at boot.
    ///
    /// WHY A SETTER AND NOT A new() PARAMETER: RunManager::new is called from
    /// lib.rs, the api test harness and nine mock-agent tests; widening its
    /// signature would edit eleven files to deliver one handle. Optional by
    /// construction: a caller that does not set it gets NO knowledge block
    /// (honest) instead of a second embedder decision.
    pub fn set_knowledge(&self, k: Arc<ruagent_knowledge::Knowledge>) {
        *self.knowledge.lock().expect("knowledge lock") = Some(k);
    }

    /// The handle, if one was set.
    fn knowledge_handle(&self) -> Option<Arc<ruagent_knowledge::Knowledge>> {
        self.knowledge.lock().expect("knowledge lock").clone()
    }

    /// Hand the manager the ONE capability-plane handle the daemon built at
    /// boot (t8). A setter, not a `new()` parameter, for exactly the reason
    /// `set_knowledge` is one: `RunManager::new` is called from eleven places,
    /// and the handle is delivered once, at boot.
    pub fn set_capabilities(
        &self,
        caps: Arc<std::sync::RwLock<crate::capability::CapabilityPlane>>,
    ) {
        *self.capabilities.lock().expect("capabilities lock") = Some(caps);
    }

    /// The shared capability handle, if boot installed one.
    fn capabilities_handle(
        &self,
    ) -> Option<Arc<std::sync::RwLock<crate::capability::CapabilityPlane>>> {
        self.capabilities.lock().expect("capabilities lock").clone()
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
    async fn run_approver(self: &Arc<Self>, ask: PendingPermission) -> Result<()> {
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

    /// The id of the agent the permission cascade designates as its APPROVER, resolved exactly the way
    /// tier 2 resolves it above (`policy.approver.agent`, a NAME, looked up in this registry), or
    /// `None` when none is configured or it is not registered (t43).
    ///
    /// The routing fallback reads this to refuse to select the adjudicator by accident. It is the same
    /// identity the run path uses, deliberately: a `kind` test, a name literal or a prompt substring
    /// would each be a second, divergent notion of "the approver".
    pub fn approver_id(&self) -> Option<ruagent_core::AgentId> {
        self.approver_id
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
                    if let Err(err) = std::fs::remove_dir_all(&dir)
                        && err.kind() != std::io::ErrorKind::NotFound
                    {
                        tracing::warn!(
                            dir = %dir.display(),
                            error = %err,
                            "removing an unregistered worktree directory failed"
                        );
                    }
                }
            } else if dir.starts_with(&workspaces)
                && let Err(err) = std::fs::remove_dir_all(&dir)
                && err.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(
                    dir = %dir.display(),
                    error = %err,
                    "removing a workspace outside the registry failed"
                );
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
    ///
    /// `self: &Arc<Self>` (not `&self`) because the supervisor task this method spawns can carry
    /// the manager: the completion hook needs `retry_run` and `start_run`, and a spawn is `'static`.
    /// Every caller in the daemon and the tests already holds an `Arc<RunManager>`.
    pub async fn start_run(
        self: &Arc<Self>,
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
        // P2.1/P2.3: attribution and the self-heal marker are decided at DISPATCH and ride the row
        // (the column and the params JSON), so the completion hook of a LATER attempt (a heal
        // retry) reads the same two facts without a second table.
        run.origin_chat_id = launch.origin_chat_id.clone();
        run.params.self_heal_of = launch.self_heal_of;
        // P5: WHICH HUMAN DISPATCH is this attempt part of. Two sources, never both: a launch that
        // already belongs to a generation carries its number (`DispatchEpoch::Of`); anything else
        // is a new dispatch and is numbered here, from the cluster, BEFORE the row is written — so
        // an attempt's generation is decided once and is visible on the row to every later reader
        // (the completion hook, the report, an operator reading `params`).
        run.params.dispatch_epoch = match launch.dispatch_epoch {
            DispatchEpoch::Of(epoch) => epoch,
            DispatchEpoch::New => Some(self.next_dispatch_epoch(task.id).await?),
        };
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
        // Resolved OUTSIDE the spawned task: the supervisor is a 'static task,
        // so it cannot borrow self. One Arc clone, captured by the closure.
        let knowledge = self.knowledge_handle();
        // The capability handle rides the same way (t8). The gate is evaluated
        // when the prompt is COMPOSED, not when the run was accepted: a run that
        // waited on its harness gate must see the configuration that is in force
        // when it actually starts.
        let capabilities = self.capabilities_handle();
        // The manager rides the supervisor (P2.3): the completion hook needs the registry (agent
        // id → NAME for the wake-up text) and `retry_run` (self-heal), and the supervisor is a
        // 'static task. Same reason `knowledge`/`capabilities` ride.
        let mgr_for_supervisor = Arc::clone(self);
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

            // `memory_inject_runs` gates the retrieval block of a run prompt
            // (design §6, §12): off means the injected string is empty and the
            // stores are not queried at all — the role identity and the retry
            // prefix below still ride, because they are the run's own context,
            // not evidence pulled from memory/knowledge/graph.
            // t129 (RV-INT INT-F6): the render now hands back its own budget
            // report, so the `context_injected` event can say what the budget DID
            // instead of `null`. `None` here means the render did not run at all
            // (gate off) — "not collected"; `Some(report)` means it ran, and an
            // all-zero report from it is a MEASUREMENT (contract §3.2: `null` and
            // all-zero are different claims).
            let (mut injection, injection_budget) = if gate_of(
                &capabilities,
                crate::capability::CapabilityId::MemoryInjectRuns,
                true,
            ) {
                let (text, report) = render_run_injection(&db, &task, knowledge.as_deref()).await;
                (text, Some(report))
            } else {
                (String::new(), None)
            };
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
                mgr_for_supervisor,
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
                injection_budget,
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

    /// The generation number for a NEW human dispatch of `task_id` (P5): how many NON-self-heal
    /// attempts the task already has, counted from zero.
    ///
    /// Counting attempts rather than reusing a stored counter keeps this a derivation instead of a
    /// second truth: the cluster's own rows are the ledger, and an attempt that is started but whose
    /// insert fails cannot leave a number behind. The cost is one read of a task's runs before each
    /// dispatch, which the completion hook already does on every attempt.
    ///
    /// A pre-versioning attempt (no `dispatch_epoch`) is counted here exactly like a versioned one —
    /// it WAS a dispatch somebody made. What it is not allowed to do is grant a heal budget: see
    /// [`heal_budget_spent`].
    async fn next_dispatch_epoch(&self, task_id: ruagent_core::TaskId) -> Result<u32> {
        let runs = self
            .db
            .list_runs_for_task(task_id)
            .await
            .with_context(|| format!("counting the dispatches of task {task_id}"))?;
        Ok(runs
            .iter()
            .filter(|r| r.params.self_heal_of.is_none())
            .count() as u32)
    }

    /// One-click retry of a dead run (design §8.3 crash row): a new
    /// run on the same task with the same agent and options, reusing
    /// the crashed attempt's workspace (half-written work survives),
    /// and a context snapshot from its transcript so the agent knows
    /// where it died instead of starting from zero.
    ///
    /// THE HUMAN/PANEL/HTTP ENTRANCE. The completion hook's self-heal goes through
    /// [`Self::retry_run_inner`] with the two P2.3 arguments, so both paths create their new
    /// attempt in exactly one place.
    pub async fn retry_run(self: &Arc<Self>, run_id: RunId) -> Result<Run> {
        self.retry_run_inner(run_id, None, false).await
    }

    /// The one retry implementation.
    ///
    /// `agent_override` — run the new attempt on THIS agent instead of the dead one's (P2.3:
    /// self-heal prefers a different agent, because retrying the same agent that just died is
    /// often the same failure again).
    /// `self_heal` — this attempt IS the cluster's heal: it is recorded on the attempt
    /// (`RunParams::self_heal_of`) so the NEXT completion hook can see the budget is spent, and its
    /// `Routed` event says so in the existing run-event stream (P2.3's "leave a trace": no new
    /// event type, no second log, and a reader of the timeline can see the daemon tried).
    ///
    /// RETURNED AS AN EXPLICITLY BOXED `Pin<Box<dyn Future + Send>>`, not as an `async fn`.
    /// `start_run` → `supervise` → [`finish_cluster`] → here → `start_run` is a genuine recursion (a
    /// self-heal launches a new attempt, whose own hook runs when it ends), and with opaque
    /// `async fn` types the compiler cannot PROVE the outer supervisor task is `Send` — it reports
    /// the peer's spawn as unsendable instead. One box, at the definition, cuts the cycle in the
    /// type system with no change to what happens at runtime.
    fn retry_run_inner<'a>(
        self: &'a Arc<Self>,
        run_id: RunId,
        agent_override: Option<&'a str>,
        self_heal: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Run>> + Send + 'a>> {
        Box::pin(async move {
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
            let card = match agent_override {
                Some(name) => self
                    .agent(name)
                    .with_context(|| format!("self-heal agent `{name}` is not registered"))?,
                None => self
                    .agents()
                    .into_iter()
                    .find(|c| c.id == old.params.agent)
                    .with_context(|| "the retried run's agent is no longer registered")?,
            };

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
                    rationale: Some(if self_heal {
                        format!(
                            "self-heal (the cluster's one automatic retry) of failed run {run_id}, \
                             on a different agent"
                        )
                    } else {
                        format!("retry of run {run_id}")
                    }),
                }),
                options: old.params.options.clone(),
                original_prompt: Some(original),
                context_prefix,
                // P2.1: a heal retry answers to the SAME conversation — the chat that asked for the
                // work asked for the work, not for one attempt at it.
                origin_chat_id: old.origin_chat_id.clone(),
                self_heal_of: self_heal.then_some(run_id),
                // P5: a HEAL belongs to the generation it heals — it is the same human dispatch
                // trying again, and giving it a number of its own would let its failure heal
                // itself again, for ever. A HUMAN retry (the one-click button) is a new dispatch by
                // definition and is numbered from the cluster like any other.
                dispatch_epoch: if self_heal {
                    DispatchEpoch::Of(old.params.dispatch_epoch)
                } else {
                    DispatchEpoch::New
                },
            },
        )
        .await
        })
    }

    /// Fan out: same prompt to N agents in parallel on one task
    /// (design §5.2). Every run records its routing provenance.
    pub async fn start_fanout(
        self: &Arc<Self>,
        task: &Task,
        agents: &[String],
        prompt: String,
        repo: Option<PathBuf>,
        options: std::collections::BTreeMap<String, String>,
        // WHICH CONVERSATION asked for this (issue: one front door). Every branch of the fan-out
        // carries the SAME value, which is what makes the cluster report once instead of once per
        // branch: the completion hook aggregates by task, and all branches share this task.
        origin_chat_id: Option<String>,
    ) -> Result<Vec<Run>> {
        let topology = Topology::FanOut {
            agents: agents.to_vec(),
        };
        let plan = ruagent_orchestrator::plan(task, &topology)?;
        // P5: ONE dispatch, ONE generation. The branches are members of a single human ask, so the
        // number is computed here and handed to all of them; letting each branch count itself
        // would give the Nth branch a generation of its own, i.e. one heal budget per branch.
        let dispatch_epoch = DispatchEpoch::Of(Some(self.next_dispatch_epoch(task.id).await?));
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
                        origin_chat_id: origin_chat_id.clone(),
                        dispatch_epoch,
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
        // The conversation that asked for the pipeline. Each step is its own sub-task, so the
        // completion hook reports per step (a pipeline is sequential by definition); what this
        // value guarantees is that the reports reach THAT conversation instead of nobody.
        origin_chat_id: Option<String>,
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
                            origin_chat_id: origin_chat_id.clone(),
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
        // The conversation that asked for the judgement (same reason as start_fanout).
        origin_chat_id: Option<String>,
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
                    origin_chat_id: origin_chat_id.clone(),
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
/// The `routed` event must name the agent that RAN, not merely the id the router handed over.
///
/// MEASURED, from a real run on this machine (the live `run-01a0f641-d8e5-7566-8247-23b7068f357d.jsonl`,
/// its first event): `routed` carried `agents:["01a0f580-b2ac-7048-a136-6bbe33d5f13b"]`, an id
/// that appears in no registry the API serves and in no table of the database, while the very
/// same run's `params.agent` was `01a0b915-8273-73c4-8fe9-0e35a34d4ded` = `approver` -- the card
/// the run resolved by name, and the agent whose prompt the `context_injected` event rendered in
/// the next line. So the run was right and the audit trail named something no reader can resolve.
///
/// WHERE THE FOREIGN ID COMES FROM (out of this file's scope, recorded because a reader of this
/// function needs it): the API's no-cascade fallback records `state.config.default_agent().id`
/// (`api.rs:2143` -> `api.rs:2148`) and then re-resolves the run BY NAME in the manager's registry
/// (`api.rs:2152`-`2155`), which is where `params.agent` comes from. The config-file view mints a
/// fresh `AgentId::generate()` per card (`config.rs:200`, `225`, `265`, `307` -- the last one's own
/// comment says "replaced by the stable DB id"), so the two ids are two VIEWS of one agent and only
/// one of them resolves. The cascade path is not affected: its ids are resolved through the
/// manager's registry before `orchestrator::route` ever sees them (`api.rs:5224`).
///
/// WHY RECONCILE HERE RATHER THAN TRUST THE DECISION: this event's job is to say what happened, and
/// the run's own agent is the ground truth the event describes. An upstream id can be wrong for
/// reasons this file cannot see -- here, a view mismatch in another module -- and a reader
/// reconstructing a finished run should not have to cross-check `params.agent` to learn who ran.
/// The router's own identity is NOT discarded: it moves into the rationale, so nothing is silently
/// dropped and the divergence itself stays visible.
fn routed_naming_what_ran(
    decision: RoutingDecision,
    ran: ruagent_core::AgentId,
) -> RoutingDecision {
    if decision.primary() == ran {
        return decision;
    }
    let named = decision.primary();
    let mut rationale = decision.rationale.unwrap_or_default();
    if !rationale.is_empty() {
        rationale.push_str("; ");
    }
    rationale.push_str(&format!(
        "router named {named} (not the registry id of the agent that ran)"
    ));
    RoutingDecision {
        agents: vec![ran],
        source: decision.source,
        rationale: Some(rationale),
    }
}

/// How long a run looks for its own new child before giving up on attribution
/// (issue #60). It only delays a BACKGROUND task; the run never waits on it.
const RUN_CHILD_WINDOW: std::time::Duration = std::time::Duration::from_secs(3);

/// The poll cadence the wait-for-a-just-spawned-child loops share (t24, F7): start at
/// CHILD_POLL_FIRST_MS and double up to CHILD_POLL_MAX_MS.
///
/// WHY ONE PAIR, IN ONE PLACE. Each attribution pass costs a machine-wide process snapshot, and a
/// flat 25 ms over the whole 3 s window paid for up to 120 of them per pass ON the async runtime --
/// hence the doubling. The chat path (crates/daemon/src/chat.rs) and the run path (here) carried
/// the SAME two numbers, TO THE DIGIT, which is the strongest evidence that they are one fact and
/// not two independent choices: t24 measured that they must move together, and a single path
/// changed alone fails NO test -- it only shows up as an occasional timeout on a loaded machine.
/// So the numbers live here, once, and chat.rs imports them under its own names.
///
/// WHY THE LOOPS ARE NOT EXTRACTED. The three wait-until loops in this crate exit on three
/// different conditions (return when the child is ATTRIBUTED, return when it is GONE, return at the
/// DEADLINE), so sharing the loop would mean passing a closure: six lines traded for a layer of
/// indirection (F7's own advice). What IS shared is the cadence -- these two numbers.
pub(crate) const CHILD_POLL_FIRST_MS: u64 = 20;
pub(crate) const CHILD_POLL_MAX_MS: u64 = 250;

/// Reclaim a run's agent child, with the reading the chat path reports too
/// (issue #60): how many it had to kill, how many were already gone, and how
/// many SURVIVED. Blocking work goes to a blocking thread; a caller spawns this
/// rather than awaiting it, so a run's exit is never delayed by a teardown.
async fn reclaim_run_child(
    child: Arc<Mutex<Option<crate::orphans::ChildRef>>>,
    run_id: String,
    reason: &'static str,
) {
    let child = child.lock().expect("run child lock").take();
    let Some(child) = child else {
        tracing::debug!(
            run = %run_id,
            reason,
            "run ended with no attributed child to reclaim"
        );
        return;
    };
    let what = format!("run {run_id}: child after {reason}");
    let label = what.clone();
    let pid = child.pid;
    // IDENTITY-SAFE (t24): a pid that has been handed to another process in the meantime
    // is not ours to kill, and reclaim_refs is what refuses to.
    let out = tokio::task::spawn_blocking(move || crate::orphans::reclaim_refs(&[child], &label))
        .await
        .unwrap_or_default();
    tracing::info!(
        run = %run_id,
        reason,
        pid,
        killed = out.killed,
        already_gone = out.already_gone,
        failed = out.failed,
        "run child reclamation"
    );
}

/// transcript, applies the permission policy, and finalizes state.
///
/// `mgr` rides along because the completion hook needs the registry (agent id → NAME for the
/// wake-up text) and `retry_run` (the P2.3 self-heal). `db`/`root` stay their own parameters: they
/// are what this function's other twenty-odd uses already read.
#[allow(clippy::too_many_arguments)]
async fn supervise(
    mgr: Arc<RunManager>,
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
    // t129 (RV-INT INT-F6): the budget report of the render that produced
    // `injection`, when that render RAN. The retry-prefix / role branches can
    // make `injection` non-empty without any render having happened, and that
    // case stays `None` (= "not collected"), never an all-zero report.
    injection_budget: Option<ruagent_memory::inject::BudgetReport>,
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
        // t37: the run's own agent is the ground truth this event describes; see
        // `routed_naming_what_ran` for the measured divergence and why the reconciliation
        // belongs at the emission site rather than in the decision's producer.
        let ran = run.params.agent;
        emit(
            &mut transcript,
            &broadcast,
            &run,
            RunEvent::Routed {
                decision: routed_naming_what_ran(decision, ran),
            },
        );
    }
    if !injection.is_empty() {
        // t129 (RV-INT INT-F6): the budget the render MEASURED. A serialisation
        // failure is a real error (`?`), never a silent `None` — the absorber
        // shape this repo's own audits name.
        let budget = match &injection_budget {
            Some(report) => Some(
                serde_json::to_value(report)
                    .with_context(|| "serialising the injection budget report")?,
            ),
            None => None,
        };
        emit(
            &mut transcript,
            &broadcast,
            &run,
            RunEvent::ContextInjected {
                render: injection.clone(),
                // DEP-INT-1 (t19): the producer's own label.
                path: Some("run".to_string()),
                budget,
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

    // P6 of `docs/plans/2026-11-23-reports-never-lost.md`: the conversation this run answers to
    // rides the AGENT PROCESS's environment, because the agent cannot otherwise know it. The
    // platform MCP server is spawned BY the agent for its session, so it inherits this — that is
    // where `task_dispatch` reads it (`crates/mcp/src/lib.rs`), and that is what makes a piece of
    // work dispatched from inside a chat carry the chat's id instead of `null`.
    //
    // ABSENT, NOT EMPTY, when the run has no origin (the panel, the CLI, an external client): an
    // empty value would make "nobody is waiting" indistinguishable from "a conversation with an
    // empty id", and `task_dispatch` deliberately does not invent an origin.
    //
    // What is NOT done here, and cannot be: removing a variable the daemon itself inherited. If the
    // daemon was started BY an agent process of some other platform, `RUAGENT_CHAT_ID` would already
    // be in its environment and would reach this child too. `AcpAgentConfig` adds variables, it does
    // not clear the environment (clearing it would take PATH, the system proxy and `RUAGENT_URL`
    // with it), so that leak is left visible rather than papered over.
    let mut agent_env: Vec<(String, String)> = Vec::new();
    if let Some(chat_id) = run
        .origin_chat_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        agent_env.push((ruagent_core::CHAT_ID_ENV.to_string(), chat_id.to_string()));
    }

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
        env: agent_env,
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
    // ISSUE #60, attribution -- ONE decision, shared with the chat path (t24):
    // orphans::attribute_child takes the window, the instant it was taken and the card's
    // program, and claims a pid only when the evidence identifies THIS run's child (the
    // program's image, unique among our live children). A run whose child cannot be told
    // apart is LEFT unattributed on purpose: reclaiming a process that might belong to
    // another live session is worse than not knowing a pid. The pass is blocking work, so
    // it goes to a blocking thread, and the poll delay doubles instead of staying at 25 ms.
    let run_child: Arc<Mutex<Option<crate::orphans::ChildRef>>> = Arc::new(Mutex::new(None));
    {
        let child_window_at = crate::orphans::now_unix_ms();
        let child_window: Vec<u32> = tokio::task::spawn_blocking(|| {
            crate::orphans::own_children()
                .into_iter()
                .map(|c| c.pid)
                .collect()
        })
        .await
        .unwrap_or_default();
        let child_program = opts.program.clone();
        let slot = run_child.clone();
        tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + RUN_CHILD_WINDOW;
            let mut wait_ms = CHILD_POLL_FIRST_MS;
            loop {
                let window = child_window.clone();
                let program = child_program.clone();
                let found = tokio::task::spawn_blocking(move || {
                    crate::orphans::attribute_child(&window, child_window_at, &program)
                })
                .await
                .unwrap_or(None);
                if let Some(child) = found {
                    *slot.lock().expect("run child lock") = Some(child);
                    return;
                }
                if tokio::time::Instant::now() >= deadline {
                    tracing::debug!(
                        "no child could be attributed to this run; its exit will reclaim nothing"
                    );
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                wait_ms = (wait_ms * 2).min(CHILD_POLL_MAX_MS);
            }
        });
    }
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
                // ISSUE #60: the abort drops the driver future, which is
                // SUPPOSED to tear the child down (the SDK's ChildGuard) -- and
                // nothing used to check whether it did. Reclaim it, and report
                // the reading.
                tokio::spawn(reclaim_run_child(
                    run_child.clone(),
                    run.id.to_string(),
                    "the run was cancelled",
                ));
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

    // ISSUE #60: the run is terminal, so its child must be too -- and this is
    // the NORMAL exit, not the exotic one. The driver task has just finished, so
    // the SDK's ChildGuard has run; the reclaim VERIFIES that and kills what is
    // still alive, so "the agent exited with the run" is a reading rather than an
    // assumption. Spawned, not awaited: a teardown must never delay a run's
    // finalization.
    tokio::spawn(reclaim_run_child(
        run_child.clone(),
        run.id.to_string(),
        "the run ended",
    ));

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
    // P0: A TERMINAL NON-SUCCESS MUST SAY WHY. Cancelled and interrupted runs often reach here
    // with `error` empty, and the run row is the only "why" carrier the task view has
    // (`get_task` returns task + runs). Before this, the board showed a task that was merely
    // "in progress" with nothing to read anywhere.
    if run.status.is_terminal() && run.status != RunStatus::Completed && run.error.is_none() {
        run.error = Some(
            match run.status {
                RunStatus::Cancelled => "the run was cancelled before it produced an answer",
                RunStatus::Interrupted => "the run was interrupted (daemon restart or lost child)",
                _ => "the run ended without reporting a cause",
            }
            .to_string(),
        );
    }
    transcript.flush()?;
    db.update_run(&run).await?;
    // P0 + P2.3: the task write-back and the completion hook, in that order and behind ONE
    // judgement ("is the whole cluster terminal"). This replaces the old
    // `if run.status == Completed { update_task_status(Done) }`, which left a failed run's task
    // "in progress" for ever.
    // A LIVE ending may self-heal: the chat that dispatched this work still has a session, so a
    // retry's eventual report has somewhere to land.
    finish_cluster(&mgr, task_id, &run, true).await;
    let _ = broadcast.send(StreamMsg::Finished {
        run_id: run.id,
        status: run.status,
    });
    Ok(())
}

/// P0's TERMINAL MAPPING, exhaustive on purpose.
///
/// WHY A `match` OVER EVERY VARIANT rather than `if status == Completed`: that is exactly the
/// shape this replaces (`runs.rs`' finalize used to write `Done` only for `Completed`), so every
/// other terminal state silently meant "write nothing" and the board said 进行中 about a run that
/// was dead. With this mapping a NEW `RunStatus` variant fails to compile here instead of becoming
/// a silent fourth truth.
///
/// `WaitingPermission` IS NON-TERMINAL, deliberately. It is an empty state today — nothing in the
/// tree ever writes it (the inbox's "waiting for permission" is joined in at read time from the
/// in-memory pending map) — and the predicate must not depend on that staying true: an attempt
/// parked on a human decision has NOT finished, and calling it terminal would make a cluster
/// announce itself while one of its attempts is still alive.
fn task_status_for(status: RunStatus) -> Option<TaskStatus> {
    match status {
        RunStatus::Queued
        | RunStatus::Spawning
        | RunStatus::Running
        | RunStatus::WaitingPermission => None,
        RunStatus::Completed => Some(TaskStatus::Done),
        RunStatus::Failed | RunStatus::Interrupted | RunStatus::Cancelled => {
            Some(TaskStatus::Pending)
        }
    }
}

/// P0 + P2.3, at ONE site, behind ONE judgement.
///
/// A CLUSTER is every run of one `task_id` (design §2): a fan-out, a retry and a re-dispatch of
/// one work item are the same cluster, because they are the same thing being attempted. The single
/// criterion below — "is every attempt terminal?" — answers both questions the design asks:
///
/// 1. **P0, the board must not lie.** While an attempt is live the task stays `InProgress` (that
///    is what "in progress" means); once they are ALL terminal the task is written back
///    exhaustively: `Completed` → `Done`, and `Failed`/`Interrupted`/`Cancelled` → `Pending` (the
///    work item is alive again and can be re-dispatched) with the failing attempt's own `error`
///    left as the "why it did not work last time" reading (`update_run` above, and the sentence
///    filled in when a cancellation reported nothing).
/// 2. **P2.3, the conversation that dispatched it is told — once.** An intermediate result is the
///    normal shape of a fan-out, so it is not news and costs no model turn. Storm suppression is
///    therefore the aggregation itself: there is NO timer and NO minimum interval here, and adding
///    one would be a second criterion for the same question (two criteria, two truths).
///
/// A failure makes one attempt at SELF-HEAL (the design's 「出问题优先自己解决」): ONE retry of the
/// newest retryable failure, preferring a DIFFERENT agent, recorded on the attempt itself
/// (`RunParams::self_heal_of` + a `Routed` event whose rationale says so) so the next hook sees the
/// budget is spent. Only when that budget is gone is the failure reported — and the report says how
/// many attempts were made and what the last failure was.
///
/// THE BUDGET IS PER HUMAN DISPATCH (P5 of `docs/plans/2026-11-23-reports-never-lost.md`, see
/// [`heal_budget_spent`]): the retry heals the generation that is ending (`params.dispatch_epoch`),
/// so a task that was re-dispatched after a failure gets a fresh automatic retry instead of being
/// told — wrongly — that it already had one. A cluster can therefore hold one heal PER GENERATION,
/// which is why the report counts the heals instead of assuming there was one.
///
/// KNOWN WINDOW, not a second truth: two attempts of one cluster that reach terminal within the
/// same read can both see "all terminal" and both report. Nothing here tries to hide that; the
/// consequence is a duplicate sentence in the chat, and the alternative (a claim table / a timer)
/// would be the second criterion the design forbids.
pub(crate) async fn finish_cluster(
    mgr: &Arc<RunManager>,
    task_id: ruagent_core::TaskId,
    run: &Run,
    // Whether this call may start the cluster's one automatic retry (the self-heal). It is a
    // PARAMETER and not a second criterion: the decision "is the cluster terminal" stays in one
    // place below; this only says whether the caller is entitled to spend tokens.
    //
    // WHO PASSES FALSE: the boot-time sweep in `lib.rs`. Measured (verifier, 2026-11-20): after a
    // restart the self-heal DOES start a fresh run, and the report it would eventually send is
    // DROPPED, because the chat has no live session any more (`WARN platform event dropped`). So a
    // restart spent a round of the user's tokens to produce a sentence nobody received. Paying and
    // not being told is the worst of the available shapes; the state write-back still happens, and
    // the reason the work died is already visible on the run's `error`
    // (`daemon restarted mid-run`).
    allow_self_heal: bool,
) {
    let runs = match mgr.db().list_runs_for_task(task_id).await {
        Ok(runs) => runs,
        Err(e) => {
            tracing::warn!(
                task_id = %task_id,
                run_id = %run.id,
                error = %e,
                "completion hook could not read the cluster: the task status was NOT written back \
                 and nothing was reported"
            );
            return;
        }
    };

    // (1) INTERMEDIATE RESULT. Some attempt of this cluster is still live: the task genuinely IS
    // in progress, and there is nothing to report yet.
    if let Some(live) = runs.iter().find(|r| !r.status.is_terminal()) {
        tracing::debug!(
            task_id = %task_id,
            run_id = %run.id,
            live_run = %live.id,
            live_status = ?live.status,
            "cluster still has a live attempt; no write-back, no report"
        );
        return;
    }

    // (2) P0: the task write-back, now that every attempt has ended.
    let task = match mgr.db().get_task(task_id).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            tracing::warn!(
                task_id = %task_id,
                run_id = %run.id,
                "completion hook found no task row for the cluster; nothing written back"
            );
            return;
        }
        Err(e) => {
            tracing::warn!(
                task_id = %task_id,
                run_id = %run.id,
                error = %e,
                "completion hook could not read the task; nothing written back"
            );
            return;
        }
    };
    if task.status == TaskStatus::Cancelled {
        // A HUMAN decided this work item is off. An automatic write-back must not resurrect it
        // into Pending (which is re-dispatchable), and there is nobody to wake about a cancelled
        // task. Said out loud instead of silently skipped.
        tracing::info!(
            task_id = %task_id,
            run_id = %run.id,
            "the task was cancelled by a human; the run's terminal state is NOT written back \
             onto it and no report is sent"
        );
        return;
    }
    // Every attempt is terminal (1), so every attempt has a target (the mapping is exhaustive);
    // a length mismatch would mean an unmapped status, which is said out loud rather than
    // guessed at.
    let targets: Vec<TaskStatus> = runs
        .iter()
        .filter_map(|r| task_status_for(r.status))
        .collect();
    if targets.len() != runs.len() {
        tracing::warn!(
            task_id = %task_id,
            run_id = %run.id,
            attempts = runs.len(),
            mapped = targets.len(),
            "a terminal run status has no task mapping; the task status was left alone"
        );
        return;
    }
    let target = if targets.iter().all(|t| *t == TaskStatus::Done) {
        TaskStatus::Done
    } else {
        TaskStatus::Pending
    };
    if task.status != target {
        match mgr.db().update_task_status(task_id, target).await {
            Ok(()) => tracing::info!(
                task_id = %task_id,
                run_id = %run.id,
                from = ?task.status,
                to = ?target,
                attempts = runs.len(),
                "task status written back from the run's terminal state"
            ),
            Err(e) => tracing::warn!(
                task_id = %task_id,
                run_id = %run.id,
                target = ?target,
                error = %e,
                "WRITING THE RUN'S TERMINAL STATE BACK TO THE TASK FAILED: the board keeps \
                 showing the old status"
            ),
        }
    }

    // (3) P2.3: is anybody waiting to hear about it? No origin chat = the panel/CLI/an external
    // client dispatched this, and that caller is already looking at the answer.
    //
    // THE ORIGIN IS READ FROM THE ATTEMPT THAT ENDED THE CLUSTER, not from whichever member
    // happens to carry one (`runs` is created_at ASC, so that is the LAST row). The difference is
    // real: a task first dispatched from a chat and later re-dispatched from the board has one
    // attributed and one unattributed attempt, and the panel's dispatch must not wake a
    // conversation the user did not write in — that is P2.1's rule, applied at the cluster level.
    // A self-heal retry INHERITS its dead attempt's origin, so it keeps answering to the same chat.
    let Some(chat_id) = runs.last().and_then(|r| r.origin_chat_id.clone()) else {
        return;
    };
    let cards = mgr.agents();
    let failed: Vec<&Run> = runs
        .iter()
        .filter(|r| r.status != RunStatus::Completed)
        .collect();

    if failed.is_empty() {
        let text = cluster_success_report(&task, &runs, &cards);
        wake_in_background(mgr, &chat_id, text, run);
        return;
    }

    // (4) SELF-HEAL, one automatic retry per HUMAN DISPATCH GENERATION (P5 of
    // `docs/plans/2026-11-23-reports-never-lost.md`).
    //
    // The budget used to be counted per CLUSTER ("does any attempt of this task carry
    // `self_heal_of`?"), which stays true for ever after the first heal — so a task a human
    // re-dispatched a second and third time got no automatic retry at all, exactly when the
    // re-dispatch was the sign they wanted it done. The question is now asked per GENERATION: the
    // heal of the dispatch that is ending, and nothing earlier.
    let healed_already = heal_budget_spent(&runs, run);
    if allow_self_heal
        && !healed_already
        && let Some(dead) = failed.iter().rev().find(|r| r.status.is_retryable())
    {
        let alternative = different_agent(&cards, dead.params.agent);
        if alternative.is_none() {
            // P2.4: say it instead of pretending a swap happened.
            tracing::warn!(
                task_id = %task_id,
                run_id = %dead.id,
                agent = %agent_name(&cards, dead.params.agent),
                "self-heal: the registry has no OTHER enabled agent, so the retry stays on the \
                 same one (the report to the chat will say so)"
            );
        }
        // BOXED BY THE FUNCTION ITSELF (`retry_run_inner` returns a boxed future): the async
        // recursion start_run → supervise → finish_cluster → here → start_run cannot otherwise be
        // proven `Send`. See that function's doc comment.
        let heal_attempt = mgr.retry_run_inner(dead.id, alternative.as_deref(), true);
        match heal_attempt.await {
            Ok(heal) => {
                tracing::warn!(
                    task_id = %task_id,
                    run_id = %run.id,
                    healed_run = %dead.id,
                    heal_run = %heal.id,
                    dispatch_epoch = ?heal.params.dispatch_epoch,
                    from_agent = %agent_name(&cards, dead.params.agent),
                    to_agent = %agent_name(&cards, heal.params.agent),
                    "self-heal: started this dispatch's one automatic retry; the report waits for it"
                );
                // The heal attempt is live again, so this cluster is NOT finished: it reports when
                // it ends, through this same hook.
                return;
            }
            Err(e) => {
                // The retry could not be started. Falling through is deliberate: the cluster IS
                // terminal, and a failure that cannot be retried must still be reported.
                tracing::warn!(
                    task_id = %task_id,
                    run_id = %dead.id,
                    error = %e,
                    "self-heal: the automatic retry could NOT be started; reporting the failure as \
                     it stands"
                );
            }
        }
    }

    // (5) Report the failure, saying what was tried.
    let text = cluster_failure_report(&task, &runs, &cards, healed_already);
    wake_in_background(mgr, &chat_id, text, run);
}

/// Whether the generation that is ENDING has already spent its one automatic retry (P5 of
/// `docs/plans/2026-11-23-reports-never-lost.md`).
///
/// The budget belongs to a HUMAN DISPATCH, not to the work item: one automatic retry per dispatch
/// (`RunParams::dispatch_epoch`), so
///
/// * generation 0 fails → one heal; that heal fails → reported (its own row is generation 0 too);
/// * a human re-dispatches → generation 1, which has a budget again;
/// * within ONE generation the heal's own row blocks a second heal, which is what keeps this from
///   degrading into "heal for ever".
///
/// THE COMPATIBILITY RULE LIVES HERE: a heal attempt written before `dispatch_epoch` existed has
/// `None` and cannot be attributed to a generation, so `is_none_or` below counts it as this one — an
/// old cluster behaves exactly as it did before P5 and gets NO extra budget. The same is true of an ending attempt with no epoch (a run that was already live when the daemon was
/// upgraded): nothing can be attributed, so nothing is spent twice. Both directions are the
/// conservative one, because the expensive way to be wrong here is a silent extra round of tokens.
fn heal_budget_spent(runs: &[Run], ending: &Run) -> bool {
    // The attempt that is ending the cluster names the generation. Without one (a row written
    // before the field existed) nothing can be attributed, so ANY heal in the cluster counts.
    let Some(current) = ending.params.dispatch_epoch else {
        return runs.iter().any(|r| r.params.self_heal_of.is_some());
    };
    runs.iter().any(|r| {
        r.params.self_heal_of.is_some()
            // `is_none_or(..)`: an un-versioned heal counts as this generation.
            && r.params.dispatch_epoch.is_none_or(|heal| heal == current)
    })
}

/// The id→NAME direction, for text a human reads. A run records `params.agent` (a registry id);
/// every sentence in the report names the agent the way the user knows it — the same contract
/// `task_dispatch`'s reply already follows. Missing from the registry is reported, not hidden.
fn agent_name(cards: &[AgentCard], id: ruagent_core::AgentId) -> String {
    cards
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| format!("{id} (no card with this id is in the registry any more)"))
}

/// The agent a self-heal should move to: an ENABLED card that is not the one that just failed,
/// or `None` when there is no other (P2.3: 「注册表里有可用者才换」). `RunManager::agents()` is
/// name-sorted, so this is deterministic rather than "whichever the map yielded first".
fn different_agent(cards: &[AgentCard], failed: ruagent_core::AgentId) -> Option<String> {
    cards
        .iter()
        .find(|c| c.enabled && c.id != failed)
        .map(|c| c.name.clone())
}

/// One attempt's line in a report: who, how it ended, and what to read.
fn report_member(cards: &[AgentCard], run: &Run) -> String {
    let name = agent_name(cards, run.params.agent);
    let healer = if run.params.self_heal_of.is_some() {
        "（自愈重试）"
    } else {
        ""
    };
    match run.status {
        RunStatus::Completed => {
            let what = run
                .result
                .as_deref()
                .map(|r| snippet(r, 300))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "（没有输出）".to_string());
            format!("  · {name}{healer} — 成功：{what}")
        }
        _ => {
            let why = run
                .error
                .as_deref()
                .map(|e| snippet(e, 300))
                .unwrap_or_else(|| "（没有记下原因）".to_string());
            format!("  · {name}{healer} — {:?}：{why}", run.status)
        }
    }
}

/// The all-success report: which agents, which work item, and the result to read.
fn cluster_success_report(task: &Task, runs: &[Run], cards: &[AgentCard]) -> String {
    let mut out = format!(
        "【平台事件】你派出去的活全部完成了（这是平台自动汇报，不是用户说的话）。\n\n任务：{}\n",
        task.title
    );
    for run in runs {
        out.push_str(&report_member(cards, run));
        out.push('\n');
    }
    if runs.len() > 1 {
        out.push_str(&format!(
            "\n（同一个任务的 {} 个尝试全部结束。）",
            runs.len()
        ));
    }
    out
}

/// The failure report. P2.3 requires it to say HOW MANY attempts were made and what the LAST
/// failure was — "我试了 N 次，最后失败原因是 …" — and P2.4 requires it to admit when there was no
/// other agent to move to instead of implying a swap that never happened.
fn cluster_failure_report(
    task: &Task,
    runs: &[Run],
    cards: &[AgentCard],
    healed_already: bool,
) -> String {
    let last_error = runs
        .iter()
        .rev()
        .find(|r| r.status != RunStatus::Completed)
        .and_then(|r| r.error.as_deref())
        .map(|e| snippet(e, 400))
        .unwrap_or_else(|| "（没有记下原因）".to_string());
    let mut out = format!(
        "【平台事件】你派出去的活没能做成（这是平台自动汇报，不是用户说的话）。\n\n任务：{}\n",
        task.title
    );
    for run in runs {
        out.push_str(&report_member(cards, run));
        out.push('\n');
    }
    // HOW MANY HEALS, read off the attempts rather than inferred from `healed_already`: since P5 a
    // cluster can hold one heal per dispatch generation (a task re-dispatched twice and failed twice
    // has two), and "含 1 次自愈重试" would then be a false sentence in a report whose whole job is
    // to say what was tried.
    let heals = runs
        .iter()
        .filter(|r| r.params.self_heal_of.is_some())
        .count();
    out.push_str(&format!(
        "\n我试了 {} 次{}，最后一次失败原因是：{last_error}\n",
        runs.len(),
        match heals {
            0 => String::new(),
            1 => "（含 1 次自愈重试）".to_string(),
            n => format!("（含 {n} 次自愈重试）"),
        }
    ));
    // Did the heal actually MOVE to another agent? Read it off the attempts themselves: the
    // registry may have had nobody else, and the honest sentence is then "同一个 agent".
    if healed_already {
        let healed: Vec<&Run> = runs
            .iter()
            .filter(|r| r.params.self_heal_of.is_some())
            .collect();
        let stayed = healed.iter().any(|heal| {
            runs.iter().any(|r| {
                Some(r.id) == heal.params.self_heal_of && r.params.agent == heal.params.agent
            })
        });
        if stayed {
            out.push_str(
                "（自愈时注册表里只有一个可用 agent，所以是用同一个 agent 重试的，不是换了一个。）\n",
            );
        }
    }
    out
}

/// Bound a multi-line agent output to one readable line.
fn snippet(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        flat
    } else {
        let cut: String = flat.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}

/// Hand the report to the chat — in a spawned task, so a slow chat can never delay this run's
/// `Finished` broadcast, and P2.4's rule holds: a delivery failure is a WARN naming the chat and
/// the run, never a panic and never a silent drop.
fn wake_in_background(mgr: &Arc<RunManager>, chat_id: &str, text: String, run: &Run) {
    let root = mgr.root();
    let chat_id = chat_id.to_string();
    let run_id = run.id;
    tokio::spawn(async move {
        if let Err(e) = crate::chat::notify_chat(&root, &chat_id, text).await {
            tracing::warn!(
                chat_id = %chat_id,
                run_id = %run_id,
                error = %e,
                "the completion report could NOT be delivered to the chat; the run itself is \
                 unaffected (this is a lost report, not a lost run)"
            );
        }
    });
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
        // The marker comes from the contract's vocabulary (t309): this site
        // used to render a lookalike with different bytes, and nothing
        // compared the two.
        format!(
            "{cut}\n{}",
            ruagent_memory::inject::cut_at("upstream result", MAX_UPSTREAM_CHARS)
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
/// The last N characters of a text, with the contract truncation marker in
/// front when anything was dropped.
///
/// WHY A HELPER (t309): this used to inline its own marker while the contract
/// rendered a different one. One concept, two byte sequences, and no test
/// could see it because each site only asserted its own text. Now the marker
/// cannot be written here without the vocabulary.
fn bounded_tail(text: &str, bound: usize) -> String {
    let total = text.chars().count();
    if total <= bound {
        return text.to_string();
    }
    let skipped = total - bound;
    let kept: String = text.chars().skip(skipped).collect();
    format!("{} {kept}", ruagent_memory::inject::tail_truncated(skipped))
}
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
    let tail = bounded_tail(&text, BOUND);
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

/// The run's retrieval query: a run has no user message -- the task IS the ask.
/// Capped so an enormous intent cannot dominate the embedding.
fn run_query(task: &Task) -> String {
    let mut q = format!("{} {}", task.title, task.intent);
    if q.chars().count() > 500 {
        q = q.chars().take(500).collect();
    }
    q
}

/// Search wider than the block keeps, so a wiki page cannot crowd a source out
/// before the contract's selection rule sees it.
const RUN_KNOWLEDGE_SEARCH_N: u32 = 12;

/// Assemble the push-path injection for a run (design SS6.4): user profile +
/// user observations + the project's observations + what the knowledge base
/// says about the task, all through the ONE injection contract
/// (crates/memory/src/inject.rs).
///
/// WHY THE KNOWLEDGE LEG IS HERE AT ALL (t260): recall returned knowledge hits
/// in 99.3% of queries and wiki hits in 100%, and NOT ONE of the 79
/// context_injected events in the transcripts carried a knowledge block -- the
/// retrieval was built and nothing injected it. This is the injection half.
///
/// The capability gate for one id, given the shared handle (t8).
///
/// NO HANDLE = LEGACY: a manager that was never given the process's one plane
/// answers with what today's code would do, which is law L1 and the reason the
/// handle is optional. This is the only place in this module that turns the
/// handle into a decision, so the "read it, drop the guard" discipline lives in
/// one line rather than at every call site.
fn gate_of(
    handle: &Option<Arc<std::sync::RwLock<crate::capability::CapabilityPlane>>>,
    id: crate::capability::CapabilityId,
    legacy: bool,
) -> bool {
    match handle {
        Some(h) => h.read().expect("capability plane").gate(id, legacy),
        None => legacy,
    }
}

/// The knowledge argument is None when the daemon never handed the manager a
/// handle (tests, or a boot with no knowledge base). Then the render has no
/// knowledge block, which is honest -- the alternative would be a second
/// embedder decision, i.e. a second source of truth for what the vectors mean.
async fn render_run_injection(
    db: &Db,
    task: &Task,
    knowledge: Option<&ruagent_knowledge::Knowledge>,
) -> (String, ruagent_memory::inject::BudgetReport) {
    use ruagent_memory::inject::{
        ContextItem, InjectionBudget, KNOWLEDGE_SOURCES, WIKI_PAGES, render_context_report,
    };

    let mut items: Vec<ContextItem> = Vec::new();
    // WHICH memories, and by what rule: the runs preset, executed by the SAME
    // rule function the chat path uses (t278). This path has no query leg --
    // see RUNS_SELECTION's own doc for why that difference is measured rather
    // than closed -- so it passes no embedder.
    for m in crate::memembed::select_injection_memories(
        db,
        None,
        &run_query(task),
        task.project.as_deref(),
        &ruagent_memory::inject::RUNS_SELECTION,
    )
    .await
    {
        items.push(ContextItem::dated(m.tag, m.content, &m.updated_at));
    }

    // Drop order (rationale with the tags in crates/memory/src/inject.rs):
    // user_profile > relevant_memories > knowledge > wiki > project_context.
    // Knowledge and wiki therefore sit BEFORE the project block -- evidence and
    // leads outlive one project's notes -- and the total budget drops the LAST
    // blocks first, visibly.
    if let Some(k) = knowledge
        && let Ok(page) = k
            .search_page(&run_query(task), RUN_KNOWLEDGE_SEARCH_N)
            .await
    {
        // t19 (DEP-INT-8 / R-A H-1..H-4 / R-D D.7): same enriched shape as the
        // chat path -- one contract, two producers. Wiki hits carry a lead from
        // the ASYNC `lead_for` (the only entry point that can report a RECORDED
        // coverage; `lead_from` would make `coverage=` permanently unknown).
        let mut enriched: Vec<ruagent_memory::inject::EnrichedHit> = Vec::new();
        for r in &page.hits {
            let relevance = r.relevance.as_ref().map(|v| {
                crate::memembed::relevance_meta(
                    v.value,
                    v.kind.as_str(),
                    v.version,
                    Some(v.query_background),
                )
            });
            let lead = if r.hit.document.starts_with("wiki/") {
                let slug = r
                    .hit
                    .document
                    .trim_start_matches("wiki/")
                    .trim_end_matches(".md");
                let record = crate::wiki::page_record(db, slug).await;
                crate::wiki::lead_for(k, &record, slug)
                    .await
                    .as_ref()
                    .map(crate::memembed::lead_meta)
            } else {
                None
            };
            enriched.push(crate::memembed::enriched_hit(&r.hit, relevance, lead));
        }
        items.extend(ruagent_memory::inject::knowledge_items_enriched(
            &enriched,
            KNOWLEDGE_SOURCES,
            WIKI_PAGES,
        ));
    }
    // G10 / F5 closure (t63): same producer, same shape as the chat path, inserted
    // before the sort for the same reason (the block order is owned by tag_rank).
    if let Ok(evidence) =
        ruagent_graph::retrieve(db, &ruagent_graph::GraphQuery::for_text(run_query(task))).await
    {
        items.extend(crate::memembed::graph_evidence_items(
            &evidence,
            ruagent_memory::inject::GRAPH_PATHS,
        ));
    }
    // The project group is NOT read here any more: it is part of
    // RUNS_SELECTION, so the preset is the only place that says this path
    // reads project:<task.project> at all. The knowledge block stays
    // between the two so the BLOCK order is unchanged (tag_rank sorts it
    // anyway, and that rank -- not this file -- owns the drop order).
    // The empty case is NOT special-cased any more (t129): it goes through the
    // same renderer as the non-empty one, so the report is produced by the code
    // that produced the bytes — an all-zero report from an empty item list is a
    // measurement, and hand-building a second "empty" report here would be the
    // second source this whole task exists to remove. (Measured by t60: the
    // empty fixture renders 0 chars, so the bytes are unchanged.)
    //
    // The BLOCK order is the contract drop order (see inject.rs::tag_rank),
    // not the order these items were discovered in.
    items.sort_by_key(|i| ruagent_memory::inject::tag_rank(i.tag));
    render_context_report(&items, &InjectionBudget::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// t129 (RV-INT INT-F6): the budget the `context_injected` event carries must
    /// come from the SAME call that produced the render bytes, and its numbers
    /// must be measurements. THE ASSERTION THAT FAILS AGAINST THE PRE-CHANGE
    /// CODE is the first one: before t129 the event site hard-coded
    /// `budget: None`, so a run whose render dropped a block could not report
    /// `dropped_items > 0` at all (V-INT t20/t21 measured `budget` 0/5).
    ///
    /// This pins the code path (`render_context_report` -> `serde_json::to_value`
    /// -> the event's field names), not a whole daemon run: a full run needs a
    /// mock-agent harness and a task whose corpus overflows the budget, which is
    /// the one reading this task could not take inside its inScope (see the
    /// report's "未覆盖什么").
    #[test]
    fn the_injection_budget_the_event_carries_is_measured_not_all_zero() {
        use ruagent_memory::inject::{ContextItem, InjectionBudget, render_context_report};
        let items = vec![
            ContextItem::dated("relevant_memories", "a".repeat(80), "2026-01-01T00:00:00Z"),
            ContextItem::dated("knowledge", "b".repeat(80), "2026-01-01T00:00:00Z"),
        ];
        // A total budget that cannot hold both blocks: the last one is dropped.
        let budget = InjectionBudget {
            per_block: 120,
            total: 120,
        };
        let (render, report) = render_context_report(&items, &budget);
        assert!(
            report.dropped_items > 0,
            "the fixture must drop a block, else this test proves nothing: {report:?}"
        );
        assert!(
            report.used_chars <= report.total_chars,
            "a report may not claim more used chars than the budget it measured: {report:?}"
        );
        // The exact bytes the event would carry (runs.rs's emit site).
        let json = serde_json::to_value(&report).expect("a budget report serialises");
        assert_eq!(
            json["dropped_items"],
            serde_json::json!(report.dropped_items),
            "the event's dropped_items must be the report's, not a default: {json}"
        );
        assert_eq!(json["used_chars"], serde_json::json!(report.used_chars));
        assert_eq!(json["total_chars"], serde_json::json!(report.total_chars));
        // THE POINT OF THE REPORT, MEASURED (t60's headline, re-taken here): with
        // the budget this tight the render holds ONE block and no
        // `<context_budget>` notice — the bytes alone do not say a block was
        // dropped, while the account does. So this test does NOT assert on the
        // text; asserting "the drop must be visible in the bytes" is what made
        // the first cut of this test FAIL (recorded in the t129 report): the
        // notice is emitted only when what remains can hold it, so that claim is
        // fixture-dependent, not contract.
        assert!(
            !render.contains("context_budget"),
            "this fixture has no room for the drop notice, so the TEXT cannot carry \
             the fact -- if this starts failing, the fixture changed, not the claim: {render:?}"
        );
        assert!(
            render.chars().count() <= budget.total,
            "the hard character bound holds: {render:?}"
        );
    }

    /// t37: the recorded `routed` event must name the agent whose prompt was actually injected.
    ///
    /// THE ASSERTION THAT FAILS AGAINST THE PRE-CHANGE CODE: `recorded.primary() == ran` -- before
    /// the reconciliation the decision went into the event verbatim, so `primary()` was the
    /// router's own id, which is a config-file card id that no registry on this machine contains.
    /// The two ids below are the MEASURED pair from the live transcript
    /// (`run-01a0f641-d8e5-7566-8247-23b7068f357d.jsonl` seq 0 vs the same run's `params.agent`).
    #[test]
    fn the_routed_event_names_the_agent_that_ran() {
        let ran: ruagent_core::AgentId = "01a0b915-8273-73c4-8fe9-0e35a34d4ded".parse().unwrap();
        let router: ruagent_core::AgentId = "01a0f580-b2ac-7048-a136-6bbe33d5f13b".parse().unwrap();
        let recorded = routed_naming_what_ran(
            RoutingDecision {
                agents: vec![router],
                source: RouteSource::Default,
                rationale: None,
            },
            ran,
        );
        assert_eq!(
            recorded.primary(),
            ran,
            "the routed event must name the agent whose prompt was injected"
        );
        assert!(
            recorded.agents.iter().all(|a| *a == ran),
            "no unresolvable id may remain in `agents`: {:?}",
            recorded.agents
        );
        assert_eq!(
            recorded.source,
            RouteSource::Default,
            "the cascade level that produced the decision is still recorded"
        );
        let why = recorded.rationale.clone().unwrap_or_default();
        assert!(
            why.contains(&router.to_string()),
            "the router's own id is kept as provenance rather than dropped: {why}"
        );

        // A decision that already names the run is passed through untouched -- no gratuitous
        // rewrite of the other producers' decisions (retry/fan-out/pipeline all use `card.id`).
        let same = RoutingDecision {
            agents: vec![ran],
            source: RouteSource::Explicit,
            rationale: Some("retry of run x".into()),
        };
        let kept = routed_naming_what_ran(same, ran);
        assert_eq!(kept.rationale.as_deref(), Some("retry of run x"));
        assert_eq!(kept.agents, vec![ran]);
    }

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

    /// t309: ONE truncation vocabulary. The contract owns the bytes; every
    /// daemon-side producer of agent context must render exactly them. This
    /// compares the producers against the contract itself, so changing either
    /// producer's wording -- or the vocabulary -- turns it red.
    #[test]
    fn truncation_markers_come_from_the_contract_vocabulary() {
        use ruagent_memory::inject::{cut_at, tail_truncated};

        // The retry tail: the last thing the agent was doing before it died.
        let retry = bounded_tail(&"x".repeat(2_000), 1_500);
        // The handoff: the upstream result, bounded at a fixed constant.
        let handoff = compose_handoff("do things", &"y".repeat(10_000));
        println!("T309 contract_retry_marker={:?}", tail_truncated(500));
        println!(
            "T309 contract_handoff_marker={:?}",
            cut_at("upstream result", 4_000)
        );

        assert!(
            retry.contains(&tail_truncated(500)),
            "the retry tail's marker is not the contract's: {retry}"
        );
        assert!(
            handoff.contains(&cut_at("upstream result", 4_000)),
            "the handoff's marker is not the contract's: {handoff}"
        );
        // The CONVENTION both share -- one byte, and it is the byte the two
        // producers disagreed on before t309 ("… [+" vs "…[+").
        for m in [tail_truncated(1), cut_at("x", 1)] {
            assert!(
                m.starts_with("… ["),
                "the vocabulary's convention drifted: {m}"
            );
        }
    }

    /// The head token of a block that formats itself as "head — rest".
    fn head_of(block: &str) -> &str {
        block.split(" — ").next().unwrap_or(block)
    }

    /// t326: the retry prefix is shared by CONSTRUCTION -- chat.rs owns
    /// retry_head() and runs.rs calls it -- but nothing pinned its BYTES, so
    /// replacing that call with a lookalike literal would have gone unnoticed.
    /// This drives the real function and compares bytes, not shapes.
    ///
    /// It is the half t309 left open: t309 pinned the truncation MARKER with a
    /// structural helper plus a byte assertion; the block PREFIXES had the
    /// structure and lacked the assertion.
    #[test]
    fn retry_context_opens_with_the_shared_retry_prefix() {
        use crate::chat::retry_head;
        use ruagent_core::ContentBlock;

        let dir = std::env::temp_dir().join(format!("ruagent-t326-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let task = Task::new("t326", "retry the dead run", TaskCreator::Human);

        // Branch 1: died before saying anything (no agent speech at all).
        let mut early = Run::new(
            task.id,
            RunParams::for_agent(ruagent_core::AgentId::generate()),
        );
        early.error = Some("the harness died".into());
        let path = ruagent_store::transcript_path(&dir, &early.id);
        std::fs::write(&path, "").unwrap();
        let text =
            retry_context(&dir, &early).expect("an errored run must produce a retry context");
        println!("T326 retry_context(early) = {text:?}");
        // BYTES, not shape: the head token is compared with ==, so a
        // lookalike that differs by one byte (the t309 class) is red -- and
        // so is a longer literal, because the head is taken up to the
        // separator the format itself uses.
        assert_eq!(
            head_of(&text),
            retry_head(),
            "the retry prefix is not the shared bytes: {text:?}"
        );

        // Branch 2: died after speaking (the tail is kept behind the prefix).
        let mut late = Run::new(
            task.id,
            RunParams::for_agent(ruagent_core::AgentId::generate()),
        );
        late.error = Some("the harness died".into());
        let path2 = ruagent_store::transcript_path(&dir, &late.id);
        let line = serde_json::json!({
            "ts": "2026-01-01T00:00:00Z",
            "seq": 1,
            "event": RunEvent::AgentMessageChunk {
                content: vec![ContentBlock::Text { text: "the last thing it said".into() }],
            },
        });
        std::fs::write(&path2, format!("{line}\n")).unwrap();
        let text2 =
            retry_context(&dir, &late).expect("a run that spoke must produce a retry context");
        println!("T326 retry_context(late) = {text2:?}");
        assert_eq!(
            head_of(&text2),
            retry_head(),
            "the retry prefix is not the shared bytes: {text2:?}"
        );
        assert!(text2.contains("the last thing it said"), "{text2}");

        // And the shared head is the bracketed block head, not a stand-in.
        assert!(
            retry_head().starts_with('[') && retry_head().ends_with("context"),
            "retry_head is not the block head: {:?}",
            retry_head()
        );
        crate::test_cleanup::report_test_root_left_behind(&dir);
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

    /// F7: ONE pair of numbers, TWO loops, and the cadence both of them produce -- the reading that
    /// says the merge changed no behaviour.
    ///
    /// The expected vector is the sequence MEASURED BEFORE the merge, printed separately by each
    /// path from its OWN constants (both said [20, 40, 80, 160, 250, 250, 250, 250]). The test now
    /// derives that sequence three ways -- from the shared pair, and from each path's own names for
    /// it -- so a changed number, a changed cap rule or a path that stopped sharing fails HERE
    /// rather than as an occasional timeout on a loaded machine.
    #[test]
    fn the_two_attribution_loops_share_one_poll_cadence() {
        let cadence = |first: u64, max: u64, steps: usize| -> Vec<u64> {
            let mut out = Vec::new();
            let mut wait_ms = first;
            for _ in 0..steps {
                out.push(wait_ms);
                wait_ms = (wait_ms * 2).min(max);
            }
            out
        };
        let shared = cadence(CHILD_POLL_FIRST_MS, CHILD_POLL_MAX_MS, 8);
        let chat_path = cadence(
            crate::chat::ATTRIBUTION_FIRST_POLL_MS,
            crate::chat::ATTRIBUTION_MAX_POLL_MS,
            8,
        );
        let run_path = cadence(CHILD_POLL_FIRST_MS, CHILD_POLL_MAX_MS, 8);
        println!(
            "READING F7 poll cadence: shared={shared:?} chat_path={chat_path:?} run_path={run_path:?}"
        );
        assert_eq!(
            shared,
            vec![20, 40, 80, 160, 250, 250, 250, 250],
            "the cadence measured before the merge"
        );
        assert_eq!(
            chat_path, shared,
            "the chat path doubles with the shared pair"
        );
        assert_eq!(
            run_path, shared,
            "the run path doubles with the shared pair"
        );
        assert_eq!(
            (CHILD_POLL_FIRST_MS, CHILD_POLL_MAX_MS),
            (
                crate::chat::ATTRIBUTION_FIRST_POLL_MS,
                crate::chat::ATTRIBUTION_MAX_POLL_MS
            ),
            "one definition: both names are the same two values"
        );
    }
}
