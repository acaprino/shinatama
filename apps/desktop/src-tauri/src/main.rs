#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use shinatama_projects::{ProjectEntry, ProjectStore};
use shinatama_terminal::{OutputBatch, SessionInfo, TerminalHost};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};
type Host = Arc<TerminalHost>;
struct Projects(Result<Arc<ProjectStore>, String>);

#[tauri::command]
async fn project_recent(
    window: tauri::WebviewWindow,
    projects: State<'_, Projects>,
) -> Result<Vec<ProjectEntry>, String> {
    local(&window)?;
    let store = projects.0.clone()?;
    tauri::async_runtime::spawn_blocking(move || store.recent())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn project_select(
    window: tauri::WebviewWindow,
    projects: State<'_, Projects>,
    path: String,
) -> Result<ProjectEntry, String> {
    local(&window)?;
    let store = projects.0.clone()?;
    tauri::async_runtime::spawn_blocking(move || store.select(&PathBuf::from(path)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}
fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Unknown workspace window".into())
    }
}
#[tauri::command]
fn project_path(window: tauri::WebviewWindow) -> Result<String, String> {
    local(&window)?;
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn terminal_start(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    cwd: String,
    rows: u16,
    cols: u16,
) -> Result<SessionInfo, String> {
    local(&window)?;
    let host = host.inner().clone();
    tauri::async_runtime::spawn_blocking(move || host.start(&PathBuf::from(cwd), rows, cols))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
fn terminal_list(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
) -> Result<Vec<SessionInfo>, String> {
    local(&window)?;
    Ok(host.list())
}
#[tauri::command]
fn terminal_read(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    id: String,
) -> Result<OutputBatch, String> {
    local(&window)?;
    host.read(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn terminal_write(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    local(&window)?;
    host.write(&id, data).map_err(|e| e.to_string())
}
#[tauri::command]
async fn terminal_resize(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    local(&window)?;
    let host = host.inner().clone();
    tauri::async_runtime::spawn_blocking(move || host.resize(&id, rows, cols))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn terminal_stop(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    id: String,
) -> Result<(), String> {
    local(&window)?;
    let host = host.inner().clone();
    tauri::async_runtime::spawn_blocking(move || host.stop(&id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn terminal_shutdown(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
) -> Result<(), String> {
    local(&window)?;
    let host = host.inner().clone();
    tauri::async_runtime::spawn_blocking(move || host.shutdown())
        .await
        .map_err(|e| e.to_string())
}
fn main() {
    tauri::Builder::default()
        .manage(Arc::new(TerminalHost::default()))
        .setup(|app| {
            let directory = std::env::var_os("SHINATAMA_HOME")
                .map(PathBuf::from)
                .map(Ok)
                .unwrap_or_else(|| app.path().app_config_dir());
            let store = directory.map_err(|e| e.to_string()).and_then(|directory| {
                if !directory.is_absolute() {
                    return Err("SHINATAMA_HOME must be an absolute folder path".into());
                }
                ProjectStore::open(&directory)
                    .map(Arc::new)
                    .map_err(|e| format!("{e:#}"))
            });
            app.manage(Projects(store));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project_path,
            project_recent,
            project_select,
            terminal_start,
            terminal_list,
            terminal_read,
            terminal_write,
            terminal_resize,
            terminal_stop,
            terminal_shutdown
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && !window.state::<Host>().list().is_empty()
            {
                api.prevent_close();
                let _ = window.emit("confirm-close", ());
            }
        })
        .run(tauri::generate_context!())
        .expect("Cannot run Shinatama desktop");
}
