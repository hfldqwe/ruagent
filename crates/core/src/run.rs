//! Runs: single execution attempts by one agent against one task. See
//! design §5.1.

use crate::id::{AgentId, RunId, TaskId};

use crate::usage::ContextUsage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lifecycle of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Accepted, waiting for a harness slot.
    Queued,
    /// Agent subprocess is spawning / ACP handshake in flight.
    Spawning,
    /// Executing a prompt.
    Running,
    /// Paused on a permission request awaiting resolution. See design §9.2.
    WaitingPermission,
    /// Finished successfully.
    Completed,
    /// Agent crashed, errored, or violated a protocol expectation.
    Failed,
    /// Cancelled by the user or policy.
    Cancelled,
    /// Daemon died mid-run; recoverable via resume or snapshot. See §8.3.
    Interrupted,
}

impl RunStatus {
    /// Whether the run is still moving — **the exact complement of
    /// [`Self::is_terminal`]**, never a second hand-kept list.
    ///
    /// REVISION 2026-09-29 (t88, adjudicated from the t78 C-2 audit): this used
    /// to be its own `matches!` list, `!matches!(self, Completed | Failed |
    /// Cancelled)`, which reported `Interrupted` as *still moving* while
    /// [`Self::is_terminal`] — on the same type — reported it as *ended*. An
    /// `Interrupted` run is produced by the restart sweep (`crates/daemon`
    /// marks every non-terminal row interrupted at boot) and nothing ever
    /// revives it, so it did terminate: `is_active == false`,
    /// `is_terminal == true`. Defining one side as the negation of the other
    /// makes "both true" unrepresentable rather than merely untested, which is
    /// the fix a third list would not have given.
    pub fn is_active(self) -> bool {
        !self.is_terminal()
    }

    /// Lowercase status name (API/panel friendly).
    pub fn status_str(self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Spawning => "spawning",
            RunStatus::Running => "running",
            RunStatus::WaitingPermission => "waiting_permission",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
            RunStatus::Interrupted => "interrupted",
        }
    }

    /// Whether the run ended (any terminal state, including interrupted).
    ///
    /// The **single source of truth** for the active/terminal partition:
    /// [`Self::is_active`] is defined as this predicate's negation, so the two
    /// cannot disagree. `status_predicates` below reads one row per variant.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Completed
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::Interrupted
        )
    }

    /// Whether a one-click retry may replay this run (design §8.3
    /// crash row): dead-but-not-delivered states. Completed is
    /// "run again", a different gesture.
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            RunStatus::Failed | RunStatus::Interrupted | RunStatus::Cancelled
        )
    }
}

/// Why a run stopped. Mirrors ACP stop reasons plus our own causes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The agent finished its turn.
    EndTurn,
    /// Cancelled by user or policy.
    Cancelled,
    /// Token budget for the run was exhausted.
    MaxTokens,
    /// Turn budget for the run was exhausted (ACP `max_turn_requests`).
    MaxTurns,
    /// The agent refused the request.
    Refusal,
    /// Error (subprocess crash, protocol violation, timeout).
    Error,
}

/// Execution parameters for a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunParams {
    pub agent: AgentId,
    /// Model override (falls back to the agent card default).
    pub model: Option<String>,
    /// Reasoning/thinking effort override.
    pub reasoning_effort: Option<crate::agent::ReasoningEffort>,
    /// Which MCP profile to inject at `session/new`. See design §7.1.
    pub mcp_profile: Option<String>,
    /// Hard cap on total tokens for this run (None = no cap).
    pub max_tokens: Option<u64>,
    /// Canonical session-option defaults applied after `session/new`
    /// (issue #36): `mode` (permission mode) and `effort` (thinking
    /// level), mapped onto whatever the runtime advertises — the same
    /// vocabulary chats and roles use.
    #[serde(default)]
    pub options: std::collections::BTreeMap<String, String>,
    /// The prompt this run was launched with (`None` = the task's
    /// intent). Recorded so a retry replays the original attempt
    /// faithfully (design §8.3 crash row).
    #[serde(default)]
    pub prompt: Option<String>,
}

impl RunParams {
    pub fn for_agent(agent: AgentId) -> Self {
        Self {
            agent,
            model: None,
            reasoning_effort: None,
            mcp_profile: None,
            max_tokens: None,
            options: Default::default(),
            prompt: None,
        }
    }
}

