//! Per-leg retrieval evidence (t250).
//!
//! WHY: the store computed a semantic leg and a keyword leg all along, then
//! dropped both raw scores inside rrf(), which keeps only ranks. The only score
//! the platform could show was the fused rank score, whose upper bound is
//! legs/61 -- measured at 3 distinct values across 574 live rows and identical
//! for a 1-word and a 7-word query (t247). This test pins the new entry point
//! that keeps the legs, and pins it against the fused entry point so the two
//! cannot drift apart.

use ruagent_knowledge::store::{KeywordStage, KnowledgeLeg, LegConfig};
use ruagent_knowledge::{Embedder, HashEmbedder, Knowledge, rrf_weighted};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

// ---------------------------------------------------------------------------
// t5: the DEFAULT leg configuration is today's behaviour, pinned.
// ---------------------------------------------------------------------------

/// THE REGRESSION BAR (t5). Written FIRST, against the pre-`LegConfig` surface,
/// so it can only stay green if making the legs configurable is a no-op at the
/// default configuration: same chunk ids, same order, same fused scores, same
/// leg evidence, same fusion label.
///
/// The golden below is a PINNED reading of this fixture, not a re-computation:
/// a test that recomputed it would ratify any ranking change.
#[tokio::test]
async fn default_ranking_is_pinned_on_the_fixed_fixture() {
    let (k, root) = kb("t5-golden").await;
    let page = k.search_page("kettle descaling", 5).await.unwrap();

    let ranked: Vec<(i64, String)> = page
        .hits
        .iter()
        .map(|r| (r.hit.chunk_id, r.hit.document.clone()))
        .collect();
    let scores: Vec<String> = page
        .hits
        .iter()
        .map(|r| format!("{:.6}", r.hit.score))
        .collect();
    println!("READING t5 knowledge golden: ranked={ranked:?}");
    println!("READING t5 knowledge golden: scores={scores:?}");
    println!(
        "READING t5 knowledge golden: fusion={} window={} candidates={} keyword_stage={:?}",
        page.evidence.fusion.label(),
        page.evidence.leg_window,
        page.evidence.candidates,
        page.evidence.keyword_stage
    );

    assert_eq!(
        ranked,
        vec![
            (1i64, "kettle".to_string()),
            (2, "rust-ownership".to_string()),
            (3, "chinese-tea".to_string()),
        ],
        "the fixed fixture's default ranking moved"
    );
    assert_eq!(
        scores,
        vec![
            "0.049180".to_string(),
            "0.032258".to_string(),
            "0.031746".to_string()
        ],
        "the fused scores are the 2:1 RRF of the two legs (3/61, 2/62, 2/63)"
    );
    assert_eq!(page.evidence.fusion.label(), "rrf(k=60,w_sem=2,w_kw=1)");
    assert_eq!(page.evidence.leg_window, 60);
    assert_eq!(page.evidence.candidates, 3);
    assert_eq!(page.evidence.keyword_stage, KeywordStage::Precision);
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

// ---------------------------------------------------------------------------
// t5: per-leg configuration. A disabled leg must be FREE, not merely ignored.
// ---------------------------------------------------------------------------

/// An `Embedder` whose two entry points can be armed to fail, and which counts
/// the calls it received. Used to prove that a disabled semantic leg never
/// embeds: with the leg off, a search that WOULD fail if it embedded still
/// succeeds, and the counter does not move.
struct ArmedEmbedder {
    inner: HashEmbedder,
    armed: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}

impl ArmedEmbedder {
    fn new(dim: usize) -> (Self, Arc<AtomicBool>, Arc<AtomicUsize>) {
        let armed = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                inner: HashEmbedder::new(dim),
                armed: armed.clone(),
                calls: calls.clone(),
            },
            armed,
            calls,
        )
    }

    fn check(&self) -> Result<(), ruagent_knowledge::embed::EmbedError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.armed.load(Ordering::SeqCst) {
            return Err(ruagent_knowledge::embed::EmbedError::Other(
                "the embedder was armed to fail: this call must not happen".into(),
            ));
        }
        Ok(())
    }
}

impl Embedder for ArmedEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ruagent_knowledge::embed::EmbedError> {
        self.check()?;
        self.inner.embed(texts)
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, ruagent_knowledge::embed::EmbedError> {
        self.check()?;
        self.inner.embed_query(text)
    }

    fn name(&self) -> &'static str {
        // Same identity as `HashEmbedder`: the stored model meta must match, or
        // opening would migrate the vector table (and embed).
        "hash-embedder"
    }

    fn dim(&self) -> usize {
        self.inner.dim()
    }
}

