//! The property the full-width-parenthetical panic violated (ruagent-close-the-gaps
//! t15): NO WELL-FORMED NAME may panic entity resolution.
//!
//! Why a table of shapes rather than the one string that crashed: the same
//! one-byte-per-parenthesis assumption was found twice by two hand sweeps in one
//! increment (crates/extract/src/text.rs:190 and crates/graph/src/lib.rs:916).
//! A test that pins `特质（trait）` would go green while the next shape — a name that
//! is nothing but delimiters, a combining mark after the opener, an emoji body —
//! stayed broken. So the table sweeps the SHAPES: ASCII, full-width, both mixed
//! widths, unmatched delimiters, delimiters-only names, nested groups, combining
//! marks and emoji in every position.
//!
//! The names are run through the whole resolution path the write side uses:
//! `variants`, `base_name`, `judge_against`, and a real `apply_extraction` into a
//! database — because the crash was in the write path, not in one helper.

use ruagent_graph as g;
use ruagent_store::Db;

/// Well-formed names, built from shapes. Bounded and deterministic: 4
/// delimiter combinations × 9 bodies × 5 outer names, plus the explicit list.
fn shape_table() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for opener in ['(', '（'] {
        for closer in [')', '）'] {
            for body in [
                "",
                "x",
                "trait",
                "特质",
                "e\u{0301}",
                "😀",
                "a b",
                "`code`",
                "x（y）",
            ] {
                for outer in ["", "Term", "特质", "字 词", "😀"] {
                    v.push(format!("{outer}{opener}{body}{closer}"));
                }
            }
        }
    }
    // Explicit shapes the generated table cannot state.
    for extra in [
        "特质（trait）",               // the shape measured in the corpus
        "栈帧（jvm stacks 虚拟机栈）", // the second one the real ingest hit
        "基础知识（准备）",
        "（",
        "）",
        "(",
        ")",
        "（（））",
        "(()))",
        "((((",
        "))))",
        "（）",
        "()",
        "（)",
        "(）",
        "（）（）",
        "()()",
        "（x（y））",
        "a(b(c)d)e",
        "e\u{0301}（x）",
        "（e\u{0301}）",
        "a\u{0301}b(c\u{0301})",
        "\u{0301}（x）",
        "（\u{0301}）",
        "😀（x）",
        "（😀）",
        "x(😀)",
        "😀(😀)😀",
        "👨‍👩‍👧‍👦（family）",
        "（👨‍👩‍👧‍👦）",
        "DeepSeek Harness (dsh)",
        "Agent Client Protocol (ACP)",
        "x",
        "字",
    ] {
        v.push(extra.to_string());
    }
    v
}

/// The property: no well-formed name panics any step of the resolution path.
#[tokio::test]
async fn no_well_formed_name_panics_the_resolution_path() {
    let db = Db::open_in_memory().unwrap();
    let other = "other object".to_string();
    for name in shape_table() {
        // 1. the shape helpers
        let variants = g::variants(&name);
        assert!(
            !variants.is_empty(),
            "variants() must never be empty for {name:?}"
        );
        let _ = g::base_name(&name);
        // 2. the judge
        let _ = g::judge_against(&[(1, other.clone())], &name);
        let _ = g::judge_against(&[(1, name.clone())], &other);
        // 3. THE WRITE PATH — what a real ingest calls, and where it crashed.
        let entities = vec![g::ExtractEntity {
            name: name.clone(),
            kind: None,
            summary: None,
            aliases: Vec::new(),
        }];
        let facts = vec![g::ExtractFact {
            src: name.clone(),
            dst: name.clone(),
            relation: "is_a".to_string(),
            fact_text: format!("{name} is_a {name}"),
            valid_at: None,
            event_time_source: g::EventTimeSource::Recorded,
        }];
        let out = g::apply_extraction(&db, &entities, &facts, "t15-property", None)
            .await
            .unwrap_or_else(|e| panic!("apply_extraction refused {name:?}: {e}"));
        if !name.trim().is_empty() {
            assert!(
                out.entities <= 1 || name.trim().is_empty(),
                "one name writes at most one entity: {name:?} -> {out:?}"
            );
        }
    }
}

