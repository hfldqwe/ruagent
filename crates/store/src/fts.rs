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

/// Recall form: every term as a prefix, ORed.
pub fn match_any_prefix(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| format!("{}*", quote(t)))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// LIKE patterns for the one case FTS5 cannot express: a substring of a term.
///
/// A single ASCII alphanumeric character is skipped on purpose: a pattern of
/// that shape matches almost every row, and token equality in the precision
/// form already covers it. A single Han character is kept, because there LIKE
/// is the only path.
pub fn like_patterns(terms: &[String]) -> Vec<String> {
    terms
        .iter()
        .filter(|t| !(t.chars().count() == 1 && t.is_ascii()))
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
        assert_eq!(match_all(&t), "\"autohotkey\" \"v2\"");
        assert_eq!(match_any_prefix(&t), "\"autohotkey\"* OR \"v2\"*");
        assert_eq!(match_all(&[]), "");
    }

    #[test]
    fn like_patterns_escape_wildcards_and_skip_one_ascii_char() {
        assert_eq!(like_patterns(&v(&["a%b"])), v(&["%a\\%b%"]));
        assert_eq!(like_patterns(&terms("潜艇")), v(&["%潜艇%"]));
        assert_eq!(like_patterns(&terms("C++")), Vec::<String>::new());
        assert_eq!(like_patterns(&terms("茶")), v(&["%茶%"]));
    }
}
