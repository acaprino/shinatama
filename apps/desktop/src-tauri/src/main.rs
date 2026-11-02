#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod storage;
#[cfg(feature = "tl1-proof")]
mod tl1;
use shinatama_projects::ProjectEntry;
use shinatama_terminal::{OutputBatch, SessionInfo, TerminalHost};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};
type Host = Arc<TerminalHost>;
type Services = Arc<storage::Services>;

#[derive(serde::Serialize)]
struct StorageDiagnostics {
    directory: String,
    database_path: String,
}

#[tauri::command]
async fn storage_location(
    window: tauri::WebviewWindow,
    services: State<'_, Services>,
) -> Result<StorageDiagnostics, String> {
    local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let location = &services.load()?.location;
        Ok(StorageDiagnostics {
            directory: location.directory().to_string_lossy().into_owned(),
            database_path: location.database_path().to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn project_recent(
    window: tauri::WebviewWindow,
    services: State<'_, Services>,
) -> Result<Vec<ProjectEntry>, String> {
    local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        services
            .load()?
            .store
            .recent()
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn project_select(
    window: tauri::WebviewWindow,
    services: State<'_, Services>,
    path: String,
) -> Result<ProjectEntry, String> {
    local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = services.load()?;
        state
            .location
            .validate_project(&PathBuf::from(&path))
            .map_err(|e| e.to_string())?;
        state
            .store
            .select(&PathBuf::from(path))
            .map_err(|e| e.to_string())
    })
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
    services: State<'_, Services>,
    cwd: String,
    rows: u16,
    cols: u16,
) -> Result<SessionInfo, String> {
    local(&window)?;
    let host = host.inner().clone();
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        services
            .load()?
            .location
            .validate_project(&PathBuf::from(&cwd))
            .map_err(|e| e.to_string())?;
        host.start(&PathBuf::from(cwd), rows, cols)
            .map_err(|e| format!("{e:#}"))
    })
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
    let builder = tauri::Builder::default()
        .manage(Arc::new(TerminalHost::default()))
        .setup(|app| {
            let directory = std::env::var_os("SHINATAMA_HOME")
                .map(PathBuf::from)
                .map(Ok)
                .unwrap_or_else(|| app.path().app_config_dir());
            app.manage(Arc::new(storage::Services::new(
                directory.map_err(|e| e.to_string()),
            )));
            Ok(())
        });
    #[cfg(not(feature = "tl1-proof"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        storage_location,
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
    ]);
    #[cfg(feature = "tl1-proof")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        storage_location,
        project_path,
        project_recent,
        project_select,
        terminal_start,
        terminal_list,
        terminal_read,
        terminal_write,
        terminal_resize,
        terminal_stop,
        terminal_shutdown,
        tl1::tl1_files,
        tl1::tl1_read
    ]);
    builder
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
