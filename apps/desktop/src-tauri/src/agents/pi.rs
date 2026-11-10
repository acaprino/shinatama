//! Pi supplies its typed engine recipes to the shared harness service.
//! Pi runs as `node.exe cli.js`: the npm shim is recognized, never executed.
use super::service::{
    DriverError, HarnessDescriptor, HarnessDriver, HarnessPolicy, PreparedTrial, ProbeContext,
    ProbeEvidence, ProbeProjection, SealedSession, SessionContext, SessionOperation, TrialContext,
};
use shinatama_adapter::catalog::ProgramCandidate;
use shinatama_engine::{
    check_scratch::ScratchKind,
    native_launch::NativeTrialReceipt,
    pi_probe::{PiProbeIssue, PiProbeOutcome, PreparedPiProbe, discover_pi, probe_budget},
    resources::admission::PreparedWorkload,
};
use shinatama_state::store_roots::StorePathRole;
use shinatama_state::{database::Database, store_roots::PrivateStoreRequest};

pub(super) struct PiDriver;

/// C18.1 observes Pi and its Node runtime; no launch recipe is qualified yet.
pub(super) enum NoPreparedSession {}
impl From<NoPreparedSession> for SealedSession {
    fn from(never: NoPreparedSession) -> Self {
        match never {}
    }
}
const LAUNCH_UNQUALIFIED: DriverError =
    DriverError::Failed("The Pi terminal launch is not qualified in this build.");

impl HarnessDriver for PiDriver {
    type Probe = PiProbeOutcome;
    type PreparedSession = NoPreparedSession;

    fn descriptor(&self) -> HarnessDescriptor {
        HarnessDescriptor {
            id: "pi",
            label: "Pi",
            native_resume: false,
        }
    }

    fn policy(&self) -> HarnessPolicy {
        HarnessPolicy {
            probe_prefix: "pi-probe-",
            trial_prefix: "pi-tui-trial-",
            trial_kind: ScratchKind::PiTuiTrial,
            probe_budget: probe_budget(),
        }
    }

    fn private_store_request(&self) -> PrivateStoreRequest {
        // PI_CODING_AGENT_DIR is the box home; Node gets a private compile cache.
        PrivateStoreRequest {
            extra_roles: vec![StorePathRole::NodeCompileCache],
            ..PrivateStoreRequest::default()
        }
    }

    fn discover(&self) -> Result<ProgramCandidate, DriverError> {
        discover_pi().map_err(probe_error)
    }

    fn probe(
        &self,
        context: ProbeContext<'_>,
        admit: &mut dyn FnMut() -> Result<PreparedWorkload, DriverError>,
    ) -> Result<Self::Probe, DriverError> {
        let prepared =
            PreparedPiProbe::prepare(context.candidate, context.base).map_err(probe_error)?;
        // Two contained runs (Node, then Pi) are each admitted by the service.
        let mut admission_error = None;
        prepared
            .run_with(context.database, || {
                admit().map_err(|error| {
                    admission_error = Some(error);
                    PiProbeIssue::AdmissionRefused
                })
            })
            .map_err(|issue| admission_error.unwrap_or_else(|| probe_error(issue)))
    }

    fn prepare_trial(
        &self,
        _context: TrialContext<'_, Self::Probe>,
    ) -> Result<PreparedTrial, DriverError> {
        Err(LAUNCH_UNQUALIFIED)
    }

    fn prepare_session(
        &self,
        _context: SessionContext<'_, Self::Probe>,
        _receipt: &NativeTrialReceipt,
        _operation: &SessionOperation,
    ) -> Result<Self::PreparedSession, DriverError> {
        Err(LAUNCH_UNQUALIFIED)
    }
}

impl ProbeEvidence for PiProbeOutcome {
    fn projection(&self) -> ProbeProjection {
        let summary = self.summary();
        ProbeProjection {
            version: summary.version.as_ref().map(ToString::to_string),
            // C18.1: a successful observation still has no qualified launch, so
            // the service never starts a TUI check (and leaves no trial scratch).
            issue: Some(summary.issue.map_or(LAUNCH_UNQUALIFIED, probe_error)),
            // Bounded hashes, sizes and allowlisted version lines; no paths.
            diagnostics: serde_json::to_value(summary).unwrap_or(serde_json::Value::Null),
        }
    }

    fn cleanup_retained(&self) -> bool {
        PiProbeOutcome::cleanup_retained(self)
    }

    fn retry_cleanup(&mut self, database: &Database) {
        PiProbeOutcome::retry_cleanup(self, database);
    }
}

fn probe_error(issue: PiProbeIssue) -> DriverError {
    match issue {
        PiProbeIssue::NotFound => DriverError::Missing,
        PiProbeIssue::AdmissionRefused => DriverError::AdmissionDeferred,
        PiProbeIssue::WrapperUnresolved => DriverError::Failed(
            "Pi's launcher is not the reviewed npm shim. No wrapper or shell was run.",
        ),
        PiProbeIssue::NodeMissing => {
            DriverError::Failed("Pi needs Node.js, but no node.exe was found beside Pi or on PATH.")
        }
        PiProbeIssue::NodeUnsupported => {
            DriverError::Failed("Pi requires Node.js 22.19.0 or newer. Update Node and try again.")
        }
        PiProbeIssue::Busy => DriverError::Failed(
            "A previous Pi version check still owns its slot. No duplicate was started.",
        ),
        PiProbeIssue::UnsafeProgram
        | PiProbeIssue::InvalidCandidate
        | PiProbeIssue::ProgramTooLarge => {
            DriverError::Failed("Node.js or the Pi script could not be safely pinned and verified.")
        }
        PiProbeIssue::PrivateRootUnavailable => DriverError::Failed(
            "An empty private version-probe directory could not be prepared. No system home was used.",
        ),
        PiProbeIssue::Expired => DriverError::Failed(
            "The version probe reached its fixed time budget. Retry explicitly after cleanup.",
        ),
        PiProbeIssue::SpawnFailed => DriverError::Failed(
            "The contained version probe could not start. No unprotected fallback was used.",
        ),
        PiProbeIssue::OutputLimit => {
            DriverError::Failed("The version probe exceeded its bounded output allowance.")
        }
        PiProbeIssue::InvalidOutput | PiProbeIssue::NonzeroExit => DriverError::Failed(
            "The version probe did not return a valid successful Node and Pi version.",
        ),
        PiProbeIssue::CleanupUnconfirmed => DriverError::Failed(
            "Version-probe cleanup is unconfirmed. Its resource reservation remains held.",
        ),
    }
}
