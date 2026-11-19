//! Bounded local measurements. This module neither discovers arbitrary children
//! nor admits, tunes, starts or terminates workloads. Run native reads off the UI
//! thread; finite API calls are not a hard kernel/I/O latency guarantee.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Storage ceilings for telemetry, not workload budgets or calibrated thresholds.
pub const MAX_PROCESSES: usize = 256;
pub const MAX_RETAINED_FRAMES: usize = 120;
pub const MAX_RETENTION_MS: u64 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessKey {
    pub pid: u32,
    /// Exact Windows FILETIME creation value; not rounded Unix milliseconds.
    pub created_at_100ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    Missing,
    Terminated,
    AccessDenied,
    IdentityChanged,
    InvalidCounter,
    OsError(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measurement<T> {
    Available(T),
    Unavailable(Unavailable),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessCounters {
    pub private_commit_bytes: u64,
    /// Working set includes shared pages: summed resident bytes are not unique RAM.
    pub resident_bytes: u64,
    pub cpu_time_100ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessSample {
    pub key: ProcessKey,
    pub counters: Measurement<ProcessCounters>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemCpuCounters {
    pub processor_group: u16,
    pub idle_100ns: u64,
    /// Windows kernel time includes idle time.
    pub kernel_including_idle_100ns: u64,
    pub user_100ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemCounters {
    pub physical_total_bytes: u64,
    pub physical_available_bytes: u64,
    pub commit_limit_bytes: u64,
    pub commit_total_bytes: u64,
    /// GetSystemTimes covers the calling thread's processor group on large hosts.
    pub observed_group_cpu: Measurement<SystemCpuCounters>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Monotonic time in the sampler's epoch; never compare different samplers.
    pub at_ms: u64,
    pub sampling_elapsed_us: u64,
    pub system: Measurement<SystemCounters>,
    pub processes: Vec<ProcessSample>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplingError {
    TooManyProcesses,
    InvalidProcessKey,
    ConflictingDuplicate,
    CounterOverflow,
    InvalidSystemCounters,
    NonIncreasingTime,
    InvalidRetention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntervalUnavailable {
    FirstOrChangedMembership,
    MissingSample,
    CounterRegression,
    CounterOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessInterval {
    pub elapsed_ms: u64,
    /// All matched process CPU deltas, without dividing by logical CPU count.
    pub cpu_time_100ns: u64,
    /// Exact rational growth; keep sign rather than saturating shrinkage to zero.
    pub private_commit_delta_bytes: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aggregate {
    pub at_ms: u64,
    pub sampling_elapsed_us: u64,
    pub unique_processes: usize,
    pub available_processes: usize,
    /// Lower bounds when any process sample is unavailable. Never treat an
    /// incomplete sample as evidence that an admission budget is available.
    pub observed_private_commit_bytes: u64,
    pub observed_resident_bytes: u64,
    pub process_samples_complete: bool,
    pub interval: Result<ProcessInterval, IntervalUnavailable>,
    /// Observed aggregate peak within this bounded window, not a tuned baseline.
    pub observed_private_commit_peak_bytes: u64,
    pub system: Measurement<SystemCounters>,
    pub remaining_commit_bytes: Option<u64>,
    pub observed_group_cpu_busy_basis_points: Option<u16>,
}

struct Frame {
    counters: BTreeMap<ProcessKey, ProcessCounters>,
    aggregate: Aggregate,
}

pub struct SamplingWindow {
    max_frames: usize,
    max_age_ms: u64,
    frames: VecDeque<Frame>,
}

impl SamplingWindow {
    pub fn new(max_frames: usize, max_age_ms: u64) -> Result<Self, SamplingError> {
        if !(1..=MAX_RETAINED_FRAMES).contains(&max_frames)
            || !(1..=MAX_RETENTION_MS).contains(&max_age_ms)
        {
            return Err(SamplingError::InvalidRetention);
        }
        Ok(Self {
            max_frames,
            max_age_ms,
            frames: VecDeque::new(),
        })
    }

    pub fn latest(&self) -> Option<&Aggregate> {
        self.frames.back().map(|frame| &frame.aggregate)
    }

    pub fn reports(&self) -> impl ExactSizeIterator<Item = &Aggregate> {
        self.frames.iter().map(|frame| &frame.aggregate)
    }

    /// Rejects a malformed frame atomically. Identical repeated process keys are
    /// counted once; discordant duplicates are refused instead of picking a value.
    pub fn record(&mut self, snapshot: &Snapshot) -> Result<&Aggregate, SamplingError> {
        if snapshot.processes.len() > MAX_PROCESSES {
            return Err(SamplingError::TooManyProcesses);
        }
        if self
            .latest()
            .is_some_and(|last| snapshot.at_ms <= last.at_ms)
        {
            return Err(SamplingError::NonIncreasingTime);
        }
        let remaining_commit_bytes = match snapshot.system {
            Measurement::Available(system) => {
                if system.physical_total_bytes == 0
                    || system.commit_limit_bytes == 0
                    || system.physical_available_bytes > system.physical_total_bytes
                    || system.commit_total_bytes > system.commit_limit_bytes
                {
                    return Err(SamplingError::InvalidSystemCounters);
                }
                Some(system.commit_limit_bytes - system.commit_total_bytes)
            }
            Measurement::Unavailable(_) => None,
        };
        let mut unique = BTreeMap::new();
        let mut counters = BTreeMap::new();
        let mut live_pids = BTreeSet::new();
        let mut committed = 0_u64;
        let mut resident = 0_u64;
        for sample in &snapshot.processes {
            if sample.key.pid == 0 || sample.key.created_at_100ns == 0 {
                return Err(SamplingError::InvalidProcessKey);
            }
            if let Some(previous) = unique.insert(sample.key, sample.counters) {
                if previous != sample.counters {
                    return Err(SamplingError::ConflictingDuplicate);
                }
                continue;
            }
            if let Measurement::Available(value) = sample.counters {
                // Two live identities for one PID cannot be a coherent snapshot.
                if !live_pids.insert(sample.key.pid) {
                    return Err(SamplingError::ConflictingDuplicate);
                }
                committed = committed
                    .checked_add(value.private_commit_bytes)
                    .ok_or(SamplingError::CounterOverflow)?;
                resident = resident
                    .checked_add(value.resident_bytes)
                    .ok_or(SamplingError::CounterOverflow)?;
                counters.insert(sample.key, value);
            }
        }
        let complete = unique.len() == counters.len();
        let previous = self
            .frames
            .back()
            .filter(|frame| snapshot.at_ms - frame.aggregate.at_ms <= self.max_age_ms);
        let interval = process_interval(previous, snapshot.at_ms, &counters, complete);
        let group_cpu =
            previous.and_then(|frame| system_cpu_delta(frame.aggregate.system, snapshot.system));
        // All validation/overflow work precedes mutation of the retained window.
        while self.frames.front().is_some_and(|frame| {
            snapshot.at_ms - frame.aggregate.at_ms > self.max_age_ms
                || self.frames.len() >= self.max_frames
        }) {
            self.frames.pop_front();
        }
        let peak = self
            .frames
            .iter()
            .map(|frame| frame.aggregate.observed_private_commit_bytes)
            .chain(std::iter::once(committed))
            .max()
            .unwrap_or(committed);
        self.frames.push_back(Frame {
            counters,
            aggregate: Aggregate {
                at_ms: snapshot.at_ms,
                sampling_elapsed_us: snapshot.sampling_elapsed_us,
                unique_processes: unique.len(),
                available_processes: live_pids.len(),
                observed_private_commit_bytes: committed,
                observed_resident_bytes: resident,
                process_samples_complete: complete,
                interval,
                observed_private_commit_peak_bytes: peak,
                system: snapshot.system,
                remaining_commit_bytes,
                observed_group_cpu_busy_basis_points: group_cpu,
            },
        });
        Ok(&self
            .frames
            .back()
            .expect("frame was just inserted")
            .aggregate)
    }
}

fn process_interval(
    previous: Option<&Frame>,
    at_ms: u64,
    counters: &BTreeMap<ProcessKey, ProcessCounters>,
    complete: bool,
) -> Result<ProcessInterval, IntervalUnavailable> {
    if !complete || previous.is_some_and(|frame| !frame.aggregate.process_samples_complete) {
        return Err(IntervalUnavailable::MissingSample);
    }
    let Some(previous) = previous else {
        return Err(IntervalUnavailable::FirstOrChangedMembership);
    };
    if previous.counters.len() != counters.len() {
        return Err(IntervalUnavailable::FirstOrChangedMembership);
    }
    let mut cpu = 0_u64;
    let mut growth = 0_i128;
    for (key, current) in counters {
        let old = previous
            .counters
            .get(key)
            .ok_or(IntervalUnavailable::FirstOrChangedMembership)?;
        let delta = current
            .cpu_time_100ns
            .checked_sub(old.cpu_time_100ns)
            .ok_or(IntervalUnavailable::CounterRegression)?;
        cpu = cpu
            .checked_add(delta)
            .ok_or(IntervalUnavailable::CounterOverflow)?;
        growth += i128::from(current.private_commit_bytes) - i128::from(old.private_commit_bytes);
    }
    Ok(ProcessInterval {
        elapsed_ms: at_ms - previous.aggregate.at_ms,
        cpu_time_100ns: cpu,
        private_commit_delta_bytes: growth,
    })
}

fn system_cpu_delta(
    old: Measurement<SystemCounters>,
    new: Measurement<SystemCounters>,
) -> Option<u16> {
    let (Measurement::Available(old), Measurement::Available(new)) = (old, new) else {
        return None;
    };
    let (Measurement::Available(old), Measurement::Available(new)) =
        (old.observed_group_cpu, new.observed_group_cpu)
    else {
        return None;
    };
    if old.processor_group != new.processor_group {
        return None;
    }
    let kernel = new
        .kernel_including_idle_100ns
        .checked_sub(old.kernel_including_idle_100ns)?;
    let user = new.user_100ns.checked_sub(old.user_100ns)?;
    let idle = new.idle_100ns.checked_sub(old.idle_100ns)?;
    if idle > kernel {
        return None;
    }
    let total = kernel.checked_add(user)?;
    if total == 0 {
        return None;
    }
    Some(((u128::from(total - idle) * 10_000) / u128::from(total)) as u16)
}

#[cfg(windows)]
pub use native::WindowsSampler;

#[cfg(windows)]
mod native {
    use super::*;
    use std::mem::size_of;
    use std::time::Instant;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, ERROR_NOT_FOUND, FILETIME,
        GetLastError, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetPerformanceInfo, K32GetProcessMemoryInfo, PERFORMANCE_INFORMATION,
        PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::SystemInformation::GROUP_AFFINITY;
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, GetProcessTimes, GetSystemTimes, GetThreadGroupAffinity, OpenProcess,
        PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        PROCESS_VM_READ, WaitForSingleObject,
    };

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn failure(code: u32) -> Unavailable {
        match code {
            ERROR_ACCESS_DENIED => Unavailable::AccessDenied,
            ERROR_INVALID_PARAMETER | ERROR_NOT_FOUND => Unavailable::Missing,
            other => Unavailable::OsError(other),
        }
    }

    fn open(pid: u32, memory: bool) -> Result<Handle, Unavailable> {
        if pid == 0 {
            return Err(Unavailable::Missing);
        }
        let rights = PROCESS_SYNCHRONIZE
            | if memory {
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ
            } else {
                PROCESS_QUERY_LIMITED_INFORMATION
            };
        let handle = unsafe { OpenProcess(rights, 0, pid) };
        if handle.is_null() {
            Err(failure(unsafe { GetLastError() }))
        } else {
            Ok(Handle(handle))
        }
    }

    fn alive(handle: &Handle) -> Result<(), Unavailable> {
        match unsafe { WaitForSingleObject(handle.0, 0) } {
            WAIT_TIMEOUT => Ok(()),
            WAIT_OBJECT_0 => Err(Unavailable::Terminated),
            _ => Err(failure(unsafe { GetLastError() })),
        }
    }

    fn ticks(value: FILETIME) -> u64 {
        (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
    }

    fn process_times(handle: &Handle) -> Result<(u64, u64), Unavailable> {
        let (mut creation, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        if unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) }
            == 0
        {
            return Err(failure(unsafe { GetLastError() }));
        }
        let cpu = ticks(kernel)
            .checked_add(ticks(user))
            .ok_or(Unavailable::InvalidCounter)?;
        Ok((ticks(creation), cpu))
    }

    fn process(key: ProcessKey) -> Result<ProcessCounters, Unavailable> {
        let handle = open(key.pid, true)?;
        let (creation, _) = process_times(&handle)?;
        if creation != key.created_at_100ns {
            return Err(Unavailable::IdentityChanged);
        }
        alive(&handle)?;
        let mut memory = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        if unsafe {
            K32GetProcessMemoryInfo(
                handle.0,
                (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                memory.cb,
            )
        } == 0
        {
            let reason = failure(unsafe { GetLastError() });
            return Err(alive(&handle).err().unwrap_or(reason));
        }
        let (_, cpu) = process_times(&handle)?;
        alive(&handle)?;
        Ok(ProcessCounters {
            private_commit_bytes: memory.PrivateUsage as u64,
            resident_bytes: memory.WorkingSetSize as u64,
            cpu_time_100ns: cpu,
        })
    }

    fn cpu() -> Measurement<SystemCpuCounters> {
        let mut before = GROUP_AFFINITY::default();
        let mut after = GROUP_AFFINITY::default();
        if unsafe { GetThreadGroupAffinity(GetCurrentThread(), &mut before) } == 0 {
            return Measurement::Unavailable(failure(unsafe { GetLastError() }));
        }
        let (mut idle, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return Measurement::Unavailable(failure(unsafe { GetLastError() }));
        }
        if unsafe { GetThreadGroupAffinity(GetCurrentThread(), &mut after) } == 0 {
            return Measurement::Unavailable(failure(unsafe { GetLastError() }));
        }
        if before.Group != after.Group {
            return Measurement::Unavailable(Unavailable::InvalidCounter);
        }
        Measurement::Available(SystemCpuCounters {
            processor_group: before.Group,
            idle_100ns: ticks(idle),
            kernel_including_idle_100ns: ticks(kernel),
            user_100ns: ticks(user),
        })
    }

    fn system() -> Result<SystemCounters, Unavailable> {
        let mut info = PERFORMANCE_INFORMATION {
            cb: size_of::<PERFORMANCE_INFORMATION>() as u32,
            ..Default::default()
        };
        if unsafe { K32GetPerformanceInfo(&mut info, info.cb) } == 0 {
            return Err(failure(unsafe { GetLastError() }));
        }
        let bytes = |pages: usize| {
            (pages as u64)
                .checked_mul(info.PageSize as u64)
                .ok_or(Unavailable::InvalidCounter)
        };
        if info.PageSize == 0 {
            return Err(Unavailable::InvalidCounter);
        }
        Ok(SystemCounters {
            physical_total_bytes: bytes(info.PhysicalTotal)?,
            physical_available_bytes: bytes(info.PhysicalAvailable)?,
            commit_limit_bytes: bytes(info.CommitLimit)?,
            commit_total_bytes: bytes(info.CommitTotal)?,
            observed_group_cpu: cpu(),
        })
    }

    impl<T> From<Result<T, Unavailable>> for Measurement<T> {
        fn from(value: Result<T, Unavailable>) -> Self {
            match value {
                Ok(value) => Self::Available(value),
                Err(reason) => Self::Unavailable(reason),
            }
        }
    }

    /// Owner supplies an already bounded membership list (host, renderer or owned
    /// job). No command line, path, environment or transcript is inspected.
    pub struct WindowsSampler {
        epoch: Instant,
    }
    impl Default for WindowsSampler {
        fn default() -> Self {
            Self::new()
        }
    }
    impl WindowsSampler {
        pub fn new() -> Self {
            Self {
                epoch: Instant::now(),
            }
        }

        pub fn identify(pid: u32) -> Result<ProcessKey, Unavailable> {
            let handle = open(pid, false)?;
            let (created_at_100ns, _) = process_times(&handle)?;
            alive(&handle)?;
            if created_at_100ns == 0 {
                return Err(Unavailable::InvalidCounter);
            }
            Ok(ProcessKey {
                pid,
                created_at_100ns,
            })
        }

        pub fn sample(&self, keys: &[ProcessKey]) -> Result<Snapshot, SamplingError> {
            if keys.len() > MAX_PROCESSES {
                return Err(SamplingError::TooManyProcesses);
            }
            let mut unique = BTreeSet::new();
            for key in keys {
                if key.pid == 0 || key.created_at_100ns == 0 {
                    return Err(SamplingError::InvalidProcessKey);
                }
                unique.insert(*key);
            }
            let started = Instant::now();
            let system = system().into();
            let processes = unique
                .into_iter()
                .map(|key| ProcessSample {
                    key,
                    counters: process(key).into(),
                })
                .collect();
            Ok(Snapshot {
                at_ms: u64::try_from(self.epoch.elapsed().as_millis())
                    .map_err(|_| SamplingError::CounterOverflow)?,
                sampling_elapsed_us: u64::try_from(started.elapsed().as_micros())
                    .map_err(|_| SamplingError::CounterOverflow)?,
                system,
                processes,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pid: u32, creation: u64, commit: u64, resident: u64, cpu: u64) -> ProcessSample {
        ProcessSample {
            key: ProcessKey {
                pid,
                created_at_100ns: creation,
            },
            counters: Measurement::Available(ProcessCounters {
                private_commit_bytes: commit,
                resident_bytes: resident,
                cpu_time_100ns: cpu,
            }),
        }
    }
    fn snapshot(at_ms: u64, processes: Vec<ProcessSample>) -> Snapshot {
        Snapshot {
            at_ms,
            sampling_elapsed_us: 12,
            system: Measurement::Available(SystemCounters {
                physical_total_bytes: 10_000,
                physical_available_bytes: 4_000,
                commit_limit_bytes: 20_000,
                commit_total_bytes: 9_000,
                observed_group_cpu: Measurement::Unavailable(Unavailable::Missing),
            }),
            processes,
        }
    }
    #[test]
    fn counts_unique_keys_and_keeps_commit_separate_from_working_set() {
        let one = sample(1, 123, 900, 200, 5);
        let two = sample(2, 456, 700, 800, 8);
        let mut window = SamplingWindow::new(3, 1000).unwrap();
        let result = window.record(&snapshot(0, vec![one, one, two])).unwrap();
        assert_eq!(
            (result.unique_processes, result.available_processes),
            (2, 2)
        );
        assert_eq!(
            (
                result.observed_private_commit_bytes,
                result.observed_resident_bytes
            ),
            (1600, 1000)
        );
        assert_eq!(result.remaining_commit_bytes, Some(11_000));
        assert!(result.process_samples_complete);
    }
    #[test]
    fn intervals_measure_growth_shrinkage_and_cpu_without_tuning_limits() {
        let mut window = SamplingWindow::new(3, 1000).unwrap();
        window
            .record(&snapshot(0, vec![sample(1, 1, 100, 90, 20)]))
            .unwrap();
        let result = window
            .record(&snapshot(100, vec![sample(1, 1, 300, 90, 50)]))
            .unwrap();
        assert_eq!(
            result.interval,
            Ok(ProcessInterval {
                elapsed_ms: 100,
                cpu_time_100ns: 30,
                private_commit_delta_bytes: 200
            })
        );
        let result = window
            .record(&snapshot(200, vec![sample(1, 1, 50, 90, 80)]))
            .unwrap();
        assert_eq!(
            result.interval.as_ref().unwrap().private_commit_delta_bytes,
            -250
        );
        assert_eq!(result.observed_private_commit_peak_bytes, 300);
    }
    #[test]
    fn missing_access_denied_and_terminated_never_become_valid_zero_samples() {
        for reason in [
            Unavailable::Missing,
            Unavailable::AccessDenied,
            Unavailable::Terminated,
            Unavailable::IdentityChanged,
        ] {
            let mut window = SamplingWindow::new(3, 1000).unwrap();
            let one = sample(1, 1, 100, 90, 20);
            window.record(&snapshot(0, vec![one])).unwrap();
            let missing = ProcessSample {
                counters: Measurement::Unavailable(reason),
                ..one
            };
            let result = window.record(&snapshot(1, vec![missing])).unwrap();
            assert!(!result.process_samples_complete);
            assert_eq!(result.interval, Err(IntervalUnavailable::MissingSample));
            let result = window.record(&snapshot(2, vec![one])).unwrap();
            assert_eq!(result.interval, Err(IntervalUnavailable::MissingSample));
        }
    }
    #[test]
    fn pid_reuse_and_counter_regression_start_no_false_cpu_or_growth_interval() {
        let mut window = SamplingWindow::new(3, 1000).unwrap();
        window
            .record(&snapshot(0, vec![sample(10, 1, 100, 90, 200)]))
            .unwrap();
        let result = window
            .record(&snapshot(1, vec![sample(10, 2, 999, 900, 1)]))
            .unwrap();
        assert_eq!(
            result.interval,
            Err(IntervalUnavailable::FirstOrChangedMembership)
        );
        let result = window
            .record(&snapshot(2, vec![sample(10, 2, 900, 900, 0)]))
            .unwrap();
        assert_eq!(result.interval, Err(IntervalUnavailable::CounterRegression));
    }
    #[test]
    fn cpu_delta_overflow_is_explicit_and_does_not_wrap_to_a_healthy_interval() {
        let mut window = SamplingWindow::new(2, 1000).unwrap();
        window
            .record(&snapshot(
                0,
                vec![sample(1, 1, 1, 1, 0), sample(2, 2, 1, 1, 0)],
            ))
            .unwrap();
        let result = window
            .record(&snapshot(
                1,
                vec![sample(1, 1, 1, 1, u64::MAX), sample(2, 2, 1, 1, 1)],
            ))
            .unwrap();
        assert_eq!(result.interval, Err(IntervalUnavailable::CounterOverflow));
    }
    #[test]
    fn overflow_conflicting_duplicates_and_time_errors_preserve_the_last_valid_frame() {
        let mut window = SamplingWindow::new(3, 1000).unwrap();
        window
            .record(&snapshot(10, vec![sample(1, 1, 100, 90, 20)]))
            .unwrap();
        assert_eq!(
            window
                .record(&snapshot(
                    11,
                    vec![sample(1, 1, u64::MAX, 1, 0), sample(2, 2, 1, 1, 0)]
                ))
                .unwrap_err(),
            SamplingError::CounterOverflow
        );
        assert_eq!(
            window
                .record(&snapshot(
                    11,
                    vec![sample(1, 1, 10, 1, 0), sample(1, 1, 20, 1, 0)]
                ))
                .unwrap_err(),
            SamplingError::ConflictingDuplicate
        );
        assert_eq!(
            window.record(&snapshot(10, vec![])).unwrap_err(),
            SamplingError::NonIncreasingTime
        );
        assert_eq!(window.latest().unwrap().at_ms, 10);
        assert_eq!(window.reports().len(), 1);
    }
    #[test]
    fn window_caps_age_and_count_and_drops_expired_peaks_and_cpu_baselines() {
        let mut window = SamplingWindow::new(2, 10).unwrap();
        for at in 0..3 {
            window
                .record(&snapshot(at, vec![sample(1, 1, 100 - at, 0, at)]))
                .unwrap();
        }
        assert_eq!(window.reports().len(), 2);
        let result = window
            .record(&snapshot(20, vec![sample(1, 1, 1, 1, 20)]))
            .unwrap();
        assert_eq!(result.observed_private_commit_peak_bytes, 1);
        assert_eq!(
            result.interval,
            Err(IntervalUnavailable::FirstOrChangedMembership)
        );
        assert_eq!(window.reports().len(), 1);
        assert!(SamplingWindow::new(MAX_RETAINED_FRAMES + 1, 1).is_err());
        assert!(SamplingWindow::new(1, MAX_RETENTION_MS + 1).is_err());
    }
    #[test]
    fn input_bound_is_checked_before_duplicate_reduction() {
        let mut window = SamplingWindow::new(1, 1).unwrap();
        let repeated = vec![sample(1, 1, 1, 1, 1); MAX_PROCESSES + 1];
        assert_eq!(
            window.record(&snapshot(0, repeated)).unwrap_err(),
            SamplingError::TooManyProcesses
        );
        assert!(window.latest().is_none());
    }
    #[test]
    fn system_pressure_missing_or_invalid_is_explicit_and_cpu_excludes_idle() {
        let mut window = SamplingWindow::new(3, 1000).unwrap();
        let mut first = snapshot(0, vec![]);
        let Measurement::Available(ref mut system) = first.system else {
            unreachable!()
        };
        system.observed_group_cpu = Measurement::Available(SystemCpuCounters {
            processor_group: 0,
            idle_100ns: 10,
            kernel_including_idle_100ns: 20,
            user_100ns: 10,
        });
        window.record(&first).unwrap();
        let mut second = first.clone();
        second.at_ms = 1;
        let Measurement::Available(ref mut system) = second.system else {
            unreachable!()
        };
        system.observed_group_cpu = Measurement::Available(SystemCpuCounters {
            processor_group: 0,
            idle_100ns: 30,
            kernel_including_idle_100ns: 60,
            user_100ns: 30,
        });
        assert_eq!(
            window
                .record(&second)
                .unwrap()
                .observed_group_cpu_busy_basis_points,
            Some(6666)
        );
        let mut missing = snapshot(2, vec![]);
        missing.system = Measurement::Unavailable(Unavailable::AccessDenied);
        let result = window.record(&missing).unwrap();
        assert_eq!(result.remaining_commit_bytes, None);
        let mut invalid = snapshot(3, vec![]);
        let Measurement::Available(ref mut system) = invalid.system else {
            unreachable!()
        };
        system.commit_total_bytes = system.commit_limit_bytes + 1;
        assert_eq!(
            window.record(&invalid).unwrap_err(),
            SamplingError::InvalidSystemCounters
        );
        assert_eq!(window.latest().unwrap().at_ms, 2);
    }
    #[test]
    fn switching_processor_groups_never_produces_a_system_cpu_delta() {
        let mut first = snapshot(0, vec![]);
        let mut second = snapshot(1, vec![]);
        let Measurement::Available(ref mut old) = first.system else {
            unreachable!()
        };
        old.observed_group_cpu = Measurement::Available(SystemCpuCounters {
            processor_group: 0,
            idle_100ns: 10,
            kernel_including_idle_100ns: 20,
            user_100ns: 10,
        });
        let Measurement::Available(ref mut new) = second.system else {
            unreachable!()
        };
        new.observed_group_cpu = Measurement::Available(SystemCpuCounters {
            processor_group: 1,
            idle_100ns: 30,
            kernel_including_idle_100ns: 60,
            user_100ns: 30,
        });
        let mut window = SamplingWindow::new(2, 1000).unwrap();
        window.record(&first).unwrap();
        assert_eq!(
            window
                .record(&second)
                .unwrap()
                .observed_group_cpu_busy_basis_points,
            None
        );
    }
    #[cfg(windows)]
    #[test]
    fn native_self_sample_preserves_exact_identity_and_refuses_a_reused_pid_binding() {
        let pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
        let key = WindowsSampler::identify(pid).unwrap();
        let sampler = WindowsSampler::new();
        let result = sampler.sample(&[key, key]).unwrap();
        assert_eq!(result.processes.len(), 1);
        assert_eq!(result.processes[0].key, key);
        let Measurement::Available(counters) = result.processes[0].counters else {
            panic!("the current process must be measurable");
        };
        assert!(counters.resident_bytes > 0);
        assert!(counters.private_commit_bytes > 0);
        let wrong = ProcessKey {
            created_at_100ns: key.created_at_100ns + 1,
            ..key
        };
        assert_eq!(
            sampler.sample(&[wrong]).unwrap().processes[0].counters,
            Measurement::Unavailable(Unavailable::IdentityChanged)
        );
    }
}
