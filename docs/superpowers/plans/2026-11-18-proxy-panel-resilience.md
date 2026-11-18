# Model proxy panel resilience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Model proxy panel always has a working action and every proxy-related refusal names its cause (stall, internal error, full disk).

**Architecture:** The engine supervisor contains panics in its own loop and reports them as a Failed state. The desktop `sidecar::Controller` derives a `stalled` flag and a disk reading (from a background sampler) into the status it already serves. It also composes one refusal sentence that the agent service substitutes for its old fixed proxy messages. The panel renders the new fields from pure, unit-tested functions and offers Restart Shinatama. Restart reuses the existing close sequence and a relauncher that waits for the old process to exit.

**Tech Stack:** Rust (Tauri v2, windows-sys), TypeScript (node:test with `--experimental-strip-types`).

**Spec:** `docs/superpowers/specs/2026-11-18-proxy-panel-resilience-design.md`

## Global Constraints

- Branch `fix/proxy-panel-resilience` from `develop`; at the end merge into `develop` with `--no-ff` and delete the branch (AGENTS.md). No push unless the owner asks.
- Never create backups of `D:\Software\Shinatama`; update the install by copying the built executables over it (AGENTS.md).
- Never change versions or tags (release workflow owns them).
- Stall threshold: 15 s without a supervisor observation. Disk: low under 1 GiB (1 073 741 824 bytes), full under the database's save floor, 64 MiB + 1 MiB = 68 157 440 bytes. Sample every 10 s.
- `Controller::status()` does no file or network I/O on the caller's thread; log writes it triggers run on a spawned thread.
- `shinatama.log` lines stay one bounded, control-free line (`app_log::write`).
- No sentence may claim saved data is preserved.
- Before any `cargo` command, check `df -B1M /d`: stop and ask the owner if D: has less than 8 GB free.

## Review Focus

- The supervisor reports Failed and the panel shows the reason. The user expects Start and Retry recovery enabled, and no stall line or Restart button: the reply is fresh. Test in Task 6 (`restartOffered` false when `stalled` is false, even when `phase` is `failed`).
- `observedAtUnixMs` lies in the future (the clock was moved back). The user expects no stall. Test in Task 3 (`stalled(Some(now + 5_000), now, Phase::Ready) == false`).
- The disk reading is still unknown (the first sample is pending, or the path fails). The user expects no disk line and no false "full". Test in Task 2 (`DiskWatch` returns `None` before the first sample) and Task 6 (`diskLine` null when `disk` is absent).
- The relauncher starts, but the user then declines the "lose unsaved preferences" question, so the old window stays open. The user expects no second Shinatama. Test in Task 5 (`wait_for_exit` returns false on timeout, and `main` returns without starting).
- The status reply carries a malformed new field (for example `disk.level: "huge"` or a `fault` with a newline). The user expects the whole reply rejected, as today. Test in Task 6.

---

### Task 1: Engine supervisor contains panics

**Files:**
- Modify: `crates/engine/src/sidecar_supervisor.rs` (enum `Reason` ~67, struct `Snapshot` ~132, `start()` ~233, `Worker::run` ~363-444, `launch()` ~494, `Command::Retry` arm ~399)
- Test: same file, `mod tests` (~1601)

**Interfaces:**
- Produces: `Reason::SupervisorFault` (serde `"supervisor_fault"`); `Snapshot.fault: Option<String>`; `pub(crate) fn panic_text(payload: &(dyn std::any::Any + Send)) -> String`; `fn fault_snapshot(snapshot: &mut Snapshot, text: String)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_panic_payload_becomes_one_bounded_line() {
    assert_eq!(panic_text(&"boom"), "boom");
    assert_eq!(panic_text(&String::from("disk\nfull\u{7}")), "disk full");
    assert_eq!(panic_text(&42_u8), "unknown panic payload");
    assert_eq!(panic_text(&"x".repeat(900)).chars().count(), 300);
}
#[test]
fn a_fault_is_a_failed_snapshot_that_says_why() {
    let mut snapshot = launch_snapshot(Phase::Recovering, true, 1);
    fault_snapshot(&mut snapshot, "boom".into());
    assert_eq!((snapshot.phase, snapshot.reason, snapshot.healthy), (Phase::Failed, Reason::SupervisorFault, false));
    assert_eq!(snapshot.fault.as_deref(), Some("boom"));
    assert_eq!(serde_json::to_value(Reason::SupervisorFault).unwrap(), "supervisor_fault");
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --locked -p shinatama-engine --lib sidecar_supervisor::tests::a_ -- --nocapture`
Expected: compile error, because `panic_text`, `fault_snapshot` and `SupervisorFault` are not defined.

