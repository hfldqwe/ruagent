//! SQLite access: WAL mode + one dedicated writer thread.
//!
//! Design §10 / D7: a single connection owned by one OS thread; every call
//! funnels through an unbounded channel. WAL lets future read-only
//! connections bypass the writer if profiling ever demands it — until
//! then, serialization through one actor keeps lock contention at zero.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database writer is shut down")]
    Closed,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

type Op = Box<dyn FnOnce(&mut rusqlite::Connection) + Send + 'static>;

/// Handle to the single-writer SQLite actor. Cheap to clone.
#[derive(Clone)]
pub struct Db {
    tx: tokio::sync::mpsc::UnboundedSender<Op>,
}

impl Db {
    /// Open (or create) the database at `path`, apply migrations, and
    /// start the writer thread.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open(path)?;
        Self::configure_and_spawn(conn)
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::configure_and_spawn(conn)
    }

    fn configure_and_spawn(mut conn: rusqlite::Connection) -> Result<Self, DbError> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        crate::migrations::apply(&mut conn)?;
        Self::spawn_thread(conn)
    }

    fn spawn_thread(mut conn: rusqlite::Connection) -> Result<Self, DbError> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Op>();
        std::thread::Builder::new()
            .name("ruagent-db-writer".into())
            .spawn(move || {
                while let Some(op) = rx.blocking_recv() {
                    op(&mut conn);
                }
                // All handles dropped: best-effort checkpoint before exit.
                let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
            })?;
        Ok(Self { tx })
    }

    /// Run a closure with the connection on the writer thread and await
    /// its result.
    pub async fn call<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&mut rusqlite::Connection) -> T + Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Box::new(move |conn| {
                let _ = tx.send(f(conn));
            }))
            .map_err(|_| DbError::Closed)?;
        rx.await.map_err(|_| DbError::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruagent_core::{Task, TaskCreator};

    #[tokio::test]
    async fn concurrent_writes_all_land() {
        // 32 concurrent inserters through the single writer: all succeed,
        // no lock errors (the whole point of D7).
        let db = Db::open_in_memory().unwrap();
        let mut handles = Vec::new();
        for i in 0..32 {
            let db = db.clone();
            handles.push(tokio::spawn(async move {
                let t = Task::new(
                    format!("task {i}"),
                    format!("intent {i}"),
                    TaskCreator::Human,
                );
                db.insert_task(&t).await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        let all = db.list_tasks(None).await.unwrap();
        assert_eq!(all.len(), 32);
    }

    #[tokio::test]
    async fn open_applies_migrations() {
        let dir = std::env::temp_dir().join(format!("ruagent-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        {
            let db = Db::open(&path).unwrap();
            let t = Task::new("t", "i", TaskCreator::Human);
            db.insert_task(&t).await.unwrap();
            let got = db.get_task(t.id).await.unwrap();
            assert!(got.is_some());
        }
        // Reopen: migrations are idempotent, data survives.
        {
            let db = Db::open(&path).unwrap();
            let all = db.list_tasks(None).await.unwrap();
            assert_eq!(all.len(), 1);
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
