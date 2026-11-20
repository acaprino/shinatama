//! Pi-only discovery and version observation. Pi is a Node script: the npm
//! `pi.cmd` shim is read as exact text but never executed; `node.exe` and the
//! package `cli.js` are pinned, hashed and run directly as an argument vector.
//! Two contained runs share one private scratch: `node --version` (Pi's Node
//! floor) and `node cli.js --version`. No TUI, login, auth-file read, install,
//! model request, provider traffic or capability qualification happens here.
#![cfg(windows)]
use crate::check_scratch::{self, Disposition, OwnedScratch, ScratchKind};
use crate::native_launch::{NativeProgramIdentity, verify_program_pin};
use crate::opencode_probe::{
    self as shared, BoundedVersionOutput, ReleasedScratch, Retained, VersionOutputSummary,
    cleanup_owned_scratch, drain, info, resolve_local_directory, utc_ms,
};
use crate::resources::{
    admission::{CleanupStatus, PreparedWorkload},
    process::{NativeCommand, spawn_contained},
    sampling::WindowsSampler,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use shinatama_adapter::{
    AgentId, CapabilitySupport, ObservedVersion, Operation, RefusalReason, Version, VersionUnknown,
    catalog::{ProgramCandidate, ProgramKind},
    pe_version::PeVersionMetadata,
    pi::{capabilities, discovery, rpc},
};
use shinatama_state::{
    database::Database, location::StorageLocation, resource_reservations::Budget,
};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fmt,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};

pub const WALL_LIMIT: Duration = Duration::from_secs(10);
const PREPARATION_LIMIT: Duration = Duration::from_secs(10);
const ADMISSION_LIMIT: Duration = Duration::from_secs(5);
const CLEANUP_LIMIT: Duration = Duration::from_secs(1);
const MAX_DISCOVERY_DIRECTORIES: usize = 128;
const MAX_NODE_BYTES: u64 = 512 * MIB;
/// The bundle entry is a small trampoline; package.json is a manifest.
const MAX_SCRIPT_BYTES: u64 = MIB;
const MIB: u64 = 1024 * 1024;
static PROBE_BUSY: AtomicBool = AtomicBool::new(false);
struct ProbeSlot;
impl Drop for ProbeSlot {
    fn drop(&mut self) {
        PROBE_BUSY.store(false, Ordering::Release);
    }
}
/// The same fixed version-probe envelope as the other harnesses.
pub fn probe_budget() -> Budget {
    Budget {
        memory_bytes: 512 * MIB,
        processes: 8,
        cpu_basis_points: 1000,
        queue_bytes: shared::MAX_OUTPUT_BYTES as u64,
        queue_items: 2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PiProbeIssue {
    Busy,
    NotFound,
    WrapperUnresolved,
    NodeMissing,
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
    NodeUnsupported,
    CleanupUnconfirmed,
}
impl From<shared::ProbeIssue> for PiProbeIssue {
    fn from(issue: shared::ProbeIssue) -> Self {
        use shared::ProbeIssue as Shared;
        match issue {
            Shared::Busy => Self::Busy,
            Shared::NotFound => Self::NotFound,
            Shared::Ambiguous | Shared::InvalidCandidate => Self::InvalidCandidate,
            Shared::WrapperUnresolved => Self::WrapperUnresolved,
            Shared::UnsafeProgram => Self::UnsafeProgram,
            Shared::ProgramTooLarge => Self::ProgramTooLarge,
            Shared::PrivateRootUnavailable => Self::PrivateRootUnavailable,
            Shared::Expired => Self::Expired,
            Shared::AdmissionRefused => Self::AdmissionRefused,
            Shared::SpawnFailed => Self::SpawnFailed,
            Shared::OutputLimit => Self::OutputLimit,
            Shared::InvalidOutput => Self::InvalidOutput,
            Shared::NonzeroExit => Self::NonzeroExit,
            Shared::CleanupUnconfirmed => Self::CleanupUnconfirmed,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authentication {
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeCleanup {
    Released,
    Retained,
}
/// Bounded diagnostics: hashes, sizes, versions and allowlisted version lines.
/// No path, environment, stderr text or authentication state is retained.
#[derive(Clone, Debug, Serialize)]
pub struct PiProbeSummary {
    pub version: Option<Version>,
    pub node_version: Option<Version>,
    pub minimum_node_version: Version,
    pub node_sha256: String,
    pub node_bytes: u64,
    pub script_sha256: String,
    pub script_bytes: u64,
    pub manifest_sha256: String,
    pub manifest_bytes: u64,
    pub authentication: Authentication,
    pub authentication_probe_performed: bool,
    pub cleanup: ProbeCleanup,
    pub issue: Option<PiProbeIssue>,
    pub elapsed_ms: u64,
    pub preparation_elapsed_ms: u64,
    pub node_exit_code: Option<u32>,
    pub node_stdout: Option<VersionOutputSummary>,
    pub exit_code: Option<u32>,
    pub stdout: Option<VersionOutputSummary>,
    pub stderr: Option<VersionOutputSummary>,
    pub capabilities: BTreeMap<Operation, CapabilitySupport>,
    /// Only for the explicit RPC control check; never part of app checks.
    pub rpc_control: Option<rpc::ControlSummary>,
}

/// Bounded PATH search in command order. Only the exact reviewed npm shim is
/// accepted; a `pi.exe` build, PowerShell wrapper or edited shim is refused.
pub fn discover_pi() -> Result<ProgramCandidate, PiProbeIssue> {
    discover_in(&std::env::var_os("PATH").unwrap_or_default())
}
fn discover_in(paths: &OsStr) -> Result<ProgramCandidate, PiProbeIssue> {
    let deadline = Instant::now() + PREPARATION_LIMIT;
    for directory in std::env::split_paths(paths).take(MAX_DISCOVERY_DIRECTORIES) {
        let (directory, _pins) = match resolve_local_directory(&directory, deadline) {
            Ok(resolved) => resolved,
            Err(shared::ProbeIssue::Expired) => return Err(PiProbeIssue::Expired),
            Err(_) => continue,
        };
        for (extension, kind) in [
            ("exe", ProgramKind::DirectExe),
            ("cmd", ProgramKind::CmdWrapper),
            ("ps1", ProgramKind::PowerShellWrapper),
        ] {
            let path = directory.join(format!("pi.{extension}"));
            if !regular_file(&path) {
                continue;
            }
            if kind != ProgramKind::CmdWrapper || !recognized_shim(&path)? {
                return Err(PiProbeIssue::WrapperUnresolved);
            }
            return Ok(ProgramCandidate {
                agent: AgentId::new("pi").map_err(|_| PiProbeIssue::InvalidCandidate)?,
                path,
                kind,
                version: ObservedVersion::Unknown {
                    reason: VersionUnknown::NeedsContainedProbe,
                },
                file_version_metadata: PeVersionMetadata::NotInspected,
            });
        }
        if Instant::now() >= deadline {
            return Err(PiProbeIssue::Expired);
        }
    }
    Err(PiProbeIssue::NotFound)
}

fn regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    })
}

fn recognized_shim(path: &Path) -> Result<bool, PiProbeIssue> {
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| PiProbeIssue::WrapperUnresolved)?;
    let metadata = info(&file)?;
    let bytes = (u64::from(metadata.nFileSizeHigh) << 32) | u64::from(metadata.nFileSizeLow);
    if metadata.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || metadata.nNumberOfLinks != 1
        || bytes > discovery::MAX_SHIM_BYTES
    {
        return Err(PiProbeIssue::WrapperUnresolved);
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|_| PiProbeIssue::WrapperUnresolved)?;
    Ok(discovery::is_npm_shim(&text))
}

/// The two fixed targets of the recognized shim. Opaque: paths stay internal.
pub struct PiInstallation {
    node: PathBuf,
    script: PathBuf,
    manifest: PathBuf,
}
impl fmt::Debug for PiInstallation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PiInstallation([redacted])")
    }
}
impl PiInstallation {
    pub fn node(&self) -> &Path {
        &self.node
    }
    pub fn script(&self) -> &Path {
        &self.script
    }
    pub fn manifest(&self) -> &Path {
        &self.manifest
    }
}

