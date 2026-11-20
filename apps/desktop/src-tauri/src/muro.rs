//! Muro keeps the plugins of Shinatama's agents in sync (docs/specs/muro-plugin-sync.md).
//!
//! Shinatama keeps a Muro registry of its own, `<data home>\muro`, whose targets are only its
//! agent boxes. At every start, on a thread of its own, it makes sure each box that exists is a
//! target there and that automatic sync is on, then starts its Muro in the background on that
//! registry: Muro's own engine checks, plans, applies and records the state. Shinatama never
//! plans or applies, never removes a target or edits a plugin, and never touches the user's own
//! Muro. Every call names the registry: `--data` for `muroctl`, `MURO_DATA_DIR` for `Muro.exe`.
//!
//! Muro ships in the package (`muro\` beside Shinatama's program, the release that
//! scripts/muro-pin.json pins); `SHINATAMA_MURO` names another folder, `off` switches it all off.
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError, TryLockError},
    time::{Duration, Instant},
};

/// Shinatama's Muro registry, inside its data home.
pub(crate) const REGISTRY: &str = "muro";
/// The folder beside Shinatama's program that holds Muro.
const FOLDER: &str = "muro";
/// `off` switches the integration off; another value names the folder that holds Muro.
const SWITCH: &str = "SHINATAMA_MURO";
/// The Start-with-Windows name Shinatama's Muro would use, so its repair never touches the
/// user's own `Muro` value. Shinatama never turns Start with Windows on.
/// Muro accepts letters, digits, `-` and `_` only.
const AUTOSTART_NAME: &str = "Muro-Shinatama";
/// What `muroctl` says when another one-shot request holds the registry for a moment.
const BUSY: &str = "Registry is busy";
/// What a save says when the registry changed since it was read.
const STALE: &str = "Registry revision changed";
const BUSY_TRIES: u32 = 8;
const BUSY_WAIT: Duration = Duration::from_millis(750);
/// How long the exit waits for an operation of Muro to finish before leaving Muro running.
const QUIT_WAIT: Duration = Duration::from_secs(20);

/// The agents Shinatama boxes: Muro's harness, the box under `agents\`, the name shown, and the
/// command used when Muro's resolver finds no program for a new target.
const AGENTS: [(&str, &str, &str, &str); 4] = [
    ("claude-code", "claude_code", "Claude Code", "claude"),
    ("codex", "codex", "Codex", "codex"),
    ("opencode", "opencode", "OpenCode", "opencode"),
    ("pi", "pi", "Pi", "pi"),
];

/// Muro's three programs in one folder, as its portable package lays them out.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Install {
    pub(crate) muro: PathBuf,
    pub(crate) muroctl: PathBuf,
    pub(crate) mukade: PathBuf,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Located {
    Off,
    /// No complete Muro in this folder.
    Missing(PathBuf),
    Found(Install),
}

/// Where Muro is: `switch` is `SHINATAMA_MURO` (`off` in any case switches it off, another value
/// names the folder), otherwise `muro\` in `program_dir`, the folder of Shinatama's program.
pub(crate) fn locate(switch: Option<&OsStr>, program_dir: Option<&Path>) -> Located {
    let folder = match switch.filter(|value| !value.is_empty()) {
        Some(value) if value.to_string_lossy().trim().eq_ignore_ascii_case("off") => return Located::Off,
        Some(value) => PathBuf::from(value),
        None => match program_dir {
            Some(directory) => directory.join(FOLDER),
            None => return Located::Missing(PathBuf::from(FOLDER)),
        },
    };
    let tools = folder.join("tools");
    let install = Install {
        muro: folder.join("Muro.exe"),
        muroctl: tools.join("muroctl.exe"),
        mukade: tools.join("mukade.exe"),
    };
    if [&install.muro, &install.muroctl, &install.mukade].iter().all(|path| path.is_file()) {
        Located::Found(install)
    } else {
        Located::Missing(folder)
    }
}

/// The one seam to Muro: its command-line tool, its resolver and its program.
pub(crate) trait Runner: Send + Sync {
    /// `muroctl --data <registry> request <action> --input <payload>`: Muro's JSON answer, or
    /// its error text.
    fn request(&self, action: &str, payload: &Value) -> Result<Value, String>;
    /// `mukade muro-resolve <harness>`: the program Muro should configure for that harness.
    fn resolve(&self, harness: &str) -> Result<String, String>;
    /// Starts `Muro.exe` on the registry: in the notification area only with `background`,
    /// otherwise showing its window (a Muro already running on the registry raises its own).
    fn launch(&self, background: bool) -> Result<(), String>;
    /// Ends the Muro this runner started, once no operation of Muro runs: `true` when it ended,
    /// `false` when none of its own was running.
    fn quit(&self) -> Result<bool, String> {
        Ok(false)
    }
    /// Waits before a request is tried again.
    fn pause(&self, wait: Duration) {
        std::thread::sleep(wait);
    }
}

/// What the Plugins section of Settings shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    /// `missing` (not in this build), `off`, `starting`, `running` or `failed`.
    pub(crate) state: &'static str,
    /// The folder looked in when missing; the error when failed; a problem that did not stop
    /// Muro (a target that could not be saved, a state that could not be read) when running.
    pub(crate) message: Option<String>,
    /// What Muro recorded, once it runs and its state was read.
    pub(crate) sync: Option<SyncState>,
}

impl Status {
    fn new(state: &'static str, message: Option<String>) -> Self {
        Self { state, message, sync: None }
    }
}

