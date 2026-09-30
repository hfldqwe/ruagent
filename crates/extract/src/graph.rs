//! Algorithm B (design §9): knowledge document text becomes entity, alias and
//! relation candidates. Zero tokens, no model, no I/O.
//!
//! The unit is the DOCUMENT, not the chunk. §9.1 states the deviation and its
//! reason: overlapping chunks (100 bytes by default,
//! crates/knowledge/src/chunk.rs:11-12) would state the same entity many times
//! and multiply the candidate count, which is exactly the unbounded growth the
//! acceptance criteria forbid. `graph_candidates` is unit-agnostic — it takes
//! any text — but its document-frequency gates (at least 2 for a capitalized
//! phrase, at least 3 for a Han run) assume a document-sized window, so a
//! per-chunk caller would fire on almost nothing.
//!
//! Five entity rules feed one identity space, in the fixed priority of
//! [`rules::ENTITY_RULE_PRIORITY`]: `heading_entity`, `wiki_link`,
//! `inline_code_identifier`, `proper_noun_phrase`, `chinese_term_run`. Four
//! relations are possible, from the closed table of §9.4, and a relation may
//! only name entities this same pass emitted.
//!
//! Determinism: one ordered pass over the windowed text fills ordered term
//! tables; nothing iterates an unordered collection; every sort ends with a
//! tie-break on the name, so equal scores cannot reorder between runs.

use std::collections::{BTreeMap, BTreeSet};

use crate::rules;
use crate::text;
use crate::{
    CandidateOrigin, EntityCandidate, ExtractLimits, GraphCandidates, RelationCandidate,
    RelationName,
};

/// Knowledge document text -> entity/alias/relation candidates.
pub fn graph_candidates(input: &str, limits: &ExtractLimits) -> GraphCandidates {
    graph_candidates_for("", input, limits)
}

