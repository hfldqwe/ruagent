//! G4: the event-time axis must carry information, and "no event time" must be
//! a DIFFERENT value from "the write clock".
//!
//! BEFORE: the frozen `add_fact` writes `event_time_source = NULL` for every row
//! (measured live: 0/67 edges carried a source, and 66/67 had valid_at ==
//! created_at within 1 second), so `retrieve` could only report the timestamp,
//! never what it meant.

use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
use ruagent_graph as g;

mod fixture;
use fixture::{PathmapDb, json_file};

#[tokio::test]
async fn the_old_writer_leaves_the_source_unknown_and_the_new_one_records_it() {
    let db = PathmapDb::live_shaped().await;
    let a = db.entity_id("ruagent").await;
    let b = db.entity_id("Codex").await;

    // BEFORE: the frozen writer. Its signature and behaviour must not change.
    let old_id = g::add_fact(
        &db.db,
        a,
        b,
        "t999-old",
        "old writer fact",
        Some("2026-01-01T00:00:00Z"),
        None,
    )
    .await
    .unwrap();
    let src = event_time_source_of(&db, old_id).await;
    assert_eq!(
        src, None,
        "the frozen writer must NOT start claiming a source"
    );

    // AFTER: the extraction writer says where the timestamp came from.
    let ext_id = g::add_fact_with_source(
        &db.db,
        a,
        b,
        "t999-extracted",
        "extracted fact",
        Some("2026-01-02T00:00:00Z"),
        g::EventTimeSource::Extracted,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        event_time_source_of(&db, ext_id).await.as_deref(),
        Some("extracted")
    );
    let rec_id = g::add_fact_with_source(
        &db.db,
        a,
        b,
        "t999-recorded",
        "recorded fact",
        None,
        g::EventTimeSource::Extracted, // a caller mistake: None has no event time
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        event_time_source_of(&db, rec_id).await.as_deref(),
        Some("recorded"),
        "valid_at=None means the clock stamped it; claiming 'extracted' would be a lie"
    );

    // The fact identity is recorded, and it is stable + whitespace/case-insensitive.
    let h1 = fact_hash_of(&db, rec_id).await;
    assert_eq!(h1.len(), 16);
    assert_eq!(h1, g::fact_hash("recorded fact"));
    assert_ne!(h1, g::fact_hash("recorded facts"));
    assert_eq!(
        g::fact_hash("Alice  works at Acme."),
        g::fact_hash("alice works at acme."),
        "whitespace and case are not part of a fact's identity"
    );
    assert_eq!(
        g::fact_hash("  recorded\tfact  "),
        h1,
        "so the same fact re-extracted with different spacing is the same fact"
    );

    // And retrieval says which of the two it is looking at.
    let ev = g::retrieve(
        &db.db,
        &g::GraphQuery {
            hops: 1,
            beam: 64,
            max_paths: 64,
            max_facts: 64,
            ..g::GraphQuery::for_text("ruagent")
        },
    )
    .await
    .unwrap();
    let state_of = |id: i64| {
        ev.paths
            .iter()
            .flat_map(|p| p.edges.iter())
            .find(|e| e.edge_id == id)
            .map(|e| format!("{:?}", e.temporal))
    };
    println!(
        "temporal states: extracted={:?} recorded={:?} old(null)={:?}",
        state_of(ext_id).as_deref(),
        state_of(rec_id).as_deref(),
        state_of(old_id).as_deref()
    );
    assert!(state_of(ext_id).unwrap().contains("Current"));
    assert!(state_of(rec_id).unwrap().contains("RecordedAtOnly"));
    assert!(
        state_of(old_id).unwrap().contains("RecordedAtOnly"),
        "an unknown source cannot be reported as a real event time"
    );
}

async fn event_time_source_of(db: &PathmapDb, id: i64) -> Option<String> {
    db.db
        .call_flat(move |conn| {
            conn.query_row(
                "SELECT event_time_source FROM entity_edges WHERE id = ?1",
                [id],
                |r| r.get::<_, Option<String>>(0),
            )
        })
        .await
        .unwrap()
}

