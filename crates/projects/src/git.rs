//! C36.1: bounded git facts for project rows (FRD-0004 R08).
//!
//! `git status --porcelain=v1 -b -z` per project, at most eight at a time, each killed at its
//! deadline. Any failure is `Unknown`, never a guess. Results are cached for a short time so
//! reopening the overview does not rescan. Read-only: `GIT_OPTIONAL_LOCKS=0`, no fetch.
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const MAX_PARALLEL: usize = 8;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);
pub const CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum GitFacts {
    Known { branch: Option<String>, dirty: u64, untracked: u64 },
    NotRepository,
    Unknown { reason: String },
}

pub fn parse_status(bytes: &[u8]) -> GitFacts {
    let (mut branch, mut dirty, mut untracked) = (None, 0, 0);
    let mut records = bytes.split(|b| *b == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        let text = String::from_utf8_lossy(record);
        if let Some(head) = text.strip_prefix("## ") {
            let name = head.split("...").next().unwrap_or(head).trim();
            branch = name
                .strip_prefix("No commits yet on ")
                .map(str::to_owned)
                .or_else(|| (!name.is_empty() && !name.starts_with("HEAD (no branch)")).then(|| name.to_owned()));
            continue;
        }
        if text.len() < 3 {
            continue;
        }
        if text.starts_with("??") {
            untracked += 1;
        } else {
            dirty += 1;
        }
        if text.starts_with('R') || text.starts_with('C') {
            records.next();
        }
    }
    GitFacts::Known { branch, dirty, untracked }
}

fn has_git(path: &Path) -> bool {
    path.ancestors().any(|dir| dir.join(".git").exists())
}

pub fn facts(path: &Path, timeout: Duration) -> GitFacts {
    if !path.is_dir() {
        return GitFacts::Unknown { reason: "folder unavailable".into() };
    }
    if !has_git(path) {
        return GitFacts::NotRepository;
    }
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(path)
        .args(["status", "--porcelain=v1", "-b", "-z", "--untracked-files=normal"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(mut child) = command.spawn() else { return GitFacts::Unknown { reason: "git not available".into() } };
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut std::io::Read::take(&mut stdout, 4 * 1024 * 1024), &mut bytes);
        bytes
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = reader.join().unwrap_or_default();
    match status {
        Some(status) if status.success() => parse_status(&bytes),
        Some(_) => GitFacts::Unknown { reason: "git status failed".into() },
        None => GitFacts::Unknown { reason: "git status timed out".into() },
    }
}

/// Facts for many projects, at most [`MAX_PARALLEL`] git processes at once, in input order.
pub fn scan(paths: &[PathBuf], timeout: Duration) -> Vec<GitFacts> {
    let mut results: Vec<Option<GitFacts>> = vec![None; paths.len()];
    for (chunk_index, chunk) in paths.chunks(MAX_PARALLEL).enumerate() {
        let handles: Vec<_> = chunk
            .iter()
            .cloned()
            .map(|path| std::thread::spawn(move || facts(&path, timeout)))
            .collect();
        for (offset, handle) in handles.into_iter().enumerate() {
            results[chunk_index * MAX_PARALLEL + offset] =
                Some(handle.join().unwrap_or(GitFacts::Unknown { reason: "scan failed".into() }));
        }
    }
    results.into_iter().map(|r| r.unwrap_or(GitFacts::Unknown { reason: "scan failed".into() })).collect()
}

#[derive(Default)]
pub struct GitCache {
    entries: Mutex<HashMap<PathBuf, (Instant, GitFacts)>>,
}
impl GitCache {
    pub fn scan(&self, paths: &[PathBuf], timeout: Duration) -> Vec<GitFacts> {
        let now = Instant::now();
        let cached: Vec<Option<GitFacts>> = {
            let entries = self.entries.lock().map(|e| e.clone()).unwrap_or_default();
            paths
                .iter()
                .map(|p| entries.get(p).filter(|(at, _)| now.duration_since(*at) < CACHE_TTL).map(|(_, f)| f.clone()))
                .collect()
        };
        let missing: Vec<PathBuf> = paths.iter().zip(&cached).filter(|(_, c)| c.is_none()).map(|(p, _)| p.clone()).collect();
        let fresh = scan(&missing, timeout);
        if let Ok(mut entries) = self.entries.lock() {
            for (path, facts) in missing.iter().zip(&fresh) {
                entries.insert(path.clone(), (now, facts.clone()));
            }
            if entries.len() > 512 {
                entries.retain(|_, (at, _)| now.duration_since(*at) < CACHE_TTL);
            }
        }
        let mut fresh = fresh.into_iter();
        cached.into_iter().map(|c| c.unwrap_or_else(|| fresh.next().expect("one per missing"))).collect()
    }
    pub fn invalidate(&self, path: &Path) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(path);
        }
    }
}

