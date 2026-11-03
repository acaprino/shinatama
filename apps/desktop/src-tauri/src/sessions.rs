//! Explicit archive registration and one contained, retained metadata worker.
//! Explicit detail/confirmed RAW share that worker. No implicit home or writes.
#![cfg(windows)]
use crate::{storage::Services, terminal_runtime::Runtime};
use serde::{Deserialize, Serialize};
use shinatama_engine::session_detail::{DetailReply, DetailRequest};
use shinatama_engine::session_find::{FindReply, FindRequest};
use shinatama_engine::session_raw::{RawReply, RawRequest};
use shinatama_engine::session_roots::{
    PreparedSessionList, RootSelector, SessionCursor, SessionListPage, SessionListRead,
    SessionReadIssue, metadata_worker_budget,
};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
};
use std::{
    fs::{File, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Manager, State, WebviewWindow};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

const ROOT_LIMIT: usize = 100;
const MIB: u64 = 1024 * 1024;
type Result<T> = std::result::Result<T, SessionReadIssue>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLibraryRoot {
    store_root_id: String,
    configuration_revision: u64,
    harness: String,
    kind: RootKind,
    access: RootAccess,
    available: bool,
    label: String,
}
impl From<StoreRootRecord> for SessionLibraryRoot {
    fn from(root: StoreRootRecord) -> Self {
        let name: String = Path::new(&root.resolved_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Archive")
            .chars()
            .take(32)
            .collect();
        let label = format!(
            "{} · {} · {}",
            root.harness.chars().take(24).collect::<String>(),
            name,
            root.id.chars().take(8).collect::<String>()
        );
        Self {
            store_root_id: root.id,
            configuration_revision: root.config_revision,
            harness: root.harness,
            kind: root.kind,
            access: root.access,
            available: root.available,
            label,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListRequest {
    root: RootSelector,
    cursor: Option<SessionCursor>,
    limit: usize,
    include_children: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveRequest {
    harness: String,
    path: String,
}
pub struct Controller {
    services: Arc<Services>,
    runtime: Arc<Runtime>,
    busy: AtomicBool,
    blocked: AtomicBool,
    retained: Mutex<Option<SessionListRead>>,
}
struct WorkerSlot(Arc<Controller>);
impl Drop for WorkerSlot {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}
impl Controller {
    pub fn new(services: Arc<Services>, runtime: Arc<Runtime>) -> Self {
        Self {
            services,
            runtime,
            busy: AtomicBool::new(false),
            blocked: AtomicBool::new(false),
            retained: Mutex::new(None),
        }
    }
    async fn worker<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        if self.blocked.load(Ordering::Acquire) {
            return Err(SessionReadIssue::CleanupUnconfirmed);
        }
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| SessionReadIssue::ReadUnavailable)?;
        let slot = WorkerSlot(Arc::clone(self));
        let controller = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let _slot = slot; // Dropping the IPC future never frees a running worker.
            match catch_unwind(AssertUnwindSafe(|| work(&controller))) {
                Ok(result) => result,
                Err(_) => {
                    controller.blocked.store(true, Ordering::Release);
                    Err(SessionReadIssue::CleanupUnconfirmed)
                }
            }
        })
        .await
        .map_err(|_| SessionReadIssue::CleanupUnconfirmed)?
    }
    fn require_no_retained(&self) -> Result<()> {
        if self
            .retained
            .lock()
            .map_err(|_| SessionReadIssue::CleanupUnconfirmed)?
            .is_some()
        {
            Err(SessionReadIssue::CleanupUnconfirmed)
        } else {
            Ok(())
        }
    }
    fn retry_retained(&self, database: &Database) -> Result<()> {
        let mut owner = self
            .retained
            .lock()
            .map_err(|_| SessionReadIssue::CleanupUnconfirmed)?
            .take();
        if let Some(read) = owner.as_mut()
            && !read.retry_cleanup(database)
        {
            *self
                .retained
                .lock()
                .map_err(|_| SessionReadIssue::CleanupUnconfirmed)? = owner;
            return Err(SessionReadIssue::CleanupUnconfirmed);
        }
        // Same owner only; no replacement read begins until native/queue/SQL release.
        drop(owner);
        Ok(())
    }
    pub async fn roots(self: &Arc<Self>) -> Result<Vec<SessionLibraryRoot>> {
        self.worker(|controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            // Explicit Load archives also works after a webview reload: retry
            // only the same retained cleanup, before reading the root catalog.
            controller.retry_retained(&state.store)?;
            state
                .store
                .list_store_roots(None, ROOT_LIMIT)
                .map(|roots| roots.into_iter().map(Into::into).collect())
                .map_err(|_| SessionReadIssue::ReadUnavailable)
        })
        .await
    }
    pub async fn list(self: &Arc<Self>, request: ListRequest) -> Result<SessionListPage> {
        self.worker(move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared = PreparedSessionList::resolve(
                &state.store,
                &state.location,
                &request.root,
                request.cursor,
                request.limit,
                request.include_children,
            )?;
            let read = controller.read_prepared(state, &prepared)?;
            read.page()
                .cloned()
                .ok_or(SessionReadIssue::ReadUnavailable)
        })
        .await
    }
    fn read_prepared(
        &self,
        state: &crate::storage::WorkspaceServices,
        prepared: &PreparedSessionList,
    ) -> Result<SessionListRead> {
        let helper = helper_path()?;
        self.runtime
            .wait_for_admission()
            .map_err(|_| SessionReadIssue::WorkerUnavailable)?;
        let workload = self
            .runtime
            .prepare_background_workload(metadata_worker_budget(), 768 * MIB, 15_000)
            .map_err(|_| SessionReadIssue::WorkerUnavailable)?;
        let read = prepared.run_contained(&state.store, &helper, &state.location, workload);
        if read.cleanup_retained() {
            *self
                .retained
                .lock()
                .map_err(|_| SessionReadIssue::CleanupUnconfirmed)? = Some(read);
            return Err(SessionReadIssue::CleanupUnconfirmed);
        }
        if let Some(issue) = read.issue {
            return Err(issue);
        }
        Ok(read)
    }
    pub async fn detail(self: &Arc<Self>, request: DetailRequest) -> Result<DetailReply> {
        self.worker(move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared =
                PreparedSessionList::resolve_detail(&state.store, &state.location, request)?;
            controller
                .read_prepared(state, &prepared)?
                .detail()
                .cloned()
                .ok_or(SessionReadIssue::ReadUnavailable)
        })
        .await
    }
    pub async fn raw(self: &Arc<Self>, request: RawRequest) -> Result<RawReply> {
        // Reject an unconfirmed request before services/archive work or retries.
        if !request.confirmed_raw {
            return Err(SessionReadIssue::InvalidRequest);
        }
        self.worker(move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared =
                PreparedSessionList::resolve_raw(&state.store, &state.location, request)?;
            controller
                .read_prepared(state, &prepared)?
                .raw()
                .cloned()
                .ok_or(SessionReadIssue::ReadUnavailable)
        })
        .await
    }
    pub async fn find(self: &Arc<Self>, request: FindRequest) -> Result<FindReply> {
        shinatama_engine::session_find::validate_request(&request)
            .map_err(|_| SessionReadIssue::InvalidRequest)?;
        self.worker(move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared =
                PreparedSessionList::resolve_find(&state.store, &state.location, request)?;
            controller
                .read_prepared(state, &prepared)?
                .find()
                .cloned()
                .ok_or(SessionReadIssue::ReadUnavailable)
        })
        .await
    }
    pub async fn add_archive(
        self: &Arc<Self>,
        request: ArchiveRequest,
    ) -> Result<SessionLibraryRoot> {
        if !matches!(request.harness.as_str(), "claude_code" | "codex" | "pi")
            || request.path.is_empty()
            || request.path.len() > 4096
            || request.path.chars().any(char::is_control)
            || !Path::new(&request.path).is_absolute()
        {
            return Err(SessionReadIssue::InvalidRequest);
        }
        self.worker(move |controller| {
            controller.require_no_retained()?;
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            state
                .store
                .check_write_admission()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            // Only canonical path/shape verification; never create, repair ACLs,
            // discover session files or read contents in the selected archive.
            StorageLocation::preflight(Path::new(&request.path))
                .map_err(|_| SessionReadIssue::UnsafeRoot)?;
            let prepared = PreparedStoreRoot::resolve(StoreRootRecord {
                id: uuid::Uuid::new_v4().to_string(),
                harness: request.harness,
                kind: RootKind::System,
                scope: RootScope::Additional,
                resolved_path: request.path,
                origin: RootOrigin::Configured,
                config_revision: 1,
                access: RootAccess::ReadOnly,
                available: true,
            })
            .map_err(|_| SessionReadIssue::UnsafeRoot)?;
            // Serialize C22 additions across windows. This file belongs to C01,
            // never the selected archive. Capacity/dedup are re-read under lock.
            let _lock = archive_lock(&state.location)?;
            let roots = state
                .store
                .list_store_roots(None, ROOT_LIMIT)
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            if let Some(existing) = roots.iter().find(|root| {
                root.harness == prepared.record().harness
                    && root
                        .resolved_path
                        .eq_ignore_ascii_case(&prepared.record().resolved_path)
            }) {
                return Ok(existing.clone().into());
            }
            if roots.len() >= ROOT_LIMIT {
                return Err(SessionReadIssue::MetadataTooLarge);
            }
            state
                .store
                .register_additional_system_root(&prepared)
                .map(Into::into)
                .map_err(|_| SessionReadIssue::ReadUnavailable)
        })
        .await
    }
}
fn helper_path() -> Result<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .map(|directory| directory.join("shinatama-session-helper.exe"))
        })
        .filter(|path| path.is_absolute())
        .ok_or(SessionReadIssue::WorkerUnavailable)
}
fn archive_lock(location: &StorageLocation) -> Result<File> {
    let path = location.directory().join("session-roots.lock");
    let options = || {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        options
    };
    let file = match options().create_new(true).open(&path) {
        Ok(file) => {
            // Protect only our newly created app lock, never an archive ACL.
            location
                .secure_private_file(&path)
                .map_err(|_| SessionReadIssue::UnsafeRoot)?;
            file
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            location
                .verify_private_file(&path)
                .map_err(|_| SessionReadIssue::UnsafeRoot)?;
            options()
                .open(&path)
                .map_err(|_| SessionReadIssue::ReadUnavailable)?
        }
        Err(_) => return Err(SessionReadIssue::ReadUnavailable),
    };
    location
        .verify_private_file(&path)
        .map_err(|_| SessionReadIssue::UnsafeRoot)?;
    file.try_lock()
        .map_err(|_| SessionReadIssue::ReadUnavailable)?;
    Ok(file)
}
fn local(window: &WebviewWindow) -> Result<()> {
    crate::local(window).map_err(|_| SessionReadIssue::InvalidRequest)?;
    if window
        .state::<crate::ExitIntent>()
        .0
        .load(Ordering::Acquire)
    {
        return Err(SessionReadIssue::ReadUnavailable);
    }
    Ok(())
}
#[tauri::command]
pub async fn session_roots(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<Vec<SessionLibraryRoot>> {
    local(&window)?;
    controller.inner().roots().await
}
#[tauri::command]
pub async fn session_list(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: ListRequest,
) -> Result<SessionListPage> {
    local(&window)?;
    controller.inner().list(request).await
}
#[tauri::command]
pub async fn session_archive_add(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: ArchiveRequest,
) -> Result<SessionLibraryRoot> {
    local(&window)?;
    controller.inner().add_archive(request).await
}
#[tauri::command]
pub async fn session_detail(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: DetailRequest,
) -> Result<DetailReply> {
    local(&window)?;
    controller.inner().detail(request).await
}
#[tauri::command]
pub async fn session_raw(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: RawRequest,
) -> Result<RawReply> {
    local(&window)?;
    controller.inner().raw(request).await
}
#[tauri::command]
pub async fn session_find(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: FindRequest,
) -> Result<FindReply> {
    local(&window)?;
    controller.inner().find(request).await
}
