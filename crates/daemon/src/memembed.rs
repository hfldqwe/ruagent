//! Memory embeddings: the semantic leg of memory recall. Rows are
//! embedded with the SAME model the knowledge base uses (one vector
//! space), stored as f32 BLOBs in the memories table, searched with
//! brute-force cosine — memory counts are hundreds, not millions.

use std::sync::Arc;

use ruagent_knowledge::embed::Embedder;
use ruagent_knowledge::rrf::{WeightError, check_weights, normalized_weight};
use ruagent_knowledge::store::LegConfig;
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
#[derive(Debug, Clone, Default, PartialEq)]
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

/// One recall leg, as a TYPE.
///
/// THE CAPABILITY IDS ARE NOT SPELLED HERE (t5 convergence, captain's review
/// handoff): the registry owns them — `crate::capability::CapabilityId::as_str()`
/// (`recall_leg_memory_semantic`, `recall_leg_wiki`, …) is the single source — and
/// the recall endpoint maps each variant onto its id when it builds the wire
/// answer's `legs_disabled` (design §11.4). A second hardcoded copy of those six
/// strings in this file would be a shadow list no test here could keep honest; a
/// mapping function is what stays checkable, and the endpoint owns it because the
/// endpoint is what speaks capability ids.
///
/// DECLARATION ORDER IS THE REPORT ORDER of [`RecallLegConfig::disabled_legs`], and
/// it is also the order of the ids each variant maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecallLeg {
    Graph,
    KnowledgeFts,
    KnowledgeSemantic,
    MemoryFts,
    MemorySemantic,
    Wiki,
}

impl RecallLeg {
    /// A HUMAN label for a log line or a reason sentence — NOT a capability id and
    /// never a wire value (see the type's note on where the ids live).
    pub const fn label(self) -> &'static str {
        match self {
            RecallLeg::Graph => "graph",
            RecallLeg::KnowledgeFts => "knowledge fts",
            RecallLeg::KnowledgeSemantic => "knowledge semantic",
            RecallLeg::MemoryFts => "memory fts",
            RecallLeg::MemorySemantic => "memory semantic",
            RecallLeg::Wiki => "wiki",
        }
    }
}

/// Which memory legs to run, and with what RRF weight (t5).
///
/// DEFAULT = TODAY EXACTLY: both legs on, weight 1.0 each. At 1.0/1.0 the fusion
/// is `rrf` itself — `rrf_weighted` is pinned bit-identical to `rrf` at equal
/// weights (crates/knowledge/src/rrf.rs, `unweighted_is_the_1_1_special_case`),
/// which is what lets this struct be threaded in without moving one rank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryLegs {
    /// The cosine leg (`semantic_search`). It owns the query EMBEDDING.
    pub semantic: bool,
    /// The bm25 leg (`search_fts_scored`).
    pub keyword: bool,
    pub w_semantic: f32,
    pub w_keyword: f32,
}

impl Default for MemoryLegs {
    fn default() -> Self {
        Self {
            semantic: true,
            keyword: true,
            w_semantic: 1.0,
            w_keyword: 1.0,
        }
    }
}

impl MemoryLegs {
    /// Every memory leg off: legal, returns empty evidence, and queries nothing.
    pub fn all_off() -> Self {
        Self {
            semantic: false,
            keyword: false,
            w_semantic: 0.0,
            w_keyword: 0.0,
        }
    }

    /// THE ONE RESOLVER for the memory pair: `(enabled, configured weight)` per
    /// leg -> a usable configuration, `None` meaning "no weight configured" and
    /// taking 1.0 (never zero, which would silently drop an enabled leg).
    ///
    /// Validation and normalization both happen here, so the daemon never holds a
    /// config whose disabled half still carries a weight.
    pub fn resolve(
        semantic: (bool, Option<f64>),
        keyword: (bool, Option<f64>),
    ) -> Result<Self, WeightError> {
        let cfg = Self {
            semantic: semantic.0,
            keyword: keyword.0,
            w_semantic: semantic.1.map_or(1.0, |w| w as f32),
            w_keyword: keyword.1.map_or(1.0, |w| w as f32),
        };
        cfg.validate()?;
        Ok(cfg.normalized())
    }

    /// The weight rule of [`ruagent_knowledge::rrf::check_weights`]: non-finite
    /// weights are rejected, an ENABLED leg at weight <= 0 is rejected (that is
    /// what makes two enabled legs at all-zero weights a configuration error
    /// rather than an empty ranking that reads like "nothing matched"), and a
    /// disabled leg's weight is ignored.
    pub fn validate(&self) -> Result<(), WeightError> {
        check_weights(&[
            (
                RecallLeg::MemorySemantic.label(),
                self.semantic,
                self.w_semantic,
            ),
            (RecallLeg::MemoryFts.label(), self.keyword, self.w_keyword),
        ])
    }

    /// Zero every disabled leg's weight — the total half of the rule.
    pub fn normalized(self) -> Self {
        Self {
            w_semantic: normalized_weight(self.semantic, self.w_semantic),
            w_keyword: normalized_weight(self.keyword, self.w_keyword),
            ..self
        }
    }

    /// THE EFFECTIVE WEIGHTS of this configuration: what each leg actually
    /// contributes to the fusion, after normalization. [`fuse_memory_legs`] builds
    /// its inputs from THIS value and [`Self::fusion_label`] renders the same value,
    /// so the label a record carries cannot describe a fusion other than the one
    /// that ran.
    pub fn effective(&self) -> Self {
        self.normalized()
    }

    /// The fusion expression as a LABEL: `"rrf:k=60,w_semantic=3,w_keyword=1"`.
    ///
    /// THE SPELLING IS THE RECALL RESPONSE'S, not the knowledge type's. Two strings
    /// already exist for this one concept and they are NOT the same:
    /// [`ruagent_knowledge::store::FusionKind::label`] renders
    /// `"rrf(k=60,w_sem=3,w_kw=1)"`, while the recall endpoint re-spells the
    /// knowledge label as `"rrf:k=...,w_semantic=...,w_keyword=..."` and does not
    /// call that `label()` (see the note at the construction site in `api.rs`). The
    /// endpoint's spelling is the one a reader of a payload meets in
    /// `scoring.fusion`, and `memory_fusion` sits in the SAME payload: emitting the
    /// type's shape here would ask one reader to learn two formats for one idea.
    /// Unifying the pre-existing pair by changing `scoring.fusion` would be a
    /// non-additive change to an existing wire value, so only this NEW key follows
    /// the response it lives in. The format is pinned by
    /// `the_memory_fusion_label_carries_the_effective_weights` here and, over HTTP,
    /// by `parse_rrf_label`, which parses BOTH labels of one payload.
    ///
    /// WHY THIS EXISTS (t2, the memory half of §20.2): the fusion's weights reached
    /// the ranking but NOT the record, so a reader of a recall response or of
    /// `recall_log` could see the hit COUNTS and not the numbers that produced them
    /// — "the weight changed" and "the corpus changed" were the same row.
    ///
    /// The numbers are the EFFECTIVE ones (`normalized_weight`), never the raw
    /// configured ones: a weight the ranking did not use is not evidence about the
    /// ranking. A DISABLED leg therefore reads `0` whatever the file carried next to
    /// `enabled = false` — and that `0` unambiguously means "off", because an
    /// ENABLED leg at weight <= 0 is REFUSED by [`Self::validate`] ("disable the leg
    /// instead of zeroing it"), so it is a value the fusion can never be given.
    pub fn fusion_label(&self) -> String {
        let effective = self.effective();
        let (w_semantic, w_keyword) = (effective.w_semantic, effective.w_keyword);
        format!("rrf:k={RRF_K},w_semantic={w_semantic},w_keyword={w_keyword}")
    }

