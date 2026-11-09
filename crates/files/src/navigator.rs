//! C41.1: read-only navigation inside one authorized project root (FRD-0001 R35).
//!
//! Every path is a relative, component-checked path below the canonical root; the result is
//! canonicalized again and must still be inside the root, so `..`, absolute paths, drive or
//! UNC prefixes, alternate data streams, device names and links pointing outside are refused.
//! Traversal never descends through a link or junction. Listing and search are bounded in
//! depth, entries and time, and stop when cancelled. Nothing is ever executed: opening reads
//! bytes for display.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const MAX_DEPTH: usize = 32;
pub const MAX_DIR_ENTRIES: usize = 2_000;
pub const MAX_SEARCH_VISITS: usize = 50_000;
pub const MAX_SEARCH_HITS: usize = 200;
pub const MAX_READ_BYTES: usize = 4 * 1024 * 1024;
pub const SEARCH_DEADLINE: Duration = Duration::from_secs(5);
/// Heavy folders listed but never searched.
pub const SEARCH_SKIP: [&str; 5] = [".git", "node_modules", "target", ".venv", "__pycache__"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    /// A symbolic link or junction; never traversed.
    Link,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    /// Forward-slash path relative to the root.
    pub path: String,
    pub kind: EntryKind,
    pub size: Option<u64>,
    /// For a link: its target resolves inside the root.
    pub link_inside: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub path: String,
    pub entries: Vec<Entry>,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub query: String,
    pub hits: Vec<Entry>,
    pub truncated: bool,
    pub cancelled: bool,
    pub timed_out: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFile {
    pub path: String,
    pub text: Option<String>,
    pub binary: bool,
    pub truncated: bool,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NavError {
    InvalidPath,
    OutsideRoot,
    NotFound,
    NotADirectory,
    NotAFile,
    TooDeep,
    Cancelled,
    Unreadable,
}
impl std::fmt::Display for NavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NavError::InvalidPath => "The path is not a plain path inside the project.",
            NavError::OutsideRoot => "The path leads outside the project; it is not opened.",
            NavError::NotFound => "The file or folder does not exist.",
            NavError::NotADirectory => "This is not a folder.",
            NavError::NotAFile => "This is not a file.",
            NavError::TooDeep => "The folder is nested too deeply.",
            NavError::Cancelled => "The scan was cancelled.",
            NavError::Unreadable => "The file or folder cannot be read.",
        })
    }
}

pub type Result<T> = std::result::Result<T, NavError>;

/// The authorized root, canonical once.
#[derive(Clone, Debug)]
pub struct Root {
    canonical: PathBuf,
}

const DEVICE_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3",
    "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn valid_component(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or_default().to_ascii_lowercase();
    !part.is_empty()
        && part != "."
        && part != ".."
        && part.len() <= 255
        && !part.ends_with([' ', '.'])
        && !part.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        && !DEVICE_NAMES.contains(&stem.as_str())
}

fn is_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT (junctions too)
    }
    #[cfg(not(windows))]
    false
}

