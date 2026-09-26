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
use crate::rrf::rrf;

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

/// One leg's result for a query, with that leg's OWN score (t250).
#[derive(Debug, Clone, PartialEq)]
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

/// Which construction produced the keyword leg (t261).
///
/// The first two stages are real FTS5 matches and carry bm25. The third is a
/// LIKE scan: those rows are NOT FTS matches, so no bm25 exists for them --
/// saying so in the type is better than a fabricated number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordStage {
    /// No term survived tokenisation, or no stage found anything.
    Empty,
    /// Every term as a literal phrase, implicitly ANDed.
    Precision,
    /// Every term as a prefix, ORed. Runs only when Precision was empty.
    Prefix,
    /// LIKE substring scan. Runs only when both FTS stages were empty; the
    /// only path that can find a substring of a Han run.
    Substring,
}

/// Both legs plus the fused ranking (t250).
///
/// WHY THIS EXISTS: the store computed two legs all along and then dropped
/// their raw scores inside rrf(), which keeps only ranks. So the only score the
/// platform could show was the fused rank score, whose upper bound is legs/61
/// -- measured at 3 distinct values across 574 live rows and IDENTICAL for a
/// 1-word and a 7-word query (t247). A caller that can see the legs can say
/// where a hit came from and how well it scored.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchLegs {
    pub semantic: Vec<LegHit>,
    pub keyword: Vec<LegHit>,
    /// Which keyword construction ran (t261). The raw_score of a keyword hit
    /// is bm25 for Precision/Prefix and 0.0 for Substring (no bm25 exists
    /// there, so a number would be invented).
    pub keyword_stage: KeywordStage,
    pub fused: Vec<(i64, f32)>,
}

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
                                (document_id, idx, content, span_start, span_end, section_id)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            rusqlite::params![
                                doc_id,
                                ids.len() as i64,
                                chunk.content,
                                chunk.start as i64,
                                chunk.end as i64,
                                section_id
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

    /// The keyword leg, degrading the same way the entity leg does (t261):
    /// precision (every term ANDed) -> recall (every term a prefix, ORed) ->
    /// substring (LIKE).
    ///
    /// Stage 2 is what reaches "autohotkey-v2" when the stored text spells it
    /// apart ("AutoHotkey" ... "v2.0.28"): the phrase form demands an
    /// adjacency the text does not have. Stage 3 is the only path to a
    /// substring of a Han run, because unicode61 makes the whole run one term.
    ///
    /// Both recall stages drop short ASCII terms (see
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
        ] {
            if pattern.is_empty() {
                continue;
            }
            let rows = self.fts_match(&pattern, leg_k).await?;
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

    /// One FTS5 MATCH over chunks_fts, scored by bm25 (unchanged semantics).
    async fn fts_match(
        &self,
        pattern: &str,
        leg_k: usize,
    ) -> Result<Vec<(i64, f32)>, KnowledgeError> {
        let pattern = pattern.to_string();
        Ok(self
            .db
            .call(move |conn| -> Result<Vec<(i64, f32)>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT rowid, bm25(chunks_fts) FROM chunks_fts
                     WHERE chunks_fts MATCH ?1 ORDER BY rank LIMIT ?2",
                )?;
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

    /// Fused hybrid search. t250 left the ranking untouched; t261 changes what
    /// the keyword leg can find (see keyword_leg), so the fused order can move.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, KnowledgeError> {
        let leg_k = limit.max(10) as usize;
        let (ann, fts, _stage) = self.compute_legs(query, leg_k).await?;
        let ann_ids: Vec<i64> = ann.iter().map(|(id, _)| *id).collect();
        let fts_ids: Vec<i64> = fts.iter().map(|(id, _)| *id).collect();
        let fused = rrf(&[ann_ids, fts_ids], 60);
        let top: Vec<i64> = fused
            .iter()
            .take(limit as usize)
            .map(|(id, _)| *id)
            .collect();
        if top.is_empty() {
            return Ok(vec![]);
        }
        let score_by_id: std::collections::HashMap<i64, f32> = fused.into_iter().collect();

        // Hydrate from SQLite.
        let top_clone = top.clone();
        let hits = self
            .db
            .call(move |conn| -> Result<Vec<SearchHit>, rusqlite::Error> {
                let placeholders = top_clone.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!(
                    "SELECT c.id, d.name, c.content FROM chunks c
                     JOIN documents d ON d.id = c.document_id
                     WHERE c.id IN ({placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                stmt.query_map(rusqlite::params_from_iter(top_clone.iter()), |row| {
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
            .map_err(ruagent_store::DbError::from)?;
        let mut hits = hits;
        for hit in &mut hits {
            hit.score = score_by_id.get(&hit.chunk_id).copied().unwrap_or(0.0);
        }
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(hits)
    }

    /// Both legs with their raw scores, plus the fused ranking (t250).
    pub async fn search_legs(&self, query: &str, limit: u32) -> Result<SearchLegs, KnowledgeError> {
        let leg_k = limit.max(10) as usize;
        let (ann, fts, keyword_stage) = self.compute_legs(query, leg_k).await?;
        let ann_ids: Vec<i64> = ann.iter().map(|(id, _)| *id).collect();
        let fts_ids: Vec<i64> = fts.iter().map(|(id, _)| *id).collect();
        let fused = rrf(&[ann_ids, fts_ids], 60);
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
        Ok(SearchLegs {
            semantic: mk(&ann),
            keyword: mk(&fts),
            keyword_stage,
            fused,
        })
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
