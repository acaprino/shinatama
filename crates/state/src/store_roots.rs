//! C11.2 filesystem preparation only. No store-root identity, configuration revision,
//! registration or capability qualification is inferred from a prepared directory.
//! FS03 owns those authoritative records. Agent file contents and credentials are
//! never read; preparation inspects only path/directory metadata and ACLs.

use crate::location::{LocationError, StorageLocation};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

const MAX_ROLES: usize = 10;
const MAX_DIRECTORIES: usize = 32;
const MAX_ANCESTORS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStoreKey(String);

impl AgentStoreKey {
    /// The engine must first resolve a registered adapter. This is a safe directory
    /// component, not a second adapter catalog and not evidence of native support.
    pub fn new(value: &str) -> Result<Self> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            || matches!(
                value,
                "con"
                    | "prn"
                    | "aux"
                    | "nul"
                    | "com1"
                    | "com2"
                    | "com3"
                    | "com4"
                    | "com5"
                    | "com6"
                    | "com7"
                    | "com8"
                    | "com9"
                    | "lpt1"
                    | "lpt2"
                    | "lpt3"
                    | "lpt4"
                    | "lpt5"
                    | "lpt6"
                    | "lpt7"
                    | "lpt8"
                    | "lpt9"
            )
        {
            return Err(StoreRootError::new(
                StoreRootErrorKind::InvalidRequest,
                "Identificativo dell'agente non valido per una cartella privata.",
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StorePathRole {
    Home,
    Temporary,
    Cache,
    NodeCompileCache,
    XdgConfig,
    XdgData,
    XdgCache,
    XdgState,
    OpenCodeDatabase,
    DeepSeekAgents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathLengthUnit {
    Utf8Bytes,
    Utf16Units,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HomePathLimit {
    pub maximum: usize,
    pub unit: PathLengthUnit,
}

#[derive(Clone, Debug, Default)]
pub struct PrivateStoreRequest {
    /// Home, Temporary and Cache always exist in the plan. Extras are fixed roles,
    /// never caller-supplied path fragments. OpenCodeDatabase denotes a file only.
    pub extra_roles: Vec<StorePathRole>,
    /// Taken from the exact qualified version/recipe, including its socket suffix.
    /// None is layout-only preparation; it does not qualify a Codex/native launch.
    pub home_path_limit: Option<HomePathLimit>,
}

#[derive(Clone, Debug)]
pub struct PrivateStorePaths {
    root: PathBuf,
    paths: BTreeMap<StorePathRole, PathBuf>,
}

impl PrivateStorePaths {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn iter(&self) -> impl Iterator<Item = (StorePathRole, &Path)> {
        self.paths
            .iter()
            .map(|(role, path)| (*role, path.as_path()))
    }
    pub fn path(&self, role: StorePathRole) -> Option<&Path> {
        self.paths.get(&role).map(PathBuf::as_path)
    }
    pub fn home(&self) -> &Path {
        self.path(StorePathRole::Home).expect("mandatory home role")
    }
    pub fn temporary(&self) -> &Path {
        self.path(StorePathRole::Temporary)
            .expect("mandatory temporary role")
    }
    pub fn cache(&self) -> &Path {
        self.path(StorePathRole::Cache)
            .expect("mandatory cache role")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreRootErrorKind {
    InvalidRequest,
    PathLimit,
    NamespaceAlias,
    Location,
    Io,
    Unsupported,
}

#[derive(Debug)]
pub struct StoreRootError {
    pub kind: StoreRootErrorKind,
    pub message: String,
    /// Newly created directories whose emptiness/identity could not be confirmed.
    /// Retained work is never deleted recursively, even on failed preparation.
    pub retained_directories: Vec<PathBuf>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl StoreRootError {
    fn new(kind: StoreRootErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retained_directories: Vec::new(),
            source: None,
        }
    }

    fn io(path: &Path, source: io::Error) -> Self {
        let mut error = Self::new(
            StoreRootErrorKind::Io,
            format!(
                "Cartella privata non disponibile: {}. {source}",
                path.display()
            ),
        );
        error.source = Some(Box::new(source));
        error
    }
}

impl From<LocationError> for StoreRootError {
    fn from(source: LocationError) -> Self {
        Self {
            kind: StoreRootErrorKind::Location,
            message: source.to_string(),
            retained_directories: Vec::new(),
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Display for StoreRootError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(&self.message)?;
        if !self.retained_directories.is_empty() {
            output.write_str(
                " Le cartelle di preparazione rimaste sono conservate per un controllo esplicito.",
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for StoreRootError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|source| source.as_ref() as _)
    }
}

pub type Result<T> = std::result::Result<T, StoreRootError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistrationStatus {
    PendingFs03,
}

#[derive(Debug, Default)]
pub struct RollbackReport {
    pub removed_directories: Vec<PathBuf>,
    pub retained_directories: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct DirectoryPlan {
    path: PathBuf,
    location: StorageLocation,
}

#[derive(Clone, Debug)]
pub struct PrivateStorePlan {
    base: StorageLocation,
    agent: AgentStoreKey,
    request: PrivateStoreRequest,
    paths: PrivateStorePaths,
    directories: Vec<DirectoryPlan>,
}

impl PrivateStorePlan {
    /// Where `agent`'s private home sits under the data root `base`. Nothing is verified,
    /// created or opened. `resolve` places the home exactly here; a process that only
    /// reads the harness's metadata (the sidecar reads Codex's model table) uses the same
    /// spelling instead of a copy of the layout.
    pub fn home_path(base: &Path, agent: &AgentStoreKey) -> PathBuf {
        agent_root(base, agent).join("home")
    }

    /// Read-only resolution. C01 must already have prepared the application data
    /// root. Existing roots/directories are verified, never repaired or re-ACL'd, except the
    /// agent's temporary folder, whose permissions the harness itself may widen.
    /// System homes are deliberately not representable as a write destination.
    pub fn resolve(
        base: &StorageLocation,
        agent: AgentStoreKey,
        request: PrivateStoreRequest,
    ) -> Result<Self> {
        if request.extra_roles.len() > MAX_ROLES
            || base.directory().ancestors().take(MAX_ANCESTORS + 1).count() > MAX_ANCESTORS
        {
            return Err(StoreRootError::new(
                StoreRootErrorKind::InvalidRequest,
                "Il piano supera il limite di percorsi privati o di profondità.",
            ));
        }
        if request.home_path_limit.is_some_and(|limit| {
            limit.maximum == 0
                || limit.maximum
                    > match limit.unit {
                        PathLengthUnit::Utf8Bytes => 131068,
                        PathLengthUnit::Utf16Units => 32767,
                    }
        }) {
            return Err(StoreRootError::new(
                StoreRootErrorKind::InvalidRequest,
                "Limite della home non valido.",
            ));
        }
        base.verify_private_directory()?;
        verify_namespace(base, base.directory())?;
        let agent_root = agent_root(base.directory(), &agent);
        let home = Self::home_path(base.directory(), &agent);
        let temporary = agent_root.join("tmp");
        let cache = agent_root.join("cache");
        if let Some(limit) = request.home_path_limit {
            let length = match limit.unit {
                PathLengthUnit::Utf8Bytes => home
                    .to_str()
                    .ok_or_else(|| {
                        StoreRootError::new(
                            StoreRootErrorKind::Unsupported,
                            "La home non è rappresentabile nel limite UTF-8 richiesto dal profilo.",
                        )
                    })?
                    .len(),
                PathLengthUnit::Utf16Units => utf16_length(&home),
            };
            if length > limit.maximum {
                return Err(StoreRootError::new(
                    StoreRootErrorKind::PathLimit,
                    format!(
                        "La home privata supera il limite {} {:?} qualificato dal profilo. Scegli esplicitamente una cartella dati più corta; nessun archivio è stato spostato.",
                        limit.maximum, limit.unit
                    ),
                ));
            }
        }
        let roles: BTreeSet<_> = request
            .extra_roles
            .iter()
            .copied()
            .chain([
                StorePathRole::Home,
                StorePathRole::Temporary,
                StorePathRole::Cache,
            ])
            .collect();
        let mut paths = BTreeMap::new();
        for role in roles {
            let path = match role {
                StorePathRole::Home => home.clone(),
                StorePathRole::Temporary => temporary.clone(),
                StorePathRole::Cache => cache.clone(),
                StorePathRole::NodeCompileCache => cache.join("node"),
                StorePathRole::XdgConfig => home.join("xdg").join("config"),
                StorePathRole::XdgData => home.join("xdg").join("data"),
                StorePathRole::XdgCache => cache.join("xdg"),
                StorePathRole::XdgState => home.join("xdg").join("state"),
                StorePathRole::OpenCodeDatabase => home.join("opencode.db"),
                StorePathRole::DeepSeekAgents => home.join("agents"),
            };
            if utf16_length(&path) >= 32767 {
                return Err(StoreRootError::new(
                    StoreRootErrorKind::PathLimit,
                    "Il percorso privato supera il limite di Windows.",
                ));
            }
            if role == StorePathRole::OpenCodeDatabase {
                // A file OpenCode creates in its home at the first start: its folder is verified
                // like the others, and the file, once there, must be a plain file (no link, no
                // folder). Verifying the file as a folder refused every start after the first.
                verify_namespace(base, path.parent().ok_or_else(namespace_error)?)?;
                match std::fs::symlink_metadata(&path) {
                    Ok(found) if !found.file_type().is_file() => return Err(namespace_error()),
                    Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(namespace_error()),
                    _ => {}
                }
            } else {
                verify_namespace(base, &path)?;
            }
            paths.insert(role, path);
        }
        let mut directory_paths = BTreeSet::new();
        for (role, path) in &paths {
            let mut cursor = if *role == StorePathRole::OpenCodeDatabase {
                path.parent()
            } else {
                Some(path.as_path())
            };
            while let Some(path) = cursor {
                if same_path(path, base.directory()) {
                    break;
                }
                if !crate::location::is_within(path, base.directory()) {
                    return Err(namespace_error());
                }
                directory_paths.insert(path.to_path_buf());
                if directory_paths.len() > MAX_DIRECTORIES {
                    return Err(StoreRootError::new(
                        StoreRootErrorKind::InvalidRequest,
                        "Troppe cartelle nel piano privato.",
                    ));
                }
                cursor = path.parent();
            }
        }
        let mut directory_paths: Vec<_> = directory_paths.into_iter().collect();
        directory_paths.sort_by_key(|path| (path.components().count(), path.clone()));
        let mut directories = Vec::with_capacity(directory_paths.len());
        for path in directory_paths {
            let location = verify_namespace(base, &path)?;
            if path
                .try_exists()
                .map_err(|error| StoreRootError::io(&path, error))?
            {
                let verified = location.verify_private_directory();
                // The harness works in its temporary folder and may open it to its own sandbox
                // (Codex on Windows adds an entry for its sandbox identity). It is the current
                // user's and holds no credential: it is made private again here, where a refusal
                // stopped every later start and resume. Home and cache are never repaired.
                if verified.is_err() && path == temporary {
                    location.prepare_private_directory()?;
                } else {
                    verified?;
                }
            }
            directories.push(DirectoryPlan { path, location });
        }
        Ok(Self {
            base: base.clone(),
            agent,
            request,
            paths: PrivateStorePaths {
                root: agent_root,
                paths,
            },
            directories,
        })
    }

    pub fn paths(&self) -> &PrivateStorePaths {
        &self.paths
    }
    pub fn registration_status(&self) -> RegistrationStatus {
        RegistrationStatus::PendingFs03
    }

    /// Caller uses a bounded blocking worker. No subprocess is started, no agent
    /// configuration is opened, and no existing ACL is changed by this operation.
    pub fn prepare(&self) -> Result<PreparedPrivateStore> {
        let current = Self::resolve(&self.base, self.agent.clone(), self.request.clone())?;
        #[cfg(windows)]
        return native::prepare(current);
        #[cfg(not(windows))]
        {
            let _ = current;
            Err(StoreRootError::new(
                StoreRootErrorKind::Unsupported,
                "Le cartelle private sono disponibili su Windows.",
            ))
        }
    }
}

fn agent_root(base: &Path, agent: &AgentStoreKey) -> PathBuf {
    base.join("agents").join(agent.as_str())
}

fn namespace_error() -> StoreRootError {
    StoreRootError::new(
        StoreRootErrorKind::NamespaceAlias,
        "La cartella privata è stata reindirizzata fuori dal suo percorso Shinatama. Nessun percorso alternativo verrà usato.",
    )
}

fn verify_namespace(base: &StorageLocation, expected: &Path) -> Result<StorageLocation> {
    let resolved = base.resolve_related_directory(expected)?;
    if !same_path(resolved.directory(), expected) {
        return Err(namespace_error());
    }
    Ok(resolved)
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    return native::same_path(left, right);
    #[cfg(not(windows))]
    return left == right;
}

fn utf16_length(path: &Path) -> usize {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().count()
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy().encode_utf16().count()
    }
}

#[derive(Debug)]
pub struct PreparedPrivateStore {
    plan: PrivateStorePlan,
    #[cfg(windows)]
    _root_guards: Vec<std::fs::File>,
    #[cfg(windows)]
    directories: Vec<native::DirectoryReceipt>,
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::ptr::{null, null_mut};
    use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_CREATE, FILE_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
        NtCreateFile,
    };
    use windows_sys::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, LocalFree, RtlNtStatusToDosError, UNICODE_STRING,
    };
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfo,
        GetFileInformationByHandle, SetFileInformationByHandle,
    };
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    const READ_CONTROL: u32 = 0x0002_0000;
    const DELETE_ACCESS: u32 = 0x0001_0000;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CompareStringOrdinal(
            left: *const u16,
            left_length: i32,
            right: *const u16,
            right_length: i32,
            ignore_case: i32,
        ) -> i32;
    }

    pub(super) fn same_path(left: &Path, right: &Path) -> bool {
        let left: Vec<_> = left.as_os_str().encode_wide().collect();
        let right: Vec<_> = right.as_os_str().encode_wide().collect();
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

    fn identity(path: &Path, file: &File) -> Result<Identity> {
        let mut value: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut value) } == 0 {
            return Err(StoreRootError::io(path, io::Error::last_os_error()));
        }
        if value.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || value.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || value.dwVolumeSerialNumber == 0
            || (value.nFileIndexHigh == 0 && value.nFileIndexLow == 0)
        {
            return Err(StoreRootError::new(
                StoreRootErrorKind::NamespaceAlias,
                "La cartella privata non è una directory regolare con identità verificabile.",
            ));
        }
        Ok(Identity {
            volume: value.dwVolumeSerialNumber,
            high: value.nFileIndexHigh,
            low: value.nFileIndexLow,
        })
    }

    fn open_directory(path: &Path, access: u32) -> Result<File> {
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES | access)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|source| StoreRootError::io(path, source))?;
        identity(path, &file)?;
        Ok(file)
    }

    struct Descriptor(*mut std::ffi::c_void);
    impl Drop for Descriptor {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    LocalFree(self.0);
                }
            }
        }
    }

