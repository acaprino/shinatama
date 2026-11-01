---
adr_id: ADR-0025
version: 1
title: Moderated roundtable of fresh headless participants, answer-only by default, capped, reusing Daodan for bare models
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - roundtable
    - headless-sessions
    - answer-only
    - cost-caps
    - daodan-moe
governs_symbols:
depends_on:
  - ADR-0010
  - ADR-0023
  - ADR-0028
  - ADR-0031
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

# ADR-0025: Moderated roundtable of fresh headless participants, answer-only by default, capped, reusing Daodan for bare models

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user asked to "let different harness instances and models talk
to each other and compare on a topic" (functional-design goal 10 and 5.10, decided as a wish).
The shape is the author's: functional-design 5.10 lists it under "My calls", and section 7
records moderated rounds, a free-form two-agent variant and answer-only by default as calls to
check. Goal 10 still marks shape and scheduling open, and whether the free-form variant is wanted
is open. The runner of technical-design 7.8 is untagged author's design. The question is how
several agents and bare models discuss one topic without steering each other, spending without
limit or touching the user's live sessions. The full design is in functional-design 5.10 and
technical-design 7.6 to 7.8; this record holds only the decision.

## Decision Drivers

- Agents must not steer each other: a peer's answer is data, not an instruction.
- A multi-model discussion multiplies token use and cost.
- The user's live working sessions stay untouched.
- Daodan already has a protocol for bare models (Ultra Nerd); Shinatama should not build a second one.
- A turn needs a clear end and a clean answer.

## Decision

Only the user starts a roundtable, after a cost warning. Participants are any mix of the
supported agents, each with its own model, including an agent pointed at a local model and the
same agent twice. The runner, in the process the user is in, gives each participant a headless
session of its own, never one of the user's live instances: `ask_headless(new session, prompt,
policy)` per participant per round, in parallel. Bare-model participants go through Daodan's
`moe` plugin ("Ultra Nerd": `moe_ask`, run state in `.moe/<run-id>/`) and `peer-review`, which
Shinatama's Daodan install already enables; the roundtable adds full agent participants and the
UI around them.

Rounds are moderated. Round 1 is independent; in rounds 2 to N each participant gets the others'
previous answers as quoted data from a named participant and critiques them. A synthesis step,
an `ask_headless` by a participant the user picks or a configured one, writes agreements,
differences and open questions. The runner enforces `max_rounds`, a token cap and a cost cap
(`caps_json`) and stops at the first cap; the user can interject between rounds (a `note` to
all) or stop. Turns run answer-only, with no tools, file writes or network, through each
adapter's `ask_headless` policy; the user may allow tools per participant, and the roundtable
header shows it. Each participant's traffic goes through the sidecar. The run is stored as a
thread of kind `roundtable`, searchable and hoppable ("continue with codex's plan"). A free-form
two-agent chat is the same machinery with one reply per round. See functional-design 5.10 and
technical-design 7.8.

## Verified Interview Examples

- **EX-001** (boundary case): Input: a run that would exceed the token cap -> Expected: it stops at the cap with a partial synthesis (technical-design 11.8).
- **EX-002** (nominal case): Input: "continue with codex's plan" on a finished roundtable -> Expected: a work thread seeded with that participant's final answer; the roundtable thread is also found by search (technical-design 7.8; register R6).
- **EX-003** (rejection or error case): Input: canary prompts in each agent's headless answer-only mode -> Expected: no file written and no command run (register R2).
- **EX-004** (rejection or error case): Input: one participant's answer tells another to write a file -> Expected: not obeyed in answer-only (register R4).

## Rejected Alternatives

- **RA-001**: reimplementing Ultra Nerd; Daodan already has this protocol for bare models, so Shinatama reuses it.
- **RA-002**: letting agents start a roundtable through an MCP tool (`shinatama_roundtable`); only the user starts one, because it spends money (technical-design 7.5).
- **RA-003**: a persistent addressed mailbox between agents; it is part of Work Controller's features on Daodan's roadmap, and the roundtable is "started by the user, session-level, no mailbox" (functional-design 2, non-goals).

## Enforceable Invariants

- **INV-001**: no roundtable starts without a user action and a cost warning; no agent tool starts one.
- **INV-002**: participants run in headless sessions of their own; no round writes into one of the user's live instances.
- **INV-003**: a participant receives other participants' answers only as quoted data from a named participant (technical-design I6).
- **INV-004**: by default a turn has no tools, file writes or network; a per-participant allowance is set by the user and shown in the header (`roundtable_participant.tools_allowed`).
- **INV-005**: the runner stops at the first of `max_rounds`, the token cap and the cost cap.
- **INV-006**: every participant's traffic, bare-model calls included, goes through the sidecar.

## Consequences

- Answer-only switches were found only for Pi, from its help output; for the other agents the exact flags were not established (technical-design 5.7 item 5); register R2 (G0) rests on the per-agent rows CL11, CX9, OC7, PI8, HM7 and DS7.
- How Shinatama reaches `moe_ask` without holding credentials is open (register R1, G0).
- The default caps of $2 and 40k tokens are calibrated on three topics (register R5); parallel participants on one account need rate-limit and overshoot data (R3); a model per participant must be chosen per launch without editing global configuration, or that agent is excluded as a participant (R7).
- Functional-design 5.10 keeps PTY typing and screen reading as a transport fallback; technical-design 7.8 specifies only `ask_headless`. The two must be aligned once the per-agent headless routes are proved.
- The database holds `roundtable` and `roundtable_participant` (technical-design 4.2); the roundtable view is a consumer of the event feed (technical-design 8.3; ADR-0026).
- Order: after the hop (functional-design 5.12, step 7); it follows its own gates and does not block the first usable slice (ADR-0039).
- Open: whether the free-form variant is wanted; goal 10 marks shape and scheduling open.

## Confirmation

- Register R1 to R7 and M6 pass; all are open.
- Acceptance test 11.8 passes.
- Nothing has been run yet.
