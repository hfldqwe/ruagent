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
    /// True when an input was cut to its bound (`ExtractLimits::max_text_bytes`
    /// for a document, `max_turns` for a transcript).
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
pub fn extract_rules(cx: &ExtractCtx<'_>) -> ExtractBundle {
    ExtractBundle {
        memories: ruagent_extract::memory_candidates(cx.turns, &cx.limits),
        entities: Vec::new(),
        relations: Vec::new(),
        source: Some(ExtractSource::Rules),
        truncated: cx.turns.len() > cx.limits.max_turns,
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
}
