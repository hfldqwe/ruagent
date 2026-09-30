//! Zero-token deterministic extraction — the free tier of
//! `docs/plans/capability-plugins-design.md` §7-§9, the "不消耗 token" half of
//! the capability plane.
//!
//! Two pure algorithms, each reachable with or without the caller's provenance
//! label:
//!
//! * [`memory_candidates`] / [`memory_candidates_for`] — session transcript ->
//!   memory candidates (§8), with [`memory_candidates_with`] returning the
//!   counters as well.
//! * [`graph_candidates`] / [`graph_candidates_for`] — knowledge document text
//!   -> entity/alias/relation candidates (§9).
//!
//! # Purity contract (design §7.2), verifiable rather than aspirational
//!
//! This crate takes text and returns plain structs. It has **no dependencies at
//! all**, no runtime, no HTTP client, no database, no internal crate; it performs
//! no filesystem, network, environment, clock, thread or randomness access, and
//! it never invokes an agent or a model — a model call is by definition a token
//! cost, and this is the tier that costs none. Applying candidates to SQLite
//! lives in the daemon (`crates/daemon/src/extract_plane.rs`, design §10) and in
//! `ruagent_graph::apply_extraction` (graph/src/lib.rs:1427).
//!
//! `tests/bounded.rs` proves that mechanically: it scans every source file of
//! this crate for the forbidden paths and scans this crate's manifest for a
//! dependency table, so a future edit that reaches for I/O fails a test instead
//! of a review.
//!
//! # Determinism
//!
//! Same input => byte-identical output, independent of collection iteration
//! order and of the process environment. The mechanisms, all visible in the
//! sources: ordered collections only (never a hash map or a hash set), explicit
//! total-order sorts with a final tie-break on the name, no clock, no
//! randomness, no environment read, and no state carried between calls. Keys are
//! derived from content, never from addresses or insertion timing.
//!
//! # Boundedness (design §8.5, §9.5)
//!
//! Output is capped by [`ExtractLimits::max_per_input`]; every emitted body is
//! at most [`ExtractLimits::max_content_bytes`] or the unit is DROPPED, never
//! truncated (half a sentence is invented text — the repo has already paid for
//! body rewriting, t347/distill.rs:671-677); entity names are at most
//! [`ExtractLimits::max_entity_name_chars`]; the transcript window is the last
//! [`ExtractLimits::max_turns`] turns; the document window is the first
//! [`ExtractLimits::max_text_bytes`] bytes; the term table is capped at
//! [`rules::MAX_DISTINCT_TERMS`]. Work is linear in the windowed input.
//!
//! # Named deviations from the design text, and why
//!
//! The design is the contract; where two of its own statements disagree, this
//! crate implements the statement the acceptance criteria and the design's own
//! gold fixture pin, and says so here:
//!
//! 1. **`RelationCandidate::relation` and `valid_at`.** §7.3 pins
//!    `relation: &'static str` and no event time; §10.2 then instructs
//!    `RelationName { Rule(&'static str), Model(String) }` on the same type,
//!    because the LLM tier carries a model-authored string and the write path
//!    needs `valid_at` to decide `event_time_source` (distill.rs:808-812). Both
//!    are implemented here, so the seam of §10 does not have to reopen this
//!    crate: rule facts use `RelationName::Rule`, model facts use
//!    `RelationName::Model`, and [`RelationName::relation_literal`] is the
//!    writer's accessor. `valid_at` is `Some` only when the source sentence
//!    states an explicit ISO-8601 instant, and `None` otherwise — the free tier
//!    has no clock to consult, which makes "no event time is known" unbreakable.
//! 2. **`ExtractLimits::max_per_input` has two documented defaults.** §7.3 ships
//!    32 and §5.2/§9.5 ship 96 for the knowledge-ingestion capability.
//!    [`ExtractLimits::default`] is §7.3's set; [`ExtractLimits::graph_default`]
//!    is §9.5's, for the caller that builds limits from the
//!    `knowledge_ingest_graph` options.
//! 3. **The §8.7 content-token guard, and the confirmation candidate.** The
//!    guard's definition (ASCII run >= 3 or Han run >= 2) does NOT reject
//!    §8.6's confirmation sentence "对，就是这样。" — "就是这样" is a four-Han
//!    run — so the guard cannot be what "stops it being a *preference*". What
//!    the fixture and the acceptance criteria require is an explicit-confirmation
//!    candidate: the confirming sentence is emitted by the `user_preference` rule
//!    with `CandidateStore::Profile` and `CandidateConfidence::Confirmed`, which
//!    §8.2's own confidence column already implies ("`Confirmed` if the same turn
//!    also contains `CONFIRM`"). The guard is implemented exactly as defined and
//!    tested; the design's parenthetical about it is noted as inaccurate.
//! 4. **`TOOLISH` is enumerated in [`rules::TOOLISH`].** §8.2 names the set but
//!    never lists it.
//! 5. **Two alias/term bounds** are narrower than a literal reading of §9.2, both
//!    to stop a deterministic rule from producing false merges: acronym aliases
//!    need [`rules::MIN_ACRONYM_CHARS`] characters, and a maximal Han run longer
//!    than [`rules::MAX_HAN_TERM_CHARS`] contributes no term. Each is stated and
//!    tested.
//! 6. **A fifth entity rule, `wiki_link`.** §9.2's table lists four structural
//!    sources (headings, inline code, capitalized phrases, Han runs) while the
//!    task's acceptance names "headings, wiki links, code identifiers,
//!    capitalized phrases". `[[target]]` / `[[target|display]]` is a deliberate
//!    reference the document made, so it is implemented as a rule of its own —
//!    placed directly after `heading_entity` in identity priority, because both
//!    are the author's own terms rather than a coincidence. A document with no
//!    wiki link is completely unaffected.
//!
//! # Provenance and keys
//!
//! Every candidate carries two things the applier needs and cannot recompute:
//! where it came from (`origin`, plus `source` — the caller's own label: a
//! session key for a transcript, a document/chunk id for knowledge text) and its
//! [`MemoryCandidate::dedup_key`] / [`EntityCandidate::dedup_key`] /
//! [`RelationCandidate::dedup_key`], the stable identity of the candidate within
//! one input. The keys are exposed rather than kept private so the daemon's
//! merge step and this crate's own dedup cannot drift apart.
//!
//! # Not in this crate
//!
//! Nothing here knows what a *capability* is (design §7.2). Limits arrive as an
//! argument — this crate never reads configuration — and candidates leave as
//! plain structs: dedup at the store is the applier's job, and the graph's
//! `judge_against`/`variants`/`add_alias` remain the authority on identity.

