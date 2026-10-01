//! Aliases are readable (ruagent-close-the-gaps t22).
//!
//! The measurement (t18) found 141 of 257 distinct parenthetical entity names living
//! ONLY in `entity_aliases` — 103 that no strict search returns at all, and 38 whose
//! `match="exact"` is a false positive on a DIFFERENT entity. Nothing was lost;
//! nothing could be READ: the alias text appeared in no response, so a search for a
//! term the graph knew answered with a candidate list no one could verify.
//!
//! These reads are the exposure. What is pinned here is the half that lives in this
//! crate: a name merged onto an entity comes back as an alias OF that entity, and
//! the bulk read keys it by the entity that absorbed it — never by the queried name,
//! which is exactly what makes it verifiable where the search candidate list was not.
//! (The route shape is pinned next door, in `daemon/src/api.rs`'s tests.)

use ruagent_graph as g;
use ruagent_store::Db;

#[tokio::test]
async fn a_merged_name_comes_back_as_an_alias_of_the_entity_that_absorbed_it() {
    let db = Db::open_in_memory().unwrap();

    // The keeper, as resolution creates it ...
    let keeper = g::upsert_entity(&db, "AMBIGUOUS", Some("concept"), Some("a verdict"))
        .await
        .unwrap();
    // ... and the folded name that missed both probes and was merged onto it, plus
    // the extraction's own alias for the same entity.
    assert!(
        g::add_alias(&db, keeper, "AMBIGUOUS（模糊）", "knowledge")
            .await
            .unwrap()
    );
    assert!(
        g::add_alias(&db, keeper, "模糊", "knowledge")
            .await
            .unwrap()
    );

    let entity = g::entity_by_id(&db, keeper)
        .await
        .unwrap()
        .expect("the entity row is readable by id");
    assert_eq!(entity.name, "AMBIGUOUS");
    assert_eq!(entity.kind.as_deref(), Some("concept"));

    assert_eq!(
        g::aliases_of(&db, keeper).await.unwrap(),
        vec!["AMBIGUOUS（模糊）".to_string(), "模糊".to_string()],
        "both aliases, in insertion order"
    );

    let map = g::aliases_for(&db, &[keeper, keeper + 999]).await.unwrap();
    assert_eq!(map.get(&keeper).map(Vec::len), Some(2));
    assert!(
        !map.contains_key(&(keeper + 999)),
        "an entity with no alias is ABSENT from the map, not present-and-empty"
    );
    assert!(
        g::aliases_for(&db, &[]).await.unwrap().is_empty(),
        "no ids, no query, no rows"
    );

    // The negative the whole finding is about: the folded name is not a NAME, so
    // the entity list — the route that used to be the only way to look for it —
    // cannot show it. Before t22 no other response could either.
    let listed: Vec<String> = g::list_entities(&db, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|(e, _)| e.name)
        .collect();
    assert!(
        !listed.iter().any(|n| n == "AMBIGUOUS（模糊）"),
        "{listed:?}"
    );
    assert!(g::entity_by_id(&db, keeper + 999).await.unwrap().is_none());
}

