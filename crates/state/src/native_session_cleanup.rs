//! Durable join between a native session and its admitted workload. The receipt
//! is written in the reservation-release transaction, never inferred from a
//! missing ticket, elapsed time, SessionEnd or process death alone.
//! Unfinished records are bounded and never evicted to admit another process.

use crate::{
    database::{Database, DatabaseError},
    records::{
        CanonicalIdKind, MAX_SAFE_REVISION, WorkspaceOwner,
        recovery::{DeadOwnerIdentityProof, OwnerObservation, probe_owner_identity},
        validate_id,
    },
    resource_reservations::{
        PolicyStamp, ReservationError, ReservationState, ReservationTicket, read_ledger,
        write_ledger,
    },
    threads::{self, ActivationOwner, ActivationRecord, ThreadError},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

pub const SETTING_KEY: &str = "operational.native_session_cleanup.v1";
pub const MAX_RECORDS: usize = 64;
pub const MAX_BYTES: usize = 128 * 1024;

#[derive(Debug)]
pub enum CleanupError {
    Invalid,
    Conflict,
    ReceiptRequired,
    Capacity,
    Malformed,
    Storage,
}
impl fmt::Display for CleanupError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(match self {
            Self::Invalid => "The native cleanup identity is invalid.",
            Self::Conflict => "The native session changed; reload before recovery.",
            Self::ReceiptRequired => {
                "The exact native cleanup receipt is unavailable; saved ownership is preserved."
            }
            Self::Capacity => {
                "Unfinished native cleanup records reached their bound; new starts are deferred."
            }
            Self::Malformed => {
                "Saved native cleanup records are unverifiable; existing records are preserved."
            }
            Self::Storage => {
                "Native cleanup storage is unavailable; existing records are preserved."
            }
        })
    }
}
impl std::error::Error for CleanupError {}
impl From<DatabaseError> for CleanupError {
    fn from(_: DatabaseError) -> Self {
        Self::Storage
    }
}
impl From<rusqlite::Error> for CleanupError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
impl From<ReservationError> for CleanupError {
    fn from(error: ReservationError) -> Self {
        match error {
            ReservationError::Conflict | ReservationError::PolicyMismatch => Self::Conflict,
            ReservationError::NotFound | ReservationError::ProofRequired => Self::ReceiptRequired,
            ReservationError::Malformed => Self::Malformed,
            ReservationError::Capacity => Self::Capacity,
            ReservationError::Invalid | ReservationError::InvalidTransition => Self::Invalid,
            _ => Self::Storage,
        }
    }
}
impl From<ThreadError> for CleanupError {
    fn from(error: ThreadError) -> Self {
        match error {
            ThreadError::Invalid => Self::Invalid,
            ThreadError::Storage => Self::Storage,
            _ => Self::Conflict,
        }
    }
}
pub type Result<T> = std::result::Result<T, CleanupError>;

/// Immutable registration made at ShellReady, while the exact admitted ticket
/// is Active and before the shell is resumed. This is metadata, not a permit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSessionOwner {
    pub activation: ActivationOwner,
    pub policy: PolicyStamp,
    pub reservation: ReservationTicket,
    pub registered_at_ms: i64,
}

