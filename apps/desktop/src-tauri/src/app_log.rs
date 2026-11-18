//! Shinatama's diagnostic log: `shinatama.log` in the data folder, beside the database. It says
//! why an open, a Resume or the saved layout failed, and how long the start steps took, so a
//! refusal can still be explained after its notice is gone. Each line is bounded and
//! control-free and holds fixed sentences, codes and durations: never a transcript, a prompt,
//! a terminal's text or a secret. The one line taken from a child is what the model proxy's
//! runtime said when its process ended (`sidecar::ExitLog`, ADR-0030).
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const FILE: &str = "shinatama.log";
/// Past [`MAX_BYTES`] the log becomes this file (replacing the one before) and a new log begins.
const PREVIOUS: &str = "shinatama.log.1";
const MAX_BYTES: u64 = 1024 * 1024;
const MAX_SOURCE: usize = 24;
const MAX_MESSAGE: usize = 600;
/// What the window may write: the layout it could not load, a page that (re)loaded.
pub(crate) const PAGE_SOURCES: [&str; 2] = ["layout", "page"];

static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static WRITING: Mutex<()> = Mutex::new(());

/// The data folder; nothing is written before this runs.
pub(crate) fn start(directory: &Path) {
    let _ = DIRECTORY.set(directory.to_path_buf());
}

/// Appends one line. A log that cannot be written is skipped: it never fails what it records.
pub(crate) fn write(source: &str, message: &str) {
    let Some(directory) = DIRECTORY.get() else {
        return;
    };
    let text = line(&chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(), source, message);
    let Ok(_writing) = WRITING.lock() else {
        return;
    };
    append(directory, &text);
}

fn append(directory: &Path, text: &str) {
    let path = directory.join(FILE);
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= MAX_BYTES) {
        let _ = fs::rename(&path, directory.join(PREVIOUS));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(text.as_bytes());
    }
}

/// `<at> <source>: <message>` and a newline. The source keeps `a-z`, `0-9` and `_`; the message
/// loses its control characters (a line break included) and is cut at [`MAX_MESSAGE`] characters.
fn line(at: &str, source: &str, message: &str) -> String {
    let source: String = source
        .chars()
        .filter(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == '_')
        .take(MAX_SOURCE)
        .collect();
    let message: String = message
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(MAX_MESSAGE)
        .collect();
    format!("{at} {}: {}\n", if source.is_empty() { "app" } else { &source }, message.trim_end())
}

/// The window's own lines: why the saved layout did not load, and each (re)load of the page.
#[tauri::command]
pub(crate) fn app_log(window: tauri::WebviewWindow, source: String, message: String) -> Result<(), String> {
    crate::local(&window)?;
    if !PAGE_SOURCES.contains(&source.as_str()) || message.len() > 4 * MAX_MESSAGE {
        return Err("This log line is not accepted.".into());
    }
    write(&source, &message);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_one_bounded_line_whatever_it_is_given() {
        assert_eq!(line("2026-11-17 14:00:00.000", "resume", "opened after 812 ms"),
            "2026-11-17 14:00:00.000 resume: opened after 812 ms\n");
        let text = line("t", "Re sume\n!", &format!("first\nsecond\r\u{7}{}", "x".repeat(2000)));
        assert_eq!(text.matches('\n').count(), 1);
        assert!(text.starts_with("t esume: first second"));
        assert!(text.len() < MAX_MESSAGE + 40);
        assert_eq!(line("t", "", "x"), "t app: x\n");
    }

    #[test]
    fn a_full_log_moves_aside_and_a_new_one_begins() {
        let directory = std::env::temp_dir().join(format!("shn-app-log-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join(FILE), vec![b'x'; MAX_BYTES as usize]).unwrap();
        append(&directory, "next\n");
        assert_eq!(fs::read_to_string(directory.join(FILE)).unwrap(), "next\n");
        assert_eq!(fs::metadata(directory.join(PREVIOUS)).unwrap().len(), MAX_BYTES);
        append(&directory, "after\n");
        assert_eq!(fs::read_to_string(directory.join(FILE)).unwrap(), "next\nafter\n");
        let _ = fs::remove_dir_all(&directory);
    }
}
