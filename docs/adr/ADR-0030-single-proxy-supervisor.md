---
adr_id: ADR-0030
version: 1
title: Shinatama alone supervises each proxy; busy is not dead; diagnostics never on agent terminals
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - billioncontext-sidecar
    - proxy-supervision
    - proxy-liveness
    - diagnostics
governs_symbols:
depends_on:
  - ADR-0008
  - ADR-0029
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

# ADR-0030: Shinatama alone supervises each proxy; busy is not dead; diagnostics never on agent terminals

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. On 2026-10-31, with BillionContext 0.1.207, several sessions shared
one proxy and each session's launcher watched it itself: it counted the proxy dead after two
missed health checks of 1.5 s, started a replacement that could only lose the port to the busy
proxy, and printed every attempt on its agent's terminal. That day 170 replacements started and
the agents' screens filled with the messages (functional-design 5.6). On 2026-11-01 a launcher
build whose recovery messages had been moved to a log still printed `bili: watcher registration
failed — the shared proxy may exit when its first owner does (The operation was aborted due to
timeout)` over Claude Code's screen: the owner-renewal path had been missed. Moving messages one
path at a time does not close the class. Three supervision items
were then added to the sidecar acceptance list of functional-design 5.6 (commit `9eabbaa`), whose
heading carries the user's decision "same resilience, security, compression". Technical-design 9
tags the proxy supervision [C], so the exact rule is the author's call, not confirmed by the
user. The full design is in functional-design 5.6 and technical-design 9; this record holds only
the decision.

Later on 2026-11-01 two more paths turned a busy shared proxy into `ECONNREFUSED` for Claude Code
(observed in Musashi Terminal, recorded in its docs/sidecar.md). First, the proxy shut itself down
while its owners were alive: after holding its event loop for about 36 s, its owner watchdog found
all six heartbeat owners past their 15 s grace on its first tick, logged `watchers-gone (...)
[stale-heartbeat pids: ...]` and force-exited with 324 connections open; this happened four times
that day. Second, on Windows a listener whose event loop is held refuses new connections once 232
wait to be accepted (Node 24.14.1, measured twice, unchanged with a backlog of 2048), while a bind
attempt on its port still fails with `EADDRINUSE`. A connection check therefore read the busy proxy
as gone, the monitors' own probe connections filled the queue, and every monitor started
replacements that died on `EADDRINUSE`. The owned source fixes both from Musashi Terminal commit
`ac7d57a`; Shinatama inherits them through the single source (ADR-0029).

## Decision Drivers

- A busy proxy is not a dead one: a long compression can hold its event loop for a minute.
- A replacement cannot bind a port that the busy proxy still holds.
- On Windows a busy proxy refuses new connections once its accept queue is full, so a
  connection cannot tell busy from gone; a bind attempt can, and takes no slot in that queue.
- Every probe connection to a busy proxy takes a slot that the agents' turns need.
- Nothing may print over an agent's TUI.
- One recovery per proxy, not one per session.

## Decision

The Shinatama process that owns a proxy is its only supervisor: it alone starts and restarts it.
Liveness is two facts checked by that process: the proxy's process is alive, and its port is
held, which is decided by binding the port (`EADDRINUSE` means held), not by connecting to it. A
missed health check while both are true marks the proxy **busy**: the window shows it and nothing
is started, and the busy proxy is probed at a growing interval. Recovery on the original address
starts only when the process has exited or the port is free; it starts one replacement, and every
waiting session attaches to it.

A session's process never probes, restarts or reports on the proxy. When the shipped per-session
launcher is used, its supervision is switched off and its output is kept off the agent's
terminal; which launch path is used is decided with register SC5. Sidecar diagnostics go to
Shinatama's log, and the window shows the busy and recovering states. The proxy's own owner
watchdog must not take a long stall of its event loop for the loss of every owner: the time it
could not read heartbeats is not its owners' silence. See functional-design 5.6 and
technical-design 9.

## Verified Interview Examples

