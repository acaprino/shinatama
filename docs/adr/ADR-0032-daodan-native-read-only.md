---
adr_id: ADR-0032
version: 1
title: Native Daodan support reads the agenda and types commands; Daodan's scripts stay the only writer
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/daodan-native-support.md
    - docs/functional-design.md
    - docs/verification-register.md
  modules:
  domains:
    - daodan
    - task-agenda
    - thread-task-link
    - command-palette
    - project-manager
governs_symbols:
depends_on:
  - ADR-0031
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

# ADR-0032: Native Daodan support reads the agenda and types commands; Daodan's scripts stay the only writer

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Daodan's lock is the task holder in `.daodan/agenda.json`. Until 2026-10-27 Shinatama only
installed Daodan and typed its commands; Daodan state inside Shinatama was a non-goal. The user
reversed that on 2026-10-27: "shinatama deve essere un supporto nativo per daodan" (Shinatama
must be a native support for Daodan). Sources, kept apart:

- Decided by the user: native support (functional-design goal 5 and 5.5, 2026-10-27). Goal 5
  puts "Read-only; Daodan stays the only writer of its state" beside that decision without
  saying whether those words are the user's.
- Author's reading, not confirmed: what "native" covers, "the widest reading that does not make
  Shinatama a second Daodan" (daodan-native-support 1); the palette that types commands
  (functional-design 5.5, "my call"); and calls 1, 2, 5 and 6 of daodan-native-support 7.
- Proposed shape: the rest of daodan-native-support, "proposed, not approved".

The full design is in `docs/daodan-native-support.md`; this record holds only the decision.
Carrying a task across a hop is ADR-0033.

## Decision Drivers

- The agenda row is the lock; a second writer would be the bypass Daodan forbids.
- Task state must be visible even when no agent is running.
- Never guess: an unreadable agenda must not look like an empty one.
- Commands stay the user's act: typed, never sent.

## Decision

Shinatama reads `.daodan/agenda.json` (schema `daodan/agenda/v1`) in the main checkout, found
through the Git common directory; outside Git the project folder is its own home. The read is
schema-checked; rows are keyed by `run_id`, rebuilt and never cached as truth. A bounded poll
watches the file, with no resident service beyond the terminal's life. Shinatama's SQLite keeps
only the derived, droppable `thread_task(thread_id, run_id, link_kind, last_row_json, seen_at)`.
A thread is linked to a task when its directory equals the row's `worktree`, or is the main
checkout with the row's `branch` checked out; the link is *exact* when the native session id
equals `holder.session`, otherwise *by place*. Tasks, questions and holders appear in the
project manager, overview and widget; a stale local holder is shown, never taken over. The
`^K` palette lists Daodan's workflows from the installed manifest and the task commands *Show
status* and *Continue this task*, typed as the agent's native command; the user presses Enter.
On a project with neither `.codebase-map` nor `.daodan/runs` as a directory, Shinatama offers
Onboard, Initialize or Not now once, by typing the command. Shinatama never writes Daodan files,
never runs a Daodan script and never creates or removes worktrees. The adapter contract gains
nothing. See daodan-native-support 3, 4 and 7.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the agenda is resolved from linked worktrees under `.daodan/worktrees/` and from a plain folder -> Expected: the same agenda is found from every worktree (register D12).
- **EX-002** (boundary case): Input: a Daodan task run per host, its row compared with the agent's session store -> Expected: per agent, `holder.session` is equal, derivable or unrelated, and the link is exact or by place accordingly (register D9).
- **EX-003** (rejection or error case): Input: an agenda with an unknown `schema` value, or unreadable -> Expected: "Daodan state unreadable (schema X)", never an empty list (daodan-native-support 2, 3).
- **EX-004** (boundary case): Input: the onboarding check on a plain folder, a Git repo, a linked worktree and a project with only `.codebase-map` -> Expected: the same answer as `run_state.py first-use` in every case; the offer shows once and "Not now" sticks (register D14).
- **EX-005** (rejection or error case): Input: the palette on Hermes Agent or DeepSeek Harness -> Expected: no Daodan entry is offered or typed, while their threads still show tasks and holders (register D8, D20; daodan-native-support 4.5).

## Rejected Alternatives

- **RA-001**: Shinatama writing Daodan state; the agenda row is the lock and a second writer would bypass Daodan's grants, deliveries, questions and gates (daodan-native-support 2).
- **RA-002**: reading the agenda through Daodan's read-only MCP in `work-controller`; the direct read needs no agent and works when none is running. To revisit if D11 shows torn reads (call 1).
- **RA-003**: calling Daodan's `first-use` script for the onboarding check; it would break "Shinatama never runs a Daodan script" (call 5).
- **RA-004**: implementing Work Controller's task graph, checkpoints, addressed mailbox or scheduler; they are Daodan's roadmap (functional-design non-goals).
- **RA-005**: the earlier position that Daodan is "a set of plugins and nothing else"; a hop then forces the user to explain and take over by hand, "exactly the re-explaining Shinatama exists to remove" (daodan-native-support 1).

## Enforceable Invariants

- **INV-001**: Shinatama never writes `agenda.json`, `runs/`, `processes/` or a worktree, and never runs a Daodan script.
- **INV-002**: an agenda whose schema differs or that cannot be read is shown as unreadable, never as empty.
- **INV-003**: a thread-task link exists only while branch, worktree or session still match; it is never repaired by assumption.
- **INV-004**: every Daodan command Shinatama offers is typed into the prompt and never sent.
- **INV-005**: no holder is taken over automatically; a holder whose `host` is another machine is shown and never judged stale.
- **INV-006**: palette workflows come from the installed manifest, not from a hard-coded list (register D8).

## Consequences

- functional-design goal 5, two non-goals and 5.5 are amended; the engine spec absorbs the reader module, the `thread_task` table and the hop seed block after approval (daodan-native-support 5). technical-design does not contain them yet.
- The new-project dialog may type Daodan's init command and never runs it (functional-design 5.4).
- Open: link exactness (D9), command spellings per host (D10), torn reads (D11), worktree resolution on Windows paths (D12); all six author's calls are marked "check them"; a new mockup comes with approval. The old `daodan-flow.html` mockup is not revived as is.

## Confirmation

- Register rows D8, D9, D10, D11, D12 and D14 must pass; D9 and D10 are G0. None has been run.
- daodan-native-support 6: no D-row result changes the principle that Daodan is the only writer.