/// Re-reads the shim each time: probe and launch never trust an older reading.
/// Node follows the shim's own order: `node.exe` beside the shim, otherwise the
/// first direct `node.exe` in PATH order (never `node.cmd`, never a shell).
pub fn resolve_installation(candidate: &ProgramCandidate) -> Result<PiInstallation, PiProbeIssue> {
    resolve_in(candidate, &std::env::var_os("PATH").unwrap_or_default())
}
fn resolve_in(candidate: &ProgramCandidate, paths: &OsStr) -> Result<PiInstallation, PiProbeIssue> {
    if candidate.agent.as_str() != "pi"
        || candidate.kind != ProgramKind::CmdWrapper
        || !candidate
            .path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(discovery::SHIM_NAME))
    {
        return Err(PiProbeIssue::InvalidCandidate);
    }
    let deadline = Instant::now() + PREPARATION_LIMIT;
    let (directory, _pins) = resolve_local_directory(
        candidate
            .path
            .parent()
            .ok_or(PiProbeIssue::InvalidCandidate)?,
        deadline,
    )?;
    let shim = directory.join(discovery::SHIM_NAME);
    if !regular_file(&shim) || !recognized_shim(&shim)? {
        return Err(PiProbeIssue::WrapperUnresolved);
    }
    let script = discovery::script_path(&directory);
    let manifest = discovery::package_manifest_path(&directory);
    if !regular_file(&script) || !regular_file(&manifest) {
        return Err(PiProbeIssue::NotFound);
    }
    let colocated = discovery::colocated_node(&directory);
    let node = if regular_file(&colocated) {
        colocated
    } else {
        path_node(paths, deadline)?
    };
    Ok(PiInstallation {
        node,
        script,
        manifest,
    })
}

