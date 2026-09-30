//! The text mechanics both algorithms share: sentence boundaries, marker
//! matching, tokenising, normalisation.
//!
//! Why this module exists instead of a dependency on the store crate's
//! tokenizer (`crates/store/src/fts.rs`), in the design's own words (design
//! §7.2): tokenisation is deliberately local rather than a dependency on
//! `ruagent_store::fts`, because depending on the store crate would drag its
//! database driver — i.e. I/O — into a crate whose defining property is that it
//! has none. The duplication is bounded to one module and is honest: the
//! extractor needs (a) sentence boundaries, (b) literal marker matching,
//! (c) identifier/Han-run tokens — not an FTS query builder.
//!
//! Two shapes are mirrored from `crates/graph/src/lib.rs` on purpose, so the
//! extractor hands the write path the spellings it already understands:
//! `variants_of` mirrors `variants` (graph/src/lib.rs:909-926) and `acronym`
//! mirrors `acronym` (graph/src/lib.rs:930-944). The mirror is documented here
//! and asserted by the gold tests; it is not a second opinion about identity —
//! the graph's judge remains the referee (graph/src/lib.rs:994-1024).
//!
//! ONE asymmetry, deliberate and reported: the mirror's OTHER side is not
//! byte-safe. `ruagent_graph::variants` (graph/src/lib.rs:916) carries the same
//! `n[open + 1..close]` assumption this module used to carry, so a name with a
//! full-width parenthetical would panic there too. graph/ is outside this
//! crate's scope; the site is named in the write-up for the
//! full-width-parenthetical fix (ruagent-close-the-gaps t8) rather than
//! silently left behind.
//!
//! Nothing here reads a clock, an environment variable, a file or a network
//! socket, and nothing iterates an unordered collection.

/// Han characters the term rules treat as Chinese text: the main block, its
/// extension A, and the compatibility ideographs.
pub fn is_han(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
}

/// Collapse every whitespace run to a single space and trim the ends.
pub fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
            }
            pending_space = false;
            out.push(c);
        }
    }
    out
}

/// `norm(s)` (design §7.4.4): trim, collapse whitespace, lowercase, strip ONE
/// trailing `.` or `。`. Used ONLY for identity/dedup keys — never for what gets
/// written, because a key is not content.
pub fn norm(s: &str) -> String {
    let collapsed = collapse(s).to_lowercase();
    let stripped = collapsed
        .strip_suffix('.')
        .or_else(|| collapsed.strip_suffix('。'))
        .unwrap_or(collapsed.as_str());
    stripped.trim_end().to_string()
}

/// Lowercase a whole string once, so per-marker matching does not re-lowercase
/// it. ASCII markers fold; Han is unchanged; non-ASCII casing (`İ`, `ß`) may
/// change byte length, which is why offsets are never derived from this string.
pub fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// Split `text` into sentences, each with the byte offset of its raw start
/// (design §7.4.2): split on `。！？!?`, on `.`/`;` only when followed by
/// whitespace, and on newlines. Sentences are trimmed, whitespace-collapsed, and
/// empty or single-character sentences are dropped.
pub fn split_sentences(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut start = 0usize;
    for (n, &(i, c)) in chars.iter().enumerate() {
        let hard = matches!(c, '。' | '！' | '？' | '!' | '?' | '\n');
        let soft = matches!(c, '.' | ';')
            && chars
                .get(n + 1)
                .map(|&(_, next)| next.is_whitespace())
                .unwrap_or(false);
        if hard || soft {
            push_sentence(&mut out, &text[start..i + c.len_utf8()], start);
            start = i + c.len_utf8();
        }
    }
    if start < text.len() {
        push_sentence(&mut out, &text[start..], start);
    }
    out
}

fn push_sentence(out: &mut Vec<(usize, String)>, raw: &str, offset: usize) {
    let sentence = collapse(raw);
    if sentence.chars().count() >= 2 {
        out.push((offset, sentence));
    }
}

/// The sentences of `text`, without their offsets.
pub fn sentences(text: &str) -> Vec<String> {
    split_sentences(text).into_iter().map(|(_, s)| s).collect()
}

