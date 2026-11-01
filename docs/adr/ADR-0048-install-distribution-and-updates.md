---
adr_id: ADR-0048
version: 1
title: Built for others to install; signed and notarized release builds; Shinatama's own updates never touch the sidecar's rule; no unintended outbound connection; Oni art is a separate distribution decision
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/verification-register.md
  modules:
  domains:
    - install
    - distribution
    - release
    - updates
    - licensing
governs_symbols:
depends_on:
  - ADR-0029
  - ADR-0047
created_from_task: frd-adr-consolidation-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: 620bcf9a66edeb57d0528b3c522f3018928b3706
accepted_on: 2026-11-01
accepted_at_sha: 202a34e2675cfe7e8d7ba1d5e7ecf88bf2299182
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0048: Built for others to install; signed and notarized release builds; Shinatama's own updates never touch the sidecar's rule; no unintended outbound connection; Oni art is a separate distribution decision

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at 202a34e · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. Goal 8 asks for easy install on macOS and Linux, and functional-design
5.12 says to build as if other people will install it, because the install instructions of Daodan
and Shinatama already point to public repositories. The register's section 17 (X1 to X6) lists the
install, distribution, legal and telemetry checks, but no record held the decisions behind them.
Sources, kept apart:

- Decided by the user: easy install on macOS and Linux (goal 8); the sidecar's no-auto-update rule
  (functional-design 5.6; ADR-0029).
- Author's calls: audience "others may install it", hence release builds, signing and
  notarization (5.12); Node provisioning for the sidecar as the builder's call within the install
  story (5.6); Shinatama's own updates specified and separate from the sidecar's rule (register
  X6); no unintended outbound connections (X4); licence compatibility (X3); the Oni-derived art as
  a distribution decision separate from the code (5.8, 5.12; register W9, O4).

This record holds only the decision.

## Decision Drivers

- A public install story is already promised by the repositories' instructions.
- The sidecar must never update itself (ADR-0029); Shinatama's updater must not become the loophole.
- A local-first product must not phone home.
- Bungie's Oni IP is fine for personal use and a legal question for distribution.

## Decision

Shinatama is built for others to install: release builds for macOS (arm64, x86_64), Linux (glibc
and musl; x86_64, aarch64) and, while O3 stands, Windows x64, with code signing and macOS
notarization, delivered as release binaries, an installer script or package managers at the
builder's choice (register X1). A first run on a clean machine from install to the first hop is
timed before release (X2). Node for the sidecar is provided by a mechanism the builder chooses
(system, bundled or downloaded) and measured for offline use, size and version (SC13). Shinatama's
own update mechanism is specified and never touches the sidecar's rule: the owned sidecar source
upgrades only through Shinatama and never by itself (X6); update checks and model-catalog calls run
only when the user enabled them, and Shinatama has no telemetry of its own, so the only outbound
connections are the agents' traffic through the sidecar and the calls the user enabled (X4). Every
dependency and bundled asset carries a licence compatible with MIT and appears in a dependency
notice (X3). The Oni-derived art (the default skin's character, the widget) is a distribution
decision taken separately from the code (W9, O4). Pinned external components (Mukade, Moli,
Patchright, the local model) advance only with a recheck, and their versions, checksums, licences
and update behaviour are recorded.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a release is cut -> Expected: binaries for every target of the matrix, signed and notarized where the platform requires it, with a dependency notice and the recorded checksums of the pinned components (register X1, X3).
- **EX-002** (nominal case): Input: a clean Ubuntu machine, install to first hop -> Expected: the elapsed time and every failure are recorded (register X2).
- **EX-003** (rejection or error case): Input: a packet capture of an idle Shinatama with update checks disabled -> Expected: no outbound connection other than the agents' own traffic through the sidecar (register X4).
- **EX-004** (rejection or error case): Input: a Shinatama update that would also pull a newer sidecar from npm -> Expected: refused; the sidecar changes only when Shinatama's owned source changes (ADR-0029 INV-001; register X6).
- **EX-005** (boundary case): Input: the widget's frames use the Oni character -> Expected: shipped in a personal build; a distributed build waits for the decision of W9 and O4.

## Rejected Alternatives

- **RA-001**: a personal build only, with no signing; the repositories' instructions already address other people (functional-design 5.12).
- **RA-002**: installing the sidecar's precompiled npm package for convenience; ADR-0029 forbids it.
- **RA-003**: telemetry or an update check on by default; a local-first product with no account (ADR-0008) and register X4.
- **RA-004**: deciding the Oni art's distribution inside the code; it is a legal decision for the user (O4).

## Enforceable Invariants

- **INV-001**: a release build is signed, and notarized on macOS, before it is published.
- **INV-002**: Shinatama's updater never changes the sidecar; the sidecar changes only through the owned source (ADR-0029 INV-001).
- **INV-003**: no outbound connection is made that the user did not enable, except the agents' traffic through the sidecar (register X4).
- **INV-004**: every dependency and bundled asset has a recorded licence compatible with MIT (register X3).
- **INV-005**: a pinned component advances only with a recorded recheck of its API, versions and evidence.
- **INV-006**: Oni-derived art ships in a distributed build only after the decision of W9 and O4.

## Consequences

- Release work (signing certificates, notarization, cross-builds) is part of the plan from the first usable slice (ADR-0040).
- Node provisioning and the install mechanism are measured, not assumed (SC13, X1, X2).
- The dependency notice includes bundled fonts (ADR-0039) and the local model's licence (ADR-0028).

## Confirmation

- Register X1 to X6 and SC13 pass; O3 and O4 are answered.
- Nothing has run.
