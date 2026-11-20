//! What the model proxy's archive keeps, and a run of the proxy's own commands that removes the
//! rest (docs/superpowers/specs/2026-11-20-proxy-archive-cleanup-design.md).
//!
//! The archive (`index.db`) is the proxy's: Shinatama only reads
//! `SELECT session_id, protocol FROM sessions` from it, read-only. Every change is the proxy's own
//! offline command, `sessions rm` and `storage gc`, which the proxy owner runs while no proxy
//! process runs: two processes writing the file corrupt it.
use crate::resources::process::NativeCommand;
use crate::sidecar_runtime::SidecarRuntimeRecipe;
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The most of a command's standard output a run reads.
pub const STDOUT_LIMIT: usize = 64 * 1024;
/// The compaction after removals; an age of 100 years archives nothing by age.
const COMPACT: [&str; 7] = [
    "storage", "gc", "--days", "36500", "--vacuum", "full", "--json",
];

/// One row of the archive's `sessions` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveSession {
    pub id: String,
    pub protocol: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArchiveError {
    Unreadable,
    InvalidCommand,
}

/// What a maintenance command printed, and how it ended (None: no exit code was read).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: Option<u32>,
    pub stdout: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Everything the rule selected was tried.
    Done,
    /// The run's budget ended it.
    Budget,
    /// A start of the proxy was asked for.
    Cancelled,
    /// The archive or Shinatama's threads could not be read: nothing was removed.
    Unreadable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub removed: usize,
    pub failed: usize,
    /// Selected but not tried, because the run stopped.
    pub left: usize,
    pub released_bytes: u64,
    pub compacted: bool,
    pub file_bytes_before: Option<u64>,
    pub file_bytes_after: Option<u64>,
    pub elapsed: Duration,
    pub stop: Stop,
}

/// The ids the archive does not keep, in the archive's order: every Claude subagent session, and
/// every session whose id, up to its first `|`, is no native id of an existing thread's segment.
/// Any other `|sub:` session stays with its root: the proxy also forks a live conversation onto
/// `<id>|sub:<16 hex of its instructions>` (`persona-anchor.ts`), which is not a finished subagent.
pub fn removable(sessions: &[ArchiveSession], natives: &HashSet<String>) -> Vec<String> {
    sessions
        .iter()
        .filter(|session| {
            claude_subagent(session)
                || !natives.contains(session.id.split('|').next().unwrap_or_default())
        })
        .map(|session| session.id.clone())
        .collect()
}

/// `<id>|sub:a<16 lowercase hex>` on the Anthropic protocol: the agent id Claude Code gives a
/// subagent, which the proxy keys its state by.
fn claude_subagent(session: &ArchiveSession) -> bool {
    session.protocol == "anthropic"
        && session.id.rsplit_once("|sub:").is_some_and(|(_, agent)| {
            agent.len() == 17
                && agent.starts_with('a')
                && agent[1..]
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        })
}

/// The archive of the proxy that `recipe` starts: its `BILI_SESSIONS_DIR` and `index.db`.
pub fn index_path(recipe: &SidecarRuntimeRecipe) -> Option<PathBuf> {
    recipe
        .child_environment()
        .find(|(name, _)| *name == std::ffi::OsStr::new("BILI_SESSIONS_DIR"))
        .map(|(_, directory)| Path::new(directory).join("index.db"))
}

/// The archive's sessions, read read-only.
pub fn archive_sessions(index: &Path) -> Result<Vec<ArchiveSession>, ArchiveError> {
    use rusqlite::{Connection, OpenFlags};
    // Opening a path that is not there would not create it read-only, but would fail later
    // with a less clear error: an archive that does not exist yet has nothing to remove.
    if !index.is_file() {
        return Err(ArchiveError::Unreadable);
    }
    let connection = Connection::open_with_flags(
        index,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| ArchiveError::Unreadable)?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(|_| ArchiveError::Unreadable)?;
    let mut statement = connection
        .prepare("SELECT session_id, protocol FROM sessions")
        .map_err(|_| ArchiveError::Unreadable)?;
    let rows = statement
        .query_map([], |row| {
            Ok(ArchiveSession {
                id: row.get(0)?,
                protocol: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            })
        })
        .map_err(|_| ArchiveError::Unreadable)?;
    rows.collect::<Result<_, _>>()
        .map_err(|_| ArchiveError::Unreadable)
}

