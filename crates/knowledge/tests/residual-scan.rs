//! The residual scan (t7 / R-A D.2 item 5) — owed to the memory crate's
//! "did we really forget it" question, and the shape mem-core froze with me on
//! 2026-09-27 (a bounded result must never read as "there is nothing else").
//!
//! `crates/memory` deliberately does NOT depend on `crates/knowledge` (it
//! re-declares the shape), so the two value sets can drift. The drift pin is
//! `residual_origin_literals_are_the_value_set_memory_also_defines` below: it
//! asserts the literals here, and its comment names the other definition. That
//! is the same trick the tokenizer uses (fts.rs is the single source and the
//! graph crate is checked against it) — a pin that fails loudly beats two
//! definitions nobody compares.

use ruagent_knowledge::{Embedder, HashEmbedder, Knowledge, ResidualOrigin};
use std::sync::Arc;

async fn kb(tag: &str) -> (Knowledge, std::path::PathBuf) {
    let root =
        std::env::temp_dir().join(format!("ruagent-t7-resid-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();
    let embedder: Arc<dyn Embedder> = Arc::new(HashEmbedder::new(64));
    let k = Knowledge::with_embedder(root.as_path(), db, embedder)
        .await
        .unwrap();
    (k, root)
}

/// The drift pin mem-core asked for. `crates/memory/src/lifecycle.rs` defines
/// its own `ResidualOrigin { Db, File, Both }` (it cannot import this one), and
/// the two are only comparable if the literals match. This test is the place
/// where a rename on either side becomes visible.
#[test]
fn residual_origin_literals_are_the_value_set_memory_also_defines() {
    let mut literals = Vec::new();
    for origin in [
        ResidualOrigin::Db,
        ResidualOrigin::File,
        ResidualOrigin::Both,
    ] {
        literals.push(serde_json::to_string(&origin).unwrap());
    }
    literals.sort();
    assert_eq!(
        literals,
        vec![
            "\"Both\"".to_string(),
            "\"Db\"".to_string(),
            "\"File\"".to_string()
        ],
        "crates/memory/src/lifecycle.rs re-declares these three values; a change here \\
         without a change there splits the two reports"
    );
}

/// A needle that is in the index and NOT on disk: the file was never written
/// (ingest writes no file) — origin `Db`, and that is a real state, not a
/// fallback.
#[tokio::test]
async fn a_needle_in_the_index_only_is_reported_as_db() {
    let (k, root) = kb("db-only").await;
    k.ingest(
        "seal-notes",
        "Harbour seal sighting at noon. The seal was asleep.",
    )
    .await
    .unwrap();
    let page = k.residual_scan("seal", 10, true).await.unwrap();
    assert_eq!(page.hits.len(), 1, "{:?}", page.hits);
    let h = &page.hits[0];
    assert_eq!(h.name, "seal-notes");
    assert_eq!(h.origin, ResidualOrigin::Db);
    assert!(h.chunk_id.is_some(), "the chunk-level match must be named");
    assert!(!page.truncated);
    assert_eq!(page.total, Some(1), "an exact total was asked for");
    let _ = std::fs::remove_dir_all(&root);
}

/// The case the whole scan exists for: the index no longer knows the text, but
/// the markdown copy still holds it. A report that could only see the index
/// would answer "gone" here.
#[tokio::test]
async fn a_needle_only_on_disk_is_reported_as_file() {
    let (k, root) = kb("file-only").await;
    k.ingest("kept-page", "Nothing sensitive in here.")
        .await
        .unwrap();
    std::fs::write(
        root.join("knowledge").join("forgotten-page.md"),
        "The launch code was NIGHTINGALE-7.\n",
    )
    .unwrap();
    let page = k.residual_scan("NIGHTINGALE", 10, true).await.unwrap();
    assert_eq!(page.hits.len(), 1, "{:?}", page.hits);
    let h = &page.hits[0];
    assert_eq!(h.name, "forgotten-page");
    assert_eq!(h.origin, ResidualOrigin::File);
    assert_eq!(
        h.document_id, -1,
        "-1 says 'there is no row at all'; a 0 would read like a real id"
    );
    assert_eq!(h.chunk_id, None);
    assert!(h.path.is_some());
    let _ = std::fs::remove_dir_all(&root);
}

/// Both copies: the index and the file still carry the needle.
#[tokio::test]
async fn a_needle_in_both_copies_is_reported_as_both() {
    let (k, root) = kb("both").await;
    k.ingest("both-page", "Temporary token ZULU-99 was issued.")
        .await
        .unwrap();
    std::fs::write(
        root.join("knowledge").join("both-page.md"),
        "Temporary token ZULU-99 was issued.\n",
    )
    .unwrap();
    let page = k.residual_scan("ZULU-99", 10, true).await.unwrap();
    assert_eq!(page.hits.len(), 1, "{:?}", page.hits);
    assert_eq!(page.hits[0].origin, ResidualOrigin::Both);
    let _ = std::fs::remove_dir_all(&root);
}

/// Truncation must be VISIBLE, and the exact total must cost something: with
/// `exact_total = false` the call must not claim a total it did not compute.
#[tokio::test]
async fn truncation_is_distinguishable_from_a_complete_answer() {
    let (k, root) = kb("trunc").await;
    for i in 0..3 {
        k.ingest(
            &format!("page-{i}"),
            &format!("The needle word is MARLIN, copy {i}."),
        )
        .await
        .unwrap();
    }
    let small = k.residual_scan("MARLIN", 2, false).await.unwrap();
    assert_eq!(small.hits.len(), 2, "the page is bounded by `limit`");
    assert!(
        small.truncated,
        "a full page must not read as 'that is all there is'"
    );
    assert_eq!(
        small.total, None,
        "no count was paid for, so no count may be claimed"
    );

    let exact = k.residual_scan("MARLIN", 2, true).await.unwrap();
    assert!(exact.truncated);
    assert_eq!(exact.total, Some(3));

    let complete = k.residual_scan("MARLIN", 10, false).await.unwrap();
    assert_eq!(complete.hits.len(), 3);
    assert!(!complete.truncated, "nothing was withheld");
    assert_eq!(complete.total, None);

    // ... and a query that found nothing is a MEASURED zero, not an absence.
    let none = k.residual_scan("PORPOISE", 10, true).await.unwrap();
    assert!(none.hits.is_empty());
    assert!(!none.truncated);
    assert_eq!(none.total, Some(0));
    let _ = std::fs::remove_dir_all(&root);
}

/// `document_path` is the mapping the memory-side report needs to name a file.
#[tokio::test]
async fn document_path_is_the_documented_mapping() {
    let (k, root) = kb("docpath").await;
    assert_eq!(
        k.document_path("wiki/kubernetes-troubleshooting"),
        root.join("knowledge")
            .join("wiki")
            .join("kubernetes-troubleshooting.md")
    );
    let _ = std::fs::remove_dir_all(&root);
}
