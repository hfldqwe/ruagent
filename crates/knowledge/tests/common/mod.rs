//! The frozen gold set and the metric judge, shared by the two live-copy
//! instruments (t7 / R-A §E3).
//!
//! WHY SHARED, and this is not tidiness: `retrieval-gold-copy.rs` must compile
//! against the PRE-t7 revision — that is how the before/after pair is taken on
//! the live corpus with ONE instrument — while `retrieval-gold-live.rs` adds the
//! post-t7 per-hit evidence. If each carried its own copy of the gold list or
//! its own recall definition, the two readings would look comparable and would
//! not be: the query set version and the metric formula would differ silently.
//! So: one list, one judge, and every helper here uses only APIs that exist
//! BEFORE t7 (`Knowledge::search`, `Knowledge::search_legs`), which is what
//! keeps the pre-t7 run a measured failure instead of a compile error.
#![allow(dead_code)] // each test binary uses a different subset

use ruagent_knowledge::Knowledge;
use std::collections::HashMap;

/// (query, gold document name, class). Verbatim from the frozen probe
/// `%TEMP%\ra_gold_probe.py` (2026-09-27T21:46:03+08:00), whose gold names were
/// each verified to exist exactly once on the live database. DO NOT EDIT: a
/// recall number is only comparable with another taken over the same set.
pub const GOLD: &[(&str, &str, &str)] = &[
    (
        "Docker compose",
        "obsidian/notes/Docker-OQEurB-Docker compose-Docker compose",
        "exact-ascii",
    ),
    (
        "docker 常用命令",
        "obsidian/notes/Docker-OQEurB-docker 常用命令-docker 常用命令",
        "mixed-cjk-ascii",
    ),
    (
        "Springboot 热部署",
        "obsidian/notes/45v6HJ-Java框架-模块-Springboot-Springboot热部署-Springboot热部署",
        "mixed-cjk-ascii",
    ),
    (
        "反射的访问权限问题",
        "obsidian/notes/45v6HJ-常见问题-反射的访问权限问题-反射的访问权限问题",
        "cjk-run-long",
    ),
    (
        "AQS并发锁",
        "obsidian/notes/45v6HJ-JavaSE-AQS并发锁-AQS并发锁",
        "cjk-run-nosep",
    ),
    (
        "缓存caffeine",
        "obsidian/notes/45v6HJ-Java框架-模块-缓存caffeine-缓存caffeine",
        "mixed-cjk-ascii",
    ),
    (
        "Maven项目管理",
        "obsidian/notes/45v6HJ-Maven项目管理-Maven项目管理",
        "mixed-cjk-ascii",
    ),
    (
        "JdbcTemplate",
        "obsidian/notes/45v6HJ-Java框架-模块-Spring5-JdbcTemplate-JdbcTemplate",
        "exact-ascii",
    ),
    (
        "OJ在线判题系统",
        "obsidian/notes/45v6HJ-项目学习-OJ在线判题系统-OJ在线判题系统",
        "mixed-cjk-ascii",
    ),
    (
        "docker exec进入容器并执行命令",
        "obsidian/notes/Docker-OQEurB-docker exec进入容器并执行命令-docker exec进入容器并执行命令",
        "mixed-long",
    ),
    (
        "@Param装饰器",
        "obsidian/notes/45v6HJ-Java框架-模块-Mybatis-不使用@Param装饰器，自动绑定参数-不使用@Param装饰器，自动绑定参数",
        "punctuated",
    ),
    (
        "RPC框架",
        "obsidian/notes/45v6HJ-项目学习-RPC框架-RPC框架",
        "mixed-cjk-ascii",
    ),
    (
        "ChatGPT提示词",
        "obsidian/notes/CNGceW-ChatGPT提示词-ChatGPT提示词",
        "mixed-cjk-ascii",
    ),
    (
        "github相关",
        "obsidian/notes/14mhGE/github相关",
        "mixed-cjk-ascii",
    ),
    (
        "kubernetes 滚动更新回滚",
        "wiki/kubernetes-troubleshooting",
        "content-wiki",
    ),
];

