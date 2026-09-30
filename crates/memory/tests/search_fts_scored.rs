//! t16: `search_fts_scored` read the WRONG COLUMN, and this file pins the repair from
//! outside the crate (public API only).
//!
//! THE DEFECT, for the record: the SELECT listed the eleven memory columns —
//! `source_episode` at index 10 — and THEN `bm25(memories_fts)` at index 11, while the
//! row closure read the score with `r.get::<_, f64>(10)`. So the leg reported the row's
//! **episode id** as its relevance score, and on a row whose `source_episode` was NULL it
//! failed outright (`Invalid column type Null at index: 10, name: source_episode`), which
//! the recall caller swallowed into an empty leg. Both shapes are pinned below.
//!
//! THE FIX: the score column is ALIASED (`bm25(memories_fts) AS bm25`) and read BY NAME,
//! the same way `row_to_memory` already reads every other column. An index that was wrong
//! once can be wrong again; a name cannot silently point at the neighbouring column.
//!
//! WHY THE ASSERTIONS COMPARE AGAINST `true_bm25` AND NOT A PINNED NUMBER: the numbers
//! belong to this fixture, and the property under test is "the leg reports SQLite's bm25",
//! not "the leg reports 0.2013". The independent reading is deliberately a SECOND SQL
//! statement with the column named, so a future edit to the production SELECT cannot move
//! both sides at once and hide a regression.

use ruagent_memory::query::{fts_pattern, search_fts, search_fts_scored};
use ruagent_memory::write::{MemoryWrite, WriteOutcome, write_memory};
use ruagent_memory::{MemoryStore, Namespace};
use ruagent_store::Db;

fn w(content: &str) -> MemoryWrite {
    MemoryWrite {
        store: MemoryStore::Lesson,
        namespace: Namespace::parse("project:t16").expect("test namespace parses"),
        content: content.to_string(),
        confidence: 0.9,
        // The NULL shape (the t5 fixture's): overwritten by `attach_episode` where a
        // platform-written row is wanted.
        source_episode: None,
        supersedes: None,
    }
}

async fn write_row(db: &Db, content: &str) -> i64 {
    match write_memory(db, &w(content))
        .await
        .expect("row is writable")
    {
        WriteOutcome::Inserted(id) => id,
        other => panic!("fixture write must insert, got {other:?}"),
    }
}

/// Give a memory row a NON-NULL `source_episode` — the shape every platform-written row
/// has (t9 measured 3 rows / 0 nulls on live data), and the shape the old read silently
/// mistook for a relevance score. Returns the episode id, i.e. the number the defect
/// reported.
async fn attach_episode(db: &Db, memory_id: i64, hash: &str) -> i64 {
    let hash = hash.to_string();
    db.call(move |conn| -> Result<i64, rusqlite::Error> {
        conn.execute(
            "INSERT INTO episodes (kind, content, content_hash, ref_time, ingested_at)
             VALUES ('manual', 't16 episode', ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![hash],
        )?;
        let episode_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE memories SET source_episode = ?1 WHERE id = ?2",
            rusqlite::params![episode_id, memory_id],
        )?;
        Ok(episode_id)
    })
    .await
    .expect("writer is alive")
    .expect("the episode attaches")
}

/// The TRUE bm25 per matching row, in the leg's own order — an INDEPENDENT statement that
/// names the column, so it cannot inherit a mistake from the production SQL.
async fn true_bm25(db: &Db, query: &str) -> Vec<(i64, f64)> {
    let query = fts_pattern(query);
    db.call(move |conn| -> Result<Vec<(i64, f64)>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT m.id, bm25(memories_fts) AS bm25 FROM memories_fts f
             JOIN memories m ON m.id = f.rowid
             WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL AND m.deleted_at IS NULL
             ORDER BY rank LIMIT 20",
        )?;
        let rows = stmt.query_map(rusqlite::params![query], |r| {
            Ok((r.get::<_, i64>("id")?, r.get::<_, f64>("bm25")?))
        })?;
        rows.collect()
    })
    .await
    .expect("writer is alive")
    .expect("the index reading runs")
}

/// Shape 1 — `source_episode` NULL. Before the repair this call FAILED; the whole leg
/// went down with it.
#[tokio::test]
async fn a_null_source_episode_reports_bm25_instead_of_failing() {
    let db = Db::open_in_memory().expect("in-memory database opens");
    let first = write_row(&db, "alpha beta").await;
    write_row(&db, "alpha beta gamma").await;

    let rows = search_fts_scored(&db, "alpha beta", 10)
        .await
        .expect("a NULL source_episode must not fail the read (t16: the old code returned Err)");
    let truth = true_bm25(&db, "alpha beta").await;
    println!(
        "READING t16 NULL shape: reported={:?} true_bm25={truth:?}",
        rows.iter().map(|(r, s)| (r.id, *s)).collect::<Vec<_>>()
    );
    assert_eq!(rows.len(), truth.len(), "the same rows match");
    assert_eq!(rows[0].0.id, first, "the better bm25 row leads");
    for ((row, score), (id, real)) in rows.iter().zip(truth.iter()) {
        assert_eq!(row.id, *id, "same order as the independent reading");
        assert_eq!(
            score, real,
            "the leg must report SQLite's bm25 for row {}",
            row.id
        );
        assert!(
            *score < 0.0,
            "bm25 is negative for a match; the old value here was a type error"
        );
        assert_eq!(
            row.source_episode, None,
            "this is the NULL shape under test, not the other branch"
        );
    }
}

