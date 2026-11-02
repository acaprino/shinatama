#![cfg(windows)]
//! Real two-process ledger admission, with no workload, job or release proof.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use shinatama_state::{
    database::Database,
    location::{LocationRequest, StorageLocation},
    resource_reservations::{
        Budget, PolicyStamp, ReservationError, ReservationOwner, ReservationPolicy,
        ReservationRequest, ReservationState,
    },
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{FILETIME, HANDLE, WAIT_OBJECT_0},
    System::{
        RemoteDesktop::ProcessIdToSessionId,
        Threading::{
            DETACHED_PROCESS, GetCurrentProcess, GetCurrentProcessId, GetProcessId,
            GetProcessTimes, WaitForSingleObject,
        },
    },
};

const ROOT: &str = "SHINATAMA_RESERVATION_TEST_ROOT";
const INDEX: &str = "SHINATAMA_RESERVATION_TEST_INDEX";
const WAIT: Duration = Duration::from_secs(5);

fn policy() -> ReservationPolicy {
    let budget = Budget {
        memory_bytes: 64 * 1024 * 1024,
        processes: 2,
        cpu_basis_points: 1000,
        queue_bytes: 4096,
        queue_items: 16,
    };
    ReservationPolicy {
        stamp: PolicyStamp {
            version: 1,
            id: "synthetic-process-race".into(),
        },
        aggregate_ceiling: budget,
        per_slot_ceiling: budget,
        max_slots: 2,
        max_lease_ms: 10_000,
    }
}
fn location(root: &Path) -> StorageLocation {
    StorageLocation::resolve(LocationRequest {
        local_data_directory: std::env::temp_dir(),
        home_override: Some(root.to_owned()),
        project_roots: Vec::new(),
        synchronized_roots: Vec::new(),
    })
    .unwrap()
}
fn native_identity(handle: HANDLE) -> (u32, u64, u32) {
    let pid = unsafe { GetProcessId(handle) };
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    assert_ne!(
        unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) },
        0
    );
    let mut session = 0;
    assert_ne!(unsafe { ProcessIdToSessionId(pid, &mut session) }, 0);
    assert_ne!(pid, 0);
    (
        pid,
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
        session,
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ready {
    owner: ReservationOwner,
    revision: u64,
    id: String,
    job_key: String,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Won,
    ConflictThenCapacity,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    ready: Ready,
    outcome: Outcome,
}

fn publish(root: &Path, name: &str, value: &impl Serialize) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert!(bytes.len() <= 1024);
    let partial = root.join(format!("{name}.part"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
    drop(file);
    fs::rename(partial, root.join(name)).unwrap();
}
fn receive<T: DeserializeOwned>(root: &Path, name: &str) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        match File::open(root.join(name)) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(1025).read_to_end(&mut bytes).unwrap();
                assert!(bytes.len() <= 1024, "synthetic metadata exceeded 1 KiB");
                return serde_json::from_slice(&bytes).unwrap();
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("bounded barrier {name}: {error}"),
        }
    }
}

struct Fixture {
    root: PathBuf,
    base: PathBuf,
    safe_cleanup: Arc<AtomicBool>,
}
impl Fixture {
    fn new() -> Self {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let root = base.join(format!(
            "shinatama-reservation-race-{}",
            uuid::Uuid::new_v4()
        ));
        location(&root).prepare_private_directory().unwrap();
        Self {
            root,
            base,
            safe_cleanup: Arc::new(AtomicBool::new(true)),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.safe_cleanup.load(Ordering::Acquire) {
            eprintln!("Synthetic reservation fixture retained: owned helper shutdown unconfirmed");
            return;
        }
        assert!(self.root.is_absolute());
        assert_eq!(self.root.parent(), Some(self.base.as_path()));
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-reservation-race-")
        );
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("Synthetic reservation fixture cleanup: {error}");
        }
    }
}
struct Helpers {
    children: Vec<Child>,
    safe_cleanup: Arc<AtomicBool>,
}
impl Helpers {
    fn spawn(&mut self, root: &Path, index: usize) {
        assert!(self.children.len() < 2);
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "reservation_process_helper", "--test-threads=1"])
            .env_clear()
            .env("SystemRoot", std::env::var_os("SystemRoot").unwrap())
            .env(ROOT, root)
            .env(INDEX, index.to_string())
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED_PROCESS)
            .spawn()
            .unwrap();
        self.children.push(child);
    }
    fn wait_all(&mut self) {
        let deadline = Instant::now() + WAIT;
        loop {
            let mut ended = 0;
            for child in &mut self.children {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "owned helper failed: {status}");
                    ended += 1;
                }
            }
            if ended == self.children.len() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "owned helpers exceeded five seconds"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Helpers {
    fn drop(&mut self) {
        for child in &mut self.children {
            if matches!(child.try_wait(), Ok(Some(_))) {
                continue;
            }
            // Only handles of these two explicitly spawned helpers are stopped.
            let _ = child.kill();
            if unsafe { WaitForSingleObject(child.as_raw_handle(), 5000) } != WAIT_OBJECT_0 {
                self.safe_cleanup.store(false, Ordering::Release);
            }
            let _ = child.try_wait();
        }
    }
}

