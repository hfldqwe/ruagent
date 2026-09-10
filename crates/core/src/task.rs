//! Tasks: durable intents that runs execute. See design §5.1.

use crate::id::{AgentId, TaskId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Lifecycle of a durable task intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Created, not yet picked up by any run.
    Pending,
    /// At least one run is executing it.
    InProgress,
    /// Waiting on an external condition (permission, dependency, human gate).
    Blocked,
    /// Finished successfully (its definition of done was accepted).
    Done,
    /// Abandoned.
    Cancelled,
}

/// Typed relation between two tasks. Edges are stored as separate records,
/// never embedded, so the graph stays normalized. See design §5.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// `from` must finish before `to` may start (pipeline stages).
    DependsOn,
    /// `to` was spawned by a decision made during `from` (leader routing, judge).
    SpawnedBy,
    /// `to` reviews the output of `from`.
    Reviews,
    /// `to` is one of the parallel attempts of `from` (fan-out compare).
    FanoutOf,
}

/// A typed edge between two tasks. Direction semantics are defined by
/// [`EdgeKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskEdge {
    pub from: TaskId,
    pub to: TaskId,
    pub kind: EdgeKind,
}

/// Who or what created a task.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskCreator {
    /// The human owner, via CLI or panel.
    Human,
    /// An agent (e.g. a judge or leader run that spawned sub-work).
    Agent { id: AgentId },
    /// A deterministic routing/scheduling rule (name identifies the rule).
    Rule { name: String },
    /// A schedule trigger (name identifies the schedule).
    Schedule { name: String },
}

/// A durable intent: what should be done. Runs are execution attempts
/// against tasks. See design §5.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    /// Short human-readable title.
    pub title: String,
    /// Full intent description (the "what", not the "how").
    pub intent: String,
    pub status: TaskStatus,
    pub creator: TaskCreator,
    /// Project scope; `None` means global/ad-hoc.
    pub project: Option<String>,
    /// Explicit agent pin (routing level 1). See design §5.4.
    pub pinned_agent: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    /// Create a fresh task in [`TaskStatus::Pending`].
    pub fn new(title: impl Into<String>, intent: impl Into<String>, creator: TaskCreator) -> Self {
        let now = Utc::now();
        Self {
            id: TaskId::generate(),
            title: title.into(),
            intent: intent.into(),
            status: TaskStatus::Pending,
            creator,
            project: None,
            pinned_agent: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_task_starts_pending() {
        let t = Task::new("fix bug", "Fix the login bug", TaskCreator::Human);
        assert_eq!(t.status, TaskStatus::Pending);
        assert!(t.created_at <= t.updated_at);
    }

    #[test]
    fn creator_serializes_tagged() {
        let json = serde_json::to_string(&TaskCreator::Human).unwrap();
        assert_eq!(json, r#"{"kind":"human"}"#);
        let back: TaskCreator = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TaskCreator::Human);
        // Internally-tagged enums need struct variants — every variant
        // must roundtrip (the pipeline creator hit this in the wild).
        let rule = TaskCreator::Rule {
            name: "pipeline".into(),
        };
        let back: TaskCreator =
            serde_json::from_str(&serde_json::to_string(&rule).unwrap()).unwrap();
        assert_eq!(back, rule);
    }
}
