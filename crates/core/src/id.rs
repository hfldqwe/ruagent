//! Strongly-typed identifiers.
//!
//! All IDs are UUIDv7 (time-ordered) wrapped in newtypes so they cannot be
//! mixed up at call sites and sort chronologically for free.

use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generate a fresh time-ordered ID.
            pub fn generate() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(v: Uuid) -> Self {
                Self(v)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }
    };
}

define_id!(
    /// A durable task intent. See design §5.1.
    TaskId
);
define_id!(
    /// A single execution attempt against a task. See design §5.1.
    RunId
);
define_id!(
    /// A registered agent (harness instance). See design §4.1.
    AgentId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_time_ordered() {
        let a = TaskId::generate();
        let b = TaskId::generate();
        assert!(a < b);
    }

    #[test]
    fn ids_serialize_transparently() {
        let id = RunId::generate();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        let back: RunId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn ids_roundtrip_through_str() {
        let id = AgentId::generate();
        let parsed: AgentId = id.to_string().parse().unwrap();
        assert_eq!(parsed, id);
        assert!("not-a-uuid".parse::<AgentId>().is_err());
    }
}
