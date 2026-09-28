//! G8: the community layer, and G7's named-object-set readings.
//!
//! G8 is split honestly: the STRUCTURAL half (a deterministic partition that
//! covers the non-isolated entities, with lazily-written summaries) is measured
//! here; the QUALITY half (GraphRAG's comprehensiveness/diversity/empowerment
//! pairwise verdict against a no-community baseline) needs an LLM judge and is
//! reported as not_measured by the implementation report, not silently dropped.

use ruagent_graph as g;

mod fixture;
use fixture::{PathmapDb, json_file};

#[tokio::test]
async fn a_level_is_unbuilt_until_it_is_built() {
    let db = PathmapDb::live_shaped().await;
    assert_eq!(
        g::communities(&db.db, 0).await.unwrap(),
        None,
        "\"never built\" and \"built and empty\" must stay distinguishable"
    );
    let build = g::build_communities(&db.db, 0).await.unwrap();
    println!(
        "level 0: communities={} covered={} non_isolated={} split_by_modularity={}",
        build.communities, build.entities_covered, build.non_isolated, build.split_by_modularity
    );
    assert!(
        build.communities >= 3,
        "the live graph has 3 non-trivial components"
    );
    assert_eq!(build.entities_covered, build.non_isolated);
    assert!(
        build.non_isolated > 0 && build.non_isolated < 63,
        "20 of 63 live entities are isolated and have no community: the coverage \
         denominator is the non-isolated count, not the entity count"
    );
    let coverage = g::community_coverage(&db.db, 0).await.unwrap();
    println!("coverage = {coverage:?}");
    let ratio = coverage.0 as f64 / coverage.1.max(1) as f64;
    assert!(
        ratio >= 0.90,
        "G8 target: >=90% of non-isolated entities, got {ratio:.4}"
    );
}

#[tokio::test]
async fn the_partition_is_deterministic_and_the_giant_component_is_split() {
    let db = PathmapDb::live_shaped().await;
    let a = g::build_communities(&db.db, 0).await.unwrap();
    let first = g::communities(&db.db, 0).await.unwrap().unwrap();
    let b = g::build_communities(&db.db, 0).await.unwrap();
    let second = g::communities(&db.db, 0).await.unwrap().unwrap();
    assert_eq!(a, b, "a rebuild of the same data must be the same build");
    assert_eq!(first, second, "and the same partition, member for member");
    let sizes: Vec<usize> = first.iter().map(|c| c.entity_ids.len()).collect();
    println!(
        "partition: {} communities, sizes {sizes:?}, split_by_modularity={}",
        first.len(),
        a.split_by_modularity
    );
    // One entity belongs to exactly one community at a level.
    let mut all: Vec<i64> = first.iter().flat_map(|c| c.entity_ids.clone()).collect();
    let n = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(
        n,
        all.len(),
        "no entity is in two communities at the same level"
    );

    // Level 1 is the whole graph, and level 0 hangs off it (a hierarchy).
    let l1 = g::build_communities(&db.db, 1).await.unwrap();
    assert_eq!(l1.communities, 1);
    let parents: Vec<Option<i64>> = g::communities(&db.db, 0)
        .await
        .unwrap()
        .unwrap()
        .iter()
        .map(|c| c.parent)
        .collect();
    assert!(
        parents.iter().all(|p| p.is_some()),
        "level 0 is re-parented to level 1"
    );
    println!("level 1 id -> children: {parents:?}");
    let of = g::communities_of(&db.db, 1).await.unwrap();
    println!(
        "communities_of(#1) = {:?}",
        of.iter().map(|c| (c.id, c.level)).collect::<Vec<_>>()
    );
    assert!(
        !of.is_empty(),
        "the direction retrieval needs: seed -> its communities"
    );

    // G8's FALSIFIABLE structural indicators (t38, RV-C2-7 / RVC-9).
    //
    // The old spec target was "coverage >= 90% of the non-isolated entities",
    // which `coverage` itself already asserts (covered == non_isolated), so it
    // could never fail -- a target that cannot fail carries no information. The
    // structural side is therefore a PRECONDITION here (build idempotent,
    // partition total, hierarchy attached, summary lazy -- all asserted above
    // and below), and the falsifiable claims are these two, both of which CAN
    // go red on real data:
    //
    //   * no single community swallows the graph (max <= 50% of covered);
    //   * the isolated-entity share stays bounded (<= 40% of all entities),
    //     because isolated nodes are exactly the ones no community covers.
    let covered: usize = sizes.iter().sum();
    let largest = sizes.iter().copied().max().unwrap_or(0);
    let all_entities: usize = db
        .db
        .call_flat(|conn| {
            conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get::<_, i64>(0))
        })
        .await
        .unwrap() as usize;
    let isolated = all_entities.saturating_sub(covered);
    let largest_share = largest as f64 / covered as f64;
    let isolated_share = isolated as f64 / all_entities as f64;
    println!(
        "G8 falsifiable structure: largest community {largest}/{covered} = {largest_share:.4} (target <= 0.50) | isolated {isolated}/{all_entities} = {isolated_share:.4} (target <= 0.40)"
    );
    assert!(
        largest_share <= 0.50,
        "no community may hold more than half of the covered entities, got {largest}/{covered}"
    );
    assert!(
        isolated_share <= 0.40,
        "too many entities are outside every community: {isolated}/{all_entities}"
    );
}

