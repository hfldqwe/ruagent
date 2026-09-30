//! The capability plane: the daemon's ONE registry of switchable pipeline
//! capabilities, and the two HTTP handlers that read and replace it
//! (docs/plans/capability-plugins-design.md §4–§6, §14).
//!
//! WHY THIS EXISTS. Memory injection, the six recall legs, unattended session
//! distillation and knowledge ingestion used to be switched either
//! unconditionally or by one unrelated boolean. This module names each of them
//! — a closed enum, so a typo is a compile error rather than a silent default —
//! says what each one costs (zero tokens vs one ACP chat turn) and answers the
//! only question a call site asks: [`CapabilityPlane::gate`].
//!
//! THE FOUR LAWS (§1), restated where a call-site implementer will read them:
//!
//! * **L1 legacy equivalence.** The shipped `policy.toml` has the
//!   `[capabilities]` table COMMENTED OUT, so the table is absent, and with it
//!   absent every gate answers with today's behaviour. `ChatManager::new`
//!   starts from [`CapabilityPlane::legacy`] for the same reason: a constructor
//!   default must not be able to change behaviour.
//! * **L2 narrowing only.** `gate = legacy && (!table_present || enabled(id))`.
//!   A capability can suppress work the legacy flags asked for; it can never
//!   start work they did not request.
//! * **L3 no silent default.** An unknown id, an option key the capability does
//!   not declare, or an out-of-range value is a hard error from
//!   [`CapabilityPlane::from_policy`] — at config load and on `PUT`, and in no
//!   other place, because the runtime accessors take the closed enum and are
//!   therefore total.
//! * **L4 cost is opt-in.** Every `llm`-tier capability defaults OFF, and so
//!   does every capability new in this increment. `PUT` refuses to switch an
//!   llm id on without `confirm_cost: true`.
//!
//! PRESENCE OF THE TABLE IS THE SWITCH. `[capabilities]` with no keys is a
//! PRESENT table and applies every registry default, which switches the llm
//! tier off. "Absent" is the only spelling of "exactly as before".

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::State;

use crate::api::{ApiError, AppState};
use crate::config::CapabilitiesEditor;

// ---------------------------------------------------------------------------
// The registry (§4.2)
// ---------------------------------------------------------------------------

/// Token cost class. `Free` = zero tokens, deterministic, no model call.
/// `Llm` = consumes model tokens (one ACP chat turn).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Free,
    Llm,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Free => "free",
            Tier::Llm => "llm",
        }
    }
}

/// The CLOSED set of capability ids. Strings appear only at the two boundaries
/// (the config file and the HTTP API); every call site uses a variant, so a
/// typo is a compile error rather than a silent default (L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CapabilityId {
    MemoryInjectChat,
    MemoryInjectRuns,
    RecallLegMemorySemantic,
    RecallLegMemoryFts,
    RecallLegKnowledgeSemantic,
    RecallLegKnowledgeFts,
    RecallLegWiki,
    RecallLegGraph,
    SessionExtractRules,
    KnowledgeIngestGraph,
    DistillSession,
}

impl CapabilityId {
    /// Every variant, in registry order.
    pub const ALL: &'static [CapabilityId] = &[
        CapabilityId::MemoryInjectChat,
        CapabilityId::MemoryInjectRuns,
        CapabilityId::RecallLegMemorySemantic,
        CapabilityId::RecallLegMemoryFts,
        CapabilityId::RecallLegKnowledgeSemantic,
        CapabilityId::RecallLegKnowledgeFts,
        CapabilityId::RecallLegWiki,
        CapabilityId::RecallLegGraph,
        CapabilityId::SessionExtractRules,
        CapabilityId::KnowledgeIngestGraph,
        CapabilityId::DistillSession,
    ];

    /// The stable config/API id. NEVER rename one: it is a user-facing key in
    /// `policy.toml` and in `GET /api/v1/capabilities`.
    pub fn as_str(self) -> &'static str {
        match self {
            CapabilityId::MemoryInjectChat => "memory_inject_chat",
            CapabilityId::MemoryInjectRuns => "memory_inject_runs",
            CapabilityId::RecallLegMemorySemantic => "recall_leg_memory_semantic",
            CapabilityId::RecallLegMemoryFts => "recall_leg_memory_fts",
            CapabilityId::RecallLegKnowledgeSemantic => "recall_leg_knowledge_semantic",
            CapabilityId::RecallLegKnowledgeFts => "recall_leg_knowledge_fts",
            CapabilityId::RecallLegWiki => "recall_leg_wiki",
            CapabilityId::RecallLegGraph => "recall_leg_graph",
            CapabilityId::SessionExtractRules => "session_extract_rules",
            CapabilityId::KnowledgeIngestGraph => "knowledge_ingest_graph",
            CapabilityId::DistillSession => "distill_session",
        }
    }
}

impl std::str::FromStr for CapabilityId {
    type Err = CapabilityError;

    /// The ONE door from a string to an id. An unlisted name is refused and the
    /// error lists the ids it knows (L3).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CapabilityId::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| CapabilityError::UnknownId(s.to_string()))
    }
}

