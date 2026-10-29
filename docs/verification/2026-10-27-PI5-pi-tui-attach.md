# PI5 — Pi TUI attach to a backend: does not exist (spec Q2)

Item: PI5 (G1) — "Whether the Pi TUI can attach to a backend at all (spec Q2); otherwise delete Pi from the attach list." Pass when: **spec corrected**.
Status: **passed (spec corrected)**.
Date: 2026-10-27.
Runner: JanuraCode agent, on the user's machine, no login, no model use, cost 0.

Machine: Windows 11 Home build 26200; PowerShell 5.1.26100.9711; Node v24.14.1.
Agent: pi 1.2.0 (`@earendil-works/pi-coding-agent`, installed package docs).
Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/PI5/run.ps1` (rerunnable without edits).
Raw output: `verify/PI5/out/2026-10-27/` (`pi-help.txt`, `pi-docs-list.txt`, `pi-docs-attach-hits.txt`).

## Result

1. `pi --help` has **no attach, remote, server, socket, connect or port flag**. The only mode-related switch is `--mode text|json|rpc`.
2. Bundled docs are explicit: "By default, running `pi` opens the interactive terminal interface" and the four interfaces — Interactive (Terminal UI), Print, JSON, RPC — are selected at startup and mutually exclusive (`docs/cli-integration.md`).
3. "RPC mode runs Pi as a long-lived subprocess controlled through JSON records on stdin and stdout… for language-independent integrations, process isolation, IDEs, and custom user interfaces" (`docs/rpc.md`). RPC is Pi **as** a backend for an external client; it is not a mode in which the TUI attaches to someone else's backend.
4. Contrast with the other two: Codex has `--remote <ADDR>` (plus `--remote-auth-token-env`) and OpenCode has `--server URL` — both confirmed in the E1 help captures.

Conclusion for Q2: the "Pi (rpc)" entry in engine 5.7 item 4 cannot mean "the hosted TUI still renders the real agent". For Pi the choice is: host the TUI (no structured backend) **or** drive an RPC process (no TUI pane) — not both.

## Criterion

Met: the spec is corrected. Edit applied to `docs/technical-design.md`, engine 5.7 item 4 (correction paragraph dated 2026-10-27 pointing at this record).

## Spec before / after

Before (item 4): "Codex (`app-server` plus `--remote`), OpenCode (`--server URL`) and Pi (`rpc`) can run a backend that Shinatama owns and talks to over a structured protocol, while the hosted TUI still renders the real agent."
After: Codex and OpenCode unchanged; Pi corrected as above (no TUI attach; RPC backend only).

## Open

- If Shinatama later wants a Pi RPC integration, it is a different feature from TUI attach and belongs in its own candidate (structured control without a pane).
- No runtime probe was possible because there is no flag to exercise; `pi --help` and the shipped docs are authoritative for the installed version (`docs/rpc.md` says so itself).
