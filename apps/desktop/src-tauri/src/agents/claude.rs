//! Claude recipes and lifecycle binding. Shared Service owns discovery flow,
//! qualification, retained resources, persistence and native startup.
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
    claude_launch::{
        ClaudeLaunchError, ClaudeSessionOperation, ClaudeStartContext, ClaudeTrialContext,
        PreparedClaudeStart, prepare_tracked_session, prepare_tracked_trial,
    },
    claude_probe::{
        ClaudeProbeOutcome, PreparedClaudeProbe, ProbeIssue, discover_claude, probe_budget,
    },
    native_launch::NativeTrialReceipt,
    resources::admission::PreparedWorkload,
};
use shinatama_state::{database::Database, store_roots::PrivateStoreRequest};
use shinatama_terminal::WindowsNativeSessionLifecycle;
use std::sync::Arc;

pub(super) struct ClaudeDriver;
pub(super) struct PreparedClaudeSession {
    inner: PreparedClaudeStart,
    pending: Option<PendingThread>,
    bound: bool,
}
impl From<PreparedClaudeSession> for SealedSession {
    fn from(prepared: PreparedClaudeSession) -> Self {
        assert!(
            prepared.bound,
            "Claude lifecycle must be bound before startup."
        );
        Self {
            launch: prepared.inner.launch,
            permit: prepared.inner.permit,
        }
    }
}
impl ProbeEvidence for ClaudeProbeOutcome {
    fn projection(&self) -> ProbeProjection {
        let summary = self.summary();
        ProbeProjection {
            version: summary.version.as_ref().map(ToString::to_string),
            issue: summary.issue.map(probe_error),
            diagnostics: serde_json::to_value(summary).unwrap_or(serde_json::Value::Null),
        }
    }
    fn cleanup_retained(&self) -> bool {
        ClaudeProbeOutcome::cleanup_retained(self)
    }
    fn retry_cleanup(&mut self, database: &Database) {
        ClaudeProbeOutcome::retry_cleanup(self, database);
    }
}
impl HarnessDriver for ClaudeDriver {
    type Probe = ClaudeProbeOutcome;
    type PreparedSession = PreparedClaudeSession;

