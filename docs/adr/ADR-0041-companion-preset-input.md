---
adr_id: ADR-0041
version: 1
title: The companion pane accepts only preset actions and selections, and has two card flows
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/specs/companion-explainer.md
    - docs/functional-design.md
  modules:
  domains:
    - companion
    - preset-input
    - card-flows
governs_symbols:
depends_on:
  - ADR-0002
created_from_task: init-abfe6369faa4489a915c5f057a84e467
proposed_on: 2026-11-01
proposed_at_sha: 6539cf1d5bf6564247a6d8b5d47ccc114f9fdc58
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0041: The companion pane accepts only preset actions and selections, and has two card flows

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

ADR-0002 defines the companion pane. Its first revision let the user "type a question at any
time" and had one explaining flow with an opt-in learning mode. On 2026-11-01 the user pasted a
design with two flows, Learning (the tutor talks to the user) and Aided Decision (the tutor
prepares the user to answer the worker), and with deeper explanation only through fixed buttons,
no free text, "so that the tutor session stays tidy and the main one is not touched". The question
this record answers is what the user can do in the pane and how the two flows are told apart. The
design is in `docs/specs/companion-explainer.md`, sections 5.1, 5.5 and 5.6. ADR-0002 v1 EX-006
assumed a typed question; ADR-0002 v2 rewrites that example to a pane action.

## Decision Drivers

- The companion session must stay tidy and bounded, and cost must be predictable.
- Every action must be testable as a fixed template.
- The main chat and the worker's context must not be touched (ADR-0003).
- The depth of an explanation must be reachable without typing.

## Decision

The companion pane has no free-text input. The user acts through fixed presets on a card (basics,
why here, alternatives, example, simpler, deeper, expand, sources) and through Explain this on a selection
in the worker's pane. Each preset reaches the companion as a fixed template, versioned with
Shinatama, with its operand wrapped as untrusted data. The one piece of user-typed text that
reaches the companion is the short level text written in the settings screen, sent as data. The
pane has two kinds of card: a Learning card, which explains to the user, and an Aided Decision
card (ADR-0003), which prepares the user to answer the worker. Which kind a turn gets is decided
by its origin (a pending request, the worker's final message, the user's expand preset), not by a
mode switch. A typed question box is an open point of the specification, not part of the first
release.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the user presses basics on a card that mentions Coolify -> Expected: a Learning card that explains the concept from level 1, produced from the fixed template of that preset with the concept wrapped as data and within the preset caps (spec row 13).
- **EX-002** (rejection or error case): Input: an inspection of the companion pane -> Expected: it has no editable text control, and no key typed in it reaches the companion (spec EX-019).
- **EX-003** (boundary case): Input: the user presses a preset while a card turn is running -> Expected: it is queued and runs after the card turn, before any queued automatic card; no two headless turns of the companion overlap (spec EX-007).
- **EX-004** (boundary case): Input: the daily cap is reached while a worker waits on a permission -> Expected: the pending request keeps its request chip, whose button is the expand preset; the presets still work and nothing starts a turn on its own (spec EX-024).

## Rejected Alternatives

- **RA-001**: a free-text box in the pane; rejected for the first release because the session stays tidy and bounded only if every action is a template (spec RA-008).
- **RA-002**: one flow with a mode switch between explaining and learning; rejected because the user's design separates the two by whom the card prepares (the user, or the user's answer to the worker), which the origin of the card already decides.

## Enforceable Invariants

- **INV-001**: the pane has no free-text input; the only user-typed text that reaches the companion is the level text of the settings screen, at most 500 characters, sent as data (spec INV-011).
- **INV-002**: every preset is a fixed template and its operand is wrapped as untrusted data and escaped (spec 5.6).
- **INV-003**: a preset pressed during a card turn is queued; at most one headless turn is in flight per companion session (spec INV-007).

## Consequences

- The companion's input surface is the preset table of the specification (5.6); a new action is a new template and a new test, not a new text field.
- The brief and the per-turn context carry the flows and the templates (spec 5.4).
- Whether to allow typing later is a user decision (spec open point 6).

## Confirmation

- Row 13 of specification 10.2: the pane exposes no editable text control; each preset yields a card within its caps on every harness.
- The concurrency test of row 8.