pub mod graph;
pub mod memory;
pub mod rules;
pub mod text;

pub use graph::{graph_candidates, graph_candidates_for};
pub use memory::{
    MemoryCandidates, memory_candidates, memory_candidates_for, memory_candidates_with,
};

/// One transcript turn, mapped by the daemon from `sessions::SessionMessage`
/// (crates/daemon/src/sessions.rs:36-39), whose `role` is normalised there.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Turn {
    pub role: Role,
    pub text: String,
    /// The turn's timestamp. The free tier never reads it as a fact time — see
    /// `text::iso_datetime`, the only event-time source the rules accept — but
    /// the field is carried so the daemon can map its own message type without
    /// losing information.
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    #[default]
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// Memory-candidate confidence as EVIDENCE, not as a number: the number is
/// chosen in ONE place already (`ruagent_memory::confidence::confidence()`,
/// crates/memory/src/confidence.rs:100), and the free tier must speak the same
/// vocabulary the LLM prompt asks the model for (distill.rs:19-22).
///
/// The applier maps the signal through that one rule: `Confirmed` -> 1.0,
/// `Corrected` -> 1.0, `Hedged` -> 0.4 (below `LOW_CONFIDENCE`, so the panel
/// renders the doubt instead of hiding it), `Unconfirmed` -> 0.8,
/// `Explicit(v)` -> `v` (the LLM tier's own number, used verbatim).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CandidateConfidence {
    /// The user explicitly confirmed it (`ConfidenceSignals::confirmed()`).
    Confirmed,
    /// The user corrected the agent (`ConfidenceSignals::corrected()`).
    Corrected,
    /// The speaker hedged (`ConfidenceSignals::hedged()`).
    Hedged,
    /// Nothing was said about it (`ConfidenceSignals::unconfirmed()`).
    Unconfirmed,
    /// The LLM tier's own number, used verbatim.
    Explicit(f64),
}

