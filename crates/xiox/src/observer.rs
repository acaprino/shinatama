//! The `xiox observe <project>` child process, one per project.
//!
//! Its stdout carries only JSON lines and is read on a thread of its own; its stderr carries text
//! for a person and is drained so the child can never block on it, keeping the last line to say
//! why it ended. Closing stdin ends the observer (Xiox treats it as `stop`), so it does not
//! outlive Shinatama even when Shinatama does not get to [`Observer::stop`].
use crate::picture::{Change, Phase, Picture};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// A line longer than this is dropped; a `report` of a large project is a few megabytes.
const MAX_LINE: usize = 16 * 1024 * 1024;
const MAX_STDERR_LINE: usize = 1024;
const STOP_GRACE: Duration = Duration::from_secs(3);
const EXIT_WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct Options {
    /// Seconds between two checks, at least 60 (Xiox refuses less).
    pub interval_secs: u32,
    /// Look for copies and name-matched folders only next to the project, not on every drive.
    pub no_drives: bool,
    /// The `mukade.exe` Xiox runs to read the session histories, passed as [`MUKADE_ENV`].
    /// `None` leaves Xiox's own lookup (beside `xiox.exe`, then its vendored copy).
    pub mukade: Option<PathBuf>,
}

/// The variable Xiox reads for the `mukade.exe` it runs.
pub const MUKADE_ENV: &str = "XIOX_MUKADE_EXE";

impl Default for Options {
    fn default() -> Self {
        Self {
            interval_secs: 120,
            no_drives: false,
            mukade: None,
        }
    }
}

/// What the observer just told, passed with the picture it produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    Line(Change),
    /// The process ended: asked to, or on its own (the picture's `message` says why).
    Ended,
}

type Listener = dyn Fn(&Picture, &Update) + Send + Sync;

pub struct Observer {
    project: PathBuf,
    picture: Arc<Mutex<Picture>>,
    stdin: Mutex<Option<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    stopping: Arc<AtomicBool>,
}

