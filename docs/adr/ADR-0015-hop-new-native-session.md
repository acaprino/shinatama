---
adr_id: ADR-0015
version: 1
title: A hop starts a new native session seeded with the thread; seamless defined by testable experience
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - hop
    - seamless-definition
    - native-session
    - loss-declaration
governs_symbols:
depends_on:
  - ADR-0004
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

# ADR-0015: A hop starts a new native session seeded with the thread; seamless defined by testable experience

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user wants to hop a live chat from one agent to another without
starting over, and asked for it to be "truly seamless", built from scratch in Rust
(functional-design 5.2 heading, decided by the user). The user's own interoperability research
of 2026-10-26 found that no tool moves a live session between different agents. The question
this record answers is what a hop is and what "seamless" can honestly mean.

Origin: "truly seamless" is the user's decision. Points 1 to 4 of the definition in
functional-design 5.2 are marked "my call" (author's call, not confirmed); point 5, the preview,
is unmarked. The timing targets in technical-design 6.1 are "[C: targets, to measure]".
Functional-design 5.2 is the summary; technical-design 6 holds the full design and wins where
they differ. This record holds only the decision.

## Decision Drivers

- Fidelity: the destination is the real agent, not a replayed chat (ADR-0004).
- Honesty about loss: promising a lossless transfer is an anti-goal (functional-design 2).
- What exists today: a new native session seeded with the thread's content, not a moved
  session (technical-design 6.8, landscape check of 2026-10-27).
- The user should not have to re-explain, copy or wait.

## Decision

A hop is "a new native session in the destination, started with the thread's content, with the
loss declared" (functional-design 5.2). Resuming within the same agent is native and lossless;
the words "native resume" are used only for that case. Seamless is defined by experience, as
five testable criteria (technical-design 6.1): (1) one key (`^H`), a destination, Enter;
(2) under 8 s from Enter to "destination ready" with a warm standby and under 25 s without, with
visible progress and the source usable throughout; (3) the destination answers correctly with a
fact only present in the history; (4) hopping back resumes the earlier native session and tells
it what it missed; (5) the user sees, before sending, exactly what is carried and what is lost.

Not promised: moving a live process, permissions, model state, or the agent's hidden reasoning.
The hop dialog lists them as lost. What carries over, as the dialog shows it: conversation text,
tool calls (summarized unless native import preserves them), files (same folder, git state
unchanged), attachments (paths) and project instructions (CLAUDE.md and AGENTS.md linked; what
linking does is still to be decided and tested, and no file is created in the user's repository
without consent, the author's call of register H18).
Agent Deck and Agent Relay are prior art to study, not to wrap. Strategies, modes, the state
machine and the seed are ADR-0016 to ADR-0019.

## Verified Interview Examples

- **EX-001** (nominal case): Input: in the source, a made-up project codename stated once in turn 3; hop to each other agent and ask for the codename without repeating it -> Expected: the correct answer for every ordered pair among the six agents at stage 1; a pair whose source is Hermes Agent or DeepSeek Harness counts from the day Mukade reads that source (technical-design 11.2; register H1: 10 of 10 on the small thread).
- **EX-002** (boundary case): Input: Enter to READY measured per pair, 95th percentile, with and without warm standby -> Expected: the 8 s and 25 s targets of 6.1 confirmed or revised (register H6, H21).
- **EX-003** (rejection or error case): Input: an agent whose installed version fails its fixtures -> Expected: the agent is `Degraded` (launch-only plus its own export command, failing capabilities `unsupported`) and hops to or from it go by `seed_prompt` or are refused, with a visible reason instead of a wrong hop (technical-design 5.5.3, 11.10; ADR-0011).

## Rejected Alternatives

- **RA-001**: a universal migration of a live session between agents; "No tool does this; the hop starts a *new* native session in the destination" (functional-design 2, non-goals).
- **RA-002**: a fixed small excerpt of the thread; Agent Deck caps its export at 32 KB by default, and "Shinatama should do better, with the budget computed for the destination" (functional-design 5.2, point 3).
- **RA-003**: promising a lossless transfer; an anti-goal: "The UI must always say what carries over and what does not" (functional-design 2).

## Enforceable Invariants

- **INV-001**: a cross-agent hop never moves a process and never resumes another agent's session; the destination is a new native session, or, for an agent that already has a segment of the thread, its own earlier session (technical-design 6.2).
- **INV-002**: the hop dialog states what carries over and what is lost before anything is sent; permissions, model state and hidden reasoning are always listed as lost.
- **INV-003**: the source session's records are never modified (technical-design I2).
- **INV-004**: "native resume" in the UI and documents means a resume within the same agent only.
- **INV-005**: carrying project instructions across a hop never creates a file in the user's repository without the user's consent (author's call; register H18; functional-design 5.2).

## Consequences

- The timing targets are author's calls to measure (H6, H21). Point 2 needs proof that a
  prepared agent costs nothing until its first turn (H6: "zero token cost while idle").
- Point 4's catch-up is "a guess to validate" (functional-design 5.2); register H10 tests it.
- The hop context passes through the sidecar and counts against the destination's window
  (functional-design 5.6; technical-design 9).
- Every adapter must support the canary hop against every other adapter in both directions as
  part of its conformance suite (technical-design 5.5.3).
- Overall approval of the designs is open (register O5).

## Confirmation

- Acceptance test 11.2 (register H1) passes for every ordered pair at stage 1.
- Register H6 and H21 record the timings; acceptance 11.3 (register H10) passes for hop back.
- Acceptance 11.10 passes for the degraded path.
- Nothing has been run yet.
