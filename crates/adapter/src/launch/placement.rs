//! C11.2: pure placement preparation. No directories, processes or credentials.
//!
//! FRD-0002/R01-R20, ADR-0012 v2 and D15-D17 describe required recipes, not
//! universal compatibility. There are deliberately no built-in qualified native
//! recipes: a declaration or private ACL grant does not prove runtime isolation.
//! The bridge translates state/store_roots grants into logical paths while holding
//! the grant alive. This crate has no dependency on state and defines no DB schema.

use crate::{
    AdapterDescriptor, AgentId, Capabilities, ConnectionArchitecture, FixtureEvidence,
    HomePlacement, Operation, Placement, ProgramPlacement, RefusalReason, Version,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

const MAX_PATH_BYTES: usize = 4096;

/// Shared by role through the bridge, without importing the FS grant type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathRole {
    Home,
    Temporary,
    Cache,
    NodeCompileCache,
    XdgConfig,
    XdgData,
    XdgCache,
    XdgState,
    OpenCodeDatabase,
    DeepSeekAgents,
    // C11.2's state grant does not provide these. Missing means refused, not guessed.
    OpenCodeToolHome,
    PiChildHome,
    ManagedVersion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathUnit {
    Utf8Bytes,
    Utf16Units,
}

/// A verified recipe supplies each bound and its unit. No socket/home constant
/// is inferred from a different agent/version or silently shortened here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathLimit {
    pub role: PathRole,
    pub unit: PathUnit,
    pub maximum: NonZeroUsize,
}

#[derive(Clone)]
pub struct PlacementPaths {
    root: PathBuf,
    paths: BTreeMap<PathRole, PathBuf>,
}
impl PlacementPaths {
    /// Inputs must come from a canonical, owner-only grant held by the caller.
    /// This lexical check does not replace identity/reparse/ACL verification.
    pub fn from_grant(
        root: PathBuf,
        paths: Vec<(PathRole, PathBuf)>,
    ) -> Result<Self, PlacementError> {
        if paths.len() > 13 {
            return Err(PlacementError::InvalidPrivatePath);
        }
        let root_parts = private_components(&root)?;
        let mut mapped = BTreeMap::new();
        for (role, path) in paths {
            let inside = {
                let parts = private_components(&path)?;
                parts.len() > root_parts.len()
                    && root_parts
                        .iter()
                        .zip(&parts)
                        .all(|(left, right)| left.eq_ignore_ascii_case(right))
            };
            if !inside || mapped.insert(role, path).is_some() {
                return Err(PlacementError::InvalidPrivatePath);
            }
        }
        drop(root_parts);
        Ok(Self {
            root,
            paths: mapped,
        })
    }
    pub fn path(&self, role: PathRole) -> Option<&Path> {
        self.paths.get(&role).map(PathBuf::as_path)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}
impl fmt::Debug for PlacementPaths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlacementPaths")
            .field("roles", &self.paths.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomeMapping {
    PreserveSystem,
    Claude,
    Codex,
    OpenCode,
    Pi,
    Hermes,
    DeepSeek,
    Synthetic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeKind {
    Native,
    Node,
    Python,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiSkillIsolation {
    NoSkillsArgument,
    PrivateChildHome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexSandbox {
    Disabled,
    UnelevatedDisclosed,
    Elevated,
}

/// These are seams the exact-version recipe must have proved. Returning a plan
/// does not prepare a plugin/config/service; the launch boundary must also bind
/// the corresponding prepared artifacts and C09R admission before execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecipeFeature {
    OwnerOnlyGrant,
    PrivateTemporary,
    PrivateNodeCompileCache,
    ClaudeMcpLogAttempt,
    ClaudeMcpLogsSuppressed,
    ClaudeMcpLogLeakDisclosed,
    PrivateCodexDaemon,
    CodexElevatedSandboxExcluded,
    CodexDenybinLeakDisclosed,
    OpenCodeXdgRestorePlugin,
    OwnedOpenCodeService,
    OpenCodeClaudeReadsDisabled,
    EmptyOpenCodeToolHome,
    PiSkillsIsolated,
    EmptyHermesExternalSkillDirs,
    OwnedHermesGateway,
    DeepSeekAgentsIsolated,
    SideBySideVersions,
    RollbackVerified,
    AutomaticUpdatesBlocked,
    ManualUpdatesBlocked,
    CodexStartupUpdatesBlocked,
    CodexUpdateCommandBlocked,
    CodexDaemonUpdatesBlocked,
    HermesInstallerQualified,
    HermesBundledRuntimeQualified,
    PiNodeAtLeast22_19,
}

/// Declarative data, intentionally not Deserialize/Serialize. IPC cannot turn a
/// user-supplied declaration into QualifiedPlacementRecipe.
#[derive(Clone, Debug)]
pub struct RecipeDefinition {
    pub home: HomeMapping,
    pub runtime: RuntimeKind,
    pub features: BTreeSet<RecipeFeature>,
    pub limits: Vec<PathLimit>,
    pub pi_skills: Option<PiSkillIsolation>,
    pub codex_sandbox: Option<CodexSandbox>,
}

#[derive(Clone)]
pub struct QualifiedPlacementRecipe {
    agent: AgentId,
    version: Version,
    synthetic: bool,
    mode: Placement,
    evidence: Capabilities,
    definition: RecipeDefinition,
}
impl QualifiedPlacementRecipe {
    /// Only trusted profile code supplies fixture results. This checks their
    /// binding, not their truth, and never performs a native qualification probe.
    pub fn qualify(
        descriptor: &AdapterDescriptor,
        mode: Placement,
        definition: RecipeDefinition,
        evidence: FixtureEvidence,
    ) -> Result<Self, PlacementError> {
        require_descriptor(descriptor)?;
        let mut qualified = Capabilities::default();
        qualified
            .qualify(
                Operation::Placement,
                &descriptor.agent,
                &descriptor.version,
                descriptor.synthetic,
                evidence,
            )
            .map_err(PlacementError::Unqualified)?;
        check_definition(descriptor, mode, &definition)?;
        Ok(Self {
            agent: descriptor.agent.clone(),
            version: descriptor.version.clone(),
            synthetic: descriptor.synthetic,
            mode,
            evidence: qualified,
            definition,
        })
    }

    /// Pass this same explicit limit to state's PrivateStoreRequest. None is not
    /// permission to invent a different Codex budget or relocate the data root.
    pub fn home_limit(&self) -> Option<PathLimit> {
        self.definition
            .limits
            .iter()
            .find(|limit| limit.role == PathRole::Home)
            .copied()
    }
}
impl fmt::Debug for QualifiedPlacementRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QualifiedPlacementRecipe")
            .field("agent", &self.agent)
            .field("version", &self.version)
            .field("mode", &self.mode)
            .field("mapping", &self.definition.home)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementError {
    MissingRecipe,
    Unqualified(RefusalReason),
    ModeMismatch,
    IncompleteRecipe,
    InvalidPrivatePath,
    MissingPath(PathRole),
    PathBudget {
        role: PathRole,
        unit: PathUnit,
        maximum: usize,
        observed: usize,
    },
    UnsupportedIsolation,
}

#[derive(Clone)]
pub struct PlacementOverride {
    name: &'static str,
    value: OsString,
}
impl PlacementOverride {
    pub fn name(&self) -> &'static str {
        self.name
    }
    /// Explicit child-builder access; never write these values to diagnostics.
    pub fn value(&self) -> &OsStr {
        &self.value
    }
}
impl fmt::Debug for PlacementOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlacementOverride")
            .field("name", &self.name)
            .field("value", &"[redacted]")
            .finish()
    }
}

#[derive(Clone)]
pub enum ProgramPlan {
    PreserveSystemBinary,
    ManagedVersion { directory: PathBuf },
}
impl fmt::Debug for ProgramPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PreserveSystemBinary => "PreserveSystemBinary",
            Self::ManagedVersion { .. } => "ManagedVersion([private path])",
        })
    }
}

