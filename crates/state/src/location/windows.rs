use super::{LocationError, LocationErrorKind, Result};
use std::ffi::{OsStr, OsString, c_void};
use std::fs;
use std::io::{self, Read};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SE_FILE_OBJECT, SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetSecurityDescriptorControl,
    GetSecurityDescriptorDacl, GetTokenInformation, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED,
    SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FileStandardInfo,
    GetDriveTypeW, GetFileAttributesW, GetFileInformationByHandle, GetFileInformationByHandleEx,
    GetVolumeInformationW, GetVolumePathNameW, INVALID_FILE_ATTRIBUTES, QueryDosDeviceW,
};
use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey,
    RegEnumKeyExW, RegEnumValueW, RegGetValueW, RegOpenKeyExW,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

// Windows ordinal comparison matches filesystem case rules without lossy UTF-8 conversion.
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

pub(super) fn same_name(left: &OsStr, right: &OsStr) -> bool {
    let left: Vec<u16> = left.encode_wide().collect();
    let right: Vec<u16> = right.encode_wide().collect();
    if left.len() > i32::MAX as usize || right.len() > i32::MAX as usize {
        return false;
    }
    unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        ) == 2
    }
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

fn from_wide(value: &[u16]) -> OsString {
    OsString::from_wide(&value[..value.iter().position(|n| *n == 0).unwrap_or(value.len())])
}

fn failed(path: &Path, kind: LocationErrorKind, action: &str) -> LocationError {
    LocationError::new(
        kind,
        path,
        format!("{action}: {}.", io::Error::last_os_error()),
    )
}

pub(super) fn preflight_drive(path: &Path) -> Result<()> {
    use std::path::{Component, Prefix};
    let letter = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter,
            _ => {
                return Err(LocationError::new(
                    LocationErrorKind::Network,
                    path,
                    "Serve un'unità locale, non un percorso di rete o dispositivo.",
                ));
            }
        },
        _ => {
            return Err(LocationError::new(
                LocationErrorKind::InvalidPath,
                path,
                "Serve un percorso locale assoluto.",
            ));
        }
    };
    let root = [letter as u16, b':' as u16, b'\\' as u16, 0];
    // GetDriveType reads the drive mapping before GetFileAttributes/canonicalize can
    // contact an offline SMB server. No file or directory is opened by this check.
    match unsafe { GetDriveTypeW(root.as_ptr()) } {
        4 => Err(LocationError::new(
            LocationErrorKind::Network,
            path,
            "L'unità è una cartella di rete.",
        )),
        2 | 3 | 6 => Ok(()),
        _ => Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "L'unità locale non è disponibile.",
        )),
    }
}

