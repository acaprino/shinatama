//! Layout-only persistence: one in-flight owner worker, no process restoration.
use crate::storage::Services;
use serde::{Deserialize, Serialize};
use shinatama_state::{
    database::DatabaseAccess,
    records::{MAX_LAYOUT_BYTES, MAX_SAFE_REVISION, RecordsError, WorkspaceOwner, WorkspaceRecord},
};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{State, WebviewWindow};

pub struct Controller {
    services: Arc<Services>,
    busy: Arc<AtomicBool>,
    owner: OnceLock<Result<WorkspaceOwner, Error>>,
}
impl Controller {
    pub fn new(services: Arc<Services>) -> Self {
        Self {
            services,
            busy: Arc::new(AtomicBool::new(false)),
            owner: OnceLock::new(),
        }
    }
    fn reserve(&self) -> Result<Reservation, Error> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                Error::new(
                    "unavailable",
                    "Workspace storage is busy; retry explicitly.",
                )
            })?;
        Ok(Reservation(Arc::clone(&self.busy)))
    }
    fn owner(&self) -> Result<&WorkspaceOwner, Error> {
        self.owner
            .get_or_init(|| {
                #[cfg(windows)]
                {
                    let key = shinatama_engine::resources::sampling::WindowsSampler::identify(
                        std::process::id(),
                    )
                    .map_err(|_| {
                        Error::new("unavailable", "Workspace ownership could not be verified.")
                    })?;
                    Ok(WorkspaceOwner {
                        window_id: format!("main-{}-{}", key.pid, key.created_at_100ns),
                        pid: key.pid,
                        start_time: format!("win-{}", key.created_at_100ns),
                    })
                }
                #[cfg(not(windows))]
                {
                    Err(Error::new(
                        "unavailable",
                        "Workspace ownership requires Windows.",
                    ))
                }
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}
struct Reservation(Arc<AtomicBool>);
impl Drop for Reservation {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Error {
    code: &'static str,
    message: &'static str,
}
impl Error {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}
impl From<RecordsError> for Error {
    fn from(error: RecordsError) -> Self {
        match error {
            RecordsError::Conflict => Self::new(
                "conflict",
                "Workspace changed; reload explicitly before saving.",
            ),
            RecordsError::OwnedElsewhere => {
                Self::new("owned_elsewhere", "Workspace is owned by another window.")
            }
            RecordsError::Invalid(_) | RecordsError::UnsupportedLayout => Self::new(
                "invalid",
                "Workspace layout or revision is invalid; the saved layout was preserved.",
            ),
            _ => Self::new(
                "uncertain",
                "Workspace save could not be confirmed; reload explicitly.",
            ),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    project_id: String,
    workspace_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Save {
    project_id: String,
    workspace_id: String,
    expected_revision: u64,
    schema_version: u32,
    layout_json: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    project_id: String,
    workspace_id: String,
    expected_revision: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    project_id: String,
    workspace_id: String,
    revision: u64,
    schema_version: u32,
    layout_json: String,
    ownership: &'static str,
    read_only: bool,
}
fn scope(project: &str, workspace: &str) -> Result<(), Error> {
    shinatama_state::records::validate_id(
        project,
        shinatama_state::records::CanonicalIdKind::Project,
    )
    .map_err(Error::from)?;
    if workspace.is_empty()
        || workspace.len() > 128
        || !workspace
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
    {
        return Err(Error::new("invalid", "Workspace identity is invalid."));
    }
    Ok(())
}
fn snapshot(record: WorkspaceRecord, owner: &WorkspaceOwner, read_only: bool) -> Snapshot {
    let ownership = match &record.owner {
        None => "unclaimed",
        Some(saved) if saved == owner => "ours",
        Some(_) => "other",
    };
    Snapshot {
        project_id: record.project_id,
        workspace_id: record.workspace_id,
        revision: record.revision,
        schema_version: record.schema_version,
        layout_json: record.layout_json,
        ownership,
        read_only,
    }
}
fn local(window: &WebviewWindow) -> Result<(), Error> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(Error::new("unavailable", "Unknown workspace window."))
    }
}
fn read_only() -> Error {
    Error::new(
        "read_only",
        "Workspace storage is read-only; update Shinatama to save.",
    )
}

#[tauri::command]
pub async fn workspace_load(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Scope,
) -> Result<Option<Snapshot>, Error> {
    local(&window)?;
    scope(&request.project_id, &request.workspace_id)?;
    let reservation = controller.reserve()?;
    let controller = Arc::clone(controller.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let _reservation = reservation;
        let state = controller
            .services
            .load()
            .map_err(|_| Error::new("unavailable", "Workspace storage is unavailable."))?;
        let owner = controller.owner()?;
        let read_only = !matches!(state.store.access(), DatabaseAccess::ReadWrite);
        state
            .store
            .workspace_record(&request.project_id, &request.workspace_id)
            .map(|record| record.map(|record| snapshot(record, owner, read_only)))
            .map_err(Error::from)
    })
    .await
    .map_err(|_| Error::new("unavailable", "Workspace read was interrupted."))?
}

#[tauri::command]
pub async fn workspace_save(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Save,
) -> Result<Snapshot, Error> {
    local(&window)?;
    scope(&request.project_id, &request.workspace_id)?;
    if request.schema_version != 1
        || request.layout_json.len() > MAX_LAYOUT_BYTES
        || request.expected_revision >= MAX_SAFE_REVISION
    {
        return Err(Error::new(
            "invalid",
            "Workspace layout exceeds its limits.",
        ));
    }
    let reservation = controller.reserve()?;
    let controller = Arc::clone(controller.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let _reservation = reservation;
        let state = controller
            .services
            .load()
            .map_err(|_| Error::new("unavailable", "Workspace storage is unavailable."))?;
        if !matches!(state.store.access(), DatabaseAccess::ReadWrite) {
            return Err(read_only());
        }
        let owner = controller.owner()?;
        let updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|value| i64::try_from(value.as_millis()).ok())
            .ok_or_else(|| Error::new("unavailable", "Workspace clock is unavailable."))?;
        let record = WorkspaceRecord {
            project_id: request.project_id,
            workspace_id: request.workspace_id,
            revision: request.expected_revision + 1,
            schema_version: 1,
            layout_json: request.layout_json,
            owner: None,
            updated_at,
        };
        state
            .store
            .save_workspace(record, request.expected_revision, owner)
            .map(|record| snapshot(record, owner, false))
            .map_err(Error::from)
    })
    .await
    .map_err(|_| {
        Error::new(
            "uncertain",
            "Workspace save was interrupted; reload explicitly.",
        )
    })?
}

#[tauri::command]
pub async fn workspace_release(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Release,
) -> Result<(), Error> {
    local(&window)?;
    scope(&request.project_id, &request.workspace_id)?;
    let reservation = controller.reserve()?;
    let controller = Arc::clone(controller.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let _reservation = reservation;
        let state = controller
            .services
            .load()
            .map_err(|_| Error::new("unavailable", "Workspace storage is unavailable."))?;
        if !matches!(state.store.access(), DatabaseAccess::ReadWrite) {
            return Err(read_only());
        }
        state
            .store
            .release_workspace(
                &request.project_id,
                &request.workspace_id,
                request.expected_revision,
                controller.owner()?,
            )
            .map_err(Error::from)
    })
    .await
    .map_err(|_| Error::new("uncertain", "Workspace release was interrupted."))?
}
