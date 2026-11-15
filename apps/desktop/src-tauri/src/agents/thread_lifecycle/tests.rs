//! Lifecycle integration with a fresh private database and authenticated loopback
//! events. Native identities and cleanup proofs are synthetic; no harness runs.
use super::*;
use shinatama_engine::launch::identity::LaunchIdentity;
use shinatama_state::{
    location::{LocationRequest, StorageLocation},
    native_session_cleanup::CleanupError,
    records::{PreparedStoreRoot, RootAccess, RootKind, RootOrigin, RootScope, ThreadKind},
    resource_reservations::{
        Budget, EmptyJobProof, PolicyStamp, ReservationOwner, ReservationPolicy, ReservationRequest,
    },
};
use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, TcpStream},
    path::PathBuf,
};

const CODEX_NATIVE: &str = "018fba11-45db-7f1e-89c8-40d5c6be1dd3";
const CLAUDE_NATIVE: &str = "12cc3a11-45db-4f1e-89c8-40d5c6be1dd3";

struct Fixture {
    directory: PathBuf,
    location: Option<StorageLocation>,
    database: Option<Arc<Database>>,
    project: PathBuf,
    thread: ThreadRecord,
    root: StoreRootRecord,
    identity: LaunchIdentity,
    stamp: PolicyStamp,
    ticket: ReservationTicket,
    process: ProcessKey,
    notifications: Arc<Mutex<Vec<Option<bool>>>>,
}
impl Fixture {
    fn new(harness: HarnessKind) -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let directory = parent.join(format!(
            "shinatama-thread-lifecycle-{}",
            uuid::Uuid::new_v4()
        ));
        let project = directory.join("project");
        fs::create_dir_all(&project).unwrap();
        let location = StorageLocation::resolve(LocationRequest {
            local_data_directory: parent,
            home_override: Some(directory.join("data")),
            project_roots: vec![project.clone()],
            synchronized_roots: vec![],
        })
        .unwrap();
        let database = Arc::new(Database::open(&location, None).unwrap());
        let selected = database.project_manager_select(&project).unwrap();
        let at = now().unwrap();
        let thread = ThreadRecord {
            id: new_id(CanonicalIdKind::Thread),
            project_id: selected.id,
            kind: ThreadKind::Work,
            parent_thread_id: None,
            companion_worker_thread_id: None,
            title: None,
            archived: false,
            revision: 1,
            created_at: at,
            updated_at: at,
        };
        database.create_thread(&thread).unwrap();
        let archive = location
            .resolve_related_directory(&location.directory().join("native"))
            .unwrap();
        archive.prepare_private_directory().unwrap();
        let root = StoreRootRecord {
            id: "synthetic-lifecycle-root".into(),
            harness: harness.id().into(),
            kind: RootKind::Box,
            scope: RootScope::Primary,
            resolved_path: archive
                .directory()
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .into(),
            origin: RootOrigin::Configured,
            config_revision: 1,
            access: RootAccess::ReadWrite,
            available: true,
        };
        database
            .register_store_root(&PreparedStoreRoot::resolve(root.clone()).unwrap())
            .unwrap();
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
                id: "synthetic-lifecycle".into(),
            },
            aggregate_ceiling: budget,
            per_slot_ceiling: budget,
            max_slots: 1,
            max_lease_ms: 300_000,
        };
        let ledger = database.initialize_resource_policy(&policy).unwrap();
        let reserved = database
            .reserve_resources(
                &policy.stamp,
                ledger.revision,
                &ReservationRequest {
                    id: uuid::Uuid::new_v4().to_string(),
                    job_key: uuid::Uuid::new_v4().to_string(),
                    owner: ReservationOwner {
                        pid: 101,
                        created_at_100ns: 1001,
                        windows_session_id: 1,
                        nonce: uuid::Uuid::new_v4().to_string(),
                    },
                    budget,
                    now_ms: at,
                    lease_expires_at_ms: at + 300_000,
                    advisory: false,
                },
            )
            .unwrap();
        let ticket = database
            .activate_reservation(&policy.stamp, &reserved, at, at + 300_000)
            .unwrap();
        let identity = LaunchIdentity::generate(&thread.id).unwrap();
        Self {
            directory,
            location: Some(location),
            database: Some(database),
            project,
            thread,
            root,
            identity,
            stamp: policy.stamp,
            ticket,
            process: ProcessKey {
                pid: 202,
                created_at_100ns: 2002,
            },
            notifications: Arc::new(Mutex::new(Vec::new())),
        }
    }
    fn db(&self) -> &Arc<Database> {
        self.database.as_ref().unwrap()
    }
    fn lifecycle(
        &self,
        harness: HarnessKind,
        requested: Option<&str>,
    ) -> (Arc<ThreadLifecycle>, u16) {
        let receiver =
            SessionEventReceiver::start_for(harness, self.identity.record(), &self.project)
                .unwrap();
        let port = receiver.port();
        let database = Arc::clone(self.db());
        let notifications = Arc::clone(&self.notifications);
        let expected_project = self.thread.project_id.clone();
        let expected_thread = self.thread.id.clone();
        let lifecycle = ThreadLifecycle::new(
            Arc::clone(self.db()),
            PendingThread::Start {
                thread: self.thread.clone(),
                root: self.root.clone(),
                native_id: requested.map(str::to_owned),
            },
            self.identity.record(),
            receiver,
            Arc::new(move |project, thread| {
                // The observer can already see the committed metadata. It
                // never reads the lifecycle mutex or controls its transition.
                let committed = (project == expected_project && thread == expected_thread)
                    .then(|| database.thread_resume_target(thread).ok())
                    .flatten()
                    .map(|saved| saved.recorded_activation.is_none());
                notifications.lock().unwrap().push(committed);
                if harness == HarnessKind::Codex {
                    panic!("Synthetic UI delivery failure after committed thread update");
                }
            }),
        )
        .unwrap();
        lifecycle
            .before_run(self.identity.instance_id(), self.process, &self.ticket)
            .unwrap();
        (lifecycle, port)
    }
    fn send(
        &self,
        harness: HarnessKind,
        port: u16,
        session: &str,
        kind: SessionEventKind,
        source: Option<SessionStartSource>,
    ) {
        self.send_as(harness, port, &self.identity, session, kind, source)
    }
    fn send_as(
        &self,
        harness: HarnessKind,
        port: u16,
        identity: &LaunchIdentity,
        session: &str,
        kind: SessionEventKind,
        source: Option<SessionStartSource>,
    ) {
        let mut body = serde_json::json!({ "session_id": session, "hook_event_name": kind, "cwd": self.project.to_str().unwrap() });
        if let Some(source) = source {
            body["source"] = serde_json::to_value(source).unwrap();
        }
        let body = serde_json::to_vec(&body).unwrap();
        let secret = identity.child_secret_override();
        let request = format!(
            "POST {}{} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            harness.route_prefix(),
            identity.instance_id(),
            secret.value().to_str().unwrap(),
            body.len()
        );
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        let mut response = Vec::new();
        stream.take(1024).read_to_end(&mut response).unwrap();
        assert!(
            response.starts_with(b"HTTP/1.1 204"),
            "The synthetic authenticated event must be queued."
        );
    }
    fn release(&self) {
        let at = now().unwrap();
        let proof = EmptyJobProof::from_trusted_closed_empty_job(
            &self.ticket,
            &self.ticket.owner,
            &self.ticket.job_key,
            at,
        )
        .unwrap();
        self.db()
            .release_reservation(&self.stamp, &self.ticket, &proof, at)
            .unwrap();
        let receipt = self
            .db()
            .native_session_cleanup(self.identity.instance_id())
            .unwrap()
            .unwrap();
        assert_eq!(receipt.owner.reservation, self.ticket);
        assert_eq!(receipt.owner.activation.pid, self.process.pid);
        assert_eq!(
            receipt.owner.activation.created_at_100ns,
            self.process.created_at_100ns
        );
    }
    fn active(&self) -> Option<ActivationRecord> {
        self.db()
            .thread_activation(self.identity.instance_id())
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.database.take();
        self.location.take();
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert_eq!(self.directory.parent(), Some(parent.as_path()));
        assert!(
            self.directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-thread-lifecycle-")
        );
        let _ = fs::remove_dir_all(&self.directory);
    }
}
fn eventually(check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !check() {
        assert!(
            Instant::now() < deadline,
            "The bounded lifecycle observation did not settle."
        );
        thread::sleep(POLL);
    }
}
fn finish_with_receipt(fixture: &Fixture, lifecycle: &ThreadLifecycle) {
    assert!(
        lifecycle
            .after_cleanup(fixture.identity.instance_id(), fixture.process)
            .is_err()
    );
    assert!(
        fixture
            .active()
            .is_some_and(|active| active.ended_at.is_none())
    );
    fixture.release();
    lifecycle
        .after_cleanup(fixture.identity.instance_id(), fixture.process)
        .unwrap();
    let ended = fixture.active().unwrap();
    assert!(ended.ended_at.is_some());
    assert!(
        fixture
            .db()
            .native_session_cleanup(fixture.identity.instance_id())
            .unwrap()
            .is_none()
    );
    lifecycle
        .after_cleanup(fixture.identity.instance_id(), fixture.process)
        .unwrap();
    assert_eq!(fixture.active().unwrap(), ended);
    assert_eq!(
        *fixture.notifications.lock().unwrap(),
        vec![Some(false), Some(true)]
    );
    assert!(!lifecycle.core.failed.load(Ordering::Acquire));
}

