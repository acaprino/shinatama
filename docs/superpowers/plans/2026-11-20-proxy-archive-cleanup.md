# Model proxy archive cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Shinatama removes the proxy state that no existing thread can use (Claude subagents and
sessions of no thread) at every start, close and idle, so the proxy's archive stays small and its
start-up check takes about a second.

**Architecture:** A pure engine module decides what to remove and drives the proxy's own offline
commands through an injected executor. The proxy supervisor's worker, which already owns the only
right to run the proxy (it holds the owner lock through its provider), runs it while no proxy
process is running: before the first launch, on a new `Command::Maintain`, and at close. The
desktop sends `Maintain` when idle, after a forget, and in its close worker, and logs each run.

**Tech Stack:** Rust (engine, state, Tauri desktop), rusqlite read-only, the pinned Node and proxy
bundle (`resources/sidecar`), TypeScript panel.

**Spec:** `docs/superpowers/specs/2026-11-20-proxy-archive-cleanup-design.md`

## Global Constraints

- Never run `cargo` (build, test, clippy, check, run, doc) on this machine; `cargo fmt` is the only
  exception. A task's tests are run by pushing to `develop` and reading the run: download the
  artifact `verification-windows-<sha>` (`gh run download <id> --pattern "verification-windows-*"`)
  and find each new test as `test <path>::<name> ... ok` in `.verification/logs/rust-tests.stdout.log`.
  The check `rust-tests` is red at baseline for 2 unrelated tests; judge by the new tests' lines.
- Shinatama writes nothing to the proxy archive itself. It reads only
  `SELECT session_id, protocol FROM sessions` from `index.db`, opened read-only
  (`file:<path>?mode=ro`, URI). Every change goes through
  `node <entry> sessions rm <id> --no-vacuum --json` and
  `node <entry> storage gc --days 36500 --vacuum full --json`.
- No maintenance command runs while a proxy process of this owner is running, and none is run by
  anything but the supervisor's worker (two writers corrupt `index.db`: proxy `storage/store.ts`).
- Maintenance commands get the proxy's own environment (`recipe.child_environment()`) plus
  `BILI_STORAGE_QUICK_CHECK=0`; the proxy's own start keeps its check.
- The rule removes: protocol `anthropic` with an id matching `\|sub:a[0-9a-f]{16}$`; and any id
  whose part before the first `|` is not the `native_id` of a segment. Nothing else.
- A failure to read Shinatama's segments or the archive removes nothing.
- Budgets: 10 s before the first launch; 10 s at close; idle runs until an open asks for the proxy.
  Idle means no live agent session for 10 minutes. Per-command timeout 120 s.
- English copy in logs and UI, as the rest of the app. Commits end with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Work on `develop` only.

## Review Focus

1. An open arrives during a run: the run stops after the command in progress and the proxy starts.
   Test in Task 2 (`a_start_request_ends_the_run_after_the_command_in_progress`).
2. Shinatama's segments cannot be read (database busy or failing): nothing is removed, the proxy
   starts. Test in Task 2 (`unreadable_segments_remove_nothing`).
3. No archive yet (first start) or an unreadable one: nothing runs, no error reaches the user.
   Test in Task 2 (`a_missing_archive_runs_nothing`).
4. One removal fails or prints something that is not JSON: that id stays, the rest go, compaction
   still runs. Test in Task 2 (`a_failed_removal_is_counted_and_the_run_goes_on`).
5. A close with a long backlog: the close waits no more than the budget plus the command in
   progress. Test in Task 2 (`the_budget_bounds_the_run`).

---

### Task 1: The native ids of all segments

**Files:**
- Modify: `crates/state/src/threads.rs` (beside `segment_record`, ~line 876)
- Test: `crates/state/tests/database.rs` (with the other thread tests)

**Interfaces:**
- Produces: `impl Database { pub fn segment_native_ids(&self) -> Result<std::collections::HashSet<String>> }`
  — every `segment.native_id`, of active and archived threads alike.

