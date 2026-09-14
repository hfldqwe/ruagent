//! Schema migrations: embedded SQL applied in order, tracked in
//! `schema_migrations`. Idempotent and safe to run on every boot.

use crate::sqlite::DbError;

/// Embedded migration scripts, in order. The index in the array is the
/// schema version.
pub const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_agents.sql"),
    include_str!("migrations/0003_m2_results.sql"),
    include_str!("migrations/0004_memory.sql"),
    include_str!("migrations/0005_graph.sql"),
    include_str!("migrations/0006_knowledge.sql"),
    include_str!("migrations/0007_sessions.sql"),
    include_str!("migrations/0008_memory_embeddings.sql"),
    include_str!("migrations/0009_knowledge_files.sql"),
];

/// Apply all pending migrations.
pub fn apply(conn: &mut rusqlite::Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL
         );",
    )?;
    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;

    for (i, script) in MIGRATIONS.iter().enumerate() {
        let version = (i + 1) as i64;
        if version <= current {
            continue;
        }
        conn.execute_batch(script)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, chrono::Utc::now().to_rfc3339()],
        )?;
        tracing::info!(version, "applied schema migration");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_idempotently() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap(); // second run must be a no-op
        let v: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
        // The M1 tables exist.
        for table in ["tasks", "task_edges", "runs"] {
            let n: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {table} missing");
        }
    }
}
