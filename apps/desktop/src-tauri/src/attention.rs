//! Early Windows widget: transient local notices, never authority to answer a prompt.
//! Terminal signals, lifecycle, and advisory host memory warnings are observed.
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
const MIB: u64 = 1024 * 1024;
const MEMORY_WARN_BYTES: u64 = 512 * MIB;
const MEMORY_RECOVER_BYTES: u64 = 768 * MIB;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Input,
    Complete,
    Error,
    Update,
    Memory,
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
    memory_pressure: bool,
    memory_dismissed: bool,
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
            memory_pressure: false,
            memory_dismissed: false,
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
        self.queue
            .retain(|old| old.kind == Kind::Memory || old.session_id.is_some());
        // Coalesce outstanding notices for the same terminal; the latest fact wins.
        if notice.kind == Kind::Memory {
            self.queue.retain(|old| old.kind != Kind::Memory);
        } else if let Some(session) = &notice.session_id {
            self.queue
                .retain(|old| old.kind == Kind::Memory || old.session_id.as_ref() != Some(session));
        }
        if self.queue.len() == LIMIT {
            self.queue.pop_front();
        }
        if notice.kind != Kind::Memory
            && let Some(position) = self.queue.iter().position(|old| old.kind == Kind::Memory)
        {
            // A persistent advisory must never hide a new session event.
            self.queue.insert(position, notice);
        } else {
            self.queue.push_back(notice);
        }
        true
    }

    fn observe_memory(
        &mut self,
        memory: Option<crate::resources::MemoryObservation>,
        consumer: Option<&MemoryConsumer>,
        partial: bool,
    ) -> MemoryChange {
        let Some(memory) = memory else {
            return self.remove_memory_notice();
        };
        let low = memory.physical_available_bytes < MEMORY_WARN_BYTES
            || memory.commit_available_bytes < MEMORY_WARN_BYTES;
        let recovered = memory.physical_available_bytes >= MEMORY_RECOVER_BYTES
            && memory.commit_available_bytes >= MEMORY_RECOVER_BYTES;
        if recovered {
            self.memory_pressure = false;
            self.memory_dismissed = false;
            return self.remove_memory_notice();
        }
        let new_episode = low && !self.memory_pressure;
        self.memory_pressure |= low;
        if !self.enabled || !self.memory_pressure || self.memory_dismissed {
            return MemoryChange::None;
        }
        let headroom = if memory.physical_available_bytes < MEMORY_RECOVER_BYTES {
            format!(
                "RAM disponibile: {} MiB.",
                memory.physical_available_bytes / MIB
            )
        } else {
            format!(
                "Memoria allocabile: {} MiB.",
                memory.commit_available_bytes / MIB
            )
        };
        let (title, body, session_id) = if let Some(consumer) = consumer {
            let ranking = if partial {
                "Più consumo tra i terminali misurati"
            } else {
                "Più consumo tra i terminali"
            };
            (
                consumer.label.clone(),
                format!(
                    "{headroom} {ranking}: {} MiB allocati. Il lavoro continua.",
                    consumer.bytes / MIB
                ),
                Some(consumer.id.clone()),
            )
        } else {
            (
                "Memoria del PC in esaurimento".into(),
                format!("{headroom} Consumi dei terminali non disponibili. Il lavoro continua."),
                None,
            )
        };
        if let Some(current) = self
            .queue
            .iter_mut()
            .find(|notice| notice.kind == Kind::Memory)
        {
            if current.title == title && current.body == body && current.session_id == session_id {
                return MemoryChange::None;
            }
            current.title = title;
            current.body = body;
            current.session_id = session_id;
            return MemoryChange::Refresh;
        }
        self.push(notice(Kind::Memory, title, &body, session_id));
        // A new memory warning behind an unanswered session notice must not
        // replay that session's sound. Updating a warning is always silent.
        if new_episode
            && self
                .queue
                .front()
                .is_some_and(|notice| notice.kind == Kind::Memory)
        {
            MemoryChange::Announce
        } else {
            MemoryChange::Refresh
        }
    }

    fn remove_memory_notice(&mut self) -> MemoryChange {
        let previous = self.queue.len();
        self.queue.retain(|notice| notice.kind != Kind::Memory);
        if self.queue.len() != previous {
            MemoryChange::Refresh
        } else {
            MemoryChange::None
        }
    }

    fn forget_session(&mut self, id: &str) {
        self.sessions.remove(id);
        self.queue
            .retain(|notice| notice.session_id.as_deref() != Some(id));
    }
}

