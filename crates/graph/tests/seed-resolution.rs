//! G1/G2: seed resolution names the leg that found each entity, on the REAL
//! query frame (the 23 distinct queries in the live `recall_log`, 2026-09-16 ..
//! 2026-09-27) and on every entity's own name.
//!
//! BEFORE/AFTER IN ONE RUN, deliberately. Every "before" number below is
//! produced by calling the PRE-gen2 function (`search_entities`, unchanged) on
//! the same fixtures, so the comparison is not a memory of an old run: it is the
//! old code path executing next to the new one.

use ruagent_graph as g;

mod fixture;
use fixture::{PathmapDb, corpus};

/// The live query frame, copied verbatim from `recall_log`'s distinct queries
/// (read-only probe, 2026-09-27T13:58Z). Copied, not generated: this is the
/// sampling frame the spec's G1 target is stated against.
const LIVE_QUERY_FRAME: &[&str] = &[
    "kettle",
    "autohotkey-v2",
    "kettle material",
    "ruagent",
    "autohotkey 改键",
    "部署流水线",
    "wiki",
    "deploy",
    "t99 control perturbation delete me",
    "postgres 连接池",
    "项目",
    "轮询 自动通知 用户偏好",
    "记忆",
    "知识库",
    "中文交流 简洁 直接 表达习惯",
    "ruagent 本地优先 多智能体平台",
    "ruagent memory",
    "panel",
    "deploy pipeline",
    "Windows 11 运行环境 操作系统",
    "T97C generating 任务",
    "T97B rollback fail",
    "T97 MARK TWO doctor probe kettle chrome",
];

#[tokio::test]
async fn every_entity_still_resolves_by_its_own_name() {
    let db = PathmapDb::live_shaped().await;
    let names = corpus::live_entities();
    // Old leg: strict phrase search (frozen since t250).
    let mut old_hits = 0;
    for (_, name) in &names {
        let hits = g::search_entities(&db.db, name, 10).await.unwrap();
        if hits.iter().any(|e| &e.name == name) {
            old_hits += 1;
        }
    }
    // New leg: named seeds.
    let mut new_hits = 0;
    let mut legs_seen = std::collections::BTreeSet::new();
    for (_, name) in &names {
        let seeds = g::resolve_seeds(&db.db, name, 32).await.unwrap();
        if let Some(hit) = seeds.iter().find(|s| &s.entity.name == name) {
            new_hits += 1;
            legs_seen.insert(hit.leg);
        }
    }
    println!(
        "self-name resolution: old(strict) {old_hits}/{} -> new {new_hits}/{} legs={legs_seen:?}",
        names.len(),
        names.len()
    );
    assert_eq!(old_hits, names.len(), "the old leg resolves every own name");
    assert_eq!(new_hits, names.len(), "so must the new one (no regression)");
}

#[tokio::test]
async fn the_live_query_frame_is_within_the_new_legs_reach() {
    let db = PathmapDb::live_shaped().await;
    let mut old_nonempty = 0;
    let mut new_nonempty = 0;
    let mut per_query = Vec::new();
    for q in LIVE_QUERY_FRAME {
        let old = g::search_entities(&db.db, q, 10).await.unwrap();
        let new = g::resolve_seeds(&db.db, q, 32).await.unwrap();
        if !old.is_empty() {
            old_nonempty += 1;
        }
        if !new.is_empty() {
            new_nonempty += 1;
        }
        per_query.push((*q, old.len(), new.len()));
    }
    println!("frame: {} queries", LIVE_QUERY_FRAME.len());
    for (q, o, n) in &per_query {
        println!("   strict={o:<3} resolved={n:<3} {q:?}");
    }
    println!("non-empty: old(strict) {old_nonempty} -> new {new_nonempty}");
    // The old leg's live reading was 1/23 (only the literal `ruagent`).
    assert!(
        new_nonempty >= 13,
        "G1 target: >=13/23 queries get a seed, got {new_nonempty}"
    );
    assert!(
        new_nonempty > old_nonempty,
        "the new legs must strictly beat the frozen strict phrase leg"
    );
}