#[test]
fn codex_native_id_is_observed_after_start_and_session_end_cannot_finalize_ownership() {
    let fixture = Fixture::new(HarnessKind::Codex);
    let (lifecycle, port) = fixture.lifecycle(HarnessKind::Codex, None);
    assert!(fixture.active().is_none());
    assert!(matches!(
        fixture.db().thread_resume_target(&fixture.thread.id),
        Err(ThreadResumeError::Unbound)
    ));
    // A competing registration for this already-bound ticket cannot replace the
    // owner, even before Codex has supplied its native UUID.
    assert!(matches!(
        fixture.db().bind_native_session_owner(
            &fixture.ticket,
            &ActivationOwner {
                instance_id: new_id(CanonicalIdKind::Instance),
                pid: 303,
                created_at_100ns: 3003,
            },
            now().unwrap()
        ),
        Err(CleanupError::Conflict)
    ));
    fixture.send(
        HarnessKind::Codex,
        port,
        CODEX_NATIVE,
        SessionEventKind::SessionStart,
        Some(SessionStartSource::Startup),
    );
    eventually(|| lifecycle.core.state.lock().unwrap().observed_start);
    let target = fixture
        .db()
        .thread_resume_target(&fixture.thread.id)
        .unwrap();
    assert_eq!(target.segment.harness, "codex");
    assert_eq!(target.segment.native.native_id, CODEX_NATIVE);
    fixture.send(
        HarnessKind::Codex,
        port,
        CODEX_NATIVE,
        SessionEventKind::SessionEnd,
        None,
    );
    finish_with_receipt(&fixture, &lifecycle);
}

