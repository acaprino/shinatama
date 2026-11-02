//! Read-only synthetic-workspace commands, compiled only for the TL1 proof binary.
use serde::Serialize;
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

const MAX_ENTRIES: usize = 256;
const MAX_FILE_BYTES: u64 = 64 * 1024;

#[derive(Serialize)]
pub struct Entry {
    path: String,
    directory: bool,
}

#[derive(Serialize)]
pub struct Workspace {
    root: String,
    entries: Vec<Entry>,
}

fn root(window: &tauri::WebviewWindow) -> Result<PathBuf, String> {
    if window.label() != "main" {
        return Err("Unknown proof window".into());
    }
    let path = std::env::var_os("SHINATAMA_TL1_ROOT")
        .map(PathBuf::from)
        .ok_or("The proof needs an explicit synthetic workspace")?;
    if !path.is_absolute() {
        return Err("The proof workspace must be absolute".into());
    }
    let path = path
        .canonicalize()
        .map_err(|_| "Proof workspace unavailable")?;
    if !path.is_dir() {
        return Err("Proof workspace is not a directory".into());
    }
    Ok(path)
}

fn collect(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<Entry>,
    depth: u8,
) -> Result<(), String> {
    if depth > 8 {
        return Err("Proof directory is too deep".into());
    }
    let mut children = fs::read_dir(directory)
        .map_err(|_| "Cannot list proof directory")?
        .take(MAX_ENTRIES + 1)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Cannot list proof directory")?;
    if children.len() > MAX_ENTRIES {
        return Err("Proof directory has too many entries".into());
    }
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        if entries.len() >= MAX_ENTRIES {
            return Err("Proof directory has too many entries".into());
        }
        let file_type = child
            .file_type()
            .map_err(|_| "Cannot inspect proof entry")?;
        // Do not follow junctions, symbolic links or any canonical escape.
        let path = child.path();
        let canonical = path
            .canonicalize()
            .map_err(|_| "Cannot resolve proof entry")?;
        if file_type.is_symlink() || !canonical.starts_with(root) {
            continue;
        }
        let directory = file_type.is_dir();
        if directory || file_type.is_file() {
            entries.push(Entry {
                path: path
                    .strip_prefix(root)
                    .map_err(|_| "Invalid proof entry")?
                    .to_string_lossy()
                    .replace('\\', "/"),
                directory,
            });
            if directory {
                collect(root, &canonical, entries, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn read(root: &Path, relative_path: &str) -> Result<String, String> {
    if relative_path.is_empty()
        || relative_path.len() > 1024
        || relative_path.contains(':')
        || Path::new(relative_path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Choose a relative proof file".into());
    }
    let path = root
        .join(relative_path)
        .canonicalize()
        .map_err(|_| "Proof file unavailable")?;
    if !path.starts_with(root) || !path.is_file() {
        return Err("File is outside the proof workspace".into());
    }
    let file = fs::File::open(path).map_err(|_| "Cannot open proof file")?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read proof file")?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("Proof files are limited to 64 KiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "Proof file must be UTF-8 text".into())
}

#[tauri::command]
pub fn tl1_files(window: tauri::WebviewWindow) -> Result<Workspace, String> {
    let root = root(&window)?;
    let mut entries = Vec::new();
    collect(&root, &root, &mut entries, 0)?;
    Ok(Workspace {
        root: root.to_string_lossy().into_owned(),
        entries,
    })
}

#[tauri::command]
pub fn tl1_read(window: tauri::WebviewWindow, relative_path: String) -> Result<String, String> {
    read(&root(&window)?, &relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("shinatama-tl1-{}-{unique}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rejects_non_relative_paths_before_opening() {
        for path in [
            "",
            "../secret",
            "/absolute",
            "a/../../secret",
            "C:\\secret",
            "a:stream",
        ] {
            assert!(read(Path::new("."), path).is_err(), "{path}");
        }
    }

    #[test]
    fn reads_exact_utf8_and_rejects_oversized_or_binary_files() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("valid.md"), "# proof\n").unwrap();
        fs::write(
            fixture.0.join("large.txt"),
            vec![b'x'; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        fs::write(fixture.0.join("binary.dat"), [0xff, 0xfe]).unwrap();
        assert_eq!(read(&fixture.0, "valid.md").unwrap(), "# proof\n");
        assert!(read(&fixture.0, "large.txt").is_err());
        assert!(read(&fixture.0, "binary.dat").is_err());
    }

    #[test]
    fn refuses_an_oversized_directory_listing() {
        let fixture = Fixture::new();
        for index in 0..=MAX_ENTRIES {
            fs::write(fixture.0.join(format!("{index}.txt")), "").unwrap();
        }
        let mut entries = Vec::new();
        assert!(collect(&fixture.0, &fixture.0, &mut entries, 0).is_err());
    }
}