/// The unanswerable half (R-A A9). They contribute to no recall number, but a
/// set that quietly drops them cannot measure the failure mode that matters
/// most, so they are carried and reported.
pub const NO_ANSWER: &[&str] = &[
    "capital of Peru",
    "xylophone zebra",
    "search_legs",
    "如何用 Rust 写一个向量索引",
    "best time to see migrating whales",
    "photosynthesis light-dependent reactions",
    "ruagent memory",
];

pub const SET_NAME: &str = "ra-gold-t1-frozen";
pub const SET_VERSION: &str = "2026-09-27T21:46:03+08:00";

/// (recall@1, recall@5, recall@20, MRR). A gold that never appeared counts as a
/// miss rather than being dropped, because dropping it would inflate the score.
pub fn metrics(ranks: &[Option<usize>]) -> (f64, f64, f64, f64) {
    let n = ranks.len().max(1) as f64;
    let (mut r1, mut r5, mut r20, mut mrr) = (0.0, 0.0, 0.0, 0.0);
    for pos in ranks.iter().flatten() {
        if *pos == 0 {
            r1 += 1.0;
        }
        if *pos < 5 {
            r5 += 1.0;
        }
        if *pos < 20 {
            r20 += 1.0;
        }
        mrr += 1.0 / (*pos as f64 + 1.0);
    }
    (r1 / n, r5 / n, r20 / n, mrr / n)
}

/// Binary nDCG@10 with exactly one gold per query: IDCG = 1, so this is
/// `1/log2(rank+2)` under rank 10 and 0 at or beyond it. R-A C9 states this form
/// explicitly so nobody mistakes it for a graded TREC-DL nDCG (B8).
pub fn ndcg_at_10(ranks: &[Option<usize>]) -> f64 {
    let n = ranks.len().max(1) as f64;
    let sum: f64 = ranks
        .iter()
        .map(|r| match r {
            Some(pos) if *pos < 10 => 1.0 / ((*pos as f64 + 2.0).log2()),
            _ => 0.0,
        })
        .sum();
    sum / n
}

pub fn mean(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}

/// Write the frozen set into the (copied) database. Idempotent -- **but only
/// because migration 0025 gives it a UNIQUE constraint to ignore against**.
///
/// This comment used to claim idempotence on its own, and that claim was false:
/// `INSERT OR IGNORE` ignores a conflict only when some UNIQUE constraint defines
/// one, and 0019 declared none on `(set_id, query)`. Measured on a scratch
/// database, six consecutive runs left `COUNT(*)` = 22, 44, 66, 88, 110, **132**
/// for a 22-query set (V-A's F-1; the pair is re-taken by
/// `tests/gold-seed-idempotence.rs`). The table's size is the premise a future
/// target is read from, so the difference between 22 and 132 is the difference
/// between a measurement and a self-awarded pass.
///
/// NOTE the semantic this keeps, on purpose: `OR IGNORE` never touches an
/// existing row. That matters more than refreshing stale seed values, because
/// `judged_by` and `note` are where a HUMAN judgement would live (0019's comment;
/// R-A C3 is waiting for `judged_by='human'` rows, of which there are 0 today). An
/// upsert that overwrote them would destroy the only human labels the corpus has.
/// An existing row that disagrees with the frozen constants is therefore REPORTED
/// (see the drift line printed below), not overwritten.
pub async fn seed(db: &ruagent_store::Db) -> (i64, i64) {
    seed_checked(db).await.unwrap_or_else(|e| panic!("{e}"))
}

/// The fallible form of [`seed`]: the same insert plus the drift judge, returning
/// the verdict instead of panicking.
///
/// This exists so the JUDGE ITSELF is testable: a test can construct a drift and
/// read the verdict as a value (`Err`), rather than having to catch a panic to
/// show that the drift was caught. It is also the shape a caller that wants to
/// report rather than abort would use -- and the pair
/// (`db_drift_check` alone, `seed_checked`) is exactly the difference between
/// t29's print and a criterion, which is what t30 is about.
pub async fn seed_checked(db: &ruagent_store::Db) -> Result<(i64, i64), String> {
    let (set_id, written) = insert_frozen_set(db).await;
    db_drift_check(db, set_id)
        .await
        .into_result()
        .map(|_| (set_id, written))
}

