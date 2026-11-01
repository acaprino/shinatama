---
adr_id: ADR-0017
version: 1
title: Every hop is close (default) or fork; a fork creates a child thread and never resumes the live source
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - hop-mode
    - fork-hop
    - thread-lineage
    - source-session
governs_symbols:
depends_on:
  - ADR-0015
  - ADR-0016
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

# ADR-0017: Every hop is close (default) or fork; a fork creates a child thread and never resumes the live source

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. After a hop the user may want the source agent gone, or may want to
keep working with it while the destination takes the work in another direction. The question
this record answers is what happens to the source at each hop.

Origin: "Close or fork the source (decided 2026-10-27, user)" (functional-design 5.2;
technical-design 6.2). The 10-minute UNDO that applies only to close hops is an author's call
("[C]", technical-design 6.3). Technical-design 6.2, 6.3 and 6.6 hold the full design; this
record holds only the decision.

## Decision Drivers

- The user can keep the source live when both directions are worth pursuing.
- Two processes must never write one native session: one Claude session resumed in two
  terminals interleaves their turns (technical-design 5.7).
- Invariant 1 (one live segment per thread) must keep holding when both stay live.

## Decision

Every hop has a `mode`, `close` or `fork`. The setting `hop.source_mode` holds the user's default
(`close`); the hop dialog offers the other mode for that hop only.

In `close` mode the destination continues the same thread and the source instance is closed at
COMMITTED, after asking first if it has a draft or a running command.

In `fork` mode the destination is a **new thread** with `parent_thread_id` = the source thread,
so invariant 1 holds on both. The source instance, thread and activation stay live and
untouched. The destination never resumes the source's live native session: in v1 it gets a new
native session through `seed_prompt`; post-v1 it may use the adapter's `fork(native_ref)` where
the agent has one, or gated `native_import`, and either must create a distinct session without
modifying the source records. After a fork the two threads diverge on purpose; nothing
synchronizes them, and the UNDO does not apply. Hopping to the source later is an ordinary hop
between two threads. In both modes the source's native session stays on disk. See
technical-design 6.2, 6.3 and 6.6.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a canary fork hop for a same-agent pair and a cross-agent pair -> Expected: the destination is a new thread with a new native session id; the source instance, thread, activation and native session stay live and unmodified; no `native_resume` of a live session; both threads are usable, the source session file is unchanged, close stays the default and the per-hop override works (register H26).
- **EX-002** (boundary case): Input: within 10 minutes of a close hop the user presses `^H` back -> Expected: a normal `native_resume` hop to the earlier segment; after a fork hop there is no UNDO, because the source is still live (technical-design 6.3, 6.6).
- **EX-003** (boundary case): Input: UNDO resumes the source natively while the destination still exists -> Expected: no duplicate live session; behaviour documented (register H13).
- **EX-004** (boundary case): Input: a close hop reaches COMMITTED while the source has a draft or a running command -> Expected: Shinatama asks before closing the source instance (technical-design 6.3).

## Rejected Alternatives

- **RA-001**: a fork whose destination resumes the source's live native session; "two processes would write one session (5.7: two terminals interleave turns)" (technical-design 6.2).

## Enforceable Invariants

- **INV-001**: every hop record has `mode` `close` or `fork`; when the user does not override it, the mode is the value of `hop.source_mode`, default `close`; a per-hop override does not change the setting.
- **INV-002**: a fork hop's destination is a new thread whose `parent_thread_id` is the source thread (technical-design I1).
- **INV-003**: a fork's destination native session id differs from the source's; no fork path resumes a live source session.
- **INV-004**: neither mode edits, deletes or moves the source session's records (technical-design I2); the source native session stays on disk.
- **INV-005**: in fork mode no source instance or activation is closed by the hop.
- **INV-006**: UNDO is offered only after a close hop.

## Consequences

- Schema: `hop.mode` and `thread.parent_thread_id` (technical-design 4.2); the sessions surface's
  Lineage tab shows the Shinatama thread chain (functional-design 5.16).
- Mukade's post-v1 sequence follows the same rule: "fork mode creates a new Shinatama thread and
  leaves the source live. Neither mode edits or deletes the source session" (mukade-handover 2).
- Daodan task handover follows the mode: a fork carries no task (daodan-native-support 4.3;
  ADR-0034).
- `fork(native_ref)` is an optional adapter operation (technical-design 5.5 D) and is post-v1.

## Confirmation

- Register H26 passes (canary fork, source file unchanged, default and override).
- Register H13 passes for UNDO after a close hop.
- Acceptance 11.1 shows no source record changed after a crash in either mode.
- Nothing has been run yet.
