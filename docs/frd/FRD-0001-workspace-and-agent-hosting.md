# FRD-0001: Workspace and agent hosting

Shell decision, user, 2026-11-02: Tauri is the definitive V1 shell over a Rust engine
(ADR-0005, ADR-0047). R22 and R51 are amended accordingly; M0 and the platform
gates remain open.

Platform decision, user, 2026-11-02: Windows is the only officially supported platform for now;
macOS and Linux are deferred. R15, R50, R51 and R55 are amended accordingly.

Resilience decision, user, 2026-11-02: host stability takes priority over an unstable
subprocess; resource use must be adaptively calibrated. Emergency termination of
only the responsible process tree is explicitly authorized after preventive measures.
R59-R63 and the emergency exception in R29 record this request; implementation
choices and open proofs are in [the resilience amendment](../implementation/2026-11-02-host-resilience.md).

| | |
|---|---|
| Version | 1 (Markdown record; the JSON record is version 1 at `c84d28e`) |
| Status | Accepted 2026-11-01 (this Markdown text; the JSON evidence record, where one exists, stays Proposed until `frd_tool.py promote`) |
| Date | 2026-11-01; R15, R22, R50, R51 and R55 amended 2026-11-02 for shell/platform; R29 amended and R59-R63 added for the user's resilience decision |
| Sources | functional-design 1, 2, 3, 4, 5.1, 5.7, 5.9, 5.12, 5.16, 6; technical-design 3, 3.1, 5.5.3, 5.7 |
| Governing ADRs | ADR-0004, ADR-0005, ADR-0006, ADR-0007, ADR-0011, ADR-0039, ADR-0040, ADR-0047 |
| Register sections | TL (tiling), P (platform and terminal), U (skins and UI), DS (DeepSeek Harness) |
| Related records | FRD-0002 (where the agents live), FRD-0004 (project manager), FRD-0006 (sessions surface), FRD-0012 (companion pane), FRD-0017 (install and release) |

## 1. Purpose and scope

Shinatama is a local-first desktop workspace, with a Rust engine and a Tauri shell, that hosts the real coding
agents in tiled terminal panes. This record states what "hosts" means: which agents, how they
run, what the workspace draws around them, how panes are arranged and restored, and what the
first UI milestone contains. It does not cover where an agent's program and home live
(FRD-0002), the hop (FRD-0005) or the session browser (FRD-0006).

Scope clarification, user, 2026-11-02: a Shinatama Git Diff tab or dedicated
diff editor is excluded from the product, including future scope. The old
mockup and the former ADR-0005 "come later" wording did not establish such
a requirement. Hosted agents may still display their own diff output.

Vocabulary (technical-design 2): an **agent** is one of Claude Code, Codex, OpenCode, Pi, Hermes
Agent and DeepSeek Harness; an **instance** is a running agent process hosted in one pane; a
**pane** or **view** is a workspace surface, distinct from the instance it shows.

## 2. Requirements

### 2.1 The hosted agents

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Shinatama hosts Claude Code, Codex, OpenCode, Pi, Hermes Agent and DeepSeek Harness as real processes. "Deep seek code" is read as DeepSeek Harness (`dsh`), DeepSeek's own agent; "hermes" is Hermes Agent by Nous Research. | U 2026-10-31 (set); A (the two readings, open O9) | M0 | E1, HM1, DS1 | 0006 |
| R02 | Hermes Agent and DeepSeek Harness join with every adapter capability declared `unsupported`; a capability is claimed for them only once its register row passes, never from documentation alone. | U | X | HM1 to HM14, DS1 to DS15 | 0006 |
| R03 | Grok is not hosted. Adding it later, or Gemini beyond launch-only, means implementing the adapter contract, recording fixtures and passing the suite, with no change to the engine, the database, the UI or the roundtable. | U (Grok out); S (adapter route) | X | — | 0006, 0011 |
| R04 | Gemini has a launch-only adapter in v1: it starts through the sidecar, with no hop and no search; it is never a hop source or destination. External terminal selection is not offered for any agent, because every agent runs in Shinatama's own panes. | A | V1 | SC11 | 0006 |
| R05 | DeepSeek Harness, which ships no terminal UI, runs in a native ACP pane: Shinatama runs `dsh --profile acp` and is its ACP client, drawing messages, tool calls and permission requests with the active skin and a harness accent colour, and answering permissions through ACP. The pane has the same tab, split, hop and overview behaviour as a terminal tab. | A (open O9) | M0 | DS1, DS2, DS4 | 0007 |
| R06 | In the ACP pane the agent's loop, tools, models and session store stay DeepSeek Harness's own; the pane renders ACP messages and relays the user's input and permission answers, nothing else. No tool runs without the user's answer to `session/request_permission`. | A | M0 | DS4 | 0007 |
| R07 | `dsh web` is offered only as an explicit "open in browser" action in the user's own browser, for what the pane lacks (the web UI's fork and export); it is never embedded, never a hop path, starts only on request and leaves no process after the window closes. | A | V1 | DS14 | 0007 |
| R08 | Every agent launch passes through one launch point, which runs the checkup (agents present, Daodan, login detection). | A | X | D7 | 0013 |

