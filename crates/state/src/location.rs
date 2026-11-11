//! Resolve storage without creating files, then prepare an owner-only directory.
//!
//! Known project roots must include the current project and the saved project catalog.
//! Registered Windows sync roots and cloud/remote volume properties are checked as well.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationErrorKind {
    InvalidPath,
    Project,
    Synchronized,
    Network,
    UnsupportedVolume,
    Permissions,
    Io,
}

#[derive(Debug)]
pub struct LocationError {
    pub kind: LocationErrorKind,
    pub path: PathBuf,
    detail: String,
    source: Option<io::Error>,
}

impl LocationError {
    fn new(kind: LocationErrorKind, path: &Path, detail: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.to_path_buf(),
            detail: detail.into(),
            source: None,
        }
    }

    fn io(path: &Path, error: io::Error) -> Self {
        Self {
            kind: LocationErrorKind::Io,
            path: path.to_path_buf(),
            detail: error.to_string(),
            source: Some(error),
        }
    }
}

impl fmt::Display for LocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cartella dati non utilizzabile: {}. {} Imposta SHINATAMA_HOME su una cartella locale privata fuori dai progetti e dalla sincronizzazione.",
            self.path.display(),
            self.detail
        )
    }
}

impl std::error::Error for LocationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|error| error as _)
    }
}

pub type Result<T> = std::result::Result<T, LocationError>;

/// Explicit inputs make tests and secondary processes independent of environment mutation.
#[derive(Debug, Clone)]
pub struct LocationRequest {
    /// The platform data root, not the Shinatama subdirectory.
    pub local_data_directory: PathBuf,
    /// SHINATAMA_HOME names the directory containing shinatama.db.
    pub home_override: Option<PathBuf>,
    pub project_roots: Vec<PathBuf>,
    pub synchronized_roots: Vec<PathBuf>,
}

impl LocationRequest {
    /// Reads location settings and sync-root metadata only; never reads credentials.
    pub fn from_environment(project_roots: Vec<PathBuf>) -> Result<Self> {
        let local_data_directory = platform_data_directory()?;
        let portable = portable_root();
        let home_override = std::env::var_os("SHINATAMA_HOME")
            .map(PathBuf::from)
            .or_else(|| portable.as_ref().map(|root| root.join(PORTABLE_DATA)));
        let project_roots = match (&portable, &home_override) {
            (Some(root), Some(home)) if same_path(home, &root.join(PORTABLE_DATA)) => {
                without_portable_root(project_roots, root)
            }
            _ => project_roots,
        };
        StorageLocation::preflight(home_override.as_deref().unwrap_or(&local_data_directory))?;
        #[cfg(windows)]
        let synchronized_roots = windows::synchronized_roots(&local_data_directory)?;
        #[cfg(not(windows))]
        let synchronized_roots = Vec::new();
        Ok(Self {
            local_data_directory,
            home_override,
            project_roots,
            synchronized_roots,
        })
    }
}

/// A portable install keeps its data beside the program. This file beside Shinatama.exe
/// marks one; programs in a subfolder of the install (`cli\`) find it one level up.
pub const PORTABLE_MARKER: &str = "Shinatama.portable";
/// The data folder of a portable install, inside its root. The WebView2 profile is `webview`.
pub const PORTABLE_DATA: &str = "data";

/// The root folder of the portable install that contains the running program, if any.
pub fn portable_root() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let folder = executable.parent()?;
    [Some(folder), folder.parent()]
        .into_iter()
        .flatten()
        .find(|candidate| candidate.join(PORTABLE_MARKER).is_file())
        .map(Path::to_path_buf)
}

/// The portable folder holds the program beside its data; it and its ancestors are not
/// projects. Git work trees are still refused by `resolve`.
fn without_portable_root(project_roots: Vec<PathBuf>, root: &Path) -> Vec<PathBuf> {
    project_roots
        .into_iter()
        .filter(|project| !is_within(root, project))
        .collect()
}

#[cfg(windows)]
fn platform_data_directory() -> Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| {
            LocationError::new(
                LocationErrorKind::InvalidPath,
                Path::new("%LOCALAPPDATA%"),
                "La cartella dati locale di Windows non è disponibile.",
            )
        })
}

#[cfg(not(windows))]
fn platform_data_directory() -> Result<PathBuf> {
    Err(LocationError::new(
        LocationErrorKind::UnsupportedVolume,
        Path::new(""),
        "Questa versione supporta Windows.",
    ))
}

#[derive(Debug, Clone)]
pub struct StorageLocation {
    directory: PathBuf,
    request: LocationRequest,
    // Keep directory names stable while callers own this location (including SQLite handles).
    guards: Arc<Mutex<Vec<fs::File>>>,
}

