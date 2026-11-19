# P14 containment probe — Windows evidence

Date: 2026-11-02. Scope: partial P14 (process lifecycle) on Windows with synthetic children,
for plan step C09. It closes no gate: no real agent, MCP server or sidecar was used.

Environment: Windows 11 Home NT 10.0.26200.0, WebView2 runtime 156.0.4314.58, rustc 1.99.1,
portable-pty 0.9.0, windows-sys 0.61.2, the system ConPTY (no sideloaded `conpty.dll`).

## Method

`verify/P14` is a standalone workspace, not a member and not part of the product. A hostile
synthetic child (`p14-probe child`) creates a grandchild at once, either inheriting the console or
detached from it (`DETACHED_PROCESS`, as a daemon would be), and sleeps for at most 120 s so a leak
cleans itself up. Membership is read with `IsProcessInJob`; liveness through process handles held
before the kill, so a reused PID cannot be mistaken for a survivor.

Strategies for one terminal:

- `portable`: portable-pty spawns, then the process is opened and assigned to a `KILL_ON_JOB_CLOSE`
  job. This is a replica of `crates/terminal/src/process_tree.rs`, not the production crate.
- `suspended`: `CreatePseudoConsole` directly, `CREATE_SUSPENDED`, assign, resume.
- `joblist`: `CreatePseudoConsole` directly, with `PROC_THREAD_ATTRIBUTE_JOB_LIST` and the
  pseudoconsole attribute in one attribute list.

A "global job" is a `KILL_ON_JOB_CLOSE` job that the probe process joins before creating anything.
"Kill of the app" is `TerminateProcess`, the stand-in for `kill -9`.

```sh
cargo build --manifest-path verify/P14/Cargo.toml
verify/P14/target/debug/p14-probe.exe driver 0   # grandchild inherits the console
verify/P14/target/debug/p14-probe.exe driver 1   # grandchild detached
verify/P14/target/debug/p14-probe.exe sweep
verify/P14/target/debug/p14-probe.exe real-app target/debug/shinatama-desktop.exe
verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/desktop-smoke.mjs
P14_NO_DSR=1 verify/P14/target/debug/p14-probe.exe app portable 0 stop-a <empty dir>
```

## Results

### 1. Stopping one of two terminals, grandchild created before the assignment (forced)

The order was forced by waiting until the grandchild existed before assigning the process.

| Strategy | Grandchild in the session job | Job termination only | Job termination and pseudoconsole close (what `stop` does) |
| --- | --- | --- | --- |
| `portable`, with or without a global job | no | grandchild survives | console-inheriting: dies; detached: **survives** |
| `suspended` | yes | whole tree dies | whole tree dies |
| `joblist` | yes | whole tree dies | whole tree dies |

Terminal B kept both processes alive in every run. A global job does not change the first row:
the grandchild is in the global job but not in the session job, so stopping A cannot reach it.
A console-inheriting grandchild dies only because closing the pseudoconsole terminates the clients
attached to it.

### 2. Kill of the app

| Strategy | Global job | Console-inheriting grandchild | Detached grandchild |
| --- | --- | --- | --- |
| `portable`, forced order | no | 0 survivors | **2 survivors** (A and B grandchildren) at 1, 3 and 10 s |
| `portable`, forced order | yes | 0 survivors | 0 survivors |
| `suspended` | yes | 0 survivors | 0 survivors |
| `joblist` | yes | 0 survivors | 0 survivors |

Survivors were counted at 1, 3 and 10 s. The global job handle is not inheritable, and kill-on-close
fired within 1 s every time, so no copy survived in a child.

### 3. Is the window reachable?

Ten trials per delay between the spawn call and the assignment (portable strategy):

| Delay | Reader answering the console started first | Reader started after the assignment (production order) |
| --- | --- | --- |
| 0–12 ms | 0/10 escaped | 0/10 escaped |
| 20 ms | 6/10 escaped | 0/10 escaped |
| 50 ms and 200 ms | 10/10 escaped | 0/10 escaped; the child had not run in any trial |

- Natural order, as production does it: 0/60 escaped. The assignment took 38–248 µs (median 77 µs).
- With a reader that drains output but never answers the console's cursor-position query, the child
  never ran within 15 s; with the answer it ran at once. The console host holds the child until the
  host answers, so in the production order no child code runs before the assignment on this build.
  This is observed behaviour of this Windows build, not a documented guarantee, and the frontend's
  emulator is what answers in the real application.
- `suspended` and `joblist`: 0/30 escapes each. `JOB_LIST` works together with the pseudoconsole
  attribute here, and the child stays in the global job as well. Creating the process and assigning
  it took a median 7.7 ms (`suspended`) and 8.1 ms (`joblist`), dominated by process creation.

### 4. The real application inside a global job

`shinatama-desktop.exe` started under a wrapper that had joined the job: 7 processes were in the
job (the application and six `msedgewebview2.exe`). Killing the wrapper left 0 survivors at 1 s.
The complete `desktop-smoke.mjs` run, with node, the application and WebView2 inside the job,
passed with no frontend errors.

## Consequences for C09

- The assignment race cannot be reached with the current order on this build, and the margin is
  about two orders of magnitude even without the console hold. It depends on behaviour nobody
  promises, so C09 needs a test that fails if a child runs before the assignment, with `JOB_LIST`
  as the verified fallback (atomic, no resume step).
- A global job is worth adopting regardless: it covers processes started outside a PTY, such as the
  sidecar, and the real application tree tolerates it.
- A terminal stop must close both the job and the pseudoconsole, as dropping the session does today.

## Not proved

Real agents (P14 stays open); crash paths other than `TerminateProcess` (panic, abort, power loss);
`CREATE_BREAKAWAY_FROM_JOB` attempts by descendants; processes started through services, COM or the
task scheduler, which escape any job; the sidecar; Windows 10; CPU load while attaching (the sweep
ran on a quiet machine, ten trials per delay); macOS and Linux, which are deferred.