/// Loaded internally from the private database. Recovery additionally requires
/// native identity proofs and a complete CAS against this exact saved receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCleanupReceipt {
    pub owner: NativeSessionOwner,
    pub released_ticket: ReservationTicket,
    pub released_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupEvidence {
    released_ticket: ReservationTicket,
    released_at_ms: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    owner: NativeSessionOwner,
    cleanup: Option<CleanupEvidence>,
}
impl Entry {
    fn receipt(&self) -> Option<NativeCleanupReceipt> {
        self.cleanup.as_ref().map(|cleanup| NativeCleanupReceipt {
            owner: self.owner.clone(),
            released_ticket: cleanup.released_ticket.clone(),
            released_at_ms: cleanup.released_at_ms,
        })
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    format_version: u8,
    entries: Vec<Entry>,
}
fn valid_time(value: i64) -> bool {
    value >= 0 && value as u64 <= MAX_SAFE_REVISION
}
fn same_workload(first: &ReservationTicket, second: &ReservationTicket) -> bool {
    first.id == second.id
        && first.job_key == second.job_key
        && first.owner == second.owner
        && first.budget == second.budget
}
impl NativeSessionOwner {
    fn validate(&self) -> Result<()> {
        self.activation.validate()?;
        self.policy.validate()?;
        self.reservation.validate()?;
        if self.reservation.state != ReservationState::Active || !valid_time(self.registered_at_ms)
        {
            return Err(CleanupError::Invalid);
        }
        Ok(())
    }
}
impl NativeCleanupReceipt {
    fn validate(&self) -> Result<()> {
        self.owner.validate()?;
        self.released_ticket.validate()?;
        if !same_workload(&self.owner.reservation, &self.released_ticket)
            || self.released_ticket.revision < self.owner.reservation.revision
            || self.released_ticket.state == ReservationState::Reserved
            || !valid_time(self.released_at_ms)
            || self.released_at_ms < self.owner.registered_at_ms
        {
            return Err(CleanupError::Invalid);
        }
        Ok(())
    }
}
impl Journal {
    fn validate(&self) -> Result<()> {
        if self.format_version != 1 || self.entries.len() > MAX_RECORDS {
            return Err(CleanupError::Malformed);
        }
        let mut instances = BTreeSet::new();
        let mut tickets = BTreeSet::new();
        let mut jobs = BTreeSet::new();
        for entry in &self.entries {
            entry
                .owner
                .validate()
                .map_err(|_| CleanupError::Malformed)?;
            if let Some(receipt) = entry.receipt() {
                receipt.validate().map_err(|_| CleanupError::Malformed)?;
            }
            if !instances.insert(&entry.owner.activation.instance_id)
                || !tickets.insert(&entry.owner.reservation.id)
                || !jobs.insert(&entry.owner.reservation.job_key)
            {
                return Err(CleanupError::Malformed);
            }
        }
        Ok(())
    }
}
fn read(connection: &Connection) -> Result<Journal> {
    let json: Option<Option<String>> = connection
        .query_row(
            "SELECT CASE WHEN octet_length(value)<=?2 THEN value END FROM setting WHERE key=?1",
            params![SETTING_KEY, MAX_BYTES as i64],
            |row| row.get(0),
        )
        .optional()?;
    let Some(json) = json else {
        return Ok(Journal {
            format_version: 1,
            entries: vec![],
        });
    };
    let journal: Journal = serde_json::from_str(&json.ok_or(CleanupError::Malformed)?)
        .map_err(|_| CleanupError::Malformed)?;
    journal.validate()?;
    Ok(journal)
}
fn write(connection: &Connection, journal: &Journal) -> Result<()> {
    journal.validate()?;
    let json = serde_json::to_string(journal).map_err(|_| CleanupError::Malformed)?;
    if json.len() > MAX_BYTES {
        return Err(CleanupError::Capacity);
    }
    connection.execute(
        "INSERT INTO setting(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![SETTING_KEY, json],
    )?;
    Ok(())
}

/// Called only inside the same transaction that consumes EmptyJobProof and
/// removes this ticket. Legacy/untracked reservations have no session join.
pub(crate) fn record_cleanup(
    connection: &Connection,
    stamp: &PolicyStamp,
    ticket: &ReservationTicket,
    at: i64,
) -> std::result::Result<(), ReservationError> {
    let operation = || -> Result<()> {
        let mut journal = read(connection)?;
        let Some(entry) = journal
            .entries
            .iter_mut()
            .find(|entry| entry.owner.reservation.id == ticket.id)
        else {
            return Ok(());
        };
        if entry.owner.policy != *stamp
            || !same_workload(&entry.owner.reservation, ticket)
            || entry.cleanup.is_some()
        {
            return Err(CleanupError::Conflict);
        }
        entry.cleanup = Some(CleanupEvidence {
            released_ticket: ticket.clone(),
            released_at_ms: at,
        });
        write(connection, &journal)
    };
    operation().map_err(|error| match error {
        CleanupError::Conflict => ReservationError::Conflict,
        CleanupError::Capacity => ReservationError::Capacity,
        _ => ReservationError::Malformed,
    })
}

fn receipt_index(
    connection: &Connection,
    journal: &Journal,
    receipt: &NativeCleanupReceipt,
) -> Result<usize> {
    receipt.validate()?;
    let index = journal
        .entries
        .iter()
        .position(|entry| {
            entry.owner.activation.instance_id == receipt.owner.activation.instance_id
        })
        .ok_or(CleanupError::ReceiptRequired)?;
    if journal.entries[index].receipt().as_ref() != Some(receipt) {
        return Err(CleanupError::Conflict);
    }
    let ledger = read_ledger(connection)?.ok_or(CleanupError::ReceiptRequired)?;
    ledger.require_policy(&receipt.owner.policy)?;
    if ledger.slots.iter().any(|ticket| {
        ticket.id == receipt.released_ticket.id || ticket.job_key == receipt.released_ticket.job_key
    }) {
        return Err(CleanupError::Conflict);
    }
    Ok(index)
}
fn finish(
    connection: &Connection,
    receipt: &NativeCleanupReceipt,
    expected: &ActivationRecord,
    at: i64,
) -> Result<ActivationRecord> {
    if expected.owner != receipt.owner.activation
        || expected.ended_at.is_some()
        || !valid_time(at)
        || at < receipt.released_at_ms
    {
        return Err(CleanupError::Invalid);
    }
    let mut journal = read(connection)?;
    let index = receipt_index(connection, &journal, receipt)?;
    if threads::activation(connection, &expected.owner.instance_id)? != *expected {
        return Err(CleanupError::Conflict);
    }
    let ended = threads::end_activation(
        connection,
        &expected.owner,
        Some(expected.sequence),
        expected.revision,
        at,
    )?;
    journal.entries.remove(index);
    write(connection, &journal)?;
    Ok(ended)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminationCertificateKind {
    /// The durable job name is absent: with the pinned kill-on-close recipe,
    /// the last handle close terminated the whole group already.
    JobNameAbsent,
    /// The named job was terminated by recovery, rundown reached zero active
    /// processes and the name is absent afterwards.
    TerminatedByRecovery,
}

/// ADR-0051: a whole-group termination certificate for the pre-receipt crash
/// window. Built only after the exact host and agent identities were observed
/// dead or PID-reused and the named kill-on-close job probe produced one of the
/// two kinds. Not Clone, Serialize or Deserialize. Never supplied by UI, hook
/// or a PID field.
pub struct NativeTerminationCertificate {
    pub owner: NativeSessionOwner,
    pub kind: TerminationCertificateKind,
    pub at: i64,
}

impl Database {
    /// ADR-0051 recovery for a crash before any cleanup receipt. Releases the
    /// dead host's ticket, writes the normal cleanup receipt, closes the exact
    /// activation and consumes the association in ONE transaction. Existing
    /// receipts belong to the normal path and are never re-consumed here.
    pub fn certify_native_session_termination(
        &self,
        certificate: &NativeTerminationCertificate,
        expected_thread_revision: u64,
        expected: &ActivationRecord,
        at: i64,
    ) -> Result<ActivationRecord> {
        certificate.owner.validate()?;
        if !valid_time(certificate.at) || at < certificate.at || expected.ended_at.is_some() {
            return Err(CleanupError::Invalid);
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let thread = threads::load_thread(&tx, &expected.thread_id)?;
        if thread.revision != expected_thread_revision || thread.archived {
            return Err(CleanupError::Conflict);
        }
        if threads::activation(&tx, &expected.owner.instance_id)? != *expected {
            return Err(CleanupError::Conflict);
        }
        let mut journal = read(&tx)?;
        let index = journal
            .entries
            .iter()
            .position(|entry| entry.owner.activation.instance_id == expected.owner.instance_id)
            .ok_or(CleanupError::ReceiptRequired)?;
        if journal.entries[index].owner != certificate.owner || journal.entries[index].cleanup.is_some() {
            return Err(CleanupError::Conflict);
        }
        let owner = journal.entries[index].owner.clone();
        let mut ledger = read_ledger(&tx)?.ok_or(CleanupError::ReceiptRequired)?;
        ledger.require_policy(&owner.policy)?;
        let ticket_index = ledger.ticket_index(&owner.reservation)?;
        // The certificate substitutes EmptyJobProof ONLY for this exact record:
        // the named kill-on-close job proved the whole group terminated. Write
        // the same receipt a normal release would write, then release capacity.
        record_cleanup(&tx, &owner.policy, &owner.reservation, certificate.at)?;
        ledger.slots.remove(ticket_index);
        ledger.next_revision()?;
        write_ledger(&tx, &ledger)?;
        let ended = threads::end_activation(
            &tx,
            &expected.owner,
            Some(expected.sequence),
            expected.revision,
            at,
        )?;
        journal.entries.remove(index);
        write(&tx, &journal)?;
        tx.commit()?;
        Ok(ended)
    }
}
impl Database {
    pub fn bind_native_session_owner(
        &self,
        ticket: &ReservationTicket,
        activation: &ActivationOwner,
        now_ms: i64,
    ) -> Result<NativeSessionOwner> {
        activation.validate()?;
        ticket.validate()?;
        if ticket.state != ReservationState::Active || !valid_time(now_ms) {
            return Err(CleanupError::Invalid);
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let ledger = read_ledger(&tx)?.ok_or(CleanupError::ReceiptRequired)?;
        ledger.ticket_index(ticket)?;
        let owner = NativeSessionOwner {
            activation: activation.clone(),
            policy: ledger.policy.stamp.clone(),
            reservation: ticket.clone(),
            registered_at_ms: now_ms,
        };
        let mut journal = read(&tx)?;
        if let Some(existing) = journal
            .entries
            .iter()
            .find(|entry| entry.owner.activation.instance_id == activation.instance_id)
        {
            if existing.cleanup.is_some()
                || existing.owner.activation != *activation
                || existing.owner.reservation != *ticket
                || existing.owner.policy != owner.policy
            {
                return Err(CleanupError::Conflict);
            }
            return Ok(existing.owner.clone());
        }
        if journal.entries.len() >= MAX_RECORDS {
            return Err(CleanupError::Capacity);
        }
        if journal.entries.iter().any(|entry| {
            entry.owner.reservation.id == ticket.id
                || entry.owner.reservation.job_key == ticket.job_key
        }) || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM activation WHERE instance_id=?1)",
            [&activation.instance_id],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(CleanupError::Conflict);
        }
        journal.entries.push(Entry {
            owner: owner.clone(),
            cleanup: None,
        });
        // Reserve serialization room for every future receipt now. A valid
        // cleanup must never fail merely because all pending rows became receipts.
        let worst = Journal {
            format_version: 1,
            entries: journal
                .entries
                .iter()
                .map(|entry| {
                    let mut entry = entry.clone();
                    if entry.cleanup.is_none() {
                        let mut released = entry.owner.reservation.clone();
                        released.state = ReservationState::Retired;
                        released.revision = MAX_SAFE_REVISION;
                        released.lease_expires_at_ms = i64::MAX;
                        entry.cleanup = Some(CleanupEvidence {
                            released_ticket: released,
                            released_at_ms: MAX_SAFE_REVISION as i64,
                        });
                    }
                    entry
                })
                .collect(),
        };
        if serde_json::to_vec(&worst)
            .map_err(|_| CleanupError::Malformed)?
            .len()
            > MAX_BYTES
        {
            return Err(CleanupError::Capacity);
        }
        write(&tx, &journal)?;
        tx.commit()?;
        Ok(owner)
    }

    /// Bounded exact join lookup for the owning runtime's idempotent cleanup.
    /// Absence does not prove native cleanup or authorize recovery.
    pub fn native_session_owner(&self, instance_id: &str) -> Result<Option<NativeSessionOwner>> {
        validate_id(instance_id, CanonicalIdKind::Instance).map_err(|_| CleanupError::Invalid)?;
        let connection = self.lock()?;
        crate::schema::require_write_version(&connection)?;
        Ok(read(&connection)?
            .entries
            .iter()
            .find(|entry| entry.owner.activation.instance_id == instance_id)
            .map(|entry| entry.owner.clone()))
    }

    pub fn native_session_cleanup(
        &self,
        instance_id: &str,
    ) -> Result<Option<NativeCleanupReceipt>> {
        validate_id(instance_id, CanonicalIdKind::Instance).map_err(|_| CleanupError::Invalid)?;
        let connection = self.lock()?;
        crate::schema::require_write_version(&connection)?;
        Ok(read(&connection)?
            .entries
            .iter()
            .find(|entry| entry.owner.activation.instance_id == instance_id)
            .and_then(Entry::receipt))
    }

    /// Normal owner callback: native cleanup was already proven by its own job.
    /// This closes the exact activation and consumes its receipt in one commit.
    pub fn finish_native_session_cleanup(
        &self,
        receipt: &NativeCleanupReceipt,
        expected: &ActivationRecord,
        at: i64,
    ) -> Result<ActivationRecord> {
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let ended = finish(&tx, receipt, expected, at)?;
        tx.commit()?;
        Ok(ended)
    }

    /// Covers a before_run failure with no interval, or an already finalized
    /// exact owner. An open interval, a foreign identity or a missing receipt
    /// prevents acknowledgement. No time-based garbage collection is used.
    pub fn acknowledge_native_session_cleanup(&self, receipt: &NativeCleanupReceipt) -> Result<()> {
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let mut journal = read(&tx)?;
        let index = receipt_index(&tx, &journal, receipt)?;
        let unsafe_interval: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM activation WHERE instance_id=?1 AND (ended_at IS NULL OR owner_pid!=?2 OR owner_created_at_100ns!=?3))",
            params![receipt.owner.activation.instance_id, receipt.owner.activation.pid, receipt.owner.activation.created_at_100ns as i64], |row| row.get(0),
        )?;
        if unsafe_interval {
            return Err(CleanupError::Conflict);
        }
        journal.entries.remove(index);
        write(&tx, &journal)?;
        tx.commit()?;
        Ok(())
    }