/// An alias-only name is SEARCHABLE, and the alias table is unchanged by it
/// (ruagent-close-the-gaps t25).
///
/// The shape is the corpus's own (t18), reproduced here so the property does not
/// depend on the copy: the queried string is a folded name that lives only in
/// `entity_aliases`, and a DIFFERENT entity carries the same tokens in its
/// SUMMARY -- which is why the phrase pass used to answer `exact` for the wrong
/// entity. `g::search_entities` gains two legs over `entity_aliases` (by
/// `norm_alias`, then through the `entity_aliases_fts` index of migration 0027),
/// and both run BEFORE the `entities_fts(name, summary)` leg, so the entity that
/// OWNS the name answers first and the summary coincidence is still listed.
#[tokio::test]
async fn an_alias_only_name_answers_with_the_entity_that_owns_it() {
    let db = Db::open_in_memory().unwrap();

    // Canary 1: `k1（饱和参数）` -> `通常` was the measured FALSE POSITIVE.
    // `通常`'s summary holds the tokens; the name belongs to `饱和参数`.
    let owner = g::upsert_entity(&db, "饱和参数", Some("concept"), Some("k1 的含义"))
        .await
        .unwrap();
    let decoy = g::upsert_entity(
        &db,
        "通常",
        Some("concept"),
        Some("k1（饱和参数）通常是这个意思"),
    )
    .await
    .unwrap();
    assert!(
        g::add_alias(&db, owner, "k1（饱和参数）", "knowledge")
            .await
            .unwrap()
    );

    // The premise, asserted rather than assumed: the alias text is not a NAME and
    // no entity's text says it except the decoy's summary. If either changes, the
    // readings below stop being about the alias leg.
    let stripped = "k1（饱和参数）";
    let named: Vec<String> = g::list_entities(&db, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|(e, _)| e.name)
        .collect();
    assert!(
        !named.iter().any(|n| n == stripped),
        "the folded name is not an entity name: {named:?}"
    );

    let hits = g::search_entities(&db, stripped, 10).await.unwrap();
    let ids: Vec<i64> = hits.iter().map(|e| e.id).collect();
    assert_eq!(
        ids.first(),
        Some(&owner),
        "the alias-owning entity must answer FIRST: {hits:?}"
    );
    assert!(
        ids.contains(&decoy),
        "the summary coincidence stays in the list -- the answer is verifiable, \
         not narrowed: {hits:?}"
    );

    // Canary 2: `ARIMA（自回归积分滑动平均模型）` -> `自回归积分滑动平均模型`, whose
    // own summary contains the queried text, so the phrase pass reaches it too.
    let arima = g::upsert_entity(
        &db,
        "自回归积分滑动平均模型",
        Some("concept"),
        Some("ARIMA（自回归积分滑动平均模型）是一种时间序列模型"),
    )
    .await
    .unwrap();
    assert!(
        g::add_alias(&db, arima, "ARIMA（自回归积分滑动平均模型）", "knowledge")
            .await
            .unwrap()
    );
    let hits = g::search_entities(&db, "ARIMA（自回归积分滑动平均模型）", 10)
        .await
        .unwrap();
    assert_eq!(
        hits.first().map(|e| e.id),
        Some(arima),
        "the entity whose alias this is must be the top hit: {hits:?}"
    );

    // A name that is BOTH an entity's own name and another entity's alias: the
    // NAME wins. Without an identity leg ahead of the alias leg this query would
    // be answered by the wrong entity -- the regression this ordering exists to
    // prevent.
    let name_owner = g::upsert_entity(&db, "DayDream", Some("tool"), Some("调度器"))
        .await
        .unwrap();
    assert!(
        g::add_alias(&db, name_owner, "E-031-DayDream", "knowledge")
            .await
            .unwrap()
    );
    let also_a_name = g::upsert_entity(&db, "E-031-DayDream", Some("doc"), None)
        .await
        .unwrap();
    let hits = g::search_entities(&db, "E-031-DayDream", 10).await.unwrap();
    assert_eq!(
        hits.first().map(|e| e.id),
        Some(also_a_name),
        "an entity NAMED by the query must outrank an entity whose alias it also is: {hits:?}"
    );

    // What the ALIAS INDEX buys over an exact `norm_alias` probe: a query that is
    // only PART of a folded name. The premise (nothing else in the graph carries
    // the token) is asserted, so the only path to the owner is the alias index.
    let aqs = g::upsert_entity(
        &db,
        "AbstractQueuedSynchronizer",
        Some("concept"),
        Some("同步器框架"),
    )
    .await
    .unwrap();
    assert!(
        g::add_alias(&db, aqs, "AQS（AbstractQueuedSynchronizer）", "knowledge")
            .await
            .unwrap()
    );
    let text_hits: i64 = db
        .call(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM entities
                  WHERE name LIKE '%AQS%' OR summary LIKE '%AQS%'",
                [],
                |r| r.get(0),
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        text_hits, 0,
        "no entity text may carry `AQS`: if it did, this probe would prove nothing"
    );
    let hits = g::search_entities(&db, "AQS", 10).await.unwrap();
    assert!(
        hits.iter().any(|e| e.id == aqs),
        "a query that is a part of a folded name must reach its entity: {hits:?}"
    );

    // READ-ONLY: the search never rewrites the alias table, and an entity's
    // `name` is still not the folded form.
    assert_eq!(
        g::aliases_of(&db, owner).await.unwrap(),
        vec!["k1（饱和参数）".to_string()]
    );
    assert_eq!(
        g::entity_by_id(&db, owner).await.unwrap().unwrap().name,
        "饱和参数"
    );
    assert_eq!(
        g::list_entities(&db, 50).await.unwrap().len(),
        6,
        "the search created no entity"
    );
}