- [ ] **Step 3: Implement**

- Add `fault: None` everywhere a `Snapshot` is built: `start()`, `launch_snapshot` in tests, and any other literal.
- `panic_text`: a `&str` or `String` payload gives its text. Control characters become spaces, runs of spaces collapse, the text is trimmed and cut at 300 chars. Any other payload gives `"unknown panic payload"`.
- `Worker::run`: move the current loop body into `fn turn(&mut self) -> bool` (false ends the thread where `run` returned before). Then:
  `loop { match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.turn())) { Ok(true) => {} Ok(false) => return, Err(payload) => self.fault(panic_text(&*payload)) } }`.
- `fn fault(&mut self, text: String)`: set `requested_start = false`, call `recovery.cancel()`, call `fault_snapshot`, then `publish(Phase::Failed, Reason::SupervisorFault)`.
- Clear `snapshot.fault` in `launch()` and in the `Command::Retry` arm.

- [ ] **Step 4: Run the tests and see them pass**

Run: `cargo test --locked -p shinatama-engine --lib sidecar_supervisor`
Expected: all pass, including the existing ones.

- [ ] **Step 5: Commit** — `fix(proxy): a panic in the supervisor becomes a Failed state that says why`

### Task 2: Free disk space, sampled off the UI thread

**Files:**
- Modify: `crates/state/src/maintenance.rs:30-76` (export the floor and `free_bytes`)
- Create: `apps/desktop/src-tauri/src/disk_space.rs` (declare `mod disk_space;` in `main.rs`)
- Test: `crates/state/src/maintenance.rs` tests, `disk_space.rs` `mod tests`

**Interfaces:**
- Produces (state): `pub const SAVE_FLOOR_BYTES: u64` (= `MIN_FREE_BYTES + WRITE_RESERVE_BYTES`), `pub fn free_bytes(directory: &Path) -> Result<u64>` (the existing private function, made public; `admit_write` uses the constant).
- Produces (desktop): `pub(crate) const LOW_BYTES: u64 = 1 << 30`; `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub(crate) enum DiskLevel { Ok, Low, Full }`; `pub(crate) fn level(free_bytes: u64) -> DiskLevel`; `pub(crate) fn root_of(path: &Path) -> String`; `pub(crate) fn line(root: &str, free_bytes: u64) -> Option<String>`; `pub(crate) struct DiskWatch` with `pub(crate) fn start(directory: PathBuf) -> Arc<Self>`, `pub(crate) fn free_bytes(&self) -> Option<u64>`, `pub(crate) fn root(&self) -> &str`.

- [ ] **Step 1: Write the failing tests**

```rust
// maintenance.rs
#[test]
fn the_save_floor_is_the_minimum_plus_one_write() {
    assert_eq!(SAVE_FLOOR_BYTES, 65 * 1024 * 1024);
    assert!(free_bytes(&std::env::temp_dir()).unwrap() > 0);
}
// disk_space.rs
#[test]
fn levels_follow_the_save_floor_and_one_gigabyte() {
    use shinatama_state::maintenance::SAVE_FLOOR_BYTES as F;
    assert_eq!(level(0), DiskLevel::Full);
    assert_eq!(level(F - 1), DiskLevel::Full);
    assert_eq!(level(F), DiskLevel::Low);
    assert_eq!(level(LOW_BYTES - 1), DiskLevel::Low);
    assert_eq!(level(LOW_BYTES), DiskLevel::Ok);
}
#[test]
fn lines_name_the_disk_and_its_free_megabytes() {
    const MIB: u64 = 1024 * 1024;
    assert_eq!(root_of(Path::new(r"D:\Software\Shinatama\data")), "D:");
    assert_eq!(line("D:", 5 * MIB).as_deref(), Some("Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama."));
    assert_eq!(line("D:", 512 * MIB).as_deref(), Some("Disk D: has 512 MB free."));
    assert_eq!(line("D:", 2 * LOW_BYTES), None);
}
#[test]
fn a_watch_is_unknown_until_its_first_sample() {
    let watch = DiskWatch::unsampled(PathBuf::from(r"D:\x"));
    assert_eq!(watch.free_bytes(), None);
    assert_eq!(watch.root(), "D:");
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --locked -p shinatama-state --lib maintenance` and `cargo test --locked -p shinatama-desktop --features custom-protocol disk_space`
Expected: compile errors, because the names are not defined.

