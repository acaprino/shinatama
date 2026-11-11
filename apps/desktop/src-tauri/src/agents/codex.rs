//! Codex supplies its typed engine recipes to the shared harness service.
use super::{
    hook_executable,
    service::{
        BindingContext, DriverError, HarnessDescriptor, HarnessDriver, HarnessPolicy,
        PreparedTrial, ProbeContext, ProbeEvidence, ProbeProjection, SealedSession, SessionContext,
        SessionOperation, TrialContext,
    },
    thread_lifecycle::{PendingThread, ThreadLifecycle},
};
use shinatama_adapter::catalog::ProgramCandidate;
use shinatama_engine::{
    check_scratch::ScratchKind,
    codex_launch::{
        CodexLaunchError, CodexSessionOperation, CodexStartContext, CodexTrialContext,
        PreparedCodexStart, prepare_tracked_session, prepare_tracked_trial,
    },
    codex_probe::{
        CodexProbeOutcome, PreparedCodexProbe, ProbeIssue, discover_codex, probe_budget,
    },
    native_launch::NativeTrialReceipt,
    resources::admission::PreparedWorkload,
};
use shinatama_state::{database::Database, store_roots::PrivateStoreRequest};
use shinatama_terminal::WindowsNativeSessionLifecycle;
use std::sync::Arc;

pub(super) struct CodexDriver;
pub(super) struct PreparedCodexSession {
    inner: PreparedCodexStart,
    pending: Option<PendingThread>,
    bound: bool,
}

impl HarnessDriver for CodexDriver {
    type Probe = CodexProbeOutcome;
    type PreparedSession = PreparedCodexSession;

    fn descriptor(&self) -> HarnessDescriptor {
        HarnessDescriptor {
            id: "codex",
            label: "Codex",
            native_resume: true,
        }
    }

    fn policy(&self) -> HarnessPolicy {
        HarnessPolicy {
            probe_prefix: "codex-probe-",
            trial_prefix: "codex-tui-trial-",
            trial_kind: ScratchKind::CodexTuiTrial,
            probe_budget: probe_budget(),
        }
    }

    fn bundled_plugins(&self) -> bool {
        true
    }
    fn private_store_request(&self) -> PrivateStoreRequest {
        PrivateStoreRequest {
            home_path_limit: Some(shinatama_engine::codex_launch::home_path_limit()),
            ..PrivateStoreRequest::default()
        }
    }

    fn discover(&self) -> Result<ProgramCandidate, DriverError> {
        discover_codex().map_err(probe_error)
    }

    fn probe(
        &self,
        context: ProbeContext<'_>,
        admit: &mut dyn FnMut() -> Result<PreparedWorkload, DriverError>,
    ) -> Result<Self::Probe, DriverError> {
        let prepared =
            PreparedCodexProbe::prepare(context.candidate, context.base).map_err(probe_error)?;
        // The service owns admission. Preserve its classified error while the
        // engine keeps the actual process, reservation and cleanup outcome.
        let mut admission_error = None;
        prepared
            .run_with(context.database, || {
                admit().map_err(|error| {
                    admission_error = Some(error);
                    ProbeIssue::AdmissionRefused
                })
            })
            .map_err(|issue| admission_error.unwrap_or_else(|| probe_error(issue)))
    }

    fn prepare_trial(
        &self,
        context: TrialContext<'_, Self::Probe>,
    ) -> Result<PreparedTrial, DriverError> {
        let hook = hook_executable().map_err(DriverError::from)?;
        let prepared = prepare_tracked_trial(
            CodexTrialContext {
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
            },
            &hook,
        )
        .map_err(launch_error)?;
        Ok(PreparedTrial {
            launch: prepared.launch,
            permit: prepared.permit,
            tracker: prepared.tracker,
            keep_alive: prepared
                .session_events
                .map(|receiver| Box::new(receiver) as Box<dyn Send>),
        })
    }

    fn prepare_session(
        &self,
        context: SessionContext<'_, Self::Probe>,
        receipt: &NativeTrialReceipt,
        operation: &SessionOperation,
    ) -> Result<Self::PreparedSession, DriverError> {
        let (selection, pending) = match operation {
            SessionOperation::New => {
                let pending = PendingThread::Start {
                    thread: context.thread.clone(),
                    root: context.root.clone(),
                    native_id: None,
                };
                (CodexSessionOperation::start_new(), pending)
            }
            SessionOperation::Resume(target) => {
                if target.thread != *context.thread
                    || target.root != context.root
                    || target.recorded_activation.is_some()
                    || target.segment.harness != "codex"
                {
                    return Err(DriverError::Failed(
                        "The saved conversation changed. Reload threads and try again.",
                    ));
                }
                let selection =
                    CodexSessionOperation::resume_native(&target.segment.native.native_id)
                        .map_err(launch_error)?;
                (selection, PendingThread::Resume(target.clone()))
            }
        };
        let hook = hook_executable().map_err(DriverError::from)?;
        let inner = prepare_tracked_session(
            CodexStartContext {
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
            },
            receipt,
            &selection,
            &hook,
        )
        .map_err(launch_error)?;
        Ok(PreparedCodexSession {
            inner,
            pending: Some(pending),
            bound: false,
        })
    }
    fn bind_lifecycle(
        &self,
        prepared: &mut Self::PreparedSession,
        binding: BindingContext<'_>,
    ) -> Result<Option<Arc<dyn WindowsNativeSessionLifecycle>>, DriverError> {
        let pending = prepared
            .pending
            .take()
            .ok_or(DriverError::Failed("This Codex launch was already bound."))?;
        let matches = match (&pending, binding.operation) {
            (
                PendingThread::Start {
                    thread,
                    root,
                    native_id: None,
                },
                SessionOperation::New,
            ) => thread == binding.thread && root == binding.root,
            (PendingThread::Resume(expected), SessionOperation::Resume(actual)) => {
                expected == actual
                    && expected.thread == *binding.thread
                    && expected.root == *binding.root
            }
            _ => false,
        };
        if !matches {
            return Err(DriverError::Failed(
                "The Codex thread binding changed before startup.",
            ));
        }
        let receiver = prepared
            .inner
            .session_events
            .take()
            .ok_or(DriverError::Failed(
                "Codex session tracking is unavailable.",
            ))?;
        let lifecycle = ThreadLifecycle::new(
            binding.database,
            pending,
            prepared.inner.launch.identity_record().clone(),
            receiver,
            binding.thread_observer,
        )
        .map_err(DriverError::from)?;
        prepared.bound = true;
        Ok(Some(lifecycle))
    }
}

