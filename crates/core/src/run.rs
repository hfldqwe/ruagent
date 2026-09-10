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
    /// Whether the run is still moving (not in a terminal state).
    pub fn is_active(self) -> bool {
        !matches!(
            self,
            RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
        )
    }

    /// Whether the run ended (any terminal state, including interrupted).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Completed
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::Interrupted
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
}

impl RunParams {
    pub fn for_agent(agent: AgentId) -> Self {
        Self {
            agent,
            model: None,
            reasoning_effort: None,
            mcp_profile: None,
            max_tokens: None,
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

    #[test]
    fn status_predicates() {
        assert!(RunStatus::Running.is_active());
        assert!(!RunStatus::Running.is_terminal());
        assert!(RunStatus::Interrupted.is_active());
        assert!(RunStatus::Interrupted.is_terminal());
        assert!(!RunStatus::Completed.is_active());
        assert!(RunStatus::Failed.is_terminal());
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
