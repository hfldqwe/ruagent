//! The "after" readings on a COPY of the live graph.
//!
//! WHY a copy, and why `#[ignore]`:
//!   * `~/.ruagent/data/ruagent.db` is owned by a running daemon (pid 79984), and
//!     a test that writes to it would be a data-loss-shaped mistake;
//!   * the copy also picks up migrations 0019-0023 that the live file does not
//!     have yet, which is exactly the state a future daemon restart creates;
//!   * a test that reads a user's private database must never run by accident, so
//!     it is opt-in and names the file it was given.
//!
//! ONE INSTRUMENT, ONE COPY (t42). Rule, measured not reasoned: **an instrument
//! that writes its input must own its input.** The three instruments below share
//! one SOURCE copy (the file `RUAGENT_GRAPH_LIVE_COPY` points at), but each one
//! `VACUUM INTO`s its own target and opens THAT.
//!
//! What went wrong before: all three opened the source directly, and `Db::open`
//! applies migrations -- a write. Two of the three also write rows (merges;
//! communities). Running all three at once therefore had two of them die inside
//! `Db::open` on a file another test was migrating: measured `1 passed; 2 failed`
//! with `-- --include-ignored`, while each passed alone -- so "run every live
//! reading at once" was impossible. Now each instrument owns its copy, removes a
//! stale target first (`VACUUM INTO` refuses an existing output file), and
//! ASSERTS its own copy (the source's object set before migrations + a private
//! marker row the source does not have). The source is opened READ-ONLY for the
//! copy, so it is never modified and no instrument can see another's writes.
//!
//! RUN (one source, three private copies; the source must itself be a copy):
//!   python -c "import sqlite3,os;d=os.path.join(os.environ['TEMP'],'ruagent-live.db');os.path.exists(d) and os.remove(d);c=sqlite3.connect(os.path.join(os.path.expanduser('~'),'.ruagent','data','ruagent.db'));c.execute(\"VACUUM INTO '%s'\"%d.replace('\\\\','/'));c.close()"
//!   $env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\ruagent-live.db"
//!   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph -- --include-ignored --nocapture

use ruagent_graph as g;
use ruagent_store::Db;

mod fixture;

/// The one file every instrument copies FROM. It is never opened read-write.
fn source_path() -> String {
    let path = std::env::var("RUAGENT_GRAPH_LIVE_COPY").expect(
        "set RUAGENT_GRAPH_LIVE_COPY to a COPY of ~/.ruagent/data/ruagent.db (never the live file)",
    );
    assert!(
        !path.to_lowercase().contains(".ruagent"),
        "refusing to open the live database: point this at a copy"
    );
    path
}

/// A READ-ONLY connection to the source. `VACUUM INTO` only reads its source, and
/// this makes that a property of the connection instead of a promise in a comment
/// (a read-write handle here is how the old shape let one test migrate the file
/// the others were reading).
fn open_source_readonly(path: &str) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap_or_else(|e| panic!("opening {path:?} read-only: {e}"))
}

/// The object set of a graph database: "is this the copy I think it is?".
///
/// Every field is `Option`: a table that the source does not have yet (the live
/// file is behind the crate's migrations, which is exactly why the copy exists)
/// counts as ABSENT, not as an empty table. Comparing source and copy BEFORE
/// `Db::open` runs migrations is what makes "my copy is the real thing" a
/// measurement rather than an assumption.
#[derive(Debug, PartialEq, Eq)]
struct Fingerprint {
    entities: Option<i64>,
    edges: Option<i64>,
    aliases: Option<i64>,
    recall_rows: Option<i64>,
    recall_queries: Option<i64>,
}

/// Rows in `table`, or `None` when this file does not have that table at all.
fn table_rows(conn: &rusqlite::Connection, table: &str) -> Option<i64> {
    let present: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
            [table],
            |r| r.get(0),
        )
        .unwrap();
    if present == 0 {
        return None;
    }
    Some(
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap(),
    )
}

fn fingerprint_of(conn: &rusqlite::Connection) -> Fingerprint {
    let distinct_queries = if table_rows(conn, "recall_log").is_some() {
        Some(
            conn.query_row("SELECT COUNT(DISTINCT query) FROM recall_log", [], |r| {
                r.get(0)
            })
            .unwrap(),
        )
    } else {
        None
    };
    Fingerprint {
        entities: table_rows(conn, "entities"),
        edges: table_rows(conn, "entity_edges"),
        aliases: table_rows(conn, "entity_aliases"),
        recall_rows: table_rows(conn, "recall_log"),
        recall_queries: distinct_queries,
    }
}

