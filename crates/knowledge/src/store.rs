//! The knowledge store: ingestion into SQLite (chunks + FTS) and LanceDB
//! (vectors), and the hybrid search path (design §19, §6.6 #3).

use std::sync::Arc;

use futures::TryStreamExt;
use lancedb::arrow::arrow_array::types::Float32Type;
use lancedb::arrow::arrow_array::{FixedSizeListArray, Float32Array, Int64Array, RecordBatch};
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use lancedb::connection::Connection;
use lancedb::query::{ExecutableQuery, QueryBase};

use ruagent_store::Db;

use crate::chunk::chunk_sections;
use crate::embed::{EmbedError, Embedder, HashEmbedder};
use crate::rrf::rrf_weighted;

pub(crate) const TABLE: &str = "knowledge_chunks";

#[derive(Debug, thiserror::Error)]
pub enum KnowledgeError {
    #[error("sqlite: {0}")]
    Db(#[from] ruagent_store::DbError),
    #[error("rusqlite: {0}")]
    Rusqlite(#[from] rusqlite::Error),
    #[error("lancedb: {0}")]
    Lance(#[from] lancedb::Error),
    #[error("arrow: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),
    #[error("embedding: {0}")]
    Embed(#[from] EmbedError),
    #[error(
        "embedding model mismatch: table was built with `{stored}`, active embedder is `{active}` — re-ingest or switch embedders"
    )]
    ModelMismatch { stored: String, active: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

/// One search hit.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchHit {
    pub chunk_id: i64,
    pub document: String,
    pub content: String,
    pub score: f32,
}

// ---------------------------------------------------------------------------
// t7: the scale of a score, the fusion, and the paged entry point (R-A D.2)
// ---------------------------------------------------------------------------

/// Which quantity a score IS. A first-class citizen, not a comment (t7).
///
/// WHY THIS EXISTS, measured (R-A A7): one live recall response carried FOUR
/// scales under TWO field names — a fused rank score (`api.rs:3003`), a LanceDB
/// distance where lower is closer (`:3006`), bm25 where more negative is better
/// (`:3008`) and, on the memory side, a cosine where higher is better
/// (`:2581-2582`). `panel/src/views/Memory.tsx:367-384` rendered `m 0.86` next
/// to `k 0.016` with nothing able to say they are not the same kind of number.
/// A scale missing from the data cannot be recovered by a consumer; it can only
/// be guessed, and this row is what guessing produces.
///
/// The literals are the wire form. `"rrf_rank"` is FROZEN by R-B D.7: two
/// generations of consumers (panel, MCP, injection) read that exact string, so
/// it may never be renamed — new scales get new keys, never a rename.
///
/// EVERY VARIANT RENAMES EXPLICITLY, and the reason deserves a sentence because
/// the obvious fix is wrong: until t30 this enum derived `Serialize` with no
/// attribute, so serde emitted the RUST VARIANT NAME — `"RrfRank"` — while
/// `as_str()` and the daemon's hand-written JSON emitted `"rrf_rank"`. Two wire
/// paths, two vocabularies, no compile error (V-A F-5). `rename_all = "snake_case"`
/// would not have fixed it either: it derives `semantic_distance` and
/// `keyword_bm25`, which are NOT the frozen literals (`semantic_l2sq`, `bm25`).
/// Two of the five literals have no derivable spelling, so the wire form is
/// written next to each variant, and `tests/score-kind-wire.rs` asserts serde
/// output == `as_str()` for all five (the consumption side is pinned in t19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ScoreKind {
    /// `Σ 1/(k + rank)` over the legs that returned the document. Bounded above
    /// by `legs/(k+1)`: it says WHERE a hit was found, not how relevant it is.
    #[serde(rename = "rrf_rank")]
    RrfRank,
    /// A vector distance: LOWER is closer. Not a similarity.
    #[serde(rename = "semantic_l2sq")]
    SemanticDistance,
    /// SQLite FTS5 `bm25()`: MORE NEGATIVE is a better match.
    #[serde(rename = "bm25")]
    KeywordBm25,
    /// A cosine similarity: HIGHER is more similar.
    #[serde(rename = "cosine")]
    Cosine,
    /// A calibrated relevance in [0,1] — see [`RelevanceScore`].
    #[serde(rename = "calibrated")]
    Calibrated,
}

impl ScoreKind {
    /// The stable wire literal.
    pub fn as_str(self) -> &'static str {
        match self {
            ScoreKind::RrfRank => "rrf_rank",
            ScoreKind::SemanticDistance => "semantic_l2sq",
            ScoreKind::KeywordBm25 => "bm25",
            ScoreKind::Cosine => "cosine",
            ScoreKind::Calibrated => "calibrated",
        }
    }
}

/// One leg's evidence for one hit: where it ranked, what it scored, on what
/// scale. `kind` is what makes `raw_score` readable without knowing which leg
/// produced it.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct LegEvidence {
    /// 0-based position within this leg.
    pub rank: usize,
    pub raw_score: f32,
    pub kind: ScoreKind,
}

/// The fusion expression, as data rather than as an assumption.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum FusionKind {
    Rrf {
        k: u32,
        w_semantic: f32,
        w_keyword: f32,
    },
}

impl FusionKind {
    /// `(k, w_semantic, w_keyword)` — the single place the numbers are read.
    pub fn weights(self) -> (u32, f32, f32) {
        match self {
            FusionKind::Rrf {
                k,
                w_semantic,
                w_keyword,
            } => (k, w_semantic, w_keyword),
        }
    }

    /// The literal that goes into `recall_log.fusion`, e.g.
    /// `"rrf(k=60,w_sem=2,w_kw=1)"`. A row scored by a different expression is
    /// not comparable with this one, which is why the expression travels with
    /// the score.
    pub fn label(self) -> String {
        let (k, ws, wk) = self.weights();
        format!("rrf(k={k},w_sem={ws},w_kw={wk})")
    }
}

/// How many candidates EACH leg contributes to the fusion.
///
/// INDEPENDENT OF THE CALLER'S PAGE SIZE — that is its whole purpose (R-A C5).
/// MEASURED (A4, 2026-09-27T21:47:54): with `leg_k = limit.max(10)`, the fused
/// top-1 document changed for 2 of 13 live queries between `limit=10` and
/// `limit=20`, i.e. asking for a second page changed the first page. `limit=20`
/// and `limit=30` agreed on 13/13, so a window that does not move with the
/// caller fixes it; 60 leaves room above the largest page a caller may ask for.
pub const LEG_WINDOW: usize = 60;

