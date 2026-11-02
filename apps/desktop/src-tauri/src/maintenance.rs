//! Optional idle work. A single worker never competes for the UI connection.
use crate::storage::Services;
use serde::Serialize;
use shinatama_state::maintenance::{Capacity, Check, Outcome, Policy, Report};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Clone, Serialize)]
pub struct Status {
    pub phase: String,
    pub completed_at: Option<i64>,
    pub message: Option<String>,
    pub report: Option<Report>,
}

struct Control {
    stop: AtomicBool,
    running: AtomicBool,
    inhibited: AtomicBool,
    last_activity: Mutex<Instant>,
    cancellation: Mutex<Option<Arc<AtomicBool>>>,
    status: Mutex<Status>,
    wake: Condvar,
    sleep: Mutex<()>,
}

impl Control {
    fn new() -> Self {
        Self {
            stop: AtomicBool::new(false),
            running: AtomicBool::new(false),
            inhibited: AtomicBool::new(false),
            last_activity: Mutex::new(Instant::now()),
            cancellation: Mutex::new(None),
            status: Mutex::new(Status {
                phase: "waiting_idle".into(),
                completed_at: None,
                message: None,
                report: None,
            }),
            wake: Condvar::new(),
            sleep: Mutex::new(()),
        }
    }

    fn reserve(&self) -> Option<Arc<AtomicBool>> {
        // Registration and ownership are atomic with respect to revocation. An
        // activity/pressure hint cannot fall between the CAS and token install.
        let mut cancellation = self.cancellation.lock().ok()?;
        self.running
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()?;
        let cancel = Arc::new(AtomicBool::new(
            self.stop.load(Ordering::Acquire) || self.inhibited.load(Ordering::Acquire),
        ));
        *cancellation = Some(cancel.clone());
        Some(cancel)
    }
}

pub struct Controller {
    control: Arc<Control>,
    services: Arc<Services>,
}

impl Controller {
    pub fn start(services: Arc<Services>) -> Self {
        let control = Arc::new(Control::new());
        let worker = control.clone();
        let loaded = services.clone();
        let started = std::thread::Builder::new()
            .name("archive-maintenance".into())
            .spawn(move || {
                loop {
                    let guard = match worker.sleep.lock() {
                        Ok(guard) => guard,
                        Err(_) => break,
                    };
                    let waited = worker.wake.wait_timeout(guard, Duration::from_secs(60));
                    if waited.is_err() || worker.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let idle = worker
                        .last_activity
                        .lock()
                        .map(|last| last.elapsed() >= Duration::from_secs(30))
                        .unwrap_or(false);
                    if idle {
                        let _ = run(&worker, &loaded, false);
                    }
                }
            });
        if started.is_err()
            && let Ok(mut status) = control.status.lock()
        {
            status.phase = "deferred".into();
            status.message =
                Some("Background archive checks are paused until the next app launch.".into());
        }
        Self { control, services }
    }

