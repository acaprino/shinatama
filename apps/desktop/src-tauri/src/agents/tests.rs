//! Request ownership tests use in-memory services, without storage or native processes.
use super::{
    AgentCheckup, AgentStarted, Controller, RegisteredService,
    service::{HarnessDescriptor, HarnessService, OpenMode, ServiceError, SessionOperation},
};
use crate::storage::Services;
use shinatama_terminal::TerminalHost;
use std::{
    collections::BTreeMap,
    future::Future,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);

struct Gate {
    entered: SyncSender<()>,
    release: Receiver<()>,
}

struct FakeService {
    descriptor: HarnessDescriptor,
    panic_on_check: bool,
    gate: Mutex<Option<Gate>>,
    checks: AtomicUsize,
}

impl FakeService {
    fn new(id: &'static str, native_resume: bool) -> Self {
        Self {
            descriptor: HarnessDescriptor {
                id,
                label: id,
                native_resume,
            },
            panic_on_check: false,
            gate: Mutex::new(None),
            checks: AtomicUsize::new(0),
        }
    }
}

impl HarnessService for FakeService {
    fn descriptor(&self) -> HarnessDescriptor {
        self.descriptor
    }

    fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"checks": self.checks.load(Ordering::Acquire)})
    }

    fn check(&self) -> Result<AgentCheckup, ServiceError> {
        self.checks.fetch_add(1, Ordering::AcqRel);
        assert!(!self.panic_on_check, "synthetic harness failure");
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            gate.entered.send(()).unwrap();
            gate.release.recv_timeout(WAIT).unwrap();
        }
        Ok(AgentCheckup::empty(
            self.descriptor.id,
            self.descriptor.label,
        ))
    }

    fn open(
        &self,
        _: &TerminalHost,
        _: &Path,
        _: u16,
        _: u16,
        _: SessionOperation,
        _: OpenMode,
        _: Option<&str>,
        _: bool,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<AgentStarted, ServiceError> {
        panic!("request-isolation tests must not start a terminal")
    }
}

fn controller(services: &[Arc<FakeService>]) -> Arc<Controller> {
    let mut registry = BTreeMap::new();
    for service in services {
        registry.insert(
            service.descriptor.id,
            Arc::new(RegisteredService {
                service: service.clone(),
                busy: AtomicBool::new(false),
                blocked: AtomicBool::new(false),
            }),
        );
    }
    Arc::new(Controller {
        services: Arc::new(Services::new(Err("storage disabled in this test".into()))),
        registry,
        host: Arc::new(TerminalHost::default()),
    })
}

#[test]
fn a_controller_with_no_started_terminals_has_no_live_agent_sessions() {
    assert_eq!(controller(&[]).live_agent_sessions(), Some(0));
}

#[test]
fn panic_blocks_only_its_harness_and_catalog_dispatches_registered_services() {
    let mut broken = FakeService::new("synthetic_broken", false);
    broken.panic_on_check = true;
    let broken = Arc::new(broken);
    let healthy = Arc::new(FakeService::new("synthetic_healthy", true));
    let owner = controller(&[Arc::clone(&healthy), Arc::clone(&broken)]);
    let catalog = owner.catalog();
    assert_eq!(catalog.len(), 2);
    assert_eq!(catalog[0].id, "synthetic_broken");
    assert!(!catalog[0].native_resume);
    assert_eq!(catalog[1].id, "synthetic_healthy");
    assert!(catalog[1].native_resume);

    tauri::async_runtime::block_on(async {
        assert!(owner.check("synthetic_broken").await.is_err());
        assert!(owner.check("synthetic_broken").await.is_err());
        let report = owner.check("synthetic_healthy").await.unwrap();
        assert_eq!(report.agent_id, "synthetic_healthy");
        assert!(owner.check("unregistered").await.is_err());
    });

    assert_eq!(broken.checks.load(Ordering::Acquire), 1);
    assert_eq!(healthy.checks.load(Ordering::Acquire), 1);
    let diagnostics = owner.diagnostics();
    assert_eq!(
        diagnostics["harnesses"]["synthetic_broken"]["blocked"],
        true
    );
    assert_eq!(
        diagnostics["harnesses"]["synthetic_broken"]["checking"],
        false
    );
    assert_eq!(
        diagnostics["harnesses"]["synthetic_healthy"]["blocked"],
        false
    );
}

#[test]
fn cancelling_a_check_keeps_its_slot_until_the_worker_finishes() {
    let (entered, started) = mpsc::sync_channel(1);
    let (release, continue_work) = mpsc::sync_channel(1);
    let pending = Arc::new(FakeService::new("synthetic_pending", false));
    *pending.gate.lock().unwrap() = Some(Gate {
        entered,
        release: continue_work,
    });
    let healthy = Arc::new(FakeService::new("synthetic_healthy", false));
    let owner = controller(&[Arc::clone(&pending), Arc::clone(&healthy)]);

    // Poll the real IPC future once so its blocking worker owns RequestSlot,
    // then discard the caller exactly as a closed/cancelled UI request would.
    let mut request = Box::pin(owner.check("synthetic_pending"));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(request.as_mut().poll(&mut context), Poll::Pending));
    started.recv_timeout(WAIT).unwrap();
    drop(request);

    tauri::async_runtime::block_on(async {
        assert!(owner.check("synthetic_pending").await.is_err());
        assert_eq!(
            owner.check("synthetic_healthy").await.unwrap().agent_id,
            "synthetic_healthy"
        );
    });
    assert_eq!(pending.checks.load(Ordering::Acquire), 1);
    assert!(
        owner.registry["synthetic_pending"]
            .busy
            .load(Ordering::Acquire)
    );

    release.send(()).unwrap();
    let deadline = Instant::now() + WAIT;
    while owner.registry["synthetic_pending"]
        .busy
        .load(Ordering::Acquire)
    {
        assert!(
            Instant::now() < deadline,
            "completed worker retained its slot"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let report = tauri::async_runtime::block_on(owner.check("synthetic_pending")).unwrap();
    assert_eq!(report.agent_id, "synthetic_pending");
    assert_eq!(pending.checks.load(Ordering::Acquire), 2);
    assert!(
        !owner.registry["synthetic_pending"]
            .blocked
            .load(Ordering::Acquire)
    );
}
