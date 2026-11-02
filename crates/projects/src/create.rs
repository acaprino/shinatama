//! Project creation stages work beside its final name, then commits one no-overwrite rename.
//! A failed/cancelled operation can remove only its own still-empty directory, by handle.
//! Nonempty staging directories are retained with an explicit recovery path.

use serde::Serialize;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const STAGING_PREFIX: &str = ".shinatama-create-";
const CREATION_LOCK_NAME: &str = ".shinatama-create.lock";
const MAX_ROOT_ENTRIES: usize = 4000;
const MAX_RETAINED_STAGES: usize = 8;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreateErrorCode {
    InvalidName,
    InvalidRoot,
    ProtectedData,
    AlreadyExists,
    Cancelled,
    RecoveryRequired,
    GitUnavailable,
    GitFailed,
    Busy,
    Io,
    Unsupported,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateError {
    pub code: CreateErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_path: Option<PathBuf>,
    #[serde(skip)]
    source: Option<io::Error>,
}

impl CreateError {
    pub fn new(code: CreateErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            recovery_path: None,
            source: None,
        }
    }

    pub fn cancelled() -> Self {
        Self::new(
            CreateErrorCode::Cancelled,
            "Creazione annullata prima del salvataggio del progetto.",
        )
    }

    fn io(path: &Path, error: io::Error) -> Self {
        let mut result = Self::new(
            CreateErrorCode::Io,
            format!(
                "Impossibile creare il progetto in {}: {error}",
                path.display()
            ),
        );
        result.source = Some(error);
        result
    }
}

impl fmt::Display for CreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        if let Some(path) = &self.recovery_path {
            write!(
                f,
                " I file di preparazione sono conservati in {}. Controllali prima di rimuoverli o riprovare.",
                path.display()
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for CreateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|error| error as _)
    }
}

pub type Result<T> = std::result::Result<T, CreateError>;

pub trait Cancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

/// Adapters wrap their commit permit in this marker; dropping it closes the mutation.
pub trait CommitGuard {}
impl<T> CommitGuard for T {}

pub trait CommitControl: Cancellation {
    /// Must linearize against cancellation. An accepted cancellation can never
    /// produce a permit; once granted, cancellation must not promise an undo.
    fn begin_commit(&self) -> Result<Box<dyn CommitGuard + '_>>;
}

#[derive(Debug)]
pub enum GitInitError {
    Cancelled,
    Unavailable(String),
    Busy,
    Failed(String),
    TimedOut,
    OutputLimit,
    Containment(String),
}

impl fmt::Display for GitInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Inizializzazione Git annullata."),
            Self::Unavailable(message) => write!(f, "Git non disponibile: {message}"),
            Self::Busy => f.write_str("La precedente operazione Git non è ancora terminata."),
            Self::Failed(message) => write!(f, "Inizializzazione Git non riuscita: {message}"),
            Self::TimedOut => f.write_str("Git ha superato il tempo disponibile."),
            Self::OutputLimit => f.write_str("Git ha superato il limite di output."),
            Self::Containment(message) => write!(
                f,
                "Impossibile avviare Git con le protezioni richieste: {message}"
            ),
        }
    }
}

impl std::error::Error for GitInitError {}