#[test]
fn codex_new_session_rejects_resume_source_and_keeps_pending_owner_until_cleanup_receipt() {
    let fixture = Fixture::new(HarnessKind::Codex);
    let (lifecycle, port) = fixture.lifecycle(HarnessKind::Codex, None);
    fixture.send(
        HarnessKind::Codex,
        port,
        CODEX_NATIVE,
        SessionEventKind::SessionStart,
        Some(SessionStartSource::Resume),
    );
    eventually(|| lifecycle.core.failed.load(Ordering::Acquire));
    assert!(fixture.active().is_none());
    assert!(matches!(
        fixture.db().thread_resume_target(&fixture.thread.id),
        Err(ThreadResumeError::Unbound)
    ));
    assert!(
        lifecycle
            .after_cleanup(fixture.identity.instance_id(), fixture.process)
            .is_err()
    );
    fixture.release();
    lifecycle
        .after_cleanup(fixture.identity.instance_id(), fixture.process)
        .unwrap();
    assert!(
        fixture
            .db()
            .native_session_cleanup(fixture.identity.instance_id())
            .unwrap()
            .is_none()
    );
    assert!(fixture.active().is_none());
    assert!(fixture.notifications.lock().unwrap().is_empty());
}

#[test]
fn claude_keeps_its_sealed_uuid_and_requires_the_same_durable_cleanup_barrier() {
    let fixture = Fixture::new(HarnessKind::ClaudeCode);
    let (lifecycle, port) = fixture.lifecycle(HarnessKind::ClaudeCode, Some(CLAUDE_NATIVE));
    let reserved = fixture.active().unwrap();
    assert_eq!(reserved.native.native_id, CLAUDE_NATIVE);
    fixture.send(
        HarnessKind::ClaudeCode,
        port,
        CLAUDE_NATIVE,
        SessionEventKind::SessionStart,
        None,
    );
    eventually(|| lifecycle.core.state.lock().unwrap().observed_start);
    assert_eq!(fixture.active().unwrap().sequence, reserved.sequence);
    fixture.send(
        HarnessKind::ClaudeCode,
        port,
        CLAUDE_NATIVE,
        SessionEventKind::SessionEnd,
        None,
    );
    finish_with_receipt(&fixture, &lifecycle);
}

