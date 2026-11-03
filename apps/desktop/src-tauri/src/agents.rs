//! Explicit checkup and original-TUI startup. Checks use a fresh credential-free
//! canary; no auth query, installation, message or implicit proxy start occurs.
#![cfg(windows)]
use crate::{sidecar, storage::Services, terminal_runtime::Runtime};
use serde::Serialize;
use shinatama_adapter::sets::{SET_SCHEMA_VERSION, SetManifest};
use shinatama_engine::claude_probe::{
    Authentication, ClaudeProbeOutcome, ClaudeProbeSummary, PreparedClaudeProbe, ProbeIssue,
    ProgramCandidate, discover_claude, probe_budget,
};
use shinatama_engine::{
    check_scratch::{self, OwnedScratch, ScratchKind},
    claude_launch::{ClaudeStartContext, ClaudeTrialContext, prepare_start, prepare_trial},
    native_launch::{NativeTrialReceipt, NativeTrialTracker},
};
use shinatama_state::{
    database::{Database, DatabaseAccess},
    location::StorageLocation,
    records::{
        CanonicalIdKind, PreparedStoreRoot, RecordsError, RootAccess, RootKind, RootOrigin,
        RootScope, StoreRootRecord, new_id,
    },
    store_roots::{AgentStoreKey, PreparedPrivateStore, PrivateStorePlan, PrivateStoreRequest},
};
use shinatama_terminal::{SessionInfo, TerminalHost};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const CLAUDE: &str = "claude_code";
const ROOT_SCAN_LIMIT: usize = 100;
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Unknown,
    Ready,
    Missing,
    Unavailable,
    Unsupported,
}
#[derive(Clone, Copy, Serialize)]
pub struct Step {
    state: StepState,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramState {
    Missing,
    Found,
    Unavailable,
}
#[derive(Clone, Serialize)]
pub struct Program {
    state: ProgramState,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}
/// The only UI projection. Strings are static/redacted except parsed CLI version.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCheckup {
    agent_id: &'static str,
    display_name: &'static str,
    program: Program,
    home: Step,
    sidecar: Step,
    sets: Step,
    authentication: Authentication,
    can_open: bool,
    can_login: bool,
    issues: Vec<&'static str>,
}
impl AgentCheckup {
    fn empty(agent_id: &'static str, display_name: &'static str) -> Self {
        Self {
            agent_id,
            display_name,
            program: Program {
                state: ProgramState::Unavailable,
                version: None,
            },
            home: Step {
                state: StepState::Unknown,
            },
            sidecar: Step {
                state: StepState::Unknown,
            },
            sets: Step {
                state: StepState::Unsupported,
            },
            authentication: Authentication::Unknown,
            can_open: false,
            can_login: false,
            issues: Vec::new(),
        }
    }
    fn issue(&mut self, message: &'static str) {
        if self.issues.len() < 8 && !self.issues.contains(&message) {
            self.issues.push(message);
        }
    }
}
struct BoxHome {
    _grant: PreparedPrivateStore,
    record: StoreRootRecord,
}
/// Trusted Rust-only observation, not authority to start a native TUI. The
/// qualifier must re-pin/hash the candidate and match this ACTUAL opaque outcome.
/// No Serialize/Deserialize: neither paths nor a fabricated ready flag enter IPC.
#[derive(Clone)]
pub struct ClaudeObservation {
    pub candidate: ProgramCandidate,
    pub outcome: Arc<ClaudeProbeOutcome>,
    pub root: StoreRootRecord,
}
#[derive(Default)]
struct Facts {
    home: Option<BoxHome>,
    retained_probe: Option<ClaudeProbeOutcome>,
    observed: Option<ClaudeObservation>,
    receipt: Option<Arc<NativeTrialReceipt>>,
    canary: Option<Arc<CanaryOwner>>,
}
struct CanaryOwner {
    host: Arc<TerminalHost>,
    tracker: NativeTrialTracker,
    session_id: Mutex<Option<String>>,
    _base: Mutex<Option<StorageLocation>>,
    scratch: Mutex<OwnedScratch>,
}
impl CanaryOwner {
    fn cleanup_scratch(&self, base: &StorageLocation, receipt: &NativeTrialReceipt) {
        // Successful receipt is issued only after real native/queue/SQL release.
        // Drop our C01 owner before opening DELETE; no prior directory is adopted.
        let Ok(mut location) = self._base.lock() else {
            return;
        };
        drop(location.take());
        drop(location);
        let Ok(mut scratch) = self.scratch.lock() else {
            return;
        };
        let mut report = check_scratch::cleanup(base, &mut scratch, receipt.cleanup_status());
        while report.issue == Some(check_scratch::Issue::InUse) && report.attempts_remaining > 0 {
            std::thread::sleep(Duration::from_millis(5));
            report = check_scratch::cleanup(base, &mut scratch, receipt.cleanup_status());
        }
    }
    fn stop(&self) -> bool {
        let id = self.session_id.lock().ok().and_then(|id| id.clone());
        if let Some(id) = id {
            return self.host.stop(&id).is_ok();
        }
        // The dedicated host tracks unannounced startup and retired sessions.
        // Its authoritative released state differs from an empty telemetry read.
        if self.host.fully_released() == Some(true) {
            return true;
        }
        let resources = self.host.resources();
        !resources.is_empty()
            && resources
                .iter()
                .all(|session| self.host.stop(&session.id).is_ok())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStarted {
    pub session: SessionInfo,
    pub thread_id: String,
    pub trial: bool,
}
pub struct Controller {
    services: Arc<Services>,
    runtime: Arc<Runtime>,
    sidecar: Arc<sidecar::Controller>,
    busy: AtomicBool,
    blocked: AtomicBool,
    facts: Mutex<Facts>,
    last_start_failure: Mutex<Option<String>>,
}
struct RequestSlot(Arc<Controller>);
impl Drop for RequestSlot {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}
impl Controller {
    pub fn diagnostics(&self) -> serde_json::Value {
        let owner = self
            .facts
            .try_lock()
            .ok()
            .and_then(|facts| facts.canary.clone());
        serde_json::json!({
            "checking": self.busy.load(Ordering::Acquire),
            "blocked": self.blocked.load(Ordering::Acquire),
            "canary": owner.as_ref().map(|owner| owner.host.resources()),
            "trialReadyForCleanup": owner.as_ref().map(|owner| owner.tracker.ready_for_cleanup()),
            "startupFailure": self.last_start_failure.try_lock().ok().and_then(|value| value.clone()),
        })
    }
    pub fn new(
        services: Arc<Services>,
        runtime: Arc<Runtime>,
        sidecar: Arc<sidecar::Controller>,
    ) -> Self {
        Self {
            services,
            runtime,
            sidecar,
            busy: AtomicBool::new(false),
            blocked: AtomicBool::new(false),
            facts: Mutex::new(Facts::default()),
            last_start_failure: Mutex::new(None),
        }
    }
    /// One retained blocking worker. Abandoning the IPC future does not free its
    /// slot while a native/FS call is still running, and never starts a replacement.
    pub async fn check(self: &Arc<Self>, agent_id: &str) -> Result<AgentCheckup, &'static str> {
        let (id, name) = match agent_id {
            CLAUDE => (CLAUDE, "Claude Code"),
            "codex" => ("codex", "Codex"),
            "opencode" => ("opencode", "OpenCode"),
            "pi" => ("pi", "Pi"),
            "hermes_agent" => ("hermes_agent", "Hermes Agent"),
            "deepseek_harness" => ("deepseek_harness", "DeepSeek Harness"),
            _ => return Err("This agent is not registered."),
        };
        if id != CLAUDE {
            let mut report = AgentCheckup::empty(id, name);
            report.issue("This agent has not been qualified for native use in this build.");
            return Ok(report);
        }
        if self.blocked.load(Ordering::Acquire) {
            return Err(
                "The previous agent check could not be confirmed. Restart Shinatama before checking again.",
            );
        }
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "An agent check is still returning. No duplicate was started.")?;
        let slot = RequestSlot(Arc::clone(self));
        if let Ok(mut failure) = self.last_start_failure.lock() {
            *failure = None;
        }
        tauri::async_runtime::spawn_blocking(move || {
            let slot = slot;
            let owner = &slot.0;
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| owner.check_claude())) {
                Ok(report) => report,
                Err(_) => {
                    owner.blocked.store(true, Ordering::Release);
                    Err("The agent check stopped unexpectedly. Saved data was preserved.")
                }
            }
        })
        .await
        .map_err(|_| "The agent check worker is unavailable; no replacement was started.")?
    }
    fn check_claude(&self) -> Result<AgentCheckup, &'static str> {
        {
            let mut facts = self
                .facts
                .lock()
                .map_err(|_| "Agent ownership information is unavailable.")?;
            facts.observed = None;
            facts.receipt = None;
        }
        let mut report = AgentCheckup::empty(CLAUDE, "Claude Code");
        let sidecar = self.sidecar.status();
        report.sidecar.state = if sidecar.healthy && matches!(sidecar.phase, "ready" | "busy") {
            StepState::Ready
        } else if sidecar.phase == "stopped" {
            StepState::Missing
        } else {
            StepState::Unavailable
        };
        if !matches!(report.sidecar.state, StepState::Ready) {
            report.issue(
                "The session index is not ready. Prepare it explicitly before opening an agent.",
            );
        }
        report.issue("The empty default set has no optional dependencies; exact-build activation is not yet qualified.");
        report.issue(
            "Shinatama did not query authentication. Sign-in stays in the original agent terminal.",
        );
        let state = self.services.load().map_err(
            |_| "Private application storage is unavailable; no alternative home was used.",
        )?;
        if !matches!(state.store.access(), DatabaseAccess::ReadWrite) {
            report.home.state = StepState::Unavailable;
            report.issue(
                "Update Shinatama before preparing this agent with the newer saved database.",
            );
            return Ok(report);
        }
        state.store.check_write_admission().map_err(
            |_| "Current storage admission is unavailable; no agent files were changed.",
        )?;
        let retained_canary = self
            .facts
            .lock()
            .map_err(|_| "Canary ownership information is unavailable.")?
            .canary
            .clone();
        if let Some(prior) = retained_canary {
            if !prior.stop() {
                report.issue("The previous TUI check has unconfirmed cleanup. Its owner is retained; no replacement was started.");
                return Ok(report);
            }
            self.facts
                .lock()
                .map_err(|_| "Canary ownership information is unavailable.")?
                .canary = None;
        }
        // Retry only the SAME retained cleanup. Expiry does not free its budget.
        let retained = self
            .facts
            .lock()
            .map_err(|_| "Agent ownership information is unavailable.")?
            .retained_probe
            .take();
        if let Some(mut prior) = retained {
            prior.retry_cleanup(&state.store);
            if prior.cleanup_retained() {
                self.project_probe(&mut report, prior.summary());
                report.issue("The previous version probe has unconfirmed cleanup. Its budget remains held; no replacement was started.");
                self.facts
                    .lock()
                    .map_err(|_| "Agent ownership information is unavailable.")?
                    .retained_probe = Some(prior);
                return Ok(report);
            }
        }
        let candidate = match discover_claude() {
            Ok(candidate) => candidate,
            Err(issue) => {
                report.program.state = if issue == ProbeIssue::NotFound {
                    ProgramState::Missing
                } else {
                    ProgramState::Unavailable
                };
                report.issue(probe_issue(issue));
                return Ok(report);
            }
        };
        report.program.state = ProgramState::Found;
        match prepare_box(&state.store, &state.location) {
            Ok(home) => {
                report.home.state = StepState::Ready;
                self.facts
                    .lock()
                    .map_err(|_| "Agent home ownership information is unavailable.")?
                    .home = Some(home);
            }
            Err(message) => {
                report.home.state = StepState::Unavailable;
                report.issue(message);
                return Ok(report);
            }
        }
        check_directory_budget(&state.location, "claude-probe-")?;
        let prepared = match PreparedClaudeProbe::prepare(&candidate, &state.location) {
            Ok(prepared) => prepared,
            Err(issue) => {
                report.issue(probe_issue(issue));
                return Ok(report);
            }
        };
        let outcome = match prepared.run_with(&state.store, || {
            self.runtime
                .prepare_background_workload(probe_budget(), 256 * 1024 * 1024, 15_000)
                .map_err(|_| ProbeIssue::AdmissionRefused)
        }) {
            Ok(outcome) => outcome,
            Err(issue) => {
                report.issue(probe_issue(issue));
                return Ok(report);
            }
        };
        self.project_probe(&mut report, outcome.summary());
        let observation = {
            let mut facts = self
                .facts
                .lock()
                .map_err(|_| "Agent probe ownership information is unavailable.")?;
            if outcome.cleanup_retained() {
                facts.retained_probe = Some(outcome);
                None
            } else if outcome.summary().issue.is_none() && outcome.summary().version.is_some() {
                let root = facts
                    .home
                    .as_ref()
                    .ok_or("The boxed-home owner is unavailable.")?
                    .record
                    .clone();
                let observation = ClaudeObservation {
                    candidate,
                    outcome: Arc::new(outcome),
                    root,
                };
                facts.observed = Some(observation.clone());
                Some(observation)
            } else {
                None
            }
        };
        if let Some(observation) = observation
            && matches!(report.sidecar.state, StepState::Ready)
        {
            match self.run_canary(&state.location, &state.store, &observation) {
                Ok(receipt) => {
                    self.facts
                        .lock()
                        .map_err(|_| "TUI qualification information is unavailable.")?
                        .receipt = Some(Arc::new(receipt));
                    report.sets.state = StepState::Ready;
                    report.can_open = true;
                    // Opening the original TUI is qualified; the separate login
                    // operation has no receipt yet. Users retain its own flow.
                    report.can_login = false;
                    report
                        .issues
                        .retain(|issue| !issue.starts_with("The empty default set"));
                    report.issue("The original TUI passed its isolated startup and cleanup check. Sign-in remains in that TUI.");
                    report.issue("Qualified scope: safe mode with an empty set. Managed CLI policy and resulting background traffic remain outside this check.");
                    return Ok(report);
                }
                Err(message) => report.issue(message),
            }
        }
        // Program observation + prepared directories are not a TUI qualification.
        report.issue("Opening and sign-in remain unavailable until the exact build passes native qualification.");
        Ok(report)
    }
    /// Explicit user Open/Sign-in only. The caller supplies its already selected
    /// project; no program, argv, environment, profile or ready flag enters IPC.
    pub async fn start(
        self: &Arc<Self>,
        agent_id: &str,
        host: Arc<TerminalHost>,
        project: PathBuf,
        rows: u16,
        cols: u16,
    ) -> Result<AgentStarted, &'static str> {
        if agent_id != CLAUDE {
            return Err("This agent is not qualified for native startup.");
        }
        shinatama_terminal::validate_size(rows, cols)
            .map_err(|_| "The agent terminal size is invalid.")?;
        if self.blocked.load(Ordering::Acquire) {
            return Err("The agent owner requires an application restart before another action.");
        }
        self.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "An agent action is still returning. No duplicate was started.")?;
        let slot = RequestSlot(Arc::clone(self));
        tauri::async_runtime::spawn_blocking(move || {
            let slot = slot;
            let owner = &slot.0;
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                owner.start_claude(&host, &project, rows, cols)
            })) {
                Ok(result) => {
                    if let Err(message) = &result {
                        if let Ok(mut failure) = owner.last_start_failure.lock() {
                            if failure.is_none() {
                                *failure = Some((*message).to_owned());
                            }
                        }
                    }
                    result
                }
                Err(_) => {
                    owner.blocked.store(true, Ordering::Release);
                    Err("Agent startup stopped unexpectedly. Its owned resources remain tracked.")
                }
            }
        })
        .await
        .map_err(|_| "The agent startup worker is unavailable; no replacement was started.")?
    }
    fn start_claude(
        &self,
        host: &TerminalHost,
        project: &std::path::Path,
        rows: u16,
        cols: u16,
    ) -> Result<AgentStarted, &'static str> {
        let (observation, receipt) = {
            let facts = self
                .facts
                .lock()
                .map_err(|_| "Agent qualification information is unavailable.")?;
            if facts.canary.is_some() || facts.retained_probe.is_some() {
                return Err(
                    "An agent check still owns unconfirmed resources. No new process was started.",
                );
            }
            (facts.observed.clone().ok_or("Check this agent before opening it.")?,
                facts.receipt.clone().ok_or("The native TUI check has not qualified this build. Check the agent explicitly." )?)
        };
        let state = self
            .services
            .load()
            .map_err(|_| "Private application storage is unavailable.")?;
        state
            .store
            .check_write_admission()
            .map_err(|_| "Storage admission is unavailable; no agent process was started.")?;
        state
            .location
            .validate_project(project)
            .map_err(|_| "The selected project cannot be used for this agent.")?;
        let home = prepare_box(&state.store, &state.location)?;
        if home.record != observation.root {
            return Err(
                "The saved agent home changed after its check. Check the agent again before opening it.",
            );
        }
        let (sidecar, port, ca) = self.sidecar.qualified_owner()?;
        let selected_set = empty_set()?;
        let thread_id = new_id(CanonicalIdKind::Thread);
        let prepared = prepare_start(ClaudeStartContext {
            candidate: &observation.candidate, observation: &observation.outcome, base: &state.location,
            database: &state.store, private_home: home._grant, root: home.record, project, sidecar,
            proxy_port: port, ca_file: &ca, selected_set: &selected_set, thread_id: &thread_id,
        }, &receipt).map_err(|_| "Claude startup no longer matches its qualified build, home or resource profile. Check the agent again.")?;
        self.sidecar.qualified_owner()?;
        self.runtime.wait_for_admission().map_err(|_| "Resources are not ready for a new agent. Existing sessions keep their limits; retry explicitly.")?;
        self.runtime
            .configure(host)
            .map_err(|_| "Contained native terminal admission is unavailable.")?;
        let project_record = state
            .store
            .select(project)
            .map_err(|_| "The selected project could not be saved. No agent was started.")?;
        let saved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|time| i64::try_from(time.as_millis()).ok())
            .ok_or("The thread timestamp is unavailable. No agent was started.")?;
        state
            .store
            .create_thread(&shinatama_state::threads::ThreadRecord {
                id: thread_id.clone(),
                project_id: project_record.id,
                kind: shinatama_state::records::ThreadKind::Work,
                parent_thread_id: None,
                companion_worker_thread_id: None,
                title: None,
                archived: false,
                revision: 1,
                created_at: saved_at,
                updated_at: saved_at,
            })
            .map_err(|_| "The work thread could not be saved. No agent was started.")?;
        let session = host.start_prepared(prepared.launch, prepared.permit, rows, cols)
            .map_err(|error| {
                if let Ok(mut failure) = self.last_start_failure.lock() { *failure = Some(error.to_string().chars().filter(|c| !c.is_control()).take(240).collect()); }
                "The agent terminal did not confirm startup. Its owned resources remain tracked; no shell fallback was used."
            })?;
        Ok(AgentStarted {
            session,
            thread_id,
            trial: false,
        })
    }
    fn run_canary(
        &self,
        base: &StorageLocation,
        database: &Database,
        observation: &ClaudeObservation,
    ) -> Result<NativeTrialReceipt, &'static str> {
        database
            .check_write_admission()
            .map_err(|_| "Storage admission changed; no TUI check was started.")?;
        let (sidecar, port, ca) = self.sidecar.qualified_owner()?;
        check_directory_budget(base, "claude-tui-trial-")?;
        let (trial, scratch) = check_scratch::create(base, ScratchKind::ClaudeTuiTrial)
            .map_err(|_| "The private TUI-check location is unavailable.")?
            .into_parts();
        let project = trial
            .resolve_related_directory(&trial.directory().join("project"))
            .map_err(|_| "The synthetic check project is unavailable.")?;
        project
            .prepare_private_directory()
            .map_err(|_| "The synthetic check project could not be created privately.")?;
        let grant = PrivateStorePlan::resolve(
            &trial,
            AgentStoreKey::new(CLAUDE).map_err(|_| "The check agent namespace is invalid.")?,
            PrivateStoreRequest::default(),
        )
        .and_then(|plan| plan.prepare())
        .map_err(|_| "A fresh private home could not be prepared for the TUI check.")?;
        let selected_set = empty_set()?;
        let thread_id = new_id(CanonicalIdKind::Thread);
        let prepared = prepare_trial(ClaudeTrialContext {
            candidate: &observation.candidate, observation: &observation.outcome, base, trial_base: &trial,
            private_home: grant, project: project.directory(), sidecar, proxy_port: port, ca_file: &ca,
            selected_set: &selected_set, thread_id: &thread_id,
        }).map_err(|_| "The exact Claude build could not prepare its isolated TUI check. Opening remains unavailable.")?;
        self.sidecar.qualified_owner()?;
        self.runtime.wait_for_admission().map_err(|_| "Resources are not ready for the isolated TUI check. No replacement was started; retry explicitly.")?;
        let host = Arc::new(TerminalHost::default());
        self.runtime
            .configure_canary(&host)
            .map_err(|_| "Contained TUI-check admission is unavailable.")?;
        let owner = Arc::new(CanaryOwner {
            host,
            tracker: prepared.tracker,
            session_id: Mutex::new(None),
            _base: Mutex::new(Some(trial)),
            scratch: Mutex::new(scratch),
        });
        self.facts
            .lock()
            .map_err(|_| "Canary ownership information is unavailable.")?
            .canary = Some(Arc::clone(&owner));
        let deadline = Instant::now() + Duration::from_secs(7);
        let session = match owner
            .host
            .start_prepared(prepared.launch, prepared.permit, 24, 80)
        {
            Ok(session) => session,
            Err(error) => {
                // Host errors are fixed lifecycle/OS messages, never child
                // stdout, environment, credentials or a caller-supplied recipe.
                if let Ok(mut failure) = self.last_start_failure.lock() {
                    *failure = Some(
                        error
                            .to_string()
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(240)
                            .collect(),
                    );
                }
                if owner.stop() {
                    self.facts
                        .lock()
                        .map_err(|_| "Canary ownership information is unavailable.")?
                        .canary = None;
                }
                return Err(
                    "The contained TUI check did not start. Any unconfirmed owner remains retained.",
                );
            }
        };
        *owner
            .session_id
            .lock()
            .map_err(|_| "Canary session ownership is unavailable.")? = Some(session.id.clone());
        let mut controls = CursorReplies::default();
        let mut total = 0usize;
        while Instant::now() < deadline {
            match owner.host.read(&session.id) {
                Ok(batch) => {
                    for chunk in &batch.chunks {
                        total = total.saturating_add(chunk.len());
                        if total > 512 * 1024 {
                            break;
                        }
                        controls.answer(&owner.host, &session.id, chunk);
                    }
                    if batch.eof || batch.error.is_some() || total > 512 * 1024 {
                        break;
                    }
                }
                Err(_) => break,
            }
            if owner.tracker.ready_for_cleanup() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let released = owner.stop();
        let receipt = released.then(|| owner.tracker.take_receipt()).flatten();
        drop(project);
        if let Some(receipt) = receipt.as_ref() {
            owner.cleanup_scratch(base, receipt);
        }
        if released {
            self.facts
                .lock()
                .map_err(|_| "Canary ownership information is unavailable.")?
                .canary = None;
        }
        receipt.ok_or(if released { "The TUI check did not produce a qualified startup receipt. No provider message was sent." }
            else { "TUI-check cleanup is unconfirmed. Its budget and owner remain retained; opening is unavailable." })
    }
    fn project_probe(&self, report: &mut AgentCheckup, summary: &ClaudeProbeSummary) {
        report.program.state = ProgramState::Found;
        report.program.version = summary.version.as_ref().map(ToString::to_string);
        if let Some(issue) = summary.issue {
            report.issue(probe_issue(issue));
        }
    }
}
fn probe_issue(issue: ProbeIssue) -> &'static str {
    match issue {
        ProbeIssue::NotFound => {
            "Claude Code was not found in the bounded local installation catalog. No installation was attempted."
        }
        ProbeIssue::Ambiguous => {
            "Multiple Claude installations were found. Choose one explicitly before native qualification."
        }
        ProbeIssue::WrapperUnresolved => {
            "Only a launcher wrapper was found. No shell fallback was run."
        }
        ProbeIssue::Busy => {
            "A previous Claude version check still owns its slot. No duplicate was started."
        }
        ProbeIssue::UnsafeProgram | ProbeIssue::InvalidCandidate | ProbeIssue::ProgramTooLarge => {
            "The Claude executable could not be safely pinned and verified."
        }
        ProbeIssue::PrivateRootUnavailable => {
            "An empty private version-probe directory could not be prepared. No system home was used."
        }
        ProbeIssue::Expired => {
            "The version probe reached its fixed time budget. Retry explicitly after cleanup."
        }
        ProbeIssue::AdmissionRefused => {
            "Fresh system headroom or shared resource admission is unavailable. Existing limits were preserved."
        }
        ProbeIssue::SpawnFailed => {
            "The contained version probe could not start. No unprotected fallback was used."
        }
        ProbeIssue::OutputLimit => "The version probe exceeded its bounded output allowance.",
        ProbeIssue::InvalidOutput | ProbeIssue::NonzeroExit => {
            "The version probe did not return a valid successful CLI version."
        }
        ProbeIssue::CleanupUnconfirmed => {
            "Version-probe cleanup is unconfirmed. Its resource reservation remains held."
        }
    }
}
fn prepare_box(database: &Database, location: &StorageLocation) -> Result<BoxHome, &'static str> {
    let plan = PrivateStorePlan::resolve(
        location,
        AgentStoreKey::new(CLAUDE).map_err(|_| "The private agent namespace is invalid.")?,
        PrivateStoreRequest::default(),
    )
    .map_err(
        |_| "The boxed home failed local/private path verification. No system home was used.",
    )?;
    // Inspect the authoritative binding BEFORE prepare() can recreate a child.
    // A missing or disconnected home needs an explicit reconnect, preserving its
    // previous identity and native session references.
    if let Some(root) = latest_box(database)? {
        let expected = plan
            .paths()
            .home()
            .to_str()
            .ok_or("The boxed home cannot be represented by this build.")?;
        let expected = expected.strip_prefix(r"\\?\").unwrap_or(expected);
        if !root.available || root.access != RootAccess::ReadWrite || root.resolved_path != expected
        {
            return Err(
                "The saved boxed-home binding is unavailable or differs. Reconnect it explicitly; history was not remapped.",
            );
        }
        if !plan
            .paths()
            .home()
            .try_exists()
            .map_err(|_| "The saved boxed home could not be inspected safely.")?
        {
            database.mark_store_root_unavailable(&root.id, root.config_revision)
                .map_err(|_| "The missing boxed home could not be marked unavailable. No directory was recreated.")?;
            return Err(
                "The saved boxed home is missing. Reconnect it explicitly; no replacement directory was created.",
            );
        }
    }
    let grant = plan.prepare().map_err(|_| "The boxed home could not be prepared privately. Existing data and ACLs were preserved.")?;
    let path = grant
        .paths()
        .home()
        .to_str()
        .ok_or("The boxed home cannot be represented by this build.")?;
    let ordinary = path.strip_prefix(r"\\?\").unwrap_or(path);
    let prepared = PreparedStoreRoot::resolve(StoreRootRecord {
        id: uuid::Uuid::new_v4().to_string(),
        harness: CLAUDE.into(),
        kind: RootKind::Box,
        scope: RootScope::Primary,
        resolved_path: ordinary.into(),
        origin: RootOrigin::Configured,
        config_revision: 1,
        access: RootAccess::ReadWrite,
        available: true,
    })
    .map_err(|_| "The boxed home could not be resolved for its authoritative store record.")?;
    let find = || -> Result<Option<StoreRootRecord>, &'static str> {
        let latest = latest_box(database)?;
        match latest {
            Some(root)
                if root.available
                    && root.access == RootAccess::ReadWrite
                    && root.resolved_path == prepared.record().resolved_path =>
            {
                Ok(Some(root))
            }
            Some(_) => Err(
                "The saved boxed-home binding is unavailable or differs. Reconnect it explicitly; history was not remapped.",
            ),
            None => Ok(None),
        }
    };
    let record = match find()? {
        Some(record) => record,
        None => match database.register_store_root(&prepared) {
            Ok(record) => record,
            Err(RecordsError::Conflict) => find()?
                .ok_or("Another window changed the boxed-home registration. Retry explicitly.")?,
            Err(_) => {
                return Err(
                    "The boxed-home store record could not be saved. No existing binding was changed.",
                );
            }
        },
    };
    Ok(BoxHome {
        _grant: grant,
        record,
    })
}
fn latest_box(database: &Database) -> Result<Option<StoreRootRecord>, &'static str> {
    let records = database
        .list_store_roots(None, ROOT_SCAN_LIMIT)
        .map_err(|_| "Saved agent roots could not be read safely.")?;
    if records.len() == ROOT_SCAN_LIMIT {
        return Err("Saved root history exceeds the bounded checkup page. No binding was changed.");
    }
    Ok(records
        .into_iter()
        .filter(|root| {
            root.harness == CLAUDE && root.kind == RootKind::Box && root.scope == RootScope::Primary
        })
        .max_by_key(|root| root.config_revision))
}
fn empty_set() -> Result<SetManifest, &'static str> {
    SetManifest::new(SET_SCHEMA_VERSION, "default", Vec::new())
        .map_err(|_| "The default empty set is unavailable.")
}
fn check_directory_budget(base: &StorageLocation, prefix: &str) -> Result<(), &'static str> {
    let entries = base
        .directory()
        .read_dir()
        .map_err(|_| "Private check-directory information is unavailable.")?;
    let mut retained = 0;
    for (index, entry) in entries.take(129).enumerate() {
        if index == 128 {
            return Err(
                "Private check-directory inspection reached its finite limit. No new check was started.",
            );
        }
        let entry =
            entry.map_err(|_| "A private check directory could not be inspected safely.")?;
        if entry.file_name().to_string_lossy().starts_with(prefix) {
            retained += 1;
        }
        if retained >= 8 {
            return Err(
                "Eight private check directories remain preserved. Archive them explicitly before further checks; no new check was started.",
            );
        }
    }
    Ok(())
}
#[derive(Default)]
struct CursorReplies {
    tail: Vec<u8>,
    replies: u8,
}
impl CursorReplies {
    fn answer(&mut self, host: &TerminalHost, session: &str, bytes: &[u8]) {
        // Only protocol answers to exact DSR requests. No Enter, navigation,
        // login response, prompt text or provider message is synthesized.
        for byte in bytes {
            if self.replies >= 16 {
                return;
            }
            self.tail.push(*byte);
            if self.tail.len() > 6 {
                self.tail.remove(0);
            }
            let reply = if self.tail.ends_with(b"\x1b[?6n") {
                Some(b"\x1b[?1;1R".as_slice())
            } else if self.tail.ends_with(b"\x1b[6n") {
                Some(b"\x1b[1;1R".as_slice())
            } else if self.tail.ends_with(b"\x1b[5n") {
                Some(b"\x1b[0n".as_slice())
            } else {
                None
            };
            if let Some(reply) = reply {
                let _ = host.write(session, reply.to_vec());
                self.replies += 1;
                self.tail.clear();
            }
        }
    }
}
