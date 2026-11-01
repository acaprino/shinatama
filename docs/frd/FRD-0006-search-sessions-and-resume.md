# FRD-0006: Search, sessions, resume and the command line

| | |
|---|---|
| Version | 1 (Markdown record; JSON record version 1 at `c84d28e`) |
| Status | Proposed |
| Date | 2026-11-01 |
| Sources | functional-design goal 3, 4, 5.3, 5.16 (sessions surface); technical-design 4.2 (`session_index`, `session_text`), 5, 5.1 to 5.3, 5.5 B, 5.5.1, 5.5.2, 5.7 (Claude retention), 6.2 (historical handoff); mukade-handover 1, 3, 8, 9 |
| Governing ADRs | ADR-0011, ADR-0020, ADR-0021, ADR-0023, ADR-0049 (proposed) |
| Register sections | SS1 to SS6, S6, S7, CL6, CL7, CX10, OC3, PI3, HM3, DS3, H15, H27 |
| Related records | FRD-0005 (live hop), FRD-0002 (store roots per home), FRD-0014 (index tables and `forget`) |

## 1. Purpose and scope

Any old chat, in any supported agent, is found by searching and resumed in the same or another
agent. This record states the readers and the canonical transcript, the full-text search, the
sessions surface of the workspace, the two export formats, the command line and the exit codes.
Reading is delegated to the Mukade library; everything here is read-only in v1.

## 2. Requirements

### 2.1 Readers and the canonical transcript

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Native session reading is delegated to the `mukade` crate, Shinatama's own library, linked in-process at a reviewed commit with the lockfile committed; the declared pin, not a local checkout, defines what Shinatama can do, and later revisions count only once the pin is advanced and rechecked. No third-party session converter is a dependency. | U 2026-10-27 | S1 | H27 | 0020 |
| R02 | Adapters own store resolution and capabilities and delegate parsing: `list_sessions` and `read_transcript` call Mukade and project its `Session` onto the canonical transcript; no Shinatama code parses a native format the pin reads, except a bounded incremental reader until a reviewed Mukade revision exposes one, which must then replace it. | A | S1 | SS1, SS2 | 0020 |
| R03 | Hermes Agent and DeepSeek Harness have no Mukade reader yet: their adapters declare `list_sessions`, `read_transcript` and `export` unsupported, they are not searched and are not hop sources, until a reviewed pin carries their readers. | A | S1 | HM3, DS3 | 0006, 0020 |
| R04 | A canonical transcript is a list of turns of items: `user_text` and `assistant_text` (transferable), `reasoning` (never transferable), `tool_call` and `tool_result` (as text), `attachment` (as a path reference), `compaction` (transferable, marked), `meta` (dropped or noted). Every item keeps its provenance: agent, native session id, entry id or ordinal, timestamp, byte range or row. | S | S1 | CL6, CX10, OC3, PI3 | 0021 |
| R05 | Reader rules: read-only, never a lock the agent could wait on; SQLite opened `mode=ro` and never `immutable` while the agent may write; an incomplete last JSONL line or torn Zstandard frame ignored until the next scan; unknown record types skipped and counted, with the count shown by `doctor`; trees followed from the root to the leaf the agent names, else the most recent, other branches indexed but not carried; sub-agent entries summarized as one tool pair. | S; A (leaf rule) | S1 | H15, OC3, HM3, DS3 | 0021 |
| R06 | For OpenCode and Hermes Agent the agent's own export is the primary source and the SQLite file a read-only fallback for listing and search, because their schemas move between versions. | S | S1 | OC3, OC10, HM3 | 0021 |
| R07 | A reader that launches a harness command (OpenCode export) inherits the adapter's full isolated launch context, timeout and cleanup; a read-only label does not make that child side-effect free. | U 2026-10-30 | S1 | SS1 | 0023 |

