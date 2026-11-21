//! Shared trusted harness ownership; hook observations never prove native cleanup.
#[cfg(test)]
mod tests;
use super::{ThreadObserver, notify_thread_changed};
use shinatama_engine::{
    harness_session_events::{
        HarnessKind, ReadError, SessionEvent, SessionEventKind, SessionEventReceiver,
        SessionStartSource,
    },
    launch::identity::LaunchIdentityRecord,
    resources::sampling::ProcessKey,
};
use shinatama_state::{
    database::Database,
    records::{CanonicalIdKind, NativeSessionKey, StoreRootRecord, new_id, validate_id},
    resource_reservations::ReservationTicket,
    threads::{
        ActivationOwner, ActivationRecord, NativeSessionStart, SegmentRecord, ThreadError,
        ThreadRecord, ThreadResumeError, ThreadResumeTarget,
    },
};
use shinatama_terminal::{TerminalResult, WindowsNativeSessionLifecycle};
use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, Thread},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, &'static str>;
const TRACKING: &str = "Thread tracking is incomplete; its saved activation remains reserved.";
const BINDING: &str = "The exact thread, native session or process binding changed.";
/// The store refused a native session another thread holds; it wrote nothing.
const HELD_ELSEWHERE: &str = "The native session belongs to another thread.";
const WAIT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(10);