/// Muro's recorded state, from a registry `snapshot`. Times are seconds since 1970.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncState {
    pub(crate) last_check: Option<u64>,
    pub(crate) last_sync: Option<u64>,
    /// Changes the last check planned and the syncs have not applied yet.
    pub(crate) pending: Option<u64>,
    /// Problems the last check found that keep a change from applying.
    pub(crate) blockers: Option<u64>,
    /// Whether the last check saw the registry as it is now.
    pub(crate) current: bool,
    /// Shinatama's agents registered in the registry.
    pub(crate) targets: usize,
    /// Plugins chosen in the registry.
    pub(crate) plugins: usize,
    /// The last automatic sync or Sync now, as Muro recorded it.
    pub(crate) last_run: Option<Run>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Run {
    pub(crate) time: Option<u64>,
    pub(crate) failed: bool,
    pub(crate) message: String,
}

/// Reads Muro's recorded state out of a `snapshot` answer.
pub(crate) fn sync_state(snapshot: &Value) -> SyncState {
    let epoch = |value: &Value| value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse().ok()));
    let sync = &snapshot["sync"];
    let registry = &snapshot["registry"];
    let last_run = snapshot["activity"]
        .as_array()
        .and_then(|events| events.iter().rev().find(|event| event["id"] == "automation"))
        .map(|event| Run {
            time: epoch(&event["time"]),
            failed: event["status"] == "failed",
            message: event["message"].as_str().unwrap_or_default().to_owned(),
        });
    SyncState {
        last_check: epoch(&sync["last_check"]),
        last_sync: epoch(&sync["last_sync"]),
        pending: sync["pending"].as_u64(),
        blockers: sync["blockers"].as_u64(),
        current: !sync["revision"].is_null() && sync["revision"] == registry["revision"],
        targets: registry["targets"].as_array().map_or(0, |targets| {
            targets
                .iter()
                .filter(|target| target["id"].as_str().is_some_and(|id| id.starts_with("shinatama-")))
                .count()
        }),
        plugins: registry["plugins"].as_array().map_or(0, Vec::len),
        last_run,
    }
}

/// One request, tried again while another one-shot request holds the registry.
fn request(runner: &dyn Runner, action: &str, payload: &Value) -> Result<Value, String> {
    let mut tries = 1;
    loop {
        match runner.request(action, payload) {
            Err(error) if error.contains(BUSY) && tries < BUSY_TRIES => {
                tries += 1;
                runner.pause(BUSY_WAIT);
            }
            answer => return answer,
        }
    }
}

/// A path as Muro should record it: without the `\\?\` long-path spelling of a drive path.
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_owned(),
        _ => text.into_owned(),
    }
}

/// The configuration folder Muro manages for one agent box, or `None` when the box was never
/// created: the box home for Claude Code, Codex and Pi; for OpenCode the `opencode` folder of
/// the box's XDG configuration (`home\xdg\config`, crates/engine/src/opencode_launch.rs), made
/// here when the box exists.
fn box_root(data: &Path, folder: &str, harness: &str) -> Option<PathBuf> {
    let home = data.join("agents").join(folder).join("home");
    if !home.is_dir() {
        return None;
    }
    if harness != "opencode" {
        return Some(home);
    }
    let config = home.join("xdg").join("config").join("opencode");
    std::fs::create_dir_all(&config).ok()?;
    Some(config)
}

/// The fields of a target that Shinatama sets; the others are left as Muro keeps them.
const TARGET_FIELDS: [&str; 7] = ["id", "name", "harness", "executable", "root", "exclusive", "enabled"];

/// Makes each agent box that exists a target of the registry: created when missing, saved again
/// only when it differs, always with the revision just read. A new target is enabled; one that
/// exists keeps the enabled choice made in Muro's window. Returns the problems of the targets
/// that could not be saved; an unreadable registry fails it all.
fn ensure_targets(runner: &dyn Runner, data: &Path) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for (harness, folder, name, command) in AGENTS {
        let Some(root) = box_root(data, folder, harness) else {
            continue;
        };
        let id = format!("shinatama-{harness}");
        let mut resolved: Option<Result<String, String>> = None;
        let mut attempt = 0;
        loop {
            attempt += 1;
            let registry = request(runner, "registry", &json!({}))?;
            let existing = registry["targets"]
                .as_array()
                .and_then(|targets| targets.iter().find(|target| target["id"] == id.as_str()))
                .cloned();
            let known = existing.as_ref().and_then(|target| target["executable"].as_str());
            // Muro's resolver only for a new target or a program that is gone: one process less
            // per agent at every start.
            let executable = match known.filter(|program| Path::new(program).is_file()) {
                Some(program) => program.to_owned(),
                None => match resolved.get_or_insert_with(|| runner.resolve(harness)) {
                    Ok(program) => program.clone(),
                    Err(_) => known.unwrap_or(command).to_owned(),
                },
            };
            let wanted = json!({
                "id": &id,
                "name": format!("Shinatama · {name}"),
                "harness": harness,
                "executable": executable,
                "root": plain(&root),
                "exclusive": false,
                "enabled": existing.as_ref().and_then(|target| target["enabled"].as_bool()).unwrap_or(true),
            });
            if existing.as_ref().is_some_and(|target| TARGET_FIELDS.iter().all(|key| target[key] == wanted[key])) {
                break;
            }
            let mut target = existing.unwrap_or_else(|| json!({}));
            for key in TARGET_FIELDS {
                target[key] = wanted[key].clone();
            }
            let payload = json!({"revision": registry["revision"], "target": target});
            match request(runner, "save_target", &payload) {
                Ok(_) => {
                    crate::app_log::write("muro", &format!("{name}: target {id} saved ({}).", plain(&root)));
                    break;
                }
                Err(error) if error.contains(STALE) && attempt < 3 => {}
                Err(error) => {
                    crate::app_log::write("muro", &format!("{name}: target {id} not saved: {error}"));
                    problems.push(format!("{name} could not be added to Muro: {error}"));
                    break;
                }
            }
        }
        if let Some(Err(error)) = resolved {
            crate::app_log::write("muro", &format!("{name}: Muro's resolver found no program ({error})."));
        }
    }
    Ok(problems)
}