/// `node <entry> <arguments>` with the proxy's own environment, without its start-up check (the
/// proxy's own start keeps it).
pub fn maintenance_command(
    recipe: &SidecarRuntimeRecipe,
    arguments: &[&str],
) -> Result<NativeCommand, ArchiveError> {
    let entry = recipe
        .arguments()
        .first()
        .ok_or(ArchiveError::InvalidCommand)?;
    let argv = std::iter::once(entry.clone())
        .chain(arguments.iter().map(OsString::from))
        .collect();
    let mut environment: BTreeMap<OsString, OsString> = recipe
        .child_environment()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    environment.insert("BILI_STORAGE_QUICK_CHECK".into(), "0".into());
    NativeCommand::new(
        recipe.executable().to_owned(),
        argv,
        recipe.working_directory().to_owned(),
        environment,
    )
    .map_err(|_| ArchiveError::InvalidCommand)
}

/// Removes what [`removable`] selects, one `sessions rm` each, then compacts the file once when
/// something was removed and no start was asked for. `deadline: None` runs until `cancelled`.
/// `natives` is Err when Shinatama's threads could not be read: then nothing is removed.
#[allow(clippy::type_complexity)]
pub fn run(
    recipe: &SidecarRuntimeRecipe,
    natives: Result<HashSet<String>, ()>,
    deadline: Option<Instant>,
    cancelled: &dyn Fn() -> bool,
    execute: &mut dyn FnMut(NativeCommand) -> Result<CommandOutput, ()>,
) -> Report {
    let index = index_path(recipe);
    run_on(
        index.as_deref(),
        natives,
        deadline,
        cancelled,
        &mut |arguments| execute(maintenance_command(recipe, arguments).map_err(|_| ())?),
    )
}

#[allow(clippy::type_complexity)]
fn run_on(
    index: Option<&Path>,
    natives: Result<HashSet<String>, ()>,
    deadline: Option<Instant>,
    cancelled: &dyn Fn() -> bool,
    execute: &mut dyn FnMut(&[&str]) -> Result<CommandOutput, ()>,
) -> Report {
    let began = Instant::now();
    let mut report = Report {
        removed: 0,
        failed: 0,
        left: 0,
        released_bytes: 0,
        compacted: false,
        file_bytes_before: None,
        file_bytes_after: None,
        elapsed: Duration::ZERO,
        stop: Stop::Done,
    };
    let selected = match (index, natives) {
        (Some(index), Ok(natives)) => archive_sessions(index)
            .ok()
            .map(|sessions| (index, removable(&sessions, &natives))),
        _ => None,
    };
    let Some((index, ids)) = selected else {
        report.stop = Stop::Unreadable;
        report.elapsed = began.elapsed();
        return report;
    };
    report.file_bytes_before = std::fs::metadata(index).ok().map(|meta| meta.len());
    for (tried, id) in ids.iter().enumerate() {
        let stop = if cancelled() {
            Some(Stop::Cancelled)
        } else if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            Some(Stop::Budget)
        } else {
            None
        };
        if let Some(stop) = stop {
            report.stop = stop;
            report.left = ids.len() - tried;
            break;
        }
        match execute(&["sessions", "rm", id.as_str(), "--no-vacuum", "--json"])
            .ok()
            .and_then(|output| released(&output, id))
        {
            Some(bytes) => {
                report.removed += 1;
                report.released_bytes += bytes;
            }
            None => report.failed += 1,
        }
    }
    if report.removed > 0 && report.stop != Stop::Cancelled && !cancelled() {
        report.compacted = execute(&COMPACT).is_ok_and(|output| output.code == Some(0));
    }
    report.file_bytes_after = std::fs::metadata(index).ok().map(|meta| meta.len());
    report.elapsed = began.elapsed();
    report
}

