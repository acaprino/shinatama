//! Windows resource metadata only. Never loads or executes the target program.
//! FileVersion is not the CLI's reported version and qualifies no capability.
//! API contract: https://learn.microsoft.com/windows/win32/api/winver/nf-winver-verqueryvaluew
use serde::Serialize;
use std::path::Path;

pub const MAX_VERSION_RESOURCE_BYTES: u32 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PeVersionMetadata {
    NotInspected,
    Unavailable {
        reason: PeVersionUnavailable,
    },
    FileVersion {
        major: u16,
        minor: u16,
        patch: u16,
        revision: u16,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeVersionUnavailable {
    UnsupportedPlatform,
    UnsafePath,
    Wrapper,
    NotLocalFixedDrive,
    ReparsePoint,
    MissingResource,
    ResourceTooLarge,
    InvalidResource,
    ReadFailed,
    InspectionLimit,
}

/// A read-only local-file resource query. It does not consult PATH, HOME, config,
/// credentials, a process, DLL entry points, update logic or CLI hooks.
#[cfg(windows)]
pub fn inspect_pe_version(path: &Path) -> PeVersionMetadata {
    use std::{
        ffi::c_void,
        os::windows::{ffi::OsStrExt, fs::MetadataExt},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO,
        VerQueryValueW,
    };

    let raw = path.to_string_lossy();
    let bytes = raw.as_bytes();
    if bytes.len() < 3
        || raw.len() > 4096
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || raw.chars().any(char::is_control)
        || bytes.iter().skip(2).any(|byte| *byte == b':')
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return unavailable(PeVersionUnavailable::UnsafePath);
    }
    if !path
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
    {
        return unavailable(PeVersionUnavailable::Wrapper);
    }
    let root = [u16::from(bytes[0]), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: root is a NUL-terminated drive root. DRIVE_FIXED is 3. UNC and
    // network/removable drives are deliberately not probed by this provider.
    if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 {
        return unavailable(PeVersionUnavailable::NotLocalFixedDrive);
    }
    // Check from drive root toward the file, so a known parent junction cannot
    // redirect this read to a network share or a different directory tree.
    let ancestors: Vec<_> = path.ancestors().take(65).collect();
    if ancestors.len() > 64 {
        return unavailable(PeVersionUnavailable::UnsafePath);
    }
    for ancestor in ancestors.into_iter().rev() {
        let metadata = match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(_) => return unavailable(PeVersionUnavailable::ReadFailed),
        };
        if metadata.file_attributes() & 0x400 != 0 {
            return unavailable(PeVersionUnavailable::ReparsePoint);
        }
    }
    let mut filename: Vec<_> = path.as_os_str().encode_wide().collect();
    if filename.contains(&0) {
        return unavailable(PeVersionUnavailable::UnsafePath);
    }
    filename.push(0);
    let mut ignored = 0_u32;
    // SAFETY: a valid NUL-terminated path and writable reserved output are passed.
    let size = unsafe { GetFileVersionInfoSizeW(filename.as_ptr(), &mut ignored) };
    if size == 0 {
        return unavailable(PeVersionUnavailable::MissingResource);
    }
    if size > MAX_VERSION_RESOURCE_BYTES {
        return unavailable(PeVersionUnavailable::ResourceTooLarge);
    }
    let mut buffer = vec![0_u8; size as usize];
    // SAFETY: the output allocation has precisely the requested bounded size.
    if unsafe { GetFileVersionInfoW(filename.as_ptr(), 0, size, buffer.as_mut_ptr().cast()) } == 0 {
        return unavailable(PeVersionUnavailable::ReadFailed);
    }
    let root_block = [u16::from(b'\\'), 0];
    let mut value: *mut c_void = std::ptr::null_mut();
    let mut value_size = 0_u32;
    // SAFETY: buffer comes from GetFileVersionInfoW, root_block is NUL-terminated,
    // output pointers are writable, and buffer remains alive during the query.
    if unsafe {
        VerQueryValueW(
            buffer.as_ptr().cast(),
            root_block.as_ptr(),
            &mut value,
            &mut value_size,
        )
    } == 0
    {
        return unavailable(PeVersionUnavailable::InvalidResource);
    }
    let fixed_size = std::mem::size_of::<VS_FIXEDFILEINFO>();
    let start = buffer.as_ptr() as usize;
    let address = value as usize;
    if value_size < fixed_size as u32
        || address < start
        || address
            .checked_add(fixed_size)
            .is_none_or(|end| end > start + buffer.len())
    {
        return unavailable(PeVersionUnavailable::InvalidResource);
    }
    // SAFETY: the API-returned address and fixed record size were checked inside
    // the live buffer. Unaligned reading avoids assuming resource alignment.
    let fixed = unsafe { std::ptr::read_unaligned(value.cast::<VS_FIXEDFILEINFO>()) };
    if fixed.dwSignature != 0xFEEF04BD {
        return unavailable(PeVersionUnavailable::InvalidResource);
    }
    PeVersionMetadata::FileVersion {
        major: (fixed.dwFileVersionMS >> 16) as u16,
        minor: fixed.dwFileVersionMS as u16,
        patch: (fixed.dwFileVersionLS >> 16) as u16,
        revision: fixed.dwFileVersionLS as u16,
    }
}

#[cfg(not(windows))]
pub fn inspect_pe_version(_path: &Path) -> PeVersionMetadata {
    unavailable(PeVersionUnavailable::UnsupportedPlatform)
}

fn unavailable(reason: PeVersionUnavailable) -> PeVersionMetadata {
    PeVersionMetadata::Unavailable { reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_wrapper_paths_cannot_trigger_cli_execution() {
        #[cfg(windows)]
        {
            assert_eq!(
                inspect_pe_version(Path::new("relative.exe")),
                unavailable(PeVersionUnavailable::UnsafePath)
            );
            assert_eq!(
                inspect_pe_version(Path::new("C:/tools/pi.cmd")),
                unavailable(PeVersionUnavailable::Wrapper)
            );
            assert_eq!(
                inspect_pe_version(Path::new(r"\\server\codex.exe")),
                unavailable(PeVersionUnavailable::UnsafePath)
            );
        }
        #[cfg(not(windows))]
        assert_eq!(
            inspect_pe_version(Path::new("C:/tools/codex.exe")),
            unavailable(PeVersionUnavailable::UnsupportedPlatform)
        );
    }

    #[cfg(windows)]
    #[test]
    fn missing_local_file_stays_unknown_without_any_process() {
        let root = std::env::var_os("SystemRoot").expect("Windows system root");
        let path = std::path::PathBuf::from(root).join("Shinatama-fixture-missing-6a9bf452.exe");
        assert!(matches!(
            inspect_pe_version(&path),
            PeVersionMetadata::Unavailable { .. }
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_file_metadata_does_not_claim_a_cli_semver() {
        let root = std::env::var_os("SystemRoot").expect("Windows system root");
        let path = std::path::PathBuf::from(root)
            .join("System32")
            .join("cmd.exe");
        assert!(matches!(
            inspect_pe_version(&path),
            PeVersionMetadata::FileVersion { major: 6.., .. }
        ));
        // This metadata type cannot be supplied where ObservedVersion::Known
        // requires Version; the values cannot implicitly qualify an adapter.
    }
}
