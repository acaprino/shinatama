use super::*;
use crate::{
    location::{LocationRequest, StorageLocation},
    records::ThreadKind,
    resource_reservations::{
        Budget, EmptyJobProof, ReservationOwner, ReservationPolicy, ReservationRequest,
    },
    threads::ThreadRecord,
};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    location: Option<StorageLocation>,
}
impl Fixture {
    fn new() -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let root = parent.join(format!("shinatama-native-cleanup-{}", uuid::Uuid::new_v4()));
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: parent,
            home_override: Some(root.clone()),
            project_roots: vec![],
            synchronized_roots: vec![],
        })
        .unwrap();
        Self {
            root,
            location: Some(location),
        }
    }
    fn open(&self) -> Database {
        Database::open(self.location.as_ref().unwrap(), None).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.location.take();
        assert_eq!(
            self.root.parent(),
            Some(fs::canonicalize(std::env::temp_dir()).unwrap().as_path())
        );
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-native-cleanup-")
        );
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("Synthetic native cleanup fixture retained: {error}");
        }
    }
}
fn process_identity() -> (u32, u64, u32) {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::{
            RemoteDesktop::ProcessIdToSessionId,
            Threading::{GetCurrentProcess, GetProcessTimes},
        },
    };
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    assert_ne!(
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        },
        0
    );
    let pid = std::process::id();
    let mut session = 0;
    assert_ne!(unsafe { ProcessIdToSessionId(pid, &mut session) }, 0);
    (
        pid,
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
        session,
    )
}
fn active_ticket(
    db: &Database,
    exact_alive: bool,
) -> (PolicyStamp, ReservationTicket, ActivationOwner) {
    let budget = Budget {
        memory_bytes: 1024,
        processes: 1,
        cpu_basis_points: 10,
        queue_bytes: 64,
        queue_items: 1,
    };
    let policy = ReservationPolicy {
        stamp: PolicyStamp {
            version: 1,
            id: "synthetic-native-cleanup".into(),
        },
        aggregate_ceiling: budget,
        per_slot_ceiling: budget,
        max_slots: 64,
        max_lease_ms: 10000,
    };
    let snapshot = db.initialize_resource_policy(&policy).unwrap();
    let (pid, creation, session) = process_identity();
    // A different FILETIME on our own PID simulates historical PID reuse. The
    // probe must never signal this live process or mistake it for the old owner.
    let creation = creation - u64::from(!exact_alive);
    let request = ReservationRequest {
        id: uuid::Uuid::new_v4().to_string(),
        job_key: uuid::Uuid::new_v4().to_string(),
        owner: ReservationOwner {
            pid,
            created_at_100ns: creation,
            windows_session_id: session,
            nonce: uuid::Uuid::new_v4().to_string(),
        },
        budget,
        now_ms: 100,
        lease_expires_at_ms: 1000,
    };
    let reserved = db
        .reserve_resources(&policy.stamp, snapshot.revision, &request)
        .unwrap();
    let active = db
        .activate_reservation(&policy.stamp, &reserved, 110, 1100)
        .unwrap();
    (
        policy.stamp,
        active,
        ActivationOwner {
            instance_id: "i_aaaaaaaaaaaa".into(),
            pid,
            created_at_100ns: creation,
        },
    )
}
fn bind(db: &Database, ticket: &ReservationTicket, owner: &ActivationOwner) -> NativeSessionOwner {
    db.bind_native_session_owner(ticket, owner, 120).unwrap()
}
fn activation(db: &Database, owner: &ActivationOwner) -> ActivationRecord {
    // Synthetic metadata only; no CLI, project/user files or native process.
    db.lock().unwrap().execute("INSERT INTO project VALUES('p_aaaaaaaaaaaa','C:\\synthetic-project','synthetic-project','Synthetic',10,1)", []).unwrap();
    db.create_thread(&ThreadRecord {
        id: "t_aaaaaaaaaaaa".into(),
        project_id: "p_aaaaaaaaaaaa".into(),
        kind: ThreadKind::Work,
        parent_thread_id: None,
        companion_worker_thread_id: None,
        title: None,
        archived: false,
        revision: 1,
        created_at: 10,
        updated_at: 10,
    })
    .unwrap();
    let connection = db.lock().unwrap();
    connection.execute_batch("INSERT INTO store_root VALUES('synthetic-native-root','claude_code','box','primary','C:\\synthetic-store','configured',1,'read_write',1,NULL);\nINSERT INTO segment VALUES('s_aaaaaaaaaaaa','t_aaaaaaaaaaaa',1,'claude_code','synthetic-native-root',1,'12cc3a11-45db-4f1e-89c8-40d5c6be1dd3',0,1,20);").unwrap();
    connection.execute("INSERT INTO activation(instance_id,owner_pid,owner_created_at_100ns,thread_id,segment_id,store_root_id,store_revision,native_id,revision,started_at,ended_at) VALUES(?1,?2,?3,'t_aaaaaaaaaaaa','s_aaaaaaaaaaaa','synthetic-native-root',1,'12cc3a11-45db-4f1e-89c8-40d5c6be1dd3',1,130,NULL)", params![owner.instance_id,owner.pid,owner.created_at_100ns as i64]).unwrap();
    drop(connection);
    db.thread_activation(&owner.instance_id).unwrap().unwrap()
}
fn release(db: &Database, stamp: &PolicyStamp, ticket: &ReservationTicket) {
    let proof =
        EmptyJobProof::from_trusted_closed_empty_job(ticket, &ticket.owner, &ticket.job_key, 200)
            .unwrap();
    db.release_reservation(stamp, ticket, &proof, 200).unwrap();
}

