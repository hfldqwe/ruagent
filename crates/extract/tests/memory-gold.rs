//! Golden test for Algorithm A (design §8.6): a fixed transcript pins the
//! candidate list, rule by rule, so a later algorithm change shows up as a diff
//! instead of as a silent behaviour change.
//!
//! The fixture is compiled in with `include_str!` — the test performs no
//! filesystem I/O at run time, so the purity the crate claims holds in its tests
//! too.

use ruagent_extract::{
    CandidateOrigin, ExtractLimits, MemoryCandidate, Role, Turn, memory_candidates_with, text,
};

/// `rule|store|confidence|content` — one line per candidate, so a failure prints
/// a readable diff rather than two struct dumps.
fn render(candidate: &MemoryCandidate) -> String {
    format!(
        "{}|{}|{}|{}",
        candidate.rule,
        candidate.store.as_str(),
        candidate.confidence.as_str(),
        candidate.content
    )
}

fn expected(line: &serde_json::Value) -> String {
    format!(
        "{}|{}|{}|{}",
        line["rule"].as_str().unwrap_or("<missing rule>"),
        line["store"].as_str().unwrap_or("<missing store>"),
        line["confidence"]
            .as_str()
            .unwrap_or("<missing confidence>"),
        line["content"].as_str().unwrap_or("<missing content>"),
    )
}

fn turns(case: &serde_json::Value) -> Vec<Turn> {
    case["turns"]
        .as_array()
        .expect("every case has turns")
        .iter()
        .map(|turn| Turn {
            role: match turn["role"].as_str() {
                Some("assistant") => Role::Assistant,
                _ => Role::User,
            },
            text: turn["text"].as_str().unwrap_or_default().to_string(),
            // The free tier never reads a clock, so the fixture does not have to
            // carry one; the daemon always does.
            ts_ms: turn["ts_ms"].as_i64().unwrap_or(0),
        })
        .collect()
}

#[test]
fn memory_gold_fixture_is_pinned() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("gold/memory.json")).expect("the fixture is valid JSON");
    let cases = fixture["cases"].as_array().expect("cases");
    assert!(!cases.is_empty(), "the fixture must have cases");

    for case in cases {
        let name = case["name"].as_str().unwrap_or("<unnamed>");
        let turns = turns(case);
        let got = memory_candidates_with(&turns, &ExtractLimits::default());

        let actual: Vec<String> = got.candidates.iter().map(render).collect();
        let want: Vec<String> = case["expect"]
            .as_array()
            .expect("every case has expect")
            .iter()
            .map(expected)
            .collect();
        assert_eq!(
            actual, want,
            "case `{name}`: candidates differ from the pinned list\nactual: {:#?}",
            got.candidates
        );

        let want_suppressed: Vec<String> = case
            .get("expect_suppressed")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            got.suppressed_rules, want_suppressed,
            "case `{name}`: the suppressed rules differ"
        );
        assert_eq!(
            got.suppressed,
            want_suppressed.len(),
            "case `{name}`: the suppressed counter must match the suppressed rules"
        );

        // The gold cases are far below every cap, so they pin the uncapped shape.
        assert!(!got.truncated, "case `{name}` must not need truncation");
        assert_eq!(got.turns_skipped, 0, "case `{name}` must read every turn");

        // Every candidate is traceable to a turn of THIS transcript, and every
        // one carries the namespace the free tier owns.
        for candidate in &got.candidates {
            match &candidate.origin {
                CandidateOrigin::Turn { index, .. } => {
                    assert!(
                        index < &turns.len(),
                        "case `{name}`: origin {index} out of range"
                    );
                }
                other => {
                    panic!("case `{name}`: a transcript candidate has document origin {other:?}")
                }
            }
            assert_eq!(candidate.namespace, "user");
        }
    }
}

/// §8.7's content token is a real filter, not decoration: it is what separates a
/// body-carrying sentence from a bare acknowledgement.
#[test]
fn the_content_token_guard_is_a_real_filter() {
    assert!(!text::has_content_token("OK"));
    assert!(!text::has_content_token("对。"));
    assert!(text::has_content_token("zip"));
    assert!(text::has_content_token("知识"));
    assert!(text::has_content_token("用 zip 打包"));
    // A question and an over-long body are dropped before any marker is read,
    // even though both would otherwise match `PREF`.
    let question = vec![Turn {
        role: Role::User,
        text: "记住 zip 工具的用法？".to_string(),
        ts_ms: 0,
    }];
    assert!(
        memory_candidates_with(&question, &ExtractLimits::default())
            .candidates
            .is_empty(),
        "a question is not an assertion about the user"
    );
    let limits = ExtractLimits {
        max_content_bytes: 20,
        ..ExtractLimits::default()
    };
    let long = vec![Turn {
        role: Role::User,
        text: format!("记住 {} 这个工具的用法。", "abcdefghij".repeat(4)),
        ts_ms: 0,
    }];
    let got = memory_candidates_with(&long, &limits);
    assert!(
        got.candidates.is_empty(),
        "a body longer than max_content_bytes is dropped, never truncated: {:#?}",
        got.candidates
    );
}

/// The suppression of §8.2 is a one-turn lookback, and it removes candidates
/// rather than the memory of what was removed.
#[test]
fn suppression_looks_back_exactly_one_turn() {
    let turns = vec![
        Turn {
            role: Role::Assistant,
            text: "可能要用 green thread。".to_string(),
            ts_ms: 0,
        },
        Turn {
            role: Role::Assistant,
            text: "可能是 work-stealing。".to_string(),
            ts_ms: 1,
        },
        Turn {
            role: Role::User,
            text: "不对，应该是别的。".to_string(),
            ts_ms: 2,
        },
    ];
    let got = memory_candidates_with(&turns, &ExtractLimits::default());
    let rules: Vec<&str> = got.candidates.iter().map(|c| c.rule).collect();
    assert_eq!(
        rules,
        vec!["hedged_statement", "user_correction"],
        "the turn before the correction is dropped, the older one is kept"
    );
    assert_eq!(got.suppressed_rules, vec!["hedged_statement"]);
    assert_eq!(got.suppressed, 1);
}
