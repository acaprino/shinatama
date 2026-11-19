//! Declared Pi capabilities for an exact observed build. A capability is
//! supported only with the fixture that proved it; everything else carries an
//! explicit refusal reason. Native evidence is bound to the exact version.
use crate::{
    AgentId, Capabilities, EvidenceKind, FixtureEvidence, Operation, RefusalReason, Version,
};

/// Contained `node --version` + `node cli.js --version` via the npm shim layout.
pub const DETECT_FIXTURE: &str = "pi-npm-shim-node-script-v1";
/// The isolated ConPTY trial of the explicit-set TUI recipe.
pub const TUI_FIXTURE: &str = super::run::TUI_PROFILE;

/// What has actually been observed for this build in this process.
#[derive(Clone, Copy, Debug, Default)]
pub struct Observed {
    /// The contained version probe released cleanly with a supported Node.
    pub detected: bool,
    /// The isolated TUI trial produced a startup receipt.
    pub tui_trial: bool,
}

pub fn report(version: &Version, observed: Observed) -> Capabilities {
    let agent = AgentId::new("pi").expect("fixed agent id");
    let mut capabilities = Capabilities::default();
    for operation in Operation::ALL {
        capabilities.refuse(operation, refusal(operation));
    }
    let mut qualify = |operation, fixture| {
        let evidence = FixtureEvidence::new(
            agent.clone(),
            version.clone(),
            fixture,
            EvidenceKind::InstalledRuntimeFixture,
        )
        .expect("fixed fixture id");
        capabilities
            .qualify(operation, &agent, version, false, evidence)
            .expect("evidence for this exact build");
    };
    if observed.detected {
        qualify(Operation::Detect, DETECT_FIXTURE);
        if observed.tui_trial {
            for operation in [
                Operation::StartNew,
                Operation::Placement,
                Operation::InheritedMarkers,
            ] {
                qualify(operation, TUI_FIXTURE);
            }
        }
    }
    capabilities
}

/// Reasons recorded for refusals; absence of a fixture is never "supported".
pub fn refusal(operation: Operation) -> RefusalReason {
    match operation {
        // PI5: Pi's TUI cannot attach to a backend; RPC has no pane.
        Operation::AttachBackend => RefusalReason::NotImplemented,
        // Not yet observed: they need the probe or the TUI trial to pass.
        Operation::Detect
        | Operation::StartNew
        | Operation::Placement
        | Operation::InheritedMarkers => RefusalReason::NeedsContainedProbe,
        // Implemented elsewhere but not qualified for Pi by a fixture here:
        // MCP per launch (PI7), non-empty sets, auth status, resume/fork,
        // hooks/events and every model-backed operation.
        _ => RefusalReason::NotQualified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CapabilitySupport;

    fn supported(capabilities: &Capabilities) -> Vec<Operation> {
        capabilities
            .complete_report()
            .into_iter()
            .filter(|(_, support)| matches!(support, CapabilitySupport::Supported { .. }))
            .map(|(operation, _)| operation)
            .collect()
    }

    #[test]
    fn each_declared_capability_has_its_fixture_and_everything_else_is_refused() {
        let version = Version::parse("1.4.0").unwrap();
        assert!(supported(&report(&version, Observed::default())).is_empty());
        let detected = report(
            &version,
            Observed {
                detected: true,
                tui_trial: false,
            },
        );
        assert_eq!(supported(&detected), [Operation::Detect]);
        let full = report(
            &version,
            Observed {
                detected: true,
                tui_trial: true,
            },
        );
        assert_eq!(
            supported(&full),
            [
                Operation::Placement,
                Operation::Detect,
                Operation::InheritedMarkers,
                Operation::StartNew
            ]
        );
        for (operation, support) in full.complete_report() {
            match support {
                CapabilitySupport::Supported { evidence } => {
                    let fixture = serde_json::to_value(&evidence).unwrap()["fixture_id"]
                        .as_str()
                        .unwrap()
                        .to_owned();
                    let expected = if operation == Operation::Detect {
                        DETECT_FIXTURE
                    } else {
                        TUI_FIXTURE
                    };
                    assert_eq!(fixture, expected, "{operation:?}");
                }
                CapabilitySupport::Unsupported { reason } => {
                    assert_eq!(reason, refusal(operation), "{operation:?}")
                }
            }
        }
        for refused in [
            Operation::AttachBackend,
            Operation::McpInject,
            Operation::InstanceSet,
            Operation::AuthStatus,
            Operation::ResumeNative,
            Operation::QueueMessage,
            Operation::AskHeadless,
            Operation::StructuredEvents,
            Operation::Login,
        ] {
            assert!(full.require(refused).is_err(), "{refused:?}");
        }
        assert_eq!(
            full.require(Operation::AttachBackend),
            Err(RefusalReason::NotImplemented)
        );
        // Evidence is bound to the exact build it was observed on.
        assert_eq!(
            full.validate_for(
                &AgentId::new("pi").unwrap(),
                &Version::parse("1.4.1").unwrap(),
                false
            ),
            Err(RefusalReason::VersionMismatch)
        );
    }
}
