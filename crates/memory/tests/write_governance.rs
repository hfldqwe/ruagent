//! t72: the write pipeline's two silent-failure paths, driven from OUTSIDE the
//! crate (public API only).
//!
//! Both defects are the family "an evidence event exists for an event that did
//! not happen" (t41/t52), so every case here pairs a FIX reading with a NEGATIVE
//! CONTROL that proves the normal path was not switched off along with the bug:
//!
//! * F1 (`write.rs` dedupe): a soft-deleted row used to look "already current",
//!   so re-writing the same content after a delete returned `SkippedDuplicate`
//!   and the fact never came back. Fix = the dedupe query uses the READ side's
//!   definition of current (`superseded_at IS NULL AND deleted_at IS NULL`).
//!   Negative control = a genuine duplicate is still skipped.
//! * F2 (`write.rs` supersede): the scoped UPDATE was not judged by its affected
//!   rows, so a `supersedes` id belonging to another store/namespace (or an
//!   already-superseded row) produced `Superseded { old, new }` AND an
//!   `op='supersede'` audit row while the target was still current.
//!   Negative control = a legitimate supersede still reports `Superseded`, still
//!   moves the old row out of `current_memories`, and still audits `supersede`.
//!
//! Every `READING t72 ...` line below is the evidence quoted in
//! `docs/design/reviews/gen3-memory-write-repair.md`.

use ruagent_memory::lifecycle::delete_memory;
use ruagent_memory::query::{current_memories, list_diffs};
use ruagent_memory::write::{MemoryWrite, WriteOutcome, write_memory};
use ruagent_memory::{MemoryStore, Namespace};
use ruagent_store::Db;

fn w(store: MemoryStore, ns: &str, content: &str, supersedes: Option<i64>) -> MemoryWrite {
    MemoryWrite {
        store,
        namespace: Namespace::parse(ns).expect("test namespace parses"),
        content: content.to_string(),
        confidence: 0.9,
        source_episode: None,
        supersedes,
    }
}

/// Every audit row, oldest first: `(op, before, after, reason)`.
async fn audit_rows(db: &Db) -> Vec<(String, Option<String>, Option<String>, Option<String>)> {
    let mut rows: Vec<_> = list_diffs(db, 200)
        .await
        .expect("audit is readable")
        .into_iter()
        .map(|d| (d.op, d.before, d.after, d.reason))
        .collect();
    rows.reverse();
    rows
}

async fn is_current(db: &Db, store: MemoryStore, ns: &str, content: &str) -> bool {
    current_memories(db, store, ns, 50)
        .await
        .expect("read works")
        .iter()
        .any(|r| r.content == content)
}

async fn row_count(db: &Db) -> i64 {
    let rows = current_memories(db, MemoryStore::Lesson, "project:t72", 50)
        .await
        .expect("read works");
    rows.len() as i64
}

/// F1 FIX: deleting a fact does not make the same content un-writable.
#[tokio::test]
async fn a_deleted_row_no_longer_blocks_the_same_content() {
    let db = Db::open_in_memory().unwrap();
    let content = "the kettle is on the second shelf";

    let first = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", content, None))
        .await
        .unwrap();
    let id = match first {
        WriteOutcome::Inserted(id) => id,
        other => panic!("setup should insert, got {other:?}"),
    };
    let deleted = delete_memory(&db, id).await.unwrap();
    println!("READING t72 F1 delete: {deleted:?}");

    let again = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", content, None)).await;
    let visible = is_current(&db, MemoryStore::Lesson, "project:t72", content).await;
    println!(
        "READING t72 F1 rewrite after delete: outcome={again:?} current_visible={visible} \
         current_rows={}",
        row_count(&db).await
    );
    let again = again.expect("re-writing deleted content must succeed");
    match again {
        WriteOutcome::Inserted(new_id) => assert_eq!(
            new_id, id,
            "the key (store, namespace, content_hash) is UNIQUE across tombstones, so the write \
             lands by REVIVING that row -- inserting a second one is impossible"
        ),
        other => panic!("the old behaviour returned {other:?}; the fact would stay lost"),
    }
    assert!(visible, "the re-written fact must be visible again");
    assert_eq!(
        row_count(&db).await,
        1,
        "the tombstone was revived, not duplicated"
    );
    // and the audit says insert (the decision) + WHY it could land at all
    let rows = audit_rows(&db).await;
    let ops: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    println!(
        "READING t72 F1 audit: ops={ops:?} last_reason={:?}",
        rows.last().and_then(|r| r.3.clone())
    );
    assert_eq!(
        ops,
        vec![
            "insert".to_string(),
            "delete".to_string(),
            "insert".to_string()
        ]
    );
    let reason = rows.last().unwrap().3.clone().unwrap_or_default();
    assert!(
        reason.contains("revived soft-deleted row") && reason.contains("affected 1 row"),
        "the audit must say the row came back BY REVIVAL, and carry the affected-row count: \
         {reason}"
    );
}

