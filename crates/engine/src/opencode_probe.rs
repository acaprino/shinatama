//! OpenCode-only version observation. No interactive launch, login, auth-file read,
//! install, model request, capability qualification or PE-version substitution.
//! The package's PE runtime version is not the CLI version. Only the contained
//! --version output is observed; authentication is never inspected.
#![cfg(windows)]
use crate::check_scratch::{self, CleanupReport, Disposition, OwnedScratch, ScratchKind};
use crate::native_launch::{NativeProgramIdentity, verify_program_pin};
use crate::resources::{
    admission::{AdmissionCoordinator, CleanupStatus, PrepareRequest, PreparedWorkload},
    process::{ContainedProcess, NativeCommand, spawn_contained},
    sampling::WindowsSampler,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
pub use shinatama_adapter::catalog::ProgramCandidate;
use shinatama_adapter::{
    AgentId, CapabilitySupport, ObservedVersion, Operation, RefusalReason, Version, VersionUnknown,
    catalog::{CatalogSnapshot, ProgramKind},
    pe_version::PeVersionMetadata,
};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    resource_reservations::{Budget, PolicyStamp},
};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fmt,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA},
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW,
        GetFileInformationByHandle, ReadFile,
    },
    System::Pipes::PeekNamedPipe,
};

pub const WALL_LIMIT: Duration = Duration::from_secs(10);
const PREPARATION_LIMIT: Duration = Duration::from_secs(10);
const ADMISSION_LIMIT: Duration = Duration::from_secs(5);
const CLEANUP_LIMIT: Duration = Duration::from_secs(1);
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
// This resolver inspects one command, unlike the multi-agent catalog. Windows
// developer PATHs commonly put npm beyond the catalog's first32 directories.
// Keep a finite directory count and the unchanged preparation time budget.
const MAX_DISCOVERY_DIRECTORIES: usize = 128;
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
pub struct OpenCodeProbeSummary {
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
    pub exit_code: Option<u32>,
    pub stdout: Option<VersionOutputSummary>,
    pub stderr: Option<VersionOutputSummary>,
    pub capabilities: BTreeMap<Operation, CapabilitySupport>,
}

