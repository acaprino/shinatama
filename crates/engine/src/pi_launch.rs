//! Windows System+Box Pi startup: `node.exe <cli.js>` with the fixed TUI recipe.
//! PI_CODING_AGENT_DIR is the private box home (sessions under `sessions/`).
//! Preparation performs bounded local reads/writes; only the owned ConPTY trial
//! can qualify the exact Node + script build and profile. No provider request,
//! login, credential read, shell, npm install or marketplace access.
#![cfg(windows)]
use crate::{
    marionette::{Host, Marionette, is_marionette_set},
    launch::{
        LaunchInputs, LaunchIssueCode, PreparedLaunch, compose_launch, identity::LaunchIdentity,
    },
    native_launch::{
        NativeAdmissionProfile, NativeLaunchBinding, NativeLaunchError, NativeLaunchPins,
        NativeTrialReceipt, NativeTrialTracker, NativeTuiPermit,
    },
    opencode_launch::{create_json, file_info, pin_directory, pin_file, same_path, set},
    pi_probe::{
        PiProbeIssue, PiProbeOutcome, ProbeCleanup, pin_node, pin_script, resolve_installation,
    },
    sidecar_supervisor::SidecarSupervisor,
};
use shinatama_adapter::{
    AdapterDescriptor, AgentId, Capabilities, ConnectionArchitecture, EvidenceKind,
    FixtureEvidence, HomePlacement, Operation, Placement, ProgramPlacement, Version,
    catalog::{ProgramCandidate, ProgramKind},
    launch::{
        arguments::{ArgvError, CommandForm, QualifiedArgvRecipe},
        placement::{
            HomeMapping, PathRole, PiSkillIsolation, PlacementError, PlacementPaths,
            QualifiedPlacementRecipe, RecipeDefinition, RecipeFeature, RuntimeKind,
        },
    },
    pi::{discovery, run, sets::ResourceArguments},
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
    num::NonZeroU16,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MIB: u64 = 1024 * 1024;
const PREPARE_BUDGET: Duration = Duration::from_secs(10);
const TRIAL_PREFIX: &str = "pi-tui-trial-";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiLaunchError {
    InvalidCandidate,
    Installation(PiProbeIssue),
    ObservationRequired,
    ProgramChanged,
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
impl std::fmt::Display for PiLaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pi startup prerequisite: {self:?}.")
    }
}
impl std::error::Error for PiLaunchError {}
impl From<NativeLaunchError> for PiLaunchError {
    fn from(_: NativeLaunchError) -> Self {
        Self::Permit
    }
}

/// Trusted backend values only. No Deserialize, custom argv, environment, model,
/// credentials, resource budget or qualification boolean is accepted from IPC.
pub struct PiStartContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a PiProbeOutcome,
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
    /// The thread's Mukade room (ADR-0057): Mukade's plugin kit joins the session to it.
    pub chat: Option<&'a crate::mukade_chat::ChatHooks>,
}
pub struct PiTrialContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a PiProbeOutcome,
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
pub struct PreparedPiTrial {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    pub tracker: NativeTrialTracker,
}
pub struct PreparedPiStart {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
}
/// Mukade's Pi extension, written beside the instance, as the arguments that load it, followed by
/// Pi's built-in MCP, which registers the extension's server (Mukade's live record, run 10).
fn chat_kit(base: &StorageLocation, instance: &Path) -> Result<(crate::mukade_mcp::Connection, Vec<OsString>), String> {
    let connection = crate::mukade_mcp::Connection::discover(base)
        .map_err(|error| error.to_string())?
        .ok_or("the Mukade service is not running")?;
    let kit = crate::mukade_chat::plugin_kit("pi", &instance.join("mukade-kit"), &connection, None)?;
    if kit.arguments.len() != 2 || kit.arguments[0] != "-e" {
        return Err("Mukade's Pi kit is not the one extension Shinatama expects".into());
    }
    let mut arguments = kit.arguments;
    arguments.extend([OsString::from("-e"), OsString::from("builtin:mcp")]);
    Ok((connection, arguments))
}

struct Context<'a> {
    candidate: &'a ProgramCandidate,
    observation: &'a PiProbeOutcome,
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
    chat: Option<&'a crate::mukade_chat::ChatHooks>,
}
struct Prepared {
    launch: PreparedLaunch,
    binding: NativeLaunchBinding,
    profile: NativeAdmissionProfile,
    pins: NativeLaunchPins,
}