/// A "content token" (design §8.7): an ASCII alphanumeric run of length >= 3,
/// or a Han run of length >= 2. This is the guard that stops a sentence whose
/// only content is a bare acknowledgement from becoming a content-bearing
/// candidate.
pub fn has_content_token(s: &str) -> bool {
    let mut ascii_run = 0usize;
    let mut han_run = 0usize;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            ascii_run += 1;
            han_run = 0;
            if ascii_run >= 3 {
                return true;
            }
        } else if is_han(c) {
            han_run += 1;
            ascii_run = 0;
            if han_run >= 2 {
                return true;
            }
        } else {
            ascii_run = 0;
            han_run = 0;
        }
    }
    false
}

/// Design §8.7: a sentence ending in a question mark is not a candidate (a
/// question is not an assertion about the user).
pub fn is_question(s: &str) -> bool {
    let t = s.trim_end();
    t.ends_with('?') || t.ends_with('？')
}

/// Characters that negate the marker that follows them. The single reason this
/// guard exists is in `rules.rs`' header: `CORRECT` contains "不对" and `CONFIRM`
/// contains "对".
const NEGATION_CHARS: &[char] = &['不', '没', '別', '别', '未', '勿'];

/// ASCII words that negate the marker that follows them.
const NEGATION_WORDS: &[&str] = &["not ", "no, ", "n't ", "never "];

/// Whole-marker containment over an already-lowercased haystack, skipping
/// occurrences that a preceding negation character or word negates.
pub fn contains_marker(hay_lower: &str, marker: &str) -> bool {
    if marker.is_empty() {
        return false;
    }
    hay_lower
        .match_indices(marker)
        .any(|(i, _)| !negated_before(hay_lower, i))
}

fn negated_before(hay: &str, i: usize) -> bool {
    if i == 0 {
        return false;
    }
    let before = &hay[..i];
    if let Some(c) = before.chars().next_back()
        && NEGATION_CHARS.contains(&c)
    {
        return true;
    }
    let mut k = before.len().saturating_sub(6);
    while k < before.len() && !before.is_char_boundary(k) {
        k += 1;
    }
    NEGATION_WORDS.iter().any(|p| before[k..].ends_with(p))
}

/// Does the haystack contain ANY of these markers?
pub fn contains_any(hay_lower: &str, markers: &[&str]) -> bool {
    markers.iter().any(|m| contains_marker(hay_lower, m))
}

/// The last of `ascii` / `wide` in `s`, with the character that matched, so a
/// caller can advance past the delimiter by ITS OWN byte length.
fn last_delimiter(s: &str, ascii: char, wide: char) -> Option<(usize, char)> {
    match (s.rfind(ascii), s.rfind(wide)) {
        (Some(i), Some(j)) => Some(if i > j { (i, ascii) } else { (j, wide) }),
        (Some(i), None) => Some((i, ascii)),
        (None, Some(j)) => Some((j, wide)),
        (None, None) => None,
    }
}

