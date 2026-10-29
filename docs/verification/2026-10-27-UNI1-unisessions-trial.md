# UNI1 — UniSessions trial (external converter; not a register row)

Topic: **vibheksoni/session-export** ("UniSessions") — an MIT, Python 3.11+ SDK/CLI that converts sessions between Codex, Claude Code, Pi, OpenCode, Devin, Factory, Windsurf Cascade, Grok (Build + grok-cli), Freebuff Desktop, OMP (oh-my-pi), Command Code and T3 Code, with trace export (HuggingFace STS / OpenAI / ShareGPT) and an MCP chat-search server. PyPI `unisessions` **0.5.0** (uploaded 2026-10-16; deps `tiktoken`, `cryptography`; extras: `mcp`, `fast`). The GitHub `main` README is ahead of the release (132 directions incl. Command Code vs 110 in the PyPI README).
Why it matters: the 2026-10-27 landscape research and the JanuraCode survey found no public cross-harness session converter; this one claims exactly that, for the pairs Shinatama cares about (Claude/Codex/Pi/OpenCode).

Date: 2026-10-27. Runner: JanuraCode agent; no login, no model calls, cost 0.
Machine: Windows 11 Home build 26200; Python 3.12.10 (venv under `%LOCALAPPDATA%\shinatama-verify\unisessions`); opencode v2.1.4 (npm binary) for the import test.
Method: scratch root `%LOCALAPPDATA%\shinatama-verify\unisessions`; every command used the tool's custom path flags (`--claude-session-dir`, `--pi-session-dir`, `--pi-dcp-home`, `--opencode-session-dir`); real homes were not written (one dry-run showed a destination under `~/.pi` but was not executed). A synthetic Claude transcript (5 lines: permission-mode, user, assistant, compact boundary, compact summary) was written from the shape used in the project's own test file.

## Results

1. `list claude` parses the fixture: one session, "3 messages", cwd `C:\Projects\test`.
2. `claude-to-opencode --write` → one file in the **old v1 export shape** (`info`: `id`, `slug` (a slug derived from the first message), `projectID: "global"`, `directory`, `title`, `version: "session-export"`, `time`; `messages`: `{info, parts}` with `providerID: "session-export"`, `path`, `tokens` all zero).
3. **opencode 2.1.4 rejects it**: CLI `session import <file> --directory <D> --standalone` → `Failed to import session: Bad Request` (exit 1); direct HTTP `POST /api/experimental/session/import` → **400** `{"_tag":"InvalidRequestError","message":"Missing key at [\"info\"][\"cost\"]","kind":"Payload"}`.
4. Patching only `info` to the v2 fields (`cost`, `tokens`, `location.directory`) moves the failure to the messages: **400** `{"_tag":"InvalidRequestError","message":"Expected Session.Message.Info at [\"messages\"][0]","kind":"Payload"}` — the v1 `{info, parts}` message shape is not accepted either.
5. `opencode-to-pi` on a **v2** export (the OC2 `rich-fixture.json`, renamed to `<id>.json` as the store indexes by file stem): no crash, but the dry-run plan says `records: 1`, `source cwd:` empty — the v2 messages are silently dropped (its OpenCode extractor targets the v1 shape).
6. `claude-to-pi --write` → a correct Pi v3 JSONL: session header, user and assistant text, two `type:"compaction"` entries with `firstKeptEntryId`; the `STX-UNI-MARKER` strings are preserved.
7. Idempotency: two `--write` runs of the same source produced **two files with different random `ses_` ids**; `--on-conflict` compares destination paths, which embed the id, so re-runs cannot deduplicate for OpenCode.

Evidence: `verify/UNI1/out/2026-10-27/` — `claude-fixture.jsonl`, `unisessions-opencode-export.json`, `unisessions-pi-export.jsonl`, `patched-import.json`, `http-import-body-1.json` (info.cost error), `http-import-body-2.json` (messages[0] error), `cli-import-badrequest.out`.

## Assessment

- **Useful as a candidate engine or reference for the Claude/Codex/Pi pairs** (and the other non-OpenCode targets): the mechanics work, the CLI is dry-run-by-default, conflicts are managed, and the format mapping (esp. compaction per harness) is documented in `docs/data-fidelity.md`. MIT — compatible with X3.
- **Not usable as-is for OpenCode on 2.1.4**: both directions need a bridge — `info` fields (`cost`, `tokens`, `location`) **and** a v1-parts → v2-message conversion, i.e. essentially the work already specified in 5.4/OC2. Alternatively, contribute the v2 shape upstream (their README points at `opencode import`, a command 2.1.4 no longer has).
- Notes: the tool reads transcripts (secrets possible) and some targets write directly into native stores (stage-3 risk) — keep scratch-only when writing. Version pin 0.5.0 until `main` ships; the README's counts/test numbers are ahead of the release.
- Verified on synthetic data only; each direction still needs its own fixture and round-trip law before adoption.
