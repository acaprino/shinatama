---
adr_id: ADR-0055
version: 4
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
    - apps/desktop/src/session-tabs.ts
    - apps/desktop/src/workset-sidebar.ts
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

## Amendment v3 (2026-11-15, user report)

The user could not tell what the threads were doing ("non si capisce mai cosa accade con i
thread", "ne apro uno, ne spuntano tanti", "ne chiudo uno, spariscono tutti"). Causes and changes:

- The thread store refused every request that arrived while another ran ("busy"). The sidebar
  reads its projects in parallel, so most of them failed at random and their rows came and went;
  a new session whose move into the thread that was on was refused got a thread of its own. The
  store now queues requests in arrival order (up to 15 s) instead of refusing them, and the
  window asks once instead of retrying.
- Each row carries a square that is filled when the thread is on; an off thread with running
  sessions says "N running · tabs hidden".
- Rename, Archive, Forget and the thread's closed sessions open under the row they belong to
  (⋯), for that thread, on or off. The action bar under the list, which acted on the thread
  worked in last, is gone.
- Removing a project from the sidebar (its × or its tick in the Project menu) turns its threads
  off, so no tab is left without a row to turn it off or on again.
- A thread that is on has a colour: its row's square is filled with it and each of its tabs
  carries a square of it before the name, so the tabs of two threads in one pane can be told
  apart. Eight far-apart colours are handed out in turn and kept while the thread stays on; a
  thread that is off has no colour and an empty grey square.
- A tab of an unnamed thread is named "project folder · agent", never "agent · agent".
- The window's layout `main`, when marked by an earlier Shinatama process proven dead, is taken
  over at start without a button. While a live second window holds it, this window keeps its
  own panes, says so once and asks again every 30 s, so it saves them once that window closes.

## Amendment v4 (2026-11-18, user report and decision)

The user reported six defects on 2026-11-18 and decided how each must behave. What changes from
v3, everywhere below:

- **Tab names.** Nine sessions of one thread were nine tabs cut to the same thread name, and a new
  session looked like a duplicate. A tab now names its session first, then the thread (its title,
  else the project folder), then the state: "Claude Code 2 · Billing refactor · not running". The
  session's name is the one the user gave it, else its agent, numbered when other sessions of the
  same thread share it. The number follows the order of all the thread's sessions, oldest first
  and closed ones included, so it is the same on the running tab, on its "not running" tab and in
  the thread's panel, and it survives a restart. This replaces "thread · agent" (v2) and
  "project folder · agent" (v3). A terminal of no thread keeps "folder · agent".
- **Project filter.** The project chips above the tabs count every tab, running or not; a tab that
  is not running belongs to its saved session's project. After a restart the chips are there and
  a chip shows that project's "not running" tabs.
- **Reopening after a restart.** The layout and its sessions are restored together, never one
  without the other (user, 2026-11-18: "non posso ripristinare una cosa senza l'altra"). Once
  the saved layout is back, every session that was running when Shinatama last closed or crashed
  is reopened by itself, one after the other in the order of the panes and their tabs, in the
  background: each terminal takes its tab's place, and the tab each pane shows, the project
  filter and the focus stay as they are. (A reopen only when a tab is shown was considered the same
  day and rejected: it left the layout back without its sessions until each tab was looked at, and
  made a click on a tab start an agent.) A session still marked open by the Shinatama that is gone
  is recovered first (`thread_recover` verifies that owner is gone and refuses otherwise), then
  resumed. One reopen runs at a time; each session and step is tried once per run, so a refusal
  leaves the tab as before, with its button and its notice. While the reopen runs, the tab of each
  session says whether it reopens now or waits its turn, and a waiting one offers "Don't reopen".
  A session the user stopped, or that ended by itself, stays "not running" with Resume; so does a
  shell, or an agent without native resume. The saved layout is what says a session was running: the tab of a running
  terminal is saved with its instance reference, a tab whose session the user stopped, ended by
  itself or was refused its reopen is saved without one, and the ends caused by a confirmed window
  close change nothing. There is no second record. A session of a thread that was off at the close
  has no tab in the layout and is not reopened, at start or when the thread is turned on again:
  the user decided so the same day ("se le ho chiuse restano chiuse"), after keeping such tabs
  parked in the layout was proposed (ADR-0005 v4, FRD-0001 R30).
- **Turning a thread off closes it, without deleting it** (user, 2026-11-18: "spegnere un thread
  significa chiuderlo, sì ma non cancellarlo"). This replaces v2's "a session keeps running
  without its tab when its thread is turned off". Turning a thread off (its row, its square, or
  removing its project from the sidebar) stops its running sessions, after asking when one still
  runs ("Turn the thread off?"); cancelled, the thread stays on and a removed project comes back
  to the sidebar. The sessions are neither archived nor forgotten: they stay in the thread, not
  running, and come back as tabs that offer Resume when it is turned on again; nothing of them
  reopens by itself. A thread whose sessions have all ended goes off without asking. A session
  opened into a thread that is off (a collaboration, a hop) still runs without a tab until the
  thread is turned on, and its row says "N running · tabs hidden".
- **Forget is called Delete** in the interface ("Delete thread", "Delete" on a closed session),
  so that the one action that deletes says so; it deletes from Shinatama only, and the agents'
  own files stay. The user proposed "Destroy"; "Delete" was kept because nothing outside
  Shinatama is destroyed. The commands (`thread_forget`) keep their names.
- **Nothing saved.** The tab of a session in which nothing was saved offers Remove, which forgets
  that one session.
- **Forgetting a thread.** A session left marked open by a Shinatama that is gone no longer blocks
  the forget with "a session is running": it is recovered first. "Running, stop it first" is said
  only when a terminal of this window runs a session of the thread or recovery finds the session
  or its owner alive; a session whose owner is gone but cannot be verified gets a sentence of its
  own. These checks come before the thread is turned off, so a refused forget leaves it on.
- **The thread's panel (⋯) lists every session**: name (as on its tab), state in words (running;
  not running with its conversation saved, marked open by a session that is no longer here, or
  nothing saved), and Show (turns the thread on if needed and brings the tab forward), Rename, and
  Remove for a session with nothing saved. Closed sessions follow, with Reopen and Forget as
  before.

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
  stopped, and their output keeps draining. v4: turning a thread off stops its running sessions
  after asking; only a session opened into a thread already off runs parked.
