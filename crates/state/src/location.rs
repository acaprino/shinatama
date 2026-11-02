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
}

impl LocationError {
    fn new(kind: LocationErrorKind, path: &Path, detail: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    fn io(path: &Path, error: io::Error) -> Self {
        Self::new(LocationErrorKind::Io, path, error.to_string())
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

impl std::error::Error for LocationError {}

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
        let home_override = std::env::var_os("SHINATAMA_HOME").map(PathBuf::from);
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
            if let Ok((root, _)) = canonical_future_path(project)
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
        let (project, _) = canonical_future_path(project)?;
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

fn is_within(path: &Path, root: &Path) -> bool {
    let mut path_parts = path.components();
    root.components().all(|part| {
        path_parts.next().is_some_and(|other| {
            same_path(Path::new(part.as_os_str()), Path::new(other.as_os_str()))
        })
    })
}
