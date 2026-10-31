use rusqlite::Connection;
use thiserror::Error;

const MIGRATIONS: &[&str] = &[include_str!("../../migrations/0001_init.sql")];

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not prepare the data folder: {0}")]
    Io(#[from] std::io::Error),
    #[error("audit row has an unknown module: {0}")]
    UnknownModule(String),
    #[error("audit row has an unknown outcome: {0}")]
    UnknownOutcome(String),
    #[error("setting key is empty or too long")]
    InvalidKey,
    #[error("setting value is too long")]
    InvalidValue,
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Opens the on-disk database, creating the parent folder when needed.
pub fn open(path: &std::path::Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    prepare(Connection::open(path)?)
}

/// In-memory database for tests.
pub fn open_in_memory() -> Result<Connection> {
    prepare(Connection::open_in_memory()?)
}

fn prepare(conn: Connection) -> Result<Connection> {
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    let applied: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    for (index, sql) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
        conn.execute_batch(sql)?;
        conn.pragma_update(None, "user_version", (index + 1) as i64)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    }

    #[test]
    fn migration_creates_the_settings_and_audit_tables() {
        let conn = open_in_memory().expect("in-memory store should open");

        assert!(table_exists(&conn, "settings"));
        assert!(table_exists(&conn, "audit_events"));
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = open_in_memory().expect("in-memory store should open");

        migrate(&conn).expect("re-running migrations should be a no-op");

        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user_version should be readable");
        assert_eq!(version, MIGRATIONS.len() as i64);
    }
}
