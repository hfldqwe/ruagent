//! Entity-leg query-shape matrix (t250).
//!
//! WHY: t247 measured 6 input classes against the LIVE graph and found the
//! hyphen-model and CJK-substring classes return 0 hits while the entity
//! exists. That was a reading about one database. This file turns it into a
//! reading about the QUERY CONSTRUCTION: the same classes against graphs built
//! here, run through BOTH constructors, so the claim is falsifiable without
//! touching ~/.ruagent.
//!
//! Object set: the entities created below, shaped like the live hard cases.
//! Sampling surface: in-memory SQLite, FTS5 populated by the same triggers the
//! live database uses.

use ruagent_graph::{search_entities, search_entities_loose, upsert_entity};
use ruagent_store::Db;

/// Shaped like the live graph: AutoHotkey carries v2.0.28 in its SUMMARY, never
/// adjacent to the word autohotkey -- which is exactly why the quoted phrase
/// "autohotkey-v2" cannot reach it.
const ENTITIES: &[(&str, &str, &str)] = &[
    (
        "AutoHotkey",
        "tool",
        "脚本语言；当前版本 v2.0.28；XButton2 改键到 F6",
    ),
    ("蓝鲸潜艇", "concept", "用户与本会话约定的暗号/验证口令"),
    (
        "银河麒麟 V10",
        "product",
        "arm64 无 root 无外网的国产操作系统",
    ),
    ("find-skills", "tool", "npx skills find 子命令"),
    (
        "Skills CLI",
        "tool",
        "通过 npx skills 调用的技能管理命令行工具（v1.7.0）",
    ),
    ("C++", "tool", "编译型系统级语言"),
    ("XButton2", "concept", "鼠标侧键，AHK 里用来改键"),
    ("deepseek-v4-flash", "product", "provider 上的快速模型"),
    ("deploy.sh", "repo", "部署脚本"),
];

/// (query, class, expected entity name)
const QUERIES: &[(&str, &str, &str)] = &[
    ("skills", "plain-word", "Skills CLI"),
    ("autohotkey-v2", "hyphen-model", "AutoHotkey"),
    ("deepseek-v4-flash", "hyphen-model", "deepseek-v4-flash"),
    ("潜艇", "cjk-substring", "蓝鲸潜艇"),
    ("麒麟", "cjk-substring", "银河麒麟 V10"),
    ("蓝鲸潜艇", "cjk-phrase", "蓝鲸潜艇"),
    ("银河麒麟", "cjk-phrase", "银河麒麟 V10"),
    ("Skills CLI", "multiword-en", "Skills CLI"),
    ("XButton2", "special", "XButton2"),
    ("C++", "special", "C++"),
    ("deploy.sh", "special", "deploy.sh"),
];

async fn seed(db: &Db, entities: &[(&str, &str, &str)]) {
    for (name, kind, summary) in entities {
        upsert_entity(db, name, Some(kind), Some(summary))
            .await
            .unwrap();
    }
}

async fn names(db: &Db, q: &str, loose: bool) -> Vec<String> {
    let hits = if loose {
        search_entities_loose(db, q, 5).await.unwrap()
    } else {
        search_entities(db, q, 5).await.unwrap()
    };
    hits.into_iter().map(|e| e.name).collect()
}

#[tokio::test]
async fn query_shape_matrix() {
    let db = Db::open_in_memory().unwrap();
    seed(&db, ENTITIES).await;

    println!("class            query                  strict  loose   expected");
    let mut regressions: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for (query, class, expected) in QUERIES {
        let s = names(&db, query, false).await;
        let l = names(&db, query, true).await;
        println!(
            "{:<16} {:<22} {:<7} {:<7} {}",
            class,
            query,
            s.len(),
            l.len(),
            expected
        );
        // A fix must never turn a working query into a dead one.
        if !s.is_empty() && l.is_empty() {
            regressions.push(query.to_string());
        }
        if !l.iter().any(|n| n == expected) {
            missing.push(format!("{} -> loose gave {:?}", query, l));
        }
    }
    assert!(
        regressions.is_empty(),
        "loose returned nothing where strict had hits: {:?}",
        regressions
    );
    assert!(
        missing.is_empty(),
        "expected entity not reached: {:?}",
        missing
    );
}

/// The mechanism, named -- not "hyphens are broken". A quoted FTS5 phrase
/// requires its tokens ADJACENT IN ONE COLUMN. So the same query, through the
/// same constructor, hits or misses depending on the STORED text. Pinned from
/// both sides so neither half can rot silently.
#[tokio::test]
async fn hyphen_phrase_requires_adjacency() {
    let split = Db::open_in_memory().unwrap();
    seed(
        &split,
        &[("AutoHotkey", "tool", "脚本语言；当前版本 v2.0.28")],
    )
    .await;
    assert!(
        names(&split, "autohotkey-v2", false).await.is_empty(),
        "phrase must MISS when autohotkey and v2 are not adjacent"
    );
    assert!(
        names(&split, "autohotkey-v2", true)
            .await
            .iter()
            .any(|n| n == "AutoHotkey"),
        "the loose constructor must still reach it"
    );

    let adjacent = Db::open_in_memory().unwrap();
    seed(&adjacent, &[("AutoHotkey v2", "tool", "release notes")]).await;
    assert!(
        !names(&adjacent, "autohotkey-v2", false).await.is_empty(),
        "phrase must HIT when the tokens are adjacent; that is what makes the failure about adjacency rather than about hyphens"
    );
}

/// CJK: unicode61 makes a whole run of Han characters ONE term, so no FTS query
/// can find a substring of it. LIKE is the only path. Pinned so a later change
/// cannot drop the fallback without turning this red.
#[tokio::test]
async fn cjk_substring_needs_like() {
    let db = Db::open_in_memory().unwrap();
    seed(&db, ENTITIES).await;
    for (query, expected) in [("潜艇", "蓝鲸潜艇"), ("麒麟", "银河麒麟 V10")] {
        assert!(
            names(&db, query, false).await.is_empty(),
            "FTS must MISS the substring; if it hits, the tokenizer changed"
        );
        assert!(
            names(&db, query, true).await.iter().any(|n| n == expected),
            "LIKE fallback must reach the entity for the substring query"
        );
    }
    // ... while a query that IS the whole run keeps working through FTS alone.
    assert!(
        names(&db, "蓝鲸潜艇", false)
            .await
            .iter()
            .any(|n| n == "蓝鲸潜艇"),
        "a whole-run query must still be found by FTS"
    );
}
