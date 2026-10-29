# CX4 — `externalAgentConfig/detect` and `/import`, with the app-server schema run

Item: CX4 (G0) — "`externalAgentConfig/detect` and `/import`: request and response shape (from `codex app-server generate-json-schema`); import of one named synthetic Claude session; resumable; tool items kept; the 30-day window; a session still being appended."
Status: **passed** (mechanics proven in a scratch profile; write-back applied to engine 5.4 and 6.2).
Date: 2026-10-27.
Runner: JanuraCode agent, on the user's machine; no login, no model calls, cost 0.

Machine: Windows 11 Home build 26200 (NT 10.0.26200); PowerShell 5.1.26100.9711; Node v24.14.1.
Agent: codex-cli 0.164.0, resolved as a **native installer binary** `C:\Users\alfio\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe` (not an npm shim; relevant to the P16 spawn matrix).
Commands (rerunnable without edits):
- `powershell -NoProfile -ExecutionPolicy Bypass -File verify/CX4/run.ps1` — schema bundles.
- `powershell -NoProfile -ExecutionPolicy Bypass -File verify/CX4/run-runtime.ps1` — runtime probe (`verify/CX4/app-server-probe.mjs`), scratch reset each run.
Raw output: `verify/CX4/out/2026-10-27/` (`schema/` 320 files, `schema-experimental/` 458 files, `runtime-092655.txt` v1, `runtime-092821.txt` v2/v3).
Scratch: `%LOCALAPPDATA%\shinatama-verify\CX4\{codex-home, profile, proj}`. The user's real homes are never read by v2/v3: `USERPROFILE`, `HOME`, `CLAUDE_CONFIG_DIR` and `CODEX_HOME` all point inside the scratch root.

## Results — schema (run.ps1)

1. `codex app-server generate-json-schema --out <dir>` (and `--experimental`) run to completion with a scratch `CODEX_HOME`; no session started.
2. Types captured: `ExternalAgentConfigDetectParams/Response`, `externalAgentConfig/detect|import`, `import/progress|completed` notifications, `import/readHistories`, `import/recordHistory`, `ExternalAgentConfigImportParams/Response`, `ExternalAgentConfigMigrationItem` (types `SESSIONS`, `AGENTS_MD`, `CONFIG`, `SKILLS`, `PLUGINS`, `MCP_SERVER_CONFIG`, `SUBAGENTS`, `HOOKS`, `COMMANDS`, `MEMORY`), `SessionMigration{cwd, path, title}`; also `thread/inject_items|list|resume|read`.
3. Caveat: with `CODEX_HOME` under `%TEMP%`, codex prints "WARNING: … Refusing to create helper binaries under temporary dir" (non-fatal here; later probes keep the scratch home outside `%TEMP%`).

## Results — runtime probe (run-runtime.ps1; scratch profile, no login)

Fixtures: three synthetic Claude sessions, each with a marker, in `profile\.claude\projects\<flattened-cwd>\<uuid>.jsonl` — [A] a named current session with a `Read` tool call and result; [C] the same shape 40 days old (timestamps and mtime); [D] a session appended by a writer every 400 ms while detect/import run.

- **[A] Import of one named session.** `detect(cwds=[proj], includeHome=false)` → 0 items (repo-scoped detection does not see home-stored Claude sessions). `detect(includeHome=true, maxSessionAgeDays=90)` → 1 `SESSIONS` item covering the fixtures. Importing only the entry for [A]: success, `target` thread `01a17c79-…-26ad16220480`, title = first user message. `thread/list(cwd=proj)` shows it; `thread/resume {threadId}` loads it from disk (`…\sessions\2026\10\27\rollout-…-26ad16220480.jsonl`, 1 turn). Rollout written: 26,392 bytes, marker present.
- **[B] Shape of thread history.** `thread/items/list` answers `{"code":-32601,"message":"thread/items/list is not supported yet"}` on 0.164.0 — the method exists in the schema but is not implemented; the rollout file is the readable record.
- **[C] The 30-day window.** With the default age limit, the 40-day session is **not detected**; with `maxSessionAgeDays: 90` it is detected and imports successfully. So the window is the **detector's filter, configurable per call** — not a hard import block when the item is supplied. After its import, the session is no longer detected (same for [A]): detection skips sessions already in the import history (inferred from `import/readHistories`; not yet probed directly).
- **[D] Session still being appended.** The writer added 4 lines during detect + import; detection found the session and the import completed **successfully** (rollout 25,476 bytes), consistent with a snapshot taken at import time.
- **Tool fidelity (declared loss).** The imported thread stores only `response_item/message` entries: the Claude `tool_use` arrives as text markers inside the assistant message —
  `[external_agent_tool_call: Read] file: …\notes.txt [/external_agent_tool_call]` — and the Claude `tool_result` turn as an assistant message with `[external_agent_tool_result] … [/external_agent_tool_result]`. **No native `function_call` items.** This answers engine 5.4's open question ("tool items or text"): text.
- **Rollout metadata observed:** `session_meta` with `originator` = the app-server client name (`shinatama-verify-cx4` here), `source: "vscode"`, `cli_version: "0.164.0"`; notifications seen: `externalAgentConfig/import/progress|completed`, `thread/status/changed`, `thread/tokenUsage/updated`, `thread/goal/cleared`, `remoteControl/status/changed`.
- **Network observation (note for X4/SEC):** during the [C] import the app-server logged `401 Unauthorized` on a websocket attempt to `wss://api.openai.com/v1/responses` from the signed-out scratch home; non-fatal for the import.
- **Isolation finding (v1 log, kept for the record):** the first probe did not override `USERPROFILE`; `detect(includeHome=true)` then returned counts from the **real** home (1 CONFIG, 1 MCP_SERVER_CONFIG, 1 SKILLS, 1 PLUGINS, 1 SESSIONS — counts only, nothing copied) and the import of an explicit path failed with `session_not_detected` / stage `session_missing`. Lesson: import requires the session to come from detection, and detection follows the invoking profile; tests must override the profile.

## Criterion

Met. Each criterion is recorded; the strategy this establishes: **Claude → Codex `native_import` (stage 2) is viable**, with the declared loss (tool calls as text markers; `tool_result` as an assistant message). Write-back applied: engine 5.4 (dated result paragraph) and 6.2 (pair note).

## Spec before / after

Before: 5.4 listed the import behaviour as open questions; 6.2 kept `native_import` conditional.
After: 5.4 carries the dated answers above; 6.2 notes Claude → Codex as the first pair with proven `native_import` mechanics. Other pairs unchanged.

## Open

- `codex resume` in the interactive CLI (a model turn) was not run: it needs a login (O1) and is not required to prove the mechanics.
- The exact default value of the age window (40 days excluded by default; not measured against 30 exactly) and the `import/readHistories` exclusion rule by direct probe.
- Stability of the text-marker syntax across Codex versions (fixture gate).
