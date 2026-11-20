//! P3 of `docs/plans/2026-11-20-primary-supervisor-design.md`: the watchdog
//! stops checking a pulse and starts a ROUND.
//!
//! WHY THIS FILE EXISTS. `docs/design/async-entry-audit.md` §2.6 measured four
//! timers in this platform — a five-minute scheduled task, a Monday CI audit, a
//! five-minute MCP health ping, a sixty-second knowledge scan — and found that
//! **not one of them reaches an agent**: `Chat::send_prompt`'s only caller in the
//! whole workspace is the `chat_message` HTTP handler, so nothing but a typed
//! message could drive a turn. This module is the missing shape: a timer that
//! STARTS A RUN.
//!
//! THREE DECISIONS, each because the alternative is a failure this repository
//! has already paid for:
//!
//! * **It calls [`RunManager::start_run`] in-process, never its own HTTP.** A
//!   POST to `/api/v1/tasks/{id}/runs` would make the daemon a client of itself
//!   and would bypass the per-harness concurrency gate (`GateHold`), which only
//!   the in-process call acquires.
//! * **Every round lands as a TASK plus a RUN**, with the task created by
//!   [`TaskCreator::Schedule`]. So a watchdog round shows up in the board and in
//!   the run receipts with no second history of its own, and "who started this?"
//!   is answered by the task's creator rather than by a prefix someone has to
//!   remember to parse.
//! * **Two gates, both off by default.** `[watchdog] enabled` says WHAT to run;
//!   the `watchdog` capability in the plane is the cost gate every
//!   token-spending capability in this daemon passes through. A round runs only
//!   when both say yes — and when the CAPABILITY is what blocks it, the log says
//!   so and says how to open it (silence here would look like a broken timer).
//!
//! WHAT IT DELIBERATELY DOES NOT DO. It does not invent a prompt wrapper, a
//! retry policy, a model, an option set or a workspace; it dispatches the same
//! way a user-dispatched task is dispatched, because a second dispatch path is a
//! second set of rules to keep in step. The prompt is exactly what
//! `[watchdog].prompt` says (or the target task's current intent), and the
//! workspace is `WorkspaceSpec::Fresh` — the same default a panel dispatch with
//! no `cwd`/`repo` gets.

use std::sync::{Arc, RwLock};

use ruagent_core::{
    AgentCard, AgentId, RouteSource, RoutingDecision, Run, RunStatus, Task, TaskCreator, TaskId,
};
use ruagent_policy::{WatchdogConfigError, WatchdogRound, WatchdogTarget};
use ruagent_store::Db;

use crate::capability::{CapabilityId, CapabilityPlane};
use crate::config::{DaemonConfig, RoutingFile};
use crate::runs::{RunLaunch, RunManager, WorkspaceSpec};

/// The `TaskCreator::Schedule` name every watchdog round is recorded under. This
/// is the round's identity: it is what makes "the previous round" a fact the
/// database can answer instead of a field this module keeps in memory (and
/// therefore loses on the restart that follows every crash).
pub const SCHEDULE_NAME: &str = "watchdog";

/// The four non-terminal run statuses, spelled out because `list_runs_by_status`
/// takes one at a time. `is_terminal`/`is_active` in `ruagent_core` remain the
/// ONE definition of the partition — `tick`'s debug assertion below fails if this
/// list ever drifts from it.
const ACTIVE: [RunStatus; 4] = [
    RunStatus::Queued,
    RunStatus::Spawning,
    RunStatus::Running,
    // An empty state today (nothing writes it), and it MUST be counted as
    // non-terminal: a round parked on a permission ask has not finished
    // (docs/plans/2026-11-20-primary-supervisor-design.md §6).
    RunStatus::WaitingPermission,
];

/// What one round did. Returned rather than logged so a caller (the loop, or a
/// test) decides what to say, and so a test can assert on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The capability gate is closed: nothing was launched, and `reason` is the
    /// sentence a human needs (it names the file and the key).
    Blocked(String),
    /// The previous round has not finished, so this one was skipped on purpose.
    Busy {
        run_id: String,
        status: &'static str,
    },
    /// A run was started.
    Started {
        run_id: String,
        task_id: String,
        agent: String,
    },
}