    fn private_descriptor(root: &Path, file: &File) -> Result<Descriptor> {
        let mut descriptor = null_mut();
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(StoreRootError::io(
                root,
                io::Error::from_raw_os_error(status as i32),
            ));
        }
        // C01 has just verified this root's current-user protected, inheritable
        // ACL. Supplying that descriptor creates every new directory private in
        // the same NtCreateFile call that returns its stable identity handle.
        Ok(Descriptor(descriptor))
    }

    #[derive(Debug)]
    pub(super) struct DirectoryReceipt {
        path: PathBuf,
        file: Option<File>,
        identity: Identity,
        created: bool,
    }

    fn create_or_open(
        path: &Path,
        parent: &File,
        descriptor: &Descriptor,
    ) -> Result<DirectoryReceipt> {
        let mut leaf: Vec<u16> = path
            .file_name()
            .expect("fixed private path component")
            .encode_wide()
            .collect();
        let name = UNICODE_STRING {
            Length: (leaf.len() * 2) as u16,
            MaximumLength: (leaf.len() * 2) as u16,
            Buffer: leaf.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: parent.as_raw_handle(),
            ObjectName: &name,
            Attributes: 0x40,
            SecurityDescriptor: descriptor.0.cast(),
            SecurityQualityOfService: null(),
        };
        let mut handle = null_mut();
        let mut status_block: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                FILE_READ_ATTRIBUTES | 0x0010_0000,
                &attributes,
                &mut status_block,
                null(),
                FILE_ATTRIBUTE_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                FILE_CREATE,
                FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
                null(),
                0,
            )
        };
        let (file, created) = if status >= 0 {
            (unsafe { File::from_raw_handle(handle) }, true)
        } else {
            let error =
                io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(status) } as i32);
            if !matches!(
                error.raw_os_error().map(|code| code as u32),
                Some(ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS)
            ) {
                return Err(StoreRootError::io(path, error));
            }
            (open_directory(path, 0)?, false)
        };
        let identity = identity(path, &file).map_err(|mut error| {
            if created {
                error.retained_directories.push(path.to_path_buf());
            }
            error
        })?;
        Ok(DirectoryReceipt {
            path: path.to_path_buf(),
            file: Some(file),
            identity,
            created,
        })
    }

    pub(super) fn prepare(plan: PrivateStorePlan) -> Result<PreparedPrivateStore> {
        let root = plan.base.directory();
        let mut root_guards = Vec::new();
        for (index, path) in root.ancestors().enumerate() {
            root_guards.push(open_directory(
                path,
                if index == 0 { READ_CONTROL } else { 0 },
            )?);
        }
        plan.base.verify_private_directory()?;
        verify_namespace(&plan.base, root)?;
        let root_again = open_directory(root, 0)?;
        if identity(root, &root_guards[0])? != identity(root, &root_again)? {
            return Err(namespace_error());
        }
        let descriptor = private_descriptor(root, &root_guards[0])?;
        let mut receipts: Vec<DirectoryReceipt> = Vec::with_capacity(plan.directories.len());
        for directory in &plan.directories {
            let result = (|| -> Result<()> {
                verify_namespace(&plan.base, &directory.path)?;
                let parent_path = directory.path.parent().expect("child of application root");
                let parent = if same_path(parent_path, root) {
                    &root_guards[0]
                } else {
                    receipts
                        .iter()
                        .find(|receipt| same_path(&receipt.path, parent_path))
                        .and_then(|receipt| receipt.file.as_ref())
                        .ok_or_else(namespace_error)?
                };
                let receipt = create_or_open(&directory.path, parent, &descriptor)?;
                receipts.push(receipt);
                // Verification only. Existing directories, even with a correct
                // owner but broad ACLs, are never repaired by launch preparation.
                directory.location.verify_private_directory()?;
                Ok(())
            })();
            if let Err(mut error) = result {
                let rollback = rollback(&mut receipts);
                error
                    .retained_directories
                    .extend(rollback.retained_directories);
                return Err(error);
            }
        }
        Ok(PreparedPrivateStore {
            plan,
            _root_guards: root_guards,
            directories: receipts,
        })
    }

    pub(super) fn rollback(receipts: &mut [DirectoryReceipt]) -> RollbackReport {
        let mut report = RollbackReport::default();
        for receipt in receipts.iter_mut().rev() {
            // Release every child pin before considering a parent. Existing
            // directories are never deletion candidates, including empty ones.
            receipt.file.take();
            if !receipt.created {
                continue;
            }
            let removed = (|| -> Result<bool> {
                let file = open_directory(&receipt.path, DELETE_ACCESS)?;
                if identity(&receipt.path, &file)? != receipt.identity {
                    return Ok(false);
                }
                let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
                // The kernel checks emptiness atomically: concurrent writes make
                // deletion fail. There is no recursive delete or file cleanup.
                Ok(unsafe {
                    SetFileInformationByHandle(
                        file.as_raw_handle(),
                        FileDispositionInfo,
                        (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                        size_of::<FILE_DISPOSITION_INFO>() as u32,
                    )
                } != 0)
            })()
            .unwrap_or(false);
            if removed {
                report.removed_directories.push(receipt.path.clone());
            } else {
                report.retained_directories.push(receipt.path.clone());
            }
        }
        report
    }
}

