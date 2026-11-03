//! Sealed native TUI authority. No public issuer, IPC recipe, execution, SQL or
//! qualification lives here. Only a future C15 qualifier may issue this permit.
use crate::launch::{PreparedLaunch, identity::verify_record};
use crate::resources::job::JobBudget;
use crate::resources::{
    admission::{CleanupStatus, PreparedWorkload, WorkloadPhase},
    job::{JobSnapshot, OwnedJobKey},
    sampling::Measurement,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use shinatama_adapter::{AgentId, HomePlacement, Operation, Placement, Version};
use shinatama_state::{
    location::StorageLocation,
    records::{RootAccess, RootKind, RootScope, StoreRootRecord},
    resource_reservations::Budget,
    store_roots::PreparedPrivateStore,
};
use std::sync::{Arc, Mutex};
use std::{ffi::OsStr, fmt, fs::File};
use subtle::ConstantTimeEq;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeLaunchError {
    MissingBinding,
    BindingMismatch,
    InvalidIdentity,
    InvalidProgram,
    InvalidRoot,
    InvalidProfile,
    PinsUnavailable,
    LimitExceeded,
}
impl fmt::Display for NativeLaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingBinding => "Native launch has no qualified binding.",
            Self::BindingMismatch => "Native launch does not match its qualified binding.",
            Self::InvalidIdentity => "Native launch identity is not valid.",
            Self::InvalidProgram => "Native executable binding is not valid.",
            Self::InvalidRoot => "Native store binding is not valid.",
            Self::InvalidProfile => "Native resource profile is not valid.",
            Self::PinsUnavailable => "Native launch filesystem bindings are unavailable.",
            Self::LimitExceeded => "Native launch exceeds its preparation bounds.",
        })
    }
}
impl std::error::Error for NativeLaunchError {}

#[derive(Clone, PartialEq, Eq)]
pub struct NativeProgramIdentity {
    volume_serial: u32,
    file_index: u64,
    bytes: u64,
    last_write_100ns: u64,
    sha256: [u8; 32],
}
impl NativeProgramIdentity {
    /// The qualifier hashes the SAME held, no-write/no-delete-sharing file.
    #[allow(dead_code)] // Deliberately unissuable until the C15 qualifier exists.
    pub(crate) fn new(
        volume_serial: u32,
        file_index: u64,
        bytes: u64,
        last_write_100ns: u64,
        sha256: [u8; 32],
    ) -> Result<Self, NativeLaunchError> {
        if file_index == 0 || bytes == 0 || last_write_100ns == 0 || sha256 == [0; 32] {
            return Err(NativeLaunchError::InvalidProgram);
        }
        Ok(Self {
            volume_serial,
            file_index,
            bytes,
            last_write_100ns,
            sha256,
        })
    }
}
impl fmt::Debug for NativeProgramIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeProgramIdentity([pinned])")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct NativeLaunchBinding {
    agent: AgentId,
    version: Version,
    operation: Operation,
    placement: Placement,
    instance_id: String,
    thread_id: String,
    program: NativeProgramIdentity,
    root: StoreBinding,
    launch_digest: [u8; 32],
}
#[derive(Clone, PartialEq, Eq)]
struct StoreBinding {
    id: String,
    revision: u64,
    ephemeral: bool,
}
impl NativeLaunchBinding {
    #[allow(dead_code)] // Future C15 qualification is the only intended caller.
    pub(crate) fn for_qualified_launch(
        launch: &PreparedLaunch,
        program: NativeProgramIdentity,
        root: &StoreRootRecord,
    ) -> Result<Self, NativeLaunchError> {
        let command = launch.command();
        let placement = launch.placement();
        if command.agent().as_str().starts_with("fake_")
            || command.agent() != placement.agent()
            || command.version() != placement.version()
            || !matches!(
                command.operation(),
                Operation::StartNew | Operation::ResumeNative | Operation::Login
            )
        {
            return Err(NativeLaunchError::BindingMismatch);
        }
        if root.validate().is_err()
            || root.harness != command.agent().as_str()
            || !root.available
            || root.scope != RootScope::Primary
            || root.access != RootAccess::ReadWrite
            || !matches!(
                (placement.placement().home(), root.kind),
                (HomePlacement::Box, RootKind::Box) | (HomePlacement::System, RootKind::System)
            )
        {
            return Err(NativeLaunchError::InvalidRoot);
        }
        let record = launch.identity_record();
        Ok(Self {
            agent: command.agent().clone(),
            version: command.version().clone(),
            operation: command.operation(),
            placement: placement.placement(),
            instance_id: record.instance_id.clone(),
            thread_id: record.thread_id.clone(),
            program,
            root: StoreBinding {
                id: root.id.clone(),
                revision: root.config_revision,
                ephemeral: false,
            },
            launch_digest: launch_digest(launch)?,
        })
    }
    pub(crate) fn for_ephemeral_trial(
        launch: &PreparedLaunch,
        program: NativeProgramIdentity,
    ) -> Result<Self, NativeLaunchError> {
        if launch.command().agent().as_str() != "claude_code"
            || launch.command().operation() != Operation::StartNew
            || launch.placement().placement().home() != HomePlacement::Box
        {
            return Err(NativeLaunchError::BindingMismatch);
        }
        Ok(Self {
            agent: launch.command().agent().clone(),
            version: launch.command().version().clone(),
            operation: Operation::StartNew,
            placement: launch.placement().placement(),
            instance_id: launch.identity_record().instance_id.clone(),
            thread_id: launch.identity_record().thread_id.clone(),
            program,
            root: StoreBinding {
                id: format!("trial_{}", launch.identity_record().instance_id),
                revision: 1,
                ephemeral: true,
            },
            launch_digest: launch_digest(launch)?,
        })
    }
}
impl fmt::Debug for NativeLaunchBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeLaunchBinding")
            .field("agent", &self.agent)
            .field("version", &self.version)
            .field("operation", &self.operation)
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