async fn fact_hash_of(db: &PathmapDb, id: i64) -> String {
    db.db
        .call_flat(move |conn| {
            conn.query_row(
                "SELECT fact_hash FROM entity_edges WHERE id = ?1",
                [id],
                |r| r.get::<_, Option<String>>(0),
            )
        })
        .await
        .unwrap()
        .unwrap_or_default()
}

#[tokio::test]
async fn the_same_instant_in_three_spellings_gives_the_same_evidence() {
    // RVC-1: an instant is a TIME, not a string. The pre-repair code compared
    // `valid_at <= as_of` as TEXT (retrieve.rs:602 and :719-726), so the three
    // spellings below -- the same moment -- produced different evidence sets.
    // This test asks the same question three ways for three different instants
    // (9 calls) and requires the SERIALIZED evidence to be byte-identical.
    use ruagent_graph::parse_ts;
    let db = PathmapDb::live_shaped().await;
    let snap = fixture::snapshot();
    let instants: [(u32, DateTime<Utc>); 3] = [
        (1, parse_ts("2026-09-13T18:34:20.791485300+00:00").unwrap()),
        (2, parse_ts("2026-09-13T18:38:15.350633200+00:00").unwrap()),
        (3, parse_ts("2026-09-27T13:58:00Z").unwrap()),
    ];
    // The OLD rule, re-executed here (not quoted from the review): text compare.
    let old_true_then = |valid_at: &str, invalid_at: Option<&str>, t: &str| -> bool {
        valid_at <= t && invalid_at.map(|i| i > t).unwrap_or(true)
    };
    let parsed_true_then = |valid_at: &str, invalid_at: Option<&str>, t: DateTime<Utc>| -> bool {
        parse_ts(valid_at).map(|v| v <= t).unwrap_or(false)
            && invalid_at
                .map(|i| parse_ts(i).map(|iv| iv > t).unwrap_or(true))
                .unwrap_or(true)
    };
    let mut old_rule_disagreements: Vec<(u32, [u32; 3])> = Vec::new();
    for (n, instant) in instants {
        let one = |name: &str| g::GraphQuery {
            hops: 1,
            beam: 128,
            max_paths: 128,
            max_facts: 128,
            as_of: Some(name.to_string()),
            ..g::GraphQuery::for_text("ruagent")
        };
        let canonical = instant.to_rfc3339_opts(SecondsFormat::AutoSi, false);
        let z = instant.to_rfc3339_opts(SecondsFormat::AutoSi, true);
        let east = instant.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
        let plus8 = east.to_rfc3339_opts(SecondsFormat::AutoSi, false);
        let a = g::retrieve(&db.db, &one(&canonical)).await.unwrap();
        let b = g::retrieve(&db.db, &one(&z)).await.unwrap();
        let c = g::retrieve(&db.db, &one(&plus8)).await.unwrap();
        let ja = serde_json::to_string(&a).unwrap();
        let jb = serde_json::to_string(&b).unwrap();
        let jc = serde_json::to_string(&c).unwrap();
        println!(
            "instant {n}: canonical {canonical:?} | Z {z:?} | +08:00 {plus8:?} -> facts {} / {} / {} | bytes {} / {} / {} | identical {}",
            a.fact_count(),
            b.fact_count(),
            c.fact_count(),
            ja.len(),
            jb.len(),
            jc.len(),
            ja == jb && jb == jc
        );
        assert_eq!(ja, jb, "the Z spelling of instant {n} changed the evidence");
        assert_eq!(
            jb, jc,
            "the +08:00 spelling of instant {n} changed the evidence"
        );
        assert!(
            a.fact_count() > 0,
            "instant {n} must be a non-vacuous reading"
        );

        // The defect's SIZE, measured on the same objects with the old rule
        // inline: how many of the 67 edges does each spelling judge backwards?
        let mut dis = [0u32; 3];
        for e in &snap.edges {
            let truth = parsed_true_then(&e.valid_at, e.invalid_at.as_deref(), instant);
            for (slot, form) in [&canonical, &z, &plus8].into_iter().enumerate() {
                if old_true_then(&e.valid_at, e.invalid_at.as_deref(), form) != truth {
                    dis[slot] += 1;
                }
            }
        }
        println!(
            "   old TEXT rule vs the parsed instant: canonical {}/67, Z-form {}/67, +08:00-form {}/67 edges judged backwards",
            dis[0], dis[1], dis[2]
        );
        // The guarantee is the byte-identity asserted above. These counts are the
        // DEFECT's size: the canonical spelling agrees (0) -- which is exactly why
        // the as-of gold, written entirely in canonical form, could not see it --
        // while the other two spellings of the SAME instant do not.
        assert_eq!(dis[0], 0, "the canonical spelling is the one the gold used");
        if n <= 2 {
            assert!(
                dis[1] > 0 || dis[2] > 0,
                "instant {n}: the old rule must be visibly spelling-dependent"
            );
        }
        old_rule_disagreements.push((n, dis));
    }

    // A text that is NOT an instant is refused loudly (the alternative was to
    // compare it as a string and answer a question nobody asked).
    let bad = g::retrieve(
        &db.db,
        &g::GraphQuery {
            as_of: Some("not-a-time".to_string()),
            ..g::GraphQuery::for_text("ruagent")
        },
    )
    .await;
    println!(
        "as_of=\"not-a-time\" -> {:?}",
        bad.as_ref().err().map(|e| e.to_string())
    );
    assert!(
        bad.is_err(),
        "a non-instant as_of must fail, not silently compare"
    );
    println!(
        "RVC-1 summary: old text rule judged backwards per instant (canonical, Z, +08:00) = {old_rule_disagreements:?}"
    );
}

