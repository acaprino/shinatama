# OC2 — OpenCode session interchange: import/export round trip

Item: OC2 (G0) — "`session import <file> --directory D`: the exact interchange format, round trip with `session export`, tool items preserved or not, the 409 on an existing id." Pass when: "Round-trip law of 5.5.1 holds or the losses are listed."
Status: **passed** — all four criteria recorded on 2.1.4 in scratch DBs; write-back applied to engine 5.4 (OpenCode stage-2 bullet).
Date: 2026-10-27.
Runner: JanuraCode agent; no login, no model calls, cost 0.
Machine: Windows 11 Home build 26200 (NT 10.0.26200); PowerShell 5.1.26100.9711.
Agent: opencode v2.1.4 (npm `@opencode/cli`; same version bundled in JanuraCode).
Command: `powershell -NoProfile -ExecutionPolicy Bypass -File verify/OC2/run.ps1` (rerunnable without edits; resets the scratch root, starts `opencode serve` twice on scratch DBs with a scratch password, then drives HTTP and the CLI).
Raw output: `verify/OC2/out/2026-10-27/` (`create-a.json`, `synthetic-*.json`, `shell.json`, `export-a.json`, `export-a-cli.*`, `import-c-cli.out.txt`, `export-c-cli.out.txt`, `import-b.json`, `export-b.json`, `import-b-again-409.json`, `rich-fixture.json`, `import-rich-b.json`, `export-rich-b.json`, server logs).
Scratch: `%LOCALAPPDATA%\shinatama-verify\OC2` (`OPENCODE_DB`, `XDG_*`, password via `OPENCODE_PASSWORD`).

## Result

1. **Interchange format.** `{info, messages[]}` where `info` is `Session.Info` with a public location ref and `messages` are the wire `PublicSessionMessage` union: `agent-switched`, `model-switched`, `location-switched`, `user`, `synthetic`, `system`, `skill`, `shell`, `assistant`, `compaction`, `idle` (schema extracted from the official `@opencode/protocol`/`@opencode/schema` shipped with 2.1.4). Example `shell` message: `{id, metadata, time, type:"shell", shellID, command, status:"exited", exit:0, output:{output,cursor,size,truncated}}`.
2. **HTTP API** (`opencode serve --hostname 127.0.0.1 --port …`; auth required: Basic `opencode:<password>`, env `OPENCODE_PASSWORD`; the log prints the password when the env is absent):
   - `POST /api/session {id}` → 200 with the **chosen** `ses_` id (no fallback needed);
   - `POST /api/session/:id/shell {command}` → 204;
   - `GET /api/experimental/session/:id/export` → 200 `{data:{info,messages}}`;
   - `POST /api/experimental/session/import {info,messages}` → 200;
   - a second import of the same id → **409** `{"_tag":"ConflictError","message":"Session already exists: <id>","resource":"<id>"}`.
3. **Round trip A → B.** Session created in scratch DB A with one `shell` message; imported into scratch DB B; exported from B: messages normalized **equal** (True), same count and type.
4. **Rich crafted fixture** (`user`, `assistant` with `text` + `tool`, `compaction`, `system`, `synthetic`): import → 200; export returns **5/5** messages of the same types, with the tool part fully preserved — `{type:"tool", id, name:"read", state:{status:"completed", input:{filePath:"notes.txt"}, content:[{type:"text", text:…}]}, time:{…}}` — and the compaction summary, system text and synthetic text intact. **No loss found for these kinds.**
5. **Loss / caveat found.** `POST /api/session/:id/synthetic` with `resume:false` is accepted (200) but is an **inbox item, not a transcript message**: it does not appear in the export until it is executed. `POST /api/session/:id/shell` instead produces a transcript message immediately.
6. **CLI.** `opencode session export <id> --standalone` prints the same JSON; `opencode session import <file> --directory <D> --standalone` → `Imported session: <id>`, exit 0, and the re-export from that DB keeps the id and messages (`projectID` is derived per DB/location, so it changes — expected).

## Criterion

Met: format, round trip, tool items and 409 are all recorded, and the losses are listed (inbox semantics of `/synthetic resume:false`; nothing lost in import/export itself).

## Spec before / after

Before: 5.4 listed OpenCode's official import/export as `[R-S]` (read, not run).
After: 5.4 carries the dated result above. Related lead kept in the register: Q11 (v2 event vocabulary).

## Open

- `--sanitize` on export not exercised.
- "Accepts transcripts exported by earlier releases" (`SessionLegacy.messages` upgrade path) not exercised.
- Attachments (`files` in a `user` message) and `native` provider fields were not in the crafted fixture.
- `session.messages` paging endpoint and `session/list` filters not exercised here.