#[derive(Clone, Debug)]
pub struct PlacementPlan {
    agent: AgentId,
    version: Version,
    mode: Placement,
    program: ProgramPlan,
    overrides: Vec<PlacementOverride>,
    features: BTreeSet<RecipeFeature>,
    pi_skills: Option<PiSkillIsolation>,
}
impl PlacementPlan {
    pub fn agent(&self) -> &AgentId {
        &self.agent
    }
    pub fn version(&self) -> &Version {
        &self.version
    }
    pub fn placement(&self) -> Placement {
        self.mode
    }
    pub fn program(&self) -> &ProgramPlan {
        &self.program
    }
    pub fn overrides(&self) -> &[PlacementOverride] {
        &self.overrides
    }
    pub fn required_features(&self) -> &BTreeSet<RecipeFeature> {
        &self.features
    }
    pub fn pi_skill_requirement(&self) -> Option<PiSkillIsolation> {
        self.pi_skills
    }
    /// A logical plan is never itself an owner-only FS or process admission grant.
    pub fn requires_held_private_grant(&self) -> bool {
        self.mode.home() == HomePlacement::Box
    }
}

pub fn default_placement(system_program_installed: bool) -> Placement {
    Placement::new(
        if system_program_installed {
            ProgramPlacement::System
        } else {
            ProgramPlacement::Own
        },
        HomePlacement::Box,
    )
    .expect("both documented defaults use a box home")
}

