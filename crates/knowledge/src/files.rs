//! Markdown documents as the source of truth (design-study memsearch /
//! EverOS): every knowledge document is a human-readable, git-friendly
//! `.md` file under `<root>/knowledge/`; the SQLite + LanceDB index is
//! a derived, fully rebuildable shadow. Also home of the WeKnora-style
//! curation surface: chunk edits with revision history and rollback,
//! and parent-section expansion (parent-child retrieval).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::store::{Knowledge, KnowledgeError, TABLE};

/// Outcome of indexing one document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexOutcome {
    /// Same name + content hash already indexed — nothing was done.
    Unchanged,
    /// (Re)indexed; N chunks written.
    Indexed(u32),
    /// The content produced no chunks (empty/whitespace document).
    Empty,
}

/// One pass of the file → index sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct ScanReport {
    pub indexed: u32,
    pub unchanged: u32,
    pub removed: u32,
    pub errors: u32,
}

impl ScanReport {
    pub fn changed(&self) -> u32 {
        self.indexed + self.removed + self.errors
    }
}

/// One recorded chunk edit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChunkRevision {
    pub id: i64,
    pub chunk_id: i64,
    pub document_name: String,
    pub old_content: String,
    pub new_content: String,
    pub edited_at: String,
}

/// A hit expanded into its parent section (memsearch's `expand` layer).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Expansion {
    pub chunk_id: i64,
    pub document: String,
    /// The full parent section: heading + its paragraphs.
    pub section: String,
    /// The hit chunk itself.
    pub chunk: String,
    /// Backing file name (markdown-file documents only).
    pub file: Option<String>,
}

/// Result of a chunk edit or revision rollback.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditOutcome {
    pub revision: i64,
    pub document: String,
    /// Chunks in the document after the (re)index.
    pub chunks: i64,
}

fn invalid_name(name: &str) -> KnowledgeError {
    KnowledgeError::Other(format!(
        "invalid document name `{name}` (no path separators, `..`, leading dot, \
         or Windows-reserved characters; 1..=128 chars)"
    ))
}

/// One chunk joined with its document, as `edit_chunk` needs it.
struct ChunkRow {
    document_id: i64,
    content: String,
    span_start: Option<i64>,
    span_end: Option<i64>,
    name: String,
    source: Option<String>,
}

/// `<name>` → a safe `<name>.md` file name (accepts an optional `.md`
/// suffix on input).
fn doc_file_name(name: &str) -> Result<String, KnowledgeError> {
    let name = name.trim();
    let stem = name.strip_suffix(".md").unwrap_or(name);
    if stem.is_empty() || stem.chars().count() > 128 {
        return Err(invalid_name(name));
    }
    if stem.contains(['/', '\\']) || stem.contains("..") || stem.starts_with('.') {
        return Err(invalid_name(name));
    }
    if stem
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || c.is_control())
    {
        return Err(invalid_name(name));
    }
    Ok(format!("{stem}.md"))
}

/// The canonical document name for a file name (`foo.md` → `foo`).
fn canonical_name(file_name: &str) -> String {
    file_name
        .strip_suffix(".md")
        .unwrap_or(file_name)
        .to_string()
}

impl Knowledge {
    /// The markdown source-of-truth directory.
    pub fn docs_dir(&self) -> &std::path::Path {
        &self.docs_dir
    }

