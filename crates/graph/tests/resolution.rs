//! G6: entity resolution with a NAMED criterion, a human gold set, an alias
//! table, and an explicit queue for what the judge will not decide.
//!
//! The readings are computed against `tests/gold/resolution.json` (24 hand-labeled
//! pairs) and against the frozen live graph, where the judge finds 4 pairs that
//! are one object under two names (the measurement the spec's G6 target is stated
//! against: "冗余 0").

use ruagent_graph as g;

mod fixture;
use fixture::{PathmapDb, json_file};

fn gold_pairs() -> Vec<(String, String, bool, String)> {
    json_file("resolution.json")["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["a"].as_str().unwrap().to_string(),
                p["b"].as_str().unwrap().to_string(),
                p["same"].as_bool().unwrap(),
                p["how"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn judge_scores_against_the_hand_labeled_pairs() {
    let pairs = gold_pairs();
    let mut tp = 0;
    let mut fp = 0;
    let mut fn_ = 0;
    let mut tn = 0;
    let mut abstain = 0;
    let mut declared_only = 0;
    for (a, b, same, how) in &pairs {
        // The judge is asked about ONE candidate pair, exactly as the write path
        // asks it: "is this name the same object as that row?".
        let verdict = g::judge_against(&[(1, b.clone())], a);
        let decided_merge = matches!(verdict, g::MergeVerdict::SameObject(_));
        let pending = matches!(verdict, g::MergeVerdict::Pending(_));
        if pending {
            abstain += 1;
        }
        // `declared` pairs are NOT the judge's job: the link lives in the alias
        // table (tested in seed-resolution.rs). The judge must keep quiet about
        // them instead of guessing -- so they are counted separately.
        if *same && how == "declared" {
            assert!(
                !decided_merge,
                "the judge must not derive {a:?} == {b:?}: nothing in the two names says so"
            );
            declared_only += 1;
            continue;
        }
        match (decided_merge, same) {
            (true, true) => tp += 1,
            (true, false) => fp += 1,
            (false, true) => {
                fn_ += 1;
                println!("   MISSED: {a:?} vs {b:?} -> {verdict:?}");
            }
            (false, false) => tn += 1,
        }
    }
    let precision = tp as f64 / (tp + fp).max(1) as f64;
    let recall = tp as f64 / (tp + fn_).max(1) as f64;
    println!(
        "resolution gold n={} | merge decisions: TP={tp} FP={fp} FN={fn_} TN={tn} | abstain(queue)={abstain} | declared-alias-only={declared_only} | precision={precision:.4} recall={recall:.4}",
        pairs.len()
    );

    // The BEFORE side, run in the same test: the pre-gen2 rule merges on
    // `norm_name` equality only, which is what `upsert_entity` still does.
    let mut old_decided = 0;
    for (a, b, same, _) in &pairs {
        let merged = a.trim().to_lowercase() == b.trim().to_lowercase();
        if merged && *same {
            old_decided += 1;
        }
        if merged && !*same {
            panic!("the old rule would merge {a:?} and {b:?}, which the gold says are different");
        }
    }
    let true_pairs = pairs.iter().filter(|(_, _, s, _)| *s).count();
    println!("BEFORE (norm_name equality): {old_decided}/{true_pairs} true pairs merged");

    assert_eq!(
        fp, 0,
        "no false merges: a wrong merge loses a distinction for ever"
    );
    assert_eq!(fn_, 0, "no missed true pair among the labeled ones");
    assert!(
        precision >= 0.85 && recall >= 0.70,
        "G6: judge precision {precision:.4} / recall {recall:.4} against the gold"
    );
    assert_eq!(
        old_decided, 0,
        "the old rule cannot see a single one of these pairs, which is why the 4 live pairs existed"
    );
    assert!(abstain > 0, "the dsh family is queued, not guessed");
    assert!(
        declared_only > 0,
        "one true pair is only reachable through a declared alias"
    );
}

#[tokio::test]
async fn the_live_redundancy_goes_to_zero_when_the_pairs_are_merged() {
    let db = PathmapDb::live_shaped().await;
    let before = g::redundant_pairs(&db.db).await.unwrap();
    println!("BEFORE: the judge finds {} redundant pairs", before.len());
    for (a, b, why) in &before {
        println!("   {a} / {b} :: {why}");
    }
    assert!(
        before.len() >= 3,
        "the frozen live graph is the measurement the spec quotes (4 pairs)"
    );

    // Merging is a DECISION, so the caller makes it here pair by pair.
    let mut merged = 0;
    for _ in 0..20 {
        let pairs = g::redundant_pairs(&db.db).await.unwrap();
        let Some((a, b, _)) = pairs.into_iter().next() else {
            break;
        };
        g::merge_entities(&db.db, b, a).await.unwrap();
        merged += 1;
    }
    let after = g::redundant_pairs(&db.db).await.unwrap();
    let edges = db
        .db
        .call_flat(|conn| {
            conn.query_row("SELECT COUNT(*) FROM entity_edges", [], |r| {
                r.get::<_, i64>(0)
            })
        })
        .await
        .unwrap();
    println!(
        "AFTER: merged {merged} pairs, redundant pairs = {}, edges still {edges}",
        after.len()
    );
    assert_eq!(after.len(), 0, "G6 target: 0 redundant pairs");
    assert_eq!(edges, 67, "a merge moves edges, it never drops them");
    // The absorbed names survive as aliases, so a query for either name still
    // reaches the one surviving object.
    let aliases = db
        .db
        .call_flat(|conn| {
            conn.query_row("SELECT COUNT(*) FROM entity_aliases", [], |r| {
                r.get::<_, i64>(0)
            })
        })
        .await
        .unwrap();
    println!("aliases written by the merges: {aliases}");
    assert!(aliases >= merged);
    let seeds = g::resolve_seeds(&db.db, "Agent Client Protocol", 32)
        .await
        .unwrap();
    assert!(
        !seeds.is_empty(),
        "the one surviving object answers to the absorbed name too"
    );
}

#[tokio::test]
async fn upsert_with_aliases_merges_or_queues_never_guesses() {
    let db = PathmapDb::live_shaped().await;
    // An exact-parenthesis variant of an existing row: MERGED, not duplicated.
    let out = g::upsert_entity_with_aliases(
        &db.db,
        "DeepSeek Harness",
        Some("product"),
        None,
        &["dsh".to_string()],
        "extraction",
    )
    .await
    .unwrap();
    match out {
        g::ResolveOutcome::Merged { id, aliases_added } => {
            println!("merged into #{id}, aliases added {aliases_added}");
        }
        other => panic!("expected a merge, got {other:?}"),
    }
    let rows = db
        .db
        .call_flat(|conn| {
            conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get::<_, i64>(0))
        })
        .await
        .unwrap();
    assert_eq!(
        rows, 63,
        "no new row was created for a name that already exists"
    );

    // The dsh family: the judge abstains and the pair is QUEUED.
    let out = g::upsert_entity_with_aliases(
        &db.db,
        "dsh-graph-view",
        Some("tool"),
        Some("a viewer built on dsh"),
        &[],
        "extraction",
    )
    .await
    .unwrap();
    match out {
        g::ResolveOutcome::PendingReview { id, other } => {
            println!("queued: #{id} vs #{other}");
            assert_ne!(id, other);
        }
        other => panic!("a first-token-only match must be queued, got {other:?}"),
    }
    let pending = g::pending_pairs(&db.db).await.unwrap();
    println!("pending queue: {pending:?}");
    assert_eq!(
        pending.len(),
        1,
        "the undecided pair is recorded, not dropped"
    );
    assert!(pending[0].2.contains("first-token"));

    // A genuinely new object is simply created.
    let out =
        g::upsert_entity_with_aliases(&db.db, "Zephyr", Some("tool"), None, &[], "extraction")
            .await
            .unwrap();
    assert!(matches!(out, g::ResolveOutcome::Created { .. }));
}
