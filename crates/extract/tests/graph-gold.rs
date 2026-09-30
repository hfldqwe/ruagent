//! Golden test for Algorithm B (design §9): a fixed document pins its entities,
//! aliases and relations, so a later algorithm change shows up as a diff.
//!
//! The fixture is compiled in with `include_str!`, so the test performs no
//! filesystem I/O at run time.

use ruagent_extract::{ExtractLimits, GraphCandidates, graph_candidates};

/// `rule|name|kind|score|aliases|summary` — one line per entity.
fn render_entity(entity: &ruagent_extract::EntityCandidate) -> String {
    format!(
        "{}|{}|{}|{:.4}|{}|{}",
        entity.rule,
        entity.name,
        entity.kind.unwrap_or("-"),
        entity.score,
        entity.aliases.join(","),
        entity.summary.as_deref().unwrap_or("-")
    )
}

fn expected_entity(line: &serde_json::Value) -> String {
    let aliases: Vec<String> = line
        .get("aliases")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    format!(
        "{}|{}|{}|{:.4}|{}|{}",
        line["rule"].as_str().unwrap_or("<missing rule>"),
        line["name"].as_str().unwrap_or("<missing name>"),
        line["kind"].as_str().unwrap_or("-"),
        line["score"].as_f64().unwrap_or(-1.0),
        aliases.join(","),
        line["summary"].as_str().unwrap_or("-"),
    )
}

/// `rule|src|relation|dst|score|valid_at|fact` — one line per relation.
fn render_relation(relation: &ruagent_extract::RelationCandidate) -> String {
    format!(
        "{}|{}|{}|{}|{:.4}|{}|{}",
        relation.rule,
        relation.src,
        relation.relation.relation_literal(),
        relation.dst,
        relation.score,
        relation.valid_at.as_deref().unwrap_or("-"),
        relation.fact
    )
}

fn expected_relation(line: &serde_json::Value) -> String {
    format!(
        "{}|{}|{}|{}|{:.4}|{}|{}",
        line["rule"].as_str().unwrap_or("<missing rule>"),
        line["src"].as_str().unwrap_or("<missing src>"),
        line["relation"].as_str().unwrap_or("<missing relation>"),
        line["dst"].as_str().unwrap_or("<missing dst>"),
        line["score"].as_f64().unwrap_or(-1.0),
        line["valid_at"].as_str().unwrap_or("-"),
        line["fact"].as_str().unwrap_or("<missing fact>"),
    )
}

fn check_case(case: &serde_json::Value) -> (String, GraphCandidates) {
    let name = case["name"].as_str().unwrap_or("<unnamed>").to_string();
    let document = case["text"].as_str().expect("every case has text");
    let got = graph_candidates(document, &ExtractLimits::graph_default());

    let actual: Vec<String> = got.entities.iter().map(render_entity).collect();
    let want: Vec<String> = case["expect_entities"]
        .as_array()
        .expect("every case has expect_entities")
        .iter()
        .map(expected_entity)
        .collect();
    assert_eq!(
        actual, want,
        "case `{name}`: entities differ from the pinned list\nactual: {:#?}",
        got.entities
    );

    let actual: Vec<String> = got.relations.iter().map(render_relation).collect();
    let want: Vec<String> = case["expect_relations"]
        .as_array()
        .expect("every case has expect_relations")
        .iter()
        .map(expected_relation)
        .collect();
    assert_eq!(
        actual, want,
        "case `{name}`: relations differ from the pinned list\nactual: {:#?}",
        got.relations
    );

    (name, got)
}

