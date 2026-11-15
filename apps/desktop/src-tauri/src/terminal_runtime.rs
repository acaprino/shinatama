//! Native terminal admission. All methods are called on bounded host/IPC workers.
#![cfg(windows)]
use crate::resource_governor::{GovernorReader, Phase};
use crate::storage::Services;
use anyhow::{Context, Result, anyhow, bail};
use shinatama_engine::native_launch::NativeTuiPermit;
use shinatama_engine::resources::{
    admission::{
        AdmissionCoordinator, AdmissionError, CleanupStatus, PrepareRequest, PreparedWorkload,
        QueueStatus,
    },
    job::{JobBudget, JobSnapshot, OwnedJobKey},
    monitor::ResourceMonitor,
    sampling::{Measurement, ProcessKey},
};
use shinatama_state::resource_reservations::{
    Budget, PolicyStamp, ReservationError, ReservationPolicy, ReservationSnapshot, ReservationState,
};
use shinatama_state::{database::Database, location::StorageLocation};
use shinatama_terminal::{
    TerminalDirective, TerminalHost, WindowsNativeAdmission, WindowsTerminalAdmission,
    WindowsTerminalConfig,
};
use std::{
    ffi::OsString,
    fs::File,
    os::windows::io::BorrowedHandle,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const LEASE_MS: i64 = 300_000;
/// Native workloads open at once: the proxy, every harness session and shell, and the
/// short helpers (probes, session reads). The ledger only counts them: memory, processes
/// and CPU of an interactive terminal are advisory, and every start is still gated on the
/// measured free RAM and commit (`check_start_headroom`).
const MAX_WORKLOADS: u8 = 32;
/// Sized for MAX_WORKLOADS at PER_SLOT, except CPU, which cannot exceed one machine.
/// The previous 4 GiB / 8 slots refused a third harness session plus a shell on any PC,
/// whatever its free memory ("Terminal admission was refused: Ledger(Capacity)").
const MAX_TOTAL: Budget = Budget {
    memory_bytes: 32 * GIB,
    processes: 1024,
    cpu_basis_points: 10_000,
    queue_bytes: 256 * MIB,
    queue_items: 65_536,
};
const PER_SLOT: Budget = Budget {
    memory_bytes: GIB,
    processes: 32,
    cpu_basis_points: 2500,
    queue_bytes: 8 * MIB,
    queue_items: 2048,
};
const TERMINAL: Budget = Budget {
    memory_bytes: 512 * MIB,
    processes: 8,
    // A shell has no CPU rate control (advisory); this is only its share of the ledger.
    cpu_basis_points: 250,
    queue_bytes: 4 * MIB,
    queue_items: 512,
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum AdmissionMode {
    Background,
    Interactive,
    // Helpers required by an explicit Open get lightweight startup admission,
    // while retaining their Windows memory limits.
    InteractiveSupport,
}
fn stamp() -> PolicyStamp {
    PolicyStamp {
        version: 1,
        id: "windows.native.v1".into(),
    }
}
fn policy() -> ReservationPolicy {
    ReservationPolicy {
        stamp: stamp(),
        aggregate_ceiling: MAX_TOTAL,
        per_slot_ceiling: PER_SLOT,
        max_slots: MAX_WORKLOADS,
        max_lease_ms: LEASE_MS as u32,
    }
}
/// A policy stored by an earlier version: 8 workloads and 4 GiB, the even older "half the
/// free memory at first launch" (between 1 and 4 GiB), or the 16 GiB one a build installed on
/// 2026-11-15 wrote. Its reservations keep it until they are released; then it is replaced by
/// the current one.
fn is_previous_policy(previous: &ReservationPolicy) -> bool {
    let ceiling = previous.aggregate_ceiling;
    previous.stamp == stamp()
        && previous.per_slot_ceiling == PER_SLOT
        && previous.max_slots == 8
        && previous.max_lease_ms == LEASE_MS as u32
        && ((GIB..=4 * GIB).contains(&ceiling.memory_bytes) || ceiling.memory_bytes == 16 * GIB)
        && ceiling.processes == 128
        && ceiling.cpu_basis_points == 8000
        && ceiling.queue_bytes == 32 * MIB
        && ceiling.queue_items == 8192
}
/// Names what is full instead of a bare `Ledger(Capacity)`: this is Shinatama's own count
/// of open workloads, not a shortage measured on Windows.
fn capacity_refusal(ledger: &ReservationSnapshot, budget: Budget) -> String {
    let policy = &ledger.policy;
    let limit = policy.aggregate_ceiling;
    let used = ledger.reserved_total().unwrap_or(limit);
    let over = |used: u64, request: u64, limit: u64| used.saturating_add(request) > limit;
    let mut full = Vec::new();
    if ledger.slots.len() >= usize::from(policy.max_slots) {
        full.push(format!(
            "{} of {} workloads",
            ledger.slots.len(),
            policy.max_slots
        ));
    }
    if over(used.memory_bytes, budget.memory_bytes, limit.memory_bytes) {
        full.push(format!(
            "memory {} of {} MiB",
            used.memory_bytes / MIB,
            limit.memory_bytes / MIB
        ));
    }
    if over(
        used.cpu_basis_points.into(),
        budget.cpu_basis_points.into(),
        limit.cpu_basis_points.into(),
    ) {
        full.push(format!(
            "CPU {}% of {}%",
            used.cpu_basis_points / 100,
            limit.cpu_basis_points / 100
        ));
    }
    if over(
        used.processes.into(),
        budget.processes.into(),
        limit.processes.into(),
    ) {
        full.push(format!(
            "processes {} of {}",
            used.processes, limit.processes
        ));
    }
    if over(used.queue_bytes, budget.queue_bytes, limit.queue_bytes)
        || over(
            used.queue_items.into(),
            budget.queue_items.into(),
            limit.queue_items.into(),
        )
    {
        full.push("output buffers".into());
    }
    if full.is_empty() {
        full.push("changed while starting".into());
    }
    format!(
        "Shinatama's budget for concurrent sessions is full ({}; {} open): close a session or terminal you no longer use",
        full.join(", "),
        ledger.slots.len()
    )
}
/// Releases the reservations whose owning Shinatama process is gone (exact pid and
/// creation time no longer match) and whose job is certified gone: either its name no
/// longer exists, which means kill-on-close already ran when the last handle closed, or
/// the recovery probe terminated what was left and saw it empty. A live owner, an owner
/// in another Windows session, or an owner that cannot be identified keeps its budget.
/// Any failure leaves the ledger as it was; the caller still applies its capacity check.
fn reclaim_orphaned_reservations(
    database: &Database,
    ledger: shinatama_state::resource_reservations::ReservationSnapshot,
) -> shinatama_state::resource_reservations::ReservationSnapshot {
    use shinatama_engine::resources::job::{JobRecoveryObservation, probe_job_recovery};
    use shinatama_engine::resources::sampling::{Unavailable, WindowsSampler};
    use shinatama_state::resource_reservations::{
        EmptyJobProof, OwnerObservation, ReconcileObservation,
    };
    let Ok(now) = now_ms() else { return ledger };
    let mut session = 0u32;
    let own_session = unsafe {
        windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId(
            std::process::id(),
            &mut session,
        )
    } != 0;
    if !own_session {
        return ledger;
    }
    let mut observations = Vec::new();
    for slot in &ledger.slots {
        if slot.owner.windows_session_id != session {
            continue;
        }
        let dead = match WindowsSampler::identify(slot.owner.pid) {
            Ok(actual) => {
                actual.pid != slot.owner.pid
                    || actual.created_at_100ns != slot.owner.created_at_100ns
            }
            Err(Unavailable::Missing | Unavailable::Terminated) => true,
            Err(_) => false,
        };
        if !dead {
            continue;
        }
        let Ok(key) = uuid::Uuid::parse_str(&slot.job_key) else {
            continue;
        };
        match probe_job_recovery(*key.as_bytes()) {
            Ok(JobRecoveryObservation::Absent | JobRecoveryObservation::TerminatedByRecovery) => {}
            _ => continue,
        }
        let Ok(proof) =
            EmptyJobProof::from_trusted_closed_empty_job(slot, &slot.owner, &slot.job_key, now)
        else {
            continue;
        };
        observations.push(ReconcileObservation {
            expected: slot.clone(),
            owner: OwnerObservation::DeadExact,
            observed_at_ms: now,
            closed_empty_job: Some(proof),
        });
    }
    if observations.is_empty() {
        return ledger;
    }
    match database.reconcile_reservations(&ledger.policy.stamp, &observations, now) {
        Ok(updated) => updated,
        Err(_) => ledger,
    }
}

fn now_ms() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

pub struct Runtime {
    admission: Arc<Admission>,
    configured: OnceLock<Result<(), String>>,
}
impl Runtime {
    /// Waits until `deadline` for the governor to admit background work: right after launch it
    /// is still calibrating (up to ~30 s), and under memory pressure it defers optional work.
    pub fn wait_for_admission_until(&self, deadline: std::time::Instant) -> Result<(), String> {
        loop {
            match self.admission.current_headroom(false) {
                Ok(_) => return Ok(()),
                Err(error) if std::time::Instant::now() >= deadline => {
                    return Err(format!("{error:#}"));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }
    pub fn wait_for_interactive_admission(&self) -> Result<(), String> {
        // Interactive starts need a fresh system memory sample, not governor
        // calibration or frontend responsiveness (the window may be minimized).
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match self.admission.current_headroom(true) {
                Ok(_) => return Ok(()),
                Err(error) if std::time::Instant::now() >= deadline => {
                    return Err(format!("{error:#}"));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }
    pub fn request_shutdown(&self) {
        self.admission.closing.store(true, Ordering::Release);
    }
    pub fn prepare_background_workload(
        &self,
        budget: Budget,
        process_commit_bytes: u64,
        lease_ms: i64,
    ) -> Result<PreparedWorkload, String> {
        self.admission
            .prepare_workload(
                budget,
                process_commit_bytes,
                lease_ms,
                AdmissionMode::Background,
            )
            .map_err(|error| format!("{error:#}"))
    }
    pub fn prepare_interactive_support_workload(
        &self,
        budget: Budget,
        process_commit_bytes: u64,
        lease_ms: i64,
    ) -> Result<PreparedWorkload, String> {
        self.admission
            .prepare_workload(
                budget,
                process_commit_bytes,
                lease_ms,
                AdmissionMode::InteractiveSupport,
            )
            .map_err(|error| format!("{error:#}"))
    }
    pub fn new(
        services: Arc<Services>,
        monitor: Option<Arc<ResourceMonitor>>,
        governor: Option<GovernorReader>,
    ) -> Self {
        Self {
            admission: Arc::new(Admission {
                services,
                monitor,
                governor,
                coordinator: OnceLock::new(),
                approved: Mutex::new(Vec::with_capacity(usize::from(MAX_WORKLOADS))),
                closing: AtomicBool::new(false),
            }),
            configured: OnceLock::new(),
        }
    }
    pub fn sidecar_provider(
        &self,
        lock: File,
        location: StorageLocation,
    ) -> Result<SidecarProvider, String> {
        let database = self.admission.services.load()?.store.clone();
        Ok(SidecarProvider {
            database,
            admission: self.admission.clone(),
            _lock: lock,
            _location: location,
        })
    }
    pub fn configure(&self, host: &TerminalHost) -> Result<(), String> {
        self.configured
            .get_or_init(|| {
                self.configure_host(host, Duration::from_secs(10), Duration::from_secs(3))
            })
            .clone()
    }
    pub fn configure_canary(&self, host: &TerminalHost) -> Result<(), String> {
        self.configure_host(host, Duration::from_secs(3), Duration::from_secs(2))
    }
    fn configure_host(
        &self,
        host: &TerminalHost,
        startup_timeout: Duration,
        cleanup_timeout: Duration,
    ) -> Result<(), String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let directory = executable
            .parent()
            .ok_or_else(|| "The terminal helper directory is unavailable.".to_string())?;
        let windows = std::env::var_os("SystemRoot")
            .ok_or_else(|| "Windows system directory is unavailable.".to_string())?;
        host.configure_windows(WindowsTerminalConfig {
            helper_executable: directory.join("shinatama-terminal-helper.exe"),
            shell_executable: std::path::PathBuf::from(windows)
                .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
            shell_arguments: [
                "-NoLogo",
                "-NoProfile",
                "-NoExit",
                "-Command",
                "Import-Module PSReadLine; Set-PSReadLineOption -HistorySaveStyle SaveNothing",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            shell_label: "PowerShell".into(),
            admission: self.admission.clone(),
            startup_timeout,
            cleanup_timeout,
        })
        .map_err(|e| format!("{e:#}"))?;
        host.configure_native_admission(Arc::new(NativeAdmission {
            shared: self.admission.clone(),
        }))
        .map_err(|e| format!("{e:#}"))
    }
}
struct Admission {
    services: Arc<Services>,
    monitor: Option<Arc<ResourceMonitor>>,
    governor: Option<GovernorReader>,
    coordinator: OnceLock<Result<AdmissionCoordinator, String>>,
    approved: Mutex<Vec<Approved>>,
    closing: AtomicBool,
}
#[derive(Clone, Copy)]
struct Approved {
    key: OwnedJobKey,
    native: JobBudget,
    queue_bytes: u64,
    queue_items: u32,
}
impl Approved {
    fn directive(&self, job: &JobSnapshot, queues: QueueStatus) -> TerminalDirective {
        if !job.membership_complete {
            return TerminalDirective::Continue;
        }
        // Memory, process count and CPU are advisory for an interactive terminal:
        // a coding agent's `npm ci` or test run takes dozens of processes. Only the
        // output queues, which bound Shinatama's own memory, stop the terminal.
        let process_cap =
            self.native.limits_enforced() && job.active_processes > self.native.max_processes();
        if process_cap
            || queues.current_bytes > self.queue_bytes
            || queues.current_items > self.queue_items
        {
            return TerminalDirective::StopResponsible;
        }
        TerminalDirective::Continue
    }
}

fn check_start_headroom(
    physical: u64,
    commit: u64,
    budget: Budget,
    ledger: &shinatama_state::resource_reservations::ReservationSnapshot,
    mode: AdmissionMode,
    margin: u64,
) -> Result<()> {
    if mode != AdmissionMode::Background {
        // These are small startup estimates, not lifetime limits. Available
        // system counters already include running terminals; charging their
        // theoretical maximum again incorrectly requires GiB of free memory.
        let startup = startup_allowance(budget);
        let pending: u64 = ledger
            .slots
            .iter()
            .filter(|slot| slot.state == ReservationState::Reserved)
            .map(|slot| startup_allowance(slot.budget))
            .sum();
        let required = startup + pending + margin;
        if physical < required || commit < required {
            bail!(
                "not enough free memory: {} MiB RAM and {} MiB allocatable are available, {} MiB are needed ({} MiB to start plus the {} MiB margin set in Settings > Memory safety). Running sessions are not affected",
                physical / MIB,
                commit / MIB,
                required / MIB,
                (startup + pending) / MIB,
                margin / MIB
            );
        }
        return Ok(());
    }
    let required = ledger
        .reserved_total()?
        .memory_bytes
        .checked_add(budget.memory_bytes)
        .context("Resource capacity overflow.")?;
    if physical < required.saturating_add(256 * MIB) || commit < required.saturating_add(512 * MIB)
    {
        bail!(
            "System headroom cannot cover another terminal; existing sessions keep their budgets."
        );
    }
    Ok(())
}

fn startup_allowance(budget: Budget) -> u64 {
    if budget.memory_bytes <= TERMINAL.memory_bytes {
        64 * MIB
    } else {
        128 * MIB
    }
}
impl Admission {
    fn prepare_registered(
        &self,
        budget: Budget,
        process_commit_bytes: u64,
    ) -> Result<PreparedWorkload> {
        let mut workload = self.prepare_workload(
            budget,
            process_commit_bytes,
            15_000,
            AdmissionMode::Interactive,
        )?;
        let registered = match self.approved.try_lock() {
            Ok(mut approved) if approved.len() < usize::from(MAX_WORKLOADS) => {
                let native = workload
                    .applied_budget()
                    .context("Native terminal limits are unavailable.")?;
                let reserved = workload.ticket().budget;
                approved.push(Approved {
                    key: workload.owned_key(),
                    native,
                    queue_bytes: reserved.queue_bytes,
                    queue_items: reserved.queue_items,
                });
                true
            }
            _ => false,
        };
        if !registered {
            if let Ok(state) = self.services.load() {
                let _ = workload.finish_cleanup(&state.store, Duration::ZERO);
            }
            bail!("Terminal ownership registration is unavailable; retry explicitly.");
        }
        Ok(workload)
    }
    fn current_headroom(&self, interactive: bool) -> Result<(u64, u64)> {
        if self.closing.load(Ordering::Acquire) {
            bail!("This application is closing; new starts are disabled.");
        }
        if !crate::memory_settings::current().enabled {
            // Memory safety is off in Settings: no start waits for, or is refused on,
            // memory measurements. The workload count in the ledger still applies.
            return Ok((u64::MAX, u64::MAX));
        }
        if !interactive {
            let decision = self
                .governor
                .as_ref()
                .context("Resource control is unavailable; new terminals are deferred.")?
                .status()
                .map_err(|_| anyhow!("Resource control is busy; retry explicitly."))?;
            if decision.phase != Phase::Running || decision.defer_new_starts {
                bail!(
                    "New background work is waiting for healthy resource measurements. Retry when Resources is available."
                );
            }
        }
        let monitor = self
            .monitor
            .as_ref()
            .context("Resource monitor is unavailable; new terminals are deferred.")?;
        let status = monitor.status();
        let frame = monitor
            .current()
            .map_err(|_| anyhow!("Resource measurements are busy; retry explicitly."))?
            .context("memory measurements are not ready yet")?;
        if (!interactive && !frame.complete())
            || !matches!(
                status.phase,
                shinatama_engine::resources::monitor::WorkerPhase::Sampling
                    | shinatama_engine::resources::monitor::WorkerPhase::Waiting
            )
            || status
                .observed_at_ms
                .checked_sub(frame.completed_at_ms)
                .is_none_or(|age| age > 3000)
        {
            bail!(
                "memory measurements are incomplete or stale (Settings > Memory safety can turn this check off)"
            );
        }
        let aggregate = frame
            .aggregate
            .as_ref()
            .context("Resource counters are unavailable.")?;
        if status
            .observed_at_ms
            .checked_sub(aggregate.at_ms)
            .is_none_or(|age| age > 3000)
        {
            bail!(
                "system memory measurements are stale (Settings > Memory safety can turn this check off)"
            );
        }
        let physical = match aggregate.system {
            Measurement::Available(system) => system.physical_available_bytes,
            _ => bail!("System memory measurements are unavailable."),
        };
        let commit = aggregate
            .remaining_commit_bytes
            .context("System commit measurements are unavailable.")?;
        Ok((physical, commit))
    }
    fn prepare_workload(
        &self,
        budget: Budget,
        process_commit_bytes: u64,
        lease_ms: i64,
        mode: AdmissionMode,
    ) -> Result<PreparedWorkload> {
        if budget.memory_bytes > PER_SLOT.memory_bytes
            || budget.processes > PER_SLOT.processes
            || budget.cpu_basis_points > PER_SLOT.cpu_basis_points
            || budget.queue_bytes > PER_SLOT.queue_bytes
            || budget.queue_items > PER_SLOT.queue_items
            || process_commit_bytes > budget.memory_bytes
            || lease_ms <= 0
            || lease_ms > LEASE_MS
        {
            bail!("Native workload profile exceeds the shared safety policy.");
        }
        let (physical, commit) = self.current_headroom(mode != AdmissionMode::Background)?;
        let state = self.services.load().map_err(|e| anyhow!(e))?;
        let database = &state.store;
        let ledger = match database.resource_reservations()? {
            Some(ledger) => ledger,
            None => {
                // The ledger bounds concurrent profiles; it does not allocate
                // their maximum memory or require it all to be currently free.
                database.initialize_resource_policy(&policy())?
            }
        };
        // A Shinatama that crashed or was killed never released its reservations, and
        // nothing else does: after a few such exits every Open was refused for capacity.
        let mut ledger = reclaim_orphaned_reservations(database, ledger);
        if ledger.policy != policy() {
            if !is_previous_policy(&ledger.policy) {
                bail!("The shared resource policy differs; no limit was raised.");
            }
            if ledger.slots.is_empty() {
                // Replace an earlier version's smaller budget only after every previous
                // reservation has been released; never change an active job's budget.
                // Until then the reservations made under it keep its lower limits.
                let previous = ledger.policy.clone();
                ledger = database.replace_empty_resource_policy(
                    &previous,
                    ledger.revision,
                    &policy(),
                )?;
            }
        }
        let policy = &ledger.policy;
        let margin = u64::from(crate::memory_settings::current().margin_mib) * MIB;
        check_start_headroom(physical, commit, budget, &ledger, mode, margin)?;
        let coordinator = self
            .coordinator
            .get_or_init(|| {
                AdmissionCoordinator::new(usize::from(MAX_WORKLOADS)).map_err(|e| format!("{e:?}"))
            })
            .as_ref()
            .map_err(|e| anyhow!(e.clone()))?;
        let now = now_ms()?;
        let prepare = if mode == AdmissionMode::Interactive {
            AdmissionCoordinator::prepare_interactive
        } else {
            AdmissionCoordinator::prepare
        };
        let workload = prepare(
            coordinator,
            database,
            &policy.stamp,
            ledger.revision,
            PrepareRequest {
                budget: budget,
                process_commit_bytes,
                now_ms: now,
                lease_expires_at_ms: now + lease_ms,
            },
        )
        .map_err(|e| match e {
            AdmissionError::Ledger(ReservationError::Capacity) => {
                anyhow!(capacity_refusal(&ledger, budget))
            }
            e => anyhow!("Terminal admission was refused: {e:?}"),
        })?;
        Ok(workload)
    }
}
impl WindowsTerminalAdmission for Admission {
    fn prepare(&self) -> Result<PreparedWorkload> {
        self.prepare_registered(TERMINAL, 256 * MIB)
    }
    fn activate(
        &self,
        workload: &mut PreparedWorkload,
        process: BorrowedHandle<'_>,
    ) -> Result<ProcessKey> {
        let state = self.services.load().map_err(|e| anyhow!(e))?;
        let now = now_ms()?;
        workload
            .activate_verified(&state.store, process, now, now + LEASE_MS)
            .map_err(|e| anyhow!("Terminal ownership could not be activated: {e:?}"))
    }
    fn cleanup(&self, workload: &mut PreparedWorkload, timeout: Duration) -> Result<CleanupStatus> {
        let state = self.services.load().map_err(|e| anyhow!(e))?;
        let result = workload
            .finish_cleanup(&state.store, timeout)
            .map_err(|e| anyhow!("Terminal cleanup remains reserved: {e:?}"))?;
        if matches!(result, CleanupStatus::Released { .. }) {
            if let Ok(mut approved) = self.approved.lock() {
                approved.retain(|item| item.key != workload.owned_key());
            }
        }
        Ok(result)
    }
    fn observe(
        &self,
        owned: OwnedJobKey,
        job: &JobSnapshot,
        queues: QueueStatus,
    ) -> Result<TerminalDirective> {
        let approved = match self.approved.try_lock() {
            Ok(approved) => approved,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(TerminalDirective::Continue),
            Err(std::sync::TryLockError::Poisoned(_)) => bail!(
                "Terminal ownership observation is unavailable; native limits remain applied."
            ),
        };
        let record = approved
            .iter()
            .find(|item| item.key == owned)
            .context("Terminal resource identity is not registered; no process was selected.")?;
        Ok(record.directive(job, queues))
    }
}

struct NativeAdmission {
    shared: Arc<Admission>,
}
impl WindowsNativeAdmission for NativeAdmission {
    fn for_launch(&self, permit: &NativeTuiPermit) -> Result<Arc<dyn WindowsTerminalAdmission>> {
        let summary = permit.summary();
        let state = self
            .shared
            .services
            .load()
            .map_err(|error| anyhow!(error))?;
        if !permit.is_trial() {
            let root = state
                .store
                .store_root(&summary.store_root_id)?
                .context("The agent store registration is unavailable.")?;
            if !root.available
                || root.config_revision != summary.store_config_revision
                || root.harness != summary.agent.as_str()
                || root.access != shinatama_state::records::RootAccess::ReadWrite
            {
                bail!("The agent store registration changed; check the agent before retrying.");
            }
        }
        // Only an engine-issued owned permit supplies these values. The same
        // measured-headroom and immutable aggregate ledger checks run at start.
        Ok(Arc::new(ProfileAdmission {
            shared: self.shared.clone(),
            budget: permit.profile().budget(),
            process_commit_bytes: permit.profile().process_commit_bytes(),
        }))
    }
}
struct ProfileAdmission {
    shared: Arc<Admission>,
    budget: Budget,
    process_commit_bytes: u64,
}
impl WindowsTerminalAdmission for ProfileAdmission {
    fn prepare(&self) -> Result<PreparedWorkload> {
        self.shared
            .prepare_registered(self.budget, self.process_commit_bytes)
    }
    fn activate(
        &self,
        workload: &mut PreparedWorkload,
        process: BorrowedHandle<'_>,
    ) -> Result<ProcessKey> {
        self.shared.activate(workload, process)
    }
    fn cleanup(&self, workload: &mut PreparedWorkload, timeout: Duration) -> Result<CleanupStatus> {
        self.shared.cleanup(workload, timeout)
    }
    fn observe(
        &self,
        owned: OwnedJobKey,
        job: &JobSnapshot,
        queues: QueueStatus,
    ) -> Result<TerminalDirective> {
        self.shared.observe(owned, job, queues)
    }
}

/// The owner lock and C01 grant live with the retained supervisor worker.
pub struct SidecarProvider {
    database: Arc<Database>,
    admission: Arc<Admission>,
    _lock: File,
    _location: StorageLocation,
}
impl shinatama_engine::sidecar_supervisor::WorkloadProvider for SidecarProvider {
    fn database(&self) -> &Database {
        &self.database
    }
    fn prepare(
        &mut self,
        now: i64,
        lease_until: i64,
    ) -> Result<PreparedWorkload, shinatama_engine::sidecar_supervisor::AdmissionUnavailable> {
        let lease = lease_until
            .checked_sub(now)
            .ok_or(shinatama_engine::sidecar_supervisor::AdmissionUnavailable)?;
        self.admission
            .prepare_workload(
                Budget {
                    memory_bytes: GIB,
                    processes: 8,
                    cpu_basis_points: 2000,
                    queue_bytes: 8 * MIB,
                    queue_items: 1024,
                },
                768 * MIB,
                lease,
                AdmissionMode::InteractiveSupport,
            )
            .map_err(|_| shinatama_engine::sidecar_supervisor::AdmissionUnavailable)
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;
    use shinatama_engine::resources::sampling::{ProcessCounters, ProcessSample};
    use shinatama_state::resource_reservations::{
        ReservationOwner, ReservationSnapshot, ReservationState, ReservationTicket,
    };

    fn ledger() -> ReservationSnapshot {
        ReservationSnapshot {
            format_version: 1,
            revision: 1,
            policy: ReservationPolicy {
                stamp: stamp(),
                aggregate_ceiling: MAX_TOTAL,
                per_slot_ceiling: PER_SLOT,
                max_slots: MAX_WORKLOADS,
                max_lease_ms: LEASE_MS as u32,
            },
            slots: vec![],
        }
    }

    fn slot(state: ReservationState) -> ReservationTicket {
        ReservationTicket {
            id: uuid::Uuid::new_v4().to_string(),
            job_key: uuid::Uuid::new_v4().to_string(),
            owner: ReservationOwner {
                pid: 1,
                created_at_100ns: 1,
                windows_session_id: 1,
                nonce: uuid::Uuid::new_v4().to_string(),
            },
            budget: PER_SLOT,
            state,
            lease_expires_at_ms: LEASE_MS,
            revision: 1,
        }
    }

    #[test]
    fn current_policy_is_valid_and_admits_many_sessions_with_shells() {
        let policy = policy();
        assert!(policy.per_slot_ceiling.memory_bytes <= policy.aggregate_ceiling.memory_bytes);
        assert!(policy.aggregate_ceiling.cpu_basis_points <= 10_000);
        assert!(usize::from(policy.max_slots) <= shinatama_state::resource_reservations::MAX_SLOTS);
        // The proxy (2000), six harness sessions (1000 each), two shells and a session read.
        let cpu = 2000 + 6 * 1000 + 2 * u32::from(TERMINAL.cpu_basis_points) + 1000;
        assert!(cpu <= u32::from(MAX_TOTAL.cpu_basis_points));
        let memory = GIB + 6 * GIB + 2 * TERMINAL.memory_bytes + GIB;
        assert!(memory <= MAX_TOTAL.memory_bytes);
    }

    #[test]
    fn earlier_policies_are_recognized_for_replacement_and_others_are_not() {
        let mut previous = policy();
        previous.max_slots = 8;
        previous.aggregate_ceiling = Budget {
            memory_bytes: 4 * GIB,
            processes: 128,
            cpu_basis_points: 8000,
            queue_bytes: 32 * MIB,
            queue_items: 8192,
        };
        assert!(is_previous_policy(&previous));
        previous.aggregate_ceiling.memory_bytes = 3 * GIB;
        assert!(
            is_previous_policy(&previous),
            "half the free memory at first launch"
        );
        previous.aggregate_ceiling.memory_bytes = 16 * GIB;
        assert!(is_previous_policy(&previous), "the 16 GiB policy of the build installed on 2026-11-15");
        previous.aggregate_ceiling.memory_bytes = 5 * GIB;
        assert!(!is_previous_policy(&previous));
        assert!(!is_previous_policy(&policy()));
        let mut foreign = policy();
        foreign.stamp.id = "other".into();
        assert!(!is_previous_policy(&foreign));
    }

    #[test]
    fn capacity_refusal_names_what_is_full_without_blaming_windows() {
        let mut ledger = ledger();
        ledger.policy.max_slots = 2;
        ledger.slots = vec![
            slot(ReservationState::Active),
            slot(ReservationState::Active),
        ];
        let message = capacity_refusal(&ledger, TERMINAL);
        assert!(message.contains("2 of 2 workloads"), "{message}");
        assert!(!message.contains("Ledger("), "{message}");
        assert!(!message.contains("Windows"), "{message}");
    }

    #[test]
    fn shell_can_start_with_256_mib_available_without_reserving_gigabytes() {
        assert!(
            check_start_headroom(
                256 * MIB,
                256 * MIB,
                TERMINAL,
                &ledger(),
                AdmissionMode::Interactive,
                128 * MIB,
            )
            .is_ok()
        );
    }

    #[test]
    fn active_terminal_budgets_are_not_subtracted_again_from_available_memory() {
        let mut ledger = ledger();
        ledger.slots = vec![
            slot(ReservationState::Active),
            slot(ReservationState::Active),
        ];
        assert!(
            check_start_headroom(
                256 * MIB,
                256 * MIB,
                TERMINAL,
                &ledger,
                AdmissionMode::Interactive,
                128 * MIB,
            )
            .is_ok()
        );
    }

    #[test]
    fn low_ram_or_commit_blocks_only_the_new_start() {
        for (physical, commit) in [(64 * MIB, GIB), (GIB, 64 * MIB)] {
            assert!(
                check_start_headroom(
                    physical,
                    commit,
                    TERMINAL,
                    &ledger(),
                    AdmissionMode::Interactive,
                    128 * MIB,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn pending_launches_keep_a_small_startup_allowance() {
        let mut ledger = ledger();
        ledger.slots.push(slot(ReservationState::Reserved));
        assert!(
            check_start_headroom(
                256 * MIB,
                256 * MIB,
                TERMINAL,
                &ledger,
                AdmissionMode::Interactive,
                128 * MIB,
            )
            .is_err()
        );
        assert!(
            check_start_headroom(
                512 * MIB,
                512 * MIB,
                TERMINAL,
                &ledger,
                AdmissionMode::Interactive,
                128 * MIB,
            )
            .is_ok()
        );
    }

    #[test]
    fn the_margin_from_settings_is_what_a_start_keeps_free() {
        let free = 512 * MIB;
        let start = |margin| {
            check_start_headroom(
                free,
                free,
                TERMINAL,
                &ledger(),
                AdmissionMode::Interactive,
                margin,
            )
        };
        assert!(start(128 * MIB).is_ok());
        assert!(start(0).is_ok());
        let refused = start(GIB).unwrap_err().to_string();
        assert!(refused.contains("Memory safety"), "{refused}");
        assert!(refused.contains("1024 MiB margin"), "{refused}");
    }

    #[test]
    fn background_workloads_keep_their_full_admission_budget() {
        assert!(
            check_start_headroom(
                256 * MIB,
                256 * MIB,
                PER_SLOT,
                &ledger(),
                AdmissionMode::Background,
                128 * MIB,
            )
            .is_err()
        );
    }

    #[test]
    fn native_startup_services_do_not_require_gigabytes_or_recharge_active_memory() {
        let mut ledger = ledger();
        ledger.slots = vec![
            slot(ReservationState::Active),
            slot(ReservationState::Active),
        ];
        assert!(
            check_start_headroom(
                256 * MIB,
                256 * MIB,
                PER_SLOT,
                &ledger,
                AdmissionMode::InteractiveSupport,
                128 * MIB,
            )
            .is_ok()
        );
    }

    #[test]
    fn interactive_memory_growth_never_requests_a_stop() {
        let key = ProcessKey {
            pid: 1,
            created_at_100ns: 1,
        };
        let record = Approved {
            key: OwnedJobKey {
                job_key: [1; 16],
                owner: key,
                owner_windows_session_id: 1,
            },
            native: JobBudget::new(512 * MIB, 256 * MIB, 8, 1000).unwrap(),
            queue_bytes: TERMINAL.queue_bytes,
            queue_items: TERMINAL.queue_items,
        };
        let mut job = JobSnapshot {
            active_processes: 1,
            total_processes: 1,
            total_terminated_processes: 0,
            cpu_time_100ns: 0,
            peak_job_commit_bytes: 2 * GIB,
            observed_private_commit_bytes: 2 * GIB,
            observed_resident_bytes: GIB,
            membership_complete: true,
            members: vec![],
            processes: vec![ProcessSample {
                key,
                counters: Measurement::Available(ProcessCounters {
                    private_commit_bytes: 2 * GIB,
                    resident_bytes: GIB,
                    cpu_time_100ns: 0,
                }),
            }],
        };
        let queues = QueueStatus {
            closed: false,
            current_bytes: 0,
            current_items: 0,
            peak_bytes: 0,
            peak_items: 0,
            refusals: 0,
        };
        assert!(matches!(
            record.directive(&job, queues),
            TerminalDirective::Continue
        ));
        // Unrelated process/queue containment still works.
        job.active_processes = 9;
        assert!(matches!(
            record.directive(&job, queues),
            TerminalDirective::StopResponsible
        ));
    }

    /// A Claude Code session running `npm ci` was stopped mid-turn: its tool children took
    /// the job past 16 processes. The process count of an interactive session is advisory.
    #[test]
    fn interactive_process_count_never_requests_a_stop_but_queues_still_do() {
        let key = ProcessKey {
            pid: 1,
            created_at_100ns: 1,
        };
        let record = Approved {
            key: OwnedJobKey {
                job_key: [1; 16],
                owner: key,
                owner_windows_session_id: 1,
            },
            native: JobBudget::new(GIB, 768 * MIB, 16, 1000)
                .unwrap()
                .with_advisory_limits(),
            queue_bytes: TERMINAL.queue_bytes,
            queue_items: TERMINAL.queue_items,
        };
        let mut job = JobSnapshot {
            active_processes: 40,
            total_processes: 120,
            total_terminated_processes: 80,
            cpu_time_100ns: 0,
            peak_job_commit_bytes: GIB,
            observed_private_commit_bytes: GIB,
            observed_resident_bytes: GIB,
            membership_complete: true,
            members: vec![],
            processes: vec![],
        };
        let mut queues = QueueStatus {
            closed: false,
            current_bytes: 0,
            current_items: 0,
            peak_bytes: 0,
            peak_items: 0,
            refusals: 0,
        };
        assert!(matches!(
            record.directive(&job, queues),
            TerminalDirective::Continue
        ));
        job.active_processes = 1;
        queues.current_items = TERMINAL.queue_items + 1;
        assert!(matches!(
            record.directive(&job, queues),
            TerminalDirective::StopResponsible
        ));
    }
}
