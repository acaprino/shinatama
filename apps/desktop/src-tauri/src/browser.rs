//! The browser tabs' commands (ADR-0056). One Moli host for the app, started
//! by the first page and ended after ten idle minutes or with the app. Frames
//! reach the tab as raw JPEG bytes over a channel, never as markup.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use shinatama_browser::{
    input::InputEvent,
    local_server::LocalServer,
    pages::{Controller, PageId},
    service::{Browser, PageInfo},
};
use tauri::{
    State,
    ipc::{Channel, InvokeResponseBody},
};
use tokio::sync::Mutex;

pub const IDLE: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
pub struct BrowserState {
    inner: Mutex<Inner>,
    /// Serves the local documents opened in tabs; started by the first one.
    documents: std::sync::OnceLock<LocalServer>,
    /// The address granted to each page opened on a local document, revoked
    /// with the page whatever it shows by then.
    grants: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

/// Ends a browser off the async runtime: dropping its host waits for Moli to
/// flush its storage (up to 6 s).
fn retire(browser: Arc<Browser>) {
    let _ = std::thread::Builder::new()
        .name("browser-retire".into())
        .spawn(move || drop(browser));
}

#[derive(Default)]
struct Inner {
    browser: Option<Arc<Browser>>,
    empty_since: Option<Instant>,
}

fn host_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = exe.with_file_name(shinatama_browser::host::HOST_EXE);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{} is missing next to Shinatama.",
            shinatama_browser::host::HOST_EXE
        ))
    }
}

impl BrowserState {
    fn documents(&self) -> Result<&LocalServer, String> {
        if let Some(server) = self.documents.get() {
            return Ok(server);
        }
        let server =
            LocalServer::start().map_err(|e| format!("Local documents cannot be served: {e}"))?;
        Ok(self.documents.get_or_init(|| server))
    }

    pub(crate) async fn browser(&self) -> Result<Arc<Browser>, String> {
        let mut inner = self.inner.lock().await;
        if let Some(browser) = inner.browser.as_ref().filter(|b| b.is_alive()) {
            return Ok(Arc::clone(browser));
        }
        if let Some(dead) = inner.browser.take() {
            retire(dead);
        }
        let browser = Arc::new(
            Browser::start(&host_exe()?)
                .await
                .map_err(|e| e.to_string())?,
        );
        inner.browser = Some(Arc::clone(&browser));
        inner.empty_since = None;
        Ok(browser)
    }

    pub(crate) async fn existing(&self) -> Result<Arc<Browser>, String> {
        self.inner
            .lock()
            .await
            .browser
            .clone()
            .ok_or_else(|| "The browser is not running.".to_owned())
    }

    /// Ends the host once it has had no page for [`IDLE`].
    pub async fn sweep(&self) {
        let mut inner = self.inner.lock().await;
        let empty = inner.browser.as_ref().is_some_and(|b| b.page_count() == 0);
        match (empty, inner.empty_since) {
            (false, _) => inner.empty_since = None,
            (true, None) => inner.empty_since = Some(Instant::now()),
            (true, Some(since)) if since.elapsed() >= IDLE => {
                if let Some(idle) = inner.browser.take() {
                    retire(idle);
                }
                inner.empty_since = None;
            }
            (true, Some(_)) => {}
        }
    }

    pub fn start_sweeper(state: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                state.sweep().await;
            }
        });
    }
}

fn page(id: &str) -> Result<PageId, String> {
    id.parse().map_err(|()| "Unknown page.".to_owned())
}

