//! Bounded Windows parent. ConPTY and its blocking close live only in the owned
//! helper. Configure explicitly; the default host cannot start a Windows process.
use crate::protocol::{self, HelperEvent, HostCommand, LaunchConfig};
use crate::windows_pty::{self, HelperProcess};
use crate::{
    AGENT_MARKERS, CHUNK_BYTES, INPUT_BYTES, MAX_SESSIONS, OUTPUT_CHUNKS, OutputBatch, SessionInfo,
    validate_size,
};
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use shinatama_engine::resources::{
    admission::{
        CleanupStatus, PreparedWorkload, QueueCredit, QueueCredits, QueueStatus, ReservationTicket,
    },
    job::{JobSnapshot, OwnedJobKey},
    sampling::{Measurement, ProcessKey, WindowsSampler},
};
use shinatama_engine::{launch::PreparedLaunch, native_launch::NativeTuiPermit};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::File,
    os::windows::{ffi::OsStrExt, io::BorrowedHandle},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, OnceLock, TryLockError,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{FILETIME, WAIT_TIMEOUT},
    System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        QueryFullProcessImageNameW, WaitForSingleObject,
    },
};

pub trait WindowsTerminalAdmission: Send + Sync {
    fn prepare(&self) -> Result<PreparedWorkload>;
    fn activate(
        &self,
        workload: &mut PreparedWorkload,
        process: BorrowedHandle<'_>,
    ) -> Result<ProcessKey>;
    fn cleanup(&self, workload: &mut PreparedWorkload, timeout: Duration) -> Result<CleanupStatus>;
    /// Cheap bounded telemetry hook. Missing telemetry must not select a victim.
    fn observe(
        &self,
        _owned: OwnedJobKey,
        _job: &JobSnapshot,
        _queues: QueueStatus,
    ) -> Result<TerminalDirective> {
        Ok(TerminalDirective::Continue)
    }
}
/// Rust-only factory. Recheck the permit's exact root revision/profile against
/// current shared state before returning a per-launch reservation context.
pub trait WindowsNativeAdmission: Send + Sync {
    fn for_launch(&self, permit: &NativeTuiPermit) -> Result<Arc<dyn WindowsTerminalAdmission>>;
}
/// Trusted Rust-only persistence hooks, never executable IPC input. Implementors
/// must use bounded local transactions; after_cleanup must be idempotent because
/// a failed/panicking transaction can be retried after native release is proved.
pub trait WindowsNativeSessionLifecycle: Send + Sync {
    /// The exact shell is suspended inside the sealed job. `reservation` is the
    /// active ticket owned by that job, including its exact host identity and
    /// nonce. Persist the association before allowing execution. Returning an
    /// error prevents ResumeShell and enters the ordinary owned cleanup path.
    fn before_run(
        &self,
        instance_id: &str,
        process: ProcessKey,
        reservation: &ReservationTicket,
    ) -> Result<()>;
    /// Called only after confirmed native/host cleanup, including a failed
    /// before_run attempt that may have persisted its activation before failing.
    fn after_cleanup(&self, instance_id: &str, process: ProcessKey) -> Result<()>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalDirective {
    Continue,
    StopResponsible,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalResourcePhase {
    Starting,
    Running,
    Closing,
    Ended,
    Retired,
    Released,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalResourceReason {
    AwaitingMeasurement,
    Observed,
    PartialSample,
    ObservationUnavailable,
    ResourceLimit,
    StopRequested,
    ProcessEnded,
    AdmissionRefused,
    LocalFailure,
    CleanupUnconfirmed,
    BindingFinalizationPending,
    Released,
    CacheBusy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResourceObservation {
    pub private_commit_bytes: u64,
    /// Summed working sets include shared pages; this is not unique physical RAM.
    pub resident_bytes: u64,
    pub peak_job_commit_bytes: u64,
    pub active_processes: u32,
    pub total_processes: u32,
    pub total_terminated_processes: u32,
    pub cpu_time_100ns: u64,
    pub membership_complete: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResourceLimits {
    pub job_commit_bytes: u64,
    pub process_commit_bytes: u64,
    /// False means the memory, process and CPU values are reservations, not Windows limits.
    pub limits_enforced: bool,
    pub processes: u32,
    pub cpu_basis_points: u16,
    pub queue_bytes: u64,
    pub queue_items: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResourceQueues {
    pub bytes: u64,
    pub items: u32,
    pub peak_bytes: u64,
    pub peak_items: u32,
    pub refusals: u64,
    pub closed: bool,
}
impl From<QueueStatus> for TerminalResourceQueues {
    fn from(status: QueueStatus) -> Self {
        Self {
            bytes: status.current_bytes,
            items: status.current_items,
            peak_bytes: status.peak_bytes,
            peak_items: status.peak_items,
            refusals: status.refusals,
            closed: status.closed,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResourceStatus {
    pub id: String,
    pub phase: TerminalResourcePhase,
    pub observed: Option<TerminalResourceObservation>,
    pub applied: Option<TerminalResourceLimits>,
    pub queues: Option<TerminalResourceQueues>,
    pub sample_age_ms: Option<u64>,
    pub reason: TerminalResourceReason,
}
struct ResourceCache {
    observed: Option<TerminalResourceObservation>,
    applied: Option<TerminalResourceLimits>,
    queues: Option<TerminalResourceQueues>,
    sampled: Option<Instant>,
    reason: TerminalResourceReason,
}
impl Default for ResourceCache {
    fn default() -> Self {
        Self {
            observed: None,
            applied: None,
            queues: None,
            sampled: None,
            reason: TerminalResourceReason::AwaitingMeasurement,
        }
    }
}
pub struct WindowsTerminalConfig {
    pub helper_executable: PathBuf,
    pub shell_executable: PathBuf,
    pub shell_arguments: Vec<OsString>,
    pub shell_label: String,
    pub admission: Arc<dyn WindowsTerminalAdmission>,
    pub startup_timeout: Duration,
    pub cleanup_timeout: Duration,
}
/// The prepared-launch entry, the configuration validator and the wire
/// protocol share this single bound so a sealed recipe can never be refused
/// after qualification by a drifted local limit.
pub use crate::protocol::MAX_LAUNCH_ARGUMENTS as SHELL_ARGUMENT_LIMIT;
impl WindowsTerminalConfig {
    fn copy_with_admission(&self, admission: Arc<dyn WindowsTerminalAdmission>) -> Self {
        Self {
            helper_executable: self.helper_executable.clone(),
            shell_executable: self.shell_executable.clone(),
            shell_arguments: self.shell_arguments.clone(),
            shell_label: self.shell_label.clone(),
            admission,
            startup_timeout: self.startup_timeout,
            cleanup_timeout: self.cleanup_timeout,
        }
    }
    fn validate(&self) -> Result<()> {
        for path in [&self.helper_executable, &self.shell_executable] {
            let units: Vec<_> = path.as_os_str().encode_wide().take(4097).collect();
            if !path.is_absolute()
                || units.len() > 4096
                || units.contains(&0)
                || !path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            {
                bail!("Terminal executables must be explicit absolute .exe paths");
            }
        }
        let mut count = 0;
        if self.shell_arguments.len() > SHELL_ARGUMENT_LIMIT {
            bail!("Too many shell arguments");
        }
        for argument in &self.shell_arguments {
            let units: Vec<_> = argument.encode_wide().take(8193).collect();
            if units.len() > 8192 || units.contains(&0) {
                bail!("Invalid bounded shell argument");
            }
            count += units.len();
        }
        if count > 16384
            || self.shell_label.is_empty()
            || self.shell_label.len() > 128
            || self.shell_label.chars().any(char::is_control)
            || self.startup_timeout.is_zero()
            || self.startup_timeout > Duration::from_secs(15)
            || self.cleanup_timeout.is_zero()
            || self.cleanup_timeout > Duration::from_secs(3)
        {
            bail!("Invalid terminal worker configuration");
        }
        Ok(())
    }
}
const STARTING: u8 = 0;
const RUNNING: u8 = 1;
const CLOSING: u8 = 2;
const ENDED: u8 = 3;
const RETIRED: u8 = 4;
const RELEASED: u8 = 5;
const NO_EXIT: u64 = u64::MAX;
const IO_WAIT: Duration = Duration::from_secs(2);
struct Outbound {
    command: HostCommand,
    _credit: QueueCredit,
}
struct OutputPacket {
    bytes: Vec<u8>,
    credit: QueueCredit,
}
enum WorkerEvent {
    ConsoleReady,
    ShellReady(ProcessKey),
    ShellClosing(u32),
    Exited(u32),
    Eof,
    Error,
}
#[derive(Default)]
struct StopState {
    done: bool,
    released: bool,
}
struct Session {
    info: SessionInfo,
    phase: AtomicU8,
    announced: AtomicBool,
    cancel: AtomicBool,
    source_eof: AtomicBool,
    exit_code: AtomicU64,
    error: Mutex<Option<String>>,
    credits: OnceLock<QueueCredits>,
    resources: Mutex<ResourceCache>,
    input: SyncSender<Outbound>,
    output: Mutex<Option<Receiver<OutputPacket>>>,
    finished: (Mutex<StopState>, Condvar),
    // Keep program/home/ancestor pins and the exact identity/environment alive
    // for the full session, including retained/unconfirmed cleanup.
    _native: Option<NativeSessionPins>,
}
struct NativeSessionPins {
    launch: PreparedLaunch,
    permit: NativeTuiPermit,
    admission: Arc<dyn WindowsNativeAdmission>,
    lifecycle: Option<NativeLifecycleGate>,
}
const BINDING_NATIVE_PENDING: u8 = 0;
const BINDING_END_PENDING: u8 = 1;
const BINDING_END_RUNNING: u8 = 2;
const BINDING_ENDED: u8 = 3;
struct NativeLifecycleGate {
    callbacks: Arc<dyn WindowsNativeSessionLifecycle>,
    attempted: OnceLock<ProcessKey>,
    cleanup: AtomicU8,
}
impl NativeLifecycleGate {
    fn new(callbacks: Arc<dyn WindowsNativeSessionLifecycle>) -> Self {
        Self {
            callbacks,
            attempted: OnceLock::new(),
            cleanup: AtomicU8::new(BINDING_NATIVE_PENDING),
        }
    }
    fn before_run(
        &self,
        instance_id: &str,
        process: ProcessKey,
        reservation: &ReservationTicket,
    ) -> Result<()> {
        self.attempted
            .set(process)
            .map_err(|_| anyhow!("Session ownership was already attempted"))?;
        // Catch here, inside the owner worker, so a callback panic still reaches
        // owned cleanup instead of unwinding past the cleanup sequence.
        catch_unwind(AssertUnwindSafe(|| {
            self.callbacks.before_run(instance_id, process, reservation)
        }))
        .map_err(|_| anyhow!("Session ownership registration failed"))?
    }
    fn after_cleanup(&self, instance_id: &str, native_released: bool) -> Result<bool> {
        if !native_released {
            return Ok(false);
        }
        let Some(process) = self.attempted.get().copied() else {
            return Ok(true);
        };
        let _ = self.cleanup.compare_exchange(
            BINDING_NATIVE_PENDING,
            BINDING_END_PENDING,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        match self.cleanup.compare_exchange(
            BINDING_END_PENDING,
            BINDING_END_RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {}
            Err(BINDING_ENDED) => return Ok(true),
            Err(_) => bail!("Session ownership finalization is already in progress"),
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.callbacks.after_cleanup(instance_id, process)
        }))
        .unwrap_or_else(|_| Err(anyhow!("Session ownership finalization failed")));
        self.cleanup.store(
            if result.is_ok() {
                BINDING_ENDED
            } else {
                BINDING_END_PENDING
            },
            Ordering::Release,
        );
        result.map(|()| true)
    }
    fn native_released_with_pending_binding(&self) -> bool {
        matches!(
            self.cleanup.load(Ordering::Acquire),
            BINDING_END_PENDING | BINDING_END_RUNNING
        )
    }
}
impl Session {
    fn error(&self, text: &str) {
        self.resource_reason(TerminalResourceReason::LocalFailure);
        if let Ok(mut error) = self.error.lock() {
            if error.is_none() {
                *error = Some(text.chars().take(1024).collect());
            }
        }
    }
    fn resource_reason(&self, reason: TerminalResourceReason) {
        if let Ok(mut cached) = self.resources.lock() {
            // Keep a concrete stop/failure cause through later telemetry/cleanup.
            if matches!(
                cached.reason,
                TerminalResourceReason::AwaitingMeasurement
                    | TerminalResourceReason::Observed
                    | TerminalResourceReason::PartialSample
                    | TerminalResourceReason::ObservationUnavailable
            ) || reason == TerminalResourceReason::ResourceLimit
            {
                cached.reason = reason;
            }
        }
    }
    fn binding_finalization_pending(&self) -> bool {
        self._native
            .as_ref()
            .and_then(|native| native.lifecycle.as_ref())
            .is_some_and(NativeLifecycleGate::native_released_with_pending_binding)
    }
    fn finish_binding(&self, native_released: bool) -> bool {
        let Some(lifecycle) = self
            ._native
            .as_ref()
            .and_then(|native| native.lifecycle.as_ref())
        else {
            return native_released;
        };
        match lifecycle.after_cleanup(&self.info.id, native_released) {
            Ok(released) => released,
            Err(_) => {
                // Native resources are released; only the persisted activation
                // is pending. Preserve the slot and report the actual remedy.
                if let Ok(mut cached) = self.resources.lock() {
                    cached.reason = TerminalResourceReason::BindingFinalizationPending;
                }
                if let Ok(mut error) = self.error.lock() {
                    *error = Some("The terminal stopped, but its saved session could not be closed. Retry Close to finish saving its status.".into());
                }
                false
            }
        }
    }
    fn cache_sample(&self, snapshot: &JobSnapshot, queues: QueueStatus) {
        if let Ok(mut cached) = self.resources.try_lock() {
            cached.observed = Some(TerminalResourceObservation {
                private_commit_bytes: snapshot.observed_private_commit_bytes,
                resident_bytes: snapshot.observed_resident_bytes,
                peak_job_commit_bytes: snapshot.peak_job_commit_bytes,
                active_processes: snapshot.active_processes,
                total_processes: snapshot.total_processes,
                total_terminated_processes: snapshot.total_terminated_processes,
                cpu_time_100ns: snapshot.cpu_time_100ns,
                membership_complete: snapshot.membership_complete,
            });
            cached.queues = Some(queues.into());
            cached.sampled = Some(Instant::now());
            if matches!(
                cached.reason,
                TerminalResourceReason::AwaitingMeasurement
                    | TerminalResourceReason::Observed
                    | TerminalResourceReason::PartialSample
                    | TerminalResourceReason::ObservationUnavailable
            ) {
                cached.reason = if snapshot.membership_complete {
                    TerminalResourceReason::Observed
                } else {
                    TerminalResourceReason::PartialSample
                };
            }
        }
    }
    fn resource_status(&self) -> TerminalResourceStatus {
        let phase = match self.phase.load(Ordering::Acquire) {
            STARTING => TerminalResourcePhase::Starting,
            RUNNING => TerminalResourcePhase::Running,
            CLOSING => TerminalResourcePhase::Closing,
            ENDED => TerminalResourcePhase::Ended,
            RETIRED => TerminalResourcePhase::Retired,
            RELEASED => TerminalResourcePhase::Released,
            _ => TerminalResourcePhase::Retired,
        };
        let mut result = TerminalResourceStatus {
            id: self.info.id.clone(),
            phase,
            observed: None,
            applied: None,
            queues: None,
            sample_age_ms: None,
            reason: TerminalResourceReason::CacheBusy,
        };
        if let Ok(cached) = self.resources.try_lock() {
            result.observed = cached.observed;
            result.applied = cached.applied;
            result.queues = cached.queues;
            result.sample_age_ms = cached
                .sampled
                .map(|sampled| sampled.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
            result.reason = cached.reason;
        }
        result
    }
    fn send(&self, command: HostCommand, bytes: u64) -> Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            bail!("Terminal is closing");
        }
        let credits = self.credits.get().context("Terminal is not ready")?;
        let credit = credits
            .acquire(bytes, 1)
            .map_err(|_| anyhow!("Terminal input is busy; retry the input"))?;
        self.input
            .try_send(Outbound {
                command,
                _credit: credit,
            })
            .map_err(|_| anyhow!("Terminal input is busy or closed"))
    }
    fn finish(&self, released: bool) {
        if let Ok(mut cached) = self.resources.lock() {
            cached.queues = self.credits.get().map(|credits| credits.status().into());
            if !released
                && !matches!(
                    cached.reason,
                    TerminalResourceReason::ResourceLimit
                        | TerminalResourceReason::BindingFinalizationPending
                )
            {
                cached.reason = TerminalResourceReason::CleanupUnconfirmed;
            } else if released
                && matches!(
                    cached.reason,
                    TerminalResourceReason::AwaitingMeasurement
                        | TerminalResourceReason::Observed
                        | TerminalResourceReason::PartialSample
                        | TerminalResourceReason::ObservationUnavailable
                        | TerminalResourceReason::BindingFinalizationPending
                )
            {
                cached.reason = TerminalResourceReason::Released;
            }
        }
        if self.exit_code.load(Ordering::Acquire) == NO_EXIT {
            self.exit_code.store(1, Ordering::Release);
        }
        self.phase
            .store(if released { RELEASED } else { RETIRED }, Ordering::Release);
        self.source_eof.store(true, Ordering::Release);
        let (lock, wake) = &self.finished;
        if let Ok(mut state) = lock.lock() {
            state.done = true;
            state.released = released;
            wake.notify_all();
        }
    }
    fn wait_finished(&self, deadline: Instant) -> Result<bool> {
        let (lock, wake) = &self.finished;
        let mut state = lock
            .lock()
            .map_err(|_| anyhow!("Terminal status unavailable"))?;
        while !state.done {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(false);
            }
            state = wake
                .wait_timeout(state, remaining)
                .map_err(|_| anyhow!("Terminal status unavailable"))?
                .0;
        }
        Ok(state.released)
    }
}

#[derive(Default)]
pub struct WindowsTerminalHost {
    config: OnceLock<Arc<WindowsTerminalConfig>>,
    native_admission: OnceLock<Arc<dyn WindowsNativeAdmission>>,
    sessions: Mutex<BTreeMap<String, Arc<Session>>>,
}
impl WindowsTerminalHost {
    pub fn configure_native_admission(
        &self,
        admission: Arc<dyn WindowsNativeAdmission>,
    ) -> Result<()> {
        self.native_admission
            .set(admission)
            .map_err(|_| anyhow!("Native launch admission is already configured"))
    }
    pub fn configure(&self, config: WindowsTerminalConfig) -> Result<()> {
        config.validate()?;
        self.config
            .set(Arc::new(config))
            .map_err(|_| anyhow!("Windows terminal resources are already configured"))
    }
    pub fn start(&self, cwd: &Path, rows: u16, cols: u16) -> Result<SessionInfo> {
        let config = Arc::clone(
            self.config
                .get()
                .context("Windows terminal resource admission is not configured")?,
        );
        self.start_configured(config, cwd, rows, cols, None)
    }
    /// Blocking caller only. An owned prepared plan cannot replace its sealed,
    /// one-launch C15 permit or the separately configured trusted admission.
    pub fn start_prepared(
        &self,
        launch: PreparedLaunch,
        permit: NativeTuiPermit,
        rows: u16,
        cols: u16,
    ) -> Result<SessionInfo> {
        self.start_prepared_with_lifecycle(launch, permit, rows, cols, None)
    }
    pub fn start_prepared_with_lifecycle(
        &self,
        launch: PreparedLaunch,
        permit: NativeTuiPermit,
        rows: u16,
        cols: u16,
        lifecycle: Option<Arc<dyn WindowsNativeSessionLifecycle>>,
    ) -> Result<SessionInfo> {
        validate_size(rows, cols)?;
        permit.validate(&launch)?;
        let base = self
            .config
            .get()
            .context("Windows terminal resource admission is not configured")?;
        let admission = Arc::clone(
            self.native_admission
                .get()
                .context("Native launch admission is not configured")?,
        );
        if launch.command().argv().len() > SHELL_ARGUMENT_LIMIT {
            bail!("Native launch exceeds the contained helper argument bound");
        }
        let cwd = launch.command().working_directory().to_owned();
        let mut config = base.copy_with_admission(Arc::clone(&base.admission));
        config.shell_executable = launch.command().executable().to_owned();
        config.shell_arguments = launch.command().argv().to_vec();
        config.shell_label = launch.command().agent().as_str().into();
        config.validate()?;
        self.start_configured(
            Arc::new(config),
            &cwd,
            rows,
            cols,
            Some(NativeSessionPins {
                launch,
                permit,
                admission,
                lifecycle: lifecycle.map(NativeLifecycleGate::new),
            }),
        )
    }
    fn start_configured(
        &self,
        config: Arc<WindowsTerminalConfig>,
        cwd: &Path,
        rows: u16,
        cols: u16,
        native: Option<NativeSessionPins>,
    ) -> Result<SessionInfo> {
        validate_size(rows, cols)?;
        let cwd = cwd
            .canonicalize()
            .context("Project folder does not exist")?;
        if !cwd.is_dir() {
            bail!("Choose a project folder");
        }
        let (input_tx, input_rx) = mpsc::sync_channel(32);
        let (output_tx, output_rx) = mpsc::sync_channel(OUTPUT_CHUNKS);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let info = SessionInfo {
            id: native
                .as_ref()
                .map(|pins| pins.launch.identity_record().instance_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            cwd: ordinary_path(&cwd),
            shell: config.shell_label.clone(),
        };
        let session = Arc::new(Session {
            info: info.clone(),
            phase: AtomicU8::new(STARTING),
            announced: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            source_eof: AtomicBool::new(false),
            exit_code: AtomicU64::new(NO_EXIT),
            error: Mutex::new(None),
            credits: OnceLock::new(),
            resources: Mutex::new(ResourceCache::default()),
            input: input_tx,
            output: Mutex::new(Some(output_rx)),
            finished: (Mutex::new(StopState::default()), Condvar::new()),
            _native: native,
        });
        {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| anyhow!("Terminal registry unavailable"))?;
            sessions.retain(|_, old| {
                old.announced.load(Ordering::Acquire)
                    || old.phase.load(Ordering::Acquire) != RELEASED
            });
            if sessions.len() >= MAX_SESSIONS {
                bail!("Terminal capacity is occupied, including any unconfirmed cleanup");
            }
            if sessions.contains_key(&info.id) {
                bail!("This launch instance is already registered; no replacement was started");
            }
            sessions.insert(info.id.clone(), Arc::clone(&session));
        }
        let worker_session = Arc::clone(&session);
        let worker_config = Arc::clone(&config);
        if let Err(error) = thread::Builder::new()
            .name("terminal-owner".into())
            .spawn(move || {
                let failure_session = Arc::clone(&worker_session);
                if catch_unwind(AssertUnwindSafe(|| {
                    owner_worker(
                        worker_config,
                        worker_session,
                        cwd,
                        rows,
                        cols,
                        input_rx,
                        output_tx,
                        started_tx,
                    )
                }))
                .is_err()
                {
                    failure_session.cancel.store(true, Ordering::Release);
                    failure_session
                        .error("Terminal owner worker failed; its reservation remains retained");
                    drop_output(&failure_session, Instant::now() + IO_WAIT);
                    failure_session.finish(false);
                }
            })
        {
            self.sessions
                .lock()
                .map_err(|_| anyhow!("Terminal registry unavailable"))?
                .remove(&info.id);
            return Err(error).context("Cannot allocate a terminal owner worker");
        }
        match started_rx.recv_timeout(config.startup_timeout) {
            Ok(Ok(())) => Ok(info),
            Ok(Err(message)) => {
                session.cancel.store(true, Ordering::Release);
                self.remove_released(&info.id);
                bail!(message)
            }
            Err(_) => {
                session.cancel.store(true, Ordering::Release);
                session.error("Terminal startup was not confirmed; its slot remains occupied until cleanup is verified");
                bail!("Terminal startup timed out; no replacement was started")
            }
        }
    }
    fn session(&self, id: &str) -> Result<Arc<Session>> {
        self.sessions
            .lock()
            .map_err(|_| anyhow!("Terminal registry unavailable"))?
            .get(id)
            .cloned()
            .context("Terminal no longer exists")
    }
    fn remove_released(&self, id: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            if sessions
                .get(id)
                .is_some_and(|session| session.phase.load(Ordering::Acquire) == RELEASED)
            {
                sessions.remove(id);
            }
        }
    }
    /// Retained identity includes ended terminals until explicitly removed.
    /// This is metadata, never a claim that a process is still alive.
    pub fn info(&self, id: &str) -> Result<SessionInfo> {
        self.session(id).map(|session| session.info.clone())
    }
    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .lock()
            .map(|sessions| {
                sessions
                    .values()
                    .filter(|session| {
                        session.announced.load(Ordering::Acquire)
                            && session.phase.load(Ordering::Acquire) != RELEASED
                    })
                    .map(|session| session.info.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Cached scalar observations only. No native query, blocking lock or join.
    pub fn resources(&self) -> Vec<TerminalResourceStatus> {
        let Ok(sessions) = self.sessions.try_lock() else {
            return Vec::new();
        };
        sessions
            .values()
            .take(MAX_SESSIONS)
            .map(|session| session.resource_status())
            .collect()
    }
    /// Registry entries disappear only after verified release or before any
    /// workload exists. Contention is unknown, never an empty/cleanup proof.
    pub fn fully_released(&self) -> Option<bool> {
        let sessions = self.sessions.try_lock().ok()?;
        Some(
            sessions
                .values()
                .all(|session| session.phase.load(Ordering::Acquire) == RELEASED),
        )
    }
    pub fn write(&self, id: &str, data: Vec<u8>) -> Result<()> {
        if data.len() > INPUT_BYTES {
            bail!("Paste is too large (maximum 64 KiB per write)");
        }
        let session = self.session(id)?;
        if session.phase.load(Ordering::Acquire) != RUNNING {
            bail!("Terminal is not accepting input");
        }
        let bytes = data.capacity() as u64;
        session.send(HostCommand::Input(data), bytes)
    }
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        validate_size(rows, cols)?;
        let session = self.session(id)?;
        if session.phase.load(Ordering::Acquire) != RUNNING {
            bail!("Terminal is not accepting resize");
        }
        session.send(HostCommand::Resize { rows, cols }, 4)
    }
    pub fn read(&self, id: &str) -> Result<OutputBatch> {
        let session = self.session(id)?;
        let mut batch = OutputBatch {
            chunks: Vec::new(),
            eof: false,
            exit_code: None,
            error: None,
            _queue_credits: Vec::new(),
        };
        let mut empty = false;
        match session.output.try_lock() {
            Ok(mut output) => {
                if let Some(receiver) = output.as_mut() {
                    for _ in 0..8 {
                        match receiver.try_recv() {
                            Ok(packet) => {
                                batch.chunks.push(packet.bytes);
                                batch._queue_credits.push(packet.credit);
                            }
                            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                                empty = true;
                                break;
                            }
                        }
                    }
                } else {
                    empty = true;
                }
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Poisoned(_)) => bail!("Terminal output unavailable"),
        }
        batch.eof = empty && session.source_eof.load(Ordering::Acquire);
        let code = session.exit_code.load(Ordering::Acquire);
        if code != NO_EXIT {
            batch.exit_code = Some(code as u32);
        }
        if let Ok(mut error) = session.error.try_lock() {
            batch.error = error.take();
        }
        Ok(batch)
    }
    pub fn stop(&self, id: &str) -> Result<()> {
        let Some(session) = self
            .sessions
            .lock()
            .map_err(|_| anyhow!("Terminal registry unavailable"))?
            .get(id)
            .cloned()
        else {
            return Ok(());
        };
        session.cancel.store(true, Ordering::Release);
        session.resource_reason(TerminalResourceReason::StopRequested);
        let wait = self
            .config
            .get()
            .map(|config| config.startup_timeout + config.cleanup_timeout + IO_WAIT)
            .unwrap_or(IO_WAIT);
        if !session.wait_finished(Instant::now() + wait)? {
            if session.binding_finalization_pending() {
                if !session.finish_binding(true) {
                    bail!(
                        "The terminal stopped, but its saved session status is still pending. Retry Close."
                    );
                }
                session.finish(true);
                self.remove_released(id);
                return Ok(());
            }
            bail!("Terminal cleanup is unconfirmed; its resource reservation remains retained");
        }
        self.remove_released(id);
        Ok(())
    }
    pub fn shutdown(&self) {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect())
            .unwrap_or_default();
        for session in &sessions {
            session.cancel.store(true, Ordering::Release);
        }
        let wait = self
            .config
            .get()
            .map(|config| config.startup_timeout + config.cleanup_timeout + IO_WAIT)
            .unwrap_or(IO_WAIT);
        let deadline = Instant::now() + wait;
        for session in sessions {
            let _ = session.wait_finished(deadline);
            self.remove_released(&session.info.id);
        }
    }
}
impl Drop for WindowsTerminalHost {
    fn drop(&mut self) {
        if let Ok(sessions) = self.sessions.get_mut() {
            for session in sessions.values() {
                session.cancel.store(true, Ordering::Release);
            }
        }
    }
}

fn ordinary_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }
}
fn now_ms() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}
fn environment() -> Result<Vec<(OsString, OsString)>> {
    let mut result = Vec::new();
    let mut units = 0_usize;
    for (name, value) in std::env::vars_os() {
        if result.len() >= 2048 {
            bail!("Inherited terminal environment exceeds its bound");
        }
        let name_units = name.encode_wide().count();
        let value_units = value.encode_wide().count();
        units = units
            .checked_add(name_units + value_units + 2)
            .context("Inherited environment size overflow")?;
        if name_units > 4096 || units > 524288 {
            bail!("Inherited terminal environment exceeds its bound");
        }
        let text = name.to_string_lossy();
        if AGENT_MARKERS
            .iter()
            .any(|marker| text.eq_ignore_ascii_case(marker))
            || text.eq_ignore_ascii_case("TERM")
            || text.eq_ignore_ascii_case("COLORTERM")
        {
            continue;
        }
        result.push((name, value));
    }
    result.push(("TERM".into(), "xterm-256color".into()));
    result.push(("COLORTERM".into(), "truecolor".into()));
    Ok(result)
}
struct Done(Arc<AtomicBool>);
impl Drop for Done {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn event(sender: &SyncSender<WorkerEvent>, value: WorkerEvent, session: &Session) {
    match sender.try_send(value) {
        Ok(()) => {}
        Err(_) if session.cancel.load(Ordering::Acquire) => {}
        Err(TrySendError::Full(_)) => {
            session.error("Terminal helper control queue overflow");
            session.cancel.store(true, Ordering::Release);
        }
        Err(TrySendError::Disconnected(_)) => {
            if !matches!(
                session.phase.load(Ordering::Acquire),
                ENDED | CLOSING | RETIRED | RELEASED
            ) {
                session.error("Terminal helper control channel closed unexpectedly");
                session.cancel.store(true, Ordering::Release);
            }
        }
    }
}
fn reader(
    mut output: File,
    session: Arc<Session>,
    credits: QueueCredits,
    sender: SyncSender<OutputPacket>,
    events: SyncSender<WorkerEvent>,
    done: Arc<AtomicBool>,
) {
    let _done = Done(done);
    loop {
        let value = match protocol::read_event(&mut output) {
            Ok(Some(value)) => value,
            Ok(None) => {
                session.source_eof.store(true, Ordering::Release);
                event(&events, WorkerEvent::Eof, &session);
                return;
            }
            Err(error) => {
                session.error(&format!(
                    "Terminal helper output protocol failed ({:?}, OS {:?}): {}",
                    error.kind(),
                    error.raw_os_error(),
                    error
                ));
                event(&events, WorkerEvent::Error, &session);
                return;
            }
        };
        match value {
            HelperEvent::Output(bytes) => {
                if let Some(native) = &session._native {
                    native.permit.observe_trial_output(&bytes);
                }
                if bytes.len() > CHUNK_BYTES {
                    session.error("Terminal helper exceeded the output frame bound");
                    session.cancel.store(true, Ordering::Release);
                    return;
                }
                let credit = loop {
                    if session.cancel.load(Ordering::Acquire) {
                        return;
                    }
                    match credits.acquire(bytes.len() as u64, 1) {
                        Ok(credit) => break credit,
                        Err(_) => thread::sleep(Duration::from_millis(5)),
                    }
                };
                if sender.send(OutputPacket { bytes, credit }).is_err() {
                    return;
                }
            }
            HelperEvent::ConsoleReady => event(&events, WorkerEvent::ConsoleReady, &session),
            HelperEvent::ShellReady(key) => event(&events, WorkerEvent::ShellReady(key), &session),
            HelperEvent::ShellClosing(code) => {
                event(&events, WorkerEvent::ShellClosing(code), &session)
            }
            HelperEvent::Exited(code) => event(&events, WorkerEvent::Exited(code), &session),
            HelperEvent::Error(_) => {
                session.error("Terminal helper reported a local failure");
                event(&events, WorkerEvent::Error, &session);
            }
        }
    }
}
fn writer(
    mut input: File,
    session: Arc<Session>,
    commands: Receiver<Outbound>,
    events: SyncSender<WorkerEvent>,
    done: Arc<AtomicBool>,
) {
    let _done = Done(done);
    while !session.cancel.load(Ordering::Acquire) {
        match commands.recv_timeout(Duration::from_millis(20)) {
            Ok(packet) => {
                if protocol::write_command(&mut input, &packet.command).is_err() {
                    session.error("Terminal helper input connection failed");
                    event(&events, WorkerEvent::Error, &session);
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
fn wait_event(
    session: &Session,
    events: &Receiver<WorkerEvent>,
    deadline: Instant,
) -> Result<WorkerEvent> {
    loop {
        if session.cancel.load(Ordering::Acquire) {
            bail!("Terminal startup was cancelled");
        }
        if Instant::now() >= deadline {
            bail!("Terminal helper startup timed out");
        }
        match events.recv_timeout(Duration::from_millis(20)) {
            Ok(event) => return Ok(event),
            Err(RecvTimeoutError::Timeout) => {}
            Err(_) => bail!("Terminal helper connection closed during startup"),
        }
    }
}
fn sample(
    session: &Session,
    workload: &PreparedWorkload,
    sampler: &WindowsSampler,
    admission: &dyn WindowsTerminalAdmission,
) -> Result<JobSnapshot> {
    let snapshot = workload
        .sample(sampler)
        .map_err(|_| anyhow!("Cannot verify terminal job membership"))?;
    if let Some(native) = &session._native {
        native.permit.observe_trial_workload(workload, &snapshot);
    }
    let queues = workload.queue_credits().status();
    session.cache_sample(&snapshot, queues);
    if admission.observe(workload.owned_key(), &snapshot, queues)?
        == TerminalDirective::StopResponsible
    {
        session.resource_reason(TerminalResourceReason::ResourceLimit);
        bail!("This terminal exceeded its attributed resource policy");
    }
    Ok(snapshot)
}
fn verified_console_host(key: ProcessKey) -> bool {
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            key.pid,
        )
    };
    if raw.is_null() {
        return false;
    }
    let handle = windows_pty::Handle(raw);
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) } == 0
    {
        return false;
    }
    let creation = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    if creation != key.created_at_100ns
        || unsafe { WaitForSingleObject(handle.0, 0) } != WAIT_TIMEOUT
    {
        return false;
    }
    let mut image = [0_u16; 1024];
    let mut length = image.len() as u32;
    if unsafe { QueryFullProcessImageNameW(handle.0, 0, image.as_mut_ptr(), &mut length) } == 0
        || length as usize > image.len()
    {
        return false;
    }
    let image = String::from_utf16_lossy(&image[..length as usize]);
    Path::new(&image).file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case("conhost.exe") || name.eq_ignore_ascii_case("OpenConsole.exe")
    })
}
fn drop_output(session: &Session, deadline: Instant) -> bool {
    loop {
        match session.output.try_lock() {
            Ok(mut receiver) => {
                receiver.take();
                return true;
            }
            Err(TryLockError::Poisoned(_)) => return false,
            Err(TryLockError::WouldBlock) => {}
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn cleanup(
    config: &WindowsTerminalConfig,
    session: &Session,
    workload: &mut PreparedWorkload,
    reader_done: &AtomicBool,
    writer_done: &AtomicBool,
    cleanup_credit: &mut Option<QueueCredit>,
    io_threads: &[JoinHandle<()>],
) -> bool {
    session.cancel.store(true, Ordering::Release);
    session.phase.store(CLOSING, Ordering::Release);
    let io_started = Instant::now();
    let deadline = io_started + IO_WAIT;
    if !drop_output(session, deadline) {
        session.error("Terminal queues could not be closed; reservation retained");
        return false;
    }
    let remaining_io = IO_WAIT.saturating_sub(io_started.elapsed());
    let initial = config.admission.cleanup(workload, config.cleanup_timeout);
    // A retained job can still be inspected after the finite cleanup attempt.
    // Publish the actual post-stop counters, rather than the last running sample.
    if let Ok(snapshot) = workload.sample(&WindowsSampler::new()) {
        session.cache_sample(&snapshot, workload.queue_credits().status());
    }
    // Native termination has its own finite budget. Preserve the remaining I/O
    // allowance; do not consume it while waiting for the native job to stop.
    let deadline = Instant::now() + remaining_io;
    while (!reader_done.load(Ordering::Acquire)
        || !writer_done.load(Ordering::Acquire)
        || io_threads.iter().any(|thread| !thread.is_finished()))
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(5));
    }
    if !reader_done.load(Ordering::Acquire)
        || !writer_done.load(Ordering::Acquire)
        || io_threads.iter().any(|thread| !thread.is_finished())
    {
        session.error("Terminal I/O cleanup is unconfirmed; reservation retained");
        return false;
    }
    cleanup_credit.take();
    match initial {
        Ok(CleanupStatus::Released { .. }) => true,
        Ok(CleanupStatus::AwaitingCredits(_)) => matches!(
            config.admission.cleanup(workload, Duration::ZERO),
            Ok(CleanupStatus::Released { .. })
        ),
        _ => false,
    }
}
#[allow(clippy::too_many_arguments)]
fn owner_worker(
    config: Arc<WindowsTerminalConfig>,
    session: Arc<Session>,
    cwd: PathBuf,
    rows: u16,
    cols: u16,
    commands: Receiver<Outbound>,
    output: SyncSender<OutputPacket>,
    started: SyncSender<std::result::Result<(), String>>,
) {
    if session.cancel.load(Ordering::Acquire) {
        let _ = started.try_send(Err("Terminal startup was cancelled".into()));
        session.finish(true);
        return;
    }
    let config = if let Some(native) = &session._native {
        let admitted = (|| -> Result<Arc<WindowsTerminalConfig>> {
            native.permit.validate(&native.launch)?;
            native.permit.verify_pins()?;
            let admission = native.admission.for_launch(&native.permit)?;
            Ok(Arc::new(config.copy_with_admission(admission)))
        })();
        match admitted {
            Ok(config) => config,
            Err(error) => {
                session.resource_reason(TerminalResourceReason::AdmissionRefused);
                let _ = started.try_send(Err(format!("The qualified native launch binding is unavailable; no process was started: {error}")));
                session.finish(true);
                return;
            }
        }
    } else {
        config
    };
    let mut workload = match config.admission.prepare() {
        Ok(workload) => workload,
        Err(error) => {
            session.resource_reason(TerminalResourceReason::AdmissionRefused);
            let _ = started.try_send(Err(format!(
                "Terminal resource admission was refused: {error}"
            )));
            session.finish(true);
            return;
        }
    };
    if let Some(native) = &session._native {
        let expected = native.permit.profile();
        let exact = workload.ticket().budget == expected.budget()
            && workload.applied_budget().is_some_and(|budget| {
                budget.job_commit_bytes() == expected.budget().memory_bytes
                    && budget.process_commit_bytes() == expected.process_commit_bytes()
                    && budget.max_processes() == expected.budget().processes
                    && budget.cpu_hard_cap_basis_points() == expected.budget().cpu_basis_points
            });
        if !exact {
            let released = matches!(
                config
                    .admission
                    .cleanup(&mut workload, config.cleanup_timeout),
                Ok(CleanupStatus::Released { .. })
            );
            session.resource_reason(TerminalResourceReason::AdmissionRefused);
            let _ = started.try_send(Err(
                "Native launch admission does not match its qualified resource profile".into(),
            ));
            session.finish(released);
            return;
        }
    }
    // Cache verified applied limits before cleanup can close the native job.
    // Ticket queue ceilings are the same frozen reservation, never inferred from
    // observed growth or rewritten after a leak.
    if let Some(budget) = workload.applied_budget()
        && let Ok(mut cached) = session.resources.lock()
    {
        cached.applied = Some(TerminalResourceLimits {
            job_commit_bytes: budget.job_commit_bytes(),
            process_commit_bytes: budget.process_commit_bytes(),
            limits_enforced: budget.limits_enforced(),
            processes: budget.max_processes(),
            cpu_basis_points: budget.cpu_hard_cap_basis_points(),
            queue_bytes: workload.ticket().budget.queue_bytes,
            queue_items: workload.ticket().budget.queue_items,
        });
        cached.queues = Some(workload.queue_credits().status().into());
    }
    let _ = session.credits.set(workload.queue_credits());
    // Keep one zero-byte ownership credit until BOTH parent I/O workers exit.
    // Native empty alone cannot release SQL capacity while host workers linger.
    let mut cleanup_credit = match workload.queue_credits().acquire(0, 1) {
        Ok(credit) => Some(credit),
        Err(_) => {
            let released = matches!(
                config
                    .admission
                    .cleanup(&mut workload, config.cleanup_timeout),
                Ok(CleanupStatus::Released { .. })
            );
            let _ = started.try_send(Err("Terminal queue admission was refused".into()));
            session.finish(released);
            return;
        }
    };
    let reader_done = Arc::new(AtomicBool::new(true));
    let writer_done = Arc::new(AtomicBool::new(true));
    let mut io_threads: Vec<JoinHandle<()>> = Vec::new();
    let mut helper_owner = None;
    let startup = (|| -> Result<(Arc<HelperProcess>, Receiver<WorkerEvent>)> {
        if session.cancel.load(Ordering::Acquire) {
            bail!("Terminal startup was cancelled");
        }
        // Native launches use EXACT C11 child composition, including only its
        // dedicated identity token. Generic shell scrubbing would erase it.
        let environment = if let Some(native) = &session._native {
            native.permit.validate(&native.launch)?;
            native
                .launch
                .environment()
                .entries()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect()
        } else {
            environment()?
        };
        let helper = windows_pty::spawn_suspended(
            &config.helper_executable,
            &cwd,
            &environment,
            &workload,
            now_ms()?,
        )?;
        config
            .admission
            .activate(&mut workload, helper.process_handle())?;
        if session.cancel.load(Ordering::Acquire) {
            bail!("Terminal startup was cancelled");
        }
        let helper = helper.resume()?;
        helper_owner = Some(Arc::clone(&helper.process));
        let (events_tx, events_rx) = mpsc::sync_channel(16);
        reader_done.store(false, Ordering::Release);
        let reader_session = Arc::clone(&session);
        let reader_credits = workload.queue_credits();
        let reader_events = events_tx.clone();
        let reader_flag = Arc::clone(&reader_done);
        match thread::Builder::new()
            .name("terminal-output".into())
            .spawn(move || {
                let failed = Arc::clone(&reader_session);
                if catch_unwind(AssertUnwindSafe(|| {
                    reader(
                        helper.output,
                        reader_session,
                        reader_credits,
                        output,
                        reader_events,
                        reader_flag,
                    )
                }))
                .is_err()
                {
                    failed.error("Terminal output worker failed");
                    failed.cancel.store(true, Ordering::Release);
                }
            }) {
            Ok(thread) => io_threads.push(thread),
            Err(error) => {
                reader_done.store(true, Ordering::Release);
                return Err(error).context("Cannot allocate terminal output worker");
            }
        }
        writer_done.store(false, Ordering::Release);
        let writer_session = Arc::clone(&session);
        let writer_events = events_tx;
        let writer_flag = Arc::clone(&writer_done);
        match thread::Builder::new()
            .name("terminal-input".into())
            .spawn(move || {
                let failed = Arc::clone(&writer_session);
                if catch_unwind(AssertUnwindSafe(|| {
                    writer(
                        helper.input,
                        writer_session,
                        commands,
                        writer_events,
                        writer_flag,
                    )
                }))
                .is_err()
                {
                    failed.error("Terminal input worker failed");
                    failed.cancel.store(true, Ordering::Release);
                }
            }) {
            Ok(thread) => io_threads.push(thread),
            Err(error) => {
                writer_done.store(true, Ordering::Release);
                return Err(error).context("Cannot allocate terminal input worker");
            }
        }
        let launch = LaunchConfig {
            rows,
            cols,
            cwd: PathBuf::from(ordinary_path(&cwd)),
            executable: config.shell_executable.clone(),
            args: config.shell_arguments.clone(),
            owned: workload.owned_key(),
        };
        session.send(HostCommand::Configure(launch), protocol::MAX_FRAME as u64)?;
        let deadline = Instant::now() + config.startup_timeout;
        let sampler = WindowsSampler::new();
        match wait_event(&session, &events_rx, deadline)? {
            WorkerEvent::ConsoleReady => {}
            _ => bail!("Terminal helper did not confirm console creation"),
        }
        // Complete membership of a helper-only job does not establish containment
        // of the console server. Verify its exact creation value and bounded image
        // query before allowing any shell to be created. No assign-after-run path.
        loop {
            if session.cancel.load(Ordering::Acquire) {
                bail!("Terminal startup was cancelled");
            }
            let console = sample(&session, &workload, &sampler, config.admission.as_ref())?;
            let root = helper.process.key();
            if console.membership_complete
                && console
                    .members
                    .iter()
                    .any(|member| member.identity == Measurement::Available(root))
                && console.members.iter().any(|member| match member.identity {
                    Measurement::Available(key) if key != root => verified_console_host(key),
                    _ => false,
                })
            {
                break;
            }
            if Instant::now() >= deadline {
                bail!("The console server could not be verified inside the terminal job");
            }
            thread::sleep(Duration::from_millis(20));
        }
        session.send(HostCommand::AllowShell, 0)?;
        let key = match wait_event(&session, &events_rx, deadline)? {
            WorkerEvent::ShellReady(key) => key,
            _ => bail!("Terminal helper did not confirm its suspended shell"),
        };
        let shell = sample(&session, &workload, &sampler, config.admission.as_ref())?;
        if !shell.membership_complete
            || !shell
                .members
                .iter()
                .any(|member| member.identity == Measurement::Available(key))
        {
            bail!("Suspended shell identity is outside the terminal job");
        }
        // ShellReady is emitted only after the helper's temporary QUERY/ASSIGN
        // binding and shell startup attributes have been destroyed. The shell's
        // handle inheritance is disabled. This owner exposes no further root
        // launch after this barrier, including before ResumeShell below.
        workload
            .seal_assignments()
            .map_err(|_| anyhow!("Terminal launch admission could not be sealed"))?;
        if let Some(lifecycle) = session
            ._native
            .as_ref()
            .and_then(|native| native.lifecycle.as_ref())
        {
            if let Err(error) = lifecycle.before_run(&session.info.id, key, workload.ticket()) {
                const MESSAGE: &str =
                    "The session could not be saved before startup. No agent was allowed to run.";
                session.error(MESSAGE);
                let _ = started.try_send(Err(MESSAGE.into()));
                return Err(error);
            }
        }
        session.send(HostCommand::ResumeShell, 0)?;
        Ok((helper.process, events_rx))
    })();
    match startup {
        Ok((process, events)) => {
            session.phase.store(RUNNING, Ordering::Release);
            session.announced.store(true, Ordering::Release);
            let _ = started.try_send(Ok(()));
            let sampler = WindowsSampler::new();
            let mut sampled = Instant::now();
            let mut draining: Option<Instant> = None;
            while !session.cancel.load(Ordering::Acquire) {
                match events.recv_timeout(Duration::from_millis(20)) {
                    Ok(WorkerEvent::ShellClosing(code) | WorkerEvent::Exited(code)) => {
                        session.resource_reason(TerminalResourceReason::ProcessEnded);
                        session.exit_code.store(u64::from(code), Ordering::Release);
                        session.phase.store(ENDED, Ordering::Release);
                        draining.get_or_insert_with(Instant::now);
                    }
                    Ok(WorkerEvent::Eof) => {
                        session.source_eof.store(true, Ordering::Release);
                        draining.get_or_insert_with(Instant::now);
                    }
                    Ok(
                        WorkerEvent::Error | WorkerEvent::ConsoleReady | WorkerEvent::ShellReady(_),
                    ) => {
                        session.error("Terminal helper reported an unexpected state");
                        break;
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        draining.get_or_insert_with(Instant::now);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
                if sampled.elapsed() >= Duration::from_millis(250) {
                    match workload.sample(&sampler) {
                        Ok(snapshot) => {
                            if let Some(native) = &session._native {
                                native.permit.observe_trial_workload(&workload, &snapshot);
                            }
                            let queues = workload.queue_credits().status();
                            session.cache_sample(&snapshot, queues);
                            match config.admission.observe(workload.owned_key(), &snapshot, queues) {
                                Ok(TerminalDirective::StopResponsible) => { session.resource_reason(TerminalResourceReason::ResourceLimit); session.error("This terminal was stopped for its attributed resource limit; saved work is preserved"); break; },
                                Ok(TerminalDirective::Continue) => {},
                                Err(_) => session.error("Terminal resource observation failed; existing native limits remain"),
                            }
                        }
                        Err(_) => session.error(
                            "Terminal resource observation failed; existing native limits remain",
                        ),
                    }
                    sampled = Instant::now();
                }
                if let Some(since) = draining {
                    if reader_done.load(Ordering::Acquire)
                        && workload.queue_credits().status().current_items == 1
                    {
                        break;
                    }
                    if since.elapsed() >= config.cleanup_timeout {
                        session.error("Terminal output drain timed out; cleanup is required");
                        break;
                    }
                }
                if let Ok(Some(code)) = process.try_exit_code()
                    && draining.is_none()
                {
                    if session.exit_code.load(Ordering::Acquire) == NO_EXIT {
                        session.exit_code.store(u64::from(code), Ordering::Release);
                    }
                    session.error("Terminal helper exited unexpectedly");
                    draining = Some(Instant::now());
                }
            }
        }
        Err(_) => {
            session.error("Terminal startup failed; its owned resources are being checked");
            let _ = started.try_send(Err("Contained terminal startup failed".into()));
        }
    }
    let native_released = cleanup(
        &config,
        &session,
        &mut workload,
        &reader_done,
        &writer_done,
        &mut cleanup_credit,
        &io_threads,
    );
    // Keep its exact handle alive through the preparatory census and native
    // termination, rather than TerminateProcess from Drop before that sequence.
    drop(helper_owner);
    if let Some(native) = &session._native {
        native.permit.finish_trial(&workload);
    }
    for thread in io_threads {
        if thread.is_finished() {
            if thread.join().is_err() {
                session
                    .error("A terminal I/O worker ended unexpectedly; its resources were closed");
            }
        }
    }
    if !native_released {
        session
            .error("Terminal stopped with unconfirmed cleanup; its reservation remains retained");
    }
    let released = session.finish_binding(native_released);
    session.finish(released);
}

#[cfg(test)]
mod tests {
    use super::*;
    const LIFECYCLE_PROCESS: ProcessKey = ProcessKey {
        pid: 321,
        created_at_100ns: 987654321,
    };
    fn lifecycle_reservation() -> ReservationTicket {
        use shinatama_state::resource_reservations::{Budget, ReservationOwner, ReservationState};
        ReservationTicket {
            id: "11111111-1111-4111-8111-111111111111".into(),
            job_key: "22222222-2222-4222-8222-222222222222".into(),
            owner: ReservationOwner {
                pid: 123,
                created_at_100ns: 123456789,
                windows_session_id: 2,
                nonce: "33333333-3333-4333-8333-333333333333".into(),
            },
            budget: Budget {
                memory_bytes: 1024 * 1024,
                processes: 8,
                cpu_basis_points: 1000,
                queue_bytes: 4096,
                queue_items: 16,
            },
            state: ReservationState::Active,
            lease_expires_at_ms: 1000,
            revision: 7,
        }
    }
    struct RecordedLifecycle {
        calls: Mutex<Vec<&'static str>>,
        refuse_start: bool,
        panic_start: bool,
        refuse_end: AtomicBool,
    }
    impl WindowsNativeSessionLifecycle for RecordedLifecycle {
        fn before_run(
            &self,
            instance_id: &str,
            process: ProcessKey,
            reservation: &ReservationTicket,
        ) -> Result<()> {
            assert_eq!(instance_id, "synthetic-instance");
            assert_eq!(process, LIFECYCLE_PROCESS);
            assert_eq!(reservation, &lifecycle_reservation());
            self.calls.lock().unwrap().push("before");
            assert!(!self.panic_start, "synthetic callback panic");
            if self.refuse_start {
                bail!("synthetic registration refusal");
            }
            Ok(())
        }
        fn after_cleanup(&self, instance_id: &str, process: ProcessKey) -> Result<()> {
            assert_eq!(instance_id, "synthetic-instance");
            assert_eq!(process, LIFECYCLE_PROCESS);
            self.calls.lock().unwrap().push("after");
            if self.refuse_end.load(Ordering::Acquire) {
                bail!("synthetic finalization refusal");
            }
            Ok(())
        }
    }
    #[test]
    fn lifecycle_waits_for_native_release_and_retries_only_pending_binding() {
        let callbacks = Arc::new(RecordedLifecycle {
            calls: Mutex::new(Vec::new()),
            refuse_start: false,
            panic_start: false,
            refuse_end: AtomicBool::new(true),
        });
        let gate = NativeLifecycleGate::new(callbacks.clone());
        assert!(!gate.after_cleanup("synthetic-instance", false).unwrap());
        assert!(callbacks.calls.lock().unwrap().is_empty());
        gate.before_run(
            "synthetic-instance",
            LIFECYCLE_PROCESS,
            &lifecycle_reservation(),
        )
        .unwrap();
        assert!(
            gate.before_run(
                "synthetic-instance",
                LIFECYCLE_PROCESS,
                &lifecycle_reservation()
            )
            .is_err()
        );
        assert!(!gate.after_cleanup("synthetic-instance", false).unwrap());
        assert_eq!(*callbacks.calls.lock().unwrap(), vec!["before"]);
        assert!(gate.after_cleanup("synthetic-instance", true).is_err());
        assert!(gate.native_released_with_pending_binding());
        callbacks.refuse_end.store(false, Ordering::Release);
        assert!(gate.after_cleanup("synthetic-instance", true).unwrap());
        assert!(!gate.native_released_with_pending_binding());
        assert!(gate.after_cleanup("synthetic-instance", true).unwrap());
        assert_eq!(
            *callbacks.calls.lock().unwrap(),
            vec!["before", "after", "after"]
        );
    }
    #[test]
    fn lifecycle_registration_failure_finishes_only_after_native_release() {
        for panic_start in [false, true] {
            let callbacks = Arc::new(RecordedLifecycle {
                calls: Mutex::new(Vec::new()),
                refuse_start: true,
                panic_start,
                refuse_end: AtomicBool::new(false),
            });
            let gate = NativeLifecycleGate::new(callbacks.clone());
            assert!(
                gate.before_run(
                    "synthetic-instance",
                    LIFECYCLE_PROCESS,
                    &lifecycle_reservation()
                )
                .is_err()
            );
            assert!(!gate.after_cleanup("synthetic-instance", false).unwrap());
            assert_eq!(*callbacks.calls.lock().unwrap(), vec!["before"]);
            assert!(gate.after_cleanup("synthetic-instance", true).unwrap());
            assert_eq!(*callbacks.calls.lock().unwrap(), vec!["before", "after"]);
        }
    }
    fn resource_session() -> Session {
        let (input, _) = mpsc::sync_channel(1);
        let (_, output) = mpsc::sync_channel(1);
        Session {
            info: SessionInfo {
                id: uuid::Uuid::new_v4().to_string(),
                cwd: "not exposed".into(),
                shell: "not exposed".into(),
            },
            phase: AtomicU8::new(RUNNING),
            announced: AtomicBool::new(true),
            cancel: AtomicBool::new(false),
            source_eof: AtomicBool::new(false),
            exit_code: AtomicU64::new(NO_EXIT),
            error: Mutex::new(None),
            credits: OnceLock::new(),
            resources: Mutex::new(ResourceCache::default()),
            _native: None,
            input,
            output: Mutex::new(Some(output)),
            finished: (Mutex::new(StopState::default()), Condvar::new()),
        }
    }
    #[test]
    fn retired_resources_keep_the_actual_limits_and_last_measurement_without_zeroing() {
        let session = resource_session();
        let first = session.resource_status();
        assert!(first.observed.is_none());
        assert!(first.applied.is_none());
        {
            let mut cache = session.resources.lock().unwrap();
            cache.applied = Some(TerminalResourceLimits {
                job_commit_bytes: 512,
                process_commit_bytes: 256,
                limits_enforced: true,
                processes: 8,
                cpu_basis_points: 1000,
                queue_bytes: 4096,
                queue_items: 16,
            });
            cache.observed = Some(TerminalResourceObservation {
                private_commit_bytes: 123,
                resident_bytes: 200,
                peak_job_commit_bytes: 300,
                active_processes: 3,
                total_processes: 5,
                total_terminated_processes: 2,
                cpu_time_100ns: 100,
                membership_complete: false,
            });
            cache.sampled = Some(Instant::now());
        }
        session.resource_reason(TerminalResourceReason::ResourceLimit);
        session.finish(false);
        let retained = session.resource_status();
        assert_eq!(retained.phase, TerminalResourcePhase::Retired);
        assert_eq!(retained.reason, TerminalResourceReason::ResourceLimit);
        assert_eq!(retained.applied.unwrap().job_commit_bytes, 512);
        assert_eq!(retained.observed.unwrap().private_commit_bytes, 123);
        assert!(!retained.observed.unwrap().membership_complete);
        assert!(retained.sample_age_ms.is_some());
    }
    #[test]
    fn resource_read_does_not_wait_for_an_occupied_cache_or_registry() {
        let session = resource_session();
        let _cache = session.resources.lock().unwrap();
        let busy = session.resource_status();
        assert_eq!(busy.reason, TerminalResourceReason::CacheBusy);
        assert!(busy.observed.is_none());
        let host = WindowsTerminalHost::default();
        let _registry = host.sessions.lock().unwrap();
        assert!(host.resources().is_empty());
    }
    struct RefusedAdmission;
    impl WindowsTerminalAdmission for RefusedAdmission {
        fn prepare(&self) -> Result<PreparedWorkload> {
            bail!("Synthetic admission refusal")
        }
        fn activate(&self, _: &mut PreparedWorkload, _: BorrowedHandle<'_>) -> Result<ProcessKey> {
            bail!("Synthetic admission refusal")
        }
        fn cleanup(&self, _: &mut PreparedWorkload, _: Duration) -> Result<CleanupStatus> {
            bail!("Synthetic admission refusal")
        }
    }
    fn config() -> WindowsTerminalConfig {
        WindowsTerminalConfig {
            helper_executable: PathBuf::from(r"C:\synthetic\terminal-helper.exe"),
            shell_executable: PathBuf::from(r"C:\synthetic\shell.exe"),
            shell_arguments: Vec::new(),
            shell_label: "Synthetic shell".into(),
            admission: Arc::new(RefusedAdmission),
            startup_timeout: Duration::from_secs(1),
            cleanup_timeout: Duration::from_secs(1),
        }
    }
    #[test]
    fn an_unconfigured_windows_host_refuses_before_creating_a_worker_or_session() {
        let host = WindowsTerminalHost::default();
        assert!(
            host.start(Path::new("."), 24, 80)
                .err()
                .expect("unconfigured host must refuse")
                .to_string()
                .contains("not configured")
        );
        assert!(host.list().is_empty());
    }
    #[test]
    fn configuration_is_bounded_explicit_and_cannot_replace_a_previous_context() {
        let host = WindowsTerminalHost::default();
        let mut invalid = config();
        invalid.helper_executable = PathBuf::from("relative.exe");
        assert!(host.configure(invalid).is_err());
        let mut invalid = config();
        invalid.startup_timeout = Duration::ZERO;
        assert!(host.configure(invalid).is_err());
        assert!(host.configure(config()).is_ok());
        assert!(host.configure(config()).is_err());
        assert!(host.list().is_empty());
    }
    #[test]
    fn the_prepared_launch_argument_bound_is_exactly_the_configuration_bound() {
        // Sealed tracked recipes carry more arguments than the historical
        // plain shells: tracked Codex currently composes 21 (15 fixed plus 6
        // hook configuration arguments). The prepared-launch entry and the
        // configuration validator must share one bound so a sealed recipe can
        // never be refused after qualification, and raising the bound stays a
        // deliberate, reviewed decision.
        assert_eq!(SHELL_ARGUMENT_LIMIT, 32);
        let mut at_bound = config();
        at_bound.shell_arguments = (0..SHELL_ARGUMENT_LIMIT)
            .map(|index| OsString::from(format!("-c{index}")))
            .collect();
        assert!(at_bound.validate().is_ok());
        let mut over_bound = config();
        over_bound.shell_arguments = (0..SHELL_ARGUMENT_LIMIT + 1)
            .map(|index| OsString::from(format!("-c{index}")))
            .collect();
        let error = over_bound
            .validate()
            .err()
            .expect("over-bound arguments must refuse");
        assert_eq!(error.to_string(), "Too many shell arguments");
        // The contained command-line builder enforces the same shared bound.
        let executable = Path::new(r"C:\synthetic\codex.exe");
        let at_limit: Vec<OsString> = (0..SHELL_ARGUMENT_LIMIT)
            .map(|index| OsString::from(format!("-c{index}")))
            .collect();
        assert!(crate::windows_pty::command_line(executable, &at_limit).is_ok());
        let mut over_limit = at_limit;
        over_limit.push(OsString::from("-c-extra"));
        assert!(crate::windows_pty::command_line(executable, &over_limit).is_err());
    }
    struct RefusedNativeAdmission;
    impl WindowsNativeAdmission for RefusedNativeAdmission {
        fn for_launch(&self, _: &NativeTuiPermit) -> Result<Arc<dyn WindowsTerminalAdmission>> {
            bail!("Synthetic native admission refusal")
        }
    }
    #[test]
    fn native_admission_is_separate_once_only_and_cannot_enable_an_unconfigured_shell() {
        let host = WindowsTerminalHost::default();
        assert!(
            host.configure_native_admission(Arc::new(RefusedNativeAdmission))
                .is_ok()
        );
        assert!(
            host.configure_native_admission(Arc::new(RefusedNativeAdmission))
                .is_err()
        );
        assert!(host.start(Path::new("."), 24, 80).is_err());
        assert!(host.list().is_empty());
    }
}
