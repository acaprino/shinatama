//! Windows System+Box Codex startup with native workspace permissions and no
//! background daemon. Preparation performs bounded local
//! reads/writes; only the owned ConPTY canary can qualify the exact build/profile.
//! No provider request, login, user credential read, shell or marketplace install.
#![cfg(windows)]
use crate::{
    codex_probe::{CodexProbeOutcome, ProbeCleanup},
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
            CodexSandbox, HomeMapping, PathLimit, PathRole, PathUnit, PlacementPaths,
            QualifiedPlacementRecipe, RecipeDefinition, RecipeFeature, RuntimeKind,
        },
    },
    sets::SetManifest,
};
use shinatama_state::{
    database::Database,
    location::StorageLocation,
    records::{RootAccess, RootKind, RootScope, StoreRootRecord},
    resource_reservations::Budget,
    store_roots::{HomePathLimit, PathLengthUnit, PreparedPrivateStore, StorePathRole},
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    num::{NonZeroU16, NonZeroUsize},
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
const RECIPE: &str = "codex-system-box-no-daemon-workspace-tui-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexLaunchError {
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
impl std::fmt::Display for CodexLaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Codex startup prerequisite: {self:?}.")
    }
}
impl std::error::Error for CodexLaunchError {}
impl From<NativeLaunchError> for CodexLaunchError {
    fn from(_: NativeLaunchError) -> Self {
        Self::Permit
    }
}

/// Trusted backend values only. No Deserialize, custom argv, environment, model,
/// credentials, resource budget or qualification boolean is accepted from IPC.
pub struct CodexStartContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a CodexProbeOutcome,
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
pub struct PreparedCodexTrial {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    pub tracker: NativeTrialTracker,
}
pub struct CodexTrialContext<'a> {
    pub candidate: &'a ProgramCandidate,
    pub observation: &'a CodexProbeOutcome,
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
    observation: &'a CodexProbeOutcome,
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
pub struct PreparedCodexStart {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
}

/// A conservative local-path budget for this no-daemon recipe. It is not
/// the shorter Unix-socket budget: `--no-daemon` removes that socket entirely.
/// Pass the same value into the private-store plan before creating any paths.
pub fn home_path_limit() -> HomePathLimit {
    HomePathLimit {
        maximum: 200,
        unit: PathLengthUnit::Utf16Units,
    }
}

