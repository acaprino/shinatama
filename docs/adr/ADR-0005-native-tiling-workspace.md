---
adr_id: ADR-0005
version: 1
title: Native Rust desktop tiling workspace; layout never starts, stops or alters sessions
status: Superseded
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - tiling-workspace
    - native-ui
    - layout-persistence
    - panel-lifecycle
    - rust-toolkit
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
superseded_by: ADR-0005 v2
changes:
---

# ADR-0005: Native Rust desktop tiling workspace; layout never starts, stops or alters sessions

<!-- adr-header:start -->
Version 1 · Superseded by ADR-0005 v2 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. An earlier decision made the UI terminal-only. The question this
record answers is what the main UI is built with, and what moving and restoring its panes may do
to the agents inside them.

Origin: decided by the user on 2026-10-29: "Native tiling, draggable tabs, resizable splits and
collapsible side panes ... with a Rust UI, not a web frontend" (functional-design 2, goal 14),
and no "HTML/CSS/JavaScript frontend, Tauri or Electron for the main UI" (non-goals). The
lifecycle rules sit under the decided heading of functional-design 5.16. Rust comes from the
user's original request; macOS and Linux are required and Windows is assumed (author's call,
functional-design 7 item 1, register O3). The toolkit is the author's call, "not selected or
prototyped", and so is the layout sketch (technical-design 3.1, [C]); exact shortcuts are
builder proposals. The terminal-first scope was decided by the user on 2026-10-30 and confirmed
on 2026-11-01 (functional-design 5.16). The full design is in functional-design 5.16 and
technical-design 3.1; this record holds only the decision.

## Decision Drivers

- The tiling, tab and pane experience the user asked for (goal 14).
- A layout action must never cost a session, a thread or a duplicate PTY writer.
- One Rust toolchain for the PTY, the emulator and the native UI on three systems (functional-design 3).
- Panels that show untrusted content must not gain filesystem, PTY or credential access.

## Decision

Shinatama's UI is a native Rust desktop application. A project workspace holds horizontal and
vertical splits with tab groups at the leaves; side panes are resizable and collapsible; tabs can
be dragged between groups or to an edge to split. There is no web frontend and no Tauri or
Electron shell; this supersedes the terminal-only decision, and earlier terminal and HTML mockups
are interaction references only. Moving, hiding or collapsing a terminal view never restarts or
stops its agent, creates a session or changes its thread; closing a view is distinct from
stopping an instance, and stopping needs confirmation. Panel identity is separate from instance,
segment and thread identity: a terminal view references a live instance and a resumable thread,
never a reusable PID or PTY handle or a credential, and initially there is one interactive
terminal view per instance. Layout is versioned JSON in a shared-database workspace record with a
revision and one owning window per workspace; restore brings back view and resource references
and never starts or resumes agents, runs apps or reopens interactive browser sessions. Only the
focused terminal receives input. The toolkit is pinned only after the combined prototype gate
passes; GPUI with GPUI Kit / `gpui-component` is the first candidate, egui with `egui_dock` or
`egui_tiles` the alternative, and the choice must not force browser startup. See
functional-design 5.16 and technical-design 3.1.

## Verified Interview Examples

- **EX-001** (nominal case): Input: nested splits and tab moves with live agents, driven by mouse and by keyboard only -> Expected: no session changes; the layout is fully controllable from the keyboard (functional-design 5.16 acceptance).
- **EX-002** (boundary case): Input: a tab drag that is cancelled or dropped on an invalid target -> Expected: the layout model is unchanged (functional-design 5.16; technical-design 3.1; register TL2).
- **EX-003** (boundary case): Input: restart with a saved per-project layout, and with corrupt or unknown layout data -> Expected: the layout survives restart; corrupt or unknown state recovers without lost documents or history; restoring starts no agents, apps or interactive browsers (register TL3).
- **EX-004** (rejection or error case): Input: a second window claims a workspace that another window owns -> Expected: the conflicting ownership is rejected (register TL3).
- **EX-005** (boundary case): Input: many noisy or hidden agent panes and large previews -> Expected: within explicit resource limits, without starving the UI or other panes; budgets are set from measured CPU, memory and responsiveness, not claimed in advance (register TL5).

## Rejected Alternatives

- **RA-001**: an HTML/CSS/JavaScript frontend, Tauri or Electron for the main UI; excluded by the user's decision of 2026-10-29 for a native Rust UI (functional-design 2). No further reason is recorded.
- **RA-002**: the earlier terminal-only shell; superseded on 2026-10-29 because the user asked for tiling, draggable tabs, resizable splits and collapsible side panes (functional-design 2, goal 14; 5.16).

## Enforceable Invariants

- **INV-001**: no layout operation (split, move, hide, collapse, maximize, close view, restore) starts, stops, restarts or resumes an agent, creates a session or changes a thread; stopping an instance is a separate, confirmed action.
- **INV-002**: saved layout JSON holds no PID or PTY handle, dirty buffer, rendered content or credential (technical-design 3.1; technical-design I5).
- **INV-003**: tree operations are validated and atomic: cycles, duplicate membership, invalid active tabs, non-finite or non-positive weights and excessive nesting are rejected, and a cancelled drag mutates nothing.
- **INV-004**: ordinary input goes to the focused terminal only; a modal prevents background keystrokes and paste.
- **INV-005**: one owning window per workspace and, in this revision, at most one interactive terminal view per instance; cross-window live PTY transfer is not offered.
- **INV-006**: a smaller window never discards a pane or makes it unreachable, and a collapsed terminal keeps its last usable PTY size.
- **INV-007**: a future browser or mini-app panel has no ambient filesystem, PTY, credential or Rust-command access; gated panel kinds show an unavailable placeholder until enabled.

## Consequences

- Gates TL1 (G0, in the first-milestone set) to TL6 are added; TL1 is the toolkit prototype gate of technical-design 3.1, not the UI milestone. The first UI milestone is terminal-first (project selector, terminal tiles, leaf tab groups, terminal profiles); file navigator, thread panes, session browsing, hop and Daodan surfaces come later (functional-design 5.16; ADR-0040).
- Correction, 2026-11-02: the earlier inclusion of "diffs" among later surfaces was an unsupported inference. The user explicitly excludes a Shinatama Git Diff tab and dedicated diff editor from the product, including future scope. This does not change diff output inside a hosted agent or the technical comparisons used by the engine.
- Native panes other than terminals must be declared in functional-design 5.16: the DeepSeek Harness ACP pane (ADR-0007) and the companion chat (ADR-0002). Session panels follow ADR-0023; browser panes keep their own gates, and "Tiling does not authorize a new engine, web frontend, automatic fallback or browser session transfer" (browser-integration; ADR-0035, ADR-0036).
- Markdown rendering executes no scripts, embedded commands or unapproved remote requests; external writes are detected and never silently overwritten; DOCX and visual Markdown editing need fidelity fixtures (TL4, TL6). Native HTML rendering is not a full browser or DOCX engine.
- The UI library, crates and shortcuts stay the builder's (functional-design 8); key bindings wait for register U3; Windows is open (O3).

## Confirmation

- The combined prototype gate of technical-design 3.1 (register TL1: a real agent, nested dock panels, Markdown source and preview, file navigation) passes on macOS, Linux and assumed Windows with versions and gaps recorded; then TL2 to TL6 pass with records, and the acceptance list of functional-design 5.16 is demonstrated.
- Nothing has been run: the register says these gates "have not been executed", and technical-design 3.1 claims no implemented feature, passed gate or measured budget.
