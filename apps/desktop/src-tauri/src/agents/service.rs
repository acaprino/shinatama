//! Shared blocking harness workflow. Tauri schedules it; drivers supply recipes.
use super::{
    AgentCheckup, AgentStarted, BoxHome, CanaryOwner, CursorReplies, ProgramState, StepState,
    ThreadObserver, check_directory_budget, empty_set, prepare_box, session_set,
};
use crate::{sidecar, storage::Services, terminal_runtime::Runtime};
use serde_json::Value;
use shinatama_adapter::{catalog::ProgramCandidate, sets::SetManifest};
use shinatama_engine::{
    check_scratch::{self, ScratchKind},
    launch::PreparedLaunch,
    native_launch::{NativeTrialReceipt, NativeTrialTracker, NativeTuiPermit},
    resources::admission::PreparedWorkload,
    sidecar_supervisor::SidecarSupervisor,
};
use shinatama_state::{
    database::{Database, DatabaseAccess},
    location::StorageLocation,
    records::{CanonicalIdKind, StoreRootRecord, ThreadKind, new_id},
    resource_reservations::Budget,
    store_roots::{AgentStoreKey, PreparedPrivateStore, PrivateStorePlan, PrivateStoreRequest},
    threads::{ThreadRecord, ThreadResumeTarget},
};
use shinatama_terminal::{TerminalHost, WindowsNativeSessionLifecycle};
use std::{
    fmt,
    num::NonZeroU16,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy)]