/// Only a recognized single version-format line may be retained. Arbitrary
/// diagnostics, paths, environment, warnings and non-ASCII text are never kept.
#[derive(Clone, Debug, Serialize)]
pub struct VersionOutputSummary {
    pub bytes: usize,
    pub classification: &'static str,
    pub version_line: Option<String>,
}
struct BoundedVersionOutput {
    bytes: usize,
    prefix: [u8; 128],
}
impl BoundedVersionOutput {
    fn new() -> Self {
        Self {
            bytes: 0,
            prefix: [0; 128],
        }
    }
    fn observe(&mut self, bytes: &[u8]) {
        let kept = self.bytes.min(self.prefix.len());
        let count = bytes.len().min(self.prefix.len() - kept);
        self.prefix[kept..kept + count].copy_from_slice(&bytes[..count]);
        self.bytes += bytes.len();
    }
    fn summary(&self) -> VersionOutputSummary {
        let raw = &self.prefix[..self.bytes.min(self.prefix.len())];
        let mut classification = if self.bytes == 0 {
            "empty"
        } else if self.bytes > 128 {
            "over_128_bytes"
        } else if !raw.is_ascii() {
            "non_ascii"
        } else if raw
            .iter()
            .any(|byte| byte.is_ascii_control() && !matches!(byte, b'\r' | b'\n' | b'\t'))
        {
            "control_sequence"
        } else {
            "other_ascii"
        };
        let line = std::str::from_utf8(raw)
            .ok()
            .map(str::trim)
            .filter(|_| self.bytes <= 128);
        let version_line = line
            .filter(|line| {
                if line.chars().any(char::is_control) {
                    return false;
                }
                let version = [
                    "opencode ",
                    "OpenCode ",
                    "opencode-cli ",
                    "OpenCode version ",
                ]
                .into_iter()
                .find_map(|prefix| line.strip_prefix(prefix))
                .unwrap_or(line);
                Version::parse(version.strip_prefix('v').unwrap_or(version)).is_ok()
            })
            .map(str::to_owned);
        if version_line.is_some() {
            classification = "version_line";
        }
        VersionOutputSummary {
            bytes: self.bytes,
            classification,
            version_line,
        }
    }
}
/// Bounded PATH search in command order. The first match wins; only the exact
/// reviewed npm shim is resolved to its native target, never executed.
pub fn discover_opencode() -> Result<ProgramCandidate, ProbeIssue> {
    // Resolve PATH directory metadata to a physical local path; the probe
    // and launch then independently hold/hash that exact executable. No shell,
    // config/credential lookup, network share or user-supplied program is used.
    let paths = std::env::var_os("PATH").unwrap_or_default();
    let deadline = Instant::now() + PREPARATION_LIMIT;
    for directory in discovery_directories(&paths) {
        let (directory, _directory_pins) = match resolve_local_directory(&directory, deadline) {
            Ok(resolved) => resolved,
            Err(ProbeIssue::Expired) => return Err(ProbeIssue::Expired),
            Err(_) => continue,
        };
        for (extension, kind) in [
            ("exe", ProgramKind::DirectExe),
            ("cmd", ProgramKind::CmdWrapper),
            ("ps1", ProgramKind::PowerShellWrapper),
        ] {
            let path = directory.join(format!("opencode.{extension}"));
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            use std::os::windows::fs::MetadataExt;
            if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                continue;
            }
            let path = if kind == ProgramKind::DirectExe {
                path
            } else if kind == ProgramKind::CmdWrapper && recognized_npm_shim(&path)? {
                // Fixed package-relative target, never interpreted shell text.
                directory.join("node_modules/@opencode/cli/bin/opencode.exe")
            } else {
                return Err(ProbeIssue::WrapperUnresolved);
            };
            return Ok(ProgramCandidate {
                agent: AgentId::new("opencode").map_err(|_| ProbeIssue::InvalidCandidate)?,
                path,
                kind: ProgramKind::DirectExe,
                version: ObservedVersion::Unknown {
                    reason: VersionUnknown::NeedsContainedProbe,
                },
                file_version_metadata: PeVersionMetadata::NotInspected,
            });
        }
        if Instant::now() >= deadline {
            return Err(ProbeIssue::Expired);
        }
    }
    Err(ProbeIssue::NotFound)
}

fn discovery_directories(paths: &OsStr) -> impl Iterator<Item = PathBuf> + '_ {
    std::env::split_paths(paths).take(MAX_DISCOVERY_DIRECTORIES)
}

fn recognized_npm_shim(path: &Path) -> Result<bool, ProbeIssue> {
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| ProbeIssue::WrapperUnresolved)?;
    let metadata = info(&file)?;
    if metadata.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || metadata.nNumberOfLinks != 1
        || metadata.nFileSizeHigh != 0
        || metadata.nFileSizeLow > 1024
    {
        return Err(ProbeIssue::WrapperUnresolved);
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|_| ProbeIssue::WrapperUnresolved)?;
    Ok(is_npm_shim(&text))
}

fn is_npm_shim(text: &str) -> bool {
    text.replace("\r\n", "\n")
        == concat!(
            "@ECHO off\nGOTO start\n:find_dp0\nSET dp0=%~dp0\nEXIT /b\n:start\nSETLOCAL\nCALL :find_dp0\n",
            "\"%dp0%\\node_modules\\@opencode\\cli\\bin\\opencode.exe\"   %*\n"
        )
}