#[tokio::test]
async fn a_fact_true_then_and_superseded_now_is_reported_as_true_as_of() {
    // RVC-10: `TemporalStatus` claims FOUR states; before this test only three
    // of them had any assertion anywhere (grep TrueAsOf in tests/ = 0 hits), so
    // the third state lived only in the implementation.
    use ruagent_graph::parse_ts;
    let db = PathmapDb::live_shaped().await;
    let snap = fixture::snapshot();
    let instant = parse_ts("2026-09-13T18:34:20.791485300+00:00").unwrap();
    // Edges that were true at that instant and were invalidated later.
    let expected_true_as_of: Vec<i64> = snap
        .edges
        .iter()
        .filter(|e| e.invalid_at.is_some())
        .filter(|e| {
            parse_ts(&e.valid_at).map(|v| v <= instant).unwrap_or(false)
                && parse_ts(e.invalid_at.as_deref().unwrap())
                    .map(|iv| iv > instant)
                    .unwrap_or(false)
        })
        .map(|e| e.id)
        .collect();
    assert!(
        !expected_true_as_of.is_empty(),
        "the fixture must contain at least one edge that was true then"
    );
    let ev = g::retrieve(
        &db.db,
        &g::GraphQuery {
            hops: 1,
            beam: 128,
            max_paths: 128,
            max_facts: 128,
            as_of: Some(instant.to_rfc3339_opts(SecondsFormat::AutoSi, false)),
            ..g::GraphQuery::for_text("ruagent")
        },
    )
    .await
    .unwrap();
    let mut saw = 0;
    for p in &ev.paths {
        for e in &p.edges {
            if expected_true_as_of.contains(&e.edge_id) {
                assert!(
                    matches!(e.temporal, g::TemporalStatus::TrueAsOf { .. }),
                    "edge {} was true at that instant and invalidated later, so it is TrueAsOf, not {:?}",
                    e.edge_id,
                    e.temporal
                );
                saw += 1;
            }
        }
    }
    let lines = ev.lines();
    let rendered = lines.iter().filter(|l| l.contains("true_as_of")).count();
    println!(
        "TrueAsOf: {} of the {} true-then edges came back in the evidence; {} of {} rendered lines say true_as_of",
        saw,
        expected_true_as_of.len(),
        rendered,
        lines.len()
    );
    assert!(saw > 0, "the true-then edges must be in the evidence");
    assert!(rendered > 0, "and the rendered line must say so");
}