/// The option keys a capability may declare. A key present in the file for a
/// capability that does not declare it is a HARD ERROR naming id + key (L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKey {
    Weight,
    MinScore,
    MaxPerInput,
    MinConfidence,
    MaxDocsPerPass,
}

impl OptionKey {
    pub fn as_str(self) -> &'static str {
        match self {
            OptionKey::Weight => "weight",
            OptionKey::MinScore => "min_score",
            OptionKey::MaxPerInput => "max_per_input",
            OptionKey::MinConfidence => "min_confidence",
            OptionKey::MaxDocsPerPass => "max_docs_per_pass",
        }
    }
}

/// One capability's declared options, RESOLVED (every field is what the
/// pipeline must use; `None` means "the pipeline's own argument decides").
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize)]
pub struct CapabilityOptions {
    pub weight: Option<f64>,
    pub min_score: Option<f64>,
    pub max_per_input: Option<u32>,
    pub min_confidence: Option<f64>,
    pub max_docs_per_pass: Option<u32>,
}

impl CapabilityOptions {
    /// "Nothing declared": every field `None`. Usable in a `const` registry
    /// row, which `Default::default()` is not.
    pub const NONE: Self = Self {
        weight: None,
        min_score: None,
        max_per_input: None,
        min_confidence: None,
        max_docs_per_pass: None,
    };
}

/// The registry row. `description` is USER-FACING and is returned verbatim by
/// the API; `gates` is a stable human phrase, never a line number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapabilitySpec {
    pub id: CapabilityId,
    pub description: &'static str,
    pub tier: Tier,
    /// The registry default, i.e. what `enabled(id)` answers when the file
    /// carries no `enabled` key for this id. Chosen so that L1 holds.
    pub default_enabled: bool,
    /// false = this capability names behaviour that exists today (so its
    /// default MUST be true); true = new in this increment (default MUST be
    /// false). Asserted for every row by `registry_obeys_the_four_laws`.
    pub new_in_this_increment: bool,
    /// The option keys this capability accepts — and no others.
    pub options: &'static [OptionKey],
    pub defaults: CapabilityOptions,
    pub gates: &'static str,
}

/// The registry: 11 rows, one per `CapabilityId::ALL` entry, same order.
const SPECS: &[CapabilitySpec] = &[
    CapabilitySpec {
        id: CapabilityId::MemoryInjectChat,
        description: "Inject long-term memory, knowledge and graph evidence into a chat's first prompt.",
        tier: Tier::Free,
        default_enabled: true,
        new_in_this_increment: false,
        options: &[],
        defaults: CapabilityOptions::NONE,
        gates: "the first prompt of a chat (crates/daemon/src/chat.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::MemoryInjectRuns,
        description: "Inject long-term memory, knowledge and graph evidence into a run's prompt.",
        tier: Tier::Free,
        default_enabled: true,
        new_in_this_increment: false,
        options: &[],
        defaults: CapabilityOptions::NONE,
        gates: "the injected block of a run prompt (crates/daemon/src/runs.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::RecallLegMemorySemantic,
        description: "Recall leg: cosine similarity over memory embeddings.",
        tier: Tier::Free,
        default_enabled: true,
        new_in_this_increment: false,
        options: &[OptionKey::Weight, OptionKey::MinScore],
        defaults: CapabilityOptions {
            weight: Some(1.0),
            ..CapabilityOptions::NONE
        },
        gates: "the cosine leg of GET /api/v1/recall (crates/daemon/src/memembed.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::RecallLegMemoryFts,
        description: "Recall leg: bm25 keyword match over memory text.",
        tier: Tier::Free,
        default_enabled: true,
        new_in_this_increment: false,
        options: &[OptionKey::Weight],
        defaults: CapabilityOptions {
            weight: Some(1.0),
            ..CapabilityOptions::NONE
        },
        gates: "the bm25 leg of GET /api/v1/recall (crates/daemon/src/memembed.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::RecallLegKnowledgeSemantic,
        description: "Recall leg: vector search over knowledge chunks.",
        tier: Tier::Free,
        default_enabled: true,
        new_in_this_increment: false,
        options: &[OptionKey::Weight],
        defaults: CapabilityOptions {
            weight: Some(2.0),
            ..CapabilityOptions::NONE
        },
        gates: "the vector leg of GET /api/v1/recall (crates/knowledge/src/store.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::RecallLegKnowledgeFts,
        description: "Recall leg: bm25 keyword match over knowledge chunks.",
        tier: Tier::Free,
        default_enabled: true,
        new_in_this_increment: false,
        options: &[OptionKey::Weight],
        defaults: CapabilityOptions {
            weight: Some(1.0),
            ..CapabilityOptions::NONE
        },
        gates: "the keyword leg of GET /api/v1/recall (crates/knowledge/src/store.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::RecallLegWiki,
        description: "Recall leg: agent-generated wiki pages (leads, not ground truth).",
        tier: Tier::Free,
        default_enabled: true,
        new_in_this_increment: false,
        options: &[],
        defaults: CapabilityOptions::NONE,
        gates: "the wiki partition of GET /api/v1/recall (crates/daemon/src/wiki.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::RecallLegGraph,
        description: "Recall leg: entity seeds and multi-hop relation paths.",
        tier: Tier::Free,
        default_enabled: true,
        new_in_this_increment: false,
        options: &[],
        defaults: CapabilityOptions::NONE,
        gates: "seed + path retrieval in GET /api/v1/recall (crates/graph)",
    },
    CapabilitySpec {
        id: CapabilityId::SessionExtractRules,
        description: "Zero-token deterministic extraction of a closed session into memory candidates.",
        tier: Tier::Free,
        default_enabled: false,
        new_in_this_increment: true,
        options: &[OptionKey::MaxPerInput, OptionKey::MinConfidence],
        defaults: CapabilityOptions {
            max_per_input: Some(32),
            min_confidence: Some(0.0),
            ..CapabilityOptions::NONE
        },
        gates: "unattended session close: transcript -> memory candidates (crates/daemon/src/extract_plane.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::KnowledgeIngestGraph,
        description: "Zero-token deterministic ingestion of knowledge documents into the entity graph.",
        tier: Tier::Free,
        default_enabled: false,
        new_in_this_increment: true,
        options: &[OptionKey::MaxPerInput, OptionKey::MaxDocsPerPass],
        defaults: CapabilityOptions {
            max_per_input: Some(96),
            max_docs_per_pass: Some(20),
            ..CapabilityOptions::NONE
        },
        gates: "the knowledge scan loop: documents -> entities/relations (crates/daemon/src/knowledge_graph.rs)",
    },
    CapabilitySpec {
        id: CapabilityId::DistillSession,
        description: "ACP-agent distillation when a session closes (spends model tokens). Manual distillation is an explicit request and is never gated.",
        tier: Tier::Llm,
        default_enabled: false,
        new_in_this_increment: false,
        options: &[],
        defaults: CapabilityOptions::NONE,
        gates: "auto-distill on session close (crates/daemon/src/chat.rs)",
    },
];

