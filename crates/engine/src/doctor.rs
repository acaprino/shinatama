use crate::Engine;
use serde::Serialize;
use shinatama_adapter::catalog::{
    CatalogSnapshot, DiscoveryIssue, KNOWN_PROGRAMS, MAX_CANDIDATES, ProgramCandidate, ProgramKind,
};
use shinatama_adapter::{
    AgentId, CapabilitySupport, ConnectionArchitecture, ObservedVersion, Operation, RefusalReason,
    Version,
};
use std::collections::BTreeMap;

pub const DOCTOR_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckupStatus {
    Missing,
    Degraded,
    Ready,
    Synthetic,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentCheckup {
    pub agent: AgentId,
    pub display_name: String,
    pub status: CheckupStatus,
    pub installations: Vec<ProgramCandidate>,
    pub adapter_version: Option<Version>,
    pub architecture: Option<ConnectionArchitecture>,
    pub capabilities: BTreeMap<Operation, CapabilitySupport>,
    pub blockers: Vec<RefusalReason>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub read_only: bool,
    pub supported_platform: &'static str,
    pub catalog_provider: String,
    pub discovery_issues: Vec<DiscoveryIssue>,
    pub agents: Vec<AgentCheckup>,
}

impl DoctorReport {
    pub fn build(engine: &Engine, mut catalog: CatalogSnapshot, filter: Option<&AgentId>) -> Self {
        if catalog.candidates.len() > MAX_CANDIDATES {
            catalog.candidates.truncate(MAX_CANDIDATES);
            if !catalog.issues.contains(&DiscoveryIssue::CandidateLimit) {
                catalog.issues.push(DiscoveryIssue::CandidateLimit);
            }
        }
        // Provider metadata is diagnostic data, not arbitrary output or a path.
        if catalog.provider.len() > 160 || catalog.provider.chars().any(char::is_control) {
            catalog.provider = "invalid_provider_metadata".to_owned();
        }
        let mut agents = Vec::new();
        for known in KNOWN_PROGRAMS {
            let agent = AgentId::new(known.agent).expect("static agent ID");
            if filter.is_some_and(|filter| filter != &agent) {
                continue;
            }
            let installations: Vec<_> = catalog
                .candidates
                .iter()
                .filter(|candidate| candidate.agent == agent)
                .cloned()
                .collect();
            let descriptor = engine.descriptor(&agent);
            // Discovery alone does not qualify a native adapter. Exactly one
            // installation with the same observed build is required even to
            // expose its declared fixture-qualified capabilities in doctor.
            let exact_binding = descriptor.is_some_and(|descriptor| {
                !descriptor.synthetic && installations.len() == 1 && matches!(&installations[0].version, ObservedVersion::Known { version } if version == &descriptor.version)
            });
            let reason = if installations.is_empty() {
                RefusalReason::MissingProgram
            } else if installations
                .iter()
                .any(|candidate| candidate.kind != ProgramKind::DirectExe)
            {
                RefusalReason::WrapperNeedsSafeLaunch
            } else if descriptor.is_none() {
                RefusalReason::NotQualified
            } else if installations.len() == 1
                && matches!(&installations[0].version, ObservedVersion::Known { .. })
            {
                RefusalReason::VersionMismatch
            } else {
                RefusalReason::UnknownVersion
            };
            let (status, capabilities, blockers) = if exact_binding {
                let effective = effective_capabilities(engine, &agent);
                let any_supported = effective
                    .values()
                    .any(|support| matches!(support, CapabilitySupport::Supported { .. }));
                (
                    if any_supported {
                        CheckupStatus::Ready
                    } else {
                        CheckupStatus::Degraded
                    },
                    effective,
                    if any_supported {
                        Vec::new()
                    } else {
                        vec![RefusalReason::NotQualified]
                    },
                )
            } else {
                let status = if installations.is_empty() {
                    CheckupStatus::Missing
                } else {
                    CheckupStatus::Degraded
                };
                let mut blockers = vec![reason];
                if installations.iter().any(|candidate| {
                    candidate.kind == ProgramKind::DirectExe
                        && matches!(candidate.version, ObservedVersion::Unknown { .. })
                }) {
                    blockers.push(RefusalReason::NeedsContainedProbe);
                }
                (status, refused_capabilities(reason), blockers)
            };
            agents.push(AgentCheckup {
                agent,
                display_name: known.display_name.to_owned(),
                status,
                installations,
                adapter_version: descriptor.map(|descriptor| descriptor.version.clone()),
                architecture: descriptor.map(|descriptor| descriptor.architecture.clone()),
                capabilities,
                blockers,
            });
        }
        for descriptor in engine.descriptors() {
            if KNOWN_PROGRAMS
                .iter()
                .any(|known| known.agent == descriptor.agent.as_str())
                || filter.is_some_and(|filter| filter != &descriptor.agent)
            {
                continue;
            }
            agents.push(AgentCheckup {
                agent: descriptor.agent.clone(),
                display_name: descriptor.display_name.clone(),
                status: if descriptor.synthetic {
                    CheckupStatus::Synthetic
                } else {
                    CheckupStatus::Degraded
                },
                installations: Vec::new(),
                adapter_version: Some(descriptor.version.clone()),
                architecture: Some(descriptor.architecture.clone()),
                capabilities: if descriptor.synthetic {
                    effective_capabilities(engine, &descriptor.agent)
                } else {
                    refused_capabilities(RefusalReason::MissingProgram)
                },
                blockers: if descriptor.synthetic {
                    Vec::new()
                } else {
                    vec![RefusalReason::MissingProgram]
                },
            });
        }
        Self {
            schema_version: DOCTOR_SCHEMA_VERSION,
            read_only: true,
            supported_platform: "windows",
            catalog_provider: catalog.provider,
            discovery_issues: catalog.issues,
            agents,
        }
    }

    pub fn has_refusals(&self) -> bool {
        self.agents.is_empty()
            || !self.discovery_issues.is_empty()
            || self.agents.iter().any(|agent| {
                matches!(
                    agent.status,
                    CheckupStatus::Missing | CheckupStatus::Degraded
                )
            })
    }
}

fn effective_capabilities(
    engine: &Engine,
    agent: &AgentId,
) -> BTreeMap<Operation, CapabilitySupport> {
    Operation::ALL
        .into_iter()
        .map(|operation| {
            let support = match engine.require(agent, operation) {
                Ok(()) => engine
                    .descriptor(agent)
                    .expect("require checked registration")
                    .capabilities
                    .support(operation),
                Err(error) => CapabilitySupport::Unsupported {
                    reason: error.reason,
                },
            };
            (operation, support)
        })
        .collect()
}

fn refused_capabilities(reason: RefusalReason) -> BTreeMap<Operation, CapabilitySupport> {
    Operation::ALL
        .into_iter()
        .map(|operation| (operation, CapabilitySupport::Unsupported { reason }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_adapter::catalog::{EntryStatus, discover};
    use shinatama_adapter::fake::FakeAdapter;
    use std::path::PathBuf;

    #[test]
    fn installed_program_is_explicitly_degraded_without_fixture_qualified_adapter() {
        let catalog = discover(&[PathBuf::from("C:/tools")], true, |path| {
            if path.ends_with("codex.exe") {
                EntryStatus::RegularFile
            } else {
                EntryStatus::Missing
            }
        });
        let report = DoctorReport::build(
            &Engine::new(),
            catalog,
            Some(&AgentId::new("codex").unwrap()),
        );
        assert_eq!(report.agents.len(), 1);
        let agent = &report.agents[0];
        assert_eq!(agent.status, CheckupStatus::Degraded);
        assert!(agent.blockers.contains(&RefusalReason::NeedsContainedProbe));
        assert!(
            agent
                .capabilities
                .values()
                .all(|support| matches!(support, CapabilitySupport::Unsupported { .. }))
        );
        assert_eq!(report.schema_version, 1);
        assert!(report.read_only && report.has_refusals());
    }

    #[test]
    fn zero_and_fake_adapters_are_useful_without_touching_agent_homes() {
        let mut engine = Engine::new();
        engine
            .register(Box::new(FakeAdapter::new(vec![]).unwrap()))
            .unwrap();
        let id = AgentId::new("fake_memory").unwrap();
        let report = DoctorReport::build(&engine, CatalogSnapshot::default(), Some(&id));
        assert_eq!(report.agents[0].status, CheckupStatus::Synthetic);
        assert!(!report.has_refusals());
        assert_eq!(report.agents[0].capabilities.len(), Operation::ALL.len());
    }

    #[test]
    fn known_installed_version_mismatch_is_not_reported_as_unknown() {
        struct NativeDescriptor(shinatama_adapter::AdapterDescriptor);
        impl shinatama_adapter::AgentAdapter for NativeDescriptor {
            fn descriptor(&self) -> &shinatama_adapter::AdapterDescriptor {
                &self.0
            }
        }
        let agent = AgentId::new("codex").unwrap();
        let mut engine = Engine::new();
        engine
            .register(Box::new(NativeDescriptor(
                shinatama_adapter::AdapterDescriptor {
                    agent: agent.clone(),
                    display_name: "Unqualified native descriptor".into(),
                    version: Version::parse("1.0.0+expected").unwrap(),
                    architecture: ConnectionArchitecture::NativePty,
                    synthetic: false,
                    capabilities: Default::default(),
                },
            )))
            .unwrap();
        let mut catalog = discover(&[PathBuf::from("C:/tools")], true, |path| {
            if path.ends_with("codex.exe") {
                EntryStatus::RegularFile
            } else {
                EntryStatus::Missing
            }
        });
        catalog.candidates[0].version = ObservedVersion::Known {
            version: Version::parse("1.0.0+installed").unwrap(),
        };
        let report = DoctorReport::build(&engine, catalog, Some(&agent));
        assert_eq!(report.agents[0].status, CheckupStatus::Degraded);
        assert_eq!(
            report.agents[0].blockers,
            vec![RefusalReason::VersionMismatch]
        );
        assert!(
            report.agents[0]
                .capabilities
                .values()
                .all(|support| matches!(
                    support,
                    CapabilitySupport::Unsupported {
                        reason: RefusalReason::VersionMismatch
                    }
                ))
        );
    }
}
