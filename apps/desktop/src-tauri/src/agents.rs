//! Registered harness services. This boundary schedules blocking work and exposes safe UI data.
#![cfg(windows)]
mod claude;
mod codex;
mod opencode;
mod pi;
pub(crate) mod resume;
mod service;
#[cfg(test)]
mod tests;
mod thread_lifecycle;
use crate::{sidecar, storage::Services, terminal_runtime::Runtime};
use serde::Serialize;
use service::{HarnessService, OpenMode, Service, ServiceContext, SessionOperation};
use shinatama_adapter::sets::{SET_SCHEMA_VERSION, SetManifest};
use shinatama_engine::{
    check_scratch::{self, OwnedScratch},
    model_config::Selection,
    native_launch::{NativeTrialReceipt, NativeTrialTracker},
};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    records::{
        PreparedStoreRoot, RecordsError, RootAccess, RootKind, RootOrigin, RootScope,
        StoreRootRecord,
    },
    store_roots::{AgentStoreKey, PreparedPrivateStore, PrivateStorePlan, PrivateStoreRequest},
};
use shinatama_terminal::{SessionInfo, TerminalHost};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
const ROOT_SCAN_LIMIT: usize = 100;
/// Notification only: project/thread selectors have no database or process
/// authority. Callbacks must enqueue UI work without waiting for a response.
pub(crate) type ThreadObserver = Arc<dyn Fn(&str, &str) + Send + Sync>;

