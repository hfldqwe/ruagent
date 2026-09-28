//! G5: bounded multi-hop retrieval, with the gold path set frozen from the live
//! graph by an INDEPENDENT implementation (python BFS, `tests/gold/multihop.json`).
//!
//! BEFORE/AFTER IN ONE RUN: the "before" leg is `current_facts` -- the pre-gen2
//! retrieval, one hop, unchanged -- executed on the same fixture. Its recall of a
//! 2-or-3-hop gold path is 0 by construction, which is exactly the measured
//! complaint (the entity leg answered "what do I know about X" and never
//! "how is X connected to Y").

use ruagent_graph as g;

mod fixture;
use fixture::{PathmapDb, json_array};

fn gold() -> Vec<serde_json::Value> {
    json_array("multihop.json")
}

#[tokio::test]
async fn gold_paths_recall_at_12() {
    let db = PathmapDb::live_shaped().await;
    let entries = gold();
    let reachable: Vec<_> = entries
        .iter()
        .filter(|e| e["kind"] == "reachable")
        .collect();
    let mut old_found = 0;
    let mut found2 = 0;
    let mut found3 = 0;
    let mut cuts = std::collections::BTreeMap::new();
    let mut fact_counts = Vec::new();
    for e in &reachable {
        let seed = e["seed"].as_str().unwrap();
        let target = e["target"].as_str().unwrap();
        let seed_id = db.entity_id(seed).await;
        let target_id = db.entity_id(target).await;
        // BEFORE: the old retrieval, one hop of current facts.
        let old = g::current_facts(&db.db, seed_id).await.unwrap();
        if old.iter().any(|f| f.src == target_id || f.dst == target_id) {
            old_found += 1;
        }
        // AFTER: the bounded walk.
        if target_reached(&db, seed, target, 2).await {
            found2 += 1;
        }
        let ev = g::retrieve(
            &db.db,
            &g::GraphQuery {
                hops: 3,
                ..g::GraphQuery::for_text(seed)
            },
        )
        .await
        .unwrap();
        if ev
            .paths
            .iter()
            .any(|p| p.nodes.last().map(|n| n.id) == Some(target_id))
        {
            found3 += 1;
        }
        *cuts
            .entry(format!("{:?}", ev.stats.truncated_by))
            .or_insert(0) += 1;
        fact_counts.push(ev.fact_count());
    }
    fact_counts.sort_unstable();
    let median = fact_counts[fact_counts.len() / 2];
    println!(
        "gold reachable n={} | old(1-hop current_facts) reached {old_found} | new hops=2 reached {found2} | new hops=3 reached {found3}",
        reachable.len()
    );
    println!("truncated_by distribution over the gold seeds: {cuts:?}");
    println!(
        "facts per query: median {median} min {} max {}",
        fact_counts[0],
        fact_counts[fact_counts.len() - 1]
    );
    let recall2 = found2 as f64 / reachable.len() as f64;
    let recall3 = found3 as f64 / reachable.len() as f64;
    println!("path-recall@12: hops=2 {recall2:.4} hops=3 {recall3:.4}");
    assert_eq!(
        old_found, 0,
        "the old 1-hop leg cannot find a 2-hop gold path"
    );
    assert!(
        recall2 >= 0.60,
        "G5 target: path-recall@12 >= 0.60 at the DEFAULT hops=2, got {recall2:.4}"
    );
}

async fn target_reached(db: &PathmapDb, seed: &str, target: &str, hops: u32) -> bool {
    let target_id = db.entity_id(target).await;
    let ev = g::retrieve(
        &db.db,
        &g::GraphQuery {
            hops,
            ..g::GraphQuery::for_text(seed)
        },
    )
    .await
    .unwrap();
    ev.paths
        .iter()
        .any(|p| p.nodes.last().map(|n| n.id) == Some(target_id))
}