impl StorageLocation {
    /// Fast storage-only rejection before any path metadata or saved-project lookup.
    pub fn preflight(path: &Path) -> Result<()> {
        validate_shape(path)?;
        #[cfg(windows)]
        return windows::preflight_drive(path);
        #[cfg(not(windows))]
        Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Questa versione supporta Windows.",
        ))
    }

    /// No filesystem mutation occurs here, including on rejected locations.
    pub fn resolve(request: LocationRequest) -> Result<Self> {
        let raw = request
            .home_override
            .as_ref()
            .cloned()
            .unwrap_or_else(|| request.local_data_directory.join("shinatama"));
        Self::preflight(&raw)?;
        if raw.parent().is_none() {
            return Err(LocationError::new(
                LocationErrorKind::InvalidPath,
                &raw,
                "Scegli una sottocartella dedicata a Shinatama.",
            ));
        }
        let (directory, ancestor) = canonical_future_path(&raw)?;
        if let Ok((platform_root, _)) = canonical_future_path(&request.local_data_directory)
            && is_within(&platform_root, &directory)
        {
            return Err(LocationError::new(
                LocationErrorKind::InvalidPath,
                &raw,
                "La cartella deve essere dedicata a Shinatama, non una cartella di sistema o del profilo.",
            ));
        }
        for project in &request.project_roots {
            if let Ok((root, _)) = canonical_project_future_path(project)
                && (is_within(&directory, &root) || is_within(&root, &directory))
            {
                return Err(LocationError::new(
                    LocationErrorKind::Project,
                    &raw,
                    "Il database non può essere salvato dentro un progetto.",
                ));
            }
        }
        // Also recognize an existing Git worktree when it was not in the recent list.
        if ancestor.ancestors().any(|part| part.join(".git").exists()) {
            return Err(LocationError::new(
                LocationErrorKind::Project,
                &raw,
                "La cartella appartiene a un progetto Git.",
            ));
        }
        for sync in &request.synchronized_roots {
            if let Ok((root, _)) = canonical_future_path(sync)
                && is_within(&directory, &root)
            {
                return Err(LocationError::new(
                    LocationErrorKind::Synchronized,
                    &raw,
                    "La cartella appartiene a un servizio di sincronizzazione.",
                ));
            }
        }
        #[cfg(windows)]
        windows::validate_volume(&ancestor)?;
        #[cfg(not(windows))]
        return Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            &raw,
            "Questa versione supporta Windows.",
        ));
        #[cfg(windows)]
        Ok(Self {
            directory,
            request,
            guards: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn database_path(&self) -> PathBuf {
        self.directory.join("shinatama.db")
    }

    /// Reuse the complete policy for a legacy application directory during migration.
    pub fn resolve_related_directory(&self, directory: &Path) -> Result<Self> {
        let mut request = self.request.clone();
        request.home_override = Some(directory.to_path_buf());
        Self::resolve(request)
    }

    /// Revalidates immediately before creation. Every new directory is private at creation.
    /// Existing storage is never deleted and no database is created by this operation.
    pub fn prepare_private_directory(&self) -> Result<()> {
        let current = Self::resolve(self.request.clone())?;
        if !same_path(&current.directory, &self.directory) {
            return Err(LocationError::new(
                LocationErrorKind::InvalidPath,
                &self.directory,
                "Il percorso è cambiato durante l'apertura.",
            ));
        }
        #[cfg(windows)]
        {
            let guards = windows::prepare_private_directory(&self.directory, || {
                let locked = Self::resolve(self.request.clone())?;
                if !same_path(&locked.directory, &self.directory) {
                    return Err(LocationError::new(
                        LocationErrorKind::InvalidPath,
                        &self.directory,
                        "Il percorso è cambiato durante l'apertura.",
                    ));
                }
                Ok(())
            })?;
            *self.guards.lock().map_err(|_| {
                LocationError::new(
                    LocationErrorKind::Permissions,
                    &self.directory,
                    "La protezione della cartella non è disponibile.",
                )
            })? = guards;
        }
        #[cfg(not(windows))]
        return Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            &self.directory,
            "Questa versione supporta Windows.",
        ));
        #[cfg(windows)]
        {
            let after = Self::resolve(self.request.clone())?;
            if !same_path(&after.directory, &self.directory) {
                return Err(LocationError::new(
                    LocationErrorKind::InvalidPath,
                    &self.directory,
                    "Il percorso è cambiato durante la creazione.",
                ));
            }
            self.verify_private_directory()
        }
    }

    pub fn verify_private_directory(&self) -> Result<()> {
        #[cfg(windows)]
        return windows::verify_private_directory(&self.directory);
        #[cfg(not(windows))]
        Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            &self.directory,
            "Questa versione supporta Windows.",
        ))
    }

    /// Apply owner-only permissions to an existing direct child, before SQLite opens it.
    /// Symlinks, other reparse points and multiply-linked files are never modified.
    pub fn secure_private_file(&self, path: &Path) -> Result<()> {
        self.validate_file(path)?;
        #[cfg(windows)]
        return windows::private_file(path, true);
        #[cfg(not(windows))]
        Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Questa versione supporta Windows.",
        ))
    }

    /// Secure an optional file without a separate existence check. SQLite may remove
    /// WAL/SHM files when its last connection closes, including during another open.
    /// Only an absent file is accepted; the private parent and all existing-file
    /// ownership, ACL, reparse-point and hard-link checks remain mandatory. A pending
    /// Windows deletion may deny opens briefly; persistent failures still propagate
    /// after at most 100 ms of retry waiting (filesystem calls are not preemptible).
    pub fn secure_private_file_if_present(&self, path: &Path) -> Result<bool> {
        self.validate_file(path)?;
        #[cfg(windows)]
        {
            use std::time::{Duration, Instant};
            use windows_sys::Win32::Foundation::{
                ERROR_ACCESS_DENIED, ERROR_DELETE_PENDING, ERROR_SHARING_VIOLATION,
            };
            let deadline = Instant::now() + Duration::from_millis(100);
            loop {
                match windows::private_file(path, true) {
                    Ok(()) => return Ok(true),
                    Err(error)
                        if error
                            .source
                            .as_ref()
                            .is_some_and(|source| source.kind() == io::ErrorKind::NotFound) =>
                    {
                        return Ok(false);
                    }
                    Err(error)
                        if Instant::now() < deadline
                            && error.source.as_ref().is_some_and(|source| {
                                matches!(
                                    source.raw_os_error().map(|code| code as u32),
                                    Some(
                                        ERROR_ACCESS_DENIED
                                            | ERROR_DELETE_PENDING
                                            | ERROR_SHARING_VIOLATION
                                    )
                                )
                            }) =>
                    {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        #[cfg(not(windows))]
        Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Questa versione supporta Windows.",
        ))
    }

    pub fn verify_private_file(&self, path: &Path) -> Result<()> {
        self.validate_file(path)?;
        #[cfg(windows)]
        return windows::private_file(path, false);
        #[cfg(not(windows))]
        Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Questa versione supporta Windows.",
        ))
    }

    /// A project may never contain the active shared-state directory, even when selected later.
    pub fn validate_project(&self, project: &Path) -> Result<()> {
        let (project, _) = canonical_project_future_path(project)?;
        if is_within(&self.directory, &project) || is_within(&project, &self.directory) {
            return Err(LocationError::new(
                LocationErrorKind::Project,
                &project,
                "Il progetto contiene la cartella dati di Shinatama, oppure ne fa parte. Scegli una cartella di progetto separata.",
            ));
        }
        Ok(())
    }

    fn validate_file(&self, path: &Path) -> Result<()> {
        self.verify_private_directory()?;
        validate_shape(path)?;
        let parent = path.parent().ok_or_else(|| {
            LocationError::new(
                LocationErrorKind::InvalidPath,
                path,
                "Il file deve appartenere alla cartella dati privata.",
            )
        })?;
        let parent = fs::canonicalize(parent).map_err(|error| LocationError::io(path, error))?;
        if !same_path(&parent, &self.directory) {
            return Err(LocationError::new(
                LocationErrorKind::InvalidPath,
                path,
                "Il file deve essere figlio diretto della cartella dati privata.",
            ));
        }
        Ok(())
    }
}