/// Second, resume-shaped owner over the same saved thread: fresh identity,
/// fresh reservation and PendingThread::Resume from the persisted target.
struct ResumeOwner {
    lifecycle: Arc<ThreadLifecycle>,
    port: u16,
    identity: LaunchIdentity,
    ticket: ReservationTicket,
    process: ProcessKey,
    target: ThreadResumeTarget,
    notifications: Arc<Mutex<Vec<Option<bool>>>>,
}
fn start_resume_owner(fixture: &Fixture) -> ResumeOwner {
    let target = fixture
        .db()
        .thread_resume_target(&fixture.thread.id)
        .unwrap();
    assert!(target.recorded_activation.is_none());
    let identity = LaunchIdentity::generate(&fixture.thread.id).unwrap();
    let at = now().unwrap();
    let revision = fixture
        .db()
        .resource_reservations()
        .unwrap()
        .unwrap()
        .revision;
    let budget = Budget {
        memory_bytes: 1024,
        processes: 1,
        cpu_basis_points: 10,
        queue_bytes: 64,
        queue_items: 1,
    };
    let reserved = fixture
        .db()
        .reserve_resources(
            &fixture.stamp,
            revision,
            &ReservationRequest {
                id: uuid::Uuid::new_v4().to_string(),
                job_key: uuid::Uuid::new_v4().to_string(),
                owner: ReservationOwner {
                    pid: 404,
                    created_at_100ns: 4004,
                    windows_session_id: 1,
                    nonce: uuid::Uuid::new_v4().to_string(),
                },
                budget,
                now_ms: at,
                lease_expires_at_ms: at + 300_000,
                advisory: false,
            },
        )
        .unwrap();
    let ticket = fixture
        .db()
        .activate_reservation(&fixture.stamp, &reserved, at, at + 300_000)
        .unwrap();
    let receiver =
        SessionEventReceiver::start_for(HarnessKind::Codex, identity.record(), &fixture.project)
            .unwrap();
    let port = receiver.port();
    let database = Arc::clone(fixture.db());
    let observer_db = Arc::clone(fixture.db());
    let notifications = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&notifications);
    let expected_project = fixture.thread.project_id.clone();
    let expected_thread = fixture.thread.id.clone();
    let lifecycle = ThreadLifecycle::new(
        database,
        PendingThread::Resume(target.clone()),
        identity.record(),
        receiver,
        Arc::new(move |project, thread| {
            // Same contract as the primary fixture observer: record only the
            // committed ownership state, never control the transition.
            let committed = (project == expected_project && thread == expected_thread)
                .then(|| observer_db.thread_resume_target(thread).ok())
                .flatten()
                .map(|saved| saved.recorded_activation.is_none());
            recorder.lock().unwrap().push(committed);
        }),
    )
    .unwrap();
    let process = ProcessKey {
        pid: 404,
        created_at_100ns: 4004,
    };
    lifecycle
        .before_run(identity.instance_id(), process, &ticket)
        .unwrap();
    ResumeOwner {
        lifecycle,
        port,
        identity,
        ticket,
        process,
        target,
        notifications,
    }
}
fn release_resume(fixture: &Fixture, owner: &ResumeOwner) {
    let at = now().unwrap();
    let proof = EmptyJobProof::from_trusted_closed_empty_job(
        &owner.ticket,
        &owner.ticket.owner,
        &owner.ticket.job_key,
        at,
    )
    .unwrap();
    fixture
        .db()
        .release_reservation(&fixture.stamp, &owner.ticket, &proof, at)
        .unwrap();
}
impl ResumeOwner {
    fn active(&self, fixture: &Fixture) -> Option<ActivationRecord> {
        fixture
            .db()
            .thread_activation(self.identity.instance_id())
            .unwrap()
    }
}

