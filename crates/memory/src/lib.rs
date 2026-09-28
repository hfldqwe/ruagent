//! ruagent-memory: the six-store memory engine (design §6, blueprint §6.6).
//!
//! Layers:
//! - [`episode`] — the non-lossy base: raw content, hash-deduped.
//! - [`write`] — the governed write pipeline: dedupe / supersede / audit.
//! - [`namespace`] — scoping model, enforced in the storage layer.
//! - [`inject`] — the injection contract: bounded tagged blocks, pure
//!   rendering, property-tested and golden-tested (design §6.4).

pub mod confidence;
pub mod consolidate;
pub mod dedupe;
pub mod episode;
pub mod inject;
pub mod lifecycle;
pub mod namespace;
pub mod query;
pub mod usage;
pub mod write;

pub use confidence::{
    CONF_CONFIRMED, CONF_CORRECTED, CONF_HEDGED, CONF_UNCONFIRMED, ConfidenceSignals,
    LOW_CONFIDENCE, confidence, is_low,
};
pub use consolidate::{ConsolidationWrite, consolidation_key, record_consolidation};
pub use dedupe::{
    MergeCandidate, MergeConfig, MergeVerdict, Verdict, judge, merge_audit_reason, merge_decision,
    scope_tau,
};
pub use inject::{InjectionBudget, MemoryForInjection, render_injection};
pub use lifecycle::{
    DeleteOutcome, ExternalResidual, ForgetReport, LocalResidual, PurgeOutcome, ResidualCount,
    ResidualHit, ResidualOrigin, ResidualStatus, ResidualSurface, RestoreOutcome, delete_memory,
    forget_report, purge_memory, restore_memory,
};
pub use namespace::{Namespace, NamespaceKind};
pub use usage::{DECAY_LAMBDA, decay_score, record_usage};
pub use write::{MemoryWrite, WriteOutcome, audit_merge_decision, write_memory};

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

    /// Parse a store name; `None` for anything unknown.
    ///
    /// ONE PARSER FOR THE READ PATH, and it returns an OPTION so a caller can
    /// refuse. This replaces the read side's `match ... _ => Observation`:
    /// until this existed, a row (or a query) naming an unknown store was
    /// silently reported as an observation. The live table cannot produce such a
    /// row through the governed write path, but "cannot happen" is not a reason
    /// for a read path to invent a value — and the daemon's HTTP layer already
    /// answers 400 for an unknown store (api.rs `parse_store`), so the two faces
    /// now agree on the vocabulary.
    ///
    /// NOT a re-implementation of the daemon's parser (that one lives in api.rs
    /// and is INT's): the two are separate by design, and the drift is a known,
    /// registered duplicate (R-B D.6 B-7 / E.9-②).
    pub fn try_parse(s: &str) -> Option<Self> {
        match s {
            "profile" => Some(MemoryStore::Profile),
            "observation" => Some(MemoryStore::Observation),
            "procedure" => Some(MemoryStore::Procedure),
            "lesson" => Some(MemoryStore::Lesson),
            _ => None,
        }
    }

    /// The namespaces this store may WRITE, as data — THE matrix.
    ///
    /// ONE SOURCE (t52): `allows_namespace` is defined as membership in this
    /// list, and `write_vocabulary()` renders this same list into the string the
    /// rejection carries. Before this, the pair (store, namespace) that the write
    /// path accepts existed only as a `match` arm, while the *claimed* vocabulary
    /// was a flat list of namespace values in three other files — so "global is
    /// a supported value" and "observation cannot write global" could both look
    /// true. Design §6.1's governance table is unchanged; it is now data.
    pub fn allowed_kinds(self) -> &'static [NamespaceKind] {
        use NamespaceKind::*;
        match self {
            // Facts about the human owner: only the owner's namespace.
            MemoryStore::Profile => &[User],
            // Unstructured observations: user/project/agent. NOT global —
            // `global` is the cross-project scope of distilled procedure/lesson.
            MemoryStore::Observation => &[User, Project, Agent],
            MemoryStore::Procedure | MemoryStore::Lesson => &[Project, Global],
        }
    }

    /// Every store, in the order the vocabulary renders them.
    pub const ALL: [MemoryStore; 4] = [
        MemoryStore::Profile,
        MemoryStore::Observation,
        MemoryStore::Procedure,
        MemoryStore::Lesson,
    ];

    /// Namespaces this store may write (design §6.1 table) — governance
    /// enforced at the storage layer, not in prompts (Agno's lesson).
    ///
    /// Membership in `allowed_kinds`, so the enforcement and the published
    /// vocabulary cannot disagree (t52).
    pub fn allows_namespace(self, ns: &Namespace) -> bool {
        self.allowed_kinds().contains(&ns.kind())
    }
}

/// THE WRITE VOCABULARY, as one string: which (store, namespace) pairs the
/// governed write path accepts, rendered from `MemoryStore::allowed_kinds` (the
/// single source) rather than re-typed.
///
/// WHY IT IS PUBLIC (t52). The rejection `WriteOutcome::RejectedNamespace` is
/// serialized into the HTTP body by the daemon (`format!("{outcome:?}")`), which
/// this crate does not own; deriving the sentence from the matrix here is what
/// makes the refusal say WHICH values are supported without a second list to
/// keep in sync. Consumers that currently publish a flat list of namespace
/// values (api.rs:4234's doc comment, `0004_memory.sql:21`'s column comment, and
/// the panel's `NAMESPACES` table) should read this instead — see the t52 report
/// for the routed findings.
pub fn write_vocabulary() -> String {
    MemoryStore::ALL
        .iter()
        .map(|s| {
            let kinds: Vec<&str> = s.allowed_kinds().iter().map(|k| k.as_str()).collect();
            format!("{}×{{{}}}", s.as_str(), kinds.join(","))
        })
        .collect::<Vec<_>>()
        .join(" | ")
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
    /// WHICH EPISODE THIS CAME FROM (t347). Distillation used to say so by
    /// writing "`[distilled] `" into the body; provenance belongs in a
    /// field, so the row carries it here and the body carries only the memory.
    /// `Some(LEGACY_DISTILLED_EPISODE)` = "distilled, episode never
    /// recorded" (the rows the prefix migration found without one).
    pub source_episode: Option<i64>,
}
