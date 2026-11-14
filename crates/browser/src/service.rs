//! One window's browser: the Moli host, its CDP connection and its pages.
//! Every page is a CDP target with a flattened session; frames go to the
//! page's sink as JPEG bytes, never as markup.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
};

use base64::Engine as _;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::{
    cdp::{Cdp, CdpError, CdpEvent},
    input::{InputEvent, to_cdp},
    pages::{Controller, DEFAULT_MAX_LIVE_PAGES, PageId, PageSet, PageState, local_allowed},
};

#[cfg(windows)]
use crate::host::{Host, HostError};

/// `moli serve` options of the host: real layout for frames and input, a
/// screencast fast enough for scrolling, optional resources for visible pages.
pub const HOST_ARGS: &[&str] = &["--layout", "--resource", "--screencast-interval-ms", "200"];
const JPEG_QUALITY: u32 = 70;

pub type FrameSink = Arc<dyn Fn(Vec<u8>) + Send + Sync>;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub controller: String,
    pub live: bool,
}

#[derive(Debug)]
pub enum BrowserError {
    #[cfg(windows)]
    Host(HostError),
    Cdp(CdpError),
    UnknownPage,
    Refused(&'static str),
}

impl std::fmt::Display for BrowserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(windows)]
            Self::Host(e) => e.fmt(f),
            Self::Cdp(e) => e.fmt(f),
            Self::UnknownPage => f.write_str("that page is closed"),
            Self::Refused(why) => f.write_str(why),
        }
    }
}

impl From<CdpError> for BrowserError {
    fn from(e: CdpError) -> Self {
        Self::Cdp(e)
    }
}

#[derive(Default)]
struct Sessions {
    by_page: HashMap<PageId, String>,
    by_session: HashMap<String, PageId>,
    sinks: HashMap<PageId, FrameSink>,
    viewport: HashMap<PageId, (u32, u32, f64)>,
}

pub struct Browser {
    cdp: Arc<Cdp>,
    pages: Arc<StdMutex<PageSet>>,
    sessions: Arc<StdMutex<Sessions>>,
    /// One lane per page: an input batch and the capture after it run alone.
    lanes: StdMutex<HashMap<PageId, Arc<Mutex<()>>>>,
    #[cfg(windows)]
    _host: Option<Host>,
}

/// Only web pages and local documents open; anything else is refused before
/// it reaches the engine.
pub fn openable(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("file:///") || lower == "about:blank"
}

impl Browser {
    #[cfg(windows)]
    pub async fn start(exe: &Path) -> Result<Self, BrowserError> {
        let exe = exe.to_owned();
        let host = tokio::task::spawn_blocking(move || Host::start(&exe, HOST_ARGS))
            .await
            .map_err(|_| BrowserError::Refused("the browser start was interrupted"))?
            .map_err(BrowserError::Host)?;
        let probe = host.connect().await.map_err(BrowserError::Host)?;
        let path = crate::cdp::discover(probe, host.token()).await?;
        let stream = host.connect().await.map_err(BrowserError::Host)?;
        let cdp = Cdp::connect(stream, host.token(), &path).await?;
        Ok(Self::with(cdp, Some(host)))
    }

    #[cfg(windows)]
    fn with(cdp: Cdp, host: Option<Host>) -> Self {
        let browser = Self {
            cdp: Arc::new(cdp),
            pages: Arc::new(StdMutex::new(PageSet::new(DEFAULT_MAX_LIVE_PAGES))),
            sessions: Arc::default(),
            lanes: StdMutex::default(),
            _host: host,
        };
        browser.listen();
        browser
    }

    #[cfg(not(windows))]
    fn with(cdp: Cdp, _host: Option<()>) -> Self {
        let browser = Self {
            cdp: Arc::new(cdp),
            pages: Arc::new(StdMutex::new(PageSet::new(DEFAULT_MAX_LIVE_PAGES))),
            sessions: Arc::default(),
            lanes: StdMutex::default(),
        };
        browser.listen();
        browser
    }

    pub fn is_alive(&self) -> bool {
        !self.cdp.is_closed()
    }

    pub fn page_count(&self) -> usize {
        self.pages.lock().unwrap().len()
    }

