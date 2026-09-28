//! V-A F-1 / t29: the gold seed must be idempotent.
//!
//! THE DEFECT THIS PINS. `common::seed` inserts with `INSERT OR IGNORE`, and
//! `INSERT OR IGNORE` only ignores a conflict that a UNIQUE constraint defines.
//! `query_eval_gold` had none on `(set_id, query)` (0019), so every run of an
//! instrument that seeds appended the whole 22-query set again: after six runs
//! the table held 132 rows for a 22-query set. V-A measured exactly that
//! ((132, 90)); this test reproduces it with the real seed code and then proves
//! the fix, so the pair can be re-taken at any time.
//!
//! THE SINGLE VARIABLE. The two sides differ by ONE object: the unique index
//! 0025 adds. The "pre" side drops it by name (rather than re-declaring 0019's
//! CREATE TABLE, which would be a reconstruction of history and could drift from
//! it), the "post" side keeps it. Same seed code, same data, same 6 runs.
//!
//! This is not a metric: the harness's own metrics read the frozen `GOLD`
//! constant, not the table. What the table's size decides is a JUDGEMENT
//! PREMISE -- next generation's C3 needs "at least 60 queries, at least 20
//! unanswerable", and reading that off `COUNT(*)` on a duplicate-inflated table
//! is how a generation awards itself a target it never met.

use std::path::{Path, PathBuf};

mod common;

/// A scratch root under the system temp directory, wiped first so a re-run of
/// this test starts from the same state (the same "a reading that cannot be
/// re-taken is not a reading" rule the t25 instrument had to be fixed for).
fn scratch_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ra-t29-{}", name));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap_or_else(|e| panic!("cannot wipe {root:?}: {e}"));
        println!("[t29] wiped the previous scratch root {root:?}");
    }
    std::fs::create_dir_all(root.join("data")).unwrap();
    root
}

async fn open(root: &Path) -> ruagent_store::Db {
    ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap()
}

