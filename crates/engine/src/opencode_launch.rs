//! Windows System+Box OpenCode startup with its owned standalone service.
//! Preparation performs bounded local
//! reads/writes; only the owned ConPTY canary can qualify the exact build/profile.
//! No provider request, login, user credential read, shell or marketplace install.
#![cfg(windows)]
use crate::{
    marionette::{Host, Marionette, is_marionette_set},
    launch::{
        LaunchInputs, LaunchIssueCode, PreparedLaunch, compose_launch, identity::LaunchIdentity,
    },
    launch_policy::LaunchPolicy,
    native_launch::{
        NativeAdmissionProfile, NativeLaunchBinding, NativeLaunchError, NativeLaunchPins,
        NativeProgramIdentity, NativeTrialReceipt, NativeTrialTracker, NativeTuiPermit,
    },
    opencode_probe::{OpenCodeProbeOutcome, ProbeCleanup},
    sidecar_supervisor::SidecarSupervisor,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use shinatama_adapter::{
    AdapterDescriptor, AgentId, Capabilities, ConnectionArchitecture, EvidenceKind,
    FixtureEvidence, HomePlacement, Operation, Placement, ProgramPlacement, Version,
    catalog::{ProgramCandidate, ProgramKind},
    launch::{
        arguments::{ArgvError, CommandForm, QualifiedArgvRecipe},
        placement::{
            HomeMapping, PathRole, PlacementError, PlacementPaths, QualifiedPlacementRecipe,
            RecipeDefinition, RecipeFeature, RuntimeKind,
        },
    },
    sets::SetManifest,
};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    records::{RootAccess, RootKind, RootScope, StoreRootRecord},
    resource_reservations::Budget,
    store_roots::{PreparedPrivateStore, StorePathRole},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    num::NonZeroU16,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
};

const MIB: u64 = 1024 * 1024;
const PREPARE_BUDGET: Duration = Duration::from_secs(10);
const RECIPE: &str = "opencode-system-box-standalone-tui-v1";
// D17's reviewed 2.x shell hook. Fixed launcher artifact, never a plugin chosen
// by IPC. Its pinned file is kept for the lifetime of this exact invocation.
const RESTORE_ENV_PLUGIN: &str = r#"const NAMES = ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"];
export default {
  id: "shinatama.restore-user-env",
  setup(ctx) {
    ctx.shell.hook("create.before", (invocation) => {
      for (const name of NAMES) {
        const original = invocation.env[`SHN_USER_${name}`];
        if (original !== undefined) invocation.env[name] = original;
        else delete invocation.env[name];
      }
    });
  },
};
"#;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenCodeLaunchError {
    InvalidCandidate,
    ObservationRequired,
    ProgramChanged,
    UnsafeProgram,
    Expired,
    PrivateHome,
    RootChanged,
    InvalidProject,
    SidecarUnavailable,
    InvalidCa,
    UnsupportedSet,
    BundledPlugins,
    Configuration,
    InheritedEnvironment,
    Descriptor,
    Evidence,
    PlacementMode,
    MissingPrivatePath(PathRole),
    PlacementRecipe(PlacementError),
    ArgumentsRecipe(ArgvError),
    Composition(LaunchIssueCode),
    Identity,
    QualificationRequired,
    Permit,
}
impl std::fmt::Display for OpenCodeLaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OpenCode startup prerequisite: {self:?}.")
    }
}
impl std::error::Error for OpenCodeLaunchError {}
impl From<NativeLaunchError> for OpenCodeLaunchError {
    fn from(_: NativeLaunchError) -> Self {
        Self::Permit
    }
}

/// Trusted backend values only. No Deserialize, custom argv, environment, model,
/// credentials, resource budget or qualification boolean is accepted from IPC.
pub struct OpenCodeStartContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a OpenCodeProbeOutcome,
    pub base: &'a StorageLocation,
    pub database: &'a Database,
    pub private_home: PreparedPrivateStore,
    pub root: StoreRootRecord,
    pub project: &'a Path,
    pub sidecar: &'a SidecarSupervisor,
    pub proxy_port: NonZeroU16,
    pub ca_file: &'a Path,
    pub selected_set: &'a SetManifest,
    /// Synced Marionette store; required when the set is the Marionette one.
    pub plugins_root: Option<&'a Path>,
    pub thread_id: &'a str,
}
pub struct PreparedOpenCodeTrial {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    pub tracker: NativeTrialTracker,
}
pub struct OpenCodeTrialContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a OpenCodeProbeOutcome,
    pub base: &'a StorageLocation,
    pub trial_base: &'a StorageLocation,
    pub private_home: PreparedPrivateStore,
    pub project: &'a Path,
    pub sidecar: &'a SidecarSupervisor,
    pub proxy_port: NonZeroU16,
    pub ca_file: &'a Path,
    pub selected_set: &'a SetManifest,
    pub thread_id: &'a str,
}
struct Context<'a> {
    candidate: &'a ProgramCandidate,
    observation: &'a OpenCodeProbeOutcome,
    base: &'a StorageLocation,
    database: Option<&'a Database>,
    trial_base: Option<&'a StorageLocation>,
    private_home: PreparedPrivateStore,
    root: Option<StoreRootRecord>,
    project: &'a Path,
    sidecar: &'a SidecarSupervisor,
    proxy_port: NonZeroU16,
    ca_file: &'a Path,
    selected_set: &'a SetManifest,
    plugins_root: Option<&'a Path>,
    thread_id: &'a str,
}
pub struct PreparedOpenCodeStart {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
}

