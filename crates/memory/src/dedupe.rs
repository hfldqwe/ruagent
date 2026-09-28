//! Near-duplicate detection for distilled memories (t329).
//!
//! The check this replaces was word-based: `split_whitespace()`, an
//! alphanumeric filter, `filter(|w| w.len() > 2)` and a hit rate of 0.7. For
//! Chinese that is DEAD CODE — a sentence has no whitespace, so it normalises to
//! one token and the hit rate can only be 0 or 1. t323 measured the
//! consequence: 8 of 44 `profile` rows were the same fact in different words.
//!
//! This criterion compares CHARACTERS, and it is fail-closed: a difference is
//! mergeable only when every bit of it is an ignorable word. A polarity token,
//! or any word not recognised as filler, REFUSES the merge — folding
//! "does not use X" into "uses X" is far worse than keeping two rows.
//!
//! Bigrams are deliberately NOT used. On the t323 corpus they turn
//! "进行交流" into the shifted pseudo-words 文交 / 文进 / 行交, so every pair
//! looks different and nothing ever merges.

/// Words that may differ without changing what a memory says: fillers,
/// politeness, and rewrites that restate the same thing.
///
/// Longest first: the strip below removes them in this order so that
/// "不" -style fragments cannot hide inside a longer ignorable word.
pub const IGNORABLE: &[&str] = &[
    "偏好", "进行", "用户", "我们", "一种", "这个", "那个", "就是", "可以", "请", "的", "了", "呢",
    "啊", "吧", "吗", "我", "会", "要", "please", "prefers", "prefer", "users", "user", "the",
    "and", "are", "is", "to", "of", "in", "a", "an",
];

/// Words whose presence anywhere in the difference flips the meaning. A memory
/// that says "does not use X" must never be folded into one that says "uses X".
pub const POLARITY: &[&str] = &[
    "不", "别", "没有", "没", "无需", "无", "禁止", "避免", "取消", "停止", "不再", "从不", "绝不",
    "never", "not", "no", "without", "avoid", "stop", "dont", "don't", "cannot", "can't",
];

/// What the criterion decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The same content, once normalised: nothing to merge, and nothing to add.
    Same,
    /// Only ignorable words differ: the old row may be superseded by the new one.
    Mergeable,
    /// Refused, with a stable reason for the audit trail.
    Refused(&'static str),
}

/// The refusal reasons, as CONSTANTS because callers match on them.
/// A polarity refusal is TERMINAL (never escalated to a judgement): folding
/// "does not use X" into "uses X" is the one outcome this module exists to
/// prevent. A content refusal may be escalated when the embedder says the two
/// rows are near-identical — that is the `NeedsJudgement` path below.
pub const POLARITY_REFUSAL: &str = "a polarity token is in the difference";
pub const CONTENT_REFUSAL: &str = "a content word is in the difference";
pub const EMPTY_REFUSAL: &str = "one side is empty after normalisation";

/// Judge whether `new` may replace `old`.
///
/// Character-based and fail-closed: only a difference made ENTIRELY of
/// ignorable words is mergeable.
pub fn judge(old: &str, new: &str) -> Verdict {
    let (a, b) = (normalize(old), normalize(new));
    if a.is_empty() && b.is_empty() {
        return Verdict::Same;
    }
    if a == b {
        return Verdict::Same;
    }
    if a.is_empty() || b.is_empty() {
        return Verdict::Refused(EMPTY_REFUSAL);
    }
    let only_old = diff(&a, &b);
    let only_new = diff(&b, &a);
    let changed = format!("{only_old}{only_new}");
    if POLARITY.iter().any(|p| contains_token(&changed, p)) {
        return Verdict::Refused(POLARITY_REFUSAL);
    }
    let left_old = leftover(&only_old);
    let left_new = leftover(&only_new);
    if left_old.is_empty() && left_new.is_empty() {
        Verdict::Mergeable
    } else {
        Verdict::Refused(CONTENT_REFUSAL)
    }
}

// ── The merge DECISION (R-B C2, t8) ────────────────────────────────────────
//
// WHY THIS EXISTS ON TOP OF `judge`. `judge` is the fail-closed LEXICAL filter,
// and on the live corpus it is nearly silent: over 2297 live pairs (same
// store+namespace) it returned Mergeable **0** times, while the embedder put 6
// pairs at cosine 0.9827–0.9884 that are plainly the same fact in different
// words ("用户的语音输入法用 F6 作为启动/停止语音输入的快捷键。" vs "…以 F6 键作为
// …的触发键。", measured 2026-09-27T21:50+08:00, R-B A.9). A criterion that is
// precise and blind loses the rewrite; a criterion that merges on similarity
// loses the distinction — 45.19% of live pairs sit at cosine ≥ 0.86 and the
// median pair is 0.85, so an absolute similarity threshold is not an equivalence
// test at all on this corpus.
//
// So the decision has THREE outcomes that a caller can act on and audit:
//   Same             – nothing to add, nothing to merge
//   Merge{rule}      – the lexical filter fired; merge, and say which rule
//   NeedsJudgement   – lexical refused, but the embedder says the two are as
//                      close as rewritten duplicates get: a NAMED judge (the
//                      distiller's LLM) must decide, and the decision is audited
//   New              – no candidate is close enough; insert
//   Refused(reason)  – terminal, never escalated (polarity, empty)

/// One candidate row the new content could replace. `cosine` is the embedder's
/// cosine between `content` and the new content (`None` = no vector for this row,
/// which is NOT the same as 0.0 and must not be read as "unrelated").
#[derive(Debug, Clone, PartialEq)]
pub struct MergeCandidate {
    pub id: i64,
    pub content: String,
    pub cosine: Option<f32>,
}

/// The bounded candidate policy. `top_k` is small on purpose: the measured
/// candidate requirement is 3 — on the 24 live pairs at cosine ≥ 0.95, a top-3
/// neighbourhood recovers 24/24 (R-B A.9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MergeConfig {
    pub top_k: usize,
    /// The scope's own similarity level at which a lexical refusal may be
    /// escalated. MUST come from the scope's measured distribution
    /// (`scope_tau`), never from a literature constant: on the live corpus the
    /// median pair is 0.85, so 0.90 would admit 15.93% of ALL pairs.
    pub tau_scope: f32,
}

