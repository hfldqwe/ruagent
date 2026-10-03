//! The extractor seam: one shape, two interchangeable implementations (t4,
//! docs/plans/capability-plugins-design.md §10).
//!
//! WHY A SEAM AT ALL: until this module, extraction existed in exactly one
//! shape — `distill.rs` composed the ACP prompt, ran ONE chat turn
//! (`Distiller::ask_agent`) and parsed the model's JSON. Every extraction cost
//! model tokens, and the deterministic machinery the repo already had (the
//! graph's resolution/normalisation layer) could only consume candidates, never
//! produce them. This module is the place where a *producer* is chosen:
//!
//! * [`ExtractSource::Rules`] — `ruagent_extract`'s pure functions
//!   (`memory_candidates`, `graph_candidates`): zero tokens, no I/O, no clock,
//!   same input ⇒ same output.
//! * [`ExtractSource::Acp`] — today's path, unchanged: `ask_agent` +
//!   `parse_extraction`, mapped onto the same candidate types.
//!
//! THE POINT OF THE SEAM is that neither call site knows which one ran: both
//! produce an [`ExtractBundle`], and `distill.rs` writes a bundle without asking
//! where it came from. The choice is data, not a branch at the call site: an
//! [`ExtractPlan`], resolved ONCE from the capability plane.
//!
//! COST (law L4): the llm tier is `distill_session`, which defaults OFF, so
//! `ExtractPlan::unattended` on the shipped configuration enables NOTHING —
//! `extract` then refuses with [`ExtractError::NoSourceEnabled`] instead of
//! quietly running the expensive path. A caller that wants the ACP tier has to
//! pass an `AcpExtractor`, and a caller that wants the free tier has to have it
//! enabled in `[capabilities]`.
//!
//! MANUAL DISTILLATION IS NOT GATED (§3.7): `POST /api/v1/sessions/{key}/distill`
//! is an explicit human/agent instruction, so `ExtractPlan::explicit` runs what
//! the caller asked for. Only the UNATTENDED path answers to the capability.

use std::collections::BTreeSet;

use ruagent_extract::{EntityCandidate, Turn};
use ruagent_extract::{ExtractLimits, MemoryCandidate, RelationCandidate};

use crate::capability::{CapabilityId, CapabilityPlane};
use crate::distill::Distiller;

/// Which implementation produced a bundle. An enum, not a trait: the set is
/// closed (two), `async-trait` appears nowhere in the tree and the repo's own
/// precedent for closed dispatch is an enum (`HarnessKind`, `FusionKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractSource {
    Rules,
    Acp,
}

impl ExtractSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ExtractSource::Rules => "rules",
            ExtractSource::Acp => "acp",
        }
    }
}

/// What the caller asked the manual route for (the optional body of §14.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extractor {
    /// Today's behaviour, and the default when no body is sent.
    Acp,
    /// Zero tokens.
    Rules,
    /// Both, rules first (a rule-derived candidate wins the dedup).
    Both,
}

impl Extractor {
    /// Parse the route's `extractor` field. Anything else is a hard error naming
    /// the value and the three accepted ones (law L3: no silent default).
    pub fn parse(value: &str) -> Result<Self, ExtractError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "acp" => Ok(Extractor::Acp),
            "rules" => Ok(Extractor::Rules),
            "both" => Ok(Extractor::Both),
            other => Err(ExtractError::UnknownExtractor(other.to_string())),
        }
    }
}

/// Which implementations one pass runs, and the bounds they run under.
///
/// Built by the CALLER from the capability plane — the seam itself reads no
/// config, the same rule `ruagent_extract` follows. The three constructors are
/// the only three intents that exist: the unattended path (gated), the explicit
/// manual request (ungated), and today's ACP-only call (the auto-distill caller
/// before the plane existed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractPlan {
    pub rules: bool,
    pub acp: bool,
    pub limits: ExtractLimits,
}

impl Default for ExtractPlan {
    fn default() -> Self {
        Self::acp_only()
    }
}

impl ExtractPlan {
    /// Today's behaviour: one ACP turn, no rule candidates.
    pub fn acp_only() -> Self {
        Self {
            rules: false,
            acp: true,
            limits: ExtractLimits::default(),
        }
    }

    /// The UNATTENDED path. `legacy_auto` is `[distill].auto`, i.e. what today's
    /// code would do; the llm tier may only NARROW it (law L2), and the new free
    /// tier is opt-in (`enabled`, not `gate`: it has no legacy flag to narrow).
    ///
    /// With the shipped configuration — no `[capabilities]` table — this resolves
    /// to `{ rules: false, acp: false }`: nothing runs.
    pub fn unattended(plane: &CapabilityPlane, legacy_auto: bool) -> Self {
        let options = plane.options(CapabilityId::SessionExtractRules);
        Self {
            rules: plane.enabled(CapabilityId::SessionExtractRules),
            acp: plane.gate(CapabilityId::DistillSession, legacy_auto),
            limits: session_limits(options.max_per_input, options.min_confidence),
        }
    }

    /// An EXPLICIT request (§3.7). Manual distillation is not gated: what the
    /// caller asked for runs.
    pub fn explicit(extractor: Extractor) -> Self {
        Self {
            rules: matches!(extractor, Extractor::Rules | Extractor::Both),
            acp: matches!(extractor, Extractor::Acp | Extractor::Both),
            limits: ExtractLimits::default(),
        }
    }

    /// The label the outcome and the API response carry: `rules`, `acp` or
    /// `rules+acp`.
    pub fn label(&self) -> &'static str {
        match (self.rules, self.acp) {
            (true, true) => "rules+acp",
            (true, false) => "rules",
            _ => "acp",
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.rules && !self.acp
    }
}

/// The session-tier bounds: the registry defaults are 32 candidates and a 0.0
/// floor (§5.2), and a capability that declares nothing still resolves to them.
fn session_limits(max_per_input: Option<u32>, min_confidence: Option<f64>) -> ExtractLimits {
    ExtractLimits {
        max_per_input: max_per_input.unwrap_or(32) as usize,
        min_score: min_confidence.unwrap_or(0.0) as f32,
        ..ExtractLimits::default()
    }
}

/// ONE extraction result, whichever tier produced it. This is the seam: the
/// writer in `distill.rs` takes this shape and cannot tell the producers apart.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractBundle {
    pub memories: Vec<MemoryCandidate>,
    pub entities: Vec<EntityCandidate>,
    pub relations: Vec<RelationCandidate>,
    /// The source that produced the LAST non-empty part of this bundle. `None`
    /// only for an empty, default bundle.
    pub source: Option<ExtractSource>,
    /// True when the pass that produced this bundle DROPPED something at a bound:
    /// the candidate cap (`ExtractLimits::max_per_input`), the transcript window
    /// (`max_turns`) or the document window (`max_text_bytes`). False means "this
    /// is everything the pass produced". Derived from the producing pass's own
    /// counters (t9), never from a quantity that pass did not measure — so a
    /// candidate list cut at the cap reports true while a list that legitimately
    /// fills it reports false.
    pub truncated: bool,
}

