//! What agent sessions leave in their projects, watched with Xiox (`xiox observe`).
//!
//! The first agent terminal of a folder starts one observer for its project; it runs until
//! Shinatama exits, so what a session left is still listed after the session ended. While an
//! agent terminal of the project is open, the observer is asked for a fresh check every minute;
//! when none is, Xiox checks on its own every ten minutes. The end of an agent terminal asks for
//! one at once. Two folders of one repository (the project and one of its worktrees) share one
//! observer: Xiox resolves both to the main working tree.
//!
//! A worktree, a copy, a folder named like the project or a session folder that appears is
//! announced in the widget at once, naming the session when only one could have made it, so
//! the person can stop a session that multiplies copies. The side panel lists them all.
//!
//! Who made an item: Xiox's own evidence from the session histories when it has some
//! (`provenance`); otherwise the agent terminals of the project that were open when it appeared,
//! which says "could have", never "did".
//!
//! Read-only: nothing here removes anything or runs `xiox clean`. The command Xiox prepares is
//! shown to the person, to copy; "show in Explorer" selects a listed folder and opens nothing.
use crate::{ExitIntent, attention};
use serde::Serialize;
use shinatama_terminal::{OutputBatch, SessionInfo};
use shinatama_xiox::{Change, NewItem, Observer, Options, Picture, Program, Update};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, OnceLock, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, State};

/// The terminals whose sessions are watched: the agent harnesses, not PowerShell.
const AGENTS: [&str; 4] = ["claude_code", "codex", "opencode", "pi"];
/// What multiplies on disk and is announced the moment it appears. Builds and logs are told at
/// the end of a session; archived sessions are what every session leaves and are never told.
const LOUD: [&str; 4] = ["worktrees", "copies", "named", "scratch"];
const MAX_PROJECTS: usize = 16;
const MAX_SESSIONS: usize = 256;
const MAX_PATH: usize = 4096;
/// A check every minute while a session of the project is open.
const TICK: Duration = Duration::from_secs(60);
/// Xiox's own interval when no session asks.
const IDLE_INTERVAL_SECS: u32 = 600;
/// An item is first seen by the check after it was made: a session that ended this long before
/// that check could still have made it.
const SLACK_MS: i64 = 90_000;

#[derive(Default)]
pub struct Leftovers {
    program: OnceLock<Program>,
    ticker: OnceLock<()>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    watches: Vec<Watch>,
    /// Agent terminals seen, by terminal id.
    sessions: HashMap<String, Seen>,
}

struct Seen {
    folder: String,
    harness: String,
    started_ms: i64,
    ended_ms: Option<i64>,
}

struct Watch {
    id: u64,
    /// The folders whose sessions this observer covers, as [`folder_key`].
    folders: Vec<String>,
    observer: Arc<Observer>,
    /// A session ended: the next report tells what it left that was not told yet.
    announce: bool,
    /// The item keys already told in the widget.
    announced: HashSet<String>,
}

/// An agent terminal that could have made an item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suspect {
    terminal_id: String,
    harness: String,
    /// The terminal still runs: the page can take the person to it.
    open: bool,
}

/// One observed project, as the page receives it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Watched {
    folders: Vec<String>,
    /// Agent terminals of the project open now.
    live_sessions: usize,
    /// By item key: the agent terminals that were open when the item appeared.
    suspects: BTreeMap<String, Vec<Suspect>>,
    #[serde(flatten)]
    picture: Picture,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    available: bool,
    /// Why nothing is observed, when `available` is false.
    reason: Option<String>,
    projects: Vec<Watched>,
}

