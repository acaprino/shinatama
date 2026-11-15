---
adr_id: ADR-0055
version: 2
title: A thread is a named set of sessions; the window shows the sessions of the threads that are on, in one layout; the stored conversation record keeps its invariants and is called a session in the interface
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - crates/state/src/worksets.rs
    - crates/state/src/threads.rs
    - crates/state/src/migrations.rs
    - apps/desktop/src-tauri/src/threads.rs
    - apps/desktop/src-tauri/src/workspace.rs
    - apps/desktop/src/thread-sidebar.ts
    - apps/desktop/src/main.ts
    - apps/desktop/src/workspace-renderer.ts
    - docs/frd/FRD-0005-threads-and-hop.md
    - docs/frd/FRD-0001-workspace-and-agent-hosting.md
  modules:
  domains:
    - threads
    - workspace-layout
    - sidebar
governs_symbols:
depends_on:
  - ADR-0005
  - ADR-0009
  - ADR-0017
  - ADR-0039
supersedes:
created_from_task: user-decision-2026-11-13-thread-is-a-set-of-sessions
proposed_on: 2026-11-13
proposed_at_sha: dbfd634567bac828248c367cce7095a9fc64beac
accepted_on: 2026-11-13
accepted_at_sha: 71f4712be3d870bcb31acd3a6a54e66e4198aff4
---

# ADR-0055: A thread is a named set of sessions; the window shows the sessions of the threads that are on, in one layout; the stored conversation record keeps its invariants and is called a session in the interface

## Context

Today the sidebar lists `thread` records and the window shows tabs. A `thread` record is one
conversation: an ordered chain of segments with at most one live activation
(`activation_thread_open`, `crates/state/src/threads.rs`; FRD-0005 R01, R02). The window has one
layout for everything (`global_workspace`, id `main`). The result is two lists of the same work:
every open conversation is a sidebar row and a tab, a click on an open row only focuses its tab,
and nothing groups conversations that belong together.

On 2026-11-13 the user rejected that ("è ridondante e inutile") and decided the model:

- project → thread → sessions;
- a thread is a set of sessions, which may be unrelated, in dialogue or linked in another way;
- a click on a thread shows all and only its sessions;
- every thread has its own saved tiled layout (one maximized pane, two columns, and so on).

## Amendment v2 (2026-11-15, user decision)

The user asked to see the threads of several projects at once and to arrange their tabs freely
("i thread solo on/off selezionabili … le tab posso disporle io nel layout a pannelli"). What
changes from v1, everywhere below:

- A thread is **on** or **off**; a click on its row switches it. Any number of threads are on.
  The window shows the sessions of every thread that is on, plus terminals of no thread.
- The window has **one** saved layout, the global record `main`. Threads no longer keep a layout
  of their own; a thread turned on puts its tabs in the focused pane, where the user drags them.
- The sidebar lists the threads of every project ticked in the Project menu, grouped by project.
- Per-session **Hide** is gone with the pane's option menu: a session keeps running without its
  tab when its thread is turned off. Close and Forget are unchanged.
- A tab is named "thread · agent"; the project is shown by the tab filter, not in the name.

## Decision

### 1. Vocabulary

| Word in the interface | Stored as | Meaning |
|---|---|---|
| **Thread** | `workset` (`w_…`) | A named set of sessions of one project, with one saved layout. |
| **Session** | `thread` (`t_…`), unchanged | One conversation: the chain of segments FRD-0005 describes. |

The stored name `thread` is not renamed: segments, activations, hops, the journal and Mukade
bindings keep their tables, ids and invariants. Only the interface and the requirement text move
the word "thread" up one level. Code and SQL call the new level **workset** so that no file holds
two meanings of `thread`.

### 2. Records

- `workset(id, project_id, title, archived, revision, created_at, updated_at)`.
- `workset_member(thread_id PRIMARY KEY → thread, workset_id → workset)`: every `thread` record
  belongs to exactly one workset; the primary key makes a second membership impossible.
- v2: the window's layout is the global `workspace` record `main` (FRD-0001 R28: versioned
  JSON, revision, one owning window). The per-workset records written under v1 stay in the
  database, unused. No second layout store.
- Migration: every existing `thread` without a parent gets a workset of its own, titled like the
  thread; a thread with `parent_thread_id` or `companion_worker_thread_id` joins the workset of
  its root. Nothing is deleted or rewritten in `thread`, `segment` or `activation`.

### 3. Membership rules

- A new session is created in the thread it was asked for (a pane's +, a thread's +), else in a
  thread of its project that is on. Otherwise starting a session creates a thread and turns it on.
