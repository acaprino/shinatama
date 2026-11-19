//! Read-only resource observations and numeric UI telemetry; no launch/kill command.
use crate::resource_governor::{GovernorConfig, ResourceGovernor, Snapshot};
use serde::{Deserialize, Serialize};
use shinatama_engine::resources::policy::PolicyConfig;
#[cfg(windows)]
use shinatama_engine::resources::{
    monitor::{CallerTelemetry, MonitorConfig, ResourceMonitor, WorkerPhase},
    sampling::Measurement,
};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{State, WebviewWindow};

pub(crate) fn default_governor_config() -> GovernorConfig {
    GovernorConfig {
        policy: PolicyConfig {
            physical_available_floor_bytes: 256 * 1024 * 1024,
            commit_available_floor_bytes: 512 * 1024 * 1024,
            // Percentage pressure complements the fresh per-launch byte reserve;
            // it must not latch out a healthy host with several GiB available.
            // A normal desktop (browser, editors, other agents) sits well above
            // 90% commit on large commit limits, so the percentage gates trip only
            // when the absolute floors above are about to be crossed anyway.
            pressure_enter_basis_points: 9800,
            pressure_exit_basis_points: 9600,
            emergency_basis_points: 9900,
            max_sample_age_ms: 3000,
            calibration_window_ms: 30_000,
            calibration_samples: 30,
            minimum_healthy_samples: 5,
            stable_ms: 3000,
            reduce_before_defer_ms: 500,
            defer_before_terminate_ms: 1000,
            cooldown_ms: 3000,
            learned_headroom_basis_points: 12_500,
            maximum_upward_step_basis_points: 1000,
        },
        update_period: Duration::from_millis(500),
        max_frame_time_us: 1_500_000,
        max_ipc_roundtrip_us: 1_500_000,
    }
}

pub struct Resources {
    #[cfg(windows)]
    monitor: Option<Arc<ResourceMonitor>>,
    governor: OnceLock<ResourceGovernor>,
}
/// Fresh host memory counters for advisory notifications, independent of UI telemetry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MemoryObservation {
    pub physical_available_bytes: u64,
    pub commit_available_bytes: u64,
}
impl Resources {
    #[cfg(windows)]
    pub(crate) fn memory_observation(&self) -> Option<MemoryObservation> {
        let monitor = self.monitor.as_ref()?;
        let status = monitor.status();
        let frame = monitor.current().ok()??;
        if !matches!(status.phase, WorkerPhase::Sampling | WorkerPhase::Waiting)
            || status.observed_at_ms.checked_sub(frame.completed_at_ms)? > 3000
        {
            return None;
        }
        let aggregate = frame.aggregate.as_ref()?;
        if status.observed_at_ms.checked_sub(aggregate.at_ms)? > 3000 {
            return None;
        }
        let Measurement::Available(system) = aggregate.system else {
            return None;
        };
        Some(MemoryObservation {
            physical_available_bytes: system.physical_available_bytes,
            commit_available_bytes: aggregate.remaining_commit_bytes?,
        })
    }
    pub fn start() -> Self {
        Self {
            #[cfg(windows)]
            monitor: ResourceMonitor::start(MonitorConfig::default())
                .ok()
                .map(Arc::new),
            governor: OnceLock::new(),
        }
    }
    pub fn attach_governor(&self, maintenance: &Arc<crate::maintenance::Controller>) {
        if self.governor.get().is_some() {
            return;
        }
        maintenance.inhibit_optional_work(true);
        #[cfg(windows)]
        if let Some(monitor) = &self.monitor {
            // Conservative host-observer policy. Per-workload native limits are
            // separately admitted; this monitor never authorizes a launch alone.
            let config = default_governor_config();
            let maintenance = Arc::downgrade(maintenance);
            if let Ok(governor) = ResourceGovernor::start(
                Arc::clone(monitor),
                config,
                Vec::new(),
                Arc::new(move |inhibit| {
                    if let Some(maintenance) = maintenance.upgrade() {
                        maintenance.inhibit_optional_work(inhibit);
                    }
                }),
            ) {
                let _ = self.governor.set(governor);
            }
        }
    }
    fn policy_status(&self) -> Option<Snapshot> {
        self.governor
            .get()
            .and_then(|governor| governor.status().ok())
    }
    #[cfg(windows)]
    pub fn admission_inputs(
        &self,
    ) -> (
        Option<Arc<ResourceMonitor>>,
        Option<crate::resource_governor::GovernorReader>,
    ) {
        (
            self.monitor.clone(),
            self.governor.get().map(ResourceGovernor::reader),
        )
    }
}

#[derive(Default, Serialize)]
pub struct ResourceStatus {
    phase: &'static str,
    complete: bool,
    discovery_complete: Option<bool>,
    discovery_enumerated_processes: Option<usize>,
    discovery_unavailable_candidates: Option<usize>,
    discovery_rejected_ancestry: Option<usize>,
    discovery_deadline_reached: Option<bool>,
    monitor_issue: Option<String>,
    completed_cycles: u64,
    cycle_age_ms: Option<u64>,
    cycle_elapsed_us: Option<u64>,
    sampling_elapsed_us: Option<u64>,
    observed_processes: Option<usize>,
    measured_processes: Option<usize>,
    host_private_commit_bytes: Option<u64>,
    owned_private_commit_bytes: Option<u64>,
    owned_resident_bytes: Option<u64>,
    system_remaining_commit_bytes: Option<u64>,
    system_available_physical_bytes: Option<u64>,
    process_interval_elapsed_ms: Option<u64>,
    observed_cpu_time_100ns: Option<u64>,
    observed_group_cpu_busy_basis_points: Option<u16>,
    caller_age_ms: Option<u64>,
    caller: Option<Telemetry>,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Telemetry {
    input_latency_us: Option<u64>,
    frame_time_us: Option<u64>,
    ipc_roundtrip_us: Option<u64>,
    pending_input_events: Option<u32>,
    pending_output_bytes: Option<u64>,
    queued_launches: Option<u32>,
}

#[tauri::command(async)]
pub fn resources_status(
    window: WebviewWindow,
    resources: State<'_, Resources>,
) -> Result<ResourceStatus, &'static str> {
    if window.label() != "main" {
        return Err("Resource observations are unavailable in this window.");
    }
    Ok(observation(resources.inner()))
}