/// This instrument's OWN copy of the source, freshly made and verified.
///
/// `tag` names the instrument; the pid separates concurrent runs. The target is
/// per-instrument, so three instruments running together cannot share one file --
/// which is the whole point: `Db::open` migrates whatever it opens.
async fn own_copy(tag: &str) -> Db {
    let src = source_path();
    let dir = std::env::temp_dir().join("ruagent-t42-live");
    std::fs::create_dir_all(&dir).expect("creating the private-copy directory");
    let dst = dir.join(format!("live-{tag}-{}.db", std::process::id()));
    // VACUUM INTO refuses an existing output file ("output file already exists"):
    // remove a stale target first instead of failing on it.
    if dst.exists() {
        std::fs::remove_file(&dst)
            .unwrap_or_else(|e| panic!("removing the stale copy target {dst:?}: {e}"));
    }
    let wanted = {
        let conn = open_source_readonly(&src);
        let fp = fingerprint_of(&conn);
        conn.execute_batch(&format!(
            "VACUUM INTO '{}'",
            dst.to_string_lossy().replace('\\', "/")
        ))
        .unwrap_or_else(|e| panic!("VACUUM INTO {dst:?} from {src:?}: {e}"));
        fp
    };
    // Before migrations touch it: the copy must BE the source's content.
    let mine = fingerprint_of(&rusqlite::Connection::open(&dst).unwrap());
    assert_eq!(
        mine, wanted,
        "the private copy must carry the source's object set (source {src:?})"
    );
    let db = Db::open(&dst).unwrap();
    // And it is mine to write: a marker row that the source must not have.
    let tag_owned = tag.to_string();
    db.call_flat(move |conn| -> Result<(), rusqlite::Error> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS t42_owner (tag TEXT NOT NULL);")?;
        conn.execute("DELETE FROM t42_owner", [])?;
        conn.execute("INSERT INTO t42_owner (tag) VALUES (?1)", [&tag_owned])?;
        Ok(())
    })
    .await
    .expect("writing the ownership marker into my own copy");
    let seen: String = db
        .call_flat(|conn| conn.query_row("SELECT tag FROM t42_owner", [], |r| r.get(0)))
        .await
        .unwrap();
    assert_eq!(seen, tag, "my copy carries MY marker");
    let src_tables: i64 = open_source_readonly(&src)
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='t42_owner'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        src_tables, 0,
        "the ownership marker must NOT be visible in the source {src:?}: \
         this instrument writes its input, so it must own its input"
    );
    println!("[t42] {tag}: own copy {dst:?} | marker exclusive | fingerprint {mine:?}");
    db
}

/// Assert that a copy is still PRISTINE, the t36-style trap announcement: if this
/// instrument were reading a copy another instrument had already written, the
/// message says exactly that instead of failing later on an unrelated assertion.
async fn assert_pristine(db: &Db, what: &str, sql: &'static str, tag: &str) {
    let n: i64 = db
        .call_flat(move |conn| conn.query_row(sql, [], |r| r.get(0)))
        .await
        .unwrap();
    assert_eq!(
        n, 0,
        "{what} is not 0 but {n}: this instrument is reading a copy that another \
         instrument has already written ({tag} needs its own input)"
    );
}

async fn distinct_live_queries(db: &Db) -> Vec<String> {
    db.call_flat(|conn| -> Result<Vec<String>, rusqlite::Error> {
        let mut st = conn
            .prepare("SELECT query FROM recall_log GROUP BY query ORDER BY COUNT(*) DESC, query")?;
        let v: Vec<String> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(v)
    })
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "needs RUAGENT_GRAPH_LIVE_COPY pointing at a COPY of the live db"]
async fn the_real_query_frame_through_the_new_legs() {
    let db = own_copy("frame").await;
    // This instrument only reads; the frame it is about to measure IS its object set.
    let (rows, distinct, entities) = db
        .call_flat(|conn| -> Result<(i64, i64, i64), rusqlite::Error> {
            Ok((
                conn.query_row("SELECT COUNT(*) FROM recall_log", [], |r| r.get(0))?,
                conn.query_row("SELECT COUNT(DISTINCT query) FROM recall_log", [], |r| {
                    r.get(0)
                })?,
                conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?,
            ))
        })
        .await
        .unwrap();
    println!(
        "[t42] frame instrument object set: recall_log rows={rows} distinct_queries={distinct} entities={entities}"
    );
    assert!(
        rows > 0 && distinct > 0,
        "a frame instrument on an empty frame measures nothing"
    );
    let queries = distinct_live_queries(&db).await;
    assert_eq!(
        queries.len() as i64,
        distinct,
        "the query frame this instrument reads must be the frame it just fingerprinted"
    );
    let queries = distinct_live_queries(&db).await;
    let mut old_nonempty = 0;
    let mut new_nonempty = 0;
    let mut multi = 0;
    let mut per_leg = std::collections::BTreeMap::new();
    for q in &queries {
        let old = g::search_entities(&db, q, 10).await.unwrap();
        let seeds = g::resolve_seeds(&db, q, 32).await.unwrap();
        if !old.is_empty() {
            old_nonempty += 1;
        }
        if !seeds.is_empty() {
            new_nonempty += 1;
        }
        if seeds.len() > 1 {
            multi += 1;
        }
        for s in &seeds {
            *per_leg.entry(s.leg.as_str()).or_insert(0) += 1;
        }
        println!(
            "   strict={:<3} seeds={:<3} {:?}",
            old.len(),
            seeds.len(),
            seeds.iter().map(|s| s.leg.as_str()).collect::<Vec<_>>()
        );
    }
    println!(
        "REAL FRAME n={} | strict non-empty {old_nonempty} -> new {new_nonempty} | queries with >1 seed: {multi}",
        queries.len()
    );
    println!("seeds by leg (all queries): {per_leg:?}");
    assert!(
        new_nonempty >= 13,
        "G1 on the real frame: >=13 non-empty queries, got {new_nonempty}"
    );
    assert!(new_nonempty > old_nonempty);
}