pub fn plan_placement(
    descriptor: &AdapterDescriptor,
    mode: Placement,
    paths: Option<&PlacementPaths>,
    recipe: Option<&QualifiedPlacementRecipe>,
) -> Result<PlacementPlan, PlacementError> {
    require_descriptor(descriptor)?;
    let recipe = recipe.ok_or(PlacementError::MissingRecipe)?;
    if recipe.agent != descriptor.agent
        || recipe.version != descriptor.version
        || recipe.synthetic != descriptor.synthetic
    {
        return Err(PlacementError::Unqualified(RefusalReason::VersionMismatch));
    }
    if recipe.mode != mode {
        return Err(PlacementError::ModeMismatch);
    }
    recipe
        .evidence
        .validate_for(&descriptor.agent, &descriptor.version, descriptor.synthetic)
        .map_err(PlacementError::Unqualified)?;
    recipe
        .evidence
        .require(Operation::Placement)
        .map_err(PlacementError::Unqualified)?;
    check_definition(descriptor, mode, &recipe.definition)?;
    let paths = if mode.home() == HomePlacement::Box {
        Some(paths.ok_or(PlacementError::MissingPath(PathRole::Home))?)
    } else {
        None
    };
    let path = |role| {
        paths
            .and_then(|paths| paths.path(role))
            .ok_or(PlacementError::MissingPath(role))
    };
    for limit in &recipe.definition.limits {
        let measured = path(limit.role)?
            .to_str()
            .ok_or(PlacementError::InvalidPrivatePath)?;
        let observed = match limit.unit {
            PathUnit::Utf8Bytes => measured.len(),
            PathUnit::Utf16Units => measured.encode_utf16().count(),
        };
        if observed > limit.maximum.get() {
            return Err(PlacementError::PathBudget {
                role: limit.role,
                unit: limit.unit,
                maximum: limit.maximum.get(),
                observed,
            });
        }
    }
    let mut overrides = Vec::new();
    let mut assign = |name, role| -> Result<(), PlacementError> {
        overrides.push(PlacementOverride {
            name,
            value: path(role)?.as_os_str().to_owned(),
        });
        Ok(())
    };
    if mode.home() == HomePlacement::Box {
        assign("TEMP", PathRole::Temporary)?;
        assign("TMP", PathRole::Temporary)?;
        if recipe.definition.runtime == RuntimeKind::Node {
            assign("NODE_COMPILE_CACHE", PathRole::NodeCompileCache)?;
        }
        match recipe.definition.home {
            HomeMapping::Claude => assign("CLAUDE_CONFIG_DIR", PathRole::Home)?,
            HomeMapping::Codex => assign("CODEX_HOME", PathRole::Home)?,
            HomeMapping::OpenCode => {
                for (name, role) in [
                    ("OPENCODE_DB", PathRole::OpenCodeDatabase),
                    ("XDG_CONFIG_HOME", PathRole::XdgConfig),
                    ("XDG_DATA_HOME", PathRole::XdgData),
                    ("XDG_CACHE_HOME", PathRole::XdgCache),
                    ("XDG_STATE_HOME", PathRole::XdgState),
                    ("OPENCODE_TEST_HOME", PathRole::OpenCodeToolHome),
                ] {
                    assign(name, role)?;
                }
            }
            HomeMapping::Pi => {
                assign("PI_CODING_AGENT_DIR", PathRole::Home)?;
                if recipe.definition.pi_skills == Some(PiSkillIsolation::PrivateChildHome) {
                    assign("HOME", PathRole::PiChildHome)?;
                }
            }
            HomeMapping::Hermes => assign("HERMES_HOME", PathRole::Home)?,
            HomeMapping::DeepSeek => {
                assign("DSH_HOME", PathRole::Home)?;
                assign("DSH_AGENTS_HOME", PathRole::DeepSeekAgents)?;
            }
            HomeMapping::Synthetic => assign("SHINATAMA_FIXTURE_HOME", PathRole::Home)?,
            HomeMapping::PreserveSystem => return Err(PlacementError::IncompleteRecipe),
        }
        if recipe.definition.home == HomeMapping::OpenCode {
            overrides.push(PlacementOverride {
                name: "OPENCODE_DISABLE_CLAUDE_CODE",
                value: "1".into(),
            });
        }
    }
    let program = if mode.program() == ProgramPlacement::System {
        ProgramPlan::PreserveSystemBinary
    } else {
        let directory = path(PathRole::ManagedVersion)?.to_owned();
        for (name, value) in own_program_controls(&descriptor.agent) {
            overrides.push(PlacementOverride {
                name,
                value: value.into(),
            });
        }
        ProgramPlan::ManagedVersion { directory }
    };
    Ok(PlacementPlan {
        agent: descriptor.agent.clone(),
        version: descriptor.version.clone(),
        mode,
        program,
        overrides,
        features: recipe.definition.features.clone(),
        pi_skills: recipe.definition.pi_skills,
    })
}

fn require_descriptor(descriptor: &AdapterDescriptor) -> Result<(), PlacementError> {
    if descriptor.synthetic != matches!(descriptor.architecture, ConnectionArchitecture::InMemory)
        || descriptor.synthetic != descriptor.agent.as_str().starts_with("fake_")
    {
        return Err(PlacementError::Unqualified(RefusalReason::NotQualified));
    }
    descriptor
        .capabilities
        .validate_for(&descriptor.agent, &descriptor.version, descriptor.synthetic)
        .map_err(PlacementError::Unqualified)?;
    descriptor
        .capabilities
        .require(Operation::Placement)
        .map_err(PlacementError::Unqualified)
}

