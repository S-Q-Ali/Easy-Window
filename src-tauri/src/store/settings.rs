use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::db::{Result, StoreError};

const KEY_MAX: usize = 100;
const VALUE_MAX: usize = 4096;

#[derive(Debug, Clone, Serialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: i64,
}

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    if key.trim().is_empty() || key.chars().count() > KEY_MAX {
        return Err(StoreError::InvalidKey);
    }
    if value.chars().count() > VALUE_MAX {
        return Err(StoreError::InvalidValue);
    }

    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE
             SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now_unix()],
    )?;

    Ok(())
}

pub fn list(conn: &Connection) -> Result<Vec<Setting>> {
    let mut stmt = conn.prepare("SELECT key, value, updated_at FROM settings ORDER BY key ASC")?;

    let rows = stmt.query_map([], |row| {
        Ok(Setting {
            key: row.get(0)?,
            value: row.get(1)?,
            updated_at: row.get(2)?,
        })
    })?;

    let mut settings = Vec::new();
    for row in rows {
        settings.push(row?);
    }

    Ok(settings)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::db::open_in_memory;

    #[test]
    fn a_missing_key_reads_as_none() {
        let conn = open_in_memory().unwrap();

        assert_eq!(get(&conn, "idle_minutes").unwrap(), None);
    }

    #[test]
    fn set_then_get_returns_the_value() {
        let conn = open_in_memory().unwrap();

        set(&conn, "idle_minutes", "30").unwrap();

        assert_eq!(get(&conn, "idle_minutes").unwrap().as_deref(), Some("30"));
    }

    #[test]
    fn setting_the_same_key_twice_overwrites_it() {
        let conn = open_in_memory().unwrap();

        set(&conn, "idle_minutes", "30").unwrap();
        set(&conn, "idle_minutes", "15").unwrap();

        assert_eq!(get(&conn, "idle_minutes").unwrap().as_deref(), Some("15"));
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn list_returns_keys_in_a_stable_order() {
        let conn = open_in_memory().unwrap();

        set(&conn, "theme", "dark").unwrap();
        set(&conn, "idle_minutes", "30").unwrap();

        let keys: Vec<String> = list(&conn).unwrap().into_iter().map(|s| s.key).collect();

        assert_eq!(keys, vec!["idle_minutes", "theme"]);
    }

    #[test]
    fn an_empty_key_is_rejected() {
        let conn = open_in_memory().unwrap();

        assert!(matches!(
            set(&conn, "   ", "x"),
            Err(StoreError::InvalidKey)
        ));
    }

    #[test]
    fn an_oversized_key_is_rejected() {
        let conn = open_in_memory().unwrap();

        assert!(matches!(
            set(&conn, &"k".repeat(KEY_MAX + 1), "x"),
            Err(StoreError::InvalidKey)
        ));
    }

    #[test]
    fn an_oversized_value_is_rejected() {
        let conn = open_in_memory().unwrap();

        assert!(matches!(
            set(&conn, "big", &"v".repeat(VALUE_MAX + 1)),
            Err(StoreError::InvalidValue)
        ));
    }
}
