//! Explicit archive registration and one contained, retained metadata worker.
//! Explicit detail/confirmed RAW share that worker. No implicit home or writes.
#![cfg(windows)]
use crate::{storage::Services, terminal_runtime::Runtime};
use serde::{Deserialize, Serialize};
use shinatama_engine::session_detail::{DetailReply, DetailRequest};
use shinatama_engine::session_export::{ExportReply, ExportRequest};
use shinatama_engine::session_find::{FindReply, FindRequest};
use shinatama_engine::session_raw::{RawReply, RawRequest};
use shinatama_engine::export_frames::Progress;
use shinatama_engine::session_roots::{
    Framed, PreparedSessionList, RootSelector, SessionCursor, SessionListPage, SessionListRead,
    SessionReadIssue, metadata_worker_budget,
};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, StoreRootRecord},
};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use shinatama_engine::resources::admission::PreparedWorkload;
use tauri::{Emitter, Manager, State, WebviewWindow};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

pub mod export;
pub mod hop;
pub mod search;

/// The event a long search or export sends the window, at most four a second, until it ends. The
/// request id is the caller's: a window ignores an event of any other request. `file_bytes` is
/// context (the session file's size), never a percentage.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    request_id: String,
    kind: &'static str,
    entries: u64,
    bytes: u64,
    hits: u64,
    file_bytes: u64,
}

/// A search or an export a window can watch and stop: its progress goes out as `session-progress`
/// events and its cancel flag is read on every turn of the worker's loop.
pub struct Watch {
    cancel: Arc<AtomicBool>,
    emit: Box<dyn Fn(Progress, u64) + Send + Sync>,
}
impl Watch {
    /// No one is watching and no one can stop it.
    pub fn none() -> Self {
        Self { cancel: Arc::new(AtomicBool::new(false)), emit: Box::new(|_, _| {}) }
    }
}
/// Removes the request's cancel flag when the command ends, whatever way it ends: a cancel that
/// arrives after the end finds nothing to stop.
pub struct WatchGuard {
    controller: Arc<Controller>,
    id: String,
}
impl Drop for WatchGuard {
    fn drop(&mut self) {
        if let Ok(mut cancels) = self.controller.cancels.lock() {
            cancels.remove(&self.id);
        }
    }
}

/// What an export streamed to a staged file came back as.
pub enum StagedExport {
    /// The worker finished: its final frame, and the staged body to commit when it holds a payload.
    Done { reply: ExportReply, staged: shinatama_export::output::Staged },
    /// Writing the staged body failed (disk full, folder gone); the worker was stopped.
    WriteFailed,
}
const ROOT_LIMIT: usize = 100;
const MIB: u64 = 1024 * 1024;
/// How long an explicit read waits for the shared worker to come free.
const EXPLICIT_WORKER_WAIT: Duration = Duration::from_secs(20);
/// How long an explicit read retries its admission through momentary dips in free memory.
const EXPLICIT_ADMISSION_WAIT: Duration = Duration::from_secs(10);

