---
adr_id: ADR-0016
version: 2
title: Handoff always carries a seed; native import and native fork are out of scope in every version; native stores stay read-only
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - docs/mukade-handover.md
  modules:
  domains:
    - hop-strategy
    - v1-scope
    - native-import
    - native-store-write
governs_symbols:
depends_on:
  - ADR-0015
created_from_task: user-decision-2026-11-03
proposed_on: 2026-11-03
proposed_at_sha: ca72385451d6196c0e0c0968640f1e9d94e3e0ff
accepted_on: 2026-11-04
accepted_at_sha: 4818caa5596cb07e8ecf5b14a6a2d791aaf64e5d
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - Decision (post-v1 route removed)
  - INV-001
  - INV-002
  - INV-004
  - INV-005
---

# ADR-0016: Handoff always carries a seed; native import and native fork are out of scope in every version; native stores stay read-only

<!-- adr-header:start -->
Version 2 · Accepted 2026-11-04 at 4818caa · proposed 2026-11-03 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Version 1 of this record made hops seed-only **in v1** and kept native import as the gated
post-v1 end state: a hybrid that imports a real native session wherever the destination format
can be written reliably, one agent and version at a time behind fixtures, privacy, store and
recovery gates (ADR-0016 v1; technical-design 5.4, 5.4.1, 6.2; functional-design 5.2, 5.16;
ADR-0022 for the handover rules). None of those gates has ever been run. On 2026-11-03 the user
decided that Shinatama must not import natively at all: a handoff imports a **summary** (the
seed document), never a native session. This version withdraws the post-v1 route; the question
it answers is what a hop carries in every version, not just v1.

Origin: user decision of 2026-11-03 (chat: "shinatama non deve importare nativo, deve invece
importare un riassunto", confirmed for recording the same day). The seed document and its
delivery are ADR-0019; the read-only sessions surface is ADR-0023; Mukade's role is ADR-0020.

## Decision Drivers

- Native import writes into another agent's session store; every write gate (register SS4
  post-v1 part, SS5 staged transfer, SS6 raw/export, mukade-handover 5) is unrun, and no code
  path exists today.
- The seed document (ADR-0019) already carries a budgeted summary plus the complete transcript
  file with redaction; what native import adds is native resumability of imported history, at
  the price of store writes, provider acceptance risk and perpetual per-pair gates.
- Mukade's write side moves quickly (23 commits between the last two pins; a native-compaction
  mode pending uncommitted in its working tree on 2026-11-03); gating per agent and version
  against a moving target is a standing cost the seed route does not have.
- User preference (2026-11-03): hand off a summary, never write another agent's store.

## Decision

In **every** version, not only v1:

- The hop strategies are exactly `native_resume`, `seed_prompt` and `refuse` (technical-design
  6.2). The `native_import` branches and native fork are unreachable and are never implemented.
- Shinatama writes no agent's native store; native stores are read-only except through the
  agent's own commands (technical-design I7, with no post-v1 exception).
- Mukade is used only through its read-side APIs (readers, store catalog and installation scan,
  detail/find projections) behind the `session-reader` feature. Shinatama never calls Mukade's
  writers or `hop()`/`hop_into()`. The post-v1 hybrid end state of version 1 is withdrawn.
- "Import a summary" means the seed document: the budgeted seed with the full transcript file
  and redaction of ADR-0019, delivered as bootstrap plus file. The hop dialog keeps declaring
  the losses (live process, permissions, model state, hidden reasoning, tool items as digests).
- Reopening native handover requires a new Accepted decision superseding this record; the
  ADR-0022 v1 rules remain the mandatory preconditions for any such reopening (ADR-0022 v2).

## Verified Interview Examples

- **EX-001** (nominal case): Input: a live hop between any supported pair -> Expected: `seed_prompt` with the staged preview; the history travels as the seed document; no native store write occurs anywhere in the flow.
- **EX-002** (boundary case): Input: the destination already holds a segment of this thread -> Expected: `native_resume` through the agent's own commands; still no write by Shinatama or Mukade.
- **EX-003** (rejection or error case): Input: a request to "import natively" into OpenCode (or any agent) -> Expected: refused as out of scope with the seed route offered; no code path exists that calls a Mukade writer, so the request cannot be satisfied silently.
- **EX-004** (rejection or error case): Input: the destination is not installed or signed in and cannot be fixed, or is launch-only (Gemini) -> Expected: `refuse`, with an explanation and, where possible, the login flow (unchanged from v1; technical-design 6.2).

## Rejected Alternatives

- **RA-001**: version 1's gated post-v1 hybrid (native import enabled one destination agent and version at a time after its gates pass). Withdrawn by the user's decision of 2026-11-03: the gates were never run, the write side keeps moving, and the seed already delivers the history in readable form.
- **RA-002**: writing OpenCode's SQLite rows directly (v1's RA-001): its import is event-sourced, so direct row writing "is wrong and never done". Moot under this record, still rejected.
- **RA-003**: adopting Mukade's pending `--compaction native` mode (uncommitted upstream work on 2026-11-03, OpenCode destination) as an import mechanism: it writes a destination-native session and is therefore out of scope here; that capability serves hosts that write OpenCode sessions, such as JanuraCode's session bridge, not Shinatama's handoff.

## Enforceable Invariants

- **INV-001**: in every version a hop's strategy is `native_resume`, `seed_prompt` or `refuse`; no code path selects `native_import` or a native fork.
- **INV-002**: native stores are read-only for Shinatama except through the agent's own commands; Mukade's write side (`hop`, `hop_into`, `import`, writers) is never called.
- **INV-003**: the source session's records are never modified (technical-design I2).
- **INV-004**: the handoff content is the ADR-0019 seed document with its budget and redaction; the dialog never promises a lossless transfer.
- **INV-005**: reopening native handover requires a new Accepted decision superseding this record; no gate result, upstream capability or fixture run reopens it on its own.

## Consequences

- ADR-0022 v1's rules survive only as the re-entry preconditions recorded in ADR-0022 v2.
- Register: SS4's post-v1 native-write verifications and SS5's same-harness staged-transfer part
  close with this decision; SS5's store-catalog identity/origin/access part stays (read side);
  next-run item 8 (SS5 against `hop_into`) is withdrawn; SS6 is unchanged.
- Requirements write-back pending: FRD-0005 R12 (post-v1 native fork/import clause), R16, R17
  and section 2.10, and FRD-0006 R30 and the native-import/handoff clauses disagree with this
  record until amended.
- Design write-back pending: technical-design 5.4 (stages 2–3), 5.4.1 ("`hop()` and native
  writers are post-v1"), the `native_import` branches of 6.2/6.3, and functional-design
  5.2/5.16 carry dated notes pointing here until they are rewritten.
- mukade-handover.md becomes the design record of a closed route, kept for a possible
  re-entry; its status header says so.
- Mukade pin advances stay read-side-driven only; upstream writer changes (including the
  pending native compaction mode) no longer imply Shinatama work by themselves.
- CX4 remains an evidence row about Codex's own detect/import/resume; it enables nothing here.
- Declared loss: imported history is never natively resumable in the destination and tool calls
  travel as digests. Accepted by the user's decision of 2026-11-03.

## Confirmation

- Register SS4's v1 part ("Correct v1 seed-only behaviour") now covers every version.
- Acceptance 11.2 (register H1) passes for every ordered pair at stage 1; no stage-2/3
  acceptance path remains.
- Hops are not implemented yet; nothing has been run.
