//! F05: evidence-based file collisions between instances (FRD-0010 R13).
//!
//! A modified path is attributed to an instance only from the targets of that instance's own
//! tool calls. Two instances with the same worktree root that are both attributed a modified
//! path raise a collision naming both. A modified path that nobody's tool calls name, while two
//! or more instances work in that worktree, raises a "possible collision" that names no one.
//! Generated paths are excluded; equal relative paths in different worktrees never collide.
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const GENERATED: [&str; 12] = [
    "target/",
    "node_modules/",
    "dist/",
    "build/",
    ".git/",
    "__pycache__/",
    ".venv/",
    ".next/",
    "out/",
    "coverage/",
    ".turbo/",
    ".cache/",
];
const GENERATED_FILES: [&str; 5] = [
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    ".DS_Store",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceWork {
    pub instance: String,
    /// Canonical worktree root (`git rev-parse --show-toplevel`).
    pub worktree: String,
    /// Paths named by this instance's tool calls, absolute or relative to the worktree.
    pub touched: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Collision {
    FileCollision {
        worktree: String,
        path: String,
        instances: Vec<String>,
    },
    PossibleCollision {
        worktree: String,
        path: String,
    },
}

pub fn normalize(worktree: &str, path: &str) -> Option<String> {
    let root = worktree
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase();
    let mut p = path.replace('\\', "/");
    let lower = p.to_lowercase();
    if lower.starts_with(&format!("{root}/")) {
        p = p[root.len() + 1..].to_owned();
    } else if p.contains(':') || p.starts_with('/') {
        return None; // outside this worktree
    }
    let parts: Vec<&str> = p
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    if parts.contains(&"..") || parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

pub fn generated(path: &str) -> bool {
    let with_slash = format!("{path}/");
    GENERATED
        .iter()
        .any(|g| with_slash.starts_with(g) || with_slash.contains(&format!("/{g}")))
        || GENERATED_FILES
            .iter()
            .any(|f| path == *f || path.ends_with(&format!("/{f}")))
}

/// `modified`: worktree root -> modified relative paths (from `git status`).
pub fn detect(work: &[InstanceWork], modified: &BTreeMap<String, Vec<String>>) -> Vec<Collision> {
    let mut found = Vec::new();
    let key = |w: &str| w.replace('\\', "/").trim_end_matches('/').to_lowercase();
    for (worktree, paths) in modified {
        let here: Vec<&InstanceWork> = work
            .iter()
            .filter(|w| key(&w.worktree) == key(worktree))
            .collect();
        if here.is_empty() {
            continue;
        }
        let attributed: Vec<(String, BTreeSet<String>)> = here
            .iter()
            .map(|w| {
                (
                    w.instance.clone(),
                    w.touched
                        .iter()
                        .filter_map(|t| normalize(worktree, t))
                        .collect(),
                )
            })
            .collect();
        for raw in paths {
            let Some(path) = normalize(worktree, raw) else {
                continue;
            };
            if generated(&path) {
                continue;
            }
            let writers: Vec<String> = attributed
                .iter()
                .filter(|(_, set)| set.contains(&path))
                .map(|(i, _)| i.clone())
                .collect();
            match writers.len() {
                0 if here.len() >= 2 => found.push(Collision::PossibleCollision {
                    worktree: worktree.clone(),
                    path,
                }),
                n if n >= 2 => found.push(Collision::FileCollision {
                    worktree: worktree.clone(),
                    path,
                    instances: writers,
                }),
                _ => {}
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(instance: &str, worktree: &str, touched: &[&str]) -> InstanceWork {
        InstanceWork {
            instance: instance.into(),
            worktree: worktree.into(),
            touched: touched.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn two_attributed_writers_are_both_named() {
        let w = [
            work("a", "C:/repo", &["C:\\repo\\src\\lib.rs"]),
            work("b", "C:/repo", &["src/lib.rs", "README.md"]),
        ];
        let modified = BTreeMap::from([(
            "C:/repo".to_owned(),
            vec!["src/lib.rs".to_owned(), "README.md".to_owned()],
        )]);
        assert_eq!(
            detect(&w, &modified),
            [Collision::FileCollision {
                worktree: "C:/repo".into(),
                path: "src/lib.rs".into(),
                instances: vec!["a".into(), "b".into()]
            }]
        );
    }

    #[test]
    fn unattributed_changes_name_nobody_and_need_two_instances() {
        let modified = BTreeMap::from([("C:/repo".to_owned(), vec!["notes.md".to_owned()])]);
        assert_eq!(
            detect(
                &[work("a", "C:/repo", &[]), work("b", "C:/repo", &[])],
                &modified
            ),
            [Collision::PossibleCollision {
                worktree: "C:/repo".into(),
                path: "notes.md".into()
            }]
        );
        assert!(
            detect(&[work("a", "C:/repo", &[])], &modified).is_empty(),
            "one instance alone is no collision"
        );
    }

    #[test]
    fn generated_files_and_distinct_worktrees_never_collide() {
        let w = [
            work("a", "C:/repo", &["target/debug/x", "Cargo.lock"]),
            work("b", "C:/repo", &["target/debug/x", "Cargo.lock"]),
            work("c", "C:/repo-worktrees/t1", &["src/lib.rs"]),
            work("d", "C:/repo", &["src/lib.rs"]),
        ];
        let modified = BTreeMap::from([
            (
                "C:/repo".to_owned(),
                vec![
                    "target/debug/x".to_owned(),
                    "Cargo.lock".to_owned(),
                    "web/node_modules/p/i.js".to_owned(),
                ],
            ),
            (
                "C:/repo-worktrees/t1".to_owned(),
                vec!["src/lib.rs".to_owned()],
            ),
        ]);
        assert!(
            detect(&w, &modified).is_empty(),
            "{:?}",
            detect(&w, &modified)
        );
        assert_eq!(normalize("C:/repo", "C:/other/x"), None);
        assert_eq!(normalize("C:/repo", "../x"), None);
        assert_eq!(
            normalize("C:/Repo", "c:\\repo\\a\\.\\b.rs").as_deref(),
            Some("a/b.rs")
        );
    }
}