fn fixed_arguments() -> Vec<OsString> {
    vec!["--standalone".into()]
}

/// The inline config of a real session. Full access adds one allow-all rule so
/// no tool asks; without it OpenCode keeps its own native permission prompts.
fn config_content(full_access: bool) -> &'static str {
    if full_access {
        r#"{"update":"disable","share":"disabled","permissions":[{"action":"*","resource":"*","effect":"allow"}]}"#
    } else {
        r#"{"update":"disable","share":"disabled"}"#
    }
}

/// This is the fixed finite trial profile, not inferred from a growing
/// process and never raised on failure. Issuance is sealed inside this qualifier.
fn profile() -> Result<NativeAdmissionProfile, OpenCodeLaunchError> {
    Ok(NativeAdmissionProfile::new(
        RECIPE,
        1,
        Budget {
            memory_bytes: 1024 * MIB,
            processes: 8,
            cpu_basis_points: 1000,
            queue_bytes: 4 * MIB,
            queue_items: 512,
        },
        768 * MIB,
    )?)
}
pub fn prepare_trial(
    context: OpenCodeTrialContext<'_>,
) -> Result<PreparedOpenCodeTrial, OpenCodeLaunchError> {
    // Filename presence only: never read a credential store for this canary.
    if context
        .private_home
        .paths()
        .home()
        .join("auth.json")
        .try_exists()
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?
    {
        return Err(OpenCodeLaunchError::PrivateHome);
    }
    let prepared = prepare(
        Context {
            candidate: context.candidate,
            observation: context.observation,
            base: context.base,
            database: None,
            trial_base: Some(context.trial_base),
            private_home: context.private_home,
            root: None,
            project: context.project,
            sidecar: context.sidecar,
            proxy_port: context.proxy_port,
            ca_file: context.ca_file,
            selected_set: context.selected_set,
            plugins_root: None,
            thread_id: context.thread_id,
        },
        None,
        true,
    )?;
    let (permit, tracker) = NativeTuiPermit::issue_trial(
        &prepared.launch,
        prepared.binding,
        prepared.profile,
        prepared.pins,
    )?;
    Ok(PreparedOpenCodeTrial {
        launch: prepared.launch,
        permit,
        tracker,
    })
}
pub fn prepare_start(
    context: OpenCodeStartContext<'_>,
    receipt: &NativeTrialReceipt,
) -> Result<PreparedOpenCodeStart, OpenCodeLaunchError> {
    let prepared = prepare(
        Context {
            candidate: context.candidate,
            observation: context.observation,
            base: context.base,
            database: Some(context.database),
            trial_base: None,
            private_home: context.private_home,
            root: Some(context.root),
            project: context.project,
            sidecar: context.sidecar,
            proxy_port: context.proxy_port,
            ca_file: context.ca_file,
            selected_set: context.selected_set,
            plugins_root: context.plugins_root,
            thread_id: context.thread_id,
        },
        Some(receipt),
        false,
    )?;
    let permit = NativeTuiPermit::issue_after_trial(
        &prepared.launch,
        prepared.binding,
        prepared.profile,
        prepared.pins,
        receipt,
    )?;
    Ok(PreparedOpenCodeStart {
        launch: prepared.launch,
        permit,
    })
}
struct Prepared {
    launch: PreparedLaunch,
    binding: NativeLaunchBinding,
    profile: NativeAdmissionProfile,
    pins: NativeLaunchPins,
}