pub trait GitInitializer: Send + Sync {
    /// The existing empty directory belongs to this operation. Implementations
    /// must contain their subprocess before it runs and use finite resource limits.
    fn initialize(
        &self,
        directory: &Path,
        cancellation: &dyn Cancellation,
    ) -> std::result::Result<(), GitInitError>;
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedProject {
    pub path: PathBuf,
    pub name: String,
    pub git_initialized: bool,
}

/// Explicit authorization for one existing root. Its canonical ancestor names stay
/// pinned during preparation and commit; this type never creates missing parents.
#[derive(Debug)]
pub struct CreationGrant {
    root: PathBuf,
    protected_data: Vec<PathBuf>,
    #[cfg(windows)]
    _guards: Vec<fs::File>,
}

impl CreationGrant {
    pub fn prepare(root: &Path, protected_data_directories: &[PathBuf]) -> Result<Self> {
        validate_root_shape(root)?;
        let root = fs::canonicalize(root).map_err(|error| CreateError::io(root, error))?;
        if !root.is_dir() {
            return Err(CreateError::new(
                CreateErrorCode::InvalidRoot,
                "La radice scelta deve essere una cartella esistente.",
            ));
        }
        let mut protected_data = Vec::with_capacity(protected_data_directories.len());
        for directory in protected_data_directories {
            validate_root_shape(directory)?;
            protected_data.push(
                fs::canonicalize(directory).map_err(|error| CreateError::io(directory, error))?,
            );
        }
        #[cfg(windows)]
        {
            let mut guards = Vec::new();
            for ancestor in root.ancestors() {
                guards.push(native::open_directory(ancestor, false)?);
            }
            native::verify_named_directory(&root, &guards[0])?;
            let data_guards = native::validate_protected_roots(&root, &guards, &protected_data)?;
            guards.extend(data_guards);
            Ok(Self {
                root,
                protected_data,
                _guards: guards,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (root, protected_data);
            Err(CreateError::new(
                CreateErrorCode::Unsupported,
                "La creazione protetta è disponibile su Windows.",
            ))
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn validate_destination(&self, destination: &Path) -> Result<()> {
        if self
            .protected_data
            .iter()
            .any(|data| is_within(destination, data) || is_within(data, destination))
        {
            return Err(CreateError::new(
                CreateErrorCode::ProtectedData,
                "Il progetto deve essere separato dalla cartella dati di Shinatama.",
            ));
        }
        Ok(())
    }
}

/// Validate a single Windows directory component; no trimming or path expansion.
pub fn validate_name(name: &str) -> Result<()> {
    // Reject oversized input before Unicode case conversion or any name-sized
    // allocation. A Windows component has at most 255 UTF-16 units; 1020 UTF-8
    // bytes is a conservative outer bound even for supplementary characters.
    if name.len() > 1020 || name.encode_utf16().take(256).count() > 255 {
        return Err(CreateError::new(
            CreateErrorCode::InvalidName,
            "Il nome supera il limite di 255 caratteri Windows.",
        ));
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
        || name.to_ascii_lowercase().starts_with(STAGING_PREFIX)
        || name.eq_ignore_ascii_case(CREATION_LOCK_NAME)
        || reserved
    {
        return Err(CreateError::new(
            CreateErrorCode::InvalidName,
            "Usa un nome di cartella valido: senza percorsi, nomi riservati, punti o spazi finali, entro 255 caratteri Windows.",
        ));
    }
    Ok(())
}

fn validate_root_shape(root: &Path) -> Result<()> {
    if !root.is_absolute()
        || root
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(CreateError::new(
            CreateErrorCode::InvalidRoot,
            "Scegli una radice esistente con percorso assoluto, senza '..'.",
        ));
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        if !matches!(root.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_) | Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _)))
        {
            return Err(CreateError::new(
                CreateErrorCode::InvalidRoot,
                "La radice deve essere una cartella locale o di rete; i percorsi dispositivo non sono ammessi.",
            ));
        }
    }
    Ok(())
}

fn is_within(path: &Path, root: &Path) -> bool {
    let mut components = path.components();
    root.components().all(|part| {
        components
            .next()
            .is_some_and(|other| same_name(part.as_os_str(), other.as_os_str()))
    })
}

fn same_name(left: &OsStr, right: &OsStr) -> bool {
    #[cfg(windows)]
    return native::same_name(left, right);
    #[cfg(not(windows))]
    return left == right;
}

fn check_cancel(control: &dyn Cancellation) -> Result<()> {
    if control.is_cancelled() {
        Err(CreateError::cancelled())
    } else {
        Ok(())
    }
}

fn inspect_root(
    grant: &CreationGrant,
    name: &str,
    control: &dyn Cancellation,
    own_stage: Option<&OsStr>,
) -> Result<()> {
    let mut stages = 0usize;
    let entries = fs::read_dir(&grant.root).map_err(|error| CreateError::io(&grant.root, error))?;
    for (index, entry) in entries.enumerate() {
        check_cancel(control)?;
        if index >= MAX_ROOT_ENTRIES {
            return Err(CreateError::new(
                CreateErrorCode::Busy,
                "La radice supera 4000 voci: scegli una radice più piccola per verificare i duplicati in modo sicuro.",
            ));
        }
        let entry = entry.map_err(|error| CreateError::io(&grant.root, error))?;
        let candidate = entry.file_name();
        if same_name(&candidate, OsStr::new(name)) {
            return Err(CreateError::new(
                CreateErrorCode::AlreadyExists,
                "Esiste già un file o una cartella con questo nome, anche con maiuscole diverse.",
            ));
        }
        if own_stage != Some(candidate.as_os_str())
            && candidate
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with(STAGING_PREFIX)
        {
            stages += 1;
            if stages >= MAX_RETAINED_STAGES {
                return Err(CreateError::new(
                    CreateErrorCode::RecoveryRequired,
                    "Sono presenti almeno 8 preparazioni precedenti. Controlla i file conservati nella radice prima di crearne altre.",
                ));
            }
        }
    }
    Ok(())
}

/// Filesystem work belongs on the caller's bounded blocking worker. Cancellation
/// before the commit permit leaves the final name absent. A successful rename is
/// the commit: no subsequent fallible work can turn success into a reported failure.
pub fn create_project(
    grant: &CreationGrant,
    name: &str,
    git: Option<&dyn GitInitializer>,
    control: &dyn CommitControl,
) -> Result<CreatedProject> {
    validate_name(name)?;
    check_cancel(control)?;
    let destination = grant.root.join(name);
    grant.validate_destination(&destination)?;
    inspect_root(grant, name, control, None)?;
    check_cancel(control)?;
    #[cfg(windows)]
    {
        let _root_lock = native::acquire_creation_lock(grant, control)?;
        inspect_root(grant, name, control, None)?;
        let stage_path = grant
            .root
            .join(format!("{STAGING_PREFIX}{}.tmp", uuid::Uuid::new_v4()));
        let mut stage = native::Stage::create(grant, &stage_path)?;
        let prepared: Result<CreatedProject> = (|| {
            if let Some(initializer) = git {
                initializer
                    .initialize(stage.path(), control)
                    .map_err(|error| {
                        let code = match &error {
                            GitInitError::Cancelled => CreateErrorCode::Cancelled,
                            GitInitError::Unavailable(_) => CreateErrorCode::GitUnavailable,
                            GitInitError::Busy => CreateErrorCode::Busy,
                            _ => CreateErrorCode::GitFailed,
                        };
                        CreateError::new(code, error.to_string())
                    })?;
            }
            check_cancel(control)?;
            // This pass also rejects aliases created by another program while Git ran.
            inspect_root(grant, name, control, stage.path().file_name())?;
            stage.prepare_rename()?;
            check_cancel(control)?;
            let _permit = control.begin_commit()?;
            stage.commit(grant, name)?;
            Ok(CreatedProject {
                path: destination,
                name: name.to_string(),
                git_initialized: git.is_some(),
            })
        })();
        match prepared {
            Ok(project) => Ok(project),
            Err(mut error) => {
                if !stage.remove_if_empty() {
                    error.recovery_path = Some(stage.path().to_owned());
                }
                Err(error)
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = git;
        Err(CreateError::new(
            CreateErrorCode::Unsupported,
            "La creazione protetta è disponibile su Windows.",
        ))
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::fs::{File, OpenOptions, TryLockError};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::ptr::null_mut;
    use std::time::{Duration, Instant};
    use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_CREATE, FILE_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
        NtCreateFile,
    };
    use windows_sys::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, INVALID_HANDLE_VALUE, RtlNtStatusToDosError,
        UNICODE_STRING,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_RENAME_INFO, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileDispositionInfo, FileRenameInfo, GetFileInformationByHandle,
        SetFileInformationByHandle,
    };
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    const DELETE_ACCESS: u32 = 0x0001_0000;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CompareStringOrdinal(
            left: *const u16,
            left_len: i32,
            right: *const u16,
            right_len: i32,
            ignore_case: i32,
        ) -> i32;
    }

    fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }

