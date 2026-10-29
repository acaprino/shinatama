# P20 — Inherited agent markers

Item: P20 (G1): "Inherited agent markers: the exact variables each agent sets for its children; what a hosted agent does differently when it inherits them; that removing exactly those names gives fresh-terminal behaviour and removes none of the user's own settings." Pass when: "Exact list per agent written into its adapter; with the removal, behaviour equals a fresh terminal; no user setting removed."
Status: **running (partial)**. Windows only. The runs use the mock model in print or exec mode, not a real nested interactive start.
Date: 2026-10-28.
Runner: Claude Code session (Opus 5.5), one worker per agent. Mock model, no login, names only (no values except fixed marker lists), cost 0.
Machine: Windows 11 Home build 26200; PowerShell 7; Node v24.14.1.

**Method.** A copy of the H27 mock gets one extra trigger, which makes the agent's shell tool print environment variable *names*. The same mock session then runs three times: clean, with the agent's own markers inherited, and with Claude's markers inherited (`CLAUDECODE=1`, `AI_AGENT=claude-code`, `CLAUDE_CODE_ENTRYPOINT`). The runs are compared on captured model requests, files written, stderr and exit codes. Separately, the binary or source is searched for marker names.

## Codex 0.164.0 (`verify/P20/codex/run.ps1`, `compare.mjs`, `server.local.mjs`)

- **Set for shell-tool children, observed:**
  - always: `CODEX_CI=1`, `CODEX_THREAD_ID`, `CODEX_SESSION_ID` (both equal the thread id), `CODEX_TOOL_CALL_ID`, `CODEX_VERSION`, `CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS`; also `TERM=dumb`, `LANG`/`LC_ALL`/`LC_CTYPE=C.UTF-8`, `COLORTERM=`, `PAGER`/`GIT_PAGER`/`GH_PAGER=cat`;
  - with the unelevated sandbox, in addition: `CODEX_SANDBOX_NETWORK_DISABLED=1`, `SBX_NONET_ACTIVE=1`, `NPM_CONFIG_OFFLINE=true`, `CARGO_NET_OFFLINE=true`, `PIP_DISABLE_PIP_VERSION_CHECK=1`, `PIP_NO_INDEX`, `GIT_ALLOW_PROTOCOLS=`, `GIT_HTTP_PROXY`, `GIT_HTTPS_PROXY`, `GIT_SSH_COMMAND`, `LESS`.

  Codex removes nothing it inherited. The sandboxed dump needed the Store `pwsh` dropped from `PATH`, because the sandbox cannot start it (`CreateProcessAsUserW failed: 5`).
- **Behaviour with markers inherited: no observable difference.** The first model request was identical (headers, instructions, environment context). Later requests differed only in random chunk ids. stderr, exit codes, home file names and egress were identical.
- **Pass-through:**
  - A new Codex overwrites `CODEX_THREAD_ID`, `CODEX_SESSION_ID` and `CODEX_TOOL_CALL_ID`.
  - It passes `CODEX_SANDBOX_NETWORK_DISABLED=1` and `SBX_NONET_ACTIVE=1` through unchanged, even when it is not sandboxed. A child would then believe it has no network.
  - It passes `CLAUDECODE` and `AI_AGENT` through to its children.
- **Other agents' names in `codex.exe`:** `CLAUDECODE`, `AI_AGENT`, `OPENCODE_CLIENT`, `CLAUDE_CODE_` and `GEMINI_CLI` do not appear. Only `CLAUDE_PLUGIN_ROOT`/`CLAUDE_PLUGIN_DATA` appear, for plugin hooks.

## Pi 1.4.0 (`verify/P20/pi/run.ps1`, `compare.mjs`, `server.p20.mjs`, `server.p20.diff`)

