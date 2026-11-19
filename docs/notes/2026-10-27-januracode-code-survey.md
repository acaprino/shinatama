# JanuraCode code survey — reuse candidates for Shinatama

Date: 2026-10-27. Question: "JanuraCode already has import/export code between harnesses; can it be useful?"
Method: read-only inspection of the installed app at `D:\Software\JanuraCode` (Electron shell with `resources/app.asar`, 3 490 entries; `resources/web-dist` = OpenChamber 2.5.1; bundled `resources/opencode-cli/opencode.exe` = OpenCode 2.1.4). The asar header was parsed and text entries scanned by keyword; no user session content was read or copied. Scan/extraction scripts and extracted files are disposable (session temp only). Names below are as shipped, inside the asar unless a disk path is given.

## What is there (and useful)

1. **OpenCode session import/export — official client and protocol schema** (reference for OC2/OC4; the same v2.1.4 generation our probes target):
   - `@opencode/protocol/dist/groups/session.js`: `POST /api/experimental/session/import`, payload `{info, messages, location}`; errors include a **409 Conflict**; "Import a projected session transcript at the requested location. Accepts transcripts exported by earlier releases. If parentID is supplied, the parent session must already exist; **import parents before children**."
   - `GET /api/experimental/session/:sessionID/export?sanitize=` → `{data: PublicSessionTransfer}`; `PublicSessionTransfer = {info: PublicSessionInfo, messages: PublicSessionMessage[]}` ("Export a complete projected session transcript").
   - `@opencode/client/dist/chunks/*` and the Electron bundle's generated client (`/dist-bundle/main.mjs`) expose both endpoints with declared statuses 400/401/404/409.
2. **A working managed-OpenCode host** — `@openchamber/web/server/lib/opencode/*` (MIT): process lifecycle with port allocation, a **local server password**, a supported-version gate, graceful shutdown that tears the child down, env/PATH handling, config v2, MCP. Reference for the sidecar/SC rows and OC4.
3. **OpenCode store migration V1→V2** — `lib/opencode/v1-migration-topup.js` + the re-arm in `lifecycle.js`: OpenCode 2.x migrates the legacy `session`/`message`/`part` tables into `session_v2`/`session_message` once and writes `{"phase":"completed"}` under `migration.v1-v2` in `kv`; the module hands OpenCode a resume cursor **before spawning the managed process** so OpenCode's own `run()` imports sessions created by 1.x afterwards. Three rules (verified by its author against v2.0.8 `v1-migration.bun.ts`): never clear the migration row; never revisit a session that saw v2 activity; trigger only on 1.x activity since the completion stamp. OpenChamber never writes session rows itself. Directly relevant to OC3/OC10.
4. **V2 event semantics** — `lib/event-stream/translate-v2.js` (openchamber server). Its header states that v2 emits **no `session.status` and no `session.idle` of its own**: live status must be synthesized from `session.execution.started|succeeded|interrupted|failed`, `session.retry.scheduled` (status `retry`) and `session.step.started`; a v2 turn is a sequence of steps and each `session.step.ended` becomes an assistant `message.updated`, only the last carrying `finish: "stop"`. This is a **lead for OC5/8.1** — our register entry for OpenCode signals says `session.status` (busy/idle) with `session.idle` deprecated; the wire should be re-checked in the OC5 fixture before the adapter relies on either.
5. **Sign-in detection reference** — `lib/opencode/claude-cli-auth.js` (MIT): `claude auth status --json`, 6 s timeout, strips `ANTHROPIC_*`/`CLAUDE_CODE_OAUTH_TOKEN` from the child env, resolves the binary via `where` on Windows. Matches our own probe (CL13 record).
6. **No cross-harness session translator found.** No Claude/Codex/Pi ↔ OpenCode conversion exists in the asar, the web bundle or the `opencode.exe` strings; "harness"/"externalAgent" have only generic hits, and the `claude`/`codex` hits are auth, model-family and skills mentions. What exists is OpenCode's own interchange between installations/releases, the V1→V2 store migration, and a markdown-export chunk in the UI. A true cross-agent transfer remains Shinatama's own work — consistent with the 2026-10-27 landscape research.

## Licenses and reuse

- Bundled OpenCode and OpenChamber are **MIT** (`LICENSE.opencode.txt`, `LICENSE.openchamber.txt`; attribution required) — compatible with X3's MIT target.
- The JanuraCode Electron bundle is Tri-Tech's own product; reuse of its code is its owner's decision.

## Suggested first uses

- OC2/OC4: check our endpoint findings against `@opencode/protocol` and `@opencode/client` while the scratch round-trip runs; the declared 409 matches OC2's "409 on an existing id".
  **Confirmed (2026-10-27, OC2 record):** the run against the wire matched the client/protocol contract — chosen `ses_` id, 409 `ConflictError`, lossless round trip for `user`/`assistant`+tool/`compaction`/`system`/`synthetic`/`shell`, and the finding that `/synthetic` with `resume:false` is an inbox item, not a transcript message.
- OC3/OC10: read `v1-migration-topup.js` and the lifecycle re-arm before designing the legacy reader.
- Sidecar/SC rows: compare against `@openchamber/web/server/lib/opencode` (version gate, local password, env, shutdown).
- OC5/8.1: treat translate-v2's statement as a lead to verify on the wire.
- CL13/CX11/OC8/PI9: the CLI status commands are usable (`claude auth status --json`; `codex login status` on stderr; `pi auth check --json --no-refresh`); **OC8 caveat (2026-10-27): `opencode auth list` turned flaky on the real home** ("Location plugins did not finish activating") — prefer the HTTP route on a managed server (OC8 record); never `opencode auth export` or `pi auth --credentials`.