/// The same pass, stamping `source` (the caller's own provenance label — the
/// document or chunk id) onto every candidate it emits.
pub fn graph_candidates_for(source: &str, input: &str, limits: &ExtractLimits) -> GraphCandidates {
    // §7.4.1 windowing: keep the first max_text_bytes, cut at a paragraph
    // boundary — a document's head states its terms.
    let (window, bytes_skipped) = window_text(input, limits.max_text_bytes);
    // The flag is the LOSS bit, not a description of one loss: it starts as the
    // input-text cut and each place below that DROPS a candidate raises it too.
    // Two caps, one truth (see the `truncated` field docs in lib.rs).
    let mut truncated = bytes_skipped > 0;

    let headings = scan_headings(window);
    let heading_offsets: Vec<usize> = headings.iter().map(|h| h.offset).collect();

    let mut tables = Tables::default();
    let mut section_first_body: BTreeMap<usize, Option<String>> = BTreeMap::new();
    let mut sentences: Vec<Sentence> = Vec::new();

    for (offset, sentence) in text::split_sentences(window) {
        let section = heading_offsets.partition_point(|o| *o < offset);
        let is_heading_line = heading_offsets.binary_search(&offset).is_ok();
        if !is_heading_line {
            section_first_body.entry(section).or_insert_with(|| {
                if sentence.len() <= limits.max_content_bytes {
                    Some(sentence.clone())
                } else {
                    None
                }
            });
        }
        let ctx = Ctx {
            sentence: &sentence,
            offset,
            section,
            max_content_bytes: limits.max_content_bytes,
        };
        scan_sentence(&ctx, is_heading_line, limits, &mut tables);
        sentences.push(Sentence {
            offset,
            section,
            text: sentence,
        });
    }

    // ---- entities, in rule order (design §9.2) ----------------------------
    let mut raw: Vec<RawEntity> = Vec::new();

    for heading in &headings {
        let name = heading.name.clone();
        if !heading_name_is_usable(&name, limits.max_entity_name_chars) {
            continue;
        }
        let summary = section_first_body
            .get(&heading.section)
            .and_then(|s| s.clone());
        raw.push(RawEntity {
            aliases: heading_aliases(&name),
            kind: Some(if is_ident_shape(&name) {
                "tool"
            } else {
                "concept"
            }),
            // §9.3: a heading is a deliberate term, not a coincidence, so its
            // ranking has a floor of 0.5 (df = 1 would otherwise give 0.25).
            score: 0.5f32.max(intrinsic_score(1)),
            name,
            summary,
            rule: rules::RULE_HEADING_ENTITY,
            first: heading.offset,
            section: heading.section,
        });
    }

    for term in tables.wiki.values() {
        raw.push(term.raw_entity(
            rules::RULE_WIKI_LINK,
            Some("concept"),
            code_twin(&term.name).into_iter().collect(),
        ));
    }

    for term in tables.code.values() {
        raw.push(term.raw_entity(
            rules::RULE_INLINE_CODE,
            Some("concept"),
            code_twin(&term.name).into_iter().collect(),
        ));
    }

    for term in tables.phrase.values() {
        // §9.2: document frequency >= 2 AND not sentence-initial-only. A phrase
        // seen only where a sentence happens to start is sentence capitalization,
        // not a name — this is the bound on "every capitalized word is an entity".
        if term.df < 2 || term.non_initial_df == 0 {
            continue;
        }
        let aliases = text::acronym_alias(&term.name).into_iter().collect();
        raw.push(term.raw_entity(rules::RULE_PROPER_NOUN, Some("concept"), aliases));
    }

    for term in tables.han.values() {
        // §9.2: document frequency >= 3 — a Chinese term has to recur.
        if term.df < 3 {
            continue;
        }
        raw.push(term.raw_entity(rules::RULE_HAN_TERM, Some("concept"), Vec::new()));
    }

    // §9.5: entity identity is norm(base_name(name)), and the first rule in
    // `rules::ENTITY_RULE_PRIORITY` owns a term several rules named — a heading
    // owns what an inline-code span also mentioned, and the owner's
    // `rule`/`kind`/`aliases`/`score` are what is emitted. One source of truth
    // for that order, so it cannot drift from the documented one.
    raw.sort_by_key(|e| rule_priority(e.rule)); // stable: BTreeMap order within a rule
    let mut seen: BTreeSet<String> = BTreeSet::new();
    raw.retain(|e| seen.insert(e.dedup_key()));

    // §9.5 ordering: score descending, then first mention ascending, then name
    // (the last key exists so two candidates with the same score AND the same
    // first-use offset still have one total order).
    raw.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.first.cmp(&b.first))
            .then(a.name.cmp(&b.name))
    });
    // §9.3: the ranking cut (`min_confidence` in the capability options). Memory
    // candidates are never filtered this way — only truncated. A score cut is
    // the caller's own setting applied to every input, so it is NOT a loss and
    // must not raise the flag; the candidate cap below is, and does.
    raw.retain(|e| e.score >= limits.min_score);
    if raw.len() > limits.max_per_input {
        // The sibling of the memory cap's defect (crates/extract/src/memory.rs
        // sets its flag at exactly this kind of drop): `raw.truncate` below
        // silently discards the candidates past the cap, and a flag that only
        // reported the input-text cut answered `false` while doing it — the one
        // direction a caller cannot guard against. `exactly 96` (nothing to
        // drop) stays `false`: the test is the drop, not the length.
        truncated = true;
    }
    raw.truncate(limits.max_per_input);

    let entities: Vec<EntityCandidate> = raw
        .into_iter()
        .map(|e| EntityCandidate {
            name: e.name,
            kind: e.kind,
            summary: e.summary,
            aliases: e.aliases,
            score: e.score,
            rule: e.rule,
            origin: CandidateOrigin::Document {
                section: Some(e.section),
            },
            source: source.to_string(),
        })
        .collect();

    // ---- relations (design §9.4) ------------------------------------------
    // The endpoint index is built from the entities that SURVIVED the cap, so a
    // relation can never name an entity this pass did not emit — the same
    // discipline as the graph's write path, which skips relations to unlisted
    // entities (graph/src/lib.rs:1467-1469).
    let endpoints = EndpointIndex::build(&entities);
    let mut relations = scan_relations(&sentences, &endpoints, limits, source);

    // §9.5: the two lists share one budget, entities first (dropping an entity
    // would orphan the relations that name it).
    let budget = limits.max_per_input.saturating_sub(entities.len());
    if relations.len() > budget {
        // The same loss at the same cap: a relation dropped here is a candidate
        // the caller will never see, so the flag has to say so.
        truncated = true;
    }
    relations.truncate(budget);

    GraphCandidates {
        entities,
        relations,
        truncated,
        bytes_skipped,
    }
}