/// The shipped fusion. The weights come from the live gold-set calibration
/// (R-A C1/C2/C9): `{2,3,4}:1` all scored recall@1 0.7333 / MRR 0.8167 /
/// nDCG@10 0.8421, and 2:1 is the LOWER edge of that plateau — the point
/// furthest from "semantic only" (0.6000), i.e. the one that still gives the
/// keyword leg real weight. It is not proven optimal: that needs a multi-gold,
/// graded set (R-A §E3, §G2), and the weight choice's cost is on the record
/// (recall@5 1.0000→0.9333, one content-derived gold falls from rank 2 to 11).
pub const FUSION: FusionKind = FusionKind::Rrf {
    k: 60,
    w_semantic: 2.0,
    w_keyword: 1.0,
};

/// Bump when the fusion OR the relevance calibration changes.
///
/// A row scored by another version is not comparable: `recall_log` carries this
/// next to `knowledge_leg_window` so a reader can tell "worse" from
/// "different". Version 2 = the weighted fusion + the fixed leg window (the
/// pre-t7 behaviour, `limit.max(10)` with 1:1 weights, is version 1 and is NOT
/// comparable with it — R-A D.3 B-2).
pub const SCORING_VERSION: u32 = 2;

/// Version of [`RelevanceScore::value`]'s calibration.
pub const RELEVANCE_VERSION: u32 = 1;

/// A relevance in [0,1], with the query background that produced it.
///
/// `value` is a DISPLAY/RANKING score, never a gate (R-A C3). It is deliberately
/// reported with `query_background`, because the transform is meaningless
/// without it and a value a reader cannot re-derive is not evidence.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct RelevanceScore {
    pub value: f32,
    pub kind: ScoreKind,
    /// Calibration version; a model or formula change must bump it.
    pub version: u32,
    /// This query's background distance — see [`relevance_from_distance`].
    pub query_background: f32,
}

/// Turn a semantic distance into a display relevance, against the query's own
/// background level.
///
/// WHY A PER-QUERY BACKGROUND: this model's distances have no absolute zero
/// (R-A B10: E5 needs distinct `query:` / `passage:` prefixes, so even a
/// document's own text re-encoded as a query does not sit at 0). "0.31" is
/// therefore unreadable without knowing what 0.31 means for THIS query. The
/// background is the mean distance over the query's own semantic leg.
///
/// `value = clamp(1 - distance/background, 0, 1)`.
///
/// WHAT IT IS NOT, and this is the load-bearing part:
/// * NOT comparable across queries — a query whose leg is uniformly far still
///   produces a value near 1 for its own best hit. That is a property of the
///   transform, not a defect, and it is why C3's separation target cannot be
///   met by this field.
/// * NOT a gate. R-A C3 measured that nothing we can compute separates
///   answerable from unanswerable queries on the 22-query sample (the semantic
///   distance gap was 0.0011 wide) and explicitly FORBIDS writing the observed
///   0.227 as a constant. Cross-query comparability, if a caller needs it, is
///   the raw distance (`RankedHit::semantic.raw_score`, kind `semantic_l2sq`).
///
/// `None` when the background is unusable (non-finite, or zero — a single-hit
/// leg has no background) or when the distance is non-finite, which is how this
/// reports "LanceDB stopped giving me a distance" instead of returning a
/// confident 1.0.
pub fn relevance_from_distance(distance: f32, background: f32) -> Option<RelevanceScore> {
    if !distance.is_finite() || !background.is_finite() || background <= 0.0 {
        return None;
    }
    Some(RelevanceScore {
        value: (1.0 - distance / background).clamp(0.0, 1.0),
        kind: ScoreKind::Calibrated,
        version: RELEVANCE_VERSION,
        query_background: background,
    })
}

/// A hit with its evidence: the fused score it is ranked by, the scale of that
/// score, and what each leg said about it.
///
/// `hit.score` keeps its exact old meaning (the RRF rank score) — R-A D.3 B-4
/// forbids changing it, because `api.rs:2984-3018` publishes it straight to
/// JSON and the panel and MCP read it. Real relevance arrives IN ADDITION, as
/// `relevance`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RankedHit {
    pub hit: SearchHit,
    /// Always [`ScoreKind::RrfRank`] — the scale of `hit.score`.
    pub score_kind: ScoreKind,
    /// `None` = this leg did not find the hit. NOT a zero: 0.0 is a legal
    /// distance and would read as a perfect match on a leg that missed.
    pub semantic: Option<LegEvidence>,
    pub keyword: Option<LegEvidence>,
    pub relevance: Option<RelevanceScore>,
}

/// A page of hits plus the evidence that produced the ranking.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchPage {
    pub hits: Vec<RankedHit>,
    pub evidence: SearchEvidence,
}

/// Which copy of the knowledge a residual hit came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ResidualOrigin {
    /// Only the SQLite index holds the needle (e.g. the file was edited).
    Db,
    /// Only the file on disk holds it (e.g. the row was deleted) — the case
    /// that makes a "we forgot it" claim checkable.
    File,
    /// Both copies still hold it.
    Both,
}

/// One residual hit: a place a needle still exists after it was supposed to be
/// forgotten. Reported, never deleted.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ResidualHit {
    /// `-1` when `origin == File`: the file has no `documents` row at all, and
    /// an `Option` would invite a `0` that reads like a real id.
    pub document_id: i64,
    pub name: String,
    /// `None` = only `documents.name` matched (whole-document granularity).
    pub chunk_id: Option<i64>,
    pub origin: ResidualOrigin,
    /// Set whenever `origin` includes `File`.
    pub path: Option<std::path::PathBuf>,
}

/// The result of a residual scan.
///
/// `truncated` is not decoration: a bounded result must never read as "there is
/// nothing else". It comes from fetching `limit + 1` rows — zero extra cost, no
/// `COUNT(*)`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ResidualPage {
    pub hits: Vec<ResidualHit>,
    pub truncated: bool,
    /// Only when `exact_total` was requested (that is the one that pays for a
    /// full count; a plain LIKE over 10765 chunks measured 6.80–10.47 ms, R-A
    /// A8).
    pub total: Option<usize>,
}

/// One leg's result for a query, with that leg's OWN score (t250).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LegHit {
    pub chunk_id: i64,
    /// 0-based position within this leg.
    pub rank: usize,
    /// semantic: LanceDB distance (lower is closer);
    /// keyword: SQLite FTS5 bm25() (more negative is better) for the
    /// Precision/Prefix stages, 0.0 for the Substring stage (see
    /// KeywordStage -- there is no bm25 for a row FTS never matched).
    pub raw_score: f32,
}

