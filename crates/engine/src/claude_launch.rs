//! Narrow Windows System+Box Claude startup. Preparation performs bounded local
//! reads/writes; only the owned ConPTY canary can qualify the exact build/profile.
//! No provider request, login, user credential read, shell or marketplace install.
#![cfg(windows)]
use crate::{
    marionette::{self, Adapter, Host, Marionette, is_marionette_set},
    claude_mods::{self, ClaudeMods},
    claude_probe::{ClaudeProbeOutcome, ProbeCleanup},
    claude_session_events::ClaudeSessionEventReceiver,
    launch::{LaunchInputs, PreparedLaunch, compose_launch, identity::LaunchIdentity},
    launch_policy::LaunchPolicy,
    native_launch::{
        NativeAdmissionProfile, NativeLaunchBinding, NativeLaunchError, NativeLaunchPins,
        NativeTrialReceipt, NativeTrialTracker, NativeTuiPermit,
    },
    sidecar_supervisor::SidecarSupervisor,
    terminal_palette::HarnessPalette,
};
use serde_json::Value;
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
    ffi::{OsStr, OsString},
    fs::{File, OpenOptions},
    io::{Read, Write},
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
const RECIPE: &str = "claude-system-box-safe-empty-tui-v1";
const TRACKED_RECIPE: &str = "claude-system-box-tracked-tui-v1";
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
    BundledPlugins,
    MukadeUnavailable,
    Configuration,
    Composition,
    Identity,
    InvalidNativeSession,
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