impl PreparedPrivateStore {
    pub fn paths(&self) -> &PrivateStorePaths {
        self.plan.paths()
    }
    pub fn registration_status(&self) -> RegistrationStatus {
        RegistrationStatus::PendingFs03
    }

    /// Explicit only: ordinary Drop preserves every directory and all work. Even
    /// rollback cannot remove an existing directory or any nonempty new directory.
    pub fn rollback_empty_created(mut self) -> RollbackReport {
        #[cfg(windows)]
        return native::rollback(&mut self.directories);
        #[cfg(not(windows))]
        {
            RollbackReport::default()
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::location::LocationRequest;
    use std::fs;

    struct Fixture {
        root: PathBuf,
        location: Option<StorageLocation>,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("shinatama-store-roots-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let location = StorageLocation::resolve(LocationRequest {
                local_data_directory: root.join("platform"),
                home_override: Some(root.join("private-Ω")),
                project_roots: Vec::new(),
                synchronized_roots: Vec::new(),
            })
            .unwrap();
            location.prepare_private_directory().unwrap();
            Self {
                root,
                location: Some(location),
            }
        }

        fn location(&self) -> &StorageLocation {
            self.location.as_ref().unwrap()
        }

        fn plan(&self, key: &str) -> PrivateStorePlan {
            PrivateStorePlan::resolve(
                self.location(),
                AgentStoreKey::new(key).unwrap(),
                PrivateStoreRequest::default(),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.location.take();
            assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
            assert!(
                self.root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("shinatama-store-roots-")
            );
            if let Err(error) = fs::remove_dir_all(&self.root) {
                eprintln!("Synthetic private-store fixture cleanup failed: {error}");
            }
        }
    }

    #[test]
    fn keys_are_bounded_components_and_never_paths_or_device_names() {
        for key in [
            "",
            ".",
            "..",
            "../codex",
            "codex/home",
            "codex\\home",
            "Codex",
            "con",
            "com1",
            "lpt9",
            "codex ",
            "codex.",
            "codex:stream",
        ] {
            assert!(AgentStoreKey::new(key).is_err(), "{key:?}");
        }
        assert!(AgentStoreKey::new(&"x".repeat(65)).is_err());
        for key in [
            "claude_code",
            "codex",
            "opencode",
            "pi",
            "hermes_agent",
            "deepseek_harness",
        ] {
            AgentStoreKey::new(key).unwrap();
        }
    }

    #[test]
    fn the_home_path_is_where_resolution_places_the_home_and_touches_nothing() {
        let fixture = Fixture::new();
        for key in ["codex", "opencode"] {
            let agent = AgentStoreKey::new(key).unwrap();
            let expected = PrivateStorePlan::home_path(fixture.location().directory(), &agent);
            assert!(!fixture.location().directory().join("agents").exists());
            assert_eq!(fixture.plan(key).paths().home(), expected.as_path());
            assert!(expected.ends_with(format!("agents/{key}/home")));
        }
    }

    #[test]
    fn resolution_is_readonly_and_directory_preparation_is_not_store_registration() {
        let fixture = Fixture::new();
        let plan = PrivateStorePlan::resolve(
            fixture.location(),
            AgentStoreKey::new("opencode").unwrap(),
            PrivateStoreRequest {
                extra_roles: vec![
                    StorePathRole::NodeCompileCache,
                    StorePathRole::XdgConfig,
                    StorePathRole::XdgData,
                    StorePathRole::XdgCache,
                    StorePathRole::XdgState,
                    StorePathRole::OpenCodeDatabase,
                ],
                home_path_limit: None,
            },
        )
        .unwrap();
        assert_eq!(plan.registration_status(), RegistrationStatus::PendingFs03);
        assert!(!fixture.location().directory().join("agents").exists());
        assert!(!fixture.location().database_path().exists());
        assert!(plan.paths().home().ends_with("agents/opencode/home"));
        assert!(plan.paths().temporary().ends_with("agents/opencode/tmp"));
        assert!(
            plan.paths()
                .path(StorePathRole::XdgConfig)
                .unwrap()
                .ends_with("home/xdg/config")
        );
        assert!(
            plan.paths()
                .path(StorePathRole::OpenCodeDatabase)
                .unwrap()
                .ends_with("home/opencode.db")
        );
        assert_ne!(plan.paths().home(), fixture.plan("codex").paths().home());
        let prepared = plan.prepare().unwrap();
        assert_eq!(
            prepared.registration_status(),
            RegistrationStatus::PendingFs03
        );
        assert!(
            !prepared
                .paths()
                .path(StorePathRole::OpenCodeDatabase)
                .unwrap()
                .exists(),
            "the native database is never created by this module"
        );
        for directory in &plan.directories {
            directory.location.verify_private_directory().unwrap();
        }
        assert!(!fixture.location().database_path().exists());

        // OpenCode creates its database at the first start; the next resolution still succeeds.
        let database = prepared.paths().path(StorePathRole::OpenCodeDatabase).unwrap().to_path_buf();
        drop(prepared);
        fs::write(&database, b"synthetic database").unwrap();
        let again = PrivateStorePlan::resolve(
            fixture.location(),
            AgentStoreKey::new("opencode").unwrap(),
            PrivateStoreRequest { extra_roles: vec![StorePathRole::OpenCodeDatabase], home_path_limit: None },
        )
        .unwrap();
        assert_eq!(again.paths().path(StorePathRole::OpenCodeDatabase).unwrap(), database.as_path());
        // A folder in its place is refused.
        fs::remove_file(&database).unwrap();
        fs::create_dir(&database).unwrap();
        assert!(PrivateStorePlan::resolve(
            fixture.location(),
            AgentStoreKey::new("opencode").unwrap(),
            PrivateStoreRequest { extra_roles: vec![StorePathRole::OpenCodeDatabase], home_path_limit: None },
        )
        .is_err());
    }

    #[test]
    fn reuse_preserves_synthetic_login_and_never_moves_another_agents_home() {
        let fixture = Fixture::new();
        let codex = fixture.plan("codex");
        let prepared = codex.prepare().unwrap();
        let login = prepared.paths().home().join("synthetic-login.fixture");
        fs::write(&login, b"synthetic fixture only").unwrap();
        drop(prepared);
        assert!(
            login.exists(),
            "ordinary Drop never rolls back or deletes a home"
        );
        let reopened = codex.prepare().unwrap();
        assert_eq!(fs::read(&login).unwrap(), b"synthetic fixture only");
        let another = fixture.plan("pi").prepare().unwrap();
        assert_ne!(another.paths().home(), reopened.paths().home());
        assert_eq!(fs::read(&login).unwrap(), b"synthetic fixture only");
    }

    #[test]
    fn rollback_only_removes_new_empty_directories_and_preserves_existing_ones() {
        let fixture = Fixture::new();
        let first = fixture.plan("codex").prepare().unwrap();
        let saved_home = first.paths().home().to_path_buf();
        fs::write(saved_home.join("saved.fixture"), b"keep existing work").unwrap();
        drop(first);
        let second = fixture.plan("pi").prepare().unwrap();
        let second_home = second.paths().home().to_path_buf();
        let report = second.rollback_empty_created();
        assert!(report.retained_directories.is_empty());
        assert!(!report.removed_directories.is_empty());
        assert!(!second_home.exists());
        assert!(fixture.location().directory().join("agents").exists());
        assert_eq!(
            fs::read(saved_home.join("saved.fixture")).unwrap(),
            b"keep existing work"
        );
        fixture.location().verify_private_directory().unwrap();
    }

    #[test]
    fn rollback_preserves_every_concurrent_file_in_a_new_home() {
        let fixture = Fixture::new();
        let prepared = fixture.plan("hermes_agent").prepare().unwrap();
        let home = prepared.paths().home().to_path_buf();
        fs::write(home.join("concurrent.fixture"), b"work after preparation").unwrap();
        let report = prepared.rollback_empty_created();
        assert!(report.retained_directories.contains(&home));
        assert_eq!(
            fs::read(home.join("concurrent.fixture")).unwrap(),
            b"work after preparation"
        );
        assert!(!home.parent().unwrap().join("tmp").exists());
    }

    fn security_descriptor(path: &Path) -> Vec<u8> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Security::{
            DACL_SECURITY_INFORMATION, GetFileSecurityW, OWNER_SECURITY_INFORMATION,
        };
        let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut length = 0u32;
        unsafe {
            GetFileSecurityW(
                path.as_ptr(),
                DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut length,
            );
        }
        assert!(length > 0 && length <= 65536);
        let mut bytes = vec![0u8; length as usize];
        assert_ne!(
            unsafe {
                GetFileSecurityW(
                    path.as_ptr(),
                    DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
                    bytes.as_mut_ptr().cast(),
                    length,
                    &mut length,
                )
            },
            0
        );
        bytes
    }

