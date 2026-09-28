//! How this workspace talks to SQLite FTS5.
//!
//! Three call sites used to build their own match string, and they had already
//! drifted (t247): one quoted each whitespace token into a phrase and ANDed
//! them, another did the same for the whole query as one phrase. Two measured
//! consequences:
//!
//! * A quoted phrase requires its tokens ADJACENT IN ONE COLUMN, so the query
//!   "autohotkey-v2" cannot reach an entity named "AutoHotkey" whose summary
//!   says "v2.0.28": the query and the stored text are tokenised the same way,
//!   but the phrase demands an adjacency the stored text does not have.
//!   Prefixing the phrase does NOT fix this -- the prefix form of a phrase
//!   still requires adjacency. Splitting the query the way the tokenizer
//!   splits the text does fix it.
//! * unicode61 makes a whole run of Han characters ONE term, so no FTS query
//!   can find a substring of it (潜艇 inside 蓝鲸潜艇); only LIKE can.
//!
//! So this module offers: a splitter that matches the tokenizer, a precision
//! form (all terms, implicit AND), a recall form (each term as a prefix, ORed),
//! LIKE patterns for the one case FTS5 cannot express, and (t6) a Han-bigram
//! form for the shadow index that makes 2-character CJK queries indexable
//! instead of a full-table LIKE scan.