/// The bytes a `sessions rm <id> --json` released, or None when it removed nothing: a removal
/// exits 0 and echoes the id; a session already gone says `"found": false` (nothing left to
/// remove); a usage error prints `{}` with exit 1 (`storage-maintenance.ts`, `fail`).
fn released(output: &CommandOutput, id: &str) -> Option<u64> {
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    if reply.get("found") == Some(&serde_json::Value::Bool(false)) {
        return Some(0);
    }
    (output.code == Some(0) && reply.get("id").and_then(|value| value.as_str()) == Some(id)).then(
        || {
            reply
                .get("dbBytesReleased")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn session(id: &str, protocol: &str) -> ArchiveSession {
        ArchiveSession {
            id: id.into(),
            protocol: protocol.into(),
        }
    }
    fn natives(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }
    /// An archive with the proxy's `sessions` table, in a fresh temporary folder.
    struct Archive(PathBuf);
    impl Archive {
        fn with(rows: &[(&str, &str)]) -> Self {
            let folder =
                std::env::temp_dir().join(format!("shinatama-archive-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&folder).unwrap();
            let path = folder.join("index.db");
            let connection = rusqlite::Connection::open(&path).unwrap();
            connection
                .execute_batch("CREATE TABLE sessions(session_id TEXT PRIMARY KEY, protocol TEXT, origin TEXT)")
                .unwrap();
            for (id, protocol) in rows {
                connection
                    .execute(
                        "INSERT INTO sessions(session_id, protocol) VALUES(?1, ?2)",
                        [id, protocol],
                    )
                    .unwrap();
            }
            Self(path)
        }
    }
    impl Drop for Archive {
        fn drop(&mut self) {
            if let Some(folder) = self.0.parent() {
                let _ = std::fs::remove_dir_all(folder);
            }
        }
    }
    /// The proxy's replies: `sessions rm <id>` echoes the id with exit 0, `storage gc` an object.
    fn proxy(arguments: &[&str]) -> Result<CommandOutput, ()> {
        let stdout = match arguments {
            ["sessions", "rm", id, ..] => format!("{{\"id\":\"{id}\",\"dbBytesReleased\":100}}"),
            _ => "{}".into(),
        };
        Ok(CommandOutput {
            code: Some(0),
            stdout: stdout.into_bytes(),
        })
    }
    const THREE: [(&str, &str); 4] = [
        ("kept", "anthropic"),
        ("gone-1", "anthropic"),
        ("gone-2", "responses"),
        ("kept|sub:a0123456789abcdef", "anthropic"),
    ];

    #[test]
    fn claude_subagents_and_sessions_of_no_thread_are_removed() {
        let sessions = [
            session("c1", "anthropic"),
            session("c1|sub:a0123456789abcdef", "anthropic"),
            session("gone", "anthropic"),
            session("gone|sub:a0123456789abcdef", "anthropic"),
            session("x1", "responses"),
            session("x1|sub:0123456789abcdef", "responses"),
            session("y2", "responses"),
            session("y2|sub:0123456789abcdef", "responses"),
        ];
        assert_eq!(
            removable(&sessions, &natives(&["c1", "x1"])),
            [
                "c1|sub:a0123456789abcdef",
                "gone",
                "gone|sub:a0123456789abcdef",
                "y2",
                "y2|sub:0123456789abcdef"
            ]
        );
    }

    #[test]
    fn a_fork_of_an_existing_thread_is_kept_whatever_its_protocol() {
        let sessions = [
            session("c1", "anthropic"),
            session("c1|sub:0123456789abcdef", "anthropic"),
            session("c1|sub:a0123456789ABCDEF", "anthropic"),
        ];
        assert!(removable(&sessions, &natives(&["c1"])).is_empty());
    }

    #[test]
    fn a_run_removes_each_id_then_compacts_once() {
        let archive = Archive::with(&THREE);
        let calls = RefCell::new(Vec::<String>::new());
        let report = run_on(
            Some(&archive.0),
            Ok(natives(&["kept"])),
            None,
            &|| false,
            &mut |arguments| {
                calls.borrow_mut().push(arguments.join(" "));
                proxy(arguments)
            },
        );
        assert_eq!(
            calls.into_inner(),
            [
                "sessions rm gone-1 --no-vacuum --json",
                "sessions rm gone-2 --no-vacuum --json",
                "sessions rm kept|sub:a0123456789abcdef --no-vacuum --json",
                "storage gc --days 36500 --vacuum full --json",
            ]
        );
        assert_eq!(
            (
                report.removed,
                report.failed,
                report.left,
                report.released_bytes
            ),
            (3, 0, 0, 300)
        );
        assert!(report.compacted);
        assert_eq!(report.stop, Stop::Done);
        assert!(report.file_bytes_before.is_some() && report.file_bytes_after.is_some());
    }

    #[test]
    fn nothing_to_remove_runs_nothing() {
        let archive = Archive::with(&[("kept", "anthropic")]);
        let calls = Cell::new(0);
        let report = run_on(
            Some(&archive.0),
            Ok(natives(&["kept"])),
            None,
            &|| false,
            &mut |arguments| {
                calls.set(calls.get() + 1);
                proxy(arguments)
            },
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(
            (report.removed, report.compacted, report.stop),
            (0, false, Stop::Done)
        );
    }

    #[test]
    fn a_start_request_ends_the_run_after_the_command_in_progress() {
        let archive = Archive::with(&THREE);
        let asked = Cell::new(false);
        let calls = Cell::new(0);
        let report = run_on(
            Some(&archive.0),
            Ok(natives(&["kept"])),
            None,
            &|| asked.get(),
            &mut |arguments| {
                calls.set(calls.get() + 1);
                asked.set(true);
                proxy(arguments)
            },
        );
        assert_eq!(
            calls.get(),
            1,
            "no compaction and no second removal after the request"
        );
        assert_eq!(
            (report.removed, report.left, report.compacted, report.stop),
            (1, 2, false, Stop::Cancelled)
        );
    }

    #[test]
    fn unreadable_segments_remove_nothing() {
        let archive = Archive::with(&THREE);
        let calls = Cell::new(0);
        let report = run_on(
            Some(&archive.0),
            Err(()),
            None,
            &|| false,
            &mut |arguments| {
                calls.set(calls.get() + 1);
                proxy(arguments)
            },
        );
        assert_eq!(calls.get(), 0);
        assert_eq!((report.removed, report.stop), (0, Stop::Unreadable));
    }

    #[test]
    fn a_missing_archive_runs_nothing() {
        let missing = std::env::temp_dir()
            .join(format!("shinatama-archive-{}", uuid::Uuid::new_v4()))
            .join("index.db");
        let calls = Cell::new(0);
        let report = run_on(
            Some(&missing),
            Ok(natives(&[])),
            None,
            &|| false,
            &mut |arguments| {
                calls.set(calls.get() + 1);
                proxy(arguments)
            },
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(report.stop, Stop::Unreadable);
        assert!(!missing.exists(), "a read never creates the archive");
        let report = run_on(None, Ok(natives(&[])), None, &|| false, &mut proxy);
        assert_eq!(report.stop, Stop::Unreadable);
    }

    #[test]
    fn a_failed_removal_is_counted_and_the_run_goes_on() {
        let archive = Archive::with(&THREE);
        let report = run_on(
            Some(&archive.0),
            Ok(natives(&["kept"])),
            None,
            &|| false,
            &mut |arguments| match arguments {
                ["sessions", "rm", "gone-2", ..] => Err(()),
                ["sessions", "rm", "kept|sub:a0123456789abcdef", ..] => Ok(CommandOutput {
                    code: Some(0),
                    stdout: b"not json".to_vec(),
                }),
                _ => proxy(arguments),
            },
        );
        assert_eq!((report.removed, report.failed, report.left), (1, 2, 0));
        assert!(report.compacted);
        assert_eq!(report.stop, Stop::Done);
    }

    #[test]
    fn a_usage_error_or_an_already_removed_session_is_told_apart() {
        let archive = Archive::with(&THREE);
        let report = run_on(
            Some(&archive.0),
            Ok(natives(&["kept"])),
            None,
            &|| false,
            &mut |arguments| match arguments {
                // The proxy's `fail()` prints `{}` with exit 1 for a usage error: not removed.
                ["sessions", "rm", "gone-1", ..] => Ok(CommandOutput {
                    code: Some(1),
                    stdout: b"{}".to_vec(),
                }),
                // A session that is already gone: nothing left to remove.
                ["sessions", "rm", "gone-2", ..] => Ok(CommandOutput {
                    code: Some(1),
                    stdout: b"{\"id\":\"gone-2\",\"found\":false}".to_vec(),
                }),
                _ => proxy(arguments),
            },
        );
        assert_eq!(
            (report.removed, report.failed, report.released_bytes),
            (2, 1, 100)
        );
    }

    #[test]
    fn the_budget_bounds_the_run() {
        let rows: Vec<(String, String)> = (0..10)
            .map(|n| (format!("gone-{n}"), "anthropic".to_owned()))
            .collect();
        let rows: Vec<(&str, &str)> = rows
            .iter()
            .map(|(id, protocol)| (id.as_str(), protocol.as_str()))
            .collect();
        let archive = Archive::with(&rows);
        let removals = Cell::new(0);
        let deadline = Instant::now() + Duration::from_millis(100);
        let report = run_on(
            Some(&archive.0),
            Ok(natives(&[])),
            Some(deadline),
            &|| false,
            &mut |arguments| {
                if arguments[0] == "sessions" {
                    removals.set(removals.get() + 1);
                    std::thread::sleep(Duration::from_millis(40));
                }
                proxy(arguments)
            },
        );
        assert!(
            (1..=3).contains(&removals.get()),
            "{} removals",
            removals.get()
        );
        assert_eq!(report.stop, Stop::Budget);
        assert_eq!(report.left, 10 - removals.get());
        assert!(
            report.compacted,
            "a compaction after the budget is one command on a small file"
        );
    }
}