    /// The memory legs this configuration turns OFF, in a stable order (types, not
    /// capability ids — see [`RecallLeg`]).
    pub fn disabled_legs(&self) -> Vec<RecallLeg> {
        let mut out = Vec::new();
        if !self.semantic {
            out.push(RecallLeg::MemorySemantic);
        }
        if !self.keyword {
            out.push(RecallLeg::MemoryFts);
        }
        out.sort_unstable();
        out
    }

    pub fn all_disabled(&self) -> bool {
        !self.semantic && !self.keyword
    }
}

/// THE ONE PLACE the six recall legs are configured (t5).
///
/// The four legs this file and `ruagent_knowledge` execute live in `memory` and
/// `knowledge`; the two legs the recall ENDPOINT executes (wiki stubs, entity
/// graph) are booleans here, so a caller reads all six from one value and reports
/// one `legs_disabled` list instead of re-deriving the same conditions at each
/// call site. Wiki and graph carry no weight: neither is a ranked fusion input
/// (wiki is a partition of the knowledge hits, and the graph's `retrieve` has its
/// own path budget) — inventing a weight for them would be a number nothing
/// reads.
///
/// `Default` = today: every leg on, every weight at today's value. A caller must
/// build this from the capability plane and pass it in; nothing here reads
/// configuration, so this struct is also the seam that keeps the pipeline
/// testable without a daemon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecallLegConfig {
    pub memory: MemoryLegs,
    pub knowledge: LegConfig,
    pub wiki: bool,
    pub graph: bool,
}

impl Default for RecallLegConfig {
    fn default() -> Self {
        Self {
            memory: MemoryLegs::default(),
            knowledge: LegConfig::default(),
            wiki: true,
            graph: true,
        }
    }
}

impl RecallLegConfig {
    /// Resolve all six legs from the plane's `(enabled, weight)` readings in one
    /// call, so no call site invent their own combination. Wiki and graph are
    /// switches only (see the type's note on weights).
    pub fn resolve(
        memory_semantic: (bool, Option<f64>),
        memory_keyword: (bool, Option<f64>),
        knowledge_semantic: (bool, Option<f64>),
        knowledge_keyword: (bool, Option<f64>),
        wiki: bool,
        graph: bool,
    ) -> Result<Self, WeightError> {
        Ok(Self {
            memory: MemoryLegs::resolve(memory_semantic, memory_keyword)?,
            knowledge: LegConfig::resolve(knowledge_semantic, knowledge_keyword)?,
            wiki,
            graph,
        })
    }

    /// Every leg off. Legal: a recall that was asked for nothing must answer
    /// "nothing was asked for", not an error and not a bare empty list.
    pub fn all_disabled(&self) -> bool {
        self.memory.all_disabled() && self.knowledge.all_disabled() && !self.wiki && !self.graph
    }

    /// Every leg this configuration turns OFF, in [`RecallLeg`]'s declaration
    /// order — which is also the id order of the wire array the endpoint builds
    /// (design §11.4). THE ONE PLACE the disabled-leg reading comes from: the
    /// endpoint reports exactly this list instead of re-deriving six conditions.
    ///
    /// NO ID IS SPELLED HERE: the endpoint maps each variant onto
    /// `crate::capability::CapabilityId` (the registry's `as_str()` is the single
    /// source of the strings). The mapping is the endpoint's because the endpoint is
    /// what speaks capability ids.
    pub fn disabled_legs(&self) -> Vec<RecallLeg> {
        let mut out: Vec<RecallLeg> = self
            .memory
            .disabled_legs()
            .into_iter()
            .chain(
                self.knowledge
                    .disabled_legs()
                    .into_iter()
                    .map(|leg| match leg {
                        ruagent_knowledge::store::KnowledgeLeg::Semantic => {
                            RecallLeg::KnowledgeSemantic
                        }
                        ruagent_knowledge::store::KnowledgeLeg::Keyword => RecallLeg::KnowledgeFts,
                    }),
            )
            .collect();
        if !self.wiki {
            out.push(RecallLeg::Wiki);
        }
        if !self.graph {
            out.push(RecallLeg::Graph);
        }
        out.sort_unstable();
        out
    }