#[test]
fn codex_resume_seals_the_saved_uuid_and_confirms_with_source_resume() {
    let fixture = Fixture::new(HarnessKind::Codex);
    // Phase 1: the original StartNew owner records the native UUID and closes.
    let (first, port) = fixture.lifecycle(HarnessKind::Codex, None);
    fixture.send(
        HarnessKind::Codex,
        port,
        CODEX_NATIVE,
        SessionEventKind::SessionStart,
        Some(SessionStartSource::Startup),
    );
    eventually(|| first.core.state.lock().unwrap().observed_start);
    finish_with_receipt(&fixture, &first);
    let sealed = fixture.active().unwrap();
    assert!(sealed.ended_at.is_some());
    // Phase 2: Resume seals the exact saved UUID before the shell resumes.
    let owner = start_resume_owner(&fixture);
    assert_eq!(owner.target.segment.native.native_id, CODEX_NATIVE);
    let reserved = owner.active(&fixture).unwrap();
    assert_eq!(reserved.native.native_id, CODEX_NATIVE);
    assert_eq!(reserved.sequence, sealed.sequence + 1);
    assert!(reserved.ended_at.is_none());
    // The authenticated resume confirmation must carry source=resume and the
    // exact sealed UUID; it never creates another segment or activation.
    fixture.send_as(
        HarnessKind::Codex,
        owner.port,
        &owner.identity,
        CODEX_NATIVE,
        SessionEventKind::SessionStart,
        Some(SessionStartSource::Resume),
    );
    eventually(|| owner.lifecycle.core.state.lock().unwrap().observed_start);
    let confirmed = owner.active(&fixture).unwrap();
    assert_eq!(confirmed.sequence, reserved.sequence);
    assert_eq!(confirmed.native.native_id, CODEX_NATIVE);
    assert!(confirmed.ended_at.is_none());
    assert!(
        fixture
            .db()
            .thread_resume_target(&fixture.thread.id)
            .unwrap()
            .recorded_activation
            .is_some()
    );
    assert!(!owner.lifecycle.core.failed.load(Ordering::Acquire));
    // The same durable barrier closes the resumed owner: no receipt, no close.
    assert!(
        owner
            .lifecycle
            .after_cleanup(owner.identity.instance_id(), owner.process)
            .is_err()
    );
    release_resume(&fixture, &owner);
    owner
        .lifecycle
        .after_cleanup(owner.identity.instance_id(), owner.process)
        .unwrap();
    let ended = owner.active(&fixture).unwrap();
    assert_eq!(ended.sequence, reserved.sequence);
    assert!(ended.ended_at.is_some());
    assert!(
        fixture
            .db()
            .native_session_cleanup(owner.identity.instance_id())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        *owner.notifications.lock().unwrap(),
        vec![Some(false), Some(true)]
    );
    assert!(matches!(
        fixture.db().thread_resume_target(&fixture.thread.id),
        Ok(target) if target.recorded_activation.is_none()
    ));
}

#[test]
fn codex_resume_rejects_wrong_source_or_uuid_and_retains_the_sealed_owner() {
    const OTHER_NATIVE: &str = "018fba11-45db-7f1e-89c8-40d5c6be9999";
    for (source, session, label) in [
        (
            Some(SessionStartSource::Startup),
            CODEX_NATIVE,
            "startup-source",
        ),
        (
            Some(SessionStartSource::Resume),
            OTHER_NATIVE,
            "foreign-uuid",
        ),
    ] {
        let fixture = Fixture::new(HarnessKind::Codex);
        let (first, port) = fixture.lifecycle(HarnessKind::Codex, None);
        fixture.send(
            HarnessKind::Codex,
            port,
            CODEX_NATIVE,
            SessionEventKind::SessionStart,
            Some(SessionStartSource::Startup),
        );
        eventually(|| first.core.state.lock().unwrap().observed_start);
        finish_with_receipt(&fixture, &first);
        let owner = start_resume_owner(&fixture);
        let sealed = owner.active(&fixture).unwrap();
        assert!(sealed.ended_at.is_none());
        fixture.send_as(
            HarnessKind::Codex,
            owner.port,
            &owner.identity,
            session,
            SessionEventKind::SessionStart,
            source,
        );
        eventually(|| owner.lifecycle.core.failed.load(Ordering::Acquire));
        // The sealed activation stays open and owned; a failed confirmation
        // never closes history, and cleanup stays retained for recovery.
        release_resume(&fixture, &owner);
        assert!(
            owner
                .lifecycle
                .after_cleanup(owner.identity.instance_id(), owner.process)
                .is_err(),
            "{label}: failed tracking must not finalize"
        );
        let retained = owner.active(&fixture).unwrap();
        assert_eq!(retained.sequence, sealed.sequence, "{label}");
        assert!(retained.ended_at.is_none(), "{label}");
        assert!(
            fixture
                .db()
                .native_session_cleanup(owner.identity.instance_id())
                .unwrap()
                .is_some(),
            "{label}: the cleanup receipt must remain retained"
        );
        assert_eq!(
            owner.active(&fixture).unwrap().native.native_id,
            CODEX_NATIVE,
            "{label}: a foreign UUID must never be recorded"
        );
    }
}