### 2.2 Search (find)

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R08 | Find is full-text search over the sessions of every agent Mukade reads, filterable by agent, project and date, with a preview of the thread. Enter resumes natively in the same agent; another key resumes in a different agent, which is a hop. | U (goal 3); S (keys) | S1 (search, same-agent resume); S2 (hop) | S6 | 0023 |
| R09 | The text index (`session_index`, `session_text` with FTS5) is operational: rebuildable from the agents' stores by `doctor`, incremental (mtime, size, content hash; no full re-read of unchanged files), and it keeps the text of sessions the agent deleted, marked as such. | S | S1 | S6, S7 | 0009, 0049 |
| R10 | Claude Code deletes session files after `cleanupPeriodDays` (default 30). An aged-out Claude session can still be found and can still seed another agent or a new Claude session, but not be natively resumed; the UI says which. Shinatama never edits the retention setting. | S | S1 | CL7 | 0049 |
| R11 | Indexing of a project can be switched off; a thread can be excluded from summaries; `shinatama forget <thread>` removes its snapshots, messages, summaries and index rows (FRD-0014). | S | S1 | I9 | 0009 |

### 2.3 The sessions surface

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R12 | The workspace browses the sessions of every harness the declared Mukade pin reads and opens one in a detail panel. It is not part of the terminal-first milestone: read-only browsing is the first slice, live seed hops follow their gates, native import and handoff are post-v1. | U 2026-10-30; revised 2026-10-31 | S1 | SS1 | 0023, 0040 |
| R13 | A Projects/Sessions switch in the navigator; harness nodes expand to their sessions, with one metadata-only incremental filter and a manual rescan (`r`) that works only while the navigator has focus; arrow keys move in the tree and never steal input from a terminal pane. | U 2026-10-30 | S1 | SS1 | 0023 |
| R14 | A catalog of persisted `store_root` records identifies box and system roots with harness, kind, origin, configuration revision and access. Discovery may use Mukade's harness catalog and installation scan, one recognition rule set; which root becomes a box or a read-only system root is Shinatama's decision, recorded in the catalog. The tree reads an agent's box and the read-only system roots and never a root from an arbitrary instance or ambient defaults. | U 2026-10-30; A (Mukade discovery) | S1 | SS5 (SEC) | 0023, 0020 |
| R15 | A session panel is identified by `(harness, native_session_id, store_root_id)`, never by a file path. Every read names an explicit `store_root_id`. | A | S1 | SS5 | 0023 |
| R16 | The detail panel has Overview, Transcript and Lineage tabs, in-session search, collapsible tool calls, a raw-JSON toggle and copy of the id, native path and resume command. | U 2026-10-30 | S1 | SS2 | 0023 |
| R17 | Opening, browsing, searching or copying a session is read-only: it never resumes, imports or writes, and the user's stores and side files stay untouched, child-process effects included. Restoring a panel never replays a command. | U 2026-10-30 | S1 | SS1 | 0023 |
| R18 | Tree and Overview show only fields the adapter can obtain for that harness and store; unavailable title, cwd, dates, turns or bytes show `unknown`, never inferred by loading every transcript. Native children are included where the reader supports them. | U 2026-10-30 | S1 | SS1 | 0023 |
| R19 | Reads run off the UI thread; the panel names its store and shows unavailable or stale when a root or session is missing or changes. | U 2026-10-30 | S1 | SS2 | 0023 |
| R20 | With the declared pin's whole-session reader the adapter enforces a pre-read size cap and returns `too large` without loading. A paged read that bounds input bytes, decoded records, retained raw data and subprocess capture end to end is a required new capability, not a feature of the current pin; with it a 30 MB transcript meets a recorded memory and responsiveness budget and larger input pages or is refused without unbounded allocation. Limiting only rendering or cache is never sufficient. | U 2026-10-30 | S1 | SS2 | 0023 |
| R21 | Asynchronous results carry `(request_id, panel_revision, store_root_id, native_session_id)`; late, cancelled, stale or concurrent results are discarded and can never update the wrong panel. | U 2026-10-30 | S1 | SS2 | 0023 |
| R22 | Raw display and copy are explicit actions with a privacy warning and redaction by default; an override reveals only the selected records; raw or copied content is never logged or persisted by the UI. | U 2026-10-30 | S1 | SS6 | 0023 |
| R23 | Lineage distinguishes the Shinatama thread chain, the native parent and the native children; native relations appear only where the reader exposes them (Pi and OpenCode at the declared pin), no edges are inferred for Claude or Codex, and missing, external and ambiguous parents and cycles are marked. Clicking a node opens that session or thread; it does not resume it. | U 2026-10-30 | S1 | SS3 | 0023 |
| R24 | Unresolved roots, missing executables and refused stores are reported, never replaced by ambient defaults; executable overrides are adapter facts; settings persist in the shared database. | U 2026-10-30 | S1 | SS5 | 0023 |