    pub fn activity(&self) {
        if let Ok(mut last) = self.control.last_activity.lock() {
            *last = Instant::now();
        }
        if let Ok(cancellation) = self.control.cancellation.lock()
            && let Some(cancel) = cancellation.as_ref()
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn status(&self) -> Result<Status, String> {
        self.control
            .status
            .lock()
            .map(|status| {
                let mut snapshot = status.clone();
                if self.control.running.load(Ordering::Acquire) {
                    snapshot.phase = "running".into();
                }
                snapshot
            })
            .map_err(|_| "Storage check status is unavailable".into())
    }

    /// C09R may revoke optional work; clearing this flag never replays a batch.
    pub fn inhibit_optional_work(&self, inhibit: bool) {
        self.control.inhibited.store(inhibit, Ordering::Release);
        if inhibit
            && let Ok(cancellation) = self.control.cancellation.lock()
            && let Some(cancel) = cancellation.as_ref()
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Reserve before scheduling blocking work, so repeated IPC requests cannot
    /// accumulate a blocking-thread queue while an OS/storage call is stuck.
    pub fn try_check(&self) -> Option<CheckTask> {
        let cancel = self.control.reserve()?;
        Some(CheckTask {
            control: self.control.clone(),
            services: self.services.clone(),
            cancel,
        })
    }
}

pub struct CheckTask {
    control: Arc<Control>,
    services: Arc<Services>,
    cancel: Arc<AtomicBool>,
}
impl CheckTask {
    pub fn execute(self) -> Result<Status, String> {
        self.execute_with(|control, services, cancel| run_reserved(control, services, true, cancel))
    }

    fn execute_with(
        self,
        work: impl FnOnce(&Control, &Services, Arc<AtomicBool>) -> Result<Status, String>,
    ) -> Result<Status, String> {
        if let Some(status) = revoked_status(&self.control, &self.cancel)? {
            return Ok(status);
        }
        work(&self.control, &self.services, self.cancel.clone())
    }
}
impl Drop for CheckTask {
    fn drop(&mut self) {
        finish(&self.control);
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.control.stop.store(true, Ordering::Relaxed);
        self.activity();
        self.control.wake.notify_all();
        // No join on the UI thread: OS/storage calls have no hard cancellation guarantee.
    }
}

struct Running<'a>(&'a Control);
impl Drop for Running<'_> {
    fn drop(&mut self) {
        finish(self.0);
    }
}
fn finish(control: &Control) {
    if let Ok(mut cancel) = control.cancellation.lock() {
        *cancel = None;
    }
    control.running.store(false, Ordering::Release);
}

fn run(control: &Control, services: &Services, explicit: bool) -> Result<Status, String> {
    let Some(cancel) = control.reserve() else {
        return control
            .status
            .lock()
            .map(|status| status.clone())
            .map_err(|_| "Storage check status is unavailable".into());
    };
    let _running = Running(control);
    run_reserved(control, services, explicit, cancel)
}

fn revoked_status(control: &Control, cancel: &AtomicBool) -> Result<Option<Status>, String> {
    if !cancel.load(Ordering::Acquire) && !control.stop.load(Ordering::Acquire) {
        return Ok(None);
    }
    let status = Status {
        phase: "deferred".into(),
        completed_at: Some(chrono_millis()),
        message: None,
        report: None,
    };
    *control
        .status
        .lock()
        .map_err(|_| "Storage check status is unavailable")? = status.clone();
    Ok(Some(status))
}

fn run_reserved(
    control: &Control,
    services: &Services,
    explicit: bool,
    cancel: Arc<AtomicBool>,
) -> Result<Status, String> {
    if let Some(status) = revoked_status(control, &cancel)? {
        return Ok(status);
    }
    let Some(state) = services.ready() else {
        return Ok(Status {
            phase: "not_ready".into(),
            completed_at: None,
            message: None,
            report: None,
        });
    };
    if !explicit
        && !control
            .last_activity
            .lock()
            .map(|last| last.elapsed() >= Duration::from_secs(30))
            .unwrap_or(false)
    {
        cancel.store(true, Ordering::Relaxed);
    }
    let policy = Policy {
        pressure: control.inhibited.load(Ordering::Acquire) || !memory_headroom(),
        cancel,
        ..Policy::default()
    };
    if let Ok(mut status) = control.status.lock() {
        status.phase = "running".into();
    }
    let result = state.store.maintain(policy);
    let status = match result {
        Ok(report) => status_from_report(report),
        Err(error) => Status {
            phase: "attention".into(),
            completed_at: Some(chrono_millis()),
            message: Some(error.to_string()),
            report: None,
        },
    };
    *control
        .status
        .lock()
        .map_err(|_| "Storage check status is unavailable")? = status.clone();
    Ok(status)
}

