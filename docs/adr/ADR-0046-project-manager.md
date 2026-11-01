---
adr_id: ADR-0046
version: 1
title: Project manager over project roots, sorted by last use, with a default root, a new-project shortcut and a per-instance worktree option
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - docs/daodan-native-support.md
  modules:
  domains:
    - project-manager
    - project-roots
    - new-project
    - worktree-per-instance
governs_symbols:
depends_on:
  - ADR-0009
  - ADR-0033
created_from_task: frd-adr-consolidation-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: 620bcf9a66edeb57d0528b3c522f3018928b3706
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0046: Project manager over project roots, sorted by last use, with a default root, a new-project shortcut and a per-instance worktree option

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The project manager is the first surface the user sees and the one
every launch starts from, yet no decision record covered it: ADR-0004 to ADR-0045 were backfilled
by domain and left functional-design 5.4 without a record, although most of its rules are the
user's decisions. Sources, kept apart:

- Decided by the user: an integrated project manager with a default root directory and a shortcut
  to create a new project (goal 4); on 2026-11-01, sorting by last use in Shinatama, the
  definition of non-standard folders, the `/` filter, `n` outside the filter, Daodan's init in the
  new-project dialog as the only Daodan offer for a created project, and the own-worktree option
  as part of the new-instance command rather than an overview action (functional-design 5.4, 7;
  daodan-native-support 4.7).
- Author's calls: pinned projects; git branch and dirty count; last agent and thread per project;
  threads listed beside the project; one key to resume the last thread; several roots; optional
  `git init`; the robustness rules PM1, PM4 and PM7.

The full design is in functional-design 5.4; this record holds only the decision.

## Decision Drivers

- The user opens projects many times a day; the list must put the right one first without a search.
- A new project must be created in a known place with one keystroke and never half-created.
- Two agents on one project must be able to work without colliding.
- Daodan is offered, never run (ADR-0033).

## Decision

The project manager lists, as projects, the first-level folders of each configured project root,
sorted by last use in Shinatama (`project.last_opened`, the last open or last thread activity),
with projects never opened sorted by folder modification time. An incremental filter opens with
`/` or by focusing the search field; any folder opens with F3; a history keeps the last 10
non-standard folders, that is, folders opened with F3 that are not under a project root. A default
root, chosen at first run and changeable, is where new projects are created; `n`, outside the
filter, creates one, with an optional `git init` and an option to type Daodan's native init command
into the chosen agent's prompt, never run; for a project Shinatama creates that option is the only
Daodan offer, and declining it is the per-project "Not now" of daodan-native-support 4.7. Each row
shows its git branch and dirty count, the last agent and thread, its threads and its open Daodan
tasks; one key resumes the last thread; projects can be pinned. The new-instance command, from the
project manager or a tab, can start the instance in a new linked worktree; it is an option of that
command, not an overview action. Listing never blocks the UI; a missing root is reported; creating
a project never leaves a half-created folder. The final keys are set with register U3.

## Verified Interview Examples

- **EX-001** (nominal case): Input: two roots with 300 folders, one on a Google Drive path, opened for the first time -> Expected: the list appears without blocking the UI, sorted by folder modification time, and the Drive root's latency is recorded (register PM1).
- **EX-002** (nominal case): Input: the user presses `n` outside the filter, names a project and ticks `git init` -> Expected: the folder is created under the default root with the default branch, or nothing is created and the error is shown; no half-created folder remains (register PM4).
- **EX-003** (boundary case): Input: the user opens `C:\Users\alfio\Desktop\scratch` with F3 -> Expected: it opens as a project and enters the non-standard history, since it is outside every root; the eleventh such folder pushes the oldest out.
- **EX-004** (rejection or error case): Input: a root on a removed drive -> Expected: the root is shown as missing with its path; nothing is silently dropped (register PM7).
- **EX-005** (nominal case): Input: new instance on a project where another instance already works, with "own worktree" ticked -> Expected: a linked worktree is created and the instance starts there; the overview shows two worktrees and no collision for equal paths (technical-design 8.5).
- **EX-006** (boundary case): Input: the user declines the Daodan init option when creating a project, then opens the project later -> Expected: no onboarding offer appears; "Not now" was recorded for that project (daodan-native-support 4.7).

## Rejected Alternatives

- **RA-001**: sorting by name or by folder modification time alone; the user decided last use in Shinatama, with modification time only for projects never opened (2026-11-01).
- **RA-002**: creating a project in the current folder or asking for a location every time; a default root chosen at first run is the user's decision (goal 4).
- **RA-003**: the own-worktree action in the overview; the user decided it belongs to the new-instance command and that the overview stays read-mostly (2026-11-01; ADR-0028).
- **RA-004**: running Daodan's init or `git init` automatically on creation; Daodan commands are typed, never sent (ADR-0033), and `git init` is an explicit option.
- **RA-005**: a second Daodan offer for a project Shinatama created; the user decided the dialog's option is the only one (2026-11-01).

## Enforceable Invariants

- **INV-001**: projects opened in Shinatama sort by `project.last_opened` descending; never-opened projects follow, by folder modification time.
- **INV-002**: a folder enters the non-standard history only if it was opened with F3 and is not under a configured root; the history holds at most 10.
- **INV-003**: `n` creates a project only when the filter is not active, and only under the default root or a root the user picks in the dialog.
- **INV-004**: no Daodan command and no `git init` runs without the user's explicit action; the Daodan option types text and the user presses Enter.
- **INV-005**: creating a project either completes or leaves no folder behind (register PM4).
- **INV-006**: listing and sorting a root never run on the UI thread (register PM1); a missing root is shown, not ignored (register PM7).
- **INV-007**: starting an instance in its own worktree is offered only by the new-instance command; the overview offers no instance-starting action (ADR-0028 INV-007).
- **INV-008**: a project Shinatama created receives one Daodan offer in total; declining it records "Not now" (ADR-0033 INV-007).

## Consequences

- The `project` table (ADR-0009) carries `root`, `pinned` and `last_opened`; the non-standard history and the default root are settings.
- Git facts for 30 projects need parallel collection with timeouts and a cache (register PM2); the configuration's location per OS is PM8.
- Keys (`/`, `n`, F3) are placeholders until register U3 settles bindings that no agent or the desktop consumes.
- The per-instance worktree feeds the overview's collision rule (ADR-0028): equal paths in different worktrees do not collide.

## Confirmation

- Register PM1 to PM8 pass; U3 settles the keys; D14 confirms the onboarding parity and the single offer for created projects.
- Nothing has run.