/// A single execution attempt: one agent, one task, full trace + cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub task_id: TaskId,
    pub params: RunParams,
    pub status: RunStatus,
    /// The ACP session id reported by the agent, once established.
    pub acp_session_id: Option<String>,
    /// Per-run isolated working directory (git worktree). See design §8.2.
    pub workspace: Option<String>,
    /// Last context-usage snapshot reported by the agent, if any.
    pub context_usage: Option<ContextUsage>,
    /// Cumulative session cost in USD reported by the agent, if any.
    pub cost_usd: Option<f64>,
    /// Human-readable failure cause when `status == Failed`.
    pub error: Option<String>,
    /// Final agent text (aggregated message chunks) on completion — the
    /// compare/handoff unit for fan-out and pipeline topologies.
    pub result: Option<String>,
    pub stop_reason: Option<StopReason>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Run {
    /// Create a fresh run in [`RunStatus::Queued`].
    pub fn new(task_id: TaskId, params: RunParams) -> Self {
        let now = Utc::now();
        Self {
            id: RunId::generate(),
            task_id,
            params,
            status: RunStatus::Queued,
            acp_session_id: None,
            workspace: None,
            context_usage: None,
            cost_usd: None,
            error: None,
            result: None,
            stop_reason: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The active/terminal partition, read one variant at a time.
    ///
    /// REVISION 2026-09-29 (t88; adjudicated from the t78 C-2 audit). The old
    /// assertions — kept here verbatim so the change is auditable — were:
    ///
    /// ```text
    /// assert!(RunStatus::Interrupted.is_active());   // old: contradictory
    /// assert!(RunStatus::Interrupted.is_terminal());
    /// ```
    ///
    /// They described a state that is simultaneously "still moving" and
    /// "ended". `Interrupted` is *produced* by the restart sweep
    /// (`crates/daemon` marks every non-terminal row interrupted at boot) and
    /// nothing revives it, so it is terminal and not active; `is_active()` is
    /// now derived as `!is_terminal()`. The matrix below states an expected
    /// value for every variant, so flipping either predicate (or adding a
    /// variant without listing it) fails here instead of shipping.
    #[test]
    fn status_predicates() {
        // (status, expected is_active, expected is_terminal)
        let matrix = [
            (RunStatus::Queued, true, false),
            (RunStatus::Spawning, true, false),
            (RunStatus::Running, true, false),
            (RunStatus::WaitingPermission, true, false),
            (RunStatus::Completed, false, true),
            (RunStatus::Failed, false, true),
            (RunStatus::Cancelled, false, true),
            // The row this test used to get wrong: was (active, terminal) => (true, true).
            (RunStatus::Interrupted, false, true),
        ];
        for (status, active, terminal) in matrix {
            // Exhaustive WITHOUT a wildcard arm on purpose: a variant added to
            // `RunStatus` stops this test compiling (non-exhaustive match) until
            // it is listed here — and whoever hits that error must add its row
            // to `matrix` above, which no wildcard arm would have forced.
            let name = match status {
                RunStatus::Queued => "queued",
                RunStatus::Spawning => "spawning",
                RunStatus::Running => "running",
                RunStatus::WaitingPermission => "waiting_permission",
                RunStatus::Completed => "completed",
                RunStatus::Failed => "failed",
                RunStatus::Cancelled => "cancelled",
                RunStatus::Interrupted => "interrupted",
            };
            assert_eq!(status.is_active(), active, "{name}: is_active");
            assert_eq!(status.is_terminal(), terminal, "{name}: is_terminal");
            // Mutually exclusive AND exhaustive: exactly one side is true.
            assert_eq!(
                status.is_active(),
                !status.is_terminal(),
                "{name}: is_active must be the complement of is_terminal"
            );
        }
        // Retryability asks a different question (dead-but-not-delivered), so it
        // is not the partition: `Completed` is re-runnable, not retryable.
        assert!(RunStatus::Failed.is_retryable());
        assert!(RunStatus::Interrupted.is_retryable());
        assert!(RunStatus::Cancelled.is_retryable());
        assert!(!RunStatus::Completed.is_retryable());
        assert!(!RunStatus::Running.is_retryable());
        assert!(!RunStatus::Queued.is_retryable());
    }

    #[test]
    fn new_run_starts_queued() {
        let r = Run::new(
            TaskId::generate(),
            RunParams::for_agent(AgentId::generate()),
        );
        assert_eq!(r.status, RunStatus::Queued);
        assert!(r.context_usage.is_none());
        assert!(r.cost_usd.is_none());
    }
}
