//! C12 preparation, never execution or admission. Verify static files/ACLs on a
//! bounded worker once, not on the UI thread or each telemetry poll. No agent
//! configuration, credentials, process environment or certificate store is read.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_state::location::StorageLocation;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroU16;
use std::path::{Component, Path, PathBuf};

const PIN: &str = include_str!("../../../sidecar/source-build/provenance.lock.json");
const MAX_FILES: usize = 1024;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_NODE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparationError {
    InvalidPath,
    MissingStaticFile,
    InvalidPin,
    StaticIdentityMismatch,
    NodeIdentityMismatch,
    NodeMetadataInvalid,
    UnsupportedNativeFormat,
    PrivateRootUnavailable,
    InvalidEnvironment,
    OverlappingRoots,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pin {
    format_version: u32,
    package_name: String,
    version: String,
    upstream_commit: String,
    source_tree_sha256: String,
    artifact_tree_sha256: String,
    runtime_dependency_tree_sha256: String,
    entry: String,
    entry_sha256: String,
    source_files: Vec<FilePin>,
    artifact_files: Vec<FilePin>,
    runtime_dependency_files: Vec<FilePin>,
}
#[derive(Deserialize)]
struct FilePin {
    path: String,
    bytes: u64,
    sha256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct StaticIdentity {
    pub version: String,
    pub upstream_commit: String,
    pub source_tree_sha256: String,
    pub artifact_tree_sha256: String,
    pub runtime_dependency_tree_sha256: String,
    pub entry_sha256: String,
}
pub struct VerifiedBundle {
    root: PathBuf,
    entry: PathBuf,
    identity: StaticIdentity,
    _entry: File,
}
impl fmt::Debug for VerifiedBundle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("VerifiedBundle")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

/// Values are supplied by the reviewed bundled-runtime inventory, never PATH
/// discovery or a version invocation. Unknown metadata stays explicitly unknown.
pub struct NodePin {
    pub relative_path: PathBuf,
    pub sha256: String,
    pub version: Option<String>,
    pub modules_abi: Option<u32>,
}
pub struct VerifiedNode {
    root: PathBuf,
    executable: PathBuf,
    pin: NodePin,
    _executable: File,
}
impl fmt::Debug for VerifiedNode {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("VerifiedNode")
            .field("sha256", &self.pin.sha256)
            .field("version", &self.pin.version)
            .field("modules_abi", &self.pin.modules_abi)
            .finish_non_exhaustive()
    }
}

/// Reuses the committed preparation lock. Receipt/version labels cannot replace
/// hashes of every source/artifact/runtime-dependency file. No directory scan or
/// source-tree patch is performed. Loading/import compatibility remains unproved.
pub fn verify_prepared_bundle(root: &Path) -> Result<VerifiedBundle, PreparationError> {
    let pin: Pin = serde_json::from_str(PIN).map_err(|_| PreparationError::InvalidPin)?;
    validate_pin(&pin)?;
    let root = checked_root(root)?;
    for (entries, prefix) in [
        (&pin.source_files, ""),
        (&pin.artifact_files, ""),
        (&pin.runtime_dependency_files, "node_modules/zod"),
    ] {
        for entry in entries {
            let relative = Path::new(prefix).join(&entry.path);
            let path = regular_inside(&root, &relative)?;
            let _ = verify_hash(&path, &entry.sha256, entry.bytes, MAX_FILE_BYTES)?;
        }
    }
    let entry = regular_inside(&root, Path::new(&pin.entry))?;
    let size = pin
        .artifact_files
        .iter()
        .find(|item| item.path == pin.entry)
        .ok_or(PreparationError::InvalidPin)?
        .bytes;
    let handle = verify_hash(&entry, &pin.entry_sha256, size, MAX_FILE_BYTES)?;
    Ok(VerifiedBundle {
        root,
        entry,
        _entry: handle,
        identity: StaticIdentity {
            version: pin.version,
            upstream_commit: pin.upstream_commit,
            source_tree_sha256: pin.source_tree_sha256,
            artifact_tree_sha256: pin.artifact_tree_sha256,
            runtime_dependency_tree_sha256: pin.runtime_dependency_tree_sha256,
            entry_sha256: pin.entry_sha256,
        },
    })
}
fn validate_pin(pin: &Pin) -> Result<(), PreparationError> {
    if pin.format_version != 1
        || pin.package_name != "billion-context"
        || pin.version != "0.1.207"
        || pin.upstream_commit != "7e69b49ff4facb00b5ba6541b48d0f7fbe63d63d"
        || pin.entry != "dist/index.js"
    {
        return Err(PreparationError::InvalidPin);
    }
    let mut count = 0;
    let mut total = 0_u64;
    for (entries, tree) in [
        (&pin.source_files, &pin.source_tree_sha256),
        (&pin.artifact_files, &pin.artifact_tree_sha256),
        (
            &pin.runtime_dependency_files,
            &pin.runtime_dependency_tree_sha256,
        ),
    ] {
        if entries.is_empty() {
            return Err(PreparationError::InvalidPin);
        }
        let mut digest = Sha256::new();
        let mut previous = "";
        for entry in entries {
            validate_relative(Path::new(&entry.path))?;
            if entry.path.as_str() <= previous
                || !valid_hash(&entry.sha256)
                || entry.bytes > MAX_FILE_BYTES
            {
                return Err(PreparationError::InvalidPin);
            }
            count += 1;
            total = total
                .checked_add(entry.bytes)
                .ok_or(PreparationError::InvalidPin)?;
            digest.update(entry.path.as_bytes());
            digest.update([0]);
            digest.update(entry.sha256.as_bytes());
            digest.update([0]);
            digest.update(entry.bytes.to_string().as_bytes());
            digest.update(b"\n");
            previous = &entry.path;
        }
        if !valid_hash(tree) || format!("{:x}", digest.finalize()) != *tree {
            return Err(PreparationError::InvalidPin);
        }
    }
    if count > MAX_FILES
        || total > MAX_BUNDLE_BYTES
        || !valid_hash(&pin.entry_sha256)
        || !pin
            .artifact_files
            .iter()
            .any(|entry| entry.path == pin.entry && entry.sha256 == pin.entry_sha256)
    {
        return Err(PreparationError::InvalidPin);
    }
    Ok(())
}

/// Verifies one inventoried native Node executable without starting it. This is
/// byte/PE verification, not Node-version, addon-ABI or sidecar qualification.
pub fn verify_bundled_node(root: &Path, pin: NodePin) -> Result<VerifiedNode, PreparationError> {
    if !valid_hash(&pin.sha256)
        || pin.version.as_ref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
        })
        || pin.modules_abi == Some(0)
    {
        return Err(PreparationError::NodeMetadataInvalid);
    }
    let root = checked_root(root)?;
    let executable = regular_inside(&root, &pin.relative_path)?;
    if !executable
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("node.exe"))
    {
        return Err(PreparationError::UnsupportedNativeFormat);
    }
    let size = fs::metadata(&executable)
        .map_err(|_| PreparationError::MissingStaticFile)?
        .len();
    let mut handle = verify_hash(&executable, &pin.sha256, size, MAX_NODE_BYTES)
        .map_err(|_| PreparationError::NodeIdentityMismatch)?;
    verify_pe(&mut handle)?;
    Ok(VerifiedNode {
        root,
        executable,
        pin,
        _executable: handle,
    })
}
fn verify_pe(file: &mut File) -> Result<(), PreparationError> {
    let read_error = |_| PreparationError::UnsupportedNativeFormat;
    let mut dos = [0; 64];
    file.seek(SeekFrom::Start(0)).map_err(read_error)?;
    file.read_exact(&mut dos).map_err(read_error)?;
    let offset = u64::from(u32::from_le_bytes(
        dos[60..64].try_into().expect("fixed header"),
    ));
    if &dos[..2] != b"MZ" || !(64..=1024 * 1024).contains(&offset) {
        return Err(PreparationError::UnsupportedNativeFormat);
    }
    file.seek(SeekFrom::Start(offset)).map_err(read_error)?;
    let mut pe = [0; 24];
    file.read_exact(&mut pe).map_err(read_error)?;
    let machine = u16::from_le_bytes([pe[4], pe[5]]);
    let executable = u16::from_le_bytes([pe[22], pe[23]]);
    let native = if cfg!(target_arch = "x86_64") {
        machine == 0x8664
    } else if cfg!(target_arch = "aarch64") {
        machine == 0xaa64
    } else {
        false
    };
    if !cfg!(windows)
        || &pe[..4] != b"PE\0\0"
        || !native
        || executable & 2 == 0
        || executable & 0x2000 != 0
    {
        return Err(PreparationError::UnsupportedNativeFormat);
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeSummary {
    pub source: StaticIdentity,
    pub node_sha256: String,
    pub declared_node_version: Option<String>,
    pub declared_modules_abi: Option<u32>,
    pub node_metadata_known: bool,
    pub argv_items: usize,
    pub environment_names: Vec<String>,
    pub runtime_qualified: bool,
    pub native_start_enabled: bool,
    pub effective_configuration_verified: bool,
    pub child_certificate_trust_verified: bool,
    pub supervision_verified: bool,
    pub sc16_passed: bool,
}
pub struct SidecarRuntimeRecipe {
    bundle: VerifiedBundle,
    node: VerifiedNode,
    _private_locations: Vec<StorageLocation>,
    cwd: PathBuf,
    argv: Vec<OsString>,
    environment: BTreeMap<OsString, OsString>,
}
impl fmt::Debug for SidecarRuntimeRecipe {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("SidecarRuntimeRecipe")
            .field("summary", &self.summary())
            .finish_non_exhaustive()
    }
}
impl SidecarRuntimeRecipe {
    pub fn summary(&self) -> RuntimeSummary {
        RuntimeSummary {
            source: self.bundle.identity.clone(),
            node_sha256: self.node.pin.sha256.clone(),
            declared_node_version: self.node.pin.version.clone(),
            declared_modules_abi: self.node.pin.modules_abi,
            node_metadata_known: self.node.pin.version.is_some()
                && self.node.pin.modules_abi.is_some(),
            argv_items: self.argv.len(),
            environment_names: self
                .environment
                .keys()
                .map(|key| key.to_string_lossy().into_owned())
                .collect(),
            runtime_qualified: false,
            native_start_enabled: false,
            effective_configuration_verified: false,
            child_certificate_trust_verified: false,
            supervision_verified: false,
            sc16_passed: false,
        }
    }
    /// Deliberate, non-loggable access for the FUTURE C09R-contained executor.
    /// None of these accessors authorizes starting this unqualified recipe.
    pub fn executable(&self) -> &Path {
        &self.node.executable
    }
    pub fn arguments(&self) -> &[OsString] {
        &self.argv
    }
    pub fn working_directory(&self) -> &Path {
        &self.cwd
    }
    /// The eventual child must use env_clear plus exactly these entries. Agent
    /// placement/home preservation remains with C11, not this proxy-only process.
    pub fn child_environment(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.environment
            .iter()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str()))
    }
}

