---
adr_id: ADR-0002
version: 2
title: Companion pane fed by the canonical transcript, harness chosen per companion
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
    - explain-mode
    - read-only-session
    - adapter-headless
governs_symbols:
depends_on:
created_from_task: init-abfe6369faa4489a915c5f057a84e467
proposed_on: 2026-11-01
proposed_at_sha: 43bccf3a34205f49473591d7d8ad43babfcd3e71
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - EX-006
---

# ADR-0002: Companion pane fed by the canonical transcript, harness chosen per companion

<!-- adr-header:start -->
Version 2 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet (`Cargo.toml` workspace with no members). The user asked for a
learning or explaining mode: a read-only session beside a working agent instance that explains
what is happening in the project (2026-10-31). The question this record answers is where that
session gets its knowledge of the worker, which harness runs it, and how "read-only" is
guaranteed. The full design is in `docs/specs/companion-explainer.md`; this record holds only the
decision and keeps the numbering of the specification's invariants; each example carries a
"(spec EX-n)" tag where its number differs. Version 2 amends version 1 in: EX-006 (the pane takes presets, not typed questions), the Decision (confinement B and its read allow-list) and the Consequences.

## Decision Drivers

- The worker's context, tokens and behaviour must not change.
- The same behaviour on every hosted harness (Shinatama's uniformity role).
- Read-only that survives a mistaken harness policy, and no steering path to the worker.
- Answers rendered as Markdown with tables and diagrams.

## Decision

The companion is a separate session that Shinatama feeds with per-turn deltas of the worker's
canonical transcript (engine 5.1), the event feed and the git state, wrapped as untrusted data
and escaped. It never forks or reads the worker's native session itself. Its harness and model
are chosen per companion; the default is the worker's harness with a lower-cost model. Each
companion turn runs through the adapter's headless route under a read-only policy (A), inside an
operating-system confinement of its process (B) whose only writable place is its own scratch
directory, which is also its harness home, so the worktree, `.git` and ignored paths are read-only to it, and the worker's agent home, the
system home, other projects and Shinatama's own data directory are not readable by it (it gets
read-only copies of its credentials and skills only),
and without any channel to the worker (C). Where
confinement is not implemented for a platform, the companion is an opt-in preview with the badge
"read-only: harness policy only". Shinatama keeps the companion's conversation and cost in its
own record. Shinatama renders the answer natively. A thread marked `summaries_excluded` gets no
companion. See the specification, sections 4 to 8.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a Codex worker finishes a turn editing `src/lock.rs` -> Expected: a card whose table lists the files named by the turn's events and tool calls, with a quoted or inference-labelled reason; the worker's next turn has the same input token count as without a companion (spec EX-001).
- **EX-002** (boundary case): Input: the worker finishes four turns while the companion answers the first -> Expected: turns two to four arrive as one coalesced request naming all three; none missing (spec EX-002).
- **EX-003** (boundary case): Input: the worker is Hermes Agent, which has no transcript reader -> Expected: the companion opens with "partial feed" and explains from events and git state (spec EX-003).
- **EX-004** (rejection or error case): Input: a tool result containing an injected instruction and the closing wrapper tag -> Expected: the content stays escaped inside its wrapper and no message reaches the worker (spec EX-004).
- **EX-005** (rejection or error case): Input: the companion's model asks its harness to write in the worktree and in an ignored directory -> Expected: both refused; the content hash of the whole worktree tree with its ignored paths, of its git directory (resolved for a linked worktree) and of the worker's agent home is equal before and after with the worker idle (spec EX-005).
- **EX-006** (boundary case): Input: the user triggers an action in the pane during a card turn -> Expected: it is queued and runs before any queued automatic card; no two turns overlap (spec EX-007).
- **EX-007** (rejection or error case): Input: a thread marked `summaries_excluded` -> Expected: no companion is offered (spec EX-008).
- **EX-008** (boundary case): Input: a companion is opened on a provider other than the worker's -> Expected: a first-use warning appears before any feed is sent; declining opens nothing (spec EX-009).

## Rejected Alternatives

- **RA-001**: a native fork of the worker session per companion turn; same harness only, not on every harness, costly, inherits write tools.
- **RA-002**: an explaining output style inside the worker; one harness only, paid on every later worker turn, changes the worker.
- **RA-003**: the companion questioning the worker through `shinatama_send`; spends the worker's tokens and is a steering path.
- **RA-004**: the companion as an interactive TUI in a terminal pane; per-turn read-only enforcement is harder and tables and diagrams cannot render uniformly.
- **RA-005**: relying on the harness's read-only policy alone; it is the only barrier between a mistaken policy and the worktree.

## Enforceable Invariants

- **INV-001**: no byte of the worker's worktree, ignored paths and `.git` included, is created, changed or deleted by the companion or its harness; enforced by policy A and by confinement B, and where B is not implemented for the platform the badge says so and the companion is an opt-in preview.
- **INV-002**: nothing the companion produces reaches the worker's PTY, inbox, approval prompts or context, or changes the worker's agent home, hooks, settings and session store.
- **INV-003**: every feed delta reaches the companion wrapped as untrusted data and escaped so its content cannot close the wrapper.
- **INV-004**: the worker's reasoning items never enter the feed.
- **INV-005**: a stated reason in a card is a quotation from the worker's transcript or labelled as inference; measured by a rubric, not enforceable at runtime.
- **INV-006**: a harness is offered as a companion host only after its read-only row passes for policy A; the plain badge "read-only (confined)" also needs the row for confinement B on that platform.
- **INV-007**: at most one headless turn is in flight per companion session.

## Consequences

- New native pane kind (companion chat) in the tiling host, a second exception to functional-design 5.16 that must be declared there; Markdown rendering with tables and native Mermaid becomes a requirement on the toolkit choice.
- technical-design 4.2 gains thread kind `companion` with its own link column (not `parent_thread_id`) and a table for the companion's messages and per-card cost (`summary_log` unchanged); technical-design 5.5 gains a read-only-with-reads policy value, a stateless mode and `turn_end_signal` as a prerequisite; technical-design 7.6 gains wrapper escaping; technical-design 4.7 gains the companion's provider data flow; functional-design 5.1 and 5.16 gain the second native chat surface.
- Privacy: the companion's file reads are not redacted; a companion on a provider other than the worker's shows a first-use warning; `summaries_excluded` threads get no companion; the user's learning memory goes to the companion's provider (ADR-0042).
- New register rows per harness and per platform (spec 10.2), including context neutrality, channel isolation and sidecar coverage.
- The pane's input is preset actions and selections only (ADR-0041), its ledger of what was explained is ADR-0042 and its web search through the harness's own web tool is ADR-0043; all three depend on this record, and ADR-0043 is the one place where the read-only companion has any web access.

## Confirmation

- Register rows of spec 10.2 pass per harness (and per platform for confinement) before that harness is offered.
- A recorded-session test shows equal worker input tokens per turn with and without a companion.
- The worktree-hash check of EX-005 and the planted-instruction fixture of EX-004 pass on every offered harness.
