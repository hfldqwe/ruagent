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
        if let Ok(rows) = ruagent_memory::query::current_memories(db, g.store, &ns, g.limit).await {
            out.extend(rows.into_iter().map(|m| SelectedMemory {
                id: m.id,
                tag: g.tag,
                content: m.content,
                updated_at: m.updated_at,
                score: None,
            }));
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
    out
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
}
