//! The contract tests: purity, determinism, caps and pathological input.
//!
//! These are the acceptance criteria that a "looks right" implementation can
//! still fail, so they are mechanical: a source scan, byte comparisons across
//! repeated runs, exact counts against the caps, and inputs built to be hostile.

use ruagent_extract::{
    ExtractLimits, Role, Turn, graph_candidates, graph_candidates_for, memory_candidates,
    memory_candidates_for, memory_candidates_with, text,
};

/// Every source file of the crate, compiled in — the scan reads the bytes the
/// compiler read, so it cannot be fooled by a build-time path.
const SOURCES: &[(&str, &str)] = &[
    ("src/lib.rs", include_str!("../src/lib.rs")),
    ("src/rules.rs", include_str!("../src/rules.rs")),
    ("src/text.rs", include_str!("../src/text.rs")),
    ("src/memory.rs", include_str!("../src/memory.rs")),
    ("src/graph.rs", include_str!("../src/graph.rs")),
];

/// Write an empty manifest instead of a dependency table, and a source tree that
/// cannot reach the outside world with a `use`, a path, or an unordered
/// collection. The list is deliberately over-broad: a false positive is a
/// conversation, a false negative is a purity claim that is not true.
///
/// Prose may NAME another crate (this crate's docs cite the graph's own shapes
/// on purpose), so the scan looks for the mechanisms that would actually reach
/// one — `use`/`extern crate` — plus the daemon crate by name, which no comment
/// here has any reason to mention.
const FORBIDDEN: &[&str] = &[
    "std::fs",
    "std::net",
    "std::env",
    "std::process",
    "std::time",
    "std::thread",
    "std::io",
    "HashMap",
    "HashSet",
    "SystemTime",
    "Instant",
    "rand::",
    "tokio",
    "rusqlite",
    "reqwest",
    "extern crate",
    "use ruagent_",
    "ruagent_daemon",
    "unsafe",
];

#[test]
fn the_crate_has_no_dependencies_and_no_io() {
    let manifest = include_str!("../Cargo.toml");
    assert!(
        !manifest.contains("[dependencies]"),
        "the purity contract forbids a dependency table (design §7.2)"
    );
    assert!(
        !manifest.contains("path ="),
        "no path dependencies: the crate must not drag an internal crate in"
    );
    for internal in [
        "ruagent-daemon",
        "ruagent-core",
        "ruagent-store",
        "ruagent-memory",
        "ruagent-graph",
        "ruagent-knowledge",
        "ruagent-policy",
    ] {
        assert!(
            !manifest.contains(internal),
            "the free tier must not depend on {internal}"
        );
    }
    assert!(manifest.contains("serde_json"), "the gold fixtures need it");

    for (path, source) in SOURCES {
        for forbidden in FORBIDDEN {
            assert!(
                !source.contains(forbidden),
                "{path} mentions `{forbidden}`: the crate must stay pure and ordered"
            );
        }
    }
}

fn sample_turns() -> Vec<Turn> {
    vec![
        Turn {
            role: Role::User,
            text: "以后统一用 pnpm 安装依赖。".to_string(),
            ts_ms: 1_700_000_000_000,
        },
        Turn {
            role: Role::Assistant,
            text: "安装步骤很简单。首先要配置 Cargo.toml，然后执行 cargo build。".to_string(),
            ts_ms: 1_700_000_001_000,
        },
        Turn {
            role: Role::User,
            text: "不对，应该是用 cargo 而不是 npm。".to_string(),
            ts_ms: 1_700_000_002_000,
        },
        Turn {
            role: Role::Assistant,
            text: "可能要用 workspaces，不过我不确定。注意 `pnpm -w` 才是根目录的写法。"
                .to_string(),
            ts_ms: 1_700_000_003_000,
        },
    ]
}

fn sample_document() -> String {
    "# ruagent-extract\n\n`ruagent-extract` is a `workspace` member since 2026-09-14.\n\n`ruagent-extract` uses `serde_json` for the fixtures.\n\n`ruagent-extract` uses `serde_json` again in the golden test.\n\n## 知识库\n\n知识库 是 知识图谱 的入口。\n知识库 是 召回 的数据源。\n召回 的速度决定体验。\n召回 也依赖 知识库。\n".to_string()
}