    pub(super) fn same_name(left: &OsStr, right: &OsStr) -> bool {
        let left: Vec<u16> = left.encode_wide().collect();
        let right: Vec<u16> = right.encode_wide().collect();
        left.len() <= i32::MAX as usize
            && right.len() <= i32::MAX as usize
            && unsafe {
                CompareStringOrdinal(
                    left.as_ptr(),
                    left.len() as i32,
                    right.as_ptr(),
                    right.len() as i32,
                    1,
                )
            } == 2
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Identity {
        volume: u32,
        high: u32,
        low: u32,
    }

    impl Identity {
        fn is_verifiable(self) -> bool {
            self.volume != 0 && (self.high != 0 || self.low != 0)
        }
    }

    fn inspect(path: &Path, file: &File, directory: bool) -> Result<Identity> {
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
            return Err(CreateError::io(path, io::Error::last_os_error()));
        }
        if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
            || (!directory
                && (information.nNumberOfLinks != 1
                    || information.nFileSizeHigh != 0
                    || information.nFileSizeLow != 0))
        {
            return Err(CreateError::new(
                CreateErrorCode::InvalidRoot,
                "Il percorso è cambiato oppure usa un collegamento non ammesso. Nessun file esistente è stato modificato.",
            ));
        }
        let identity = Identity {
            volume: information.dwVolumeSerialNumber,
            high: information.nFileIndexHigh,
            low: information.nFileIndexLow,
        };
        if !identity.is_verifiable() {
            return Err(CreateError::new(
                CreateErrorCode::InvalidRoot,
                "Il filesystem non fornisce un'identità verificabile per questa cartella. Scegli una radice che permetta di controllare la separazione dai dati di Shinatama.",
            ));
        }
        Ok(identity)
    }

    pub(super) fn open_directory(path: &Path, for_rename: bool) -> Result<File> {
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES | if for_rename { DELETE_ACCESS } else { 0 })
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .map_err(|error| CreateError::io(path, error))?;
        inspect(path, &file, true)?;
        Ok(file)
    }

    pub(super) fn verify_named_directory(path: &Path, pinned: &File) -> Result<()> {
        let current = open_directory(path, false)?;
        if inspect(path, &current, true)? != inspect(path, pinned, true)? {
            return Err(CreateError::new(
                CreateErrorCode::InvalidRoot,
                "La radice è cambiata durante la verifica. Sceglila di nuovo.",
            ));
        }
        Ok(())
    }