/// The uncovered corner of F1: the content's unique key is held by a tombstone
/// that is ALSO superseded. Reviving it would not make it current and a second
/// row cannot exist, so the write is refused — loudly and audited. (This is the
/// stop-gap for the schema's global unique key; see the report's routed finding.)
#[tokio::test]
async fn a_tombstone_that_is_also_superseded_refuses_the_write_loudly() {
    let db = Db::open_in_memory().unwrap();
    let content = "fact with a tombstone that was replaced";
    let v1 = match write_memory(&db, &w(MemoryStore::Lesson, "project:t72", content, None))
        .await
        .unwrap()
    {
        WriteOutcome::Inserted(id) => id,
        other => panic!("setup should insert, got {other:?}"),
    };
    // replace it, then delete the (now superseded) original
    let _v2 = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", "v2", Some(v1)))
        .await
        .unwrap();
    delete_memory(&db, v1).await.unwrap();

    let refused = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", content, None)).await;
    let rows = audit_rows(&db).await;
    println!(
        "READING t72 F1 corner: outcome={refused:?} audit_ops={:?} last_reason={:?}",
        rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
        rows.last().and_then(|r| r.3.clone())
    );
    let err = refused.expect_err("a superseded tombstone cannot be revived: refuse, do not lie");
    assert!(
        err.to_string().contains("insert refused"),
        "the refusal names itself: {err}"
    );
    let ops: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    assert_eq!(
        ops.last().map(String::as_str),
        Some("insert_refused"),
        "a refusal must not be silent: {ops:?}"
    );
    assert!(
        !rows.iter().any(|r| r.0 == "insert" && r.3.is_some()),
        "no revive happened"
    );
}

/// F1 NEGATIVE CONTROL: a genuine duplicate is still skipped (the dedupe was not
/// switched off with the bug).
#[tokio::test]
async fn a_current_duplicate_is_still_skipped() {
    let db = Db::open_in_memory().unwrap();
    let content = "same current content";
    let first = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", content, None))
        .await
        .unwrap();
    let id = match first {
        WriteOutcome::Inserted(id) => id,
        other => panic!("setup should insert, got {other:?}"),
    };
    let dup = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", content, None))
        .await
        .unwrap();
    println!(
        "READING t72 F1 negative control: first=Inserted({id}) duplicate={dup:?} rows={}",
        row_count(&db).await
    );
    assert_eq!(dup, WriteOutcome::SkippedDuplicate(id));
    assert_eq!(row_count(&db).await, 1, "no second row for a duplicate");
}