- [ ] **Step 1: Write the test** `segment_native_ids_names_every_segment_of_every_existing_thread`:
  two threads with one segment each, archive one, forget a third with `forget_thread`; assert the
  set holds exactly the two native ids of the threads that still exist.
- [ ] **Step 2: Implement** with one `SELECT native_id FROM segment` on the read connection.
- [ ] **Step 3: Commit** `feat(state): the native ids of every segment`.

### Task 2: The archive cleanup, as a pure engine module

**Files:**
- Create: `crates/engine/src/sidecar_archive.rs`
- Modify: `crates/engine/src/lib.rs` (`pub mod sidecar_archive;` beside `sidecar_runtime`)
- Modify: `crates/engine/Cargo.toml` (`rusqlite.workspace = true` under `[dependencies]`)

**Interfaces:**
- Consumes: `SidecarRuntimeRecipe::{executable, arguments, working_directory, child_environment}`
  (`arguments()[0]` is the proxy's entry script), `resources::process::NativeCommand::new`.
- Produces:
  - `pub struct ArchiveSession { pub id: String, pub protocol: String }`
  - `pub fn removable(sessions: &[ArchiveSession], natives: &HashSet<String>) -> Vec<String>`
  - `pub fn index_path(recipe: &SidecarRuntimeRecipe) -> Option<PathBuf>` —
    `BILI_SESSIONS_DIR` of the recipe's environment joined with `index.db`.
  - `pub fn archive_sessions(index: &Path) -> Result<Vec<ArchiveSession>, ArchiveError>`
  - `pub fn maintenance_command(recipe: &SidecarRuntimeRecipe, arguments: &[&str]) -> Result<NativeCommand, ArchiveError>`
  - `pub struct CommandOutput { pub code: Option<u32>, pub stdout: Vec<u8> }` (stdout kept up to 64 KiB)
  - `pub enum Stop { Done, Budget, Cancelled, Unreadable }`
  - `pub struct Report { pub removed: usize, pub failed: usize, pub left: usize, pub released_bytes: u64, pub compacted: bool, pub file_bytes_before: Option<u64>, pub file_bytes_after: Option<u64>, pub elapsed: Duration, pub stop: Stop }`
  - `pub fn run(recipe: &SidecarRuntimeRecipe, natives: Result<HashSet<String>, ()>, deadline: Option<Instant>, cancelled: &dyn Fn() -> bool, execute: &mut dyn FnMut(NativeCommand) -> Result<CommandOutput, ()>) -> Report`
    (`deadline: None` = no budget, the idle run).

Since `run` needs a recipe and `SidecarRuntimeRecipe` has no public constructor, the tests build
the rule's input directly and test `run` through a private
`fn run_on(index: Option<&Path>, natives, deadline, cancelled, command: &dyn Fn(&[&str]) -> NativeCommand, execute) -> Report`
that `run` calls; the tests write a small SQLite file with a `sessions(session_id, protocol)`
table as the archive.

- [ ] **Step 1: Write the rule tests** in the module's `#[cfg(test)] mod tests`:
  - `claude_subagents_and_sessions_of_no_thread_are_removed`: natives `{"c1", "x1"}`; sessions
    `c1` anthropic, `c1|sub:a0123456789abcdef` anthropic, `gone` anthropic,
    `gone|sub:a0123456789abcdef` anthropic, `x1` responses, `x1|sub:0123456789abcdef` responses
    (a live persona fork), `y2` responses, `y2|sub:0123456789abcdef` responses. Expected removed, in
    order: `c1|sub:a0123456789abcdef`, `gone`, `gone|sub:a0123456789abcdef`, `y2`,
    `y2|sub:0123456789abcdef`.
  - `a_fork_of_an_existing_thread_is_kept_whatever_its_protocol`: `c1|sub:0123456789abcdef`
    anthropic (16 hex, no `a`) with `c1` a native → kept.
- [ ] **Step 2: Write the run tests** with a fake `execute` that records argument lists and replies
  `{"dbBytesReleased":100}` to `sessions rm`, `{}` to `storage gc`:
  - `a_run_removes_each_id_then_compacts_once`: 3 removable → 3 `sessions rm <id> --no-vacuum --json`
    then 1 `storage gc --days 36500 --vacuum full --json`; `removed == 3`, `released_bytes == 300`,
    `compacted`, `stop == Done`.
  - `nothing_to_remove_runs_nothing`: no command executed, `stop == Done`, not compacted.
  - `a_start_request_ends_the_run_after_the_command_in_progress`: `cancelled` turns true after the
    first command → exactly 1 command, no compaction, `left == 2`, `stop == Cancelled`.
  - `unreadable_segments_remove_nothing`: `natives = Err(())` → no command, `stop == Unreadable`.
  - `a_missing_archive_runs_nothing`: index path that does not exist → no command, `stop == Unreadable`.
  - `a_failed_removal_is_counted_and_the_run_goes_on`: the second `rm` returns `Err(())`, the third
    prints `not json` → `removed == 1`, `failed == 2`, compaction runs.
  - `the_budget_bounds_the_run`: `execute` sleeps 40 ms, deadline now + 100 ms, 10 removable →
    at most 3 `rm`, `stop == Budget`, compaction runs because `removed > 0` (a compaction after the
    budget is one command on a small file).
- [ ] **Step 3: Implement** the module. `removable` keeps archive order. `archive_sessions` opens
  with `OpenFlags::SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI | SQLITE_OPEN_NO_MUTEX` and a 2 s busy
  timeout. A reply of `sessions rm` counts as removed when it is a JSON object without
  `"found": false`; `found: false` (already gone) counts as removed with 0 bytes. File sizes come
  from `fs::metadata(index)`.
- [ ] **Step 4: Commit** `feat(engine): what the proxy archive keeps, and a run of its own commands`,
  push, and read the nine tests' lines in the run (Global Constraints).

### Task 3: The supervisor runs the cleanup while no proxy runs

**Files:**
- Modify: `crates/engine/src/sidecar_supervisor.rs`
- Modify: `apps/desktop/src-tauri/src/sidecar.rs` (`phase_name`, `SupervisorConfig` literal)
- Modify: `crates/engine/tests/sidecar_supervisor.rs` (its `SupervisorConfig` literals get the new fields)

**Interfaces:**
- Consumes: Task 1 `segment_native_ids`, Task 2 `run`, `index_path`, `Report`.
- Produces:
  - `Command::Maintain { budget: Option<Duration>, restart: bool }` (`budget: None` = until a start
    is asked for).
  - `Phase::Maintaining` (serialized `"maintaining"`); `Phase::backs_launch` stays false for it.
  - `SupervisorConfig { …, pub maintenance_at_start: Duration, pub maintenance_command_timeout: Duration }`,
    valid when `maintenance_at_start <= 60 s` and `maintenance_command_timeout` is within 10–300 s.
  - `WorkloadProvider::maintained(&mut self, _report: &sidecar_archive::Report) {}` (default no-op).

- [ ] **Step 1: Write the tests** in the supervisor's unit tests:
  - `maintaining_never_backs_a_launch`: `Phase::Maintaining.backs_launch() == false`, and the
    serde name is `"maintaining"`.
  - `a_start_request_marks_a_running_maintenance_for_cancel`: `SidecarSupervisor::request(Command::Start)`
    and `Command::Retry` set the shared start-wanted flag that the run's `cancelled` reads (expose it
    to tests as `pub(crate) fn start_wanted(&self) -> bool`).
  - Extend the config validation test: `maintenance_command_timeout` of 5 s or 301 s is refused.
