//! Advice only: no native launch permit, executor, SQL, PID termination or job
//! registry. Native reads/evaluation and the cheap optional-work callback belong
//! to one retained background worker. A stuck worker is never replaced.
use serde::{Serialize, Serializer, ser::SerializeStruct};
#[cfg(windows)]
use shinatama_engine::resources::monitor::{ResourceMonitor, WorkerPhase};
use shinatama_engine::resources::{
    policy::{
        CpuCoverage, Decision, Frame, PolicyConfig, PolicyError, ProfileLimits, Reason,
        ResourcePolicy, Stage,
    },
    sampling::{Aggregate, IntervalUnavailable, Measurement, Unavailable},
};
use std::sync::{
    Arc, Mutex, TryLockError,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::time::Duration;
#[cfg(windows)]
use std::{
    thread::{self, Thread},
    time::Instant,
};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct GovernorConfig {
    #[serde(serialize_with = "serialize_policy")]
    pub policy: PolicyConfig,
    #[serde(serialize_with = "serialize_period")]
    pub update_period: Duration,
    pub max_frame_time_us: u64,
    pub max_ipc_roundtrip_us: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Phase {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    Unsupported,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Snapshot {
    pub observed_at_ms: u64,
    pub phase: Phase,
    #[serde(serialize_with = "serialize_stage")]
    pub stage: Stage,
    #[serde(serialize_with = "serialize_reason")]
    pub reason: Reason,
    pub reduce_optional: bool,
    pub defer_new_starts: bool,
    pub advice_only: bool,
    pub native_launch_enabled: bool,
    pub native_workloads_protected: bool,
    pub active_native_workloads: usize,
    pub configured_profiles: usize,
    pub termination_recommended_but_not_executed: bool,
    pub monitor_age_ms: Option<u64>,
    pub caller_age_ms: Option<u64>,
    pub host_responsive: Option<bool>,
    #[serde(serialize_with = "serialize_coverage")]
    pub cpu_coverage: CpuCoverage,
    pub config: GovernorConfig,
    #[serde(skip)]
    sample_at_ms: Option<u64>,
    #[serde(skip)]
    caller_at_ms: Option<u64>,
}
impl Snapshot {
    fn deferred(
        config: GovernorConfig,
        profiles: usize,
        now: u64,
        phase: Phase,
        reason: Reason,
    ) -> Self {
        Self {
            observed_at_ms: now,
            phase,
            stage: Stage::DeferStarts,
            reason,
            reduce_optional: true,
            defer_new_starts: true,
            advice_only: true,
            native_launch_enabled: false,
            native_workloads_protected: false,
            active_native_workloads: 0,
            configured_profiles: profiles,
            termination_recommended_but_not_executed: false,
            monitor_age_ms: None,
            caller_age_ms: None,
            host_responsive: None,
            cpu_coverage: CpuCoverage::Missing,
            config,
            sample_at_ms: None,
            caller_at_ms: None,
        }
    }
    fn refresh(mut self, now: u64, phase: Phase) -> Self {
        self.phase = phase;
        self.monitor_age_ms = self.sample_at_ms.and_then(|at| now.checked_sub(at));
        self.caller_age_ms = self.caller_at_ms.and_then(|at| now.checked_sub(at));
        let stale = now < self.observed_at_ms
            || self.sample_at_ms.is_some_and(|at| at > now)
            || self.caller_at_ms.is_some_and(|at| at > now)
            || self
                .monitor_age_ms
                .is_some_and(|age| age > self.config.policy.max_sample_age_ms)
            || self
                .caller_age_ms
                .is_some_and(|age| age > self.config.policy.max_sample_age_ms);
        let missing = self.sample_at_ms.is_none() || self.caller_at_ms.is_none();
        if phase != Phase::Running || stale || missing {
            self.stage = Stage::DeferStarts;
            self.reason = if stale && phase == Phase::Running {
                Reason::StaleTelemetry
            } else {
                Reason::MissingTelemetry
            };
            self.reduce_optional = true;
            self.defer_new_starts = true;
            self.host_responsive = None;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    InvalidConfig,
    AlreadyRunning,
    ThreadUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    Busy,
    Poisoned,
}
struct Shared {
    stop: AtomicBool,
    phase: AtomicU8,
    latest: Mutex<Snapshot>,
}
pub struct ResourceGovernor {
    shared: Arc<Shared>,
    #[cfg(windows)]
    monitor: Arc<ResourceMonitor>,
    #[cfg(windows)]
    worker: Thread,
}
#[derive(Clone)]
pub struct GovernorReader {
    shared: Arc<Shared>,
    #[cfg(windows)]
    monitor: Arc<ResourceMonitor>,
}
impl ResourceGovernor {
    /// Start once with explicit, unqualified configuration. The callback must be
    /// cheap, nonblocking and contain no native/SQL work. No launch is enabled.
    pub fn start(
        #[cfg(windows)] monitor: Arc<ResourceMonitor>,
        config: GovernorConfig,
        profiles: Vec<ProfileLimits>,
        inhibit_optional_work: Arc<dyn Fn(bool) + Send + Sync>,
    ) -> Result<Self, StartError> {
        if !(Duration::from_millis(250)..=Duration::from_secs(30)).contains(&config.update_period)
            || config.max_frame_time_us == 0
            || config.max_ipc_roundtrip_us == 0
        {
            return Err(StartError::InvalidConfig);
        }
        let profile_count = profiles.len();
        let policy =
            ResourcePolicy::new(config.policy, profiles).map_err(|_| StartError::InvalidConfig)?;
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            phase: AtomicU8::new(Phase::Starting as u8),
            latest: Mutex::new(Snapshot::deferred(
                config,
                profile_count,
                0,
                Phase::Starting,
                Reason::MissingTelemetry,
            )),
        });
        #[cfg(windows)]
        {
            if ACTIVE
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Err(StartError::AlreadyRunning);
            }
            let state = Arc::clone(&shared);
            let source = Arc::clone(&monitor);
            let worker = match thread::Builder::new()
                .name("shinatama-governor".into())
                .spawn(move || {
                    let _slot = WorkerSlot;
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run(
                            &state,
                            &source,
                            policy,
                            config,
                            profile_count,
                            inhibit_optional_work,
                        )
                    }))
                    .is_err()
                    {
                        state.phase.store(Phase::Failed as u8, Ordering::Release);
                    }
                }) {
                Ok(worker) => worker.thread().clone(),
                Err(_) => {
                    ACTIVE.store(false, Ordering::Release);
                    return Err(StartError::ThreadUnavailable);
                }
            };
            Ok(Self {
                shared,
                monitor,
                worker,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (policy, inhibit_optional_work);
            shared
                .phase
                .store(Phase::Unsupported as u8, Ordering::Release);
            Ok(Self { shared })
        }
    }
    /// No native reads, joins, SQL or waiting on the worker/monitor mutex. A read
    /// error is unavailable advice: callers must keep new starts deferred.
    pub fn status(&self) -> Result<Snapshot, ReadError> {
        self.reader().status()
    }
    pub fn reader(&self) -> GovernorReader {
        GovernorReader {
            shared: Arc::clone(&self.shared),
            #[cfg(windows)]
            monitor: Arc::clone(&self.monitor),
        }
    }
}
impl GovernorReader {
    pub fn status(&self) -> Result<Snapshot, ReadError> {
        let snapshot = match self.shared.latest.try_lock() {
            Ok(latest) => *latest,
            Err(TryLockError::WouldBlock) => return Err(ReadError::Busy),
            Err(TryLockError::Poisoned(_)) => return Err(ReadError::Poisoned),
        };
        let phase = match self.shared.phase.load(Ordering::Acquire) {
            0 => Phase::Starting,
            1 => Phase::Running,
            2 => Phase::Stopping,
            3 => Phase::Stopped,
            4 => Phase::Failed,
            _ => Phase::Unsupported,
        };
        #[cfg(windows)]
        let now = self.monitor.status().observed_at_ms;
        #[cfg(not(windows))]
        let now = 0;
        Ok(snapshot.refresh(now, phase))
    }
}
impl Drop for ResourceGovernor {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        #[cfg(windows)]
        {
            let _ = self
                .shared
                .phase
                .try_update(Ordering::AcqRel, Ordering::Acquire, |phase| {
                    (phase < Phase::Stopping as u8).then_some(Phase::Stopping as u8)
                });
            self.worker.unpark();
        }
    }
}