#[tokio::test]
async fn pairs_beyond_the_hop_cap_are_not_invented() {
    let db = PathmapDb::live_shaped().await;
    let mut checked = 0;
    for e in gold().iter().filter(|e| e["kind"] == "beyond_cap") {
        let seed = e["seed"].as_str().unwrap();
        let target = e["target"].as_str().unwrap();
        let hops = e["hops"].as_u64().unwrap() as u32;
        assert!(hops >= 4);
        // At the ceiling the walk must NOT claim the pair: no <=3-hop path exists.
        assert!(
            !target_reached(&db, seed, target, 3).await,
            "{seed} -> {target} is {hops} hops; it must not appear at hops=3"
        );
        checked += 1;
    }
    println!("beyond-cap negatives checked: {checked}");
    assert!(checked >= 3);
}

#[tokio::test]
async fn a_returned_path_says_how_it_got_there() {
    let db = PathmapDb::live_shaped().await;
    let ev = g::retrieve(
        &db.db,
        &g::GraphQuery {
            hops: 3,
            ..g::GraphQuery::for_text("ruagent")
        },
    )
    .await
    .unwrap();
    assert!(!ev.paths.is_empty(), "the hub has neighbours");
    for p in &ev.paths {
        assert_eq!(p.hops as usize, p.edges.len(), "hops == edges");
        assert_eq!(p.nodes.len(), p.edges.len() + 1, "nodes == edges + 1");
        let mut seen = std::collections::BTreeSet::new();
        for n in &p.nodes {
            assert!(seen.insert(n.id), "a walk never repeats a node");
        }
        for (i, e) in p.edges.iter().enumerate() {
            let a = p.nodes[i].id;
            let b = p.nodes[i + 1].id;
            assert!(
                (e.src == a && e.dst == b) || (e.src == b && e.dst == a),
                "edge {} does not join {} and {}",
                e.edge_id,
                a,
                b
            );
            // Every edge carries its own provenance slot, even when unknown.
            assert!(!e.source.recorded_at.is_empty());
        }
        // The score is recomputable from the factors a reader is given.
        let product: f32 = p.why.factors.iter().product();
        assert!(
            (product - p.score).abs() < 1e-6,
            "score {} != product of factors {product}",
            p.score
        );
        assert_eq!(p.why.factors.len(), p.edges.len() + 1, "seed + one per hop");
    }
    // The rendered line carries path + hops + temporal + source (spec §D.2).
    let lines = ev.lines();
    println!("first evidence line: {}", lines[0]);
    assert!(lines[0].contains("hops="));
    assert!(lines[0].contains("valid_at="));
    assert!(lines[0].contains("state="));
    assert!(lines[0].contains("edges="));
}