/// The registry: 11 rows, one per `CapabilityId::ALL` entry, same order.
pub fn specs() -> &'static [CapabilitySpec] {
    SPECS
}

/// The row for one id. TOTAL: the id is a variant, and `SPECS` is exhaustive
/// over the enum (pinned by `registry_covers_every_id_in_order`), so there is
/// no fallback row and no way to ask about a capability that does not exist.
pub fn spec(id: CapabilityId) -> &'static CapabilitySpec {
    SPECS
        .iter()
        .find(|s| s.id == id)
        .expect("every CapabilityId has a registry row")
}

// ---------------------------------------------------------------------------
// Errors (§4.3): hard, and they name the thing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum CapabilityError {
    /// `unknown capability `foo` in [capabilities]: known ids are a, b, c, …`
    UnknownId(String),
    /// `capability `distill_session` does not accept the option `weight` (it accepts: none)`
    UnknownKey {
        id: String,
        key: String,
        accepted: String,
    },
    /// `capability `session_extract_rules`: `max_per_input = 0` is out of range (1..=10000)`
    BadValue {
        id: String,
        key: String,
        value: String,
        expectation: &'static str,
    },
}

impl std::fmt::Display for CapabilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapabilityError::UnknownId(id) => write!(
                f,
                "unknown capability `{id}` in [capabilities]: known ids are {}",
                known_ids()
            ),
            CapabilityError::UnknownKey { id, key, accepted } => write!(
                f,
                "capability `{id}` does not accept the option `{key}` (it accepts: {accepted})"
            ),
            CapabilityError::BadValue {
                id,
                key,
                value,
                expectation,
            } => write!(
                f,
                "capability `{id}`: `{key} = {value}` is out of range ({expectation})"
            ),
        }
    }
}

impl std::error::Error for CapabilityError {}

