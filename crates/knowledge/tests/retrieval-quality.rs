//! Retrieval-quality harness (t245).
//!
//! WHY: the platform's only retrieval log (recall_log) had 574 rows but only 15
//! distinct queries, 96.5% of them from ruagent doctor self-checks, and the
//! top_knowledge_score in it is an RRF RANK score (upper bound = legs/61), not a
//! relevance number. Nothing in the platform could answer "is retrieval good?".
//! This test builds that reading, so later changes can be falsified.
//!
//! OBJECT SETS, named (a judge whose object set is unnamed is the defect this
//! workflow keeps finding):
//!   corpus -- 16 documents, ingested through the product (Knowledge::ingest).
//!   hits   -- CHUNK ids, as returned by the product (Knowledge::search).
//!   gold   -- a DOCUMENT id: the answer to "did the right document come back".
//!             Chunk ids and document ids are DIFFERENT object sets. The
//!             chunk -> document map is the only legal way to compare them.
//!             Comparing a chunk id against a document id is what the first two
//!             attempts did; the self-test below pins that defect shut.
//!   fused  -- Knowledge::search, the store's own RRF fusion.
//!   semantic / keyword / entity -- ABSENT, with the reason recorded in the JSON.
//!             The store does not expose per-leg rankings and the entity leg
//!             lives in crates/graph. Reconstructing a leg inside the test would
//!             make the instrument measure its own copy of the retrieval logic.
//!             Absent and named, never silently skipped, never a second copy.
//!
//! Determinism: hash embedder, tie-broken sorts, fixed key order, and NO timing
//! inside the JSON -- a wall-clock field would make the byte-equality judge fail
//! on every run. Timing is printed, never written.
//!
//! Isolation: everything under a temp root. ~/.ruagent is never opened.
//! Real embedder mode: RUAGENT_T245_REAL=1 uses the product's FastEmbedder; if it
//! cannot be constructed the JSON records that as ABSENT with the error, and the
//! run continues on the hash embedder.

use ruagent_knowledge::{Embedder, HashEmbedder, Knowledge};
use std::collections::HashMap;
use std::sync::Arc;

const QUERY_SET_VERSION: &str = "t245-v1";

/// (name, body). Near-miss pairs are deliberate: documents sharing most words
/// but whose ANSWER differs, so a surface-matching leg is caught.
const CORPUS: &[(&str, &str)] = &[
    (
        "kettle",
        "The kettle boils water. Kettle descaling: fill with vinegar and water, boil, then rinse the kettle twice. Kettle scale is calcium carbonate.",
    ),
    (
        "kettle-vs-teapot",
        "A teapot brews tea leaves. A teapot is not a kettle: it does not boil water, it infuses leaves. Teapot cleaning uses baking soda.",
    ),
    (
        "autohotkey-v2",
        "AutoHotkey v2 scripts use expressions. autohotkey-v2 removed legacy syntax: commands became functions. Migration from autohotkey-v1 requires rewriting labels.",
    ),
    (
        "autohotkey-v1",
        "AutoHotkey v1 scripts use legacy command syntax with percent signs. autohotkey-v1 is deprecated but still runs.",
    ),
    (
        "sunrise",
        "Sunrise is when the sun appears above the horizon. Sunrise time depends on latitude and season. Photographers shoot at sunrise.",
    ),
    (
        "dawn",
        "Dawn is the twilight before sunrise. Dawn is not sunrise itself: dawn is when the sky first brightens. Astronomers distinguish civil, nautical and astronomical dawn.",
    ),
    (
        "rust-ownership",
        "Rust ownership: each value has one owner. When the owner goes out of scope the value is dropped. Borrowing avoids moving ownership.",
    ),
    (
        "rust-borrowing",
        "Rust borrowing rules: many immutable references or one mutable reference, never both. Borrowing is checked at compile time.",
    ),
    (
        "chinese-tea",
        "泡茶的水温很重要。绿茶用八十度的水，红茶用一百度的水。泡茶时间过长会发苦。",
    ),
    (
        "chinese-coffee",
        "手冲咖啡的水温是九十二度。咖啡研磨度决定萃取速度。咖啡与茶不同，不讲究洗茶。",
    ),
    (
        "vector-index",
        "A vector index stores embeddings for similarity search. HNSW builds a navigable small-world graph for approximate nearest neighbours.",
    ),
    (
        "rrf-fusion",
        "Reciprocal rank fusion merges several rankings by summing 1 over k plus rank. RRF needs no score calibration and is robust to differing score scales.",
    ),
    (
        "postgres-vacuum",
        "PostgreSQL VACUUM reclaims dead tuple space. Autovacuum triggers on a fraction of table size. VACUUM FULL rewrites the table and takes a lock.",
    ),
    (
        "irrelevant-birdwatching",
        "Birdwatching tips: bring binoculars, arrive early, and keep quiet. A field guide helps identify plumage.",
    ),
    (
        "irrelevant-taxes",
        "Quarterly tax filing deadlines differ by jurisdiction. Keep receipts for deductions and reconcile at year end.",
    ),
    (
        "irrelevant-marathon",
        "Marathon training plans build weekly mileage slowly. Taper for three weeks before race day and practise fuelling.",
    ),
];

