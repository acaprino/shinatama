//! C27.3: private hand-off files (FRD-0005 R24, R08).
//!
//! `<handoffs>` is `<git common dir>/shinatama/handoffs` for a Git project (shared by linked
//! worktrees, never tracked) or the application data folder otherwise. Every folder and file is
//! created owner-only. Nothing is written into the working tree: no instruction file appears in
//! the user's repository.
use shinatama_state::location::external;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const SEED_FILE: &str = "seed.md";
pub const FULL_FILE: &str = "full.md";
pub const MAX_FULL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffPaths {
    pub dir: PathBuf,
    pub seed: PathBuf,
    pub full: PathBuf,
    pub in_repository: bool,
}

/// Where this hop's files go. `project` is the conversation's folder when it still exists.
pub fn handoff_paths(project: Option<&Path>, data_dir: &Path, hop_id: &str) -> HandoffPaths {
    let (root, in_repository) = match project.filter(|p| p.is_dir()).and_then(crate::git::common_dir) {
        Some(common) => (common.join("shinatama").join("handoffs"), true),
        None => (data_dir.join("handoffs"), false),
    };
    let dir = root.join(hop_id);
    HandoffPaths { seed: dir.join(SEED_FILE), full: dir.join(FULL_FILE), dir, in_repository }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.to_owned())
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    external::secure_private_file(path).map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e.to_string()))?;
    external::verify_private_file(path).map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e.to_string()))
}

/// Creates the hop folder owner-only and writes both files once (never overwrites).
pub fn write_handoff(paths: &HandoffPaths, seed: &str, full: &str) -> io::Result<()> {
    if full.len() > MAX_FULL_BYTES {
        return Err(invalid("The full transcript exceeds 64 MiB."));
    }
    let name = paths.dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if name.len() != 32 || !name.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid("The hop id is not a simple UUID."));
    }
    let fresh = !paths.dir.exists();
    let _guards = external::prepare_private_directory(&paths.dir).map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e.to_string()))?;
    // Remove only what this call created; an existing hand-off is never touched.
    let mut created = Vec::new();
    let mut result = Ok(());
    for (path, bytes) in [(&paths.full, full.as_bytes()), (&paths.seed, seed.as_bytes())] {
        match write_private(path, bytes) {
            Ok(()) => created.push(path.clone()),
            Err(error) => {
                if error.kind() != io::ErrorKind::AlreadyExists {
                    let _ = std::fs::remove_file(path);
                }
                result = Err(error);
                break;
            }
        }
    }
    if result.is_err() {
        for path in created {
            let _ = std::fs::remove_file(path);
        }
        if fresh {
            let _ = std::fs::remove_dir(&paths.dir);
        }
    }
    result
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn git_projects_use_the_common_dir_and_others_the_data_folder() {
        let base = std::env::temp_dir().join(format!("shinatama-handoff-{}", uuid::Uuid::new_v4().simple()));
        let project = base.join("proj");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::create_dir_all(base.join("data")).unwrap();
        let id = uuid::Uuid::new_v4().simple().to_string();
        let paths = handoff_paths(Some(&project), &base.join("data"), &id);
        assert!(paths.in_repository);
        assert_eq!(paths.dir, project.join(".git").join("shinatama").join("handoffs").join(&id));
        write_handoff(&paths, "seed", "full").unwrap();
        assert_eq!(std::fs::read_to_string(&paths.seed).unwrap(), "seed");
        assert!(write_handoff(&paths, "again", "again").is_err(), "files are written once");
        assert_eq!(std::fs::read_to_string(&paths.full).unwrap(), "full");
        let entries: Vec<_> = std::fs::read_dir(&project).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(entries, [std::ffi::OsString::from(".git")], "nothing lands in the working tree");
        let plain = handoff_paths(Some(&base.join("data")), &base.join("data"), &id);
        assert!(!plain.in_repository);
        assert_eq!(plain.dir, base.join("data").join("handoffs").join(&id));
        assert!(write_handoff(&handoff_paths(None, &base, "not-a-uuid"), "", "").is_err());
        std::fs::remove_dir_all(base).unwrap();
    }
}
