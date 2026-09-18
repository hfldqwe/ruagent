//! ruagent-policy: deterministic decision rules.
//!
//! M1 scope (design §9.2, level ①): config-driven permission rules —
//! first matching rule wins, fall back to the default. The full cascade
//! (approver agent, scoped authority, high-risk human escalation) is M2.

use serde::Deserialize;

/// What to do with a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    /// Auto-select the first allow-kind option.
    Allow,
    /// Auto-select the first reject-kind option.
    Reject,
    /// Park for a human decision (the inbox). Fail-closed default.
    Ask,
}

/// The full resolution path for a permission request (design SS9.2):
/// rules auto-answer; the approver agent delegates; the human inbox is
/// the fail-closed floor. High-risk titles and the approver's own runs
/// always go to the human.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionPath {
    /// A deterministic rule decided.
    Auto(PermissionAction),
    /// The configured approver agent decides (unattended operation).
    Delegate,
    /// Park in the human inbox.
    Human,
}

/// The M1 permission rule set.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionPolicy {
    pub default: PermissionAction,
    pub rules: Vec<PermissionRule>,
    /// Approver agent name (tier 2). None disables delegation.
    pub approver: Option<ApproverConfig>,
    /// Per-harness concurrency gates (design §8.3).
    pub concurrency: ConcurrencyLimits,
}

/// Resolved `[concurrency]` limits: a fan-out to N agents on one
/// runtime queues instead of spawning N children at once; beyond the
/// queue cap launches are rejected with a clear error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConcurrencyLimits {
    /// Max simultaneous runs per harness kind.
    pub per_harness: usize,
    /// Max runs WAITING per harness before new launches are rejected.
    pub queue_per_harness: usize,
}

impl Default for ConcurrencyLimits {
    fn default() -> Self {
        Self {
            per_harness: 2,
            queue_per_harness: 8,
        }
    }
}

/// Tier-2 configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ApproverConfig {
    /// Agent name that decides on behalf of the human.
    pub agent: String,
    /// Title substrings that ALWAYS escalate to the human, regardless of
    /// the approver (design SS9.2 hard constraint).
    pub high_risk: Vec<String>,
}

/// One rule: matches when the tool title contains `title_contains`.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRule {
    pub title_contains: String,
    pub action: PermissionAction,
}

impl PermissionPolicy {
    /// Decide for a tool title: first matching rule wins, else default.
    pub fn decide(&self, title: &str) -> PermissionAction {
        for rule in &self.rules {
            if title
                .to_lowercase()
                .contains(&rule.title_contains.to_lowercase())
            {
                return rule.action;
            }
        }
        self.default
    }

    /// The full path for a request, honoring the approver tier.
    /// `asking_agent` is the agent of the run that asked (as an id
    /// string); `approver_agent_id` is the resolved approver id.
    pub fn path(
        &self,
        title: &str,
        asking_agent: Option<&str>,
        approver_agent_id: Option<&str>,
    ) -> PermissionPath {
        let action = self.decide(title);
        if !matches!(action, PermissionAction::Ask) {
            return PermissionPath::Auto(action);
        }
        let Some(approver) = &self.approver else {
            return PermissionPath::Human;
        };
        // High-risk titles always escalate to the human.
        let lower = title.to_lowercase();
        if approver
            .high_risk
            .iter()
            .any(|h| lower.contains(&h.to_lowercase()))
        {
            return PermissionPath::Human;
        }
        // The approver cannot approve runs it participates in.
        if let (Some(asking), Some(approver_id)) = (asking_agent, approver_agent_id)
            && asking == approver_id
        {
            return PermissionPath::Human;
        }
        PermissionPath::Delegate
    }
}

// ---------------------------------------------------------------------------
// Config (policy.toml)
// ---------------------------------------------------------------------------

/// Root of `policy.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub permissions: PermissionsConfig,
    #[serde(default)]
    pub distill: DistillConfig,
    #[serde(default)]
    pub concurrency: ConcurrencyConfig,
}

/// `[concurrency]` — per-harness run gates (design §8.3).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConcurrencyConfig {
    /// Max simultaneous runs per harness kind (default 2).
    pub per_harness: Option<usize>,
    /// Max waiting runs per harness before new launches are rejected
    /// with a clear error (default 8).
    pub queue_per_harness: Option<usize>,
}

impl ConcurrencyConfig {
    fn limits(&self) -> ConcurrencyLimits {
        let d = ConcurrencyLimits::default();
        ConcurrencyLimits {
            per_harness: self.per_harness.unwrap_or(d.per_harness),
            queue_per_harness: self.queue_per_harness.unwrap_or(d.queue_per_harness),
        }
    }
}

/// `[distill]` — session → memory distillation policy.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DistillConfig {
    /// Distill sessions automatically when they close (idle timeout or
    /// explicit). Off by default: extraction quality is judged per
    /// deployment before enabling unattended runs.
    #[serde(default)]
    pub auto: bool,
    /// Agent used for extraction; default = dsh if enabled, else the
    /// first enabled agent.
    pub agent: Option<String>,
}

/// `[permissions]` section.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PermissionsConfig {
    pub default: Option<String>,
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    #[serde(default)]
    pub approver: Option<ApproverFileConfig>,
}

/// `[permissions.approver]` section.
#[derive(Debug, Clone, Deserialize)]
pub struct ApproverFileConfig {
    /// Agent name that decides on behalf of the human.
    pub agent: String,
    /// Title substrings that always escalate to the human.
    #[serde(default)]
    pub high_risk: Vec<String>,
}