- A session of a thread that is on and is not running is still shown, as a tab that says so and
  offers Resume. v4: one that was running when Shinatama last closed or crashed resumes by itself
  at start, in turn, once the saved layout is back.
- Two different actions, never merged (user, 2026-11-13; Hide removed in v2). **Close** (the tab's
  ×) stops the session, after asking if it runs, and keeps
  it: it leaves the tabs and is listed under "Closed sessions" of its thread, from where it is
  reopened as a tab (not started) or forgotten. **Forget** deletes: one closed session after a
  second click, or the whole thread with every session, closed ones included. Archiving a thread
  deletes nothing. A closed session is the stored record's `archived` mark.
- The sidebar no longer lists sessions as rows of their own, and a click on a thread never
  "focuses a tab": the only two levels are the thread list and the panes. v4: the thread's panel
  (⋯), opened under its row, lists its sessions, and its Show brings one session's tab forward.
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
- **INV-003**: turning a thread on or off starts, resumes and stops no process. v4: turning a
  thread off stops its running sessions, after asking, and deletes nothing. The one exception for
  starting is the start: a tab the saved layout restores as running has its session resumed, one
  at a time and once per run, never one the user stopped or that ended by itself.
- **INV-004**: FRD-0005's invariants (one live activation per session, read-only native stores)
  are unchanged.
- **INV-005**: a workset with a running session cannot be forgotten.

## Examples

- **EX-001** (nominal, v2): threads "Billing refactor" (Claude Code, Codex) and "Docs" (Pi) are on;
  the window shows three tabs, arranged by dragging. Clicking "Docs" turns it off: its tab leaves,
  Pi keeps running; clicking it again brings the tab back. All three agents ran throughout.
  v4: clicking "Docs" asks "Turn the thread off?"; confirmed, Pi stops and its tab leaves;
  clicking it again brings the tab back not running, with Resume.
- **EX-002** (nominal): in "Billing refactor" the user forks the Claude session to Codex; the new
  session appears as a tab of the same thread.
- **EX-003** (nominal): after a restart, "Docs" is on again and shows the Pi tab as not running
  with Resume; nothing started by itself. v4: that holds when the Pi session was not running at
  the close; had it been running, and its agent resumes natively, it would be resumed at start,
  in its tab, after the saved layout is back.
- **EX-006** (nominal, v4): "Billing refactor" holds two Claude Code sessions and one Codex
  session; its tabs read "Claude Code 1 · Billing refactor", "Codex · Billing refactor" and
  "Claude Code 2 · Billing refactor", the same before and after a restart.
- **EX-007** (error, v4): Forget thread on a thread whose session is still marked open by a
  Shinatama that crashed recovers that session first, then forgets; if its former owner is alive,
  the forget is refused and the thread stays on.
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
- v4: `apps/desktop/tests/session-tabs.test.ts` covers the tab names (EX-006), which sessions reopen
  and when, and the recovery before a forget (EX-007); the real-app proof of the reopen is open.
- Register rows are added with the plan.
