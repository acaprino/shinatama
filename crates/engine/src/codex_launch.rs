//! Windows System+Box Codex startup with native workspace permissions and no
//! background daemon. Preparation performs bounded local
//! reads/writes; only the owned ConPTY canary can qualify the exact build/profile.
//! No provider request, login, user credential read, shell or marketplace install.
#![cfg(windows)]
#[cfg(test)]
mod native_hook_test;
use crate::{
    marionette::{Host, Marionette, is_marionette_set},
    codex_probe::{CodexProbeOutcome, ProbeCleanup},
    harness_session_events::{HarnessKind, SessionEventReceiver},
    launch::{LaunchInputs, PreparedLaunch, compose_launch, identity::LaunchIdentity},
    native_launch::{
        NativeAdmissionProfile, NativeLaunchBinding, NativeLaunchError, NativeLaunchPins,
        NativeProgramIdentity, NativeTrialReceipt, NativeTrialTracker, NativeTuiPermit,
    },
    sidecar_supervisor::SidecarSupervisor,
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
const TRACKED_RECIPE: &str = "codex-system-box-tracked-no-daemon-workspace-tui-v1";
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
    BundledPlugins,
    Configuration,
    InvalidNativeSession,
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

/// A bounded selection, not a native launch permit or evidence of a saved
/// conversation. New lets Codex choose its UUID; Resume preserves an exact
/// recorded UUID. There is deliberately no public Resume preparation entry
/// point until its native identity capture and qualification are implemented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexSessionOperation {
    selection: CodexSessionSelection,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum CodexSessionSelection {
    New,
    Resume(String),
}
impl CodexSessionOperation {
    pub fn start_new() -> Self {
        Self {
            selection: CodexSessionSelection::New,
        }
    }
    pub fn resume_native(native_id: &str) -> Result<Self, CodexLaunchError> {
        if native_id.len() != 36 {
            return Err(CodexLaunchError::InvalidNativeSession);
        }
        let uuid =
            uuid::Uuid::parse_str(native_id).map_err(|_| CodexLaunchError::InvalidNativeSession)?;
        // Codex also accepts names and prefixes in some entry points. Those are
        // not exact durable identities; never normalize or resolve one here.
        if uuid.hyphenated().to_string() != native_id
            || uuid.get_variant() != uuid::Variant::RFC4122
            || uuid.get_version().is_none()
        {
            return Err(CodexLaunchError::InvalidNativeSession);
        }
        Ok(Self {
            selection: CodexSessionSelection::Resume(native_id.into()),
        })
    }
    pub fn operation(&self) -> Operation {
        match self.selection {
            CodexSessionSelection::New => Operation::StartNew,
            CodexSessionSelection::Resume(_) => Operation::ResumeNative,
        }
    }
    pub fn native_id(&self) -> Option<&str> {
        match &self.selection {
            CodexSessionSelection::New => None,
            CodexSessionSelection::Resume(native_id) => Some(native_id),
        }
    }
    fn arguments(&self) -> Vec<OsString> {
        let mut arguments = Vec::new();
        if let CodexSessionSelection::Resume(native_id) = &self.selection {
            arguments.extend([OsString::from("resume"), native_id.into()]);
        }
        arguments.extend(fixed_arguments());
        arguments
    }
    fn qualify_arguments(
        &self,
        descriptor: &AdapterDescriptor,
        executable: PathBuf,
    ) -> Result<QualifiedArgvRecipe, CodexLaunchError> {
        self.qualify_arguments_with_hooks(descriptor, executable, &[])
    }
    fn qualify_arguments_with_hooks(
        &self,
        descriptor: &AdapterDescriptor,
        executable: PathBuf,
        hook_arguments: &[OsString],
    ) -> Result<QualifiedArgvRecipe, CodexLaunchError> {
        // The recipe records its own exact operation and the sealed launch
        // permit binds the argv digest, so resume argv can never ride a
        // StartNew permit or its native qualification. The shared trial
        // receipt covers contained TUI startup only; resume fidelity is
        // proved by the lifecycle's exact sealed-UUID hook confirmation.
        let mut arguments = self.arguments();
        arguments.extend_from_slice(hook_arguments);
        QualifiedArgvRecipe::qualify(
            descriptor,
            self.operation(),
            CommandForm::NativeExe(executable),
            arguments,
            evidence(
                descriptor,
                if hook_arguments.is_empty() {
                    RECIPE
                } else {
                    TRACKED_RECIPE
                },
            )?,
        )
        .map_err(|_| CodexLaunchError::Composition)
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
    /// Synced Marionette store; required when the set is the Marionette one.
    pub plugins_root: Option<&'a Path>,
    pub thread_id: &'a str,
}
pub struct PreparedCodexTrial {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    pub tracker: NativeTrialTracker,
    /// Retained through trial cleanup; presence is not proof of event delivery.
    pub session_events: Option<SessionEventReceiver>,
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
    plugins_root: Option<&'a Path>,
    thread_id: &'a str,
}
pub struct PreparedCodexStart {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    /// Owned by the session lifecycle through native cleanup, never sent to IPC.
    pub session_events: Option<SessionEventReceiver>,
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
fn profile(tracked: bool) -> Result<NativeAdmissionProfile, CodexLaunchError> {
    Ok(NativeAdmissionProfile::new(
        if tracked { TRACKED_RECIPE } else { RECIPE },
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
    prepare_trial_with_hooks(context, None)
}
/// Qualifies startup for the distinct tracked profile. Native event delivery
/// and resume fidelity still require their own observations.
pub fn prepare_tracked_trial(
    context: CodexTrialContext<'_>,
    trusted_hook_exe: &Path,
) -> Result<PreparedCodexTrial, CodexLaunchError> {
    prepare_trial_with_hooks(context, Some(trusted_hook_exe))
}
fn prepare_trial_with_hooks(
    context: CodexTrialContext<'_>,
    trusted_hook_exe: Option<&Path>,
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
            plugins_root: None,
            thread_id: context.thread_id,
        },
        None,
        true,
        None,
        trusted_hook_exe,
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
        session_events: prepared.session_events,
    })
}
pub fn prepare_start(
    context: CodexStartContext<'_>,
    receipt: &NativeTrialReceipt,
) -> Result<PreparedCodexStart, CodexLaunchError> {
    prepare_for_operation(context, receipt, None, None)
}
/// Codex chooses the native UUID. The lifecycle must consume the authenticated
/// SessionStart event; this function does not invent or reserve a native ID.
pub fn prepare_tracked_start(
    context: CodexStartContext<'_>,
    receipt: &NativeTrialReceipt,
    trusted_hook_exe: &Path,
) -> Result<PreparedCodexStart, CodexLaunchError> {
    prepare_for_operation(context, receipt, None, Some(trusted_hook_exe))
}
/// Compose an exact new/resume invocation with the existing qualified startup
/// protections. The receipt covers contained TUI startup, not native transcript
/// fidelity; a Resume selection carries its exact recorded UUID into the sealed
/// argv recipe and launch binding, and the lifecycle confirms it through the
/// authenticated SessionStart(source=resume) hook event.
pub fn prepare_tracked_session(
    context: CodexStartContext<'_>,
    receipt: &NativeTrialReceipt,
    session: &CodexSessionOperation,
    trusted_hook_exe: &Path,
) -> Result<PreparedCodexStart, CodexLaunchError> {
    prepare_for_operation(context, receipt, Some(session), Some(trusted_hook_exe))
}
fn prepare_for_operation(
    context: CodexStartContext<'_>,
    receipt: &NativeTrialReceipt,
    session: Option<&CodexSessionOperation>,
    trusted_hook_exe: Option<&Path>,
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
            plugins_root: context.plugins_root,
            thread_id: context.thread_id,
        },
        Some(receipt),
        false,
        session,
        trusted_hook_exe,
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
        session_events: prepared.session_events,
    })
}
struct Prepared {
    launch: PreparedLaunch,
    binding: NativeLaunchBinding,
    profile: NativeAdmissionProfile,
    pins: NativeLaunchPins,
    session_events: Option<SessionEventReceiver>,
}

