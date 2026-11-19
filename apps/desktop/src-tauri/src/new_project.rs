//! One explicit project creation, using the shared folder-operation reservation.
use crate::{Services, projects::ProjectCatalog};
use serde::{Deserialize, Serialize};
use shinatama_projects::create::{
    self, Cancellation, CommitControl, CommitGuard, CreateError, CreateErrorCode, CreationGrant,
};
use shinatama_projects::git_init::ContainedGit;
use shinatama_state::cancellation::MutationToken;
use shinatama_state::database::{Database, ProjectRecord};
use shinatama_state::projects::{ProjectRoot, RootsConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{State, WebviewWindow};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationOptions {
    revision: u64,
    roots: Vec<ProjectRoot>,
    default_root_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreationRequest {
    expected_revision: u64,
    root_id: String,
    name: String,
    git_init: bool,
}

struct Control<'a>(&'a MutationToken);
impl Cancellation for Control<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}
impl CommitControl for Control<'_> {
    fn begin_commit(&self) -> create::Result<Box<dyn CommitGuard + '_>> {
        self.0
            .begin_commit()
            .map(|permit| Box::new(permit) as Box<dyn CommitGuard>)
            .map_err(|_| {
                if self.0.is_cancelled() {
                    CreateError::cancelled()
                } else {
                    CreateError::new(CreateErrorCode::Busy, "Il salvataggio è già in corso.")
                }
            })
    }
}

struct AdmissionControl<'a> {
    cancellation: Control<'a>,
    store: &'a Database,
    request: &'a CreationRequest,
    root: &'a Path,
}
impl AdmissionControl<'_> {
    fn validate(&self) -> create::Result<()> {
        self.store
            .check_write_admission()
            .map_err(|error| CreateError::new(CreateErrorCode::Unsupported, error.to_string()))?;
        let current = self
            .store
            .project_manager_read()
            .map_err(|error| CreateError::new(CreateErrorCode::Io, error.to_string()))?;
        if selected_root(&current, self.request)? != self.root {
            return Err(CreateError::new(
                CreateErrorCode::InvalidRoot,
                "La destinazione è cambiata. Ricarica le radici prima di creare il progetto.",
            ));
        }
        Ok(())
    }
}
impl Cancellation for AdmissionControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}
impl CommitControl for AdmissionControl<'_> {
    fn begin_commit(&self) -> create::Result<Box<dyn CommitGuard + '_>> {
        // Recheck after optional Git/filesystem work, immediately before the
        // atomic cancel/rename boundary. No SQL transaction spans native I/O.
        self.validate()?;
        self.cancellation.begin_commit()
    }
}

#[tauri::command]
pub async fn new_project_options(
    window: WebviewWindow,
    services: State<'_, Services>,
) -> Result<CreationOptions, String> {
    crate::local(&window)?;
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let config = services
            .load()?
            .store
            .project_manager_read()
            .map_err(|error| error.to_string())?;
        Ok(CreationOptions {
            revision: config.revision,
            roots: config.roots,
            default_root_id: config.default_root_id,
        })
    })
    .await
    .map_err(|_| "Impossibile leggere le radici dei progetti.".to_owned())?
}

fn selected_root(config: &RootsConfig, request: &CreationRequest) -> create::Result<PathBuf> {
    if config.revision != request.expected_revision {
        return Err(CreateError::new(
            CreateErrorCode::InvalidRoot,
            "Le radici sono cambiate. Ricaricale e verifica la destinazione prima di creare il progetto.",
        ));
    }
    config
        .roots
        .iter()
        .find(|root| root.id == request.root_id)
        .map(|root| PathBuf::from(&root.path))
        .ok_or_else(|| {
            CreateError::new(
                CreateErrorCode::InvalidRoot,
                "Scegli esplicitamente una radice configurata; nessuna cartella alternativa è stata usata.",
            )
        })
}