#[tokio::test]
async fn cjk_substrings_resolve_through_the_store_helpers() {
    let db = PathmapDb::live_shaped().await;
    // 2-char Han inside a 4-char name: only the store's CJK vocabulary + LIKE
    // can reach it (unicode61 keeps a Han run as ONE token).
    for (q, want) in [
        ("潜艇", "蓝鲸潜艇"),
        ("蓝鲸", "蓝鲸潜艇"),
        ("银河麒麟", "银河麒麟 V10"),
    ] {
        let seeds = g::resolve_seeds(&db.db, q, 32).await.unwrap();
        let names: Vec<&str> = seeds.iter().map(|s| s.entity.name.as_str()).collect();
        println!("CJK {q:?} -> {names:?}");
        assert!(names.contains(&want), "CJK probe {q:?} must reach {want:?}");
        assert!(
            seeds.iter().all(|s| s.leg != g::SeedLeg::VectorNearest),
            "no embedder is wired here, so the vector leg must not claim a hit"
        );
    }
    // The query CONTAINING the entity name (reverse direction).
    let seeds = g::resolve_seeds(&db.db, "蓝鲸潜艇是什么", 32)
        .await
        .unwrap();
    assert!(
        seeds.iter().any(|s| s.entity.name == "蓝鲸潜艇"),
        "an entity name inside the query must resolve"
    );
    // A Han term that names nothing in the graph stays empty -- the leg must not
    // invent a hit to look busy.
    let seeds = g::resolve_seeds(&db.db, "改键", 32).await.unwrap();
    assert!(
        seeds.is_empty(),
        "改键 is not an entity in the frozen corpus: {seeds:?}"
    );
}

#[tokio::test]
async fn each_leg_reports_its_own_count_including_zero() {
    let db = PathmapDb::live_shaped().await;
    // A query with nothing in the graph: every leg is 0 and the reason is NoSeed.
    let ev = g::retrieve(&db.db, &g::GraphQuery::for_text("zzz-nothing-here"))
        .await
        .unwrap();
    assert_eq!(ev.seeds.len(), 0);
    assert_eq!(ev.stats.empty_reason, Some(g::EmptyReason::NoSeed));
    assert_eq!(ev.stats.seeds_by_leg.len(), g::SeedLeg::ALL.len());
    for (_, n) in &ev.stats.seeds_by_leg {
        assert_eq!(*n, 0);
    }
    // An exact name: the strongest leg, and the weaker legs still report 0.
    let ev = g::retrieve(&db.db, &g::GraphQuery::for_text("ruagent"))
        .await
        .unwrap();
    let by_leg: std::collections::HashMap<_, _> = ev.stats.seeds_by_leg.iter().copied().collect();
    assert_eq!(by_leg[&g::SeedLeg::ExactName], 1);
    assert_eq!(by_leg[&g::SeedLeg::VectorNearest], 0);
    println!("seeds_by_leg for \"ruagent\": {:?}", ev.stats.seeds_by_leg);
}

#[tokio::test]
async fn alias_table_hits_are_merged_into_the_one_object() {
    let db = PathmapDb::live_shaped().await;
    let acp = db.entity_id("ACP").await;
    // Before: the phrase reaches the entity's SUMMARY (its description mentions
    // "protocol"), never the alias table.
    let before = g::resolve_seeds(&db.db, "Zed Protocol", 32).await.unwrap();
    println!(
        "before alias: {:?}",
        before
            .iter()
            .map(|s| (s.leg.as_str(), s.entity.name.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        before.iter().all(|s| s.leg != g::SeedLeg::AliasTable),
        "no alias is registered yet"
    );
    assert!(
        g::add_alias(&db.db, acp, "Zed Protocol", "human")
            .await
            .unwrap()
    );
    let after = g::resolve_seeds(&db.db, "Zed Protocol", 32).await.unwrap();
    let hit = after
        .iter()
        .find(|s| s.entity.id == acp)
        .expect("the alias resolves to the entity the alias was attached to");
    assert_eq!(hit.leg, g::SeedLeg::AliasTable);
    println!(
        "after alias: acp#{} = {:?} via {}",
        hit.entity.id,
        hit.entity.name,
        hit.leg.as_str()
    );
    // The live graph has BOTH `ACP`(#2) and `Agent Client Protocol (ACP)`(#21):
    // the alias lands on the row it was attached to, and merging those two rows
    // is a DECISION (see the resolution test), not something an alias does.
    assert_eq!(hit.entity.name, "ACP");
}