    fn make_broad_acl(path: &Path) {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
        use windows_sys::Win32::Security::{
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
        };
        let sddl: Vec<_> = "D:P(A;OICI;FA;;;WD)"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let path: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        let status = unsafe {
            SetFileSecurityW(
                path.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            )
        };
        unsafe {
            LocalFree(descriptor);
        }
        assert_ne!(status, 0);
    }

    #[test]
    fn existing_acl_is_never_repaired_during_resolution_or_preparation() {
        let fixture = Fixture::new();
        let plan = fixture.plan("pi");
        let prepared = plan.prepare().unwrap();
        let home = prepared.paths().home().to_path_buf();
        let sentinel = home.join("synthetic-config.fixture");
        fs::write(&sentinel, b"configuration fixture must remain unchanged").unwrap();
        drop(prepared);
        let planned_while_private = fixture.plan("pi");
        make_broad_acl(&home);
        let before = security_descriptor(&home);
        assert!(
            PrivateStorePlan::resolve(
                fixture.location(),
                AgentStoreKey::new("pi").unwrap(),
                PrivateStoreRequest::default()
            )
            .is_err()
        );
        assert!(planned_while_private.prepare().is_err());
        assert_eq!(security_descriptor(&home), before);
        assert_eq!(
            fs::read(sentinel).unwrap(),
            b"configuration fixture must remain unchanged"
        );
        fixture.location().verify_private_directory().unwrap();
    }

