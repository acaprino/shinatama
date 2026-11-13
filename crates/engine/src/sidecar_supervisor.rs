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
    RepeatedExit,
    OutputLimit,
    ObservationUnavailable,
    CleanupUncertain,
    ResourceLimit,
    ListenerUnavailable,
    PortProbeUnavailable,
    DrainTimedOut,
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    Full,
    Closed,
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
    pub output_bytes_per_stream: u64,
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
            && self.reservation_lease <= Duration::from_secs(300)
            && (4096..=64 * 1024 * 1024).contains(&self.output_bytes_per_stream)
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
        };
        let status = Arc::new(Mutex::new(snapshot.clone()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop_requested = Arc::new(AtomicBool::new(false));
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
        })
    }
    pub fn request(&self, command: Command) -> Result<(), RequestError> {
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
}
impl<P: WorkloadProvider> Worker<P> {
    fn publish(&mut self, phase: Phase, reason: Reason) {
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
    fn run(mut self) {
        loop {
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
                return;
            }
            match self.receiver.recv_timeout(self.config.poll_period) {
                Ok(Command::Stop) => {
                    self.requested_start = false;
                    self.stop(true);
                }
                Ok(Command::Drain) => self.drain_requested(),
                Ok(Command::Start) => {
                    let effect = self.drain.command(
                        self.snapshot.phase,
                        Command::Start,
                        Instant::now(),
                        self.config.drain_timeout,
                    );
                    if effect == DrainEffect::Cancel {
                        self.publish(Phase::Ready, Reason::None);
                    } else if self.snapshot.phase == Phase::Stopped {
                        self.requested_start = true;
                    }
                }
                Ok(Command::Retry) => {
                    if self.retained.is_some() {
                        self.retry_cleanup();
                    } else if self.running.is_none() && self.snapshot.phase == Phase::Failed {
                        self.snapshot.restart_used = false;
                        self.requested_start = true;
                    } else {
                        self.next_health = Instant::now();
                    }
                }
                Ok(Command::Activity(activity)) => self.snapshot.activity = activity,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.shutdown.store(true, Ordering::Release)
                }
                _ => {}
            }
            if self.stop_requested.swap(false, Ordering::AcqRel) {
                self.requested_start = false;
                for _ in 0..4 {
                    let _ = self.receiver.try_recv();
                }
                self.stop(true);
                continue;
            }
            if self.requested_start && self.running.is_none() && self.retained.is_none() {
                self.requested_start = false;
                self.launch();
            }
            self.tick();
            self.drain_step();
        }
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
            self.publish(Phase::Failed, Reason::ObservationUnavailable);
            return;
        };
        let until = at.saturating_add(self.config.reservation_lease.as_millis() as i64);
        let Ok(workload) = self.provider.prepare(at, until) else {
            self.publish(Phase::Failed, Reason::AdmissionDeferred);
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
        match spawn_contained(command, workload, self.provider.database(), at, until) {
            Ok(running) => {
                self.running = Some(running);
                self.started = Instant::now();
                self.next_health = self.started;
                self.next_renew = self.started + self.config.reservation_lease / 3;
                self.lease_valid = true;
                self.snapshot.stdout_bytes = 0;
                self.snapshot.stderr_bytes = 0;
                self.snapshot.process_alive = Some(true);
                self.snapshot.pid = self.running.as_ref().map(|running| running.process.key().pid);
            }
            Err(mut failure) => {
                if !matches!(
                    failure
                        .workload
                        .finish_cleanup(self.provider.database(), self.config.cleanup_timeout),
                    Ok(CleanupStatus::Released { .. })
                ) {
                    self.retained = Some(Retained::Reservation(failure.workload));
                    self.publish(Phase::Retired, Reason::CleanupUncertain);
                } else {
                    self.publish(Phase::Failed, Reason::StartupRefused);
                }
            }
        }
    }
    fn tick(&mut self) {
        let Some(running) = &mut self.running else {
            return;
        };
        let observed = running.process.try_wait();
        if matches!(observed, Ok(Some(_))) {
            // A process that exits while draining has done what the drain asked for.
            let draining = self.drain.active();
            self.drain.clear();
            self.snapshot.process_alive = Some(false);
            self.snapshot.healthy = false;
            self.publish(Phase::Recovering, Reason::ProcessExited);
            self.stop(false);
            if self.retained.is_none() && !draining {
                if self.snapshot.restart_used {
                    self.publish(Phase::Failed, Reason::RepeatedExit);
                } else {
                    self.snapshot.restart_used = true;
                    self.requested_start = true;
                    self.publish(Phase::Recovering, Reason::ProcessExited);
                }
            }
            return;
        }
        if observed.is_err() {
            self.snapshot.process_alive = None;
            self.snapshot.healthy = false;
            self.publish(Phase::Degraded, Reason::ObservationUnavailable);
            return;
        }
        self.snapshot.process_alive = Some(true);
        let Some(applied) = running.process.applied_budget() else {
            self.snapshot.healthy = false;
            self.publish(Phase::Degraded, Reason::ObservationUnavailable);
            return;
        };
        let Ok(sample) = running.process.sample(&WindowsSampler::new()) else {
            self.snapshot.healthy = false;
            self.publish(Phase::Degraded, Reason::ObservationUnavailable);
            return;
        };
        if !sample.membership_complete {
            self.snapshot.healthy = false;
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
            self.publish(
                if self.retained.is_some() {
                    Phase::Retired
                } else {
                    Phase::Failed
                },
                Reason::ResourceLimit,
            );
            return; // Explicit Retry only: never replay agent work or raise caps.
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
            self.publish(Phase::Degraded, Reason::LeaseUnavailable);
            return;
        }
        if !drain(
            running,
            &mut self.snapshot.stdout_bytes,
            &mut self.snapshot.stderr_bytes,
            self.config.output_bytes_per_stream,
        ) {
            self.snapshot.healthy = false;
            self.stop(false);
            if self.retained.is_none() {
                self.publish(Phase::Failed, Reason::OutputLimit);
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
                    self.config.output_bytes_per_stream,
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
fn drain(running: &mut SpawnedProcess, stdout: &mut u64, stderr: &mut u64, limit: u64) -> bool {
    let Ok(_credit) = running.process.queue_credits().acquire(8192, 2) else {
        return false;
    };
    drain_pipe(&mut running.stdout, stdout, limit) && drain_pipe(&mut running.stderr, stderr, limit)
}
fn drain_pipe(pipe: &mut File, total: &mut u64, limit: u64) -> bool {
    let mut buffer = [0; 4096];
    for _ in 0..4 {
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
        if *total > limit {
            return false;
        }
    }
    true
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
