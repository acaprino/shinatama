---
adr_id: ADR-0008
version: 1
title: Local-first; one owning process per window; no resident service; coordination through SQLite
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - process-model
    - local-first
    - liveness
    - inter-process-coordination
governs_symbols:
depends_on:
  - ADR-0009
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

# ADR-0008: Local-first; one owning process per window; no resident service; coordination through SQLite

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. Several windows, the desktop widget, the command line mode and each
agent's MCP server and hooks are separate processes that must share state and reach PTYs
(technical-design 3). The question this record answers is which process owns what, whether a
background service coordinates them, and how they learn about each other's changes and crashes.

Origin: "Local-first: no account, no hosted service. Threads and settings live on disk" is a
constraint in functional-design 3 with no decided or my-call marker; "A second scheduler or a
resident service that keeps executing work after the owning application window closes" is an
anti-goal (functional-design 2), also unmarked. The process model of technical-design 3 and the
liveness rule of 4.4 are untagged; the wake mechanism of 4.6 is the author's call [C], and the
optional wake socket is left to the builder (functional-design 8). None of it is confirmed by a
dated user decision. The full design is in technical-design 3, 4.4, 4.6 and 7.5; this record
holds only the decision.

## Decision Drivers

- One writer per PTY.
- No work keeps running after the owning window closes.
- No account and no hosted service.
- Crash recovery from shared state rather than from a supervisor process.

## Decision

Shinatama runs as one owning runtime process per native application window. Each window process
owns the PTYs of the instances it hosts, and only that process writes into them; browser
operations are owned the same way (technical-design 7.9). There is no resident service and no
daemon: windows, the widget, the command line, `shinatama mcp` and `shinatama hook` are separate
processes that coordinate through one SQLite file per user (ADR-0009) and open it directly. Any
process may run an operation that does not touch a foreign PTY; operations that need one are
queued in the database and executed by the owning process. Every owning process updates
`instance.last_seen` every 5 s; an instance is dead when `last_seen` is older than 20 s and its
`owner_pid` with `owner_start_time` is gone; a reaper, run by any process at start and every
30 s, marks dead instances ended and closes their activations with `end_reason = crashed`. A
process that wants to hear about changes polls `PRAGMA data_version` every 200 ms and reads the
`event` table after its last `seq`; a one-byte "look now" ping over a named pipe or Unix socket is
an optional later optimization. Background processes that an agent would leave running (OpenCode's
service, Hermes Agent's gateway) are started and stopped by Shinatama per box. See
technical-design 3, 4.4, 4.6 and 7.5.

## Verified Interview Examples

- **EX-001** (rejection or error case): Input: kill -9 of Shinatama, and window-close tests -> Expected: closing an instance kills its whole tree; no agent or MCP server process survives after 10 s, or the survivors are documented (register P14).
- **EX-002** (boundary case): Input: Hermes Agent started, detached, closed and killed -> Expected: nothing of Hermes runs after Shinatama's window closes; no gateway is shared across homes (register HM2).
- **EX-003** (boundary case): Input: kill, suspend and resume tests, laptop sleep included -> Expected: no false "dead" after sleep; a dead instance is found within the stated time (register S8).
- **EX-004** (boundary case): Input: the owning process dies with messages queued for its instances -> Expected: the queued messages are shown and delivered when an owner returns (register M14).
- **EX-005** (nominal case): Input: two processes, one committing and one polling `PRAGMA data_version` at 200 ms -> Expected: latency and CPU recorded; the interval chosen from the data (register S3).

## Rejected Alternatives

- **RA-001**: a resident service, or a second scheduler, that keeps executing work after the owning window closes; an anti-goal, a failure "even if it technically works" (functional-design 2). The same reason makes Shinatama stop Hermes Agent's gateway with the box's last instance (technical-design 5.7).
- **RA-002**: an account or a hosted service; excluded by the local-first constraint (functional-design 3). No reason is recorded.

## Enforceable Invariants

- **INV-001**: only the owning window process writes into a PTY; a request from another process reaches that PTY through a queue in the database.
- **INV-002**: no Shinatama process, and no agent background process that Shinatama started, keeps executing work after the owning window closes.
- **INV-003**: an instance is declared dead only when `last_seen` is older than 20 s and its owner's pid together with its start time is gone; pid alone is never enough.
- **INV-004**: `shinatama mcp`, `shinatama hook` and the command line open the database directly; no process acts as a broker between them.
- **INV-005**: the overview, the widget and the roundtable view read the `event` feed by `seq` and need no other channel (technical-design 8.3).
- **INV-006**: every hop step is journaled before its effect and idempotent after it, so recovery reads the database (technical-design I3).

## Consequences

- The shared database is the single coordination point; its location, schema and rules are ADR-0009. The hop journal and its recovery are ADR-0018; message queues are ADR-0024.
- A workspace has one owning window, and live PTY transfer between windows is outside the tiling revision (ADR-0005).
- The sidecar proxy is supervised by the owning Shinatama process alone (ADR-0030); one proxy per instance or one shared is open (register SC5).
- S3 chooses the 200 ms polling interval from measurement; S8 tests the 5 s and 20 s liveness thresholds against sleep and resume on each system.

## Confirmation

- Register P14 (G0, first-milestone set), S3, S4, S8, M14 and HM2 pass with records; acceptance 11.1 (a crash after each hop state) and 11.4 (two processes hop one thread) pass.
- Nothing has been run.