/// Shape 2 — `source_episode` NON-NULL, i.e. what live rows look like. Before the repair
/// this returned the EPISODE ID as the score, silently.
#[tokio::test]
async fn a_non_null_source_episode_reports_bm25_not_the_episode_id() {
    let db = Db::open_in_memory().expect("in-memory database opens");
    let first = write_row(&db, "alpha beta").await;
    let second = write_row(&db, "alpha beta gamma").await;
    let first_episode = attach_episode(&db, first, "t16-episode-1").await;
    let second_episode = attach_episode(&db, second, "t16-episode-2").await;

    let rows = search_fts_scored(&db, "alpha beta", 10)
        .await
        .expect("a non-NULL source_episode must not fail the read");
    let truth = true_bm25(&db, "alpha beta").await;
    println!(
        "READING t16 NON-NULL shape: reported={:?} true_bm25={truth:?} episodes={{first: \
         {first_episode}, second: {second_episode}}}",
        rows.iter().map(|(r, s)| (r.id, *s)).collect::<Vec<_>>()
    );
    assert_eq!(rows.len(), 2, "both rows match");
    for ((row, score), (id, real)) in rows.iter().zip(truth.iter()) {
        assert_eq!(row.id, *id);
        let episode_id = if row.id == first {
            first_episode
        } else {
            second_episode
        };
        assert_eq!(
            row.source_episode,
            Some(episode_id),
            "the row really carries an episode, so the old `get(10)` had a number to return"
        );
        assert_eq!(
            score, real,
            "the leg must report SQLite's bm25 for row {}",
            row.id
        );
        assert_ne!(
            *score, episode_id as f64,
            "the reported score must NOT be the row's episode id — that was the defect"
        );
        assert!(*score < 0.0, "bm25 is negative for a match");
    }
}

/// The mixed shape, which is what actually broke the leg hardest: ONE NULL row used to
/// fail the whole statement, so the non-NULL rows disappeared with it.
#[tokio::test]
async fn a_null_row_no_longer_hides_the_others() {
    let db = Db::open_in_memory().expect("in-memory database opens");
    let null_row = write_row(&db, "alpha beta").await;
    let episode_row = write_row(&db, "alpha beta gamma").await;
    attach_episode(&db, episode_row, "t16-episode-mixed").await;

    let rows = search_fts_scored(&db, "alpha beta", 10)
        .await
        .expect("one NULL row must not take the leg down");
    let truth = true_bm25(&db, "alpha beta").await;
    println!(
        "READING t16 mixed shape: reported={:?} true_bm25={truth:?}",
        rows.iter().map(|(r, s)| (r.id, *s)).collect::<Vec<_>>()
    );
    assert_eq!(
        rows.iter().map(|(r, _)| r.id).collect::<Vec<_>>(),
        vec![null_row, episode_row],
        "both shapes are returned, in the leg's own rank order"
    );
    assert!(
        rows.iter()
            .zip(truth.iter())
            .all(|((_, score), (_, real))| score == real),
        "each row carries its own bm25, whichever shape it is"
    );
}

/// THE RANKING QUESTION AT THE LEG LEVEL: the leg's ORDER never came from the wrong value.
/// `search_fts_scored` and the score-less `search_fts` share the SELECT's `ORDER BY rank`,
/// so the repair moves the reported NUMBER, not the leg's rank order. (Whether the fused
/// page moved is measured in the daemon's `memembed` tests, where the fused sections live.)
#[tokio::test]
async fn the_leg_order_did_not_come_from_the_score() {
    let db = Db::open_in_memory().expect("in-memory database opens");
    let first = write_row(&db, "alpha beta").await;
    let second = write_row(&db, "alpha beta gamma").await;
    // One of each shape, so this test would ALSO have been red before the repair.
    attach_episode(&db, second, "t16-episode-order").await;

    let scored = search_fts_scored(&db, "alpha beta", 10)
        .await
        .expect("the scored leg reads both shapes");
    let plain = search_fts(&db, "alpha beta", 10)
        .await
        .expect("the score-less leg reads both shapes");
    println!(
        "READING t16 order: scored={:?} plain={:?}",
        scored.iter().map(|(r, _)| r.id).collect::<Vec<_>>(),
        plain.iter().map(|r| r.id).collect::<Vec<_>>()
    );
    assert_eq!(
        scored.iter().map(|(r, _)| r.id).collect::<Vec<_>>(),
        plain.iter().map(|r| r.id).collect::<Vec<_>>(),
        "the scored and score-less legs must agree on the ROWS and their order"
    );
    assert_eq!(
        plain.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![first, second],
        "and that order is bm25's, which the wrong score never touched"
    );
}