    fn listen(&self) {
        let mut events = self.cdp.events();
        let cdp = Arc::clone(&self.cdp);
        let pages = Arc::clone(&self.pages);
        let sessions = Arc::clone(&self.sessions);
        tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return,
                };
                handle_event(&cdp, &pages, &sessions, event).await;
            }
        });
    }

    fn session(&self, page: PageId) -> Result<String, BrowserError> {
        self.sessions.lock().unwrap().by_page.get(&page).cloned().ok_or(BrowserError::UnknownPage)
    }

    fn lane(&self, page: PageId) -> Arc<Mutex<()>> {
        Arc::clone(self.lanes.lock().unwrap().entry(page).or_default())
    }

    pub fn info(&self, page: PageId) -> Option<PageInfo> {
        let pages = self.pages.lock().unwrap();
        let p = pages.get(page)?;
        Some(PageInfo {
            id: page.to_string(),
            url: p.url.clone(),
            title: p.title.clone(),
            controller: match &p.controller {
                Controller::User => "user".into(),
                Controller::Agent(id) => format!("agent:{id}"),
            },
            live: matches!(p.state, PageState::Live { .. }),
        })
    }

    pub async fn open(&self, url: &str, controller: Controller, local_root: Option<PathBuf>) -> Result<PageId, BrowserError> {
        if !openable(url) {
            return Err(BrowserError::Refused("only http, https and local HTML documents open in the browser"));
        }
        let (id, discarded) = self.pages.lock().unwrap().open(url, controller, local_root);
        if let Some(old) = discarded {
            self.detach(old).await;
        }
        if let Err(error) = self.attach(id, url).await {
            self.pages.lock().unwrap().close(id);
            return Err(error);
        }
        Ok(id)
    }

    async fn attach(&self, id: PageId, url: &str) -> Result<(), BrowserError> {
        let created = self.cdp.call(None, "Target.createTarget", json!({ "url": "about:blank" })).await?;
        let target = created["targetId"].as_str().ok_or(BrowserError::Refused("no target id"))?.to_owned();
        let attached = self.cdp.call(None, "Target.attachToTarget", json!({ "targetId": target, "flatten": true })).await?;
        let session = attached["sessionId"].as_str().ok_or(BrowserError::Refused("no session id"))?.to_owned();
        {
            let mut s = self.sessions.lock().unwrap();
            s.by_page.insert(id, session.clone());
            s.by_session.insert(session.clone(), id);
        }
        self.pages.lock().unwrap().set_target(id, target);
        let session = Some(session.as_str());
        self.cdp.call(session, "Page.enable", json!({})).await?;
        let local = self.pages.lock().unwrap().get(id).and_then(|p| p.local_root.clone()).is_some();
        if local {
            self.cdp.call(session, "Fetch.enable", json!({ "patterns": [{ "urlPattern": "file://*" }] })).await?;
        }
        let viewport = self.sessions.lock().unwrap().viewport.get(&id).copied();
        if let Some((width, height, dpr)) = viewport {
            self.metrics(session, width, height, dpr).await?;
        }
        self.cdp.call(session, "Page.navigate", json!({ "url": url })).await?;
        let visible = self.pages.lock().unwrap().get(id).is_some_and(|p| p.visible);
        if visible {
            self.screencast(id, true).await?;
        }
        Ok(())
    }

    async fn detach(&self, id: PageId) {
        let session = {
            let mut s = self.sessions.lock().unwrap();
            let session = s.by_page.remove(&id);
            if let Some(session) = &session {
                s.by_session.remove(session);
            }
            session
        };
        let target = match self.pages.lock().unwrap().get(id).map(|p| p.state.clone()) {
            Some(PageState::Live { target: Some(target) }) => Some(target),
            _ => None,
        };
        if let Some(target) = target {
            let _ = self.cdp.call(None, "Target.closeTarget", json!({ "targetId": target })).await;
        }
        drop(session);
    }

    pub fn set_sink(&self, page: PageId, sink: FrameSink) {
        self.sessions.lock().unwrap().sinks.insert(page, sink);
    }

    pub async fn navigate(&self, page: PageId, action: &str, url: Option<&str>) -> Result<(), BrowserError> {
        let session = self.session(page)?;
        let session = Some(session.as_str());
        match (action, url) {
            ("go", Some(url)) if openable(url) => {
                self.cdp.call(session, "Page.navigate", json!({ "url": url })).await?;
            }
            ("go", _) => return Err(BrowserError::Refused("only http, https and local HTML documents open in the browser")),
            ("reload", _) => {
                self.cdp.call(session, "Page.reload", json!({})).await?;
            }
            ("back" | "forward", _) => {
                let history = self.cdp.call(session, "Page.getNavigationHistory", json!({})).await?;
                let current = history["currentIndex"].as_i64().unwrap_or(0);
                let target = if action == "back" { current - 1 } else { current + 1 };
                let entry = history["entries"].as_array().and_then(|e| e.get(usize::try_from(target).ok()?)).cloned();
                if let Some(id) = entry.and_then(|e| e["id"].as_i64()) {
                    self.cdp.call(session, "Page.navigateToHistoryEntry", json!({ "entryId": id })).await?;
                }
            }
            _ => return Err(BrowserError::Refused("unknown navigation")),
        }
        Ok(())
    }

    /// Sends one batch in order, then pushes a fresh frame: the screencast
    /// only emits on its own interval.
    pub async fn input(&self, page: PageId, events: &[InputEvent]) -> Result<(), BrowserError> {
        let lane = self.lane(page);
        let _turn = lane.lock().await;
        let session = self.session(page)?;
        for event in events {
            if let Some((method, params)) = to_cdp(event) {
                self.cdp.call(Some(&session), method, params).await?;
            }
        }
        let frame = self.capture(&session).await?;
        if let Some(sink) = self.sessions.lock().unwrap().sinks.get(&page).cloned() {
            sink(frame);
        }
        Ok(())
    }

    async fn capture(&self, session: &str) -> Result<Vec<u8>, BrowserError> {
        let shot = self
            .cdp
            .call(Some(session), "Page.captureScreenshot", json!({ "format": "jpeg", "quality": JPEG_QUALITY }))
            .await?;
        decode(&shot["data"])
    }

    pub async fn screenshot_png(&self, page: PageId) -> Result<Vec<u8>, BrowserError> {
        let session = self.session(page)?;
        let shot = self.cdp.call(Some(&session), "Page.captureScreenshot", json!({ "format": "png" })).await?;
        decode(&shot["data"])
    }

    async fn metrics(&self, session: Option<&str>, width: u32, height: u32, dpr: f64) -> Result<(), BrowserError> {
        self.cdp
            .call(session, "Emulation.setDeviceMetricsOverride", json!({
                "width": width, "height": height, "deviceScaleFactor": dpr, "mobile": false,
            }))
            .await?;
        Ok(())
    }

    pub async fn resize(&self, page: PageId, width: u32, height: u32, dpr: f64) -> Result<(), BrowserError> {
        let (width, height) = (width.clamp(64, 8192), height.clamp(64, 8192));
        let dpr = if dpr.is_finite() { dpr.clamp(0.5, 4.0) } else { 1.0 };
        self.sessions.lock().unwrap().viewport.insert(page, (width, height, dpr));
        let Ok(session) = self.session(page) else { return Ok(()) };
        self.metrics(Some(&session), width, height, dpr).await?;
        let visible = self.pages.lock().unwrap().get(page).is_some_and(|p| p.visible);
        if visible {
            self.screencast(page, true).await?;
        }
        Ok(())
    }

    async fn screencast(&self, page: PageId, on: bool) -> Result<(), BrowserError> {
        let session = self.session(page)?;
        if on {
            let (width, height, dpr) = self.sessions.lock().unwrap().viewport.get(&page).copied().unwrap_or((1280, 800, 1.0));
            self.cdp
                .call(Some(&session), "Page.startScreencast", json!({
                    "format": "jpeg", "quality": JPEG_QUALITY,
                    "maxWidth": (width as f64 * dpr) as u32, "maxHeight": (height as f64 * dpr) as u32,
                }))
                .await?;
            let frame = self.capture(&session).await?;
            if let Some(sink) = self.sessions.lock().unwrap().sinks.get(&page).cloned() {
                sink(frame);
            }
        } else {
            self.cdp.call(Some(&session), "Page.stopScreencast", json!({})).await?;
        }
        Ok(())
    }

    pub async fn set_visible(&self, page: PageId, visible: bool) -> Result<(), BrowserError> {
        if visible {
            let reviving = matches!(self.pages.lock().unwrap().get(page).map(|p| &p.state), Some(PageState::Discarded));
            let discarded = self.pages.lock().unwrap().shown(page).map_err(|_| BrowserError::UnknownPage)?;
            if let Some(old) = discarded {
                self.detach(old).await;
            }
            if reviving {
                let url = self.pages.lock().unwrap().get(page).map(|p| p.url.clone()).unwrap_or_default();
                return self.attach(page, &url).await;
            }
            self.screencast(page, true).await
        } else {
            self.pages.lock().unwrap().hidden(page);
            match self.session(page) {
                Ok(_) => self.screencast(page, false).await,
                Err(_) => Ok(()),
            }
        }
    }

    pub async fn close(&self, page: PageId) {
        self.detach(page).await;
        self.pages.lock().unwrap().close(page);
        let mut s = self.sessions.lock().unwrap();
        s.sinks.remove(&page);
        s.viewport.remove(&page);
        self.lanes.lock().unwrap().remove(&page);
    }

    pub fn take_control(&self, page: PageId) -> Result<(), BrowserError> {
        self.pages.lock().unwrap().take_control(page).map_err(|_| BrowserError::UnknownPage)
    }

    pub fn give_back(&self, page: PageId) -> Result<(), BrowserError> {
        self.pages.lock().unwrap().give_back(page).map_err(|_| BrowserError::Refused("this page was not opened by an agent"))
    }

    pub fn agent_may_act(&self, page: PageId, instance: &str) -> bool {
        self.pages.lock().unwrap().agent_may_act(page, instance)
    }

    pub async fn evaluate(&self, page: PageId, expression: &str) -> Result<Value, BrowserError> {
        let session = self.session(page)?;
        let result = self
            .cdp
            .call(Some(&session), "Runtime.evaluate", json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }))
            .await?;
        Ok(result["result"]["value"].clone())
    }

    pub async fn call(&self, page: PageId, method: &str, params: Value) -> Result<Value, BrowserError> {
        let session = self.session(page)?;
        Ok(self.cdp.call(Some(&session), method, params).await?)
    }
}

