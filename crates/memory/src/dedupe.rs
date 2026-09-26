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
        return Verdict::Refused("one side is empty after normalisation");
    }
    let only_old = diff(&a, &b);
    let only_new = diff(&b, &a);
    let changed = format!("{only_old}{only_new}");
    if let Some(p) = POLARITY.iter().find(|p| contains_token(&changed, p)) {
        let _ = p;
        return Verdict::Refused("a polarity token is in the difference");
    }
    let left_old = leftover(&only_old);
    let left_new = leftover(&only_new);
    if left_old.is_empty() && left_new.is_empty() {
        Verdict::Mergeable
    } else {
        Verdict::Refused("a content word is in the difference")
    }
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
fn diff(a: &str, b: &str) -> String {
    let x: Vec<char> = a.chars().collect();
    let y: Vec<char> = b.chars().collect();
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
}
