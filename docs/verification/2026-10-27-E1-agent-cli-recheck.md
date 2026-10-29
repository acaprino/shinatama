# E1 — [R] facts of engine 5.7 re-checked on the installed versions

Item: E1 (G0) — "Every `[R]` line in engine 5.7 (read from docs, source or `--help`, not run) is confirmed on the installed versions by the matching fixture above."
Status: **running (partial)** — CLI surface re-checked from `--help` on the installed binaries today; the runtime halves of the CL/CX/OC/PI rows are not run.
Date: 2026-10-27.
Runner: JanuraCode agent, on the user's machine, no login, no model calls, cost 0.

Machine: Windows 11 Home, build 26200 (NT 10.0.26200); PowerShell 5.1.26100.9711; Node v24.14.1; npm 11.11.0.
Repo state: `D:\Projects\shinatama`, git initialized with no commits yet; all files untracked.
Machine file: `verify/E1/out/2026-10-27/machine.txt`.

Installed versions (captured by the script):

| agent | installed | spec recorded in engine 5.7 | note |
|---|---|---|---|
| Claude Code | **2.1.311** | 2.1.310 | **drift**: per the register §0 version rule, CL rows must be re-run on 2.1.311 before any support claim |
| Codex CLI | 0.164.0 | 0.164.0 | match |
| OpenCode | v2.1.4 | 2.1.4 | match |
| Pi | 1.2.0 | 1.2.0 | match; npm registry latest is 1.4.0 (future drift, see PI10) |

Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/E1/run.ps1` (rerunnable without edits).
Raw output: `verify/E1/out/2026-10-27/` (`*-version.txt`, `*-help.txt`).

## Result — what `--help` on the installed binaries confirms

**Claude Code 2.1.311** (file `claude-help.txt`)
- Interactive start `claude [prompt]`, working directory is the process's own; **no `-C/--cd`** — confirmed (the only `-c` is `--continue`).
- `--resume` (`-r, --resume [value]`), `-c/--continue` — confirmed present.
- `--fork-session` — confirmed. `--session-id <uuid>` ("Use a specific session ID for the…") — confirmed.
- "No list command": no transcript-list subcommand in help; **new surface not in the spec (2.1.311)**: `agents`, `attach <id|name>` (open a background session), `logs`, `respawn`, `rm`, `stop|kill`, `--bg`, `auto-mode`, `gateway`, `ultrareview`, `purge`.
- Headless flags present: `--output-format` (json|stream-json, print only), `--input-format`, `--json-schema`, `--include-hook-events`, `--include-partial-messages`, `--permission-prompts`, `--no-session-persistence`.
- Injection flags present: `--settings <file-or-json>`, `--mcp-config`, `--allowedTools/--disallowedTools`, `--permission-mode`, `--append-system-prompt[-file]`, `--strict-mcp-config`.
- NOT checkable from help (still open): store path/format, retention `cleanupPeriodDays`, hook events and exit-code semantics, `--resume` in a different cwd, transcript hand-build.

**Codex CLI 0.164.0** (files `codex-help.txt`, `codex-*-help.txt`)
- `codex [OPTIONS] [PROMPT]`, `-C/--cd <DIR>` — confirmed.
- `codex resume [OPTIONS] [SESSION_ID] [PROMPT]` with `--last`, `--all`; `codex fork`; `codex exec resume|fork` — confirmed.
- `codex queue [OPTIONS] --thread <THREAD> --message <TEXT>` — confirmed (the register's `[R-H, V]` line is now CLI-confirmed).
- `archive`, `unarchive`, `delete` (session commands), `login status` — confirmed.
- `-c <key=value>`, `-p/--profile <CONFIG_PROFILE_V2>`, `--remote <ADDR>`, `--remote-auth-token-env` — confirmed present.
- app-server: `generate-json-schema` and `generate-ts` — confirmed; **new in this version**: `daemon` and `proxy` subcommands, `--listen stdio://|unix://|ws://IP:PORT|off`, `--ws-auth capability-token|signed-bearer-token`, `--startup-sign-out`, `--print-startup-config`. The spec's "local app-server daemon (`codex agents`)" is `codex app-server daemon` on 0.164.0 — candidate correction.
- NOT checkable from help (still open): whether the id can be chosen, hooks/trust semantics, `notify`, TUI notifications, rollout format.

**OpenCode 2.1.4** (files `opencode-help.txt`, `opencode-*-help.txt`)
- `--standalone`, `--server <url>`, `--auto`, `--prompt`, `--session/-s` ("Session ID to continue, or to create if it does not exist"), `--continue/-c`, `--fork` — confirmed.
- `opencode mini`, `opencode run [flags] [<message...>]` with `--format default|json` — confirmed.
- `session list|delete|export|import` (import from a JSON file or URL) — confirmed.
- `auth list|login|export|import` — confirmed. **SEC note:** `auth export` prints stored credentials including secrets; Shinatama must never call it (D5).
- NOT checkable from help (still open): HTTP API surface and SSE behavior, plugin/config merge, DB generations.

**Pi 1.2.0** (file `pi-help.txt`)
- `pi [options] [--] [@files...] [messages...]` — confirmed.
- `--session-id <id>` "Use exact project session ID, creating it if missing" — confirmed; also `--session <path|id>`, `--fork <path|id>`, `--session-dir`, `--continue/-c`, `--resume/-r` — confirmed.
- The spec's "not combinable with `--session`, `-c`, `-r`; combinable with `--fork`" is NOT visible in `--help` — check `docs/cli.md` in the runtime rows.
- `--mode text|json|rpc` — confirmed; RPC lifecycle (`disposition started|queued|handled`, `agent_settled` as the end of work) — confirmed in the bundled docs (`docs/rpc.md`, `docs/cli-integration.md`).
- `--extension/-e`, `--no-extensions/-ne`, `--no-context-files/-nc` — confirmed. **New levers not in the spec**: `--no-tools/-nt`, `--tools/-t`, `--exclude-tools/-xt` (answer-only candidates), `--tui-mode fullscreen|regular`, `--export`, `--approve/-a`, `--no-approve/-na`, `--system-prompt`, `--append-system-prompt`, `--offline`, and `pi auth print-api-key|print-bearer-token` (prints credentials for external clients — SEC-relevant).
- No attach/remote/server flag: confirmed (see PI5).

## Criterion

Not met yet: E1 passes only when the matching CL/CX/OC/PI fixture rows confirm every `[R]` line on the installed versions. Today's run confirms the `--help`-level surface of all four agents and adds evidence on the version drift; it does not run the runtime rows (spawn, resume, store, hooks), which need scratch homes and, for live-model steps, the O1 decision.

## Spec before / after

Before: engine 5.7 recorded as `[R]` on Claude 2.1.310, Codex 0.164.0, OpenCode 2.1.4, Pi 1.2.0.
After (2026-10-27, partial write-back applied at the user's request): engine 5.7 gained a dated re-check paragraph and four inline additions (Claude 2.1.311 drift + background-session surface; `codex app-server daemon` naming + schema capture; OpenCode `auth export` SEC note; Pi answer-only switches), item 5 was updated for Pi, and the register's section 4 header and CX6 row were corrected. The `[R]`→`[V]` conversion still waits: it belongs to the full E1, which needs the runtime rows on the installed versions.
