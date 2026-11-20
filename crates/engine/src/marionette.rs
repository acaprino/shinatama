//! Marionette, the one plugin every real session of Claude Code, Codex, Pi and
//! OpenCode gets. Muro (github.com/acaprino/muro) builds it as one of its
//! examples and publishes each build as a `marionette-v<version>` release of
//! its own repository, github.com/acaprino/marionette; it carries
//! what Ponytail and i-have-adhd used to bring natively. At every app start
//! [`sync`] lists the repository's releases, takes the highest published
//! `marionette-v<version>` and, when the tag is new, downloads its `marionette*.zip`,
//! checks the SHA-256 GitHub reports for it, extracts it beside the previous
//! copy and switches to it. The package ships one pinned release beside the
//! executable ([`BUNDLE`]); before the sync, [`seed`] installs it the same way,
//! from that zip, when the store has no intact copy at least as new, so a fresh
//! install has Marionette offline too. Launches read only that local copy: every file of
//! the adapter they hand to a harness is checked against the lock written at
//! extraction. Offline or on any doubt the last good copy stays in use; with
//! no good copy a session starts without Marionette. Nothing is installed in
//! the user's own configuration.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shinatama_adapter::sets::{SET_SCHEMA_VERSION, SetEntry, SetKind, SetManifest};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// `owner/name` of the repository whose Marionette releases are followed.
pub const REPOSITORY: &str = "acaprino/marionette";
/// Tag prefix of Marionette's releases; the rest of the tag is its version.
pub const TAG_PREFIX: &str = "marionette-v";
pub const SET_ID: &str = "marionette";
pub const PLUGIN: &str = "marionette";
pub const MARKETPLACE_NAME: &str = "shinatama";
/// Folder under the Shinatama home that holds the synced copies.
pub const STORE: &str = "marionette";
/// Folder beside the executable that holds the release a package ships:
/// `build-release.bat` puts there the zip `scripts/marionette-pin.json` names,
/// checked against its SHA-256, and that pin as [`BUNDLE_PIN`].
pub const BUNDLE: &str = "marionette";
/// The pin inside [`BUNDLE`]: repository, tag, and the one zip with its size and SHA-256.
pub const BUNDLE_PIN: &str = "marionette-pin.json";
/// Folder under the Shinatama home that holds Claude Code's private copy of
/// its adapter; never inside [`STORE`].
pub const CLAUDE_COPY: &str = "marionette-claude";
/// OpenCode's private copy of its adapter, likewise never inside [`STORE`].
pub const OPENCODE_COPY: &str = "marionette-opencode";
/// Pi's private copy of its adapter, likewise never inside [`STORE`].
pub const PI_COPY: &str = "marionette-pi";
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
const MAX_RELEASES_BYTES: u64 = 8 * 1024 * 1024;
/// One page of releases, newest first: Marionette's latest is always on it.
const RELEASES_PER_PAGE: u32 = 100;
// Bounds of one release. Marionette carries its sources whole since 2026-11-17
// (taste-skill's kit and images in every adapter). Measured on that build: a
// zip of 188 MiB, 12,061 files, a tree of 257 MiB, a lock of 2.6 MiB. The zip
// was over the 128 MiB it was allowed, so the sync refused the release and kept
// the copy before it; the files were at three quarters of their 16,384. All four
// are four times what they were. The tree bound stays above what a zip at its
// own bound extracts to: a release is not downloaded to be refused afterwards.
const MAX_LOCK_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 64 * 1024;
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
        && set
            .entries()
            .iter()
            .all(|e| e.kind() == SetKind::Plugin && e.id() == PLUGIN && e.revision() == 1)
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
        let current: Current = serde_json::from_slice(&bytes).map_err(|_| MarionetteError::Lock)?;
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
        Ok(Self {
            release,
            tag: lock.tag,
            version: lock.version,
            files,
        })
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
        Ok(Adapter {
            dir: destination.to_path_buf(),
            ..self.clone()
        })
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
        // The mirror removes every file the adapter does not hold: a target that
        // holds the release, or lies inside it, would wipe the files it copies.
        let release = resolved(&self.marketplace_root).ok_or(MarionetteError::Missing)?;
        let resolved_target = resolved(target).ok_or(MarionetteError::UnsafePath)?;
        if resolved_target.starts_with(&release) || release.starts_with(&resolved_target) {
            return Err(MarionetteError::UnsafePath);
        }
        let wanted: BTreeSet<&str> = self.files.iter().map(String::as_str).collect();
        if target.is_dir() {
            let mut present = BTreeSet::new();
            if collect(target, "", 0, &mut present).is_ok() {
                for stale in present
                    .iter()
                    .filter(|path| !wanted.contains(path.as_str()))
                {
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
/// What [`seed`] did with the release the package ships.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedOutcome {
    /// The current copy is intact and at least as new: kept, nothing installed.
    Kept(String),
    /// The shipped release was installed and is now current.
    Installed(String),
}

/// Network and archive operations, apart so the sync logic runs in tests.
pub trait Fetch {
    /// Body of `GET https://api.github.com/repos/<repository>/releases?per_page=100`,
    /// a JSON array of releases, newest first.
    fn releases(&self, repository: &str) -> Result<Vec<u8>, MarionetteError>;
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

/// Follow the highest published `marionette-v<version>` release of
/// [`REPOSITORY`] into `store`.
pub fn sync(store: &Path, fetch: &dyn Fetch) -> Result<SyncOutcome, MarionetteError> {
    let (store, _guard) = lock_store(store)?;
    let metadata = fetch.releases(REPOSITORY)?;
    if metadata.len() as u64 > MAX_RELEASES_BYTES {
        return Err(MarionetteError::Limit);
    }
    let releases: Vec<ReleaseMetadata> =
        serde_json::from_slice(&metadata).map_err(|_| MarionetteError::Release)?;
    let release = choose_release(releases)?;
    let tag = release.tag_name.clone();
    let asset = choose_asset(&release)?;
    let previous = Marionette::current(&store).ok();
    if let Some(current) = &previous
        && current.tag == tag
        && current.verify_all().is_ok()
    {
        return Ok(SyncOutcome::UpToDate(tag));
    }
    make_current(&store, &tag, asset, fetch, previous.as_ref())?;
    Ok(SyncOutcome::Updated(tag))
}

/// `scripts/marionette-pin.json` as the package carries it in [`BUNDLE`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundlePin {
    schema_version: u32,
    release: BundleRelease,
}
#[derive(Deserialize)]
struct BundleRelease {
    repository: String,
    tag: String,
    /// File name to SHA-256; a Marionette pin names one zip.
    assets: BTreeMap<String, String>,
    /// File name to size in bytes.
    bytes: BTreeMap<String, u64>,
}

/// Installs the release the package ships (`bundle` holds [`BUNDLE_PIN`] and
/// the zip it names) into `store`, unless the current copy is intact and at
/// least as new. Run before [`sync`]: a fresh install has Marionette offline
/// too. `fetch` only extracts. The zip is copied from the package and goes
/// through a download's own checks: size and SHA-256 against the pin, the tree,
/// the lock and every adapter verified before `current.json` switches. A
/// refused zip switches nothing.
pub fn seed(
    store: &Path,
    bundle: &Path,
    fetch: &dyn Fetch,
) -> Result<SeedOutcome, MarionetteError> {
    let pin = read_bounded(&bundle.join(BUNDLE_PIN), MAX_METADATA_BYTES)?;
    let pin: BundlePin = serde_json::from_slice(&pin).map_err(|_| MarionetteError::Release)?;
    let release = &pin.release;
    let mut assets = release.assets.iter();
    let (Some((name, sha256)), None, Some(version)) =
        (assets.next(), assets.next(), tag_version(&release.tag))
    else {
        return Err(MarionetteError::Release);
    };
    let size = release.bytes.get(name).copied().unwrap_or(0);
    let lower = name.to_ascii_lowercase();
    if pin.schema_version != 1
        || release.repository != REPOSITORY
        || !valid_tag(&release.tag)
        || !lower.starts_with("marionette")
        || !lower.ends_with(".zip")
        || name.len() > 128
        || name.contains(['/', '\\'])
        || !valid_digest(sha256)
        || size == 0
        || size > MAX_ASSET_BYTES
    {
        return Err(MarionetteError::Release);
    }
    let (store, _guard) = lock_store(store)?;
    let previous = Marionette::current(&store).ok();
    if let Some(current) = &previous
        && tag_version(&current.tag).is_some_and(|have| have >= version)
        && current.verify_all().is_ok()
    {
        return Ok(SeedOutcome::Kept(current.tag.clone()));
    }
    let url = format!(
        "https://github.com/{REPOSITORY}/releases/download/{}/{name}",
        release.tag
    );
    let asset = ChosenAsset {
        name,
        size,
        url: &url,
        sha256: sha256.clone(),
    };
    let shipped = Bundled {
        zip: bundle.join(name),
        fetch,
    };
    make_current(&store, &release.tag, asset, &shipped, previous.as_ref())?;
    Ok(SeedOutcome::Installed(release.tag.clone()))
}

/// The package's zip in place of a download; extraction is `fetch`'s.
struct Bundled<'a> {
    zip: PathBuf,
    fetch: &'a dyn Fetch,
}
impl Fetch for Bundled<'_> {
    fn releases(&self, _: &str) -> Result<Vec<u8>, MarionetteError> {
        Err(MarionetteError::Release)
    }
    fn download(&self, _: &str, destination: &Path, limit: u64) -> Result<(), MarionetteError> {
        let meta = fs::symlink_metadata(&self.zip).map_err(|_| MarionetteError::Missing)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(MarionetteError::UnsafePath);
        }
        if meta.len() > limit {
            return Err(MarionetteError::Mismatch);
        }
        fs::copy(&self.zip, destination)
            .map(drop)
            .map_err(|_| MarionetteError::Missing)
    }
    fn extract(&self, archive: &Path, destination: &Path) -> Result<(), MarionetteError> {
        self.fetch.extract(archive, destination)
    }
}