impl ExtractBundle {
    pub fn is_empty(&self) -> bool {
        self.memories.is_empty() && self.entities.is_empty() && self.relations.is_empty()
    }

    /// Fold `other` into `self`, keeping the FIRST candidate per identity — so
    /// the fixed order Rules → Acp means a rule-derived candidate outlives an
    /// LLM paraphrase of the same sentence.
    pub fn merge(&mut self, other: &ExtractBundle) {
        let mut seen_memories: BTreeSet<String> = self.memories.iter().map(memory_key).collect();
        for m in &other.memories {
            if seen_memories.insert(memory_key(m)) {
                self.memories.push(m.clone());
            }
        }
        let mut seen_entities: BTreeSet<String> = self.entities.iter().map(entity_key).collect();
        for e in &other.entities {
            if seen_entities.insert(entity_key(e)) {
                self.entities.push(e.clone());
            }
        }
        let mut seen_relations: BTreeSet<String> =
            self.relations.iter().map(relation_key).collect();
        for r in &other.relations {
            if seen_relations.insert(relation_key(r)) {
                self.relations.push(r.clone());
            }
        }
        self.truncated |= other.truncated;
        if other.source.is_some() {
            self.source = other.source;
        }
    }

    /// Byte-level identity guard, applied to a bundle that may hold candidates
    /// from several sources.
    pub fn dedup(self) -> Self {
        let mut out = ExtractBundle {
            source: self.source,
            truncated: self.truncated,
            ..ExtractBundle::default()
        };
        out.merge(&self);
        out
    }
}

fn norm_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn memory_key(c: &MemoryCandidate) -> String {
    format!("{}\u{1f}{}", c.rule, norm_key(&c.content))
}

fn entity_key(c: &EntityCandidate) -> String {
    norm_key(&c.name)
}

fn relation_key(c: &RelationCandidate) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        norm_key(&c.src),
        c.relation.relation_literal().to_lowercase(),
        norm_key(&c.dst)
    )
}

/// What an extraction pass may read. Every field is available at the call site
/// today: the transcript is exactly what `render_transcript` produces
/// (distill.rs:520-536) and the turns are the same session messages the free
/// tier reads.
pub struct ExtractCtx<'a> {
    pub session_key: &'a str,
    /// The rendered transcript, verbatim — the ACP prompt appends it and the
    /// rules tier reads it as one text.
    pub transcript: &'a str,
    /// The transcript as turns. Empty for a plan that does not run the free tier.
    pub turns: &'a [Turn],
    pub limits: ExtractLimits,
}

/// Why an extraction produced nothing, or failed. Every variant NAMES the thing
/// that went wrong: a silent empty bundle would be indistinguishable from "the
/// session had nothing in it".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    /// No implementation was enabled. This is the DEFAULT configuration's
    /// answer, and it is a refusal, never a fallback to the expensive path.
    NoSourceEnabled,
    /// The ACP turn failed to run or the agent was not reachable.
    Acp { agent: String, reason: String },
    /// The ACP reply carried no parsable JSON object (`parse_extraction`).
    Malformed(String),
    /// An `extractor` value the route does not know.
    UnknownExtractor(String),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::NoSourceEnabled => write!(
                f,
                "no extractor is enabled: enable `session_extract_rules` (zero tokens) or \
                 `distill_session` (spends model tokens) in [capabilities]"
            ),
            ExtractError::Acp { agent, reason } => {
                write!(f, "extraction agent `{agent}` failed: {reason}")
            }
            ExtractError::Malformed(reason) => {
                write!(f, "the extraction reply carried no JSON object: {reason}")
            }
            ExtractError::UnknownExtractor(value) => write!(
                f,
                "unknown extractor `{value}`: accepted values are `acp` (the default), `rules` \
                 and `both`"
            ),
        }
    }
}

impl std::error::Error for ExtractError {}

/// The free implementation: pure functions, zero tokens, no I/O.
///
/// The session tier produces MEMORY candidates — that is what the capability's
/// registry row says it gates ("transcript -> memory candidates"), and widening
/// it to graph candidates would be a second, undeclared capability. The
/// knowledge-document half of the free tier (`graph_candidates`) is called
/// directly by `crate::knowledge_graph`, which is a different producer with its
/// own switch.
/// The heads of the daemon's OWN job prompts, as LITERALS, plus the reason they
/// are literals.
///
/// **THE RULE IS THE MARKER, NOT THIS LIST** (t23). Every unattended prompt this daemon
/// sends goes through `Distiller::ask_agent`, which appends `chat::USER_TEXT_SENTINEL`
/// (t21); a turn that carries that marker with nothing of the user's after it is
/// platform text WHATEVER IT SAYS. Keying on the marker is strictly better than
/// enumerating our own prompts, because the marker is applied where a prompt is SENT —
/// so the next unattended prompt is covered with nobody remembering anything. That is
/// the property this list cannot have, and it is not hypothetical: the list held the
/// extraction prompt and `WIKI PLANNER`, and the wiki PAGE WRITER prompt — sent through
/// the same `ask_agent` — was not on it, so that whole family was still read as the
/// user's words. Measured on the corpus copy (t23): `WIKI PAGE WRITER` appears in 63
/// lines across 16 session files.
///
/// **THIS LIST IS THE RESIDUE PATH, NOT A SECOND RULE.** It exists only for turns
/// ALREADY ON DISK, written before the marker did: the daemon cannot rewrite a
/// transcript an agent CLI logged months ago, and such a turn carries nothing else that
/// distinguishes it from the user's own paste. It is kept honest by
/// `every_agent_prompt_sender_is_covered_or_marked`, which walks the SENDERS instead of
/// relying on whoever adds the next prompt to update this comment.
///
/// Each string below is the first line of a prompt THIS platform composes to run one of
/// its own unattended jobs, stored one module over as a private `const`:
///
/// * `"You are a memory distillation engine"` — `distill.rs` `EXTRACTION_PROMPT`, the
///   prompt the ACP tier sends and the prompt a `workspaces/distill` job run records.
/// * `"WIKI PLANNER"` — `wiki.rs` `PLANNER_PROMPT`, the wiki build's planner.
/// * `"WIKI PAGE WRITER"` — `wiki.rs` `writer_prompt(slug)`, the wiki build's page
///   writer, whose prompt embeds the source documents in `<sources>`. Added by t23.
///
/// WHY LITERALS AND NOT THE CONSTANTS: both live in files this change does not own, and
/// neither is visible outside its module (a private `const` is not reachable from a
/// sibling). Naming them here instead of widening their visibility keeps this change
/// inside its two files, and the drift risk that copying a literal normally carries is
/// closed by `the_registered_prompt_heads_are_still_our_own_prompts`, which reads those
/// files at compile time and fails if a registered head stops matching the prompt it
/// names.
const OUR_JOB_PROMPT_HEADS: [&str; 3] = [
    "You are a memory distillation engine",
    "WIKI PLANNER",
    "WIKI PAGE WRITER",
];