#[derive(Clone, Copy)]
struct Caller {
    at_ms: u64,
    frame_time_us: Option<u64>,
    ipc_roundtrip_us: Option<u64>,
}
fn evaluate(
    policy: &mut ResourcePolicy,
    config: GovernorConfig,
    now: u64,
    aggregate: Option<&Aggregate>,
    complete: bool,
    caller: Option<Caller>,
    cpu: CpuCoverage,
) -> Snapshot {
    let caller_age = caller.and_then(|caller| now.checked_sub(caller.at_ms));
    let responsive = caller
        .filter(|_| caller_age.is_some_and(|age| age <= config.policy.max_sample_age_ms))
        .and_then(|caller| {
            Some(
                caller.frame_time_us? <= config.max_frame_time_us
                    && caller.ipc_roundtrip_us? <= config.max_ipc_roundtrip_us,
            )
        });
    // Explicitly unavailable placeholder, never exposed as an observed zero or
    // healthy baseline; it allows the policy to enter its own recovery hysteresis.
    let mut system = aggregate.cloned().unwrap_or_else(|| missing(now));
    system.process_samples_complete &= complete;
    let result = policy.evaluate(
        now,
        &Frame {
            system: &system,
            cpu_coverage: cpu,
            host_responsive: responsive,
            jobs: &[],
        },
    );
    let mut snapshot = Snapshot::deferred(config, 0, now, Phase::Running, Reason::MissingTelemetry);
    if let Ok(Decision {
        stage,
        reason,
        reduce_optional,
        defer_new_starts,
        terminate,
    }) = result
    {
        snapshot.stage = stage;
        snapshot.reason = reason;
        snapshot.reduce_optional = reduce_optional;
        snapshot.defer_new_starts = defer_new_starts;
        snapshot.termination_recommended_but_not_executed = terminate.is_some();
    } else if matches!(result, Err(PolicyError::NonIncreasingTime)) {
        snapshot.reason = Reason::StaleTelemetry;
    }
    snapshot.sample_at_ms = aggregate.map(|value| value.at_ms);
    snapshot.caller_at_ms = caller.map(|value| value.at_ms);
    snapshot.monitor_age_ms = snapshot.sample_at_ms.and_then(|at| now.checked_sub(at));
    snapshot.caller_age_ms = caller_age;
    snapshot.host_responsive = responsive;
    snapshot.cpu_coverage = cpu;
    snapshot
}
fn missing(now: u64) -> Aggregate {
    Aggregate {
        at_ms: now,
        sampling_elapsed_us: 0,
        unique_processes: 0,
        available_processes: 0,
        observed_private_commit_bytes: 0,
        observed_resident_bytes: 0,
        process_samples_complete: false,
        interval: Err(IntervalUnavailable::MissingSample),
        observed_private_commit_peak_bytes: 0,
        system: Measurement::Unavailable(Unavailable::Missing),
        remaining_commit_bytes: None,
        observed_group_cpu_busy_basis_points: None,
    }
}