fn prepare(
    context: Context<'_>,
    receipt: Option<&NativeTrialReceipt>,
    trial: bool,
    session: Option<&CodexSessionOperation>,
    trusted_hook_exe: Option<&Path>,
) -> Result<Prepared, CodexLaunchError> {
    let deadline = Instant::now() + PREPARE_BUDGET;
    let default_session = CodexSessionOperation::start_new();
    let session = session.unwrap_or(&default_session);
    // The contained canary only ever qualifies the fresh-session startup.
    if trial && session.operation() != Operation::StartNew {
        return Err(CodexLaunchError::QualificationRequired);
    }
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
    // Empty set, or exactly Marionette for a tracked real session (never the
    // trial). Digests are checked at every launch.
    let bundled = if context.selected_set.is_empty() {
        None
    } else if is_marionette_set(context.selected_set) && !trial && trusted_hook_exe.is_some() {
        let root = context
            .plugins_root
            .ok_or(CodexLaunchError::BundledPlugins)?;
        Some(
            Marionette::current(root)
                .and_then(|release| release.verify(Host::Codex))
                .map_err(|_| CodexLaunchError::BundledPlugins)?,
        )
    } else {
        return Err(CodexLaunchError::UnsupportedSet);
    };
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
    let resource_profile = profile(trusted_hook_exe.is_some())?;
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
    let mut hook_arguments = Vec::new();
    let session_events = if let Some(helper_path) = trusted_hook_exe {
        let mut helper = crate::harness_hooks::pin_session_hook(helper_path, deadline)
            .map_err(|_| CodexLaunchError::Configuration)?;
        let mut hook_shell = pin_hook_shell(deadline)?;
        let receiver =
            SessionEventReceiver::start_for(HarnessKind::Codex, identity.record(), &project)
                .map_err(|_| CodexLaunchError::Configuration)?;
        hook_arguments =
            session_hook_arguments(&version, &helper.path, &hook_shell.path, receiver.port())?;
        pinned.ancestors.append(&mut helper.ancestors);
        pinned.ancestors.append(&mut hook_shell.ancestors);
        artifacts.push(helper.file);
        artifacts.push(hook_shell.file);
        Some(receiver)
    } else {
        None
    };
    if let Some(plugins) = &bundled {
        // CODEX_HOME is the private home: place the verified adapter where Codex
        // looks for installed plugins, then enable it per launch with -c.
        plugins
            .install_codex_cache(context.private_home.paths().home())
            .map_err(|_| CodexLaunchError::BundledPlugins)?;
        hook_arguments.extend(plugins.codex_overrides().into_iter().map(OsString::from));
    }
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
    let recipe_id = if trusted_hook_exe.is_some() {
        TRACKED_RECIPE
    } else {
        RECIPE
    };
    let descriptor = descriptor(&version, session.operation(), recipe_id)?;
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
        evidence(&descriptor, recipe_id)?,
    )
    .map_err(|_| CodexLaunchError::Composition)?;
    let arguments_recipe = if session_events.is_some() {
        session.qualify_arguments_with_hooks(&descriptor, pinned.path, &hook_arguments)?
    } else {
        session.qualify_arguments(&descriptor, pinned.path)?
    };
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
    let mut launch = compose_launch(&descriptor, session.operation(), mode, &inputs)
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
        session_events,
    })
}
fn pin_hook_shell(
    deadline: Instant,
) -> Result<crate::harness_hooks::PinnedExecutable, CodexLaunchError> {
    // Ask Windows, not the caller's PATH/SystemRoot/COMSPEC. The held executable
    // and ancestor handles keep this trampoline stable through native cleanup.
    let mut directory = [0u16; 32768];
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            directory.as_mut_ptr(),
            directory.len() as u32,
        )
    } as usize;
    if length == 0 || length >= directory.len() {
        return Err(CodexLaunchError::Configuration);
    }
    let directory =
        String::from_utf16(&directory[..length]).map_err(|_| CodexLaunchError::Configuration)?;
    let path = PathBuf::from(directory).join("WindowsPowerShell/v1.0/powershell.exe");
    crate::harness_hooks::pin_executable(&path, "powershell.exe", deadline, true)
        .map_err(|_| CodexLaunchError::Configuration)
}