#[test]
fn graph_gold_fixture_is_pinned() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("gold/graph.json")).expect("the fixture is valid JSON");
    let cases = fixture["cases"].as_array().expect("cases");
    assert!(!cases.is_empty(), "the fixture must have cases");

    for case in cases {
        let (name, got) = check_case(case);
        assert!(!got.truncated, "case `{name}` must fit max_text_bytes");
        assert_eq!(got.bytes_skipped, 0, "case `{name}` must not skip bytes");
        // Every relation names entities this pass emitted (§9.4) — that is what
        // lets the graph's write path accept it.
        let names: Vec<&str> = got.entities.iter().map(|e| e.name.as_str()).collect();
        for relation in &got.relations {
            assert!(
                names.contains(&relation.src.as_str()),
                "case `{name}`: relation src {} is not an emitted entity",
                relation.src
            );
            assert!(
                names.contains(&relation.dst.as_str()),
                "case `{name}`: relation dst {} is not an emitted entity",
                relation.dst
            );
        }
    }
}

/// The bounds that keep the entity rules honest are asserted, not asserted-about:
/// a capitalized word used once, or only where a sentence begins, is not an
/// entity; a Han term needs three occurrences.
#[test]
fn the_entity_rules_are_bounded() {
    let document = "\
Use Linters now. Linters help.
Kubernetes is a platform.
Something completely unrelated here.
";
    let got = graph_candidates(document, &ExtractLimits::graph_default());
    let names: Vec<&str> = got.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"Linters"),
        "a phrase seen twice, once away from a sentence start, is an entity: {names:?}"
    );
    assert!(
        !names.contains(&"Kubernetes"),
        "a capitalized word seen once is not an entity: {names:?}"
    );

    // Sentence-initial only: the phrase never appears away from the first token.
    let sentence_initial = "Recall is fast. Recall is simple.\n";
    let got = graph_candidates(sentence_initial, &ExtractLimits::graph_default());
    assert!(
        got.entities.is_empty(),
        "sentence-initial-only capitalization is not a name: {:#?}",
        got.entities
    );

    // A Han term needs document frequency >= 3.
    let han = "知识库 是 图谱。\n知识库 很快。\n";
    let got = graph_candidates(han, &ExtractLimits::graph_default());
    assert!(
        got.entities.is_empty(),
        "a Han run seen twice is below the df >= 3 gate: {:#?}",
        got.entities
    );
    let han_three = "知识库 是 图谱。\n知识库 很快。\n知识库 很稳。\n";
    let got = graph_candidates(han_three, &ExtractLimits::graph_default());
    let names: Vec<&str> = got.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"知识库"),
        "df = 3 passes the gate: {names:?}"
    );
}

/// §9.4's support gate is the bound on relation noise: `is_a` is definitional and
/// fires on one sentence, the other three need a second, independent sentence.
#[test]
fn relation_support_is_enforced() {
    let once = "\
`alpha` is a `beta`.
`alpha` uses `gamma`.
";
    let got = graph_candidates(once, &ExtractLimits::graph_default());
    let relations: Vec<(&str, &str)> = got
        .relations
        .iter()
        .map(|r| (r.relation.relation_literal(), r.src.as_str()))
        .collect();
    assert_eq!(
        relations,
        vec![("is_a", "alpha")],
        "a single `uses` sentence is below the support gate: {:#?}",
        got.relations
    );

    let twice = "\
`alpha` is a `beta`.
`alpha` uses `gamma`.
`alpha` uses `gamma`.
";
    let got = graph_candidates(twice, &ExtractLimits::graph_default());
    let relations: Vec<&str> = got
        .relations
        .iter()
        .map(|r| r.relation.relation_literal())
        .collect();
    assert_eq!(
        relations,
        vec!["uses", "is_a"],
        "two sentences state `uses`"
    );
    let uses = &got.relations[0];
    assert_eq!(
        uses.valid_at, None,
        "no stated time means None, never a clock"
    );
    assert_eq!(uses.score, 1.0, "support 2 -> score 1.0");
}