/// Keep the first `max` bytes, cut back to the previous paragraph boundary so a
/// paragraph is never half-extracted. The cut never splits a character.
fn window_text(input: &str, max: usize) -> (&str, usize) {
    if input.len() <= max {
        return (input, 0);
    }
    let mut cut = max.min(input.len());
    while cut > 0 && !input.is_char_boundary(cut) {
        cut -= 1;
    }
    let prefix = &input[..cut];
    let end = match prefix.rfind("\n\n") {
        Some(p) => p + 2,
        None => cut,
    };
    let window = &input[..end];
    (window, input.len() - window.len())
}

// ---------------------------------------------------------------------------
// One pass over the window
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Tables {
    code: BTreeMap<String, Term>,
    wiki: BTreeMap<String, Term>,
    phrase: BTreeMap<String, Term>,
    han: BTreeMap<String, Term>,
}

#[derive(Debug, Clone)]
struct Term {
    /// The spelling of the FIRST occurrence — what gets emitted (verbatim).
    name: String,
    df: usize,
    /// Occurrences that are not at the start of their sentence. Only
    /// `proper_noun_phrase` reads it (§9.2).
    non_initial_df: usize,
    /// The first sentence the term appeared in, when it fits the body cap.
    first_sentence: Option<String>,
    first: usize,
    section: usize,
    /// The kind the rule could determine from context (an inline-code span whose
    /// sentence says it is run is a `tool`); `None` means "the rule's own
    /// default".
    kind: Option<&'static str>,
    /// An extra spelling the source itself supplied (a wiki link's display form).
    alias: Option<String>,
}

impl Term {
    fn raw_entity(
        &self,
        rule: &'static str,
        kind: Option<&'static str>,
        aliases: Vec<String>,
    ) -> RawEntity {
        let mut all: Vec<String> = Vec::new();
        if let Some(alias) = &self.alias {
            all.push(alias.clone());
        }
        all.extend(aliases);
        all.dedup();
        RawEntity {
            name: self.name.clone(),
            kind: self.kind.or(kind),
            summary: self.first_sentence.clone(),
            aliases: all,
            score: intrinsic_score(self.df),
            rule,
            first: self.first,
            section: self.section,
        }
    }
}

/// What one sentence contributes to a term table.
struct Ctx<'a> {
    sentence: &'a str,
    /// Byte offset of the sentence in the window.
    offset: usize,
    section: usize,
    max_content_bytes: usize,
}

/// One term occurrence to be counted.
struct TermSeed<'a> {
    /// The identity key inside the table (`norm(name)` for phrases, the spelling
    /// itself for identifiers and Han runs).
    key: &'a str,
    name: &'a str,
    non_initial: bool,
    kind: Option<&'static str>,
    alias: Option<String>,
}

fn bump(table: &mut BTreeMap<String, Term>, ctx: &Ctx<'_>, seed: TermSeed<'_>) {
    if let Some(term) = table.get_mut(seed.key) {
        term.df += 1;
        if seed.non_initial {
            term.non_initial_df += 1;
        }
        // A tool stays a tool however it was spelled later on.
        if seed.kind == Some("tool") {
            term.kind = Some("tool");
        }
        if term.alias.is_none() {
            term.alias = seed.alias;
        }
        return;
    }
    // The cap is what stops a pathological input from growing the table; past it,
    // new terms are simply not counted (first-occurrence order wins, §9.2).
    if table.len() >= rules::MAX_DISTINCT_TERMS {
        return;
    }
    table.insert(
        seed.key.to_string(),
        Term {
            name: seed.name.to_string(),
            df: 1,
            non_initial_df: usize::from(seed.non_initial),
            first_sentence: if ctx.sentence.len() <= ctx.max_content_bytes {
                Some(ctx.sentence.to_string())
            } else {
                None
            },
            first: ctx.offset,
            section: ctx.section,
            kind: seed.kind,
            alias: seed.alias,
        },
    );
}

/// §9.3: `score = df / (df + 3.0)`; df = 1 -> 0.25, df = 3 -> 0.5, df = 9 -> 0.75.
fn intrinsic_score(df: usize) -> f32 {
    let df = df as f32;
    df / (df + 3.0)
}