impl Inner {
    fn watched(&self, watch: &Watch, picture: Picture) -> Watched {
        let runs: Vec<(&String, &Seen)> = self
            .sessions
            .iter()
            .filter(|(_, seen)| watch.folders.contains(&seen.folder))
            .collect();
        let suspects = picture
            .new_items
            .iter()
            .filter_map(|item| {
                let seen_at = item.first_seen_ms?;
                let mut found: Vec<Suspect> = runs
                    .iter()
                    .filter(|(_, run)| open_when(seen_at, run.started_ms, run.ended_ms))
                    .map(|(id, run)| Suspect {
                        terminal_id: (*id).clone(),
                        harness: run.harness.clone(),
                        open: run.ended_ms.is_none(),
                    })
                    .collect();
                found.sort_by(|a, b| a.terminal_id.cmp(&b.terminal_id));
                (!found.is_empty()).then(|| (item.key.clone(), found))
            })
            .collect();
        Watched {
            folders: watch.folders.clone(),
            live_sessions: runs
                .iter()
                .filter(|(_, run)| run.ended_ms.is_none())
                .count(),
            suspects,
            picture,
        }
    }

    fn live(&self, watch: &Watch) -> bool {
        self.sessions
            .values()
            .any(|seen| seen.ended_ms.is_none() && watch.folders.contains(&seen.folder))
    }
}

impl Leftovers {
    /// From `terminal_read`, for every batch of every terminal: starts watching the folder of a
    /// new agent terminal and asks for a report when one ends. Cheap when nothing changes.
    pub fn observe(&self, app: &tauri::AppHandle, session: &SessionInfo, batch: &OutputBatch) {
        if !AGENTS.contains(&session.shell.as_str()) {
            return;
        }
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        match inner.sessions.get_mut(&session.id) {
            None => {
                if inner.sessions.len() >= MAX_SESSIONS {
                    inner.sessions.retain(|_, seen| seen.ended_ms.is_none());
                }
                if inner.sessions.len() >= MAX_SESSIONS {
                    return;
                }
                let now = now_ms();
                inner.sessions.insert(
                    session.id.clone(),
                    Seen {
                        folder: folder_key(&session.cwd),
                        harness: session.shell.clone(),
                        started_ms: now,
                        ended_ms: batch.eof.then_some(now),
                    },
                );
                drop(inner);
                if !batch.eof
                    && let Err(reason) = self.watch(app, &session.cwd)
                {
                    eprintln!("Xiox does not observe {}: {reason}", session.cwd);
                }
            }
            Some(seen) if batch.eof && seen.ended_ms.is_none() => {
                seen.ended_ms = Some(now_ms());
                let folder = seen.folder.clone();
                if let Some(watch) = inner
                    .watches
                    .iter_mut()
                    .find(|watch| watch.folders.contains(&folder))
                {
                    watch.announce = true;
                    watch.observer.report();
                }
            }
            Some(_) => {}
        }
    }

    pub fn status(&self) -> Status {
        let reason = match self.program() {
            Program::Found(_) => None,
            Program::Off => Some(format!(
                "Xiox is switched off ({}=off).",
                shinatama_xiox::ENV
            )),
            Program::Missing(reason) => Some(reason.clone()),
        };
        let projects = self
            .inner
            .lock()
            .map(|inner| {
                inner
                    .watches
                    .iter()
                    .map(|watch| inner.watched(watch, watch.observer.picture()))
                    .collect()
            })
            .unwrap_or_default();
        Status {
            available: reason.is_none(),
            reason,
            projects,
        }
    }

    /// A fresh check of the project of `folder`, started now if it is not watched yet (then
    /// "new" means new from now on).
    pub fn check(&self, app: &tauri::AppHandle, folder: &str) -> Result<(), String> {
        let key = folder_key(folder);
        let reported = self.inner.lock().is_ok_and(|inner| {
            inner
                .watches
                .iter()
                .find(|watch| {
                    let root = watch.observer.picture().root.as_deref().map(folder_key);
                    (watch.folders.contains(&key) || root.as_ref() == Some(&key))
                        && !watch.observer.ended()
                })
                .is_some_and(|watch| watch.observer.report())
        });
        if reported {
            Ok(())
        } else {
            self.watch(app, folder)
        }
    }

    /// True when `path` is a folder or file a current picture lists: the page may only ask to
    /// show what Xiox reported.
    fn listed(&self, path: &str) -> bool {
        self.inner.lock().is_ok_and(|inner| {
            inner.watches.iter().any(|watch| {
                watch
                    .observer
                    .picture()
                    .new_items
                    .iter()
                    .any(|item| item.path.as_deref() == Some(path))
            })
        })
    }