#[test]
fn durable_receipt_survives_reopen_and_recovery_cas_consumes_it_exactly_once() {
    let fixture = Fixture::new();
    let db = fixture.open();
    let (stamp, ticket, owner) = active_ticket(&db, false);
    let binding = bind(&db, &ticket, &owner);
    assert_eq!(
        db.bind_native_session_owner(&ticket, &owner, 121).unwrap(),
        binding
    );
    let active = activation(&db, &owner);
    assert!(
        db.native_session_cleanup(&owner.instance_id)
            .unwrap()
            .is_none()
    );
    release(&db, &stamp, &ticket);
    drop(db); // Fault boundary: cleanup committed, activation finalization lost.

    let first = fixture.open();
    let second = fixture.open();
    let receipt = first
        .native_session_cleanup(&owner.instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(receipt.owner, binding);
    assert!(
        first
            .resource_reservations()
            .unwrap()
            .unwrap()
            .slots
            .is_empty()
    );
    assert_eq!(
        first
            .thread_activation(&owner.instance_id)
            .unwrap()
            .unwrap(),
        active
    );
    assert!(first.acknowledge_native_session_cleanup(&receipt).is_err());
    let first_probe = probe_native_session_cleanup(&receipt).unwrap();
    assert_eq!(first_probe.host, OwnerObservation::PidReused);
    assert_eq!(first_probe.shell, OwnerObservation::PidReused);
    assert!(first_probe.same_windows_session);
    let proof = first_probe.proof.unwrap();
    assert!(matches!(
        first.recover_native_session_cleanup(&proof, 2, &active, 210),
        Err(CleanupError::Conflict)
    ));
    let mut wrong = active.clone();
    wrong.sequence += 1;
    assert!(matches!(
        first.recover_native_session_cleanup(&proof, 1, &wrong, 210),
        Err(CleanupError::Conflict)
    ));
    assert!(
        first
            .native_session_cleanup(&owner.instance_id)
            .unwrap()
            .is_some()
    );

    let other_proof = probe_native_session_cleanup(&receipt)
        .unwrap()
        .proof
        .unwrap();
    let barrier = std::sync::Barrier::new(2);
    let (left, right) = std::thread::scope(|scope| {
        let left = scope.spawn(|| {
            barrier.wait();
            first.recover_native_session_cleanup(&proof, 1, &active, 210)
        });
        let right = scope.spawn(|| {
            barrier.wait();
            second.recover_native_session_cleanup(&other_proof, 1, &active, 210)
        });
        (left.join().unwrap(), right.join().unwrap())
    });
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let ended = first
        .thread_activation(&owner.instance_id)
        .unwrap()
        .unwrap();
    assert_eq!(ended.ended_at, Some(210));
    assert_eq!(ended.revision, active.revision + 1);
    assert!(
        first
            .native_session_cleanup(&owner.instance_id)
            .unwrap()
            .is_none()
    );
    assert!(read(&first.lock().unwrap()).unwrap().entries.is_empty());
    assert_eq!(
        first
            .thread_record(&active.thread_id)
            .unwrap()
            .unwrap()
            .revision,
        2
    );
}

#[test]
fn alive_unknown_session_and_absent_receipt_never_authorize_recovery() {
    let fixture = Fixture::new();
    let db = fixture.open();
    let (stamp, ticket, owner) = active_ticket(&db, true);
    bind(&db, &ticket, &owner);
    let active = activation(&db, &owner);
    assert!(
        db.native_session_cleanup(&owner.instance_id)
            .unwrap()
            .is_none()
    );
    let fabricated = NativeCleanupReceipt {
        owner: read(&db.lock().unwrap()).unwrap().entries[0].owner.clone(),
        released_ticket: ticket.clone(),
        released_at_ms: 200,
    };
    assert!(matches!(
        db.finish_native_session_cleanup(&fabricated, &active, 210),
        Err(CleanupError::Conflict)
    ));
    assert_eq!(
        db.resource_reservations().unwrap().unwrap().slots,
        vec![ticket.clone()]
    );
    release(&db, &stamp, &ticket);
    let receipt = db
        .native_session_cleanup(&owner.instance_id)
        .unwrap()
        .unwrap();
    let probe = probe_native_session_cleanup(&receipt).unwrap();
    assert_eq!(probe.host, OwnerObservation::AliveExact);
    assert_eq!(probe.shell, OwnerObservation::AliveExact);
    assert!(probe.proof.is_none());
    let mut another_session = receipt.clone();
    another_session.owner.reservation.owner.windows_session_id ^= 1;
    another_session.released_ticket.owner.windows_session_id ^= 1;
    let probe = probe_native_session_cleanup(&another_session).unwrap();
    assert!(!probe.same_windows_session);
    assert!(probe.proof.is_none());
    let mut wrong_identity = receipt.clone();
    wrong_identity.owner.activation.created_at_100ns += 1;
    assert!(
        db.finish_native_session_cleanup(&wrong_identity, &active, 210)
            .is_err()
    );
    assert!(
        db.native_session_cleanup(&owner.instance_id)
            .unwrap()
            .is_some()
    );
    // Owning-runtime cleanup is allowed while host is still alive.
    assert_eq!(
        db.finish_native_session_cleanup(&receipt, &active, 210)
            .unwrap()
            .ended_at,
        Some(210)
    );
}

#[test]
fn failed_receipt_write_rolls_back_release_and_failed_finalization_retains_receipt() {
    let fixture = Fixture::new();
    let db = fixture.open();
    let (stamp, ticket, owner) = active_ticket(&db, false);
    bind(&db, &ticket, &owner);
    let active = activation(&db, &owner);
    db.lock().unwrap().execute_batch("CREATE TRIGGER fail_cleanup BEFORE UPDATE ON setting WHEN NEW.key='operational.native_session_cleanup.v1' BEGIN SELECT RAISE(ABORT,'synthetic fault'); END;").unwrap();
    let proof =
        EmptyJobProof::from_trusted_closed_empty_job(&ticket, &ticket.owner, &ticket.job_key, 200)
            .unwrap();
    assert!(
        db.release_reservation(&stamp, &ticket, &proof, 200)
            .is_err()
    );
    assert_eq!(
        db.resource_reservations().unwrap().unwrap().slots,
        vec![ticket.clone()]
    );
    assert!(
        db.native_session_cleanup(&owner.instance_id)
            .unwrap()
            .is_none()
    );
    db.lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_cleanup;")
        .unwrap();
    release(&db, &stamp, &ticket);
    let receipt = db
        .native_session_cleanup(&owner.instance_id)
        .unwrap()
        .unwrap();
    db.lock().unwrap().execute_batch("CREATE TRIGGER fail_cleanup BEFORE UPDATE ON setting WHEN NEW.key='operational.native_session_cleanup.v1' BEGIN SELECT RAISE(ABORT,'synthetic fault'); END;").unwrap();
    assert!(
        db.finish_native_session_cleanup(&receipt, &active, 210)
            .is_err()
    );
    assert_eq!(
        db.thread_activation(&owner.instance_id).unwrap().unwrap(),
        active
    );
    assert!(
        db.native_session_cleanup(&owner.instance_id)
            .unwrap()
            .is_some()
    );
    db.lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_cleanup;")
        .unwrap();
    db.finish_native_session_cleanup(&receipt, &active, 210)
        .unwrap();
}

#[test]
fn pending_records_are_bounded_without_eviction_and_failed_before_run_can_acknowledge() {
    let fixture = Fixture::new();
    let db = fixture.open();
    let (stamp, ticket, owner) = active_ticket(&db, false);
    bind(&db, &ticket, &owner);
    release(&db, &stamp, &ticket);
    let receipt = db
        .native_session_cleanup(&owner.instance_id)
        .unwrap()
        .unwrap();
    db.acknowledge_native_session_cleanup(&receipt).unwrap();
    assert!(read(&db.lock().unwrap()).unwrap().entries.is_empty());

    let mut journal = Journal {
        format_version: 1,
        entries: vec![],
    };
    for index in 0..MAX_RECORDS {
        let mut entry = Entry {
            owner: receipt.owner.clone(),
            cleanup: None,
        };
        // Valid base32 canonical IDs, unique for this bounded synthetic journal.
        entry.owner.activation.instance_id = format!(
            "i_aaaaaaaaaa{}{}",
            char::from(b'a' + (index / 26) as u8),
            char::from(b'a' + (index % 26) as u8)
        );
        entry.owner.reservation.id = uuid::Uuid::new_v4().to_string();
        entry.owner.reservation.job_key = uuid::Uuid::new_v4().to_string();
        journal.entries.push(entry);
    }
    write(&db.lock().unwrap(), &journal).unwrap();
    let (_, new_ticket, mut new_owner) = active_ticket(&db, false);
    new_owner.instance_id = "i_zzzzzzzzzzzz".into();
    assert!(matches!(
        db.bind_native_session_owner(&new_ticket, &new_owner, 120),
        Err(CleanupError::Capacity)
    ));
    assert_eq!(read(&db.lock().unwrap()).unwrap().entries, journal.entries);
    db.lock()
        .unwrap()
        .execute(
            "UPDATE setting SET value=?2 WHERE key=?1",
            params![SETTING_KEY, format!("\"{}\"", "x".repeat(MAX_BYTES))],
        )
        .unwrap();
    assert!(matches!(
        db.native_session_cleanup(&new_owner.instance_id),
        Err(CleanupError::Malformed)
    ));
    assert_eq!(
        db.resource_reservations().unwrap().unwrap().slots,
        vec![new_ticket]
    );
}
