---
adr_id: ADR-0040
version: 1
title: Terminal-first baseline, then read-only browsing and resume, then the seed hop; first-milestone gates decide the design
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - docs/verification-register.md
  modules:
  domains:
    - delivery-plan
    - milestones
    - verification-gates
    - release
governs_symbols:
depends_on:
  - ADR-0005
  - ADR-0010
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

# ADR-0040: Terminal-first baseline, then read-only browsing and resume, then the seed hop; first-milestone gates decide the design

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet, and nothing in the technical design has been run as a prototype.
The v1 goals are many (hop, Daodan support, browser, roundtable, widget, overview); the question
is what is built first and what must be proved before the rest is built on it. Sources, kept
apart:

- Decided by the user: native Rust tiling (2026-10-29, functional-design 5.16); v1 hops
  seed-only (2026-10-31); the project `shinatama`, named 2026-10-26, in this repository
  (functional-design 7 item 3); the former launcher out of scope (2026-10-28, register O6); the
  terminal-first scope (2026-10-30, functional-design 5.16) and the delivery baseline
  (2026-10-31, functional-design 5.12), both confirmed as the user's on 2026-11-01.
- Author's call, "to check": the order of work and the audience (functional-design 5.12).
- Proposed shape: the first-milestone list of technical-design 10 and the gate rules of the
  register, section 0.

The full plan is in functional-design 5.12 and 5.16, technical-design 10 and the verification
register; this record holds only the decision.

## Decision Drivers

- Foundations first: PTY hosting, emulator fidelity and shared state decide everything above.
- A usable slice early, without waiting for every v1 surface.
- No surface delivered before its own gates pass.
- Build as if other people will install it.

## Decision

The first UI milestone is terminal-first (distinct from register row TL1, the toolkit prototype
gate of technical-design 3.1): project selector, resizable terminal tiles, leaf tab groups and per-harness terminal profiles, with no git integration, file navigator,
diffs, thread side panes, session browsing, hop or Daodan surfaces. The first usable release
slice adds read-only store browsing and search and same-agent native resume. The next slice is
the live seed hop. Native import is post-v1. Daodan task handover, browser UI, roundtable and
widget follow their own gates and do not block the first usable slice. No downstream surface
is treated as deliverable in the same baseline before its dependencies and G0/G1 gates are
closed. The suggested order is: skin, PTY host and threads; checkup; sidecar; project manager
and launch; search and same-agent resume; seed hop with preview; roundtable; widget; browser
fast path, then the UI path; the overview grows alongside. The first milestone must prove the
foundations "or the design changes" (technical-design 10); the register is the complete list
and wins where they differ, and a failed `G0` row changes the design, with nothing built on it
before it passes. Release builds, signing and notarization are part of the plan; the code lives
in this repository. See functional-design 5.12 and 5.16; technical-design 10; register 0.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the chosen native toolkit prototyped with real agent PTYs, nested splits and Markdown and file panels on macOS, Linux and assumed Windows -> Expected: no web frontend or required browser startup; versions and platform gaps recorded before selection (register TL1).
- **EX-002** (rejection or error case): Input: a `G0` row fails -> Expected: the design changes; nothing is built on it before it passes (register 0).
- **EX-003** (boundary case): Input: Daodan task handover, browser UI, roundtable or widget have not passed their gates -> Expected: they do not block the first usable slice (functional-design 5.12).
- **EX-004** (boundary case): Input: acceptance tests 1 to 11 fail on one operating system -> Expected: the failing system is named in the release notes (technical-design 11.12).

## Rejected Alternatives

- **RA-001**: letting Daodan task handover, the browser UI, the roundtable and the widget block the first usable slice; each "follow[s] their own gates rather than blocking the first usable slice" (functional-design 5.12).
- **RA-002**: dropping the other v1 goals to fit the slices; "These slices do not drop the other v1 goals" (functional-design 5.12).

## Enforceable Invariants

- **INV-001**: the terminal-first milestone contains no git integration, file navigator, diffs, thread side panes, session browsing, hop or Daodan surfaces.
- **INV-002**: no surface is delivered in a baseline before its dependencies and its G0/G1 gates are closed.
- **INV-003**: nothing is built on a `G0` item before it passes; where technical-design 10 and the register differ, the register wins.
- **INV-004**: v1 never writes a native session through Shinatama or Mukade; native import is post-v1 (technical-design I7).

## Consequences

- The register's first-milestone set is 30 rows (register 0) and technical-design 10 is called its short form, yet technical-design 10 also lists browser B1 to B10 and the tool-definition baseline (register T1), which are not in that set. The register says it wins; technical-design 10 should be aligned.
- Register row TL1 is the toolkit prototype gate of technical-design 3.1, not the terminal-first UI milestone: it prototypes Markdown and file panels to choose the toolkit, while the milestone ships no file navigator (functional-design 5.12, 5.16).
- The other `G0` rows gate their own feature and run when it is built (register 0).
- Removing the launcher left the first-milestone set unchanged (register O6).
- Open: Windows in v1 is assumed (register O3); the Oni-derived art is a distribution decision separate from the code (register O4); overall approval of both designs (register O5).
- The widget is built last within v1, after the core it listens to (functional-design goal 9).
- The browser fast path in the order above is gated, not guaranteed: if Moli fails the priority cases or B10, its fast path waits while Moli stays the default, and meanwhile only the explicit Patchright UI or no browser is offered (user, 2026-11-01; ADR-0035).

## Confirmation

- The 30 first-milestone rows of the register must pass, TL1 among them; TL1 to TL6 have not been run. Of the first-milestone rows, CX4 passed (2026-10-27) and PI11 is running (partial).
- technical-design 11 acceptance tests 1 to 12 define "done"; none has been run.