- [ ] **Step 2: Implement** in the worker:
  - One `fn maintain(&mut self, budget: Option<Duration>) -> Report`: returns at once with
    `Stop::Done` when a process is running (`self.running.is_some()`) or cleanup is retained
    (`self.retained.is_some()`); otherwise publishes `Maintaining`, calls `sidecar_archive::run` with
    `self.provider.database().segment_native_ids().map_err(|_| ())`, the deadline from `budget`,
    `cancelled` = the start-wanted flag, and an executor that runs each command exactly as `launch`
    runs the proxy (`provider.prepare`, `spawn_contained`, without `high_priority()`), reads stdout
    into a 64 KiB buffer and stderr through the existing drain until exit or
    `maintenance_command_timeout`, then `finish_cleanup`; a cleanup that is not `Released` is
    retained like the proxy's and ends the run. Then `self.provider.maintained(&report)` and
    publishes `Stopped`.
  - Before the first `launch()` of this worker (a `maintained_at_start: bool` field):
    `maintain(Some(config.maintenance_at_start))`.
  - On `Command::Maintain { budget, restart }`: when a process runs and `in_flight == Some(0)`,
    stop it as `Command::Stop` does (`self.stop(true)`); when requests are in flight, do nothing.
    Then `maintain(budget)`, and `requested_start = restart || start_wanted`. Clear the flag after
    each run.