- [ ] **Step 3: Implement.** `DiskWatch` holds `root: String` and `free: AtomicU64` (`u64::MAX` means unknown).
  - `unsampled(directory)` is `#[cfg(test)]` or `pub(crate)` and builds the watch without a thread.
  - `start` spawns the thread `disk-space`: sample now, then every 10 s with `shinatama_state::maintenance::free_bytes`. On an error it stores `u64::MAX`.
  - `root_of`: the path's `Prefix` component as displayed (`D:`), else the first component.
  - `line`: megabytes are `free_bytes / (1024 * 1024)`, rounded down.

- [ ] **Step 4: Run the tests and see them pass** (same commands, all PASS).

- [ ] **Step 5: Commit** — `feat(disk): Shinatama samples the free space of its data disk`

### Task 3: Controller status says stall, fault and disk; one refusal sentence

**Files:**
- Modify: `apps/desktop/src-tauri/src/sidecar.rs`:
  - `Status` struct (40-66)
  - `Controller` struct and `new` (254-282)
  - `status()` (283-368)
  - the panic message in `start()` (~400)
  - `qualified_owner()` (~573)
- Modify: `apps/desktop/src-tauri/src/main.rs:717-786` (start `DiskWatch` after `app_log::start`, then `sidecar.watch_disk(...)`)
- Modify: `apps/desktop/tests/claude-resume-smoke.mjs:85` (new preparation-failure sentence)
- Test: `sidecar.rs` `mod tests`