/// `store` made if missing and locked against another window's sync or seed for
/// as long as the returned file stays open.
fn lock_store(store: &Path) -> Result<(PathBuf, fs::File), MarionetteError> {
    fs::create_dir_all(store.join(RELEASES)).map_err(|_| MarionetteError::Missing)?;
    let store = ordinary_directory(store)?;
    let guard = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(store.join(SYNC_LOCK_FILE))
        .map_err(|_| MarionetteError::Missing)?;
    guard.try_lock().map_err(|_| MarionetteError::Busy)?;
    Ok((store, guard))
}

/// Makes `tag` current: an intact copy already in `releases/` is reused,
/// otherwise `asset` is installed through `fetch`. The copy before it stays (a
/// running session may still read it); older copies are removed.
fn make_current(
    store: &Path,
    tag: &str,
    asset: ChosenAsset<'_>,
    fetch: &dyn Fetch,
    previous: Option<&Marionette>,
) -> Result<(), MarionetteError> {
    let target = store.join(RELEASES).join(tag);
    let reusable = Marionette::open(&target, tag)
        .and_then(|copy| copy.verify_all())
        .is_ok();
    if !reusable {
        install(store, &target, tag, asset, fetch)?;
    }
    write_current(store, tag)?;
    prune(store, tag, previous.map(|p| p.tag.as_str()));
    Ok(())
}

