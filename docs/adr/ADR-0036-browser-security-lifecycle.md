---
adr_id: ADR-0036
version: 1
title: Moli `serve` gated off, window-owned process tree, one controller, untrusted content, evidence before performance claims
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/browser-integration.md
    - docs/technical-design.md
    - docs/functional-design.md
    - docs/verification-register.md
  modules:
  domains:
    - browser
    - process-lifecycle
    - browser-security
    - browser-privacy
    - performance-evidence
governs_symbols:
depends_on:
  - ADR-0008
  - ADR-0024
  - ADR-0035
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

# ADR-0036: Moli `serve` gated off, window-owned process tree, one controller, untrusted content, evidence before performance claims

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet; no browser binary has been installed or run, and no endpoint,
cleanup or performance claim has been verified locally. Source reading of Moli at commit
`dcae77e5aa27ef645b819773f8420750bc06e291` found that `moli serve` binds a TCP HTTP/WebSocket
server (default `127.0.0.1:9222`) exposing CDP, WebDriver Classic and BiDi, with no
request-credential check on the routes inspected: "a scoped source finding, not a completed
security audit". Sources, kept apart:

- Decided by the user, 2026-10-29: no resident Chrome or Patchright companion and nothing
  installed automatically (browser-integration 1, requirement 5).
- Proposed shape, not approved: the `serve` gate, process ownership, scope and privacy rules,
  control and performance protocol of browser-integration 6.2 to 10. functional-design 5.15
  also says no speed or memory saving is claimed before B10; that section marks its
  requirements decided and its shape proposed, without saying which this sentence is.

The full design is in `docs/browser-integration.md` and technical-design 7.9; this record holds
only the decision. Engine choice and routing are ADR-0035.

## Decision Drivers

- No unauthenticated control endpoint reachable by pages or local processes.
- Nothing browser-related runs or downloads until the user asks; nothing survives its window.
- Cleanup that never kills an unrelated process and never claims `closed` falsely.
- Browser content is untrusted and may hold secrets.
- Claims of speed or memory only from reproducible measurement.

## Decision

`moli serve`, persistent CDP/WebDriver automation and interactive Moli previews stay disabled
until register B4 proves a private authenticated transport or an equivalent verified boundary
that untrusted pages cannot reach; the same gate applies to Patchright transports. No Moli,
Patchright, companion or Chrome process starts before a request, there is no cross-window
browser daemon, the companion is lazy-loaded, and a missing component returns `unsupported`
instead of being downloaded. The Shinatama process of the current window owns each request and
its whole child tree, recording OS process, group or job identity, not just a PID. A session
moves `requested -> authorized -> starting -> active -> closing -> closed` with `denied` and
`failed` branches; every effect is idempotent by `request_id`; `closed` is not reported while a
managed process or endpoint is alive. Content (HTML, scripts, screenshots, downloads,
redirects, MCP UI, agent output) is untrusted under the rules of technical-design 7.6. Local
roots, services, redirects, subresources and proxies are explicit scope; profiles are temporary
by default; no cookie, token or credential reaches logs, SQLite, transcripts or summaries, and
browser content reaches no title or summary provider by default. An interactive session has
one owner and one controller; handover is explicit; closing invalidates queued actions. No performance figure is accepted before a frozen whole-process-tree baseline
(B10). See browser-integration 6.2 to 11.

## Verified Interview Examples

- **EX-001** (rejection or error case): Input: hostile local pages and socket tests against `moli serve` and the Patchright transports -> Expected: no unauthenticated or reachable control path; otherwise `serve` or the failing UI transport stays disabled (register B4).
- **EX-002** (nominal case): Input: process inventory before and after startup and commands, with the optional component absent -> Expected: no browser or companion before a request; explicit UI works or returns a clear `unsupported`; nothing auto-installs (register B5).
- **EX-003** (rejection or error case): Input: normal close, cancellation, startup failure, owner crash and PID reuse on Windows, macOS and Linux -> Expected: no managed descendant or listener remains; the cleanup state is truthful; unrelated processes survive (register B7).
- **EX-004** (rejection or error case): Input: canary secrets and out-of-scope targets through symlinks, subresources, redirects, local services and proxies -> Expected: no out-of-scope read, personal-profile use, cross-engine state or secret leakage (register B8).
- **EX-005** (boundary case): Input: a second UI session, a user takeover, an agent resume and a stale queued action after close -> Expected: one controller; no focus or profile theft and no stale or duplicate effect (register B9).

## Rejected Alternatives

- **RA-001**: enabling `moli serve` in the first integration behind loopback, an ephemeral port or a random path; these are not by themselves authentication (browser-integration 6.2).
- **RA-002**: a shared browser daemon or a resident Chrome or Patchright companion; the graphical path is loaded only on explicit request (user requirement 5; technical-design 7.9).
- **RA-003**: a PID as the process identity for cleanup; PID reuse could make cleanup kill an unrelated process (browser-integration 7).
- **RA-004**: Moli's upstream performance figures, or any arbitrary saving percentage or latency target, as acceptance evidence before a local baseline exists (browser-integration 6.1, 10).

## Enforceable Invariants

- **INV-001**: `moli serve` and any persistent automation transport stay off until B4 passes for that transport.
- **INV-002**: no Moli, Patchright, companion or Chrome process exists before a request; enabling browser support does not start the UI path; `/browse` never triggers a download.
- **INV-003**: `closed` is never reported while a managed browser, companion or endpoint is alive, and cleanup never targets a process outside the recorded tree.
- **INV-004**: no cookie, token, authorization header or credential appears in logs, SQLite, transcripts, summaries or error reports; browser content is not sent to title or summary providers by default (technical-design I5).
- **INV-005**: untrusted content cannot authorize navigation outside scope, start Chrome, change controller or request credentials (technical-design I6, 7.6).
- **INV-006**: an interactive session has exactly one controller; closing or replacing it invalidates outstanding actions, approvals and queued results.
- **INV-007**: restoring a saved layout never reopens an interactive browser session (functional-design 5.16; register TL3).

## Consequences

- A failed `serve` gate limits persistent Moli automation; it does not invalidate one-shot `fetch` (browser-integration 11).
- The integration claims policy enforcement only for its managed path; OS isolation against an agent with an arbitrary shell is not claimed (browser-integration 8).
- Moli's client-side network blocking skips proxy-resolved targets, so a proxy needs its own equivalent enforcement (browser-integration 8).
- There is no exactly-once claim for external web effects (browser-integration 9).
- Builder choices: Node.js or Python for the companion, the private transport, OS process cleanup, artifact viewer and retention defaults. Open with the user: the performance budget after B10 (register O8).
- Interactive embedded browsing and mini-apps need their own gates (register TL6).

## Confirmation

- Register rows B4, B5, B7 and B8 (G0) and B9, B10 (G1) must pass, with technical-design 11.11. B4 and B8 are `SEC` rows, reviewed by a second reader. None has been run.
