//! Test fixture: a TEMPORARY database built from a frozen snapshot of the live
//! graph, so the assertions run against the real 63-entity / 67-edge data
//! without touching `~/.ruagent` (which a live daemon owns).
//!
//! The snapshot (`gold/live_graph_snapshot.json`) was captured read-only on
//! 2026-09-27T13:58Z. Freezing it means a test that passes today keeps meaning
//! tomorrow: the corpus cannot drift under the assertions.
//!
//! `dead_code` is allowed because this module is compiled into EVERY integration
//! test binary, and each binary uses a different subset of it. The alternative
//! (a copy per test file) would guarantee the copies drift.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

use ruagent_store::Db;

pub struct PathmapDb {
    pub db: Db,
    pub ids: HashMap<String, i64>,
    root: PathBuf,
}

// ---------------------------------------------------------------------------
// t30: the cleanup shape t27 introduced for the t16 roots, applied to this fixture.
//
// The text is a COPY, not an import: this module is compiled into seven test binaries of
// this crate, the daemon's copy is private to that crate's unit tests, and the third copy
// lives in mock-agent/tests/e2e_daemon.rs. The shape has three parts and all three matter:
//   1. a BOUNDED retry that RETURNS the reason, so a failure can be reported or asserted;
//   2. a REPORTING call site for a root whose holder is the test process itself;
//   3. a PRUNER that removes, from the NEXT process, what the previous one could not.
// ---------------------------------------------------------------------------