### 2.2 Real agents in Shinatama-owned pseudo-terminals

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R09 | Each hosted agent that ships a terminal UI runs as its real CLI in a pseudo-terminal owned by the Shinatama window process that hosts it; the user types into the real agent. Shinatama never replays or simulates the conversation. | S (anti-goal) | M0 | P1 | 0004 |
| R10 | What the agent draws inside the PTY stays the agent's: layout, text, tables, tool calls, reasoning, prompts, boxes and any 24-bit colour. Shinatama does not redraw those widgets, present them as native controls or rewrite truecolour. Redrawing the live chat from a structured stream is not planned, with the ACP pane (R05) as the only exception. | U 2026-10-29 | M0 | P2, P10 | 0004 |
| R11 | Shinatama draws its own rich views only where the data is already structured and read-only: find, the hop preview, the roundtable, the overview and the companion pane. | S | V1 | — | 0004 |
| R12 | Around and over the pane Shinatama draws: a frame with border and title; a top bar with thread, folder, branch, cost and the agent's icon; a bottom bar of keys; side panels such as the thread column; and native overlays (hop dialog, Daodan palette, find). All are coloured by the skin. | S | M0 (frame, bars); V1 (side panels, overlays) | P11, P12 | 0004 |
| R13 | A native overlay leaves the emulator screen intact, keeps parsing the agent's output while open and restores focus to the originating pane when dismissed. | S | V1 | P12 | 0004 |
| R14 | When a side panel narrows a pane, the agent receives a narrower terminal and the resize is forwarded correctly: rows and columns come from the content bounds excluding chrome, resize events are coalesced and the final size delivered, and a collapsed view keeps its last usable size, never zero. | S | M0 | P1, P11, TL2 | 0004, 0005 |
| R15 | The terminal emulator hosts the six agents faithfully on Windows: text and attributes match a reference terminal; wide characters, emoji, CJK and combining marks have correct widths; alternate screen and scroll regions work; mouse, wheel, bracketed paste and advanced keys (kitty protocol, modifyOtherKeys, Shift+Enter, Alt, AltGr) reach the agent; agent images (kitty, sixel, iTerm2) are drawn in the pane or cleanly dropped; no flicker. | S | M0 | P1 to P8, U7 | 0004 |
| R16 | Containment: a hosted agent's escape sequences cannot write the clipboard, read the screen, retitle the window or inject input unless a documented decision allows it; each decision on OSC and CSI handling is written down. | A | M0 | P9 (SEC) | 0004 |
| R17 | Stopping an instance kills its whole process tree (job object on Windows, process group on Unix); a Shinatama crash leaves no orphan agent or MCP server. Closing a view is not stopping an instance. | A | M0 | P14 | 0004, 0005 |
| R18 | The environment given to a child is child-only (`SHINATAMA_*`, `TERM`, `COLORTERM`); nothing leaks to the parent or to sibling instances. | S | M0 | P15 | 0013 |
| R19 | Agents are spawned with an argument vector and no shell, on every platform, including Windows paths with spaces, parentheses, ampersands, non-ASCII and over 260 characters; `.cmd` shims are never used as the spawned program (Pi runs as `node … cli.js`). | A | M0 | P16 | 0019 |
| R20 | Four instances streaming at once stay within measured budgets: frame time, CPU and keystroke-to-echo latency are measured and the targets set from the data (starting target: echo under 20 ms at the 95th percentile). Background panes are parsed but not drawn. | A | M0 | P7, U6, TL5 | 0005 |
| R21 | Terminal capability fallbacks work and stay legible: 16-colour only, no Unicode box drawing, no Nerd Font, `NO_COLOR`, screen readers; keyboard-only operation is complete. | S | V1 | P19, U4 | 0039 |