/// The live watcher: one per daemon, spawned by `lib.rs` when `[watchdog]`
/// enables it.
pub struct Watchdog {
    mgr: Arc<RunManager>,
    config: Arc<DaemonConfig>,
    /// The daemon's ONE capability plane, the same `Arc` the chat manager and the
    /// run manager share — never a copy, so `PUT /api/v1/capabilities` is seen at
    /// the next round without a restart.
    capabilities: Arc<RwLock<CapabilityPlane>>,
    round: WatchdogRound,
}

impl Watchdog {
    /// Resolve `[watchdog]` into a watcher, or explain why it cannot run.
    ///
    /// `Ok(None)` = the watchdog is OFF (the shipped default; nothing to log).
    /// `Err` = the section is not usable: the caller MUST log it and start
    /// nothing, because half a watchdog spends tokens on the wrong thing.
    pub fn from_config(
        mgr: Arc<RunManager>,
        config: Arc<DaemonConfig>,
        capabilities: Arc<RwLock<CapabilityPlane>>,
    ) -> Result<Option<Self>, WatchdogConfigError> {
        let Some(round) = config.policy.watchdog.round()? else {
            return Ok(None);
        };
        Ok(Some(Self {
            mgr,
            config,
            capabilities,
            round,
        }))
    }

    /// The loop. Runs until the process ends; a failing round is LOGGED and the
    /// loop continues — the one bug this shape invites is a `?` that takes the
    /// whole daemon with it.
    pub async fn run(self) {
        tracing::info!(
            every = ?self.round.interval,
            agent = self.round.agent.as_deref().unwrap_or("<routing cascade>"),
            "watchdog enabled: one round per interval, the first one AFTER one interval \
             (a restart must not spend a round)"
        );
        // The last gate complaint, so a closed gate is said ONCE per transition
        // instead of every interval — and so reopening it is said too.
        let mut blocked_before: Option<String> = None;
        loop {
            tokio::time::sleep(self.round.interval).await;
            match self.tick().await {
                Ok(Outcome::Blocked(reason)) => {
                    if blocked_before.as_deref() != Some(reason.as_str()) {
                        tracing::warn!(
                            "watchdog: a round was NOT started — the capability gate is closed \
                             (nothing is broken; this is the cost gate doing its job)"
                        );
                        tracing::warn!("{reason}");
                        blocked_before = Some(reason);
                    }
                }
                Ok(Outcome::Busy { run_id, status }) => {
                    if blocked_before.take().is_some() {
                        tracing::info!("watchdog: the capability gate is open again");
                    }
                    tracing::warn!(
                        run_id = %run_id,
                        status = %status,
                        "watchdog: skipping this round — the previous one is still running \
                         (idempotent on purpose: two rounds at once would spend twice)"
                    );
                }
                Ok(Outcome::Started {
                    run_id,
                    task_id,
                    agent,
                }) => {
                    if blocked_before.take().is_some() {
                        tracing::info!("watchdog: the capability gate is open again");
                    }
                    tracing::info!(
                        run_id = %run_id,
                        task_id = %task_id,
                        agent = %agent,
                        "watchdog: started a round"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = %e, "watchdog: this round failed; the loop continues");
                }
            }
        }
    }

