//! One bounded, retained owner worker. This module does not open a product Start
//! capability: the caller supplies a qualified recipe and C09R admission service.
#![cfg(windows)]
use crate::{
    resources::{
        admission::{CleanupStatus, PreparedWorkload, QueueStatus},
        job::{JobBudget, JobSnapshot},
        process::{ContainedProcess, NativeCommand, SpawnedProcess, spawn_contained},
        sampling::{Measurement, WindowsSampler},
    },
    sidecar_admin::{AdminTarget, in_flight},
    sidecar_runtime::SidecarRuntimeRecipe,
};
use serde::Serialize;
use serde_json::Value;
use shinatama_state::database::Database;
use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    num::NonZeroU16,
    os::windows::io::AsRawHandle,
    ptr::null_mut,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};
static CLAIMED: AtomicBool = AtomicBool::new(false);

/// Methods run only on this owner worker. `prepare` MUST check fresh governor
/// headroom and atomically reserve from the shared coordinator; never raise caps.
pub trait WorkloadProvider: Send + 'static {
    fn database(&self) -> &Database;
    fn prepare(
        &mut self,
        now_ms: i64,
        lease_until_ms: i64,
    ) -> Result<PreparedWorkload, AdmissionUnavailable>;
    /// The proxy's process ended by itself. Told once per exit, so the owner can record why
    /// in Shinatama's log (ADR-0030): what the child wrote is read nowhere else.
    fn exited(&mut self, _exit: &ProxyExit) {}
    /// A run of the archive cleanup ended (docs/superpowers/specs/2026-11-20-proxy-archive-cleanup-design.md).
    fn maintained(&mut self, _report: &crate::sidecar_archive::Report) {}
    /// Admission for one of the proxy's maintenance commands: the proxy's own unless the owner
    /// says otherwise (the close cleanup must be admitted while the application closes).
    fn prepare_maintenance(
        &mut self,
        now_ms: i64,
        lease_until_ms: i64,
    ) -> Result<PreparedWorkload, AdmissionUnavailable> {
        self.prepare(now_ms, lease_until_ms)
    }
    /// True while an agent session is open or opening: an idle proxy is stopped for a cleanup
    /// only when it is false, and a run that stopped one ends when it turns true. None: the
    /// owner cannot tell, and nothing is checked.
    fn sessions_active(&self) -> Option<Arc<dyn Fn() -> bool + Send + Sync>> {
        None
    }
}
#[derive(Debug)]
pub struct AdmissionUnavailable;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Stopped,
    Starting,
    Ready,
    Busy,
    /// A requested stop is waiting for in-flight requests to finish. Still the
    /// same live process, still the same pid.
    Draining,
    /// No proxy process runs: the owner is running the proxy's own commands that clean its
    /// archive. An open waits; its Start ends the run after the command in progress.
    Maintaining,
    Degraded,
    Recovering,
    Failed,
    Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    None,
    AdmissionDeferred,
    LeaseUnavailable,
    StartupRefused,
    StartupTimedOut,
    HealthUnavailable,
    ProcessExited,
    OutputLimit,
    ObservationUnavailable,
    CleanupUncertain,
    ResourceLimit,
    ListenerUnavailable,
    PortProbeUnavailable,
    DrainTimedOut,
    /// The supervisor's own loop panicked; `Snapshot::fault` says with what.
    SupervisorFault,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceLimitKind {
    Processes,
    JobCommit,
    ProcessCommit,
    QueueBytes,
    QueueItems,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceLimit {
    pub kind: ResourceLimitKind,
    pub observed: u64,
    pub applied: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Idle,
    RequestInFlight,
    SessionSave,
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Start,
    Stop,
    /// Stop once in-flight requests reach zero (or `drain_timeout` passes).
    Drain,
    Retry,
    Activity(Activity),
    /// Clean the proxy's archive while no proxy process runs: an idle proxy is stopped for it,
    /// one with requests in flight is left alone. `budget: None` runs until a Start is asked
    /// for; `restart` starts the proxy again after the run.
    Maintain {
        budget: Option<Duration>,
        restart: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    Full,
    Closed,
}
/// How the proxy's process last ended by itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProxyExit {
    pub code: u32,
    pub uptime_ms: u64,
    pub at_unix_ms: Option<i64>,
    /// What the runtime last said outside the proxy's own log (a refused start, Node's
    /// fatal error): one bounded line without control characters. None when it said nothing.
    pub detail: Option<String>,
    /// Exits seen by this owner since Shinatama started.
    pub count: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub phase: Phase,
    pub reason: Reason,
    pub activity: Activity,
    pub process_alive: Option<bool>,
    pub healthy: bool,
    pub restart_used: bool,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub observed_at_unix_ms: Option<i64>,
    /// Process id of the child this owner currently holds; None when it holds none.
    pub pid: Option<u32>,
    /// `inFlight` as last read from the sidecar; None when never read or unreadable.
    pub in_flight: Option<u64>,
    /// While Draining: when the drain gives up and stops anyway.
    pub drain_deadline_unix_ms: Option<i64>,
    pub resource_limit: Option<ResourceLimit>,
    /// Fresh bind result after a missed health response; never inferred from connect.
    pub port_held: Option<bool>,
    pub health_probe_delay_ms: u64,
    /// Kept after the proxy recovers: it says why the last interruption happened.
    pub last_exit: Option<ProxyExit>,
    /// What stopped the supervisor's own loop (a panic), one bounded line; cleared by the next
    /// start or Retry.
    pub fault: Option<String>,
}
impl Phase {
    /// Phases in which a healthy proxy backs a new launch. Busy is a healthy
    /// proxy with requests in flight (another agent is streaming); it must not
    /// block opening a second agent. A Busy that came from a missed health
    /// response carries `healthy == false` and is refused by `Snapshot::backs_launch`.
    pub fn backs_launch(self) -> bool {
        matches!(self, Phase::Ready | Phase::Busy)
    }
}
impl Snapshot {
    /// The one launch gate: a live, healthy, freshly observed owner in a phase
    /// that `Phase::backs_launch` accepts.
    pub fn backs_launch(&self, now_unix_ms: i64) -> bool {
        self.phase.backs_launch()
            && self.healthy
            && self.process_alive == Some(true)
            && self
                .observed_at_unix_ms
                .is_some_and(|at| at <= now_unix_ms && now_unix_ms - at <= 10_000)
    }
}
/// Longest panic text a snapshot carries.
const FAULT_CHARS: usize = 300;
/// A panic payload as one line: its text when it has one (control characters become spaces,
/// runs of spaces collapse), at most [`FAULT_CHARS`] UTF-16 units: the panel's own bound.
pub(crate) fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
    let Some(text) = text else {
        return "unknown panic payload".to_owned();
    };
    let spaced: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let mut units = 0;
    spaced
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take_while(|character| {
            units += character.len_utf16();
            units <= FAULT_CHARS
        })
        .collect()
}
/// The state a panic in the supervisor's loop leaves: Failed, unhealthy, and why. Start and
/// Retry recovery act on a Failed owner, so the panel keeps an action that works.
fn fault_snapshot(snapshot: &mut Snapshot, text: String) {
    snapshot.phase = Phase::Failed;
    snapshot.reason = Reason::SupervisorFault;
    snapshot.healthy = false;
    snapshot.fault = Some(text);
}
pub struct SupervisorConfig {
    pub port: NonZeroU16,
    pub poll_period: Duration,
    pub health_period: Duration,
    pub health_timeout: Duration,
    pub startup_timeout: Duration,
    pub graceful_stop_timeout: Duration,
    pub cleanup_timeout: Duration,
    /// Longest a requested drain waits for in-flight requests before it stops anyway.
    pub drain_timeout: Duration,
    pub reservation_lease: Duration,
    /// The archive cleanup's budget before the first start of the proxy; zero skips it.
    pub maintenance_at_start: Duration,
    /// Longest one of the proxy's maintenance commands may run.
    pub maintenance_command_timeout: Duration,
}
/// The bounds of the archive cleanup's settings.
pub(crate) fn maintenance_valid(at_start: Duration, command_timeout: Duration) -> bool {
    at_start <= Duration::from_secs(60)
        && (Duration::from_secs(10)..=Duration::from_secs(300)).contains(&command_timeout)
}
/// Whether a Start (or Retry) was asked for since the owner last took one: what ends a run of
/// the archive cleanup. Set by the caller's thread, read by the owner's.
#[derive(Clone, Default)]
pub(crate) struct StartWanted(Arc<AtomicBool>);
impl StartWanted {
    pub(crate) fn note(&self, command: &Command) {
        if matches!(command, Command::Start | Command::Retry) {
            self.0.store(true, Ordering::Release);
        }
    }
    pub(crate) fn is_set(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(crate) fn clear(&self) {
        self.0.store(false, Ordering::Release);
    }
    /// Whether it was set, clearing it: a run consumes the request it ended for.
    pub(crate) fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}
/// What ends a cleanup run after the command in progress: a Start or Retry, a Stop, the owner's
/// shutdown, a cleanup that could not be confirmed, or, in a run that stopped an idle proxy, an
/// agent session that opened meanwhile.
pub(crate) fn run_cancelled(
    start_wanted: bool,
    stop: bool,
    shutdown: bool,
    halted: bool,
    sessions_active: bool,
) -> bool {
    start_wanted || stop || shutdown || halted || sessions_active
}
/// Whether the proxy starts after a run: one that ran before it and is to be restarted, or one
/// a Start asked for meanwhile. A proxy the user stopped stays stopped, and nothing starts while
/// a Stop or the owner's shutdown is pending.
pub(crate) fn start_after_run(
    restart: bool,
    was_running: bool,
    start_wanted: bool,
    stopping: bool,
) -> bool {
    !stopping && ((restart && was_running) || start_wanted)
}
/// A run is one long turn of the owner: it says it is alive, or the panel and the opens take
/// the owner for stalled (no report for 15 s) and refuse what would end the run.
fn heartbeat(snapshot: &mut Snapshot, status: &Mutex<Snapshot>) {
    snapshot.observed_at_unix_ms = now();
    if let Ok(mut shown) = status.try_lock() {
        *shown = snapshot.clone();
    }
}
impl SupervisorConfig {
    fn valid(&self, recipe: &SidecarRuntimeRecipe) -> bool {
        (Duration::from_millis(10)..=Duration::from_secs(1)).contains(&self.poll_period)
            && (Duration::from_millis(100)..=Duration::from_secs(5)).contains(&self.health_period)
            && (Duration::from_millis(50)..=Duration::from_secs(2)).contains(&self.health_timeout)
            && (Duration::from_secs(1)..=Duration::from_secs(30)).contains(&self.startup_timeout)
            && self.graceful_stop_timeout <= Duration::from_secs(10)
            && self.cleanup_timeout <= Duration::from_secs(10)
            && (Duration::from_secs(1)..=Duration::from_secs(120)).contains(&self.drain_timeout)
            && self.reservation_lease >= self.startup_timeout
            && maintenance_valid(self.maintenance_at_start, self.maintenance_command_timeout)
            && self.reservation_lease <= Duration::from_secs(300)
            && recipe.arguments().get(4).is_some_and(|a| a == "--port")
            && recipe
                .arguments()
                .get(5)
                .is_some_and(|a| a == self.port.get().to_string().as_str())
    }
}
#[derive(Debug)]
pub enum StartError {
    InvalidConfiguration,
    WorkerAlreadyRetained,
    WorkerUnavailable,
}
pub struct SidecarSupervisor {
    commands: SyncSender<Command>,
    status: Arc<Mutex<Snapshot>>,
    shutdown: Arc<AtomicBool>,
    stop_requested: Arc<AtomicBool>,
    start_wanted: StartWanted,
}
impl SidecarSupervisor {
    pub fn start<P: WorkloadProvider>(
        recipe: SidecarRuntimeRecipe,
        config: SupervisorConfig,
        provider: P,
    ) -> Result<Self, StartError> {
        if !config.valid(&recipe) {
            return Err(StartError::InvalidConfiguration);
        }
        if CLAIMED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(StartError::WorkerAlreadyRetained);
        }
        let snapshot = Snapshot {
            phase: Phase::Stopped,
            reason: Reason::None,
            activity: Activity::Idle,
            process_alive: Some(false),
            healthy: false,
            restart_used: false,
            stdout_bytes: 0,
            stderr_bytes: 0,
            observed_at_unix_ms: now(),
            pid: None,
            in_flight: None,
            drain_deadline_unix_ms: None,
            resource_limit: None,
            port_held: None,
            health_probe_delay_ms: config.health_period.as_millis() as u64,
            last_exit: None,
            fault: None,
        };
        let status = Arc::new(Mutex::new(snapshot.clone()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop_requested = Arc::new(AtomicBool::new(false));
        let start_wanted = StartWanted::default();
        let (commands, receiver) = mpsc::sync_channel(4);
        let worker = Worker {
            recipe,
            config,
            provider,
            receiver,
            status: status.clone(),
            shutdown: shutdown.clone(),
            stop_requested: stop_requested.clone(),
            snapshot,
            running: None,
            retained: None,
            requested_start: false,
            drain: DrainState::default(),
            next_health: Instant::now(),
            started: Instant::now(),
            ever_healthy: false,
            next_renew: Instant::now(),
            lease_valid: true,
            queue_limits: (0, 0),
            busy_backoff: BusyBackoff::default(),
            recovery: RecoveryBackoff::default(),
            stderr_tail: OutputTail::default(),
            exits: 0,
            faulted: false,
            start_wanted: start_wanted.clone(),
            maintained_at_start: false,
        };
        if thread::Builder::new()
            .name("shinatama-sidecar-owner".into())
            .spawn(move || worker.run())
            .is_err()
        {
            CLAIMED.store(false, Ordering::Release);
            return Err(StartError::WorkerUnavailable);
        }
        Ok(Self {
            commands,
            status,
            shutdown,
            stop_requested,
            start_wanted,
        })
    }
    pub fn request(&self, command: Command) -> Result<(), RequestError> {
        // Before the send: a run of the archive cleanup sees it while the command waits.
        self.start_wanted.note(&command);
        if matches!(command, Command::Stop) {
            self.stop_requested.store(true, Ordering::Release);
            return Ok(());
        }
        self.commands.try_send(command).map_err(|e| match e {
            TrySendError::Full(_) => RequestError::Full,
            TrySendError::Disconnected(_) => RequestError::Closed,
        })
    }
    /// Never waits behind SQL, native sampling, HTTP or cleanup.
    pub fn status(&self) -> Option<Snapshot> {
        self.status.try_lock().ok().map(|value| value.clone())
    }
}
impl Drop for SidecarSupervisor {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}
enum Retained {
    Process(ContainedProcess),
    Reservation(PreparedWorkload),
}
struct Worker<P> {
    recipe: SidecarRuntimeRecipe,
    config: SupervisorConfig,
    provider: P,
    receiver: Receiver<Command>,
    status: Arc<Mutex<Snapshot>>,
    shutdown: Arc<AtomicBool>,
    stop_requested: Arc<AtomicBool>,
    snapshot: Snapshot,
    running: Option<SpawnedProcess>,
    retained: Option<Retained>,
    requested_start: bool,
    drain: DrainState,
    next_health: Instant,
    started: Instant,
    ever_healthy: bool,
    next_renew: Instant,
    lease_valid: bool,
    queue_limits: (u64, u32),
    busy_backoff: BusyBackoff,
    recovery: RecoveryBackoff,
    stderr_tail: OutputTail,
    exits: u32,
    /// A panic stopped a turn: no recovery or health work until a command arrives.
    faulted: bool,
    /// Set by `request(Start | Retry)`; cleared when the owner takes one. Ends a cleanup run.
    start_wanted: StartWanted,
    /// The cleanup before the first start of the proxy has run.
    maintained_at_start: bool,
}
impl<P: WorkloadProvider> Worker<P> {
    fn publish(&mut self, phase: Phase, reason: Reason) {
        if !self.snapshot.healthy {
            self.recovery.unhealthy();
        }
        // Health keeps answering while a drain waits: that is still Draining.
        let phase = if self.drain.active() && matches!(phase, Phase::Ready | Phase::Busy) {
            Phase::Draining
        } else {
            phase
        };
        if !self.drain.active() {
            self.snapshot.drain_deadline_unix_ms = None;
        }
        self.snapshot.phase = phase;
        self.snapshot.reason = reason;
        self.snapshot.observed_at_unix_ms = now();
        if let Ok(mut status) = self.status.try_lock() {
            *status = self.snapshot.clone();
        }
    }
    /// Runs [`Worker::turn`] until the worker ends. A panic inside a turn used to end this thread
    /// silently and freeze its last snapshot, leaving the panel with no working button; now it is
    /// a Failed owner that says why, and Start or Retry recovery act on it.
    fn run(mut self) {
        loop {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.turn())) {
                Ok(true) => {}
                Ok(false) => return,
                Err(payload) => self.fault(panic_text(&*payload)),
            }
        }
    }
    /// After a panic: Failed with its text, no recovery scheduled, and no further work until a
    /// command arrives, so a panic that repeats on every turn cannot spin or flood the log.
    fn fault(&mut self, text: String) {
        self.requested_start = false;
        self.recovery.cancel();
        self.faulted = true;
        fault_snapshot(&mut self.snapshot, text);
        self.publish(Phase::Failed, Reason::SupervisorFault);
    }
    /// One iteration of the owner loop; false once the worker has ended.
    fn turn(&mut self) -> bool {
        {
            if self.shutdown.load(Ordering::Acquire) {
                self.stop(true);
                if self.retained.is_some() {
                    // Uncertain capacity/handles stay owned. No duplicate worker,
                    // SQL retry loop, detached replacement or fabricated release.
                    self.publish(Phase::Retired, Reason::CleanupUncertain);
                    loop {
                        thread::park();
                    }
                }
                CLAIMED.store(false, Ordering::Release);
                return false;
            }
            let received = self.receiver.recv_timeout(self.config.poll_period);
            if matches!(&received, Ok(command) if !matches!(command, Command::Activity(_))) {
                self.faulted = false;
            }
            match received {
                Ok(Command::Stop) => {
                    self.requested_start = false;
                    self.recovery.cancel();
                    self.stop(true);
                }
                Ok(Command::Drain) => self.drain_requested(),
                Ok(Command::Start) => {
                    self.start_wanted.clear();
                    let effect = self.drain.command(
                        self.snapshot.phase,
                        Command::Start,
                        Instant::now(),
                        self.config.drain_timeout,
                    );
                    if effect == DrainEffect::Cancel {
                        self.publish(Phase::Ready, Reason::None);
                    } else if self.snapshot.phase == Phase::Stopped {
                        self.recovery = RecoveryBackoff::default();
                        self.requested_start = true;
                    }
                }
                Ok(Command::Retry) => {
                    self.start_wanted.clear();
                    self.snapshot.fault = None;
                    if self.retained.is_some()
                        || (self.running.is_none() && self.snapshot.phase == Phase::Failed)
                    {
                        self.snapshot.restart_used = false;
                        self.recovery.cancel();
                        self.requested_start = true;
                    } else {
                        self.next_health = Instant::now();
                    }
                }
                Ok(Command::Activity(activity)) => self.snapshot.activity = activity,
                Ok(Command::Maintain { budget, restart }) => {
                    self.maintain_requested(budget, restart)
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.shutdown.store(true, Ordering::Release)
                }
                _ => {}
            }
            if self.stop_requested.swap(false, Ordering::AcqRel) {
                self.requested_start = false;
                self.recovery.cancel();
                self.faulted = false;
                for _ in 0..4 {
                    let _ = self.receiver.try_recv();
                }
                self.stop(true);
                return true;
            }
            if self.faulted {
                // A process may still run after the panic: keep reading its output so it never
                // blocks on a full pipe, and keep the Failed state current for the panel.
                if let Some(running) = self.running.as_mut() {
                    let _ = drain(
                        running,
                        &mut self.snapshot.stdout_bytes,
                        &mut self.snapshot.stderr_bytes,
                        &mut self.stderr_tail,
                    );
                }
                self.publish(Phase::Failed, Reason::SupervisorFault);
                return true;
            }
            if self.requested_start && self.running.is_none() {
                if self.recovery.ready(Instant::now()) {
                    if self.retained.is_some() {
                        self.retry_cleanup();
                    }
                    if self.retained.is_none() && !self.maintained_at_start {
                        self.maintained_at_start = true;
                        let _ = self.maintain(Some(self.config.maintenance_at_start), None);
                    }
                    if self.retained.is_none() {
                        // A Stop or the shutdown that ended that run is taken at the next turn.
                        if !self.stop_requested.load(Ordering::Acquire)
                            && !self.shutdown.load(Ordering::Acquire)
                        {
                            self.requested_start = false;
                            self.launch();
                        }
                    } else {
                        self.recover(Reason::CleanupUncertain);
                    }
                } else {
                    // A scheduled retry is still a live supervisor observation.
                    self.publish(self.snapshot.phase, self.snapshot.reason);
                }
            }
            self.tick();
            self.drain_step();
        }
        true
    }
    /// `Command::Maintain`: an idle proxy is stopped for the run, one with requests in flight
    /// (or not read) is left alone, and nothing runs while a cleanup is unconfirmed or a drain
    /// is in progress.
    fn maintain_requested(&mut self, budget: Option<Duration>, restart: bool) {
        if self.retained.is_some() || self.drain.active() {
            return;
        }
        let was_running = self.running.is_some();
        let activity = if was_running {
            self.provider.sessions_active()
        } else {
            None
        };
        if was_running {
            // An agent opening has no request in flight yet: it is asked for separately.
            if self.snapshot.in_flight != Some(0)
                || activity.as_ref().is_some_and(|active| active())
            {
                return;
            }
            // Read-only while the proxy runs: it is stopped only for something to remove.
            let natives = self
                .provider
                .database()
                .segment_native_ids()
                .map_err(|_| ());
            if !crate::sidecar_archive::pending(&self.recipe, natives)
                .is_some_and(|count| count > 0)
            {
                return;
            }
            self.stop(true);
            if self.running.is_some() || self.retained.is_some() {
                return;
            }
        }
        let wanted = self.maintain(budget, activity);
        let stopping =
            self.stop_requested.load(Ordering::Acquire) || self.shutdown.load(Ordering::Acquire);
        if self.retained.is_none() && start_after_run(restart, was_running, wanted, stopping) {
            self.recovery = RecoveryBackoff::default();
            self.requested_start = true;
        }
    }
    /// One run of the archive cleanup (`sidecar_archive`) while this owner runs no proxy
    /// process: two processes writing the archive corrupt it. A zero budget skips it. True when
    /// a Start was asked for meanwhile (the run consumed that request).
    fn maintain(
        &mut self,
        budget: Option<Duration>,
        activity: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    ) -> bool {
        if self.running.is_some() || self.retained.is_some() || budget == Some(Duration::ZERO) {
            return false;
        }
        self.publish(Phase::Maintaining, Reason::None);
        let deadline = budget.map(|budget| Instant::now() + budget);
        let natives = self
            .provider
            .database()
            .segment_native_ids()
            .map_err(|_| ());
        let halted = std::cell::Cell::new(false);
        let mut retained = None;
        let report = {
            let wanted = &self.start_wanted;
            let stop = &self.stop_requested;
            let shutdown = &self.shutdown;
            let cancelled = || {
                run_cancelled(
                    wanted.is_set(),
                    stop.load(Ordering::Acquire),
                    shutdown.load(Ordering::Acquire),
                    halted.get(),
                    activity.as_ref().is_some_and(|active| active()),
                )
            };
            let snapshot = &mut self.snapshot;
            let status = &self.status;
            let mut beat = || heartbeat(snapshot, status);
            let provider = &mut self.provider;
            let config = &self.config;
            let mut execute = |command: NativeCommand| {
                let output =
                    run_maintenance_command(provider, config, command, &mut retained, &mut beat);
                if retained.is_some() {
                    halted.set(true);
                }
                output
            };
            crate::sidecar_archive::run(&self.recipe, natives, deadline, &cancelled, &mut execute)
        };
        self.provider.maintained(&report);
        self.snapshot.process_alive = Some(false);
        if let Some(retained) = retained {
            self.retained = Some(retained);
            self.publish(Phase::Retired, Reason::CleanupUncertain);
        } else {
            self.publish(Phase::Stopped, Reason::None);
        }
        self.start_wanted.take()
    }
    fn drain_requested(&mut self) {
        let began = Instant::now();
        match self.drain.command(
            self.snapshot.phase,
            Command::Drain,
            began,
            self.config.drain_timeout,
        ) {
            DrainEffect::Begin => {
                self.snapshot.drain_deadline_unix_ms = now().and_then(|at| {
                    at.checked_add(i64::try_from(self.config.drain_timeout.as_millis()).ok()?)
                });
                self.publish(Phase::Draining, Reason::None);
            }
            DrainEffect::StopNow => {
                self.requested_start = false;
                self.recovery.cancel();
                self.stop(true);
            }
            DrainEffect::Cancel | DrainEffect::Unaffected => {}
        }
    }
    /// One drain poll per owner iteration: read `inFlight`, and stop once the drain
    /// has seen idle twice in a row or its deadline has passed.
    fn drain_step(&mut self) {
        let Some(pid) = self.snapshot.pid.filter(|_| self.running.is_some()) else {
            return;
        };
        let target = AdminTarget {
            port: self.config.port,
            pid,
            timeout: self.config.health_timeout,
        };
        let Some((reading, step)) = self.drain.poll(&target) else {
            return;
        };
        self.snapshot.in_flight = reading;
        self.snapshot.activity = activity_for(reading, self.snapshot.activity);
        match step {
            DrainStep::Waiting => self.publish(self.snapshot.phase, self.snapshot.reason),
            DrainStep::Idle | DrainStep::TimedOut => {
                self.stop(true);
                // Only a clean Stopped carries the drain's own outcome; Retired keeps its reason.
                if step.reason() != Reason::None && self.snapshot.phase == Phase::Stopped {
                    self.publish(Phase::Stopped, step.reason());
                }
            }
        }
    }
    fn launch(&mut self) {
        self.snapshot.fault = None;
        self.publish(Phase::Starting, Reason::None);
        self.snapshot.healthy = false;
        self.snapshot.process_alive = None;
        self.snapshot.pid = None;
        self.snapshot.in_flight = None;
        self.drain.clear();
        self.ever_healthy = false;
        self.snapshot.resource_limit = None;
        self.snapshot.port_held = None;
        self.busy_backoff = BusyBackoff::default();
        self.snapshot.health_probe_delay_ms = self.config.health_period.as_millis() as u64;
        let command = NativeCommand::new(
            self.recipe.executable().to_owned(),
            self.recipe.arguments().to_vec(),
            self.recipe.working_directory().to_owned(),
            self.recipe
                .child_environment()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        );
        let Ok(command) = command else {
            self.publish(Phase::Failed, Reason::StartupRefused);
            return;
        };
        let Some(at) = now() else {
            self.recover(Reason::ObservationUnavailable);
            return;
        };
        let until = at.saturating_add(self.config.reservation_lease.as_millis() as i64);
        let Ok(workload) = self.provider.prepare(at, until) else {
            self.recover(Reason::AdmissionDeferred);
            return;
        };
        // Shared admission may stamp the reservation AFTER the request time.
        // Never use the pre-admission timestamp as proof it is ready to start.
        let at = now().unwrap_or(-1);
        let until = at.saturating_add(self.config.reservation_lease.as_millis() as i64);
        self.queue_limits = (
            workload.ticket().budget.queue_bytes,
            workload.ticket().budget.queue_items,
        );
        match spawn_contained(
            command.high_priority(),
            workload,
            self.provider.database(),
            at,
            until,
        ) {
            Ok(running) => {
                self.running = Some(running);
                self.started = Instant::now();
                self.next_health = self.started;
                self.next_renew = self.started + self.config.reservation_lease / 3;
                self.lease_valid = true;
                self.snapshot.stdout_bytes = 0;
                self.snapshot.stderr_bytes = 0;
                self.stderr_tail.clear();
                self.snapshot.process_alive = Some(true);
                self.snapshot.pid = self
                    .running
                    .as_ref()
                    .map(|running| running.process.key().pid);
            }
            Err(mut failure) => {
                if !matches!(
                    failure
                        .workload
                        .finish_cleanup(self.provider.database(), self.config.cleanup_timeout),
                    Ok(CleanupStatus::Released { .. })
                ) {
                    self.retained = Some(Retained::Reservation(failure.workload));
                    self.recover(Reason::CleanupUncertain);
                } else {
                    self.recover(Reason::StartupRefused);
                }
            }
        }
    }
    fn tick(&mut self) {
        let draining = self.drain.active();
        let Some(running) = &mut self.running else {
            return;
        };
        let observed = running.process.try_wait();
        if let Ok(Some(code)) = &observed {
            // What it wrote last is still in its pipes: read it before they are closed.
            for _ in 0..64 {
                let before = self.snapshot.stderr_bytes;
                if !drain(
                    running,
                    &mut self.snapshot.stdout_bytes,
                    &mut self.snapshot.stderr_bytes,
                    &mut self.stderr_tail,
                ) || self.snapshot.stderr_bytes == before
                {
                    break;
                }
            }
            self.exits = self.exits.saturating_add(1);
            let exit = ProxyExit {
                code: *code,
                uptime_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
                at_unix_ms: now(),
                detail: self.stderr_tail.diagnostic(),
                count: self.exits,
            };
            self.provider.exited(&exit);
            self.snapshot.last_exit = Some(exit);
            // A process that exits while draining has done what the drain asked for.
            self.drain.clear();
            self.snapshot.process_alive = Some(false);
            self.snapshot.healthy = false;
            self.publish(Phase::Recovering, Reason::ProcessExited);
            self.stop(false);
            if !draining {
                self.recover(Reason::ProcessExited);
            }
            return;
        }
        if observed.is_err() {
            self.snapshot.process_alive = None;
            self.snapshot.healthy = false;
            self.snapshot.in_flight = None; // a degraded reading must not keep an old count.
            self.publish(Phase::Degraded, Reason::ObservationUnavailable);
            return;
        }
        self.snapshot.process_alive = Some(true);
        let Some(applied) = running.process.applied_budget() else {
            self.snapshot.healthy = false;
            self.snapshot.in_flight = None; // a degraded reading must not keep an old count.
            self.publish(Phase::Degraded, Reason::ObservationUnavailable);
            return;
        };
        let Ok(sample) = running.process.sample(&WindowsSampler::new()) else {
            self.snapshot.healthy = false;
            self.snapshot.in_flight = None; // a degraded reading must not keep an old count.
            self.publish(Phase::Degraded, Reason::ObservationUnavailable);
            return;
        };
        if !sample.membership_complete {
            self.snapshot.healthy = false;
            self.snapshot.in_flight = None; // a degraded reading must not keep an old count.
            self.publish(Phase::Degraded, Reason::ObservationUnavailable);
            return;
        }
        if let Some(violation) = resource_limit(
            &sample,
            applied,
            running.process.queue_credits().status(),
            self.queue_limits,
        ) {
            self.snapshot.resource_limit = Some(violation);
            self.snapshot.healthy = false;
            self.requested_start = false;
            self.publish(Phase::Recovering, Reason::ResourceLimit);
            self.stop(false); // exact retained process/job only; never another PID.
            if !draining {
                self.recover(Reason::ResourceLimit);
            }
            return; // Every replacement must pass fresh admission with the same caps.
        }
        if Instant::now() >= self.next_renew {
            self.next_renew = Instant::now() + self.config.reservation_lease / 3;
            self.lease_valid = now().is_some_and(|at| {
                running
                    .process
                    .renew(
                        self.provider.database(),
                        at,
                        at.saturating_add(self.config.reservation_lease.as_millis() as i64),
                    )
                    .is_ok()
            });
        }
        if !self.lease_valid {
            self.snapshot.healthy = false;
            self.snapshot.in_flight = None; // a degraded reading must not keep an old count.
            self.publish(Phase::Degraded, Reason::LeaseUnavailable);
            return;
        }
        if !drain(
            running,
            &mut self.snapshot.stdout_bytes,
            &mut self.snapshot.stderr_bytes,
            &mut self.stderr_tail,
        ) {
            self.snapshot.healthy = false;
            self.stop(false);
            if !draining {
                self.recover(Reason::OutputLimit);
            }
            return;
        }
        if Instant::now() < self.next_health {
            // Fresh native ownership/resource observation, without another TCP
            // health probe. Busy's8s backoff must not make a live worker stale.
            self.publish(self.snapshot.phase, self.snapshot.reason);
            return;
        }
        let probe_started = Instant::now();
        self.next_health = probe_started + self.config.health_period;
        self.snapshot.healthy = health(
            self.config.port,
            running.process.key().pid,
            self.config.health_timeout,
        );
        if self.snapshot.healthy {
            self.recovery.healthy(Instant::now());
            self.ever_healthy = true;
            self.busy_backoff.healthy();
            self.snapshot.port_held = None;
            self.snapshot.health_probe_delay_ms = self.config.health_period.as_millis() as u64;
            let reading = in_flight(&AdminTarget {
                port: self.config.port,
                pid: running.process.key().pid,
                timeout: self.config.health_timeout,
            });
            self.snapshot.in_flight = reading;
            self.snapshot.activity = activity_for(reading, self.snapshot.activity);
            self.publish(
                if self.snapshot.activity == Activity::Idle {
                    Phase::Ready
                } else {
                    Phase::Busy
                },
                Reason::None,
            );
        } else {
            self.recovery.unhealthy();
            // No fresh reading: an old count must not survive a failed probe.
            self.snapshot.in_flight = None;
            // The process may have exited DURING the bounded HTTP attempt.
            // Only this exact handle can establish death; health/bind cannot.
            match running.process.try_wait() {
                Ok(None) => self.snapshot.process_alive = Some(true),
                Ok(Some(_)) => {
                    self.snapshot.process_alive = Some(false);
                    self.publish(Phase::Recovering, Reason::ProcessExited);
                    return;
                }
                Err(_) => {
                    self.snapshot.process_alive = None;
                    self.publish(Phase::Degraded, Reason::ObservationUnavailable);
                    return;
                }
            }
            let port = probe_port(self.config.port);
            self.snapshot.port_held = match port {
                PortState::Occupied => Some(true),
                PortState::Free => Some(false),
                PortState::Unavailable => None,
            };
            let (phase, mut reason, delay) = self
                .busy_backoff
                .missed_while_alive(port, self.config.health_period);
            if port == PortState::Occupied
                && !self.ever_healthy
                && self.started.elapsed() >= self.config.startup_timeout
            {
                reason = Reason::StartupTimedOut;
            }
            self.snapshot.health_probe_delay_ms = delay.as_millis() as u64;
            self.next_health = probe_started + delay;
            // Keep the same owner for BOTH occupied and free ports while alive.
            // No prepare/start/replay path is reachable from this transition.
            self.publish(phase, reason);
        }
    }
    /// Retry the proxy only, on its original address. Agent commands are never replayed.
    /// Cleanup stays owned and must complete before another reservation is admitted.
    fn recover(&mut self, reason: Reason) {
        self.snapshot.restart_used = true;
        self.requested_start = true;
        self.recovery.schedule(Instant::now());
        let reason = if self.retained.is_some() {
            Reason::CleanupUncertain
        } else {
            reason
        };
        self.publish(Phase::Recovering, reason);
    }
    fn stop(&mut self, graceful: bool) {
        self.drain.clear();
        self.snapshot.in_flight = None;
        let Some(mut running) = self.running.take() else {
            if self.retained.is_none() {
                self.snapshot.healthy = false;
                self.snapshot.process_alive = Some(false);
                self.snapshot.activity = Activity::Idle;
                self.publish(Phase::Stopped, Reason::None);
            }
            return;
        };
        self.snapshot.healthy = false;
        self.snapshot.pid = None;
        self.publish(
            Phase::Recovering,
            if self.snapshot.resource_limit.is_some() {
                Reason::ResourceLimit
            } else {
                Reason::None
            },
        );
        if graceful && matches!(running.process.try_wait(), Ok(None)) {
            // Identity check and shutdown share ONE TCP connection: never send
            // shutdown to an unrelated listener that later reuses the same port.
            graceful_shutdown(
                self.config.port,
                running.process.key().pid,
                self.config.health_timeout,
            );
            let until = Instant::now() + self.config.graceful_stop_timeout;
            while Instant::now() < until && matches!(running.process.try_wait(), Ok(None)) {
                if !drain(
                    &mut running,
                    &mut self.snapshot.stdout_bytes,
                    &mut self.snapshot.stderr_bytes,
                    &mut self.stderr_tail,
                ) {
                    break;
                }
                thread::sleep(self.config.poll_period.min(Duration::from_millis(20)));
            }
        }
        let SpawnedProcess {
            mut process,
            stdin,
            stdout,
            stderr,
        } = running;
        drop((stdin, stdout, stderr)); // No reader payload/credit survives DB release.
        if matches!(
            process.finish_cleanup(self.provider.database(), self.config.cleanup_timeout),
            Ok(CleanupStatus::Released { .. })
        ) {
            self.snapshot.process_alive = Some(false);
            self.snapshot.activity = Activity::Idle;
            self.publish(Phase::Stopped, Reason::None);
        } else {
            self.snapshot.process_alive = process.try_wait().ok().map(|exit| exit.is_none());
            self.retained = Some(Retained::Process(process));
            self.publish(Phase::Retired, Reason::CleanupUncertain);
        }
    }
    fn retry_cleanup(&mut self) {
        self.publish(Phase::Recovering, Reason::CleanupUncertain);
        let released = match self.retained.as_mut() {
            Some(Retained::Process(process)) => matches!(
                process.retry_cleanup(self.provider.database(), self.config.cleanup_timeout),
                Ok(CleanupStatus::Released { .. })
            ),
            Some(Retained::Reservation(workload)) => matches!(
                workload.retry_cleanup(self.provider.database(), self.config.cleanup_timeout),
                Ok(CleanupStatus::Released { .. })
            ),
            None => false,
        };
        if released {
            self.retained = None;
            self.snapshot.process_alive = Some(false);
            self.publish(Phase::Stopped, Reason::None);
        } else {
            self.publish(Phase::Retired, Reason::CleanupUncertain);
        }
    }
}
/// One of the proxy's maintenance commands, admitted and contained as the proxy is (`launch`)
/// at the usual background priority: its exit code (None when it did not end in time) and the
/// start of its stdout. A cleanup that is not confirmed is kept in `retained`, as the proxy's.
fn run_maintenance_command<P: WorkloadProvider>(
    provider: &mut P,
    config: &SupervisorConfig,
    command: NativeCommand,
    retained: &mut Option<Retained>,
    beat: &mut dyn FnMut(),
) -> Result<crate::sidecar_archive::CommandOutput, ()> {
    beat();
    let lease = |at: i64| at.saturating_add(config.reservation_lease.as_millis() as i64);
    let at = now().ok_or(())?;
    let workload = provider
        .prepare_maintenance(at, lease(at))
        .map_err(|_| ())?;
    let at = now().unwrap_or(-1);
    let mut spawned = match spawn_contained(command, workload, provider.database(), at, lease(at)) {
        Ok(spawned) => spawned,
        Err(mut failure) => {
            if !matches!(
                failure
                    .workload
                    .finish_cleanup(provider.database(), config.cleanup_timeout),
                Ok(CleanupStatus::Released { .. })
            ) {
                *retained = Some(Retained::Reservation(failure.workload));
            }
            return Err(());
        }
    };
    let began = Instant::now();
    let mut last_beat = began;
    let mut stdout = Vec::new();
    let mut stderr = 0;
    let mut tail = OutputTail::default();
    let code = loop {
        if last_beat.elapsed() >= Duration::from_millis(500) {
            beat();
            last_beat = Instant::now();
        }
        let exit = spawned.process.try_wait();
        let read = spawned
            .process
            .queue_credits()
            .acquire(8192, 2)
            .is_ok_and(|_credit| {
                read_pipe(
                    &mut spawned.stdout,
                    &mut stdout,
                    crate::sidecar_archive::STDOUT_LIMIT,
                ) && drain_pipe(&mut spawned.stderr, &mut stderr, Some(&mut tail))
            });
        match exit {
            // Read once more after the exit: what it wrote last is in the pipe.
            Ok(Some(code)) if read => break Some(code),
            Ok(None) if read && began.elapsed() < config.maintenance_command_timeout => {
                thread::sleep(Duration::from_millis(20))
            }
            _ => break None,
        }
    };
    let SpawnedProcess {
        mut process,
        stdin,
        stdout: out,
        stderr: err,
    } = spawned;
    drop((stdin, out, err));
    if !matches!(
        process.finish_cleanup(provider.database(), config.cleanup_timeout),
        Ok(CleanupStatus::Released { .. })
    ) {
        *retained = Some(Retained::Process(process));
    }
    Ok(crate::sidecar_archive::CommandOutput { code, stdout })
}
/// Reads what is waiting in `pipe` into `into`, keeping at most `limit` bytes; the rest is
/// read and dropped. False only when the pipe itself fails (as `drain_pipe`).
fn read_pipe(pipe: &mut File, into: &mut Vec<u8>, limit: usize) -> bool {
    let mut buffer = [0; 16 * 1024];
    for _ in 0..16 {
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        } == 0
        {
            return std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_BROKEN_PIPE as i32);
        }
        if available == 0 {
            return true;
        }
        let count = (available as usize).min(buffer.len());
        let Ok(read) = pipe.read(&mut buffer[..count]) else {
            return false;
        };
        let room = limit.saturating_sub(into.len());
        into.extend_from_slice(&buffer[..read.min(room)]);
    }
    true
}
/// Repeated failures keep recovering; the delay rises to a maximum of 30 seconds.
/// A full minute of observed health restores the initial one-second retry.
#[derive(Default)]
struct RecoveryBackoff {
    failures: u32,
    next: Option<Instant>,
    healthy_since: Option<Instant>,
}
impl RecoveryBackoff {
    fn schedule(&mut self, now: Instant) {
        let seconds = (1u64 << self.failures.min(5)).min(30);
        self.failures = self.failures.saturating_add(1);
        self.next = Some(now + Duration::from_secs(seconds));
        self.healthy_since = None;
    }
    fn ready(&self, now: Instant) -> bool {
        self.next.is_none_or(|next| now >= next)
    }
    fn cancel(&mut self) {
        self.next = None;
        self.healthy_since = None;
    }
    fn unhealthy(&mut self) {
        self.healthy_since = None;
    }
    fn healthy(&mut self, now: Instant) {
        self.next = None;
        let since = self.healthy_since.get_or_insert(now);
        if now.duration_since(*since) >= Duration::from_secs(60) {
            self.failures = 0;
        }
    }
}
/// What a health read of `inFlight` says about activity. Unknown changes nothing.
fn activity_for(reading: Option<u64>, current: Activity) -> Activity {
    match reading {
        Some(0) => Activity::Idle,
        Some(_) => Activity::RequestInFlight,
        None => current,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrainStep {
    Waiting,
    Idle,
    TimedOut,
}
impl DrainStep {
    /// The reason the owner reports once this step has stopped the process.
    fn reason(self) -> Reason {
        match self {
            DrainStep::TimedOut => Reason::DrainTimedOut,
            DrainStep::Waiting | DrainStep::Idle => Reason::None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrainEffect {
    Begin,
    Cancel,
    StopNow,
    Unaffected,
}
/// One drain: ends after two consecutive zero reads or at the deadline.
struct DrainWatch {
    deadline: Instant,
    zero_reads: u8,
}
impl DrainWatch {
    fn begin(now: Instant, timeout: Duration) -> Self {
        Self {
            deadline: now + timeout,
            zero_reads: 0,
        }
    }
    /// An unknown or non-zero read never counts as idle and restarts the count.
    fn observe(&mut self, now: Instant, reading: Option<u64>) -> DrainStep {
        self.zero_reads = if reading == Some(0) {
            self.zero_reads.saturating_add(1)
        } else {
            0
        };
        if self.zero_reads >= 2 {
            DrainStep::Idle
        } else if now >= self.deadline {
            DrainStep::TimedOut
        } else {
            DrainStep::Waiting
        }
    }
}
/// The owner's drain bookkeeping; no process, no worker thread.
#[derive(Default)]
struct DrainState {
    watch: Option<DrainWatch>,
}
impl DrainState {
    fn active(&self) -> bool {
        self.watch.is_some()
    }
    fn clear(&mut self) {
        self.watch = None;
    }
    /// Drain from Ready/Busy begins one; from any other phase it is a stop. Start
    /// cancels a drain in progress. Everything else leaves a drain alone.
    fn command(
        &mut self,
        phase: Phase,
        command: Command,
        now: Instant,
        timeout: Duration,
    ) -> DrainEffect {
        match command {
            Command::Drain if self.active() => DrainEffect::Unaffected,
            Command::Drain if matches!(phase, Phase::Ready | Phase::Busy) => {
                self.watch = Some(DrainWatch::begin(now, timeout));
                DrainEffect::Begin
            }
            Command::Drain => {
                self.clear();
                DrainEffect::StopNow
            }
            Command::Start if self.active() => {
                self.clear();
                DrainEffect::Cancel
            }
            _ => DrainEffect::Unaffected,
        }
    }
    /// One read of `inFlight` and its verdict; None when no drain is in progress.
    /// A finished drain clears itself.
    fn poll(&mut self, target: &AdminTarget) -> Option<(Option<u64>, DrainStep)> {
        let watch = self.watch.as_mut()?;
        let reading = in_flight(target);
        let step = watch.observe(Instant::now(), reading);
        if step != DrainStep::Waiting {
            self.watch = None;
        }
        Some((reading, step))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortState {
    Occupied,
    Free,
    Unavailable,
}
fn probe_port(port: NonZeroU16) -> PortState {
    // Literal loopback only: one bind, no DNS/connect/accept/read, immediate drop.
    // A permission or resource error is unknown, never proof of a free port.
    match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port.get())) {
        Ok(listener) => {
            let nonblocking = listener.set_nonblocking(true).is_ok();
            drop(listener);
            if nonblocking {
                PortState::Free
            } else {
                PortState::Unavailable
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => PortState::Occupied,
        Err(_) => PortState::Unavailable,
    }
}
#[derive(Default)]
struct BusyBackoff {
    step: u8,
}
impl BusyBackoff {
    fn healthy(&mut self) {
        self.step = 0;
    }
    fn missed_while_alive(
        &mut self,
        port: PortState,
        normal: Duration,
    ) -> (Phase, Reason, Duration) {
        match port {
            PortState::Occupied => {
                let delay = Duration::from_secs(1u64 << self.step);
                self.step = (self.step + 1).min(3);
                (Phase::Busy, Reason::HealthUnavailable, delay)
            }
            PortState::Free => (Phase::Degraded, Reason::ListenerUnavailable, normal),
            PortState::Unavailable => (Phase::Degraded, Reason::PortProbeUnavailable, normal),
        }
    }
}
fn resource_limit(
    sample: &JobSnapshot,
    applied: JobBudget,
    queues: QueueStatus,
    queue_limits: (u64, u32),
) -> Option<ResourceLimit> {
    let exceeded = |kind, observed, limit| {
        (observed > limit).then_some(ResourceLimit {
            kind,
            observed,
            applied: limit,
        })
    };
    // An advisory budget is a share of the ledger, not a quota: every session's model
    // traffic goes through the proxy, so its memory or process count never stops it.
    let quota = || {
        exceeded(
            ResourceLimitKind::Processes,
            u64::from(sample.active_processes),
            u64::from(applied.max_processes()),
        )
        .or_else(|| {
            exceeded(
                ResourceLimitKind::JobCommit,
                sample.observed_private_commit_bytes,
                applied.job_commit_bytes(),
            )
        })
        .or_else(|| {
            sample
                .processes
                .iter()
                .find_map(|process| match process.counters {
                    Measurement::Available(counters) => exceeded(
                        ResourceLimitKind::ProcessCommit,
                        counters.private_commit_bytes,
                        applied.process_commit_bytes(),
                    ),
                    Measurement::Unavailable(_) => None,
                })
        })
    };
    let quota = if applied.limits_enforced() {
        quota()
    } else {
        None
    };
    quota
        .or_else(|| {
            exceeded(
                ResourceLimitKind::QueueBytes,
                queues.current_bytes,
                queue_limits.0,
            )
        })
        .or_else(|| {
            exceeded(
                ResourceLimitKind::QueueItems,
                u64::from(queues.current_items),
                u64::from(queue_limits.1),
            )
        })
}
fn now() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
}
fn drain(
    running: &mut SpawnedProcess,
    stdout: &mut u64,
    stderr: &mut u64,
    tail: &mut OutputTail,
) -> bool {
    let Ok(_credit) = running.process.queue_credits().acquire(8192, 2) else {
        return false;
    };
    drain_pipe(&mut running.stdout, stdout, None)
        && drain_pipe(&mut running.stderr, stderr, Some(tail))
}
/// Reads what is waiting and discards it, keeping only the end of stderr. The totals are
/// counters, not a ceiling: the proxy copies its log to stderr for as long as it lives, and
/// a ceiling on the volume written since its start stops a healthy proxy once its log has
/// grown that far. False only when the pipe itself fails.
fn drain_pipe(pipe: &mut File, total: &mut u64, mut tail: Option<&mut OutputTail>) -> bool {
    let mut buffer = [0; 16 * 1024];
    for _ in 0..16 {
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        } == 0
        {
            return std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_BROKEN_PIPE as i32);
        }
        if available == 0 {
            return true;
        }
        let count = (available as usize).min(buffer.len());
        let Ok(read) = pipe.read(&mut buffer[..count]) else {
            return false;
        };
        *total = total.saturating_add(read as u64);
        if let Some(tail) = tail.as_deref_mut() {
            tail.push(&buffer[..read]);
        }
    }
    true
}
/// The end of what the proxy wrote to stderr since it started. The proxy copies its own log
/// there, and that is already in its log file; what explains an exit is what the runtime
/// added: a refused start, Node's fatal error.
#[derive(Default)]
struct OutputTail {
    bytes: std::collections::VecDeque<u8>,
    /// Earlier output was dropped: the first line kept may be the end of a longer one.
    cut: bool,
}
impl OutputTail {
    const LIMIT: usize = 8 * 1024;
    const DIAGNOSTIC_CHARS: usize = 300;
    fn clear(&mut self) {
        self.bytes.clear();
        self.cut = false;
    }
    fn push(&mut self, chunk: &[u8]) {
        let chunk = if chunk.len() > Self::LIMIT {
            self.cut = true;
            &chunk[chunk.len() - Self::LIMIT..]
        } else {
            chunk
        };
        let excess = (self.bytes.len() + chunk.len()).saturating_sub(Self::LIMIT);
        if excess > 0 {
            self.bytes.drain(..excess);
            self.cut = true;
        }
        self.bytes.extend(chunk);
    }
    /// One line for the log and the window: the last whole line that words an error, else
    /// the last thing said. Lines of the proxy's own log and Node's warnings are not candidates.
    fn diagnostic(&self) -> Option<String> {
        let bytes: Vec<u8> = self.bytes.iter().copied().collect();
        let text = String::from_utf8_lossy(&bytes);
        let said: Vec<&str> = text
            .lines()
            .skip(usize::from(self.cut))
            .filter(|line| {
                !line.trim().is_empty() && !proxy_log_line(line) && !node_warning_line(line)
            })
            .collect();
        // An indented line continues another one: a stack frame, a field of a printed error.
        let whole = |line: &&&str| !line.starts_with(char::is_whitespace);
        let chosen = said
            .iter()
            .rev()
            .filter(whole)
            .find(|line| {
                ["FATAL ERROR", "Error:", "failed"]
                    .iter()
                    .any(|wording| line.contains(wording))
            })
            .or_else(|| said.iter().rev().find(whole))
            .or_else(|| said.last())?;
        Some(
            chosen
                .trim()
                .chars()
                .filter(|character| !character.is_control())
                .take(Self::DIAGNOSTIC_CHARS)
                .collect(),
        )
    }
}
/// `(node:46300) ExperimentalWarning: ...` and the hint Node prints under it: said at every
/// start, and never why a process ended.
fn node_warning_line(line: &str) -> bool {
    (line.starts_with("(node:") && line.contains("Warning:"))
        || line.starts_with("(Use `node --trace-")
}
/// `2026-11-18T11:25:15.481Z [info] ...`: a line of the proxy's own log.
fn proxy_log_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() > 26
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[10] == b'T'
        && &bytes[23..26] == b"Z ["
}
pub(crate) fn connect(port: NonZeroU16, timeout: Duration) -> Option<TcpStream> {
    TcpStream::connect_timeout(&([127, 0, 0, 1], port.get()).into(), timeout).ok()
}
fn health(port: NonZeroU16, pid: u32, timeout: Duration) -> bool {
    let until = Instant::now() + timeout;
    connect(port, timeout)
        .and_then(|mut stream| request(&mut stream, port, "GET", "/__bili/health", false, until))
        .is_some_and(|(status, body)| {
            status == 200
                && serde_json::from_slice::<Value>(&body)
                    .ok()
                    .is_some_and(|value| {
                        value["ok"] == true && value["pid"].as_u64() == Some(pid.into())
                    })
        })
}
fn graceful_shutdown(port: NonZeroU16, pid: u32, timeout: Duration) {
    let until = Instant::now() + timeout;
    let Some(mut stream) = connect(port, timeout) else {
        return;
    };
    let Some((200, body)) = request(&mut stream, port, "GET", "/__bili/health", true, until) else {
        return;
    };
    let owned = serde_json::from_slice::<Value>(&body)
        .ok()
        .is_some_and(|value| value["ok"] == true && value["pid"].as_u64() == Some(pid.into()));
    if owned {
        let _ = request(&mut stream, port, "POST", "/__bili/shutdown", false, until);
    }
}
pub(crate) fn remaining(until: Instant) -> Option<Duration> {
    until
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
}
fn byte(stream: &mut TcpStream, until: Instant) -> Option<u8> {
    stream.set_read_timeout(Some(remaining(until)?)).ok()?;
    let mut value = [0];
    stream.read_exact(&mut value).ok()?;
    Some(value[0])
}
fn line(stream: &mut TcpStream, until: Instant) -> Option<String> {
    let mut value = Vec::with_capacity(128);
    for _ in 0..512 {
        value.push(byte(stream, until)?);
        if value.ends_with(b"\r\n") {
            value.truncate(value.len() - 2);
            return String::from_utf8(value).ok();
        }
    }
    None
}
fn body(
    stream: &mut TcpStream,
    count: usize,
    until: Instant,
    limit: usize,
    result: &mut Vec<u8>,
) -> Option<()> {
    if result.len().checked_add(count)? > limit {
        return None;
    }
    // Large admin bodies are read in blocks, not byte by byte. The timeout is
    // recomputed before every `read`, so a peer dripping bytes cannot stretch
    // the single deadline: each read is bounded by what is left of `until`.
    let mut left = count;
    let mut block = [0u8; 8192];
    while left > 0 {
        let take = left.min(block.len());
        stream.set_read_timeout(Some(remaining(until)?)).ok()?;
        let read = stream.read(&mut block[..take]).ok()?;
        if read == 0 {
            return None;
        }
        result.extend_from_slice(&block[..read]);
        left -= read;
    }
    Some(())
}
pub(crate) fn request(
    stream: &mut TcpStream,
    port: NonZeroU16,
    method: &str,
    path: &str,
    keep_alive: bool,
    until: Instant,
) -> Option<(u16, Vec<u8>)> {
    request_limited(stream, port, method, path, keep_alive, until, 8192)
}
/// Same exchange as `request`, with the response body capped at `limit` bytes.
pub(crate) fn request_limited(
    stream: &mut TcpStream,
    port: NonZeroU16,
    method: &str,
    path: &str,
    keep_alive: bool,
    until: Instant,
    limit: usize,
) -> Option<(u16, Vec<u8>)> {
    stream.set_write_timeout(Some(remaining(until)?)).ok()?;
    write!(stream,"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: {}\r\nContent-Length: 0\r\n\r\n",port.get(),if keep_alive {"keep-alive"}else{"close"}).ok()?;
    let first = line(stream, until)?;
    let status = first.split_whitespace().nth(1)?.parse().ok()?;
    let mut length = None;
    let mut chunked = false;
    let mut complete = false;
    for _ in 0..32 {
        let next = line(stream, until)?;
        if next.is_empty() {
            complete = true;
            break;
        }
        let (name, value) = next.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return None;
            }
            length = Some(value.trim().parse::<usize>().ok()?);
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            if !value.trim().eq_ignore_ascii_case("chunked") {
                return None;
            }
            chunked = true;
        }
    }
    if !complete || chunked && length.is_some() {
        return None;
    }
    let mut result = Vec::with_capacity(1024);
    if chunked {
        for _ in 0..32 {
            let count = usize::from_str_radix(line(stream, until)?.split(';').next()?, 16).ok()?;
            if count == 0 {
                return if line(stream, until)?.is_empty() {
                    Some((status, result))
                } else {
                    None
                };
            }
            body(stream, count, until, limit, &mut result)?;
            if !line(stream, until)?.is_empty() {
                return None;
            }
        }
        None
    } else {
        body(stream, length?, until, limit, &mut result)?;
        Some((status, result))
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::net::TcpListener;
    pub fn server(
        pid: u32,
        chunked: bool,
        oversized: bool,
    ) -> (NonZeroU16, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
        listener.set_nonblocking(true).unwrap();
        let worker = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(2);
            let mut stream = loop {
                if let Ok((stream, _)) = listener.accept() {
                    break stream;
                }
                assert!(Instant::now() < until, "synthetic HTTP accept deadline");
                thread::sleep(Duration::from_millis(2));
            };
            // Windows accept inherits nonblocking mode. Keep-alive must wait
            // within the finite read deadline for the second request, not treat
            // WouldBlock as EOF immediately after returning the health response.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_write_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let mut requests = Vec::new();
            for _ in 0..2 {
                let Some(first) = line(&mut stream, until) else {
                    break;
                };
                requests.push(first);
                for _ in 0..32 {
                    if line(&mut stream, until).is_none_or(|line| line.is_empty()) {
                        break;
                    }
                }
                if oversized {
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 8193\r\n\r\n").unwrap();
                    break;
                }
                let body = format!("{{\"ok\":true,\"pid\":{pid}}}");
                if chunked {
                    write!(stream,"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",body.len(),body).unwrap();
                } else {
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .unwrap();
                }
            }
            requests
        });
        (port, worker)
    }
    /// Serves any number of connections: `/__bili/health` for `pid`, each
    /// `/__bili/status` from `script` then from `fallback` (None = never answers),
    /// `POST /__bili/shutdown` acknowledged. Every request line is recorded.
    pub struct StatusServer {
        pub port: NonZeroU16,
        requests: Arc<Mutex<Vec<String>>>,
        done: Arc<AtomicBool>,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl StatusServer {
        pub fn start(pid: u32, script: Vec<Option<u64>>, fallback: Option<u64>) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let port = NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
            listener.set_nonblocking(true).unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let done = Arc::new(AtomicBool::new(false));
            let (log, stop) = (requests.clone(), done.clone());
            let worker = thread::spawn(move || {
                let mut script: std::collections::VecDeque<_> = script.into();
                while !stop.load(Ordering::Acquire) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_millis(500)))
                        .unwrap();
                    let until = Instant::now() + Duration::from_secs(5);
                    while let Some(first) = line(&mut stream, until) {
                        log.lock().unwrap().push(first.clone());
                        for _ in 0..32 {
                            if line(&mut stream, until).is_none_or(|line| line.is_empty()) {
                                break;
                            }
                        }
                        let body = if first.contains("/__bili/health") {
                            format!("{{\"ok\":true,\"pid\":{pid}}}")
                        } else if first.contains("/__bili/status") {
                            match script.pop_front().unwrap_or(fallback) {
                                Some(count) => format!("{{\"inFlight\":{count}}}"),
                                None => continue,
                            }
                        } else {
                            "{\"ok\":true}".to_owned()
                        };
                        if write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .is_err()
                        {
                            break;
                        }
                    }
                }
            });
            Self {
                port,
                requests,
                done,
                worker: Some(worker),
            }
        }
        pub fn count(&self, needle: &str) -> usize {
            self.requests
                .lock()
                .unwrap()
                .iter()
                .filter(|line| line.contains(needle))
                .count()
        }
    }
    impl Drop for StatusServer {
        fn drop(&mut self) {
            self.done.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
    /// One keep-alive connection: the first request is answered as `/__bili/health`
    /// for `pid`, each following request with the next of `bodies` verbatim.
    /// Returns the request lines it saw.
    pub fn server_with(
        pid: u32,
        bodies: Vec<String>,
    ) -> (NonZeroU16, thread::JoinHandle<Vec<String>>) {
        server_with_status(pid, bodies.into_iter().map(|body| (200, body)).collect())
    }
    /// `server_with` where each following response carries its own status code.
    pub fn server_with_status(
        pid: u32,
        responses: Vec<(u16, String)>,
    ) -> (NonZeroU16, thread::JoinHandle<Vec<String>>) {
        let bodies = responses;
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
        listener.set_nonblocking(true).unwrap();
        let worker = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(2);
            let mut stream = loop {
                if let Ok((stream, _)) = listener.accept() {
                    break stream;
                }
                assert!(Instant::now() < until, "synthetic HTTP accept deadline");
                thread::sleep(Duration::from_millis(2));
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_write_timeout(Some(Duration::from_millis(500)))
                .unwrap();
            let mut requests = Vec::new();
            let mut bodies = bodies.into_iter();
            for index in 0..=bodies.len() {
                let Some(first) = line(&mut stream, until) else {
                    break;
                };
                requests.push(first);
                for _ in 0..32 {
                    if line(&mut stream, until).is_none_or(|line| line.is_empty()) {
                        break;
                    }
                }
                let (status, body) = if index == 0 {
                    (200, format!("{{\"ok\":true,\"pid\":{pid}}}"))
                } else {
                    bodies.next().unwrap()
                };
                // The client may close early (oversized refusal); that is not a test failure.
                if write!(
                    stream,
                    "HTTP/1.1 {status} Synthetic\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .is_err()
                {
                    break;
                }
            }
            requests
        });
        (port, worker)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{StatusServer, server};
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn maintaining_never_backs_a_launch() {
        assert!(!Phase::Maintaining.backs_launch());
        assert_eq!(
            serde_json::to_string(&Phase::Maintaining).unwrap(),
            "\"maintaining\""
        );
    }

    #[test]
    fn a_start_request_marks_a_running_maintenance_for_cancel() {
        let wanted = StartWanted::default();
        wanted.note(&Command::Activity(Activity::Idle));
        wanted.note(&Command::Maintain {
            budget: None,
            restart: true,
        });
        wanted.note(&Command::Drain);
        assert!(!wanted.is_set());
        wanted.note(&Command::Start);
        assert!(wanted.is_set());
        wanted.clear();
        assert!(!wanted.is_set());
        wanted.note(&Command::Retry);
        assert!(
            wanted.clone().is_set(),
            "the owner's copy sees what the caller's set"
        );
    }

    #[test]
    fn a_heartbeat_keeps_a_maintaining_owner_fresh() {
        let at = now().unwrap();
        let mut snapshot = launch_snapshot(Phase::Maintaining, false, at - 60_000);
        let status = Mutex::new(snapshot.clone());
        heartbeat(&mut snapshot, &status);
        let shown = status.lock().unwrap().clone();
        assert_eq!(shown.phase, Phase::Maintaining);
        assert!(
            shown
                .observed_at_unix_ms
                .is_some_and(|seen| (at - 1_000..=at + 5_000).contains(&seen)),
            "{:?}",
            shown.observed_at_unix_ms
        );
        assert_eq!(snapshot.observed_at_unix_ms, shown.observed_at_unix_ms);
    }

    #[test]
    fn a_stop_or_a_shutdown_ends_a_run_too() {
        assert!(!run_cancelled(false, false, false, false, false));
        assert!(run_cancelled(true, false, false, false, false), "a start");
        assert!(run_cancelled(false, true, false, false, false), "a stop");
        assert!(
            run_cancelled(false, false, true, false, false),
            "the owner's shutdown"
        );
        assert!(
            run_cancelled(false, false, false, true, false),
            "an unconfirmed cleanup"
        );
        assert!(
            run_cancelled(false, false, false, false, true),
            "an agent session that opened"
        );
    }

    #[test]
    fn a_run_restarts_only_a_proxy_that_was_running_or_asked_for() {
        assert!(
            start_after_run(true, true, false, false),
            "the idle run restarts what it stopped"
        );
        assert!(
            !start_after_run(true, false, false, false),
            "a proxy stopped meanwhile stays stopped"
        );
        assert!(
            !start_after_run(false, true, false, false),
            "the close run never restarts"
        );
        assert!(
            start_after_run(false, false, true, false),
            "an open asked for it"
        );
        assert!(
            !start_after_run(true, true, true, true),
            "a pending Stop or shutdown starts nothing"
        );
    }

    #[test]
    fn taking_the_start_flag_clears_it() {
        let wanted = StartWanted::default();
        assert!(!wanted.take());
        wanted.note(&Command::Start);
        assert!(wanted.take());
        assert!(
            !wanted.is_set(),
            "a request dropped from the queue cannot cancel every later run"
        );
        assert!(!wanted.take());
    }

    #[test]
    fn maintenance_limits_are_bounded() {
        let seconds = Duration::from_secs;
        assert!(maintenance_valid(seconds(10), seconds(120)));
        assert!(maintenance_valid(Duration::ZERO, seconds(10)));
        assert!(maintenance_valid(seconds(60), seconds(300)));
        assert!(!maintenance_valid(seconds(61), seconds(120)));
        assert!(!maintenance_valid(seconds(10), seconds(5)));
        assert!(!maintenance_valid(seconds(10), seconds(301)));
    }

    #[test]
    fn recovery_keeps_retrying_with_a_bounded_delay_and_resets_only_after_stable_health() {
        let mut recovery = RecoveryBackoff::default();
        let mut now = Instant::now();
        for delay in [1, 2, 4, 8, 16, 30, 30, 30] {
            recovery.schedule(now);
            assert!(!recovery.ready(now + Duration::from_secs(delay) - Duration::from_millis(1)));
            now += Duration::from_secs(delay);
            assert!(recovery.ready(now));
            recovery.healthy(now); // Briefly responding must not reset a crash loop.
        }
        recovery.healthy(now + Duration::from_secs(59));
        recovery.unhealthy();
        recovery.healthy(now + Duration::from_secs(60));
        recovery.healthy(now + Duration::from_secs(119));
        assert_ne!(recovery.failures, 0);
        recovery.healthy(now + Duration::from_secs(120));
        recovery.schedule(now + Duration::from_secs(121));
        assert!(recovery.ready(now + Duration::from_secs(122)));
    }

    #[test]
    fn a_panic_payload_becomes_one_bounded_line() {
        assert_eq!(panic_text(&"boom"), "boom");
        assert_eq!(panic_text(&String::from("disk\nfull\u{7}")), "disk full");
        assert_eq!(panic_text(&42_u8), "unknown panic payload");
        assert_eq!(panic_text(&"x".repeat(900)).chars().count(), 300);
    }
    #[test]
    fn a_fault_is_a_failed_snapshot_that_says_why() {
        let mut snapshot = launch_snapshot(Phase::Recovering, true, 1);
        fault_snapshot(&mut snapshot, "boom".into());
        assert_eq!(
            (snapshot.phase, snapshot.reason, snapshot.healthy),
            (Phase::Failed, Reason::SupervisorFault, false)
        );
        assert_eq!(snapshot.fault.as_deref(), Some("boom"));
        assert_eq!(
            serde_json::to_value(Reason::SupervisorFault).unwrap(),
            "supervisor_fault"
        );
    }

    fn launch_snapshot(phase: Phase, healthy: bool, now: i64) -> Snapshot {
        Snapshot {
            phase,
            reason: Reason::None,
            activity: Activity::Idle,
            process_alive: Some(true),
            healthy,
            restart_used: false,
            stdout_bytes: 0,
            stderr_bytes: 0,
            observed_at_unix_ms: Some(now - 100),
            pid: Some(7),
            in_flight: Some(1),
            drain_deadline_unix_ms: None,
            resource_limit: None,
            port_held: None,
            health_probe_delay_ms: 250,
            last_exit: None,
            fault: None,
        }
    }
    #[test]
    fn busy_backs_a_launch_when_the_proxy_is_healthy_and_fresh() {
        let now = 1_000_000;
        assert!(launch_snapshot(Phase::Ready, true, now).backs_launch(now));
        // Another agent's response is streaming: the proxy is healthy and Busy.
        assert!(launch_snapshot(Phase::Busy, true, now).backs_launch(now));
        // Busy from a missed health response is not healthy, so it never backs a launch.
        assert!(!launch_snapshot(Phase::Busy, false, now).backs_launch(now));
        for phase in [
            Phase::Stopped,
            Phase::Starting,
            Phase::Draining,
            Phase::Degraded,
            Phase::Recovering,
            Phase::Failed,
            Phase::Retired,
        ] {
            assert!(!launch_snapshot(phase, true, now).backs_launch(now));
        }
        let mut stale = launch_snapshot(Phase::Busy, true, now);
        stale.observed_at_unix_ms = Some(now - 10_001);
        assert!(!stale.backs_launch(now));
        let mut dead = launch_snapshot(Phase::Busy, true, now);
        dead.process_alive = Some(false);
        assert!(!dead.backs_launch(now));
    }
    #[test]
    fn live_owner_with_occupied_port_stays_busy_and_probe_backoff_caps_and_recovers() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
        let mut policy = BusyBackoff::default();
        let normal = Duration::from_millis(250);
        // This live-owner transition has no process/admission/restart capability.
        // It observes a real occupied loopback port rather than assuming connect.
        for seconds in [1, 2, 4, 8, 8, 8] {
            let port_state = probe_port(port);
            assert_eq!(port_state, PortState::Occupied);
            assert_eq!(
                policy.missed_while_alive(port_state, normal),
                (
                    Phase::Busy,
                    Reason::HealthUnavailable,
                    Duration::from_secs(seconds)
                )
            );
        }
        drop(listener);
        assert_eq!(probe_port(port), PortState::Free);
        assert_eq!(
            policy.missed_while_alive(PortState::Free, normal),
            (Phase::Degraded, Reason::ListenerUnavailable, normal)
        );
        assert_eq!(
            policy.missed_while_alive(PortState::Occupied, normal).2,
            Duration::from_secs(8)
        );
        policy.healthy();
        assert_eq!(
            policy.missed_while_alive(PortState::Occupied, normal).2,
            Duration::from_secs(1)
        );
    }
    #[test]
    fn owned_budget_guard_uses_raw_counts_and_current_commit_and_queue_usage() {
        use crate::resources::sampling::{ProcessCounters, ProcessKey, ProcessSample};
        let applied = JobBudget::new(100, 60, 8, 2000).unwrap();
        let frame = |count: u32, per_process: u64| JobSnapshot {
            active_processes: count,
            total_processes: count,
            total_terminated_processes: 0,
            cpu_time_100ns: 0,
            peak_job_commit_bytes: 1_000_000,
            observed_private_commit_bytes: u64::from(count) * per_process,
            observed_resident_bytes: 1_000_000,
            membership_complete: true,
            members: vec![],
            processes: (1..=count)
                .map(|pid| ProcessSample {
                    key: ProcessKey {
                        pid,
                        created_at_100ns: 1,
                    },
                    counters: Measurement::Available(ProcessCounters {
                        private_commit_bytes: per_process,
                        resident_bytes: 1000,
                        cpu_time_100ns: 0,
                    }),
                })
                .collect(),
        };
        let mut queue = QueueStatus {
            closed: false,
            current_bytes: 0,
            current_items: 0,
            peak_bytes: 1_000_000,
            peak_items: 1000,
            refusals: 0,
        };
        // Native quota8 can coexist with raw13 console processes on Windows.
        // Neither subtract helpers nor classify a historical/RSS peak as commit.
        assert_eq!(
            resource_limit(&frame(13, 1), applied, queue, (9, 4)),
            Some(ResourceLimit {
                kind: ResourceLimitKind::Processes,
                observed: 13,
                applied: 8
            })
        );
        assert_eq!(resource_limit(&frame(8, 8), applied, queue, (9, 4)), None);
        assert_eq!(
            resource_limit(&frame(2, 51), applied, queue, (9, 4))
                .unwrap()
                .kind,
            ResourceLimitKind::JobCommit
        );
        assert_eq!(
            resource_limit(&frame(1, 61), applied, queue, (9, 4))
                .unwrap()
                .kind,
            ResourceLimitKind::ProcessCommit
        );
        queue.current_bytes = 10;
        assert_eq!(
            resource_limit(&frame(1, 1), applied, queue, (9, 4))
                .unwrap()
                .kind,
            ResourceLimitKind::QueueBytes
        );
        queue.current_bytes = 9;
        queue.current_items = 5;
        assert_eq!(
            resource_limit(&frame(1, 1), applied, queue, (9, 4))
                .unwrap()
                .kind,
            ResourceLimitKind::QueueItems
        );
    }
    /// Observed on 2026-11-18: a 768 MiB Windows quota on the proxy's process, which Node
    /// reached in minutes with six sessions, ended every session's connection at once.
    #[test]
    fn an_advisory_budget_never_stops_the_proxy_for_memory_or_process_count() {
        use crate::resources::sampling::{ProcessCounters, ProcessKey, ProcessSample};
        let advisory = JobBudget::new(100, 60, 8, 2000)
            .unwrap()
            .with_advisory_limits();
        let sample = JobSnapshot {
            active_processes: 13,
            total_processes: 13,
            total_terminated_processes: 0,
            cpu_time_100ns: 0,
            peak_job_commit_bytes: 1_000_000,
            observed_private_commit_bytes: 1_000_000,
            observed_resident_bytes: 1_000_000,
            membership_complete: true,
            members: vec![],
            processes: vec![ProcessSample {
                key: ProcessKey {
                    pid: 1,
                    created_at_100ns: 1,
                },
                counters: Measurement::Available(ProcessCounters {
                    private_commit_bytes: 1_000_000,
                    resident_bytes: 1000,
                    cpu_time_100ns: 0,
                }),
            }],
        };
        let mut queue = QueueStatus {
            closed: false,
            current_bytes: 0,
            current_items: 0,
            peak_bytes: 0,
            peak_items: 0,
            refusals: 0,
        };
        assert_eq!(resource_limit(&sample, advisory, queue, (9, 4)), None);
        // The supervisor's own reader queue is still bounded.
        queue.current_bytes = 10;
        assert_eq!(
            resource_limit(&sample, advisory, queue, (9, 4))
                .unwrap()
                .kind,
            ResourceLimitKind::QueueBytes
        );
    }
    /// The proxy copies its log to stderr: a long-lived proxy writes without end, and the
    /// volume it has written since it started is no reason to stop it.
    #[test]
    fn output_is_read_without_a_lifetime_ceiling_and_its_last_lines_are_kept() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        let mut pipe = File::from(std::os::windows::io::OwnedHandle::from(reader));
        let line = b"2026-11-18T11:16:31.083Z [debug] [v=0.1.207] [conn#4884] tcp closed\n";
        let lines = 3000;
        let feeder = thread::spawn(move || {
            for _ in 0..lines {
                writer.write_all(line).unwrap();
            }
            writer
                .write_all(b"bili: failed to start: Error: EBUSY: resource busy or locked\n")
                .unwrap();
        });
        let (mut total, mut tail) = (0, OutputTail::default());
        let expected = (lines * line.len() + 61) as u64;
        let until = Instant::now() + Duration::from_secs(20);
        while total < expected && Instant::now() < until {
            assert!(drain_pipe(&mut pipe, &mut total, Some(&mut tail)));
        }
        feeder.join().unwrap();
        assert_eq!(total, expected);
        // A closed pipe is the child's exit, not a failure of the reader.
        assert!(drain_pipe(&mut pipe, &mut total, Some(&mut tail)));
        assert!(tail.bytes.len() <= OutputTail::LIMIT);
        assert_eq!(
            tail.diagnostic().as_deref(),
            Some("bili: failed to start: Error: EBUSY: resource busy or locked")
        );
    }
    #[test]
    fn the_exit_diagnostic_is_what_the_runtime_said_not_the_proxys_own_log() {
        let said = |text: &str| {
            let mut tail = OutputTail::default();
            tail.push(text.as_bytes());
            tail.diagnostic()
        };
        // The refused start of 2026-11-18, as Node wrote it.
        assert_eq!(
            said(concat!(
                "2026-11-18T11:25:15.481Z [info] [v=0.1.207] [persist] enabled\n",
                "(node:46300) ExperimentalWarning: SQLite is an experimental feature\n",
                "2026-11-18T11:25:15.509Z [warn] [v=0.1.207] os-store merge skipped (win32): Error: spawnSync powershell ENOENT\n",
                "bili: failed to start: Error: EBUSY: resource busy or locked, open 'D:\\data\\ca\\combined-ca.pem'\n",
                "    at Object.openSync (node:fs:560:18)\n",
                "    at Object.writeFileSync (node:fs:2427:35) {\n",
                "  errno: -4082,\n  code: 'EBUSY',\n  syscall: 'open',\n}\n",
            ))
            .as_deref(),
            Some(
                "bili: failed to start: Error: EBUSY: resource busy or locked, open 'D:\\data\\ca\\combined-ca.pem'"
            )
        );
        // Node out of memory.
        assert_eq!(
            said(concat!(
                "<--- Last few GCs --->\n\n",
                "[4242:0000] 91234 ms: Mark-Compact 760.1 (771.2) -> 759.8 (771.2) MB\n\n",
                "FATAL ERROR: Reached heap limit Allocation failed - JavaScript heap out of memory\n",
                "----- Native stack trace -----\n 1: 00007FF6 node::SetCppgcReference\n",
            ))
            .as_deref(),
            Some("FATAL ERROR: Reached heap limit Allocation failed - JavaScript heap out of memory")
        );
        // Nothing but the proxy's own log lines: they are in its log file already.
        assert_eq!(
            said("2026-11-18T11:16:31.083Z [debug] [v=0.1.207] [conn#4884] tcp closed\n"),
            None
        );
        // Killed from outside (2026-11-18, test copy): Node had only warned at its start.
        // A warning is not why the process ended.
        assert_eq!(
            said(concat!(
                "(node:10932) ExperimentalWarning: SQLite is an experimental feature and might change at any time\n",
                "(Use `node --trace-warnings ...` to show where the warning was created)\n",
                "2026-11-18T12:02:41.227Z [info] [v=0.1.207] acp-proxy v0.1.207 listening\n",
            )),
            None
        );
        // No error wording: the last thing the runtime said. Control characters never pass.
        assert_eq!(
            said("Node.js v24.14.1\r\nabnormal\x1b[31m end\n").as_deref(),
            Some("abnormal[31m end")
        );
        assert_eq!(
            said(&"x".repeat(4000)).map(|text| text.chars().count()),
            Some(300)
        );
        // Only the end of a long output is kept, and a cut line is not mistaken for a whole one.
        let mut tail = OutputTail::default();
        tail.push("Error: early\n".as_bytes());
        tail.push(&vec![b'a'; OutputTail::LIMIT]);
        tail.push(b"\nError: late\n");
        assert_eq!(tail.diagnostic().as_deref(), Some("Error: late"));
    }
    #[test]
    fn slow_drip_body_cannot_outlive_the_deadline() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let stop = Instant::now() + Duration::from_secs(3);
            while line(&mut stream, stop).is_some_and(|line| !line.is_empty()) {}
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 4000\r\n\r\n").unwrap();
            // One byte every 40 ms: each byte arrives well inside any single
            // read timeout, but the whole body can never fit the deadline.
            while Instant::now() < stop {
                if stream.write_all(b"x").is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(40));
            }
        });
        let started = Instant::now();
        let until = started + Duration::from_millis(300);
        let mut stream = connect(port, Duration::from_millis(300)).unwrap();
        assert_eq!(
            request_limited(&mut stream, port, "GET", "/__bili/x", false, until, 8192),
            None
        );
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(900),
            "deadline overrun: {elapsed:?}"
        );
        drop(stream);
        worker.join().unwrap();
    }
    #[test]
    fn graceful_shutdown_uses_same_identity_verified_connection() {
        let (port, worker) = server(7, true, false);
        graceful_shutdown(port, 7, Duration::from_millis(500));
        assert_eq!(
            worker.join().unwrap(),
            [
                "GET /__bili/health HTTP/1.1",
                "POST /__bili/shutdown HTTP/1.1"
            ]
        );
        let (port, worker) = server(8, false, false);
        graceful_shutdown(port, 7, Duration::from_millis(500));
        assert_eq!(worker.join().unwrap(), ["GET /__bili/health HTTP/1.1"]);
    }
    #[test]
    fn health_refuses_other_identity_and_oversized_response() {
        for (pid, oversized) in [(7, false), (8, false), (7, true)] {
            let (port, worker) = server(pid, false, oversized);
            assert_eq!(
                health(port, 7, Duration::from_millis(500)),
                pid == 7 && !oversized
            );
            assert_eq!(worker.join().unwrap().len(), 1);
        }
    }
    /// Drives the owner's own drain pieces (state, poll, shutdown) the way the
    /// worker does, one poll per `poll`, until the drain ends.
    fn run_drain(
        server: &StatusServer,
        pid: u32,
        timeout: Duration,
        poll: Duration,
    ) -> Vec<DrainStep> {
        let target = AdminTarget {
            port: server.port,
            pid,
            timeout: Duration::from_millis(100),
        };
        let mut drain = DrainState::default();
        let began = Instant::now();
        assert_eq!(
            drain.command(Phase::Ready, Command::Drain, began, timeout),
            DrainEffect::Begin
        );
        let mut steps = Vec::new();
        loop {
            let (_, step) = drain
                .poll(&target)
                .expect("drain stays active until it ends");
            steps.push(step);
            if step == DrainStep::Waiting {
                assert!(drain.active());
                thread::sleep(poll);
                continue;
            }
            // What the owner does next: one identity-checked graceful shutdown.
            assert!(!drain.active());
            graceful_shutdown(server.port, pid, target.timeout);
            return steps;
        }
    }
    #[test]
    fn drain_waits_for_in_flight_then_stops() {
        let server = StatusServer::start(41, vec![Some(2), Some(1), Some(0), Some(0)], Some(0));
        let steps = run_drain(
            &server,
            41,
            Duration::from_secs(5),
            Duration::from_millis(10),
        );
        assert_eq!(
            steps,
            [
                DrainStep::Waiting,
                DrainStep::Waiting,
                DrainStep::Waiting,
                DrainStep::Idle
            ]
        );
        assert_eq!(steps[3].reason(), Reason::None);
        assert_eq!(server.count("GET /__bili/status"), 4);
        assert_eq!(server.count("POST /__bili/shutdown"), 1);
    }
    #[test]
    fn a_non_zero_or_unknown_read_resets_the_idle_count() {
        let mut watch = DrainWatch::begin(Instant::now(), Duration::from_secs(60));
        let now = Instant::now();
        for (reading, expected) in [
            (Some(0), DrainStep::Waiting),
            (None, DrainStep::Waiting),
            (Some(0), DrainStep::Waiting),
            (Some(3), DrainStep::Waiting),
            (Some(0), DrainStep::Waiting),
            (Some(0), DrainStep::Idle),
        ] {
            assert_eq!(watch.observe(now, reading), expected);
        }
    }
    #[test]
    fn unknown_in_flight_is_not_idle() {
        let server = StatusServer::start(42, vec![], None);
        let timeout = Duration::from_millis(400);
        let began = Instant::now();
        let steps = run_drain(&server, 42, timeout, Duration::from_millis(10));
        assert!(began.elapsed() >= timeout);
        assert_eq!(steps.last(), Some(&DrainStep::TimedOut));
        assert!(
            steps[..steps.len() - 1]
                .iter()
                .all(|step| *step == DrainStep::Waiting)
        );
        assert!(steps.len() > 1, "unknown reads must keep the drain waiting");
        assert_eq!(steps.last().unwrap().reason(), Reason::DrainTimedOut);
        assert!(server.count("GET /__bili/status") >= 2);
        assert_eq!(server.count("POST /__bili/shutdown"), 1);
    }
    #[test]
    fn start_during_drain_cancels_it() {
        let server = StatusServer::start(43, vec![], Some(3));
        let target = AdminTarget {
            port: server.port,
            pid: 43,
            timeout: Duration::from_millis(100),
        };
        let mut drain = DrainState::default();
        let now = Instant::now();
        let timeout = Duration::from_secs(30);
        assert_eq!(
            drain.command(Phase::Busy, Command::Drain, now, timeout),
            DrainEffect::Begin
        );
        for _ in 0..2 {
            assert_eq!(drain.poll(&target).unwrap().1, DrainStep::Waiting);
        }
        assert_eq!(
            drain.command(Phase::Draining, Command::Start, now, timeout),
            DrainEffect::Cancel
        );
        assert!(!drain.active());
        let reads = server.count("GET /__bili/status");
        assert_eq!(reads, 2);
        assert!(
            drain.poll(&target).is_none(),
            "a cancelled drain never polls or stops"
        );
        assert_eq!(server.count("GET /__bili/status"), reads);
        assert_eq!(server.count("POST /__bili/shutdown"), 0);
        // Start with no drain in progress is left to the owner's normal handling.
        assert_eq!(
            drain.command(Phase::Ready, Command::Start, now, timeout),
            DrainEffect::Unaffected
        );
    }
    #[test]
    fn drain_in_any_other_phase_is_a_stop() {
        let mut drain = DrainState::default();
        let now = Instant::now();
        for phase in [
            Phase::Stopped,
            Phase::Starting,
            Phase::Degraded,
            Phase::Recovering,
            Phase::Failed,
            Phase::Retired,
        ] {
            assert_eq!(
                drain.command(phase, Command::Drain, now, Duration::from_secs(30)),
                DrainEffect::StopNow
            );
            assert!(!drain.active());
        }
    }
    #[test]
    fn stop_is_still_immediate() {
        let server = StatusServer::start(44, vec![], Some(5));
        let target = AdminTarget {
            port: server.port,
            pid: 44,
            timeout: Duration::from_millis(100),
        };
        let mut drain = DrainState::default();
        let began = Instant::now();
        drain.command(Phase::Busy, Command::Drain, began, Duration::from_secs(30));
        assert_eq!(drain.poll(&target).unwrap(), (Some(5), DrainStep::Waiting));
        // Stop ends any drain at once and goes straight to the shutdown request.
        drain.clear();
        assert!(!drain.active());
        graceful_shutdown(server.port, 44, target.timeout);
        assert!(began.elapsed() < Duration::from_secs(5));
        assert_eq!(server.count("POST /__bili/shutdown"), 1);
        assert_eq!(server.count("GET /__bili/status"), 1);
    }
}