pub(super) fn validate_volume(path: &Path) -> Result<()> {
    let raw = wide(path);
    let mut volume = vec![0u16; 32768];
    // Existing ancestors are used: GetVolumePathName does not need to create a probe file.
    if unsafe { GetVolumePathNameW(raw.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) } == 0 {
        return Err(failed(
            path,
            LocationErrorKind::Io,
            "Impossibile identificare il volume",
        ));
    }
    let drive_type = unsafe { GetDriveTypeW(volume.as_ptr()) };
    if drive_type == 4 {
        return Err(LocationError::new(
            LocationErrorKind::Network,
            path,
            "Il volume è un'unità di rete.",
        ));
    }
    if !matches!(drive_type, 2 | 3 | 6) {
        return Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Il volume non è un disco locale scrivibile.",
        ));
    }
    let mut flags = 0;
    let mut filesystem = vec![0u16; 256];
    if unsafe {
        GetVolumeInformationW(
            volume.as_ptr(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } == 0
    {
        return Err(failed(
            path,
            LocationErrorKind::Io,
            "Impossibile verificare il filesystem",
        ));
    }
    let fs_name = from_wide(&filesystem)
        .to_string_lossy()
        .to_ascii_lowercase();
    let volume_path = from_wide(&volume);
    let drive = volume_path.to_string_lossy();
    let drive = drive.trim_start_matches(r"\\?\");
    let device = if drive.as_bytes().get(1) == Some(&b':') {
        let drive_name = wide(&drive[..2]);
        let mut result = vec![0u16; 32768];
        let count = unsafe {
            QueryDosDeviceW(
                drive_name.as_ptr(),
                result.as_mut_ptr(),
                result.len() as u32,
            )
        };
        if count == 0 {
            String::new()
        } else {
            from_wide(&result).to_string_lossy().to_ascii_lowercase()
        }
    } else {
        String::new()
    };
    if device.contains("googledrive")
        || device.contains("drivefs")
        || fs_name.contains("googledrive")
        || fs_name.contains("drivefs")
    {
        return Err(LocationError::new(
            LocationErrorKind::Synchronized,
            path,
            "Il volume appartiene a Google Drive.",
        ));
    }
    if device.contains("lanmanredirector")
        || device.contains(r"\device\mup")
        || device.contains(r"\??\unc\")
    {
        return Err(LocationError::new(
            LocationErrorKind::Network,
            path,
            "Il dispositivo usa un percorso di rete.",
        ));
    }
    // FILE_PERSISTENT_ACLS. SQLite data must not be written before privacy is enforceable.
    if flags & 8 == 0 {
        return Err(LocationError::new(
            LocationErrorKind::UnsupportedVolume,
            path,
            "Il filesystem non supporta permessi privati per utente.",
        ));
    }
    for ancestor in path.ancestors() {
        let attributes = unsafe { GetFileAttributesW(wide(ancestor).as_ptr()) };
        if attributes == INVALID_FILE_ATTRIBUTES {
            return Err(failed(
                ancestor,
                LocationErrorKind::Io,
                "Impossibile verificare la cartella",
            ));
        }
        if attributes
            & (FILE_ATTRIBUTE_OFFLINE
                | FILE_ATTRIBUTE_RECALL_ON_OPEN
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
            != 0
        {
            return Err(LocationError::new(
                LocationErrorKind::Synchronized,
                path,
                "La cartella usa archiviazione cloud o offline.",
            ));
        }
    }
    Ok(())
}

struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct CurrentUser {
    // u64 storage provides the alignment required by TOKEN_USER and its embedded SID.
    data: Vec<u64>,
}
impl CurrentUser {
    fn load(path: &Path) -> Result<Self> {
        let mut handle = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
            return Err(failed(
                path,
                LocationErrorKind::Permissions,
                "Impossibile identificare l'utente",
            ));
        }
        let token = Token(handle);
        let mut length = 0;
        unsafe {
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut length);
        }
        if length == 0 || length > 65536 {
            return Err(failed(
                path,
                LocationErrorKind::Permissions,
                "Identità utente non valida",
            ));
        }
        let mut data = vec![0u64; (length as usize).div_ceil(8)];
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                data.as_mut_ptr().cast(),
                (data.len() * 8) as u32,
                &mut length,
            )
        } == 0
        {
            return Err(failed(
                path,
                LocationErrorKind::Permissions,
                "Impossibile leggere l'identità utente",
            ));
        }
        Ok(Self { data })
    }

    fn sid(&self) -> PSID {
        unsafe { (*(self.data.as_ptr().cast::<TOKEN_USER>())).User.Sid }
    }

    fn descriptor(&self, path: &Path) -> Result<LocalAllocation> {
        let mut text = null_mut();
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut text) } == 0 {
            return Err(failed(
                path,
                LocationErrorKind::Permissions,
                "Identità utente non convertibile",
            ));
        }
        let allocation = LocalAllocation(text.cast());
        let mut length = 0;
        while length < 256 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        if length == 256 {
            return Err(LocationError::new(
                LocationErrorKind::Permissions,
                path,
                "Identità utente troppo lunga.",
            ));
        }
        let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) });
        let sddl = wide(format!("O:{sid}D:P(A;OICI;FA;;;{sid})"));
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(failed(
                path,
                LocationErrorKind::Permissions,
                "Impossibile preparare i permessi privati",
            ));
        }
        drop(allocation);
        Ok(LocalAllocation(descriptor))
    }
}

fn security_info(
    path: &Path,
    handle: HANDLE,
) -> Result<(
    LocalAllocation,
    PSID,
    *mut windows_sys::Win32::Security::ACL,
)> {
    let mut descriptor = null_mut();
    let mut owner = null_mut();
    let mut acl = null_mut();
    let code = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut descriptor,
        )
    };
    if code != ERROR_SUCCESS {
        return Err(LocationError::new(
            LocationErrorKind::Permissions,
            path,
            format!(
                "Impossibile ispezionare i permessi: {}.",
                io::Error::from_raw_os_error(code as i32)
            ),
        ));
    }
    Ok((LocalAllocation(descriptor), owner, acl))
}

