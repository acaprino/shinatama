#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod launch;
pub mod maintenance;
mod new_project;
mod projects;
mod storage;
#[cfg(feature = "tl1-proof")]
mod tl1;
use shinatama_state::database::{DatabaseAccess, ProjectRecord as ProjectEntry};
use shinatama_terminal::{OutputBatch, SessionInfo, TerminalHost};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};
type Host = Arc<TerminalHost>;
type Services = Arc<storage::Services>;
type Maintenance = Arc<maintenance::Controller>;

#[tauri::command]
fn maintenance_activity(
    window: tauri::WebviewWindow,
    maintenance: State<'_, Maintenance>,
) -> Result<(), String> {
    local(&window)?;
    maintenance.activity();
    Ok(())
}

#[tauri::command]
fn maintenance_status(
    window: tauri::WebviewWindow,
    maintenance: State<'_, Maintenance>,
) -> Result<maintenance::Status, String> {
    local(&window)?;
    maintenance.status()
}

#[tauri::command]
async fn maintenance_check(
    window: tauri::WebviewWindow,
    maintenance: State<'_, Maintenance>,
) -> Result<maintenance::Status, String> {
    local(&window)?;
    let maintenance = maintenance.inner().clone();
    let Some(check) = maintenance.try_check() else {
        return maintenance.status();
    };
    tauri::async_runtime::spawn_blocking(move || check.execute())
        .await
        .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
struct StorageDiagnostics {
    directory: String,
    database_path: String,
    read_only: bool,
    message: Option<String>,
}

#[tauri::command]
async fn storage_location(
    window: tauri::WebviewWindow,
    services: State<'_, Services>,
) -> Result<StorageDiagnostics, String> {
    local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = services.load()?;
        let location = &state.location;
        let (read_only, message) = match state.store.access() {
            DatabaseAccess::ReadWrite => (false, None),
            DatabaseAccess::ReadOnlyNewer { found, supported } => (true, Some(format!("Database versione {found}, più recente di quella supportata ({supported}). Aperto in sola lettura; aggiorna Shinatama per modificarlo."))),
        };
        Ok(StorageDiagnostics {
            directory: location.directory().to_string_lossy().into_owned(),
            database_path: location.database_path().to_string_lossy().into_owned(),
            read_only,
            message,
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
    catalog: State<'_, Arc<projects::ProjectCatalog>>,
    path: String,
    request_id: Option<String>,
) -> Result<ProjectEntry, String> {
    local(&window)?;
    let services = services.inner().clone();
    let catalog = catalog.inner().clone();
    let reservation = catalog.reserve(request_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = services.load()?;
        let selected = state
            .store
            .project_manager_select_if_not_cancelled(&PathBuf::from(path), reservation.token())
            .map_err(|e| e.to_string())?;
        // The selection is committed. A cache refresh must not turn that success
        // into a failed-save response if an unrelated read becomes unavailable.
        if let Ok(config) = state.store.project_manager_read() {
            catalog.engine.configure(config);
        }
        Ok::<ProjectEntry, String>(selected)
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
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(TerminalHost::default()))
        .manage(shinatama_engine::Engine::new())
        .setup(|app| {
            let directory = std::env::var_os("SHINATAMA_HOME")
                .map(PathBuf::from)
                .map(Ok)
                .unwrap_or_else(|| app.path().app_config_dir());
            let services = Arc::new(storage::Services::new(directory.map_err(|e| e.to_string())));
            app.manage(Arc::new(maintenance::Controller::start(services.clone())));
            app.manage(Arc::new(projects::ProjectCatalog::new(services.clone())));
            app.manage(services);
            Ok(())
        });
    #[cfg(not(feature = "tl1-proof"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        launch::launch_preflight,
        storage_location,
        maintenance_activity,
        maintenance_status,
        maintenance_check,
        project_path,
        project_recent,
        project_select,
        projects::catalog_begin,
        projects::catalog_cancel,
        projects::catalog_read,
        projects::catalog_roots_set,
        projects::catalog_browse,
        new_project::new_project_options,
        new_project::new_project_create,
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
        launch::launch_preflight,
        storage_location,
        maintenance_activity,
        maintenance_status,
        maintenance_check,
        project_path,
        project_recent,
        project_select,
        projects::catalog_begin,
        projects::catalog_cancel,
        projects::catalog_read,
        projects::catalog_roots_set,
        projects::catalog_browse,
        new_project::new_project_options,
        new_project::new_project_create,
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
