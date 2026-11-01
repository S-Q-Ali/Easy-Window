pub mod commands;
#[cfg(windows)]
pub mod monitor;
pub mod risk;
pub mod store;

use std::sync::Mutex;

use serde::Serialize;
use tauri::Manager;

use commands::AppState;

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
}

/// Minimal IPC round-trip so the frontend can prove it is talking to Rust.
#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().setup(|app| {
        let data_dir = app.path().app_local_data_dir()?;
        let connection = store::db::open(&data_dir.join("easy-window.db"))?;
        app.manage(AppState {
            db: Mutex::new(connection),
        });
        #[cfg(windows)]
        app.manage(commands::processes::Metrics::start(app.handle().clone()));
        Ok(())
    });

    // `generate_handler!` is a macro over a fixed list, so the commands that
    // exist only where the sampler exists are registered by their own chain.
    #[cfg(windows)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        app_info,
        commands::list_audit_events,
        commands::get_setting,
        commands::set_setting,
        commands::processes::latest_tick,
    ]);
    #[cfg(not(windows))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        app_info,
        commands::list_audit_events,
        commands::get_setting,
        commands::set_setting,
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