fn path_node(paths: &OsStr, deadline: Instant) -> Result<PathBuf, PiProbeIssue> {
    for directory in std::env::split_paths(paths).take(MAX_DISCOVERY_DIRECTORIES) {
        let (directory, _pins) = match resolve_local_directory(&directory, deadline) {
            Ok(resolved) => resolved,
            Err(shared::ProbeIssue::Expired) => return Err(PiProbeIssue::Expired),
            Err(_) => continue,
        };
        let node = directory.join("node.exe");
        if regular_file(&node) {
            return Ok(node);
        }
    }
    Err(PiProbeIssue::NodeMissing)
}

/// A held, hashed file. Content writers through every name, deletion of this
/// name and renames of every ancestor are denied while the handles live.
pub(crate) struct PinnedFile {
    pub(crate) path: PathBuf,
    pub(crate) file: File,
    /// Moved into the launch pins; the probe keeps them for its lifetime.
    #[allow(dead_code)]
    pub(crate) ancestors: Vec<File>,
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
    pub(crate) identity: NativeProgramIdentity,
}
impl fmt::Debug for PinnedFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PinnedFile([pinned])")
    }
}

/// `node.exe` must be a PE image with exactly one link: unlike OpenCode's npm
/// binary, no alias exception is qualified for Node.
pub(crate) fn pin_node(path: &Path, deadline: Instant) -> Result<PinnedFile, PiProbeIssue> {
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("node.exe"))
    {
        return Err(PiProbeIssue::InvalidCandidate);
    }
    pin_file(path, deadline, 64..=MAX_NODE_BYTES, true)
}

/// The script and manifest are plain single-link files of bounded size.
pub(crate) fn pin_script(path: &Path, deadline: Instant) -> Result<PinnedFile, PiProbeIssue> {
    if !path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("js") || extension.eq_ignore_ascii_case("json")
    }) {
        return Err(PiProbeIssue::InvalidCandidate);
    }
    pin_file(path, deadline, 1..=MAX_SCRIPT_BYTES, false)
}

