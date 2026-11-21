//! C42.1/C42.3: one buffer per canonical file and saves that never overwrite silently
//! (FRD-0001 R36, R38).
//!
//! Views of the same file share one buffer. A save re-reads the file and refuses when its
//! content changed since the buffer's base (an agent wrote it meanwhile) unless the user chose
//! to overwrite; the new content is written to a temporary file in the same folder and swapped
//! in. A failed save leaves both the file and the buffer as they were.
use crate::navigator::{NavError, Root};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MAX_UNDO: usize = 200;
pub const MAX_BUFFER_BYTES: usize = 4 * 1024 * 1024;

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "error")]
pub enum SaveError {
    /// The file changed on disk after it was opened; nothing was written.
    Conflict {
        disk_sha256: String,
    },
    TooLarge,
    Navigation {
        reason: String,
    },
    Write {
        reason: String,
    },
}
impl From<NavError> for SaveError {
    fn from(error: NavError) -> Self {
        SaveError::Navigation {
            reason: error.to_string(),
        }
    }
}
impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Conflict { .. } => write!(
                f,
                "The file changed on disk since you opened it. Reload it, or overwrite it explicitly."
            ),
            SaveError::TooLarge => write!(f, "The text exceeds 4 MiB and was not saved."),
            SaveError::Navigation { reason } => write!(f, "{reason}"),
            SaveError::Write { reason } => write!(
                f,
                "Saving failed ({reason}); the file and your changes are unchanged."
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overwrite {
    /// Refuse when the disk content is not the buffer's base.
    IfUnchanged,
    /// The user chose to replace what is on disk.
    Always,
}

/// Saves `text` to `relative` inside `root` when the disk still holds `base_sha256`.
/// Returns the new base hash.
pub fn save_text(
    root: &Root,
    relative: &str,
    text: &str,
    base_sha256: &str,
    overwrite: Overwrite,
) -> Result<String, SaveError> {
    if text.len() > MAX_BUFFER_BYTES {
        return Err(SaveError::TooLarge);
    }
    let path = root.resolve(relative)?;
    if !fs::metadata(&path)
        .map_err(|e| SaveError::Write {
            reason: e.kind().to_string(),
        })?
        .is_file()
    {
        return Err(SaveError::Navigation {
            reason: NavError::NotAFile.to_string(),
        });
    }
    let disk = fs::read(&path).map_err(|e| SaveError::Write {
        reason: e.kind().to_string(),
    })?;
    let disk_sha256 = sha256(&disk);
    if overwrite == Overwrite::IfUnchanged && disk_sha256 != base_sha256 {
        return Err(SaveError::Conflict { disk_sha256 });
    }
    replace(&path, text.as_bytes()).map_err(|e| SaveError::Write {
        reason: e.kind().to_string(),
    })?;
    Ok(sha256(text.as_bytes()))
}

fn replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    let name = path
        .file_name()
        .ok_or(std::io::ErrorKind::InvalidInput)?
        .to_string_lossy();
    let temp: PathBuf = parent.join(format!(".{name}.{}.saving", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        swap(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(windows)]
fn swap(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let (from, to) = (wide(from), wide(to));
    // SAFETY: NUL-terminated UTF-16 paths that outlive the call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
#[cfg(not(windows))]
fn swap(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

/// An open document. Its text lives only here, never in the saved layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffer {
    pub path: String,
    pub base_sha256: String,
    pub base_text: String,
    pub text: String,
    pub views: usize,
    undo: Vec<String>,
    redo: Vec<String>,
}
impl Buffer {
    pub fn dirty(&self) -> bool {
        self.text != self.base_text
    }
    pub fn edit(&mut self, text: String) {
        if text == self.text {
            return;
        }
        self.undo.push(std::mem::replace(&mut self.text, text));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(previous) => {
                self.redo.push(std::mem::replace(&mut self.text, previous));
                true
            }
            None => false,
        }
    }
    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.text, next));
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CloseOutcome {
    Closed,
    /// Still open: the user cancelled, or the save failed and the text is kept.
    KeptOpen(Option<SaveError>),
}

/// Buffers keyed by canonical path, so two views of one file share edits.
pub struct Buffers {
    root: Root,
    open: HashMap<PathBuf, Buffer>,
}
impl Buffers {
    pub fn new(root: Root) -> Self {
        Self {
            root,
            open: HashMap::new(),
        }
    }
    fn key(&self, relative: &str) -> Result<PathBuf, NavError> {
        self.root.resolve(relative)
    }
    /// Opens (or joins) the buffer of a text file.
    pub fn open(&mut self, relative: &str) -> Result<&mut Buffer, NavError> {
        let key = self.key(relative)?;
        if !self.open.contains_key(&key) {
            let file = self.root.read(relative)?;
            let text = match (file.text, file.truncated) {
                (Some(text), false) => text,
                _ => return Err(NavError::Unreadable),
            };
            self.open.insert(
                key.clone(),
                Buffer {
                    path: file.path,
                    base_sha256: file.sha256,
                    base_text: text.clone(),
                    text,
                    views: 0,
                    undo: Vec::new(),
                    redo: Vec::new(),
                },
            );
        }
        let buffer = self.open.get_mut(&key).expect("inserted");
        buffer.views += 1;
        Ok(buffer)
    }
    pub fn get(&mut self, relative: &str) -> Option<&mut Buffer> {
        let key = self.key(relative).ok()?;
        self.open.get_mut(&key)
    }
    pub fn save(&mut self, relative: &str, overwrite: Overwrite) -> Result<(), SaveError> {
        let key = self.key(relative)?;
        let buffer = self.open.get_mut(&key).ok_or(SaveError::Navigation {
            reason: "not open".into(),
        })?;
        let sha = save_text(
            &self.root,
            relative,
            &buffer.text,
            &buffer.base_sha256,
            overwrite,
        )?;
        buffer.base_sha256 = sha;
        buffer.base_text = buffer.text.clone();
        Ok(())
    }
    /// Closes one view; the last view of a dirty buffer needs a decision.
    pub fn close(&mut self, relative: &str, decision: CloseDecision) -> CloseOutcome {
        let Ok(key) = self.key(relative) else {
            return CloseOutcome::Closed;
        };
        let Some(buffer) = self.open.get(&key) else {
            return CloseOutcome::Closed;
        };
        if buffer.views > 1 {
            self.open.get_mut(&key).expect("present").views -= 1;
            return CloseOutcome::Closed;
        }
        if buffer.dirty() {
            match decision {
                CloseDecision::Cancel => return CloseOutcome::KeptOpen(None),
                CloseDecision::Save => {
                    if let Err(error) = self.save(relative, Overwrite::IfUnchanged) {
                        return CloseOutcome::KeptOpen(Some(error));
                    }
                }
                CloseDecision::Discard => {}
            }
        }
        self.open.remove(&key);
        CloseOutcome::Closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn project() -> (Dir, Buffers) {
        let dir = std::env::temp_dir().join(format!(
            "shinatama-buffers-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("notes.md"), "one\n").unwrap();
        let buffers = Buffers::new(Root::open(&dir).unwrap());
        (Dir(dir), buffers)
    }

    #[test]
    fn two_views_share_one_buffer_with_undo_and_redo() {
        let (_dir, mut buffers) = project();
        buffers.open("notes.md").unwrap().edit("two\n".into());
        let second = buffers.open("./notes.md".trim_start_matches("./")).unwrap();
        assert_eq!(second.text, "two\n");
        assert_eq!(second.views, 2);
        assert!(second.dirty());
        assert!(second.undo());
        assert!(!second.dirty());
        assert!(second.redo());
        assert_eq!(second.text, "two\n");
    }

    #[test]
    fn an_external_write_is_never_overwritten_silently() {
        let (dir, mut buffers) = project();
        buffers.open("notes.md").unwrap().edit("mine\n".into());
        fs::write(dir.0.join("notes.md"), "agent\n").unwrap();
        assert!(matches!(
            buffers.save("notes.md", Overwrite::IfUnchanged),
            Err(SaveError::Conflict { .. })
        ));
        assert_eq!(
            fs::read_to_string(dir.0.join("notes.md")).unwrap(),
            "agent\n"
        );
        assert_eq!(
            buffers.get("notes.md").unwrap().text,
            "mine\n",
            "the buffer is kept"
        );
        buffers.save("notes.md", Overwrite::Always).unwrap();
        assert_eq!(
            fs::read_to_string(dir.0.join("notes.md")).unwrap(),
            "mine\n"
        );
        assert!(!buffers.get("notes.md").unwrap().dirty());
        let leftovers: Vec<_> = fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers.len(), 1, "no temporary file is left");
    }

    #[test]
    fn dirty_close_offers_save_discard_and_cancel() {
        let (dir, mut buffers) = project();
        buffers.open("notes.md").unwrap().edit("draft\n".into());
        assert_eq!(
            buffers.close("notes.md", CloseDecision::Cancel),
            CloseOutcome::KeptOpen(None)
        );
        fs::write(dir.0.join("notes.md"), "agent\n").unwrap();
        assert!(matches!(
            buffers.close("notes.md", CloseDecision::Save),
            CloseOutcome::KeptOpen(Some(SaveError::Conflict { .. }))
        ));
        assert_eq!(buffers.get("notes.md").unwrap().text, "draft\n");
        assert_eq!(
            buffers.close("notes.md", CloseDecision::Discard),
            CloseOutcome::Closed
        );
        assert_eq!(
            fs::read_to_string(dir.0.join("notes.md")).unwrap(),
            "agent\n"
        );
        assert!(buffers.get("notes.md").is_none());
    }

    #[test]
    fn a_failed_save_keeps_file_and_buffer() {
        let (dir, mut buffers) = project();
        buffers
            .open("notes.md")
            .unwrap()
            .edit("x".repeat(MAX_BUFFER_BYTES + 1));
        assert_eq!(
            buffers.save("notes.md", Overwrite::Always),
            Err(SaveError::TooLarge)
        );
        assert_eq!(fs::read_to_string(dir.0.join("notes.md")).unwrap(), "one\n");
        assert!(buffers.get("notes.md").unwrap().dirty());
    }
}
