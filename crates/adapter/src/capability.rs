use crate::{AgentId, Version};
use serde::Serialize;
use std::collections::BTreeMap;

/// Stable names cover every operation group in ADR-0011. Absence means refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Placement,
    Detect,
    InstanceSet,
    FixtureGate,
    InheritedMarkers,
    AuthStatus,
    Login,
    InstallOrUpdate,
    SystemHomes,
    ListSessions,
    ReadTranscript,
    ExportInterchange,
    ExportNative,
    ImportSupport,
    ImportNative,
    VerifyImport,
    StartNew,
    ResumeNative,
    ForkNative,
    AttachBackend,
    HandoverOut,
    HandoverIn,
    ConfirmReceipt,
    CatchUp,
    Release,
    StructuredEvents,
    QueueMessage,
    AskHeadless,
    Hooks,
    McpInject,
}

impl Operation {
    pub const ALL: [Self; 30] = [
        Self::Placement,
        Self::Detect,
        Self::InstanceSet,
        Self::FixtureGate,
        Self::InheritedMarkers,
        Self::AuthStatus,
        Self::Login,
        Self::InstallOrUpdate,
        Self::SystemHomes,
        Self::ListSessions,
        Self::ReadTranscript,
        Self::ExportInterchange,
        Self::ExportNative,
        Self::ImportSupport,
        Self::ImportNative,
        Self::VerifyImport,
        Self::StartNew,
        Self::ResumeNative,
        Self::ForkNative,
        Self::AttachBackend,
        Self::HandoverOut,
        Self::HandoverIn,
        Self::ConfirmReceipt,
        Self::CatchUp,
        Self::Release,
        Self::StructuredEvents,
        Self::QueueMessage,
        Self::AskHeadless,
        Self::Hooks,
        Self::McpInject,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    SyntheticFixture,
    InstalledRuntimeFixture,
}

/// A fixture reference is an audit identifier, never a transcript or secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FixtureEvidence {
    agent: AgentId,
    version: Version,
    fixture_id: String,
    kind: EvidenceKind,
}