/// The mixed-width rule must be the SAME rule the extractor chose (pair by
/// POSITION), or the two crates would disagree about what `（x)` means: the
/// extractor hands the write path `("术语", "x")` for `术语（x)` and the write path
/// must see one group there too.
#[test]
fn mixed_width_delimiters_pair_by_position_like_the_extractor() {
    assert_eq!(
        g::variants("矩阵（matrix)"),
        vec!["矩阵", "矩阵（matrix)", "matrix"],
        "full-width open, ASCII close: one group"
    );
    assert_eq!(
        g::variants("Matrix (矩阵）"),
        vec!["matrix", "matrix (矩阵）", "矩阵"],
        "ASCII open, full-width close: one group"
    );
    assert_eq!(g::base_name("矩阵（matrix)"), "矩阵");
    assert_eq!(g::base_name("Matrix (矩阵）"), "matrix");
    // The one-byte pair keeps the frozen behaviour the gold tests already pin.
    assert_eq!(
        g::variants("DeepSeek Harness (dsh)"),
        vec!["deepseek harness", "deepseek harness (dsh)", "dsh"]
    );
    // Unmatched delimiters are "no group", and a group with an empty outer still
    // contributes its inner name.
    assert_eq!(g::variants("没有闭括号（"), vec!["没有闭括号（"]);
    assert_eq!(g::variants("（）"), vec!["（）"]);
    assert_eq!(
        g::variants("outer（inner）"),
        vec!["outer", "outer（inner）", "inner"]
    );
}

/// The corpus shape, pinned as itself: it resolves to its outer term, its inner
/// term is the alias the graph links through, and the two spellings MERGE — which
/// is the whole reason `variants` exists.
#[tokio::test]
async fn the_corpus_shape_resolves_and_merges() {
    assert_eq!(g::base_name("特质（trait）"), "特质");
    assert_eq!(
        g::variants("特质（trait）"),
        vec!["特质", "特质（trait）", "trait"]
    );
    assert!(matches!(
        g::judge_against(&[(1, "trait".to_string())], "特质（trait）"),
        g::MergeVerdict::SameObject(1)
    ));

    // Through the REAL resolving upsert (not `upsert_entity`, which resolves on
    // exact norm_name only): the second spelling must MERGE, exactly as the ASCII
    // `DeepSeek Harness (dsh)` / `dsh` pair does.
    let db = Db::open_in_memory().unwrap();
    let first =
        g::upsert_entity_with_aliases(&db, "特质（trait）", Some("concept"), None, &[], "t15")
            .await
            .unwrap();
    assert!(
        matches!(first, g::ResolveOutcome::Created { .. }),
        "the first spelling creates the object: {first:?}"
    );
    let second = g::upsert_entity_with_aliases(&db, "trait", Some("concept"), None, &[], "t15")
        .await
        .unwrap();
    assert!(
        matches!(second, g::ResolveOutcome::Merged { .. }),
        "the inner term of the full-width group is the same object: {second:?}"
    );
    assert_eq!(ids(&first), ids(&second), "one object, not two");

    // The ASCII shape the parenthetical rule was built for, as the control.
    let ascii_a =
        g::upsert_entity_with_aliases(&db, "DeepSeek Harness (dsh)", None, None, &[], "t15")
            .await
            .unwrap();
    let ascii_b = g::upsert_entity_with_aliases(&db, "dsh", None, None, &[], "t15")
        .await
        .unwrap();
    assert!(matches!(ascii_a, g::ResolveOutcome::Created { .. }));
    assert!(matches!(ascii_b, g::ResolveOutcome::Merged { .. }));
    assert_eq!(ids(&ascii_a), ids(&ascii_b));
}

/// The entity id a resolve outcome names, whichever variant it is.
fn ids(outcome: &g::ResolveOutcome) -> i64 {
    match outcome {
        g::ResolveOutcome::Created { id } | g::ResolveOutcome::Merged { id, .. } => *id,
        g::ResolveOutcome::PendingReview { id, .. } => *id,
    }
}