    fn reject_protected_identities(
        root: &Path,
        root_identities: &[Identity],
        protected: &[(PathBuf, Identity)],
    ) -> Result<()> {
        if let Some((data, _)) = protected
            .iter()
            .find(|(_, data_identity)| root_identities.contains(data_identity))
        {
            return Err(CreateError::new(
                CreateErrorCode::ProtectedData,
                format!(
                    "La radice {} coincide con la cartella dati {} oppure ne fa parte, anche tramite un alias. Scegli una radice separata.",
                    root.display(),
                    data.display()
                ),
            ));
        }
        Ok(())
    }

    pub(super) fn validate_protected_roots(
        root: &Path,
        root_guards: &[File],
        protected_data: &[PathBuf],
    ) -> Result<Vec<File>> {
        let root_identities = root_guards
            .iter()
            .map(|guard| inspect(root, guard, true))
            .collect::<Result<Vec<_>>>()?;
        let mut guards = Vec::new();
        let mut protected = Vec::with_capacity(protected_data.len());
        for data in protected_data {
            let first = guards.len();
            for ancestor in data.ancestors() {
                let guard = open_directory(ancestor, false)?;
                if ancestor == data {
                    protected.push((data.clone(), inspect(data, &guard, true)?));
                }
                guards.push(guard);
            }
            verify_named_directory(data, &guards[first])?;
        }
        // Windows identities are independent of drive-letter/UNC spelling. Only
        // the protected directory's identity is compared: shared ancestors such
        // as C:\ do not prohibit a separate sibling project root.
        reject_protected_identities(root, &root_identities, &protected)?;
        Ok(guards)
    }