/// C36.3: a linked worktree created only when the user confirmed it for a new instance
/// (FRD-0004 R16). It lives beside the project, in `<project>-worktrees/<name>`, on a new
/// branch `shinatama/<name>`; nothing existing is replaced and nothing is ever deleted here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreePlan {
    pub project: PathBuf,
    pub path: PathBuf,
    pub branch: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum WorktreeError {
    InvalidName,
    NotRepository,
    Exists { path: String },
    Git { message: String },
}
impl std::fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorktreeError::InvalidName => write!(f, "Use 1-40 lowercase letters, digits or dashes for the worktree name."),
            WorktreeError::NotRepository => write!(f, "This project is not a Git repository, so it cannot have a worktree."),
            WorktreeError::Exists { path } => write!(f, "{path} already exists; nothing was changed."),
            WorktreeError::Git { message } => write!(f, "Git refused the worktree: {message}"),
        }
    }
}

pub fn plan_worktree(project: &Path, name: &str) -> Result<WorktreePlan, WorktreeError> {
    if name.is_empty()
        || name.len() > 40
        || name.starts_with('-')
        || name.ends_with('-')
        || !name.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err(WorktreeError::InvalidName);
    }
    if !project.is_dir() || !has_git(project) {
        return Err(WorktreeError::NotRepository);
    }
    let folder = project.file_name().and_then(|n| n.to_str()).ok_or(WorktreeError::NotRepository)?;
    let parent = project.parent().ok_or(WorktreeError::NotRepository)?;
    let path = parent.join(format!("{folder}-worktrees")).join(name);
    if path.exists() {
        return Err(WorktreeError::Exists { path: path.display().to_string() });
    }
    Ok(WorktreePlan { project: project.to_path_buf(), path, branch: format!("shinatama/{name}") })
}

fn run_git(dir: &Path, args: &[&std::ffi::OsStr], timeout: Duration) -> Result<String, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|_| "git is not available".to_owned())?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git timed out".into());
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(if output.status.success() { &output.stdout } else { &output.stderr }).trim().chars().take(400).collect();
    if output.status.success() { Ok(text) } else { Err(text) }
}