    /// Codex's Windows sandbox adds an entry for its sandbox identity to the temporary folder
    /// it is given. Refusing that folder refused every later start and resume.
    #[test]
    fn a_temporary_folder_the_harness_opened_to_others_is_made_private_again_and_the_agent_starts()
    {
        let fixture = Fixture::new();
        let prepared = fixture.plan("codex").prepare().unwrap();
        let temporary = prepared.paths().temporary().to_path_buf();
        let left = temporary.join("left-by-the-harness.fixture");
        fs::write(&left, b"scratch").unwrap();
        drop(prepared);
        make_broad_acl(&temporary);
        let prepared = fixture.plan("codex").prepare().unwrap();
        assert_eq!(prepared.paths().temporary(), temporary);
        fixture
            .location()
            .resolve_related_directory(&temporary)
            .unwrap()
            .verify_private_directory()
            .unwrap();
        assert_eq!(fs::read(left).unwrap(), b"scratch");
    }

    #[test]
    fn home_budgets_use_explicit_units_and_never_relocate_or_create_on_failure() {
        let fixture = Fixture::new();
        let home = fixture.plan("codex").paths().home().to_path_buf();
        let units = utf16_length(&home);
        let bytes = home.to_str().unwrap().len();
        assert!(
            bytes > units,
            "the synthetic root contains a Unicode character"
        );
        let utf16 = PrivateStoreRequest {
            extra_roles: Vec::new(),
            home_path_limit: Some(HomePathLimit {
                maximum: units,
                unit: PathLengthUnit::Utf16Units,
            }),
        };
        assert!(
            PrivateStorePlan::resolve(
                fixture.location(),
                AgentStoreKey::new("codex").unwrap(),
                utf16
            )
            .is_ok()
        );
        let utf8 = PrivateStoreRequest {
            extra_roles: Vec::new(),
            home_path_limit: Some(HomePathLimit {
                maximum: units,
                unit: PathLengthUnit::Utf8Bytes,
            }),
        };
        assert_eq!(
            PrivateStorePlan::resolve(
                fixture.location(),
                AgentStoreKey::new("codex").unwrap(),
                utf8
            )
            .unwrap_err()
            .kind,
            StoreRootErrorKind::PathLimit
        );
        assert!(!fixture.location().directory().join("agents").exists());
    }

