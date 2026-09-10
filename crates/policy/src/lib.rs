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

/// The M1 permission rule set.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionPolicy {
    pub default: PermissionAction,
    pub rules: Vec<PermissionRule>,
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
}

// ---------------------------------------------------------------------------
// Config (policy.toml)
// ---------------------------------------------------------------------------

/// Root of `policy.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub permissions: PermissionsConfig,
}

/// `[permissions]` section.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PermissionsConfig {
    pub default: Option<String>,
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
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
        };
        assert_eq!(policy.decide("Read file"), PermissionAction::Allow);
        assert_eq!(policy.decide("WRITE FILE now"), PermissionAction::Reject);
        assert_eq!(policy.decide("Run command"), PermissionAction::Ask);
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
