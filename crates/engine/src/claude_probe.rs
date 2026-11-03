//! Claude-only version observation. No interactive launch, login, auth-file read,
//! install, model request, capability qualification or PE-version substitution.
//! Vendor contracts: https://code.claude.com/docs/en/cli-reference and
//! https://code.claude.com/docs/en/env-vars. A private version probe does not prove
//! credential-store isolation for auth status, so that operation is not invoked.
#![cfg(windows)]
use crate::check_scratch::{self, CleanupReport, Disposition, OwnedScratch, ScratchKind};
use crate::resources::{
    admission::{AdmissionCoordinator, CleanupStatus, PrepareRequest, PreparedWorkload},
    process::{ContainedProcess, NativeCommand, spawn_contained},
    sampling::WindowsSampler,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
pub use shinatama_adapter::catalog::ProgramCandidate;
use shinatama_adapter::{
    CapabilitySupport, Operation, RefusalReason, Version,
    catalog::{CatalogSnapshot, InstallationCatalog, PathCatalog, ProgramKind},
};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    resource_reservations::{Budget, PolicyStamp},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
        ReadFile,
    },
    System::Pipes::PeekNamedPipe,
};

pub const WALL_LIMIT: Duration = Duration::from_secs(10);
const PREPARATION_LIMIT: Duration = Duration::from_secs(10);
const ADMISSION_LIMIT: Duration = Duration::from_secs(5);
const CLEANUP_LIMIT: Duration = Duration::from_secs(1);
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
static PROBE_BUSY: AtomicBool = AtomicBool::new(false);
struct ProbeSlot;
impl Drop for ProbeSlot {
    fn drop(&mut self) {
        PROBE_BUSY.store(false, Ordering::Release);
    }
}
pub fn probe_budget() -> Budget {
    Budget {
        memory_bytes: 512 * MIB,
        processes: 8,
        cpu_basis_points: 1000,
        queue_bytes: MAX_OUTPUT_BYTES as u64,
        queue_items: 2,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeIssue {
    Busy,
    NotFound,
    Ambiguous,
    WrapperUnresolved,
    InvalidCandidate,
    UnsafeProgram,
    ProgramTooLarge,
    PrivateRootUnavailable,
    Expired,
    AdmissionRefused,
    SpawnFailed,
    OutputLimit,
    InvalidOutput,
    NonzeroExit,
    CleanupUnconfirmed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authentication {
    Authenticated,
    Unauthenticated,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeCleanup {
    Released,
    Retained,
}
#[derive(Clone, Debug, Serialize)]
pub struct ClaudeProbeSummary {
    pub version: Option<Version>,
    pub executable_sha256: String,
    pub executable_bytes: u64,
    pub authentication: Authentication,
    pub authentication_probe_performed: bool,
    pub cleanup: ProbeCleanup,
    pub issue: Option<ProbeIssue>,
    pub elapsed_ms: u64,
    pub preparation_elapsed_ms: u64,
    pub admission_elapsed_ms: u64,
    pub execution_elapsed_ms: u64,
    pub capabilities: BTreeMap<Operation, CapabilitySupport>,
}
/// Reuses the doctor's bounded PATH catalog. Wrappers are observed, never run.
pub fn discover_claude() -> Result<ProgramCandidate, ProbeIssue> {
    select_claude(&PathCatalog::from_environment().inspect())
}
pub fn select_claude(catalog: &CatalogSnapshot) -> Result<ProgramCandidate, ProbeIssue> {
    let mut candidates = catalog
        .candidates
        .iter()
        .filter(|candidate| candidate.agent.as_str() == "claude_code");
    let first = candidates.next().ok_or(ProbeIssue::NotFound)?;
    if candidates.next().is_some() {
        return Err(ProbeIssue::Ambiguous);
    }
    if first.kind != ProgramKind::DirectExe {
        return Err(ProbeIssue::WrapperUnresolved);
    }
    Ok(first.clone())
}
/// Accept only a bounded single CLI version line, never arbitrary diagnostic text.
pub fn parse_version(bytes: &[u8]) -> Option<Version> {
    if bytes.len() > 128 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?.trim();
    let number = text.strip_suffix(" (Claude Code)").unwrap_or(text);
    if number.is_empty()
        || number.len() > 64
        || !number.is_ascii()
        || number
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !b".-+".contains(&byte))
    {
        return None;
    }
    let core = number.split(['-', '+']).next()?;
    let parts: Vec<_> = core.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || part.len() > 9
                || (part.len() > 1 && part.starts_with('0'))
                || part.parse::<u32>().is_err()
        })
    {
        return None;
    }
    Version::parse(number).ok()
}
struct ProgramPin {
    path: PathBuf,
    file: File,
    _ancestors: Vec<File>,
    sha256: String,
    bytes: u64,
}
fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, ProbeIssue> {
    let mut result = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut result) } == 0 {
        return Err(ProbeIssue::UnsafeProgram);
    }
    Ok(result)
}
fn pin(path: &Path, deadline: Instant) -> Result<ProgramPin, ProbeIssue> {
    StorageLocation::preflight(path).map_err(|_| ProbeIssue::UnsafeProgram)?;
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("claude.exe"))
    {
        return Err(ProbeIssue::InvalidCandidate);
    }
    let parents: Vec<_> = path
        .parent()
        .ok_or(ProbeIssue::UnsafeProgram)?
        .ancestors()
        .take(65)
        .collect();
    if parents.len() > 64 {
        return Err(ProbeIssue::UnsafeProgram);
    }
    let mut held = Vec::new();
    for parent in parents.into_iter().rev() {
        let file = OpenOptions::new()
            .read(true)
            // Attribute-only handles do not participate in Windows sharing
            // checks; LIST makes this no-DELETE-share pin block renaming.
            .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(parent)
            .map_err(|_| ProbeIssue::UnsafeProgram)?;
        let metadata = info(&file)?;
        if metadata.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || metadata.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        {
            return Err(ProbeIssue::UnsafeProgram);
        }
        held.push(file);
    }
    // Deny write/delete while hashing AND while CreateProcess resolves this path.
    // Ancestor handles deny rename, preventing a path swap behind the pinned file.
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| ProbeIssue::UnsafeProgram)?;
    let before = info(&file)?;
    if before.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0 {
        return Err(ProbeIssue::UnsafeProgram);
    }
    let bytes = (u64::from(before.nFileSizeHigh) << 32) | u64::from(before.nFileSizeLow);
    if !(64..=MAX_BINARY_BYTES).contains(&bytes) {
        return Err(ProbeIssue::ProgramTooLarge);
    }
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .map_err(|_| ProbeIssue::UnsafeProgram)?;
    let pe = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if &dos[..2] != b"MZ" || pe > 1024 * 1024 || pe + 4 > bytes {
        return Err(ProbeIssue::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(pe))
        .map_err(|_| ProbeIssue::UnsafeProgram)?;
    let mut signature = [0; 4];
    file.read_exact(&mut signature)
        .map_err(|_| ProbeIssue::UnsafeProgram)?;
    if signature != *b"PE\0\0" {
        return Err(ProbeIssue::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ProbeIssue::UnsafeProgram)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut read = 0;
    loop {
        if Instant::now() >= deadline {
            return Err(ProbeIssue::Expired);
        }
        let count = file
            .read(&mut buffer)
            .map_err(|_| ProbeIssue::UnsafeProgram)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > bytes {
            return Err(ProbeIssue::UnsafeProgram);
        }
        hash.update(&buffer[..count]);
    }
    let after = info(&file)?;
    if read != bytes
        || before.dwVolumeSerialNumber != after.dwVolumeSerialNumber
        || before.nFileIndexHigh != after.nFileIndexHigh
        || before.nFileIndexLow != after.nFileIndexLow
    {
        return Err(ProbeIssue::UnsafeProgram);
    }
    let path = path.canonicalize().map_err(|_| ProbeIssue::UnsafeProgram)?;
    Ok(ProgramPin {
        path,
        file,
        _ancestors: held,
        sha256: format!("{:x}", hash.finalize()),
        bytes,
    })
}
pub struct PreparedClaudeProbe {
    program: ProgramPin,
    root: StorageLocation,
    _private: Vec<StorageLocation>,
    base: StorageLocation,
    scratch: OwnedScratch,
    started: Instant,
    deadline: Instant,
    environment: BTreeMap<OsString, OsString>,
    _slot: ProbeSlot,
}
impl fmt::Debug for PreparedClaudeProbe {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("PreparedClaudeProbe([redacted])")
    }
}
impl PreparedClaudeProbe {
    /// Fresh private directories only. No existing Claude HOME or config is read.
    pub fn prepare(
        candidate: &ProgramCandidate,
        base: &StorageLocation,
    ) -> Result<Self, ProbeIssue> {
        if candidate.agent.as_str() != "claude_code" || candidate.kind != ProgramKind::DirectExe {
            return Err(ProbeIssue::InvalidCandidate);
        }
        PROBE_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ProbeIssue::Busy)?;
        let slot = ProbeSlot;
        let started = Instant::now();
        let deadline = started + PREPARATION_LIMIT;
        let program = pin(&candidate.path, deadline)?;
        base.verify_private_directory()
            .map_err(|_| ProbeIssue::PrivateRootUnavailable)?;
        let (root, scratch) = check_scratch::create(base, ScratchKind::ClaudeProbe)
            .map_err(|_| ProbeIssue::PrivateRootUnavailable)?
            .into_parts();
        let mut private = Vec::new();
        for role in ["home", "config", "cwd", "tmp", "local", "roaming"] {
            let location = root
                .resolve_related_directory(&root.directory().join(role))
                .map_err(|_| ProbeIssue::PrivateRootUnavailable)?;
            location
                .prepare_private_directory()
                .map_err(|_| ProbeIssue::PrivateRootUnavailable)?;
            private.push(location);
        }
        let system = PathBuf::from(
            std::env::var_os("SystemRoot").ok_or(ProbeIssue::PrivateRootUnavailable)?,
        );
        StorageLocation::preflight(&system).map_err(|_| ProbeIssue::PrivateRootUnavailable)?;
        let mut environment = BTreeMap::new();
        for (name, value) in [
            ("SystemRoot", system.as_os_str().to_owned()),
            ("WINDIR", system.as_os_str().to_owned()),
            ("PATH", system.join("System32").into_os_string()),
            ("HOME", root.directory().join("home").into_os_string()),
            (
                "USERPROFILE",
                root.directory().join("home").into_os_string(),
            ),
            (
                "CLAUDE_CONFIG_DIR",
                root.directory().join("config").into_os_string(),
            ),
            ("APPDATA", root.directory().join("roaming").into_os_string()),
            (
                "LOCALAPPDATA",
                root.directory().join("local").into_os_string(),
            ),
            ("TEMP", root.directory().join("tmp").into_os_string()),
            ("TMP", root.directory().join("tmp").into_os_string()),
        ] {
            environment.insert(name.into(), value);
        }
        // Official env-vars reference. These are child-only; never mutate the
        // application's environment or claim that these switches are a firewall.
        for (name, value) in [
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
            ("DISABLE_AUTOUPDATER", "1"),
            ("DISABLE_UPDATES", "1"),
            ("DISABLE_TELEMETRY", "1"),
            ("DISABLE_ERROR_REPORTING", "1"),
            ("CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL", "1"),
            ("CLAUDE_CODE_AUTO_CONNECT_IDE", "false"),
            ("CLAUDE_CODE_SAFE_MODE", "1"),
        ] {
            environment.insert(name.into(), value.into());
        }
        if Instant::now() >= deadline {
            return Err(ProbeIssue::Expired);
        }
        Ok(Self {
            program,
            root,
            _private: private,
            base: base.clone(),
            scratch,
            started,
            deadline,
            environment,
            _slot: slot,
        })
    }
    pub fn executable_sha256(&self) -> &str {
        &self.program.sha256
    }
    pub fn run(
        self,
        database: &Database,
        admission: &AdmissionCoordinator,
        stamp: &PolicyStamp,
        expected_revision: u64,
    ) -> Result<ClaudeProbeOutcome, ProbeIssue> {
        self.run_with(database, || {
            let now = utc_ms()?;
            admission
                .prepare(
                    database,
                    stamp,
                    expected_revision,
                    PrepareRequest {
                        budget: probe_budget(),
                        process_commit_bytes: 256 * MIB,
                        now_ms: now,
                        lease_expires_at_ms: now + 10_000,
                    },
                )
                .map_err(|_| ProbeIssue::AdmissionRefused)
        })
    }
    /// The desktop supplies its SAME shared runtime admission, without creating
    /// a separate policy or exposing any command/recipe to IPC. Preparation is
    /// invoked only after command validation, before the first native child.
    pub fn run_with(
        self,
        database: &Database,
        prepare: impl FnOnce() -> Result<PreparedWorkload, ProbeIssue>,
    ) -> Result<ClaudeProbeOutcome, ProbeIssue> {
        if Instant::now() >= self.deadline {
            return Err(ProbeIssue::Expired);
        }
        // Keep the exact executable and ancestor handles alive through cleanup.
        let _pinned_handle = &self.program.file;
        utc_ms()?;
        let command = NativeCommand::new(
            self.program.path.clone(),
            vec![OsString::from("--version")],
            self.root.directory().join("cwd"),
            self.environment.clone(),
        )
        .map_err(|_| ProbeIssue::InvalidCandidate)?;
        let mut report = ClaudeProbeSummary {
            version: None,
            executable_sha256: self.program.sha256.clone(),
            executable_bytes: self.program.bytes,
            authentication: Authentication::Unknown,
            authentication_probe_performed: false,
            cleanup: ProbeCleanup::Retained,
            issue: None,
            elapsed_ms: 0,
            preparation_elapsed_ms: self.started.elapsed().as_millis() as u64,
            admission_elapsed_ms: 0,
            execution_elapsed_ms: 0,
            capabilities: Operation::ALL
                .into_iter()
                .map(|operation| {
                    (
                        operation,
                        CapabilitySupport::Unsupported {
                            reason: RefusalReason::NotQualified,
                        },
                    )
                })
                .collect(),
        };
        let admission_started = Instant::now();
        let workload = prepare()?;
        report.admission_elapsed_ms = admission_started.elapsed().as_millis() as u64;
        let valid_budget = workload.ticket().budget == probe_budget()
            && workload.applied_budget().is_some_and(|budget| {
                budget.job_commit_bytes() == probe_budget().memory_bytes
                    && budget.process_commit_bytes() == 256 * MIB
                    && budget.max_processes() == 8
                    && budget.cpu_hard_cap_basis_points() == 1000
            });
        if !valid_budget || admission_started.elapsed() >= ADMISSION_LIMIT {
            report.issue = Some(if valid_budget {
                ProbeIssue::Expired
            } else {
                ProbeIssue::AdmissionRefused
            });
            return Ok(self.prepared_failure(database, workload, report));
        }
        // Admission can take time and establishes a new prepared_at timestamp.
        // Never activate with the timestamp captured before that reservation.
        let now = match utc_ms() {
            Ok(now) => now,
            Err(issue) => {
                report.issue = Some(issue);
                return Ok(self.prepared_failure(database, workload, report));
            }
        };
        let spawned = spawn_contained(command, workload, database, now, now + 10_000);
        let mut owned = match spawned {
            Ok(spawned) => spawned,
            Err(failure) => {
                report.issue = Some(ProbeIssue::SpawnFailed);
                return Ok(self.prepared_failure(database, failure.workload, report));
            }
        };
        // Hashing, private ACL preparation and admission have their own finite
        // budgets. The child receives its full observation window, rather than
        // inheriting the time already spent before CreateProcess.
        let execution_started = Instant::now();
        let execution_deadline = execution_started + WALL_LIMIT;
        drop(owned.stdin);
        let mut output = Vec::with_capacity(MAX_OUTPUT_BYTES);
        let mut scratch = [0u8; 8192];
        let (mut stdout_done, mut stderr_done) = (false, false);
        let mut exit = None;
        let mut total = 0;
        let credits = owned
            .process
            .queue_credits()
            .acquire(MAX_OUTPUT_BYTES as u64, 2)
            .ok();
        if credits.is_none() {
            report.issue = Some(ProbeIssue::OutputLimit);
        } else {
            loop {
                let stdout = drain(
                    &mut owned.stdout,
                    &mut output,
                    &mut scratch,
                    &mut total,
                    true,
                );
                match stdout {
                    Ok(done) => stdout_done |= done,
                    Err(issue) => {
                        report.issue = Some(issue);
                        break;
                    }
                }
                // Discard stderr after counting it against the SAME total cap.
                let stderr = drain(
                    &mut owned.stderr,
                    &mut output,
                    &mut scratch,
                    &mut total,
                    false,
                );
                match stderr {
                    Ok(done) => stderr_done |= done,
                    Err(issue) => {
                        report.issue = Some(issue);
                        break;
                    }
                }
                // Only stdout is eligible as a CLI version. stderr is never logged.
                match owned.process.try_wait() {
                    Ok(code) => exit = code,
                    Err(_) => {
                        report.issue = Some(ProbeIssue::SpawnFailed);
                        break;
                    }
                }
                if exit.is_some() && stdout_done && stderr_done {
                    break;
                }
                if Instant::now() >= execution_deadline {
                    report.issue = Some(ProbeIssue::Expired);
                    break;
                }
                let _ = owned.process.sample(&WindowsSampler::new());
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if report.issue.is_none() {
            if exit != Some(0) {
                report.issue = Some(ProbeIssue::NonzeroExit);
            } else {
                report.version = parse_version(&output);
                if report.version.is_none() {
                    report.issue = Some(ProbeIssue::InvalidOutput);
                }
            }
        }
        drop(output);
        drop(credits);
        drop(owned.stdout);
        drop(owned.stderr);
        report.execution_elapsed_ms = execution_started.elapsed().as_millis() as u64;
        let cleanup = owned.process.finish_cleanup(database, CLEANUP_LIMIT);
        let released = matches!(cleanup, Ok(CleanupStatus::Released { .. }));
        report.cleanup = if released {
            ProbeCleanup::Released
        } else {
            ProbeCleanup::Retained
        };
        if !released && report.issue.is_none() {
            report.issue = Some(ProbeIssue::CleanupUnconfirmed);
        }
        report.elapsed_ms = self.started.elapsed().as_millis() as u64;
        if let Ok(status @ CleanupStatus::Released { .. }) = cleanup {
            drop(owned.process);
            let scratch = self.cleanup_scratch(status);
            Ok(ClaudeProbeOutcome {
                summary: report,
                retained: None,
                _probe: None,
                scratch,
            })
        } else {
            Ok(ClaudeProbeOutcome {
                summary: report,
                retained: Some(Retained::Process(owned.process)),
                _probe: Some(self),
                scratch: None,
            })
        }
    }
    fn prepared_failure(
        self,
        database: &Database,
        mut workload: PreparedWorkload,
        mut report: ClaudeProbeSummary,
    ) -> ClaudeProbeOutcome {
        let cleanup = workload.finish_cleanup(database, CLEANUP_LIMIT);
        let released = matches!(cleanup, Ok(CleanupStatus::Released { .. }));
        report.cleanup = if released {
            ProbeCleanup::Released
        } else {
            ProbeCleanup::Retained
        };
        report.elapsed_ms = self.started.elapsed().as_millis() as u64;
        if let Ok(status @ CleanupStatus::Released { .. }) = cleanup {
            drop(workload);
            let scratch = self.cleanup_scratch(status);
            ClaudeProbeOutcome {
                summary: report,
                retained: None,
                _probe: None,
                scratch,
            }
        } else {
            ClaudeProbeOutcome {
                summary: report,
                retained: Some(Retained::Prepared(workload)),
                _probe: Some(self),
                scratch: None,
            }
        }
    }
    fn cleanup_scratch(self, native: CleanupStatus) -> Option<ReleasedScratch> {
        let Self {
            program,
            root,
            _private,
            base,
            mut scratch,
            ..
        } = self;
        // No C01/name/program pin from this probe remains when DELETE is opened.
        drop(program);
        drop(_private);
        drop(root);
        let report = cleanup_owned_scratch(&base, &mut scratch, native);
        (report.disposition != Disposition::Removed).then_some(ReleasedScratch {
            base,
            receipt: scratch,
            native,
        })
    }
}

fn cleanup_owned_scratch(
    base: &StorageLocation,
    scratch: &mut OwnedScratch,
    native: CleanupStatus,
) -> CleanupReport {
    let mut report = check_scratch::cleanup(base, scratch, native);
    // Sharing races can outlive the finished owner worker by a few instructions.
    // Only retry this exact in-memory receipt, at most its three fixed attempts.
    while report.issue == Some(check_scratch::Issue::InUse) && report.attempts_remaining > 0 {
        std::thread::sleep(Duration::from_millis(5));
        report = check_scratch::cleanup(base, scratch, native);
    }
    report
}

struct ReleasedScratch {
    base: StorageLocation,
    receipt: OwnedScratch,
    native: CleanupStatus,
}
fn utc_ms() -> Result<i64, ProbeIssue> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| i64::try_from(time.as_millis()).ok())
        .ok_or(ProbeIssue::Expired)
}
fn drain(
    file: &mut File,
    output: &mut Vec<u8>,
    scratch: &mut [u8],
    total: &mut usize,
    retain: bool,
) -> Result<bool, ProbeIssue> {
    let mut available = 0;
    if unsafe {
        PeekNamedPipe(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return match std::io::Error::last_os_error()
            .raw_os_error()
            .map(|value| value as u32)
        {
            Some(ERROR_BROKEN_PIPE | ERROR_NO_DATA) => Ok(true),
            _ => Err(ProbeIssue::SpawnFailed),
        };
    }
    if available == 0 {
        return Ok(false);
    }
    let count = (available as usize).min(scratch.len());
    if *total + count > MAX_OUTPUT_BYTES {
        return Err(ProbeIssue::OutputLimit);
    }
    let mut read = 0;
    if unsafe {
        ReadFile(
            file.as_raw_handle(),
            scratch.as_mut_ptr(),
            count as u32,
            &mut read,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(ProbeIssue::SpawnFailed);
    }
    *total += read as usize;
    if retain {
        output.extend_from_slice(&scratch[..read as usize]);
    }
    Ok(read == 0)
}
enum Retained {
    Prepared(PreparedWorkload),
    Process(ContainedProcess),
}
/// Only summary is serializable; keep this owner while cleanup is unconfirmed.
pub struct ClaudeProbeOutcome {
    summary: ClaudeProbeSummary,
    retained: Option<Retained>,
    _probe: Option<PreparedClaudeProbe>,
    scratch: Option<ReleasedScratch>,
}
impl ClaudeProbeOutcome {
    pub fn summary(&self) -> &ClaudeProbeSummary {
        &self.summary
    }
    pub fn cleanup_retained(&self) -> bool {
        self.retained.is_some()
    }
    pub fn scratch_retained(&self) -> bool {
        self.scratch.is_some()
    }
    pub fn retry_cleanup(&mut self, database: &Database) -> bool {
        let status = match self.retained.as_mut() {
            None => None,
            Some(Retained::Prepared(workload)) => workload
                .retry_cleanup(database, Duration::from_secs(1))
                .ok(),
            Some(Retained::Process(process)) => {
                process.retry_cleanup(database, Duration::from_secs(1)).ok()
            }
        };
        if let Some(native @ CleanupStatus::Released { .. }) = status {
            // Close exact process/workload handles before releasing C01 pins.
            self.retained = None;
            self.scratch = self
                ._probe
                .take()
                .and_then(|probe| probe.cleanup_scratch(native));
            self.summary.cleanup = ProbeCleanup::Released;
        }
        if self.retained.is_none() {
            if let Some(scratch) = self.scratch.as_mut() {
                let report =
                    cleanup_owned_scratch(&scratch.base, &mut scratch.receipt, scratch.native);
                if report.disposition == Disposition::Removed {
                    self.scratch = None;
                }
            }
            true
        } else {
            false
        }
    }
}
