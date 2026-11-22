//! C09R.2 aggregate accounting, not native enforcement or adaptive calibration.
//! One bounded operational setting is serialized across application connections.
//! No process, clock, sampler, job or termination API is called by this module.

use crate::database::{Database, DatabaseError};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

pub const SETTING_KEY: &str = "operational.resource_reservations.v1";
pub const MAX_LEDGER_BYTES: usize = 64 * 1024;
pub const MAX_SLOTS: usize = 64;
const MAX_REVISION: u64 = 9_007_199_254_740_991;
const MAX_LEASE_MS: u32 = 300_000;
const MAX_PROOF_AGE_MS: i64 = 5_000;

#[derive(Debug)]
pub enum ReservationError {
    Invalid,
    Malformed,
    PolicyMismatch,
    Conflict,
    Capacity,
    NotFound,
    InvalidTransition,
    ProofRequired,
    Database(DatabaseError),
    Sqlite(rusqlite::Error),
}
impl From<DatabaseError> for ReservationError {
    fn from(value: DatabaseError) -> Self {
        Self::Database(value)
    }
}
impl From<rusqlite::Error> for ReservationError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}
impl fmt::Display for ReservationError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(match self {
            Self::Invalid=>"Richiesta di risorse non valida.", Self::Malformed=>"Le prenotazioni salvate non sono verificabili; nuovi avvii rinviati e dati conservati.",
            Self::PolicyMismatch=>"La policy delle risorse non corrisponde a quella condivisa; nessun limite è stato aumentato.",
            Self::Conflict=>"La prenotazione è cambiata; aggiorna lo stato prima di riprovare.", Self::Capacity=>"Il budget condiviso non ammette un altro avvio.",
            Self::NotFound=>"La prenotazione richiesta non è disponibile.", Self::InvalidTransition=>"La prenotazione non consente questa transizione.",
            Self::ProofRequired=>"Manca una prova recente che il job corretto sia vuoto e definitivamente chiuso a nuovi avvii.",
            Self::Database(_)|Self::Sqlite(_)=>"Impossibile aggiornare le prenotazioni; il budget precedente resta conservato.",
        })
    }
}
impl std::error::Error for ReservationError {}
pub type Result<T> = std::result::Result<T, ReservationError>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Committed bytes for the entire job, never summed working sets.
    pub memory_bytes: u64,
    pub processes: u32,
    /// One machine's CPU capacity is 10000. Native OS enforcement is separate.
    pub cpu_basis_points: u16,
    pub queue_bytes: u64,
    pub queue_items: u32,
}
impl Budget {
    fn validate(self, positive: bool) -> Result<()> {
        if self.memory_bytes > MAX_REVISION
            || self.queue_bytes > MAX_REVISION
            || self.cpu_basis_points > 10000
            || (positive
                && (self.memory_bytes == 0 || self.processes == 0 || self.cpu_basis_points == 0))
        {
            return Err(ReservationError::Invalid);
        }
        Ok(())
    }
    fn fits(self, limit: Self) -> bool {
        self.memory_bytes <= limit.memory_bytes
            && self.processes <= limit.processes
            && self.cpu_basis_points <= limit.cpu_basis_points
            && self.queue_bytes <= limit.queue_bytes
            && self.queue_items <= limit.queue_items
    }
    fn add(self, other: Self) -> Result<Self> {
        Ok(Self {
            memory_bytes: self
                .memory_bytes
                .checked_add(other.memory_bytes)
                .ok_or(ReservationError::Capacity)?,
            processes: self
                .processes
                .checked_add(other.processes)
                .ok_or(ReservationError::Capacity)?,
            cpu_basis_points: self
                .cpu_basis_points
                .checked_add(other.cpu_basis_points)
                .ok_or(ReservationError::Capacity)?,
            queue_bytes: self
                .queue_bytes
                .checked_add(other.queue_bytes)
                .ok_or(ReservationError::Capacity)?,
            queue_items: self
                .queue_items
                .checked_add(other.queue_items)
                .ok_or(ReservationError::Capacity)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyStamp {
    pub version: u32,
    pub id: String,
}
impl PolicyStamp {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.version == 0
            || self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
        {
            Err(ReservationError::Invalid)
        } else {
            Ok(())
        }
    }
}

/// Reservations never change their policy. A trusted caller may replace an empty
/// ledger's policy explicitly, comparing its complete policy and revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservationPolicy {
    pub stamp: PolicyStamp,
    pub aggregate_ceiling: Budget,
    pub per_slot_ceiling: Budget,
    pub max_slots: u8,
    pub max_lease_ms: u32,
}
impl ReservationPolicy {
    fn validate(&self) -> Result<()> {
        self.stamp.validate()?;
        self.aggregate_ceiling.validate(true)?;
        self.per_slot_ceiling.validate(true)?;
        if !self.per_slot_ceiling.fits(self.aggregate_ceiling)
            || self.max_slots == 0
            || usize::from(self.max_slots) > MAX_SLOTS
            || self.max_lease_ms == 0
            || self.max_lease_ms > MAX_LEASE_MS
        {
            return Err(ReservationError::Invalid);
        }
        Ok(())
    }
}

/// A budget as the aggregate ceiling counts it: an advisory slot's CPU is not counted.
fn charged(budget: Budget, advisory: bool) -> Budget {
    if advisory {
        Budget {
            cpu_basis_points: 0,
            ..budget
        }
    } else {
        budget
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservationOwner {
    pub pid: u32,
    /// Exact Windows FILETIME creation value, never rounded to milliseconds.
    pub created_at_100ns: u64,
    /// Local named jobs are scoped to this exact Windows session; zero is valid.
    pub windows_session_id: u32,
    /// Fresh random UUID v4 for this owning runtime, supplied by trusted caller.
    pub nonce: String,
}
impl ReservationOwner {
    fn validate(&self) -> Result<()> {
        if self.pid == 0 || self.created_at_100ns == 0 {
            return Err(ReservationError::Invalid);
        }
        validate_uuid(&self.nonce)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationState {
    Reserved,
    Active,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservationTicket {
    pub id: String,
    pub job_key: String,
    pub owner: ReservationOwner,
    pub budget: Budget,
    pub state: ReservationState,
    pub lease_expires_at_ms: i64,
    /// Assigned from the monotonic ledger revision, never restarted at one.
    pub revision: u64,
    /// A session the user opened, whose CPU is advisory (no Windows rate control): its CPU is
    /// not charged against the aggregate ceiling, so the count of open sessions is bounded by
    /// slots, memory and processes, not by a CPU share nothing enforces. Written only when set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub advisory: bool,
}
impl ReservationTicket {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_uuid(&self.id)?;
        validate_uuid(&self.job_key)?;
        self.owner.validate()?;
        self.budget.validate(true)?;
        if self.revision == 0 || self.revision > MAX_REVISION || self.lease_expires_at_ms < 0 {
            return Err(ReservationError::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ReservationRequest {
    pub id: String,
    pub job_key: String,
    pub owner: ReservationOwner,
    pub budget: Budget,
    /// See `ReservationTicket::advisory`.
    pub advisory: bool,
    pub now_ms: i64,
    pub lease_expires_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservationSnapshot {
    pub format_version: u32,
    pub revision: u64,
    pub policy: ReservationPolicy,
    pub slots: Vec<ReservationTicket>,
}
impl ReservationSnapshot {
    pub fn reserved_total(&self) -> Result<Budget> {
        self.slots
            .iter()
            .try_fold(Budget::default(), |total, slot| total.add(slot.budget))
    }
    /// What the aggregate ceiling is checked against: every budget, except the CPU of advisory slots.
    pub fn charged_total(&self) -> Result<Budget> {
        self.slots
            .iter()
            .try_fold(Budget::default(), |total, slot| {
                total.add(charged(slot.budget, slot.advisory))
            })
    }
    fn validate(&self) -> Result<()> {
        if self.format_version != 1
            || self.revision > MAX_REVISION
            || self.slots.len() > MAX_SLOTS
            || self.slots.len() > usize::from(self.policy.max_slots)
        {
            return Err(ReservationError::Malformed);
        }
        self.policy
            .validate()
            .map_err(|_| ReservationError::Malformed)?;
        let mut ids = BTreeSet::new();
        let mut jobs = BTreeSet::new();
        for slot in &self.slots {
            slot.validate().map_err(|_| ReservationError::Malformed)?;
            if slot.revision > self.revision
                || !slot.budget.fits(self.policy.per_slot_ceiling)
                || !ids.insert(&slot.id)
                || !jobs.insert(&slot.job_key)
            {
                return Err(ReservationError::Malformed);
            }
        }
        if !self
            .charged_total()
            .map_err(|_| ReservationError::Malformed)?
            .fits(self.policy.aggregate_ceiling)
        {
            return Err(ReservationError::Malformed);
        }
        Ok(())
    }
    pub(crate) fn next_revision(&mut self) -> Result<u64> {
        self.revision = self
            .revision
            .checked_add(1)
            .filter(|revision| *revision <= MAX_REVISION)
            .ok_or(ReservationError::Capacity)?;
        Ok(self.revision)
    }
    pub(crate) fn require_policy(&self, stamp: &PolicyStamp) -> Result<()> {
        stamp.validate()?;
        if &self.policy.stamp == stamp {
            Ok(())
        } else {
            Err(ReservationError::PolicyMismatch)
        }
    }
    pub(crate) fn ticket_index(&self, expected: &ReservationTicket) -> Result<usize> {
        expected.validate()?;
        let index = self
            .slots
            .iter()
            .position(|slot| slot.id == expected.id)
            .ok_or(ReservationError::NotFound)?;
        if self.slots[index] != *expected {
            return Err(ReservationError::Conflict);
        }
        Ok(index)
    }
}

/// This proof is not deserializable. Only the trusted native admission bridge
/// may construct it after consuming/sealing its exact job and proving empty.
#[derive(Clone, Debug)]
pub struct EmptyJobProof {
    ticket: ReservationTicket,
    observed_at_ms: i64,
}
impl EmptyJobProof {
    pub fn from_trusted_closed_empty_job(
        ticket: &ReservationTicket,
        observed_owner: &ReservationOwner,
        observed_job_key: &str,
        observed_at_ms: i64,
    ) -> Result<Self> {
        ticket.validate()?;
        if observed_at_ms < 0
            || observed_owner != &ticket.owner
            || observed_job_key != ticket.job_key
        {
            return Err(ReservationError::Invalid);
        }
        Ok(Self {
            ticket: ticket.clone(),
            observed_at_ms,
        })
    }
    fn verify(&self, ticket: &ReservationTicket, now_ms: i64) -> Result<()> {
        if &self.ticket != ticket
            || now_ms < self.observed_at_ms
            || now_ms - self.observed_at_ms > MAX_PROOF_AGE_MS
        {
            return Err(ReservationError::ProofRequired);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerObservation {
    AliveExact,
    DeadExact,
    Unknown,
    OtherWindowsSession,
}

/// Observations are computed outside SQL by the trusted reconciler. They carry
/// the complete prior record so renew/activate/retire invalidates a stale probe.
#[derive(Clone, Debug)]
pub struct ReconcileObservation {
    pub expected: ReservationTicket,
    pub owner: OwnerObservation,
    pub observed_at_ms: i64,
    pub closed_empty_job: Option<EmptyJobProof>,
}

fn validate_uuid(value: &str) -> Result<()> {
    if value.len() != 36 {
        return Err(ReservationError::Invalid);
    }
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| ReservationError::Invalid)?;
    if uuid.get_version_num() != 4 || uuid.to_string() != value {
        return Err(ReservationError::Invalid);
    }
    Ok(())
}

fn validate_lease(policy: &ReservationPolicy, now_ms: i64, expires_at_ms: i64) -> Result<()> {
    if now_ms < 0
        || expires_at_ms <= now_ms
        || expires_at_ms
            .checked_sub(now_ms)
            .is_none_or(|span| span > i64::from(policy.max_lease_ms))
    {
        return Err(ReservationError::Invalid);
    }
    Ok(())
}

pub(crate) fn read_ledger(
    connection: &rusqlite::Connection,
) -> Result<Option<ReservationSnapshot>> {
    let value: Option<Option<String>> = connection
        .query_row(
            "SELECT CASE WHEN octet_length(value)<=65536 THEN value END FROM setting WHERE key=?1",
            [SETTING_KEY],
            |row| row.get(0),
        )
        .optional()?;
    let Some(value) = value else {
        return Ok(None);
    };
    let json = value.ok_or(ReservationError::Malformed)?;
    let ledger: ReservationSnapshot =
        serde_json::from_str(&json).map_err(|_| ReservationError::Malformed)?;
    ledger.validate()?;
    Ok(Some(ledger))
}

pub(crate) fn write_ledger(
    connection: &rusqlite::Connection,
    ledger: &ReservationSnapshot,
) -> Result<()> {
    ledger.validate()?;
    let json = serde_json::to_string(ledger).map_err(|_| ReservationError::Malformed)?;
    if json.len() > MAX_LEDGER_BYTES {
        return Err(ReservationError::Capacity);
    }
    connection.execute("INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![SETTING_KEY,json])?;
    Ok(())
}

fn replace_empty_policy(
    connection: &mut rusqlite::Connection,
    expected: &ReservationPolicy,
    expected_revision: u64,
    replacement: &ReservationPolicy,
) -> Result<ReservationSnapshot> {
    expected.validate()?;
    replacement.validate()?;
    if expected_revision > MAX_REVISION {
        return Err(ReservationError::Invalid);
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    crate::schema::require_write_version(&transaction)?;
    let mut ledger = read_ledger(&transaction)?.ok_or(ReservationError::NotFound)?;
    if ledger.revision != expected_revision {
        return Err(ReservationError::Conflict);
    }
    if ledger.policy != *expected {
        return Err(ReservationError::PolicyMismatch);
    }
    // Even a retired reservation still owns capacity until native cleanup is
    // proven. Neither expiry nor an apparently dead owner establishes emptiness.
    if !ledger.slots.is_empty() {
        return Err(ReservationError::InvalidTransition);
    }
    if ledger.policy != *replacement {
        ledger.next_revision()?;
        ledger.policy = replacement.clone();
        write_ledger(&transaction, &ledger)?;
    }
    transaction.commit()?;
    Ok(ledger)
}

impl Database {
    /// Explicit policy installation, not part of reserve. An existing policy can
    /// only be reused verbatim, even when every reservation has been released.
    pub fn initialize_resource_policy(
        &self,
        policy: &ReservationPolicy,
    ) -> Result<ReservationSnapshot> {
        policy.validate()?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        if let Some(existing) = read_ledger(&transaction)? {
            if existing.policy != *policy {
                return Err(ReservationError::PolicyMismatch);
            }
            transaction.commit()?;
            return Ok(existing);
        }
        let ledger = ReservationSnapshot {
            format_version: 1,
            revision: 0,
            policy: policy.clone(),
            slots: Vec::new(),
        };
        write_ledger(&transaction, &ledger)?;
        transaction.commit()?;
        Ok(ledger)
    }

    pub fn resource_reservations(&self) -> Result<Option<ReservationSnapshot>> {
        let connection = self.lock()?;
        read_ledger(&connection)
    }

    /// Explicit policy migration after all reservations have been released.
    /// The complete expected policy and revision are compared in one write
    /// transaction; any outstanding reservation or concurrent change refuses it.
    /// Ordinary initialization and reservation calls cannot raise a ceiling.
    pub fn replace_empty_resource_policy(
        &self,
        expected: &ReservationPolicy,
        expected_revision: u64,
        replacement: &ReservationPolicy,
    ) -> Result<ReservationSnapshot> {
        self.require_writable()?;
        let mut connection = self.lock()?;
        replace_empty_policy(&mut connection, expected, expected_revision, replacement)
    }

    pub fn reserve_resources(
        &self,
        stamp: &PolicyStamp,
        expected_revision: u64,
        request: &ReservationRequest,
    ) -> Result<ReservationTicket> {
        validate_uuid(&request.id)?;
        validate_uuid(&request.job_key)?;
        request.owner.validate()?;
        request.budget.validate(true)?;
        self.mutate_reservations(stamp, |_connection, ledger| {
            if ledger.revision != expected_revision {
                return Err(ReservationError::Conflict);
            }
            validate_lease(&ledger.policy, request.now_ms, request.lease_expires_at_ms)?;
            if ledger.slots.len() >= usize::from(ledger.policy.max_slots)
                || !request.budget.fits(ledger.policy.per_slot_ceiling)
                || !ledger
                    .charged_total()?
                    .add(charged(request.budget, request.advisory))?
                    .fits(ledger.policy.aggregate_ceiling)
            {
                return Err(ReservationError::Capacity);
            }
            if ledger
                .slots
                .iter()
                .any(|slot| slot.id == request.id || slot.job_key == request.job_key)
            {
                return Err(ReservationError::Conflict);
            }
            let ticket = ReservationTicket {
                id: request.id.clone(),
                job_key: request.job_key.clone(),
                owner: request.owner.clone(),
                budget: request.budget,
                state: ReservationState::Reserved,
                lease_expires_at_ms: request.lease_expires_at_ms,
                revision: ledger.next_revision()?,
                advisory: request.advisory,
            };
            ledger.slots.push(ticket.clone());
            Ok(ticket)
        })
    }

    pub fn activate_reservation(
        &self,
        stamp: &PolicyStamp,
        expected: &ReservationTicket,
        now_ms: i64,
        lease_expires_at_ms: i64,
    ) -> Result<ReservationTicket> {
        self.mutate_reservations(stamp, |_connection, ledger| {
            let index = ledger.ticket_index(expected)?;
            validate_lease(&ledger.policy, now_ms, lease_expires_at_ms)?;
            if expected.state != ReservationState::Reserved
                || now_ms >= expected.lease_expires_at_ms
            {
                return Err(ReservationError::InvalidTransition);
            }
            let revision = ledger.next_revision()?;
            let slot = &mut ledger.slots[index];
            slot.state = ReservationState::Active;
            slot.lease_expires_at_ms = lease_expires_at_ms;
            slot.revision = revision;
            Ok(slot.clone())
        })
    }

    pub fn renew_reservation(
        &self,
        stamp: &PolicyStamp,
        expected: &ReservationTicket,
        now_ms: i64,
        lease_expires_at_ms: i64,
    ) -> Result<ReservationTicket> {
        self.mutate_reservations(stamp, |_connection, ledger| {
            let index = ledger.ticket_index(expected)?;
            validate_lease(&ledger.policy, now_ms, lease_expires_at_ms)?;
            if expected.state == ReservationState::Retired {
                return Err(ReservationError::InvalidTransition);
            }
            let revision = ledger.next_revision()?;
            let slot = &mut ledger.slots[index];
            slot.lease_expires_at_ms = lease_expires_at_ms;
            slot.revision = revision;
            Ok(slot.clone())
        })
    }

    pub fn retire_reservation(
        &self,
        stamp: &PolicyStamp,
        expected: &ReservationTicket,
    ) -> Result<ReservationTicket> {
        self.mutate_reservations(stamp, |_connection, ledger| {
            let index = ledger.ticket_index(expected)?;
            if expected.state == ReservationState::Retired {
                return Ok(expected.clone());
            }
            let revision = ledger.next_revision()?;
            let slot = &mut ledger.slots[index];
            slot.state = ReservationState::Retired;
            slot.revision = revision;
            Ok(slot.clone())
        })
    }

    pub fn release_reservation(
        &self,
        stamp: &PolicyStamp,
        expected: &ReservationTicket,
        proof: &EmptyJobProof,
        now_ms: i64,
    ) -> Result<ReservationSnapshot> {
        proof.verify(expected, now_ms)?;
        self.mutate_reservations(stamp, |connection, ledger| {
            let index = ledger.ticket_index(expected)?;
            crate::native_session_cleanup::record_cleanup(connection, stamp, expected, now_ms)?;
            ledger.slots.remove(index);
            ledger.next_revision()?;
            Ok(ledger.clone())
        })
    }

    pub fn reconcile_reservations(
        &self,
        stamp: &PolicyStamp,
        observations: &[ReconcileObservation],
        now_ms: i64,
    ) -> Result<ReservationSnapshot> {
        if now_ms < 0 || observations.len() > MAX_SLOTS {
            return Err(ReservationError::Invalid);
        }
        let mut ids = BTreeSet::new();
        for observation in observations {
            observation.expected.validate()?;
            if !ids.insert(&observation.expected.id)
                || observation.observed_at_ms < 0
                || observation.observed_at_ms > now_ms
                || now_ms - observation.observed_at_ms > MAX_PROOF_AGE_MS
            {
                return Err(ReservationError::Invalid);
            }
        }
        self.mutate_reservations(stamp, |connection, ledger| {
            // Validate the entire batch before changing any slot.
            for observation in observations {
                ledger.ticket_index(&observation.expected)?;
            }
            for observation in observations {
                if observation.owner != OwnerObservation::DeadExact {
                    continue;
                }
                let index = ledger.ticket_index(&observation.expected)?;
                if let Some(proof) = &observation.closed_empty_job {
                    proof.verify(&observation.expected, now_ms)?;
                    crate::native_session_cleanup::record_cleanup(
                        connection,
                        stamp,
                        &observation.expected,
                        now_ms,
                    )?;
                    ledger.slots.remove(index);
                    ledger.next_revision()?;
                } else if ledger.slots[index].state != ReservationState::Retired {
                    let revision = ledger.next_revision()?;
                    ledger.slots[index].state = ReservationState::Retired;
                    ledger.slots[index].revision = revision;
                }
            }
            Ok(ledger.clone())
        })
    }

    fn mutate_reservations<T>(
        &self,
        stamp: &PolicyStamp,
        operation: impl FnOnce(&rusqlite::Connection, &mut ReservationSnapshot) -> Result<T>,
    ) -> Result<T> {
        stamp.validate()?;
        self.require_writable()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&transaction)?;
        let mut ledger = read_ledger(&transaction)?.ok_or(ReservationError::NotFound)?;
        ledger.require_policy(stamp)?;
        let prior_revision = ledger.revision;
        let result = operation(&transaction, &mut ledger)?;
        // Alive/unknown observations and already-retired tickets are read-only:
        // repeated monitoring must not grow WAL merely by rewriting identical JSON.
        if ledger.revision != prior_revision {
            write_ledger(&transaction, &ledger)?;
        }
        transaction.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod policy_replacement_tests {
    use super::*;

    fn policy() -> ReservationPolicy {
        ReservationPolicy {
            stamp: PolicyStamp {
                version: 1,
                id: "empty-policy-migration".into(),
            },
            aggregate_ceiling: Budget {
                memory_bytes: 1024,
                processes: 8,
                cpu_basis_points: 8000,
                queue_bytes: 256,
                queue_items: 8,
            },
            per_slot_ceiling: Budget {
                memory_bytes: 1024,
                processes: 1,
                cpu_basis_points: 1000,
                queue_bytes: 64,
                queue_items: 2,
            },
            max_slots: 8,
            max_lease_ms: 10_000,
        }
    }

    fn fixture() -> (rusqlite::Connection, ReservationSnapshot, ReservationPolicy) {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch(crate::schema::CREATE_V1).unwrap();
        connection
            .pragma_update(None, "user_version", crate::schema::CURRENT_VERSION)
            .unwrap();
        let before = ReservationSnapshot {
            format_version: 1,
            revision: 4,
            policy: policy(),
            slots: vec![],
        };
        write_ledger(&connection, &before).unwrap();
        let mut replacement = before.policy.clone();
        replacement.aggregate_ceiling.memory_bytes = 4096;
        (connection, before, replacement)
    }

    fn raw(connection: &rusqlite::Connection) -> String {
        connection
            .query_row(
                "SELECT value FROM setting WHERE key=?1",
                [SETTING_KEY],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn empty_policy_replacement_persists_and_advances_its_revision_once() {
        let (mut connection, before, replacement) = fixture();
        let updated = replace_empty_policy(
            &mut connection,
            &before.policy,
            before.revision,
            &replacement,
        )
        .unwrap();
        assert_eq!(updated.policy, replacement);
        assert_eq!(updated.revision, before.revision + 1);
        assert!(updated.slots.is_empty());
        assert_eq!(read_ledger(&connection).unwrap().unwrap(), updated);
        // Retrying the old CAS cannot apply a second policy change.
        assert!(matches!(
            replace_empty_policy(
                &mut connection,
                &before.policy,
                before.revision,
                &replacement
            ),
            Err(ReservationError::Conflict)
        ));
        assert_eq!(read_ledger(&connection).unwrap().unwrap(), updated);
    }

    #[test]
    fn replacement_requires_the_entire_expected_policy_and_current_revision() {
        let (mut connection, before, replacement) = fixture();
        let original = raw(&connection);
        assert!(matches!(
            replace_empty_policy(
                &mut connection,
                &before.policy,
                before.revision - 1,
                &replacement
            ),
            Err(ReservationError::Conflict)
        ));
        let mut unexpected = before.policy.clone();
        unexpected.max_lease_ms -= 1;
        assert!(matches!(
            replace_empty_policy(&mut connection, &unexpected, before.revision, &replacement),
            Err(ReservationError::PolicyMismatch)
        ));
        assert_eq!(raw(&connection), original);
    }

    #[test]
    fn every_outstanding_reservation_blocks_policy_replacement() {
        for state in [
            ReservationState::Reserved,
            ReservationState::Active,
            ReservationState::Retired,
        ] {
            let (mut connection, mut before, replacement) = fixture();
            before.slots.push(ReservationTicket {
                id: uuid::Uuid::new_v4().to_string(),
                job_key: uuid::Uuid::new_v4().to_string(),
                owner: ReservationOwner {
                    pid: 1,
                    created_at_100ns: 1,
                    windows_session_id: 0,
                    nonce: uuid::Uuid::new_v4().to_string(),
                },
                budget: before.policy.per_slot_ceiling,
                state,
                lease_expires_at_ms: 1000,
                revision: before.revision,
                advisory: false,
            });
            write_ledger(&connection, &before).unwrap();
            let original = raw(&connection);
            assert!(matches!(
                replace_empty_policy(
                    &mut connection,
                    &before.policy,
                    before.revision,
                    &replacement
                ),
                Err(ReservationError::InvalidTransition)
            ));
            assert_eq!(raw(&connection), original);
        }
    }

    #[test]
    fn invalid_replacement_and_exhausted_revision_preserve_the_original_ledger() {
        let (mut connection, mut before, mut replacement) = fixture();
        let original = raw(&connection);
        replacement.max_slots = 0;
        assert!(matches!(
            replace_empty_policy(
                &mut connection,
                &before.policy,
                before.revision,
                &replacement
            ),
            Err(ReservationError::Invalid)
        ));
        assert_eq!(raw(&connection), original);
        replacement.max_slots = before.policy.max_slots;
        before.revision = MAX_REVISION;
        write_ledger(&connection, &before).unwrap();
        let original = raw(&connection);
        assert!(matches!(
            replace_empty_policy(
                &mut connection,
                &before.policy,
                before.revision,
                &replacement
            ),
            Err(ReservationError::Capacity)
        ));
        assert_eq!(raw(&connection), original);
    }

    #[test]
    fn identical_empty_policy_is_an_idempotent_read() {
        let (mut connection, before, _) = fixture();
        let original = raw(&connection);
        assert_eq!(
            replace_empty_policy(
                &mut connection,
                &before.policy,
                before.revision,
                &before.policy
            )
            .unwrap(),
            before
        );
        assert_eq!(raw(&connection), original);
    }

    #[test]
    fn failed_sql_write_rolls_back_the_policy_and_revision() {
        let (mut connection, before, replacement) = fixture();
        let original = raw(&connection);
        connection.execute_batch("CREATE TRIGGER refuse_policy_change BEFORE UPDATE ON setting BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END;").unwrap();
        assert!(matches!(
            replace_empty_policy(
                &mut connection,
                &before.policy,
                before.revision,
                &replacement
            ),
            Err(ReservationError::Sqlite(_))
        ));
        assert_eq!(raw(&connection), original);
        assert!(connection.is_autocommit());
    }

    #[test]
    fn newer_schema_or_malformed_ledger_is_never_replaced() {
        let (mut connection, before, replacement) = fixture();
        let original = raw(&connection);
        connection
            .pragma_update(None, "user_version", crate::schema::CURRENT_VERSION + 1)
            .unwrap();
        assert!(matches!(
            replace_empty_policy(
                &mut connection,
                &before.policy,
                before.revision,
                &replacement
            ),
            Err(ReservationError::Database(
                DatabaseError::NewerSchema { .. }
            ))
        ));
        assert_eq!(raw(&connection), original);
        connection
            .pragma_update(None, "user_version", crate::schema::CURRENT_VERSION)
            .unwrap();
        connection
            .execute("UPDATE setting SET value='{}' WHERE key=?1", [SETTING_KEY])
            .unwrap();
        assert!(matches!(
            replace_empty_policy(
                &mut connection,
                &before.policy,
                before.revision,
                &replacement
            ),
            Err(ReservationError::Malformed)
        ));
        assert_eq!(raw(&connection), "{}");
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::location::{LocationRequest, StorageLocation};
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Barrier},
    };

    struct Fixture {
        root: PathBuf,
        location: Option<StorageLocation>,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "shinatama-resource-reservations-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir(&root).unwrap();
            let location = StorageLocation::resolve(LocationRequest {
                local_data_directory: root.join("platform"),
                home_override: Some(root.join("private")),
                project_roots: Vec::new(),
                synchronized_roots: Vec::new(),
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
            assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
            assert!(
                self.root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("shinatama-resource-reservations-")
            );
            if let Err(error) = fs::remove_dir_all(&self.root) {
                eprintln!("Synthetic reservation fixture cleanup: {error}");
            }
        }
    }

    fn budget() -> Budget {
        Budget {
            memory_bytes: 1024,
            processes: 1,
            cpu_basis_points: 100,
            queue_bytes: 64,
            queue_items: 2,
        }
    }
    fn policy() -> ReservationPolicy {
        ReservationPolicy {
            stamp: PolicyStamp {
                version: 1,
                id: "synthetic-frozen-policy".into(),
            },
            aggregate_ceiling: budget(),
            per_slot_ceiling: budget(),
            max_slots: 64,
            max_lease_ms: 10000,
        }
    }
    fn owner() -> ReservationOwner {
        ReservationOwner {
            pid: 7,
            created_at_100ns: 133801234567890123,
            windows_session_id: 0,
            nonce: uuid::Uuid::new_v4().to_string(),
        }
    }
    fn request() -> ReservationRequest {
        ReservationRequest {
            id: uuid::Uuid::new_v4().to_string(),
            job_key: uuid::Uuid::new_v4().to_string(),
            owner: owner(),
            budget: budget(),
            now_ms: 100,
            lease_expires_at_ms: 1000,
            advisory: false,
        }
    }
    #[test]
    fn advisory_sessions_are_not_charged_cpu_and_older_ledgers_still_read() {
        // Four advisory sessions whose CPU alone would exceed the ceiling fit; memory still counts.
        let mut snapshot = ReservationSnapshot {
            format_version: 1,
            revision: 0,
            policy: policy(),
            slots: Vec::new(),
        };
        snapshot.policy.aggregate_ceiling.processes = 2;
        let ceiling = snapshot.policy.aggregate_ceiling;
        let session = Budget {
            cpu_basis_points: ceiling.cpu_basis_points,
            ..Budget {
                memory_bytes: 1,
                processes: 1,
                cpu_basis_points: 1,
                queue_bytes: 1,
                queue_items: 1,
            }
        };
        for index in 0..2u64 {
            snapshot.revision += 1;
            snapshot.slots.push(ReservationTicket {
                id: uuid::Uuid::new_v4().to_string(),
                job_key: uuid::Uuid::new_v4().to_string(),
                owner: owner(),
                budget: session,
                state: ReservationState::Active,
                lease_expires_at_ms: 1000,
                revision: index + 1,
                advisory: true,
            });
        }
        assert!(snapshot.reserved_total().unwrap().cpu_basis_points > ceiling.cpu_basis_points);
        assert_eq!(snapshot.charged_total().unwrap().cpu_basis_points, 0);
        snapshot.validate().unwrap();
        // A non-advisory slot is charged in full.
        snapshot.slots[0].advisory = false;
        assert_eq!(
            snapshot.charged_total().unwrap().cpu_basis_points,
            ceiling.cpu_basis_points
        );
        // `advisory` is written only when set, and a ticket saved without it reads as not advisory.
        let plain = serde_json::to_string(&snapshot.slots[0]).unwrap();
        assert!(!plain.contains("advisory"));
        let read: ReservationTicket = serde_json::from_str(&plain).unwrap();
        assert!(!read.advisory);
        assert!(
            serde_json::to_string(&snapshot.slots[1])
                .unwrap()
                .contains("\"advisory\":true")
        );
    }
    fn raw(database: &Database) -> String {
        database
            .lock()
            .unwrap()
            .query_row(
                "SELECT value FROM setting WHERE key=?1",
                [SETTING_KEY],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn two_connections_atomically_reserve_only_one_available_budget() {
        let fixture = Fixture::new();
        let first = fixture.open();
        let second = fixture.open();
        let policy = policy();
        first.initialize_resource_policy(&policy).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let outcomes = std::thread::scope(|scope| {
            let left = barrier.clone();
            let right = barrier.clone();
            let stamp = &policy.stamp;
            let first = scope.spawn(move || {
                left.wait();
                first.reserve_resources(stamp, 0, &request())
            });
            let second = scope.spawn(move || {
                right.wait();
                second.reserve_resources(stamp, 0, &request())
            });
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Err(ReservationError::Conflict)))
                .count(),
            1
        );
        let database = fixture.open();
        let snapshot = database.resource_reservations().unwrap().unwrap();
        assert_eq!(snapshot.slots.len(), 1);
        assert_eq!(snapshot.reserved_total().unwrap(), policy.aggregate_ceiling);
        assert!(matches!(
            database.reserve_resources(&policy.stamp, snapshot.revision, &request()),
            Err(ReservationError::Capacity)
        ));
    }

    #[test]
    fn expired_alive_or_unknown_owner_retains_budget_and_dead_unconfirmed_retires() {
        let fixture = Fixture::new();
        let database = fixture.open();
        let policy = policy();
        database.initialize_resource_policy(&policy).unwrap();
        let ticket = database
            .reserve_resources(&policy.stamp, 0, &request())
            .unwrap();
        let before = raw(&database);
        for state in [
            OwnerObservation::AliveExact,
            OwnerObservation::Unknown,
            OwnerObservation::OtherWindowsSession,
        ] {
            let observation = ReconcileObservation {
                expected: ticket.clone(),
                owner: state,
                observed_at_ms: 2000,
                closed_empty_job: None,
            };
            let snapshot = database
                .reconcile_reservations(&policy.stamp, &[observation], 2000)
                .unwrap();
            assert_eq!(snapshot.slots.as_slice(), std::slice::from_ref(&ticket));
            assert_eq!(raw(&database), before);
        }
        let observation = ReconcileObservation {
            expected: ticket,
            owner: OwnerObservation::DeadExact,
            observed_at_ms: 2000,
            closed_empty_job: None,
        };
        let retired = database
            .reconcile_reservations(&policy.stamp, &[observation], 2000)
            .unwrap();
        assert_eq!(retired.slots[0].state, ReservationState::Retired);
        assert_eq!(retired.reserved_total().unwrap(), policy.aggregate_ceiling);
        assert!(matches!(
            database.reserve_resources(&policy.stamp, retired.revision, &request()),
            Err(ReservationError::Capacity)
        ));
        let proof = EmptyJobProof::from_trusted_closed_empty_job(
            &retired.slots[0],
            &retired.slots[0].owner,
            &retired.slots[0].job_key,
            2000,
        )
        .unwrap();
        let observation = ReconcileObservation {
            expected: retired.slots[0].clone(),
            owner: OwnerObservation::DeadExact,
            observed_at_ms: 2000,
            closed_empty_job: Some(proof),
        };
        assert!(
            database
                .reconcile_reservations(&policy.stamp, &[observation], 2000)
                .unwrap()
                .slots
                .is_empty()
        );
    }

    #[test]
    fn transitions_require_exact_ticket_and_closed_empty_proof_does_not_survive_aba() {
        let fixture = Fixture::new();
        let database = fixture.open();
        let policy = policy();
        database.initialize_resource_policy(&policy).unwrap();
        let request = request();
        let ticket = database
            .reserve_resources(&policy.stamp, 0, &request)
            .unwrap();
        let active = database
            .activate_reservation(&policy.stamp, &ticket, 200, 1200)
            .unwrap();
        assert_eq!(active.state, ReservationState::Active);
        assert!(matches!(
            database.retire_reservation(&policy.stamp, &ticket),
            Err(ReservationError::Conflict)
        ));
        let wrong = EmptyJobProof::from_trusted_closed_empty_job(
            &ticket,
            &ticket.owner,
            &ticket.job_key,
            300,
        )
        .unwrap();
        assert!(matches!(
            database.release_reservation(&policy.stamp, &active, &wrong, 300),
            Err(ReservationError::ProofRequired)
        ));
        let retired = database.retire_reservation(&policy.stamp, &active).unwrap();
        let mut other_session = retired.owner.clone();
        other_session.windows_session_id += 1;
        assert!(
            EmptyJobProof::from_trusted_closed_empty_job(
                &retired,
                &other_session,
                &retired.job_key,
                300
            )
            .is_err()
        );
        let proof = EmptyJobProof::from_trusted_closed_empty_job(
            &retired,
            &retired.owner,
            &retired.job_key,
            300,
        )
        .unwrap();
        let released = database
            .release_reservation(&policy.stamp, &retired, &proof, 300)
            .unwrap();
        let replacement = database
            .reserve_resources(&policy.stamp, released.revision, &request)
            .unwrap();
        assert!(replacement.revision > retired.revision);
        assert!(matches!(
            database.release_reservation(&policy.stamp, &replacement, &proof, 300),
            Err(ReservationError::ProofRequired)
        ));
        let mut wrong_owner = replacement.clone();
        wrong_owner.owner.created_at_100ns += 1;
        assert!(matches!(
            database.retire_reservation(&policy.stamp, &wrong_owner),
            Err(ReservationError::Conflict)
        ));
    }

    #[test]
    fn a_caller_cannot_raise_ceilings_or_replace_the_frozen_policy() {
        let fixture = Fixture::new();
        let database = fixture.open();
        let policy = policy();
        database.initialize_resource_policy(&policy).unwrap();
        let before = raw(&database);
        let mut higher = policy.clone();
        higher.aggregate_ceiling.memory_bytes *= 2;
        assert!(matches!(
            database.initialize_resource_policy(&higher),
            Err(ReservationError::PolicyMismatch)
        ));
        let mut changed = policy.stamp.clone();
        changed.version += 1;
        assert!(matches!(
            database.reserve_resources(&changed, 0, &request()),
            Err(ReservationError::PolicyMismatch)
        ));
        assert_eq!(raw(&database), before);
    }

    #[test]
    fn malformed_and_oversized_metadata_is_preserved_without_partial_mutations() {
        let fixture = Fixture::new();
        let database = fixture.open();
        let policy = policy();
        database
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO setting(key,value) VALUES(?1,?2)",
                params![SETTING_KEY, "{invalid"],
            )
            .unwrap_err();
        // JSON-valid malformed content, including an oversize value, is accepted
        // only by this raw fixture writer; the ledger API never repairs it.
        for json in [
            "{}".to_string(),
            format!("{{\"padding\":\"{}\"}}", "x".repeat(MAX_LEDGER_BYTES)),
        ] {
            database.lock().unwrap().execute("INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![SETTING_KEY,json]).unwrap();
            assert!(matches!(
                database.resource_reservations(),
                Err(ReservationError::Malformed)
            ));
            assert!(matches!(
                database.initialize_resource_policy(&policy),
                Err(ReservationError::Malformed)
            ));
            assert_eq!(raw(&database), json);
        }
    }

    #[test]
    fn metadata_cannot_hide_a_sixty_fifth_slot_inside_the_byte_budget() {
        let fixture = Fixture::new();
        let database = fixture.open();
        let mut policy = policy();
        policy.aggregate_ceiling = Budget {
            memory_bytes: 1024 * 100,
            processes: 100,
            cpu_basis_points: 10000,
            queue_bytes: 6400,
            queue_items: 200,
        };
        let ledger = ReservationSnapshot {
            format_version: 1,
            revision: 65,
            policy,
            slots: (0..65)
                .map(|index| ReservationTicket {
                    id: uuid::Uuid::new_v4().to_string(),
                    job_key: uuid::Uuid::new_v4().to_string(),
                    owner: owner(),
                    budget: budget(),
                    state: ReservationState::Reserved,
                    lease_expires_at_ms: 1000,
                    revision: index + 1,
                    advisory: false,
                })
                .collect(),
        };
        let json = serde_json::to_string(&ledger).unwrap();
        assert!(json.len() < MAX_LEDGER_BYTES);
        database
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO setting(key,value) VALUES(?1,?2)",
                params![SETTING_KEY, json],
            )
            .unwrap();
        assert!(matches!(
            database.resource_reservations(),
            Err(ReservationError::Malformed)
        ));
        assert_eq!(raw(&database), json);
    }

    #[test]
    fn a_stale_writer_cannot_change_reservations_after_schema_999() {
        let fixture = Fixture::new();
        let database = fixture.open();
        let policy = policy();
        database.initialize_resource_policy(&policy).unwrap();
        let ticket = database
            .reserve_resources(&policy.stamp, 0, &request())
            .unwrap();
        let proof = EmptyJobProof::from_trusted_closed_empty_job(
            &ticket,
            &ticket.owner,
            &ticket.job_key,
            200,
        )
        .unwrap();
        let observation = ReconcileObservation {
            expected: ticket.clone(),
            owner: OwnerObservation::DeadExact,
            observed_at_ms: 200,
            closed_empty_job: Some(proof.clone()),
        };
        let before = raw(&database);
        let raw_database =
            rusqlite::Connection::open(fixture.location.as_ref().unwrap().database_path()).unwrap();
        raw_database
            .pragma_update(None, "user_version", 999)
            .unwrap();
        for result in [
            database.initialize_resource_policy(&policy).map(|_| ()),
            database
                .reserve_resources(&policy.stamp, 1, &request())
                .map(|_| ()),
            database
                .activate_reservation(&policy.stamp, &ticket, 200, 1200)
                .map(|_| ()),
            database
                .renew_reservation(&policy.stamp, &ticket, 200, 1200)
                .map(|_| ()),
            database
                .retire_reservation(&policy.stamp, &ticket)
                .map(|_| ()),
            database
                .release_reservation(&policy.stamp, &ticket, &proof, 200)
                .map(|_| ()),
            database
                .reconcile_reservations(&policy.stamp, &[observation], 200)
                .map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(ReservationError::Database(DatabaseError::NewerSchema {
                    found: 999,
                    ..
                }))
            ));
        }
        assert_eq!(raw(&database), before);
    }

    #[test]
    fn an_observation_cannot_release_a_ticket_renewed_after_the_native_probe() {
        let fixture = Fixture::new();
        let database = fixture.open();
        let policy = policy();
        database.initialize_resource_policy(&policy).unwrap();
        let ticket = database
            .reserve_resources(&policy.stamp, 0, &request())
            .unwrap();
        let proof = EmptyJobProof::from_trusted_closed_empty_job(
            &ticket,
            &ticket.owner,
            &ticket.job_key,
            200,
        )
        .unwrap();
        let observed = ReconcileObservation {
            expected: ticket.clone(),
            owner: OwnerObservation::DeadExact,
            observed_at_ms: 200,
            closed_empty_job: Some(proof),
        };
        let renewed = database
            .renew_reservation(&policy.stamp, &ticket, 200, 1200)
            .unwrap();
        let before = raw(&database);
        assert!(matches!(
            database.reconcile_reservations(&policy.stamp, &[observed], 200),
            Err(ReservationError::Conflict)
        ));
        assert_eq!(raw(&database), before);
        assert_eq!(
            database.resource_reservations().unwrap().unwrap().slots,
            [renewed]
        );
    }
}