### 2.3 Native tiling workspace

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R22 | The main UI is a Tauri desktop application: a TypeScript frontend in the system webview over a Rust engine, with xterm.js as the terminal emulator. No Electron shell, and no Node, development server or browser needed to start. Earlier terminal and HTML mockups are interaction references, not binding. | U 2026-11-02 | M0 | TL1 | 0005 |
| R23 | A project workspace holds horizontal and vertical splits with tab groups at the leaves; side panes are resizable and collapsible; tabs and tiling coexist. | U 2026-10-29 | M0 | TL1, TL2 | 0005 |
| R24 | The user resizes panes by dragging splitters and drags tabs to reorder them, move them between groups or create an edge split; valid drop targets are previewed and a cancelled or invalid drop changes nothing. | U 2026-10-29 | M0 | TL2 | 0005 |
| R25 | Every layout action has a keyboard equivalent (focus, resize, tab moves, splitting, side-pane collapse and restore); ordinary input goes to the focused agent, and workspace actions use documented explicit shortcuts or a command palette. Key names in the design (`^H`, `^K`, `^E`) are placeholders until register row U3 settles bindings that neither the agents nor the desktop consume. | U 2026-10-29 (equivalents); S (placeholders) | M0 | U3, U4 | 0005 |
| R26 | Only the focused terminal receives input through its owning runtime; while a modal is focused no keystroke or paste reaches a background terminal. | S | M0 | TL2 | 0005 |
| R27 | A pane can be temporarily maximized and its prior layout restored; minimum usable pane sizes are enforced, and a smaller window never discards a pane or makes it unreachable. | U 2026-10-29 | M0 | TL2, U5 | 0005 |
| R28 | Layout is saved and restored per project and window workspace as versioned JSON in a shared-database workspace record, with a revision for optimistic updates and one owning window per workspace; corrupt or unknown data falls back to the last valid layout without deleting documents or session history. | U 2026-10-29 (save and restore); A (shape) | M0 | TL3 | 0005, 0009 |
| R29 | Moving, hiding or collapsing a terminal view never restarts or stops its agent, creates a session or changes its thread; stopping a live instance requires confirmation, except the user-authorized emergency resource policy in R61; closing a view with a dirty document offers save, discard or cancel. | U 2026-10-29; emergency exception U 2026-11-02 | M0 | TL2, TL4, RES3 | 0005 |
| R30 | Restoring a layout brings back view and resource references, never stale PIDs or PTY handles; a saved layout alone never starts or resumes an agent, executes an app or reopens an interactive browser session; unavailable resources are shown as unavailable; a gated panel kind shows an unavailable placeholder until it is enabled. | U 2026-10-29 | M0 | TL3 | 0005, 0036 |
| R31 | Layout JSON holds no PID, PTY handle, dirty buffer, rendered content or credential. Tree operations are validated and atomic: cycles, duplicate membership, invalid active tabs, non-finite or non-positive weights and excessive nesting are rejected; empty groups and redundant splits are collapsed. | A | M0 | TL3 | 0005 |
| R32 | Panel identity is separate from instance, segment and thread identity; a terminal view references a live instance and a resumable thread. Initially there is one interactive terminal view per instance; cross-window live PTY transfer is not offered. | A | M0 | TL2 | 0005 |
| R33 | PTY reads, parsing, file I/O and previews never block the UI thread; queues and resources are bounded and scheduled fairly so a noisy agent cannot freeze other panes. | A | M0 | TL5 | 0005 |

### 2.4 Panels beyond the terminal

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R34 | The agent terminal panel offers selection, scrollback and focus isolation; several agents are visible together. | U 2026-10-29 | M0 | P5, TL2 | 0005 |
| R35 | The file navigator shows the project tree and search and opens a file only by an explicit open into a tab or pane; it never executes a file implicitly and gives no access to arbitrary roots. | U 2026-10-29 | V1 | TL4 | 0005 |
| R36 | The Markdown panel offers rendered reading, source editing and a source/preview split. Rendering never executes scripts, embedded commands or unapproved remote requests; links and assets resolve only within authorized roots, symlinks included; parser size, depth and rendering work are bounded. | U 2026-10-29 | V1 | TL4 | 0005 |
| R37 | Direct visual Markdown editing is not delivered until editor and syntax fidelity checks pass; visual editing preserves unsupported syntax rather than discarding it. | U 2026-10-29 | V1 | TL4 | 0005 |
| R38 | Editing provides undo and redo, a visible dirty state and explicit save. When an open file changes outside the editor, agent writes included, the buffer is preserved and reload, compare or resolution is offered; nothing is overwritten silently. Save checks the file version against the version read and uses safe replacement where supported. | U 2026-10-29 | V1 | TL4 | 0005 |
| R39 | The file and document preview panel shows supported text and image artifacts. DOCX reading needs a separate renderer with declared fidelity; DOCX editing and full Office support are not implied. | U 2026-10-29 | V1 | TL6 | 0005 |
| R40 | Interactive embedded browsing and mini-app hosting stay unavailable until their separate design and security gates pass; a future browser or mini-app panel has no ambient filesystem, PTY, credential or Rust-command access; JanuraCode app compatibility is not assumed. Static previews follow FRD-0011. | U 2026-10-29 | P1 | TL6, B-series | 0005, 0036 |
| R41 | Document buffers are shared by canonical file identity rather than duplicated as independent writable buffers; crash-recovery buffers need their own privacy and retention design and are not stored in the layout. | A | V1 | TL4 | 0005 |

