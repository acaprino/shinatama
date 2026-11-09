//! Atomic export output (FRD-0006 R31): a temporary file in the destination folder, flushed to
//! disk, then renamed. A failure leaves neither a partial file nor a replaced one; an existing
//! file is replaced only with [`Overwrite::Replace`].
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overwrite {
    Refuse,
    Replace,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputError {
    InvalidPath,
    ParentMissing,
    Exists,
    AttachmentsExist,
    Write(io::ErrorKind),
}
impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputError::InvalidPath => write!(f, "The output path is not a valid absolute file path."),
            OutputError::ParentMissing => write!(f, "The output folder does not exist; it is not created automatically."),
            OutputError::Exists => write!(f, "The output file already exists; choose replace to overwrite it."),
            OutputError::AttachmentsExist => write!(f, "The attachments folder already exists; choose replace to overwrite it."),
            OutputError::Write(kind) => write!(f, "Writing the export failed ({kind}); no file was left behind."),
        }
    }
}
impl std::error::Error for OutputError {}
impl From<io::Error> for OutputError {
    fn from(error: io::Error) -> Self {
        OutputError::Write(error.kind())
    }
}

pub struct AttachmentFile {
    /// File name inside the attachments folder (`<sha256>.<ext>`); validated before writing.
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct Receipt {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub attachments_dir: Option<PathBuf>,
    pub attachments: u64,
}

pub fn attachments_dir_for(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    Some(path.with_file_name(format!("{name}.attachments")))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
}

fn check_target(path: &Path) -> Result<(&Path, &str), OutputError> {
    if !path.is_absolute() {
        return Err(OutputError::InvalidPath);
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| valid_name(n))
        .ok_or(OutputError::InvalidPath)?;
    let parent = path.parent().ok_or(OutputError::InvalidPath)?;
    match fs::metadata(parent) {
        Ok(meta) if meta.is_dir() => {}
        _ => return Err(OutputError::ParentMissing),
    }
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
        return Err(OutputError::InvalidPath);
    }
    Ok((parent, name))
}

fn temp_name(parent: &Path, name: &str, suffix: &str) -> PathBuf {
    parent.join(format!(".{name}.{}.{suffix}", uuid::Uuid::new_v4().simple()))
}

/// Writes `content` (and optional attachments) atomically to `path`.
pub fn write_export(
    path: &Path,
    overwrite: Overwrite,
    content: &[u8],
    attachments: &[AttachmentFile],
) -> Result<Receipt, OutputError> {
    write_with(path, overwrite, attachments, |writer| writer.write_all(content))
}

