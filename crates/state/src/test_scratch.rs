//! Where the tests keep their throwaway state: beside the repository, on the drive it is
//! worked on, and outside every Git work tree (a storage location inside one is refused).
//! `SHINATAMA_TEST_SCRATCH` names another folder.

use std::path::{Path, PathBuf};

#[doc(hidden)]
pub fn root() -> PathBuf {
    if let Some(root) = std::env::var_os("SHINATAMA_TEST_SCRATCH") {
        return PathBuf::from(root);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .filter(|folder| folder.join(".git").exists())
        .last()
        .and_then(Path::parent)
        .map(|folder| folder.join(".test-scratch").join("shinatama"))
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA")))
}