### 2.4 Write entry points from the surface

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R25 | Resume starts a PTY through the adapter: native, same agent only; a Claude session is resumed only in its own cwd and never in two instances at once. | U 2026-10-30; S (Claude rules) | S1 | CL4, CL15 | 0011, 0023 |
| R26 | `hop_live` operates on a hosted instance and segment and follows the close/fork, cursor, idle and catch-up rules of FRD-0005. | U 2026-10-30 | S2 | SS4 | 0023 |
| R27 | `handoff_history` operates on any `(store_root_id, native_session_id)`: in one transaction it creates or reuses the project mapping and persists a derived thread and a historical source segment without a live activation; it never closes a process, waits for idle or performs live catch-up; the source fingerprint is captured for the preview and rechecked before use, and a mismatch restarts the preview. In v1 it uses `seed_prompt` only. | U 2026-10-30 | S2 | SS4 (SEC) | 0023 |
| R28 | Before any post-v1 native write the dialog shows the exact staged payload and known losses, obtains consent and journals the reserved destination where supported; the receipt is persisted even when launch fails; uncertain writes are reconciled, never blindly retried, so a double-click or retry never creates a duplicate session. | U 2026-10-30 | P1 | SS4 | 0022, 0023 |
| R29 | A headless resume is a separate, cost-warned action. | U 2026-10-30 | V1 | — | 0023 |
| R30 | A same-harness transfer between two homes (box to system, or into a portable installation) is not offered until SS5 verifies it, even though Mukade revisions after the pin represent it (`hop_into`). | U 2026-10-30 | P1 | SS5 | 0023, 0022 |