#[tokio::test]
async fn a_summary_may_be_absent_because_summarising_is_deferred() {
    let db = PathmapDb::live_shaped().await;
    g::build_communities(&db.db, 0).await.unwrap();
    let cs = g::communities(&db.db, 0).await.unwrap().unwrap();
    assert!(
        cs.iter().all(|c| c.summary.is_none()),
        "building the partition spends no LLM call"
    );
    let id = cs[0].id;
    assert!(
        g::set_community_summary(&db.db, id, "the ruagent platform and its runtimes")
            .await
            .unwrap()
    );
    let cs = g::communities(&db.db, 0).await.unwrap().unwrap();
    let with = cs.iter().filter(|c| c.summary.is_some()).count();
    println!("summaries written: {with} of {}", cs.len());
    assert_eq!(with, 1);
    assert!(
        !g::set_community_summary(&db.db, 999_999, "ghost")
            .await
            .unwrap()
    );
}

// ---------------------------------------------------------------------------
// G7: extraction quality against a NAMED object set with HUMAN labels.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_gold_object_sets_cover_every_row_they_claim_to() {
    let snap = fixture::snapshot();
    let current: Vec<i64> = snap
        .edges
        .iter()
        .filter(|e| e.invalid_at.is_none())
        .map(|e| e.id)
        .collect();
    let edges = json_file("edges.json")["edges"].clone();
    let labeled: Vec<i64> = edges
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.parse::<i64>().unwrap())
        .collect();
    assert_eq!(
        current.len(),
        labeled.len(),
        "every current edge must be labeled: an unlabeled row is an unmeasured row"
    );
    for id in &current {
        assert!(labeled.contains(id), "edge {id} has no label");
    }
    let ontology = json_file("ontology.json")["relations"].clone();
    let names: std::collections::BTreeSet<String> =
        snap.edges.iter().map(|e| e.relation.clone()).collect();
    for n in &names {
        assert!(
            ontology.as_object().unwrap().contains_key(n),
            "relation {n} has no conformance label"
        );
    }
    println!(
        "gold covers {} current edges and {} relation names",
        current.len(),
        names.len()
    );
}

