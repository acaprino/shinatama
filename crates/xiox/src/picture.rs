//! The observer's JSON lines (schema 1, `docs/xiox-observe.md` in Xiox) folded into one picture.
//!
//! Only what appeared since the observer started is kept: the items of the first snapshot are
//! the project as the person left it, not something a session made. A `report` is the source of
//! truth and replaces what the events between two reports said.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: u64 = 1;
/// New items kept. Every archived agent session is an item, so the listing is bounded.
pub const MAX_NEW_ITEMS: usize = 200;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Started, the first snapshot is not complete yet.
    #[default]
    Starting,
    Watching,
    /// The process ended; `message` says why when it was not asked to.
    Ended,
}

/// One thing that appeared in the project since the observer started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewItem {
    pub key: String,
    /// `builds`, `worktrees`, `copies`, `named`, `scratch`, `logs` or `sessions`.
    pub category: String,
    /// `None` for an archived session.
    pub path: Option<String>,
    /// The title of an archived session.
    pub label: Option<String>,
    pub bytes: Option<u64>,
    /// `bytes` is a lower bound: the folder was larger than Xiox's measuring caps.
    pub bytes_partial: bool,
    /// `recommended`, `optional` or `caution`.
    pub level: String,
    /// Why it cannot be removed; empty when it can.
    pub blocked: Vec<String>,
    /// Why Xiox lists it, in a few words.
    pub reason: String,
    pub first_seen_ms: Option<i64>,
    /// The agent sessions whose history shows they made or wrote this folder (Xiox's
    /// `provenance`), most recently active first. Empty when Xiox found no such evidence.
    pub sessions: Vec<SessionRef>,
}

/// One agent session named by Xiox's evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    #[serde(default)]
    pub harness: String,
    #[serde(default)]
    pub id: String,
    /// `created`, `written` or `used`.
    #[serde(default)]
    pub strength: Option<String>,
}

/// Sessions kept per item: the evidence can name many.
const MAX_SESSIONS: usize = 3;

/// Everything Shinatama knows about one observed project.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Picture {
    pub phase: Phase,
    /// The project as Xiox resolved it: the main working tree of a repository.
    pub root: Option<String>,
    pub name: Option<String>,
    pub is_repo: bool,
    pub new_items: Vec<NewItem>,
    /// New items beyond [`MAX_NEW_ITEMS`], not listed.
    pub new_items_omitted: usize,
    /// Measured growth since the start, in bytes.
    pub growth_bytes: u64,
    /// The thresholds exceeded: `growth`, `worktrees`.
    pub attention: Vec<String>,
    /// The last check failed; this is the last good picture.
    pub stale: bool,
    /// Xiox's `xiox clean` command for the new recommended items. Never run by Shinatama.
    pub clean_command: Option<String>,
    pub clean_command_truncated: bool,
    pub started_at_ms: Option<i64>,
    pub checked_at_ms: Option<i64>,
    /// The last error the observer reported, or why it ended.
    pub message: Option<String>,
}

/// What one line changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Started,
    /// The keys of the new items that were not listed before.
    Changed {
        added: Vec<String>,
    },
    Attention {
        kind: String,
        value: u64,
    },
    Report,
    Error,
    /// A line this version does not know: a later event, or another schema.
    Ignored,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Project {
    root: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    is_repo: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    key: String,
    category: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    bytes: Option<u64>,
    #[serde(default)]
    level: String,
    #[serde(default)]
    blocked: Vec<String>,
    #[serde(default)]
    why: Value,
    #[serde(default)]
    new: bool,
    #[serde(default)]
    bytes_partial: bool,
    #[serde(default)]
    first_seen_ms: Option<i64>,
    #[serde(default)]
    provenance: Option<Provenance>,
}

#[derive(Deserialize)]
struct Provenance {
    #[serde(default)]
    sessions: Vec<SessionRef>,
}

#[derive(Deserialize)]
#[serde(
    tag = "event",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
enum Event {
    Started {
        project: Project,
    },
    Changed {
        #[serde(default)]
        added: Vec<Item>,
        #[serde(default)]
        removed: Vec<String>,
        #[serde(default)]
        growth_bytes: u64,
    },
    Attention {
        kind: String,
        #[serde(default)]
        value: u64,
        #[serde(default)]
        clean_command: Option<String>,
    },
    Report {
        project: Project,
        #[serde(default)]
        items: Vec<Item>,
        #[serde(default)]
        growth_bytes: u64,
        #[serde(default)]
        attention: Vec<String>,
        #[serde(default)]
        stale: bool,
        #[serde(default)]
        clean_command: Option<String>,
        #[serde(default)]
        clean_command_truncated: bool,
        #[serde(default)]
        started_at_ms: Option<i64>,
        #[serde(default)]
        checked_at_ms: Option<i64>,
    },
    Error {
        message: String,
    },
}

const EVENTS: [&str; 5] = ["started", "changed", "attention", "report", "error"];

