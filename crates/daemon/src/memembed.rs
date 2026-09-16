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
                      WHERE embedding IS NOT NULL AND superseded_at IS NULL",
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
                  WHERE embedding IS NULL OR embedder IS NULL OR embedder != ?1",
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
