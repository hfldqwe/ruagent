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
use std::sync::Mutex;

/// Which TAGS have leaked a root in this process (t173). `Drop` must not panic
/// (a panic while the thread unwinds aborts the whole test binary -- t167
/// re-checked that), so a leak is recorded here and the tests assert their OWN
/// tag never appears. Keyed by tag, not by a global count, so a test that leaks
/// ON PURPOSE (the marker test below) cannot redden a test running beside it.
static LEAKED_TAGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A marker OUTSIDE the directory that could not be deleted: the in-place one is
/// only findable by looking at the very directory you are failing to remove, and
/// a passing test's `eprintln!` is captured by the harness.
fn leaked_roots_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("ruagent-leaked-roots")
}

fn leaked_tags() -> Vec<String> {
    match LEAKED_TAGS.lock() {
        Ok(t) => t.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// Called at the END of a test that owns a root (after the root is dropped):
/// a leak makes THIS TEST RED, which is the whole point -- before t173 a leak
/// left exit code 0 as long as every other assertion passed.
fn assert_own_root_did_not_leak(tag: &str, what: &str) {
    let leaked = leaked_tags();
    assert!(
        !leaked.contains(&tag.to_string()),
        "the test root `{tag}` leaked during {what}: a root that cannot be removed must \
         fail its own test, not only leave a marker (leaked tags: {leaked:?})"
    );
}

/// A private root under `%TEMP%`; removed on drop.
struct Root(std::path::PathBuf, String);

impl Root {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ruagent-t98-{tag}-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("wipe a previous t98 root");
        }
        std::fs::create_dir_all(dir.join("data")).expect("create the t98 root");
        Root(dir, tag.to_string())
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
        let mut last: Option<std::io::Error> = None;
        for _ in 0..20 {
            match std::fs::remove_dir_all(&self.0) {
                Ok(()) => return,
                Err(e) => {
                    last = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        // A TRACE, NOT A SILENCE (t157 B3, fixed in t160). Giving up quietly here
        // is exactly how this crate earned its "leaks its root" reputation: the
        // residue was real and nothing ever said so. Twenty attempts is ~1s; the
        // bound is a ceiling on "the handle closes a moment after the drop", not a
        // judgement about correctness.
        //
        // WHY NOT `panic!`: this runs in `Drop`. A panic while the thread is
        // ALREADY unwinding from a failed assertion aborts the whole test binary,
        // so one leaked root would take every other test in the process down with
        // it -- a diagnosable residue turned into an undiagnosable crash. It would
        // also fail an otherwise-passing test for a reason the test is not about.
        // The durable marker below survives the process, which a swallowed
        // `eprintln!` from a PASSING test does not (the harness captures it).
        let err = last
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown (no error recorded)".to_string());
        let marker = self.0.join("LEAKED-ROOT.txt");
        let _ = std::fs::write(
            &marker,
            format!(
                "t98: this test root could not be removed after 20 attempts (~1s).\n\
                 root: {}\n\
                 last error: {err}\n\
                 This marker IS the fix: the residue is diagnosable instead of silent \
                 (t157 B3 / t160). Delete the directory once no test is running.\n",
                self.0.display()
            ),
        );
        eprintln!(
            "t98 LEAKED ROOT after 20 attempts: {} (last error: {err}); marker: {}",
            self.0.display(),
            marker.display()
        );
        // VISIBLE BEYOND THE ROOT (t173): the marker above lives INSIDE the
        // directory that could not be deleted, so on its own a leak still leaves
        // exit code 0 (a passing test's stderr is captured by the harness). Two
        // additions fix that without panicking here: a durable copy in a STABLE
        // location, and this root's TAG on a list its owning test asserts
        // against -- which is what turns "a leak happened" into "a test went red".
        if let Ok(mut tags) = LEAKED_TAGS.lock() {
            tags.push(self.1.clone());
        }
        let dir = leaked_roots_dir();
        let _ = std::fs::create_dir_all(&dir);
        let name = self
            .0
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown-root".to_string());
        let _ = std::fs::write(
            dir.join(format!("{name}.txt")),
            format!(
                "t98/t173: this test root could not be removed after 20 attempts (~1s).\n\
                 root: {}\n\
                 last error: {err}\n\
                 in-place marker: {}\n\
                 Delete the directory once no test is running.\n",
                self.0.display(),
                marker.display()
            ),
        );
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
/// still contiguous (1..=SCHEMA_VERSION - 1, i.e. 1..=25 while the highest
/// migration is 26), so a boot must re-apply that highest migration and succeed.
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
    drop(root);
    assert_own_root_did_not_leak("max", "a_lost_maximum_version_row_still_boots");
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
    drop(root);
    assert_own_root_did_not_leak("hole", "a_middle_hole_refuses_the_boot_by_name");
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
    drop(root);
    assert_own_root_did_not_leak("future", "a_future_version_is_not_a_hole");
}

/// The residue's OWN test (t160): when the root cannot be removed, `Drop` must
/// leave a durable marker instead of silence.
///
/// WHY THIS IS A TEST AND NOT A COMMENT: the claim above ("a trace, not a
/// silence") is only worth something if it can go red. The deterministic way to
/// make `remove_dir_all` fail on Windows is to hold a file inside the root open
/// with `share_mode(0)` (an EXCLUSIVE open) -- that is a real sharing violation,
/// not a sleep, so the failure is guaranteed rather than observed.
///
/// WINDOWS-ONLY, and that is honest rather than convenient: on Unix an open file
/// does not stop `remove_dir_all`, so this cannot be pressed there. The marker
/// path itself is exercised on both platforms (every other test in this file
/// runs it on the success branch); only the failure branch is Windows-only.
#[cfg(windows)]
#[test]
fn a_root_that_cannot_be_removed_leaves_a_marker_instead_of_silence() {
    use std::os::windows::fs::OpenOptionsExt;

    let root = Root::new("leak-marker");
    let path = root.0.clone();
    let held = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .share_mode(0) // EXCLUSIVE: any attempt to remove the dir must fail
        .open(path.join("held-open"))
        .expect("hold a file open inside the root");

    drop(root); // 20 retries, ~1s, all of them must fail

    let marker = path.join("LEAKED-ROOT.txt");
    assert!(
        marker.exists(),
        "a root that could not be removed must leave a durable marker, or the leak \
         is silent again (looked for {})",
        marker.display()
    );
    let text = std::fs::read_to_string(&marker).expect("the marker is readable");
    assert!(
        text.contains("could not be removed") && text.contains("last error"),
        "the marker must name the failure, got: {text}"
    );

    // t173: the leak is now VISIBLE and FAIL-ABLE, not only written down.
    assert!(
        leaked_tags().contains(&"leak-marker".to_string()),
        "the leak must be recorded by tag, or the guard below cannot see it"
    );
    let stable = leaked_roots_dir().join(format!(
        "{}.txt",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown-root".to_string())
    ));
    let stable_text = std::fs::read_to_string(&stable).expect("the stable marker is readable");
    assert!(
        stable_text.contains("could not be removed")
            && stable_text.contains(&path.display().to_string()),
        "the stable marker must name the failure AND the root, got: {stable_text}"
    );
    // The guard the OTHER tests run must redden for exactly this condition. It is
    // caught here so this (intentionally leaking) test still passes; the point is
    // that "visible" is not cosmetic -- the assertion really does fail.
    let reddened =
        std::panic::catch_unwind(|| assert_own_root_did_not_leak("leak-marker", "the t173 probe"))
            .is_err();
    assert!(
        reddened,
        "the leak guard must redden when a root leaked, or the leak is still only cosmetic"
    );

    drop(held); // release the handle, then clean up by exact path
    let _ = std::fs::remove_file(&stable);
    let _ = std::fs::remove_dir(leaked_roots_dir());
    let _ = std::fs::remove_dir_all(&path);
}
