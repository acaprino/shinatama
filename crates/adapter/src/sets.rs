//! Per-instance set references and pure materialization plans. No marketplace,
//! installation, configuration read/write, shell, credential or process access.
//! A fixture recipe records evidence; only the later launch boundary can admit it.
use crate::launch::{
    arguments::CommandPlan,
    environment::ChildEnvironment,
    placement::{PathRole, PlacementPaths, PlacementPlan},
};
use crate::{
    AdapterDescriptor, AgentId, Capabilities, ConnectionArchitecture, FixtureEvidence,
    HomePlacement, Operation, Placement, RefusalReason, Version,
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

pub const SET_SCHEMA_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetKind {
    Plugin,
    Skill,
    Mcp,
}

/// Catalog references only: an ID cannot supply an executable, path or settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SetEntry {
    kind: SetKind,
    id: String,
    revision: u64,
}
impl SetEntry {
    pub fn new(kind: SetKind, id: &str, revision: u64) -> Result<Self, SetError> {
        if !identifier(id) || revision == 0 {
            return Err(SetError::InvalidManifest);
        }
        Ok(Self {
            kind,
            id: id.into(),
            revision,
        })
    }
    pub fn kind(&self) -> SetKind {
        self.kind
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetManifest {
    schema_version: u32,
    id: String,
    entries: Vec<SetEntry>,
}
impl SetManifest {
    pub fn new(
        schema_version: u32,
        id: &str,
        mut entries: Vec<SetEntry>,
    ) -> Result<Self, SetError> {
        if schema_version != SET_SCHEMA_VERSION || !identifier(id) {
            return Err(SetError::InvalidManifest);
        }
        if entries.len() > MAX_ENTRIES {
            return Err(SetError::LimitExceeded);
        }
        let mut seen = BTreeSet::new();
        for entry in &entries {
            if !seen.insert(&entry.id) {
                return Err(SetError::ConflictingEntry);
            }
        }
        // Canonical ordering makes a selection stable across UI list ordering.
        entries.sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
        Ok(Self {
            schema_version,
            id: id.into(),
            entries,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn entries(&self) -> &[SetEntry] {
        &self.entries
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        && !matches!(value, "." | "..")
}

#[derive(Clone, Copy, Debug)]
pub enum SetSelection<'a> {
    Exact(&'a SetManifest),
    InheritHome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetScope {
    PerInstance,
    InheritHome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetError {
    InvalidManifest,
    ConflictingEntry,
    LimitExceeded,
    MissingRecipe,
    Unqualified(RefusalReason),
    UnsupportedSelection,
    SelectionMismatch,
    LaunchMismatch,
    PrivateHomeRequired,
    InvalidInstance,
}

/// Trusted adapter code binds exact set contents, mode, build and fixture kind.
/// This is deliberately not deserializable; a caller cannot submit a recipe.
#[derive(Clone)]
pub struct QualifiedSetRecipe {
    agent: AgentId,
    version: Version,
    synthetic: bool,
    placement: Placement,
    scope: SetScope,
    manifest: Option<SetManifest>,
    evidence: Capabilities,
}
impl QualifiedSetRecipe {
    pub fn qualify(
        descriptor: &AdapterDescriptor,
        placement: Placement,
        selection: SetSelection<'_>,
        supported_kinds: BTreeSet<SetKind>,
        evidence: FixtureEvidence,
    ) -> Result<Self, SetError> {
        check_descriptor(descriptor)?;
        let (scope, manifest) = match selection {
            SetSelection::Exact(manifest) => (SetScope::PerInstance, Some(manifest)),
            SetSelection::InheritHome => (SetScope::InheritHome, None),
        };
        // FRD-0002/R24/R27: these modes keep the home's existing set and must
        // not claim that an empty selection excludes its existing components.
        if scope == SetScope::PerInstance
            && (descriptor.agent.as_str() == "hermes_agent"
                || (descriptor.agent.as_str() == "codex"
                    && placement.home() == HomePlacement::System))
        {
            return Err(SetError::UnsupportedSelection);
        }
        if let Some(manifest) = manifest {
            for entry in &manifest.entries {
                if !supported_kinds.contains(&entry.kind) {
                    return Err(SetError::UnsupportedSelection);
                }
                // DeepSeek skills follow the box/project, not the launch overlay.
                if descriptor.agent.as_str() == "deepseek_harness" && entry.kind == SetKind::Skill {
                    return Err(SetError::UnsupportedSelection);
                }
            }
        }
        let mut qualified = Capabilities::default();
        qualified
            .qualify(
                Operation::InstanceSet,
                &descriptor.agent,
                &descriptor.version,
                descriptor.synthetic,
                evidence.clone(),
            )
            .map_err(SetError::Unqualified)?;
        if manifest.is_some_and(|manifest| {
            manifest
                .entries
                .iter()
                .any(|item| item.kind == SetKind::Mcp)
        }) {
            descriptor
                .capabilities
                .require(Operation::McpInject)
                .map_err(SetError::Unqualified)?;
            qualified
                .qualify(
                    Operation::McpInject,
                    &descriptor.agent,
                    &descriptor.version,
                    descriptor.synthetic,
                    evidence,
                )
                .map_err(SetError::Unqualified)?;
        }
        Ok(Self {
            agent: descriptor.agent.clone(),
            version: descriptor.version.clone(),
            synthetic: descriptor.synthetic,
            placement,
            scope,
            manifest: manifest.cloned(),
            evidence: qualified,
        })
    }
}
impl fmt::Debug for QualifiedSetRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QualifiedSetRecipe")
            .field("agent", &self.agent)
            .field("version", &self.version)
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}
fn check_descriptor(descriptor: &AdapterDescriptor) -> Result<(), SetError> {
    if descriptor.synthetic != matches!(descriptor.architecture, ConnectionArchitecture::InMemory)
        || descriptor.synthetic != descriptor.agent.as_str().starts_with("fake_")
    {
        return Err(SetError::Unqualified(RefusalReason::VersionMismatch));
    }
    descriptor
        .capabilities
        .validate_for(&descriptor.agent, &descriptor.version, descriptor.synthetic)
        .and_then(|()| descriptor.capabilities.require(Operation::InstanceSet))
        .map_err(SetError::Unqualified)
}

pub struct SetLaunchContext<'a> {
    pub placement: &'a PlacementPlan,
    pub command: &'a CommandPlan,
    pub environment: &'a ChildEnvironment,
    /// Logical paths from a private grant that the caller must keep alive.
    pub private_paths: Option<&'a PlacementPaths>,
    pub instance_id: &'a str,
}

/// Declarative destination and exact launch references, never a write permission.
/// The writer must resolve catalog entries, use create_new under a held private
/// grant, then prepare a qualified config/argv recipe; no system home is a target.
pub struct SetPlan<'a> {
    agent: AgentId,
    version: Version,
    mode: Placement,
    instance: String,
    scope: SetScope,
    manifest: Option<&'a SetManifest>,
    destination: Option<PathBuf>,
    command: &'a CommandPlan,
    environment: &'a ChildEnvironment,
}
impl SetPlan<'_> {
    pub fn agent(&self) -> &AgentId {
        &self.agent
    }
    pub fn version(&self) -> &Version {
        &self.version
    }
    pub fn scope(&self) -> SetScope {
        self.scope
    }
    pub fn manifest(&self) -> Option<&SetManifest> {
        self.manifest
    }
    pub fn destination(&self) -> Option<&Path> {
        self.destination.as_deref()
    }
    pub fn mounted_mcp(&self) -> impl Iterator<Item = &SetEntry> {
        self.manifest
            .into_iter()
            .flat_map(|set| set.entries.iter())
            .filter(|entry| entry.kind == SetKind::Mcp)
    }
    pub fn matches_launch(
        &self,
        placement: &PlacementPlan,
        command: &CommandPlan,
        environment: &ChildEnvironment,
        instance: &str,
    ) -> bool {
        self.agent == *placement.agent()
            && self.version == *placement.version()
            && self.mode == placement.placement()
            && self.instance == instance
            && command.agent() == self.command.agent()
            && command.version() == self.command.version()
            && command.operation() == self.command.operation()
            && command.executable() == self.command.executable()
            && command.argv() == self.command.argv()
            && command.working_directory() == self.command.working_directory()
            && environment.entries().eq(self.environment.entries())
    }
}
impl fmt::Debug for SetPlan<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetPlan")
            .field("agent", &self.agent)
            .field("scope", &self.scope)
            .field(
                "entry_count",
                &self.manifest.map_or(0, |set| set.entries.len()),
            )
            .finish_non_exhaustive()
    }
}

pub fn plan_set<'a>(
    descriptor: &AdapterDescriptor,
    selection: SetSelection<'a>,
    recipe: Option<&QualifiedSetRecipe>,
    context: SetLaunchContext<'a>,
) -> Result<SetPlan<'a>, SetError> {
    check_descriptor(descriptor)?;
    let recipe = recipe.ok_or(SetError::MissingRecipe)?;
    recipe
        .evidence
        .validate_for(&descriptor.agent, &descriptor.version, descriptor.synthetic)
        .map_err(SetError::Unqualified)?;
    if recipe.agent != descriptor.agent
        || recipe.version != descriptor.version
        || recipe.synthetic != descriptor.synthetic
    {
        return Err(SetError::Unqualified(RefusalReason::VersionMismatch));
    }
    let manifest = match selection {
        SetSelection::Exact(manifest) => Some(manifest),
        SetSelection::InheritHome => None,
    };
    if recipe.manifest.as_ref() != manifest {
        return Err(SetError::SelectionMismatch);
    }
    if recipe.placement != context.placement.placement()
        || context.placement.agent() != &descriptor.agent
        || context.placement.version() != &descriptor.version
        || context.command.agent() != &descriptor.agent
        || context.command.version() != &descriptor.version
        || !matches!(
            context.command.operation(),
            Operation::StartNew | Operation::ResumeNative
        )
    {
        return Err(SetError::LaunchMismatch);
    }
    if context.instance_id.len() != 14
        || !context.instance_id.starts_with("i_")
        || !context.instance_id.as_bytes()[2..]
            .iter()
            .all(|b| b"abcdefghijklmnopqrstuvwxyz234567".contains(b))
    {
        return Err(SetError::InvalidInstance);
    }
    if !context.environment.entries().any(|(name, value)| {
        name.eq_ignore_ascii_case("SHINATAMA_INSTANCE") && value == context.instance_id
    }) {
        return Err(SetError::LaunchMismatch);
    }
    let destination = if manifest.is_some() {
        let home = context
            .private_paths
            .and_then(|paths| paths.path(PathRole::Home))
            .ok_or(SetError::PrivateHomeRequired)?;
        Some(
            home.join(".shinatama")
                .join("instances")
                .join(context.instance_id)
                .join("set-manifest.json"),
        )
    } else {
        None
    };
    Ok(SetPlan {
        agent: descriptor.agent.clone(),
        version: descriptor.version.clone(),
        mode: recipe.placement,
        instance: context.instance_id.into(),
        scope: recipe.scope,
        manifest,
        destination,
        command: context.command,
        environment: context.environment,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::{
        arguments::{CommandForm, QualifiedArgvRecipe, plan_arguments},
        environment::{ChildEnvironmentPlan, ChildOverride, OverrideName},
        placement::{
            HomeMapping, QualifiedPlacementRecipe, RecipeDefinition, RuntimeKind, plan_placement,
        },
    };
    use crate::{EvidenceKind, ProgramPlacement};
    use std::collections::BTreeMap;
    fn descriptor() -> AdapterDescriptor {
        let agent = AgentId::new("fake_sets").unwrap();
        let version = Version::parse("1.0.0+one").unwrap();
        let mut capabilities = Capabilities::default();
        for op in [
            Operation::InstanceSet,
            Operation::McpInject,
            Operation::Placement,
            Operation::StartNew,
        ] {
            capabilities
                .qualify(
                    op,
                    &agent,
                    &version,
                    true,
                    FixtureEvidence::new(
                        agent.clone(),
                        version.clone(),
                        "sets/fixture",
                        crate::EvidenceKind::SyntheticFixture,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        AdapterDescriptor {
            agent,
            version,
            display_name: "Synthetic sets".into(),
            synthetic: true,
            architecture: ConnectionArchitecture::InMemory,
            capabilities,
        }
    }
    fn evidence(d: &AdapterDescriptor) -> FixtureEvidence {
        FixtureEvidence::new(
            d.agent.clone(),
            d.version.clone(),
            "sets/fixture",
            EvidenceKind::SyntheticFixture,
        )
        .unwrap()
    }
    #[test]
    fn empty_set_without_daodan_has_a_private_plan_and_no_mounted_servers() {
        let d = descriptor();
        let manifest = SetManifest::new(1, "empty", vec![]).unwrap();
        let mode = Placement::new(ProgramPlacement::System, HomePlacement::Box).unwrap();
        let paths = PlacementPaths::from_grant(
            "C:/private/agent".into(),
            vec![
                (PathRole::Home, "C:/private/agent/home".into()),
                (PathRole::Temporary, "C:/private/agent/tmp".into()),
                (PathRole::Cache, "C:/private/agent/cache".into()),
            ],
        )
        .unwrap();
        let placement_recipe = QualifiedPlacementRecipe::qualify(
            &d,
            mode,
            RecipeDefinition {
                home: HomeMapping::Synthetic,
                runtime: RuntimeKind::Native,
                features: [
                    crate::launch::placement::RecipeFeature::OwnerOnlyGrant,
                    crate::launch::placement::RecipeFeature::PrivateTemporary,
                ]
                .into_iter()
                .collect(),
                limits: vec![],
                pi_skills: None,
                codex_sandbox: None,
            },
            evidence(&d),
        )
        .unwrap();
        let placement = plan_placement(&d, mode, Some(&paths), Some(&placement_recipe)).unwrap();
        let argv_recipe = QualifiedArgvRecipe::qualify(
            &d,
            Operation::StartNew,
            CommandForm::NativeExe("C:/fixture/agent.exe".into()),
            vec![],
            evidence(&d),
        )
        .unwrap();
        let command = plan_arguments(
            &d,
            Operation::StartNew,
            Path::new("C:/work"),
            Some(&argv_recipe),
        )
        .unwrap();
        let environment = ChildEnvironmentPlan::build(
            &[],
            None,
            None,
            vec![ChildOverride::new(OverrideName::Instance, "i_abcdefghijkl".into()).unwrap()],
        )
        .unwrap()
        .child_from(&BTreeMap::new())
        .unwrap();
        let recipe = QualifiedSetRecipe::qualify(
            &d,
            mode,
            SetSelection::Exact(&manifest),
            BTreeSet::new(),
            evidence(&d),
        )
        .unwrap();
        let plan = plan_set(
            &d,
            SetSelection::Exact(&manifest),
            Some(&recipe),
            SetLaunchContext {
                placement: &placement,
                command: &command,
                environment: &environment,
                private_paths: Some(&paths),
                instance_id: "i_abcdefghijkl",
            },
        )
        .unwrap();
        assert!(plan.manifest().unwrap().is_empty());
        assert_eq!(plan.mounted_mcp().count(), 0);
        assert!(
            plan.destination()
                .unwrap()
                .starts_with(paths.path(PathRole::Home).unwrap())
        );
        assert!(plan.matches_launch(&placement, &command, &environment, "i_abcdefghijkl"));
    }
    #[test]
    fn conflicting_or_unsupported_selections_do_not_produce_a_partial_manifest_or_recipe() {
        let entry = SetEntry::new(SetKind::Plugin, "same", 1).unwrap();
        assert_eq!(
            SetManifest::new(1, "conflict", vec![entry.clone(), entry]),
            Err(SetError::ConflictingEntry)
        );
        assert_eq!(
            SetManifest::new(999, "future", vec![]),
            Err(SetError::InvalidManifest)
        );
        let d = descriptor();
        let manifest = SetManifest::new(
            1,
            "skills",
            vec![SetEntry::new(SetKind::Skill, "sample", 1).unwrap()],
        )
        .unwrap();
        let mode = Placement::new(ProgramPlacement::System, HomePlacement::Box).unwrap();
        assert_eq!(
            QualifiedSetRecipe::qualify(
                &d,
                mode,
                SetSelection::Exact(&manifest),
                BTreeSet::new(),
                evidence(&d)
            )
            .unwrap_err(),
            SetError::UnsupportedSelection
        );
        let mut other = d.clone();
        other.version = Version::parse("1.0.0+different").unwrap();
        assert_eq!(
            QualifiedSetRecipe::qualify(
                &other,
                mode,
                SetSelection::Exact(&manifest),
                [SetKind::Skill].into_iter().collect(),
                evidence(&d)
            )
            .unwrap_err(),
            SetError::Unqualified(RefusalReason::VersionMismatch)
        );
        assert_eq!(manifest.entries().len(), 1);
    }
}