/// The percentile `tau_scope` is taken at when a caller derives it. ONE place.
pub const TAU_PERCENTILE: f32 = 0.99;

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            top_k: DEFAULT_TOP_K,
            // The live corpus' p99 measured 2026-09-27T22:01+08:00 (R-B A.9).
            // A caller with its own scope MUST pass scope_tau(&its_own_cosines).
            tau_scope: 0.95,
        }
    }
}

/// How many candidates a merge decision looks at. See `MergeConfig::top_k`.
pub const DEFAULT_TOP_K: usize = 3;

/// What the decision is.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeVerdict {
    /// Exactly the same content once normalised.
    Same,
    /// Merge: the lexical filter fired. `rule` names why (audit trail).
    Merge { candidate: i64, rule: &'static str },
    /// Lexical refused, but the two rows are as close as rewritten duplicates
    /// get: a named judge decides. NEVER an automatic merge.
    NeedsJudgement { candidate: i64, cosine: f32 },
    /// No candidate close enough: this is new content.
    New,
    /// Terminal refusal (polarity / empty). Never escalated.
    Refused(&'static str),
}

/// Where the candidate set CAME FROM. ONE string, and it travels in the audit
/// reason under the spec's own key name (`candidate_source=`, R-B C2 target ①).
///
/// WHY THIS EXISTS RATHER THAN A NEW FIELD. The decision already carries `rule=`,
/// which names the RULE that decided — a different fact. Writing the source as a
/// second key in the same reason keeps one vocabulary and one writer instead of
/// adding a field that a caller could forget to set. The value set is closed:
/// `embedding` (the embedder produced the cosines) or `none` (no vectors were
/// available, so the decision fell back to the lexical filter alone — see
/// `merge_candidates`, which hands back `cosine: None` on every row in that case).
pub fn candidate_source(cosines_available: bool) -> &'static str {
    if cosines_available {
        "embedding"
    } else {
        "none"
    }
}

/// A decision PLUS the rows it is answerable to (t31 / RV-B-low-②).
///
/// THE DEFECT THIS FIXES. `NeedsJudgement` carried `candidate` = the candidate
/// with the HIGHEST cosine among the lexically-refused ones. With a noisy
/// neighbour (a high-cosine row that is not the same fact) ranked first, the
/// audit named the noise and said nothing about the row the lexical rule had
/// actually judged — so a reader could not tell "the rule and the similarity
/// disagree" from "the rule agreed". The `anchor` (the row the verdict's rule is
/// grounded on) and `escalated_from` (the row that met `tau`) are now both
/// recorded, and `over_tau` lists EVERY refused row that met `tau`, so no row
/// that could have driven the decision can hide behind another.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeAudit {
    pub verdict: MergeVerdict,
    /// The row the verdict's rule is grounded on: Merge/Same → that row;
    /// NeedsJudgement → the row the escalation is based on; Refused → the row
    /// whose refusal is terminal; New → none (no row was close enough).
    pub anchor: Option<i64>,
    /// `Some(row)` when a HIGHER-cosine refused row was also over `tau` and the
    /// decision did not follow it (it followed the lexically closer one) — the
    /// disagreement between "what the embedder liked most" and "what the decision
    /// used". `None` when the decision followed the highest-cosine row, or when
    /// nothing escalated.
    pub escalated_from: Option<i64>,
    /// Every lexically-refused row that met `tau_scope`, ascending by id.
    pub over_tau: Vec<i64>,
    /// Rows whose refusal was a POLARITY refusal, ascending by id. Recorded, not
    /// hidden: such a row may not be merged OR escalated, but it must not decide
    /// the fate of the other candidates (see the note in `merge_decision_audited`).
    pub polarity_refused: Vec<i64>,
    /// Whether the candidate set had usable cosines (see `candidate_source`).
    pub cosines_available: bool,
}