### 2.5 Skins and terminal profiles

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R42 | Skins are selectable at runtime and stored in the configuration; UI colours come from a skin definition, never hard-coded. The default is the Shinatama skin (midnight navy, cobalt and ice lines, pink as the only warm accent); Section 9, TCTF and Sumi are the others. | U (default and selectable); S (set) | M0 | U1, U2 | 0039 |
| R43 | A skin controls only the chrome: window, menus, tabs, side panels, UI typography, borders and panel states. The DeepSeek Harness ACP pane is drawn with the skin and a harness accent colour. | S | M0 | U2 | 0039 |
| R44 | Each harness has a versioned terminal profile (monospace font and size, cell metrics, cursor, 16-colour ANSI palette, default foreground and background) that gives it a stable identity in its pane; Hermes Agent gets one like the other four. A profile is presentation data, never layout or session identity, and never replaces or reinterprets the TUI. | U 2026-10-29 (profiles); S (Hermes) | M0 | U2, P10 | 0039 |
| R45 | A missing or invalid profile or font falls back to the built-in monospace profile without restarting the PTY; the terminal is initialised with its profile before rows and columns are derived, and a font or metric change delivers one final resize. | U 2026-10-29 | M0 | TL2 | 0039 |
| R46 | "Follow Shinatama skin" is an explicit opt-in compatibility option, off by default; it remaps only the default colours and the 16-colour ANSI palette and never rewrites truecolour. Every remapped colour meets 4.5:1 contrast against its background or is exempted in writing. | U 2026-10-29 (opt-in); S (scope) | V1 | P10, U1 | 0039 |
| R47 | Secondary text reaches 4.5:1 contrast in every skin; there are no glowing shadows; scan lines and cut corners named by a skin are optional decoration, not requirements. Changing the skin live applies to the chrome without restart or flicker. | S | M0 | U1, U2, U8 | 0039 |
| R48 | Each agent has an icon and an accent colour shown in the tab, the overview and the project manager, rendered natively with a text label or letter badge fallback and accessible names independent of colour. Asset sourcing and licensing are the builder's responsibility. | U (icons on tabs); A (fallbacks) | V1 | I10, X3 | 0028 |
| R49 | Bundled fonts and their licences are part of the dependency notice. | A | M0 | X3 | 0039, 0047 |

### 2.6 Platform, language and shell

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R50 | The Shinatama engine is written in Rust. Windows is the only officially supported platform; ConPTY, Windows paths, environment and trust handling are part of the first milestone. macOS and Linux are deferred. | U 2026-11-02 | M0 | P1, P13, P17 | 0047 |
| R51 | The Rust crates and npm packages the design relies on (PTY, terminal emulation, Tauri, SQLite) exist, are maintained, are licence-compatible and work on Windows; TL1 proves the Tauri shell there, and it must not force browser startup. | A | M0 | P17, TL1 | 0005, 0047 |
| R52 | Shinatama is local-first: no account, no hosted service; threads and settings live on local disk. | S | X | — | 0008 |
| R53 | No second scheduler and no resident service keeps executing work after the owning application window closes. | S (anti-goal) | X | P14, HM2 | 0008 |
| R54 | Agent behaviour Shinatama depends on (resume flags, session stores, hooks, HTTP APIs) is relied on only after it is verified against the installed version with real captured fixtures; documentation alone is not proof. | S | X | E1, E5 | 0010 |
| R55 | Startup time, idle memory, per-instance memory and binary size are measured on Windows and the targets set from the measurements. | A | V1 | P18 | 0047 |