fn fixed_arguments() -> Vec<OsString> {
    [
        "--no-daemon",
        "--sandbox",
        "workspace-write",
        "--ask-for-approval",
        "on-request",
        "-c",
        "windows.sandbox=\"unelevated\"",
        "-c",
        "cli_auth_credentials_store=\"file\"",
        "-c",
        "check_for_update_on_startup=false",
        "-c",
        "analytics.enabled=false",
        "-c",
        "feedback.enabled=false",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

/// This is the fixed finite trial profile, not inferred from a growing
/// process and never raised on failure. Issuance is sealed inside this qualifier.
fn profile() -> Result<NativeAdmissionProfile, CodexLaunchError> {
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
    context: CodexTrialContext<'_>,
) -> Result<PreparedCodexTrial, CodexLaunchError> {
    // Filename presence only: never read a credential store for this canary.
    if context
        .private_home
        .paths()
        .home()
        .join("auth.json")
        .try_exists()
        .map_err(|_| CodexLaunchError::PrivateHome)?
    {
        return Err(CodexLaunchError::PrivateHome);
    }
    if context
        .private_home
        .paths()
        .home()
        .read_dir()
        .map_err(|_| CodexLaunchError::PrivateHome)?
        .next()
        .transpose()
        .map_err(|_| CodexLaunchError::PrivateHome)?
        .is_some()
    {
        return Err(CodexLaunchError::PrivateHome);
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
    Ok(PreparedCodexTrial {
        launch: prepared.launch,
        permit,
        tracker,
    })
}
pub fn prepare_start(
    context: CodexStartContext<'_>,
    receipt: &NativeTrialReceipt,
) -> Result<PreparedCodexStart, CodexLaunchError> {
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
    Ok(PreparedCodexStart {
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
) -> Result<Prepared, CodexLaunchError> {
    let deadline = Instant::now() + PREPARE_BUDGET;
    if context.candidate.agent.as_str() != "codex"
        || context.candidate.kind != ProgramKind::DirectExe
    {
        return Err(CodexLaunchError::InvalidCandidate);
    }
    let observation = context.observation.summary();
    let version = observation
        .version
        .clone()
        .ok_or(CodexLaunchError::ObservationRequired)?;
    if observation.issue.is_some()
        || observation.cleanup != ProbeCleanup::Released
        || context.observation.cleanup_retained()
    {
        return Err(CodexLaunchError::ObservationRequired);
    }
    if !context.selected_set.is_empty() {
        return Err(CodexLaunchError::UnsupportedSet);
    }
    verify_sidecar(context.sidecar)?;
    context
        .base
        .verify_private_directory()
        .map_err(|_| CodexLaunchError::PrivateHome)?;
    let root = if let (Some(database), Some(expected)) = (context.database, context.root.as_ref()) {
        let root = database
            .store_root(&expected.id)
            .map_err(|_| CodexLaunchError::RootChanged)?
            .filter(|root| root == expected)
            .ok_or(CodexLaunchError::RootChanged)?;
        if root.harness != "codex"
            || root.kind != RootKind::Box
            || root.scope != RootScope::Primary
            || root.access != RootAccess::ReadWrite
            || !root.available
            || !same_path(
                Path::new(&root.resolved_path),
                context.private_home.paths().home(),
            )?
        {
            return Err(CodexLaunchError::RootChanged);
        }
        Some(root)
    } else if trial && context.trial_base.is_some() {
        None
    } else {
        return Err(CodexLaunchError::RootChanged);
    };
    let project = context
        .project
        .canonicalize()
        .map_err(|_| CodexLaunchError::InvalidProject)?;
    if !project.is_dir() {
        return Err(CodexLaunchError::InvalidProject);
    }
    if let Some(trial_base) = context.trial_base {
        trial_base
            .verify_private_directory()
            .map_err(|_| CodexLaunchError::PrivateHome)?;
        let relative = trial_base
            .directory()
            .strip_prefix(context.base.directory())
            .map_err(|_| CodexLaunchError::PrivateHome)?;
        let name = relative.to_str().ok_or(CodexLaunchError::PrivateHome)?;
        let suffix = name
            .strip_prefix("codex-tui-trial-")
            .ok_or(CodexLaunchError::PrivateHome)?;
        let trial_id = uuid::Uuid::parse_str(suffix).map_err(|_| CodexLaunchError::PrivateHome)?;
        if trial_id.get_version() != Some(uuid::Version::Random)
            || !same_path(&project, &trial_base.directory().join("project"))?
            || !same_path(
                context.private_home.paths().root(),
                &trial_base.directory().join("agents/codex"),
            )?
        {
            return Err(CodexLaunchError::PrivateHome);
        }
    } else {
        context
            .base
            .validate_project(&project)
            .map_err(|_| CodexLaunchError::InvalidProject)?;
    }
    let mut pinned = pin_program(&context.candidate.path, deadline)?;
    if pinned.hash != observation.executable_sha256 || pinned.bytes != observation.executable_bytes
    {
        return Err(CodexLaunchError::ProgramChanged);
    }
    let resource_profile = profile()?;
    if let Some(receipt) = receipt {
        if !receipt.matches(
            &context.candidate.agent,
            &pinned.identity,
            &version,
            &resource_profile,
        ) {
            return Err(CodexLaunchError::QualificationRequired);
        }
    } else if !trial {
        return Err(CodexLaunchError::QualificationRequired);
    }
    let identity =
        LaunchIdentity::generate(context.thread_id).map_err(|_| CodexLaunchError::Identity)?;
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
    .map_err(|_| CodexLaunchError::PrivateHome)?;
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
        .map_err(|_| CodexLaunchError::PrivateHome)?;
    instance_directory
        .prepare_private_directory()
        .map_err(|_| CodexLaunchError::PrivateHome)?;
    let mut locations = vec![context.base.clone(), instance_directory.clone()];
    if let Some(trial_base) = context.trial_base {
        locations.push(trial_base.clone());
    }
    let mut artifacts = Vec::new();
    create_json(
        &instance_directory,
        "set-manifest.json",
        &serde_json::to_value(context.selected_set).map_err(|_| CodexLaunchError::Configuration)?,
        &mut artifacts,
    )?;
    let ca = context
        .ca_file
        .canonicalize()
        .map_err(|_| CodexLaunchError::InvalidCa)?;
    let expected_ca = context
        .base
        .directory()
        .join("sidecar/data/ca/combined-ca.pem");
    if !same_path(&ca, &expected_ca)? {
        return Err(CodexLaunchError::InvalidCa);
    }
    let ca_file = pin_file(&ca).map_err(|_| CodexLaunchError::InvalidCa)?;
    let ca_info = file_info(&ca_file)?;
    let ca_bytes = (u64::from(ca_info.nFileSizeHigh) << 32) | u64::from(ca_info.nFileSizeLow);
    if ca_bytes == 0 || ca_bytes > 4 * MIB {
        return Err(CodexLaunchError::InvalidCa);
    }
    artifacts.push(ca_file);
    let home = context.private_home.paths().home();
    let mut environment: BTreeMap<OsString, OsString> = if trial {
        BTreeMap::new()
    } else {
        let values: Vec<_> = std::env::vars_os().take(2049).collect();
        if values.len() > 2048 {
            return Err(CodexLaunchError::Composition);
        }
        values.into_iter().collect()
    };
    let system = std::env::var_os("SystemRoot").ok_or(CodexLaunchError::Composition)?;
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
                .map_err(|_| CodexLaunchError::PrivateHome)?;
            directory
                .prepare_private_directory()
                .map_err(|_| CodexLaunchError::PrivateHome)?;
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
    for name in ["CODEX_CA_CERTIFICATE", "SSL_CERT_FILE"] {
        set(&mut environment, name, ca.as_os_str().to_owned());
    }
    // Do not inherit another Codex process's alternate database, daemon or
    // logging targets. The private home remains the sole persistent namespace.
    for name in [
        "CODEX_SQLITE_HOME",
        "CODEX_MANAGED_PACKAGE_ROOT",
        "CODEX_MANAGED_BY_NPM",
        "RUST_LOG",
        "RUST_LOG_STYLE",
    ] {
        environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
    }
    set(
        &mut environment,
        "CODEX_SQLITE_HOME",
        home.as_os_str().to_owned(),
    );
    let descriptor = descriptor(&version)?;
    let mode = Placement::new(ProgramPlacement::System, HomePlacement::Box)
        .map_err(|_| CodexLaunchError::Composition)?;
    let placement_recipe = QualifiedPlacementRecipe::qualify(
        &descriptor,
        mode,
        RecipeDefinition {
            home: HomeMapping::Codex,
            runtime: RuntimeKind::Native,
            features: [
                RecipeFeature::OwnerOnlyGrant,
                RecipeFeature::PrivateTemporary,
                RecipeFeature::CodexDaemonDisabled,
                RecipeFeature::CodexElevatedSandboxExcluded,
                RecipeFeature::CodexDenybinLeakDisclosed,
            ]
            .into_iter()
            .collect(),
            limits: vec![PathLimit {
                role: PathRole::Home,
                unit: PathUnit::Utf16Units,
                maximum: NonZeroUsize::new(home_path_limit().maximum).unwrap(),
            }],
            pi_skills: None,
            codex_sandbox: Some(CodexSandbox::UnelevatedDisclosed),
        },
        evidence(&descriptor)?,
    )
    .map_err(|_| CodexLaunchError::Composition)?;
    let args = fixed_arguments();
    let arguments_recipe = QualifiedArgvRecipe::qualify(
        &descriptor,
        Operation::StartNew,
        CommandForm::NativeExe(pinned.path),
        args,
        evidence(&descriptor)?,
    )
    .map_err(|_| CodexLaunchError::Composition)?;
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
        .map_err(|_| CodexLaunchError::Composition)?;
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
        return Err(CodexLaunchError::Expired);
    }
    Ok(Prepared {
        launch,
        binding,
        profile: resource_profile,
        pins,
    })
}
fn verify_sidecar(sidecar: &SidecarSupervisor) -> Result<(), CodexLaunchError> {
    let status = sidecar
        .status()
        .ok_or(CodexLaunchError::SidecarUnavailable)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CodexLaunchError::SidecarUnavailable)?
        .as_millis() as i64;
    if status.phase != Phase::Ready
        || !status.healthy
        || status.process_alive != Some(true)
        || status
            .observed_at_unix_ms
            .is_none_or(|at| at > now || now - at > 10_000)
    {
        return Err(CodexLaunchError::SidecarUnavailable);
    }
    Ok(())
}
fn descriptor(version: &Version) -> Result<AdapterDescriptor, CodexLaunchError> {
    let mut descriptor = AdapterDescriptor {
        agent: AgentId::new("codex").map_err(|_| CodexLaunchError::Composition)?,
        version: version.clone(),
        display_name: "Codex local TUI qualification".into(),
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
            .map_err(|_| CodexLaunchError::Composition)?;
    }
    Ok(descriptor)
}
fn evidence(descriptor: &AdapterDescriptor) -> Result<FixtureEvidence, CodexLaunchError> {
    FixtureEvidence::new(
        descriptor.agent.clone(),
        descriptor.version.clone(),
        RECIPE,
        EvidenceKind::InstalledRuntimeFixture,
    )
    .map_err(|_| CodexLaunchError::Composition)
}
fn set(environment: &mut BTreeMap<OsString, OsString>, name: &str, value: OsString) {
    environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
    environment.insert(name.into(), value);
}
fn same_path(left: &Path, right: &Path) -> Result<bool, CodexLaunchError> {
    Ok(left
        .canonicalize()
        .map_err(|_| CodexLaunchError::PrivateHome)?
        .as_os_str()
        .eq_ignore_ascii_case(
            right
                .canonicalize()
                .map_err(|_| CodexLaunchError::PrivateHome)?
                .as_os_str(),
        ))
}
fn create_json(
    location: &StorageLocation,
    name: &str,
    value: &Value,
    pins: &mut Vec<File>,
) -> Result<PathBuf, CodexLaunchError> {
    let path = location.directory().join(name);
    let bytes = serde_json::to_vec(value).map_err(|_| CodexLaunchError::Configuration)?;
    if bytes.len() > 1024 * 1024 {
        return Err(CodexLaunchError::Configuration);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(&path)
        .map_err(|_| CodexLaunchError::Configuration)?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| CodexLaunchError::Configuration)?;
    drop(file);
    location
        .secure_private_file(&path)
        .map_err(|_| CodexLaunchError::Configuration)?;
    pins.push(pin_file(&path)?);
    Ok(path)
}
fn file_info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, CodexLaunchError> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(CodexLaunchError::UnsafeProgram);
    }
    Ok(info)
}
fn pin_file(path: &Path) -> Result<File, CodexLaunchError> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| CodexLaunchError::UnsafeProgram)?;
    let info = file_info(&file)?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DIRECTORY) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(CodexLaunchError::UnsafeProgram);
    }
    Ok(file)
}
fn pin_directory(path: &Path) -> Result<(File, Vec<File>), CodexLaunchError> {
    StorageLocation::preflight(path).map_err(|_| CodexLaunchError::PrivateHome)?;
    let parents: Vec<_> = path.ancestors().take(65).collect();
    if parents.len() > 64 {
        return Err(CodexLaunchError::PrivateHome);
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
            .map_err(|_| CodexLaunchError::PrivateHome)?;
        let info = file_info(&file)?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(CodexLaunchError::PrivateHome);
        }
        pins.push(file);
    }
    let root = pins.pop().ok_or(CodexLaunchError::PrivateHome)?;
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
fn pin_program(path: &Path, deadline: Instant) -> Result<ProgramPin, CodexLaunchError> {
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("codex.exe"))
    {
        return Err(CodexLaunchError::InvalidCandidate);
    }
    let (_parent, mut ancestors) =
        pin_directory(path.parent().ok_or(CodexLaunchError::InvalidCandidate)?)?;
    ancestors.push(_parent);
    let mut file = pin_file(path)?;
    let before = file_info(&file)?;
    let bytes = (u64::from(before.nFileSizeHigh) << 32) | u64::from(before.nFileSizeLow);
    if !(64..=512 * MIB).contains(&bytes) {
        return Err(CodexLaunchError::UnsafeProgram);
    }
    let mut dos = [0; 64];
    file.read_exact(&mut dos)
        .map_err(|_| CodexLaunchError::UnsafeProgram)?;
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if &dos[..2] != b"MZ" || offset > MIB || offset + 4 > bytes {
        return Err(CodexLaunchError::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| CodexLaunchError::UnsafeProgram)?;
    let mut pe = [0; 4];
    file.read_exact(&mut pe)
        .map_err(|_| CodexLaunchError::UnsafeProgram)?;
    if pe != *b"PE\0\0" {
        return Err(CodexLaunchError::UnsafeProgram);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| CodexLaunchError::UnsafeProgram)?;
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0; 65536];
    loop {
        if Instant::now() >= deadline {
            return Err(CodexLaunchError::Expired);
        }
        let count = file
            .read(&mut buffer)
            .map_err(|_| CodexLaunchError::UnsafeProgram)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > bytes {
            return Err(CodexLaunchError::ProgramChanged);
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
        return Err(CodexLaunchError::ProgramChanged);
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
            .map_err(|_| CodexLaunchError::UnsafeProgram)?,
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
