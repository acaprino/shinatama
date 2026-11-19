//! Deterministic recommendations, not native execution or an admission permit.
//! The coordinator must still reserve aggregate capacity and enforce native limits.
//! No qualified defaults, I/O, termination, suspension or replay live here.
use super::job::JobBudget;
use super::sampling::{Aggregate, Measurement, ProcessKey};
use shinatama_adapter::{AgentId, Operation, Placement, Version};
use std::collections::VecDeque;

const MAX_PROFILES: usize = 32;
const MAX_JOBS: usize = 64;
const MAX_SAMPLES: usize = 120;
const MAX_WINDOW_MS: u64 = 60_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactProfile {
    pub agent: AgentId,
    pub version: Version,
    pub placement: Placement,
    pub operation: Operation,
    pub synthetic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct JobIdentity {
    pub job_key: [u8; 16],
    pub owner: ProcessKey,
    pub windows_session_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueBudget {
    pub bytes: u64,
    pub items: u32,
}

#[derive(Clone, Debug)]
pub struct ProfileLimits {
    pub profile: ExactProfile,
    pub minimum: JobBudget,
    pub ceiling: JobBudget,
    pub queues: QueueBudget,
}

/// All thresholds are explicitly supplied, frozen choices awaiting qualification.
/// Byte floors preserve OS/host headroom; CPU is basis points of ONE machine.
#[derive(Clone, Copy, Debug)]
pub struct PolicyConfig {
    pub physical_available_floor_bytes: u64,
    pub commit_available_floor_bytes: u64,
    pub pressure_enter_basis_points: u16,
    pub pressure_exit_basis_points: u16,
    pub emergency_basis_points: u16,
    pub max_sample_age_ms: u64,
    pub calibration_window_ms: u64,
    pub calibration_samples: usize,
    pub minimum_healthy_samples: usize,
    pub stable_ms: u64,
    pub reduce_before_defer_ms: u64,
    pub defer_before_terminate_ms: u64,
    pub cooldown_ms: u64,
    pub learned_headroom_basis_points: u16,
    pub maximum_upward_step_basis_points: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuCoverage {
    CompleteMachine,
    PartialProcessorGroup,
    Missing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub private_commit_bytes: u64,
    pub largest_process_commit_bytes: u64,
    pub cpu_basis_points: u16,
    pub processes: u32,
    pub queue_bytes: u64,
    pub queue_items: u32,
}
impl Usage {
    fn peak(self, other: Self) -> Self {
        Self {
            private_commit_bytes: self.private_commit_bytes.max(other.private_commit_bytes),
            largest_process_commit_bytes: self
                .largest_process_commit_bytes
                .max(other.largest_process_commit_bytes),
            cpu_basis_points: self.cpu_basis_points.max(other.cpu_basis_points),
            processes: self.processes.max(other.processes),
            queue_bytes: self.queue_bytes.max(other.queue_bytes),
            queue_items: self.queue_items.max(other.queue_items),
        }
    }
}
pub struct JobObservation<'a> {
    pub identity: JobIdentity,
    pub profile: &'a ExactProfile,
    pub at_ms: u64,
    pub applied_budget: JobBudget,
    pub usage: Measurement<Usage>,
    pub membership_complete: bool,
    pub growth_delta_bytes: Option<i128>,
    pub healthy: bool,
    pub incident: bool,
}
pub struct Frame<'a> {
    pub system: &'a Aggregate,
    pub cpu_coverage: CpuCoverage,
    pub host_responsive: Option<bool>,
    pub jobs: &'a [JobObservation<'a>],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    InvalidConfig,
    InvalidFrame,
    UnknownProfile,
    NonIncreasingTime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Normal,
    ReduceOptional,
    DeferStarts,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Healthy,
    Pressure,
    BudgetExceeded,
    RecoveryWait,
    MissingTelemetry,
    StaleTelemetry,
    PartialCpuCoverage,
    AttributedViolation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Violation {
    JobCommit,
    ProcessCommit,
    Cpu,
    ProcessCount,
    QueueBytes,
    QueueItems,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminationRecommendation {
    pub identity: JobIdentity,
    pub violation: Violation,
    /// The actual installed limit remains visible; it is never silently raised.
    pub applied_budget: JobBudget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    pub stage: Stage,
    pub reason: Reason,
    pub reduce_optional: bool,
    pub defer_new_starts: bool,
    pub terminate: Option<TerminationRecommendation>,
}

struct ProfileState {
    limits: ProfileLimits,
    budget: JobBudget,
    samples: VecDeque<(u64, Usage)>,
    last_update_ms: Option<u64>,
}
struct JobState {
    identity: JobIdentity,
    profile: usize,
    last_observed_ms: u64,
    last_commit: u64,
    growth_anchor: Option<u64>,
    episode_peak: Usage,
    learning_tainted: bool,
    violation_since_ms: Option<u64>,
    violation_kind: Option<Violation>,
    termination_issued: bool,
}
pub struct ResourcePolicy {
    config: PolicyConfig,
    profiles: Vec<ProfileState>,
    jobs: Vec<JobState>,
    last_now: Option<u64>,
    stage: Stage,
    stage_since_ms: u64,
    recovery_since_ms: Option<u64>,
    last_pressure_ms: Option<u64>,
    last_termination_ms: Option<u64>,
    // Unknown telemetry is not itself a memory/CPU pressure episode. A genuine
    // entry latches only its own dimension until stable Normal recovery.
    memory_pressure_latched: bool,
    cpu_pressure_latched: bool,
}

impl ResourcePolicy {
    pub fn new(config: PolicyConfig, limits: Vec<ProfileLimits>) -> Result<Self, PolicyError> {
        if config.physical_available_floor_bytes == 0
            || config.commit_available_floor_bytes == 0
            || config.pressure_exit_basis_points >= config.pressure_enter_basis_points
            || config.pressure_enter_basis_points >= config.emergency_basis_points
            || config.emergency_basis_points > 10_000
            || config.max_sample_age_ms == 0
            || !(1..=MAX_WINDOW_MS).contains(&config.calibration_window_ms)
            || !(2..=MAX_SAMPLES).contains(&config.calibration_samples)
            || !(2..=config.calibration_samples).contains(&config.minimum_healthy_samples)
            || config.stable_ms == 0
            || config.stable_ms > config.calibration_window_ms
            || config.reduce_before_defer_ms == 0
            || config.defer_before_terminate_ms == 0
            || config.cooldown_ms == 0
            || config.learned_headroom_basis_points < 10_000
            || !(1..=10_000).contains(&config.maximum_upward_step_basis_points)
            || limits.len() > MAX_PROFILES
        {
            return Err(PolicyError::InvalidConfig);
        }
        for limits in &limits {
            if !fits(limits.minimum, limits.ceiling)
                || limits.queues.bytes == 0
                || limits.queues.items == 0
                || limits.profile.synthetic != limits.profile.agent.as_str().starts_with("fake_")
            {
                return Err(PolicyError::InvalidConfig);
            }
        }
        // Do not merge profile versions/builds, home/program modes or fixture kind.
        for index in 0..limits.len() {
            if limits[..index]
                .iter()
                .any(|old| old.profile == limits[index].profile)
            {
                return Err(PolicyError::InvalidConfig);
            }
        }
        Ok(Self {
            config,
            profiles: limits
                .into_iter()
                .map(|limits| ProfileState {
                    budget: limits.minimum,
                    limits,
                    samples: VecDeque::new(),
                    last_update_ms: None,
                })
                .collect(),
            jobs: Vec::new(),
            last_now: None,
            stage: Stage::Normal,
            stage_since_ms: 0,
            recovery_since_ms: None,
            last_pressure_ms: None,
            last_termination_ms: None,
            memory_pressure_latched: false,
            cpu_pressure_latched: false,
        })
    }
    /// A recommendation for a FUTURE launch, never a permission or live OS update.
    pub fn launch_budget(&self, profile: &ExactProfile) -> Option<JobBudget> {
        self.profiles
            .iter()
            .find(|state| &state.limits.profile == profile)
            .map(|state| state.budget)
    }

    /// An error is a failed evaluation: the coordinator must defer new starts and
    /// keep existing native limits. It must not turn an error into a healthy frame.
    pub fn evaluate(&mut self, now_ms: u64, frame: &Frame<'_>) -> Result<Decision, PolicyError> {
        if self.last_now.is_some_and(|last| now_ms <= last) {
            return Err(PolicyError::NonIncreasingTime);
        }
        if frame.jobs.len() > MAX_JOBS {
            return Err(PolicyError::InvalidFrame);
        }
        for (index, job) in frame.jobs.iter().enumerate() {
            let profile = self
                .profiles
                .iter()
                .position(|state| &state.limits.profile == job.profile)
                .ok_or(PolicyError::UnknownProfile)?;
            if job.identity.job_key == [0; 16]
                || job.identity.owner.pid == 0
                || job.identity.owner.created_at_100ns == 0
                || !fits(job.applied_budget, self.profiles[profile].limits.ceiling)
                || !fits(self.profiles[profile].limits.minimum, job.applied_budget)
                || frame.jobs[..index]
                    .iter()
                    .any(|old| old.identity.job_key == job.identity.job_key)
                || self.jobs.iter().any(|old| {
                    old.identity.job_key == job.identity.job_key
                        && (old.identity != job.identity || old.profile != profile)
                })
            {
                return Err(PolicyError::InvalidFrame);
            }
            if let Measurement::Available(usage) = job.usage
                && (usage.cpu_basis_points > 10_000
                    || usage.largest_process_commit_bytes > usage.private_commit_bytes)
            {
                return Err(PolicyError::InvalidFrame);
            }
        }
        self.last_now = Some(now_ms);
        let backwards = frame.jobs.iter().any(|observation| {
            self.jobs.iter().any(|old| {
                old.identity == observation.identity && observation.at_ms < old.last_observed_ms
            })
        });
        let unavailable = if backwards {
            Some(Reason::StaleTelemetry)
        } else {
            frame_problem(now_ms, frame, self.config.max_sample_age_ms)
        };
        if let Some(reason) = unavailable {
            return Ok(self.defer_unknown(now_ms, reason));
        }
        let Measurement::Available(system) = frame.system.system else {
            unreachable!()
        };
        if system.physical_total_bytes == 0
            || system.commit_limit_bytes == 0
            || system.physical_available_bytes > system.physical_total_bytes
            || system.commit_total_bytes > system.commit_limit_bytes
        {
            return Ok(self.defer_unknown(now_ms, Reason::MissingTelemetry));
        }
        let memory_pressure = used_bp(system.physical_total_bytes, system.physical_available_bytes)
            .max(used_bp(
                system.commit_limit_bytes,
                system.commit_limit_bytes - system.commit_total_bytes,
            ));
        let cpu_pressure = frame
            .system
            .observed_group_cpu_busy_basis_points
            .expect("coverage checked");
        let headroom_missing = system.physical_available_bytes
            < self.config.physical_available_floor_bytes
            || system.commit_limit_bytes - system.commit_total_bytes
                < self.config.commit_available_floor_bytes;
        let host_unresponsive = frame.host_responsive == Some(false);
        let budget_violation = frame.jobs.iter().any(|job| {
            let profile = self.profiles.iter().find(|state| &state.limits.profile == job.profile).expect("profile validated");
            matches!(job.usage, Measurement::Available(usage) if over_budget(usage, job.applied_budget, profile.limits.queues))
        });
        let warning = budget_violation
            || headroom_missing
            || host_unresponsive
            || memory_pressure >= self.config.pressure_enter_basis_points
            || cpu_pressure >= self.config.pressure_enter_basis_points;
        let memory_emergency =
            headroom_missing || memory_pressure >= self.config.emergency_basis_points;
        let cpu_emergency = cpu_pressure >= self.config.emergency_basis_points;
        if headroom_missing || memory_pressure >= self.config.pressure_enter_basis_points {
            self.memory_pressure_latched = true;
        }
        if cpu_pressure >= self.config.pressure_enter_basis_points {
            self.cpu_pressure_latched = true;
        }
        let memory_reentry = if self.memory_pressure_latched {
            memory_pressure <= self.config.pressure_exit_basis_points
        } else {
            memory_pressure < self.config.pressure_enter_basis_points
        };
        let cpu_reentry = if self.cpu_pressure_latched {
            cpu_pressure <= self.config.pressure_exit_basis_points
        } else {
            cpu_pressure < self.config.pressure_enter_basis_points
        };
        self.update_stage(
            now_ms,
            warning,
            !headroom_missing && !host_unresponsive && memory_reentry && cpu_reentry,
        );
        if self.stage == Stage::Normal {
            self.memory_pressure_latched = false;
            self.cpu_pressure_latched = false;
        }

        self.jobs
            .retain(|old| frame.jobs.iter().any(|job| job.identity == old.identity));
        let mut peaks = [None::<Usage>; MAX_PROFILES];
        let mut blocked = [false; MAX_PROFILES];
        let mut selected: Option<(u64, TerminationRecommendation)> = None;
        for observation in frame.jobs {
            let profile = self
                .profiles
                .iter()
                .position(|state| &state.limits.profile == observation.profile)
                .expect("profile validated");
            let Measurement::Available(usage) = observation.usage else {
                unreachable!()
            };
            let limits = &self.profiles[profile].limits;
            let violation = violation(
                usage,
                observation.applied_budget,
                limits.queues,
                memory_emergency,
                cpu_emergency,
                host_unresponsive,
            );
            let any_violation = over_budget(usage, observation.applied_budget, limits.queues);
            let track = if let Some(index) = self
                .jobs
                .iter()
                .position(|old| old.identity == observation.identity)
            {
                &mut self.jobs[index]
            } else {
                self.jobs.push(JobState {
                    identity: observation.identity,
                    profile,
                    last_observed_ms: observation.at_ms,
                    last_commit: usage.private_commit_bytes,
                    growth_anchor: None,
                    episode_peak: usage,
                    learning_tainted: false,
                    violation_since_ms: None,
                    violation_kind: None,
                    termination_issued: false,
                });
                self.jobs.last_mut().expect("track inserted")
            };
            let new_sample = observation.at_ms > track.last_observed_ms;
            if observation.incident
                || !observation.healthy
                || any_violation
                || warning
                || self.stage != Stage::Normal
            {
                track.learning_tainted = true;
            }
            if new_sample && usage.private_commit_bytes > track.last_commit {
                track.growth_anchor.get_or_insert(track.last_commit);
            }
            track.episode_peak = track.episode_peak.peak(usage);
            let returned = track
                .growth_anchor
                .is_some_and(|anchor| usage.private_commit_bytes <= anchor);
            let eligible = track.growth_anchor.is_none() || returned;
            let healthy = observation.healthy
                && !observation.incident
                && !any_violation
                && observation
                    .growth_delta_bytes
                    .is_some_and(|growth| growth <= 0)
                && eligible
                && new_sample;
            if healthy {
                let peak = if track.learning_tainted {
                    usage
                } else {
                    track.episode_peak
                };
                peaks[profile] = Some(peaks[profile].unwrap_or_default().peak(peak));
                track.growth_anchor = None;
                track.episode_peak = usage;
                track.learning_tainted = false;
            } else if !observation.healthy
                || observation.incident
                || any_violation
                || observation
                    .growth_delta_bytes
                    .is_none_or(|growth| growth > 0)
                || !eligible
            {
                blocked[profile] = true;
            }
            if new_sample {
                track.last_commit = usage.private_commit_bytes;
                track.last_observed_ms = observation.at_ms;
            }
            if let Some(kind) = violation {
                if track.violation_kind != Some(kind) {
                    track.violation_since_ms = Some(now_ms);
                }
                track.violation_kind = Some(kind);
                let since = *track.violation_since_ms.get_or_insert(now_ms);
                if self.stage == Stage::DeferStarts
                    && now_ms - self.stage_since_ms >= self.config.defer_before_terminate_ms
                    && now_ms - since >= self.config.defer_before_terminate_ms
                    && !track.termination_issued
                    && new_sample
                    && self
                        .last_termination_ms
                        .is_none_or(|last| now_ms - last >= self.config.cooldown_ms)
                {
                    let candidate = TerminationRecommendation {
                        identity: observation.identity,
                        violation: kind,
                        applied_budget: observation.applied_budget,
                    };
                    if selected.as_ref().is_none_or(|(old_since, old)| {
                        (since, candidate.identity) < (*old_since, old.identity)
                    }) {
                        selected = Some((since, candidate));
                    }
                }
            } else {
                track.violation_since_ms = None;
                track.violation_kind = None;
            }
        }
        for (index, profile) in self.profiles.iter_mut().enumerate() {
            while profile
                .samples
                .front()
                .is_some_and(|(at, _)| now_ms - at > self.config.calibration_window_ms)
            {
                profile.samples.pop_front();
            }
            if self.stage != Stage::Normal || warning || blocked[index] {
                profile.samples.clear();
            } else if let Some(usage) = peaks[index] {
                learn(profile, usage, now_ms, self.config);
            }
        }
        let terminate = selected.map(|(_, action)| action);
        if let Some(action) = terminate {
            self.jobs
                .iter_mut()
                .find(|state| state.identity == action.identity)
                .expect("selected existing job")
                .termination_issued = true;
            self.last_termination_ms = Some(now_ms);
        }
        let reason = if terminate.is_some() {
            Reason::AttributedViolation
        } else if budget_violation {
            Reason::BudgetExceeded
        } else if warning {
            Reason::Pressure
        } else if self.stage != Stage::Normal {
            Reason::RecoveryWait
        } else {
            Reason::Healthy
        };
        Ok(decision(
            self.stage,
            reason,
            headroom_missing || host_unresponsive,
            terminate,
        ))
    }
    fn defer_unknown(&mut self, now: u64, reason: Reason) -> Decision {
        self.stage = Stage::DeferStarts;
        self.stage_since_ms = now;
        self.recovery_since_ms = None;
        for profile in &mut self.profiles {
            profile.samples.clear();
        }
        for job in &mut self.jobs {
            job.violation_since_ms = None;
            job.violation_kind = None;
            job.learning_tainted = true;
        }
        decision(self.stage, reason, true, None)
    }
    fn update_stage(&mut self, now: u64, warning: bool, reentry: bool) {
        if warning {
            self.last_pressure_ms = Some(now);
            self.recovery_since_ms = None;
            match self.stage {
                Stage::Normal => {
                    self.stage = Stage::ReduceOptional;
                    self.stage_since_ms = now;
                }
                Stage::ReduceOptional
                    if now - self.stage_since_ms >= self.config.reduce_before_defer_ms =>
                {
                    self.stage = Stage::DeferStarts;
                    self.stage_since_ms = now;
                }
                _ => {}
            }
        } else if self.stage != Stage::Normal && reentry {
            let since = *self.recovery_since_ms.get_or_insert(now);
            if now - since >= self.config.stable_ms
                && self
                    .last_pressure_ms
                    .is_none_or(|last| now - last >= self.config.cooldown_ms)
            {
                self.stage = if self.stage == Stage::DeferStarts {
                    Stage::ReduceOptional
                } else {
                    Stage::Normal
                };
                self.stage_since_ms = now;
                self.recovery_since_ms = Some(now);
            }
        } else {
            self.recovery_since_ms = None;
        }
    }
}

fn decision(
    stage: Stage,
    reason: Reason,
    safety_defer: bool,
    terminate: Option<TerminationRecommendation>,
) -> Decision {
    Decision {
        stage,
        reason,
        reduce_optional: stage != Stage::Normal,
        defer_new_starts: safety_defer || stage == Stage::DeferStarts,
        terminate,
    }
}
fn frame_problem(now: u64, frame: &Frame<'_>, age: u64) -> Option<Reason> {
    if frame.system.at_ms > now
        || now - frame.system.at_ms > age
        || frame
            .jobs
            .iter()
            .any(|job| job.at_ms > now || now - job.at_ms > age)
    {
        return Some(Reason::StaleTelemetry);
    }
    match frame.cpu_coverage {
        CpuCoverage::CompleteMachine => {}
        CpuCoverage::PartialProcessorGroup => return Some(Reason::PartialCpuCoverage),
        CpuCoverage::Missing => return Some(Reason::MissingTelemetry),
    }
    if !frame.system.process_samples_complete
        || frame.system.available_processes != frame.system.unique_processes
        || frame.host_responsive.is_none()
        || frame
            .system
            .observed_group_cpu_busy_basis_points
            .is_none_or(|value| value > 10_000)
        || matches!(frame.system.system, Measurement::Unavailable(_))
        || frame
            .jobs
            .iter()
            .any(|job| !job.membership_complete || matches!(job.usage, Measurement::Unavailable(_)))
    {
        return Some(Reason::MissingTelemetry);
    }
    None
}
fn fits(value: JobBudget, limit: JobBudget) -> bool {
    value.job_commit_bytes() <= limit.job_commit_bytes()
        && value.process_commit_bytes() <= limit.process_commit_bytes()
        && value.max_processes() <= limit.max_processes()
        && value.cpu_hard_cap_basis_points() <= limit.cpu_hard_cap_basis_points()
}
fn used_bp(total: u64, available: u64) -> u16 {
    ((u128::from(total - available) * 10_000) / u128::from(total)) as u16
}
fn over_budget(usage: Usage, budget: JobBudget, queues: QueueBudget) -> bool {
    usage.private_commit_bytes > budget.job_commit_bytes()
        || usage.largest_process_commit_bytes > budget.process_commit_bytes()
        || usage.cpu_basis_points > budget.cpu_hard_cap_basis_points()
        || usage.processes > budget.max_processes()
        || usage.queue_bytes > queues.bytes
        || usage.queue_items > queues.items
}
fn violation(
    usage: Usage,
    budget: JobBudget,
    queues: QueueBudget,
    memory: bool,
    cpu: bool,
    host: bool,
) -> Option<Violation> {
    if memory && usage.private_commit_bytes > budget.job_commit_bytes() {
        Some(Violation::JobCommit)
    } else if memory && usage.largest_process_commit_bytes > budget.process_commit_bytes() {
        Some(Violation::ProcessCommit)
    } else if cpu && usage.cpu_basis_points > budget.cpu_hard_cap_basis_points() {
        Some(Violation::Cpu)
    } else if (memory || cpu) && usage.processes > budget.max_processes() {
        Some(Violation::ProcessCount)
    } else if host && usage.queue_bytes > queues.bytes {
        Some(Violation::QueueBytes)
    } else if host && usage.queue_items > queues.items {
        Some(Violation::QueueItems)
    } else {
        None
    }
}
fn scale(value: u64, basis_points: u32) -> u64 {
    ((u128::from(value) * u128::from(basis_points)).div_ceil(10_000)).min(u128::from(u64::MAX))
        as u64
}
fn learn(profile: &mut ProfileState, usage: Usage, now: u64, config: PolicyConfig) {
    while profile.samples.front().is_some_and(|(at, _)| {
        now - at > config.calibration_window_ms
            || profile.samples.len() >= config.calibration_samples
    }) {
        profile.samples.pop_front();
    }
    profile.samples.push_back((now, usage));
    if profile.samples.len() < config.minimum_healthy_samples
        || now - profile.samples[0].0 < config.stable_ms
        || profile
            .last_update_ms
            .is_some_and(|last| now - last < config.cooldown_ms)
    {
        return;
    }
    let peak = profile
        .samples
        .iter()
        .fold(Usage::default(), |max, (_, sample)| max.peak(*sample));
    let bound = |observed: u64, floor: u64, ceiling: u64, current: u64| {
        scale(observed, u32::from(config.learned_headroom_basis_points))
            .max(floor)
            .min(ceiling)
            .min(scale(
                current,
                10_000 + u32::from(config.maximum_upward_step_basis_points),
            ))
    };
    let low = profile.limits.minimum;
    let high = profile.limits.ceiling;
    let current = profile.budget;
    let job = bound(
        peak.private_commit_bytes,
        low.job_commit_bytes(),
        high.job_commit_bytes(),
        current.job_commit_bytes(),
    );
    let process = bound(
        peak.largest_process_commit_bytes,
        low.process_commit_bytes(),
        high.process_commit_bytes(),
        current.process_commit_bytes(),
    )
    .min(job);
    profile.budget = JobBudget::new(
        job,
        process,
        bound(
            u64::from(peak.processes),
            u64::from(low.max_processes()),
            u64::from(high.max_processes()),
            u64::from(current.max_processes()),
        ) as u32,
        bound(
            u64::from(peak.cpu_basis_points),
            u64::from(low.cpu_hard_cap_basis_points()),
            u64::from(high.cpu_hard_cap_basis_points()),
            u64::from(current.cpu_hard_cap_basis_points()),
        ) as u16,
    )
    .expect("bounded validated budgets");
    profile.last_update_ms = Some(now);
}

#[cfg(test)]
mod tests {
    use super::super::sampling::{IntervalUnavailable, SystemCounters, Unavailable};
    use super::*;
    use shinatama_adapter::{HomePlacement, ProgramPlacement};
    fn profile(build: &str) -> ExactProfile {
        ExactProfile {
            agent: AgentId::new("fake_resilience").unwrap(),
            version: Version::parse(build).unwrap(),
            placement: Placement::new(ProgramPlacement::System, HomePlacement::Box).unwrap(),
            operation: Operation::StartNew,
            synthetic: true,
        }
    }
    fn budget(memory: u64) -> JobBudget {
        JobBudget::new(memory, memory, 4, 2000).unwrap()
    }
    fn config() -> PolicyConfig {
        PolicyConfig {
            physical_available_floor_bytes: 100,
            commit_available_floor_bytes: 100,
            pressure_enter_basis_points: 8000,
            pressure_exit_basis_points: 6000,
            emergency_basis_points: 9500,
            max_sample_age_ms: 10,
            calibration_window_ms: 1000,
            calibration_samples: 20,
            minimum_healthy_samples: 2,
            stable_ms: 20,
            reduce_before_defer_ms: 10,
            defer_before_terminate_ms: 10,
            cooldown_ms: 20,
            learned_headroom_basis_points: 15_000,
            maximum_upward_step_basis_points: 1000,
        }
    }
    fn policy(profile: &ExactProfile) -> ResourcePolicy {
        ResourcePolicy::new(
            config(),
            vec![ProfileLimits {
                profile: profile.clone(),
                minimum: budget(1000),
                ceiling: budget(5000),
                queues: QueueBudget {
                    bytes: 1000,
                    items: 10,
                },
            }],
        )
        .unwrap()
    }
    fn system(at: u64, available: u64) -> Aggregate {
        Aggregate {
            at_ms: at,
            sampling_elapsed_us: 1,
            unique_processes: 1,
            available_processes: 1,
            observed_private_commit_bytes: 1,
            observed_resident_bytes: 1,
            process_samples_complete: true,
            interval: Err(IntervalUnavailable::FirstOrChangedMembership),
            observed_private_commit_peak_bytes: 1,
            system: Measurement::Available(SystemCounters {
                physical_total_bytes: 10_000,
                physical_available_bytes: available,
                commit_limit_bytes: 10_000,
                commit_total_bytes: 10_000 - available,
                observed_group_cpu: Measurement::Unavailable(Unavailable::Missing),
            }),
            remaining_commit_bytes: Some(available),
            observed_group_cpu_busy_basis_points: Some(1000),
        }
    }
    fn job(profile: &ExactProfile, at: u64, memory: u64) -> JobObservation<'_> {
        JobObservation {
            identity: JobIdentity {
                job_key: [1; 16],
                owner: ProcessKey {
                    pid: 1,
                    created_at_100ns: 1,
                },
                windows_session_id: 1,
            },
            profile,
            at_ms: at,
            applied_budget: budget(1000),
            usage: Measurement::Available(Usage {
                private_commit_bytes: memory,
                largest_process_commit_bytes: memory,
                cpu_basis_points: 500,
                processes: 1,
                queue_bytes: 10,
                queue_items: 1,
            }),
            membership_complete: true,
            growth_delta_bytes: Some(0),
            healthy: true,
            incident: false,
        }
    }
    fn tick(
        policy: &mut ResourcePolicy,
        at: u64,
        available: u64,
        jobs: &[JobObservation<'_>],
    ) -> Decision {
        policy
            .evaluate(
                at,
                &Frame {
                    system: &system(at, available),
                    cpu_coverage: CpuCoverage::CompleteMachine,
                    host_responsive: Some(true),
                    jobs,
                },
            )
            .unwrap()
    }
    #[test]
    fn continuous_growth_and_incident_samples_never_raise_a_baseline() {
        let profile = profile("1.0.0+build-a");
        let mut policy = policy(&profile);
        for at in 0..30 {
            let mut observation = job(&profile, at * 10, 500 + at * 10);
            observation.growth_delta_bytes = Some(10);
            if at == 10 {
                observation.incident = true;
            }
            tick(&mut policy, at * 10, 5000, &[observation]);
            assert_eq!(policy.launch_budget(&profile), Some(budget(1000)));
        }
    }
    #[test]
    fn completed_healthy_burst_can_learn_only_within_the_exact_profiles_frozen_ceiling() {
        let profile = profile("1.0.0+build-a");
        let mut policy = policy(&profile);
        for (at, memory) in [
            (0, 500),
            (10, 500),
            (20, 900),
            (30, 900),
            (40, 500),
            (50, 500),
            (60, 500),
            (80, 500),
        ] {
            let mut observation = job(&profile, at, memory);
            observation.growth_delta_bytes = Some(if at == 20 {
                400
            } else if at == 40 {
                -400
            } else {
                0
            });
            tick(&mut policy, at, 5000, &[observation]);
            let learned = policy.launch_budget(&profile).unwrap();
            assert!(learned.job_commit_bytes() <= 5000);
            if at < 40 {
                assert_eq!(learned.job_commit_bytes(), 1000);
            }
        }
        assert!(policy.launch_budget(&profile).unwrap().job_commit_bytes() > 1000);
        assert!(
            policy
                .launch_budget(&self::profile("1.0.0+build-b"))
                .is_none()
        );
    }
    #[test]
    fn external_pressure_reduces_then_defers_without_selecting_a_healthy_victim() {
        let profile = profile("1.0.0");
        let mut policy = policy(&profile);
        assert_eq!(
            tick(&mut policy, 0, 50, &[job(&profile, 0, 900)]).stage,
            Stage::ReduceOptional
        );
        for at in [10, 20, 30] {
            let result = tick(&mut policy, at, 50, &[job(&profile, at, 900)]);
            assert!(result.defer_new_starts);
            assert!(result.terminate.is_none());
        }
    }
    #[test]
    fn intervention_targets_the_exact_persistent_violator_once_after_both_stages() {
        let profile = profile("1.0.0");
        let mut policy = policy(&profile);
        let first = tick(&mut policy, 0, 50, &[job(&profile, 0, 1200)]);
        assert!(first.terminate.is_none());
        assert!(
            tick(&mut policy, 10, 50, &[job(&profile, 10, 1200)])
                .terminate
                .is_none()
        );
        let result = tick(&mut policy, 20, 50, &[job(&profile, 20, 1200)]);
        assert_eq!(
            result.terminate.unwrap().identity,
            job(&profile, 20, 1200).identity
        );
        assert_eq!(result.terminate.unwrap().applied_budget, budget(1000));
        assert!(
            tick(&mut policy, 30, 50, &[job(&profile, 30, 1200)])
                .terminate
                .is_none()
        );
    }
    #[test]
    fn stale_partial_and_unknown_monitor_samples_defer_without_learning_or_termination() {
        let profile = profile("1.0.0");
        let mut policy = policy(&profile);
        let system = system(0, 50);
        let jobs = [job(&profile, 0, 1200)];
        let mut frame = Frame {
            system: &system,
            cpu_coverage: CpuCoverage::PartialProcessorGroup,
            host_responsive: Some(true),
            jobs: &jobs,
        };
        assert_eq!(
            policy.evaluate(0, &frame).unwrap().reason,
            Reason::PartialCpuCoverage
        );
        frame.cpu_coverage = CpuCoverage::CompleteMachine;
        frame.host_responsive = None;
        assert_eq!(
            policy.evaluate(1, &frame).unwrap().reason,
            Reason::MissingTelemetry
        );
        frame.host_responsive = Some(true);
        let result = policy.evaluate(20, &frame).unwrap();
        assert_eq!(result.reason, Reason::StaleTelemetry);
        assert!(result.terminate.is_none());
        assert_eq!(policy.launch_budget(&profile), Some(budget(1000)));
    }
    #[test]
    fn reentry_requires_lower_threshold_stability_and_restores_one_stage_at_a_time() {
        let profile = profile("1.0.0");
        let mut policy = policy(&profile);
        tick(&mut policy, 0, 50, &[]);
        tick(&mut policy, 10, 50, &[]);
        assert_eq!(tick(&mut policy, 20, 3000, &[]).stage, Stage::DeferStarts); // between enter and exit
        assert_eq!(tick(&mut policy, 30, 5000, &[]).stage, Stage::DeferStarts);
        assert_eq!(tick(&mut policy, 49, 5000, &[]).stage, Stage::DeferStarts);
        assert_eq!(
            tick(&mut policy, 50, 5000, &[]).stage,
            Stage::ReduceOptional
        );
        assert_eq!(
            tick(&mut policy, 69, 5000, &[]).stage,
            Stage::ReduceOptional
        );
        assert_eq!(tick(&mut policy, 70, 5000, &[]).stage, Stage::Normal);
    }
    #[test]
    fn an_incident_peak_is_not_relabelled_as_a_later_healthy_baseline() {
        let profile = profile("1.0.0");
        let mut policy = policy(&profile);
        for at in [0, 10, 20, 30, 40, 50, 60] {
            let mut observation = job(&profile, at, 500);
            if at == 20 {
                observation.incident = true;
                let Measurement::Available(ref mut usage) = observation.usage else {
                    unreachable!()
                };
                usage.cpu_basis_points = 1900;
            }
            tick(&mut policy, at, 5000, &[observation]);
            assert_eq!(
                policy
                    .launch_budget(&profile)
                    .unwrap()
                    .cpu_hard_cap_basis_points(),
                2000
            );
        }
    }
    #[test]
    fn repeated_fresh_polls_neither_inflate_nor_erase_a_healthy_learning_window() {
        let profile = profile("1.0.0");
        let mut policy = policy(&profile);
        for (now, observed) in [(0, 0), (10, 10), (15, 10), (20, 20), (25, 20), (30, 30)] {
            tick(&mut policy, now, 5000, &[job(&profile, observed, 900)]);
            if now < 30 {
                assert_eq!(
                    policy.launch_budget(&profile).unwrap().job_commit_bytes(),
                    1000
                );
            }
        }
        assert_eq!(
            policy.launch_budget(&profile).unwrap().job_commit_bytes(),
            1100
        );
    }
    #[test]
    fn quota_overrun_warns_before_host_pressure_but_does_not_invent_a_kill_cause() {
        let profile = profile("1.0.0");
        let mut policy = policy(&profile);
        let first = tick(&mut policy, 0, 5000, &[job(&profile, 0, 1200)]);
        assert_eq!(first.stage, Stage::ReduceOptional);
        assert_eq!(first.reason, Reason::BudgetExceeded);
        let later = tick(&mut policy, 20, 5000, &[job(&profile, 20, 1200)]);
        assert!(later.defer_new_starts);
        assert!(later.terminate.is_none());
    }
    #[test]
    fn recovery_requires_exit_only_for_dimensions_that_actually_entered_pressure() {
        let mut frozen = config();
        frozen.pressure_enter_basis_points = 8500;
        frozen.pressure_exit_basis_points = 7500;
        let evaluate = |policy: &mut ResourcePolicy, now, available, cpu, coverage| {
            let mut sample = system(now, available);
            sample.observed_group_cpu_busy_basis_points = Some(cpu);
            policy
                .evaluate(
                    now,
                    &Frame {
                        system: &sample,
                        cpu_coverage: coverage,
                        host_responsive: Some(true),
                        jobs: &[],
                    },
                )
                .unwrap()
        };
        // Startup telemetry was unknown, but 80% memory never crossed 85% entry.
        let mut startup = ResourcePolicy::new(frozen, vec![]).unwrap();
        assert_eq!(
            evaluate(&mut startup, 0, 2000, 1000, CpuCoverage::Missing).stage,
            Stage::DeferStarts
        );
        assert_eq!(
            evaluate(&mut startup, 10, 2000, 1000, CpuCoverage::CompleteMachine).stage,
            Stage::DeferStarts
        );
        assert_eq!(
            evaluate(&mut startup, 30, 2000, 1000, CpuCoverage::CompleteMachine).stage,
            Stage::ReduceOptional
        );
        assert_eq!(
            evaluate(&mut startup, 50, 2000, 1000, CpuCoverage::CompleteMachine).stage,
            Stage::Normal
        );

        // A real CPU incident requires CPU below 75%, while memory may stay 80%.
        let mut cpu = ResourcePolicy::new(frozen, vec![]).unwrap();
        evaluate(&mut cpu, 0, 2000, 9000, CpuCoverage::CompleteMachine);
        evaluate(&mut cpu, 10, 2000, 9000, CpuCoverage::CompleteMachine);
        assert_eq!(
            evaluate(&mut cpu, 20, 2000, 8000, CpuCoverage::CompleteMachine).stage,
            Stage::DeferStarts
        );
        evaluate(&mut cpu, 30, 2000, 7000, CpuCoverage::CompleteMachine);
        assert_eq!(
            evaluate(&mut cpu, 50, 2000, 7000, CpuCoverage::CompleteMachine).stage,
            Stage::ReduceOptional
        );
        assert_eq!(
            evaluate(&mut cpu, 70, 2000, 7000, CpuCoverage::CompleteMachine).stage,
            Stage::Normal
        );

        // Actual memory pressure keeps the original 75% exit threshold latched.
        let mut memory = ResourcePolicy::new(frozen, vec![]).unwrap();
        evaluate(&mut memory, 0, 1000, 1000, CpuCoverage::CompleteMachine);
        evaluate(&mut memory, 10, 1000, 1000, CpuCoverage::CompleteMachine);
        for now in [20, 40, 60] {
            assert_eq!(
                evaluate(&mut memory, now, 2000, 1000, CpuCoverage::CompleteMachine).stage,
                Stage::DeferStarts
            );
        }
    }
}