/// The content of ONE trailing parenthetical group, as `(outer, inner)`:
/// `DeepSeek Harness (dsh)` -> `("deepseek harness", "dsh")`, and
/// `基础知识（准备）` -> `("基础知识", "准备")`.
///
/// # Why this is not `open + 1`
///
/// It used to be. `（` is THREE bytes, so `name[open + 1..close]` landed inside
/// the character and panicked: "start byte index 13 is not a char boundary; it
/// is inside '（' (bytes 12..15) of `基础知识（准备）`" (text.rs:190). Measured on
/// the user's real knowledge base: 116 of 936 documents panicked there, and the
/// live ingest route answered a dropped connection with no error body. Full-width
/// punctuation is ordinary Chinese text, so the assumption "a parenthesis is one
/// byte" — not the input — was the bug.
///
/// Now every index is either produced by `rfind` (a character boundary) or
/// advanced by the matched delimiter's own `len_utf8()` (1 for `(`/`)`, 3 for
/// `（`/`）`), so no byte-width assumption remains anywhere in the function.
///
/// # Mixed delimiters: paired by POSITION
///
/// `矩阵（matrix)` and `Matrix (矩阵）` each yield one group. Width is a
/// keyboard/encoding artifact, not a semantic one, so refusing to pair across
/// widths would silently drop a real alias and would leave two spellings of the
/// same construct with different answers. The tie-break for a name holding both
/// closer forms is also positional: the LAST delimiter wins, which is what
/// "trailing" means. (The old code preferred an ASCII `)` anywhere over a later
/// `）`; that is the only shape whose answer could move, and the corpus
/// measurement for ruagent-close-the-gaps t8 found no document where it does.)
///
/// # Mirror
///
/// The shape is still mirrored from `ruagent_graph::variants`
/// (graph/src/lib.rs:909-926). That function carries the SAME one-byte
/// assumption at graph/src/lib.rs:916 and is out of this crate's scope; it is
/// named in the t8 write-up so the same panic cannot be rediscovered later.
pub fn trailing_parenthetical(name: &str) -> Option<(String, String)> {
    let (close, _) = last_delimiter(name, ')', '）')?;
    let (open, opener) = last_delimiter(&name[..close], '(', '（')?;
    let inner_start = open + opener.len_utf8();
    Some((
        name[..open].trim().to_string(),
        name[inner_start..close].trim().to_string(),
    ))
}

/// Every spelling a name carries: the base, plus the content of one trailing
/// parenthetical group (mirrors `ruagent_graph::variants`).
pub fn variants_of(name: &str) -> Vec<String> {
    let base = collapse(name).to_lowercase();
    let mut out = vec![base.clone()];
    if let Some((outer, inner)) = trailing_parenthetical(&base) {
        if !outer.is_empty() {
            out.insert(0, outer);
        }
        if !inner.is_empty() {
            out.push(inner);
        }
    }
    out.dedup();
    out
}

/// The identity form of a name: lowercased, whitespace-collapsed, one trailing
/// parenthetical removed (the `base_name` shape, design §9.5).
pub fn base_name(name: &str) -> String {
    let base = collapse(name).to_lowercase();
    match trailing_parenthetical(&base) {
        Some((outer, _)) if !outer.is_empty() => outer,
        _ => base,
    }
}

/// The initials of a multi-word name: `Agent Client Protocol` -> `ACP`. One word
/// (or a single Han run) has no acronym (mirrors `ruagent_graph::acronym`).
pub fn acronym(name: &str) -> Option<String> {
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    let mut out = String::new();
    for w in words {
        let c = w.chars().next()?;
        if !c.is_alphanumeric() {
            return None;
        }
        out.push(c);
    }
    Some(out)
}

/// An acronym that is safe to offer as an alias candidate.
///
/// Two bounds beyond `acronym`: every accepted character must be ASCII
/// alphanumeric (an acronym of Han characters spells nothing in either script),
/// and the result must reach `MIN_ACRONYM_CHARS` — see that constant for why a
/// two-character acronym is refused.
pub fn acronym_alias(name: &str) -> Option<String> {
    let ac = acronym(name)?;
    if !ac.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    if ac.chars().count() < crate::rules::MIN_ACRONYM_CHARS {
        return None;
    }
    Some(ac)
}

/// Case-insensitive suffix match. Returns false instead of panicking when the
/// byte length cannot line up with a character boundary.
pub fn ends_with_ci(hay: &str, needle: &str) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    let k = hay.len() - needle.len();
    hay.is_char_boundary(k) && hay[k..].eq_ignore_ascii_case(needle)
}

/// Case-insensitive prefix match, with the same boundary guard.
pub fn starts_with_ci(hay: &str, needle: &str) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    hay.is_char_boundary(needle.len()) && hay[..needle.len()].eq_ignore_ascii_case(needle)
}