- [ ] **Step 3: Implement** `phase_name(Phase::Maintaining) => "maintaining"` and its panel text
  `"cleaning its archive"` in `apps/desktop/src-tauri/src/sidecar.rs` (`phase_name`, ~line 102, and
  the text match at ~line 135). Opens keep waiting in `maintaining`: do not add it to the terminal
  phases at `apps/desktop/src-tauri/src/agents/service.rs:371` or `sidecar.rs:308`. The desktop's
  `SupervisorConfig` gets `maintenance_at_start: 10 s`, `maintenance_command_timeout: 120 s`.
- [ ] **Step 4: Commit** `feat(engine): the proxy owner cleans the archive while no proxy runs`,
  push, read the tests' lines and that `desktop-build` passed.

### Task 4: The desktop asks for a run when idle, after a forget and at close, and logs it

**Files:**
- Modify: `apps/desktop/src-tauri/src/terminal_runtime.rs` (`SidecarProvider`, ~line 337)
- Modify: `apps/desktop/src-tauri/src/sidecar.rs` (`Controller`, `Status`)
- Modify: `apps/desktop/src-tauri/src/main.rs` (`terminal_shutdown`, ~line 381)
- Modify: `apps/desktop/src-tauri/src/threads.rs` (after a successful forget, ~line 514)

**Interfaces:**
- Consumes: Task 3 `Command::Maintain`, `WorkloadProvider::maintained`, `Report`.
- Produces:
  - `Controller::maintain(&self, budget: Option<Duration>, restart: bool) -> Result<(), &'static str>`
  - `Controller::maintain_soon(&self)` — makes the next idle check due now.
  - `Controller::maintain_and_wait(&self, budget: Duration)` — for the close worker; returns when
    the owner reports `Stopped` or after `budget + 5 s`.
  - `fn idle_run_due(live_agent_sessions: Option<usize>, idle_since: Option<Instant>, now: Instant, due_now: bool) -> bool`
    (pure): true when sessions are `Some(0)` and either `due_now` or idle for at least 600 s.
  - `Status.archive: Option<ArchiveStatus>` with
    `ArchiveStatus { file_bytes: Option<u64>, last_run_unix_ms: Option<i64>, last_removed: usize, last_left: usize }`
    (serde camelCase).

