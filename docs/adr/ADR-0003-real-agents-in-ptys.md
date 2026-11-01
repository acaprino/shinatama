---
adr_id: ADR-0003
version: 1
title: Run the real agent CLIs in Shinatama-owned PTYs, never a simulated chat
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - agent-hosting
    - pty
    - terminal-emulation
    - tui-fidelity
governs_symbols:
depends_on:
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

# ADR-0003: Run the real agent CLIs in Shinatama-owned PTYs, never a simulated chat

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The design states its point as "a *runtime*, not a wrapper that
fakes the conversation" (functional-design 1). The question this record answers is how a hosted
agent's live session appears inside Shinatama: as the agent's own process and terminal output,
or as a chat view that Shinatama draws from the agent's data.

Origin: hosting the agents "as real processes in a terminal multiplexed inside Shinatama" is
goal 1, and goals are decided by the user unless marked (the agent list on 2026-10-31). The
native tiling decision of 2026-10-29 kept it: "Agents still run in real PTYs, not simulated
chats" (functional-design 2, non-goals). Which views Shinatama draws itself is stated in
functional-design 5.1 without a marker. Emulator fidelity is unproved. The full design is in
functional-design 5.1 and 5.16 and technical-design 3.1; this record holds only the decision.

## Decision Drivers

- Fidelity to the real agent: its loop, tools, text and colours stay its own.
- The thread survives the agent and the terminal (functional-design 1).
- No wrapper that fakes or replays the conversation (functional-design 2, anti-goals).
- One hosting route for every agent that ships a terminal UI.

## Decision

Each agent runs as its real CLI in a pseudo-terminal owned by Shinatama, and the user types into
the real agent. What the agent draws inside the PTY stays the agent's: layout, text, tables, tool
calls, reasoning, prompts and any 24-bit colour it picks. The harness is the sole owner of TUI
layout, text and controls inside the PTY (technical-design 3.1); Shinatama does not redraw those
widgets or present them as native controls (functional-design 5.16). Shinatama draws only around
and over the pane: frame and bars, side panels, and native overlays (hop dialog, Daodan palette,
find) that leave the emulator screen intact and restore focus to the originating pane. Resizing
is forwarded to the agent. Shinatama draws its own rich views only where the data is already
structured and read-only: find, the hop preview, the roundtable and the overview. Redrawing the
live chat from an agent's structured stream is not planned; the one exception is DeepSeek
Harness, which ships no terminal UI (ADR-0006). See functional-design 5.1 and 5.16 and
technical-design 3.1.

## Verified Interview Examples

- **EX-001** (nominal case): Input: each agent spawned in a PTY on Windows 11 (ConPTY), macOS and Linux, with scripted keystrokes, a resize, Ctrl-C and exit -> Expected: echo correct; resize reaches the agent; exit code propagated; closing never hangs; no orphan processes (register P1).
- **EX-002** (nominal case): Input: the raw byte stream of a scripted session per agent, replayed into the candidate emulator and into a reference terminal -> Expected: the final grids have identical text and attributes, except a written list of exceptions (register P2).
- **EX-003** (boundary case): Input: a window drawn over the pane and removed while the agent keeps writing -> Expected: screen, cursor, SGR state, wide characters and alternate screen identical before and after; no output lost while the overlay is open (register P12).
- **EX-004** (boundary case): Input: the agent's truecolour output and the 16-colour remap onto each skin -> Expected: truecolour passes through untouched; every remapped colour meets 4.5:1 against its background or is exempted in writing (register P10; functional-design 5.7).
- **EX-005** (rejection or error case): Input: a corpus of known terminal escape attacks plus each agent's real stream (OSC 52 clipboard, query answerbacks, window retitling) -> Expected: a hosted agent cannot write the clipboard, read the screen, retitle the outer window or inject input without a documented decision (register P9).

## Rejected Alternatives

- **RA-001**: a wrapper that replays or simulates the conversation instead of running the agent; an anti-goal, a failure "even if it technically works" (functional-design 2).
- **RA-002**: redrawing the live chat itself from each agent's structured stream; not planned, because "Formatting of live work stays the agent's own" (functional-design 5.1). DeepSeek Harness is the stated exception.

## Enforceable Invariants

- **INV-001**: every hosted agent that ships a terminal UI runs as its real CLI process in a PTY owned by the Shinatama window process that hosts it.
- **INV-002**: Shinatama never rewrites what the agent draws inside the PTY; truecolour sequences are preserved, and the opt-in follow-skin mode remaps only the default and 16-colour ANSI palette (technical-design 3.1).
- **INV-003**: a native overlay leaves the emulator screen intact, keeps parsing the agent's output while open and restores focus to the originating pane when dismissed.
- **INV-004**: the agent sees its real pane size: rows and columns come from the content bounds, the final size of a resize is delivered, and zero dimensions are never sent for a collapsed view (technical-design 3.1).
- **INV-005**: Shinatama's own rich views (find, hop preview, roundtable, overview) are drawn only from structured, read-only data.

## Consequences

- The first milestone must prove emulator fidelity against the hosted agents on macOS, Linux and Windows: ConPTY escape changes, wide characters and emoji, alternate screen and scroll regions, mouse and advanced keys, kitty or sixel images, no flicker, and how the colour remap reads per skin (functional-design 5.1; technical-design 10 item 7).
- Key names `^H` and `^K` are placeholders until register U3 settles bindings that no agent or outer terminal consumes.
- State should come from native signals rather than the screen where the agent offers them (ADR-0026); only the owning window process writes into a PTY (ADR-0007).
- The tiling host is ADR-0004; per-harness terminal profiles are ADR-0038; the DeepSeek Harness exception is ADR-0006.
- Windows in v1 is assumed, not decided (register O3); that a Rust PTY plus emulator hosts the agents faithfully on ConPTY is unproved (functional-design 3).

## Confirmation

- Register rows P1, P2, P3, P4, P9 and P12 (G0, in the first-milestone set), P6 (G0) and P5, P7, P8, P10, P11 and P13 (G1) pass on macOS, Linux and Windows, each with a record in `docs/verification/`.
- None of these rows has been run, and no PTY or emulator prototype exists.
