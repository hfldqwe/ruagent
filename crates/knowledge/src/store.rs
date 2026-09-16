//! The knowledge store: ingestion into SQLite (chunks + FTS) and LanceDB
//! (vectors), and the hybrid search path (design §19, §6.6 #3).

use std::sync::Arc;

use futures::TryStreamExt;
use lancedb::arrow::arrow_array::types::Float32Type;
use lancedb::arrow::arrow_array::{FixedSizeListArray, Int64Array, RecordBatch};
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

/// One ingested document (panel listing).
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
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, KnowledgeError> {
        let leg_k = limit.max(10) as usize;

        // Leg 1: semantic ANN.
        let qvec = self.embedder.embed_query(query)?;
        let mut ann_ids: Vec<i64> = Vec::new();
        let table = self.lance.open_table(TABLE).execute().await?;
        let batches = table
            .query()
            .limit(leg_k)
            .nearest_to(qvec.clone())?
            .execute()
            .await?;
        for batch in batches.try_collect::<Vec<_>>().await? {
            if let Some(ids) = batch.column_by_name("id")
                && let Some(ids) = ids.as_any().downcast_ref::<Int64Array>()
            {
                ann_ids.extend(ids.iter().flatten());
            }
        }

        // Leg 2: keyword FTS. Punctuated tokens (deploy.sh, scripts/*)
        // are FTS5 syntax errors as raw input — quote each token into a
        // literal phrase (the same treatment memory recall uses).
        let fts_query = query
            .split_whitespace()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" ");
        let fts_ids: Vec<i64> = if fts_query.is_empty() {
            Vec::new()
        } else {
            self.db
                .call(move |conn| -> Result<Vec<i64>, rusqlite::Error> {
                    let mut stmt = conn.prepare(
                        "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1
                         ORDER BY rank LIMIT ?2",
                    )?;
                    let ids = stmt
                        .query_map(rusqlite::params![fts_query, leg_k as i64], |r| r.get(0))?
                        .collect::<Result<Vec<i64>, _>>()?;
                    Ok(ids)
                })
                .await?
                .map_err(ruagent_store::DbError::from)?
        };

        // Fuse.
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
                let mut hits = stmt
                    .query_map(rusqlite::params_from_iter(top_clone.iter()), |row| {
                        Ok(SearchHit {
                            chunk_id: row.get(0)?,
                            document: row.get(1)?,
                            content: row.get(2)?,
                            score: 0.0,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                for hit in &mut hits {
                    hit.score = 0.0; // filled below
                }
                Ok(hits)
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