pub(super) struct HarnessDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub native_resume: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DriverError {
    Missing,
    AdmissionDeferred,
    StaleQualification,
    Failed(&'static str),
}
pub(super) type ServiceError = DriverError;
impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}
impl DriverError {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Missing => "This harness is not installed. Install it and try Open again.",
            Self::AdmissionDeferred => {
                "Windows is low on resources. Close an unused app and try Open again."
            }
            Self::StaleQualification => {
                "The harness build or private home changed. Try Open again."
            }
            Self::Failed(message) => message,
        }
    }
}
impl DriverError {
    /// A refused admission names its cause (bounded, control-free) instead of a bare "low on
    /// resources": the cause is either measured Windows memory or Shinatama's own count of
    /// open sessions, and only the cause can say which. The one-off text is leaked; a refusal
    /// is rare and user-initiated.
    pub(super) fn deferred(cause: String) -> Self {
        let cause: String = cause.chars().filter(|ch| !ch.is_control()).take(240).collect();
        Self::Failed(Box::leak(
            format!("The harness check could not start ({cause}). Try Open again.")
                .into_boxed_str(),
        ))
    }
}
impl From<&'static str> for DriverError {
    fn from(value: &'static str) -> Self {
        Self::Failed(value)
    }
}
#[derive(Clone)]
pub(super) enum SessionOperation {
    New,
    Resume(ThreadResumeTarget),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenMode {
    PrepareWhenNeeded,
    RequireQualified,
}
#[derive(Clone)]
pub(super) struct ProbeProjection {
    pub version: Option<String>,
    pub issue: Option<DriverError>,
    pub diagnostics: Value,
}
pub(super) trait ProbeEvidence: Send + Sync + 'static {
    fn projection(&self) -> ProbeProjection;
    fn cleanup_retained(&self) -> bool;
    fn retry_cleanup(&mut self, database: &Database);
}
pub(super) struct HarnessPolicy {
    pub probe_prefix: &'static str,
    pub trial_prefix: &'static str,
    pub trial_kind: ScratchKind,
    pub probe_budget: Budget,
}
pub(super) struct ProbeContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub base: &'a StorageLocation,
    pub database: &'a Database,
}
pub(super) struct TrialContext<'a, P> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a P,
    pub base: &'a StorageLocation,
    pub trial_base: &'a StorageLocation,
    pub private_home: PreparedPrivateStore,
    pub project: &'a Path,
    pub sidecar: &'a SidecarSupervisor,
    pub proxy_port: NonZeroU16,
    pub ca_file: &'a Path,
    pub selected_set: &'a SetManifest,
    pub thread_id: &'a str,
}
pub(super) struct SessionContext<'a, P> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a P,
    pub base: &'a StorageLocation,
    pub database: &'a Database,
    pub private_home: PreparedPrivateStore,
    pub root: StoreRootRecord,
    pub project: &'a Path,
    pub sidecar: &'a SidecarSupervisor,
    pub proxy_port: NonZeroU16,
    pub ca_file: &'a Path,
    pub selected_set: &'a SetManifest,
    pub plugins_root: Option<&'a Path>,
    /// `x-shinatama-control: <kind>;<hash>` of the first prompt of a hand-off, when this launch is one.
    pub control_header: Option<&'a str>,
    pub thread: &'a ThreadRecord,
    pub initial_prompt: Option<&'a str>,
    pub require_mukade: bool,
}
pub(super) struct BindingContext<'a> {
    pub database: Arc<Database>,
    pub thread: &'a ThreadRecord,
    pub root: &'a StoreRootRecord,
    pub operation: &'a SessionOperation,
    pub thread_observer: ThreadObserver,
}
pub(super) struct PreparedTrial {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    pub tracker: NativeTrialTracker,
    pub keep_alive: Option<Box<dyn Send>>,
}
pub(super) struct SealedSession {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
}
pub(super) trait HarnessDriver: Send + Sync + 'static {
    type Probe: ProbeEvidence;
    type PreparedSession: Into<SealedSession>;
    fn descriptor(&self) -> HarnessDescriptor;
    fn policy(&self) -> HarnessPolicy;
    fn private_store_request(&self) -> PrivateStoreRequest;
    /// True once the harness launcher can carry the bundled plugin set.
    fn bundled_plugins(&self) -> bool {
        false
    }
    fn discover(&self) -> Result<ProgramCandidate, DriverError>;
    fn probe(
        &self,
        context: ProbeContext<'_>,
        admit: &mut dyn FnMut() -> Result<PreparedWorkload, DriverError>,
    ) -> Result<Self::Probe, DriverError>;
    fn prepare_trial(
        &self,
        context: TrialContext<'_, Self::Probe>,
    ) -> Result<PreparedTrial, DriverError>;
    fn prepare_session(
        &self,
        context: SessionContext<'_, Self::Probe>,
        receipt: &NativeTrialReceipt,
        operation: &SessionOperation,
    ) -> Result<Self::PreparedSession, DriverError>;
    fn bind_lifecycle(
        &self,
        _prepared: &mut Self::PreparedSession,
        _binding: BindingContext<'_>,
    ) -> Result<Option<Arc<dyn WindowsNativeSessionLifecycle>>, DriverError> {
        Ok(None)
    }
}
pub(super) trait HarnessService: Send + Sync {
    fn descriptor(&self) -> HarnessDescriptor;
    fn diagnostics(&self) -> Value;
    fn check(&self) -> Result<AgentCheckup, ServiceError>;
    fn open(
        &self,
        host: &TerminalHost,
        project: &Path,
        rows: u16,
        cols: u16,
        operation: SessionOperation,
        mode: OpenMode,
        initial_prompt: Option<&str>,
        require_mukade: bool,
        control_header: Option<&str>,
    ) -> Result<AgentStarted, ServiceError>;
}
pub(super) struct ServiceContext {
    pub services: Arc<Services>,
    pub runtime: Arc<Runtime>,
    pub sidecar: Arc<sidecar::Controller>,
    pub thread_observer: ThreadObserver,
}
impl ServiceContext {
    pub(super) fn new(
        services: Arc<Services>,
        runtime: Arc<Runtime>,
        sidecar: Arc<sidecar::Controller>,
        thread_observer: ThreadObserver,
    ) -> Self {
        Self {
            services,
            runtime,
            sidecar,
            thread_observer,
        }
    }
    fn ensure_sidecar(&self) -> Result<(), ServiceError> {
        if self.sidecar.qualified_owner().is_ok() {
            return Ok(());
        }
        let previous = self.sidecar.status();
        // Start would cancel a requested stop; a new agent never overrides it.
        if previous.phase == "draining" {
            return Err(DriverError::Failed(sidecar::STOPPING));
        }
        let observed = previous
            .observation
            .as_ref()
            .and_then(|value| value.observed_at_unix_ms);
        self.sidecar.start()?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let status = self.sidecar.status();
            let changed = status.phase != previous.phase
                || status
                    .observation
                    .as_ref()
                    .and_then(|value| value.observed_at_unix_ms)
                    != observed;
            if sidecar::launch_allowed(status.phase, status.healthy) {
                return self
                    .sidecar
                    .qualified_owner()
                    .map(|_| ())
                    .map_err(Into::into);
            }
            if changed
                && status.observation.as_ref().is_some_and(|snapshot| {
                    matches!(
                        snapshot.reason,
                        shinatama_engine::sidecar_supervisor::Reason::AdmissionDeferred
                            | shinatama_engine::sidecar_supervisor::Reason::ResourceLimit
                    )
                })
            {
                return Err(DriverError::AdmissionDeferred);
            }
            if changed && matches!(status.phase, "failed" | "retired") {
                return Err(DriverError::Failed(
                    status
                        .message
                        .unwrap_or("The local session service could not be prepared."),
                ));
            }
            if Instant::now() >= deadline {
                return Err(DriverError::Failed(
                    "Session preparation is still finishing. Try Open again in a few seconds.",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fn wait_for_qualified_sidecar(
        &self,
    ) -> Result<(&SidecarSupervisor, NonZeroU16, PathBuf), ServiceError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.sidecar.qualified_owner() {
                Ok(owner) => return Ok(owner),
                Err(message)
                    if matches!(
                        self.sidecar.status().phase,
                        "stopped" | "failed" | "retired" | "draining"
                    ) || Instant::now() >= deadline =>
                {
                    return Err(message.into());
                }
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }
}
struct Observation<P> {
    candidate: ProgramCandidate,
    outcome: Arc<P>,
    root: StoreRootRecord,
}
impl<P> Clone for Observation<P> {
    fn clone(&self) -> Self {
        Self {
            candidate: self.candidate.clone(),
            outcome: Arc::clone(&self.outcome),
            root: self.root.clone(),
        }
    }
}
struct Facts<P> {
    home: Option<BoxHome>,
    retained_probe: Option<P>,
    observed: Option<Observation<P>>,
    receipt: Option<Arc<NativeTrialReceipt>>,
    canary: Option<Arc<CanaryOwner>>,
    last_probe: Option<ProbeProjection>,
    last_error: Option<DriverError>,
    last_start_failure: Option<String>,
}
impl<P> Default for Facts<P> {
    fn default() -> Self {
        Self {
            home: None,
            retained_probe: None,
            observed: None,
            receipt: None,
            canary: None,
            last_probe: None,
            last_error: None,
            last_start_failure: None,
        }
    }
}
pub(super) struct Service<D: HarnessDriver> {
    context: Arc<ServiceContext>,
    driver: D,
    facts: Mutex<Facts<D::Probe>>,
}
impl<D: HarnessDriver> Service<D> {
    pub(super) fn new(context: Arc<ServiceContext>, driver: D) -> Self {
        Self {
            context,
            driver,
            facts: Mutex::new(Facts::default()),
        }
    }
    fn facts(&self) -> Result<std::sync::MutexGuard<'_, Facts<D::Probe>>, ServiceError> {
        self.facts
            .lock()
            .map_err(|_| DriverError::Failed("Harness ownership information is unavailable."))
    }
    fn remember(&self, error: DriverError) {
        if let Ok(mut facts) = self.facts.lock() {
            facts.last_error = Some(error);
            if facts.last_start_failure.is_none() {
                facts.last_start_failure = Some(error.message().into());
            }
        }
    }
    fn native_failure(&self, error: impl fmt::Display) {
        if let Ok(mut facts) = self.facts.lock() {
            facts.last_start_failure = Some(
                error
                    .to_string()
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(240)
                    .collect(),
            );
        }
    }
    fn project_probe(report: &mut AgentCheckup, projection: &ProbeProjection) {
        report.program.state = ProgramState::Found;
        report.program.version = projection.version.clone();
        if let Some(issue) = projection.issue {
            report.issue(issue.message());
        }
    }
    fn check_impl(&self) -> Result<AgentCheckup, ServiceError> {
        {
            let mut facts = self.facts()?;
            facts.observed = None;
            facts.receipt = None;
            facts.last_probe = None;
            facts.last_error = None;
            facts.last_start_failure = None;
        }
        let descriptor = self.driver.descriptor();
        let policy = self.driver.policy();
        let mut report = AgentCheckup::empty(descriptor.id, descriptor.label);
        let sidecar = self.context.sidecar.status();
        report.sidecar.state = if sidecar.healthy && matches!(sidecar.phase, "ready" | "busy") {
            StepState::Ready
        } else if sidecar.phase == "stopped" {
            StepState::Missing
        } else {
            StepState::Unavailable
        };
        if !matches!(report.sidecar.state, StepState::Ready) {
            report.issue("The local session service is unavailable.");
        }
        report.issue(
            "Shinatama did not query authentication. Sign-in stays in the original agent terminal.",
        );
        let state = self.context.services.load().map_err(|_| {
            DriverError::Failed(
                "Private application storage is unavailable; no alternative home was used.",
            )
        })?;
        if !matches!(state.store.access(), DatabaseAccess::ReadWrite) {
            report.home.state = StepState::Unavailable;
            report.issue(
                "Update Shinatama before preparing this harness with the newer saved database.",
            );
            return Ok(report);
        }
        state.store.check_write_admission().map_err(|_| {
            DriverError::Failed("Storage admission is unavailable; no harness files were changed.")
        })?;
        let canary = self.facts()?.canary.clone();
        if let Some(prior) = canary {
            if !prior.stop() {
                let error = DriverError::Failed(
                    "The previous TUI check has unconfirmed cleanup. Its owner is retained; no replacement was started.",
                );
                self.remember(error);
                report.issue(error.message());
                return Ok(report);
            }
            // A previous stop may have returned before the terminal worker
            // published its release receipt. Consume it before dropping owner.
            prior.finish_released(&state.location)?;
            self.facts()?.canary = None;
        }
        let retained = self.facts()?.retained_probe.take();
        if let Some(mut prior) = retained {
            prior.retry_cleanup(&state.store);
            if prior.cleanup_retained() {
                let projection = prior.projection();
                Self::project_probe(&mut report, &projection);
                let error = DriverError::Failed(
                    "The previous version probe has unconfirmed cleanup. Its budget remains held; no replacement was started.",
                );
                {
                    let mut facts = self.facts()?;
                    facts.last_probe = Some(projection);
                    facts.retained_probe = Some(prior);
                }
                self.remember(error);
                report.issue(error.message());
                return Ok(report);
            }
        }
        let candidate = match self.driver.discover() {
            Ok(candidate) => candidate,
            Err(error) => {
                report.program.state = if error == DriverError::Missing {
                    ProgramState::Missing
                } else {
                    ProgramState::Unavailable
                };
                self.remember(error);
                report.issue(error.message());
                return Ok(report);
            }
        };
        report.program.state = ProgramState::Found;
        match prepare_box(
            &state.store,
            &state.location,
            descriptor.id,
            self.driver.private_store_request(),
        ) {
            Ok(home) => {
                report.home.state = StepState::Ready;
                self.facts()?.home = Some(home);
            }
            Err(message) => {
                report.home.state = StepState::Unavailable;
                self.remember(message.into());
                report.issue(message);
                return Ok(report);
            }
        }
        check_directory_budget(&state.location, policy.probe_prefix)?;
        let mut admit = || {
            self.context
                .runtime
                .prepare_interactive_support_workload(policy.probe_budget, 256 * 1024 * 1024, 15_000)
                .map_err(DriverError::deferred)
        };
        let outcome = match self.driver.probe(
            ProbeContext {
                candidate: &candidate,
                base: &state.location,
                database: &state.store,
            },
            &mut admit,
        ) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.remember(error);
                report.issue(error.message());
                return Ok(report);
            }
        };
        let projection = outcome.projection();
        Self::project_probe(&mut report, &projection);
        let observation = {
            let mut facts = self.facts()?;
            facts.last_probe = Some(projection.clone());
            facts.last_error = projection.issue;
            if outcome.cleanup_retained() {
                facts.retained_probe = Some(outcome);
                None
            } else if projection.issue.is_none() && projection.version.is_some() {
                let root = facts
                    .home
                    .as_ref()
                    .ok_or(DriverError::Failed(
                        "The private home owner is unavailable.",
                    ))?
                    .record
                    .clone();
                let observation = Observation {
                    candidate,
                    outcome: Arc::new(outcome),
                    root,
                };
                facts.observed = Some(observation.clone());
                Some(observation)
            } else {
                None
            }
        };
        if let Some(observation) = observation
            && matches!(report.sidecar.state, StepState::Ready)
        {
            match self.run_canary(&state.location, &state.store, &observation) {
                Ok(receipt) => {
                    self.facts()?.receipt = Some(Arc::new(receipt));
                    report.sets.state = StepState::Ready;
                    report.can_open = true;
                    report.can_login = false;
                    report.issue("The original TUI passed its isolated startup and cleanup check. Sign-in remains in that TUI.");
                    report.issue("Qualified scope: original TUI startup and an empty optional set. Provider traffic and actual tool use remain outside this check.");
                    return Ok(report);
                }
                Err(error) => {
                    self.remember(error);
                    report.issue(error.message());
                }
            }
        }
        report.issue(
            "Opening remains unavailable until the exact build passes native qualification.",
        );
        Ok(report)
    }
    fn qualify(&self) -> Result<(), ServiceError> {
        let report = self.check_impl()?;
        if report.can_open {
            return Ok(());
        }
        if matches!(report.program.state, ProgramState::Missing) {
            return Err(DriverError::Missing);
        }
        Err(self.facts()?.last_error.unwrap_or(DriverError::Failed(
            "The harness did not finish preparing. Try Open again.",
        )))
    }
    fn reusable(&self) -> Result<bool, ServiceError> {
        let current = self.driver.discover().ok();
        let facts = self.facts()?;
        Ok(facts.canary.is_none()
            && facts.retained_probe.is_none()
            && facts.receipt.is_some()
            && facts
                .observed
                .as_ref()
                .zip(current.as_ref())
                .is_some_and(|(old, current)| {
                    old.candidate.path == current.path && old.candidate.kind == current.kind
                }))
    }
    fn open_impl(
        &self,
        host: &TerminalHost,
        project: &Path,
        rows: u16,
        cols: u16,
        operation: SessionOperation,
        mode: OpenMode,
        initial_prompt: Option<&str>,
        require_mukade: bool,
        control_header: Option<&str>,
    ) -> Result<AgentStarted, ServiceError> {
        shinatama_terminal::validate_size(rows, cols)
            .map_err(|_| DriverError::Failed("The terminal size is invalid."))?;
        let descriptor = self.driver.descriptor();
        if matches!(operation, SessionOperation::Resume(_)) && !descriptor.native_resume {
            return Err(DriverError::Failed(
                "Native Resume is unavailable for this harness.",
            ));
        }
        let state = self
            .context
            .services
            .load()
            .map_err(|_| DriverError::Failed("Saved projects are unavailable."))?;
        state.location.validate_project(project).map_err(|_| {
            DriverError::Failed("Choose an existing project folder and try Open again.")
        })?;
        state.store.check_write_admission().map_err(|_| {
            DriverError::Failed("Storage admission is unavailable; no harness process was started.")
        })?;
        if mode == OpenMode::PrepareWhenNeeded {
            self.context.ensure_sidecar()?;
            if !self.reusable()? {
                self.qualify()?;
            }
        }
        let selected = state.store.select(project).map_err(|_| {
            DriverError::Failed("The selected project could not be saved. No harness was started.")
        })?;
        let requested_thread = match &operation {
            SessionOperation::New => {
                let at = now()?;
                ThreadRecord {
                    id: new_id(CanonicalIdKind::Thread),
                    project_id: selected.id.clone(),
                    kind: ThreadKind::Work,
                    parent_thread_id: None,
                    companion_worker_thread_id: None,
                    title: Some(descriptor.label.into()),
                    archived: false,
                    revision: 1,
                    created_at: at,
                    updated_at: at,
                }
            }
            SessionOperation::Resume(target) => {
                if target.thread.project_id != selected.id
                    || target.segment.harness != descriptor.id
                    || target.root.harness != descriptor.id
                    || target.recorded_activation.is_some()
                {
                    return Err(DriverError::Failed(
                        "The saved conversation does not match the selected project or harness, or is already owned.",
                    ));
                }
                let current = state
                    .store
                    .thread_resume_target(&target.thread.id)
                    .map_err(|_| {
                        DriverError::Failed("The saved conversation binding is unavailable.")
                    })?;
                if current != *target {
                    return Err(DriverError::Failed(
                        "The saved conversation changed. Select it again before Resume.",
                    ));
                }
                target.thread.clone()
            }
        };
        // Refresh only preparation, at most once and before any native start.
        // A running/failed-started process is never repeated by this loop.
        for attempt in 0..2 {
            let (observation, receipt) = {
                let facts = self.facts()?;
                if facts.canary.is_some() || facts.retained_probe.is_some() {
                    return Err(DriverError::Failed(
                        "A harness check still owns unconfirmed resources. No new process was started.",
                    ));
                }
                (
                    facts
                        .observed
                        .clone()
                        .ok_or(DriverError::StaleQualification)?,
                    facts
                        .receipt
                        .clone()
                        .ok_or(DriverError::StaleQualification)?,
                )
            };
            state.store.check_write_admission().map_err(|_| {
                DriverError::Failed(
                    "Storage admission is unavailable; no harness process was started.",
                )
            })?;
            state.location.validate_project(project).map_err(|_| {
                DriverError::Failed("The selected project cannot be used for this harness.")
            })?;
            let home = prepare_box(
                &state.store,
                &state.location,
                descriptor.id,
                self.driver.private_store_request(),
            )?;
            if let SessionOperation::Resume(target) = &operation
                && home.record != target.root
            {
                return Err(DriverError::Failed(
                    "The saved conversation's exact private home is unavailable. No other home was used.",
                ));
            }
            if home.record != observation.root {
                if mode == OpenMode::PrepareWhenNeeded && attempt == 0 {
                    self.qualify()?;
                    continue;
                }
                return Err(DriverError::StaleQualification);
            }
            let (sidecar, port, ca) = self.context.wait_for_qualified_sidecar()?;
            let (selected_set, plugins_root) = session_set(self.driver.bundled_plugins())?;
            let root = home.record.clone();
            let prepared = self.driver.prepare_session(
                SessionContext {
                    candidate: &observation.candidate,
                    observation: &observation.outcome,
                    base: &state.location,
                    database: &state.store,
                    private_home: home._grant,
                    root: home.record,
                    project,
                    sidecar,
                    proxy_port: port,
                    ca_file: &ca,
                    selected_set: &selected_set,
                    plugins_root: plugins_root.as_deref(),
                    control_header,
                    thread: &requested_thread,
                    initial_prompt,
                    require_mukade,
                },
                &receipt,
                &operation,
            );
            let mut prepared = match prepared {
                Ok(prepared) => prepared,
                Err(DriverError::StaleQualification)
                    if mode == OpenMode::PrepareWhenNeeded && attempt == 0 =>
                {
                    self.qualify()?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            self.context.wait_for_qualified_sidecar()?;
            self.context
                .runtime
                .wait_for_interactive_admission()
                .map_err(DriverError::deferred)?;
            self.context.runtime.configure(host).map_err(|_| {
                DriverError::Failed("Contained native terminal admission is unavailable.")
            })?;
            let thread = match &operation {
                SessionOperation::New => {
                    state.store.create_thread(&requested_thread).map_err(|_| {
                        DriverError::Failed(
                            "The work thread could not be saved. No harness was started.",
                        )
                    })?
                }
                SessionOperation::Resume(_) => requested_thread.clone(),
            };
            let lifecycle = self.driver.bind_lifecycle(
                &mut prepared,
                BindingContext {
                    database: Arc::clone(&state.store),
                    thread: &thread,
                    root: &root,
                    operation: &operation,
                    thread_observer: Arc::clone(&self.context.thread_observer),
                },
            )?;
            let prepared: SealedSession = prepared.into();
            if prepared.launch.identity_record().thread_id != thread.id {
                return Err(DriverError::Failed(
                    "The sealed launch does not match the saved thread.",
                ));
            }
            let session = host.start_prepared_with_lifecycle(prepared.launch, prepared.permit, rows, cols, lifecycle)
                .map_err(|error| {
                    // Say why, so the person is not left with a bare refusal. The cause is
                    // control-free and bounded (240 characters), kept in the diagnostics
                    // as before, and the one-off text is leaked because a failed start is
                    // rare and user-initiated.
                    let cause = format!("{error:#}")
                        .chars()
                        .filter(|ch| !ch.is_control())
                        .take(240)
                        .collect::<String>();
                    self.native_failure(&cause);
                    DriverError::Failed(Box::leak(
                        format!(
                            "The harness terminal did not confirm startup ({cause}). Its owned resources remain tracked; no shell fallback was used."
                        )
                        .into_boxed_str(),
                    ))
                })?;
            return Ok(AgentStarted {
                session,
                thread_id: thread.id,
                trial: false,
            });
        }
        Err(DriverError::StaleQualification)
    }
    fn run_canary(
        &self,
        base: &StorageLocation,
        database: &Database,
        observation: &Observation<D::Probe>,
    ) -> Result<NativeTrialReceipt, ServiceError> {
        database.check_write_admission().map_err(|_| {
            DriverError::Failed("Storage admission changed; no TUI check was started.")
        })?;
        let descriptor = self.driver.descriptor();
        let policy = self.driver.policy();
        let (sidecar, port, ca) = self.context.wait_for_qualified_sidecar()?;
        check_directory_budget(base, policy.trial_prefix)?;
        let (trial, scratch) = check_scratch::create(base, policy.trial_kind)
            .map_err(|_| DriverError::Failed("The private TUI-check location is unavailable."))?
            .into_parts();
        let project = trial
            .resolve_related_directory(&trial.directory().join("project"))
            .map_err(|_| DriverError::Failed("The synthetic check project is unavailable."))?;
        project.prepare_private_directory().map_err(|_| {
            DriverError::Failed("The synthetic check project could not be created privately.")
        })?;
        let home = PrivateStorePlan::resolve(
            &trial,
            AgentStoreKey::new(descriptor.id)
                .map_err(|_| DriverError::Failed("The harness namespace is invalid."))?,
            self.driver.private_store_request(),
        )
        .and_then(|plan| plan.prepare())
        .map_err(|_| {
            DriverError::Failed("A fresh private home could not be prepared for the TUI check.")
        })?;
        let selected_set = empty_set()?;
        let thread_id = new_id(CanonicalIdKind::Thread);
        let prepared = self.driver.prepare_trial(TrialContext {
            candidate: &observation.candidate,
            observation: &observation.outcome,
            base,
            trial_base: &trial,
            private_home: home,
            project: project.directory(),
            sidecar,
            proxy_port: port,
            ca_file: &ca,
            selected_set: &selected_set,
            thread_id: &thread_id,
        })?;
        self.context.wait_for_qualified_sidecar()?;
        self.context
            .runtime
            .wait_for_interactive_admission()
            .map_err(DriverError::deferred)?;
        let host = Arc::new(TerminalHost::default());
        self.context
            .runtime
            .configure_canary(&host)
            .map_err(|_| DriverError::Failed("Contained TUI-check admission is unavailable."))?;
        let owner = Arc::new(CanaryOwner {
            host,
            tracker: prepared.tracker,
            session_id: Mutex::new(None),
            _base: Mutex::new(Some(trial)),
            scratch: Mutex::new(scratch),
            _keep_alive: Mutex::new(prepared.keep_alive),
        });
        self.facts()?.canary = Some(Arc::clone(&owner));
        let deadline = Instant::now() + Duration::from_secs(7);
        let session = match owner
            .host
            .start_prepared(prepared.launch, prepared.permit, 24, 80)
        {
            Ok(session) => session,
            Err(error) => {
                self.native_failure(error);
                if owner.stop() {
                    drop(project);
                    owner.finish_released(base)?;
                    self.facts()?.canary = None;
                }
                return Err(DriverError::Failed(
                    "The contained TUI check did not start. Any unconfirmed owner remains retained.",
                ));
            }
        };
        *owner
            .session_id
            .lock()
            .map_err(|_| DriverError::Failed("Canary session ownership is unavailable."))? =
            Some(session.id.clone());
        let mut controls = CursorReplies::default();
        let mut total = 0usize;
        while Instant::now() < deadline {
            match owner.host.read(&session.id) {
                Ok(batch) => {
                    for chunk in &batch.chunks {
                        total = total.saturating_add(chunk.len());
                        if total > 512 * 1024 {
                            break;
                        }
                        controls.answer(&owner.host, &session.id, chunk);
                    }
                    if batch.eof || batch.error.is_some() || total > 512 * 1024 {
                        break;
                    }
                }
                Err(_) => break,
            }
            if owner.tracker.ready_for_cleanup() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let released = owner.stop();
        // Pins/listeners must be dropped before attempting owned scratch cleanup.
        drop(project);
        let receipt = if released {
            let receipt = owner.finish_released(base)?;
            self.facts()?.canary = None;
            receipt
        } else {
            None
        };
        receipt.ok_or(DriverError::Failed(if released {
            "The TUI check did not produce a qualified startup receipt. No provider message was sent."
        } else { "TUI-check cleanup is unconfirmed. Its budget and owner remain retained; opening is unavailable." }))
    }
}
impl<D: HarnessDriver> HarnessService for Service<D> {
    fn descriptor(&self) -> HarnessDescriptor {
        self.driver.descriptor()
    }
    fn diagnostics(&self) -> Value {
        let (owner, probe, failure) = self
            .facts
            .try_lock()
            .ok()
            .map(|facts| {
                (
                    facts.canary.clone(),
                    facts
                        .last_probe
                        .as_ref()
                        .map(|value| value.diagnostics.clone()),
                    facts.last_start_failure.clone(),
                )
            })
            .unwrap_or_default();
        serde_json::json!({
            "canary": owner.as_ref().map(|value| value.host.resources()),
            "trialReadyForCleanup": owner.as_ref().map(|value| value.tracker.ready_for_cleanup()),
            "versionProbe": probe, "startupFailure": failure,
        })
    }
    fn check(&self) -> Result<AgentCheckup, ServiceError> {
        let result = self.check_impl();
        if let Err(error) = result {
            self.remember(error);
        }
        result
    }
    fn open(
        &self,
        host: &TerminalHost,
        project: &Path,
        rows: u16,
        cols: u16,
        operation: SessionOperation,
        mode: OpenMode,
        initial_prompt: Option<&str>,
        require_mukade: bool,
        control_header: Option<&str>,
    ) -> Result<AgentStarted, ServiceError> {
        {
            let mut facts = self.facts()?;
            facts.last_error = None;
            facts.last_start_failure = None;
        }
        let result = self.open_impl(host, project, rows, cols, operation, mode, initial_prompt, require_mukade, control_header);
        if let Err(error) = result {
            self.remember(error);
        }
        result
    }
}
fn now() -> Result<i64, ServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
        .ok_or(DriverError::Failed(
            "The thread timestamp is unavailable. No harness was started.",
        ))
}