fn prepare(
    context: Context<'_>,
    receipt: Option<&NativeTrialReceipt>,
    trial: bool,
) -> Result<Prepared, OpenCodeLaunchError> {
    let deadline = Instant::now() + PREPARE_BUDGET;
    if context.candidate.agent.as_str() != "opencode"
        || context.candidate.kind != ProgramKind::DirectExe
    {
        return Err(OpenCodeLaunchError::InvalidCandidate);
    }
    let observation = context.observation.summary();
    let version = observation
        .version
        .clone()
        .ok_or(OpenCodeLaunchError::ObservationRequired)?;
    if observation.issue.is_some()
        || observation.cleanup != ProbeCleanup::Released
        || context.observation.cleanup_retained()
    {
        return Err(OpenCodeLaunchError::ObservationRequired);
    }
    // Empty set, or exactly Marionette for a real session (never the trial).
    // Digests are checked at every launch; files stay where verified.
    let bundled = if context.selected_set.is_empty() {
        None
    } else if is_marionette_set(context.selected_set) && !trial {
        let root = context
            .plugins_root
            .ok_or(OpenCodeLaunchError::BundledPlugins)?;
        Some(
            Marionette::current(root)
                .and_then(|release| release.verify(Host::OpenCode))
                .map_err(|_| OpenCodeLaunchError::BundledPlugins)?,
        )
    } else {
        return Err(OpenCodeLaunchError::UnsupportedSet);
    };
    verify_sidecar(context.sidecar)?;
    context
        .base
        .verify_private_directory()
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    let root = if let (Some(database), Some(expected)) = (context.database, context.root.as_ref()) {
        let root = database
            .store_root(&expected.id)
            .map_err(|_| OpenCodeLaunchError::RootChanged)?
            .filter(|root| root == expected)
            .ok_or(OpenCodeLaunchError::RootChanged)?;
        if root.harness != "opencode"
            || root.kind != RootKind::Box
            || root.scope != RootScope::Primary
            || root.access != RootAccess::ReadWrite
            || !root.available
            || !same_path(
                Path::new(&root.resolved_path),
                context.private_home.paths().home(),
            )?
        {
            return Err(OpenCodeLaunchError::RootChanged);
        }
        Some(root)
    } else if trial && context.trial_base.is_some() {
        None
    } else {
        return Err(OpenCodeLaunchError::RootChanged);
    };
    let project = context
        .project
        .canonicalize()
        .map_err(|_| OpenCodeLaunchError::InvalidProject)?;
    if !project.is_dir() {
        return Err(OpenCodeLaunchError::InvalidProject);
    }
    if let Some(trial_base) = context.trial_base {
        trial_base
            .verify_private_directory()
            .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
        let relative = trial_base
            .directory()
            .strip_prefix(context.base.directory())
            .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
        let name = relative.to_str().ok_or(OpenCodeLaunchError::PrivateHome)?;
        let suffix = name
            .strip_prefix("opencode-tui-trial-")
            .ok_or(OpenCodeLaunchError::PrivateHome)?;
        let trial_id =
            uuid::Uuid::parse_str(suffix).map_err(|_| OpenCodeLaunchError::PrivateHome)?;
        if trial_id.get_version() != Some(uuid::Version::Random)
            || !same_path(&project, &trial_base.directory().join("project"))?
            || !same_path(
                context.private_home.paths().root(),
                &trial_base.directory().join("agents/opencode"),
            )?
        {
            return Err(OpenCodeLaunchError::PrivateHome);
        }
    } else {
        context
            .base
            .validate_project(&project)
            .map_err(|_| OpenCodeLaunchError::InvalidProject)?;
    }
    let mut pinned = pin_program(&context.candidate.path, deadline)?;
    if pinned.hash != observation.executable_sha256 || pinned.bytes != observation.executable_bytes
    {
        return Err(OpenCodeLaunchError::ProgramChanged);
    }
    let resource_profile = profile()?;
    if let Some(receipt) = receipt {
        if !receipt.matches(
            &context.candidate.agent,
            &pinned.identity,
            &version,
            &resource_profile,
        ) {
            return Err(OpenCodeLaunchError::QualificationRequired);
        }
    } else if !trial {
        return Err(OpenCodeLaunchError::QualificationRequired);
    }
    let identity =
        LaunchIdentity::generate(context.thread_id).map_err(|_| OpenCodeLaunchError::Identity)?;
    let mut path_values: Vec<_> = context
        .private_home
        .paths()
        .iter()
        .map(|(role, path)| {
            (
                match role {
                    StorePathRole::Home => PathRole::Home,
                    StorePathRole::Temporary => PathRole::Temporary,
                    StorePathRole::Cache => PathRole::Cache,
                    StorePathRole::NodeCompileCache => PathRole::NodeCompileCache,
                    StorePathRole::XdgConfig => PathRole::XdgConfig,
                    StorePathRole::XdgData => PathRole::XdgData,
                    StorePathRole::XdgCache => PathRole::XdgCache,
                    StorePathRole::XdgState => PathRole::XdgState,
                    StorePathRole::OpenCodeDatabase => PathRole::OpenCodeDatabase,
                    StorePathRole::DeepSeekAgents => PathRole::DeepSeekAgents,
                },
                path.to_owned(),
            )
        })
        .collect();
    let instance_directory = context
        .base
        .resolve_related_directory(
            &context
                .private_home
                .paths()
                .root()
                .join("instances")
                .join(identity.instance_id()),
        )
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    instance_directory
        .prepare_private_directory()
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    let mut locations = vec![context.base.clone(), instance_directory.clone()];
    if let Some(trial_base) = context.trial_base {
        locations.push(trial_base.clone());
    }
    // Each invocation gets only its selected set and an empty skill-search home.
    // Shared authentication/session storage stays in the boxed XDG data paths.
    let config_directory = instance_directory.directory().join("config");
    let plugin_directory = context
        .base
        .resolve_related_directory(&config_directory.join("plugins"))
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    plugin_directory
        .prepare_private_directory()
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    let tool_home = context
        .base
        .resolve_related_directory(&instance_directory.directory().join("tool-home"))
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    tool_home
        .prepare_private_directory()
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    path_values.push((PathRole::OpenCodeToolHome, tool_home.directory().to_owned()));
    let private_paths =
        PlacementPaths::from_grant(context.private_home.paths().root().to_owned(), path_values)
            .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    require_private_paths(&private_paths)?;
    let mut artifacts = Vec::new();
    create_bytes(
        &plugin_directory,
        "shn-restore-env.js",
        RESTORE_ENV_PLUGIN.as_bytes(),
        &mut artifacts,
    )?;
    if let Some(adapter) = &bundled {
        // A shim only: the adapter registers its skills and policy relative to
        // its own tree, so it is imported where it was verified, never copied.
        let target = adapter.dir_arg() + MARIONETTE_ENTRY;
        create_bytes(
            &plugin_directory,
            MARIONETTE_SHIM,
            bundled_shim(&target).as_bytes(),
            &mut artifacts,
        )?;
    }
    locations.push(plugin_directory);
    locations.push(tool_home);
    create_json(
        &instance_directory,
        "set-manifest.json",
        &serde_json::to_value(context.selected_set)
            .map_err(|_| OpenCodeLaunchError::Configuration)?,
        &mut artifacts,
    )?;
    let ca = context
        .ca_file
        .canonicalize()
        .map_err(|_| OpenCodeLaunchError::InvalidCa)?;
    let expected_ca = context
        .base
        .directory()
        .join("sidecar/data/ca/combined-ca.pem");
    if !same_path(&ca, &expected_ca)? {
        return Err(OpenCodeLaunchError::InvalidCa);
    }
    let ca_file = pin_file(&ca).map_err(|_| OpenCodeLaunchError::InvalidCa)?;
    let ca_info = file_info(&ca_file)?;
    let ca_bytes = (u64::from(ca_info.nFileSizeHigh) << 32) | u64::from(ca_info.nFileSizeLow);
    if ca_bytes == 0 || ca_bytes > 4 * MIB {
        return Err(OpenCodeLaunchError::InvalidCa);
    }
    artifacts.push(ca_file);
    let home = context.private_home.paths().home();
    let mut environment: BTreeMap<OsString, OsString> = if trial {
        BTreeMap::new()
    } else {
        let values: Vec<_> = std::env::vars_os().take(2049).collect();
        if values.len() > 2048 {
            return Err(OpenCodeLaunchError::InheritedEnvironment);
        }
        values.into_iter().collect()
    };
    let system = std::env::var_os("SystemRoot").ok_or(OpenCodeLaunchError::InheritedEnvironment)?;
    set(&mut environment, "SystemRoot", system.clone());
    set(&mut environment, "WINDIR", system.clone());
    if trial {
        set(
            &mut environment,
            "PATH",
            PathBuf::from(system).join("System32").into_os_string(),
        );
    }
    // The canary gets an entirely private OS home. Interactive launches retain
    // the user's tool environment; OpenCode itself uses OPENCODE_TEST_HOME/XDG.
    for (name, value) in if trial {
        vec![
            ("HOME", home.to_owned()),
            ("USERPROFILE", home.to_owned()),
            (
                "APPDATA",
                context.private_home.paths().root().join("roaming"),
            ),
            (
                "LOCALAPPDATA",
                context.private_home.paths().root().join("local"),
            ),
        ]
    } else {
        Vec::new()
    } {
        if matches!(name, "APPDATA" | "LOCALAPPDATA") {
            let directory = context
                .base
                .resolve_related_directory(&value)
                .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
            directory
                .prepare_private_directory()
                .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
            locations.push(directory);
        }
        set(&mut environment, name, value.into_os_string());
    }
    let proxy = format!("http://127.0.0.1:{}", context.proxy_port);
    for name in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
        set(&mut environment, name, proxy.clone().into());
    }
    for name in ["NO_PROXY", "no_proxy"] {
        // Its owned standalone service talks over loopback, never the sidecar.
        set(&mut environment, name, "127.0.0.1,localhost,::1".into());
    }
    for name in [
        "NODE_TLS_REJECT_UNAUTHORIZED",
        "BUN_TLS_REJECT_UNAUTHORIZED",
    ] {
        environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
    }
    for name in ["NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE"] {
        set(&mut environment, name, ca.as_os_str().to_owned());
    }
    // Never inherit another instance's config, server address, tool-home,
    // password or bootstrap switches. Preserve only the launcher-owned recipe.
    environment.retain(|key, _| {
        let name = key.to_string_lossy().to_ascii_uppercase();
        name != "OPENCODE" && !name.starts_with("OPENCODE_") && !name.starts_with("SHN_USER_")
    });
    for name in [
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
    ] {
        let original = environment
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone());
        if let Some(value) = original {
            set(&mut environment, &format!("SHN_USER_{name}"), value);
        }
    }
    set(
        &mut environment,
        "OPENCODE_CONFIG_DIR",
        config_directory.into_os_string(),
    );
    // Qualification trials stay on the fixed recipe; only a real session honours
    // the user's launch policy.
    let policy = if trial {
        LaunchPolicy::default()
    } else {
        crate::launch_policy::current()
    };
    set(
        &mut environment,
        "OPENCODE_CONFIG_CONTENT",
        config_content(policy.full_access()).into(),
    );
    for name in [
        "OPENCODE_DISABLE_PROJECT_CONFIG",
        "OPENCODE_DISABLE_AUTOUPDATE",
    ] {
        set(&mut environment, name, "1".into());
    }
    if trial {
        set(
            &mut environment,
            "OPENCODE_DISABLE_MODELS_FETCH",
            "1".into(),
        );
    }
    policy.apply_environment(&mut environment);
    let descriptor = descriptor(&version)?;
    let mode = Placement::new(ProgramPlacement::System, HomePlacement::Box)
        .map_err(|_| OpenCodeLaunchError::PlacementMode)?;
    let placement_recipe = placement_recipe(&descriptor, mode)?;
    let args = fixed_arguments();
    let arguments_recipe = QualifiedArgvRecipe::qualify(
        &descriptor,
        Operation::StartNew,
        CommandForm::NativeExe(pinned.path),
        args,
        evidence(&descriptor)?,
    )
    .map_err(OpenCodeLaunchError::ArgumentsRecipe)?;
    let database_path = context.base.database_path();
    let inputs = LaunchInputs::new(
        &placement_recipe,
        &arguments_recipe,
        &project,
        &database_path,
        &environment,
        &identity,
    )
    .with_private_paths(&private_paths);
    let mut launch = compose_launch(&descriptor, Operation::StartNew, mode, &inputs)
        .map_err(OpenCodeLaunchError::Composition)?;
    let binding = if let Some(root) = &root {
        NativeLaunchBinding::for_qualified_launch(&launch, pinned.identity, root)?
    } else {
        NativeLaunchBinding::for_ephemeral_trial(&launch, pinned.identity)?
    };
    launch.set_native_binding(binding.clone());
    let (root_pin, mut root_ancestors) = pin_directory(home)?;
    pinned.ancestors.append(&mut root_ancestors);
    let pins = NativeLaunchPins::new(
        pinned.file,
        root_pin,
        pinned.ancestors,
        Some(context.private_home),
        locations,
        artifacts,
    )?;
    verify_sidecar(context.sidecar)?;
    if Instant::now() >= deadline {
        return Err(OpenCodeLaunchError::Expired);
    }
    Ok(Prepared {
        launch,
        binding,
        profile: resource_profile,
        pins,
    })
}
fn verify_sidecar(sidecar: &SidecarSupervisor) -> Result<(), OpenCodeLaunchError> {
    let status = sidecar
        .status()
        .ok_or(OpenCodeLaunchError::SidecarUnavailable)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OpenCodeLaunchError::SidecarUnavailable)?
        .as_millis() as i64;
    if !status.backs_launch(now) {
        return Err(OpenCodeLaunchError::SidecarUnavailable);
    }
    Ok(())
}
fn require_private_paths(paths: &PlacementPaths) -> Result<(), OpenCodeLaunchError> {
    // The default store grant has only Home, Temporary and Cache. OpenCode's
    // additional file/directory roles must come from its held private grant.
    for role in [
        PathRole::OpenCodeDatabase,
        PathRole::XdgConfig,
        PathRole::XdgData,
        PathRole::XdgCache,
        PathRole::XdgState,
        PathRole::OpenCodeToolHome,
    ] {
        if paths.path(role).is_none() {
            return Err(OpenCodeLaunchError::MissingPrivatePath(role));
        }
    }
    Ok(())
}
fn placement_recipe(
    descriptor: &AdapterDescriptor,
    mode: Placement,
) -> Result<QualifiedPlacementRecipe, OpenCodeLaunchError> {
    QualifiedPlacementRecipe::qualify(
        descriptor,
        mode,
        RecipeDefinition {
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
            .into_iter()
            .collect(),
            limits: vec![],
            pi_skills: None,
            codex_sandbox: None,
        },
        evidence(descriptor)?,
    )
    .map_err(OpenCodeLaunchError::PlacementRecipe)
}
fn descriptor(version: &Version) -> Result<AdapterDescriptor, OpenCodeLaunchError> {
    let mut descriptor = AdapterDescriptor {
        agent: AgentId::new("opencode").map_err(|_| OpenCodeLaunchError::Descriptor)?,
        version: version.clone(),
        display_name: "OpenCode local TUI qualification".into(),
        synthetic: false,
        architecture: ConnectionArchitecture::NativePty,
        capabilities: Capabilities::default(),
    };
    // Private invocation schema for the qualification path. It is NOT registered
    // in Engine or returned as a publicly supported native capability.
    for operation in [
        Operation::StartNew,
        Operation::Placement,
        Operation::InheritedMarkers,
    ] {
        descriptor
            .capabilities
            .qualify(
                operation,
                &descriptor.agent,
                version,
                false,
                evidence(&descriptor)?,
            )
            .map_err(|_| OpenCodeLaunchError::Descriptor)?;
    }
    Ok(descriptor)
}
fn evidence(descriptor: &AdapterDescriptor) -> Result<FixtureEvidence, OpenCodeLaunchError> {
    FixtureEvidence::new(
        descriptor.agent.clone(),
        descriptor.version.clone(),
        RECIPE,
        EvidenceKind::InstalledRuntimeFixture,
    )
    .map_err(|_| OpenCodeLaunchError::Evidence)
}
pub(crate) fn set(environment: &mut BTreeMap<OsString, OsString>, name: &str, value: OsString) {
    environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
    environment.insert(name.into(), value);
}
pub(crate) fn same_path(left: &Path, right: &Path) -> Result<bool, OpenCodeLaunchError> {
    Ok(left
        .canonicalize()
        .map_err(|_| OpenCodeLaunchError::PrivateHome)?
        .as_os_str()
        .eq_ignore_ascii_case(
            right
                .canonicalize()
                .map_err(|_| OpenCodeLaunchError::PrivateHome)?
                .as_os_str(),
        ))
}
pub(crate) fn create_json(
    location: &StorageLocation,
    name: &str,
    value: &Value,
    pins: &mut Vec<File>,
) -> Result<PathBuf, OpenCodeLaunchError> {
    let bytes = serde_json::to_vec(value).map_err(|_| OpenCodeLaunchError::Configuration)?;
    create_bytes(location, name, &bytes, pins)
}
/// Shim file in the instance's plugin folder, and the adapter's entry it imports
/// (the `exports` of the adapter's `package.json`).
const MARIONETTE_SHIM: &str = "shn-marionette.js";
const MARIONETTE_ENTRY: &str = "/index.js";
/// `export { default } from "file:///D:/.../plugin.mjs";`, percent-encoding the
/// characters a file URL cannot carry raw.
fn bundled_shim(path: &str) -> String {
    let mut url = String::from("file:///");
    for ch in path.replace('\\', "/").chars() {
        match ch {
            ' ' => url.push_str("%20"),
            '%' => url.push_str("%25"),
            '#' => url.push_str("%23"),
            '?' => url.push_str("%3F"),
            '"' => url.push_str("%22"),
            other => url.push(other),
        }
    }
    format!("export {{ default }} from \"{url}\";\n")
}
fn create_bytes(
    location: &StorageLocation,
    name: &str,
    bytes: &[u8],
    pins: &mut Vec<File>,
) -> Result<PathBuf, OpenCodeLaunchError> {
    let path = location.directory().join(name);
    if bytes.len() > 1024 * 1024 {
        return Err(OpenCodeLaunchError::Configuration);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(&path)
        .map_err(|_| OpenCodeLaunchError::Configuration)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| OpenCodeLaunchError::Configuration)?;
    drop(file);
    location
        .secure_private_file(&path)
        .map_err(|_| OpenCodeLaunchError::Configuration)?;
    pins.push(pin_file(&path)?);
    Ok(path)
}
pub(crate) fn file_info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, OpenCodeLaunchError> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(OpenCodeLaunchError::UnsafeProgram);
    }
    Ok(info)
}
pub(crate) fn pin_file(path: &Path) -> Result<File, OpenCodeLaunchError> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| OpenCodeLaunchError::UnsafeProgram)?;
    let info = file_info(&file)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(OpenCodeLaunchError::UnsafeProgram);
    }
    Ok(file)
}
fn pin_program_file(path: &Path) -> Result<File, OpenCodeLaunchError> {
    // Unlike data/config, the npm-distributed executable can have multiple
    // names. The share mode blocks content writers through every name and
    // deletion of THIS opened name. Another alias can be removed; identity
    // validation rejects its changed link count before launch. Parent pins
    // separately prevent the executable's selected path from being renamed.
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| OpenCodeLaunchError::UnsafeProgram)?;
    let info = file_info(&file)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks == 0
    {
        return Err(OpenCodeLaunchError::UnsafeProgram);
    }
    Ok(file)
}
pub(crate) fn pin_directory(path: &Path) -> Result<(File, Vec<File>), OpenCodeLaunchError> {
    StorageLocation::preflight(path).map_err(|_| OpenCodeLaunchError::PrivateHome)?;
    let parents: Vec<_> = path.ancestors().take(65).collect();
    if parents.len() > 64 {
        return Err(OpenCodeLaunchError::PrivateHome);
    }
    let mut pins = Vec::new();
    for parent in parents.into_iter().rev() {
        let file = OpenOptions::new()
            .read(true)
            // Participate in sharing checks without enumerating any contents.
            .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(parent)
            .map_err(|_| OpenCodeLaunchError::PrivateHome)?;
        let info = file_info(&file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(OpenCodeLaunchError::PrivateHome);
        }
        pins.push(file);
    }
    let root = pins.pop().ok_or(OpenCodeLaunchError::PrivateHome)?;
    Ok((root, pins))
}
struct ProgramPin {
    path: PathBuf,
    file: File,
    ancestors: Vec<File>,
    identity: NativeProgramIdentity,
    hash: String,
    bytes: u64,
}
fn pin_program(path: &Path, deadline: Instant) -> Result<ProgramPin, OpenCodeLaunchError> {
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("opencode.exe"))
    {
        return Err(OpenCodeLaunchError::InvalidCandidate);
    }
    let (_parent, mut ancestors) =
        pin_directory(path.parent().ok_or(OpenCodeLaunchError::InvalidCandidate)?)?;
    ancestors.push(_parent);
    let mut file = pin_program_file(path)?;
    let before = file_info(&file)?;
    let bytes = (u64::from(before.nFileSizeHigh) << 32) | u64::from(before.nFileSizeLow);
    if !(64..=512 * MIB).contains(&bytes) {
        return Err(OpenCodeLaunchError::UnsafeProgram);
    }
    let mut dos = [0; 64];
    file.read_exact(&mut dos)
        .map_err(|_| OpenCodeLaunchError::UnsafeProgram)?;
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if &dos[..2] != b"MZ" || offset > MIB || offset + 4 > bytes {
        return Err(OpenCodeLaunchError::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| OpenCodeLaunchError::UnsafeProgram)?;
    let mut pe = [0; 4];
    file.read_exact(&mut pe)
        .map_err(|_| OpenCodeLaunchError::UnsafeProgram)?;
    if pe != *b"PE\0\0" {
        return Err(OpenCodeLaunchError::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| OpenCodeLaunchError::UnsafeProgram)?;
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0; 65536];
    loop {
        if Instant::now() >= deadline {
            return Err(OpenCodeLaunchError::Expired);
        }
        let count = file
            .read(&mut buffer)
            .map_err(|_| OpenCodeLaunchError::UnsafeProgram)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > bytes {
            return Err(OpenCodeLaunchError::ProgramChanged);
        }
        hash.update(&buffer[..count]);
    }
    let digest: [u8; 32] = hash.finalize().into();
    let after = file_info(&file)?;
    if read != bytes
        || before.dwVolumeSerialNumber != after.dwVolumeSerialNumber
        || before.nFileIndexHigh != after.nFileIndexHigh
        || before.nFileIndexLow != after.nFileIndexLow
        || before.nNumberOfLinks != after.nNumberOfLinks
        || before.ftLastWriteTime.dwLowDateTime != after.ftLastWriteTime.dwLowDateTime
        || before.ftLastWriteTime.dwHighDateTime != after.ftLastWriteTime.dwHighDateTime
    {
        return Err(OpenCodeLaunchError::ProgramChanged);
    }
    let identity = NativeProgramIdentity::opencode_program(
        before.dwVolumeSerialNumber,
        (u64::from(before.nFileIndexHigh) << 32) | u64::from(before.nFileIndexLow),
        bytes,
        (u64::from(before.ftLastWriteTime.dwHighDateTime) << 32)
            | u64::from(before.ftLastWriteTime.dwLowDateTime),
        digest,
        before.nNumberOfLinks,
    )?;
    Ok(ProgramPin {
        path: path
            .canonicalize()
            .map_err(|_| OpenCodeLaunchError::UnsafeProgram)?,
        file,
        ancestors,
        identity,
        hash: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_shim_is_a_file_url_reexport() {
        assert_eq!(
            bundled_shim(r"D:\My App\data\marionette\adapters\opencode/index.js"),
            "export { default } from \"file:///D:/My%20App/data/marionette/adapters/opencode/index.js\";\n"
        );
    }
    #[test]
    fn opencode_standalone_composition_requires_private_database_and_xdg_paths() {
        let root = PathBuf::from("C:/private/opencode");
        let roles = [
            (PathRole::Home, "home"),
            (PathRole::Temporary, "tmp"),
            (PathRole::Cache, "cache"),
            (PathRole::OpenCodeDatabase, "home/opencode.db"),
            (PathRole::XdgConfig, "home/xdg/config"),
            (PathRole::XdgData, "home/xdg/data"),
            (PathRole::XdgCache, "cache/xdg"),
            (PathRole::XdgState, "home/xdg/state"),
            (PathRole::OpenCodeToolHome, "instances/trial/tool-home"),
        ];
        let descriptor = descriptor(&Version::parse("2.2.5").unwrap()).unwrap();
        let mode = Placement::new(ProgramPlacement::System, HomePlacement::Box).unwrap();
        let placement = placement_recipe(&descriptor, mode).unwrap();
        for (missing, _) in &roles[3..] {
            let paths = PlacementPaths::from_grant(
                root.clone(),
                roles
                    .iter()
                    .filter(|(role, _)| role != missing)
                    .map(|(role, suffix)| (*role, root.join(suffix)))
                    .collect(),
            )
            .unwrap();
            assert_eq!(
                require_private_paths(&paths),
                Err(OpenCodeLaunchError::MissingPrivatePath(*missing))
            );
            assert_eq!(
                shinatama_adapter::launch::placement::plan_placement(
                    &descriptor,
                    mode,
                    Some(&paths),
                    Some(&placement),
                )
                .unwrap_err(),
                PlacementError::MissingPath(*missing)
            );
        }
        let paths = PlacementPaths::from_grant(
            root.clone(),
            roles
                .iter()
                .map(|(role, suffix)| (*role, root.join(suffix)))
                .collect(),
        )
        .unwrap();
        require_private_paths(&paths).unwrap();
        let arguments = QualifiedArgvRecipe::qualify(
            &descriptor,
            Operation::StartNew,
            CommandForm::NativeExe("C:/program/opencode.exe".into()),
            fixed_arguments(),
            evidence(&descriptor).unwrap(),
        )
        .unwrap();
        let identity = LaunchIdentity::generate("t_abcdefghijkl").unwrap();
        let inherited = BTreeMap::new();
        let inputs = LaunchInputs::new(
            &placement,
            &arguments,
            Path::new("C:/project"),
            Path::new("C:/private/index.sqlite"),
            &inherited,
            &identity,
        )
        .with_private_paths(&paths);
        let launch = compose_launch(&descriptor, Operation::StartNew, mode, &inputs).unwrap();
        assert_eq!(launch.command().argv(), fixed_arguments());
        for (name, role) in [
            ("OPENCODE_DB", PathRole::OpenCodeDatabase),
            ("XDG_CONFIG_HOME", PathRole::XdgConfig),
            ("XDG_DATA_HOME", PathRole::XdgData),
            ("XDG_CACHE_HOME", PathRole::XdgCache),
            ("XDG_STATE_HOME", PathRole::XdgState),
            ("OPENCODE_TEST_HOME", PathRole::OpenCodeToolHome),
        ] {
            assert!(launch.environment().entries().any(|(key, value)| {
                key == name && Some(value) == paths.path(role).map(Path::as_os_str)
            }));
        }
    }
    #[test]
    fn opencode_program_alias_unlink_preserves_bytes_but_invalidates_prepared_identity() {
        let directory =
            std::env::temp_dir().join(format!("shn-opencode-pin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let program = directory.join("opencode.exe");
        let alias = directory.join("platform.exe");
        let second_alias = directory.join("platform-other.exe");
        std::fs::write(&program, b"synthetic executable pin fixture").unwrap();
        std::fs::hard_link(&program, &alias).unwrap();
        std::fs::hard_link(&program, &second_alias).unwrap();
        let held = pin_program_file(&program).unwrap();
        let before = file_info(&held).unwrap();
        assert_eq!(before.nNumberOfLinks, 3);
        let identity = NativeProgramIdentity::opencode_program(
            before.dwVolumeSerialNumber,
            (u64::from(before.nFileIndexHigh) << 32) | u64::from(before.nFileIndexLow),
            u64::from(before.nFileSizeLow),
            (u64::from(before.ftLastWriteTime.dwHighDateTime) << 32)
                | u64::from(before.ftLastWriteTime.dwLowDateTime),
            Sha256::digest(b"synthetic executable pin fixture").into(),
            before.nNumberOfLinks,
        )
        .unwrap();
        assert!(crate::native_launch::verify_program_pin(&held, &identity).is_ok());
        assert!(OpenOptions::new().write(true).open(&alias).is_err());
        assert!(OpenOptions::new().write(true).open(&second_alias).is_err());
        assert!(std::fs::remove_file(&program).is_err());
        assert!(
            pin_file(&program).is_err(),
            "data/config pins still reject hard links"
        );
        std::fs::remove_file(&alias).unwrap();
        assert_eq!(file_info(&held).unwrap().nNumberOfLinks, 2);
        assert!(crate::native_launch::verify_program_pin(&held, &identity).is_err());
        assert_eq!(
            std::fs::read(&program).unwrap(),
            b"synthetic executable pin fixture"
        );
        assert!(OpenOptions::new().write(true).open(&program).is_err());
        assert!(std::fs::remove_file(&program).is_err());
        drop(held);
        std::fs::remove_file(&second_alias).unwrap();
        std::fs::remove_file(&program).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
    #[test]
    fn opencode_standalone_keeps_native_permission_prompts() {
        assert_eq!(fixed_arguments(), vec![OsString::from("--standalone")]);
        let protected: Value = serde_json::from_str(config_content(false)).unwrap();
        assert!(protected.get("permissions").is_none());
    }
    #[test]
    fn opencode_full_access_adds_one_allow_all_rule_and_keeps_the_rest() {
        let open: Value = serde_json::from_str(config_content(true)).unwrap();
        assert_eq!(open["update"], "disable");
        assert_eq!(open["share"], "disabled");
        assert_eq!(
            open["permissions"],
            serde_json::json!([{"action": "*", "resource": "*", "effect": "allow"}])
        );
    }
    #[test]
    fn narrow_profile_is_fixed_and_failure_never_increases_it() {
        let first = profile().unwrap();
        let second = profile().unwrap();
        assert_eq!(first.budget(), second.budget());
        assert_eq!(first.budget().processes, 8);
        assert_eq!(first.process_commit_bytes(), 768 * MIB);
        assert_eq!(first.budget().memory_bytes, 1024 * MIB);
        assert_eq!(first.budget().cpu_basis_points, 1000);
    }
}