### 2.7 Delivery scope of the first UI milestone

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R56 | The terminal-first UI milestone contains the project selector, resizable terminal tiles, leaf tab groups and per-harness terminal profiles, and nothing else: no git integration, file navigator, diffs, thread side panes, session browsing, hop or Daodan surfaces. It is distinct from register row TL1, the toolkit prototype gate. | U 2026-10-30 | M0 | TL1, TL2 | 0040 |
| R57 | Later workspace surfaces (side panes, file navigator, Markdown, previews, sessions surface, companion pane) are delivered only after their dependencies and G0/G1 gates close; no downstream surface is treated as deliverable in the same baseline before that. | U 2026-10-31 | V1 | register 0 | 0040 |
| R58 | A failed `G0` row of the first milestone changes the design; nothing is built on it before it passes. | S | X | register 0 | 0010, 0040 |

### 2.8 Host resilience and adaptive resource control

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R59 | Host stability has priority: a hosted subprocess crash, hang, resource exhaustion, broken pipe or malformed/excessive output must not crash or indefinitely block Shinatama, corrupt saved state or stop healthy sibling sessions. Qualify this against an explicit supported resource envelope and reproducible fault tests. | U 2026-11-02 (priority); A (testable envelope) | M0 | RES1, RES2, TL5 | 0004, 0008 |
| R60 | Measure host/renderer, each process tree and system pressure with bounded local telemetry; account for committed and resident memory, CPU, process count and I/O queues. Aggregate admission across windows preserves measured headroom for Shinatama and the OS; workload limits must not impose the same restrictive cap on the host. | U 2026-11-02 (resource calibration); A (metrics and mechanism) | M0 | RES2, P18 | 0008, 0009 |
| R61 | Adapt resource budgets conservatively to available capacity and healthy observed workloads, within fixed safety ceilings and with stable recovery thresholds. Reduce optional load and defer new launches first; if a responsible subprocess still threatens host stability, automatically stop only its tree, retain already saved state and show the reason and available resume action. No blind termination for unattributed pressure, unbounded budget growth to accommodate leaks, or automatic replay of agent work. | U 2026-11-02 (adaptive control and emergency termination); A (policy safeguards) | M0 | RES3, RES4 | 0008 |
| R62 | Recovery is bounded and attributable: one owner, finite retries with increasing delay and a stop after repeated failure. A live busy subprocess is not duplicated. Agent commands are not replayed automatically; source stores and healthy sessions remain intact, and native resume is offered only when supported. | U 2026-11-02 (resilience); A (recovery contract) | M0 | RES4, SC12, SC14 | 0008, 0030 |
| R63 | Resilience is a delivery gate: synthetic fault tests precede dependent integration, each supported Windows agent is qualified with the applicable faults, and release includes a sustained pressure/recovery run with budgets frozen before testing. Record resource and responsiveness measurements; plan coverage or unrelated passing tests do not constitute runtime proof. | U 2026-11-02 (stability priority); A (qualification) | M0, V1 | RES1-RES4, P14, TL5 | 0010, 0040 |

## 3. Open questions

1. Windows in v1 (functional-design 7.1; register O3). Assumed yes.
2. The two readings "deep seek code" = DeepSeek Harness and DeepSeek Harness hosted in an ACP pane (register O9).
3. Whether the hop dialog, roundtable and palette should offer only the actions both adapters' capabilities support, showing what each choice loses (register O7).
4. The exact global key bindings (register U3, Q4).

## 4. Acceptance

The native tiling gates TL1 to TL6, the terminal rows P1 to P20 and the skin rows U1 to U8 are
the acceptance of this record; functional-design 5.16 "Acceptance" and technical-design 3.1
"Prototype verification gate" are their short form. Nothing here is claimed implemented.
RES1-RES4 additionally qualify R59-R63; the plan amendment does not close these gates.

## 5. Relation to the JSON record

FRD-0001.json holds 100 extracted sentences. This record consolidates them: its 2.1 covers
REQ-001 to REQ-013 and REQ-087 to REQ-097; 2.2 covers REQ-008 to REQ-011, REQ-025 to REQ-034
and REQ-098 to REQ-100; 2.3 and 2.4 cover REQ-049 to REQ-086; 2.5 covers REQ-035 to REQ-048;
2.6 covers REQ-014 to REQ-022. The install and release sentences (REQ-018, REQ-019) moved to
FRD-0017.

R59-R63 and the R29 emergency exception were added from the user's 2026-11-02
resilience request. The older JSON evidence snapshot has not been refreshed or
promoted and does not yet contain this amendment.
