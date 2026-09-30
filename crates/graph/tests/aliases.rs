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
