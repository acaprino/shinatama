//! Read-only resource observations and numeric UI telemetry; no launch/kill command.
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use shinatama_engine::resources::{
    monitor::{CallerTelemetry, MonitorConfig, ResourceMonitor, WorkerPhase},
    sampling::Measurement,
};
use tauri::{State, WebviewWindow};

pub struct Resources {
    #[cfg(windows)]
    monitor: Option<ResourceMonitor>,
}
impl Resources {
    pub fn start() -> Self {
        Self {
            #[cfg(windows)]
            monitor: ResourceMonitor::start(MonitorConfig::default()).ok(),
        }
    }
}

#[derive(Default, Serialize)]
pub struct ResourceStatus {
    phase: &'static str,
    complete: bool,
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

#[tauri::command]
pub fn resources_status(
    window: WebviewWindow,
    resources: State<'_, Resources>,
) -> Result<ResourceStatus, &'static str> {
    if window.label() != "main" {
        return Err("Resource observations are unavailable in this window.");
    }
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
    Ok(result)
}

#[tauri::command]
pub fn resources_telemetry(
    window: WebviewWindow,
    resources: State<'_, Resources>,
    telemetry: Telemetry,
) -> Result<bool, &'static str> {
    if window.label() != "main" {
        return Err("Resource observations are unavailable in this window.");
    }
    #[cfg(windows)]
    if let Some(monitor) = &resources.monitor {
        return Ok(monitor
            .report(CallerTelemetry {
                input_latency_us: telemetry.input_latency_us,
                frame_time_us: telemetry.frame_time_us,
                ipc_roundtrip_us: telemetry.ipc_roundtrip_us,
                pending_input_events: telemetry.pending_input_events,
                pending_output_bytes: telemetry.pending_output_bytes,
                queued_launches: telemetry.queued_launches,
            })
            .is_ok());
    }
    Ok(false)
}