/// Which construction produced the keyword leg (t261, extended by t7).
///
/// The first two stages are real FTS5 matches and carry bm25. `Bigram` matches
/// the Han-bigram shadow index and also carries bm25 (a different index, so a
/// different scale of number, but the same `bm25()` function). `Substring` is a
/// LIKE scan: those rows are NOT FTS matches, so no bm25 exists for them --
/// saying so in the type is better than a fabricated number.
///
/// `[冻结]` per R-A D.1 with one registered change: the FIRST FOUR variants'
/// names and meanings are frozen, the SET may only grow (D.3 B-8). `Bigram` is
/// that growth. No exhaustive `match` on this enum exists anywhere in the tree
/// (checked: all 13 use sites compare or construct), and `api.rs:2967` renders
/// it with `format!("{stage:?}").to_lowercase()`, so the new variant reaches the
/// wire as `"bigram"` with zero consumer changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum KeywordStage {
    /// No term survived tokenisation, or no stage found anything.
    Empty,
    /// Every term as a literal phrase, implicitly ANDed.
    Precision,
    /// Every term as a prefix, ORed. Runs only when Precision was empty.
    Prefix,
    /// The Han-bigram shadow index (`chunks_fts_cjk`). Runs only when both FTS
    /// stages were empty; the indexed path for a substring of a Han run (t7).
    Bigram,
    /// LIKE substring scan. Runs only when every stage above was empty; the
    /// last resort, because it is a full-table scan.
    Substring,
}

/// Both legs plus the fused ranking, with the fusion's own parameters (t250,
/// extended by t7).
///
/// WHY THIS EXISTS: the store computed two legs all along and then dropped
/// their raw scores inside rrf(), which keeps only ranks. So the only score the
/// platform could show was the fused rank score, whose upper bound is legs/61
/// -- measured at 3 distinct values across 574 live rows and IDENTICAL for a
/// 1-word and a 7-word query (t247). A caller that can see the legs can say
/// where a hit came from and how well it scored -- and, for `leg_window`, with
/// which window it was scored, which is what makes two readings comparable or
/// not (C5).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchEvidence {
    pub semantic: Vec<LegHit>,
    pub keyword: Vec<LegHit>,
    /// Which keyword construction ran (t261). The raw_score of a keyword hit
    /// is bm25 for Precision/Prefix/Bigram and 0.0 for Substring (no bm25
    /// exists there, so a number would be invented).
    pub keyword_stage: KeywordStage,
    pub fused: Vec<(i64, f32)>,
    /// The fusion expression that produced `fused` (t7).
    pub fusion: FusionKind,
    /// The window each leg was asked for; always [`LEG_WINDOW`] (t7).
    pub leg_window: usize,
    /// How many DISTINCT candidates entered the fusion (t7).
    pub candidates: usize,
}

/// The pre-t7 name, kept so no consumer has to change to gain the new fields
/// (R-A D.3 B-1). `.semantic` / `.keyword` / `.keyword_stage` / `.fused` all
/// read exactly as before.
pub type SearchLegs = SearchEvidence;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct KnowledgeDocument {
    pub id: i64,
    pub name: String,
    pub source: String,
    pub chunk_count: i64,
    pub created_at: String,
}

/// The knowledge base handle. Cheap to clone.
#[derive(Clone)]
pub struct Knowledge {
    pub(crate) db: Db,
    pub(crate) lance: Connection,
    pub(crate) embedder: Arc<dyn Embedder>,
    /// `<root>/knowledge` — the markdown documents (source of truth).
    pub(crate) docs_dir: std::path::PathBuf,
    /// Serializes index mutations (scan / save / edit / rebuild): they
    /// are multi-step read-modify-write sequences over SQLite +
    /// LanceDB, and a scan racing a save must not interleave.
    pub(crate) index_lock: Arc<tokio::sync::Mutex<()>>,
}

impl Knowledge {
    /// Open with the offline hash embedder (always works; the daemon
    /// upgrades to fastembed when its model is available).
    pub async fn open(root: &std::path::Path, db: Db) -> Result<Self, KnowledgeError> {
        Self::with_embedder(root, db, Arc::new(HashEmbedder::default())).await
    }

    /// Open with a specific embedder.
    pub async fn with_embedder(
        root: &std::path::Path,
        db: Db,
        embedder: Arc<dyn Embedder>,
    ) -> Result<Self, KnowledgeError> {
        let dir = root.join("data").join("lancedb");
        std::fs::create_dir_all(&dir)?;
        let lance = lancedb::connect(dir.to_string_lossy().as_ref())
            .execute()
            .await?;
        let docs_dir = root.join("knowledge");
        std::fs::create_dir_all(&docs_dir)?;
        let this = Self {
            db,
            lance,
            embedder,
            docs_dir,
            index_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        this.ensure_table().await?;
        this.check_model_meta().await?;
        // t7 / R-A E4 ③: populate `chunks.grams` for the Han-bigram index.
        //
        // WHY HERE: migration 0019 adds the column and `chunks_fts_cjk`, but no
        // SQL statement can compute a bigram decomposition -- so SOMETHING has
        // to fill it, and a schema whose column is empty indexes nothing. This
        // is the one place the single writer already holds the database at open,
        // before any scan or API can race it. The call is idempotent and
        // resumable, so an empty database costs one indexed-by-nothing SELECT
        // and a live one costs one batched pass; the ROW COUNT is logged rather
        // than assumed, because "0 rows needed filling" and "the pass never ran"
        // are different facts and only one of them is good news.
        let filled = this.db.backfill_chunk_grams().await?;
        if filled > 0 {
            tracing::info!(
                rows = filled,
                "knowledge: filled chunks.grams for the Han-bigram index"
            );
        }
        Ok(this)
    }

    /// The active embedder's identity (for diagnostics).
    /// The shared embedder — memory embedding reuses the same model so
    /// queries and rows live in one vector space.
    pub fn embedder(&self) -> std::sync::Arc<dyn Embedder> {
        self.embedder.clone()
    }

    pub fn embedder_name(&self) -> &'static str {
        self.embedder.name()
    }

