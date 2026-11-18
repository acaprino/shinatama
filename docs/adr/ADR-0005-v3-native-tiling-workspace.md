---
adr_id: ADR-0005
version: 3
title: Tauri desktop tiling workspace over a Rust engine; layout never starts, stops or alters sessions
status: Superseded
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - apps/desktop/**
  modules:
  domains:
    - tiling-workspace
    - desktop-shell
    - layout-persistence
    - panel-lifecycle
    - ui-toolkit
governs_symbols:
depends_on:
  - ADR-0004
created_from_task: windows-only-2026-11-02
proposed_on: 2026-11-02
proposed_at_sha: 65bee163e43b0b9f7b2d71f4e353b9fe4c580df2
accepted_on: 2026-11-02
accepted_at_sha: 65bee163e43b0b9f7b2d71f4e353b9fe4c580df2
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: ADR-0005 v4
changes:
  - EX-009
---

# ADR-0005: Tauri desktop tiling workspace over a Rust engine; layout never starts, stops or alters sessions

<!-- adr-header:start -->
Version 3 · Superseded by ADR-0005 v4 · proposed 2026-11-02 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

This record answers what the main UI is built with, and what moving and restoring its panes may do
to the agents inside them. A Tauri and Rust terminal prototype exists and runs on Windows
(docs/verification/2026-11-02-tauri-terminal-prototype.md); it is a two-pane experiment and closes
no gate. The previous version of this decision excluded a web frontend, and the implementation
plan was being built on a shell that the accepted records still rejected.

Sources, kept apart:

- Decided by the user on 2026-11-02: Tauri is the definitive V1 shell ("Tauri definitivo, sì"),
  after selecting the Tauri and Rust prototype the same day. No further reason is recorded. Tiling,
  draggable tabs, resizable splits and collapsible side panes remain the user's request
  (functional-design 2, goal 14), and so does the terminal-first scope (functional-design 5.16).
- Decided by the user on 2026-11-02: Windows is the only officially supported platform for now; macOS and Linux are deferred, not removed ("al momento supportiamo solo windows ufficialmente"). The user has no Mac or Linux machine.
- Author's calls: the division of authority between the Rust engine and the webview, the reading of
  TL1 as proof rather than selection, and xterm.js as the V1 terminal emulator, which the prototype
  already uses. Exact shortcuts stay builder proposals.
- Not proved: nested split trees, real agent TUIs, IME, high DPI and
  resource limits. The prototype exercised none of them.

The full design is in functional-design 5.16 and technical-design 3.1; this record holds only the
decision.

## Decision Drivers

- The tiling, tab and pane experience the user asked for (goal 14).
- A layout action must never cost a session, a thread or a duplicate PTY writer.
- Process, file, state and credential authority must stay in one audited Rust engine.
- Panels that show untrusted content must not gain filesystem, PTY or credential access.
- UI work must not be built on a shell the records leave undecided.

## Decision

Shinatama's main UI is a Tauri desktop application: a TypeScript frontend in the system webview,
with xterm.js as the terminal emulator, over a Rust engine. The toolkit is chosen; GPUI and egui are
no longer candidates. Electron stays excluded.

The Rust engine owns PTYs, processes, files, shared state and every operating-system effect. The
frontend reaches it only through explicit application commands and declared window permissions; it
holds no PID or PTY handle and no credential, and no generic SQL, shell or filesystem bridge is
exposed to it. Terminal bytes go to the emulator and never into the page as HTML. The application
starts from assets embedded in the executable, with no Node, development server or external browser.

A project workspace holds horizontal and vertical splits with tab groups at the leaves; side panes
are resizable and collapsible; tabs can be dragged between groups or to an edge to split. Moving,
hiding or collapsing a terminal view never restarts or stops its agent, creates a session or changes
its thread; closing a view is distinct from stopping an instance, and stopping needs confirmation.
Panel identity is separate from instance, segment and thread identity: a terminal view references a
live instance and a resumable thread, never a reusable PID or PTY handle or a credential, and
initially there is one interactive terminal view per instance. Layout is versioned JSON in a
shared-database workspace record with a revision and one owning window per workspace; restore brings
back view and resource references and never starts or resumes agents, runs apps or reopens
interactive browser sessions. Only the focused terminal receives input.

TL1 no longer selects a toolkit. It proves this one, with a real agent, nested dock panels, Markdown
source and preview and file navigation, on Windows in WebView2. It stays a G0 row: a failure
changes the design under ADR-0040, through a new version of this
record, and never through a silent fallback.

See functional-design 5.16 and technical-design 3.1.

## Verified Interview Examples

- **EX-001** (nominal case): Input: nested splits and tab moves with live agents, driven by mouse and by keyboard only -> Expected: no session changes; the layout is fully controllable from the keyboard (functional-design 5.16 acceptance).
- **EX-002** (boundary case): Input: a tab drag that is cancelled or dropped on an invalid target -> Expected: the layout model is unchanged (functional-design 5.16; technical-design 3.1; register TL2).
- **EX-003** (boundary case): Input: restart with a saved per-project layout, and with corrupt or unknown layout data -> Expected: the layout survives restart; corrupt or unknown state recovers without lost documents or history; restoring starts no agents, apps or interactive browsers (register TL3).
- **EX-004** (rejection or error case): Input: a second window claims a workspace that another window owns -> Expected: the conflicting ownership is rejected (register TL3).
- **EX-005** (boundary case): Input: many noisy or hidden agent panes and large previews -> Expected: within explicit resource limits, without starving the UI or other panes; budgets are set from measured CPU, memory and responsiveness, not claimed in advance (register TL5).
- **EX-006** (rejection or error case): Input: frontend code calls for a file, a process or a SQL statement outside the explicit application commands -> Expected: the call is refused, because no such permission, plugin or bridge exists.
- **EX-007** (rejection or error case): Input: an agent prints an escape sequence that sets a link, the title, the clipboard or the window state, or text that looks like HTML -> Expected: the sequence is blocked and the text stays terminal text, while terminal protocol replies still work, including from a hidden view.
- **EX-008** (boundary case): Input: the release executable starts on a machine with no Node and no development server -> Expected: it runs from its embedded assets, and starts no process until the user asks.
- **EX-009** (rejection or error case): Input: TL1 fails on Windows in WebView2 -> Expected: no further UI work is built on it; the failure is recorded and this decision is revised, with no silent switch of toolkit.

## Rejected Alternatives

- **RA-001**: GPUI with GPUI Kit, or egui with `egui_dock` or `egui_tiles`; the previous candidates, never prototyped, not chosen by the user's decision of 2026-11-02. No claim is made that they could not reproduce the style.
- **RA-002**: the earlier terminal-only shell; superseded on 2026-10-29 because the user asked for tiling, draggable tabs, resizable splits and collapsible side panes (functional-design 2, goal 14; 5.16).
- **RA-003**: Electron; excluded by the user on 2026-10-29, and the decision of 2026-11-02 names Tauri only.
- **RA-004**: keeping Tauri as an experiment and selecting the toolkit later from evidence; author's call, rejected because the UI steps would be built on an undecided shell, and the user decided.
- **RA-005**: letting the frontend act directly through the Tauri filesystem, shell or HTTP plugins; author's call, rejected because it would split process and file authority across two languages and expose it to rendered content.

## Enforceable Invariants

- **INV-001**: no layout operation (split, move, hide, collapse, maximize, close view, restore) starts, stops, restarts or resumes an agent, creates a session or changes a thread; stopping an instance is a separate, confirmed action.
- **INV-002**: saved layout JSON holds no PID or PTY handle, dirty buffer, rendered content or credential (technical-design 3.1; technical-design I5).
- **INV-003**: tree operations are validated and atomic: cycles, duplicate membership, invalid active tabs, non-finite or non-positive weights and excessive nesting are rejected, and a cancelled drag mutates nothing.
- **INV-004**: ordinary input goes to the focused terminal only; a modal prevents background keystrokes and paste.
- **INV-005**: one owning window per workspace and, in this revision, at most one interactive terminal view per instance; cross-window live PTY transfer is not offered.
- **INV-006**: a smaller window never discards a pane or makes it unreachable, and a collapsed terminal keeps its last usable PTY size.
- **INV-007**: a future browser or mini-app panel has no ambient filesystem, PTY, credential or Rust-command access and is isolated from the main webview; gated panel kinds show an unavailable placeholder until enabled.
- **INV-008**: every operating-system effect goes through the Rust engine by an explicit application command; no filesystem, shell, HTTP or opener plugin and no generic SQL bridge is available to the frontend, and adding a plugin is a recorded decision.
- **INV-009**: terminal output never enters the page as HTML, and the main window's content security policy excludes frames and remote resources.
- **INV-010**: the application starts from embedded assets, needs no Node, development server or external browser, and keeps no debugging port open in normal use.

## Consequences

- WebView2 becomes a runtime dependency on Windows (ADR-0048). If macOS and Linux are resumed, WKWebView and WebKitGTK must pass TL1 first; nothing about them is claimed.
- Gates TL1 (G0, in the first-milestone set) to TL6 stay; TL1 is reworded from choosing a native toolkit to proving this one, and TL1 needs a plan step before the UI steps C06 to C09.
- To be realigned once this version is accepted: FRD-0001 R22 and R51 through the FRD method; functional-design 2 non-goals and 3; technical-design 3.1; the register row TL1; the prototype note and the `Cargo.toml` comments, which still call Tauri experimental. ADR-0040 EX-001 is a dependent to revise. Records that say "native" for panes (ADR-0002, ADR-0004, ADR-0007, ADR-0039, ADR-0040) are dependents to review.
- ADR-0002's native Mermaid requirement must be restated as a local isolated renderer with non-executable input and no ambient access or remote resources.
- The first UI milestone stays terminal-first (project selector, terminal tiles, leaf tab groups, terminal profiles); file navigator, thread panes, session browsing and hop surfaces come later (functional-design 5.16; ADR-0040). Shinatama has no Git Diff tab or dedicated diff editor, including in future scope; this does not change diff output inside a hosted agent or the technical comparisons used by the engine.
- Non-terminal panes must be declared in functional-design 5.16: the DeepSeek Harness ACP pane (ADR-0007) and the companion chat (ADR-0002). Session panels follow ADR-0023; browser panes keep their own gates, and tiling does not authorize a new browser engine, an automatic fallback or browser session transfer (ADR-0035, ADR-0036).
- Markdown rendering executes no scripts, embedded commands or unapproved remote requests; external writes are detected and never silently overwritten; DOCX and visual Markdown editing need fidelity fixtures (TL4, TL6).
- Key bindings wait for register U3; Windows is open (O3).

## Confirmation

- TL1 passes on Windows with the WebView2 version and gaps recorded; then TL2 to TL6 pass with records, and the acceptance list of functional-design 5.16 is demonstrated.
- Not yet confirmed: the Windows prototype gives partial evidence for TL1, TL2, TL3 and TL5 and closes none of them (docs/verification/2026-11-02-tauri-terminal-prototype.md).
