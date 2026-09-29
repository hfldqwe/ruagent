//! t98: the migration LEDGER, at the level the daemon actually boots through.
//!
//! `ruagent serve` opens the database with [`ruagent_store::Db::open`], and that
//! call is where `migrations::apply` runs. These tests therefore exercise the
//! acceptance reading -- "a lost version row must not stop the boot; a hole in
//! the ledger must stop it BY NAME" -- without a daemon process, on a private
//! temp root, and they leave the ledger's three interesting states behind as
//! assertions rather than as a transcript.
//!
//! Everything here is a COPY: the tests build their own database under
//! `%TEMP%` and delete it at the end.

use ruagent_store::Db;
use ruagent_store::migrations::SCHEMA_VERSION;

/// A private root under `%TEMP%`; removed on drop.
struct Root(std::path::PathBuf);

impl Root {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ruagent-t98-{tag}-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("wipe a previous t98 root");
        }
        std::fs::create_dir_all(dir.join("data")).expect("create the t98 root");
        Root(dir)
    }

    fn db_path(&self) -> std::path::PathBuf {
        self.0.join("data").join("ruagent.db")
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        // The writer thread closes the file a moment after the last `Db` handle
        // drops, and Windows refuses to remove a directory with an open file, so
        // retry briefly: a self-cleaning test that leaks its root is the residue
        // this crate has been asked about before.
        for _ in 0..20 {
            match std::fs::remove_dir_all(&self.0) {
                Ok(()) => return,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
    }
}

async fn versions(db: &Db) -> Vec<i64> {
    let mut rows = db
        .call_flat(|conn| {
            let mut stmt =
                conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
            let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .await
        .expect("read the ledger");
    rows.sort_unstable();
    rows
}

async fn punch(db: &Db, sql: &str) {
    let sql = sql.to_string();
    db.call_flat(move |conn| {
        conn.execute(&sql, [])?;
        Ok(())
    })
    .await
    .expect("punch the ledger");
}

/// The S1 acceptance reading: with the HIGHEST version row gone the ledger is
/// still contiguous (1..24), so a boot must re-apply migration 25 and succeed.
/// Before t98 this was the case that left the daemon unable to start at all
/// (`index ux_query_eval_gold_set_query already exists`).
#[tokio::test]
async fn a_lost_maximum_version_row_still_boots() {
    let root = Root::new("max");
    let path = root.db_path();

    let db = Db::open(&path).expect("fresh database opens and migrates");
    assert_eq!(
        versions(&db).await,
        (1..=SCHEMA_VERSION).collect::<Vec<_>>(),
        "a fresh database records every migration"
    );
    punch(
        &db,
        "DELETE FROM schema_migrations WHERE version = (SELECT MAX(version) FROM schema_migrations)",
    )
    .await;
    drop(db);

    // the daemon's own boot path, on the damaged ledger
    let reopened = Db::open(&path).expect("a lost MAXIMUM row must not stop the boot (t98/S1)");
    assert_eq!(
        versions(&reopened).await,
        (1..=SCHEMA_VERSION).collect::<Vec<_>>(),
        "the re-applied migration must be recorded again"
    );
    drop(reopened);
}

/// The S2 acceptance reading, negative control included: a HOLE in the middle
/// must refuse the boot and NAME the missing version -- never skip it silently,
/// which is what `MAX(version)` used to do.
#[tokio::test]
async fn a_middle_hole_refuses_the_boot_by_name() {
    let root = Root::new("hole");
    let path = root.db_path();

    let db = Db::open(&path).expect("fresh database opens and migrates");
    punch(&db, "DELETE FROM schema_migrations WHERE version = 3").await;

    // NEGATIVE: the damaged ledger must be red, and say which version is missing.
    // (A second connection to the same file is how the next boot sees it.)
    let err = match Db::open(&path) {
        Ok(_) => panic!("a hole must refuse the boot (t98/S2)"),
        Err(e) => e,
    };
    let text = err.to_string();
    assert!(
        text.contains('3') && text.contains("contiguous"),
        "the refusal must name version 3 and call the ledger non-contiguous, got: {text}"
    );

    // RECOVERY: restore the row; the boot is green again.
    punch(
        &db,
        "INSERT INTO schema_migrations (version, applied_at) VALUES (3, 't98-repair')",
    )
    .await;
    let reopened = Db::open(&path).expect("a repaired ledger boots again");
    assert_eq!(
        versions(&reopened).await,
        (1..=SCHEMA_VERSION).collect::<Vec<_>>()
    );
    drop(reopened);
    drop(db);
}

/// A database migrated by a NEWER binary is a superset, not a hole: it must still
/// boot (the downgrade path), and the newer version must not be silently erased.
#[tokio::test]
async fn a_future_version_is_not_a_hole() {
    let root = Root::new("future");
    let path = root.db_path();

    let db = Db::open(&path).expect("fresh database opens and migrates");
    punch(
        &db,
        "INSERT INTO schema_migrations (version, applied_at) VALUES (999, 't98-future')",
    )
    .await;

    let reopened = Db::open(&path).expect("a future version must not stop this binary");
    let mut expected = (1..=SCHEMA_VERSION).collect::<Vec<_>>();
    expected.push(999);
    assert_eq!(versions(&reopened).await, expected);
    drop(reopened);
    drop(db);
}
