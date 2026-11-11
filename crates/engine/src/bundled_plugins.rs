//! The two plugins every launch gets: ponytail and i-have-adhd. Files are the
//! pinned copies under `resources/agent-plugins`; each file is checked against
//! `plugins.lock.json` before a launcher may name it. Pure local reads, no
//! network, no execution, no user configuration touched.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use shinatama_adapter::sets::{SET_SCHEMA_VERSION, SetEntry, SetKind, SetManifest};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Component, Path, PathBuf},
};

pub const SET_ID: &str = "bundled-plugins";
pub const PLUGINS: [&str; 2] = ["ponytail", "i-have-adhd"];
pub const MARKETPLACE_NAME: &str = "shinatama-bundled";
pub const LOCK_FILE: &str = "plugins.lock.json";
pub const MARKETPLACE_FILE: &str = ".agents/plugins/marketplace.json";
const MAX_LOCK_BYTES: u64 = 64 * 1024;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FILES: usize = 512;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundledPluginsError {
    Missing,
    Lock,
    UnsafePath,
    Mismatch,
    Unexpected,
    Limit,
}
impl fmt::Display for BundledPluginsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "bundled plugin file is missing",
            Self::Lock => "bundled plugin lock is invalid",
            Self::UnsafePath => "bundled plugin path is not an ordinary file",
            Self::Mismatch => "bundled plugin file does not match its pinned digest",
            Self::Unexpected => "bundled plugin tree holds a file the lock does not list",
            Self::Limit => "bundled plugin tree exceeds its fixed bound",
        })
    }
}
impl std::error::Error for BundledPluginsError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Lock {
    #[serde(rename = "formatVersion")]
    format_version: u32,
    marketplace: Pin,
    plugins: BTreeMap<String, LockedPlugin>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedPlugin {
    #[allow(dead_code)]
    repository: String,
    #[allow(dead_code)]
    commit: String,
    version: String,
    #[allow(dead_code)]
    license: String,
    files: Vec<LockedFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedFile {
    path: String,
    bytes: u64,
    sha256: String,
}

/// The one set the desktop selects by default.
pub fn bundled_set() -> SetManifest {
    let entries = PLUGINS
        .iter()
        .map(|id| SetEntry::new(SetKind::Plugin, id, 1).expect("static plugin id"))
        .collect();
    SetManifest::new(SET_SCHEMA_VERSION, SET_ID, entries).expect("static bundled set")
}

/// Launchers accept this exact set and the empty set, nothing else.
pub fn is_bundled_set(set: &SetManifest) -> bool {
    set.id() == SET_ID
        && set.entries().len() == PLUGINS.len()
        && PLUGINS.iter().all(|id| {
            set.entries()
                .iter()
                .any(|e| e.kind() == SetKind::Plugin && e.id() == *id && e.revision() == 1)
        })
}

/// A plugin root whose files were just verified against the lock.
#[derive(Clone, Debug)]
pub struct BundledPlugins {
    root: PathBuf,
    versions: BTreeMap<String, String>,
}
impl BundledPlugins {
    pub fn verify(root: &Path) -> Result<Self, BundledPluginsError> {
        let root = ordinary_directory(root)?;
        let lock_path = root.join(LOCK_FILE);
        let lock_bytes = read_bounded(&lock_path, MAX_LOCK_BYTES)?;
        let lock: Lock =
            serde_json::from_slice(&lock_bytes).map_err(|_| BundledPluginsError::Lock)?;
        if lock.format_version != 1 {
            return Err(BundledPluginsError::Lock);
        }
        let marketplace = read_bounded(&root.join(MARKETPLACE_FILE), MAX_LOCK_BYTES)?;
        if marketplace.len() as u64 != lock.marketplace.bytes
            || hex(&marketplace) != lock.marketplace.sha256
        {
            return Err(BundledPluginsError::Mismatch);
        }
        let mut total = 0usize;
        let mut versions = BTreeMap::new();
        for name in PLUGINS {
            let plugin = lock.plugins.get(name).ok_or(BundledPluginsError::Lock)?;
            let version = &plugin.version;
            if version.is_empty()
                || version.len() > 64
                || !version
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
            {
                return Err(BundledPluginsError::Lock);
            }
            versions.insert(name.to_owned(), version.clone());
            let mut expected = BTreeSet::new();
            for file in &plugin.files {
                total += 1;
                if total > MAX_FILES || !expected.insert(file.path.clone()) {
                    return Err(BundledPluginsError::Limit);
                }
                let path = join_relative(&root.join(name), &file.path)?;
                let bytes = read_bounded(&path, MAX_FILE_BYTES)?;
                if bytes.len() as u64 != file.bytes || hex(&bytes) != file.sha256 {
                    return Err(BundledPluginsError::Mismatch);
                }
            }
            let mut found = BTreeSet::new();
            collect(&root.join(name), "", 0, &mut found)?;
            if found != expected {
                return Err(BundledPluginsError::Unexpected);
            }
        }
        Ok(Self { root, versions })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn plugin_dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    /// Ordinary drive path without the extended `\\?\` prefix, as the harness
    /// command lines and Git Bash hooks expect.
    pub fn plugin_dir_arg(&self, name: &str) -> String {
        let path = self.plugin_dir(name);
        let text = path.to_string_lossy();
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }
    pub fn version(&self, name: &str) -> Option<&str> {
        self.versions.get(name).map(String::as_str)
    }
    /// Codex reads an installed plugin from
    /// `<CODEX_HOME>/plugins/cache/<marketplace>/<plugin>/<version>`. Place the
    /// verified files there (idempotent: identical files are left alone) so the
    /// plugin counts as installed without running `codex plugin add`.
    pub fn install_codex_cache(&self, codex_home: &Path) -> Result<(), BundledPluginsError> {
        for name in PLUGINS {
            let version = self.version(name).ok_or(BundledPluginsError::Lock)?;
            let target = codex_home
                .join("plugins")
                .join("cache")
                .join(MARKETPLACE_NAME)
                .join(name)
                .join(version);
            for relative in self.files(name)? {
                let source = self.plugin_dir(name).join(&relative);
                let destination = target.join(&relative);
                let bytes = read_bounded(&source, MAX_FILE_BYTES)?;
                if let Ok(existing) = fs::read(&destination) {
                    if existing == bytes {
                        continue;
                    }
                }
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|_| BundledPluginsError::Missing)?;
                }
                fs::write(&destination, bytes).map_err(|_| BundledPluginsError::Missing)?;
            }
        }
        Ok(())
    }
    /// The `-c` overrides that register the marketplace and enable both plugins.
    pub fn codex_overrides(&self) -> Vec<String> {
        let root = self.root.to_string_lossy();
        let root = root.strip_prefix(r"\\?\").unwrap_or(&root).replace('\\', "/");
        vec![
            "-c".into(),
            format!(
                "marketplaces.{MARKETPLACE_NAME}={{source_type=\"local\",source={}}}",
                serde_json::to_string(&root).unwrap_or_default()
            ),
            "-c".into(),
            format!(
                "plugins={{{}}}",
                PLUGINS
                    .iter()
                    .map(|name| format!("\"{name}@{MARKETPLACE_NAME}\"={{enabled=true}}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        ]
    }
    /// Relative file list of one plugin, as locked. Used by launchers that copy.
    pub fn files(&self, name: &str) -> Result<Vec<PathBuf>, BundledPluginsError> {
        let mut found = BTreeSet::new();
        collect(&self.plugin_dir(name), "", 0, &mut found)?;
        Ok(found.into_iter().map(PathBuf::from).collect())
    }
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn ordinary_directory(path: &Path) -> Result<PathBuf, BundledPluginsError> {
    let meta = fs::symlink_metadata(path).map_err(|_| BundledPluginsError::Missing)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(BundledPluginsError::UnsafePath);
    }
    path.canonicalize().map_err(|_| BundledPluginsError::Missing)
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, BundledPluginsError> {
    let meta = fs::symlink_metadata(path).map_err(|_| BundledPluginsError::Missing)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(BundledPluginsError::UnsafePath);
    }
    if meta.len() > limit {
        return Err(BundledPluginsError::Limit);
    }
    fs::read(path).map_err(|_| BundledPluginsError::Missing)
}
fn join_relative(base: &Path, relative: &str) -> Result<PathBuf, BundledPluginsError> {
    if relative.is_empty() || relative.len() > 512 || relative.contains('\\') {
        return Err(BundledPluginsError::UnsafePath);
    }
    let mut out = base.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => out.push(part),
            _ => return Err(BundledPluginsError::UnsafePath),
        }
    }
    Ok(out)
}
fn collect(
    dir: &Path,
    prefix: &str,
    depth: usize,
    out: &mut BTreeSet<String>,
) -> Result<(), BundledPluginsError> {
    if depth > MAX_DEPTH {
        return Err(BundledPluginsError::Limit);
    }
    for entry in fs::read_dir(dir).map_err(|_| BundledPluginsError::Missing)? {
        let entry = entry.map_err(|_| BundledPluginsError::Missing)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| BundledPluginsError::UnsafePath)?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| BundledPluginsError::Missing)?;
        if meta.file_type().is_symlink() {
            return Err(BundledPluginsError::UnsafePath);
        }
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if meta.is_dir() {
            collect(&entry.path(), &relative, depth + 1, out)?;
        } else if meta.is_file() {
            out.insert(relative);
            if out.len() > MAX_FILES {
                return Err(BundledPluginsError::Limit);
            }
        } else {
            return Err(BundledPluginsError::UnsafePath);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/agent-plugins")
    }
    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    fn scratch(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "shinatama-bundled-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn pinned_resources_verify() {
        let plugins = BundledPlugins::verify(&repo_root()).unwrap();
        assert!(plugins.plugin_dir("ponytail").ends_with("ponytail"));
        assert!(!plugins.plugin_dir_arg("i-have-adhd").starts_with(r"\\?\"));
        assert!(!plugins.files("ponytail").unwrap().is_empty());
    }
    #[test]
    fn set_is_exactly_the_two_plugins() {
        let set = bundled_set();
        assert!(is_bundled_set(&set));
        let empty = SetManifest::new(SET_SCHEMA_VERSION, SET_ID, Vec::new()).unwrap();
        assert!(!is_bundled_set(&empty));
        let other = SetManifest::new(
            SET_SCHEMA_VERSION,
            SET_ID,
            vec![SetEntry::new(SetKind::Plugin, "ponytail", 1).unwrap()],
        )
        .unwrap();
        assert!(!is_bundled_set(&other));
    }
    #[test]
    fn codex_cache_is_idempotent_and_overrides_name_both_plugins() {
        let plugins = BundledPlugins::verify(&repo_root()).unwrap();
        let home = scratch("codex-home");
        fs::create_dir_all(&home).unwrap();
        plugins.install_codex_cache(&home).unwrap();
        plugins.install_codex_cache(&home).unwrap();
        for name in PLUGINS {
            let version = plugins.version(name).unwrap();
            let installed = home
                .join("plugins/cache")
                .join(MARKETPLACE_NAME)
                .join(name)
                .join(version);
            let mut found = BTreeSet::new();
            collect(&installed, "", 0, &mut found).unwrap();
            assert_eq!(found.len(), plugins.files(name).unwrap().len());
        }
        let overrides = plugins.codex_overrides();
        assert_eq!(overrides.len(), 4);
        assert_eq!(overrides[0], "-c");
        assert!(overrides[1].starts_with("marketplaces.shinatama-bundled={source_type=\"local\",source=\""));
        assert!(overrides[1].contains("agent-plugins"));
        assert!(!overrides[1].contains('\\'));
        assert_eq!(
            overrides[3],
            "plugins={\"ponytail@shinatama-bundled\"={enabled=true},\"i-have-adhd@shinatama-bundled\"={enabled=true}}"
        );
        let _ = fs::remove_dir_all(&home);
    }
    #[test]
    fn tampered_file_is_refused() {
        let copy = scratch("tamper");
        copy_tree(&repo_root(), &copy);
        let target = copy.join("ponytail/LICENSE");
        let mut bytes = fs::read(&target).unwrap();
        bytes[0] ^= 1;
        fs::write(&target, bytes).unwrap();
        assert_eq!(
            BundledPlugins::verify(&copy).unwrap_err(),
            BundledPluginsError::Mismatch
        );
        let _ = fs::remove_dir_all(&copy);
    }
    #[test]
    fn unlisted_file_is_refused() {
        let copy = scratch("extra");
        copy_tree(&repo_root(), &copy);
        fs::write(copy.join("i-have-adhd/extra.js"), b"x").unwrap();
        assert_eq!(
            BundledPlugins::verify(&copy).unwrap_err(),
            BundledPluginsError::Unexpected
        );
        let _ = fs::remove_dir_all(&copy);
    }
}