/// Runs `git worktree add -b <branch> <path>` for a confirmed plan.
pub fn create_worktree(plan: &WorktreePlan) -> Result<PathBuf, WorktreeError> {
    if plan.path.exists() {
        return Err(WorktreeError::Exists { path: plan.path.display().to_string() });
    }
    if let Some(parent) = plan.path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| WorktreeError::Git { message: e.to_string() })?;
    }
    let args = [std::ffi::OsStr::new("worktree"), "add".as_ref(), "-b".as_ref(), plan.branch.as_ref(), plan.path.as_os_str()];
    match run_git(&plan.project, &args, Duration::from_secs(60)) {
        Ok(_) => Ok(plan.path.clone()),
        Err(message) => {
            // Only metadata of a worktree that never finished is pruned; files are not deleted.
            let _ = run_git(&plan.project, &["worktree".as_ref(), "prune".as_ref()], Duration::from_secs(10));
            Err(WorktreeError::Git { message })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git").arg("-C").arg(dir).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
        assert!(status.success(), "git {args:?}");
    }

    #[test]
    fn worktrees_are_planned_beside_the_project_and_never_replace_anything() {
        let base = std::env::temp_dir().join(format!("shinatama-worktree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let project = base.join("app");
        std::fs::create_dir_all(&project).unwrap();
        assert_eq!(plan_worktree(&project, "fix-1"), Err(WorktreeError::NotRepository));
        git(&project, &["init", "-q", "-b", "main"]);
        git(&project, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
        for bad in ["", "Bad", "a b", "../x", "-x", "x-", &"a".repeat(41)] {
            assert_eq!(plan_worktree(&project, bad), Err(WorktreeError::InvalidName), "{bad}");
        }
        let plan = plan_worktree(&project, "fix-1").unwrap();
        assert_eq!(plan.path, base.join("app-worktrees").join("fix-1"));
        assert_eq!(plan.branch, "shinatama/fix-1");
        let path = create_worktree(&plan).unwrap();
        assert!(path.join(".git").is_file(), "a linked worktree has a .git file");
        assert!(matches!(facts(&path, DEFAULT_TIMEOUT), GitFacts::Known { branch: Some(ref b), .. } if b == "shinatama/fix-1"));
        assert!(matches!(plan_worktree(&project, "fix-1"), Err(WorktreeError::Exists { .. })));
        assert!(matches!(create_worktree(&plan), Err(WorktreeError::Exists { .. })));
        // Same branch again from a different folder: git refuses, nothing is deleted.
        let mut clash = plan_worktree(&project, "fix-2").unwrap();
        clash.branch = "shinatama/fix-1".into();
        assert!(matches!(create_worktree(&clash), Err(WorktreeError::Git { .. })));
        assert!(path.join(".git").is_file());
        let _ = run_git(&project, &["worktree".as_ref(), "remove".as_ref(), "--force".as_ref(), path.as_os_str()], Duration::from_secs(20));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn detached_linked_bare_submodule_and_non_git_folders_report_facts_not_guesses() {
        let base = std::env::temp_dir().join(format!("shinatama-gitcases-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "one"]);
        // Detached HEAD: no branch is invented.
        let head = String::from_utf8(Command::new("git").arg("-C").arg(&repo).args(["rev-parse", "HEAD"]).output().unwrap().stdout).unwrap();
        git(&repo, &["checkout", "-q", "--detach", head.trim()]);
        assert_eq!(facts(&repo, DEFAULT_TIMEOUT), GitFacts::Known { branch: None, dirty: 0, untracked: 0 });
        git(&repo, &["checkout", "-q", "main"]);
        // Linked worktree on its own branch.
        let linked = base.join("linked");
        git(&repo, &["worktree", "add", "-q", "-b", "feature", linked.to_str().unwrap()]);
        assert!(matches!(facts(&linked, DEFAULT_TIMEOUT), GitFacts::Known { branch: Some(ref b), .. } if b == "feature"));
        // Bare repository: status is not possible, so the answer is Unknown, not a guess.
        let bare = base.join("bare.git");
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "-q", "--bare"]);
        assert!(matches!(facts(&bare, DEFAULT_TIMEOUT), GitFacts::Unknown { .. } | GitFacts::NotRepository), "{:?}", facts(&bare, DEFAULT_TIMEOUT));
        // Submodule checkout reports its own branch state.
        let sub_src = base.join("sub-src");
        std::fs::create_dir_all(&sub_src).unwrap();
        git(&sub_src, &["init", "-q", "-b", "main"]);
        git(&sub_src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "s"]);
        let added = Command::new("git").arg("-C").arg(&repo).args(["-c", "protocol.file.allow=always", "submodule", "add", "-q"]).arg(&sub_src).arg("vendor/sub").stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
        if added.success() {
            assert!(matches!(facts(&repo.join("vendor/sub"), DEFAULT_TIMEOUT), GitFacts::Known { .. }));
        }
        // Non-git folder outside any repository.
        let plain = std::env::temp_dir().join(format!("shinatama-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&plain).unwrap();
        if !plain.ancestors().any(|a| a.join(".git").exists()) {
            assert_eq!(facts(&plain, DEFAULT_TIMEOUT), GitFacts::NotRepository);
        }
        let _ = std::fs::remove_dir_all(&plain);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn parses_branch_dirty_and_untracked() {
        assert_eq!(
            parse_status(b"## main...origin/main [ahead 1]\0 M a.rs\0R  new.rs\0old.rs\0?? x\0?? y\0"),
            GitFacts::Known { branch: Some("main".into()), dirty: 2, untracked: 2 }
        );
        assert_eq!(parse_status(b"## No commits yet on dev\0"), GitFacts::Known { branch: Some("dev".into()), dirty: 0, untracked: 0 });
        assert_eq!(parse_status(b"## HEAD (no branch)\0"), GitFacts::Known { branch: None, dirty: 0, untracked: 0 });
    }

    #[test]
    fn thirty_projects_scan_in_parallel_with_unknown_for_failures() {
        let base = std::env::temp_dir().join(format!("shinatama-gitscan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mut paths = Vec::new();
        for i in 0..30 {
            let dir = base.join(format!("p{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            if i % 3 == 0 {
                git(&dir, &["init", "-q", "-b", "main"]);
                std::fs::write(dir.join("new.txt"), "x").unwrap();
            }
            paths.push(dir);
        }
        paths.push(base.join("missing"));
        let started = Instant::now();
        let results = scan(&paths, DEFAULT_TIMEOUT);
        let elapsed = started.elapsed();
        assert_eq!(results.len(), 31);
        for (i, facts) in results.iter().take(30).enumerate() {
            if i % 3 == 0 {
                assert_eq!(facts, &GitFacts::Known { branch: Some("main".into()), dirty: 0, untracked: 1 }, "{i}");
            } else {
                // Outside a repository: the temp folder itself is not under Git.
                assert!(matches!(facts, GitFacts::NotRepository | GitFacts::Known { .. }), "{i}: {facts:?}");
            }
        }
        assert!(matches!(results[30], GitFacts::Unknown { .. }));
        assert!(elapsed < Duration::from_secs(20), "30 projects took {elapsed:?}");
        eprintln!("30-project git scan took {elapsed:?}");
        let cache = GitCache::default();
        let first = cache.scan(&paths[..3], DEFAULT_TIMEOUT);
        let again_started = Instant::now();
        assert_eq!(cache.scan(&paths[..3], DEFAULT_TIMEOUT), first);
        assert!(again_started.elapsed() < Duration::from_millis(50), "cached");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_hung_git_is_killed_at_the_deadline() {
        let base = std::env::temp_dir().join(format!("shinatama-gitzero-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        git(&base, &["init", "-q"]);
        let started = Instant::now();
        let facts = facts(&base, Duration::from_millis(1));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(matches!(facts, GitFacts::Unknown { .. } | GitFacts::Known { .. }));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