/// The highest `marionette-v<version>` release that is published (no draft, no
/// prerelease); any other tag is ignored.
fn choose_release(releases: Vec<ReleaseMetadata>) -> Result<ReleaseMetadata, MarionetteError> {
    releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease && valid_tag(&release.tag_name))
        .filter_map(|release| Some((tag_version(&release.tag_name)?, release)))
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, release)| release)
        .ok_or(MarionetteError::Release)
}
/// `marionette-v0.10.2` gives `[0, 10, 2]`; any other tag gives `None`.
fn tag_version(tag: &str) -> Option<Vec<u64>> {
    let parts = tag
        .strip_prefix(TAG_PREFIX)?
        .split('.')
        .map(|part| {
            let digits =
                !part.is_empty() && part.len() <= 9 && part.bytes().all(|b| b.is_ascii_digit());
            digits.then(|| part.parse::<u64>().ok()).flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    (parts.len() <= 4).then_some(parts)
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
    let prefix = format!(
        "https://github.com/{REPOSITORY}/releases/download/{}/",
        release.tag_name
    );
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
            files.push(LockedFile {
                path,
                bytes,
                sha256,
            });
        }
        for host in Host::ALL {
            if !files
                .iter()
                .any(|file| file.path.starts_with(&host.prefix()))
            {
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
    let path = staging
        .join(TREE)
        .join("adapters/codex/.codex-plugin/plugin.json");
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
    let current = Current {
        format_version: 1,
        tag: tag.into(),
    };
    let text = serde_json::to_vec_pretty(&current).map_err(|_| MarionetteError::Lock)?;
    let temporary = store.join("current.json.part");
    fs::write(&temporary, text).map_err(|_| MarionetteError::Missing)?;
    // Replaces the old file in one step (MoveFileEx with REPLACE_EXISTING).
    fs::rename(&temporary, store.join(CURRENT_FILE)).map_err(|_| MarionetteError::Missing)
}
/// Keeps the new copy and the one before it (a running session may still read
/// it); older copies are removed.
fn prune(store: &Path, current: &str, previous: Option<&str>) {
    let Ok(entries) = fs::read_dir(store.join(RELEASES)) else {
        return;
    };
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
        if path.is_file() {
            Ok(path)
        } else {
            Err(MarionetteError::Network)
        }
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
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(())
        }
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
    fn releases(&self, repository: &str) -> Result<Vec<u8>, MarionetteError> {
        let curl = Self::tool("curl.exe")?;
        let mut args = Self::curl_base("30", MAX_RELEASES_BYTES);
        for header in [
            "Accept: application/vnd.github+json",
            "X-GitHub-Api-Version: 2022-11-28",
        ] {
            args.push("--header".into());
            args.push(header.into());
        }
        args.push(
            format!(
                "https://api.github.com/repos/{repository}/releases?per_page={RELEASES_PER_PAGE}"
            )
            .into(),
        );
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
        Self::run(&curl, &refs)
            .map(drop)
            .map_err(|_| MarionetteError::Network)
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
        Self::run(&tar, &args)
            .map(drop)
            .map_err(|_| MarionetteError::Archive)
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
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}
fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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
        let read = file
            .read(&mut buffer)
            .map_err(|_| MarionetteError::Missing)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_ASSET_BYTES {
            return Err(MarionetteError::Limit);
        }
        hasher.update(&buffer[..read]);
    }
    Ok((
        total,
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    ))
}
fn ordinary_directory(path: &Path) -> Result<PathBuf, MarionetteError> {
    let meta = fs::symlink_metadata(path).map_err(|_| MarionetteError::Missing)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(MarionetteError::UnsafePath);
    }
    path.canonicalize().map_err(|_| MarionetteError::Missing)
}
/// `path` with its deepest existing ancestor canonicalized and the missing
/// rest appended, so a target that does not exist yet compares like one that does.
fn resolved(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(mut out) = existing.canonicalize() {
            out.extend(missing.iter().rev());
            return Some(out);
        }
        missing.push(existing.file_name()?.to_owned());
        existing = existing.parent()?;
    }
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
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| MarionetteError::UnsafePath)?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| MarionetteError::Missing)?;
        if meta.file_type().is_symlink() {
            return Err(MarionetteError::UnsafePath);
        }
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
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
        let path =
            std::env::temp_dir().join(format!("shinatama-marionette-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }
    /// A small release tree with every host's adapter.
    pub(crate) fn fixture_tree(root: &Path, version: &str) {
        let adapters = root.join(TREE).join("adapters");
        let files: [(&str, String); 8] = [
            (
                "claude-code/.claude-plugin/plugin.json",
                format!("{{\"name\":\"marionette\",\"version\":\"{version}\"}}"),
            ),
            (
                "claude-code/skills/marionette/SKILL.md",
                "claude skill".into(),
            ),
            (
                "codex/.codex-plugin/plugin.json",
                format!("{{\"name\":\"marionette\",\"version\":\"{version}\"}}"),
            ),
            ("codex/skills/marionette/SKILL.md", "codex skill".into()),
            ("opencode/index.js", "export default {};".into()),
            (
                "opencode/skills/marionette/SKILL.md",
                "opencode skill".into(),
            ),
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
        /// As GitHub lists Muro: Muro's own release newest, then Marionette's.
        fn releases(&self, repository: &str) -> Result<Vec<u8>, MarionetteError> {
            if self.offline {
                return Err(MarionetteError::Network);
            }
            Ok(serde_json::to_vec(&serde_json::json!([
                { "tag_name": "muro-v9.9.9", "draft": false, "prerelease": false, "assets": [] },
                {
                    "tag_name": self.tag,
                    "draft": false,
                    "prerelease": false,
                    "assets": [{
                        "name": format!("marionette-{}.zip", self.version),
                        "size": self.asset.len(),
                        "browser_download_url": format!("https://github.com/{repository}/releases/download/{}/marionette.zip", self.tag),
                        "digest": format!("sha256:{}", hex(&self.asset)),
                    }, {
                        "name": format!("marionette-{}.zip.sha256", self.version),
                        "size": 87,
                        "browser_download_url": format!("https://github.com/{repository}/releases/download/{}/marionette.zip.sha256", self.tag),
                    }]
                }
            ]))
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
        let first = FakeFetch::new("marionette-v0.1.0", "0.1.0");
        assert_eq!(
            sync(&store, &first).unwrap(),
            SyncOutcome::Updated("marionette-v0.1.0".into())
        );
        assert_eq!(
            sync(&store, &first).unwrap(),
            SyncOutcome::UpToDate("marionette-v0.1.0".into())
        );
        assert_eq!(first.downloads.get(), 1);
        let current = Marionette::current(&store).unwrap();
        assert_eq!(
            (current.tag(), current.version()),
            ("marionette-v0.1.0", "0.1.0")
        );
        for host in Host::ALL {
            let adapter = current.verify(host).unwrap();
            assert!(adapter.dir().ends_with(host.adapter()));
            assert!(!adapter.dir_arg().starts_with(r"\\?\"));
        }
        let second = FakeFetch::new("marionette-v0.2.0", "0.2.0");
        assert_eq!(
            sync(&store, &second).unwrap(),
            SyncOutcome::Updated("marionette-v0.2.0".into())
        );
        let third = FakeFetch::new("marionette-v0.3.0", "0.3.0");
        sync(&store, &third).unwrap();
        assert_eq!(
            Marionette::current(&store).unwrap().tag(),
            "marionette-v0.3.0"
        );
        // The new copy and the one before it stay; older copies go.
        assert!(!store.join(RELEASES).join("marionette-v0.1.0").exists());
        assert!(store.join(RELEASES).join("marionette-v0.2.0").exists());
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn offline_keeps_the_last_good_copy() {
        let store = scratch("offline");
        sync(&store, &FakeFetch::new("marionette-v0.1.0", "0.1.0")).unwrap();
        let mut offline = FakeFetch::new("marionette-v0.2.0", "0.2.0");
        offline.offline = true;
        assert_eq!(
            sync(&store, &offline).unwrap_err(),
            MarionetteError::Network
        );
        assert_eq!(
            Marionette::current(&store).unwrap().tag(),
            "marionette-v0.1.0"
        );
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn a_digest_mismatch_installs_nothing() {
        let store = scratch("digest");
        struct Corrupt(FakeFetch);
        impl Fetch for Corrupt {
            fn releases(&self, r: &str) -> Result<Vec<u8>, MarionetteError> {
                self.0.releases(r)
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
        let corrupt = Corrupt(FakeFetch::new("marionette-v0.1.0", "0.1.0"));
        assert_eq!(
            sync(&store, &corrupt).unwrap_err(),
            MarionetteError::Mismatch
        );
        assert!(Marionette::current(&store).is_err());
        assert!(!store.join("staging").exists());
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn tampered_or_unlisted_adapter_file_is_refused() {
        let store = scratch("tamper");
        sync(&store, &FakeFetch::new("marionette-v0.1.0", "0.1.0")).unwrap();
        let current = Marionette::current(&store).unwrap();
        let pi = current.verify(Host::Pi).unwrap();
        fs::write(pi.dir().join("extensions/policy.js"), "changed").unwrap();
        assert_eq!(
            current.verify(Host::Pi).unwrap_err(),
            MarionetteError::Mismatch
        );
        let opencode = current.verify(Host::OpenCode).unwrap();
        fs::write(opencode.dir().join("extra.js"), "x").unwrap();
        assert_eq!(
            current.verify(Host::OpenCode).unwrap_err(),
            MarionetteError::Unexpected
        );
        // The other hosts are unaffected, and the next sync repairs the copy.
        current.verify(Host::Codex).unwrap();
        sync(&store, &FakeFetch::new("marionette-v0.1.0", "0.1.0")).unwrap();
        Marionette::current(&store)
            .unwrap()
            .verify(Host::Pi)
            .unwrap();
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn a_copy_never_lands_on_its_own_release() {
        let store = scratch("overlap");
        sync(&store, &FakeFetch::new("marionette-v0.1.0", "0.1.0")).unwrap();
        let current = Marionette::current(&store).unwrap();
        let claude = current.verify(Host::ClaudeCode).unwrap();
        let release = store.join(RELEASES).join("marionette-v0.1.0");
        for target in [
            store.clone(),
            release.clone(),
            release.join("marionette/copy"),
        ] {
            assert_eq!(
                claude.private_copy(&target).unwrap_err(),
                MarionetteError::UnsafePath
            );
        }
        // Nothing was removed: the release is still current and intact.
        Marionette::current(&store).unwrap().verify_all().unwrap();
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn claude_copy_and_codex_cache_mirror_the_adapter() {
        let store = scratch("mirror");
        sync(&store, &FakeFetch::new("marionette-v0.1.0", "0.1.0")).unwrap();
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
        assert!(
            overrides[1].starts_with("marketplaces.shinatama={source_type=\"local\",source=\"")
        );
        assert!(!overrides[1].contains('\\'));
        assert_eq!(
            overrides[3],
            "plugins={\"marionette@shinatama\"={enabled=true}}"
        );
        let marketplace: serde_json::Value = serde_json::from_slice(
            &fs::read(
                store
                    .join(RELEASES)
                    .join("marionette-v0.1.0")
                    .join(MARKETPLACE_FILE),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            marketplace["plugins"][0]["source"]["path"],
            "./marionette/adapters/codex"
        );
        let _ = fs::remove_dir_all(&store);
    }
    /// Manual: `SHN_MARIONETTE_BUNDLE=<folder with marionette-pin.json and its zip> cargo test
    /// -p shinatama-engine -- --ignored real_bundle` seeds a store from the folder a package
    /// ships (build-release.bat's release\marionette) through System32 `tar.exe`.
    #[test]
    #[ignore]
    fn real_bundle_seeds_through_system_tar() {
        // SHN_MARIONETTE_BUNDLE: a folder laid out as build-release.bat makes release\marionette.
        let bundle = PathBuf::from(
            std::env::var_os("SHN_MARIONETTE_BUNDLE").expect("SHN_MARIONETTE_BUNDLE"),
        );
        let store = scratch("real-bundle");
        let started = std::time::Instant::now();
        let outcome = seed(&store, &bundle, &SystemFetch).unwrap();
        let current = Marionette::current(&store).unwrap();
        current.verify_all().unwrap();
        eprintln!(
            "{outcome:?}: version {} installed and verified in {:?}",
            current.version(),
            started.elapsed()
        );
        assert_eq!(
            seed(&store, &bundle, &SystemFetch).unwrap(),
            SeedOutcome::Kept(current.tag().to_owned())
        );
    }

    /// Manual: `SHN_MARIONETTE_ZIP=<marionette.zip> cargo test -p shinatama-engine
    /// -- --ignored real_zip` installs a real Muro release through System32 `tar.exe`.
    #[test]
    #[ignore]
    fn real_zip_installs_through_system_tar() {
        struct LocalZip(PathBuf, Vec<u8>);
        impl Fetch for LocalZip {
            fn releases(&self, repository: &str) -> Result<Vec<u8>, MarionetteError> {
                Ok(serde_json::to_vec(&serde_json::json!([{
                    "tag_name": "marionette-v0.0.0",
                    "assets": [{
                        "name": "marionette.zip",
                        "size": self.1.len(),
                        "browser_download_url": format!("https://github.com/{repository}/releases/download/marionette-v0.0.0/marionette.zip"),
                        "digest": format!("sha256:{}", hex(&self.1)),
                    }]
                }]))
                .unwrap())
            }
            fn download(&self, _: &str, destination: &Path, _: u64) -> Result<(), MarionetteError> {
                fs::copy(&self.0, destination)
                    .map(drop)
                    .map_err(|_| MarionetteError::Network)
            }
            fn extract(&self, archive: &Path, destination: &Path) -> Result<(), MarionetteError> {
                SystemFetch.extract(archive, destination)
            }
        }
        let zip =
            PathBuf::from(std::env::var_os("SHN_MARIONETTE_ZIP").expect("SHN_MARIONETTE_ZIP"));
        let bytes = fs::read(&zip).unwrap();
        let store = scratch("real-zip");
        let started = std::time::Instant::now();
        sync(&store, &LocalZip(zip, bytes)).unwrap();
        let current = Marionette::current(&store).unwrap();
        eprintln!(
            "version {} installed and verified in {:?}",
            current.version(),
            started.elapsed()
        );
        // How far the release is from each bound, and what a launch pays.
        let release = store.join(RELEASES).join("marionette-v0.0.0");
        let lock = fs::metadata(release.join(LOCK_FILE)).unwrap().len();
        let longest = current
            .files
            .iter()
            .map(|(path, _, _)| path.len())
            .max()
            .unwrap_or(0);
        let tree: u64 = current.files.iter().map(|(_, bytes, _)| bytes).sum();
        eprintln!(
            "lock.json {lock} of {MAX_LOCK_BYTES} bytes; {} of {MAX_FILES} files",
            current.files.len()
        );
        eprintln!("tree {tree} of {MAX_TREE_BYTES} bytes; longest path {longest} characters");
        for host in Host::ALL {
            let started = std::time::Instant::now();
            let adapter = current.verify(host).unwrap();
            eprintln!(
                "{:?}: {} files verified in {:?}",
                host,
                adapter.files().len(),
                started.elapsed()
            );
        }
        // What one Claude Code launch does, four times over: the first copy writes
        // every file, the later ones find them unchanged.
        for launch in 1..=4 {
            let started = std::time::Instant::now();
            let opened = Marionette::current(&store).unwrap();
            let read = started.elapsed();
            let claude = opened.verify(Host::ClaudeCode).unwrap();
            let verified = started.elapsed();
            claude.private_copy(&store.join("claude-copy")).unwrap();
            eprintln!(
                "Claude Code launch {launch}: lock read {read:?}, verified {verified:?}, copy ready {:?}",
                started.elapsed()
            );
        }
        let _ = fs::remove_dir_all(&store);
    }
    /// Manual, needs the network: `cargo test -p shinatama-engine -- --ignored
    /// real_github_sync` follows the highest `marionette-v*` release of [`REPOSITORY`] through
    /// `SystemFetch`.
    #[test]
    #[ignore]
    fn real_github_sync() {
        let store = scratch("github");
        let started = std::time::Instant::now();
        let outcome = sync(&store, &SystemFetch).unwrap();
        assert!(matches!(&outcome, SyncOutcome::Updated(tag) if tag.starts_with(TAG_PREFIX)));
        assert!(matches!(
            sync(&store, &SystemFetch).unwrap(),
            SyncOutcome::UpToDate(_)
        ));
        let current = Marionette::current(&store).unwrap();
        for host in Host::ALL {
            let adapter = current.verify(host).unwrap();
            eprintln!("{:?}: {} files", host, adapter.files().len());
        }
        eprintln!(
            "{outcome:?} version {} in {:?}",
            current.version(),
            started.elapsed()
        );
        let _ = fs::remove_dir_all(&store);
    }
    #[test]
    fn release_without_a_digest_or_from_elsewhere_is_refused() {
        let base = FakeFetch::new("marionette-v0.1.0", "0.1.0");
        let list: serde_json::Value =
            serde_json::from_slice(&base.releases(REPOSITORY).unwrap()).unwrap();
        let mut metadata = list[1].clone();
        let release = |value: &serde_json::Value| -> ReleaseMetadata {
            serde_json::from_value(value.clone()).unwrap()
        };
        assert!(choose_asset(&release(&metadata)).is_ok());
        metadata["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(choose_asset(&release(&metadata)).is_err());
        metadata["assets"][0]["digest"] = format!("sha256:{}", "0".repeat(64)).into();
        let good_url = metadata["assets"][0]["browser_download_url"].clone();
        metadata["assets"][0]["browser_download_url"] = "https://example.com/marionette.zip".into();
        assert!(choose_asset(&release(&metadata)).is_err());
        // An asset of another release of the repository is refused too.
        metadata["assets"][0]["browser_download_url"] = good_url
            .as_str()
            .unwrap()
            .replace("marionette-v0.1.0", "muro-v9.9.9")
            .into();
        assert!(choose_asset(&release(&metadata)).is_err());
        assert!(!valid_tag("../x") && !valid_tag(".hidden") && valid_tag("marionette-v0.1.0"));
    }
    #[test]
    fn a_release_that_carries_its_sources_whole_is_within_the_asset_bound() {
        // Muro, 2026-11-17: a source travels with every file its method uses, so
        // taste-skill is whole, images included, in every adapter. The zip of that
        // build is 197,039,241 bytes. The bound was 128 MiB: the sync refused the
        // release and kept the copy before it, with one line in the log.
        let base = FakeFetch::new("marionette-v0.4.0", "0.4.0");
        let list: serde_json::Value =
            serde_json::from_slice(&base.releases(REPOSITORY).unwrap()).unwrap();
        let mut metadata = list[1].clone();
        let release = |value: &serde_json::Value| -> ReleaseMetadata {
            serde_json::from_value(value.clone()).unwrap()
        };
        metadata["assets"][0]["size"] = 197_039_241u64.into();
        assert_eq!(choose_asset(&release(&metadata)).unwrap().size, 197_039_241);
        // A bound stays: an asset over it is refused before anything is downloaded.
        metadata["assets"][0]["size"] = (MAX_ASSET_BYTES + 1).into();
        assert!(choose_asset(&release(&metadata)).is_err());
    }
    #[test]
    fn a_lock_twice_the_size_of_a_whole_source_release_opens() {
        // Measured on Muro's build of 2026-11-18 through the real zip test below:
        // 12,061 files and a lock of 2,732,682 bytes, against bounds of 16,384 files
        // and 4 MiB. One more source with a kit would have been refused again.
        let release = scratch("large-lock");
        fs::create_dir_all(&release).unwrap();
        let digest = "0".repeat(64);
        let files: Vec<serde_json::Value> = (0..24_122)
            .map(|n| {
                serde_json::json!({
                    "path": format!("marionette/adapters/claude-code/skills/marionette/scripts/kit/{n:0100}.svg"),
                    "bytes": 1,
                    "sha256": digest,
                })
            })
            .collect();
        let lock = serde_json::json!({
            "formatVersion": 1,
            "repository": REPOSITORY,
            "tag": "marionette-v0.4.0",
            "version": "0.4.0",
            "asset": { "name": "marionette-0.4.0.zip", "bytes": 1, "sha256": digest },
            "files": files,
        });
        fs::write(
            release.join(LOCK_FILE),
            serde_json::to_vec_pretty(&lock).unwrap(),
        )
        .unwrap();
        let opened = Marionette::open(&release, "marionette-v0.4.0").unwrap();
        assert_eq!(opened.files.len(), 24_122);
        let _ = fs::remove_dir_all(&release);
    }
    #[test]
    fn only_the_highest_published_marionette_release_is_followed() {
        let entry = |tag: &str, draft: bool, prerelease: bool| serde_json::json!({ "tag_name": tag, "draft": draft, "prerelease": prerelease });
        let pick = |list: serde_json::Value| {
            choose_release(serde_json::from_value(list).unwrap()).map(|release| release.tag_name)
        };
        let list = serde_json::json!([
            entry("muro-v3.0.0", false, false),
            entry("marionette-v0.11.0", true, false),
            entry("marionette-v0.10.0-rc1", false, false),
            entry("marionette-v0.9.0", false, true),
            entry("marionette-v0.2.0", false, false),
            entry("marionette-v0.10.0", false, false),
            entry("v9.0.0", false, false),
            entry("marionette-v+1.0.0", false, false),
        ]);
        assert_eq!(pick(list).unwrap(), "marionette-v0.10.0");
        let none = serde_json::json!([entry("muro-v0.1.0", false, false)]);
        assert_eq!(pick(none).unwrap_err(), MarionetteError::Release);
        assert_eq!(tag_version("marionette-v0.2.0"), Some(vec![0, 2, 0]));
        assert_eq!(tag_version("marionette-v"), None);
        assert_eq!(tag_version("marionette-v1..0"), None);
    }

    /// A package's [`BUNDLE`] under `root`: a "zip" and the pin naming it with `sha256`.
    fn bundle(root: &Path, tag: &str, zip: &[u8], sha256: &str) -> PathBuf {
        let bundle = root.join(BUNDLE);
        fs::create_dir_all(&bundle).unwrap();
        let name = format!("marionette-{}.zip", tag.trim_start_matches(TAG_PREFIX));
        fs::write(bundle.join(&name), zip).unwrap();
        let pin = serde_json::json!({
            "schemaVersion": 1,
            "release": {
                "repository": REPOSITORY,
                "tag": tag,
                "commit": "0".repeat(40),
                "assets": { name.clone(): sha256 },
                "bytes": { name.clone(): zip.len() },
            }
        });
        fs::write(
            bundle.join(BUNDLE_PIN),
            serde_json::to_vec_pretty(&pin).unwrap(),
        )
        .unwrap();
        bundle
    }
    #[test]
    fn a_fresh_store_gets_the_bundled_copy_offline() {
        let root = scratch("seed-fresh");
        let store = root.join("store");
        let zip = b"zip marionette-v0.1.0".to_vec();
        let shipped = bundle(&root, "marionette-v0.1.0", &zip, &hex(&zip));
        let mut offline = FakeFetch::new("marionette-v0.1.0", "0.1.0");
        offline.offline = true;
        assert_eq!(
            seed(&store, &shipped, &offline).unwrap(),
            SeedOutcome::Installed("marionette-v0.1.0".into())
        );
        assert_eq!(offline.downloads.get(), 0);
        assert_eq!(
            sync(&store, &offline).unwrap_err(),
            MarionetteError::Network
        );
        let current = Marionette::current(&store).unwrap();
        assert_eq!(
            (current.tag(), current.version()),
            ("marionette-v0.1.0", "0.1.0")
        );
        current.verify_all().unwrap();
        let release = store.join(RELEASES).join("marionette-v0.1.0");
        let lock: Lock =
            serde_json::from_slice(&fs::read(release.join(LOCK_FILE)).unwrap()).unwrap();
        assert_eq!(
            (lock.asset.name.as_str(), lock.asset.bytes),
            ("marionette-0.1.0.zip", zip.len() as u64)
        );
        assert_eq!(lock.asset.sha256, hex(&zip));
        // The package keeps its zip; the store keeps no copy of it and no staging.
        assert!(shipped.join("marionette-0.1.0.zip").is_file());
        assert!(!store.join("download.zip.part").exists() && !store.join("staging").exists());
        // The next start finds the intact copy and installs nothing.
        assert_eq!(
            seed(&store, &shipped, &offline).unwrap(),
            SeedOutcome::Kept("marionette-v0.1.0".into())
        );
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn an_intact_copy_at_least_as_new_is_kept_and_an_older_or_damaged_one_is_not() {
        let root = scratch("seed-newer");
        let store = root.join("store");
        sync(&store, &FakeFetch::new("marionette-v0.2.0", "0.2.0")).unwrap();
        let zip = b"zip marionette-v0.1.0".to_vec();
        let older = bundle(&root.join("old"), "marionette-v0.1.0", &zip, &hex(&zip));
        let fetch = FakeFetch::new("marionette-v0.1.0", "0.1.0");
        assert_eq!(
            seed(&store, &older, &fetch).unwrap(),
            SeedOutcome::Kept("marionette-v0.2.0".into())
        );
        assert_eq!(
            Marionette::current(&store).unwrap().tag(),
            "marionette-v0.2.0"
        );
        assert!(!store.join(RELEASES).join("marionette-v0.1.0").exists());
        // A damaged current copy, even a newer one, gives way to the intact shipped
        // one; the next online sync goes back to the latest release.
        let pi = Marionette::current(&store)
            .unwrap()
            .verify(Host::Pi)
            .unwrap();
        fs::write(pi.dir().join("extensions/policy.js"), "changed").unwrap();
        assert_eq!(
            seed(&store, &older, &fetch).unwrap(),
            SeedOutcome::Installed("marionette-v0.1.0".into())
        );
        Marionette::current(&store).unwrap().verify_all().unwrap();
        assert_eq!(
            sync(&store, &FakeFetch::new("marionette-v0.2.0", "0.2.0")).unwrap(),
            SyncOutcome::Updated("marionette-v0.2.0".into())
        );
        // A newer package brings a newer release (numeric order: 0.10 after 0.2).
        let zip = b"zip marionette-v0.10.0".to_vec();
        let newer = bundle(&root.join("new"), "marionette-v0.10.0", &zip, &hex(&zip));
        assert_eq!(
            seed(
                &store,
                &newer,
                &FakeFetch::new("marionette-v0.10.0", "0.10.0")
            )
            .unwrap(),
            SeedOutcome::Installed("marionette-v0.10.0".into())
        );
        assert_eq!(Marionette::current(&store).unwrap().version(), "0.10.0");
        assert!(store.join(RELEASES).join("marionette-v0.2.0").exists());
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn a_bundled_zip_that_differs_from_its_pin_is_refused_and_nothing_switches() {
        let root = scratch("seed-digest");
        let store = root.join("store");
        sync(&store, &FakeFetch::new("marionette-v0.1.0", "0.1.0")).unwrap();
        let zip = b"zip marionette-v0.2.0".to_vec();
        let mut other = zip.clone();
        other[0] ^= 1;
        let shipped = bundle(&root, "marionette-v0.2.0", &zip, &hex(&other));
        let fetch = FakeFetch::new("marionette-v0.2.0", "0.2.0");
        assert_eq!(
            seed(&store, &shipped, &fetch).unwrap_err(),
            MarionetteError::Mismatch
        );
        assert_eq!(
            Marionette::current(&store).unwrap().tag(),
            "marionette-v0.1.0"
        );
        Marionette::current(&store).unwrap().verify_all().unwrap();
        assert!(!store.join(RELEASES).join("marionette-v0.2.0").exists());
        assert!(!store.join("staging").exists() && !store.join("download.zip.part").exists());
        // A zip longer than its pin says is refused the same way.
        fs::write(
            shipped.join("marionette-0.2.0.zip"),
            [zip.as_slice(), b"x".as_slice()].concat(),
        )
        .unwrap();
        assert_eq!(
            seed(&store, &shipped, &fetch).unwrap_err(),
            MarionetteError::Mismatch
        );
        // A fresh store stays without a copy rather than taking a refused one.
        let fresh = root.join("fresh");
        assert_eq!(
            seed(&fresh, &shipped, &fetch).unwrap_err(),
            MarionetteError::Mismatch
        );
        assert!(Marionette::current(&fresh).is_err());
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn a_pin_for_anything_but_one_marionette_zip_of_its_repository_is_refused_before_the_store() {
        let root = scratch("seed-pin");
        let store = root.join("store");
        let zip = b"zip marionette-v0.1.0".to_vec();
        let shipped = bundle(&root, "marionette-v0.1.0", &zip, &hex(&zip));
        let fetch = FakeFetch::new("marionette-v0.1.0", "0.1.0");
        let pin: serde_json::Value =
            serde_json::from_slice(&fs::read(shipped.join(BUNDLE_PIN)).unwrap()).unwrap();
        let two = serde_json::json!({ "marionette-0.1.0.zip": hex(&zip), "marionette-extra.zip": hex(&zip) });
        for (key, value) in [
            ("repository", serde_json::json!("acaprino/muro")),
            ("tag", serde_json::json!("muro-v0.1.0")),
            ("assets", two),
            ("bytes", serde_json::json!({})),
        ] {
            let mut changed = pin.clone();
            changed["release"][key] = value;
            fs::write(
                shipped.join(BUNDLE_PIN),
                serde_json::to_vec(&changed).unwrap(),
            )
            .unwrap();
            assert_eq!(
                seed(&store, &shipped, &fetch).unwrap_err(),
                MarionetteError::Release,
                "{key}"
            );
        }
        assert!(!store.exists());
        // No package folder at all, as in a development build.
        assert_eq!(
            seed(&store, &root.join("none"), &fetch).unwrap_err(),
            MarionetteError::Missing
        );
        let _ = fs::remove_dir_all(&root);
    }
}
