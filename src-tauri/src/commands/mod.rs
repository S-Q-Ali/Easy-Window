use std::sync::Mutex;

use rusqlite::Connection;
use tauri::State;

use crate::store::audit::{self, AuditEvent};
use crate::store::settings;

#[cfg(windows)]
pub mod processes;

/// Shared handle to the local store. One connection, guarded by a mutex.
pub struct AppState {
    pub db: Mutex<Connection>,
}

#[tauri::command]
pub fn list_audit_events(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<AuditEvent>, String> {
    let db = state
        .db
        .lock()
        .map_err(|_| "store lock poisoned".to_string())?;
    audit::list(&db, limit).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    let db = state
        .db
        .lock()
        .map_err(|_| "store lock poisoned".to_string())?;
    settings::get(&db, &key).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    let db = state
        .db
        .lock()
        .map_err(|_| "store lock poisoned".to_string())?;
    settings::set(&db, &key, &value).map_err(|error| error.to_string())
}