    /// Delete a document completely: index rows AND, for file-backed
    /// documents, the `.md` itself (otherwise the next scan would
    /// resurrect it — the file is the truth). Returns removed chunks.
    pub async fn delete_document_with_file(&self, document_id: i64) -> Result<u32, KnowledgeError> {
        let source: Option<String> = self
            .db
            .call(move |conn| -> Result<Option<String>, rusqlite::Error> {
                conn.query_row(
                    "SELECT source FROM documents WHERE id = ?1",
                    [document_id],
                    |r| r.get::<_, Option<String>>(0),
                )
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })
            })
            .await??;
        let count = self.delete_document(document_id).await?;
        if let Some(source) = source {
            let _ = std::fs::remove_file(self.docs_dir.join(source));
        }
        Ok(count)
    }

    /// Save a document: write `<root>/knowledge/<name>.md` and index
    /// it. This is the ingest path going forward — the file is the
    /// source of truth, the index is derived.
    pub async fn save(&self, name: &str, content: &str) -> Result<u32, KnowledgeError> {
        let file_name = doc_file_name(name)?;
        let path = self.docs_dir.join(&file_name);
        std::fs::write(&path, content)?;
        match self
            .index_doc(&canonical_name(&file_name), content, Some(&file_name))
            .await?
        {
            IndexOutcome::Indexed(n) => Ok(n),
            _ => Ok(0),
        }
    }

    /// Read the raw markdown of a document (None when no file exists).
    pub fn read_raw(&self, name: &str) -> Result<Option<String>, KnowledgeError> {
        let file_name = doc_file_name(name)?;
        match std::fs::read_to_string(self.docs_dir.join(file_name)) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Incremental sync of the index with the knowledge directory:
    /// new/changed files are (re)indexed (SHA-256 content hash skips
    /// unchanged documents), files that vanished take their rows with
    /// them. Non-file rows (legacy ingests) are never touched.
    pub async fn scan(&self) -> Result<ScanReport, KnowledgeError> {
        let mut report = ScanReport::default();
        let mut present: Vec<String> = Vec::new();

        let entries = match std::fs::read_dir(self.docs_dir()) {
            Ok(entries) => entries,
            Err(_) => return Ok(report), // no dir yet: nothing to sync
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let name = canonical_name(file_name);
            let Ok(content) = std::fs::read_to_string(&path) else {
                report.errors += 1;
                continue;
            };
            present.push(name.clone());
            match self.index_doc(&name, &content, Some(file_name)).await {
                Ok(IndexOutcome::Indexed(_)) => report.indexed += 1,
                Ok(_) => report.unchanged += 1,
                Err(e) => {
                    tracing::warn!(document = %name, error = %e, "knowledge scan: index failed");
                    report.errors += 1;
                }
            }
        }

        // File-backed rows whose file is gone.
        let rows: Vec<(i64, String)> = self
            .db
            .call(|conn| -> Result<Vec<(i64, String)>, rusqlite::Error> {
                let mut stmt =
                    conn.prepare("SELECT id, name FROM documents WHERE source IS NOT NULL")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await??;
        for (id, name) in rows {
            if !present.contains(&name) {
                match self.delete_document(id).await {
                    Ok(_) => report.removed += 1,
                    Err(_) => report.errors += 1,
                }
            }
        }
        Ok(report)
    }

    /// Rebuild the shadow index from the markdown files: force
    /// reindex of every document (hashes ignored). The proof that the
    /// index is disposable.
    pub async fn rebuild(&self) -> Result<ScanReport, KnowledgeError> {
        let mut report = ScanReport::default();
        let entries = match std::fs::read_dir(self.docs_dir()) {
            Ok(entries) => entries,
            Err(_) => return Ok(report),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let name = canonical_name(file_name);
            let Ok(content) = std::fs::read_to_string(&path) else {
                report.errors += 1;
                continue;
            };
            // Force: drop the current rows for this name first.
            let ids: Vec<i64> = self
                .db
                .call({
                    let name = name.clone();
                    move |conn| -> Result<Vec<i64>, rusqlite::Error> {
                        let mut stmt = conn.prepare("SELECT id FROM documents WHERE name = ?1")?;
                        let ids = stmt
                            .query_map(rusqlite::params![name], |r| r.get(0))?
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(ids)
                    }
                })
                .await??;
            for id in ids {
                self.delete_document(id).await?;
            }
            match self.index_doc(&name, &content, Some(file_name)).await {
                Ok(IndexOutcome::Indexed(_)) => report.indexed += 1,
                Ok(_) => report.unchanged += 1,
                Err(_) => report.errors += 1,
            }
        }
        Ok(report)
    }

    /// Edit one chunk's content (WeKnora chunk editing): records a
    /// revision, rewrites the source `.md` through the chunk's span,
    /// and reindexes the document. For legacy non-file documents the
    /// chunk is edited in place and re-embedded.
    pub async fn edit_chunk(
        &self,
        chunk_id: i64,
        new_content: &str,
    ) -> Result<EditOutcome, KnowledgeError> {
        let row: Option<ChunkRow> = self
            .db
            .call(move |conn| -> Result<Option<ChunkRow>, rusqlite::Error> {
                conn.query_row(
                    "SELECT c.document_id, c.content, c.span_start, c.span_end,
                            d.name, d.source
                       FROM chunks c JOIN documents d ON d.id = c.document_id
                      WHERE c.id = ?1",
                    [chunk_id],
                    |r| {
                        Ok(ChunkRow {
                            document_id: r.get(0)?,
                            content: r.get(1)?,
                            span_start: r.get(2)?,
                            span_end: r.get(3)?,
                            name: r.get(4)?,
                            source: r.get(5)?,
                        })
                    },
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })
            })
            .await??;
        let Some(chunk) = row else {
            return Err(KnowledgeError::Other(format!("chunk {chunk_id} not found")));
        };
        let ChunkRow {
            document_id: doc_id,
            content: old_content,
            span_start,
            span_end,
            name: doc_name,
            source,
        } = chunk;
        if new_content == old_content {
            return Err(KnowledgeError::Other(
                "new content is identical to the current chunk".into(),
            ));
        }

        let revision = self
            .record_revision(chunk_id, doc_id, &doc_name, &old_content, new_content)
            .await?;

        let chunk_count = if let Some(source) = source {
            // File-backed: the .md file is the truth — rewrite the span.
            let path: PathBuf = self.docs_dir.join(&source);
            let mut text = std::fs::read_to_string(&path)?;
            let replaced = match (span_start, span_end) {
                (Some(start), Some(end))
                    if (end as usize) <= text.len()
                        && text.is_char_boundary(start as usize)
                        && text.is_char_boundary(end as usize) =>
                {
                    text.replace_range(start as usize..end as usize, new_content);
                    true
                }
                _ => match text.find(&old_content) {
                    // Pre-migration row without spans: locate the old
                    // text instead.
                    Some(pos) => {
                        text.replace_range(pos..pos + old_content.len(), new_content);
                        true
                    }
                    None => false,
                },
            };
            if !replaced {
                return Err(KnowledgeError::Other(format!(
                    "cannot locate chunk {chunk_id} in `{source}` — re-save the document"
                )));
            }
            std::fs::write(&path, &text)?;
            match self.index_doc(&doc_name, &text, Some(&source)).await? {
                IndexOutcome::Indexed(n) => n as i64,
                _ => 0,
            }
        } else {
            // Legacy index-only document: edit in place, re-embed.
            self.replace_chunk_in_place(chunk_id, doc_id, new_content)
                .await? as i64
        };

        Ok(EditOutcome {
            revision,
            document: doc_name,
            chunks: chunk_count,
        })
    }

    /// Revision history of one chunk (newest first).
    pub async fn chunk_revisions(
        &self,
        chunk_id: i64,
    ) -> Result<Vec<ChunkRevision>, KnowledgeError> {
        Ok(self
            .db
            .call(move |conn| -> Result<Vec<ChunkRevision>, rusqlite::Error> {
                let mut stmt = conn.prepare(
                    "SELECT id, chunk_id, document_name, old_content, new_content, edited_at
                       FROM chunk_revisions WHERE chunk_id = ?1 ORDER BY id DESC",
                )?;
                let rows = stmt
                    .query_map([chunk_id], |r| {
                        Ok(ChunkRevision {
                            id: r.get(0)?,
                            chunk_id: r.get(1)?,
                            document_name: r.get(2)?,
                            old_content: r.get(3)?,
                            new_content: r.get(4)?,
                            edited_at: r.get(5)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await??)
    }

    /// Roll a revision back: the edited text goes back to its previous
    /// form and the reverse edit is recorded as a new revision (a
    /// rollback is itself history, WeKnora-style).
    pub async fn rollback_revision(&self, revision_id: i64) -> Result<EditOutcome, KnowledgeError> {
        let rev: Option<ChunkRevision> = self
            .db
            .call(
                move |conn| -> Result<Option<ChunkRevision>, rusqlite::Error> {
                    conn.query_row(
                        "SELECT id, chunk_id, document_name, old_content, new_content, edited_at
                       FROM chunk_revisions WHERE id = ?1",
                        [revision_id],
                        |r| {
                            Ok(ChunkRevision {
                                id: r.get(0)?,
                                chunk_id: r.get(1)?,
                                document_name: r.get(2)?,
                                old_content: r.get(3)?,
                                new_content: r.get(4)?,
                                edited_at: r.get(5)?,
                            })
                        },
                    )
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        e => Err(e),
                    })
                },
            )
            .await??;
        let Some(rev) = rev else {
            return Err(KnowledgeError::Other(format!(
                "revision {revision_id} not found"
            )));
        };

        let doc: Option<(i64, Option<String>)> = self
            .db
            .call({
                let name = rev.document_name.clone();
                move |conn| -> Result<Option<(i64, Option<String>)>, rusqlite::Error> {
                    conn.query_row(
                        "SELECT id, source FROM documents WHERE name = ?1",
                        [&name],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        e => Err(e),
                    })
                }
            })
            .await??;
        let Some((doc_id, source)) = doc else {
            return Err(KnowledgeError::Other(format!(
                "document `{}` of revision {revision_id} no longer exists",
                rev.document_name
            )));
        };

        // The reverse edit, recorded before mutating anything.
        let revision = self
            .record_revision(
                rev.chunk_id,
                doc_id,
                &rev.document_name,
                &rev.new_content,
                &rev.old_content,
            )
            .await?;

        let chunk_count = if let Some(source) = source {
            let path = self.docs_dir.join(&source);
            let mut text = std::fs::read_to_string(&path)?;
            let Some(pos) = text.find(&rev.new_content) else {
                return Err(KnowledgeError::Other(format!(
                    "edited text of revision {revision_id} is no longer in `{source}` \
                     (the document changed since) — edit the file directly"
                )));
            };
            text.replace_range(pos..pos + rev.new_content.len(), &rev.old_content);
            std::fs::write(&path, &text)?;
            match self
                .index_doc(&rev.document_name, &text, Some(&source))
                .await?
            {
                IndexOutcome::Indexed(n) => n as i64,
                _ => 0,
            }
        } else {
            // Legacy in-place document: find the chunk carrying the
            // edited text and swap it back.
            let target: Option<i64> = self
                .db
                .call({
                    let (doc_id, needle) = (doc_id, rev.new_content.clone());
                    move |conn| -> Result<Option<i64>, rusqlite::Error> {
                        conn.query_row(
                            "SELECT id FROM chunks WHERE document_id = ?1 AND content = ?2",
                            rusqlite::params![doc_id, needle],
                            |r| r.get(0),
                        )
                        .map(Some)
                        .or_else(|e| match e {
                            rusqlite::Error::QueryReturnedNoRows => Ok(None),
                            e => Err(e),
                        })
                    }
                })
                .await??;
            let Some(target) = target else {
                return Err(KnowledgeError::Other(format!(
                    "chunk of revision {revision_id} no longer matches — cannot roll back"
                )));
            };
            self.replace_chunk_in_place(target, doc_id, &rev.old_content)
                .await? as i64
        };

        Ok(EditOutcome {
            revision,
            document: rev.document_name,
            chunks: chunk_count,
        })
    }

    /// Expand one hit into its parent section: the complete context
    /// around the chunk (memsearch's `expand`, WeKnora's parent block).
    pub async fn expand(&self, chunk_id: i64) -> Result<Expansion, KnowledgeError> {
        let row: Option<(String, String, Option<String>, Option<String>)> = self
            .db
            .call(move |conn| -> Result<Option<_>, rusqlite::Error> {
                conn.query_row(
                    "SELECT d.name, c.content, s.content, d.source
                       FROM chunks c
                       JOIN documents d ON d.id = c.document_id
                       LEFT JOIN chunk_sections s ON s.id = c.section_id
                      WHERE c.id = ?1",
                    [chunk_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })
            })
            .await??;
        let Some((document, chunk, section, file)) = row else {
            return Err(KnowledgeError::Other(format!("chunk {chunk_id} not found")));
        };
        Ok(Expansion {
            chunk_id,
            document,
            // Legacy rows without sections: the chunk is its own parent.
            section: section.unwrap_or_else(|| chunk.clone()),
            chunk,
            file,
        })
    }

    /// Parent sections for a batch of chunk ids (recall's aggressive
    /// strategy returns the parent, not the fragment). Chunks without
    /// a section are absent from the map.
    pub async fn parents_for(&self, chunk_ids: &[i64]) -> HashMap<i64, String> {
        if chunk_ids.is_empty() {
            return HashMap::new();
        }
        let placeholders = chunk_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT c.id, s.content FROM chunks c
              JOIN chunk_sections s ON s.id = c.section_id
             WHERE c.id IN ({placeholders})"
        );
        let chunk_ids = chunk_ids.to_vec();
        self.db
            .call(
                move |conn| -> Result<HashMap<i64, String>, rusqlite::Error> {
                    let mut stmt = conn.prepare(&sql)?;
                    let rows = stmt
                        .query_map(rusqlite::params_from_iter(chunk_ids.iter()), |r| {
                            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                        })?
                        .collect::<Result<HashMap<_, _>, _>>()?;
                    Ok(rows)
                },
            )
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default()
    }

    /// Update a legacy (non-file) chunk in place: SQLite content + FTS
    /// trigger + vector swap in LanceDB. Returns the document's chunk
    /// count.
    async fn replace_chunk_in_place(
        &self,
        chunk_id: i64,
        document_id: i64,
        new_content: &str,
    ) -> Result<u32, KnowledgeError> {
        let content = new_content.to_string();
        self.db
            .call(move |conn| -> Result<(), rusqlite::Error> {
                conn.execute(
                    "UPDATE chunks SET content = ?1 WHERE id = ?2",
                    rusqlite::params![content, chunk_id],
                )?;
                Ok(())
            })
            .await??;
        // Vector swap under the same id.
        let table = self.lance.open_table(TABLE).execute().await?;
        table.delete(&format!("id = {chunk_id}")).await?;
        if let Ok(vectors) = self.embedder.embed(&[new_content])
            && let Some(v) = vectors.first()
        {
            self.add_vectors(&[chunk_id], std::slice::from_ref(v))
                .await?;
        }
        let count: i64 = self
            .db
            .call(move |conn| -> Result<i64, rusqlite::Error> {
                conn.query_row(
                    "SELECT COUNT(*) FROM chunks WHERE document_id = ?1",
                    [document_id],
                    |r| r.get(0),
                )
            })
            .await??;
        Ok(count as u32)
    }

    async fn record_revision(
        &self,
        chunk_id: i64,
        document_id: i64,
        document_name: &str,
        old_content: &str,
        new_content: &str,
    ) -> Result<i64, KnowledgeError> {
        let name = document_name.to_string();
        let old = old_content.to_string();
        let new = new_content.to_string();
        Ok(self
            .db
            .call(move |conn| -> Result<i64, rusqlite::Error> {
                conn.execute(
                    "INSERT INTO chunk_revisions
                        (chunk_id, document_id, document_name, old_content, new_content, edited_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        chunk_id,
                        document_id,
                        name,
                        old,
                        new,
                        chrono::Utc::now().to_rfc3339()
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await??)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_kb(tag: &str) -> (Knowledge, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ruagent-kbfiles-{}-{}-{tag}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = ruagent_store::Db::open_in_memory().unwrap();
        let kb = Knowledge::open(&dir, db).await.unwrap();
        (kb, dir)
    }

    fn doc() -> String {
        "# Deploy guide\n\nThe deploy script lives in scripts/release.sh.\n\nRun it from the repository root after merging.\n\n# Tea notes\n\nEarl grey tastes best with a slice of lemon.".to_string()
    }

    #[tokio::test]
    async fn save_writes_file_and_indexes() {
        let (kb, root) = test_kb("save").await;
        let n = kb.save("deploy-guide", &doc()).await.unwrap();
        assert!(n >= 2, "two sections → at least two chunks: {n}");
        let raw = kb.read_raw("deploy-guide").unwrap().unwrap();
        assert_eq!(raw, doc());
        // .md suffix on input maps to the same file.
        let raw2 = kb.read_raw("deploy-guide.md").unwrap().unwrap();
        assert_eq!(raw2, doc());
        let (docs, _) = kb.stats().await.unwrap();
        assert_eq!(docs, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn invalid_names_are_rejected() {
        let (kb, root) = test_kb("names").await;
        for bad in ["", "..", "a/b", "a\\b", ".hidden", "a<b", "con:"] {
            assert!(kb.save(bad, "x").await.is_err(), "name {bad:?} must fail");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn delete_takes_the_file_with_it() {
        let (kb, root) = test_kb("delete").await;
        kb.save("guide", &doc()).await.unwrap();
        let docs = kb.list_documents().await.unwrap();
        kb.delete_document_with_file(docs[0].id).await.unwrap();
        assert!(
            !root.join("knowledge").join("guide.md").exists(),
            "file-backed delete removes the .md"
        );
        let (docs, _) = kb.stats().await.unwrap();
        assert_eq!(docs, 0);
        // And the scanner has nothing to resurrect.
        let report = kb.scan().await.unwrap();
        assert_eq!(report, ScanReport::default());

        // Legacy rows (no file) delete their rows only.
        kb.ingest("legacy", "# Legacy\n\nrow without a file.")
            .await
            .unwrap();
        let docs = kb.list_documents().await.unwrap();
        kb.delete_document_with_file(docs[0].id).await.unwrap();
        let (docs, _) = kb.stats().await.unwrap();
        assert_eq!(docs, 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn scan_picks_up_out_of_band_edits_and_deletes() {
        let (kb, root) = test_kb("scan").await;
        kb.save("guide", &doc()).await.unwrap();
        kb.save("other", "# Other\n\nSome unrelated text.")
            .await
            .unwrap();

        // Nothing changed: the second scan skips both (SHA-256).
        let report = kb.scan().await.unwrap();
        assert_eq!(
            report,
            ScanReport {
                indexed: 0,
                unchanged: 2,
                removed: 0,
                errors: 0
            }
        );

        // Edit the file behind the daemon's back (vim / git checkout).
        let edited = doc().replace("scripts/release.sh", "scripts/deploy.sh");
        std::fs::write(root.join("knowledge").join("guide.md"), &edited).unwrap();
        let report = kb.scan().await.unwrap();
        assert_eq!(report.indexed, 1, "changed file reindexes: {report:?}");
        let hits = kb.search("deploy.sh", 5).await.unwrap();
        assert!(hits.iter().any(|h| h.content.contains("scripts/deploy.sh")));

        // Delete the file: the row goes with it.
        std::fs::remove_file(root.join("knowledge").join("other.md")).unwrap();
        let report = kb.scan().await.unwrap();
        assert_eq!(
            report.removed, 1,
            "vanished file removes its row: {report:?}"
        );
        let (docs, _) = kb.stats().await.unwrap();
        assert_eq!(docs, 1);
        // The ANN leg always returns nearest neighbours (no threshold),
        // so the assertion is that the deleted content itself is gone.
        let hits = kb.search("unrelated text", 5).await.unwrap();
        assert!(
            hits.iter().all(|h| !h.content.contains("unrelated")),
            "deleted content must not be retrievable: {hits:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn legacy_rows_survive_scan() {
        let (kb, root) = test_kb("legacy").await;
        // Index-only ingest (no file): scan must not remove it.
        kb.ingest("no-file-doc", "# Legacy\n\nOld row without a file.")
            .await
            .unwrap();
        let report = kb.scan().await.unwrap();
        assert_eq!(report.removed, 0);
        let (docs, _) = kb.stats().await.unwrap();
        assert_eq!(docs, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn rebuild_reindexes_everything() {
        let (kb, root) = test_kb("rebuild").await;
        kb.save("guide", &doc()).await.unwrap();
        kb.save("tea", "# Tea\n\nEarl grey with lemon.")
            .await
            .unwrap();
        // Simulate a corrupted index: drop the chunks of one document.
        let docs = kb.list_documents().await.unwrap();
        let guide = docs.iter().find(|d| d.name == "guide").unwrap();
        kb.delete_document(guide.id).await.unwrap();
        let (docs, _) = kb.stats().await.unwrap();
        assert_eq!(docs, 1);

        let report = kb.rebuild().await.unwrap();
        assert_eq!(report.indexed, 2, "both files reindexed: {report:?}");
        let (docs, chunks) = kb.stats().await.unwrap();
        assert_eq!(docs, 2);
        assert!(chunks >= 3);
        let hits = kb.search("release.sh", 5).await.unwrap();
        assert!(!hits.is_empty(), "rebuild restores searchability");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn chunk_edit_revisions_and_rollback() {
        let (kb, root) = test_kb("edit").await;
        kb.save("guide", &doc()).await.unwrap();
        let docs = kb.list_documents().await.unwrap();
        let chunks = kb.document_chunks(docs[0].id).await.unwrap();
        let target = chunks
            .iter()
            .find(|(_, c)| c.contains("release.sh"))
            .expect("deploy chunk");

        // Edit the chunk.
        let out = kb
            .edit_chunk(
                target.0,
                "The deploy script lives in scripts/deploy.sh now.",
            )
            .await
            .unwrap();
        assert!(out.revision > 0);
        // The file is the truth: it must carry the edit.
        let raw = kb.read_raw("guide").unwrap().unwrap();
        assert!(
            raw.contains("scripts/deploy.sh now."),
            "edit written through"
        );
        assert!(!raw.contains("scripts/release.sh"));
        // Search sees the new text.
        let hits = kb.search("deploy.sh now", 5).await.unwrap();
        assert!(!hits.is_empty());

        // Revision history.
        let revs = kb.chunk_revisions(target.0).await.unwrap();
        assert_eq!(revs.len(), 1);
        assert!(revs[0].old_content.contains("release.sh"));

        // Rollback restores the original text and is itself recorded.
        let out = kb.rollback_revision(revs[0].id).await.unwrap();
        assert!(out.revision > revs[0].id, "rollback appends history");
        let raw = kb.read_raw("guide").unwrap().unwrap();
        assert!(
            raw.contains("scripts/release.sh"),
            "rollback restores the file"
        );
        let hits = kb.search("release.sh", 5).await.unwrap();
        assert!(!hits.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn chunk_edit_on_legacy_document_is_in_place() {
        let (kb, root) = test_kb("legacy-edit").await;
        kb.ingest("legacy", "# Legacy\n\nThe old pipeline used Jenkins.")
            .await
            .unwrap();
        let docs = kb.list_documents().await.unwrap();
        let chunks = kb.document_chunks(docs[0].id).await.unwrap();
        let out = kb
            .edit_chunk(chunks[0].0, "# Legacy\n\nThe old pipeline used ArgoCD.")
            .await
            .unwrap();
        assert!(out.revision > 0);
        let hits = kb.search("ArgoCD", 5).await.unwrap();
        assert!(!hits.is_empty(), "in-place edit is searchable");
        let revs = kb.chunk_revisions(chunks[0].0).await.unwrap();
        assert_eq!(revs.len(), 1);
        let _ = kb.rollback_revision(revs[0].id).await.unwrap();
        let hits = kb.search("Jenkins", 5).await.unwrap();
        assert!(!hits.is_empty(), "rollback restores content");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn expand_returns_parent_section() {
        let (kb, root) = test_kb("expand").await;
        kb.save("guide", &doc()).await.unwrap();
        let hits = kb.search("release.sh", 5).await.unwrap();
        let hit = &hits[0];
        let expansion = kb.expand(hit.chunk_id).await.unwrap();
        assert_eq!(expansion.document, "guide");
        assert!(expansion.file.as_deref().is_some_and(|f| f == "guide.md"));
        // The parent section carries the heading the fragment lacks.
        assert!(expansion.section.starts_with("# Deploy guide"));
        assert!(expansion.section.contains("repository root after merging"));
        assert!(expansion.chunk.contains("release.sh"));
        // parents_for: same answer in batch form.
        let parents = kb.parents_for(&[hit.chunk_id]).await;
        assert_eq!(
            parents.get(&hit.chunk_id).map(String::as_str),
            Some(expansion.section.as_str())
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