**Interfaces:**
- Consumes: Task 1 `Reason::SupervisorFault`, `Snapshot.fault`; Task 2 `DiskWatch`, `level`, `line`, `DiskLevel`.
- Produces:
  - `pub const NOT_BACKING: &str = "The model proxy stopped backing new sessions."`
  - `pub const NOT_READY: &str = "Start the Model proxy and wait until it is ready."`, which `qualified_owner` returns.
  - `pub(crate) const STALL_AFTER_MS: i64 = 15_000`
  - `pub(crate) fn stalled(observed_at_unix_ms: Option<i64>, now_unix_ms: i64, phase: Phase) -> bool`
  - `fn phase_name(phase: Phase) -> &'static str`, the existing mapping moved out.
  - `pub(crate) struct RefusalFacts { pub phase: &'static str, pub healthy: bool, pub stalled: Option<(String, &'static str)>, pub message: Option<&'static str>, pub exit: Option<(String, u32, Option<String>)>, pub disk_line: Option<String> }`
  - `pub(crate) fn refusal_text(facts: &RefusalFacts) -> String`
  - `pub fn refusal(&self) -> String`
  - `pub fn disk_full_line(&self) -> Option<String>`, `Some` only at `DiskLevel::Full`.
  - `pub fn watch_disk(&self, watch: Arc<DiskWatch>)`
  - New `Status` fields, serialized camelCase:
    - `observed_at_unix_ms: Option<i64>`
    - `stalled: bool`
    - `last_phase: Option<&'static str>`, the snapshot's phase name, only while stalled
    - `fault: Option<String>`
    - `disk: Option<DiskStatus>` with `#[derive(Clone, Serialize)] #[serde(rename_all = "camelCase")] pub struct DiskStatus { pub root: String, pub free_bytes: u64, pub level: &'static str }`; `level` is `"ok"`, `"low"` or `"full"`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_supervisor_silent_for_more_than_fifteen_seconds_is_stalled() {
    let now = 1_000_000;
    assert!(stalled(Some(now - 15_001), now, Phase::Recovering));
    assert!(!stalled(Some(now - 15_000), now, Phase::Recovering));
    assert!(!stalled(Some(now - 60_000), now, Phase::Failed));
    assert!(!stalled(Some(now - 60_000), now, Phase::Stopped));
    assert!(!stalled(Some(now - 60_000), now, Phase::Retired));
    assert!(!stalled(None, now, Phase::Recovering));
    assert!(!stalled(Some(now + 5_000), now, Phase::Ready));
}
#[test]
fn the_refusal_names_the_state_its_age_the_exit_and_the_disk() {
    let incident = RefusalFacts {
        phase: "unavailable", healthy: false,
        stalled: Some(("20:52:40".into(), "recovering")),
        message: Some("Waiting to confirm cleanup before restarting the proxy. Its reservation remains held."),
        exit: Some(("20:52:39".into(), 1, None)),
        disk_line: Some("Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama.".into()),
    };
    assert_eq!(refusal_text(&incident), "Not started: the model proxy has not reported since 20:52:40 (last state: recovering). Waiting to confirm cleanup before restarting the proxy. Its reservation remains held. Its process ended at 20:52:39 with code 1. Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama. Its state is in the Model proxy panel.");
    let busy = RefusalFacts { phase: "busy", healthy: false, stalled: None, message: None, exit: None, disk_line: None };
    assert_eq!(refusal_text(&busy), "Not started: the model proxy is not answering its health check. Its state is in the Model proxy panel.");
    let exited = RefusalFacts { exit: Some(("20:52:39".into(), 1, Some("Error: ENOSPC: no space left on device.".into()))), ..busy };
    assert!(refusal_text(&exited).contains("Its process ended at 20:52:39 with code 1: Error: ENOSPC: no space left on device. Its state"));
}
#[test]
fn no_status_message_claims_saved_data_is_preserved() {
    for reason in [Reason::AdmissionDeferred, Reason::StartupRefused, Reason::ProcessExited, Reason::CleanupUncertain, Reason::ResourceLimit, Reason::SupervisorFault] {
        for phase in [Phase::Recovering, Phase::Failed, Phase::Retired] {
            if let Some(text) = reason_message(phase, reason) {
                assert!(!text.to_lowercase().contains("preserved"), "{text}");
            }
        }
    }
    assert_eq!(reason_message(Phase::Failed, Reason::SupervisorFault), Some("The proxy supervisor stopped on an internal error. Start or Retry recovery starts it again."));
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --locked -p shinatama-desktop --features custom-protocol sidecar::tests`
Expected: compile errors, because the names are not defined.

- [ ] **Step 3: Implement**

`stalled`: true when the phase is not Stopped, Failed or Retired and `0 <= now - at > STALL_AFTER_MS`, with a checked subtraction.

`reason_message(phase, reason) -> Option<&'static str>` is the existing match, moved out. Its sentences become:
- StartupRefused while Recovering: "The proxy could not start. Recovery will retry automatically."
- StartupRefused: "The proxy launch configuration is unavailable; retry Start after correcting the installation."
- ProcessExited: "The proxy exited. Recovering automatically on the same address."
- ResourceLimit: "The proxy reached its resource limit. Recovery will retry with fresh resource checks."
- SupervisorFault: the sentence in the test above.
- The other sentences stay as they are.

The panic message in `start()` becomes "Proxy preparation failed. Start retries it." Update `claude-resume-smoke.mjs:85` to match.

`refusal_text` order:
1. The opening: "Not started: the model proxy has not reported since {t} (last state: {state})." when stalled, else "Not started: the model proxy is {state}."
2. " {message}"
3. " Its process ended at {at} with code {code}[: {detail without its trailing '.'}]."
4. " {disk_line}"
5. " Its state is in the Model proxy panel."

The state words, by phase:

| Phase | State words |
|---|---|
| ready, busy (not healthy) | "not answering its health check" |
| stopped | "stopped" |
| preparing | "being prepared" |
| starting | "starting" |
| draining | "stopping" |
| degraded | "temporarily unavailable" |
| recovering | "recovering" |
| failed | "stopped after a failure" |
| retired | "waiting for recovery" |
| unavailable | "not reporting" |
| anything else | "unavailable" |

`status()` now also fills `observed_at_unix_ms`, `stalled`, `last_phase`, `fault` and `disk`.

Stall transitions use the new field `stall_logged: AtomicBool`:
- A false→true swap spawns a thread that writes source `proxy`: "The supervisor has not reported for 15 s (last report at HH:MM:SS: {phase}, {reason:?})."
- A true→false swap, when the status is fresh, writes "The supervisor reports again ({phase})."

`refusal()` formats times with `chrono::Local` as `%H:%M:%S`.

- [ ] **Step 4: Run the tests and see them pass** (same command, all PASS).

- [ ] **Step 5: Commit** — `fix(proxy): the status says a stalled supervisor, an internal error and a full disk`

### Task 4: Open and Resume refusals carry the cause

**Files:**
- Modify the `SidecarUnavailable` arm in each driver, so all four map to `DriverError::Failed(crate::sidecar::NOT_BACKING)`:
  - `apps/desktop/src-tauri/src/agents/claude.rs:313`
  - `apps/desktop/src-tauri/src/agents/codex.rs:341`
  - `apps/desktop/src-tauri/src/agents/opencode.rs:264`
  - `apps/desktop/src-tauri/src/agents/pi.rs:260`
- Modify: `apps/desktop/src-tauri/src/agents/service.rs`. Rename the body of `HarnessService::open` for the generic service to `fn open_unexplained`. `open` then calls it and maps the error through `explain`.
- Test: `service.rs` `mod tests`; create the module if absent.

**Interfaces:**
- Consumes: Task 3 `NOT_BACKING`, `NOT_READY`, `Controller::refusal`, `Controller::disk_full_line`.
- Produces: `pub(super) fn explain(error: DriverError, proxy_refusal: impl FnOnce() -> String, disk_full: Option<String>) -> DriverError`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_proxy_refusal_is_replaced_by_its_cause_and_a_full_disk_is_named() {
    let said = |e: DriverError| e.message().to_owned();
    assert_eq!(said(explain(DriverError::Failed(crate::sidecar::NOT_BACKING), || "Not started: X.".into(), None)), "Not started: X.");
    assert_eq!(said(explain(DriverError::Failed(crate::sidecar::NOT_READY), || "Not started: Y.".into(), None)), "Not started: Y.");
    let full = Some("Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama.".to_owned());
    assert_eq!(said(explain(DriverError::Failed("The work thread could not be saved. No harness was started."), || unreachable!(), full.clone())),
        "The work thread could not be saved. No harness was started. Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama.");
    assert_eq!(explain(DriverError::Missing, || unreachable!(), None), DriverError::Missing);
}
```

- [ ] **Step 2: Run it and see it fail**

Run: `cargo test --locked -p shinatama-desktop --features custom-protocol agents::service`
Expected: compile error, because `explain` is not defined.

- [ ] **Step 3: Implement.** Leak the composed text with `Box::leak`, as `DriverError::deferred` already does: a refusal is rare and user-initiated. Then grep `local session service changed` and `network sidecar is not ready`; no hit may remain.

- [ ] **Step 4: Run the tests and see them pass** (same command, all PASS).

- [ ] **Step 5: Commit** — `fix(agents): an open refused by the proxy says the proxy's state, the same for every harness`

### Task 5: Panic evidence and a relaunch that cannot double the app

**Files:**
- Modify: `apps/desktop/src-tauri/src/app_log.rs`
- Create: `apps/desktop/src-tauri/src/relaunch.rs` (`mod relaunch;` in `main.rs`)
- Modify: `apps/desktop/src-tauri/src/main.rs`:
  - the first lines of `fn main` (695)
  - both `generate_handler!` lists (873 and 1067); the non-Windows list gets a stub that returns `Err`
- Test: `app_log.rs` and `relaunch.rs` `mod tests`

**Interfaces:**
- Produces:
  - `pub(crate) fn panic_line(thread: &str, location: Option<(&str, u32)>, payload: &str) -> String`
  - `pub(crate) fn install_panic_hook()`. The hook writes with `WRITING.try_lock()`, skips the line when the lock is busy, then calls the previous hook.
  - `pub(crate) fn after_exit_pid(args: impl IntoIterator<Item = String>) -> Option<u32>`
  - `pub(crate) fn wait_for_exit(pid: u32, timeout: Duration) -> bool`
  - `pub(crate) fn spawn_after_exit() -> std::io::Result<()>`
  - `#[tauri::command(async)] pub(crate) fn app_relaunch(window: tauri::WebviewWindow) -> Result<(), String>`

- [ ] **Step 1: Write the failing tests**

```rust
// app_log.rs
#[test]
fn a_panic_is_one_line_with_thread_place_and_text() {
    assert_eq!(panic_line("shinatama-sidecar-owner", Some(("crates/engine/src/x.rs", 12)), "boom"),
        "thread 'shinatama-sidecar-owner' panicked at crates/engine/src/x.rs:12: boom");
    assert_eq!(panic_line("main", None, "boom"), "thread 'main' panicked at an unknown location: boom");
}
// relaunch.rs
#[test]
fn only_a_positive_after_exit_pid_is_read() {
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(after_exit_pid(args(&["Shinatama.exe", "--after-exit=4242"])), Some(4242));
    assert_eq!(after_exit_pid(args(&["Shinatama.exe", "--after-exit=0"])), None);
    assert_eq!(after_exit_pid(args(&["Shinatama.exe", "--after-exit=x"])), None);
    assert_eq!(after_exit_pid(args(&["Shinatama.exe"])), None);
}
#[test]
fn waiting_on_a_live_process_times_out_and_on_an_ended_one_returns_at_once() {
    assert!(!wait_for_exit(std::process::id(), Duration::from_millis(50)));
    let mut child = std::process::Command::new("cmd").args(["/c", "exit 0"]).spawn().unwrap();
    let pid = child.id(); child.wait().unwrap();
    assert!(wait_for_exit(pid, Duration::from_secs(5)));
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test --locked -p shinatama-desktop --features custom-protocol app_log relaunch`
Expected: compile errors, because the names are not defined.

- [ ] **Step 3: Implement**

`wait_for_exit`:
- `OpenProcess(SYNCHRONIZE, 0, pid)`. A null handle means the process has already ended, so return true.
- `WaitForSingleObject(handle, ms)`: return true on `WAIT_OBJECT_0`, and close the handle.

`spawn_after_exit`:
- `std::env::current_exe()` with the single argument `--after-exit=<this pid>`.
- Use `creation_flags(CREATE_BREAKAWAY_FROM_JOB)`; if that spawn fails, retry without the flag.

`app_relaunch`:
- `local(&window)?`, then `spawn_after_exit()`.
- On error, return "Shinatama could not start its new window; nothing was closed."

`main` begins:
1. `app_log::install_panic_hook();`
2. On Windows, `if let Some(pid) = relaunch::after_exit_pid(std::env::args())`, then `if !relaunch::wait_for_exit(pid, Duration::from_secs(60)) { return; }`. Nothing else runs before this.

- [ ] **Step 4: Run the tests and see them pass** (same command, all PASS).

- [ ] **Step 5: Commit** — `feat(app): panics are logged, and Shinatama can restart itself without a second window`

### Task 6: Pure panel decisions for stall, fault, disk and restart

**Files:**
- Modify: `apps/desktop/src/sidecar-status.ts`
- Test: `apps/desktop/tests/sidecar-status.test.ts`

**Interfaces:**
- Consumes: Task 3 status fields (`observedAtUnixMs`, `stalled`, `lastPhase`, `fault`, `disk`).
- Produces:
  - Optional `ProxyStatus` fields: `observedAtUnixMs?: number | null`, `stalled?: boolean`, `lastPhase?: string | null`, `fault?: string | null`, `disk?: DiskStatus | null`.
  - `export type DiskStatus = { root: string; freeBytes: number; level: 'ok' | 'low' | 'full' }`
  - `export function restartOffered(status: ProxyStatus | null, failedPolls: number): boolean`
  - `export function stallLine(status: ProxyStatus, clock?: (unixMs: number) => string): string | null`
  - `export function faultLine(status: ProxyStatus): string | null`
  - `export function diskLine(status: ProxyStatus): string | null`
  - `export function messageLine(status: ProxyStatus | null): string | null`
  - `export function indicator(status: ProxyStatus | null): { tone: IndicatorTone; label: string }`

- [ ] **Step 1: Write the failing tests**

```ts
test('a stalled supervisor offers Restart and says since when, a fresh failed one does not', () => {
  const clock = () => '20:52:40';
  const stalled = status({ phase: 'unavailable', healthy: false, stalled: true, observedAtUnixMs: 1, lastPhase: 'recovering' });
  assert.equal(restartOffered(stalled, 0), true);
  assert.equal(restartOffered(status({ phase: 'failed', healthy: false, stalled: false }), 0), false);
  assert.equal(restartOffered(null, 2), false);
  assert.equal(restartOffered(null, 3), true);
  assert.equal(stallLine(stalled, clock), 'The proxy supervisor has not reported since 20:52:40 (last state: Recovering…). Restart Shinatama to start a new proxy.');
  assert.equal(stallLine(status(), clock), null);
  assert.deepEqual(indicator(stalled), { tone: 'red', label: 'Model proxy: Not reporting' });
  assert.deepEqual(indicator(status()), { tone: 'green', label: 'Model proxy: Ready' });
});
test('fault, disk and the default message say only what is known', () => {
  assert.equal(faultLine(status({ fault: 'boom' })), 'The proxy supervisor stopped on an internal error: boom');
  assert.equal(faultLine(status()), null);
  const MiB = 1024 * 1024;
  assert.equal(diskLine(status({ disk: { root: 'D:', freeBytes: 5 * MiB, level: 'full' } })), 'Disk D: is full (5 MB free): saving fails for the proxy and for Shinatama.');
  assert.equal(diskLine(status({ disk: { root: 'D:', freeBytes: 512 * MiB, level: 'low' } })), 'Disk D: has 512 MB free.');
  assert.equal(diskLine(status({ disk: { root: 'D:', freeBytes: 9e9, level: 'ok' } })), null);
  assert.equal(diskLine(status()), null);
  assert.equal(messageLine(status({ healthy: false, message: null })), null);
  assert.equal(messageLine(status({ healthy: true, message: null })), 'Local service is responding.');
});
test('the new fields are validated like the rest of the reply', () => {
  const ok = { phase: 'ready', healthy: true, message: null, inFlight: 0, agentSessions: 0, drainDeadlineUnixMs: null, configDrift: false };
  assert.ok(parseStatus({ ...ok, stalled: true, observedAtUnixMs: 5, lastPhase: 'recovering', fault: null, disk: { root: 'D:', freeBytes: 1, level: 'full' } }));
  for (const bad of [{ stalled: 'yes' }, { lastPhase: 'nope' }, { fault: 'a\nb' }, { fault: 'x'.repeat(301) },
    { disk: { root: 'D:', freeBytes: -1, level: 'low' } }, { disk: { root: 'D:', freeBytes: 1, level: 'huge' } }, { disk: { root: '', freeBytes: 1, level: 'low' } }, { observedAtUnixMs: 'now' }]) {
    assert.equal(parseStatus({ ...ok, ...bad }), null, JSON.stringify(bad));
  }
});
```

- [ ] **Step 2: Run them and see them fail**

Run: `node --experimental-strip-types --test apps/desktop/tests/sidecar-status.test.ts`
Expected: FAIL, because the new exports do not exist.

- [ ] **Step 3: Implement.**
  - `diskLine` uses `Math.floor(freeBytes / 2**20)`.
  - `indicator` returns red and "Model proxy: Not reporting" when `stalled`. Otherwise it returns `indicatorTone(phase)` and `` `Model proxy: ${phaseLabel(phase)}` ``.
  - `parseStatus` accepts each new field only in the shapes the tests allow: `root` 1-16 characters, `fault` at most 300 characters with no control character, `lastPhase` a `LABELS` key.

- [ ] **Step 4: Run the tests and see them pass**

Run: `npm test`
Expected: every test passes.

- [ ] **Step 5: Commit** — `fix(panel): pure decisions for a stalled supervisor, an internal error and a full disk`

### Task 7: The panel renders them and restarts Shinatama

**Files:**
- Modify: `apps/desktop/src/sidecar-control.ts`
- Modify: `apps/desktop/src/main.ts`:
  - the `confirm-close` listener at 1663-1681, whose body is extracted
  - the `createSidecarControl` call at 249

**Interfaces:**
- Consumes: Task 6 functions; Task 5 command `app_relaunch`.
- Produces: `createSidecarControl(desktop: boolean, notify: (message: string) => void, restart: () => Promise<void>)`; in `main.ts`, `async function closeWorkspace(relaunch: boolean): Promise<void>`.

- [ ] **Step 1: Implement `closeWorkspace(relaunch)` in `main.ts`.** It holds the current `confirm-close` body, except the first "Close workspace?" question.
  - When `relaunch` is set, it starts with `await invoke('app_relaunch')`. An error is thrown before anything has closed.
  - The listener keeps its question and then calls `closeWorkspace(false)`.
  - `createSidecarControl` receives `() => closeWorkspace(true)`.
  - Because `createSidecarControl` runs at line 249, before `closeWorkspace` exists, pass a function that looks it up when called.

- [ ] **Step 2: Implement the panel.**
  - Three new hint paragraphs: stall, fault and disk, each hidden when its line is null.
  - The message paragraph uses `messageLine`, and the toolbar button uses `indicator()`.
  - Add a `Restart Shinatama` button, shown and enabled only when `restartOffered(current, failedPolls)`.
  - Clicking it opens an inline group with the text "Shinatama will close and start again. Running terminals stop; their sessions are restored at the next start.", a `Restart Shinatama` button and a `Keep waiting` button.
  - Confirming calls `restart()` and passes any error to `notify`.
  - `failedPolls` counts consecutive `sidecar_status` failures and resets on a parsed reply.

- [ ] **Step 3: Type-check and build the frontend**

Run: `npm run build`
Expected: `tsc --noEmit` reports no error and the vite build completes.

- [ ] **Step 4: Run all frontend tests**

Run: `npm test`
Expected: every test passes.

- [ ] **Step 5: Commit** — `fix(panel): the Model proxy panel shows a stall, an internal error and a full disk, and can restart Shinatama`

### Task 8: Verify, install, merge

- [ ] **Step 1: Check free space.** Run `df -B1M /d`. Continue only with at least 8 GB free.
- [ ] **Step 2: Formatting and lints**

Run: `cargo fmt --all --check` and `cargo clippy --locked -p shinatama-engine -p shinatama-state -p shinatama-desktop --all-targets --features custom-protocol -- -D warnings`
Expected: no output from fmt; clippy ends with `Finished` and no warning.

- [ ] **Step 3: Rust tests of the touched crates**

Run: `cargo test --locked -p shinatama-engine -p shinatama-state -p shinatama-desktop --features custom-protocol --no-fail-fast`
Expected: `test result: ok` for every binary.

- [ ] **Step 4: Release build**

Run: `build-release.bat`
Expected: it ends without error and fills `release\`.

- [ ] **Step 5: Install in place.** This needs the owner's go, because Shinatama must close and the sessions come back at the next start.
  - Close Shinatama.
  - Copy each executable from `release\` over the file of the same relative path in `D:\Software\Shinatama`. Make no backup.
  - Start Shinatama.
  - Check that `shinatama.log` shows `proxy: Ready` and that the panel reads Ready.
- [ ] **Step 6: Merge**

Run: `git switch develop && git merge --no-ff fix/proxy-panel-resilience && git branch -d fix/proxy-panel-resilience`
Expected: the merge succeeds and the branch is deleted.