/// `valid_at` is the literal the sentence states, and nothing else — the free
/// tier has no clock, so "no event time is known" cannot be faked.
#[test]
fn event_time_comes_only_from_the_sentence() {
    let stated = "`alpha` is a `beta` since 2026-09-14T21:39:08+08:00.\n";
    let got = graph_candidates(stated, &ExtractLimits::graph_default());
    assert_eq!(got.relations.len(), 1);
    assert_eq!(
        got.relations[0].valid_at.as_deref(),
        Some("2026-09-14T21:39:08+08:00")
    );

    let unstated = "`alpha` is a `beta` since yesterday.\n";
    let got = graph_candidates(unstated, &ExtractLimits::graph_default());
    assert_eq!(got.relations.len(), 1);
    assert_eq!(got.relations[0].valid_at, None);
}

/// The fifth entity rule: a wiki link is a term the document pointed at, and its
/// display form is an alias candidate the write path can resolve later.
#[test]
fn wiki_links_are_entities_with_their_display_alias() {
    let document = "See [[Recall Plane]] and [[Recall Plane|the plane]] for context.\n";
    let got = graph_candidates(document, &ExtractLimits::graph_default());
    assert_eq!(
        got.entities.len(),
        1,
        "one term, whatever it is written next to: {:#?}",
        got.entities
    );
    let entity = &got.entities[0];
    assert_eq!(entity.name, "Recall Plane");
    assert_eq!(entity.rule, "wiki_link");
    assert_eq!(entity.kind, Some("concept"));
    assert_eq!(entity.aliases, vec!["the plane".to_string()]);
    // df = 2 (two links) -> 2/5, not the heading floor.
    assert!((entity.score - 0.4).abs() < 1e-6, "score {}", entity.score);

    // An anchor is not part of the page name, and the target keeps its spelling.
    let anchored = "See [[ruagent-extract#Determinism]] for the argument.\n";
    let got = graph_candidates(anchored, &ExtractLimits::graph_default());
    assert_eq!(got.entities.len(), 1);
    assert_eq!(got.entities[0].name, "ruagent-extract");
    assert_eq!(got.entities[0].rule, "wiki_link");
    // ... and a link to a path-shaped target is a tool, not a concept.
    let path = "See [[crates/extract]] for the crate.\n";
    let got = graph_candidates(path, &ExtractLimits::graph_default());
    assert_eq!(got.entities[0].kind, Some("tool"));
}

/// Endpoint resolution through the aliases this pass emitted: a relation that
/// spells an entity the way the document's own alias does resolves to the
/// canonical name. Nothing outside this pass is consulted — there is nothing to
/// consult without I/O.
#[test]
fn relation_endpoints_resolve_through_emitted_aliases() {
    let document = "\
`ruagent-extract` uses `serde_json`.
`ruagent-extract` uses serde-json.
";
    let got = graph_candidates(document, &ExtractLimits::graph_default());
    assert_eq!(
        got.relations.len(),
        1,
        "both sentences state the same fact: {:#?}",
        got.relations
    );
    let relation = &got.relations[0];
    assert_eq!(relation.dst, "serde_json", "the alias resolves to the name");
    assert_eq!(relation.relation.relation_literal(), "uses");
    assert_eq!(relation.score, 1.0, "two distinct sentences -> support 2");
    assert_eq!(
        relation.dedup_key(),
        "ruagent-extract\u{1f}uses\u{1f}serde_json"
    );
}

// The dedup keys of §9.5 are the identity the applier merges on, so they are
// pinned rather than described.
#[test]
fn entity_dedup_keys_ignore_case_and_one_parenthetical() {
    let document = "# DeepSeek Harness (dsh)\n\nADaPters are adapters. ADaPters are adapters.\n";
    let got = graph_candidates(document, &ExtractLimits::graph_default());
    let harness = got
        .entities
        .iter()
        .find(|e| e.name.starts_with("DeepSeek"))
        .expect("the heading is an entity");
    assert_eq!(harness.dedup_key(), "deepseek harness");
    assert!(
        harness.aliases.contains(&"dsh".to_string()),
        "the parenthetical is an alias candidate, not part of the name: {:?}",
        harness.aliases
    );
    assert!(
        !harness.aliases.contains(&"dh".to_string()),
        "a two-letter acronym is refused (see rules::MIN_ACRONYM_CHARS)"
    );
}