pub(super) fn notify_thread_changed(observer: &ThreadObserver, project: &str, thread: &str) {
    // Notification failure must never turn a committed lifecycle mutation into
    // failed tracking or retain native ownership. The UI may reload later.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(project, thread)));
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Authentication {
    Unknown,
}
fn hook_executable() -> Result<PathBuf, &'static str> {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("shinatama-hook.exe"))
        })
        .ok_or("The session component is unavailable. Open the complete Shinatama package.")
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Unknown,
    Ready,
    Missing,
    Unavailable,
    Unsupported,
}
#[derive(Clone, Copy, Serialize)]
pub struct Step {
    state: StepState,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramState {
    Missing,
    Found,
    Unavailable,
}
#[derive(Clone, Serialize)]
pub struct Program {
    state: ProgramState,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}
/// The only UI projection. Strings are static/redacted except parsed CLI version.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCheckup {
    agent_id: &'static str,
    display_name: &'static str,
    program: Program,
    home: Step,
    sidecar: Step,
    sets: Step,
    authentication: Authentication,
    can_open: bool,
    can_login: bool,
    issues: Vec<&'static str>,
}
impl AgentCheckup {
    fn empty(agent_id: &'static str, display_name: &'static str) -> Self {
        Self {
            agent_id,
            display_name,
            program: Program {
                state: ProgramState::Unavailable,
                version: None,
            },
            home: Step {
                state: StepState::Unknown,
            },
            sidecar: Step {
                state: StepState::Unknown,
            },
            sets: Step {
                state: StepState::Unsupported,
            },
            authentication: Authentication::Unknown,
            can_open: false,
            can_login: false,
            issues: Vec::new(),
        }
    }
    fn issue(&mut self, message: &'static str) {
        if self.issues.len() < 8 && !self.issues.contains(&message) {
            self.issues.push(message);
        }
    }
}
struct BoxHome {
    _grant: PreparedPrivateStore,
    record: StoreRootRecord,
}
struct CanaryOwner {
    host: Arc<TerminalHost>,
    tracker: NativeTrialTracker,
    session_id: Mutex<Option<String>>,
    _base: Mutex<Option<StorageLocation>>,
    scratch: Mutex<OwnedScratch>,
    _keep_alive: Mutex<Option<Box<dyn Send>>>,
}
impl CanaryOwner {
    /// Call only after stop confirms release and temporary caller C01 pins have
    /// been dropped. A failed/unqualified trial has no receipt: preserve it.
    fn finish_released(
        &self,
        base: &StorageLocation,
    ) -> Result<Option<NativeTrialReceipt>, &'static str> {
        self._keep_alive
            .lock()
            .map_err(|_| "Canary attachment ownership is unavailable.")?
            .take();
        let receipt = self.tracker.take_receipt();
        if let Some(receipt) = receipt.as_ref() {
            self.cleanup_scratch(base, receipt);
        }
        Ok(receipt)
    }
    fn cleanup_scratch(&self, base: &StorageLocation, receipt: &NativeTrialReceipt) {
        // Successful receipt is issued only after real native/queue/SQL release.
        // Drop our C01 owner before opening DELETE; no prior directory is adopted.
        let Ok(mut location) = self._base.lock() else {
            return;
        };
        drop(location.take());
        drop(location);
        let Ok(mut scratch) = self.scratch.lock() else {
            return;
        };
        let mut report = check_scratch::cleanup(base, &mut scratch, receipt.cleanup_status());
        while report.issue == Some(check_scratch::Issue::InUse) && report.attempts_remaining > 0 {
            std::thread::sleep(Duration::from_millis(5));
            report = check_scratch::cleanup(base, &mut scratch, receipt.cleanup_status());
        }
    }
    fn stop(&self) -> bool {
        let id = self.session_id.lock().ok().and_then(|id| id.clone());
        if let Some(id) = id {
            return self.host.stop(&id).is_ok();
        }
        // The dedicated host tracks unannounced startup and retired sessions.
        // Its authoritative released state differs from an empty telemetry read.
        if self.host.fully_released() == Some(true) {
            return true;
        }
        let resources = self.host.resources();
        !resources.is_empty()
            && resources
                .iter()
                .all(|session| self.host.stop(&session.id).is_ok())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStarted {
    pub session: SessionInfo,
    pub thread_id: String,
    pub trial: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessCapabilities {
    pub id: &'static str,
    pub label: &'static str,
    pub native_resume: bool,
}
struct RegisteredService {
    service: Arc<dyn HarnessService>,
    busy: AtomicBool,
    blocked: AtomicBool,
}
struct RequestSlot(Arc<RegisteredService>);
impl Drop for RequestSlot {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}
impl RegisteredService {
    fn reserve(self: &Arc<Self>) -> Result<RequestSlot, &'static str> {
        if self.blocked.load(Ordering::Acquire) {
            return Err(
                "This harness stopped unexpectedly. Restart Shinatama before opening it again.",
            );
        }
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(
                |_| "An action for this harness is still returning. No duplicate was started.",
            )?;
        let slot = RequestSlot(Arc::clone(self));
        // Recheck after acquiring: the previous worker may have failed between
        // the first observation and releasing its slot.
        if self.blocked.load(Ordering::Acquire) {
            return Err(
                "This harness stopped unexpectedly. Restart Shinatama before opening it again.",
            );
        }
        Ok(slot)
    }
}
impl RequestSlot {
    fn run<T, E>(
        self,
        panic_error: E,
        action: impl FnOnce(&dyn HarnessService) -> Result<T, E>,
    ) -> Result<T, E> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            action(self.0.service.as_ref())
        })) {
            Ok(result) => result,
            Err(_) => {
                self.0.blocked.store(true, Ordering::Release);
                Err(panic_error)
            }
        }
    }
}
pub struct Controller {
    services: Arc<Services>,
    registry: BTreeMap<&'static str, Arc<RegisteredService>>,
    /// The host every agent terminal is started on; its registry is the only
    /// record of which agent processes are alive.
    host: Arc<TerminalHost>,
}
impl Controller {
    pub fn new(
        services: Arc<Services>,
        host: Arc<TerminalHost>,
        runtime: Arc<Runtime>,
        sidecar: Arc<sidecar::Controller>,
        thread_observer: ThreadObserver,
    ) -> Self {
        let context = Arc::new(ServiceContext::new(
            Arc::clone(&services),
            runtime,
            sidecar,
            thread_observer,
        ));
        let implementations: [Arc<dyn HarnessService>; 4] = [
            Arc::new(Service::new(Arc::clone(&context), claude::ClaudeDriver)),
            Arc::new(Service::new(Arc::clone(&context), codex::CodexDriver)),
            Arc::new(Service::new(Arc::clone(&context), opencode::OpenCodeDriver)),
            Arc::new(Service::new(context, pi::PiDriver)),
        ];
        let mut registry = BTreeMap::new();
        for service in implementations {
            let id = service.descriptor().id;
            let previous = registry.insert(
                id,
                Arc::new(RegisteredService {
                    service,
                    busy: AtomicBool::new(false),
                    blocked: AtomicBool::new(false),
                }),
            );
            assert!(
                previous.is_none(),
                "Each harness service must have a unique ID."
            );
        }
        Self {
            services,
            registry,
            host,
        }
    }
    /// Agent terminals whose process is alive right now; None when unreadable.
    pub fn live_agent_sessions(&self) -> Option<usize> {
        self.host.live_native_sessions()
    }
    fn registered(&self, id: &str) -> Result<Arc<RegisteredService>, &'static str> {
        self.registry
            .get(id)
            .cloned()
            .ok_or("This harness is not registered in this build.")
    }
    pub fn catalog(&self) -> Vec<HarnessCapabilities> {
        self.registry
            .values()
            .map(|entry| {
                let descriptor = entry.service.descriptor();
                HarnessCapabilities {
                    id: descriptor.id,
                    label: descriptor.label,
                    native_resume: descriptor.native_resume,
                }
            })
            .collect()
    }
    pub fn diagnostics(&self) -> serde_json::Value {
        let harnesses: serde_json::Map<String, serde_json::Value> = self
            .registry
            .iter()
            .map(|(id, entry)| {
                let mut facts = entry.service.diagnostics();
                if let Some(facts) = facts.as_object_mut() {
                    facts.insert("checking".into(), entry.busy.load(Ordering::Acquire).into());
                    facts.insert(
                        "blocked".into(),
                        entry.blocked.load(Ordering::Acquire).into(),
                    );
                }
                ((*id).into(), facts)
            })
            .collect();
        serde_json::json!({"harnesses": harnesses})
    }
    pub async fn check(self: &Arc<Self>, agent_id: &str) -> Result<AgentCheckup, &'static str> {
        let slot = self.registered(agent_id)?.reserve()?;
        tauri::async_runtime::spawn_blocking(move || {
            slot.run(
                "The harness check stopped unexpectedly. Saved data was preserved.",
                |service| service.check().map_err(|error| error.message()),
            )
        })
        .await
        .map_err(|_| "The harness worker is unavailable; no replacement was started.")?
    }
    pub async fn start(
        self: &Arc<Self>,
        agent_id: &str,
        host: Arc<TerminalHost>,
        project: PathBuf,
        rows: u16,
        cols: u16,
    ) -> Result<AgentStarted, &'static str> {
        self.start_request(
            agent_id,
            host,
            project,
            rows,
            cols,
            OpenMode::RequireQualified,
            None,
            false,
            None,
            None,
            None,
        )
        .await
    }
    /// `workset`: the thread (ADR-0055) the new session is created in; `None` gives it
    /// a thread of its own.
    pub async fn open(
        self: &Arc<Self>,
        agent_id: &str,
        host: Arc<TerminalHost>,
        project: PathBuf,
        rows: u16,
        cols: u16,
        workset: Option<String>,
    ) -> Result<AgentStarted, &'static str> {
        self.start_request(
            agent_id,
            host,
            project,
            rows,
            cols,
            OpenMode::PrepareWhenNeeded,
            None,
            false,
            None,
            workset,
            None,
        )
        .await
    }
    /// `open` for the destination of a hand-off: `control_header` declares the first prompt to the
    /// model proxy (`x-shinatama-control`). Only a harness that can add a request header uses it.
    pub async fn open_handoff(
        self: &Arc<Self>,
        agent_id: &str,
        host: Arc<TerminalHost>,
        project: PathBuf,
        rows: u16,
        cols: u16,
        control_header: String,
    ) -> Result<AgentStarted, &'static str> {
        self.start_request(
            agent_id,
            host,
            project,
            rows,
            cols,
            OpenMode::PrepareWhenNeeded,
            None,
            false,
            Some(control_header),
            None,
            None,
        )
        .await
    }
    async fn start_request(
        self: &Arc<Self>,
        agent_id: &str,
        host: Arc<TerminalHost>,
        project: PathBuf,
        rows: u16,
        cols: u16,
        mode: OpenMode,
        initial_prompt: Option<String>,
        require_mukade: bool,
        control_header: Option<String>,
        workset: Option<String>,
        model: Option<Option<Selection>>,
    ) -> Result<AgentStarted, &'static str> {
        shinatama_terminal::validate_size(rows, cols)
            .map_err(|_| "The terminal size is invalid.")?;
        let began = std::time::Instant::now();
        let result = match self.registered(agent_id).and_then(|entry| entry.reserve()) {
            Err(message) => Err(message),
            Ok(slot) => tauri::async_runtime::spawn_blocking(move || {
                slot.run(
                    "Harness startup stopped unexpectedly. Its owned resources remain tracked.",
                    |service| {
                        service
                            .open(
                                &host,
                                &project,
                                rows,
                                cols,
                                SessionOperation::New,
                                mode,
                                initial_prompt.as_deref(),
                                require_mukade,
                                control_header.as_deref(),
                                workset.as_deref(),
                                model.as_ref().map(Option::as_ref),
                            )
                            .map_err(|error| error.message())
                    },
                )
            })
            .await
            .unwrap_or(Err(
                "The harness worker is unavailable; no replacement was started.",
            )),
        };
        let elapsed = began.elapsed().as_millis();
        crate::app_log::write(
            "open",
            &match &result {
                Ok(_) => format!("{agent_id} opened after {elapsed} ms."),
                Err(message) => format!("{agent_id} refused after {elapsed} ms: {message}"),
            },
        );
        result
    }

    /// A bounded instruction, never caller-supplied argv, uses the same native
    /// preparation, admission and terminal ownership as a user-opened tab.
    pub async fn open_collaboration(
        self: &Arc<Self>,
        agent_id: &str,
        host: Arc<TerminalHost>,
        project: PathBuf,
        initial_prompt: String,
        require_mukade: bool,
        workset: Option<String>,
    ) -> Result<AgentStarted, &'static str> {
        if !matches!(agent_id, "claude_code" | "codex") {
            return Err("This harness does not support collaborative startup.");
        }
        if initial_prompt.trim().is_empty()
            || initial_prompt.len() > 8192
            || initial_prompt.contains('\0')
        {
            return Err("The initial collaboration instruction is invalid.");
        }
        self.start_request(
            agent_id,
            host,
            project,
            24,
            80,
            OpenMode::PrepareWhenNeeded,
            Some(initial_prompt),
            require_mukade,
            None,
            workset,
            None,
        )
        .await
    }

    /// A session of `program` for an agent of a thread's team, created in `workset` with the
    /// agent's model and a first message. `own_model`: the program's own sign-in and model, and
    /// `model` is then ignored; otherwise `model` is the Models panel's entry the program runs
    /// (only OpenCode and Pi take one). The thread's room is joined through the launch's hooks as
    /// for any session of a talking thread; a failure of the chat never refuses the launch.
    pub async fn open_agent(
        self: &Arc<Self>,
        program: &str,
        host: Arc<TerminalHost>,
        project: PathBuf,
        model: Option<Selection>,
        own_model: bool,
        initial_prompt: String,
        workset: String,
    ) -> Result<AgentStarted, &'static str> {
        if !matches!(program, "claude_code" | "codex" | "opencode" | "pi") {
            return Err("This program cannot be opened for a team.");
        }
        if initial_prompt.trim().is_empty()
            || initial_prompt.len() > 8192
            || initial_prompt.contains('\0')
        {
            return Err("The agent's first message is invalid.");
        }
        let model = match (own_model, model) {
            (true, _) => Some(None),
            (false, Some(selection)) if matches!(program, "opencode" | "pi") => {
                Some(Some(selection))
            }
            (false, Some(_)) => {
                return Err("This program has no model setting: use its own sign-in and model.");
            }
            (false, None) => return Err("The agent has no model."),
        };
        self.start_request(
            program,
            host,
            project,
            24,
            80,
            OpenMode::PrepareWhenNeeded,
            Some(initial_prompt),
            false,
            None,
            Some(workset),
            model,
        )
        .await
    }
}
fn prepare_box(
    database: &Database,
    location: &StorageLocation,
    harness: &str,
    request: PrivateStoreRequest,
) -> Result<BoxHome, &'static str> {
    let plan = PrivateStorePlan::resolve(
        location,
        AgentStoreKey::new(harness).map_err(|_| "The private agent namespace is invalid.")?,
        request,
    )
    .map_err(|error| -> &'static str {
        // Say which check refused: the bare sentence left nothing to act on. The cause is
        // bounded and control-free; the one-off text is leaked (a refusal is rare).
        let cause: String = error
            .to_string()
            .chars()
            .filter(|ch| !ch.is_control())
            .take(240)
            .collect();
        Box::leak(
            format!(
                "The boxed home failed local/private path verification ({cause}). No system home was used."
            )
            .into_boxed_str(),
        )
    })?;
    // Inspect the authoritative binding BEFORE prepare() can recreate a child.
    // A missing or disconnected home needs an explicit reconnect, preserving its
    // previous identity and native session references.
    if let Some(root) = latest_box(database, harness)? {
        let expected = plan
            .paths()
            .home()
            .to_str()
            .ok_or("The boxed home cannot be represented by this build.")?;
        let expected = expected.strip_prefix(r"\\?\").unwrap_or(expected);
        if !root.available || root.access != RootAccess::ReadWrite || root.resolved_path != expected
        {
            return Err(
                "The saved boxed-home binding is unavailable or differs. Reconnect it explicitly; history was not remapped.",
            );
        }
        if !plan
            .paths()
            .home()
            .try_exists()
            .map_err(|_| "The saved boxed home could not be inspected safely.")?
        {
            database.mark_store_root_unavailable(&root.id, root.config_revision)
                .map_err(|_| "The missing boxed home could not be marked unavailable. No directory was recreated.")?;
            return Err(
                "The saved boxed home is missing. Reconnect it explicitly; no replacement directory was created.",
            );
        }
    }
    let grant = plan.prepare().map_err(|_| "The boxed home could not be prepared privately. Existing data and ACLs were preserved.")?;
    let path = grant
        .paths()
        .home()
        .to_str()
        .ok_or("The boxed home cannot be represented by this build.")?;
    let ordinary = path.strip_prefix(r"\\?\").unwrap_or(path);
    let prepared = PreparedStoreRoot::resolve(StoreRootRecord {
        id: uuid::Uuid::new_v4().to_string(),
        harness: harness.into(),
        kind: RootKind::Box,
        scope: RootScope::Primary,
        resolved_path: ordinary.into(),
        origin: RootOrigin::Configured,
        config_revision: 1,
        access: RootAccess::ReadWrite,
        available: true,
    })
    .map_err(|_| "The boxed home could not be resolved for its authoritative store record.")?;
    let find = || -> Result<Option<StoreRootRecord>, &'static str> {
        let latest = latest_box(database, harness)?;
        match latest {
            Some(root)
                if root.available
                    && root.access == RootAccess::ReadWrite
                    && root.resolved_path == prepared.record().resolved_path =>
            {
                Ok(Some(root))
            }
            Some(_) => Err(
                "The saved boxed-home binding is unavailable or differs. Reconnect it explicitly; history was not remapped.",
            ),
            None => Ok(None),
        }
    };
    let record = match find()? {
        Some(record) => record,
        None => match database.register_store_root(&prepared) {
            Ok(record) => record,
            Err(RecordsError::Conflict) => find()?
                .ok_or("Another window changed the boxed-home registration. Retry explicitly.")?,
            Err(_) => {
                return Err(
                    "The boxed-home store record could not be saved. No existing binding was changed.",
                );
            }
        },
    };
    Ok(BoxHome {
        _grant: grant,
        record,
    })
}
fn latest_box(database: &Database, harness: &str) -> Result<Option<StoreRootRecord>, &'static str> {
    let records = database
        .list_store_roots(None, ROOT_SCAN_LIMIT)
        .map_err(|_| "Saved agent roots could not be read safely.")?;
    if records.len() == ROOT_SCAN_LIMIT {
        return Err("Saved root history exceeds the bounded checkup page. No binding was changed.");
    }
    Ok(records
        .into_iter()
        .filter(|root| {
            root.harness == harness
                && root.kind == RootKind::Box
                && root.scope == RootScope::Primary
        })
        .max_by_key(|root| root.config_revision))
}
/// The set a real session starts with: Marionette when the driver supports it, a
/// synced copy is current and the user did not switch it off
/// (`SHINATAMA_MARIONETTE=off`); otherwise the empty set. The launcher checks the
/// host's adapter files; a missing or altered copy never blocks a launch, it only
/// drops Marionette.
/// Marionette for `harness` when its adapter in the synced store passes its digest check;
/// otherwise the empty set, so the session starts without Marionette (as AVVIO.md says) instead
/// of being refused: one stray file in the store refused every OpenCode launch until a restart.
fn session_set(
    supported: bool,
    harness: &str,
) -> Result<(SetManifest, Option<std::path::PathBuf>), &'static str> {
    use shinatama_engine::marionette::{self, Host, Marionette, MarionetteError};
    if supported
        && !marionette::switched_off()
        && let Some(store) = crate::marionette::store()
    {
        let host = match harness {
            "claude_code" => Some(Host::ClaudeCode),
            "codex" => Some(Host::Codex),
            "opencode" => Some(Host::OpenCode),
            "pi" => Some(Host::Pi),
            _ => None,
        };
        let verified = Marionette::current(store).and_then(|release| {
            release
                .verify(host.ok_or(MarionetteError::Missing)?)
                .map(|_| ())
        });
        match verified {
            Ok(()) => return Ok((marionette::marionette_set(), Some(store.to_path_buf()))),
            Err(error) => crate::app_log::write(
                "marionette",
                &format!(
                    "{harness} starts without Marionette: {error}; the next start repairs the copy."
                ),
            ),
        }
    }
    Ok((empty_set()?, None))
}
fn empty_set() -> Result<SetManifest, &'static str> {
    SetManifest::new(SET_SCHEMA_VERSION, "default", Vec::new())
        .map_err(|_| "The default empty set is unavailable.")
}
fn check_directory_budget(base: &StorageLocation, prefix: &str) -> Result<(), &'static str> {
    let entries = base
        .directory()
        .read_dir()
        .map_err(|_| "Private check-directory information is unavailable.")?;
    let mut retained = 0;
    for (index, entry) in entries.take(129).enumerate() {
        if index == 128 {
            return Err(
                "Private check-directory inspection reached its finite limit. No new check was started.",
            );
        }
        let entry =
            entry.map_err(|_| "A private check directory could not be inspected safely.")?;
        if entry.file_name().to_string_lossy().starts_with(prefix) {
            retained += 1;
        }
        if retained >= 8 {
            return Err(
                "Eight private check directories remain preserved. Archive them explicitly before further checks; no new check was started.",
            );
        }
    }
    Ok(())
}
#[derive(Default)]
struct CursorReplies {
    tail: Vec<u8>,
    replies: u8,
}
impl CursorReplies {
    fn answer(&mut self, host: &TerminalHost, session: &str, bytes: &[u8]) {
        // Only protocol answers to exact DSR requests. No Enter, navigation,
        // login response, prompt text or provider message is synthesized.
        for byte in bytes {
            if self.replies >= 16 {
                return;
            }
            self.tail.push(*byte);
            if self.tail.len() > 6 {
                self.tail.remove(0);
            }
            let reply = if self.tail.ends_with(b"\x1b[?6n") {
                Some(b"\x1b[?1;1R".as_slice())
            } else if self.tail.ends_with(b"\x1b[6n") {
                Some(b"\x1b[1;1R".as_slice())
            } else if self.tail.ends_with(b"\x1b[5n") {
                Some(b"\x1b[0n".as_slice())
            } else {
                None
            };
            if let Some(reply) = reply {
                let _ = host.write(session, reply.to_vec());
                self.replies += 1;
                self.tail.clear();
            }
        }
    }
}
