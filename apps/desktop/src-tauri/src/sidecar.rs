//! Private proxy setup and one retained owner. UI commands supply no program,
//! environment, port, configuration payload or resource budget.
#![cfg(windows)]
use crate::{
    disk_space::{self, DiskLevel, DiskWatch},
    storage::Services,
    terminal_runtime::Runtime,
};
use serde::Serialize;
use shinatama_engine::{
    sidecar_admin::{
        AdminTarget, CapabilityRead, ResolvedBaseline, resolved_baseline, session_capabilities,
    },
    sidecar_config::{intended_baseline, prepare_private_sidecar},
    sidecar_runtime::{
        NodePin, RuntimeSummary, prepare_runtime, verify_bundled_node, verify_prepared_bundle,
    },
    sidecar_supervisor::{
        Command, Phase, ProxyExit, Reason, SidecarSupervisor, Snapshot, SupervisorConfig,
    },
};
use shinatama_state::{
    database::DatabaseAccess,
    location::StorageLocation,
    store_roots::{AgentStoreKey, PrivateStorePlan},
};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    net::TcpListener,
    num::NonZeroU16,
    os::windows::fs::OpenOptionsExt,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub phase: &'static str,
    pub healthy: bool,
    pub message: Option<&'static str>,
    pub observation: Option<Snapshot>,
    pub prepared_identity: Option<RuntimeSummary>,
    /// Read once from the running process by a worker; `status` never does I/O.
    pub resolved: Option<ResolvedBaseline>,
    /// None until the configuration was read; true when it differs from what
    /// Shinatama writes.
    pub config_drift: Option<bool>,
    /// The values read from the running process that differ from the baseline.
    pub config_differences: Vec<ConfigDifference>,
    /// Requests the proxy is still answering; None while it is not running.
    pub in_flight: Option<u64>,
    /// Agent terminals alive right now. `status` cannot know it: the command
    /// layer fills it from the agents controller.
    pub agent_sessions: usize,
    /// False when the count could not be read: the panel then asks before stopping.
    pub agent_sessions_known: bool,
    /// When a requested drain stops waiting for in-flight requests.
    pub drain_deadline_unix_ms: Option<i64>,
    /// How the proxy's process last ended by itself; kept after it recovers.
    pub last_exit: Option<LastExit>,
    /// When the supervisor last reported; None before it ever did.
    pub observed_at_unix_ms: Option<i64>,
    /// No report for more than 15 s while it should be working: only a restart helps then.
    pub stalled: bool,
    /// The phase it last reported, sent only while it is stalled.
    pub last_phase: Option<&'static str>,
    /// The internal error that stopped the supervisor's loop.
    pub fault: Option<String>,
    /// Free space of the data disk; None until the first sample.
    pub disk: Option<DiskStatus>,
    /// The proxy archive after the last cleanup run; None before one ran.
    pub archive: Option<ArchiveStatus>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskStatus {
    pub root: String,
    pub free_bytes: u64,
    /// `ok`, `low` or `full`.
    pub level: &'static str,
}
/// A supervisor silent this long while it should be working is stalled.
pub(crate) const STALL_AFTER_MS: i64 = 15_000;
/// What a driver says when the engine's launch gate found no proxy backing the launch; the agent
/// service replaces it with [`Controller::refusal`], which says why.
pub const NOT_BACKING: &str = "The model proxy stopped backing new sessions.";
/// What [`Controller::qualified_owner`] says while the proxy cannot back a launch; replaced the same way.
pub const NOT_READY: &str = "Start the Model proxy and wait until it is ready.";
pub(crate) fn stalled(observed_at_unix_ms: Option<i64>, now_unix_ms: i64, phase: Phase) -> bool {
    !matches!(phase, Phase::Stopped | Phase::Failed | Phase::Retired)
        && observed_at_unix_ms
            .and_then(|at| now_unix_ms.checked_sub(at))
            .is_some_and(|age| age > STALL_AFTER_MS)
}
fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Stopped => "stopped",
        Phase::Starting => "starting",
        Phase::Ready => "ready",
        Phase::Busy => "busy",
        Phase::Draining => "draining",
        Phase::Maintaining => "maintaining",
        Phase::Degraded => "degraded",
        Phase::Recovering => "recovering",
        Phase::Failed => "failed",
        Phase::Retired => "retired",
    }
}
/// The panel's sentence for a supervisor reason. None claims that saved data is preserved: that
/// is not something the supervisor can know (the proxy's own saves may be failing).
fn reason_message(phase: Phase, reason: Reason) -> Option<&'static str> {
    match reason {
        Reason::AdmissionDeferred => Some("Waiting for resources. The proxy will retry automatically; existing sessions keep their limits."),
        Reason::StartupRefused if phase == Phase::Recovering => Some("The proxy could not start. Recovery will retry automatically."),
        Reason::StartupRefused => Some("The proxy launch configuration is unavailable; retry Start after correcting the installation."),
        Reason::ProcessExited => Some("The proxy exited. Recovering automatically on the same address."),
        Reason::CleanupUncertain if phase == Phase::Recovering => Some("Waiting to confirm cleanup before restarting the proxy. Its reservation remains held."),
        Reason::CleanupUncertain => Some("Cleanup is not confirmed. Its reservation remains held; retry cleanup before another start."),
        Reason::ResourceLimit => Some("The proxy reached its resource limit. Recovery will retry with fresh resource checks."),
        Reason::SupervisorFault => Some("The proxy supervisor stopped on an internal error. Start or Retry recovery starts it again."),
        _ => None,
    }
}
fn state_words(phase: &str, healthy: bool) -> &'static str {
    match phase {
        "ready" | "busy" if !healthy => "not answering its health check",
        "ready" => "ready",
        "busy" => "busy",
        "stopped" => "stopped",
        "preparing" => "being prepared",
        "starting" => "starting",
        "draining" => "stopping",
        "maintaining" => "cleaning its archive",
        "degraded" => "temporarily unavailable",
        "recovering" => "recovering",
        "failed" => "stopped after a failure",
        "retired" => "waiting for recovery",
        "unavailable" => "not reporting",
        _ => "unavailable",
    }
}
/// What a refused open says about the proxy, gathered when it is refused. Times are already
/// written as the user reads them.
pub(crate) struct RefusalFacts {
    pub phase: &'static str,
    pub healthy: bool,
    /// Since when it has not reported, and the phase it last reported.
    pub stalled: Option<(String, &'static str)>,
    pub message: Option<&'static str>,
    /// When the process last ended, its code, and what it said.
    pub exit: Option<(String, u32, Option<String>)>,
    pub disk_line: Option<String>,
}
/// One sentence for every harness: the proxy's state, since when, its last exit, a low disk.
pub(crate) fn refusal_text(facts: &RefusalFacts) -> String {
    let mut text = match &facts.stalled {
        Some((since, last)) => format!(
            "Not started: the model proxy has not reported since {since} (last state: {last})."
        ),
        // The gate refused, and by now the proxy answers: say so instead of "is ready".
        None if facts.healthy && matches!(facts.phase, "ready" | "busy") => {
            "Not started: the model proxy was not ready when this open was checked; it is ready now. Try Open again."
                .to_owned()
        }
        None => format!(
            "Not started: the model proxy is {}.",
            state_words(facts.phase, facts.healthy)
        ),
    };
    if let Some(message) = facts.message {
        // A stalled supervisor's message is what it said last, not what is happening now.
        text.push_str(if facts.stalled.is_some() { " It last said: " } else { " " });
        text.push_str(message);
    }
    if let Some((at, code, detail)) = &facts.exit {
        match detail.as_deref().map(|said| said.trim_end_matches('.')) {
            Some(said) if !said.is_empty() => {
                text.push_str(&format!(" Its process ended at {at} with code {code}: {said}."))
            }
            _ => text.push_str(&format!(" Its process ended at {at} with code {code}.")),
        }
    }
    if let Some(line) = &facts.disk_line {
        text.push(' ');
        text.push_str(line);
    }
    text.push_str(if facts.stalled.is_some() {
        " Restart Shinatama from the Model proxy panel."
    } else {
        " Its state is in the Model proxy panel."
    });
    text
}
/// A Unix time as the wall clock the user reads (`20:52:40`).
fn clock(unix_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(unix_ms).map_or_else(
        || "an unknown time".to_owned(),
        |at| at.with_timezone(&chrono::Local).format("%H:%M:%S").to_string(),
    )
}
/// [`ProxyExit`] as the panel reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastExit {
    pub code: u32,
    pub uptime_ms: u64,
    pub at_unix_ms: Option<i64>,
    pub detail: Option<String>,
    pub count: u32,
}
impl From<&ProxyExit> for LastExit {
    fn from(exit: &ProxyExit) -> Self {
        Self {
            code: exit.code,
            uptime_ms: exit.uptime_ms,
            at_unix_ms: exit.at_unix_ms,
            detail: exit.detail.clone(),
            count: exit.count,
        }
    }
}
/// One line of Shinatama's log for each exit of the proxy's process: its diagnostics are
/// read nowhere else (ADR-0030). A proxy that keeps ending the same way is written five
/// times, then once every twenty exits, so a loop cannot push the rest out of the log.
#[derive(Default)]
pub(crate) struct ExitLog {
    last: Option<(u32, Option<String>)>,
    streak: u32,
}
impl ExitLog {
    pub(crate) fn line(&mut self, exit: &ProxyExit) -> Option<String> {
        let key = (exit.code, exit.detail.clone());
        if self.last.as_ref() == Some(&key) {
            self.streak = self.streak.saturating_add(1);
        } else {
            self.last = Some(key);
            self.streak = 1;
        }
        if self.streak > 5 && self.streak % 20 != 0 {
            return None;
        }
        let said = match &exit.detail {
            Some(detail) => format!("It said: {detail}"),
            None => "It reported no error.".to_owned(),
        };
        let again = if self.streak > 1 {
            format!(" The same exit {} times in a row.", self.streak)
        } else {
            String::new()
        };
        Some(format!(
            "The process ended with code {} after {} ms (exit {} since launch). {said}{again}",
            exit.code, exit.uptime_ms, exit.count
        ))
    }
}
/// One setting whose value in the running process is not the baseline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConfigDifference {
    pub setting: &'static str,
    pub read: String,
    pub baseline: String,
}
/// What a user-requested stop does while agent sessions may be live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopDecision {
    Refuse,
    Drain,
}
/// A stop that would cut live agents off needs an explicit `force`.
/// An unknown count is not "no sessions": it asks like a live one does.
fn decide_stop(agent_sessions: Option<usize>, force: bool) -> StopDecision {
    if agent_sessions.is_none_or(|count| count > 0) && !force {
        StopDecision::Refuse
    } else {
        StopDecision::Drain
    }
}
/// Only a healthy proxy backs a new launch: ready, or busy with requests in
/// flight (another agent is streaming). A draining one never does.
/// Starts the proxy when Shinatama starts (it used to wait for the first Open, which then spent
/// 15-30 s on it) and logs when it is ready. Its start has the same admission as an Open's; one
/// refused for resources retries by itself. A user's Stop still stops it.
pub(crate) fn start_at_launch(controller: &Arc<Controller>) {
    let began = std::time::Instant::now();
    if let Err(message) = controller.start() {
        crate::app_log::write("proxy", &format!("Not started at launch: {message}"));
        return;
    }
    crate::app_log::write("proxy", "Starting at launch.");
    let owner = Arc::clone(controller);
    let _ = std::thread::Builder::new()
        .name("proxy-at-launch".into())
        .spawn(move || {
            let deadline = began + Duration::from_secs(180);
            loop {
                let status = owner.status();
                let elapsed = began.elapsed().as_millis();
                if launch_allowed(status.phase, status.healthy) {
                    crate::app_log::write("proxy", &format!("Ready {elapsed} ms after launch."));
                    return;
                }
                if matches!(status.phase, "failed" | "retired" | "stopped" | "draining") && elapsed > 10_000 {
                    crate::app_log::write("proxy", &format!("Not ready {elapsed} ms after launch: {} ({}).",
                        status.phase, status.message.unwrap_or("no message")));
                    return;
                }
                if std::time::Instant::now() >= deadline {
                    crate::app_log::write("proxy", &format!("Still {} 3 minutes after launch.", status.phase));
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
}
/// How long no agent session is open before the proxy's archive is cleaned
/// (docs/superpowers/specs/2026-11-20-proxy-archive-cleanup-design.md).
pub(crate) const IDLE_BEFORE_CLEANUP: Duration = Duration::from_secs(600);
/// Whether an idle cleanup is due: no agent session is open (known, not guessed), and either a
/// forget asked for it or none has been open for [`IDLE_BEFORE_CLEANUP`].
pub(crate) fn idle_run_due(
    live_agent_sessions: Option<usize>,
    idle_since: Option<std::time::Instant>,
    now: std::time::Instant,
    due_now: bool,
) -> bool {
    live_agent_sessions == Some(0)
        && (due_now
            || idle_since
                .is_some_and(|since| now.saturating_duration_since(since) >= IDLE_BEFORE_CLEANUP))
}
/// Opens in progress: an open that passed the proxy check has no live terminal yet, and the
/// idle cleanup must not stop the proxy under it.
#[derive(Clone, Default)]
pub(crate) struct Opening(Arc<std::sync::atomic::AtomicUsize>);
pub(crate) struct OpeningGuard(Arc<std::sync::atomic::AtomicUsize>);
impl Opening {
    pub(crate) fn enter(&self) -> OpeningGuard {
        self.0.fetch_add(1, Ordering::AcqRel);
        OpeningGuard(Arc::clone(&self.0))
    }
    pub(crate) fn count(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}
impl Drop for OpeningGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
/// What the proxy owner asks before it stops an idle proxy for a cleanup, and between the run's
/// commands: the live agent sessions and the opens in progress. An unknown count is activity.
#[derive(Clone, Default)]
pub(crate) struct SessionActivity {
    opening: Opening,
    live: Arc<OnceLock<Box<dyn Fn() -> Option<usize> + Send + Sync>>>,
}
impl SessionActivity {
    pub(crate) fn set_live(&self, live: Box<dyn Fn() -> Option<usize> + Send + Sync>) {
        let _ = self.live.set(live);
    }
    pub(crate) fn busy(&self) -> Option<usize> {
        todo!("RED: SessionActivity::busy")
    }
    pub(crate) fn active(&self) -> bool {
        todo!("RED: SessionActivity::active")
    }
}
/// Whether the idle cleanup is asked for (again): an owner that skipped a run (requests in
/// flight, a drain) is asked once more after another idle period, a forget asks at once.
pub(crate) fn send_due(
    last_sent: Option<std::time::Instant>,
    now: std::time::Instant,
    due_now: bool,
) -> bool {
    todo!("RED: send_due")
}
/// The agent sessions the idle rule counts: the live ones and the opens in progress.
pub(crate) fn busy_sessions(live_agent_sessions: Option<usize>, opening: usize) -> Option<usize> {
    live_agent_sessions.map(|live| live + opening)
}
/// The line `shinatama.log` gets for a run of the archive cleanup.
pub(crate) fn report_line(report: &shinatama_engine::sidecar_archive::Report) -> String {
    use shinatama_engine::sidecar_archive::Stop;
    if report.stop == Stop::Unreadable {
        return "nothing removed: the archive or Shinatama's threads could not be read.".into();
    }
    let mut line = format!(
        "removed {} sessions ({} MB) in {} s",
        report.removed,
        report.released_bytes / 1_000_000,
        report.elapsed.as_secs()
    );
    if report.failed > 0 {
        line.push_str(&format!("; {} could not be removed", report.failed));
    }
    line.push_str(&format!("; {} left; ", report.left));
    match report.file_bytes_after {
        Some(bytes) => line.push_str(&format!("archive now {:.1} MB", bytes as f64 / 1e6)),
        None => line.push_str("archive size unknown"),
    }
    line.push_str(match report.stop {
        Stop::Budget => "; the rest goes in the next run.",
        Stop::Cancelled => "; stopped for an open.",
        Stop::Done | Stop::Unreadable => ".",
    });
    line
}
/// What the panel shows of the proxy's archive: its size after the last cleanup run.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStatus {
    pub file_bytes: Option<u64>,
    pub last_run_unix_ms: Option<i64>,
    pub last_removed: usize,
    pub last_left: usize,
}
/// The last run, written by the owner's provider and read by `Controller::status`.
pub(crate) type ArchiveRecord = Arc<Mutex<Option<ArchiveStatus>>>;
pub(crate) fn archive_status(report: &shinatama_engine::sidecar_archive::Report) -> ArchiveStatus {
    ArchiveStatus {
        file_bytes: report.file_bytes_after,
        last_run_unix_ms: now_ms(),
        last_removed: report.removed,
        last_left: report.left,
    }
}
/// Every 30 s: when no agent session has been open for ten minutes, or a forget asked for it,
/// and the proxy is ready, its owner cleans the archive and starts the proxy again. Once per
/// idle period; an agent session that opens starts a new one.
pub(crate) fn watch_idle(
    controller: &Arc<Controller>,
    live_agent_sessions: impl Fn() -> Option<usize> + Send + 'static,
) {
    let owner = Arc::clone(controller);
    let _ = std::thread::Builder::new()
        .name("proxy-archive-idle".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(30));
                let live = busy_sessions(live_agent_sessions(), owner.opening.count());
                let now = std::time::Instant::now();
                let due = {
                    let Ok(mut idle) = owner.idle.lock() else {
                        return;
                    };
                    if live != Some(0) {
                        *idle = IdleState::default();
                        false
                    } else {
                        let since = *idle.since.get_or_insert(now);
                        !idle.sent && idle_run_due(live, Some(since), now, idle.due_now)
                    }
                };
                if !due {
                    continue;
                }
                let status = owner.status();
                if status.phase != "ready" || !status.healthy {
                    continue;
                }
                if owner.maintain(None, true).is_ok()
                    && let Ok(mut idle) = owner.idle.lock()
                {
                    idle.sent = true;
                    idle.due_now = false;
                }
            }
        });
}
/// When the idle cleanup is due: see [`watch_idle`].
#[derive(Default)]
struct IdleState {
    since: Option<std::time::Instant>,
    due_now: bool,
    sent: bool,
}
pub(crate) fn launch_allowed(phase: &str, healthy: bool) -> bool {
    matches!(phase, "ready" | "busy") && healthy
}
pub const STOPPING: &str = "The model proxy is stopping. Start it again before opening an agent.";
pub enum StopOutcome {
    Draining(Status),
    Refused { status: Status },
}
/// How long a worker waits for one admin read, and how often it retries an
/// unknown one for the same process.
const RESOLVED_READ_TIMEOUT: Duration = Duration::from_secs(1);
const RESOLVED_READ_ATTEMPTS: u8 = 3;
/// `None` until the running process's configuration is known.
fn drift(resolved: Option<&ResolvedBaseline>) -> Option<bool> {
    resolved.map(|resolved| *resolved != intended_baseline())
}
/// Longest setting value shown; the status reply is read back with the same bound.
const DIFFERENCE_TEXT_LIMIT: usize = 80;
/// A value read from the process made displayable: never empty, never longer
/// than the panel accepts, cut on a character boundary.
fn bounded_value(value: String) -> String {
    if value.is_empty() {
        return "(empty)".to_owned();
    }
    if value.chars().count() <= DIFFERENCE_TEXT_LIMIT {
        return value;
    }
    let mut cut: String = value.chars().take(DIFFERENCE_TEXT_LIMIT - 1).collect();
    cut.push('\u{2026}');
    cut
}
/// The settings of `resolved` that differ from what Shinatama writes; a value
/// the process did not report is listed as such rather than as the baseline.
fn config_differences(resolved: Option<&ResolvedBaseline>) -> Vec<ConfigDifference> {
    let Some(resolved) = resolved else {
        return Vec::new();
    };
    let baseline = intended_baseline();
    let mut out = Vec::new();
    fn check<T: PartialEq + std::fmt::Display>(
        out: &mut Vec<ConfigDifference>,
        setting: &'static str,
        read: &Option<T>,
        baseline: &Option<T>,
    ) {
        if read != baseline {
            let show = |value: &Option<T>| {
                value
                    .as_ref()
                    .map_or_else(|| "not reported".to_owned(), |value| bounded_value(value.to_string()))
            };
            out.push(ConfigDifference {
                setting,
                read: show(read),
                baseline: show(baseline),
            });
        }
    }
    check(&mut out, "Prompt rewrite", &resolved.prompt_rewrite_mode, &baseline.prompt_rewrite_mode);
    check(&mut out, "Proxy messages in the conversation", &resolved.visibility_markers, &baseline.visibility_markers);
    check(&mut out, "Tiers", &resolved.tiers, &baseline.tiers);
    check(&mut out, "Absorb", &resolved.absorb_enabled, &baseline.absorb_enabled);
    check(&mut out, "Absorb minimum tool tokens", &resolved.absorb_min_tool_tokens, &baseline.absorb_min_tool_tokens);
    check(&mut out, "Large-result storage", &resolved.ccr_enabled, &baseline.ccr_enabled);
    check(&mut out, "Large-result storage minimum tool tokens", &resolved.ccr_min_tool_tokens, &baseline.ccr_min_tool_tokens);
    check(&mut out, "Adaptive pressure", &resolved.adaptive_enabled, &baseline.adaptive_enabled);
    check(&mut out, "Adaptive pressure maximum tokens", &resolved.adaptive_max_tokens, &baseline.adaptive_max_tokens);
    check(&mut out, "Output steering", &resolved.steering_enabled, &baseline.steering_enabled);
    check(&mut out, "Verbosity level", &resolved.verbosity_level, &baseline.verbosity_level);
    check(&mut out, "Effort routing", &resolved.effort_routing, &baseline.effort_routing);
    out
}
pub struct Controller {
    services: Arc<Services>,
    runtime: Arc<Runtime>,
    preparing: AtomicBool,
    stop_requested: AtomicBool,
    supervisor: OnceLock<SidecarSupervisor>,
    identity: OnceLock<RuntimeSummary>,
    connection: OnceLock<(NonZeroU16, StorageLocation)>,
    failure: Mutex<Option<&'static str>>,
    /// The read configuration with the pid of the process it came from; it is
    /// shown only while the supervisor still reports that pid.
    resolved: Mutex<Option<(u32, Option<ResolvedBaseline>)>>,
    watching: AtomicBool,
    /// True while a stall has been logged and the supervisor has not reported again.
    stall_logged: AtomicBool,
    disk: OnceLock<Arc<DiskWatch>>,
    /// The last run of the archive cleanup.
    archive: ArchiveRecord,
    idle: Mutex<IdleState>,
    opening: Opening,
}
impl Controller {
    pub fn new(services: Arc<Services>, runtime: Arc<Runtime>) -> Self {
        Self {
            services,
            runtime,
            preparing: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            supervisor: OnceLock::new(),
            identity: OnceLock::new(),
            connection: OnceLock::new(),
            failure: Mutex::new(None),
            resolved: Mutex::new(None),
            watching: AtomicBool::new(false),
            stall_logged: AtomicBool::new(false),
            disk: OnceLock::new(),
            archive: ArchiveRecord::default(),
            idle: Mutex::new(IdleState::default()),
            opening: Opening::default(),
        }
    }
    /// Held by an open from its start to its end: the idle cleanup counts it as a live session.
    pub(crate) fn opening(&self) -> OpeningGuard {
        self.opening.enter()
    }
    /// Asks the owner to clean the proxy's archive (`Command::Maintain`).
    pub fn maintain(&self, budget: Option<Duration>, restart: bool) -> Result<(), &'static str> {
        self.supervisor
            .get()
            .ok_or("The Model proxy owner is unavailable.")?
            .request(Command::Maintain { budget, restart })
            .map_err(|_| "The proxy owner is busy; its archive is cleaned at the next run.")
    }
    /// After a forget: the idle check cleans the archive at once when no agent session is open.
    pub fn maintain_soon(&self) {
        if let Ok(mut idle) = self.idle.lock() {
            idle.due_now = true;
            idle.sent = false;
        }
    }
    /// The close worker's run, once the proxy has stopped: returns when the owner has run it
    /// and reports a stopped state again, or after `budget` and 5 s more.
    pub fn maintain_and_wait(&self, budget: Duration) {
        if self.maintain(Some(budget), false).is_err() {
            return;
        }
        let began = std::time::Instant::now();
        let deadline = began + budget + Duration::from_secs(5);
        let mut seen = false;
        while std::time::Instant::now() < deadline {
            match self
                .supervisor
                .get()
                .and_then(SidecarSupervisor::status)
                .map(|snapshot| snapshot.phase)
            {
                Some(Phase::Maintaining) => seen = true,
                Some(Phase::Stopped | Phase::Retired | Phase::Failed)
                    if seen || began.elapsed() > Duration::from_secs(1) =>
                {
                    // Seen running, or never seen in a second: a run with nothing to read
                    // passes between two polls.
                    return;
                }
                None => return,
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    /// The data disk's free-space reading, shown in the status and in refusals.
    pub fn watch_disk(&self, watch: Arc<DiskWatch>) {
        let _ = self.disk.set(watch);
    }
    fn disk_status(&self) -> Option<DiskStatus> {
        let watch = self.disk.get()?;
        let free_bytes = watch.free_bytes()?;
        Some(DiskStatus {
            root: watch.root().to_owned(),
            free_bytes,
            level: disk_space::level(free_bytes).name(),
        })
    }
    /// The data disk's line while it is low or full.
    fn disk_line(&self) -> Option<String> {
        let watch = self.disk.get()?;
        disk_space::line(watch.root(), watch.free_bytes()?)
    }
    /// The data disk's line only while it is full: added to any refused open, since a full disk
    /// is a cause whatever the step that failed.
    pub fn disk_full_line(&self) -> Option<String> {
        let watch = self.disk.get()?;
        let free = watch.free_bytes()?;
        (disk_space::level(free) == DiskLevel::Full)
            .then(|| disk_space::line(watch.root(), free))
            .flatten()
    }
    /// Why the proxy does not back an open, in one sentence the same for every harness.
    pub fn refusal(&self) -> String {
        let status = self.status();
        refusal_text(&RefusalFacts {
            phase: status.phase,
            healthy: status.healthy,
            stalled: status.stalled.then(|| {
                (
                    status.observed_at_unix_ms.map_or_else(|| "an unknown time".to_owned(), clock),
                    status.last_phase.unwrap_or("unavailable"),
                )
            }),
            message: status.message,
            exit: status.last_exit.as_ref().map(|exit| {
                (
                    exit.at_unix_ms.map_or_else(|| "an unknown time".to_owned(), clock),
                    exit.code,
                    exit.detail.clone(),
                )
            }),
            disk_line: self.disk_line(),
        })
    }
    /// One log line when the supervisor stops reporting and one when it reports again, written
    /// on a thread of its own: `status` does no I/O on its caller's thread.
    fn note_stall(&self, stalled: bool, fresh: bool, snapshot: Option<&Snapshot>) {
        let line = if stalled && !self.stall_logged.swap(true, Ordering::AcqRel) {
            snapshot.map(|snapshot| {
                format!(
                    "The supervisor has not reported for 15 s (last report at {}: {}, {:?}).",
                    snapshot.observed_at_unix_ms.map_or_else(|| "an unknown time".to_owned(), clock),
                    phase_name(snapshot.phase),
                    snapshot.reason
                )
            })
        } else if !stalled && fresh && self.stall_logged.swap(false, Ordering::AcqRel) {
            snapshot.map(|snapshot| format!("The supervisor reports again ({}).", phase_name(snapshot.phase)))
        } else {
            None
        };
        if let Some(line) = line {
            let _ = std::thread::Builder::new()
                .name("proxy-stall-log".into())
                .spawn(move || crate::app_log::write("proxy", &line));
        }
    }
    pub fn status(&self) -> Status {
        let observation = self.supervisor.get().and_then(SidecarSupervisor::status);
        let fresh = observation.as_ref().is_some_and(|snapshot| {
            snapshot
                .observed_at_unix_ms
                .zip(now_ms())
                .is_some_and(|(at, now)| now.checked_sub(at).is_some_and(|age| age <= 3000))
        });
        let phase = if self.preparing.load(Ordering::Acquire) {
            "preparing"
        } else if let Some(snapshot) = &observation {
            if !fresh
                && !matches!(
                    snapshot.phase,
                    Phase::Stopped | Phase::Failed | Phase::Retired
                )
            {
                "unavailable"
            } else {
                phase_name(snapshot.phase)
            }
        } else if self
            .failure
            .try_lock()
            .ok()
            .is_some_and(|value| value.is_some())
        {
            "failed"
        } else {
            "stopped"
        };
        let message = self.failure.try_lock().ok().and_then(|value| *value).or_else(|| {
            observation
                .as_ref()
                .and_then(|snapshot| reason_message(snapshot.phase, snapshot.reason))
        });
        // Silent for 15 s while it should be working: its buttons no longer act, only a restart.
        let is_stalled = !self.preparing.load(Ordering::Acquire)
            && observation.as_ref().zip(now_ms()).is_some_and(|(snapshot, now)| {
                stalled(snapshot.observed_at_unix_ms, now, snapshot.phase)
            });
        self.note_stall(is_stalled, fresh, observation.as_ref());
        let resolved = observation
            .as_ref()
            .and_then(|snapshot| snapshot.pid)
            .and_then(|pid| {
                self.resolved.try_lock().ok().and_then(|value| {
                    value
                        .as_ref()
                        .filter(|(read_for, _)| *read_for == pid)
                        .and_then(|(_, resolved)| resolved.clone())
                })
            });
        Status {
            in_flight: observation.as_ref().and_then(|snapshot| snapshot.in_flight),
            agent_sessions: 0,
            agent_sessions_known: true,
            drain_deadline_unix_ms: observation
                .as_ref()
                .and_then(|snapshot| snapshot.drain_deadline_unix_ms),
            phase,
            healthy: fresh
                && observation
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.healthy),
            message,
            last_exit: observation
                .as_ref()
                .and_then(|snapshot| snapshot.last_exit.as_ref())
                .map(LastExit::from),
            observed_at_unix_ms: observation
                .as_ref()
                .and_then(|snapshot| snapshot.observed_at_unix_ms),
            stalled: is_stalled,
            last_phase: observation
                .as_ref()
                .filter(|_| is_stalled)
                .map(|snapshot| phase_name(snapshot.phase)),
            fault: observation.as_ref().and_then(|snapshot| snapshot.fault.clone()),
            disk: self.disk_status(),
            archive: self.archive.lock().ok().and_then(|value| value.clone()),
            observation,
            prepared_identity: self.identity.get().cloned(),
            config_drift: drift(resolved.as_ref()),
            config_differences: config_differences(resolved.as_ref()),
            resolved,
        }
    }
    pub fn start(self: &Arc<Self>) -> Result<Status, &'static str> {
        if let Some(supervisor) = self.supervisor.get() {
            self.stop_requested.store(false, Ordering::Release);
            let command = if supervisor
                .status()
                .is_some_and(|snapshot| snapshot.phase == Phase::Failed)
            {
                Command::Retry
            } else {
                Command::Start
            };
            supervisor
                .request(command)
                .map_err(|_| "The proxy owner is busy; retry explicitly.")?;
            self.watch_resolved();
            return Ok(self.status());
        }
        if self
            .preparing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(self.status());
        }
        self.stop_requested.store(false, Ordering::Release);
        let owner = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| owner.prepare()));
            let failure = match result {
                Ok(Ok(())) => None,
                Ok(Err(message)) => Some(message),
                Err(_) => Some("Proxy preparation failed. Start retries it."),
            };
            let prepared = failure.is_none();
            if let Ok(mut value) = owner.failure.lock() {
                *value = failure;
            }
            owner.preparing.store(false, Ordering::Release);
            if prepared {
                owner.watch_resolved();
            }
        });
        Ok(self.status())
    }
    pub fn stop(&self) -> Result<Status, &'static str> {
        self.stop_requested.store(true, Ordering::Release);
        if let Some(supervisor) = self.supervisor.get() {
            supervisor
                .request(Command::Stop)
                .map_err(|_| "Proxy stop could not be delivered.")?;
        }
        Ok(self.status())
    }
    /// The user's Stop: refuses while agent sessions are live unless forced;
    /// otherwise lets in-flight requests finish before the process stops.
    pub fn stop_requested_by_user(
        &self,
        agent_sessions: Option<usize>,
        force: bool,
    ) -> Result<StopOutcome, &'static str> {
        let with_sessions = |mut status: Status| {
            status.agent_sessions = agent_sessions.unwrap_or(0);
            status.agent_sessions_known = agent_sessions.is_some();
            status
        };
        match decide_stop(agent_sessions, force) {
            StopDecision::Refuse => Ok(StopOutcome::Refused {
                status: with_sessions(self.status()),
            }),
            StopDecision::Drain => {
                self.stop_requested.store(true, Ordering::Release);
                if let Some(supervisor) = self.supervisor.get() {
                    supervisor
                        .request(Command::Drain)
                        .map_err(|_| "Proxy stop could not be delivered.")?;
                }
                Ok(StopOutcome::Draining(with_sessions(self.status())))
            }
        }
    }
    pub fn retry(self: &Arc<Self>) -> Result<Status, &'static str> {
        self.stop_requested.store(false, Ordering::Release);
        self.supervisor
            .get()
            .ok_or("Prepare the proxy before retrying recovery.")?
            .request(Command::Retry)
            .map_err(|_| "The proxy owner is busy; no replacement was created.")?;
        self.watch_resolved();
        Ok(self.status())
    }
    /// Starts the one blocking worker that reads the running process's
    /// configuration once per process, never on the UI thread.
    fn watch_resolved(self: &Arc<Self>) {
        if self.watching.swap(true, Ordering::AcqRel) {
            return;
        }
        let owner = Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                owner.read_resolved_until_idle()
            }));
            owner.watching.store(false, Ordering::Release);
        });
    }
    fn read_resolved_until_idle(&self) {
        let began = std::time::Instant::now();
        let mut seen_active = false;
        let mut current: Option<(u32, u8)> = None;
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let Some(snapshot) = self.supervisor.get().and_then(SidecarSupervisor::status) else {
                break;
            };
            match snapshot.phase {
                Phase::Failed | Phase::Retired => break,
                Phase::Stopped => {
                    // The owner reports Stopped until its worker takes Start.
                    if seen_active || began.elapsed() > Duration::from_secs(10) {
                        break;
                    }
                    continue;
                }
                _ => seen_active = true,
            }
            if self.stop_requested.load(Ordering::Acquire) {
                break;
            }
            let (Phase::Ready | Phase::Busy, true, Some(pid)) =
                (snapshot.phase, snapshot.healthy, snapshot.pid)
            else {
                continue;
            };
            let attempts = match current {
                Some((known, attempts)) if known == pid => attempts,
                _ => 0,
            };
            if attempts >= RESOLVED_READ_ATTEMPTS
                || self
                    .resolved
                    .lock()
                    .is_ok_and(|value| value.as_ref().is_some_and(|(read_for, r)| *read_for == pid && r.is_some()))
            {
                current = Some((pid, RESOLVED_READ_ATTEMPTS));
                continue;
            }
            let Some((port, _)) = self.connection.get() else {
                continue;
            };
            let read = resolved_baseline(&AdminTarget {
                port: *port,
                pid,
                timeout: RESOLVED_READ_TIMEOUT,
            });
            current = Some((pid, attempts + 1));
            if let Ok(mut value) = self.resolved.lock() {
                *value = Some((pid, read));
            }
        }
        if let Ok(mut value) = self.resolved.lock() {
            *value = None;
        }
    }
    /// One session's capabilities from the running process, never on the UI thread.
    /// Unavailable while no owned process is known; the engine proves the process
    /// identity (health `pid`) before the capabilities path is asked.
    pub fn session_capabilities(&self, native_id: &str) -> CapabilityRead {
        let Some(pid) = self
            .supervisor
            .get()
            .and_then(SidecarSupervisor::status)
            .and_then(|snapshot| snapshot.pid)
        else {
            return CapabilityRead::Unavailable;
        };
        let Some((port, _)) = self.connection.get() else {
            return CapabilityRead::Unavailable;
        };
        session_capabilities(
            &AdminTarget {
                port: *port,
                pid,
                timeout: RESOLVED_READ_TIMEOUT,
            },
            native_id,
        )
    }
    pub fn is_active(&self) -> bool {
        self.preparing.load(Ordering::Acquire)
            || self.supervisor.get().is_some_and(|owner| {
                owner.status().is_none_or(|snapshot| {
                    !matches!(snapshot.phase, Phase::Stopped | Phase::Failed)
                })
            })
    }
    /// Trusted launch workers only: no frontend port or certificate selector.
    /// The engine repeats the live owner check before issuing a launch permit.
    pub fn qualified_owner(
        &self,
    ) -> Result<(&SidecarSupervisor, NonZeroU16, PathBuf), &'static str> {
        let status = self.status();
        if status.phase == "draining" {
            return Err(STOPPING);
        }
        if !launch_allowed(status.phase, status.healthy) {
            return Err(NOT_READY);
        }
        let (port, root) = self
            .connection
            .get()
            .ok_or("The Model proxy connection is unavailable.")?;
        let ca = root
            .resolve_related_directory(&root.directory().join("data/ca"))
            .map_err(|_| "The Model proxy certificate directory is unavailable.")?;
        ca.verify_private_directory()
            .map_err(|_| "The Model proxy certificate directory could not be verified.")?;
        let path = ca.directory().join("combined-ca.pem");
        // This is the public CA bundle created by our contained proxy, never a
        // system-store installation or a read of the private CA key.
        ca.secure_private_file(&path)
            .map_err(|_| "The Model proxy public certificate bundle is unavailable.")?;
        Ok((
            self.supervisor
                .get()
                .ok_or("The Model proxy owner is unavailable.")?,
            *port,
            path,
        ))
    }
    /// Called only on the existing bounded close worker. A timeout preserves the
    /// retained cleanup owner and reservation; it never starts another process.
    pub fn stop_and_wait(&self, timeout: Duration) {
        let _ = self.stop();
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if !self.preparing.load(Ordering::Acquire)
                && self.supervisor.get().is_none_or(|owner| {
                    owner.status().is_some_and(|snapshot| {
                        // A Failed owner after a caught panic may still hold a live process.
                        matches!(snapshot.phase, Phase::Stopped | Phase::Retired)
                            || (snapshot.phase == Phase::Failed && snapshot.pid.is_none())
                    })
                })
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn prepare(&self) -> Result<(), &'static str> {
        let state = self
            .services
            .load()
            .map_err(|_| "Private storage is unavailable.")?;
        if !matches!(state.store.access(), DatabaseAccess::ReadWrite) {
            return Err("Update Shinatama before preparing the proxy in read-only storage.");
        }
        let lock = owner_lock(&state.location)?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| "No local proxy address is available.")?;
        let port = NonZeroU16::new(
            listener
                .local_addr()
                .map_err(|_| "The local proxy address is unavailable.")?
                .port(),
        )
        .ok_or("The local proxy address is invalid.")?;
        let private = prepare_private_sidecar(&state.location, port).map_err(
            |_| "Private proxy configuration could not be verified; existing files were preserved.",
        )?;
        let resources = resources_root()?;
        let bundle = verify_prepared_bundle(&resources.join("source-build/prepared")).map_err(|_| "The proxy package is missing or its fingerprint differs. Prepare the pinned package before opening an agent.")?;
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../../../../sidecar/runtime.lock.json"))
                .map_err(|_| "The bundled runtime inventory is invalid.")?;
        let node = verify_bundled_node(
            &resources.join("runtime"),
            NodePin {
                relative_path: PathBuf::from(
                    pin.get("relativePath")
                        .and_then(|value| value.as_str())
                        .ok_or("The runtime inventory is incomplete.")?,
                ),
                sha256: pin
                    .get("sha256")
                    .and_then(|value| value.as_str())
                    .ok_or("The runtime fingerprint is unavailable.")?
                    .to_owned(),
                version: pin
                    .get("fileVersion")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned),
                modules_abi: pin
                    .get("modulesAbi")
                    .and_then(|value| value.as_u64())
                    .and_then(|value| u32::try_from(value).ok()),
            },
        )
        .map_err(|_| "The bundled runtime is missing or its fingerprint differs.")?;
        let mut os = BTreeMap::new();
        for name in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(name) {
                os.insert(name.into(), value);
            }
        }
        // The sidecar sizes a Codex conversation by the window Codex itself uses, read from
        // the model table Codex keeps in its private home. Without it a model newer than the
        // sidecar's bundled table is held to a generic 272K window and refused long before
        // Codex would compact it.
        let codex_home = AgentStoreKey::new("codex")
            .map(|agent| PrivateStorePlan::home_path(state.location.directory(), &agent))
            .map_err(|_| "The private agent namespace is invalid.")?;
        let recipe = prepare_runtime(bundle, node, private.root(), &os, port, Some(&codex_home))
            .map_err(|_| "The private proxy launch environment could not be verified.")?;
        let identity = recipe.summary();
        let provider = self
            .runtime
            .sidecar_provider(lock, state.location.clone(), self.archive.clone())
            .map_err(|_| "Proxy resource admission is unavailable.")?;
        self.runtime.wait_for_interactive_admission().map_err(|_| "Resources are not yet ready for the local service. Wait and retry Start; no process was created.")?;
        // A competing bind is handled by exact child/health identity checks and
        // finite startup failure, never by attaching to or stopping that listener.
        drop(listener);
        if self.stop_requested.load(Ordering::Acquire) {
            return Err("Proxy preparation was cancelled; saved files were preserved.");
        }
        let supervisor = SidecarSupervisor::start(
            recipe,
            SupervisorConfig {
                port,
                poll_period: Duration::from_millis(100),
                health_period: Duration::from_millis(500),
                health_timeout: Duration::from_millis(200),
                startup_timeout: Duration::from_secs(20),
                graceful_stop_timeout: Duration::from_secs(2),
                cleanup_timeout: Duration::from_secs(3),
                drain_timeout: Duration::from_secs(30),
                reservation_lease: Duration::from_secs(300),
                maintenance_at_start: Duration::from_secs(10),
                maintenance_command_timeout: Duration::from_secs(120),
            },
            provider,
        )
        .map_err(
            |_| "A proxy owner is already retained or could not start; no duplicate was created.",
        )?;
        self.supervisor
            .set(supervisor)
            .map_err(|_| "A proxy owner is already retained.")?;
        let _ = self.connection.set((port, private.root().clone()));
        self.supervisor
            .get()
            .ok_or("The proxy owner is unavailable.")?
            .request(if self.stop_requested.load(Ordering::Acquire) {
                Command::Stop
            } else {
                Command::Start
            })
            .map_err(|_| "Proxy start could not be delivered.")?;
        let _ = self.identity.set(identity);
        Ok(())
    }
}
fn now_ms() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
}
fn owner_lock(location: &StorageLocation) -> Result<File, &'static str> {
    let path = location.directory().join("sidecar.owner.lock");
    let options = |new| {
        let mut value = OpenOptions::new();
        value
            .read(true)
            .write(true)
            .create_new(new)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        value
    };
    let file = match options(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => options(false)
            .open(&path)
            .map_err(|_| "The proxy owner lock is unavailable.")?,
        Err(_) => return Err("The proxy owner lock could not be created."),
    };
    location
        .secure_private_file(&path)
        .map_err(|_| "The proxy owner lock failed private-storage verification.")?;
    file.try_lock()
        .map_err(|_| "Another Shinatama window owns this proxy. Its process was preserved.")?;
    Ok(file)
}
/// Pinned Claude Code mods beside the executable (packaged) or in the repository
/// (debug). `None` when absent: Claude launches then simply carry no mods.
pub(crate) fn claude_mods_root() -> Option<PathBuf> {
    let packaged = std::env::current_exe()
        .ok()?
        .parent()?
        .join("resources/claude-mods");
    if packaged.is_dir() {
        return Some(packaged);
    }
    #[cfg(debug_assertions)]
    {
        let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)?
            .join("resources/claude-mods");
        if development.is_dir() {
            return Some(development);
        }
    }
    None
}
fn resources_root() -> Result<PathBuf, &'static str> {
    let executable = std::env::current_exe()
        .map_err(|_| "The application resource directory is unavailable.")?;
    let packaged = executable
        .parent()
        .ok_or("The application resource directory is unavailable.")?
        .join("resources/sidecar");
    if packaged
        .try_exists()
        .map_err(|_| "The application resources are unavailable.")?
    {
        return Ok(packaged);
    }
    #[cfg(debug_assertions)]
    {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .map(|root| root.join("sidecar"))
            .ok_or("The development resource directory is unavailable.");
    }
    #[cfg(not(debug_assertions))]
    Err("The proxy resources are missing from this application package.")
}
/// The `sidecar_capabilities` answer: `{ state, capabilities? }`.
pub fn capabilities_reply(read: CapabilityRead) -> serde_json::Value {
    match read {
        CapabilityRead::Observed(capabilities) => serde_json::json!({
            "state": "observed",
            "capabilities": capabilities,
        }),
        CapabilityRead::NotObserved => serde_json::json!({ "state": "not_observed" }),
        CapabilityRead::Unavailable => serde_json::json!({ "state": "unavailable" }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_engine::sidecar_admin::{Lever, SessionCapabilities};
    use shinatama_engine::sidecar_config::intended_baseline;

    #[test]
    fn idle_runs_after_ten_minutes_without_live_sessions() {
        let now = std::time::Instant::now() + Duration::from_secs(1000);
        let ago = |seconds| Some(now - Duration::from_secs(seconds));
        assert!(!idle_run_due(Some(0), ago(599), now, false));
        assert!(idle_run_due(Some(0), ago(600), now, false));
        assert!(!idle_run_due(Some(1), ago(900), now, false));
        assert!(!idle_run_due(None, ago(900), now, false), "an unknown count is not idle");
        assert!(idle_run_due(Some(0), ago(1), now, true), "a forget makes it due now");
        assert!(!idle_run_due(Some(0), None, now, false));
        assert!(!idle_run_due(Some(2), ago(1), now, true), "not even a forget with an agent open");
    }

    #[test]
    fn a_session_that_opens_or_is_live_is_activity() {
        let activity = SessionActivity::default();
        assert_eq!(activity.busy(), None, "no count yet");
        assert!(activity.active(), "an unknown count is activity");
        activity.set_live(Box::new(|| Some(0)));
        assert!(!activity.active());
        let guard = activity.opening.enter();
        assert_eq!(activity.busy(), Some(1));
        assert!(activity.active(), "an open in progress");
        drop(guard);
        assert!(!activity.active());
    }

    #[test]
    fn a_skipped_idle_run_is_asked_again_after_another_idle_period() {
        let now = std::time::Instant::now() + Duration::from_secs(2000);
        assert!(send_due(None, now, false));
        assert!(!send_due(Some(now - Duration::from_secs(599)), now, false));
        assert!(send_due(Some(now - Duration::from_secs(600)), now, false));
        assert!(send_due(Some(now - Duration::from_secs(1)), now, true), "a forget asks at once");
    }

    #[test]
    fn an_open_in_progress_is_not_idle() {
        let opening = Opening::default();
        let now = std::time::Instant::now() + Duration::from_secs(1000);
        let idle = Some(now - Duration::from_secs(900));
        {
            let _first = opening.enter();
            let _second = opening.enter();
            assert_eq!(opening.count(), 2);
            assert!(!idle_run_due(busy_sessions(Some(0), opening.count()), idle, now, true));
        }
        assert_eq!(opening.count(), 0, "an open that ends, by any path, no longer counts");
        assert!(idle_run_due(busy_sessions(Some(0), opening.count()), idle, now, false));
        assert_eq!(busy_sessions(None, 1), None, "an unknown count stays unknown");
    }

    #[test]
    fn the_log_line_says_what_a_run_did() {
        use shinatama_engine::sidecar_archive::{Report, Stop};
        let done = Report {
            removed: 487,
            failed: 0,
            left: 0,
            released_bytes: 1_642_000_000,
            compacted: true,
            file_bytes_before: Some(2_451_185_664),
            file_bytes_after: Some(18_731_008),
            elapsed: Duration::from_secs(1396),
            stop: Stop::Done,
        };
        assert_eq!(
            report_line(&done),
            "removed 487 sessions (1642 MB) in 1396 s; 0 left; archive now 18.7 MB."
        );
        let stopped = Report { removed: 3, left: 484, stop: Stop::Cancelled, ..done.clone() };
        assert!(report_line(&stopped).ends_with("; stopped for an open."), "{}", report_line(&stopped));
        let budget = Report { left: 10, stop: Stop::Budget, ..done.clone() };
        assert!(report_line(&budget).ends_with("; 10 left; archive now 18.7 MB; the rest goes in the next run."), "{}", report_line(&budget));
        let failed = Report { failed: 2, ..done.clone() };
        assert!(report_line(&failed).contains("; 2 could not be removed"), "{}", report_line(&failed));
        let unreadable = Report { removed: 0, stop: Stop::Unreadable, ..done };
        assert_eq!(
            report_line(&unreadable),
            "nothing removed: the archive or Shinatama's threads could not be read."
        );
    }

    #[test]
    fn a_supervisor_silent_for_more_than_fifteen_seconds_is_stalled() {
        let now = 1_000_000;
        assert!(stalled(Some(now - 15_001), now, Phase::Recovering));
        assert!(!stalled(Some(now - 15_000), now, Phase::Recovering));
        assert!(!stalled(Some(now - 60_000), now, Phase::Failed));
        assert!(!stalled(Some(now - 60_000), now, Phase::Stopped));
        assert!(!stalled(Some(now - 60_000), now, Phase::Retired));
        assert!(!stalled(None, now, Phase::Recovering));
        assert!(!stalled(Some(now + 5_000), now, Phase::Ready));
    }
    #[test]
    fn the_refusal_names_the_state_its_age_the_exit_and_the_disk() {
        let incident = RefusalFacts {
            phase: "unavailable",
            healthy: false,
            stalled: Some(("20:52:40".into(), "recovering")),
            message: Some(
                "Waiting to confirm cleanup before restarting the proxy. Its reservation remains held.",
            ),
            exit: Some(("20:52:39".into(), 1, None)),
            disk_line: Some(
                "Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama.".into(),
            ),
        };
        assert_eq!(
            refusal_text(&incident),
            "Not started: the model proxy has not reported since 20:52:40 (last state: recovering). It last said: Waiting to confirm cleanup before restarting the proxy. Its reservation remains held. Its process ended at 20:52:39 with code 1. Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama. Restart Shinatama from the Model proxy panel."
        );
        let busy = RefusalFacts {
            phase: "busy",
            healthy: false,
            stalled: None,
            message: None,
            exit: None,
            disk_line: None,
        };
        assert_eq!(
            refusal_text(&busy),
            "Not started: the model proxy is not answering its health check. Its state is in the Model proxy panel."
        );
        let exited = RefusalFacts {
            exit: Some((
                "20:52:39".into(),
                1,
                Some("Error: ENOSPC: no space left on device.".into()),
            )),
            ..busy
        };
        assert!(refusal_text(&exited).contains(
            "Its process ended at 20:52:39 with code 1: Error: ENOSPC: no space left on device. Its state"
        ));
    }
    #[test]
    fn no_status_message_claims_saved_data_is_preserved() {
        for reason in [
            Reason::AdmissionDeferred,
            Reason::StartupRefused,
            Reason::ProcessExited,
            Reason::CleanupUncertain,
            Reason::ResourceLimit,
            Reason::SupervisorFault,
        ] {
            for phase in [Phase::Recovering, Phase::Failed, Phase::Retired] {
                if let Some(text) = reason_message(phase, reason) {
                    assert!(!text.to_lowercase().contains("preserved"), "{text}");
                }
            }
        }
        assert_eq!(
            reason_message(Phase::Failed, Reason::SupervisorFault),
            Some("The proxy supervisor stopped on an internal error. Start or Retry recovery starts it again.")
        );
    }

    /// 2026-11-18: the proxy ended at every start with an error nothing recorded.
    #[test]
    fn each_exit_of_the_proxy_is_logged_with_what_it_said_and_a_loop_is_thinned() {
        let exit = |count: u32, code: u32, detail: Option<&str>| ProxyExit {
            code,
            uptime_ms: 412,
            at_unix_ms: Some(1),
            detail: detail.map(str::to_owned),
            count,
        };
        let busy = Some("bili: failed to start: Error: EBUSY: resource busy or locked");
        let mut log = ExitLog::default();
        assert_eq!(
            log.line(&exit(1, 1, busy)).as_deref(),
            Some(
                "The process ended with code 1 after 412 ms (exit 1 since launch). It said: bili: failed to start: Error: EBUSY: resource busy or locked"
            )
        );
        assert_eq!(
            log.line(&exit(2, 1, busy)).as_deref(),
            Some(
                "The process ended with code 1 after 412 ms (exit 2 since launch). It said: bili: failed to start: Error: EBUSY: resource busy or locked The same exit 2 times in a row."
            )
        );
        let written: Vec<u32> = (3..=45)
            .filter(|count| log.line(&exit(*count, 1, busy)).is_some())
            .collect();
        assert_eq!(written, [3, 4, 5, 20, 40]);
        // Another cause is written at once and starts its own count.
        assert_eq!(
            log.line(&exit(46, 134, None)).as_deref(),
            Some(
                "The process ended with code 134 after 412 ms (exit 46 since launch). It reported no error."
            )
        );
    }

    #[test]
    fn capabilities_reply_names_the_state_and_carries_the_report_only_when_observed() {
        let lever = |applied, reason: &str| Lever {
            configured: None,
            applied,
            reason: reason.to_owned(),
        };
        let observed = capabilities_reply(CapabilityRead::Observed(SessionCapabilities {
            schema_version: 1,
            protocol: "responses".into(),
            requests: 4,
            profile: None,
            passthrough: false,
            ccr: lever(Some(false), "codex_host"),
            read_original_image: lever(Some(false), "codex_host"),
            image_rescale: lever(None, "not_observed"),
            steering_level: Some(2),
            effort_routing: Some(false),
            prompt_rewrite_mode: Some("auto".into()),
            stamped_at_request: Some(3),
        }));
        assert_eq!(observed["state"], "observed");
        assert_eq!(observed["capabilities"]["ccr"]["reason"], "codex_host");
        assert_eq!(observed["capabilities"]["ccr"]["applied"], false);
        assert_eq!(observed["capabilities"]["steeringLevel"], 2);
        assert_eq!(observed["capabilities"]["promptRewriteMode"], "auto");
        assert_eq!(
            capabilities_reply(CapabilityRead::NotObserved),
            serde_json::json!({ "state": "not_observed" })
        );
        assert_eq!(
            capabilities_reply(CapabilityRead::Unavailable),
            serde_json::json!({ "state": "unavailable" })
        );
    }

    #[test]
    fn drift_is_reported_when_resolved_differs_from_baseline() {
        let mut resolved = intended_baseline();
        assert_eq!(drift(Some(&resolved)), Some(false));
        resolved.verbosity_level = Some(3);
        assert_eq!(drift(Some(&resolved)), Some(true));
        assert_eq!(drift(None), None);
    }

    #[test]
    fn differences_name_the_values_read_from_the_process() {
        assert!(config_differences(None).is_empty());
        let mut resolved = intended_baseline();
        assert!(config_differences(Some(&resolved)).is_empty());
        resolved.verbosity_level = Some(3);
        resolved.effort_routing = Some(true);
        resolved.ccr_min_tool_tokens = None;
        // A configuration written before FRD-0009 R31 has no visibilityMarkers key.
        resolved.visibility_markers = None;
        let found = config_differences(Some(&resolved));
        let line = |setting: &str| {
            let item = found.iter().find(|item| item.setting == setting).expect(setting);
            (item.read.as_str(), item.baseline.as_str())
        };
        assert_eq!(found.len(), 4);
        assert_eq!(line("Verbosity level"), ("3", "2"));
        assert_eq!(line("Effort routing"), ("true", "false"));
        assert_eq!(line("Large-result storage minimum tool tokens"), ("not reported", "4000"));
        assert_eq!(line("Proxy messages in the conversation"), ("not reported", "false"));
    }

    #[test]
    fn differences_bound_an_empty_or_overlong_prompt_rewrite_mode() {
        let mut resolved = intended_baseline();
        resolved.prompt_rewrite_mode = Some(String::new());
        let found = config_differences(Some(&resolved));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].read, "(empty)");
        // Multi-byte characters: the cut must land on a character boundary.
        resolved.prompt_rewrite_mode = Some("\u{e9}".repeat(200));
        let found = config_differences(Some(&resolved));
        assert_eq!(found[0].read.chars().count(), DIFFERENCE_TEXT_LIMIT);
        assert!(found[0].read.ends_with('\u{2026}'));
        assert!(found[0].read.starts_with("\u{e9}\u{e9}"));
        resolved.prompt_rewrite_mode = Some("x".repeat(DIFFERENCE_TEXT_LIMIT));
        assert_eq!(config_differences(Some(&resolved))[0].read.len(), DIFFERENCE_TEXT_LIMIT);
    }

    #[test]
    fn stop_is_refused_while_agents_are_live_unless_forced() {
        assert_eq!(decide_stop(Some(2), false), StopDecision::Refuse);
        assert_eq!(decide_stop(Some(2), true), StopDecision::Drain);
        assert_eq!(decide_stop(Some(0), false), StopDecision::Drain);
        assert_eq!(decide_stop(Some(0), true), StopDecision::Drain);
        // An unreadable count (no agents controller yet, poisoned registry) fails closed.
        assert_eq!(decide_stop(None, false), StopDecision::Refuse);
        assert_eq!(decide_stop(None, true), StopDecision::Drain);
    }

    #[test]
    fn draining_owner_issues_no_launch_permit() {
        assert!(!launch_allowed("draining", true));
        assert!(launch_allowed("ready", true));
        // A streaming response keeps the proxy busy; opening another agent must still pass.
        assert!(launch_allowed("busy", true));
        assert!(!launch_allowed("busy", false));
        assert!(!launch_allowed("ready", false));
        assert!(!launch_allowed("stopped", true));
    }
}