impl CandidateConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateConfidence::Confirmed => "Confirmed",
            CandidateConfidence::Corrected => "Corrected",
            CandidateConfidence::Hedged => "Hedged",
            CandidateConfidence::Unconfirmed => "Unconfirmed",
            CandidateConfidence::Explicit(_) => "Explicit",
        }
    }
}

/// The memory store a candidate belongs in. The names match the LLM contract
/// (`EXTRACTION_PROMPT`, distill.rs:31: `profile|observation|procedure|lesson`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CandidateStore {
    #[default]
    Profile,
    Observation,
    Procedure,
    Lesson,
}

impl CandidateStore {
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateStore::Profile => "profile",
            CandidateStore::Observation => "observation",
            CandidateStore::Procedure => "procedure",
            CandidateStore::Lesson => "lesson",
        }
    }
}

/// Where a candidate came from, for the applier's audit and for the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateOrigin {
    /// A transcript turn: the index in the input slice, and its role.
    Turn { index: usize, role: Role },
    /// A document-level candidate; `section` is the 0-based section index the
    /// rules computed (0 = the preamble before the first heading, heading *n*
    /// owns section *n+1*), or `None` when the caller passed manifestly
    /// sectionless text (the LLM tier's own reply).
    Document { section: Option<usize> },
}