/// Turns the registry's automatic sync on, keeping every other setting as it is.
fn ensure_auto_sync(runner: &dyn Runner) -> Result<(), String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let registry = request(runner, "registry", &json!({}))?;
        let mut settings = registry["settings"].clone();
        if !settings.is_object() {
            return Err("Muro's registry has no settings.".into());
        }
        if settings["auto_sync"] == true {
            return Ok(());
        }
        settings["auto_sync"] = json!(true);
        match request(runner, "settings", &json!({"revision": registry["revision"], "settings": settings})) {
            Ok(_) => {
                crate::app_log::write("muro", "Automatic sync switched on.");
                return Ok(());
            }
            Err(error) if error.contains(STALE) && attempt < 3 => {}
            Err(error) => return Err(error),
        }
    }
}

/// Shinatama's Muro: what it found, what it started and what the page is told.
pub(crate) struct Muro {
    /// `None` when Muro is switched off, missing or the data home is unknown: `status` says why.
    runner: Option<Arc<dyn Runner>>,
    data: PathBuf,
    status: Mutex<Status>,
    /// Held by the start and by Sync now: one at a time, and a second Sync now waits for the
    /// first and gets its result.
    work: Mutex<()>,
    launched: Mutex<bool>,
    /// What the start could not do (a target Muro refused): said with every status until the
    /// next start, unlike a read or a Sync now that failed once.
    problems: Mutex<Option<String>>,
}

impl Muro {
    /// `data` is Shinatama's data home; `runner` is `None` when `status` already says why there
    /// is no Muro to run.
    pub(crate) fn new(runner: Option<Arc<dyn Runner>>, data: PathBuf, status: Status) -> Self {
        Self { runner, data, status: Mutex::new(status), work: Mutex::new(()), launched: Mutex::new(false), problems: Mutex::new(None) }
    }

    /// The Muro the app uses: found beside the program or through `SHINATAMA_MURO`.
    pub(crate) fn for_app(data: Result<PathBuf, String>) -> Self {
        let data = match data {
            Ok(data) => data,
            Err(error) => {
                let status = Status::new("failed", Some(format!("Shinatama's data folder is unavailable: {error}")));
                return Self::new(None, PathBuf::new(), status);
            }
        };
        let program = std::env::current_exe().ok();
        let switch = std::env::var_os(SWITCH);
        match locate(switch.as_deref(), program.as_deref().and_then(Path::parent)) {
            Located::Off => Self::new(None, data, Status::new("off", None)),
            Located::Missing(folder) => Self::new(None, data, Status::new("missing", Some(plain(&folder)))),
            Located::Found(install) => {
                let registry = data.join(REGISTRY);
                let runner: Arc<dyn Runner> = Arc::new(System { install, registry, running: Mutex::new(None) });
                Self::new(Some(runner), data, Status::new("starting", None))
            }
        }
    }