/// Is this turn the PLATFORM's own text rather than something the user wrote?
///
/// STRUCTURAL, in the same sense `sessions::is_injected_title` documents for
/// titles: it matches text the daemon itself emits, by a registered head, with
/// `starts_with` and never `contains` — a user who pastes one of our prompts IS
/// talking to us, and dropping their message for mentioning ours would be the
/// substring mistake this codebase has already paid for once ("end" matching
/// "end_turn"). A message that merely quotes a head inside its own words is
/// untouched.
///
/// WHY THIS EXISTS AT ALL, measured on the 333 sessions of the real corpus copy
/// (ruagent-close-the-gaps t11): the free tier was reading OUR OWN PROMPTS back
/// as the user's words. 63 of the 501 user turns are a job prompt — 56 of them
/// the extraction prompt, 7 the wiki planner — and they produced 337 of the 1222
/// candidates, including nearly every `user_correction` (124 -> 9) and
/// `user_decision` (34 -> 9): the extraction prompt quotes its own examples
/// (`If the user CORRECTED …`), so the rules matched the scaffolding around the
/// conversation instead of the conversation.
///
/// The injected-block heads are `chat::INJECTED_HEADERS` — the same registry
/// `is_injected_title` uses, read from its one source rather than re-spelled. They
/// fire 0 times on that corpus (no session there carried an injected context
/// block); the registry is consulted anyway, because the mechanism that emits
/// those blocks is live and the next corpus will contain them, and a turn that is
/// ONLY such a block has nothing of the user's in it.
fn is_platform_text(text: &str) -> bool {
    let head = text.trim_start();
    OUR_JOB_PROMPT_HEADS
        .iter()
        .chain(crate::chat::INJECTED_HEADERS.iter())
        .any(|h| head.starts_with(h))
}

/// The turn as the free tier may read it, or `None` when there is nothing of the
/// user's in it.
///
/// TWO STEPS, in this order, and the order is the whole point:
///
/// 1. `sessions::user_text` splits off the injected prefix the daemon emits
///    (`chat::USER_TEXT_SENTINEL`) — the SAME structural split the session title
///    uses, from its one definition. That is what keeps a turn that carries BOTH
///    an injected block and the user's words: the user's part is kept, not
///    dropped with the scaffolding.
/// 2. What is left is checked against the platform's own prompt heads. A job
///    prompt has no sentinel (the job runner sends it as the prompt), so it
///    reaches this step intact and is dropped here.
///
/// A message that never carried injection and does not start with one of our
/// heads comes back unchanged.
fn turn_for_extraction(t: &Turn) -> Option<Turn> {
    // THE TWO RULES, in the order that matters (t23):
    //
    // 1. `sessions::user_text` cuts at the marker the daemon appends when it SENDS an
    //    unattended prompt. A platform prompt is all of the text BEFORE that marker, so
    //    its user side is empty -- and that is true of prompts nobody has written yet,
    //    which is why this arm, not the head list below, is the rule.
    // 2. The head list is the residue: a turn already on disk, written before the
    //    marker existed, has no marker to cut at, so the only handle left is the head
    //    of a prompt this build can compose.
    //
    // BOTH arms read the SPLIT text and never the raw one: a turn where the daemon
    // injected context and the USER's words follow it keeps those words (they are
    // non-empty after the split), and a user who merely pastes one of our prompts
    // mid-message is untouched -- `is_platform_text` matches `starts_with`, never
    // `contains`.
    let user_side = crate::sessions::user_text(&t.text);
    if user_side.trim().is_empty() || is_platform_text(user_side) {
        return None;
    }
    Some(Turn {
        role: t.role,
        text: user_side.to_string(),
        ts_ms: t.ts_ms,
    })
}

pub fn extract_rules(cx: &ExtractCtx<'_>) -> ExtractBundle {
    // TURN SELECTION, BEFORE THE PASS (t11). The seam is deliberate:
    //
    // * NOT in `sessions::parse_file_messages`: that parser is shared with the
    //   session VIEWER, so dropping messages there would hide the user's own
    //   transcript from the panel — the wrong fix for a problem in the free tier.
    // * NOT at candidate shaping (filtering what the pass returned): the pass
    //   applies `max_per_input` (32) to what it READ, so candidates made of our
    //   own prompt text would still consume the cap and squeeze the user's real
    //   candidates out of it. Measured on the corpus: with the filter here rather
    //   than after the pass, `procedure_note` 457 -> 491 and `lesson_learned`
    //   129 -> 145 RISE, because sessions that had been filled with prompt text
    //   now report what the user actually said. A post-pass filter cannot produce
    //   that, and the `max_turns` window (reported as `turns_skipped`) likewise
    //   counts the turns this tier is willing to read.
    //
    // The clone is the pass's own API (`&[Turn]`): it clones only what survives
    // the selection, and the allocation is per session, not per candidate.
    let turns: Vec<Turn> = cx.turns.iter().filter_map(turn_for_extraction).collect();
    let pass = ruagent_extract::memory_candidates_with(&turns, &cx.limits);
    // THE FLAG IS THE PASS'S OWN READING, NOT A RECOMPUTATION (t9).
    //
    // This used to be `truncated: cx.turns.len() > cx.limits.max_turns` — a
    // different quantity from the one it described. `max_turns` defaults to 2000
    // (`ExtractLimits::default`), so the flag was FALSE for a session whose
    // candidate list had been cut at `max_per_input` (32 by default). Measured on
    // 333 real sessions: 15 had hit the cap and all 333 reported `false`, with 16
    // reporting exactly 32 candidates — nothing in the response distinguished
    // "this session really has 32" from "this session was cut off at 32". A caller
    // that cannot tell those apart cannot decide whether to fall back to the model
    // tier, which is the decision this tier exists to inform.
    //
    // `memory_candidates_with` is the SAME pass `memory_candidates` runs (it is
    // literally that function with its counters kept: `.candidates` is their
    // candidates), so this reads what the pass did instead of asking a second
    // question beside it. The predicate is:
    //
    //     truncated = pass.truncated || pass.turns_skipped > 0
    //
    // * `pass.truncated` is set by the pass only when a deduped candidate was
    //   DROPPED because `out.len() >= max_per_input` (crates/extract/src/memory.rs)
    //   — "the cap actually cut something". That is what makes `exactly 32`
    //   distinguishable from `capped at 32`: a session that legitimately yields 32
    //   leaves it false.
    // * `pass.turns_skipped` is the number of LEADING turns the pass did not read
    //   (the `max_turns` window), counted over the turns SELECTED above. It is the
    //   same fact the old expression approximated, TAKEN FROM THE PASS rather than
    //   recomputed at the call site, so the two cannot disagree about what the pass
    //   did — and it is kept, because dropping it would make this flag newly false
    //   for a genuinely cut transcript, the same class of lie in the other
    //   direction.
    // * `pass.bytes_skipped` is the transcript's BYTE budget (t132, closing the
    //   t125 F1 finding): the turns `max_input_bytes` pushed out of the window,
    //   plus the cut tail of the boundary turn when only its head fitted. The pass
    //   raises `pass.truncated` for it as well, so this predicate covers it
    //   WITHOUT a second expression here — and the daemon does not cut bytes
    //   itself. `turn_for_extraction` below filters platform prompts; it does not
    //   size what it hands in. One bound, one door, one report: a caller that
    //   sized the text here would be a second source of truth for a limit the
    //   limit set already owns, and its cut would be invisible to `truncated`,
    //   which is the defect class t9 was about.
    //
    // NOT changed here: the cap and the rules. This is a reporting fix.
    ExtractBundle {
        memories: pass.candidates,
        entities: Vec::new(),
        relations: Vec::new(),
        source: Some(ExtractSource::Rules),
        truncated: pass.truncated || pass.turns_skipped > 0,
    }
}

