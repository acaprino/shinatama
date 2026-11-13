//! Finite optional maintenance for Shinatama-owned state, never agent or sidecar stores.
//!
//! Run on a dedicated idle worker connection, with no existing progress handler.
//! The cooperative deadline bounds SQLite VM work, not blocking filesystem calls.

use crate::database::{DatabaseError, Result};
use crate::location::StorageLocation;
use crate::schema;
use rusqlite::{
    Connection, ErrorCode, MAIN_DB, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

const LEASE_KEY: &str = "maintenance.lease.v1";
const STATE_KEY: &str = "maintenance.state.v1";
const MAX_ROWS: usize = 128;
const MAX_METADATA_BYTES: usize = 64 * 1024;
const MAX_VM_STEPS: u64 = 50_000;
const LEASE_MILLIS: i64 = 30_000;
const ANOMALY_RETENTION_MILLIS: i64 = 30 * 24 * 60 * 60 * 1000;
const QUICK_CHECK_INTERVAL_MILLIS: i64 = 7 * 24 * 60 * 60 * 1000;
const OPTIMIZE_INTERVAL_MILLIS: i64 = 24 * 60 * 60 * 1000;
const WRITE_RESERVE_BYTES: u64 = 1024 * 1024;
const MIN_FREE_BYTES: u64 = 64 * 1024 * 1024;
const SOFT_ADMISSION_TIME: Duration = Duration::from_millis(500);

/// Shared pre-transaction admission for typed writers. This conservative check
/// cannot reserve space against unrelated processes; C09R owns aggregate admission.
pub(crate) fn admit_write(location: &StorageLocation) -> Result<()> {
    let limits = Policy::default();
    let growth = measure_growth(location, &limits)?;
    if growth.total_bytes.saturating_add(WRITE_RESERVE_BYTES) > limits.database_hard_limit_bytes {
        return Err(DatabaseError::CapacityBudget(
            "il database e i suoi file di lavoro hanno raggiunto il budget di spazio",
        ));
    }
    if free_bytes(location.directory())? < MIN_FREE_BYTES.saturating_add(WRITE_RESERVE_BYTES) {
        return Err(DatabaseError::CapacityBudget(
            "lo spazio libero è inferiore al margine minimo per un salvataggio",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn free_bytes(directory: &std::path::Path) -> Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let path = directory
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut available = 0;
    if unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(DatabaseError::Io(io::Error::last_os_error()));
    }
    Ok(available)
}
#[cfg(not(windows))]
fn free_bytes(_directory: &std::path::Path) -> Result<u64> {
    Err(DatabaseError::UnsupportedConfiguration)
}

#[derive(Clone)]
pub struct Policy {
    pub now_millis: i64,
    pub max_rows: usize,
    pub max_payload_bytes: usize,
    pub max_vm_steps: u64,
    pub cooperative_time: Duration,
    pub wal_checkpoint_max_bytes: u64,
    pub database_soft_limit_bytes: u64,
    pub database_hard_limit_bytes: u64,
    pub incremental_vacuum_pages: u32,
    pub quick_check: bool,
    pub optimize: bool,
    pub pressure: bool,
    /// C09R sets this flag to stop optional work; a later admitted tick is explicit.
    pub cancel: Arc<AtomicBool>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            now_millis: chrono::Utc::now().timestamp_millis(),
            max_rows: MAX_ROWS,
            max_payload_bytes: MAX_METADATA_BYTES,
            max_vm_steps: MAX_VM_STEPS,
            cooperative_time: Duration::from_millis(100),
            wal_checkpoint_max_bytes: 32 * 1024 * 1024,
            database_soft_limit_bytes: 1024 * 1024 * 1024,
            database_hard_limit_bytes: 4 * 1024 * 1024 * 1024,
            incremental_vacuum_pages: 16,
            quick_check: true,
            optimize: true,
            pressure: false,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    DeferredBudget,
    DeferredPressure,
    DeferredBusy,
    DeferredCapacity,
    ReadOnly,
    InTransaction,
    InvalidMetadata,
    CorruptionDetected,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capacity {
    Healthy,
    SoftLimit,
    HardLimit,
}

#[derive(Debug, Clone, Serialize)]
pub struct Growth {
    pub database_bytes: u64,
    pub wal_bytes: u64,
    pub shared_memory_bytes: u64,
    pub total_bytes: u64,
    pub capacity: Capacity,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    NotRequested,
    NotDue,
    Passed,
    Corrupt,
    DeferredBudget,
    SkippedLargeDatabase,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Checkpoint {
    NotRun,
    SkippedLargeWal,
    Done {
        busy: i64,
        wal_frames: i64,
        copied_frames: i64,
        elapsed_micros: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Anomaly {
    pub table: String,
    pub row_id: i64,
    pub reason: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub occurrences: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeferredRow {
    pub table: String,
    pub row_id: i64,
    pub last_seen: i64,
    pub occurrences: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub outcome: Outcome,
    pub observed_rows: usize,
    pub inspected_payload_bytes: usize,
    pub vm_steps_approx: u64,
    /// Elapsed core time including OS admission, excluding the facade's connection open.
    pub elapsed_micros: u64,
    /// Local filesystem/ACL admission is measured separately from cooperative SQL work.
    pub admission_elapsed_micros: u64,
    pub growth: Option<Growth>,
    pub quick_check: Check,
    pub checkpoint: Checkpoint,
    pub optimized: bool,
    pub vacuum_pages_requested: u32,
    pub more_rows: bool,
    pub anomalies: Vec<Anomaly>,
    /// Unverified for the current byte quota; these records are not malformed.
    pub deferred_rows: Vec<DeferredRow>,
    pub unrecorded_deferred_rows: u64,
    pub unrecorded_anomalies: u64,
    pub lease_released: bool,
    pub scan_progress_saved: bool,
}

impl Report {
    fn new() -> Self {
        Self {
            outcome: Outcome::Completed,
            observed_rows: 0,
            inspected_payload_bytes: 0,
            vm_steps_approx: 0,
            elapsed_micros: 0,
            admission_elapsed_micros: 0,
            growth: None,
            quick_check: Check::NotRequested,
            checkpoint: Checkpoint::NotRun,
            optimized: false,
            vacuum_pages_requested: 0,
            more_rows: false,
            anomalies: Vec::new(),
            deferred_rows: Vec::new(),
            unrecorded_deferred_rows: 0,
            unrecorded_anomalies: 0,
            lease_released: false,
            scan_progress_saved: false,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaintenanceState {
    project_cursor: i64,
    setting_cursor: i64,
    scan_settings_next: bool,
    last_quick_check: Option<i64>,
    last_optimize: Option<i64>,
    anomalies: Vec<Anomaly>,
    #[serde(default)]
    deferred_rows: Vec<DeferredRow>,
    #[serde(default)]
    unrecorded_deferred_rows: u64,
    unrecorded_anomalies: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lease {
    owner: String,
    pid: u32,
    process_started: u64,
    expires_at: i64,
}

struct Progress<'a> {
    connection: &'a Connection,
    steps: Arc<AtomicU64>,
    previous_timeout: u32,
    previous_analysis_limit: u32,
    phase: Arc<AtomicU8>,
    phase_limit: Arc<AtomicU64>,
    phase_nanos: Arc<AtomicU64>,
    started: Instant,
    hard_steps: u64,
    work_steps: u64,
    finalize_steps: u64,
    hard_nanos: u64,
}

#[derive(Clone, Copy)]
enum Phase {
    Work = 0,
    Finalize = 1,
    Optional = 2,
    Cleanup = 3,
}

impl<'a> Progress<'a> {
    fn install(connection: &'a Connection, policy: &Policy, started: Instant) -> Result<Self> {
        let previous_timeout =
            connection.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
        let previous_analysis_limit =
            connection.pragma_query_value(None, "analysis_limit", |row| row.get(0))?;
        connection.busy_timeout(Duration::ZERO)?;
        let steps = Arc::new(AtomicU64::new(0));
        let observed = steps.clone();
        let cancelled = policy.cancel.clone();
        let hard_steps = policy.max_vm_steps;
        let cleanup_reserve = (hard_steps / 10).clamp(1, 1000);
        let state_reserve = (hard_steps / 5)
            .clamp(1, 4000)
            .min(hard_steps.saturating_sub(cleanup_reserve));
        let work_steps = hard_steps
            .saturating_sub(cleanup_reserve)
            .saturating_sub(state_reserve);
        let finalize_steps = hard_steps.saturating_sub(cleanup_reserve);
        let hard_nanos = policy.cooperative_time.as_nanos().min(u64::MAX as u128) as u64;
        let phase = Arc::new(AtomicU8::new(Phase::Work as u8));
        let phase_limit = Arc::new(AtomicU64::new(work_steps));
        let phase_nanos = Arc::new(AtomicU64::new(hard_nanos.saturating_mul(8) / 10));
        let active_phase = phase.clone();
        let active_limit = phase_limit.clone();
        let active_time = phase_nanos.clone();
        connection.progress_handler(
            1,
            Some(move || {
                let total = observed.fetch_add(1, Ordering::Relaxed).saturating_add(1);
                let work = matches!(active_phase.load(Ordering::Relaxed), 0 | 2);
                total >= active_limit.load(Ordering::Relaxed)
                    || started.elapsed().as_nanos() >= active_time.load(Ordering::Relaxed) as u128
                    || (work && cancelled.load(Ordering::Relaxed))
            }),
        )?;
        Ok(Self {
            connection,
            steps,
            previous_timeout,
            previous_analysis_limit,
            phase,
            phase_limit,
            phase_nanos,
            started,
            hard_steps,
            work_steps,
            finalize_steps,
            hard_nanos,
        })
    }

    fn phase(&self, phase: Phase) {
        let (steps, nanos) = match phase {
            Phase::Work | Phase::Optional => {
                (self.work_steps, self.hard_nanos.saturating_mul(8) / 10)
            }
            Phase::Finalize => (self.finalize_steps, self.hard_nanos.saturating_mul(9) / 10),
            Phase::Cleanup => (self.hard_steps, self.hard_nanos),
        };
        self.phase_limit.store(steps, Ordering::Relaxed);
        self.phase_nanos.store(nanos, Ordering::Relaxed);
        self.phase.store(phase as u8, Ordering::Relaxed);
    }

    fn available(&self) -> bool {
        self.steps.load(Ordering::Relaxed) < self.phase_limit.load(Ordering::Relaxed)
            && self.started.elapsed().as_nanos() < self.phase_nanos.load(Ordering::Relaxed) as u128
    }

    fn work_deadline(&self) -> Instant {
        self.started + Duration::from_nanos(self.hard_nanos.saturating_mul(8) / 10)
    }
}

impl Drop for Progress<'_> {
    fn drop(&mut self) {
        let _ = self.connection.progress_handler(0, None::<fn() -> bool>);
        let _ = self
            .connection
            .busy_timeout(Duration::from_millis(self.previous_timeout as u64));
    }
}

/// Open an existing database on a separate worker connection. This path never
/// creates a database, migrates a schema or takes the interactive state mutex.
pub(crate) fn run_database(
    location: &StorageLocation,
    writable: bool,
    policy: Policy,
) -> Result<Report> {
    validate_policy(&policy)?;
    if !writable {
        let mut report = Report::new();
        report.outcome = Outcome::ReadOnly;
        return Ok(report);
    }
    if policy.pressure || policy.cancel.load(Ordering::Relaxed) {
        let mut report = Report::new();
        report.outcome = Outcome::DeferredPressure;
        return Ok(report);
    }
    location.verify_private_directory()?;
    location.verify_private_file(&location.database_path())?;
    for name in ["shinatama.db-wal", "shinatama.db-shm"] {
        let path = location.directory().join(name);
        if path.try_exists().map_err(DatabaseError::Io)? {
            location.verify_private_file(&path)?;
        }
    }
    let mut connection = Connection::open_with_flags(
        location.database_path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    connection.busy_timeout(Duration::ZERO)?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    run(&mut connection, location, policy)
}

/// Does not schedule itself, retry a deferred batch, run full VACUUM, delete work
/// history, or erase migration backups. The caller must admit the next idle tick.
pub fn run(
    connection: &mut Connection,
    location: &StorageLocation,
    policy: Policy,
) -> Result<Report> {
    validate_policy(&policy)?;
    let started = Instant::now();
    let mut report = Report::new();
    if policy.pressure || policy.cancel.load(Ordering::Relaxed) {
        report.outcome = Outcome::DeferredPressure;
        return Ok(report);
    }
    if !connection.is_autocommit() {
        report.outcome = Outcome::InTransaction;
        return Ok(report);
    }
    if connection.is_readonly(MAIN_DB)?
        || connection.pragma_query_value::<bool, _>(None, "query_only", |row| row.get(0))?
    {
        report.outcome = Outcome::ReadOnly;
        return Ok(report);
    }
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > schema::CURRENT_VERSION {
        report.outcome = Outcome::ReadOnly;
        return Ok(report);
    }
    if version != schema::CURRENT_VERSION {
        return Err(DatabaseError::UnsupportedSchema);
    }
    location.verify_private_directory()?;
    let Some(_lock) = acquire_lock(location)? else {
        report.outcome = Outcome::DeferredBusy;
        return Ok(report);
    };
    report.growth = Some(measure_growth(location, &policy)?);
    if report.growth.as_ref().is_some_and(|growth| {
        growth.capacity == Capacity::HardLimit || growth.wal_bytes > 128 * 1024 * 1024
    }) {
        report.outcome = Outcome::DeferredCapacity;
        report.elapsed_micros = micros(started.elapsed());
        return Ok(report);
    }
    if let Err(error) = admit_write(location) {
        if matches!(error, DatabaseError::CapacityBudget(_)) {
            report.outcome = Outcome::DeferredCapacity;
            return Ok(report);
        }
        return Err(error);
    }
    report.admission_elapsed_micros = micros(started.elapsed());
    if let Some(outcome) = admission_outcome(&policy, started.elapsed()) {
        report.outcome = outcome;
        report.elapsed_micros = micros(started.elapsed());
        return Ok(report);
    }
    // The cooperative SQLite quota starts after bounded-count OS admission calls.
    // Their latency remains visible in total elapsed time and has no hard deadline.
    let progress = Progress::install(connection, &policy, Instant::now())?;
    let owner = uuid::Uuid::new_v4().to_string();
    let mut claimed = false;
    let result = (|| -> Result<()> {
        schema::validate_version(connection, version).map_err(|error| {
            if matches!(error, rusqlite::Error::InvalidQuery) {
                DatabaseError::UnsupportedSchema
            } else {
                DatabaseError::Sqlite(error)
            }
        })?;
        if !claim_lease(connection, &policy, &owner, &mut report)? {
            return Ok(());
        }
        claimed = true;
        let Some(mut state) = read_state(connection)? else {
            report.outcome = Outcome::InvalidMetadata;
            return Ok(());
        };
        state.anomalies.retain(|item| {
            item.last_seen >= policy.now_millis.saturating_sub(ANOMALY_RETENTION_MILLIS)
        });
        state.deferred_rows.retain(|item| {
            item.last_seen >= policy.now_millis.saturating_sub(ANOMALY_RETENTION_MILLIS)
        });

        let scan_result = inspect_records(
            connection,
            &policy,
            &mut state,
            &mut report,
            progress.work_deadline(),
        );
        report.anomalies = state.anomalies.clone();
        report.deferred_rows = state.deferred_rows.clone();
        report.unrecorded_deferred_rows = state.unrecorded_deferred_rows;
        report.unrecorded_anomalies = state.unrecorded_anomalies;

        // Completed records update the cursor individually. The reserved finalization
        // phase makes that frontier durable before any expensive optional check.
        progress.phase(Phase::Finalize);
        if !corrupt_error(&scan_result) && progress.available() {
            persist_state(connection, &state)?;
            report.scan_progress_saved = true;
        }
        scan_result?;
        if !report.scan_progress_saved {
            report.outcome = Outcome::DeferredBudget;
            return Ok(());
        }
        if report.outcome != Outcome::Completed {
            return Ok(());
        }

        progress.phase(Phase::Optional);
        if !progress.available() || policy.cancel.load(Ordering::Relaxed) {
            report.outcome = if policy.cancel.load(Ordering::Relaxed) {
                Outcome::DeferredPressure
            } else {
                Outcome::DeferredBudget
            };
            return Ok(());
        }
        let previous_checks = (state.last_quick_check, state.last_optimize);
        run_quick_check(
            connection,
            &policy,
            &mut state,
            &mut report,
            progress.work_deadline(),
        )?;
        if report.outcome == Outcome::Completed {
            run_optimization(
                connection,
                &policy,
                &mut state,
                &mut report,
                progress.work_deadline(),
            )?;
        }
        if report.outcome == Outcome::Completed
            && progress.available()
            && !policy.cancel.load(Ordering::Relaxed)
        {
            run_checkpoint(connection, &policy, &mut report)?;
        }
        if report.outcome != Outcome::CorruptionDetected
            && previous_checks != (state.last_quick_check, state.last_optimize)
        {
            progress.phase(Phase::Finalize);
            if progress.available() {
                persist_state(connection, &state)?;
            }
        }
        Ok(())
    })();

    let unsafe_to_write = report.quick_check == Check::Corrupt || corrupt_error(&result);
    let unhandled = match result {
        Ok(()) => None,
        Err(DatabaseError::NewerSchema { .. }) => {
            report.outcome = Outcome::ReadOnly;
            None
        }
        Err(DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _)))
            if error.code == ErrorCode::OperationInterrupted =>
        {
            report.outcome = if policy.cancel.load(Ordering::Relaxed) {
                Outcome::DeferredPressure
            } else {
                Outcome::DeferredBudget
            };
            if !matches!(
                report.quick_check,
                Check::NotDue | Check::NotRequested | Check::Passed | Check::Corrupt
            ) {
                report.quick_check = Check::DeferredBudget;
            }
            None
        }
        Err(DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _)))
            if matches!(
                error.code,
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
            ) =>
        {
            report.outcome = Outcome::DeferredBusy;
            None
        }
        Err(error) => Some(error),
    };

    // The same monotonic VM counter and original hard deadline cover control-plane
    // cleanup. No handler reset or unbudgeted lease UPDATE follows this phase.
    progress.phase(Phase::Cleanup);
    if !connection.is_autocommit() && progress.available() {
        let _ = connection.execute_batch("ROLLBACK");
    }
    if claimed && !unsafe_to_write && progress.available() {
        match release_lease(connection, &owner) {
            Ok(()) => report.lease_released = true,
            Err(DatabaseError::NewerSchema { .. }) => report.outcome = Outcome::ReadOnly,
            Err(DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error, _)))
                if error.code == ErrorCode::OperationInterrupted =>
            {
                report.outcome = Outcome::DeferredBudget
            }
            Err(_) => {} // The private OS lock releases; the persisted lease still expires.
        }
    }
    if progress.available() {
        let _ = connection.pragma_update(None, "analysis_limit", progress.previous_analysis_limit);
    }
    report.vm_steps_approx = progress.steps.load(Ordering::Relaxed);
    drop(progress);
    if let Some(error) = unhandled {
        return Err(error);
    }
    report.elapsed_micros = micros(started.elapsed());
    Ok(report)
}

fn corrupt_error(result: &Result<()>) -> bool {
    matches!(result,Err(DatabaseError::Sqlite(rusqlite::Error::SqliteFailure(error,_))) if matches!(error.code,ErrorCode::DatabaseCorrupt|ErrorCode::NotADatabase))
}

fn persist_state(connection: &Connection, state: &MaintenanceState) -> Result<()> {
    let value =
        serde_json::to_string(state).map_err(|_| DatabaseError::UnsupportedConfiguration)?;
    if value.len() > MAX_METADATA_BYTES {
        return Err(DatabaseError::UnsupportedConfiguration);
    }
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    schema::require_write_version(&transaction)?;
    transaction.execute("INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![STATE_KEY,value])?;
    transaction.commit()?;
    Ok(())
}

fn validate_policy(policy: &Policy) -> Result<()> {
    if policy.now_millis < 0
        || policy.max_rows == 0
        || policy.max_rows > MAX_ROWS
        || policy.max_payload_bytes == 0
        || policy.max_payload_bytes > MAX_METADATA_BYTES
        || policy.max_vm_steps == 0
        || policy.max_vm_steps > MAX_VM_STEPS
        || policy.cooperative_time.is_zero()
        || policy.cooperative_time > Duration::from_millis(100)
        || policy.wal_checkpoint_max_bytes > 32 * 1024 * 1024
        || policy.database_soft_limit_bytes > policy.database_hard_limit_bytes
        || policy.database_hard_limit_bytes == 0
        || policy.database_hard_limit_bytes > 4 * 1024 * 1024 * 1024
        || policy.incremental_vacuum_pages > 16
    {
        return Err(DatabaseError::UnsupportedConfiguration);
    }
    Ok(())
}

fn admission_outcome(policy: &Policy, elapsed: Duration) -> Option<Outcome> {
    if policy.pressure || policy.cancel.load(Ordering::Relaxed) {
        Some(Outcome::DeferredPressure)
    } else if elapsed > SOFT_ADMISSION_TIME {
        Some(Outcome::DeferredBudget)
    } else {
        None
    }
}

fn should_yield(policy: &Policy, deadline: Instant) -> bool {
    policy.cancel.load(Ordering::Relaxed) || Instant::now() >= deadline
}
fn micros(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

fn acquire_lock(location: &StorageLocation) -> Result<Option<File>> {
    let path = location.directory().join("maintenance.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(&path).map_err(DatabaseError::Io)?;
    location.secure_private_file(&path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(DatabaseError::Io(error)),
    }
}

fn measure_growth(location: &StorageLocation, policy: &Policy) -> Result<Growth> {
    let mut sizes = [0; 3];
    for (index, name) in ["shinatama.db", "shinatama.db-wal", "shinatama.db-shm"]
        .into_iter()
        .enumerate()
    {
        let path = location.directory().join(name);
        match fs::metadata(&path) {
            Ok(metadata) => {
                location.verify_private_file(&path)?;
                sizes[index] = metadata.len();
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(DatabaseError::Io(error)),
        }
    }
    let total = sizes.iter().copied().fold(0u64, u64::saturating_add);
    let capacity = if total >= policy.database_hard_limit_bytes {
        Capacity::HardLimit
    } else if total >= policy.database_soft_limit_bytes {
        Capacity::SoftLimit
    } else {
        Capacity::Healthy
    };
    Ok(Growth {
        database_bytes: sizes[0],
        wal_bytes: sizes[1],
        shared_memory_bytes: sizes[2],
        total_bytes: total,
        capacity,
    })
}

fn bounded_setting(connection: &Connection, key: &str) -> Result<Option<Option<String>>> {
    Ok(connection.query_row("SELECT CASE WHEN octet_length(value)<=?2 THEN value ELSE NULL END FROM setting WHERE key=?1",params![key,MAX_METADATA_BYTES as i64],|row| row.get(0)).optional()?)
}

fn read_state(connection: &Connection) -> Result<Option<MaintenanceState>> {
    match bounded_setting(connection, STATE_KEY)? {
        None => Ok(Some(MaintenanceState::default())),
        Some(Some(value)) => Ok(serde_json::from_str::<MaintenanceState>(&value)
            .ok()
            .filter(|state| {
                state.anomalies.len() <= MAX_ROWS
                    && state.deferred_rows.len() <= MAX_ROWS
                    && state.deferred_rows.iter().all(|item| {
                        matches!(item.table.as_str(), "project" | "setting")
                            && item.row_id > 0
                            && item.last_seen >= 0
                            && item.occurrences > 0
                    })
                    && state.project_cursor >= 0
                    && state.setting_cursor >= 0
                    && state.anomalies.iter().all(|item| {
                        matches!(item.table.as_str(), "project" | "setting")
                            && matches!(
                                item.reason.as_str(),
                                "oversized"
                                    | "invalid_json"
                                    | "appearance_shape"
                                    | "import_marker_shape"
                                    | "project_id"
                                    | "project_path"
                            )
                            && item.row_id > 0
                            && item.first_seen >= 0
                            && item.last_seen >= item.first_seen
                            && item.occurrences > 0
                    })
            })),
        Some(None) => Ok(None),
    }
}

fn claim_lease(
    connection: &Connection,
    policy: &Policy,
    owner: &str,
    report: &mut Report,
) -> Result<bool> {
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    schema::require_write_version(&transaction)?;
    if let Some(value) = bounded_setting(&transaction, LEASE_KEY)? {
        let Some(existing) = value.and_then(|json| serde_json::from_str::<Lease>(&json).ok())
        else {
            report.outcome = Outcome::InvalidMetadata;
            return Ok(false);
        };
        if existing.owner.len() > 64 {
            report.outcome = Outcome::InvalidMetadata;
            return Ok(false);
        }
        if !existing.owner.is_empty() && existing.expires_at > policy.now_millis {
            report.outcome = Outcome::DeferredBusy;
            return Ok(false);
        }
    }
    let lease = Lease {
        owner: owner.to_owned(),
        pid: std::process::id(),
        process_started: process_started()?,
        expires_at: policy.now_millis.saturating_add(LEASE_MILLIS),
    };
    let json =
        serde_json::to_string(&lease).map_err(|_| DatabaseError::UnsupportedConfiguration)?;
    transaction.execute("INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![LEASE_KEY,json])?;
    transaction.commit()?;
    Ok(true)
}

fn release_lease(connection: &Connection, owner: &str) -> Result<()> {
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    schema::require_write_version(&transaction)?;
    transaction.execute("UPDATE setting SET value=json_set(value,'$.owner','','$.expires_at',0) WHERE key=?1 AND json_extract(value,'$.owner')=?2",params![LEASE_KEY,owner])?;
    transaction.commit()?;
    Ok(())
}

#[cfg(windows)]
fn process_started() -> Result<u64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exited = created;
    let mut kernel = created;
    let mut user = created;
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(DatabaseError::Io(io::Error::last_os_error()));
    }
    Ok(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
}
#[cfg(not(windows))]
fn process_started() -> Result<u64> {
    Err(DatabaseError::UnsupportedConfiguration)
}

fn inspect_records(
    connection: &Connection,
    policy: &Policy,
    state: &mut MaintenanceState,
    report: &mut Report,
    deadline: Instant,
) -> Result<()> {
    // The first query reads sizes only. Payload validation is prepared only after
    // admitting that row against the remaining byte quota, avoiding hidden work
    // on an otherwise deferred second record.
    let settings = state.scan_settings_next;
    // Rotate fairly even if preparation or a later record hits its work quota.
    state.scan_settings_next = !settings;
    let (table, cursor, sql) = if settings {
        (
            "setting",
            state.setting_cursor,
            "SELECT rowid,octet_length(key)+octet_length(value),octet_length(key)>256 OR octet_length(value)>CASE WHEN key='operational.native_session_cleanup.v1' THEN 131072 ELSE 65536 END FROM setting WHERE rowid>?1 AND key NOT IN ('maintenance.lease.v1','maintenance.state.v1') ORDER BY rowid LIMIT ?2",
        )
    } else {
        (
            "project",
            state.project_cursor,
            "SELECT rowid,octet_length(id)+octet_length(path)+octet_length(path_key)+octet_length(name),octet_length(id)>64 OR octet_length(path)>65536 OR octet_length(path_key)>65536 OR octet_length(name)>1024 FROM project WHERE rowid>?1 ORDER BY rowid LIMIT ?2",
        )
    };
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![cursor, policy.max_rows as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, bool>(2)?,
        ))
    })?;
    let mut last = cursor;
    let mut observed = 0;
    for row in rows {
        if should_yield(policy, deadline) {
            report.outcome = if policy.cancel.load(Ordering::Relaxed) {
                Outcome::DeferredPressure
            } else {
                Outcome::DeferredBudget
            };
            break;
        }
        let (row_id, bytes, violates_policy) = row?;
        let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
        if !violates_policy
            && bytes <= policy.max_payload_bytes
            && report.inspected_payload_bytes.saturating_add(bytes) > policy.max_payload_bytes
        {
            report.more_rows = true;
            break;
        }
        if violates_policy {
            record_anomaly(state, table, row_id, "oversized", policy.now_millis);
            advance_cursor(state, settings, row_id, report);
            observed += 1;
            last = row_id;
            continue;
        }
        if bytes > policy.max_payload_bytes {
            record_deferral(state, table, row_id, policy.now_millis);
            report.outcome = Outcome::DeferredBudget;
            report.more_rows = true;
            advance_cursor(state, settings, row_id, report);
            observed += 1;
            last = row_id;
            continue;
        }
        let remaining = policy
            .max_payload_bytes
            .saturating_sub(report.inspected_payload_bytes);
        let Some((validated_bytes, reason)) =
            validate_record(connection, settings, row_id, remaining)?
        else {
            advance_cursor(state, settings, row_id, report);
            observed += 1;
            last = row_id;
            continue;
        };
        if reason.as_deref() == Some("__quota") {
            record_deferral(state, table, row_id, policy.now_millis);
            report.outcome = Outcome::DeferredBudget;
            report.more_rows = true;
            advance_cursor(state, settings, row_id, report);
            observed += 1;
            last = row_id;
            continue;
        }
        state
            .deferred_rows
            .retain(|item| !(item.table == table && item.row_id == row_id));
        if reason.as_deref() != Some("oversized") {
            report.inspected_payload_bytes = report.inspected_payload_bytes.saturating_add(
                usize::try_from(validated_bytes)
                    .map_err(|_| DatabaseError::UnsupportedConfiguration)?,
            );
        }
        if let Some(reason) = reason {
            record_anomaly(state, table, row_id, &reason, policy.now_millis);
        }
        advance_cursor(state, settings, row_id, report);
        observed += 1;
        last = row_id;
    }
    let next = if observed == 0 && report.outcome == Outcome::Completed {
        0
    } else {
        last
    };
    if settings {
        state.setting_cursor = next;
    } else {
        state.project_cursor = next;
    }
    if observed >= policy.max_rows {
        report.more_rows = true;
    }
    Ok(())
}

fn advance_cursor(state: &mut MaintenanceState, settings: bool, row_id: i64, report: &mut Report) {
    if settings {
        state.setting_cursor = row_id;
    } else {
        state.project_cursor = row_id;
    }
    report.observed_rows += 1;
}

fn validate_record(
    connection: &Connection,
    settings: bool,
    row_id: i64,
    remaining: usize,
) -> Result<Option<(i64, Option<String>)>> {
    let sql = if settings {
        "SELECT octet_length(key)+octet_length(value),CASE
        WHEN octet_length(key)>256 OR octet_length(value)>CASE WHEN key='operational.native_session_cleanup.v1' THEN 131072 ELSE 65536 END THEN 'oversized'
        WHEN octet_length(key)+octet_length(value)>?2 THEN '__quota'
        WHEN NOT json_valid(value) THEN 'invalid_json'
        WHEN key='ui.appearance.v1' AND (json_type(value)!='object' OR coalesce(json_type(value,'$.terminal_font_size'),'')!='integer' OR json_extract(value,'$.terminal_font_size') NOT BETWEEN 8 AND 40 OR coalesce(json_type(value,'$.reduced_motion'),'') NOT IN ('true','false') OR (SELECT count(*) FROM json_each(value))!=2) THEN 'appearance_shape'
        WHEN key='migration.projects_json.v1' AND coalesce(json_type(value,'$.completed'),'')!='true' THEN 'import_marker_shape'
        WHEN key='operational.native_session_cleanup.v1' AND (coalesce(json_type(value),'')!='object' OR coalesce(json_extract(value,'$.format_version'),0)!=1 OR coalesce(json_type(value,'$.entries'),'')!='array' OR json_array_length(value,'$.entries')>64 OR (SELECT count(*) FROM json_each(value))!=2) THEN 'native_cleanup_shape'
        ELSE NULL END FROM setting WHERE rowid=?1"
    } else {
        "SELECT octet_length(id)+octet_length(path)+octet_length(path_key)+octet_length(name),CASE
        WHEN octet_length(id)>64 OR octet_length(path)>65536 OR octet_length(path_key)>65536 OR octet_length(name)>1024 THEN 'oversized'
        WHEN octet_length(id)+octet_length(path)+octet_length(path_key)+octet_length(name)>?2 THEN '__quota'
        WHEN id NOT GLOB 'p_[a-z2-7][a-z2-7][a-z2-7][a-z2-7][a-z2-7][a-z2-7][a-z2-7][a-z2-7][a-z2-7][a-z2-7][a-z2-7][a-z2-7]' THEN 'project_id'
        WHEN NOT ((substr(path,2,1)=':' AND substr(path,3,1) IN (char(92),'/')) OR substr(path,1,2)=char(92)||char(92)) OR instr(path,char(0))>0 OR octet_length(path_key)=0 THEN 'project_path'
        ELSE NULL END FROM project WHERE rowid=?1"
    };
    Ok(connection
        .query_row(sql, params![row_id, remaining as i64], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?)
}

fn record_anomaly(state: &mut MaintenanceState, table: &str, row_id: i64, reason: &str, now: i64) {
    if let Some(existing) = state
        .anomalies
        .iter_mut()
        .find(|item| item.table == table && item.row_id == row_id && item.reason == reason)
    {
        existing.occurrences = existing.occurrences.saturating_add(1);
        existing.last_seen = now;
    } else if state.anomalies.len() < MAX_ROWS {
        state.anomalies.push(Anomaly {
            table: table.to_owned(),
            row_id,
            reason: reason.to_owned(),
            first_seen: now,
            last_seen: now,
            occurrences: 1,
        });
    } else {
        state.unrecorded_anomalies = state.unrecorded_anomalies.saturating_add(1);
    }
}

fn record_deferral(state: &mut MaintenanceState, table: &str, row_id: i64, now: i64) {
    if let Some(existing) = state
        .deferred_rows
        .iter_mut()
        .find(|item| item.table == table && item.row_id == row_id)
    {
        existing.occurrences = existing.occurrences.saturating_add(1);
        existing.last_seen = now;
    } else if state.deferred_rows.len() < MAX_ROWS {
        state.deferred_rows.push(DeferredRow {
            table: table.to_owned(),
            row_id,
            last_seen: now,
            occurrences: 1,
        });
    } else {
        state.unrecorded_deferred_rows = state.unrecorded_deferred_rows.saturating_add(1);
    }
}

fn run_quick_check(
    connection: &Connection,
    policy: &Policy,
    state: &mut MaintenanceState,
    report: &mut Report,
    deadline: Instant,
) -> Result<()> {
    if !policy.quick_check {
        return Ok(());
    }
    if state
        .last_quick_check
        .is_some_and(|last| policy.now_millis.saturating_sub(last) < QUICK_CHECK_INTERVAL_MILLIS)
    {
        report.quick_check = Check::NotDue;
        return Ok(());
    }
    report.quick_check = Check::DeferredBudget;
    if should_yield(policy, deadline) {
        report.outcome = Outcome::DeferredBudget;
        return Ok(());
    }
    let pages: u32 = connection.pragma_query_value(None, "page_count", |row| row.get(0))?;
    if pages > 4096 {
        report.quick_check = Check::SkippedLargeDatabase;
        return Ok(());
    }
    let mut statement = connection.prepare("PRAGMA quick_check(1)")?;
    let mut rows = statement.query([])?;
    let Some(row) = rows.next()? else {
        return Err(DatabaseError::UnsupportedConfiguration);
    };
    let value: String = row.get(0)?;
    if value == "ok" {
        report.quick_check = Check::Passed;
        state.last_quick_check = Some(policy.now_millis);
    } else {
        report.quick_check = Check::Corrupt;
        report.outcome = Outcome::CorruptionDetected;
    }
    Ok(())
}

fn run_optimization(
    connection: &Connection,
    policy: &Policy,
    state: &mut MaintenanceState,
    report: &mut Report,
    deadline: Instant,
) -> Result<()> {
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    schema::require_write_version(&transaction)?;
    if policy.optimize
        && !state
            .last_optimize
            .is_some_and(|last| policy.now_millis.saturating_sub(last) < OPTIMIZE_INTERVAL_MILLIS)
        && !should_yield(policy, deadline)
    {
        transaction.pragma_update(None, "analysis_limit", 128)?;
        transaction.execute_batch("PRAGMA optimize")?;
        report.optimized = true;
        state.last_optimize = Some(policy.now_millis);
    }
    if policy.incremental_vacuum_pages > 0 && !should_yield(policy, deadline) {
        let mode: u32 = transaction.pragma_query_value(None, "auto_vacuum", |row| row.get(0))?;
        if mode == 2 {
            transaction.pragma_update(
                None,
                "incremental_vacuum",
                policy.incremental_vacuum_pages,
            )?;
            report.vacuum_pages_requested = policy.incremental_vacuum_pages;
        }
    }
    transaction.commit()?;
    Ok(())
}

#[cfg(all(test, windows))]
mod ownership_tests {
    use super::*;
    use crate::database::Database;
    use crate::location::LocationRequest;

    #[test]
    fn admission_rechecks_cancel_and_has_a_fixed_soft_500ms_ceiling() {
        let policy = Policy::default();
        assert_eq!(admission_outcome(&policy, Duration::from_millis(500)), None);
        assert_eq!(
            admission_outcome(&policy, Duration::from_millis(501)),
            Some(Outcome::DeferredBudget)
        );
        policy.cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            admission_outcome(&policy, Duration::from_millis(1)),
            Some(Outcome::DeferredPressure)
        );
    }

    #[test]
    fn newer_schema_between_probe_and_job_writes_preserves_all_job_metadata() {
        let root = crate::test_scratch::root()
            .join("shinatama-maintenance-guards")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&root).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: root.clone(),
            home_override: Some(root.join("state")),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        let database = Database::open(&location, None).unwrap();
        let stale = Connection::open(location.database_path()).unwrap();
        let upgrader = Connection::open(location.database_path()).unwrap();
        assert_eq!(
            stale
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            schema::CURRENT_VERSION
        );
        upgrader.pragma_update(None, "user_version", 999).unwrap();
        let mut report = Report::new();
        assert!(matches!(
            claim_lease(&stale, &Policy::default(), "old-owner", &mut report),
            Err(DatabaseError::NewerSchema { found: 999, .. })
        ));
        assert!(matches!(
            persist_state(&stale, &MaintenanceState::default()),
            Err(DatabaseError::NewerSchema { found: 999, .. })
        ));
        assert_eq!(
            stale
                .query_row::<i64, _, _>("SELECT count(*) FROM setting", [], |row| row.get(0))
                .unwrap(),
            0
        );

        // A previously committed lease may also outlive an upgrade. Releasing it
        // is a write and must honor the fresh header under the same transaction.
        upgrader
            .pragma_update(None, "user_version", schema::CURRENT_VERSION)
            .unwrap();
        let lease = Lease {
            owner: "old-owner".into(),
            pid: std::process::id(),
            process_started: process_started().unwrap(),
            expires_at: 30_000,
        };
        let original = serde_json::to_string(&lease).unwrap();
        upgrader
            .execute(
                "INSERT INTO setting VALUES(?1,?2)",
                params![LEASE_KEY, original],
            )
            .unwrap();
        upgrader.pragma_update(None, "user_version", 999).unwrap();
        assert!(matches!(
            release_lease(&stale, "old-owner"),
            Err(DatabaseError::NewerSchema { found: 999, .. })
        ));
        assert_eq!(
            stale
                .query_row::<String, _, _>(
                    "SELECT value FROM setting WHERE key=?1",
                    [LEASE_KEY],
                    |row| row.get(0)
                )
                .unwrap(),
            original
        );
        drop(upgrader);
        drop(stale);
        drop(database);
        drop(location);
        fs::remove_dir_all(root).unwrap();
    }
}

fn run_checkpoint(connection: &Connection, policy: &Policy, report: &mut Report) -> Result<()> {
    if report
        .growth
        .as_ref()
        .is_some_and(|growth| growth.wal_bytes > policy.wal_checkpoint_max_bytes)
    {
        report.checkpoint = Checkpoint::SkippedLargeWal;
        return Ok(());
    }
    let started = Instant::now();
    let (busy, wal_frames, copied_frames) =
        connection.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    report.checkpoint = Checkpoint::Done {
        busy,
        wal_frames,
        copied_frames,
        elapsed_micros: micros(started.elapsed()),
    };
    Ok(())
}