    #[test]
    fn a_home_that_became_a_project_is_refused_without_changing_its_files() {
        let fixture = Fixture::new();
        let prepared = fixture.plan("pi").prepare().unwrap();
        let home = prepared.paths().home().to_path_buf();
        drop(prepared);
        fs::write(home.join(".git"), b"synthetic project marker").unwrap();
        fs::write(home.join("saved.fixture"), b"project work").unwrap();
        assert!(
            PrivateStorePlan::resolve(
                fixture.location(),
                AgentStoreKey::new("pi").unwrap(),
                PrivateStoreRequest::default()
            )
            .is_err()
        );
        assert_eq!(
            fs::read(home.join("saved.fixture")).unwrap(),
            b"project work"
        );
    }

    #[test]
    fn a_junction_into_another_agents_home_is_refused_before_any_write() {
        let fixture = Fixture::new();
        let first = fixture.plan("codex").prepare().unwrap();
        let original_home = first.paths().home().to_path_buf();
        fs::write(
            original_home.join("synthetic-login.fixture"),
            b"other agents fixture",
        )
        .unwrap();
        drop(first);
        let second = fixture.plan("pi").prepare().unwrap();
        let redirected = second.paths().home().to_path_buf();
        drop(second);
        // The same local junction fixture mechanism as C01: this test launches
        // only Windows' filesystem helper, never an agent or authentication CLI.
        fs::remove_dir(&redirected).unwrap();
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&redirected)
            .arg(&original_home)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let before = security_descriptor(&original_home);
        let error = PrivateStorePlan::resolve(
            fixture.location(),
            AgentStoreKey::new("pi").unwrap(),
            PrivateStoreRequest::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind, StoreRootErrorKind::NamespaceAlias);
        assert_eq!(security_descriptor(&original_home), before);
        assert_eq!(
            fs::read(original_home.join("synthetic-login.fixture")).unwrap(),
            b"other agents fixture"
        );
        fs::remove_dir(&redirected).unwrap();
    }
}
