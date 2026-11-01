---
adr_id: ADR-0023
version: 1
title: Session browsing and search are read-only, bounded and explicit about store roots; live hop vs historical handoff
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - sessions-surface
    - session-search
    - store-roots
    - read-only-session
    - historical-handoff
governs_symbols:
depends_on:
  - ADR-0009
  - ADR-0016
  - ADR-0020
  - ADR-0021
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0023: Session browsing and search are read-only, bounded and explicit about store roots; live hop vs historical handoff

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user decided that Shinatama searches all local sessions of every
supported agent Mukade reads (four today; Hermes Agent and DeepSeek Harness once Mukade reads
them) and resumes one in the same agent or in a different one, with an interactive flow and a
scriptable command line (functional-design goal 3). The sequencing of the sessions surface,
read-only browsing first and live seed hops after their gates, with native import and native
handoff post-v1, is the user's decision of 2026-10-30, revised 2026-10-31 and confirmed on
2026-11-01 (functional-design 5.16 and 7). Which discovered root becomes a box or a
read-only system root is the author's call, the session panels of technical-design 3.1 are
tagged [C], and command names are left to the builder (functional-design 4, 8). The question
is how browsing, search and opening of native sessions stay read-only, bounded in memory and
tied to an explicit store, and how a hop from history differs from a hop of a live instance.
The full design is in functional-design 5.3 and 5.16 and technical-design 3.1, 4.2 and 6.2;
this record holds only the decision.

## Decision Drivers

- Opening or searching an old session must never resume, import or change a native store (technical-design I2, I7).
- A native session id is not an identity without its store root; a box and a system home can hold the same harness.
- Memory stays bounded on large transcripts while the declared Mukade pin reads whole sessions only.
- Raw records can contain credentials, private paths and tool arguments.
- The terminal keeps its input; the navigator never takes keys from it.

## Decision

Search is full text over the sessions of every agent Mukade reads, filterable by agent, project
and date, with a preview. Enter resumes natively in the same agent; another key resumes in a
different agent, which is a hop. The command line mirrors it (`shinatama find`, `sessions`,
`resume`, `export`) with a stable `--json` schema (functional-design 5.3; technical-design 5.5.2).

The sessions surface is a Projects/Sessions navigator over a catalog of persisted `store_root`
records (harness, kind, origin, configuration revision, access). Every read names an explicit
`store_root_id`, and a session panel is identified by `(harness, native_session_id,
store_root_id)`, never by a file path. Opening a session is read-only: it never resumes, imports
or writes, and restoring a panel never replays a command. Reads run off the UI thread;
unavailable metadata shows `unknown` instead of being inferred by loading every transcript. With
the declared Mukade pin, which reads whole sessions, the adapter enforces a pre-read size cap and
returns `too large`; a paged read that bounds input bytes, decoded records, retained raw data and
subprocess capture end to end is a required new capability. Asynchronous results carry
`(request_id, panel_revision, store_root_id, native_session_id)`, and late or cancelled ones are
discarded. A reader that launches a harness command inherits the adapter's full isolated launch
context, timeout and cleanup. Raw display and copy are explicit, redacted by default and never
logged or persisted. Export (STX, portable Mukade JSON, native) is separate and declares format
version, losses and overwrite and atomic behaviour.

There are two write entry points. `hop_live` starts from a hosted instance and follows the
close/fork, cursor, idle and catch-up rules. `handoff_history` starts from any `(store_root_id,
native_session_id)`: in one transaction it creates or reuses the project mapping and persists a
derived thread and a historical source segment; it never closes a process, waits for idle or
does live catch-up, and it rechecks the source fingerprint before use. In v1 both use only seed
delivery. A headless resume is a separate, cost-warned action. See functional-design 5.16 and
technical-design 3.1 and 6.2.

## Verified Interview Examples

