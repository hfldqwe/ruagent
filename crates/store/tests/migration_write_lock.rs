//! The migration transaction must ask for its WRITE LOCK UP FRONT.
//!
//! WHY THIS TEST EXISTS (ruagent-close-the-gaps t7). The store configures a
//! 5-second `busy_timeout` (`crates/store/src/sqlite.rs:107`). MEASURED, on a
//! private root with a second process holding `BEGIN IMMEDIATE`:
//!
//! * a single statement acquiring the write lock WAITS — a lock held 1.5-2.5s
//!   made the second writer wait ~1.000-1.005x that and then SUCCEED
//!   (`t7-measure open`, 23/23 rounds, 0 failures); a lock held 6.5s produced a
//!   clean `database is locked` at 5.615s/5.643s/5.652s, i.e. the timeout doing
//!   its job;
//! * a DEFERRED transaction that READS before it writes does NOT wait: it pins a
//!   read snapshot, the later write must UPGRADE, and the upgrade failed in
//!   **17-87 microseconds** with `extended=5` or `SQLITE_BUSY_SNAPSHOT (517)`.
//!   Isolated step by step under a live holder (`t7-measure probe`), every step
//!   of `Db::open` succeeded — `journal_mode=WAL` waited 1.98s and completed,
//!   the ledger read took 25us — and then the transaction's INSERT failed in
//!   54/50/30us with `database is locked`.
//!
//! `apply_one` READ THE LEDGER and only then wrote, so it took the branch the
//! configured timeout cannot absorb, *by construction*. It now begins with
//! `BEGIN IMMEDIATE`, which puts it in the first group: contention is an
//! ordinary wait, and a wait longer than the timeout is a clean failure.
//!
//! WHAT THIS TEST WOULD CATCH, and how it would have caught the old shape: with
//! `conn.transaction()` (rusqlite's default, `BEGIN DEFERRED`) the migration's
//! first statement takes no write lock, so the competing writer below COMMITS
//! during the migration, the migration's snapshot goes stale, and its ledger
//! INSERT fails with `database is locked` — the panic names the real error from
//! SQLite rather than a guessed one. With the write lock taken up front, the
//! competing writer's commit is ordered AFTER the migration's, and the whole
//! thing succeeds. So reverting the one line reddens this test; keeping it green
//! is the behaviour, not the construction.

use std::time::Duration;

use ruagent_store::Db;

/// A root no other test can pick (the store suite's own convention: pid + a
/// counter + the clock, because two parallel tests can share a timestamp).
fn harness_root() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "ruagent-store-migration-lock-{}-{}-{}",
        std::process::id(),
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// The store's own connection configuration (`sqlite.rs:107-110`), so the
/// competing writer is configured exactly like a second store client rather
/// than like a different animal.
fn configure_like_the_store(conn: &rusqlite::Connection) {
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "synchronous", "NORMAL").unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
}

