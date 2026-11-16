//! OpenCode supplies its typed engine recipes to the shared harness service.
use super::service::{
    DriverError, HarnessDescriptor, HarnessDriver, HarnessPolicy, PreparedTrial, ProbeContext,
    ProbeEvidence, ProbeProjection, SealedSession, SessionContext, SessionOperation, TrialContext,
};
use shinatama_adapter::catalog::ProgramCandidate;
use shinatama_adapter::launch::placement::PathRole;
use shinatama_engine::launch::LaunchIssueCode;
use shinatama_engine::{
    check_scratch::ScratchKind,
    native_launch::NativeTrialReceipt,
    opencode_launch::{
        OpenCodeLaunchError, OpenCodeStartContext, OpenCodeTrialContext, PreparedOpenCodeStart,
        prepare_start, prepare_trial,
    },
    opencode_probe::{
        OpenCodeProbeOutcome, PreparedOpenCodeProbe, ProbeIssue, discover_opencode, probe_budget,
    },
    resources::admission::PreparedWorkload,
};
use shinatama_state::store_roots::StorePathRole;
use shinatama_state::{database::Database, store_roots::PrivateStoreRequest};

pub(super) struct OpenCodeDriver;

impl HarnessDriver for OpenCodeDriver {
    type Probe = OpenCodeProbeOutcome;
    type PreparedSession = PreparedOpenCodeStart;

    fn descriptor(&self) -> HarnessDescriptor {
        HarnessDescriptor {
            id: "opencode",
            label: "OpenCode",
            native_resume: false,
        }
    }

    fn policy(&self) -> HarnessPolicy {
        HarnessPolicy {
            probe_prefix: "opencode-probe-",
            trial_prefix: "opencode-tui-trial-",
            trial_kind: ScratchKind::OpenCodeTuiTrial,
            probe_budget: probe_budget(),
        }
    }

    fn bundled_plugins(&self) -> bool {
        true
    }
    fn private_store_request(&self) -> PrivateStoreRequest {
        PrivateStoreRequest {
            extra_roles: vec![
                StorePathRole::OpenCodeDatabase,
                StorePathRole::XdgConfig,
                StorePathRole::XdgData,
                StorePathRole::XdgCache,
                StorePathRole::XdgState,
            ],
            ..PrivateStoreRequest::default()
        }
    }

    fn discover(&self) -> Result<ProgramCandidate, DriverError> {
        discover_opencode().map_err(probe_error)
    }

    fn probe(
        &self,
        context: ProbeContext<'_>,
        admit: &mut dyn FnMut() -> Result<PreparedWorkload, DriverError>,
    ) -> Result<Self::Probe, DriverError> {
        let prepared =
            PreparedOpenCodeProbe::prepare(context.candidate, context.base).map_err(probe_error)?;
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
        let prepared = prepare_trial(OpenCodeTrialContext {
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
                "Native resume has not been qualified for OpenCode.",
            ));
        }
        prepare_start(
            OpenCodeStartContext {
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
            },
            receipt,
        )
        .map_err(launch_error)
    }
    // The default bind_lifecycle returns None: native thread ownership and
    // resume are not qualified by a successful OpenCode startup alone.
}

impl From<PreparedOpenCodeStart> for SealedSession {
    fn from(prepared: PreparedOpenCodeStart) -> Self {
        Self {
            launch: prepared.launch,
            permit: prepared.permit,
        }
    }
}

impl ProbeEvidence for OpenCodeProbeOutcome {
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
        OpenCodeProbeOutcome::cleanup_retained(self)
    }

    fn retry_cleanup(&mut self, database: &Database) {
        OpenCodeProbeOutcome::retry_cleanup(self, database);
    }
}

