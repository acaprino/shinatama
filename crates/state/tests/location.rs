#![cfg(windows)]

use shinatama_state::location::{LocationErrorKind, LocationRequest, StorageLocation};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "shinatama-location-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn request(&self) -> LocationRequest {
        LocationRequest {
            local_data_directory: self.0.join("local-data"),
            home_override: Some(self.0.join("storage")),
            project_roots: vec![],
            synchronized_roots: vec![],
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn default_is_exact_and_resolution_never_creates_files() {
    let fixture = Fixture::new();
    let mut request = fixture.request();
    request.home_override = None;
    let location = StorageLocation::resolve(request.clone()).unwrap();
    assert!(
        location
            .database_path()
            .ends_with(Path::new("local-data/shinatama/shinatama.db"))
    );
    assert!(!request.local_data_directory.exists());
    location.prepare_private_directory().unwrap();
    location.verify_private_directory().unwrap();
    assert!(!location.database_path().exists());
}

#[test]
fn invalid_and_network_paths_are_refused_before_creation() {
    let fixture = Fixture::new();
    for path in [
        PathBuf::from("relative"),
        fixture.0.join("missing/../unsafe"),
        fixture.0.join("NUL/data"),
        fixture.0.join("trailing./data"),
        PathBuf::from(r"\\server\share\shinatama"),
        PathBuf::from(r"\\?\UNC\server\share\shinatama"),
        PathBuf::from(r"\\.\C:\shinatama"),
        PathBuf::from(r"C:\"),
    ] {
        let mut request = fixture.request();
        request.home_override = Some(path.clone());
        let error = StorageLocation::resolve(request).unwrap_err();
        assert!(
            matches!(
                error.kind,
                LocationErrorKind::InvalidPath | LocationErrorKind::Network
            ),
            "{path:?}: {error}"
        );
    }
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
}

#[test]
fn project_and_registered_sync_roots_reject_missing_descendants() {
    let fixture = Fixture::new();
    for (kind, root) in [
        (LocationErrorKind::Project, "project"),
        (LocationErrorKind::Synchronized, "sync"),
    ] {
        let base = fixture.0.join(root);
        fs::create_dir(&base).unwrap();
        let mut request = fixture.request();
        request.home_override = Some(base.join("missing/storage"));
        if kind == LocationErrorKind::Project {
            request.project_roots.push(base.clone());
        } else {
            request.synchronized_roots.push(base.clone());
        }
        assert_eq!(StorageLocation::resolve(request).unwrap_err().kind, kind);
        assert_eq!(fs::read_dir(base).unwrap().count(), 0);
    }
}

#[test]
fn project_boundaries_do_not_match_similar_names() {
    let fixture = Fixture::new();
    let mut request = fixture.request();
    request.project_roots.push(fixture.0.join("project"));
    request.home_override = Some(fixture.0.join("project-backups"));
    let location = StorageLocation::resolve(request).unwrap();
    location.prepare_private_directory().unwrap();
    // Folder names alone do not classify a perfectly local disk as a sync provider.
    let mut request = fixture.request();
    request.home_override = Some(fixture.0.join("OneDrive ordinary local folder"));
    assert!(StorageLocation::resolve(request).is_ok());
}

#[test]
fn existing_git_worktree_is_refused_without_a_recent_project_record() {
    let fixture = Fixture::new();
    let project = fixture.0.join("unlisted-project");
    fs::create_dir(&project).unwrap();
    fs::write(project.join(".git"), "gitdir: elsewhere").unwrap();
    let mut request = fixture.request();
    request.home_override = Some(project.join("state"));
    assert_eq!(
        StorageLocation::resolve(request).unwrap_err().kind,
        LocationErrorKind::Project
    );
    assert!(!project.join("state").exists());
}

#[test]
fn private_directory_and_files_preserve_existing_bytes() {
    let fixture = Fixture::new();
    let location = StorageLocation::resolve(fixture.request()).unwrap();
    location.prepare_private_directory().unwrap();
    let file = location.database_path();
    fs::write(&file, b"saved work").unwrap();
    location.secure_private_file(&file).unwrap();
    location.verify_private_file(&file).unwrap();
    location.prepare_private_directory().unwrap();
    assert_eq!(fs::read(file).unwrap(), b"saved work");
    assert_eq!(fs::read_dir(location.directory()).unwrap().count(), 1);
}

#[test]
fn optional_sqlite_files_accept_disappearance_but_reject_unsafe_existing_files() {
    let fixture = Fixture::new();
    let location = StorageLocation::resolve(fixture.request()).unwrap();
    location.prepare_private_directory().unwrap();
    let outside = fixture.0.join("outside.db");
    fs::write(&outside, b"unrelated saved work").unwrap();
    for name in ["shinatama.db-wal", "shinatama.db-shm"] {
        let sidecar = location.directory().join(name);
        fs::write(&sidecar, b"transient SQLite content").unwrap();
        assert!(location.secure_private_file_if_present(&sidecar).unwrap());
        location.verify_private_file(&sidecar).unwrap();
        fs::remove_file(&sidecar).unwrap();
        assert!(!location.secure_private_file_if_present(&sidecar).unwrap());
        assert!(!sidecar.exists(), "absence must never recreate a sidecar");
        fs::hard_link(&outside, &sidecar).unwrap();
        assert_eq!(
            location
                .secure_private_file_if_present(&sidecar)
                .unwrap_err()
                .kind,
            LocationErrorKind::InvalidPath
        );
        fs::remove_file(&sidecar).unwrap();
    }
    assert_eq!(fs::read(&outside).unwrap(), b"unrelated saved work");
    assert_eq!(
        location
            .secure_private_file(&location.database_path())
            .unwrap_err()
            .kind,
        LocationErrorKind::Io,
        "the authoritative database must retain its strict file guard"
    );
    assert_eq!(
        location
            .secure_private_file_if_present(&outside)
            .unwrap_err()
            .kind,
        LocationErrorKind::InvalidPath,
        "optional files must still be direct children of private storage"
    );
}

#[test]
fn optional_sqlite_file_waits_for_pending_deletion_without_ignoring_a_persistent_block() {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfo, SetFileInformationByHandle,
    };
    let fixture = Fixture::new();
    let location = StorageLocation::resolve(fixture.request()).unwrap();
    location.prepare_private_directory().unwrap();
    let sidecar = location.directory().join("shinatama.db-shm");
    fs::write(&sidecar, b"transient SQLite content").unwrap();
    let handle = fs::OpenOptions::new()
        .access_mode(0x0001_0000 | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(&sidecar)
        .unwrap();
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    assert_ne!(
        unsafe {
            SetFileInformationByHandle(
                handle.as_raw_handle(),
                FileDispositionInfo,
                (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        },
        0
    );
    let closer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(25));
        drop(handle);
    });
    let result = location.secure_private_file_if_present(&sidecar);
    closer.join().unwrap();
    assert!(!result.unwrap(), "a completed SQLite deletion is absence");
    assert!(!sidecar.exists());

    fs::write(&sidecar, b"persistent content").unwrap();
    let mut persistent = fs::OpenOptions::new()
        .access_mode(0x0001_0000 | FILE_READ_ATTRIBUTES | FILE_READ_DATA)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(&sidecar)
        .unwrap();
    assert_ne!(
        unsafe {
            SetFileInformationByHandle(
                persistent.as_raw_handle(),
                FileDispositionInfo,
                (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        },
        0
    );
    let started = std::time::Instant::now();
    assert_eq!(
        location
            .secure_private_file_if_present(&sidecar)
            .unwrap_err()
            .kind,
        LocationErrorKind::Io,
        "a persistent access/sharing failure must never be accepted as absence"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut persistent, &mut bytes).unwrap();
    assert_eq!(bytes, b"persistent content");
    drop(persistent);
}

#[test]
fn file_outside_storage_and_hard_links_are_never_secured() {
    let fixture = Fixture::new();
    let location = StorageLocation::resolve(fixture.request()).unwrap();
    location.prepare_private_directory().unwrap();
    let outside = fixture.0.join("outside.db");
    fs::write(&outside, b"outside work").unwrap();
    assert_eq!(
        location.secure_private_file(&outside).unwrap_err().kind,
        LocationErrorKind::InvalidPath
    );
    let alias = location.database_path();
    fs::hard_link(&outside, &alias).unwrap();
    assert_eq!(
        location.secure_private_file(&alias).unwrap_err().kind,
        LocationErrorKind::InvalidPath
    );
    assert_eq!(fs::read(outside).unwrap(), b"outside work");
}

#[test]
fn junction_aliases_cannot_bypass_project_or_sync_policy() {
    let fixture = Fixture::new();
    let target = fixture.0.join("target");
    fs::create_dir(&target).unwrap();
    let alias = fixture.0.join("alias");
    let output = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&alias)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mklink: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for kind in [LocationErrorKind::Project, LocationErrorKind::Synchronized] {
        let mut request = fixture.request();
        request.home_override = Some(alias.join("not-created/state"));
        if kind == LocationErrorKind::Project {
            request.project_roots.push(target.clone());
        } else {
            request.synchronized_roots.push(target.clone());
        }
        assert_eq!(StorageLocation::resolve(request).unwrap_err().kind, kind);
    }
    assert!(!target.join("not-created").exists());
    fs::remove_dir(alias).unwrap();
}

#[test]
fn prefixed_paths_and_drive_case_resolve_to_one_location() {
    let fixture = Fixture::new();
    let first = StorageLocation::resolve(fixture.request()).unwrap();
    first.prepare_private_directory().unwrap();
    let mut request = fixture.request();
    let canonical = fs::canonicalize(first.directory()).unwrap();
    request.home_override = Some(PathBuf::from(canonical.to_string_lossy().to_lowercase()));
    let second = StorageLocation::resolve(request).unwrap();
    assert_eq!(first.database_path(), second.database_path());
}

#[test]
fn storage_cannot_take_over_platform_or_project_parent_directory() {
    let fixture = Fixture::new();
    let mut request = fixture.request();
    request.home_override = Some(fixture.0.clone());
    assert_eq!(
        StorageLocation::resolve(request).unwrap_err().kind,
        LocationErrorKind::InvalidPath
    );
    let mut request = fixture.request();
    request.home_override = Some(fixture.0.join("projects"));
    request.project_roots.push(fixture.0.join("projects/one"));
    assert_eq!(
        StorageLocation::resolve(request).unwrap_err().kind,
        LocationErrorKind::Project
    );
    assert!(!fixture.0.join("projects").exists());
}

#[test]
fn later_project_selection_cannot_include_active_storage() {
    let fixture = Fixture::new();
    let location = StorageLocation::resolve(fixture.request()).unwrap();
    location.prepare_private_directory().unwrap();
    assert_eq!(
        location.validate_project(&fixture.0).unwrap_err().kind,
        LocationErrorKind::Project
    );
    assert_eq!(
        location
            .validate_project(location.directory())
            .unwrap_err()
            .kind,
        LocationErrorKind::Project
    );
    assert!(
        location
            .validate_project(&fixture.0.join("ordinary-project"))
            .is_ok()
    );
}

/// The DACL stored on the file itself. GetFileSecurityW returns the descriptor as it is;
/// GetNamedSecurityInfoW, for a DACL without SE_DACL_AUTO_INHERITED, reports each ACE's
/// inherited flag against the current parent, so making the parent private reads as a change
/// to an untouched child (Verify Windows: SY and BA "lost" their ID flag).
fn dacl(path: &Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertSecurityDescriptorToStringSecurityDescriptorW;
    use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, GetFileSecurityW};
    let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut needed = 0;
    unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION,
            null_mut(),
            0,
            &mut needed,
        );
    }
    assert!(needed > 0, "the descriptor size of {}", path.display());
    // u64 cells keep the self-relative descriptor aligned.
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
    let descriptor = buffer.as_mut_ptr().cast();
    let mut text = null_mut();
    let mut length = 0;
    unsafe {
        assert_ne!(
            GetFileSecurityW(
                name.as_ptr(),
                DACL_SECURITY_INFORMATION,
                descriptor,
                needed,
                &mut needed,
            ),
            0
        );
        assert_ne!(
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                &mut length
            ),
            0
        );
        let value = std::slice::from_raw_parts(text, length as usize);
        let value = String::from_utf16_lossy(
            &value[..value.iter().position(|n| *n == 0).unwrap_or(value.len())],
        );
        LocalFree(text.cast());
        value
    }
}

#[test]
fn repairing_existing_root_never_propagates_permissions_to_existing_children() {
    let fixture = Fixture::new();
    let request = fixture.request();
    let storage = request.home_override.as_ref().unwrap();
    fs::create_dir(storage).unwrap();
    let existing = storage.join("existing.txt");
    fs::write(&existing, b"preserve saved work").unwrap();
    let before = dacl(&existing);
    let location = StorageLocation::resolve(request).unwrap();
    location.prepare_private_directory().unwrap();
    location.verify_private_directory().unwrap();
    assert_eq!(dacl(&existing), before);
    assert_eq!(fs::read(existing).unwrap(), b"preserve saved work");
}

#[test]
fn storage_preflight_requires_no_existing_directory() {
    let fixture = Fixture::new();
    let missing = fixture.0.join("not-created/any/storage");
    StorageLocation::preflight(&missing).unwrap();
    assert!(!missing.exists());
    assert_eq!(
        StorageLocation::preflight(Path::new(r"\\offline-server\share\storage"))
            .unwrap_err()
            .kind,
        LocationErrorKind::Network
    );
}
