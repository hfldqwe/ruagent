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
        max_input_bytes: 0,
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

/// t132, closing the t125 F1 finding: the transcript window is bounded in BYTES,
/// not only in turns — and what it cuts is REPORTED.
///
/// Why this test exists, in one sentence: `pathological_inputs_stay_bounded`
/// above only asserted that a 1.1 MB single turn yields ONE candidate, so nothing
/// in this file would have gone red while a 16 MB turn made the pass do 16 MB of
/// work and allocate a 16 MB `char_indices` table (measured: 137 → 513 → 1943 ms
/// for 1/4/16 MB, peak allocation = 16x the input).
///
/// THREE things make the assertion able to go red, and the third is the one that
/// cannot be satisfied by accident:
///
/// 1. the cut is reported (`truncated` true, `bytes_skipped` exact arithmetic),
/// 2. a sentence placed BEYOND the budget is never extracted,
/// 3. the same prefix in a 16x larger turn yields the SAME candidate list — the
///    pass's output may not depend on the input past its own budget.
///
/// The negative control (run outside the shared tree, `byte_window` neutralised)
/// turns 1, 2 and 3 red: see docs/design/reviews/gen4-extract-bounds-repair.md.
#[test]
fn an_over_long_transcript_is_bounded_by_bytes_and_the_loss_is_reported() {
    // Sentences of a FIXED byte length, so the budget lands exactly on a sentence
    // boundary and the skipped-byte count is exact arithmetic rather than a
    // range.
    let sentence = |i: usize| format!("以后统一用 填充{i:04} 处理这件事。");
    let unit = sentence(0).len();
    let kept_sentences = 100usize;
    let limit = unit * kept_sentences;
    let marker = "以后统一用 独特标记ZZZ 处理这件事。";
    let make = |total: usize| -> String {
        let mut s = String::with_capacity(total);
        let mut i = 0usize;
        while s.len() + unit <= total - marker.len() {
            s.push_str(&sentence(i));
            i += 1;
        }
        while s.len() + marker.len() < total {
            s.push(' ');
        }
        s.push_str(marker);
        assert_eq!(s.len(), total, "the fixture's length is exact");
        s
    };

    let limits = ExtractLimits {
        max_input_bytes: limit,
        // The byte budget must be the ONLY bound in play here, or the cap could
        // take the credit for the loss.
        max_per_input: 4096,
        ..ExtractLimits::default()
    };

    let one = vec![Turn {
        role: Role::User,
        text: make(limit * 16),
        ts_ms: 0,
    }];
    let got = memory_candidates_with(&one, &limits);
    let contents: Vec<&str> = got.candidates.iter().map(|c| c.content.as_str()).collect();

    // 1. the cut is reported, exactly.
    assert!(
        got.truncated,
        "an input cut to max_input_bytes must be reported as a loss: {got:?}"
    );
    assert_eq!(
        got.bytes_skipped,
        one[0].text.len() - limit,
        "the skipped byte count must be the exact number of bytes not read"
    );
    assert_eq!(got.turns_skipped, 0, "max_turns cut nothing here");

    // 2. nothing beyond the budget was read, and the head still was.
    assert!(
        !contents.iter().any(|c| c.contains("独特标记ZZZ")),
        "a sentence past the byte budget must not be extracted: {contents:?}"
    );
    assert!(
        contents.iter().any(|c| c.contains("填充0000")),
        "the head of the turn is inside the budget and must still be read: {contents:?}"
    );

    // 3. scale invariance: 16x the input, byte-identical output.
    let big = vec![Turn {
        role: Role::User,
        text: make(limit * 256),
        ts_ms: 0,
    }];
    let got_big = memory_candidates_with(&big, &limits);
    assert_eq!(
        format!("{:?}", got_big.candidates),
        format!("{:?}", got.candidates),
        "the candidate list must not depend on the input size beyond the budget"
    );
    assert!(got_big.truncated, "the larger turn is cut too");
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

/// THE PROPERTY THE PANIC VIOLATED (the full-width-parenthetical panic,
/// ruagent-close-the-gaps t8). Every string below is well-formed text,
/// and no well-formed text may panic this crate — so the table is run through the
/// PUBLIC entry points, not only through the function that was fixed.
///
/// What it sweeps: each opener/closer combination of `(`/`）` in each arrangement
/// (the corpus shape was full-width open + full-width close; the mixed pairs were
/// never seen in the corpus but are one keystroke away), with a multi-byte body
/// and a multi-byte outer name, at a document heading and as plain prose; plus
/// timestamps whose seconds or offset are followed by multi-byte text, wiki
/// links and code spans next to multi-byte characters.
///
/// Bounded and honest about it: this is a hand-built table of the positions where
/// a byte index can land inside a character, NOT a fuzz corpus over arbitrary
/// text. It pins the shapes this task found and the neighbours of them.
#[test]
fn no_well_formed_multibyte_text_panics_the_extractor() {
    let mut inputs: Vec<String> = Vec::new();
    for opener in ['(', '（'] {
        for closer in [')', '）'] {
            for body in ["", "x", "中", "😀", "a b", "`code`"] {
                for outer in ["术语", "Term", "字 词"] {
                    inputs.push(format!("{outer}{opener}{body}{closer}"));
                    inputs.push(format!("# {outer}{opener}{body}{closer}\n\n正文。\n"));
                    inputs.push(format!("[[{outer}{opener}{body}{closer}]] 是 概念。\n"));
                }
            }
        }
    }
    inputs.extend(
        [
            // The seconds and the offset advanced by a byte count.
            "2026-09-14T21:39:中文",
            "2026-09-14T21:39:08中文",
            "2026-09-14T21:39+中文字",
            "2026-09-14T21:39+08:00",
            "2026-09-14T21:39:08+08:00",
            "2026-09-14T21:39:08.123+08:00",
            "2026-09-14T21:39:08Z",
            "2026-09-14T21:39:中",
            "2026-09-14T21:39:😀",
            "2026-09-14T21:39+😀😀",
            "截至 2026-09-14T21:39:08+08:00 有效。",
            // The same characters next to the other scanners.
            "`代码`（注释） 是 一种 术语。",
            "术语（注释） 使用 `代码`。",
            "字 是 字。字 是 字。",
            "（前括号）和（后括号）",
            "😀（emoji）",
            "（😀）",
        ]
        .iter()
        .map(|s| s.to_string()),
    );

    for input in &inputs {
        let _ = graph_candidates(input, &ExtractLimits::graph_default());
        let turns = vec![
            Turn {
                role: Role::User,
                text: input.clone(),
                ts_ms: 0,
            },
            Turn {
                role: Role::Assistant,
                text: input.clone(),
                ts_ms: 0,
            },
        ];
        let _ = memory_candidates_with(&turns, &ExtractLimits::default());
        // The derived identity shapes, which is where the panic surfaced.
        let _ = text::base_name(input);
        let _ = text::variants_of(input);
        let _ = text::iso_datetime(input);
    }
}