#[test]
fn reservation_process_helper() {
    let Some(index) = std::env::var_os(INDEX) else {
        return;
    };
    let index: usize = index.to_str().unwrap().parse().unwrap();
    assert!(index < 2);
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    let (pid, created_at_100ns, windows_session_id) =
        native_identity(unsafe { GetCurrentProcess() });
    assert_eq!(pid, unsafe { GetCurrentProcessId() });
    let owner = ReservationOwner {
        pid,
        created_at_100ns,
        windows_session_id,
        nonce: uuid::Uuid::new_v4().to_string(),
    };
    let database = Database::open(&location(&root), None).unwrap();
    let initial = database.resource_reservations().unwrap().unwrap();
    let ready = Ready {
        owner,
        revision: initial.revision,
        id: uuid::Uuid::new_v4().to_string(),
        job_key: uuid::Uuid::new_v4().to_string(),
    };
    publish(&root, &format!("ready-{index}"), &ready);
    assert!(receive::<bool>(&root, "start"));
    let now = chrono::Utc::now().timestamp_millis();
    let request = ReservationRequest {
        id: ready.id.clone(),
        job_key: ready.job_key.clone(),
        owner: ready.owner.clone(),
        budget: policy().per_slot_ceiling,
        now_ms: now,
        lease_expires_at_ms: now + 10_000,
    };
    let outcome = match database.reserve_resources(&initial.policy.stamp, ready.revision, &request)
    {
        Ok(ticket) => {
            assert_eq!(ticket.owner, ready.owner);
            Outcome::Won
        }
        Err(ReservationError::Conflict) => {
            let fresh = database.resource_reservations().unwrap().unwrap();
            assert!(fresh.revision > ready.revision);
            assert!(matches!(
                database.reserve_resources(&fresh.policy.stamp, fresh.revision, &request),
                Err(ReservationError::Capacity)
            ));
            Outcome::ConflictThenCapacity
        }
        other => panic!("unexpected cross-process admission result: {other:?}"),
    };
    publish(
        &root,
        &format!("result-{index}"),
        &Report { ready, outcome },
    );
}

#[test]
fn two_real_processes_share_one_atomic_budget_and_exit_does_not_release_it() {
    let fixture = Fixture::new();
    let location = location(&fixture.root);
    let database = Database::open(&location, None).unwrap();
    let policy = policy();
    assert_eq!(
        database
            .initialize_resource_policy(&policy)
            .unwrap()
            .revision,
        0
    );
    let mut helpers = Helpers {
        children: Vec::new(),
        safe_cleanup: fixture.safe_cleanup.clone(),
    };
    helpers.spawn(&fixture.root, 0);
    helpers.spawn(&fixture.root, 1);
    let ready: Vec<Ready> = (0..2)
        .map(|index| receive(&fixture.root, &format!("ready-{index}")))
        .collect();
    for (child, value) in helpers.children.iter().zip(&ready) {
        assert_eq!(
            value.revision, 0,
            "both processes must read before the start barrier"
        );
        let native = native_identity(child.as_raw_handle());
        assert_eq!(
            native,
            (
                value.owner.pid,
                value.owner.created_at_100ns,
                value.owner.windows_session_id
            )
        );
        assert_eq!(child.id(), value.owner.pid);
        assert_eq!(
            uuid::Uuid::parse_str(&value.owner.nonce)
                .unwrap()
                .get_version_num(),
            4
        );
    }
    assert_ne!(ready[0].owner.pid, ready[1].owner.pid);
    assert_ne!(ready[0].owner.nonce, ready[1].owner.nonce);
    publish(&fixture.root, "start", &true);
    helpers.wait_all();
    let reports: Vec<Report> = (0..2)
        .map(|index| receive(&fixture.root, &format!("result-{index}")))
        .collect();
    for (report, expected) in reports.iter().zip(&ready) {
        assert_eq!(&report.ready, expected);
    }
    assert_eq!(
        reports
            .iter()
            .filter(|report| report.outcome == Outcome::Won)
            .count(),
        1
    );
    assert_eq!(
        reports
            .iter()
            .filter(|report| report.outcome == Outcome::ConflictThenCapacity)
            .count(),
        1
    );
    let winner = reports
        .iter()
        .find(|report| report.outcome == Outcome::Won)
        .unwrap();
    let snapshot = database.resource_reservations().unwrap().unwrap();
    assert_eq!(snapshot.revision, 1);
    assert_eq!(snapshot.slots.len(), 1);
    assert_eq!(snapshot.reserved_total().unwrap(), policy.aggregate_ceiling);
    assert_eq!(snapshot.slots[0].owner, winner.ready.owner);
    assert_eq!(snapshot.slots[0].id, winner.ready.id);
    assert_eq!(snapshot.slots[0].job_key, winner.ready.job_key);
    assert_eq!(snapshot.slots[0].state, ReservationState::Reserved);
    // Process exit is not a closed-empty workload proof and cannot release budget.
    drop(database);
    let reopened = Database::open(&location, None).unwrap();
    assert_eq!(reopened.resource_reservations().unwrap().unwrap(), snapshot);
}