/// The ACP half: today's `ask_agent` + `parse_extraction`, mapped onto the seam.
///
/// The wire contract (`Extraction`/`ExtractedMemory`/`ExtractedEntity`/
/// `ExtractedRelation` and `EXTRACTION_PROMPT`) is UNCHANGED — no prompt change,
/// no extraction-value drift. The only new code is the mapping below.
pub struct AcpExtractor<'a> {
    pub distiller: &'a Distiller,
    pub card: &'a ruagent_core::AgentCard,
}

impl AcpExtractor<'_> {
    /// One chat turn, one JSON object. Nothing else.
    pub async fn extract(&self, cx: &ExtractCtx<'_>) -> Result<ExtractBundle, ExtractError> {
        let prompt = format!("{}{}", self.distiller.compose_prompt(), cx.transcript);
        let raw = self.ask(&prompt).await?;
        let extraction = crate::distill::parse_extraction(&raw)
            .map_err(|e| ExtractError::Malformed(format!("{e:#}")))?;
        Ok(bundle_of_acp(&extraction, cx.session_key))
    }

    async fn ask(&self, prompt: &str) -> Result<String, ExtractError> {
        self.distiller
            .ask_agent(self.card, prompt)
            .await
            .map_err(|e| ExtractError::Acp {
                agent: self.card.name.clone(),
                reason: format!("{e:#}"),
            })
    }
}

/// Today's wire structs, mapped onto the seam's candidates.
///
/// Two deliberate mappings, both recorded in the t4 report:
/// * `confidence: None` → `Unconfirmed` (0.8) — byte-identical to today, because
///   that is already what the writer did;
/// * `kind` outside the prompt's vocabulary (`person|project|tool|org|product|
///   concept`) → `None`, because a candidate's kind is a `&'static str`. The
///   graph then stores no kind for that row instead of an unknown string.
///   Behaviour for a compliant model — the only kind the prompt asks for — is
///   identical.
///
/// `provenance` is the session key the candidates came from, carried on the
/// candidate so a later reader can attribute a rule-derived row to its session.
pub(crate) fn bundle_of_acp(
    extraction: &crate::distill::Extraction,
    provenance: &str,
) -> ExtractBundle {
    use ruagent_extract::{CandidateConfidence, CandidateOrigin, RelationName};

    let memories = extraction
        .memories
        .iter()
        .map(|m| MemoryCandidate {
            store: acp_store(&m.store),
            namespace: m.namespace.clone(),
            content: m.content.clone(),
            confidence: match m.confidence {
                Some(v) => CandidateConfidence::Explicit(v),
                None => CandidateConfidence::Unconfirmed,
            },
            rule: "acp",
            origin: CandidateOrigin::Document { section: None },
            source: provenance.to_string(),
        })
        .collect();
    let entities = extraction
        .entities
        .iter()
        .map(|e| EntityCandidate {
            name: e.name.clone(),
            kind: e.kind.as_deref().and_then(acp_kind),
            summary: e.summary.clone(),
            aliases: e.aliases.clone(),
            score: 1.0,
            rule: "acp",
            origin: CandidateOrigin::Document { section: None },
            source: provenance.to_string(),
        })
        .collect();
    let relations = extraction
        .relations
        .iter()
        .map(|r| RelationCandidate {
            src: r.src.clone(),
            dst: r.dst.clone(),
            relation: RelationName::Model(r.relation.clone()),
            fact: r.fact.clone(),
            valid_at: r.valid_at.clone(),
            score: 1.0,
            rule: "acp",
            origin: CandidateOrigin::Document { section: None },
            source: provenance.to_string(),
        })
        .collect();
    ExtractBundle {
        memories,
        entities,
        relations,
        source: Some(ExtractSource::Acp),
        truncated: false,
    }
}

/// The wire store string → the candidate store. An unknown string is
/// `Observation`, which is what `normalize_store` already did with it
/// (distill.rs:848-855), so the write path sees the same store as before.
fn acp_store(store: &str) -> ruagent_extract::CandidateStore {
    match store.trim().to_lowercase().as_str() {
        "profile" => ruagent_extract::CandidateStore::Profile,
        "procedure" => ruagent_extract::CandidateStore::Procedure,
        "lesson" => ruagent_extract::CandidateStore::Lesson,
        _ => ruagent_extract::CandidateStore::Observation,
    }
}

/// The prompt's own kind vocabulary. Anything else is `None` (see above).
fn acp_kind(kind: &str) -> Option<&'static str> {
    match kind.trim().to_lowercase().as_str() {
        "person" => Some("person"),
        "project" => Some("project"),
        "tool" => Some("tool"),
        "org" => Some("org"),
        "product" => Some("product"),
        "concept" => Some("concept"),
        _ => None,
    }
}

