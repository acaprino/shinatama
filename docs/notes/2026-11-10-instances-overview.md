# F04 instance overview (2026-11-10)

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-desktop instances` | 1 passed: only process exit is certain; no permission state inferred |
| `node apps/desktop/tests/instances-smoke.mjs` | PASS (4 checks) on a real PowerShell terminal |

Open: native agent hooks as the trusted state source (Claude `--settings` hooks, Codex
notifications, OpenCode SSE, Pi extension: CL8, CX7, OC5, PI4/PI6), pending-request payloads
(R04), tokens/cost/context fill per row (R09; the per-session ledger is in the session detail),
flows and timeline (R10, F05), hop and pause-summaries actions (R14 allows them; only Focus
exists), agent icons and accents (R15).
