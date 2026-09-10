//! Routing decisions and their provenance. See design §5.4 and §9.1.

use crate::id::AgentId;
use serde::{Deserialize, Serialize};

/// Which level of the cascade produced a decision. Always recorded with
/// the decision so "why this agent" is forever answerable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum RouteSource {
    /// Level 1: explicitly pinned (CLI `--agent` or `task.pinned_agent`).
    Explicit,
    /// Level 2: a deterministic routing rule matched (rule id).
    Rule { rule_id: String },
    /// Level 3: the LLM router decided (pluggable, off by default).
    Llm { model: String },
    /// Fallback: the configured default agent.
    Default,
}

/// The output of `route(task, agent_registry) -> RoutingDecision`.
///
/// `agents` is non-empty; for fan-out it carries the full parallel set
/// (primary first). For direct/pipeline it is a single agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub agents: Vec<AgentId>,
    pub source: RouteSource,
    /// Human-readable reasoning (always present for LLM, optional elsewhere).
    pub rationale: Option<String>,
}

impl RoutingDecision {
    pub fn explicit(agent: AgentId) -> Self {
        Self {
            agents: vec![agent],
            source: RouteSource::Explicit,
            rationale: None,
        }
    }

    pub fn default_agent(agent: AgentId) -> Self {
        Self {
            agents: vec![agent],
            source: RouteSource::Default,
            rationale: None,
        }
    }

    /// The primary (first) agent.
    pub fn primary(&self) -> AgentId {
        self.agents[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_serializes_with_provenance() {
        let d = RoutingDecision {
            agents: vec![AgentId::generate()],
            source: RouteSource::Rule {
                rule_id: "rust-project".into(),
            },
            rationale: Some("project=rust → claude".into()),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: RoutingDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }
}
