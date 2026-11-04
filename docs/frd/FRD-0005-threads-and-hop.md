# FRD-0005: Threads and hop

| | |
|---|---|
| Version | 1 (Markdown record; JSON record version 1 at `c84d28e`) |
| Status | Accepted 2026-11-01 (this Markdown text; the JSON evidence record, where one exists, stays Proposed until `frd_tool.py promote`). Amended by hand 2026-11-04: native import and native fork are out of scope in every version (user decision 2026-11-03; ADR-0016 v2, ADR-0022 v2, Accepted 2026-11-04); R16, R17 and 2.10 (R52–R59) withdrawn, R04/R07/R12/R14 amended |
| Date | 2026-11-01, amended 2026-11-04 |
| Sources | functional-design goal 2, 5.1, 5.2, 7; technical-design 1, 2, 4.2 to 4.4, 5.4, 5.6, 6.1 to 6.8, 9, 11, 12; mukade-handover 2 to 7; daodan-native-support 4.3 |
| Governing ADRs | ADR-0015, ADR-0016 v2, ADR-0017, ADR-0018, ADR-0019, ADR-0022 v2, ADR-0034 |
| Register sections | H1 to H27, S4, S10, SC9, Q5, Q9, D13 |
| Related records | FRD-0006 (historical handoff from the sessions surface), FRD-0008 (task across a hop), FRD-0009 (seed through the sidecar), FRD-0014 (journal and tables) |

## 1. Purpose and scope

A **thread** is the user's unit of work across agents; a **hop** moves the live work of a thread
from one agent to another. This record states what a thread is on disk, what "seamless" means,
the two modes (close and fork), the v1 strategies, the seed, the state machine with its
recovery, the failure handling and the native path closed on 2026-11-03 (2.10, withdrawn). The sessions-surface entry points
(`hop_live`, `handoff_history`) are in FRD-0006.

Vocabulary (technical-design 2): **segment** `s_…` is Shinatama's record that a thread lived in
one native session of one agent; a thread is an ordered chain of segments; an **activation** is
one period a segment was open; a **hop** `h_…` moves the live work to a new or earlier segment;
the **seed** is the text a destination receives.

## 2. Requirements

### 2.1 Threads on disk

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | A thread is Shinatama's own record on disk: an id, the project, the chain of segments (agent, store root, native session id, time span), and enough to rebuild the hand-over context. It survives the agent and the terminal; closing Shinatama loses no thread. | S | S1 | S4 | 0009 |
| R02 | A thread has at most one live segment; two hops of the same thread never run at once, enforced in the database (invariant I1). When two processes try to hop one thread exactly one wins and the loser gets a clear message. | S | S2 | S10, AT-4, H25 | 0009, 0018 |
| R03 | Shinatama never edits, deletes or moves the source session's records; "archived" is a mark in Shinatama's database (invariant I2). | A | X | AT-1 | 0015, 0016 |
| R04 | Native stores are read-only in every version except through the agent's own commands (invariant I7); Shinatama never writes a native session through itself or Mukade. | U 2026-10-31; U 2026-11-03 | X | — | 0016 v2, 0040 |

### 2.2 What a hop is and what "seamless" means

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R05 | A hop starts a new native session in the destination with the thread's content, with the loss declared; no live session is moved between different agents. Resuming within the same agent is native and lossless, and "native resume" is used only for that case. | U ("truly seamless"); S (what exists) | S2 | H1 | 0015 |
| R06 | Seamless is five testable criteria: one key, a destination, Enter, nothing to type or copy; destination ready under 8 s with a warm standby and under 25 s without, with visible progress and the source usable throughout; the destination answers correctly a question whose answer exists only in the history; hopping back resumes the earlier native session and tells it what it missed; the user sees before sending exactly what is carried and lost. | A (targets to measure) | S2 | H1, H6, H10, H21, AT-2, AT-3 | 0015 |
| R07 | The hop dialog states what carries over (conversation text; tool calls always as digests, native import being out of scope; files, same folder, git state unchanged; attachments as paths; project instructions linked) and what is lost (the live process, permissions, model state, hidden reasoning). The UI never promises a lossless transfer. | S; U 2026-11-03 (digests clause) | S2 | H2 | 0015, 0016 v2 |
| R08 | Linking project instructions (CLAUDE.md, AGENTS.md) never creates a file in the user's repository without consent; what "linked" does exactly is decided and tested under H18. | A | S2 | H18 | 0015 |
| R09 | The destination receives the whole thread, not a fixed excerpt, bounded by the destination model's window and by what the sidecar lets through, with the budget computed for the destination. | A | S2 | H3, AT-5 | 0019 |

