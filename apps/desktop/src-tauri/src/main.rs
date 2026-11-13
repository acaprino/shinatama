#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod agents;
mod attention;
mod attention_signals;
mod files;
mod instances;
#[cfg(windows)]
mod titles;
mod project_overview;
#[cfg(windows)]
mod hop_run;
mod launch;
pub mod maintenance;
mod new_project;
mod presentation;
mod projects;
mod resource_governor;
mod resources;
#[cfg(windows)]
mod marionette;
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
    let runtime = ready::<terminal_runtime::Runtime>(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        services
            .load()?
            .location
            .validate_project(&PathBuf::from(&cwd))
            .map_err(|e| e.to_string())?;
        #[cfg(windows)]
        runtime.configure(&host)?;
        #[cfg(windows)]
        runtime.wait_for_interactive_admission()?;
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
    let batch = host.read(&id).map_err(|e| e.to_string())?;
    if let Ok(session) = host.info(&id) {
        window.state::<attention::Controller>().observe(window.app_handle(), &session, &batch);
    }
    Ok(batch)
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
    let stopped_id = id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || host.stop(&id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string());
    if result.is_ok() { window.state::<attention::Controller>().forget(window.app_handle(), &stopped_id); }
    result
}
#[tauri::command]
async fn terminal_shutdown(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
) -> Result<(), String> {
    local(&window)?;
    let host = host.inner().clone();
    #[cfg(windows)]
    let sidecar = window.try_state::<Arc<sidecar::Controller>>().map(|s| s.inner().clone());
    #[cfg(windows)]
    if let Some(runtime) = window.try_state::<Arc<terminal_runtime::Runtime>>() {
        runtime.request_shutdown();
    }
    tauri::async_runtime::spawn_blocking(move || {
        host.shutdown();
        #[cfg(windows)]
        if let Some(sidecar) = &sidecar {
            sidecar.stop_and_wait(std::time::Duration::from_secs(6));
        }
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
/// Managed in setup; an IPC call can arrive while setup still runs. Never panic: say so.
#[cfg(windows)]
pub(crate) fn ready<T: Send + Sync + 'static>(window: &tauri::WebviewWindow) -> Result<std::sync::Arc<T>, String>
where
    std::sync::Arc<T>: Send + Sync,
{
    window
        .try_state::<std::sync::Arc<T>>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| "Shinatama is still finishing its start; not yet ready. Retry in a moment.".to_owned())
}
fn sidecar_active(window: &tauri::Window) -> bool {
    #[cfg(windows)]
    return window.try_state::<Arc<sidecar::Controller>>().is_some_and(|c| c.is_active());
    #[cfg(not(windows))]
    false
}
#[tauri::command]
fn agent_status(window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    local(&window)?;
    #[cfg(windows)]
    return Ok(ready::<agents::Controller>(&window)?.diagnostics());
    #[cfg(not(windows))]
    Ok(serde_json::json!({"checking":false,"blocked":false}))
}
#[cfg(windows)]
#[tauri::command]
fn harness_catalog(
    window: tauri::WebviewWindow,
) -> Result<Vec<agents::HarnessCapabilities>, String> {
    local(&window)?;
    Ok(ready::<agents::Controller>(&window)?.catalog())
}
#[cfg(not(windows))]
#[tauri::command]
fn harness_catalog(window: tauri::WebviewWindow) -> Result<Vec<serde_json::Value>, String> {
    local(&window)?;
    Ok(Vec::new())
}
#[cfg(windows)]
#[tauri::command]
async fn thread_resume(
    window: tauri::WebviewWindow,
    host: State<'_, Host>,
    request: agents::resume::Request,
) -> Result<agents::AgentStarted, agents::resume::Error> {
    local(&window).map_err(|_| agents::resume::Error {
        code: "invalid_request",
    })?;
    if window.state::<ExitIntent>().0.load(Ordering::Acquire) {
        return Err(agents::resume::Error {
            code: "unavailable",
        });
    }
    let not_ready = |_| agents::resume::Error { code: "unavailable" };
    let owner = ready::<agents::Controller>(&window).map_err(not_ready)?;
    let reader = ready::<sessions::Controller>(&window).map_err(not_ready)?;
    owner.resume(host.inner().clone(), reader, request).await
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
        let owner = ready::<agents::Controller>(&window)?;
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
        let owner = ready::<agents::Controller>(&window)?;
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
        let owner = ready::<agents::Controller>(&window)?;
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
    return serde_json::to_value(ready::<sidecar::Controller>(&window)?.status())
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
    return ready::<sidecar::Controller>(&window)?
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
    return ready::<sidecar::Controller>(&window)?
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
    return ready::<sidecar::Controller>(&window)?
        .retry()
        .map_err(str::to_owned)
        .and_then(|status| {
            serde_json::to_value(status).map_err(|_| "Session index status is unavailable.".into())
        });
    #[cfg(not(windows))]
    Err("The session index is available on Windows.".into())
}
/// A portable install (`Shinatama.portable` beside the exe) keeps its database in `data\` and
/// its WebView2 profile in `webview\` beside Shinatama.exe; child processes inherit both.
/// Started from its own folder with its own data, it opens the user's profile folder as the
/// default project. An explicit SHINATAMA_HOME keeps the folder it was started from, since a
/// data folder inside the profile would make the profile a project that contains the data.
fn portable_environment() {
    use shinatama_state::location::{PORTABLE_DATA, portable_root};
    let Some(root) = portable_root() else { return };
    let portable_data = std::env::var_os("SHINATAMA_HOME").is_none();
    // SAFETY: first statement of main, before any other thread exists.
    unsafe {
        if portable_data {
            std::env::set_var("SHINATAMA_HOME", root.join(PORTABLE_DATA));
        }
        if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_none() {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", root.join("webview"));
        }
    }
    if !portable_data {
        return;
    }
    let inside = std::env::current_dir().is_ok_and(|cwd| {
        cwd.ancestors().any(|part| part.as_os_str().eq_ignore_ascii_case(root.as_os_str()))
    });
    if inside && let Some(profile) = std::env::var_os("USERPROFILE") {
        let _ = std::env::set_current_dir(profile);
    }
}
fn main() {
    portable_environment();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(TerminalHost::default()))
        .manage(ExitIntent::default())
        .manage(attention::Controller::default())
        .manage(shinatama_engine::Engine::new())
        .manage(resources::Resources::start())
        .setup(|app| {
            let directory = std::env::var_os("SHINATAMA_HOME")
                .map(PathBuf::from)
                .map(Ok)
                .unwrap_or_else(|| app.path().app_config_dir());
            #[cfg(windows)]
            if let Ok(home) = &directory {
                marionette::start(home);
            }
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
                let thread_window = app.handle().clone();
                app.manage(Arc::new(agents::Controller::new(
                    services.clone(),
                    runtime.clone(),
                    sidecar.clone(),
                    Arc::new(move |project_id, thread_id| {
                        // Only saved selectors leave the lifecycle observer. The
                        // frontend re-reads bounded metadata; no native data or
                        // action authority is carried by this notification.
                        let _ = thread_window.emit_to("main", "thread-changed", serde_json::json!({
                            "projectId": project_id, "threadId": thread_id,
                        }));
                    }),
                )));
                app.manage(sidecar);
                app.manage(runtime);
            }
            app.manage(services);
            // Tauri builds configured windows before this hook runs, so a page could call a
            // command whose state is not managed yet ("Agent choices could not be loaded").
            // The windows are declared with "create": false and built only now.
            for config in app.config().app.windows.clone() {
                tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?.build()?;
            }
            attention::start_memory_warnings(app.handle());
            Ok(())
        });
    #[cfg(not(feature = "tl1-proof"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        attention::attention_current,
        attention::attention_configure,
        attention::attention_preview,
        attention::attention_open,
        attention::attention_dismiss,
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
        threads::thread_rename,
        threads::thread_archive,
        threads::thread_forget,
        threads::thread_recover,
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
        harness_catalog,
        #[cfg(windows)]
        thread_resume,
        shell_open,
        #[cfg(windows)]
        sessions::session_roots,
        #[cfg(windows)]
        sessions::session_list,
        #[cfg(windows)]
        sessions::session_detail,
        #[cfg(windows)]
        sessions::session_raw,
        sessions::session_find,
        files::files_list,
        files::files_search,
        files::files_cancel,
        files::files_read,
        files::files_save,
        files::files_markdown,
        files::files_image,
        files::files_visual_parse,
        files::files_visual_apply,
        project_overview::project_overview,
        instances::instances_overview,
        #[cfg(windows)]
        titles::instance_titles,
        #[cfg(windows)]
        titles::titles_set_enabled,
        #[cfg(windows)]
        titles::thread_summaries_pause,
        project_overview::project_pin,
        project_overview::project_worktree_plan,
        project_overview::project_worktree_create,
        #[cfg(windows)]
        sessions::export::session_export,
        #[cfg(windows)]
        sessions::hop::hop_preview,
        #[cfg(windows)]
        hop_run::hop_launch,
        #[cfg(windows)]
        hop_run::hop_deliver,
        #[cfg(windows)]
        hop_run::hop_finish,
        #[cfg(windows)]
        hop_run::hop_status,
        #[cfg(windows)]
        hop_run::hops_recent,
        #[cfg(windows)]
        sessions::session_archive_add,
        #[cfg(windows)]
        sessions::search::session_search,
        #[cfg(windows)]
        sessions::search::session_index_start,
        #[cfg(windows)]
        sessions::search::session_index_cancel,
        #[cfg(windows)]
        sessions::search::session_index_status,
        #[cfg(windows)]
        sessions::search::session_index_enable,
        #[cfg(windows)]
        sessions::search::session_index_forget,
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
        attention::attention_current,
        attention::attention_configure,
        attention::attention_preview,
        attention::attention_open,
        attention::attention_dismiss,
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
        threads::thread_rename,
        threads::thread_archive,
        threads::thread_forget,
        threads::thread_recover,
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
        harness_catalog,
        #[cfg(windows)]
        thread_resume,
        shell_open,
        #[cfg(windows)]
        sessions::session_roots,
        #[cfg(windows)]
        sessions::session_list,
        #[cfg(windows)]
        sessions::session_detail,
        #[cfg(windows)]
        sessions::session_raw,
        sessions::session_find,
        files::files_list,
        files::files_search,
        files::files_cancel,
        files::files_read,
        files::files_save,
        files::files_markdown,
        files::files_image,
        files::files_visual_parse,
        files::files_visual_apply,
        project_overview::project_overview,
        instances::instances_overview,
        #[cfg(windows)]
        titles::instance_titles,
        #[cfg(windows)]
        titles::titles_set_enabled,
        #[cfg(windows)]
        titles::thread_summaries_pause,
        project_overview::project_pin,
        project_overview::project_worktree_plan,
        project_overview::project_worktree_create,
        #[cfg(windows)]
        sessions::export::session_export,
        #[cfg(windows)]
        sessions::hop::hop_preview,
        #[cfg(windows)]
        hop_run::hop_launch,
        #[cfg(windows)]
        hop_run::hop_deliver,
        #[cfg(windows)]
        hop_run::hop_finish,
        #[cfg(windows)]
        hop_run::hop_status,
        #[cfg(windows)]
        hop_run::hops_recent,
        #[cfg(windows)]
        sessions::session_archive_add,
        #[cfg(windows)]
        sessions::search::session_search,
        #[cfg(windows)]
        sessions::search::session_index_start,
        #[cfg(windows)]
        sessions::search::session_index_cancel,
        #[cfg(windows)]
        sessions::search::session_index_status,
        #[cfg(windows)]
        sessions::search::session_index_enable,
        #[cfg(windows)]
        sessions::search::session_index_forget,
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
            if window.label() == "attention" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                return;
            }
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(widget) = window.app_handle().get_webview_window("attention") { let _ = widget.destroy(); }
                return;
            }
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