/// A local document: an existing `.html`/`.htm` file and its folder.
pub fn local_document(path: &str) -> Result<(PathBuf, PathBuf), String> {
    let path = std::fs::canonicalize(Path::new(path))
        .map_err(|_| "That file does not exist.".to_owned())?;
    let html = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"));
    if !html || !path.is_file() {
        return Err("Only .html and .htm files open in the browser.".into());
    }
    let shown = PathBuf::from(path.to_string_lossy().trim_start_matches(r"\\?\"));
    let root = shown.parent().ok_or("That file has no folder.")?.to_owned();
    Ok((shown, root))
}

#[tauri::command]
pub async fn browser_open(
    state: State<'_, Arc<BrowserState>>,
    url: Option<String>,
    file: Option<String>,
) -> Result<PageInfo, String> {
    let (url, root) = match (url, file) {
        (Some(url), None) => (url, None),
        (None, Some(file)) => {
            // Read only through the document server, which serves this folder and nothing above it.
            let (file, root) = local_document(&file)?;
            let url = state
                .documents()?
                .grant(&root, &file)
                .ok_or("That file is outside its folder.")?;
            (url, Some(root))
        }
        _ => return Err("Give either a link or a file.".into()),
    };
    let browser = state.browser().await?;
    let granted = root.is_some().then(|| url.clone());
    let opened = browser.open(&url, Controller::User, root).await;
    let id = match opened {
        Ok(id) => id,
        Err(error) => {
            if let (Some(url), Some(documents)) = (&granted, state.documents.get()) {
                documents.revoke(url);
            }
            return Err(error.to_string());
        }
    };
    if let Some(url) = granted {
        state.grants.lock().unwrap().insert(id.to_string(), url);
    }
    browser.info(id).ok_or_else(|| "The page closed.".into())
}

#[tauri::command]
pub async fn browser_info(
    state: State<'_, Arc<BrowserState>>,
    page: String,
) -> Result<PageInfo, String> {
    state
        .existing()
        .await?
        .info(self::page(&page)?)
        .ok_or_else(|| "The page closed.".into())
}

#[tauri::command]
pub async fn browser_frames(
    state: State<'_, Arc<BrowserState>>,
    page: String,
    channel: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let browser = state.existing().await?;
    browser.set_sink(
        self::page(&page)?,
        Arc::new(move |frame| {
            let _ = channel.send(InvokeResponseBody::Raw(frame));
        }),
    );
    Ok(())
}

#[tauri::command]
pub async fn browser_navigate(
    state: State<'_, Arc<BrowserState>>,
    page: String,
    action: String,
    url: Option<String>,
) -> Result<(), String> {
    state
        .existing()
        .await?
        .navigate(self::page(&page)?, &action, url.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_input(
    state: State<'_, Arc<BrowserState>>,
    page: String,
    events: Vec<InputEvent>,
) -> Result<(), String> {
    if events.len() > 256 {
        return Err("Too many input events at once.".into());
    }
    let browser = state.existing().await?;
    let id = self::page(&page)?;
    // The user acting on an agent's page takes control of it (FRD-0011 R25).
    if !matches!(browser.info(id).map(|i| i.controller), Some(c) if c == "user") {
        return Err("An agent is using this page. Take control first.".into());
    }
    browser.input(id, &events).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_resize(
    state: State<'_, Arc<BrowserState>>,
    page: String,
    width: u32,
    height: u32,
    dpr: f64,
) -> Result<(), String> {
    state
        .existing()
        .await?
        .resize(self::page(&page)?, width, height, dpr)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_visibility(
    state: State<'_, Arc<BrowserState>>,
    page: String,
    visible: bool,
) -> Result<(), String> {
    state
        .existing()
        .await?
        .set_visible(self::page(&page)?, visible)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_control(
    state: State<'_, Arc<BrowserState>>,
    page: String,
    take: bool,
) -> Result<PageInfo, String> {
    let browser = state.existing().await?;
    let id = self::page(&page)?;
    if take {
        browser.take_control(id)
    } else {
        browser.give_back(id)
    }
    .map_err(|e| e.to_string())?;
    browser.info(id).ok_or_else(|| "The page closed.".into())
}

#[tauri::command]
pub async fn browser_close(
    state: State<'_, Arc<BrowserState>>,
    page: String,
) -> Result<(), String> {
    if let Ok(browser) = state.existing().await {
        let id = self::page(&page)?;
        let granted = state.grants.lock().unwrap().remove(&page);
        if let (Some(url), Some(documents)) = (granted, state.documents.get()) {
            documents.revoke(&url);
        }
        browser.close(id).await;
    }
    Ok(())
}

/// Hands the page's URL, and nothing else, to the system browser.
#[tauri::command]
pub async fn browser_external(
    state: State<'_, Arc<BrowserState>>,
    page: String,
) -> Result<(), String> {
    let info = state
        .existing()
        .await?
        .info(self::page(&page)?)
        .ok_or("The page closed.")?;
    let lower = info.url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("Only web pages open in the system browser.".into());
    }
    open_in_system_browser(&info.url)
}

#[cfg(windows)]
fn open_in_system_browser(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    const SW_SHOWNORMAL: i32 = 1;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, target) = (wide("open"), wide(url));
    // SAFETY: NUL-terminated wide strings that outlive the call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err("The system browser did not open.".into())
    }
}

#[cfg(not(windows))]
fn open_in_system_browser(_url: &str) -> Result<(), String> {
    Err("Windows only.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_document_needs_an_existing_html_file() {
        let dir =
            std::env::temp_dir().join(format!("shinatama-browser-desktop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("a b.html");
        std::fs::write(&page, "<p>x").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let (file, root) = local_document(page.to_str().unwrap()).unwrap();
        assert_eq!(file.file_name().unwrap(), "a b.html");
        assert_eq!(root.file_name(), dir.file_name());
        assert!(local_document(dir.join("notes.txt").to_str().unwrap()).is_err());
        assert!(local_document(dir.join("missing.html").to_str().unwrap()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn sweep_without_a_browser_keeps_nothing() {
        let state = BrowserState::default();
        state.sweep().await;
        assert!(state.inner.lock().await.browser.is_none());
    }
}