struct Heading {
    offset: usize,
    /// 1-based: heading *n* owns section *n*, section 0 is the preamble.
    section: usize,
    name: String,
}

struct Sentence {
    offset: usize,
    section: usize,
    text: String,
}

fn scan_headings(window: &str) -> Vec<Heading> {
    let mut out: Vec<Heading> = Vec::new();
    let mut offset = 0usize;
    for line in window.split_inclusive('\n') {
        if let Some(name) = heading_name(line) {
            out.push(Heading {
                offset,
                section: out.len() + 1,
                name,
            });
        }
        offset += line.len();
    }
    out
}

/// `^#{1,6}\s+` (design §9.2), then the name rules: surrounding backticks (a
/// heading that is one inline-code span), leading numbering (`1.`, `3.2.`),
/// trailing `:`, and the length floor are applied here; the length ceiling and
/// the stop list are applied by [`heading_name_is_usable`].
fn heading_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let mut name = rest.trim();
    name = name.strip_prefix('`').unwrap_or(name);
    name = name.strip_suffix('`').unwrap_or(name);
    let name = strip_numbering(name.trim());
    let name = name.trim_end_matches([':', '：']).trim();
    if name.chars().count() < 2 {
        return None;
    }
    Some(name.to_string())
}

fn strip_numbering(s: &str) -> &str {
    let digits = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .count();
    if digits == 0 {
        return s;
    }
    let rest = &s[digits..];
    let rest = rest.strip_prefix(')').unwrap_or(rest);
    if rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        s
    }
}

fn heading_name_is_usable(name: &str, max_chars: usize) -> bool {
    if name.chars().count() > max_chars {
        return false;
    }
    let norm = text::norm(name);
    let base = text::base_name(name);
    !rules::STOP_HEADINGS
        .iter()
        .any(|s| *s == norm || *s == base)
}

/// Aliases for a heading (§9.2): the content of ONE trailing parenthetical group
/// (the `variants` shape, graph/src/lib.rs:909-926) plus an acronym candidate.
fn heading_aliases(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let lowered = text::collapse(name).to_lowercase();
    if let Some((_, inner)) = text::trailing_parenthetical(&lowered)
        && !inner.is_empty()
        && text::norm(&inner) != text::norm(name)
    {
        out.push(inner);
    }
    // The acronym of the OUTER name, not of the parenthetical: `Agent Client
    // Protocol (ACP)` already spells its alias inside the parentheses, and
    // `DeepSeek Harness (dsh)` gets `dsh` there rather than the `dh` bound of
    // `MIN_ACRONYM_CHARS`.
    if let Some(acronym) = text::acronym_alias(&text::base_name(name)) {
        out.push(acronym);
    }
    out.dedup();
    out
}

/// The hyphen/underscore twin of an identifier (§9.2): `a_b` <-> `a-b`.
fn code_twin(name: &str) -> Option<String> {
    if name.contains('_') {
        Some(name.replace('_', "-"))
    } else if name.contains('-') {
        Some(name.replace('-', "_"))
    } else {
        None
    }
}

/// `^[A-Za-z][A-Za-z0-9_.:/-]{2,60}$` (design §9.2).
fn is_ident_shape(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    let rest: Vec<char> = chars.collect();
    if rest.len() < 2 || rest.len() > 60 {
        return false;
    }
    rest.iter()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '/' | '-'))
}

/// Is this inline-code span a `tool` rather than a `concept`? (§9.2: it contains
/// a slash, or carries a known extension, or its sentence says it is run.)
fn is_tool_span(span: &str) -> bool {
    let lower = span.to_lowercase();
    span.contains('/')
        || rules::FILE_EXTENSIONS
            .iter()
            .any(|ext| lower.ends_with(ext))
}

