//! ruagent-memory: the six-store memory engine (design §6, blueprint §6.6).
//!
//! Layers:
//! - [`episode`] — the non-lossy base: raw content, hash-deduped.
//! - [`write`] — the governed write pipeline: dedupe / supersede / audit.
//! - [`namespace`] — scoping model, enforced in the storage layer.
//! - [`inject`] — the injection contract: bounded tagged blocks, pure
//!   rendering, property-tested and golden-tested (design §6.4).

pub mod episode;
pub mod inject;
pub mod lifecycle;
pub mod namespace;
pub mod query;
pub mod write;

pub use inject::{InjectionBudget, MemoryForInjection, render_injection};
pub use lifecycle::{
    DeleteOutcome, PurgeOutcome, RestoreOutcome, delete_memory, purge_memory, restore_memory,
};
pub use namespace::Namespace;
pub use write::{MemoryWrite, WriteOutcome, write_memory};

/// The four SQLite-backed memory stores (design §6.1; the other two of
/// the six are the entity graph and the decision log, which live in
/// their own tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStore {
    /// Structured facts about the user (`user` namespace only).
    Profile,
    /// Unstructured observations (`user | project | agent` namespaces).
    Observation,
    /// Distilled procedures — how things are done (`project | global`).
    Procedure,
    /// Lessons learned, cross-run (`project | global`).
    Lesson,
}

impl MemoryStore {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryStore::Profile => "profile",
            MemoryStore::Observation => "observation",
            MemoryStore::Procedure => "procedure",
            MemoryStore::Lesson => "lesson",
        }
    }

    /// Namespaces this store may write (design §6.1 table) — governance
    /// enforced at the storage layer, not in prompts (Agno's lesson).
    pub fn allows_namespace(self, ns: &Namespace) -> bool {
        match self {
            MemoryStore::Profile => matches!(ns, Namespace::User),
            MemoryStore::Observation => {
                matches!(
                    ns,
                    Namespace::User | Namespace::Project(_) | Namespace::Agent(_)
                )
            }
            MemoryStore::Procedure | MemoryStore::Lesson => {
                matches!(ns, Namespace::Project(_) | Namespace::Global)
            }
        }
    }
}

/// A memory row as stored.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MemoryRow {
    pub id: i64,
    pub store: MemoryStore,
    pub namespace: String,
    pub content: String,
    pub confidence: f64,
    pub supersedes: Option<i64>,
    pub superseded_at: Option<String>,
    /// Soft-delete tombstone (t251). `Some` = retracted by the user or by a
    /// governance rule; the row and its audit trail stay. Every read path
    /// filters `deleted_at IS NULL`.
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