#[tokio::test]
async fn the_same_query_serializes_byte_identically_across_runs() {
    // RVC-4: a query that hits PARALLEL edges must still be reproducible. The
    // walk collects candidates from a HashMap (`next.into_values()`), so a tie
    // between equal-scoring edges was resolved by that map's iteration order --
    // and the ranking key `(score, hops, node ids)` cannot separate two edges
    // that join the SAME pair. Measured by RV-C before the fix: 6 runs, 6
    // different JSON documents, the same slot rotating between edges 49/52/53.
    let db = PathmapDb::live_shaped().await;
    let q = g::GraphQuery {
        hops: 2,
        beam: 8,
        max_paths: 12,
        max_facts: 24,
        ..g::GraphQuery::for_text("DeepSeek Harness")
    };
    // The parallel edges: same endpoints, same score, different relations.
    let snap = fixture::snapshot();
    let mut parallel: std::collections::BTreeMap<(i64, i64), Vec<i64>> = Default::default();
    for e in snap.edges.iter().filter(|e| e.invalid_at.is_none()) {
        let key = (e.src.min(e.dst), e.src.max(e.dst));
        parallel.entry(key).or_default().push(e.id);
    }
    let biggest = parallel
        .values()
        .max_by_key(|v| v.len())
        .cloned()
        .unwrap_or_default();
    println!("the largest parallel-edge group in the fixture: {biggest:?}");
    assert!(
        biggest.len() >= 3,
        "the fixture must contain >=3 parallel edges"
    );

    let mut json = Vec::new();
    for _ in 0..6 {
        let ev = g::retrieve(&db.db, &q).await.unwrap();
        json.push(serde_json::to_string(&ev).unwrap());
    }
    let first = &json[0];
    let identical = json.iter().all(|j| j == first);
    println!(
        "6 runs of the same query: lengths {:?} | byte-identical: {identical}",
        json.iter().map(|j| j.len()).collect::<Vec<_>>()
    );
    for (i, j) in json.iter().enumerate() {
        assert_eq!(&json[0], j, "run {i} differs from run 0");
    }
    // WHY the old key was not enough, shown on the real tie group: the key
    // `(score, hops, node ids)` is not injective, the extended key is.
    let ev = g::retrieve(&db.db, &q).await.unwrap();
    let mut old_keys: Vec<(u64, usize, Vec<i64>)> = Vec::new();
    let mut new_keys: Vec<(u64, usize, Vec<i64>, Vec<i64>)> = Vec::new();
    for p in &ev.paths {
        let score = (p.score * 1_000_000.0) as u64;
        let nodes: Vec<i64> = p.nodes.iter().map(|n| n.id).collect();
        let edges: Vec<i64> = p.edges.iter().map(|e| e.edge_id).collect();
        old_keys.push((score, p.edges.len(), nodes.clone()));
        new_keys.push((score, p.edges.len(), nodes, edges));
    }
    let old_unique = {
        let mut v = old_keys.clone();
        v.sort();
        v.dedup();
        v.len()
    };
    let new_unique = {
        let mut v = new_keys.clone();
        v.sort();
        v.dedup();
        v.len()
    };
    println!(
        "ranking key: {} paths -> (score,hops,nodes) distinct {old_unique}, + edge ids distinct {new_unique}",
        ev.paths.len()
    );
    assert!(old_unique < new_unique || new_unique == ev.paths.len());
    assert_eq!(
        new_unique,
        ev.paths.len(),
        "the extended key is a total order here"
    );
}

#[tokio::test]
async fn every_rendered_hop_uses_the_edges_stored_direction() {
    // RVC-5: the walk is UNDIRECTED, so a hop may traverse an edge backwards. The
    // first version always rendered `nodes[i] -relation-> nodes[i+1]` and printed
    // `微信 -uses-> 用户19410` for an edge stored as `用户19410 -uses-> 微信` --
    // a line headed for injection into an agent, asserting the reverse fact.
    let db = PathmapDb::live_shaped().await;
    let mut reversed_hops = 0;
    let mut paths_checked = 0;
    for question in ["微信", "DeepSeek Harness", "银河麒麟 V10", "ruagent"] {
        let ev = g::retrieve(
            &db.db,
            &g::GraphQuery {
                hops: 2,
                ..g::GraphQuery::for_text(question)
            },
        )
        .await
        .unwrap();
        let lines = ev.lines();
        let mut q_paths = 0;
        let mut q_reversed = 0;
        for (pi, p) in ev.paths.iter().enumerate() {
            paths_checked += 1;
            q_paths += 1;
            let line = &lines[pi];
            for (i, e) in p.edges.iter().enumerate() {
                let here = &p.nodes[i];
                let next = &p.nodes[i + 1];
                if e.src == here.id && e.dst == next.id {
                    let want = format!("{} -{}-> {}", here.name, e.relation, next.name);
                    assert!(
                        line.contains(&want),
                        "forward hop missing {want:?} in {line:?}"
                    );
                } else {
                    reversed_hops += 1;
                    q_reversed += 1;
                    // The chain reads `here <-rel- next`, which means "next
                    // relation here" -- exactly what the edge stores (src=next,
                    // dst=here). The buggy rendering was the other one.
                    let want = format!("{} <-{}- {}", here.name, e.relation, next.name);
                    assert!(
                        line.contains(&want),
                        "reverse hop must render as {want:?} (the edge is {} -{}-> {}), got {line:?}",
                        next.name,
                        e.relation,
                        here.name
                    );
                    // And the WRONG rendering must be absent: it would assert
                    // that `here` relates to `next`, the reverse of the fact.
                    let wrong = format!("{} -{}-> {}", here.name, e.relation, next.name);
                    assert!(
                        !line.contains(&wrong),
                        "the reversed fact is rendered: {wrong:?}"
                    );
                }
            }
        }
        println!(
            "Q {question:?}: {q_paths} paths, {q_reversed} reverse hops, all in the stored direction"
        );
    }
    println!(
        "direction check: {paths_checked} paths across 4 questions, {reversed_hops} of their hops traverse an edge backwards, all rendered in the stored direction"
    );
    assert!(
        reversed_hops > 0,
        "this test is vacuous without a reversed hop"
    );
}

