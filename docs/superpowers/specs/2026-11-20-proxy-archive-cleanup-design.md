# Model proxy archive: cleaned at every start, close and idle

Design, 2026-11-20. Approved by the owner ("eseguilo"), with the builder check's answers below
as the builder's calls.

## Intent

Owner, 2026-11-20, after sessions took a minute or more to open: "è troppo troppo lento ad avviare
una sessione", "si fa prima a usare terminale di windows con comando claude", then, on the proxy's
archive: "è anomalo che pesi così tanto" and "la pulizia è obbligatoria! sia al forget di un thread,
sia al close forget di una session, sia come task a ogni avvio o chiusura o in caso di idle". What
is removed, in the owner's words: "Solo i subagent finiti, più i thread dimenticati."

The proxy must start in about a second, and its archive must hold only what an existing thread
can still use.

## What happened (the case this design answers)

- The proxy (billion-context acp-proxy v0.1.207, the pinned sidecar source build) keeps every
  message of every session and subagent it has ever served in one SQLite file,
  `data\sidecar\data\sessions\index.db`. On the owner's install it was 2,336 MB: 530 sessions,
  383,146 content rows, 386,827 messages (307,541 of them retired).
- The proxy runs `PRAGMA quick_check` on that file at every start. It measured 82.8 s for it on
  2026-11-20; `shinatama.log` has `proxy: Ready 100925 ms after launch`. Every open that needs the
  proxy waits for this, and it is paid again after each proxy exit (code 3221226505 four times on
  2026-11-19 and 20).
- The proxy's own cleanup is off by default (`sessions.gc`), its age sweep archives after 365 days,
  and Shinatama never asks for either. Nothing has ever been removed.
- By the owner's rule 43 of the 530 sessions stay (the main sessions of existing threads, about
  6 MB of messages) and 487 go. A count earlier the same day, on 521 sessions: 371 subagents with
  1,340 MB of messages, 111 main sessions no thread refers to with 211 MB.

## Trial on a copy (2026-11-20)

A consistent snapshot of the archive (SQLite backup API, read-only on the original) was cleaned in
the scratch folder with the proxy's own commands, run with the pinned Node and the environment
Shinatama gives the proxy:

| Step | Result |
|---|---|
| `sessions rm <id> --no-vacuum --json`, 487 sessions, one process each | 1,396 s (2.9 s each), 1,642 MB released, 0 failures |
| `storage gc --vacuum full` | 1 s; file 2,451 MB → 18.7 MB |
| open with the start-up check on, 43 sessions left | 0.9 s |
| `sessions ls --json` before the cleanup (loads every session) | 32.5 s |

## Goals

1. After a cleanup the archive holds only the main sessions of existing threads.
2. A proxy start does not wait on a large archive: the start-up check stays on and takes about a
   second because the file is small.
3. Cleanup happens without the owner asking: at start, at close, when idle, and after a forget.
4. A cleanup never delays an open by more than a few seconds and never loses a session a thread
   still has.

## Non-goals

- Changing the proxy (the pinned source build) or its configuration of retention.
- The subagent transcript files of the harnesses (`projects\<project>\<session>\subagents`): they
  are the harness's data. Their weight on the session reader is a separate fix.
