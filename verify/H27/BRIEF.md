# H27 bench — brief for the per-harness runs (2026-10-27)

Goal: produce **authentic native sessions** of one harness, and a way to **resume any session
of that harness with a prompt**, both against a local mock model so that nothing costs money
and every request the harness sends to "the model" is captured. The captures are the evidence
of what a destination model actually receives after an import (the resume context).

Register row: H27 (`docs/verification-register.md`). Design context: `docs/technical-design.md`
5.3, 5.4, 5.4.1, 5.7. Earlier records with working scripts to reuse:
`docs/verification/2026-10-27-{CX4,OC2,PI5,PI11,CL13}.md` and `verify/<row>/run.ps1`.

## Hard rules

1. **Never touch a real home.** Every run sets the harness's own root variable to a scratch
   folder under `%LOCALAPPDATA%\shinatama-verify\H27\<harness>\` (`CLAUDE_CONFIG_DIR`,
   `CODEX_HOME`, `PI_CODING_AGENT_DIR`, `OPENCODE_DB` + `XDG_*`). Before the first run, prove it
   (e.g. list the scratch root afterwards, check the real store's newest file mtime did not move).
2. **No real model call, no login, no credential read.** The harness must talk only to the mock
   (`http://127.0.0.1:<port>`); API keys are the literal `h27-dummy`. If a run shows traffic to a
   real endpoint, stop and report.
3. Do **not** edit `verify/H27/mock/server.mjs` (other runs use it at the same time). If it needs a
   change, copy it to `verify/H27/<harness>/server.local.mjs`, change the copy, and report the diff.
4. Scripts are PowerShell 5.1-compatible, rerunnable without edits, and print where they wrote.
   Generous timeouts (Pi starts slowly: see PI11).
5. Do not commit. Do not touch other harnesses' folders.

## The mock model

`node verify/H27/mock/server.mjs --port <port> --capture <file.jsonl>` — speaks Anthropic Messages
(`POST …/v1/messages`, streaming or not, plus `count_tokens`), OpenAI Responses (`POST …/responses`)
and Chat Completions (`POST …/chat/completions`), `GET …/models`. Each request is one JSON line in
the capture file (`{n, ts, method, url, headers, body}`, auth headers redacted). Replies are scripted
from the last user turn:

| last user turn contains | reply |
|---|---|
| `H27-TOOLS` + two absolute `*.txt` paths | reasoning (`thinking` with signature `h27-mock-signature`, or Responses `reasoning` with `encrypted_content`) + text `H27-PRE…` + **two parallel calls** of the read tool the harness offered (`read`/`Read`/`read_file`; else a shell tool with `Get-Content -Raw '<path>'`) |
| tool results | `H27-FINAL: both files were read.` |
| `H27-HANG` | starts streaming, never finishes (for an interrupted turn) |
| summar / compact / checkpoint / handoff … | `<summary>H27-SUMMARY: …</summary>` |
| anything else | `H27-REPLY: <echo>` |

Ports: claude_code 4131, codex 4132, opencode 4133, pi 4134.

## Deliverables (in `verify/H27/<harness>/`)

1. `env.ps1` — dot-sourced; sets the scratch home variables and whatever config points the
   harness at the mock (write config files into the scratch home, never the real one).
2. `gen-source.ps1` — resets the scratch home, starts the mock (capture
   `…\H27\<harness>\capture-gen.jsonl`), creates `…\H27\<harness>\work\alpha.txt`
   (`ALPHA-CONTENT-42`) and `beta.txt` (`BETA-CONTENT-17`), then in ONE session, non-interactively:
   - T1: `H27-TOOLS read <abs alpha.txt> and <abs beta.txt>` (expect two parallel reads, results, `H27-FINAL`);
   - T2 (same session, resumed): `H27-T2 say hello`;
   - the harness's **own compaction**, by whatever non-interactive route exists (slash command in
     print mode, RPC/app-server/HTTP method, or auto-compaction with a low threshold);
   - T3 (same session): `H27-T3 after compaction`;
   - T4 if feasible: `H27-HANG`, killed after ~15 s → an interrupted last turn on disk.
   Copies the native session (file, or OpenCode `session export` JSON plus the rows if useful) to
   `verify/H27/out/2026-10-27/<harness>/source/`, with the capture file beside it. Prints the
   session id.
3. `resume-capture.ps1 -SessionId <id> -Prompt <text> -Tag <name>` — resumes an EXISTING session
   of the scratch home (it will be called later on sessions written by txcript, so it must not
   reset anything), sends the prompt non-interactively, saves the mock capture of that run to
   `verify/H27/out/2026-10-27/<harness>/resume/<Tag>.capture.jsonl` and the harness's
   stdout/stderr/exit code beside it. Test it on the T1–T4 source session.
4. `FINDINGS.md` — facts, each marked [V] (seen in this run) with the file it is seen in:
   - versions; exact config that makes the harness use the mock; commands for new turn, resume
     with a prompt, compaction;
   - native record shape (verbatim JSON, shortened) for: user text, assistant text,
     reasoning/thinking (is the signature / encrypted content stored?), tool call, tool result,
     how the **two parallel calls** are grouped, the **compaction record(s)** (exact fields:
     summary text, boundary, what is kept), the interrupted turn, session id / file naming /
     title / cwd / model fields;
   - from the captures: on resume, what the model receives — number of messages, whether the
     pre-compaction turns are sent or only summary + tail, whether reasoning (signature /
     encrypted content) is sent back, where the resume prompt lands;
   - anything that surprised you.

Final report back (≤ 40 lines): commands, compaction record shape, what the resume request
contains, open problems.