/// The ids a reader can fix a typo from (`from_policy`'s message).
fn known_ids() -> String {
    CapabilityId::ALL
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn accepted_list(s: &CapabilitySpec) -> String {
    if s.options.is_empty() {
        "none".to_string()
    } else {
        s.options
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// ---------------------------------------------------------------------------
// The plane (§4.4)
// ---------------------------------------------------------------------------

/// The live plane. Cheap to clone (a handful of entries) and shared by every
/// consumer through one `Arc<RwLock<..>>` on `ChatManager` (§4.5).
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityPlane {
    /// `None` = the `[capabilities]` table is ABSENT = legacy mode (L1).
    /// `Some(map)` = present; `map.is_empty()` is legal and means "every
    /// capability at its registry default" (which, per L4, switches every
    /// llm-tier capability OFF).
    table: Option<BTreeMap<String, ruagent_policy::CapabilityFile>>,
}

impl CapabilityPlane {
    /// Legacy mode: gates answer with today's behaviour, options are the
    /// registry defaults. This is the value `ChatManager::new` starts from, so
    /// no test harness has to opt in.
    pub fn legacy() -> Self {
        Self { table: None }
    }

    /// Build the plane from the parsed policy, validating ids, keys and ranges.
    /// This is the ONLY fallible constructor: config load and `PUT` both go
    /// through it, so a name nobody recognises can never reach a runtime
    /// accessor (L3).
    pub fn from_policy(p: &ruagent_policy::PolicyConfig) -> Result<Self, CapabilityError> {
        validate(p.capabilities.clone())
    }

    /// true when the `[capabilities]` table exists (an EMPTY table counts).
    pub fn table_present(&self) -> bool {
        self.table.is_some()
    }

    /// The configured/registry-default enable state. TOTAL: the id is an enum.
    /// With the table absent this is the registry default, so a legacy row is
    /// honest about the capability's own state (what is *effective* is
    /// [`CapabilityPlane::gate`], and what that narrows is
    /// [`CapabilityPlane::conflicts`]).
    pub fn enabled(&self, id: CapabilityId) -> bool {
        self.file_of(id)
            .and_then(|f| f.enabled)
            .unwrap_or_else(|| spec(id).default_enabled)
    }

    /// THE GATE (L2). `legacy` = what today's code would do.
    /// `gate = legacy && (!self.table_present() || self.enabled(id))`
    pub fn gate(&self, id: CapabilityId, legacy: bool) -> bool {
        legacy && (!self.table_present() || self.enabled(id))
    }

    /// The resolved options for one capability (defaults merged with the file).
    pub fn options(&self, id: CapabilityId) -> CapabilityOptions {
        let defaults = spec(id).defaults;
        let Some(f) = self.file_of(id) else {
            return defaults;
        };
        CapabilityOptions {
            weight: f.weight.or(defaults.weight),
            min_score: f.min_score.or(defaults.min_score),
            max_per_input: f.max_per_input.or(defaults.max_per_input),
            min_confidence: f.min_confidence.or(defaults.min_confidence),
            max_docs_per_pass: f.max_docs_per_pass.or(defaults.max_docs_per_pass),
        }
    }

    /// Which state the file left this capability in: `"legacy"` (no table at
    /// all), `"file"` (the file names this id), `"default"` (the table exists
    /// and this id is not in it).
    pub fn configured(&self, id: CapabilityId) -> &'static str {
        match &self.table {
            None => "legacy",
            Some(m) if m.contains_key(id.as_str()) => "file",
            Some(_) => "default",
        }
    }

    /// API/panel rows, one per registry row, in registry order.
    pub fn rows(&self) -> Vec<CapabilityRow> {
        specs()
            .iter()
            .map(|s| CapabilityRow {
                id: s.id.as_str(),
                tier: s.tier.as_str(),
                description: s.description,
                gates: s.gates,
                default_enabled: s.default_enabled,
                enabled: self.enabled(s.id),
                configured: self.configured(s.id),
                new: s.new_in_this_increment,
                options: self.options(s.id),
            })
            .collect()
    }

    /// What this configuration is NARROWING right now: one entry per
    /// (capability, legacy flag) pair whose legacy flag asks for work that the
    /// plane switches off. Legacy mode narrows nothing, so it reports nothing —
    /// the warning is about a user who turned the table on and lost behaviour
    /// they had (§5.4).
    pub fn conflicts(&self, distill_auto: bool) -> Vec<CapabilityConflict> {
        let mut out = Vec::new();
        if distill_auto && !self.gate(CapabilityId::DistillSession, true) {
            out.push(CapabilityConflict {
                id: CapabilityId::DistillSession.as_str(),
                legacy_key: "distill.auto",
                reason: "[distill] auto = true but capability `distill_session` is off: \
                         unattended distillation will not run. Add \
                         [capabilities.distill_session] enabled = true to restore it."
                    .to_string(),
            });
        }
        out
    }

    fn file_of(&self, id: CapabilityId) -> Option<&ruagent_policy::CapabilityFile> {
        self.table.as_ref().and_then(|m| m.get(id.as_str()))
    }
}

/// One row of `GET /api/v1/capabilities`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CapabilityRow {
    pub id: &'static str,
    pub tier: &'static str,
    pub description: &'static str,
    pub gates: &'static str,
    pub default_enabled: bool,
    pub enabled: bool,
    /// "legacy" (table absent) | "default" (table present, no key) | "file"
    pub configured: &'static str,
    pub new: bool,
    pub options: CapabilityOptions,
}

/// One entry of the response's `conflicts[]`: work today's flags ask for that
/// this configuration suppresses.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CapabilityConflict {
    pub id: &'static str,
    pub legacy_key: &'static str,
    pub reason: String,
}

/// The `GET`/`PUT /api/v1/capabilities` payload (design §14.1).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CapabilitiesResponse {
    pub table_present: bool,
    pub config_file: String,
    pub capabilities: Vec<CapabilityRow>,
    pub conflicts: Vec<CapabilityConflict>,
}

// ---------------------------------------------------------------------------
// Validation (§4.3, the one fallible door)
// ---------------------------------------------------------------------------