- Why the proxy exits with 3221226505.
- Switching the start-up check off.
- Keeping the proxy state of Pi threads across a cleanup (see the builder check on Pi's id).

## Anti-goals

- Two processes writing `index.db`. The proxy's own source says it corrupts the file
  (`storage/store.ts`, "two processes on one index.db corrupt it", #3923/#3959) and it does not
  block the second one. Shinatama must make it impossible.
- Removing the proxy state of a session that is open, or of a thread that still exists (an
  archived thread exists).
- An open, or the close of the app, that hangs on a cleanup.

## Constraints

- The archive is third-party state. It is changed only through the proxy's own commands; Shinatama
  writes no SQL to it.
- The proxy has no per-session delete on its HTTP admin surface, only `POST /__bili/storage/gc`
  (age sweep and vacuum). Per-session removal exists only as the offline command
  `bili sessions rm <id>`, one id per process.
- Offline commands run only while the proxy process is not running (anti-goal 1).
- A proxy session id is the harness's native session id; a subagent is `<native id>|sub:<agent>`.

## Approaches

1. **Shinatama runs the proxy's own offline commands while the proxy is stopped** (chosen). Proven
   on the copy above. Uses only what the proxy ships; slow per session (2.9 s) because each
   removal is a process.
2. Shinatama deletes rows in `index.db` itself. Fast, but it writes a third party's schema
   (content is reference-counted across sessions) and breaks at the next proxy version. Rejected.
3. Turn on the proxy's own retention (`sessions.gc`, a shorter `archiveAfterDays`). It works by age
   only: it cannot tell a forgotten thread from a live one, and archiving exports to
   `sessions\archive` instead of removing. It does not give the owner's rule. Rejected as the
   mechanism; nothing prevents adding it later.

The owner states outcomes and left the mechanism to the builder; approach 1 is the builder's call.

## Design

### What is removed

At the moment of a cleanup, with the proxy stopped (so no session is running and every subagent
is finished):

- every Claude subagent session: protocol `anthropic` and an id ending in `|sub:a` followed by
  16 hex digits (the agent id Claude Code gives a subagent);
- every proxy session whose id, up to `|`, is not the `native_id` of a segment of an existing
  thread in Shinatama's database (an archived thread exists).

Everything else stays. Any other `|sub:` session stays with its root: the proxy also forks a
*live* conversation onto `<id>|sub:<16 hex of the instructions>` when a Codex or
Claude-over-Responses client changes its instructions (`persona-anchor.ts:141`,
`server/lane-decisions.ts:113,120`), and later requests follow that fork; it is not a finished
subagent. On the owner's archive all 375 `|sub:` sessions are Claude subagents. There is no queue of ids to remove: the rule is recomputed from the two
stores each time, so a forgotten thread's sessions become removable by the forget itself, and a
cleanup that is interrupted loses nothing.

The proxy's session ids are read from `index.db` opened read-only (`SELECT session_id FROM
sessions`), because `sessions ls` loads every session (32.5 s on the large file). This reads one
column of a third party's schema: when the read fails, the cleanup falls back to `sessions ls
--json`. (Builder's call.)

### How it is removed

One maintenance run, owned by the sidecar controller (`apps/desktop/src-tauri/src/sidecar.rs`),
with the mechanism in the engine beside `sidecar_runtime.rs`:

1. Hold the sidecar owner lock (`sidecar.owner.lock`) and confirm the proxy process is not
   running. The controller does not start the proxy while a run holds the lock; a start request
   that arrives during a run stops the run after the removal in progress (at most a few seconds)
   and then starts the proxy.
2. For each id to remove: `node dist/index.js sessions rm <id> --no-vacuum --json`, with the
   proxy's prepared environment plus `BILI_STORAGE_QUICK_CHECK=0` (the proxy's own start still
   checks). The JSON reply gives rows and bytes released.
3. When at least one session was removed and the run was not interrupted:
   `storage gc --vacuum full --json` with an age far in the future, so nothing is archived by age.
4. Each run has a time budget; what is left is done by the next run.

### When it runs

| Moment | Budget | Notes |
|---|---|---|
| App start, before the proxy is launched | 10 s | Then the proxy starts whatever is left. |
| App close, after the proxy has stopped | 10 s | Inside the existing close worker (`terminal_shutdown`), which already stops the proxy while the window is still shown; the same process holds the owner lock until it exits, so a Shinatama started meanwhile cannot start the proxy on the archive. |
| Idle: no agent session open for 10 minutes and something to remove | until a session is asked for | The proxy is stopped for the run and started again right after it, so the next open does not pay a start. |
| Forget of a thread, or close and forget of a session | none of its own | It changes what the rule selects. When no agent session is open, the idle run starts at once instead of after 10 minutes. |

### The first run on a large archive

The owner's archive is cleaned once by hand, on 2026-11-20, with the rule above and the proxy's
own commands, while Shinatama is closed (a script that waits for Shinatama and its proxy to exit
and holds the proxy owner lock while it runs). From then on the runs below keep it small, so no
run has a large backlog. An archive that is large for another reason is worked off in the
budgeted slices.

### What the owner sees

- `shinatama.log`: one line per run, `proxy-archive: removed N sessions, M MB, in S s; K left;
  file now X MB` or the reason it did not run or stopped.
- The Model proxy panel: the archive's size and the last cleanup, and the first-run offer above.
- A removal that fails leaves that session and is logged with the proxy's error; the run goes on.

## Sessions kept only as legacy files (found 2026-11-20, on the owner's archive)

The proxy also keeps sessions as JSON files (`sessions/anthropic/`, `sessions/responses/`; older
ones with `storageMode: legacy`), and a session that exists only as such a file has no row in
`sessions` until a proxy load imports it (`persist.ts` `boot()`/`loadAll()`, `buildForLoadAll`).
The rule reads only the table, so such a session is not seen by a run before the proxy's first
start; after it, it is. `sessions rm` removes both the row and the file (`findSessionFiles`), and a
later load does not bring it back: proven on a copy (one legacy session: imported by a load,
`filesRemoved: 1`, absent after another load). On the owner's archive, 57 such sessions (52 Claude
subagents, 5 of no thread, files of 2026-11-18) came back after the hand-run cleanup's final
`sessions ls`; the next run after a proxy start removes them.

## Builder check (2026-11-20), answered

- *Is every `|sub:` a finished subagent?* No: see "What is removed"; only Claude subagents are.
- *Is the proxy id the native id for every harness?* Claude Code sends its session id
  (`x-claude-code-session-id`, `session-id.ts`) and OpenCode 2.x its own
  (`x-opencode-session-id`); on the owner's install 5 of 5 Codex segments and 38 Claude segments
  have a proxy session of the same id. Pi (1.4.0, the installed build) sends its session id to
  the proxy only when the model's `compat.sendSessionAffinityHeaders` is on
  (`x-session-affinity`, or `x-session-id` in the OpenRouter format; `pi-ai`
  `anthropic-messages.js`), and the provider extension Shinatama writes for Pi
  (`crates/engine/src/model_config.rs`) does not turn it on. So a Pi thread's proxy session is
  keyed by the proxy's own id, and each cleanup removes it as a session of no thread: the proxy
  then replays that thread's history raw at its next request (the conversation in Pi's own store
  is untouched). The owner has no Pi thread today. Turning the compat flag on in that extension,
  so that Pi sends the native id Shinatama gives it with `--session-id`, is a separate change.
- *After an idle run, does the proxy stay stopped?* No, it is started again (table above).
- *A Shinatama reopened during a close run?* The close run happens in the closing process, before
  it exits and while it still holds the owner lock (table above); the new one reports that another
  window owns the proxy, as it does today during any close.
- *First run?* By hand (section above).
- *The age sweep:* `storage gc --days 36500` (proven on the copy) archives nothing by age.
- *The owner lock* belongs to the supervisor's provider for the process lifetime
  (`sidecar.rs:843,901-904`), and the supervisor restarts the proxy by itself
  (`sidecar_supervisor.rs:33,264`): a run is a state of the supervisor worker, between a stop and
  the next start, not a second owner.
- *Forget* has two callers, the desktop (`threads.rs:514`) and the CLI (`crates/cli/src/sessions.rs:435`);
  both delete the segments, so the recomputed rule covers both. A forget from the CLI is cleaned
  at the next run of the desktop.

## Tests

- Engine, unit: the rule (ids with and without `|sub:`, ids of existing, archived and forgotten
  threads, a Codex id), the budget loop and the interruption with a fake command runner.
- Engine, with the real pinned proxy (as `crates/engine/tests/sidecar_supervisor.rs` does): a
  small synthetic archive, a run, and the proxy starting on the result.
- The controller: no proxy start while a run holds the owner lock; a start request ends the run.
- On the owner's data: the run on a fresh copy of the archive before the first real one.

All Rust is compiled and tested on GitHub Actions only.

## Left to the builder

- The exact place of the budgets and of the idle timer, and the wording in the panel.
- Whether the read-only id query or `sessions ls` is the default once the file is small.
