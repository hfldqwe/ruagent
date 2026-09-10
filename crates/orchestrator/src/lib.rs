//! ruagent-orchestrator: pure topology planning and the routing cascade.
//!
//! This crate has zero I/O: it turns a [`Topology`] request into a
//! [`Plan`] (runs to launch, sub-tasks to create, typed edges between
//! them). The daemon executes plans; leader routing (design §5.3) and the
//! shared task board (v2) reuse the same primitives.

use ruagent_core::{EdgeKind, RouteSource, RoutingDecision, Task, TaskCreator, TaskEdge};

/// A requested execution shape for a task. See design §5.2.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Topology {
    /// One run on one agent.
    Direct { agent: String },
    /// The same prompt to N agents in parallel; results compared and one
    /// selected (human by default, design §5.2).
    FanOut { agents: Vec<String> },
    /// Sequential steps, each a sub-task; upstream result hands off into
    /// the downstream prompt.
    Pipeline { steps: Vec<PipelineStep> },
}

/// One pipeline stage.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PipelineStep {
    pub agent: String,
    /// Prompt override; `None` = task intent + handoff context.
    pub prompt: Option<String>,
}

impl Topology {
    /// Agents this topology will run on, in order.
    pub fn agents(&self) -> Vec<&str> {
        match self {
            Topology::Direct { agent } => vec![agent],
            Topology::FanOut { agents } => agents.iter().map(String::as_str).collect(),
            Topology::Pipeline { steps } => steps.iter().map(|s| s.agent.as_str()).collect(),
        }
    }
}

/// A single launchable run within a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRun {
    /// The task this run executes (the original or a pipeline sub-task).
    pub task: Task,
    pub agent: String,
    pub prompt: Option<String>,
}

/// What the daemon should execute for one topology request.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub runs: Vec<PlannedRun>,
    /// Typed edges to record (pipeline chains, fan-out lineage).
    pub edges: Vec<TaskEdge>,
    /// Whether runs launch concurrently (fan-out) or sequentially with
    /// handoff (pipeline). Direct = one run, trivially both.
    pub concurrent: bool,
}

/// Planning errors.
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("topology needs at least one agent")]
    Empty,
    #[error("fan-out needs at least two agents to compare (got {0})")]
    FanOutTooSmall(usize),
}

