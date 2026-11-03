#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod agents;
mod launch;
pub mod maintenance;
mod new_project;
mod presentation;
mod projects;
mod resource_governor;
mod resources;
#[cfg(windows)]
mod sessions;
#[cfg(windows)]
mod sidecar;
mod storage;
mod threads;
#[cfg(windows)]
mod terminal_runtime;
#[cfg(feature = "tl1-proof")]
mod tl1;
mod workspace;
use shinatama_state::database::{DatabaseAccess, ProjectRecord as ProjectEntry};
use shinatama_terminal::{OutputBatch, SessionInfo, TerminalHost};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager, State};
type Host = Arc<TerminalHost>;
type Services = Arc<storage::Services>;
type Maintenance = Arc<maintenance::Controller>;
#[derive(Default)]
struct ExitIntent(AtomicBool);

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
    if window.state::<ExitIntent>().0.load(Ordering::Acquire) {
        return Err("This window is closing; no new terminal was started.".into());
    }
    let host = host.inner().clone();
    let services = services.inner().clone();
    #[cfg(windows)]
    let runtime = window
        .state::<Arc<terminal_runtime::Runtime>>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        services
            .load()?
            .location
            .validate_project(&PathBuf::from(&cwd))
            .map_err(|e| e.to_string())?;
        #[cfg(windows)]
        runtime.configure(&host)?;
        #[cfg(windows)]
        runtime.wait_for_admission()?;
        host.start(&PathBuf::from(cwd), rows, cols)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadStarted {
    session: SessionInfo,
    thread_id: String,
}
#[tauri::command]
async fn shell_open(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    services: State<'_, Services>,
    cwd: String,
    rows: u16,
    cols: u16,
) -> Result<ThreadStarted, String> {
    local(&window)?;
    if window.state::<ExitIntent>().0.load(Ordering::Acquire) {
        return Err("This window is closing.".into());
    }
    let saved_services = services.inner().clone();
    let project_path = PathBuf::from(&cwd);
    let thread_id = tauri::async_runtime::spawn_blocking(move || {
        let state = saved_services.load().map_err(anyhow::Error::msg)?;
        state.location.validate_project(&project_path)?;
        let project = state.store.select(&project_path)?;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?;
        let timestamp = i64::try_from(timestamp.as_millis())?;
        let thread = state.store.create_thread(&shinatama_state::threads::ThreadRecord {
            id: shinatama_state::records::new_id(shinatama_state::records::CanonicalIdKind::Thread),
            project_id: project.id,
            kind: shinatama_state::records::ThreadKind::Work,
            parent_thread_id: None,
            companion_worker_thread_id: None,
            title: Some("PowerShell".into()),
            archived: false,
            revision: 1,
            created_at: timestamp,
            updated_at: timestamp,
        })?;
        Ok::<_, anyhow::Error>(thread.id)
    }).await.map_err(|_| "The thread could not be saved.".to_owned())?
        .map_err(|_| "The project or thread could not be saved. No session was started.".to_owned())?;
    // A failed start leaves the saved thread intact; closing a view never deletes it.
    let session = terminal_start(window, host, services, cwd, rows, cols).await?;
    Ok(ThreadStarted { session, thread_id })
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
fn terminal_resources(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
) -> Result<serde_json::Value, String> {
    local(&window)?;
    #[cfg(windows)]
    return serde_json::to_value(host.resources())
        .map_err(|_| "Terminal resource information is unavailable.".into());
    #[cfg(not(windows))]
    Ok(serde_json::json!([]))
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
    #[cfg(windows)]
    let sidecar = window.state::<Arc<sidecar::Controller>>().inner().clone();
    #[cfg(windows)]
    window
        .state::<Arc<terminal_runtime::Runtime>>()
        .request_shutdown();
    tauri::async_runtime::spawn_blocking(move || {
        host.shutdown();
        #[cfg(windows)]
        sidecar.stop_and_wait(std::time::Duration::from_secs(6));
    })
    .await
    .map_err(|e| e.to_string())?;
    // Explicit shutdown has already performed bounded cleanup. Unconfirmed jobs
    // retain reservations; they must not trap the user in a close-confirm loop.
    window
        .state::<ExitIntent>()
        .0
        .store(true, Ordering::Release);
    Ok(())
}
fn sidecar_active(window: &tauri::Window) -> bool {
    #[cfg(windows)]
    return window.state::<Arc<sidecar::Controller>>().is_active();
    #[cfg(not(windows))]
    false
}
#[tauri::command]
fn agent_status(window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    local(&window)?;
    #[cfg(windows)]
    return Ok(window.state::<Arc<agents::Controller>>().diagnostics());
    #[cfg(not(windows))]
    Ok(serde_json::json!({"checking":false,"blocked":false}))
}
#[tauri::command]
async fn agent_check(
    window: tauri::WebviewWindow,
    agent_id: String,
) -> Result<serde_json::Value, String> {
    local(&window)?;
    if window.state::<ExitIntent>().0.load(Ordering::Acquire) {
        return Err("This window is closing.".into());
    }
    #[cfg(windows)]
    {
        let owner = window.state::<Arc<agents::Controller>>().inner().clone();
        let checked = owner.check(&agent_id).await.map_err(str::to_owned)?;
        return serde_json::to_value(checked).map_err(|_| "Agent status is unavailable.".into());
    }
    #[cfg(not(windows))]
    Err("Agent checks are available on Windows.".into())
}
#[tauri::command]
async fn agent_start(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    agent_id: String,
    cwd: String,
    rows: u16,
    cols: u16,
) -> Result<serde_json::Value, String> {
    local(&window)?;
    if window.state::<ExitIntent>().0.load(Ordering::Acquire) {
        return Err("This window is closing.".into());
    }
    if cwd.len() > 4096 {
        return Err("The selected project path is invalid.".into());
    }
    #[cfg(windows)]
    {
        let owner = window.state::<Arc<agents::Controller>>().inner().clone();
        let started = owner
            .start(
                &agent_id,
                host.inner().clone(),
                PathBuf::from(cwd),
                rows,
                cols,
            )
            .await
            .map_err(str::to_owned)?;
        return serde_json::to_value(started)
            .map_err(|_| "Agent startup status is unavailable.".into());
    }
    #[cfg(not(windows))]
    Err("Agent startup is available on Windows.".into())
}
#[tauri::command]
async fn agent_open(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    agent_id: String,
    cwd: String,
    rows: u16,
    cols: u16,
) -> Result<serde_json::Value, String> {
    local(&window)?;
    if window.state::<ExitIntent>().0.load(Ordering::Acquire) {
        return Err("This window is closing.".into());
    }
    if cwd.len() > 4096 {
        return Err("The selected project path is invalid.".into());
    }
    #[cfg(windows)]
    {
        let owner = window.state::<Arc<agents::Controller>>().inner().clone();
        let started = owner
            .open(
                &agent_id,
                host.inner().clone(),
                PathBuf::from(cwd),
                rows,
                cols,
            )
            .await
            .map_err(str::to_owned)?;
        return serde_json::to_value(started)
            .map_err(|_| "Agent startup status is unavailable.".into());
    }
    #[cfg(not(windows))]
    Err("Agent startup is available on Windows.".into())
}
#[tauri::command]
fn sidecar_status(window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    local(&window)?;
    #[cfg(windows)]
    return serde_json::to_value(window.state::<Arc<sidecar::Controller>>().status())
        .map_err(|_| "Session index status is unavailable.".into());
    #[cfg(not(windows))]
    Err("The session index is available on Windows.".into())
}
#[tauri::command]
fn sidecar_start(window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    local(&window)?;
    if window.state::<ExitIntent>().0.load(Ordering::Acquire) {
        return Err("This window is closing.".into());
    }
    #[cfg(windows)]
    return window
        .state::<Arc<sidecar::Controller>>()
        .inner()
        .start()
        .map_err(str::to_owned)
        .and_then(|status| {
            serde_json::to_value(status).map_err(|_| "Session index status is unavailable.".into())
        });
    #[cfg(not(windows))]
    Err("The session index is available on Windows.".into())
}
#[tauri::command]
fn sidecar_stop(window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    local(&window)?;
    #[cfg(windows)]
    return window
        .state::<Arc<sidecar::Controller>>()
        .stop()
        .map_err(str::to_owned)
        .and_then(|status| {
            serde_json::to_value(status).map_err(|_| "Session index status is unavailable.".into())
        });
    #[cfg(not(windows))]
    Err("The session index is available on Windows.".into())
}
#[tauri::command]
fn sidecar_retry(window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    local(&window)?;
    if window.state::<ExitIntent>().0.load(Ordering::Acquire) {
        return Err("This window is closing.".into());
    }
    #[cfg(windows)]
    return window
        .state::<Arc<sidecar::Controller>>()
        .retry()
        .map_err(str::to_owned)
        .and_then(|status| {
            serde_json::to_value(status).map_err(|_| "Session index status is unavailable.".into())
        });
    #[cfg(not(windows))]
    Err("The session index is available on Windows.".into())
}
fn main() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(TerminalHost::default()))
        .manage(ExitIntent::default())
        .manage(shinatama_engine::Engine::new())
        .manage(resources::Resources::start())
        .setup(|app| {
            let directory = std::env::var_os("SHINATAMA_HOME")
                .map(PathBuf::from)
                .map(Ok)
                .unwrap_or_else(|| app.path().app_config_dir());
            let services = Arc::new(storage::Services::new(directory.map_err(|e| e.to_string())));
            let maintenance = Arc::new(maintenance::Controller::start(services.clone()));
            app.state::<resources::Resources>()
                .attach_governor(&maintenance);
            app.manage(maintenance);
            app.manage(Arc::new(projects::ProjectCatalog::new(services.clone())));
            app.manage(Arc::new(workspace::Controller::new(services.clone())));
            app.manage(Arc::new(presentation::Controller::new(services.clone())));
            app.manage(Arc::new(threads::Controller::new(services.clone())));
            #[cfg(windows)]
            {
                let (monitor, governor) = app.state::<resources::Resources>().admission_inputs();
                let runtime = Arc::new(terminal_runtime::Runtime::new(
                    services.clone(),
                    monitor,
                    governor,
                ));
                let sidecar = Arc::new(sidecar::Controller::new(services.clone(), runtime.clone()));
                app.manage(Arc::new(sessions::Controller::new(services.clone(), runtime.clone())));
                app.manage(Arc::new(agents::Controller::new(
                    services.clone(),
                    runtime.clone(),
                    sidecar.clone(),
                )));
                app.manage(sidecar);
                app.manage(runtime);
            }
            app.manage(services);
            Ok(())
        });
    #[cfg(not(feature = "tl1-proof"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        launch::launch_preflight,
        resources::resources_status,
        resources::resources_telemetry,
        workspace::workspace_load,
        workspace::workspace_save,
        workspace::workspace_release,
        workspace::workspace_recover,
        workspace::global_workspace_load,
        workspace::global_workspace_save,
        workspace::global_workspace_release,
        workspace::global_workspace_recover,
        workspace::workspace_allow_close,
        presentation::presentation_load,
        presentation::presentation_save,
        threads::thread_list,
        threads::thread_create,
        threads::thread_sidebar_load,
        threads::thread_sidebar_save,
        sidecar_status,
        sidecar_start,
        sidecar_stop,
        sidecar_retry,
        agent_check,
        agent_status,
        agent_start,
        agent_open,
        shell_open,
        #[cfg(windows)]
        sessions::session_roots,
        #[cfg(windows)]
        sessions::session_list,
        #[cfg(windows)]
        sessions::session_detail,
        #[cfg(windows)]
        sessions::session_archive_add,
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
        terminal_resources,
        terminal_read,
        terminal_write,
        terminal_resize,
        terminal_stop,
        terminal_shutdown
    ]);
    #[cfg(feature = "tl1-proof")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        launch::launch_preflight,
        resources::resources_status,
        resources::resources_telemetry,
        workspace::workspace_load,
        workspace::workspace_save,
        workspace::workspace_release,
        workspace::workspace_recover,
        workspace::global_workspace_load,
        workspace::global_workspace_save,
        workspace::global_workspace_release,
        workspace::global_workspace_recover,
        workspace::workspace_allow_close,
        presentation::presentation_load,
        presentation::presentation_save,
        threads::thread_list,
        threads::thread_create,
        threads::thread_sidebar_load,
        threads::thread_sidebar_save,
        sidecar_status,
        sidecar_start,
        sidecar_stop,
        sidecar_retry,
        agent_check,
        agent_status,
        agent_start,
        agent_open,
        shell_open,
        #[cfg(windows)]
        sessions::session_roots,
        #[cfg(windows)]
        sessions::session_list,
        #[cfg(windows)]
        sessions::session_detail,
        #[cfg(windows)]
        sessions::session_archive_add,
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
        terminal_resources,
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
                && !window.state::<ExitIntent>().0.load(Ordering::Acquire)
                && (!window.state::<Host>().list().is_empty()
                    || window.state::<Arc<workspace::Controller>>().owns_layout()
                    || sidecar_active(window))
            {
                api.prevent_close();
                let _ = window.emit("confirm-close", ());
            }
        })
        .run(tauri::generate_context!())
        .expect("Cannot run Shinatama desktop");
}