impl Observer {
    /// Starts `program observe <project>`. `listener` runs on the reader thread after every
    /// line that changed the picture, and once when the process ends.
    pub fn spawn(
        program: &Path,
        project: &Path,
        options: Options,
        listener: impl Fn(&Picture, &Update) + Send + Sync + 'static,
    ) -> io::Result<Arc<Self>> {
        let mut command = Command::new(program);
        command
            .arg("observe")
            .arg(project)
            .arg("--interval")
            .arg(options.interval_secs.max(60).to_string());
        if options.no_drives {
            command.arg("--no-drives");
        }
        if let Some(mukade) = &options.mukade {
            command.env(MUKADE_ENV, mukade);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            // A check walks folders: it must never compete with the terminals for the CPU.
            const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
            command.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
        }
        let mut child = command.spawn()?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("the observer's pipes were not created"));
        };
        let observer = Arc::new(Self {
            project: project.to_path_buf(),
            picture: Arc::new(Mutex::new(Picture::default())),
            stdin: Mutex::new(Some(stdin)),
            child: Arc::new(Mutex::new(child)),
            stopping: Arc::new(AtomicBool::new(false)),
        });
        let last_stderr = Arc::new(Mutex::new(String::new()));
        let drained = Arc::clone(&last_stderr);
        std::thread::Builder::new()
            .name("xiox-observe-stderr".into())
            .spawn(move || drain(stderr, &drained))?;
        let picture = Arc::clone(&observer.picture);
        let child = Arc::clone(&observer.child);
        let stopping = Arc::clone(&observer.stopping);
        let listener: Box<Listener> = Box::new(listener);
        let started = std::thread::Builder::new()
            .name("xiox-observe".into())
            .spawn(move || {
                read(stdout, &picture, listener.as_ref());
                let status = wait(&child, EXIT_WAIT);
                let message = (!stopping.load(Ordering::Acquire)).then(|| {
                    let detail = last_stderr
                        .lock()
                        .map(|line| line.clone())
                        .unwrap_or_default();
                    match (detail.is_empty(), status) {
                        (false, _) => detail,
                        (true, Some(code)) => format!("xiox observe ended with exit code {code}."),
                        (true, None) => "xiox observe ended.".to_owned(),
                    }
                });
                let ended = {
                    let Ok(mut picture) = picture.lock() else {
                        return;
                    };
                    picture.end(message);
                    picture.clone()
                };
                notify(listener.as_ref(), &ended, &Update::Ended);
            });
        if let Err(error) = started {
            observer.stop();
            return Err(error);
        }
        Ok(observer)
    }

    /// The folder this observer was started for.
    pub fn project(&self) -> &Path {
        &self.project
    }

    pub fn picture(&self) -> Picture {
        self.picture
            .lock()
            .map(|picture| picture.clone())
            .unwrap_or_default()
    }

    pub fn ended(&self) -> bool {
        self.picture
            .lock()
            .map(|picture| picture.phase == Phase::Ended)
            .unwrap_or(true)
    }

    /// Asks for a fresh check now; its events and a `report` follow on the reader thread.
    /// False when the observer has already ended.
    pub fn report(&self) -> bool {
        let Ok(mut stdin) = self.stdin.lock() else {
            return false;
        };
        let Some(pipe) = stdin.as_mut() else {
            return false;
        };
        let written = pipe
            .write_all(b"report\n")
            .and_then(|()| pipe.flush())
            .is_ok();
        if !written {
            stdin.take();
        }
        written
    }

    /// Ends the observer: `stop`, then a short grace, then kill. Idempotent.
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
        if let Ok(mut stdin) = self.stdin.lock()
            && let Some(mut pipe) = stdin.take()
        {
            let _ = pipe.write_all(b"stop\n").and_then(|()| pipe.flush());
        }
        if wait(&self.child, STOP_GRACE).is_none()
            && let Ok(mut child) = self.child.lock()
            && child.try_wait().ok().flatten().is_none()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn read(stdout: impl Read, picture: &Mutex<Picture>, listener: &Listener) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    while let Ok(Some(whole)) = read_line(&mut reader, &mut line, MAX_LINE) {
        if !whole {
            continue;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches('\r');
        if text.trim().is_empty() {
            continue;
        }
        let (change, current) = {
            let Ok(mut picture) = picture.lock() else {
                return;
            };
            let change = picture.apply(text).unwrap_or(Change::Ignored);
            (change, picture.clone())
        };
        if change != Change::Ignored {
            notify(listener, &current, &Update::Line(change));
        }
    }
}

fn notify(listener: &Listener, picture: &Picture, update: &Update) {
    // A listener that panics must not stop the reading, or the child would block on its pipe.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| listener(picture, update)));
}

/// One line without its `\n`. `Some(false)` for a line over `max`, which is skipped whole and
/// never held in memory; `None` at the end of the stream.
pub(crate) fn read_line(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    max: usize,
) -> io::Result<Option<bool>> {
    line.clear();
    let mut over = false;
    let mut any = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(any.then_some(!over));
        }
        any = true;
        let end = buffer.iter().position(|byte| *byte == b'\n');
        let part = &buffer[..end.unwrap_or(buffer.len())];
        if !over && line.len() + part.len() > max {
            over = true;
            line.clear();
        }
        if !over {
            line.extend_from_slice(part);
        }
        let used = end.map_or(buffer.len(), |end| end + 1);
        reader.consume(used);
        if end.is_some() {
            return Ok(Some(!over));
        }
    }
}

fn drain(stderr: impl Read, last: &Mutex<String>) {
    let mut reader = BufReader::new(stderr);
    let mut line = Vec::new();
    while let Ok(Some(_)) = read_line(&mut reader, &mut line, MAX_STDERR_LINE) {
        let text: String = String::from_utf8_lossy(&line)
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        let text = text.trim();
        if !text.is_empty()
            && let Ok(mut last) = last.lock()
        {
            *last = text.to_owned();
        }
    }
}

/// The exit code once the child has ended within `limit`; `None` while it runs.
fn wait(child: &Mutex<Child>, limit: Duration) -> Option<i32> {
    let deadline = Instant::now() + limit;
    loop {
        if let Ok(mut child) = child.lock() {
            match child.try_wait() {
                Ok(Some(status)) => return Some(status.code().unwrap_or(-1)),
                Ok(None) => {}
                Err(_) => return Some(-1),
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