### 2.3 Close or fork

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R10 | Every hop has a mode, `close` or `fork`; the setting `hop.source_mode` holds the default (`close`) and the hop dialog offers the other mode for that hop only without changing the setting. | U 2026-10-27 | S2 | H26 | 0017 |
| R11 | In close mode the destination continues the same thread; the source instance is closed at COMMITTED, after asking if it has a draft or a running command; the source's native session stays on disk. | U 2026-10-27 | S2 | H26 | 0017 |
| R12 | In fork mode the destination is a new thread with `parent_thread_id` = the source thread; the source instance, thread and activation stay live and untouched; the destination never resumes the source's live native session (two processes never write one session); it always gets a new session through a seed — the adapter's `fork()` and gated native import are withdrawn (2026-11-03). | U 2026-10-27; U 2026-11-03 | S2 | H26 | 0017, 0016 v2 |
| R13 | After a fork the two threads diverge on purpose; nothing synchronizes them, UNDO does not apply, and hopping to the source later is an ordinary hop between two threads. | U 2026-10-27 | S2 | H26 | 0017 |

### 2.4 Strategies in v1 and after

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R14 | Every version permits only `native_resume` (the destination already has a segment of the thread), `seed_prompt` (every other supported pair) and `refuse` (destination not installed or signed in and not fixable, or launch-only). `native_import` and native fork are unreachable in every version and are never implemented. | U 2026-10-31; U 2026-11-03 | S2 | H1 | 0016 v2 |
| R15 | V1 ships the text hand-off for every ordered pair among the six agents, with the preview; a pair whose source is Hermes Agent or DeepSeek Harness counts from the day Mukade reads that source. Until then those two resume natively and receive seeds but are not hop sources. | U 2026-10-31; A (Hermes and DeepSeek sources) | S2 | H1, HM3, DS3 | 0006, 0016 |
| R16 | **Withdrawn 2026-11-03 (user; ADR-0016 v2).** Was: native import post-v1, enabled one destination agent and version at a time behind gates, hybrid end state. Native import is out of scope in every version; the number is kept and never reused. | U 2026-11-03 (withdrawal); U 2026-10-31 (original) | — | — | 0016 v2 |
| R17 | **Withdrawn 2026-11-03 (user; ADR-0016 v2).** Was: post-v1 native writes following the stages of technical-design 5.4 (agent's own import; Mukade-written files or rows). No version writes a native store; the number is kept and never reused. | U 2026-11-03 (withdrawal); U 2026-11-01 (original) | — | — | 0016 v2 |
| R18 | An agent whose installed version fails its fixtures drops every hop to or from it to `seed_prompt` or `refuse`, with the reason shown. | U 2026-11-01 | S2 | AT-10 | 0011 |
| R19 | A hop whose destination window cannot hold the minimum seed (header, task statement, last turn) is refused with the numbers. | S | S2 | H3 | 0019 |

### 2.5 The seed document

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R20 | The seed keeps a header: thread, source agent and session short id, time, cwd, branch and worktree, the number of uncommitted and untracked files with the first 20 paths, the instruction files in effect as paths, attachments as path, mime, size and sha256, and the instruction to treat the content as quoted history, continue from the last user request, not repeat finished work, and reply `ready <nonce>` plus one line, then wait. The header also tells the destination which instruction file it reads and names every instruction file the project has where the destination loads only one. | S | S2 | H2, CL14, CX14, OC9, HM8, DS8 | 0019 |
| R21 | The conversation is rendered as `[user]`, `[assistant]` and `[tool] <name> <short args> → <status>` lines; tool calls are one-line digests (name, target or command head, status); a tool output over 40 lines keeps its first and last 15 lines with a marker. | S | S2 | H16 | 0019 |
| R22 | Budget: `seed_tokens = min(0.40 × destination_window, 60,000)`, estimated as bytes ÷ 3.5; the window comes from the provider's model catalog for the destination model, never from a name in the code; an unknown window caps the seed at 24,000 tokens. | A | S2 | H3 | 0019 |
| R23 | Shedding order: header; the first user message; the last turns verbatim back to 55 % of the budget; the middle as one-line digests with user requests kept; the oldest remaining turns dropped first, each gap marked `[N turns omitted]`. Cuts fall on turn and UTF-8 boundaries only; an empty projection is refused, not sent. | A | S2 | AT-5 | 0019 |
| R24 | The whole rendered transcript is written to `<handoffs>/<hop-id>/full.md`, named in the seed, so the destination can read on demand what the budget cut. `<handoffs>` is `<git common dir>/shinatama/handoffs` for a git project (shared by linked worktrees, never tracked, 0600 or a current-user ACL) or the data directory, which is also the fallback where an agent's sandbox cannot read the project location. | A | S2 | H4 | 0019 |
| R25 | `reasoning` items are never part of a seed, a catch-up or the full transcript file. Noise is filtered: Claude `isMeta` and sidechain records, Codex injected-context user messages, Claude hook-output attachments, and hook or system-reminder text injected into a turn. | S | S2 | CL6, CX10 | 0019, 0021 |
| R26 | Every seed and catch-up starts with `[shinatama handoff v1]` and the hop's nonce; read back from any store it becomes a `meta` marker, never a user turn, so the history of A → B → C never nests earlier seeds. | S | S2 | H11 | 0019 |
| R27 | Secret-shaped strings (API keys, bearer and basic tokens, private key blocks, secret-looking `.env` values, URLs with credentials, long high-entropy tokens) become `[redacted:<kind>]` before anything is stored or sent; the dialog shows the count; the rule set is configurable and tested on a corpus for recall and false positives. | A | S2 | H14, AT-6 | 0019 |
| R28 | No destination executes or expands a seed or bootstrap line that starts with `/`, `!`, `@path`, `#` or `-`; where a channel would, the seed is escaped or sent by a safe channel. | A | S2 | H7, HM14, DS15 (SEC) | 0019 |
| R29 | Files over 64 MiB are read in streaming mode from the end, never loaded whole. | S | S2 | CL16 | 0019 |
| R30 | When the source thread is linked to a Daodan task the seed gains an optional Daodan block with its own budget line, shown verbatim in the preview (FRD-0008). | P | V1 | D13 | 0034 |

### 2.6 Seed delivery

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R31 | Delivery goes through the adapter's `seed_channel`. `argv_prompt` carries only a bootstrap of at most 2 KB (header, last user request, the instruction to read the seed file); Pi uses `file_arg` (`@<path>`), Hermes Agent `query_file`, DeepSeek Harness `protocol_prompt` (the first ACP `session/prompt`); `system_prompt_file` on Claude is evaluated, not the default; `paste` is the last resort and depends on the agent's input being ready. | A | S2 | CL2, CL3, PI1, HM1, DS2, H8 | 0019 |
| R32 | Per agent: Claude `claude --session-id <uuid> <bootstrap>`; Codex `codex -C <cwd> <bootstrap>`; OpenCode `opencode <dir> --standalone --session ses_<id> --prompt <bootstrap>`; Pi `node cli.js --session-id <id> @<seed> <bootstrap>`; Hermes `hermes chat --query-file <seed>`; DeepSeek ACP `session/new` then `session/prompt`. Where the id is known in advance (Claude, OpenCode, Pi, DeepSeek Harness at `session/new`) the journal records the destination segment before the spawn or first prompt; where it is learned (Codex, Hermes) it comes from the new store entry or a notify. | S | S2 | CL1, CX1, OC1, PI2, HM1, DS2, H12 | 0019, 0007 |
| R33 | The seed, bootstrap and receipt pass through the sidecar as ordinary user turns, below its adaptive threshold, unaltered by the prompt rewriter and never externalized by CCR; where either would, a bypass marker is added in the sidecar's single source (FRD-0009). | A; U 2026-11-01 (marker's place) | S2 | SC9 (SEC) | 0029 |
| R34 | Warm standby: for the agents the user names, one idle, un-seeded instance per project in use is kept in a hidden pane, costing memory but no tokens, discarded after 30 minutes idle or on a version change; off by default. | A | S2 | H6 | 0019 |