    /// The explicit reason an ALL-OFF configuration returns an empty result.
    ///
    /// `None` whenever at least one leg is on. This is the answer to "empty
    /// because nothing matched" vs "empty because nothing was asked for": the
    /// endpoint puts this sentence (and the mapped `legs_disabled`) in the
    /// response, so an all-off recall can never read as a retrieval miss.
    pub fn disabled_reason(&self) -> Option<String> {
        if !self.all_disabled() {
            return None;
        }
        let labels: Vec<&'static str> = self.disabled_legs().iter().map(|l| l.label()).collect();
        Some(format!(
            "every recall leg is disabled by configuration ({})",
            labels.join(", ")
        ))
    }
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
///
/// This is the DEFAULT entry point (t5): it delegates to
/// [`recall_memories_with`] with [`MemoryLegs::default`], so a caller that knows
/// nothing about leg configuration gets exactly the pre-t5 behaviour.
pub async fn recall_memories(
    db: &Db,
    embedder: Arc<dyn Embedder>,
    query: &str,
    top_n: u32,
    min_score: f32,
) -> RecallLegs {
    recall_memories_with(
        db,
        embedder,
        query,
        top_n,
        min_score,
        &MemoryLegs::default(),
    )
    .await
}

/// THE MEMORY FUSION, as a pure function: the one place the two memory weights
/// become a ranking.
///
/// Extracted from [`recall_memories_with`] for two reasons: the weights' effect is
/// then decidable without a database (and the acceptance's "a non-default weight
/// reorders the fixture" observable does not depend on a fixture that happens to
/// have both legs alive), and the equivalence with the frozen unweighted `rrf`
/// becomes ONE assertion instead of an argument.
///
/// A DISABLED leg is dropped from the input list as well as zero-weighted: a
/// caller cannot re-inject documents through a leg the configuration turned off.
///
/// The weights come from [`MemoryLegs::effective`] — the same value
/// [`MemoryLegs::fusion_label`] renders into the record (t2), so the reported label
/// and the arithmetic here cannot drift apart.
fn fuse_memory_legs(sem_ids: &[i64], kw_ids: &[i64], legs: &MemoryLegs) -> Vec<(i64, f32)> {
    let legs = legs.effective();
    let mut inputs: Vec<(&[i64], f32)> = Vec::new();
    if legs.semantic {
        inputs.push((sem_ids, legs.w_semantic));
    }
    if legs.keyword {
        inputs.push((kw_ids, legs.w_keyword));
    }
    ruagent_knowledge::rrf_weighted(&inputs, RRF_K)
}

/// [`recall_memories`] under an explicit leg configuration (t5).
///
/// THE COST BOUNDARY, and the whole point of the parameter: a DISABLED leg is
/// never QUERIED — the gate wraps the call, not its result.
///
/// * `memory.semantic = false` skips the query EMBEDDING (`embed_query`, a model
///   forward pass on the fastembed path) and the brute-force cosine scan over
///   every embedded memory row.
/// * `memory.keyword = false` skips the FTS5 `MATCH` over `memories_fts`, which
///   is a scan of the whole memory index for this query.
///
/// Filtering the legs' results afterwards would return the same answer while
/// still paying every one of those costs — including the model call, which is the
/// user's token/time budget, not an implementation detail (design §11.3).
///
/// The caller that owns the HTTP response reports the disabled legs from
/// [`RecallLegConfig::disabled_legs`] / [`RecallLegConfig::disabled_reason`]: with
/// both memory legs off the evidence below is all-zero, which is identical to "the
/// corpus has nothing" unless the configuration says which legs were asked for.
pub async fn recall_memories_with(
    db: &Db,
    embedder: Arc<dyn Embedder>,
    query: &str,
    top_n: u32,
    min_score: f32,
    legs: &MemoryLegs,
) -> RecallLegs {
    let legs = legs.normalized();
    if legs.all_disabled() {
        tracing::info!(
            legs = ?legs.disabled_legs(),
            "memory recall: every leg is disabled by configuration — returning empty evidence \
             without querying anything"
        );
    }
    let semantic = if legs.semantic {
        semantic_search(db, embedder, query, top_n, min_score).await
    } else {
        Vec::new()
    };
    let keyword = if legs.keyword {
        ruagent_memory::query::search_fts_scored(db, &keyword_pattern(query), top_n)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let sem_ids: Vec<i64> = semantic.iter().map(|m| m.0).collect();
    let kw_ids: Vec<i64> = keyword.iter().map(|(row, _)| row.id).collect();
    let keyword_new = kw_ids.iter().filter(|id| !sem_ids.contains(id)).count();

    // The weights come from the configuration. At the default 1.0/1.0 this is
    // `rrf` itself, bit for bit (crates/knowledge/src/rrf.rs pins the identity);
    // a disabled leg arrives with weight 0.0 AND an empty id list, so both halves
    // of "off" are enforced here.
    let fused = fuse_memory_legs(&sem_ids, &kw_ids, &legs);
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

    // ── The t5 recall fixture ──────────────────────────────────────────────
    //
    // A FIXED, deterministic corpus with no model in the loop (the offline
    // `HashEmbedder`), built so the two memory legs DISAGREE. That is what makes
    // a per-leg toggle or a weight change observable: if both legs ranked the
    // same rows the same way, every assertion below would pass on a pipeline
    // that ignored the configuration entirely.
    //
    //   row                                    semantic            keyword
    //   M1 "alpha beta"                        1.000  (rank 0)     rank 0
    //   M2 "beta beta alpha alpha"             1.000  (rank 1)     absent
    //   M3 "alpha beta gamma delta eps zeta"   0.577  (rank 2)     rank 1
    //   M4 "alpha zeta eta theta iota kappa"   0.289  (rank 3)     absent
    //   M5 "zebra xylophone marmalade"         0.000               absent
    //
    // The keyword leg's pattern is the WHOLE query as one phrase (memembed's
    // `keyword_pattern`, deliberately unchanged), so only M1 and M3 contain
    // "alpha beta" adjacent; M2 is the same bag of words in another order, which
    // is exactly the semantic-leg-only row a weight can lift.
    //
    // WHICH BRANCH OF THE `search_fts_scored` DEFECT THIS FIXTURE EXERCISED (t13): it
    // writes `source_episode: None`, so the rows were all NULL in that column and that
    // call ERRED (see `memory_fts_leg_error`). REAL rows are not null there (t9 measured
    // 3 rows, 0 nulls) — the OTHER branch, where the leg ran and returned an EPISODE ID
    // as its score. Both branches were pinned by t13's tripwire and are pinned in the
    // corrected state by `the_source_episode_index_repair_reports_bm25_for_both_shapes`;
    // t16 repaired the read (§21.1), so the NULL branch reads real bm25 now.
    async fn fixture() -> (Db, Arc<dyn Embedder>, Vec<i64>) {
        let db = Db::open_in_memory().unwrap();
        let embedder: Arc<dyn Embedder> = Arc::new(ruagent_knowledge::HashEmbedder::new(1024));
        let mut ids = Vec::new();
        for content in [
            "alpha beta",
            "beta beta alpha alpha",
            "alpha beta gamma delta epsilon zeta",
            "alpha zeta eta theta iota kappa",
            "zebra xylophone marmalade tapestry",
        ] {
            let out = ruagent_memory::write::write_memory(
                &db,
                &ruagent_memory::write::MemoryWrite {
                    store: ruagent_memory::MemoryStore::Lesson,
                    namespace: ruagent_memory::Namespace::parse("project:t5")
                        .expect("test namespace parses"),
                    content: content.to_string(),
                    confidence: 0.9,
                    source_episode: None,
                    supersedes: None,
                },
            )
            .await
            .expect("fixture row is writable");
            let id = match out {
                ruagent_memory::write::WriteOutcome::Inserted(id) => id,
                other => panic!("fixture write must insert, got {other:?}"),
            };
            embed_row(&db, embedder.clone(), id, content).await;
            ids.push(id);
        }
        (db, embedder, ids)
    }

    /// THE REGRESSION BAR (t5). Written FIRST, against the pre-configuration
    /// entry point, so it can only stay green if making the legs configurable is
    /// a no-op at the default configuration: same ids, same order, same fused
    /// scores, same per-leg evidence.
    ///
    /// The expected values are a PINNED reading of this fixture, not a
    /// recomputation — a test that recomputed them would ratify any change.
    ///
    /// RE-DERIVED BY t16 (the `source_episode` index repair), NOT hand-edited until
    /// green: this fixture's rows have a NULL `source_episode`, so before the repair the
    /// keyword leg ERRORED here and contributed nothing, and the pinned order was
    /// `[1, 2, 3, 4]` with fused scores `0.016393 / 0.016129 / 0.015873 / 0.015625`.
    /// With the leg reading real bm25 it contributes rows 1 and 3, and the measured page
    /// is `[1, 3, 2, 4]` with `0.032787 / 0.032002 / 0.016129 / 0.015625`. That move is
    /// the INTENDED delta of increment 2 (see `the_source_episode_index_repair_...` and
    /// the design document's §21.1); nothing was re-tuned to keep the old order.
    #[tokio::test]
    async fn default_memory_recall_is_pinned_on_the_fixed_fixture() {
        let (db, embedder, ids) = fixture().await;
        let legs = recall_memories(&db, embedder.clone(), "alpha beta", 5, 0.25).await;
        let keyword_error = memory_fts_leg_error(&db, "alpha beta").await;
        println!(
            "READING t5 memory golden: ids={ids:?} hits={:?}",
            legs.hits
                .iter()
                .map(|h| (h.id, h.score, h.legs.clone()))
                .collect::<Vec<_>>()
        );
        println!(
            "READING t5 memory golden: semantic={} keyword={} keyword_new={} dropped_by_top_n={} \
             top_semantic_score={:?} keyword_leg_error={:?}",
            legs.semantic,
            legs.keyword,
            legs.keyword_new,
            legs.dropped_by_top_n,
            legs.top_semantic_score,
            keyword_error
        );

        let got: Vec<(i64, String)> = legs
            .hits
            .iter()
            .map(|h| (h.id, format!("{:.6}", h.score)))
            .collect();
        assert_eq!(
            got,
            vec![
                (ids[0], "0.032787".to_string()),
                (ids[2], "0.032002".to_string()),
                (ids[1], "0.016129".to_string()),
                (ids[3], "0.015625".to_string()),
            ],
            "the fixed fixture's default ranking moved (re-derived by t16: the keyword leg now \
             contributes rows 1 and 3 instead of erroring on this fixture's NULL source_episode)"
        );
        assert_eq!(legs.semantic, 4);
        assert_eq!(
            legs.keyword, 2,
            "this fixture's rows have a NULL `source_episode`, which used to make \
             `search_fts_scored` ERROR (no keyword rows at all); since the t16 repair the column is \
             read by name and the leg returns the two rows whose text contains the query phrase \
             ({keyword_error:?})"
        );
        assert_eq!(
            legs.keyword_new, 0,
            "both keyword rows are in the semantic leg too"
        );
        assert_eq!(legs.dropped_by_top_n, 0);
        assert!(
            (legs.top_semantic_score.unwrap_or(0.0) - 1.0).abs() < 1e-5,
            "{:?}",
            legs.top_semantic_score
        );
        assert_eq!(
            legs.hits[0].legs,
            vec!["semantic", "keyword"],
            "the leading row is found by BOTH legs now"
        );

        // THE EQUIVALENCE BAR, re-derived from today's OWN primitives rather than
        // from the pinned numbers above: rebuild the pre-t5 algorithm — both legs,
        // the frozen unweighted `rrf`, the same top_n cut — and require the
        // configured pipeline to agree with it exactly. This is what makes the
        // golden a regression bar and not a snapshot of whatever the new code does.
        let sem = semantic_search(&db, embedder.clone(), "alpha beta", 5, 0.25).await;
        let kw = ruagent_memory::query::search_fts_scored(&db, &keyword_pattern("alpha beta"), 5)
            .await
            .unwrap_or_default();
        let sem_ids: Vec<i64> = sem.iter().map(|m| m.0).collect();
        let kw_ids: Vec<i64> = kw.iter().map(|(row, _)| row.id).collect();
        let frozen = ruagent_knowledge::rrf(&[sem_ids.clone(), kw_ids.clone()], RRF_K);
        let configured = fuse_memory_legs(&sem_ids, &kw_ids, &MemoryLegs::default());
        println!("READING t5 memory golden: frozen_rrf={frozen:?}");
        assert_eq!(
            frozen, configured,
            "at the default weights the configured fusion must BE the frozen `rrf`"
        );
        assert_eq!(
            legs.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
            frozen.iter().take(5).map(|(id, _)| *id).collect::<Vec<_>>()
        );
        for (hit, (id, score)) in legs.hits.iter().zip(frozen.iter()) {
            assert_eq!(hit.id, *id);
            assert_eq!(hit.score, *score as f64, "the score must be the frozen one");
        }
    }

    // ── t5: the leg configuration ──────────────────────────────────────────

    /// An `Embedder` that counts the calls it receives: the only honest way to
    /// assert that a disabled leg was never QUERIED (a result-shaped assertion
    /// cannot tell "skipped" from "queried and empty").
    struct CountingEmbedder {
        inner: ruagent_knowledge::HashEmbedder,
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl CountingEmbedder {
        fn new() -> (Self, Arc<std::sync::atomic::AtomicUsize>) {
            let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            (
                Self {
                    inner: ruagent_knowledge::HashEmbedder::new(1024),
                    calls: calls.clone(),
                },
                calls,
            )
        }
    }

    impl Embedder for CountingEmbedder {
        fn embed(
            &self,
            texts: &[&str],
        ) -> Result<Vec<Vec<f32>>, ruagent_knowledge::embed::EmbedError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.embed(texts)
        }
        fn embed_query(
            &self,
            text: &str,
        ) -> Result<Vec<f32>, ruagent_knowledge::embed::EmbedError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.embed_query(text)
        }
        fn name(&self) -> &'static str {
            self.inner.name()
        }
        fn dim(&self) -> usize {
            self.inner.dim()
        }
    }

