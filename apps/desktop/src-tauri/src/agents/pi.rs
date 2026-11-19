//! Pi supplies its typed engine recipes to the shared harness service.
//! Pi runs as `node.exe cli.js`: the npm shim is recognized, never executed.
use super::{
    notify_thread_changed,
    service::{
        BindingContext, DriverError, HarnessDescriptor, HarnessDriver, HarnessPolicy, PreparedTrial,
        ProbeContext, ProbeEvidence, ProbeProjection, SealedSession, SessionContext,
        SessionOperation, TrialContext,
    },
    thread_lifecycle::record_chosen_native_session,
};
use shinatama_adapter::{catalog::ProgramCandidate, launch::placement::PathRole};
use shinatama_engine::{
    check_scratch::ScratchKind,
    launch::LaunchIssueCode,
    native_launch::NativeTrialReceipt,
    pi_launch::{
        PiLaunchError, PiStartContext, PiTrialContext, PreparedPiStart, new_session_id,
        prepare_start, prepare_trial,
    },
    pi_probe::{PiProbeIssue, PiProbeOutcome, PreparedPiProbe, discover_pi, probe_budget},
    resources::admission::PreparedWorkload,
};
use shinatama_state::store_roots::StorePathRole;
use shinatama_state::{database::Database, store_roots::PrivateStoreRequest};
use shinatama_terminal::WindowsNativeSessionLifecycle;
use std::sync::Arc;

pub(super) struct PiDriver;
/// A start and the session id it was given: Pi reports no session events, so the id the launcher
/// chose is recorded for the thread when the launch is bound.
pub(super) struct PreparedPiSession {
    inner: PreparedPiStart,
    native_id: String,
}

impl HarnessDriver for PiDriver {
    type Probe = PiProbeOutcome;
    type PreparedSession = PreparedPiSession;

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

    fn bundled_plugins(&self) -> bool {
        true
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
        context: TrialContext<'_, Self::Probe>,
    ) -> Result<PreparedTrial, DriverError> {
        let prepared = prepare_trial(PiTrialContext {
            candidate: context.candidate,
            observation: context.observation,
            base: context.base,
            trial_base: context.trial_base,
            private_home: context.private_home,
            project: context.project,
            sidecar: context.sidecar,
            proxy_port: context.proxy_port,
            ca_file: context.ca_file,
            selected_set: context.selected_set,
            thread_id: context.thread_id,
        })
        .map_err(launch_error)?;
        Ok(PreparedTrial {
            launch: prepared.launch,
            permit: prepared.permit,
            tracker: prepared.tracker,
            keep_alive: None,
        })
    }

    fn prepare_session(
        &self,
        context: SessionContext<'_, Self::Probe>,
        receipt: &NativeTrialReceipt,
        operation: &SessionOperation,
    ) -> Result<Self::PreparedSession, DriverError> {
        if !matches!(operation, SessionOperation::New) {
            return Err(DriverError::Failed(
                "Native resume has not been qualified for Pi.",
            ));
        }
        let native_id = new_session_id();
        let inner = prepare_start(
            PiStartContext {
                candidate: context.candidate,
                observation: context.observation,
                base: context.base,
                database: context.database,
                private_home: context.private_home,
                root: context.root,
                project: context.project,
                sidecar: context.sidecar,
                proxy_port: context.proxy_port,
                ca_file: context.ca_file,
                selected_set: context.selected_set,
                plugins_root: context.plugins_root,
                thread_id: &context.thread.id,
                chat: context.chat.as_ref(),
                native_id: &native_id,
                model: context.model,
                initial_prompt: context.initial_prompt,
            },
            receipt,
        )
        .map_err(launch_error)?;
        Ok(PreparedPiSession { inner, native_id })
    }
    /// Returns no lifecycle: Pi has no qualified native thread binding, hook or resume; a
    /// successful startup proves none of them. It only records the session id the launch chose, so
    /// the tab's session can be tied to its member in the thread's room. Failing to record it
    /// leaves an ordinary session (a chat failure never prevents a session from opening).
    fn bind_lifecycle(
        &self,
        prepared: &mut Self::PreparedSession,
        binding: BindingContext<'_>,
    ) -> Result<Option<Arc<dyn WindowsNativeSessionLifecycle>>, DriverError> {
        match record_chosen_native_session(&binding.database, binding.thread, binding.root, &prepared.native_id) {
            Ok(()) => notify_thread_changed(&binding.thread_observer, &binding.thread.project_id, &binding.thread.id),
            Err(reason) => eprintln!("This Pi session is not recorded under its id: {reason}"),
        }
        Ok(None)
    }
}