- **EX-001** (nominal case): Input: `shinatama find "refresh token" --json | jq -r '.[0].id'`, then `shinatama resume t_8f3a --agent codex` -> Expected: the first prints a thread id for a script; the second resumes thread `t_8f3a` in Codex, as a hop when Codex is not the thread's agent (functional-design 5.3; technical-design 5.5.2).
- **EX-002** (boundary case): Input: a session over the pre-read size cap, with the current whole-session Mukade reader -> Expected: `too large` before the read, without loading the session (functional-design 5.16 acceptance).
- **EX-003** (boundary case): Input: stale, cancelled and concurrent reads while panels or the store change -> Expected: none of them updates the wrong panel (functional-design 5.16 acceptance; register SS2).
- **EX-004** (rejection or error case): Input: the source session changes between the preview of a historical handoff and its use -> Expected: the fingerprint mismatch restarts the preview instead of writing (technical-design 6.2; register SS4).
- **EX-005** (rejection or error case): Input: raw records, tool arguments and attachments with planted secrets, displayed, copied and exported with default settings -> Expected: no secret is disclosed by default; export is explicit and atomic, with no partial output (register SS6).

## Rejected Alternatives

- **RA-001**: choosing a store root from an arbitrary instance or from ambient defaults; agent plus native id alone is not an identity, and unresolved roots are reported, not replaced (functional-design 5.16; technical-design 4.2).
- **RA-002**: filling missing metadata by loading every transcript; the listing contract allows no full read per row, so unavailable fields stay `unknown` (technical-design 5.5 B).
- **RA-003**: bounding only rendering or the cache; "never sufficient", because input bytes, decoded records, retained raw data and subprocess capture must be bounded end to end.
- **RA-004**: inferring lineage edges for Claude or Codex; the declared reader exposes native relations only for Pi and OpenCode.
- **RA-005**: relying on a read-only UI label for a reader that launches a harness command; the label "alone does not make that child side-effect free".

## Enforceable Invariants

- **INV-001**: browsing, opening, searching and copying never resume, import or write a native store or its side files (technical-design I2, I7).
- **INV-002**: every native session reference carries `store_root_id`; a file path is never a panel's identity.
- **INV-003**: with the whole-session reader, input over the pre-read cap returns `too large` before anything is loaded.
- **INV-004**: an asynchronous result whose `(request_id, panel_revision, store_root_id, native_session_id)` no longer matches its panel is discarded.
- **INV-005**: raw display and copy are redacted by default and are never logged or persisted by the UI.
- **INV-006**: additional system roots are `read_only` and never a box's write destination; a same-harness transfer between two homes is not offered until register SS5 passes.
- **INV-007**: in v1, `hop_live` and `handoff_history` use only seed delivery; `handoff_history` never closes a source process, waits for idle or performs live catch-up.
- **INV-008**: arrow keys in the navigator never take input from a terminal pane; rescan (`r`) works only while the navigator has focus.

## Consequences

- The adapter contract gains a bounded `read_transcript(native_ref, from_cursor?, max_bytes, max_records)`; it needs a reviewed Mukade revision or an adapter-owned bounded reader, and until then large sessions show `too large` (technical-design 5.5 B).
- Only harnesses the declared Mukade pin reads are listed: four today. Hermes Agent and DeepSeek Harness resume natively and receive seeds but are not searched and are not hop sources until their readers land (functional-design 7; register HM3, DS3).
- The surface is not part of the terminal-first milestone; the first usable slice adds read-only browsing and search with same-agent native resume, then the live seed hop (functional-design 5.12; ADR-0040).
- The text index (`session_index`, `session_text` with FTS5) is operational and sensitive: `shinatama forget` and per-project indexing opt-out apply (technical-design 4.2, 4.7). Milestone item 12 must confirm that an aged-out Claude session stays searchable and usable as a seed (technical-design 10).
- Post-v1 native writes from this surface need an exact staged preview, a persisted reserved destination, reconciliation after every crash point and no duplicate on double-click or retry (functional-design 5.16; ADR-0016, ADR-0022).
- Open: SS5 waits until the Mukade pin is advanced past `4124b1f` (register 20, next run 8).

## Confirmation

- Register SS1 to SS6 pass; they have not been executed.
- The acceptance list of the read-only slice in functional-design 5.16 passes, including no native write from browsing and the planted-secret fixtures; the 30 MB memory and responsiveness budget applies after the incremental-reader gate.
- Nothing has been run yet for this decision.