/// THE SEAM. Runs every ENABLED source in the fixed order Rules → Acp, merges
/// the bundles (first candidate wins) and reports which ran.
///
/// `acp` is required exactly when `plan.acp` is set: a plan that does not enable
/// the llm tier never touches the agent, so a caller cannot enable it by
/// accidentally passing an extractor.
pub async fn extract(
    cx: &ExtractCtx<'_>,
    plan: ExtractPlan,
    acp: Option<AcpExtractor<'_>>,
) -> Result<ExtractBundle, ExtractError> {
    if plan.is_empty() {
        return Err(ExtractError::NoSourceEnabled);
    }
    let mut out = ExtractBundle::default();
    if plan.rules {
        out.merge(&extract_rules(cx));
    }
    if plan.acp {
        let Some(acp) = acp else {
            // A plan asking for the llm tier without an extractor is a
            // programming error, and it must not silently degrade to the free
            // tier: naming it is the only honest answer.
            return Err(ExtractError::Acp {
                agent: String::new(),
                reason: "the plan enables the ACP tier but no extractor was supplied".into(),
            });
        };
        out.merge(&acp.extract(cx).await?);
    }
    Ok(out.dedup())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distill::{AgentRegistry, AutoDistill, Distiller};
    use ruagent_store::Db;

    fn plane(text: &str) -> CapabilityPlane {
        let policy = ruagent_policy::PolicyConfig::parse(text).expect("policy text parses");
        CapabilityPlane::from_policy(&policy).expect("plane builds")
    }

    fn ctx<'a>(turns: &'a [Turn], transcript: &'a str, limits: ExtractLimits) -> ExtractCtx<'a> {
        ExtractCtx {
            session_key: "s-1",
            transcript,
            turns,
            limits,
        }
    }

    fn turn(role: ruagent_extract::Role, text: &str) -> Turn {
        Turn {
            role,
            text: text.to_string(),
            ts_ms: 0,
        }
    }

    /// t11: a turn that IS one of the platform's own job prompts is not read as
    /// the user's words.
    ///
    /// The markers in the two prompt turns below are the point: the extraction
    /// prompt's own few-shot examples are what the rules were matching
    /// (`If the user CORRECTED …` became a `user_correction`), so the test puts the
    /// same marker in all three turns and asserts that only the user's own turn
    /// reaches the extractor.
    #[test]
    fn a_job_prompt_turn_is_not_the_users_words() {
        let extraction_prompt = format!(
            "{}. Analyze the agent session transcript below. If the user CORRECTED the agent \
             (\"不对\"), extract the corrected fact. 以后统一用工具A处理这件事。",
            OUR_JOB_PROMPT_HEADS[0]
        );
        let planner_prompt = format!(
            "{}\n\nYou are a wiki planning engine. 以后统一用工具B处理这件事。",
            OUR_JOB_PROMPT_HEADS[1]
        );
        let user_turn = "以后统一用工具C处理这件事。";
        assert!(
            is_platform_text(&extraction_prompt) && is_platform_text(&planner_prompt),
            "both job prompts are the platform's own text"
        );
        assert!(!is_platform_text(user_turn), "the user's turn is not");

        let turns = vec![
            turn(ruagent_extract::Role::User, &extraction_prompt),
            turn(ruagent_extract::Role::User, &planner_prompt),
            turn(ruagent_extract::Role::User, user_turn),
        ];
        let limits = ExtractLimits::default();
        let raw = ruagent_extract::memory_candidates_with(&turns, &limits);
        assert!(
            raw.candidates.len() > 1,
            "the prompt turns DO produce candidates on their own — {} of them — which is why the \
             rule cannot be a marker rule",
            raw.candidates.len()
        );
        let bundle = extract_rules(&ctx(&turns, "transcript", limits));
        assert_eq!(
            bundle.memories.len(),
            1,
            "only the user's own turn is read: {:#?}",
            bundle
                .memories
                .iter()
                .map(|c| c.content.chars().take(40).collect::<String>())
                .collect::<Vec<_>>()
        );
        assert!(
            bundle.memories[0].content.contains("工具C"),
            "the surviving candidate is the user's"
        );
    }

    /// The case that must NOT be a drop: a turn carrying an injected block AND
    /// the user's own words. `sessions::user_text` splits the block off and the
    /// user's part is read — dropping the whole turn here would be the "junk-rate
    /// win bought by losing user text" this change must not make.
    #[test]
    fn the_user_side_of_an_injected_turn_is_still_read() {
        let injected_with_user_text = format!(
            "{HDR}] You are the platform role. [memory context — what the platform remembers]\n\
             remembered things\n{}\n---\n以后统一用工具G处理这件事。",
            crate::chat::USER_TEXT_SENTINEL,
            HDR = crate::chat::HDR_ROLE
        );
        assert!(
            is_platform_text(&injected_with_user_text),
            "the turn DOES start with our injected block"
        );
        let turns = vec![turn(ruagent_extract::Role::User, &injected_with_user_text)];
        let limits = ExtractLimits::default();
        let bundle = extract_rules(&ctx(&turns, "transcript", limits));
        assert_eq!(
            bundle.memories.len(),
            1,
            "the user's words after the sentinel are still read"
        );
        assert!(
            bundle.memories[0].content.contains("工具G"),
            "and they are the user's, without the injected block: {:?}",
            bundle.memories[0].content
        );
        assert!(
            !bundle.memories[0].content.contains("remembered things"),
            "the injected body is gone"
        );
    }

    /// A turn that is ONLY an injected block has nothing of the user's in it and
    /// is skipped.
    #[test]
    fn an_injected_block_with_no_user_text_is_skipped() {
        let injected_only = format!(
            "{HDR}] You are the platform role. 以后统一用工具H处理这件事。",
            HDR = crate::chat::HDR_ROLE
        );
        let turns = vec![turn(ruagent_extract::Role::User, &injected_only)];
        let limits = ExtractLimits::default();
        let raw = ruagent_extract::memory_candidates_with(&turns, &limits);
        assert!(!raw.candidates.is_empty(), "the raw pass would extract it");
        let bundle = extract_rules(&ctx(&turns, "transcript", limits));
        assert!(bundle.memories.is_empty(), "the injected block is skipped");
    }

    /// STATS-WITH, NEVER CONTAINS: a user who quotes one of our prompts is
    /// talking to us. Dropping their message for mentioning ours is the substring
    /// mistake this codebase has already paid for once.
    #[test]
    fn a_message_that_only_mentions_our_prompt_is_left_alone() {
        let quoted = format!(
            "以后统一用工具D处理这件事，就像 {} 里说的那样。",
            OUR_JOB_PROMPT_HEADS[0]
        );
        assert!(
            !is_platform_text(&quoted),
            "the head is inside the user's sentence, not at its start"
        );
        let turns = vec![turn(ruagent_extract::Role::User, &quoted)];
        let limits = ExtractLimits::default();
        let bundle = extract_rules(&ctx(&turns, "transcript", limits));
        assert_eq!(bundle.memories.len(), 1, "the user's message is still read");
    }

    /// The OTHER registry: the injected context blocks `chat.rs` emits. They fire
    /// 0 times on the measured corpus, and this pins that the seam consults them
    /// from their one source (`chat::INJECTED_HEADERS`) rather than re-spelling
    /// them.
    #[test]
    fn the_injected_context_blocks_are_ours_too() {
        for head in crate::chat::INJECTED_HEADERS {
            let injected = format!("{head}] body the daemon injected. 以后统一用工具E处理这件事。");
            assert!(is_platform_text(&injected), "not recognised: {head}");
            let turns = vec![
                turn(ruagent_extract::Role::User, &injected),
                turn(ruagent_extract::Role::User, "以后统一用工具F处理这件事。"),
            ];
            let limits = ExtractLimits::default();
            let bundle = extract_rules(&ctx(&turns, "transcript", limits));
            assert_eq!(
                bundle.memories.len(),
                1,
                "the injected block is skipped, the user's turn is not ({head})"
            );
            assert!(bundle.memories[0].content.contains("工具F"));
        }
    }

    /// The common case: a session that carries none of our text is read EXACTLY as
    /// the pass would have read it. This is the assertion that would go red if the
    /// exclusion ever widened beyond the registry.
    #[test]
    fn a_session_without_our_text_is_read_unchanged() {
        let turns: Vec<Turn> = (0..8)
            .map(|i| {
                turn(
                    ruagent_extract::Role::User,
                    &format!("以后统一用工具{i}处理这件事。"),
                )
            })
            .collect();
        let limits = ExtractLimits::default();
        let raw = ruagent_extract::memory_candidates_with(&turns, &limits);
        let bundle = extract_rules(&ctx(&turns, "transcript", limits));
        assert_eq!(raw.candidates.len(), 8);
        assert_eq!(
            bundle.memories.len(),
            raw.candidates.len(),
            "nothing was dropped from a session with no platform text"
        );
        assert_eq!(bundle.truncated, raw.truncated || raw.turns_skipped > 0);
    }

    /// The literals above live in files this change does not own, and a private
    /// `const` is not reachable from a sibling module — so the drift risk that
    /// copying a head normally carries is closed here instead: this reads both
    /// prompt files at COMPILE time and fails the moment either prompt stops
    /// starting with its registered head (a reworded prompt would otherwise
    /// silently stop being excluded).
    #[test]
    fn the_registered_prompt_heads_are_still_our_own_prompts() {
        let distill = include_str!("distill.rs");
        let wiki = include_str!("wiki.rs");
        assert!(
            distill.contains(&format!("{}. ", OUR_JOB_PROMPT_HEADS[0])),
            "distill.rs's EXTRACTION_PROMPT no longer starts with the registered head: {}",
            OUR_JOB_PROMPT_HEADS[0]
        );
        assert!(
            wiki.contains(&format!("{}\n", OUR_JOB_PROMPT_HEADS[1])),
            "wiki.rs's PLANNER_PROMPT no longer starts with the registered head: {}",
            OUR_JOB_PROMPT_HEADS[1]
        );
        // t23: the page writer's prompt is the third member, and it is the one the
        // registry was missing until this task -- it is built by `writer_prompt(slug)`,
        // so the literal to look for is its template head.
        assert!(
            wiki.contains("WIKI PAGE WRITER (slug:"),
            "wiki.rs's writer_prompt no longer starts with the registered head: {}",
            OUR_JOB_PROMPT_HEADS[2]
        );
        assert!(
            !OUR_JOB_PROMPT_HEADS.iter().any(|h| h.trim().is_empty()),
            "an empty head would match every turn"
        );
    }

    /// t23: **the marker is the rule.** A prompt nobody registered a head for -- the NEXT
    /// unattended prompt -- is dropped because `ask_agent` marked it when it was sent, so
    /// coverage does not depend on anyone remembering to edit the registry.
    ///
    /// The other half is asserted too, because it is the way this rule could do harm: a
    /// turn where the USER's words FOLLOW a platform prompt keeps those words.
    #[test]
    fn a_marked_prompt_is_dropped_even_with_an_unregistered_head() {
        let sentinel = crate::chat::USER_TEXT_SENTINEL;
        let durable = "以后统一用工具D处理这件事。";
        // A prompt this build has never heard of: an unregistered head, sent through
        // `ask_agent` (which appends the marker), logged by the agent CLI.
        let future_prompt =
            format!("FUTURE UNATTENDED JOB v2\nDo the job.\n\nINPUT:\n{durable}\n{sentinel}");
        assert!(
            !is_platform_text(crate::sessions::user_text(&future_prompt)),
            "the head must really be unregistered, otherwise this test proves nothing"
        );
        assert!(
            !ruagent_extract::memory_candidates_with(
                &[turn(ruagent_extract::Role::User, &future_prompt)],
                &ExtractLimits::default()
            )
            .candidates
            .is_empty(),
            "the unread prompt WOULD yield candidates, so dropping it is a real change"
        );
        assert!(
            turn_for_extraction(&turn(ruagent_extract::Role::User, &future_prompt)).is_none(),
            "a marked platform prompt must not reach the tier, whatever it says"
        );

        // REGRESSION GUARD (t21's control): the marker must not strip the user's words
        // when they FOLLOW a platform prompt -- injected block, marker, then the user.
        let mixed = format!("[memory context — 3 items]\n\n{sentinel}\n---\n{durable}");
        assert_eq!(
            turn_for_extraction(&turn(ruagent_extract::Role::User, &mixed)).map(|k| k.text),
            Some(durable.to_string()),
            "the user's words after the marker must survive the split"
        );

        // RESIDUE (rule 2): the wiki PAGE WRITER prompt, as it was written to disk
        // BEFORE the marker existed, is covered by the registry -- the family t23 closes.
        let writer = "WIKI PAGE WRITER (slug: x)\n\nYou are a wiki page writer.\n\n<sources>\n\
                      <chunk id=\"1\">\n以后统一用工具E处理这件事。\n</chunk>\n</sources>";
        assert!(
            turn_for_extraction(&turn(ruagent_extract::Role::User, writer)).is_none(),
            "the writer family must not reach the tier (residue rule)"
        );
    }

    /// Lines that send an agent prompt WITHOUT the daemon's marker. t23's drift guard is
    /// built on this, and it is a function so the CONTROL below can prove it can fail.
    fn unmarked_prompt_sends(src: &str) -> Vec<usize> {
        let lines: Vec<&str> = src.lines().collect();
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("ChatCommand::Prompt") {
                continue;
            }
            let window = lines[i..(i + 3).min(lines.len())].join("\n");
            if !window.contains("platform_prompt(") {
                out.push(i + 1);
            }
        }
        out
    }

    /// t23: **every unattended prompt is covered, and the CHECK does not lean on memory.**
    ///
    /// Two invariants over the daemon's SOURCES, so a new prompt added later is covered
    /// without anyone editing a list:
    ///
    /// 1. THE MARKER CANNOT BE BYPASSED. Outside `chat.rs` (the user's OWN chat, which
    ///    composes injected blocks + the marker + the user's text, so it is not a sender
    ///    of unattended prompts), every prompt that reaches an agent is sent by
    ///    `ask_agent`, and that one send carries `platform_prompt(`. A new module that
    ///    talks to an agent directly fails here -- including a new FILE, because the scan
    ///    walks the directory rather than a list of names.
    /// 2. THE RESIDUE REGISTRY still names the prompts the senders compose, so the family
    ///    already written to disk stays covered.
    ///
    /// The scan is over the real sources; step 3 is the control that shows what failure
    /// looks like, without planting anything in the shared tree.
    #[test]
    fn every_agent_prompt_sender_is_covered_or_marked() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = 0usize;
        for entry in std::fs::read_dir(&dir)
            .expect("the daemon's source directory")
            .flatten()
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            // chat.rs is the user's own path, by name and for a stated reason.
            if name == "chat.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("readable source");
            // Only the code that SHIPS: a test module's own literals mention
            // `ChatCommand::Prompt` (the control at the end of this test does), and none
            // of them is a sender. Truncating at the test module keeps the check about
            // the daemon.
            let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
            let bad = unmarked_prompt_sends(production);
            assert!(
                bad.is_empty(),
                "{name} sends an agent prompt the daemon's marker does not cover, at line(s) {bad:?} \
                 -- an unattended prompt must go through `ask_agent`, which marks it"
            );
            scanned += 1;
        }
        assert!(
            scanned > 10,
            "the scan must actually walk the daemon's sources; it walked {scanned}"
        );

        // 2. the registry's members are still the senders' prompts.
        for (file, src, needle) in [
            (
                "distill.rs",
                include_str!("distill.rs"),
                OUR_JOB_PROMPT_HEADS[0],
            ),
            ("wiki.rs", include_str!("wiki.rs"), OUR_JOB_PROMPT_HEADS[1]),
            ("wiki.rs", include_str!("wiki.rs"), OUR_JOB_PROMPT_HEADS[2]),
        ] {
            assert!(
                src.contains(needle),
                "{file} no longer composes the registered prompt head {needle:?}"
            );
        }

        // 3. THE CONTROL: a bypassing sender IS detected, so an empty result above means
        //    something. (Planted here, never in the tree.)
        let planted = "fn a_new_unattended_sender() -> Result<()> {\n    \
                       session.send(ChatCommand::Prompt {\n        \
                       text: prompt.to_string(),\n        context: None,\n    })?;\n    Ok(())\n}\n";
        assert_eq!(
            unmarked_prompt_sends(planted),
            vec![2],
            "the scanner must fail on a sender that skips the marker, otherwise the check is vacuous"
        );
    }

    /// A distiller that CANNOT run an agent: a `root` that is a FILE, so the
    /// workspace it creates before spawning fails, and an empty registry. If the
    /// seam entered the ACP path at all, the error would be `Acp { .. }`, never
    /// `NoSourceEnabled`.
    async fn unusable_acp() -> (Distiller, ruagent_core::AgentCard) {
        let root = std::env::temp_dir().join(format!(
            "ruagent-t4-seam-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // Deliberately NOT a directory: `ask_agent` creates
        // `<root>/workspaces/distill` before it spawns anything, and that fails
        // here — a run that reached the ACP path cannot succeed by accident.
        std::fs::write(&root, b"not a directory").unwrap();
        let distiller = Distiller {
            db: Db::open_in_memory().unwrap(),
            root,
            embedder: None,
            registry: AgentRegistry::default(),
            language: None,
            prompt_override: None,
            graph: true,
        };
        let card = ruagent_core::AgentCard {
            id: ruagent_core::AgentId::generate(),
            name: "cannot-run".into(),
            harness: ruagent_core::HarnessKind::Mock,
            command: Some("ruagent-no-such-agent-binary".into()),
            description: String::new(),
            model: None,
            reasoning_effort: None,
            context_window: None,
            mcp_profile: None,
            models: Vec::new(),
            prompt: None,
            runtime: None,
            runtimes: Vec::new(),
            options: Default::default(),
            tags: Vec::new(),
            enabled: true,
        };
        (distiller, card)
    }

    /// ACCEPTANCE: with the DEFAULT configuration nothing is enabled, and the
    /// call is a loud refusal — not a silent ACP run.
    #[test]
    fn the_default_configuration_enables_no_extractor_at_all() {
        let legacy = CapabilityPlane::legacy();
        let plan = ExtractPlan::unattended(&legacy, false);
        assert!(!plan.rules, "the free tier is new and opt-in");
        assert!(!plan.acp, "[distill].auto is off by default (law L1)");
        assert!(plan.is_empty());
        assert_eq!(
            plan.label(),
            "acp",
            "the label names what was asked, not what ran"
        );

        // A present-but-empty table is the same answer: an llm tier defaults off.
        let present = plane("[capabilities]\n");
        assert!(ExtractPlan::unattended(&present, false).is_empty());
        // …and the gate can only NARROW: `auto = true` is not enough.
        assert!(
            !ExtractPlan::unattended(&present, true).acp,
            "law L2: a capability may only suppress what the legacy flag asked for"
        );
        assert!(
            !ExtractPlan::unattended(&legacy, false).acp,
            "law L1: legacy mode answers with today's behaviour, and today nothing runs"
        );
    }

    /// ACCEPTANCE: no ACP run, no agent spawn and no network call from this path
    /// under the default configuration. The `AcpExtractor` handed in points at a
    /// distiller that cannot run an agent; the seam must never reach it.
    #[tokio::test]
    async fn no_source_enabled_is_a_refusal_and_never_touches_the_acp_path() {
        let (distiller, card) = unusable_acp().await;
        let turns = vec![turn(
            ruagent_extract::Role::User,
            "记住：以后统一用简体中文。",
        )];
        let transcript = "[01-01 00:00] User: 记住：以后统一用简体中文。";
        let plan = ExtractPlan::unattended(&CapabilityPlane::legacy(), false);
        let cx = ctx(&turns, transcript, plan.limits);

        let err = extract(
            &cx,
            plan,
            Some(AcpExtractor {
                distiller: &distiller,
                card: &card,
            }),
        )
        .await
        .expect_err("the default configuration extracts nothing");

        assert_eq!(err, ExtractError::NoSourceEnabled, "{err}");
    }

    /// ACCEPTANCE #4: the plane — not the call site — decides which
    /// implementation runs. The SAME call, with the same unusable ACP extractor,
    /// succeeds on the free tier and fails on the llm tier.
    #[tokio::test]
    async fn the_plane_selects_the_implementation_and_the_call_site_does_not_branch() {
        let (distiller, card) = unusable_acp().await;
        let turns = vec![turn(
            ruagent_extract::Role::User,
            "记住：以后统一用简体中文。",
        )];
        let transcript = "[01-01 00:00] User: 记住：以后统一用简体中文。";

        // Free tier on (new capability, enabled explicitly).
        let free_plane = plane("[capabilities.session_extract_rules]\nenabled = true\n");
        let free_plan = ExtractPlan::unattended(&free_plane, false);
        assert!(free_plan.rules && !free_plan.acp, "{free_plan:?}");
        let cx = ctx(&turns, transcript, free_plan.limits);
        let bundle = extract(
            &cx,
            free_plan,
            Some(AcpExtractor {
                distiller: &distiller,
                card: &card,
            }),
        )
        .await
        .expect("the free tier needs no agent at all");
        assert_eq!(bundle.source, Some(ExtractSource::Rules));
        assert!(
            !bundle.memories.is_empty(),
            "the deterministic rules fired on a remember-that turn: {bundle:?}"
        );

        // Llm tier on: the same call now goes to the ACP path (which cannot run
        // here), proving the selection is the PLAN's and not the caller's.
        let llm_plane = plane("[capabilities.distill_session]\nenabled = true\n");
        let llm_plan = ExtractPlan::unattended(&llm_plane, true);
        assert!(!llm_plan.rules && llm_plan.acp, "{llm_plan:?}");
        let cx = ctx(&turns, transcript, llm_plan.limits);
        let err = extract(
            &cx,
            llm_plan,
            Some(AcpExtractor {
                distiller: &distiller,
                card: &card,
            }),
        )
        .await
        .expect_err("the ACP tier cannot run without an agent");
        assert!(matches!(err, ExtractError::Acp { .. }), "{err}");
    }

    /// A plan that asks for the llm tier without an extractor must not degrade
    /// silently to the free tier.
    #[tokio::test]
    async fn an_acp_plan_without_an_extractor_is_refused() {
        let turns: Vec<Turn> = Vec::new();
        let plan = ExtractPlan::acp_only();
        let cx = ctx(&turns, "", plan.limits);
        let err = extract(&cx, plan, None).await.expect_err("no extractor");
        assert!(matches!(err, ExtractError::Acp { .. }), "{err}");
    }

    #[test]
    fn dedup_keeps_the_first_candidate_of_each_identity() {
        let first = ExtractBundle {
            memories: vec![MemoryCandidate {
                store: ruagent_extract::CandidateStore::Profile,
                namespace: "user".into(),
                content: "Use Chinese.".into(),
                confidence: ruagent_extract::CandidateConfidence::Confirmed,
                rule: "user_preference",
                origin: Default::default(),
                source: "s-1".into(),
            }],
            source: Some(ExtractSource::Rules),
            ..ExtractBundle::default()
        };
        let mut paraphrased = first.clone();
        paraphrased.memories[0].content = "use  chinese.".into();
        paraphrased.source = Some(ExtractSource::Acp);

        let mut merged = first.clone();
        merged.merge(&paraphrased);
        assert_eq!(merged.memories.len(), 1, "same identity: {merged:?}");
        assert_eq!(merged.memories[0].rule, "user_preference");
        assert_eq!(
            merged.source,
            Some(ExtractSource::Acp),
            "the last source that contributed is reported"
        );
    }

    #[test]
    fn explicit_plans_run_what_the_caller_asked_for() {
        assert_eq!(ExtractPlan::explicit(Extractor::Acp).label(), "acp");
        assert_eq!(ExtractPlan::explicit(Extractor::Rules).label(), "rules");
        assert_eq!(ExtractPlan::explicit(Extractor::Both).label(), "rules+acp");
        assert_eq!(
            Extractor::parse("RULES").unwrap(),
            Extractor::Rules,
            "the value is case-insensitive"
        );
        let err = Extractor::parse("llm").unwrap_err();
        assert!(err.to_string().contains("llm"), "{err}");
        assert!(
            err.to_string().contains("rules"),
            "lists the accepted values: {err}"
        );
    }

    /// `[distill].graph` and the seam's bundle are the same three stores: the
    /// ACP mapping produces the shape the writer already consumed.
    #[test]
    fn the_acp_mapping_keeps_todays_values() {
        let extraction = crate::distill::Extraction {
            memories: vec![crate::distill::ExtractedMemory {
                store: "profile".into(),
                namespace: "user".into(),
                content: "x".into(),
                confidence: None,
            }],
            entities: vec![crate::distill::ExtractedEntity {
                name: "Acme".into(),
                kind: Some("org".into()),
                summary: None,
                aliases: vec!["ACME".into()],
            }],
            relations: vec![crate::distill::ExtractedRelation {
                src: "Acme".into(),
                dst: "Bob".into(),
                relation: "employs".into(),
                fact: "Acme employs Bob.".into(),
                valid_at: Some("2026-01-01T00:00:00Z".into()),
            }],
        };
        let bundle = bundle_of_acp(&extraction, "ruagent:t4");
        assert_eq!(bundle.source, Some(ExtractSource::Acp));
        assert_eq!(
            bundle.memories[0].confidence,
            ruagent_extract::CandidateConfidence::Unconfirmed,
            "None confidence was 0.8 (unconfirmed) before this change too"
        );
        assert_eq!(bundle.entities[0].kind, Some("org"));
        assert_eq!(bundle.entities[0].aliases, vec!["ACME".to_string()]);
        assert_eq!(
            bundle.relations[0].relation,
            ruagent_extract::RelationName::Model("employs".into()),
            "an LLM relation is never routed through the rule table"
        );
        assert_eq!(
            bundle.relations[0].valid_at.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
    }

    /// An out-of-vocabulary kind is the one ACP value the seam cannot carry
    /// (`&'static str`); it becomes "no kind", never an invented one.
    #[test]
    fn an_unknown_acp_kind_becomes_no_kind() {
        let extraction = crate::distill::Extraction {
            entities: vec![crate::distill::ExtractedEntity {
                name: "Tokio".into(),
                kind: Some("framework".into()),
                summary: None,
                aliases: Vec::new(),
            }],
            ..Default::default()
        };
        assert_eq!(bundle_of_acp(&extraction, "").entities[0].kind, None);
    }

    /// `AutoDistill`'s default is the shipped policy: nothing runs unattended.
    #[test]
    fn the_shipped_distill_policy_is_off() {
        assert!(!AutoDistill::default().auto);
    }

    // -----------------------------------------------------------------------
    // t9: the truncation flag tells the truth about the MEMORY PASS
    // -----------------------------------------------------------------------

    /// One user turn the memory rules fire on, with content unique per `n` so the
    /// pass cannot dedup two of them into one candidate.
    fn memory_turn(n: usize) -> Turn {
        turn(
            ruagent_extract::Role::User,
            &format!("记住：约定编号 {n}，发布流程使用 scripts/release-{n}.sh 这一步。"),
        )
    }

    /// THE t9 REGRESSION, and it is also the test that fails if the flag goes back
    /// to `turns.len() > max_turns`: 60 turns is far below `max_turns` (2000), so
    /// that expression is false while the pass really did drop candidates at the
    /// 32 cap. The measurement found exactly this shape on 333 real sessions.
    #[test]
    fn a_session_cut_at_the_candidate_cap_reports_truncated() {
        let limits = ExtractLimits::default();
        let turns: Vec<Turn> = (0..60).map(memory_turn).collect();
        let pass = ruagent_extract::memory_candidates_with(&turns, &limits);
        let bundle = extract_rules(&ctx(&turns, "transcript", limits));

        assert!(
            turns.len() < limits.max_turns,
            "the turn-count predicate had to be false here, or this test proves nothing"
        );
        assert_eq!(
            pass.candidates.len(),
            limits.max_per_input,
            "the pass fills the cap: {pass:?}"
        );
        assert!(
            pass.truncated,
            "…and it dropped candidates to get there: {pass:?}"
        );
        assert_eq!(bundle.memories.len(), limits.max_per_input);
        assert!(
            bundle.truncated,
            "a candidate list cut at the cap must say so: {bundle:?}"
        );
    }

    /// `exactly N candidates` stays DISTINGUISHABLE from `capped at N`: filling the
    /// cap exactly drops nothing, so the flag stays false. This is the case the
    /// measurement could not separate (16 sessions reported exactly 32).
    #[test]
    fn a_session_that_fills_the_cap_exactly_is_not_truncated() {
        let limits = ExtractLimits::default();
        let turns: Vec<Turn> = (0..limits.max_per_input).map(memory_turn).collect();
        let pass = ruagent_extract::memory_candidates_with(&turns, &limits);
        let bundle = extract_rules(&ctx(&turns, "transcript", limits));

        assert_eq!(pass.candidates.len(), limits.max_per_input, "{pass:?}");
        assert!(!pass.truncated, "nothing was dropped: {pass:?}");
        assert_eq!(bundle.memories.len(), limits.max_per_input);
        assert!(
            !bundle.truncated,
            "32 candidates from 32 turns is everything the pass produced"
        );
    }

    /// The OTHER loss the old expression stood for is still reported, and now it
    /// comes from the pass's own `turns_skipped` rather than a recomputation.
    #[test]
    fn a_transcript_window_that_drops_turns_is_still_reported() {
        let limits = ExtractLimits {
            max_turns: 4,
            ..ExtractLimits::default()
        };
        let turns: Vec<Turn> = (0..10).map(memory_turn).collect();
        let pass = ruagent_extract::memory_candidates_with(&turns, &limits);
        assert_eq!(
            pass.turns_skipped, 6,
            "the window read only the last 4 turns"
        );
        assert!(
            extract_rules(&ctx(&turns, "transcript", limits)).truncated,
            "6 leading turns were not read, so this is not everything the session had"
        );
    }
}