/// Same input => byte-identical output, 32 times over: an unordered collection
/// leaking into the output would show up here as a difference between runs.
#[test]
fn same_input_is_byte_identical_across_runs() {
    let turns = sample_turns();
    let document = sample_document();
    let limits = ExtractLimits::default();
    let graph_limits = ExtractLimits::graph_default();

    let memory_first = format!("{:?}", memory_candidates_with(&turns, &limits));
    let graph_first = format!("{:?}", graph_candidates(&document, &graph_limits));
    assert!(!memory_first.is_empty() && !graph_first.is_empty());

    for run in 1..32 {
        assert_eq!(
            format!("{:?}", memory_candidates_with(&turns, &limits)),
            memory_first,
            "memory run {run} differs"
        );
        assert_eq!(
            format!("{:?}", graph_candidates(&document, &graph_limits)),
            graph_first,
            "graph run {run} differs"
        );
    }

    // The two entry points cannot disagree: the plain one is the other's field.
    assert_eq!(
        memory_candidates(&turns, &limits),
        memory_candidates_with(&turns, &limits).candidates
    );
}

/// Every candidate carries where it came from (the caller's own label, plus its
/// position) and a stable key, and the keys are what the dedup used — so a second
/// run over the same input yields the same candidates under the same keys.
#[test]
fn provenance_and_dedup_keys_are_stable() {
    let turns = sample_turns();
    let limits = ExtractLimits::default();
    let first = memory_candidates_for("sess-42", &turns, &limits);
    let second = memory_candidates_for("sess-42", &turns, &limits);
    assert!(!first.candidates.is_empty());

    let keys: Vec<String> = first.candidates.iter().map(|c| c.dedup_key()).collect();
    let again: Vec<String> = second.candidates.iter().map(|c| c.dedup_key()).collect();
    assert_eq!(keys, again, "the same input yields the same keys");
    assert!(
        keys.iter().all(|k| k.contains('\u{1f}')),
        "a memory key is (rule, norm(content)): {keys:?}"
    );
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "one key per candidate: {keys:?}");
    assert!(first.candidates.iter().all(|c| c.source == "sess-42"));
    // The pinned §7.3 signatures cannot carry a label, and it says so instead of
    // inventing one.
    assert!(
        memory_candidates(&turns, &limits)
            .iter()
            .all(|c| c.source.is_empty())
    );

    let document = sample_document();
    let graph = graph_candidates_for(
        "knowledge/rust.md",
        &document,
        &ExtractLimits::graph_default(),
    );
    assert!(
        graph
            .entities
            .iter()
            .all(|e| e.source == "knowledge/rust.md")
    );
    assert!(
        graph
            .relations
            .iter()
            .all(|r| r.source == "knowledge/rust.md")
    );
    let keys: Vec<String> = graph.entities.iter().map(|e| e.dedup_key()).collect();
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "one key per entity: {keys:?}");
    let keys: Vec<String> = graph.relations.iter().map(|r| r.dedup_key()).collect();
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "one key per relation: {keys:?}");

    // A second run over the same document produces the same key sets.
    let again = graph_candidates_for(
        "knowledge/rust.md",
        &document,
        &ExtractLimits::graph_default(),
    );
    let keys_again: Vec<String> = again.entities.iter().map(|e| e.dedup_key()).collect();
    let keys_first: Vec<String> = graph.entities.iter().map(|e| e.dedup_key()).collect();
    assert_eq!(keys_first, keys_again);
}

/// The cap is a cap: the output stops at `max_per_input`, and the caller is told.
#[test]
fn memory_output_is_capped_and_reported() {
    let turns: Vec<Turn> = (0..200)
        .map(|i| Turn {
            role: Role::User,
            text: format!("以后统一用工具{i}处理这件事。"),
            ts_ms: i,
        })
        .collect();
    let limits = ExtractLimits {
        max_per_input: 5,
        ..ExtractLimits::default()
    };
    let got = memory_candidates_with(&turns, &limits);
    assert_eq!(got.candidates.len(), 5, "the cap is the cap");
    assert!(got.truncated, "a cut list must say so");
    assert_eq!(got.turns_skipped, 0);

    // Windowing: only the LAST max_turns turns are read.
    let limits = ExtractLimits {
        max_turns: 3,
        ..ExtractLimits::default()
    };
    let got = memory_candidates_with(&turns, &limits);
    assert_eq!(got.turns_skipped, 197);
    for candidate in &got.candidates {
        match &candidate.origin {
            ruagent_extract::CandidateOrigin::Turn { index, .. } => assert!(*index >= 197),
            other => panic!("unexpected origin {other:?}"),
        }
    }
}

