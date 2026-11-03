//! One pure preflight for start/resume, shared by CLI and desktop callers.
//! Only exact synthetic fixtures can produce a composed plan in this increment.
//! Native execution remains closed: recipes and a plan are not runtime admission.
pub mod identity;

use crate::Engine;
use identity::{LaunchIdentity, LaunchIdentityRecord, verify_record};
use serde::{Deserialize, Deserializer, Serialize, de};
use shinatama_adapter::launch::{
    arguments::{CommandPlan, QualifiedArgvRecipe, plan_arguments},
    environment::{
        ChildEnvironment, ChildEnvironmentPlan, ChildOverride, KnownClaudeEditor, OverrideName,
    },
    placement::{PlacementPaths, PlacementPlan, QualifiedPlacementRecipe, plan_placement},
};
use shinatama_adapter::{
    AgentId, ConnectionArchitecture, HomePlacement, Operation, Placement, ProgramPlacement,
    RefusalReason,
};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::Path;

const INVALID_SELECTORS: &str = "Invalid launch selectors.";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchAction {
    Start,
    Resume,
}
impl LaunchAction {
    fn operation(self) -> Operation {
        match self {
            Self::Start => Operation::StartNew,
            Self::Resume => Operation::ResumeNative,
        }
    }
}

/// The only caller DTO. No argv, environment, identity token or proof is accepted.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct LaunchIntent {
    pub agent: AgentId,
    pub action: LaunchAction,
    pub placement: Placement,
}

