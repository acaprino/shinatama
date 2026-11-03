//! Narrow Windows System+Box Claude startup. Preparation performs bounded local
//! reads/writes; only the owned ConPTY canary can qualify the exact build/profile.
//! No provider request, login, user credential read, shell or marketplace install.
#![cfg(windows)]
use crate::{
    claude_probe::{ClaudeProbeOutcome, ProbeCleanup},
    launch::{LaunchInputs, PreparedLaunch, compose_launch, identity::LaunchIdentity},
    native_launch::{
        NativeAdmissionProfile, NativeLaunchBinding, NativeLaunchError, NativeLaunchPins,
        NativeProgramIdentity, NativeTrialReceipt, NativeTrialTracker, NativeTuiPermit,
    },
    sidecar_supervisor::{Phase, SidecarSupervisor},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use shinatama_adapter::{
    AdapterDescriptor, AgentId, Capabilities, ConnectionArchitecture, EvidenceKind,
    FixtureEvidence, HomePlacement, Operation, Placement, ProgramPlacement, Version,
    catalog::{ProgramCandidate, ProgramKind},
    launch::{
        arguments::{CommandForm, QualifiedArgvRecipe},
        placement::{
            HomeMapping, PathRole, PlacementPaths, QualifiedPlacementRecipe, RecipeDefinition,
            RecipeFeature, RuntimeKind,
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
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
};

const MIB: u64 = 1024 * 1024;
const PREPARE_BUDGET: Duration = Duration::from_secs(10);
const RECIPE: &str = "claude-system-box-safe-empty-tui-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeLaunchError {
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
    Configuration,
    Composition,
    Identity,
    QualificationRequired,
    Permit,
}
impl std::fmt::Display for ClaudeLaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Claude startup prerequisite: {self:?}.")
    }
}
impl std::error::Error for ClaudeLaunchError {}
impl From<NativeLaunchError> for ClaudeLaunchError {
    fn from(_: NativeLaunchError) -> Self {
        Self::Permit
    }
}

/// Trusted backend values only. No Deserialize, custom argv, environment, model,
/// credentials, resource budget or qualification boolean is accepted from IPC.
pub struct ClaudeStartContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a ClaudeProbeOutcome,
    pub base: &'a StorageLocation,
    pub database: &'a Database,
    pub private_home: PreparedPrivateStore,
    pub root: StoreRootRecord,
    pub project: &'a Path,
    pub sidecar: &'a SidecarSupervisor,
    pub proxy_port: NonZeroU16,
    pub ca_file: &'a Path,
    pub selected_set: &'a SetManifest,
    pub thread_id: &'a str,
}
pub struct PreparedClaudeTrial {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    pub tracker: NativeTrialTracker,
}
pub struct ClaudeTrialContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a ClaudeProbeOutcome,
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
    observation: &'a ClaudeProbeOutcome,
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
    thread_id: &'a str,
}
pub struct PreparedClaudeStart {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
}

