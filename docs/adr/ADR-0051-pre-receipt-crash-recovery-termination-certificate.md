---
adr_id: ADR-0051
version: 1
title: Pre-receipt native crash recovery is unlocked only by a qualified whole-group termination certificate derived from the named kill-on-close job
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - crates/engine/src/resources/job.rs
    - crates/state/src/native_session_cleanup.rs
    - crates/state/src/records/recovery.rs
    - apps/desktop/src-tauri/src/agents/thread_lifecycle.rs
    - apps/desktop/src-tauri/src/agents/threads.rs
  modules:
  domains:
    - native-containment
    - crash-recovery
    - resource-admission
governs_symbols:
depends_on:
  - ADR-0050
created_from_task: forced-crash-recovery-2026-11-04
proposed_on: 2026-11-04
proposed_at_sha: c9deab5bbf161c9aa04d21b8d19d245f42394811
accepted_on:
---

# ADR-0051: Pre-receipt native crash recovery is unlocked only by a qualified whole-group termination certificate derived from the named kill-on-close job

## Context and Problem Statement

The durable-receipt increment (`docs/notes/2026-11-03-native-cleanup-recovery.md`)
made the window between proven native cleanup and interrupted finalization
recoverable: `Recover session` consumes the receipt and closes the exact
activation once. A crash **before** the receipt remains blocked by design
(`docs/notes/2026-11-03-native-owner-recovery-gap.md`): the activation stays
open, the thread stays `recorded_open`, and the dead host's ledger ticket keeps
capacity reserved. The real-crash proof
(`apps/desktop/tests/forced-crash-recovery-smoke.mjs`) demonstrates exactly this
state on a force-terminated app: refusal with "shutdown was not confirmed",
activation and budget preserved. Today the only outcome for such a thread is
manual database surgery, which the product must never require or invite.

The blocker is evidentiary, not mechanical: after the host dies, the in-memory
job handle census is lost, and the previous policy refused "job absent by name"
as complete cleanup proof because nothing tied that absence to a guaranteed
group termination.

## Decision Drivers

- No unlock by timeout, elapsed time, PID alone, ticket absence, or name
  absence without a qualification chain.
- OS-guaranteed semantics beat reconstruction: the certificate must come from
  properties Windows enforces, not from re-deriving who was inside the group.
- Recovery stays explicit, per thread, revision-scoped, and single-closing.
- Reserved capacity of dead hosts must become recoverable together with the
  activation, atomically, never independently.
- Historical records without the durable association stay permanently blocked
  and recognizable; no completion by assumption.

## Decision

Introduce a **qualified whole-group termination certificate** as the only new
unlock evidence for pre-receipt crashes, produced by an extension of the
existing explicit recovery request (same UI seam, same selectors plus expected
revision). The durable association (activation owner, host identity,
reservation ticket including `job_key`) is the sole input; `job_key` is never
accepted from IPC or UI.

Qualification chain, all steps mandatory:

1. **Exact identity observation**: old host process and agent process are dead
   or PID-reused (FILETIME-exact `probe_owner_identity`), Windows session
   matches the recorded owner session. Alive or uncertain identities block.
2. **Recipe pinning**: the recorded ticket budget must equal the creation
   recipe limits, which are fixed to include `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
   and to exclude any breakaway flag (`extended()` in
   `crates/engine/src/resources/job.rs`; creation re-queries and verifies the
   applied limits, and name collision is terminal). Membership escape was
   therefore impossible for the whole life of the job.
3. **Job probe by derived name** `Local\Shinatama.Workload.<hex(job_key)>`
   (128-bit per-reservation secret key; creation-time collision refusal):
   - **Name absent** → every handle to the job has closed; with
     kill-on-close pinned by step 2, Windows terminated all remaining members
     at the last close. Certificate kind `JobNameAbsent`. No census is needed:
     the termination fact is the OS invariant, not a reconstruction.
   - **Name present** → open with `JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE`
     (default owner DACL, same user), enumerate members,
     `TerminateJobObject`, poll accounting until `ActiveProcesses == 0` with
     the existing rundown proof, close the recovery handle, then require the
     name to be absent. Certificate kind `TerminatedByRecovery`. Any partial
     or uncertain observation blocks without a certificate.
   - **Access denied or unexpected OS error** → blocked, explicitly recorded.
4. **Durable receipt**: on certificate success only, write a termination
   receipt into the existing cleanup journal, bound to the exact association
   (instance, host, agent, ticket, job key digest, certificate kind), in the
   same transaction shape as the proven-cleanup receipt.
5. **Single closing**: the existing recovery transaction then compares the
   whole association, activation revision and thread revision, closes the
   activation exactly once (unique partial indexes unchanged), consumes only
   this receipt, and releases the dead host's ledger ticket in the same
   transaction. Resume remains a separate action with its existing native
   verifications.

No general reaper, no startup sweep, no background termination: the protocol
runs only inside an explicit user recovery request for one thread.

## Rejected Alternatives

- **Startup reconcile/sweep of dead-host records** (`reconcile_dead_owner`,
  `reconcile_reservations` at boot): implicit, broad blast radius, and it still
  lacks group-termination proof; explicitly excluded by the gap note.
- **Lease expiry or time-based unlock**: time never proves termination of the
  exact group.
- **PID/name kills without the durable association**: no authority chain from
  the recorded owner to the observed process.
- **Ticket absence as receipt**: already rejected; absence is ambiguous.
- **Filesystem/transcript reconstruction of the census**: the census is a
  handle-backed runtime fact; files prove conversation, not termination.

## Enforceable Invariants

- A termination receipt exists only with kind `JobNameAbsent` or
  `TerminatedByRecovery`, an exact association match, and step 2 recipe
  equality; it is consumed only by the single closing transaction.
- `TerminatedByRecovery` requires observed rundown to zero active processes
  before the recovery handle closes, and name absence afterwards.
- Recovery never starts, signals or terminates anything outside the job named
  by the durable ticket; the agent process is never killed by name or bare PID.
- Blocked outcomes (alive owner, uncertain identity, missing association,
  denied job open) preserve activation, journal entry and ledger slot
  unchanged; retries remain possible and idempotent.
- Records predating the durable association stay blocked and recognizable.

## Consequences

- Threads crashed before the receipt become recoverable without weakening any
  existing refusal; dead-host capacity is reclaimed atomically with closing.
- New engine primitives (recovery-mode job open with TERMINATE, certificate
  kinds) need their own unit tests: name-absent certificate, live-job
  terminate-and-rundown, orphaned-helper holder, access-denied, recipe
  mismatch, concurrent recovery single-closing.
- The real-crash smoke extends with a pre-receipt recovery leg (currently it
  proves the blocked state) and an orphaned-terminal-helper variant where the
  job name survives the host crash.
- The conservative stance of the gap note is preserved: what changes is the
  evidence available, not the burden of proof.

## Confirmation

Proposed; awaiting user approval. Acceptance evidence will be: engine/state
unit tests for every certificate and refusal path, plus a passing real-crash
proof on a pinned build showing pre-receipt recovery closing exactly one
activation, consuming only its certificate-backed receipt and releasing only
the dead host's ticket.