/// First case-insensitive occurrence of `needle` in `hay`, as a byte index.
pub fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let n = needle.len();
    if n == 0 || hay.len() < n {
        return None;
    }
    let mut i = 0;
    while i + n <= hay.len() {
        if hay.is_char_boundary(i)
            && hay.is_char_boundary(i + n)
            && hay[i..i + n].eq_ignore_ascii_case(needle)
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Trim whitespace and punctuation from the END of a relation endpoint: what
/// separates a name from its connector is markdown (` `, `` ` ``, `*`) or
/// punctuation, and neither is part of the name.
pub fn trim_end_boundary(s: &str) -> &str {
    s.trim_end().trim_end_matches(is_boundary_punct)
}

/// Trim whitespace and punctuation from the START of a relation endpoint.
pub fn trim_start_boundary(s: &str) -> &str {
    s.trim_start().trim_start_matches(is_boundary_punct)
}

/// ASCII and full-width punctuation that can surround a name in prose.
fn is_boundary_punct(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '，' | '。'
                | '、'
                | '；'
                | '：'
                | '“'
                | '”'
                | '‘'
                | '’'
                | '（'
                | '）'
                | '《'
                | '》'
                | '「'
                | '」'
                | '『'
                | '』'
                | '！'
                | '？'
                | '…'
                | '—'
                | '·'
        )
}