    fn set(&self, status: Status) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }

    fn current(&self) -> Status {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// The start sequence: the registry folder, the targets, automatic sync on, then Muro in the
    /// background. Each step is logged; the status says where it stopped.
    pub(crate) fn start(&self) {
        let Some(runner) = self.runner.as_deref() else {
            let status = self.current();
            crate::app_log::write("muro", &match status.state {
                "off" => "Switched off (SHINATAMA_MURO=off).".to_owned(),
                "missing" => format!("Not in this build: no Muro in {}.", status.message.unwrap_or_default()),
                _ => format!("Not started: {}", status.message.unwrap_or_default()),
            });
            return;
        };
        let _work = self.work.lock().unwrap_or_else(PoisonError::into_inner);
        self.set(Status::new("starting", None));
        let registry = self.data.join(REGISTRY);
        let outcome = std::fs::create_dir_all(&registry)
            .map_err(|error| format!("The registry folder {} could not be created: {error}", plain(&registry)))
            .and_then(|()| ensure_targets(runner, &self.data))
            .and_then(|problems| ensure_auto_sync(runner).map(|()| problems))
            .and_then(|problems| runner.launch(true).map(|()| problems));
        match outcome {
            Ok(problems) => {
                *self.launched.lock().unwrap_or_else(PoisonError::into_inner) = true;
                crate::app_log::write("muro", &format!("Started in the background on {}.", plain(&registry)));
                *self.problems.lock().unwrap_or_else(PoisonError::into_inner) = (!problems.is_empty()).then(|| problems.join(" "));
                self.refresh(runner, Status::new("running", None));
            }
            Err(error) => {
                crate::app_log::write("muro", &format!("Not started: {error}"));
                self.set(Status::new("failed", Some(error)));
            }
        }
    }

    /// Muro's recorded state, asked once: `None` while Muro's own operation holds the registry
    /// (it is working; the last state stays), an error when it cannot be read.
    fn read_state(&self, runner: &dyn Runner) -> Result<Option<SyncState>, String> {
        match runner.request("snapshot", &json!({})) {
            Ok(snapshot) => {
                let state = sync_state(&snapshot);
                if let Some(run) = state.last_run.as_ref().filter(|run| run.failed) {
                    crate::app_log::write("muro", &format!("Muro recorded a failed sync: {}", run.message));
                }
                Ok(Some(state))
            }
            Err(error) if error.contains(BUSY) => Ok(None),
            Err(error) => Err(format!("Muro's state could not be read: {error}")),
        }
    }

    /// The status, with Muro's recorded state read again while it runs. While a start or a
    /// sync is in progress it answers at once with what it knows; the read itself never holds
    /// the work, so a Sync now pressed meanwhile is never taken for one already running.
    pub(crate) fn status(&self) -> Status {
        let status = self.current();
        let (Some(runner), "running") = (self.runner.as_deref(), status.state) else {
            return status;
        };
        if self.work.try_lock().is_err() {
            return status;
        }
        self.refresh(runner, status)
    }

    /// Reads the state again. The message becomes the start's problems, plus the read error
    /// when there is one: a failure said once does not stay.
    fn refresh(&self, runner: &dyn Runner, mut status: Status) -> Status {
        let problems = self.problems.lock().unwrap_or_else(PoisonError::into_inner).clone();
        match self.read_state(runner) {
            Ok(Some(state)) => {
                status.sync = Some(state);
                status.message = problems;
            }
            Ok(None) => status.message = problems,
            Err(error) => status.message = Some(joined(problems, error)),
        }
        self.set(status.clone());
        status
    }

    /// Asks Muro to check and apply now. A Sync now pressed while one runs waits for it and
    /// answers with its result. A start that failed is tried again first.
    pub(crate) fn sync_now(&self) -> Result<Status, String> {
        let runner = self.runner.as_deref().ok_or_else(|| unavailable(&self.current()))?;
        if self.current().state == "failed" {
            self.start();
        }
        let status = self.current();
        if status.state != "running" {
            return Err(unavailable(&status));
        }
        let _work = match self.work.try_lock() {
            Ok(work) => work,
            Err(TryLockError::WouldBlock) => {
                drop(self.work.lock().unwrap_or_else(PoisonError::into_inner));
                return Ok(self.status());
            }
            Err(TryLockError::Poisoned(work)) => work.into_inner(),
        };
        crate::app_log::write("muro", "Sync now.");
        let mut status = status;
        match request(runner, "sync_now", &json!({"apply": true})) {
            Ok(answer) => {
                crate::app_log::write("muro", &format!("Sync now done: {answer}"));
                status = self.refresh(runner, status);
            }
            Err(error) => {
                crate::app_log::write("muro", &format!("Sync now failed: {error}"));
                status = self.refresh(runner, status);
                let problems = self.problems.lock().unwrap_or_else(PoisonError::into_inner).clone();
                status.message = Some(joined(problems, format!("Sync now failed: {error}")));
                self.set(status.clone());
            }
        }
        Ok(status)
    }

    /// Shows the window of Shinatama's Muro.
    pub(crate) fn open(&self) -> Result<(), String> {
        let runner = self.runner.as_deref().ok_or_else(|| unavailable(&self.current()))?;
        runner.launch(false).inspect_err(|error| crate::app_log::write("muro", &format!("Open Muro failed: {error}")))
    }

    /// Shinatama is ending, and its Muro ends with it once no operation of Muro runs: Muro
    /// offers no quit request, so the exit waits for the registry's own lock (an apply is never
    /// cut off), then ends the Muro it started. Past the wait Muro keeps running, said in the log.
    pub(crate) fn at_exit(&self) {
        // A start still at work finishes first, so the Muro it launches is the one ended; the
        // work stays held, so no Sync now starts meanwhile.
        let deadline = Instant::now() + QUIT_WAIT;
        let _work = loop {
            match self.work.try_lock() {
                Ok(work) => break Some(work),
                Err(TryLockError::Poisoned(work)) => break Some(work.into_inner()),
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(200)),
                Err(TryLockError::WouldBlock) => {
                    crate::app_log::write("muro", "Shinatama ended while its Muro was still starting: that Muro may keep running.");
                    break None;
                }
            }
        };
        let launched = *self.launched.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(runner) = self.runner.as_deref().filter(|_| launched) else {
            return;
        };
        match runner.quit() {
            Ok(true) => crate::app_log::write("muro", "Shinatama's Muro ended with Shinatama."),
            Ok(false) => {}
            Err(error) => crate::app_log::write(
                "muro",
                &format!("Shinatama's Muro keeps running ({error}). Quit it from its tray icon."),
            ),
        }
    }
}

/// The start's problems and one more, in one sentence each.
fn joined(problems: Option<String>, more: String) -> String {
    match problems {
        Some(problems) => format!("{problems} {more}"),
        None => more,
    }
}

/// Why Sync now and Open Muro cannot run, in the page's words.
fn unavailable(status: &Status) -> String {
    match status.state {
        "off" => "Muro is switched off (SHINATAMA_MURO=off).".into(),
        "missing" => "Muro is not in this build.".into(),
        "starting" => "Muro is still starting.".into(),
        _ => status.message.clone().unwrap_or_else(|| "Muro did not start.".into()),
    }
}

/// The real runner: Muro's programs, always on Shinatama's registry.
struct System {
    install: Install,
    registry: PathBuf,
    /// The Muro this runner started and that kept running: its handle, so its process number
    /// cannot go to another program before the exit ends it.
    running: Mutex<Option<std::process::Child>>,
}

