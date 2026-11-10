//! Small project-picker preferences in the platform configuration directory.
//! This is not the authoritative database for sessions, threads or agent data.

pub mod collisions;
pub mod create;
pub mod git;
pub mod git_init;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

const SCHEMA_VERSION: u32 = 1;
const HISTORY_LIMIT: usize = 10;
const MAX_CONFIG_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct ProjectEntry {
    pub path: String,
    pub name: String,
    pub last_opened: i64,
    pub available: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedProject {
    path: String,
    name: String,
    last_opened: i64,
}

impl SavedProject {
    fn visible(&self) -> ProjectEntry {
        ProjectEntry {
            path: self.path.clone(),
            name: self.name.clone(),
            last_opened: self.last_opened,
            available: Path::new(&self.path).is_dir(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedPreferences {
    schema_version: u32,
    projects: Vec<SavedProject>,
}

struct State {
    projects: Vec<SavedProject>,
    // Exact bytes establish whether another window changed the file since loading.
    original: Option<Vec<u8>>,
}

pub struct ProjectStore {
    directory: PathBuf,
    state: Mutex<State>,
}

impl ProjectStore {
    /// Read the existing catalog before validating a data location. This never
    /// creates a directory or lock file and preserves invalid source bytes.
    pub fn known_paths(directory: &Path) -> Result<Vec<PathBuf>> {
        match read_config(&directory.join("projects.json"))? {
            Some(bytes) => Ok(decode_config(&bytes)?
                .into_iter()
                .map(|project| PathBuf::from(project.path))
                .collect()),
            None => Ok(Vec::new()),
        }
    }

    /// Open picker preferences only. Invalid or future files are preserved unchanged.
    pub fn open(directory: &Path) -> Result<Self> {
        if !directory.is_absolute() {
            bail!("Project settings directory must be an absolute path");
        }
        create_private_directory(directory)?;
        let original = read_config(&directory.join("projects.json"))?;
        let mut projects = match &original {
            Some(bytes) => decode_config(bytes)?,
            None => Vec::new(),
        };
        projects.sort_by_key(|project| std::cmp::Reverse(project.last_opened));
        Ok(Self {
            directory: directory.to_path_buf(),
            state: Mutex::new(State { projects, original }),
        })
    }

    /// Missing folders remain in the history and are reported as unavailable.
    pub fn recent(&self) -> Result<Vec<ProjectEntry>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Project settings are unavailable"))?;
        let current = read_config(&self.directory.join("projects.json"))?;
        if current != state.original {
            let mut projects = match &current {
                Some(bytes) => decode_config(bytes)?,
                None => Vec::new(),
            };
            projects.sort_by_key(|project| std::cmp::Reverse(project.last_opened));
            state.projects = projects;
            state.original = current;
        }
        let projects = state.projects.clone();
        drop(state);
        Ok(projects.iter().map(SavedProject::visible).collect())
    }

    /// Select an existing absolute directory; never create or modify that directory.
    pub fn select(&self, path: &Path) -> Result<ProjectEntry> {
        let canonical = canonical_project_path(path)?;
        let name = Path::new(&canonical)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&canonical)
            .to_string();
        let key = path_key(&canonical);
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Project settings are unavailable"))?;

        // The OS releases this lock if a writer crashes. Keep the lock file itself
        // in place so all writers lock the same underlying file.
        let lock = private_file(&self.directory.join("projects.lock"), false)?;
        lock.try_lock().map_err(|error| {
            anyhow::anyhow!("Another window is updating project settings; retry: {error}")
        })?;
        let config_path = self.directory.join("projects.json");
        if read_config(&config_path)? != state.original {
            bail!(
                "Project settings changed in another window. Reload the project history before selecting a folder"
            );
        }

        // A monotonic tie-break preserves explicit last-open order within one millisecond.
        let previous_time = state
            .projects
            .first()
            .map_or(0, |project| project.last_opened);
        let project = SavedProject {
            path: canonical,
            name,
            last_opened: chrono::Utc::now()
                .timestamp_millis()
                .max(previous_time.saturating_add(1)),
        };
        let mut projects = state.projects.clone();
        projects.retain(|entry| path_key(&entry.path) != key);
        projects.insert(0, project.clone());
        projects.truncate(HISTORY_LIMIT);

        let mut bytes = serde_json::to_vec_pretty(&SavedPreferences {
            schema_version: SCHEMA_VERSION,
            projects: projects.clone(),
        })?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            bail!("Project settings exceed the supported size");
        }
        atomic_replace(&self.directory, &config_path, &bytes)?;
        // No fallible operation follows the replacement: an error never presents a
        // new selection in memory while leaving the old preferences on disk.
        state.projects = projects;
        state.original = Some(bytes);
        Ok(project.visible())
    }
}

fn read_config(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Cannot read project settings"),
    };
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
            GetFileInformationByHandle,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(std::io::Error::last_os_error()).context("Cannot inspect project settings");
        }
        if info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
            || info.nNumberOfLinks != 1
        {
            bail!(
                "Project settings must be a regular file without symbolic or hard links. The original has been preserved"
            );
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .context("Cannot read project settings")?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        bail!("Project settings are too large. The original file has been preserved");
    }
    Ok(Some(bytes))
}

