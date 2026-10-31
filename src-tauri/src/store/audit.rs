use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::db::{Result, StoreError};

/// Which part of the app performed the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditModule {
    Sessions,
    Processes,
    Startup,
    App,
}

impl AuditModule {
    const fn as_db(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Processes => "processes",
            Self::Startup => "startup",
            Self::App => "app",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        match value {
            "sessions" => Some(Self::Sessions),
            "processes" => Some(Self::Processes),
            "startup" => Some(Self::Startup),
            "app" => Some(Self::App),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    Partial,
    Failed,
    Blocked,
}

impl AuditOutcome {
    const fn as_db(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        match value {
            "success" => Some(Self::Success),
            "partial" => Some(Self::Partial),
            "failed" => Some(Self::Failed),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }
}

/// What the caller supplies. The timestamp is assigned by the store.
#[derive(Debug, Clone)]
pub struct NewAuditEvent {
    pub module: AuditModule,
    pub action: String,
    pub target: String,
    pub outcome: AuditOutcome,
    pub detail: Option<String>,
    pub reversible: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub id: i64,
    pub occurred_at: i64,
    pub module: AuditModule,
    pub action: String,
    pub target: String,
    pub outcome: AuditOutcome,
    pub detail: Option<String>,
    pub reversible: bool,
}

/// Field caps. The audit log is for humans, not a data dump.
const ACTION_MAX: usize = 64;
const TARGET_MAX: usize = 200;
const DETAIL_MAX: usize = 280;

pub fn record(conn: &Connection, event: &NewAuditEvent) -> Result<i64> {
    conn.execute(
        "INSERT INTO audit_events
             (occurred_at, module, action, target, outcome, detail, reversible)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            now_unix(),
            event.module.as_db(),
            sanitize(&event.action, ACTION_MAX),
            sanitize(&event.target, TARGET_MAX),
            event.outcome.as_db(),
            event.detail.as_deref().map(|d| sanitize(d, DETAIL_MAX)),
            i64::from(event.reversible),
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn list(conn: &Connection, limit: u32) -> Result<Vec<AuditEvent>> {
    let limit = limit.clamp(1, 500);
    let mut stmt = conn.prepare(
        "SELECT id, occurred_at, module, action, target, outcome, detail, reversible
         FROM audit_events
         ORDER BY occurred_at DESC, id DESC
         LIMIT ?1",
    )?;

    let rows = stmt.query_map([limit], |row| {
        Ok(RawRow {
            id: row.get(0)?,
            occurred_at: row.get(1)?,
            module: row.get(2)?,
            action: row.get(3)?,
            target: row.get(4)?,
            outcome: row.get(5)?,
            detail: row.get(6)?,
            reversible: row.get(7)?,
        })
    })?;

    let mut events = Vec::new();
    for row in rows {
        let raw = row?;
        events.push(AuditEvent {
            id: raw.id,
            occurred_at: raw.occurred_at,
            module: AuditModule::from_db(&raw.module)
                .ok_or_else(|| StoreError::UnknownModule(raw.module.clone()))?,
            action: raw.action,
            target: raw.target,
            outcome: AuditOutcome::from_db(&raw.outcome)
                .ok_or_else(|| StoreError::UnknownOutcome(raw.outcome.clone()))?,
            detail: raw.detail,
            reversible: raw.reversible != 0,
        });
    }

    Ok(events)
}

struct RawRow {
    id: i64,
    occurred_at: i64,
    module: String,
    action: String,
    target: String,
    outcome: String,
    detail: Option<String>,
    reversible: i64,
}

/// Strips control characters (newlines, escapes) and caps the length so a
/// hostile process name or domain cannot forge log lines or bloat the file.
fn sanitize(input: &str, max_chars: usize) -> String {
    input
        .chars()
        .filter(|c| !c.is_control())
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
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

    fn sample() -> NewAuditEvent {
        NewAuditEvent {
            module: AuditModule::Sessions,
            action: "clear_site_data".into(),
            target: "example.com".into(),
            outcome: AuditOutcome::Success,
            detail: Some("cleared 12 cookies".into()),
            reversible: false,
        }
    }

    #[test]
    fn record_then_list_round_trips_every_field() {
        let conn = open_in_memory().unwrap();
        record(&conn, &sample()).unwrap();

        let events = list(&conn, 10).unwrap();

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.module, AuditModule::Sessions);
        assert_eq!(event.action, "clear_site_data");
        assert_eq!(event.target, "example.com");
        assert_eq!(event.outcome, AuditOutcome::Success);
        assert_eq!(event.detail.as_deref(), Some("cleared 12 cookies"));
        assert!(!event.reversible);
        assert!(event.occurred_at > 0);
    }

    #[test]
    fn list_returns_the_newest_event_first() {
        let conn = open_in_memory().unwrap();
        record(&conn, &sample()).unwrap();
        record(
            &conn,
            &NewAuditEvent {
                target: "second.example".into(),
                ..sample()
            },
        )
        .unwrap();

        let events = list(&conn, 10).unwrap();

        assert_eq!(events[0].target, "second.example");
        assert_eq!(events[1].target, "example.com");
    }

    #[test]
    fn list_respects_the_limit() {
        let conn = open_in_memory().unwrap();
        for index in 0..5 {
            record(
                &conn,
                &NewAuditEvent {
                    target: format!("site{index}.example"),
                    ..sample()
                },
            )
            .unwrap();
        }

        assert_eq!(list(&conn, 2).unwrap().len(), 2);
    }

    #[test]
    fn list_caps_an_absurd_limit_instead_of_trusting_the_caller() {
        let conn = open_in_memory().unwrap();
        record(&conn, &sample()).unwrap();

        // The clamp is 500; asking for u32::MAX must not allocate a huge query.
        assert_eq!(list(&conn, u32::MAX).unwrap().len(), 1);
    }

    #[test]
    fn control_characters_are_stripped_from_stored_text() {
        let conn = open_in_memory().unwrap();
        record(
            &conn,
            &NewAuditEvent {
                action: "close\nprocess".into(),
                target: "evil\r\nsite.example".into(),
                detail: Some("line one\nline two\t".into()),
                ..sample()
            },
        )
        .unwrap();

        let event = &list(&conn, 1).unwrap()[0];

        assert_eq!(event.action, "closeprocess");
        assert_eq!(event.target, "evilsite.example");
        assert_eq!(event.detail.as_deref(), Some("line oneline two"));
    }

    #[test]
    fn oversized_text_is_truncated_to_the_field_cap() {
        let conn = open_in_memory().unwrap();
        record(
            &conn,
            &NewAuditEvent {
                action: "a".repeat(500),
                target: "t".repeat(500),
                detail: Some("d".repeat(500)),
                ..sample()
            },
        )
        .unwrap();

        let event = &list(&conn, 1).unwrap()[0];

        assert_eq!(event.action.chars().count(), ACTION_MAX);
        assert_eq!(event.target.chars().count(), TARGET_MAX);
        assert_eq!(event.detail.as_deref().unwrap().chars().count(), DETAIL_MAX);
    }

    #[test]
    fn reversible_flag_persists() {
        let conn = open_in_memory().unwrap();
        record(
            &conn,
            &NewAuditEvent {
                module: AuditModule::Startup,
                action: "disable_startup_item".into(),
                reversible: true,
                ..sample()
            },
        )
        .unwrap();

        assert!(list(&conn, 1).unwrap()[0].reversible);
    }
}
