//! Claude Code mods shipped beside the bundled plugins: display-only
//! extensions that only Claude Code loads (`resources/claude-mods`, next to
//! `resources/agent-plugins`). Kept apart from `bundled_plugins` on purpose:
//! their own folder and their own lock, so a mod that is missing, altered or
//! not tried with the running Claude Code is dropped alone. The bundled
//! plugins and every other harness launch exactly as without it, and no doubt
//! about a mod ever blocks a launch. Pure local reads and copies, no network,
//! no execution, no user configuration touched.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

pub const MODS: [&str; 1] = ["shn-render"];
/// Sibling of the bundled plugins' folder, in the repository and the package.
pub const DIRECTORY: &str = "claude-mods";
pub const LOCK_FILE: &str = "mods.lock.json";
/// `SHINATAMA_CLAUDE_MODS=off` turns the mods off alone;
/// `SHINATAMA_BUNDLED_PLUGINS=off` turns them off with the plugins.
pub const SWITCH: &str = "SHINATAMA_CLAUDE_MODS";
/// Claude Code versions the mods were run with. Claude's mod interface is
/// early access and may change between releases, so any other version starts
/// without them; a version is added here only after a run with it.
pub const TRIED_CLAUDE_VERSIONS: [&str; 1] = ["2.1.322"];
const MAX_LOCK_BYTES: u64 = 64 * 1024;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_FILES: usize = 64;
const MAX_DEPTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeModsError {
    Missing,
    Lock,
    UnsafePath,
    Mismatch,
    Unexpected,
    Limit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Lock {
    format_version: u32,
    mods: BTreeMap<String, LockedMod>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedMod {
    #[allow(dead_code)]
    version: String,
    files: Vec<LockedFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedFile {
    path: String,
    bytes: u64,
    sha256: String,
}

/// A mods root whose files were just verified against its lock.
#[derive(Clone, Debug)]
pub struct ClaudeMods {
    root: PathBuf,
}
impl ClaudeMods {
    /// The mods for one Claude launch, or None: switched off, a Claude version
    /// they were not tried with, no folder beside `plugins_root`, or any file
    /// that does not match the lock. Claude gets a private copy under
    /// `destination` (it writes into the directories it loads).
    pub fn for_claude_launch(
        plugins_root: &Path,
        claude_version: &str,
        destination: &Path,
    ) -> Option<Self> {
        let switched_off =
            std::env::var(SWITCH).is_ok_and(|value| value.eq_ignore_ascii_case("off"));
        if switched_off || !TRIED_CLAUDE_VERSIONS.contains(&claude_version) {
            return None;
        }
        let root = plugins_root.parent()?.join(DIRECTORY);
        Self::verify(&root).ok()?.private_copy(destination).ok()
    }

    pub fn verify(root: &Path) -> Result<Self, ClaudeModsError> {
        let root = ordinary_directory(root)?;
        let lock: Lock =
            serde_json::from_slice(&read_bounded(&root.join(LOCK_FILE), MAX_LOCK_BYTES)?)
                .map_err(|_| ClaudeModsError::Lock)?;
        if lock.format_version != 1 || lock.mods.len() != MODS.len() {
            return Err(ClaudeModsError::Lock);
        }
        for name in MODS {
            let locked = lock.mods.get(name).ok_or(ClaudeModsError::Lock)?;
            if locked.files.len() > MAX_FILES {
                return Err(ClaudeModsError::Limit);
            }
            let mut expected = BTreeSet::new();
            for file in &locked.files {
                if !expected.insert(file.path.clone()) {
                    return Err(ClaudeModsError::Lock);
                }
                let bytes = read_bounded(
                    &join_relative(&root.join(name), &file.path)?,
                    MAX_FILE_BYTES,
                )?;
                if bytes.len() as u64 != file.bytes || hex(&bytes) != file.sha256 {
                    return Err(ClaudeModsError::Mismatch);
                }
            }
            let mut found = BTreeSet::new();
            collect(&root.join(name), "", 0, &mut found)?;
            if found != expected {
                return Err(ClaudeModsError::Unexpected);
            }
        }
        Ok(Self { root })
    }

    pub fn mod_dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// Writes the verified files under `destination` (identical files left
    /// alone; whatever Claude adds stays in the copy) and points at the copy.
    pub fn private_copy(&self, destination: &Path) -> Result<Self, ClaudeModsError> {
        for name in MODS {
            let mut files = BTreeSet::new();
            collect(&self.mod_dir(name), "", 0, &mut files)?;
            for relative in files {
                let source = join_relative(&self.mod_dir(name), &relative)?;
                let target = join_relative(&destination.join(name), &relative)?;
                let bytes = read_bounded(&source, MAX_FILE_BYTES)?;
                if fs::read(&target).is_ok_and(|existing| existing == bytes) {
                    continue;
                }
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|_| ClaudeModsError::Missing)?;
                }
                fs::write(&target, bytes).map_err(|_| ClaudeModsError::Missing)?;
            }
        }
        Ok(Self {
            root: destination.to_path_buf(),
        })
    }

    /// One `--plugin-dir <ordinary drive path>` pair per mod.
    pub fn arguments(&self) -> Vec<OsString> {
        let mut args = Vec::new();
        for name in MODS {
            let path = self.mod_dir(name);
            let text = path.to_string_lossy();
            args.push("--plugin-dir".into());
            args.push(
                text.strip_prefix(r"\\?\")
                    .unwrap_or(&text)
                    .to_owned()
                    .into(),
            );
        }
        args
    }
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn ordinary_directory(path: &Path) -> Result<PathBuf, ClaudeModsError> {
    let meta = fs::symlink_metadata(path).map_err(|_| ClaudeModsError::Missing)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(ClaudeModsError::UnsafePath);
    }
    path.canonicalize().map_err(|_| ClaudeModsError::Missing)
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, ClaudeModsError> {
    let meta = fs::symlink_metadata(path).map_err(|_| ClaudeModsError::Missing)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(ClaudeModsError::UnsafePath);
    }
    if meta.len() > limit {
        return Err(ClaudeModsError::Limit);
    }
    fs::read(path).map_err(|_| ClaudeModsError::Missing)
}
fn join_relative(base: &Path, relative: &str) -> Result<PathBuf, ClaudeModsError> {
    if relative.is_empty() || relative.len() > 512 || relative.contains('\\') {
        return Err(ClaudeModsError::UnsafePath);
    }
    let mut out = base.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => out.push(part),
            _ => return Err(ClaudeModsError::UnsafePath),
        }
    }
    Ok(out)
}
fn collect(
    dir: &Path,
    prefix: &str,
    depth: usize,
    out: &mut BTreeSet<String>,
) -> Result<(), ClaudeModsError> {
    if depth > MAX_DEPTH {
        return Err(ClaudeModsError::Limit);
    }
    for entry in fs::read_dir(dir).map_err(|_| ClaudeModsError::Missing)? {
        let entry = entry.map_err(|_| ClaudeModsError::Missing)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| ClaudeModsError::UnsafePath)?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| ClaudeModsError::Missing)?;
        if meta.file_type().is_symlink() {
            return Err(ClaudeModsError::UnsafePath);
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
                return Err(ClaudeModsError::Limit);
            }
        } else {
            return Err(ClaudeModsError::UnsafePath);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../resources")
            .join(DIRECTORY)
    }
    fn plugins_root() -> PathBuf {
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
            "shinatama-claude-mods-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn pinned_resources_verify() {
        let mods = ClaudeMods::verify(&repo_root()).unwrap();
        assert!(mods.mod_dir("shn-render").ends_with("shn-render"));
        let args = mods.arguments();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--plugin-dir");
        assert!(!args[1].to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn a_launch_gets_the_private_copy_only_on_a_tried_version() {
        let destination = scratch("launch");
        let tried = TRIED_CLAUDE_VERSIONS[0];
        let mods = ClaudeMods::for_claude_launch(&plugins_root(), tried, &destination).unwrap();
        assert!(mods.mod_dir("shn-render").starts_with(&destination));
        assert!(destination.join("shn-render/hooks/register.ts").is_file());
        assert!(
            ClaudeMods::for_claude_launch(&plugins_root(), "2.1.321", &scratch("old")).is_none()
        );
        assert!(!scratch("old").exists());
        // What Claude writes into the copy stays there; the pinned tree stays clean.
        fs::write(destination.join("shn-render/tsconfig.json"), b"{}").unwrap();
        assert!(ClaudeMods::for_claude_launch(&plugins_root(), tried, &destination).is_some());
        assert!(ClaudeMods::verify(&repo_root()).is_ok());
        let _ = fs::remove_dir_all(destination);
    }

    #[test]
    fn a_missing_folder_or_an_altered_file_drops_the_mods_alone() {
        let base = scratch("altered");
        let plugins = base.join("agent-plugins");
        fs::create_dir_all(&plugins).unwrap();
        let tried = TRIED_CLAUDE_VERSIONS[0];
        assert!(ClaudeMods::for_claude_launch(&plugins, tried, &base.join("copy")).is_none());
        copy_tree(&repo_root(), &base.join(DIRECTORY));
        assert!(ClaudeMods::for_claude_launch(&plugins, tried, &base.join("copy")).is_some());
        let register = base.join(DIRECTORY).join("shn-render/hooks/register.ts");
        let mut bytes = fs::read(&register).unwrap();
        bytes.push(b'\n');
        fs::write(&register, bytes).unwrap();
        assert_eq!(
            ClaudeMods::verify(&base.join(DIRECTORY)).unwrap_err(),
            ClaudeModsError::Mismatch
        );
        assert!(ClaudeMods::for_claude_launch(&plugins, tried, &base.join("copy2")).is_none());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn an_extra_file_is_refused() {
        let base = scratch("extra");
        copy_tree(&repo_root(), &base);
        fs::write(base.join("shn-render/hooks/extra.ts"), b"export {}").unwrap();
        assert_eq!(
            ClaudeMods::verify(&base).unwrap_err(),
            ClaudeModsError::Unexpected
        );
        let _ = fs::remove_dir_all(base);
    }
}
