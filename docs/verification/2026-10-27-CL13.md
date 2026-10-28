# CL13 — `claude auth status` (sign-in detection)

Item: CL13 (G1) — "`claude auth status` reports sign-in without exposing credentials; its output format; `auth login` runs in a PTY." Pass when: "Parseable; no secret in output."
Status: **running (partial)** — both sign-in states are recorded (real home signed in, scratch `CLAUDE_CONFIG_DIR` signed out); `auth login` in a PTY and the polling behaviour remain (O1).
Date: 2026-10-27.
Runner: JanuraCode agent; no login performed, no model calls, cost 0.
Machine: Windows 11 Home build 26200 (NT 10.0.26200); PowerShell 5.1.26100.9711.
Agent: claude 2.1.311 (drift vs the recorded 2.1.310 — see the E1 record).
Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/CL13/run.ps1` (rerunnable without edits; runs both cases; masks identity fields before writing).
Raw output: `verify/CL13/out/2026-10-27/claude-auth-status.real.json` and `…scratch.json` (plus the first-run copy `claude-auth-status.json`).

## Result

- **Real home (signed in):** exit 0. One JSON object; keys observed: `loggedIn` (true), `authMethod` (`claude.ai`), `apiProvider` (`firstParty`), `analyticsDisabled`, `projectsDirectory`, `configDirectory`, `email`, `orgId`, `orgName`, `subscriptionType`.
- **Scratch `CLAUDE_CONFIG_DIR` (signed out):** exit 1. Same shape with `loggedIn` (false), `authMethod` (`none`); no identity keys. Parseable the same way.
- **No credential material** in either output; identity fields (email, org id, org name) are present only in the signed-in case and are redacted in the archived copy.
- Cross-check: JanuraCode's `lib/opencode/claude-cli-auth.js` (MIT) runs exactly this command (6 s timeout, strips `ANTHROPIC_API_KEY`/`ANTHROPIC_AUTH_TOKEN`/`CLAUDE_CODE_OAUTH_TOKEN`, resolves the binary via `where claude` on Windows) — `docs/notes/2026-10-27-januracode-code-survey.md`.

## Criterion

Partially met: the output is parseable and carries no secrets in both states. Still open: `auth login` in a PTY and the polling cost/behaviour while signed out.

## Spec before / after

Before: CL13 open; engine 5.5 intends `auth_status()` to ask the agent, never read credentials.
After: unchanged; the exit code separates the two states (0 in, 1 out on 2.1.311).