fn decode_config(bytes: &[u8]) -> Result<Vec<SavedProject>> {
    let saved: SavedPreferences = serde_json::from_slice(bytes).context(
        "Project settings are invalid or unsupported. The original file has been preserved",
    )?;
    if saved.schema_version != SCHEMA_VERSION {
        bail!(
            "Project settings use unsupported schema version {}. The original file has been preserved",
            saved.schema_version
        );
    }
    if saved.projects.len() > HISTORY_LIMIT {
        bail!("Project history exceeds 10 entries. The original file has been preserved");
    }
    let mut keys = HashSet::new();
    for project in &saved.projects {
        if !Path::new(&project.path).is_absolute()
            || project.name.is_empty()
            || project.last_opened < 0
            || !keys.insert(path_key(&project.path))
        {
            bail!("Project settings contain invalid entries. The original file has been preserved");
        }
    }
    Ok(saved.projects)
}

fn canonical_project_path(path: &Path) -> Result<String> {
    if !path.is_absolute() {
        bail!("Choose an absolute project folder path");
    }
    let canonical = path
        .canonicalize()
        .context("Project folder does not exist")?;
    if !canonical.is_dir() {
        bail!("Choose a project folder, not a file");
    }
    let path = canonical
        .to_str()
        .context("Project folder path must be valid Unicode")?;
    #[cfg(windows)]
    {
        let mut normal = if let Some(unc) = path.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{unc}")
        } else {
            path.strip_prefix("\\\\?\\").unwrap_or(path).to_string()
        };
        if normal.as_bytes().get(1) == Some(&b':') {
            normal.replace_range(..1, &normal[..1].to_ascii_uppercase());
        }
        Ok(normal)
    }
    #[cfg(not(windows))]
    {
        Ok(path.to_string())
    }
}

fn path_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

fn create_private_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .context("Cannot create project settings directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn private_file(path: &Path, exclusive: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if exclusive {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .context("Cannot open project settings file")
}

