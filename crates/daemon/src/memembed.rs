//! Memory embeddings: the semantic leg of memory recall. Rows are
//! embedded with the SAME model the knowledge base uses (one vector
//! space), stored as f32 BLOBs in the memories table, searched with
//! brute-force cosine — memory counts are hundreds, not millions.

use std::sync::Arc;

use ruagent_knowledge::embed::Embedder;
use ruagent_store::Db;

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// Embed one memory row (after any write path inserts it).
pub async fn embed_row(db: &Db, embedder: Arc<dyn Embedder>, id: i64, content: &str) {
    let Ok(vectors) = embedder.embed(&[content]) else {
        return; // offline hash fallback still works; skip silently
    };
    let Some(v) = vectors.first() else { return };
    let blob = to_blob(v);
    let name = embedder.name();
    let _ = db
        .call(move |conn| {
            conn.execute(
                "UPDATE memories SET embedding = ?1, embedder = ?2 WHERE id = ?3",
                rusqlite::params![blob, name, id],
            )
        })
        .await;
}

/// Semantic search: cosine over embedded rows (brute force). Returns
/// (id, store, namespace, content, score) above `min_score`.
pub async fn semantic_search(
    db: &Db,
    embedder: Arc<dyn Embedder>,
    query: &str,
    top_n: u32,
    min_score: f32,
) -> Vec<(i64, String, String, String, f32)> {
    let Ok(qv) = embedder.embed_query(query) else {
        return Vec::new();
    };
    let qnorm: f32 = qv.iter().map(|x| x * x).sum::<f32>().sqrt();
    if qnorm == 0.0 {
        return Vec::new();
    }
    let rows: Vec<(i64, String, String, String, Vec<u8>)> = db
        .call(|conn| -> Result<_, ruagent_store::DbError> {
            let mut stmt = conn
                .prepare(
                    "SELECT id, store, namespace, content, embedding
                       FROM memories
                      WHERE embedding IS NOT NULL AND superseded_at IS NULL
                        AND deleted_at IS NULL",
                )
                .map_err(ruagent_store::DbError::from)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get::<_, Option<Vec<u8>>>(4)?.unwrap_or_default(),
                    ))
                })
                .map_err(ruagent_store::DbError::from)?;
            Ok(rows.filter_map(|r| r.ok()).collect())
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();

    let mut scored: Vec<(i64, String, String, String, f32)> = rows
        .into_iter()
        .filter_map(|(id, store, ns, content, blob)| {
            let v = from_blob(&blob);
            if v.len() != qv.len() {
                return None; // different embedder generation — skip
            }
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm == 0.0 {
                return None;
            }
            let dot: f32 = v.iter().zip(qv.iter()).map(|(a, b)| a * b).sum();
            Some((id, store, ns, content, dot / (norm * qnorm)))
        })
        .filter(|(_, _, _, _, score)| *score >= min_score)
        .collect();
    scored.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_n as usize);
    scored
}

/// Re-embed memory rows whose embeddings are missing or were written
/// by a different model — the memory-side half of an embedder switch
/// (the knowledge crate migrates its own table at open). Skipped when
/// the active embedder is the offline fallback: an offline boot must
/// never downgrade real vectors to hash vectors. Returns the number
/// of rows re-embedded.
pub async fn reembed_stale(db: &Db, embedder: Arc<dyn Embedder>) -> usize {
    if embedder.is_fallback() {
        return 0;
    }
    let active = embedder.name().to_string();
    let rows: Vec<(i64, String)> = match db
        .call(move |conn| -> Result<Vec<(i64, String)>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT id, content FROM memories
                  WHERE (embedding IS NULL OR embedder IS NULL OR embedder != ?1)
                    AND deleted_at IS NULL",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![active], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await
    {
        Ok(Ok(rows)) => rows,
        _ => return 0,
    };
    let n = rows.len();
    for (id, content) in rows {
        embed_row(db, embedder.clone(), id, &content).await;
    }
    n
}

/// One memory recall result: the row, the ONE fused score both legs share,
/// and each leg's own raw evidence.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MemoryHit {
    pub id: i64,
    pub store: String,
    pub namespace: String,
    pub content: String,
    /// Fused rank score (RRF, k = 60): the single scale every returned row
    /// shares, so a semantic row and a keyword-only row can be ordered against
    /// each other. It is a RANK score, not a similarity -- the API labels it
    /// (score_kind) and carries each leg's raw score beside it.
    pub score: f64,
    /// Which legs returned this row: "semantic", "keyword", or both.
    pub legs: Vec<&'static str>,
    /// This row's cosine from the semantic leg (None when that leg missed it).
    pub semantic_score: Option<f64>,
    /// This row's bm25 from the keyword leg -- more negative is better (None
    /// when that leg missed it).
    pub keyword_score: Option<f64>,
}