    /// A probe over `search_fts_scored` that SURFACES an error instead of swallowing it the
    /// way `recall_memories_with` does — it returns `None` when the leg's SQL ran.
    ///
    /// WHY IT STILL EXISTS AFTER THE t16 REPAIR: it is how the repair's own test asserts the
    /// NULL branch no longer fails (`assert!(null_error.is_none())`), so a regression back to
    /// a failing read is caught by name rather than as a mysterious empty leg. The fixture's
    /// rows all carry `source_episode: None`, and before t16 that made this call error with
    /// `Invalid column type Null at index: 10` — the shape that once made this effort believe
    /// the leg was dead (§21.1). REAL rows carry a non-NULL `source_episode`, and there the
    /// old code silently reported an episode id as the score instead.
    async fn memory_fts_leg_error(db: &Db, query: &str) -> Option<String> {
        ruagent_memory::query::search_fts_scored(db, &keyword_pattern(query), 5)
            .await
            .err()
            .map(|e| e.to_string())
    }

    /// Give every fixture row a NON-NULL `source_episode` — the state REAL rows are
    /// in (t9 measured 3 rows, 0 nulls) and the branch the fixture does not cover.
    ///
    /// Returns `id -> episode id`. EVERY matching row needs one when the OLD read is being
    /// reproduced: that read mapped its rows one by one, so a single NULL row failed the
    /// whole statement. After the t16 repair the episode id is exactly what the score must
    /// NOT be, which is why the returned ids are the assertion's comparison point.
    async fn attach_episodes(db: &Db, ids: &[i64]) -> std::collections::HashMap<i64, i64> {
        let ids = ids.to_vec();
        db.call(
            move |conn| -> Result<std::collections::HashMap<i64, i64>, rusqlite::Error> {
                let mut out = std::collections::HashMap::new();
                for (i, memory_id) in ids.iter().enumerate() {
                    conn.execute(
                        "INSERT INTO episodes (kind, content, content_hash, ref_time, ingested_at)
                     VALUES ('manual', 't13 episode', ?1, '2026-01-01T00:00:00Z',
                             '2026-01-01T00:00:00Z')",
                        rusqlite::params![format!("t13-episode-{i}")],
                    )?;
                    let episode_id = conn.last_insert_rowid();
                    conn.execute(
                        "UPDATE memories SET source_episode = ?1 WHERE id = ?2",
                        rusqlite::params![episode_id, memory_id],
                    )?;
                    out.insert(*memory_id, episode_id);
                }
                Ok(out)
            },
        )
        .await
        .expect("writer is alive")
        .expect("the episodes attach")
    }