    /// At exit: an observer must not outlive Shinatama (its stdin closes anyway).
    pub fn stop_all(&self) {
        let watches = self
            .inner
            .lock()
            .map(|mut inner| std::mem::take(&mut inner.watches))
            .unwrap_or_default();
        let stopping: Vec<_> = watches
            .into_iter()
            .map(|watch| std::thread::spawn(move || watch.observer.stop()))
            .collect();
        for stop in stopping {
            let _ = stop.join();
        }
    }

    fn program(&self) -> &Program {
        self.program.get_or_init(shinatama_xiox::locate)
    }

    /// Every [`TICK`]: a fresh check of each project with an agent terminal open.
    fn tick(&self) {
        let Ok(inner) = self.inner.lock() else {
            return;
        };
        for watch in &inner.watches {
            if !watch.observer.ended() && inner.live(watch) {
                watch.observer.report();
            }
        }
    }

    fn start_ticker(&self, app: &tauri::AppHandle) {
        self.ticker.get_or_init(|| {
            let app = app.clone();
            let _ = std::thread::Builder::new()
                .name("xiox-tick".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(TICK);
                        if app.state::<ExitIntent>().0.load(Ordering::Acquire) {
                            return;
                        }
                        app.state::<Leftovers>().tick();
                    }
                });
        });
    }

    fn watch(&self, app: &tauri::AppHandle, folder: &str) -> Result<(), String> {
        let program = match self.program() {
            Program::Found(program) => program.clone(),
            Program::Off => return Err("switched off".into()),
            Program::Missing(reason) => return Err(reason.clone()),
        };
        let path = Path::new(folder);
        if folder.len() > MAX_PATH || !path.is_absolute() || !path.is_dir() {
            return Err("the folder is not an absolute path to an existing folder".into());
        }
        let key = folder_key(folder);
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "the Xiox state is unavailable".to_owned())?;
        if inner
            .watches
            .iter()
            .any(|watch| watch.folders.contains(&key) && !watch.observer.ended())
        {
            return Ok(());
        }
        // An observer that ended (its folder moved, Xiox failed) is replaced, not kept beside.
        inner
            .watches
            .retain(|watch| !(watch.observer.ended() && watch.folders.contains(&key)));
        if inner.watches.len() >= MAX_PROJECTS {
            return Err(format!("Xiox already observes {MAX_PROJECTS} projects"));
        }
        inner.next_id += 1;
        let id = inner.next_id;
        let listener = app.clone();
        let options = Options {
            interval_secs: IDLE_INTERVAL_SECS,
            no_drives: false,
        };
        let observer = Observer::spawn(&program, path, options, move |picture, update| {
            listener
                .state::<Leftovers>()
                .updated(&listener, id, picture, update);
        })
        .map_err(|error| format!("{} could not start: {error}", program.display()))?;
        inner.watches.push(Watch {
            id,
            folders: vec![key],
            observer,
            announce: false,
            announced: HashSet::new(),
        });
        drop(inner);
        self.start_ticker(app);
        Ok(())
    }

    /// On the reader thread of observer `id`, after a line or at its end.
    fn updated(&self, app: &tauri::AppHandle, id: u64, picture: &Picture, update: &Update) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let Some(index) = inner.watches.iter().position(|watch| watch.id == id) else {
            return;
        };
        if *update == Update::Line(Change::Started)
            && let Some(root) = picture.root.as_deref().map(folder_key)
            && let Some(existing) = inner.watches.iter().position(|watch| {
                watch.id != id
                    && !watch.observer.ended()
                    && watch
                        .observer
                        .picture()
                        .root
                        .as_deref()
                        .map(folder_key)
                        .as_ref()
                        == Some(&root)
            })
        {
            // A worktree of a project already observed: one observer is enough.
            let duplicate = inner.watches.remove(index);
            let existing = if existing > index {
                existing - 1
            } else {
                existing
            };
            for folder in duplicate.folders {
                if !inner.watches[existing].folders.contains(&folder) {
                    inner.watches[existing].folders.push(folder);
                }
            }
            // Never from the reader thread of the observer that is being stopped.
            std::thread::spawn(move || duplicate.observer.stop());
            return;
        }
        let watched = inner.watched(&inner.watches[index], picture.clone());
        let watch = &mut inner.watches[index];
        let tell: Vec<&NewItem> = match update {
            Update::Line(Change::Changed { added }) => picture
                .new_items
                .iter()
                .filter(|item| added.contains(&item.key) && LOUD.contains(&item.category.as_str()))
                .collect(),
            Update::Line(Change::Report) if watch.announce => {
                watch.announce = false;
                picture
                    .new_items
                    .iter()
                    .filter(|item| item.category != "sessions")
                    .collect()
            }
            _ => Vec::new(),
        };
        let tell: Vec<&NewItem> = tell
            .into_iter()
            .filter(|item| watch.announced.insert(item.key.clone()))
            .collect();
        let announce = match update {
            Update::Line(Change::Attention { kind, value }) => {
                Some((threshold(kind, *value), None))
            }
            _ if !tell.is_empty() => Some(appeared(&tell, &watched.suspects)),
            _ => None,
        };
        drop(inner);
        let _ = app.emit_to("main", "project-leftovers", watched);
        if let Some((body, session)) = announce {
            let name = picture.name.as_deref().unwrap_or("progetto");
            app.state::<attention::Controller>().advise(
                app,
                format!("Xiox · {name}"),
                &body,
                session,
            );
        }
    }
}