/// (query, class, gold document name, answerable)
const QUERIES: &[(&str, &str, &str, bool)] = &[
    ("kettle descaling", "exact-keyword", "kettle", true),
    ("rust ownership", "exact-keyword", "rust-ownership", true),
    (
        "does a teapot boil water",
        "exact-keyword",
        "kettle-vs-teapot",
        true,
    ),
    (
        "reclaim dead tuple space",
        "exact-keyword",
        "postgres-vacuum",
        true,
    ),
    ("autohotkey-v2", "hyphen-model", "autohotkey-v2", true),
    ("autohotkey-v1", "hyphen-model", "autohotkey-v1", true),
    ("泡茶 水温", "chinese", "chinese-tea", true),
    ("手冲咖啡 研磨度", "chinese", "chinese-coffee", true),
    (
        "when the sky first brightens before the sun appears",
        "synonym",
        "dawn",
        true,
    ),
    (
        "the sun appearing above the horizon in the morning",
        "synonym",
        "sunrise",
        true,
    ),
    (
        "how do I clean a teapot",
        "synonym",
        "kettle-vs-teapot",
        true,
    ),
    (
        "merge several rankings without calibrating scores",
        "multiword-phrase",
        "rrf-fusion",
        true,
    ),
    (
        "navigable small world graph approximate nearest neighbours",
        "multiword-phrase",
        "vector-index",
        true,
    ),
    (
        "many immutable references or one mutable reference",
        "multiword-phrase",
        "rust-borrowing",
        true,
    ),
    (
        "kettle scale is calcium carbonate",
        "multiword-phrase",
        "kettle",
        true,
    ),
    ("what is the capital of Peru", "no-answer", "", false),
    ("how do I change a bicycle tyre", "no-answer", "", false),
    ("best time to see migrating whales", "no-answer", "", false),
];

const NL: char = char::from_u32(10).unwrap();

/// JSON string escaping with no backslash literals in the source: the escape
/// character is built from its code point, so nothing here depends on how the
/// file was written.
fn esc(s: &str) -> String {
    let bs = char::from_u32(92).unwrap();
    let qt = char::from_u32(34).unwrap();
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        if c == qt {
            o.push(bs);
            o.push(qt);
        } else if c == bs {
            o.push(bs);
            o.push(bs);
        } else if c == char::from_u32(10).unwrap() {
            o.push(bs);
            o.push('n');
        } else if c == char::from_u32(13).unwrap() {
            o.push(bs);
            o.push('r');
        } else if c == char::from_u32(9).unwrap() {
            o.push(bs);
            o.push('t');
        } else if (c as u32) < 0x20 {
            o.push(bs);
            o.push_str(&format!("u{:04x}", c as u32));
        } else {
            o.push(c);
        }
    }
    o
}

fn q(s: &str) -> String {
    let qt = char::from_u32(34).unwrap();
    let mut o = String::new();
    o.push(qt);
    o.push_str(&esc(s));
    o.push(qt);
    o
}

/// THE JUDGE (single source). Rank of the gold DOCUMENT in a fused hit list.
/// hit_chunk_ids are chunk ids; gold_doc is a document id; doc_of is the only
/// legal bridge between the two object sets.
fn gold_rank(hit_chunk_ids: &[i64], doc_of: &HashMap<i64, i64>, gold_doc: i64) -> Option<usize> {
    hit_chunk_ids
        .iter()
        .position(|cid| doc_of.get(cid) == Some(&gold_doc))
}