    async fn ensure_table(&self) -> Result<(), KnowledgeError> {
        let names = self.lance.table_names().execute().await?;
        if names.iter().any(|n| n == TABLE) {
            return Ok(());
        }
        let dim = self.embedder.dim() as i32;
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), dim),
                true,
            ),
        ]));
        self.lance
            .create_empty_table(TABLE, schema)
            .execute()
            .await?;
        Ok(())
    }

    /// Record + verify the embedder identity backing the vectors
    /// (design §6.6 trap #6: never mix models silently).
    async fn check_model_meta(&self) -> Result<(), KnowledgeError> {
        let active = self.embedder.name().to_string();
        let stored: Option<String> = self
            .db
            .call(move |conn| -> Result<Option<String>, rusqlite::Error> {
                conn.query_row(
                    "SELECT value FROM knowledge_meta WHERE key = 'embedder'",
                    [],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })
            })
            .await?
            .map_err(ruagent_store::DbError::from)?;
        match stored {
            None => {
                let a = active.clone();
                self.db
                    .call(move |conn| -> Result<(), rusqlite::Error> {
                        conn.execute(
                            "INSERT OR REPLACE INTO knowledge_meta (key, value) VALUES ('embedder', ?1)",
                            [&a],
                        )?;
                        Ok(())
                    })
                    .await??;
                Ok(())
            }
            Some(stored) if stored == active => Ok(()),
            Some(stored) if self.embedder.is_fallback() => {
                // Offline fallback boot against a table built with a
                // real model: never migrate TO the fallback (that would
                // destroy real vectors on a temporary outage). Searches
                // are degraded but the platform boots. Previously this
                // errored — and could prevent boot entirely.
                tracing::warn!(
                    stored,
                    active,
                    "embedder mismatch on fallback boot — semantic search degraded until the model is available"
                );
                Ok(())
            }
            Some(stored) => self.migrate_embedder(&stored, &active).await,
        }
    }

    /// One-time model switch: adopt the active model, rebuild the
    /// vector table from the chunk texts (the sqlite rows are the
    /// truth; the old vectors are noise in the new space). Runs inside
    /// `with_embedder`, i.e. at open — before any scanner or API can
    /// race it.
    async fn migrate_embedder(&self, stored: &str, active: &str) -> Result<(), KnowledgeError> {
        tracing::warn!(
            stored,
            active,
            "embedder model changed — re-embedding the knowledge base (one-time migration)"
        );
        let a = active.to_string();
        self.db
            .call(move |conn| -> Result<(), rusqlite::Error> {
                conn.execute(
                    "INSERT OR REPLACE INTO knowledge_meta (key, value) VALUES ('embedder', ?1)",
                    [&a],
                )?;
                Ok(())
            })
            .await?
            .map_err(ruagent_store::DbError::from)?;
        // Drop and recreate the vector table: old vectors are noise and
        // the dimensions may differ entirely.
        if let Err(e) = self.lance.drop_table(TABLE, &[]).await {
            // a missing table is fine (nothing built yet)
            tracing::debug!(error = %e, "dropping vector table");
        }
        self.ensure_table().await?;
        // Re-embed every chunk from its text, in batches.
        let rows: Vec<(i64, String)> = self
            .db
            .call(|conn| -> Result<Vec<(i64, String)>, rusqlite::Error> {
                let mut stmt = conn.prepare("SELECT id, content FROM chunks ORDER BY id")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await?
            .map_err(ruagent_store::DbError::from)?;
        for batch in rows.chunks(32) {
            let texts: Vec<&str> = batch.iter().map(|(_, c)| c.as_str()).collect();
            let vectors = self.embedder.embed(&texts)?;
            let ids: Vec<i64> = batch.iter().map(|(id, _)| *id).collect();
            self.add_vectors(&ids, &vectors).await?;
        }
        tracing::info!(
            chunks = rows.len(),
            active,
            "knowledge base re-embedded under the new model"
        );
        Ok(())
    }

    /// Ingest a document into the index WITHOUT writing a file (the
    /// legacy path; tests and pre-file rows). Idempotent per
    /// (name, content-hash); a changed document replaces the old rows.
    pub async fn ingest(&self, name: &str, content: &str) -> Result<u32, KnowledgeError> {
        match self.index_doc(name, content, None).await? {
            crate::files::IndexOutcome::Indexed(n) => Ok(n),
            _ => Ok(0),
        }
    }

    /// Index a document: chunk (with sections + spans) → embed →
    /// SQLite + LanceDB. Upsert by name; same name + hash is a no-op.
    /// `source` = the backing file name for markdown-file documents.
    pub(crate) async fn index_doc(
        &self,
        name: &str,
        content: &str,
        source: Option<&str>,
    ) -> Result<crate::files::IndexOutcome, KnowledgeError> {
        use crate::files::IndexOutcome;

        // Serialize index mutations (see field docs).
        let _guard = self.index_lock.lock().await;

        let hash = crate::sha256_hex(content.as_bytes());
        let name = name.to_string();
        let source = source.map(str::to_string);

        // Upsert by name: same name + hash already indexed → no-op.
        // Newest row wins the hash comparison; older duplicates (e.g. a
        // pre-upgrade row) are replaced.
        let existing: Vec<(i64, String)> = self
            .db
            .call({
                let name = name.clone();
                move |conn| -> Result<Vec<(i64, String)>, rusqlite::Error> {
                    let mut stmt =
                        conn.prepare("SELECT id, content_hash FROM documents WHERE name = ?1")?;
                    let rows = stmt
                        .query_map(rusqlite::params![name], |r| Ok((r.get(0)?, r.get(1)?)))?
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(rows)
                }
            })
            .await?
            .map_err(ruagent_store::DbError::from)?;
        if let Some((_, latest_hash)) = existing.last()
            && *latest_hash == hash
        {
            return Ok(IndexOutcome::Unchanged);
        }
        for (id, _) in &existing {
            self.delete_document(*id).await?;
        }

        let sections = chunk_sections(content);
        let texts: Vec<String> = sections
            .iter()
            .flat_map(|s| s.chunks.iter().map(|c| c.content.clone()))
            .collect();
        if texts.is_empty() {
            return Ok(IndexOutcome::Empty);
        }

        // One writer-thread pass: document → sections → chunks.
        let chunk_count = texts.len();
        let name_for_insert = name.clone();
        let source_for_insert = source.clone();
        let ids: Vec<i64> = self
            .db
            .call(move |conn| -> Result<Vec<i64>, rusqlite::Error> {
                conn.execute(
                    "INSERT INTO documents (name, source, content_hash, chunk_count, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        name_for_insert,
                        source_for_insert,
                        hash,
                        chunk_count as i64,
                        chrono::Utc::now().to_rfc3339()
                    ],
                )?;
                let doc_id = conn.last_insert_rowid();
                let mut ids = Vec::with_capacity(chunk_count);
                for (s_idx, section) in sections.iter().enumerate() {
                    conn.execute(
                        "INSERT INTO chunk_sections (document_id, idx, content)
                         VALUES (?1, ?2, ?3)",
                        rusqlite::params![doc_id, s_idx as i64, section.content],
                    )?;
                    let section_id = conn.last_insert_rowid();
                    for chunk in &section.chunks {
                        conn.execute(
                            "INSERT INTO chunks
                                (document_id, idx, content, span_start, span_end, section_id, grams)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                            rusqlite::params![
                                doc_id,
                                ids.len() as i64,
                                chunk.content,
                                chunk.start as i64,
                                chunk.end as i64,
                                section_id,
                                ruagent_store::fts::han_bigrams(&chunk.content)
                            ],
                        )?;
                        ids.push(conn.last_insert_rowid());
                    }
                }
                Ok(ids)
            })
            .await?
            .map_err(ruagent_store::DbError::from)?;

        // Vectors into LanceDB.
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let vectors = self.embedder.embed(&refs)?;
        self.add_vectors(&ids, &vectors).await?;
        Ok(IndexOutcome::Indexed(ids.len() as u32))
    }

    pub(crate) async fn add_vectors(
        &self,
        ids: &[i64],
        vectors: &[Vec<f32>],
    ) -> Result<(), KnowledgeError> {
        let dim = self.embedder.dim() as i32;
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), dim),
                true,
            ),
        ]));
        let vector_col = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vectors
                .iter()
                .map(|v| Some(v.iter().map(|x| Some(*x)).collect::<Vec<_>>())),
            dim,
        );
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(ids.to_vec())),
                Arc::new(vector_col),
            ],
        )
        .map_err(|e| KnowledgeError::Other(e.to_string()))?;
        let table = self.lance.open_table(TABLE).execute().await?;
        table.add(batch).execute().await?;
        Ok(())
    }

    /// Hybrid search: LanceDB ANN + SQLite FTS, fused with RRF.
    /// Zero LLM at query time (design §6.6 #3).
    /// The two retrieval legs, each with its own raw score. Single source: both
    /// search() and search_legs() call this, so the fused ranking cannot drift
    /// away from the legs a caller inspects.
    async fn compute_legs(
        &self,
        query: &str,
        leg_k: usize,
    ) -> Result<(Vec<(i64, f32)>, Vec<(i64, f32)>, KeywordStage), KnowledgeError> {
        // Leg 1: semantic ANN. LanceDB reports its own distance column.
        let qvec = self.embedder.embed_query(query)?;
        let mut ann: Vec<(i64, f32)> = Vec::new();
        let table = self.lance.open_table(TABLE).execute().await?;
        let batches = table
            .query()
            .limit(leg_k)
            .nearest_to(qvec.clone())?
            .execute()
            .await?;
        for batch in batches.try_collect::<Vec<_>>().await? {
            let ids = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<Int64Array>());
            let dist = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());
            let Some(ids) = ids else { continue };
            for (i, id) in ids.iter().enumerate() {
                if let Some(id) = id {
                    // NAN if LanceDB stops reporting the column: a caller can
                    // test for that, where a silent 0.0 would hide it.
                    let d = dist.map(|d| d.value(i)).unwrap_or(f32::NAN);
                    ann.push((id, d));
                }
            }
        }

        // Leg 2: keyword FTS, three stages (t261). The construction lives in
        // ruagent_store::fts so this crate and the graph crate cannot drift
        // apart again: t247 measured two copies of it plus a third variant.
        let (fts, keyword_stage) = self.keyword_leg(query, leg_k).await?;
        Ok((ann, fts, keyword_stage))
    }

    /// The keyword leg, degrading the same way the entity leg does (t261,
    /// extended by t7):
    /// precision (every term ANDed) -> recall (every term a prefix, ORed) ->
    /// Han bigrams -> substring (LIKE, a full-table scan).
    ///
    /// Stage 2 is what reaches "autohotkey-v2" when the stored text spells it
    /// apart ("AutoHotkey" ... "v2.0.28"): the phrase form demands an
    /// adjacency the text does not have.
    ///
    /// Stage 3 (t7) is the indexed path to a substring of a Han run, which
    /// stage 1 and 2 cannot reach at all: unicode61 makes the WHOLE run one
    /// term. MEASURED (R-A A8): 63.20% of the live corpus's 6922 two-character
    /// Han substrings are neither a token nor a token prefix, and the LIKE scan
    /// that was the only other path cost 6.80–10.47 ms per query, while the
    /// bigram index answered 40/40 at 0.08 ms. Stage 4 stays as the last resort
    /// because the bigram decomposition cannot express every needle (one that
    /// spans a punctuation boundary, for instance).
    ///
    /// All recall stages drop short ASCII terms (see
    /// ruagent_store::fts::MIN_RECALL_ASCII) -- a two-letter fragment matches
    /// whatever word happens to start with it, which is how a stray short word
    /// in a query used to buy a hit on unrelated text. So a recall pattern can
    /// come out empty even though the query has terms; an empty pattern is
    /// skipped rather than handed to MATCH, and the degradation moves on.
    /// Precision is never filtered: there a short term is exact evidence.
    async fn keyword_leg(
        &self,
        query: &str,
        leg_k: usize,
    ) -> Result<(Vec<(i64, f32)>, KeywordStage), KnowledgeError> {
        let terms = ruagent_store::fts::terms(query);
        if terms.is_empty() {
            return Ok((Vec::new(), KeywordStage::Empty));
        }
        for (stage, pattern) in [
            (
                KeywordStage::Precision,
                ruagent_store::fts::match_all(&terms),
            ),
            (
                KeywordStage::Prefix,
                ruagent_store::fts::match_any_prefix(&terms),
            ),
            (
                KeywordStage::Bigram,
                ruagent_store::fts::match_bigrams(&terms),
            ),
        ] {
            if pattern.is_empty() {
                continue;
            }
            let rows = if stage == KeywordStage::Bigram {
                self.fts_match("chunks_fts_cjk", &pattern, leg_k).await?
            } else {
                self.fts_match("chunks_fts", &pattern, leg_k).await?
            };
            if !rows.is_empty() {
                return Ok((rows, stage));
            }
        }
        let patterns = ruagent_store::fts::like_patterns(&terms);
        let rows = self.fts_like(&patterns, leg_k).await?;
        if rows.is_empty() {
            return Ok((Vec::new(), KeywordStage::Empty));
        }
        Ok((rows, KeywordStage::Substring))
    }

    /// One FTS5 MATCH over `table`, scored by bm25 (unchanged semantics).
    ///
    /// `table` is always a literal from this module's own call sites -- never
    /// caller input -- so interpolating it is not an injection path; the
    /// PATTERN is bound as a parameter. The two tables carry bm25 values from
    /// different indexes (`chunks_fts` over content, `chunks_fts_cjk` over the
    /// bigram decomposition), so a `KeywordStage` reader can tell which one a
    /// number came from.
    async fn fts_match(
        &self,
        table: &'static str,
        pattern: &str,
        leg_k: usize,
    ) -> Result<Vec<(i64, f32)>, KnowledgeError> {
        let pattern = pattern.to_string();
        Ok(self
            .db
            .call(move |conn| -> Result<Vec<(i64, f32)>, rusqlite::Error> {
                let sql = format!(
                    "SELECT rowid, bm25({table}) FROM {table}
                     WHERE {table} MATCH ?1 ORDER BY rank LIMIT ?2"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params![pattern, leg_k as i64], |r| {
                    Ok((r.get(0)?, r.get::<_, f64>(1)? as f32))
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .await?
            .map_err(ruagent_store::DbError::from)?)
    }

    /// The substring fallback: LIKE over chunks.content, the terms ORed.
    ///
    /// raw_score is 0.0 for every row of this stage: these rows are not FTS
    /// matches, so there is no bm25 to report (KeywordStage::Substring says
    /// which stage ran, so a reader cannot mistake the 0.0 for a bm25). The
    /// rank order inside the stage is chunk id order -- arbitrary, stable, and
    /// deliberately NOT a relevance claim.
    async fn fts_like(
        &self,
        patterns: &[String],
        leg_k: usize,
    ) -> Result<Vec<(i64, f32)>, KnowledgeError> {
        if patterns.is_empty() {
            return Ok(Vec::new());
        }
        let patterns = patterns.to_vec();
        Ok(self
            .db
            .call(move |conn| -> Result<Vec<(i64, f32)>, rusqlite::Error> {
                let clause = vec!["content LIKE ? ESCAPE '\\'"; patterns.len()].join(" OR ");
                let sql = format!(
                    "SELECT id FROM chunks WHERE {clause} ORDER BY id LIMIT ?{}",
                    patterns.len() + 1
                );
                let mut args: Vec<rusqlite::types::Value> = patterns
                    .iter()
                    .map(|p| rusqlite::types::Value::Text(format!("%{p}%")))
                    .collect();
                args.push(rusqlite::types::Value::Integer(leg_k as i64));
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
                    Ok((r.get(0)?, 0.0f32))
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .await?
            .map_err(ruagent_store::DbError::from)?)
    }

    /// The ONE place the fusion happens. Every entry point calls it, so the
    /// ranking a caller gets from `search()` cannot drift from the ranking
    /// `search_legs()` reports and `search_page()` annotates.
    fn fuse(&self, ann: &[(i64, f32)], fts: &[(i64, f32)]) -> Vec<(i64, f32)> {
        let ann_ids: Vec<i64> = ann.iter().map(|(id, _)| *id).collect();
        let fts_ids: Vec<i64> = fts.iter().map(|(id, _)| *id).collect();
        let (k, w_sem, w_kw) = FUSION.weights();
        rrf_weighted(&[(&ann_ids, w_sem), (&fts_ids, w_kw)], k)
    }

    /// Hydrate chunk ids into hits (the one place that SQL lives).
    async fn hydrate(&self, ids: &[i64]) -> Result<Vec<SearchHit>, KnowledgeError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let ids: Vec<i64> = ids.to_vec();
        Ok(self
            .db
            .call(move |conn| -> Result<Vec<SearchHit>, rusqlite::Error> {
                let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!(
                    "SELECT c.id, d.name, c.content FROM chunks c
                     JOIN documents d ON d.id = c.document_id
                     WHERE c.id IN ({placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                    Ok(SearchHit {
                        chunk_id: row.get(0)?,
                        document: row.get(1)?,
                        content: row.get(2)?,
                        score: 0.0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
            })
            .await?
            .map_err(ruagent_store::DbError::from)?)
    }

    /// Fused hybrid search.
    ///
    /// t7 changed two things and only two: the leg window is now the constant
    /// [`LEG_WINDOW`] instead of `limit.max(10)` (C5 — asking for a second page
    /// used to move the first), and the fusion is [`FUSION`] instead of 1:1
    /// (C1/C2/C9). `SearchHit.score` keeps its exact old meaning; a caller that
    /// wants the leg evidence or the relevance calls `search_page`.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, KnowledgeError> {
        let (ann, fts, _stage) = self.compute_legs(query, LEG_WINDOW).await?;
        let fused = self.fuse(&ann, &fts);
        let top: Vec<i64> = fused
            .iter()
            .take(limit as usize)
            .map(|(id, _)| *id)
            .collect();
        if top.is_empty() {
            return Ok(vec![]);
        }
        let score_by_id: std::collections::HashMap<i64, f32> = fused.into_iter().collect();
        let mut hits = self.hydrate(&top).await?;
        for hit in &mut hits {
            hit.score = score_by_id.get(&hit.chunk_id).copied().unwrap_or(0.0);
        }
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(hits)
    }

    /// Both legs with their raw scores, plus the fused ranking (t250).
    ///
    /// `_limit` is ignored ON PURPOSE and the parameter is kept because the
    /// signature is called from `api.rs` and two test files: the legs now come
    /// from [`LEG_WINDOW`], so a caller cannot change the ranking by asking for
    /// a different page size (C5). Naming it `_limit` is the honest form --
    /// renaming the parameter is not an API change.
    pub async fn search_legs(
        &self,
        query: &str,
        _limit: u32,
    ) -> Result<SearchLegs, KnowledgeError> {
        let (ann, fts, keyword_stage) = self.compute_legs(query, LEG_WINDOW).await?;
        let fused = self.fuse(&ann, &fts);
        let mk = |v: &[(i64, f32)]| -> Vec<LegHit> {
            v.iter()
                .enumerate()
                .map(|(i, (id, s))| LegHit {
                    chunk_id: *id,
                    rank: i,
                    raw_score: *s,
                })
                .collect()
        };
        Ok(SearchEvidence {
            semantic: mk(&ann),
            keyword: mk(&fts),
            keyword_stage,
            candidates: fused.len(),
            fused,
            fusion: FUSION,
            leg_window: LEG_WINDOW,
        })
    }

    /// The paged entry point: hits WITH their evidence (t7 / R-A D.2 item 5).
    ///
    /// WHY IT EXISTS ALONGSIDE `search`: `search`, then `search_legs`, computed
    /// the two legs TWICE for one answer (closure §7.47 measured the second
    /// retrieval at ~60% of the cost of the first). This computes them once and
    /// returns both the annotated hits and the evidence, and it is the entry
    /// point that can carry a relevance score and a scale for every number.
    ///
    /// `limit` bounds the PAGE. It does not bound the legs — that is
    /// [`LEG_WINDOW`] — so the top of page 2 cannot differ from the top of
    /// page 1.
    pub async fn search_page(&self, query: &str, limit: u32) -> Result<SearchPage, KnowledgeError> {
        let (ann, fts, keyword_stage) = self.compute_legs(query, LEG_WINDOW).await?;
        let fused = self.fuse(&ann, &fts);
        let top: Vec<i64> = fused
            .iter()
            .take(limit as usize)
            .map(|(id, _)| *id)
            .collect();
        let evidence = SearchEvidence {
            semantic: ann
                .iter()
                .enumerate()
                .map(|(i, (id, s))| LegHit {
                    chunk_id: *id,
                    rank: i,
                    raw_score: *s,
                })
                .collect(),
            keyword: fts
                .iter()
                .enumerate()
                .map(|(i, (id, s))| LegHit {
                    chunk_id: *id,
                    rank: i,
                    raw_score: *s,
                })
                .collect(),
            keyword_stage,
            candidates: fused.len(),
            fused: fused.clone(),
            fusion: FUSION,
            leg_window: LEG_WINDOW,
        };
        // The relevance is a within-query display score: its background is the
        // semantic leg's own mean distance for THIS query (see
        // relevance_from_distance for what it is and is not).
        let sem_evidence: std::collections::HashMap<i64, LegEvidence> = ann
            .iter()
            .enumerate()
            .map(|(i, (id, s))| {
                (
                    *id,
                    LegEvidence {
                        rank: i,
                        raw_score: *s,
                        kind: ScoreKind::SemanticDistance,
                    },
                )
            })
            .collect();
        let kw_evidence: std::collections::HashMap<i64, LegEvidence> = fts
            .iter()
            .enumerate()
            .map(|(i, (id, s))| {
                (
                    *id,
                    LegEvidence {
                        rank: i,
                        raw_score: *s,
                        kind: ScoreKind::KeywordBm25,
                    },
                )
            })
            .collect();
        let background = match ann.len() {
            0 => 0.0,
            n => ann.iter().map(|(_, d)| d).sum::<f32>() / n as f32,
        };

        let score_by_id: std::collections::HashMap<i64, f32> = fused.into_iter().collect();
        let mut hits = self.hydrate(&top).await?;
        for hit in &mut hits {
            hit.score = score_by_id.get(&hit.chunk_id).copied().unwrap_or(0.0);
        }
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        let hits: Vec<RankedHit> = hits
            .into_iter()
            .map(|hit| {
                let semantic = sem_evidence.get(&hit.chunk_id).copied();
                RankedHit {
                    relevance: semantic
                        .and_then(|e| relevance_from_distance(e.raw_score, background)),
                    score_kind: ScoreKind::RrfRank,
                    semantic,
                    keyword: kw_evidence.get(&hit.chunk_id).copied(),
                    hit,
                }
            })
            .collect();
        Ok(SearchPage { hits, evidence })
    }

    /// Where a document's markdown copy lives: `<root>/knowledge/<name>.md`.
    pub fn document_path(&self, name: &str) -> std::path::PathBuf {
        self.docs_dir.join(format!("{name}.md"))
    }

    /// Residual scan: report where a needle still exists, in the index and on
    /// disk. Read-only, and it never deletes (R-A D.2 item 5 — owed to the
    /// memory crate's "did we really forget it" question).
    ///
    /// `needle` is matched as a LITERAL SUBSTRING, not as tokens: the question
    /// is "is this text still somewhere", and a tokenizer would answer a
    /// different question.
    ///
    /// `truncated` comes from fetching `limit + 1` rows, so a full page never
    /// reads as "that is all there is" — the same lesson the keyword leg's
    /// `LIMIT leg_k` taught. `total` is only filled when `exact_total` is asked
    /// for, because an exact count is the thing that costs a full scan.
    pub async fn residual_scan(
        &self,
        needle: &str,
        limit: u32,
        exact_total: bool,
    ) -> Result<ResidualPage, KnowledgeError> {
        let needle_db = needle.to_string();
        let cap: i64 = if exact_total { -1 } else { limit as i64 + 1 };
        let db_limit = if exact_total { -1 } else { cap };
        let limit_u = limit as usize;
        let db_hits: Vec<(i64, String, Option<i64>)> = self
            .db
            .call(
                move |conn| -> Result<Vec<(i64, String, Option<i64>)>, rusqlite::Error> {
                    let like = format!(
                        "%{}%",
                        needle_db
                            .replace('\\', "\\\\")
                            .replace('%', "\\%")
                            .replace('_', "\\_")
                    );
                    let mut out: Vec<(i64, String, Option<i64>)> = Vec::new();
                    let mut stmt = conn.prepare(
                        "SELECT c.document_id, d.name, c.id FROM chunks c
                     JOIN documents d ON d.id = c.document_id
                     WHERE c.content LIKE ?1 ESCAPE '\\' ORDER BY c.id LIMIT ?2",
                    )?;
                    let rows = stmt.query_map(rusqlite::params![like, db_limit], |r| {
                        Ok((r.get(0)?, r.get::<_, String>(1)?, Some(r.get(2)?)))
                    })?;
                    for row in rows {
                        out.push(row?);
                    }
                    let mut stmt = conn.prepare(
                        "SELECT id, name FROM documents
                     WHERE name LIKE ?1 ESCAPE '\\' ORDER BY id LIMIT ?2",
                    )?;
                    let rows = stmt.query_map(rusqlite::params![like, db_limit], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                    })?;
                    let mut seen: Vec<i64> = out.iter().map(|(d, _, _)| *d).collect();
                    for row in rows {
                        let (id, name) = row?;
                        if !seen.contains(&id) {
                            seen.push(id);
                            out.push((id, name, None));
                        }
                    }
                    Ok(out)
                },
            )
            .await?
            .map_err(ruagent_store::DbError::from)?;

        // The file side: which of these copies is still on disk, plus any file
        // the index no longer knows about at all (origin = File).
        let mut hits: Vec<ResidualHit> = Vec::new();
        let mut known: Vec<String> = Vec::new();
        for (document_id, name, chunk_id) in db_hits {
            let path = self.document_path(&name);
            let on_disk = std::fs::read_to_string(&path)
                .map(|t| t.contains(needle))
                .unwrap_or(false);
            known.push(name.clone());
            hits.push(ResidualHit {
                document_id,
                name,
                chunk_id,
                origin: if on_disk {
                    ResidualOrigin::Both
                } else {
                    ResidualOrigin::Db
                },
                path: Some(path),
            });
        }
        for path in self.markdown_paths()? {
            let rel = match path.strip_prefix(&self.docs_dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let name = rel.with_extension("").to_string_lossy().replace('\\', "/");
            if known.contains(&name) {
                continue;
            }
            if std::fs::read_to_string(&path)
                .map(|t| t.contains(needle))
                .unwrap_or(false)
            {
                hits.push(ResidualHit {
                    document_id: -1,
                    name,
                    chunk_id: None,
                    origin: ResidualOrigin::File,
                    path: Some(path),
                });
            }
        }
        hits.sort_by(|a, b| {
            a.document_id
                .cmp(&b.document_id)
                .then(a.chunk_id.cmp(&b.chunk_id))
                .then(a.name.cmp(&b.name))
        });
        let total = hits.len();
        let truncated = total > limit_u;
        hits.truncate(limit_u);
        Ok(ResidualPage {
            hits,
            truncated,
            total: exact_total.then_some(total),
        })
    }

    /// Every markdown file under `<root>/knowledge`, recursive
    /// (document names are paths without the extension, e.g.
    /// `wiki/kubernetes-troubleshooting`).
    fn markdown_paths(&self) -> Result<Vec<std::path::PathBuf>, KnowledgeError> {
        let mut out = Vec::new();
        let mut stack = vec![self.docs_dir.clone()];
        while let Some(dir) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    out.push(path);
                }
            }
        }
        Ok(out)
    }

    /// List all documents (panel knowledge manager).
    pub async fn list_documents(&self) -> Result<Vec<KnowledgeDocument>, KnowledgeError> {
        Ok(self
            .db
            .call(|conn| -> Result<Vec<KnowledgeDocument>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT id, name, COALESCE(source, ''), chunk_count, created_at
                     FROM documents ORDER BY created_at DESC",
                )?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok(KnowledgeDocument {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            source: row.get(2)?,
                            chunk_count: row.get(3)?,
                            created_at: row.get(4)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await??)
    }

    /// Chunks of one document.
    pub async fn document_chunks(
        &self,
        document_id: i64,
    ) -> Result<Vec<(i64, String)>, KnowledgeError> {
        Ok(self
            .db
            .call(move |conn| -> Result<Vec<(i64, String)>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT id, content FROM chunks WHERE document_id = ?1 ORDER BY idx",
                )?;
                let rows = stmt
                    .query_map([document_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await??)
    }

    /// Delete a document: chunks (+FTS via trigger) and sections from
    /// SQLite, vectors from LanceDB by chunk-id filter. Ordered for the
    /// FK constraints (chunks → sections → document).
    pub async fn delete_document(&self, document_id: i64) -> Result<u32, KnowledgeError> {
        let chunk_ids: Vec<i64> = self
            .db
            .call(move |conn| -> Result<Vec<i64>, rusqlite::Error> {
                let mut stmt = conn.prepare("SELECT id FROM chunks WHERE document_id = ?1")?;
                let ids = stmt
                    .query_map([document_id], |r| r.get(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ids)
            })
            .await??;
        let count = chunk_ids.len() as u32;
        self.db
            .call(move |conn| -> Result<(), rusqlite::Error> {
                conn.execute("DELETE FROM chunks WHERE document_id = ?1", [document_id])?;
                conn.execute(
                    "DELETE FROM chunk_sections WHERE document_id = ?1",
                    [document_id],
                )?;
                conn.execute("DELETE FROM documents WHERE id = ?1", [document_id])?;
                Ok(())
            })
            .await??;
        // Vectors out of LanceDB.
        if !chunk_ids.is_empty() {
            let table = self.lance.open_table(TABLE).execute().await?;
            let filter = format!(
                "id IN ({})",
                chunk_ids
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            table.delete(&filter).await?;
        }
        Ok(count)
    }

    /// Document count + chunk count (panel / MCP browse).
    pub async fn stats(&self) -> Result<(i64, i64), KnowledgeError> {
        let (docs, chunks): (i64, i64) = self
            .db
            .call(|conn| {
                conn.query_row(
                    "SELECT (SELECT COUNT(*) FROM documents), (SELECT COUNT(*) FROM chunks)",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .await?
            .map_err(ruagent_store::DbError::from)?;
        Ok((docs, chunks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ruagent-kb-{}-{}-{tag}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn ingest_search_roundtrip_hybrid() {
        let root = test_root("hybrid");
        let db = Db::open_in_memory().unwrap();
        let kb = Knowledge::open(&root, db).await.unwrap();
        assert_eq!(kb.embedder_name(), "hash-embedder");

        let n1 = kb
            .ingest(
                "deploy-guide",
                "The deploy script lives in scripts/release.sh. Run it from the repository root after merging.",
            )
            .await
            .unwrap();
        assert!(n1 >= 1);
        let n2 = kb
            .ingest(
                "tea-notes",
                "Earl grey tastes best with a slice of lemon and a little honey.",
            )
            .await
            .unwrap();

        // Semantic leg: shared rare words rank the deploy doc first.
        let hits = kb.search("release script deploy", 5).await.unwrap();
        assert!(!hits.is_empty(), "must find something");
        assert_eq!(
            hits[0].document, "deploy-guide",
            "ranked first: {:?}",
            hits[0]
        );

        // Keyword leg: exact tea words.
        let hits = kb.search("lemon honey", 5).await.unwrap();
        assert_eq!(hits[0].document, "tea-notes");

        // Idempotent ingest.
        let again = kb
            .ingest(
                "deploy-guide",
                "The deploy script lives in scripts/release.sh. Run it from the repository root after merging.",
            )
            .await
            .unwrap();
        assert_eq!(again, 0, "same content re-ingested is a no-op");

        let (docs, chunks) = kb.stats().await.unwrap();
        assert_eq!(docs, 2);
        assert!(chunks >= 2);
        let _ = n2;
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Constant vectors; a distinct identity per instance — stands in
    /// for two real models in the migration tests.
    struct Tagged(&'static str, usize);
    impl Embedder for Tagged {
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
            Ok(texts.iter().map(|_| vec![1.0; self.1]).collect())
        }
        fn name(&self) -> &'static str {
            self.0
        }
        fn dim(&self) -> usize {
            self.1
        }
    }

    async fn stored_embedder(db: &Db) -> String {
        db.call(|conn| -> Result<String, rusqlite::Error> {
            conn.query_row(
                "SELECT value FROM knowledge_meta WHERE key = 'embedder'",
                [],
                |r| r.get(0),
            )
        })
        .await
        .unwrap()
        .unwrap()
    }

    #[tokio::test]
    async fn model_switch_migrates_the_table() {
        let root = test_root("migrate");
        let db = Db::open_in_memory().unwrap();
        {
            let kb = Knowledge::with_embedder(&root, db.clone(), Arc::new(Tagged("model-a", 8)))
                .await
                .unwrap();
            kb.ingest(
                "deploy-guide",
                "The deploy script lives in scripts/release.sh. Run it from the root.",
            )
            .await
            .unwrap();
        }
        assert_eq!(stored_embedder(&db).await, "model-a");

        // Reopen with a DIFFERENT model (and even a different dim): the
        // migration adopts it, rebuilds the vector table from the chunk
        // texts, and search still works.
        let kb = Knowledge::with_embedder(&root, db.clone(), Arc::new(Tagged("model-b", 16)))
            .await
            .unwrap();
        assert_eq!(kb.embedder_name(), "model-b");
        assert_eq!(stored_embedder(&db).await, "model-b");
        let hits = kb.search("release.sh", 5).await.unwrap();
        assert!(!hits.is_empty(), "re-embedded table still finds the doc");
        // reopening with the same model is a no-op (no second migration
        // path to assert beyond: it simply opens)
        drop(kb);
        Knowledge::with_embedder(&root, db, Arc::new(Tagged("model-b", 16)))
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn fallback_boot_does_not_migrate_the_table() {
        let root = test_root("fallback");
        let db = Db::open_in_memory().unwrap();
        {
            let kb = Knowledge::with_embedder(&root, db.clone(), Arc::new(Tagged("model-a", 8)))
                .await
                .unwrap();
            kb.ingest("x", "content to anchor the meta").await.unwrap();
        }

        // Offline fallback boot against a real-model table: opens (it
        // used to hard-error and could prevent boot entirely), and
        // NEVER adopts the fallback — the meta still says model-a, so
        // the next boot with a real model migrates cleanly.
        let kb = Knowledge::open(&root, db.clone()).await.unwrap();
        assert_eq!(kb.embedder_name(), "hash-embedder");
        assert_eq!(
            stored_embedder(&db).await,
            "model-a",
            "the fallback must not overwrite the real model's identity"
        );
        drop(kb);
        let _ = std::fs::remove_dir_all(&root);
    }
}