#[tokio::test]
async fn the_write_side_dedupes_the_duplicate_facts_and_refuses_non_relations() {
    // The frozen 60 current edges are REPLAYED through the real writer
    // (`upsert_fact`) in id order, into a database that holds the same 63
    // entities and NO edges. So the surviving set is what today's write path
    // would have stored, and the labels are the same human labels as before:
    // nothing about the criterion or the annotation changes.
    let snapshot = fixture::snapshot();
    let db = PathmapDb::live_entities_only().await;
    let gold = json_file("edges.json")["edges"].clone();
    let label_of = |src: i64, dst: i64, relation: &str| -> Option<String> {
        gold.as_object().unwrap().values().find_map(|v| {
            if v["src"].as_i64() == Some(src)
                && v["dst"].as_i64() == Some(dst)
                && v["relation"].as_str() == Some(relation)
            {
                v["label"].as_str().map(str::to_string)
            } else {
                None
            }
        })
    };
    let mut current: Vec<&fixture::SnapEdge> = snapshot
        .edges
        .iter()
        .filter(|e| e.invalid_at.is_none())
        .collect();
    current.sort_by_key(|e| e.id);

    let mut written = 0;
    let mut dupes = 0;
    let mut refused = 0;
    let mut surviving: Vec<(i64, i64, String)> = Vec::new();
    for e in &current {
        let outcome = g::upsert_fact(
            &db.db,
            e.src,
            e.dst,
            &e.relation,
            &e.fact_text,
            Some(&e.valid_at),
            g::EventTimeSource::Extracted,
            None,
        )
        .await
        .unwrap();
        match outcome {
            g::FactOutcome::Written { .. } => {
                written += 1;
                surviving.push((e.src, e.dst, e.relation.clone()));
            }
            g::FactOutcome::Duplicate {
                of,
                ref of_relation,
            } => {
                dupes += 1;
                println!(
                    "   dropped #{} {}({}->{}) as the same claim as #{} {}",
                    e.id, e.relation, e.src, e.dst, of, of_relation
                );
            }
            g::FactOutcome::RefusedNotARelation(reason) => {
                refused += 1;
                println!(
                    "   refused #{} {}({}->{}) : {reason}",
                    e.id, e.relation, e.src, e.dst
                );
            }
        }
    }
    let mut labels: std::collections::BTreeMap<String, u32> = Default::default();
    for (src, dst, relation) in &surviving {
        let l = label_of(*src, *dst, relation)
            .unwrap_or_else(|| panic!("a written edge {relation}({src}->{dst}) has no label"));
        *labels.entry(l).or_default() += 1;
    }
    let supported = *labels.get("supported").unwrap_or(&0);
    let total_surviving = surviving.len() as u32;
    let strict = supported as f64 / total_surviving as f64;
    println!(
        "G7 AFTER: replay of {} frozen current edges -> written {written}, duplicate {dupes}, refused {refused} | labels {labels:?}",
        current.len()
    );
    println!(
        "G7 AFTER: relation-level precision (strict) = {supported}/{total_surviving} = {strict:.4}  (BEFORE: 45/60 = 0.7500)"
    );
    assert!(
        dupes >= 10,
        "the family rule must collapse the duplicate facts, got {dupes}"
    );
    assert!(
        refused >= 2,
        "the two non-relation names must be refused, got {refused}"
    );
    assert!(
        strict >= 0.80,
        "G7 target: relation-level precision {strict:.4} must be >= 0.80 (criterion and labels unchanged)"
    );
    // The vocabulary also gets narrower, in the direction the spec asks for:
    // fewer distinct names and fewer object-specific ones among the survivors.
    let names_before: std::collections::BTreeSet<&str> =
        current.iter().map(|e| e.relation.as_str()).collect();
    let names_after: std::collections::BTreeSet<&str> =
        surviving.iter().map(|(_, _, r)| r.as_str()).collect();
    let ont = json_file("ontology.json")["relations"].clone();
    let ad_hoc = |names: &std::collections::BTreeSet<&str>| {
        names
            .iter()
            .filter(|n| ont[*n].as_str() != Some("controlled"))
            .count()
    };
    println!(
        "G7 ontology direction: distinct relation names {} -> {}, object-specific (ad_hoc) {} -> {}",
        names_before.len(),
        names_after.len(),
        ad_hoc(&names_before),
        ad_hoc(&names_after)
    );
    assert!(names_after.len() < names_before.len());
    assert!(ad_hoc(&names_after) < ad_hoc(&names_before));
}

