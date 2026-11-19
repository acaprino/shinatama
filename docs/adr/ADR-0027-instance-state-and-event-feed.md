---
adr_id: ADR-0027
version: 1
title: Instance state from native signals in trust order, one event feed; the widget is a separate client
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - instance-state
    - event-feed
    - hook-command
    - desktop-widget
governs_symbols:
depends_on:
  - ADR-0008
  - ADR-0009
  - ADR-0011
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

# ADR-0027: Instance state from native signals in trust order, one event feed; the widget is a separate client

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user decided that a desktop widget with Shinatama as an assistant
is in v1 and is built last within v1 (functional-design goal 9 and 5.8). Its shape is the
author's call: the widget reads the `event` feed in the shared SQLite database as just another
client, with an optional local socket or named pipe used only as a wake-up ping, and quick
answers appear only for prompts recognized with certainty. The widget rules of register W2, W3,
W5 and W10 are written into functional-design 5.8 as the author's calls. What "assistant" means
is open (functional-design 7 item 2; register O3). The instance states, the hook command and the event
feed of technical-design 8.1 to 8.3 are untagged author's design. The question is where
Shinatama learns what each instance is doing, how far it can trust that, and how the overview,
the widget and other views hear about it without a daemon. The full design is in
functional-design 5.8 and 5.13 and technical-design 4.6 and 8.1 to 8.3; this record holds only
the decision.

## Decision Drivers

- A wrong `waiting_permission` that lets the widget send keystrokes could answer a prompt the user never saw.
- Screen parsing is fragile; the agents' native hooks and events are preferred.
- No resident service: processes coordinate through SQLite (ADR-0008).
- A hook must never slow down or break the agent it runs in.
- The widget comes last and must not need its own channel.

## Decision

An instance is in one of seven states: `starting`, `idle`, `working`, `waiting_permission`,
`waiting_question`, `error`, `exited`. Each state records its `state_source`, taken in order of
trust: the agent's native hook or event stream, then its structured transcript tail, then a PTY
heuristic (8 s of output silence after activity means probably `idle`), which is always labelled
`unsure`. `waiting_permission` is never inferred from screen text alone in a way that lets the
widget send keystrokes; the widget's allow and deny buttons are offered only for states reported
by a native event.

`shinatama hook <agent> <event>` reads the agent's JSON from stdin, writes an `event` row and the
`instance` update in one transaction, prints what the agent expects on stdout, exits in under
100 ms and never fails the agent: errors are logged and swallowed, and it exits 0 unless the
agent's contract says otherwise. All changes go into one feed, `event(seq, ts, instance_id, kind,
payload)`, with kinds `state`, `hop`, `message`, `summary`, `alert`, `file_change` and
`file_collision`. The
overview, the widget and the roundtable view are consumers that remember their last `seq`; a
process wakes by polling `PRAGMA data_version` every 200 ms, and a socket ping is an optional
later optimization. The widget is a separate process and needs no other channel; clicking an
alert focuses the right session, and an alert in the widget is an event in the overview. See
technical-design 4.6 and 8.1 to 8.3.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a scripted session on every agent that offers a native signal -> Expected: `waiting_permission`, `idle` and `working` reported correctly (technical-design 11.9); precision and recall recorded (register I1).
- **EX-002** (boundary case): Input: the same script on an agent with no native signal -> Expected: the state carries the `unsure` label (technical-design 11.9).
- **EX-003** (rejection or error case): Input: the hook command meets Claude's exit-code semantics, a timeout, a locked database or a crash -> Expected: the agent continues in all cases (register M11).
- **EX-004** (rejection or error case): Input: the prompt changes between the native event and the user's click on allow -> Expected: the quick answer is refused; allow and deny go only to the right instance and prompt, and the bubble shows the real command from the native event (register W5).
- **EX-005** (boundary case): Input: the user types continuously while alerts fire -> Expected: no keystroke is lost or redirected; the widget never takes keyboard focus (register W3).

## Rejected Alternatives

- **RA-001**: detecting "needs input" by parsing the screen; the docs prefer the agent's native hooks or events, and quick answers need certainty "because they send keystrokes to the agent" (functional-design 5.8).
- **RA-002**: a channel of its own for the widget; the widget is just another client of the core's events, so it can be built after the core (functional-design 5.8; technical-design 8.3).

## Enforceable Invariants

- **INV-001**: every instance state has a recorded `state_source`; a state from the PTY heuristic is always labelled `unsure`.
- **INV-002**: widget quick answers exist only for states reported by a native event.
- **INV-003**: `shinatama hook` writes the event and the instance update in one transaction, exits in under 100 ms and never makes the agent fail.
- **INV-004**: `shinatama hook` refuses a caller whose instance token does not match (technical-design 7.5; register M15).
- **INV-005**: consumers read the `event` feed in `seq` order; the widget uses no other channel.
- **INV-006**: the widget never takes keyboard focus and never steals keystrokes when an alert appears (author's call; register W3).
- **INV-007**: a quick answer goes only to the instance and prompt it was offered for, the bubble shows the real command from the native event, and a prompt that changed between the event and the click is refused (author's call; register W5).
- **INV-008**: the widget honours the reduced-motion setting and has a mute and a disable switch (author's call; register W10).
- **INV-009**: where always-on-top or free placement fails on Wayland, the widget falls back to a notification-only mode (author's call; register W2).

## Consequences

- The per-agent signal candidates (Claude hooks through `--settings`, Codex PTY notifications plus `notify` and app-server status, OpenCode SSE, a Pi extension, Hermes shell hooks in the box's `config.yaml`, DeepSeek Harness ACP notifications) each need a fixture; a Hermes on the `system` home has PTY and gateway only (technical-design 5.7 item 3; register HM4).
- Backend-attach mode, decided before the adapters are written, can change how state is obtained (technical-design 10 item 11).
- `event` is an operational table; its pruning policy is open (register I8).
- The companion pane of ADR-0002 also reads this feed; the overview (ADR-0028) and the roundtable view (ADR-0026) are consumers.
- Widget specifics stay open: the toolkit (register W1), Wayland placement per compositor (W2; the notification-only fallback is INV-009), the "assistant" scope (O3) and the Oni character (O4, W9).

## Confirmation

- Register I1, M11, M15, W2, W3, W4, W5 and W10 pass; all are open.
- Acceptance test 11.9 passes for every agent.
- Nothing has been run yet.
