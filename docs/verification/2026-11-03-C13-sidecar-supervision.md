# C13 — Windows sidecar owner and bounded recovery

Local native proof passed on 2026-11-03: **1/1 test, 8.33 s** (compile 2.09 s).
The receipt is [2026-11-03-C13-sidecar-supervision.result.json](2026-11-03-C13-sidecar-supervision.result.json).

```powershell
cargo test --offline -p shinatama-engine --test sidecar_supervisor -j1 -- --ignored --test-threads=1 --nocapture
```

The test uses the verified Node/bundle recipe, private C01 storage, a real SQLite
reservation ledger and the real `SidecarSupervisor`. A synthetic private CA is
prepared in a separately cleaned, contained setup process. Native limits remain
1 GiB per job, 768 MiB per process, eight active processes, 20% CPU, 256 KiB/four
queue credits, and a 30-second wall budget.

| Actual observation | Result |
| --- | --- |
| Finite pause of the CLI's own earliest thread, verified through retained process/thread handles, creation time and exact job membership | Busy; same owner; no extra reservation |
| Busy observation while TCP probes wait | Heartbeat advances; first delay 1,000 ms |
| Resume the exact paused thread | Same process becomes healthy; normal delay restored to 200 ms |
| First controlled exit through the retained private process handle | Exactly one replacement, same address, new job/process identity |
| Second controlled exit and final stop | Failed / repeated_exit; two prepare calls total; saved marker intact; all jobs closed, ledger empty and worker exited |

The owner worker also checks raw process counts, current private commit and queue
usage against its own applied job/ticket budgets. Console helpers are counted.
Resource violations stop only that contained job; uncertain cleanup retains its
owner and reservation. Health failures with a living process use a loopback bind:
occupied means Busy, free means Degraded. Busy probe delays grow 1/2/4/8 seconds;
only a real healthy response resets them. Unit coverage records the eight-second
cap; this native fixture observes the first busy delay and recovery.

The targeted same-connection shutdown regression passed separately (1/1, 0.01 s).
Its initial failure came from a Windows test socket inheriting nonblocking mode;
the server now waits within its finite deadline for the second keep-alive request.
The native driver's initial final-stop failure incorrectly rejected the expected
Failed → Stopped transition and was corrected before the passing run.

This is one bounded Windows/Node proof, not the five-instance soak, other runtime
or OS matrix, full SC5/SC6/SC14/SC15 acceptance, or desktop UI qualification. The
harness issues no model/provider calls and replays no agent commands. Offline
startup is not qualified: the private upstream log reports loading the public
models.dev registry. No upstream bundle patch, OS trust installation, real user
configuration or credentials is involved. The separate final combined desktop
proof in [C15](2026-11-03-C15-first-tui.md) covers service Ready/stop and original
Claude TUI startup. No workspace-wide Clippy or full release result is claimed.
