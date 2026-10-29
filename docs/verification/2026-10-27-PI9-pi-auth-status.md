# PI9 — Pi sign-in detection (partial)

Item: PI9 (G1) — "Sign-in detection without reading `auth.json`; login flow in a PTY." Pass when: "Parseable status or marked 'unknown'."
Status: **running (partial)** — the surface and one executed check are recorded; the signed-in case needs a provider actually configured in Pi (O1 for any login).
Date: 2026-10-27.
Runner: JanuraCode agent; no login, no credential material read, cost 0.
Machine: Windows 11 Home build 26200; PowerShell 5.1.26100.9711.
Agent: pi 1.2.0 (`@earendil-works/pi-coding-agent`).
Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/PI9/run.ps1` (rerunnable without edits; captures the help, then runs the check on the real home and in a scratch `PI_CODING_AGENT_DIR`).
Raw output: `verify/PI9/out/2026-10-27/pi-auth-help.txt`, `pi-auth-check.real.txt`, `pi-auth-check.scratch.txt`.

## Result — `pi auth --help` (1.2.0)

- `pi auth print-api-key [--provider <provider>] [--model <model>]`
- `pi auth print-bearer-token [--provider <provider>] [--model <model>] [--min-expiry <duration>]`
- `pi auth check [--provider <provider>|--model <model>] [--json] [--credentials] [--no-refresh]`
- "Auth commands require at least one of `--provider` or `--model`." "Checks refresh expired OAuth credentials by default; `--no-refresh` prevents this." "`--credentials` emits the credential, or includes it in JSON output."

## Executed check

`pi auth check --json --no-refresh --provider anthropic` (the `--no-refresh` flag avoids writing/refreshing credentials):
- **Real home:** exit 1, `{"status":"not_ready","provider":"anthropic","reason":"credentials_not_configured"}`.
- **Scratch `PI_CODING_AGENT_DIR`:** identical output.
- The JSON is parseable; the exit code separates ready (0) from not-ready (1) in this observation. No credential material appears without `--credentials`.

## Criterion

Partially met: a parseable status exists (`--json`). Still open: the signed-in case with a configured provider, and `pi auth login` in a PTY. `--credentials` and the `print-*` commands must never be used (D5).

## Spec before / after

Before: PI9 open.
After: unchanged; the candidate detector is `pi auth check --json --no-refresh --provider <p>`.