#[tokio::test]
#[ignore = "needs RUAGENT_GRAPH_LIVE_COPY pointing at a COPY of the live db"]
async fn redundant_pairs_on_the_real_graph_go_to_zero() {
    let db = own_copy("redundancy").await;
    // This instrument WRITES (merges), so its reading is only meaningful on a
    // pristine copy: aliases and merges are its own output, not its input.
    assert_pristine(
        &db,
        "entity_aliases",
        "SELECT COUNT(*) FROM entity_aliases",
        "redundancy",
    )
    .await;
    let (entities0, edges0) = db
        .call_flat(|conn| -> Result<(i64, i64), rusqlite::Error> {
            Ok((
                conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0))?,
                conn.query_row("SELECT COUNT(*) FROM entity_edges", [], |r| r.get(0))?,
            ))
        })
        .await
        .unwrap();
    println!(
        "[t42] redundancy instrument object set (before its own writes): entities={entities0} edges={edges0} aliases=0"
    );
    let before = g::redundant_pairs(&db).await.unwrap();
    println!("REAL: the judge finds {} redundant pairs:", before.len());
    for (a, b, why) in &before {
        println!("   #{a} / #{b} :: {why}");
    }
    let mut merged = 0;
    for _ in 0..30 {
        let pairs = g::redundant_pairs(&db).await.unwrap();
        let Some((a, b, _)) = pairs.into_iter().next() else {
            break;
        };
        g::merge_entities(&db, b, a).await.unwrap();
        merged += 1;
    }
    let after = g::redundant_pairs(&db).await.unwrap();
    let edges: i64 = db
        .call_flat(|c| c.query_row("SELECT COUNT(*) FROM entity_edges", [], |r| r.get(0)))
        .await
        .unwrap();
    let entities: i64 = db
        .call_flat(|c| c.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0)))
        .await
        .unwrap();
    let aliases: i64 = db
        .call_flat(|c| c.query_row("SELECT COUNT(*) FROM entity_aliases", [], |r| r.get(0)))
        .await
        .unwrap();
    println!(
        "REAL: merged {merged} pairs -> redundant {} | entities {entities} | edges {edges} (unchanged) | aliases {aliases}",
        after.len()
    );
    assert_eq!(after.len(), 0, "G6 target on the real graph");
    assert_eq!(edges, 67, "merges move edges, they never drop them");
}

#[tokio::test]
#[ignore = "needs RUAGENT_GRAPH_LIVE_COPY pointing at a COPY of the live db"]
async fn community_coverage_and_multihop_on_the_real_graph() {
    let db = own_copy("communities").await;
    // This instrument WRITES the partition, so a copy that already carries one is
    // someone else's input, not its own.
    assert_pristine(
        &db,
        "communities",
        "SELECT COUNT(*) FROM communities",
        "communities",
    )
    .await;
    let entities0: i64 = db
        .call_flat(|conn| conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get(0)))
        .await
        .unwrap();
    println!(
        "[t42] communities instrument object set (before its own writes): entities={entities0} communities=0"
    );
    let build = g::build_communities(&db, 0).await.unwrap();
    let (covered, non_isolated) = g::community_coverage(&db, 0).await.unwrap();
    println!(
        "REAL communities: {} communities, covered {covered}/{non_isolated} non-isolated, split_by_modularity={}",
        build.communities, build.split_by_modularity
    );
    assert!(covered as f64 / non_isolated.max(1) as f64 >= 0.90);

    let ev = g::retrieve(&db, &g::GraphQuery::for_text("ruagent"))
        .await
        .unwrap();
    println!(
        "REAL retrieve(ruagent): seeds={} paths={} facts={} truncated_by={:?} frontiers={:?}",
        ev.seeds.len(),
        ev.paths.len(),
        ev.fact_count(),
        ev.stats.truncated_by,
        ev.stats.frontier_sizes
    );
    assert_eq!(ev.paths.len(), 12, "the default max_paths bound holds");
    assert!(ev.fact_count() <= 24, "the default max_facts bound holds");
    assert!(
        ev.paths.iter().all(|p| p.hops <= 2),
        "the default hop bound holds"
    );
    println!("first evidence line: {}", ev.lines()[0]);
}
