//! Marionette, the one plugin every real session of Claude Code, Codex, Pi and
//! OpenCode gets. It is built by Muro (github.com/acaprino/muro) and published
//! there as a release asset; it carries what Ponytail and i-have-adhd used to
//! bring natively. At every app start [`sync`] reads the repository's latest
//! published release and, when the tag is new, downloads its `marionette*.zip`,
//! checks the SHA-256 GitHub reports for it, extracts it beside the previous
//! copy and switches to it. Launches read only that local copy: every file of
//! the adapter they hand to a harness is checked against the lock written at
//! extraction. Offline or on any doubt the last good copy stays in use; with
//! no good copy a session starts without Marionette. Nothing is installed in
//! the user's own configuration.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_adapter::sets::{SET_SCHEMA_VERSION, SetEntry, SetKind, SetManifest};
use std::{
    collections::BTreeSet,
    fmt, fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// `owner/name` of the repository whose latest release is followed.
pub const REPOSITORY: &str = "acaprino/muro";
pub const SET_ID: &str = "marionette";
pub const PLUGIN: &str = "marionette";
pub const MARKETPLACE_NAME: &str = "shinatama";
/// Folder under the Shinatama home that holds the synced copies.
pub const STORE: &str = "marionette";
/// `SHINATAMA_MARIONETTE=off` starts every session without Marionette and
/// skips the sync.
pub const SWITCH: &str = "SHINATAMA_MARIONETTE";
const CURRENT_FILE: &str = "current.json";
const LOCK_FILE: &str = "lock.json";
const SYNC_LOCK_FILE: &str = "sync.lock";
const RELEASES: &str = "releases";
const TREE: &str = "marionette";
const MARKETPLACE_FILE: &str = ".agents/plugins/marketplace.json";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_LOCK_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TREE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FILES: usize = 16 * 1024;
const MAX_DEPTH: usize = 24;

/// The harnesses with a Marionette adapter Shinatama launches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    ClaudeCode,
    Codex,
    OpenCode,
    Pi,
}
impl Host {
    pub const ALL: [Host; 4] = [Host::ClaudeCode, Host::Codex, Host::OpenCode, Host::Pi];
    /// Directory of this host's adapter under `marionette/adapters`.
    pub fn adapter(self) -> &'static str {
        match self {
            Host::ClaudeCode => "claude-code",
            Host::Codex => "codex",
            Host::OpenCode => "opencode",
            Host::Pi => "pi",
        }
    }
    fn prefix(self) -> String {
        format!("{TREE}/adapters/{}/", self.adapter())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarionetteError {
    Missing,
    Lock,
    UnsafePath,
    Mismatch,
    Unexpected,
    Limit,
    Release,
    Network,
    Archive,
    Busy,
}
impl fmt::Display for MarionetteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "Marionette file is missing",
            Self::Lock => "Marionette lock is invalid",
            Self::UnsafePath => "Marionette path is not an ordinary file",
            Self::Mismatch => "Marionette file does not match its recorded digest",
            Self::Unexpected => "Marionette adapter holds a file the lock does not list",
            Self::Limit => "Marionette release exceeds its fixed bound",
            Self::Release => "the latest Marionette release has no usable asset",
            Self::Network => "the Marionette release could not be downloaded",
            Self::Archive => "the Marionette archive could not be extracted",
            Self::Busy => "another Shinatama window is syncing Marionette",
        })
    }
}
impl std::error::Error for MarionetteError {}

/// The one set the desktop selects for a real session.
pub fn marionette_set() -> SetManifest {
    let entries = vec![SetEntry::new(SetKind::Plugin, PLUGIN, 1).expect("static plugin id")];
    SetManifest::new(SET_SCHEMA_VERSION, SET_ID, entries).expect("static Marionette set")
}

/// Launchers accept this exact set and the empty set, nothing else.
pub fn is_marionette_set(set: &SetManifest) -> bool {
    set.id() == SET_ID
        && set.entries().len() == 1
        && set.entries().iter().all(|e| {
            e.kind() == SetKind::Plugin && e.id() == PLUGIN && e.revision() == 1
        })
}