/// F2 FIX (a): a `supersedes` id from ANOTHER store+namespace is refused, and
/// the refusal is audited under its own op — while the target stays current.
#[tokio::test]
async fn a_cross_store_supersede_is_refused_not_faked() {
    let db = Db::open_in_memory().unwrap();
    let target = write_memory(
        &db,
        &w(
            MemoryStore::Observation,
            "user",
            "deploy via scripts/release.sh",
            None,
        ),
    )
    .await
    .unwrap();
    let old = match target {
        WriteOutcome::Inserted(id) => id,
        other => panic!("setup should insert, got {other:?}"),
    };

    // same id, but from the procedure store's point of view
    let refused = write_memory(
        &db,
        &w(
            MemoryStore::Procedure,
            "project:other",
            "deploy via make",
            Some(old),
        ),
    )
    .await;
    let old_still_current = current_memories(&db, MemoryStore::Observation, "user", 10)
        .await
        .unwrap()
        .iter()
        .any(|r| r.id == old);
    let procedure_rows = current_memories(&db, MemoryStore::Procedure, "project:other", 10)
        .await
        .unwrap()
        .len();
    let rows = audit_rows(&db).await;
    println!(
        "READING t72 F2a cross-store supersede: outcome={refused:?} old_still_current={old_still_current} \
         procedure_rows_written={procedure_rows} audit={:?}",
        rows.iter()
            .map(|r| (r.0.clone(), r.1.clone(), r.2.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        refused.is_err(),
        "a supersede that did not happen must not be Ok"
    );
    assert!(
        old_still_current,
        "the target was not superseded: it is still current"
    );
    assert_eq!(
        procedure_rows, 0,
        "nothing may be written when the supersede refused"
    );
    let ops: Vec<String> = rows.into_iter().map(|r| r.0).collect();
    assert_eq!(
        ops,
        vec!["insert".to_string(), "supersede_refused".to_string()]
    );
    assert!(
        !ops.contains(&"supersede".to_string()),
        "the audit must not borrow the name of a supersede that did not happen: {ops:?}"
    );
}

/// F2 FIX (b): the same rule covers an already-superseded target — the C11
/// reading from the audit.
#[tokio::test]
async fn superseding_an_already_superseded_row_is_refused() {
    let db = Db::open_in_memory().unwrap();
    let old = match write_memory(&db, &w(MemoryStore::Lesson, "project:t72", "v1", None))
        .await
        .unwrap()
    {
        WriteOutcome::Inserted(id) => id,
        other => panic!("setup should insert, got {other:?}"),
    };
    let legit = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", "v2", Some(old)))
        .await
        .unwrap();
    println!("READING t72 F2b first supersede (legit): {legit:?}");

    let again = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", "v3", Some(old))).await;
    let rows = audit_rows(&db).await;
    let v3_written = is_current(&db, MemoryStore::Lesson, "project:t72", "v3").await;
    println!(
        "READING t72 F2b supersede an already-superseded id: outcome={again:?} v3_written={v3_written} \
         audit_ops={:?}",
        rows.iter().map(|r| r.0.clone()).collect::<Vec<_>>()
    );
    assert!(
        again.is_err(),
        "a second supersede of the same row must not be Ok"
    );
    assert!(!v3_written, "the refused write must not land");
    let ops: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    assert_eq!(
        ops,
        vec![
            "insert".to_string(),
            "supersede".to_string(),
            "supersede_refused".to_string()
        ],
        "exactly one supersede happened, and the refusal is named as a refusal"
    );
}

/// F2 FIX (c): a soft-deleted target is not "current" either (one definition of
/// current on both sides).
#[tokio::test]
async fn superseding_a_deleted_row_is_refused() {
    let db = Db::open_in_memory().unwrap();
    let old = match write_memory(
        &db,
        &w(MemoryStore::Lesson, "project:t72", "will be deleted", None),
    )
    .await
    .unwrap()
    {
        WriteOutcome::Inserted(id) => id,
        other => panic!("setup should insert, got {other:?}"),
    };
    delete_memory(&db, old).await.unwrap();
    let refused = write_memory(
        &db,
        &w(MemoryStore::Lesson, "project:t72", "successor", Some(old)),
    )
    .await;
    println!(
        "READING t72 F2c supersede a deleted target: outcome={refused:?} \
         successor_written={}",
        is_current(&db, MemoryStore::Lesson, "project:t72", "successor").await
    );
    assert!(refused.is_err());
    assert!(!is_current(&db, MemoryStore::Lesson, "project:t72", "successor").await);
    let ops: Vec<String> = audit_rows(&db).await.into_iter().map(|r| r.0).collect();
    assert_eq!(
        ops,
        vec![
            "insert".to_string(),
            "delete".to_string(),
            "supersede_refused".to_string()
        ]
    );
}

/// F2 UNCHANGED: a `supersedes` id that does not exist at all is still rejected by
/// the FOREIGN KEY (this behaviour was not in scope to change).
#[tokio::test]
async fn a_nonexistent_supersedes_target_is_still_rejected() {
    let db = Db::open_in_memory().unwrap();
    let out = write_memory(
        &db,
        &w(MemoryStore::Lesson, "project:t72", "ghost", Some(999_999)),
    )
    .await;
    println!("READING t72 F2 nonexistent target: {out:?}");
    let err = out.expect_err("a nonexistent supersedes target must be rejected");
    let text = err.to_string();
    assert!(
        text.contains("FOREIGN KEY constraint failed"),
        "the FK is still the rejecter for a nonexistent id: {text}"
    );
    assert_eq!(row_count(&db).await, 0, "nothing may be written");
}

/// F2 NEGATIVE CONTROL: a legitimate supersede still works, still moves the old
/// row out of the read surface, and still writes the `supersede` audit row.
#[tokio::test]
async fn a_legitimate_supersede_still_works_and_is_audited() {
    let db = Db::open_in_memory().unwrap();
    let old = match write_memory(
        &db,
        &w(MemoryStore::Lesson, "project:t72", "old fact", None),
    )
    .await
    .unwrap()
    {
        WriteOutcome::Inserted(id) => id,
        other => panic!("setup should insert, got {other:?}"),
    };
    let out = write_memory(
        &db,
        &w(MemoryStore::Lesson, "project:t72", "new fact", Some(old)),
    )
    .await
    .expect("a legitimate supersede must succeed");
    let new = match out {
        WriteOutcome::Superseded { old: got_old, new } => {
            assert_eq!(got_old, old);
            new
        }
        other => panic!("expected Superseded, got {other:?}"),
    };
    let old_visible = current_memories(&db, MemoryStore::Lesson, "project:t72", 10)
        .await
        .unwrap()
        .iter()
        .any(|r| r.id == old);
    let rows = audit_rows(&db).await;
    println!(
        "READING t72 F2 negative control: outcome=Superseded {{ old: {old}, new: {new} }} \
         old_still_current={old_visible} audit={:?}",
        rows.iter()
            .map(|r| (r.0.clone(), r.1.clone(), r.2.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        !old_visible,
        "the superseded row must leave the read surface"
    );
    assert_eq!(
        rows.last().map(|r| r.0.clone()),
        Some("supersede".to_string()),
        "a real supersede IS audited"
    );
    assert_eq!(
        rows.last().and_then(|r| r.1.clone()),
        Some(format!("id={old}"))
    );
    assert_eq!(
        rows.last().and_then(|r| r.2.clone()),
        Some(format!("id={new}"))
    );
}

/// The family guard: for every outcome that reaches the audit, the op name
/// matches what actually happened — and the refusal is the only arm that can
/// appear without a new row.
#[tokio::test]
async fn every_audit_op_matches_what_really_happened() {
    let db = Db::open_in_memory().unwrap();
    let a = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", "alpha", None))
        .await
        .unwrap();
    let id = match a {
        WriteOutcome::Inserted(id) => id,
        other => panic!("{other:?}"),
    };
    let dup = write_memory(&db, &w(MemoryStore::Lesson, "project:t72", "alpha", None))
        .await
        .unwrap();
    let replaced = write_memory(
        &db,
        &w(MemoryStore::Lesson, "project:t72", "beta", Some(id)),
    )
    .await
    .unwrap();
    let refused = write_memory(&db, &w(MemoryStore::Profile, "user", "gamma", Some(id))).await;
    let rows = audit_rows(&db).await;
    let ops: Vec<(String, Option<String>)> =
        rows.iter().map(|r| (r.0.clone(), r.3.clone())).collect();
    println!(
        "READING t72 family: outcomes insert={a:?} duplicate={dup:?} supersede={replaced:?} \
         refusal_is_err={} audit_ops={ops:?}",
        refused.is_err()
    );
    let ops: Vec<String> = rows.into_iter().map(|r| r.0).collect();
    assert_eq!(
        ops,
        vec![
            "insert".to_string(),
            "skip_dedupe".to_string(),
            "supersede".to_string(),
            "supersede_refused".to_string(),
        ]
    );
    // the refusal's reason names the affected-row count, so the audit carries
    // the evidence rather than a claim
    let reason = audit_rows(&db).await.last().unwrap().3.clone().unwrap();
    println!("READING t72 family: refusal reason = {reason}");
    assert!(reason.contains("0 rows superseded"), "{reason}");
}