/// Validate a `[capabilities]` table exactly as the file would be validated.
/// Order is deliberate: the id is checked first, so the later messages can name
/// the capability the key or the value belongs to.
fn validate(
    table: Option<BTreeMap<String, ruagent_policy::CapabilityFile>>,
) -> Result<CapabilityPlane, CapabilityError> {
    if let Some(rows) = table.as_ref() {
        for (name, file) in rows {
            let id: CapabilityId = name.parse()?;
            let s = spec(id);
            // A key the capability does not declare is refused, never ignored:
            // `weight` on a capability with no ranked leg would look honoured
            // in the file and change nothing at runtime.
            for (key, present) in [
                (OptionKey::Weight, file.weight.is_some()),
                (OptionKey::MinScore, file.min_score.is_some()),
                (OptionKey::MaxPerInput, file.max_per_input.is_some()),
                (OptionKey::MinConfidence, file.min_confidence.is_some()),
                (OptionKey::MaxDocsPerPass, file.max_docs_per_pass.is_some()),
            ] {
                if present && !s.options.contains(&key) {
                    return Err(CapabilityError::UnknownKey {
                        id: name.clone(),
                        key: key.as_str().to_string(),
                        accepted: accepted_list(s),
                    });
                }
            }
            // Ranges, one table, one message shape. `is_finite` is first
            // because NaN is false against every bound and would otherwise
            // slip through as "not out of range".
            if let Some(v) = file.weight
                && !(v.is_finite() && (0.0..=100.0).contains(&v))
            {
                return Err(CapabilityError::BadValue {
                    id: name.clone(),
                    key: OptionKey::Weight.as_str().to_string(),
                    value: v.to_string(),
                    expectation: "finite and 0.0..=100.0",
                });
            }
            if let Some(v) = file.min_score
                && !(v.is_finite() && (0.0..=1.0).contains(&v))
            {
                return Err(CapabilityError::BadValue {
                    id: name.clone(),
                    key: OptionKey::MinScore.as_str().to_string(),
                    value: v.to_string(),
                    expectation: "finite and 0.0..=1.0",
                });
            }
            if let Some(v) = file.max_per_input
                && !(1..=10_000).contains(&v)
            {
                return Err(CapabilityError::BadValue {
                    id: name.clone(),
                    key: OptionKey::MaxPerInput.as_str().to_string(),
                    value: v.to_string(),
                    expectation: "1..=10000",
                });
            }
            if let Some(v) = file.min_confidence
                && !(v.is_finite() && (0.0..=1.0).contains(&v))
            {
                return Err(CapabilityError::BadValue {
                    id: name.clone(),
                    key: OptionKey::MinConfidence.as_str().to_string(),
                    value: v.to_string(),
                    expectation: "finite and 0.0..=1.0",
                });
            }
            if let Some(v) = file.max_docs_per_pass
                && !(1..=1000).contains(&v)
            {
                return Err(CapabilityError::BadValue {
                    id: name.clone(),
                    key: OptionKey::MaxDocsPerPass.as_str().to_string(),
                    value: v.to_string(),
                    expectation: "1..=1000",
                });
            }
        }
    }
    Ok(CapabilityPlane { table })
}

// ---------------------------------------------------------------------------
// HTTP surface (design §14.1, §14.2)
// ---------------------------------------------------------------------------
//
// WARNING, repeated here as the third of the three places the design requires
// it (module docs, `DEFAULT_POLICY_TOML`'s comment, and this doc comment): an
// EMPTY `[capabilities]` table is a PRESENT table. It applies every registry
// default, and `distill_session` (the llm tier) defaults OFF. Only the absence
// of the table means "exactly as before".

/// `PUT /api/v1/capabilities` body. The map REPLACES the whole
/// `[capabilities]` table: an id absent here goes back to its registry default,
/// and an EMPTY map removes the table entirely (back to legacy mode).
#[derive(Debug, serde::Deserialize)]
pub struct CapabilitiesPutRequest {
    /// Required to switch an `llm`-tier capability on from off: it spends the
    /// user's tokens.
    #[serde(default)]
    pub confirm_cost: bool,
    #[serde(default)]
    pub capabilities: BTreeMap<String, ruagent_policy::CapabilityFile>,
}

fn policy_path(state: &AppState) -> PathBuf {
    state.config.root.join("config").join("policy.toml")
}

/// The §14.1 payload. Both handlers answer with it, so `PUT` needs no second
/// shape and a client can re-render straight from its own response.
pub fn response(
    plane: &CapabilityPlane,
    config_file: &Path,
    distill_auto: bool,
) -> CapabilitiesResponse {
    CapabilitiesResponse {
        table_present: plane.table_present(),
        config_file: config_file.display().to_string(),
        capabilities: plane.rows(),
        conflicts: plane.conflicts(distill_auto),
    }
}

fn current_response(state: &AppState) -> CapabilitiesResponse {
    response(
        &state.chats.capabilities(),
        &policy_path(state),
        state.chats.distill_policy_now().auto,
    )
}

/// `GET /api/v1/capabilities` — every capability with its id, description,
/// tier, enabled state, resolved options and whether it differs from the
/// registry default (`configured == "file"`), plus what this configuration is
/// narrowing (design §14.1).
pub async fn list_capabilities(State(state): State<AppState>) -> Json<CapabilitiesResponse> {
    Json(current_response(&state))
}

