//! What a terminal receives besides typed text: files and folders dropped on it, files copied
//! in Explorer and images on the clipboard. Every harness reads a path, so each becomes the
//! path it names: a dropped or copied file is its own path, a clipboard image is first saved
//! as a file under `<home>\pastes`. The page pastes the paths into the terminal.
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tauri::Manager;

/// A clipboard image larger than this is refused rather than written.
const IMAGE_LIMIT: usize = 64 * 1024 * 1024;
/// Saved clipboard images older than this are removed when a new one is saved.
const KEEP_PASTES: Duration = Duration::from_secs(14 * 24 * 60 * 60);

fn pastes_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let home = match std::env::var_os("SHINATAMA_HOME") {
        Some(home) => PathBuf::from(home),
        None => app.path().app_config_dir().map_err(|e| e.to_string())?,
    };
    Ok(home.join("pastes"))
}

/// The file extension for an image, read from its first bytes; `None` if it is not one.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

fn prune(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| now.duration_since(modified).is_ok_and(|age| age > KEEP_PASTES));
        if old && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Saves the image in the request body and returns the file's absolute path.
#[tauri::command(async)]
pub fn terminal_paste_image(
    window: tauri::WebviewWindow,
    request: tauri::ipc::Request<'_>,
) -> Result<String, String> {
    crate::local(&window)?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("The pasted image did not arrive as bytes".into());
    };
    if bytes.len() > IMAGE_LIMIT {
        return Err("The pasted image is larger than 64 MiB".into());
    }
    let extension = image_extension(bytes).ok_or("The clipboard holds an image format the harnesses cannot read")?;
    let dir = pastes_dir(window.app_handle())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("The pastes folder could not be created: {e}"))?;
    prune(&dir);
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let path = dir.join(format!("paste-{stamp}-{}.{extension}", &uuid::Uuid::new_v4().simple().to_string()[..8]));
    std::fs::write(&path, bytes).map_err(|e| format!("The pasted image could not be saved: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

/// The files and folders copied to the clipboard (Explorer's Copy), in order; empty when the
/// clipboard holds none.
#[tauri::command(async)]
pub fn terminal_clipboard_files(window: tauri::WebviewWindow) -> Result<Vec<String>, String> {
    crate::local(&window)?;
    #[cfg(windows)]
    {
        clipboard_files()
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[cfg(windows)]
fn clipboard_files() -> Result<Vec<String>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};
    const CF_HDROP: u32 = 15;
    // SAFETY: plain Win32 clipboard calls; the handle is read only while the clipboard is open.
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP) == 0 {
            return Ok(Vec::new());
        }
        // Another program may hold the clipboard for a moment.
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !opened {
            return Err("The clipboard is in use by another program".into());
        }
        let handle = GetClipboardData(CF_HDROP) as HDROP;
        let mut paths = Vec::new();
        if !handle.is_null() {
            let count = DragQueryFileW(handle, u32::MAX, std::ptr::null_mut(), 0);
            for index in 0..count {
                let length = DragQueryFileW(handle, index, std::ptr::null_mut(), 0);
                let mut buffer = vec![0u16; length as usize + 1];
                let written = DragQueryFileW(handle, index, buffer.as_mut_ptr(), buffer.len() as u32);
                buffer.truncate(written as usize);
                paths.push(std::ffi::OsString::from_wide(&buffer).to_string_lossy().into_owned());
            }
        }
        CloseClipboard();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::image_extension;

    #[test]
    fn images_are_known_by_their_first_bytes() {
        assert_eq!(image_extension(b"\x89PNG\r\n\x1a\n...."), Some("png"));
        assert_eq!(image_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_extension(b"GIF89a.."), Some("gif"));
        assert_eq!(image_extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_extension(b"plain text"), None);
    }
}
