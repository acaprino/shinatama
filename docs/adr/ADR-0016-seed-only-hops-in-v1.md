---
adr_id: ADR-0016
version: 1
title: v1 hops are seed-only and native stores are read-only; native import is post-v1, gated per agent and version
status: Superseded
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
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: ADR-0016 v2
changes:
---

# ADR-0016: v1 hops are seed-only and native stores are read-only; native import is post-v1, gated per agent and version

<!-- adr-header:start -->
Version 1 · Superseded by ADR-0016 v2 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. A hop can hand history to the destination as text (a seed) or by
writing a real native session into the destination agent's store. Native import writes into
another agent's session store. The question this record answers is which of these v1 ships.

Origin: "v1 is seed-only (decided 2026-10-31, user)" (functional-design 5.2; README). The stage
model of technical-design 5.4 "preserves the research history, not three implemented release
stages". Technical-design 12 also lists "Native store writing (stage 3) is not in v1" as an
author's default; the user's decision of 2026-10-31 covers it. Which agents a post-v1 stage-3
write may cover was decided by the user on 2026-11-01 (technical-design 5.4). Technical-design
5.4 and 6.2 hold the full design; this record holds only the decision.

## Decision Drivers

- Native import writes into another agent's session store (functional-design 5.2).
- Every pair of agents should get a working hop with a preview in v1.
- Pair support proven in a scratch home is not approval to write a real store (technical-design
  6.2).

## Decision

V1 permits only the strategies `native_resume` (the destination already has a segment of this
thread) and `seed_prompt` (every other supported handoff), plus `refuse` when the destination is
not installed or signed in and cannot be fixed, or is launch-only (technical-design 6.2). V1
ships the text hand-off for every pair of agents, with the preview, and never writes a native
session through Shinatama or Mukade. A fork also uses a seed in v1, never a native fork or
import. Native stores are read-only in v1, except through the agent's own commands
(technical-design I7). In v1 the hop state machine can be entered only with `native_resume` or
`seed_prompt`; its `native_import` branches are unreachable.

`native_import` and native fork are post-v1. They are added one destination agent and version at
a time, only when that pair's fixtures, privacy, store and recovery gates pass. The end state is
a hybrid: import a real native session where the destination's format can be written reliably,
otherwise a readable text export. Post-v1 writes follow the stages of technical-design 5.4
(stage 2: the agent's own import; stage 3: a native file or rows written for an agent with no
import of its own or whose own import loses what Mukade's writer keeps, as Codex's turns tool
calls into text markers (decided by the user 2026-11-01), only into a new file or new rows,
after a backup of any index the agent keeps, and never while that agent runs against the same
store) and the writer choices of technical-design 5.4.1. The
post-v1 handover rules are in ADR-0022.

## Verified Interview Examples

- **EX-001** (boundary case): Input: a Claude Code to Codex hop in v1 -> Expected: `seed_prompt`. CX4 proved Codex's own detect, import and resume of one named Claude session in a scratch home, with tool calls as text markers; "With v1 seed-only (6.2) it is a post-v1 strategy" and "Pair support is not by itself approval to use a real store" (technical-design 6.2, 10.13; CX4 record).
- **EX-002** (rejection or error case): Input: the destination's version is unknown to its adapter -> Expected: the agent is `Degraded` and the pair drops to `seed_prompt` or `refuse` (technical-design 5.5.3, 6.7; ADR-0011).
- **EX-003** (rejection or error case): Input: the destination is not installed or signed in and cannot be fixed, or is launch-only (Gemini) -> Expected: `refuse`, with an explanation and, where possible, the login flow (technical-design 6.2).
- **EX-004** (nominal case): Input: a live seed hop and a historical handoff -> Expected: "Correct v1 seed-only behaviour" for both, with distinct source models (register SS4).

## Rejected Alternatives

- **RA-001**: writing OpenCode's SQLite rows directly; its import is event-sourced, so this "is wrong and never done" (technical-design 5.4).
- **RA-002**: `claude import` as a session import; its own description is "Import config", configuration and not conversations, so it is treated as not a session import until a fixture shows otherwise (technical-design 5.4).

## Enforceable Invariants

- **INV-001**: native stores are read-only in v1, except through the agent's own commands (technical-design I7).
- **INV-002**: in v1 every hop's strategy is `native_resume`, `seed_prompt` or `refuse`; no v1 code path selects `native_import` or a native fork.
- **INV-003**: the source session's records are never modified (technical-design I2).
- **INV-004**: a post-v1 native strategy is enabled for one destination agent and version only after its fixtures, privacy, store and recovery gates pass; a passing pair probe alone does not enable it.
- **INV-005**: a stage-3 write creates only a new file or new rows, after backing up any agent index, and never while that agent runs against the same store.

## Consequences

- CX3 left the first-milestone set on 2026-10-31 "because native import is post-v1" (register 0).
  CX4 (passed) becomes a post-v1 strategy; Mukade's Codex writer writes the rollout instead
  (technical-design 10.13).
- Tool calls travel as one-line digests in the seed; they are not preserved as tool items in v1
  (functional-design 5.2: "summarized unless native import preserves them").
- Any pair with Claude Code as destination moves above `seed_prompt` only after
  technical-design 10.14 shows the CLI resumes a hand-built transcript (post-v1).
- The sessions surface offers no native Import or native handoff in v1 (functional-design 5.16).
- In v1 Mukade is used only through its read-side APIs (ADR-0020).
- Native import is deferred, not rejected: it stays the end state, behind per-pair gates.
- Mukade's Codex writer may write the rollout although Codex has an import: stage 3 also covers
  an agent whose own import loses what the writer keeps (decided by the user 2026-11-01;
  technical-design 5.4, 5.4.1; CX4). It still waits for the post-v1 gates of that pair.

## Confirmation

- Register SS4 passes for the v1 part ("Correct v1 seed-only behaviour").
- Acceptance 11.2 (register H1) passes for every ordered pair at stage 1.
- Each later pair passes its own gates (register SS4 post-v1 part, SS5, SS6; mukade-handover 7)
  before it leaves stage 1.
- Nothing has been run yet.