#[tokio::test]
async fn as_of_gold_returns_what_was_true_then() {
    let db = PathmapDb::live_shaped().await;
    let snap = fixture::snapshot();
    let gold = json_file("as_of.json");
    let queries = gold["queries"].as_array().unwrap();
    let mut correct = 0;
    for q in queries {
        let entity = q["entity_id"].as_i64().unwrap();
        let name = q["entity"].as_str().unwrap();
        let t = q["as_of"].as_str().unwrap();
        let expected: Vec<i64> = q["expected_edge_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        let ev = g::retrieve(
            &db.db,
            &g::GraphQuery {
                hops: 1,
                beam: 128,
                max_paths: 128,
                max_facts: 128,
                as_of: Some(t.to_string()),
                ..g::GraphQuery::for_text(name)
            },
        )
        .await
        .unwrap();
        assert!(
            ev.seeds.iter().any(|s| s.entity.id == entity),
            "{name} must seed the walk"
        );
        let mut got: Vec<i64> = ev
            .paths
            .iter()
            .flat_map(|p| p.edges.iter())
            .map(|e| e.edge_id)
            .collect();
        got.sort_unstable();
        got.dedup();

        // A `retrieve` call starts from EVERY seed the query resolves, not just
        // the one we are asking about ("ruagent" also resolves 11 entities whose
        // SUMMARIES mention it). So two assertions, each precise:
        //
        // (1) the seed entity's OWN true-then facts come back exactly;
        // (2) and no edge comes back that touches none of the returned seeds --
        //     a one-hop walk that produced such an edge would have invented a hop.
        let own: Vec<i64> = {
            let mut v: Vec<i64> = expected
                .iter()
                .copied()
                .filter(|id| got.contains(id))
                .collect();
            v.sort_unstable();
            v
        };
        let seed_ids: std::collections::BTreeSet<i64> =
            ev.seeds.iter().map(|s| s.entity.id).collect();
        let by_id: std::collections::HashMap<i64, (i64, i64)> =
            snap.edges.iter().map(|e| (e.id, (e.src, e.dst))).collect();
        let orphans: Vec<i64> = got
            .iter()
            .copied()
            .filter(|id| {
                let (s, d) = by_id[id];
                !seed_ids.contains(&s) && !seed_ids.contains(&d)
            })
            .collect();
        let ok = own == expected && orphans.is_empty();
        println!(
            "as_of {} at {}: own {}/{} edges, {} seeds, orphan edges {:?} -> {}{}",
            q["entity"].as_str().unwrap(),
            t,
            own.len(),
            expected.len(),
            seed_ids.len(),
            orphans,
            if ok { "MATCH" } else { "MISMATCH" },
            if ok {
                String::new()
            } else {
                format!(
                    " | missing {:?}",
                    expected
                        .iter()
                        .filter(|i| !got.contains(i))
                        .collect::<Vec<_>>()
                )
            }
        );
        assert!(orphans.is_empty(), "every returned edge must touch a seed");
        if ok {
            correct += 1;
        }
    }
    assert_eq!(
        correct,
        queries.len(),
        "G4: the as-of gold must be answered exactly ({correct}/{} correct)",
        queries.len()
    );
}

#[tokio::test]
async fn superseded_edges_are_history_only_when_asked_for() {
    let db = PathmapDb::live_shaped().await;
    let superseded_id: i64 = 18; // ruagent -uses-> LanceDB (English), invalidated 09-13
    let q = |include: bool| g::GraphQuery {
        hops: 1,
        beam: 64,
        max_paths: 64,
        max_facts: 64,
        include_superseded: include,
        ..g::GraphQuery::for_text("ruagent")
    };
    let hidden = g::retrieve(&db.db, &q(false)).await.unwrap();
    assert!(
        !hidden
            .paths
            .iter()
            .flat_map(|p| p.edges.iter())
            .any(|e| e.edge_id == superseded_id),
        "a superseded edge is not current evidence"
    );
    let shown = g::retrieve(&db.db, &q(true)).await.unwrap();
    let hit = shown
        .paths
        .iter()
        .flat_map(|p| p.edges.iter())
        .find(|e| e.edge_id == superseded_id)
        .expect("asked for history, the history is there");
    println!("superseded edge {} state = {:?}", hit.edge_id, hit.temporal);
    assert!(format!("{:?}", hit.temporal).contains("Superseded"));
}
