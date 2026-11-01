---
adr_id: ADR-0003
version: 1
title: "Decision cards explain a worker's pending request before the user answers"
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/specs/companion-explainer.md
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - companion
    - decision-cards
    - approval-explanation
governs_symbols:
depends_on:
  - ADR-0002
created_from_task: init-6c1ecf1751fc4d45a9a63202041ab5e6
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: 2026-11-01
accepted_at_sha: 5ff9deaec52dd2c4036bc14f1a91c6c692e39ea0
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0003: Decision cards explain a worker's pending request before the user answers

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at 5ff9dea · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

ADR-0002 defines the companion pane. Its cards arrive when the worker's turn ends. The user
reported (2026-11-01) that the model takes decisions the user does not understand and proposes
tools outside the user's competence (example: Coolify), and that the user obeyed "come un automa
senza capire". The moment of obedience is mid-turn, at an approval or a question, so a card at
turn end arrives after the answer. The question this record answers is when and how the
companion explains a pending request, and what role it has in it. The design is in
`docs/specs/companion-explainer.md`, section 5.5; this record holds only the decision.

## Decision Drivers

- Understanding must come before the answer, not after.
- Explanations can raise acceptance of an AI recommendation whether or not it is right, so the
  explanation must be neutral.
- The companion stays read-only with no channel to the worker (ADR-0002 INV-002).
- It should work on every hosted harness where it can.

## Decision

When a harness natively reports a pending permission or question, Shinatama sends it at once as
a pending-request item and the companion answers with a **decision card**: what the thing is,
what happens on yes and on no, why the worker asks, what could make it a bad idea, what to check
first, a risk marker from a fixed set, and a source label on each claim. The card is neutral: it
never says to approve or deny and never argues for the worker's choice. It is advice only: the
companion cannot answer, hold, delay or alter the request. The default setting explains every
request that is not a read inside the worktree and every question offering options. Separately,
the user can select text in the worker's pane and ask for an explanation, which works on every
harness. A short text of the user's own level calibrates the depth. See specification 5.5.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a Claude Code worker asks permission to run an installer for a self-hosted deployment platform and the user does not know it -> Expected: before the user answers, a decision card with its seven parts appears, the claims are labelled, the risk markers include changes outside the worktree and opening a service, it contains no approve or deny, and the worker's prompt is unchanged (spec EX-010).
- **EX-002** (boundary case): Input: setting "smart", the worker asks to read a file inside the worktree -> Expected: no card and no cost (spec EX-011).
- **EX-003** (boundary case): Input: the worker's harness has no native waiting event -> Expected: "approvals not detected", no decision card, and select-to-ask still works (spec EX-012).
- **EX-004** (rejection or error case): Input: the companion is paused, capped or crashed when the worker asks permission -> Expected: the worker's prompt appears and is answerable exactly as without a companion; at the cap a one-line stub offers to explain anyway (spec INV-009 and cap rule).

## Rejected Alternatives

- **RA-001**: making the companion hold the worker's approval until the card is read, or pre-answering it; it puts the companion in the approval path (spec RA-006).
- **RA-002**: explaining only at turn end; the explanation arrives after the user has answered.
- **RA-003**: a card that recommends approve or deny; explanations raised acceptance regardless of correctness in the evidence of spec 10.1, so a recommendation steers rather than informs.

## Enforceable Invariants

- **INV-001**: no state of the companion (a card running, queued, capped, paused or crashed) delays or changes a worker's approval prompt.
- **INV-002**: a decision card never says to approve or deny; its template has no verdict field.
- **INV-003**: every claim about a tool or a concept in a decision card is labelled "from the repository" with its path and line, or "general knowledge, not checked"; measured by a rubric, not enforceable at runtime.
- **INV-004**: a decision card is produced only from a native waiting event, an explicit user selection or an explicit user request; a pending request is never inferred from screen text.

## Consequences

- technical-design 8.1 and 8.3: the waiting event carries the pending request where the harness reports it, redacted before the feed; functional-design 5.16 gains an "Explain this" action on a text selection.
- Cost: one card per request that is not a read inside the worktree; it counts against the companion's daily cap, and at the cap a stub remains.
- Without network, an explanation of an external tool is general knowledge and says so; a user-approved documentation lookup is an open point of the specification.
- New register rows 10 to 12 of the specification (pending-request feed and latency, decision-card rubric, select to ask).

## Confirmation

- Rows 10, 11 and 12 of specification 10.2 pass per harness before decision cards are offered on it.
- The card template is checked to have no verdict field.
- The latency test shows the worker's prompt with the same timing with the companion running, queued, capped, paused and killed.
