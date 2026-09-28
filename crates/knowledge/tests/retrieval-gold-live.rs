//! The post-t7 half of the live-corpus reading: per-hit scales, relevance, and
//! the recorded run (R-A E2/E3, C6's library half).
//!
//! The NUMBERS (C1/C2/C9) are produced and asserted by `retrieval-gold-copy.rs`,
//! whose judge only uses APIs that exist before t7 — that is what makes the
//! before/after pair one instrument. This file uses the new surface
//! (`search_page`, `RankedHit`, `ScoreKind`, `FUSION`, `LEG_WINDOW`), so it is
//! expected to fail to COMPILE against the pre-t7 revision; it is not part of
//! the pair and it asserts nothing about quality. What it does assert is the
//! SHAPE: every score carries its scale, a missing leg is `None` and not `0.0`,
//! the fusion's parameters travel with the reading, and the run is recorded with
//! its provenance.
//!
//! Same env gate and same copy rule as retrieval-gold-copy.rs (see its header).

mod common;

use ruagent_knowledge::{Embedder, HashEmbedder, Knowledge, ScoreKind};
use std::sync::Arc;

const TAG: &str = "t7-evidence";

/// t30 / RV-A-5: this test MEASURES, or it does not report green.
///
/// It used to print `NOT MEASURED` and `return` without the env var, so the
/// aggregate `N passed` counted it as a measurement. Now it is `#[ignore]`d (the
/// harness says `ignored`, which no reader can mistake for a reading) and running
/// it without the env var FAILS loudly instead of skipping.
#[ignore = "measures the per-hit evidence reading: needs RUAGENT_IA_LIVE_COPY (a COPY of a live \
            root -- it WRITES) and runs with `-- --ignored`"]
