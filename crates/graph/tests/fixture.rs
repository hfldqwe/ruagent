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
        let root = root.to_path_buf();
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap_or_else(|e| {
                panic!("removing a stale fixture root {root:?} (it must not be inherited): {e}")
            });
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
        let _ = std::fs::remove_dir_all(&self.root);
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