fn validate_shape(path: &Path) -> Result<()> {
    validate_path_shape(path, false)
}

pub(crate) fn validate_project_shape(path: &Path) -> Result<()> {
    validate_path_shape(path, true)
}

fn validate_path_shape(path: &Path, network_project: bool) -> Result<()> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(LocationError::new(
            LocationErrorKind::InvalidPath,
            path,
            "Serve un percorso assoluto senza componenti '..'.",
        ));
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        match path.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(_) | Prefix::VerbatimDisk(_) => {}
                Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _) if network_project => {}
                Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _) => {
                    return Err(LocationError::new(
                        LocationErrorKind::Network,
                        path,
                        "I percorsi di rete UNC non supportano il database locale.",
                    ));
                }
                _ => {
                    return Err(LocationError::new(
                        LocationErrorKind::InvalidPath,
                        path,
                        "I percorsi dispositivo non sono cartelle dati.",
                    ));
                }
            },
            _ => {
                return Err(LocationError::new(
                    LocationErrorKind::InvalidPath,
                    path,
                    "Serve un percorso locale con lettera di unità.",
                ));
            }
        }
        for component in path.components() {
            if let Component::Normal(name) = component {
                let name = name.to_string_lossy();
                let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
                if name.ends_with([' ', '.'])
                    || name.contains([':', '*', '?', '"', '<', '>', '|'])
                    || name.chars().any(|c| c == '\0' || c.is_control())
                    || matches!(
                        stem.as_str(),
                        "CON"
                            | "PRN"
                            | "AUX"
                            | "NUL"
                            | "COM1"
                            | "COM2"
                            | "COM3"
                            | "COM4"
                            | "COM5"
                            | "COM6"
                            | "COM7"
                            | "COM8"
                            | "COM9"
                            | "LPT1"
                            | "LPT2"
                            | "LPT3"
                            | "LPT4"
                            | "LPT5"
                            | "LPT6"
                            | "LPT7"
                            | "LPT8"
                            | "LPT9"
                    )
                {
                    return Err(LocationError::new(
                        LocationErrorKind::InvalidPath,
                        path,
                        "Il percorso contiene un nome non valido per Windows.",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn canonical_future_path(path: &Path) -> Result<(PathBuf, PathBuf)> {
    validate_shape(path)?;
    canonical_validated_future_path(path)
}

fn canonical_project_future_path(path: &Path) -> Result<(PathBuf, PathBuf)> {
    validate_project_shape(path)?;
    canonical_validated_future_path(path)
}

fn canonical_validated_future_path(path: &Path) -> Result<(PathBuf, PathBuf)> {
    let mut ancestor = path;
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                tail.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| LocationError::io(path, error))?
                        .to_os_string(),
                );
                ancestor = ancestor.parent().ok_or_else(|| {
                    LocationError::new(
                        LocationErrorKind::InvalidPath,
                        path,
                        "Nessuna cartella locale esistente.",
                    )
                })?;
            }
            Err(error) => return Err(LocationError::io(path, error)),
        }
    }
    let canonical = fs::canonicalize(ancestor).map_err(|error| LocationError::io(path, error))?;
    if !canonical.is_dir() {
        return Err(LocationError::new(
            LocationErrorKind::InvalidPath,
            path,
            "Una componente del percorso è un file.",
        ));
    }
    let mut full = canonical.clone();
    for component in tail.iter().rev() {
        full.push(component);
    }
    Ok((full, canonical))
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        windows::same_name(left.as_os_str(), right.as_os_str())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

pub(crate) fn is_within(path: &Path, root: &Path) -> bool {
    let mut path_parts = path.components();
    root.components().all(|part| {
        path_parts.next().is_some_and(|other| {
            same_path(Path::new(part.as_os_str()), Path::new(other.as_os_str()))
        })
    })
}

/// Owner-only folders and files at an explicit location the caller is allowed to use outside
/// the application directory, such as `<git common dir>/shinatama/handoffs/<hop-id>`
/// (FRD-0005 R24). The Git-project refusal of [`StorageLocation`] does not apply: these hold
/// one hand-off, never shared state. Reparse points and multiply-linked files are refused.
pub mod external {
    use super::*;

    /// Creates every missing folder with a current-user-only ACL and verifies `path`. The
    /// returned handles deny delete-sharing on the chain while the caller writes.
    pub fn prepare_private_directory(path: &Path) -> Result<Vec<fs::File>> {
        if !path.is_absolute() {
            return Err(LocationError::new(
                LocationErrorKind::InvalidPath,
                path,
                "Il percorso deve essere assoluto.",
            ));
        }
        #[cfg(windows)]
        return windows::prepare_private_directory(path, || Ok(()));
        #[cfg(not(windows))]
        Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Questa versione supporta Windows.",
        ))
    }

    /// Applies owner-only permissions to a file just created inside such a folder.
    pub fn secure_private_file(path: &Path) -> Result<()> {
        #[cfg(windows)]
        return windows::private_file(path, true);
        #[cfg(not(windows))]
        Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Questa versione supporta Windows.",
        ))
    }

    pub fn verify_private_file(path: &Path) -> Result<()> {
        #[cfg(windows)]
        return windows::private_file(path, false);
        #[cfg(not(windows))]
        Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Questa versione supporta Windows.",
        ))
    }
}

#[cfg(test)]
mod portable_tests {
    use super::*;

    #[test]
    fn the_portable_folder_and_its_ancestors_are_not_projects() {
        let root = PathBuf::from(r"D:\Software\Shinatama");
        let roots = vec![
            PathBuf::from(r"d:\software\shinatama"),
            PathBuf::from(r"D:\Software"),
            PathBuf::from(r"D:\Software\Shinatama\cli"),
            PathBuf::from(r"D:\Projects\demo"),
        ];
        assert_eq!(
            without_portable_root(roots, &root),
            vec![PathBuf::from(r"D:\Software\Shinatama\cli"), PathBuf::from(r"D:\Projects\demo")]
        );
    }
}