impl System {
    /// A Muro process: on Shinatama's registry, with no WebView2 setting of Shinatama's own and
    /// a Start-with-Windows name that is not the user's Muro's.
    fn command(&self, program: &Path) -> std::process::Command {
        let mut command = std::process::Command::new(program);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().to_ascii_uppercase().starts_with("WEBVIEW2_") {
                command.env_remove(name);
            }
        }
        // The plain drive spelling: Muro records the path, and a `\\?\` working folder breaks
        // the cmd and npm launchers Muro runs for some harnesses.
        let registry = plain(&self.registry);
        command
            .env("MURO_DATA_DIR", &registry)
            .env("MURO_AUTOSTART_NAME", AUTOSTART_NAME)
            .current_dir(&registry)
            .stdin(std::process::Stdio::null());
        command
    }

    /// A console helper that answers on its output: no window, its error on failure.
    fn helper(&self, mut command: std::process::Command) -> Result<Vec<u8>, String> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let output = command.output().map_err(|error| format!("{} could not start: {error}", command.get_program().to_string_lossy()))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            Err(if error.is_empty() { format!("ended with {}", output.status) } else { error })
        }
    }
}

impl Runner for System {
    fn request(&self, action: &str, payload: &Value) -> Result<Value, String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let input = std::env::temp_dir().join(format!("shinatama-muro-{}-{number}.json", std::process::id()));
        std::fs::write(&input, payload.to_string()).map_err(|error| format!("The request could not be written: {error}"))?;
        let mut command = self.command(&self.install.muroctl);
        command.arg("--data").arg(plain(&self.registry)).args(["request", action, "--input"]).arg(&input);
        let output = self.helper(command);
        let _ = std::fs::remove_file(&input);
        serde_json::from_slice(&output?).map_err(|error| format!("Muro's answer to {action} is not JSON: {error}"))
    }

    fn resolve(&self, harness: &str) -> Result<String, String> {
        let mut command = self.command(&self.install.mukade);
        command.args(["muro-resolve", harness]);
        let answer: Value = serde_json::from_slice(&self.helper(command)?).map_err(|error| error.to_string())?;
        answer["executable"].as_str().filter(|path| !path.is_empty()).map(str::to_owned).ok_or_else(|| format!("no {harness} program found"))
    }

    fn launch(&self, background: bool) -> Result<(), String> {
        // Muro reads its registry once as it starts and ends (exit code 101) when another
        // request holds the registry at that moment: no request goes out while it starts (the
        // start holds the work), and a background start that ended that way is made once more.
        match self.spawn(background) {
            Err(error) if background && error.contains("Muro ended as it started") => {
                crate::app_log::write("muro", &format!("{error} Starting it once more."));
                self.spawn(background)
            }
            result => result,
        }
    }

    fn quit(&self) -> Result<bool, String> {
        self.end()
    }
}

impl System {
    fn spawn(&self, background: bool) -> Result<(), String> {
        let mut command = self.command(&self.install.muro);
        if background {
            command.arg("--background");
        }
        command.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        let mut child = {
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                // Its own process, outside any job Shinatama runs in: ending Shinatama never
                // cuts a sync off.
                const DETACHED_PROCESS: u32 = 0x0000_0008;
                const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
                const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
                let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
                command
                    .creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB)
                    .spawn()
                    .or_else(|_| command.creation_flags(flags).spawn())
            }
            #[cfg(not(windows))]
            {
                command.spawn()
            }
        }
        .map_err(|error| format!("Muro could not start: {error}"))?;
        // A Muro that ends at once either handed over to the one already running on this
        // registry (success) or could not start (its exit code). A background start is watched
        // longer: its first read of the registry comes after its window system is up.
        for _ in 0..if background { 50 } else { 20 } {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => return Err(format!("Muro ended as it started ({status}).")),
                Ok(None) => std::thread::sleep(Duration::from_millis(150)),
                Err(_) => return Ok(()),
            }
        }
        // Still running: this is the Muro of the registry, the one the exit ends.
        *self.running.lock().unwrap_or_else(PoisonError::into_inner) = Some(child);
        Ok(())
    }

    fn end(&self) -> Result<bool, String> {
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(child) = running.as_mut() else {
            return Ok(false);
        };
        if child.try_wait().ok().flatten().is_some() {
            *running = None;
            return Ok(false);
        }
        // Every operation of Muro holds `writer.lock` of the registry: holding it waits for an
        // apply that runs and keeps the next one from starting while Muro is ended.
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.registry.join("writer.lock"))
            .map_err(|error| format!("its registry lock could not be opened: {error}"))?;
        let deadline = Instant::now() + QUIT_WAIT;
        while lock.try_lock().is_err() {
            if Instant::now() >= deadline {
                return Err("an operation of Muro was still running".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        // Muro and its own helpers (its request server): the whole tree, by the number the open
        // handle keeps reserved.
        let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let mut command = std::process::Command::new(system.join("System32").join("taskkill.exe"));
        command.args(["/PID", &child.id().to_string(), "/T", "/F"]);
        self.helper(command).map_err(|error| format!("it could not be ended: {error}"))?;
        let _ = child.wait();
        *running = None;
        Ok(true)
    }
}

/// Starts Shinatama's Muro on a thread of its own, never holding up the window.
pub(crate) fn start(muro: Arc<Muro>) {
    let _ = std::thread::Builder::new().name("muro-start".into()).spawn(move || muro.start());
}

/// Runs `work` off the event loop: Muro's programs take a moment to answer.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|_| "Muro's worker stopped.".to_owned())
}

#[tauri::command]
pub(crate) async fn muro_status(window: tauri::WebviewWindow, muro: tauri::State<'_, Arc<Muro>>) -> Result<Status, String> {
    crate::local(&window)?;
    let muro = muro.inner().clone();
    blocking(move || muro.status()).await
}

#[tauri::command]
pub(crate) async fn muro_sync_now(window: tauri::WebviewWindow, muro: tauri::State<'_, Arc<Muro>>) -> Result<Status, String> {
    crate::local(&window)?;
    let muro = muro.inner().clone();
    blocking(move || muro.sync_now()).await?
}