/// Both legs, and the fused result already bounded by top_n.
#[derive(Debug, Clone, Default)]
pub struct RecallLegs {
    /// Rows the semantic leg returned (already thresholded and cut to top_n).
    pub semantic: usize,
    /// Rows the keyword leg returned (cut to top_n by rank).
    pub keyword: usize,
    /// Keyword rows the semantic leg did NOT have -- the keyword leg's actual
    /// contribution. Before t251 nothing recorded this: the merged array was a
    /// concatenation, so "the keyword leg added nothing" and "the keyword leg
    /// was discarded" looked identical from outside.
    pub keyword_new: usize,
    /// Fused rows the top_n bound cut. The old merge had no bound at all: 5
    /// semantic + 2 keyword-only rows returned 7 rows for top_n = 5.
    pub dropped_by_top_n: usize,
    /// The semantic leg's best cosine BEFORE the threshold -- what the log
    /// records as top_memory_score.
    pub top_semantic_score: Option<f64>,
    pub hits: Vec<MemoryHit>,
}

/// RRF's k. The knowledge base fuses with 60 (crates/knowledge/src/rrf.rs);
/// reusing the same constant here means the two subsystems do not invent
/// different rank scales for the same kind of evidence.
const RRF_K: u32 = 60;

/// The keyword leg's FTS query: the WHOLE query as one quoted phrase.
///
/// DELIBERATELY UNCHANGED BY t251. The knowledge and entity legs split the
/// query into tokens and AND them (crates/store/src/fts.rs, t250); this leg
/// wraps the whole query in one phrase, so a multi-word query matches only when
/// its tokens are adjacent (t247 measured 11/15 real queries at 0 keyword hits
/// for exactly this reason). Consolidating it onto ruagent_store::fts::terms +
/// match_all is a one-line change -- and it changes what recall RETURNS, not
/// how the legs are fused, so it is left for a task whose acceptance covers it.
/// t251 makes the leg's contribution VISIBLE (keyword_new) instead of changing
/// it.
fn keyword_pattern(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

/// One memory selected for injection, WITH its id: the rendered text carries
/// content, not identity, so without this a probe could only say "the marker
/// text is in the context", never "row 42 is in the context".
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedMemory {
    pub id: i64,
    pub tag: &'static str,
    pub content: String,
    /// Empty for the query leg -- a semantic hit has no timestamp in the
    /// render's shape, so it renders undated (unchanged since before t278).
    pub updated_at: String,
    /// The query leg's cosine, when this row came from that leg.
    pub score: Option<f32>,
}

/// THE ONE memory-selection rule for injection (t278): the preset's groups in
/// order -- each through the memory crate's OWN read path, so superseded and
/// soft-deleted rows stay out -- then the preset's optional query leg.
///
/// The embedder argument is needed only when the preset HAS a query leg; a
/// preset with query_top_n == 0 (the runs path) never embeds anything and works
/// with None. That is what keeps "runs has no query leg" a PARAMETER rather
/// than a second code path.
pub async fn select_injection_memories(
    db: &Db,
    embedder: Option<Arc<dyn Embedder>>,
    query: &str,
    project: Option<&str>,
    sel: &ruagent_memory::inject::InjectionSelection,
) -> Vec<SelectedMemory> {
    use ruagent_memory::inject::MemoryScope;
    let mut out: Vec<SelectedMemory> = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();
    for g in sel.groups {
        let ns = match g.scope {
            MemoryScope::User => "user".to_string(),
            MemoryScope::Global => "global".to_string(),
            // A group whose scope cannot be resolved is SKIPPED, never guessed
            // at: a chat with no project has no project memories to read.
            MemoryScope::Project => match project {
                Some(p) => format!("project:{p}"),
                None => continue,
            },
        };
        // C3 (t8): read MORE rows than the group keeps, then order by the decay
        // score and cut. Ordering the already-limited "N most recent" rows would
        // make the usage term decorative: it could reorder the window but never
        // change which rows are in it. The overfetch factor is a named constant.
        let read = g
            .limit
            .saturating_mul(ruagent_memory::usage::DECAY_OVERFETCH);
        if let Ok(rows) = ruagent_memory::query::current_memories(db, g.store, &ns, read).await {
            let usage = usage_of(db, &rows.iter().map(|m| m.id).collect::<Vec<_>>()).await;
            let mut ranked: Vec<(f64, SelectedMemory)> = rows
                .into_iter()
                .map(|m| {
                    // Δt = now − COALESCE(last_used_at, updated_at) (t31 / RV-B-1):
                    // BOTH columns are read here, so the use clock is an input and
                    // not merely the curve's steepness.
                    let (used, last_used) = usage.get(&m.id).cloned().unwrap_or((None, None));
                    let score = ruagent_memory::usage::decay_score(
                        0.0,
                        &m.updated_at,
                        last_used.as_deref(),
                        &now,
                        used,
                    );
                    (
                        score,
                        SelectedMemory {
                            id: m.id,
                            tag: g.tag,
                            content: m.content,
                            updated_at: m.updated_at,
                            score: None,
                        },
                    )
                })
                .collect();
            ranked.sort_by(|a, b| {
                b.0.partial_cmp(&a.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    // A tie must not depend on the read order: id ascending.
                    .then_with(|| a.1.id.cmp(&b.1.id))
            });
            ranked.truncate(g.limit as usize);
            out.extend(ranked.into_iter().map(|(_, m)| m));
        }
    }
    if sel.query_top_n > 0
        && let Some(embedder) = embedder
    {
        let tag = sel
            .groups
            .last()
            .map(|g| g.tag)
            .unwrap_or(ruagent_memory::inject::TAG_RELEVANT_MEMORIES);
        for (id, _store, _ns, content, score) in
            semantic_search(db, embedder, query, sel.query_top_n, sel.query_min_score).await
        {
            out.push(SelectedMemory {
                id,
                tag,
                content,
                updated_at: String::new(),
                score: Some(score),
            });
        }
    }
    // C3 (t8): the rows that actually reach an agent are recorded as USED, which
    // is the input the decay ordering above reads back on the next call. Until
    // this existed, recall happened 651 times on the live DB and no row was ever
    // written back, so "has this memory ever been used?" had no answer.
    let used: Vec<i64> = out.iter().map(|m| m.id).collect();
    let _ = ruagent_memory::usage::record_usage(db, &used).await;
    out
}

/// The two usage columns the decay clock needs, as ONE named shape:
/// `(access_count, last_used_at)`, each `None` meaning UNKNOWN.
///
/// A named alias rather than the inline nested tuple: that tuple is unreadable at
/// every use site (clippy `type_complexity`, found by wiki's forced re-check of
/// `crates/daemon` after t31 widened this type by adding `last_used_at`), and one
/// name makes the producer, the query and the consumer state the same shape once.
type UsageRow = (i64, Option<i64>, Option<String>);

/// The recorded usage of these rows: `id -> (access_count, last_used_at)`.
///
/// BOTH columns, because the decay clock is `COALESCE(last_used_at, updated_at)`
/// (t31 / RV-B-1): reading only the counter made the use timestamp write-only.
/// `None` in either half is "unknown", never zero/now.
///
/// A separate read rather than a field on `MemoryRow`: the row type is
/// serialized into the panel's API and a new field would move that response
/// shape — a change that belongs to the API owner, not to this selection rule.
async fn usage_of(
    db: &Db,
    ids: &[i64],
) -> std::collections::HashMap<i64, (Option<i64>, Option<String>)> {
    let mut map = std::collections::HashMap::new();
    let ids: Vec<i64> = ids.to_vec();
    if ids.is_empty() {
        return map;
    }
    let rows: Vec<UsageRow> = db
        .call(
            move |conn| -> Result<Vec<UsageRow>, ruagent_store::DbError> {
                let mut stmt = conn
                    .prepare("SELECT id, access_count, last_used_at FROM memories WHERE id = ?1")
                    .map_err(ruagent_store::DbError::from)?;
                let mut out = Vec::new();
                for id in &ids {
                    let mut rows = stmt.query([id]).map_err(ruagent_store::DbError::from)?;
                    if let Some(r) = rows.next().map_err(ruagent_store::DbError::from)? {
                        out.push((
                            r.get(0).map_err(ruagent_store::DbError::from)?,
                            r.get(1).map_err(ruagent_store::DbError::from)?,
                            r.get(2).map_err(ruagent_store::DbError::from)?,
                        ));
                    }
                }
                Ok(out)
            },
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    for (id, n, last) in rows {
        map.insert(id, (n, last));
    }
    map
}

/// The candidate set for ONE merge decision (R-B C2 / E.2), scored and bounded.
///
/// WHAT THIS REPLACES. `distill::mergeable_target` read every live row of the
/// scope with its own SQL, judged them lexically, and kept only the verdict — no
/// candidate bound, no similarity evidence, and no record of the decision. The
/// measurements behind the shape (R-B A.9, 2026-09-27T21:50+08:00): over 2297
/// live pairs the lexical filter merged **0**, while the embedder put 6 pairs at
/// cosine 0.9827–0.9884 that are the same fact in different words; and 45.19% of
/// all pairs sit at cosine ≥ 0.86, so a fixed similarity threshold cannot decide
/// anything either.
///
/// `tau_scope` therefore comes from THE SCOPE'S OWN distribution (p99 by
/// `ruagent_memory::dedupe::TAU_PERCENTILE`), never from a constant: when the
/// scope has too few rows to have a distribution, the default is kept and that
/// fact is visible in the returned config.
pub async fn merge_candidates(
    db: &Db,
    embedder: Arc<dyn Embedder>,
    store: ruagent_memory::MemoryStore,
    namespace: &str,
    content: &str,
) -> (
    Vec<ruagent_memory::MergeCandidate>,
    ruagent_memory::MergeConfig,
) {
    use ruagent_memory::dedupe::{MergeCandidate, MergeConfig, TAU_PERCENTILE, scope_tau};
    let mut cfg = MergeConfig::default();
    let Ok(scope) = ruagent_memory::query::scope_rows(db, store, namespace).await else {
        return (Vec::new(), cfg);
    };
    if scope.is_empty() {
        return (Vec::new(), cfg);
    }
    let ids: Vec<i64> = scope.iter().map(|(id, _)| *id).collect();
    let vectors = embeddings_of(db, &ids).await;
    let Ok(mut qv) = embedder.embed(&[content]) else {
        // No vector for the new content: the decision falls back to the lexical
        // filter alone, with `cosine: None` on every candidate — which is
        // UNKNOWN, not "unrelated" (the candidate still gets judged).
        return (
            scope
                .into_iter()
                .map(|(id, c)| MergeCandidate {
                    id,
                    content: c,
                    cosine: None,
                })
                .collect(),
            cfg,
        );
    };
    let Some(q) = qv.pop() else {
        return (Vec::new(), cfg);
    };
    let qn: f32 = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mut scope_vecs: Vec<Vec<f32>> = Vec::new();
    let cands: Vec<MergeCandidate> = scope
        .into_iter()
        .map(|(id, c)| {
            let cosine = vectors.get(&id).and_then(|blob| {
                let v = from_blob(blob);
                if v.len() != q.len() || v.is_empty() {
                    return None; // different embedder generation — unknown
                }
                scope_vecs.push(v.clone());
                let vn: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                if qn == 0.0 || vn == 0.0 {
                    return None;
                }
                Some(v.iter().zip(q.iter()).map(|(a, b)| a * b).sum::<f32>() / (vn * qn))
            });
            MergeCandidate {
                id,
                content: c,
                cosine,
            }
        })
        .collect();
    // The scope's own similarity level. Sampled: a scope of hundreds of rows
    // would make the pairwise distribution quadratic, so the estimate is taken
    // over the first TAU_SAMPLE vectors and SAID to be an estimate.
    let sampled: Vec<&Vec<f32>> = scope_vecs.iter().take(TAU_SAMPLE).collect();
    let mut cosines: Vec<f32> = Vec::new();
    for (i, a) in sampled.iter().enumerate() {
        for b in sampled.iter().skip(i + 1) {
            let an: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let bn: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
            if an > 0.0 && bn > 0.0 && a.len() == b.len() {
                cosines.push(a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>() / (an * bn));
            }
        }
    }
    if let Some(tau) = scope_tau(&cosines, TAU_PERCENTILE) {
        cfg.tau_scope = tau;
    }
    tracing::debug!(
        candidates = cands.len(),
        sampled_pairs = cosines.len(),
        tau_scope = cfg.tau_scope,
        "merge candidate set built"
    );
    (cands, cfg)
}

/// How many scope vectors the `tau_scope` estimate is taken over.
const TAU_SAMPLE: usize = 64;

/// The embeddings of these rows, by id (`null` blobs are simply absent).
async fn embeddings_of(db: &Db, ids: &[i64]) -> std::collections::HashMap<i64, Vec<u8>> {
    let mut map = std::collections::HashMap::new();
    let ids: Vec<i64> = ids.to_vec();
    let rows: Vec<(i64, Vec<u8>)> = db
        .call(
            move |conn| -> Result<Vec<(i64, Vec<u8>)>, ruagent_store::DbError> {
                let mut stmt = conn
                    .prepare("SELECT id, embedding FROM memories WHERE id = ?1")
                    .map_err(ruagent_store::DbError::from)?;
                let mut out = Vec::new();
                for id in &ids {
                    let mut rows = stmt.query([id]).map_err(ruagent_store::DbError::from)?;
                    if let Some(r) = rows.next().map_err(ruagent_store::DbError::from)? {
                        let blob: Option<Vec<u8>> =
                            r.get(1).map_err(ruagent_store::DbError::from)?;
                        if let Some(b) = blob {
                            out.push((r.get(0).map_err(ruagent_store::DbError::from)?, b));
                        }
                    }
                }
                Ok(out)
            },
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();
    for (id, blob) in rows {
        map.insert(id, blob);
    }
    map
}

/// Decide whether `content` may replace a row in this scope, AND record the
/// decision (R-B C2 / X-3).
///
/// Returns the verdict and the audit reason. Until this existed, `memory_diffs`
/// held the OUTCOME of every write (insert / supersede / skip_dedupe) and no row
/// for the decision that produced it — so a refused merge and an escalated one
/// were equally invisible.
///
/// The caller is distillation (I-C/t9, `crates/daemon/src/distill.rs`): it owns
/// what to DO with `NeedsJudgement`. Nothing here writes a memory row.
pub async fn judge_merge(
    db: &Db,
    embedder: Arc<dyn Embedder>,
    store: ruagent_memory::MemoryStore,
    namespace: &str,
    content: &str,
) -> (ruagent_memory::MergeVerdict, String) {
    let (cands, cfg) = merge_candidates(db, embedder, store, namespace, content).await;
    // The AUDITED decision (t31 / RV-B-low-②): the reason carries the source, the
    // anchor, the neighbour the decision did NOT follow and every row over tau.
    let decision = ruagent_memory::dedupe::merge_decision_audited(content, &cands, &cfg);
    let reason = ruagent_memory::merge_audit_reason(&decision, cands.len());
    // The audit's `before` names the row the decision is grounded on (None for
    // `New`): the same value the reason's `anchor=` carries, from one source.
    let candidate = decision.anchor;
    if let Some(ns) = ruagent_memory::Namespace::parse(namespace) {
        let _ = ruagent_memory::audit_merge_decision(db, store, &ns, candidate, &reason).await;
    }
    (decision.verdict, reason)
}

/// Recall memories across BOTH legs, fused on one scale and bounded by top_n.
///
/// THE BUG THIS REPLACES (t246): the handler took the semantic leg, seeded a
/// seen-set from it, then APPENDED keyword rows that were not in the set. So
/// (a) the result was a concatenation, not a ranking; (b) the total was bounded
/// by 2 * top_n, not top_n; (c) keyword rows carried no score at all, so no
/// consumer could compare or re-rank them; (d) recall_log could not tell a
/// keyword contribution from a keyword discard.
///
/// The semantic leg keeps its cosine floor (the caller's threshold): that is
/// the precision gate. The keyword leg is admitted by RANK (its top_n by
/// bm25), because bm25 has no absolute scale to threshold on. The two ranked
/// lists are then fused with RRF, which is the only scale that is honestly
/// comparable across a cosine leg and a bm25 leg.
pub async fn recall_memories(
    db: &Db,
    embedder: Arc<dyn Embedder>,
    query: &str,
    top_n: u32,
    min_score: f32,
) -> RecallLegs {
    let semantic = semantic_search(db, embedder, query, top_n, min_score).await;
    let keyword = ruagent_memory::query::search_fts_scored(db, &keyword_pattern(query), top_n)
        .await
        .unwrap_or_default();

    let sem_ids: Vec<i64> = semantic.iter().map(|m| m.0).collect();
    let kw_ids: Vec<i64> = keyword.iter().map(|(row, _)| row.id).collect();
    let keyword_new = kw_ids.iter().filter(|id| !sem_ids.contains(id)).count();

    let fused = ruagent_knowledge::rrf(&[sem_ids, kw_ids], RRF_K);
    let dropped_by_top_n = fused.len().saturating_sub(top_n as usize);

    // Row data from either leg: the semantic leg's tuple, the keyword leg's row.
    let mut row_of: std::collections::HashMap<i64, (String, String, String)> =
        std::collections::HashMap::new();
    let mut sem_of: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    let mut kw_of: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for (id, store, ns, content, score) in &semantic {
        row_of.insert(*id, (store.clone(), ns.clone(), content.clone()));
        sem_of.insert(*id, *score as f64);
    }
    for (row, score) in &keyword {
        row_of.insert(
            row.id,
            (
                row.store.as_str().to_string(),
                row.namespace.clone(),
                row.content.clone(),
            ),
        );
        kw_of.insert(row.id, *score);
    }

    let hits = fused
        .into_iter()
        .take(top_n as usize)
        .filter_map(|(id, score)| {
            let (store, namespace, content) = row_of.get(&id)?.clone();
            let mut legs = Vec::new();
            if sem_of.contains_key(&id) {
                legs.push("semantic");
            }
            if kw_of.contains_key(&id) {
                legs.push("keyword");
            }
            Some(MemoryHit {
                id,
                store,
                namespace,
                content,
                score: score as f64,
                legs,
                semantic_score: sem_of.get(&id).copied(),
                keyword_score: kw_of.get(&id).copied(),
            })
        })
        .collect();

    RecallLegs {
        semantic: semantic.len(),
        keyword: keyword.len(),
        keyword_new,
        dropped_by_top_n,
        top_semantic_score: semantic.first().map(|m| m.4 as f64),
        hits,
    }
}

// ── The adapter: knowledge/wiki producers → the injection contract ─────────
//
// WHY THESE EXIST AND WHY THEY LIVE HERE. The two cross-region payloads of this
// generation (`SearchHit.relevance` from R-A, `wiki::WikiLead` from R-D) must
// reach `crates/memory/src/inject.rs` as PURE DATA — inject.rs must not depend on
// this crate, because the dependency would run the wrong way (memory is below the
// daemon). Somebody has to hold the mapping, and if the two injection paths
// (chat.rs, runs.rs) each wrote their own, the contract would have two adapters
// the moment they drift — the exact shape this repo keeps paying for.
//
// So the mapping has ONE home, in the file that already owns the injection
// selection rule, and the callers only compose:
//
// ```ignore
// let hits = kb.search(query, N).await?;               // knowledge's stable entry
// let mut enriched = Vec::new();
// for h in &hits {
//     let lead = if h.document.starts_with("wiki/") {
//         let slug = h.document.trim_start_matches("wiki/").trim_end_matches(".md");
//         crate::wiki::lead_for(kb, None, slug).await.as_ref().map(lead_meta)
//     } else { None };
//     // `relevance` comes from whichever calibrated value the retrieval produced:
//     let relevance = /* I-A: RelevanceScore -> */ Some(relevance_meta(
//         r.value, r.kind.as_str(), r.version, Some(r.query_background),
//     ));
//     enriched.push(enriched_hit(h, relevance, lead));
// }
// items.extend(knowledge_items_enriched(&enriched, KNOWLEDGE_SOURCES, WIKI_PAGES));
// ```
//
// WHAT THIS DELIBERATELY DOES NOT DEPEND ON (measured, 2026-09-28T00:2x vs 00:3x):
// I-A's calibrated-relevance type (`RelevanceScore` / `RankedHit` / `SearchPage`)
// EXISTED in `crates/knowledge/src/store.rs` at 00:1x and was GONE by 00:3x —
// i.e. that surface is still moving. A daemon file that names it inherits the
// other region's refactors as compile failures (this file's build was red on
// `cannot find type RelevanceScore` while that refactor was in flight, and it
// blocked I-D's t10 too). So the adapter depends only on the STABLE surface
// (`SearchHit`, `SearchLegs`) and takes the calibrated numbers BY VALUE. When
// I-A's type settles, the call site above is one line and no adapter here changes.

/// One wiki lead, in the contract's shape. Three-state fields stay three-state:
/// `None` means "this surface cannot decide", and the renderer says `unknown`.
///
/// The anchors are narrowed from `Citation` to `(document, chunk_id)`: those are
/// the two fields a reader needs to FETCH the evidence
/// (`GET /api/v1/knowledge/expand/{chunk_id}`); the section and the build-time
/// chunk hash are for verifying a page against its sources, which the wiki side
/// does itself and the injected lead cannot act on.
pub fn lead_meta(lead: &crate::wiki::WikiLead) -> ruagent_memory::inject::WikiLeadMeta {
    ruagent_memory::inject::WikiLeadMeta {
        slug: lead.slug.clone(),
        stale: lead.stale,
        stale_since: lead.stale_since.clone(),
        edited: lead.edited,
        // Passed through UNCHANGED, in both directions (RV-D-1 / wiki t34): the
        // contract's `cite_coverage` is three-state for the same reason `stale`
        // and `edited` are, and an adapter that filled `None` with 0.0 or 1.0
        // would recreate exactly the defect RV-D-1 removed.
        cite_coverage: lead.cite_coverage,
        anchors: lead
            .anchors
            .iter()
            .map(|c| (c.document.clone(), c.chunk_id))
            .collect(),
        hint: lead.hint.to_string(),
    }
}

/// One calibrated relevance, in the contract's shape — taken BY VALUE, so this
/// file does not name the (still moving) knowledge-side type. See the note above.
///
/// `kind` must be the frozen LITERAL the producer publishes (`"calibrated"` for a
/// calibrated relevance, `ScoreKind::as_str()` on the knowledge side) — not a
/// re-spelled copy here: the literal is a wire value, and inventing a second
/// spelling is how two scales end up under one name (R-A A7).
pub fn relevance_meta(
    value: f32,
    kind: &'static str,
    version: u32,
    query_background: Option<f32>,
) -> ruagent_memory::inject::RelevanceMeta {
    ruagent_memory::inject::RelevanceMeta {
        value,
        kind,
        version,
        query_background,
    }
}

/// ONE retrieval hit, ready for the injection contract: the hit itself, the
/// calibrated relevance when the retrieval produced one, and the wiki lead card
/// when the hit is a generated page.
pub fn enriched_hit(
    hit: &ruagent_knowledge::SearchHit,
    relevance: Option<ruagent_memory::inject::RelevanceMeta>,
    lead: Option<ruagent_memory::inject::WikiLeadMeta>,
) -> ruagent_memory::inject::EnrichedHit {
    ruagent_memory::inject::EnrichedHit {
        hit: ruagent_memory::inject::RetrievalHit {
            document: hit.document.clone(),
            content: hit.content.clone(),
            score: hit.score,
            wiki: hit.document.starts_with("wiki/"),
        },
        lead,
        relevance,
    }
}

/// THE GRAPH EVIDENCE ADAPTER (t58 / F5 / G10): `crates/graph`'s evidence → the
/// contract's `<graph>` items.
///
/// The producer of the CONTENT is `crates/graph` (`GraphEvidence::lines()` renders
/// one line per path with hops/temporal state/source in that same line, RVC-5);
/// this function is the only place the daemon converts it, so the call sites stay
/// one line and the contract stays the owner of the block shape.
///
/// NOTHING IS INVENTED HERE: an evidence with no paths yields no items, and the
/// caller then emits NO `<graph>` block (never a placeholder saying "no graph
/// evidence"), which is the same honest failure mode as `knowledge_items`.
///
/// The budget comes from the contract (`inject::GRAPH_PATHS`) at the call site —
/// this adapter takes it as a parameter so a caller can narrow it deliberately,
/// never widen it by accident.
pub fn graph_evidence_items(
    evidence: &ruagent_graph::GraphEvidence,
    limit: usize,
) -> Vec<ruagent_memory::inject::ContextItem> {
    ruagent_memory::inject::graph_items(&evidence.lines(), limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip() {
        let v = vec![0.1f32, -0.5, 3.0];
        let b = to_blob(&v);
        assert_eq!(b.len(), 12);
        assert_eq!(from_blob(&b), v);
    }

    /// The adapter must not turn "not decidable" into a value, and must not
    /// re-spell the frozen score kind. Both are falsifiable in one line each.
    ///
    /// RV-D-1 (wiki's t34) made `cite_coverage` three-state too: the value is
    /// passed through UNCHANGED in both directions, so `None` stays `None` and
    /// `Some(0.5)` stays `Some(0.5)` — the adapter must never synthesise a ratio.
    #[test]
    fn the_adapter_preserves_three_states_and_the_frozen_kind_literal() {
        let lead = crate::wiki::WikiLead {
            slug: "ops".into(),
            title: "Ops".into(),
            summary: "runbook".into(),
            stale: None,
            stale_since: None,
            edited: None,
            cite_coverage: None,
            // Not consumed by the adapter: the injection face renders the anchors
            // it can actually hand to a reader, so the page-level anchor counts
            // stay wiki's own business (they are recorded, not resolved).
            has_anchors: true,
            anchored_sections: 1,
            anchors: vec![crate::wiki::Citation {
                section: "## Pods".into(),
                document: "ops-handbook".into(),
                chunk_id: 18,
                chunk_hash: "deadbeef".into(),
            }],
            hint: crate::wiki::LEAD_HINT,
        };
        let meta = lead_meta(&lead);
        println!(
            "READING adapter: wiki lead -> stale={:?} edited={:?} coverage={:?} anchors={:?}",
            meta.stale, meta.edited, meta.cite_coverage, meta.anchors
        );
        assert_eq!(meta.stale, None);
        assert_eq!(meta.edited, None, "None must survive the adapter");
        assert_eq!(
            meta.cite_coverage, None,
            "an unknown coverage must not be invented by the adapter"
        );
        let mut some = lead.clone();
        some.cite_coverage = Some(0.5);
        assert_eq!(lead_meta(&some).cite_coverage, Some(0.5));
        assert_eq!(meta.anchors, vec![("ops-handbook".to_string(), 18)]);
        assert_eq!(meta.hint, ruagent_memory::inject::WIKI_LEAD_HINT);

        // The calibrated numbers are taken BY VALUE, so this file never names the
        // knowledge side's (still moving) relevance type. When it settles, the
        // call site passes `r.kind.as_str()`.
        let rm = relevance_meta(0.75, "calibrated", 1, Some(0.31));
        println!("READING adapter: relevance -> {rm:?}");
        assert_eq!(rm.kind, "calibrated");
        assert_eq!(rm.version, 1);
        assert_eq!(rm.query_background, Some(0.31));

        let hit = ruagent_knowledge::SearchHit {
            chunk_id: 18,
            document: "wiki/ops".into(),
            content: "body".into(),
            score: 0.016,
        };
        let enriched = enriched_hit(&hit, Some(rm), Some(meta));
        println!(
            "READING adapter: enriched order_value={}",
            enriched.order_value()
        );
        assert!(enriched.hit.wiki, "a wiki/ document is a generated page");
        assert_eq!(enriched.order_value(), 0.75, "ordering uses relevance");
        let plain = enriched_hit(&hit, None, None);
        assert_eq!(
            plain.order_value(),
            0.016,
            "without a relevance the rank score still orders"
        );
    }

    /// t58 / F5 / G10: the graph adapter's two readings — a real `GraphEvidence`
    /// with one path becomes a `<graph>` block, and an evidence with NO paths
    /// becomes **no block at all** (the negative control the finding asked for).
    ///
    /// The fixture is built by hand because `retrieve` needs a database; the
    /// POINT here is the adapter, not the retrieval (graph's own tests cover the
    /// walk). `lines()` is graph's renderer, so this also pins that the daemon
    /// does not re-render the evidence itself.
    #[test]
    fn graph_evidence_becomes_a_block_and_empty_evidence_becomes_nothing() {
        let ed = |id: i64, src: i64, dst: i64, src_name: &str, dst_name: &str, rel: &str| {
            ruagent_graph::EvidenceEdge {
                edge_id: id,
                src,
                dst,
                src_name: src_name.into(),
                dst_name: dst_name.into(),
                relation: rel.into(),
                fact_text: format!("{src_name} {rel} {dst_name}"),
                valid_at: "2026-09-20T00:00:00Z".into(),
                invalid_at: None,
                temporal: ruagent_graph::TemporalStatus::Current,
                source: ruagent_graph::EdgeSource {
                    episode: Some(7),
                    session_key: Some("session:abc".into()),
                    recorded_at: "2026-09-20T00:00:00Z".into(),
                },
            }
        };
        let node = |id: i64, name: &str| ruagent_graph::Entity {
            id,
            name: name.into(),
            kind: Some("tool".into()),
            summary: None,
        };
        let evidence = ruagent_graph::GraphEvidence {
            seeds: vec![ruagent_graph::SeedHit {
                entity: node(1, "用户19410"),
                leg: ruagent_graph::SeedLeg::ExactName,
                score: 1.0,
            }],
            paths: vec![ruagent_graph::EvidencePath {
                hops: 1,
                score: 1.0,
                nodes: vec![node(1, "用户19410"), node(2, "微信")],
                edges: vec![ed(12, 1, 2, "用户19410", "微信", "uses")],
                why: ruagent_graph::PathRationale {
                    seed_leg: ruagent_graph::SeedLeg::ExactName,
                    factors: vec![1.0, 1.0],
                },
            }],
            stats: ruagent_graph::RetrievalStats {
                seeds_by_leg: vec![(ruagent_graph::SeedLeg::ExactName, 1)],
                frontier_sizes: vec![1],
                paths_considered: 1,
                paths_emitted: 1,
                truncated_by: None,
                empty_reason: None,
                graph_edges: 67,
            },
        };
        let lines = evidence.lines();
        println!("READING t58 adapter: lines()={lines:?}");
        let items = graph_evidence_items(&evidence, ruagent_memory::inject::GRAPH_PATHS);
        println!("READING t58 adapter: items={}", items.len());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tag, ruagent_memory::inject::TAG_GRAPH);
        assert!(items[0].content.contains(" -uses-> 微信"), "{:?}", items[0]);
        assert!(
            items[0].content.contains("hops=1") && items[0].content.contains("state="),
            "the line must keep its provenance: {:?}",
            items[0]
        );
        let out = ruagent_memory::inject::render_context(
            &items,
            &ruagent_memory::inject::InjectionBudget::default(),
        );
        println!("READING t58 adapter: rendered block:\n{out}");
        assert!(
            out.starts_with("<graph>\n") && out.ends_with("</graph>\n"),
            "{out}"
        );

        // NEGATIVE CONTROL: structurally-valid evidence, no path ⇒ no items ⇒ the
        // caller emits no block (no placeholder).
        let empty = ruagent_graph::GraphEvidence {
            seeds: Vec::new(),
            paths: Vec::new(),
            stats: ruagent_graph::RetrievalStats {
                seeds_by_leg: Vec::new(),
                frontier_sizes: Vec::new(),
                paths_considered: 0,
                paths_emitted: 0,
                truncated_by: None,
                empty_reason: Some(ruagent_graph::EmptyReason::NoSeed),
                graph_edges: 67,
            },
        };
        let empty_items = graph_evidence_items(&empty, ruagent_memory::inject::GRAPH_PATHS);
        println!(
            "READING t58 adapter: empty evidence -> items={} render={:?}",
            empty_items.len(),
            ruagent_memory::inject::render_context(
                &empty_items,
                &ruagent_memory::inject::InjectionBudget::default()
            )
        );
        assert!(empty_items.is_empty());
    }
}
