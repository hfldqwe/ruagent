//! Read path: current memories by store/namespace + FTS search over them.

use ruagent_store::Db;

use crate::{MemoryRow, MemoryStore};

pub use ruagent_store::DbError;

fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRow> {
    let store: String = row.get("store")?;
    let store = match store.as_str() {
        "profile" => MemoryStore::Profile,
        "procedure" => MemoryStore::Procedure,
        "lesson" => MemoryStore::Lesson,
        _ => MemoryStore::Observation,
    };
    Ok(MemoryRow {
        id: row.get("id")?,
        store,
        namespace: row.get("namespace")?,
        content: row.get("content")?,
        confidence: row.get("confidence")?,
        supersedes: row.get("supersedes")?,
        superseded_at: row.get("superseded_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// Current (non-superseded) memories for one store in one namespace,
/// newest first.
pub async fn current_memories(
    db: &Db,
    store: MemoryStore,
    namespace: &str,
    limit: u32,
) -> Result<Vec<MemoryRow>, DbError> {
    let store = store.as_str();
    let namespace = namespace.to_string();
    db.call(move |conn| -> Result<Vec<MemoryRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, store, namespace, content, confidence, supersedes, superseded_at,
                    created_at, updated_at
             FROM memories
             WHERE store = ?1 AND namespace = ?2 AND superseded_at IS NULL
             ORDER BY updated_at DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![store, namespace, limit], row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// Full-text search over current memories, all namespaces (the hybrid
/// retrieval's keyword leg; vectors land in the knowledge crate).
pub async fn search_fts(db: &Db, query: &str, limit: u32) -> Result<Vec<MemoryRow>, DbError> {
    let query = query.to_string();
    db.call(move |conn| -> Result<Vec<MemoryRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT m.id, m.store, m.namespace, m.content, m.confidence, m.supersedes,
                    m.superseded_at, m.created_at, m.updated_at
             FROM memories_fts f
             JOIN memories m ON m.id = f.rowid
             WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![query, limit], row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// One memory by id (any state, superseded included).
pub async fn get_memory(db: &Db, id: i64) -> Result<Option<MemoryRow>, DbError> {
    db.call(move |conn| -> Result<Option<MemoryRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, store, namespace, content, confidence, supersedes, superseded_at,
                    created_at, updated_at
             FROM memories WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_memory(row)?)),
            None => Ok(None),
        }
    })
    .await?
    .map_err(DbError::from)
}

/// The audit log, newest first (design SS6.3: every write decision).
#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryDiff {
    pub id: i64,
    pub ts: String,
    pub op: String,
    pub mem_store: Option<String>,
    pub namespace: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub reason: Option<String>,
}

pub async fn list_diffs(db: &Db, limit: u32) -> Result<Vec<MemoryDiff>, DbError> {
    db.call(move |conn| -> Result<Vec<MemoryDiff>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, ts, op, mem_store, namespace, before, after, reason
             FROM memory_diffs ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit], |row| {
                Ok(MemoryDiff {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    op: row.get(2)?,
                    mem_store: row.get(3)?,
                    namespace: row.get(4)?,
                    before: row.get(5)?,
                    after: row.get(6)?,
                    reason: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await?
    .map_err(DbError::from)
}

/// The `ruagent://` root listing: per-store counts of current memories.
pub async fn store_counts(db: &Db) -> Result<Vec<(String, String, i64)>, DbError> {
    db.call(
        |conn| -> Result<Vec<(String, String, i64)>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT store, namespace, COUNT(*) FROM memories
             WHERE superseded_at IS NULL GROUP BY store, namespace ORDER BY store, namespace",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        },
    )
    .await?
    .map_err(DbError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::Namespace;
    use crate::write::{MemoryWrite, WriteOutcome, write_memory};

    #[tokio::test]
    async fn search_finds_current_only() {
        let db = Db::open_in_memory().unwrap();
        let w = |content: &str| MemoryWrite {
            store: MemoryStore::Observation,
            namespace: Namespace::parse("project:demo").unwrap(),
            content: content.into(),
            confidence: 0.9,
            source_episode: None,
            supersedes: None,
        };
        let first = write_memory(&db, &w("the deploy script lives in scripts/deploy.sh"))
            .await
            .unwrap();
        let WriteOutcome::Inserted(old) = first else {
            panic!()
        };
        let mut updated = w("the deploy script lives in scripts/release.sh");
        updated.supersedes = Some(old);
        write_memory(&db, &updated).await.unwrap();
        write_memory(&db, &w("unrelated note about tea"))
            .await
            .unwrap();

        let hits = search_fts(&db, "deploy", 10).await.unwrap();
        assert_eq!(hits.len(), 1, "superseded memory must not match");
        assert!(hits[0].content.contains("release.sh"));

        let current = current_memories(&db, MemoryStore::Observation, "project:demo", 10)
            .await
            .unwrap();
        assert_eq!(current.len(), 2);

        let counts = store_counts(&db).await.unwrap();
        assert_eq!(
            counts,
            vec![("observation".into(), "project:demo".into(), 2)]
        );
    }
}