#[tauri::command]
pub async fn new_project_create(
    window: WebviewWindow,
    services: State<'_, Services>,
    catalog: State<'_, Arc<ProjectCatalog>>,
    request: CreationRequest,
    request_id: String,
) -> create::Result<ProjectRecord> {
    crate::local(&window).map_err(|error| CreateError::new(CreateErrorCode::Unsupported, error))?;
    let catalog = catalog.inner().clone();
    let reservation = catalog
        .reserve(Some(request_id))
        .map_err(|error| CreateError::new(CreateErrorCode::Busy, error))?;
    create::validate_name(&request.name)?;
    if request.root_id.is_empty() || request.root_id.len() > 64 {
        return Err(CreateError::new(
            CreateErrorCode::InvalidRoot,
            "Identificatore della radice non valido.",
        ));
    }
    let services = services.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let control = Control(reservation.token());
        if control.is_cancelled() {
            return Err(CreateError::cancelled());
        }
        let state = services
            .load()
            .map_err(|error| CreateError::new(CreateErrorCode::Io, error))?;
        state.store.check_write_admission().map_err(|error| {
            CreateError::new(CreateErrorCode::Unsupported, error.to_string())
        })?;
        let config = state
            .store
            .project_manager_read()
            .map_err(|error| CreateError::new(CreateErrorCode::Io, error.to_string()))?;
        let root = selected_root(&config, &request)?;
        // Git discovery is opt-in and occurs before any staging directory exists.
        let git = if request.git_init {
            Some(ContainedGit::new(find_git()?).map_err(|error| {
                CreateError::new(CreateErrorCode::GitUnavailable, error.to_string())
            })?)
        } else {
            None
        };
        if control.is_cancelled() {
            return Err(CreateError::cancelled());
        }
        let admission = AdmissionControl {
            cancellation: control,
            store: &state.store,
            request: &request,
            root: &root,
        };
        // The revision could change during executable discovery. Check again
        // before preparing the grant or creating any coordination/staging file.
        admission.validate()?;
        let grant = CreationGrant::prepare(
            &root,
            &[
                state.location.directory().to_path_buf(),
                state._legacy.directory().to_path_buf(),
            ],
        )?;
        let created = create::create_project(
            &grant,
            &request.name,
            git.as_ref().map(|git| git as &dyn create::GitInitializer),
            &admission,
        )?;
        // The no-overwrite rename has committed. Its token is done, so Cancel
        // cannot promise an undo while the separate metadata transaction finishes.
        let selected = state
            .store
            .project_manager_select(&created.path)
            .map_err(|error| {
                let mut failure = CreateError::new(
                    CreateErrorCode::RecoveryRequired,
                    format!("Il progetto è stato creato, ma non è stato possibile salvarne la selezione: {error}"),
                );
                failure.recovery_path = Some(created.path.clone());
                failure
            })?;
        if let Ok(config) = state.store.project_manager_read() {
            catalog.engine.configure(config);
        }
        Ok(selected)
    })
    .await
    .map_err(|_| {
        CreateError::new(
            CreateErrorCode::Io,
            "La creazione si è interrotta. Controlla la radice scelta prima di riprovare; nessuna operazione sarà ripetuta automaticamente.",
        )
    })?
}

fn find_git() -> create::Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .take(32)
                .map(|directory| directory.join("git.exe"))
                .collect()
        })
        .unwrap_or_default();
    for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(variable) {
            let suffix = if variable == "LOCALAPPDATA" {
                "Programs/Git/cmd/git.exe"
            } else {
                "Git/cmd/git.exe"
            };
            candidates.push(PathBuf::from(base).join(suffix));
        }
    }
    let started = std::time::Instant::now();
    for candidate in candidates {
        if started.elapsed() > std::time::Duration::from_secs(2) {
            break;
        }
        if local_native_file(&candidate) {
            return Ok(candidate);
        }
    }
    Err(CreateError::new(
        CreateErrorCode::GitUnavailable,
        "Git nativo non trovato nei percorsi locali ammessi. Installa Git oppure crea il progetto senza inizializzazione Git.",
    ))
}

#[cfg(windows)]
fn local_native_file(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    let raw = path.to_string_lossy();
    let raw = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    let bytes = raw.as_bytes();
    if bytes.len() < 3
        || bytes.len() > 4096
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || raw.chars().any(char::is_control)
        || bytes.iter().skip(2).any(|byte| *byte == b':')
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return false;
    }
    let root = [u16::from(bytes[0]), u16::from(b':'), u16::from(b'\\'), 0];
    // Reject mapped network/removable roots before any file metadata lookup.
    if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 {
        return false;
    }
    let ancestors: Vec<_> = path.ancestors().take(65).collect();
    if ancestors.len() > 64 {
        return false;
    }
    for ancestor in ancestors.into_iter().rev() {
        let Ok(metadata) = std::fs::symlink_metadata(ancestor) else {
            return false;
        };
        if metadata.file_attributes() & 0x400 != 0
            || if ancestor == path {
                !metadata.is_file()
            } else {
                !metadata.is_dir()
            }
        {
            return false;
        }
    }
    true
}

