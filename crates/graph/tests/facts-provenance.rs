//! Where a fact came from, as a reader can get it back (issues #49 and #52, the
//! backend half).
//!
//! TWO QUESTIONS, ONE ROW SHAPE. #49 asks the WRITE side to say where `valid_at`
//! came from (`event_time_source`), and #52 asks the READ side to name the source
//! session of an extracted fact. They are pinned together here because the read
//! answer is only checkable against rows a real write path produced: every fact
//! below is written by the code under test (`add_fact`, `add_fact_with_source`,
//! `apply_extraction`), never inserted behind its back — except the legacy row,
//! which exists precisely because a pre-0021 writer inserted one that way.
//!
//! WHAT FAILS IF THIS ROTS: `Edge` stops carrying `event_time_source`,
//! `source_session` or `source_kind`; the join from `source_episode` to
//! `sessions.key` breaks (or is dropped from one of the three edge reads while the
//! others keep it); an unknown provenance starts being reported as one of the two
//! known kinds; `add_fact` stops recording that it was a manual write.

use ruagent_graph as g;
use ruagent_store::Db;

const SESSION_KEY: &str = "ruagent:t52-provenance";
const SESSION_TITLE: &str = "the session the graph facts came from";

/// One `sessions` row plus the `episodes` row distillation links facts to: the
/// chain `entity_edges.source_episode` -> `episodes.source_run` -> `sessions.key`.
async fn seed_session(db: &Db) -> i64 {
    let key = SESSION_KEY.to_string();
    db.call_flat(move |conn| {
        conn.execute(
            "INSERT INTO sessions
                 (key, source, title, project, ref_path, started_at, updated_at,
                  mtime_ms, size_bytes, message_count, preview)
             VALUES (?1, 'ruagent', ?2, '/w', '/f.jsonl', 1, 2, 2, 3, 4, 'p')",
            rusqlite::params![key, SESSION_TITLE],
        )?;
        conn.execute(
            "INSERT INTO episodes (kind, content, content_hash, ref_time, ingested_at, source_run)
             VALUES ('run_turn', 'the user said something', 'hash-t52-provenance', ?1, ?1, ?2)",
            rusqlite::params!["2026-01-01T00:00:00Z", key],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .unwrap()
}

/// A row shaped like the pre-0021 writer's: no episode, no recorded source.
async fn insert_legacy_edge(db: &Db, src: i64, dst: i64) -> i64 {
    db.call_flat(move |conn| {
        conn.execute(
            "INSERT INTO entity_edges
               (src, dst, relation, fact_text, valid_at, created_at, source_episode)
             VALUES (?1, ?2, 'legacy_link', 'written before the column existed', ?3, ?3, NULL)",
            rusqlite::params![src, dst, "2025-06-01T00:00:00Z"],
        )?;
        Ok(conn.last_insert_rowid())
    })
    .await
    .unwrap()
}

fn edge(facts: &[g::Edge], id: i64) -> &g::Edge {
    facts
        .iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("edge {id} must be in the answer: {facts:?}"))
}

#[tokio::test]
async fn an_extracted_fact_names_its_session_and_a_manual_fact_says_manual() {
    let db = Db::open_in_memory().unwrap();
    let episode = seed_session(&db).await;
    let a = g::upsert_entity(&db, "ruagent", Some("project"), None)
        .await
        .unwrap();
    let b = g::upsert_entity(&db, "Codex", Some("tool"), None)
        .await
        .unwrap();

    // The extraction write (the distillation shape): an event time AND an episode.
    let extracted = g::add_fact_with_source(
        &db,
        a,
        b,
        "drives_model",
        "ruagent drives Codex",
        Some("2026-01-01T00:00:00Z"),
        g::EventTimeSource::Extracted,
        Some(episode),
    )
    .await
    .unwrap();
    // The manual write: no event time, no episode.
    let manual = g::add_fact(&db, a, b, "touches", "a human typed this", None, None)
        .await
        .unwrap();
    // The third state: a row from before the column existed.
    let legacy = insert_legacy_edge(&db, a, b).await;

    let facts = g::current_facts(&db, a).await.unwrap();
    let ex = edge(&facts, extracted);
    let ma = edge(&facts, manual);
    let lg = edge(&facts, legacy);

    println!(
        "READING #52: extracted -> ets={:?} kind={:?} session={:?} | manual -> ets={:?} kind={:?} \
         session={:?} | legacy -> ets={:?} kind={:?} session={:?}",
        ex.event_time_source,
        ex.source_kind,
        ex.source_session,
        ma.event_time_source,
        ma.source_kind,
        ma.source_session,
        lg.event_time_source,
        lg.source_kind,
        lg.source_session
    );

    // 1. the extracted fact names the session it came from, key AND title.
    assert_eq!(ex.event_time_source.as_deref(), Some("extracted"));
    assert_eq!(ex.source_kind, "extraction");
    let session = ex
        .source_session
        .as_ref()
        .expect("an extracted fact carries the session it came from");
    assert_eq!(session.key, SESSION_KEY);
    assert_eq!(session.title.as_deref(), Some(SESSION_TITLE));

    // 2. a manual write is `manual` -- not `unknown`, and not `extraction`.
    assert_eq!(ma.event_time_source.as_deref(), Some("recorded"));
    assert_eq!(ma.source_kind, "manual");
    assert_eq!(
        ma.source_session, None,
        "a hand-written fact has no session, and none is invented for it"
    );
    assert_eq!(ma.source_episode, None);

    // 3. the legacy row keeps BOTH nulls and is reported as unknown.
    assert_eq!(lg.event_time_source, None);
    assert_eq!(lg.source_kind, "unknown");
    assert_eq!(lg.source_session, None);

    // 4. the same row reads the same through the OTHER edge endpoints: one row
    //    shape, three reads (`current_facts`, `list_edges`, `facts_as_of`).
    let (listed, total) = g::list_edges(&db, 50, 0).await.unwrap();
    assert_eq!(total, 3);
    for id in [extracted, manual, legacy] {
        let from_list = edge(&listed, id);
        let from_entity = edge(&facts, id);
        assert_eq!(
            from_list, from_entity,
            "edge {id} must not have a different shape per endpoint"
        );
    }
    let as_of = g::facts_as_of(&db, a, "2026-06-01T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(edge(&as_of, extracted), ex);
    assert_eq!(edge(&as_of, legacy), lg);
}

/// The distillation path itself: `apply_extraction` writes the episode onto every
/// relation it lands, so the session is reachable without a second write.
#[tokio::test]
async fn the_extraction_write_path_attaches_the_episode_to_every_fact() {
    let db = Db::open_in_memory().unwrap();
    let episode = seed_session(&db).await;
    let entities = vec![
        g::ExtractEntity {
            name: "ruagent".into(),
            kind: Some("project".into()),
            summary: None,
            aliases: vec!["ruagent-rs".into()],
        },
        g::ExtractEntity {
            name: "Codex".into(),
            kind: Some("tool".into()),
            summary: None,
            aliases: Vec::new(),
        },
    ];
    let facts = vec![
        g::ExtractFact {
            src: "ruagent".into(),
            dst: "Codex".into(),
            relation: "drives_model".into(),
            fact_text: "ruagent drives Codex".into(),
            valid_at: Some("2026-01-01T00:00:00Z".into()),
            event_time_source: g::EventTimeSource::Extracted,
        },
        // The extraction found no event time for this one: it must NOT claim one.
        g::ExtractFact {
            src: "ruagent".into(),
            dst: "Codex".into(),
            relation: "touches".into(),
            fact_text: "extracted without an event time".into(),
            valid_at: None,
            event_time_source: g::EventTimeSource::Extracted,
        },
    ];
    let report = g::apply_extraction(
        &db,
        &entities,
        &facts,
        g::AliasSource::Extraction,
        Some(episode),
    )
    .await
    .unwrap();
    assert_eq!((report.entities, report.relations), (2, 2));

    let id = g::resolve_entity_id(&db, "ruagent").await.unwrap().unwrap();
    let read = g::current_facts(&db, id).await.unwrap();
    assert_eq!(read.len(), 2);
    let by_relation = |rel: &str| {
        read.iter()
            .find(|e| e.relation == rel)
            .unwrap_or_else(|| panic!("{rel} must be readable: {read:?}"))
    };
    let timed = by_relation("drives_model");
    assert_eq!(timed.event_time_source.as_deref(), Some("extracted"));
    assert_eq!(timed.source_kind, "extraction");
    assert_eq!(
        timed.source_session.as_ref().map(|s| s.key.as_str()),
        Some(SESSION_KEY),
        "the episode the extraction was given is the session the fact names"
    );
    let untimed = by_relation("touches");
    assert_eq!(untimed.event_time_source.as_deref(), Some("recorded"));
    assert_eq!(
        untimed.source_kind, "manual",
        "an extracted fact with no event time is recorded-by-the-clock, and the \
         reader says so rather than calling it extraction"
    );
    assert_eq!(
        untimed.source_session.as_ref().map(|s| s.key.as_str()),
        Some(SESSION_KEY),
        "it still came from the session, even though the event time did not"
    );
}

/// The HTTP face's name resolution is THIS rule, so it is pinned here: exact
/// `norm_name` first, then the alias table, and a miss resolves to nothing rather
/// than to a new node.
#[tokio::test]
async fn resolve_entity_id_answers_by_name_and_by_alias_and_never_creates() {
    let db = Db::open_in_memory().unwrap();
    let a = g::upsert_entity(&db, "ruagent", Some("project"), None)
        .await
        .unwrap();
    g::upsert_entity(&db, "Codex", Some("tool"), None)
        .await
        .unwrap();

    assert_eq!(
        g::resolve_entity_id(&db, "  RUAGENT ").await.unwrap(),
        Some(a),
        "the same normalisation the extraction probes with: trim + lowercase"
    );
    assert!(
        g::add_alias(&db, a, "ruagent-rs", g::AliasSource::Human)
            .await
            .unwrap()
    );
    assert_eq!(
        g::resolve_entity_id(&db, "ruagent-rs").await.unwrap(),
        Some(a),
        "a recorded alias resolves to the entity it was attached to"
    );
    assert_eq!(
        g::resolve_entity_id(&db, "no-such-entity").await.unwrap(),
        None
    );
    assert_eq!(g::resolve_entity_id(&db, "   ").await.unwrap(), None);
    let count = db
        .call_flat(|conn| {
            conn.query_row("SELECT COUNT(*) FROM entities", [], |r| r.get::<_, i64>(0))
        })
        .await
        .unwrap();
    assert_eq!(count, 2, "resolution never creates a row for a miss");
}

/// The enum's text is the column's text, for all four values — the property that
/// makes `AliasSource` a definition of the vocabulary rather than a renaming of it.
#[tokio::test]
async fn every_alias_source_reaches_the_column_as_its_own_text() {
    let db = Db::open_in_memory().unwrap();
    let id = g::upsert_entity(&db, "ruagent", Some("project"), None)
        .await
        .unwrap();
    let written = [
        (g::AliasSource::Extraction, "extraction"),
        (g::AliasSource::Knowledge, "knowledge"),
        (g::AliasSource::Human, "human"),
        (g::AliasSource::Merge, "merge"),
    ];
    for (source, want) in written {
        assert_eq!(source.as_str(), want);
        assert!(
            g::add_alias(&db, id, want, source).await.unwrap(),
            "{want} is a new alias row"
        );
        let stored = stored_source(&db, want).await;
        assert_eq!(
            stored.as_deref(),
            Some(want),
            "the stored text is the enum's own text"
        );
    }
}

/// Reads the COLUMN, so the assertion above is not the function's return value
/// checked against itself.
async fn stored_source(db: &Db, alias: &str) -> Option<String> {
    let alias = alias.to_string();
    db.call(move |conn| -> Result<Option<String>, rusqlite::Error> {
        let row = conn.query_row(
            "SELECT source FROM entity_aliases WHERE alias = ?1",
            [&alias],
            |r| r.get::<_, String>(0),
        );
        match row {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    })
    .await
    .unwrap()
    .unwrap()
}
