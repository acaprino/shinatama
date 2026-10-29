# CX11 — `codex login status` (partial)

Item: CX11 (G1) — "`codex login status` output; `codex login` in a PTY." Pass when: "Parseable; no secret in output."
Status: **running (partial)** — both sign-in states recorded; `codex login` in a PTY remains (O1).
Date: 2026-10-27.
Runner: JanuraCode agent; no login performed, no model calls, cost 0.
Machine: Windows 11 Home build 26200; PowerShell 5.1.26100.9711.
Agent: codex-cli 0.164.0 (native installer binary; see the CX4 record).
Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/CX11/run.ps1` (rerunnable without edits; runs both cases).
Raw output: `verify/CX11/out/2026-10-27/codex-login-status.{real,scratch}.stdout.txt` and `…stderr.txt` (plus the first-run copies without the suffix).

## Result

- **Real home (signed in):** exit 0; **stdout empty; the message is on stderr**: `Logged in using ChatGPT`.
- **Scratch `CODEX_HOME` (signed out):** exit 1; stderr `Not logged in`; stdout empty.
- `codex login --help` (E1 capture) lists only the `status` command; no `--json` flag observed.
- No secret in the output.
- Adapter implication: read **stderr**, use the exit code (0 = signed in, 1 = not) and the last line as the human-readable state.

## Criterion

Partially met: parseable in both states via stderr + exit code. Still open: `codex login` in a PTY.

## Spec before / after

Before: CX11 open.
After: unchanged; the record fixes how to parse this command on 0.164.0.