- **EX-001** (boundary case): Input: five instances attached; the proxy's event loop held for 60 s (a debugger pause or a synthetic long compression) while it holds its port, including after its accept queue fills and new connections are refused -> Expected: no replacement process starts, no line appears in any agent's terminal, the window shows the proxy busy and then normal, and the agents' next turns go through (register SC14).
- **EX-002** (rejection or error case): Input: the proxy killed mid-turn with five instances attached -> Expected: exactly one replacement starts, every agent recovers, no final flush is lost and nothing is printed in any agent's terminal (register SC6).
- **EX-003** (boundary case): Input: the Windows proxy watcher (2 s) with Shinatama as the long-lived owner, after the proxy's own event loop was blocked for 60 s -> Expected: no premature or late shutdown (register SC12).
- **EX-004** (rejection or error case): Input: five instances attached and running; the owner-renewal request to the proxy times out after 2 s and is then refused, an attach-gate refusal is raised and a stale-proxy rotation is attempted -> Expected: each event is recorded once in Shinatama's log, and every agent's terminal carries only bytes the agent itself wrote (register SC15).
- **EX-005** (boundary case): Input: six live owners heartbeating every 5 s; the proxy's event loop held for 36 s -> Expected: the proxy's owner watchdog keeps all six owners and does not shut the proxy down; no agent receives `ECONNREFUSED` from a shutdown (register SC12; observed to fail with the shipped 0.1.207 watchdog on 2026-11-01).

## Rejected Alternatives

- **RA-001**: per-session supervision of a shared proxy, as the shipped launcher does it; observed on 2026-10-31 to count a busy proxy dead, start replacements that lose the port and print every attempt on the agents' terminals (170 replacements in one day).
- **RA-002**: treating missed health checks alone as death; a long compression can hold the event loop for a minute while the port still accepts connections, and a replacement could not bind that port.
- **RA-003**: guarding the terminal by moving the launcher's messages to a log one path at a time; rejected because the 2026-11-01 message came from a path that such a fix had missed, and the sidecar code stays unmodified (ADR-0029), so Shinatama cannot audit every message of a later upstream version.
- **RA-004**: deciding liveness by a TCP connection to the port; on Windows a busy proxy refuses connections once 232 wait to be accepted, so the check read it as gone, and its probe connections filled the queue further (observed 2026-11-01).

## Enforceable Invariants

- **INV-001**: only the owning Shinatama process starts or restarts a proxy.
- **INV-002**: no replacement starts while the proxy's process is alive and its port is held (a bind attempt fails with `EADDRINUSE`).
- **INV-003**: a recovery starts exactly one replacement, on the original address, and waiting sessions attach to it.
- **INV-004**: no sidecar diagnostic is written to an agent's terminal.
- **INV-005**: a session's process never probes, restarts or reports on the proxy.
- **INV-006**: the bytes on an agent's terminal come only from the agent. This is verified by capturing the terminal while faults are injected (register SC15), not by auditing individual messages, and it holds for every launch path chosen under SC5.
- **INV-007**: the supervisor decides whether the port is held without opening a connection to it, and probes a busy proxy at a growing interval.
- **INV-008**: the proxy's owner watchdog does not count the time its own event loop was held as owner silence.

## Consequences

- The launch path is open: the shipped per-session launcher with its supervision off, or a direct launch with the proxy environment, decided together with one proxy per instance versus one shared (register SC5, G0). The sidecar code stays unmodified (ADR-0029).
- The window gains a proxy status with busy and recovering states; Shinatama's log receives the sidecar's diagnostics.
- The proxy's own owner watchdog must not read a long stall as the loss of every owner; the owned source does this from Musashi Terminal commit `ac7d57a`, and register SC12 tests it for the Windows watcher under Shinatama.
- A held event loop is itself an availability limit on Windows: once the agents' own connections fill the accept queue, their turns are refused until the loop resumes. On 2026-11-01 the shared proxy held its loop for 15 to 71 s every few minutes with six Claude Code sessions; how long it may be held is open (register SC16) and bears on one proxy per instance versus one shared (SC5).
- The julia1 sidecar of the token-savings spec is to be "launched and supervised like the BillionContext sidecar (5.6)" (token-savings 7).

## Confirmation

- Register SC5, SC6, SC12, SC14, SC15 and SC16 pass; all are open (SC14 and SC15 were added with this rule, SC16 on 2026-11-01).
- Nothing has been run under Shinatama; the evidence is the shipped launcher observed on 2026-10-31 and 2026-11-01, and Musashi Terminal's unit tests of the fixes (`test/watchdog-stall.test.mjs`, `test/proxy-recovery.test.mjs`).