pub(super) fn prepare_private_directory(
    path: &Path,
    validate_locked: impl FnOnce() -> Result<()>,
) -> Result<Vec<fs::File>> {
    let user = CurrentUser::load(path)?;
    let descriptor = user.descriptor(path)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let mut missing = Vec::new();
    let mut ancestor = path;
    while !ancestor.exists() {
        missing.push(ancestor.to_path_buf());
        ancestor = ancestor.parent().ok_or_else(|| {
            LocationError::new(
                LocationErrorKind::InvalidPath,
                path,
                "Cartella principale non disponibile.",
            )
        })?;
    }
    // Deny delete-sharing on every resolved ancestor, so a different user cannot replace
    // a writable parent with a junction between validation and CreateDirectoryW.
    let mut guards = Vec::new();
    for directory in ancestor.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if directory == path {
            continue;
        }
        let guard = open_object(directory, FILE_READ_ATTRIBUTES)?;
        inspect_object(directory, &guard, true)?;
        guards.push(guard);
    }
    validate_locked()?;
    for directory in missing.iter().rev() {
        if unsafe { CreateDirectoryW(wide(directory).as_ptr(), &attributes) } == 0 {
            let error = io::Error::last_os_error();
            // A concurrent process may have created it; validate its owner before changing ACLs.
            if error.raw_os_error() != Some(183) {
                return Err(LocationError::io(directory, error));
            }
        }
        let guard = open_object(directory, FILE_READ_ATTRIBUTES)?;
        inspect_object(directory, &guard, true)?;
        guards.push(guard);
        verify_private_directory(directory)?;
    }
    private_object(path, true, true)?;
    let guard = open_object(path, FILE_READ_ATTRIBUTES)?;
    inspect_object(path, &guard, true)?;
    guards.push(guard);
    Ok(guards)
}

pub(super) fn verify_private_directory(path: &Path) -> Result<()> {
    private_object(path, true, false)
}

fn verify_acl(
    path: &Path,
    user: &CurrentUser,
    descriptor: &LocalAllocation,
    owner: PSID,
    acl: *mut windows_sys::Win32::Security::ACL,
) -> Result<()> {
    let invalid = || {
        LocationError::new(
            LocationErrorKind::Permissions,
            path,
            "La cartella deve consentire accesso soltanto all'utente corrente.",
        )
    };
    if owner.is_null() || acl.is_null() || unsafe { EqualSid(owner, user.sid()) } == 0 {
        return Err(invalid());
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) } == 0
        || control & SE_DACL_PROTECTED == 0
    {
        return Err(invalid());
    }
    if unsafe { (*acl).AceCount } != 1 {
        return Err(invalid());
    }
    let mut ace = null_mut();
    if unsafe { GetAce(acl, 0, &mut ace) } == 0 || ace.is_null() {
        return Err(invalid());
    }
    let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
    // ACCESS_ALLOWED_ACE_TYPE is zero. Reject inherited/foreign or partial grants.
    let sid = (&allowed.SidStart as *const u32).cast_mut().cast();
    if allowed.Header.AceType != 0
        || allowed.Mask & FILE_ALL_ACCESS != FILE_ALL_ACCESS
        || unsafe { EqualSid(sid, user.sid()) } == 0
    {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn private_file(path: &Path, change: bool) -> Result<()> {
    private_object(path, false, change)
}

fn open_object(path: &Path, access: u32) -> Result<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .access_mode(access)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map_err(|error| LocationError::io(path, error))
}

fn inspect_object(
    path: &Path,
    file: &fs::File,
    directory: bool,
) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(failed(
            path,
            LocationErrorKind::Permissions,
            "Impossibile ispezionare il file",
        ));
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
    {
        return Err(LocationError::new(
            LocationErrorKind::InvalidPath,
            path,
            "Il percorso deve essere regolare, senza collegamenti simbolici o hard link.",
        ));
    }
    if !directory && info.nNumberOfLinks != 1 {
        if info.nNumberOfLinks == 0 {
            // A last SQLite connection can unlink its sidecar after our open but
            // before this inspection. Verify deletion on that same handle; zero
            // links alone never exempts an object from the hard-link guard.
            let mut standard: FILE_STANDARD_INFO = unsafe { std::mem::zeroed() };
            if unsafe {
                GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    FileStandardInfo,
                    (&mut standard as *mut FILE_STANDARD_INFO).cast(),
                    size_of::<FILE_STANDARD_INFO>() as u32,
                )
            } == 0
            {
                return Err(failed(
                    path,
                    LocationErrorKind::Permissions,
                    "Impossibile verificare la cancellazione del file",
                ));
            }
            if standard.NumberOfLinks == 0 && standard.DeletePending {
                return Err(LocationError::io(
                    path,
                    io::Error::new(io::ErrorKind::NotFound, "Il file è in cancellazione."),
                ));
            }
        }
        return Err(LocationError::new(
            LocationErrorKind::InvalidPath,
            path,
            "Il file deve avere un solo collegamento, senza hard link.",
        ));
    }
    Ok(info)
}