/// Remove a root with a bounded retry, RETURNING the reason (t30).
pub fn try_remove_test_root(
    root: &std::path::Path,
    budget: std::time::Duration,
) -> Result<(), String> {
    let started = std::time::Instant::now();
    loop {
        match std::fs::remove_dir_all(root) {
            Ok(()) => {
                if started.elapsed() > std::time::Duration::from_millis(250) {
                    println!(
                        "READING t30 cleanup of {} waited {:.1} s for the handles to be released",
                        root.display(),
                        started.elapsed().as_secs_f64()
                    );
                }
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                if started.elapsed() >= budget {
                    return Err(format!(
                        "{} could NOT be removed within {:.2} s: {e}",
                        root.display(),
                        budget.as_secs_f64()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }
}

/// Remove a root this fixture OWNS, or PANIC naming it (t30): a cleanup that cannot happen
/// is a red test, not a directory nobody counts.
pub fn remove_test_root(root: &std::path::Path, what: &str, budget: std::time::Duration) {
    if let Err(why) = try_remove_test_root(root, budget) {
        panic!("{what}: {why}");
    }
}

/// Try once, then REPORT -- never silently drop -- a root that is still held (t30).
///
/// This is the call for a root whose holder is the test process itself, where an assertion
/// would red a test for a fact that is not its own.
pub fn report_test_root_left_behind(root: &std::path::Path) {
    if let Err(why) = try_remove_test_root(root, std::time::Duration::from_millis(500)) {
        eprintln!(
            "READING t30 cleanup: {why} -- the root is held by THIS test process, so the \
             failure is REPORTED rather than swallowed; prune_stale_graph_roots removes it \
             in the next run"
        );
    }
}

/// Is a process with this pid alive? (t30)
///
/// WHY NOT THE DAEMON'S PRIMITIVE. ruagent_daemon::orphans::process_started_at_ms is exactly
/// what this wants, but ruagent-daemon DEPENDS on this crate, so a test dev-dependency back
/// at it would invert the dependency direction. The probe is therefore written here: /proc on
/// unix, ONE parsed tasklist call on Windows.
///
/// WHAT IT DOES NOT DO: check the start time. A RECYCLED pid can therefore keep one stale
/// root out of the pruner's reach until that process exits. That is bounded and harmless --
/// from_snapshot_at removes a root before it writes, so a leftover is never INHERITED; the
/// pruner is about the pile, not about correctness.
pub fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return true;
    }
    #[cfg(unix)]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(windows)]
    {
        LIVE_PIDS.get_or_init(live_pids).contains(&pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// The live pid set, asked ONCE per test process: a per-root tasklist spawn would cost
/// seconds on a pile of thousands.
#[cfg(windows)]
static LIVE_PIDS: std::sync::OnceLock<std::collections::HashSet<u32>> = std::sync::OnceLock::new();

/// One tasklist call, parsed. An EMPTY result means the tool could not be read, which the
/// pruner treats as "no oracle" and refuses to prune on -- never delete on a guess.
#[cfg(windows)]
fn live_pids() -> std::collections::HashSet<u32> {
    use std::collections::HashSet;
    let mut out = HashSet::new();
    let Ok(text) = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()
    else {
        return out;
    };
    for line in String::from_utf8_lossy(&text.stdout).lines() {
        // "name","pid","session","#","mem" -- the pid is the second field, and a comma inside
        // the LAST field cannot reach it.
        let mut fields = line.split(',');
        let _name = fields.next();
        if let Some(pid) = fields
            .next()
            .and_then(|f| f.trim_matches('"').parse::<u32>().ok())
        {
            out.insert(pid);
        }
    }
    out
}

/// Remove the roots of DEAD processes this fixture (and its stale-root test) left behind,
/// once per test process (t30).
///
/// The safety rule is t27's: a root whose pid is still ALIVE may be evidence a run needs,
/// and is left alone; a root of a pid that is gone can only be litter.
pub fn prune_stale_graph_roots() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        let me = std::process::id();
        let mut candidates: Vec<(std::path::PathBuf, u32)> = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let rest = name
                .strip_prefix("ruagent-graph-fixture-")
                .or_else(|| name.strip_prefix("ruagent-graph-stale-"));
            let Some(rest) = rest else { continue };
            let Some(pid) = rest.split('-').next().and_then(|p| p.parse::<u32>().ok()) else {
                continue;
            };
            if pid == me {
                continue;
            }
            candidates.push((e.path(), pid));
        }
        if candidates.is_empty() {
            return;
        }
        // NO ORACLE, NO PRUNING: if the liveness question cannot be answered, every root
        // looks dead and a concurrent run's live root would be deleted from under it.
        #[cfg(windows)]
        if LIVE_PIDS.get_or_init(live_pids).is_empty() {
            eprintln!(
                "READING t30 pruning SKIPPED: the live-pid probe returned nothing ({} candidate \
                 root(s) left alone rather than guessed at)",
                candidates.len()
            );
            return;
        }
        let (mut pruned, mut failed) = (0usize, 0usize);
        for (path, pid) in candidates {
            if pid_is_alive(pid) {
                continue;
            }
            match std::fs::remove_dir_all(&path) {
                Ok(()) => pruned += 1,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    failed += 1;
                    eprintln!(
                        "READING t30 stale graph root {} could NOT be removed: {err}",
                        path.display()
                    );
                }
            }
        }
        if pruned > 0 || failed > 0 {
            eprintln!(
                "READING t30 pruned {pruned} stale ruagent-graph-* root(s) of dead test \
                 processes; {failed} could not be removed"
            );
        }
    });
}

impl PathmapDb {
    /// Build a fresh temp DB holding every entity and edge of the snapshot.
    pub async fn live_shaped() -> Self {
        let snapshot = snapshot();
        Self::from_snapshot(&snapshot).await
    }

    /// Entities only, NO edges (G7): the frozen edges are then written by the
    /// code under test (`upsert_fact`) instead of being inserted behind its back,
    /// which is what makes "what would the new write rule have stored?" a
    /// measurement of the REAL code path rather than a re-implementation of it.
    pub async fn live_entities_only() -> Self {
        let mut snapshot = snapshot();
        snapshot.edges.clear();
        Self::from_snapshot(&snapshot).await
    }

    pub async fn from_snapshot(snapshot: &Snapshot) -> Self {
        Self::from_snapshot_at(&Self::next_root(), snapshot).await
    }

    /// The process-unique root the NEXT fixture would use. Exposed so a test can
    /// prove that a root already holding a fixture is emptied rather than
    /// inherited (t42).
    pub fn next_root() -> PathBuf {
        // A process-unique counter, not a timestamp: `Utc::now()` on Windows has
        // ~0.5-15 ms resolution, so two tests in one process can land on the SAME
        // nanosecond tick and then share a database file (UNIQUE constraint on
        // entities.id -- observed, not theorised).
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "ruagent-graph-fixture-{}-{seq}",
            std::process::id()
        ))
    }

    /// Build the snapshot at an EXACT root. The ownership rule lives here, so it
    /// can be tested directly: whatever a previous run left at `root` is removed
    /// before a row is written.
    pub async fn from_snapshot_at(root: &std::path::Path, snapshot: &Snapshot) -> Self {
        // OWN THE DIRECTORY (t42): `{pid}-{seq}` is unique within THIS process,
        // but a pid is recycled by the OS, and this fixture's own `Drop` cannot
        // always clean up -- it runs while `self.db` is still open, so on Windows
        // the locked database file makes `remove_dir_all` fail (measurement: 725
        // `ruagent-graph-fixture-*` directories under %TEMP% when this was
        // written). A leftover directory is not harmless: it still holds the
        // snapshot's rows, so this process's inserts hit `UNIQUE(entities.id)`
        // and a test fails with a message about constraints instead of about the
        // thing it measures. That is the "flaky test" this line removes: a run
        // was seen failing 2 of 5 tests in one target while passing every time
        // alone. Remove before creating, never inherit.
        // t30: the pile this fixture leaves behind is bounded by a pruner the NEXT process
        // runs, because the fixture's own Drop cannot remove a root whose connection is still
        // open (see the Drop below).
        prune_stale_graph_roots();
        let root = root.to_path_buf();
        if root.exists() {
            remove_test_root(
                &root,
                &format!("removing a stale fixture root {root:?} (it must not be inherited)"),
                std::time::Duration::from_millis(2_000),
            );
        }
        std::fs::create_dir_all(&root).unwrap();
        let db = Db::open(root.join("graph.db")).unwrap();
        let mut ids = HashMap::new();
        for e in &snapshot.entities {
            let id = e.id;
            let (name, kind, summary) = (e.name.clone(), e.kind.clone(), e.summary.clone());
            db.call(move |conn| {
                conn.execute(
                    "INSERT INTO entities (id, name, norm_name, kind, summary, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                    rusqlite::params![
                        id,
                        name,
                        name.trim().to_lowercase(),
                        kind,
                        summary,
                        "2026-09-27T13:58:00Z"
                    ],
                )
            })
            .await
            .unwrap()
            .unwrap();
            ids.insert(e.name.clone(), id);
        }
        for e in &snapshot.edges {
            let (id, src, dst) = (e.id, e.src, e.dst);
            let (relation, fact) = (e.relation.clone(), e.fact_text.clone());
            let (valid_at, invalid_at, ets) = (
                e.valid_at.clone(),
                e.invalid_at.clone(),
                e.event_time_source.clone(),
            );
            db.call(move |conn| {
                conn.execute(
                    "INSERT INTO entity_edges
                       (id, src, dst, relation, fact_text, valid_at, invalid_at, created_at,
                        expired_at, source_episode, event_time_source)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?6,?8,NULL,?9)",
                    rusqlite::params![
                        id,
                        src,
                        dst,
                        relation,
                        fact,
                        valid_at,
                        invalid_at,
                        if invalid_at.is_some() {
                            Some("2026-09-27T13:58:00Z")
                        } else {
                            None::<&str>
                        },
                        ets
                    ],
                )
            })
            .await
            .unwrap()
            .unwrap();
        }
        Self { db, ids, root }
    }

    pub async fn entity_id(&self, name: &str) -> i64 {
        if let Some(id) = self.ids.get(name) {
            return *id;
        }
        let name = name.to_string();
        self.db
            .call(move |conn| {
                conn.query_row("SELECT id FROM entities WHERE name = ?1", [&name], |r| {
                    r.get::<_, i64>(0)
                })
            })
            .await
            .unwrap()
            .unwrap()
    }

    pub async fn entities(&self) -> Vec<(i64, String)> {
        self.db
            .call(|conn| {
                let mut stmt = conn.prepare("SELECT id, name FROM entities ORDER BY id")?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .await
            .unwrap()
            .unwrap()
    }
}

impl Drop for PathmapDb {
    fn drop(&mut self) {
        // Best effort ONLY, and knowingly so: `Drop::drop` runs before the fields
        // are dropped, so `self.db` is still open here and Windows refuses to
        // delete a file that a live connection holds. Correctness does not depend
        // on this (t42 makes `from_snapshot` remove a stale root before creating
        // it); this is just tidiness for the cases where the handle is already
        // free. Do not "fix" it by calling into the store: a fixture must not
        // need store-side cooperation to own its own input.
        //
        // t30: the failure is REPORTED instead of swallowed -- a Drop cannot panic (it may be
        // running while another panic unwinds), so the pile is made VISIBLE here and bounded
        // by prune_stale_graph_roots, which the next process runs.
        if let Err(why) = try_remove_test_root(&self.root, std::time::Duration::from_millis(200)) {
            eprintln!(
                "READING t30 cleanup: {why} -- this fixture's root could not be removed from its \
                 own Drop (the connection is still open), so the failure is REPORTED; the next \
                 run's pruner reclaims it"
            );
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct Snapshot {
    pub entities: Vec<SnapEntity>,
    pub edges: Vec<SnapEdge>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SnapEntity {
    pub id: i64,
    pub name: String,
    pub kind: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SnapEdge {
    pub id: i64,
    pub src: i64,
    pub dst: i64,
    pub relation: String,
    pub fact_text: String,
    pub valid_at: String,
    pub invalid_at: Option<String>,
    pub event_time_source: Option<String>,
}

pub fn snapshot() -> Snapshot {
    let raw = include_str!("gold/live_graph_snapshot.json");
    serde_json::from_str(raw).expect("the frozen snapshot parses")
}

pub mod corpus {
    /// Every (id, name) of the frozen live graph.
    pub fn live_entities() -> Vec<(i64, String)> {
        super::snapshot()
            .entities
            .into_iter()
            .map(|e| (e.id, e.name))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Small JSON helpers for the gold sets (no serde derive on test-only types).
// ---------------------------------------------------------------------------

pub fn json_file(rel: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/gold")
        .join(rel);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

pub fn json_array(rel: &str) -> Vec<serde_json::Value> {
    match json_file(rel) {
        serde_json::Value::Array(v) => v,
        other => panic!("{rel} is not a JSON array: {other}"),
    }
}