/// `SHINATAMA_MARIONETTE=off` in the app's environment.
pub fn switched_off() -> bool {
    std::env::var(SWITCH).is_ok_and(|value| value.eq_ignore_ascii_case("off"))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Current {
    format_version: u32,
    tag: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Lock {
    format_version: u32,
    repository: String,
    tag: String,
    version: String,
    asset: LockedAsset,
    files: Vec<LockedFile>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LockedAsset {
    name: String,
    bytes: u64,
    sha256: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LockedFile {
    path: String,
    bytes: u64,
    sha256: String,
}

/// The release a launch uses: `current.json` and its lock read, nothing hashed yet.
#[derive(Clone, Debug)]
pub struct Marionette {
    release: PathBuf,
    tag: String,
    version: String,
    files: Vec<(String, u64, String)>,
}
impl Marionette {
    /// The release `current.json` names inside `store`.
    pub fn current(store: &Path) -> Result<Self, MarionetteError> {
        let store = ordinary_directory(store)?;
        let bytes = read_bounded(&store.join(CURRENT_FILE), MAX_METADATA_BYTES)?;
        let current: Current =
            serde_json::from_slice(&bytes).map_err(|_| MarionetteError::Lock)?;
        if current.format_version != 1 || !valid_tag(&current.tag) {
            return Err(MarionetteError::Lock);
        }
        Self::open(&store.join(RELEASES).join(&current.tag), &current.tag)
    }
    fn open(release: &Path, tag: &str) -> Result<Self, MarionetteError> {
        let release = ordinary_directory(release)?;
        let bytes = read_bounded(&release.join(LOCK_FILE), MAX_LOCK_BYTES)?;
        let lock: Lock = serde_json::from_slice(&bytes).map_err(|_| MarionetteError::Lock)?;
        if lock.format_version != 1
            || lock.repository != REPOSITORY
            || lock.tag != tag
            || !valid_version(&lock.version)
            || lock.files.len() > MAX_FILES
        {
            return Err(MarionetteError::Lock);
        }
        let mut seen = BTreeSet::new();
        let mut files = Vec::with_capacity(lock.files.len());
        for file in lock.files {
            join_relative(&release, &file.path)?;
            if !seen.insert(file.path.clone()) || !valid_digest(&file.sha256) {
                return Err(MarionetteError::Lock);
            }
            files.push((file.path, file.bytes, file.sha256));
        }
        Ok(Self { release, tag: lock.tag, version: lock.version, files })
    }
    pub fn tag(&self) -> &str {
        &self.tag
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Checks every file of one host's adapter (and, for Codex, the marketplace
    /// file) against the lock; a file the lock does not list is refused.
    pub fn verify(&self, host: Host) -> Result<Adapter, MarionetteError> {
        let prefix = host.prefix();
        let dir = join_relative(&self.release, prefix.trim_end_matches('/'))?;
        ordinary_directory(&dir)?;
        let mut expected = BTreeSet::new();
        let mut files = Vec::new();
        for (path, bytes, sha256) in &self.files {
            let inside = path.strip_prefix(&prefix);
            if inside.is_none() && !(host == Host::Codex && path == MARKETPLACE_FILE) {
                continue;
            }
            let content = read_bounded(&join_relative(&self.release, path)?, MAX_FILE_BYTES)?;
            if content.len() as u64 != *bytes || hex(&content) != *sha256 {
                return Err(MarionetteError::Mismatch);
            }
            if let Some(relative) = inside {
                expected.insert(relative.to_owned());
                files.push(relative.to_owned());
            }
        }
        if expected.is_empty() {
            return Err(MarionetteError::Lock);
        }
        let mut found = BTreeSet::new();
        collect(&dir, "", 0, &mut found)?;
        if found != expected {
            return Err(MarionetteError::Unexpected);
        }
        Ok(Adapter {
            host,
            dir,
            marketplace_root: self.release.clone(),
            version: self.version.clone(),
            files,
        })
    }
    /// Every host verified, the check [`sync`] runs before keeping a copy.
    fn verify_all(&self) -> Result<(), MarionetteError> {
        for host in Host::ALL {
            self.verify(host)?;
        }
        Ok(())
    }
}

/// One host's adapter whose files were just verified.
#[derive(Clone, Debug)]
pub struct Adapter {
    host: Host,
    dir: PathBuf,
    marketplace_root: PathBuf,
    version: String,
    files: Vec<String>,
}
impl Adapter {
    pub fn host(&self) -> Host {
        self.host
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    /// Ordinary drive path without the extended `\\?\` prefix, as the harness
    /// command lines and hook shells expect.
    pub fn dir_arg(&self) -> String {
        plain(&self.dir)
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Relative paths of the adapter's files, as locked.
    pub fn files(&self) -> &[String] {
        &self.files
    }
    /// Claude Code writes into every `--plugin-dir` it loads (a `tsconfig.json`
    /// and `.claude-plugin/types/` with Claude 2.1.322), so Claude gets a private
    /// copy under `destination`: the verified files are mirrored there and the
    /// returned adapter points at the copy.
    pub fn private_copy(&self, destination: &Path) -> Result<Adapter, MarionetteError> {
        self.mirror(destination)?;
        Ok(Adapter { dir: destination.to_path_buf(), ..self.clone() })
    }
    /// Codex reads an installed plugin from
    /// `<CODEX_HOME>/plugins/cache/<marketplace>/<plugin>/<version>`. Mirror the
    /// verified files there so the plugin counts as installed without running
    /// `codex plugin add`.
    pub fn install_codex_cache(&self, codex_home: &Path) -> Result<(), MarionetteError> {
        let target = codex_home
            .join("plugins")
            .join("cache")
            .join(MARKETPLACE_NAME)
            .join(PLUGIN)
            .join(&self.version);
        self.mirror(&target)
    }
    /// The `-c` overrides that register Shinatama's marketplace and enable Marionette.
    pub fn codex_overrides(&self) -> Vec<String> {
        let root = plain(&self.marketplace_root).replace('\\', "/");
        vec![
            "-c".into(),
            format!(
                "marketplaces.{MARKETPLACE_NAME}={{source_type=\"local\",source={}}}",
                serde_json::to_string(&root).unwrap_or_default()
            ),
            "-c".into(),
            format!("plugins={{\"{PLUGIN}@{MARKETPLACE_NAME}\"={{enabled=true}}}}"),
        ]
    }
    /// Identical files are left alone, changed ones rewritten, files the
    /// adapter does not have removed (a previous release's leftovers).
    fn mirror(&self, target: &Path) -> Result<(), MarionetteError> {
        let wanted: BTreeSet<&str> = self.files.iter().map(String::as_str).collect();
        if target.is_dir() {
            let mut present = BTreeSet::new();
            if collect(target, "", 0, &mut present).is_ok() {
                for stale in present.iter().filter(|path| !wanted.contains(path.as_str())) {
                    if let Ok(path) = join_relative(target, stale) {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
        for relative in &self.files {
            let bytes = read_bounded(&join_relative(&self.dir, relative)?, MAX_FILE_BYTES)?;
            let path = join_relative(target, relative)?;
            if fs::read(&path).is_ok_and(|existing| existing == bytes) {
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|_| MarionetteError::Missing)?;
            }
            fs::write(&path, bytes).map_err(|_| MarionetteError::Missing)?;
        }
        Ok(())
    }
}

/// What a sync found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncOutcome {
    /// The latest release was already the current, intact copy.
    UpToDate(String),
    /// A new copy was installed and is now current.
    Updated(String),
}

/// Network and archive operations, apart so the sync logic runs in tests.
pub trait Fetch {
    /// Body of `GET https://api.github.com/repos/<repository>/releases/latest`.
    fn latest_release(&self, repository: &str) -> Result<Vec<u8>, MarionetteError>;
    /// Download `url` to `destination`, at most `limit` bytes.
    fn download(&self, url: &str, destination: &Path, limit: u64) -> Result<(), MarionetteError>;
    /// Extract the zip `archive` into the empty directory `destination`.
    fn extract(&self, archive: &Path, destination: &Path) -> Result<(), MarionetteError>;
}

#[derive(Deserialize)]
struct ReleaseMetadata {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetMetadata>,
}
#[derive(Deserialize)]
struct AssetMetadata {
    name: String,
    size: u64,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// Follow the latest published release of [`REPOSITORY`] into `store`.
pub fn sync(store: &Path, fetch: &dyn Fetch) -> Result<SyncOutcome, MarionetteError> {
    fs::create_dir_all(store.join(RELEASES)).map_err(|_| MarionetteError::Missing)?;
    let store = ordinary_directory(store)?;
    let guard = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(store.join(SYNC_LOCK_FILE))
        .map_err(|_| MarionetteError::Missing)?;
    guard.try_lock().map_err(|_| MarionetteError::Busy)?;

    let metadata = fetch.latest_release(REPOSITORY)?;
    if metadata.len() as u64 > MAX_METADATA_BYTES {
        return Err(MarionetteError::Limit);
    }
    let release: ReleaseMetadata =
        serde_json::from_slice(&metadata).map_err(|_| MarionetteError::Release)?;
    let tag = release.tag_name.clone();
    if release.draft || release.prerelease || !valid_tag(&tag) {
        return Err(MarionetteError::Release);
    }
    let asset = choose_asset(&release)?;
    let previous = Marionette::current(&store).ok();
    if let Some(current) = &previous
        && current.tag == tag
        && current.verify_all().is_ok()
    {
        return Ok(SyncOutcome::UpToDate(tag));
    }
    let target = store.join(RELEASES).join(&tag);
    let reusable = Marionette::open(&target, &tag)
        .and_then(|copy| copy.verify_all().map(|()| copy))
        .is_ok();
    if !reusable {
        install(&store, &target, &tag, asset, fetch)?;
    }
    write_current(&store, &tag)?;
    prune(&store, &tag, previous.as_ref().map(|p| p.tag.as_str()));
    Ok(SyncOutcome::Updated(tag))
}

struct ChosenAsset<'a> {
    name: &'a str,
    size: u64,
    url: &'a str,
    sha256: String,
}
fn choose_asset(release: &ReleaseMetadata) -> Result<ChosenAsset<'_>, MarionetteError> {
    let mut chosen = release.assets.iter().filter(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name.starts_with("marionette") && name.ends_with(".zip")
    });
    let asset = chosen.next().ok_or(MarionetteError::Release)?;
    if chosen.next().is_some() {
        return Err(MarionetteError::Release);
    }
    let prefix = format!("https://github.com/{REPOSITORY}/releases/download/");
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| valid_digest(digest))
        .ok_or(MarionetteError::Release)?;
    if !asset.browser_download_url.starts_with(&prefix)
        || asset.size == 0
        || asset.size > MAX_ASSET_BYTES
        || asset.name.len() > 128
    {
        return Err(MarionetteError::Release);
    }
    Ok(ChosenAsset {
        name: &asset.name,
        size: asset.size,
        url: &asset.browser_download_url,
        sha256: sha256.to_owned(),
    })
}

fn install(
    store: &Path,
    target: &Path,
    tag: &str,
    asset: ChosenAsset<'_>,
    fetch: &dyn Fetch,
) -> Result<(), MarionetteError> {
    let archive = store.join("download.zip.part");
    let staging = store.join("staging");
    let _ = fs::remove_file(&archive);
    let _ = fs::remove_dir_all(&staging);
    let result = (|| {
        fetch.download(asset.url, &archive, asset.size)?;
        let (bytes, sha256) = digest_file(&archive)?;
        if bytes != asset.size || sha256 != asset.sha256 {
            return Err(MarionetteError::Mismatch);
        }
        fs::create_dir_all(&staging).map_err(|_| MarionetteError::Missing)?;
        fetch.extract(&archive, &staging)?;
        let version = adapter_version(&staging)?;
        write_marketplace(&staging)?;
        let mut found = BTreeSet::new();
        collect(&staging, "", 0, &mut found)?;
        let mut files = Vec::with_capacity(found.len());
        let mut total = 0u64;
        for path in found {
            let keep = path.starts_with(&format!("{TREE}/")) || path == MARKETPLACE_FILE;
            if !keep {
                return Err(MarionetteError::Unexpected);
            }
            let (bytes, sha256) = digest_file(&join_relative(&staging, &path)?)?;
            total += bytes;
            if bytes > MAX_FILE_BYTES || total > MAX_TREE_BYTES {
                return Err(MarionetteError::Limit);
            }
            files.push(LockedFile { path, bytes, sha256 });
        }
        for host in Host::ALL {
            if !files.iter().any(|file| file.path.starts_with(&host.prefix())) {
                return Err(MarionetteError::Archive);
            }
        }
        let lock = Lock {
            format_version: 1,
            repository: REPOSITORY.into(),
            tag: tag.into(),
            version,
            asset: LockedAsset {
                name: asset.name.into(),
                bytes: asset.size,
                sha256: asset.sha256.clone(),
            },
            files,
        };
        let text = serde_json::to_vec_pretty(&lock).map_err(|_| MarionetteError::Lock)?;
        fs::write(staging.join(LOCK_FILE), text).map_err(|_| MarionetteError::Missing)?;
        Marionette::open(&staging, tag)?.verify_all()?;
        if target.exists() {
            fs::remove_dir_all(target).map_err(|_| MarionetteError::Missing)?;
        }
        fs::rename(&staging, target).map_err(|_| MarionetteError::Missing)
    })();
    let _ = fs::remove_file(&archive);
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// The Codex adapter's manifest version names the Codex cache folder.
fn adapter_version(staging: &Path) -> Result<String, MarionetteError> {
    let path = staging.join(TREE).join("adapters/codex/.codex-plugin/plugin.json");
    let bytes = read_bounded(&path, MAX_METADATA_BYTES).map_err(|_| MarionetteError::Archive)?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| MarionetteError::Archive)?;
    let version = manifest
        .get("version")
        .and_then(serde_json::Value::as_str)
        .filter(|version| valid_version(version))
        .ok_or(MarionetteError::Archive)?;
    Ok(version.to_owned())
}
/// The local marketplace Codex registers: one plugin, the Codex adapter.
fn write_marketplace(staging: &Path) -> Result<(), MarionetteError> {
    let marketplace = serde_json::json!({
        "name": MARKETPLACE_NAME,
        "interface": { "displayName": "Shinatama" },
        "plugins": [{
            "name": PLUGIN,
            "source": { "source": "local", "path": format!("./{TREE}/adapters/codex") },
            "policy": { "installation": "AVAILABLE", "authentication": "ON_INSTALL" },
            "category": "Productivity"
        }]
    });
    let path = join_relative(staging, MARKETPLACE_FILE)?;
    if path.exists() {
        return Err(MarionetteError::Unexpected);
    }
    fs::create_dir_all(path.parent().ok_or(MarionetteError::Missing)?)
        .map_err(|_| MarionetteError::Missing)?;
    let text = serde_json::to_vec_pretty(&marketplace).map_err(|_| MarionetteError::Lock)?;
    fs::write(path, text).map_err(|_| MarionetteError::Missing)
}
fn write_current(store: &Path, tag: &str) -> Result<(), MarionetteError> {
    let current = Current { format_version: 1, tag: tag.into() };
    let text = serde_json::to_vec_pretty(&current).map_err(|_| MarionetteError::Lock)?;
    let temporary = store.join("current.json.part");
    fs::write(&temporary, text).map_err(|_| MarionetteError::Missing)?;
    // Replaces the old file in one step (MoveFileEx with REPLACE_EXISTING).
    fs::rename(&temporary, store.join(CURRENT_FILE)).map_err(|_| MarionetteError::Missing)
}
/// Keeps the new copy and the one before it (a running session may still read
/// it); older copies are removed.
fn prune(store: &Path, current: &str, previous: Option<&str>) {
    let Ok(entries) = fs::read_dir(store.join(RELEASES)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name != current && Some(name.as_ref()) != previous {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// `curl.exe` and `tar.exe` from System32 (Windows 10 1803 and later): no
/// network library in the app, proxies from the environment, no console window.
pub struct SystemFetch;
impl SystemFetch {
    fn tool(name: &str) -> Result<PathBuf, MarionetteError> {
        let root = std::env::var_os("SystemRoot").ok_or(MarionetteError::Network)?;
        let path = PathBuf::from(root).join("System32").join(name);
        if path.is_file() { Ok(path) } else { Err(MarionetteError::Network) }
    }
    fn run(program: &Path, args: &[&std::ffi::OsStr]) -> Result<Vec<u8>, ()> {
        let mut command = std::process::Command::new(program);
        command
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let output = command.output().map_err(|_| ())?;
        if output.status.success() { Ok(output.stdout) } else { Err(()) }
    }
    fn curl_base(max_time: &str, limit: u64) -> Vec<std::ffi::OsString> {
        [
            "--silent",
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-time",
            max_time,
            "--user-agent",
            "Shinatama",
        ]
        .iter()
        .map(Into::into)
        .chain(["--max-filesize".into(), limit.to_string().into()])
        .collect()
    }
}
impl Fetch for SystemFetch {
    fn latest_release(&self, repository: &str) -> Result<Vec<u8>, MarionetteError> {
        let curl = Self::tool("curl.exe")?;
        let mut args = Self::curl_base("30", MAX_METADATA_BYTES);
        for header in ["Accept: application/vnd.github+json", "X-GitHub-Api-Version: 2022-11-28"] {
            args.push("--header".into());
            args.push(header.into());
        }
        args.push(format!("https://api.github.com/repos/{repository}/releases/latest").into());
        let refs: Vec<&std::ffi::OsStr> = args.iter().map(|a| a.as_os_str()).collect();
        Self::run(&curl, &refs).map_err(|_| MarionetteError::Network)
    }
    fn download(&self, url: &str, destination: &Path, limit: u64) -> Result<(), MarionetteError> {
        let curl = Self::tool("curl.exe")?;
        let mut args = Self::curl_base("900", limit);
        args.push("--output".into());
        args.push(destination.as_os_str().to_owned());
        args.push(url.into());
        let refs: Vec<&std::ffi::OsStr> = args.iter().map(|a| a.as_os_str()).collect();
        Self::run(&curl, &refs).map(drop).map_err(|_| MarionetteError::Network)
    }
    fn extract(&self, archive: &Path, destination: &Path) -> Result<(), MarionetteError> {
        // bsdtar refuses absolute and `..` member paths by default; links are
        // refused afterwards by the tree walk, which accepts ordinary files only.
        let tar = Self::tool("tar.exe").map_err(|_| MarionetteError::Archive)?;
        let args: [&std::ffi::OsStr; 5] = [
            "-x".as_ref(),
            "-f".as_ref(),
            archive.as_os_str(),
            "-C".as_ref(),
            destination.as_os_str(),
        ];
        Self::run(&tar, &args).map(drop).map_err(|_| MarionetteError::Archive)
    }
}

fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && !tag.starts_with('.')
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+' | b'_'))
}
fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && !version.starts_with('.')
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}
fn valid_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}
fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}
fn digest_file(path: &Path) -> Result<(u64, String), MarionetteError> {
    let meta = fs::symlink_metadata(path).map_err(|_| MarionetteError::Missing)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(MarionetteError::UnsafePath);
    }
    let mut file = fs::File::open(path).map_err(|_| MarionetteError::Missing)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(|_| MarionetteError::Missing)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_ASSET_BYTES {
            return Err(MarionetteError::Limit);
        }
        hasher.update(&buffer[..read]);
    }
    Ok((total, hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()))
}
fn ordinary_directory(path: &Path) -> Result<PathBuf, MarionetteError> {
    let meta = fs::symlink_metadata(path).map_err(|_| MarionetteError::Missing)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(MarionetteError::UnsafePath);
    }
    path.canonicalize().map_err(|_| MarionetteError::Missing)
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, MarionetteError> {
    let meta = fs::symlink_metadata(path).map_err(|_| MarionetteError::Missing)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(MarionetteError::UnsafePath);
    }
    if meta.len() > limit {
        return Err(MarionetteError::Limit);
    }
    fs::read(path).map_err(|_| MarionetteError::Missing)
}
fn join_relative(base: &Path, relative: &str) -> Result<PathBuf, MarionetteError> {
    if relative.is_empty() || relative.len() > 1024 || relative.contains('\\') {
        return Err(MarionetteError::UnsafePath);
    }
    let mut out = base.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => out.push(part),
            _ => return Err(MarionetteError::UnsafePath),
        }
    }
    Ok(out)
}
fn collect(
    dir: &Path,
    prefix: &str,
    depth: usize,
    out: &mut BTreeSet<String>,
) -> Result<(), MarionetteError> {
    if depth > MAX_DEPTH {
        return Err(MarionetteError::Limit);
    }
    for entry in fs::read_dir(dir).map_err(|_| MarionetteError::Missing)? {
        let entry = entry.map_err(|_| MarionetteError::Missing)?;
        let name = entry.file_name().into_string().map_err(|_| MarionetteError::UnsafePath)?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| MarionetteError::Missing)?;
        if meta.file_type().is_symlink() {
            return Err(MarionetteError::UnsafePath);
        }
        let relative = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if meta.is_dir() {
            collect(&entry.path(), &relative, depth + 1, out)?;
        } else if meta.is_file() {
            out.insert(relative);
            if out.len() > MAX_FILES {
                return Err(MarionetteError::Limit);
            }
        } else {
            return Err(MarionetteError::UnsafePath);
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::Cell;

    pub(crate) fn scratch(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "shinatama-marionette-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }
    /// A small release tree with every host's adapter.
    pub(crate) fn fixture_tree(root: &Path, version: &str) {
        let adapters = root.join(TREE).join("adapters");
        let files: [(&str, String); 8] = [
            ("claude-code/.claude-plugin/plugin.json", format!("{{\"name\":\"marionette\",\"version\":\"{version}\"}}")),
            ("claude-code/skills/marionette/SKILL.md", "claude skill".into()),
            ("codex/.codex-plugin/plugin.json", format!("{{\"name\":\"marionette\",\"version\":\"{version}\"}}")),
            ("codex/skills/marionette/SKILL.md", "codex skill".into()),
            ("opencode/index.js", "export default {};".into()),
            ("opencode/skills/marionette/SKILL.md", "opencode skill".into()),
            ("pi/extensions/policy.js", "export default () => {};".into()),
            ("pi/skills/marionette/SKILL.md", "pi skill".into()),
        ];
        for (path, text) in files {
            let path = adapters.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        fs::write(root.join(TREE).join("NOTICE.md"), "notice").unwrap();
    }
    /// Serves one release whose "zip" is a fixture tree copied on extract.
    pub(crate) struct FakeFetch {
        pub tag: String,
        pub version: String,
        pub asset: Vec<u8>,
        pub downloads: Cell<u32>,
        pub offline: bool,
    }
    impl FakeFetch {
        pub(crate) fn new(tag: &str, version: &str) -> Self {
            Self {
                tag: tag.into(),
                version: version.into(),
                asset: format!("zip {tag}").into_bytes(),
                downloads: Cell::new(0),
                offline: false,
            }
        }
    }
    impl Fetch for FakeFetch {
        fn latest_release(&self, repository: &str) -> Result<Vec<u8>, MarionetteError> {
            if self.offline {
                return Err(MarionetteError::Network);
            }
            Ok(serde_json::to_vec(&serde_json::json!({
                "tag_name": self.tag,
                "draft": false,
                "prerelease": false,
                "assets": [{
                    "name": format!("marionette-{}.zip", self.version),
                    "size": self.asset.len(),
                    "browser_download_url": format!("https://github.com/{repository}/releases/download/{}/marionette.zip", self.tag),
                    "digest": format!("sha256:{}", hex(&self.asset)),
                }]
            }))
            .unwrap())
        }
        fn download(&self, _: &str, destination: &Path, _: u64) -> Result<(), MarionetteError> {
            self.downloads.set(self.downloads.get() + 1);
            fs::write(destination, &self.asset).map_err(|_| MarionetteError::Network)
        }
        fn extract(&self, _: &Path, destination: &Path) -> Result<(), MarionetteError> {
            fixture_tree(destination, &self.version);
            Ok(())
        }
    }

    #[test]
    fn set_is_exactly_marionette() {
        assert!(is_marionette_set(&marionette_set()));
        let empty = SetManifest::new(SET_SCHEMA_VERSION, SET_ID, Vec::new()).unwrap();
        assert!(!is_marionette_set(&empty));
        let other = SetManifest::new(
            SET_SCHEMA_VERSION,
            SET_ID,
            vec![SetEntry::new(SetKind::Plugin, "ponytail", 1).unwrap()],
        )
        .unwrap();
        assert!(!is_marionette_set(&other));
    }
    #[test]
    fn sync_installs_then_stays_up_to_date_then_follows_a_new_release() {
        let store = scratch("follow");
        let first = FakeFetch::new("v0.1.0", "0.1.0");
        assert_eq!(sync(&store, &first).unwrap(), SyncOutcome::Updated("v0.1.0".into()));
        assert_eq!(sync(&store, &first).unwrap(), SyncOutcome::UpToDate("v0.1.0".into()));
        assert_eq!(first.downloads.get(), 1);
        let current = Marionette::current(&store).unwrap();
        assert_eq!((current.tag(), current.version()), ("v0.1.0", "0.1.0"));
        for host in Host::ALL {
            let adapter = current.verify(host).unwrap();
            assert!(adapter.dir().ends_with(host.adapter()));
            assert!(!adapter.dir_arg().starts_with(r"\\?\"));
        }
        let second = FakeFetch::new("v0.2.0", "0.2.0");
        assert_eq!(sync(&store, &second).unwrap(), SyncOutcome::Updated("v0.2.0".into()));
        let third = FakeFetch::new("v0.3.0", "0.3.0");
        sync(&store, &third).unwrap();
        assert_eq!(Marionette::current(&store).unwrap().tag(), "v0.3.0");
        // The new copy and the one before it stay; older copies go.
        assert!(!store.join(RELEASES).join("v0.1.0").exists());
        assert!(store.join(RELEASES).join("v0.2.0").exists());
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn offline_keeps_the_last_good_copy() {
        let store = scratch("offline");
        sync(&store, &FakeFetch::new("v0.1.0", "0.1.0")).unwrap();
        let mut offline = FakeFetch::new("v0.2.0", "0.2.0");
        offline.offline = true;
        assert_eq!(sync(&store, &offline).unwrap_err(), MarionetteError::Network);
        assert_eq!(Marionette::current(&store).unwrap().tag(), "v0.1.0");
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn a_digest_mismatch_installs_nothing() {
        let store = scratch("digest");
        struct Corrupt(FakeFetch);
        impl Fetch for Corrupt {
            fn latest_release(&self, r: &str) -> Result<Vec<u8>, MarionetteError> {
                self.0.latest_release(r)
            }
            fn download(&self, _: &str, d: &Path, _: u64) -> Result<(), MarionetteError> {
                let mut bytes = self.0.asset.clone();
                bytes[0] ^= 1;
                fs::write(d, bytes).map_err(|_| MarionetteError::Network)
            }
            fn extract(&self, a: &Path, d: &Path) -> Result<(), MarionetteError> {
                self.0.extract(a, d)
            }
        }
        let corrupt = Corrupt(FakeFetch::new("v0.1.0", "0.1.0"));
        assert_eq!(sync(&store, &corrupt).unwrap_err(), MarionetteError::Mismatch);
        assert!(Marionette::current(&store).is_err());
        assert!(!store.join("staging").exists());
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn tampered_or_unlisted_adapter_file_is_refused() {
        let store = scratch("tamper");
        sync(&store, &FakeFetch::new("v0.1.0", "0.1.0")).unwrap();
        let current = Marionette::current(&store).unwrap();
        let pi = current.verify(Host::Pi).unwrap();
        fs::write(pi.dir().join("extensions/policy.js"), "changed").unwrap();
        assert_eq!(current.verify(Host::Pi).unwrap_err(), MarionetteError::Mismatch);
        let opencode = current.verify(Host::OpenCode).unwrap();
        fs::write(opencode.dir().join("extra.js"), "x").unwrap();
        assert_eq!(current.verify(Host::OpenCode).unwrap_err(), MarionetteError::Unexpected);
        // The other hosts are unaffected, and the next sync repairs the copy.
        current.verify(Host::Codex).unwrap();
        sync(&store, &FakeFetch::new("v0.1.0", "0.1.0")).unwrap();
        Marionette::current(&store).unwrap().verify(Host::Pi).unwrap();
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn claude_copy_and_codex_cache_mirror_the_adapter() {
        let store = scratch("mirror");
        sync(&store, &FakeFetch::new("v0.1.0", "0.1.0")).unwrap();
        let current = Marionette::current(&store).unwrap();
        let claude = current.verify(Host::ClaudeCode).unwrap();
        let destination = store.join("claude-copy");
        let copy = claude.private_copy(&destination).unwrap();
        fs::write(copy.dir().join("tsconfig.json"), "{}").unwrap();
        claude.private_copy(&destination).unwrap();
        assert!(!destination.join("tsconfig.json").exists());
        current.verify(Host::ClaudeCode).unwrap();

        let codex = current.verify(Host::Codex).unwrap();
        let home = store.join("codex-home");
        codex.install_codex_cache(&home).unwrap();
        codex.install_codex_cache(&home).unwrap();
        let installed = home.join("plugins/cache/shinatama/marionette/0.1.0");
        let mut found = BTreeSet::new();
        collect(&installed, "", 0, &mut found).unwrap();
        assert_eq!(found.len(), codex.files().len());
        let overrides = codex.codex_overrides();
        assert_eq!(overrides.len(), 4);
        assert!(overrides[1].starts_with("marketplaces.shinatama={source_type=\"local\",source=\""));
        assert!(!overrides[1].contains('\\'));
        assert_eq!(overrides[3], "plugins={\"marionette@shinatama\"={enabled=true}}");
        let marketplace: serde_json::Value = serde_json::from_slice(
            &fs::read(store.join(RELEASES).join("v0.1.0").join(MARKETPLACE_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(marketplace["plugins"][0]["source"]["path"], "./marionette/adapters/codex");
        let _ = fs::remove_dir_all(&store);
    }
    /// Manual: `SHN_MARIONETTE_ZIP=<marionette.zip> cargo test -p shinatama-engine
    /// -- --ignored real_zip` installs a real Muro release through System32 `tar.exe`.
    #[test]
    #[ignore]
    fn real_zip_installs_through_system_tar() {
        struct LocalZip(PathBuf, Vec<u8>);
        impl Fetch for LocalZip {
            fn latest_release(&self, repository: &str) -> Result<Vec<u8>, MarionetteError> {
                Ok(serde_json::to_vec(&serde_json::json!({
                    "tag_name": "v0.0.0-local",
                    "assets": [{
                        "name": "marionette.zip",
                        "size": self.1.len(),
                        "browser_download_url": format!("https://github.com/{repository}/releases/download/v0.0.0-local/marionette.zip"),
                        "digest": format!("sha256:{}", hex(&self.1)),
                    }]
                }))
                .unwrap())
            }
            fn download(&self, _: &str, destination: &Path, _: u64) -> Result<(), MarionetteError> {
                fs::copy(&self.0, destination).map(drop).map_err(|_| MarionetteError::Network)
            }
            fn extract(&self, archive: &Path, destination: &Path) -> Result<(), MarionetteError> {
                SystemFetch.extract(archive, destination)
            }
        }
        let zip = PathBuf::from(std::env::var_os("SHN_MARIONETTE_ZIP").expect("SHN_MARIONETTE_ZIP"));
        let bytes = fs::read(&zip).unwrap();
        let store = scratch("real-zip");
        let started = std::time::Instant::now();
        sync(&store, &LocalZip(zip, bytes)).unwrap();
        let current = Marionette::current(&store).unwrap();
        for host in Host::ALL {
            let adapter = current.verify(host).unwrap();
            eprintln!("{:?}: {} files at {}", host, adapter.files().len(), adapter.dir_arg());
        }
        eprintln!("version {} installed and verified in {:?}", current.version(), started.elapsed());
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn release_without_a_digest_or_from_elsewhere_is_refused() {
        let base = FakeFetch::new("v0.1.0", "0.1.0");
        let mut metadata: serde_json::Value =
            serde_json::from_slice(&base.latest_release(REPOSITORY).unwrap()).unwrap();
        let release = |value: &serde_json::Value| -> ReleaseMetadata {
            serde_json::from_value(value.clone()).unwrap()
        };
        assert!(choose_asset(&release(&metadata)).is_ok());
        metadata["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(choose_asset(&release(&metadata)).is_err());
        metadata["assets"][0]["digest"] = format!("sha256:{}", "0".repeat(64)).into();
        metadata["assets"][0]["browser_download_url"] = "https://example.com/marionette.zip".into();
        assert!(choose_asset(&release(&metadata)).is_err());
        assert!(!valid_tag("../x") && !valid_tag(".hidden") && valid_tag("v0.1.0"));
    }
}