/// t30: the drift JUDGE, and its falsifiability pair.
///
/// t29 left the drift as a print: `[gold] drift: N of 22 …` could say anything
/// and every criterion that reads the frozen constants stayed green -- while the
/// next generation's C3 premise ("at least 60 queries, at least 20 unanswerable")
/// is read FROM THE TABLE. A note that cannot fail is not a criterion.
///
/// The pair below uses ONE constructed drift and ONE checker; the single variable
/// is whether the report has a consumer:
///   * report-only (the pre-t30 shape): the same disagreement is returned and
///     printed, nothing judges it → the run stays green;
///   * judged (this revision): the same report reaches `into_result()` →
///     `Err`, naming the row.
#[tokio::test]
async fn a_seed_owned_drift_fails_only_because_the_report_has_a_consumer() {
    let db = open(&scratch_root("drift")).await;
    let (set_id, written) = common::seed(&db).await;
    println!("[t30] seeded set_id={set_id} written={written}");

    // A clean table is green, and that must be measured before the drift is
    // constructed -- otherwise "it is red" would not distinguish anything.
    let clean = common::db_drift_check(&db, set_id).await;
    assert!(
        clean.seed_owned.is_empty() && clean.absent.is_empty(),
        "a freshly seeded table must agree with the frozen constants: {:?}",
        clean.seed_owned
    );
    assert!(clean.into_result().is_ok(), "clean table => ok");

    // Construct ONE seed-owned drift: a row the seed wrote, with a class the
    // frozen constants do not have. (`judged_by` stays 'title-derived', i.e. this
    // row is the seed's, not a human's.)
    let query = common::GOLD[0].0.to_string();
    let for_update = query.clone();
    let changed = db
        .call(move |conn| -> Result<usize, rusqlite::Error> {
            conn.execute(
                "UPDATE query_eval_gold SET class = 'wrong-class', answerable = 0
                 WHERE set_id = ?1 AND query = ?2 AND judged_by <> 'human'",
                rusqlite::params![set_id, for_update],
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(changed, 1, "the constructed drift must hit exactly one row");

    // (a) THE PRE-t30 SHAPE: ask, print, judge nothing. Same report, no consumer.
    let reported = common::db_drift_check(&db, set_id).await;
    println!(
        "[t30] PRE-t30 SHAPE (report only): the checker found {} seed-owned disagreement(s) and \
         nothing consumed the report -> this run is not failed by it. rows={:?}",
        reported.seed_owned.len(),
        reported.seed_owned
    );
    assert_eq!(
        reported.seed_owned.len(),
        1,
        "the checker must SEE the constructed drift even in the report-only shape"
    );

    // (b) THE JUDGE: the same report, consumed.
    let verdict = common::db_drift_check(&db, set_id).await.into_result();
    let err = verdict.err().expect(
        "the same drift must FAIL once the report is judged -- this is the whole of t30 item 2",
    );
    println!("[t30] JUDGED (this revision): verdict=Err -> {err}");
    // The row must be NAMED. (`contains(&query)`, not `contains("{query:?}")`: the
    // row list is rendered with Debug, which escapes the quotes around a name, so
    // asserting the quoted form would fail for a reason that has nothing to do
    // with the drift.)
    assert!(
        err.contains(&query),
        "the failure must NAME the row, not just count it: {err}"
    );
    assert!(
        err.contains("wrong-class"),
        "the failure must show what the table holds vs what is frozen: {err}"
    );

    // (c) The instrument's own entry point fails too: `seed_checked` seeds (which
    //     is a no-op here) and then judges, so every instrument goes red.
    let via_seed = common::seed_checked(&db).await;
    assert!(
        via_seed.is_err(),
        "seed_checked must surface the drift, got {via_seed:?}"
    );
    println!("[t30] seed_checked -> Err(..) as required");

    // (d) A HUMAN row is allowed to differ, is counted separately, and is never
    //     overwritten by a later seed. This is the other half of the ruling: the
    //     judge must not make human labels fail the suite.
    let human_query = common::GOLD[1].0.to_string();
    let for_mark = human_query.clone();
    let marked = db
        .call(move |conn| -> Result<usize, rusqlite::Error> {
            conn.execute(
                "UPDATE query_eval_gold SET class = 'human-says-something-else',
                                            judged_by = 'human', note = 'graded by a person'
                 WHERE set_id = ?1 AND query = ?2",
                rusqlite::params![set_id, for_mark],
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(marked, 1);

    // Clear the seed-owned drift so the ONLY disagreement left is the human one.
    let restored = db
        .call(move |conn| -> Result<usize, rusqlite::Error> {
            conn.execute(
                "UPDATE query_eval_gold SET class = ?3, answerable = 1
                 WHERE set_id = ?1 AND query = ?2 AND judged_by <> 'human'",
                rusqlite::params![set_id, query, common::GOLD[0].2],
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restored, 1);

    let human_only = common::db_drift_check(&db, set_id).await;
    assert_eq!(
        human_only.human.len(),
        1,
        "the human divergence must be counted separately: {:?}",
        human_only.human
    );
    assert!(
        human_only.into_result().is_ok(),
        "a HUMAN label disagreeing is not a failure -- the frozen constants describe the seed's \
         own rows, and overwriting a person's grading is the one thing this seed must not do"
    );

    // And a later seed leaves that human row exactly as the person left it.
    common::seed(&db).await;
    let kept: (String, String) = db
        .call(move |conn| -> Result<(String, String), rusqlite::Error> {
            conn.query_row(
                "SELECT class, note FROM query_eval_gold WHERE set_id = ?1 AND query = ?2",
                rusqlite::params![set_id, human_query],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .await
        .unwrap()
        .unwrap();
    println!("[t30] after re-seeding, the human row still holds {kept:?}");
    assert_eq!(
        kept,
        (
            "human-says-something-else".to_string(),
            "graded by a person".to_string()
        ),
        "a human label must survive every re-seed: this is WHY the seed does not upsert"
    );
}

/// `COUNT(*)`, `SUM(answerable)`, the deduplicated size, and how many duplicate
/// groups exist -- the four numbers that decide whether the table can be read as
/// a set.
async fn shape(db: &ruagent_store::Db) -> (i64, i64, i64, i64) {
    db.call(|conn| -> Result<(i64, i64, i64, i64), rusqlite::Error> {
        Ok((
            conn.query_row("SELECT COUNT(*) FROM query_eval_gold", [], |r| r.get(0))?,
            conn.query_row(
                "SELECT COALESCE(SUM(answerable), 0) FROM query_eval_gold",
                [],
                |r| r.get(0),
            )?,
            conn.query_row(
                "SELECT COUNT(*) FROM (SELECT DISTINCT set_id, query FROM query_eval_gold)",
                [],
                |r| r.get(0),
            )?,
            conn.query_row(
                "SELECT COUNT(*) FROM (SELECT set_id, query FROM query_eval_gold
                                       GROUP BY 1, 2 HAVING COUNT(*) > 1)",
                [],
                |r| r.get(0),
            )?,
        ))
    })
    .await
    .unwrap()
    .unwrap()
}

async fn run_six(db: &ruagent_store::Db, side: &str) -> Vec<(i64, i64)> {
    let mut per_run = Vec::new();
    for run in 1..=6 {
        common::seed(db).await;
        let (rows, answerable, distinct, dup_groups) = shape(db).await;
        println!(
            "[t29] {side} run {run}: COUNT(*)={rows} SUM(answerable)={answerable} \
             distinct(set_id,query)={distinct} duplicate_groups={dup_groups}"
        );
        per_run.push((rows, answerable));
    }
    per_run
}

#[tokio::test]
async fn the_gold_seed_is_idempotent_only_because_0025_makes_it_so() {
    let set_size = (common::GOLD.len() + common::NO_ANSWER.len()) as i64;
    let answerable_per_run = common::GOLD.len() as i64;
    println!(
        "[t29] frozen set: {set_size} queries, {answerable_per_run} answerable, \
         {} unanswerable",
        common::NO_ANSWER.len()
    );

    // ---- PRE: the schema as 0019 left it. The only difference from POST is the
    //      object 0025 adds, dropped here by name.
    let pre = open(&scratch_root("pre")).await;
    let dropped: i64 = pre
        .call(|conn| -> Result<i64, rusqlite::Error> {
            let before: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' \
                 AND name='ux_query_eval_gold_set_query'",
                [],
                |r| r.get(0),
            )?;
            conn.execute("DROP INDEX IF EXISTS ux_query_eval_gold_set_query", [])?;
            Ok(before)
        })
        .await
        .unwrap()
        .unwrap();
    println!(
        "[t29] PRE side: dropped {} unique index(es) -- {}",
        dropped,
        if dropped == 0 {
            "none existed, so this side IS the pre-0025 schema".to_string()
        } else {
            "0025's only object, so this side reproduces the pre-0025 schema".to_string()
        }
    );
    let pre_sequence = run_six(&pre, "PRE").await;

    // ---- POST: as migrated.
    let post = open(&scratch_root("post")).await;
    let post_sequence = run_six(&post, "POST").await;

    println!(
        "[t29] PRE  sequence COUNT(*): {:?}",
        pre_sequence.iter().map(|(r, _)| *r).collect::<Vec<_>>()
    );
    println!(
        "[t29] POST sequence COUNT(*): {:?}",
        post_sequence.iter().map(|(r, _)| *r).collect::<Vec<_>>()
    );

    // The defect, reproduced on the real seed code and the real pre-0025 schema.
    // If this ever stops holding, the premise of 0025's unique index has changed
    // and the message says so rather than blaming the fix.
    assert_eq!(
        pre_sequence,
        (1..=6)
            .map(|run| (run * set_size, run * answerable_per_run))
            .collect::<Vec<_>>(),
        "the PRE side must show the F-1 defect: every run appends the whole set \
         (`INSERT OR IGNORE` has no constraint to ignore against)"
    );

    // The fix.
    assert_eq!(
        post_sequence,
        (1..=6)
            .map(|_| (set_size, answerable_per_run))
            .collect::<Vec<_>>(),
        "six runs of the same seed must leave the set at its own size"
    );

    // The two readings a downstream premise can be taken from, on the table that
    // still carries the duplicates. They are NOT equal, and that gap is the
    // entrance to a false pass.
    let (rows, answerable, distinct, dup_groups) = shape(&pre).await;
    println!(
        "[t29] PRE side BY TABLE ROWS: COUNT(*)={rows} SUM(answerable)={answerable} \
         -> would read as a {rows}-query set with {answerable} answerable"
    );
    println!(
        "[t29] PRE side AFTER DEDUPLICATION: {distinct} queries in {dup_groups} duplicate groups \
         -> the set that was actually frozen"
    );
    assert_ne!(
        rows, distinct,
        "the whole point is that these two numbers differ on an inflated table"
    );

    // And the statement a human would run to clean such a table. It is NOT run
    // here against anything real (this database is a throwaway scratch root),
    // and it is written into the report for the reader who has one.
    println!(
        "[t29] cleanup (written into the report, not executed against any real db):\n\
         DELETE FROM query_eval_gold WHERE id NOT IN (\n\
         \x20   SELECT MIN(id) FROM query_eval_gold GROUP BY set_id, query);"
    );
}

/// The seed WRITES, so it must not be able to write the live root. The decision
/// is a pure function of two paths, so it is testable without going near the
/// user's database.
#[test]
fn the_seed_guard_tells_a_copy_from_the_live_root() {
    let live = PathBuf::from(r"C:\Users\x\.ruagent");
    for (path, inside, note) in [
        (
            r"C:\Users\x\.ruagent\data\ruagent.db",
            true,
            "the live database itself",
        ),
        (
            r"C:\Users\x\.ruagent\data\ra-t29.db",
            true,
            "any file under the live root, not just the one name",
        ),
        (
            r"C:\Users\x\.ruagent-other\data\ruagent.db",
            false,
            "a SIBLING directory: component-wise containment must not confuse it",
        ),
        (
            r"C:\Users\x\AppData\Local\Temp\ra-t29-post\data\ruagent.db",
            false,
            "the copy this instrument uses",
        ),
    ] {
        assert_eq!(
            common::is_inside(Path::new(path), &live),
            inside,
            "{note}: {path}"
        );
    }
}
