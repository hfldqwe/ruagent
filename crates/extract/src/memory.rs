//! Algorithm A (design §8): a session transcript becomes memory candidates.
//! Zero tokens, no model, no I/O.
//!
//! The rules are marker-driven and precision-first: a sentence must clear the
//! §8.7 guards (a real content token, not a question, within the body cap)
//! before any marker is even consulted. Marker matching alone is never
//! sufficient, and every confidence value is a SIGNAL here — the number is
//! chosen by the applier through `ruagent_memory::confidence::confidence()`
//! (crates/memory/src/confidence.rs:100).

use std::collections::BTreeSet;

use crate::rules;
use crate::text;
use crate::{
    CandidateConfidence, CandidateOrigin, CandidateStore, ExtractLimits, MemoryCandidate, Role,
    Turn,
};

/// One transcript pass, with the counters the caller reports (design §7.4.1,
/// §8.2). [`memory_candidates`] returns only the candidates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MemoryCandidates {
    pub candidates: Vec<MemoryCandidate>,
    /// How many candidates the §8.2 correction suppression removed.
    pub suppressed: usize,
    /// Which rules those candidates came from, in rule-id order — the audit
    /// trail for `suppressed` (design §8.6's `expect_suppressed`). Sorted rather
    /// than left in generation order so the counter reads the same way whatever
    /// order the rules happened to run in.
    pub suppressed_rules: Vec<&'static str>,
    /// True when the deduped list was cut to `max_per_input` (§8.5).
    pub truncated: bool,
    /// Leading turns dropped because only the last `max_turns` are read (§7.4.1).
    pub turns_skipped: usize,
}

/// Session transcript -> memory candidates. Deterministic, bounded, pure.
pub fn memory_candidates(turns: &[Turn], limits: &ExtractLimits) -> Vec<MemoryCandidate> {
    memory_candidates_for("", turns, limits).candidates
}

/// The same pass, with its counters. `memory_candidates` is this function's
/// `.candidates`, so the two entry points can never disagree.
pub fn memory_candidates_with(turns: &[Turn], limits: &ExtractLimits) -> MemoryCandidates {
    memory_candidates_for("", turns, limits)
}

/// The same pass, stamping `source` (the caller's own provenance label — a
/// session key) onto every candidate it emits.
pub fn memory_candidates_for(
    source: &str,
    turns: &[Turn],
    limits: &ExtractLimits,
) -> MemoryCandidates {
    // §7.4.1 windowing: keep the LAST max_turns turns.
    let start = turns.len().saturating_sub(limits.max_turns);
    let window = &turns[start..];

    let mut found: Vec<(usize, MemoryCandidate)> = Vec::new();
    let mut suppressed_rules: Vec<&'static str> = Vec::new();

    for (offset, turn) in window.iter().enumerate() {
        let index = start + offset;
        let turn_lower = text::lower(&turn.text);
        let has_confirm = text::contains_any(&turn_lower, rules::CONFIRM);
        let has_hedge = text::contains_any(&turn_lower, rules::HEDGE);
        let candidates = turn_candidates(
            turn,
            index,
            source,
            has_confirm,
            has_hedge,
            limits.max_content_bytes,
        );

        // §8.2: a user turn that CORRECTS the agent suppresses the candidates
        // derived from the assistant turn immediately before it — "extract the
        // corrected fact from the user's message, NOT the agent's wrong answer"
        // (distill.rs:19). One-turn lookback only; a longer window is not a rule,
        // it is a guess. They are counted, not returned.
        let corrected_next = window
            .get(offset + 1)
            .map(|next| {
                next.role == Role::User
                    && text::contains_any(&text::lower(&next.text), rules::CORRECT)
            })
            .unwrap_or(false);
        if turn.role == Role::Assistant && corrected_next {
            suppressed_rules.extend(candidates.iter().map(|c| c.rule));
            continue;
        }

        found.extend(candidates.into_iter().map(|c| (index, c)));
    }

    // The audit trail is sorted, so it does not depend on the order the rules
    // happened to run in.
    suppressed_rules.sort();

    // §7.4.5 ordering: occurrence index ascending, then rule id ascending. The
    // sort is stable, so sentences of one rule keep their order in the turn.
    found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.rule.cmp(b.1.rule)));

    // §7.4.6 dedup within a run: (rule, normalized content); first occurrence
    // wins, so a repeated sentence yields one candidate and first-mention order
    // survives. The key is the SAME `dedup_key()` the applier gets, so the two
    // crates cannot drift apart.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<MemoryCandidate> = Vec::new();
    let mut truncated = false;
    for (_, candidate) in found {
        if !seen.insert(candidate.dedup_key()) {
            continue;
        }
        if out.len() >= limits.max_per_input {
            truncated = true;
            continue;
        }
        out.push(candidate);
    }

    MemoryCandidates {
        candidates: out,
        suppressed: suppressed_rules.len(),
        suppressed_rules,
        truncated,
        turns_skipped: start,
    }
}