/// Split a query the way unicode61 splits stored text: every non-alphanumeric
/// character is a separator. Han characters are alphanumeric, so a run of them
/// stays one term -- which is exactly why substring search needs LIKE.
pub fn terms(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
    {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

fn quote(t: &str) -> String {
    format!("\"{}\"", t.replace('"', "\"\""))
}

/// Precision form: every term as a literal phrase, implicitly ANDed.
pub fn match_all(terms: &[String]) -> String {
    terms.iter().map(|t| quote(t)).collect::<Vec<_>>().join(" ")
}

/// Shortest ASCII term that may enter a RECALL form (prefix or LIKE).
///
/// WHY a floor at all: the recall forms exist to bridge a SPELLING difference
/// (postgres vs postgresql, autohotkey vs autohotkey-v2). A one- or two-letter
/// ASCII fragment does not bridge a spelling difference -- it matches whatever
/// word happens to start with it, so a recall stage that ORs such fragments
/// turns one stray short word in a query into a hit on text that has nothing to
/// do with the query.
///
/// WHY three: two ASCII characters give only a few hundred possible fragments,
/// so a match at that length is still mostly coincidence; three is the first
/// length at which the fragment carries enough of the word to be evidence
/// rather than luck. The LIKE form used to refuse only one character; a prefix
/// is strictly weaker evidence than a substring, so both forms share this one
/// floor instead of drifting apart.
///
/// WHY Han is exempt: unicode61 makes a whole run of Han characters ONE term, so
/// a two-character Han term is not a fragment of a word, it IS the word
/// (水温, 改键, 潜艇). A floor there would delete real terms, and for a
/// substring of a Han run the LIKE form is the only path that exists at all.
///
/// PUBLIC since t6 because the knife cuts both ways for a caller: `knowledge`'s
/// quality harness and the graph crate both need to say WHY a keyword leg came
/// back empty, and the honest answer is sometimes "every term in this query is
/// below the floor, by design".
pub const MIN_RECALL_ASCII: usize = 3;

/// Whether a term is long enough to be evidence in a recall form.
fn recallable(t: &str) -> bool {
    !(t.chars().count() < MIN_RECALL_ASCII && t.is_ascii())
}

/// Recall form: every term that is evidence on its own, as a prefix, ORed.
///
/// Short ASCII terms are dropped (see MIN_RECALL_ASCII); Han terms of any
/// length are kept. The precision form is untouched -- there a short term is
/// exact evidence ("c" for C++, "11" in Windows 11).
pub fn match_any_prefix(terms: &[String]) -> String {
    terms
        .iter()
        .filter(|t| recallable(t))
        .map(|t| format!("{}*", quote(t)))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// LIKE patterns for the one case FTS5 cannot express: a substring of a term.
///
/// Same floor as the prefix form (see MIN_RECALL_ASCII): a pattern built from a
/// one- or two-letter ASCII fragment matches almost every row, and token
/// equality in the precision form already covers the exact case. A short Han
/// term is kept, because there LIKE is the only path.
pub fn like_patterns(terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .filter(|t| recallable(t))
        .map(|t| {
            format!(
                "%{}%",
                t.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The Han-bigram form (t6)
// ---------------------------------------------------------------------------
//
// WHY IT EXISTS, measured (R-A / gen2-recall-spec A8): over the live corpus's
// 74353-token vocabulary, 6922 distinct 2-character Han substrings were drawn
// from long Han tokens, and **63.20% of them are neither a corpus token nor a
// token prefix** -- so both FTS stages miss by construction and the ONLY path
// left was a full-table LIKE scan (measured 10.47 ms in the sampler's Python
// layer, 6.80 ms natively by a second reader, on 10765 chunks). SQLite's own
// `trigram` tokenizer does not help: it was measured at 0/40 on exactly those
// queries, which is what its documentation says ("Substrings consisting of
// fewer than 3 unicode characters do not match any rows"). A Han-bigram index
// answered 40/40 at 0.08 ms.
//
// WHY BIGRAMS AND NOT UNIGRAMS: a unigram index makes every single Han
// character a term, so a 2-character query becomes an OR of two very common
// characters -- evidence so weak that precision collapses. A bigram index makes
// the 2-character query ONE token, which is exactly the substring test.
//
// WHAT THIS PAIR IS NOT: a replacement for `terms`/`match_all`/
// `match_any_prefix`/`like_patterns`. Those four are frozen: the graph crate
// reuses them as the single tokenizer judgement (R-C DEP-5), and a second
// implementation living in another crate is the drift t247 already measured.
// Everything here is ADDITIVE.

/// Whether a character is a Han (CJK ideograph) character.
///
/// Ext A and the compatibility block are included because they appear in real
/// Chinese text; kana and Hangul are deliberately NOT, because unicode61 already
/// emits them as separate terms and bigramming them would only add noise for a
/// corpus whose second language is Chinese.
fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn push_token(out: &mut String, token: &str) {
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(token);
}

/// Emit the tokens for one maximal Han run: a single character stays itself, a
/// longer run becomes its overlapping 2-character bigrams.
fn flush_han_run(run: &mut Vec<char>, out: &mut String) {
    match run.len() {
        0 => {}
        1 => push_token(out, &run[0].to_string()),
        _ => {
            for w in run.windows(2) {
                let bg: String = w.iter().collect();
                push_token(out, &bg);
            }
        }
    }
    run.clear();
}

/// The text to store in `chunks.grams`: the value the `chunks_fts_cjk` shadow
/// index is built over and the only thing that makes a 2-character Han query an
/// indexed lookup.
///
/// Rules, and each one is a decision:
/// * a maximal Han run of length 1 -> that character (a 1-character run is a
///   word, not a fragment);
/// * a maximal Han run of length >= 2 -> its overlapping 2-character bigrams,
///   space separated. Consecutive tokens are consecutive FTS5 positions, which
///   is what makes an ordered PHRASE of bigrams mean "this exact substring";
/// * a non-Han alphanumeric run -> passed through UNCHANGED. Bigramming ASCII
///   would turn "docker" into single characters, i.e. into noise; exact and
///   prefix token matching is already the precision/stage-1 and stage-2 job;
/// * anything else -> a separator.
///
/// Case is left alone: `chunks_fts` indexes raw content and lets unicode61 fold
/// case, and this index must behave the same way.
pub fn han_bigrams(text: &str) -> String {
    let mut out = String::new();
    let mut run: Vec<char> = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if is_han(c) {
            if !word.is_empty() {
                push_token(&mut out, &word);
                word.clear();
            }
            run.push(c);
        } else if c.is_alphanumeric() {
            flush_han_run(&mut run, &mut out);
            word.push(c);
        } else {
            flush_han_run(&mut run, &mut out);
            if !word.is_empty() {
                push_token(&mut out, &word);
                word.clear();
            }
        }
    }
    flush_han_run(&mut run, &mut out);
    if !word.is_empty() {
        push_token(&mut out, &word);
    }
    out
}

/// The bigram form of a query: one quoted PHRASE per Han term, ORed.
///
/// A phrase of the term's own bigrams is exact-substring evidence for that run
/// (the indexed bigrams of a run are consecutive positions, and a run that
/// contains the term contains exactly that consecutive subsequence). ORed, like
/// the prefix recall form: this is the weaker half of the degradation ladder.
///
/// NON-HAN TERMS ARE SKIPPED, deliberately. The bigram index does not contain
/// them (`han_bigrams` passes ASCII through as whole words), so a phrase built
/// from one could never match, and ORing a pattern that cannot match only makes
/// the arm look bigger than it is. A query with no Han term returns "" and the
/// caller must move on to the next stage -- an empty pattern is never handed to
/// MATCH (the same rule `keyword_leg` already applies to the other stages).
pub fn match_bigrams(terms: &[String]) -> String {
    terms
        .iter()
        .filter_map(|t| {
            let chars: Vec<char> = t.chars().collect();
            if chars.is_empty() || !chars.iter().all(|c| is_han(*c)) {
                return None;
            }
            if chars.len() == 1 {
                return Some(quote(t));
            }
            let grams: Vec<String> = chars
                .windows(2)
                .map(|w| w.iter().collect::<String>())
                .collect();
            Some(quote(&grams.join(" ")))
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn terms_split_like_the_tokenizer() {
        assert_eq!(terms("autohotkey-v2"), v(&["autohotkey", "v2"]));
        assert_eq!(terms("deploy.sh"), v(&["deploy", "sh"]));
        assert_eq!(terms("C++"), v(&["c"]));
        assert_eq!(terms("Skills  CLI"), v(&["skills", "cli"]));
        // Han characters are alphanumeric: a run stays ONE term.
        assert_eq!(terms("蓝鲸潜艇"), v(&["蓝鲸潜艇"]));
        assert_eq!(terms("潜艇"), v(&["潜艇"]));
        assert_eq!(terms("银河麒麟 V10"), v(&["银河麒麟", "v10"]));
        assert_eq!(terms(""), Vec::<String>::new());
        assert_eq!(terms("!!!"), Vec::<String>::new());
    }

    #[test]
    fn precision_ands_recall_ors() {
        let t = terms("autohotkey-v2");
        // Precision keeps every term: exact token equality is evidence even for
        // a short one.
        assert_eq!(match_all(&t), "\"autohotkey\" \"v2\"");
        // Recall drops the short ASCII term and keeps the rest.
        assert_eq!(match_any_prefix(&t), "\"autohotkey\"*");
        assert_eq!(match_all(&[]), "");
        assert_eq!(match_any_prefix(&[]), "");
    }

    /// The floor is the same for both recall forms, and it never touches Han.
    #[test]
    fn recall_forms_drop_short_ascii_but_keep_han() {
        // A two-letter ASCII fragment is not evidence in either recall form.
        assert_eq!(match_any_prefix(&terms("me")), "");
        assert_eq!(like_patterns(&terms("me")), Vec::<String>::new());
        // ... but it does not take the other terms of the query down with it.
        assert_eq!(
            match_any_prefix(&terms("t99 control perturbation delete me")),
            "\"t99\"* OR \"control\"* OR \"perturbation\"* OR \"delete\"*"
        );
        assert_eq!(
            like_patterns(&terms("t99 control perturbation delete me")),
            v(&["%t99%", "%control%", "%perturbation%", "%delete%"])
        );
        // The threshold itself, from both sides.
        assert_eq!(match_any_prefix(&v(&["ab"])), "");
        assert_eq!(match_any_prefix(&v(&["abc"])), "\"abc\"*");
        assert_eq!(like_patterns(&v(&["ab"])), Vec::<String>::new());
        assert_eq!(like_patterns(&v(&["abc"])), v(&["%abc%"]));
        // Han is exempt at every length: unicode61 makes a whole run one term,
        // so there the term IS the word, and LIKE is the only path to a part.
        assert_eq!(match_any_prefix(&terms("改键")), "\"改键\"*");
        assert_eq!(like_patterns(&terms("改键")), v(&["%改键%"]));
        assert_eq!(match_any_prefix(&terms("茶")), "\"茶\"*");
        assert_eq!(like_patterns(&terms("茶")), v(&["%茶%"]));
        // Mixed: the Han term survives next to a dropped ASCII one.
        assert_eq!(match_any_prefix(&terms("go 并发")), "\"并发\"*");
        assert_eq!(like_patterns(&terms("go 并发")), v(&["%并发%"]));
    }

    #[test]
    fn like_patterns_escape_wildcards_and_skip_one_ascii_char() {
        assert_eq!(like_patterns(&v(&["a%b"])), v(&["%a\\%b%"]));
        assert_eq!(like_patterns(&terms("潜艇")), v(&["%潜艇%"]));
        assert_eq!(like_patterns(&terms("C++")), Vec::<String>::new());
        assert_eq!(like_patterns(&terms("茶")), v(&["%茶%"]));
    }

    // -- the Han-bigram form (t6) ------------------------------------------

    #[test]
    fn han_bigrams_bigrams_a_run_and_passes_ascii_through() {
        assert_eq!(
            han_bigrams("咖啡研磨度决定萃取速度"),
            "咖啡 啡研 研磨 磨度 度决 决定 定萃 萃取 取速 速度"
        );
        // A one-character run is a word, not a fragment.
        assert_eq!(han_bigrams("茶"), "茶");
        // The ASCII word is NOT bigrammed: bigramming it would make it noise.
        assert_eq!(han_bigrams("docker 常用命令"), "docker 常用 用命 命令");
        assert_eq!(han_bigrams("AutoHotkey v2"), "AutoHotkey v2");
        // Two runs separated by punctuation stay two runs.
        assert_eq!(han_bigrams("改键（热键）"), "改键 热键");
        // Non-Han separators split; nothing is invented for empty input.
        assert_eq!(han_bigrams(""), "");
        assert_eq!(han_bigrams("!!!  "), "");
        // Ext-A ideographs are Han too.
        assert_eq!(han_bigrams("\u{3400}\u{3401}"), "\u{3400}\u{3401}");
    }

    #[test]
    fn match_bigrams_is_a_phrase_of_the_terms_own_bigrams() {
        // A 2-character term is ONE bigram: exactly the substring test.
        assert_eq!(match_bigrams(&terms("水温")), "\"水温\"");
        // A longer run becomes an ordered phrase (adjacent positions in the
        // index == the exact substring).
        assert_eq!(match_bigrams(&terms("研磨度")), "\"研磨 磨度\"");
        assert_eq!(match_bigrams(&terms("蓝鲸潜艇")), "\"蓝鲸 鲸潜 潜艇\"");
        // A one-character Han term has no bigram; it IS a token.
        assert_eq!(match_bigrams(&terms("茶")), "\"茶\"");
        // Non-Han terms are skipped, because the index does not contain them.
        assert_eq!(match_bigrams(&terms("docker")), "");
        assert_eq!(match_bigrams(&terms("C++")), "");
        // ... and skipping one does not take the Han terms down with it.
        assert_eq!(
            match_bigrams(&terms("docker 常用命令")),
            "\"常用 用命 命令\""
        );
        assert_eq!(match_bigrams(&terms("go 并发")), "\"并发\"");
        assert_eq!(match_bigrams(&[]), "");
        // The prefix/exact forms are untouched by any of this.
        assert_eq!(
            match_all(&terms("docker 常用命令")),
            "\"docker\" \"常用命令\""
        );
        assert_eq!(
            match_any_prefix(&terms("docker 常用命令")),
            "\"docker\"* OR \"常用命令\"*"
        );
    }

    /// The knife's edge: the floor that drops short ASCII fragments must NOT
    /// touch Han, and the bigram arm must not apply it either (a 1-character Han
    /// term is a word, and it is the ONLY path for it).
    #[test]
    fn the_recall_floor_never_reaches_han_in_any_form() {
        // The floor's value is pinned at compile time: a runtime `assert!` on a
        // constant is a tautology, and clippy says so.
        const _: () = assert!(MIN_RECALL_ASCII == 3);
        assert!(!recallable("ab"));
        assert!(recallable("abc"));
        assert!(recallable("茶"));
        assert_eq!(match_bigrams(&terms("茶")), "\"茶\"");
        assert_eq!(like_patterns(&terms("茶")), v(&["%茶%"]));
    }
}