impl Picture {
    /// Folds one stdout line in. A line that is not JSON, or a known event with the wrong
    /// shape, is an error; an unknown event or schema is [`Change::Ignored`].
    pub fn apply(&mut self, line: &str) -> Result<Change, String> {
        let value: Value =
            serde_json::from_str(line).map_err(|e| format!("not a JSON line: {e}"))?;
        let known = value.get("schema").and_then(Value::as_u64) == Some(SCHEMA)
            && value
                .get("event")
                .and_then(Value::as_str)
                .is_some_and(|name| EVENTS.contains(&name));
        if !known {
            return Ok(Change::Ignored);
        }
        let event: Event =
            serde_json::from_value(value).map_err(|e| format!("unexpected event shape: {e}"))?;
        Ok(match event {
            Event::Started { project } => {
                self.phase = Phase::Watching;
                self.project(project);
                self.started_at_ms.get_or_insert_with(now_ms);
                Change::Started
            }
            Event::Changed {
                added,
                removed,
                growth_bytes,
            } => {
                self.new_items.retain(|item| !removed.contains(&item.key));
                let mut fresh = Vec::new();
                for item in added {
                    if !item.new {
                        // An item of the snapshot seen again in another category.
                        self.new_items.retain(|known| known.key != item.key);
                        continue;
                    }
                    let item = NewItem::from(item);
                    if let Some(known) = self
                        .new_items
                        .iter_mut()
                        .find(|known| known.key == item.key)
                    {
                        *known = item;
                    } else if self.new_items.len() < MAX_NEW_ITEMS {
                        fresh.push(item.key.clone());
                        self.new_items.push(item);
                    } else {
                        self.new_items_omitted += 1;
                    }
                }
                self.growth_bytes = growth_bytes;
                Change::Changed { added: fresh }
            }
            Event::Attention {
                kind,
                value,
                clean_command,
            } => {
                if !self.attention.contains(&kind) {
                    self.attention.push(kind.clone());
                }
                self.clean_command = clean_command;
                Change::Attention { kind, value }
            }
            Event::Report {
                project,
                items,
                growth_bytes,
                attention,
                stale,
                clean_command,
                clean_command_truncated,
                started_at_ms,
                checked_at_ms,
            } => {
                self.phase = Phase::Watching;
                self.project(project);
                let mut fresh: Vec<NewItem> = items
                    .into_iter()
                    .filter(|item| item.new)
                    .map(NewItem::from)
                    .collect();
                self.new_items_omitted = fresh.len().saturating_sub(MAX_NEW_ITEMS);
                fresh.truncate(MAX_NEW_ITEMS);
                self.new_items = fresh;
                self.growth_bytes = growth_bytes;
                self.attention = attention;
                self.stale = stale;
                self.clean_command = clean_command;
                self.clean_command_truncated = clean_command_truncated;
                self.started_at_ms = started_at_ms.or(self.started_at_ms);
                self.checked_at_ms = checked_at_ms;
                if !stale {
                    self.message = None;
                }
                Change::Report
            }
            Event::Error { message } => {
                self.message = Some(bounded(&message));
                Change::Error
            }
        })
    }

    /// The process is gone. `message` is set when it ended on its own.
    pub fn end(&mut self, message: Option<String>) {
        self.phase = Phase::Ended;
        if let Some(message) = message {
            self.message = Some(bounded(&message));
        }
    }

    fn project(&mut self, project: Project) {
        self.root = Some(project.root);
        self.name = project.name;
        self.is_repo = project.is_repo;
    }
}

impl From<Item> for NewItem {
    fn from(item: Item) -> Self {
        let text = |field: &str| {
            item.why
                .get(field)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let label = (item.category == "sessions")
            .then(|| text("title"))
            .flatten();
        Self {
            reason: reason(&item.why, &item.category),
            blocked: item.blocked.iter().map(|block| blocked(block)).collect(),
            label,
            key: item.key,
            category: item.category,
            path: item.path,
            bytes: item.bytes,
            bytes_partial: item.bytes_partial,
            level: item.level,
            first_seen_ms: item.first_seen_ms,
            sessions: item
                .provenance
                .map(|provenance| provenance.sessions.into_iter().take(MAX_SESSIONS).collect())
                .unwrap_or_default(),
        }
    }
}

/// Xiox's `why`, in a few words. An unknown type says the category.
fn reason(why: &Value, category: &str) -> String {
    let text = |field: &str| why.get(field).and_then(Value::as_str);
    match text("type").unwrap_or_default() {
        "linked-worktree" => match text("branch") {
            Some(branch) => format!("worktree on branch {branch}"),
            None => "linked worktree".into(),
        },
        "same-repository" if why.get("hasOwnWork").and_then(Value::as_bool) == Some(true) => {
            "copy of the repository, with its own work".into()
        }
        "same-repository" => "copy of the repository".into(),
        "name-only" => "named like the project".into(),
        "catalog-folder" => text("labelEn").unwrap_or("build folder").into(),
        "log-file" => "log file".into(),
        "archived-session" => match text("agent") {
            Some(agent) => format!("archived {agent} session"),
            None => "archived agent session".into(),
        },
        "session-folder" => match text("strength") {
            Some("created") => "created by an agent session".into(),
            Some("written") => "written by an agent session".into(),
            Some("used") => "used by an agent session".into(),
            _ => "folder of an agent session".into(),
        },
        _ => category.into(),
    }
}

fn blocked(block: &str) -> String {
    match block {
        "dirty-git" => "uncommitted changes".into(),
        "unsaved-work" => "has commits or changes the project does not have".into(),
        other => other.replace('-', " "),
    }
}

fn bounded(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(400).collect()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}