fn observation(resources: &Resources) -> ResourceStatus {
    let mut result = ResourceStatus {
        phase: "unavailable",
        ..Default::default()
    };
    #[cfg(windows)]
    if let Some(monitor) = &resources.monitor {
        let status = monitor.status();
        result.phase = match status.phase {
            WorkerPhase::Starting => "starting",
            WorkerPhase::Sampling => "sampling",
            WorkerPhase::Waiting => "waiting",
            WorkerPhase::Stopping => "stopping",
            WorkerPhase::Stopped => "stopped",
            WorkerPhase::Failed => "failed",
        };
        result.completed_cycles = status.completed_cycles;
        result.cycle_age_ms = status.last_completed_ago_ms;
        if let Ok(Some(frame)) = monitor.current() {
            result.complete = frame.complete();
            result.discovery_complete = Some(frame.discovery.complete());
            result.discovery_enumerated_processes = Some(frame.discovery.enumerated_processes);
            result.discovery_unavailable_candidates = Some(frame.discovery.unavailable_candidates);
            result.discovery_rejected_ancestry = Some(frame.discovery.rejected_ancestry);
            result.discovery_deadline_reached = Some(frame.discovery.deadline_reached);
            // These bounded enums contain counter/discovery failures, never paths,
            // process names, arguments or terminal contents.
            result.monitor_issue =
                frame
                    .failure
                    .map(|reason| format!("{reason:?}"))
                    .or_else(|| {
                        frame
                            .discovery
                            .first_failure
                            .map(|reason| format!("{reason:?}"))
                    });
            result.cycle_elapsed_us = Some(frame.cycle_elapsed_us);
            if let Some(aggregate) = &frame.aggregate {
                result.sampling_elapsed_us = Some(aggregate.sampling_elapsed_us);
                result.observed_processes = Some(aggregate.unique_processes);
                result.measured_processes = Some(aggregate.available_processes);
                result.owned_private_commit_bytes = Some(aggregate.observed_private_commit_bytes);
                result.owned_resident_bytes = Some(aggregate.observed_resident_bytes);
                result.system_remaining_commit_bytes = aggregate.remaining_commit_bytes;
                result.observed_group_cpu_busy_basis_points =
                    aggregate.observed_group_cpu_busy_basis_points;
                if let Ok(interval) = aggregate.interval {
                    result.process_interval_elapsed_ms = Some(interval.elapsed_ms);
                    result.observed_cpu_time_100ns = Some(interval.cpu_time_100ns);
                }
                if let Measurement::Available(system) = aggregate.system {
                    result.system_available_physical_bytes = Some(system.physical_available_bytes);
                }
            }
            result.host_private_commit_bytes = frame
                .owned_processes
                .iter()
                .find(|sample| Some(sample.key) == frame.host)
                .and_then(|sample| match sample.counters {
                    Measurement::Available(counters) => Some(counters.private_commit_bytes),
                    _ => None,
                });
            if let Some(caller) = frame.caller {
                result.caller_age_ms = Some(
                    frame
                        .completed_at_ms
                        .saturating_sub(caller.observed_at_ms)
                        .saturating_add(status.last_completed_ago_ms.unwrap_or(0)),
                );
                let value = caller.telemetry;
                result.caller = Some(Telemetry {
                    input_latency_us: value.input_latency_us,
                    frame_time_us: value.frame_time_us,
                    ipc_roundtrip_us: value.ipc_roundtrip_us,
                    pending_input_events: value.pending_input_events,
                    pending_output_bytes: value.pending_output_bytes,
                    queued_launches: value.queued_launches,
                });
            }
        }
    }
    result
}

#[derive(Serialize)]
pub struct TelemetryReply {
    accepted: bool,
    policy: Option<Snapshot>,
    resources: ResourceStatus,
}

#[tauri::command(async)]
pub fn resources_telemetry(
    window: WebviewWindow,
    resources: State<'_, Resources>,
    telemetry: Telemetry,
) -> Result<TelemetryReply, &'static str> {
    if window.label() != "main" {
        return Err("Resource observations are unavailable in this window.");
    }
    let mut accepted = false;
    #[cfg(windows)]
    if let Some(monitor) = &resources.monitor {
        accepted = monitor
            .report(CallerTelemetry {
                input_latency_us: telemetry.input_latency_us,
                frame_time_us: telemetry.frame_time_us,
                ipc_roundtrip_us: telemetry.ipc_roundtrip_us,
                pending_input_events: telemetry.pending_input_events,
                pending_output_bytes: telemetry.pending_output_bytes,
                queued_launches: telemetry.queued_launches,
            })
            .is_ok();
    }
    Ok(TelemetryReply {
        accepted,
        policy: resources.policy_status(),
        resources: observation(resources.inner()),
    })
}