    pub fn recover_native_session_cleanup(
        &self,
        proof: &NativeSessionRecoveryProof,
        expected_thread_revision: u64,
        expected: &ActivationRecord,
        at: i64,
    ) -> Result<ActivationRecord> {
        if !proof.host.matches(&host_identity(&proof.receipt))
            || !proof.shell.matches(&shell_identity(&proof.receipt))
            || !same_windows_session(proof.receipt.owner.reservation.owner.windows_session_id)
        {
            return Err(CleanupError::Invalid);
        }
        self.require_writable()?;
        let mut connection = self.lock()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        crate::schema::require_write_version(&tx)?;
        let thread = threads::load_thread(&tx, &expected.thread_id)?;
        if thread.revision != expected_thread_revision || thread.archived {
            return Err(CleanupError::Conflict);
        }
        let ended = finish(&tx, &proof.receipt, expected, at)?;
        tx.commit()?;
        Ok(ended)
    }
}

pub struct NativeCleanupProbe {
    pub host: OwnerObservation,
    pub shell: OwnerObservation,
    pub same_windows_session: bool,
    pub proof: Option<NativeSessionRecoveryProof>,
}
/// Carries two irreversible identity observations and the exact durable receipt.
/// Not Clone, Serialize or Deserialize. Never supplied by UI, hook or a PID field.
pub struct NativeSessionRecoveryProof {
    receipt: NativeCleanupReceipt,
    host: DeadOwnerIdentityProof,
    shell: DeadOwnerIdentityProof,
}
fn host_identity(receipt: &NativeCleanupReceipt) -> WorkspaceOwner {
    WorkspaceOwner {
        window_id: receipt.owner.activation.instance_id.clone(),
        pid: receipt.owner.reservation.owner.pid,
        start_time: format!("win-{}", receipt.owner.reservation.owner.created_at_100ns),
    }
}
fn shell_identity(receipt: &NativeCleanupReceipt) -> WorkspaceOwner {
    WorkspaceOwner {
        window_id: receipt.owner.activation.instance_id.clone(),
        pid: receipt.owner.activation.pid,
        start_time: format!("win-{}", receipt.owner.activation.created_at_100ns),
    }
}
fn same_windows_session(expected: u32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::{
            RemoteDesktop::ProcessIdToSessionId, Threading::GetCurrentProcessId,
        };
        let mut current = 0;
        unsafe {
            ProcessIdToSessionId(GetCurrentProcessId(), &mut current) != 0 && current == expected
        }
    }
    #[cfg(not(windows))]
    {
        let _ = expected;
        false
    }
}
/// Bounded native observations outside the transaction, without terminating or
/// signalling anything. A receipt alone does not authorize recovery from a live
/// former host; normal cleanup uses its separate owning-runtime callback.
pub fn probe_native_session_cleanup(receipt: &NativeCleanupReceipt) -> Result<NativeCleanupProbe> {
    receipt.validate()?;
    let host = probe_owner_identity(&host_identity(receipt)).map_err(|_| CleanupError::Invalid)?;
    let shell =
        probe_owner_identity(&shell_identity(receipt)).map_err(|_| CleanupError::Invalid)?;
    let same_windows_session =
        same_windows_session(receipt.owner.reservation.owner.windows_session_id);
    let proof = match (same_windows_session, host.proof, shell.proof) {
        (true, Some(host), Some(shell)) => Some(NativeSessionRecoveryProof {
            receipt: receipt.clone(),
            host,
            shell,
        }),
        _ => None,
    };
    Ok(NativeCleanupProbe {
        host: host.observation,
        shell: shell.observation,
        same_windows_session,
        proof,
    })
}

#[cfg(all(test, windows))]
mod tests;
