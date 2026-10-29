# PI11 — Pi start-up timing (partial)

Item: PI11 (G0) — "**Observed:** `pi --help` printed nothing within 25 s on this machine today. … Time `pi --version` and a bare launch with and without `pi-subagents`." Pass when: "Start time recorded; the hop timeouts and `doctor` timeouts set above it, or a workaround is used."
Status: **running (partial)** — `--version` and `--help` timings recorded on 2026-10-27; the bare-launch measurement and with/without `pi-subagents` still to do.
Date: 2026-10-27.
Runner: JanuraCode agent, on the user's machine, no login, no model use, cost 0.

Machine: Windows 11 Home build 26200; PowerShell 5.1.26100.9711; Node v24.14.1.
Agent: pi 1.2.0.
Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/PI11/run.ps1` (rerunnable without edits).
Raw output: `verify/PI11/out/2026-10-27/` (`pi-version.txt`, `pi-help-timing.txt`).

## Result (this machine)

| run | command | result |
|---|---|---|
| 1 (earlier today) | `pi --help` | **no output within 40 s** (capped) |
| 2 (earlier today) | `pi --version` | 1.6 s, "1.2.0" |
| 3 (script run) | `pi --version` | **1.4 s**, "1.2.0" |
| 4 (script run) | `pi --help` | **12.2 s**, 10 979 chars |
| 5 (E1/PI5 runs) | `pi --help` | completed twice; one run measured at 12.5 s |

Reading: `--version` is fast and stable; `--help` is variable — twice ~12 s, once above 40 s with no output. The register's 2026-10-26 note (nothing within 25 s) is consistent with the slow tail, not with a constant hang. A variable start cost of this size matters for the hop receipt (90 s timeout) and `doctor` timeouts; it cannot yet be attributed (cold start, npm shim, WMI, `pi-subagents`, disk) without the bare-launch runs.

## Criterion

Not met yet. Still to run: a bare interactive launch timed to input-ready, with and without `pi-subagents`, in a scratch `PI_CODING_AGENT_DIR`, before the hop/doctor timeouts can be set from data.

## Spec before / after

Before: register row carries the 25 s observation of 2026-10-26.
After: unchanged; this record is the timing evidence collected since.
