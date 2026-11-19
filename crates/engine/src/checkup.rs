//! Generic launch review. It consumes provider observations and delegates all
//! launch composition to C11; neither a ready checkbox nor a set enables native
//! execution. Daodan is intentionally absent from the prerequisite model.
use crate::{
    Engine,
    launch::{
        LaunchInputs, LaunchIntent, LaunchPreflight, LaunchStatus, LaunchSummary, PreparedLaunch,
    },
    sidecar_runtime::SidecarRuntimeRecipe,
};
use serde::Serialize;
use shinatama_adapter::{
    AdapterDescriptor, Capabilities, FixtureEvidence, HomePlacement, ObservedVersion, Operation,
    RefusalReason,
};
use shinatama_adapter::{
    catalog::{ProgramCandidate, ProgramKind},
    sets::{SetPlan, SetScope},
};
use shinatama_state::{
    records::{RootAccess, RootKind, StoreRootRecord},
    store_roots::PreparedPrivateStore,
};

pub enum ProgramReadiness<'a> {
    Missing,
    Candidate(&'a ProgramCandidate),
    Fixture(&'a FixtureEvidence),
}
pub enum HomeReadiness<'a> {
    Missing,
    /// Held owner-only grant, not a root registration or runtime qualification.
    Private(&'a PreparedPrivateStore),
    /// A validated database record does not replace a held filesystem binding.
    System(&'a StoreRootRecord),
    Fixture(&'a FixtureEvidence),
}
pub enum SidecarReadiness<'a> {
    Missing,
    Prepared(&'a SidecarRuntimeRecipe),
    Fixture(&'a FixtureEvidence),
}
/// Trusted provider values, not Deserialize/IPC data or user-supplied booleans.
pub struct CheckupReadiness<'a> {
    pub program: ProgramReadiness<'a>,
    pub home: HomeReadiness<'a>,
    pub sidecar: SidecarReadiness<'a>,
    pub sets: Option<&'a SetPlan<'a>>,
}
impl Default for CheckupReadiness<'_> {
    fn default() -> Self {
        Self {
            program: ProgramReadiness::Missing,
            home: HomeReadiness::Missing,
            sidecar: SidecarReadiness::Missing,
            sets: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckupStatus {
    SyntheticReady,
    Incomplete,
    Refused,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckupComponent {
    Program,
    Home,
    Sidecar,
    Set,
    Launch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    Missing,
    UnknownVersion,
    VersionMismatch,
    WrapperNeedsResolution,
    EvidenceRejected,
    PrivateHomeRequired,
    HomeRegistrationRequired,
    HeldFilesystemBindingRequired,
    SidecarQualificationRequired,
    SetSelectionRequired,
    SetLaunchMismatch,
    NativeAdapterQualificationRequired,
    LaunchIncomplete,
    LaunchRefused,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckupAction {
    InspectProgram,
    PrepareHome,
    QualifySidecar,
    SelectSet,
    QualifyAdapter,
    ReviewLaunch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CheckupIssue {
    pub component: CheckupComponent,
    pub reason: MissingReason,
    pub action: CheckupAction,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSummary {
    pub scope: SetScope,
    pub manifest_id: Option<String>,
    pub entries: usize,
    pub mounted_mcp: Vec<String>,
    /// Planning never implies that native configuration has been written/tested.
    pub materialized: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckupSummary {
    pub schema_version: u32,
    pub status: CheckupStatus,
    pub launch: LaunchSummary,
    pub issues: Vec<CheckupIssue>,
    pub selected_set: Option<SetSummary>,
    pub native_start_enabled: bool,
}
pub struct Checkup {
    summary: CheckupSummary,
    preflight: LaunchPreflight,
}
impl Checkup {
    pub fn summary(&self) -> &CheckupSummary {
        &self.summary
    }
    pub fn preflight(&self) -> &LaunchPreflight {
        &self.preflight
    }
    /// Only a fully prepared synthetic review may expose its non-executable plan.
    pub fn prepared(&self) -> Option<&PreparedLaunch> {
        (self.summary.status == CheckupStatus::SyntheticReady)
            .then(|| self.preflight.prepared())
            .flatten()
    }
}
impl std::fmt::Debug for Checkup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checkup")
            .field("summary", &self.summary)
            .finish_non_exhaustive()
    }
}

impl Engine {
    pub fn checkup(
        &self,
        intent: &LaunchIntent,
        inputs: Option<&LaunchInputs<'_>>,
        readiness: CheckupReadiness<'_>,
    ) -> Checkup {
        let preflight = self.preflight_launch(intent, inputs);
        let mut summary = CheckupSummary {
            schema_version: 1,
            status: CheckupStatus::Incomplete,
            launch: preflight.summary().clone(),
            issues: Vec::new(),
            selected_set: None,
            native_start_enabled: false,
        };
        let issue = |component, reason, action| CheckupIssue {
            component,
            reason,
            action,
        };
        let Some(descriptor) = self.descriptor(&intent.agent) else {
            summary.status = CheckupStatus::Refused;
            summary.issues.push(issue(
                CheckupComponent::Launch,
                MissingReason::LaunchRefused,
                CheckupAction::QualifyAdapter,
            ));
            return Checkup { summary, preflight };
        };
        let program = match readiness.program {
            ProgramReadiness::Missing => Some(MissingReason::Missing),
            ProgramReadiness::Fixture(evidence) => {
                fixture(descriptor, Operation::Detect, evidence).err()
            }
            ProgramReadiness::Candidate(candidate) => {
                if descriptor.synthetic || candidate.agent != descriptor.agent {
                    Some(MissingReason::EvidenceRejected)
                } else if candidate.kind != ProgramKind::DirectExe {
                    Some(MissingReason::WrapperNeedsResolution)
                } else {
                    match &candidate.version {
                        ObservedVersion::Unknown { .. } => Some(MissingReason::UnknownVersion),
                        ObservedVersion::Known { version } if version != &descriptor.version => {
                            Some(MissingReason::VersionMismatch)
                        }
                        ObservedVersion::Known { .. } => None,
                    }
                }
            }
        };
        if let Some(reason) = program {
            summary.issues.push(issue(
                CheckupComponent::Program,
                reason,
                CheckupAction::InspectProgram,
            ));
        }
        let home = match readiness.home {
            HomeReadiness::Missing => Some(MissingReason::Missing),
            HomeReadiness::Fixture(evidence) => {
                fixture(descriptor, Operation::Placement, evidence).err()
            }
            HomeReadiness::Private(grant) => {
                if intent.placement.home() != HomePlacement::Box
                    || descriptor.synthetic
                    || grant
                        .paths()
                        .root()
                        .file_name()
                        .is_none_or(|part| part != descriptor.agent.as_str())
                {
                    Some(MissingReason::PrivateHomeRequired)
                } else {
                    Some(MissingReason::HomeRegistrationRequired)
                }
            }
            HomeReadiness::System(root) => {
                if intent.placement.home() != HomePlacement::System
                    || descriptor.synthetic
                    || root.validate().is_err()
                    || root.harness != descriptor.agent.as_str()
                    || root.kind != RootKind::System
                    || root.access != RootAccess::ReadWrite
                    || !root.available
                {
                    Some(MissingReason::PrivateHomeRequired)
                } else {
                    Some(MissingReason::HeldFilesystemBindingRequired)
                }
            }
        };
        if let Some(reason) = home {
            summary.issues.push(issue(
                CheckupComponent::Home,
                reason,
                CheckupAction::PrepareHome,
            ));
        }
        let sidecar = match readiness.sidecar {
            SidecarReadiness::Missing => Some(MissingReason::Missing),
            SidecarReadiness::Fixture(evidence) => {
                fixture(descriptor, Operation::FixtureGate, evidence).err()
            }
            SidecarReadiness::Prepared(recipe) => {
                // C12 preparation is real filesystem/hash evidence, not a running
                // health check. Its current API never grants runtime readiness.
                let _prepared_static_summary = recipe.summary();
                Some(MissingReason::SidecarQualificationRequired)
            }
        };
        if let Some(reason) = sidecar {
            summary.issues.push(issue(
                CheckupComponent::Sidecar,
                reason,
                CheckupAction::QualifySidecar,
            ));
        }
        if let Some(set) = readiness.sets {
            summary.selected_set = Some(SetSummary {
                scope: set.scope(),
                manifest_id: set.manifest().map(|set| set.id().to_owned()),
                entries: set.manifest().map_or(0, |set| set.entries().len()),
                mounted_mcp: set
                    .mounted_mcp()
                    .map(|entry| entry.id().to_owned())
                    .collect(),
                materialized: false,
            });
            if set.agent() != &descriptor.agent
                || set.version() != &descriptor.version
                || preflight.prepared().is_none_or(|prepared| {
                    !set.matches_launch(
                        prepared.placement(),
                        prepared.command(),
                        prepared.environment(),
                        &prepared.identity_record().instance_id,
                    )
                })
            {
                summary.issues.push(issue(
                    CheckupComponent::Set,
                    MissingReason::SetLaunchMismatch,
                    CheckupAction::SelectSet,
                ));
            }
        } else {
            summary.issues.push(issue(
                CheckupComponent::Set,
                MissingReason::SetSelectionRequired,
                CheckupAction::SelectSet,
            ));
        }
        if !descriptor.synthetic {
            summary.issues.push(issue(
                CheckupComponent::Launch,
                MissingReason::NativeAdapterQualificationRequired,
                CheckupAction::QualifyAdapter,
            ));
        }
        match preflight.summary().status {
            LaunchStatus::Refused => {
                summary.status = CheckupStatus::Refused;
                summary.issues.push(issue(
                    CheckupComponent::Launch,
                    MissingReason::LaunchRefused,
                    CheckupAction::ReviewLaunch,
                ));
            }
            LaunchStatus::Incomplete => summary.issues.push(issue(
                CheckupComponent::Launch,
                MissingReason::LaunchIncomplete,
                CheckupAction::ReviewLaunch,
            )),
            LaunchStatus::Available if descriptor.synthetic && summary.issues.is_empty() => {
                summary.status = CheckupStatus::SyntheticReady
            }
            LaunchStatus::Available => {}
        }
        Checkup { summary, preflight }
    }
}

fn fixture(
    descriptor: &AdapterDescriptor,
    operation: Operation,
    evidence: &FixtureEvidence,
) -> Result<(), MissingReason> {
    // Fixture substitutes are exclusively synthetic. An installed-runtime fixture
    // still cannot replace a program/grant/sidecar observation for a native agent.
    if !descriptor.synthetic {
        return Err(MissingReason::EvidenceRejected);
    }
    let mut check = Capabilities::default();
    check
        .qualify(
            operation,
            &descriptor.agent,
            &descriptor.version,
            true,
            evidence.clone(),
        )
        .map_err(|reason| {
            if reason == RefusalReason::VersionMismatch {
                MissingReason::VersionMismatch
            } else {
                MissingReason::EvidenceRejected
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_adapter::{
        AgentAdapter, AgentId, ConnectionArchitecture, EvidenceKind, Placement, ProgramPlacement,
        Version,
    };
    struct Adapter(AdapterDescriptor);
    impl AgentAdapter for Adapter {
        fn descriptor(&self) -> &AdapterDescriptor {
            &self.0
        }
    }
    #[test]
    fn unknown_native_program_is_actionable_and_synthetic_evidence_cannot_open_native_gate() {
        let agent = AgentId::new("codex").unwrap();
        let version = Version::parse("1.0.0").unwrap();
        let mut capabilities = Capabilities::default();
        for operation in [
            Operation::StartNew,
            Operation::Placement,
            Operation::InheritedMarkers,
        ] {
            capabilities
                .qualify(
                    operation,
                    &agent,
                    &version,
                    false,
                    FixtureEvidence::new(
                        agent.clone(),
                        version.clone(),
                        "native/declared",
                        EvidenceKind::InstalledRuntimeFixture,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let mut engine = Engine::new();
        engine
            .register(Box::new(Adapter(AdapterDescriptor {
                agent: agent.clone(),
                version: version.clone(),
                display_name: "Native fixture declaration".into(),
                synthetic: false,
                architecture: ConnectionArchitecture::NativePty,
                capabilities,
            })))
            .unwrap();
        let evidence = FixtureEvidence::new(
            agent.clone(),
            version,
            "fake/not-native",
            EvidenceKind::SyntheticFixture,
        )
        .unwrap();
        let result = engine.checkup(
            &LaunchIntent {
                agent,
                action: crate::launch::LaunchAction::Start,
                placement: Placement::new(ProgramPlacement::System, HomePlacement::Box).unwrap(),
            },
            None,
            CheckupReadiness {
                program: ProgramReadiness::Fixture(&evidence),
                home: HomeReadiness::Fixture(&evidence),
                sidecar: SidecarReadiness::Fixture(&evidence),
                sets: None,
            },
        );
        assert!(!result.summary().native_start_enabled);
        assert!(result.prepared().is_none());
        assert!(
            result
                .summary()
                .issues
                .iter()
                .any(|item| item.reason == MissingReason::EvidenceRejected)
        );
        assert!(
            result
                .summary()
                .issues
                .iter()
                .any(|item| item.reason == MissingReason::NativeAdapterQualificationRequired)
        );
        assert_eq!(result.summary().status, CheckupStatus::Incomplete);
    }
}