/// Fixed finite profile shared by trial and interactive launch, never raised
/// on failure. Pi's tools spawn shells, so the process count leaves headroom.
fn profile() -> Result<NativeAdmissionProfile, PiLaunchError> {
    Ok(NativeAdmissionProfile::new(
        run::TUI_PROFILE,
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

pub fn prepare_trial(context: PiTrialContext<'_>) -> Result<PreparedPiTrial, PiLaunchError> {
    // Filename presence only: the canary home must be fresh, never a credential store.
    if context
        .private_home
        .paths()
        .home()
        .join("auth.json")
        .try_exists()
        .map_err(|_| PiLaunchError::PrivateHome)?
    {
        return Err(PiLaunchError::PrivateHome);
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
            chat: None,
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
    Ok(PreparedPiTrial {
        launch: prepared.launch,
        permit,
        tracker,
    })
}

pub fn prepare_start(
    context: PiStartContext<'_>,
    receipt: &NativeTrialReceipt,
) -> Result<PreparedPiStart, PiLaunchError> {
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
            chat: context.chat,
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
    Ok(PreparedPiStart {
        launch: prepared.launch,
        permit,
    })
}

fn prepare(
    context: Context<'_>,
    receipt: Option<&NativeTrialReceipt>,
    trial: bool,
) -> Result<Prepared, PiLaunchError> {
    let deadline = Instant::now() + PREPARE_BUDGET;
    if context.candidate.agent.as_str() != "pi" || context.candidate.kind != ProgramKind::CmdWrapper
    {
        return Err(PiLaunchError::InvalidCandidate);
    }
    let observation = context.observation.summary();
    let (version, node_version) = observation
        .version
        .clone()
        .zip(observation.node_version.clone())
        .ok_or(PiLaunchError::ObservationRequired)?;
    if observation.issue.is_some()
        || observation.cleanup != ProbeCleanup::Released
        || context.observation.cleanup_retained()
        || !discovery::node_supported(&node_version)
    {
        return Err(PiLaunchError::ObservationRequired);
    }
    // Empty set, or exactly Marionette for a real session (never the trial);
    // discovery stays disabled and digests are checked at every launch.
    let bundled = if context.selected_set.is_empty() {
        None
    } else if is_marionette_set(context.selected_set) && !trial {
        let root = context.plugins_root.ok_or(PiLaunchError::BundledPlugins)?;
        let adapter = Marionette::current(root)
            .and_then(|release| release.verify(Host::Pi))
            .map_err(|_| PiLaunchError::BundledPlugins)?;
        let arguments =
            shinatama_adapter::pi::sets::plan_marionette(Path::new(&adapter.dir_arg()))
        .map_err(|_| PiLaunchError::BundledPlugins)?;
        Some(arguments)
    } else {
        return Err(PiLaunchError::UnsupportedSet);
    };
    verify_sidecar(context.sidecar)?;
    context
        .base
        .verify_private_directory()
        .map_err(|_| PiLaunchError::PrivateHome)?;
    let root = if let (Some(database), Some(expected)) = (context.database, context.root.as_ref()) {
        let root = database
            .store_root(&expected.id)
            .map_err(|_| PiLaunchError::RootChanged)?
            .filter(|root| root == expected)
            .ok_or(PiLaunchError::RootChanged)?;
        if root.harness != "pi"
            || root.kind != RootKind::Box
            || root.scope != RootScope::Primary
            || root.access != RootAccess::ReadWrite
            || !root.available
            || !same_path(
                Path::new(&root.resolved_path),
                context.private_home.paths().home(),
            )
            .map_err(|_| PiLaunchError::RootChanged)?
        {
            return Err(PiLaunchError::RootChanged);
        }
        Some(root)
    } else if trial && context.trial_base.is_some() {
        None
    } else {
        return Err(PiLaunchError::RootChanged);
    };
    let project = context
        .project
        .canonicalize()
        .map_err(|_| PiLaunchError::InvalidProject)?;
    if !project.is_dir() {
        return Err(PiLaunchError::InvalidProject);
    }
    if let Some(trial_base) = context.trial_base {
        verify_trial_layout(context.base, trial_base, &project, &context.private_home)?;
    } else {
        context
            .base
            .validate_project(&project)
            .map_err(|_| PiLaunchError::InvalidProject)?;
    }
    // Resolve the shim again and pin both targets; the observation must match.
    let installation =
        resolve_installation(context.candidate).map_err(PiLaunchError::Installation)?;
    let node = pin_node(installation.node(), deadline).map_err(pin_error)?;
    let script = pin_script(installation.script(), deadline).map_err(pin_error)?;
    if node.sha256 != observation.node_sha256
        || node.bytes != observation.node_bytes
        || script.sha256 != observation.script_sha256
        || script.bytes != observation.script_bytes
    {
        return Err(PiLaunchError::ProgramChanged);
    }
    let resource_profile = profile()?;
    if let Some(receipt) = receipt {
        if !receipt.matches(
            &context.candidate.agent,
            &node.identity,
            &version,
            &resource_profile,
        ) {
            return Err(PiLaunchError::QualificationRequired);
        }
    } else if !trial {
        return Err(PiLaunchError::QualificationRequired);
    }
    let identity =
        LaunchIdentity::generate(context.thread_id).map_err(|_| PiLaunchError::Identity)?;
    let path_values: Vec<_> = context
        .private_home
        .paths()
        .iter()
        .filter_map(|(role, path)| {
            let role = match role {
                StorePathRole::Home => PathRole::Home,
                StorePathRole::Temporary => PathRole::Temporary,
                StorePathRole::Cache => PathRole::Cache,
                StorePathRole::NodeCompileCache => PathRole::NodeCompileCache,
                _ => return None,
            };
            Some((role, path.to_owned()))
        })
        .collect();
    let private_paths =
        PlacementPaths::from_grant(context.private_home.paths().root().to_owned(), path_values)
            .map_err(|_| PiLaunchError::PrivateHome)?;
    for role in [
        PathRole::Home,
        PathRole::Temporary,
        PathRole::NodeCompileCache,
    ] {
        if private_paths.path(role).is_none() {
            return Err(PiLaunchError::MissingPrivatePath(role));
        }
    }
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
        .map_err(|_| PiLaunchError::PrivateHome)?;
    instance_directory
        .prepare_private_directory()
        .map_err(|_| PiLaunchError::PrivateHome)?;
    let mut locations = vec![context.base.clone(), instance_directory.clone()];
    if let Some(trial_base) = context.trial_base {
        locations.push(trial_base.clone());
    }
    let mut artifacts = Vec::new();
    create_json(
        &instance_directory,
        "set-manifest.json",
        &serde_json::to_value(context.selected_set).map_err(|_| PiLaunchError::Configuration)?,
        &mut artifacts,
    )
    .map_err(|_| PiLaunchError::Configuration)?;
    let ca = context
        .ca_file
        .canonicalize()
        .map_err(|_| PiLaunchError::InvalidCa)?;
    let expected_ca = context
        .base
        .directory()
        .join("sidecar/data/ca/combined-ca.pem");
    if !same_path(&ca, &expected_ca).map_err(|_| PiLaunchError::InvalidCa)? {
        return Err(PiLaunchError::InvalidCa);
    }
    let ca_file = pin_file(&ca).map_err(|_| PiLaunchError::InvalidCa)?;
    let ca_info = file_info(&ca_file).map_err(|_| PiLaunchError::InvalidCa)?;
    let ca_bytes = (u64::from(ca_info.nFileSizeHigh) << 32) | u64::from(ca_info.nFileSizeLow);
    if ca_bytes == 0 || ca_bytes > 4 * MIB {
        return Err(PiLaunchError::InvalidCa);
    }
    artifacts.push(ca_file);
    let ca = crate::pi_probe::ordinary(&ca).map_err(|_| PiLaunchError::InvalidCa)?;
    let home = context.private_home.paths().home();
    let mut environment: BTreeMap<OsString, OsString> = if trial {
        BTreeMap::new()
    } else {
        let values: Vec<_> = std::env::vars_os().take(2049).collect();
        if values.len() > 2048 {
            return Err(PiLaunchError::InheritedEnvironment);
        }
        values.into_iter().collect()
    };
    // Never another process's Mukade membership; a talking thread sets its own below.
    crate::mukade_mcp::clear_environment(&mut environment);
    let system = std::env::var_os("SystemRoot").ok_or(PiLaunchError::InheritedEnvironment)?;
    set(&mut environment, "SystemRoot", system.clone());
    set(&mut environment, "WINDIR", system.clone());
    if trial {
        // The canary gets an entirely private OS home and no user tool PATH.
        set(
            &mut environment,
            "PATH",
            PathBuf::from(&system).join("System32").into_os_string(),
        );
        for (name, value) in [
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
        ] {
            if matches!(name, "APPDATA" | "LOCALAPPDATA") {
                let directory = context
                    .base
                    .resolve_related_directory(&value)
                    .map_err(|_| PiLaunchError::PrivateHome)?;
                directory
                    .prepare_private_directory()
                    .map_err(|_| PiLaunchError::PrivateHome)?;
                locations.push(directory);
            }
            set(&mut environment, name, value.into_os_string());
        }
    }
    // Never inherit another agent dir, session dir, package dir or Node injection.
    environment.retain(|key, _| !run::is_inherited_override(key));
    let proxy = format!("http://127.0.0.1:{}", context.proxy_port);
    for name in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
        set(&mut environment, name, proxy.clone().into());
    }
    for name in ["NO_PROXY", "no_proxy"] {
        set(&mut environment, name, "127.0.0.1,localhost,::1".into());
    }
    environment.retain(|key, _| !key.eq_ignore_ascii_case("NODE_TLS_REJECT_UNAUTHORIZED"));
    for name in ["NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE"] {
        set(&mut environment, name, ca.as_os_str().to_owned());
    }
    for (name, value) in run::fixed_environment(trial) {
        set(&mut environment, name, value.into());
    }
    // Pi has no permission prompts of its own, so the launch policy only adds the
    // user's variables, and only to a real session.
    if !trial {
        crate::launch_policy::current().apply_environment(&mut environment);
    }
    // ADR-0057: a session of a talking thread loads Mukade's Pi extension and Pi's own MCP,
    // which `--no-extensions` otherwise leaves off. Any failure leaves it an ordinary session.
    let mut chat_arguments = Vec::new();
    if let Some(chat) = context.chat.filter(|_| !trial) {
        match chat_kit(context.base, instance_directory.directory()) {
            Ok((connection, arguments)) => {
                chat_arguments = arguments;
                connection.apply_kit_environment(&mut environment);
                chat.apply_environment(&mut environment);
                crate::mukade_chat::note_launch(context.thread_id, chat.room_id());
            }
            Err(reason) => eprintln!("This Pi session opens without its thread's room: {reason}"),
        }
    }
    let descriptor = descriptor(&version)?;
    let mode = Placement::new(ProgramPlacement::System, HomePlacement::Box)
        .map_err(|_| PiLaunchError::PlacementMode)?;
    let placement_recipe = placement_recipe(&descriptor, mode)?;
    let arguments_recipe = QualifiedArgvRecipe::qualify(
        &descriptor,
        Operation::StartNew,
        CommandForm::NodeScript {
            node_exe: node.path.clone(),
            script: script.path.clone(),
            node_version,
            minimum_node_version: Some(discovery::minimum_node()),
        },
        run::tui_arguments(&bundled.unwrap_or_else(ResourceArguments::empty))
            .into_iter()
            .chain(chat_arguments)
            .collect(),
        evidence(&descriptor)?,
    )
    .map_err(PiLaunchError::ArgumentsRecipe)?;
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
        .map_err(PiLaunchError::Composition)?;
    let binding = if let Some(root) = &root {
        NativeLaunchBinding::for_qualified_launch(&launch, node.identity.clone(), root)?
    } else {
        NativeLaunchBinding::for_ephemeral_trial(&launch, node.identity.clone())?
    };
    launch.set_native_binding(binding.clone());
    let (root_pin, mut ancestors) = pin_directory(home).map_err(|_| PiLaunchError::PrivateHome)?;
    let mut node_ancestors = node.ancestors;
    let mut script_ancestors = script.ancestors;
    ancestors.append(&mut node_ancestors);
    ancestors.append(&mut script_ancestors);
    // The held script handle denies writers and deletion of the entry module.
    artifacts.push(script.file);
    let pins = NativeLaunchPins::new(
        node.file,
        root_pin,
        ancestors,
        Some(context.private_home),
        locations,
        artifacts,
    )?;
    verify_sidecar(context.sidecar)?;
    if Instant::now() >= deadline {
        return Err(PiLaunchError::Expired);
    }
    Ok(Prepared {
        launch,
        binding,
        profile: resource_profile,
        pins,
    })
}

fn pin_error(issue: PiProbeIssue) -> PiLaunchError {
    match issue {
        PiProbeIssue::Expired => PiLaunchError::Expired,
        _ => PiLaunchError::ProgramChanged,
    }
}

fn verify_trial_layout(
    base: &StorageLocation,
    trial_base: &StorageLocation,
    project: &Path,
    private_home: &PreparedPrivateStore,
) -> Result<(), PiLaunchError> {
    trial_base
        .verify_private_directory()
        .map_err(|_| PiLaunchError::PrivateHome)?;
    let relative = trial_base
        .directory()
        .strip_prefix(base.directory())
        .map_err(|_| PiLaunchError::PrivateHome)?;
    let name = relative.to_str().ok_or(PiLaunchError::PrivateHome)?;
    let suffix = name
        .strip_prefix(TRIAL_PREFIX)
        .ok_or(PiLaunchError::PrivateHome)?;
    let trial_id = uuid::Uuid::parse_str(suffix).map_err(|_| PiLaunchError::PrivateHome)?;
    let same =
        |left: &Path, right: &Path| same_path(left, right).map_err(|_| PiLaunchError::PrivateHome);
    if trial_id.get_version() != Some(uuid::Version::Random)
        || !same(project, &trial_base.directory().join("project"))?
        || !same(
            private_home.paths().root(),
            &trial_base.directory().join("agents/pi"),
        )?
    {
        return Err(PiLaunchError::PrivateHome);
    }
    Ok(())
}

fn verify_sidecar(sidecar: &SidecarSupervisor) -> Result<(), PiLaunchError> {
    let status = sidecar.status().ok_or(PiLaunchError::SidecarUnavailable)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| PiLaunchError::SidecarUnavailable)?
        .as_millis() as i64;
    if !status.backs_launch(now) {
        return Err(PiLaunchError::SidecarUnavailable);
    }
    Ok(())
}

fn placement_recipe(
    descriptor: &AdapterDescriptor,
    mode: Placement,
) -> Result<QualifiedPlacementRecipe, PiLaunchError> {
    QualifiedPlacementRecipe::qualify(
        descriptor,
        mode,
        RecipeDefinition {
            home: HomeMapping::Pi,
            runtime: RuntimeKind::Node,
            features: [
                RecipeFeature::OwnerOnlyGrant,
                RecipeFeature::PrivateTemporary,
                RecipeFeature::PrivateNodeCompileCache,
                RecipeFeature::PiSkillsIsolated,
            ]
            .into_iter()
            .collect(),
            limits: vec![],
            // Windows Node resolves the user home from USERPROFILE, not HOME,
            // so isolation is the discovery switches, not a child HOME.
            pi_skills: Some(PiSkillIsolation::NoSkillsArgument),
            codex_sandbox: None,
        },
        evidence(descriptor)?,
    )
    .map_err(PiLaunchError::PlacementRecipe)
}

fn descriptor(version: &Version) -> Result<AdapterDescriptor, PiLaunchError> {
    let mut descriptor = AdapterDescriptor {
        agent: AgentId::new("pi").map_err(|_| PiLaunchError::Descriptor)?,
        version: version.clone(),
        display_name: "Pi local TUI qualification".into(),
        synthetic: false,
        architecture: ConnectionArchitecture::NativePty,
        capabilities: Capabilities::default(),
    };
    // Private invocation schema for this qualification path, not a public
    // Engine registration of native Pi capabilities.
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
            .map_err(|_| PiLaunchError::Descriptor)?;
    }
    Ok(descriptor)
}

fn evidence(descriptor: &AdapterDescriptor) -> Result<FixtureEvidence, PiLaunchError> {
    FixtureEvidence::new(
        descriptor.agent.clone(),
        descriptor.version.clone(),
        run::TUI_PROFILE,
        EvidenceKind::InstalledRuntimeFixture,
    )
    .map_err(|_| PiLaunchError::Evidence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::identity::LaunchIdentity;

    #[test]
    fn node_script_composition_requires_private_home_temp_and_compile_cache() {
        let version = Version::parse("1.4.0").unwrap();
        let descriptor = descriptor(&version).unwrap();
        let mode = Placement::new(ProgramPlacement::System, HomePlacement::Box).unwrap();
        let placement = placement_recipe(&descriptor, mode).unwrap();
        let root = PathBuf::from(r"C:\private data ü\agents\pi");
        let complete = PlacementPaths::from_grant(
            root.clone(),
            vec![
                (PathRole::Home, root.join("home")),
                (PathRole::Temporary, root.join("tmp")),
                (PathRole::Cache, root.join("cache")),
                (PathRole::NodeCompileCache, root.join("cache").join("node")),
            ],
        )
        .unwrap();
        let script = PathBuf::from(
            r"C:\Users\Zoë Ünïcode\npm global\node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js",
        );
        let arguments = QualifiedArgvRecipe::qualify(
            &descriptor,
            Operation::StartNew,
            CommandForm::NodeScript {
                node_exe: PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
                script: script.clone(),
                node_version: Version::parse("24.14.1").unwrap(),
                minimum_node_version: Some(discovery::minimum_node()),
            },
            run::tui_arguments(&ResourceArguments::empty()),
            evidence(&descriptor).unwrap(),
        )
        .unwrap();
        let identity = LaunchIdentity::generate("t_abcdefghijkl").unwrap();
        let environment = BTreeMap::from([(
            OsString::from("PI_CODING_AGENT_DIR"),
            OsString::from(r"C:\Users\Zoë\.pi\agent"),
        )]);
        let project = PathBuf::from(r"C:\Projects\Zoë project");
        let database = PathBuf::from(r"C:\private data ü\shinatama.db");
        let launch = compose_launch(
            &descriptor,
            Operation::StartNew,
            mode,
            &LaunchInputs::new(
                &placement,
                &arguments,
                &project,
                &database,
                &environment,
                &identity,
            )
            .with_private_paths(&complete),
        )
        .unwrap();
        // node.exe is the executable; the script is ONE token, never a .cmd.
        assert!(launch.command().executable().ends_with("node.exe"));
        assert_eq!(launch.command().argv()[0], script.as_os_str());
        assert_eq!(
            &launch.command().argv()[1..],
            run::DISCOVERY_SWITCHES.map(OsString::from)
        );
        let value = |name: &str| {
            launch
                .environment()
                .entries()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.to_owned())
        };
        // The placement override replaces any inherited agent dir.
        assert_eq!(
            value("PI_CODING_AGENT_DIR"),
            Some(root.join("home").into_os_string())
        );
        assert_eq!(
            value("NODE_COMPILE_CACHE"),
            Some(root.join("cache").join("node").into_os_string())
        );
        let incomplete = PlacementPaths::from_grant(
            root.clone(),
            vec![
                (PathRole::Home, root.join("home")),
                (PathRole::Temporary, root.join("tmp")),
            ],
        )
        .unwrap();
        assert_eq!(
            compose_launch(
                &descriptor,
                Operation::StartNew,
                mode,
                &LaunchInputs::new(
                    &placement,
                    &arguments,
                    &project,
                    &database,
                    &environment,
                    &identity,
                )
                .with_private_paths(&incomplete),
            )
            .err(),
            Some(LaunchIssueCode::PlacementRejected)
        );
    }

    #[test]
    fn pi_recipe_refuses_an_old_node_or_a_missing_floor() {
        let version = Version::parse("1.4.0").unwrap();
        let descriptor = descriptor(&version).unwrap();
        for (node, minimum) in [
            ("22.18.0", Some(discovery::minimum_node())),
            ("24.14.1", None),
        ] {
            assert!(
                QualifiedArgvRecipe::qualify(
                    &descriptor,
                    Operation::StartNew,
                    CommandForm::NodeScript {
                        node_exe: PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
                        script: PathBuf::from(r"C:\pi\cli.js"),
                        node_version: Version::parse(node).unwrap(),
                        minimum_node_version: minimum,
                    },
                    run::tui_arguments(&ResourceArguments::empty()),
                    evidence(&descriptor).unwrap(),
                )
                .is_err()
            );
        }
        let profile = profile().unwrap();
        assert_eq!(profile.id(), run::TUI_PROFILE);
        assert_eq!(profile.budget().processes, 8);
    }
}
