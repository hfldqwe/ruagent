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

use crate::chunk::chunk_text;
use crate::embed::{EmbedError, Embedder, HashEmbedder};
use crate::fnv1a;
use crate::rrf::rrf;

const TABLE: &str = "knowledge_chunks";

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
    db: Db,
    lance: Connection,
    embedder: Arc<dyn Embedder>,
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
        let this = Self {
            db,
            lance,
            embedder,
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
            Some(stored) => Err(KnowledgeError::ModelMismatch { stored, active }),
        }
    }

    /// Ingest a document: chunk → embed → SQLite + LanceDB.
    /// Idempotent per (name, content-hash).
    pub async fn ingest(&self, name: &str, content: &str) -> Result<u32, KnowledgeError> {
        let hash = format!("{:016x}", fnv1a(content.as_bytes()));
        let name = name.to_string();
        let name_for_check = name.clone();
        let hash_for_check = hash.clone();

        // Idempotency: same name+hash already ingested.
        let exists: Option<i64> = self
            .db
            .call(move |conn| -> Result<Option<i64>, rusqlite::Error> {
                conn.query_row(
                    "SELECT id FROM documents WHERE name = ?1 AND content_hash = ?2",
                    rusqlite::params![name_for_check, hash_for_check],
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
        if exists.is_some() {
            return Ok(0);
        }

        let chunks = chunk_text(content);
        if chunks.is_empty() {
            return Ok(0);
        }
        let vectors = self
            .embedder
            .embed(&chunks.iter().map(String::as_str).collect::<Vec<_>>())?;

        let chunk_count = chunks.len() as i64;
        let doc_id: i64 = self
            .db
            .call(move |conn| -> Result<i64, rusqlite::Error> {
                conn.execute(
                    "INSERT INTO documents (name, source, content_hash, chunk_count, created_at)
                     VALUES (?1, NULL, ?2, ?3, ?4)",
                    rusqlite::params![name, hash, chunk_count, chrono::Utc::now().to_rfc3339()],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await?
            .map_err(ruagent_store::DbError::from)?;

        // Insert chunks, collecting ids.
        let mut ids = Vec::with_capacity(chunks.len());
        for (idx, chunk) in chunks.iter().enumerate() {
            let chunk = chunk.clone();
            let idx = idx as i64;
            let id: i64 = self
                .db
                .call(move |conn| -> Result<i64, rusqlite::Error> {
                    conn.execute(
                        "INSERT INTO chunks (document_id, idx, content) VALUES (?1, ?2, ?3)",
                        rusqlite::params![doc_id, idx, chunk],
                    )?;
                    Ok(conn.last_insert_rowid())
                })
                .await?
                .map_err(ruagent_store::DbError::from)?;
            ids.push(id);
        }

        // Vectors into LanceDB.
        self.add_vectors(&ids, &vectors).await?;
        Ok(ids.len() as u32)
    }

    async fn add_vectors(&self, ids: &[i64], vectors: &[Vec<f32>]) -> Result<(), KnowledgeError> {
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
        let qvec = self.embedder.embed(&[query])?.remove(0);
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

        // Leg 2: keyword FTS.
        let q = query.to_string();
        let fts_ids: Vec<i64> = self
            .db
            .call(move |conn| -> Result<Vec<i64>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1
                     ORDER BY rank LIMIT ?2",
                )?;
                let ids = stmt
                    .query_map(rusqlite::params![q, leg_k as i64], |r| r.get(0))?
                    .collect::<Result<Vec<i64>, _>>()?;
                Ok(ids)
            })
            .await?
            .map_err(ruagent_store::DbError::from)?;

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

    /// Delete a document: chunks (+FTS via trigger) from SQLite, vectors
    /// from LanceDB by chunk-id filter.
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
        if chunk_ids.is_empty() {
            return Ok(0);
        }
        let count = chunk_ids.len() as u32;
        let ids_for_db = chunk_ids.clone();
        self.db
            .call(move |conn| -> Result<(), rusqlite::Error> {
                conn.execute("DELETE FROM chunks WHERE document_id = ?1", [document_id])?;
                conn.execute("DELETE FROM documents WHERE id = ?1", [document_id])?;
                let _ = ids_for_db;
                Ok(())
            })
            .await??;
        // Vectors out of LanceDB.
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

    #[tokio::test]
    async fn model_mismatch_is_a_hard_error() {
        let root = test_root("mismatch");
        let db = Db::open_in_memory().unwrap();
        // Open with hash embedder (records meta).
        let kb = Knowledge::open(&root, db.clone()).await.unwrap();
        kb.ingest("x", "some content to anchor the meta")
            .await
            .unwrap();
        drop(kb);

        // Reopen with a DIFFERENT embedder identity -> hard error.
        struct Other;
        impl Embedder for Other {
            fn embed(&self, _t: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
                Ok(vec![])
            }
            fn name(&self) -> &'static str {
                "other-embedder"
            }
            fn dim(&self) -> usize {
                8
            }
        }
        let err = Knowledge::with_embedder(&root, db, Arc::new(Other)).await;
        assert!(matches!(err, Err(KnowledgeError::ModelMismatch { .. })));
        let _ = std::fs::remove_dir_all(&root);
    }
}
