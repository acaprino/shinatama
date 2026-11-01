---
adr_id: ADR-0006
version: 1
title: DeepSeek Harness runs in a native ACP pane with Shinatama as ACP client
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - deepseek-harness
    - acp-client
    - native-pane
    - agent-hosting
governs_symbols:
depends_on:
  - ADR-0003
  - ADR-0004
  - ADR-0005
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

# ADR-0006: DeepSeek Harness runs in a native ACP pane with Shinatama as ACP client

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. DeepSeek Harness ships no terminal UI: its interactive surface is
`dsh web`, a browser page on `127.0.0.1:3080` behind a browser-trust check with an internal,
undocumented API, and its other profiles are `headless`, `acp` (ACP v1 over stdio) and `sdk`
(JSON-RPC over stdio) (technical-design 5.7, research of 2026-10-31 on 0.2.1-rc.5, nothing run).
It cannot be hosted in a PTY like the other agents (ADR-0003). The question this record answers
is what its tab is.

Origin: the user decided on 2026-10-31 that "deep seek code" is supported (ADR-0005). Hosting it
in a native ACP pane is the author's call of 2026-10-31 (functional-design 5.16, "my call";
functional-design 7), not confirmed: register O9 asks the user to confirm that DeepSeek Harness
"is hosted as a native ACP pane rather than through its web UI". The full design is in
functional-design 5.16 and technical-design 5.6 and 5.7; this record holds only the decision.

## Decision Drivers

- The agent's loop, tools, models and session store must stay its own, so this is not the simulated chat of the anti-goals.
- The same tab, split, hop and overview behaviour as every other agent.
- Exact state from protocol signals, without hooks or screen reading.
- No embedded interactive browser in the first integration.

## Decision

DeepSeek Harness's tab is a native ACP pane. Shinatama runs `dsh --profile acp` and is its ACP
client, "the way an editor is": it draws messages, tool calls and permission requests with the
active skin and a harness accent colour, and answers permissions through ACP. The pane has the
same tab, split, hop and overview behaviour as a terminal tab. A new session starts with ACP
`session/new`, whose returned id lets the hop journal record the destination segment before the
first prompt; the seed is the first `session/prompt` (`protocol_prompt`), with no command line and
no PTY. Resume is ACP `session/resume`. State comes from `session/update` notifications and
`session/request_permission`. MCP arrives per launch through a `--patch` file or per session
through ACP `mcpServers`. `dsh web` is offered only as an explicit "open in browser" action in the
user's browser, for what the pane lacks (the web UI's fork and export); it is never embedded and
never a hop path. `fixture_gate` pins the exact version, since the project announces breaking
changes. See functional-design 5.16 and technical-design 5.6 and 5.7.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a scripted ACP client runs `session/new`, `session/prompt` with a realistic seed, streamed updates, cancel, `session/resume` and `session/list` -> Expected: the whole seed arrives; resume works; the id known before the first prompt equals the store's (register DS2).
- **EX-002** (rejection or error case): Input: scripted tool calls under the default `workspace-write` mode -> Expected: `waiting_permission` and turn end are mapped; no tool runs without the user's answer (register DS4).
- **EX-003** (rejection or error case): Input: a seed sent through `session/prompt` that starts with `/compact` or carries file references -> Expected: nothing is executed or expanded (register DS15).
- **EX-004** (boundary case): Input: the user asks for `dsh web`, then closes the window -> Expected: a loopback port per box with its browser-trust check, opened in the user's browser, never embedded; no process left after close (register DS14).

## Rejected Alternatives

- **RA-001**: hosting DeepSeek Harness through its web UI, or embedding `dsh web` in Shinatama; the web UI stays an explicit action in the user's browser and is "never embedded" (functional-design 5.15, 5.16) and "never a hop path" (technical-design 5.7), and interactive embedded browsing needs separate design and security gates (functional-design 5.16).

## Enforceable Invariants

- **INV-001**: the agent's loop, tools, models and session store run inside `dsh`; the pane renders ACP messages and relays the user's input and permission answers, nothing else.
- **INV-002**: no tool runs without the user's answer to `session/request_permission` (register DS4).
- **INV-003**: `dsh web` is never embedded and never a hop path; it starts only on explicit request and leaves no process after the window closes (register DS14).
- **INV-004**: the destination segment is journaled from the `session/new` id before the first `session/prompt` (technical-design 5.7, consequence 1).
- **INV-005**: a seed sent through `session/prompt` is not executed or expanded as a command or file reference (register DS15).
- **INV-006**: among hosted agents, DeepSeek Harness is the only one whose live view Shinatama draws from a structured stream (functional-design 5.1).

## Consequences

- A native conversation pane kind joins the tiling host (ADR-0004), drawn with the skin rather than a terminal profile (functional-design 5.7; ADR-0038); ADR-0002 counts the companion chat as the second such exception to functional-design 5.16.
- The pane lacks fork and export, which only the web UI has; DeepSeek Harness has no import of any kind.
- There is no login or status command: `auth_status()` is `Unknown` until DS9 finds a check that reads no secret, and `login()` opens `dsh web` on the box's home (ADR-0030).
- Stdio MCP servers get a scrubbed environment (names matching `KEY|PASSWORD|SECRET|TOKEN` and every `DSH_*` are dropped), so `SHINATAMA_INSTANCE_TOKEN` must reach `shinatama mcp` another way (register DS5; ADR-0012).
- Search and hops from DeepSeek Harness wait for its Mukade reader (register DS3; ADR-0005).
- Open: O9; DS1 (starts on Windows, macOS and Linux with Node `^22.19.0 || >=24`).

## Confirmation

- Register DS1, DS2 and DS4 (G0; DS1 and DS2 in the first-milestone set) pass with a scripted ACP client, then DS5, DS13, DS14 and DS15 (technical-design 10 item 16).
- Nothing has run: DeepSeek Harness 0.2.1-rc.5 is not installed on this machine (register 7b).