#[derive(Debug, PartialEq, Eq)]
enum MemoryChange {
    None,
    Refresh,
    Announce,
}
struct MemoryConsumer {
    id: String,
    label: String,
    bytes: u64,
}

#[cfg(windows)]
fn largest_memory_consumer(
    statuses: &[shinatama_terminal::TerminalResourceStatus],
    sessions: &[SessionInfo],
) -> (Option<MemoryConsumer>, bool) {
    use shinatama_terminal::TerminalResourcePhase;
    let mut largest: Option<MemoryConsumer> = None;
    let mut partial = false;
    for status in statuses
        .iter()
        .filter(|status| status.phase == TerminalResourcePhase::Running)
    {
        let Some(observed) = status.observed.filter(|observed| {
            observed.membership_complete && status.sample_age_ms.is_some_and(|age| age <= 3000)
        }) else {
            partial = true;
            continue;
        };
        let Some(session) = sessions.iter().find(|session| session.id == status.id) else {
            partial = true;
            continue;
        };
        if largest.as_ref().is_some_and(|old| {
            old.bytes > observed.private_commit_bytes
                || (old.bytes == observed.private_commit_bytes && old.id <= status.id)
        }) {
            continue;
        }
        let project: String = session
            .cwd
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .take(32)
            .collect();
        let harness = match session.shell.as_str() {
            "claude_code" => "Claude Code".to_owned(),
            "codex" => "Codex".to_owned(),
            shell => shell.chars().filter(|c| !c.is_control()).take(24).collect(),
        };
        // Private commit avoids double-counting shared working-set pages in this
        // comparison. It identifies the largest measured terminal, not the cause
        // of host-wide pressure (other applications are outside this ranking).
        largest = Some(MemoryConsumer {
            id: status.id.clone(),
            label: format!(
                "{harness} #{} · {project}",
                status.id.chars().take(6).collect::<String>()
            ),
            bytes: observed.private_commit_bytes,
        });
    }
    (largest, partial)
}

