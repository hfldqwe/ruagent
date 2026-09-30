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
//!
//! DECLARED vs RESOLVED vs IN-THE-FILE (increment 4, design §11.5). A row answers
//! three different questions and this module keeps them in three different fields:
//! `options` is the RESOLVED value (registry default merged with the file — what
//! the pipeline uses), `options_schema` is what the REGISTRY DECLARES (every key
//! it accepts, with the daemon's own bounds and expectation phrase) and
//! `options_set` is which keys the FILE carries right now. The first is what three
//! callers already parse (the panel, the MCP list, the MCP write's re-emit), so it
//! is NOT redefined; the other two are additive. The split is not cosmetic: a
//! declared key whose registry default is `None` appears in NO resolved value, and
//! `configured` says the row's id is in the file, not WHICH KEYS are — so without
//! `options_schema` such a key is unreachable from every surface, and without
//! `options_set` a read-modify-write client re-emits resolved defaults and writes
//! them into the user's `policy.toml`.

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
    /// Every declared option key, in registry order. This is the order
    /// `options_schema` reports keys in, and the order `validate` refuses them in
    /// — the FIRST offending key is the one a user is told about, so the order is
    /// load-bearing and the two loops must stay in step.
    pub const ALL: &'static [OptionKey] = &[
        OptionKey::Weight,
        OptionKey::MinScore,
        OptionKey::MaxPerInput,
        OptionKey::MinConfidence,
        OptionKey::MaxDocsPerPass,
    ];

    /// The stable config/API id. NEVER rename one: it is a user-facing key in
    /// `policy.toml` and in `options_set` / `options_schema`.
    pub fn as_str(self) -> &'static str {
        match self {
            OptionKey::Weight => "weight",
            OptionKey::MinScore => "min_score",
            OptionKey::MaxPerInput => "max_per_input",
            OptionKey::MinConfidence => "min_confidence",
            OptionKey::MaxDocsPerPass => "max_docs_per_pass",
        }
    }

    /// The JSON kind of this key's value in [`ruagent_policy::CapabilityFile`]:
    /// `"float"` (an `Option<f64>` field) or `"uint"` (an `Option<u32>` field).
    /// The panel needs it to know whether an INTEGER is required — the bounds
    /// alone cannot say.
    pub fn kind(self) -> &'static str {
        match self {
            OptionKey::Weight | OptionKey::MinScore | OptionKey::MinConfidence => "float",
            OptionKey::MaxPerInput | OptionKey::MaxDocsPerPass => "uint",
        }
    }

    /// The accepted range, in ONE place: `options_schema` publishes it as
    /// `min`/`max` and `validate` enforces exactly it, so the range a panel shows
    /// cannot be wider or narrower than the range the daemon accepts. For a
    /// `uint` key the bounds are integral, which is what keeps `min - 1` a legal
    /// probe on both kinds.
    pub fn bounds(self) -> (f64, f64) {
        match self {
            OptionKey::Weight => (0.0, 100.0),
            OptionKey::MinScore | OptionKey::MinConfidence => (0.0, 1.0),
            OptionKey::MaxPerInput => (1.0, 10_000.0),
            OptionKey::MaxDocsPerPass => (1.0, 1_000.0),
        }
    }

    /// The daemon's OWN phrase for the accepted range — the exact string a 400
    /// already carries ([`CapabilityError::BadValue::expectation`]), surfaced by
    /// `options_schema` instead of being paraphrased. That is the point: the panel
    /// shows the daemon's words and a second wording cannot drift from the first.
    pub fn expectation(self) -> &'static str {
        match self {
            OptionKey::Weight => "finite and 0.0..=100.0",
            OptionKey::MinScore | OptionKey::MinConfidence => "finite and 0.0..=1.0",
            OptionKey::MaxPerInput => "1..=10000",
            OptionKey::MaxDocsPerPass => "1..=1000",
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

/// One entry of a row's `options_schema`: what the REGISTRY DECLARES for one
/// option key, generated from `spec(id).options` + `spec(id).defaults` (design
/// §11.5).
///
/// WHY IT EXISTS (design §11.5, defect 1). The API reports RESOLVED values, and a
/// client renders an input per key whose resolved value is non-null, so a declared
/// key whose registry default is `None` renders NO input anywhere and can only be
/// set by hand-editing `policy.toml`. Today that is exactly one key —
/// `min_score` on `recall_leg_memory_semantic` — and it is live: the recall handler
/// reads it (`api.rs`, `.map(|v| v as f32).unwrap_or(...)`). The schema is what
/// makes every DECLARED key reachable without changing what `options` means.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OptionSchemaEntry {
    pub key: &'static str,
    /// `"float"` | `"uint"` ([`OptionKey::kind`]) — whether an integer is required.
    pub kind: &'static str,
    /// The accepted range, from the same table `validate` enforces.
    pub min: f64,
    pub max: f64,
    /// The registry's declared default — `null` ONLY where the registry declares
    /// none ([`CapabilityOptions`]'s field is `None`, the pipeline's own argument
    /// decides). A declared ZERO is `0`, never `null`.
    pub default: Option<f64>,
    /// The daemon's own accepted-range phrase ([`OptionKey::expectation`]), i.e.
    /// the string a 400 already carries.
    pub expectation: &'static str,
}