impl From<PreparedPiSession> for SealedSession {
    fn from(prepared: PreparedPiSession) -> Self {
        Self {
            launch: prepared.inner.launch,
            permit: prepared.inner.permit,
        }
    }
}

impl ProbeEvidence for PiProbeOutcome {
    fn projection(&self) -> ProbeProjection {
        let summary = self.summary();
        ProbeProjection {
            version: summary.version.as_ref().map(ToString::to_string),
            issue: summary.issue.map(probe_error),
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

fn launch_error(issue: PiLaunchError) -> DriverError {
    match issue {
        PiLaunchError::ProgramChanged
        | PiLaunchError::RootChanged
        | PiLaunchError::ObservationRequired
        | PiLaunchError::QualificationRequired
        | PiLaunchError::InvalidCandidate
        | PiLaunchError::Installation(_) => DriverError::StaleQualification,
        PiLaunchError::Expired => {
            DriverError::Failed("Pi launch preparation reached its fixed time budget.")
        }
        PiLaunchError::PrivateHome => {
            DriverError::Failed("The Pi private home could not be prepared or pinned.")
        }
        PiLaunchError::InvalidProject => {
            DriverError::Failed("The selected project cannot be used for Pi.")
        }
        PiLaunchError::SidecarUnavailable => DriverError::Failed(crate::sidecar::NOT_BACKING),
        PiLaunchError::InvalidCa => {
            DriverError::Failed("The local session service certificate could not be pinned for Pi.")
        }
        PiLaunchError::UnsupportedSet => {
            DriverError::Failed("This Pi launch profile requires the empty optional set.")
        }
        PiLaunchError::BundledPlugins => {
            DriverError::Failed("The bundled plugins could not be verified for Pi.")
        }
        PiLaunchError::Configuration => {
            DriverError::Failed("The private Pi launch configuration could not be prepared.")
        }
        PiLaunchError::Identity => {
            DriverError::Failed("The Pi launch identity could not be prepared.")
        }
        PiLaunchError::InvalidNativeSession => {
            DriverError::Failed("The session id chosen for Pi is not a valid identifier.")
        }
        PiLaunchError::InvalidFirstMessage => DriverError::Failed(
            "The first message for Pi is empty, over 8192 bytes or holds a NUL.",
        ),
        PiLaunchError::Model => {
            DriverError::Failed("The model chosen for Pi is no longer in the Models panel.")
        }
        PiLaunchError::Permit => {
            DriverError::Failed("The Pi native launch permit did not match its qualification.")
        }
        PiLaunchError::InheritedEnvironment => {
            DriverError::Failed("Pi's bounded inherited environment is unavailable.")
        }
        PiLaunchError::Descriptor | PiLaunchError::Evidence => {
            DriverError::Failed("The Pi launch recipe evidence is invalid.")
        }
        PiLaunchError::PlacementMode | PiLaunchError::PlacementRecipe(_) => {
            DriverError::Failed("The Pi system-program and private-home placement is invalid.")
        }
        PiLaunchError::MissingPrivatePath(role) => DriverError::Failed(match role {
            PathRole::NodeCompileCache => "Pi is missing its private Node compile-cache path.",
            PathRole::Temporary => "Pi is missing its private temporary path.",
            _ => "Pi is missing its private agent directory.",
        }),
        PiLaunchError::ArgumentsRecipe(_) => {
            DriverError::Failed("The Pi node-and-script argument recipe was rejected.")
        }
        PiLaunchError::Composition(issue) => DriverError::Failed(match issue {
            LaunchIssueCode::PlacementRejected => "Pi private-path placement was rejected.",
            LaunchIssueCode::ArgumentsRejected => "Pi node-and-script arguments were rejected.",
            LaunchIssueCode::EnvironmentRejected => "Pi child environment was rejected.",
            LaunchIssueCode::IdentityRejected => "Pi launch identity was rejected.",
            LaunchIssueCode::SecretOutsideIdentityEnvironment => {
                "Pi identity data appeared outside its protected environment field."
            }
            _ => "Pi launch composition rejected its fixed recipe.",
        }),
    }
}