/// Private directories/configuration must already exist under the held C01
/// location. This method creates/copies nothing and reads no config contents.
///
/// `codex_home` is the private home Shinatama gives Codex. The sidecar sizes a
/// Codex conversation by the window Codex itself works with, which Codex keeps
/// there (`models_cache.json`, `config.toml`). Without it the sidecar's own
/// private HOME leaves only a bundled model table, and a model newer than that
/// table falls back to a generic 272K window: the sidecar then refuses a
/// conversation long before Codex compacts it. The sidecar reads that model
/// metadata only; the path is not created, verified or held here.
pub fn prepare_runtime(
    bundle: VerifiedBundle,
    node: VerifiedNode,
    private: &StorageLocation,
    os_snapshot: &BTreeMap<OsString, OsString>,
    port: NonZeroU16,
    codex_home: Option<&Path>,
) -> Result<SidecarRuntimeRecipe, PreparationError> {
    private
        .verify_private_directory()
        .map_err(|_| PreparationError::PrivateRootUnavailable)?;
    let root = private.directory();
    valid_absolute(root)?;
    if root.starts_with(&bundle.root)
        || bundle.root.starts_with(root)
        || root.starts_with(&node.root)
        || node.root.starts_with(root)
    {
        return Err(PreparationError::OverlappingRoots);
    }
    let mut held = vec![private.clone()];
    for relative in [
        "home",
        "config",
        "data",
        "cache",
        "state",
        "temp",
        "cache/node-compile",
        "data/sessions",
        "state/dumps",
        "home/AppData/Roaming",
        "home/AppData/Local",
    ] {
        let location = private
            .resolve_related_directory(&root.join(relative))
            .map_err(|_| PreparationError::PrivateRootUnavailable)?;
        location
            .verify_private_directory()
            .map_err(|_| PreparationError::PrivateRootUnavailable)?;
        held.push(location);
    }
    let config = root.join("config/billion-context.json");
    held[2]
        .verify_private_file(&config)
        .map_err(|_| PreparationError::PrivateRootUnavailable)?;
    // Node's Windows main-module realpath rejects canonical \\?\ drive spelling
    // (EISDIR at the drive). Keep canonical identity checks and file grants;
    // use only an equivalent local DOS spelling at the child boundary.
    let child_root = node_path(root)?;
    let child_entry = node_path(&bundle.entry)?;
    let child_config = node_path(&config)?;
    let child_node = node_path(&node.executable)?;
    let child_codex_home = codex_home.map(node_path).transpose()?;
    let environment = environment(
        &child_root,
        &child_node,
        child_codex_home.as_deref(),
        os_snapshot,
    )?;
    let argv = arguments(&child_entry, &child_config, port);
    Ok(SidecarRuntimeRecipe {
        bundle,
        node,
        _private_locations: held,
        cwd: child_root,
        argv,
        environment,
    })
}
fn node_path(path: &Path) -> Result<PathBuf, PreparationError> {
    valid_absolute(path)?;
    #[cfg(windows)]
    {
        use std::path::Prefix;
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return Err(PreparationError::InvalidPath);
        };
        match prefix.kind() {
            Prefix::Disk(_) => Ok(path.to_owned()),
            Prefix::VerbatimDisk(drive) => {
                let mut compatible = PathBuf::from(format!("{}:", char::from(drive)));
                compatible.extend(path.components().skip(1).map(|part| part.as_os_str()));
                Ok(compatible)
            }
            _ => Err(PreparationError::InvalidPath),
        }
    }
    #[cfg(not(windows))]
    Ok(path.to_owned())
}
fn arguments(entry: &Path, config: &Path, port: NonZeroU16) -> Vec<OsString> {
    // Exact canonical cli.ts parser: start (398-405), --host/--port/--config
    // (362-376), --no-auto-update (261). No launcher/harness subcommand.
    vec![
        entry.as_os_str().into(),
        "start".into(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string().into(),
        "--config".into(),
        config.as_os_str().into(),
        "--no-auto-update".into(),
    ]
}
fn environment(
    root: &Path,
    node: &Path,
    codex_home: Option<&Path>,
    snapshot: &BTreeMap<OsString, OsString>,
) -> Result<BTreeMap<OsString, OsString>, PreparationError> {
    if snapshot.len() > 2048 {
        return Err(PreparationError::InvalidEnvironment);
    }
    let mut result: BTreeMap<OsString, OsString> = BTreeMap::new();
    for name in ["SystemRoot", "WINDIR"] {
        let values: Vec<_> = snapshot
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
            .collect();
        if values.len() > 1 {
            return Err(PreparationError::InvalidEnvironment);
        }
        if let Some((_, value)) = values.first() {
            valid_absolute(Path::new(value))?;
            result.insert(name.into(), value.as_os_str().to_os_string());
        }
    }
    if cfg!(windows) && !result.contains_key(OsStr::new("SystemRoot")) {
        return Err(PreparationError::InvalidEnvironment);
    }
    for (name, relative) in [
        ("MUSASHI_TERMINAL_BILI_ROOT", ""),
        ("HORNYNESS_BILI_ROOT", ""),
        ("HOME", "home"),
        ("USERPROFILE", "home"),
        ("APPDATA", "home/AppData/Roaming"),
        ("LOCALAPPDATA", "home/AppData/Local"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
        ("TEMP", "temp"),
        ("TMP", "temp"),
        ("TMPDIR", "temp"),
        ("NODE_COMPILE_CACHE", "cache/node-compile"),
        ("BILI_CONFIG_FILE", "config/billion-context.json"),
        ("BILI_SESSIONS_DIR", "data/sessions"),
        ("ACP_DUMP_DIR", "state/dumps"),
    ] {
        result.insert(name.into(), root.join(relative).into_os_string());
    }
    // Codex's own home, never the user's ~/.codex nor one inherited from the
    // parent: only the Shinatama-owned path the caller passes is used.
    if let Some(home) = codex_home {
        valid_absolute(home)?;
        result.insert("CODEX_HOME".into(), home.as_os_str().to_os_string());
    }
    result.insert(
        "PATH".into(),
        node.parent()
            .ok_or(PreparationError::InvalidPath)?
            .as_os_str()
            .into(),
    );
    for name in [
        "ACP_AUTO_UPDATE",
        "ACP_AUTO_RESTART_ON_UPDATE",
        "BILI_ADVISORY_CHECK",
        "BILI_RELEASE_NOTES_CHECK",
    ] {
        result.insert(name.into(), "0".into());
    }
    // A conservative complete Windows environment-block ceiling, not merely
    // per-variable checks. No user-controlled extra entries are appended later.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let units = result.iter().fold(1usize, |total, (name, value)| {
            total
                .saturating_add(name.encode_wide().count())
                .saturating_add(value.encode_wide().count())
                .saturating_add(2)
        });
        if units > 32_000 {
            return Err(PreparationError::InvalidEnvironment);
        }
    }
    Ok(result)
}