impl FixtureEvidence {
    pub fn new(
        agent: AgentId,
        version: Version,
        fixture_id: &str,
        kind: EvidenceKind,
    ) -> Result<Self, &'static str> {
        if fixture_id.is_empty()
            || fixture_id.len() > 160
            || !fixture_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b))
        {
            return Err("invalid_fixture_reference");
        }
        Ok(Self {
            agent,
            version,
            fixture_id: fixture_id.to_owned(),
            kind,
        })
    }

    fn validate_for(
        &self,
        agent: &AgentId,
        version: &Version,
        synthetic: bool,
    ) -> Result<(), RefusalReason> {
        if &self.agent != agent || &self.version != version {
            return Err(RefusalReason::VersionMismatch);
        }
        match (synthetic, self.kind) {
            (false, EvidenceKind::SyntheticFixture) => {
                Err(RefusalReason::SyntheticEvidenceForNativeAdapter)
            }
            (true, EvidenceKind::InstalledRuntimeFixture) => {
                Err(RefusalReason::NativeEvidenceForSyntheticAdapter)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CapabilitySupport {
    Unsupported { reason: RefusalReason },
    Supported { evidence: FixtureEvidence },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalReason {
    NotImplemented,
    NotQualified,
    UnknownVersion,
    VersionMismatch,
    SyntheticEvidenceForNativeAdapter,
    NativeEvidenceForSyntheticAdapter,
    ArchitectureNotSelected,
    MissingProgram,
    WrapperNeedsSafeLaunch,
    NeedsContainedProbe,
    UnsupportedPlatform,
    AdapterNotRegistered,
    InvalidRequest,
    NotFound,
    StaleCursor,
    LimitExceeded,
    PostV1Only,
    NativeLaunchNeedsC09rAndC11,
}

/// Construction checks exact identity, including prerelease and build metadata.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct Capabilities(BTreeMap<Operation, CapabilitySupport>);

impl Capabilities {
    pub fn qualify(
        &mut self,
        operation: Operation,
        agent: &AgentId,
        version: &Version,
        synthetic: bool,
        evidence: FixtureEvidence,
    ) -> Result<(), RefusalReason> {
        evidence.validate_for(agent, version, synthetic)?;
        self.0
            .insert(operation, CapabilitySupport::Supported { evidence });
        Ok(())
    }

    /// Cloning a capability set does not rebind its evidence to another adapter.
    pub fn validate_for(
        &self,
        agent: &AgentId,
        version: &Version,
        synthetic: bool,
    ) -> Result<(), RefusalReason> {
        for support in self.0.values() {
            if let CapabilitySupport::Supported { evidence } = support {
                evidence.validate_for(agent, version, synthetic)?;
            }
        }
        Ok(())
    }

    pub fn refuse(&mut self, operation: Operation, reason: RefusalReason) {
        self.0
            .insert(operation, CapabilitySupport::Unsupported { reason });
    }

    pub fn support(&self, operation: Operation) -> CapabilitySupport {
        self.0
            .get(&operation)
            .cloned()
            .unwrap_or(CapabilitySupport::Unsupported {
                reason: RefusalReason::NotImplemented,
            })
    }

    pub fn require(&self, operation: Operation) -> Result<(), RefusalReason> {
        match self.support(operation) {
            CapabilitySupport::Supported { .. } => Ok(()),
            CapabilitySupport::Unsupported { reason } => Err(reason),
        }
    }

    pub fn complete_report(&self) -> BTreeMap<Operation, CapabilitySupport> {
        Operation::ALL
            .into_iter()
            .map(|op| (op, self.support(op)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_cannot_qualify_another_build_or_native_adapter() {
        let id = AgentId::new("codex").unwrap();
        let version = Version::parse("1.2.3+one").unwrap();
        let evidence = FixtureEvidence::new(
            id.clone(),
            version.clone(),
            "fixtures/fake",
            EvidenceKind::SyntheticFixture,
        )
        .unwrap();
        let mut capabilities = Capabilities::default();
        assert_eq!(
            capabilities.qualify(
                Operation::ListSessions,
                &id,
                &Version::parse("1.2.3+two").unwrap(),
                true,
                evidence.clone()
            ),
            Err(RefusalReason::VersionMismatch)
        );
        assert_eq!(
            capabilities.qualify(Operation::ListSessions, &id, &version, false, evidence),
            Err(RefusalReason::SyntheticEvidenceForNativeAdapter)
        );
        assert_eq!(capabilities.complete_report().len(), Operation::ALL.len());
        assert_eq!(
            capabilities.require(Operation::AskHeadless),
            Err(RefusalReason::NotImplemented)
        );
    }

    #[test]
    fn cloned_evidence_stays_bound_to_agent_build_and_fixture_kind() {
        let agent = AgentId::new("fake_memory").unwrap();
        let version = Version::parse("1.2.3+one").unwrap();
        let mut capabilities = Capabilities::default();
        capabilities
            .qualify(
                Operation::ListSessions,
                &agent,
                &version,
                true,
                FixtureEvidence::new(
                    agent.clone(),
                    version.clone(),
                    "fixtures/synthetic",
                    EvidenceKind::SyntheticFixture,
                )
                .unwrap(),
            )
            .unwrap();
        let cloned = capabilities.clone();
        assert!(cloned.validate_for(&agent, &version, true).is_ok());
        assert_eq!(
            cloned.validate_for(&AgentId::new("codex").unwrap(), &version, false),
            Err(RefusalReason::VersionMismatch)
        );
        for other_version in ["1.2.4+one", "1.2.3-rc.1+one", "1.2.3+two"] {
            assert_eq!(
                cloned.validate_for(&agent, &Version::parse(other_version).unwrap(), true),
                Err(RefusalReason::VersionMismatch)
            );
        }
        assert_eq!(
            cloned.validate_for(&agent, &version, false),
            Err(RefusalReason::SyntheticEvidenceForNativeAdapter)
        );
        assert_eq!(
            capabilities.qualify(
                Operation::ReadTranscript,
                &agent,
                &version,
                true,
                FixtureEvidence::new(
                    agent.clone(),
                    version.clone(),
                    "fixtures/installed",
                    EvidenceKind::InstalledRuntimeFixture,
                )
                .unwrap(),
            ),
            Err(RefusalReason::NativeEvidenceForSyntheticAdapter)
        );
    }
}