/// An explicit ISO-8601/RFC3339 instant written verbatim in `s`
/// (`2026-09-14`, `2026-09-14T21:39:08+08:00`), else `None`.
///
/// This is the ONLY way the free tier can produce an event time: the rules read
/// no clock (there is none in this crate), so "the transcript states a time"
/// and "no time is known" stay different facts — the distinction the LLM prompt
/// insists on (distill.rs:27: "NEVER put the current time there").
pub fn iso_datetime(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0usize;
    while i + 10 <= b.len() {
        let digit_run = b[i].is_ascii_digit()
            && b[i + 1].is_ascii_digit()
            && b[i + 2].is_ascii_digit()
            && b[i + 3].is_ascii_digit();
        let date_shape = digit_run
            && b[i + 4] == b'-'
            && b[i + 5].is_ascii_digit()
            && b[i + 6].is_ascii_digit()
            && b[i + 7] == b'-'
            && b[i + 8].is_ascii_digit()
            && b[i + 9].is_ascii_digit();
        let not_inside_a_number = i == 0 || !b[i - 1].is_ascii_digit();
        if date_shape && not_inside_a_number {
            let mut end = i + 10;
            if matches!(b.get(end), Some(b'T') | Some(b' ')) {
                let t = end + 1;
                let has_time = t + 5 <= b.len()
                    && b[t].is_ascii_digit()
                    && b[t + 1].is_ascii_digit()
                    && b[t + 2] == b':'
                    && b[t + 3].is_ascii_digit()
                    && b[t + 4].is_ascii_digit();
                if has_time {
                    end = t + 5;
                    // `:SS`. The two bytes after the colon are consumed as a byte
                    // COUNT, so they are required to be single-byte first:
                    // `21:39:中文` used to slice inside '中' and panic at
                    // text.rs:388 ("end byte index 19 is not a char boundary").
                    // ASCII-only input behaves exactly as before.
                    if b.get(end) == Some(&b':')
                        && end + 3 <= b.len()
                        && b[end + 1..end + 3].iter().all(u8::is_ascii)
                    {
                        end += 3;
                    }
                    if b.get(end) == Some(&b'.') {
                        let mut k = end + 1;
                        while k < b.len() && b[k].is_ascii_digit() {
                            k += 1;
                        }
                        end = k;
                    }
                    match b.get(end) {
                        Some(&b'Z') => end += 1,
                        // `±HH:MM`: five bytes consumed as a byte count, so they
                        // must be single-byte (`+中文字`, `+😀😀` used to panic
                        // here for the same reason as the seconds above).
                        Some(&sign)
                            if (sign == b'+' || sign == b'-')
                                && end + 6 <= b.len()
                                && b[end + 1..end + 6].iter().all(u8::is_ascii) =>
                        {
                            end += 6;
                        }
                        _ => {}
                    }
                }
            }
            return Some(s[i..end].to_string());
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // The full-width parenthetical (ruagent-close-the-gaps t8). Measured on the
    // user's real knowledge base: 116 of 936 documents made `graph_candidates`
    // panic here, and the connection of the live ingest route simply dropped
    // with no error body. Full-width punctuation is ordinary Chinese text, so
    // this is the shape to pin, not an edge case.
    // -----------------------------------------------------------------------

    /// The EXACT shape found in the corpus: a Han term whose trailing group uses
    /// the full-width pair.
    #[test]
    fn a_full_width_parenthetical_is_one_group() {
        let (outer, inner) = trailing_parenthetical("基础知识（准备）").expect("one group");
        assert_eq!(outer, "基础知识");
        assert_eq!(inner, "准备");
        // The two derived shapes the write path merges on.
        assert_eq!(base_name("基础知识（准备）"), "基础知识");
        assert_eq!(
            variants_of("基础知识（准备）"),
            vec![
                "基础知识".to_string(),
                "基础知识（准备）".to_string(),
                "准备".to_string()
            ]
        );
    }

    /// A one-byte parenthesis is unchanged: `base_name`/`variants_of` keep the
    /// gold behaviour (`DeepSeek Harness (dsh)` shares a base with `dsh`).
    #[test]
    fn a_one_byte_parenthetical_is_unchanged() {
        let (outer, inner) = trailing_parenthetical("deepseek harness (dsh)").expect("one group");
        assert_eq!(outer, "deepseek harness");
        assert_eq!(inner, "dsh");
        assert_eq!(base_name("DeepSeek Harness (dsh)"), "deepseek harness");
        assert_eq!(
            variants_of("DeepSeek Harness (dsh)"),
            vec![
                "deepseek harness".to_string(),
                "deepseek harness (dsh)".to_string(),
                "dsh".to_string()
            ]
        );
    }

    /// Width is a keyboard/encoding artifact, not a semantic one: a group is
    /// paired by POSITION, so the mixed shapes each yield one group instead of
    /// being dropped (or crashing).
    #[test]
    fn mixed_width_delimiters_are_paired_by_position() {
        assert_eq!(
            trailing_parenthetical("矩阵（matrix)"),
            Some(("矩阵".to_string(), "matrix".to_string()))
        );
        assert_eq!(
            trailing_parenthetical("Matrix (矩阵）"),
            Some(("Matrix".to_string(), "矩阵".to_string()))
        );
    }

    /// A closer with no opener before it, and an opener with no closer, are both
    /// "no group" — the function is total.
    #[test]
    fn unmatched_delimiters_yield_no_group() {
        assert_eq!(trailing_parenthetical("没有开括号）"), None);
        assert_eq!(trailing_parenthetical("没有闭括号（"), None);
        assert_eq!(
            trailing_parenthetical("空组（）"),
            Some(("空组".to_string(), String::new())),
            "an empty group is a group: the outer name survives, the inner is empty"
        );
    }

    /// The other index in this file that advanced by a byte count without
    /// checking the bytes were single-byte: `iso_datetime`'s `:SS` and its
    /// `±HH:MM` offset. Both are reachable from a document's text.
    #[test]
    fn iso_datetime_does_not_slice_into_a_multibyte_suffix() {
        // `:中` is three bytes, so `end += 3` used to land inside it.
        assert_eq!(
            iso_datetime("2026-09-14T21:39:中文"),
            Some("2026-09-14T21:39".to_string())
        );
        // `+中文字` is nine bytes, so `end += 6` used to land inside a character.
        assert_eq!(
            iso_datetime("2026-09-14T21:39+中文字"),
            Some("2026-09-14T21:39".to_string())
        );
        // A four-byte character after the offset sign: `end += 6` lands inside it.
        assert_eq!(
            iso_datetime("2026-09-14T21:39+😀😀"),
            Some("2026-09-14T21:39".to_string())
        );
        // The shapes the function exists for are untouched.
        assert_eq!(
            iso_datetime("as of 2026-09-14T21:39:08+08:00 ok"),
            Some("2026-09-14T21:39:08+08:00".to_string())
        );
        assert_eq!(iso_datetime("2026-09-14"), Some("2026-09-14".to_string()));
    }
}
