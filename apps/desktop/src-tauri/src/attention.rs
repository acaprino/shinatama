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
    last_output: Option<Instant>,
    exit_code: Option<u32>,
}
/// What the terminal stream alone says about one session (F04). Only `ended` is certain.
#[derive(Clone, Copy, Debug)]
pub struct TerminalObservation {
    pub ended: bool,
    pub exit_code: Option<u32>,
    pub since_signal_ms: Option<u64>,
    pub since_output_ms: Option<u64>,
}
struct Feed {
    enabled: bool,
    muted: bool,
    queue: VecDeque<Notice>,
    sessions: HashMap<String, Observed>,
    last_sound: Option<Instant>,
    last_saved: Option<Instant>,
    /// Errors alternate between two sounds; true when the next one is the second.
    second_error: bool,
}
impl Default for Feed {
    fn default() -> Self {
        Self {
            enabled: true,
            muted: false,
            queue: VecDeque::new(),
            sessions: HashMap::new(),
            last_sound: None,
            last_saved: None,
            second_error: false,
        }
    }
}
impl Feed {
    /// The sound for an alert that is about to play.
    fn cue(&mut self, kind: Kind) -> Cue {
        match kind {
            Kind::Error => {
                let second = self.second_error;
                self.second_error = !second;
                if second { Cue::SecondError } else { Cue::Error }
            }
            other => Cue::from(other),
        }
    }
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
    let beep = (sound
        && visible
        && !feed.muted
        && feed
            .last_sound
            .is_none_or(|last| last.elapsed() >= Duration::from_secs(3)))
    .then(|| feed.queue.front().map(|current| current.kind))
    .flatten()
    .map(|kind| feed.cue(kind));
    if beep.is_some() {
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
    if let Some(cue) = beep {
        play(cue);
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
/// One short sound per kind of event, so the event can be told apart by ear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cue {
    Input,
    Complete,
    Error,
    SecondError,
    Update,
    Saved,
}
impl From<Kind> for Cue {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Input => Cue::Input,
            Kind::Complete => Cue::Complete,
            Kind::Error => Cue::Error,
            Kind::Update => Cue::Update,
        }
    }
}
impl Cue {
    fn wav(self) -> &'static [u8] {
        match self {
            // An input request shares the save sound (user, 2026-11-11).
            Cue::Input => include_bytes!("../sounds/saved.wav"),
            Cue::Complete => include_bytes!("../sounds/complete.wav"),
            Cue::Error => include_bytes!("../sounds/error.wav"),
            Cue::SecondError => include_bytes!("../sounds/error2.wav"),
            Cue::Update => include_bytes!("../sounds/update.wav"),
            Cue::Saved => include_bytes!("../sounds/saved.wav"),
        }
    }
}
#[cfg(windows)]
fn play(cue: Cue) {
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(sound: *const u8, module: isize, flags: u32) -> i32;
    }
    // SND_ASYNC | SND_NODEFAULT | SND_MEMORY: the embedded WAV is 'static, so it
    // outlives the asynchronous playback. A newer cue replaces one still playing.
    unsafe {
        PlaySoundW(cue.wav().as_ptr(), 0, 0x0001 | 0x0002 | 0x0004);
    }
}
#[cfg(not(windows))]
fn play(_cue: Cue) {}

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
        if batch.chunks.iter().any(|c| !c.is_empty()) {
            observed.last_output = Some(Instant::now());
        }
        if batch.eof {
            observed.exit_code = batch.exit_code;
        }
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
    /// A file the user saved: a short confirmation sound unless sound is muted.
    /// It never cuts an alert sound that started a moment ago, and rapid saves
    /// sound once.
    pub fn saved(&self) {
        let Ok(mut feed) = lock(self) else {
            return;
        };
        let recent = |last: Option<Instant>, gap: u64| {
            last.is_some_and(|last| last.elapsed() < Duration::from_millis(gap))
        };
        if feed.muted || recent(feed.last_sound, 1500) || recent(feed.last_saved, 1000) {
            return;
        }
        feed.last_saved = Some(Instant::now());
        drop(feed);
        play(Cue::Saved);
    }
    pub fn observations(&self) -> HashMap<String, TerminalObservation> {
        let Ok(feed) = lock(self) else { return HashMap::new() };
        feed.sessions
            .iter()
            .map(|(id, o)| (id.clone(), TerminalObservation {
                ended: o.ended,
                exit_code: o.exit_code,
                since_signal_ms: o.last_signal.map(|t| t.elapsed().as_millis() as u64),
                since_output_ms: o.last_output.map(|t| t.elapsed().as_millis() as u64),
            }))
            .collect()
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
    fn every_kind_has_an_embedded_pcm_wav() {
        let cues = [Kind::Input, Kind::Complete, Kind::Error, Kind::Update]
            .map(Cue::from)
            .into_iter()
            .chain([Cue::SecondError, Cue::Saved])
            .collect::<Vec<_>>();
        for cue in &cues {
            let wav = cue.wav();
            assert_eq!(&wav[0..4], b"RIFF", "{cue:?}");
            assert_eq!(&wav[8..12], b"WAVE", "{cue:?}");
            // PCM, short enough to stay a brief cue (under two seconds of 16-bit mono 44.1 kHz).
            assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1, "{cue:?}");
            assert!(wav.len() < 2 * 44_100 * 2, "{cue:?}");
        }
        // Only an input request and a save share a sound; every other pair differs.
        assert_eq!(Cue::Input.wav(), Cue::Saved.wav());
        for (i, a) in cues.iter().enumerate() {
            for b in &cues[i + 1..] {
                if matches!((a, b), (Cue::Input, Cue::Saved) | (Cue::Saved, Cue::Input)) {
                    continue;
                }
                assert_ne!(a.wav(), b.wav(), "{a:?} and {b:?} share a sound");
            }
        }
    }
    #[test]
    fn errors_alternate_between_two_sounds_and_other_kinds_do_not() {
        let mut feed = Feed::default();
        assert_eq!(feed.cue(Kind::Error), Cue::Error);
        assert_eq!(feed.cue(Kind::Update), Cue::Update);
        assert_eq!(feed.cue(Kind::Error), Cue::SecondError);
        assert_eq!(feed.cue(Kind::Complete), Cue::Complete);
        assert_eq!(feed.cue(Kind::Error), Cue::Error);
        assert_eq!(feed.cue(Kind::Input), Cue::Input);
    }
    /// Manual audition: `cargo test --bin shinatama-desktop audition -- --ignored`
    /// plays every cue in turn and fails if Windows refuses one.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn audition_every_cue() {
        #[link(name = "winmm")]
        unsafe extern "system" {
            fn PlaySoundW(sound: *const u8, module: isize, flags: u32) -> i32;
        }
        for cue in [
            Cue::Complete,
            Cue::Error,
            Cue::SecondError,
            Cue::Update,
            Cue::Saved,
        ] {
            // SND_SYNC | SND_NODEFAULT | SND_MEMORY
            let played = unsafe { PlaySoundW(cue.wav().as_ptr(), 0, 0x0002 | 0x0004) };
            assert_eq!(played, 1, "{cue:?}");
            std::thread::sleep(Duration::from_millis(400));
        }
    }
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
