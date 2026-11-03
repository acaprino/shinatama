//! Shared held-file/path validation for harness executables and the command bridge.
//! Hardlinks are accepted only for trusted helpers, whose bytes and selected name stay
//! locked against writes/deletion through every alias for the invocation.
#![cfg(windows)]
use crate::native_launch::NativeProgramIdentity;
use sha2::{Digest, Sha256};
use shinatama_state::location::StorageLocation;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    time::Instant,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
};
const MIB: u64 = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutablePinError {
    InvalidCandidate,
    PrivateHome,
    UnsafeProgram,
    ProgramChanged,
    Expired,
    Identity,
}
pub(crate) struct PinnedExecutable {
    pub(crate) path: PathBuf,
    pub(crate) file: File,
    pub(crate) ancestors: Vec<File>,
    pub(crate) identity: NativeProgramIdentity,
    pub(crate) hash: String,
    pub(crate) bytes: u64,
}

pub(crate) fn pin_session_hook(
    path: &Path,
    deadline: Instant,
) -> Result<PinnedExecutable, ExecutablePinError> {
    pin_executable(path, "shinatama-hook.exe", deadline, true)
}
fn file_info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, ExecutablePinError> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(ExecutablePinError::UnsafeProgram);
    }
    Ok(info)
}
fn pin_file(path: &Path, allow_hardlinks: bool) -> Result<File, ExecutablePinError> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| ExecutablePinError::UnsafeProgram)?;
    let info = file_info(&file)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks == 0
        || (!allow_hardlinks && info.nNumberOfLinks != 1)
    {
        return Err(ExecutablePinError::UnsafeProgram);
    }
    Ok(file)
}
fn pin_directory(path: &Path) -> Result<(File, Vec<File>), ExecutablePinError> {
    StorageLocation::preflight(path).map_err(|_| ExecutablePinError::PrivateHome)?;
    let parents: Vec<_> = path.ancestors().take(65).collect();
    if parents.len() > 64 {
        return Err(ExecutablePinError::PrivateHome);
    }
    let mut pins = Vec::new();
    for parent in parents.into_iter().rev() {
        let file = OpenOptions::new()
            .read(true)
            // Participate in sharing checks without enumerating any contents.
            .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(parent)
            .map_err(|_| ExecutablePinError::PrivateHome)?;
        let info = file_info(&file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(ExecutablePinError::PrivateHome);
        }
        pins.push(file);
    }
    let root = pins.pop().ok_or(ExecutablePinError::PrivateHome)?;
    Ok((root, pins))
}
pub(crate) fn pin_executable(
    path: &Path,
    name: &str,
    deadline: Instant,
    allow_hardlinks: bool,
) -> Result<PinnedExecutable, ExecutablePinError> {
    if !path
        .file_name()
        .is_some_and(|filename| filename.eq_ignore_ascii_case(name))
    {
        return Err(ExecutablePinError::InvalidCandidate);
    }
    let (_parent, mut ancestors) =
        pin_directory(path.parent().ok_or(ExecutablePinError::InvalidCandidate)?)?;
    ancestors.push(_parent);
    let mut file = pin_file(path, allow_hardlinks)?;
    let before = file_info(&file)?;
    let bytes = (u64::from(before.nFileSizeHigh) << 32) | u64::from(before.nFileSizeLow);
    if !(64..=512 * MIB).contains(&bytes) {
        return Err(ExecutablePinError::UnsafeProgram);
    }
    let mut dos = [0; 64];
    file.read_exact(&mut dos)
        .map_err(|_| ExecutablePinError::UnsafeProgram)?;
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if &dos[..2] != b"MZ" || offset > MIB || offset + 4 > bytes {
        return Err(ExecutablePinError::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| ExecutablePinError::UnsafeProgram)?;
    let mut pe = [0; 4];
    file.read_exact(&mut pe)
        .map_err(|_| ExecutablePinError::UnsafeProgram)?;
    if pe != *b"PE\0\0" {
        return Err(ExecutablePinError::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ExecutablePinError::UnsafeProgram)?;
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0; 65536];
    loop {
        if Instant::now() >= deadline {
            return Err(ExecutablePinError::Expired);
        }
        let count = file
            .read(&mut buffer)
            .map_err(|_| ExecutablePinError::UnsafeProgram)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > bytes {
            return Err(ExecutablePinError::ProgramChanged);
        }
        hash.update(&buffer[..count]);
    }
    let digest: [u8; 32] = hash.finalize().into();
    let after = file_info(&file)?;
    if read != bytes
        || before.dwFileAttributes != after.dwFileAttributes
        || before.dwVolumeSerialNumber != after.dwVolumeSerialNumber
        || before.nFileIndexHigh != after.nFileIndexHigh
        || before.nFileIndexLow != after.nFileIndexLow
        || before.nFileSizeHigh != after.nFileSizeHigh
        || before.nFileSizeLow != after.nFileSizeLow
        || before.nNumberOfLinks != after.nNumberOfLinks
        || before.ftLastWriteTime.dwLowDateTime != after.ftLastWriteTime.dwLowDateTime
        || before.ftLastWriteTime.dwHighDateTime != after.ftLastWriteTime.dwHighDateTime
    {
        return Err(ExecutablePinError::ProgramChanged);
    }
    let identity = NativeProgramIdentity::new(
        before.dwVolumeSerialNumber,
        (u64::from(before.nFileIndexHigh) << 32) | u64::from(before.nFileIndexLow),
        bytes,
        (u64::from(before.ftLastWriteTime.dwHighDateTime) << 32)
            | u64::from(before.ftLastWriteTime.dwLowDateTime),
        digest,
    )
    .map_err(|_| ExecutablePinError::Identity)?;
    Ok(PinnedExecutable {
        path: path
            .canonicalize()
            .map_err(|_| ExecutablePinError::UnsafeProgram)?,
        file,
        ancestors,
        identity,
        hash: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        bytes,
    })
}