/// Remove the keyword leg's SQL surface.
///
/// WHY THIS IS THE PROOF, and not a convenience: `keyword_leg` PROPAGATES a
/// failed statement (`fts_match` uses `?`), so once the FTS tables are gone a
/// search that still runs the keyword leg FAILS LOUDLY, while a search whose
/// keyword leg is switched off succeeds. "The SQL was not issued" is therefore
/// decidable without any tracing machinery: the leg's absence is the only way to
/// get an answer at all.
async fn drop_keyword_indexes(db: &ruagent_store::Db) {
    db.call(|conn| {
        conn.execute_batch("DROP TABLE IF EXISTS chunks_fts; DROP TABLE IF EXISTS chunks_fts_cjk;")
    })
    .await
    .expect("writer accepts the DDL")
    .expect("the FTS tables are droppable");
}

/// The default configuration IS today's page: byte-for-byte over the whole
/// `SearchPage` (hits, per-leg evidence, fusion, window, candidates).
#[tokio::test]
async fn the_default_configuration_is_the_default_entry_point() {
    let (k, root) = kb("t5-default-eq").await;
    for q in [
        "kettle descaling",
        "rust ownership",
        "泡茶 水温",
        "zzzz nothing",
    ] {
        let today = k.search_page(q, 5).await.unwrap();
        let configured = k
            .search_page_with(q, 5, &LegConfig::default())
            .await
            .unwrap();
        assert_eq!(
            today, configured,
            "the default leg configuration changed the page for {q:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// ACCEPTANCE: a disabled leg is never queried.
///
/// * the SEMANTIC leg is proved not-queried by an embedder that FAILS when armed
///   and counts its calls: with the leg off the search succeeds while the embedder
///   is armed, and the counter does not move. (With the leg on, the same armed
///   embedder makes the search FAIL — the control that proves the probe can see
///   an embedding.)
/// * the KEYWORD leg is proved not-queried by removing its SQL surface: a search
///   that still issued the FTS statement fails loudly, so the leg being off is the
///   only way to get an answer at all.
#[tokio::test]
async fn a_disabled_knowledge_leg_is_never_queried() {
    let root = std::env::temp_dir().join(format!("ruagent-t5-kb-off-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
    let (embedder, armed, calls) = ArmedEmbedder::new(64);
    let k = Knowledge::with_embedder(root.as_path(), db.clone(), Arc::new(embedder))
        .await
        .unwrap();
    for (name, body) in DOCS {
        k.ingest(name, body).await.unwrap();
    }

    // CONTROL: both legs on, the keyword leg really produces hits.
    let both = k.search_page("kettle descaling", 5).await.unwrap();
    let on_calls = calls.load(Ordering::SeqCst);
    println!(
        "READING t5 kb leg-off CONTROL: embed_calls={on_calls} keyword_leg={} stage={:?} hits={:?}",
        both.evidence.keyword.len(),
        both.evidence.keyword_stage,
        both.hits.iter().map(|h| h.hit.chunk_id).collect::<Vec<_>>()
    );
    assert!(on_calls > 0, "the semantic leg embeds at query time");
    assert!(
        !both.evidence.keyword.is_empty(),
        "the keyword leg must produce hits when it is on (the probe itself must work)"
    );

    // SEMANTIC OFF: no embedding at all, and every hit loses its semantic evidence.
    armed.store(true, Ordering::SeqCst);
    let keyword_only = LegConfig {
        semantic: false,
        ..LegConfig::default()
    };
    let calls_before = calls.load(Ordering::SeqCst);
    let page = k
        .search_page_with("kettle descaling", 5, &keyword_only)
        .await
        .expect("with no semantic leg, the armed embedder must never be reached");
    let calls_after = calls.load(Ordering::SeqCst);
    println!(
        "READING t5 kb semantic-off: embed_calls={} hits={:?} semantic_leg={} keyword_leg={} \
         semantic_evidence={:?}",
        calls_after - calls_before,
        page.hits.iter().map(|h| h.hit.chunk_id).collect::<Vec<_>>(),
        page.evidence.semantic.len(),
        page.evidence.keyword.len(),
        page.hits
            .iter()
            .map(|h| (h.hit.chunk_id, h.semantic.is_some(), h.relevance.is_some()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        calls_after - calls_before,
        0,
        "a disabled semantic leg must not embed the query"
    );
    assert!(page.evidence.semantic.is_empty());
    assert!(
        page.hits
            .iter()
            .all(|h| h.semantic.is_none() && h.relevance.is_none()),
        "no hit may carry semantic evidence when the leg is off: {:?}",
        page.hits
    );
    assert!(
        !page.evidence.keyword.is_empty(),
        "the keyword leg still runs"
    );

    // ... and with the leg ON the same armed embedder proves the probe is live.
    let control = k.search_page("kettle descaling", 5).await;
    println!("READING t5 kb semantic-on armed control: {control:?}");
    assert!(
        control.is_err(),
        "the armed embedder must fail when the semantic leg IS on"
    );
    armed.store(false, Ordering::SeqCst);

    // KEYWORD OFF: remove the FTS tables. A leg that still queries them cannot
    // answer; a leg that is switched off answers without them.
    drop_keyword_indexes(&db).await;
    let semantic_only = LegConfig {
        keyword: false,
        ..LegConfig::default()
    };
    let page = k
        .search_page_with("kettle descaling", 5, &semantic_only)
        .await
        .expect("the keyword leg is off, so the missing FTS tables cannot be reached");
    println!(
        "READING t5 kb keyword-off: keyword_leg={} keyword_evidence={:?} stage={:?} fused={:?}",
        page.evidence.keyword.len(),
        page.hits
            .iter()
            .map(|h| (h.hit.chunk_id, h.keyword.is_some()))
            .collect::<Vec<_>>(),
        page.evidence.keyword_stage,
        page.evidence.fused
    );
    assert!(page.evidence.keyword.is_empty());
    assert!(
        page.hits.iter().all(|h| h.keyword.is_none()),
        "{:?}",
        page.hits
    );
    assert!(
        page.evidence.fused.iter().all(|(id, _)| page
            .evidence
            .semantic
            .iter()
            .any(|l| l.chunk_id == *id)),
        "with only the semantic leg on, every fused id comes from that leg"
    );
    // CONTROL for the keyword probe itself: with the leg ON and the same missing
    // tables, the search must FAIL — proving that the keyword leg really issues
    // that SQL when it is on.
    let keyword_on = k
        .search_page_with("kettle descaling", 5, &LegConfig::default())
        .await;
    println!("READING t5 kb keyword-on missing tables control: {keyword_on:?}");
    assert!(
        keyword_on.is_err(),
        "the keyword leg must query the FTS tables when it is on"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Every leg off: an empty page, no query, and the CONFIGURATION names the legs
/// that were switched off (so the empty page cannot read as "nothing matched").
#[tokio::test]
async fn all_knowledge_legs_off_is_an_empty_page_that_names_them() {
    let root = std::env::temp_dir().join(format!("ruagent-t5-kb-alloff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
    let (embedder, _armed, calls) = ArmedEmbedder::new(64);
    let k = Knowledge::with_embedder(root.as_path(), db.clone(), Arc::new(embedder))
        .await
        .unwrap();
    for (name, body) in DOCS {
        k.ingest(name, body).await.unwrap();
    }

    // Remove the FTS surface first: with every leg off the search must still
    // answer, which is only possible if NO leg queried anything.
    drop_keyword_indexes(&db).await;
    let before_calls = calls.load(Ordering::SeqCst);
    let page = k
        .search_page_with("kettle descaling", 5, &LegConfig::all_off())
        .await
        .expect("an all-off configuration is a reading, never an error");
    let after_calls = calls.load(Ordering::SeqCst);
    println!(
        "READING t5 kb all-off: hits={} candidates={} semantic={} keyword={} stage={:?} \
         fusion={} embed_delta={} disabled={:?}",
        page.hits.len(),
        page.evidence.candidates,
        page.evidence.semantic.len(),
        page.evidence.keyword.len(),
        page.evidence.keyword_stage,
        page.evidence.fusion.label(),
        after_calls - before_calls,
        LegConfig::all_off().disabled_legs()
    );
    assert!(page.hits.is_empty());
    assert_eq!(page.evidence.candidates, 0);
    assert!(page.evidence.fused.is_empty());
    assert_eq!(after_calls, before_calls, "no leg, no embedding");
    assert_eq!(
        LegConfig::all_off().disabled_legs(),
        vec![KnowledgeLeg::Semantic, KnowledgeLeg::Keyword],
        "the empty page's reason is the configuration itself (typed legs; the \
         endpoint maps them to the registry's capability ids)"
    );
    assert!(LegConfig::all_off().all_disabled());
    let _ = std::fs::remove_dir_all(&root);
}

/// A NON-DEFAULT knowledge weight changes the configured fusion, and the changes
/// are visible in the page: the ranking follows the leg the weights favour, and
/// the fused scores are exactly the RRF of the reported legs at those weights.
///
/// The fixture is built so the two legs' orders genuinely DISAGREE (a doc every
/// term matched but long, versus a short doc that repeats one term), because a
/// reorder assertion on a fixture where both legs agree would pass on a pipeline
/// that ignored the weights entirely.
#[tokio::test]
async fn a_non_default_weight_reorders_the_configured_fusion() {
    let root = std::env::temp_dir().join(format!("ruagent-t5-kb-weight-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
    let embedder: Arc<dyn Embedder> = Arc::new(HashEmbedder::new(64));
    let k = Knowledge::with_embedder(root.as_path(), db, embedder)
        .await
        .unwrap();
    k.ingest(
        "both-terms-long",
        "kettle descaling zzz yyy xxx www vvv uuu ttt sss rrr",
    )
    .await
    .unwrap();
    k.ingest("one-term-short", "kettle kettle kettle kettle kettle")
        .await
        .unwrap();

    let default = k.search_page("kettle descaling", 5).await.unwrap();
    let semantic_leg: Vec<i64> = default
        .evidence
        .semantic
        .iter()
        .map(|l| l.chunk_id)
        .collect();
    let keyword_leg: Vec<i64> = default
        .evidence
        .keyword
        .iter()
        .map(|l| l.chunk_id)
        .collect();
    let default_order: Vec<i64> = default.hits.iter().map(|h| h.hit.chunk_id).collect();
    println!(
        "READING t5 kb weights: semantic_leg={semantic_leg:?} keyword_leg={keyword_leg:?} \
         default_order={default_order:?} fusion={}",
        default.evidence.fusion.label()
    );
    assert!(
        !keyword_leg.is_empty() && semantic_leg.len() >= 2,
        "the fixture must put both docs in the semantic leg and at least one in the keyword leg"
    );
    assert_ne!(
        semantic_leg, keyword_leg,
        "the fixture's legs must disagree, or 'reorder' could not be observed"
    );

    // Keyword-dominant: the fusion must follow the KEYWORD leg's order.
    let keyword_dominant = LegConfig::resolve((true, Some(0.000001)), (true, Some(1.0))).unwrap();
    let kw_page = k
        .search_page_with("kettle descaling", 5, &keyword_dominant)
        .await
        .unwrap();
    let kw_order: Vec<i64> = kw_page.hits.iter().map(|h| h.hit.chunk_id).collect();
    println!(
        "READING t5 kb weights: keyword_dominant order={kw_order:?} fusion={}",
        kw_page.evidence.fusion.label()
    );
    assert_eq!(
        kw_order.first(),
        keyword_leg.first(),
        "with the keyword weight dominating, the page must open with the keyword leg's top hit"
    );
    assert_eq!(
        kw_page.evidence.fusion.label(),
        "rrf(k=60,w_sem=0.000001,w_kw=1)"
    );

    // Semantic-dominant: the fusion must follow the SEMANTIC leg's order.
    let semantic_dominant = LegConfig::resolve((true, Some(1.0)), (true, Some(0.000001))).unwrap();
    let sem_page = k
        .search_page_with("kettle descaling", 5, &semantic_dominant)
        .await
        .unwrap();
    let sem_order: Vec<i64> = sem_page.hits.iter().map(|h| h.hit.chunk_id).collect();
    println!(
        "READING t5 kb weights: semantic_dominant order={sem_order:?} fusion={}",
        sem_page.evidence.fusion.label()
    );
    assert_eq!(
        sem_order.first(),
        semantic_leg.first(),
        "with the semantic weight dominating, the page must open with the semantic leg's top hit"
    );
    assert_ne!(
        kw_order, sem_order,
        "the fixture's legs disagree, so the two weightings must order the page differently \
         (this is what makes the weights observable)"
    );

    // The fused scores ARE the RRF of the reported legs at the configured
    // weights — recomputed here from the evidence, so a fusion that ignored the
    // configuration (or a page that reported other weights than it used) fails.
    for (cfg, page) in [
        (keyword_dominant, &kw_page),
        (semantic_dominant, &sem_page),
        (LegConfig::default(), &default),
    ] {
        let ann: Vec<i64> = page.evidence.semantic.iter().map(|l| l.chunk_id).collect();
        let kw: Vec<i64> = page.evidence.keyword.iter().map(|l| l.chunk_id).collect();
        let expected = rrf_weighted(&[(&ann, cfg.w_semantic), (&kw, cfg.w_keyword)], 60);
        assert_eq!(
            page.evidence.fused,
            expected,
            "the fused ranking must be the RRF of the reported legs at {:?}",
            cfg.fusion()
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}