#[cfg(windows)]
static ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
struct WorkerSlot;
#[cfg(windows)]
impl Drop for WorkerSlot {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}
#[cfg(windows)]
fn run(
    shared: &Shared,
    monitor: &ResourceMonitor,
    mut policy: ResourcePolicy,
    config: GovernorConfig,
    profiles: usize,
    inhibit: Arc<dyn Fn(bool) + Send + Sync>,
) {
    let mut prior_reduction = None;
    while !shared.stop.load(Ordering::Acquire) {
        let status = monitor.status();
        let frame = monitor.current().ok().flatten();
        let groups =
            unsafe { windows_sys::Win32::System::Threading::GetActiveProcessorGroupCount() };
        let cpu = match groups {
            1 => CpuCoverage::CompleteMachine,
            0 => CpuCoverage::Missing,
            _ => CpuCoverage::PartialProcessorGroup,
        };
        let complete = matches!(status.phase, WorkerPhase::Sampling | WorkerPhase::Waiting)
            && frame.as_ref().is_some_and(|frame| frame.complete());
        let caller = frame
            .as_ref()
            .and_then(|frame| frame.caller)
            .map(|caller| Caller {
                at_ms: caller.observed_at_ms,
                frame_time_us: caller.telemetry.frame_time_us,
                ipc_roundtrip_us: caller.telemetry.ipc_roundtrip_us,
            });
        let mut snapshot = evaluate(
            &mut policy,
            config,
            status.observed_at_ms,
            frame.as_ref().and_then(|frame| frame.aggregate.as_ref()),
            complete,
            caller,
            cpu,
        );
        snapshot.configured_profiles = profiles;
        if prior_reduction != Some(snapshot.reduce_optional) {
            inhibit(snapshot.reduce_optional);
            prior_reduction = Some(snapshot.reduce_optional);
        }
        if let Ok(mut latest) = shared.latest.lock() {
            *latest = snapshot;
        }
        shared.phase.store(Phase::Running as u8, Ordering::Release);
        let started = Instant::now();
        while !shared.stop.load(Ordering::Acquire) {
            let Some(remaining) = config.update_period.checked_sub(started.elapsed()) else {
                break;
            };
            thread::park_timeout(remaining);
        }
    }
    if prior_reduction != Some(true) {
        inhibit(true);
    }
    shared.phase.store(Phase::Stopped as u8, Ordering::Release);
}