/// Turn a topology request into an executable plan.
///
/// - Direct/FanOut run against the original task.
/// - Pipeline creates one sub-task per step (`depends_on` chain), named
///   after the step's agent; the executor injects handoff context.
pub fn plan(task: &Task, topology: &Topology) -> Result<Plan, PlanError> {
    match topology {
        Topology::Direct { agent } => {
            if agent.is_empty() {
                return Err(PlanError::Empty);
            }
            Ok(Plan {
                runs: vec![PlannedRun {
                    task: task.clone(),
                    agent: agent.clone(),
                    prompt: None,
                }],
                edges: vec![],
                concurrent: false,
            })
        }
        Topology::FanOut { agents } => {
            if agents.is_empty() {
                return Err(PlanError::Empty);
            }
            if agents.len() < 2 {
                return Err(PlanError::FanOutTooSmall(agents.len()));
            }
            Ok(Plan {
                runs: agents
                    .iter()
                    .map(|a| PlannedRun {
                        task: task.clone(),
                        agent: a.clone(),
                        prompt: None,
                    })
                    .collect(),
                edges: vec![],
                concurrent: true,
            })
        }
        Topology::Pipeline { steps } => {
            if steps.is_empty() {
                return Err(PlanError::Empty);
            }
            let mut runs = Vec::new();
            let mut edges = Vec::new();
            let mut prev: Option<Task> = None;
            for (i, step) in steps.iter().enumerate() {
                let sub = if i == 0 && steps.len() == 1 {
                    task.clone()
                } else {
                    Task::new(
                        format!("{} · step {} ({})", task.title, i + 1, step.agent),
                        task.intent.clone(),
                        TaskCreator::Rule {
                            name: "pipeline".into(),
                        },
                    )
                };
                if let Some(upstream) = prev {
                    edges.push(TaskEdge {
                        from: upstream.id,
                        to: sub.id,
                        kind: EdgeKind::DependsOn,
                    });
                }
                prev = Some(sub.clone());
                runs.push(PlannedRun {
                    task: sub,
                    agent: step.agent.clone(),
                    prompt: step.prompt.clone(),
                });
            }
            Ok(Plan {
                runs,
                edges,
                concurrent: false,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Routing cascade (design §5.4 / §9.1)
// ---------------------------------------------------------------------------

/// A deterministic routing rule.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoutingRule {
    /// Matches when the task project equals this (case-insensitive).
    pub project: Option<String>,
    /// Matches when the task carries this tag (M2: matched against title
    /// keywords until task tags land).
    pub title_contains: Option<String>,
    pub agent: String,
}

/// The rule table + default fallback.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct RoutingConfig {
    #[serde(default)]
    pub routes: Vec<RoutingRule>,
    /// Fallback agent name when nothing matches.
    pub default: Option<String>,
}

/// Route a task through the cascade: explicit pin > rules > default.
/// The LLM tier (level 3) is a pluggable future addition — the cascade
/// records provenance either way.
pub fn route(task: &Task, config: &RoutingConfig) -> Option<RoutingDecision> {
    // Level 1: explicit pin.
    if let Some(agent) = &task.pinned_agent {
        return Some(RoutingDecision {
            agents: vec![*agent],
            source: RouteSource::Explicit,
            rationale: None,
        });
    }
    // Level 2: first matching rule.
    for rule in &config.routes {
        let project_match = rule
            .project
            .as_ref()
            .map(|p| {
                task.project
                    .as_deref()
                    .is_some_and(|tp| tp.eq_ignore_ascii_case(p))
            })
            .unwrap_or(true);
        let title_match = rule
            .title_contains
            .as_ref()
            .map(|t| task.title.to_lowercase().contains(&t.to_lowercase()))
            .unwrap_or(true);
        if project_match && title_match {
            let agent: ruagent_core::AgentId = rule.agent.parse().ok()?;
            return Some(RoutingDecision {
                agents: vec![agent],
                source: RouteSource::Rule {
                    rule_id: format!("project~{:?}+title~{:?}", rule.project, rule.title_contains),
                },
                rationale: None,
            });
        }
    }
    // Fallback: configured default.
    config.default.as_ref().and_then(|agent| {
        agent.parse().ok().map(|id| RoutingDecision {
            agents: vec![id],
            source: RouteSource::Default,
            rationale: None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Task {
        Task::new("Fix login bug", "Fix the login flow", TaskCreator::Human)
    }

    #[test]
    fn direct_plans_one_run() {
        let plan = plan(
            &task(),
            &Topology::Direct {
                agent: "claude".into(),
            },
        )
        .unwrap();
        assert_eq!(plan.runs.len(), 1);
        assert_eq!(plan.runs[0].agent, "claude");
        assert!(plan.edges.is_empty());
    }

    #[test]
    fn fanout_needs_two() {
        let err = plan(
            &task(),
            &Topology::FanOut {
                agents: vec!["claude".into()],
            },
        )
        .unwrap_err();
        assert!(matches!(err, PlanError::FanOutTooSmall(1)));
        let plan = plan(
            &task(),
            &Topology::FanOut {
                agents: vec!["claude".into(), "dsh".into(), "opencode".into()],
            },
        )
        .unwrap();
        assert_eq!(plan.runs.len(), 3);
        assert!(plan.concurrent);
        // Same task, different agents.
        let ids: Vec<_> = plan.runs.iter().map(|r| r.task.id).collect();
        assert!(ids.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn pipeline_chains_subtasks() {
        let plan = plan(
            &task(),
            &Topology::Pipeline {
                steps: vec![
                    PipelineStep {
                        agent: "claude".into(),
                        prompt: Some("implement".into()),
                    },
                    PipelineStep {
                        agent: "opencode".into(),
                        prompt: None,
                    },
                    PipelineStep {
                        agent: "dsh".into(),
                        prompt: None,
                    },
                ],
            },
        )
        .unwrap();
        assert_eq!(plan.runs.len(), 3);
        assert_eq!(plan.edges.len(), 2);
        assert_eq!(plan.edges[0].kind, EdgeKind::DependsOn);
        assert_eq!(plan.edges[0].from, plan.runs[0].task.id);
        assert_eq!(plan.edges[0].to, plan.runs[1].task.id);
        assert_eq!(plan.edges[1].from, plan.runs[1].task.id);
        assert!(!plan.concurrent);
        // Step prompts survive; None prompts defer to executor handoff.
        assert_eq!(plan.runs[0].prompt.as_deref(), Some("implement"));
        assert!(plan.runs[1].prompt.is_none());
    }

    #[test]
    fn routing_cascade_prefers_pin_then_rules() {
        let mut t = task();
        t.project = Some("ruagent".into());
        let claude_id = ruagent_core::AgentId::generate();
        let dsh_id = ruagent_core::AgentId::generate();
        let config = RoutingConfig {
            routes: vec![RoutingRule {
                project: Some("ruagent".into()),
                title_contains: None,
                agent: claude_id.to_string(),
            }],
            default: Some(dsh_id.to_string()),
        };

        // No pin, rule matches by project.
        let d = route(&t, &config).unwrap();
        assert!(matches!(d.source, RouteSource::Rule { .. }));
        assert_eq!(d.primary(), claude_id);

        // Explicit pin wins.
        t.pinned_agent = Some(dsh_id);
        let d = route(&t, &config).unwrap();
        assert_eq!(d.source, RouteSource::Explicit);
        assert_eq!(d.primary(), dsh_id);

        // Nothing matches and no default -> None (caller decides).
        let mut t2 = task();
        t2.project = Some("other".into());
        assert!(route(&t2, &RoutingConfig::default()).is_none());
    }
}
