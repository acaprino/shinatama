---
adr_id: ADR-0026
version: 1
title: Moderated roundtable of fresh headless participants, answer-only by default, capped, reusing Daodan for bare models
status: Accepted
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
  - ADR-0011
  - ADR-0024
  - ADR-0029
  - ADR-0032
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

# ADR-0026: Moderated roundtable of fresh headless participants, answer-only by default, capped, reusing Daodan for bare models

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user asked to "let different harness instances and models talk
to each other and compare on a topic" (functional-design goal 10 and 5.10, decided as a wish).
The shape, moderated rounds and answer-only turns by default with a per-participant opt-in to
tools, is the user's decision, confirmed on 2026-11-01 (functional-design goal 10, the 5.10
heading and section 7). Scheduling, and whether the free-form two-agent variant is wanted, stay
open. The runner of technical-design 7.8 is untagged author's design, and its default caps are
the author's call (register R5). The question is how
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
(`caps_json`; by default $2 and 40k tokens per roundtable, to be calibrated) and stops at the
first cap; the user can interject between rounds (a `note` to
all) or stop. Turns run answer-only, with no tools, file writes or network, through each
adapter's `ask_headless` policy; the user may allow tools per participant, and the roundtable
header shows it. Each participant's traffic goes through the sidecar. The run is stored as a
thread of kind `roundtable`, searchable and hoppable ("continue with codex's plan"). A free-form
two-agent chat would be the same machinery with one reply per round; whether it is wanted is
open. See functional-design 5.10 and technical-design 7.8.

## Verified Interview Examples

- **EX-001** (boundary case): Input: a run that would exceed the token cap -> Expected: it stops at the cap with a partial synthesis (technical-design 11.8).
- **EX-002** (nominal case): Input: "continue with codex's plan" on a finished roundtable -> Expected: a work thread seeded with that participant's final answer; the roundtable thread is also found by search (technical-design 7.8; register R6).
- **EX-003** (rejection or error case): Input: canary prompts in each agent's headless answer-only mode -> Expected: no file written and no command run (register R2).
- **EX-004** (rejection or error case): Input: one participant's answer tells another to write a file -> Expected: not obeyed in answer-only (register R4).

## Rejected Alternatives

- **RA-001**: reimplementing Ultra Nerd; Daodan already has this protocol for bare models, so Shinatama reuses it.
- **RA-002**: letting agents start a roundtable through an MCP tool (`shinatama_roundtable`); only the user starts one, because it spends money (technical-design 7.5).
- **RA-003**: an addressed mailbox between agents that outlives the sessions or the expiry; it is part of Work Controller's features on Daodan's roadmap. The boundary is persistence (user, 2026-11-01): the messages of ADR-0024 expire within an hour, and the roundtable "is started by the user and session-level" (functional-design 2, non-goals).

## Enforceable Invariants

- **INV-001**: no roundtable starts without a user action and a cost warning; no agent tool starts one.
- **INV-002**: participants run in headless sessions of their own; no round writes into one of the user's live instances, and there is no fallback that types into a PTY and reads the screen (functional-design 5.10; technical-design 7.8).
- **INV-003**: a participant receives other participants' answers only as quoted data from a named participant (technical-design I6).
- **INV-004**: by default a turn has no tools, file writes or network; a per-participant allowance is set by the user and shown in the header (`roundtable_participant.tools_allowed`).
- **INV-005**: the runner stops at the first of `max_rounds`, the token cap and the cost cap.
- **INV-006**: every participant's traffic, bare-model calls included, goes through the sidecar.
- **INV-007**: unless the user sets other caps, a roundtable runs with a cost cap of $2 and a token cap of 40k tokens, defaults to be calibrated from the real cost of three topics (author's call; technical-design 7.8; register R5).

## Consequences

- Answer-only switches were found only for Pi, from its help output; for the other agents the exact flags were not established (technical-design 5.7 item 5); register R2 (G0) rests on the per-agent rows CL11, CX9, OC7, PI8, HM7 and DS7.
- How Shinatama reaches `moe_ask` without holding credentials is open (register R1, G0).
- The default caps of $2 and 40k tokens are the author's call, calibrated on three topics (register R5); parallel participants on one account need rate-limit and overshoot data (R3); a model per participant must be chosen per launch without editing global configuration, or that agent is excluded as a participant (R7).
- The database holds `roundtable` and `roundtable_participant` (technical-design 4.2); the roundtable view is a consumer of the event feed (technical-design 8.3; ADR-0027).
- Order: after the hop (functional-design 5.12, step 7); it follows its own gates and does not block the first usable slice (ADR-0040).
- Open: scheduling, and whether the free-form variant is wanted (functional-design goal 10, 5.10).

## Confirmation

- Register R1 to R7 and M6 pass; all are open.
- Acceptance test 11.8 passes.
- Nothing has been run yet.