#[tauri::command]
pub(crate) async fn muro_open(window: tauri::WebviewWindow, muro: tauri::State<'_, Arc<Muro>>) -> Result<(), String> {
    crate::local(&window)?;
    let muro = muro.inner().clone();
    blocking(move || muro.open()).await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Muro as the tests need it: a registry kept in memory that answers `registry`, `save_target`,
    /// `settings`, `snapshot` and `sync_now`, with scripted failures in front.
    #[derive(Default)]
    struct Fake {
        registry: Mutex<Value>,
        snapshot: Mutex<Value>,
        /// Errors returned, in order, before the real answers.
        failures: Mutex<VecDeque<String>>,
        calls: Mutex<Vec<(String, Value)>>,
        resolved: Mutex<Option<Result<String, String>>>,
        launches: Mutex<Vec<bool>>,
        launch_error: Mutex<Option<String>>,
        pauses: Mutex<u32>,
        quits: Mutex<u32>,
    }

    impl Fake {
        fn new() -> Arc<Self> {
            let fake = Self::default();
            *fake.registry.lock().unwrap() = json!({"schema":1,"revision":4,"plugins":[],"targets":[],
                "settings":{"auto_sync":false,"interval_minutes":15,"agent":"codex","python":"py","factory_path":"F","close_to_tray":false}});
            *fake.resolved.lock().unwrap() = Some(Ok("C:\\bin\\agent.exe".into()));
            Arc::new(fake)
        }
        fn calls(&self, action: &str) -> Vec<Value> {
            self.calls.lock().unwrap().iter().filter(|(name, _)| name == action).map(|(_, payload)| payload.clone()).collect()
        }
        fn fail(&self, error: &str, times: usize) {
            self.failures.lock().unwrap().extend(std::iter::repeat_n(error.to_owned(), times));
        }
    }

    impl Runner for Fake {
        fn request(&self, action: &str, payload: &Value) -> Result<Value, String> {
            self.calls.lock().unwrap().push((action.to_owned(), payload.clone()));
            if let Some(error) = self.failures.lock().unwrap().pop_front() {
                return Err(error);
            }
            let mut registry = self.registry.lock().unwrap();
            let revision = registry["revision"].as_u64().unwrap();
            match action {
                "registry" => Ok(registry.clone()),
                "save_target" | "settings" => {
                    if payload["revision"].as_u64() != Some(revision) {
                        return Err("Registry revision changed; refresh before saving".into());
                    }
                    if action == "settings" {
                        registry["settings"] = payload["settings"].clone();
                    } else {
                        let targets = registry["targets"].as_array_mut().unwrap();
                        targets.retain(|target| target["id"] != payload["target"]["id"]);
                        targets.push(payload["target"].clone());
                    }
                    registry["revision"] = json!(revision + 1);
                    Ok(json!({"registry": registry.clone()}))
                }
                "snapshot" => Ok(self.snapshot.lock().unwrap().clone()),
                "sync_now" => Ok(json!({"applied":1,"pending":0,"blockers":[],"errors":[]})),
                other => Err(format!("Unknown registry action: {other}")),
            }
        }
        fn resolve(&self, _harness: &str) -> Result<String, String> {
            self.resolved.lock().unwrap().clone().unwrap()
        }
        fn launch(&self, background: bool) -> Result<(), String> {
            self.launches.lock().unwrap().push(background);
            self.launch_error.lock().unwrap().clone().map_or(Ok(()), Err)
        }
        fn pause(&self, _wait: Duration) {
            *self.pauses.lock().unwrap() += 1;
        }
        fn quit(&self) -> Result<bool, String> {
            *self.quits.lock().unwrap() += 1;
            Ok(true)
        }
    }

    /// A data home with the boxes of `agents` (box folder names).
    fn home(agents: &[&str]) -> PathBuf {
        let data = std::env::temp_dir().join(format!("shinatama-muro-test-{}", uuid::Uuid::new_v4()));
        for agent in agents {
            std::fs::create_dir_all(data.join("agents").join(agent).join("home")).unwrap();
        }
        data
    }

    fn muro(fake: &Arc<Fake>, data: &Path) -> Muro {
        Muro::new(Some(fake.clone() as Arc<dyn Runner>), data.to_path_buf(), Status::new("starting", None))
    }

    #[test]
    fn each_box_that_exists_becomes_a_target_with_the_revision_just_read() {
        let data = home(&["claude_code", "opencode"]);
        let fake = Fake::new();
        let problems = ensure_targets(fake.as_ref(), &data).unwrap();
        assert!(problems.is_empty());
        let saved = fake.calls("save_target");
        assert_eq!(saved.len(), 2, "codex and pi have no box: skipped");
        assert_eq!(saved[0]["revision"], 4);
        assert_eq!(saved[1]["revision"], 5);
        assert_eq!(
            saved[0]["target"],
            json!({"id":"shinatama-claude-code","name":"Shinatama · Claude Code","harness":"claude-code",
                   "executable":"C:\\bin\\agent.exe","root":plain(&data.join("agents").join("claude_code").join("home")),"exclusive":false,"enabled":true})
        );
        let opencode = data.join("agents").join("opencode").join("home").join("xdg").join("config").join("opencode");
        assert!(opencode.is_dir(), "the OpenCode config folder is made inside its box");
        assert_eq!(saved[1]["target"]["root"], plain(&opencode));
        assert_eq!(saved[1]["target"]["harness"], "opencode");
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn an_equal_target_is_not_saved_again_and_a_changed_one_keeps_its_enabled_choice() {
        let data = home(&["codex", "pi"]);
        let fake = Fake::new();
        ensure_targets(fake.as_ref(), &data).unwrap();
        assert_eq!(fake.calls("save_target").len(), 2);
        ensure_targets(fake.as_ref(), &data).unwrap();
        assert_eq!(fake.calls("save_target").len(), 2, "nothing differs: nothing saved");
        // The user disables Pi in Muro's window, and Pi's program moves.
        {
            let mut registry = fake.registry.lock().unwrap();
            let targets = registry["targets"].as_array_mut().unwrap();
            let pi = targets.iter_mut().find(|target| target["id"] == "shinatama-pi").unwrap();
            pi["enabled"] = json!(false);
            pi["note"] = json!("kept");
        }
        *fake.resolved.lock().unwrap() = Some(Ok("D:\\pi.cmd".into()));
        ensure_targets(fake.as_ref(), &data).unwrap();
        let saved = fake.calls("save_target");
        let pi = saved.iter().rev().find(|payload| payload["target"]["id"] == "shinatama-pi").unwrap();
        assert_eq!(pi["target"]["executable"], "D:\\pi.cmd");
        assert_eq!(pi["target"]["enabled"], false);
        assert_eq!(pi["target"]["note"], "kept", "a field Shinatama does not set is left as it was");
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_program_the_resolver_cannot_find_falls_back_to_the_known_one_or_the_command() {
        let data = home(&["codex"]);
        let fake = Fake::new();
        *fake.resolved.lock().unwrap() = Some(Err("no codex program found".into()));
        ensure_targets(fake.as_ref(), &data).unwrap();
        assert_eq!(fake.calls("save_target")[0]["target"]["executable"], "codex");
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_target_muro_refuses_is_a_problem_not_a_failure() {
        let data = home(&["claude_code"]);
        let fake = Fake::new();
        // The registry read answers; the save is refused.
        let problems = ensure_targets(&Refusing(fake.clone()), &data).unwrap();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("Claude Code could not be added to Muro: Target configuration directories overlap"));
        let _ = std::fs::remove_dir_all(data);
    }

    /// A runner whose saves Muro refuses.
    struct Refusing(Arc<Fake>);
    impl Runner for Refusing {
        fn request(&self, action: &str, payload: &Value) -> Result<Value, String> {
            if action == "save_target" { Err("Target configuration directories overlap".into()) } else { self.0.request(action, payload) }
        }
        fn resolve(&self, harness: &str) -> Result<String, String> { self.0.resolve(harness) }
        fn launch(&self, background: bool) -> Result<(), String> { self.0.launch(background) }
        fn pause(&self, wait: Duration) { self.0.pause(wait) }
    }

    #[test]
    fn automatic_sync_is_switched_on_keeping_every_other_setting() {
        let fake = Fake::new();
        ensure_auto_sync(fake.as_ref()).unwrap();
        let saved = fake.calls("settings");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0]["revision"], 4);
        assert_eq!(
            saved[0]["settings"],
            json!({"auto_sync":true,"interval_minutes":15,"agent":"codex","python":"py","factory_path":"F","close_to_tray":false})
        );
        ensure_auto_sync(fake.as_ref()).unwrap();
        assert_eq!(fake.calls("settings").len(), 1, "already on: nothing saved");
    }

    #[test]
    fn a_busy_registry_is_tried_again_and_then_given_up() {
        let fake = Fake::new();
        fake.fail("Registry is busy; wait for the running operation: os error 33", 3);
        assert_eq!(request(fake.as_ref(), "registry", &json!({})).unwrap()["revision"], 4);
        assert_eq!(*fake.pauses.lock().unwrap(), 3);
        fake.fail("Registry is busy; wait for the running operation", BUSY_TRIES as usize);
        assert!(request(fake.as_ref(), "registry", &json!({})).unwrap_err().contains(BUSY));
        fake.fail("Unsupported or corrupt registry schema", 1);
        let before = *fake.pauses.lock().unwrap();
        assert!(request(fake.as_ref(), "registry", &json!({})).is_err());
        assert_eq!(*fake.pauses.lock().unwrap(), before, "another error is not tried again");
    }

    #[test]
    fn the_status_reads_what_muro_recorded() {
        let snapshot = json!({
            "registry": {"revision": 9, "targets": [{"id":"shinatama-codex"},{"id":"shinatama-pi"},{"id":"mine"}], "plugins": [{"id":"a"}]},
            "sync": {"last_check":"1763560000","last_sync":"1763559000","revision":9,"pending":2,"blockers":1,"targets":{}},
            "activity": [{"id":"x","time":"1","action":"apply","status":"complete","message":"m"},
                         {"id":"automation","time":"1763560001","action":"Automatic sync","status":"failed","message":"Codex: cannot read"}],
        });
        assert_eq!(
            sync_state(&snapshot),
            SyncState {
                last_check: Some(1_763_560_000),
                last_sync: Some(1_763_559_000),
                pending: Some(2),
                blockers: Some(1),
                current: true,
                targets: 2,
                plugins: 1,
                last_run: Some(Run { time: Some(1_763_560_001), failed: true, message: "Codex: cannot read".into() }),
            }
        );
        let fresh = json!({"registry":{"revision":0,"targets":[],"plugins":[]},
            "sync":{"last_check":null,"last_sync":null,"revision":null,"pending":null,"blockers":null,"targets":{}},"activity":[]});
        let state = sync_state(&fresh);
        assert_eq!((state.last_check, state.pending, state.current, state.last_run), (None, None, false, None));
        let stale = json!({"registry":{"revision":3},"sync":{"revision":2}});
        assert!(!sync_state(&stale).current, "a check of an older registry is not current");
    }

    #[test]
    fn the_start_saves_targets_switches_sync_on_and_launches_in_the_background() {
        let data = home(&["claude_code"]);
        let fake = Fake::new();
        *fake.snapshot.lock().unwrap() = json!({"registry":{"revision":6,"targets":[{"id":"shinatama-claude-code"}],"plugins":[]},
            "sync":{"revision":6,"pending":0,"last_check":"5"},"activity":[]});
        let muro = muro(&fake, &data);
        muro.start();
        assert!(data.join(REGISTRY).is_dir());
        let order: Vec<String> = fake.calls.lock().unwrap().iter().map(|(action, _)| action.clone()).filter(|action| action != "registry").collect();
        assert_eq!(order, ["save_target", "settings", "snapshot"]);
        assert_eq!(*fake.launches.lock().unwrap(), [true]);
        let status = muro.current();
        assert_eq!((status.state, status.message.as_deref()), ("running", None));
        assert_eq!(status.sync.unwrap().last_check, Some(5));
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_launch_that_fails_is_the_status() {
        let data = home(&[]);
        let fake = Fake::new();
        *fake.launch_error.lock().unwrap() = Some("Muro ended as it started (exit code: 101).".into());
        let muro = muro(&fake, &data);
        muro.start();
        let status = muro.current();
        assert_eq!(status.state, "failed");
        assert_eq!(status.message.as_deref(), Some("Muro ended as it started (exit code: 101)."));
        // Sync now tries the start again first.
        *fake.launch_error.lock().unwrap() = None;
        let status = muro.sync_now().unwrap();
        assert_eq!(status.state, "running");
        assert_eq!(fake.calls("sync_now"), [json!({"apply": true})]);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn sync_now_asks_muro_and_answers_with_the_new_state() {
        let data = home(&[]);
        let fake = Fake::new();
        let muro = muro(&fake, &data);
        muro.start();
        *fake.snapshot.lock().unwrap() = json!({"registry":{"revision":5},"sync":{"revision":5,"pending":0,"last_sync":"77"},
            "activity":[{"id":"automation","time":"77","status":"complete","message":"1 change applied"}]});
        let status = muro.sync_now().unwrap();
        assert_eq!(status.sync.as_ref().unwrap().last_sync, Some(77));
        assert_eq!(status.sync.unwrap().last_run.unwrap().message, "1 change applied");
        fake.fail("Registry is busy; wait for the running operation", BUSY_TRIES as usize);
        let status = muro.sync_now().unwrap();
        assert!(status.message.unwrap().starts_with("Sync now failed: Registry is busy"));
        // A failure said once does not stay: the next read clears it.
        assert_eq!(muro.status().message, None);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn the_exit_ends_only_a_muro_that_was_started() {
        let data = home(&[]);
        let fake = Fake::new();
        let muro = muro(&fake, &data);
        muro.at_exit();
        assert_eq!(*fake.quits.lock().unwrap(), 0, "nothing started, nothing to end");
        muro.start();
        muro.at_exit();
        assert_eq!(*fake.quits.lock().unwrap(), 1);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_sync_now_during_another_waits_for_it_instead_of_starting_one() {
        let data = home(&[]);
        let fake = Fake::new();
        let muro = Arc::new(muro(&fake, &data));
        muro.start();
        let held = muro.work.lock().unwrap();
        let waiting = {
            let muro = muro.clone();
            std::thread::spawn(move || muro.sync_now())
        };
        std::thread::sleep(Duration::from_millis(100));
        drop(held);
        assert_eq!(waiting.join().unwrap().unwrap().state, "running");
        assert!(fake.calls("sync_now").is_empty(), "it took the running sync's result");
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn muro_switched_off_or_missing_does_nothing_and_says_why() {
        let program = std::env::temp_dir().join(format!("shinatama-muro-program-{}", uuid::Uuid::new_v4()));
        assert_eq!(locate(Some(OsStr::new("OFF")), Some(&program)), Located::Off);
        assert_eq!(locate(Some(OsStr::new(" off ")), Some(&program)), Located::Off);
        assert_eq!(locate(None, Some(&program)), Located::Missing(program.join("muro")));
        let named = program.join("elsewhere");
        assert_eq!(locate(Some(named.as_os_str()), Some(&program)), Located::Missing(named.clone()));
        std::fs::create_dir_all(named.join("tools")).unwrap();
        for file in ["Muro.exe", "tools/muroctl.exe", "tools/mukade.exe"] {
            std::fs::write(named.join(file), b"").unwrap();
        }
        assert_eq!(
            locate(Some(named.as_os_str()), Some(&program)),
            Located::Found(Install { muro: named.join("Muro.exe"), muroctl: named.join("tools/muroctl.exe"), mukade: named.join("tools/mukade.exe") })
        );
        for (state, message) in [("off", None), ("missing", Some("C:\\x\\muro".to_owned()))] {
            let muro = Muro::new(None, program.clone(), Status::new(state, message));
            muro.start();
            assert_eq!(muro.status().state, state);
            assert!(muro.sync_now().is_err());
            assert!(muro.open().is_err());
        }
        assert_eq!(unavailable(&Status::new("off", None)), "Muro is switched off (SHINATAMA_MURO=off).");
        assert_eq!(unavailable(&Status::new("missing", None)), "Muro is not in this build.");
        let _ = std::fs::remove_dir_all(program);
    }

    #[test]
    fn a_long_path_is_recorded_in_its_plain_spelling() {
        assert_eq!(plain(Path::new(r"\\?\D:\Data\agents")), r"D:\Data\agents");
        assert_eq!(plain(Path::new(r"\\?\UNC\server\share")), r"\\?\UNC\server\share");
        assert_eq!(plain(Path::new(r"C:\x")), r"C:\x");
    }
}
