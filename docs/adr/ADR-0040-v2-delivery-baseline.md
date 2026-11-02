---
adr_id: ADR-0040
version: 2
title: Terminal-first baseline, then read-only browsing and resume, then the seed hop; first-milestone gates decide the design
status: Proposed
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
created_from_task: TL1.2
proposed_on: 2026-11-02
proposed_at_sha: 0e7e409a4406c62999405ea5a097a00debc4de12
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - EX-001
---

# ADR-0040: Terminal-first baseline, then read-only browsing and resume, then the seed hop; first-milestone gates decide the design

<!-- adr-header:start -->
Version 2 · Proposed 2026-11-02 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

A Tauri and Rust terminal prototype exists, and the Windows verification runner has been exercised locally. The complete product and its release gates remain unproved.
The v1 goals are many (hop, Daodan support, browser, roundtable, widget, overview); the question
is what is built first and what must be proved before the rest is built on it. Sources, kept
apart:

- Decided by the user: tiling (2026-10-29, functional-design 5.16), now implemented with the definitive Tauri shell over a Rust engine (2026-11-02, ADR-0005 v3), and Windows as the only officially supported platform for now (ADR-0047 v3); v1 hops
  seed-only (2026-10-31); the project `shinatama`, named 2026-10-26, in this repository
  (functional-design 7 item 3); the former launcher out of scope (2026-10-28, register O6); the
  terminal-first scope (2026-10-30, functional-design 5.16) and the delivery baseline
  (2026-10-31, functional-design 5.12), both confirmed as the user's on 2026-11-01.
- Author's call, "to check": the order of work and the audience (functional-design 5.12).
- Proposed shape: the first-milestone list of technical-design 10 and the gate rules of the
  register, section 0.

The full plan is in functional-design 5.12 and 5.16, technical-design 10 and the verification
register; this record holds only the decision.

Version 2 is proposed on 2026-11-02 to align EX-001 and the TL1 interpretation with the accepted shell and platform decisions. It does not report a TL1 result: the verification register remains authoritative. The user separately deferred Daodan integration on 2026-11-02; the deferred steps remain tracked without blocking independent product work.

## Decision Drivers

- Foundations first: PTY hosting, emulator fidelity and shared state decide everything above.
- A usable slice early, without waiting for every v1 surface.
- No surface delivered before its own gates pass.
- Build as if other people will install it.

## Decision

The first UI milestone is terminal-first (distinct from register row TL1, the shell-proof
gate of technical-design 3.1): project selector, resizable terminal tiles, leaf tab groups and per-harness terminal profiles, with no git integration, file navigator,
diffs, thread side panes, session browsing, hop or Daodan surfaces. The first usable release
slice adds read-only store browsing and search and same-agent native resume. The next slice is
the live seed hop. Native import is post-v1. Daodan task handover, browser UI, roundtable and
widget follow their own gates and do not block the first usable slice. No downstream surface
is treated as deliverable in the same baseline before its dependencies and G0/G1 gates are
closed. The suggested order is governed by the current implementation plan, including the binding C09R host-resilience prerequisite before terminal hardening and sidecar supervision: skin, PTY host and threads; checkup; sidecar; project manager
and launch; search and same-agent resume; seed hop with preview; roundtable; widget; browser
fast path, then the UI path; the overview grows alongside. The first milestone must prove the
foundations "or the design changes" (technical-design 10); the register is the complete list
and wins where they differ, and a failed `G0` row changes the design, with nothing built on it
before it passes. Release builds and signing on Windows are part of the plan; macOS notarization and all macOS/Linux delivery work remain deferred; the code lives
in this repository. See functional-design 5.12 and 5.16; technical-design 10; register 0.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the chosen Tauri shell is exercised with a real agent typed into its PTY, nested splits and Markdown source/preview and file panels on Windows in WebView2 -> Expected: the executable starts from embedded assets without required Node, development server or external browser startup; the WebView2 version and untested conditions are recorded as evidence for the shell proof, without reopening toolkit selection or shipping the proof fixture as product UI (register TL1; ADR-0005 v3).
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
- Register row TL1 is the shell-proof gate of technical-design 3.1, distinct from the terminal-first UI milestone: it proves the already chosen Tauri shell with Markdown and file panels in an isolated test fixture, while the milestone ships no file navigator (functional-design 5.12, 5.16). A failure requires a new version of ADR-0005 before dependent UI work, with no silent fallback.
- The other `G0` rows gate their own feature and run when it is built (register 0).
- Removing the launcher left the first-milestone set unchanged (register O6).
- Windows is the only officially supported platform under ADR-0047 v3; macOS and Linux work waits for an explicit user resumption and their own shell proof. The Oni-derived art remains a distribution decision separate from the code (register O4), and overall design approval remains separately recorded (register O5).
- The widget is built last within v1, after the core it listens to (functional-design goal 9).
- The browser fast path in the order above is gated, not guaranteed: if Moli fails the priority cases or B10, its fast path waits while Moli stays the default, and meanwhile only the explicit Patchright UI or no browser is offered (user, 2026-11-01; ADR-0035).

## Confirmation

- The register's current first-milestone rows must pass, TL1 among them. The existing two-pane prototype and F32CI evidence close no full TL/P/X gate. TL1's result must be recorded from its dedicated Windows run, and missing-WebView2 behavior must be either demonstrated or explicitly named as a gap.
- The release acceptance tests define "done" on the supported Windows platform; local prototype or CI checks do not substitute for their recorded results.
