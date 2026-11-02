---
adr_id: ADR-0047
version: 1
title: Rust runtime; macOS and Linux required, Windows assumed; one allowed-dependency list in the workspace, each addition a decision
status: Superseded
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - Cargo.toml
  modules:
  domains:
    - language
    - platforms
    - dependency-policy
    - rust-toolkit
governs_symbols:
depends_on:
  - ADR-0005
  - ADR-0010
created_from_task: frd-adr-consolidation-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: 620bcf9a66edeb57d0528b3c522f3018928b3706
accepted_on: 2026-11-01
accepted_at_sha: 202a34e2675cfe7e8d7ba1d5e7ecf88bf2299182
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: ADR-0047 v2
changes:
---

# ADR-0047: Rust runtime; macOS and Linux required, Windows assumed; one allowed-dependency list in the workspace, each addition a decision

<!-- adr-header:start -->
Version 1 · Superseded by ADR-0047 v2 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The language, the platform matrix and the dependency rule are stated in
functional-design 3 and in the comments of `Cargo.toml`, but no record holds them, and ADR-0005
covers only the UI toolkit. Sources, kept apart:

- Decided by the user: Rust, and macOS and Linux as required platforms (the original request;
  functional-design 3).
- Author's calls: Windows assumed required, to be confirmed (functional-design 7.1; register O3);
  the reasons for Rust (one toolchain that cross-builds for three systems, mature pseudo-terminal,
  escape-parser and native-UI crates, many concurrent tasks); the workspace's single allowed
  dependency list (`Cargo.toml`, "adding one is a decision, not a side effect"); edition 2024 and
  resolver 3.

What is not proved: that a Rust pseudo-terminal plus emulator hosts the six agents faithfully on
Windows with ConPTY (register P1, P2), and that the named crates exist, are maintained,
licence-compatible and work on the three systems (register P17). The full text is in
functional-design 3 and 7 and technical-design 3.1; this record holds only the decision.

## Decision Drivers

- Shinatama hosts agents: it needs a pseudo-terminal and a terminal emulator on all three systems.
- One toolchain builds and cross-builds for macOS, Linux and Windows.
- A dependency is an attack surface and a licence obligation; none may arrive as a side effect.
- The first milestone must prove the foundations or the design changes (ADR-0040).

## Decision

Shinatama is written in Rust. macOS and Linux are required platforms; Windows is assumed required
until the user confirms (register O3), and if it is in v1, ConPTY, Windows paths, environment and
trust handling are part of the first milestone. The Cargo workspace uses edition 2024 and resolver
3 and has no members until the runtime's first crate exists. `[workspace.dependencies]` is the only
list of dependencies the crates may use; adding a dependency is a recorded decision, never a side
effect of a crate's needs, and each entry's licence is checked against MIT (register X3). The
`mukade` crate is declared there as a Git dependency at a reviewed commit with the lockfile
committed when the first crate exists (ADR-0020). The pseudo-terminal, terminal-emulation, native-UI
and SQLite crates are shortlisted with versions and licences before the first milestone (register
P17); the UI toolkit is pinned only after the combined prototype gate passes (ADR-0005). Startup
time, idle and per-instance memory and binary size are measured on the three systems and the
targets set from the data (register P18).

## Verified Interview Examples

- **EX-001** (nominal case): Input: the first runtime crate is created -> Expected: it is listed in `[workspace] members`, uses only `workspace = true` dependencies from the allowed list, and `cargo build` passes on the three systems in CI.
- **EX-002** (rejection or error case): Input: a pull request adds a crate directly to a member's `[dependencies]` with a version of its own -> Expected: refused in review; the crate is first added to `[workspace.dependencies]` with a one-line reason and its licence noted.
- **EX-003** (boundary case): Input: a candidate PTY crate has no Windows ConPTY support -> Expected: it fails P1 and is not shortlisted; the shortlist records why.
- **EX-004** (rejection or error case): Input: a crate under a licence incompatible with MIT -> Expected: not added (register X3); an alternative is sought or the feature waits.
- **EX-005** (boundary case): Input: the user answers "no" to Windows in v1 -> Expected: the Windows rows of the register move out of the first-milestone set, and the design keeps macOS and Linux only; nothing else changes.

## Rejected Alternatives

- **RA-001**: a web-built shell or an Electron or Tauri application; decided against on 2026-10-29 (ADR-0005).
- **RA-002**: Go with tmux, as Agent Deck does; Windows only through WSL, and no native UI (technical-design 6.8).
- **RA-003**: letting each crate declare its own dependency versions; two versions of one crate and unreviewed licences would arrive silently.
- **RA-004**: deciding Windows now without the user; it is the user's call (register O3), and the design already assumes yes.

## Enforceable Invariants

- **INV-001**: every dependency of a workspace crate is an entry of `[workspace.dependencies]` with `workspace = true`; no member declares a version of its own.
- **INV-002**: no dependency is added without a recorded reason and a licence compatible with MIT (register X3).
- **INV-003**: the product is built for macOS and Linux; Windows is built for as long as O3 stands assumed or decided yes.
- **INV-004**: no crate that fails the first-milestone gates (P1, P2, P17) is kept in the allowed list for that purpose.
- **INV-005**: the UI toolkit is not pinned before the TL1 prototype gate passes (ADR-0005).

## Consequences

- The workspace stays empty until the first crate; the two probe workspaces under `verify/H27/` are not members and not part of the product.
- Cross-building for three systems is a release concern (ADR-0048).
- The Windows assumption shapes the first milestone (ConPTY, `CreateProcessW` limits, ACLs instead of file modes); a "no" removes those rows, not the design.

## Confirmation

- Register P1, P2, P17 and P18 pass; X3 lists every licence; O3 is answered.
- Nothing has run; the crates have not been shortlisted.
