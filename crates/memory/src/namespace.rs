//! Namespaces: the sharing model (design §6.1). Governance is enforced
//! here, in the storage layer — not in prompts.

use std::fmt;

/// A memory namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// Facts about the human owner.
    User,
    /// Shared across everything.
    Global,
    /// Scoped to a project.
    Project(String),
    /// Scoped to one agent.
    Agent(String),
}

impl Namespace {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        match s {
            "user" => Some(Namespace::User),
            "global" => Some(Namespace::Global),
            _ => {
                if let Some(name) = s.strip_prefix("project:") {
                    (!name.is_empty()).then(|| Namespace::Project(name.to_string()))
                } else if let Some(name) = s.strip_prefix("agent:") {
                    (!name.is_empty()).then(|| Namespace::Agent(name.to_string()))
                } else {
                    None
                }
            }
        }
    }

    /// Whether `reader` may see memories in this namespace.
    pub fn visible_to(&self, reader: &Namespace) -> bool {
        use Namespace::*;
        match (self, reader) {
            // Single-user system: the owner and the global scope see
            // everything; and user/global memories are visible to every
            // reader.
            (User, _) | (Global, _) => true,
            (_, User) | (_, Global) => true,
            // project/agent scopes only match themselves.
            (Project(a), Project(b)) => a == b,
            (Agent(a), Agent(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Namespace::User => write!(f, "user"),
            Namespace::Global => write!(f, "global"),
            Namespace::Project(name) => write!(f, "project:{name}"),
            Namespace::Agent(name) => write!(f, "agent:{name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_roundtrip() {
        for s in ["user", "global", "project:ruagent", "agent:claude"] {
            assert_eq!(Namespace::parse(s).unwrap().to_string(), s);
        }
        assert!(Namespace::parse("project:").is_none());
        assert!(Namespace::parse("bogus").is_none());
        assert!(Namespace::parse("").is_none());
    }

    #[test]
    fn visibility_rules() {
        let user = Namespace::User;
        let global = Namespace::Global;
        let proj = Namespace::parse("project:a").unwrap();
        let other_proj = Namespace::parse("project:b").unwrap();
        let agent = Namespace::parse("agent:x").unwrap();

        assert!(proj.visible_to(&user) && proj.visible_to(&global));
        assert!(proj.visible_to(&proj));
        assert!(!proj.visible_to(&other_proj));
        assert!(!proj.visible_to(&agent));
        assert!(user.visible_to(&agent));
    }
}