fn session_hook_arguments(
    version: &Version,
    helper: &Path,
    shell: &Path,
    port: u16,
) -> Result<Vec<OsString>, CodexLaunchError> {
    // This schema/hash is pinned to the source tag rust-v0.167.1, not inferred
    // from a successful version probe. Other versions need schema validation.
    if version != &Version::parse("0.167.1").map_err(|_| CodexLaunchError::Configuration)? {
        return Err(CodexLaunchError::QualificationRequired);
    }
    let command = session_hook_command(helper, shell, port)?;
    let matcher = "^(startup|resume|clear|compact|fork)$";
    // Codex hooks/engine/discovery.rs normalizes one handler to TOML, then
    // config/fingerprint.rs hashes its recursively sorted JSON. None fields
    // disappear in TOML; the default async=false remains part of the identity.
    let mut identity = serde_json::json!({
        "event_name": "session_start", "matcher": matcher,
        "hooks": [{"type":"command", "command":command, "timeout":5, "async":false}]
    });
    identity.sort_all_objects();
    let normalized = serde_json::to_vec(&identity).map_err(|_| CodexLaunchError::Configuration)?;
    let hash = format!("sha256:{:x}", Sha256::digest(normalized));
    let quoted_command =
        serde_json::to_string(&command).map_err(|_| CodexLaunchError::Configuration)?;
    let quoted_matcher =
        serde_json::to_string(matcher).map_err(|_| CodexLaunchError::Configuration)?;
    // SessionFlags is its own discovery layer. Existing project/user hooks and
    // their trust remain untouched; no global trust bypass or user-file write.
    //
    // Codex parses `-c` keys by splitting the raw string on '.' (utils/cli
    // config_override + config/overrides apply_toml_override), so a quoted
    // dotted key such as hooks.state."…config.toml:session_start:0:0" is torn
    // apart and silently ignored, leaving the hook untrusted. Carry the state
    // inside a single-segment `hooks` table instead; the TOML literal-string
    // key survives parsing intact. Verified against real Codex 0.167.1
    // (hooks/list: exact key, matching hash, trustStatus=trusted). Order is
    // significant: the single-segment override must precede the definition,
    // because a later one would replace the whole `hooks` table.
    Ok(vec![
        "-c".into(), "features.hooks=true".into(),
        "-c".into(),
        format!(
            "hooks={{state={{'C:\\<session-flags>\\config.toml:session_start:0:0'={{enabled=true,trusted_hash=\"{hash}\"}}}}}}"
        ).into(),
        "-c".into(), format!("hooks.SessionStart=[{{matcher={quoted_matcher},hooks=[{{type=\"command\",command={quoted_command},timeout=5,async=false}}]}}]").into(),
    ])
}