fn scan_sentence(
    ctx: &Ctx<'_>,
    is_heading_line: bool,
    limits: &ExtractLimits,
    tables: &mut Tables,
) {
    let sentence_lower = text::lower(ctx.sentence);
    let exec = text::contains_any(&sentence_lower, rules::EXEC);

    // Wiki links: `[[target]]` / `[[target|display]]`. The author pointed at the
    // term on purpose, which is why this rule outranks a capitalized phrase (and
    // why it sits with `heading_entity` — both are deliberate terms).
    for (local, target, display) in wiki_links(ctx.sentence) {
        let chars = target.chars().count();
        if chars < 2 || chars > limits.max_entity_name_chars {
            continue;
        }
        let alias = display.filter(|d| text::norm(d) != text::norm(&target));
        let span_ctx = Ctx {
            sentence: ctx.sentence,
            offset: ctx.offset + local,
            section: ctx.section,
            max_content_bytes: ctx.max_content_bytes,
        };
        let kind = tool_or_concept(&target, exec);
        bump(
            &mut tables.wiki,
            &span_ctx,
            TermSeed {
                key: &target,
                name: &target,
                non_initial: false,
                kind: Some(kind),
                alias,
            },
        );
    }

    // Inline-code spans.
    for (local, span) in code_spans(ctx.sentence) {
        let chars = span.chars().count();
        if chars < 3 || chars > limits.max_entity_name_chars {
            continue;
        }
        if !is_ident_shape(&span) {
            continue;
        }
        let lower = span.to_lowercase();
        if rules::STOP_CODE.iter().any(|s| *s == lower) {
            continue;
        }
        let span_ctx = Ctx {
            sentence: ctx.sentence,
            offset: ctx.offset + local,
            section: ctx.section,
            max_content_bytes: ctx.max_content_bytes,
        };
        let kind = tool_or_concept(&span, exec);
        bump(
            &mut tables.code,
            &span_ctx,
            TermSeed {
                key: &span,
                name: &span,
                non_initial: false,
                kind: Some(kind),
                alias: None,
            },
        );
    }

    // Capitalized phrases. Heading lines are skipped: a heading is already a
    // deliberate term, and counting it here would let a title's words leak into
    // the phrase table as if they were a sentence.
    if !is_heading_line {
        for (word_index, phrase) in phrase_windows(ctx.sentence, limits) {
            let key = text::norm(&phrase);
            if key.chars().count() < 2 {
                continue;
            }
            // `word_index == 0` is the sentence's own first word, i.e. sentence
            // capitalization rather than a name.
            bump(
                &mut tables.phrase,
                ctx,
                TermSeed {
                    key: &key,
                    name: &phrase,
                    non_initial: word_index != 0,
                    kind: None,
                    alias: None,
                },
            );
        }
    }

    // Han runs (§9.2: 2..=12 characters; a longer maximal run is a clause with no
    // deterministic term boundary, so it contributes nothing).
    for run in han_runs(ctx.sentence) {
        let chars = run.chars().count();
        if !(2..=rules::MAX_HAN_TERM_CHARS).contains(&chars) {
            continue;
        }
        if rules::STOP_HAN.iter().any(|s| *s == run) {
            continue;
        }
        bump(
            &mut tables.han,
            ctx,
            TermSeed {
                key: &run,
                name: &run,
                non_initial: false,
                kind: None,
                alias: None,
            },
        );
    }
}

/// §9.2's `tool` test, applied to a wiki link's target as well: a slash, a known
/// extension, or a sentence that says the thing is run.
fn tool_or_concept(spelling: &str, exec_sentence: bool) -> &'static str {
    if is_tool_span(spelling) || exec_sentence {
        "tool"
    } else {
        "concept"
    }
}

/// `[[target]]` / `[[target|display]]` spans, with their byte offset in the
/// sentence. A `#anchor` is not part of the page name.
fn wiki_links(sentence: &str) -> Vec<(usize, String, Option<String>)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(open) = sentence[cursor..].find("[[") {
        let start = cursor + open;
        let body_start = start + 2;
        let Some(close) = sentence[body_start..].find("]]") else {
            break;
        };
        let body = &sentence[body_start..body_start + close];
        let (target, display) = match body.split_once('|') {
            Some((target, display)) => (target, Some(display.trim())),
            None => (body, None),
        };
        let target = target.split('#').next().unwrap_or_default().trim();
        if !target.is_empty() {
            let display = display.filter(|d| !d.is_empty()).map(str::to_string);
            out.push((body_start, target.to_string(), display));
        }
        cursor = body_start + close + 2;
    }
    out
}