- **Set for the whole process (every child inherits):** `PI_CODING_AGENT=true` and `AI_AGENT=pi`, which overwrites an inherited `AI_AGENT` (`dist/cli/setup.js:5-6`, `dist/rpc-entry.js:6-7`; documented in `docs/environment-variables.md`).
- **Set for bash-tool children:** `PI_SESSION_ID`, `PI_SESSION_FILE`, `PI_PROVIDER`, `PI_MODEL` and `PI_REASONING_LEVEL`. Pi deletes inherited copies first (`core/tools/bash.js:136-153`). `PATH` also gets `<agentDir>\bin` prepended (`utils/shell.js:147`); a name-based removal cannot undo that.
- **Observed dump:** the child had exactly these 7 names more than the parent, plus Git Bash's own additions (Pi's bash tool on Windows is Git Bash).
- **Behaviour with markers inherited: no difference.** Three variants were compared: clean; all 7 Pi markers plus the outer Pi's `bin` first in `PATH`; and Claude's markers. They were identical in requests, files, output and stderr. The outer session file named by the inherited `PI_SESSION_FILE` was not touched. Pi reads none of its own markers, and passes `CLAUDECODE`/`CLAUDE_CODE_ENTRYPOINT` through unchanged.
- **Other agents' names:** only in a bundled library's agent-detection table (`dist/bundle/chunks/jiti-loader-GL4HJFAD.js`): `CLAUDECODE`, `CLAUDE_CODE`, `CODEX_SANDBOX`, `CODEX_THREAD_ID`, `OPENCODE`, `AI_AGENT`. (inference) No behaviour of Pi depends on them.

## Claude Code 2.1.312 (`verify/P20/claude_code/run.ps1`, `bench.ps1`, `compare.mjs`, `server.p20.mjs`, `dump-env.mjs`, `mcp-env.mjs`, `pipe-listener.mjs`)

- **Set for its children (observed, no markers inherited):**
  - Bash tool: `AI_AGENT CLAUDECODE CLAUDE_CODE_CHILD_SESSION CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_EXECPATH CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN CLAUDE_CODE_SESSION_ATTENDED CLAUDE_CODE_SESSION_ID CLAUDE_EFFORT CLAUDE_PID SHELL`.
  - Hooks: the same minus `CLAUDE_CODE_EXECPATH`, plus `CLAUDE_PROJECT_DIR`; SessionStart also gets `CLAUDE_ENV_FILE`.
  - MCP server: `AI_AGENT CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_MESSAGING_SOCKET CLAUDE_CODE_MESSAGING_TOKEN CLAUDE_CODE_SESSION_ID CLAUDE_PROJECT_DIR SHELL`.
  - The binary's own list of child variables also has `GIT_EDITOR`, `TRACEPARENT` and `CLAUDE_CODE_INVOKED_SKILLS`.
- **`-p` turns with 11 marker names inherited** (invented values; the messaging socket was a dummy named pipe): no observable difference. The 10 requests, headers, system prompt, 26 tools, files and stderr were all the same.
- **Interactive turn with markers inherited: transcript saving is off.** The turn reached the model but wrote **no transcript and no `history.jsonl`**; the clean run wrote both. The binary's message reads: "Transcript saving is off — inherited CLAUDE_CODE_CHILD_SESSION marker · restart with CLAUDE_CODE_FORCE_SESSION_PERSISTENCE=1". A hosted Claude that inherits the marker **cannot be resumed and is invisible to search**.
- **Messaging socket:** 0 connections in both runs.
- **Pass-through:** inherited names reach the children unchanged.
- **Also changed by "child session"** (strings, inference): `-y/--accept-command` is refused, the skill-proposal tool is off, and the run counts as unattended. `AI_AGENT` is overwritten only when it starts with `claude-code_`.
- The H27 `env.ps1` removes `CLAUDE*`/`ANTHROPIC*` but not `GIT_EDITOR` or `AI_AGENT`, which came through from the outer session.

## OpenCode 2.1.4 (`verify/P20/opencode/run.ps1`)