/// The default (current-thread) runtime flavor: `ruagent-store`'s own tokio
/// features are `sync`/`rt`/`macros`, which is all this needs -- the competing
/// writer is a plain OS thread, not a spawned task.
#[tokio::test]
async fn the_migration_transaction_holds_the_write_lock_from_its_start() {
    let dir = harness_root();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ruagent.db");

    // 1. A fully migrated database (HEAD version).
    {
        let db = Db::open(&path).unwrap_or_else(|e| panic!("first open failed: {e}"));
        drop(db); // ordered shutdown: sqlite.rs:72
    }

    // 2. Punch out the MAX ledger row, so the NEXT open genuinely has a migration
    //    to apply -- the whole migration transaction runs, not just the ledger
    //    read. (This is the shape `crates/store/tests/ledger_reopen.rs` uses.)
    let max_script = {
        let conn = rusqlite::Connection::open(&path).unwrap();
        configure_like_the_store(&conn);
        let n = conn
            .execute(
                "DELETE FROM schema_migrations
                  WHERE version = (SELECT MAX(version) FROM schema_migrations)",
                [],
            )
            .unwrap();
        assert_eq!(n, 1, "the MAX ledger row must be gone");
        let max: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(
            max >= 1,
            "the ledger must still have a head below the punched row"
        );
        max
    };

    // 3. The competing writer takes the write lock and holds it across the
    //    migration's whole window. Its own commit lands BEFORE the migration's
    //    write -- which is exactly what makes a DEFERRED migration's snapshot
    //    stale the moment it tries to upgrade.
    //
    //    The channel matters: without it this test is a RACE (the old shape
    //    failed it, and the first version of this test measured nothing because
    //    `Db::open` reached `BEGIN IMMEDIATE` before the holder did). The holder
    //    reports only after its write transaction is OPEN, so the main thread
    //    knows the lock is held before it opens the database.
    let (locked_tx, locked_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let holder_path = path.clone();
    let holder = std::thread::spawn(move || -> Result<(), String> {
        let mut conn = rusqlite::Connection::open(&holder_path).map_err(|e| e.to_string())?;
        configure_like_the_store(&conn);
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        // The write lock is genuinely held from here on.
        let _ = locked_tx.send(Ok(()));
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![500_000_000i64, "t7-competing-writer"],
        )
        .map_err(|e| e.to_string())?;
        std::thread::sleep(Duration::from_millis(400));
        tx.commit().map_err(|e| e.to_string())
    });

    // Do not open the database until the competing writer holds the lock.
    locked_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the competing writer must report that it holds the write lock")
        .expect("the competing writer must acquire the write lock");

    // 4. Open the database while the lock is held: the migration must WAIT for
    //    it and then complete, not fail on the upgrade.
    let started = std::time::Instant::now();
    let opened = Db::open(&path);
    let waited = started.elapsed();

    let holder_result = holder.join().expect("the holder thread must not panic");
    holder_result.expect("the competing writer must commit");

    // The migration's write had to wait for the holder (it cannot have beaten a
    // transaction that was already open), so the elapsed time proves the write
    // lock was waited on rather than bypassed. Note the ORDER of the two checks:
    // `opened` is judged FIRST, because the deferred shape's signature reading is
    // an immediate failure, and only then this guard stops the test from passing
    // vacuously.
    let db = opened.unwrap_or_else(|e| {
        panic!(
            "Db::open failed while a competing writer held the lock for 400ms (it returned in \
             {waited:?}): {e}. The migration transaction took the read-then-write UPGRADE branch, \
             which the 5s busy_timeout cannot absorb (measured: a deferred upgrade fails in \
             17-87us). Check that `apply_one` still begins with \
             `transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)`."
        )
    });

    assert!(
        waited >= Duration::from_millis(300),
        "the migration returned in {waited:?}, which is inside the 400ms hold: the write lock \
         cannot have been acquired through contention, so this test measured nothing"
    );

    // 5. The migration COMMITTED WHILE THE LOCK WAS HELD, not before it or after
    //    it: the punched row is back. If the migration had come back early
    //    without applying (the silent half of the old failure mode), this fails.
    db.call_flat({
        let v = max_script;
        move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
                rusqlite::params![v],
                |r| r.get::<_, i64>(0),
            )
        }
    })
    .await
    .unwrap_or_else(|e| panic!("ledger read failed after the migration: {e}"))
    .eq(&1)
    .then_some(())
    .expect(
        "the punched migration did not commit while the lock was held -- the migration \
         either ran outside the lock window or returned without applying",
    );

    // 6. The re-applied migration's schema object is present (0026 is the head
    //    migration: `knowledge_graph_ingest`).
    let table_present: i64 = db
        .call_flat(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master
                  WHERE type = 'table' AND name = 'knowledge_graph_ingest'",
                [],
                |r| r.get(0),
            )
        })
        .await
        .expect("schema read after the migration");
    assert_eq!(table_present, 1, "the head migration's table must exist");

    // 7. The competing writer's row survived too: its commit happened, and the
    //    migration did not roll it back.
    let competing: i64 = db
        .call_flat(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = 500000000",
                [],
                |r| r.get(0),
            )
        })
        .await
        .expect("ledger read for the competing row");
    assert_eq!(
        competing, 1,
        "the competing writer's committed row must still be there"
    );

    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}
