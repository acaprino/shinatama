//! Private per-workload jobs. No process creation, assignment of running children,
//! host/renderer restriction or adaptive policy lives here. Native calls and finite
//! cleanup waits belong on a worker, never on the UI thread.
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

pub const MAX_JOB_PROCESSES: u32 = 256;
pub const MAX_CLEANUP_WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobBudget {
    job_commit_bytes: usize,
    process_commit_bytes: usize,
    max_processes: u32,
    cpu_hard_cap_basis_points: u16,
    limits_enforced: bool,
}

impl JobBudget {
    pub fn new(
        job_commit_bytes: u64,
        process_commit_bytes: u64,
        max_processes: u32,
        cpu_hard_cap_basis_points: u16,
    ) -> Result<Self, JobError> {
        if process_commit_bytes == 0
            || process_commit_bytes > job_commit_bytes
            || !(1..=MAX_JOB_PROCESSES).contains(&max_processes)
            || !(1..=10_000).contains(&cpu_hard_cap_basis_points)
        {
            return Err(JobError::InvalidBudget);
        }
        Ok(Self {
            job_commit_bytes: usize::try_from(job_commit_bytes)
                .map_err(|_| JobError::InvalidBudget)?,
            process_commit_bytes: usize::try_from(process_commit_bytes)
                .map_err(|_| JobError::InvalidBudget)?,
            max_processes,
            cpu_hard_cap_basis_points,
            limits_enforced: true,
        })
    }
    /// Interactive sessions retain memory, process count and CPU as advisory thresholds.
    /// Windows must not refuse their allocations or child processes, nor throttle their
    /// CPU, because of a Shinatama quota: a coding agent runs builds, installs and test
    /// runners that take dozens of processes. Ownership and kill-on-close cleanup remain.
    pub fn with_advisory_limits(mut self) -> Self {
        self.limits_enforced = false;
        self
    }
    pub fn limits_enforced(self) -> bool {
        self.limits_enforced
    }
    pub fn job_commit_bytes(self) -> u64 {
        self.job_commit_bytes as u64
    }
    pub fn process_commit_bytes(self) -> u64 {
        self.process_commit_bytes as u64
    }
    pub fn max_processes(self) -> u32 {
        self.max_processes
    }
    pub fn cpu_hard_cap_basis_points(self) -> u16 {
        self.cpu_hard_cap_basis_points
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobError {
    InvalidBudget,
    InvalidJobKey,
    InvalidWait,
    SlotBusy,
    SlotRetired,
    AdmissionClosed,
    NameCollision,
    LimitsNotApplied,
    InvalidCounter,
    IncompleteMembership,
    RundownUnverified,
    Allocation,
    CloseFailed,
    OwnerAlive,
    OwnerUnknown,
    OwnerSessionMismatch,
    Os(u32),
    Sampling(super::sampling::SamplingError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotState {
    Free = 0,
    Active = 1,
    Retired = 2,
}

/// Keep this slot with the coordinator's reservation. A retired slot cannot be
/// reset by an arbitrary new request. The DB reservation is released separately
/// with the exact ticket/owner and a native closed-empty proof.
#[derive(Clone)]
pub struct WorkloadSlot(Arc<AtomicU8>);
impl Default for WorkloadSlot {
    fn default() -> Self {
        Self::new()
    }
}
impl WorkloadSlot {
    pub fn new() -> Self {
        Self(Arc::new(AtomicU8::new(SlotState::Free as u8)))
    }
    pub fn state(&self) -> SlotState {
        match self.0.load(Ordering::Acquire) {
            0 => SlotState::Free,
            1 => SlotState::Active,
            _ => SlotState::Retired,
        }
    }
    fn acquire(&self) -> Result<(), JobError> {
        self.0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|state| {
                if state == 2 {
                    JobError::SlotRetired
                } else {
                    JobError::SlotBusy
                }
            })
    }
    fn set(&self, state: SlotState) {
        self.0.store(state as u8, Ordering::Release);
    }
}

#[cfg(windows)]
pub use native::{
    CleanupOutcome, ClosedEmptyJobProof, InheritedJobBinding, JobRecoveryObservation, JobSnapshot,
    MemberObservation, OwnedJobKey, ReconcileOutcome, StartupJobBinding, WorkloadJob,
    probe_job_recovery, reconcile_dead_owner,
};

#[cfg(windows)]
mod native {
    use super::super::sampling::{
        Measurement, ProcessKey, ProcessSample, Unavailable, WindowsSampler,
    };
    use super::*;
    use std::marker::PhantomPinned;
    use std::mem::{ManuallyDrop, size_of};
    use std::os::windows::io::{AsRawHandle, BorrowedHandle};
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::time::Instant;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HANDLE, SetLastError,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::JobObjects::*;
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetProcessId, GetProcessTimes,
        LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcess, PROC_THREAD_ATTRIBUTE_JOB_LIST,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, UpdateProcThreadAttribute,
        WaitForSingleObject,
    };

    struct Handle(HANDLE);
    impl Handle {
        fn close(self) -> Result<(), Self> {
            let this = ManuallyDrop::new(self);
            if unsafe { CloseHandle(this.0) } != 0 {
                Ok(())
            } else {
                Err(Self(this.0))
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Phase {
        Active,
        Retired,
        Empty,
        Closed,
    }

    struct TrackedProcess {
        key: ProcessKey,
        handle: Handle,
    }
    #[derive(Default)]
    struct Census {
        /// Lifetime count, including handles already observed signaled and closed.
        /// A missed short-lived descendant leaves a permanent accounting gap.
        verified_total: u64,
        live: Vec<TrackedProcess>,
    }
    impl Census {
        fn reap_signaled(&mut self) -> Result<(), JobError> {
            let mut index = 0;
            while index < self.live.len() {
                match unsafe { WaitForSingleObject(self.live[index].handle.0, 0) } {
                    WAIT_OBJECT_0 => {
                        self.live.swap_remove(index);
                    }
                    WAIT_TIMEOUT => index += 1,
                    _ => return Err(os_error()),
                }
            }
            Ok(())
        }
    }

    pub struct WorkloadJob {
        handle: Option<Handle>,
        slot: WorkloadSlot,
        budget: JobBudget,
        job_key: [u8; 16],
        owner: ProcessKey,
        owner_windows_session_id: u32,
        phase: Phase,
        assignments_sealed: bool,
        termination_confirmed: bool,
        census: Mutex<Census>,
    }
    // The unique handle can be used on another worker. Shared references expose
    // kernel queries plus the mutex-protected bounded census; shutdown/admission
    // changes require an exclusive borrow.
    unsafe impl Send for WorkloadJob {}
    unsafe impl Sync for WorkloadJob {}

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CleanupOutcome {
        EmptyConfirmed,
        Retired { error: Option<JobError> },
    }

    /// Private producer; neither serde nor a public constructor can fabricate it.
    /// The trusted DB bridge must additionally match the exact ticket/revision,
    /// owner nonce and job key. This is not a generic authorization to release.
    pub struct ClosedEmptyJobProof {
        job_key: [u8; 16],
        owner: ProcessKey,
        owner_windows_session_id: u32,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct OwnedJobKey {
        pub job_key: [u8; 16],
        pub owner: ProcessKey,
        pub owner_windows_session_id: u32,
    }
    impl ClosedEmptyJobProof {
        pub fn job_key(&self) -> [u8; 16] {
            self.job_key
        }
        pub fn owner(&self) -> ProcessKey {
            self.owner
        }
        pub fn owner_windows_session_id(&self) -> u32 {
            self.owner_windows_session_id
        }
        pub fn owned_key(&self) -> OwnedJobKey {
            OwnedJobKey {
                job_key: self.job_key,
                owner: self.owner,
                owner_windows_session_id: self.owner_windows_session_id,
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MemberObservation {
        pub pid: u32,
        pub identity: Measurement<ProcessKey>,
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct JobSnapshot {
        pub active_processes: u32,
        pub total_processes: u32,
        pub total_terminated_processes: u32,
        pub cpu_time_100ns: u64,
        pub peak_job_commit_bytes: u64,
        pub observed_private_commit_bytes: u64,
        pub observed_resident_bytes: u64,
        /// Current commit is the sum of sampled PrivateUsage, not the peak native
        /// counter or a stale notification value. False means a lower bound only.
        pub membership_complete: bool,
        pub members: Vec<MemberObservation>,
        pub processes: Vec<ProcessSample>,
    }

    #[repr(C)]
    struct MemberIds {
        assigned: u32,
        count: u32,
        ids: [usize; MAX_JOB_PROCESSES as usize],
    }

    fn name(key: [u8; 16]) -> Result<[u16; 80], JobError> {
        if key == [0; 16] {
            return Err(JobError::InvalidJobKey);
        }
        let mut result = [0; 80];
        let mut offset = 0;
        for unit in "Local\\Shinatama.Workload.".encode_utf16() {
            result[offset] = unit;
            offset += 1;
        }
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in key {
            result[offset] = u16::from(HEX[usize::from(byte >> 4)]);
            result[offset + 1] = u16::from(HEX[usize::from(byte & 15)]);
            offset += 2;
        }
        Ok(result)
    }

    fn os_error() -> JobError {
        JobError::Os(unsafe { GetLastError() })
    }
    fn current_session_id() -> Result<u32, JobError> {
        let mut session = 0;
        if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
            Err(os_error())
        } else {
            Ok(session)
        }
    }

    /// T is one of the fixed-size Win32 information records below, never caller
    /// supplied or derived from a native-reported allocation length.
    fn query<T: Default>(handle: &Handle, class: JOBOBJECTINFOCLASS) -> Result<T, JobError> {
        let mut value = T::default();
        if unsafe {
            QueryInformationJobObject(
                handle.0,
                class,
                (&mut value as *mut T).cast(),
                size_of::<T>() as u32,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(os_error());
        }
        Ok(value)
    }
    fn set<T>(handle: &Handle, class: JOBOBJECTINFOCLASS, value: &T) -> Result<(), JobError> {
        if unsafe {
            SetInformationJobObject(
                handle.0,
                class,
                (value as *const T).cast(),
                size_of::<T>() as u32,
            )
        } == 0
        {
            Err(os_error())
        } else {
            Ok(())
        }
    }
    fn extended(budget: JobBudget) -> JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if budget.limits_enforced {
            limits.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_PROCESS_MEMORY
                | JOB_OBJECT_LIMIT_JOB_MEMORY;
            limits.BasicLimitInformation.ActiveProcessLimit = budget.max_processes;
            limits.ProcessMemoryLimit = budget.process_commit_bytes;
            limits.JobMemoryLimit = budget.job_commit_bytes;
        }
        limits
    }
    /// An advisory budget applies no rate control: the zero value turns it off.
    fn cpu_limit(budget: JobBudget) -> JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
        if !budget.limits_enforced {
            return JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
        }
        JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
            ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
            Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
                CpuRate: u32::from(budget.cpu_hard_cap_basis_points),
            },
        }
    }
    fn accounting(handle: &Handle) -> Result<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobError> {
        query(handle, JobObjectBasicAccountingInformation)
    }
    fn census_proves_rundown(
        info: &JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        verified_total: u64,
        unsignaled_handles: usize,
    ) -> bool {
        info.ActiveProcesses == 0
            && unsignaled_handles == 0
            && u64::from(info.TotalProcesses) == verified_total
    }

    fn sealed_rundown(
        info: &JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        verified_total: u64,
        unsignaled_handles: usize,
        assignments_sealed: bool,
        termination_confirmed: bool,
    ) -> bool {
        assignments_sealed
            && termination_confirmed
            && info.ActiveProcesses == 0
            && unsignaled_handles == 0
            && u64::from(info.TotalProcesses) >= verified_total
    }

    impl WorkloadJob {
        pub fn create(
            slot: &WorkloadSlot,
            job_key: [u8; 16],
            budget: JobBudget,
        ) -> Result<Self, JobError> {
            let name = name(job_key)?;
            slot.acquire()?;
            let initialized = (|| {
                let owner = WindowsSampler::identify(unsafe { GetCurrentProcessId() })
                    .map_err(|_| JobError::OwnerUnknown)?;
                let owner_windows_session_id = current_session_id()?;
                // Null security attributes make the handle non-inheritable. Name
                // collision is terminal before configuring anybody else's object.
                unsafe {
                    SetLastError(0);
                }
                let raw = unsafe { CreateJobObjectW(std::ptr::null(), name.as_ptr()) };
                let error = unsafe { GetLastError() };
                if raw.is_null() {
                    return Err(JobError::Os(error));
                }
                let handle = Handle(raw);
                if error == ERROR_ALREADY_EXISTS {
                    return Err(JobError::NameCollision);
                }
                let limits = extended(budget);
                let cpu = cpu_limit(budget);
                set(&handle, JobObjectExtendedLimitInformation, &limits)?;
                if budget.limits_enforced {
                    set(&handle, JobObjectCpuRateControlInformation, &cpu)?;
                }
                let actual: JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
                    query(&handle, JobObjectExtendedLimitInformation)?;
                let actual_cpu: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION =
                    query(&handle, JobObjectCpuRateControlInformation)?;
                if actual.BasicLimitInformation.LimitFlags
                    != limits.BasicLimitInformation.LimitFlags
                    || actual.BasicLimitInformation.ActiveProcessLimit
                        != limits.BasicLimitInformation.ActiveProcessLimit
                    || actual.JobMemoryLimit != limits.JobMemoryLimit
                    || actual.ProcessMemoryLimit != limits.ProcessMemoryLimit
                    || actual_cpu.ControlFlags != cpu.ControlFlags
                    || (budget.limits_enforced
                        && unsafe { actual_cpu.Anonymous.CpuRate }
                            != u32::from(budget.cpu_hard_cap_basis_points))
                {
                    return Err(JobError::LimitsNotApplied);
                }
                Ok((handle, owner, owner_windows_session_id))
            })();
            match initialized {
                Ok((handle, owner, owner_windows_session_id)) => Ok(Self {
                    handle: Some(handle),
                    slot: slot.clone(),
                    budget,
                    job_key,
                    owner,
                    owner_windows_session_id,
                    phase: Phase::Active,
                    assignments_sealed: false,
                    termination_confirmed: false,
                    census: Mutex::new(Census::default()),
                }),
                Err(error) => {
                    slot.set(SlotState::Free);
                    Err(error)
                }
            }
        }
        fn handle(&self) -> &Handle {
            self.handle.as_ref().expect("live job owns its handle")
        }
        pub fn budget(&self) -> JobBudget {
            self.budget
        }
        pub fn owner(&self) -> ProcessKey {
            self.owner
        }
        pub fn job_key(&self) -> [u8; 16] {
            self.job_key
        }
        pub fn owner_windows_session_id(&self) -> u32 {
            self.owner_windows_session_id
        }
        pub fn owned_key(&self) -> OwnedJobKey {
            OwnedJobKey {
                job_key: self.job_key,
                owner: self.owner,
                owner_windows_session_id: self.owner_windows_session_id,
            }
        }

        /// Pin this guard before applying it. The guard must outlive the ENTIRE
        /// attribute list, including DeleteProcThreadAttributeList, not only spawn.
        pub fn startup_binding(&self) -> Result<StartupJobBinding<'_>, JobError> {
            if self.phase != Phase::Active || self.assignments_sealed {
                return Err(JobError::AdmissionClosed);
            }
            Ok(StartupJobBinding {
                job: self,
                handles: [self.handle().0],
                _pinned: PhantomPinned,
            })
        }

        /// Irreversible owner-worker barrier, never accepted from IPC. The caller
        /// must have destroyed every startup attribute list/temporary ASSIGN
        /// handle, excluded job handles from child inheritance, and received any
        /// trusted helper's confirmation that its temporary ASSIGN handles closed.
        /// This does not stop child creation inside the job or itself prove exit.
        pub fn seal_assignments(&mut self) -> Result<(), JobError> {
            if self.phase != Phase::Active {
                return Err(JobError::AdmissionClosed);
            }
            self.assignments_sealed = true;
            Ok(())
        }

        pub fn query(&self, sampler: &WindowsSampler) -> Result<JobSnapshot, JobError> {
            let before = accounting(self.handle())?;
            let mut ids = MemberIds {
                assigned: 0,
                count: 0,
                ids: [0; MAX_JOB_PROCESSES as usize],
            };
            if unsafe {
                QueryInformationJobObject(
                    self.handle().0,
                    JobObjectBasicProcessIdList,
                    (&mut ids as *mut MemberIds).cast(),
                    size_of::<MemberIds>() as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(os_error());
            }
            // Read storage is bounded independently of the workload quota:
            // Windows can attach console helpers beyond ActiveProcessLimit.
            // Refusing that very observation would hide an attributable breach.
            if ids.count > MAX_JOB_PROCESSES || ids.assigned > MAX_JOB_PROCESSES {
                return Err(JobError::IncompleteMembership);
            }
            let mut members = Vec::new();
            let mut keys = Vec::new();
            members
                .try_reserve_exact(ids.count as usize)
                .map_err(|_| JobError::Allocation)?;
            keys.try_reserve_exact(ids.count as usize)
                .map_err(|_| JobError::Allocation)?;
            for pid in &ids.ids[..ids.count as usize] {
                let pid = u32::try_from(*pid).map_err(|_| JobError::InvalidCounter)?;
                let identity = self.member_identity(pid);
                if let Measurement::Available(key) = identity {
                    keys.push(key);
                }
                members.push(MemberObservation { pid, identity });
            }
            let samples = sampler.sample(&keys).map_err(JobError::Sampling)?;
            let after = accounting(self.handle())?;
            let limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
                query(self.handle(), JobObjectExtendedLimitInformation)?;
            let mut committed = 0_u64;
            let mut resident = 0_u64;
            let mut available = 0;
            for process in &samples.processes {
                if let Measurement::Available(value) = process.counters {
                    committed = committed
                        .checked_add(value.private_commit_bytes)
                        .ok_or(JobError::InvalidCounter)?;
                    resident = resident
                        .checked_add(value.resident_bytes)
                        .ok_or(JobError::InvalidCounter)?;
                    available += 1;
                }
            }
            let cpu = u64::try_from(after.TotalUserTime)
                .ok()
                .and_then(|user| {
                    u64::try_from(after.TotalKernelTime)
                        .ok()
                        .and_then(|kernel| user.checked_add(kernel))
                })
                .ok_or(JobError::InvalidCounter)?;
            Ok(JobSnapshot {
                active_processes: after.ActiveProcesses,
                total_processes: after.TotalProcesses,
                total_terminated_processes: after.TotalTerminatedProcesses,
                cpu_time_100ns: cpu,
                peak_job_commit_bytes: limits.PeakJobMemoryUsed as u64,
                observed_private_commit_bytes: committed,
                observed_resident_bytes: resident,
                membership_complete: before.ActiveProcesses == after.ActiveProcesses
                    && before.TotalProcesses == after.TotalProcesses
                    && ids.assigned == ids.count
                    && after.ActiveProcesses == ids.count
                    && available == ids.count as usize,
                members,
                processes: samples.processes,
            })
        }

        fn member_identity(&self, pid: u32) -> Measurement<ProcessKey> {
            let result = (|| {
                let key = WindowsSampler::identify(pid)?;
                let raw = unsafe {
                    OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                        0,
                        pid,
                    )
                };
                if raw.is_null() {
                    return Err(Unavailable::OsError(unsafe { GetLastError() }));
                }
                let process = Handle(raw);
                let mut member = 0;
                if unsafe { IsProcessInJob(process.0, self.handle().0, &mut member) } == 0 {
                    return Err(Unavailable::OsError(unsafe { GetLastError() }));
                }
                if member == 0 {
                    return Err(Unavailable::IdentityChanged);
                }
                let (mut creation, mut exit, mut kernel, mut user) = (
                    FILETIME::default(),
                    FILETIME::default(),
                    FILETIME::default(),
                    FILETIME::default(),
                );
                if unsafe {
                    GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user)
                } == 0
                {
                    return Err(Unavailable::OsError(unsafe { GetLastError() }));
                }
                let creation =
                    (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
                if creation != key.created_at_100ns {
                    return Err(Unavailable::IdentityChanged);
                }
                self.record_member(key, process)
                    .map_err(|_| Unavailable::InvalidCounter)?;
                Ok(key)
            })();
            match result {
                Ok(key) => Measurement::Available(key),
                Err(reason) => Measurement::Unavailable(reason),
            }
        }

        fn record_member(&self, key: ProcessKey, handle: Handle) -> Result<(), JobError> {
            let mut census = self
                .census
                .lock()
                .map_err(|_| JobError::RundownUnverified)?;
            if census.live.iter().any(|old| old.key == key) {
                return Ok(());
            }
            // Do not double count a handle opened before another observer reaped
            // this same exiting process. Only a presently live identity is new.
            match unsafe { WaitForSingleObject(handle.0, 0) } {
                WAIT_TIMEOUT => {}
                WAIT_OBJECT_0 => return Err(JobError::RundownUnverified),
                _ => return Err(os_error()),
            }
            census.reap_signaled()?;
            if census.live.len() >= MAX_JOB_PROCESSES as usize {
                return Err(JobError::IncompleteMembership);
            }
            census
                .live
                .try_reserve(1)
                .map_err(|_| JobError::Allocation)?;
            let total = census
                .verified_total
                .checked_add(1)
                .ok_or(JobError::InvalidCounter)?;
            census.live.push(TrackedProcess { key, handle });
            census.verified_total = total;
            Ok(())
        }

        fn observe_current_members(&self) -> Result<(), JobError> {
            let mut ids = MemberIds {
                assigned: 0,
                count: 0,
                ids: [0; MAX_JOB_PROCESSES as usize],
            };
            if unsafe {
                QueryInformationJobObject(
                    self.handle().0,
                    JobObjectBasicProcessIdList,
                    (&mut ids as *mut MemberIds).cast(),
                    size_of::<MemberIds>() as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(os_error());
            }
            if ids.count > MAX_JOB_PROCESSES || ids.assigned != ids.count {
                return Err(JobError::IncompleteMembership);
            }
            for pid in &ids.ids[..ids.count as usize] {
                let pid = u32::try_from(*pid).map_err(|_| JobError::InvalidCounter)?;
                // A disappearing identity is not accepted as coverage. The final
                // lifetime accounting comparison detects anything never tracked.
                let _ = self.member_identity(pid);
            }
            Ok(())
        }

        fn rundown_complete(&self) -> Result<bool, JobError> {
            let info = accounting(self.handle())?;
            let mut census = self
                .census
                .lock()
                .map_err(|_| JobError::RundownUnverified)?;
            census.reap_signaled()?;
            if self.assignments_sealed {
                // Query the SAME held job after successful explicit termination,
                // never a reopened name or NotFound result. No root can be newly
                // assigned; every observed exact handle must still be signaled.
                // Current native membership may be zero despite a historical
                // census gap caused by already-exited short-lived descendants.
                // https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information
                if u64::from(info.TotalProcesses) < census.verified_total {
                    return Err(JobError::RundownUnverified);
                }
                return Ok(sealed_rundown(
                    &info,
                    census.verified_total,
                    census.live.len(),
                    self.assignments_sealed,
                    self.termination_confirmed,
                ));
            }
            if info.ActiveProcesses == 0 && u64::from(info.TotalProcesses) != census.verified_total
            {
                // A finished but never observed member cannot be recovered by
                // polling the now-empty list. Retain its budget immediately.
                return Err(JobError::RundownUnverified);
            }
            Ok(census_proves_rundown(
                &info,
                census.verified_total,
                census.live.len(),
            ))
        }

        /// Closes launch admission irreversibly, even on error/timeout. An empty
        /// observation alone is not a release proof: close_after_empty must close
        /// the final owned handle before a reservation can be released.
        pub fn terminate_and_wait(
            &mut self,
            timeout: Duration,
        ) -> Result<CleanupOutcome, JobError> {
            if timeout > MAX_CLEANUP_WAIT {
                return Err(JobError::InvalidWait);
            }
            if self.phase == Phase::Empty {
                return Ok(CleanupOutcome::EmptyConfirmed);
            }
            self.phase = Phase::Retired;
            self.slot.set(SlotState::Retired);
            let started = Instant::now();
            if let Err(error) = self.observe_current_members() {
                // Still request termination of our own job, but never mint a
                // release proof from incomplete membership evidence.
                unsafe {
                    TerminateJobObject(self.handle().0, 1);
                }
                return Ok(CleanupOutcome::Retired { error: Some(error) });
            }
            self.termination_confirmed = unsafe { TerminateJobObject(self.handle().0, 1) } != 0;
            if !self.termination_confirmed {
                return Ok(CleanupOutcome::Retired {
                    error: Some(os_error()),
                });
            }
            loop {
                if let Err(error) = self.observe_current_members() {
                    return Ok(CleanupOutcome::Retired { error: Some(error) });
                }
                match self.rundown_complete() {
                    Ok(true) => {
                        self.phase = Phase::Empty;
                        return Ok(CleanupOutcome::EmptyConfirmed);
                    }
                    Ok(false) => {}
                    Err(error) => return Ok(CleanupOutcome::Retired { error: Some(error) }),
                }
                let remaining = timeout.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Ok(CleanupOutcome::Retired {
                        error: Some(JobError::RundownUnverified),
                    });
                }
                std::thread::sleep(remaining.min(Duration::from_millis(10)));
            }
        }

        #[allow(
            clippy::result_large_err,
            reason = "Preserve job ownership without allocating on the resource-pressure error path."
        )]
        pub fn close_after_empty(mut self) -> Result<ClosedEmptyJobProof, Self> {
            if self.phase != Phase::Empty {
                return Err(self);
            }
            // Exclusive ownership excludes live binding borrows. No subsequent
            // admission or assignment is exposed, and empty job has no live child.
            if !matches!(self.rundown_complete(), Ok(true)) {
                self.phase = Phase::Retired;
                return Err(self);
            }
            let handle = self.handle.take().expect("live job owns its handle");
            match handle.close() {
                Ok(()) => {
                    self.phase = Phase::Closed;
                    self.slot.set(SlotState::Free);
                    Ok(ClosedEmptyJobProof {
                        job_key: self.job_key,
                        owner: self.owner,
                        owner_windows_session_id: self.owner_windows_session_id,
                    })
                }
                Err(handle) => {
                    self.handle = Some(handle);
                    self.phase = Phase::Retired;
                    Err(self)
                }
            }
        }
    }

    impl Drop for WorkloadJob {
        fn drop(&mut self) {
            if self.phase == Phase::Closed {
                return;
            }
            self.slot.set(SlotState::Retired);
            // No retry loop or destructor wait. Only a confirmed empty AND closed
            // job frees the local slot; otherwise it remains persistently retired.
            let observed = self.observe_current_members().is_ok();
            self.termination_confirmed = unsafe { TerminateJobObject(self.handle().0, 1) } != 0;
            let confirmed = observed && matches!(self.rundown_complete(), Ok(true));
            if let Some(handle) = self.handle.take()
                && handle.close().is_ok()
                && confirmed
            {
                self.slot.set(SlotState::Free);
            }
        }
    }

    /// Bootstrap capability for a helper already atomically inside this job.
    /// QUERY/ASSIGN only: no termination, limit changes, cleanup or release proof.
    pub struct InheritedJobBinding {
        handle: Handle,
        handles: [HANDLE; 1],
        _pinned: PhantomPinned,
    }
    impl InheritedJobBinding {
        pub fn open(owned: OwnedJobKey) -> Result<Self, JobError> {
            let name = name(owned.job_key)?;
            if current_session_id()? != owned.owner_windows_session_id {
                return Err(JobError::OwnerSessionMismatch);
            }
            if WindowsSampler::identify(owned.owner.pid) != Ok(owned.owner) {
                return Err(JobError::OwnerUnknown);
            }
            let raw = unsafe { OpenJobObjectW(0x0001 | 0x0004, 0, name.as_ptr()) };
            if raw.is_null() {
                return Err(os_error());
            }
            let handle = Handle(raw);
            let mut member = 0;
            if unsafe { IsProcessInJob(GetCurrentProcess(), handle.0, &mut member) } == 0 {
                return Err(os_error());
            }
            if member == 0 {
                return Err(JobError::IncompleteMembership);
            }
            Ok(Self {
                handle,
                handles: [raw],
                _pinned: PhantomPinned,
            })
        }
        /// # Safety
        /// Same pinned-storage/list-lifetime and suspended-start requirements as
        /// StartupJobBinding::apply_to. This helper must never inherit this handle.
        pub unsafe fn apply_to(
            self: Pin<&Self>,
            list: LPPROC_THREAD_ATTRIBUTE_LIST,
        ) -> Result<(), JobError> {
            if list.is_null() {
                return Err(JobError::AdmissionClosed);
            }
            if unsafe {
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                    self.handles.as_ptr().cast(),
                    size_of::<HANDLE>(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } == 0
            {
                Err(os_error())
            } else {
                Ok(())
            }
        }
        pub fn verify_membership(
            &self,
            process: BorrowedHandle<'_>,
        ) -> Result<ProcessKey, JobError> {
            let raw = process.as_raw_handle() as HANDLE;
            let mut member = 0;
            if unsafe { IsProcessInJob(raw, self.handle.0, &mut member) } == 0 {
                return Err(os_error());
            }
            if member == 0 {
                return Err(JobError::IncompleteMembership);
            }
            let pid = unsafe { GetProcessId(raw) };
            let (mut creation, mut exit, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            if pid == 0
                || unsafe { GetProcessTimes(raw, &mut creation, &mut exit, &mut kernel, &mut user) }
                    == 0
            {
                return Err(os_error());
            }
            let key = ProcessKey {
                pid,
                created_at_100ns: (u64::from(creation.dwHighDateTime) << 32)
                    | u64::from(creation.dwLowDateTime),
            };
            if WindowsSampler::identify(pid) != Ok(key) {
                return Err(JobError::IncompleteMembership);
            }
            Ok(key)
        }
    }

    pub struct StartupJobBinding<'a> {
        job: &'a WorkloadJob,
        handles: [HANDLE; 1],
        _pinned: PhantomPinned,
    }
    impl StartupJobBinding<'_> {
        /// # Safety
        /// list must be a valid initialized exclusive attribute list with a free
        /// JOB_LIST slot, destroyed before this pinned guard. CreateProcess must
        /// use EXTENDED_STARTUPINFO_PRESENT + CREATE_SUSPENDED, keep the binding
        /// alive, and exclude the job handle from HANDLE_LIST/inheritance. Failure
        /// is terminal: no launch without JOB_LIST and no assign-after-run fallback.
        pub unsafe fn apply_to(
            self: Pin<&Self>,
            list: LPPROC_THREAD_ATTRIBUTE_LIST,
        ) -> Result<(), JobError> {
            if self.job.phase != Phase::Active {
                return Err(JobError::AdmissionClosed);
            }
            if list.is_null() {
                return Err(JobError::AdmissionClosed);
            }
            if unsafe {
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                    self.handles.as_ptr().cast(),
                    size_of::<HANDLE>(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } == 0
            {
                Err(os_error())
            } else {
                Ok(())
            }
        }

        /// Verify the newly created suspended process before a future launcher
        /// resumes it. This method never resumes or assigns a process.
        pub fn verify_membership(
            &self,
            process: BorrowedHandle<'_>,
        ) -> Result<ProcessKey, JobError> {
            let mut member = 0;
            if unsafe {
                IsProcessInJob(
                    process.as_raw_handle() as HANDLE,
                    self.job.handle().0,
                    &mut member,
                )
            } == 0
            {
                return Err(os_error());
            }
            if member == 0 {
                return Err(JobError::IncompleteMembership);
            }
            let pid = unsafe { GetProcessId(process.as_raw_handle() as HANDLE) };
            if pid == 0 {
                return Err(os_error());
            }
            match self.job.member_identity(pid) {
                Measurement::Available(key) => Ok(key),
                Measurement::Unavailable(_) => Err(JobError::IncompleteMembership),
            }
        }
    }

    pub enum ReconcileOutcome {
        ClosedEmpty(ClosedEmptyJobProof),
        Retired,
    }

    /// Exact owner death closes admission, but missing names and job counters do
    /// NOT prove process rundown. The owner-local handle census was lost in this
    /// crash, so the current inputs cannot produce a release proof. Keep capacity
    /// retired until a separately qualified recovery protocol proves termination.
    /// No object opened by name is ever terminated here.
    pub fn reconcile_dead_owner(
        job_key: [u8; 16],
        owner: ProcessKey,
        owner_windows_session_id: u32,
    ) -> Result<ReconcileOutcome, JobError> {
        name(job_key)?;
        if owner.pid == 0 || owner.created_at_100ns == 0 {
            return Err(JobError::OwnerUnknown);
        }
        if current_session_id()? != owner_windows_session_id {
            return Err(JobError::OwnerSessionMismatch);
        }
        match WindowsSampler::identify(owner.pid) {
            Ok(actual) if actual == owner => return Err(JobError::OwnerAlive),
            Ok(_) | Err(Unavailable::Missing | Unavailable::Terminated) => {}
            Err(_) => return Err(JobError::OwnerUnknown),
        }
        Ok(ReconcileOutcome::Retired)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn budget() -> JobBudget {
            JobBudget::new(16 * 1024 * 1024, 8 * 1024 * 1024, 2, 2000).unwrap()
        }
        fn key() -> [u8; 16] {
            let mut key = [0; 16];
            getrandom::fill(&mut key).unwrap();
            key
        }
        #[test]
        fn limits_restrict_only_the_new_empty_job_and_have_no_breakaway_flags() {
            let expected = extended(budget());
            assert_eq!(
                expected.BasicLimitInformation.LimitFlags,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                    | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                    | JOB_OBJECT_LIMIT_PROCESS_MEMORY
                    | JOB_OBJECT_LIMIT_JOB_MEMORY
            );
            assert_eq!(
                cpu_limit(budget()).ControlFlags,
                JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP
            );
            let slot = WorkloadSlot::new();
            let mut job = WorkloadJob::create(&slot, key(), budget()).unwrap();
            assert_eq!(slot.state(), SlotState::Active);
            assert_eq!(
                job.query(&WindowsSampler::new()).unwrap().active_processes,
                0
            );
            assert_eq!(
                job.terminate_and_wait(Duration::ZERO).unwrap(),
                CleanupOutcome::EmptyConfirmed
            );
            assert_eq!(slot.state(), SlotState::Retired);
            assert!(job.startup_binding().is_err());
            let expected_key = job.job_key();
            let proof = job.close_after_empty().ok().unwrap();
            assert_eq!(proof.job_key(), expected_key);
            assert_eq!(slot.state(), SlotState::Free);
        }
        /// An interactive job owns and cleans up its processes and nothing else: Windows
        /// must not refuse a child process, an allocation or CPU time for a Shinatama quota.
        #[test]
        fn advisory_job_keeps_ownership_without_process_memory_or_cpu_caps() {
            let budget = budget().with_advisory_limits();
            let slot = WorkloadSlot::new();
            let mut job = WorkloadJob::create(&slot, key(), budget).unwrap();
            let actual: JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
                query(job.handle(), JobObjectExtendedLimitInformation).unwrap();
            assert_eq!(
                actual.BasicLimitInformation.LimitFlags,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            );
            assert_eq!(actual.JobMemoryLimit, 0);
            assert_eq!(actual.ProcessMemoryLimit, 0);
            assert_eq!(actual.BasicLimitInformation.ActiveProcessLimit, 0);
            let cpu: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION =
                query(job.handle(), JobObjectCpuRateControlInformation).unwrap();
            assert_eq!(cpu.ControlFlags, 0);
            assert_eq!(job.budget(), budget);
            assert_eq!(
                job.terminate_and_wait(Duration::ZERO).unwrap(),
                CleanupOutcome::EmptyConfirmed
            );
            assert!(job.close_after_empty().is_ok());
            assert_eq!(slot.state(), SlotState::Free);
        }
        #[test]
        fn absent_job_name_after_last_handle_close_is_a_recovery_certificate() {
            let slot = WorkloadSlot::new();
            let job_key = key();
            let job = WorkloadJob::create(&slot, job_key, budget()).unwrap();
            drop(job);
            // Last handle closed: the named object is gone, so the group was
            // terminated by the OS through the pinned kill-on-close recipe.
            assert!(matches!(
                probe_job_recovery(job_key),
                Ok(JobRecoveryObservation::Absent)
            ));
            assert!(matches!(
                probe_job_recovery([0; 16]),
                Err(JobError::InvalidJobKey)
            ));
        }
        #[test]
        fn surviving_job_name_with_another_holder_blocks_recovery() {
            let slot = WorkloadSlot::new();
            let job_key = key();
            let job = WorkloadJob::create(&slot, job_key, budget()).unwrap();
            // This test keeps its own handle alive: recovery must not certify
            // a group while the name still has a live holder.
            assert!(matches!(
                probe_job_recovery(job_key),
                Err(JobError::RundownUnverified)
            ));
            drop(job);
            assert!(matches!(
                probe_job_recovery(job_key),
                Ok(JobRecoveryObservation::Absent)
            ));
        }
        #[test]
        fn existing_name_is_refused_before_limits_on_the_existing_job_can_change() {
            let first_slot = WorkloadSlot::new();
            let second_slot = WorkloadSlot::new();
            let job_key = key();
            let original = WorkloadJob::create(&first_slot, job_key, budget()).unwrap();
            assert!(matches!(
                WorkloadJob::create(
                    &second_slot,
                    job_key,
                    JobBudget::new(32 * 1024 * 1024, 16 * 1024 * 1024, 3, 4000).unwrap()
                ),
                Err(JobError::NameCollision)
            ));
            let actual: JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
                query(original.handle(), JobObjectExtendedLimitInformation).unwrap();
            assert_eq!(actual.JobMemoryLimit, budget().job_commit_bytes);
            assert_eq!(second_slot.state(), SlotState::Free);
        }
        #[test]
        fn unconfirmed_cleanup_retains_admission_and_cannot_be_reset_by_another_request() {
            let slot = WorkloadSlot::new();
            let mut job = WorkloadJob::create(&slot, key(), budget()).unwrap();
            // Deterministic model of a timeout/query failure, without hanging a
            // child or pressuring the machine. No empty proof has been observed.
            job.phase = Phase::Retired;
            slot.set(SlotState::Retired);
            assert!(job.startup_binding().is_err());
            assert!(matches!(
                WorkloadJob::create(&slot, key(), budget()),
                Err(JobError::SlotRetired)
            ));
            job = job.close_after_empty().err().unwrap();
            assert_eq!(slot.state(), SlotState::Retired);
            assert_eq!(
                job.terminate_and_wait(Duration::ZERO).unwrap(),
                CleanupOutcome::EmptyConfirmed
            );
            assert!(job.close_after_empty().is_ok());
            assert_eq!(slot.state(), SlotState::Free);
        }
        #[test]
        fn reconciliation_refuses_a_live_exact_owner_even_when_the_job_is_empty() {
            let slot = WorkloadSlot::new();
            let job = WorkloadJob::create(&slot, key(), budget()).unwrap();
            assert!(matches!(
                reconcile_dead_owner(job.job_key(), job.owner(), job.owner_windows_session_id()),
                Err(JobError::OwnerAlive)
            ));
            assert!(matches!(
                reconcile_dead_owner(
                    job.job_key(),
                    job.owner(),
                    job.owner_windows_session_id().wrapping_add(1)
                ),
                Err(JobError::OwnerSessionMismatch)
            ));
        }
        #[test]
        fn zero_active_count_without_complete_signaled_census_is_never_a_proof() {
            let info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION {
                ActiveProcesses: 0,
                TotalProcesses: 2,
                ..Default::default()
            };
            assert!(
                !census_proves_rundown(&info, 1, 0),
                "unobserved exited child leaves a lifetime gap"
            );
            assert!(
                !census_proves_rundown(&info, 2, 1),
                "job zero must not precede process-handle signaling"
            );
            assert!(
                !census_proves_rundown(&info, 3, 0),
                "counter mismatch fails closed in either direction"
            );
            assert!(census_proves_rundown(&info, 2, 0));
        }
        #[test]
        fn a_sealed_launcher_requires_native_termination_zero_members_and_signaled_handles() {
            let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION {
                ActiveProcesses: 0,
                TotalProcesses: 7,
                ..Default::default()
            };
            assert!(sealed_rundown(&info, 3, 0, true, true));
            assert!(!sealed_rundown(&info, 3, 0, false, true));
            assert!(!sealed_rundown(&info, 3, 0, true, false));
            assert!(!sealed_rundown(&info, 3, 1, true, true));
            info.ActiveProcesses = 1;
            assert!(!sealed_rundown(&info, 3, 0, true, true));
            info.ActiveProcesses = 0;
            assert!(!sealed_rundown(&info, 8, 0, true, true));
        }
        #[test]
        fn sealing_assignments_irreversibly_refuses_new_startup_bindings() {
            let slot = WorkloadSlot::new();
            let mut job = WorkloadJob::create(&slot, key(), budget()).unwrap();
            assert!(job.startup_binding().is_ok());
            job.seal_assignments().unwrap();
            assert!(matches!(
                job.startup_binding(),
                Err(JobError::AdmissionClosed)
            ));
            assert_eq!(
                job.terminate_and_wait(Duration::ZERO).unwrap(),
                CleanupOutcome::EmptyConfirmed
            );
            assert!(matches!(
                job.seal_assignments(),
                Err(JobError::AdmissionClosed)
            ));
            assert!(job.close_after_empty().is_ok());
            assert_eq!(slot.state(), SlotState::Free);
        }
        #[test]
        fn a_retained_census_gap_prevents_cleanup_close_and_replacement_even_for_an_empty_job() {
            let slot = WorkloadSlot::new();
            let mut job = WorkloadJob::create(&slot, key(), budget()).unwrap();
            // Inject a mismatch deterministically; a real missed descendant is
            // exercised by the native fixture. No child or memory pressure here.
            job.census.lock().unwrap().verified_total = 1;
            assert!(matches!(
                job.terminate_and_wait(Duration::ZERO).unwrap(),
                CleanupOutcome::Retired { .. }
            ));
            assert!(job.close_after_empty().is_err());
            assert_eq!(slot.state(), SlotState::Retired);
            assert!(matches!(
                WorkloadJob::create(&slot, key(), budget()),
                Err(JobError::SlotRetired)
            ));
        }
    }
    /// Recovery probe for ADR-0051: inspect the durable job name only after
    /// the exact host/shell identities were observed dead or PID-reused.
    /// Absence of the name is the certificate that the last handle closed and
    /// the pinned kill-on-close recipe already terminated the whole group.
    /// Presence requires terminating the group and proving rundown before any
    /// receipt exists; a surviving name afterwards is an explicit block.
    pub enum JobRecoveryObservation {
        Absent,
        TerminatedByRecovery,
    }
    pub fn probe_job_recovery(job_key: [u8; 16]) -> Result<JobRecoveryObservation, JobError> {
        let name = name(job_key)?;
        // QUERY | TERMINATE only: this probe never assigns, never changes limits
        // and never fabricates a receipt while the name survives.
        let raw = unsafe { OpenJobObjectW(0x0004 | 0x0008, 0, name.as_ptr()) };
        if raw.is_null() {
            let error = unsafe { GetLastError() };
            // ERROR_FILE_NOT_FOUND: no object with this name exists anymore.
            if error == 2 {
                return Ok(JobRecoveryObservation::Absent);
            }
            return Err(JobError::Os(error));
        }
        let handle = Handle(raw);
        if unsafe { TerminateJobObject(handle.0, 1) } == 0 {
            return Err(os_error());
        }
        let deadline = Instant::now() + MAX_CLEANUP_WAIT;
        loop {
            let info = accounting(&handle)?;
            if info.ActiveProcesses == 0 {
                break;
            }
            if Instant::now() >= deadline {
                return Err(JobError::RundownUnverified);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        handle.close().map_err(|_| JobError::CloseFailed)?;
        let raw = unsafe { OpenJobObjectW(0x0004, 0, name.as_ptr()) };
        if !raw.is_null() {
            unsafe { CloseHandle(raw) };
            // Some other holder still exists: the group is not certified gone.
            return Err(JobError::RundownUnverified);
        }
        let error = unsafe { GetLastError() };
        if error != 2 {
            return Err(JobError::Os(error));
        }
        Ok(JobRecoveryObservation::TerminatedByRecovery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advisory_limits_preserve_thresholds_and_are_explicit() {
        let hard = JobBudget::new(1024, 512, 2, 1500).unwrap();
        assert!(hard.limits_enforced());
        let advisory = hard.with_advisory_limits();
        assert!(!advisory.limits_enforced());
        assert_eq!(advisory.job_commit_bytes(), hard.job_commit_bytes());
        assert_eq!(advisory.process_commit_bytes(), hard.process_commit_bytes());
        assert_eq!(advisory.max_processes(), hard.max_processes());
        assert_eq!(
            advisory.cpu_hard_cap_basis_points(),
            hard.cpu_hard_cap_basis_points()
        );
        assert_ne!(advisory, hard);
    }
    #[test]
    fn rejects_zero_inverted_or_unbounded_policy_values() {
        for (job, process, count, cpu) in [
            (0, 0, 1, 1),
            (10, 11, 1, 1),
            (10, 1, 0, 1),
            (10, 1, 257, 1),
            (10, 1, 1, 0),
            (10, 1, 1, 10_001),
        ] {
            assert_eq!(
                JobBudget::new(job, process, count, cpu),
                Err(JobError::InvalidBudget)
            );
        }
        let budget = JobBudget::new(1024, 512, 2, 1500).unwrap();
        assert_eq!(
            (
                budget.job_commit_bytes(),
                budget.process_commit_bytes(),
                budget.max_processes(),
                budget.cpu_hard_cap_basis_points()
            ),
            (1024, 512, 2, 1500)
        );
    }
    #[test]
    fn retired_slot_never_automatically_becomes_free() {
        let slot = WorkloadSlot::new();
        slot.acquire().unwrap();
        assert_eq!(slot.acquire(), Err(JobError::SlotBusy));
        slot.set(SlotState::Retired);
        assert_eq!(slot.acquire(), Err(JobError::SlotRetired));
        assert_eq!(slot.clone().state(), SlotState::Retired);
    }
}