fn check_definition(
    descriptor: &AdapterDescriptor,
    mode: Placement,
    definition: &RecipeDefinition,
) -> Result<(), PlacementError> {
    let required =
        |items: &[RecipeFeature]| items.iter().all(|item| definition.features.contains(item));
    let mut seen = BTreeSet::new();
    if definition
        .limits
        .iter()
        .any(|limit| !seen.insert(limit.role))
    {
        return Err(PlacementError::IncompleteRecipe);
    }
    if mode.home() == HomePlacement::System {
        return if mode.program() == ProgramPlacement::System
            && definition.home == HomeMapping::PreserveSystem
            && definition.limits.is_empty()
            && definition.features.is_empty()
            && definition.pi_skills.is_none()
            && definition.codex_sandbox.is_none()
        {
            Ok(())
        } else {
            Err(PlacementError::IncompleteRecipe)
        };
    }
    if (definition.home != HomeMapping::Pi && definition.pi_skills.is_some())
        || (definition.home != HomeMapping::Codex && definition.codex_sandbox.is_some())
        || definition
            .features
            .iter()
            .any(|feature| !feature_belongs(*feature, definition.home, mode.program()))
    {
        return Err(PlacementError::IncompleteRecipe);
    }
    if !required(&[
        RecipeFeature::OwnerOnlyGrant,
        RecipeFeature::PrivateTemporary,
    ]) || (definition.runtime == RuntimeKind::Node
        && !required(&[RecipeFeature::PrivateNodeCompileCache]))
    {
        return Err(PlacementError::IncompleteRecipe);
    }
    let valid = match (descriptor.agent.as_str(), definition.home) {
        ("claude_code", HomeMapping::Claude) => {
            required(&[RecipeFeature::ClaudeMcpLogAttempt])
                && (required(&[RecipeFeature::ClaudeMcpLogsSuppressed])
                    || required(&[RecipeFeature::ClaudeMcpLogLeakDisclosed]))
        }
        ("codex", HomeMapping::Codex) => {
            if definition.codex_sandbox == Some(CodexSandbox::Elevated) {
                return Err(PlacementError::UnsupportedIsolation);
            }
            required(&[
                RecipeFeature::PrivateCodexDaemon,
                RecipeFeature::CodexElevatedSandboxExcluded,
            ]) && definition
                .limits
                .iter()
                .any(|limit| limit.role == PathRole::Home)
                && match definition.codex_sandbox {
                    Some(CodexSandbox::Disabled) => true,
                    Some(CodexSandbox::UnelevatedDisclosed) => {
                        required(&[RecipeFeature::CodexDenybinLeakDisclosed])
                    }
                    _ => false,
                }
        }
        ("opencode", HomeMapping::OpenCode) => required(&[
            RecipeFeature::OpenCodeXdgRestorePlugin,
            RecipeFeature::OwnedOpenCodeService,
            RecipeFeature::OpenCodeClaudeReadsDisabled,
            RecipeFeature::EmptyOpenCodeToolHome,
        ]),
        ("pi", HomeMapping::Pi) => {
            required(&[RecipeFeature::PiSkillsIsolated]) && definition.pi_skills.is_some()
        }
        ("hermes_agent", HomeMapping::Hermes) => required(&[
            RecipeFeature::EmptyHermesExternalSkillDirs,
            RecipeFeature::OwnedHermesGateway,
        ]),
        ("deepseek_harness", HomeMapping::DeepSeek) => {
            required(&[RecipeFeature::DeepSeekAgentsIsolated])
        }
        (_, HomeMapping::Synthetic) => descriptor.synthetic,
        _ => false,
    };
    if !valid {
        return Err(PlacementError::IncompleteRecipe);
    }
    if mode.program() == ProgramPlacement::Own {
        if !required(&[
            RecipeFeature::SideBySideVersions,
            RecipeFeature::RollbackVerified,
            RecipeFeature::AutomaticUpdatesBlocked,
            RecipeFeature::ManualUpdatesBlocked,
        ]) {
            return Err(PlacementError::IncompleteRecipe);
        }
        if descriptor.agent.as_str() == "codex"
            && !required(&[
                RecipeFeature::CodexStartupUpdatesBlocked,
                RecipeFeature::CodexUpdateCommandBlocked,
                RecipeFeature::CodexDaemonUpdatesBlocked,
            ])
        {
            return Err(PlacementError::IncompleteRecipe);
        }
        if descriptor.agent.as_str() == "hermes_agent"
            && !required(&[
                RecipeFeature::HermesInstallerQualified,
                RecipeFeature::HermesBundledRuntimeQualified,
            ])
        {
            return Err(PlacementError::IncompleteRecipe);
        }
        if descriptor.agent.as_str() == "pi"
            && (definition.runtime != RuntimeKind::Node
                || !required(&[RecipeFeature::PiNodeAtLeast22_19]))
        {
            return Err(PlacementError::IncompleteRecipe);
        }
    }
    Ok(())
}