/// The inserts, with no opinion about what the table afterwards holds.
async fn insert_frozen_set(db: &ruagent_store::Db) -> (i64, i64) {
    let names: Vec<(String, String, String)> = GOLD
        .iter()
        .map(|(q, g, c)| (q.to_string(), g.to_string(), c.to_string()))
        .collect();
    let no_answer: Vec<String> = NO_ANSWER.iter().map(|s| s.to_string()).collect();
    let now = chrono::Utc::now().to_rfc3339();
    let seeded: (i64, i64) = db
        .call(move |conn| -> Result<(i64, i64), rusqlite::Error> {
        refuse_live_root(conn);
        conn.execute(
            "INSERT OR IGNORE INTO query_eval_sets (name, version, created_at, frozen_at)
             VALUES (?1, ?2, ?3, ?3)",
            rusqlite::params![SET_NAME, SET_VERSION, now],
        )?;
        let set_id: i64 = conn.query_row(
            "SELECT id FROM query_eval_sets WHERE name = ?1",
            [SET_NAME],
            |r| r.get(0),
        )?;
        let mut written = 0i64;
        for (q, gold, class) in &names {
            let doc: Option<i64> = conn
                .query_row("SELECT id FROM documents WHERE name = ?1", [gold], |r| {
                    r.get(0)
                })
                .ok();
            written += conn.execute(
                "INSERT OR IGNORE INTO query_eval_gold
                 (set_id, query, class, gold_document_id, answerable, judged_by, note)
                 VALUES (?1, ?2, ?3, ?4, 1, 'title-derived', ?5)",
                rusqlite::params![
                    set_id,
                    q,
                    class,
                    doc,
                    "frozen by R-A A3; the gold NAME is derived from the query text (weak proxy, C7)"
                ],
            )? as i64;
        }
        for q in &no_answer {
            written += conn.execute(
                "INSERT OR IGNORE INTO query_eval_gold
                 (set_id, query, class, gold_document_id, answerable, judged_by, note)
                 VALUES (?1, ?2, 'no-answer', NULL, 0, 'title-derived', ?3)",
                rusqlite::params![set_id, q, "frozen by R-A A9"],
            )? as i64;
        }
        Ok((set_id, written))
    })
    .await
    .unwrap()
    .unwrap();

    let (set_id, written) = seeded;
    (set_id, written)
}

/// What a drift check found, split by WHO owns the row.
pub struct DriftReport {
    /// Rows the SEED owns (`judged_by` is not `'human'`): the table is supposed to
    /// hold exactly the frozen constants here, so a disagreement is a defect.
    pub seed_owned: Vec<String>,
    /// Rows a HUMAN labelled: they may legitimately disagree, and the seed must
    /// never overwrite them. Counted and printed, never a failure.
    pub human: Vec<String>,
    /// Frozen queries with no row at all: the seeding did not take.
    pub absent: Vec<String>,
    pub total: usize,
}

impl DriftReport {
    /// The criterion, in one place: an unowned disagreement or a missing row is a
    /// failure; a human label disagreeing is not.
    pub fn into_result(self) -> Result<Self, String> {
        if self.seed_owned.is_empty() && self.absent.is_empty() {
            return Ok(self);
        }
        Err(format!(
            "gold drift: {} seed-owned row(s) disagree with the frozen constants and {} frozen \
             quer(y/ies) have no row -- the evaluation set is a judgement PREMISE (C3 reads its \
             size), so this is a failure and not a note. Rows: {:?}. Human-labelled rows are \
             listed separately and are allowed to differ: {:?}",
            self.seed_owned.len(),
            self.absent.len(),
            self.seed_owned,
            self.human
        ))
    }
}