/// An agent terminal opened at `started_ms` and ended at `ended_ms` (if it did) could have made
/// an item that a check first saw at `seen_at`.
fn open_when(seen_at: i64, started_ms: i64, ended_ms: Option<i64>) -> bool {
    started_ms <= seen_at && ended_ms.is_none_or(|ended| ended + SLACK_MS >= seen_at)
}

/// The same folder spelled the same way: separators, trailing separator and, on Windows, case.
fn folder_key(folder: &str) -> String {
    let folder = folder.strip_prefix(r"\\?\").unwrap_or(folder);
    let unified: String = if cfg!(windows) {
        folder.replace('/', "\\").to_lowercase()
    } else {
        folder.to_owned()
    };
    let trimmed = unified.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        unified
    } else {
        trimmed.to_owned()
    }
}

/// The widget's text for items that just appeared, and the terminal to open when exactly one
/// agent terminal could have made all of them. The widget speaks Italian.
fn appeared(
    items: &[&NewItem],
    suspects: &BTreeMap<String, Vec<Suspect>>,
) -> (String, Option<String>) {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for item in items {
        match counts
            .iter_mut()
            .find(|(category, _)| *category == item.category)
        {
            Some((_, count)) => *count += 1,
            None => counts.push((&item.category, 1)),
        }
    }
    let parts: Vec<String> = counts
        .iter()
        .map(|(category, count)| format!("{count} {}", category_it(category, *count)))
        .collect();
    let mut names: Vec<&str> = items
        .iter()
        .filter_map(|item| item.path.as_deref())
        .map(|path| {
            path.trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(path)
        })
        .take(3)
        .collect();
    if items.len() > names.len() {
        names.push("…");
    }
    let mut terminals: Vec<&Suspect> = items
        .iter()
        .flat_map(|item| suspects.get(&item.key).into_iter().flatten())
        .collect();
    terminals.sort_by(|a, b| a.terminal_id.cmp(&b.terminal_id));
    terminals.dedup_by(|a, b| a.terminal_id == b.terminal_id);
    let only = match terminals.as_slice() {
        [one] if items.iter().all(|item| suspects.contains_key(&item.key)) => Some(*one),
        _ => None,
    };
    let what = format!("{} ({})", parts.join(", "), names.join(", "));
    match only {
        Some(terminal) => (
            format!(
                "Mentre era aperta solo {} #{}: {what}. Elenco nel pannello Leftovers; non è stato rimosso niente.",
                harness_name(&terminal.harness),
                terminal.terminal_id.chars().take(6).collect::<String>()
            ),
            terminal.open.then(|| terminal.terminal_id.clone()),
        ),
        None => (
            format!("Nuovi: {what}. Elenco nel pannello Leftovers; non è stato rimosso niente."),
            None,
        ),
    }
}