fn feature_belongs(
    feature: RecipeFeature,
    mapping: HomeMapping,
    program: ProgramPlacement,
) -> bool {
    use RecipeFeature::*;
    match feature {
        OwnerOnlyGrant | PrivateTemporary | PrivateNodeCompileCache => true,
        ClaudeMcpLogAttempt | ClaudeMcpLogsSuppressed | ClaudeMcpLogLeakDisclosed => {
            mapping == HomeMapping::Claude
        }
        PrivateCodexDaemon | CodexElevatedSandboxExcluded | CodexDenybinLeakDisclosed => {
            mapping == HomeMapping::Codex
        }
        OpenCodeXdgRestorePlugin
        | OwnedOpenCodeService
        | OpenCodeClaudeReadsDisabled
        | EmptyOpenCodeToolHome => mapping == HomeMapping::OpenCode,
        PiSkillsIsolated => mapping == HomeMapping::Pi,
        EmptyHermesExternalSkillDirs | OwnedHermesGateway => mapping == HomeMapping::Hermes,
        DeepSeekAgentsIsolated => mapping == HomeMapping::DeepSeek,
        SideBySideVersions | RollbackVerified | AutomaticUpdatesBlocked | ManualUpdatesBlocked => {
            program == ProgramPlacement::Own
        }
        CodexStartupUpdatesBlocked | CodexUpdateCommandBlocked | CodexDaemonUpdatesBlocked => {
            program == ProgramPlacement::Own && mapping == HomeMapping::Codex
        }
        HermesInstallerQualified | HermesBundledRuntimeQualified => {
            program == ProgramPlacement::Own && mapping == HomeMapping::Hermes
        }
        PiNodeAtLeast22_19 => program == ProgramPlacement::Own && mapping == HomeMapping::Pi,
    }
}

fn own_program_controls(agent: &AgentId) -> Vec<(&'static str, &'static str)> {
    match agent.as_str() {
        "claude_code" => vec![("DISABLE_AUTOUPDATER", "1"), ("DISABLE_UPDATES", "1")],
        "opencode" => vec![("OPENCODE_DISABLE_AUTOUPDATE", "1")],
        "pi" => vec![("PI_OFFLINE", "1"), ("PI_TELEMETRY", "0")],
        _ => Vec::new(),
    }
}

