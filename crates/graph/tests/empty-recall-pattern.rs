//! The empty recall pattern must DEGRADE, not error (t312 / t321).
//!
//! WHY this file exists: search_entities_loose degrades in three stages
//! (precision -> prefix OR -> LIKE). When EVERY term of the query is a
//! sub-floor ASCII fragment (see ruagent_store::fts::MIN_RECALL_ASCII, added by
//! t270), the prefix form comes out as an EMPTY STRING -- and FTS5 rejects an
//! empty MATCH with "fts5: syntax error near \"\"". Until t312 the function
//! handed that empty string to MATCH, so a query like "A2", "go", "11" or "v2"
//! came back as an Err instead of "nothing found". t312 fixed it; this test
//! pins it, because a fix with no test comes back.
//!
//! OBJECT SET: the four entities created below; hits are ENTITY ids as returned
//! by the product (search_entities / search_entities_loose).
//! SAMPLING SURFACE: in-memory SQLite, entities_fts populated by the same
//! triggers the live database uses.
//!
//! The fixture deliberately contains NO token "a2"/"go"/"11"/"v2": if the
//! precision stage could answer one of these queries, the empty recall form
//! would never be reached and the test would prove nothing. The premise is
//! asserted below, not assumed.
use ruagent_graph::{search_entities, search_entities_loose, upsert_entity};
use ruagent_store::Db;

/// Shaped like the live hard cases: t301's hyphenated probe names (the names
/// that made F-301b look like "hyphens are broken"), a plain ASCII name, and a
/// Han name whose characters only LIKE can reach.
const ENTITIES: &[(&str, &str, &str)] = &[
    ("t301probe-A", "concept", "probe entity"),
    ("t301probe-B", "concept", "probe entity"),
    ("submarine", "concept", "深海潜艇暗号"),
    ("泡茶水温", "concept", "绿茶用八十度"),
];

async fn seed(db: &Db) {
    for (name, kind, summary) in ENTITIES {
        upsert_entity(db, name, Some(kind), Some(summary))
            .await
            .unwrap();
    }
}

/// (strict hits, loose hits) as SORTED NAMES, so the reading is about the
/// answer rather than about rank order.
async fn hits(db: &Db, q: &str) -> (Vec<String>, Vec<String>) {
    let mut s: Vec<String> = search_entities(db, q, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    let mut l: Vec<String> = search_entities_loose(db, q, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    s.sort();
    l.sort();
    (s, l)
}

#[tokio::test]
async fn two_char_ascii_queries_degrade_instead_of_erroring() {
    let db = Db::open_in_memory().unwrap();
    seed(&db).await;

    // THE PREMISE, asserted rather than assumed: for these queries every term is
    // below the recall floor, so the prefix form is EMPTY (this is exactly the
    // string that used to be handed to MATCH) and the LIKE form is empty too.
    // If a later change moves the floor, this assertion is where the reader
    // finds out why the counts below are what they are.
    for q in ["A2", "go", "11", "v2"] {
        let terms = ruagent_store::fts::terms(q);
        assert!(
            !terms.is_empty(),
            "{q:?} must tokenize to at least one term, or it is not this case"
        );
        assert!(
            ruagent_store::fts::match_any_prefix(&terms).is_empty(),
            "{q:?}: the prefix form must be EMPTY for this test to exercise the empty-pattern path (did the recall floor move?)"
        );
        assert!(
            ruagent_store::fts::like_patterns(&terms).is_empty(),
            "{q:?}: the LIKE form is floored too, which is why the expected answer is zero"
        );
    }

    // THE TEETH, executed rather than asserted in prose: the exact statement the
    // pre-t312 code reached is still an error today, so the guard is guarding
    // something. If SQLite ever accepts an empty MATCH, this line goes red and
    // the reason this test exists has changed.
    let empty_match = db
        .call(|conn| {
            conn.query_row(
                "SELECT count(*) FROM entities_fts WHERE entities_fts MATCH ''",
                [],
                |r| r.get::<_, i64>(0),
            )
        })
        .await
        .expect("the statement must reach sqlite");
    assert!(
        empty_match.is_err(),
        "an empty MATCH must still be an error, or this guard guards nothing (got {empty_match:?})"
    );

    // The regression itself: these four used to return
    // Err("sqlite error: fts5: syntax error near \"\""). They must now return a
    // normal, empty answer -- and the same answer the strict path gives.
    for q in ["A2", "go", "11", "v2"] {
        let loose = search_entities_loose(&db, q, 10)
            .await
            .unwrap_or_else(|e| panic!("{q:?} must degrade, not error: {e}"));
        let strict = search_entities(&db, q, 10).await.unwrap();
        assert!(
            loose.is_empty() && strict.is_empty(),
            "{q:?}: a sub-floor ASCII query is not evidence, so both legs must come back empty (got strict={}, loose={})",
            strict.len(),
            loose.len()
        );
    }

    // CONTROLS: normal queries keep the counts they had. "a" is answered by
    // PRECISION (the token a of t301probe-A) even though its prefix form would
    // be empty -- which is why the empty-pattern guard must skip that stage
    // rather than empty the whole answer. "茶" is the LIKE case: strict cannot
    // reach a substring of the Han run, loose can.
    assert_eq!(
        hits(&db, "a").await,
        (
            vec!["t301probe-A".to_string()],
            vec!["t301probe-A".to_string()]
        ),
        "a one-character ASCII query still hits by token equality"
    );
    assert_eq!(
        hits(&db, "submarine").await,
        (vec!["submarine".to_string()], vec!["submarine".to_string()]),
        "a normal-length ASCII term is unchanged"
    );
    assert_eq!(
        hits(&db, "茶").await,
        (Vec::<String>::new(), vec!["泡茶水温".to_string()]),
        "a Han substring is still reached by LIKE and still missed by FTS"
    );

    // And the entity names themselves: the t301 shape is findable, which is why
    // F-301b ("a hyphenated name cannot be searched") is not the defect it
    // looked like.
    let (s, l) = hits(&db, "t301probe").await;
    assert_eq!(
        s,
        vec!["t301probe-A".to_string(), "t301probe-B".to_string()],
        "the t301 shape: the strict path finds both hyphenated probe names"
    );
    assert!(l.len() >= s.len(), "loose must never lose a strict hit");
}