    pub(super) fn acquire_creation_lock(
        grant: &CreationGrant,
        control: &dyn Cancellation,
    ) -> Result<File> {
        check_cancel(control)?;
        let path = grant.root.join(CREATION_LOCK_NAME);
        let created = unsafe {
            CreateFileW(
                wide(&path).as_ptr(),
                0x8000_0000 | 0x4000_0000 | 0x0002_0000,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                CREATE_NEW,
                FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        };
        let file = if created != INVALID_HANDLE_VALUE {
            unsafe { File::from_raw_handle(created) }
        } else {
            let error = io::Error::last_os_error();
            if !matches!(
                error.raw_os_error().map(|code| code as u32),
                Some(ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS)
            ) {
                return Err(CreateError::io(&path, error));
            }
            OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path)
                .map_err(|error| CreateError::io(&path, error))?
        };
        inspect(&path, &file, false)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            check_cancel(control)?;
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(TryLockError::WouldBlock) => {
                    return Err(CreateError::new(
                        CreateErrorCode::Busy,
                        "Un'altra creazione è ancora in corso in questa radice. Riprova quando è terminata.",
                    ));
                }
                Err(TryLockError::Error(error)) => return Err(CreateError::io(&path, error)),
            }
        }
    }

    pub(super) struct Stage {
        path: PathBuf,
        file: Option<File>,
        identity: Identity,
        rename_ready: bool,
        committed: bool,
    }

    impl Stage {
        pub(super) fn create(grant: &CreationGrant, path: &Path) -> Result<Self> {
            let mut name: Vec<u16> = path
                .file_name()
                .expect("generated stage name")
                .encode_wide()
                .collect();
            let name = UNICODE_STRING {
                Length: (name.len() * 2) as u16,
                MaximumLength: (name.len() * 2) as u16,
                Buffer: name.as_mut_ptr(),
            };
            let attributes = OBJECT_ATTRIBUTES {
                Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
                RootDirectory: grant._guards[0].as_raw_handle(),
                ObjectName: &name,
                Attributes: 0x40, // OBJ_CASE_INSENSITIVE
                SecurityDescriptor: std::ptr::null(),
                SecurityQualityOfService: std::ptr::null(),
            };
            let mut handle = null_mut();
            let mut status_block: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
            // FILE_CREATE never opens/replaces an existing directory. The returned
            // no-delete-sharing handle pins the created identity in the same call.
            // Permissions explicitly follow the chosen root, including SMB/cloud
            // identities whose access model differs from a local Windows SID.
            let status = unsafe {
                NtCreateFile(
                    &mut handle,
                    FILE_READ_ATTRIBUTES | 0x0010_0000,
                    &attributes,
                    &mut status_block,
                    std::ptr::null(),
                    FILE_ATTRIBUTE_DIRECTORY,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    FILE_CREATE,
                    FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
                    std::ptr::null(),
                    0,
                )
            };
            if status < 0 {
                return Err(CreateError::io(
                    path,
                    io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(status) } as i32),
                ));
            }
            let file = unsafe { File::from_raw_handle(handle) };
            let identity = inspect(path, &file, true).map_err(|mut error| {
                error.recovery_path = Some(path.to_path_buf());
                error
            })?;
            Ok(Self {
                path: path.to_path_buf(),
                file: Some(file),
                identity,
                rename_ready: false,
                committed: false,
            })
        }

        pub(super) fn path(&self) -> &Path {
            &self.path
        }

        pub(super) fn prepare_rename(&mut self) -> Result<()> {
            if !self.rename_ready {
                // A DELETE-access handle would prevent Git from opening its cwd.
                // Reopen only after Git exits; no-follow + identity reject swaps.
                self.file.take();
                let file = open_directory(&self.path, true)?;
                if inspect(&self.path, &file, true)? != self.identity {
                    return Err(CreateError::new(
                        CreateErrorCode::InvalidRoot,
                        "La cartella di preparazione è cambiata. Il contenuto è stato conservato.",
                    ));
                }
                self.file = Some(file);
                self.rename_ready = true;
            }
            Ok(())
        }

        pub(super) fn commit(&mut self, grant: &CreationGrant, name: &str) -> Result<()> {
            // Win32 rejects relative FILE_RENAME_INFO on some supported Windows
            // builds. Every canonical destination ancestor is already pinned, so
            // an absolute target preserves the same namespace/identity guarantee.
            let name = wide(grant.root.join(name));
            let header = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
            let bytes = (header + name.len() * 2).max(size_of::<FILE_RENAME_INFO>());
            let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
            let information = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
            unsafe {
                (*information).Anonymous.ReplaceIfExists = false;
                (*information).RootDirectory = null_mut();
                (*information).FileNameLength = ((name.len() - 1) * 2) as u32;
                std::ptr::copy_nonoverlapping(
                    name.as_ptr(),
                    std::ptr::addr_of_mut!((*information).FileName).cast::<u16>(),
                    name.len(),
                );
            }
            if unsafe {
                SetFileInformationByHandle(
                    self.file.as_ref().expect("prepared stage").as_raw_handle(),
                    FileRenameInfo,
                    information.cast(),
                    bytes as u32,
                )
            } == 0
            {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::AlreadyExists {
                    return Err(CreateError::new(
                        CreateErrorCode::AlreadyExists,
                        "Un altro progetto ha già occupato questo nome. Nessun file è stato sovrascritto.",
                    ));
                }
                return Err(CreateError::io(&self.path, error));
            }
            self.committed = true;
            Ok(())
        }

        pub(super) fn remove_if_empty(&mut self) -> bool {
            if self.committed || self.prepare_rename().is_err() {
                return false;
            }
            let Some(file) = self.file.as_ref() else {
                return false;
            };
            if inspect(&self.path, file, true).ok() != Some(self.identity) {
                return false;
            }
            // The kernel checks emptiness atomically. A concurrent user's file
            // makes this fail; no enumeration/copy/recursive delete is involved.
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            let removed = unsafe {
                SetFileInformationByHandle(
                    file.as_raw_handle(),
                    FileDispositionInfo,
                    (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            } != 0;
            if removed {
                self.file.take();
            }
            removed
        }
    }
    #[cfg(test)]
    mod identity_tests {
        use super::*;

        #[test]
        fn identical_directory_ids_reject_different_drive_and_unc_spellings() {
            let local = PathBuf::from(r"C:\Users\example\AppData\Local\shinatama");
            let alias = PathBuf::from(r"\\localhost\private-share\state\nested");
            assert!(
                !is_within(&alias, &local),
                "the lexical guard alone cannot detect this alias"
            );
            let data_id = Identity {
                volume: 71,
                high: 0,
                low: 400,
            };
            let child_id = Identity {
                volume: 71,
                high: 0,
                low: 401,
            };
            let other_id = Identity {
                volume: 71,
                high: 0,
                low: 402,
            };
            let protected = vec![(local, data_id)];
            assert_eq!(
                reject_protected_identities(&alias, &[child_id, data_id], &protected)
                    .unwrap_err()
                    .code,
                CreateErrorCode::ProtectedData
            );
            assert_eq!(
                reject_protected_identities(&alias, &[data_id], &protected)
                    .unwrap_err()
                    .code,
                CreateErrorCode::ProtectedData
            );
            assert!(
                reject_protected_identities(&alias, &[other_id], &protected).is_ok(),
                "siblings on the same volume remain allowed"
            );
        }

        #[test]
        fn incomplete_filesystem_identities_are_never_treated_as_verified_separation() {
            assert!(
                !Identity {
                    volume: 0,
                    high: 0,
                    low: 12
                }
                .is_verifiable()
            );
            assert!(
                !Identity {
                    volume: 7,
                    high: 0,
                    low: 0
                }
                .is_verifiable()
            );
            assert!(
                Identity {
                    volume: 7,
                    high: 1,
                    low: 0
                }
                .is_verifiable()
            );
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicU8, Ordering},
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("shinatama-create-tests-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("roots")).unwrap();
            Self(path)
        }
        fn root(&self) -> PathBuf {
            self.0.join("roots")
        }
        fn grant(&self) -> CreationGrant {
            CreationGrant::prepare(&self.root(), &[]).unwrap()
        }
        fn stages(&self) -> Vec<PathBuf> {
            fs::read_dir(self.root())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(STAGING_PREFIX)
                })
                .collect()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Default)]
    struct Control(AtomicU8);
    impl Control {
        fn cancel(&self) -> bool {
            self.0
                .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        }
    }
    impl Cancellation for Control {
        fn is_cancelled(&self) -> bool {
            self.0.load(Ordering::SeqCst) == 1
        }
    }
    struct Permit<'a>(&'a Control);
    impl Drop for Permit<'_> {
        fn drop(&mut self) {
            self.0.0.store(3, Ordering::SeqCst);
        }
    }
    impl CommitControl for Control {
        fn begin_commit(&self) -> Result<Box<dyn CommitGuard + '_>> {
            self.0
                .compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst)
                .map_err(|_| CreateError::cancelled())?;
            Ok(Box::new(Permit(self)))
        }
    }

    struct Git<F>(F);
    impl<F> GitInitializer for Git<F>
    where
        F: Fn(&Path, &dyn Cancellation) -> std::result::Result<(), GitInitError> + Send + Sync,
    {
        fn initialize(
            &self,
            directory: &Path,
            cancellation: &dyn Cancellation,
        ) -> std::result::Result<(), GitInitError> {
            (self.0)(directory, cancellation)
        }
    }

    #[test]
    fn windows_name_corpus_rejects_aliases_devices_and_path_payloads() {
        for name in [
            "",
            ".",
            "..",
            "CON",
            "con.txt",
            "CON .txt",
            "prn",
            "AUX.rs",
            "NUL",
            "COM1",
            "com9.log",
            "LPT1",
            "Lpt9.log",
            "COM¹",
            "COM².foo",
            "LPT³",
            "CONIN$",
            "CONOUT$",
            "C:\\escape",
            "../escape",
            "a/b",
            "a\\b",
            "a:b",
            "a?b",
            "a*b",
            "a|b",
            "a<b",
            "a>b",
            "a\"b",
            "a\0b",
            "a\nb",
            "trailing.",
            "trailing ",
            ".shinatama-create-elsewhere.tmp",
            CREATION_LOCK_NAME,
        ] {
            assert_eq!(
                validate_name(name).unwrap_err().code,
                CreateErrorCode::InvalidName,
                "{name:?}"
            );
        }
        assert!(validate_name(&"x".repeat(256)).is_err());
        assert!(validate_name(&"界".repeat(256)).is_err());
        assert!(validate_name(&"ß".repeat(1024 * 1024)).is_err());
        validate_name(&"界".repeat(255)).unwrap();
        validate_name(&format!("{}x", "😀".repeat(127))).unwrap();
        assert!(validate_name(&"😀".repeat(128)).is_err());
        for name in [
            "Studio",
            "ricerca Ω 日本語",
            ".config",
            "com10",
            "lpt0",
            "prn_notes",
            "x_y-z",
            &"x".repeat(255),
        ] {
            validate_name(name).unwrap();
        }
    }

    #[test]
    fn explicit_existing_root_is_required_and_data_overlap_is_rejected_without_creation() {
        let fixture = Fixture::new();
        let missing = fixture.0.join("missing-default");
        assert!(CreationGrant::prepare(&missing, &[]).is_err());
        assert!(!missing.exists());
        assert!(CreationGrant::prepare(Path::new("relative"), &[]).is_err());
        let data = fixture.root().join("AppData");
        fs::create_dir(&data).unwrap();
        fs::write(data.join("saved.db"), b"authoritative state").unwrap();
        let grant = CreationGrant::prepare(&fixture.root(), std::slice::from_ref(&data)).unwrap();
        assert_eq!(
            create_project(&grant, "AppData", None, &Control::default())
                .unwrap_err()
                .code,
            CreateErrorCode::ProtectedData
        );
        assert_eq!(
            CreationGrant::prepare(&data, std::slice::from_ref(&data))
                .unwrap_err()
                .code,
            CreateErrorCode::ProtectedData
        );
        let nested = data.join("nested");
        fs::create_dir(&nested).unwrap();
        assert_eq!(
            CreationGrant::prepare(&nested, std::slice::from_ref(&data))
                .unwrap_err()
                .code,
            CreateErrorCode::ProtectedData
        );
        assert_eq!(
            fs::read(data.join("saved.db")).unwrap(),
            b"authoritative state"
        );
        assert!(fixture.stages().is_empty());
        assert!(!fixture.root().join(CREATION_LOCK_NAME).exists());
        assert!(!data.join("child").exists());
    }

    #[test]
    fn no_git_creation_commits_once_and_case_aliases_never_modify_existing_work() {
        let fixture = Fixture::new();
        let grant = fixture.grant();
        let token = Control::default();
        let project = create_project(&grant, "Studio Ω", None, &token).unwrap();
        assert_eq!(project.path, grant.root().join("Studio Ω"));
        assert!(!project.git_initialized);
        assert!(
            !token.cancel(),
            "commit cannot be acknowledged as cancelled"
        );
        fs::write(project.path.join("user.txt"), b"saved work").unwrap();
        assert_eq!(
            create_project(&grant, "studio Ω", None, &Control::default())
                .unwrap_err()
                .code,
            CreateErrorCode::AlreadyExists
        );
        assert_eq!(
            fs::read(project.path.join("user.txt")).unwrap(),
            b"saved work"
        );
        let short = create_project(&grant, "x", None, &Control::default()).unwrap();
        assert!(
            short.path.is_dir(),
            "one-character names must rename correctly"
        );
        assert!(fixture.stages().is_empty());
    }

    #[test]
    fn git_success_publishes_its_complete_directory_only_after_initialization() {
        let fixture = Fixture::new();
        let grant = fixture.grant();
        let destination = grant.root().join("with-git");
        let initializer = Git(|directory: &Path, _: &dyn Cancellation| {
            assert!(
                !destination.exists(),
                "the final name is not visible during preparation"
            );
            fs::create_dir(directory.join(".git")).unwrap();
            fs::write(directory.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
            Ok(())
        });
        let project =
            create_project(&grant, "with-git", Some(&initializer), &Control::default()).unwrap();
        assert!(project.git_initialized);
        assert_eq!(
            fs::read(project.path.join(".git/HEAD")).unwrap(),
            b"ref: refs/heads/main\n"
        );
        assert!(fixture.stages().is_empty());
    }

    #[test]
    fn creation_crash_helper() {
        let Some(root) = std::env::var_os("SHINATAMA_CREATE_CRASH_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let grant = CreationGrant::prepare(&root, &[]).unwrap();
        let interrupted = Git(|directory: &Path, _: &dyn Cancellation| {
            let mut file = fs::File::create(directory.join("recover-after-crash.txt")).unwrap();
            std::io::Write::write_all(&mut file, b"durable work before commit").unwrap();
            file.sync_all().unwrap();
            // Synthetic process loss bypasses Rust cleanup. No production hook
            // or arbitrary subprocess command is exposed by this test harness.
            std::process::exit(73);
        });
        let _ = create_project(
            &grant,
            "never-published",
            Some(&interrupted),
            &Control::default(),
        );
        panic!("synthetic interruption must terminate the child");
    }

    #[test]
    fn process_loss_retains_durable_staging_and_releases_the_root_lock() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let fixture = Fixture::new();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "create::tests::creation_crash_helper",
                "--test-threads=1",
            ])
            .env("SHINATAMA_CREATE_CRASH_ROOT", fixture.root())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("synthetic child exceeded its fixed wait budget");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(73));
        assert!(!fixture.root().join("never-published").exists());
        let stages = fixture.stages();
        assert_eq!(stages.len(), 1);
        assert_eq!(
            fs::read(stages[0].join("recover-after-crash.txt")).unwrap(),
            b"durable work before commit"
        );
        let grant = fixture.grant();
        let recovered =
            create_project(&grant, "explicit-retry", None, &Control::default()).unwrap();
        assert!(recovered.path.is_dir());
        assert_eq!(
            fs::read(stages[0].join("recover-after-crash.txt")).unwrap(),
            b"durable work before commit"
        );
    }

    #[test]
    fn two_parallel_creations_have_exactly_one_winner() {
        let fixture = Fixture::new();
        let grant = Arc::new(fixture.grant());
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let grant = grant.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    create_project(&grant, "one-project", None, &Control::default())
                })
            })
            .collect();
        let outcomes: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(outcomes.iter().filter(|result| matches!(result, Err(error) if error.code == CreateErrorCode::AlreadyExists)).count(), 1);
        assert!(fixture.root().join("one-project").is_dir());
        assert!(fixture.stages().is_empty());
    }

    #[test]
    fn git_failure_removes_only_an_empty_owned_stage_and_preserves_partial_work() {
        let fixture = Fixture::new();
        let grant = fixture.grant();
        let unrelated = fixture.root().join("existing-project");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("saved.txt"), b"keep this").unwrap();
        let empty_failure = Git(|_: &Path, _: &dyn Cancellation| {
            Err(GitInitError::Failed(
                "synthetic failure before writes".into(),
            ))
        });
        let error = create_project(
            &grant,
            "empty-failure",
            Some(&empty_failure),
            &Control::default(),
        )
        .unwrap_err();
        assert!(error.recovery_path.is_none());
        assert!(fixture.stages().is_empty());
        let writes_then_fails = Git(|directory: &Path, _: &dyn Cancellation| {
            fs::create_dir(directory.join(".git")).unwrap();
            fs::write(directory.join(".git/config"), b"partial Git data").unwrap();
            fs::write(directory.join("concurrent-user.txt"), b"unsaved user work").unwrap();
            Err(GitInitError::Failed(
                "synthetic failure after writes".into(),
            ))
        });
        let error = create_project(
            &grant,
            "partial-failure",
            Some(&writes_then_fails),
            &Control::default(),
        )
        .unwrap_err();
        let recovery = error.recovery_path.unwrap();
        assert_eq!(
            fs::read(recovery.join(".git/config")).unwrap(),
            b"partial Git data"
        );
        assert_eq!(
            fs::read(recovery.join("concurrent-user.txt")).unwrap(),
            b"unsaved user work"
        );
        assert!(!fixture.root().join("empty-failure").exists());
        assert!(!fixture.root().join("partial-failure").exists());
        assert_eq!(fs::read(unrelated.join("saved.txt")).unwrap(), b"keep this");
    }

    #[test]
    fn cancellation_during_git_never_publishes_or_discards_partial_files() {
        let fixture = Fixture::new();
        let grant = fixture.grant();
        let token = Control::default();
        assert!(token.cancel());
        assert_eq!(
            create_project(&grant, "early", None, &token)
                .unwrap_err()
                .code,
            CreateErrorCode::Cancelled
        );
        assert_eq!(fs::read_dir(fixture.root()).unwrap().count(), 0);
        let token = Control::default();
        let initializer = Git(|directory: &Path, _: &dyn Cancellation| {
            fs::write(directory.join("user-draft.txt"), b"concurrent content").unwrap();
            assert!(token.cancel());
            Ok(())
        });
        let error = create_project(&grant, "cancelled", Some(&initializer), &token).unwrap_err();
        assert_eq!(error.code, CreateErrorCode::Cancelled);
        assert_eq!(
            fs::read(error.recovery_path.unwrap().join("user-draft.txt")).unwrap(),
            b"concurrent content"
        );
        assert!(!fixture.root().join("cancelled").exists());
    }

    #[test]
    fn a_destination_created_at_commit_is_never_overwritten() {
        struct Competitor {
            token: Control,
            destination: PathBuf,
        }
        impl Cancellation for Competitor {
            fn is_cancelled(&self) -> bool {
                self.token.is_cancelled()
            }
        }
        impl CommitControl for Competitor {
            fn begin_commit(&self) -> Result<Box<dyn CommitGuard + '_>> {
                fs::create_dir(&self.destination).unwrap();
                fs::write(
                    self.destination.join("other-writer.txt"),
                    b"other writer wins",
                )
                .unwrap();
                self.token.begin_commit()
            }
        }
        let fixture = Fixture::new();
        let grant = fixture.grant();
        let competitor = Competitor {
            token: Control::default(),
            destination: grant.root().join("race"),
        };
        assert!(create_project(&grant, "race", None, &competitor).is_err());
        assert_eq!(
            fs::read(competitor.destination.join("other-writer.txt")).unwrap(),
            b"other writer wins"
        );
        assert!(fixture.stages().is_empty());
    }

    #[test]
    fn retained_failures_have_a_fixed_budget_and_no_ninth_directory_is_reserved() {
        let fixture = Fixture::new();
        let grant = fixture.grant();
        let initializer = Git(|directory: &Path, _: &dyn Cancellation| {
            fs::write(directory.join("recover.txt"), b"retain every byte").unwrap();
            Err(GitInitError::Failed("synthetic partial write".into()))
        });
        for index in 0..MAX_RETAINED_STAGES {
            let error = create_project(
                &grant,
                &format!("failed-{index}"),
                Some(&initializer),
                &Control::default(),
            )
            .unwrap_err();
            assert!(error.recovery_path.is_some());
        }
        let before = fixture.stages();
        assert_eq!(before.len(), MAX_RETAINED_STAGES);
        assert_eq!(
            create_project(&grant, "ninth", Some(&initializer), &Control::default())
                .unwrap_err()
                .code,
            CreateErrorCode::RecoveryRequired
        );
        assert_eq!(fixture.stages().len(), MAX_RETAINED_STAGES);
        for directory in before {
            assert_eq!(
                fs::read(directory.join("recover.txt")).unwrap(),
                b"retain every byte"
            );
        }
        assert!(!fixture.root().join("ninth").exists());
    }

    #[test]
    fn existing_lock_content_and_hard_links_are_never_modified() {
        let fixture = Fixture::new();
        let grant = fixture.grant();
        let lock = fixture.root().join(CREATION_LOCK_NAME);
        fs::write(&lock, b"user-owned contents").unwrap();
        assert_eq!(
            create_project(&grant, "new", None, &Control::default())
                .unwrap_err()
                .code,
            CreateErrorCode::InvalidRoot
        );
        assert_eq!(fs::read(&lock).unwrap(), b"user-owned contents");
        fs::remove_file(&lock).unwrap();
        let original = fixture.0.join("other-empty-file");
        fs::write(&original, b"").unwrap();
        fs::hard_link(&original, &lock).unwrap();
        assert_eq!(
            create_project(&grant, "new", None, &Control::default())
                .unwrap_err()
                .code,
            CreateErrorCode::InvalidRoot
        );
        assert!(original.exists());
        assert_eq!(fs::metadata(&original).unwrap().len(), 0);
        assert!(fixture.stages().is_empty());
    }
}
