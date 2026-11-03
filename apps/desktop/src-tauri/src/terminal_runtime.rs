//! Native terminal admission. All methods are called on bounded host/IPC workers.
#![cfg(windows)]
use crate::resource_governor::{GovernorReader, Phase};
use crate::storage::Services;
use anyhow::{Context, Result, anyhow, bail};
use shinatama_engine::native_launch::NativeTuiPermit;
use shinatama_engine::resources::{
    admission::{
        AdmissionCoordinator, CleanupStatus, PrepareRequest, PreparedWorkload, QueueStatus,
    },
    job::{JobBudget, JobSnapshot, OwnedJobKey},
    monitor::ResourceMonitor,
    sampling::{Measurement, ProcessKey},
};
use shinatama_state::resource_reservations::{Budget, PolicyStamp, ReservationPolicy};
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
const MAX_TOTAL: Budget = Budget {
    memory_bytes: 4 * GIB,
    processes: 128,
    cpu_basis_points: 8000,
    queue_bytes: 32 * MIB,
    queue_items: 8192,
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
    cpu_basis_points: 1000,
    queue_bytes: 4 * MIB,
    queue_items: 512,
};
fn stamp() -> PolicyStamp {
    PolicyStamp {
        version: 1,
        id: "windows.native.v1".into(),
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
    pub fn wait_for_admission(&self) -> Result<(), String> {
        // Startup needs two complete samples plus the governor's stable window.
        // Wait for that observation; resource thresholds and limits stay unchanged.
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            match self.admission.current_headroom() {
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
            .prepare_workload(budget, process_commit_bytes, lease_ms)
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
                approved: Mutex::new(Vec::with_capacity(8)),
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
impl Admission {
    fn prepare_registered(
        &self,
        budget: Budget,
        process_commit_bytes: u64,
    ) -> Result<PreparedWorkload> {
        let mut workload = self.prepare_workload(budget, process_commit_bytes, 15_000)?;
        let registered = match self.approved.try_lock() {
            Ok(mut approved) if approved.len() < 8 => {
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
    fn current_headroom(&self) -> Result<(u64, u64)> {
        if self.closing.load(Ordering::Acquire) {
            bail!("This application is closing; new starts are disabled.");
        }
        let decision = self
            .governor
            .as_ref()
            .context("Resource control is unavailable; new terminals are deferred.")?
            .status()
            .map_err(|_| anyhow!("Resource control is busy; retry explicitly."))?;
        if decision.phase != Phase::Running || decision.defer_new_starts {
            bail!(
                "New terminals are waiting for healthy resource measurements. Retry when Resources is available."
            );
        }
        let monitor = self
            .monitor
            .as_ref()
            .context("Resource monitor is unavailable; new terminals are deferred.")?;
        let status = monitor.status();
        let frame = monitor
            .current()
            .map_err(|_| anyhow!("Resource measurements are busy; retry explicitly."))?
            .context("Resource measurements are not ready.")?;
        if !frame.complete()
            || status
                .observed_at_ms
                .checked_sub(frame.completed_at_ms)
                .is_none_or(|age| age > 3000)
        {
            bail!("Resource measurements are incomplete or stale; new terminals are deferred.");
        }
        let aggregate = frame
            .aggregate
            .as_ref()
            .context("Resource counters are unavailable.")?;
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
        let (physical, commit) = self.current_headroom()?;
        let state = self.services.load().map_err(|e| anyhow!(e))?;
        let database = &state.store;
        let ledger = match database.resource_reservations()? {
            Some(ledger) => ledger,
            None => {
                let memory = (physical / 2).min(commit / 2).min(MAX_TOTAL.memory_bytes);
                if memory < PER_SLOT.memory_bytes {
                    bail!("Available system headroom is too small for the native runtime.");
                }
                database.initialize_resource_policy(&ReservationPolicy {
                    stamp: stamp(),
                    aggregate_ceiling: Budget {
                        memory_bytes: memory,
                        ..MAX_TOTAL
                    },
                    per_slot_ceiling: PER_SLOT,
                    max_slots: 8,
                    max_lease_ms: LEASE_MS as u32,
                })?
            }
        };
        let policy = &ledger.policy;
        if policy.stamp != stamp()
            || policy.per_slot_ceiling != PER_SLOT
            || policy.max_slots != 8
            || policy.max_lease_ms != LEASE_MS as u32
            || policy.aggregate_ceiling.memory_bytes > MAX_TOTAL.memory_bytes
            || policy.aggregate_ceiling.memory_bytes < GIB
            || policy.aggregate_ceiling.processes != MAX_TOTAL.processes
            || policy.aggregate_ceiling.cpu_basis_points != MAX_TOTAL.cpu_basis_points
            || policy.aggregate_ceiling.queue_bytes != MAX_TOTAL.queue_bytes
            || policy.aggregate_ceiling.queue_items != MAX_TOTAL.queue_items
        {
            bail!("The shared resource policy differs; no limit was raised.");
        }
        let reserved = ledger.reserved_total()?.memory_bytes;
        let required = reserved
            .checked_add(budget.memory_bytes)
            .context("Resource capacity overflow.")?;
        if physical < required.saturating_add(512 * MIB) || commit < required.saturating_add(GIB) {
            bail!(
                "System headroom cannot cover another terminal; existing sessions keep their budgets."
            );
        }
        let coordinator = self
            .coordinator
            .get_or_init(|| AdmissionCoordinator::new(8).map_err(|e| format!("{e:?}")))
            .as_ref()
            .map_err(|e| anyhow!(e.clone()))?;
        let now = now_ms()?;
        let workload = coordinator
            .prepare(
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
            .map_err(|e| anyhow!("Terminal admission was refused: {e:?}"))?;
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
        if !job.membership_complete {
            return Ok(TerminalDirective::Continue);
        }
        // Windows console helpers can appear above ActiveProcessLimit. Keep the
        // raw physical census and stop this exact registered job on overshoot.
        // No external pressure, PID ranking or unrelated job enters this choice.
        let process_memory_exceeded = job.processes.iter().any(|process|
            matches!(process.counters, Measurement::Available(counters) if counters.private_commit_bytes > record.native.process_commit_bytes()));
        if job.active_processes > record.native.max_processes()
            || job.observed_private_commit_bytes > record.native.job_commit_bytes()
            || process_memory_exceeded
            || queues.current_bytes > record.queue_bytes
            || queues.current_items > record.queue_items
        {
            return Ok(TerminalDirective::StopResponsible);
        }
        Ok(TerminalDirective::Continue)
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
            )
            .map_err(|_| shinatama_engine::sidecar_supervisor::AdmissionUnavailable)
    }
}
