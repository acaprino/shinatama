//! App-created Check scratch only. No path adoption, recursive path deletion,
//! native/SQLite cleanup in Drop, content reads or cleanup before Released.
//! Call on the existing bounded Check worker after dropping every C01 owner pin.
#![cfg(windows)]
use crate::resources::admission::CleanupStatus;
use shinatama_state::location::StorageLocation;
use std::{
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    mem::{offset_of, size_of, size_of_val},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path, PathBuf},
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
            FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
        },
    },
    Win32::{
        Foundation::{
            ERROR_LOCK_VIOLATION, ERROR_NO_MORE_FILES, ERROR_SHARING_VIOLATION, GetLastError,
            LocalFree, RtlNtStatusToDosError, UNICODE_STRING,
        },
        Security::{
            Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
            DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        },
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_ID_BOTH_DIR_INFO, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
            FILE_STREAM_INFO, FileDispositionInfo, FileIdBothDirectoryInfo,
            FileIdBothDirectoryRestartInfo, FileStreamInfo, GetFileInformationByHandle,
            GetFileInformationByHandleEx, SetFileInformationByHandle,
        },
        System::IO::IO_STATUS_BLOCK,
    },
};

pub const MAX_ENTRIES: usize = 512;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_DEPTH: u8 = 12;
pub const MAX_ATTEMPTS: u8 = 3;
const TIME_BUDGET: Duration = Duration::from_secs(2);
const READ_CONTROL: u32 = 0x0002_0000;
const DELETE: u32 = 0x0001_0000;
const SYNC: u32 = 0x0010_0000;
const LIST: u32 = 1;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScratchKind {
    ClaudeProbe,
    ClaudeTuiTrial,
}
impl ScratchKind {
    fn prefix(self) -> &'static str {
        match self {
            Self::ClaudeProbe => "claude-probe-",
            Self::ClaudeTuiTrial => "claude-tui-trial-",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Issue {
    Unreleased,
    RetryLimit,
    InvalidPath,
    PrivateRoot,
    IdentityChanged,
    UnsafeEntry,
    BoundExceeded,
    InUse,
    Io,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Removed,
    Retained,
}
#[derive(Clone, Copy, Debug)]
pub struct CleanupReport {
    pub disposition: Disposition,
    pub issue: Option<Issue>,
    pub scanned_entries: usize,
    pub removed_entries: usize,
    pub bytes: u64,
    pub attempts_remaining: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    volume: u32,
    index: u64,
}
pub struct CreatedScratch {
    location: StorageLocation,
    receipt: OwnedScratch,
}
impl CreatedScratch {
    pub fn location(&self) -> &StorageLocation {
        &self.location
    }
    pub fn into_parts(self) -> (StorageLocation, OwnedScratch) {
        (self.location, self.receipt)
    }
}
/// Only `create` can produce this non-Clone/non-Deserialize receipt. Dropping it
/// preserves files. Never rebuild receipts from a filename or a prior app run.
pub struct OwnedScratch {
    base: PathBuf,
    path: PathBuf,
    base_id: Identity,
    root_id: Identity,
    pin: Option<File>,
    attempts: u8,
    removed: bool,
}
impl std::fmt::Debug for OwnedScratch {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("OwnedScratch([redacted])")
    }
}
impl OwnedScratch {
    pub fn path(&self) -> &Path {
        &self.path
    }
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
fn io_issue(code: u32) -> Issue {
    if code == ERROR_SHARING_VIOLATION || code == ERROR_LOCK_VIOLATION {
        Issue::InUse
    } else {
        Issue::Io
    }
}
fn bounded(at: Instant) -> Result<(), Issue> {
    if at.elapsed() < TIME_BUDGET {
        Ok(())
    } else {
        Err(Issue::BoundExceeded)
    }
}
fn shape(base: &Path, path: &Path) -> Result<(), Issue> {
    if !base.is_absolute()
        || !path.is_absolute()
        || path.parent() != Some(base)
        || path.as_os_str().len() > 4096
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(Issue::InvalidPath);
    }
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or(Issue::InvalidPath)?;
    let id = [ScratchKind::ClaudeProbe, ScratchKind::ClaudeTuiTrial]
        .into_iter()
        .find_map(|kind| name.strip_prefix(kind.prefix()))
        .ok_or(Issue::InvalidPath)?;
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| Issue::InvalidPath)?;
    if parsed.get_version_num() != 4 || parsed.hyphenated().to_string() != id {
        return Err(Issue::InvalidPath);
    }
    Ok(())
}
fn inspect(file: &File) -> Result<(Identity, bool, u64), Issue> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io_issue(unsafe { GetLastError() }));
    }
    let directory = info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (!directory && info.nNumberOfLinks != 1)
    {
        return Err(Issue::UnsafeEntry);
    }
    let identity = Identity {
        volume: info.dwVolumeSerialNumber,
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    };
    if identity.volume == 0 || identity.index == 0 {
        return Err(Issue::IdentityChanged);
    }
    Ok((
        identity,
        directory,
        (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
    ))
}
fn open_base(base: &StorageLocation) -> Result<File, Issue> {
    base.verify_private_directory()
        .map_err(|_| Issue::PrivateRoot)?;
    let file = OpenOptions::new()
        .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES | LIST | SYNC)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(base.directory())
        .map_err(|error| io_issue(error.raw_os_error().unwrap_or(0) as u32))?;
    if !inspect(&file)?.1 {
        return Err(Issue::PrivateRoot);
    }
    Ok(file)
}
fn relative(
    parent: &File,
    name: &OsStr,
    create: bool,
    descriptor: Option<&Descriptor>,
) -> Result<File, Issue> {
    let mut name: Vec<u16> = name.encode_wide().collect();
    if name.is_empty()
        || name.len() > 512
        || name.iter().any(|value| matches!(*value, 0 | 47 | 92 | 58))
    {
        return Err(Issue::InvalidPath);
    }
    let name = UNICODE_STRING {
        Length: (name.len() * 2) as u16,
        MaximumLength: (name.len() * 2) as u16,
        Buffer: name.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &name,
        Attributes: 0x1040, // OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE, including ancestors.
        SecurityDescriptor: descriptor.map_or(null_mut(), |value| value.0.cast()),
        SecurityQualityOfService: null(),
    };
    let mut handle = null_mut();
    let mut status_block = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_READ_ATTRIBUTES | LIST | SYNC | if create { READ_CONTROL } else { DELETE },
            &attributes,
            &mut status_block,
            null(),
            if create { FILE_ATTRIBUTE_DIRECTORY } else { 0 },
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            if create { FILE_CREATE } else { FILE_OPEN },
            FILE_SYNCHRONOUS_IO_NONALERT
                | FILE_OPEN_REPARSE_POINT
                | if create { FILE_DIRECTORY_FILE } else { 0 },
            null(),
            0,
        )
    };
    if status < 0 {
        return Err(io_issue(unsafe { RtlNtStatusToDosError(status) }));
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}
pub fn create(base: &StorageLocation, kind: ScratchKind) -> Result<CreatedScratch, Issue> {
    let parent = open_base(base)?;
    let base_id = inspect(&parent)?.0;
    let path = base
        .directory()
        .join(format!("{}{}", kind.prefix(), uuid::Uuid::new_v4()));
    shape(base.directory(), &path)?;
    let mut raw = null_mut();
    let status = unsafe {
        GetSecurityInfo(
            parent.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut raw,
        )
    };
    if status != 0 {
        return Err(Issue::PrivateRoot);
    }
    let descriptor = Descriptor(raw);
    // C01 has verified the parent's protected/inheritable owner-only ACL. FILE_CREATE
    // atomically creates this new directory with that descriptor; never open-existing.
    let file = relative(
        &parent,
        path.file_name().ok_or(Issue::InvalidPath)?,
        true,
        Some(&descriptor),
    )?;
    let (root_id, directory, _) = inspect(&file)?;
    if !directory || root_id.volume != base_id.volume {
        return Err(Issue::IdentityChanged);
    }
    let location = base
        .resolve_related_directory(&path)
        .map_err(|_| Issue::PrivateRoot)?;
    location
        .prepare_private_directory()
        .map_err(|_| Issue::PrivateRoot)?;
    Ok(CreatedScratch {
        location,
        receipt: OwnedScratch {
            base: base.directory().to_owned(),
            path,
            base_id,
            root_id,
            pin: Some(file),
            attempts: 0,
            removed: false,
        },
    })
}
struct Node {
    file: Option<File>,
    id: Identity,
    directory: bool,
    bytes: u64,
    depth: u8,
}
fn no_named_streams(file: &File, expected: u64) -> Result<(), Issue> {
    let mut page = [0u64; 2048];
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileStreamInfo,
            page.as_mut_ptr().cast(),
            size_of_val(&page) as u32,
        )
    } == 0
    {
        return Err(Issue::UnsafeEntry);
    }
    let info = unsafe { std::ptr::read_unaligned(page.as_ptr().cast::<FILE_STREAM_INFO>()) };
    let start = offset_of!(FILE_STREAM_INFO, StreamName);
    let length = info.StreamNameLength as usize;
    if info.NextEntryOffset != 0
        || length != 14
        || start + length > size_of_val(&page)
        || info.StreamSize < 0
        || info.StreamSize as u64 != expected
    {
        return Err(Issue::UnsafeEntry);
    }
    let name = unsafe {
        std::slice::from_raw_parts(
            page.as_ptr().cast::<u8>().add(start).cast::<u16>(),
            length / 2,
        )
    };
    if OsString::from_wide(name) != OsStr::new("::$DATA") {
        return Err(Issue::UnsafeEntry);
    }
    Ok(())
}
fn collect(nodes: &mut Vec<Node>, total: &mut u64, at: Instant) -> Result<(), Issue> {
    let mut index = 0;
    while index < nodes.len() {
        bounded(at)?;
        if !nodes[index].directory {
            index += 1;
            continue;
        }
        let mut restart = true;
        let mut completed = false;
        for _ in 0..=MAX_ENTRIES {
            bounded(at)?;
            let mut page = [0u64; 2048];
            let class = if restart {
                FileIdBothDirectoryRestartInfo
            } else {
                FileIdBothDirectoryInfo
            };
            restart = false;
            if unsafe {
                GetFileInformationByHandleEx(
                    nodes[index].file.as_ref().unwrap().as_raw_handle(),
                    class,
                    page.as_mut_ptr().cast(),
                    size_of_val(&page) as u32,
                )
            } == 0
            {
                if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
                    completed = true;
                    break;
                }
                return Err(Issue::Io);
            }
            let mut offset = 0;
            loop {
                let header = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
                if offset + size_of::<FILE_ID_BOTH_DIR_INFO>() > size_of_val(&page) {
                    return Err(Issue::UnsafeEntry);
                }
                let pointer = unsafe { page.as_ptr().cast::<u8>().add(offset) };
                let entry =
                    unsafe { std::ptr::read_unaligned(pointer.cast::<FILE_ID_BOTH_DIR_INFO>()) };
                let bytes = entry.FileNameLength as usize;
                if bytes == 0
                    || bytes % 2 != 0
                    || bytes > 1024
                    || offset + header + bytes > size_of_val(&page)
                {
                    return Err(Issue::UnsafeEntry);
                }
                let name = OsString::from_wide(unsafe {
                    std::slice::from_raw_parts(pointer.add(header).cast::<u16>(), bytes / 2)
                });
                if name != OsStr::new(".") && name != OsStr::new("..") {
                    if nodes.len() >= MAX_ENTRIES || nodes[index].depth >= MAX_DEPTH {
                        return Err(Issue::BoundExceeded);
                    }
                    if entry.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        return Err(Issue::UnsafeEntry);
                    }
                    let file = relative(nodes[index].file.as_ref().unwrap(), &name, false, None)?;
                    let (id, directory, length) = inspect(&file)?;
                    if id.volume != nodes[0].id.volume || id.index != entry.FileId as u64 {
                        return Err(Issue::IdentityChanged);
                    }
                    if !directory {
                        no_named_streams(&file, length)?;
                        *total = total.checked_add(length).ok_or(Issue::BoundExceeded)?;
                        if *total > MAX_BYTES {
                            return Err(Issue::BoundExceeded);
                        }
                    }
                    nodes.push(Node {
                        file: Some(file),
                        id,
                        directory,
                        bytes: length,
                        depth: nodes[index].depth + 1,
                    });
                }
                if entry.NextEntryOffset == 0 {
                    break;
                }
                let next = entry.NextEntryOffset as usize;
                if next < header + bytes || offset + next >= size_of_val(&page) || next % 8 != 0 {
                    return Err(Issue::UnsafeEntry);
                }
                offset += next;
            }
        }
        if !completed {
            return Err(Issue::BoundExceeded);
        }
        index += 1;
    }
    // Finish all validation before marking ANY file for deletion.
    for node in nodes.iter() {
        bounded(at)?;
        let (id, directory, bytes) = inspect(node.file.as_ref().unwrap())?;
        if id != node.id || directory != node.directory || (!directory && bytes != node.bytes) {
            return Err(Issue::IdentityChanged);
        }
        if !directory {
            no_named_streams(node.file.as_ref().unwrap(), bytes)?;
        }
    }
    Ok(())
}
/// Caller is trusted Rust: pass the Released status of THIS Check, not another
/// job, after dropping its prepared probe/canary/home/location pins. An uncertain
/// native cleanup never authorizes even a filesystem traversal. Three attempts
/// maximum; a failure preserves the root and reports any partial owned deletion.
pub fn cleanup(
    base: &StorageLocation,
    receipt: &mut OwnedScratch,
    native: CleanupStatus,
) -> CleanupReport {
    let mut report = CleanupReport {
        disposition: Disposition::Retained,
        issue: None,
        scanned_entries: 0,
        removed_entries: 0,
        bytes: 0,
        attempts_remaining: MAX_ATTEMPTS.saturating_sub(receipt.attempts),
    };
    if !matches!(native, CleanupStatus::Released { .. }) {
        report.issue = Some(Issue::Unreleased);
        return report;
    }
    if receipt.removed {
        report.disposition = Disposition::Removed;
        return report;
    }
    if receipt.attempts >= MAX_ATTEMPTS {
        report.issue = Some(Issue::RetryLimit);
        return report;
    }
    receipt.attempts += 1;
    report.attempts_remaining = MAX_ATTEMPTS - receipt.attempts;
    let result = (|| -> Result<(), Issue> {
        shape(base.directory(), &receipt.path)?;
        if base.directory() != receipt.base {
            return Err(Issue::InvalidPath);
        }
        let at = Instant::now();
        let parent = open_base(base)?;
        if inspect(&parent)?.0 != receipt.base_id {
            return Err(Issue::IdentityChanged);
        }
        // Release only our own name pin; any still-live C01 caller pin makes the
        // DELETE open fail with sharing violation. Never clear/repair its ACL.
        receipt.pin.take();
        let file = relative(
            &parent,
            receipt.path.file_name().ok_or(Issue::InvalidPath)?,
            false,
            None,
        )?;
        let (id, directory, bytes) = inspect(&file)?;
        if id != receipt.root_id || !directory {
            return Err(Issue::IdentityChanged);
        }
        let mut nodes = Vec::with_capacity(MAX_ENTRIES);
        nodes.push(Node {
            file: Some(file),
            id,
            directory,
            bytes,
            depth: 0,
        });
        let preflight = collect(&mut nodes, &mut report.bytes, at);
        report.scanned_entries = nodes.len();
        preflight?;
        for node in nodes.iter_mut().rev() {
            bounded(at)?;
            let file = node.file.as_ref().unwrap();
            let (id, directory, bytes) = inspect(file)?;
            if id != node.id || directory != node.directory || (!directory && bytes != node.bytes) {
                return Err(Issue::IdentityChanged);
            }
            if !directory {
                no_named_streams(file, bytes)?;
            }
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            if unsafe {
                SetFileInformationByHandle(
                    file.as_raw_handle(),
                    FileDispositionInfo,
                    (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            } == 0
            {
                return Err(io_issue(unsafe { GetLastError() }));
            }
            node.file.take();
            report.removed_entries += 1; // close before deleting parent.
        }
        receipt.removed = true;
        Ok(())
    })();
    match result {
        Ok(()) => report.disposition = Disposition::Removed,
        Err(issue) => report.issue = Some(issue),
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_state::location::LocationRequest;
    use std::fs;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(
                std::env::temp_dir()
                    .join(format!("shinatama-check-scratch-{}", uuid::Uuid::new_v4())),
            )
        }
        fn base(&self) -> StorageLocation {
            let base = StorageLocation::resolve(LocationRequest {
                local_data_directory: self.0.clone(),
                home_override: Some(self.0.join("private")),
                project_roots: vec![],
                synchronized_roots: vec![],
            })
            .unwrap();
            base.prepare_private_directory().unwrap();
            base
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let parent = std::env::temp_dir();
            if self.0.parent() == Some(parent.as_path())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("shinatama-check-scratch-")
            {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }
    #[test]
    fn scratch_is_preserved_until_native_release_and_all_owner_pins_are_dropped() {
        let fixture = Fixture::new();
        let base = fixture.base();
        let (location, mut receipt) = create(&base, ScratchKind::ClaudeProbe)
            .unwrap()
            .into_parts();
        let file = location.directory().join("synthetic.txt");
        fs::write(&file, b"owned check").unwrap();
        let owner_pin = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES | LIST)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(location.directory())
            .unwrap();
        let report = cleanup(&base, &mut receipt, CleanupStatus::Retired);
        assert_eq!(report.issue, Some(Issue::Unreleased));
        assert_eq!(report.attempts_remaining, MAX_ATTEMPTS);
        assert!(file.exists());
        let report = cleanup(
            &base,
            &mut receipt,
            CleanupStatus::Released { ledger_revision: 1 },
        );
        assert_eq!(report.issue, Some(Issue::InUse));
        assert_eq!(fs::read(&file).unwrap(), b"owned check");
        drop(location);
        drop(owner_pin);
        let report = cleanup(
            &base,
            &mut receipt,
            CleanupStatus::Released { ledger_revision: 1 },
        );
        assert_eq!(report.disposition, Disposition::Removed);
        assert_eq!(report.removed_entries, 2);
        assert!(!receipt.path().exists());
        // More than the desktop's eight retained-directory slots can be checked
        // successfully without accumulating scratch. Released here is a local
        // filesystem-test input; it never produces a native launch authority.
        for index in 0..9 {
            let kind = if index % 2 == 0 {
                ScratchKind::ClaudeProbe
            } else {
                ScratchKind::ClaudeTuiTrial
            };
            let (location, mut receipt) = create(&base, kind).unwrap().into_parts();
            fs::create_dir(location.directory().join("nested")).unwrap();
            fs::write(
                location.directory().join("nested/owned.txt"),
                b"check scratch",
            )
            .unwrap();
            drop(location);
            let report = cleanup(
                &base,
                &mut receipt,
                CleanupStatus::Released { ledger_revision: 1 },
            );
            assert_eq!(report.disposition, Disposition::Removed);
            assert!(!receipt.path().exists());
            assert!(
                !fs::read_dir(base.directory())
                    .unwrap()
                    .take(MAX_ENTRIES + 1)
                    .any(|entry| {
                        let name = entry.unwrap().file_name();
                        let name = name.to_string_lossy();
                        name.starts_with("claude-probe-") || name.starts_with("claude-tui-trial-")
                    })
            );
        }
    }
    #[test]
    fn junction_refusal_preserves_entire_scratch_and_external_synthetic_sentinel() {
        use windows_sys::Win32::System::IO::DeviceIoControl;
        let fixture = Fixture::new();
        let base = fixture.base();
        let (location, mut receipt) = create(&base, ScratchKind::ClaudeTuiTrial)
            .unwrap()
            .into_parts();
        let outside = fixture.0.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"outside owned fixture").unwrap();
        fs::write(location.directory().join("keep"), b"preserve complete tree").unwrap();
        let link = location.directory().join("junction");
        fs::create_dir(&link).unwrap();
        // Local NTFS mount-point reparse data, no shell or symlink privilege.
        let target: Vec<u16> = format!("\\??\\{}", outside.display())
            .encode_utf16()
            .collect();
        let printable: Vec<u16> = outside.as_os_str().encode_wide().collect();
        let mut data = Vec::new();
        data.extend_from_slice(&0xa0000003u32.to_le_bytes());
        data.extend_from_slice(
            &((8 + (target.len() + printable.len() + 2) * 2) as u16).to_le_bytes(),
        );
        data.extend_from_slice(&0u16.to_le_bytes());
        for value in [
            0u16,
            (target.len() * 2) as u16,
            ((target.len() + 1) * 2) as u16,
            (printable.len() * 2) as u16,
        ] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        for value in target
            .into_iter()
            .chain(Some(0))
            .chain(printable)
            .chain(Some(0))
        {
            data.extend_from_slice(&value.to_le_bytes());
        }
        let opened = OpenOptions::new()
            .access_mode(0x4000_0000)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&link)
            .unwrap();
        let mut returned = 0;
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    opened.as_raw_handle(),
                    0x0009_00a4,
                    data.as_ptr().cast(),
                    data.len() as u32,
                    null_mut(),
                    0,
                    &mut returned,
                    null_mut(),
                )
            },
            0
        );
        drop(opened);
        drop(location);
        let report = cleanup(
            &base,
            &mut receipt,
            CleanupStatus::Released { ledger_revision: 2 },
        );
        assert_eq!(report.disposition, Disposition::Retained);
        assert_eq!(report.issue, Some(Issue::UnsafeEntry));
        assert_eq!(report.removed_entries, 0);
        assert_eq!(
            fs::read(outside.join("sentinel")).unwrap(),
            b"outside owned fixture"
        );
        assert_eq!(
            fs::read(receipt.path().join("keep")).unwrap(),
            b"preserve complete tree"
        );
        fs::remove_dir(link).unwrap();
    }
}