/// Exact initial native conversation selection from trusted Rust. No picker,
/// name, path, latest-session fallback or caller-supplied argv is accepted.
/// This does not prove conversation presence, resume success or a later TUI
/// session switch; SHINATAMA_INSTANCE remains the stable process identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeSessionOperation {
    operation: Operation,
    native_id: String,
    initial_prompt: Option<String>,
    require_mukade: bool,
    /// The thread's Mukade room, when its sessions talk to each other (ADR-0057).
    chat: Option<crate::mukade_chat::ChatHooks>,
}
impl ClaudeSessionOperation {
    /// The caller generates and durably records this UUID before releasing the
    /// child. This function never invents a replacement ID on failure.
    pub fn start_new(uuid: &str) -> Result<Self, ClaudeLaunchError> {
        Self::new(Operation::StartNew, uuid)
    }
    /// The caller resolves this exact ID from its authoritative segment, checks
    /// the native source/cwd and acquires exclusive activation before release.
    pub fn resume_native(native_id: &str) -> Result<Self, ClaudeLaunchError> {
        Self::new(Operation::ResumeNative, native_id)
    }
    fn new(operation: Operation, value: &str) -> Result<Self, ClaudeLaunchError> {
        if value.len() != 36 {
            return Err(ClaudeLaunchError::InvalidNativeSession);
        }
        let uuid =
            uuid::Uuid::parse_str(value).map_err(|_| ClaudeLaunchError::InvalidNativeSession)?;
        // Preserve exact identity: reject alternate spellings rather than
        // normalizing a stored ID into a potentially different source lookup.
        if uuid.hyphenated().to_string() != value
            || uuid.get_variant() != uuid::Variant::RFC4122
            || uuid.get_version().is_none()
        {
            return Err(ClaudeLaunchError::InvalidNativeSession);
        }
        Ok(Self {
            operation,
            native_id: value.into(),
            initial_prompt: None,
            require_mukade: false,
            chat: None,
        })
    }
    /// A bounded first user message for a new conversation, never shell syntax
    /// or native options. Resume keeps its original conversation unchanged.
    pub fn with_initial_prompt(mut self, prompt: &str) -> Result<Self, ClaudeLaunchError> {
        if self.operation != Operation::StartNew
            || prompt.trim().is_empty()
            || prompt.len() > 8192
            || prompt.contains('\0')
        {
            return Err(ClaudeLaunchError::Configuration);
        }
        self.initial_prompt = Some(prompt.to_owned());
        Ok(self)
    }
    /// Fail this launch if the requested room cannot receive a Mukade client.
    /// The qualifier checks again after desktop preflight, before sealing.
    pub fn with_mukade_required(mut self) -> Self {
        self.require_mukade = true;
        self
    }
    /// Make the session a member of its thread's room through Mukade's hooks. Without
    /// a reachable service at launch the session opens as an ordinary one.
    pub fn with_chat_hooks(mut self, hooks: crate::mukade_chat::ChatHooks) -> Self {
        self.chat = Some(hooks);
        self
    }
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn native_id(&self) -> &str {
        &self.native_id
    }
    fn arguments(&self) -> Vec<OsString> {
        let mut arguments = vec![
            match self.operation {
                Operation::StartNew => "--session-id",
                Operation::ResumeNative => "--resume",
                _ => unreachable!("Claude session constructors only accept new or resume"),
            }
            .into(),
            self.native_id.clone().into(),
        ];
        if let Some(prompt) = &self.initial_prompt {
            arguments.extend([OsString::from("--"), prompt.into()]);
        }
        arguments
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
    /// Synced Marionette store; required when the set is the Marionette one.
    pub plugins_root: Option<&'a Path>,
    /// `resources/claude-mods` beside the executable, when present.
    pub mods_root: Option<&'a Path>,
    /// `x-shinatama-control: <kind>;<hash>` for the one control message this launch will send as its
    /// first prompt; set only by a Shinatama hand-off. Added to the model requests through
    /// `ANTHROPIC_CUSTOM_HEADERS`, so the proxy can tell that message from every other.
    pub control_header: Option<&'a str>,
    pub thread_id: &'a str,
}
pub struct PreparedClaudeTrial {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    pub tracker: NativeTrialTracker,
    /// Keep alive while the trial runs. Presence configures hooks; it is not
    /// evidence that Claude emitted a session event.
    pub session_events: Option<ClaudeSessionEventReceiver>,
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
    plugins_root: Option<&'a Path>,
    mods_root: Option<&'a Path>,
    control_header: Option<&'a str>,
    thread_id: &'a str,
}
pub struct PreparedClaudeStart {
    pub launch: PreparedLaunch,
    pub permit: NativeTuiPermit,
    /// Owned by the session controller through cleanup, never exposed to IPC.
    pub session_events: Option<ClaudeSessionEventReceiver>,
}

/// The tracked profile carries Marionette, whose skills run helpers and need headroom
/// beyond the bare TUI. The number is fixed by recipe, measured once (Claude with the
/// earlier two plugins and one prompt reached 10 active processes) and never raised at
/// runtime because of a failure.
const TRACKED_PROCESSES: u32 = 16;
const SAFE_PROCESSES: u32 = 8;

/// This is the ROOT-approved finite trial profile, not inferred from a growing
/// process and never raised on failure. Issuance is sealed inside this qualifier.
fn profile(tracked: bool) -> Result<NativeAdmissionProfile, ClaudeLaunchError> {
    Ok(NativeAdmissionProfile::new(
        if tracked { TRACKED_RECIPE } else { RECIPE },
        1,
        Budget {
            memory_bytes: 1024 * MIB,
            processes: if tracked { TRACKED_PROCESSES } else { SAFE_PROCESSES },
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
    prepare_trial_with_hooks(context, None)
}
/// A separate exact-build trial for normal tools and session command hooks.
/// No safe-mode receipt qualifies this profile, or proves an event was emitted.
pub fn prepare_tracked_trial(
    context: ClaudeTrialContext<'_>,
    trusted_hook_exe: &Path,
) -> Result<PreparedClaudeTrial, ClaudeLaunchError> {
    prepare_trial_with_hooks(context, Some(trusted_hook_exe))
}
fn prepare_trial_with_hooks(
    context: ClaudeTrialContext<'_>,
    trusted_hook_exe: Option<&Path>,
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
            plugins_root: None,
            mods_root: None,
            control_header: None,
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
    Ok(PreparedClaudeTrial {
        launch: prepared.launch,
        permit,
        tracker,
        session_events: prepared.session_events,
    })
}
pub fn prepare_start(
    context: ClaudeStartContext<'_>,
    receipt: &NativeTrialReceipt,
) -> Result<PreparedClaudeStart, ClaudeLaunchError> {
    prepare_for_operation(context, receipt, None, None)
}

/// Compose an exact new/resume invocation with the existing qualified startup
/// protections. The receipt covers TUI startup, not native transcript fidelity;
/// this function does not acquire or release a durable thread activation.
pub fn prepare_session(
    context: ClaudeStartContext<'_>,
    receipt: &NativeTrialReceipt,
    session: &ClaudeSessionOperation,
) -> Result<PreparedClaudeStart, ClaudeLaunchError> {
    prepare_for_operation(context, receipt, Some(session), None)
}

/// Trusted helper path and exact native operation only. The returned receiver
/// must be retained and observed by the session owner; this does not activate SQL
/// ownership or qualify hook delivery, permissions, tool execution or resume.
pub fn prepare_tracked_session(
    context: ClaudeStartContext<'_>,
    receipt: &NativeTrialReceipt,
    session: &ClaudeSessionOperation,
    trusted_hook_exe: &Path,
) -> Result<PreparedClaudeStart, ClaudeLaunchError> {
    prepare_for_operation(context, receipt, Some(session), Some(trusted_hook_exe))
}

fn prepare_for_operation(
    context: ClaudeStartContext<'_>,
    receipt: &NativeTrialReceipt,
    session: Option<&ClaudeSessionOperation>,
    trusted_hook_exe: Option<&Path>,
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
            plugins_root: context.plugins_root,
            mods_root: context.mods_root,
            control_header: context.control_header,
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
    Ok(PreparedClaudeStart {
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
    session_events: Option<ClaudeSessionEventReceiver>,
}

fn prepare(
    context: Context<'_>,
    receipt: Option<&NativeTrialReceipt>,
    trial: bool,
    session: Option<&ClaudeSessionOperation>,
    trusted_hook_exe: Option<&Path>,
) -> Result<Prepared, ClaudeLaunchError> {
    let deadline = Instant::now() + PREPARE_BUDGET;
    let tracked = trusted_hook_exe.is_some();
    let recipe = if tracked { TRACKED_RECIPE } else { RECIPE };
    let operation = session.map_or(Operation::StartNew, ClaudeSessionOperation::operation);
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
    // Only the exact Marionette set, only for a tracked real session (the safe-mode
    // trial profile stays empty). Digests are checked at every launch.
    let bundled = if context.selected_set.is_empty() {
        None
    } else if is_marionette_set(context.selected_set) && tracked && !trial {
        let root = context
            .plugins_root
            .ok_or(ClaudeLaunchError::BundledPlugins)?;
        let verified = Marionette::current(root)
            .and_then(|release| release.verify(Host::ClaudeCode))
            .map_err(|_| ClaudeLaunchError::BundledPlugins)?;
        // Claude writes into the directories it loads: never hand it the synced tree.
        Some(
            verified
                .private_copy(&claude_plugin_copy(context.base.directory()))
                .map_err(|_| ClaudeLaunchError::BundledPlugins)?,
        )
    } else {
        return Err(ClaudeLaunchError::UnsupportedSet);
    };
    // Display-only Claude mods ride on Marionette: never without it, dropped
    // alone on any doubt, never a reason to refuse the launch.
    let mods = if bundled.is_some() {
        context.mods_root.and_then(|mods_root| {
            ClaudeMods::for_claude_launch(
                mods_root,
                &version.to_string(),
                &context.base.directory().join(claude_mods::DIRECTORY),
            )
        })
    } else {
        None
    };
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
    let resource_profile = profile(tracked)?;
    if let Some(receipt) = receipt {
        if !receipt.matches(
            &context.candidate.agent,
            &pinned.identity,
            &version,
            &resource_profile,
        ) {
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
    let mut settings = if trial {
        serde_json::json!({})
    } else {
        read_settings(context.private_home.paths().home().join("settings.json"))?
    };
    // Qualification trials stay on the fixed recipe; only a real session honours
    // the user's launch policy.
    let policy = if trial {
        LaunchPolicy::default()
    } else {
        crate::launch_policy::current()
    };
    if policy.full_access() {
        apply_full_access(&mut settings)?;
    }
    if !trial {
        apply_terminal_palette(&mut settings, crate::terminal_palette::current())?;
    }
    let session_events = if let Some(helper_path) = trusted_hook_exe {
        let mut helper = pin_session_hook(helper_path, deadline)?;
        let receiver = ClaudeSessionEventReceiver::start(identity.record(), &project)
            .map_err(|_| ClaudeLaunchError::Configuration)?;
        let command = session_hook_command(&helper.path, receiver.port())?;
        merge_session_hooks(&mut settings, &command)?;
        // The helper's content and chosen path stay pinned for this invocation.
        // No bearer token is put in argv or settings; it is inherited from C11.
        pinned.ancestors.append(&mut helper.ancestors);
        artifacts.push(helper.file);
        Some(receiver)
    } else {
        None
    };
    let mcp_endpoint = session_events
        .as_ref()
        .filter(|_| !trial && crate::session_control::is_installed())
        .map(ClaudeSessionEventReceiver::endpoint);
    // Recheck a required bus here: desktop preflight can become stale before
    // this qualifier runs. Ordinary sessions still tolerate an optional outage.
    let mukade = resolve_optional_mukade(
        if mcp_endpoint.is_some() {
            crate::mukade_mcp::Connection::discover(context.base)
        } else {
            Ok(None)
        },
        session.is_some_and(|session| session.require_mukade),
    )?;
    // ADR-0057: the thread's room, through Mukade's own hooks, beside the tracking
    // hook above. Only with that hook (hooks enabled) and a reachable service.
    // Hooks that cannot be written leave the session an ordinary one, never a failed launch.
    let chat = session
        .and_then(|session| session.chat.as_ref())
        .filter(|chat| mukade.is_some() && session_events.is_some() && chat.claude_hooks().is_some());
    if let Some(chat) = chat {
        merge_chat_hooks(&mut settings, chat)?;
        crate::mukade_chat::note_launch(context.thread_id, chat.room_id());
    }
    let settings_path = create_json(
        &instance_directory,
        "settings.json",
        &settings,
        &mut artifacts,
    )?;
    let mut mcp = mcp_configuration(mcp_endpoint.as_deref());
    if let Some(connection) = &mukade {
        mcp["mcpServers"]["mukade"] = connection.claude_server();
    }
    let mcp_path = create_json(&instance_directory, "mcp.json", &mcp, &mut artifacts)?;
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
    crate::mukade_mcp::clear_environment(&mut environment);
    if let Some(connection) = &mukade {
        connection.apply_environment(&mut environment);
    }
    if let Some(chat) = chat {
        chat.apply_environment(&mut environment);
    }
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
        set(
            &mut environment,
            name,
            if mcp_endpoint.is_some() {
                "127.0.0.1,localhost".into()
            } else {
                OsString::new()
            },
        );
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
    configure_child_switches(&mut environment);
    configure_safe_mode(&mut environment, tracked);
    if let Some(line) = context.control_header {
        let existing = environment
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(CUSTOM_HEADERS))
            .map(|(_, value)| value.clone());
        let merged = merge_control_header(existing.as_deref(), line)?;
        set(&mut environment, CUSTOM_HEADERS, merged);
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
    policy.apply_environment(&mut environment);
    let descriptor = descriptor(&version, operation, recipe)?;
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
        evidence(&descriptor, recipe)?,
    )
    .map_err(|_| ClaudeLaunchError::Composition)?;
    let mut args = vec![
        "--setting-sources".into(),
        "project,local".into(),
        "--settings".into(),
        settings_path.into_os_string(),
        "--mcp-config".into(),
        mcp_path.into_os_string(),
        "--strict-mcp-config".into(),
    ];
    if let Some(plugins) = &bundled {
        args.extend(plugin_arguments(plugins));
    }
    if let Some(mods) = &mods {
        args.extend(mods.arguments());
    }
    if let Some(session) = session {
        args.extend(session.arguments());
    }
    let arguments_recipe = QualifiedArgvRecipe::qualify(
        &descriptor,
        operation,
        CommandForm::NativeExe(pinned.path),
        args,
        evidence(&descriptor, recipe)?,
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
    let mut launch = compose_launch(&descriptor, operation, mode, &inputs)
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
        session_events,
    })
}
/// Claude's private copy of Marionette's adapter under the Shinatama home, a
/// folder of its own beside the synced store (`marionette`): the copy removes
/// every file it does not hold, so inside the store it would wipe the release.
fn claude_plugin_copy(home: &Path) -> PathBuf {
    home.join(marionette::CLAUDE_COPY)
}
/// Optional discovery failures do not block ordinary conversations. A room
/// requirement must survive the interval between preflight and qualification.
fn resolve_optional_mukade<T, E>(
    discovered: Result<Option<T>, E>,
    required: bool,
) -> Result<Option<T>, ClaudeLaunchError> {
    match discovered {
        Ok(Some(connection)) => Ok(Some(connection)),
        _ if required => Err(ClaudeLaunchError::MukadeUnavailable),
        _ => Ok(None),
    }
}
/// Only a receiver-owned loopback endpoint reaches this function. The bearer
/// stays in the existing launch environment; this file contains a reference.
fn mcp_configuration(endpoint: Option<&str>) -> Value {
    match endpoint {
        Some(endpoint) => serde_json::json!({"mcpServers":{"shinatama":{
            "type":"http", "url":format!("{endpoint}/mcp"),
            "headers":{"Authorization":"Bearer ${SHINATAMA_INSTANCE_TOKEN}"}
        }}}),
        None => serde_json::json!({"mcpServers":{}}),
    }
}
/// `--plugin-dir <ordinary drive path>` of Marionette's Claude Code adapter.
fn plugin_arguments(adapter: &Adapter) -> Vec<OsString> {
    vec!["--plugin-dir".into(), adapter.dir_arg().into()]
}
fn verify_sidecar(sidecar: &SidecarSupervisor) -> Result<(), ClaudeLaunchError> {
    let status = sidecar
        .status()
        .ok_or(ClaudeLaunchError::SidecarUnavailable)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ClaudeLaunchError::SidecarUnavailable)?
        .as_millis() as i64;
    if !status.backs_launch(now) {
        return Err(ClaudeLaunchError::SidecarUnavailable);
    }
    Ok(())
}
fn descriptor(
    version: &Version,
    operation: Operation,
    recipe: &str,
) -> Result<AdapterDescriptor, ClaudeLaunchError> {
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
            .map_err(|_| ClaudeLaunchError::Composition)?;
    }
    Ok(descriptor)
}
fn evidence(
    descriptor: &AdapterDescriptor,
    recipe: &str,
) -> Result<FixtureEvidence, ClaudeLaunchError> {
    FixtureEvidence::new(
        descriptor.agent.clone(),
        descriptor.version.clone(),
        recipe,
        EvidenceKind::InstalledRuntimeFixture,
    )
    .map_err(|_| ClaudeLaunchError::Composition)
}
fn set(environment: &mut BTreeMap<OsString, OsString>, name: &str, value: OsString) {
    environment.retain(|key, _| !key.eq_ignore_ascii_case(name));
    environment.insert(name.into(), value);
}
/// Full access: Claude starts in bypass-permissions mode and never shows its
/// one-time confirmation. This lives in the per-instance settings file, so the
/// qualified argv stays what it was proved as.
fn apply_full_access(settings: &mut Value) -> Result<(), ClaudeLaunchError> {
    let object = settings
        .as_object_mut()
        .ok_or(ClaudeLaunchError::Configuration)?;
    let permissions = object
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or(ClaudeLaunchError::Configuration)?;
    permissions.insert("defaultMode".into(), "bypassPermissions".into());
    object.insert("skipDangerousModePermissionPrompt".into(), true.into());
    Ok(())
}
/// Terminals that follow the skin: Claude draws with the terminal's 16 colours
/// ("ANSI colors only" in `/theme`). Merged settings beat the global config's
/// `theme`, and these flag settings beat the user, project and local ones, so
/// a stored theme is set aside for this launch only and its file is untouched.
fn apply_terminal_palette(
    settings: &mut Value,
    palette: HarnessPalette,
) -> Result<(), ClaudeLaunchError> {
    let Some(theme) = palette.claude_theme() else {
        return Ok(());
    };
    settings
        .as_object_mut()
        .ok_or(ClaudeLaunchError::Configuration)?
        .insert("theme".into(), theme.into());
    Ok(())
}
/// Child-only switches: updates, telemetry, error reporting and marketplace
/// auto-install stay off. `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` is
/// removed, even when the parent process has it: Claude Code makes Remote
/// Control unavailable while it is present, and the switches above already
/// cover what this launcher needs quiet.
fn configure_child_switches(environment: &mut BTreeMap<OsString, OsString>) {
    environment.retain(|name, _| {
        !name.eq_ignore_ascii_case("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
    });
    for name in [
        "DISABLE_AUTOUPDATER",
        "DISABLE_UPDATES",
        "DISABLE_TELEMETRY",
        "DISABLE_ERROR_REPORTING",
        "CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL",
        "CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES",
    ] {
        set(environment, name, "1".into());
    }
}

/// Header name of a Shinatama control message (see `shinatama_hop::channel::CONTROL_HEADER`).
const CONTROL_HEADER_NAME: &str = "x-shinatama-control";
const CUSTOM_HEADERS: &str = "ANTHROPIC_CUSTOM_HEADERS";

/// The value of `ANTHROPIC_CUSTOM_HEADERS` (one `Name: value` per line) with the control header
/// added. A value the user already has is kept, except an earlier control header, which is
/// replaced. `line` must be exactly `x-shinatama-control: <kind>;<64 lowercase hex>`.
fn merge_control_header(existing: Option<&OsStr>, line: &str) -> Result<OsString, ClaudeLaunchError> {
    let value = line
        .strip_prefix(CONTROL_HEADER_NAME)
        .and_then(|rest| rest.strip_prefix(": "))
        .ok_or(ClaudeLaunchError::Composition)?;
    let well_formed = value.split_once(';').is_some_and(|(kind, hash)| {
        matches!(kind, "seed" | "bootstrap" | "receipt")
            && hash.len() == 64
            && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    });
    if !well_formed {
        return Err(ClaudeLaunchError::Composition);
    }
    let mut merged = OsString::new();
    if let Some(existing) = existing {
        let existing = existing.to_str().ok_or(ClaudeLaunchError::Composition)?;
        for kept in existing.split(['\r', '\n']).filter(|kept| {
            !kept.trim().is_empty()
                && !kept
                    .split_once(':')
                    .is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case(CONTROL_HEADER_NAME))
        }) {
            merged.push(kept);
            merged.push("\n");
        }
    }
    merged.push(line);
    Ok(merged)
}

fn configure_safe_mode(environment: &mut BTreeMap<OsString, OsString>, tracked: bool) {
    if tracked {
        environment.retain(|name, _| !name.eq_ignore_ascii_case("CLAUDE_CODE_SAFE_MODE"));
    } else {
        set(environment, "CLAUDE_CODE_SAFE_MODE", "1".into());
    }
}
/// Claude's Windows command hooks run in Git Bash. A quoted ordinary drive path
/// avoids extended Windows prefixes and shell expansion, including apostrophes.
fn session_hook_command(path: &Path, port: u16) -> Result<String, ClaudeLaunchError> {
    let path = path.to_str().ok_or(ClaudeLaunchError::Configuration)?;
    let ordinary = path.strip_prefix(r"\\?\").unwrap_or(path);
    let bytes = ordinary.as_bytes();
    if port == 0
        || ordinary.len() > 4096
        || ordinary.chars().any(char::is_control)
        || bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'\\' | b'/')
    {
        return Err(ClaudeLaunchError::Configuration);
    }
    let escaped = ordinary.replace('\\', "/").replace('\'', "'\"'\"'");
    Ok(format!("'{escaped}' claude-session {port}"))
}
fn merge_session_hooks(settings: &mut Value, command: &str) -> Result<(), ClaudeLaunchError> {
    let object = settings
        .as_object_mut()
        .ok_or(ClaudeLaunchError::Configuration)?;
    // The tracked profile needs its hooks enabled. Existing hook events and
    // entries remain intact; only this invocation receives the two additions.
    object.insert("disableAllHooks".into(), Value::Bool(false));
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or(ClaudeLaunchError::Configuration)?;
    for event in ["SessionStart", "SessionEnd"] {
        let entries = hooks
            .entry(event)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or(ClaudeLaunchError::Configuration)?;
        entries.push(
            serde_json::json!({"hooks": [{"type": "command", "command": command, "timeout": 5}]}),
        );
    }
    Ok(())
}
/// Mukade's three hooks, appended to whatever the settings already hold.
fn merge_chat_hooks(settings: &mut Value, chat: &crate::mukade_chat::ChatHooks) -> Result<(), ClaudeLaunchError> {
    let entries = chat.claude_hooks().ok_or(ClaudeLaunchError::Configuration)?;
    let hooks = settings
        .as_object_mut()
        .ok_or(ClaudeLaunchError::Configuration)?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or(ClaudeLaunchError::Configuration)?;
    for (event, entry) in entries {
        hooks
            .entry(event)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or(ClaudeLaunchError::Configuration)?
            .push(entry);
    }
    Ok(())
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
            // Participate in sharing checks without enumerating any contents.
            .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
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
type ProgramPin = crate::harness_hooks::PinnedExecutable;
fn pin_program(path: &Path, deadline: Instant) -> Result<ProgramPin, ClaudeLaunchError> {
    use crate::harness_hooks::ExecutablePinError;
    crate::harness_hooks::pin_executable(path, "claude.exe", deadline, false).map_err(|error| {
        match error {
            ExecutablePinError::InvalidCandidate => ClaudeLaunchError::InvalidCandidate,
            ExecutablePinError::PrivateHome => ClaudeLaunchError::PrivateHome,
            ExecutablePinError::UnsafeProgram => ClaudeLaunchError::UnsafeProgram,
            ExecutablePinError::ProgramChanged => ClaudeLaunchError::ProgramChanged,
            ExecutablePinError::Expired => ClaudeLaunchError::Expired,
            ExecutablePinError::Identity => ClaudeLaunchError::Permit,
        }
    })
}
fn pin_session_hook(path: &Path, deadline: Instant) -> Result<ProgramPin, ClaudeLaunchError> {
    crate::harness_hooks::pin_session_hook(path, deadline)
        .map_err(|_| ClaudeLaunchError::Configuration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn required_mukade_refuses_absence_and_discovery_failure_after_preflight() {
        for missing in [Ok(None), Err("service disappeared after preflight")] {
            assert_eq!(resolve_optional_mukade::<u8, _>(missing, true),
                Err(ClaudeLaunchError::MukadeUnavailable));
            assert_eq!(resolve_optional_mukade::<u8, _>(missing, false), Ok(None));
        }
        for required in [false, true] {
            assert_eq!(resolve_optional_mukade::<u8, ()>(Ok(Some(7)), required), Ok(Some(7)));
        }
        let id = "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3";
        let new = ClaudeSessionOperation::start_new(id).unwrap();
        let resume = ClaudeSessionOperation::resume_native(id).unwrap();
        assert!(!new.require_mukade && !resume.require_mukade);
        let required = new.clone().with_mukade_required();
        assert!(required.require_mukade);
        assert_eq!(required.arguments(), new.arguments());
    }
    #[test]
    fn initial_prompt_is_one_literal_argument_after_session_selection() {
        let id = "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3";
        let prompt =
            "--dangerously-skip-permissions\nReview C:\\Project; $(echo nope) `x` \"quoted\"";
        let session = ClaudeSessionOperation::start_new(id)
            .unwrap()
            .with_initial_prompt(prompt)
            .unwrap();
        assert_eq!(
            session.arguments(),
            vec![
                OsString::from("--session-id"),
                id.into(),
                "--".into(),
                prompt.into(),
            ]
        );
        assert_eq!(
            ClaudeSessionOperation::start_new(id).unwrap().arguments(),
            [OsString::from("--session-id"), id.into()]
        );
        assert_eq!(
            ClaudeSessionOperation::resume_native(id)
                .unwrap()
                .arguments(),
            [OsString::from("--resume"), id.into()]
        );
    }
    #[test]
    fn initial_prompt_is_bounded_and_cannot_be_attached_to_resume() {
        let id = "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3";
        for invalid in [
            "".to_owned(),
            " \r\n\t".into(),
            "hello\0world".into(),
            "x".repeat(8193),
            "é".repeat(4097),
        ] {
            assert_eq!(
                ClaudeSessionOperation::start_new(id)
                    .unwrap()
                    .with_initial_prompt(&invalid),
                Err(ClaudeLaunchError::Configuration)
            );
        }
        assert!(
            ClaudeSessionOperation::start_new(id)
                .unwrap()
                .with_initial_prompt(&"x".repeat(8192))
                .is_ok()
        );
        assert!(
            ClaudeSessionOperation::start_new(id)
                .unwrap()
                .with_initial_prompt(&"é".repeat(4096))
                .is_ok()
        );
        assert_eq!(
            ClaudeSessionOperation::resume_native(id)
                .unwrap()
                .with_initial_prompt("review"),
            Err(ClaudeLaunchError::Configuration)
        );
    }
    #[test]
    fn session_mcp_configuration_uses_only_an_environment_token_reference() {
        assert_eq!(
            mcp_configuration(None),
            serde_json::json!({"mcpServers":{}})
        );
        assert_eq!(
            mcp_configuration(Some("http://127.0.0.1:32123/session-opaque")),
            serde_json::json!({"mcpServers":{"shinatama":{
                "type":"http", "url":"http://127.0.0.1:32123/session-opaque/mcp",
                "headers":{"Authorization":"Bearer ${SHINATAMA_INSTANCE_TOKEN}"}
            }}})
        );
    }
    #[test]
    fn marionette_becomes_one_plugin_dir_pair() {
        use crate::marionette::tests::{FakeFetch, scratch};
        let store = scratch("claude-args");
        marionette::sync(&store, &FakeFetch::new("marionette-v0.1.0", "0.1.0")).unwrap();
        let adapter = Marionette::current(&store).unwrap().verify(Host::ClaudeCode).unwrap();
        let args = plugin_arguments(&adapter);
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--plugin-dir");
        assert!(args[1].to_string_lossy().ends_with("claude-code"));
        assert!(!args[1].to_string_lossy().starts_with(r"\?\"));
        let _ = std::fs::remove_dir_all(&store);
    }
    #[test]
    fn claude_private_copy_leaves_the_synced_store_current() {
        // The desktop syncs into `<home>/marionette`; every launch then copies
        // Claude's adapter under the same home. The copy must not touch the store.
        use crate::marionette::tests::{FakeFetch, scratch};
        let home = scratch("claude-copy-home");
        let store = home.join(marionette::STORE);
        marionette::sync(&store, &FakeFetch::new("marionette-v0.1.0", "0.1.0")).unwrap();
        let adapter = Marionette::current(&store).unwrap().verify(Host::ClaudeCode).unwrap();
        for _launch in 0..2 {
            let copy = adapter.private_copy(&claude_plugin_copy(&home)).unwrap();
            assert!(copy.dir().join(".claude-plugin/plugin.json").is_file());
            let current = Marionette::current(&store).unwrap();
            for host in Host::ALL {
                current.verify(host).unwrap();
            }
        }
        let _ = std::fs::remove_dir_all(&home);
    }
    #[test]
    fn tracked_hook_hardlink_keeps_bytes_and_selected_path_pinned() {
        let directory =
            std::env::temp_dir().join(format!("shn-claude-hook-pin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let hook = directory.join("shinatama-hook.exe");
        let alias = directory.join("claude.exe");
        // A PE-shaped byte fixture only: this test never executes a program.
        let mut bytes = [0u8; 96];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        std::fs::write(&hook, bytes).unwrap();
        std::fs::hard_link(&hook, &alias).unwrap();
        let held = pin_session_hook(&hook, Instant::now() + PREPARE_BUDGET).unwrap();
        assert_eq!(held.path, hook.canonicalize().unwrap());
        assert_eq!(held.bytes, bytes.len() as u64);
        assert_eq!(file_info(&held.file).unwrap().nNumberOfLinks, 2);
        assert_eq!(
            held.hash,
            Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        assert!(OpenOptions::new().write(true).open(&alias).is_err());
        assert!(OpenOptions::new().write(true).open(&hook).is_err());
        assert!(std::fs::remove_file(&hook).is_err());
        assert!(std::fs::rename(&directory, directory.with_extension("moved")).is_err());
        assert!(matches!(
            pin_file(&hook),
            Err(ClaudeLaunchError::UnsafeProgram)
        ));
        assert!(matches!(
            pin_program(&alias, Instant::now() + PREPARE_BUDGET),
            Err(ClaudeLaunchError::UnsafeProgram)
        ));
        drop(held);
        assert!(OpenOptions::new().write(true).open(&alias).is_ok());
        std::fs::remove_file(&alias).unwrap();
        std::fs::remove_file(&hook).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
    #[test]
    fn tracked_claude_hooks_preserve_existing_entries_and_reject_malformed_settings() {
        let mut settings = serde_json::json!({
            "disableAllHooks": true,
            "permissions": {"defaultMode": "default"},
            "hooks": {
                "SessionStart": [{"matcher":"resume", "hooks":[{"type":"command", "command":"existing-start"}]}],
                "SessionEnd": [{"hooks":[{"type":"command", "command":"existing-end"}]}],
                "PreToolUse": [{"hooks":[{"type":"command", "command":"existing-tool"}]}]
            }
        });
        let original = settings.clone();
        merge_session_hooks(
            &mut settings,
            "'C:/Shinatama/shinatama-hook.exe' claude-session 43210",
        )
        .unwrap();
        assert_eq!(settings["disableAllHooks"], false);
        assert_eq!(settings["permissions"], original["permissions"]);
        assert_eq!(
            settings["hooks"]["PreToolUse"],
            original["hooks"]["PreToolUse"]
        );
        for event in ["SessionStart", "SessionEnd"] {
            assert_eq!(settings["hooks"][event][0], original["hooks"][event][0]);
            assert_eq!(settings["hooks"][event].as_array().unwrap().len(), 2);
            assert_eq!(settings["hooks"][event][1]["hooks"][0]["type"], "command");
            assert_eq!(settings["hooks"][event][1]["hooks"][0]["timeout"], 5);
        }
        assert!(
            !serde_json::to_string(&settings)
                .unwrap()
                .contains("SHINATAMA_INSTANCE_TOKEN")
        );
        for mut malformed in [
            serde_json::json!([]),
            serde_json::json!({"hooks":[]}),
            serde_json::json!({"hooks":{"SessionStart":{}}}),
        ] {
            assert_eq!(
                merge_session_hooks(&mut malformed, "fixed"),
                Err(ClaudeLaunchError::Configuration)
            );
        }
    }
    #[test]
    fn tracked_claude_hook_command_quotes_shell_characters_and_rejects_controls() {
        assert_eq!(
            session_hook_command(
                Path::new(r"\\?\C:\Program Files\O'Neil\$name`$(x)\shinatama-hook.exe"),
                43210
            )
            .unwrap(),
            "'C:/Program Files/O'\"'\"'Neil/$name`$(x)/shinatama-hook.exe' claude-session 43210"
        );
        for path in [
            "C:\\bad\nname\\shinatama-hook.exe",
            "C:\\bad\0name\\shinatama-hook.exe",
            r"\\server\share\shinatama-hook.exe",
            "relative/shinatama-hook.exe",
        ] {
            assert_eq!(
                session_hook_command(Path::new(path), 43210),
                Err(ClaudeLaunchError::Configuration)
            );
        }
        assert!(session_hook_command(Path::new(r"C:\Shinatama\shinatama-hook.exe"), 0).is_err());
    }
    #[test]
    fn terminal_palette_replaces_only_the_theme() {
        let mut settings = serde_json::json!({"theme": "dark", "model": "opus"});
        apply_terminal_palette(&mut settings, HarnessPalette::Own).unwrap();
        assert_eq!(
            settings["theme"], "dark",
            "own palette keeps the stored theme"
        );
        apply_terminal_palette(&mut settings, HarnessPalette::Terminal { light: false }).unwrap();
        assert_eq!(settings["theme"], "dark-ansi");
        assert_eq!(settings["model"], "opus");
        let mut settings = serde_json::json!({"theme": "dark-ansi"});
        apply_terminal_palette(&mut settings, HarnessPalette::Terminal { light: true }).unwrap();
        assert_eq!(
            settings["theme"], "light-ansi",
            "the background decides the variant"
        );
        let mut empty = serde_json::json!({});
        apply_terminal_palette(&mut empty, HarnessPalette::Terminal { light: false }).unwrap();
        assert_eq!(empty["theme"], "dark-ansi");
        assert!(matches!(
            apply_terminal_palette(
                &mut serde_json::json!([]),
                HarnessPalette::Terminal { light: false }
            ),
            Err(ClaudeLaunchError::Configuration)
        ));
        assert!(apply_terminal_palette(&mut serde_json::json!([]), HarnessPalette::Own).is_ok());
    }
    #[test]
    fn full_access_sets_bypass_mode_and_keeps_the_other_permissions() {
        let mut settings = serde_json::json!({
            "permissions": {"defaultMode": "default", "deny": ["Bash(rm -rf /)"]},
            "hooks": {}
        });
        apply_full_access(&mut settings).unwrap();
        assert_eq!(settings["permissions"]["defaultMode"], "bypassPermissions");
        assert_eq!(settings["permissions"]["deny"][0], "Bash(rm -rf /)");
        assert_eq!(settings["skipDangerousModePermissionPrompt"], true);
        assert!(settings["hooks"].is_object());

        let mut empty = serde_json::json!({});
        apply_full_access(&mut empty).unwrap();
        assert_eq!(empty["permissions"]["defaultMode"], "bypassPermissions");

        for bad in [serde_json::json!([]), serde_json::json!({"permissions": 3})] {
            let mut bad = bad;
            assert!(matches!(
                apply_full_access(&mut bad),
                Err(ClaudeLaunchError::Configuration)
            ));
        }
    }
    #[test]
    fn claude_child_switches_keep_remote_control_available() {
        let mut environment = BTreeMap::from([
            (
                OsString::from("claude_code_disable_nonessential_traffic"),
                OsString::from("1"),
            ),
            (OsString::from("PATH"), OsString::from("keep")),
        ]);
        configure_child_switches(&mut environment);
        assert!(!environment.keys().any(|name| {
            name.eq_ignore_ascii_case("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
        }));
        assert_eq!(environment.get(&OsString::from("PATH")).unwrap(), "keep");
        for name in [
            "DISABLE_AUTOUPDATER",
            "DISABLE_UPDATES",
            "DISABLE_TELEMETRY",
            "DISABLE_ERROR_REPORTING",
            "CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL",
            "CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES",
        ] {
            assert_eq!(environment.get(&OsString::from(name)).unwrap(), "1", "{name}");
        }
    }
    #[test]
    fn tracked_claude_profile_changes_only_safe_mode_and_keeps_fixed_limits() {
        let baseline = profile(false).unwrap();
        let tracked = profile(true).unwrap();
        assert_eq!(baseline.id(), RECIPE);
        assert_eq!(tracked.id(), TRACKED_RECIPE);
        assert_ne!(baseline.id(), tracked.id());
        let (narrow, wide) = (baseline.budget(), tracked.budget());
        assert_eq!(wide.processes, 16, "only the process ceiling differs: the plugin hooks need it");
        assert_eq!(
            (narrow.memory_bytes, narrow.cpu_basis_points, narrow.queue_bytes, narrow.queue_items),
            (wide.memory_bytes, wide.cpu_basis_points, wide.queue_bytes, wide.queue_items)
        );
        assert_eq!(
            baseline.process_commit_bytes(),
            tracked.process_commit_bytes()
        );
        let mut environment = BTreeMap::from([
            (OsString::from("claude_code_safe_mode"), OsString::from("1")),
            (OsString::from("DISABLE_AUTOUPDATER"), OsString::from("1")),
            (
                OsString::from("CLAUDE_CODE_TMPDIR"),
                OsString::from("private"),
            ),
        ]);
        configure_safe_mode(&mut environment, true);
        assert!(
            !environment
                .keys()
                .any(|name| name.eq_ignore_ascii_case("CLAUDE_CODE_SAFE_MODE"))
        );
        assert_eq!(
            environment
                .get(&OsString::from("DISABLE_AUTOUPDATER"))
                .unwrap(),
            "1"
        );
        assert_eq!(
            environment
                .get(&OsString::from("CLAUDE_CODE_TMPDIR"))
                .unwrap(),
            "private"
        );
        configure_safe_mode(&mut environment, false);
        assert_eq!(
            environment
                .get(&OsString::from("CLAUDE_CODE_SAFE_MODE"))
                .unwrap(),
            "1"
        );
    }
    #[test]
    fn control_header_joins_the_users_custom_headers() {
        let hash = "ab".repeat(32);
        let line = format!("x-shinatama-control: bootstrap;{hash}");
        assert_eq!(merge_control_header(None, &line).unwrap(), OsString::from(&line));
        let user = OsString::from("X-Team: a\r\nX-Shinatama-Control: seed;old\n\nX-Other: b");
        assert_eq!(
            merge_control_header(Some(&user), &line).unwrap(),
            OsString::from(format!("X-Team: a\nX-Other: b\n{line}"))
        );
        for bad in [
            "x-other: bootstrap;ab".to_owned(),
            format!("x-shinatama-control: unknown;{hash}"),
            format!("x-shinatama-control: bootstrap;{}", "AB".repeat(32)),
            format!("x-shinatama-control: bootstrap;{}", &hash[1..]),
            format!("x-shinatama-control: bootstrap;{hash}\nX-Injected: 1"),
        ] {
            assert!(merge_control_header(None, &bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn explicit_session_operations_preserve_exact_uuid_and_reject_ambiguous_targets() {
        let native_id = "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3";
        let new = ClaudeSessionOperation::start_new(native_id).unwrap();
        let resume = ClaudeSessionOperation::resume_native(native_id).unwrap();
        assert_eq!(new.operation(), Operation::StartNew);
        assert_eq!(resume.operation(), Operation::ResumeNative);
        assert_eq!(new.native_id(), native_id);
        assert_eq!(resume.native_id(), native_id);
        assert_eq!(
            new.arguments(),
            [OsString::from("--session-id"), native_id.into()]
        );
        assert_eq!(
            resume.arguments(),
            [OsString::from("--resume"), native_id.into()]
        );
        for invalid in [
            "",
            "latest",
            "--continue",
            "session name",
            "../session.jsonl",
            "12cc3a1145db4f1e89c840d5c6be1dd3",
            "12CC3A11-45DB-4F1E-89C8-40D5C6BE1DD3",
            "{12cc3a11-45db-4f1e-89c8-40d5c6be1dd3}",
            "00000000-0000-0000-0000-000000000000",
            "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3 --fork-session",
        ] {
            assert_eq!(
                ClaudeSessionOperation::start_new(invalid),
                Err(ClaudeLaunchError::InvalidNativeSession)
            );
            assert_eq!(
                ClaudeSessionOperation::resume_native(invalid),
                Err(ClaudeLaunchError::InvalidNativeSession)
            );
        }
    }

    #[test]
    fn narrow_profile_is_fixed_and_failure_never_increases_it() {
        let first = profile(false).unwrap();
        let second = profile(false).unwrap();
        assert_eq!(first.budget(), second.budget());
        assert_eq!(first.budget().processes, 8);
        let tracked = profile(true).unwrap();
        assert_eq!(tracked.budget().processes, 16, "tracked sessions carry the plugin hooks");
        assert_eq!(tracked.budget(), profile(true).unwrap().budget());
        assert_eq!(first.process_commit_bytes(), 768 * MIB);
        assert_eq!(first.budget().memory_bytes, 1024 * MIB);
        assert_eq!(first.budget().cpu_basis_points, 1000);
    }
}