impl Default for CandidateOrigin {
    fn default() -> Self {
        CandidateOrigin::Document { section: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCandidate {
    pub store: CandidateStore,
    /// A canonical namespace string: "user" | "global" | "project:<name>" |
    /// "agent:<name>". The free tier emits [`rules::NAMESPACE_USER`]; the
    /// applier validates it through `ruagent_memory::namespace::Namespace::parse`
    /// (used at distill.rs:683) and may narrow it.
    pub namespace: String,
    /// The source sentence, verbatim (trimmed, whitespace-collapsed). Never
    /// rewritten or completed (design §7.4.3).
    pub content: String,
    pub confidence: CandidateConfidence,
    /// The rule that produced it, e.g. "user_preference". Stable; it is part of
    /// the dedup key and is named in the tests.
    pub rule: &'static str,
    pub origin: CandidateOrigin,
    /// The caller's provenance label — the session key, when the caller supplied
    /// one through [`memory_candidates_for`]. Empty when it did not: this crate
    /// cannot know a session key without reading something, and reading
    /// something is exactly what it must not do.
    pub source: String,
}

impl MemoryCandidate {
    /// The stable identity of this candidate within one input (design §8.4):
    /// `rule` + normalized content. Two runs over the same transcript produce
    /// the same key for the same fact, and a re-stated sentence is one candidate.
    pub fn dedup_key(&self) -> String {
        format!("{}\u{1f}{}", self.rule, text::norm(&self.content))
    }
}

/// The relation on a candidate, in the two shapes the seam needs (design §10.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationName {
    /// A literal from the closed table [`rules::RELATION_PATTERNS`]. The free
    /// tier never invents a relation name (§9.4).
    Rule(&'static str),
    /// The model's own snake_case string. Deliberately not subject to the rule
    /// table: routing an LLM relation through it would reject every one.
    Model(String),
}

impl RelationName {
    /// What the writer stores.
    pub fn relation_literal(&self) -> &str {
        match self {
            RelationName::Rule(s) => s,
            RelationName::Model(s) => s.as_str(),
        }
    }
}

impl Default for RelationName {
    fn default() -> Self {
        RelationName::Rule("")
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityCandidate {
    pub name: String,
    /// "person" | "project" | "tool" | "org" | "product" | "concept". The free
    /// tier only ever produces "tool" or "concept".
    pub kind: Option<&'static str>,
    /// One line of source text about the term — the deterministic stand-in for
    /// the LLM's "one line". Verbatim, and `None` when no source sentence for
    /// the term fits `max_content_bytes`.
    pub summary: Option<String>,
    /// Candidate spellings. The authoritative resolution is the write path's
    /// (graph's `judge_against`/`variants`/`add_alias` via `apply_extraction`);
    /// the extractor's job is to hand it the spellings, not to decide identity.
    pub aliases: Vec<String>,
    /// Ranking in (0,1]; the graph has no confidence column
    /// (`ExtractEntity`, graph/src/lib.rs:1375-1380), so this is used ONLY to
    /// cut the candidate set (design §9.3).
    pub score: f32,
    pub rule: &'static str,
    pub origin: CandidateOrigin,
    /// The caller's provenance label — the document or chunk id, when the caller
    /// supplied one through [`graph_candidates_for`]. Empty otherwise.
    pub source: String,
}

impl EntityCandidate {
    /// The stable identity of this entity within one input (design §9.5):
    /// `norm(base_name(name))`, so case, whitespace and one trailing
    /// parenthetical cannot make one object into two.
    pub fn dedup_key(&self) -> String {
        text::base_name(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RelationCandidate {
    /// Entity NAMES, resolved against the entities emitted in the same pass.
    pub src: String,
    pub dst: String,
    pub relation: RelationName,
    /// The source sentence, verbatim.
    pub fact: String,
    /// The explicit ISO-8601 instant the sentence states, else `None`. Never the
    /// current time (the crate has no clock): `None` means "no event time is
    /// known", which is not the same fact as "it became true now".
    pub valid_at: Option<String>,
    pub score: f32,
    pub rule: &'static str,
    pub origin: CandidateOrigin,
    /// The caller's provenance label — the document or chunk id, when the caller
    /// supplied one through [`graph_candidates_for`]. Empty otherwise.
    pub source: String,
}

impl RelationCandidate {
    /// The stable identity of this relation within one input (design §9.5):
    /// `(norm(src), relation, norm(dst))`. `support` is counted on exactly this
    /// key, so the score and the identity cannot disagree.
    pub fn dedup_key(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            text::norm(&self.src),
            self.relation.relation_literal(),
            text::norm(&self.dst)
        )
    }
}

/// Input bounds. The daemon builds this from capability options; the crate never
/// reads config (design §7.2). [`Default`] is the shipped set of §7.3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractLimits {
    /// Candidates per input. 32 for transcripts (§7.3/§5.2), 96 for a document
    /// (§9.5) — see [`ExtractLimits::graph_default`].
    pub max_per_input: usize,
    /// Graph candidates below this ranking are dropped (§9.3). Memory candidates
    /// are never filtered by confidence (§7.4.7) — only truncated.
    pub min_score: f32,
    /// The document window (§9.5). The transcript window is `max_turns`.
    pub max_text_bytes: usize,
    /// A longer content/fact unit is DROPPED, not truncated (§7.4.3).
    pub max_content_bytes: usize,
    pub max_entity_name_chars: usize,
    /// Only the last `max_turns` turns are read (§7.4.1): recent material is what
    /// a session is about.
    pub max_turns: usize,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self {
            max_per_input: 32,
            min_score: 0.0,
            max_text_bytes: 256 * 1024,
            max_content_bytes: 400,
            max_entity_name_chars: 60,
            max_turns: 2000,
        }
    }
}

impl ExtractLimits {
    /// The knowledge-ingestion defaults of §9.5/§5.2: the same bounds with
    /// `max_per_input = 96`, which is what `knowledge_ingest_graph` ships.
    pub fn graph_default() -> Self {
        Self {
            max_per_input: 96,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphCandidates {
    pub entities: Vec<EntityCandidate>,
    pub relations: Vec<RelationCandidate>,
    /// True when the input was cut to `max_text_bytes`.
    pub truncated: bool,
    pub bytes_skipped: usize,
}