fn private_components(path: &Path) -> Result<Vec<&str>, PlacementError> {
    let raw = path.to_str().ok_or(PlacementError::InvalidPrivatePath)?;
    if raw.len() > MAX_PATH_BYTES || raw.chars().any(char::is_control) {
        return Err(PlacementError::InvalidPrivatePath);
    }
    let raw = raw.strip_prefix(r"\\?\").unwrap_or(raw);
    let bytes = raw.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
        || bytes.iter().skip(2).any(|byte| *byte == b':')
    {
        return Err(PlacementError::InvalidPrivatePath);
    }
    let components: Vec<_> = raw
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect();
    if components.iter().any(|part| matches!(*part, "." | "..")) {
        return Err(PlacementError::InvalidPrivatePath);
    }
    Ok(components)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EvidenceKind;

    fn mode(home: HomePlacement) -> Placement {
        Placement::new(ProgramPlacement::System, home).unwrap()
    }
    fn descriptor(id: &str, version: &str, synthetic: bool) -> AdapterDescriptor {
        let agent = AgentId::new(id).unwrap();
        let version = Version::parse(version).unwrap();
        let mut capabilities = Capabilities::default();
        capabilities
            .qualify(
                Operation::Placement,
                &agent,
                &version,
                synthetic,
                FixtureEvidence::new(
                    agent.clone(),
                    version.clone(),
                    "fixtures/placement-only",
                    if synthetic {
                        EvidenceKind::SyntheticFixture
                    } else {
                        EvidenceKind::InstalledRuntimeFixture
                    },
                )
                .unwrap(),
            )
            .unwrap();
        AdapterDescriptor {
            agent,
            display_name: "Explicit test profile".into(),
            version,
            architecture: if synthetic {
                ConnectionArchitecture::InMemory
            } else {
                ConnectionArchitecture::NativePty
            },
            synthetic,
            capabilities,
        }
    }
    fn fixture_definition() -> RecipeDefinition {
        RecipeDefinition {
            home: HomeMapping::Synthetic,
            runtime: RuntimeKind::Node,
            features: [
                RecipeFeature::OwnerOnlyGrant,
                RecipeFeature::PrivateTemporary,
                RecipeFeature::PrivateNodeCompileCache,
            ]
            .into(),
            limits: vec![],
            pi_skills: None,
            codex_sandbox: None,
        }
    }
    fn qualify(
        descriptor: &AdapterDescriptor,
        mode: Placement,
        definition: RecipeDefinition,
    ) -> QualifiedPlacementRecipe {
        QualifiedPlacementRecipe::qualify(
            descriptor,
            mode,
            definition,
            FixtureEvidence::new(
                descriptor.agent.clone(),
                descriptor.version.clone(),
                "fixtures/placement-recipe",
                if descriptor.synthetic {
                    EvidenceKind::SyntheticFixture
                } else {
                    EvidenceKind::InstalledRuntimeFixture
                },
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn paths() -> PlacementPaths {
        PlacementPaths::from_grant(
            "C:/private/fake".into(),
            vec![
                (PathRole::Home, "C:/private/fake/home".into()),
                (PathRole::Temporary, "C:/private/fake/tmp".into()),
                (PathRole::Cache, "C:/private/fake/cache".into()),
                (
                    PathRole::NodeCompileCache,
                    "C:/private/fake/cache/node".into(),
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn synthetic_recipe_demonstrates_pipeline_without_qualifying_a_native_adapter() {
        let descriptor = descriptor("fake_placement", "1.0.0+fixture", true);
        let recipe = qualify(&descriptor, mode(HomePlacement::Box), fixture_definition());
        let paths = paths();
        let plan = plan_placement(
            &descriptor,
            mode(HomePlacement::Box),
            Some(&paths),
            Some(&recipe),
        )
        .unwrap();
        assert_eq!(
            plan.overrides()
                .iter()
                .map(PlacementOverride::name)
                .collect::<Vec<_>>(),
            vec![
                "TEMP",
                "TMP",
                "NODE_COMPILE_CACHE",
                "SHINATAMA_FIXTURE_HOME"
            ]
        );
        assert!(plan.requires_held_private_grant());
        assert!(!format!("{plan:?} {paths:?}").contains("C:/private"));
        assert!(matches!(plan.program(), ProgramPlan::PreserveSystemBinary));
    }

    #[test]
    fn absent_native_capability_or_recipe_never_means_supported() {
        let mut unqualified = descriptor("codex", "0.164.0", false);
        unqualified.capabilities = Capabilities::default();
        assert!(matches!(
            plan_placement(&unqualified, mode(HomePlacement::Box), None, None),
            Err(PlacementError::Unqualified(_))
        ));
        let descriptor = descriptor("codex", "0.164.0", false);
        assert_eq!(
            plan_placement(&descriptor, mode(HomePlacement::Box), None, None).unwrap_err(),
            PlacementError::MissingRecipe
        );
    }

    #[test]
    fn recipe_cannot_cross_agent_build_or_mode_and_system_home_stays_unchanged() {
        let original = descriptor("fake_placement", "1.0.0+one", true);
        let definition = RecipeDefinition {
            home: HomeMapping::PreserveSystem,
            runtime: RuntimeKind::Native,
            features: BTreeSet::new(),
            limits: vec![],
            pi_skills: None,
            codex_sandbox: None,
        };
        let recipe = qualify(&original, mode(HomePlacement::System), definition);
        let plan =
            plan_placement(&original, mode(HomePlacement::System), None, Some(&recipe)).unwrap();
        assert!(plan.overrides().is_empty());
        assert!(!plan.requires_held_private_grant());
        for other in [
            descriptor("fake_other", "1.0.0+one", true),
            descriptor("fake_placement", "1.0.0+two", true),
        ] {
            assert_eq!(
                plan_placement(&other, mode(HomePlacement::System), None, Some(&recipe))
                    .unwrap_err(),
                PlacementError::Unqualified(RefusalReason::VersionMismatch)
            );
        }
        assert_eq!(
            plan_placement(
                &original,
                mode(HomePlacement::Box),
                Some(&paths()),
                Some(&recipe)
            )
            .unwrap_err(),
            PlacementError::ModeMismatch
        );
        assert!(Placement::new(ProgramPlacement::Own, HomePlacement::System).is_err());
    }

    #[test]
    fn incomplete_private_recipe_or_missing_private_paths_is_refused() {
        let descriptor = descriptor("fake_placement", "1.0.0", true);
        let mut definition = fixture_definition();
        definition
            .features
            .remove(&RecipeFeature::PrivateNodeCompileCache);
        let evidence = FixtureEvidence::new(
            descriptor.agent.clone(),
            descriptor.version.clone(),
            "fixtures/incomplete",
            EvidenceKind::SyntheticFixture,
        )
        .unwrap();
        assert_eq!(
            QualifiedPlacementRecipe::qualify(
                &descriptor,
                mode(HomePlacement::Box),
                definition,
                evidence
            )
            .unwrap_err(),
            PlacementError::IncompleteRecipe
        );
        let recipe = qualify(&descriptor, mode(HomePlacement::Box), fixture_definition());
        let paths = PlacementPaths::from_grant(
            "C:/private/fake".into(),
            vec![(PathRole::Home, "C:/private/fake/home".into())],
        )
        .unwrap();
        assert_eq!(
            plan_placement(
                &descriptor,
                mode(HomePlacement::Box),
                Some(&paths),
                Some(&recipe)
            )
            .unwrap_err(),
            PlacementError::MissingPath(PathRole::Temporary)
        );
    }

    #[test]
    fn path_budgets_apply_the_exact_recipe_unit_without_shortening_or_relocation() {
        let descriptor = descriptor("fake_placement", "1.0.0", true);
        let mut definition = fixture_definition();
        definition.limits.push(PathLimit {
            role: PathRole::Home,
            unit: PathUnit::Utf8Bytes,
            maximum: NonZeroUsize::new(4).unwrap(),
        });
        let recipe = qualify(&descriptor, mode(HomePlacement::Box), definition);
        assert!(matches!(
            plan_placement(
                &descriptor,
                mode(HomePlacement::Box),
                Some(&paths()),
                Some(&recipe)
            ),
            Err(PlacementError::PathBudget {
                unit: PathUnit::Utf8Bytes,
                maximum: 4,
                ..
            })
        ));
        assert_eq!(recipe.home_limit().unwrap().maximum.get(), 4);
        assert!(
            PlacementPaths::from_grant(
                "C:/private".into(),
                vec![(PathRole::Home, "C:/users/real/home".into())]
            )
            .is_err()
        );
        assert!(
            PlacementPaths::from_grant(
                "C:/private".into(),
                vec![(PathRole::Home, "C:/private/../real".into())]
            )
            .is_err()
        );
    }

    #[test]
    fn codex_requires_a_specific_verified_home_budget_and_excludes_elevated_sandbox() {
        let descriptor = descriptor("codex", "0.164.0", false);
        let definition = RecipeDefinition {
            home: HomeMapping::Codex,
            runtime: RuntimeKind::Native,
            features: [
                RecipeFeature::OwnerOnlyGrant,
                RecipeFeature::PrivateTemporary,
                RecipeFeature::PrivateCodexDaemon,
                RecipeFeature::CodexElevatedSandboxExcluded,
            ]
            .into(),
            limits: vec![],
            pi_skills: None,
            codex_sandbox: Some(CodexSandbox::Disabled),
        };
        let proof = || {
            FixtureEvidence::new(
                descriptor.agent.clone(),
                descriptor.version.clone(),
                "fixtures/explicit-synthetic-record-not-runtime-proof",
                EvidenceKind::InstalledRuntimeFixture,
            )
            .unwrap()
        };
        assert_eq!(
            QualifiedPlacementRecipe::qualify(
                &descriptor,
                mode(HomePlacement::Box),
                definition.clone(),
                proof()
            )
            .unwrap_err(),
            PlacementError::IncompleteRecipe
        );
        let mut elevated = definition;
        elevated.codex_sandbox = Some(CodexSandbox::Elevated);
        assert_eq!(
            QualifiedPlacementRecipe::qualify(
                &descriptor,
                mode(HomePlacement::Box),
                elevated,
                proof()
            )
            .unwrap_err(),
            PlacementError::UnsupportedIsolation
        );
    }

    #[test]
    fn owned_program_needs_update_rollback_proof_and_managed_version_path() {
        let descriptor = descriptor("fake_placement", "1.0.0", true);
        let own = Placement::new(ProgramPlacement::Own, HomePlacement::Box).unwrap();
        let mut definition = fixture_definition();
        for item in [
            RecipeFeature::SideBySideVersions,
            RecipeFeature::RollbackVerified,
            RecipeFeature::AutomaticUpdatesBlocked,
            RecipeFeature::ManualUpdatesBlocked,
        ] {
            definition.features.insert(item);
        }
        let recipe = qualify(&descriptor, own, definition);
        assert_eq!(
            plan_placement(&descriptor, own, Some(&paths()), Some(&recipe)).unwrap_err(),
            PlacementError::MissingPath(PathRole::ManagedVersion)
        );
        assert_eq!(default_placement(true).program(), ProgramPlacement::System);
        assert_eq!(default_placement(false).program(), ProgramPlacement::Own);
        assert_eq!(default_placement(false).home(), HomePlacement::Box);
    }

    #[test]
    fn utf16_and_utf8_limits_are_distinct_recipe_units() {
        let descriptor = descriptor("fake_placement", "1.0.0", true);
        let home = "C:/private/fake/界";
        let paths = PlacementPaths::from_grant(
            "C:/private/fake".into(),
            vec![
                (PathRole::Home, home.into()),
                (PathRole::Temporary, "C:/private/fake/tmp".into()),
                (
                    PathRole::NodeCompileCache,
                    "C:/private/fake/cache/node".into(),
                ),
            ],
        )
        .unwrap();
        let mut definition = fixture_definition();
        let maximum = NonZeroUsize::new(home.encode_utf16().count()).unwrap();
        definition.limits.push(PathLimit {
            role: PathRole::Home,
            unit: PathUnit::Utf16Units,
            maximum,
        });
        let utf16 = qualify(&descriptor, mode(HomePlacement::Box), definition.clone());
        assert!(
            plan_placement(
                &descriptor,
                mode(HomePlacement::Box),
                Some(&paths),
                Some(&utf16)
            )
            .is_ok()
        );
        definition.limits[0].unit = PathUnit::Utf8Bytes;
        let utf8 = qualify(&descriptor, mode(HomePlacement::Box), definition);
        assert!(matches!(
            plan_placement(
                &descriptor,
                mode(HomePlacement::Box),
                Some(&paths),
                Some(&utf8)
            ),
            Err(PlacementError::PathBudget {
                unit: PathUnit::Utf8Bytes,
                ..
            })
        ));
    }

    #[test]
    fn system_home_cannot_smuggle_box_config_or_foreign_adapter_actions() {
        let descriptor = descriptor("fake_placement", "1.0.0", true);
        let definition = RecipeDefinition {
            home: HomeMapping::PreserveSystem,
            runtime: RuntimeKind::Native,
            features: [RecipeFeature::EmptyHermesExternalSkillDirs].into(),
            limits: vec![],
            pi_skills: None,
            codex_sandbox: None,
        };
        let proof = FixtureEvidence::new(
            descriptor.agent.clone(),
            descriptor.version.clone(),
            "fixtures/system-must-stay-unchanged",
            EvidenceKind::SyntheticFixture,
        )
        .unwrap();
        assert_eq!(
            QualifiedPlacementRecipe::qualify(
                &descriptor,
                mode(HomePlacement::System),
                definition,
                proof
            )
            .unwrap_err(),
            PlacementError::IncompleteRecipe
        );
        let mut wrong_agent_feature = fixture_definition();
        wrong_agent_feature
            .features
            .insert(RecipeFeature::EmptyHermesExternalSkillDirs);
        let proof = FixtureEvidence::new(
            descriptor.agent.clone(),
            descriptor.version.clone(),
            "fixtures/foreign-requirement",
            EvidenceKind::SyntheticFixture,
        )
        .unwrap();
        assert_eq!(
            QualifiedPlacementRecipe::qualify(
                &descriptor,
                mode(HomePlacement::Box),
                wrong_agent_feature,
                proof
            )
            .unwrap_err(),
            PlacementError::IncompleteRecipe
        );
    }

    #[test]
    fn opencode_tool_home_missing_from_this_grant_is_not_invented() {
        // A declarative test record exercises refusal only; it is not an installed
        // OpenCode conformance result or a registered native adapter.
        let descriptor = descriptor("opencode", "2.1.4", false);
        let definition = RecipeDefinition {
            home: HomeMapping::OpenCode,
            runtime: RuntimeKind::Native,
            features: [
                RecipeFeature::OwnerOnlyGrant,
                RecipeFeature::PrivateTemporary,
                RecipeFeature::OpenCodeXdgRestorePlugin,
                RecipeFeature::OwnedOpenCodeService,
                RecipeFeature::OpenCodeClaudeReadsDisabled,
                RecipeFeature::EmptyOpenCodeToolHome,
            ]
            .into(),
            limits: vec![],
            pi_skills: None,
            codex_sandbox: None,
        };
        let recipe = qualify(&descriptor, mode(HomePlacement::Box), definition);
        let paths = PlacementPaths::from_grant(
            "C:/private/opencode".into(),
            vec![
                (PathRole::Home, "C:/private/opencode/home".into()),
                (PathRole::Temporary, "C:/private/opencode/tmp".into()),
                (
                    PathRole::OpenCodeDatabase,
                    "C:/private/opencode/home/opencode.db".into(),
                ),
                (
                    PathRole::XdgConfig,
                    "C:/private/opencode/home/xdg/config".into(),
                ),
                (
                    PathRole::XdgData,
                    "C:/private/opencode/home/xdg/data".into(),
                ),
                (PathRole::XdgCache, "C:/private/opencode/cache/xdg".into()),
                (
                    PathRole::XdgState,
                    "C:/private/opencode/home/xdg/state".into(),
                ),
            ],
        )
        .unwrap();
        assert_eq!(
            plan_placement(
                &descriptor,
                mode(HomePlacement::Box),
                Some(&paths),
                Some(&recipe)
            )
            .unwrap_err(),
            PlacementError::MissingPath(PathRole::OpenCodeToolHome)
        );
    }

    #[test]
    fn synthetic_fixture_evidence_never_qualifies_a_native_recipe() {
        let descriptor = descriptor("codex", "0.164.0", false);
        let evidence = FixtureEvidence::new(
            descriptor.agent.clone(),
            descriptor.version.clone(),
            "fixtures/synthetic-only",
            EvidenceKind::SyntheticFixture,
        )
        .unwrap();
        assert_eq!(
            QualifiedPlacementRecipe::qualify(
                &descriptor,
                mode(HomePlacement::Box),
                fixture_definition(),
                evidence
            )
            .unwrap_err(),
            PlacementError::Unqualified(RefusalReason::SyntheticEvidenceForNativeAdapter)
        );
    }
}