    /// One round: gate, idempotency, launch. Every error is a `String` because
    /// this is a LOG LINE, not a decision the caller branches on — the loop above
    /// never propagates one.
    pub async fn tick(&self) -> Result<Outcome, String> {
        debug_assert!(
            ACTIVE.iter().all(|s| s.is_active()),
            "ACTIVE must stay the complement of RunStatus::is_terminal"
        );

        // 1. The cost gate, read from the LIVE plane (a `PUT` takes effect at the
        // next round). The guard is cloned and dropped before any await: the same
        // discipline `ChatManager::capabilities` documents.
        let plane = self.capabilities.read().expect("capability plane").clone();
        if let Some(reason) = gate_refusal(&plane) {
            return Ok(Outcome::Blocked(reason));
        }

        // 2. What to run: an existing task, or the watchdog's own standing task.
        let task = task_for(self.mgr.db(), &self.round.target).await?;

        // 3. Idempotency: a round is skipped while the PREVIOUS one is in flight.
        if let Some(run) = inflight(self.mgr.db(), task.id).await? {
            return Ok(Outcome::Busy {
                run_id: run.id.to_string(),
                status: run.status.status_str(),
            });
        }

        // 4. Who runs it, and how.
        let routing = crate::config::load_routing_file(
            &self.config.root.join("config"),
            &self.config.routing,
        );
        let (card, decision) = target(
            &self.mgr.agents(),
            &routing,
            &task,
            self.round.agent.as_deref(),
            self.mgr.approver_id(),
        )?;
        let prompt = match &self.round.target {
            WatchdogTarget::Prompt(p) => p.clone(),
            // The task's CURRENT intent, re-read every round — editing the task
            // changes the next round instead of leaving a stale copy behind.
            WatchdogTarget::Task(_) => task.intent.clone(),
        };
        let mcp = self
            .config
            .mcp
            .expand_profile_preflighted(card.mcp_profile.as_deref(), &card.name)
            .map_err(|e| format!("pre-flighting the MCP servers of `{}`: {e}", card.name))?;
        let run = self
            .mgr
            .start_run(
                &task,
                &card.name,
                prompt,
                mcp,
                WorkspaceSpec::Fresh,
                RunLaunch {
                    routed: Some(decision),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| {
                format!(
                    "starting a run of task {} on `{}`: {e:#}",
                    task.id, card.name
                )
            })?;
        Ok(Outcome::Started {
            run_id: run.id.to_string(),
            task_id: task.id.to_string(),
            agent: card.name,
        })
    }
}

/// Why the capability plane refuses a round — `None` when it allows one.
///
/// `gate(id, legacy)` with `legacy = true` BY CONSTRUCTION: a `Watchdog` only
/// exists because `[watchdog] enabled` said so, so that flag IS this
/// capability's pre-plane switch (law L1: with no `[capabilities]` table the gate
/// passes through, which is what makes `[watchdog] enabled = true` alone enough).
///
/// The refusal is a SENTENCE, not a bool, because the failure it prevents is
/// silent by nature: `[capabilities]` present for some unrelated reason puts
/// every capability at its registry default, `watchdog` is `llm`-tier, `llm`-tier
/// defaults OFF — so the watchdog would simply never fire and the user would be
/// looking at a timer that looks enabled.
pub fn gate_refusal(plane: &CapabilityPlane) -> Option<String> {
    if plane.gate(CapabilityId::Watchdog, true) {
        return None;
    }
    Some(format!(
        "the `watchdog` capability is OFF while [watchdog] enabled = true, so no round will \
         start. Fix it in policy.toml:\n    [capabilities.{}]\n    enabled = true\n(or \
         PUT /api/v1/capabilities with confirm_cost = true — it spends model tokens). The \
         capability is the cost gate your `[capabilities]` table applies to every \
         token-spending feature; with no such table at all the gate passes through.",
        CapabilityId::Watchdog.as_str()
    ))
}

/// The task a round works on: the configured one, or the watchdog's own standing
/// task for this prompt (created once, reused every round — one task, one run per
/// round, so the board shows one "watchdog" entry whose history IS the record
/// rather than a new row every fifteen minutes).
pub async fn task_for(db: &Db, target: &WatchdogTarget) -> Result<Task, String> {
    match target {
        WatchdogTarget::Task(id) => {
            let task_id: TaskId = id
                .parse()
                .map_err(|_| format!("`[watchdog] task = \"{id}\"` is not a task id"))?;
            db.get_task(task_id)
                .await
                .map_err(|e| format!("reading task {task_id}: {e}"))?
                .ok_or_else(|| {
                    format!(
                        "`[watchdog] task = \"{id}\"` names a task that does not exist — \
                         point it at a real task or use `prompt` instead"
                    )
                })
        }
        WatchdogTarget::Prompt(prompt) => {
            let existing = db
                .list_tasks(None)
                .await
                .map_err(|e| format!("listing tasks: {e}"))?;
            // Newest first (`list_tasks` orders by created_at DESC), and matched
            // on the INTENT: editing `[watchdog].prompt` must not silently keep
            // running the old text under a task row that says the new one.
            if let Some(task) = existing
                .into_iter()
                .find(|t| is_watchdog_task(t) && t.intent == *prompt)
            {
                return Ok(task);
            }
            let task = Task::new(
                round_title(prompt),
                prompt.clone(),
                TaskCreator::Schedule {
                    name: SCHEDULE_NAME.to_string(),
                },
            );
            db.insert_task(&task)
                .await
                .map_err(|e| format!("creating the watchdog task: {e}"))?;
            Ok(task)
        }
    }
}

/// True when this task was created by the watchdog's schedule.
fn is_watchdog_task(task: &Task) -> bool {
    matches!(&task.creator, TaskCreator::Schedule { name } if name == SCHEDULE_NAME)
}

/// A board-readable title: the first non-empty line of the prompt, bounded, so a
/// multi-paragraph intent does not become a task row nobody can scan.
fn round_title(prompt: &str) -> String {
    const MAX: usize = 60;
    let first = prompt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("periodic round");
    let mut cut: String = first.chars().take(MAX).collect();
    if first.chars().count() > MAX {
        cut.push('…');
    }
    format!("watchdog: {cut}")
}

/// The run of `task_id` that has not finished, if there is one.
///
/// Asked through the EXISTING reader (`list_runs_by_status`) rather than a new
/// query: the four non-terminal statuses are also what the boot-time crash sweep
/// walks, so "still running" means the same thing in both places.
pub async fn inflight(db: &Db, task_id: TaskId) -> Result<Option<Run>, String> {
    for status in ACTIVE {
        let runs = db
            .list_runs_by_status(status)
            .await
            .map_err(|e| format!("listing {status:?} runs: {e}"))?;
        if let Some(run) = runs.into_iter().find(|r| r.task_id == task_id) {
            return Ok(Some(run));
        }
    }
    Ok(None)
}

/// Who runs the round — the ONE place this module decides it.
///
/// `[watchdog].agent` names an agent: it is used, and it is used ONLY if it is
/// registered and enabled (naming an agent that cannot run is a configuration
/// error the user must see, never a silent substitution).
///
/// Otherwise the platform's own routing cascade decides, in the same order a
/// dispatched task goes through: task pin → `routing.toml` rule → `routing.toml`
/// `default` → the `primary = true` card → the first enabled card that is not the
/// permission approver.
///
/// THE CASCADE LIVES IN TWO PLACES AND THAT IS A DEBT, recorded here rather than
/// discovered later. Levels 1–3 are the orchestrator's own
/// [`ruagent_orchestrator::route`] — the same call `api.rs`'s `start_run` makes,
/// with the same name→id resolution — but levels 4–5 (the primary marker and the
/// approver-skipping fallback) are implemented in `api.rs` as `primary_decision`
/// and `default_route_agent`, which are PRIVATE to that module, and `api.rs`
/// belongs to another owner in this increment. So this function carries the two
/// arms until they can be shared; see the report's handover note.
pub fn target(
    cards: &[AgentCard],
    routing: &RoutingFile,
    task: &Task,
    named: Option<&str>,
    approver: Option<AgentId>,
) -> Result<(AgentCard, RoutingDecision), String> {
    if let Some(name) = named {
        let card = cards.iter().find(|c| c.name == name).ok_or_else(|| {
            format!(
                "`[watchdog] agent = \"{name}\"` is not a registered agent (registered: {})",
                if cards.is_empty() {
                    "none".to_string()
                } else {
                    cards
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            )
        })?;
        if !card.enabled {
            return Err(format!(
                "`[watchdog] agent = \"{name}\"` is DISABLED in agents.toml, so it cannot run a \
                 round — enable it or name another (the watchdog does not pick a replacement \
                 behind your back)"
            ));
        }
        return Ok((card.clone(), RoutingDecision::explicit(card.id)));
    }

    // Levels 1–3: the orchestrator's cascade, fed the enabled cards' ids by NAME
    // (a disabled card never resolves, the rule api.rs documents as F5).
    let mut by_name: std::collections::HashMap<&str, AgentId> = cards
        .iter()
        .filter(|c| c.enabled)
        .map(|c| (c.name.as_str(), c.id))
        .collect();
    let mut resolve = |name: &str| by_name.remove(name).map(|id| id.to_string());
    let config = ruagent_orchestrator::RoutingConfig {
        routes: routing
            .routes
            .iter()
            .filter_map(|r| {
                resolve(&r.agent).map(|id| ruagent_orchestrator::RoutingRule {
                    project: r.project.clone(),
                    title_contains: r.title_contains.clone(),
                    agent: id,
                })
            })
            .collect(),
        default: routing.default.as_deref().and_then(resolve),
    };
    if let Some(decision) = ruagent_orchestrator::route(task, &config) {
        let id = decision.primary();
        let card = cards.iter().find(|c| c.id == id).ok_or_else(|| {
            format!("the routing cascade named agent {id}, which is not registered")
        })?;
        return Ok((card.clone(), decision));
    }

    // Levels 4–5: nothing is configured, so a card is picked by MARKER, then by
    // ordering — and the fallback skips the permission approver, which adjudicates
    // instead of working.
    let primary = cards.iter().find(|c| c.enabled && c.primary);
    let card = match primary {
        Some(card) => {
            let mut decision = RoutingDecision::default_agent(card.id);
            decision.rationale = Some(format!(
                "no agent named in [watchdog], no route matched and no routing.toml default; \
                 using the PRIMARY agent `{}` (agents.toml marks it `primary = true`)",
                card.name
            ));
            (card.clone(), decision)
        }
        None => {
            let card = cards
                .iter()
                .find(|c| c.enabled && Some(c.id) != approver)
                .ok_or_else(|| {
                    "no agent named in [watchdog], no route matched, and no enabled agent is \
                     registered: the watchdog has nothing to run a round on"
                        .to_string()
                })?;
            let mut decision = RoutingDecision::default_agent(card.id);
            decision.source = RouteSource::Default;
            decision.rationale = Some(format!(
                "no agent named in [watchdog], no route matched and no default configured; \
                 using the first enabled card `{}` (set [watchdog].agent or a routing.toml \
                 default to choose deliberately)",
                card.name
            ));
            (card.clone(), decision)
        }
    };
    Ok(card)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruagent_core::{HarnessKind, TaskStatus};
    use ruagent_policy::PolicyConfig;

    /// A `DaemonConfig` without touching the filesystem: every field is public
    /// and nothing here reads a file (`load_routing_file` falls back to the
    /// snapshot when `routing.toml` is absent, which is what a test root is).
    fn config_with(policy: &str) -> Arc<DaemonConfig> {
        Arc::new(DaemonConfig {
            root: std::env::temp_dir().join("ruagent-watchdog-unit"),
            agents: Vec::new(),
            mcp: crate::config::McpConfig::default(),
            policy: PolicyConfig::parse(policy).expect("policy parses"),
            routing: RoutingFile::default(),
            capabilities: CapabilityPlane::legacy(),
        })
    }

    fn manager(db: &Db, cards: Vec<AgentCard>) -> Arc<RunManager> {
        Arc::new(RunManager::new(
            db.clone(),
            std::env::temp_dir().join("ruagent-watchdog-unit"),
            cards,
            PolicyConfig::default().to_policy(),
            crate::config::McpConfig::default(),
        ))
    }

    /// A run row in `status` for `task`, on the in-memory database.
    async fn run_row(db: &Db, task: TaskId, status: RunStatus) -> Run {
        let mut run = Run::new(
            task,
            ruagent_core::RunParams::for_agent(AgentId::generate()),
        );
        run.status = status;
        db.insert_run(&run).await.expect("insert");
        run
    }

    /// A card with a REAL agent id (`AgentId` is a ULID: a short literal is a
    /// parse error, not a convenient fixture).
    fn card(name: &str, enabled: bool, primary: bool) -> AgentCard {
        AgentCard {
            id: AgentId::generate(),
            name: name.to_string(),
            harness: HarnessKind::Mock,
            command: None,
            description: String::new(),
            model: None,
            reasoning_effort: None,
            context_window: None,
            mcp_profile: None,
            models: Vec::new(),
            prompt: None,
            runtime: None,
            runtimes: Vec::new(),
            options: std::collections::BTreeMap::new(),
            tags: Vec::new(),
            enabled,
            primary,
        }
    }

    fn plane(text: &str) -> CapabilityPlane {
        CapabilityPlane::from_policy(&PolicyConfig::parse(text).expect("policy parses"))
            .expect("plane builds")
    }

    // -----------------------------------------------------------------------
    // 3. The gate (the required "闸门联动" test, at UNIT level: the round's own
    //    decision function, with no daemon and no agent).
    // -----------------------------------------------------------------------

    #[test]
    fn the_watchdog_capability_is_the_cost_gate_and_a_closed_gate_explains_itself() {
        // No `[capabilities]` table at all: the gate passes through (L1), which is
        // what makes `[watchdog] enabled = true` on its own enough.
        assert_eq!(gate_refusal(&CapabilityPlane::legacy()), None);

        // A table that exists for ANY reason puts every capability at its
        // registry default, and this one defaults OFF (L4, llm tier). This is the
        // silent failure the sentence exists for.
        let reason = gate_refusal(&plane("[capabilities]\n")).expect("blocked");
        assert!(reason.contains("watchdog"), "{reason}");
        assert!(
            reason.contains("[capabilities.watchdog]"),
            "the remedy is named exactly: {reason}"
        );
        assert!(reason.contains("enabled = true"), "{reason}");

        // Explicitly off, and explicitly on.
        assert!(gate_refusal(&plane("[capabilities.watchdog]\nenabled = false\n")).is_some());
        assert_eq!(
            gate_refusal(&plane("[capabilities.watchdog]\nenabled = true\n")),
            None
        );
        // And another capability being configured does not open OR close this one
        // beyond what the registry default already says.
        assert_eq!(
            gate_refusal(&plane(
                "[capabilities.watchdog]\nenabled = true\n[capabilities.recall_leg_wiki]\nenabled = false\n"
            )),
            None
        );
    }

    // -----------------------------------------------------------------------
    // The routing cascade (levels 1-3 through the orchestrator, 4-5 locally).
    // -----------------------------------------------------------------------

    fn routing(text: &str) -> RoutingFile {
        toml::from_str(text).expect("routing parses")
    }

    #[test]
    fn a_named_agent_is_used_and_a_bad_name_is_refused_out_loud() {
        let cards = vec![card("dsh", true, false), card("off", false, false)];
        let task = Task::new("t", "i", TaskCreator::Human);
        let r = routing("");

        let (picked, decision) = target(&cards, &r, &task, Some("dsh"), None).expect("named");
        assert_eq!(picked.name, "dsh");
        assert_eq!(decision.source, RouteSource::Explicit);
        assert_eq!(decision.primary(), cards[0].id);

        let unknown = target(&cards, &r, &task, Some("nope"), None).unwrap_err();
        assert!(unknown.contains("not a registered agent"), "{unknown}");
        assert!(
            unknown.contains("dsh"),
            "the registered names are listed: {unknown}"
        );

        // A DISABLED agent is a configuration error, never a silent substitution.
        let disabled = target(&cards, &r, &task, Some("off"), None).unwrap_err();
        assert!(disabled.contains("DISABLED"), "{disabled}");
    }

    #[test]
    fn without_a_name_the_cascade_decides_in_the_platforms_order() {
        let cards = vec![
            card("first", true, false),
            card("primary", true, true),
            card("approver", true, false),
        ];
        let r = routing("default = \"first\"\n");
        let mut task = Task::new("deploy the site", "i", TaskCreator::Human);

        // Level 1: the task's own pin wins over everything, including the file.
        task.pinned_agent = Some(cards[2].id);
        let (picked, decision) = target(&cards, &r, &task, None, None).expect("explicit pin");
        assert_eq!(picked.name, "approver");
        assert_eq!(decision.source, RouteSource::Explicit);
        task.pinned_agent = None;

        // Level 2: a matching rule.
        let ruled = routing("[[routes]]\ntitle_contains = \"deploy\"\nagent = \"primary\"\n");
        let (picked, decision) = target(&cards, &ruled, &task, None, None).expect("rule");
        assert_eq!(picked.name, "primary");
        assert!(matches!(decision.source, RouteSource::Rule { .. }));

        // Level 3: routing.toml's default, which outranks the primary marker.
        let (picked, decision) = target(&cards, &r, &task, None, None).expect("default");
        assert_eq!(
            picked.name, "first",
            "a configured default is the operator's choice"
        );
        assert_eq!(decision.source, RouteSource::Default);

        // Level 4: no rule, no default -> the primary marker.
        let (picked, decision) = target(&cards, &routing(""), &task, None, None).expect("primary");
        assert_eq!(picked.name, "primary");
        assert!(
            decision
                .rationale
                .as_deref()
                .is_some_and(|r| r.contains("PRIMARY agent")),
            "{:?}",
            decision.rationale
        );

        // Level 5: nothing marked -> the first enabled card, and NEVER the
        // permission approver (it adjudicates instead of working).
        let plain = vec![card("approver", true, false), card("worker", true, false)];
        let (picked, decision) =
            target(&plain, &routing(""), &task, None, Some(plain[0].id)).expect("fallback");
        assert_eq!(picked.name, "worker", "the approver is skipped");
        assert!(
            decision
                .rationale
                .as_deref()
                .is_some_and(|r| r.contains("first enabled card")),
            "{:?}",
            decision.rationale
        );

        // A registry with nothing runnable is an ERROR, not a panic and not a
        // round on some other agent.
        let none = target(
            &[card("off", false, false)],
            &routing(""),
            &task,
            None,
            None,
        )
        .unwrap_err();
        assert!(none.contains("nothing to run"), "{none}");
    }

    // -----------------------------------------------------------------------
    // 2. Idempotency (the required test, at UNIT level: the round's own
    //    decision against a real `Db`, with no agent process involved).
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn the_previous_round_in_flight_skips_this_one_and_a_finished_one_does_not() {
        let db = Db::open_in_memory().expect("in-memory db");
        let task = task_for(&db, &WatchdogTarget::Prompt("sweep".into()))
            .await
            .expect("the standing task");

        // Nothing has run yet: the round would start.
        assert!(inflight(&db, task.id).await.expect("query").is_none());

        // A run in ANY non-terminal state blocks the next round - including the
        // empty WaitingPermission state, which is the one a hand-written list
        // would forget.
        for status in ACTIVE {
            let run = run_row(&db, task.id, status).await;
            let blocked = inflight(&db, task.id)
                .await
                .expect("query")
                .unwrap_or_else(|| panic!("{status:?} must count as still running"));
            assert_eq!(blocked.id, run.id, "{status:?}");
            // A FINISHED run does not block: the next round goes ahead.
            let mut done = run.clone();
            done.status = RunStatus::Completed;
            db.update_run(&done).await.expect("update");
            assert!(
                inflight(&db, task.id).await.expect("query").is_none(),
                "{status:?} completed must not block the next round"
            );
        }

        // A run of a DIFFERENT task never blocks this one: two watchdog prompts
        // (or one watchdog and one human task) are independent.
        let other = Task::new("other", "other", TaskCreator::Human);
        db.insert_task(&other).await.expect("insert");
        run_row(&db, other.id, RunStatus::Running).await;
        assert!(inflight(&db, task.id).await.expect("query").is_none());
        assert!(inflight(&db, other.id).await.expect("query").is_some());
    }

    /// The standing task is created once and reused, carries the schedule
    /// creator, and follows the configured prompt instead of going stale.
    #[tokio::test]
    async fn the_standing_task_is_created_once_and_tracks_the_configured_prompt() {
        let db = Db::open_in_memory().expect("in-memory db");
        let first = task_for(&db, &WatchdogTarget::Prompt("sweep the board".into()))
            .await
            .expect("created");
        assert_eq!(first.status, TaskStatus::Pending);
        assert!(
            is_watchdog_task(&first),
            "the creator is the round's identity: {:?}",
            first.creator
        );
        assert_eq!(first.title, "watchdog: sweep the board");
        assert_eq!(first.intent, "sweep the board");

        let again = task_for(&db, &WatchdogTarget::Prompt("sweep the board".into()))
            .await
            .expect("found");
        assert_eq!(again.id, first.id, "one task, many rounds");
        assert_eq!(db.list_tasks(None).await.expect("list").len(), 1);

        // Editing the prompt must not keep running the old text under a task row
        // that claims the new one: a second, self-describing task is created.
        let changed = task_for(&db, &WatchdogTarget::Prompt("look for failing runs".into()))
            .await
            .expect("created");
        assert_ne!(changed.id, first.id);
        assert_eq!(changed.intent, "look for failing runs");
        assert_eq!(db.list_tasks(None).await.expect("list").len(), 2);
    }

    /// `task` mode dispatches an EXISTING task, and a bad id is an error that
    /// names the field instead of a round that silently never happens.
    #[tokio::test]
    async fn task_mode_dispatches_an_existing_task_and_refuses_a_bad_id() {
        let db = Db::open_in_memory().expect("in-memory db");
        let human = Task::new("nightly audit", "audit the repos", TaskCreator::Human);
        db.insert_task(&human).await.expect("insert");

        let found = task_for(&db, &WatchdogTarget::Task(human.id.to_string()))
            .await
            .expect("found");
        assert_eq!(found.id, human.id);
        assert_eq!(found.intent, "audit the repos");

        let bad = task_for(&db, &WatchdogTarget::Task("not-an-id".into()))
            .await
            .unwrap_err();
        assert!(bad.contains("is not a task id"), "{bad}");

        let missing = task_for(&db, &WatchdogTarget::Task(TaskId::generate().to_string()))
            .await
            .unwrap_err();
        assert!(missing.contains("does not exist"), "{missing}");
    }

    /// The whole tick, on a real manager: the gate blocks it, and with the gate
    /// open it walks all the way to the launch and reports THAT failure as a
    /// `String` (the "must not take the daemon down with a `?`" property).
    #[tokio::test]
    async fn a_blocked_tick_launches_nothing_and_a_permitted_tick_never_panics() {
        let db = Db::open_in_memory().expect("in-memory db");
        // `command = "ruagent-program-that-does-not-exist"` is deliberate: the
        // round resolves the agent and then fails at the spawn pre-flight, which
        // is the LAST step that can fail before a row is written. So this test
        // proves the wiring reaches `start_run` without spawning anything.
        let mut broken = card("worker", true, false);
        broken.command = Some("ruagent-program-that-does-not-exist --acp".into());
        let mgr = manager(&db, vec![broken]);
        let config =
            config_with("[watchdog]\nenabled = true\ninterval = \"15m\"\nprompt = \"sweep\"\n");

        // (1) capability OFF in a PRESENT table -> blocked, and nothing is
        // created: no task, no run.
        let closed = Arc::new(RwLock::new(plane("[capabilities]\n")));
        let wd = Watchdog::from_config(Arc::clone(&mgr), Arc::clone(&config), closed)
            .expect("valid config")
            .expect("on");
        match wd.tick().await.expect("a blocked round is not an error") {
            Outcome::Blocked(reason) => assert!(reason.contains("[capabilities.watchdog]")),
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(db.list_tasks(None).await.expect("list").is_empty());
        assert!(
            db.list_runs_by_status(RunStatus::Queued)
                .await
                .expect("runs")
                .is_empty()
        );

        // (2) gate open -> the round creates its standing task and fails at the
        // launch, LOGGABLY.
        let open = Arc::new(RwLock::new(plane(
            "[capabilities.watchdog]\nenabled = true\n",
        )));
        let wd = Watchdog::from_config(mgr, config, open)
            .expect("valid config")
            .expect("on");
        let err = wd.tick().await.unwrap_err();
        assert!(
            err.contains("ruagent-program-that-does-not-exist"),
            "the reason names the command: {err}"
        );
        assert_eq!(
            db.list_tasks(None).await.expect("list").len(),
            1,
            "the task half of the round landed before the launch failed"
        );
    }

    /// The shipped default: `[watchdog]` absent means no watcher at all, which is
    /// the requirement that outranks everything else here (it spends the user's
    /// tokens).
    #[test]
    fn no_watcher_without_an_explicit_enable() {
        let db = Db::open_in_memory().expect("in-memory db");
        let mgr = manager(&db, Vec::new());
        let caps = Arc::new(RwLock::new(CapabilityPlane::legacy()));
        assert!(
            Watchdog::from_config(mgr.clone(), config_with(""), caps)
                .expect("nothing to refuse")
                .is_none(),
            "a daemon with no [watchdog] section must not spawn one"
        );

        // And an invalid section is a typed error, never a half-configured watcher.
        // (`match` rather than `expect_err`: `Watchdog` carries an `Arc<RunManager>`
        // and is deliberately not `Debug`.)
        let err = match Watchdog::from_config(
            mgr,
            config_with("[watchdog]\nenabled = true\ninterval = \"0s\"\nprompt = \"x\"\n"),
            Arc::new(RwLock::new(CapabilityPlane::legacy())),
        ) {
            Err(e) => e,
            Ok(_) => panic!("a zero interval must be refused, not half-configured"),
        };
        assert!(err.to_string().contains("zero"), "{err}");
    }
}