- **Set for the programs it starts** (the binary and the shell-tool dump agree): `AGENT=1`, `OPENCODE=1`, `AI_AGENT`, `OPENCODE_SESSION_ID`, `OPENCODE_TERMINAL=1` and `TERM=xterm-256color`. `AI_AGENT` is set only if absent, so it defaults to `opencode`. The terminal path also sets `LC_ALL`, `LC_CTYPE` and `LANG=C.UTF-8`. `OPENCODE_CLIENT`, the marker harness/cli relies on, is set only by ACP and Desktop.
- **Own markers inherited: no difference.** Requests (system hash `0671CFC8617F`), tools and child variable names are the same. An inherited `OPENCODE_SESSION_ID` is overwritten.
- **Claude's markers inherited: same requests** (`5B4D8B9C805A`) and the same `~/.claude` skill loading. Those skills load whenever `OPENCODE_DISABLE_CLAUDE_CODE` is unset, whatever the markers say. But the markers pass through to children, and **`AI_AGENT` stays `claude-code`** because OpenCode only fills it when absent.
- **From the binary only, not run:** the TUI connects to `CLAUDE_CODE_SSE_PORT` (Claude's editor-integration port) when it is inherited. Removing that name is required.
- Other agents' names (`CODEX_SANDBOX`, `GEMINI_CLI`, `CURSOR_AGENT`) appear only in embedded agent-detection lists.
- **Read leak found on the way:** a private OpenCode loads the user's `~/.claude` skills unless `OPENCODE_DISABLE_CLAUDE_CODE` is set (D15 concern).

## Criterion

Partly met. The rule is confirmed: one Claude marker silently disables session saving. Removal lists per adapter (exact names):

| Adapter | Remove before launch |
|---|---|
| Claude Code | `CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_SESSION_ATTENDED`, `CLAUDE_CODE_ENTRYPOINT`, `CLAUDE_CODE_EXECPATH`, `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_CODE_MESSAGING_TOKEN`, `CLAUDE_CODE_INVOKED_SKILLS`, `CLAUDE_CODE_SSE_PORT`, `CLAUDE_EFFORT`, `CLAUDE_PID`, `CLAUDE_PROJECT_DIR`, `CLAUDE_ENV_FILE`, `GIT_EDITOR` when it points at Claude, `TRACEPARENT` |
| Codex | `CODEX_CI`, `CODEX_THREAD_ID`, `CODEX_SESSION_ID`, `CODEX_TOOL_CALL_ID`, `CODEX_VERSION`, `CODEX_APPLY_PATCH_PRESERVE_LINE_ENDINGS`, `CODEX_SANDBOX`, `CODEX_SANDBOX_NETWORK_DISABLED`, `SBX_NONET_ACTIVE` |
| OpenCode | `AGENT`, `OPENCODE`, `OPENCODE_SESSION_ID`, `OPENCODE_TERMINAL`, `OPENCODE_CLIENT` |
| Pi | `PI_CODING_AGENT`, `PI_SESSION_ID`, `PI_SESSION_FILE`, `PI_PROVIDER`, `PI_MODEL`, `PI_REASONING_LEVEL` |
| shared | `AI_AGENT` (set by Claude, OpenCode and Pi) |

Shinatama removes the union of all four lists before launching *any* agent, because each agent passes the others' markers through. `GIT_EDITOR` is removed only when its value is Claude's own editor helper, since the user may have set it. Values are not read otherwise. Open: the sandbox variables Codex adds (`NPM_CONFIG_OFFLINE`, `PIP_NO_INDEX` and the others) are generic names a user may set. Shinatama removes them only when `SBX_NONET_ACTIVE` shows they came from a Codex sandbox. Pi's `<agentDir>\bin` prepended to `PATH` cannot be removed by name. Not yet run: a real nested interactive start for Codex, OpenCode and Pi; macOS and Linux.

## Spec before / after

Before: engine 3 listed Claude's markers from one shell's environment and marked the effect and the other agents' lists [U].
After: engine 3 states the effect [V 2026-10-28, P20] (transcript saving off for Claude), and the union rule; adapter 5.5 A's `inherited_markers()` lists come from the table above.
