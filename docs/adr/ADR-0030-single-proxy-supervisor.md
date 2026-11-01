---
adr_id: ADR-0030
version: 1
title: Shinatama alone supervises each proxy; busy is not dead; diagnostics never on agent terminals
status: Proposed
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
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0030: Shinatama alone supervises each proxy; busy is not dead; diagnostics never on agent terminals

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
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
heading carries the user's decision "same resilience, security, compression". The user confirmed
on 2026-11-01 that this supervision rule (one supervisor per proxy, busy is not dead, no
diagnostics on agent terminals) is the user's decision, and technical-design 9 marks it so. The
full design is in functional-design 5.6 and technical-design 9; this record holds only the
decision.

## Decision Drivers

- A busy proxy is not a dead one: a long compression can hold its event loop for a minute.
- A replacement cannot bind a port that the busy proxy still holds.
- Nothing may print over an agent's TUI.
- One recovery per proxy, not one per session.

## Decision

The Shinatama process that owns a proxy is its only supervisor: it alone starts and restarts it.
Liveness is two facts checked by that process: the proxy's process is alive, and its port accepts
a TCP connection. A missed health check while both are true marks the proxy **busy**: the window
shows it and nothing is started. Recovery on the original address starts only when the process
has exited or the port refuses connections; it starts one replacement, and every waiting session
attaches to it.

A session's process never probes, restarts or reports on the proxy. When the shipped per-session
launcher is used, its supervision is switched off and its output is kept off the agent's
terminal; which launch path is used is decided with register SC5. Sidecar diagnostics go to
Shinatama's log, and the window shows the busy and recovering states. The proxy's own owner
watchdog must not take a long stall of its event loop for the loss of every owner. See
functional-design 5.6 and technical-design 9.

## Verified Interview Examples

- **EX-001** (boundary case): Input: five instances attached; the proxy's event loop held for 60 s (a debugger pause or a synthetic long compression) while its port still accepts connections -> Expected: no replacement process starts, no line appears in any agent's terminal, the window shows the proxy busy and then normal, and the agents' next turns go through (register SC14).
- **EX-002** (rejection or error case): Input: the proxy killed mid-turn with five instances attached -> Expected: exactly one replacement starts, every agent recovers, no final flush is lost and nothing is printed in any agent's terminal (register SC6).
- **EX-003** (boundary case): Input: the Windows proxy watcher (2 s) with Shinatama as the long-lived owner, after the proxy's own event loop was blocked for 60 s -> Expected: no premature or late shutdown (register SC12).
- **EX-004** (rejection or error case): Input: five instances attached and running; the owner-renewal request to the proxy times out after 2 s and is then refused, an attach-gate refusal is raised and a stale-proxy rotation is attempted -> Expected: each event is recorded once in Shinatama's log, and every agent's terminal carries only bytes the agent itself wrote (register SC15).

## Rejected Alternatives

- **RA-001**: per-session supervision of a shared proxy, as the shipped launcher does it; observed on 2026-10-31 to count a busy proxy dead, start replacements that lose the port and print every attempt on the agents' terminals (170 replacements in one day).
- **RA-002**: treating missed health checks alone as death; a long compression can hold the event loop for a minute while the port still accepts connections, and a replacement could not bind that port.
- **RA-003**: guarding the terminal by moving the launcher's messages to a log one path at a time; rejected because the 2026-11-01 message came from a path that such a fix had missed, and the sidecar code stays unmodified (ADR-0029), so Shinatama cannot audit every message of a later upstream version.

## Enforceable Invariants

- **INV-001**: only the owning Shinatama process starts or restarts a proxy.
- **INV-002**: no replacement starts while the proxy's process is alive and its port accepts connections.
- **INV-003**: a recovery starts exactly one replacement, on the original address, and waiting sessions attach to it.
- **INV-004**: no sidecar diagnostic is written to an agent's terminal.
- **INV-005**: a session's process never probes, restarts or reports on the proxy.
- **INV-006**: the bytes on an agent's terminal come only from the agent. This is verified by capturing the terminal while faults are injected (register SC15), not by auditing individual messages, and it holds for every launch path chosen under SC5.

## Consequences

- The launch path is open: the shipped per-session launcher with its supervision off, or a direct launch with the proxy environment, decided together with one proxy per instance versus one shared (register SC5, G0). The sidecar code stays unmodified (ADR-0029).
- The window gains a proxy status with busy and recovering states; Shinatama's log receives the sidecar's diagnostics.
- The proxy's own owner watchdog must not read a long stall as the loss of every owner; register SC12 tests this for the Windows watcher.
- The julia1 sidecar of the token-savings spec is to be "launched and supervised like the BillionContext sidecar (5.6)" (token-savings 7).

## Confirmation

- Register SC5, SC6, SC12, SC14 and SC15 pass; all are open (SC14 and SC15 were added with this rule).
- Nothing has been run under Shinatama; the only evidence is the 2026-10-31 observation of the shipped launcher.
