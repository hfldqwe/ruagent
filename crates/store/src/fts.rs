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
//! and LIKE patterns for the one case FTS5 cannot express.

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
const MIN_RECALL_ASCII: usize = 3;

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
}