/// One row's `options_set`: for `enabled` and for every key the capability
/// DECLARES ([`OptionKey::ALL`] filtered by the spec), whether the FILE carries
/// that key right now (design §11.5).
///
/// WHY IT EXISTS (design §11.5, defect 2). `configured` says the row's ID is in the
/// file, not WHICH KEYS are, and `options` cannot tell a key the file carries from
/// one that is merely defaulted. A read-modify-write client that re-emits `options`
/// therefore writes registry DEFAULTS into the user's file: measured live on
/// `a0f1eae`, `enabled = true` (the row shape a Reset leaves) became
/// `enabled = true` + `weight = 1` on the next unrelated write, which is a PERSISTED
/// rewrite of the user's configuration rather than a display artefact.
///
/// The source of every `true` here is the plane's own
/// [`ruagent_policy::CapabilityFile`] for the id — the same value `configured` is
/// derived from — never the resolved config. `enabled` is in this object because it
/// has the SAME present-or-defaulted ambiguity as an option value.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OptionSet {
    /// true = the file carries `enabled` for this id.
    pub enabled: bool,
    /// One entry per DECLARED key of this capability, in registry order.
    #[serde(flatten)]
    pub keys: BTreeMap<&'static str, bool>,
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

/// What the REGISTRY DECLARES for one capability: one entry per declared key, in
/// registry order (design §11.5). A row that declares nothing reports `[]` — its
/// "no editor at all" behaviour is preserved rather than replaced by an empty
/// editor claiming a knob.
///
/// The bounds and the phrase come from `OptionKey` (`bounds`/`expectation`), i.e.
/// from the same table `validate` enforces, and the default from the registry's
/// own `defaults` — never from the resolved values of a particular plane.
fn options_schema(s: &CapabilitySpec) -> Vec<OptionSchemaEntry> {
    s.options
        .iter()
        .map(|k| OptionSchemaEntry {
            key: k.as_str(),
            kind: k.kind(),
            min: k.bounds().0,
            max: k.bounds().1,
            default: declared_default(s.defaults, *k),
            expectation: k.expectation(),
        })
        .collect()
}

