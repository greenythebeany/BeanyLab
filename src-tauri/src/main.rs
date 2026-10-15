// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use beany_core::Session;
use tauri::{Emitter, Manager, State};

struct AppState {
    session: Arc<Mutex<Session>>,
    interrupt: Arc<AtomicBool>,
}

/// Every engine request goes through one command; long evaluations run off the UI thread.
#[tauri::command]
async fn api(
    state: State<'_, AppState>,
    method: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let session = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = session
            .lock()
            .map_err(|_| "engine is unavailable".to_string())?;
        s.call(&method, &args)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn interrupt(state: State<'_, AppState>) {
    state.interrupt.store(true, Ordering::Relaxed);
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let mut session = Session::new();
            // Start in the user's Documents/BeanyLab folder, like a default work folder.
            if let Ok(docs) = app.path().document_dir() {
                let work = docs.join("BeanyLab");
                let _ = std::fs::create_dir_all(&work);
                let _ = session.set_cwd(&work.to_string_lossy());
            }
            // Stream output and figure updates to the UI while code runs.
            let h = app.handle().clone();
            session.interp.stream = Some(Box::new(move |s: &str| {
                let _ = h.emit("beany-output", s.to_string());
            }));
            let h = app.handle().clone();
            session.interp.figure_stream = Some(Box::new(move |figs, closed| {
                let _ = h.emit(
                    "beany-figures",
                    serde_json::json!({ "figures": figs, "closed": closed }),
                );
            }));
            let interrupt = session.interp.interrupt.clone();
            app.manage(AppState {
                session: Arc::new(Mutex::new(session)),
                interrupt,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api, interrupt])
        .run(tauri::generate_context!())
        .expect("error while running BeanyLab");
}
