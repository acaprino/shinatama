//! Trusted preparation only; no agent execution or product Start/Resume gate.
//! All methods touching SQLite/native jobs run on one bounded owner worker, never
//! the UI thread. Drops close owned resources but perform no SQL or cleanup wait.
#![cfg(windows)]

use super::job::{
    CleanupOutcome, ClosedEmptyJobProof, JobBudget, JobError, JobSnapshot, MAX_CLEANUP_WAIT,
    OwnedJobKey, SlotState, StartupJobBinding, WorkloadJob, WorkloadSlot,
};
use super::sampling::{ProcessKey, WindowsSampler};
use shinatama_state::database::Database;
use shinatama_state::resource_reservations::{
    Budget, EmptyJobProof, MAX_SLOTS, PolicyStamp, ReservationError, ReservationOwner,
    ReservationRequest, ReservationState, ReservationTicket,
};
use std::os::windows::io::BorrowedHandle;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const FREE: u8 = 0;
const OCCUPIED: u8 = 1;
const RETIRED: u8 = 2;
const CAS_ATTEMPTS: usize = 8;
static POOL_STATE: AtomicU8 = AtomicU8::new(FREE);

#[derive(Debug)]
pub enum AdmissionError {
    InvalidRequest,
    EntropyUnavailable,
    ClockUnavailable,
    PoolBusy,
    PoolRetired,
    LocalCapacity,
    AdmissionClosed,
    BindingMismatch,
    Job(JobError),
    Ledger(ReservationError),
}
impl From<JobError> for AdmissionError {
    fn from(value: JobError) -> Self {
        Self::Job(value)
    }
}
impl From<ReservationError> for AdmissionError {
    fn from(value: ReservationError) -> Self {
        Self::Ledger(value)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PrepareRequest {
    pub budget: Budget,
    pub process_commit_bytes: u64,
    pub now_ms: i64,
    pub lease_expires_at_ms: i64,
}
impl PrepareRequest {
    pub fn native_budget(self) -> Result<JobBudget, AdmissionError> {
        if self.now_ms < 0 || self.lease_expires_at_ms <= self.now_ms {
            return Err(AdmissionError::InvalidRequest);
        }
        Ok(JobBudget::new(
            self.budget.memory_bytes,
            self.process_commit_bytes,
            self.budget.processes,
            self.budget.cpu_basis_points,
        )?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueError {
    Closed,
    Capacity,
    Contended,
    Invalid,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueStatus {
    pub closed: bool,
    pub current_bytes: u64,
    pub current_items: u32,
    pub peak_bytes: u64,
    pub peak_items: u32,
    pub refusals: u64,
}
struct QueueState {
    byte_limit: u64,
    item_limit: u32,
    closed: AtomicBool,
    bytes: AtomicU64,
    items: AtomicU32,
    peak_bytes: AtomicU64,
    peak_items: AtomicU32,
    refusals: AtomicU64,
}
/// The queue implementation must hold each guard until its items and allocated
/// bytes are removed. Clones share one ceiling; no payloads are stored here.
#[derive(Clone)]
pub struct QueueCredits(Arc<QueueState>);
pub struct QueueCredit {
    queue: Arc<QueueState>,
    bytes: u64,
    items: u32,
}
impl QueueCredits {
    fn new(bytes: u64, items: u32) -> Self {
        Self(Arc::new(QueueState {
            byte_limit: bytes,
            item_limit: items,
            closed: AtomicBool::new(false),
            bytes: AtomicU64::new(0),
            items: AtomicU32::new(0),
            peak_bytes: AtomicU64::new(0),
            peak_items: AtomicU32::new(0),
            refusals: AtomicU64::new(0),
        }))
    }
    /// Finite CAS attempts; contention refuses work instead of waiting or queuing.
    pub fn acquire(&self, bytes: u64, items: u32) -> Result<QueueCredit, QueueError> {
        let result = self.acquire_inner(bytes, items);
        if result.is_err() {
            self.0.refusals.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
    fn acquire_inner(&self, bytes: u64, items: u32) -> Result<QueueCredit, QueueError> {
        if self.0.closed.load(Ordering::SeqCst) {
            return Err(QueueError::Closed);
        }
        if items == 0 {
            return Err(QueueError::Invalid);
        }
        let current_bytes = reserve_bytes(&self.0.bytes, bytes, self.0.byte_limit)?;
        let current_items = match reserve_items(&self.0.items, items, self.0.item_limit) {
            Ok(current) => current,
            Err(error) => {
                self.0.bytes.fetch_sub(bytes, Ordering::AcqRel);
                return Err(error);
            }
        };
        if self.0.closed.load(Ordering::SeqCst) {
            self.0.items.fetch_sub(items, Ordering::AcqRel);
            self.0.bytes.fetch_sub(bytes, Ordering::AcqRel);
            return Err(QueueError::Closed);
        }
        self.0
            .peak_bytes
            .fetch_max(current_bytes, Ordering::Relaxed);
        self.0
            .peak_items
            .fetch_max(current_items, Ordering::Relaxed);
        Ok(QueueCredit {
            queue: Arc::clone(&self.0),
            bytes,
            items,
        })
    }
    pub fn close(&self) {
        self.0.closed.store(true, Ordering::SeqCst);
    }
    pub fn status(&self) -> QueueStatus {
        QueueStatus {
            closed: self.0.closed.load(Ordering::SeqCst),
            current_bytes: self.0.bytes.load(Ordering::Acquire),
            current_items: self.0.items.load(Ordering::Acquire),
            peak_bytes: self.0.peak_bytes.load(Ordering::Relaxed),
            peak_items: self.0.peak_items.load(Ordering::Relaxed),
            refusals: self.0.refusals.load(Ordering::Relaxed),
        }
    }
    fn returned(&self) -> bool {
        let state = self.status();
        state.closed && state.current_items == 0 && state.current_bytes == 0
    }
}
impl Drop for QueueCredit {
    fn drop(&mut self) {
        self.queue.items.fetch_sub(self.items, Ordering::AcqRel);
        self.queue.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
fn reserve_bytes(counter: &AtomicU64, amount: u64, limit: u64) -> Result<u64, QueueError> {
    let mut current = counter.load(Ordering::Acquire);
    for _ in 0..CAS_ATTEMPTS {
        let next = current
            .checked_add(amount)
            .filter(|next| *next <= limit)
            .ok_or(QueueError::Capacity)?;
        match counter.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Ok(next),
            Err(observed) => current = observed,
        }
    }
    Err(QueueError::Contended)
}
fn reserve_items(counter: &AtomicU32, amount: u32, limit: u32) -> Result<u32, QueueError> {
    let mut current = counter.load(Ordering::Acquire);
    for _ in 0..CAS_ATTEMPTS {
        let next = current
            .checked_add(amount)
            .filter(|next| *next <= limit)
            .ok_or(QueueError::Capacity)?;
        match counter.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Ok(next),
            Err(observed) => current = observed,
        }
    }
    Err(QueueError::Contended)
}

struct LocalSlot {
    state: AtomicU8,
    native: WorkloadSlot,
}
struct Pool {
    slots: Box<[LocalSlot]>,
    nonce: Uuid,
}
impl Drop for Pool {
    fn drop(&mut self) {
        let retired = self.slots.iter().any(|slot| {
            slot.state.load(Ordering::Acquire) != FREE || slot.native.state() != SlotState::Free
        });
        POOL_STATE.store(if retired { RETIRED } else { FREE }, Ordering::Release);
    }
}
/// One fixed process-wide pool. A dropped retired pool cannot be replaced with a
/// fresh free pool; outstanding workloads retain the pool even if its owner drops.
pub struct AdmissionCoordinator {
    pool: Arc<Pool>,
}
impl AdmissionCoordinator {
    pub fn new(slot_count: usize) -> Result<Self, AdmissionError> {
        if !(1..=MAX_SLOTS).contains(&slot_count) {
            return Err(AdmissionError::InvalidRequest);
        }
        let nonce = fresh_uuid()?;
        POOL_STATE
            .compare_exchange(FREE, OCCUPIED, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|state| {
                if state == RETIRED {
                    AdmissionError::PoolRetired
                } else {
                    AdmissionError::PoolBusy
                }
            })?;
        let slots = (0..slot_count)
            .map(|_| LocalSlot {
                state: AtomicU8::new(FREE),
                native: WorkloadSlot::new(),
            })
            .collect();
        Ok(Self {
            pool: Arc::new(Pool { slots, nonce }),
        })
    }
    /// Blocking native/SQLite work. Policy installation/calibration is separate;
    /// this method never raises a ceiling or retries a stale ledger revision.
    pub fn prepare(
        &self,
        database: &Database,
        stamp: &PolicyStamp,
        expected_revision: u64,
        request: PrepareRequest,
    ) -> Result<PreparedWorkload, AdmissionError> {
        let native_budget = request.native_budget()?;
        let id = fresh_uuid()?;
        let job_key = fresh_uuid()?;
        let index = self
            .pool
            .slots
            .iter()
            .position(|slot| {
                slot.state
                    .compare_exchange(FREE, OCCUPIED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            })
            .ok_or(AdmissionError::LocalCapacity)?;
        let slot = &self.pool.slots[index];
        let mut job = match WorkloadJob::create(&slot.native, *job_key.as_bytes(), native_budget) {
            Ok(job) => job,
            Err(error) => {
                slot.state.store(
                    if slot.native.state() == SlotState::Free {
                        FREE
                    } else {
                        RETIRED
                    },
                    Ordering::Release,
                );
                return Err(error.into());
            }
        };
        let owned = job.owned_key();
        let reservation = ReservationRequest {
            id: id.to_string(),
            job_key: job_key.to_string(),
            owner: ReservationOwner {
                pid: owned.owner.pid,
                created_at_100ns: owned.owner.created_at_100ns,
                windows_session_id: owned.owner_windows_session_id,
                nonce: self.pool.nonce.to_string(),
            },
            budget: request.budget,
            now_ms: request.now_ms,
            lease_expires_at_ms: request.lease_expires_at_ms,
        };
        let ticket = match database.reserve_resources(stamp, expected_revision, &reservation) {
            Ok(ticket) => ticket,
            Err(error) => {
                // No binding has escaped, so the job cannot contain a child. Still
                // require the native empty+close proof before recycling the slot.
                let empty = matches!(
                    job.terminate_and_wait(Duration::ZERO),
                    Ok(CleanupOutcome::EmptyConfirmed)
                );
                let closed = empty && job.close_after_empty().is_ok();
                slot.state
                    .store(if closed { FREE } else { RETIRED }, Ordering::Release);
                return Err(error.into());
            }
        };
        Ok(PreparedWorkload {
            pool: Arc::clone(&self.pool),
            index,
            job: Some(job),
            closed: None,
            owned,
            stamp: stamp.clone(),
            ticket,
            queues: QueueCredits::new(request.budget.queue_bytes, request.budget.queue_items),
            phase: WorkloadPhase::Prepared,
            prepared_at_ms: request.now_ms,
            released_revision: 0,
        })
    }
    pub fn retired_slots(&self) -> usize {
        self.pool
            .slots
            .iter()
            .filter(|slot| slot.state.load(Ordering::Acquire) == RETIRED)
            .count()
    }
}
fn fresh_uuid() -> Result<Uuid, AdmissionError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| AdmissionError::EntropyUnavailable)?;
    Ok(uuid::Builder::from_random_bytes(bytes).into_uuid())
}
fn now_ms() -> Result<i64, AdmissionError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AdmissionError::ClockUnavailable)?
            .as_millis(),
    )
    .map_err(|_| AdmissionError::ClockUnavailable)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkloadPhase {
    Prepared,
    Active,
    Closing,
    Retired,
    Released,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupStatus {
    Released { ledger_revision: u64 },
    AwaitingCredits(QueueStatus),
    Retired,
}
pub struct PreparedWorkload {
    pool: Arc<Pool>,
    index: usize,
    job: Option<WorkloadJob>,
    closed: Option<ClosedEmptyJobProof>,
    owned: OwnedJobKey,
    stamp: PolicyStamp,
    ticket: ReservationTicket,
    queues: QueueCredits,
    phase: WorkloadPhase,
    prepared_at_ms: i64,
    released_revision: u64,
}
impl PreparedWorkload {
    pub fn ticket(&self) -> &ReservationTicket {
        &self.ticket
    }
    /// Verified native limits; callers may cache these before owned-job cleanup.
    pub fn applied_budget(&self) -> Option<JobBudget> {
        self.job.as_ref().map(WorkloadJob::budget)
    }
    pub fn owned_key(&self) -> OwnedJobKey {
        self.owned
    }
    pub fn phase(&self) -> WorkloadPhase {
        self.phase
    }
    pub fn queue_credits(&self) -> QueueCredits {
        self.queues.clone()
    }
    pub fn startup_binding(&self, now_ms: i64) -> Result<StartupJobBinding<'_>, AdmissionError> {
        if self.phase != WorkloadPhase::Prepared
            || self.ticket.state != ReservationState::Reserved
            || now_ms < self.prepared_at_ms
            || now_ms >= self.ticket.lease_expires_at_ms
        {
            return Err(AdmissionError::AdmissionClosed);
        }
        Ok(self
            .job
            .as_ref()
            .ok_or(AdmissionError::AdmissionClosed)?
            .startup_binding()?)
    }
    /// Verify the exact suspended child before activation. The future executor
    /// must not resume it unless this succeeds. This method never starts/resumes.
    pub fn activate_verified(
        &mut self,
        database: &Database,
        child: BorrowedHandle<'_>,
        now_ms: i64,
        lease_expires_at_ms: i64,
    ) -> Result<ProcessKey, AdmissionError> {
        let key = self.startup_binding(now_ms)?.verify_membership(child)?;
        self.ticket = database.activate_reservation(
            &self.stamp,
            &self.ticket,
            now_ms,
            lease_expires_at_ms,
        )?;
        self.phase = WorkloadPhase::Active;
        Ok(key)
    }
    pub fn sample(&self, sampler: &WindowsSampler) -> Result<JobSnapshot, AdmissionError> {
        Ok(self
            .job
            .as_ref()
            .ok_or(AdmissionError::AdmissionClosed)?
            .query(sampler)?)
    }
    /// Trusted owner-worker only, after the final verified launch has closed its
    /// startup attribute lists and temporary ASSIGN handles. No IPC caller may
    /// assert this barrier; subsequent startup_binding calls are refused forever.
    pub fn seal_assignments(&mut self) -> Result<(), AdmissionError> {
        if self.phase != WorkloadPhase::Active {
            return Err(AdmissionError::AdmissionClosed);
        }
        Ok(self
            .job
            .as_mut()
            .ok_or(AdmissionError::AdmissionClosed)?
            .seal_assignments()?)
    }
    /// Single owner worker only. Keep the same active ticket/owner/budget;
    /// replace the local CAS ticket only after the persisted renewal succeeds.
    pub fn renew(
        &mut self,
        database: &Database,
        now_ms: i64,
        lease_until_ms: i64,
    ) -> Result<(), AdmissionError> {
        if self.phase != WorkloadPhase::Active {
            return Err(AdmissionError::AdmissionClosed);
        }
        self.ticket =
            database.renew_reservation(&self.stamp, &self.ticket, now_ms, lease_until_ms)?;
        Ok(())
    }
    /// Explicit recovery on the owner worker: one new bounded cleanup attempt
    /// against the SAME retained job. Never creates a replacement or starts work.
    /// A previously closed proof is reused while credits/SQLite remain pending.
    pub fn retry_cleanup(
        &mut self,
        database: &Database,
        timeout: Duration,
    ) -> Result<CleanupStatus, AdmissionError> {
        if timeout > MAX_CLEANUP_WAIT {
            return Err(AdmissionError::Job(JobError::InvalidWait));
        }
        if self.phase == WorkloadPhase::Retired {
            if self.job.is_none() && self.closed.is_none() {
                return Err(AdmissionError::BindingMismatch);
            }
            self.phase = WorkloadPhase::Closing;
        } else if !matches!(self.phase, WorkloadPhase::Closing | WorkloadPhase::Released) {
            return Err(AdmissionError::AdmissionClosed);
        }
        self.finish_cleanup(database, timeout)
    }
    /// One finite native cleanup attempt, then a non-waiting credit check, then
    /// SQLite release LAST. Uncertainty retains ticket and local slot. Repeated
    /// calls while credits/DB are pending perform no further native termination.
    pub fn finish_cleanup(
        &mut self,
        database: &Database,
        timeout: Duration,
    ) -> Result<CleanupStatus, AdmissionError> {
        if timeout > MAX_CLEANUP_WAIT {
            return Err(AdmissionError::Job(JobError::InvalidWait));
        }
        if self.phase == WorkloadPhase::Retired {
            return Ok(CleanupStatus::Retired);
        }
        if self.phase == WorkloadPhase::Released {
            return Ok(CleanupStatus::Released {
                ledger_revision: self.released_revision,
            });
        }
        self.queues.close();
        self.phase = WorkloadPhase::Closing;
        if let Some(mut job) = self.job.take() {
            match job.terminate_and_wait(timeout) {
                Ok(CleanupOutcome::EmptyConfirmed) => match job.close_after_empty() {
                    Ok(proof) => self.closed = Some(proof),
                    Err(job) => {
                        self.job = Some(job);
                        return self.retire(database);
                    }
                },
                _ => {
                    self.job = Some(job);
                    return self.retire(database);
                }
            }
        }
        if !self.queues.returned() {
            return Ok(CleanupStatus::AwaitingCredits(self.queues.status()));
        }
        let native = self
            .closed
            .as_ref()
            .ok_or(AdmissionError::BindingMismatch)?;
        if native.owned_key() != self.owned
            || Uuid::from_bytes(native.job_key()).to_string() != self.ticket.job_key
            || native.owner().pid != self.ticket.owner.pid
            || native.owner().created_at_100ns != self.ticket.owner.created_at_100ns
            || native.owner_windows_session_id() != self.ticket.owner.windows_session_id
            || self.ticket.owner.nonce != self.pool.nonce.to_string()
        {
            return Err(AdmissionError::BindingMismatch);
        }
        let now = now_ms()?;
        let proof = EmptyJobProof::from_trusted_closed_empty_job(
            &self.ticket,
            &self.ticket.owner,
            &self.ticket.job_key,
            now,
        )?;
        let released = database.release_reservation(&self.stamp, &self.ticket, &proof, now)?;
        self.phase = WorkloadPhase::Released;
        self.released_revision = released.revision;
        self.pool.slots[self.index]
            .state
            .store(FREE, Ordering::Release);
        Ok(CleanupStatus::Released {
            ledger_revision: released.revision,
        })
    }
    fn retire(&mut self, database: &Database) -> Result<CleanupStatus, AdmissionError> {
        self.queues.close();
        self.phase = WorkloadPhase::Retired;
        self.pool.slots[self.index]
            .state
            .store(RETIRED, Ordering::Release);
        self.ticket = database.retire_reservation(&self.stamp, &self.ticket)?;
        Ok(CleanupStatus::Retired)
    }
}
impl Drop for PreparedWorkload {
    fn drop(&mut self) {
        self.queues.close();
        if self.phase != WorkloadPhase::Released {
            self.pool.slots[self.index]
                .state
                .store(RETIRED, Ordering::Release);
        }
        // WorkloadJob owns its close-only, no-wait fallback. No DB operation here.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_state::location::{LocationRequest, StorageLocation};
    use shinatama_state::resource_reservations::ReservationPolicy;
    use std::fs;

    fn request() -> PrepareRequest {
        let now = now_ms().unwrap();
        PrepareRequest {
            budget: Budget {
                memory_bytes: 32 * 1024 * 1024,
                processes: 2,
                cpu_basis_points: 1000,
                queue_bytes: 64,
                queue_items: 2,
            },
            process_commit_bytes: 16 * 1024 * 1024,
            now_ms: now,
            lease_expires_at_ms: now + 10_000,
        }
    }
    #[test]
    fn budget_mapping_cannot_weaken_any_native_dimension() {
        let request = request();
        let native = request.native_budget().unwrap();
        assert_eq!(native.job_commit_bytes(), request.budget.memory_bytes);
        assert_eq!(native.process_commit_bytes(), request.process_commit_bytes);
        assert_eq!(native.max_processes(), request.budget.processes);
        assert_eq!(
            native.cpu_hard_cap_basis_points(),
            request.budget.cpu_basis_points
        );
        let mut invalid = request;
        invalid.process_commit_bytes = invalid.budget.memory_bytes + 1;
        assert!(invalid.native_budget().is_err());
        invalid = request;
        invalid.budget.processes = 257;
        assert!(invalid.native_budget().is_err());
        invalid = request;
        invalid.budget.cpu_basis_points = 0;
        assert!(invalid.native_budget().is_err());
    }
    #[test]
    fn queue_guards_bound_bytes_and_items_and_closed_clones_cannot_accept() {
        let queue = QueueCredits::new(10, 2);
        let clone = queue.clone();
        let first = queue.acquire(6, 1).unwrap();
        assert!(matches!(clone.acquire(5, 1), Err(QueueError::Capacity)));
        let second = clone.acquire(4, 1).unwrap();
        assert!(matches!(queue.acquire(0, 1), Err(QueueError::Capacity)));
        assert_eq!(queue.status().current_bytes, 10);
        assert_eq!(queue.status().current_items, 2);
        queue.close();
        assert!(matches!(clone.acquire(0, 1), Err(QueueError::Closed)));
        assert!(!queue.returned());
        drop(first);
        drop(second);
        assert!(queue.returned());
        let status = queue.status();
        assert_eq!(
            (status.peak_bytes, status.peak_items, status.refusals),
            (10, 2, 3)
        );
    }
    #[test]
    fn native_empty_job_reservation_is_held_until_all_queue_credits_return() {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let root = base.join(format!("shinatama-admission-{}", fresh_uuid().unwrap()));
        fs::create_dir(&root).unwrap();
        let result = std::panic::catch_unwind(|| {
            let location = StorageLocation::resolve(LocationRequest {
                local_data_directory: root.join("platform"),
                home_override: Some(root.join("private")),
                project_roots: Vec::new(),
                synchronized_roots: Vec::new(),
            })
            .unwrap();
            let database = Database::open(&location, None).unwrap();
            let request = request();
            let policy = ReservationPolicy {
                stamp: PolicyStamp {
                    version: 1,
                    id: "admission-fixture".into(),
                },
                aggregate_ceiling: request.budget,
                per_slot_ceiling: request.budget,
                max_slots: 1,
                max_lease_ms: 10_000,
            };
            database.initialize_resource_policy(&policy).unwrap();
            let coordinator = AdmissionCoordinator::new(1).unwrap();
            assert!(matches!(
                AdmissionCoordinator::new(1),
                Err(AdmissionError::PoolBusy)
            ));
            let mut workload = coordinator
                .prepare(&database, &policy.stamp, 0, request)
                .unwrap();
            assert_eq!(workload.ticket().owner.pid, workload.owned_key().owner.pid);
            assert_eq!(
                workload.ticket().job_key,
                Uuid::from_bytes(workload.owned_key().job_key).to_string()
            );
            assert!(workload.startup_binding(request.now_ms).is_ok());
            let queue = workload.queue_credits();
            let guard = queue.acquire(32, 1).unwrap();
            assert!(matches!(
                workload.finish_cleanup(&database, Duration::ZERO).unwrap(),
                CleanupStatus::AwaitingCredits(_)
            ));
            assert!(workload.startup_binding(request.now_ms).is_err());
            assert_eq!(
                database
                    .resource_reservations()
                    .unwrap()
                    .unwrap()
                    .slots
                    .len(),
                1
            );
            assert!(matches!(
                coordinator.prepare(&database, &policy.stamp, 1, request),
                Err(AdmissionError::LocalCapacity)
            ));
            assert!(matches!(queue.acquire(1, 1), Err(QueueError::Closed)));
            drop(guard);
            let released = workload.finish_cleanup(&database, Duration::ZERO).unwrap();
            assert!(matches!(released, CleanupStatus::Released { .. }));
            assert!(
                database
                    .resource_reservations()
                    .unwrap()
                    .unwrap()
                    .slots
                    .is_empty()
            );
            drop(workload);
            let revision = database.resource_reservations().unwrap().unwrap().revision;
            let mut recoverable = coordinator
                .prepare(&database, &policy.stamp, revision, request)
                .unwrap();
            let original_key = recoverable.owned_key();
            let original_id = recoverable.ticket().id.clone();
            let held_credit = recoverable.queue_credits().acquire(1, 1).unwrap();
            assert_eq!(
                recoverable.retire(&database).unwrap(),
                CleanupStatus::Retired
            );
            assert_eq!(recoverable.ticket().state, ReservationState::Retired);
            assert_eq!(
                recoverable
                    .finish_cleanup(&database, Duration::ZERO)
                    .unwrap(),
                CleanupStatus::Retired
            );
            assert!(matches!(
                recoverable
                    .retry_cleanup(&database, Duration::ZERO)
                    .unwrap(),
                CleanupStatus::AwaitingCredits(_)
            ));
            assert_eq!(recoverable.owned_key(), original_key);
            assert_eq!(recoverable.ticket().id, original_id);
            assert_eq!(coordinator.retired_slots(), 1);
            assert_eq!(
                database
                    .resource_reservations()
                    .unwrap()
                    .unwrap()
                    .slots
                    .len(),
                1
            );
            drop(held_credit);
            assert!(matches!(
                recoverable
                    .retry_cleanup(&database, Duration::ZERO)
                    .unwrap(),
                CleanupStatus::Released { .. }
            ));
            assert_eq!(coordinator.retired_slots(), 0);
            assert!(
                database
                    .resource_reservations()
                    .unwrap()
                    .unwrap()
                    .slots
                    .is_empty()
            );
            drop(recoverable);
            // No SQL in Drop: un-finished ownership conservatively retires its
            // fixed local slot and leaves the persistent global ticket intact.
            let revision = database.resource_reservations().unwrap().unwrap().revision;
            let abandoned = coordinator
                .prepare(&database, &policy.stamp, revision, request)
                .unwrap();
            drop(abandoned);
            assert_eq!(coordinator.retired_slots(), 1);
            assert!(matches!(
                coordinator.prepare(&database, &policy.stamp, revision, request),
                Err(AdmissionError::LocalCapacity)
            ));
            assert_eq!(
                database
                    .resource_reservations()
                    .unwrap()
                    .unwrap()
                    .slots
                    .len(),
                1
            );
            drop(coordinator);
            assert!(matches!(
                AdmissionCoordinator::new(1),
                Err(AdmissionError::PoolRetired)
            ));
            // Retired process-wide state is deliberate; this is the sole test
            // constructing a coordinator, and no test-only reset is provided.
        });
        assert_eq!(root.parent(), Some(base.as_path()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-admission-")
        );
        let _ = fs::remove_dir_all(&root);
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