/// recall@1 / recall@5 / MRR. n counts ANSWERABLE queries only; a query whose
/// gold never appeared contributes 0 rather than being dropped, because dropping
/// it would inflate the score.
fn metrics(ranks: &[Option<usize>]) -> (f64, f64, f64) {
    let n = ranks.len().max(1) as f64;
    let mut r1 = 0.0f64;
    let mut r5 = 0.0f64;
    let mut mrr = 0.0f64;
    for pos in ranks.iter().flatten() {
        if *pos == 0 {
            r1 += 1.0;
        }
        if *pos < 5 {
            r5 += 1.0;
        }
        mrr += 1.0 / (*pos as f64 + 1.0);
    }
    (r1 / n, r5 / n, mrr / n)
}

fn arr(v: &[i64]) -> String {
    let mut o = String::from("[");
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            o.push_str(", ");
        }
        o.push_str(&x.to_string());
    }
    o.push(']');
    o
}

fn line(out: &mut String, indent: usize, s: &str) {
    for _ in 0..indent {
        out.push(' ');
    }
    out.push_str(s);
    out.push(NL);
}

async fn run_once(tag: &str, real: bool) -> String {
    let started = std::time::Instant::now();
    let root = std::env::temp_dir().join(format!("ruagent-t245-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).unwrap();
    let db = ruagent_store::Db::open(root.join("data").join("ruagent.db")).unwrap();

    let mut embedder_note = String::from("hash-embedder (deterministic, offline)");
    let embedder: Arc<dyn Embedder> = if real {
        match ruagent_knowledge::FastEmbedder::try_new().await {
            Ok(fe) => {
                embedder_note = format!("{} (dim {})", fe.name(), fe.dim());
                Arc::new(fe)
            }
            Err(e) => {
                embedder_note = format!(
                    "ABSENT -- FastEmbedder::try_new failed ({}); this run used hash-embedder instead",
                    e
                );
                Arc::new(HashEmbedder::new(64))
            }
        }
    } else {
        Arc::new(HashEmbedder::new(64))
    };

    let kb = Knowledge::with_embedder(root.as_path(), db, embedder)
        .await
        .unwrap();
    for (name, body) in CORPUS {
        kb.ingest(name, body).await.unwrap();
    }
    let docs = kb.list_documents().await.unwrap();

    let mut doc_of: HashMap<i64, i64> = HashMap::new();
    let mut doc_id_of_name: HashMap<String, i64> = HashMap::new();
    for d in &docs {
        doc_id_of_name.insert(d.name.clone(), d.id);
        for (cid, _text) in kb.document_chunks(d.id).await.unwrap_or_default() {
            doc_of.insert(cid, d.id);
        }
    }

    let mut rows: Vec<String> = Vec::new();
    let mut ranks: Vec<Option<usize>> = Vec::new();
    let mut no_answer: Vec<(String, f32, usize)> = Vec::new();
    let mut missing_gold: Vec<String> = Vec::new();
    // t261: the keyword leg's own reading. t250 exposed the legs through
    // Knowledge::search_legs, so these numbers come from the store itself --
    // not from a reconstruction (which is why t245 had to report them ABSENT).
    let mut kw_rows: Vec<String> = Vec::new();
    let mut kw_nonempty = 0usize;
    let mut sem_nonempty = 0usize;
    // Deliberately stage-agnostic: this file must compile against BOTH the
    // pre-t261 store and the post-t261 store, so the before/after pair is taken
    // from two runs of one harness rather than from a second copy of the old
    // construction living here. The stage behaviour itself is pinned by
    // retrieval-legs.rs.

    for (query, class, gold_name, answerable) in QUERIES {
        let hits = kb.search(query, 5).await.unwrap();
        let hit_ids: Vec<i64> = hits.iter().map(|h| h.chunk_id).collect();

        // t261: the two legs exactly as the store computed them.
        let legs = kb.search_legs(query, 5).await.unwrap();
        if !legs.keyword.is_empty() {
            kw_nonempty += 1;
        }
        if !legs.semantic.is_empty() {
            sem_nonempty += 1;
        }
        kw_rows.push(format!(
            "    {{ \"query\": {}, \"class\": {}, \"keyword_hits\": {}, \"keyword_raw_scores\": [{}], \"semantic_top_chunk\": {} }}",
            q(query),
            q(class),
            arr(&legs.keyword.iter().map(|h| h.chunk_id).collect::<Vec<i64>>()),
            legs.keyword
                .iter()
                .map(|h| format!("{:.6}", h.raw_score))
                .collect::<Vec<String>>()
                .join(", "),
            legs.semantic.first().map(|h| h.chunk_id).unwrap_or(-1)
        ));
        let top = hits.first().map(|h| h.score).unwrap_or(0.0);
        let hit_docs: Vec<i64> = hit_ids
            .iter()
            .map(|c| *doc_of.get(c).unwrap_or(&-1))
            .collect();

        let mut gold_doc = -1i64;
        let mut rank: Option<usize> = None;
        if *answerable {
            match doc_id_of_name.get(*gold_name) {
                Some(id) => {
                    gold_doc = *id;
                    rank = gold_rank(&hit_ids, &doc_of, *id);
                }
                None => missing_gold.push(query.to_string()),
            }
            ranks.push(rank);
        } else {
            no_answer.push((query.to_string(), top, hit_ids.len()));
        }

        let rank_json = match rank {
            Some(p) => p.to_string(),
            None => String::from("null"),
        };
        rows.push(format!(
            "    {{ \"query\": {}, \"class\": {}, \"answerable\": {}, \"gold_document\": {}, \"gold_document_id\": {}, \"gold_rank\": {}, \"hits_top5_chunks\": {}, \"hits_top5_documents\": {}, \"fused_top_score\": {:.6} }}",
            q(query),
            q(class),
            answerable,
            q(gold_name),
            gold_doc,
            rank_json,
            arr(&hit_ids),
            arr(&hit_docs),
            top
        ));
    }

    let (f1, f5, fm) = metrics(&ranks);

    let mut out = String::new();
    out.push('{');
    out.push(NL);
    line(
        &mut out,
        2,
        &format!("\"query_set_version\": {},", q(QUERY_SET_VERSION)),
    );
    line(&mut out, 2, "\"provenance\": {");
    line(
        &mut out,
        4,
        &format!("\"embedder\": {},", q(&embedder_note)),
    );
    line(
        &mut out,
        4,
        &format!("\"corpus_documents\": {},", CORPUS.len()),
    );
    line(&mut out, 4, &format!("\"queries\": {},", QUERIES.len()));
    line(&mut out, 4, &format!("\"answerable\": {},", ranks.len()));
    line(
        &mut out,
        4,
        &format!("\"no_answer\": {},", QUERIES.len() - ranks.len()),
    );
    line(
        &mut out,
        4,
        "\"timing\": \"printed to stdout, deliberately NOT written here: a wall-clock field would make the byte-equality judge fail on every run\",",
    );
    // The t245 shape, kept so a reader that knows it is not surprised. Two of
    // the three entries changed status in t250: the knowledge legs ARE exposed
    // now, and t261 reads them here instead of reconstructing them.
    line(
        &mut out,
        4,
        "\"leg_status_note\": \"t245 shape kept for stability; t250 exposed both knowledge legs, so semantic and keyword now report PRESENT and only the entity leg is absent\",",
    );
    line(&mut out, 4, "\"absent_legs\": {");
    line(
        &mut out,
        6,
        "\"semantic\": \"PRESENT since t250 -- read from Knowledge::search_legs, never reconstructed\",",
    );
    line(
        &mut out,
        6,
        "\"keyword\": \"PRESENT since t250 -- read from Knowledge::search_legs; the per-query hits are in keyword_leg_rows\",",
    );
    line(
        &mut out,
        6,
        "\"entity\": \"ABSENT -- the entity leg lives in crates/graph, which this crate cannot read\"",
    );
    line(&mut out, 4, "},");
    line(
        &mut out,
        4,
        "\"fused_score_semantics\": \"rank-based RRF: sum of 1/(60+rank) over the legs that returned the document, so an UNANSWERABLE query also reaches about 1/61 -- this column cannot be used as a relevance threshold\"",
    );
    line(&mut out, 2, "},");
    line(&mut out, 2, "\"metrics\": {");
    line(
        &mut out,
        4,
        &format!(
            "\"fused\": {{ \"recall_at_1\": {:.4}, \"recall_at_5\": {:.4}, \"mrr\": {:.4}, \"n\": {} }},",
            f1,
            f5,
            fm,
            ranks.len()
        ),
    );
    line(
        &mut out,
        4,
        &format!(
            "\"semantic\": {{ \"nonempty_queries\": {}, \"n\": {} }},",
            sem_nonempty,
            QUERIES.len()
        ),
    );
    line(
        &mut out,
        4,
        &format!(
            "\"keyword\": {{ \"nonempty_queries\": {}, \"n\": {} }},",
            kw_nonempty,
            QUERIES.len()
        ),
    );
    line(&mut out, 4, "\"entity\": null");
    line(&mut out, 2, "},");
    line(&mut out, 2, "\"missing_gold_documents\": [");
    let mg: Vec<String> = missing_gold
        .iter()
        .map(|s| format!("    {}", q(s)))
        .collect();
    out.push_str(&mg.join(","));
    out.push(NL);
    line(&mut out, 2, "],");
    line(&mut out, 2, "\"no_answer_top_scores\": [");
    let na: Vec<String> = no_answer
        .iter()
        .map(|(qq, s, n)| {
            format!(
                "    {{ \"query\": {}, \"top_score\": {:.6}, \"hits\": {} }}",
                q(qq),
                s,
                n
            )
        })
        .collect();
    out.push_str(&na.join(","));
    out.push(NL);
    line(&mut out, 2, "],");
    line(&mut out, 2, "\"keyword_leg_rows\": [");
    out.push_str(&kw_rows.join(","));
    out.push(NL);
    line(&mut out, 2, "],");
    line(&mut out, 2, "\"rows\": [");
    out.push_str(&rows.join(","));
    out.push(NL);
    line(&mut out, 2, "]");
    out.push('}');
    out.push(NL);

    println!(
        "[t245/{}] embedder={} docs={} queries={} answerable={} fused recall@1={:.4} recall@5={:.4} mrr={:.4} keyword_nonempty={}/{} semantic_nonempty={} elapsed={}ms",
        tag,
        embedder_note,
        docs.len(),
        QUERIES.len(),
        ranks.len(),
        f1,
        f5,
        fm,
        kw_nonempty,
        QUERIES.len(),
        sem_nonempty,
        started.elapsed().as_millis()
    );

    let _ = std::fs::remove_dir_all(&root);
    out
}

#[test]
fn judge_is_falsifiable() {
    let mut m: HashMap<i64, i64> = HashMap::new();
    m.insert(10, 100);
    m.insert(11, 101);
    m.insert(12, 102);
    let hits = vec![10, 11];
    // must PASS: gold document 101 is hit, at rank 2.
    assert_eq!(gold_rank(&hits, &m, 101), Some(1));
    // must FAIL: gold document 102 is not in the hit list.
    assert_eq!(gold_rank(&hits, &m, 102), None);
    // The first two attempts compared a CHUNK id against a DOCUMENT id. Gold
    // document 11 must NOT be reported as hit merely because chunk 11 is present.
    assert_eq!(gold_rank(&[11, 12], &m, 11), None);
    // recall@5 must count a rank-6 gold as a miss.
    let (_, r5, _) = metrics(&[Some(5), Some(0)]);
    assert!(
        (r5 - 0.5).abs() < 1e-9,
        "rank 6 must not count as a top-5 hit"
    );
    // ... and an all-miss run must score zero, not be dropped from the denominator.
    let (r1, r5b, mrr) = metrics(&[None, None]);
    assert_eq!((r1, r5b, mrr), (0.0, 0.0, 0.0));
}

#[tokio::test]
async fn retrieval_quality() {
    let real = std::env::var("RUAGENT_T245_REAL").as_deref() == Ok("1");
    let a = run_once("a", real).await;
    let b = run_once("b", real).await;
    println!("{}", a);
    // Write the reading where build artefacts live, not into the package
    // directory: a bare relative "target" would litter crates/knowledge.
    let dir = match std::env::var("CARGO_TARGET_DIR") {
        Ok(d) => std::path::PathBuf::from(d),
        Err(_) => std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/target")),
    };
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("retrieval-quality.json"), &a).unwrap();
    println!(
        "[t245] byte-identical across two runs = {}",
        if a == b { "yes" } else { "NO" }
    );
    assert_eq!(
        a, b,
        "two runs must be byte-identical (no timing inside the JSON)"
    );
}