### 2.5 Export formats

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R31 | Export is separate from browsing and offers explicit **STX** (`.stx.jsonl`), **portable Mukade JSON** and **native** choices where supported; each declares format version, raw and unknown-field handling, attachment handling, redaction, losses, overwrite and atomic-output behaviour; portable Mukade JSON is never presented as a lossless backup. | U 2026-10-30 | S1 | SS6 | 0023, 0021 |
| R32 | `.stx.jsonl` version 1 is Shinatama's own export: a header line (format, version, thread, source, cwd, git, created, body sha256, redactions, attachments, instruction files) then one line per item with provenance. Readers ignore and keep unknown fields; a higher version is refused with a message, never guessed at. | A | S1 | OC2 (round trip) | 0021 |
| R33 | Two STX modes, `full` (the user's own backup) and `portable` (redacted for sharing); secrets are redacted in both before writing; `reasoning` is never exported; attachments are hashed references, copied only with `--with-attachments`. | A | S1 | H14 | 0021 |
| R34 | Round-trip law, tested per adapter where import exists: `read_transcript(import(export(x)))` has the same `user_text`, `assistant_text` and `compaction` items in order and the same tool items as digests; what an agent cannot represent is listed in the report. | A | P1 | OC2, H27 | 0021 |
| R35 | STX and Mukade's interchange JSON are distinct documents: neither is parsed into the other and neither parses native stores. | A | S1 | — | 0021 |

### 2.6 Command line

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R36 | The command line mirrors the UI for scripts and calls the same adapter operations: `shinatama sessions [--agent A] [--project P] [--json]`, `find "text" [--agent A] [--json]`, `export <thread \| agent:session> [-o file] [--mode full\|portable] [--format stx\|mukade\|native\|md] [--with-attachments]`, `resume <thread \| agent:session> [--agent A]` (a hop when A differs), `hop <thread> --to A [--preview] [--yes]`, `doctor [--agent A]`, `forget <thread>`; `import <file> --agent A [--cwd DIR] [--dry-run]` is post-v1. Names are the author's call. | U (scriptable mode); A (names) | S1 (sessions, find, export, resume, doctor, forget); S2 (hop); P1 (import) | — | 0011 |
| R37 | `--json` output has a stable schema and exit codes: 0 done, 2 refused with a reason, 3 needs the user (login, a first-run prompt), 4 failed and nothing changed, 5 failed and recovery is needed (`shinatama hop --recover`). | A | S1 | — | 0011 |
| R38 | `doctor` reports per agent: installed, version against the fixture gate, sign-in status, unknown-record counts; it can run `integrity_check` and rebuild the index tables. | A | S1 | — | 0009, 0011 |
| R39 | `--format md` is the seed rendering of FRD-0005; `native` is the agent's own export (`export_native`) where it has one. | A | S1 | — | 0021 |

### 2.7 Adapter contract and conformance

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R40 | Every supported agent has an adapter with export, import, resume and handover plus the rest of the contract: identity and environment (A), read and export (B), import post-v1 (C), run (D), handover (E), signals and messages (F), injection and capabilities (G). Capability differences are declared and shown, never hidden; anything not implemented is `unsupported`, not faked. | U (the requirement); A (shape) | X | — | 0011 |
| R41 | Each adapter passes a contract suite on recorded fixtures per agent version, structure-only and synthetic: `list_sessions` matches; read → export → read is stable; the round-trip law where import exists; the canary hop against every other adapter in both directions; refusal and degraded paths; every declared capability exercised and every undeclared one refused. | S | X | E1, E5 | 0011 |
| R42 | An agent that fails its suite for the installed version is `Degraded`: failing capabilities `unsupported`, launch-only plus its own export command, hops by seed or refused, the UI says why. | U 2026-11-01 | X | AT-10 | 0011 |
| R43 | The engine compiles and passes its tests with no adapter at all and with a fake in-memory adapter; adding an agent changes only its adapter and fixtures. | S | X | — | 0011 |
| R44 | `list_sessions` never reads a whole session per row; unknown metadata stays unknown. | S | S1 | SS1 | 0011 |
| R45 | Where the agent exposes them, the adapter reports "needs input" and "turn finished" without screen parsing (FRD-0010). | U | V1 | I1 | 0011, 0027 |
| R46 | Backend-attach mode (Codex `app-server` plus `--remote`, OpenCode `--server URL`) is a milestone-1 candidate decided before the adapters are written; Pi has no TUI attach (corrected by PI5); Hermes's gateway route is HM2; DeepSeek Harness has only the backend. | S | M0 | CX6, HM2 | 0011 |
| R47 | Path identity: every path is canonicalized before storing or comparing (strip `\\?\`, resolve junctions and symlinks, normalize drive-letter case, case-insensitive comparison where the volume is), so Claude's flattened folders, Codex's extended-length prefixes and Pi's and DeepSeek Harness's `--cwd--` folders map to one project. | S | S1 | S9 | 0009 |
| R48 | Facts about each agent's store and commands (technical-design 5.3 and 5.7) are research until their fixture confirms them on the installed version; version drift (Claude 2.1.316, OpenCode 2.2.5, Pi 1.4.0 installed against the recorded 2.1.310/2.1.311, 2.1.4, 1.2.0) re-runs the rows before any support claim. | S | X | E1, E5 | 0010 |

## 3. Open questions

1. Whether the sessions surface should offer only actions both adapters support (register O7).
2. Advancing the Mukade pin past `4124b1f` (installation scan, `hop_into`), which reopens SS5.

## 4. Acceptance

The read-only slice: functional-design 5.16 "Acceptance of the read-only slice" and register
SS1 to SS6; reader rows CL6, CX10, OC3, PI3, HM3, DS3 and H15; S6, S7, S9. OC2 (OpenCode
round trip) and PI5 passed; everything else is open.

## 5. Relation to the JSON record

FRD-0006.json holds 66 extracted sentences; this record covers them all and adds 2.1 (the
readers, which the JSON set spread over FRD-0001 and FRD-0005), 2.7 (the adapter contract and
conformance) and the Claude retention rule (R10).