/// This is the ROOT-approved finite trial profile, not inferred from a growing
/// process and never raised on failure. Issuance is sealed inside this qualifier.
fn profile() -> Result<NativeAdmissionProfile, ClaudeLaunchError> {
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
    context: ClaudeTrialContext<'_>,
) -> Result<PreparedClaudeTrial, ClaudeLaunchError> {
    // Filename presence only: never read a credential store for this canary.
    if context
        .private_home
        .paths()
        .home()
        .join(".credentials.json")
        .try_exists()
        .map_err(|_| ClaudeLaunchError::PrivateHome)?
    {
        return Err(ClaudeLaunchError::PrivateHome);
    }
    if context
        .private_home
        .paths()
        .home()
        .read_dir()
        .map_err(|_| ClaudeLaunchError::PrivateHome)?
        .next()
        .transpose()
        .map_err(|_| ClaudeLaunchError::PrivateHome)?
        .is_some()
    {
        return Err(ClaudeLaunchError::PrivateHome);
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
    Ok(PreparedClaudeTrial {
        launch: prepared.launch,
        permit,
        tracker,
    })
}
pub fn prepare_start(
    context: ClaudeStartContext<'_>,
    receipt: &NativeTrialReceipt,
) -> Result<PreparedClaudeStart, ClaudeLaunchError> {
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
    Ok(PreparedClaudeStart {
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
) -> Result<Prepared, ClaudeLaunchError> {
    let deadline = Instant::now() + PREPARE_BUDGET;
    if context.candidate.agent.as_str() != "claude_code"
        || context.candidate.kind != ProgramKind::DirectExe
    {
        return Err(ClaudeLaunchError::InvalidCandidate);
    }
    let observation = context.observation.summary();
    let version = observation
        .version
        .clone()
        .ok_or(ClaudeLaunchError::ObservationRequired)?;
    if observation.issue.is_some()
        || observation.cleanup != ProbeCleanup::Released
        || context.observation.cleanup_retained()
    {
        return Err(ClaudeLaunchError::ObservationRequired);
    }
    if !context.selected_set.is_empty() {
        return Err(ClaudeLaunchError::UnsupportedSet);
    }
    verify_sidecar(context.sidecar)?;
    context
        .base
        .verify_private_directory()
        .map_err(|_| ClaudeLaunchError::PrivateHome)?;
    let root = if let (Some(database), Some(expected)) = (context.database, context.root.as_ref()) {
        let root = database
            .store_root(&expected.id)
            .map_err(|_| ClaudeLaunchError::RootChanged)?
            .filter(|root| root == expected)
            .ok_or(ClaudeLaunchError::RootChanged)?;
        if root.harness != "claude_code"
            || root.kind != RootKind::Box
            || root.scope != RootScope::Primary
            || root.access != RootAccess::ReadWrite
            || !root.available
            || !same_path(
                Path::new(&root.resolved_path),
                context.private_home.paths().home(),
            )?
        {
            return Err(ClaudeLaunchError::RootChanged);
        }
        Some(root)
    } else if trial && context.trial_base.is_some() {
        None
    } else {
        return Err(ClaudeLaunchError::RootChanged);
    };
    let project = context
        .project
        .canonicalize()
        .map_err(|_| ClaudeLaunchError::InvalidProject)?;
    if !project.is_dir() {
        return Err(ClaudeLaunchError::InvalidProject);
    }
    if let Some(trial_base) = context.trial_base {
        trial_base
            .verify_private_directory()
            .map_err(|_| ClaudeLaunchError::PrivateHome)?;
        let relative = trial_base
            .directory()
            .strip_prefix(context.base.directory())
            .map_err(|_| ClaudeLaunchError::PrivateHome)?;
        let name = relative.to_str().ok_or(ClaudeLaunchError::PrivateHome)?;
        let suffix = name
            .strip_prefix("claude-tui-trial-")
            .ok_or(ClaudeLaunchError::PrivateHome)?;
        let trial_id = uuid::Uuid::parse_str(suffix).map_err(|_| ClaudeLaunchError::PrivateHome)?;
        if trial_id.get_version() != Some(uuid::Version::Random)
            || !same_path(&project, &trial_base.directory().join("project"))?
            || !same_path(
                context.private_home.paths().root(),
                &trial_base.directory().join("agents/claude_code"),
            )?
        {
            return Err(ClaudeLaunchError::PrivateHome);
        }
    } else {
        context
            .base
            .validate_project(&project)
            .map_err(|_| ClaudeLaunchError::InvalidProject)?;
    }
    let mut pinned = pin_program(&context.candidate.path, deadline)?;
    if pinned.hash != observation.executable_sha256 || pinned.bytes != observation.executable_bytes
    {
        return Err(ClaudeLaunchError::ProgramChanged);
    }
    let resource_profile = profile()?;
    if let Some(receipt) = receipt {
        if !receipt.matches(&pinned.identity, &version, &resource_profile) {
            return Err(ClaudeLaunchError::QualificationRequired);
        }
    } else if !trial {
        return Err(ClaudeLaunchError::QualificationRequired);
    }
    let identity =
        LaunchIdentity::generate(context.thread_id).map_err(|_| ClaudeLaunchError::Identity)?;
    let private_paths = PlacementPaths::from_grant(
        context.private_home.paths().root().to_owned(),
        context
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
            .collect(),
    )
    .map_err(|_| ClaudeLaunchError::PrivateHome)?;
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
        .map_err(|_| ClaudeLaunchError::PrivateHome)?;
    instance_directory
        .prepare_private_directory()
        .map_err(|_| ClaudeLaunchError::PrivateHome)?;
    let mut locations = vec![context.base.clone(), instance_directory.clone()];
    if let Some(trial_base) = context.trial_base {
        locations.push(trial_base.clone());
    }
    let mut artifacts = Vec::new();
    let settings = if trial {
        serde_json::json!({})
    } else {
        read_settings(context.private_home.paths().home().join("settings.json"))?
    };
    let settings_path = create_json(
        &instance_directory,
        "settings.json",
        &settings,
        &mut artifacts,
    )?;
    let mcp_path = create_json(
        &instance_directory,
        "mcp.json",
        &serde_json::json!({"mcpServers":{}}),
        &mut artifacts,
    )?;
    create_json(
        &instance_directory,
        "set-manifest.json",
        &serde_json::to_value(context.selected_set)
            .map_err(|_| ClaudeLaunchError::Configuration)?,
        &mut artifacts,
    )?;
    let ca = context
        .ca_file
        .canonicalize()
        .map_err(|_| ClaudeLaunchError::InvalidCa)?;
    let expected_ca = context
        .base
        .directory()
        .join("sidecar/data/ca/combined-ca.pem");
    if !same_path(&ca, &expected_ca)? {
        return Err(ClaudeLaunchError::InvalidCa);
    }
    let ca_file = pin_file(&ca).map_err(|_| ClaudeLaunchError::InvalidCa)?;
    let ca_info = file_info(&ca_file)?;
    let ca_bytes = (u64::from(ca_info.nFileSizeHigh) << 32) | u64::from(ca_info.nFileSizeLow);
    if ca_bytes == 0 || ca_bytes > 4 * MIB {
        return Err(ClaudeLaunchError::InvalidCa);
    }
    artifacts.push(ca_file);
    let home = context.private_home.paths().home();
    let mut environment: BTreeMap<OsString, OsString> = if trial {
        BTreeMap::new()
    } else {
        let values: Vec<_> = std::env::vars_os().take(2049).collect();
        if values.len() > 2048 {
            return Err(ClaudeLaunchError::Composition);
        }
        values.into_iter().collect()
    };
    let system = std::env::var_os("SystemRoot").ok_or(ClaudeLaunchError::Composition)?;
    set(&mut environment, "SystemRoot", system.clone());
    set(&mut environment, "WINDIR", system.clone());
    if trial {
        set(
            &mut environment,
            "PATH",
            PathBuf::from(system).join("System32").into_os_string(),
        );
    }
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
                .map_err(|_| ClaudeLaunchError::PrivateHome)?;
            directory
                .prepare_private_directory()
                .map_err(|_| ClaudeLaunchError::PrivateHome)?;
            locations.push(directory);
        }
        set(&mut environment, name, value.into_os_string());
    }
    let proxy = format!("http://127.0.0.1:{}", context.proxy_port);
    for name in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
        set(&mut environment, name, proxy.clone().into());
    }
    for name in ["NO_PROXY", "no_proxy"] {
        set(&mut environment, name, OsString::new());
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
    for name in [
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
        "DISABLE_AUTOUPDATER",
        "DISABLE_UPDATES",
        "DISABLE_TELEMETRY",
        "DISABLE_ERROR_REPORTING",
        "CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL",
        "CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES",
        // Narrow initial recipe: the canary and normal entry BOTH retain safe
        // mode, so a safe-mode observation cannot qualify unobserved hooks/tools.
        "CLAUDE_CODE_SAFE_MODE",
    ] {
        set(&mut environment, name, "1".into());
    }
    set(
        &mut environment,
        "CLAUDE_CODE_AUTO_CONNECT_IDE",
        "false".into(),
    );
    set(
        &mut environment,
        "CLAUDE_CODE_TMPDIR",
        context
            .private_home
            .paths()
            .temporary()
            .as_os_str()
            .to_owned(),
    );
    let descriptor = descriptor(&version)?;
    let mode = Placement::new(ProgramPlacement::System, HomePlacement::Box)
        .map_err(|_| ClaudeLaunchError::Composition)?;
    let placement_recipe = QualifiedPlacementRecipe::qualify(
        &descriptor,
        mode,
        RecipeDefinition {
            home: HomeMapping::Claude,
            runtime: RuntimeKind::Native,
            features: [
                RecipeFeature::OwnerOnlyGrant,
                RecipeFeature::PrivateTemporary,
                RecipeFeature::ClaudeMcpLogAttempt,
                RecipeFeature::ClaudeMcpLogLeakDisclosed,
            ]
            .into_iter()
            .collect(),
            limits: vec![],
            pi_skills: None,
            codex_sandbox: None,
        },
        evidence(&descriptor)?,
    )
    .map_err(|_| ClaudeLaunchError::Composition)?;
    let args = vec![
        "--setting-sources".into(),
        "project,local".into(),
        "--settings".into(),
        settings_path.into_os_string(),
        "--mcp-config".into(),
        mcp_path.into_os_string(),
        "--strict-mcp-config".into(),
    ];
    let arguments_recipe = QualifiedArgvRecipe::qualify(
        &descriptor,
        Operation::StartNew,
        CommandForm::NativeExe(pinned.path),
        args,
        evidence(&descriptor)?,
    )
    .map_err(|_| ClaudeLaunchError::Composition)?;
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
        .map_err(|_| ClaudeLaunchError::Composition)?;
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
        return Err(ClaudeLaunchError::Expired);
    }
    Ok(Prepared {
        launch,
        binding,
        profile: resource_profile,
        pins,
    })
}
fn verify_sidecar(sidecar: &SidecarSupervisor) -> Result<(), ClaudeLaunchError> {
    let status = sidecar
        .status()
        .ok_or(ClaudeLaunchError::SidecarUnavailable)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ClaudeLaunchError::SidecarUnavailable)?
        .as_millis() as i64;
    if status.phase != Phase::Ready
        || !status.healthy
        || status.process_alive != Some(true)
        || status
            .observed_at_unix_ms
            .is_none_or(|at| at > now || now - at > 10_000)
    {
        return Err(ClaudeLaunchError::SidecarUnavailable);
    }
    Ok(())
}
fn descriptor(version: &Version) -> Result<AdapterDescriptor, ClaudeLaunchError> {
    let mut descriptor = AdapterDescriptor {
        agent: AgentId::new("claude_code").map_err(|_| ClaudeLaunchError::Composition)?,
        version: version.clone(),
        display_name: "Claude local TUI qualification".into(),
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
            .map_err(|_| ClaudeLaunchError::Composition)?;
    }
    Ok(descriptor)
}
fn evidence(descriptor: &AdapterDescriptor) -> Result<FixtureEvidence, ClaudeLaunchError> {
    FixtureEvidence::new(
        descriptor.agent.clone(),
        descriptor.version.clone(),
        RECIPE,
        EvidenceKind::InstalledRuntimeFixture,
    )
    .map_err(|_| ClaudeLaunchError::Composition)
}
fn set(environment: &mut BTreeMap<OsString, OsString>, name: &str, value: OsString) {
    environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
    environment.insert(name.into(), value);
}
fn same_path(left: &Path, right: &Path) -> Result<bool, ClaudeLaunchError> {
    Ok(left
        .canonicalize()
        .map_err(|_| ClaudeLaunchError::PrivateHome)?
        .as_os_str()
        .eq_ignore_ascii_case(
            right
                .canonicalize()
                .map_err(|_| ClaudeLaunchError::PrivateHome)?
                .as_os_str(),
        ))
}
fn read_settings(path: PathBuf) -> Result<Value, ClaudeLaunchError> {
    if !path
        .try_exists()
        .map_err(|_| ClaudeLaunchError::Configuration)?
    {
        return Ok(serde_json::json!({}));
    }
    let file = pin_file(&path)?;
    if file
        .metadata()
        .map_err(|_| ClaudeLaunchError::Configuration)?
        .len()
        > 1024 * 1024
    {
        return Err(ClaudeLaunchError::Configuration);
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ClaudeLaunchError::Configuration)?;
    let mut value: Value =
        serde_json::from_slice(&bytes).map_err(|_| ClaudeLaunchError::Configuration)?;
    let object = value
        .as_object_mut()
        .ok_or(ClaudeLaunchError::Configuration)?;
    object.remove("enabledPlugins");
    object.remove("extraKnownMarketplaces");
    Ok(value)
}
fn create_json(
    location: &StorageLocation,
    name: &str,
    value: &Value,
    pins: &mut Vec<File>,
) -> Result<PathBuf, ClaudeLaunchError> {
    let path = location.directory().join(name);
    let bytes = serde_json::to_vec(value).map_err(|_| ClaudeLaunchError::Configuration)?;
    if bytes.len() > 1024 * 1024 {
        return Err(ClaudeLaunchError::Configuration);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(&path)
        .map_err(|_| ClaudeLaunchError::Configuration)?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| ClaudeLaunchError::Configuration)?;
    drop(file);
    location
        .secure_private_file(&path)
        .map_err(|_| ClaudeLaunchError::Configuration)?;
    pins.push(pin_file(&path)?);
    Ok(path)
}
fn file_info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, ClaudeLaunchError> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(ClaudeLaunchError::UnsafeProgram);
    }
    Ok(info)
}
fn pin_file(path: &Path) -> Result<File, ClaudeLaunchError> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| ClaudeLaunchError::UnsafeProgram)?;
    let info = file_info(&file)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(ClaudeLaunchError::UnsafeProgram);
    }
    Ok(file)
}
fn pin_directory(path: &Path) -> Result<(File, Vec<File>), ClaudeLaunchError> {
    StorageLocation::preflight(path).map_err(|_| ClaudeLaunchError::PrivateHome)?;
    let parents: Vec<_> = path.ancestors().take(65).collect();
    if parents.len() > 64 {
        return Err(ClaudeLaunchError::PrivateHome);
    }
    let mut pins = Vec::new();
    for parent in parents.into_iter().rev() {
        let file = OpenOptions::new()
            .read(true)
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(parent)
            .map_err(|_| ClaudeLaunchError::PrivateHome)?;
        let info = file_info(&file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(ClaudeLaunchError::PrivateHome);
        }
        pins.push(file);
    }
    let root = pins.pop().ok_or(ClaudeLaunchError::PrivateHome)?;
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
fn pin_program(path: &Path, deadline: Instant) -> Result<ProgramPin, ClaudeLaunchError> {
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("claude.exe"))
    {
        return Err(ClaudeLaunchError::InvalidCandidate);
    }
    let (_parent, mut ancestors) =
        pin_directory(path.parent().ok_or(ClaudeLaunchError::InvalidCandidate)?)?;
    ancestors.push(_parent);
    let mut file = pin_file(path)?;
    let before = file_info(&file)?;
    let bytes = (u64::from(before.nFileSizeHigh) << 32) | u64::from(before.nFileSizeLow);
    if !(64..=512 * MIB).contains(&bytes) {
        return Err(ClaudeLaunchError::UnsafeProgram);
    }
    let mut dos = [0; 64];
    file.read_exact(&mut dos)
        .map_err(|_| ClaudeLaunchError::UnsafeProgram)?;
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if &dos[..2] != b"MZ" || offset > MIB || offset + 4 > bytes {
        return Err(ClaudeLaunchError::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| ClaudeLaunchError::UnsafeProgram)?;
    let mut pe = [0; 4];
    file.read_exact(&mut pe)
        .map_err(|_| ClaudeLaunchError::UnsafeProgram)?;
    if pe != *b"PE\0\0" {
        return Err(ClaudeLaunchError::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ClaudeLaunchError::UnsafeProgram)?;
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0; 65536];
    loop {
        if Instant::now() >= deadline {
            return Err(ClaudeLaunchError::Expired);
        }
        let count = file
            .read(&mut buffer)
            .map_err(|_| ClaudeLaunchError::UnsafeProgram)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > bytes {
            return Err(ClaudeLaunchError::ProgramChanged);
        }
        hash.update(&buffer[..count]);
    }
    let digest: [u8; 32] = hash.finalize().into();
    let after = file_info(&file)?;
    if read != bytes
        || before.dwVolumeSerialNumber != after.dwVolumeSerialNumber
        || before.nFileIndexHigh != after.nFileIndexHigh
        || before.nFileIndexLow != after.nFileIndexLow
        || before.ftLastWriteTime.dwLowDateTime != after.ftLastWriteTime.dwLowDateTime
        || before.ftLastWriteTime.dwHighDateTime != after.ftLastWriteTime.dwHighDateTime
    {
        return Err(ClaudeLaunchError::ProgramChanged);
    }
    let identity = NativeProgramIdentity::new(
        before.dwVolumeSerialNumber,
        (u64::from(before.nFileIndexHigh) << 32) | u64::from(before.nFileIndexLow),
        bytes,
        (u64::from(before.ftLastWriteTime.dwHighDateTime) << 32)
            | u64::from(before.ftLastWriteTime.dwLowDateTime),
        digest,
    )?;
    Ok(ProgramPin {
        path: path
            .canonicalize()
            .map_err(|_| ClaudeLaunchError::UnsafeProgram)?,
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
