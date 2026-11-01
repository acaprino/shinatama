---
adr_id: ADR-0034
version: 1
title: Task takeover only after confirmed close; a fork carries no task
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/daodan-native-support.md
    - docs/functional-design.md
    - docs/technical-design.md
    - docs/verification-register.md
  modules:
  domains:
    - daodan
    - hop
    - task-holder
    - hop-seed
governs_symbols:
depends_on:
  - ADR-0017
  - ADR-0018
  - ADR-0033
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0034: Task takeover only after confirmed close; a fork carries no task

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

A hop starts a new native session in the destination. Daodan's holder rule then refuses the
destination agent, because the old session still holds the task, and the user has to explain
and take over by hand. Daodan's take-over needs the user to confirm that the old session is
closed. Sources, kept apart:

- Decided by the user, 2026-10-27: Shinatama carries the task across a hop (functional-design
  goal 5 and 5.5, as part of native support).
- Author's call, not confirmed: a fork hands over no task (daodan-native-support 7, call 3).
- Proposed shape: the close-mode sequence of daodan-native-support 4.3, "proposed, not approved".

The full design is in daodan-native-support 4.3; this record holds only the decision. It
relies on the close and fork modes (ADR-0017), the hop state machine (ADR-0018) and the
read-only link (ADR-0033).

## Decision Drivers

- Two live processes must never both believe they hold one task.
- The takeover must never happen before the source is really closed.
- One user confirmation, not two: the hop dialog's confirmation is the one Daodan needs.
- Shinatama writes no Daodan state; the destination takes over through Daodan's own flow.

## Decision

When the source thread is linked to an open task, the hop dialog gains a Daodan task block
(`run_id`, objective, phase, branch, worktree, unanswered questions, current holder), shown
verbatim in the preview. The seed gains an optional Daodan block with its own budget line.

- **Close (default).** The preliminary seed carries the task identity and says that close was
  requested but is **not yet confirmed**; the source keeps the holder and the destination may
  read the task, not take it over. Only after the hop reaches COMMITTED and the source process
  has actually closed does Shinatama send a separate handover message through the
  destination's normal channel, stating that the source session is closed and the user
  confirmed the takeover. The destination then continues through Daodan's own continue flow.
  If closure is refused, crashes or remains uncertain, the message is not sent and the holder
  is unchanged.
- **Fork.** The source stays live and keeps the holder. The destination is a new thread without
  the task; its seed says the task is held by the source and may be read, not written.
- **No Daodan package** (Hermes Agent, DeepSeek Harness). The task block arrives as read-only
  context, the dialog says the task cannot be taken over there, and close mode sends no
  handover message.

See daodan-native-support 4.3.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a task thread hopped between two agents in close mode -> Expected: no early takeover, one confirmation in total, the holder changes only after confirmed closure (register D13).
- **EX-002** (rejection or error case): Input: the same hop with closure refused, a crash before COMMITTED, or an already active destination -> Expected: the holder is unchanged (register D13).
- **EX-003** (boundary case): Input: the same hop in fork mode -> Expected: the destination reads the task without writing and the holder is unchanged (register D13).
- **EX-004** (boundary case): Input: a close-mode hop of a task thread into Hermes Agent or DeepSeek Harness, which have no Daodan package -> Expected: the task block arrives as read-only context, no handover message is sent, the holder is unchanged (daodan-native-support 4.3).

## Rejected Alternatives

- **RA-001**: sharing the holder between the two threads of a fork; handing over no task is safer. To revisit if forks of task threads turn out to be the common case (daodan-native-support 7, call 3).
- **RA-002**: Shinatama running `--take-over` itself, or telling an agent to, without the user's confirmation in the dialog; listed as an anti-goal (daodan-native-support 2).

## Enforceable Invariants

- **INV-001**: the preliminary seed never claims that the source is closed.
- **INV-002**: the handover message is sent only after COMMITTED and a confirmed closure of the source process.
- **INV-003**: two threads never both hold one task; a fork destination never receives the task.
- **INV-004**: no hop path writes Daodan state; the holder changes only through Daodan's own flow in the destination agent.
- **INV-005**: the hop needs exactly one user confirmation, the one in the hop dialog.
- **INV-006**: the Daodan block the destination receives is exactly what the preview showed (technical-design I4).

## Consequences

- The hop dialog and preview (functional-design 5.2 item 5) and the seed document (technical-design 6.4) gain the Daodan block and its budget line; technical-design does not contain them yet.
- The continue flow depends on the per-host command form (register D10); the link may be by place only (register D9), which the design accepts.
- After a failed close the holder is unchanged. A local holder whose session matches no live thread is shown as "held by a closed session" and is never taken over automatically (daodan-native-support 4.4; ADR-0033).

## Confirmation

- Register row D13 (G0) must pass, with closure refusal, a crash before COMMITTED and an already active destination injected. It has not been run.
- D9 and D10 (G0) must be recorded first, since the link and the typed continue command depend on them.