/// The registry's declared default for one key: `None` ONLY where the registry
/// leaves the key unset. That is the one key class this surface exists for — the
/// design measures exactly one such key today (`min_score` on
/// `recall_leg_memory_semantic`), and the count is pinned by a test so a new
/// no-default key cannot arrive unnoticed.
fn declared_default(defaults: CapabilityOptions, key: OptionKey) -> Option<f64> {
    match key {
        OptionKey::Weight => defaults.weight,
        OptionKey::MinScore => defaults.min_score,
        OptionKey::MaxPerInput => defaults.max_per_input.map(f64::from),
        OptionKey::MinConfidence => defaults.min_confidence,
        OptionKey::MaxDocsPerPass => defaults.max_docs_per_pass.map(f64::from),
    }
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

    /// Which of a row's DECLARED keys the FILE carries right now, plus `enabled`
    /// (design §11.5). The source is the plane's own
    /// [`ruagent_policy::CapabilityFile`] for the id — the same value
    /// [`CapabilityPlane::configured`] is derived from — never the resolved
    /// config: `configured == "file"` says the id is in the table, and this says
    /// which of its keys are. With the table ABSENT every entry is `false`,
    /// `enabled` included.
    pub fn options_set(&self, id: CapabilityId) -> OptionSet {
        let file = self.file_of(id);
        OptionSet {
            enabled: file.and_then(|f| f.enabled).is_some(),
            keys: spec(id)
                .options
                .iter()
                .map(|k| (k.as_str(), file.is_some_and(|f| key_in_file(f, *k))))
                .collect(),
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
                options_schema: options_schema(s),
                options_set: self.options_set(s.id),
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
    /// "legacy" (table absent) | "default" (table present, no key) | "file".
    /// NOT WIDENED by increment 4: its three values and its meaning are unchanged.
    pub configured: &'static str,
    pub new: bool,
    /// The RESOLVED options (registry defaults merged with the file) — DO NOT
    /// REDEFINE THIS FIELD, and the reason is in the code rather than in a review
    /// comment: three callers already parse `options` as resolved values — the
    /// panel's editor (`panel/src/capability-options.ts`), the MCP list
    /// (`crates/mcp/src/lib.rs`, which renders its text from these values) and the
    /// MCP write's re-emit, whose READ-MODIFY-WRITE LOOP is the only thing keeping
    /// one toggle from resetting every other configured row. Redefining the field
    /// that loop re-emits through would break the write path of a surface that was
    /// just verified, so the declared schema is a SECOND field
    /// ([`Self::options_schema`]) and not a richer `options`: two additive fields
    /// cost a client that ignores them exactly nothing (design §11.5).
    pub options: CapabilityOptions,
    /// ADDITIVE (increment 4, design §11.5): one entry per key the capability
    /// DECLARES, in registry order, with the daemon's own bounds and expectation
    /// phrase. `[]` for a row that declares nothing. This is what makes a declared
    /// key with no registry default (no resolved value to render) reachable.
    pub options_schema: Vec<OptionSchemaEntry>,
    /// ADDITIVE (increment 4, design §11.5): which of those keys (plus `enabled`)
    /// the FILE carries right now — the fact a read-modify-write client needs in
    /// order to stop writing registry defaults into the user's file.
    pub options_set: OptionSet,
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

/// Whether the FILE carries this key at all. The file-side half of
/// [`CapabilityPlane::options_set`] and the same question `validate` asks before
/// refusing a key the capability does not declare — one mapping, so the two
/// cannot disagree about which field a key name means.
fn key_in_file(file: &ruagent_policy::CapabilityFile, key: OptionKey) -> bool {
    key_value(file, key).is_some()
}

/// The file's value for one declared key, in `f64` form so the range check is one
/// expression per key instead of one block per key (`u32` -> `f64` is exact for
/// every `u32`, so no bound is distorted). `None` = the file does not carry it.
fn key_value(file: &ruagent_policy::CapabilityFile, key: OptionKey) -> Option<f64> {
    match key {
        OptionKey::Weight => file.weight,
        OptionKey::MinScore => file.min_score,
        OptionKey::MaxPerInput => file.max_per_input.map(f64::from),
        OptionKey::MinConfidence => file.min_confidence,
        OptionKey::MaxDocsPerPass => file.max_docs_per_pass.map(f64::from),
    }
}

/// Validate a `[capabilities]` table exactly as the file would be validated.
///
/// ORDER, stated per ROW because that is what the code has — and it is the order
/// this surface has always had, not something this refactor introduced: the id is
/// checked first (so the later messages can name the capability the key or the
/// value belongs to), then every key this row carries, then every value in it. Rows
/// are visited in the table's own order (`BTreeMap`, i.e. by id). WITHIN the row
/// that is reported, an undeclared key always beats an out-of-range value, because
/// the key loop runs to completion before the value loop.
///
/// It is NOT a global ordering across rows, and an earlier revision of this comment
/// claimed it was ("an undeclared key is always reported before an out-of-range
/// value, whichever row carries it"). That was false when written and is false
/// here — the FIRST row with anything wrong is the row a user hears about
/// (measured, increment-4 review and again for this repair:
/// `[capabilities.knowledge_ingest_graph] max_per_input = 0` plus
/// `[capabilities.session_extract_rules] weight = 1.0` reports the `BadValue` for
/// the first row, while swapping the two rows reports the `UnknownKey` for the
/// first). `from_policy` reports ONE problem — the first one — not all of them, so
/// a user with two malformed rows fixes them one at a time.
///
/// The repair for that false sentence is THIS comment, not a reordering of the
/// loops: moving every key check ahead of every value check would be a behaviour
/// change to a surface three callers read, and this increment's law was
/// additive-only.
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
            for key in OptionKey::ALL {
                if key_in_file(file, *key) && !s.options.contains(key) {
                    return Err(CapabilityError::UnknownKey {
                        id: name.clone(),
                        key: key.as_str().to_string(),
                        accepted: accepted_list(s),
                    });
                }
            }
            // Ranges, one table, one message shape — the range and the phrase both
            // come from `OptionKey`, the SAME table `options_schema` publishes, so
            // the range a panel shows is the range this refuses on. `is_finite` is
            // checked first because NaN is false against every bound and would
            // otherwise slip through as "not out of range".
            for key in OptionKey::ALL {
                let Some(v) = key_value(file, *key) else {
                    continue;
                };
                let (min, max) = key.bounds();
                if !(v.is_finite() && (min..=max).contains(&v)) {
                    return Err(CapabilityError::BadValue {
                        id: name.clone(),
                        key: key.as_str().to_string(),
                        value: v.to_string(),
                        expectation: key.expectation(),
                    });
                }
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

/// Does the RECALL RUNTIME refuse this plane because of THIS capability — and with
/// what sentence? Asked through the runtime's own composition,
/// [`crate::api::recall_legs`] (= `RecallLegConfig::resolve`, the call the recall
/// handler makes), on a plane whose ONLY configured row is this capability, so the
/// answer can only be about it.
///
/// `None` = the runtime accepts it. That covers three shapes, and none of them is
/// decided here: the capability declares no `weight` at all (the daemon's other nine
/// rows, `recall_leg_wiki` and `recall_leg_graph` among them); the runtime does not
/// weigh it (`RecallLegConfig` carries wiki and graph as plain bools, so
/// `check_weights` never sees them); or the leg is DISABLED, and the runtime ignores a
/// disabled leg's weight (clause 3 of `check_weights` — that is what `enabled = false`
/// is for).
///
/// The probe carries the plane's OWN readings, so the runtime does its own
/// `Option<f64> -> f32` conversion and applies its own default: the door re-implements
/// neither. Every other row of the probe is ABSENT, i.e. at its registry default,
/// which is a value the runtime accepts for every weighted row — so no other row can
/// produce this answer.
fn runtime_refusal(plane: &CapabilityPlane, id: CapabilityId) -> Option<String> {
    let file = ruagent_policy::CapabilityFile {
        enabled: Some(plane.gate(id, true)),
        weight: plane.options(id).weight,
        ..Default::default()
    };
    // Built structurally rather than through `from_policy`: both values are readings of
    // a plane that already validated (the registry default in between is valid by
    // construction), and this probe is an ARGUMENT SOURCE for the runtime — it is never
    // written to a file and never installed anywhere.
    let probe = CapabilityPlane {
        table: Some(BTreeMap::from([(id.as_str().to_string(), file)])),
    };
    crate::api::recall_legs(&probe)
        .err()
        .map(crate::api::leg_config_error)
}

/// THE WRITE DOOR for the leg-weight invariant (design §11.5; increment 4 bounded it
/// to the MEMORY legs, increment 5 REMOVED that bound). `Some(message)` = this request
/// would newly leave a leg in a state the runtime refuses.
///
/// THE INVARIANT. `check_weights` (`crates/knowledge/src/rrf.rs`) refuses an ENABLED
/// leg at weight `<= 0.0`, and the recall handler turns that into a 400 naming the leg.
/// The negative half never reaches this door — `validate` already refuses a weight
/// below `0.0` — so the state this checks is exactly `weight == 0.0` with the leg
/// enabled. `CapabilityPlane::from_policy` deliberately does NOT enforce it: doing so
/// would stop a `policy.toml` that boots today from booting at all, and a daemon that
/// refuses to start cannot show the user which line to fix.
///
/// THE COVERAGE IS THE RUNTIME'S, AND IT IS DISCOVERED RATHER THAN LISTED. For every id
/// the registry knows, the door asks the runtime ([`runtime_refusal`]) whether this
/// plane would put THAT capability in a state the runtime refuses, and whether the
/// previous plane already did. There is no leg list here, no leg LABEL and no copy of
/// the rule: `MemoryLegs::validate` and `LegConfig::validate` are what decide which legs
/// carry a weight — measured over HTTP and in this module: `memory semantic`,
/// `memory fts`, `knowledge semantic`, `knowledge keyword`, exactly the four rows that
/// declare a `weight` — and a leg added to the runtime later is covered the moment it
/// exists, without this file changing, PROVIDED its reading reaches `recall_legs`
/// (crate::api), which is the one piece of leg WIRING the daemon owns: that is the
/// argument list of the runtime's own resolver, it is arity-checked by the compiler,
/// and a weighted leg that is declared but left unwired turns
/// `the_write_door_covers_exactly_the_legs_the_runtime_refuses` RED rather than
/// leaving a silent gap. The equality is asserted per capability id
/// against `recall_legs` by `the_write_door_covers_exactly_the_legs_the_runtime_refuses`,
/// rather than against a second list.
///
/// BEFORE/AFTER, the [`unconfirmed_llm_enable`] shape: the door fires on the
/// TRANSITION (the state is new to this request), not on the resulting state. A file
/// that ALREADY carries a refused leg still boots, its read path is untouched (recall
/// still 400s, loudly, naming the leg), AND it stays editable: refusing every write
/// that merely carries the state forward would brick the panel of exactly the file the
/// boot refusal was rejected for.
///
/// BEHAVIOUR CHANGE, recorded as one (a change, not a bug fix). Increment 4 shipped
/// this door for the MEMORY pair only, which left the two surfaces disagreeing: the
/// panel's own guard refuses an enabled `weight = 0` leg on EVERY row that declares a
/// weight (`panel/src/capability-options.ts`, `zeroWeightEnabled`) — the knowledge legs
/// included — while the door accepted them. So the panel could refuse a state the
/// daemon's own door would have taken, and a client that did not carry that guard could
/// persist one.
/// * BEFORE (`4677912`, increment 4): `PUT {"recall_leg_knowledge_semantic":
///   {"enabled": true, "weight": 0.0}}` answered **200** and wrote it; the NEXT
///   `GET /api/v1/recall` answered **400**. The configuration was accepted by the door
///   that owns the file and rejected by the path that reads it.
/// * AFTER: the same `PUT` answers **400** with the recall path's own sentence
///   (`leg \`knowledge semantic\` is enabled with weight 0: … disable the leg instead
///   of zeroing it`), and the file is byte-identical. Load and read are unchanged.
///
/// BOUNDED TO WHAT WAS MEASURED: the four legs named above are refused at the door and
/// wiki/graph are not (they have no weight to refuse); a DISABLED leg at any weight is
/// never refused; and a plane that merely carries a refused row forward is accepted, so
/// such a file stays editable. The panel and this door now enumerate the same set — the
/// rows that declare a `weight`, which the equality test pins to the runtime's own
/// weighed legs.
///
/// The SENTENCE is not spelled in this file: it is [`WeightError`]'s own `Display`
/// under the prefix [`crate::api::leg_config_error`] builds, which the recall handler
/// uses too, so the door and the read path answer with one sentence and neither can
/// drift from the other.
fn zero_weight_leg(before: &CapabilityPlane, after: &CapabilityPlane) -> Option<String> {
    // Registry order, so which leg a request hears about first is deterministic (and
    // matches the runtime's own memory-then-knowledge order for a plane that breaks
    // one leg in each pair).
    CapabilityId::ALL.iter().find_map(|id| {
        let refusal = runtime_refusal(after, *id)?;
        runtime_refusal(before, *id).is_none().then_some(refusal)
    })
}

/// `PUT /api/v1/capabilities` — replace the whole `[capabilities]` table
/// (design §14.2).
///
/// Order is the DistillEditor discipline, and it matters: validate the body,
/// refuse a configuration the recall path is guaranteed to reject, refuse an
/// unconfirmed cost, write the file, THEN swap the live plane. A refused request
/// therefore leaves both the file and every running pipeline untouched.
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
    // The cross-field invariant the recall path enforces (§11.5): a state the read
    // path is GUARANTEED to reject is not accepted here either. This is a
    // deliberate behaviour change — before it, this PUT answered 200 and wrote a
    // configuration whose next read answered 400; see `zero_weight_leg` for the
    // before/after and for why it fires on the transition rather than on the
    // resulting state.
    if let Some(message) = zero_weight_leg(&before, &after) {
        return Err(ApiError::bad_request(message));
    }
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

    /// `[capabilities.<id>]` with ONE key set — the smallest file that exercises one
    /// declared key's range. The literal is spelled in the key's own kind, because
    /// the two directions of "wrong kind" are NOT symmetric (measured, t15; the
    /// sentence this comment used to carry called the first one a parse error and
    /// was false):
    ///
    /// * an INTEGER literal in a FLOAT key PARSES: `weight = 2` boots and resolves
    ///   to `2.0`. The asymmetry is on the WRITE side — the editor re-emits the key
    ///   from an `f64` (`CapabilitiesEditor::update` -> `set_or_remove_f64` ->
    ///   `toml_edit::value(f64)`), so a write that re-emits such a row NORMALIZES
    ///   the literal: `weight = 2` becomes `weight = 2.0` in the file. The VALUE is
    ///   unchanged; only the bytes are. Nothing in this increment promises a
    ///   byte-identical round trip of a hand-spelled integer.
    /// * a FLOAT literal in a `uint` key IS REFUSED, before any write: the row does
    ///   not deserialize into `Option<u32>`, so `max_per_input = 2.5` fails the
    ///   parse ("invalid type: floating point `2.5`, expected u32") — the daemon
    ///   does not boot on it and a `PUT` carrying it never reaches the file.
    ///
    /// So a `uint` boundary probe MUST spell an integer, while a float key accepts
    /// either spelling; this helper uses the float spelling (`{v:?}`) so the literal
    /// is the value a schema entry publishes.
    fn one_key(id: &str, key: OptionKey, v: f64) -> String {
        let literal = if key.kind() == "uint" {
            format!("{}", v as u32)
        } else {
            format!("{v:?}")
        };
        format!("[capabilities.{id}]\n{} = {literal}\n", key.as_str())
    }

    /// THE DECLARED SCHEMA IS THE DAEMON'S OWN TABLE (design §11.5). For every key
    /// of every registry row: the entry's `min`/`max` are the bounds `validate`
    /// ACTUALLY enforces (both bounds accepted, one step outside refused) and its
    /// `expectation` is the phrase that 400 carries — so a panel that renders the
    /// schema shows the range and the wording the daemon refuses on, and neither is
    /// a second copy that could drift.
    #[test]
    fn the_declared_schema_is_the_range_validation_enforces() {
        let mut declared = 0;
        for s in specs() {
            let schema = options_schema(s);
            assert_eq!(
                schema.len(),
                s.options.len(),
                "{}: one entry per declared key",
                s.id.as_str()
            );
            for (entry, key) in schema.iter().zip(s.options) {
                declared += 1;
                assert_eq!(entry.key, key.as_str(), "registry order");
                assert!(
                    matches!(entry.kind, "float" | "uint"),
                    "{}: kind is one of the two JSON kinds, not `{}`",
                    entry.key,
                    entry.kind
                );
                assert_eq!(entry.expectation, key.expectation());
                assert_eq!(entry.default, declared_default(s.defaults, *key));
                for (side, v) in [("min", entry.min), ("max", entry.max)] {
                    assert!(
                        try_plane(&one_key(s.id.as_str(), *key, v)).is_ok(),
                        "{}={v} is the declared {side} and must be accepted",
                        entry.key
                    );
                }
                // `min - 1` is a legal probe on both kinds because a uint key's
                // declared bounds are integral (`OptionKey::bounds`).
                let below = if entry.kind == "uint" {
                    entry.min - 1.0
                } else {
                    entry.min - 0.1
                };
                for (side, v) in [("below min", below), ("above max", entry.max + 1.0)] {
                    match try_plane(&one_key(s.id.as_str(), *key, v)) {
                        Err(CapabilityError::BadValue {
                            key: refused,
                            expectation,
                            ..
                        }) => {
                            assert_eq!(refused, entry.key, "{side}");
                            assert_eq!(
                                expectation, entry.expectation,
                                "{side} {v}: the 400 and the schema carry ONE phrase"
                            );
                        }
                        other => panic!(
                            "{side}: {} = {v} was not refused with the range error: {other:?}",
                            entry.key
                        ),
                    }
                }
            }
        }
        assert!(declared > 0, "the registry declares option keys");
    }

    /// THE ADDITIVE LAW of increment 4, pinned at the JSON boundary: the row's
    /// pre-existing keys keep their names and their JSON types, `options` is still
    /// the RESOLVED-values object, `configured` still carries its own string — and
    /// `options_schema`/`options_set` are the ONLY additions. `options` may not be
    /// redefined: three callers parse it as resolved values, including the MCP
    /// write's read-modify-write loop (see `CapabilityRow`'s field docs).
    #[test]
    fn options_schema_and_options_set_are_the_only_new_row_fields() {
        let json = serde_json::to_value(CapabilityPlane::legacy().rows()).expect("rows serialize");
        let rows = json.as_array().expect("array");
        assert_eq!(rows.len(), CapabilityId::ALL.len());
        for row in rows {
            let mut keys: Vec<&str> = row
                .as_object()
                .expect("a row is an object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                [
                    "configured",
                    "default_enabled",
                    "description",
                    "enabled",
                    "gates",
                    "id",
                    "new",
                    "options",
                    "options_schema",
                    "options_set",
                    "tier",
                ],
                "the row's key set, additions included: {row}"
            );
            for pre_existing in ["id", "tier", "description", "gates", "configured"] {
                assert!(
                    row[pre_existing].is_string(),
                    "{pre_existing} keeps its type: {row}"
                );
            }
            for pre_existing in ["default_enabled", "enabled", "new"] {
                assert!(
                    row[pre_existing].is_boolean(),
                    "{pre_existing} keeps its type: {row}"
                );
            }
            let mut options: Vec<&str> = row["options"]
                .as_object()
                .expect("options is the resolved-values object")
                .keys()
                .map(String::as_str)
                .collect();
            options.sort_unstable();
            assert_eq!(
                options,
                [
                    "max_docs_per_pass",
                    "max_per_input",
                    "min_confidence",
                    "min_score",
                    "weight",
                ],
                "`options` is NOT redefined: {row}"
            );
            assert!(row["options_schema"].is_array(), "{row}");
            assert!(row["options_set"].is_object(), "{row}");
        }
    }

    /// `options_schema` is the REGISTRY's declaration and `options_set` is the
    /// FILE's key set — two different facts from two different sources, neither of
    /// them the resolved config (design §11.5):
    ///
    /// * the schema lists EVERY declared key, so the one key with no declared
    ///   default is reachable even though no resolved value can show it;
    /// * `options_set` reports what the file carries, which is how a write stops
    ///   re-materializing a resolved default into a row the user never set.
    #[test]
    fn the_schema_is_the_registry_and_the_key_set_is_the_file() {
        let plane = plane("[capabilities.recall_leg_memory_semantic]\nenabled = true\n");

        let schema = options_schema(spec(CapabilityId::RecallLegMemorySemantic));
        assert_eq!(
            schema.iter().map(|e| e.key).collect::<Vec<_>>(),
            ["weight", "min_score"],
            "every DECLARED key, in registry order"
        );
        assert_eq!(
            schema[0].default,
            Some(1.0),
            "the registry declares a weight"
        );
        assert_eq!(
            schema[1].default, None,
            "and declares NO min_score: the key no resolved value can render"
        );
        assert_eq!(schema[1].kind, "float");
        assert_eq!((schema[1].min, schema[1].max), (0.0, 1.0));
        assert_eq!(schema[1].expectation, "finite and 0.0..=1.0");
        assert!(
            options_schema(spec(CapabilityId::RecallLegWiki)).is_empty(),
            "a row that declares nothing keeps its empty schema, not an empty editor"
        );

        let set = plane.options_set(CapabilityId::RecallLegMemorySemantic);
        assert!(set.enabled, "the file carries `enabled`");
        assert_eq!(set.keys.get("weight"), Some(&false));
        assert_eq!(set.keys.get("min_score"), Some(&false));
        assert_eq!(
            plane.options(CapabilityId::RecallLegMemorySemantic).weight,
            Some(1.0),
            "RESOLVED: the default the file did NOT carry — the pair of facts (2) is about"
        );
        assert_eq!(
            plane
                .options(CapabilityId::RecallLegMemorySemantic)
                .min_score,
            None
        );

        // A row the table does not name, and a plane with no table at all: every
        // entry false, `enabled` included.
        for leg in [
            plane.options_set(CapabilityId::RecallLegWiki),
            CapabilityPlane::legacy().options_set(CapabilityId::RecallLegMemorySemantic),
        ] {
            assert!(!leg.enabled);
            assert!(leg.keys.values().all(|present| !present), "{leg:?}");
        }
        assert!(
            plane
                .options_set(CapabilityId::RecallLegWiki)
                .keys
                .is_empty()
        );

        // The count the whole surface exists for: EXACTLY ONE declared key has no
        // registry default. If this moves, a behaviour was invented (a default is a
        // behaviour) or a reachable key was lost — both must be deliberate.
        let no_default: Vec<(&str, &str)> = specs()
            .iter()
            .flat_map(|s| {
                options_schema(s)
                    .into_iter()
                    .filter(|e| e.default.is_none())
                    .map(move |e| (s.id.as_str(), e.key))
            })
            .collect();
        assert_eq!(
            no_default,
            [(
                CapabilityId::RecallLegMemorySemantic.as_str(),
                OptionKey::MinScore.as_str()
            )],
            "design §11.5: the ONE declared key with no default, named"
        );
    }

    /// THE WRITE DOOR (§11.5). Before/after, the `unconfirmed_llm_enable` shape: a
    /// `PUT` that CREATES an enabled leg at weight 0.0 is refused; one that merely
    /// carries an existing one forward is not (a file that already carries the state
    /// still boots, and its panel must still be able to save). Increment 5 covered the
    /// KNOWLEDGE legs too, so both families are asserted here.
    #[test]
    fn the_write_door_refuses_a_new_enabled_leg_at_zero_weight_and_is_a_transition() {
        let healthy =
            plane("[capabilities.recall_leg_memory_semantic]\nenabled = true\nweight = 1.0\n");
        let zeroed =
            plane("[capabilities.recall_leg_memory_semantic]\nenabled = true\nweight = 0.0\n");

        let message = zero_weight_leg(&healthy, &zeroed).expect("the transition is refused");
        // The sentence IS the recall path's: claimed here against the runtime's own
        // constructor (`RecallLegConfig::resolve` -> `check_weights`), so the door
        // cannot grow a second wording.
        let runtime = crate::memembed::RecallLegConfig::resolve(
            (true, Some(0.0)),
            (true, Some(1.0)),
            (true, None),
            (true, None),
            true,
            true,
        )
        .expect_err("the runtime refuses an enabled leg at weight 0");
        assert_eq!(message, crate::api::leg_config_error(runtime));
        assert!(
            message.contains("disable the leg instead of zeroing it"),
            "{message}"
        );
        assert!(message.contains("memory semantic"), "{message}");

        // No transition, no refusal: an existing zero is not a NEW misconfiguration.
        assert_eq!(zero_weight_leg(&zeroed, &zeroed), None);
        assert_eq!(
            zero_weight_leg(&zeroed, &healthy),
            None,
            "fixing it is a legal write"
        );
        // A DISABLED leg at 0.0 is legal at both doors: `check_weights` ignores a
        // disabled leg's weight, and disabling is the supported way to drop one.
        let off =
            plane("[capabilities.recall_leg_memory_semantic]\nenabled = false\nweight = 0.0\n");
        assert_eq!(zero_weight_leg(&healthy, &off), None);
        // The other memory leg, whose message names ITS leg.
        let fts = plane("[capabilities.recall_leg_memory_fts]\nenabled = true\nweight = 0.0\n");
        let fts_message = zero_weight_leg(&healthy, &fts).expect("fts is refused too");
        assert!(fts_message.contains("memory fts"), "{fts_message}");

        // INCREMENT 5: the KNOWLEDGE legs are covered by the same door, and their
        // sentence names the runtime's own label for them.
        let knowledge_semantic =
            plane("[capabilities.recall_leg_knowledge_semantic]\nenabled = true\nweight = 0.0\n");
        let ks_message =
            zero_weight_leg(&healthy, &knowledge_semantic).expect("a knowledge leg is refused");
        assert!(ks_message.contains("knowledge semantic"), "{ks_message}");
        assert!(
            ks_message.contains("disable the leg instead of zeroing it"),
            "{ks_message}"
        );
        let knowledge_fts =
            plane("[capabilities.recall_leg_knowledge_fts]\nenabled = true\nweight = 0.0\n");
        let kf_message =
            zero_weight_leg(&healthy, &knowledge_fts).expect("the other knowledge leg too");
        assert!(kf_message.contains("knowledge keyword"), "{kf_message}");
        // ...and the transition rule holds for them as well, so a file that already
        // carries one stays editable.
        assert_eq!(
            zero_weight_leg(&knowledge_semantic, &knowledge_semantic),
            None
        );
        assert_eq!(
            zero_weight_leg(&knowledge_semantic, &healthy),
            None,
            "fixing a knowledge leg is a legal write"
        );
        // A leg the runtime does NOT weigh is never refused by this door: wiki and
        // graph are switches in `RecallLegConfig`, and `check_weights` never sees them.
        let wiki = plane("[capabilities.recall_leg_wiki]\nenabled = true\n");
        assert_eq!(zero_weight_leg(&healthy, &wiki), None);
        assert_eq!(
            zero_weight_leg(&healthy, &CapabilityPlane::legacy()),
            None,
            "legacy mode (table absent) can carry no refused leg"
        );
    }

    /// THE DOOR'S COVERAGE IS THE RUNTIME'S COVERAGE (increment 5). NEITHER SIDE OF
    /// THIS COMPARISON IS A LIST OF LEGS: for every capability the registry knows, the
    /// door is asked about a plane that puts that capability at `enabled + weight 0.0`
    /// (or plain `enabled` where it cannot carry a weight), and the runtime's own answer
    /// is `recall_legs(&after)` — `RecallLegConfig::resolve`, the call the recall handler
    /// makes. Equality is asserted per id, so a leg the runtime learns to weigh (or one
    /// the registry stops configuring) cannot be covered by one side and not the other,
    /// and the door's sentence is asserted to be the runtime's sentence.
    #[test]
    fn the_write_door_covers_exactly_the_legs_the_runtime_refuses() {
        let healthy = CapabilityPlane::legacy();
        let mut covered: Vec<&'static str> = Vec::new();
        for id in CapabilityId::ALL {
            // The one row this probe configures. A row that declares no `weight` cannot
            // be given one — `from_policy` refuses the undeclared key — so the probe
            // spells only what the registry declares.
            let file = ruagent_policy::CapabilityFile {
                enabled: Some(true),
                weight: spec(*id)
                    .options
                    .contains(&OptionKey::Weight)
                    .then_some(0.0),
                ..Default::default()
            };
            let after = CapabilityPlane::from_policy(&ruagent_policy::PolicyConfig {
                capabilities: Some(BTreeMap::from([(id.as_str().to_string(), file)])),
                ..Default::default()
            })
            .expect("a probe row the registry declares validates");

            let door = zero_weight_leg(&healthy, &after);
            let runtime = crate::api::recall_legs(&after);
            assert_eq!(
                door.is_some(),
                runtime.is_err(),
                "`{}`: the door and the runtime must agree",
                id.as_str()
            );
            match (door, runtime) {
                (Some(message), Err(e)) => {
                    assert_eq!(
                        message,
                        crate::api::leg_config_error(e),
                        "`{}`: one sentence, the runtime's",
                        id.as_str()
                    );
                    covered.push(id.as_str());
                }
                (None, Ok(_)) => {}
                (door, runtime) => panic!("`{}`: door={door:?} runtime={runtime:?}", id.as_str()),
            }
        }
        // The reading, and why it is the same set on both daemon surfaces: the legs the
        // runtime weighs are exactly the rows that DECLARE a `weight`, which is the set
        // the panel's own guard (`zeroWeightEnabled`) applies to. Computed, not listed.
        //
        // THIS ASSERTION IS ALSO THE "SIXTH LEG" ALARM, and it is the reason the door
        // itself may not carry a list. If a leg is added to the runtime later: with a
        // registry row that declares `weight` and the reading wired into
        // `crate::api::recall_legs`, the door covers it automatically (the per-id
        // equality above stays green because the door ASKS the runtime) and this
        // comparison stays green; a leg added to the runtime but NOT wired into
        // `recall_legs` makes `covered` miss an id that `declares_weight` has, so this
        // goes RED and forces the decision instead of drifting; a registry row that
        // declares no `weight` (wiki, graph, and the nine non-recall capabilities)
        // appears in neither list. A hardcoded list in the door would make all three
        // cases silent.
        let declares_weight: Vec<&'static str> = specs()
            .iter()
            .filter(|s| s.options.contains(&OptionKey::Weight))
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(
            covered, declares_weight,
            "the door's coverage is exactly the registry's weight-declaring rows"
        );
        // The four ids below are the READING this increment is about, not an input to
        // the door: the set above was computed from the runtime and the registry.
        assert_eq!(
            covered,
            [
                "recall_leg_memory_semantic",
                "recall_leg_memory_fts",
                "recall_leg_knowledge_semantic",
                "recall_leg_knowledge_fts",
            ],
            "the reading this increment is about: four legs, wiki and graph NOT among them"
        );
    }
}