fn probe_error(issue: ProbeIssue) -> DriverError {
    match issue {
        ProbeIssue::NotFound => DriverError::Missing,
        ProbeIssue::AdmissionRefused => DriverError::AdmissionDeferred,
        ProbeIssue::Ambiguous => DriverError::Failed(
            "Multiple OpenCode installations were found. Choose one explicitly before native qualification.",
        ),
        ProbeIssue::WrapperUnresolved => {
            DriverError::Failed("Only a launcher wrapper was found. No shell fallback was run.")
        }
        ProbeIssue::Busy => DriverError::Failed(
            "A previous OpenCode version check still owns its slot. No duplicate was started.",
        ),
        ProbeIssue::UnsafeProgram | ProbeIssue::InvalidCandidate | ProbeIssue::ProgramTooLarge => {
            DriverError::Failed("The OpenCode executable could not be safely pinned and verified.")
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

fn launch_error(issue: OpenCodeLaunchError) -> DriverError {
    match issue {
        OpenCodeLaunchError::ProgramChanged
        | OpenCodeLaunchError::RootChanged
        | OpenCodeLaunchError::ObservationRequired
        | OpenCodeLaunchError::QualificationRequired
        | OpenCodeLaunchError::InvalidCandidate
        | OpenCodeLaunchError::UnsafeProgram => DriverError::StaleQualification,
        OpenCodeLaunchError::Expired => {
            DriverError::Failed("OpenCode executable preparation reached its fixed time budget.")
        }
        OpenCodeLaunchError::PrivateHome => {
            DriverError::Failed("The OpenCode private home could not be prepared or pinned.")
        }
        OpenCodeLaunchError::InvalidProject => {
            DriverError::Failed("The selected project cannot be used for OpenCode.")
        }
        OpenCodeLaunchError::SidecarUnavailable => {
            DriverError::Failed("The local session service changed during OpenCode preparation.")
        }
        OpenCodeLaunchError::InvalidCa => DriverError::Failed(
            "The local session service certificate could not be pinned for OpenCode.",
        ),
        OpenCodeLaunchError::UnsupportedSet => {
            DriverError::Failed("This OpenCode launch profile requires the empty optional set.")
        }
        OpenCodeLaunchError::BundledPlugins => {
            DriverError::Failed("The bundled plugins could not be verified for OpenCode.")
        }
        OpenCodeLaunchError::Configuration => {
            DriverError::Failed("The private OpenCode launch configuration could not be prepared.")
        }
        OpenCodeLaunchError::Identity => {
            DriverError::Failed("The OpenCode launch identity could not be prepared.")
        }
        OpenCodeLaunchError::Permit => DriverError::Failed(
            "The OpenCode native launch permit did not match its qualification.",
        ),
        OpenCodeLaunchError::InheritedEnvironment => {
            DriverError::Failed("OpenCode's bounded inherited environment is unavailable.")
        }
        OpenCodeLaunchError::Descriptor => {
            DriverError::Failed("The OpenCode launch descriptor is invalid.")
        }
        OpenCodeLaunchError::Evidence => {
            DriverError::Failed("The OpenCode launch recipe evidence is invalid.")
        }
        OpenCodeLaunchError::PlacementMode => DriverError::Failed(
            "The OpenCode system-program and private-home placement is invalid.",
        ),
        OpenCodeLaunchError::MissingPrivatePath(role) => missing_private_path(role),
        OpenCodeLaunchError::PlacementRecipe(_) => {
            DriverError::Failed("The OpenCode private-home recipe was rejected.")
        }
        OpenCodeLaunchError::ArgumentsRecipe(_) => {
            DriverError::Failed("The OpenCode standalone argument recipe was rejected.")
        }
        OpenCodeLaunchError::Composition(issue) => DriverError::Failed(match issue {
            LaunchIssueCode::PlacementRejected => "OpenCode private-path placement was rejected.",
            LaunchIssueCode::ArgumentsRejected => "OpenCode standalone arguments were rejected.",
            LaunchIssueCode::EnvironmentRejected => "OpenCode child environment was rejected.",
            LaunchIssueCode::IdentityRejected => "OpenCode launch identity was rejected.",
            LaunchIssueCode::DatabasePathRejected => {
                "OpenCode application database path was rejected."
            }
            LaunchIssueCode::SecretOutsideIdentityEnvironment => {
                "OpenCode identity data appeared outside its protected environment field."
            }
            _ => "OpenCode launch composition rejected its fixed recipe.",
        }),
    }
}

fn missing_private_path(role: PathRole) -> DriverError {
    DriverError::Failed(match role {
        PathRole::OpenCodeDatabase => "OpenCode is missing its private database-file path.",
        PathRole::XdgConfig => "OpenCode is missing its private XDG configuration path.",
        PathRole::XdgData => "OpenCode is missing its private XDG data path.",
        PathRole::XdgCache => "OpenCode is missing its private XDG cache path.",
        PathRole::XdgState => "OpenCode is missing its private XDG state path.",
        PathRole::OpenCodeToolHome => "OpenCode is missing its private tool-home path.",
        _ => "OpenCode is missing a required private storage path.",
    })
}