struct Selector(String);
impl<'de> Deserialize<'de> for Selector {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Selector;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(INVALID_SELECTORS)
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Selector, E> {
                if value.len() > 64 {
                    return Err(E::custom(INVALID_SELECTORS));
                }
                Ok(Selector(value.to_owned()))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Selector, E> {
                if value.len() > 64 {
                    return Err(E::custom(INVALID_SELECTORS));
                }
                Ok(Selector(value))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}
impl<'de> Deserialize<'de> for LaunchIntent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawPlacement {
            program: Selector,
            home: Selector,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawIntent {
            agent: Selector,
            action: Selector,
            placement: RawPlacement,
        }
        let raw = RawIntent::deserialize(deserializer)
            .map_err(|_| <D::Error as de::Error>::custom(INVALID_SELECTORS))?;
        let invalid = || <D::Error as de::Error>::custom(INVALID_SELECTORS);
        let agent = AgentId::new(&raw.agent.0).map_err(|_| invalid())?;
        let action = match raw.action.0.as_str() {
            "start" => LaunchAction::Start,
            "resume" => LaunchAction::Resume,
            _ => return Err(invalid()),
        };
        let program = match raw.placement.program.0.as_str() {
            "system" => ProgramPlacement::System,
            "own" => ProgramPlacement::Own,
            _ => return Err(invalid()),
        };
        let home = match raw.placement.home.0.as_str() {
            "box" => HomePlacement::Box,
            "system" => HomePlacement::System,
            _ => return Err(invalid()),
        };
        let placement = Placement::new(program, home).map_err(|_| invalid())?;
        Ok(Self {
            agent,
            action,
            placement,
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStatus {
    Available,
    Refused,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchIssueCode {
    AdapterNotRegistered,
    CapabilityRefused,
    PreparedInputsRequired,
    PrivateStoreGrantRequired,
    C09rRequired,
    ProgramBindingRequired,
    StoreRegistrationRequired,
    SidecarQualificationRequired,
    PlacementRejected,
    ArgumentsRejected,
    EnvironmentRejected,
    IdentityRejected,
    DatabasePathRejected,
    SecretOutsideIdentityEnvironment,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct LaunchIssue {
    pub code: LaunchIssueCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<RefusalReason>,
}
impl LaunchIssue {
    fn new(code: LaunchIssueCode) -> Self {
        Self { code, reason: None }
    }
    fn refused(code: LaunchIssueCode, reason: RefusalReason) -> Self {
        Self {
            code,
            reason: Some(reason),
        }
    }
}

/// Safe review projection; never serialize the composed plan or launch inputs.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct LaunchSummary {
    pub agent: AgentId,
    pub action: LaunchAction,
    pub placement: Placement,
    pub status: LaunchStatus,
    pub issues: Vec<LaunchIssue>,
    pub synthetic: bool,
}

/// Trusted composition input, not an IPC DTO. An identity has already been minted
/// by the caller; preflight never requests entropy or changes process state.
pub struct LaunchInputs<'a> {
    placement_recipe: &'a QualifiedPlacementRecipe,
    arguments_recipe: &'a QualifiedArgvRecipe,
    working_directory: &'a Path,
    database_path: &'a Path,
    inherited_environment: &'a BTreeMap<OsString, OsString>,
    identity: &'a LaunchIdentity,
    private_paths: Option<&'a PlacementPaths>,
    claude_editor: Option<&'a KnownClaudeEditor>,
}
impl<'a> LaunchInputs<'a> {
    pub fn new(
        placement_recipe: &'a QualifiedPlacementRecipe,
        arguments_recipe: &'a QualifiedArgvRecipe,
        working_directory: &'a Path,
        database_path: &'a Path,
        inherited_environment: &'a BTreeMap<OsString, OsString>,
        identity: &'a LaunchIdentity,
    ) -> Self {
        Self {
            placement_recipe,
            arguments_recipe,
            working_directory,
            database_path,
            inherited_environment,
            identity,
            private_paths: None,
            claude_editor: None,
        }
    }
    pub fn with_private_paths(mut self, paths: &'a PlacementPaths) -> Self {
        self.private_paths = Some(paths);
        self
    }
    pub fn with_known_claude_editor(mut self, editor: &'a KnownClaudeEditor) -> Self {
        self.claude_editor = Some(editor);
        self
    }
}

/// No Serialize or public constructor. Even an available synthetic plan has no
/// execution method and conveys no C09R, store-registration or sidecar grant.
pub struct PreparedLaunch {
    command: CommandPlan,
    placement: PlacementPlan,
    environment: ChildEnvironment,
    identity: LaunchIdentityRecord,
    native_binding: Option<crate::native_launch::NativeLaunchBinding>,
}
impl PreparedLaunch {
    pub fn command(&self) -> &CommandPlan {
        &self.command
    }
    pub fn placement(&self) -> &PlacementPlan {
        &self.placement
    }
    pub fn environment(&self) -> &ChildEnvironment {
        &self.environment
    }
    pub fn identity_record(&self) -> &LaunchIdentityRecord {
        &self.identity
    }
    pub fn native_binding(&self) -> Option<&crate::native_launch::NativeLaunchBinding> {
        self.native_binding.as_ref()
    }
    /// Only the future C15 qualifier binds a prepared plan to its native pins.
    #[allow(dead_code)]
    pub(crate) fn set_native_binding(
        &mut self,
        binding: crate::native_launch::NativeLaunchBinding,
    ) {
        self.native_binding = Some(binding);
    }
}
impl fmt::Debug for PreparedLaunch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedLaunch")
            .field("agent", self.command.agent())
            .field("operation", &self.command.operation())
            .field("instance_id", &self.identity.instance_id)
            .finish_non_exhaustive()
    }
}

pub struct LaunchPreflight {
    summary: LaunchSummary,
    prepared: Option<PreparedLaunch>,
}
impl LaunchPreflight {
    pub fn summary(&self) -> &LaunchSummary {
        &self.summary
    }
    pub fn prepared(&self) -> Option<&PreparedLaunch> {
        self.prepared.as_ref()
    }
    pub fn into_prepared(self) -> Option<PreparedLaunch> {
        self.prepared
    }
}
impl fmt::Debug for LaunchPreflight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchPreflight")
            .field("summary", &self.summary)
            .field("prepared", &self.prepared.is_some())
            .finish()
    }
}

impl Engine {
    /// UI, CLI, start and resume share this bounded, IO-free entry point. The
    /// registry's execution require() stays closed; preparation cannot relax it.
    pub fn preflight_launch(
        &self,
        intent: &LaunchIntent,
        inputs: Option<&LaunchInputs<'_>>,
    ) -> LaunchPreflight {
        let mut summary = LaunchSummary {
            agent: intent.agent.clone(),
            action: intent.action,
            placement: intent.placement,
            status: LaunchStatus::Refused,
            issues: Vec::new(),
            synthetic: false,
        };
        let Some(descriptor) = self.descriptor(&intent.agent) else {
            summary.issues.push(LaunchIssue::refused(
                LaunchIssueCode::AdapterNotRegistered,
                RefusalReason::AdapterNotRegistered,
            ));
            return LaunchPreflight {
                summary,
                prepared: None,
            };
        };
        summary.synthetic = descriptor.synthetic;
        if descriptor.agent != intent.agent
            || descriptor.synthetic
                != matches!(descriptor.architecture, ConnectionArchitecture::InMemory)
            || descriptor.synthetic != descriptor.agent.as_str().starts_with("fake_")
        {
            summary.issues.push(LaunchIssue::refused(
                LaunchIssueCode::CapabilityRefused,
                RefusalReason::VersionMismatch,
            ));
            return LaunchPreflight {
                summary,
                prepared: None,
            };
        }
        let required = descriptor
            .capabilities
            .validate_for(&descriptor.agent, &descriptor.version, descriptor.synthetic)
            .and_then(|()| descriptor.capabilities.require(intent.action.operation()))
            .and_then(|()| descriptor.capabilities.require(Operation::Placement))
            .and_then(|()| descriptor.capabilities.require(Operation::InheritedMarkers));
        if let Err(reason) = required {
            summary.issues.push(LaunchIssue::refused(
                LaunchIssueCode::CapabilityRefused,
                reason,
            ));
            return LaunchPreflight {
                summary,
                prepared: None,
            };
        }
        // Native prerequisites are surfaced before inspecting environment values,
        // identities, paths, files, programs or sidecars. No caller-ready boolean
        // and no claimed fixture can open the execution gate in this increment.
        if !descriptor.synthetic {
            summary.status = LaunchStatus::Incomplete;
            for code in [
                LaunchIssueCode::C09rRequired,
                LaunchIssueCode::ProgramBindingRequired,
                LaunchIssueCode::StoreRegistrationRequired,
                LaunchIssueCode::SidecarQualificationRequired,
            ] {
                summary.issues.push(LaunchIssue::new(code));
            }
            if intent.placement.home() == HomePlacement::Box {
                summary
                    .issues
                    .push(LaunchIssue::new(LaunchIssueCode::PrivateStoreGrantRequired));
            }
            if inputs.is_none() {
                summary
                    .issues
                    .push(LaunchIssue::new(LaunchIssueCode::PreparedInputsRequired));
            }
            return LaunchPreflight {
                summary,
                prepared: None,
            };
        }
        let Some(inputs) = inputs else {
            summary.status = LaunchStatus::Incomplete;
            summary
                .issues
                .push(LaunchIssue::new(LaunchIssueCode::PreparedInputsRequired));
            return LaunchPreflight {
                summary,
                prepared: None,
            };
        };
        if intent.placement.home() == HomePlacement::Box && inputs.private_paths.is_none() {
            summary.status = LaunchStatus::Incomplete;
            summary
                .issues
                .push(LaunchIssue::new(LaunchIssueCode::PrivateStoreGrantRequired));
            return LaunchPreflight {
                summary,
                prepared: None,
            };
        }
        let composed = compose_launch(
            descriptor,
            intent.action.operation(),
            intent.placement,
            inputs,
        );
        match composed {
            Ok(plan) => {
                summary.status = LaunchStatus::Available;
                LaunchPreflight {
                    summary,
                    prepared: Some(plan),
                }
            }
            Err(code) => {
                summary.issues.push(LaunchIssue::new(code));
                LaunchPreflight {
                    summary,
                    prepared: None,
                }
            }
        }
    }
}

/// Shared C11 composition; this function grants no runtime authority.
pub(crate) fn compose_launch(
    descriptor: &shinatama_adapter::AdapterDescriptor,
    operation: Operation,
    mode: Placement,
    inputs: &LaunchInputs<'_>,
) -> Result<PreparedLaunch, LaunchIssueCode> {
    let placement = plan_placement(
        descriptor,
        mode,
        inputs.private_paths,
        Some(inputs.placement_recipe),
    )
    .map_err(|_| LaunchIssueCode::PlacementRejected)?;
    let command = plan_arguments(
        descriptor,
        operation,
        inputs.working_directory,
        Some(inputs.arguments_recipe),
    )
    .map_err(|_| LaunchIssueCode::ArgumentsRejected)?;
    if !absolute_database_path(inputs.database_path) {
        return Err(LaunchIssueCode::DatabasePathRejected);
    }
    let record = inputs.identity.record();
    let secret = inputs.identity.child_secret_override();
    let wire = secret
        .value()
        .to_str()
        .ok_or(LaunchIssueCode::IdentityRejected)?;
    verify_record(&record.instance_id, wire, Some(&record))
        .map_err(|_| LaunchIssueCode::IdentityRejected)?;
    if command
        .argv()
        .iter()
        .any(|token| contains_wire(token, wire))
        || contains_wire(command.executable().as_os_str(), wire)
        || contains_wire(command.working_directory().as_os_str(), wire)
        || contains_wire(inputs.database_path.as_os_str(), wire)
        || placement
            .overrides()
            .iter()
            .any(|item| contains_wire(item.value(), wire))
    {
        return Err(LaunchIssueCode::SecretOutsideIdentityEnvironment);
    }
    // Admit the names-only copy before allocating it. Values are still
    // validated and copied only by the bounded environment component.
    if inputs.inherited_environment.len() > 2048
        || inputs
            .inherited_environment
            .keys()
            .try_fold(0usize, |bytes, name| {
                let bytes = bytes.saturating_add(name.len());
                if name.len() > 4096 || bytes > 1024 * 1024 {
                    None
                } else {
                    Some(bytes)
                }
            })
            .is_none()
    {
        return Err(LaunchIssueCode::EnvironmentRejected);
    }
    let names: Vec<_> = inputs.inherited_environment.keys().cloned().collect();
    let editor = git_editor(inputs.inherited_environment);
    let scrub = ChildEnvironmentPlan::build(&names, editor, inputs.claude_editor, vec![])
        .map_err(|_| LaunchIssueCode::EnvironmentRejected)?;
    let clean = scrub
        .child_from(inputs.inherited_environment)
        .map_err(|_| LaunchIssueCode::EnvironmentRejected)?;
    let mut placed: BTreeMap<OsString, OsString> = clean
        .entries()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    for item in placement.overrides() {
        if is_identity_name(item.name()) {
            return Err(LaunchIssueCode::EnvironmentRejected);
        }
        placed.retain(|name, _| !name.eq_ignore_ascii_case(item.name()));
        placed.insert(item.name().into(), item.value().to_owned());
    }
    // Identity is derived solely from the authenticated record and is
    // applied last. No external custom override can shadow these fields.
    let overrides = vec![
        ChildOverride::new(OverrideName::Instance, record.instance_id.clone().into()),
        ChildOverride::new(OverrideName::Thread, record.thread_id.clone().into()),
        ChildOverride::new(
            OverrideName::Database,
            inputs.database_path.as_os_str().to_owned(),
        ),
        Ok(secret.clone()),
        ChildOverride::new(OverrideName::Terminal, "xterm-256color".into()),
        ChildOverride::new(OverrideName::ColorTerminal, "truecolor".into()),
    ]
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| LaunchIssueCode::EnvironmentRejected)?;
    let names: Vec<_> = placed.keys().cloned().collect();
    let environment =
        ChildEnvironmentPlan::build(&names, git_editor(&placed), inputs.claude_editor, overrides)
            .and_then(|plan| plan.child_from(&placed))
            .map_err(|_| LaunchIssueCode::EnvironmentRejected)?;
    // The bounded final child block is the last authority on token
    // placement, including accidental copies inherited from the caller.
    if environment.entries().any(|(name, value)| {
        contains_wire(name, wire)
            || (!name.eq_ignore_ascii_case("SHINATAMA_INSTANCE_TOKEN")
                && contains_wire(value, wire))
    }) {
        return Err(LaunchIssueCode::SecretOutsideIdentityEnvironment);
    }
    Ok(PreparedLaunch {
        command,
        placement,
        environment,
        identity: record,
        native_binding: None,
    })
}
fn git_editor(environment: &BTreeMap<OsString, OsString>) -> Option<&OsStr> {
    environment
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("GIT_EDITOR"))
        .map(|(_, value)| value.as_os_str())
}
fn is_identity_name(name: &str) -> bool {
    [
        "SHINATAMA_INSTANCE",
        "SHINATAMA_THREAD",
        "SHINATAMA_DB",
        "SHINATAMA_INSTANCE_TOKEN",
    ]
    .iter()
    .any(|reserved| name.eq_ignore_ascii_case(reserved))
}
fn contains_wire(value: &OsStr, wire: &str) -> bool {
    value
        .as_encoded_bytes()
        .windows(wire.len())
        .any(|bytes| bytes == wire.as_bytes())
}
fn absolute_database_path(path: &Path) -> bool {
    let Some(value) = path.to_str() else {
        return false;
    };
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return false;
    }
    let raw = value.strip_prefix(r"\\?\").unwrap_or(value);
    let bytes = raw.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
        && !bytes.iter().skip(2).any(|byte| *byte == b':')
        && !raw
            .split(['\\', '/'])
            .any(|part| matches!(part, "." | ".."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_adapter::launch::{
        arguments::CommandForm,
        placement::{HomeMapping, RecipeDefinition, RuntimeKind},
    };
    use shinatama_adapter::{
        AdapterDescriptor, AgentAdapter, Capabilities, EvidenceKind, FixtureEvidence,
    };
    use std::collections::BTreeSet;

    const THREAD: &str = "t_abcdefghijkl";
    struct Adapter(AdapterDescriptor);
    impl AgentAdapter for Adapter {
        fn descriptor(&self) -> &AdapterDescriptor {
            &self.0
        }
    }
    fn descriptor(id: &str, synthetic: bool) -> AdapterDescriptor {
        let agent = AgentId::new(id).unwrap();
        let version = shinatama_adapter::Version::parse("1.0.0+fixture").unwrap();
        let mut capabilities = Capabilities::default();
        for operation in [
            Operation::StartNew,
            Operation::ResumeNative,
            Operation::Placement,
            Operation::InheritedMarkers,
        ] {
            capabilities
                .qualify(
                    operation,
                    &agent,
                    &version,
                    synthetic,
                    evidence(&agent, &version, synthetic),
                )
                .unwrap();
        }
        AdapterDescriptor {
            agent,
            display_name: "Preflight fixture".into(),
            version,
            synthetic,
            capabilities,
            architecture: if synthetic {
                ConnectionArchitecture::InMemory
            } else {
                ConnectionArchitecture::NativePty
            },
        }
    }
    fn evidence(
        agent: &AgentId,
        version: &shinatama_adapter::Version,
        synthetic: bool,
    ) -> FixtureEvidence {
        FixtureEvidence::new(
            agent.clone(),
            version.clone(),
            "fixtures/preflight-only",
            if synthetic {
                EvidenceKind::SyntheticFixture
            } else {
                EvidenceKind::InstalledRuntimeFixture
            },
        )
        .unwrap()
    }
    fn intent(agent: &AgentId, action: LaunchAction) -> LaunchIntent {
        LaunchIntent {
            agent: agent.clone(),
            action,
            placement: Placement::new(ProgramPlacement::System, HomePlacement::System).unwrap(),
        }
    }
    fn recipes(
        descriptor: &AdapterDescriptor,
        action: LaunchAction,
        arguments: Vec<OsString>,
    ) -> (QualifiedPlacementRecipe, QualifiedArgvRecipe) {
        let placement = QualifiedPlacementRecipe::qualify(
            descriptor,
            intent(&descriptor.agent, action).placement,
            RecipeDefinition {
                home: HomeMapping::PreserveSystem,
                runtime: RuntimeKind::Native,
                features: BTreeSet::new(),
                limits: vec![],
                pi_skills: None,
                codex_sandbox: None,
            },
            evidence(&descriptor.agent, &descriptor.version, descriptor.synthetic),
        )
        .unwrap();
        let arguments = QualifiedArgvRecipe::qualify(
            descriptor,
            action.operation(),
            CommandForm::NativeExe("C:/fixture/agent.exe".into()),
            arguments,
            evidence(&descriptor.agent, &descriptor.version, descriptor.synthetic),
        )
        .unwrap();
        (placement, arguments)
    }

    #[test]
    fn default_registry_refuses_start_and_resume_with_identical_safe_shape() {
        let engine = Engine::new();
        let agent = AgentId::new("codex").unwrap();
        for action in [LaunchAction::Start, LaunchAction::Resume] {
            let checked = engine.preflight_launch(&intent(&agent, action), None);
            assert_eq!(checked.summary().status, LaunchStatus::Refused);
            assert_eq!(
                checked.summary().issues[0].code,
                LaunchIssueCode::AdapterNotRegistered
            );
            assert!(checked.prepared().is_none());
        }
    }

    #[test]
    fn native_claims_never_bypass_c09r_and_no_inputs_are_inspected() {
        let mut engine = Engine::new();
        let descriptor = descriptor("codex", false);
        let agent = descriptor.agent.clone();
        engine.register(Box::new(Adapter(descriptor))).unwrap();
        let checked = engine.preflight_launch(&intent(&agent, LaunchAction::Start), None);
        assert_eq!(checked.summary().status, LaunchStatus::Incomplete);
        assert!(
            checked
                .summary()
                .issues
                .iter()
                .any(|issue| issue.code == LaunchIssueCode::C09rRequired)
        );
        assert!(
            checked
                .summary()
                .issues
                .iter()
                .any(|issue| issue.code == LaunchIssueCode::SidecarQualificationRequired)
        );
        assert!(checked.prepared().is_none());
        assert_eq!(
            engine
                .require(&agent, Operation::StartNew)
                .unwrap_err()
                .reason,
            RefusalReason::NativeLaunchNeedsC09rAndC11
        );
    }

    #[test]
    fn exact_synthetic_start_and_resume_share_composition_and_stable_identity() {
        let mut engine = Engine::new();
        let descriptor = descriptor("fake_launch", true);
        engine
            .register(Box::new(Adapter(descriptor.clone())))
            .unwrap();
        let identity = LaunchIdentity::generate(THREAD).unwrap();
        let parent: BTreeMap<OsString, OsString> = [
            (
                "CLAUDE_CODE_CHILD_SESSION".into(),
                "synthetic-marker".into(),
            ),
            ("CODEX_HOME".into(), "preserve-user-config".into()),
            ("SHINATAMA_INSTANCE_TOKEN".into(), "outer-token".into()),
            ("SHINATAMA_THREAD".into(), "outer-thread".into()),
        ]
        .into();
        let original = parent.clone();
        for action in [LaunchAction::Start, LaunchAction::Resume] {
            let (placement, arguments) =
                recipes(&descriptor, action, vec!["synthetic-argument".into()]);
            let inputs = LaunchInputs::new(
                &placement,
                &arguments,
                Path::new("C:/fixture/work"),
                Path::new("C:/fixture/shinatama.db"),
                &parent,
                &identity,
            );
            let checked =
                engine.preflight_launch(&intent(&descriptor.agent, action), Some(&inputs));
            assert_eq!(checked.summary().status, LaunchStatus::Available);
            let prepared = checked.prepared().unwrap();
            assert_eq!(prepared.command().operation(), action.operation());
            assert_eq!(
                prepared.identity_record().instance_id,
                identity.instance_id()
            );
            assert_eq!(prepared.identity_record().thread_id, THREAD);
            let child: BTreeMap<_, _> = prepared.environment().entries().collect();
            assert!(!child.contains_key(OsStr::new("CLAUDE_CODE_CHILD_SESSION")));
            assert_eq!(
                child[OsStr::new("CODEX_HOME")],
                OsStr::new("preserve-user-config")
            );
            assert_eq!(child[OsStr::new("SHINATAMA_THREAD")], OsStr::new(THREAD));
            assert_eq!(
                child[OsStr::new("SHINATAMA_INSTANCE")],
                OsStr::new(identity.instance_id())
            );
            let secret = identity.child_secret_override();
            assert_eq!(
                child[OsStr::new("SHINATAMA_INSTANCE_TOKEN")],
                secret.value()
            );
            let summary = serde_json::to_string(checked.summary()).unwrap();
            let debug = format!("{checked:?} {prepared:?}");
            for value in [
                secret.value().to_str().unwrap(),
                "preserve-user-config",
                "synthetic-argument",
                "C:/fixture",
            ] {
                assert!(!summary.contains(value));
                assert!(!debug.contains(value));
            }
        }
        assert_eq!(parent, original);
    }

    #[test]
    fn missing_capability_or_prepared_inputs_is_never_available() {
        let mut engine = Engine::new();
        let mut descriptor = descriptor("fake_launch", true);
        descriptor
            .capabilities
            .refuse(Operation::InheritedMarkers, RefusalReason::NotQualified);
        let agent = descriptor.agent.clone();
        engine.register(Box::new(Adapter(descriptor))).unwrap();
        assert_eq!(
            engine
                .preflight_launch(&intent(&agent, LaunchAction::Start), None)
                .summary()
                .status,
            LaunchStatus::Refused
        );
        let mut engine = Engine::new();
        engine
            .register(Box::new(Adapter(super::tests::descriptor(
                "fake_launch",
                true,
            ))))
            .unwrap();
        assert_eq!(
            engine
                .preflight_launch(&intent(&agent, LaunchAction::Start), None)
                .summary()
                .status,
            LaunchStatus::Incomplete
        );
    }

    #[test]
    fn wrong_operation_or_token_in_argv_is_refused_without_secret_diagnostics() {
        let mut engine = Engine::new();
        let descriptor = descriptor("fake_launch", true);
        engine
            .register(Box::new(Adapter(descriptor.clone())))
            .unwrap();
        let identity = LaunchIdentity::generate(THREAD).unwrap();
        let parent = BTreeMap::new();
        let secret = identity.child_secret_override();
        let (placement, arguments) = recipes(
            &descriptor,
            LaunchAction::Start,
            vec![secret.value().to_owned()],
        );
        let inputs = LaunchInputs::new(
            &placement,
            &arguments,
            Path::new("C:/fixture/work"),
            Path::new("C:/fixture/shinatama.db"),
            &parent,
            &identity,
        );
        let checked = engine.preflight_launch(
            &intent(&descriptor.agent, LaunchAction::Start),
            Some(&inputs),
        );
        assert_eq!(checked.summary().status, LaunchStatus::Refused);
        assert_eq!(
            checked.summary().issues[0].code,
            LaunchIssueCode::SecretOutsideIdentityEnvironment
        );
        let checked = engine.preflight_launch(
            &intent(&descriptor.agent, LaunchAction::Resume),
            Some(&inputs),
        );
        assert_eq!(
            checked.summary().issues[0].code,
            LaunchIssueCode::ArgumentsRejected
        );
    }

    #[test]
    fn caller_selectors_are_bounded_and_deserialization_errors_never_reflect_input() {
        let valid =
            r#"{"agent":"codex","action":"start","placement":{"program":"system","home":"box"}}"#;
        let parsed: LaunchIntent = serde_json::from_str(valid).unwrap();
        assert_eq!(parsed.action, LaunchAction::Start);
        for invalid in [
            valid.replace("start", "synthetic-secret-selector"),
            valid.replace("codex", &"synthetic-secret".repeat(100)),
            valid
                .replace("\"program\":\"system\"", "\"program\":\"own\"")
                .replace("\"home\":\"box\"", "\"home\":\"system\""),
            valid.replace(
                "\"action\":\"start\"",
                "\"action\":\"start\",\"token\":\"synthetic-secret\"",
            ),
        ] {
            let error = serde_json::from_str::<LaunchIntent>(&invalid)
                .unwrap_err()
                .to_string();
            assert!(error.contains(INVALID_SELECTORS));
            assert!(!error.contains("synthetic-secret"));
        }
    }

    #[test]
    fn inherited_token_in_any_non_reserved_environment_name_or_value_is_refused() {
        let mut engine = Engine::new();
        let descriptor = descriptor("fake_launch", true);
        engine
            .register(Box::new(Adapter(descriptor.clone())))
            .unwrap();
        let identity = LaunchIdentity::generate(THREAD).unwrap();
        let secret = identity.child_secret_override();
        let wire = secret.value().to_str().unwrap();
        let (placement, arguments) = recipes(&descriptor, LaunchAction::Start, vec![]);
        for in_name in [false, true] {
            let mut parent = BTreeMap::new();
            if in_name {
                parent.insert(format!("SYNTHETIC_{wire}").into(), "preserved-input".into());
            } else {
                parent.insert(
                    "SYNTHETIC_OTHER".into(),
                    format!("prefix-{wire}-suffix").into(),
                );
            }
            let original = parent.clone();
            let inputs = LaunchInputs::new(
                &placement,
                &arguments,
                Path::new("C:/fixture/work"),
                Path::new("C:/fixture/shinatama.db"),
                &parent,
                &identity,
            );
            let checked = engine.preflight_launch(
                &intent(&descriptor.agent, LaunchAction::Start),
                Some(&inputs),
            );
            assert_eq!(checked.summary().status, LaunchStatus::Refused);
            assert_eq!(
                checked.summary().issues[0].code,
                LaunchIssueCode::SecretOutsideIdentityEnvironment
            );
            assert!(checked.prepared().is_none());
            assert!(
                !serde_json::to_string(checked.summary())
                    .unwrap()
                    .contains(wire)
            );
            assert!(!format!("{checked:?}").contains(wire));
            assert_eq!(parent, original);
        }
    }
}
