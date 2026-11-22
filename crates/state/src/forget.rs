//! FRD-0014 R27: `forget <thread>` across every store that holds a thread's text.
//!
//! Hand-off files and hop rows go first and the thread rows after, so a failure there leaves
//! the thread in place and the same command can be run again. The search index and the
//! summaries setting change only once the thread is gone, so a thread that survives a refused
//! forget keeps its index entries and its summaries pause. The agents' own sessions are never
//! touched.
use crate::database::Database;
use crate::hops::{HopError, HopJournal};
use crate::location::StorageLocation;
use crate::search_index::SearchIndex;
use crate::threads::ThreadError;
use serde::Serialize;
use std::ffi::OsStr;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgetReport {
    /// The requested thread first, then its companions.
    pub thread_ids: Vec<String>,
    pub hops: u64,
    pub handoff_files: u64,
    pub index_rows: u64,
}

#[derive(Debug)]
pub enum ForgetError {
    NotFound,
    Invalid,
    /// The thread changed since it was shown or since forget began (unarchive, rename, a new
    /// session); nothing more was deleted.
    Changed,
    /// A session of the thread or of a companion is recorded open.
    Open,
    /// A hop from or into the thread has not finished.
    HopInProgress,
    /// One store could not be read or changed; the thread is still saved.
    Storage(&'static str),
    /// The thread is gone, but its search-index copies could not be removed.
    IndexNotUpdated,
}

impl std::fmt::Display for ForgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("No saved thread has this id."),
            Self::Invalid => f.write_str("The thread id is invalid."),
            Self::Changed => f.write_str("The thread changed while it was being forgotten. It is still saved; check it and run forget again."),
            Self::Open => f.write_str("A session of this thread is open. Close it, then forget the thread."),
            Self::HopInProgress => f.write_str("A hop of this thread has not finished. Let it finish, then forget the thread."),
            Self::Storage(store) => write!(f, "The {store} could not be changed. The thread is still saved; run forget again."),
            Self::IndexNotUpdated => f.write_str("The thread was forgotten, but the search index could not be updated; its text may still appear in search."),
        }
    }
}

impl From<ThreadError> for ForgetError {
    fn from(error: ThreadError) -> Self {
        match error {
            ThreadError::NotFound => Self::NotFound,
            ThreadError::Invalid => Self::Invalid,
            ThreadError::OwnedElsewhere => Self::Open,
            ThreadError::Conflict => Self::Changed,
            _ => Self::Storage("thread database"),
        }
    }
}

fn hop_error(error: HopError) -> ForgetError {
    match error {
        HopError::ThreadBusy { .. } => ForgetError::HopInProgress,
        _ => ForgetError::Storage("hop journal"),
    }
}

/// Removes `handoffs/<hop id>/{seed,full}.md` (crates/hop/src/files.rs) and then the folder if it
/// is empty. Any other recorded path is left alone.
fn remove_handoff(hop_id: &str, seed: &Path) -> std::io::Result<u64> {
    let Some(dir) = seed.parent() else {
        return Ok(0);
    };
    if seed.file_name() != Some(OsStr::new("seed.md"))
        || dir.file_name() != Some(OsStr::new(hop_id))
        || dir.parent().and_then(Path::file_name) != Some(OsStr::new("handoffs"))
    {
        return Ok(0);
    }
    let mut removed = 0;
    for file in ["seed.md", "full.md"] {
        match std::fs::remove_file(dir.join(file)) {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    let _ = std::fs::remove_dir(dir);
    Ok(removed)
}

/// `expected_revision` is the revision the caller showed the user (the desktop); `None` takes
/// the current one (the CLI names the thread by id).
pub fn forget_thread(
    location: &StorageLocation,
    database: &Database,
    thread_id: &str,
    expected_revision: Option<u64>,
    at: i64,
) -> Result<ForgetReport, ForgetError> {
    let scope = database.thread_forget_scope(thread_id)?;
    if expected_revision.is_some_and(|revision| revision != scope.revision) {
        return Err(ForgetError::Changed);
    }
    let index = SearchIndex::open(location).map_err(|_| ForgetError::Storage("search index"))?;
    let journal = HopJournal::open(location).map_err(hop_error)?;
    let sources: Vec<(String, String)> = scope
        .sessions
        .iter()
        .map(|(harness, _, native_id)| (harness.clone(), native_id.clone()))
        .collect();
    // shortcut: a hop planned between this read and the delete below keeps its row and files;
    // the window is the length of this call, upgrade to a journal-side tombstone if hops ever start unattended.
    let hops = journal
        .for_threads(&scope.thread_ids, &sources)
        .map_err(hop_error)?;
    let mut report = ForgetReport::default();
    for (hop_id, seed) in &hops {
        report.handoff_files += remove_handoff(hop_id, Path::new(seed))
            .map_err(|_| ForgetError::Storage("hand-off files"))?;
    }
    let ids: Vec<String> = hops.into_iter().map(|(id, _)| id).collect();
    report.hops = journal.forget(&ids).map_err(hop_error)?;
    // Refused with Changed if anything moved since `scope` (same revision means same archive
    // state and segments); companions created meanwhile are re-read and deleted with it.
    let removed = database.forget_thread_records(thread_id, scope.revision, at)?;
    report.thread_ids = removed.thread_ids;
    for (_, root_id, native_id) in &removed.sessions {
        report.index_rows += index
            .forget_document(root_id, native_id)
            .map_err(|_| ForgetError::IndexNotUpdated)?;
    }
    // A stale id in this list is harmless, so a failure here does not fail the forget.
    if let Ok(excluded) = database.summaries_excluded() {
        for id in report.thread_ids.iter().filter(|id| excluded.contains(id)) {
            let _ = database.set_summaries_excluded(id, false);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::remove_handoff;

    #[test]
    fn removes_only_a_recorded_handoff_folder() {
        let base = std::env::temp_dir().join(format!("shinatama-forget-{}", std::process::id()));
        let hop = "0123456789abcdef0123456789abcdef";
        let dir = base.join("handoffs").join(hop);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("seed.md"), "s").unwrap();
        std::fs::write(dir.join("full.md"), "f").unwrap();
        let stray = base.join("seed.md");
        std::fs::write(&stray, "keep").unwrap();
        assert_eq!(
            remove_handoff(hop, &stray).unwrap(),
            0,
            "a path outside handoffs/<hop> is ignored"
        );
        assert_eq!(
            remove_handoff("ffffffffffffffffffffffffffffffff", &dir.join("seed.md")).unwrap(),
            0,
            "another hop's folder is ignored"
        );
        assert_eq!(remove_handoff(hop, &dir.join("seed.md")).unwrap(), 2);
        assert!(!dir.exists() && stray.exists());
        assert_eq!(
            remove_handoff(hop, &dir.join("seed.md")).unwrap(),
            0,
            "running again is harmless"
        );
        std::fs::remove_dir_all(base).unwrap();
    }
}