- A fork (ADR-0017), a companion and a roundtable seat are created in the workset of the session
  they come from. This is where "sessions in dialogue" live side by side.
- A session moves to another thread only by an explicit user action; membership is never inferred
  from titles, folders or agents.
- A hop in close mode adds a segment to the same session, as today; nothing changes for the workset.

### 4. What the window shows

- The sidebar lists the threads of the projects ticked in the Project menu, grouped by project,
  most recently used first. A row shows the title, how many sessions it holds and how many are
  running.
- A click on a thread turns it on or off. The tabs of the window are the sessions of every thread
  that is on. Sessions of threads that are off keep running; their terminals are parked, not
  stopped, and their output keeps draining.
- A session of a thread that is on and is not running is still shown, as a tab that says so and
  offers Resume.
- Two different actions, never merged (user, 2026-11-13; Hide removed in v2). **Close** (the tab's
  ×) stops the session, after asking if it runs, and keeps
  it: it leaves the tabs and is listed under "Closed sessions" of its thread, from where it is
  reopened as a tab (not started) or forgotten. **Forget** deletes: one closed session after a
  second click, or the whole thread with every session, closed ones included. Archiving a thread
  deletes nothing. A closed session is the stored record's `archived` mark.
- The sidebar no longer lists sessions as rows of their own, and a click never "focuses a tab":
  the only two levels are the thread list and the panes.
- Attention (an agent waiting in a thread that is off) is shown on that thread's row; the
  attention control turns the thread on and focuses the session.

### 5. Layout saving

- v2: the window's layout `main` is loaded, claimed, saved and released with the existing global
  workspace commands and their compare-and-swap. Turning a thread on or off reconciles that one
  layout with the members of the threads that are on (a tab for every session, none for anything
  else); nothing is claimed or released on a switch.
- The threads that are on are restored at start before the saved layout loads, so their tabs
  keep the panes they had.
- A new session's own workset is made by the state layer; the window then moves the session into
  the thread it was asked for (`workset_adopt`), and the emptied, unnamed workset is deleted with it.

## Rejected alternatives

- **RA-001: keep one list and rename rows to match tabs.** Removes the naming mismatch, keeps the
  duplication the user rejected.
- **RA-002: allow several live activations per `thread` record.** Breaks invariant I1 and the hop
  state machine (FRD-0005 R02), which exist for sound reasons; grouping is a level above.
- **RA-003: store the layout inside the workset row.** A second layout store with its own
  ownership and conflict rules; the `workspace` record already has them.
- **RA-004: rename the `thread` table to `session`.** A migration over every table, the journal and
  every document for no behaviour; the interface word is enough.

## Invariants

- **INV-001**: every `thread` record has exactly one `workset_member` row, in the same project as
  its workset.
- **INV-002**: the window draws no tab of a session that is not a member of a workset that is on.
- **INV-003**: turning a thread on or off starts, resumes and stops no process.
- **INV-004**: FRD-0005's invariants (one live activation per session, read-only native stores)
  are unchanged.
- **INV-005**: a workset with a running session cannot be forgotten.

## Examples

- **EX-001** (nominal, v2): threads "Billing refactor" (Claude Code, Codex) and "Docs" (Pi) are on;
  the window shows three tabs, arranged by dragging. Clicking "Docs" turns it off: its tab leaves,
  Pi keeps running; clicking it again brings the tab back. All three agents ran throughout.
- **EX-002** (nominal): in "Billing refactor" the user forks the Claude session to Codex; the new
  session appears as a tab of the same thread.
- **EX-003** (nominal): after a restart, "Docs" is on again and shows the Pi tab as not running
  with Resume; nothing started by itself.
- **EX-004** (migration): a database with 12 threads, two of them forks of a third, becomes 10
  worksets; the fork family is one workset of three sessions.
- **EX-005** (error): a thread that no longer exists is not turned on at start; the others are.

## Consequences

- FRD-0005 section 1 and FRD-0001 R28 are amended: "thread" in user-facing requirements means the
  set; the conversation record is "session"; v2: the layout is per window.
- Sidebar actions (rename, archive, forget) apply to the thread; the same actions for one session
  move to its tab or pane.
- Search results and the sessions surface open the thread that owns the session, then focus it.
- The one-layout-per-window ownership stays.

## Confirmation

- State tests: the migration (EX-004), INV-001 and INV-005.
- Desktop smoke: EX-001 and EX-003 through the real app, with INV-003 checked on the process list.
- Register rows are added with the plan.