fn private_object(path: &Path, directory: bool, change: bool) -> Result<()> {
    let user = CurrentUser::load(path)?;
    // READ_CONTROL; open the reparse object itself, never its target.
    let file = open_object(path, 0x0002_0000 | FILE_READ_ATTRIBUTES)?;
    let handle = file.as_raw_handle();
    let before = inspect_object(path, &file, directory)?;
    let (existing, owner, acl) = security_info(path, handle)?;
    if owner.is_null() || unsafe { EqualSid(owner, user.sid()) } == 0 {
        return Err(LocationError::new(
            LocationErrorKind::Permissions,
            path,
            "Il file appartiene a un altro utente; i permessi non sono stati modificati.",
        ));
    }
    let verified = verify_acl(path, &user, &existing, owner, acl);
    if !change || verified.is_ok() {
        return verified;
    }
    drop(existing);
    drop(file);
    // Documented SetSecurityInfo behavior: MAXIMUM_ALLOWED prevents recursive ACE
    // propagation to existing children. New files still inherit this directory's ACL.
    // Never alter an existing child (including junction targets) as a side effect.
    let access = if directory {
        0x0200_0000
    } else {
        0x0002_0000 | 0x0004_0000 | FILE_READ_ATTRIBUTES
    };
    let file = open_object(path, access)?;
    let handle = file.as_raw_handle();
    let current = inspect_object(path, &file, directory)?;
    if before.dwVolumeSerialNumber != current.dwVolumeSerialNumber
        || before.nFileIndexHigh != current.nFileIndexHigh
        || before.nFileIndexLow != current.nFileIndexLow
    {
        return Err(LocationError::new(
            LocationErrorKind::InvalidPath,
            path,
            "Il percorso è cambiato durante l'apertura.",
        ));
    }
    let (existing, owner, _) = security_info(path, handle)?;
    if owner.is_null() || unsafe { EqualSid(owner, user.sid()) } == 0 {
        return Err(LocationError::new(
            LocationErrorKind::Permissions,
            path,
            "La cartella appartiene a un altro utente; i permessi non sono stati modificati.",
        ));
    }
    let private = user.descriptor(path)?;
    let mut present = 0;
    let mut defaulted = 0;
    let mut private_acl = null_mut();
    if unsafe {
        GetSecurityDescriptorDacl(private.0, &mut present, &mut private_acl, &mut defaulted)
    } == 0
        || present == 0
        || private_acl.is_null()
    {
        return Err(failed(
            path,
            LocationErrorKind::Permissions,
            "Permessi privati non validi",
        ));
    }
    let code = unsafe {
        SetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            private_acl,
            null(),
        )
    };
    if code != ERROR_SUCCESS {
        return Err(LocationError::new(
            LocationErrorKind::Permissions,
            path,
            format!(
                "Impossibile applicare permessi privati: {}.",
                io::Error::from_raw_os_error(code as i32)
            ),
        ));
    }
    drop(existing);
    let (updated, owner, acl) = security_info(path, handle)?;
    verify_acl(path, &user, &updated, owner, acl)?;
    let named = open_object(path, FILE_READ_ATTRIBUTES)?;
    let after = inspect_object(path, &named, directory)?;
    if before.dwVolumeSerialNumber != after.dwVolumeSerialNumber
        || before.nFileIndexHigh != after.nFileIndexHigh
        || before.nFileIndexLow != after.nFileIndexLow
    {
        return Err(LocationError::new(
            LocationErrorKind::InvalidPath,
            path,
            "Il percorso è cambiato durante l'apertura.",
        ));
    }
    Ok(())
}

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

fn open_key(parent: HKEY, name: &str) -> Option<Key> {
    let mut key = null_mut();
    (unsafe { RegOpenKeyExW(parent, wide(name).as_ptr(), 0, KEY_READ, &mut key) } == ERROR_SUCCESS)
        .then_some(Key(key))
}

fn children(key: HKEY) -> Vec<String> {
    let mut names = Vec::new();
    for index in 0..256 {
        let mut name = [0u16; 256];
        let mut length = name.len() as u32;
        let code = unsafe {
            RegEnumKeyExW(
                key,
                index,
                name.as_mut_ptr(),
                &mut length,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if code != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&name[..length as usize]));
    }
    names
}