fn session_hook_command(
    helper: &Path,
    shell: &Path,
    port: u16,
) -> Result<String, CodexLaunchError> {
    let helper = helper.to_str().ok_or(CodexLaunchError::Configuration)?;
    let helper = helper
        .strip_prefix(r"\\?\")
        .unwrap_or(helper)
        .replace('\\', "/");
    let shell = shell.to_str().ok_or(CodexLaunchError::Configuration)?;
    let shell = shell
        .strip_prefix(r"\\?\")
        .unwrap_or(shell)
        .replace('\\', "/");
    let drive_path = |path: &str| {
        path.len() > 3
            && path.as_bytes()[0].is_ascii_alphabetic()
            && &path.as_bytes()[1..3] == b":/"
    };
    if port == 0
        || helper.len() > 4096
        || !drive_path(&helper)
        || helper.chars().any(char::is_control)
        || !drive_path(&shell)
        || !shell
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":/_.-".contains(&byte))
    {
        return Err(CodexLaunchError::Configuration);
    }
    // The outer command is valid in Cmd, PowerShell and Git Bash. Only the
    // trusted Windows path and base64 enter that parser; helper path quoting is
    // handled once by a no-profile PowerShell trampoline. No policy bypass.
    let script = session_hook_script(&helper, port);
    let bytes: Vec<_> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = encode_base64(&bytes);
    Ok(format!(
        "{shell} -NoProfile -NonInteractive -EncodedCommand {encoded}"
    ))
}

fn session_hook_script(helper: &str, port: u16) -> String {
    format!(
        "$ErrorActionPreference='Stop'; & '{}' codex-session {port}; exit $LASTEXITCODE",
        helper.replace('\'', "''")
    )
}

fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn verify_sidecar(sidecar: &SidecarSupervisor) -> Result<(), CodexLaunchError> {
    let status = sidecar
        .status()
        .ok_or(CodexLaunchError::SidecarUnavailable)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CodexLaunchError::SidecarUnavailable)?
        .as_millis() as i64;
    if !status.backs_launch(now) {
        return Err(CodexLaunchError::SidecarUnavailable);
    }
    Ok(())
}
fn descriptor(
    version: &Version,
    operation: Operation,
    recipe: &str,
) -> Result<AdapterDescriptor, CodexLaunchError> {
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
    for operation in [operation, Operation::Placement, Operation::InheritedMarkers] {
        descriptor
            .capabilities
            .qualify(
                operation,
                &descriptor.agent,
                version,
                false,
                evidence(&descriptor, recipe)?,
            )
            .map_err(|_| CodexLaunchError::Composition)?;
    }
    Ok(descriptor)
}
fn evidence(
    descriptor: &AdapterDescriptor,
    recipe: &str,
) -> Result<FixtureEvidence, CodexLaunchError> {
    FixtureEvidence::new(
        descriptor.agent.clone(),
        descriptor.version.clone(),
        recipe,
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
    fn tracked_codex_uses_a_distinct_profile_with_unchanged_limits() {
        let baseline = profile(false).unwrap();
        let tracked = profile(true).unwrap();
        assert_eq!(baseline.id(), RECIPE);
        assert_eq!(tracked.id(), TRACKED_RECIPE);
        assert_ne!(baseline.id(), tracked.id());
        assert_eq!(baseline.budget(), tracked.budget());
        assert_eq!(
            baseline.process_commit_bytes(),
            tracked.process_commit_bytes()
        );
    }
    #[test]
    fn tracked_codex_trust_is_scoped_to_its_definition_and_supported_schema() {
        let version = Version::parse("0.167.1").unwrap();
        let helper = Path::new(r"C:\App\shinatama-hook.exe");
        let shell = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        let arguments = session_hook_arguments(&version, helper, shell, 43210).unwrap();
        assert_eq!(arguments.len(), 6);
        assert_eq!(arguments[0], "-c");
        assert_eq!(arguments[1], "features.hooks=true");
        // Single-segment hooks table carrying the trust state; Codex's `-c`
        // parser would tear a quoted dotted hooks.state key apart.
        let trust = arguments[3].to_str().unwrap();
        assert!(trust.starts_with(
            "hooks={state={'C:\\<session-flags>\\config.toml:session_start:0:0'={enabled=true,trusted_hash=\"sha256:"
        ));
        assert!(trust.ends_with("\"}}}"));
        let definition = arguments[5].to_str().unwrap();
        assert!(definition.starts_with("hooks.SessionStart=[{matcher=\"^(startup|resume|clear|compact|fork)$\",hooks=[{type=\"command\",command="));
        assert!(definition.ends_with(",timeout=5,async=false}]}]"));
        assert!(
            !arguments
                .iter()
                .any(|value| value.to_string_lossy().contains("bypass"))
        );
        let other_port = session_hook_arguments(&version, helper, shell, 43211).unwrap();
        assert_ne!(arguments[3], other_port[3]);
        assert_ne!(arguments[5], other_port[5]);
        assert_eq!(
            session_hook_arguments(&Version::parse("0.167.0").unwrap(), helper, shell, 43210),
            Err(CodexLaunchError::QualificationRequired)
        );
        let descriptor = descriptor(&version, Operation::StartNew, TRACKED_RECIPE).unwrap();
        let recipe = CodexSessionOperation::start_new()
            .qualify_arguments_with_hooks(
                &descriptor,
                PathBuf::from(r"C:\App\codex.exe"),
                &arguments,
            )
            .unwrap();
        let command = shinatama_adapter::launch::arguments::plan_arguments(
            &descriptor,
            Operation::StartNew,
            Path::new(r"C:\Project"),
            Some(&recipe),
        )
        .unwrap();
        let baseline = fixed_arguments();
        assert_eq!(&command.argv()[..baseline.len()], baseline);
        assert_eq!(&command.argv()[baseline.len()..], arguments);
        // The terminal host refuses sealed launches above its shared argument
        // bound (SHELL_ARGUMENT_LIMIT = 32). Track the exact tracked-recipe
        // count so any growth toward that bound is a visible, reviewed change.
        assert_eq!(command.argv().len(), 21);
        assert!(
            CodexSessionOperation::resume_native("01a17e07-93f7-7a73-85c7-9f0205836600")
                .unwrap()
                .qualify_arguments_with_hooks(
                    &descriptor,
                    PathBuf::from(r"C:\App\codex.exe"),
                    &arguments
                )
                .is_err()
        );
    }
    #[test]
    fn tracked_codex_bridge_has_no_outer_shell_interpolation() {
        let helper = r"C:/Program Files/O'Neil/$name`$(x)/shinatama-hook.exe";
        assert_eq!(
            session_hook_script(helper, 43210),
            "$ErrorActionPreference='Stop'; & 'C:/Program Files/O''Neil/$name`$(x)/shinatama-hook.exe' codex-session 43210; exit $LASTEXITCODE"
        );
        let shell = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        let command = session_hook_command(Path::new(helper), shell, 43210).unwrap();
        let prefix = "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -NoProfile -NonInteractive -EncodedCommand ";
        assert!(
            command
                .strip_prefix(prefix)
                .unwrap()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
        );
        for invalid in [
            "relative.exe",
            r"\\server\share\shinatama-hook.exe",
            "C:/bad\nname.exe",
        ] {
            assert!(session_hook_command(Path::new(invalid), shell, 43210).is_err());
        }
        assert!(
            session_hook_command(
                Path::new(helper),
                Path::new(r"C:\bad path\powershell.exe"),
                43210
            )
            .is_err()
        );
        assert!(session_hook_command(Path::new(helper), shell, 0).is_err());
        for (input, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode_base64(input.as_bytes()), encoded);
        }
    }
    #[test]
    fn codex_new_recipe_preserves_existing_arguments_and_operation() {
        let new = CodexSessionOperation::start_new();
        let baseline: Vec<OsString> = [
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
        .collect();
        assert_eq!(new.operation(), Operation::StartNew);
        assert_eq!(new.native_id(), None);
        assert_eq!(new.arguments(), baseline);
        let descriptor = descriptor(
            &Version::parse("0.167.1").unwrap(),
            Operation::StartNew,
            RECIPE,
        )
        .unwrap();
        let recipe = new
            .qualify_arguments(&descriptor, PathBuf::from(r"C:\synthetic\codex.exe"))
            .unwrap();
        let command = shinatama_adapter::launch::arguments::plan_arguments(
            &descriptor,
            Operation::StartNew,
            Path::new(r"C:\synthetic\project"),
            Some(&recipe),
        )
        .unwrap();
        assert_eq!(command.operation(), Operation::StartNew);
        assert_eq!(command.argv(), baseline);
        assert!(
            descriptor
                .capabilities
                .require(Operation::ResumeNative)
                .is_err()
        );
    }
    #[test]
    fn codex_resume_selection_is_exact_and_qualifies_only_its_own_operation() {
        let id = "01a17e07-93f7-7a73-85c7-9f0205836600";
        let resume = CodexSessionOperation::resume_native(id).unwrap();
        assert_eq!(resume.operation(), Operation::ResumeNative);
        assert_eq!(resume.native_id(), Some(id));
        let arguments = resume.arguments();
        assert_eq!(&arguments[..2], &[OsString::from("resume"), id.into()]);
        assert_eq!(
            &arguments[2..],
            CodexSessionOperation::start_new().arguments()
        );
        let version = Version::parse("0.167.1").unwrap();
        let startup = descriptor(&version, Operation::StartNew, RECIPE).unwrap();
        // A StartNew qualification never requires or plans resume argv.
        assert!(
            startup
                .capabilities
                .require(Operation::ResumeNative)
                .is_err()
        );
        assert!(matches!(
            resume.qualify_arguments(&startup, PathBuf::from(r"C:\synthetic\codex.exe")),
            Err(CodexLaunchError::Composition)
        ));
        // Its own operation qualifies the exact sealed argv, untracked and
        // tracked (resume 2 + fixed 15 + hook 6 = 23, inside the shared bound).
        let resume_descriptor = descriptor(&version, Operation::ResumeNative, RECIPE).unwrap();
        let recipe = resume
            .qualify_arguments(&resume_descriptor, PathBuf::from(r"C:\synthetic\codex.exe"))
            .unwrap();
        let command = shinatama_adapter::launch::arguments::plan_arguments(
            &resume_descriptor,
            Operation::ResumeNative,
            Path::new(r"C:\synthetic\project"),
            Some(&recipe),
        )
        .unwrap();
        assert_eq!(command.operation(), Operation::ResumeNative);
        assert_eq!(command.argv(), arguments);
        let hook_arguments = session_hook_arguments(
            &version,
            Path::new(r"C:\App\shinatama-hook.exe"),
            Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            43210,
        )
        .unwrap();
        let tracked_descriptor =
            descriptor(&version, Operation::ResumeNative, TRACKED_RECIPE).unwrap();
        let tracked_recipe = resume
            .qualify_arguments_with_hooks(
                &tracked_descriptor,
                PathBuf::from(r"C:\synthetic\codex.exe"),
                &hook_arguments,
            )
            .unwrap();
        let tracked_command = shinatama_adapter::launch::arguments::plan_arguments(
            &tracked_descriptor,
            Operation::ResumeNative,
            Path::new(r"C:\synthetic\project"),
            Some(&tracked_recipe),
        )
        .unwrap();
        assert_eq!(tracked_command.argv().len(), 23);
        assert_eq!(
            &tracked_command.argv()[..2],
            &[OsString::from("resume"), id.into()]
        );
        for invalid in [
            "",
            "latest",
            "--last",
            "session name",
            "../session.jsonl",
            "01a17e07",
            "01a17e0793f77a7385c79f0205836600",
            "01A17E07-93F7-7A73-85C7-9F0205836600",
            "{01a17e07-93f7-7a73-85c7-9f0205836600}",
            "00000000-0000-0000-0000-000000000000",
            "01a17e07-93f7-7a73-85c7-9f0205836600 --last",
        ] {
            assert_eq!(
                CodexSessionOperation::resume_native(invalid),
                Err(CodexLaunchError::InvalidNativeSession)
            );
        }
    }
    #[test]
    fn narrow_profile_is_fixed_and_failure_never_increases_it() {
        let first = profile(false).unwrap();
        let second = profile(false).unwrap();
        assert_eq!(first.budget(), second.budget());
        assert_eq!(first.budget().processes, 8);
        assert_eq!(first.process_commit_bytes(), 768 * MIB);
        assert_eq!(first.budget().memory_bytes, 1024 * MIB);
        assert_eq!(first.budget().cpu_basis_points, 1000);
    }
}