/// One bounded, read-only watcher. It works while the main window is minimized;
/// it never starts, suspends, throttles or stops a terminal.
#[cfg(windows)]
pub(crate) fn start_memory_warnings(app: &tauri::AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("shinatama-memory-warnings".into())
        .spawn(move || {
            loop {
                if app.state::<crate::ExitIntent>().0.load(Ordering::Acquire)
                    || app.get_webview_window("main").is_none()
                {
                    break;
                }
                let memory = app
                    .state::<crate::resources::Resources>()
                    .memory_observation();
                let host = app.state::<Host>();
                let (consumer, partial) = largest_memory_consumer(&host.resources(), &host.list());
                let controller = app.state::<Controller>();
                let change = match controller.0.try_lock() {
                    Ok(mut feed) => feed.observe_memory(memory, consumer.as_ref(), partial),
                    Err(_) => MemoryChange::None,
                };
                if change != MemoryChange::None {
                    present(&app, &controller, change == MemoryChange::Announce);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
}

#[cfg(not(windows))]
pub(crate) fn start_memory_warnings(_app: &tauri::AppHandle) {}
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
    // Normal before it is drawn, low once it is gone.
    if visible {
        crate::responsiveness::set_memory_target(widget, true);
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
    if !visible {
        crate::responsiveness::set_memory_target(widget, false);
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
            Kind::Memory => Cue::Update,
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
    /// A notice about what a project gained (Xiox's leftovers). With a terminal it opens that
    /// session; without one it opens Shinatama, and the next notice of this kind replaces it.
    /// No sound.
    pub fn advise(
        &self,
        app: &tauri::AppHandle,
        title: String,
        body: &str,
        session_id: Option<String>,
    ) {
        let Ok(mut feed) = lock(self) else {
            return;
        };
        let pushed = feed.push(notice(Kind::Update, title, body, session_id));
        drop(feed);
        if pushed {
            present(app, self, false);
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
        let Ok(feed) = lock(self) else {
            return HashMap::new();
        };
        feed.sessions
            .iter()
            .map(|(id, o)| {
                (
                    id.clone(),
                    TerminalObservation {
                        ended: o.ended,
                        exit_code: o.exit_code,
                        since_signal_ms: o.last_signal.map(|t| t.elapsed().as_millis() as u64),
                        since_output_ms: o.last_output.map(|t| t.elapsed().as_millis() as u64),
                    },
                )
            })
            .collect()
    }
    pub fn forget(&self, app: &tauri::AppHandle, id: &str) {
        if let Ok(mut feed) = lock(self) {
            feed.forget_session(id);
        }
        present(app, self, false);
    }
}
#[tauri::command(async)]
pub fn attention_current(
    window: tauri::WebviewWindow,
    controller: State<'_, Controller>,
) -> Result<Snapshot, String> {
    allowed(&window)?;
    Ok(lock(&controller)?.snapshot())
}
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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
    if feed
        .queue
        .front()
        .is_some_and(|current| current.kind == Kind::Memory)
    {
        feed.memory_dismissed = true;
    }
    feed.queue.pop_front();
    drop(feed);
    present(window.app_handle(), &controller, false);
    Ok(())
}
#[tauri::command(async)]
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
    let memory_notice = current.kind == Kind::Memory;
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
    if memory_notice {
        feed.memory_dismissed = true;
    }
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
    fn memory(physical: u64, commit: u64) -> crate::resources::MemoryObservation {
        crate::resources::MemoryObservation {
            physical_available_bytes: physical * MIB,
            commit_available_bytes: commit * MIB,
        }
    }
    #[test]
    fn memory_warning_preserves_session_attention_and_updates_without_repeating() {
        let mut feed = Feed::default();
        feed.push(notice(
            Kind::Input,
            "Question".into(),
            "Answer needed",
            Some("one".into()),
        ));
        let usage = MemoryConsumer {
            id: "one".into(),
            label: "Codex · project".into(),
            bytes: 1800 * MIB,
        };
        assert_eq!(
            feed.observe_memory(Some(memory(400, 2000)), Some(&usage), false),
            MemoryChange::Refresh
        );
        assert_eq!(feed.queue.len(), 2);
        assert_eq!(feed.queue.front().unwrap().kind, Kind::Input);
        let warning = feed.queue.back().unwrap();
        assert_eq!(warning.kind, Kind::Memory);
        assert!(warning.body.contains("1800 MiB"));
        let id = warning.id.clone();
        assert_eq!(
            feed.observe_memory(Some(memory(380, 2000)), Some(&usage), false),
            MemoryChange::Refresh
        );
        assert_eq!(feed.queue.back().unwrap().id, id);
        feed.push(notice(
            Kind::Update,
            "Update".into(),
            "New information",
            Some("one".into()),
        ));
        assert_eq!(
            feed.queue
                .iter()
                .filter(|notice| notice.kind == Kind::Memory)
                .count(),
            1
        );
        assert_eq!(feed.queue.len(), 2);
        assert_eq!(feed.queue.front().unwrap().kind, Kind::Update);
        feed.observe_memory(Some(memory(900, 2000)), None, false);
        assert_eq!(feed.queue.len(), 1);
        assert_eq!(feed.queue.front().unwrap().kind, Kind::Update);
    }
    #[test]
    fn memory_warning_recovers_with_hysteresis_and_respects_dismissal() {
        let mut feed = Feed::default();
        assert_eq!(
            feed.observe_memory(Some(memory(400, 2000)), None, false),
            MemoryChange::Announce
        );
        assert!(feed.queue.front().unwrap().body.contains("non disponibili"));
        feed.memory_dismissed = true;
        feed.queue.clear();
        assert_eq!(
            feed.observe_memory(Some(memory(600, 2000)), None, false),
            MemoryChange::None
        );
        assert_eq!(
            feed.observe_memory(Some(memory(400, 2000)), None, false),
            MemoryChange::None
        );
        feed.observe_memory(Some(memory(900, 2000)), None, false);
        assert_eq!(
            feed.observe_memory(Some(memory(2000, 400)), None, false),
            MemoryChange::Announce
        );
        assert_eq!(
            feed.observe_memory(Some(memory(2000, 900)), None, false),
            MemoryChange::Refresh
        );
        assert!(feed.queue.is_empty());
        assert_eq!(feed.observe_memory(None, None, false), MemoryChange::None);
    }
    #[test]
    fn missing_memory_samples_do_not_invent_pressure_or_current_consumers() {
        let mut feed = Feed::default();
        assert_eq!(feed.observe_memory(None, None, false), MemoryChange::None);
        feed.observe_memory(Some(memory(400, 2000)), None, false);
        assert_eq!(
            feed.observe_memory(None, None, false),
            MemoryChange::Refresh
        );
        assert!(feed.queue.is_empty());
        assert_eq!(
            feed.observe_memory(Some(memory(400, 2000)), None, false),
            MemoryChange::Refresh
        );
        assert_eq!(feed.queue.len(), 1);
    }
    #[cfg(windows)]
    #[test]
    fn memory_ranking_ignores_stale_partial_and_ended_terminals() {
        use shinatama_terminal::{
            TerminalResourceObservation, TerminalResourcePhase, TerminalResourceReason,
            TerminalResourceStatus,
        };
        let sessions: Vec<_> = ["small", "large", "stale", "partial", "ended"]
            .into_iter()
            .map(|id| SessionInfo {
                id: id.into(),
                cwd: r"D:\Projects\example".into(),
                shell: "codex".into(),
            })
            .collect();
        let mut statuses: Vec<_> = sessions
            .iter()
            .enumerate()
            .map(|(index, session)| TerminalResourceStatus {
                id: session.id.clone(),
                phase: TerminalResourcePhase::Running,
                observed: Some(TerminalResourceObservation {
                    private_commit_bytes: (index as u64 + 1) * 100 * MIB,
                    resident_bytes: 0,
                    peak_job_commit_bytes: 0,
                    active_processes: 1,
                    total_processes: 1,
                    total_terminated_processes: 0,
                    cpu_time_100ns: 0,
                    membership_complete: true,
                }),
                applied: None,
                queues: None,
                sample_age_ms: Some(100),
                reason: TerminalResourceReason::Observed,
            })
            .collect();
        statuses[2].sample_age_ms = Some(3001);
        statuses[3].observed.as_mut().unwrap().membership_complete = false;
        statuses[4].phase = TerminalResourcePhase::Ended;
        let (largest, partial) = largest_memory_consumer(&statuses, &sessions);
        let largest = largest.unwrap();
        assert_eq!(largest.id, "large");
        assert_eq!(largest.bytes, 200 * MIB);
        assert_eq!(largest.label, "Codex #large · example");
        assert!(partial);
        let (largest, partial) = largest_memory_consumer(&statuses[..2], &sessions);
        assert_eq!(largest.unwrap().id, "large");
        assert!(!partial);
    }
    #[test]
    fn disabled_widget_does_not_queue_memory_warnings() {
        let mut feed = Feed::default();
        feed.enabled = false;
        assert_eq!(
            feed.observe_memory(Some(memory(400, 2000)), None, false),
            MemoryChange::None
        );
        assert!(feed.queue.is_empty());
        feed.enabled = true;
        assert_eq!(
            feed.observe_memory(Some(memory(400, 2000)), None, false),
            MemoryChange::Refresh
        );
        assert_eq!(feed.queue.len(), 1);
    }
    #[test]
    fn closing_a_terminal_removes_its_memory_notice_and_target() {
        let mut feed = Feed::default();
        let usage = MemoryConsumer {
            id: "one".into(),
            label: "Codex · project".into(),
            bytes: 1800 * MIB,
        };
        feed.observe_memory(Some(memory(400, 2000)), Some(&usage), false);
        feed.forget_session("one");
        assert!(feed.queue.is_empty());
        feed.observe_memory(Some(memory(400, 2000)), None, false);
        assert!(feed.queue.front().unwrap().session_id.is_none());
        assert!(!feed.queue.front().unwrap().body.contains("1800"));
    }
    #[test]
    fn every_kind_has_an_embedded_pcm_wav() {
        assert_eq!(Cue::from(Kind::Memory), Cue::Update);
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
