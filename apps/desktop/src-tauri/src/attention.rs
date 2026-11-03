//! Early Windows widget: transient local notices, never authority to answer a prompt.
//! Only explicit terminal notification controls and process lifecycle are observed.
use crate::{
    Host,
    attention_signals::{AttentionSignals, Signal},
};
use serde::{Deserialize, Serialize};
use shinatama_terminal::{OutputBatch, SessionInfo};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State};

const LIMIT: usize = 16;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Input,
    Complete,
    Error,
    Update,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    id: String,
    kind: Kind,
    title: String,
    body: String,
    session_id: Option<String>,
    created_at: u64,
}
#[derive(Serialize)]
pub struct Snapshot {
    enabled: bool,
    muted: bool,
    current: Option<Notice>,
}
#[derive(Default)]
struct Observed {
    parser: AttentionSignals,
    ended: bool,
    last_signal: Option<Instant>,
}
struct Feed {
    enabled: bool,
    muted: bool,
    queue: VecDeque<Notice>,
    sessions: HashMap<String, Observed>,
    last_sound: Option<Instant>,
}
impl Default for Feed {
    fn default() -> Self {
        Self {
            enabled: true,
            muted: false,
            queue: VecDeque::new(),
            sessions: HashMap::new(),
            last_sound: None,
        }
    }
}
impl Feed {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            enabled: self.enabled,
            muted: self.muted,
            current: self.queue.front().cloned(),
        }
    }
    fn push(&mut self, notice: Notice) -> bool {
        if !self.enabled {
            return false;
        }
        // A test must never build a permanent queue ahead of real terminal events.
        self.queue.retain(|old| old.session_id.is_some());
        // Coalesce outstanding notices for the same terminal; the latest fact wins.
        if let Some(session) = &notice.session_id {
            self.queue
                .retain(|old| old.session_id.as_ref() != Some(session));
        }
        if self.queue.len() == LIMIT {
            self.queue.pop_front();
        }
        self.queue.push_back(notice);
        true
    }
}
#[derive(Default)]
pub struct Controller(Mutex<Feed>);
fn lock(controller: &Controller) -> Result<std::sync::MutexGuard<'_, Feed>, String> {
    controller
        .0
        .lock()
        .map_err(|_| "Desktop alerts are unavailable.".into())
}
fn notice(kind: Kind, title: String, body: &str, session_id: Option<String>) -> Notice {
    Notice {
        id: uuid::Uuid::new_v4().to_string(),
        kind,
        title,
        body: body.into(),
        session_id,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    }
}
fn allowed(window: &tauri::WebviewWindow) -> Result<(), String> {
    if matches!(window.label(), "main" | "attention") {
        Ok(())
    } else {
        Err("Unknown alert window.".into())
    }
}
fn present(app: &tauri::AppHandle, controller: &Controller, sound: bool) {
    let Ok(mut feed) = lock(controller) else {
        return;
    };
    let visible = feed.enabled && !feed.queue.is_empty();
    let beep = sound
        && visible
        && !feed.muted
        && feed
            .last_sound
            .is_none_or(|last| last.elapsed() >= Duration::from_secs(3));
    if beep {
        feed.last_sound = Some(Instant::now());
    }
    drop(feed);
    if let Some(widget) = app.get_webview_window("attention") {
        let mut position = None;
        if visible {
            // Position in the main window's monitor work area, including negative
            // coordinates and DPI scaling; leave the taskbar unobstructed.
            if let Some(monitor) = app
                .get_webview_window("main")
                .and_then(|main| main.current_monitor().ok().flatten())
            {
                let area = monitor.work_area();
                let scale = monitor.scale_factor();
                let width = (440.0 * scale).ceil() as i32;
                let height = (190.0 * scale).ceil() as i32;
                let margin = (16.0 * scale).ceil() as i32;
                let x = area.position.x + (area.size.width as i32 - width - margin).max(0);
                let y = area.position.y + (area.size.height as i32 - height - margin).max(0);
                position = Some((x, y));
            }
        }
        place_without_activation(&widget, visible, position);
        let _ = widget.emit("attention-changed", ());
    }
    if beep {
        attention_sound();
    }
}
#[cfg(windows)]
fn place_without_activation(
    widget: &tauri::WebviewWindow,
    visible: bool,
    position: Option<(i32, i32)>,
) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(window: isize, command: i32) -> i32;
        fn SetWindowPos(
            window: isize,
            after: isize,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
    }
    if let Ok(handle) = widget.hwnd() {
        // WS_EX_NOACTIVATE alone does not constrain explicit programmatic
        // activation. Use the native non-activating show and positioning flags.
        unsafe {
            if visible {
                let (x, y) = position.unwrap_or((0, 0));
                let flags = 0x0011 | 0x0040 | if position.is_none() { 0x0002 } else { 0 };
                // NOACTIVATE | NOSIZE | SHOWWINDOW, optionally NOMOVE.
                SetWindowPos(handle.0 as isize, -1, x, y, 0, 0, flags);
            } else {
                ShowWindow(handle.0 as isize, 0);
            } // SW_HIDE
        }
    }
}
#[cfg(not(windows))]
fn place_without_activation(
    widget: &tauri::WebviewWindow,
    visible: bool,
    position: Option<(i32, i32)>,
) {
    if let Some((x, y)) = position {
        let _ = widget.set_position(tauri::PhysicalPosition::new(x, y));
    }
    if visible {
        let _ = widget.show();
    } else {
        let _ = widget.hide();
    }
}
#[cfg(windows)]
fn attention_sound() {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBeep(kind: u32) -> i32;
    }
    // Asynchronous OS sound; respects the user's Windows sound configuration.
    unsafe {
        MessageBeep(0x40);
    }
}
#[cfg(not(windows))]
fn attention_sound() {}

