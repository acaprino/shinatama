//! Git facts for the seed header (FRD-0005 R20) and the hand-off location (R24).
//! Read-only: `git status` with a short deadline; the common dir is found without Git.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitFacts {
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub uncommitted: u64,
    pub untracked: u64,
    pub changed_paths: Vec<String>,
}

/// The repository's common Git directory: `.git` for a main checkout, the `commondir` of a
/// linked worktree. `None` outside a repository.
pub fn common_dir(project: &Path) -> Option<PathBuf> {
    for directory in project.ancestors() {
        let dot_git = directory.join(".git");
        let Ok(meta) = std::fs::symlink_metadata(&dot_git) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            return None;
        }
        if meta.is_dir() {
            return Some(dot_git);
        }
        if meta.is_file() {
            let text = std::fs::read_to_string(&dot_git)
                .ok()
                .filter(|t| t.len() < 4096)?;
            let gitdir = text.strip_prefix("gitdir:")?.trim();
            let gitdir = if Path::new(gitdir).is_absolute() {
                PathBuf::from(gitdir)
            } else {
                directory.join(gitdir)
            };
            let common = std::fs::read_to_string(gitdir.join("commondir"))
                .ok()
                .filter(|t| t.len() < 4096);
            return Some(match common {
                Some(relative) => {
                    let relative = relative.trim();
                    if Path::new(relative).is_absolute() {
                        PathBuf::from(relative)
                    } else {
                        gitdir.join(relative)
                    }
                }
                None => gitdir,
            });
        }
    }
    None
}

/// Worktree name when `project` is a linked worktree (its `.git` is a file).
fn worktree_name(project: &Path) -> Option<String> {
    for directory in project.ancestors() {
        let dot_git = directory.join(".git");
        match std::fs::symlink_metadata(&dot_git) {
            Ok(meta) if meta.is_file() => {
                return directory
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned);
            }
            Ok(_) => return None,
            Err(_) => continue,
        }
    }
    None
}

/// `git status --porcelain=v1 -b -z`, killed after two seconds. Missing Git yields defaults.
pub fn facts(project: &Path) -> GitFacts {
    let mut facts = GitFacts {
        worktree: worktree_name(project),
        ..GitFacts::default()
    };
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(project)
        .args([
            "status",
            "--porcelain=v1",
            "-b",
            "-z",
            "--untracked-files=normal",
        ])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(mut child) = command.spawn() else {
        return facts;
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return facts;
            }
        }
    }
    let Ok(output) = child.wait_with_output() else {
        return facts;
    };
    if !output.status.success() {
        return facts;
    }
    parse_status(&output.stdout, &mut facts);
    facts
}

pub fn parse_status(bytes: &[u8], facts: &mut GitFacts) {
    let mut records = bytes.split(|b| *b == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        let text = String::from_utf8_lossy(record);
        if let Some(branch) = text.strip_prefix("## ") {
            let name = branch.split("...").next().unwrap_or(branch).trim();
            facts.branch = (!name.is_empty()
                && !name.starts_with("No commits yet on")
                && name != "HEAD (no branch)")
                .then(|| name.to_owned())
                .or_else(|| name.strip_prefix("No commits yet on ").map(str::to_owned));
            continue;
        }
        if text.len() < 4 {
            continue;
        }
        let (code, path) = text.split_at(3);
        if code.starts_with("??") {
            facts.untracked += 1;
        } else {
            facts.uncommitted += 1;
        }
        if facts.changed_paths.len() < 20 {
            facts.changed_paths.push(path.to_owned());
        }
        // A rename carries its original path as the next record.
        if code.starts_with('R') || code.starts_with('C') {
            records.next();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v1_z() {
        let mut facts = GitFacts::default();
        parse_status(
            b"## main...origin/main [ahead 1]\0 M src/a.rs\0R  new.rs\0old.rs\0?? notes.md\0",
            &mut facts,
        );
        assert_eq!(facts.branch.as_deref(), Some("main"));
        assert_eq!((facts.uncommitted, facts.untracked), (2, 1));
        assert_eq!(facts.changed_paths, ["src/a.rs", "new.rs", "notes.md"]);
    }

    #[test]
    fn finds_main_and_linked_worktree_common_dirs() {
        let base =
            std::env::temp_dir().join(format!("shinatama-git-{}", uuid::Uuid::new_v4().simple()));
        let main = base.join("main");
        std::fs::create_dir_all(main.join(".git/worktrees/feature")).unwrap();
        std::fs::write(main.join(".git/worktrees/feature/commondir"), "../..\n").unwrap();
        let linked = base.join("feature");
        std::fs::create_dir_all(linked.join("src")).unwrap();
        std::fs::write(
            linked.join(".git"),
            format!(
                "gitdir: {}\n",
                main.join(".git/worktrees/feature").display()
            ),
        )
        .unwrap();
        assert_eq!(
            common_dir(&main.join("sub")).map(|p| p.ends_with(".git")),
            Some(true)
        );
        let common = common_dir(&linked.join("src")).unwrap();
        assert_eq!(
            std::fs::canonicalize(common).unwrap(),
            std::fs::canonicalize(main.join(".git")).unwrap()
        );
        assert_eq!(
            worktree_name(&linked.join("src")).as_deref(),
            Some("feature")
        );
        std::fs::remove_dir_all(base).unwrap();
    }
}
