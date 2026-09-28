//! The live-corpus gold reading, taken with the SAME instrument on both sides of
//! t7 (R-A C1/C2/C8/C9; §E5's "one instrument, two committed states").
//!
//! WHY IT IS ENV-GATED AND WHY IT COPIES: the acceptance numbers are defined on
//! the live corpus (934 documents / 10765 chunks / real e5-small vectors), and
//! this instrument WRITES — it seeds the frozen gold set and records a run. So
//! it never opens `~/.ruagent`: point `RUAGENT_IA_LIVE_COPY` at a COPIED root
//! (`data/ruagent.db`, `data/lancedb`, `knowledge/`) and, for the real model,
//! `HF_HOME` at the directory that CONTAINS `models--<org>--<name>` (that is
//! what the daemon sets, see `crates/daemon/src/lib.rs:135-138`).
//!
//! WHAT THIS FILE DELIBERATELY DOES NOT USE: `search_page`, `RankedHit`,
//! `FUSION`, `LEG_WINDOW`. Every API here exists BEFORE t7, so `cargo test
//! --test retrieval-gold-copy` runs against the pre-t7 revision unchanged and
//! the before/after pair on the live corpus comes from one instrument rather
//! than from two that happen to compute similar numbers. The post-t7 surface
//! (per-hit scale, relevance, the recorded run) is exercised in
//! `retrieval-gold-live.rs`, which is allowed to require the new code because it
//! is not part of the pair.

mod common;

use ruagent_knowledge::{Embedder, HashEmbedder, Knowledge};
use std::sync::Arc;

const TAG: &str = "t7-gold";

/// t30 / RV-A-5: this test MEASURES, or it does not report green.
///
/// It used to print `NOT MEASURED` and `return` when the env var was absent, so
/// `cargo test -p ruagent-knowledge` counted it as **passed** while it had
/// measured nothing -- "64 passed" mixed measurements with non-attempts, and a
/// reader who saw only the count concluded 64 readings existed. It is now
/// `#[ignore]`d (the harness's own count field then says `ignored`, which cannot
/// be mistaken for a measurement), and running it explicitly without the env var
/// FAILS at the `expect` below instead of skipping quietly.
#[ignore = "measures the C1/C2/C9 live reading: needs RUAGENT_IA_LIVE_COPY (a COPY of a live \
            root -- it WRITES) and runs with `-- --ignored`"]
#[tokio::test]
async fn live_copy_gold_set() {
    let root = std::env::var("RUAGENT_IA_LIVE_COPY").expect(
        "this instrument has no state in which it passes without measuring: set \
         RUAGENT_IA_LIVE_COPY=<a COPY of a live root: data/ruagent.db, data/lancedb, knowledge/> \
         (it seeds gold rows, so never point it at ~/.ruagent) and run with `-- --ignored`",
    );
    let root = std::path::PathBuf::from(root);
    let db_path = root.join("data").join("ruagent.db");
    assert!(
        db_path.exists(),
        "no database under {root:?}: RUAGENT_IA_LIVE_COPY must point at a copied ROOT, not a .db file"
    );

    let db = ruagent_store::Db::open(&db_path).unwrap();
    let (embedder, embedder_note): (Arc<dyn Embedder>, String) =
        match ruagent_knowledge::FastEmbedder::try_new().await {
            Ok(fe) => {
                let note = format!("{} (dim {})", fe.name(), fe.dim());
                (Arc::new(fe), note)
            }
            Err(e) => (
                Arc::new(HashEmbedder::new(384)),
                format!(
                    "ABSENT -- FastEmbedder::try_new failed ({e}); hash-embedder used instead, so \
                     the semantic leg is NOT the live vector space and these numbers are not the \
                     acceptance numbers"
                ),
            ),
        };
    let kb = Knowledge::with_embedder(root.as_path(), db.clone(), embedder)
        .await
        .unwrap();

    let (set_id, written) = common::seed(&db).await;
    println!("[{TAG}] gold set id={set_id} newly written={written} embedder={embedder_note}");

    let report = common::measure(&kb).await;
    common::print_report(TAG, &report);
    // A READING REALLY HAPPENED, asserted rather than assumed: one row per frozen
    // query, on BOTH sides of the set (the answerable ones and the unanswerable
    // ones are separate vectors -- `report.rows` is the 15 that carry a gold
    // document, `report.no_answer` the 7 that must not be answerable). This is the
    // assertion RV-A-5 asked for: a report that came back short (or empty) must
    // not be able to pass on a prior run's numbers.
    assert_eq!(
        report.rows.len(),
        common::GOLD.len(),
        "the instrument must produce one reading per answerable frozen query; a short report is \
         not a measurement"
    );
    assert_eq!(
        report.no_answer.len(),
        common::NO_ANSWER.len(),
        "the unanswerable half of the frozen set must be measured too"
    );

    let rs = common::ranks(&report);
    let (r1, r5, r20, mrr) = common::metrics(&rs);
    let ndcg = common::ndcg_at_10(&rs);

    // The targets are R-A C1/C2/C9. They are checked HERE, on the instrument that
    // produces them, so a regression fails the suite instead of being noticed in
    // a report.
    assert!(
        r20 >= 0.99,
        "candidate recall regressed: recall@20 = {r20} (the live baseline was 1.0000; the pre-t7 \
         failure mode was RANKING, not recall)"
    );
    assert!(
        r1 >= 0.70,
        "C1 not met: recall@1 = {r1} (baseline 0.4667, target 0.70)"
    );
    assert!(mrr >= 0.80, "C2 not met: MRR = {mrr} (baseline 0.6889)");
    assert!(
        ndcg >= 0.83,
        "C9 not met: nDCG@10 = {ndcg} (baseline 0.7682)"
    );
    // C8(a): the instrument must have HEADROOM. The synthetic harness had three
    // metrics at exactly 1.0000, so no regression in it could ever be seen; this
    // slice must not repeat that.
    assert!(
        r1 < 1.0 || r5 < 1.0 || mrr < 1.0,
        "the live slice is saturated: every metric is 1.0, so nothing here can fail"
    );
}