/// The calibrated profile is chosen by trusted Rust qualification, never UI input.
#[derive(Clone)]
pub struct NativeAdmissionProfile {
    id: String,
    revision: u64,
    budget: Budget,
    process_commit_bytes: u64,
}
impl NativeAdmissionProfile {
    #[allow(dead_code)]
    pub(crate) fn new(
        id: &str,
        revision: u64,
        budget: Budget,
        process_commit_bytes: u64,
    ) -> Result<Self, NativeLaunchError> {
        if id.is_empty()
            || id.len() > 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || revision == 0
            || revision > i64::MAX as u64
            || budget.queue_bytes == 0
            || budget.queue_items == 0
            || budget.queue_bytes > i64::MAX as u64
            || budget.memory_bytes > i64::MAX as u64
        {
            return Err(NativeLaunchError::InvalidProfile);
        }
        JobBudget::new(
            budget.memory_bytes,
            process_commit_bytes,
            budget.processes,
            budget.cpu_basis_points,
        )
        .map_err(|_| NativeLaunchError::InvalidProfile)?;
        Ok(Self {
            id: id.into(),
            revision,
            budget,
            process_commit_bytes,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn budget(&self) -> Budget {
        self.budget
    }
    pub fn process_commit_bytes(&self) -> u64 {
        self.process_commit_bytes
    }
}
impl fmt::Debug for NativeAdmissionProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeAdmissionProfile")
            .field("id", &self.id)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

/// Handles must be opened/verified by the qualifier before issuance. The private
/// home and ancestor guards stay owned; ordinary Drop does no SQL, wait or cleanup.
pub(crate) struct NativeLaunchPins {
    program: File,
    root: File,
    _ancestor_pins: Vec<File>,
    _private_home: Option<PreparedPrivateStore>,
    locations: Vec<StorageLocation>,
    _artifacts: Vec<File>,
}
impl NativeLaunchPins {
    #[allow(dead_code)]
    pub(crate) fn new(
        program: File,
        root: File,
        ancestor_pins: Vec<File>,
        private_home: Option<PreparedPrivateStore>,
        locations: Vec<StorageLocation>,
        artifacts: Vec<File>,
    ) -> Result<Self, NativeLaunchError> {
        if locations.len() > 32
            || ancestor_pins.len() > 128
            || ancestor_pins.is_empty()
            || artifacts.len() > 8
        {
            return Err(NativeLaunchError::LimitExceeded);
        }
        if !program
            .metadata()
            .map_err(|_| NativeLaunchError::PinsUnavailable)?
            .is_file()
            || !root
                .metadata()
                .map_err(|_| NativeLaunchError::PinsUnavailable)?
                .is_dir()
        {
            return Err(NativeLaunchError::PinsUnavailable);
        }
        Ok(Self {
            program,
            root,
            _ancestor_pins: ancestor_pins,
            _private_home: private_home,
            locations,
            _artifacts: artifacts,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeLaunchSummary {
    pub agent: AgentId,
    pub version: Version,
    pub operation: Operation,
    pub instance_id: String,
    pub thread_id: String,
    pub store_root_id: String,
    pub store_config_revision: u64,
    pub profile_id: String,
    pub profile_revision: u64,
    pub purpose: NativeLaunchPurpose,
    pub tui_startup_observed: bool,
    pub provider_traffic: NativeCoverage,
    pub effective_sidecar_configuration: NativeCoverage,
    pub startup_scope: Option<NativeStartupScope>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeStartupScope {
    SafeModeEmptySet,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeCoverage {
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeLaunchPurpose {
    QualificationTrial,
    Interactive,
}
/// Owned, single launch only. No Clone, Deserialize or public ready flag.
pub struct NativeTuiPermit {
    binding: NativeLaunchBinding,
    profile: NativeAdmissionProfile,
    pins: NativeLaunchPins,
    trial: Option<Arc<Mutex<NativeTrialState>>>,
    startup_observed: bool,
}
impl NativeTuiPermit {
    /// C15 must already have qualified this exact adapter/build/operation, sidecar,
    /// sets, program, root revision and resource profile. This is not a qualifier.
    #[allow(dead_code)]
    pub(crate) fn issue(
        launch: &PreparedLaunch,
        binding: NativeLaunchBinding,
        profile: NativeAdmissionProfile,
        pins: NativeLaunchPins,
    ) -> Result<Self, NativeLaunchError> {
        if binding.root.ephemeral {
            return Err(NativeLaunchError::InvalidRoot);
        }
        Self::issue_inner(launch, binding, profile, pins)
    }
    fn issue_inner(
        launch: &PreparedLaunch,
        binding: NativeLaunchBinding,
        profile: NativeAdmissionProfile,
        pins: NativeLaunchPins,
    ) -> Result<Self, NativeLaunchError> {
        if binding.placement.home() == HomePlacement::Box && pins._private_home.is_none() {
            return Err(NativeLaunchError::InvalidRoot);
        }
        let permit = Self {
            binding,
            profile,
            pins,
            trial: None,
            startup_observed: false,
        };
        permit.validate(launch)?;
        permit.verify_pins()?;
        Ok(permit)
    }
    pub fn validate(&self, launch: &PreparedLaunch) -> Result<(), NativeLaunchError> {
        let actual = launch
            .native_binding()
            .ok_or(NativeLaunchError::MissingBinding)?;
        if actual != &self.binding
            || launch.command().agent() != &self.binding.agent
            || launch.command().version() != &self.binding.version
            || launch.command().operation() != self.binding.operation
            || launch.placement().placement() != self.binding.placement
            || launch.identity_record().instance_id != self.binding.instance_id
            || launch.identity_record().thread_id != self.binding.thread_id
            || !bool::from(launch_digest(launch)?.ct_eq(&self.binding.launch_digest))
        {
            return Err(NativeLaunchError::BindingMismatch);
        }
        Ok(())
    }
    /// Bounded filesystem/native reads: call only on the admitted startup worker.
    pub fn verify_pins(&self) -> Result<(), NativeLaunchError> {
        verify_directory_pin(&self.pins.root)?;
        for ancestor in &self.pins._ancestor_pins {
            verify_directory_pin(ancestor)?;
        }
        verify_program_pin(&self.pins.program, &self.binding.program)?;
        for location in &self.pins.locations {
            location
                .verify_private_directory()
                .map_err(|_| NativeLaunchError::PinsUnavailable)?;
        }
        Ok(())
    }
    pub fn profile(&self) -> &NativeAdmissionProfile {
        &self.profile
    }
    pub(crate) fn issue_after_trial(
        launch: &PreparedLaunch,
        binding: NativeLaunchBinding,
        profile: NativeAdmissionProfile,
        pins: NativeLaunchPins,
        receipt: &NativeTrialReceipt,
    ) -> Result<Self, NativeLaunchError> {
        if !receipt.matches(&binding.program, &binding.version, &profile) {
            return Err(NativeLaunchError::BindingMismatch);
        }
        let mut permit = Self::issue(launch, binding, profile, pins)?;
        permit.startup_observed = true;
        Ok(permit)
    }
    pub fn is_trial(&self) -> bool {
        self.trial.is_some() && self.binding.root.ephemeral
    }
    pub fn summary(&self) -> NativeLaunchSummary {
        NativeLaunchSummary {
            agent: self.binding.agent.clone(),
            version: self.binding.version.clone(),
            operation: self.binding.operation,
            instance_id: self.binding.instance_id.clone(),
            thread_id: self.binding.thread_id.clone(),
            store_root_id: self.binding.root.id.clone(),
            store_config_revision: self.binding.root.revision,
            profile_id: self.profile.id.clone(),
            profile_revision: self.profile.revision,
            purpose: if self.trial.is_some() {
                NativeLaunchPurpose::QualificationTrial
            } else {
                NativeLaunchPurpose::Interactive
            },
            tui_startup_observed: self.startup_observed,
            provider_traffic: NativeCoverage::Unknown,
            effective_sidecar_configuration: NativeCoverage::Unknown,
            startup_scope: (self.profile.id == "claude-system-box-safe-empty-tui-v1")
                .then_some(NativeStartupScope::SafeModeEmptySet),
        }
    }
}

/// Read-only observer handle. Only the owned Host's real output/workload hooks
/// can complete it; no IPC/Deserialize or caller-ready boolean exists.
pub struct NativeTrialTracker(Arc<Mutex<NativeTrialState>>);
pub struct NativeTrialReceipt {
    binding: NativeLaunchBinding,
    profile: NativeAdmissionProfile,
    output_bytes: u64,
    cleanup: CleanupStatus,
}
impl NativeTrialReceipt {
    /// Produced only after the owned workload's real native/queue/SQL release.
    pub fn cleanup_status(&self) -> CleanupStatus {
        self.cleanup
    }
    pub fn summary(&self) -> NativeLaunchSummary {
        NativeLaunchSummary {
            agent: self.binding.agent.clone(),
            version: self.binding.version.clone(),
            operation: self.binding.operation,
            instance_id: self.binding.instance_id.clone(),
            thread_id: self.binding.thread_id.clone(),
            store_root_id: self.binding.root.id.clone(),
            store_config_revision: self.binding.root.revision,
            profile_id: self.profile.id.clone(),
            profile_revision: self.profile.revision,
            purpose: NativeLaunchPurpose::QualificationTrial,
            tui_startup_observed: true,
            provider_traffic: NativeCoverage::Unknown,
            effective_sidecar_configuration: NativeCoverage::Unknown,
            startup_scope: Some(NativeStartupScope::SafeModeEmptySet),
        }
    }
    pub(crate) fn matches(
        &self,
        program: &NativeProgramIdentity,
        version: &Version,
        profile: &NativeAdmissionProfile,
    ) -> bool {
        self.binding.agent.as_str() == "claude_code"
            && self.binding.version == *version
            && self.binding.program == *program
            && self.binding.operation == Operation::StartNew
            && self.profile.id == profile.id
            && self.profile.revision == profile.revision
            && self.profile.budget == profile.budget
            && self.profile.process_commit_bytes == profile.process_commit_bytes
            && self.output_bytes > 0
    }
}
impl fmt::Debug for NativeTrialReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTrialReceipt")
            .field("summary", &self.summary())
            .finish()
    }
}
struct NativeTrialState {
    binding: NativeLaunchBinding,
    profile: NativeAdmissionProfile,
    owned: Option<OwnedJobKey>,
    tail: Vec<u8>,
    output_bytes: u64,
    tui_seen: bool,
    sampled_after_tui: bool,
    rejected: bool,
    finished: bool,
    receipt: Option<NativeTrialReceipt>,
}
impl NativeTrialTracker {
    pub fn take_receipt(&self) -> Option<NativeTrialReceipt> {
        self.0.lock().ok()?.receipt.take()
    }
    pub fn ready_for_cleanup(&self) -> bool {
        self.0.lock().is_ok_and(|state| {
            !state.rejected
                && !state.finished
                && state.tui_seen
                && state.sampled_after_tui
                && state.owned.is_some()
        })
    }
}
impl NativeTuiPermit {
    pub(crate) fn issue_trial(
        launch: &PreparedLaunch,
        binding: NativeLaunchBinding,
        profile: NativeAdmissionProfile,
        pins: NativeLaunchPins,
    ) -> Result<(Self, NativeTrialTracker), NativeLaunchError> {
        if !binding.root.ephemeral {
            return Err(NativeLaunchError::InvalidRoot);
        }
        let mut permit = Self::issue_inner(launch, binding.clone(), profile.clone(), pins)?;
        let state = Arc::new(Mutex::new(NativeTrialState {
            binding,
            profile,
            owned: None,
            tail: Vec::new(),
            output_bytes: 0,
            tui_seen: false,
            sampled_after_tui: false,
            rejected: false,
            finished: false,
            receipt: None,
        }));
        permit.trial = Some(Arc::clone(&state));
        Ok((permit, NativeTrialTracker(state)))
    }
    /// Owned Host only: use the fresh snapshot produced by THIS opaque workload.
    /// No HTTP/UI DTO is accepted as a source of trial observations.
    pub fn observe_trial_workload(&self, workload: &PreparedWorkload, snapshot: &JobSnapshot) {
        let Some(trial) = &self.trial else {
            return;
        };
        let Ok(mut state) = trial.lock() else {
            return;
        };
        if state.finished {
            return;
        }
        let owned = workload.owned_key();
        let exact = workload.phase() == WorkloadPhase::Active
            && workload.ticket().budget == state.profile.budget
            && state.owned.is_none_or(|previous| previous == owned);
        if !exact {
            state.rejected = true;
            return;
        }
        state.owned = Some(owned);
        if !snapshot.membership_complete {
            return;
        }
        let process_exceeded = snapshot
            .processes
            .iter()
            .any(|process| match process.counters {
                Measurement::Available(value) => {
                    value.private_commit_bytes > state.profile.process_commit_bytes
                }
                Measurement::Unavailable(_) => false,
            });
        if snapshot.active_processes > state.profile.budget.processes
            || snapshot.peak_job_commit_bytes > state.profile.budget.memory_bytes
            || process_exceeded
        {
            state.rejected = true;
        }
        state.sampled_after_tui |= state.tui_seen && snapshot.active_processes >= 3;
    }
    /// Called only on actual ConPTY output, never a frontend-supplied transcript.
    /// Bounded ephemeral bytes are wiped on completion; diagnostics never see them.
    pub fn observe_trial_output(&self, bytes: &[u8]) {
        let Some(trial) = &self.trial else {
            return;
        };
        let Ok(mut state) = trial.lock() else {
            return;
        };
        if state.finished {
            return;
        }
        state.output_bytes = state.output_bytes.saturating_add(bytes.len() as u64);
        if state.output_bytes > 512 * 1024 {
            state.rejected = true;
            return;
        }
        let keep = bytes.len().min(16 * 1024);
        state.tail.extend_from_slice(&bytes[bytes.len() - keep..]);
        if state.tail.len() > 16 * 1024 {
            let excess = state.tail.len() - 16 * 1024;
            state.tail.drain(..excess);
        }
        let lower: Vec<u8> = state.tail.iter().map(u8::to_ascii_lowercase).collect();
        let contains = |needle: &[u8]| lower.windows(needle.len()).any(|part| part == needle);
        state.tui_seen |= contains(b"claude")
            && [
                b"welcome".as_slice(),
                b"theme",
                b"trust",
                b"sign in",
                b"log in",
            ]
            .iter()
            .any(|needle| contains(needle));
    }
    /// Called after bounded cleanup. Released is an actual native+queue+SQL state,
    /// not a user assertion; Retired never produces a qualification receipt.
    pub fn finish_trial(&self, workload: &PreparedWorkload) {
        let Some(trial) = &self.trial else {
            return;
        };
        let Ok(mut state) = trial.lock() else {
            return;
        };
        if state.finished {
            return;
        }
        let valid = !state.rejected
            && state.tui_seen
            && state.sampled_after_tui
            && state.owned == Some(workload.owned_key())
            && workload.phase() == WorkloadPhase::Released;
        if valid {
            state.receipt = Some(NativeTrialReceipt {
                binding: state.binding.clone(),
                profile: state.profile.clone(),
                output_bytes: state.output_bytes,
                cleanup: workload
                    .released_status()
                    .expect("released trial has its ledger revision"),
            });
        }
        state.tail.fill(0);
        state.tail.clear();
        state.finished = true;
    }
}
impl fmt::Debug for NativeTuiPermit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTuiPermit")
            .field("summary", &self.summary())
            .finish_non_exhaustive()
    }
}

fn launch_digest(launch: &PreparedLaunch) -> Result<[u8; 32], NativeLaunchError> {
    let record = launch.identity_record();
    let mut environment_count = 0;
    let mut total = 0usize;
    let mut token = None;
    let mut instance = None;
    let mut thread = None;
    for (name, value) in launch.environment().entries() {
        environment_count += 1;
        total = total.saturating_add(name.len()).saturating_add(value.len());
        if environment_count > 2048 || total > 1024 * 1024 {
            return Err(NativeLaunchError::LimitExceeded);
        }
        for (reserved, slot) in [
            ("SHINATAMA_INSTANCE_TOKEN", &mut token),
            ("SHINATAMA_INSTANCE", &mut instance),
            ("SHINATAMA_THREAD", &mut thread),
        ] {
            if name.eq_ignore_ascii_case(reserved) {
                if slot.replace(value).is_some() {
                    return Err(NativeLaunchError::InvalidIdentity);
                }
            }
        }
    }
    let token = token
        .and_then(OsStr::to_str)
        .ok_or(NativeLaunchError::InvalidIdentity)?;
    if instance != Some(OsStr::new(&record.instance_id))
        || thread != Some(OsStr::new(&record.thread_id))
    {
        return Err(NativeLaunchError::InvalidIdentity);
    }
    verify_record(&record.instance_id, token, Some(record))
        .map_err(|_| NativeLaunchError::InvalidIdentity)?;
    let contains_token = |value: &OsStr| {
        value
            .as_encoded_bytes()
            .windows(token.len())
            .any(|part| part == token.as_bytes())
    };
    if launch.environment().entries().any(|(name, value)| {
        contains_token(name)
            || (!name.eq_ignore_ascii_case("SHINATAMA_INSTANCE_TOKEN") && contains_token(value))
    }) || contains_token(launch.command().executable().as_os_str())
        || contains_token(launch.command().working_directory().as_os_str())
        || launch
            .command()
            .argv()
            .iter()
            .any(|argument| contains_token(argument))
    {
        return Err(NativeLaunchError::InvalidIdentity);
    }
    let mut hash = Sha256::new();
    hash.update(b"shinatama.native-launch.v1\0");
    hash.update((launch.command().argv().len() as u64).to_le_bytes());
    hash.update((environment_count as u64).to_le_bytes());
    hash.update(record.started_at_utc_ms.to_le_bytes());
    hash.update(record.token_sha256);
    let mut add = |value: &OsStr| {
        let bytes = value.as_encoded_bytes();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    add(launch.command().executable().as_os_str());
    add(launch.command().working_directory().as_os_str());
    if launch.command().argv().len() > 128 {
        return Err(NativeLaunchError::LimitExceeded);
    }
    for argument in launch.command().argv() {
        add(argument);
    }
    add(OsStr::new(&record.instance_id));
    add(OsStr::new(&record.thread_id));
    for (name, value) in launch.environment().entries() {
        add(name);
        add(value);
    }
    Ok(hash.finalize().into())
}

#[cfg(windows)]
fn verify_program_pin(
    file: &File,
    identity: &NativeProgramIdentity,
) -> Result<(), NativeLaunchError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0
        || info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || info.nNumberOfLinks != 1
        || info.dwVolumeSerialNumber != identity.volume_serial
        || ((u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
            != identity.file_index
        || ((u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow)) != identity.bytes
        || ((u64::from(info.ftLastWriteTime.dwHighDateTime) << 32)
            | u64::from(info.ftLastWriteTime.dwLowDateTime))
            != identity.last_write_100ns
    {
        return Err(NativeLaunchError::PinsUnavailable);
    }
    Ok(())
}
#[cfg(not(windows))]
fn verify_program_pin(_: &File, _: &NativeProgramIdentity) -> Result<(), NativeLaunchError> {
    Err(NativeLaunchError::PinsUnavailable)
}
#[cfg(windows)]
fn verify_directory_pin(file: &File) -> Result<(), NativeLaunchError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0
        || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(NativeLaunchError::PinsUnavailable);
    }
    Ok(())
}
#[cfg(not(windows))]
fn verify_directory_pin(_: &File) -> Result<(), NativeLaunchError> {
    Err(NativeLaunchError::PinsUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> Budget {
        Budget {
            memory_bytes: 512 * 1024 * 1024,
            processes: 8,
            cpu_basis_points: 1000,
            queue_bytes: 4 * 1024 * 1024,
            queue_items: 256,
        }
    }
    #[test]
    fn trusted_profile_rejects_invalid_native_limits_instead_of_raising_them() {
        let original = budget();
        let profile =
            NativeAdmissionProfile::new("qualified-fixture", 3, original, 256 * 1024 * 1024)
                .unwrap();
        assert_eq!(profile.budget(), original);
        for request in [
            Budget {
                processes: 0,
                ..original
            },
            Budget {
                cpu_basis_points: 10001,
                ..original
            },
            Budget {
                queue_bytes: 0,
                ..original
            },
        ] {
            assert!(
                NativeAdmissionProfile::new("qualified-fixture", 3, request, 256 * 1024 * 1024)
                    .is_err()
            );
        }
        assert!(NativeAdmissionProfile::new("qualified-fixture", 0, original, 1).is_err());
        assert!(
            NativeAdmissionProfile::new(
                "qualified-fixture",
                3,
                original,
                original.memory_bytes + 1
            )
            .is_err()
        );
        assert!(NativeProgramIdentity::new(1, 2, 3, 4, [0; 32]).is_err());
    }
    #[test]
    fn opaque_binding_debug_contains_neither_fingerprints_nor_program_metadata() {
        let identity = NativeProgramIdentity::new(12345, 67890, 45678, 98765, [0xac; 32]).unwrap();
        let binding = NativeLaunchBinding {
            agent: AgentId::new("codex").unwrap(),
            version: Version::parse("1.0.0+exact").unwrap(),
            operation: Operation::StartNew,
            placement: Placement::new(
                shinatama_adapter::ProgramPlacement::System,
                HomePlacement::Box,
            )
            .unwrap(),
            instance_id: "i_abcdefghijkl".into(),
            thread_id: "t_abcdefghijkl".into(),
            program: identity.clone(),
            root: StoreBinding {
                id: "sr_abcdefghijkl".into(),
                revision: 17,
                ephemeral: false,
            },
            launch_digest: [0xbd; 32],
        };
        let debug = format!("{identity:?} {binding:?}");
        for private in [
            "12345",
            "67890",
            "45678",
            "98765",
            "172",
            "189",
            "launch_digest",
            "sha256",
        ] {
            assert!(!debug.contains(private));
        }
        assert!(debug.contains("codex"));
        assert!(debug.contains("1.0.0+exact"));
    }
}