/// Rust-only inputs from the sealed launch recipe, never UI or hook selectors.
pub(super) enum PendingThread {
    Start {
        thread: ThreadRecord,
        root: StoreRootRecord,
        /// Claude seals its UUID before launch; Codex reports its own UUID in
        /// the first authenticated SessionStart. Never manufacture that ID.
        native_id: Option<String>,
    },
    Resume(ThreadResumeTarget),
}
struct State {
    thread: ThreadRecord,
    owner: Option<ActivationOwner>,
    activation: Option<ActivationRecord>,
    attempted: bool,
    observed_start: bool,
    owner_bound: bool,
    ack_attempted: bool,
    complete: bool,
}
struct Core {
    database: Arc<Database>,
    identity: LaunchIdentityRecord,
    root: StoreRootRecord,
    requested: Option<String>,
    harness: HarnessKind,
    resume: Option<SegmentRecord>,
    process: OnceLock<ProcessKey>,
    reservation: OnceLock<ReservationTicket>,
    state: Mutex<State>,
    stop: AtomicBool,
    failed: AtomicBool,
    done: AtomicBool,
    deadline: Mutex<Option<Instant>>,
    thread_observer: ThreadObserver,
}
pub(super) struct ThreadLifecycle {
    core: Arc<Core>,
    receiver: Mutex<Option<SessionEventReceiver>>,
    worker: Mutex<Option<Thread>>,
}
impl ThreadLifecycle {
    pub(super) fn new(
        database: Arc<Database>,
        pending: PendingThread,
        identity: LaunchIdentityRecord,
        receiver: SessionEventReceiver,
        thread_observer: ThreadObserver,
    ) -> Result<Arc<Self>> {
        let (thread, root, requested, resume) = match pending {
            PendingThread::Start {
                thread,
                root,
                native_id,
            } => (thread, root, native_id, None),
            PendingThread::Resume(target) => {
                if target.recorded_activation.is_some()
                    || !target.segment.native.matches_root(&target.root)
                    || target.segment.harness != target.root.harness
                {
                    return Err(BINDING);
                }
                (
                    target.thread,
                    target.root,
                    Some(target.segment.native.native_id.clone()),
                    Some(target.segment),
                )
            }
        };
        let harness = receiver.harness();
        if let Some(requested) = &requested {
            let uuid = uuid::Uuid::parse_str(requested).map_err(|_| BINDING)?;
            if uuid.hyphenated().to_string() != *requested
                || uuid.get_variant() != uuid::Variant::RFC4122
                || uuid.get_version().is_none()
            {
                return Err(BINDING);
            }
        } else if harness != HarnessKind::Codex || resume.is_some() {
            return Err(BINDING);
        }
        if thread.id != identity.thread_id
            || thread.archived
            || root.harness != harness.id()
            || identity.ended_at_utc_ms.is_some()
            || identity.token_sha256 == [0; 32]
        {
            return Err(BINDING);
        }
        root.validate().map_err(|_| BINDING)?;
        validate_id(&identity.instance_id, CanonicalIdKind::Instance).map_err(|_| BINDING)?;
        if identity.started_at_utc_ms < 0 {
            return Err(BINDING);
        }
        Ok(Arc::new(Self {
            core: Arc::new(Core {
                database,
                identity,
                root,
                requested,
                harness,
                resume,
                process: OnceLock::new(),
                reservation: OnceLock::new(),
                state: Mutex::new(State {
                    thread,
                    owner: None,
                    activation: None,
                    attempted: false,
                    observed_start: false,
                    owner_bound: false,
                    ack_attempted: false,
                    complete: false,
                }),
                stop: AtomicBool::new(false),
                failed: AtomicBool::new(false),
                done: AtomicBool::new(true),
                deadline: Mutex::new(None),
                thread_observer,
            }),
            receiver: Mutex::new(Some(receiver)),
            worker: Mutex::new(None),
        }))
    }
    fn prepare(
        &self,
        instance: &str,
        process: ProcessKey,
        reservation: &ReservationTicket,
    ) -> Result<()> {
        let core = &self.core;
        if instance != core.identity.instance_id
            || process.pid == 0
            || process.created_at_100ns == 0
        {
            return Err(BINDING);
        }
        core.process.set(process).map_err(|_| BINDING)?;
        core.reservation
            .set(reservation.clone())
            .map_err(|_| BINDING)?;
        let mut state = core.state.lock().map_err(|_| TRACKING)?;
        if state.owner.is_some() {
            return Err(BINDING);
        }
        state.owner = Some(ActivationOwner {
            instance_id: instance.into(),
            pid: process.pid,
            created_at_100ns: process.created_at_100ns,
        });
        if core
            .database
            .store_root(&core.root.id)
            .map_err(|_| TRACKING)?
            .as_ref()
            != Some(&core.root)
        {
            return Err(BINDING);
        }
        initial_target(core, &state.thread.id)?;
        // Persist the exact host/job/shell association while the shell is still
        // suspended. A crash after cleanup can then be distinguished from a
        // missing ticket; no authority is inferred from the UI or a hook.
        core.database
            .bind_native_session_owner(reservation, state.owner.as_ref().ok_or(BINDING)?, now()?)
            .map_err(|_| TRACKING)?;
        state.owner_bound = true;
        // Reserve an explicitly sealed UUID before ResumeShell. A fresh Codex
        // launch has no native ID yet: only its exact owner is recorded now.
        if let Some(requested) = &core.requested
            && apply(core, &mut state, requested).is_err()
        {
            core.failed.store(true, Ordering::Release);
            return Err(TRACKING);
        }
        drop(state);
        let receiver = self
            .receiver
            .lock()
            .map_err(|_| TRACKING)?
            .take()
            .ok_or(TRACKING)?;
        core.done.store(false, Ordering::Release);
        let shared = Arc::clone(core);
        let spawned = thread::Builder::new()
            .name("harness-thread-events".into())
            .spawn(move || {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    consume(&shared, receiver)
                }))
                .is_err()
                {
                    shared.failed.store(true, Ordering::Release);
                }
                shared.done.store(true, Ordering::Release);
            });
        match spawned {
            Ok(worker) => {
                *self.worker.lock().map_err(|_| TRACKING)? = Some(worker.thread().clone());
                Ok(())
            }
            Err(_) => {
                core.failed.store(true, Ordering::Release);
                core.done.store(true, Ordering::Release);
                Err(TRACKING)
            }
        }
    }
    fn finish(&self, instance: &str, process: ProcessKey) -> Result<()> {
        let core = &self.core;
        if instance != core.identity.instance_id || core.process.get() != Some(&process) {
            return Err(BINDING);
        }
        let deadline = Instant::now() + WAIT;
        core.deadline
            .lock()
            .map_err(|_| TRACKING)?
            .get_or_insert(deadline);
        core.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.lock().map_err(|_| TRACKING)?.as_ref() {
            worker.unpark();
        }
        // A failed before_run can leave the receiver here, without a consumer.
        if let Some(receiver) = self.receiver.lock().map_err(|_| TRACKING)?.take() {
            receiver.stop();
            while !receiver.status().stopped && Instant::now() < deadline {
                thread::sleep(POLL);
            }
            if !receiver.status().stopped
                || receiver.status().failed
                || receiver.status().queue_overflowed
                || matches!(receiver.try_next(), Ok(Some(_)))
            {
                core.failed.store(true, Ordering::Release);
            }
        }
        // Never wait on the state mutex while a worker may be inside SQLite.
        // An in-flight local transaction cannot be cancelled; retain ownership
        // on timeout and allow the terminal owner to retry finalization later.
        while !core.done.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(POLL);
        }
        if !core.done.load(Ordering::Acquire) {
            return Err(TRACKING);
        }
        let mut state = core.state.lock().map_err(|_| TRACKING)?;
        let owner = state.owner.as_ref().ok_or(BINDING)?.clone();
        if owner.pid != process.pid || owner.created_at_100ns != process.created_at_100ns {
            return Err(BINDING);
        }
        if state.complete {
            return Ok(());
        }
        let receipt = core
            .database
            .native_session_cleanup(instance)
            .map_err(|_| TRACKING)?;
        if let Some(receipt) = &receipt
            && (receipt.owner.activation != owner
                || core.reservation.get() != Some(&receipt.owner.reservation))
        {
            return Err(BINDING);
        }
        let Some(active) = state.activation.clone() else {
            if state.attempted
                && core
                    .database
                    .thread_activation(instance)
                    .map_err(|_| TRACKING)?
                    .is_some()
            {
                return Err(TRACKING);
            }
            if state.owner_bound && receipt.is_none() {
                // Codex may exit before reporting its UUID. Its saved job join
                // still needs a cleanup receipt. Absence is acceptable only
                // when retrying our own uncertain acknowledgement commit.
                if !state.ack_attempted
                    || core
                        .database
                        .native_session_owner(instance)
                        .map_err(|_| TRACKING)?
                        .is_some()
                {
                    return Err(TRACKING);
                }
            }
            if let Some(receipt) = &receipt {
                state.ack_attempted = true;
                core.database
                    .acknowledge_native_session_cleanup(receipt)
                    .map_err(|_| TRACKING)?;
            }
            state.complete = true;
            return Ok(());
        };
        if core.failed.load(Ordering::Acquire) {
            return Err(TRACKING);
        }
        let at = now()?
            .max(active.started_at)
            .max(receipt.as_ref().map_or(0, |value| value.released_at_ms));
        let ended = match receipt.as_ref().ok_or(TRACKING).and_then(|receipt| {
            core.database
                .finish_native_session_cleanup(receipt, &active, at)
                .map_err(|_| TRACKING)
        }) {
            Ok(ended) => ended,
            Err(_) => {
                // A previous commit may have succeeded before its receipt was
                // lost. Accept only that exact interval's single close update.
                let ended = core
                    .database
                    .thread_activation(instance)
                    .map_err(|_| TRACKING)?
                    .ok_or(TRACKING)?;
                if ended.owner != owner
                    || ended.sequence != active.sequence
                    || ended.native != active.native
                    || ended.thread_id != active.thread_id
                    || ended.segment_id != active.segment_id
                    || ended.started_at != active.started_at
                    || Some(ended.revision) != active.revision.checked_add(1)
                    || ended.ended_at.is_none()
                {
                    return Err(TRACKING);
                }
                ended
            }
        };
        state.activation = Some(ended);
        state.complete = true;
        notify_thread_changed(
            &core.thread_observer,
            &state.thread.project_id,
            &state.thread.id,
        );
        Ok(())
    }
}
impl WindowsNativeSessionLifecycle for ThreadLifecycle {
    fn before_run(
        &self,
        instance_id: &str,
        process: ProcessKey,
        reservation: &ReservationTicket,
    ) -> TerminalResult<()> {
        self.prepare(instance_id, process, reservation)
            .map_err(anyhow::Error::msg)
    }
    fn after_cleanup(&self, instance_id: &str, process: ProcessKey) -> TerminalResult<()> {
        self.finish(instance_id, process)
            .map_err(anyhow::Error::msg)
    }
}
impl Drop for ThreadLifecycle {
    fn drop(&mut self) {
        self.core.stop.store(true, Ordering::Release);
        if let Ok(worker) = self.worker.get_mut()
            && let Some(worker) = worker
        {
            worker.unpark();
        }
        // Drop stops the receiver/consumer, but never closes a saved activation.
    }
}
/// OpenCode and Pi report no session events: the launcher chooses the native id and passes it
/// to the program, so it is recorded as the thread's first segment before the program starts
/// (as Claude's sealed UUID is). It is a name, not a claim that the session exists or can be
/// resumed: those two harnesses declare no native resume, and nothing here opens an activation.
pub(super) fn record_chosen_native_session(
    database: &Database,
    thread: &ThreadRecord,
    root: &StoreRootRecord,
    native_id: &str,
) -> Result<()> {
    database
        .attach_thread_segment(
            &SegmentRecord {
                id: new_id(CanonicalIdKind::Segment),
                thread_id: thread.id.clone(),
                ordinal: 1,
                harness: root.harness.clone(),
                native: NativeSessionKey {
                    store_root_id: root.id.clone(),
                    store_revision: root.config_revision,
                    native_id: native_id.into(),
                },
                archived: false,
                revision: 1,
                created_at: now()?.max(thread.created_at),
            },
            thread.revision,
        )
        .map(|_| ())
        .map_err(|_| TRACKING)
}
fn now() -> Result<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
        .ok_or(TRACKING)
}
fn same_thread_except_title(previous: &ThreadRecord, fresh: &ThreadRecord) -> bool {
    let mut expected = previous.clone();
    expected.title = fresh.title.clone();
    expected.revision = fresh.revision;
    expected.updated_at = fresh.updated_at;
    expected == *fresh && !fresh.archived
}
fn initial_target(core: &Core, thread: &str) -> Result<()> {
    let target = core.database.thread_resume_target(thread);
    match (&core.resume, target) {
        (Some(expected), Ok(target))
            if target.recorded_activation.is_none()
                && target.root == core.root
                && target.segment == *expected =>
        {
            Ok(())
        }
        (None, Err(ThreadResumeError::Unbound)) => Ok(()),
        _ => Err(BINDING),
    }
}
fn apply(core: &Core, state: &mut State, session: &str) -> Result<()> {
    state.attempted = true;
    let native = NativeSessionKey {
        store_root_id: core.root.id.clone(),
        store_revision: core.root.config_revision,
        native_id: session.into(),
    };
    let candidate = new_id(CanonicalIdKind::Segment);
    let owner = state.owner.as_ref().ok_or(BINDING)?;
    let at = now()?.max(state.thread.updated_at).max(
        state
            .activation
            .as_ref()
            .map_or(0, |active| active.started_at),
    );
    for attempt in 0..2 {
        let result = core
            .database
            .observe_native_session_start(&NativeSessionStart {
                thread_id: &state.thread.id,
                expected_thread_revision: state.thread.revision,
                candidate_segment_id: &candidate,
                harness: core.harness.id(),
                native: &native,
                owner,
                expected_activation: state.activation.as_ref().map(ActivationRecord::version),
                observed_at: at,
            });
        match result {
            Ok(value) => {
                state.thread = value.thread;
                state.activation = Some(value.activation);
                if value.changed {
                    notify_thread_changed(
                        &core.thread_observer,
                        &state.thread.project_id,
                        &state.thread.id,
                    );
                }
                return Ok(());
            }
            Err(ThreadError::Conflict) if attempt == 0 => {
                let fresh = core
                    .database
                    .thread_record(&state.thread.id)
                    .map_err(|_| TRACKING)?
                    .ok_or(BINDING)?;
                let active = core
                    .database
                    .thread_activation(&owner.instance_id)
                    .map_err(|_| TRACKING)?;
                if !same_thread_except_title(&state.thread, &fresh) || active != state.activation {
                    return Err(BINDING);
                }
                if state.activation.is_none() {
                    initial_target(core, &state.thread.id)?;
                }
                state.thread = fresh;
            }
            Err(ThreadError::OwnedElsewhere) => return Err(HELD_ELSEWHERE),
            Err(_) => return Err(TRACKING),
        }
    }
    Err(TRACKING)
}
fn observe_start(core: &Core, state: &mut State, event: &SessionEvent) -> Result<()> {
    if event.kind != SessionEventKind::SessionStart
        || event.instance_id != core.identity.instance_id
        || event.thread_id != core.identity.thread_id
    {
        return Err(BINDING);
    }
    if core.harness == HarnessKind::Codex && !state.observed_start {
        let expected = if core.resume.is_some() {
            SessionStartSource::Resume
        } else {
            SessionStartSource::Startup
        };
        if event.source != Some(expected)
            || core
                .requested
                .as_ref()
                .is_some_and(|id| id != &event.session_id)
        {
            return Err(BINDING);
        }
    }
    match apply(core, state, &event.session_id) {
        // After the confirmed start, the harness switched to a conversation another thread
        // holds (Claude's own /resume). The refusal wrote nothing: this thread keeps its exact
        // interval, which its process's cleanup still ends. Following the switch is not done.
        Err(HELD_ELSEWHERE) if state.observed_start => return Ok(()),
        Err(error) => return Err(error),
        Ok(()) => {}
    }
    state.observed_start = true;
    Ok(())
}
fn consume(core: &Core, receiver: SessionEventReceiver) {
    loop {
        let stopping = core.stop.load(Ordering::Acquire);
        if stopping {
            receiver.stop();
        }
        let status = receiver.status();
        if status.failed || status.queue_overflowed {
            core.failed.store(true, Ordering::Release);
            receiver.stop();
        }
        match receiver.try_next() {
            Ok(Some(event)) => {
                let expired = core.deadline.lock().map_or(true, |deadline| {
                    deadline.is_some_and(|end| Instant::now() >= end)
                });
                if expired
                    || core.failed.load(Ordering::Acquire)
                    || event.instance_id != core.identity.instance_id
                    || event.thread_id != core.identity.thread_id
                {
                    core.failed.store(true, Ordering::Release);
                    receiver.stop();
                } else if event.kind == SessionEventKind::SessionStart {
                    let result = core
                        .state
                        .lock()
                        .map_err(|_| TRACKING)
                        .and_then(|mut state| observe_start(core, &mut state, &event));
                    if result.is_err() {
                        core.failed.store(true, Ordering::Release);
                        receiver.stop();
                    }
                }
                // SessionEnd describes the harness, never process/job cleanup.
            }
            Ok(None) => {}
            Err(ReadError::Stopped) => {
                let final_status = receiver.status();
                if final_status.failed || final_status.queue_overflowed {
                    core.failed.store(true, Ordering::Release);
                }
                if final_status.stopped {
                    return;
                }
            }
            Err(ReadError::WorkerUnavailable) => {
                core.failed.store(true, Ordering::Release);
                return;
            }
        }
        thread::park_timeout(POLL);
    }
}