/// Non-overlapping backticked spans, with their byte offset in the sentence.
fn code_spans(sentence: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut base = 0usize;
    let mut rest = sentence;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            break;
        };
        let span = &after[..close];
        if !span.is_empty() && !span.contains(char::is_whitespace) {
            out.push((base + open + 1, span.to_string()));
        }
        base += open + 1 + close + 1;
        rest = &after[close + 1..];
    }
    out
}

/// The greedy longest match of `[A-Z][A-Za-z0-9]*( [A-Z][A-Za-z0-9]*){0,3}`
/// (§9.2) at every position of every maximal run of capitalized words, in
/// document order. Returns `(sentence token index, phrase)`.
fn phrase_windows(sentence: &str, limits: &ExtractLimits) -> Vec<(usize, String)> {
    let tokens = tokens(sentence);
    let cleaned: Vec<Option<&str>> = tokens.iter().map(|(_, t)| clean_word(t)).collect();

    let mut out = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        match cleaned[i] {
            Some(word) if is_capitalized(word) => {}
            _ => {
                i += 1;
                continue;
            }
        }
        // The run of capitalized words starting here.
        let mut end = i + 1;
        while end < tokens.len() {
            match cleaned[end] {
                Some(w) if is_capitalized(w) => end += 1,
                _ => break,
            }
        }
        let mut start = i;
        while start < end {
            let take = (end - start).min(rules::MAX_PHRASE_WORDS);
            let mut phrase = String::new();
            let mut acceptable = true;
            for word in cleaned[start..start + take]
                .iter()
                .map(|w| w.unwrap_or_default())
            {
                if rules::STOP_WORDS.iter().any(|s| *s == word.to_lowercase()) {
                    acceptable = false;
                    break;
                }
                if !phrase.is_empty() {
                    phrase.push(' ');
                }
                phrase.push_str(word);
            }
            if acceptable && phrase.chars().count() <= limits.max_entity_name_chars {
                out.push((start, phrase));
            }
            start += 1;
        }
        i = end;
    }
    out
}

/// Whitespace-separated tokens with their byte offsets.
fn tokens(sentence: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for token in sentence.split_whitespace() {
        let offset = sentence[cursor..]
            .find(token)
            .map(|i| cursor + i)
            .unwrap_or(cursor);
        cursor = offset + token.len();
        out.push((offset, token));
    }
    out
}

/// Strip surrounding punctuation/markdown from a word (`Rust,` -> `Rust`).
fn clean_word(token: &str) -> Option<&str> {
    let start = token
        .find(|c: char| c.is_alphanumeric())
        .unwrap_or(token.len());
    let end = token
        .rfind(|c: char| c.is_alphanumeric())
        .map(|i| i + token[i..].chars().next().map_or(0, char::len_utf8))
        .unwrap_or(start);
    let word = &token[start..end.max(start)];
    if word.is_empty() { None } else { Some(word) }
}

fn is_capitalized(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    word.chars().count() >= 2 && chars.all(|c| c.is_ascii_alphanumeric())
}

