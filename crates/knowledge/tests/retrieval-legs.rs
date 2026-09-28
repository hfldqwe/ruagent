//! Per-leg retrieval evidence (t250).
//!
//! WHY: the store computed a semantic leg and a keyword leg all along, then
//! dropped both raw scores inside rrf(), which keeps only ranks. The only score
//! the platform could show was the fused rank score, whose upper bound is
//! legs/61 -- measured at 3 distinct values across 574 live rows and identical
//! for a 1-word and a 7-word query (t247). This test pins the new entry point
//! that keeps the legs, and pins it against the fused entry point so the two
//! cannot drift apart.

use ruagent_knowledge::store::KeywordStage;
use ruagent_knowledge::{Embedder, HashEmbedder, Knowledge};
use std::sync::Arc;

const DOCS: &[(&str, &str)] = &[
    (
        "kettle",
        "The kettle boils water. Kettle descaling: fill with vinegar and water, boil, then rinse twice.",
    ),
    (
        "rust-ownership",
        "Rust ownership: each value has one owner. When the owner goes out of scope the value is dropped.",
    ),
    (
        "chinese-tea",
        "泡茶的水温很重要。绿茶用八十度的水，红茶用一百度的水。",
    ),
];

async fn kb(tag: &str) -> (Knowledge, std::path::PathBuf) {
    let root =
        std::env::temp_dir().join(format!("ruagent-t250-legs-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
    let embedder: Arc<dyn Embedder> = Arc::new(HashEmbedder::new(64));
    let k = Knowledge::with_embedder(root.as_path(), db, embedder)
        .await
        .unwrap();
    for (name, body) in DOCS {
        k.ingest(name, body).await.unwrap();
    }
    (k, root)
}

#[tokio::test]
async fn both_legs_carry_their_own_raw_score() {
    let (k, root) = kb("a").await;
    let legs = k.search_legs("kettle descaling", 5).await.unwrap();

    assert!(
        !legs.semantic.is_empty(),
        "semantic leg must return something"
    );
    assert!(
        !legs.keyword.is_empty(),
        "keyword leg must return something"
    );
    assert!(!legs.fused.is_empty(), "fused ranking must not be empty");

    for (i, h) in legs.semantic.iter().enumerate() {
        assert_eq!(h.rank, i, "semantic ranks must be 0-based and contiguous");
        // NAN would mean LanceDB stopped reporting its distance column. A test
        // that accepted it would hide exactly the failure this guards.
        assert!(
            h.raw_score.is_finite(),
            "semantic raw_score must be the ANN distance, not NAN"
        );
        assert!(h.raw_score >= 0.0, "a distance is not negative");
    }
    for (i, h) in legs.keyword.iter().enumerate() {
        assert_eq!(h.rank, i, "keyword ranks must be 0-based and contiguous");
        assert!(
            h.raw_score.is_finite(),
            "keyword raw_score must be bm25, not NAN"
        );
        // SQLite FTS5 bm25(): more negative is a better match.
        assert!(h.raw_score <= 0.0, "bm25 must be <= 0, got {}", h.raw_score);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn fused_ranking_is_the_same_as_the_search_entry_point() {
    let (k, root) = kb("b").await;
    for q in [
        "kettle descaling",
        "rust ownership",
        "泡茶 水温",
        "zzzz nothing here",
    ] {
        let legs = k.search_legs(q, 5).await.unwrap();
        let hits = k.search(q, 5).await.unwrap();
        let from_fused: Vec<i64> = legs.fused.iter().take(5).map(|(id, _)| *id).collect();
        let from_search: Vec<i64> = hits.iter().map(|h| h.chunk_id).collect();
        // Same set (search re-sorts after hydrating by the same fused score).
        let mut a = from_fused.clone();
        let mut b = from_search.clone();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(
            a, b,
            "the two entry points disagree for {:?}: legs {:?} vs search {:?}",
            q, from_fused, from_search
        );
        if !legs.fused.is_empty() {
            assert_eq!(
                legs.fused[0].0, from_search[0],
                "the top fused id must be the top hit for {:?}",
                q
            );
        }
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn the_legs_are_genuinely_separate() {
    let (k, root) = kb("c").await;
    // A query whose words appear nowhere: the keyword leg must be empty while
    // the semantic leg still returns its nearest neighbours. If both were the
    // same computation this would be impossible.
    let legs = k.search_legs("zebra xylophone", 5).await.unwrap();
    assert!(
        legs.keyword.is_empty(),
        "a query with no lexical match must leave the keyword leg empty, got {:?}",
        legs.keyword
    );
    assert!(
        !legs.semantic.is_empty(),
        "the semantic leg still returns nearest neighbours"
    );
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// t261: the keyword leg degrades precision -> prefix -> LIKE.
// ---------------------------------------------------------------------------

/// The three stages, each asserted on a query that can ONLY be served by the
/// stage it is supposed to reach. A test that merely asserted "non-empty" would
/// pass on any of the three.
#[tokio::test]
async fn keyword_leg_stage_is_precision_when_every_term_matches() {
    let (k, root) = kb("stage-precision").await;
    let legs = k.search_legs("kettle descaling", 5).await.unwrap();
    assert_eq!(legs.keyword_stage, KeywordStage::Precision);
    assert!(!legs.keyword.is_empty());
    // A real FTS match, so bm25 exists and is reported.
    for h in &legs.keyword {
        assert!(h.raw_score.is_finite() && h.raw_score <= 0.0, "{:?}", h);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// "kettles descaling": the token "kettles" is in NO document, so the AND form
/// cannot match at all; the prefix form ("kettles"* OR "descaling"*) can. The
/// stage label is the evidence that the AND form was tried first and lost.
#[tokio::test]
async fn keyword_leg_stage_is_prefix_only_when_precision_found_nothing() {
    let (k, root) = kb("stage-prefix").await;
    let legs = k.search_legs("kettles descaling", 5).await.unwrap();
    assert_eq!(legs.keyword_stage, KeywordStage::Prefix);
    assert!(!legs.keyword.is_empty());
    // Control, run through the same single-source construction: the precision
    // form for this query contains a term no document has.
    let terms = ruagent_store::fts::terms("kettles descaling");
    assert_eq!(
        ruagent_store::fts::match_all(&terms),
        "\"kettles\" \"descaling\""
    );
    for h in &legs.keyword {
        assert!(h.raw_score.is_finite() && h.raw_score <= 0.0, "{:?}", h);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// unicode61 makes the whole Han run "泡茶的水温很重要" ONE term, so no FTS query
/// can find "水温" inside it -- neither the phrase form nor a prefix. Since t7 a
/// bigram index reaches it WITHOUT scanning the table; before t7 the ONLY path
/// was LIKE.
///
/// This test is deliberately version-agnostic about WHICH index does the work:
/// it asserts the stage is NOT the full-table scan, which is the property the
/// spec's C4 is about. Pre-t7 this is red (the stage there is `Substring`);
/// naming `KeywordStage::Bigram` here would make the pre-t7 run a compile error
/// instead of a measured failure, which is why the exact variant is pinned in
/// retrieval-cjk.rs.
#[tokio::test]
async fn han_substring_is_not_served_by_the_full_table_scan() {
    let (k, root) = kb("stage-substring").await;
    let legs = k.search_legs("水温", 5).await.unwrap();
    assert_ne!(
        legs.keyword_stage,
        KeywordStage::Substring,
        "a 2-character Han substring must be indexed, not scanned"
    );
    assert!(
        !legs.keyword.is_empty(),
        "the index must find 水温 inside 泡茶的水温很重要"
    );
    for h in &legs.keyword {
        // Both the indexed stage (bm25 <= 0) and the scan stage (0.0 by
        // construction) satisfy this; asserting 0.0 exactly would ratify the
        // scan, and asserting < 0 would ratify only the bm25 path.
        assert!(h.raw_score.is_finite() && h.raw_score <= 0.0, "{:?}", h);
    }
    // The hit really is the tea chunk (the only document with that run), and
    // the query term is a strict substring of the stored term -- which is
    // exactly the case FTS5's plain tokenizer cannot express.
    let hits = k.search("水温", 5).await.unwrap();
    assert!(
        hits.iter().any(|h| h.document == "chinese-tea"),
        "{:?}",
        hits
    );
    let terms = ruagent_store::fts::terms("水温");
    assert_eq!(terms, vec!["水温".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// t7: the leg window must not follow the caller's page size (R-A C5).
// ---------------------------------------------------------------------------

/// C5's MECHANISM, asserted where it is decidable rather than where it happens
/// to be observable: with `leg_k = limit.max(10)`, the leg LENGTHS change with
/// the page size, so the fused score of a document depends on how many results
/// the caller asked for. The live measurement (R-A A4) saw that surface as 2 of
/// 13 queries changing their top-1 between limit 10 and 20 — an occasional
/// symptom of a guaranteed cause.
///
/// The fixture needs the limits to BIND: 12 documents sharing one token, so the
/// keyword leg has more candidates than the small limit allows.
///
/// Pre-t7 this is red on the first assertion (10 vs 12 rows); post-t7 both calls
/// read LEG_WINDOW and agree.
#[tokio::test]
async fn the_leg_window_does_not_follow_the_page_size() {
    let root = std::env::temp_dir().join(format!("ruagent-t7-window-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
    let embedder: Arc<dyn Embedder> = Arc::new(HashEmbedder::new(64));
    let k = Knowledge::with_embedder(root.as_path(), db, embedder)
        .await
        .unwrap();
    for i in 0..12 {
        k.ingest(
            &format!("kettle-{i}"),
            &format!(
                "Kettle note {i}: the kettle boils water for batch {i}. Kettle descaling follows."
            ),
        )
        .await
        .unwrap();
    }

    for query in ["kettle", "kettle descaling", "kettle water boil"] {
        let small = k.search_legs(query, 5).await.unwrap();
        let large = k.search_legs(query, 60).await.unwrap();
        assert!(
            large.keyword.len() > 10,
            "the fixture must exceed the pre-t7 small-limit window (10), or nothing here binds \
             ({query:?}: {})",
            large.keyword.len()
        );
        assert_eq!(
            small.keyword.len(),
            large.keyword.len(),
            "the keyword leg's size depends on the page size for {query:?}"
        );
        assert_eq!(
            small.semantic.len(),
            large.semantic.len(),
            "the semantic leg's size depends on the page size for {query:?}"
        );
        assert_eq!(
            small.fused, large.fused,
            "the fused ranking depends on the page size for {query:?}"
        );
        // The user-visible form of the same property: page 1 must not move
        // when the caller asks for a longer page.
        let a = k.search(query, 5).await.unwrap();
        let b = k.search(query, 60).await.unwrap();
        assert_eq!(
            (a[0].document.clone(), a[0].score),
            (b[0].document.clone(), b[0].score),
            "top-1 changed with the page size for {query:?}"
        );
        assert_eq!(
            a.iter().map(|h| h.chunk_id).collect::<Vec<_>>(),
            b.iter()
                .take(a.len())
                .map(|h| h.chunk_id)
                .collect::<Vec<_>>(),
            "the first page must be a prefix of the longer page for {query:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// A pre-t7 behaviour check that is now vacuous must not silently disappear:
/// `search_legs`'s second argument no longer affects anything (the window is
/// constant), so 5 and 500 must return the SAME legs. Stated with the old
/// surface only (`.semantic` / `.keyword` / `.fused`), because this file has to
/// compile against the pre-t7 revision; the new fields are pinned in
/// retrieval-cjk.rs.
#[tokio::test]
async fn the_page_argument_no_longer_changes_the_legs() {
    let (k, root) = kb("ignored-arg").await;
    let a = k.search_legs("kettle descaling", 1).await.unwrap();
    let b = k.search_legs("kettle descaling", 500).await.unwrap();
    assert_eq!(
        a, b,
        "the leg window is a constant, whatever the caller says"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A query with no lexical presence must leave the leg EMPTY, and say so --
/// not silently fall through to a stage that returns noise.
#[tokio::test]
async fn keyword_leg_stage_is_empty_when_no_stage_matches() {
    let (k, root) = kb("stage-empty").await;
    let legs = k.search_legs("zebra xylophone", 5).await.unwrap();
    assert_eq!(legs.keyword_stage, KeywordStage::Empty);
    assert!(legs.keyword.is_empty());
    // ... and the semantic leg is untouched by any of this.
    assert!(!legs.semantic.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
