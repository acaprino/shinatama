# C09 — contained Windows terminals

> **Superseded for interactive memory behavior, 2026-11-12.** The evidence below
> records the original hard-cap implementation and is preserved unchanged.
> Interactive terminal jobs now use advisory memory: no Windows memory caps and
> no memory-triggered stop or pause. New starts use a small absolute headroom
> check, and the widget reports pressure and the largest measured terminal.
> Process/CPU/queue constraints and background workload containment remain.
> See [the replacement policy](../notes/2026-11-12-interactive-memory.md).
> Historical allocation-refusal results below do not verify this new behavior.

The desktop now creates ConPTY in an owned helper, admitted into its native job
before executing helper code. Before the shell can run, the parent verifies live
helper and console-host identities in that same job; the shell is also created
suspended with job and pseudoconsole attributes and verified before resuming.
There is no unprotected Windows fallback. The default host refuses execution until
the runtime provides explicit admission and configuration.

Desktop shell limits are frozen at 512 MiB job commit, 256 MiB per process, a
native process quota of eight and 10% machine CPU. Windows 11 build 26200 can
materialize console hosts above that native quota: the raw physical census is
retained, and the exact registered job is stopped when its observed count exceeds
eight. Console hosts are never subtracted to claim compliance. Shared admission
uses fresh measured headroom and an
immutable SQLite ledger policy capped at 4 GiB/80% CPU. Actual installed limits are
separate from host/renderer processes. Native reads, waits, parsing and cleanup run
on bounded owners; input/output queues, emulator scrollback and IPC batches are
finite. Parsing acknowledgement supplies frontend backpressure. Uncertain cleanup
retains its reservation and slot. Explicit shutdown cannot trap the user in a
repeated close confirmation; it never falsely releases an uncertain reservation.

The real three-session fixture passes CPU/output flooding, shell self-crash and
exact-handle helper crash while saved work remains intact and a healthy sibling
continues responding. The latest passing run includes separate managed-memory and
direct-process pressure: 26 fresh responses, maximum 82 ms against the frozen
1500 ms limit, and 338008 output bytes. Managed allocation hits the native memory
refusal at 252 MiB peak process / 255 MiB peak job commit. The process case records
five successful child starts, eleven native quota refusals (1816), thirteen raw
physical members and the attributed guard stop. The raw query now materializes up
to its independent 256-member observation bound instead of rejecting an OS count
above the configured quota. No automatic
replacement or command replay occurred. The combined real WebView proof also
passes input/output, three nested panes, find, themes, reload and confirmed close.

Two defects were found and fixed: the shell initially inherited the RPC stdout
handle (raw prompt bytes corrupted framing), and startup's missing measurements
could incorrectly latch recovery thresholds. The helper now explicitly lets the
pseudoconsole supply shell standard handles. Pressure hysteresis latches only the
dimension that actually crossed its entry threshold; ten policy tests pass without
changing any threshold or cap. Seven terminal protocol/configuration tests pass.

Ten focused terminal tests pass, including cached consumption, retained actual
limits, nonblocking cache reads and refusal without a native launch context.
The final real WebView proof passes cached per-terminal consumption/applied limits,
three nested panes, themes/find/reload, sidecar Ready, an isolated Claude startup
receipt followed by its original visible TUI, exact instance/thread references,
explicit terminal stop and confirmed application close. Per-agent model traffic
and prolonged release qualification are not inferred from startup alone.

Short-lived startup children exposed a historical-census gap. The trusted owner
now irreversibly seals further assignment only after its startup attributes and
temporary ASSIGN handles are gone (terminal: verified ShellReady before Resume).
Cleanup still requires successful termination, ActiveProcesses=0 on the same held
job, all known process handles signaled, final job-handle close, returned I/O/queue
credits and SQL release last. Unsealed/reopened/crash paths retain strict history
requirements. Ten job tests preserve the known-handle and failed-termination
guards. A real sealed Node fixture starts three short-lived children without an
intermediate census: TotalProcesses=8, ActiveProcesses=2 at its barrier; exact
known handle signaled, marker intact and reservation released in 2.05 s. No quota,
timeout, PID-absence or job-name fallback was used.

The resource governor's explicit defer flag governs admission; reducing optional
work alone does not prohibit starts. A bounded read-only preflight wait precedes
reservation, and admission rechecks fresh complete measurements. Shutdown closes
new admission. Expected control-channel disconnection during cleanup no longer
reports a queue overflow; a real full queue still cancels only that terminal.