fn atomic_replace(directory: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = directory.join(format!("projects.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = private_file(&temporary, true)?;
        file.write_all(bytes)
            .context("Cannot write project settings")?;
        file.sync_all().context("Cannot flush project settings")?;
        drop(file);
        fs::rename(&temporary, destination).context("Cannot replace project settings")?;
        // Directory flush is best effort after the atomic replacement. Platforms may
        // not support it; do not report an already-committed write as a failed update.
        #[cfg(unix)]
        if let Ok(directory) = File::open(directory) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("shinatama-projects-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn config(&self) -> PathBuf {
            self.0.join("config")
        }
        fn project(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir(&path).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn location_preflight_reads_catalog_without_creating_files() {
        let fixture = Fixture::new();
        assert!(
            ProjectStore::known_paths(&fixture.config())
                .unwrap()
                .is_empty()
        );
        assert!(!fixture.config().exists());
        let project = fixture.project("known-project");
        let selected = ProjectStore::open(&fixture.config())
            .unwrap()
            .select(&project)
            .unwrap();
        assert_eq!(
            ProjectStore::known_paths(&fixture.config()).unwrap(),
            vec![PathBuf::from(selected.path)]
        );
        let config = fixture.config().join("projects.json");
        fs::write(&config, b"unsupported original").unwrap();
        assert!(ProjectStore::known_paths(&fixture.config()).is_err());
        assert_eq!(fs::read(config).unwrap(), b"unsupported original");
    }

    #[cfg(windows)]
    #[test]
    fn location_preflight_refuses_hard_link_before_reading_json() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.config()).unwrap();
        let original = fixture.0.join("unrelated.json");
        fs::write(&original, b"{\"schema_version\":1,\"projects\":[]}").unwrap();
        fs::hard_link(&original, fixture.config().join("projects.json")).unwrap();
        let error = ProjectStore::known_paths(&fixture.config())
            .unwrap_err()
            .to_string();
        assert!(error.contains("without symbolic or hard links"));
        assert_eq!(
            fs::read(original).unwrap(),
            b"{\"schema_version\":1,\"projects\":[]}"
        );
    }

    #[test]
    fn selection_survives_restart_and_does_not_write_in_project() {
        let fixture = Fixture::new();
        let project = fixture.project("Caffè & notes");
        let selected = ProjectStore::open(&fixture.config())
            .unwrap()
            .select(&project)
            .unwrap();
        let reopened = ProjectStore::open(&fixture.config()).unwrap();
        let recent = reopened.recent().unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].path, selected.path);
        assert!(recent[0].available);
        assert_eq!(fs::read_dir(project).unwrap().count(), 0);
    }

    #[test]
    fn aliases_deduplicate_and_reselection_updates_order() {
        let fixture = Fixture::new();
        let first = fixture.project("first");
        let second = fixture.project("second");
        let store = ProjectStore::open(&fixture.config()).unwrap();
        let selected = store.select(&first).unwrap();
        store.select(&second).unwrap();
        let reselected = store.select(&first.join(".")).unwrap();
        let recent = store.recent().unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].path, selected.path);
        assert!(reselected.last_opened > selected.last_opened);
        #[cfg(windows)]
        {
            store
                .select(&PathBuf::from(selected.path.to_lowercase()))
                .unwrap();
            assert_eq!(store.recent().unwrap().len(), 2);
        }
    }

    #[test]
    fn history_is_bounded_and_missing_folders_remain_visible() {
        let fixture = Fixture::new();
        let store = ProjectStore::open(&fixture.config()).unwrap();
        let mut last = PathBuf::new();
        for index in 0..12 {
            last = fixture.project(&format!("project-{index}"));
            store.select(&last).unwrap();
        }
        fs::remove_dir(&last).unwrap();
        let recent = store.recent().unwrap();
        assert_eq!(recent.len(), 10);
        assert_eq!(recent[0].name, "project-11");
        assert!(!recent[0].available);
        assert!(store.select(&last).is_err());
        assert_eq!(store.recent().unwrap().len(), 10);
    }

    #[test]
    fn invalid_selection_preserves_memory_and_file() {
        let fixture = Fixture::new();
        let store = ProjectStore::open(&fixture.config()).unwrap();
        store.select(&fixture.project("valid")).unwrap();
        let before = fs::read(fixture.config().join("projects.json")).unwrap();
        let file = fixture.0.join("not-a-folder");
        fs::write(&file, b"fixture").unwrap();
        for invalid in [Path::new("relative"), &file, &fixture.0.join("missing")] {
            assert!(store.select(invalid).is_err());
        }
        assert_eq!(store.recent().unwrap().len(), 1);
        assert_eq!(
            fs::read(fixture.config().join("projects.json")).unwrap(),
            before
        );
    }

    #[test]
    fn malformed_future_and_oversized_preferences_are_preserved() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.config()).unwrap();
        for bytes in [
            b"{".to_vec(),
            br#"{"schema_version":2,"projects":[]}"#.to_vec(),
            vec![b'x'; MAX_CONFIG_BYTES as usize + 1],
        ] {
            let path = fixture.config().join("projects.json");
            fs::write(&path, &bytes).unwrap();
            assert!(ProjectStore::open(&fixture.config()).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn a_stale_window_cannot_overwrite_another_windows_selection() {
        let fixture = Fixture::new();
        let first = ProjectStore::open(&fixture.config()).unwrap();
        let stale = ProjectStore::open(&fixture.config()).unwrap();
        let selected = first.select(&fixture.project("first")).unwrap();
        let before = fs::read(fixture.config().join("projects.json")).unwrap();
        assert!(stale.select(&fixture.project("second")).is_err());
        assert_eq!(
            fs::read(fixture.config().join("projects.json")).unwrap(),
            before
        );
        assert_eq!(stale.recent().unwrap()[0].path, selected.path);
        // Refresh is the explicit recovery path; subsequent selection can commit.
        assert!(stale.select(&fixture.0.join("second")).is_ok());
        assert_eq!(
            ProjectStore::open(&fixture.config())
                .unwrap()
                .recent()
                .unwrap()[0]
                .name,
            "second"
        );
    }

    #[test]
    fn failed_replacement_keeps_previous_selection() {
        let fixture = Fixture::new();
        let store = ProjectStore::open(&fixture.config()).unwrap();
        store.select(&fixture.project("first")).unwrap();
        let original = store.recent().unwrap()[0].path.clone();
        // An externally replaced config is a failed optimistic write, never an overwrite.
        fs::write(fixture.config().join("projects.json"), b"external-change").unwrap();
        assert!(store.select(&fixture.project("second")).is_err());
        assert!(store.recent().is_err());
        assert_eq!(store.state.lock().unwrap().projects[0].path, original);
        assert_eq!(
            fs::read(fixture.config().join("projects.json")).unwrap(),
            b"external-change"
        );
    }

    #[test]
    fn competing_writer_fails_safely_and_releases_its_os_lock() {
        let fixture = Fixture::new();
        let store = ProjectStore::open(&fixture.config()).unwrap();
        let project = fixture.project("project");
        let lock = private_file(&fixture.config().join("projects.lock"), false).unwrap();
        lock.try_lock().unwrap();
        assert!(store.select(&project).is_err());
        assert!(!fixture.config().join("projects.json").exists());
        drop(lock);
        assert!(store.select(&project).is_ok());
    }

    #[test]
    fn selections_from_threads_share_one_consistent_history() {
        let fixture = Fixture::new();
        let store = std::sync::Arc::new(ProjectStore::open(&fixture.config()).unwrap());
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let path = fixture.project(&format!("thread-{index}"));
                let store = store.clone();
                std::thread::spawn(move || store.select(&path).unwrap())
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let recent = store.recent().unwrap();
        assert_eq!(recent.len(), 8);
        assert!(
            recent
                .windows(2)
                .all(|pair| pair[0].last_opened > pair[1].last_opened)
        );
        assert_eq!(
            ProjectStore::open(&fixture.config())
                .unwrap()
                .recent()
                .unwrap()
                .len(),
            8
        );
    }

    #[cfg(unix)]
    #[test]
    fn preferences_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let store = ProjectStore::open(&fixture.config()).unwrap();
        store.select(&fixture.project("private")).unwrap();
        assert_eq!(
            fs::metadata(fixture.config()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(fixture.config().join("projects.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
