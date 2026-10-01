//! Deleting an entity that carries aliases (ruagent-close-the-gaps t31).
//!
//! WHAT WAS BROKEN: `entity_aliases.entity_id`, `resolution_pending.entity_a/b`
//! and `community_entities.entity_id` all declare `REFERENCES entities(id)`
//! (0021) with no `ON DELETE CASCADE`, `foreign_keys=ON` is set at open, and
//! `delete_entity` removed only `entity_edges` — so the panel's delete button
//! answered `sqlite error: FOREIGN KEY constraint failed` for every entity a
//! merge had folded a name onto (and for every entity sitting in a community).
//! Each statement ran in autocommit, so the 500 was not even a no-op: the edges
//! were already gone when the entity delete failed.
//!
//! WHICH ASSERTION CARRIES THIS TEST AGAINST THE PRE-CHANGE BYTES: the
//! `.unwrap()` on `delete_entity(&db, keeper)` below. Before the fix that call
//! returns `Err(DbError::Sqlite("… FOREIGN KEY constraint failed"))` for a merged
//! entity, so the test panics there; every later assertion is a consequence.
//! The no-aliases case is included so the fix cannot regress the path that
//! already worked (it deleted cleanly before, and still must).
//!
//! WHAT THE TEST DOES NOT CLAIM: that the alias TEXT survives. Cascading means
//! it does not — the alias row NAMES the entity, so it cannot outlive it. The
//! last assertion records that consequence rather than hiding it.

use ruagent_graph as g;
use ruagent_store::Db;

/// One `SELECT COUNT(*)` through the product's own entry point.
async fn rows(db: &Db, sql: &'static str, id: i64) -> i64 {
    db.call_flat(move |conn| conn.query_row(sql, [id], |r| r.get(0)))
        .await
        .unwrap()
}

async fn child_rows(db: &Db, id: i64) -> (i64, i64, i64) {
    (
        rows(
            db,
            "SELECT COUNT(*) FROM entity_aliases WHERE entity_id = ?1",
            id,
        )
        .await,
        rows(
            db,
            "SELECT COUNT(*) FROM resolution_pending
              WHERE entity_a = ?1 OR entity_b = ?1",
            id,
        )
        .await,
        rows(
            db,
            "SELECT COUNT(*) FROM community_entities WHERE entity_id = ?1",
            id,
        )
        .await,
    )
}

/// The case the panel broke on: a keeper a merge folded a name onto.
#[tokio::test]
async fn a_merged_entity_is_deletable_and_its_referencing_rows_go_with_it() {
    let db = Db::open_in_memory().unwrap();
    let keeper = g::upsert_entity(&db, "AMBIGUOUS", Some("concept"), Some("a verdict"))
        .await
        .unwrap();
    let absorbed = g::upsert_entity(
        &db,
        "AMBIGUOUS（模糊）",
        Some("concept"),
        Some("the folded name"),
    )
    .await
    .unwrap();
    g::add_fact(
        &db,
        keeper,
        absorbed,
        "alias_of",
        "the two names are one thing",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        g::merge_entities(&db, keeper, absorbed).await.unwrap(),
        1,
        "the edge must follow the keeper"
    );
    assert_eq!(
        g::aliases_of(&db, keeper).await.unwrap(),
        vec!["AMBIGUOUS（模糊）".to_string()],
        "the merge must leave the alias row this defect is about"
    );
    assert!(
        g::search_entities(&db, "AMBIGUOUS（模糊）", 10)
            .await
            .unwrap()
            .iter()
            .any(|e| e.id == keeper),
        "and the folded name must be findable -- the feature the 500 was blocking"
    );

    // THE ASSERTION THAT CARRIES THE TEST: pre-change this is
    // Err(FOREIGN KEY constraint failed).
    let outcome = g::delete_entity(&db, keeper).await.unwrap();
    assert_eq!(
        outcome,
        g::EntityDeleteOutcome::Deleted {
            id: keeper,
            edges_removed: 1,
            facts_removed: 1,
        },
        "deleting a merged entity must succeed and report what it took"
    );

    // No row in ANY of the four child tables still points at the id.
    assert_eq!(
        child_rows(&db, keeper).await,
        (0, 0, 0),
        "child rows survived"
    );
    assert_eq!(
        rows(
            &db,
            "SELECT COUNT(*) FROM entity_edges WHERE src = ?1 OR dst = ?1",
            keeper
        )
        .await,
        0
    );
    assert!(g::entity_by_id(&db, keeper).await.unwrap().is_none());
    assert!(g::aliases_of(&db, keeper).await.unwrap().is_empty());
    // Stated, not hidden: the fold record went with the entity it named.
    assert!(
        !g::search_entities(&db, "AMBIGUOUS（模糊）", 10)
            .await
            .unwrap()
            .iter()
            .any(|e| e.id == keeper),
        "nothing may still answer for a deleted entity"
    );

    // The path that already worked: an entity with no aliases deletes cleanly.
    let fresh = g::upsert_entity(&db, "UNTANGLED", Some("tool"), None)
        .await
        .unwrap();
    assert_eq!(
        g::delete_entity(&db, fresh).await.unwrap(),
        g::EntityDeleteOutcome::Deleted {
            id: fresh,
            edges_removed: 0,
            facts_removed: 0,
        }
    );
    assert!(g::entity_by_id(&db, fresh).await.unwrap().is_none());
    assert_eq!(
        g::delete_entity(&db, fresh).await.unwrap(),
        g::EntityDeleteOutcome::NotFound,
        "an id that is not there is not a silent success"
    );
}

/// The two siblings the same pragma blocks: a pending pair and a community
/// membership. Neither is the reported defect, and both returned the same 500.
#[tokio::test]
async fn the_other_two_fk_children_do_not_block_a_delete() {
    let db = Db::open_in_memory().unwrap();
    let a = g::upsert_entity(&db, "AURORA", Some("concept"), None)
        .await
        .unwrap();
    let b = g::upsert_entity(&db, "BOREALIS", Some("concept"), None)
        .await
        .unwrap();
    let c = g::upsert_entity(&db, "CIRRUS", Some("concept"), None)
        .await
        .unwrap();
    g::add_fact(&db, a, b, "near", "a near b", None, None)
        .await
        .unwrap();
    g::add_fact(&db, b, c, "near", "b near c", None, None)
        .await
        .unwrap();
    g::queue_pending(&db, a, c, "judge said maybe")
        .await
        .unwrap();
    let build = g::build_communities(&db, 0).await.unwrap();
    assert!(build.entities_covered >= 3, "{build:?}");

    assert_eq!(
        child_rows(&db, a).await,
        (0, 1, 1),
        "the premise: this entity has a pending pair AND a community membership"
    );
    let outcome = g::delete_entity(&db, a).await.unwrap();
    assert!(
        matches!(outcome, g::EntityDeleteOutcome::Deleted { .. }),
        "a pending pair or a community membership must not block the delete: {outcome:?}"
    );
    assert_eq!(child_rows(&db, a).await, (0, 0, 0));
    // The OTHER entities' rows are untouched: deleting A must not empty the
    // community B and C still belong to.
    assert_eq!(
        child_rows(&db, b).await.2,
        1,
        "a sibling's community membership must survive"
    );
}