/// Follow at most eight LOCAL directory links, never a file link. Every parent
/// stays open without delete sharing while its child is inspected. The returned
/// physical directory is rechecked by the stricter no-reparse executable pin.
fn resolve_local_directory(
    input: &Path,
    deadline: Instant,
) -> Result<(PathBuf, Vec<File>), ProbeIssue> {
    let mut path = input.to_owned();
    let mut held = Vec::new();
    for _ in 0..=8 {
        if Instant::now() >= deadline {
            return Err(ProbeIssue::Expired);
        }
        let raw = path.to_str().ok_or(ProbeIssue::UnsafeProgram)?;
        let raw = raw.strip_prefix(r"\\?\").unwrap_or(raw);
        let bytes = raw.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || !matches!(bytes[2], b'\\' | b'/')
            || raw.len() > 4096
            || bytes.iter().skip(2).any(|byte| *byte == b':')
            || raw.chars().any(char::is_control)
            || raw
                .split(['\\', '/'])
                .any(|part| part == ".." || part == ".")
        {
            return Err(ProbeIssue::UnsafeProgram);
        }
        let drive = [u16::from(bytes[0]), u16::from(b':'), u16::from(b'\\'), 0];
        path = PathBuf::from(raw);
        if unsafe { GetDriveTypeW(drive.as_ptr()) } != 3 {
            return Err(ProbeIssue::UnsafeProgram);
        }
        let mut ancestors: Vec<_> = path.ancestors().take(65).collect();
        if ancestors.len() > 64 {
            return Err(ProbeIssue::UnsafeProgram);
        }
        ancestors.reverse();
        let mut replacement = None;
        for ancestor in ancestors {
            let file = OpenOptions::new()
                .read(true)
                .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(ancestor)
                .map_err(|_| ProbeIssue::UnsafeProgram)?;
            let metadata = info(&file)?;
            if metadata.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
                return Err(ProbeIssue::UnsafeProgram);
            }
            held.push(file);
            if metadata.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                let target = std::fs::read_link(ancestor).map_err(|_| ProbeIssue::UnsafeProgram)?;
                // Relative link targets are deliberately not adopted.
                if !target.is_absolute()
                    || target
                        .components()
                        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
                {
                    return Err(ProbeIssue::UnsafeProgram);
                }
                replacement = Some(
                    target.join(
                        path.strip_prefix(ancestor)
                            .map_err(|_| ProbeIssue::UnsafeProgram)?,
                    ),
                );
                break;
            }
        }
        match replacement {
            Some(next) => path = next,
            None => return Ok((path, held)),
        }
    }
    Err(ProbeIssue::UnsafeProgram)
}
pub fn select_opencode(catalog: &CatalogSnapshot) -> Result<ProgramCandidate, ProbeIssue> {
    let mut candidates = catalog
        .candidates
        .iter()
        .filter(|candidate| candidate.agent.as_str() == "opencode");
    let first = candidates.next().ok_or(ProbeIssue::NotFound)?;
    if first.kind != ProgramKind::DirectExe {
        return Err(ProbeIssue::WrapperUnresolved);
    }
    Ok(first.clone())
}
/// Accept only a bounded single CLI version line, never arbitrary diagnostic text.
pub fn parse_version(bytes: &[u8]) -> Option<Version> {
    shinatama_adapter::version::parse_version_output("opencode", bytes).ok()
}

