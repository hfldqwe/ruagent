//! The Han-bigram keyword stage and the new t7 surface (R-A E4, C4, C5, C6).
//!
//! WHY THIS FILE IS SEPARATE FROM retrieval-legs.rs / retrieval-quality.rs:
//! those two must keep COMPILING against the pre-t7 code, because that is how
//! the before/after pair is taken — one instrument, two revisions. Naming
//! `KeywordStage::Bigram` there would turn the pre-t7 run into a compile error,
//! and a compile error is not a measured failure. Everything that needs the new
//! variant or the new types lives here.

use ruagent_knowledge::store::KeywordStage;
use ruagent_knowledge::{Embedder, HashEmbedder, Knowledge, ScoreKind};
use std::sync::Arc;

/// A corpus whose Han runs are LONG on purpose: unicode61 turns a whole run
/// into ONE token, so `水温` inside `泡茶的水温很重要` is neither a token nor a
/// token prefix — the exact case the bigram index exists for.
const HAN: &str = "泡茶的水温很重要。绿茶用八十度的水，红茶用一百度的水。";
const ASCII: &str = "The kettle boils water. Kettle descaling uses vinegar.";

async fn kb(tag: &str) -> (Knowledge, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("ruagent-t7-cjk-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
    let embedder: Arc<dyn Embedder> = Arc::new(HashEmbedder::new(64));
    let k = Knowledge::with_embedder(root.as_path(), db, embedder)
        .await
        .unwrap();
    k.ingest("tea", HAN).await.unwrap();
    k.ingest("kettle", ASCII).await.unwrap();
    (k, root)
}

/// C4's mechanism, at the stage level: a 2-character substring of a Han run must
/// be served by the INDEXED bigram stage, not by the full-table LIKE scan.
///
/// This is the assertion that fails on the pre-t7 code (there the stage is
/// `Substring`), which is what makes it evidence rather than a tautology.
#[tokio::test]
async fn a_two_char_han_substring_is_served_by_the_bigram_index() {
    let (k, root) = kb("bigram").await;
    let legs = k.search_legs("水温", 5).await.unwrap();
    assert_eq!(
        legs.keyword_stage,
        KeywordStage::Bigram,
        "a Han substring must come from chunks_fts_cjk, not from the LIKE scan"
    );
    assert!(!legs.keyword.is_empty(), "the bigram index must find 水温");
    for h in &legs.keyword {
        // The bigram table is a real FTS5 index, so bm25 EXISTS here: a row of
        // this stage reporting 0.0 would be the substring stage's answer
        // (invented) rather than the index's (measured).
        assert!(
            h.raw_score.is_finite() && h.raw_score <= 0.0,
            "bigram bm25 must be finite and <= 0, got {:?}",
            h
        );
    }
    // The construction is the store's own, read from the single source.
    let terms = ruagent_store::fts::terms("水温");
    assert_eq!(ruagent_store::fts::match_bigrams(&terms), "\"水温\"");
    // ... and the user-visible path agrees.
    let hits = k.search("水温", 5).await.unwrap();
    assert!(
        hits.iter().any(|h| h.document == "tea"),
        "the tea page must come back for its own substring: {hits:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The ladder's ORDER, negative half: an exact Han token must stop at
/// `Precision`. A hit that came from the bigram index when the exact token was
/// available would mean the stages run out of order.
#[tokio::test]
async fn an_exact_han_token_stops_at_precision() {
    let (k, root) = kb("precision-first").await;
    let legs = k.search_legs("泡茶的水温很重要", 5).await.unwrap();
    assert_eq!(legs.keyword_stage, KeywordStage::Precision);
    assert!(!legs.keyword.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

/// The bigram stage must not be entered at all for a query with no Han term:
/// `han_bigrams` passes ASCII through as WORDS, so a bigram phrase built from
/// one could not match, and an arm that cannot match only makes the leg look
/// bigger than it is.
#[tokio::test]
async fn a_pure_ascii_query_never_uses_the_bigram_arm() {
    let (k, root) = kb("ascii").await;
    let legs = k.search_legs("kettle descaling", 5).await.unwrap();
    assert_eq!(legs.keyword_stage, KeywordStage::Precision);
    let terms = ruagent_store::fts::terms("kettle descaling");
    assert!(
        ruagent_store::fts::match_bigrams(&terms).is_empty(),
        "no Han term -> no bigram pattern"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// V-SCHEMA(t22) F6, pinned here so a later "simplification" cannot delete the
/// scan: a ONE-character Han query is NOT equivalent to the bigram index.
/// `han_bigrams` emits bigrams for runs of length >= 2, so a single character is
/// not a token in `chunks_fts_cjk` unless the corpus itself has a 1-character
/// run; the LIKE stage is the only path that still reaches it.
///
/// Measured by the verifier on the live corpus: `方` 0/1748, `件` 0/1831,
/// `日` 21/333 rows reachable by bigram but not by LIKE. The bigram arm is a
/// SUPPLEMENT to the scan, never a replacement.
#[tokio::test]
async fn a_one_char_han_query_still_needs_the_scan() {
    let (k, root) = kb("one-char").await;
    let legs = k.search_legs("温", 5).await.unwrap();
    assert_eq!(
        legs.keyword_stage,
        KeywordStage::Substring,
        "a bare Han character is not in the bigram index, so the scan must still run"
    );
    assert!(
        !legs.keyword.is_empty(),
        "the scan must still find 温 inside 泡茶的水温很重要"
    );
    for h in &legs.keyword {
        assert_eq!(
            h.raw_score, 0.0,
            "the scan stage has no bm25 and must not invent one: {h:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The write path fills `chunks.grams` (R-A E4 ①). Read back through a separate
/// read-only connection: asserting it through the search path alone would pass
/// even if the search fell back to the scan.
#[tokio::test]
async fn the_ingest_path_writes_grams() {
    let (_k, root) = kb("grams-written").await;
    let db_path = root.join("data").join("ruagent.db");
    let conn =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let han: String = conn
        .query_row(
            "SELECT grams FROM chunks WHERE content LIKE '%泡茶%' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        han.split(' ').any(|g| g == "水温"),
        "the Han chunk's grams must contain the bigram 水温, got {han:?}"
    );
    assert!(
        han.split(' ').count() >= 2,
        "a Han run longer than 2 chars must produce several bigrams, got {han:?}"
    );
    let ascii: String = conn
        .query_row(
            "SELECT grams FROM chunks WHERE content LIKE '%kettle%' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        ascii.contains("kettle"),
        "ASCII words pass through whole (bigramming them would only add noise), got {ascii:?}"
    );
    assert!(
        !ascii.contains("ke tt"),
        "an ASCII word must NOT be split into bigrams, got {ascii:?}"
    );
    let nulls: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunks WHERE grams IS NULL", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(nulls, 0, "every inserted chunk must carry grams");
    let _ = std::fs::remove_dir_all(&root);
}

/// The new surface's contract, as plain assertions: the wire literals that two
/// generations of consumers read, the fusion's own label, and the four scales
/// that must be distinguishable (C6's library half).
#[test]
fn kinds_labels_and_windows_are_the_frozen_values() {
    // Frozen by R-B D.7: THIS literal, not a synonym.
    assert_eq!(ScoreKind::RrfRank.as_str(), "rrf_rank");
    assert_eq!(ScoreKind::SemanticDistance.as_str(), "semantic_l2sq");
    assert_eq!(ScoreKind::KeywordBm25.as_str(), "bm25");
    assert_eq!(ScoreKind::Cosine.as_str(), "cosine");
    assert_eq!(ScoreKind::Calibrated.as_str(), "calibrated");
    // Four scales, four literals: a consumer can tell them apart.
    let all = [
        ScoreKind::RrfRank,
        ScoreKind::SemanticDistance,
        ScoreKind::KeywordBm25,
        ScoreKind::Cosine,
        ScoreKind::Calibrated,
    ];
    let mut literals: Vec<&str> = all.iter().map(|k| k.as_str()).collect();
    literals.sort_unstable();
    literals.dedup();
    assert_eq!(literals.len(), all.len(), "literals must be distinct");

    assert_eq!(ruagent_knowledge::LEG_WINDOW, 60);
    assert_eq!(
        ruagent_knowledge::FUSION.label(),
        "rrf(k=60,w_sem=2,w_kw=1)"
    );
    assert_eq!(ruagent_knowledge::FUSION.weights(), (60, 2.0, 1.0));
    // Constants pinned at COMPILE time: a runtime assertion on a constant is a
    // tautology, and clippy says so.
    const _: () = assert!(ruagent_knowledge::SCORING_VERSION >= 2);
    const _: () = assert!(ruagent_knowledge::LEG_WINDOW == 60);
}

/// `relevance` is a within-query display score and must behave like one:
/// monotone in the distance, clamped to [0,1], and honest (None) when it has no
/// background to normalise against.
#[test]
fn relevance_is_monotone_clamped_and_honest() {
    let near = ruagent_knowledge::relevance_from_distance(0.1, 0.4).unwrap();
    let far = ruagent_knowledge::relevance_from_distance(0.3, 0.4).unwrap();
    assert!(near.value > far.value, "closer must be more relevant");
    assert!((near.value - 0.75).abs() < 1e-6, "{}", near.value);
    assert_eq!(near.kind, ScoreKind::Calibrated);
    assert_eq!(near.query_background, 0.4);

    // Clamped, not extrapolated: at or beyond the background the value is 0.
    assert_eq!(
        ruagent_knowledge::relevance_from_distance(0.4, 0.4)
            .unwrap()
            .value,
        0.0
    );
    assert_eq!(
        ruagent_knowledge::relevance_from_distance(0.9, 0.4)
            .unwrap()
            .value,
        0.0
    );
    assert_eq!(
        ruagent_knowledge::relevance_from_distance(0.0, 0.4)
            .unwrap()
            .value,
        1.0
    );

    // No background (a one-hit leg) -> no claim. A 1.0 here would be a
    // confident statement about nothing.
    assert!(ruagent_knowledge::relevance_from_distance(0.2, 0.0).is_none());
    // A non-finite distance is how "LanceDB stopped reporting it" arrives; it
    // must not become a number.
    assert!(ruagent_knowledge::relevance_from_distance(f32::NAN, 0.4).is_none());
    assert!(ruagent_knowledge::relevance_from_distance(0.2, f32::NAN).is_none());
}

/// The evidence a caller needs to compare two readings: which fusion and which
/// leg window produced them. Read from the store's own constants (one place),
/// and pinned here because a second copy of these numbers in a test file is
/// exactly the drift this workflow keeps finding.
#[tokio::test]
async fn the_legs_carry_the_window_and_fusion_they_were_computed_with() {
    let (k, root) = kb("window-fields").await;
    let legs = k.search_legs("kettle descaling", 500).await.unwrap();
    assert_eq!(legs.leg_window, ruagent_knowledge::LEG_WINDOW);
    assert_eq!(legs.leg_window, 60);
    assert_eq!(legs.fusion, ruagent_knowledge::FUSION);
    assert_eq!(legs.fusion.label(), "rrf(k=60,w_sem=2,w_kw=1)");
    assert_eq!(
        legs.candidates,
        legs.fused.len(),
        "candidates = the union that entered the fusion"
    );
    // The page argument is ignored ON PURPOSE: 1 and 500 return the same legs.
    let tiny = k.search_legs("kettle descaling", 1).await.unwrap();
    assert_eq!(tiny, legs, "the window must not follow the page size");
    let _ = std::fs::remove_dir_all(&root);
}

/// The paged entry point: the same ranking as `search`, plus the evidence. This
/// is what the daemon will call to stop computing both legs twice (R-A H-2).
#[tokio::test]
async fn search_page_agrees_with_search_and_carries_the_evidence() {
    let (k, root) = kb("page").await;
    for query in ["水温", "kettle descaling", "zebra xylophone"] {
        let page = k.search_page(query, 5).await.unwrap();
        let plain = k.search(query, 5).await.unwrap();
        assert_eq!(
            page.hits.iter().map(|h| h.hit.chunk_id).collect::<Vec<_>>(),
            plain.iter().map(|h| h.chunk_id).collect::<Vec<_>>(),
            "the paged entry point must rank exactly like search() for {query:?}"
        );
        for (a, b) in page.hits.iter().zip(plain.iter()) {
            assert_eq!(a.hit.score, b.score, "same fused score for {query:?}");
            assert_eq!(a.score_kind, ScoreKind::RrfRank);
        }
        assert_eq!(page.evidence.leg_window, ruagent_knowledge::LEG_WINDOW);
        // Every hit's evidence must agree with the leg it came from.
        for hit in &page.hits {
            if let Some(s) = hit.semantic {
                assert_eq!(s.kind, ScoreKind::SemanticDistance);
                assert!(s.raw_score.is_finite() && s.raw_score >= 0.0, "{s:?}");
                assert_eq!(
                    page.evidence.semantic[s.rank].chunk_id, hit.hit.chunk_id,
                    "a hit's semantic rank must index the same hit in the leg"
                );
            }
            if let Some(w) = hit.keyword {
                assert_eq!(w.kind, ScoreKind::KeywordBm25);
                assert_eq!(page.evidence.keyword[w.rank].chunk_id, hit.hit.chunk_id);
            }
            // A hit is in the fused ranking, so at least one leg found it.
            assert!(
                hit.semantic.is_some() || hit.keyword.is_some(),
                "a fused hit with no leg evidence at all is impossible: {hit:?}"
            );
            if let Some(r) = hit.relevance {
                assert_eq!(r.kind, ScoreKind::Calibrated);
                assert!((0.0..=1.0).contains(&r.value), "{r:?}");
                assert_eq!(r.version, ruagent_knowledge::RELEVANCE_VERSION);
            }
        }
    }
    // "No leg found it" must be None, never a zero: a 0.0 would read as a
    // perfect match on a leg that missed entirely.
    let page = k.search_page("zebra xylophone", 5).await.unwrap();
    assert_eq!(page.evidence.keyword_stage, KeywordStage::Empty);
    for hit in &page.hits {
        assert!(
            hit.keyword.is_none(),
            "an empty keyword leg must report None, not 0.0: {hit:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