/// Every candidate one turn produces, before dedup and truncation.
fn turn_candidates(
    turn: &Turn,
    index: usize,
    source: &str,
    has_confirm: bool,
    has_hedge: bool,
    max_content_bytes: usize,
) -> Vec<MemoryCandidate> {
    let ctx = TurnCtx {
        index,
        role: turn.role,
        source,
    };
    let mut out: Vec<MemoryCandidate> = Vec::new();
    for (_, sentence) in text::split_sentences(&turn.text) {
        // §8.7 guards: the body cap, the question filter and the content token.
        // Marker matching alone is never sufficient.
        if sentence.len() > max_content_bytes {
            continue;
        }
        if text::is_question(&sentence) {
            continue;
        }
        if !text::has_content_token(&sentence) {
            continue;
        }
        let sentence_lower = text::lower(&sentence);

        match turn.role {
            Role::User => {
                // `contains_requirement` and not `contains_any` (ruagent-close-the-gaps
                // t32): `PREF` carries the requirement markers, and a containment test
                // fired on text that REPORTS a requirement — a quotation, a third
                // party's statement, a hypothetical, boilerplate attribution, or the
                // substring collision `不能用` inside `能不能用`. The other sets keep the
                // plain containment they were designed with (see `text.rs`).
                let preference = text::contains_requirement(&sentence_lower, rules::PREF);
                let confirmation = text::contains_any(&sentence_lower, rules::CONFIRM);
                if preference || confirmation {
                    // Store (§8.2): profile when the sentence is about the user
                    // (a first-person marker) — and, per the `confirmed_preference`
                    // fixture of §8.6, also when the sentence IS the confirmation:
                    // only the user's own "对" makes it a confirmed preference.
                    // Otherwise the preference is an observation.
                    let store = if confirmation
                        || text::contains_any(&sentence_lower, rules::FIRST_PERSON)
                    {
                        CandidateStore::Profile
                    } else {
                        CandidateStore::Observation
                    };
                    // Confidence (§8.2 table, §8.3): `Confirmed` when this TURN
                    // contains a CONFIRM marker, else `Unconfirmed`. Not a number:
                    // the applier derives 1.0 / 0.8 through memory's one rule.
                    let confidence = if has_confirm {
                        CandidateConfidence::Confirmed
                    } else {
                        CandidateConfidence::Unconfirmed
                    };
                    out.push(candidate(
                        ctx,
                        store,
                        &sentence,
                        confidence,
                        rules::RULE_USER_PREFERENCE,
                    ));
                }
                if text::contains_any(&sentence_lower, rules::CORRECT) {
                    // Confidence: `Corrected`. The corrected fact is the user's
                    // own words, so memory's rule maps it to 1.0 (confidence.rs:15).
                    out.push(candidate(
                        ctx,
                        CandidateStore::Observation,
                        &sentence,
                        CandidateConfidence::Corrected,
                        rules::RULE_USER_CORRECTION,
                    ));
                }
                // `contains_decision` (ruagent-close-the-gaps t32): the bare
                // single-character marker `选` fired inside `可选` — the same
                // word-boundary defect the requirement-marker finding names, so it is
                // closed with the `可+V` stative rule rather than left behind.
                if text::contains_decision(&sentence_lower, rules::DECISION) {
                    // Store (§8.2): procedure when the decision names something
                    // you operate (TOOLISH), else an observation.
                    let store = if text::contains_any(&sentence_lower, rules::TOOLISH) {
                        CandidateStore::Procedure
                    } else {
                        CandidateStore::Observation
                    };
                    // Confidence: `Unconfirmed` — a decision is asserted, not
                    // confirmed (only a CONFIRM marker means that), and not
                    // hedged. The applier maps it to 0.8, today's ordinary value.
                    out.push(candidate(
                        ctx,
                        store,
                        &sentence,
                        CandidateConfidence::Unconfirmed,
                        rules::RULE_USER_DECISION,
                    ));
                }
            }
            Role::Assistant => {
                if text::contains_any(&sentence_lower, rules::PROC) {
                    // Confidence: `Hedged` when the same turn contains a HEDGE
                    // marker (0.4, below LOW_CONFIDENCE, so the panel renders the
                    // doubt), else `Unconfirmed` (0.8). §8.2.
                    let confidence = if has_hedge {
                        CandidateConfidence::Hedged
                    } else {
                        CandidateConfidence::Unconfirmed
                    };
                    out.push(candidate(
                        ctx,
                        CandidateStore::Procedure,
                        &sentence,
                        confidence,
                        rules::RULE_PROCEDURE_NOTE,
                    ));
                }
                if text::contains_any(&sentence_lower, rules::LESSON) {
                    // Confidence: `Hedged` when the turn also hedged, else
                    // `Unconfirmed` — same rule as `procedure_note` (§8.2).
                    let confidence = if has_hedge {
                        CandidateConfidence::Hedged
                    } else {
                        CandidateConfidence::Unconfirmed
                    };
                    out.push(candidate(
                        ctx,
                        CandidateStore::Lesson,
                        &sentence,
                        confidence,
                        rules::RULE_LESSON_LEARNED,
                    ));
                }
            }
        }

        // The hedge rule has no role filter (§8.2): either speaker can express
        // doubt. Confidence is always `Hedged`, and the candidate is KEPT, not
        // dropped — visible doubt beats a silent omission (§8.3).
        if text::contains_any(&sentence_lower, rules::HEDGE) {
            out.push(candidate(
                ctx,
                CandidateStore::Observation,
                &sentence,
                CandidateConfidence::Hedged,
                rules::RULE_HEDGED_STATEMENT,
            ));
        }
    }
    out
}

/// What every rule of one turn shares: where the candidate came from.
#[derive(Debug, Clone, Copy)]
struct TurnCtx<'a> {
    index: usize,
    role: Role,
    source: &'a str,
}

fn candidate(
    ctx: TurnCtx<'_>,
    store: CandidateStore,
    content: &str,
    confidence: CandidateConfidence,
    rule: &'static str,
) -> MemoryCandidate {
    MemoryCandidate {
        store,
        namespace: rules::NAMESPACE_USER.to_string(),
        content: content.to_string(),
        confidence,
        rule,
        origin: CandidateOrigin::Turn {
            index: ctx.index,
            role: ctx.role,
        },
        source: ctx.source.to_string(),
    }
}