fn decode(data: &Value) -> Result<Vec<u8>, BrowserError> {
    let text = data.as_str().ok_or(BrowserError::Refused("no image data"))?;
    base64::engine::general_purpose::STANDARD.decode(text).map_err(|_| BrowserError::Refused("bad image data"))
}

async fn handle_event(cdp: &Cdp, pages: &StdMutex<PageSet>, sessions: &StdMutex<Sessions>, event: CdpEvent) {
    let Some(session) = event.session.clone() else {
        if event.method == "Target.targetInfoChanged" {
            let info = &event.params["targetInfo"];
            let target = info["targetId"].as_str().unwrap_or_default();
            let mut pages = pages.lock().unwrap();
            let found = pages.ids().into_iter().find(|id| {
                matches!(pages.get(*id).map(|p| &p.state), Some(PageState::Live { target: Some(t) }) if t == target)
            });
            if let Some(id) = found {
                pages.set_location(id, info["url"].as_str().unwrap_or_default(), info["title"].as_str().unwrap_or_default());
            }
        }
        return;
    };
    let Some(page) = sessions.lock().unwrap().by_session.get(&session).copied() else { return };
    match event.method.as_str() {
        "Page.screencastFrame" => {
            let ack = event.params["sessionId"].clone();
            let _ = cdp.call(Some(&session), "Page.screencastFrameAck", json!({ "sessionId": ack })).await;
            let sink = sessions.lock().unwrap().sinks.get(&page).cloned();
            if let (Some(sink), Ok(frame)) = (sink, decode(&event.params["data"])) {
                sink(frame);
            }
        }
        "Page.frameNavigated" if event.params["frame"]["parentId"].is_null() => {
            let url = event.params["frame"]["url"].as_str().unwrap_or_default().to_owned();
            let mut pages = pages.lock().unwrap();
            let title = pages.get(page).map(|p| p.title.clone()).unwrap_or_default();
            pages.set_location(page, &url, &title);
        }
        "Fetch.requestPaused" => {
            let request = event.params["requestId"].clone();
            let url = event.params["request"]["url"].as_str().unwrap_or_default();
            let root = pages.lock().unwrap().get(page).and_then(|p| p.local_root.clone());
            let allowed = root.as_deref().is_some_and(|root: &Path| local_allowed(root, url));
            let _ = if allowed {
                cdp.call(Some(&session), "Fetch.continueRequest", json!({ "requestId": request })).await
            } else {
                cdp.call(Some(&session), "Fetch.failRequest", json!({ "requestId": request, "errorReason": "AccessDenied" })).await
            };
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{KeyKind, MouseKind};
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::AsyncReadExt;
    use tokio_tungstenite::tungstenite::Message;

    /// A fake Moli: answers every call, records methods in order, makes the
    /// first screenshot slow so a second batch queues behind it.
    async fn fake() -> (Browser, Arc<StdMutex<Vec<String>>>) {
        let (client, mut server) = tokio::io::duplex(1 << 20);
        let log: Arc<StdMutex<Vec<String>>> = Arc::default();
        let record = Arc::clone(&log);
        tokio::spawn(async move {
            let mut token = [0u8; 32];
            server.read_exact(&mut token).await.unwrap();
            let socket = tokio_tungstenite::accept_async(server).await.unwrap();
            let (mut sink, mut source) = socket.split();
            let mut shots = 0;
            while let Some(Ok(Message::Text(text))) = source.next().await {
                let request: Value = serde_json::from_str(&text).unwrap();
                let method = request["method"].as_str().unwrap().to_owned();
                let detail = match method.as_str() {
                    "Input.dispatchKeyEvent" => format!("{method}:{}", request["params"]["key"].as_str().unwrap()),
                    _ => method.clone(),
                };
                record.lock().unwrap().push(detail);
                let result = match method.as_str() {
                    "Target.createTarget" => json!({ "targetId": "T1" }),
                    "Target.attachToTarget" => json!({ "sessionId": "S1" }),
                    "Page.captureScreenshot" => {
                        shots += 1;
                        if shots == 1 {
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        }
                        json!({ "data": "/9j/" })
                    }
                    _ => json!({}),
                };
                sink.send(Message::text(json!({ "id": request["id"], "result": result }).to_string())).await.unwrap();
            }
        });
        let cdp = Cdp::connect(client, &[0; 32], "/devtools/browser/x").await.unwrap();
        (Browser::with(cdp, None), log)
    }

    fn key(k: &str) -> InputEvent {
        InputEvent::Key { kind: KeyKind::Down, key: k.into(), code: String::new(), text: Some(k.into()), modifiers: 0, key_code: 0 }
    }

    #[tokio::test]
    async fn input_batches_keep_order_under_capture() {
        let (browser, log) = fake().await;
        let browser = Arc::new(browser);
        let page = browser.open("https://example.com/", Controller::User, None).await.unwrap();
        let frames: Arc<StdMutex<usize>> = Arc::default();
        let counted = Arc::clone(&frames);
        browser.set_sink(page, Arc::new(move |_| *counted.lock().unwrap() += 1));
        log.lock().unwrap().clear();
        let (a, b) = (Arc::clone(&browser), Arc::clone(&browser));
        let first = tokio::spawn(async move { a.input(page, &[key("a"), key("b")]).await });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let second = tokio::spawn(async move { b.input(page, &[key("c")]).await });
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            [
                "Input.dispatchKeyEvent:a",
                "Input.dispatchKeyEvent:b",
                "Page.captureScreenshot",
                "Input.dispatchKeyEvent:c",
                "Page.captureScreenshot"
            ]
        );
        assert_eq!(*frames.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn open_attaches_a_flat_session_and_navigates() {
        let (browser, log) = fake().await;
        let page = browser.open("https://example.com/", Controller::User, None).await.unwrap();
        assert_eq!(*log.lock().unwrap(), ["Target.createTarget", "Target.attachToTarget", "Page.enable", "Page.navigate"]);
        assert_eq!(browser.info(page).unwrap().controller, "user");
    }

    #[tokio::test]
    async fn a_local_document_enables_file_interception() {
        let (browser, log) = fake().await;
        browser.open("file:///D:/site/index.html", Controller::User, Some(PathBuf::from("D:/site"))).await.unwrap();
        assert!(log.lock().unwrap().contains(&"Fetch.enable".to_owned()));
    }

    #[tokio::test]
    async fn other_schemes_never_reach_the_engine() {
        let (browser, log) = fake().await;
        for url in ["mailto:a@b.c", "javascript:alert(1)", "vscode://x", "chrome://settings"] {
            assert!(matches!(browser.open(url, Controller::User, None).await, Err(BrowserError::Refused(_))), "{url}");
        }
        assert!(log.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn bad_mouse_values_are_skipped_not_sent() {
        let (browser, log) = fake().await;
        let page = browser.open("https://example.com/", Controller::User, None).await.unwrap();
        log.lock().unwrap().clear();
        let bad = InputEvent::Mouse { kind: MouseKind::Moved, x: f64::INFINITY, y: 0.0, button: "none".into(), buttons: 0, modifiers: 0, click_count: 0 };
        browser.input(page, &[bad]).await.unwrap();
        assert_eq!(*log.lock().unwrap(), ["Page.captureScreenshot"]);
    }
}
