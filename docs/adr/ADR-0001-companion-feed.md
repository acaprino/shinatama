---
adr_id: ADR-0001
version: 1
title: Companion pane fed by the canonical transcript, harness chosen per companion
status: Abandoned
author: Alfio Caprino
scope:
  paths:
    - docs/specs/companion-explainer.md
  modules:
  domains:
    - companion
    - explain-mode
    - read-only-session
governs_symbols:
depends_on:
created_from_task: init-fe486e2c842b4c92a29a97ee409ce689
proposed_on: 2026-11-01
proposed_at_sha: 9eabbaa290bf11cf242e660da54d08504987929b
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: 2026-11-01
retired_reason: "Draft corrected before approval after the claim check of 2026-11-01; replaced by ADR-0002, which keeps the specification's numbering."
supersedes:
superseded_by: null
changes:
---

# ADR-0001: Companion pane fed by the canonical transcript, harness chosen per companion

<!-- adr-header:start -->
Version 1 · Abandoned 2026-11-01: Draft corrected before approval after the claim check of 2026-11-01; replaced by ADR-0002, which keeps the specification's numbering. · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet (`Cargo.toml` workspace with no members). The user asked for a
learning or explaining mode: a read-only session beside a working agent instance that explains
what is happening in the project (2026-10-31). The question this record answers is where that
session gets its knowledge of the worker and which harness runs it. The full design is in
`docs/specs/companion-explainer.md`; this record holds only the decision.

## Decision Drivers

- The worker's context, tokens and behaviour must not change.
- The same behaviour on every hosted harness (Shinatama's uniformity role).
- Read-only: the companion must not write the worktree or steer the worker.
- Answers rendered as Markdown with tables and diagrams.

## Decision

The companion is a separate session that Shinatama feeds with per-turn deltas of the worker's
canonical transcript (engine 5.1), the event feed and the git delta, wrapped as untrusted data.
It never forks or reads the worker's native session itself. Its harness and model are chosen
per companion; the default is the worker's harness with a lower-cost model. Each companion turn
runs through the adapter's headless route under a read-only policy, and Shinatama renders the
answer natively. See the specification, sections 4 to 8.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a Codex worker finishes a turn editing `src/lock.rs` -> Expected: a card in the companion pane with a table of the change, the reason quoted from Codex's message and a diagram; the worker's next turn has the same context size as without a companion (spec EX-001).
- **EX-002** (boundary case): Input: the worker is DeepSeek Harness, which has no transcript reader -> Expected: the companion opens with "partial feed" and explains from events and the git delta (spec EX-003).
- **EX-003** (rejection or error case): Input: the companion's model tries to write a file -> Expected: its harness refuses under the read-only policy and the worktree is unchanged (spec EX-005).

## Rejected Alternatives

- **RA-001**: a native fork of the worker session per companion turn; same harness only, not on every harness, costly, inherits write tools.
- **RA-002**: an explaining output style inside the worker; one harness only, paid on every later worker turn, changes the worker.
- **RA-003**: the companion questioning the worker through `shinatama_send`; spends the worker's tokens and is a steering path.
- **RA-004**: the companion as an interactive TUI in a terminal pane; per-turn read-only enforcement is harder and tables and diagrams cannot render uniformly.

## Enforceable Invariants

- **INV-001**: no byte of the worker's worktree is created, changed or deleted by the companion or its harness.
- **INV-002**: nothing the companion produces reaches the worker's PTY, inbox, approval prompts or context.
- **INV-003**: every feed delta reaches the companion wrapped as untrusted data.
- **INV-004**: the worker's reasoning items never enter the feed.
- **INV-005**: a harness hosts a companion only after its read-only register row passes.

## Consequences

- New native pane kind (companion chat) in the tiling host; Markdown rendering with tables and native Mermaid becomes a requirement on the toolkit choice.
- New register rows per harness for the read-only headless route and for context neutrality.
- The adapter contract's `ask_headless` gains a read-only policy used outside the roundtable.

## Confirmation

- Register rows of spec 10.2 pass per harness before that harness is offered.
- A recorded-session test shows equal worker input tokens per turn with and without a companion.
- A planted-instruction fixture (spec EX-004) produces no write and no message to the worker.