impl From<PreparedCodexSession> for SealedSession {
    fn from(prepared: PreparedCodexSession) -> Self {
        assert!(
            prepared.bound,
            "Codex lifecycle must be bound before startup."
        );
        Self {
            launch: prepared.inner.launch,
            permit: prepared.inner.permit,
        }
    }
}

impl ProbeEvidence for CodexProbeOutcome {
    fn projection(&self) -> ProbeProjection {
        let summary = self.summary();
        ProbeProjection {
            version: summary.version.as_ref().map(ToString::to_string),
            issue: summary.issue.map(probe_error),
            // Preserve the existing exact diagnostic projection; its source
            // summary is already bounded and contains no private output.
            diagnostics: serde_json::to_value(summary).unwrap_or(serde_json::Value::Null),
        }
    }

    fn cleanup_retained(&self) -> bool {
        CodexProbeOutcome::cleanup_retained(self)
    }

    fn retry_cleanup(&mut self, database: &Database) {
        CodexProbeOutcome::retry_cleanup(self, database);
    }
}

fn probe_error(issue: ProbeIssue) -> DriverError {
    match issue {
        ProbeIssue::NotFound => DriverError::Missing,
        ProbeIssue::AdmissionRefused => DriverError::AdmissionDeferred,
        ProbeIssue::Ambiguous => DriverError::Failed(
            "Multiple Codex installations were found. Choose one explicitly before native qualification.",
        ),
        ProbeIssue::WrapperUnresolved => {
            DriverError::Failed("Only a launcher wrapper was found. No shell fallback was run.")
        }
        ProbeIssue::Busy => DriverError::Failed(
            "A previous Codex version check still owns its slot. No duplicate was started.",
        ),
        ProbeIssue::UnsafeProgram | ProbeIssue::InvalidCandidate | ProbeIssue::ProgramTooLarge => {
            DriverError::Failed("The Codex executable could not be safely pinned and verified.")
        }
        ProbeIssue::PrivateRootUnavailable => DriverError::Failed(
            "An empty private version-probe directory could not be prepared. No system home was used.",
        ),
        ProbeIssue::Expired => DriverError::Failed(
            "The version probe reached its fixed time budget. Retry explicitly after cleanup.",
        ),
        ProbeIssue::SpawnFailed => DriverError::Failed(
            "The contained version probe could not start. No unprotected fallback was used.",
        ),
        ProbeIssue::OutputLimit => {
            DriverError::Failed("The version probe exceeded its bounded output allowance.")
        }
        ProbeIssue::InvalidOutput | ProbeIssue::NonzeroExit => {
            DriverError::Failed("The version probe did not return a valid successful CLI version.")
        }
        ProbeIssue::CleanupUnconfirmed => DriverError::Failed(
            "Version-probe cleanup is unconfirmed. Its resource reservation remains held.",
        ),
    }
}

fn launch_error(issue: CodexLaunchError) -> DriverError {
    match issue {
        CodexLaunchError::ProgramChanged
        | CodexLaunchError::RootChanged
        | CodexLaunchError::ObservationRequired
        | CodexLaunchError::QualificationRequired
        | CodexLaunchError::InvalidCandidate
        | CodexLaunchError::UnsafeProgram => DriverError::StaleQualification,
        CodexLaunchError::Expired => {
            DriverError::Failed("Codex executable preparation reached its fixed time budget.")
        }
        CodexLaunchError::PrivateHome => {
            DriverError::Failed("The Codex private home could not be prepared or pinned.")
        }
        CodexLaunchError::InvalidProject => {
            DriverError::Failed("The selected project cannot be used for Codex.")
        }
        CodexLaunchError::SidecarUnavailable => {
            DriverError::Failed("The local session service changed during Codex preparation.")
        }
        CodexLaunchError::InvalidCa => DriverError::Failed(
            "The local session service certificate could not be pinned for Codex.",
        ),
        CodexLaunchError::BundledPlugins => {
            DriverError::Failed("The bundled plugins could not be verified for Codex.")
        }
        CodexLaunchError::UnsupportedSet => {
            DriverError::Failed("This Codex launch profile requires the empty optional set.")
        }
        CodexLaunchError::Configuration => {
            DriverError::Failed("The private Codex launch configuration could not be prepared.")
        }
        CodexLaunchError::InvalidNativeSession => {
            DriverError::Failed("The saved Codex conversation identifier is invalid.")
        }
        CodexLaunchError::Identity => {
            DriverError::Failed("The Codex launch identity could not be prepared.")
        }
        CodexLaunchError::Permit => {
            DriverError::Failed("The Codex native launch permit did not match its qualification.")
        }
        CodexLaunchError::Composition => {
            DriverError::Failed("Codex launch composition rejected its fixed recipe.")
        }
    }
}