#[tokio::test]
async fn live_copy_evidence_and_run_record() {
    let root = std::env::var("RUAGENT_IA_LIVE_COPY").expect(
        "this instrument has no state in which it passes without measuring: set \
         RUAGENT_IA_LIVE_COPY=<a COPY of a live root> (it records a query_eval_runs row, so never \
         point it at ~/.ruagent) and run with `-- --ignored`",
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
                format!("ABSENT -- FastEmbedder::try_new failed ({e}); hash-embedder used instead"),
            ),
        };
    let kb = Knowledge::with_embedder(root.as_path(), db.clone(), embedder)
        .await
        .unwrap();
    let (set_id, written) = common::seed(&db).await;
    println!(
        "[{TAG}] gold set id={set_id} newly written={written} embedder={embedder_note}\n\
         [{TAG}] fusion={} leg_window={} scoring_version={} relevance_version={}",
        ruagent_knowledge::FUSION.label(),
        ruagent_knowledge::LEG_WINDOW,
        ruagent_knowledge::SCORING_VERSION,
        ruagent_knowledge::RELEVANCE_VERSION
    );

    let mut kinds_seen: Vec<&'static str> = Vec::new();
    let mut missing_leg_is_none = 0usize;
    let mut missing_leg_was_zero = 0usize;
    let mut relevance_values: Vec<f32> = Vec::new();
    let mut ranks: Vec<Option<usize>> = Vec::new();
    let mut queries_measured = 0usize;
    for (query, gold, class) in common::GOLD {
        queries_measured += 1;
        let page = kb.search_page(query, 20).await.unwrap();
        assert_eq!(
            page.evidence.leg_window,
            ruagent_knowledge::LEG_WINDOW,
            "the evidence must name the window it was computed with"
        );
        assert_eq!(page.evidence.fusion, ruagent_knowledge::FUSION);
        for hit in &page.hits {
            kinds_seen.push(hit.score_kind.as_str());
            if let Some(s) = hit.semantic {
                kinds_seen.push(s.kind.as_str());
                assert_eq!(s.kind, ScoreKind::SemanticDistance);
                // The hit's own rank must index the same hit in the leg: this is
                // what makes `legs` + `rank` a checkable pointer rather than a
                // number that happens to be plausible.
                assert_eq!(
                    page.evidence.semantic[s.rank].chunk_id, hit.hit.chunk_id,
                    "rank/leg disagreement for {:?}",
                    hit.hit.chunk_id
                );
            } else {
                missing_leg_is_none += 1;
            }
            if let Some(w) = hit.keyword {
                kinds_seen.push(w.kind.as_str());
                assert_eq!(page.evidence.keyword[w.rank].chunk_id, hit.hit.chunk_id);
            } else {
                missing_leg_is_none += 1;
            }
            if let Some(r) = hit.relevance {
                kinds_seen.push(r.kind.as_str());
                assert!(
                    (0.0..=1.0).contains(&r.value),
                    "a relevance must be in [0,1]: {r:?}"
                );
                assert!(r.query_background > 0.0, "the background must be reported");
                relevance_values.push(r.value);
            }
        }
        if page.evidence.keyword.is_empty() {
            // An empty leg must be `None` on every hit, never 0.0 — which would
            // read as a perfect match on a leg that found nothing.
            for hit in &page.hits {
                if hit.keyword.is_none() {
                    missing_leg_is_none += 1;
                } else if hit.keyword.map(|k| k.raw_score) == Some(0.0) {
                    missing_leg_was_zero += 1;
                }
            }
        }
        let docs: Vec<String> = page.hits.iter().map(|h| h.hit.document.clone()).collect();
        let rank = docs.iter().position(|d| d == gold);
        ranks.push(rank);
        println!(
            "[{TAG}] {:?} class={class} rank={:?} top1={:?} score={:.6} kind={} relevance={:?} legs={:?}",
            query,
            rank,
            docs.first(),
            page.hits.first().map(|h| h.hit.score).unwrap_or(0.0),
            page.hits
                .first()
                .map(|h| h.score_kind.as_str())
                .unwrap_or("none"),
            page.hits
                .first()
                .and_then(|h| h.relevance)
                .map(|r| format!("{:.4}/{:.4}", r.value, r.query_background)),
            page.hits
                .first()
                .map(|h| (h.semantic.map(|s| s.rank), h.keyword.map(|k| k.rank)))
        );
    }
    // RV-A-5: the loop above is the reading. Its length is asserted so a
    // truncated/empty pass cannot produce this test's green.
    assert_eq!(
        queries_measured,
        common::GOLD.len(),
        "every frozen query must have been measured in this run"
    );
    assert!(
        !relevance_values.is_empty(),
        "no relevance values were collected: the per-hit evidence never ran"
    );
    kinds_seen.sort_unstable();
    kinds_seen.dedup();
    println!(
        "[{TAG}] score kinds seen={kinds_seen:?} hits with a missing leg reported as None={missing_leg_is_none} \
         reported as 0.0={missing_leg_was_zero} relevance min={:.4} max={:.4}",
        relevance_values.iter().cloned().fold(f32::MAX, f32::min),
        relevance_values.iter().cloned().fold(f32::MIN, f32::max)
    );
    assert_eq!(
        missing_leg_was_zero, 0,
        "a leg that found nothing must be None, never 0.0 (0.0 is a legal distance)"
    );
    assert!(
        kinds_seen.contains(&"rrf_rank") && kinds_seen.contains(&"semantic_l2sq"),
        "both the fused scale and the semantic distance scale must appear: {kinds_seen:?}"
    );

    // Record the run. An unattributed recall number is not a reading: a row
    // carries the set, the embedder, the window, the fusion and the metrics.
    let rs = ranks.clone();
    let (r1, r5, r20, mrr) = common::metrics(&rs);
    let ndcg = common::ndcg_at_10(&rs);
    let metrics_json = format!(
        "{{\"recall_at_1\":{r1:.4},\"recall_at_5\":{r5:.4},\"recall_at_20\":{r20:.4},\"mrr\":{mrr:.4},\"ndcg_at_10\":{ndcg:.4},\"n\":{},\"relevance_version\":{}}}",
        rs.len(),
        ruagent_knowledge::RELEVANCE_VERSION
    );
    let (embedder_note, fusion) = (embedder_note.clone(), ruagent_knowledge::FUSION.label());
    let recorded = metrics_json.clone();
    db.call(move |conn| -> Result<(), rusqlite::Error> {
        conn.execute(
            "INSERT INTO query_eval_runs (ts, set_id, embedder, leg_window, fusion, metrics_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                chrono::Utc::now().to_rfc3339(),
                set_id,
                embedder_note,
                ruagent_knowledge::LEG_WINDOW as i64,
                fusion,
                metrics_json
            ],
        )?;
        Ok(())
    })
    .await
    .unwrap()
    .unwrap();
    println!("[{TAG}] recorded a query_eval_runs row: {recorded}");

    // ---- C5 on the live corpus: the top of page 1 must not depend on the
    // page size (R-A A4 measured the defect: 2 of 13 live queries changed their
    // top-1 between limit 10 and 20). The judging pair is ( dokument, score ) at
    // four page sizes; all four must agree.
    let mut differing: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (query, _gold, _class) in common::GOLD {
        let mut seen: Vec<(Option<String>, f32)> = Vec::new();
        for limit in [5u32, 10, 20, 30] {
            let page = kb.search_page(query, limit).await.unwrap();
            seen.push((
                page.hits.first().map(|h| h.hit.document.clone()),
                page.hits.first().map(|h| h.hit.score).unwrap_or(0.0),
            ));
        }
        checked += 1;
        if seen.iter().any(|x| *x != seen[0]) {
            differing.push(format!("{query:?}: {seen:?}"));
        }
    }
    println!(
        "[{TAG}] C5 page-size independence: {}/{} queries keep the same top-1 document AND score \
         across limit ∈ {{5,10,20,30}}; differing={differing:?}",
        checked - differing.len(),
        checked
    );
    assert!(
        differing.is_empty(),
        "C5 not met: the top of page 1 changed with the page size: {differing:?}"
    );

    // ---- C4(a): does the INDEXED stage reach the substrings that need it? ---
    //
    // THE FIRST DRAFT OF THIS PROBE WAS WRONG, and the reading it produced is
    // kept here because the mistake is instructive: it counted a query as
    // "reached" only when the stage label was `Bigram`, and so scored 20/51.
    // The other 31 were answered by `Precision`/`Prefix` — i.e. the term turned
    // out to BE a corpus token (长 runs get split by punctuation constantly), so
    // the cheap, bm25-scored stages had already answered it. That is the ladder
    // WORKING, not the index failing.
    //
    // So the population C4 is about is not "all 2-character Han substrings" but
    // the ones the two FTS stages CANNOT reach — the 63.20% measured in R-A A8.
    // The classification uses `ruagent_store::fts::terms`, the same tokenizer the
    // product uses, over ALL chunks (a sample would misclassify and make the
    // assertion below fail spuriously).
    let (chunk_texts, tokens) = {
        let rows: Vec<String> = db
            .call(|conn| -> Result<Vec<String>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT content FROM chunks WHERE content GLOB '*[一-龥]*[一-龥]*' ORDER BY id",
                )?;
                let rows = stmt
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .unwrap()
            .unwrap();
        let mut tokens: Vec<String> = Vec::new();
        for text in &rows {
            for t in ruagent_store::fts::terms(text) {
                if !tokens.contains(&t) {
                    tokens.push(t);
                }
            }
        }
        (rows, tokens)
    };
    let substrings: Vec<String> = {
        let mut seen: Vec<String> = Vec::new();
        for (i, text) in chunk_texts.iter().enumerate() {
            if i % 37 != 0 {
                continue;
            }
            for token in ruagent_store::fts::han_bigrams(text).split(' ') {
                if token.chars().count() == 2
                    && token.chars().all(|c| (c as u32) >= 0x4E00)
                    && !seen.iter().any(|s| s == token)
                {
                    seen.push(token.to_string());
                }
            }
            if seen.len() >= 40 {
                break;
            }
        }
        seen
    };
    assert!(
        substrings.len() >= 20,
        "the corpus must yield at least 20 distinct 2-character Han substrings, or this reading is \
         vacuous (got {})",
        substrings.len()
    );
    // `terms` reach for a term: it IS a token, or a token starts with it.
    let cheap = |s: &str| tokens.iter().any(|t| t == s || t.starts_with(s));
    let (mut cheap_total, mut cheap_found) = (0usize, 0usize);
    let mut needs_index: Vec<(String, String, usize)> = Vec::new();
    let started = std::time::Instant::now();
    for sub in &substrings {
        let legs = kb.search_legs(sub, 5).await.unwrap();
        if cheap(sub) {
            cheap_total += 1;
            if !legs.keyword.is_empty() {
                cheap_found += 1;
            }
        } else {
            needs_index.push((
                sub.clone(),
                format!("{:?}", legs.keyword_stage),
                legs.keyword.len(),
            ));
        }
    }
    let indexed = needs_index
        .iter()
        .filter(|(_, stage, hits)| stage == "Bigram" && *hits > 0)
        .count();
    let fell_through: Vec<&(String, String, usize)> = needs_index
        .iter()
        .filter(|(_, stage, hits)| !(stage == "Bigram" && *hits > 0))
        .collect();
    println!(
        "[{TAG}] C4(a) over {} derived 2-char Han substrings in {}ms: a term-prefix stage already \
         reaches {cheap_found}/{cheap_total} (ladder working); of the {} the two FTS stages CANNOT \
         reach, the bigram index answered {indexed} = {:.4}; fell through: {fell_through:?}",
        substrings.len(),
        started.elapsed().as_millis(),
        needs_index.len(),
        indexed as f64 / needs_index.len().max(1) as f64
    );
    assert!(
        needs_index.len() >= 10,
        "fewer than 10 substrings need the index, so the C4(a) rate below would be near-vacuous \
         ({} of {}): the sample must contain substrings of longer Han runs",
        needs_index.len(),
        substrings.len()
    );
    let rate = indexed as f64 / needs_index.len() as f64;
    assert!(
        rate >= 0.95,
        "C4(a) not met: the bigram index reached {rate:.4} of the 2-character Han substrings that \
         neither FTS stage can reach (target 0.95); fell through: {fell_through:?}"
    );
}