impl Controller {
    pub fn observe(&self, app: &tauri::AppHandle, session: &SessionInfo, batch: &OutputBatch) {
        let Ok(mut feed) = lock(self) else {
            return;
        };
        if !feed.sessions.contains_key(&session.id) && feed.sessions.len() >= 8 {
            if let Some(ended) = feed
                .sessions
                .iter()
                .find(|(_, observed)| observed.ended)
                .map(|(id, _)| id.clone())
            {
                feed.sessions.remove(&ended);
            } else {
                return;
            }
        }
        let observed = feed.sessions.entry(session.id.clone()).or_default();
        let mut signalled = false;
        for chunk in &batch.chunks {
            signalled |= observed
                .parser
                .feed(chunk)
                .iter()
                .any(|signal| matches!(signal, Signal::Bell | Signal::Notification));
        }
        let signal = signalled
            && observed
                .last_signal
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(5));
        if signal {
            observed.last_signal = Some(Instant::now());
        }
        let ended = batch.eof && !observed.ended;
        if batch.eof {
            observed.ended = true;
        }
        let event = if ended {
            Some((
                if batch.error.is_some() || batch.exit_code.is_some_and(|code| code != 0) {
                    Kind::Error
                } else {
                    Kind::Complete
                },
                "Il processo del terminale è terminato. Apri la sessione per vedere il risultato.",
            ))
        } else if signal {
            Some((
                Kind::Update,
                "Il terminale ha inviato un segnale di attenzione. Apri la sessione per leggere la richiesta o l'aggiornamento.",
            ))
        } else {
            None
        };
        let Some((kind, body)) = event else {
            return;
        };
        let harness = if session.shell == "claude_code" {
            "Claude Code"
        } else {
            "Terminale"
        };
        let project: String = session
            .cwd
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .take(48)
            .collect();
        let pushed = feed.push(notice(
            kind,
            format!("{harness} · {project}"),
            body,
            Some(session.id.clone()),
        ));
        drop(feed);
        if pushed {
            present(app, self, true);
        }
    }
    pub fn forget(&self, app: &tauri::AppHandle, id: &str) {
        if let Ok(mut feed) = lock(self) {
            feed.sessions.remove(id);
            feed.queue
                .retain(|notice| notice.session_id.as_deref() != Some(id));
        }
        present(app, self, false);
    }
}
#[tauri::command]
pub fn attention_current(
    window: tauri::WebviewWindow,
    controller: State<'_, Controller>,
) -> Result<Snapshot, String> {
    allowed(&window)?;
    Ok(lock(&controller)?.snapshot())
}
#[tauri::command]
pub fn attention_configure(
    window: tauri::WebviewWindow,
    controller: State<'_, Controller>,
    enabled: bool,
    muted: bool,
) -> Result<Snapshot, String> {
    crate::local(&window)?;
    let mut feed = lock(&controller)?;
    feed.enabled = enabled;
    feed.muted = muted;
    if !enabled {
        feed.queue.clear();
    }
    let snapshot = feed.snapshot();
    drop(feed);
    present(window.app_handle(), &controller, false);
    Ok(snapshot)
}
#[tauri::command]
pub fn attention_preview(
    window: tauri::WebviewWindow,
    controller: State<'_, Controller>,
    kind: Kind,
) -> Result<Snapshot, String> {
    crate::local(&window)?;
    let mut feed = lock(&controller)?;
    if !feed.enabled {
        return Err("Abilita gli avvisi desktop per provare Shinatama.".into());
    }
    let body = if kind == Kind::Input {
        "Questa è una prova: quando un terminale richiede attenzione comparirò qui. Apri la sessione per continuare."
    } else {
        "Questa è una prova di aggiornamento: il lavoro ha una novità da mostrarti."
    };
    feed.push(notice(kind, "Shinatama · prova avviso".into(), body, None));
    let snapshot = feed.snapshot();
    drop(feed);
    present(window.app_handle(), &controller, true);
    Ok(snapshot)
}
#[tauri::command]
pub fn attention_dismiss(
    window: tauri::WebviewWindow,
    controller: State<'_, Controller>,
    id: String,
) -> Result<(), String> {
    allowed(&window)?;
    let mut feed = lock(&controller)?;
    if feed.queue.front().is_none_or(|current| current.id != id) {
        return Err("This alert is no longer current.".into());
    }
    feed.queue.pop_front();
    drop(feed);
    present(window.app_handle(), &controller, false);
    Ok(())
}
#[tauri::command]
pub fn attention_open(
    window: tauri::WebviewWindow,
    controller: State<'_, Controller>,
    host: State<'_, Host>,
    id: String,
) -> Result<(), String> {
    allowed(&window)?;
    let mut feed = lock(&controller)?;
    let current = feed
        .queue
        .front()
        .filter(|current| current.id == id)
        .ok_or("This alert is no longer current.")?;
    let session_id = current.session_id.clone();
    if let Some(session) = &session_id
        && host.info(session).is_err()
    {
        return Err("This terminal is no longer available.".into());
    }
    let main = window
        .app_handle()
        .get_webview_window("main")
        .ok_or("The workspace is unavailable.")?;
    // Consume this exact notice atomically. A concurrent terminal event can then
    // enqueue a new notice without our click dismissing that newer one.
    feed.queue.pop_front();
    drop(feed);
    present(window.app_handle(), &controller, false);
    main.unminimize().map_err(|e| e.to_string())?;
    main.show().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())?;
    main.emit(
        "attention-focus",
        serde_json::json!({"sessionId":session_id}),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_is_bounded_coalesces_and_disabled_discards() {
        let mut feed = Feed::default();
        for _ in 0..100 {
            feed.push(notice(
                Kind::Update,
                "test".into(),
                "safe",
                Some("one".into()),
            ));
        }
        assert_eq!(feed.queue.len(), 1);
        for id in 0..100 {
            feed.push(notice(
                Kind::Update,
                "test".into(),
                "safe",
                Some(id.to_string()),
            ));
        }
        assert_eq!(feed.queue.len(), LIMIT);
        for _ in 0..100 {
            feed.push(notice(Kind::Input, "test".into(), "safe", None));
        }
        assert_eq!(
            feed.queue
                .iter()
                .filter(|notice| notice.session_id.is_none())
                .count(),
            1
        );
        feed.push(notice(
            Kind::Update,
            "real".into(),
            "safe",
            Some("real".into()),
        ));
        assert!(feed.queue.iter().all(|notice| notice.session_id.is_some()));
        feed.enabled = false;
        assert!(!feed.push(notice(Kind::Input, "test".into(), "safe", None)));
    }
}
