//! One process-wide, bounded observer. No admission, workload attribution or kill
//! policy lives here. Toolhelp ancestry is not a host/renderer baseline: owned
//! descendants can include future workloads until a job registry classifies them.
#![cfg(windows)]

use super::sampling::{
    Aggregate, IntervalUnavailable, MAX_PROCESSES, Measurement, ProcessKey, ProcessSample,
    SamplingError, SamplingWindow, Unavailable, WindowsSampler,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, TryLockError};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, GetLastError,
    HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};

pub const MAX_ENUMERATED_PROCESSES: usize = 1024;
pub const MIN_UPDATE_PERIOD: Duration = Duration::from_millis(250);
pub const MAX_UPDATE_PERIOD: Duration = Duration::from_secs(30);
const DISCOVERY_BUDGET: Duration = Duration::from_millis(100);
static WORKER_OCCUPIED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorConfig {
    /// Minimum quiet interval after a cycle; slow reads never trigger catch-up work.
    pub update_period: Duration,
}
impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            update_period: Duration::from_secs(1),
        }
    }
}

/// Caller-observed counters only. None means not instrumented/available, including
/// before the first report; the monitor never invents latency or queue samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CallerTelemetry {
    pub input_latency_us: Option<u64>,
    pub frame_time_us: Option<u64>,
    pub ipc_roundtrip_us: Option<u64>,
    pub pending_input_events: Option<u32>,
    pub pending_output_bytes: Option<u64>,
    pub queued_launches: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallerReport {
    pub observed_at_ms: u64,
    pub telemetry: CallerTelemetry,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiscoveryCoverage {
    pub enumerated_processes: usize,
    pub snapshot_complete: bool,
    pub owned_limit_reached: bool,
    pub deadline_reached: bool,
    pub unavailable_candidates: usize,
    pub rejected_ancestry: usize,
    pub first_failure: Option<Unavailable>,
}
impl DiscoveryCoverage {
    pub fn complete(&self) -> bool {
        self.snapshot_complete
            && !self.owned_limit_reached
            && !self.deadline_reached
            && self.unavailable_candidates == 0
            && self.rejected_ancestry == 0
    }
    fn unavailable(&mut self, reason: Unavailable) {
        self.unavailable_candidates += 1;
        self.first_failure.get_or_insert(reason);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorFailure {
    HostUnavailable(Unavailable),
    Sampling(SamplingError),
}
#[derive(Clone, Debug)]
pub struct MonitorFrame {
    pub host: Option<ProcessKey>,
    /// Same monotonic epoch as CallerReport and MonitorStatus.
    pub completed_at_ms: u64,
    /// Entire cycle, including discovery; aggregate timing covers counter reads.
    pub cycle_elapsed_us: u64,
    pub discovery: DiscoveryCoverage,
    /// Confirmed owned membership only, including host; <= MAX_PROCESSES.
    /// Roles are unknown except the exact host key. No process names are retained.
    pub owned_processes: Vec<ProcessSample>,
    pub aggregate: Option<Aggregate>,
    pub caller: Option<CallerReport>,
    pub failure: Option<MonitorFailure>,
}
impl MonitorFrame {
    pub fn complete(&self) -> bool {
        self.failure.is_none()
            && self.discovery.complete()
            && self.aggregate.as_ref().is_some_and(|value| {
                value.process_samples_complete
                    && matches!(value.system, Measurement::Available(system)
                        if matches!(system.observed_group_cpu, Measurement::Available(_)))
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WorkerPhase {
    Starting,
    Sampling,
    Waiting,
    Stopping,
    Stopped,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorStatus {
    pub phase: WorkerPhase,
    pub completed_cycles: u64,
    pub in_flight_for_ms: Option<u64>,
    pub last_completed_ago_ms: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    InvalidPeriod,
    AlreadyRunning,
    ThreadUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    Busy,
    Poisoned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportError {
    Busy,
    Stopped,
}

struct Shared {
    epoch: Instant,
    stop: AtomicBool,
    phase: AtomicU8,
    cycle_started_ms: AtomicU64,
    last_completed_ms: AtomicU64,
    completed_cycles: AtomicU64,
    latest: Mutex<Option<Arc<MonitorFrame>>>,
}
impl Shared {
    fn now_ms(&self) -> u64 {
        millis(self.epoch.elapsed())
    }
    fn phase(&self, phase: WorkerPhase) {
        self.phase.store(phase as u8, Ordering::Release);
    }
    fn publish(&self, frame: MonitorFrame) {
        // Native reads and allocation of the frame precede the brief publication lock.
        let frame = Arc::new(frame);
        let previous = self
            .latest
            .lock()
            .ok()
            .and_then(|mut latest| latest.replace(frame));
        drop(previous);
        self.last_completed_ms
            .store(self.now_ms(), Ordering::Release);
        self.completed_cycles.fetch_add(1, Ordering::Release);
    }
}

/// Drop signals shutdown and wakes the one worker; it never joins or waits for a
/// native read. A stuck worker retains the global slot, preventing replacement
/// threads or queues from accumulating. Current/status never wait on that worker.
pub struct ResourceMonitor {
    shared: Arc<Shared>,
    reports: SyncSender<CallerReport>,
    worker: Thread,
}
impl ResourceMonitor {
    pub fn start(config: MonitorConfig) -> Result<Self, StartError> {
        if !(MIN_UPDATE_PERIOD..=MAX_UPDATE_PERIOD).contains(&config.update_period) {
            return Err(StartError::InvalidPeriod);
        }
        WORKER_OCCUPIED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| StartError::AlreadyRunning)?;
        let shared = Arc::new(Shared {
            epoch: Instant::now(),
            stop: AtomicBool::new(false),
            phase: AtomicU8::new(WorkerPhase::Starting as u8),
            cycle_started_ms: AtomicU64::new(0),
            last_completed_ms: AtomicU64::new(0),
            completed_cycles: AtomicU64::new(0),
            latest: Mutex::new(None),
        });
        let (reports, receiver) = sync_channel(1);
        let state = Arc::clone(&shared);
        let worker = match thread::Builder::new()
            .name("shinatama-resources".into())
            .spawn(move || {
                let _slot = WorkerSlot;
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&state, receiver, config)
                }))
                .is_err()
                {
                    state.phase(WorkerPhase::Failed);
                }
            }) {
            Ok(worker) => worker,
            Err(_) => {
                WORKER_OCCUPIED.store(false, Ordering::Release);
                return Err(StartError::ThreadUnavailable);
            }
        };
        let worker = worker.thread().clone(); // JoinHandle detaches here; shutdown is nonblocking.
        Ok(Self {
            shared,
            reports,
            worker,
        })
    }

    /// A single latest immutable frame; consumers must also bound their own retention.
    pub fn current(&self) -> Result<Option<Arc<MonitorFrame>>, ReadError> {
        match self.shared.latest.try_lock() {
            Ok(latest) => Ok(latest.as_ref().map(Arc::clone)),
            Err(TryLockError::WouldBlock) => Err(ReadError::Busy),
            Err(TryLockError::Poisoned(_)) => Err(ReadError::Poisoned),
        }
    }

    pub fn status(&self) -> MonitorStatus {
        let phase = match self.shared.phase.load(Ordering::Acquire) {
            0 => WorkerPhase::Starting,
            1 => WorkerPhase::Sampling,
            2 => WorkerPhase::Waiting,
            3 => WorkerPhase::Stopping,
            4 => WorkerPhase::Stopped,
            _ => WorkerPhase::Failed,
        };
        let completed_cycles = self.shared.completed_cycles.load(Ordering::Acquire);
        let now = self.shared.now_ms();
        MonitorStatus {
            phase,
            completed_cycles,
            in_flight_for_ms: (phase == WorkerPhase::Sampling)
                .then(|| now.saturating_sub(self.shared.cycle_started_ms.load(Ordering::Acquire))),
            last_completed_ago_ms: (completed_cycles > 0)
                .then(|| now.saturating_sub(self.shared.last_completed_ms.load(Ordering::Acquire))),
        }
    }

    pub fn report(&self, telemetry: CallerTelemetry) -> Result<(), ReportError> {
        if self.shared.stop.load(Ordering::Acquire) {
            return Err(ReportError::Stopped);
        }
        let report = CallerReport {
            observed_at_ms: self.shared.now_ms(),
            telemetry,
        };
        self.reports.try_send(report).map_err(|error| match error {
            TrySendError::Full(_) => ReportError::Busy,
            TrySendError::Disconnected(_) => ReportError::Stopped,
        })
    }
}
impl Drop for ResourceMonitor {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        // Preserve terminal failure/stop status; no lock or native wait is needed.
        let _ = self
            .shared
            .phase
            .try_update(Ordering::AcqRel, Ordering::Acquire, |phase| {
                (phase < WorkerPhase::Stopping as u8).then_some(WorkerPhase::Stopping as u8)
            });
        self.worker.unpark();
    }
}
struct WorkerSlot;
impl Drop for WorkerSlot {
    fn drop(&mut self) {
        WORKER_OCCUPIED.store(false, Ordering::Release);
    }
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}
fn new_window() -> SamplingWindow {
    SamplingWindow::new(120, 60_000).expect("fixed bounded retention")
}

fn run(shared: &Shared, receiver: Receiver<CallerReport>, config: MonitorConfig) {
    shared
        .cycle_started_ms
        .store(shared.now_ms(), Ordering::Release);
    shared.phase(WorkerPhase::Sampling);
    let started = Instant::now();
    let host = match WindowsSampler::identify(std::process::id()) {
        Ok(host) => host,
        Err(reason) => {
            shared.publish(MonitorFrame {
                host: None,
                completed_at_ms: shared.now_ms(),
                cycle_elapsed_us: micros(started.elapsed()),
                discovery: DiscoveryCoverage {
                    first_failure: Some(reason),
                    ..Default::default()
                },
                owned_processes: Vec::new(),
                aggregate: None,
                caller: None,
                failure: Some(MonitorFailure::HostUnavailable(reason)),
            });
            shared.phase(WorkerPhase::Failed);
            return;
        }
    };
    let sampler = WindowsSampler::new();
    let mut window = new_window();
    let mut caller = None;
    let mut retained = Vec::new();
    while !shared.stop.load(Ordering::Acquire) {
        if let Ok(report) = receiver.try_recv() {
            caller = Some(report);
        }
        shared
            .cycle_started_ms
            .store(shared.now_ms(), Ordering::Release);
        shared.phase(WorkerPhase::Sampling);
        let started = Instant::now();
        let (links, coverage) = links(started);
        let (keys, coverage) = descendants(
            host,
            &retained,
            &links,
            coverage,
            started,
            WindowsSampler::identify,
        );
        let (owned_processes, aggregate, failure) = match sampler.sample(&keys) {
            Ok(snapshot) => {
                // Confirmed identities remain owned after their parent exits. A
                // vanished/reused key is reported once, then discarded; denied or
                // uncertain identities keep their bounded slot until resolved.
                retained = snapshot
                    .processes
                    .iter()
                    .filter(|sample| {
                        !matches!(
                            sample.counters,
                            Measurement::Unavailable(
                                Unavailable::Missing
                                    | Unavailable::Terminated
                                    | Unavailable::IdentityChanged
                            )
                        )
                    })
                    .map(|sample| sample.key)
                    .collect();
                let aggregate = window.record(&snapshot).cloned();
                match aggregate {
                    Ok(mut aggregate) => {
                        if !coverage.complete() {
                            aggregate.process_samples_complete = false;
                            aggregate.interval = Err(IntervalUnavailable::MissingSample);
                            // Do not form the next growth/CPU baseline from partial ancestry.
                            window = new_window();
                        }
                        (snapshot.processes, Some(aggregate), None)
                    }
                    Err(reason) => (
                        snapshot.processes,
                        None,
                        Some(MonitorFailure::Sampling(reason)),
                    ),
                }
            }
            Err(reason) => (Vec::new(), None, Some(MonitorFailure::Sampling(reason))),
        };
        shared.publish(MonitorFrame {
            host: Some(host),
            completed_at_ms: shared.now_ms(),
            cycle_elapsed_us: micros(started.elapsed()),
            discovery: coverage,
            owned_processes,
            aggregate,
            caller,
            failure,
        });
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        shared.phase(WorkerPhase::Waiting);
        let wait_started = Instant::now();
        while !shared.stop.load(Ordering::Acquire) {
            let Some(remaining) = config.update_period.checked_sub(wait_started.elapsed()) else {
                break;
            };
            thread::park_timeout(remaining);
        }
    }
    shared.phase(WorkerPhase::Stopped);
}

#[derive(Clone, Copy)]
struct Link {
    pid: u32,
    parent: u32,
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn unavailable(code: u32) -> Unavailable {
    match code {
        ERROR_ACCESS_DENIED => Unavailable::AccessDenied,
        ERROR_INVALID_PARAMETER => Unavailable::Missing,
        other => Unavailable::OsError(other),
    }
}

// Only the process-list snapshot is requested: no module, path, command-line,
// environment, executable-name or file reads. No ERROR_BAD_LENGTH retry loop.
fn links(started: Instant) -> (Vec<Link>, DiscoveryCoverage) {
    let mut coverage = DiscoveryCoverage::default();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        coverage.first_failure = Some(unavailable(unsafe { GetLastError() }));
        return (Vec::new(), coverage);
    }
    let snapshot = Handle(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut present = unsafe { Process32FirstW(snapshot.0, &mut entry) };
    let mut result = Vec::new();
    loop {
        if present == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_NO_MORE_FILES {
                coverage.snapshot_complete = true;
            } else {
                coverage.first_failure = Some(unavailable(error));
            }
            break;
        }
        result.push(Link {
            pid: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
        });
        if result.len() == MAX_ENUMERATED_PROCESSES {
            break;
        }
        if started.elapsed() >= DISCOVERY_BUDGET {
            coverage.deadline_reached = true;
            break;
        }
        present = unsafe { Process32NextW(snapshot.0, &mut entry) };
    }
    coverage.enumerated_processes = result.len();
    (result, coverage)
}

fn descendants(
    host: ProcessKey,
    retained: &[ProcessKey],
    links: &[Link],
    mut coverage: DiscoveryCoverage,
    started: Instant,
    mut identify: impl FnMut(u32) -> Result<ProcessKey, Unavailable>,
) -> (Vec<ProcessKey>, DiscoveryCoverage) {
    let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for link in links.iter().take(MAX_ENUMERATED_PROCESSES) {
        children.entry(link.parent).or_default().push(link.pid);
    }
    let mut keys = vec![host];
    let mut pending = VecDeque::from([host]);
    let mut seen = BTreeSet::from([host.pid]);
    // Seed every previously confirmed key before native reads. Even a discovery
    // deadline cannot silently drop an orphan or an access-denied prior member.
    for &key in retained.iter().take(MAX_PROCESSES) {
        if !seen.insert(key.pid) {
            continue;
        }
        if keys.len() == MAX_PROCESSES {
            coverage.owned_limit_reached = true;
            break;
        }
        keys.push(key);
    }
    for index in 1..keys.len() {
        let key = keys[index];
        if started.elapsed() >= DISCOVERY_BUDGET {
            coverage.deadline_reached = true;
            return (keys, coverage);
        }
        match identify(key.pid) {
            Ok(current) if current == key => pending.push_back(key),
            Ok(_) => coverage.unavailable(Unavailable::IdentityChanged),
            Err(reason) => coverage.unavailable(reason),
        }
    }
    while let Some(parent) = pending.pop_front() {
        for &pid in children
            .get(&parent.pid)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if started.elapsed() >= DISCOVERY_BUDGET {
                coverage.deadline_reached = true;
                return (keys, coverage);
            }
            if !seen.insert(pid) {
                if keys
                    .iter()
                    .any(|key| key.pid == pid && key.created_at_100ns < parent.created_at_100ns)
                {
                    coverage.rejected_ancestry += 1;
                }
                continue;
            }
            if keys.len() == MAX_PROCESSES {
                coverage.owned_limit_reached = true;
                return (keys, coverage);
            }
            match identify(pid) {
                Ok(key) if key.pid == pid && key.created_at_100ns >= parent.created_at_100ns => {
                    keys.push(key);
                    pending.push_back(key);
                }
                Ok(_) => {
                    coverage.rejected_ancestry += 1;
                    coverage
                        .first_failure
                        .get_or_insert(Unavailable::IdentityChanged);
                }
                Err(reason) => coverage.unavailable(reason),
            }
        }
    }
    (keys, coverage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn host() -> ProcessKey {
        ProcessKey {
            pid: 1,
            created_at_100ns: 100,
        }
    }
    fn complete() -> DiscoveryCoverage {
        DiscoveryCoverage {
            snapshot_complete: true,
            ..Default::default()
        }
    }

    #[test]
    fn ancestry_refusals_and_missing_candidates_are_explicit_not_zero_usage() {
        let links = [
            Link { pid: 2, parent: 1 },
            Link { pid: 3, parent: 1 },
            Link { pid: 4, parent: 2 },
        ];
        let (keys, coverage) = descendants(
            host(),
            &[],
            &links,
            complete(),
            Instant::now(),
            |pid| match pid {
                2 => Ok(ProcessKey {
                    pid,
                    created_at_100ns: 99,
                }),
                _ => Err(Unavailable::AccessDenied),
            },
        );
        assert_eq!(keys, [host()]);
        assert_eq!(coverage.rejected_ancestry, 1);
        assert_eq!(coverage.unavailable_candidates, 1);
        assert!(!coverage.complete());
    }

    #[test]
    fn process_membership_never_exceeds_the_sampler_bound() {
        let links: Vec<_> = (2..=MAX_ENUMERATED_PROCESSES as u32)
            .map(|pid| Link { pid, parent: 1 })
            .collect();
        let (keys, coverage) =
            descendants(host(), &[], &links, complete(), Instant::now(), |pid| {
                Ok(ProcessKey {
                    pid,
                    created_at_100ns: 101,
                })
            });
        assert_eq!(keys.len(), MAX_PROCESSES);
        assert!(coverage.owned_limit_reached);
        assert!(!coverage.complete());
    }

    #[test]
    fn a_confirmed_orphan_remains_owned_and_reuse_is_incomplete() {
        let orphan = ProcessKey {
            pid: 3,
            created_at_100ns: 300,
        };
        let retired_parent = ProcessKey {
            pid: 2,
            created_at_100ns: 200,
        };
        let links = [Link { pid: 3, parent: 2 }, Link { pid: 4, parent: 3 }];
        let (keys, coverage) = descendants(
            host(),
            &[retired_parent, orphan],
            &links,
            complete(),
            Instant::now(),
            |pid| match pid {
                2 => Err(Unavailable::Missing),
                3 => Ok(orphan),
                _ => Ok(ProcessKey {
                    pid,
                    created_at_100ns: 400,
                }),
            },
        );
        assert!(keys.contains(&orphan));
        assert!(keys.contains(&ProcessKey {
            pid: 4,
            created_at_100ns: 400
        }));
        assert!(!coverage.complete());
        let (keys, coverage) =
            descendants(host(), &[orphan], &[], complete(), Instant::now(), |pid| {
                Ok(ProcessKey {
                    pid,
                    created_at_100ns: 999,
                })
            });
        assert_eq!(keys, [host(), orphan]);
        assert_eq!(coverage.first_failure, Some(Unavailable::IdentityChanged));
        assert!(!coverage.complete());
    }

    #[test]
    fn controlled_descendant_fixture() {
        if std::env::var_os("SHINATAMA_RESOURCE_FIXTURE").is_some() {
            // A finite self-test child, with no inherited environment or agent/config reads.
            thread::sleep(Duration::from_secs(4));
        }
    }

    #[test]
    fn native_background_monitor_measures_own_descendant_and_reports_real_caller_values() {
        assert!(matches!(
            ResourceMonitor::start(MonitorConfig {
                update_period: Duration::ZERO
            }),
            Err(StartError::InvalidPeriod)
        ));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "resources::monitor::tests::controlled_descendant_fixture",
                "--nocapture",
            ])
            .env_clear()
            .env("SHINATAMA_RESOURCE_FIXTURE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let child_pid = child.id();
        let result = std::panic::catch_unwind(|| {
            let monitor = ResourceMonitor::start(MonitorConfig {
                update_period: MIN_UPDATE_PERIOD,
            })
            .unwrap();
            assert!(matches!(
                ResourceMonitor::start(MonitorConfig::default()),
                Err(StartError::AlreadyRunning)
            ));
            let telemetry = CallerTelemetry {
                input_latency_us: Some(1234),
                pending_input_events: Some(2),
                pending_output_bytes: None,
                queued_launches: Some(0),
                ..Default::default()
            };
            monitor.report(telemetry).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Ok(Some(frame)) = monitor.current()
                    && frame.owned_processes.iter().any(|sample| {
                        sample.key.pid == child_pid
                            && matches!(sample.counters, Measurement::Available(_))
                    })
                    && frame
                        .caller
                        .is_some_and(|report| report.telemetry == telemetry)
                {
                    assert_eq!(frame.host.unwrap().pid, std::process::id());
                    assert!(frame.owned_processes.len() <= MAX_PROCESSES);
                    assert!(frame.cycle_elapsed_us > 0);
                    assert!(frame.aggregate.is_some());
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "owned child was not measured within the test deadline"
                );
                thread::sleep(Duration::from_millis(10));
            }
            assert!(monitor.status().completed_cycles > 0);
            let shared = Arc::clone(&monitor.shared);
            let before = Instant::now();
            drop(monitor);
            assert!(
                before.elapsed() < Duration::from_millis(100),
                "Drop must not join native sampling"
            );
            let deadline = Instant::now() + Duration::from_secs(1);
            while shared.phase.load(Ordering::Acquire) != WorkerPhase::Stopped as u8 {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(5));
            }
        });
        let _ = child.kill(); // Only this known self-test child is ever signalled.
        let _ = child.wait();
        if let Err(reason) = result {
            std::panic::resume_unwind(reason);
        }
    }
}