    /// The TRUE bm25 per row, read with the column NAMED — the same reading the repaired
    /// `search_fts_scored` performs, kept INDEPENDENT of it (a second statement, not a call
    /// to the production function) so that a future change to either cannot move both sides
    /// at once. It is also what the t13 tripwire used to show the defect's number was not
    /// this one.
    async fn real_bm25_by_memory(db: &Db, query: &str) -> std::collections::HashMap<i64, f64> {
        let query = keyword_pattern(query);
        db.call(
            move |conn| -> Result<std::collections::HashMap<i64, f64>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT m.id, bm25(memories_fts) FROM memories_fts f
                     JOIN memories m ON m.id = f.rowid
                     WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL AND m.deleted_at IS NULL
                     ORDER BY rank LIMIT 5",
                )?;
                let rows = stmt.query_map(rusqlite::params![query], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
                })?;
                rows.collect()
            },
        )
        .await
        .expect("writer is alive")
        .expect("the index-11 reading runs")
    }

    /// The default entry point and the explicit default configuration are the
    /// SAME call: `recall_memories` is a delegation, not a second pipeline.
    #[tokio::test]
    async fn the_default_entry_point_is_the_default_configuration() {
        let (db, embedder, _ids) = fixture().await;
        let today = recall_memories(&db, embedder.clone(), "alpha beta", 5, 0.25).await;
        let configured =
            recall_memories_with(&db, embedder, "alpha beta", 5, 0.25, &MemoryLegs::default())
                .await;
        assert_eq!(
            today, configured,
            "the default configuration must reproduce the pre-t5 entry point exactly"
        );
        assert_eq!(
            MemoryLegs::default().disabled_legs(),
            Vec::<RecallLeg>::new()
        );
    }

    /// ACCEPTANCE: a DISABLED leg is never queried, and (since t16) the switch is also
    /// VISIBLE in the response.
    ///
    /// The proof of "never queried" is a counting `Embedder`, and it is the strongest
    /// available here: the semantic leg owns the query EMBEDDING (a model forward pass on
    /// the fastembed path) and the brute-force cosine scan, and with the leg off the
    /// counter does not move while the same call still returns an answer. The leg-ON
    /// control shows the counter DOES move, so the probe cannot pass by observing nothing.
    ///
    /// The keyword leg COULD NOT be observed at all when t5 wrote this: `search_fts_scored`
    /// failed on the fixture's NULL `source_episode` (the [`memory_fts_leg_error`] branch of
    /// the index defect), so the leg returned nothing whether or not it was switched off,
    /// and t5 recorded that unobservability rather than pretending to test it. t16 repaired
    /// the read, so the leg's on/off state IS observable now, and this test asserts the new
    /// observable instead of the old unobservability.
    #[tokio::test]
    async fn a_disabled_memory_semantic_leg_is_never_queried() {
        let (db, _setup_embedder, ids) = fixture().await;
        let (embedder, calls) = CountingEmbedder::new();
        let embedder: Arc<dyn Embedder> = Arc::new(embedder);

        // CONTROL, both legs on: exactly one embedding.
        let both = recall_memories_with(
            &db,
            embedder.clone(),
            "alpha beta",
            5,
            0.25,
            &MemoryLegs::default(),
        )
        .await;
        let on_calls = calls.load(std::sync::atomic::Ordering::SeqCst);
        println!(
            "READING t5 leg-off CONTROL: embed_calls={on_calls} semantic={} keyword={} hits={:?}",
            both.semantic,
            both.keyword,
            both.hits.iter().map(|h| h.id).collect::<Vec<_>>()
        );
        assert_eq!(
            on_calls, 1,
            "the semantic leg embeds the query exactly once"
        );
        assert_eq!(both.semantic, 4);

        // SEMANTIC OFF: no embedding at all — and the keyword leg, now that it reads real
        // bm25, still answers. That IS the observable t5 could not have: the switch changes
        // the response.
        let semantic_off = MemoryLegs {
            semantic: false,
            ..MemoryLegs::default()
        };
        let off =
            recall_memories_with(&db, embedder.clone(), "alpha beta", 5, 0.25, &semantic_off).await;
        let after_calls = calls.load(std::sync::atomic::Ordering::SeqCst);
        println!(
            "READING t5 semantic-off: embed_calls={} (was {on_calls}) semantic={} keyword={} \
             top_semantic_score={:?} hits={:?}",
            after_calls - on_calls,
            off.semantic,
            off.keyword,
            off.top_semantic_score,
            off.hits
                .iter()
                .map(|h| (h.id, h.score, h.legs.clone(), h.keyword_score))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            after_calls - on_calls,
            0,
            "a disabled semantic leg must not embed the query at all"
        );
        assert_eq!(off.semantic, 0);
        assert_eq!(off.top_semantic_score, None);
        assert_eq!(
            off.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
            vec![ids[0], ids[2]],
            "with the semantic leg off, the keyword leg's own bm25 order is the answer"
        );
        assert_eq!(off.keyword, 2);
        assert_eq!(off.keyword_new, 2, "neither row came from the semantic leg");
        assert!(
            off.hits
                .iter()
                .all(|h| !h.legs.contains(&"semantic") && h.semantic_score.is_none()),
            "no row may carry semantic evidence when that leg was never queried: {:?}",
            off.hits
        );
        assert!(
            off.hits
                .iter()
                .all(|h| h.keyword_score.is_some_and(|s| s < 0.0)),
            "each row carries the keyword leg's own (now correct) bm25: {:?}",
            off.hits
        );
        assert_ne!(
            both.hits, off.hits,
            "THE OBSERVABILITY PIN (t16): before the repair the keyword leg contributed nothing, so \
             this switch changed nothing at all; now the two configurations must differ"
        );
    }

    /// THE `source_episode` INDEX REPAIR, POSITIVE FORM (t16).
    ///
    /// THE DEFECT AND ITS FIX, kept named so the history stays readable:
    /// `crates/memory/src/query.rs` read the FTS5 relevance score with
    /// `r.get::<_, f64>(10)`. Index 10 of that SELECT is `source_episode` and
    /// `bm25(memories_fts)` is index 11, so the leg reported a row's EPISODE ID as
    /// its relevance score whenever `source_episode` was non-NULL — the shape
    /// platform-written rows have (t9 measured 3 rows, 0 nulls) — and failed with
    /// `Invalid column type Null at index: 10` when it was NULL, which
    /// `recall_memories_with` swallowed into an empty leg. That NULL shape is what
    /// `fixture()` writes, and it is why this effort first believed the leg was dead.
    /// The repair aliases the column (`bm25(memories_fts) AS bm25`) and reads it BY NAME.
    ///
    /// t13's `characterizes_defect_both_branches_of_the_source_episode_index_bug` was the
    /// TRIPWIRE for that defect: it asserted the BROKEN behaviour on both branches, so
    /// this repair turned it red by design. This test is its positive replacement and
    /// pins both branches in the corrected state; the tripwire's readings are recorded in
    /// the design document's §21.1 (CLOSED by t16).
    ///
    /// THE RANKING QUESTION t13 LEFT OPEN, answered by the readings below: the wrong
    /// score never reached the ORDER. Both branches produce the leg's bm25 order (the
    /// SELECT says `ORDER BY rank`) and the memory fusion consumes ID LISTS, so:
    /// * on the NON-NULL branch (platform rows) the fused page is UNCHANGED — only the
    ///   reported `keyword_score` was wrong;
    /// * on the NULL branch the leg contributed NOTHING before, so the page DID move once
    ///   it started contributing. That is the intended delta, and the golden above has
    ///   been re-derived for it.
    #[tokio::test]
    async fn the_source_episode_index_repair_reports_bm25_for_both_shapes() {
        let (db, embedder, ids) = fixture().await;

        // ── Branch 1: the fixture's NULL `source_episode`. BEFORE: the whole leg errored. ──
        let null_legs = recall_memories(&db, embedder.clone(), "alpha beta", 5, 0.25).await;
        let null_error = memory_fts_leg_error(&db, "alpha beta").await;
        let null_kw =
            ruagent_memory::query::search_fts_scored(&db, &keyword_pattern("alpha beta"), 5)
                .await
                .expect("the NULL branch must read after the repair");
        let null_truth = real_bm25_by_memory(&db, "alpha beta").await;
        println!(
            "READING t16 branch NULL: leg={:?} fused={:?} counters=(semantic={} keyword={} \
             keyword_new={} top_semantic={:?}) leg_was_error={:?}",
            null_kw.iter().map(|(r, s)| (r.id, *s)).collect::<Vec<_>>(),
            null_legs
                .hits
                .iter()
                .map(|h| (h.id, h.score, h.legs.clone(), h.keyword_score))
                .collect::<Vec<_>>(),
            null_legs.semantic,
            null_legs.keyword,
            null_legs.keyword_new,
            null_legs.top_semantic_score,
            null_error
        );
        assert!(
            null_error.is_none(),
            "the NULL branch no longer errors — that error WAS the defect: {null_error:?}"
        );
        assert!(
            !null_kw.is_empty(),
            "the keyword leg contributes rows on this shape now"
        );
        for (row, score) in &null_kw {
            assert_eq!(
                score,
                null_truth.get(&row.id).expect("the index-11 reading"),
                "row {} must carry SQLite's bm25",
                row.id
            );
            assert!(*score < 0.0, "bm25 is negative for a match");
        }
        assert_eq!(null_legs.keyword as usize, null_kw.len());
        assert!(
            null_legs.keyword > 0,
            "BEFORE this repair the leg contributed nothing on this fixture (keyword == 0); it does \
             now, and that is the deliberate, documented delta"
        );

        // ── Branch 2: NON-NULL `source_episode` — what live rows look like. BEFORE: these same
        // ids came back with the EPISODE ID as the score (t13 measured `rows=[(1, 1.0, Some(1)),
        // (3, 3.0, Some(3))]`). ──
        let episodes = attach_episodes(&db, &ids).await;
        let non_null_legs = recall_memories(&db, embedder, "alpha beta", 5, 0.25).await;
        let non_null_kw =
            ruagent_memory::query::search_fts_scored(&db, &keyword_pattern("alpha beta"), 5)
                .await
                .expect("the NON-NULL branch must read");
        let non_null_truth = real_bm25_by_memory(&db, "alpha beta").await;
        println!(
            "READING t16 branch NON-NULL: leg={:?} episodes={:?} fused={:?} keyword={}",
            non_null_kw
                .iter()
                .map(|(r, s)| (r.id, *s))
                .collect::<Vec<_>>(),
            non_null_kw
                .iter()
                .map(|(r, _)| (r.id, episodes.get(&r.id).copied()))
                .collect::<Vec<_>>(),
            non_null_legs
                .hits
                .iter()
                .map(|h| (h.id, h.score, h.legs.clone(), h.keyword_score))
                .collect::<Vec<_>>(),
            non_null_legs.keyword
        );
        for (row, score) in &non_null_kw {
            let episode_id = episodes
                .get(&row.id)
                .copied()
                .expect("every matched row got an episode");
            assert_eq!(
                row.source_episode,
                Some(episode_id),
                "this really is the non-NULL branch"
            );
            assert_eq!(
                score,
                non_null_truth.get(&row.id).expect("the index-11 reading"),
                "row {} must carry SQLite's bm25",
                row.id
            );
            assert_ne!(
                *score, episode_id as f64,
                "the score must NOT be the row's episode id — that WAS the defect"
            );
            assert!(*score < 0.0, "bm25 is negative for a match");
        }

        // ── THE ANSWER: the leg's ID order is identical on both branches, and the fusion
        // consumes that order — so the page does not move for the shape that already worked. ──
        assert_eq!(
            non_null_legs.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
            null_legs.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
            "the leg's ID order never depended on the score"
        );
        assert_eq!(
            non_null_legs
                .hits
                .iter()
                .map(|h| h.score)
                .collect::<Vec<_>>(),
            null_legs.hits.iter().map(|h| h.score).collect::<Vec<_>>(),
            "RRF consumes ranks, not raw bm25 values"
        );
        assert_eq!(
            non_null_legs
                .hits
                .iter()
                .map(|h| h.keyword_score)
                .collect::<Vec<_>>(),
            null_legs
                .hits
                .iter()
                .map(|h| h.keyword_score)
                .collect::<Vec<_>>(),
            "and the reported keyword_score is the same on both branches"
        );
    }

    /// Every leg off: an empty result with NO query issued — never an error, and
    /// never a silently-empty success (the configuration names the legs).
    #[tokio::test]
    async fn all_memory_legs_off_queries_nothing_and_says_why() {
        let (db, _setup, _ids) = fixture().await;
        let (embedder, calls) = CountingEmbedder::new();
        let embedder: Arc<dyn Embedder> = Arc::new(embedder);

        let empty =
            recall_memories_with(&db, embedder, "alpha beta", 5, 0.25, &MemoryLegs::all_off())
                .await;
        println!("READING t5 all-off: {empty:?}");
        assert_eq!(empty, RecallLegs::default(), "all-off evidence is all-zero");
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "no embedding when both legs are off"
        );

        // ... and the CONFIGURATION is what makes that empty result readable.
        let cfg = RecallLegConfig {
            memory: MemoryLegs::all_off(),
            ..RecallLegConfig::default()
        };
        assert!(!cfg.all_disabled(), "the wiki and graph legs are still on");
        assert_eq!(
            cfg.disabled_legs(),
            vec![RecallLeg::MemoryFts, RecallLeg::MemorySemantic],
            "the disabled memory legs are named (typed), in report order"
        );
        assert_eq!(cfg.disabled_reason(), None);

        let nothing = RecallLegConfig {
            memory: MemoryLegs::all_off(),
            knowledge: LegConfig::all_off(),
            wiki: false,
            graph: false,
        };
        assert!(nothing.all_disabled());
        let reason = nothing
            .disabled_reason()
            .expect("an all-off config states why");
        println!("READING t5 all-off reason: {reason}");
        for label in [
            RecallLeg::Graph.label(),
            RecallLeg::KnowledgeFts.label(),
            RecallLeg::KnowledgeSemantic.label(),
            RecallLeg::MemoryFts.label(),
            RecallLeg::MemorySemantic.label(),
            RecallLeg::Wiki.label(),
        ] {
            assert!(
                reason.contains(label),
                "the reason must name `{label}`: {reason}"
            );
        }
        assert!(
            reason.contains("disabled by configuration"),
            "the reason must not read as a retrieval miss: {reason}"
        );
    }

    /// The disabled-leg list is COMPLETE and STABLE over the six legs, and it is a
    /// list of TYPES: the registry's capability ids are not respelled here.
    ///
    /// WHY THAT MATTERS (t5 convergence): `crate::capability::CapabilityId::as_str()`
    /// is the single source of the ids, and the endpoint maps these variants onto it
    /// when it builds the response's `legs_disabled`. A second hardcoded copy of the
    /// strings in this file would be a shadow list that no test here could keep
    /// honest — so what is asserted is the SHAPE (six legs, stable order, typed),
    /// and the id mapping is checked where the ids live.
    #[test]
    fn the_six_recall_legs_are_typed_complete_and_stably_ordered() {
        let all_off = RecallLegConfig {
            memory: MemoryLegs::all_off(),
            knowledge: LegConfig::all_off(),
            wiki: false,
            graph: false,
        };
        let legs = all_off.disabled_legs();
        println!("READING t5 recall legs: {legs:?}");
        assert_eq!(
            legs,
            vec![
                RecallLeg::Graph,
                RecallLeg::KnowledgeFts,
                RecallLeg::KnowledgeSemantic,
                RecallLeg::MemoryFts,
                RecallLeg::MemorySemantic,
                RecallLeg::Wiki,
            ],
            "the six recall legs, one entry each, in declaration order"
        );
        assert_eq!(
            RecallLegConfig::default().disabled_legs(),
            Vec::<RecallLeg>::new()
        );
        // The knowledge crate reports its own two legs as TYPES, and the daemon
        // maps them into this enum (the mapping under test on the line above).
        let kb_off = LegConfig::all_off();
        assert_eq!(
            kb_off.disabled_legs(),
            vec![
                ruagent_knowledge::store::KnowledgeLeg::Semantic,
                ruagent_knowledge::store::KnowledgeLeg::Keyword,
            ]
        );
    }

    /// Weights feed the memory fusion as configured: the same leg fixture reorders
    /// under a non-default weight, and the DEFAULT is the frozen unweighted `rrf`
    /// bit for bit.
    ///
    /// The fixture is expressed as RANKS (sem `[1,2,3,4]`, kw `[1,3]` — the shape
    /// the DB fixture has), so this is pure fusion arithmetic and does not depend on
    /// the keyword leg being alive in the database.
    #[test]
    fn a_non_default_memory_weight_reorders_the_fusion() {
        let sem = vec![1i64, 2, 3, 4];
        let kw = vec![1i64, 3];
        let default = fuse_memory_legs(&sem, &kw, &MemoryLegs::default());
        println!("READING t5 memory weights: default={default:?}");
        assert_eq!(
            default,
            ruagent_knowledge::rrf(&[sem.clone(), kw.clone()], RRF_K),
            "1:1 must be the frozen `rrf`, not merely similar"
        );
        assert_eq!(
            default.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![1, 3, 2, 4],
            "the both-legs row 1 leads, then 3 (kw rank 1) over 2 (sem rank 1)"
        );

        // A tiny keyword weight (legal: > 0) drops the keyword-only row 3 below the
        // row only the semantic leg finds (row 2) — the flip 3 > 2 requires
        // w_kw >= w_sem/63. (Both rows reach this test as ids handed to the fusion,
        // so the ranking effect of the keyword leg's NUMBERS is not what is measured
        // here — see the source_episode defect's open question.)
        let semantic_leaning = MemoryLegs {
            w_semantic: 1.0,
            w_keyword: 0.01,
            ..MemoryLegs::default()
        };
        let reordered = fuse_memory_legs(&sem, &kw, &semantic_leaning);
        println!("READING t5 memory weights: w_kw=0.01 -> {reordered:?}");
        assert_eq!(
            reordered.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4],
            "a non-default weight must move the memory fusion, not just be stored"
        );

        // The keyword-heavy direction keeps 1 on top and lifts its score, so the
        // weight is visibly in the number and not only in the order.
        let keyword_heavy = MemoryLegs {
            w_semantic: 1.0,
            w_keyword: 10.0,
            ..MemoryLegs::default()
        };
        let heavy = fuse_memory_legs(&sem, &kw, &keyword_heavy);
        println!("READING t5 memory weights: w_kw=10 -> {heavy:?}");
        assert_eq!(
            heavy.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![1, 3, 2, 4]
        );
        assert!(heavy[0].1 > default[0].1);

        // A DISABLED leg contributes nothing even when a caller hands in its id
        // list, so "off" has no way back into the ranking.
        let semantic_off = MemoryLegs {
            semantic: false,
            ..MemoryLegs::default()
        };
        assert_eq!(
            fuse_memory_legs(&sem, &kw, &semantic_off)
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert!(fuse_memory_legs(&sem, &kw, &MemoryLegs::all_off()).is_empty());
    }

    /// t2 / design §20.2 (memory half): the label a record carries states the
    /// EFFECTIVE weights — the numbers the fusion above actually consumed — so a
    /// weight change is readable even when the corpus alone cannot show it.
    ///
    /// The string is also the FROZEN wire spelling, so it cannot drift back to the
    /// knowledge type's `rrf(k=...,w_sem=...,w_kw=...)` form: that form is a
    /// different string for the same information, and one payload must not carry
    /// both — see [`MemoryLegs::fusion_label`].
    ///
    /// The last assertion is the one that makes the label evidence rather than
    /// decoration: the ranking those weights produce, computed by hand through the
    /// frozen `rrf_weighted`, is the ranking `fuse_memory_legs` produces from the
    /// same configuration the label was rendered from.
    #[test]
    fn the_memory_fusion_label_carries_the_effective_weights() {
        assert_eq!(
            MemoryLegs::default().fusion_label(),
            "rrf:k=60,w_semantic=1,w_keyword=1",
            "the default must read as the 1:1 fusion that reproduces the frozen `rrf`"
        );
        let weighted = MemoryLegs {
            w_semantic: 3.0,
            w_keyword: 1.0,
            ..MemoryLegs::default()
        };
        assert_eq!(weighted.fusion_label(), "rrf:k=60,w_semantic=3,w_keyword=1");
        println!(
            "READING memory fusion label: default={} w_sem=3={} all_off={}",
            MemoryLegs::default().fusion_label(),
            weighted.fusion_label(),
            MemoryLegs::all_off().fusion_label()
        );
        // A DISABLED leg's weight does not appear: a stale 3.0 beside
        // `enabled = false` normalizes to 0.0 (`normalized_weight`), exactly as t5
        // established for the knowledge side.
        let stale = MemoryLegs {
            semantic: false,
            w_semantic: 3.0,
            ..MemoryLegs::default()
        };
        assert_eq!(stale.fusion_label(), "rrf:k=60,w_semantic=0,w_keyword=1");
        assert_eq!(
            MemoryLegs::all_off().fusion_label(),
            "rrf:k=60,w_semantic=0,w_keyword=0"
        );
        // The string pinned above is the response's OWN spelling, byte for byte —
        // the same shape `api.rs` builds for the knowledge label, so a reader parses
        // both with one parser (asserted over HTTP by `parse_rrf_label`). The
        // knowledge TYPE's `label()` renders a DIFFERENT string for the same
        // information, and no payload carries it.
        assert_ne!(
            weighted.fusion_label(),
            "rrf(k=60,w_sem=3,w_kw=1)",
            "the knowledge TYPE's label() spelling is not what a payload carries"
        );

        // The label and the fusion read the SAME weights.
        let sem = vec![1i64, 2, 3, 4];
        let kw = vec![1i64, 3];
        assert_eq!(
            fuse_memory_legs(&sem, &kw, &weighted),
            ruagent_knowledge::rrf_weighted(&[(&sem, 3.0), (&kw, 1.0)], RRF_K),
            "the weights the label reports are the weights the fusion used"
        );
    }

    /// The weight rule, at the layer that rejects it: negative and all-zero are
    /// refused for ENABLED legs, and a disabled leg's weight is ignored.
    #[test]
    fn the_configured_weights_are_validated() {
        assert_eq!(
            MemoryLegs::resolve((true, Some(0.0)), (true, Some(0.0))),
            Err(WeightError::NonPositive {
                leg: RecallLeg::MemorySemantic.label(),
                weight: 0.0
            }),
            "an all-zero enabled pair is a configuration error, not an empty ranking"
        );
        assert!(matches!(
            MemoryLegs::resolve((true, Some(-1.0)), (true, None)),
            Err(WeightError::NonPositive { .. })
        ));
        assert!(matches!(
            MemoryLegs::resolve((true, Some(f64::NAN)), (true, None)),
            Err(WeightError::NonFinite { .. })
        ));
        // A weight a disabled leg carries is ignored, and normalized to nothing.
        let resolved = MemoryLegs::resolve((false, Some(-3.0)), (true, None)).expect("legal");
        assert_eq!(resolved.w_semantic, 0.0);
        assert_eq!(resolved.w_keyword, 1.0);
        // `None` takes the leg's own default — never zero.
        assert_eq!(
            MemoryLegs::resolve((true, None), (true, None)).expect("legal"),
            MemoryLegs::default()
        );
        // The whole six-leg config resolves (and validates) in one call.
        assert!(
            RecallLegConfig::resolve(
                (true, None),
                (true, None),
                (true, None),
                (true, Some(0.5)),
                false,
                true
            )
            .is_ok()
        );
        assert_eq!(
            RecallLegConfig::resolve(
                (true, None),
                (true, None),
                (true, None),
                (true, None),
                false,
                true
            )
            .expect("legal")
            .disabled_legs(),
            vec![RecallLeg::Wiki]
        );
    }

    /// Both STRATEGIES still work over the toggleable legs: the caller's cosine
    /// floor (0.30 conservative / 0.25 aggressive, api.rs) keeps deciding which
    /// semantic rows are admissible, and with the semantic leg off it decides
    /// nothing at all because nothing is embedded.
    #[tokio::test]
    async fn both_strategies_work_over_the_toggleable_legs() {
        let (db, embedder, ids) = fixture().await;
        let aggressive = recall_memories(&db, embedder.clone(), "alpha beta", 5, 0.25).await;
        let conservative = recall_memories(&db, embedder.clone(), "alpha beta", 5, 0.30).await;
        println!(
            "READING t5 strategies: aggressive={:?} conservative={:?}",
            aggressive.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
            conservative.hits.iter().map(|h| h.id).collect::<Vec<_>>()
        );
        // The 0.289 row sits between the two floors: admissible for the aggressive
        // one, not for the conservative one. (Re-derived by t16: the keyword leg now
        // contributes rows 1 and 3, so both pages lead with the two rows BOTH legs found,
        // in fused order, before the semantic-only tail.)
        assert_eq!(
            aggressive.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
            vec![ids[0], ids[2], ids[1], ids[3]]
        );
        assert_eq!(
            conservative.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
            vec![ids[0], ids[2], ids[1]],
            "the conservative floor drops the row between the two floors"
        );
        assert_eq!(aggressive.semantic, 4);
        assert_eq!(conservative.semantic, 3);
        assert_eq!(aggressive.keyword, 2);
        assert_eq!(conservative.keyword, 2, "the floor is the semantic leg's");

        // With the semantic leg off the floor decides nothing (nothing is embedded
        // or measured), and both strategies agree — the leg configuration, not the
        // strategy, is what removed the rows. The keyword leg still answers, with the
        // same rows for both floors (t16: before the repair both were empty here, which
        // is why t5 could only record that the two agreed).
        let keyword_only = MemoryLegs {
            semantic: false,
            ..MemoryLegs::default()
        };
        let a =
            recall_memories_with(&db, embedder.clone(), "alpha beta", 5, 0.25, &keyword_only).await;
        let c = recall_memories_with(&db, embedder, "alpha beta", 5, 0.30, &keyword_only).await;
        assert_eq!(a.hits, c.hits, "a floor nothing is measured against");
        assert_eq!(a.semantic, 0);
        assert_eq!(
            a.hits.iter().map(|h| h.id).collect::<Vec<_>>(),
            vec![ids[0], ids[2]],
            "the keyword leg's rows are what remains"
        );
    }

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
