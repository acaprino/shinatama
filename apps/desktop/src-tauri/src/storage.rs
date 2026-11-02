//! Validate both the shared data root and the legacy picker directory before writing.
//! C02 imports the latter without removing the original preferences.
use shinatama_projects::ProjectStore;
use shinatama_state::location::{LocationRequest, StorageLocation};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub struct PreparedStorage {
    pub data: StorageLocation,
    pub legacy: StorageLocation,
}

pub struct WorkspaceServices {
    pub store: ProjectStore,
    pub location: StorageLocation,
    pub _legacy: StorageLocation,
}

pub struct Services {
    directory: Result<PathBuf, String>,
    loaded: OnceLock<Result<WorkspaceServices, String>>,
}

impl Services {
    pub fn new(directory: Result<PathBuf, String>) -> Self {
        Self {
            directory,
            loaded: OnceLock::new(),
        }
    }

    /// Blocking IO belongs to an IPC worker, never Tauri's window setup thread.
    pub fn load(&self) -> Result<&WorkspaceServices, String> {
        self.loaded
            .get_or_init(|| {
                let directory = self.directory.as_ref().map_err(Clone::clone)?;
                let prepared = prepare(directory)?;
                let store = ProjectStore::open(prepared.legacy.directory())
                    .map_err(|e| format!("{e:#}"))?;
                Ok(WorkspaceServices {
                    store,
                    location: prepared.data,
                    _legacy: prepared.legacy,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}

pub fn prepare(legacy_directory: &Path) -> Result<PreparedStorage, String> {
    StorageLocation::preflight(legacy_directory).map_err(|e| e.to_string())?;
    let mut roots = ProjectStore::known_paths(legacy_directory).map_err(|e| format!("{e:#}"))?;
    roots.push(std::env::current_dir().map_err(|e| e.to_string())?);
    let request = LocationRequest::from_environment(roots).map_err(|e| e.to_string())?;
    let location = StorageLocation::resolve(request.clone()).map_err(|e| e.to_string())?;
    let mut legacy_request = request;
    legacy_request.home_override = Some(legacy_directory.to_path_buf());
    let legacy = StorageLocation::resolve(legacy_request).map_err(|e| e.to_string())?;
    // Resolve both first: an invalid legacy override must not trigger a silent fallback.
    location
        .prepare_private_directory()
        .map_err(|e| e.to_string())?;
    legacy
        .prepare_private_directory()
        .map_err(|e| e.to_string())?;
    for name in ["projects.json", "projects.lock"] {
        let path = legacy.directory().join(name);
        if path.try_exists().map_err(|e| e.to_string())? {
            legacy
                .secure_private_file(&path)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(PreparedStorage {
        data: location,
        legacy,
    })
}
