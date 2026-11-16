//! What agent sessions leave behind in a project, as Xiox sees it.
//!
//! [`Observer`] runs `xiox observe <project>` (github.com/acaprino/xiox) as a child process: it
//! takes a snapshot when it starts and reports, one JSON line at a time, the git worktrees,
//! copies of the repository, folders named like the project (bench, backup, ...), build folders
//! and logs that appeared since. [`Picture`] folds those lines into what Shinatama shows: the
//! new items, the growth since the start, the thresholds crossed and Xiox's own `xiox clean`
//! command for the new recommended items.
//!
//! Read-only by construction: the observer never removes anything and this crate never runs
//! `xiox clean`. The command is shown to the person, who decides.
//!
//! [`locate`] finds the program: `SHINATAMA_XIOX` (a path, or `off`), then `xiox\xiox.exe`
//! beside the running program (or one folder up, for programs in `cli\`), then `PATH`.
mod observer;
mod picture;

pub use observer::{Observer, Options, Update};
pub use picture::{Change, MAX_NEW_ITEMS, NewItem, Phase, Picture};

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

/// Names the program, or `off` to switch the observation off.
pub const ENV: &str = "SHINATAMA_XIOX";
const EXE: &str = if cfg!(windows) { "xiox.exe" } else { "xiox" };

/// Where the observer program is, if anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Program {
    /// `SHINATAMA_XIOX=off`: nothing is observed.
    Off,
    Found(PathBuf),
    /// No program was found; the text says where it was looked for.
    Missing(String),
}

/// The program for this process: [`ENV`], beside the running program, then `PATH`.
pub fn locate() -> Program {
    locate_from(
        std::env::var_os(ENV),
        std::env::current_exe().ok(),
        std::env::var_os("PATH"),
    )
}

fn locate_from(
    variable: Option<OsString>,
    exe: Option<PathBuf>,
    path: Option<OsString>,
) -> Program {
    if let Some(value) = variable.filter(|value| !value.is_empty()) {
        if switched_off(&value) {
            return Program::Off;
        }
        let named = PathBuf::from(&value);
        return if named.is_file() {
            Program::Found(named)
        } else {
            Program::Missing(format!(
                "{ENV} names {}, which is not a file.",
                named.display()
            ))
        };
    }
    let beside = exe
        .as_deref()
        .and_then(Path::parent)
        .into_iter()
        .flat_map(|dir| [Some(dir), dir.parent()])
        .flatten()
        .map(|dir| dir.join("xiox").join(EXE));
    let on_path = path
        .iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join(EXE));
    beside
        .chain(on_path)
        .find(|candidate| candidate.is_file())
        .map(Program::Found)
        .unwrap_or_else(|| {
            Program::Missing(format!(
                "No {EXE} in the xiox folder beside Shinatama or on PATH. Set {ENV} to its path, or to off."
            ))
        })
}

fn switched_off(value: &OsStr) -> bool {
    value.to_str().is_some_and(|text| {
        ["off", "0", "false", "no"].contains(&text.trim().to_ascii_lowercase().as_str())
    })
}

#[cfg(test)]
mod tests;