### 2.7 State machine, receipt and recovery

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R35 | A hop runs PLANNED → PREVIEWED → CONFIRMED → PREPARING → READY → SWITCHING → COMMITTED, with ABORTED and FAILED terminal and ROLLED_BACK through UNDO. Each arrow is one SQLite transaction that updates `hop.state` and inserts a `hop_event`; external effects happen between transactions and are journaled under `hop.id`; a journal key does not make an external effect idempotent. | S | S2 | S4, H24, AT-1 | 0018 |
| R36 | PLANNED computes mode, strategy, source cursor, snapshot, budget and loss list and takes the uniqueness of I1. PREVIEWED shows the snapshot and loss list, with the exact text one key away. CONFIRMED means Enter; nothing has run in the destination yet. | S | S2 | H2 | 0018 |
| R37 | What the destination receives is exactly what the user previewed, stored verbatim as a snapshot after redaction (invariant I4). | S | S2 | AT-6 | 0019 |
| R38 | READY needs both technical evidence (process alive, native session exists, the first user entry hashes to the seed sent) and the receipt (the destination's first reply contains the nonce); timeout 90 s → FAILED. A missing receipt is asked for once more, then the hop fails offering to switch anyway. | A | S2 | H2, H12 | 0018 |
| R39 | SWITCHING requires the source to be idle (turn finished); if it is working the user waits, interrupts or cancels. Turns gained after the snapshot are sent as a catch-up and rechecked, at most twice. Focus moves to the destination. In close mode the source activation ends with `end_reason = hop_out` and its process runs until COMMITTED; in fork mode the source activation stays open. | S | S2 | H9 | 0018 |
| R40 | COMMITTED when the destination has been live 30 s or accepted the user's next message; then the source is closed in close mode (asking first about a draft or running command) and the hop's resources are released. | S | S2 | H1 | 0018 |
| R41 | UNDO: within 10 minutes of a close hop the user can hop back; it is a normal hop to the earlier segment by `native_resume`, with no duplicate live session. | A | S2 | H13 | 0018 |
| R42 | Recovery at start, by any process once: PLANNED, PREVIEWED, CONFIRMED → ABORTED. In PREPARING the journal records `spawn_attempted` before and `spawn_observed` on evidence; an uncertain spawn is only observed, never relaunched, and after 90 s is FAILED with a manual retry offered; leftovers Shinatama can prove it started are killed. READY → ABORT; SWITCHING → COMMITTED if the destination is alive, else the source activation is reopened and the hop ABORTs. No recovery path deletes anything an agent owns. | S | S2 | H12, H24, AT-1 | 0018 |
| R43 | On `SQLITE_BUSY` or `SQLITE_FULL` the hop retries with backoff, shows the error and never proceeds without the journal. | S | S2 | S4 | 0018 |
| R44 | A hop is never left hidden in a half state: a destination stuck without a visible reason fails at the READY timeout with the pane's last 40 lines shown, and a destination blocked at a first-run prompt (folder trust) is brought forward for the user to answer; Shinatama never writes the agent's configuration to pre-answer it. | S | S2 | H5, CL12 | 0018 |

### 2.8 Hop back and catch-up

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R45 | When the destination already has a segment of the thread, Shinatama reads what the other segments did since that segment's `native_cursor`, resumes the earlier native session (its id unchanged) and delivers a catch-up message as the first user turn ("While you were away (N turns, via <agent>): …"), rendered like a seed, limited to the missed turns, same budget and receipt. | A | S2 | H10, AT-3 | 0019 |
| R46 | If the earlier native session is gone or cannot be resumed (for example Claude's retention deleted it), the hop falls back to `seed_prompt` and says why. | S | S2 | CL7 | 0019, 0049 |
| R47 | `native_cursor` is a per-agent JSON object (last entry id or ordinal, file length or row sequence, a hash of what was read) sufficient to know what happened after that point. | S | S2 | H15 | 0009 |

### 2.9 Failure matrix

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R48 | Destination not installed: offer install through the checkup, then retry. Destination signed out: run the login flow, then retry. Not fixable or launch-only: `refuse` with an explanation. | S | S2 | H5 | 0018, 0031 |
| R49 | Agent fails to start or exits during PREPARING: FAILED, its last output shown, the source untouched. Session id never appears: FAILED, the destination killed. Seed not found in the new session (hash mismatch): FAILED with the diff. | S | S2 | H12 | 0018 |
| R50 | Source gained turns: catch-up, at most two rounds. Source busy with no turn-end signal: wait, interrupt or cancel; the dialog tells the truth about what an interrupt loses. | S | S2 | H9 | 0018 |
| R51 | Shinatama crash: recovery per state. Database locked or full: R43. Agent version unknown: the pair drops to seed or refuse. Context window too small: refuse with numbers. | S | S2 | AT-1, AT-10 | 0018 |

### 2.10 Post-v1 native handover

**Closed 2026-11-03 (user decision; ADR-0016 v2, ADR-0022 v2, Accepted 2026-11-04).** The
native handover route is out of scope in every version: R52–R59 are **withdrawn**, their numbers
kept and never reused; their rules survive only as the re-entry preconditions of ADR-0022 v2
(mukade-handover.md is the design record of the closed route). The rows below are the withdrawn
text, kept for traceability; their `P1` slice no longer commits anything.

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R52 | The native route is delegated to Mukade's `hop()` with `resume: None`, called in-process at the reviewed pin; Mukade owns conversion and the new destination session, the engine owns the state machine, close/fork and the source activation, the adapter owns capabilities, configuration and launch. | U 2026-10-27 (Mukade, no third-party converter) | P1 | H27 | 0020, 0022 |
| R53 | Before any external work the journal holds `hop.id`, source reference and cursor, settings, target store and intended effect, and a destination id or path is reserved where Mukade supports it; the source fingerprint is rechecked immediately before the call and a mismatch aborts before any write. | S | P1 | SS4 | 0022 |
| R54 | The receipt and manifest are persisted at once, even when launch or resume fails. A Mukade receipt never satisfies READY: READY needs the adapter's check against the manifest and the separate nonce acknowledgement. | S | P1 | SS4 | 0022 |
| R55 | No automatic second `hop()` or import after an uncertain or failed outcome: `SourceChanged` discards the preview and needs a new confirmation; `WriteUncertain` reconciles the reserved id and location first; `ResumeFailed` persists the embedded receipt and recovers that destination; native sessions, indexes or leftovers are never deleted automatically. | S | P1 | SS4 | 0022 |
| R56 | Hops request `ReasoningPolicy::Drop`, never Mukade's `DropForeign` default; signed or encrypted payloads are never altered. | S | P1 | — | 0022 |
| R57 | `StoreRoots` is built from the resolved instances, never ambient defaults; a boxed OpenCode's database and `XDG_*` are resolved explicitly; process-wide environment is not changed for concurrent hops; Mukade's children receive Shinatama's isolation, marker removal and credential configuration. A same-harness two-home transfer is refused until SS5 verifies it. | S | P1 | SS5 | 0022 |
| R58 | The native route stays closed until the gates of mukade-handover 5 are met: an exact staged preview with separate consent before a source-model summary and another confirmation if the result changes what is sent; a redaction boundary over the complete selected data, raw records, tool arguments and attachments included; a stable source cursor with serialized target writes; isolation of children, caches, hooks and temporary files. | S | P1 | SS4, H27 | 0022 |
| R59 | `DestinationNative` compaction is refused by Mukade today; using the destination's own compaction means writing the full history first, then invoking that harness's compaction in its environment. | S | P1 | H27 | 0022 |

### 2.11 Daodan task across a hop

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R60 | In close mode the preliminary seed carries the task identity and says that close was requested but not yet confirmed; the holder is unchanged and the destination may read, not take over. Only after COMMITTED and the source's confirmed closure does Shinatama send a separate handover message through the destination's normal channel; if closure is refused, crashes or is uncertain, no message is sent. One user confirmation in total, the one in the hop dialog. | U 2026-11-01 | V1 | D13 | 0034 |
| R61 | In fork mode the source keeps the holder and the destination is a new thread without the task; two threads never both hold one task. | A | V1 | D13 | 0034 |
| R62 | A destination with no Daodan package receives the task block as read-only context, the dialog says the task cannot be taken over there, and close mode sends no handover message. | P | V1 | D20 | 0034 |

## 3. Open questions

Technical-design 12 lists the author's defaults the user may change: the 40 %/60,000 budget, the
`ready <nonce>` receipt (one short turn per hop), warm standby off by default, the 10-minute
UNDO, the 90 s timeout. Whether to offer only actions both adapters support is register O7.

## 4. Acceptance

AT-1 (journal and crash), AT-2 (canary hop for every ordered pair), AT-3 (hop back), AT-4
(concurrency), AT-5 (budget), AT-6 (redaction), AT-10 (unknown version) of technical-design
11, and register H1 to H27. Nothing has run on the Shinatama runtime; CX4 and OC2 are agent
probes, H27 a harness probe with an external converter that was not adopted.

## 5. Relation to the JSON record

FRD-0005.json holds 74 extracted sentences; this record covers them all and adds 2.10 (the
post-v1 native handover rules, which the JSON set left to the hand-over document) and 2.11
(the task across a hop, which the JSON set kept in FRD-0008).