/// As [`write_export`], with the content produced by `fill`; an error from `fill` aborts the
/// export and removes every temporary file.
pub fn write_with(
    path: &Path,
    overwrite: Overwrite,
    attachments: &[AttachmentFile],
    fill: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<Receipt, OutputError> {
    let (parent, name) = check_target(path)?;
    if attachments.iter().any(|a| !valid_name(&a.name) || a.name.starts_with('.')) {
        return Err(OutputError::InvalidPath);
    }
    let dir = attachments_dir_for(path).ok_or(OutputError::InvalidPath)?;
    if overwrite == Overwrite::Refuse {
        if fs::symlink_metadata(path).is_ok() {
            return Err(OutputError::Exists);
        }
        if !attachments.is_empty() && fs::symlink_metadata(&dir).is_ok() {
            return Err(OutputError::AttachmentsExist);
        }
    }
    // 1. Main content into a temporary file, flushed to disk.
    let temp = temp_name(parent, name, "partial");
    let mut hasher = HashingWriter { inner: None, hash: Sha256::new(), bytes: 0 };
    let staged = (|| -> Result<(), OutputError> {
        let file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        hasher.inner = Some(BufWriter::new(file));
        fill(&mut hasher)?;
        let file = hasher.inner.take().expect("writer present").into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = staged {
        drop(hasher.inner.take());
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    // 2. Attachments into a staging folder, committed before the main file names them.
    let mut committed_dir = None;
    let mut displaced = None;
    if !attachments.is_empty() {
        let staging = temp_name(parent, name, "attachments.partial");
        let result = (|| -> Result<(), OutputError> {
            fs::create_dir(&staging)?;
            for attachment in attachments {
                let mut file = OpenOptions::new().write(true).create_new(true).open(staging.join(&attachment.name))?;
                file.write_all(&attachment.data)?;
                file.sync_all()?;
            }
            if overwrite == Overwrite::Replace && fs::symlink_metadata(&dir).is_ok() {
                let old = temp_name(parent, name, "attachments.old");
                move_path(&dir, &old, false)?;
                displaced = Some(old);
            }
            move_path(&staging, &dir, false)?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&staging);
            if let Some(old) = displaced.take() {
                let _ = move_path(&old, &dir, false);
            }
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        committed_dir = Some(dir.clone());
    }
    // 3. Commit the main file.
    if let Err(error) = move_path(&temp, path, overwrite == Overwrite::Replace) {
        let _ = fs::remove_file(&temp);
        if let Some(created) = committed_dir.take() {
            let _ = fs::remove_dir_all(&created);
        }
        if let Some(old) = displaced.take() {
            let _ = move_path(&old, &dir, false);
        }
        return Err(if error.kind() == io::ErrorKind::AlreadyExists { OutputError::Exists } else { error.into() });
    }
    if let Some(old) = displaced {
        let _ = fs::remove_dir_all(old);
    }
    Ok(Receipt {
        path: path.to_path_buf(),
        bytes: hasher.bytes,
        sha256: crate::stx::hex(&hasher.hash.finalize()),
        attachments_dir: committed_dir,
        attachments: attachments.len() as u64,
    })
}

struct HashingWriter {
    inner: Option<BufWriter<File>>,
    hash: Sha256,
    bytes: u64,
}
impl Write for HashingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.as_mut().ok_or_else(|| io::Error::other("closed"))?.write(buf)?;
        self.hash.update(&buf[..written]);
        self.bytes += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.as_mut().ok_or_else(|| io::Error::other("closed"))?.flush()
    }
}

#[cfg(windows)]
fn move_path(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};
    let wide = |p: &Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (from, to) = (wide(from), wide(to));
    let flags = MOVEFILE_WRITE_THROUGH | if replace { MOVEFILE_REPLACE_EXISTING } else { 0 };
    // SAFETY: both buffers are NUL-terminated UTF-16 paths that outlive the call.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn move_path(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    if replace || from.is_dir() {
        if !replace && to.exists() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        fs::rename(from, to)
    } else {
        fs::hard_link(from, to)?;
        fs::remove_file(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("shinatama-export-{tag}-{}", uuid::Uuid::new_v4().simple()));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }
        fn entries(&self) -> Vec<String> {
            let mut names: Vec<_> = fs::read_dir(&self.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn writes_and_refuses_to_overwrite_without_consent() {
        let scratch = Scratch::new("refuse");
        let path = scratch.0.join("a.stx.jsonl");
        let receipt = write_export(&path, Overwrite::Refuse, b"one", &[]).unwrap();
        assert_eq!(receipt.bytes, 3);
        assert_eq!(fs::read(&path).unwrap(), b"one");
        assert_eq!(write_export(&path, Overwrite::Refuse, b"two", &[]).unwrap_err(), OutputError::Exists);
        assert_eq!(fs::read(&path).unwrap(), b"one");
        write_export(&path, Overwrite::Replace, b"two", &[]).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(scratch.entries(), ["a.stx.jsonl"]);
    }

    #[test]
    fn a_failing_writer_leaves_nothing_and_keeps_the_old_file() {
        let scratch = Scratch::new("fault");
        let path = scratch.0.join("b.md");
        fs::write(&path, b"previous").unwrap();
        let error = write_with(&path, Overwrite::Replace, &[], |w| {
            w.write_all(b"half")?;
            Err(io::Error::other("disk full"))
        })
        .unwrap_err();
        assert_eq!(error, OutputError::Write(io::ErrorKind::Other));
        assert_eq!(fs::read(&path).unwrap(), b"previous");
        assert_eq!(scratch.entries(), ["b.md"]);
        let fresh = scratch.0.join("c.md");
        assert!(write_with(&fresh, Overwrite::Refuse, &[], |_| Err(io::Error::other("x"))).is_err());
        assert!(!fresh.exists());
        assert_eq!(scratch.entries(), ["b.md"]);
    }

    #[test]
    fn attachments_are_committed_with_the_file_and_refused_when_present() {
        let scratch = Scratch::new("attach");
        let path = scratch.0.join("d.stx.jsonl");
        let files = [AttachmentFile { name: "ab12.png".into(), data: vec![1, 2, 3] }];
        let receipt = write_export(&path, Overwrite::Refuse, b"x", &files).unwrap();
        let dir = receipt.attachments_dir.unwrap();
        assert_eq!(fs::read(dir.join("ab12.png")).unwrap(), [1, 2, 3]);
        fs::remove_file(&path).unwrap();
        assert_eq!(write_export(&path, Overwrite::Refuse, b"x", &files).unwrap_err(), OutputError::AttachmentsExist);
        assert!(!path.exists());
        let bad = [AttachmentFile { name: "../escape".into(), data: vec![] }];
        assert_eq!(write_export(&path, Overwrite::Replace, b"x", &bad).unwrap_err(), OutputError::InvalidPath);
        write_export(&path, Overwrite::Replace, b"y", &files).unwrap();
        assert_eq!(scratch.entries(), ["d.stx.jsonl", "d.stx.jsonl.attachments"]);
    }

    #[test]
    fn invalid_targets_are_refused_before_writing() {
        let scratch = Scratch::new("paths");
        assert_eq!(write_export(Path::new("relative.md"), Overwrite::Refuse, b"", &[]).unwrap_err(), OutputError::InvalidPath);
        assert_eq!(write_export(&scratch.0.join("missing").join("x.md"), Overwrite::Refuse, b"", &[]).unwrap_err(), OutputError::ParentMissing);
        assert_eq!(write_export(&scratch.0, Overwrite::Replace, b"", &[]).unwrap_err(), OutputError::InvalidPath);
        assert!(scratch.entries().is_empty());
    }
}