fn string_value(key: HKEY, value: &str) -> Option<PathBuf> {
    let mut data = vec![0u16; 32768];
    let mut size = (data.len() * 2) as u32;
    if unsafe {
        RegGetValueW(
            key,
            null(),
            wide(value).as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            data.as_mut_ptr().cast(),
            &mut size,
        )
    } != ERROR_SUCCESS
    {
        return None;
    }
    Some(PathBuf::from(from_wide(
        &data[..(size as usize / 2).min(data.len())],
    )))
}

pub(super) fn synchronized_roots(local_data: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if let Some(accounts) = open_key(HKEY_CURRENT_USER, r"Software\Microsoft\OneDrive\Accounts") {
        for name in children(accounts.0) {
            if let Some(account) = open_key(accounts.0, &name)
                && let Some(root) = string_value(account.0, "UserFolder")
            {
                roots.push(root);
            }
        }
    }
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        if let Some(manager) = open_key(
            hive,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager",
        ) {
            for name in children(manager.0) {
                if let Some(provider) = open_key(manager.0, &format!(r"{name}\UserSyncRoots")) {
                    for index in 0..256 {
                        let mut name = [0u16; 256];
                        let mut length = name.len() as u32;
                        if unsafe {
                            RegEnumValueW(
                                provider.0,
                                index,
                                name.as_mut_ptr(),
                                &mut length,
                                null(),
                                null_mut(),
                                null_mut(),
                                null_mut(),
                            )
                        } != ERROR_SUCCESS
                        {
                            break;
                        }
                        if let Some(root) = string_value(
                            provider.0,
                            &String::from_utf16_lossy(&name[..length as usize]),
                        ) {
                            roots.push(root);
                        }
                    }
                }
            }
        }
    }
    // Official DriveFS configuration, including policy overrides and account-specific mounts.
    // The FAT virtual volume is independently refused if it has no persistent Windows ACLs.
    for (hive, key_path) in [
        (HKEY_CURRENT_USER, r"Software\Google\DriveFS"),
        (HKEY_LOCAL_MACHINE, r"Software\Google\DriveFS"),
        (HKEY_LOCAL_MACHINE, r"Software\Policies\Google\DriveFS"),
    ] {
        if let Some(key) = open_key(hive, key_path) {
            if let Some(path) = string_value(key.0, "DefaultMountPoint")
                && let Some(path) = expand_path(&path)
            {
                roots.push(path);
            }
            for account in children(key.0) {
                if let Some(account) = open_key(key.0, &account) {
                    for name in ["MountPoint", "DefaultMountPoint"] {
                        if let Some(path) = string_value(account.0, name)
                            && let Some(path) = expand_path(&path)
                        {
                            roots.push(path);
                        }
                    }
                }
            }
        }
    }
    // Dropbox's documented local path metadata contains no token. Bound the read; keep no bytes.
    for metadata in [
        local_data.join("Dropbox/info.json"),
        local_data
            .parent()
            .map(|p| p.join("Roaming/Dropbox/info.json"))
            .unwrap_or_default(),
    ] {
        let info = match fs::metadata(&metadata) {
            Ok(info) if info.len() <= 128 * 1024 => info,
            _ => continue,
        };
        if !info.is_file() {
            continue;
        }
        let mut bytes = Vec::new();
        if fs::File::open(&metadata)
            .and_then(|file| file.take(128 * 1024 + 1).read_to_end(&mut bytes))
            .is_ok()
            && bytes.len() <= 128 * 1024
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        {
            for account in ["personal", "business"] {
                if let Some(path) = value
                    .get(account)
                    .and_then(|v| v.get("path"))
                    .and_then(|v| v.as_str())
                {
                    roots.push(PathBuf::from(path));
                }
            }
        }
    }
    Ok(roots)
}

fn expand_path(path: &Path) -> Option<PathBuf> {
    let raw = wide(path);
    let length = unsafe { ExpandEnvironmentStringsW(raw.as_ptr(), null_mut(), 0) };
    if length == 0 || length > 32768 {
        return None;
    }
    let mut expanded = vec![0u16; length as usize];
    if unsafe { ExpandEnvironmentStringsW(raw.as_ptr(), expanded.as_mut_ptr(), length) } != length {
        return None;
    }
    let mut path = from_wide(&expanded);
    let value = path.to_string_lossy();
    if value.len() == 2 && value.as_bytes()[1] == b':' {
        path.push(r"\");
    }
    Some(PathBuf::from(path))
}