/// The cost gate's decision, split out so it is testable without HTTP:
/// `Some(id)` = this request would switch an llm-tier capability on from off
/// without the explicit confirmation.
fn unconfirmed_llm_enable(
    before: &CapabilityPlane,
    after: &CapabilityPlane,
    confirm_cost: bool,
) -> Option<CapabilityId> {
    if confirm_cost {
        return None;
    }
    specs()
        .iter()
        .find(|s| s.tier == Tier::Llm && after.enabled(s.id) && !before.enabled(s.id))
        .map(|s| s.id)
}

/// `PUT /api/v1/capabilities` — replace the whole `[capabilities]` table
/// (design §14.2).
///
/// Order is the DistillEditor discipline, and it matters: validate the body,
/// refuse an unconfirmed cost, write the file, THEN swap the live plane. A
/// refused request therefore leaves both the file and every running pipeline
/// untouched.
pub async fn update_capabilities(
    State(state): State<AppState>,
    Json(req): Json<CapabilitiesPutRequest>,
) -> Result<Json<CapabilitiesResponse>, ApiError> {
    let before = state.chats.capabilities();
    // An empty map means legacy mode, which is the table being ABSENT — not a
    // present-and-empty one, whose defaults would switch the llm tier off.
    let table = if req.capabilities.is_empty() {
        None
    } else {
        Some(req.capabilities.clone())
    };
    // The body is validated exactly like the file would be: a body written
    // without validation would fail the NEXT boot instead of this request.
    let after = CapabilityPlane::from_policy(&ruagent_policy::PolicyConfig {
        capabilities: table,
        ..Default::default()
    })
    .map_err(|e| ApiError::bad_request(e.to_string()))?;
    if let Some(id) = unconfirmed_llm_enable(&before, &after, req.confirm_cost) {
        return Err(ApiError::conflict(format!(
            "capability `{}` is llm-tier: enabling it spends model tokens. \
             Resend with \"confirm_cost\": true.",
            id.as_str()
        )));
    }
    // The whole table is replaced: an empty map removes it -> legacy mode.
    CapabilitiesEditor::new(policy_path(&state))
        .update(&req.capabilities)
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    state.chats.set_capabilities(after);
    Ok(Json(current_response(&state)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn try_plane(text: &str) -> Result<CapabilityPlane, CapabilityError> {
        let p = ruagent_policy::PolicyConfig::parse(text).expect("policy text parses");
        CapabilityPlane::from_policy(&p)
    }

    fn plane(text: &str) -> CapabilityPlane {
        try_plane(text).expect("plane builds")
    }

    #[test]
    fn registry_covers_every_id_in_order() {
        assert_eq!(specs().len(), CapabilityId::ALL.len());
        assert_eq!(
            specs().len(),
            11,
            "the design registers eleven capabilities"
        );
        for (i, id) in CapabilityId::ALL.iter().enumerate() {
            assert_eq!(
                specs()[i].id,
                *id,
                "registry order follows CapabilityId::ALL"
            );
            assert_eq!(spec(*id).id, *id);
            assert!(!id.as_str().is_empty());
            assert_eq!(id.as_str().parse::<CapabilityId>().unwrap(), *id);
        }
        let mut names: Vec<&str> = specs().iter().map(|s| s.id.as_str()).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "ids are unique");
        // An unlisted name is refused, and it can never look like an id.
        assert!(matches!(
            "session_extract_rule".parse::<CapabilityId>(),
            Err(CapabilityError::UnknownId(_))
        ));
    }

    #[test]
    fn registry_obeys_the_four_laws() {
        for s in specs() {
            assert!(!s.description.is_empty(), "{}", s.id.as_str());
            assert!(!s.gates.is_empty(), "{}", s.id.as_str());
            assert!(
                !s.gates.contains(".rs:"),
                "`gates` is a stable phrase, never a line number: {}",
                s.id.as_str()
            );
            if s.tier == Tier::Llm {
                assert!(
                    !s.default_enabled,
                    "L4: an llm-tier capability defaults off ({})",
                    s.id.as_str()
                );
            }
            if s.new_in_this_increment {
                assert!(
                    !s.default_enabled,
                    "L4: a capability new in this increment defaults off ({})",
                    s.id.as_str()
                );
            } else if s.tier == Tier::Free {
                assert!(
                    s.default_enabled,
                    "a free capability that exists today must default on ({})",
                    s.id.as_str()
                );
            }
        }
    }

    #[test]
    fn legacy_plane_is_todays_behaviour() {
        let plane = CapabilityPlane::legacy();
        assert!(!plane.table_present());
        for id in CapabilityId::ALL {
            assert!(
                plane.gate(*id, true),
                "L1: legacy mode passes every gate through ({})",
                id.as_str()
            );
            assert!(
                !plane.gate(*id, false),
                "L2: and never starts work the legacy flag did not ask for ({})",
                id.as_str()
            );
            assert_eq!(plane.options(*id), spec(*id).defaults);
            assert_eq!(plane.enabled(*id), spec(*id).default_enabled);
            assert_eq!(plane.configured(*id), "legacy");
        }
        assert!(plane.conflicts(true).is_empty(), "legacy narrows nothing");
    }

    #[test]
    fn single_key_override_narrows_only_that_capability() {
        let plane = plane("[capabilities.recall_leg_wiki]\nenabled = false\n");
        assert!(plane.table_present());
        assert!(!plane.gate(CapabilityId::RecallLegWiki, true));
        assert_eq!(plane.configured(CapabilityId::RecallLegWiki), "file");
        assert_eq!(
            plane.options(CapabilityId::RecallLegWiki),
            CapabilityOptions::NONE
        );
        for id in CapabilityId::ALL {
            if *id == CapabilityId::RecallLegWiki {
                continue;
            }
            assert_eq!(plane.configured(*id), "default", "{}", id.as_str());
            assert_eq!(plane.enabled(*id), spec(*id).default_enabled);
            assert_eq!(plane.gate(*id, true), spec(*id).default_enabled);
        }
    }

    #[test]
    fn empty_table_is_present_and_switches_the_llm_tier_off() {
        let plane = plane("[capabilities]\n");
        assert!(plane.table_present(), "an empty header is a PRESENT table");
        assert!(
            !plane.gate(CapabilityId::DistillSession, true),
            "L4: the llm tier is off the moment the table exists"
        );
        assert!(
            plane.gate(CapabilityId::MemoryInjectChat, true),
            "a free capability that exists today stays on"
        );
        assert!(
            !plane.gate(CapabilityId::SessionExtractRules, true),
            "a new free capability stays off"
        );
    }

    #[test]
    fn unknown_id_is_refused_and_lists_the_known_ids() {
        let err = try_plane("[capabilities.session_extract_rule]\nenabled = true\n").unwrap_err();
        assert_eq!(
            err,
            CapabilityError::UnknownId("session_extract_rule".into())
        );
        let msg = err.to_string();
        assert!(msg.contains("session_extract_rule"), "{msg}");
        assert!(
            msg.contains("session_extract_rules"),
            "lists the known ids: {msg}"
        );
        assert!(msg.contains("distill_session"), "lists all of them: {msg}");
    }

    #[test]
    fn undeclared_option_key_is_refused() {
        let err = try_plane("[capabilities.distill_session]\nweight = 1.0\n").unwrap_err();
        assert_eq!(
            err,
            CapabilityError::UnknownKey {
                id: "distill_session".into(),
                key: "weight".into(),
                accepted: "none".into(),
            }
        );
        assert!(
            err.to_string()
                .contains("does not accept the option `weight`")
        );
        // A declared key of ANOTHER capability is still undeclared here, and
        // the message says which ones this one does accept.
        let err = try_plane("[capabilities.recall_leg_memory_semantic]\nmax_per_input = 8\n")
            .unwrap_err();
        assert_eq!(
            err,
            CapabilityError::UnknownKey {
                id: "recall_leg_memory_semantic".into(),
                key: "max_per_input".into(),
                accepted: "weight, min_score".into(),
            }
        );
    }

    #[test]
    fn out_of_range_values_are_refused() {
        let err =
            try_plane("[capabilities.session_extract_rules]\nmax_per_input = 0\n").unwrap_err();
        assert_eq!(
            err,
            CapabilityError::BadValue {
                id: "session_extract_rules".into(),
                key: "max_per_input".into(),
                value: "0".into(),
                expectation: "1..=10000",
            }
        );
        assert!(err.to_string().contains("1..=10000"), "{err}");
        assert!(matches!(
            try_plane("[capabilities.recall_leg_memory_semantic]\nmin_score = 1.5\n"),
            Err(CapabilityError::BadValue { .. })
        ));
        assert!(matches!(
            try_plane("[capabilities.knowledge_ingest_graph]\nmax_docs_per_pass = 1001\n"),
            Err(CapabilityError::BadValue { .. })
        ));
        assert!(matches!(
            try_plane("[capabilities.session_extract_rules]\nmin_confidence = -0.1\n"),
            Err(CapabilityError::BadValue { .. })
        ));
        // NaN is inside no range and must not pass a naive bound check.
        assert!(matches!(
            try_plane("[capabilities.recall_leg_memory_fts]\nweight = nan\n"),
            Err(CapabilityError::BadValue { .. })
        ));
        // The boundary values themselves are legal.
        assert!(
            try_plane(
                "[capabilities.recall_leg_memory_fts]\nweight = 0.0\n\
                 [capabilities.recall_leg_memory_semantic]\nmin_score = 1.0\n\
                 [capabilities.session_extract_rules]\nmax_per_input = 10000\nmin_confidence = 0.0\n"
            )
            .is_ok()
        );
    }

    #[test]
    fn options_merge_the_file_over_the_defaults() {
        let plane = plane(
            "[capabilities.recall_leg_knowledge_semantic]\nweight = 3.5\n\
             [capabilities.session_extract_rules]\nenabled = true\nmax_per_input = 4\n",
        );
        assert_eq!(
            plane
                .options(CapabilityId::RecallLegKnowledgeSemantic)
                .weight,
            Some(3.5)
        );
        assert_eq!(
            plane.options(CapabilityId::RecallLegKnowledgeFts).weight,
            Some(1.0),
            "an untouched leg keeps its registry default"
        );
        let rules = plane.options(CapabilityId::SessionExtractRules);
        assert_eq!(rules.max_per_input, Some(4));
        assert_eq!(
            rules.min_confidence,
            Some(0.0),
            "an absent key keeps the declared default"
        );
        assert_eq!(
            plane.options(CapabilityId::RecallLegGraph),
            CapabilityOptions::NONE
        );
    }

    #[test]
    fn conflicts_only_name_work_that_is_actually_narrowed() {
        assert!(CapabilityPlane::legacy().conflicts(true).is_empty());
        let unrelated = plane("[capabilities.recall_leg_wiki]\nenabled = false\n");
        assert!(
            unrelated.conflicts(false).is_empty(),
            "[distill].auto is off: nothing is being narrowed"
        );
        let c = unrelated.conflicts(true);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].id, "distill_session");
        assert_eq!(c[0].legacy_key, "distill.auto");
        assert!(c[0].reason.contains("distill_session"), "{}", c[0].reason);
        assert!(c[0].reason.contains("unattended distillation"));
        let restored = plane("[capabilities.distill_session]\nenabled = true\n");
        assert!(
            restored.conflicts(true).is_empty(),
            "an explicitly restored capability is not a conflict"
        );
    }

    #[test]
    fn cost_gate_refuses_an_unconfirmed_llm_enable() {
        let before = CapabilityPlane::legacy();
        let on = plane("[capabilities.distill_session]\nenabled = true\n");
        assert_eq!(
            unconfirmed_llm_enable(&before, &on, false),
            Some(CapabilityId::DistillSession)
        );
        assert_eq!(unconfirmed_llm_enable(&before, &on, true), None);
        assert_eq!(
            unconfirmed_llm_enable(&on, &on, false),
            None,
            "already on: the request spends nothing new"
        );
        let free = plane("[capabilities.session_extract_rules]\nenabled = true\n");
        assert_eq!(
            unconfirmed_llm_enable(&before, &free, false),
            None,
            "a free capability never asks for confirmation"
        );
    }

    #[test]
    fn put_request_parses_the_documented_body() {
        let req: CapabilitiesPutRequest = serde_json::from_value(serde_json::json!({
            "confirm_cost": true,
            "capabilities": {
                "recall_leg_wiki": { "enabled": false },
                "distill_session": { "enabled": true }
            }
        }))
        .expect("body parses");
        assert!(req.confirm_cost);
        assert_eq!(req.capabilities.len(), 2);
        assert_eq!(req.capabilities["recall_leg_wiki"].enabled, Some(false));
        assert_eq!(req.capabilities["distill_session"].enabled, Some(true));
        // `{}` is legal and means "remove the table": legacy mode.
        let empty: CapabilitiesPutRequest =
            serde_json::from_value(serde_json::json!({})).expect("empty body parses");
        assert!(empty.capabilities.is_empty());
        assert!(!empty.confirm_cost);
    }

    #[test]
    fn row_and_response_json_are_the_api_contract() {
        let plane = plane("[capabilities.distill_session]\nenabled = true\n");
        let rows = plane.rows();
        assert_eq!(rows.len(), CapabilityId::ALL.len());
        assert_eq!(rows[0].id, "memory_inject_chat", "registry order");
        let json = serde_json::to_value(&rows).expect("rows serialize");
        let rows = json.as_array().expect("array");
        let row = rows
            .iter()
            .find(|r| r["id"] == "distill_session")
            .expect("the llm row");
        assert_eq!(row["tier"], "llm");
        assert_eq!(row["enabled"], true);
        assert_eq!(row["default_enabled"], false);
        assert_eq!(row["configured"], "file");
        assert_eq!(row["new"], false);
        assert!(
            row["description"]
                .as_str()
                .is_some_and(|d| d.contains("distillation"))
        );
        assert!(
            row["gates"]
                .as_str()
                .is_some_and(|g| g.contains("auto-distill"))
        );
        assert_eq!(row["options"]["weight"], serde_json::Value::Null);
        assert_eq!(row["options"]["max_docs_per_pass"], serde_json::Value::Null);
        let wiki = rows
            .iter()
            .find(|r| r["id"] == "recall_leg_wiki")
            .expect("the wiki row");
        assert_eq!(wiki["configured"], "default");
        assert_eq!(wiki["enabled"], true);

        let resp = response(&plane, Path::new("/tmp/policy.toml"), true);
        let v = serde_json::to_value(&resp).expect("response serializes");
        assert_eq!(v["table_present"], true);
        assert_eq!(v["config_file"], "/tmp/policy.toml");
        assert_eq!(v["capabilities"].as_array().map(Vec::len), Some(11));
        assert_eq!(v["conflicts"], serde_json::json!([]));
    }
}