#[test]
fn graph_output_is_capped_and_scored() {
    let document = sample_document();
    let limits = ExtractLimits {
        max_per_input: 3,
        ..ExtractLimits::graph_default()
    };
    let got = graph_candidates(&document, &limits);
    assert!(
        got.entities.len() + got.relations.len() <= 3,
        "entities + relations share one budget: {}",
        got.entities.len() + got.relations.len()
    );
    assert_eq!(got.entities.len(), 3, "entities are taken first");

    // `min_score` is the ranking cut of §9.3: the df = 1 identifiers (0.25) go,
    // the heading floor (0.5) and the recurring terms stay.
    let limits = ExtractLimits {
        min_score: 0.3,
        ..ExtractLimits::graph_default()
    };
    let got = graph_candidates(&document, &limits);
    for entity in &got.entities {
        assert!(
            entity.score >= 0.3,
            "{} has score {} below min_score",
            entity.name,
            entity.score
        );
    }
    assert!(
        got.entities.iter().all(|e| e.name != "rules"),
        "a df = 1 identifier scores 0.25 and is cut"
    );
}

/// A hostile input must neither explode the output nor panic.
#[test]
fn pathological_inputs_stay_bounded() {
    // One line of a megabyte, no paragraph break anywhere.
    let huge = "a".repeat(1024 * 1024);
    let limits = ExtractLimits::graph_default();
    let got = graph_candidates(&huge, &limits);
    assert!(got.truncated, "the window must be reported as cut");
    assert!(got.bytes_skipped > 0);
    assert!(got.entities.len() <= limits.max_per_input);
    assert!(got.relations.len() <= limits.max_per_input);

    // A megabyte of capitalized words: the phrase table has to stay capped and
    // the sort has to stay total, or this either explodes or takes forever.
    let words = "Alpha Beta Gamma Delta ".repeat(43_000);
    let got = graph_candidates(&words, &limits);
    assert!(got.truncated);
    assert!(got.entities.len() + got.relations.len() <= limits.max_per_input);
    assert!(got.entities.iter().all(|e| !e.name.is_empty()));

    // Deep repetition: dedup must collapse it, not accumulate it.
    let repeated = "以后统一用 pnpm。".repeat(50_000);
    let turns = vec![Turn {
        role: Role::User,
        text: repeated,
        ts_ms: 0,
    }];
    let got = memory_candidates_with(&turns, &ExtractLimits::default());
    assert_eq!(
        got.candidates.len(),
        1,
        "a repeated sentence produces one candidate: {:#?}",
        got.candidates
    );

    let repeated_doc = "知识库 是 召回 的数据源。\n".repeat(50_000);
    let got = graph_candidates(&repeated_doc, &limits);
    assert!(
        got.entities.len() <= limits.max_per_input && got.relations.len() <= limits.max_per_input,
        "repetition must not grow the output"
    );
    assert!(
        got.entities.iter().any(|e| e.name == "知识库"),
        "the repeated term is still the strongest candidate"
    );

    // Degenerate limits must be handled, not panicked on.
    let zero = ExtractLimits {
        max_per_input: 0,
        max_turns: 0,
        max_text_bytes: 0,
        max_content_bytes: 0,
        max_entity_name_chars: 0,
        min_score: 1.0,
    };
    let got = graph_candidates(&sample_document(), &zero);
    assert!(got.entities.is_empty() && got.relations.is_empty());
    let got = memory_candidates_with(&sample_turns(), &zero);
    assert!(got.candidates.is_empty());
    assert_eq!(got.turns_skipped, sample_turns().len());
}

/// The guards that keep a deterministic rule honest are visible from outside:
/// `norm`, the question filter and the content token.
#[test]
fn the_shared_mechanics_are_deterministic() {
    assert_eq!(text::norm("  Hello   World. "), "hello world");
    assert_eq!(text::norm("  知识库。 "), "知识库");
    assert!(text::is_question("是这样吗？"));
    assert!(!text::is_question("就是这样。"));
    // `variants_of` and `acronym` mirror the graph's own shapes
    // (graph/src/lib.rs:909-944) — the extractor hands the write path spellings
    // it already understands.
    assert_eq!(
        text::variants_of("DeepSeek Harness (dsh)"),
        vec!["deepseek harness", "deepseek harness (dsh)", "dsh"]
    );
    assert_eq!(
        text::acronym("Agent Client Protocol").as_deref(),
        Some("ACP")
    );
    // A two-letter acronym is refused: it is a false-merge generator on the
    // graph's own judge (see rules::MIN_ACRONYM_CHARS).
    assert_eq!(text::acronym_alias("DeepSeek Harness"), None);
    assert_eq!(
        text::acronym_alias("Agent Client Protocol").as_deref(),
        Some("ACP")
    );
}