/// Holds the proxy's public CA bundle for the life of an agent session: the file a session
/// was told to trust cannot be removed, renamed or replaced by another while it runs.
///
/// The proxy owns this file and rewrites it in place when it starts and the bundle is older
/// than a day or lacks its root certificate. A session must therefore never deny that write:
/// a pin that shared reading only made every later proxy start fail with `EBUSY` for as long
/// as one session stayed open, so the proxy could not be recovered (2026-11-18).
#[cfg(windows)]
pub fn pin_ca_bundle(path: &Path) -> std::io::Result<File> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetFileInformationByHandle,
    };
    let file = OpenOptions::new()
        .read(true)
        // No FILE_SHARE_DELETE: writing in place is allowed, removal and replacement are not.
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the CA bundle is not one ordinary file",
        ));
    }
    Ok(file)
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn validate_relative(path: &Path) -> Result<(), PreparationError> {
    if path.as_os_str().is_empty()
        || path.as_os_str().len() > 4096
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || path.as_os_str().as_encoded_bytes().contains(&0)
    {
        return Err(PreparationError::InvalidPath);
    }
    Ok(())
}
fn valid_absolute(path: &Path) -> Result<(), PreparationError> {
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path.as_os_str().as_encoded_bytes().contains(&0)
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(PreparationError::InvalidPath);
    }
    Ok(())
}
fn no_link(path: &Path) -> Result<(), PreparationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| PreparationError::MissingStaticFile)?;
    if metadata.file_type().is_symlink() {
        return Err(PreparationError::InvalidPath);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(PreparationError::InvalidPath);
        }
    }
    Ok(())
}
fn checked_root(root: &Path) -> Result<PathBuf, PreparationError> {
    valid_absolute(root)?;
    if root.ancestors().take(65).count() > 64 {
        return Err(PreparationError::InvalidPath);
    }
    for ancestor in root.ancestors() {
        no_link(ancestor)?;
    }
    fs::canonicalize(root).map_err(|_| PreparationError::MissingStaticFile)
}
fn regular_inside(root: &Path, relative: &Path) -> Result<PathBuf, PreparationError> {
    validate_relative(relative)?;
    let mut target = root.to_path_buf();
    for part in relative.components() {
        target.push(part.as_os_str());
        no_link(&target)?;
    }
    if !fs::metadata(&target).is_ok_and(|value| value.is_file()) {
        return Err(PreparationError::MissingStaticFile);
    }
    Ok(target)
}
fn verify_hash(
    path: &Path,
    expected: &str,
    bytes: u64,
    bound: u64,
) -> Result<File, PreparationError> {
    if !valid_hash(expected) || bytes == 0 || bytes > bound {
        return Err(PreparationError::InvalidPin);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    let mut file = options
        .open(path)
        .map_err(|_| PreparationError::MissingStaticFile)?;
    if file
        .metadata()
        .map_err(|_| PreparationError::MissingStaticFile)?
        .len()
        != bytes
    {
        return Err(PreparationError::StaticIdentityMismatch);
    }
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut remaining = bytes;
    while remaining != 0 {
        let maximum = usize::try_from(remaining.min(buffer.len() as u64)).expect("bounded buffer");
        let count = file
            .read(&mut buffer[..maximum])
            .map_err(|_| PreparationError::StaticIdentityMismatch)?;
        if count == 0 {
            return Err(PreparationError::StaticIdentityMismatch);
        }
        digest.update(&buffer[..count]);
        remaining -= count as u64;
    }
    if file
        .read(&mut buffer[..1])
        .map_err(|_| PreparationError::StaticIdentityMismatch)?
        != 0
        || format!("{:x}", digest.finalize()) != expected
    {
        return Err(PreparationError::StaticIdentityMismatch);
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_pin_identifies_the_actual_entry_and_all_three_bounded_trees() {
        let pin: Pin = serde_json::from_str(PIN).unwrap();
        validate_pin(&pin).unwrap();
        assert_eq!(pin.source_files.len(), 398);
        assert_eq!(pin.artifact_files.len(), 15);
        assert_eq!(pin.runtime_dependency_files.len(), 205);
        assert_eq!(
            pin.entry_sha256,
            "5dea8a6b2bff85b80593a3fc92bbf4e8f249a62f06403b426da2c5d216d1e070"
        );
    }
    #[test]
    fn changed_or_escaping_pins_are_refused_before_file_reads() {
        let mut pin: Pin = serde_json::from_str(PIN).unwrap();
        pin.source_files[0].sha256 = "0".repeat(64);
        assert_eq!(validate_pin(&pin), Err(PreparationError::InvalidPin));
        let mut pin: Pin = serde_json::from_str(PIN).unwrap();
        pin.artifact_files[0].path = "../private".into();
        assert_eq!(validate_pin(&pin), Err(PreparationError::InvalidPath));
    }
    #[test]
    fn fixed_start_arguments_are_loopback_only_and_cannot_accept_extra_flags() {
        let argv = arguments(
            Path::new("entry.js"),
            Path::new("private.json"),
            NonZeroU16::new(8787).unwrap(),
        );
        let values: Vec<_> = argv.iter().map(|value| value.to_str().unwrap()).collect();
        assert_eq!(
            values,
            [
                "entry.js",
                "start",
                "--host",
                "127.0.0.1",
                "--port",
                "8787",
                "--config",
                "private.json",
                "--no-auto-update"
            ]
        );
    }
    #[cfg(windows)]
    #[test]
    fn node_child_paths_use_equivalent_dos_spelling_without_losing_unicode() {
        assert_eq!(
            node_path(Path::new(r"\\?\D:\synthetic\資料 ü\index.js")).unwrap(),
            PathBuf::from(r"D:\synthetic\資料 ü\index.js")
        );
        assert_eq!(
            node_path(Path::new(r"C:\synthetic\config.json")).unwrap(),
            PathBuf::from(r"C:\synthetic\config.json")
        );
        assert_eq!(
            node_path(Path::new(r"\\?\UNC\server\share\index.js")),
            Err(PreparationError::InvalidPath)
        );
    }
    #[cfg(windows)]
    #[test]
    fn sidecar_environment_is_child_only_private_and_does_not_inherit_secrets_or_node_injection() {
        let parent: BTreeMap<OsString, OsString> = [
            ("SystemRoot", "C:\\Windows"),
            ("HOME", "C:\\synthetic-old"),
            ("NODE_OPTIONS", "synthetic-injection"),
            ("NODE_PATH", "synthetic-modules"),
            ("FAKE_PROVIDER_KEY", "synthetic-secret"),
            ("BILI_CONFIG_FILE", "C:\\old-config"),
            ("CODEX_HOME", "C:\\users\\someone\\.codex"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        let before = parent.clone();
        let child = environment(
            Path::new("C:\\private-sidecar"),
            Path::new("C:\\bundle\\node.exe"),
            None,
            &parent,
        )
        .unwrap();
        assert_eq!(parent, before);
        for forbidden in [
            "NODE_OPTIONS",
            "NODE_PATH",
            "FAKE_PROVIDER_KEY",
            "CODEX_HOME",
        ] {
            assert!(!child.contains_key(OsStr::new(forbidden)));
        }
        assert_eq!(
            child.get(OsStr::new("HOME")),
            Some(&OsString::from("C:\\private-sidecar\\home"))
        );
        assert_eq!(
            child.get(OsStr::new("BILI_CONFIG_FILE")),
            Some(&OsString::from(
                "C:\\private-sidecar\\config/billion-context.json"
            ))
        );
    }
    #[cfg(windows)]
    #[test]
    fn sidecar_reads_the_codex_home_it_is_given_and_never_the_inherited_one() {
        let parent: BTreeMap<OsString, OsString> = [
            ("SystemRoot", "C:\\Windows"),
            ("CODEX_HOME", "C:\\users\\someone\\.codex"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        let child = environment(
            Path::new("C:\\private-sidecar"),
            Path::new("C:\\bundle\\node.exe"),
            Some(Path::new("C:\\data\\agents\\codex\\home")),
            &parent,
        )
        .unwrap();
        assert_eq!(
            child.get(OsStr::new("CODEX_HOME")),
            Some(&OsString::from("C:\\data\\agents\\codex\\home"))
        );
        // The sidecar's own HOME stays private: only the Codex metadata moves.
        assert_eq!(
            child.get(OsStr::new("HOME")),
            Some(&OsString::from("C:\\private-sidecar\\home"))
        );
        for relative in ["relative\\codex\\home", "C:\\data\\..\\elsewhere"] {
            assert_eq!(
                environment(
                    Path::new("C:\\private-sidecar"),
                    Path::new("C:\\bundle\\node.exe"),
                    Some(Path::new(relative)),
                    &parent,
                ),
                Err(PreparationError::InvalidPath)
            );
        }
    }
    /// Observed on 2026-11-18: with a session open, every proxy restart ended in
    /// `EBUSY ... combined-ca.pem` and the window stayed on "Recovering" for good.
    #[cfg(windows)]
    #[test]
    fn a_pinned_ca_bundle_can_be_rewritten_by_the_proxy_but_not_removed_or_replaced() {
        let directory =
            std::env::temp_dir().join(format!("shn-ca-bundle-pin-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let bundle = directory.join("combined-ca.pem");
        fs::write(&bundle, "old bundle\n").unwrap();
        let first = pin_ca_bundle(&bundle).unwrap();
        // Two sessions hold it at once.
        let second = pin_ca_bundle(&bundle).unwrap();
        // The proxy rewrites it in place at a start, as Node's writeFileSync does.
        fs::write(&bundle, "new bundle\n").unwrap();
        assert_eq!(fs::read_to_string(&bundle).unwrap(), "new bundle\n");
        // The trust file a session was given is still not removed or swapped for another.
        assert!(fs::remove_file(&bundle).is_err());
        assert!(fs::rename(&bundle, directory.join("moved.pem")).is_err());
        fs::write(directory.join("other.pem"), "other\n").unwrap();
        assert!(fs::rename(directory.join("other.pem"), &bundle).is_err());
        assert!(pin_ca_bundle(&directory).is_err());
        drop((first, second));
        fs::remove_dir_all(&directory).unwrap();
    }
}