struct ProgramPin {
    path: PathBuf,
    file: File,
    _ancestors: Vec<File>,
    sha256: String,
    bytes: u64,
    identity: NativeProgramIdentity,
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
        .is_some_and(|name| name.eq_ignore_ascii_case("opencode.exe"))
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
    // Deny content writes through every alias and deletion of this opened name
    // while hashing AND while CreateProcess resolves this path. Other names can
    // be unlinked; their count is checked again after admission, before spawn.
    // Ancestor handles deny rename, preventing a path swap behind the pinned file.
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| ProbeIssue::UnsafeProgram)?;
    let before = info(&file)?;
    if before.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || before.nNumberOfLinks == 0
    {
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
        || before.nNumberOfLinks != after.nNumberOfLinks
        || before.ftLastWriteTime.dwLowDateTime != after.ftLastWriteTime.dwLowDateTime
        || before.ftLastWriteTime.dwHighDateTime != after.ftLastWriteTime.dwHighDateTime
    {
        return Err(ProbeIssue::UnsafeProgram);
    }
    let path = path.canonicalize().map_err(|_| ProbeIssue::UnsafeProgram)?;
    let digest: [u8; 32] = hash.finalize().into();
    let identity = NativeProgramIdentity::opencode_program(
        before.dwVolumeSerialNumber,
        (u64::from(before.nFileIndexHigh) << 32) | u64::from(before.nFileIndexLow),
        bytes,
        (u64::from(before.ftLastWriteTime.dwHighDateTime) << 32)
            | u64::from(before.ftLastWriteTime.dwLowDateTime),
        digest,
        before.nNumberOfLinks,
    )
    .map_err(|_| ProbeIssue::UnsafeProgram)?;
    Ok(ProgramPin {
        path,
        file,
        _ancestors: held,
        sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        bytes,
        identity,
    })
}
pub struct PreparedOpenCodeProbe {
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
impl fmt::Debug for PreparedOpenCodeProbe {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("PreparedOpenCodeProbe([redacted])")
    }
}
impl PreparedOpenCodeProbe {
    /// Fresh private directories only. No existing OpenCode HOME or config is read.
    pub fn prepare(
        candidate: &ProgramCandidate,
        base: &StorageLocation,
    ) -> Result<Self, ProbeIssue> {
        if candidate.agent.as_str() != "opencode" || candidate.kind != ProgramKind::DirectExe {
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
        let (root, scratch) = check_scratch::create(base, ScratchKind::OpenCodeProbe)
            .map_err(|_| ProbeIssue::PrivateRootUnavailable)?
            .into_parts();
        let mut private = Vec::new();
        for role in [
            "home", "config", "data", "cache", "state", "cwd", "tmp", "local", "roaming",
        ] {
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
                "XDG_CONFIG_HOME",
                root.directory().join("config").into_os_string(),
            ),
            (
                "XDG_DATA_HOME",
                root.directory().join("data").into_os_string(),
            ),
            (
                "XDG_CACHE_HOME",
                root.directory().join("cache").into_os_string(),
            ),
            (
                "XDG_STATE_HOME",
                root.directory().join("state").into_os_string(),
            ),
            (
                "OPENCODE_DB",
                root.directory().join("data/opencode.db").into_os_string(),
            ),
            (
                "OPENCODE_TEST_HOME",
                root.directory().join("home").into_os_string(),
            ),
            ("OPENCODE_DISABLE_CLAUDE_CODE", "1".into()),
            ("OPENCODE_DISABLE_PROJECT_CONFIG", "1".into()),
            ("OPENCODE_DISABLE_AUTOUPDATE", "1".into()),
            ("OPENCODE_DISABLE_MODELS_FETCH", "1".into()),
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
    ) -> Result<OpenCodeProbeOutcome, ProbeIssue> {
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
    ) -> Result<OpenCodeProbeOutcome, ProbeIssue> {
        if Instant::now() >= self.deadline {
            return Err(ProbeIssue::Expired);
        }
        // Keep exact executable/ancestor handles alive through cleanup and
        // reject a link-count change since hashing before reserving resources.
        verify_program_pin(&self.program.file, &self.program.identity)
            .map_err(|_| ProbeIssue::UnsafeProgram)?;
        utc_ms()?;
        let command = NativeCommand::new(
            self.program.path.clone(),
            vec![OsString::from("--version")],
            self.root.directory().join("cwd"),
            self.environment.clone(),
        )
        .map_err(|_| ProbeIssue::InvalidCandidate)?;
        let mut report = OpenCodeProbeSummary {
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
            exit_code: None,
            stdout: None,
            stderr: None,
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
        if verify_program_pin(&self.program.file, &self.program.identity).is_err() {
            report.issue = Some(ProbeIssue::UnsafeProgram);
            return Ok(self.prepared_failure(database, workload, report));
        }
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
        let mut stdout_observed = BoundedVersionOutput::new();
        let mut stderr_observed = BoundedVersionOutput::new();
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
                    &mut stdout_observed,
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
                    &mut stderr_observed,
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
        report.exit_code = exit;
        report.stdout = Some(stdout_observed.summary());
        report.stderr = Some(stderr_observed.summary());
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
            Ok(OpenCodeProbeOutcome {
                summary: report,
                retained: None,
                _probe: None,
                scratch,
            })
        } else {
            Ok(OpenCodeProbeOutcome {
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
        mut report: OpenCodeProbeSummary,
    ) -> OpenCodeProbeOutcome {
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
            OpenCodeProbeOutcome {
                summary: report,
                retained: None,
                _probe: None,
                scratch,
            }
        } else {
            OpenCodeProbeOutcome {
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
    observed: &mut BoundedVersionOutput,
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
    observed.observe(&scratch[..read as usize]);
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
pub struct OpenCodeProbeOutcome {
    summary: OpenCodeProbeSummary,
    retained: Option<Retained>,
    _probe: Option<PreparedOpenCodeProbe>,
    scratch: Option<ReleasedScratch>,
}
impl OpenCodeProbeOutcome {
    pub fn summary(&self) -> &OpenCodeProbeSummary {
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

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_adapter::catalog::{EntryStatus, discover};
    #[test]
    fn version_output_diagnostics_retain_only_a_single_allowlisted_version_line() {
        let observed = |bytes: &[u8]| {
            let mut output = BoundedVersionOutput::new();
            for part in bytes.chunks(3) {
                output.observe(part);
            }
            output.summary()
        };
        assert_eq!(
            observed(b"opencode 2.2.5\r\n").version_line.as_deref(),
            Some("opencode 2.2.5")
        );
        for bytes in [
            b"warning token=private 2.2.5".as_slice(),
            b"C:\\private\\config.json",
            b"2.2.5\nwarning",
            b"\x1b[31m2.2.5\x1b[0m",
        ] {
            assert!(observed(bytes).version_line.is_none());
        }
        assert_eq!(observed(&[b'x'; 129]).classification, "over_128_bytes");
    }
    #[test]
    #[ignore = "explicit contained installed OpenCode --version diagnostic; private homes only"]
    fn installed_opencode_version_only_under_a_private_frozen_budget() {
        use shinatama_state::{
            location::LocationRequest, resource_reservations::ReservationPolicy,
        };
        let candidate = discover_opencode().unwrap_or_else(|issue| panic!("discovery={issue:?}"));
        let fixture =
            std::env::temp_dir().join(format!("shn-opencode-version-{}", uuid::Uuid::new_v4()));
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: fixture.clone(),
            home_override: Some(fixture.join("private")),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap_or_else(|error| panic!("private_resolve={:?}", error.kind));
        location
            .prepare_private_directory()
            .unwrap_or_else(|error| panic!("private_prepare={:?}", error.kind));
        let database = Database::open(&location, None)
            .unwrap_or_else(|_| panic!("private database unavailable"));
        let stamp = PolicyStamp {
            version: 1,
            id: "opencode-version-probe-v1".into(),
        };
        let initial = database
            .initialize_resource_policy(&ReservationPolicy {
                stamp: stamp.clone(),
                aggregate_ceiling: probe_budget(),
                per_slot_ceiling: probe_budget(),
                max_slots: 1,
                max_lease_ms: 30_000,
            })
            .unwrap_or_else(|_| panic!("private admission unavailable"));
        let admission = AdmissionCoordinator::new(1).unwrap();
        let prepared = PreparedOpenCodeProbe::prepare(&candidate, &location)
            .unwrap_or_else(|issue| panic!("prepare={issue:?}"));
        let mut outcome = prepared
            .run(&database, &admission, &stamp, initial.revision)
            .unwrap_or_else(|issue| panic!("probe={issue:?}"));
        if outcome.cleanup_retained() {
            outcome.retry_cleanup(&database);
        }
        let summary = outcome.summary();
        println!(
            "opencode_version_observation={}",
            serde_json::to_string(&serde_json::json!({
                "version": summary.version, "issue": summary.issue, "exitCode": summary.exit_code,
                "stdout": summary.stdout, "stderr": summary.stderr, "cleanup": summary.cleanup,
            }))
            .unwrap()
        );
        let preserve = outcome.cleanup_retained() || outcome.scratch_retained();
        drop(outcome);
        drop(database);
        drop(location);
        if !preserve
            && fixture.parent() == Some(std::env::temp_dir().as_path())
            && fixture
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("shn-opencode-version-"))
        {
            let _ = std::fs::remove_dir_all(fixture);
        }
    }
    /// Explicit local diagnostic: reads installed executable metadata/bytes only;
    /// prepare creates fresh private scratch but never executes --version/TUI.
    #[test]
    #[ignore = "explicit installed OpenCode preparation diagnostic; no child process"]
    fn diagnose_installed_opencode_preparation_without_execution() {
        use shinatama_state::location::LocationRequest;
        let candidate = match discover_opencode() {
            Ok(candidate) => candidate,
            Err(issue) => {
                eprintln!("discovery={issue:?}");
                return;
            }
        };
        eprintln!(
            "preflight={:?}",
            StorageLocation::preflight(&candidate.path).map_err(|error| error.kind)
        );
        let mut ancestors: Vec<_> = candidate.path.parent().unwrap().ancestors().collect();
        ancestors.reverse();
        for (index, parent) in ancestors.into_iter().enumerate() {
            let opened = OpenOptions::new()
                .read(true)
                .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(parent);
            match opened {
                Ok(file) => match info(&file) {
                    Ok(metadata) => eprintln!(
                        "ancestor={index} attributes={} links={}",
                        metadata.dwFileAttributes, metadata.nNumberOfLinks
                    ),
                    Err(issue) => eprintln!("ancestor={index} info={issue:?}"),
                },
                Err(error) => {
                    eprintln!("ancestor={index} open_os_error={:?}", error.raw_os_error())
                }
            }
        }
        match OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&candidate.path)
        {
            Ok(mut file) => {
                match info(&file) {
                    Ok(metadata) => eprintln!(
                        "program attributes={} links={} volume={} index={} bytes={} last_write={}",
                        metadata.dwFileAttributes,
                        metadata.nNumberOfLinks,
                        metadata.dwVolumeSerialNumber,
                        (u64::from(metadata.nFileIndexHigh) << 32)
                            | u64::from(metadata.nFileIndexLow),
                        (u64::from(metadata.nFileSizeHigh) << 32)
                            | u64::from(metadata.nFileSizeLow),
                        (u64::from(metadata.ftLastWriteTime.dwHighDateTime) << 32)
                            | u64::from(metadata.ftLastWriteTime.dwLowDateTime)
                    ),
                    Err(issue) => eprintln!("program info={issue:?}"),
                }
                let mut header = [0; 64];
                if file.read_exact(&mut header).is_ok() {
                    let offset = u32::from_le_bytes(header[60..64].try_into().unwrap());
                    let mut signature = [0; 4];
                    let pe = file
                        .seek(SeekFrom::Start(u64::from(offset)))
                        .and_then(|_| file.read_exact(&mut signature))
                        .is_ok()
                        && signature == *b"PE\0\0";
                    eprintln!(
                        "program mz={} pe_offset={offset} pe={pe}",
                        &header[..2] == b"MZ"
                    );
                }
            }
            Err(error) => eprintln!("program open_os_error={:?}", error.raw_os_error()),
        }
        let fixture =
            std::env::temp_dir().join(format!("shn-opencode-prepare-{}", uuid::Uuid::new_v4()));
        let private = fixture.join("private");
        let base = match StorageLocation::resolve(LocationRequest {
            local_data_directory: fixture.clone(),
            home_override: Some(private.clone()),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        }) {
            Ok(base) => base,
            Err(error) => {
                eprintln!("private_resolve={:?}", error.kind);
                return;
            }
        };
        if let Err(error) = base.prepare_private_directory() {
            eprintln!("private_prepare={:?}", error.kind);
            return;
        }
        match PreparedOpenCodeProbe::prepare(&candidate, &base) {
            Ok(probe) => {
                eprintln!("prepare=Ok");
                // No workload was created. This explicit test can dispose of its
                // fresh scratch without obtaining a native reservation receipt.
                let retained =
                    probe.cleanup_scratch(CleanupStatus::Released { ledger_revision: 0 });
                eprintln!("scratch_retained={}", retained.is_some());
            }
            Err(issue) => eprintln!("prepare={issue:?}"),
        }
        drop(base);
        // Empty-directory removal only; never recurse into an unexpected entry.
        let _ = std::fs::remove_dir(private);
        let _ = std::fs::remove_dir(fixture);
    }
    #[test]
    fn opencode_discovery_reaches_npm_after_the_catalog_prefix_without_reordering() {
        let mut directories: Vec<_> = (0..140)
            .map(|index| PathBuf::from(format!(r"C:\synthetic\path{index}")))
            .collect();
        directories[33] = PathBuf::from(r"C:\synthetic\npm");
        let paths = std::env::join_paths(&directories).unwrap();
        let discovered: Vec<_> = discovery_directories(&paths).collect();
        assert_eq!(discovered[33], directories[33]);
        assert_eq!(
            discovered.as_slice(),
            &directories[..MAX_DISCOVERY_DIRECTORIES]
        );
    }
    #[test]
    fn npm_shim_recognition_never_accepts_added_commands_or_an_alternate_target() {
        let shim = concat!(
            "@ECHO off\nGOTO start\n:find_dp0\nSET dp0=%~dp0\nEXIT /b\n:start\nSETLOCAL\nCALL :find_dp0\n",
            "\"%dp0%\\node_modules\\@opencode\\cli\\bin\\opencode.exe\"   %*\n"
        );
        assert!(is_npm_shim(&shim.replace('\n', "\r\n")));
        assert!(!is_npm_shim(&format!("{shim}echo changed\n")));
        assert!(!is_npm_shim(&shim.replace("@opencode", "@other")));
    }
    #[test]
    fn opencode_uses_path_precedence_and_never_skips_a_wrapper_to_another_installation() {
        let first = PathBuf::from(r"C:\first");
        let second = PathBuf::from(r"C:\second");
        let paths = [first.clone(), second.clone()];
        let catalog = discover(&paths, true, |path| {
            if path.ends_with("opencode.exe") {
                EntryStatus::RegularFile
            } else {
                EntryStatus::Missing
            }
        });
        assert_eq!(
            select_opencode(&catalog).unwrap().path,
            first.join("opencode.exe")
        );
        let wrapped = discover(&paths, true, |path| {
            if path == first.join("opencode.cmd") || path == second.join("opencode.exe") {
                EntryStatus::RegularFile
            } else {
                EntryStatus::Missing
            }
        });
        assert_eq!(
            select_opencode(&wrapped).unwrap_err(),
            ProbeIssue::WrapperUnresolved
        );
        assert_eq!(parse_version(b"2.2.5\r\n").unwrap().to_string(), "2.2.5");
        assert_eq!(
            parse_version(b"opencode v2.2.5\n").unwrap().to_string(),
            "2.2.5"
        );
        assert!(parse_version(b"warning\nopencode v2.2.5").is_none());
        assert!(parse_version(b"codex-cli 2.2.5").is_none());
        assert!(parse_version(b"warning\n2.2.5").is_none());
        assert!(parse_version(b"2.1.316 (Claude Code)").is_none());
    }
}