fn pin_file(
    path: &Path,
    deadline: Instant,
    size: std::ops::RangeInclusive<u64>,
    portable_executable: bool,
) -> Result<PinnedFile, PiProbeIssue> {
    StorageLocation::preflight(path).map_err(|_| PiProbeIssue::UnsafeProgram)?;
    let parents: Vec<_> = path
        .parent()
        .ok_or(PiProbeIssue::UnsafeProgram)?
        .ancestors()
        .take(65)
        .collect();
    if parents.len() > 64 {
        return Err(PiProbeIssue::UnsafeProgram);
    }
    let mut ancestors = Vec::new();
    for parent in parents.into_iter().rev() {
        let file = OpenOptions::new()
            .read(true)
            .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(parent)
            .map_err(|_| PiProbeIssue::UnsafeProgram)?;
        let metadata = info(&file)?;
        if metadata.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || metadata.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        {
            return Err(PiProbeIssue::UnsafeProgram);
        }
        ancestors.push(file);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| PiProbeIssue::UnsafeProgram)?;
    let before = info(&file)?;
    if before.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || before.nNumberOfLinks != 1
    {
        return Err(PiProbeIssue::UnsafeProgram);
    }
    let bytes = (u64::from(before.nFileSizeHigh) << 32) | u64::from(before.nFileSizeLow);
    if !size.contains(&bytes) {
        return Err(PiProbeIssue::ProgramTooLarge);
    }
    if portable_executable {
        let mut dos = [0u8; 64];
        file.read_exact(&mut dos)
            .map_err(|_| PiProbeIssue::UnsafeProgram)?;
        let pe = u64::from(u32::from_le_bytes(dos[60..64].try_into().unwrap()));
        if &dos[..2] != b"MZ" || pe > MIB || pe + 4 > bytes {
            return Err(PiProbeIssue::UnsafeProgram);
        }
        file.seek(SeekFrom::Start(pe))
            .map_err(|_| PiProbeIssue::UnsafeProgram)?;
        let mut signature = [0; 4];
        file.read_exact(&mut signature)
            .map_err(|_| PiProbeIssue::UnsafeProgram)?;
        if signature != *b"PE\0\0" {
            return Err(PiProbeIssue::UnsafeProgram);
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|_| PiProbeIssue::UnsafeProgram)?;
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut read = 0;
    loop {
        if Instant::now() >= deadline {
            return Err(PiProbeIssue::Expired);
        }
        let count = file
            .read(&mut buffer)
            .map_err(|_| PiProbeIssue::UnsafeProgram)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > bytes {
            return Err(PiProbeIssue::UnsafeProgram);
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
        return Err(PiProbeIssue::UnsafeProgram);
    }
    let digest: [u8; 32] = hash.finalize().into();
    let identity = NativeProgramIdentity::new(
        before.dwVolumeSerialNumber,
        (u64::from(before.nFileIndexHigh) << 32) | u64::from(before.nFileIndexLow),
        bytes,
        (u64::from(before.ftLastWriteTime.dwHighDateTime) << 32)
            | u64::from(before.ftLastWriteTime.dwLowDateTime),
        digest,
    )
    .map_err(|_| PiProbeIssue::UnsafeProgram)?;
    Ok(PinnedFile {
        path: ordinary(
            &path
                .canonicalize()
                .map_err(|_| PiProbeIssue::UnsafeProgram)?,
        )?,
        file,
        ancestors,
        sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        bytes,
        identity,
    })
}

/// Node resolves a `\\?\` entry script as a relative module name, so the
/// canonical path is passed in its ordinary drive form (never a UNC path).
pub(crate) fn ordinary(path: &Path) -> Result<PathBuf, PiProbeIssue> {
    let raw = path.to_str().ok_or(PiProbeIssue::UnsafeProgram)?;
    let plain = raw.strip_prefix(r"\\?\").unwrap_or(raw);
    let bytes = plain.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return Err(PiProbeIssue::UnsafeProgram);
    }
    Ok(PathBuf::from(plain))
}

/// Lowercase hex of `sha256(node || script)`: the observation must match both.
pub fn combined_digest(node_sha256: &str, script_sha256: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(node_sha256.as_bytes());
    hash.update(b"\n");
    hash.update(script_sha256.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub struct PreparedPiProbe {
    node: PinnedFile,
    script: PinnedFile,
    manifest: PinnedFile,
    root: StorageLocation,
    _private: Vec<StorageLocation>,
    base: StorageLocation,
    scratch: OwnedScratch,
    started: Instant,
    deadline: Instant,
    environment: BTreeMap<OsString, OsString>,
    rpc_control: bool,
    _slot: ProbeSlot,
}
impl fmt::Debug for PreparedPiProbe {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("PreparedPiProbe([redacted])")
    }
}
struct Step {
    exit: Option<u32>,
    output: Vec<u8>,
    stdout: VersionOutputSummary,
    stderr: VersionOutputSummary,
    issue: Option<PiProbeIssue>,
    cleanup: Result<CleanupStatus, Retained>,
}
impl PreparedPiProbe {
    /// Fresh private directories only. No existing Pi agent dir, HOME or
    /// settings is read; Pi's own `--version` path is observed in a scratch dir.
    pub fn prepare(
        candidate: &ProgramCandidate,
        base: &StorageLocation,
    ) -> Result<Self, PiProbeIssue> {
        if candidate.agent.as_str() != "pi" || candidate.kind != ProgramKind::CmdWrapper {
            return Err(PiProbeIssue::InvalidCandidate);
        }
        PROBE_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| PiProbeIssue::Busy)?;
        let slot = ProbeSlot;
        let started = Instant::now();
        let deadline = started + PREPARATION_LIMIT;
        let installation = resolve_installation(candidate)?;
        let node = pin_node(installation.node(), deadline)?;
        let script = pin_script(installation.script(), deadline)?;
        let manifest = pin_script(installation.manifest(), deadline)?;
        base.verify_private_directory()
            .map_err(|_| PiProbeIssue::PrivateRootUnavailable)?;
        let (root, scratch) = check_scratch::create(base, ScratchKind::PiProbe)
            .map_err(|_| PiProbeIssue::PrivateRootUnavailable)?
            .into_parts();
        let mut private = Vec::new();
        for role in ["home", "agent", "cache", "cwd", "tmp", "local", "roaming"] {
            let location = root
                .resolve_related_directory(&root.directory().join(role))
                .map_err(|_| PiProbeIssue::PrivateRootUnavailable)?;
            location
                .prepare_private_directory()
                .map_err(|_| PiProbeIssue::PrivateRootUnavailable)?;
            private.push(location);
        }
        let system = PathBuf::from(
            std::env::var_os("SystemRoot").ok_or(PiProbeIssue::PrivateRootUnavailable)?,
        );
        StorageLocation::preflight(&system).map_err(|_| PiProbeIssue::PrivateRootUnavailable)?;
        let directory = root.directory();
        let mut environment = BTreeMap::new();
        for (name, value) in [
            ("SystemRoot", system.as_os_str().to_owned()),
            ("WINDIR", system.as_os_str().to_owned()),
            ("PATH", system.join("System32").into_os_string()),
            ("HOME", directory.join("home").into_os_string()),
            ("USERPROFILE", directory.join("home").into_os_string()),
            ("APPDATA", directory.join("roaming").into_os_string()),
            ("LOCALAPPDATA", directory.join("local").into_os_string()),
            ("TEMP", directory.join("tmp").into_os_string()),
            ("TMP", directory.join("tmp").into_os_string()),
            (
                "NODE_COMPILE_CACHE",
                directory.join("cache").into_os_string(),
            ),
        ] {
            environment.insert(OsString::from(name), value);
        }
        if Instant::now() >= deadline {
            return Err(PiProbeIssue::Expired);
        }
        Ok(Self {
            node,
            script,
            manifest,
            root,
            _private: private,
            base: base.clone(),
            scratch,
            started,
            deadline,
            environment,
            rpc_control: false,
            _slot: slot,
        })
    }
    fn verify_pins(&self) -> bool {
        verify_program_pin(&self.node.file, &self.node.identity).is_ok()
            && verify_program_pin(&self.script.file, &self.script.identity).is_ok()
            && verify_program_pin(&self.manifest.file, &self.manifest.identity).is_ok()
    }
    /// Same pins, scratch and admission as the version probe, but the second
    /// contained run is `--mode rpc` fed the fixed read-only control script
    /// (state, command list, one unsupported command). No prompt is encodable,
    /// so no model run or provider request can start. Not used by app checks.
    pub fn run_rpc_control_with(
        mut self,
        database: &Database,
        prepare: impl FnMut() -> Result<PreparedWorkload, PiProbeIssue>,
    ) -> Result<PiProbeOutcome, PiProbeIssue> {
        self.rpc_control = true;
        self.run_with(database, prepare)
    }
    /// The desktop supplies its shared runtime admission once per contained run.
    /// Each run is admitted, observed and released before the next one starts.
    pub fn run_with(
        self,
        database: &Database,
        mut prepare: impl FnMut() -> Result<PreparedWorkload, PiProbeIssue>,
    ) -> Result<PiProbeOutcome, PiProbeIssue> {
        if Instant::now() >= self.deadline {
            return Err(PiProbeIssue::Expired);
        }
        if !self.verify_pins() {
            return Err(PiProbeIssue::UnsafeProgram);
        }
        let mut report = PiProbeSummary {
            version: None,
            node_version: None,
            minimum_node_version: discovery::minimum_node(),
            node_sha256: self.node.sha256.clone(),
            node_bytes: self.node.bytes,
            script_sha256: self.script.sha256.clone(),
            script_bytes: self.script.bytes,
            manifest_sha256: self.manifest.sha256.clone(),
            manifest_bytes: self.manifest.bytes,
            authentication: Authentication::Unknown,
            authentication_probe_performed: false,
            cleanup: ProbeCleanup::Retained,
            issue: None,
            elapsed_ms: 0,
            preparation_elapsed_ms: self.started.elapsed().as_millis() as u64,
            node_exit_code: None,
            node_stdout: None,
            exit_code: None,
            stdout: None,
            stderr: None,
            rpc_control: None,
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
        let admission = Instant::now();
        let workload = prepare()?;
        let node = self.step(
            database,
            workload,
            admission.elapsed() >= ADMISSION_LIMIT,
            vec![OsString::from("--version")],
            &[],
            self.environment.clone(),
        );
        report.node_exit_code = node.exit;
        report.node_stdout = Some(node.stdout);
        let released = match node.cleanup {
            Ok(status) => status,
            Err(retained) => {
                report.issue = Some(node.issue.unwrap_or(PiProbeIssue::CleanupUnconfirmed));
                return Ok(self.retained(report, retained));
            }
        };
        report.issue = node.issue.or(
            match (node.exit, discovery::parse_node_version(&node.output)) {
                (Some(0), Some(version)) => {
                    let supported = discovery::node_supported(&version);
                    report.node_version = Some(version);
                    (!supported).then_some(PiProbeIssue::NodeUnsupported)
                }
                (Some(0), None) => Some(PiProbeIssue::InvalidOutput),
                _ => Some(PiProbeIssue::NonzeroExit),
            },
        );
        if report.issue.is_some() {
            return Ok(self.released(report, released));
        }
        let admission = Instant::now();
        let workload = match prepare() {
            Ok(workload) => workload,
            Err(issue) => {
                report.issue = Some(issue);
                return Ok(self.released(report, released));
            }
        };
        let mut environment = self.environment.clone();
        let directory = self.root.directory();
        for (name, value) in [
            (
                "PI_CODING_AGENT_DIR",
                directory.join("agent").into_os_string(),
            ),
            ("PI_OFFLINE", "1".into()),
            ("PI_SKIP_VERSION_CHECK", "1".into()),
            ("PI_TELEMETRY", "0".into()),
        ] {
            environment.insert(OsString::from(name), value);
        }
        let script = self.script.path.as_os_str().to_owned();
        let (arguments, input) = if self.rpc_control {
            (
                std::iter::once(script)
                    .chain(rpc::control_arguments())
                    .collect(),
                rpc::control_script(),
            )
        } else {
            (vec![script, OsString::from("--version")], Vec::new())
        };
        let pi = self.step(
            database,
            workload,
            admission.elapsed() >= ADMISSION_LIMIT,
            arguments,
            &input,
            environment,
        );
        report.exit_code = pi.exit;
        report.stdout = Some(pi.stdout);
        report.stderr = Some(pi.stderr);
        let released = match pi.cleanup {
            Ok(status) => status,
            Err(retained) => {
                report.issue = Some(pi.issue.unwrap_or(PiProbeIssue::CleanupUnconfirmed));
                return Ok(self.retained(report, retained));
            }
        };
        if self.rpc_control {
            let control = rpc::summarize(&pi.output);
            report.issue = pi.issue.or(match pi.exit {
                Some(0) => (!control.proven()).then_some(PiProbeIssue::InvalidOutput),
                _ => Some(PiProbeIssue::NonzeroExit),
            });
            report.rpc_control = Some(control);
            return Ok(self.released(report, released));
        }
        report.issue = pi.issue.or(match pi.exit {
            Some(0) => {
                report.version = discovery::parse_pi_version(&pi.output);
                report
                    .version
                    .is_none()
                    .then_some(PiProbeIssue::InvalidOutput)
            }
            _ => Some(PiProbeIssue::NonzeroExit),
        });
        if let (None, Some(version)) = (report.issue, report.version.as_ref()) {
            // Detection is proven by this run; launch operations still need the
            // isolated TUI trial, so they stay refused in this projection.
            report.capabilities = capabilities::report(
                version,
                capabilities::Observed {
                    detected: true,
                    tui_trial: false,
                },
            )
            .complete_report();
        }
        Ok(self.released(report, released))
    }
    fn step(
        &self,
        database: &Database,
        workload: PreparedWorkload,
        admission_slow: bool,
        arguments: Vec<OsString>,
        input: &[u8],
        environment: BTreeMap<OsString, OsString>,
    ) -> Step {
        let early = |issue: PiProbeIssue, workload: PreparedWorkload| Step {
            exit: None,
            output: Vec::new(),
            stdout: BoundedVersionOutput::new().summary(),
            stderr: BoundedVersionOutput::new().summary(),
            issue: Some(issue),
            cleanup: release_workload(database, workload),
        };
        let valid_budget = workload.ticket().budget == probe_budget()
            && workload.applied_budget().is_some_and(|budget| {
                budget.job_commit_bytes() == probe_budget().memory_bytes
                    && budget.process_commit_bytes() == 256 * MIB
                    && budget.max_processes() == 8
                    && budget.cpu_hard_cap_basis_points() == 1000
            });
        if !valid_budget {
            return early(PiProbeIssue::AdmissionRefused, workload);
        }
        if admission_slow {
            return early(PiProbeIssue::Expired, workload);
        }
        let Ok(command) = NativeCommand::new(
            self.node.path.clone(),
            arguments,
            self.root.directory().join("cwd"),
            environment,
        ) else {
            return early(PiProbeIssue::InvalidCandidate, workload);
        };
        // Admission establishes a new prepared_at timestamp: never activate
        // with one captured before that reservation.
        let now = match utc_ms() {
            Ok(now) => now,
            Err(issue) => return early(issue.into(), workload),
        };
        if !self.verify_pins() {
            return early(PiProbeIssue::UnsafeProgram, workload);
        }
        let mut owned = match spawn_contained(command, workload, database, now, now + 10_000) {
            Ok(spawned) => spawned,
            Err(failure) => return early(PiProbeIssue::SpawnFailed, failure.workload),
        };
        let mut step = Step {
            exit: None,
            output: Vec::new(),
            stdout: BoundedVersionOutput::new().summary(),
            stderr: BoundedVersionOutput::new().summary(),
            issue: None,
            cleanup: Ok(CleanupStatus::Released { ledger_revision: 0 }),
        };
        let deadline = Instant::now() + WALL_LIMIT;
        // Bounded fixed input (a few hundred bytes) fits the pipe buffer;
        // closing stdin afterwards ends Pi's RPC loop.
        let mut stdin = owned.stdin;
        if !input.is_empty() && stdin.write_all(input).and_then(|()| stdin.flush()).is_err() {
            step.issue = Some(PiProbeIssue::SpawnFailed);
        }
        drop(stdin);
        let mut scratch = [0u8; 8192];
        let (mut stdout_done, mut stderr_done) = (false, false);
        let mut total = 0;
        let mut stdout = BoundedVersionOutput::new();
        let mut stderr = BoundedVersionOutput::new();
        let credits = owned
            .process
            .queue_credits()
            .acquire(shared::MAX_OUTPUT_BYTES as u64, 2)
            .ok();
        if credits.is_none() {
            step.issue = Some(PiProbeIssue::OutputLimit);
        } else {
            loop {
                match drain(
                    &mut owned.stdout,
                    &mut step.output,
                    &mut scratch,
                    &mut total,
                    true,
                    &mut stdout,
                ) {
                    Ok(done) => stdout_done |= done,
                    Err(issue) => {
                        step.issue = Some(issue.into());
                        break;
                    }
                }
                // stderr counts against the same cap and is never retained.
                match drain(
                    &mut owned.stderr,
                    &mut Vec::new(),
                    &mut scratch,
                    &mut total,
                    false,
                    &mut stderr,
                ) {
                    Ok(done) => stderr_done |= done,
                    Err(issue) => {
                        step.issue = Some(issue.into());
                        break;
                    }
                }
                match owned.process.try_wait() {
                    Ok(code) => step.exit = code,
                    Err(_) => {
                        step.issue = Some(PiProbeIssue::SpawnFailed);
                        break;
                    }
                }
                if step.exit.is_some() && stdout_done && stderr_done {
                    break;
                }
                if Instant::now() >= deadline {
                    step.issue = Some(PiProbeIssue::Expired);
                    break;
                }
                let _ = owned.process.sample(&WindowsSampler::new());
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        step.stdout = stdout.summary();
        step.stderr = stderr.summary();
        drop(credits);
        drop(owned.stdout);
        drop(owned.stderr);
        step.cleanup = match owned.process.finish_cleanup(database, CLEANUP_LIMIT) {
            Ok(status @ CleanupStatus::Released { .. }) => {
                drop(owned.process);
                Ok(status)
            }
            _ => Err(Retained::Process(owned.process)),
        };
        step
    }
    fn released(self, mut report: PiProbeSummary, native: CleanupStatus) -> PiProbeOutcome {
        report.cleanup = ProbeCleanup::Released;
        report.elapsed_ms = self.started.elapsed().as_millis() as u64;
        let scratch = self.cleanup_scratch(native);
        PiProbeOutcome {
            summary: report,
            retained: None,
            _probe: None,
            scratch,
        }
    }
    fn retained(self, mut report: PiProbeSummary, retained: Retained) -> PiProbeOutcome {
        report.cleanup = ProbeCleanup::Retained;
        report.elapsed_ms = self.started.elapsed().as_millis() as u64;
        PiProbeOutcome {
            summary: report,
            retained: Some(retained),
            _probe: Some(self),
            scratch: None,
        }
    }
    fn cleanup_scratch(self, native: CleanupStatus) -> Option<ReleasedScratch> {
        let Self {
            node,
            script,
            manifest,
            root,
            _private,
            base,
            mut scratch,
            ..
        } = self;
        // No C01/name/program pin from this probe remains when DELETE is opened.
        drop((node, script, manifest));
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
#[allow(clippy::result_large_err)]
fn release_workload(
    database: &Database,
    mut workload: PreparedWorkload,
) -> Result<CleanupStatus, Retained> {
    match workload.finish_cleanup(database, CLEANUP_LIMIT) {
        Ok(status @ CleanupStatus::Released { .. }) => Ok(status),
        _ => Err(Retained::Prepared(workload)),
    }
}

/// Only summary is serializable; keep this owner while cleanup is unconfirmed.
pub struct PiProbeOutcome {
    summary: PiProbeSummary,
    retained: Option<Retained>,
    _probe: Option<PreparedPiProbe>,
    scratch: Option<ReleasedScratch>,
}
impl PiProbeOutcome {
    pub fn summary(&self) -> &PiProbeSummary {
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
    use crate::resources::admission::{AdmissionCoordinator, PrepareRequest};
    use shinatama_state::resource_reservations::PolicyStamp;

    const SHIM: &str = include_str!("../tests/fixtures/pi/pi.cmd.txt");

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("shn-pi-layout-{label}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        /// A directory whose name carries spaces and non-ASCII characters.
        fn npm(&self, shim: &str) -> PathBuf {
            let directory = self.0.join("Zoë Ünïcode npm");
            let script = discovery::script_path(&directory);
            std::fs::create_dir_all(script.parent().unwrap()).unwrap();
            std::fs::write(&script, b"import './cli-runtime.js';\n").unwrap();
            std::fs::write(
                discovery::package_manifest_path(&directory),
                b"{\"version\":\"1.4.0\"}",
            )
            .unwrap();
            std::fs::write(directory.join("pi.cmd"), shim).unwrap();
            directory
        }
        fn directory(&self, name: &str) -> PathBuf {
            let directory = self.0.join(name);
            std::fs::create_dir_all(&directory).unwrap();
            directory
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn path(directories: &[&Path]) -> OsString {
        std::env::join_paths(directories).unwrap()
    }

    #[test]
    fn shim_targets_resolve_to_node_and_script_without_executing_the_wrapper() {
        let fixture = Fixture::new("resolve");
        let npm = fixture.npm(SHIM);
        let runtime = fixture.directory("Node Runtime ü");
        std::fs::write(runtime.join("node.exe"), b"MZ placeholder never executed").unwrap();
        let empty = fixture.directory("empty");
        let candidate = discover_in(&path(&[&empty, &npm, &runtime])).unwrap();
        assert_eq!(candidate.kind, ProgramKind::CmdWrapper);
        assert!(candidate.path.ends_with("Zoë Ünïcode npm\\pi.cmd"));
        let installation = resolve_in(&candidate, &path(&[&empty, &npm, &runtime])).unwrap();
        assert!(installation.node().ends_with("Node Runtime ü\\node.exe"));
        assert_eq!(installation.script(), discovery::script_path(&npm));
        // The shim's first choice wins over PATH order, exactly as cmd.exe would.
        std::fs::write(npm.join("node.exe"), b"MZ colocated").unwrap();
        let installation = resolve_in(&candidate, &path(&[&runtime, &npm])).unwrap();
        assert_eq!(installation.node(), discovery::colocated_node(&npm));
    }

    #[test]
    fn edited_wrappers_and_missing_node_are_refused_without_fallback() {
        let fixture = Fixture::new("refuse");
        let npm = fixture.npm(&SHIM.replace("%*", "%* & calc"));
        assert_eq!(
            discover_in(&path(&[&npm])).unwrap_err(),
            PiProbeIssue::WrapperUnresolved
        );
        let other = Fixture::new("powershell");
        let first = other.directory("first");
        std::fs::write(first.join("pi.ps1"), b"& node cli.js").unwrap();
        let npm = other.npm(SHIM);
        // A wrapper earlier in PATH is never skipped for a later installation.
        assert_eq!(
            discover_in(&path(&[&first, &npm])).unwrap_err(),
            PiProbeIssue::WrapperUnresolved
        );
        let candidate = discover_in(&path(&[&npm])).unwrap();
        assert_eq!(
            resolve_in(&candidate, &path(&[&npm])).unwrap_err(),
            PiProbeIssue::NodeMissing
        );
        // The shim is reread for every resolution: an edit after discovery is refused.
        std::fs::write(npm.join("pi.cmd"), SHIM.replace("cli.js", "evil.js")).unwrap();
        assert_eq!(
            resolve_in(&candidate, &path(&[&npm])).unwrap_err(),
            PiProbeIssue::WrapperUnresolved
        );
        let none = other.directory("none");
        assert_eq!(
            discover_in(&path(&[&none])).unwrap_err(),
            PiProbeIssue::NotFound
        );
    }

    #[test]
    #[ignore = "explicit contained installed Pi RPC control check; no prompt, offline, private homes only"]
    fn installed_pi_rpc_reads_state_and_refuses_an_unsupported_command() {
        installed(true);
    }

    #[test]
    #[ignore = "explicit contained installed Pi --version diagnostic; private homes only"]
    fn installed_pi_version_and_node_floor_under_a_private_frozen_budget() {
        installed(false);
    }

    fn installed(rpc_control: bool) {
        use shinatama_state::{
            location::LocationRequest, resource_reservations::ReservationPolicy,
        };
        let candidate = discover_pi().unwrap_or_else(|issue| panic!("discovery={issue:?}"));
        let fixture = std::env::temp_dir().join(format!("shn-pi-version-{}", uuid::Uuid::new_v4()));
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
            id: "pi-version-probe-v1".into(),
        };
        database
            .initialize_resource_policy(&ReservationPolicy {
                stamp: stamp.clone(),
                aggregate_ceiling: probe_budget(),
                per_slot_ceiling: probe_budget(),
                max_slots: 1,
                max_lease_ms: 30_000,
            })
            .unwrap_or_else(|_| panic!("private admission unavailable"));
        let admission = AdmissionCoordinator::new(1).unwrap();
        let prepared = PreparedPiProbe::prepare(&candidate, &location)
            .unwrap_or_else(|issue| panic!("prepare={issue:?}"));
        let admit = || {
            let now = utc_ms().map_err(PiProbeIssue::from)?;
            let revision = database
                .resource_reservations()
                .ok()
                .flatten()
                .map(|ledger| ledger.revision)
                .ok_or(PiProbeIssue::AdmissionRefused)?;
            admission
                .prepare(
                    &database,
                    &stamp,
                    revision,
                    PrepareRequest {
                        budget: probe_budget(),
                        process_commit_bytes: 256 * MIB,
                        now_ms: now,
                        lease_expires_at_ms: now + 10_000,
                    },
                )
                .map_err(|_| PiProbeIssue::AdmissionRefused)
        };
        let mut outcome = if rpc_control {
            prepared.run_rpc_control_with(&database, admit)
        } else {
            prepared.run_with(&database, admit)
        }
        .unwrap_or_else(|issue| panic!("probe={issue:?}"));
        if outcome.cleanup_retained() {
            outcome.retry_cleanup(&database);
        }
        let summary = outcome.summary();
        println!(
            "pi_version_observation={}",
            serde_json::to_string(&serde_json::json!({
                "version": summary.version, "nodeVersion": summary.node_version,
                "issue": summary.issue, "nodeExitCode": summary.node_exit_code,
                "exitCode": summary.exit_code, "stdout": summary.stdout,
                "nodeStdout": summary.node_stdout, "cleanup": summary.cleanup,
                "elapsedMs": summary.elapsed_ms, "rpcControl": summary.rpc_control,
            }))
            .unwrap()
        );
        assert_eq!(summary.issue, None);
        assert!(summary.node_version.is_some());
        if rpc_control {
            assert!(
                summary
                    .rpc_control
                    .as_ref()
                    .is_some_and(rpc::ControlSummary::proven)
            );
            assert!(summary.version.is_none());
        } else {
            assert!(summary.version.is_some() && summary.rpc_control.is_none());
        }
        assert_eq!(summary.cleanup, ProbeCleanup::Released);
        let preserve = outcome.cleanup_retained() || outcome.scratch_retained();
        drop(outcome);
        drop(database);
        drop(location);
        if !preserve
            && fixture.parent() == Some(std::env::temp_dir().as_path())
            && fixture
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("shn-pi-version-"))
        {
            let _ = std::fs::remove_dir_all(fixture);
        }
    }
}
