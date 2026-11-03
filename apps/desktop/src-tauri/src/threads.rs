//! Bounded saved metadata and fixed sidebar preferences. No native launch API.
use crate::storage::Services;
use serde::{Deserialize, Serialize};
use shinatama_state::threads::{
    SidebarSnapshot, SidebarState, ThreadCursor, ThreadError, ThreadListPage,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Manager, State, WebviewWindow};

#[derive(Serialize)]
pub struct Error {
    code: &'static str,
}
impl From<ThreadError> for Error {
    fn from(error: ThreadError) -> Self {
        Self {
            code: match error {
                ThreadError::Invalid => "invalid",
                ThreadError::Conflict => "conflict",
                ThreadError::OwnedElsewhere => "owned_elsewhere",
                ThreadError::NotFound => "project_missing",
                ThreadError::RootUnavailable => "root_unavailable",
                ThreadError::Storage => "unavailable",
            },
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct List {
    project_id: String,
    cursor: Option<ThreadCursor>,
    limit: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Save {
    expected_revision: u64,
    settings: SidebarState,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Create {
    project_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rename {
    project_id: String,
    thread_id: String,
    expected_revision: u64,
    title: String,
}
pub struct Controller {
    services: Arc<Services>,
    busy: AtomicBool,
}
struct Slot(Arc<Controller>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}
impl Controller {
    pub fn new(services: Arc<Services>) -> Self {
        Self {
            services,
            busy: AtomicBool::new(false),
        }
    }
    async fn work<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce(&shinatama_state::database::Database) -> Result<T, ThreadError>
        + Send
        + 'static,
    ) -> Result<T, Error> {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error { code: "busy" })?;
        let slot = Slot(self.clone());
        tauri::async_runtime::spawn_blocking(move || {
            let state = slot.0.services.load().map_err(|_| Error {
                code: "unavailable",
            })?;
            work(&state.store).map_err(Into::into)
        })
        .await
        .map_err(|_| Error {
            code: "unavailable",
        })?
    }
}
fn local(window: &WebviewWindow) -> Result<(), Error> {
    crate::local(window).map_err(|_| Error { code: "invalid" })?;
    if window
        .state::<crate::ExitIntent>()
        .0
        .load(Ordering::Acquire)
    {
        return Err(Error { code: "closing" });
    }
    Ok(())
}
#[tauri::command]
pub async fn thread_list(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: List,
) -> Result<ThreadListPage, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            db.list_threads(&request.project_id, request.cursor.as_ref(), request.limit)
        })
        .await
}
#[tauri::command]
pub async fn thread_sidebar_load(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<SidebarSnapshot, Error> {
    local(&window)?;
    controller
        .inner()
        .work(|db| db.thread_sidebar_snapshot())
        .await
}
#[tauri::command]
pub async fn thread_sidebar_save(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Save,
) -> Result<SidebarSnapshot, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| db.save_thread_sidebar(request.expected_revision, request.settings))
        .await
}
#[tauri::command]
pub async fn thread_create(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Create,
) -> Result<shinatama_state::threads::ThreadRecord, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            db.list_threads(&request.project_id, None, 1)?;
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|time| i64::try_from(time.as_millis()).ok())
                .ok_or(ThreadError::Storage)?;
            db.create_thread(&shinatama_state::threads::ThreadRecord {
                id: shinatama_state::records::new_id(
                    shinatama_state::records::CanonicalIdKind::Thread,
                ),
                project_id: request.project_id,
                kind: shinatama_state::records::ThreadKind::Work,
                parent_thread_id: None,
                companion_worker_thread_id: None,
                title: None,
                archived: false,
                revision: 1,
                created_at: at,
                updated_at: at,
            })
        })
        .await
}

#[tauri::command]
pub async fn thread_rename(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Rename,
) -> Result<shinatama_state::threads::ThreadRecord, Error> {
    local(&window)?;
    controller
        .inner()
        .work(move |db| {
            // Bound the raw input before trimming. Preserve every other saved
            // field; neither archive state nor native binding comes from IPC.
            if request.title.len() > 256 || request.title.chars().any(char::is_control) {
                return Err(ThreadError::Invalid);
            }
            let title = request.title.trim();
            if title.is_empty() {
                return Err(ThreadError::Invalid);
            }
            let current = db
                .thread_record(&request.thread_id)?
                .ok_or(ThreadError::NotFound)?;
            if current.project_id != request.project_id {
                return Err(ThreadError::Invalid);
            }
            if current.revision != request.expected_revision {
                return Err(ThreadError::Conflict);
            }
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|time| i64::try_from(time.as_millis()).ok())
                .ok_or(ThreadError::Storage)?;
            db.update_thread_metadata(
                &request.thread_id,
                request.expected_revision,
                Some(title),
                current.archived,
                at,
            )
        })
        .await
}
