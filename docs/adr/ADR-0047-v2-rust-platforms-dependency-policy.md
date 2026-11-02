---
adr_id: ADR-0047
version: 2
title: Rust engine and TypeScript frontend; macOS and Linux required, Windows assumed; one allowed-dependency list per ecosystem, each addition a decision
status: Superseded
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - Cargo.toml
    - package.json
    - package-lock.json
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
created_from_task: tauri-definitive-2026-11-02
proposed_on: 2026-11-02
proposed_at_sha: 65bee163e43b0b9f7b2d71f4e353b9fe4c580df2
accepted_on: 2026-11-02
accepted_at_sha: 65bee163e43b0b9f7b2d71f4e353b9fe4c580df2
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: ADR-0047 v3
changes:
  - EX-001
  - EX-004
  - EX-006
  - EX-007
  - INV-002
  - INV-004
  - INV-005
  - INV-006
  - RA-001
  - RA-002
  - RA-003
  - RA-005
---

# ADR-0047: Rust engine and TypeScript frontend; macOS and Linux required, Windows assumed; one allowed-dependency list per ecosystem, each addition a decision

<!-- adr-header:start -->
Version 2 · Superseded by ADR-0047 v3 · proposed 2026-11-02 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

The user decided on 2026-11-02 that Tauri is the definitive V1 shell (ADR-0005 v2). The previous
version of this record excluded it and left the UI toolkit unpinned, and it counted only Rust
dependencies, while the desktop application now also has a TypeScript frontend and npm packages.

Sources, kept apart:

- Decided by the user: Rust, and macOS and Linux as required platforms (the original request;
  functional-design 3); Tauri as the shell (2026-11-02).
- Author's calls: Windows assumed required, to be confirmed (functional-design 7.1; register O3);
  the reasons for Rust (one toolchain that cross-builds for three systems, mature pseudo-terminal
  crates, many concurrent tasks); one allowed list per ecosystem ("adding one is a decision, not a
  side effect"); edition 2024 and resolver 3; TypeScript for the frontend, as the prototype uses.

What is not proved: that a Rust pseudo-terminal with xterm.js hosts the six agents faithfully on the
three systems (register P1, P2), and that the crates and packages in use are maintained,
licence-compatible and work on them (register P17). The Windows prototype is partial evidence only.
The full text is in functional-design 3 and 7 and technical-design 3.1; this record holds only the
decision.

## Decision Drivers

- Shinatama hosts agents: it needs a pseudo-terminal and a terminal emulator on all three systems.
- One Rust toolchain builds and cross-builds the engine for macOS, Linux and Windows.
- A dependency is an attack surface and a licence obligation; none may arrive as a side effect, in
  either ecosystem.
- The first milestone must prove the foundations or the design changes (ADR-0040).

## Decision

The Shinatama engine is written in Rust; the frontend is TypeScript in the Tauri webview (ADR-0005).
macOS and Linux are required platforms; Windows is assumed required until the user confirms (register
O3), and if it is in v1, ConPTY, Windows paths, environment and trust handling are part of the first
milestone. The Cargo workspace uses edition 2024 and resolver 3.

`[workspace.dependencies]` is the only list of Rust dependencies the crates may use. The root
`package.json` is the only list of npm packages, with exact versions and `package-lock.json`
committed; no other manifest declares packages. Node is a build and test tool and never a runtime
requirement of the desktop application. Adding a dependency to either list is a recorded decision,
never a side effect of a component's needs, and each entry's licence is checked against MIT (register
X3). The `mukade` crate is declared in the Rust list as a Git dependency at a reviewed commit, with
`Cargo.lock` committed (ADR-0020). The pseudo-terminal, terminal-emulation and SQLite choices are
shortlisted with versions and licences before the first milestone (register P17). Startup time, idle
and per-instance memory and binary size are measured on the three systems and the targets set from
the data (register P18).

## Verified Interview Examples

- **EX-001** (nominal case): Input: a new runtime crate is created -> Expected: it is listed in `[workspace] members`, uses only `workspace = true` dependencies from the allowed list, and `cargo build` passes on the three systems in CI.
- **EX-002** (rejection or error case): Input: a pull request adds a crate directly to a member's `[dependencies]` with a version of its own -> Expected: refused in review; the crate is first added to `[workspace.dependencies]` with a one-line reason and its licence noted.
- **EX-003** (boundary case): Input: a candidate PTY crate has no Windows ConPTY support -> Expected: it fails P1 and is not shortlisted; the shortlist records why.
- **EX-004** (rejection or error case): Input: a crate or npm package under a licence incompatible with MIT -> Expected: not added (register X3); an alternative is sought or the feature waits.
- **EX-005** (boundary case): Input: the user answers "no" to Windows in v1 -> Expected: the Windows rows of the register move out of the first-milestone set, and the design keeps macOS and Linux only; nothing else changes.
- **EX-006** (rejection or error case): Input: a pull request adds an npm package with a version range, or a second `package.json` under `apps/desktop` -> Expected: refused in review; the package goes in the root manifest at an exact version, with its reason and licence noted and the lockfile updated.
- **EX-007** (nominal case): Input: the built desktop executable runs with no Node on the machine -> Expected: it starts normally.

## Rejected Alternatives

- **RA-001**: Electron, or a native Rust toolkit such as GPUI or egui, for the main UI; not chosen (ADR-0005).
- **RA-002**: Go with tmux, as Agent Deck does; Windows only through WSL, and no desktop UI of its own (technical-design 6.8).
- **RA-003**: letting each crate or application declare its own dependency versions; two versions of one dependency and unreviewed licences would arrive silently.
- **RA-004**: deciding Windows now without the user; it is the user's call (register O3), and the design already assumes yes.
- **RA-005**: exempting frontend packages from the allowed-list rule because they are build-time or UI-only; they ship in the executable or shape it, so they carry the same risk.

## Enforceable Invariants

- **INV-001**: every dependency of a workspace crate is an entry of `[workspace.dependencies]` with `workspace = true`; no member declares a version of its own.
- **INV-002**: no dependency is added to either list without a recorded reason and a licence compatible with MIT (register X3).
- **INV-003**: the product is built for macOS and Linux; Windows is built for as long as O3 stands assumed or decided yes.
- **INV-004**: no dependency that fails the first-milestone gates (P1, P2, P17) is kept in an allowed list for that purpose.
- **INV-005**: the shell is Tauri (ADR-0005); no second UI toolkit is added without a new version of that record.
- **INV-006**: the root `package.json` holds every npm package at an exact version, with `package-lock.json` committed, and the desktop application needs no Node at run time.

## Consequences

- `Cargo.toml` keeps version requirements and `Cargo.lock` the resolved versions; the comments that call the Tauri dependencies prototype-only must be realigned, and the dependency table moves out of the prototype note into the X3 record.
- FRD-0001 R51 and register P17 must drop the native-UI crate and the toolkit candidates; npm packages join the X3 and P17 scope.
- Cross-building for three systems is a release concern (ADR-0048); Tauri links the system webview on each system, so cross-building and the Linux musl target are unproved and must be reviewed there.
- The Windows assumption shapes the first milestone (ConPTY, `CreateProcessW` limits, ACLs instead of file modes); a "no" removes those rows, not the design.

## Confirmation

- Register P1, P2, P17 and P18 pass; X3 lists every licence in both lists; O3 is answered.
- Not yet confirmed: a Windows prototype run exists, the release-wide licence audit does not, and macOS and Linux have not been built.