#[tokio::test]
async fn edge_and_ontology_readings_are_pinned() {
    let edges = json_file("edges.json")["edges"].clone();
    let map = edges.as_object().unwrap();
    let total = map.len();
    let count = |label: &str| {
        map.values()
            .filter(|v| v["label"].as_str() == Some(label))
            .count()
    };
    let supported = count("supported");
    let duplicate = count("duplicate");
    let mislabeled = count("mislabeled");
    let unsupported = count("unsupported");
    let strict_precision = supported as f64 / total as f64;
    let lenient_precision = (supported + duplicate) as f64 / total as f64;
    println!(
        "edges n={total} supported={supported} duplicate={duplicate} mislabeled={mislabeled} unsupported={unsupported}"
    );
    println!(
        "relation-level precision (strict) = {strict_precision:.4} | endpoint-level (duplicates tolerated) = {lenient_precision:.4} | duplicate rate = {:.4} | mislabel rate = {:.4}",
        duplicate as f64 / total as f64,
        mislabeled as f64 / total as f64
    );
    assert_eq!(supported + duplicate + mislabeled + unsupported, total);
    // The readings a re-extraction must move. Pinned so a change is visible.
    assert_eq!(supported, 45);
    assert_eq!(duplicate, 11);
    assert_eq!(mislabeled, 4);
    assert_eq!(unsupported, 0);
    assert!(
        (strict_precision - 0.75).abs() < 1e-9,
        "relation-level precision is 0.75, BELOW the spec's 0.80 target: registered, not hidden"
    );

    let ont = json_file("ontology.json")["relations"].clone();
    let om = ont.as_object().unwrap();
    let ad_hoc = om.values().filter(|v| v.as_str() == Some("ad_hoc")).count();
    let controlled = om.len() - ad_hoc;
    let snapshot = fixture::snapshot();
    let singleton = {
        let mut seen: std::collections::BTreeMap<String, u32> = Default::default();
        for e in &snapshot.edges {
            *seen.entry(e.relation.clone()).or_default() += 1;
        }
        seen.values().filter(|c| **c == 1).count()
    };
    println!(
        "ontology: {} relations, controlled={controlled} ad_hoc={ad_hoc} ({:.4}), singleton-based names={singleton}",
        om.len(),
        ad_hoc as f64 / om.len() as f64
    );
    assert_eq!(controlled, 29);
    assert_eq!(ad_hoc, 6);
    assert_eq!(
        singleton, 24,
        "the measurement the spec quotes: 24/35 names occur once"
    );
}

/// The hallucination check, mechanised on a fixture where support IS checkable.
///
/// WHY a fixture: on the live graph the check is not runnable at all -- 0 of 67
/// historical edges carry a `source_episode`, so there is no transcript to check
/// the fact text against. The mechanism still has to be tested, so it is tested
/// where it can be: the same function the extraction path will call.
#[tokio::test]
async fn the_support_check_flags_every_fact_it_cannot_tie_to_the_transcript() {
    let transcript = "[09-26 10:00] User: ruagent 运行在 Windows 11 上，用 LanceDB 做向量检索。\n\
                      [09-26 10:01] Agent: 明白，已记录这两点。";
    let facts = [
        (
            "ruagent 运行在 Windows 11 上，用 LanceDB 做向量检索",
            "verbatim",
            false,
        ),
        ("ruagent 使用 LanceDB 做向量检索", "paraphrase", true),
        ("ruagent 2019 年成立，有 40 名员工", "invented", true),
    ];
    let mut flagged = Vec::new();
    for (fact, kind, _) in facts {
        let ok = supported_by(fact, transcript);
        println!("   support({kind}) = {ok}");
        if !ok {
            flagged.push(kind);
        }
    }
    // The gate must catch the invented fact (no false negative) ...
    assert!(
        flagged.contains(&"invented"),
        "an invented fact must be flagged"
    );
    // ... and it ALSO flags the paraphrase, because a lexical check cannot tell
    // an invention from a rewording. That false positive is why the live
    // hallucination rate is reported as not_measured instead of a number: the
    // mechanism is tested, the estimate is not available.
    assert!(
        flagged.contains(&"paraphrase"),
        "documented limitation: lexical support alone cannot separate rewording from invention"
    );
    assert_eq!(flagged.len(), 2);
}

/// Content-word support: every evidence-bearing word of the fact appears in the
/// transcript (case-insensitive). Words shorter than 4 characters and Han runs
/// shorter than 2 are ignored, because they are not evidence on their own.
pub fn supported_by(fact: &str, transcript: &str) -> bool {
    let hay = transcript.to_lowercase();
    let words: Vec<String> = fact
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    let mut evidence = 0;
    for w in &words {
        let han = w
            .chars()
            .any(|c| matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF));
        let keep = if han {
            w.chars().count() >= 2
        } else {
            w.chars().count() >= 4
        };
        if !keep {
            continue;
        }
        evidence += 1;
        if !hay.contains(w.as_str()) {
            return false;
        }
    }
    evidence > 0
}