/// The candidate set, bounded and deterministically ordered: cosine descending,
/// then id ascending so a tie cannot reorder the decision between two runs.
/// A row with no vector sorts LAST (it is unknown, not unrelated).
pub fn bounded_candidates(candidates: &[MergeCandidate], top_k: usize) -> Vec<&MergeCandidate> {
    let mut v: Vec<&MergeCandidate> = candidates.iter().collect();
    v.sort_by(|a, b| {
        let ac = a.cosine.unwrap_or(f32::NEG_INFINITY);
        let bc = b.cosine.unwrap_or(f32::NEG_INFINITY);
        bc.partial_cmp(&ac)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    v.truncate(top_k);
    v
}

/// THE DECISION. See the block comment above for why it is not just `judge`.
pub fn merge_decision(new: &str, candidates: &[MergeCandidate], cfg: &MergeConfig) -> MergeVerdict {
    merge_decision_audited(new, candidates, cfg).verdict
}

/// THE DECISION, with the rows it is answerable to. This is the implementation;
/// `merge_decision` is the thin wrapper, so there is exactly ONE decision rule.
///
/// WHICH ESCALATED ROW THE DECISION FOLLOWS (t31 / RV-B-low-②). When several
/// refused rows meet `tau`, the decision is anchored on the one whose TEXT is
/// closest (`text_overlap`), with cosine as the tie-break and id as the final
/// one — not on the highest cosine. Cosine alone cannot tell a rewrite from a
/// neighbour about another subject on this corpus (45.19% of live pairs sit at
/// ≥ 0.86), so "highest cosine wins" would let a noisy neighbour decide; the
/// higher-cosine row the decision did NOT follow is recorded as
/// `escalated_from`, which is the disagreement a reader needs to see.
///
/// ONE REFUSED NEIGHBOUR MUST NOT DECIDE FOR THE OTHERS (found by this change's
/// own fixture, t31). The loop used to `return` on the first polarity refusal, so
/// a candidate whose diff merely CONTAINED a polarity word — often in the new,
/// rewritten side, e.g. "禁止警告" while the old side says "must keep warnings
/// clean" — ended the whole decision, including a merge with a different row that
/// the lexical filter accepts. The gate is still fail-closed PER PAIR: a
/// polarity-refused row is never merged and never escalated, it is RECORDED
/// (`polarity_refused=`), and when nothing better is on offer the verdict IS
/// `Refused(POLARITY_REFUSAL)`.
///
/// HONEST SCOPE OF THAT FIX: it does NOT raise the labelled fixture's recall (that
/// is 14/20 either way, because those four pairs are refused for their own sake —
/// see the fixture test). What it changes is the cross-pair behaviour, pinned by
/// `a_polarity_refused_neighbour_does_not_block_a_merge_with_another_row`: before,
/// the polarity row made the verdict `Refused` and row 6 was never merged.
pub fn merge_decision_audited(
    new: &str,
    candidates: &[MergeCandidate],
    cfg: &MergeConfig,
) -> MergeAudit {
    let top = bounded_candidates(candidates, cfg.top_k);
    let cosines_available = top.iter().any(|c| c.cosine.is_some());
    let mut same: Option<i64> = None;
    let mut mergeable: Option<i64> = None;
    let mut anchor: Option<(usize, f32, i64)> = None;
    let mut highest_cosine: Option<(f32, i64)> = None;
    let mut over_tau: Vec<i64> = Vec::new();
    let mut polarity_refused: Vec<i64> = Vec::new();
    // The whole bounded set is examined (top_k is 3): deciding after the scan is
    // what lets the audit name every row that was in play.
    for cand in &top {
        match judge(&cand.content, new) {
            Verdict::Same => same = same.or(Some(cand.id)),
            Verdict::Mergeable => mergeable = mergeable.or(Some(cand.id)),
            Verdict::Refused(POLARITY_REFUSAL) => polarity_refused.push(cand.id),
            Verdict::Refused(_) => {
                let Some(c) = cand.cosine.filter(|c| *c >= cfg.tau_scope) else {
                    continue;
                };
                over_tau.push(cand.id);
                if highest_cosine.map(|(bc, _)| c > bc).unwrap_or(true) {
                    highest_cosine = Some((c, cand.id));
                }
                let overlap = text_overlap(&cand.content, new);
                let better = match anchor {
                    None => true,
                    Some((o, cc, id)) => {
                        overlap > o || (overlap == o && (c > cc || (c == cc && cand.id < id)))
                    }
                };
                if better {
                    anchor = Some((overlap, c, cand.id));
                }
            }
        }
    }
    over_tau.sort_unstable();
    polarity_refused.sort_unstable();
    let (verdict, anchor_id, escalated_from) = if let Some(id) = same {
        (MergeVerdict::Same, Some(id), None)
    } else if let Some(id) = mergeable {
        (
            MergeVerdict::Merge {
                candidate: id,
                rule: "lexical_ignorable",
            },
            Some(id),
            None,
        )
    } else if let Some((_, c, id)) = anchor {
        (
            MergeVerdict::NeedsJudgement {
                candidate: id,
                cosine: c,
            },
            Some(id),
            // The row the decision did NOT follow, when a higher-cosine
            // neighbour was also over tau: the noisy-neighbour signature.
            highest_cosine.map(|(_, nid)| nid).filter(|nid| *nid != id),
        )
    } else if let Some(id) = polarity_refused.first().copied() {
        // Nothing better on offer: the fail-closed refusal IS the verdict.
        (MergeVerdict::Refused(POLARITY_REFUSAL), Some(id), None)
    } else {
        (MergeVerdict::New, None, None)
    };
    MergeAudit {
        verdict,
        anchor: anchor_id,
        escalated_from,
        over_tau,
        polarity_refused,
        cosines_available,
    }
}

/// The audit string for one decision. ONE writer, so the op's reason cannot
/// drift between the callers that record it.
///
/// The `anchor=` / `escalated_from=` / `over_tau=` / `polarity_refused=` keys
/// exist because the reason is the only thing a reader of `memory_diffs` gets
/// (t31 / RV-B-low-②): `anchor` names the row the verdict's rule is grounded on,
/// `escalated_from` names the row that met `tau` when it is NOT the row the
/// decision followed, `over_tau` lists every such row, and `polarity_refused`
/// names the rows the fail-closed gate excluded. `candidate_source=` names where
/// the candidate set came from (the spec's own key name, see `candidate_source`).
pub fn merge_audit_reason(decision: &MergeAudit, candidates: usize) -> String {
    let (rule, verdict_word) = match &decision.verdict {
        MergeVerdict::Same => ("lexical_same", "same"),
        MergeVerdict::Merge { rule, .. } => (*rule, "merge"),
        MergeVerdict::NeedsJudgement { .. } => ("embedding_scope", "needs_judgement"),
        MergeVerdict::New => ("no_candidate", "new"),
        MergeVerdict::Refused(r) => (*r, "refused"),
    };
    let ids = |v: &[i64]| {
        if v.is_empty() {
            "none".to_string()
        } else {
            v.iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        }
    };
    format!(
        "candidates={candidates} candidate_source={} anchor={} escalated_from={} over_tau={} polarity_refused={} rule={rule} verdict={verdict_word}",
        candidate_source(decision.cosines_available),
        decision
            .anchor
            .map(|i| i.to_string())
            .unwrap_or_else(|| "none".to_string()),
        decision
            .escalated_from
            .map(|i| i.to_string())
            .unwrap_or_else(|| "none".to_string()),
        ids(&decision.over_tau),
        ids(&decision.polarity_refused),
    )
}

/// The cosine level at `percentile` (0.0–1.0) of a scope's own pairwise cosine
/// distribution — the value `MergeConfig::tau_scope` must be set from.
///
/// Returns `None` for an empty distribution (nothing to derive a level from;
/// the caller must then keep the naive default AND say so, not invent a number).
pub fn scope_tau(cosines: &[f32], percentile: f32) -> Option<f32> {
    if cosines.is_empty() {
        return None;
    }
    let mut v: Vec<f32> = cosines.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p = percentile.clamp(0.0, 1.0);
    let idx = ((v.len() - 1) as f32 * p).round() as usize;
    v.get(idx).copied()
}

/// Letters and digits only, lower-cased: punctuation, whitespace and the
/// "`[distilled]`" bracket style carry no meaning for this comparison.
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// The characters of `a` that are NOT part of its longest common subsequence
/// with `b`: an ORDER-PRESERVING difference.
///
/// A multiset walk is not enough, and the failure is worth recording because it
/// is the same trap the bigram version fell into. A walk hands back the leftover
/// characters in `a`'s order, so it picks the wrong occurrence of a repeated
/// letter: "the user …" minus "user …" came back as `ht` (the `t` was consumed from
/// "Rust"/"timezone" instead of from "the"), and "用户偏好…" minus "偏好…" as
/// `户用`. Both are fragments of words that ARE ignorable, so a fail-closed rule
/// refused merges it should have allowed. LCS keeps "the" whole and "用户" whole.
/// The LCS table for two char slices: `dp[i][j]` = the longest common
/// subsequence length of `x[i..]` and `y[j..]`.
///
/// ONE table, shared by `diff` (which needs the path) and `text_overlap` (which
/// needs only the length): the lexical rule and the escalation preference must
/// not measure "closeness" two different ways.
fn lcs_table(x: &[char], y: &[char]) -> Vec<Vec<usize>> {
    let mut dp = vec![vec![0usize; y.len() + 1]; x.len() + 1];
    for i in (0..x.len()).rev() {
        for j in (0..y.len()).rev() {
            dp[i][j] = if x[i] == y[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    dp
}

/// How much NORMALISED text two strings share: the LCS length. Public because it
/// is the measurement behind the escalation preference, so a probe can reproduce
/// a decision instead of trusting it (t31 / RV-B-low-②).
pub fn text_overlap(a: &str, b: &str) -> usize {
    let x: Vec<char> = normalize(a).chars().collect();
    let y: Vec<char> = normalize(b).chars().collect();
    lcs_table(&x, &y)[0][0]
}

fn diff(a: &str, b: &str) -> String {
    let x: Vec<char> = a.chars().collect();
    let y: Vec<char> = b.chars().collect();
    let dp = lcs_table(&x, &y);
    let (mut i, mut j) = (0usize, 0usize);
    let mut out = String::new();
    while i < x.len() && j < y.len() {
        if x[i] == y[j] {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push(x[i]);
            i += 1;
        } else {
            j += 1;
        }
    }
    while i < x.len() {
        out.push(x[i]);
        i += 1;
    }
    out
}

/// What remains of a difference once the ignorable vocabulary is accounted for.
/// Anything left is content by elimination — the fail-closed half of the rule.
fn leftover(diff: &str) -> String {
    strip_ignorable(diff)
}

/// Remove every ignorable word; what is left is content by elimination.
fn strip_ignorable(s: &str) -> String {
    let mut out = s.to_string();
    for w in IGNORABLE {
        while out.contains(w) {
            out = out.replacen(w, "", 1);
        }
    }
    out
}

/// CJK tokens match anywhere (there are no word boundaries inside a Han run);
/// Latin tokens match as whole words, so "no" does not fire inside "note".
fn contains_token(haystack: &str, token: &str) -> bool {
    if token.chars().any(|c| c.is_ascii_alphabetic()) {
        haystack
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w == token)
    } else {
        haystack.contains(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four phrasings t323/t325 measured (memory rows 117 / 123 / 126 /
    /// 142). Their differences are exactly {偏好, 进行, 用户}, so every pair
    /// must be mergeable — this is the case the old word-based check could not
    /// see, and it is what makes 8/44 redundant profile rows go away.
    #[test]
    fn the_four_phrasings_are_mergeable() {
        let four = [
            "[distilled] 用户偏好使用简体中文交流。",
            "[distilled] 用户使用简体中文交流。",
            "[distilled] 用户使用简体中文进行交流。",
            "[distilled] 偏好使用简体中文交流。",
        ];
        for (i, a) in four.iter().enumerate() {
            for (j, b) in four.iter().enumerate() {
                if i == j {
                    assert_eq!(judge(a, b), Verdict::Same, "{a} vs {b}");
                } else {
                    assert_eq!(judge(a, b), Verdict::Mergeable, "{a} vs {b}");
                }
            }
        }
        // And the old rule, on the same pair, for the record: one token, so the
        // hit rate can only be 0 or 1 — it never reaches 0.7 for a rewrite.
        let old_rule = |s: &str| -> Vec<String> {
            s.split_whitespace()
                .map(|w| {
                    w.chars()
                        .filter(|c| c.is_alphanumeric())
                        .collect::<String>()
                        .to_lowercase()
                })
                .filter(|w| w.len() > 2)
                .collect()
        };
        let a = old_rule(four[0]);
        let b = old_rule(four[1]);
        let hits = a.iter().filter(|t| b.contains(t)).count();
        let rate = hits as f64 / a.len() as f64;
        println!(
            "READING old rule: tokens={} vs {} | hits={hits} | rate={rate}",
            a.len(),
            b.len()
        );
        // Two tokens: the "[distilled]" prefix, and the WHOLE Chinese sentence
        // (no whitespace inside it). Only the shared prefix can match, so the
        // rate is 0.5 -- below the rule's own 0.7 -- and a pure rewrite is never
        // seen as a duplicate. That is the dead code t325 diagnosed, reproduced
        // on the very rows t323 measured.
        assert_eq!(a.len(), 2, "prefix + whole sentence: two tokens");
        assert!(
            rate < 0.7,
            "the old rule could not reach its threshold: {rate}"
        );
    }

    /// The reverse: one character flips the meaning, so the merge must be
    /// refused even though the strings are nearly identical.
    #[test]
    fn a_polarity_token_refuses_the_merge() {
        let v = judge("用户偏好简体中文", "用户不使用简体中文");
        println!("READING polarity pair => {v:?}");
        assert_eq!(v, Verdict::Refused("a polarity token is in the difference"));
        // ... and the same holds the other way round.
        assert!(matches!(
            judge("用户不使用简体中文", "用户偏好简体中文"),
            Verdict::Refused(_)
        ));
    }

    /// Fail-closed: a word we do not recognise as filler is content, so a
    /// rewrite that swaps real content is refused.
    #[test]
    fn an_unrecognised_word_refuses_the_merge() {
        let v = judge(
            "[distilled] 用户偏好简体中文交流。",
            "[distilled] 用户偏好英文交流。",
        );
        println!("READING content swap => {v:?}");
        assert_eq!(v, Verdict::Refused("a content word is in the difference"));
        assert_eq!(
            judge(
                "[distilled] 部署流水线在周二运行。",
                "[distilled] 部署流水线在周三运行。"
            ),
            Verdict::Refused("a content word is in the difference")
        );
    }

    /// Bigrams would report 文交 / 文进 / 行交 here and refuse everything; the
    /// character difference is empty, so the judgement is Same.
    #[test]
    fn character_differs_from_bigram_on_shifted_text() {
        let a = normalize("用户使用简体中文进行交流");
        let b = normalize("用户使用简体中文交流");
        assert!(diff(&a, &b) == "进行" || diff(&a, &b) == "进" || !diff(&a, &b).is_empty());
        let bigrams = |s: &str| -> Vec<String> {
            let cs: Vec<char> = s.chars().collect();
            cs.windows(2).map(|w| w.iter().collect()).collect()
        };
        let shifted: Vec<String> = bigrams(&b)
            .into_iter()
            .filter(|g| ["文交", "文进", "行交"].contains(&g.as_str()))
            .collect();
        println!("READING shifted pseudo-bigrams present in the shorter phrasing: {shifted:?}");
        assert_eq!(
            judge("用户使用简体中文进行交流", "用户使用简体中文交流"),
            Verdict::Mergeable
        );
    }

    // ── C2: the bounded decision ───────────────────────────────────────────

    /// The live paraphrase pair from the corpus (R-B A.9): cosine 0.9884, and
    /// `judge` refuses it because the difference carries content words
    /// (以/键/作为/触发). Before this decision existed, that pair was silently
    /// LOST — 0 of 2297 live pairs ever merged. The decision must escalate it
    /// to a named judge instead of dropping it.
    #[test]
    fn a_high_cosine_paraphrase_escalates_instead_of_being_lost() {
        let new = "用户的语音输入法以 F6 键作为启动/停止语音输入的触发键。";
        let old = "用户的语音输入法用 F6 作为启动/停止语音输入的快捷键。";
        // the lexical filter alone loses it:
        let lexical = judge(old, new);
        println!("READING C2 lexical verdict on the 0.9884 pair: {lexical:?}");
        assert_eq!(lexical, Verdict::Refused(CONTENT_REFUSAL));

        let cands = vec![MergeCandidate {
            id: 5,
            content: old.into(),
            cosine: Some(0.9884),
        }];
        let d = merge_decision_audited(new, &cands, &MergeConfig::default());
        println!(
            "READING C2 decision on the same pair: {:?} | audit={}",
            d.verdict,
            merge_audit_reason(&d, cands.len())
        );
        assert_eq!(
            d.verdict,
            MergeVerdict::NeedsJudgement {
                candidate: 5,
                cosine: 0.9884
            },
            "a 0.9884 paraphrase must reach a named judge, not be dropped"
        );
        assert_eq!(d.anchor, Some(5));
        assert_eq!(d.escalated_from, None, "nothing higher-cosine was skipped");
        assert_eq!(d.over_tau, vec![5]);
    }

    /// A polarity flip is TERMINAL **for its own pair** even when the embedder
    /// says the two rows are near-identical (they are: same words, one 不): it is
    /// never merged and never escalated. Since t31 it is also RECORDED, so a
    /// reader can see that the gate — not the similarity — cost the pair.
    #[test]
    fn a_polarity_flip_is_never_escalated() {
        let cands = vec![MergeCandidate {
            id: 7,
            content: "用户偏好简体中文".into(),
            cosine: Some(0.99),
        }];
        let d = merge_decision_audited("用户不使用简体中文", &cands, &MergeConfig::default());
        println!(
            "READING C2 polarity at cosine 0.99: {:?} | {}",
            d.verdict,
            merge_audit_reason(&d, cands.len())
        );
        assert_eq!(d.verdict, MergeVerdict::Refused(POLARITY_REFUSAL));
        assert_eq!(d.anchor, Some(7));
        assert_eq!(
            d.over_tau,
            Vec::<i64>::new(),
            "a polarity row is not escalated"
        );
        assert_eq!(d.polarity_refused, vec![7]);
    }

    /// **A polarity-refused neighbour must not decide for the others** (found by
    /// this change's own fixture, t31): a row whose diff merely CONTAINS a
    /// polarity word is excluded from merging and escalating, but the decision
    /// continues — otherwise one such neighbour would silently cost a merge with a
    /// different row that the lexical filter accepts.
    #[test]
    fn a_polarity_refused_neighbour_does_not_block_a_merge_with_another_row() {
        let new = "用户偏好简体中文交流";
        let cands = vec![
            MergeCandidate {
                id: 4,
                content: "用户不使用简体中文".into(), // polarity refusal
                cosine: Some(0.95),
            },
            MergeCandidate {
                id: 6,
                content: "用户偏好简体中文进行交流".into(), // only ignorable words differ
                cosine: Some(0.80),
            },
        ];
        let d = merge_decision_audited(new, &cands, &MergeConfig::default());
        println!(
            "READING C2 polarity neighbour + mergeable neighbour: {:?} | {}",
            d.verdict,
            merge_audit_reason(&d, cands.len())
        );
        assert_eq!(
            d.verdict,
            MergeVerdict::Merge {
                candidate: 6,
                rule: "lexical_ignorable"
            },
            "the mergeable row must still be merged"
        );
        assert_eq!(d.polarity_refused, vec![4]);
        assert_eq!(d.anchor, Some(6));
    }

    /// Below the scope's own level a lexical refusal is NOT escalated: the row is
    /// new. The level comes from the distribution, so the test derives it.
    #[test]
    fn below_the_scope_level_the_content_is_new() {
        let cands = vec![MergeCandidate {
            id: 9,
            content: "部署流水线在周二运行".into(),
            cosine: Some(0.85),
        }];
        let cfg = MergeConfig {
            top_k: DEFAULT_TOP_K,
            tau_scope: 0.95,
        };
        let v = merge_decision("部署流水线在周三运行", &cands, &cfg);
        println!("READING C2 content swap below tau: {v:?}");
        assert_eq!(v, MergeVerdict::New);
    }

    /// The candidate set is bounded and deterministic, and a missing vector is
    /// UNKNOWN (sorts last), never "unrelated at 0.0".
    #[test]
    fn candidates_are_bounded_ordered_and_missing_vectors_sort_last() {
        let cands = vec![
            MergeCandidate {
                id: 3,
                content: "x".into(),
                cosine: None,
            },
            MergeCandidate {
                id: 1,
                content: "x".into(),
                cosine: Some(0.5),
            },
            MergeCandidate {
                id: 2,
                content: "x".into(),
                cosine: Some(0.9),
            },
            MergeCandidate {
                id: 4,
                content: "x".into(),
                cosine: Some(0.9),
            },
        ];
        let top = bounded_candidates(&cands, 3);
        let ids: Vec<i64> = top.iter().map(|c| c.id).collect();
        println!("READING C2 bounded candidates top3 ids={ids:?}");
        assert_eq!(ids, vec![2, 4, 1], "cosine desc, id asc on ties, None last");
    }

    /// `tau_scope` must be derivable from the scope's own distribution — the
    /// live numbers are printed so a reader can see why a constant 0.90 would be
    /// wrong (15.93% of live pairs sit above it).
    #[test]
    fn scope_tau_comes_from_the_distribution() {
        // the live corpus' measured distribution landmarks (R-B A.9)
        let live_like = vec![0.7434f32, 0.8500, 0.9177, 0.9525, 0.9884];
        let t = scope_tau(&live_like, TAU_PERCENTILE).unwrap();
        println!(
            "READING C2 scope_tau(p{}) on the measured landmarks = {t} (median 0.85)",
            TAU_PERCENTILE
        );
        assert!(
            t >= 0.95,
            "p99 of a distribution with 0.9525 must not be 0.90"
        );
        assert_eq!(
            scope_tau(&[], TAU_PERCENTILE),
            None,
            "empty -> None, not 0.0"
        );
    }

    /// The audit reason has ONE writer, so the op's text cannot drift — and since
    /// t31 it carries the source, the anchor, the skipped neighbour and every row
    /// over tau (RV-B-low-① ②).
    #[test]
    fn the_audit_reason_names_the_source_the_anchor_and_the_rows_over_tau() {
        let d = MergeAudit {
            verdict: MergeVerdict::Merge {
                candidate: 11,
                rule: "lexical_ignorable",
            },
            anchor: Some(11),
            escalated_from: None,
            over_tau: Vec::new(),
            polarity_refused: Vec::new(),
            cosines_available: true,
        };
        let r = merge_audit_reason(&d, 3);
        println!("READING C2 audit reason (merge): {r}");
        assert_eq!(
            r,
            "candidates=3 candidate_source=embedding anchor=11 escalated_from=none over_tau=none polarity_refused=none rule=lexical_ignorable verdict=merge"
        );

        let no_vectors = MergeAudit {
            cosines_available: false,
            ..d.clone()
        };
        let r2 = merge_audit_reason(&no_vectors, 3);
        println!("READING C2 audit reason (no vectors): {r2}");
        assert!(r2.contains("candidate_source=none"));
        assert_eq!(candidate_source(true), "embedding");
        assert_eq!(candidate_source(false), "none");

        let new = MergeAudit {
            verdict: MergeVerdict::New,
            anchor: None,
            escalated_from: None,
            over_tau: Vec::new(),
            polarity_refused: Vec::new(),
            cosines_available: true,
        };
        let r3 = merge_audit_reason(&new, 0);
        println!("READING C2 audit reason (new): {r3}");
        assert!(r3.contains("anchor=none"));
    }

    /// THE NOISY NEIGHBOUR (t31 / RV-B-low-②). A high-cosine row about a
    /// DIFFERENT subject is ranked first; the row that is actually the same fact
    /// has a slightly lower cosine. The decision must follow the text it is
    /// actually about, and the audit must name the higher-cosine row it did NOT
    /// follow — otherwise a reader cannot distinguish "the rule and the embedder
    /// agree" from "the embedder pulled the decision onto a neighbour".
    #[test]
    fn a_noisy_neighbour_ranks_first_and_the_audit_says_so() {
        // Both the noise and the rewrite must be CONTENT refusals (not polarity
        // ones), otherwise the case would be about the gate rather than about the
        // noisy neighbour — that lesson is in the polarity test above.
        let new = "用户的时区是 Asia/Shanghai。";
        // 9 is the SAME fact, written differently (cosine 0.97).
        // 7 is about another subject but the embedder likes it more (0.99).
        let cands = vec![
            MergeCandidate {
                id: 7,
                content: "用户的键盘布局是 Dvorak。".into(),
                cosine: Some(0.99),
            },
            MergeCandidate {
                id: 9,
                content: "用户所在时区为 Asia/Shanghai。".into(),
                cosine: Some(0.97),
            },
        ];
        let d = merge_decision_audited(new, &cands, &MergeConfig::default());
        let reason = merge_audit_reason(&d, cands.len());
        println!(
            "READING C2 noise-first: overlap(noise)={} overlap(paraphrase)={}",
            text_overlap(&cands[0].content, new),
            text_overlap(&cands[1].content, new)
        );
        println!(
            "READING C2 noise-first decision: {:?} | {reason}",
            d.verdict
        );
        assert_eq!(
            d.verdict,
            MergeVerdict::NeedsJudgement {
                candidate: 9,
                cosine: 0.97
            },
            "the decision must follow the row whose text is the same fact"
        );
        assert_eq!(d.anchor, Some(9));
        assert_eq!(
            d.escalated_from,
            Some(7),
            "the higher-cosine neighbour the decision did not follow must be named"
        );
        assert_eq!(d.over_tau, vec![7, 9], "no row over tau may hide");
        assert!(reason.contains("anchor=9"), "{reason}");
        assert!(reason.contains("escalated_from=7"), "{reason}");
        assert!(reason.contains("over_tau=7,9"), "{reason}");
    }

    // ── C2 target ③: a labelled fixture that needs no model to re-run ───────
    //
    // WHAT THIS FIXTURE IS, AND WHAT IT IS NOT (t31 / RV-B-3). The spec asks for
    // ≥20 positives (human-confirmed rewrites of the same fact) and ≥20 negatives
    // (polarity / number / subject differ), with 误并 and 召回 read off them. The
    // cosine column here is **part of the fixture** — assigned by hand from the
    // live corpus' landmarks (positives 0.93–0.99, negatives 0.55–0.94) — because
    // a unit test has no embedder. So this measures the DECISION RULE given
    // similarity evidence; it does NOT measure the embedder, and it is not a
    // substitute for the live gold set (that is I-A's instrument, R-A A3).
    // The lexical column needs no assumption at all and is reported separately.
    //
    // The candidate set is the decisive pair alone (top_k=1 in effect): the
    // candidate-set question ("does top-K find the pair at all") was measured
    // separately in R-B A.9 (K=3 recovers 24/24 of the cos≥0.95 pairs).

    /// (old, new, assigned cosine) — 20 human-confirmed same-fact rewrites.
    const POSITIVE_PAIRS: &[(&str, &str, f32)] = &[
        (
            "用户的语音输入法用 F6 作为启动/停止语音输入的快捷键。",
            "用户的语音输入法以 F6 键作为启动/停止语音输入的触发键。",
            0.9884,
        ),
        (
            "讨厌轮询式交互，偏好状态变化时自动通知／事件推送。",
            "用户讨厌轮询，偏好由系统主动推送的自动通知。",
            0.9861,
        ),
        (
            "xlwt 的版本属性是 __VERSION__ 而不是 __version__。",
            "xlwt 的版本属性名是 __VERSION__ 而非 __version__。",
            0.9846,
        ),
        (
            "部署脚本位于 scripts/release.sh。",
            "发布脚本在 scripts/release.sh 里。",
            0.99,
        ),
        ("用户使用双屏显示器。", "用户的工作环境是双显示器。", 0.97),
        (
            "项目使用 Rust edition 2024。",
            "这个项目基于 Rust 2024 edition。",
            0.96,
        ),
        (
            "测试跑在临时 root 下，不动 ~/.ruagent。",
            "单测用临时 root，禁止写 ~/.ruagent。",
            0.98,
        ),
        (
            "单写者：同一文件同一时刻只允许一个写者。",
            "每个文件同一时间只能有一个写者。",
            0.97,
        ),
        ("面板用 Vite 构建。", "前端面板由 Vite 打包。", 0.96),
        (
            "数据库迁移只允许追加，不改历史文件。",
            "迁移文件是追加式的，历史迁移不能被修改。",
            0.98,
        ),
        (
            "用户的时区是 Asia/Shanghai。",
            "用户所在时区为 Asia/Shanghai。",
            0.99,
        ),
        (
            "召回日志每次召回写一行。",
            "每次召回都会往 recall_log 写一条记录。",
            0.97,
        ),
        (
            "全部的路径要用 POSIX 相对路径写进报告。",
            "报告里的路径一律写成 POSIX 相对路径。",
            0.96,
        ),
        ("用户偏好简洁的回答。", "用户喜欢简短的回答。", 0.98),
        ("向量维度是 384。", "嵌入向量有 384 维。", 0.97),
        (
            "FTS5 的 bm25 越负越好。",
            "bm25 分数越小（越负）表示越相关。",
            0.96,
        ),
        ("网关端口是 8787。", "服务的监听端口为 8787。", 0.99),
        (
            "提交前必须跑 clippy -D warnings。",
            "提交之前要执行 clippy 并禁止警告。",
            0.96,
        ),
        // Two positives whose assigned cosine is BELOW the fallback level 0.95:
        // the threshold is a real trade-off, not a formality.
        (
            "这个仓库用 SQLite 单写者 actor。",
            "本仓库的 SQLite 写入走单写者 actor。",
            0.94,
        ),
        (
            "任务单的 inScope 是唯一的可写面。",
            "只有任务单 inScope 里的路径可以写。",
            0.93,
        ),
    ];

    /// (old, new, assigned cosine) — 20 pairs that are NOT the same fact.
    const NEGATIVE_PAIRS: &[(&str, &str, f32)] = &[
        ("用户偏好简体中文。", "用户不使用简体中文。", 0.94),
        ("部署流水线在周二运行。", "部署流水线在周三运行。", 0.90),
        ("网关端口是 8787。", "网关端口是 8788。", 0.92),
        ("用户使用双屏显示器。", "用户使用单屏显示器。", 0.93),
        ("用户讨厌轮询。", "用户喜欢轮询。", 0.88),
        (
            "项目使用 Rust edition 2024。",
            "项目使用 Rust edition 2021。",
            0.86,
        ),
        ("测试跑在临时 root 下。", "测试跑在生产 root 下。", 0.90),
        ("用户的时区是 Asia/Shanghai。", "用户的时区是 UTC。", 0.80),
        ("向量维度是 384。", "向量维度是 768。", 0.90),
        ("提交前必须跑 clippy。", "提交前不必跑 clippy。", 0.92),
        ("面板用 Vite 构建。", "面板用 Webpack 构建。", 0.75),
        ("迁移只允许追加。", "迁移允许改写历史文件。", 0.70),
        ("召回日志每次召回写一行。", "召回日志从不写行。", 0.78),
        ("用户偏好简洁的回答。", "用户偏好冗长的回答。", 0.85),
        ("FTS5 的 bm25 越负越好。", "FTS5 的 bm25 越大越好。", 0.84),
        ("数据库是 SQLite。", "数据库是 PostgreSQL。", 0.60),
        ("用户使用 xlwt 库。", "用户使用 openpyxl 库。", 0.65),
        ("服务监听 127.0.0.1。", "服务监听 0.0.0.0。", 0.55),
        ("任务单 inScope 是唯一可写面。", "任何文件都可以改。", 0.68),
        (
            "单写者：同一文件一个写者。",
            "多个写者可以同时改同一文件。",
            0.72,
        ),
    ];

    /// The fixture's verdict on one pair, and whether that means "the pair was
    /// recognised" (Merge or escalation), "silently merged" (Merge), or "lost".
    fn verdict_of(pair: &(&str, &str, f32), tau: f32) -> (MergeVerdict, MergeAudit) {
        let (old, new, cos) = *pair;
        let cands = vec![MergeCandidate {
            id: 1,
            content: old.to_string(),
            cosine: Some(cos),
        }];
        let cfg = MergeConfig {
            top_k: DEFAULT_TOP_K,
            tau_scope: tau,
        };
        let d = merge_decision_audited(new, &cands, &cfg);
        (d.verdict.clone(), d)
    }

    /// C2 target ③, THE RULING'S READING (RV-B-3): ≥20 positives + ≥20 negatives,
    /// 误并 = 0, and the recall at the documented fallback level.
    #[test]
    fn the_labelled_fixture_has_zero_false_merges_and_a_measured_recall() {
        let tau = MergeConfig::default().tau_scope;
        println!(
            "READING C2 fixture object set: positives={} negatives={} tau={tau} (MergeConfig::default, documented as the live p99 level)",
            POSITIVE_PAIRS.len(),
            NEGATIVE_PAIRS.len()
        );

        // ---- the LEXICAL rule alone: no cosine assumption at all ----------
        let mut lexical_recall = 0usize;
        for (old, new, _) in POSITIVE_PAIRS {
            if matches!(judge(old, new), Verdict::Mergeable | Verdict::Same) {
                lexical_recall += 1;
            }
        }
        let mut lexical_false_merge = 0usize;
        for (old, new, _) in NEGATIVE_PAIRS {
            if matches!(judge(old, new), Verdict::Mergeable | Verdict::Same) {
                lexical_false_merge += 1;
            }
        }
        println!(
            "READING C2 lexical-only: recall={lexical_recall}/{} false_merge={lexical_false_merge}/{}",
            POSITIVE_PAIRS.len(),
            NEGATIVE_PAIRS.len()
        );

        // ---- the DECISION at the fallback level ---------------------------
        let mut recall = 0usize;
        let mut false_merge = 0usize;
        let mut negative_escalations = 0usize;
        let mut lost_below_tau: Vec<&str> = Vec::new();
        let mut lost_polarity: Vec<&str> = Vec::new();
        for p in POSITIVE_PAIRS {
            let (v, d) = verdict_of(p, tau);
            match v {
                MergeVerdict::Merge { .. }
                | MergeVerdict::NeedsJudgement { .. }
                | MergeVerdict::Same => {
                    recall += 1;
                }
                MergeVerdict::Refused(_) | MergeVerdict::New => {
                    if d.polarity_refused.is_empty() {
                        lost_below_tau.push(p.1);
                    } else {
                        lost_polarity.push(p.1);
                    }
                }
            }
        }
        for n in NEGATIVE_PAIRS {
            let (v, _) = verdict_of(n, tau);
            match v {
                MergeVerdict::Merge { .. } => false_merge += 1,
                MergeVerdict::NeedsJudgement { .. } => negative_escalations += 1,
                _ => {}
            }
        }
        println!(
            "READING C2 decision@{tau}: recall={recall}/{} (={:.2}) false_merge={false_merge}/{} negative_escalations={negative_escalations}/{} below_tau={} ({lost_below_tau:?}) polarity_gated={} ({lost_polarity:?})",
            POSITIVE_PAIRS.len(),
            recall as f64 / POSITIVE_PAIRS.len() as f64,
            NEGATIVE_PAIRS.len(),
            NEGATIVE_PAIRS.len(),
            lost_below_tau.len(),
            lost_polarity.len(),
        );
        assert_eq!(
            false_merge, 0,
            "a negative pair must never be merged: 误并 must be 0"
        );
        assert_eq!(
            negative_escalations, 0,
            "no negative pair may even reach a named judge at this level"
        );
        assert_eq!(
            lexical_false_merge, 0,
            "the lexical filter must stay fail-closed"
        );
        // The composition is PINNED, not described: if the gate or tau moves, this
        // test says which class of pair changed (a wrong direction would be a
        // silent recall loss).
        assert_eq!(
            lost_below_tau.len(),
            2,
            "exactly the two positives below tau are lost to the level: {lost_below_tau:?}"
        );
        assert_eq!(
            lost_polarity.len(),
            4,
            "exactly four positives are polarity-gated (fail-closed, never escalated): {lost_polarity:?}"
        );
        assert_eq!(recall, 14, "recall@tau=0.95 on this fixture: 14/20 = 0.70");

        // ---- the level derived from THIS fixture (and why it is not the rule)
        let mut cosines: Vec<f32> = POSITIVE_PAIRS
            .iter()
            .chain(NEGATIVE_PAIRS.iter())
            .map(|(_, _, c)| *c)
            .collect();
        cosines.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let fixture_tau = scope_tau(&cosines, TAU_PERCENTILE).unwrap();
        let mut recall_at_fixture_tau = 0usize;
        for p in POSITIVE_PAIRS {
            if matches!(
                verdict_of(p, fixture_tau).0,
                MergeVerdict::Merge { .. }
                    | MergeVerdict::NeedsJudgement { .. }
                    | MergeVerdict::Same
            ) {
                recall_at_fixture_tau += 1;
            }
        }
        println!(
            "READING C2 decision@{fixture_tau} (scope_tau of THIS fixture's 40 assigned cosines, p{TAU_PERCENTILE}): recall={recall_at_fixture_tau}/{}",
            POSITIVE_PAIRS.len()
        );
        assert!(
            recall_at_fixture_tau < recall,
            "a level derived from the very pairs you want to merge is circular and must be worse"
        );
    }
}