fn serialize_period<S: Serializer>(value: &Duration, out: S) -> Result<S::Ok, S::Error> {
    out.serialize_u64(value.as_millis() as u64)
}
fn serialize_stage<S: Serializer>(value: &Stage, out: S) -> Result<S::Ok, S::Error> {
    out.serialize_str(match value {
        Stage::Normal => "normal",
        Stage::ReduceOptional => "reduce_optional",
        Stage::DeferStarts => "defer_starts",
    })
}
fn serialize_reason<S: Serializer>(value: &Reason, out: S) -> Result<S::Ok, S::Error> {
    out.serialize_str(match value {
        Reason::Healthy => "healthy",
        Reason::Pressure => "pressure",
        Reason::BudgetExceeded => "budget_exceeded",
        Reason::RecoveryWait => "recovery_wait",
        Reason::MissingTelemetry => "missing_telemetry",
        Reason::StaleTelemetry => "stale_telemetry",
        Reason::PartialCpuCoverage => "partial_cpu_coverage",
        Reason::AttributedViolation => "attributed_violation",
    })
}
fn serialize_coverage<S: Serializer>(value: &CpuCoverage, out: S) -> Result<S::Ok, S::Error> {
    out.serialize_str(match value {
        CpuCoverage::CompleteMachine => "complete_machine",
        CpuCoverage::PartialProcessorGroup => "partial_processor_group",
        CpuCoverage::Missing => "missing",
    })
}
fn serialize_policy<S: Serializer>(value: &PolicyConfig, out: S) -> Result<S::Ok, S::Error> {
    let mut fields = out.serialize_struct("PolicyConfig", 15)?;
    macro_rules! fields { ($($name:ident),* $(,)?) => { $(fields.serialize_field(stringify!($name), &value.$name)?;)* }; }
    fields!(
        physical_available_floor_bytes,
        commit_available_floor_bytes,
        pressure_enter_basis_points,
        pressure_exit_basis_points,
        emergency_basis_points,
        max_sample_age_ms,
        calibration_window_ms,
        calibration_samples,
        minimum_healthy_samples,
        stable_ms,
        reduce_before_defer_ms,
        defer_before_terminate_ms,
        cooldown_ms,
        learned_headroom_basis_points,
        maximum_upward_step_basis_points
    );
    fields.end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_engine::resources::sampling::{SystemCounters, SystemCpuCounters};
    fn config() -> GovernorConfig {
        GovernorConfig {
            policy: PolicyConfig {
                physical_available_floor_bytes: 10,
                commit_available_floor_bytes: 10,
                pressure_enter_basis_points: 8000,
                pressure_exit_basis_points: 6000,
                emergency_basis_points: 9500,
                max_sample_age_ms: 50,
                calibration_window_ms: 1000,
                calibration_samples: 4,
                minimum_healthy_samples: 2,
                stable_ms: 10,
                reduce_before_defer_ms: 10,
                defer_before_terminate_ms: 10,
                cooldown_ms: 10,
                learned_headroom_basis_points: 12000,
                maximum_upward_step_basis_points: 1000,
            },
            update_period: Duration::from_millis(250),
            max_frame_time_us: 50_000,
            max_ipc_roundtrip_us: 100_000,
        }
    }
    fn caller(at: u64) -> Caller {
        Caller {
            at_ms: at,
            frame_time_us: Some(10_000),
            ipc_roundtrip_us: Some(5000),
        }
    }
    #[test]
    fn desktop_defaults_allow_measured_headroom_and_recover_after_acute_pressure() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let config = crate::resources::default_governor_config();
        let mut policy = ResourcePolicy::new(config.policy, Vec::new()).unwrap();
        let startup = evaluate(
            &mut policy,
            config,
            0,
            None,
            false,
            None,
            CpuCoverage::Missing,
        );
        assert!(startup.defer_new_starts);
        let sample = |policy: &mut ResourcePolicy, at, remaining_commit| {
            let mut aggregate = missing(at);
            aggregate.process_samples_complete = true;
            aggregate.system = Measurement::Available(SystemCounters {
                physical_total_bytes: 32 * GIB,
                physical_available_bytes: 8 * GIB,
                commit_limit_bytes: 40 * GIB,
                commit_total_bytes: 40 * GIB - remaining_commit,
                observed_group_cpu: Measurement::Available(SystemCpuCounters {
                    processor_group: 0,
                    idle_100ns: 790,
                    kernel_including_idle_100ns: 790,
                    user_100ns: 210,
                }),
            });
            aggregate.remaining_commit_bytes = Some(remaining_commit);
            aggregate.observed_group_cpu_busy_basis_points = Some(2100);
            evaluate(
                policy,
                config,
                at,
                Some(&aggregate),
                true,
                Some(caller(at)),
                CpuCoverage::CompleteMachine,
            )
        };
        // Five GiB free commit on a forty-GiB host, with eight GiB physical,
        // complete counters and a responsive UI, supports fresh admission checks.
        assert!(sample(&mut policy, 500, 5 * GIB).defer_new_starts);
        let available = sample(&mut policy, 3500, 5 * GIB);
        assert!(!available.defer_new_starts);
        assert_eq!(sample(&mut policy, 6500, 5 * GIB).stage, Stage::Normal);

        // Genuine pressure still escalates, without attributing an invented job.
        sample(&mut policy, 7000, GIB);
        let acute = sample(&mut policy, 7500, GIB);
        assert!(acute.defer_new_starts);
        assert_eq!(acute.reason, Reason::Pressure);
        assert!(!acute.termination_recommended_but_not_executed);

        assert!(sample(&mut policy, 8000, 5 * GIB).defer_new_starts);
        assert!(sample(&mut policy, 10_999, 5 * GIB).defer_new_starts);
        let recovered = sample(&mut policy, 11_000, 5 * GIB);
        assert!(!recovered.defer_new_starts);
        assert!(recovered.advice_only && !recovered.native_launch_enabled);
        assert_eq!(sample(&mut policy, 14_000, 5 * GIB).stage, Stage::Normal);
    }
    #[test]
    fn absent_future_stale_and_partial_observations_defer_without_a_native_permit() {
        let config = config();
        let mut policy = ResourcePolicy::new(config.policy, Vec::new()).unwrap();
        let first = evaluate(
            &mut policy,
            config,
            10,
            None,
            false,
            None,
            CpuCoverage::Missing,
        );
        assert_eq!(
            (first.stage, first.reason),
            (Stage::DeferStarts, Reason::MissingTelemetry)
        );
        assert!(
            first.advice_only && !first.native_launch_enabled && !first.native_workloads_protected
        );
        let future = first.refresh(9, Phase::Running);
        assert!(future.defer_new_starts);
        let mut aggregate = missing(10);
        aggregate.process_samples_complete = true;
        let partial = evaluate(
            &mut policy,
            config,
            11,
            Some(&aggregate),
            true,
            Some(caller(11)),
            CpuCoverage::PartialProcessorGroup,
        );
        assert_eq!(partial.reason, Reason::PartialCpuCoverage);
        let stale = evaluate(
            &mut policy,
            config,
            100,
            Some(&aggregate),
            true,
            Some(caller(100)),
            CpuCoverage::CompleteMachine,
        );
        assert_eq!(stale.reason, Reason::StaleTelemetry);
    }
    #[test]
    fn caller_values_must_be_present_and_recent_and_clock_regression_is_refused() {
        let config = config();
        let mut policy = ResourcePolicy::new(config.policy, Vec::new()).unwrap();
        let partial = Caller {
            frame_time_us: None,
            ..caller(10)
        };
        let snapshot = evaluate(
            &mut policy,
            config,
            10,
            None,
            false,
            Some(partial),
            CpuCoverage::CompleteMachine,
        );
        assert_eq!(snapshot.host_responsive, None);
        let stale = evaluate(
            &mut policy,
            config,
            100,
            None,
            false,
            Some(caller(10)),
            CpuCoverage::CompleteMachine,
        );
        assert_eq!(stale.host_responsive, None);
        let backwards = evaluate(
            &mut policy,
            config,
            90,
            None,
            false,
            Some(caller(90)),
            CpuCoverage::CompleteMachine,
        );
        assert_eq!(backwards.reason, Reason::StaleTelemetry);
        assert!(backwards.defer_new_starts);
    }
}
