//! What agent sessions leave in their projects, watched with Xiox (`xiox observe`).
//!
//! The first agent terminal of a folder starts one observer for its project; it runs until
//! Shinatama exits, so what a session left is still listed after the session ended. When an agent
//! terminal ends, the observer is asked for a fresh report, and the widget says what appeared.
//! Two folders of one repository (the project and one of its worktrees) share one observer: Xiox
//! resolves both to the main working tree.
//!
//! Read-only: nothing here removes anything or runs `xiox clean`. The command Xiox prepares is
//! shown to the person in the project overview, to copy.
use crate::attention;
use serde::Serialize;
use shinatama_terminal::{OutputBatch, SessionInfo};
use shinatama_xiox::{Change, Observer, Options, Picture, Program, Update};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};
use tauri::{Emitter, Manager, State};

/// The terminals whose sessions are watched: the agent harnesses, not PowerShell.
const AGENTS: [&str; 4] = ["claude_code", "codex", "opencode", "pi"];
const MAX_PROJECTS: usize = 16;
const MAX_SESSIONS: usize = 256;
const MAX_PATH: usize = 4096;

#[derive(Default)]
pub struct Leftovers {
    program: OnceLock<Program>,
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
    ended: bool,
}

struct Watch {
    id: u64,
    /// The folders whose sessions this observer covers, as [`folder_key`].
    folders: Vec<String>,
    observer: Arc<Observer>,
    /// A session ended: the next report goes to the widget if it lists something new.
    announce: bool,
    announced_items: usize,
}

/// One observed project, as the page receives it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Watched {
    folders: Vec<String>,
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
        let folder = folder_key(&session.cwd);
        match inner.sessions.get_mut(&session.id) {
            None => {
                if inner.sessions.len() >= MAX_SESSIONS {
                    inner.sessions.retain(|_, seen| !seen.ended);
                }
                if inner.sessions.len() >= MAX_SESSIONS {
                    return;
                }
                inner.sessions.insert(
                    session.id.clone(),
                    Seen {
                        folder,
                        ended: batch.eof,
                    },
                );
                drop(inner);
                if !batch.eof
                    && let Err(reason) = self.watch(app, &session.cwd)
                {
                    eprintln!("Xiox does not observe {}: {reason}", session.cwd);
                }
            }
            Some(seen) if batch.eof && !seen.ended => {
                seen.ended = true;
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
                    .map(|watch| Watched {
                        folders: watch.folders.clone(),
                        picture: watch.observer.picture(),
                    })
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
        let observer = Observer::spawn(
            &program,
            path,
            Options::default(),
            move |picture, update| {
                listener
                    .state::<Leftovers>()
                    .updated(&listener, id, picture, update);
            },
        )
        .map_err(|error| format!("{} could not start: {error}", program.display()))?;
        inner.watches.push(Watch {
            id,
            folders: vec![key],
            observer,
            announce: false,
            announced_items: 0,
        });
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
        let watch = &mut inner.watches[index];
        let folders = watch.folders.clone();
        let announce = match update {
            Update::Line(Change::Attention { kind, value }) => Some(threshold(kind, *value)),
            Update::Line(Change::Report) if watch.announce => {
                watch.announce = false;
                let count = picture.new_items.len();
                (count > watch.announced_items).then(|| {
                    watch.announced_items = count;
                    appeared(picture)
                })
            }
            _ => None,
        };
        drop(inner);
        let _ = app.emit_to(
            "main",
            "project-leftovers",
            Watched {
                folders,
                picture: picture.clone(),
            },
        );
        if let Some(body) = announce {
            let name = picture.name.as_deref().unwrap_or("progetto");
            app.state::<attention::Controller>()
                .advise(app, format!("Xiox · {name}"), &body);
        }
    }
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

/// The widget's text after a session ended. The widget speaks Italian.
fn appeared(picture: &Picture) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for item in &picture.new_items {
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
    let growth = if picture.growth_bytes > 0 {
        format!(" (+{})", size(picture.growth_bytes))
    } else {
        String::new()
    };
    format!(
        "Dall'inizio dell'osservazione: {}{growth}. Rivedili in Progetti; non è stato rimosso niente.",
        parts.join(", ")
    )
}

fn threshold(kind: &str, value: u64) -> String {
    match kind {
        "worktrees" => format!(
            "{value} worktree nuovi in questo progetto. Rivedili in Progetti; non è stato rimosso niente."
        ),
        "growth" => format!(
            "Il progetto è cresciuto di {} da quando è osservato. Rivedi in Progetti; non è stato rimosso niente.",
            size(value)
        ),
        _ => "Xiox segnala una soglia superata. Rivedi in Progetti; non è stato rimosso niente."
            .into(),
    }
}

fn category_it(category: &str, count: usize) -> &'static str {
    let one = count == 1;
    match category {
        "worktrees" => "worktree",
        "copies" => {
            if one {
                "copia del repository"
            } else {
                "copie del repository"
            }
        }
        "named" => {
            if one {
                "cartella col nome del progetto"
            } else {
                "cartelle col nome del progetto"
            }
        }
        "scratch" => {
            if one {
                "cartella di sessione"
            } else {
                "cartelle di sessione"
            }
        }
        "builds" => {
            if one {
                "cartella di build"
            } else {
                "cartelle di build"
            }
        }
        "logs" => "log",
        "sessions" => {
            if one {
                "sessione archiviata"
            } else {
                "sessioni archiviate"
            }
        }
        _ => {
            if one {
                "elemento"
            } else {
                "elementi"
            }
        }
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
}