#[tokio::test]
async fn every_cut_is_reported_and_the_budgets_hold() {
    let db = PathmapDb::live_shaped().await;
    let ev = g::retrieve(
        &db.db,
        &g::GraphQuery {
            hops: 3,
            beam: 2,
            max_paths: 3,
            max_facts: 2,
            ..g::GraphQuery::for_text("ruagent")
        },
    )
    .await
    .unwrap();
    println!(
        "tight budgets -> paths={} facts={} truncated_by={:?} frontiers={:?}",
        ev.paths.len(),
        ev.fact_count(),
        ev.stats.truncated_by,
        ev.stats.frontier_sizes
    );
    assert!(ev.paths.len() <= 3, "max_paths is a hard bound");
    assert!(ev.fact_count() <= 2, "max_facts is a hard bound");
    assert!(
        ev.stats.truncated_by.is_some(),
        "a cut must be REPORTED, never silent"
    );
    assert_eq!(ev.stats.paths_emitted as usize, ev.paths.len());
}

#[tokio::test]
async fn empty_results_names_why() {
    let db = PathmapDb::live_shaped().await;
    // (a) nothing in the query resolves. The words must be nonces: an ordinary
    // English word reaches the summary leg as a PREFIX (that is what a recall
    // leg is for), and then "no seed" would be the wrong diagnosis.
    let ev = g::retrieve(&db.db, &g::GraphQuery::for_text("zzqqxx-wvvvv-yyyyy"))
        .await
        .unwrap();
    println!(
        "nonce query: seeds={} reason={:?}",
        ev.seeds.len(),
        ev.stats.empty_reason
    );
    assert_eq!(ev.stats.empty_reason, Some(g::EmptyReason::NoSeed));
    // (b) an entity with no edges at all (蓝鲸潜艇 is isolated in the live graph).
    let ev = g::retrieve(&db.db, &g::GraphQuery::for_text("蓝鲸潜艇"))
        .await
        .unwrap();
    println!(
        "isolated seed: seeds={} reason={:?}",
        ev.seeds.len(),
        ev.stats.empty_reason
    );
    assert!(!ev.seeds.is_empty(), "the isolated entity still resolves");
    assert_eq!(ev.stats.empty_reason, Some(g::EmptyReason::NoEdgeFromSeeds));
    // (c) an instant before any fact existed.
    let ev = g::retrieve(
        &db.db,
        &g::GraphQuery {
            as_of: Some("2000-01-01T00:00:00Z".to_string()),
            ..g::GraphQuery::for_text("ruagent")
        },
    )
    .await
    .unwrap();
    println!(
        "as-of 2000: edges_reachable={} reason={:?}",
        ev.stats.graph_edges, ev.stats.empty_reason
    );
    assert_eq!(
        ev.stats.empty_reason,
        Some(g::EmptyReason::AsOfBeforeAnyFact)
    );
}