fn strip_verbatim(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

impl Root {
    pub fn open(path: &Path) -> Result<Self> {
        let canonical = fs::canonicalize(path).map_err(|_| NavError::NotFound)?;
        if !fs::metadata(&canonical).map_err(|_| NavError::Unreadable)?.is_dir() {
            return Err(NavError::NotADirectory);
        }
        Ok(Self { canonical })
    }
    pub fn path(&self) -> &Path {
        &self.canonical
    }
    pub fn display(&self) -> String {
        strip_verbatim(&self.canonical)
    }

    /// Joins a relative path after checking every component; no filesystem access.
    fn join(&self, relative: &str) -> Result<(PathBuf, Vec<String>)> {
        if relative.len() > 4096 {
            return Err(NavError::InvalidPath);
        }
        let parts: Vec<String> = relative.split(['/', '\\']).filter(|p| !p.is_empty()).map(str::to_owned).collect();
        if parts.len() > MAX_DEPTH {
            return Err(NavError::TooDeep);
        }
        if relative.starts_with(['/', '\\']) || parts.iter().any(|p| !valid_component(p)) {
            return Err(NavError::InvalidPath);
        }
        let mut path = self.canonical.clone();
        for part in &parts {
            path.push(part);
        }
        Ok((path, parts))
    }

    /// The canonical target of `relative`, which must be inside the root (links included).
    pub fn resolve(&self, relative: &str) -> Result<PathBuf> {
        let (joined, _) = self.join(relative)?;
        let canonical = fs::canonicalize(&joined).map_err(|_| NavError::NotFound)?;
        if !canonical.starts_with(&self.canonical) {
            return Err(NavError::OutsideRoot);
        }
        Ok(canonical)
    }

    fn relative_of(&self, path: &Path) -> String {
        path.strip_prefix(&self.canonical)
            .map(|p| p.components().filter_map(|c| match c {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            }).collect::<Vec<_>>().join("/"))
            .unwrap_or_default()
    }

    fn entry(&self, path: &Path, meta: &fs::Metadata) -> Entry {
        let link = is_link(meta);
        Entry {
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            path: self.relative_of(path),
            kind: if link { EntryKind::Link } else if meta.is_dir() { EntryKind::Directory } else { EntryKind::File },
            size: (!link && meta.is_file()).then(|| meta.len()),
            link_inside: link.then(|| fs::canonicalize(path).is_ok_and(|t| t.starts_with(&self.canonical))),
        }
    }

    /// One folder, sorted folders first, at most [`MAX_DIR_ENTRIES`].
    pub fn list(&self, relative: &str, cancel: &AtomicBool) -> Result<Listing> {
        let directory = self.resolve(relative)?;
        let meta = fs::metadata(&directory).map_err(|_| NavError::Unreadable)?;
        if !meta.is_dir() {
            return Err(NavError::NotADirectory);
        }
        let mut entries = Vec::new();
        let mut truncated = false;
        for item in fs::read_dir(&directory).map_err(|_| NavError::Unreadable)? {
            if cancel.load(Ordering::Relaxed) {
                return Err(NavError::Cancelled);
            }
            let Ok(item) = item else { continue };
            if entries.len() >= MAX_DIR_ENTRIES {
                truncated = true;
                break;
            }
            let path = item.path();
            let Ok(meta) = fs::symlink_metadata(&path) else { continue };
            entries.push(self.entry(&path, &meta));
        }
        entries.sort_by(|a, b| (a.kind != EntryKind::Directory).cmp(&(b.kind != EntryKind::Directory)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(Listing { path: self.relative_of(&directory), entries, truncated })
    }

    /// Case-insensitive name search, breadth first, never through links or [`SEARCH_SKIP`].
    pub fn search(&self, query: &str, cancel: &AtomicBool) -> Result<SearchResult> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() || needle.len() > 128 || needle.chars().any(char::is_control) {
            return Err(NavError::InvalidPath);
        }
        let started = Instant::now();
        let mut result = SearchResult { query: query.trim().to_owned(), hits: Vec::new(), truncated: false, cancelled: false, timed_out: false };
        let mut queue = std::collections::VecDeque::from([(self.canonical.clone(), 0usize)]);
        let mut visits = 0usize;
        'scan: while let Some((directory, depth)) = queue.pop_front() {
            let Ok(items) = fs::read_dir(&directory) else { continue };
            for item in items {
                if cancel.load(Ordering::Relaxed) {
                    result.cancelled = true;
                    break 'scan;
                }
                if started.elapsed() > SEARCH_DEADLINE {
                    result.timed_out = true;
                    break 'scan;
                }
                visits += 1;
                if visits > MAX_SEARCH_VISITS {
                    result.truncated = true;
                    break 'scan;
                }
                let Ok(item) = item else { continue };
                let path = item.path();
                let Ok(meta) = fs::symlink_metadata(&path) else { continue };
                let name = item.file_name().to_string_lossy().into_owned();
                if name.to_lowercase().contains(&needle) {
                    if result.hits.len() >= MAX_SEARCH_HITS {
                        result.truncated = true;
                        break 'scan;
                    }
                    result.hits.push(self.entry(&path, &meta));
                }
                if meta.is_dir() && !is_link(&meta) && depth + 1 < MAX_DEPTH && !SEARCH_SKIP.contains(&name.as_str()) {
                    queue.push_back((path, depth + 1));
                }
            }
        }
        Ok(result)
    }

    /// Reads a file for display: text when valid UTF-8 without NUL, else marked binary.
    pub fn read(&self, relative: &str) -> Result<TextFile> {
        let path = self.resolve(relative)?;
        let meta = fs::metadata(&path).map_err(|_| NavError::Unreadable)?;
        if !meta.is_file() {
            return Err(NavError::NotAFile);
        }
        let mut file = fs::File::open(&path).map_err(|_| NavError::Unreadable)?;
        let mut bytes = Vec::with_capacity((meta.len() as usize).min(MAX_READ_BYTES));
        file.by_ref().take(MAX_READ_BYTES as u64).read_to_end(&mut bytes).map_err(|_| NavError::Unreadable)?;
        let truncated = meta.len() > MAX_READ_BYTES as u64;
        let sha256 = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        let text = std::str::from_utf8(&bytes)
            .ok()
            .or_else(|| truncated.then(|| std::str::from_utf8(&bytes[..utf8_prefix(&bytes)]).ok()).flatten())
            .filter(|t| !t.contains('\0'))
            .map(str::to_owned);
        Ok(TextFile { path: self.relative_of(&path), binary: text.is_none(), text, truncated, bytes: meta.len(), sha256 })
    }
}

/// Length of the longest valid UTF-8 prefix (a cut may split the last character).
fn utf8_prefix(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(error) => error.valid_up_to(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn tree() -> Tree {
        let base = std::env::temp_dir().join(format!("shinatama-files-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(base.join("project/src/deep")).unwrap();
        fs::create_dir_all(base.join("project/node_modules/pkg")).unwrap();
        fs::create_dir_all(base.join("outside")).unwrap();
        fs::write(base.join("project/README.md"), "# hi\n").unwrap();
        fs::write(base.join("project/src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(base.join("project/src/deep/needle.txt"), "x").unwrap();
        fs::write(base.join("project/node_modules/pkg/needle.js"), "x").unwrap();
        fs::write(base.join("project/run.bat"), "@echo pwned > %TEMP%\\pwned.txt\n").unwrap();
        fs::write(base.join("project/blob.bin"), [0u8, 159, 146, 150]).unwrap();
        fs::write(base.join("outside/secret.txt"), "secret").unwrap();
        Tree(base)
    }

    #[test]
    fn traversal_device_and_stream_paths_are_refused() {
        let t = tree();
        let root = Root::open(&t.0.join("project")).unwrap();
        for bad in ["../outside/secret.txt", "src/../../outside", "C:/Windows", "/etc", "\\\\server\\share", "README.md:stream", "src/con", "nul.txt", "src/ trailing ", "a/./b"] {
            assert!(matches!(root.read(bad), Err(NavError::InvalidPath)), "{bad}");
        }
        assert_eq!(root.read(&"a/".repeat(40)), Err(NavError::TooDeep));
    }

    #[test]
    fn lists_folders_first_and_reads_text_without_executing() {
        let t = tree();
        let root = Root::open(&t.0.join("project")).unwrap();
        let cancel = AtomicBool::new(false);
        let listing = root.list("", &cancel).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["node_modules", "src", "blob.bin", "README.md", "run.bat"]);
        let script = root.read("run.bat").unwrap();
        assert_eq!(script.text.as_deref(), Some("@echo pwned > %TEMP%\\pwned.txt\n"));
        assert!(!std::env::temp_dir().join("pwned.txt").exists(), "opening never executes");
        assert!(root.read("blob.bin").unwrap().binary);
        assert_eq!(root.read("src"), Err(NavError::NotAFile));
        assert_eq!(root.list("README.md", &cancel), Err(NavError::NotADirectory));
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(root.list("", &cancel), Err(NavError::Cancelled));
    }

    #[test]
    fn search_skips_heavy_folders_and_stops_when_cancelled() {
        let t = tree();
        let root = Root::open(&t.0.join("project")).unwrap();
        let result = root.search("NEEDLE", &AtomicBool::new(false)).unwrap();
        assert_eq!(result.hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(), ["src/deep/needle.txt"]);
        let cancelled = root.search("needle", &AtomicBool::new(true)).unwrap();
        assert!(cancelled.cancelled && cancelled.hits.is_empty());
        assert!(root.search("  ", &AtomicBool::new(false)).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn junctions_out_of_the_root_are_listed_but_never_followed_or_opened() {
        let t = tree();
        let link = t.0.join("project/escape");
        let made = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "New-Item -ItemType Junction -Path $env:LINK -Target $env:TARGET | Out-Null"])
            .env("LINK", &link)
            .env("TARGET", t.0.join("outside"))
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
        let root = Root::open(&t.0.join("project")).unwrap();
        let listing = root.list("", &AtomicBool::new(false)).unwrap();
        let escape = listing.entries.iter().find(|e| e.name == "escape").unwrap();
        assert_eq!((escape.kind, escape.link_inside), (EntryKind::Link, Some(false)));
        assert_eq!(root.read("escape/secret.txt"), Err(NavError::OutsideRoot));
        assert_eq!(root.list("escape", &AtomicBool::new(false)), Err(NavError::OutsideRoot));
        assert!(root.search("secret", &AtomicBool::new(false)).unwrap().hits.is_empty(), "search never descends a junction");
        let _ = fs::remove_dir(&link);
    }
}
