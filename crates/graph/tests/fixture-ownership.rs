//! t42 regression: **an instrument that writes its input must own its input.**
//!
//! The live instruments (`live-after.rs`) own their copies. The FIXTURE is the
//! other instrument in this crate that writes its input: it creates a temp
//! directory, opens a database in it and inserts the whole snapshot. Its root
//! name is `{pid}-{seq}`, and `{pid}` is recycled by the OS -- so the directory a
//! previous run left behind can be handed to a new process. `Drop` cannot be
//! relied on to prevent that (it runs while the connection is still open, so on
//! Windows the delete fails), and it had not: 725 `ruagent-graph-fixture-*`
//! directories were counted under %TEMP% when this test was written.
//!
//! A stale root is not a cosmetic problem. It already holds the snapshot's
//! entities at ids 1..63, so a rebuild INSERTS over them and dies on
//! `UNIQUE(entities.id)` -- observed once as `3 passed; 2 failed` in a target
//! that passes every time when run alone, i.e. the classic "flaky" reading that
//! is really a test reading another run's data.
//!
//! This test makes the rule falsifiable: it leaves a poisoned root behind and
//! requires the next build at that exact root to contain the snapshot ONCE.

mod fixture;

use ruagent_store::Db;

async fn count(db: &Db, sql: &'static str) -> i64 {
    db.call_flat(move |conn| conn.query_row(sql, [], |r| r.get(0)))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_stale_fixture_root_is_emptied_not_inherited() {
    let snap = fixture::snapshot();
    let expected = snap.entities.len() as i64;
    let root = std::env::temp_dir().join(format!("ruagent-graph-stale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join("graph.db");

    // 1. The leftover a recycled pid finds: the directory and a `graph.db` are
    //    there, nothing holds them, and the bytes are NOT a database this fixture
    //    may inherit. A scope with its own `Db::open` was the first attempt and is
    //    NOT usable: the store's handle does not release the file when it is
    //    dropped (measured here as `os error 32`, another process is using the
    //    file) -- which is precisely why leftovers pile up in the first place.
    std::fs::write(&db_path, b"t42-stale-root-not-a-database").unwrap();
    std::fs::write(root.join("graph.db-wal"), b"t42-stale-wal").unwrap();
    assert!(db_path.exists(), "the leftover is the premise of this test");

    // 2. The next build at the SAME root must not inherit it: the directory is
    //    removed before a row is written, so the stale bytes cannot even be
    //    opened, and the snapshot lands exactly once. Without the removal this
    //    test fails loudly instead of quietly: `Db::open` on those bytes is
    //    "file is not a database" -- the same class of confusing red as the
    //    measured flake (which failed on UNIQUE(entities.id) when the leftover
    //    was a real snapshot).
    let second = fixture::PathmapDb::from_snapshot_at(&root, &snap).await;
    let rows = count(&second.db, "SELECT COUNT(*) FROM entities").await;
    let inherited = count(
        &second.db,
        "SELECT COUNT(*) FROM entities WHERE name LIKE 't42-%'",
    )
    .await;
    let bytes = std::fs::metadata(&db_path).unwrap().len();
    println!(
        "t42 fixture ownership: stale root rebuilt -> entities {rows} (expected {expected}), inherited rows {inherited} (expected 0), db size {bytes} bytes (the stale 28-byte file is gone)"
    );
    assert_eq!(
        inherited, 0,
        "the stale root was inherited: a foreign row survived"
    );
    assert!(
        bytes > 1000,
        "the database at this path is still the stale {bytes}-byte file, not a rebuilt one"
    );
    assert_eq!(
        rows, expected,
        "the rebuild must carry the snapshot exactly once, not the old rows plus the new ones"
    );
}