- [ ] **Step 1: Write the tests** in `sidecar.rs`'s tests:
  - `idle_runs_after_ten_minutes_without_live_sessions`: `Some(0)`, idle 599 s → false; 600 s → true;
    `Some(1)` at 900 s → false; `None` (unknown) at 900 s → false; `Some(0)`, idle 1 s, `due_now` → true.
  - `the_log_line_says_what_a_run_did`: `report_line(&Report{removed: 487, released_bytes: 1_642_000_000, left: 0, file_bytes_after: Some(18_731_008), elapsed: 1396 s, stop: Done, ..})`
    == `"removed 487 sessions (1642 MB) in 1396 s; 0 left; archive now 18.7 MB."`; with
    `stop: Cancelled` it ends `"; stopped for an open."`, with `Unreadable`
    `"nothing removed: the archive or Shinatama's threads could not be read."`.
- [ ] **Step 2: Implement** `SidecarProvider::maintained`: when `removed + failed > 0` or the stop is
  not `Done`, `crate::app_log::write("proxy-archive", &report_line(report))`, and store the
  `ArchiveStatus` the controller's `status()` returns.
- [ ] **Step 3: Implement** the idle watcher: one thread started with the supervisor, every 60 s
  reads `live_agent_sessions()` (as `main.rs:567` does) and the phase; when `idle_run_due` and the
  phase is `ready`, sends `maintain(None, true)` once per idle period (reset `idle_since` when a
  session goes live).
- [ ] **Step 4: Implement** the triggers: in `threads.rs` after `forget_thread` returns `Ok`, call
  `maintain_soon()` on the managed `Arc<sidecar::Controller>` when present. In `terminal_shutdown`,
  after `sidecar.stop_and_wait(6 s)`, call `sidecar.maintain_and_wait(Duration::from_secs(10))`.
- [ ] **Step 5: Commit** `feat(desktop): the proxy archive is cleaned when idle, after a forget and at close`,
  push, read the tests' lines and `desktop-build`.

### Task 5: The panel shows the archive

**Files:**
- Modify: `apps/desktop/src/sidecar-status.ts` (status type and the panel's details)
- Test: the existing tests of `sidecar-status.ts` (`npm test`, which may run locally)

**Interfaces:**
- Consumes: Task 4 `Status.archive` (`fileBytes`, `lastRunUnixMs`, `lastRemoved`, `lastLeft`), phase `"maintaining"`.

- [ ] **Step 1: Write the test** `the_panel_names_the_archive_size_and_the_last_cleanup`: archive
  `{fileBytes: 18731008, lastRunUnixMs: <fixed>, lastRemoved: 487, lastLeft: 0}` renders
  `"Archive 18.7 MB · last cleanup removed 487 sessions"` (with the time as the panel shows other
  times); `archive: null` renders no archive line; phase `"maintaining"` renders
  `"Cleaning its archive"` with no Start button.
- [ ] **Step 2: Run** `npm test` → the new test fails; **implement**; `npm test` → passes.
- [ ] **Step 3: Commit** `feat(panel): the proxy panel shows its archive and the last cleanup`.

### Task 6: Pi's proxy session id

**Files:**
- Modify: `docs/superpowers/specs/2026-11-20-proxy-archive-cleanup-design.md` (builder check answer)

- [ ] **Step 1:** Find which header the pinned Pi build sends with its model requests (its package
  under the Pi home Shinatama installs, or its published source for the pinned version) and match
  it against `sidecar/source-build/prepared/src/session-id.ts`'s header list.
- [ ] **Step 2:** Write the finding in the spec. When Pi's proxy id is not its native id, add to the
  spec's Non-goals that a Pi thread's proxy state is rebuilt after a cleanup, and tell the owner.
- [ ] **Step 3: Commit** `docs(spec): Pi's proxy session id`.

### Task 7: Proof on the owner's data

- [ ] **Step 1:** After Tasks 1–5 pass in CI, the owner opens the pull request `develop` → `main`
  (a release); the builder never merges it.
- [ ] **Step 2:** With the released build, read `shinatama.log` after a start, an idle period and a
  close: a `proxy-archive:` line for each run that removed something, `proxy: Ready` within a few
  seconds of launch, and `index.db` under 50 MB.