/// Does an existing row still agree with the frozen constants?
///
/// Returns the disagreement split by ownership, and PRINTS it either way, so the
/// reading is visible even when the criterion passes. The failure decision lives
/// in [`DriftReport::into_result`], not here: the check is also the instrument
/// that demonstrates the judge works (a test can construct a drift, ask this
/// function, and see it reported without having to catch a panic).
pub async fn db_drift_check(db: &ruagent_store::Db, set_id: i64) -> DriftReport {
    let expected: Vec<(String, String, i64)> = GOLD
        .iter()
        .map(|(q, _g, c)| (q.to_string(), c.to_string(), 1i64))
        .chain(
            NO_ANSWER
                .iter()
                .map(|q| (q.to_string(), "no-answer".to_string(), 0i64)),
        )
        .collect();
    let total = expected.len();
    let for_query = expected.clone();
    let rows: Vec<(String, String, i64, String)> = db
        .call(
            move |conn| -> Result<Vec<(String, String, i64, String)>, rusqlite::Error> {
                let mut out = Vec::new();
                for (query, _class, _answerable) in &for_query {
                    let row: Option<(String, i64, String)> = conn
                        .query_row(
                            "SELECT class, answerable, COALESCE(judged_by, '') FROM query_eval_gold
                         WHERE set_id = ?1 AND query = ?2",
                            rusqlite::params![set_id, query],
                            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                        )
                        .ok();
                    match row {
                        Some((c, a, by)) => out.push((query.clone(), c, a, by)),
                        None => out.push((query.clone(), String::new(), -1, String::new())),
                    }
                }
                Ok(out)
            },
        )
        .await
        .unwrap()
        .unwrap();

    let mut report = DriftReport {
        seed_owned: Vec::new(),
        human: Vec::new(),
        absent: Vec::new(),
        total,
    };
    for ((query, class, answerable), (_, c, a, by)) in expected.iter().zip(rows.iter()) {
        if *a < 0 {
            report.absent.push(format!("{query}: no row for this set"));
            continue;
        }
        if *c == *class && *a == *answerable {
            continue;
        }
        let detail =
            format!("{query}: table=({c},{a},judged_by={by}) frozen=({class},{answerable})");
        if by == "human" {
            report.human.push(detail);
        } else {
            report.seed_owned.push(detail);
        }
    }
    println!(
        "[gold] drift: {} seed-owned disagreement(s), {} human-labelled disagreement(s), {} \
         missing row(s), of {total} frozen queries -- {}",
        report.seed_owned.len(),
        report.human.len(),
        report.absent.len(),
        if report.seed_owned.is_empty() && report.absent.is_empty() {
            "the table still holds the frozen set".to_string()
        } else {
            format!(
                "FAILURE: seed-owned={:?} missing={:?} (human-labelled, allowed to differ: {:?})",
                report.seed_owned, report.absent, report.human
            )
        }
    );
    report
}

/// THE SEED WRITES, so it refuses the live root.
///
/// The decision looks at the file the connection ACTUALLY has open
/// (`pragma_database_list`), not at an argument a caller could forget to pass, so
/// "the seed only ever writes a copy" is enforced rather than documented. A panic
/// is deliberate: this runs inside a test, and the failure mode it prevents --
/// migrating and writing the user's live database from an evaluation instrument --
/// must stop the run in the loudest available way.
fn refuse_live_root(conn: &rusqlite::Connection) {
    let file: String = conn
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) else {
        return;
    };
    if file.is_empty() {
        return; // an in-memory scratch database cannot be the user's
    }
    let (Ok(live), Ok(target)) = (
        std::fs::canonicalize(std::path::Path::new(&home).join(".ruagent")),
        std::fs::canonicalize(&file),
    ) else {
        return; // an uncanonicalisable path cannot be shown to be the live root
    };
    if is_inside(&target, &live) {
        panic!(
            "refusing to seed {target:?}: it is inside the live root {live:?}. This instrument \
             applies migrations and writes gold rows -- point it at a COPY (VACUUM INTO into \
             %TEMP%) and never at the user's database."
        );
    }
}

/// Component-wise containment, so `.ruagent-other` is not inside `.ruagent`.
pub fn is_inside(target: &std::path::Path, root: &std::path::Path) -> bool {
    target.starts_with(root)
}

/// One query's raw reading, per query (so a stratified slice can be recomputed
/// from the same rows rather than re-queried).
pub struct Row {
    pub query: String,
    pub class: String,
    pub rank: Option<usize>,
    pub top_document: Option<String>,
    pub top_score: f32,
    pub top_distance: Option<f32>,
    /// 0-based semantic rank of the gold, when the keyword leg is not the only
    /// source; used for diagnosis, never for the headline metrics.
    pub semantic_rank_of_gold: Option<usize>,
}

pub struct Report {
    pub rows: Vec<Row>,
    pub no_answer: Vec<Row>,
    pub misses: usize,
}