fn status_from_report(report: Report) -> Status {
    let mut status = Status {
        phase: "idle".into(),
        completed_at: Some(chrono_millis()),
        message: None,
        report: None,
    };
    if report.quick_check == Check::Corrupt {
        status.phase = "attention".into();
        status.message = Some(
            "The saved archive needs an integrity check. Your saved work has been kept.".into(),
        );
    } else if report.outcome == Outcome::DeferredCapacity
        || report
            .growth
            .as_ref()
            .is_some_and(|growth| growth.capacity != Capacity::Healthy)
    {
        status.phase = "attention".into();
        status.message = Some(
            "Archive storage is low on space or at its limit. Your saved work has been kept."
                .into(),
        );
    } else if report.outcome == Outcome::InvalidMetadata {
        status.phase = "attention".into();
        status.message =
            Some("Background archive checks need attention. Your saved work has been kept.".into());
    } else if !report.anomalies.is_empty() {
        status.phase = "attention".into();
        status.message = Some(
            "Some archive records need attention. Their original content has been kept.".into(),
        );
    } else if report.outcome != Outcome::Completed {
        status.phase = "deferred".into();
    }
    status.report = Some(report);
    status
}

fn chrono_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(windows)]
fn memory_headroom() -> bool {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut memory: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    (unsafe { GlobalMemoryStatusEx(&mut memory) != 0 })
        && memory.ullAvailPageFile >= 1024 * 1024 * 1024
        && memory.ullAvailPhys >= 512 * 1024 * 1024
}

#[cfg(not(windows))]
fn memory_headroom() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_state::maintenance::{Checkpoint, Growth};

    fn controller() -> Controller {
        Controller {
            control: Arc::new(Control::new()),
            services: Arc::new(Services::new(Err("Storage must stay unloaded".into()))),
        }
    }

    fn assert_revoked_before_work(task: CheckTask) {
        let called = AtomicBool::new(false);
        let status = task
            .execute_with(|_, _, _| {
                called.store(true, Ordering::Relaxed);
                Err("Revoked work must never start".into())
            })
            .expect("revocation is a normal deferral");
        assert!(!called.load(Ordering::Relaxed));
        assert_eq!(status.phase, "deferred");
        assert!(status.report.is_none());
        assert!(status.message.is_none());
    }

    #[test]
    fn activity_revokes_a_reserved_task_before_it_starts() {
        let controller = controller();
        let task = controller.try_check().expect("first reservation");
        assert!(controller.try_check().is_none());
        controller.activity();
        assert_revoked_before_work(task);
        assert!(!controller.control.running.load(Ordering::Acquire));
        assert!(controller.services.ready().is_none());
        // A new explicit request receives a new token, not a replay of the old one.
        let next = controller.try_check().expect("reservation released");
        assert!(!next.cancel.load(Ordering::Acquire));
    }

    #[test]
    fn clearing_pressure_does_not_revive_a_reserved_task() {
        let controller = controller();
        let task = controller.try_check().expect("first reservation");
        controller.inhibit_optional_work(true);
        controller.inhibit_optional_work(false);
        assert_revoked_before_work(task);
        assert!(!controller.control.running.load(Ordering::Acquire));
        assert!(controller.services.ready().is_none());
        let next = controller.try_check().expect("new admission is allowed");
        assert!(!next.cancel.load(Ordering::Acquire));
    }

    #[test]
    fn capacity_deferral_is_visible_even_when_database_size_is_healthy() {
        let report = Report {
            outcome: Outcome::DeferredCapacity,
            observed_rows: 0,
            inspected_payload_bytes: 0,
            vm_steps_approx: 0,
            elapsed_micros: 0,
            admission_elapsed_micros: 0,
            growth: Some(Growth {
                database_bytes: 4096,
                wal_bytes: 0,
                shared_memory_bytes: 0,
                total_bytes: 4096,
                capacity: Capacity::Healthy,
            }),
            quick_check: Check::NotRequested,
            checkpoint: Checkpoint::NotRun,
            optimized: false,
            vacuum_pages_requested: 0,
            more_rows: false,
            anomalies: Vec::new(),
            deferred_rows: Vec::new(),
            unrecorded_deferred_rows: 0,
            unrecorded_anomalies: 0,
            lease_released: false,
            scan_progress_saved: false,
        };
        let status = status_from_report(report);
        assert_eq!(status.phase, "attention");
        assert!(status.message.as_deref().is_some_and(|message| {
            message.contains("space") && message.contains("saved work has been kept")
        }));
        assert_eq!(
            status.report.expect("diagnostic is preserved").outcome,
            Outcome::DeferredCapacity
        );
    }
}
