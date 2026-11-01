---
adr_id: ADR-0018
version: 1
title: Journaled hop state machine with nonce receipt; recovery observes and never relaunches
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - hop-state-machine
    - hop-journal
    - crash-recovery
    - hop-receipt
    - hop-failure-handling
governs_symbols:
depends_on:
  - ADR-0009
  - ADR-0015
  - ADR-0017
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0018: Journaled hop state machine with nonce receipt; recovery observes and never relaunches

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. A hop spawns a process, writes files and switches the user's focus
across several processes that can crash at any point. The question this record answers is how a
hop progresses, when the destination counts as ready, and what recovery may do after a crash.

Origin: the state machine of technical-design 6.3 and the failure matrix of 6.7 are the author's
proposed shape; the READY timeout and the UNDO are tagged "[C]" (author's call, not confirmed).
The receipt `ready <nonce>` is listed among the author's defaults in technical-design 12. The
no-relaunch rule matches Agent Deck ("[S: Agent Deck does the same]"), whose journal and staged
payload are "adopted in spirit" (6.8). None of this is marked as a user decision. Technical-design
6.3, 6.7 and 6.8 hold the full design; this record holds only the decision.

## Decision Drivers

- A crash at any point must be recoverable from the journal (technical-design I3).
- A relaunch can create a duplicate session.
- Technical readiness alone does not show that the destination took the context.
- The hop must never be left hidden half-done.

## Decision

A hop runs PLANNED → PREVIEWED → CONFIRMED → PREPARING → READY → SWITCHING → COMMITTED, with
ABORTED and FAILED as terminal states and ROLLED_BACK through UNDO within 10 minutes (close mode
only). Each arrow is one SQLite transaction that updates `hop.state` and inserts a `hop_event`.
Effects outside the database (spawning, writing a file, pasting into a PTY) happen between
transactions and are journaled under `hop.id`; a journal key does not make an external effect
idempotent. PLANNED takes the database uniqueness of invariant 1 and fails if another hop of the
thread is active.

READY needs both: **technical** evidence (the process is alive, the native session exists, and
for a seed its first user entry hashes to the seed sent) and a **receipt** (the destination's
first reply contains the hop's `nonce`). Timeout 90 s → FAILED. SWITCHING requires the source to
be idle (wait, or interrupt if the user chooses); turns gained after the snapshot are sent as a
catch-up and rechecked, at most twice. COMMITTED follows 30 s live or the user's next accepted
message.

Recovery at start: PLANNED, PREVIEWED and CONFIRMED → ABORTED. In PREPARING the journal records
`spawn_attempted` before the spawn and `spawn_observed` on evidence; an uncertain spawn is only
observed, never relaunched automatically, and after 90 s it is FAILED with a manual retry
offered. READY → ABORT; SWITCHING → COMMITTED if the destination is alive, else the source
activation is reopened and the hop ABORTs. No recovery path deletes anything an agent owns.
Failures follow the matrix of technical-design 6.7.

## Verified Interview Examples

- **EX-001** (rejection or error case): Input: a crash injected after each state of 6.3 -> Expected: on restart the hop ends in the documented terminal state; no source record is changed; at most the intended new destination session exists and is reconciled through the journal; no duplicate destination session or process remains (technical-design 11.1; register H24).
- **EX-002** (rejection or error case): Input: two processes try to hop the same thread -> Expected: exactly one wins; the loser gets a clear message (technical-design 11.4; register H25).
- **EX-003** (rejection or error case): Input: the destination's first reply lacks the `nonce` -> Expected: one re-ask for the receipt, then FAILED, offering to switch anyway (technical-design 6.7).
- **EX-004** (boundary case): Input: the destination stops at a first-run prompt, such as Claude's folder-trust question in an unfamiliar directory -> Expected: the hidden pane is brought forward so the user answers it; Shinatama writes no agent configuration to pre-answer it (technical-design 6.7; register H5).
- **EX-005** (rejection or error case): Input: recovery finds `spawn_attempted` without evidence -> Expected: observe only, never relaunch; FAILED after 90 s with a manual retry offered (technical-design 6.3; register H12).

## Rejected Alternatives

- **RA-001**: relaunching an uncertain spawn during recovery; "a relaunch can create a duplicate session" (technical-design 6.3).
- **RA-002**: pre-answering first-run or trust prompts by writing the agent's configuration; Agent Deck pre-seeds trust best-effort, and a stalled target stayed archived forever (its issue #2677) (technical-design 6.7).
- **RA-003**: technical readiness only, as in Agent Deck, whose semantic acceptance is a constant `"pending"` (technical-design 6.8).
- **RA-004**: exporting while the source still runs, as in Agent Deck; "the tail can race" (technical-design 6.8).
- **RA-005**: no rollback, with a stalled target hidden and archived, as in Agent Deck (technical-design 6.8).

## Enforceable Invariants

- **INV-001**: every step is journaled before its effect and idempotent after it, keyed by the hop id (technical-design I3).
- **INV-002**: at most one non-terminal hop per thread, enforced by a partial unique index on `hop(thread_id)` (technical-design I1, 4.3).
- **INV-003**: each state change is one transaction that updates `hop.state` and inserts one `hop_event`; no external effect runs inside it.
- **INV-004**: no hop reaches READY without both the technical check and the nonce reply.
- **INV-005**: recovery never relaunches automatically and never deletes anything an agent owns.
- **INV-006**: on `SQLITE_BUSY` or `SQLITE_FULL` the hop retries with backoff and never proceeds without the journal.
- **INV-007**: SWITCHING never starts while the source is working; catch-up runs at most twice.

## Consequences

- The receipt costs one short turn per hop (technical-design 12); register H2 decides whether
  the design keeps or drops it.
- The 90 s timeout, 30 s commit and 10-minute UNDO are author's calls to measure.
- The engine must compile and test the state machine with no adapter and with a fake in-memory
  adapter (technical-design 5.5.3).
- `--json` exit code 5 means "failed and recovery is needed" (`shinatama hop --recover`,
  technical-design 5.5.2).
- The post-v1 `native_import` branches of 6.3 stay unreachable in v1 (ADR-0016, ADR-0022).

## Confirmation

- Acceptance 11.1 and 11.4 pass (register H24, H25).
- Register H2, H5, H9 and H12 pass.
- Nothing has been run yet.
