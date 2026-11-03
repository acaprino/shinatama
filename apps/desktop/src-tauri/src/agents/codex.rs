//! Codex's separate observations and native qualification share the desktop's
//! single request owner, resource governor and retained cleanup discipline.
use super::*;
use shinatama_engine::codex_launch::{
    CodexLaunchError, CodexStartContext, CodexTrialContext, prepare_start, prepare_trial,
};
use shinatama_engine::codex_probe::{
    CodexProbeOutcome, CodexProbeSummary, PreparedCodexProbe, ProbeIssue, discover_codex,
    probe_budget,
};

#[derive(Clone)]
struct CodexObservation {
    candidate: ProgramCandidate,
    outcome: Arc<CodexProbeOutcome>,
    root: StoreRootRecord,
}
#[derive(Default)]
pub(super) struct Facts {
    home: Option<BoxHome>,
    retained_probe: Option<CodexProbeOutcome>,
    observed: Option<CodexObservation>,
    receipt: Option<Arc<NativeTrialReceipt>>,
    canary: Option<Arc<CanaryOwner>>,
    last_probe: Option<CodexProbeSummary>,
}
impl Controller {
    pub(super) fn codex_diagnostics(&self) -> serde_json::Value {
        let owner = self
            .codex_facts
            .try_lock()
            .ok()
            .and_then(|facts| facts.canary.clone());
        serde_json::json!({
            "canary": owner.as_ref().map(|owner| owner.host.resources()),
            "trialReadyForCleanup": owner.as_ref().map(|owner| owner.tracker.ready_for_cleanup()),
            "versionProbe": self.codex_facts.try_lock().ok().and_then(|facts| facts.last_probe.clone()),
        })
    }
    pub(super) fn check_codex(&self) -> Result<AgentCheckup, &'static str> {
        {
            let mut facts = self
                .codex_facts
                .lock()
                .map_err(|_| "Agent ownership information is unavailable.")?;
            facts.observed = None;
            facts.receipt = None;
            facts.last_probe = None;
        }
        let mut report = AgentCheckup::empty(CODEX, "Codex");
        let sidecar = self.sidecar.status();
        report.sidecar.state = if sidecar.healthy && matches!(sidecar.phase, "ready" | "busy") {
            StepState::Ready
        } else if sidecar.phase == "stopped" {
            StepState::Missing
        } else {
            StepState::Unavailable
        };
        if !matches!(report.sidecar.state, StepState::Ready) {
            report.issue("The local session service is unavailable.");
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
            .codex_facts
            .lock()
            .map_err(|_| "Canary ownership information is unavailable.")?
            .canary
            .clone();
        if let Some(prior) = retained_canary {
            if !prior.stop() {
                report.issue("The previous TUI check has unconfirmed cleanup. Its owner is retained; no replacement was started.");
                return Ok(report);
            }
            self.codex_facts
                .lock()
                .map_err(|_| "Canary ownership information is unavailable.")?
                .canary = None;
        }
        // Retry only the SAME retained cleanup. Expiry does not free its budget.
        let retained = self
            .codex_facts
            .lock()
            .map_err(|_| "Agent ownership information is unavailable.")?
            .retained_probe
            .take();
        if let Some(mut prior) = retained {
            prior.retry_cleanup(&state.store);
            if prior.cleanup_retained() {
                self.project_codex_probe(&mut report, prior.summary());
                report.issue("The previous version probe has unconfirmed cleanup. Its budget remains held; no replacement was started.");
                self.codex_facts
                    .lock()
                    .map_err(|_| "Agent ownership information is unavailable.")?
                    .retained_probe = Some(prior);
                return Ok(report);
            }
        }
        let candidate = match discover_codex() {
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
        match prepare_box(&state.store, &state.location, CODEX) {
            Ok(home) => {
                report.home.state = StepState::Ready;
                self.codex_facts
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
        check_directory_budget(&state.location, "codex-probe-")?;
        let prepared = match PreparedCodexProbe::prepare(&candidate, &state.location) {
            Ok(prepared) => prepared,
            Err(issue) => {
                if let Ok(mut failure) = self.last_start_failure.lock() {
                    *failure = Some(if issue == ProbeIssue::Expired {
                        "Codex executable preparation timed out before starting the version process."
                    } else {
                        probe_issue(issue)
                    }.to_owned());
                }
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
                if let Ok(mut failure) = self.last_start_failure.lock() {
                    *failure = Some(
                        if issue == ProbeIssue::Expired {
                            "The prepared Codex version check expired before process startup."
                        } else {
                            probe_issue(issue)
                        }
                        .to_owned(),
                    );
                }
                report.issue(probe_issue(issue));
                return Ok(report);
            }
        };
        self.project_codex_probe(&mut report, outcome.summary());
        if let Ok(mut facts) = self.codex_facts.lock() {
            facts.last_probe = Some(outcome.summary().clone());
        }
        let observation = {
            let mut facts = self
                .codex_facts
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
                let observation = CodexObservation {
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
            match self.run_codex_canary(&state.location, &state.store, &observation) {
                Ok(receipt) => {
                    self.codex_facts
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
                Err(message) => {
                    if let Ok(mut failure) = self.last_start_failure.lock() {
                        if failure.is_none() {
                            *failure = Some(message.to_owned());
                        }
                    }
                    report.issue(message);
                }
            }
        }
        // Program observation + prepared directories are not a TUI qualification.
        report.issue("Opening and sign-in remain unavailable until the exact build passes native qualification.");
        Ok(report)
    }
    fn qualify_codex_for_open(&self) -> Result<(), &'static str> {
        let report = self.check_codex().map_err(|message| {
            if let Ok(mut failure) = self.last_start_failure.lock() {
                *failure = Some(message.to_owned());
            }
            "This session could not be prepared. Try Open again."
        })?;
        if report.can_open {
            return Ok(());
        }
        if matches!(report.program.state, ProgramState::Missing) {
            return Err("Codex is not installed. Install it and try Open again.");
        }
        let resource_refused = self.codex_facts.lock().ok().is_some_and(|facts| {
            facts
                .last_probe
                .as_ref()
                .is_some_and(|probe| probe.issue == Some(ProbeIssue::AdmissionRefused))
        }) || report.issues.iter().any(|issue| {
            issue.starts_with("Resources are not ready")
                || *issue == probe_issue(ProbeIssue::AdmissionRefused)
        });
        if resource_refused {
            return Err("Windows is low on resources. Close an unused app and try Open again.");
        }
        Err("Codex did not finish starting. Try Open again.")
    }
    pub(super) fn open_codex(
        &self,
        host: &TerminalHost,
        project: &std::path::Path,
        rows: u16,
        cols: u16,
    ) -> Result<AgentStarted, &'static str> {
        // Validate the chosen project before preparing any process. No fallback
        // to an ambient directory or to a different harness is permitted.
        self.services
            .load()
            .map_err(|_| "Saved projects are unavailable.")?
            .location
            .validate_project(project)
            .map_err(|_| "Choose an existing project folder and try Open again.")?;
        self.ensure_sidecar()?;
        let current_candidate = discover_codex().ok();
        let reusable = {
            let facts = self
                .codex_facts
                .lock()
                .map_err(|_| "This session is still finishing another action.")?;
            facts.canary.is_none()
                && facts.retained_probe.is_none()
                && facts.receipt.is_some()
                && facts
                    .observed
                    .as_ref()
                    .zip(current_candidate.as_ref())
                    .is_some_and(|(observed, current)| {
                        observed.candidate.path == current.path
                            && observed.candidate.kind == current.kind
                    })
        };
        if !reusable {
            self.qualify_codex_for_open()?;
        }
        self.start_codex(host, project, rows, cols, true)
            .map_err(|message| {
                if let Ok(mut failure) = self.last_start_failure.lock() {
                    if failure.is_none() {
                        *failure = Some(message.to_owned());
                    }
                }
                if message.starts_with("Resources are not ready") {
                    "Windows is low on resources. Close an unused app and try Open again."
                } else {
                    "Codex did not finish starting. Try Open again."
                }
            })
    }
    pub(super) fn start_codex(
        &self,
        host: &TerminalHost,
        project: &std::path::Path,
        rows: u16,
        cols: u16,
        refresh_stale: bool,
    ) -> Result<AgentStarted, &'static str> {
        let (observation, receipt) = {
            let facts = self
                .codex_facts
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
        let home = prepare_box(&state.store, &state.location, CODEX)?;
        if home.record != observation.root {
            if refresh_stale {
                self.qualify_codex_for_open()?;
                return self.start_codex(host, project, rows, cols, false);
            }
            return Err(
                "The saved agent home changed after its check. Check the agent again before opening it.",
            );
        }
        let (sidecar, port, ca) = self.wait_for_qualified_sidecar()?;
        let selected_set = empty_set()?;
        let thread_id = new_id(CanonicalIdKind::Thread);
        let prepared = match prepare_start(
            CodexStartContext {
                candidate: &observation.candidate,
                observation: &observation.outcome,
                base: &state.location,
                database: &state.store,
                private_home: home._grant,
                root: home.record,
                project,
                sidecar,
                proxy_port: port,
                ca_file: &ca,
                selected_set: &selected_set,
                thread_id: &thread_id,
            },
            &receipt,
        ) {
            Ok(prepared) => prepared,
            Err(issue)
                if refresh_stale
                    && matches!(
                        issue,
                        CodexLaunchError::ProgramChanged
                            | CodexLaunchError::RootChanged
                            | CodexLaunchError::ObservationRequired
                            | CodexLaunchError::QualificationRequired
                            | CodexLaunchError::InvalidCandidate
                            | CodexLaunchError::UnsafeProgram
                    ) =>
            {
                self.qualify_codex_for_open()?;
                return self.start_codex(host, project, rows, cols, false);
            }
            Err(issue) => {
                if let Ok(mut failure) = self.last_start_failure.lock() {
                    *failure = Some(format!("Codex startup preparation: {issue}"));
                }
                return Err(
                    "Codex startup no longer matches its qualified build, home or resource profile. Check the agent again.",
                );
            }
        };
        self.wait_for_qualified_sidecar()?;
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
                title: Some("Codex".to_owned()),
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
    fn run_codex_canary(
        &self,
        base: &StorageLocation,
        database: &Database,
        observation: &CodexObservation,
    ) -> Result<NativeTrialReceipt, &'static str> {
        database
            .check_write_admission()
            .map_err(|_| "Storage admission changed; no TUI check was started.")?;
        let (sidecar, port, ca) = self.wait_for_qualified_sidecar()?;
        check_directory_budget(base, "codex-tui-trial-")?;
        let (trial, scratch) = check_scratch::create(base, ScratchKind::CodexTuiTrial)
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
            AgentStoreKey::new(CODEX).map_err(|_| "The check agent namespace is invalid.")?,
            private_store_request(CODEX),
        )
        .and_then(|plan| plan.prepare())
        .map_err(|_| "A fresh private home could not be prepared for the TUI check.")?;
        let selected_set = empty_set()?;
        let thread_id = new_id(CanonicalIdKind::Thread);
        let prepared = prepare_trial(CodexTrialContext {
            candidate: &observation.candidate, observation: &observation.outcome, base, trial_base: &trial,
            private_home: grant, project: project.directory(), sidecar, proxy_port: port, ca_file: &ca,
            selected_set: &selected_set, thread_id: &thread_id,
        }).map_err(|issue| {
            if let Ok(mut failure) = self.last_start_failure.lock() {
                *failure = Some(format!("Codex TUI preparation: {issue}"));
            }
            "The exact Codex build could not prepare its isolated TUI check. Opening remains unavailable."
        })?;
        self.wait_for_qualified_sidecar()?;
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
        self.codex_facts
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
                    self.codex_facts
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
            self.codex_facts
                .lock()
                .map_err(|_| "Canary ownership information is unavailable.")?
                .canary = None;
        }
        receipt.ok_or(if released { "The TUI check did not produce a qualified startup receipt. No provider message was sent." }
            else { "TUI-check cleanup is unconfirmed. Its budget and owner remain retained; opening is unavailable." })
    }
    fn project_codex_probe(&self, report: &mut AgentCheckup, summary: &CodexProbeSummary) {
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
            "Codex was not found in the bounded local installation catalog. No installation was attempted."
        }
        ProbeIssue::Ambiguous => {
            "Multiple Codex installations were found. Choose one explicitly before native qualification."
        }
        ProbeIssue::WrapperUnresolved => {
            "Only a launcher wrapper was found. No shell fallback was run."
        }
        ProbeIssue::Busy => {
            "A previous Codex version check still owns its slot. No duplicate was started."
        }
        ProbeIssue::UnsafeProgram | ProbeIssue::InvalidCandidate | ProbeIssue::ProgramTooLarge => {
            "The Codex executable could not be safely pinned and verified."
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