    fn descriptor(&self) -> HarnessDescriptor {
        HarnessDescriptor {
            id: "claude_code",
            label: "Claude Code",
            native_resume: true,
        }
    }
    fn policy(&self) -> HarnessPolicy {
        HarnessPolicy {
            probe_prefix: "claude-probe-",
            trial_prefix: "claude-tui-trial-",
            trial_kind: ScratchKind::ClaudeTuiTrial,
            probe_budget: probe_budget(),
        }
    }
    fn private_store_request(&self) -> PrivateStoreRequest {
        PrivateStoreRequest::default()
    }
    fn discover(&self) -> Result<ProgramCandidate, DriverError> {
        discover_claude().map_err(probe_error)
    }
    fn probe(
        &self,
        context: ProbeContext<'_>,
        admit: &mut dyn FnMut() -> Result<PreparedWorkload, DriverError>,
    ) -> Result<Self::Probe, DriverError> {
        let prepared =
            PreparedClaudeProbe::prepare(context.candidate, context.base).map_err(probe_error)?;
        let mut admission_error = None;
        prepared
            .run_with(context.database, || {
                admit().map_err(|error| {
                    admission_error = Some(error);
                    ProbeIssue::AdmissionRefused
                })
            })
            .map_err(|error| admission_error.unwrap_or_else(|| probe_error(error)))
    }
    fn prepare_trial(
        &self,
        context: TrialContext<'_, Self::Probe>,
    ) -> Result<PreparedTrial, DriverError> {
        let hook = hook_executable().map_err(DriverError::from)?;
        let prepared = prepare_tracked_trial(
            ClaudeTrialContext {
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
                let selection =
                    ClaudeSessionOperation::start_new(&uuid::Uuid::new_v4().to_string())
                        .map_err(launch_error)?;
                let pending = PendingThread::Start {
                    thread: context.thread.clone(),
                    root: context.root.clone(),
                    native_id: selection.native_id().into(),
                };
                (selection, pending)
            }
            SessionOperation::Resume(target) => {
                if target.thread != *context.thread
                    || target.root != context.root
                    || target.recorded_activation.is_some()
                    || target.segment.harness != "claude_code"
                {
                    return Err(DriverError::Failed(
                        "The saved conversation changed. Reload threads and try again.",
                    ));
                }
                let selection =
                    ClaudeSessionOperation::resume_native(&target.segment.native.native_id)
                        .map_err(launch_error)?;
                (selection, PendingThread::Resume(target.clone()))
            }
        };
        let hook = hook_executable().map_err(DriverError::from)?;
        let inner = prepare_tracked_session(
            ClaudeStartContext {
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
                thread_id: &context.thread.id,
            },
            receipt,
            &selection,
            &hook,
        )
        .map_err(launch_error)?;
        Ok(PreparedClaudeSession {
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
            .ok_or(DriverError::Failed("This Claude launch was already bound."))?;
        let matches = match (&pending, binding.operation) {
            (PendingThread::Start { thread, root, .. }, SessionOperation::New) => {
                thread == binding.thread && root == binding.root
            }
            (PendingThread::Resume(expected), SessionOperation::Resume(actual)) => {
                expected == actual
                    && expected.thread == *binding.thread
                    && expected.root == *binding.root
            }
            _ => false,
        };
        if !matches {
            return Err(DriverError::Failed(
                "The Claude thread binding changed before startup.",
            ));
        }
        let receiver = prepared
            .inner
            .session_events
            .take()
            .ok_or(DriverError::Failed(
                "Claude session tracking is unavailable.",
            ))?;
        let lifecycle = ThreadLifecycle::new(
            binding.database,
            pending,
            prepared.inner.launch.identity_record().clone(),
            receiver,
        )
        .map_err(DriverError::from)?;
        prepared.bound = true;
        Ok(Some(lifecycle))
    }
}
fn probe_error(error: ProbeIssue) -> DriverError {
    match error {
        ProbeIssue::NotFound => DriverError::Missing,
        ProbeIssue::AdmissionRefused => DriverError::AdmissionDeferred,
        ProbeIssue::Busy | ProbeIssue::CleanupUnconfirmed => {
            DriverError::Failed("Claude cleanup is still finishing. Retry after cleanup.")
        }
        ProbeIssue::Ambiguous => DriverError::Failed(
            "Multiple Claude installations were found. Choose one before opening a session.",
        ),
        ProbeIssue::WrapperUnresolved => {
            DriverError::Failed("The Claude executable could not be resolved from its launcher.")
        }
        ProbeIssue::UnsafeProgram | ProbeIssue::InvalidCandidate | ProbeIssue::ProgramTooLarge => {
            DriverError::Failed("The Claude executable could not be verified.")
        }
        ProbeIssue::PrivateRootUnavailable => {
            DriverError::Failed("Claude's private version-check folder is unavailable.")
        }
        ProbeIssue::Expired => {
            DriverError::Failed("Claude's version check timed out. Retry after cleanup.")
        }
        ProbeIssue::SpawnFailed => {
            DriverError::Failed("Claude's contained version check could not start.")
        }
        ProbeIssue::OutputLimit => {
            DriverError::Failed("Claude's version check exceeded its output limit.")
        }
        ProbeIssue::InvalidOutput | ProbeIssue::NonzeroExit => {
            DriverError::Failed("Claude did not return a valid successful version.")
        }
    }
}
fn launch_error(error: ClaudeLaunchError) -> DriverError {
    match error {
        ClaudeLaunchError::ProgramChanged
        | ClaudeLaunchError::RootChanged
        | ClaudeLaunchError::ObservationRequired
        | ClaudeLaunchError::QualificationRequired
        | ClaudeLaunchError::InvalidCandidate
        | ClaudeLaunchError::UnsafeProgram => DriverError::StaleQualification,
        ClaudeLaunchError::InvalidNativeSession => {
            DriverError::Failed("The saved Claude conversation identifier is invalid.")
        }
        _ => DriverError::Failed(
            "Claude startup no longer matches its qualified configuration. Try Open again.",
        ),
    }
}