/// Maximal Han runs (design §9.2).
fn han_runs(sentence: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    for c in sentence.chars() {
        if text::is_han(c) {
            run.push(c);
        } else if !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        out.push(run);
    }
    out
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

struct RawEntity {
    name: String,
    kind: Option<&'static str>,
    summary: Option<String>,
    aliases: Vec<String>,
    score: f32,
    rule: &'static str,
    first: usize,
    section: usize,
}

impl RawEntity {
    fn dedup_key(&self) -> String {
        text::base_name(&self.name)
    }
}

/// The identity priority of an entity rule (lower is stronger). A rule that is
/// not in the table sorts last, which cannot happen for a rule this crate emits.
fn rule_priority(rule: &str) -> usize {
    rules::ENTITY_RULE_PRIORITY
        .iter()
        .position(|r| *r == rule)
        .unwrap_or(rules::ENTITY_RULE_PRIORITY.len())
}

// ---------------------------------------------------------------------------
// Relations
// ---------------------------------------------------------------------------

/// Longest-needle-first endpoint lookup over the emitted names and aliases.
struct EndpointIndex {
    needles: Vec<(String, String)>,
}

impl EndpointIndex {
    fn build(entities: &[EntityCandidate]) -> Self {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut needles: Vec<(String, String)> = Vec::new();
        for entity in entities {
            for spelling in std::iter::once(&entity.name).chain(entity.aliases.iter()) {
                if spelling.is_empty() || !seen.insert(spelling.clone()) {
                    continue;
                }
                needles.push((spelling.clone(), entity.name.clone()));
            }
        }
        // Longest first, so the most specific spelling wins; the spelling and the
        // canonical name break ties so equal lengths cannot reorder between runs.
        needles.sort_by(|a, b| {
            b.0.len()
                .cmp(&a.0.len())
                .then(a.0.cmp(&b.0))
                .then(a.1.cmp(&b.1))
        });
        EndpointIndex { needles }
    }

    fn left(&self, before: &str) -> Option<String> {
        let trimmed = text::trim_end_boundary(before);
        self.needles
            .iter()
            .find(|(needle, _)| text::ends_with_ci(trimmed, needle))
            .map(|(_, canonical)| canonical.clone())
    }

    fn right(&self, after: &str) -> Option<String> {
        let trimmed = text::trim_start_boundary(after);
        self.needles
            .iter()
            .find(|(needle, _)| text::starts_with_ci(trimmed, needle))
            .map(|(_, canonical)| canonical.clone())
    }
}

struct RelAccum {
    src: String,
    dst: String,
    relation: &'static str,
    rule: &'static str,
    min_support: usize,
    fact: String,
    valid_at: Option<String>,
    first: usize,
    section: usize,
    /// One entry per distinct sentence stating this tuple (support, §9.4).
    offsets: BTreeSet<usize>,
}

fn scan_relations(
    sentences: &[Sentence],
    endpoints: &EndpointIndex,
    limits: &ExtractLimits,
    source: &str,
) -> Vec<RelationCandidate> {
    let mut accum: BTreeMap<(String, &'static str, String), RelAccum> = BTreeMap::new();

    for sentence in sentences {
        // §9.5: a longer fact is dropped, not truncated.
        if sentence.text.len() > limits.max_content_bytes {
            continue;
        }
        for pattern in rules::RELATION_PATTERNS {
            for connector in pattern.connectors {
                let connector = *connector;
                let Some(at) = text::find_ci(&sentence.text, connector) else {
                    continue;
                };
                let before = &sentence.text[..at];
                let after = &sentence.text[at + connector.len()..];
                let Some(src) = endpoints.left(before) else {
                    continue;
                };
                let Some(dst) = endpoints.right(after) else {
                    continue;
                };
                if src == dst {
                    continue;
                }
                let key = (text::norm(&src), pattern.relation, text::norm(&dst));
                let entry = accum.entry(key).or_insert_with(|| RelAccum {
                    src,
                    dst,
                    relation: pattern.relation,
                    rule: pattern.rule,
                    min_support: pattern.min_support,
                    fact: sentence.text.clone(),
                    valid_at: text::iso_datetime(&sentence.text),
                    first: sentence.offset,
                    section: sentence.section,
                    offsets: BTreeSet::new(),
                });
                entry.offsets.insert(sentence.offset);
                // One connector per pattern per sentence: a repeated connector in
                // one sentence states one fact, and support counts distinct
                // SENTENCES.
                break;
            }
        }
    }

    let mut kept: Vec<RelAccum> = accum
        .into_values()
        .filter(|r| r.offsets.len() >= r.min_support)
        .collect();

    // §9.5 ordering: score descending, then first mention ascending, then the
    // tuple itself.
    kept.sort_by(|a, b| {
        b.score()
            .total_cmp(&a.score())
            .then(a.first.cmp(&b.first))
            .then(a.src.cmp(&b.src))
            .then(a.relation.cmp(b.relation))
            .then(a.dst.cmp(&b.dst))
    });

    kept.into_iter()
        .map(|r| {
            let score = r.score();
            RelationCandidate {
                src: r.src,
                dst: r.dst,
                relation: RelationName::Rule(r.relation),
                fact: r.fact,
                valid_at: r.valid_at,
                score,
                rule: r.rule,
                origin: CandidateOrigin::Document {
                    section: Some(r.section),
                },
                source: source.to_string(),
            }
        })
        .collect()
}

impl RelAccum {
    /// §9.3: `score = min(1.0, 0.5 x support)`.
    fn score(&self) -> f32 {
        (0.5 * self.offsets.len() as f32).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // The truncation flag (ruagent-close-the-gaps t17). It lives here, next to
    // the cap it describes, rather than in tests/bounded.rs: the value under
    // test is the pass's own drop bookkeeping, and the property is stated over
    // the pass's output, not over a helper.
    //
    // The flag is the DROP bit, not the list length — the same property the
    // memory pass gained in the same increment (`memory.rs` sets its flag where
    // it drops, not from `out.len()`). Before the fix, `truncated` came from
    // `bytes_skipped` alone, so a document that overflowed the 96-candidate cap
    // reported `truncated = false` while dropping everything past 96: through
    // the real route, 300 headings + 300 spans and 120 + 120 both answered
    // `entities=96 candidates=96 truncated=0`.
    //
    // Both directions are pinned, because the fix WIDENS the flag:
    // * the candidate drop must raise it (fails if the flag reverts to
    //   text-cut-only), and
    // * the input-text cut must keep raising it (fails if someone replaces the
    //   text cut with the drop bit and trades one silent truncation for another).
    // -----------------------------------------------------------------------

    #[test]
    fn the_truncation_flag_reports_the_candidate_drop_not_only_the_text_cut() {
        // A probe far BELOW the text window, so `bytes_skipped == 0` and the
        // input cut cannot be the cause of whatever the flag says: 200 ASCII
        // headings, one entity each.
        let document: String = (1..=200).map(|i| format!("## Qm{i:04}\n\n")).collect();
        let open = ExtractLimits {
            max_per_input: 1_000,
            ..ExtractLimits::graph_default()
        };
        assert!(
            document.len() < open.max_text_bytes,
            "the text cut must not be in play for this probe"
        );
        let all = graph_candidates(&document, &open);
        assert_eq!(all.bytes_skipped, 0, "no byte of the input was skipped");
        assert!(!all.truncated, "the whole document fits an open cap");
        let total = all.entities.len();
        assert!(
            total > 96,
            "the probe must exceed the default cap, got {total}"
        );
        assert!(
            all.relations.is_empty(),
            "the probe must state no relation, so the drop bit measures entities only"
        );

        // CAPPED AT 96: candidates past the cap are dropped, so the flag must
        // say so.
        let capped = graph_candidates(&document, &ExtractLimits::graph_default());
        assert_eq!(capped.entities.len(), 96, "the cap is the cap");
        assert_eq!(
            capped.bytes_skipped, 0,
            "this loss is the candidate drop, not the text cut"
        );
        assert!(
            capped.truncated,
            "a dropped candidate must be reported — this is the t17 defect"
        );

        // EXACTLY at the cap, with nothing left to drop, is NOT a cut.
        let exact = ExtractLimits {
            max_per_input: total,
            ..ExtractLimits::graph_default()
        };
        let at_cap = graph_candidates(&document, &exact);
        assert_eq!(at_cap.entities.len(), total, "the cap is filled exactly");
        assert!(
            !at_cap.truncated,
            "filling the cap is not dropping from it: `exactly {total}` must stay distinguishable from `capped at {total}`"
        );

        // One candidate beyond the cap is a cut again.
        let minus_one = ExtractLimits {
            max_per_input: total - 1,
            ..ExtractLimits::graph_default()
        };
        let dropped = graph_candidates(&document, &minus_one);
        assert_eq!(dropped.entities.len(), total - 1);
        assert!(dropped.truncated, "one candidate beyond the cap is a cut");

        // The loss the flag ALREADY covered must survive: a document larger than
        // the text window holding almost no candidates is cut in the INPUT, and
        // nothing is dropped by the cap — so only `bytes_skipped` can raise the
        // flag here.
        let huge = format!("## Only\n\n{}", "x".repeat(400 * 1024));
        let cut = graph_candidates(&huge, &ExtractLimits::graph_default());
        assert!(cut.bytes_skipped > 0, "the window cut real bytes");
        assert!(
            cut.entities.len() < 96,
            "nothing was dropped by the candidate cap here: {}",
            cut.entities.len()
        );
        assert!(
            cut.truncated,
            "the input-text cut must still raise the flag (do not trade one silent truncation for another)"
        );
    }
}