/// The judge. Uses `search` + `search_legs` only, i.e. APIs that exist on BOTH
/// sides of t7 — that is what makes the pre-t7 run of this same function a
/// measurement instead of a compile error.
pub async fn measure(kb: &Knowledge) -> Report {
    let mut rows = Vec::new();
    for (query, gold, class) in GOLD {
        let hits = kb.search(query, 20).await.unwrap();
        let docs: Vec<String> = hits.iter().map(|h| h.document.clone()).collect();
        let rank = docs.iter().position(|d| d == gold);
        let legs = kb.search_legs(query, 20).await.unwrap();
        let semantic_rank_of_gold = legs.semantic.iter().position(|h| {
            hits.iter()
                .any(|hit| hit.chunk_id == h.chunk_id && &hit.document == gold)
        });
        rows.push(Row {
            query: (*query).to_string(),
            class: (*class).to_string(),
            rank,
            top_document: docs.first().cloned(),
            top_score: hits.first().map(|h| h.score).unwrap_or(0.0),
            top_distance: legs.semantic.first().map(|h| h.raw_score),
            semantic_rank_of_gold,
        });
    }
    let mut no_answer = Vec::new();
    for query in NO_ANSWER {
        let hits = kb.search(query, 20).await.unwrap();
        let legs = kb.search_legs(query, 20).await.unwrap();
        no_answer.push(Row {
            query: (*query).to_string(),
            class: "no-answer".to_string(),
            rank: None,
            top_document: hits.first().map(|h| h.document.clone()),
            top_score: hits.first().map(|h| h.score).unwrap_or(0.0),
            top_distance: legs.semantic.first().map(|h| h.raw_score),
            semantic_rank_of_gold: None,
        });
    }
    let misses = rows.iter().filter(|r| r.rank.is_none()).count();
    Report {
        rows,
        no_answer,
        misses,
    }
}

/// Ranks in the gold set's own order (the order the metrics are defined over).
pub fn ranks(report: &Report) -> Vec<Option<usize>> {
    report.rows.iter().map(|r| r.rank).collect()
}

/// The stratified slices, by class, in a stable order.
pub fn by_class(report: &Report) -> Vec<(String, Vec<Option<usize>>)> {
    let mut groups: HashMap<String, Vec<Option<usize>>> = HashMap::new();
    for row in &report.rows {
        groups.entry(row.class.clone()).or_default().push(row.rank);
    }
    let mut out: Vec<(String, Vec<Option<usize>>)> = groups.into_iter().collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Print every per-query line and the headline numbers. Nothing here asserts:
/// the two instruments decide their own targets, so this stays a pure reading.
pub fn print_report(tag: &str, report: &Report) {
    for row in report.rows.iter().chain(report.no_answer.iter()) {
        println!(
            "[{tag}] {:?} class={} rank={:?} top1={:?} top1_score={:.6} top1_distance={:?} semantic_rank_of_gold={:?}",
            row.query,
            row.class,
            row.rank,
            row.top_document,
            row.top_score,
            row.top_distance,
            row.semantic_rank_of_gold
        );
    }
    let rs = ranks(report);
    let (r1, r5, r20, mrr) = metrics(&rs);
    println!(
        "[{tag}] n={} misses={} recall@1={:.4} recall@5={:.4} recall@20={:.4} MRR={:.4} nDCG@10={:.4}",
        rs.len(),
        report.misses,
        r1,
        r5,
        r20,
        mrr,
        ndcg_at_10(&rs)
    );
    for (class, rs) in by_class(report) {
        let (c1, c5, c20, cm) = metrics(&rs);
        println!(
            "[{tag}]   class={class:<16} n={} recall@1={:.4} recall@5={:.4} recall@20={:.4} MRR={:.4}",
            rs.len(),
            c1,
            c5,
            c20,
            cm
        );
    }
    let answerable: Vec<f32> = report.rows.iter().filter_map(|r| r.top_distance).collect();
    let unanswerable: Vec<f32> = report
        .no_answer
        .iter()
        .filter_map(|r| r.top_distance)
        .collect();
    if !answerable.is_empty() && !unanswerable.is_empty() {
        println!(
            "[{tag}] semantic top-1 distance: answerable mean={:.4} max={:.4} | no-answer mean={:.4} min={:.4}",
            mean(&answerable),
            answerable.iter().cloned().fold(f32::MIN, f32::max),
            mean(&unanswerable),
            unanswerable.iter().cloned().fold(f32::MAX, f32::min)
        );
    }
}