/// Who is waiting for a read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// A read the user is waiting on: Resume, an opened session, the names on screen, a search
    /// or an export. It gets the lightweight admission of an explicit Open, waits briefly for the
    /// worker and goes before background reads.
    Explicit,
    /// Bulk or periodic work (the search index pass, instance summaries). It waits for healthy
    /// resource measurements without holding the worker, and yields the worker to explicit reads.
    Background,
}
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
    gate: WorkerGate,
    blocked: AtomicBool,
    retained: Mutex<Option<SessionListRead>>,
    search: Mutex<Option<Arc<shinatama_state::search_index::SearchIndex>>>,
    run: Arc<search::RunState>,
    /// The cancel flag of each watched request that is still running, by the window's request id.
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}
/// The one contained reader's slot. Explicit reads wait for it and go first: a background read
/// never takes it while an explicit read is waiting.
#[derive(Default)]
struct WorkerGate {
    busy: AtomicBool,
    explicit_waiting: AtomicUsize,
}
#[derive(Debug, PartialEq, Eq)]
enum GateRefusal {
    /// The slot stayed taken until the deadline.
    TimedOut,
    /// The reader was blocked while the request waited.
    Blocked,
}
impl WorkerGate {
    fn try_take(&self) -> bool {
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    fn release(&self) {
        self.busy.store(false, Ordering::Release);
    }
    fn take_background(&self) -> bool {
        self.explicit_waiting.load(Ordering::Acquire) == 0 && self.try_take()
    }
    fn take_explicit(&self, wait: Duration, blocked: impl Fn() -> bool) -> std::result::Result<(), GateRefusal> {
        self.explicit_waiting.fetch_add(1, Ordering::AcqRel);
        let deadline = Instant::now() + wait;
        let result = loop {
            if blocked() {
                break Err(GateRefusal::Blocked);
            }
            if self.try_take() {
                break Ok(());
            }
            if Instant::now() >= deadline {
                break Err(GateRefusal::TimedOut);
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        self.explicit_waiting.fetch_sub(1, Ordering::AcqRel);
        result
    }
}
struct WorkerSlot(Arc<Controller>);
impl Drop for WorkerSlot {
    fn drop(&mut self) {
        self.0.gate.release();
    }
}
impl Controller {
    pub fn new(services: Arc<Services>, runtime: Arc<Runtime>) -> Self {
        Self {
            services,
            runtime,
            gate: WorkerGate::default(),
            blocked: AtomicBool::new(false),
            retained: Mutex::new(None),
            search: Mutex::new(None),
            run: Arc::new(search::RunState::default()),
            cancels: Mutex::new(HashMap::new()),
        }
    }
    /// Makes a request watchable: `request_id` (1 to 64 of A-Z a-z 0-9 _ -) names its events and
    /// its cancel. Without one the request is not watched.
    pub fn watch(
        self: &Arc<Self>,
        window: &WebviewWindow,
        kind: &'static str,
        request_id: Option<String>,
    ) -> Result<(Watch, Option<WatchGuard>)> {
        let Some(id) = request_id else {
            return Ok((Watch::none(), None));
        };
        if id.is_empty()
            || id.len() > 64
            || !id.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err(SessionReadIssue::InvalidRequest);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut cancels = self.cancels.lock().map_err(|_| SessionReadIssue::ReadUnavailable)?;
            if cancels.contains_key(&id) {
                return Err(SessionReadIssue::InvalidRequest);
            }
            cancels.insert(id.clone(), Arc::clone(&cancel));
        }
        let window = window.clone();
        let event_id = id.clone();
        let emit = Box::new(move |progress: Progress, file_bytes: u64| {
            let _ = window.emit(
                "session-progress",
                ProgressEvent {
                    request_id: event_id.clone(),
                    kind,
                    entries: progress.entries,
                    bytes: progress.bytes,
                    hits: progress.hits,
                    file_bytes,
                },
            );
        });
        Ok((Watch { cancel, emit }, Some(WatchGuard { controller: Arc::clone(self), id })))
    }
    /// Stops the watched request `request_id` if it is still running; true when there was one.
    pub fn cancel(&self, request_id: &str) -> bool {
        self.cancels
            .lock()
            .ok()
            .and_then(|cancels| cancels.get(request_id).cloned())
            .is_some_and(|flag| {
                flag.store(true, Ordering::Release);
                true
            })
    }
    async fn worker<T: Send + 'static>(
        self: &Arc<Self>,
        priority: Priority,
        work: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        if self.blocked.load(Ordering::Acquire) {
            return Err(SessionReadIssue::CleanupUnconfirmed);
        }
        let controller = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            // Dropping the IPC future never frees a running worker.
            let _slot = controller.acquire(priority)?;
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
    /// The one worker, for a read of `priority`. A background read waits for healthy resource
    /// measurements before it takes the worker, never while holding it, and leaves the worker to
    /// any explicit read that is waiting. An explicit read waits up to [`EXPLICIT_WORKER_WAIT`]
    /// for the read in progress to end.
    fn acquire(self: &Arc<Self>, priority: Priority) -> Result<WorkerSlot> {
        match priority {
            Priority::Background => {
                self.runtime
                    .wait_for_admission()
                    .map_err(|_| SessionReadIssue::WorkerUnavailable)?;
                if !self.gate.take_background() {
                    return Err(SessionReadIssue::ReadUnavailable);
                }
            }
            Priority::Explicit => self
                .gate
                .take_explicit(EXPLICIT_WORKER_WAIT, || self.blocked.load(Ordering::Acquire))
                .map_err(|refusal| match refusal {
                    GateRefusal::Blocked => SessionReadIssue::CleanupUnconfirmed,
                    GateRefusal::TimedOut => SessionReadIssue::ReadUnavailable,
                })?,
        }
        Ok(WorkerSlot(Arc::clone(self)))
    }
    /// The contained helper's admission for a read the user is waiting on: the same lightweight
    /// admission starting a session gets (a fresh memory sample and a startup allowance), never
    /// the governor's background deferral, which a busy desktop holds for minutes on end. The
    /// helper keeps its Windows memory limits. Momentary dips in free memory are retried.
    fn explicit_workload(&self) -> Result<PreparedWorkload> {
        self.runtime
            .wait_for_interactive_admission()
            .map_err(|_| SessionReadIssue::WorkerUnavailable)?;
        let deadline = Instant::now() + EXPLICIT_ADMISSION_WAIT;
        loop {
            match self.runtime.prepare_interactive_support_workload(
                metadata_worker_budget(),
                768 * MIB,
                15_000,
            ) {
                Ok(workload) => return Ok(workload),
                Err(_) if Instant::now() >= deadline => {
                    return Err(SessionReadIssue::WorkerUnavailable);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(250)),
            }
        }
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
        self.worker(Priority::Explicit, |controller| {
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
        self.list_at(Priority::Explicit, request).await
    }
    pub async fn list_background(self: &Arc<Self>, request: ListRequest) -> Result<SessionListPage> {
        self.list_at(Priority::Background, request).await
    }
    async fn list_at(self: &Arc<Self>, priority: Priority, request: ListRequest) -> Result<SessionListPage> {
        self.worker(priority, move |controller| {
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
            let read = controller.read_prepared(state, &prepared, priority)?;
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
        priority: Priority,
    ) -> Result<SessionListRead> {
        self.read_prepared_with(state, prepared, None, priority)
    }
    /// [`Controller::read_prepared`] for an export: `sink` receives each body frame as it arrives.
    fn read_prepared_with(
        &self,
        state: &crate::storage::WorkspaceServices,
        prepared: &PreparedSessionList,
        hooks: Option<Framed<'_>>,
        priority: Priority,
    ) -> Result<SessionListRead> {
        let helper = helper_path()?;
        let workload = match priority {
            Priority::Explicit => self.explicit_workload()?,
            // `acquire` already waited for healthy measurements.
            Priority::Background => self
                .runtime
                .prepare_background_workload(metadata_worker_budget(), 768 * MIB, 15_000)
                .map_err(|_| SessionReadIssue::WorkerUnavailable)?,
        };
        let read = match hooks {
            Some(hooks) => {
                prepared.run_contained_framed(&state.store, &helper, &state.location, workload, hooks)
            }
            None => prepared.run_contained(&state.store, &helper, &state.location, workload),
        };
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
        self.detail_at(Priority::Explicit, request).await
    }
    pub async fn detail_background(self: &Arc<Self>, request: DetailRequest) -> Result<DetailReply> {
        self.detail_at(Priority::Background, request).await
    }
    async fn detail_at(self: &Arc<Self>, priority: Priority, request: DetailRequest) -> Result<DetailReply> {
        self.worker(priority, move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared =
                PreparedSessionList::resolve_detail(&state.store, &state.location, request)?;
            controller
                .read_prepared(state, &prepared, priority)?
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
        self.worker(Priority::Explicit, move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared =
                PreparedSessionList::resolve_raw(&state.store, &state.location, request)?;
            controller
                .read_prepared(state, &prepared, Priority::Explicit)?
                .raw()
                .cloned()
                .ok_or(SessionReadIssue::ReadUnavailable)
        })
        .await
    }
    pub async fn find(self: &Arc<Self>, request: FindRequest, watch: Watch) -> Result<FindReply> {
        shinatama_engine::session_find::validate_request(&request)
            .map_err(|_| SessionReadIssue::InvalidRequest)?;
        self.worker(Priority::Explicit, move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared =
                PreparedSessionList::resolve_find(&state.store, &state.location, request)?;
            let file_bytes = prepared.file_bytes();
            let mut body = |_: &[u8]| -> std::io::Result<()> {
                Err(std::io::Error::other("a search has no body"))
            };
            let mut progress = |progress: Progress| (watch.emit)(progress, file_bytes);
            let cancel = || watch.cancel.load(Ordering::Acquire);
            // The window learns the worker has started (it may have waited for a free slot).
            progress(Progress::default());
            let hooks = Framed { body: &mut body, progress: &mut progress, cancel: &cancel };
            let read = controller.read_prepared_with(state, &prepared, Some(hooks), Priority::Explicit)?;
            read.find().cloned().ok_or(SessionReadIssue::ReadUnavailable)
        })
        .await
    }
    /// An export held whole in memory, bounded to `MAX_EXPORT_CONTENT_BYTES`: for the hop
    /// preview, which holds the whole transcript in memory by design. The file's content is the
    /// reply's prefix followed by the body.
    pub async fn export(self: &Arc<Self>, request: ExportRequest) -> Result<(ExportReply, Vec<u8>)> {
        shinatama_engine::session_export::validate_request(&request)
            .map_err(|_| SessionReadIssue::InvalidRequest)?;
        self.worker(Priority::Explicit, move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared =
                PreparedSessionList::resolve_export(&state.store, &state.location, request)?;
            let mut body = Vec::new();
            let mut over = false;
            let read = {
                let mut sink = |bytes: &[u8]| {
                    if body.len() + bytes.len()
                        > shinatama_engine::session_export::MAX_EXPORT_CONTENT_BYTES
                    {
                        over = true;
                        return Err(std::io::Error::other("export over its in-memory bound"));
                    }
                    body.extend_from_slice(bytes);
                    Ok(())
                };
                let mut progress = |_: Progress| {};
                let never = || false;
                let hooks = Framed { body: &mut sink, progress: &mut progress, cancel: &never };
                controller.read_prepared_with(state, &prepared, Some(hooks), Priority::Explicit)
            };
            if over {
                return Err(SessionReadIssue::OutputLimit);
            }
            let reply = read?
                .export()
                .cloned()
                .ok_or(SessionReadIssue::ReadUnavailable)?;
            let mut content = reply
                .payload
                .as_ref()
                .map(|payload| payload.prefix.as_bytes().to_vec())
                .unwrap_or_default();
            content.extend_from_slice(&body);
            Ok((reply, content))
        })
        .await
    }
    /// An export of any size: the worker's body frames are written to `staged` as they arrive
    /// and the caller commits after the final frame (the reply). Dropping `staged` removes it.
    pub async fn export_staged(
        self: &Arc<Self>,
        request: ExportRequest,
        staged: shinatama_export::output::Staged,
        watch: Watch,
    ) -> Result<StagedExport> {
        shinatama_engine::session_export::validate_request(&request)
            .map_err(|_| SessionReadIssue::InvalidRequest)?;
        self.worker(Priority::Explicit, move |controller| {
            let state = controller
                .services
                .load()
                .map_err(|_| SessionReadIssue::ReadUnavailable)?;
            controller.retry_retained(&state.store)?;
            let prepared =
                PreparedSessionList::resolve_export(&state.store, &state.location, request)?;
            let mut staged = staged;
            let mut write_failed = false;
            let file_bytes = prepared.file_bytes();
            let read = {
                let mut sink = |bytes: &[u8]| staged.write(bytes).inspect_err(|_| write_failed = true);
                let mut progress = |progress: Progress| (watch.emit)(progress, file_bytes);
                let cancel = || watch.cancel.load(Ordering::Acquire);
                // The window learns the worker has started (it may have waited for a free slot).
                progress(Progress::default());
                let hooks = Framed { body: &mut sink, progress: &mut progress, cancel: &cancel };
                controller.read_prepared_with(state, &prepared, Some(hooks), Priority::Explicit)
            };
            if write_failed {
                return Ok(StagedExport::WriteFailed);
            }
            let reply = read?
                .export()
                .cloned()
                .ok_or(SessionReadIssue::ReadUnavailable)?;
            Ok(StagedExport::Done { reply, staged })
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
        self.worker(Priority::Explicit, move |controller| {
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
    request_id: Option<String>,
) -> Result<FindReply> {
    local(&window)?;
    let (watch, _guard) = controller.inner().watch(&window, "search", request_id)?;
    controller.inner().find(request, watch).await
}
/// Stops the watched search or export `request_id`, if it is still running. The worker is
/// stopped, its resources are released and the staged export file is removed.
#[tauri::command]
pub async fn session_cancel(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request_id: String,
) -> Result<bool> {
    local(&window)?;
    Ok(controller.inner().cancel(&request_id))
}

#[cfg(test)]
mod gate_tests {
    use super::{GateRefusal, WorkerGate};
    use std::{sync::Arc, thread, time::{Duration, Instant}};

    #[test]
    fn an_explicit_read_waits_for_the_read_in_progress() {
        let gate = Arc::new(WorkerGate::default());
        assert!(gate.try_take());
        let holder = Arc::clone(&gate);
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            holder.release();
        });
        let started = Instant::now();
        assert_eq!(gate.take_explicit(Duration::from_secs(5), || false), Ok(()));
        assert!(started.elapsed() >= Duration::from_millis(150));
        release.join().unwrap();
    }

    #[test]
    fn a_background_read_yields_to_a_waiting_explicit_read() {
        let gate = Arc::new(WorkerGate::default());
        assert!(gate.try_take());
        let waiter = Arc::clone(&gate);
        let explicit = thread::spawn(move || waiter.take_explicit(Duration::from_secs(5), || false));
        while gate.explicit_waiting.load(std::sync::atomic::Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        gate.release();
        // The freed slot is not taken by background work while the explicit read waits.
        assert!(!gate.take_background() || explicit.is_finished());
        assert_eq!(explicit.join().unwrap(), Ok(()));
        assert!(!gate.take_background(), "the explicit read holds the slot");
        gate.release();
        assert!(gate.take_background());
    }

    #[test]
    fn an_explicit_wait_is_bounded_and_counted_off() {
        let gate = WorkerGate::default();
        assert!(gate.try_take());
        assert_eq!(gate.take_explicit(Duration::from_millis(120), || false), Err(GateRefusal::TimedOut));
        assert_eq!(gate.explicit_waiting.load(std::sync::atomic::Ordering::Acquire), 0);
        assert_eq!(gate.take_explicit(Duration::from_secs(5), || true), Err(GateRefusal::Blocked));
        assert_eq!(gate.explicit_waiting.load(std::sync::atomic::Ordering::Acquire), 0);
    }
}