fn harness_name(harness: &str) -> &str {
    match harness {
        "claude_code" => "Claude Code",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "pi" => "Pi",
        other => other,
    }
}

fn threshold(kind: &str, value: u64) -> String {
    match kind {
        "worktrees" => format!(
            "{value} worktree nuovi in questo progetto. Elenco nel pannello Leftovers; non è stato rimosso niente."
        ),
        "growth" => format!(
            "Il progetto è cresciuto di {} da quando è osservato. Elenco nel pannello Leftovers; non è stato rimosso niente.",
            size(value)
        ),
        _ => "Xiox segnala una soglia superata. Elenco nel pannello Leftovers; non è stato rimosso niente."
            .into(),
    }
}

fn category_it(category: &str, count: usize) -> &'static str {
    let one = count == 1;
    match category {
        "worktrees" => "worktree",
        "copies" if one => "copia del repository",
        "copies" => "copie del repository",
        "named" if one => "cartella col nome del progetto",
        "named" => "cartelle col nome del progetto",
        "scratch" if one => "cartella di sessione",
        "scratch" => "cartelle di sessione",
        "builds" if one => "cartella di build",
        "builds" => "cartelle di build",
        "logs" => "log",
        _ if one => "elemento",
        _ => "elementi",
    }
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

#[tauri::command(async)]
pub fn project_leftovers(
    window: tauri::WebviewWindow,
    leftovers: State<'_, Leftovers>,
) -> Result<Status, String> {
    crate::local(&window)?;
    Ok(leftovers.status())
}

#[tauri::command(async)]
pub fn project_leftovers_check(
    window: tauri::WebviewWindow,
    leftovers: State<'_, Leftovers>,
    path: String,
) -> Result<(), String> {
    crate::local(&window)?;
    if path.len() > MAX_PATH {
        return Err("The project path is invalid.".into());
    }
    leftovers.check(window.app_handle(), &path)
}

/// Selects a listed item in Explorer. Only a path a current picture lists is accepted; nothing
/// is opened or run.
#[tauri::command(async)]
pub fn leftovers_reveal(
    window: tauri::WebviewWindow,
    leftovers: State<'_, Leftovers>,
    path: String,
) -> Result<(), String> {
    crate::local(&window)?;
    if path.len() > MAX_PATH || !leftovers.listed(&path) {
        return Err("Only a folder in the Leftovers list can be shown.".into());
    }
    if !Path::new(&path).exists() {
        return Err("It is no longer there.".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The argument is one token: explorer reads `/select,` and the path after it.
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{path}\""))
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Explorer could not start: {error}"))
    }
    #[cfg(not(windows))]
    Err("Showing a folder is available on Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_compare_by_their_spelling_of_the_same_place() {
        assert_eq!(folder_key("/w/app/"), folder_key("/w/app"));
        if cfg!(windows) {
            assert_eq!(folder_key(r"C:\W\App\"), folder_key("c:/w/app"));
            assert_eq!(folder_key(r"\\?\C:\w\app"), folder_key(r"C:\w\app"));
            assert_eq!(folder_key(r"C:\"), r"c:\");
        }
    }

    #[test]
    fn sizes_read_like_a_person_writes_them() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn a_terminal_could_have_made_what_appeared_while_it_ran() {
        assert!(open_when(1_000, 500, None));
        assert!(open_when(1_000, 500, Some(1_000)));
        assert!(
            open_when(1_000 + SLACK_MS, 500, Some(1_000)),
            "the check comes after"
        );
        assert!(!open_when(2_000 + SLACK_MS, 500, Some(1_000)));
        assert!(
            !open_when(400, 500, None),
            "it appeared before the terminal started"
        );
    }
}