/// One `[[permissions.rules]]` entry.
#[derive(Debug, Clone, Deserialize)]
pub struct RuleConfig {
    pub title_contains: String,
    pub action: String,
}

impl PolicyConfig {
    /// Parse from TOML text.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Compile to the executable policy.
    pub fn to_policy(&self) -> PermissionPolicy {
        PermissionPolicy {
            concurrency: self.concurrency.limits(),
            default: self
                .permissions
                .default
                .as_deref()
                .and_then(parse_action)
                .unwrap_or(PermissionAction::Ask),
            rules: self
                .permissions
                .rules
                .iter()
                .filter_map(|r| {
                    parse_action(&r.action).map(|a| PermissionRule {
                        title_contains: r.title_contains.clone(),
                        action: a,
                    })
                })
                .collect(),
            approver: self.permissions.approver.as_ref().map(|a| ApproverConfig {
                agent: a.agent.clone(),
                high_risk: a.high_risk.clone(),
            }),
        }
    }
}

fn parse_action(s: &str) -> Option<PermissionAction> {
    match s.to_lowercase().as_str() {
        "allow" => Some(PermissionAction::Allow),
        "reject" => Some(PermissionAction::Reject),
        "ask" => Some(PermissionAction::Ask),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_matching_rule_wins() {
        let policy = PermissionPolicy {
            default: PermissionAction::Ask,
            rules: vec![
                PermissionRule {
                    title_contains: "read".into(),
                    action: PermissionAction::Allow,
                },
                PermissionRule {
                    title_contains: "write file".into(),
                    action: PermissionAction::Reject,
                },
            ],
            approver: None,
            concurrency: ConcurrencyLimits::default(),
        };
        assert_eq!(policy.decide("Read file"), PermissionAction::Allow);
        assert_eq!(policy.decide("WRITE FILE now"), PermissionAction::Reject);
        assert_eq!(policy.decide("Run command"), PermissionAction::Ask);
    }

    #[test]
    fn concurrency_limits_parse() {
        let config =
            PolicyConfig::parse("[concurrency]\nper_harness = 4\nqueue_per_harness = 2\n").unwrap();
        assert_eq!(
            config.concurrency.limits(),
            ConcurrencyLimits {
                per_harness: 4,
                queue_per_harness: 2
            }
        );
        // Omitted keys fall back to the defaults, not zero.
        let partial = PolicyConfig::parse("[concurrency]\nper_harness = 3\n").unwrap();
        assert_eq!(
            partial.concurrency.limits(),
            ConcurrencyLimits {
                per_harness: 3,
                queue_per_harness: 8
            }
        );
        // And an absent section entirely.
        let bare = PolicyConfig::parse("").unwrap();
        assert_eq!(bare.concurrency.limits(), ConcurrencyLimits::default());
    }

    #[test]
    fn config_parses_and_compiles() {
        let text = r#"
[permissions]
default = "ask"

[[permissions.rules]]
title_contains = "read"
action = "allow"
"#;
        let config = PolicyConfig::parse(text).unwrap();
        let policy = config.to_policy();
        assert_eq!(policy.default, PermissionAction::Ask);
        assert_eq!(policy.decide("Read file"), PermissionAction::Allow);
        assert_eq!(policy.decide("Delete repo"), PermissionAction::Ask);
    }

    #[test]
    fn approver_tier_delegates_with_constraints() {
        let policy = PermissionPolicy {
            default: PermissionAction::Ask,
            rules: vec![],
            approver: Some(ApproverConfig {
                agent: "claude".into(),
                high_risk: vec!["delete".into()],
            }),
            concurrency: ConcurrencyLimits::default(),
        };
        let approver_id = "approver-uuid";
        // Normal ask from another agent -> delegate.
        assert_eq!(
            policy.path("Write file", Some("other-uuid"), Some(approver_id)),
            PermissionPath::Delegate
        );
        // The approver's own run -> human (no self-approval).
        assert_eq!(
            policy.path("Write file", Some(approver_id), Some(approver_id)),
            PermissionPath::Human
        );
        // High-risk -> human, always.
        assert_eq!(
            policy.path("Delete the repo", Some("other-uuid"), Some(approver_id)),
            PermissionPath::Human
        );
        // No approver configured -> human.
        let no_approver = PermissionPolicy {
            default: PermissionAction::Ask,
            rules: vec![],
            approver: None,
            concurrency: ConcurrencyLimits::default(),
        };
        assert_eq!(
            no_approver.path("Write file", Some("other-uuid"), None),
            PermissionPath::Human
        );
        // Rules still auto-answer before any delegation.
        let ruled = PermissionPolicy {
            default: PermissionAction::Ask,
            rules: vec![PermissionRule {
                title_contains: "read".into(),
                action: PermissionAction::Allow,
            }],
            approver: Some(ApproverConfig {
                agent: "claude".into(),
                high_risk: vec![],
            }),
            concurrency: ConcurrencyLimits::default(),
        };
        assert_eq!(
            ruled.path("Read file", Some("x"), Some("y")),
            PermissionPath::Auto(PermissionAction::Allow)
        );
    }

    #[test]
    fn unknown_actions_are_dropped_not_fatal() {
        let text = r#"
[[permissions.rules]]
title_contains = "x"
action = "explode"
"#;
        let config = PolicyConfig::parse(text).unwrap();
        assert!(config.to_policy().rules.is_empty());
    }
}