#[cfg(not(windows))]
fn local_native_file(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(revision: u64, root_id: &str) -> CreationRequest {
        CreationRequest {
            expected_revision: revision,
            root_id: root_id.into(),
            name: "project".into(),
            git_init: false,
        }
    }

    #[test]
    fn stale_root_revision_cannot_redirect_creation() {
        let config = RootsConfig {
            revision: 2,
            roots: vec![ProjectRoot {
                id: "r_one".into(),
                path: r"C:\changed".into(),
            }],
            ..RootsConfig::default()
        };
        assert!(selected_root(&config, &request(1, "r_one")).is_err());
        assert!(selected_root(&config, &request(2, "removed")).is_err());
        assert_eq!(
            selected_root(&config, &request(2, "r_one")).unwrap(),
            PathBuf::from(r"C:\changed")
        );
    }

    #[test]
    fn creation_control_has_the_same_atomic_cancel_boundary_as_the_picker() {
        let token = MutationToken::new();
        assert!(token.cancel());
        assert!(Control(&token).begin_commit().is_err());
        let token = MutationToken::new();
        let control = Control(&token);
        let permit = control.begin_commit().unwrap();
        assert!(!token.cancel());
        drop(permit);
        assert!(!token.cancel());
    }

    #[test]
    #[cfg(windows)]
    fn roots_changed_during_preparation_refuse_the_publication_permit() {
        use shinatama_state::location::{LocationRequest, StorageLocation};
        use shinatama_state::projects::{RootInput, RootsSetRequest};
        use std::{
            fs,
            time::{SystemTime, UNIX_EPOCH},
        };
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                if self.0.parent() == Some(std::env::temp_dir().as_path())
                    && self.0.file_name().is_some_and(|name| {
                        name.to_string_lossy()
                            .starts_with("shinatama-c05-admission-")
                    })
                {
                    let _ = fs::remove_dir_all(&self.0);
                }
            }
        }
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "shinatama-c05-admission-{}-{}",
            std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        )));
        fs::create_dir(&fixture.0).unwrap();
        let project_root = fixture.0.join("roots");
        fs::create_dir(&project_root).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: fixture.0.clone(),
            home_override: Some(fixture.0.join("data")),
            project_roots: Vec::new(),
            synchronized_roots: Vec::new(),
        })
        .unwrap();
        let database = Database::open(&location, None).unwrap();
        let config = database
            .project_roots_set(RootsSetRequest {
                expected_revision: 0,
                roots: vec![RootInput {
                    id: None,
                    path: project_root.to_string_lossy().into_owned(),
                }],
                default_root_index: Some(0),
            })
            .unwrap();
        let request = request(config.revision, &config.roots[0].id);
        let root = selected_root(&config, &request).unwrap();
        let token = MutationToken::new();
        let admission = AdmissionControl {
            cancellation: Control(&token),
            store: &database,
            request: &request,
            root: &root,
        };
        admission.validate().unwrap();
        // Another window commits a new configuration after preparation began.
        database
            .project_roots_set(RootsSetRequest {
                expected_revision: config.revision,
                roots: config
                    .roots
                    .into_iter()
                    .map(|root| RootInput {
                        id: Some(root.id),
                        path: root.path,
                    })
                    .collect(),
                default_root_index: Some(0),
            })
            .unwrap();
        assert!(admission.begin_commit().is_err());
        assert!(token.cancel()); // No point of no return was crossed.
        assert!(fs::read_dir(project_root).unwrap().next().is_none());
    }

    #[test]
    fn executable_discovery_rejects_device_relative_and_network_paths() {
        for path in [
            r"git.exe",
            r"\\offline\share\git.exe",
            r"C:\git.exe:stream",
            r"C:\tools\..\git.exe",
        ] {
            assert!(!local_native_file(Path::new(path)));
        }
    }
}
