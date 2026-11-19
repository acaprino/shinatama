---
adr_id: ADR-0053
version: 1
title: No session is too large to resume, view, search or export; each operation reads only what it needs, and past a threshold the reading goes by chunks instead of refusing
status: Superseded
superseded_by: ADR-0053 v2
author: Alfio Caprino
scope:
  paths:
    - crates/engine/src/session_detail.rs
    - crates/engine/src/session_find.rs
    - crates/engine/src/session_export.rs
    - crates/engine/src/session_roots.rs
    - apps/desktop/src-tauri/src/agents/resume.rs
    - apps/desktop/src-tauri/src/titles.rs
    - apps/desktop/src-tauri/src/sessions/search.rs
  modules:
    - mukade
  domains:
    - session-reading
    - resume
governs_symbols:
depends_on:
  - ADR-0050
created_from_task: large-session-resume-2026-11-13
proposed_on: 2026-11-13
proposed_at_sha: 20b7ed4a0f7ec89dc3eb3b43dabcc00f6b31012e
accepted_on: 2026-11-13
---

# ADR-0053: No session is too large to resume, view, search or export; each operation reads only what it needs, and past a threshold the reading goes by chunks instead of refusing

## Context and Problem Statement

On 2026-11-13 the user could not resume a Claude Code conversation: the app said "The
conversation could not be resumed. Try again." Two size refusals stood behind it:

1. The session detail refused when the whole archive (every session of the agent's home)
   passed 16 MiB. The archive held 17.10 MiB, so every view, export and resume in it was
   refused. Fixed in `ad72dd3`: the bound became the one session's file.
2. That bound still applies: the reader materializes the whole session, so a session past
   16 MiB is refused. The conversation in question had reached 10.6 MB and kept growing.

Resume, thread names, the activity line, search indexing and export all reached the
conversation through the same whole-session read, and every one of them inherits the
refusal, although resume and thread names need only what the list already reads from a
file's two ends.

Measured on the user's sessions with the installed debug build, export end to end through
the contained helper:

| File | Peak memory | Time |
|---|---|---|
| 10.6 MB | 51 MB | 3.3 s |
| 3.6 MB | 26 MB | 1.0 s |
| 2.4 MB | 23 MB | 0.9 s |

Memory is about three times the file over a 20 MB base, far below the helper's 768 MiB to
1 GiB budget. The binding limit is the helper's ten-second wall time, reached near 30 MB
in a debug build. The register already asks for paging (SS2: "a 30 MB session meets the
recorded ... protocol, larger input pages or returns `too large`"; CL16 and FRD-0005 R29:
files over 64 MiB are read in streaming mode from the end, never loaded whole).

## Decision Drivers

- The user, 2026-11-13: there must not be a limit; past a threshold the size must be
  handled intelligently, by compressing or by chunks.
- A size must never make an operation fail with a message that invites a retry that
  cannot succeed.
- Cost follows what the operation needs, not the size of the file.
- The contained helper keeps its memory, process and wall-time budgets; reading changes,
  the budgets do not grow without bound.

## Decision

No operation refuses a session for its size. Each one reads only what it needs:

| Operation | Reads | Cost |
|---|---|---|
| Resume (verify the native session) | the overview: what the list reads from the file's two ends (id, working directory, title, update time, parent) | constant |
| Thread names | the overview | constant |
| View (detail pages) | up to the threshold, the whole session as today; past it, chunks from the end, newest page first, older pages on request | one page's chunks |
| Activity line, usage ledger, touched paths | the tail chunks of the session | the tail |
| Search in a session, search indexing | forward, one record at a time | constant memory, time in proportion to size, with progress |
| Export | forward, one record at a time, written as read | constant memory; the wall time grows with the file and shows progress |

The threshold is the existing 16 MiB, now a switch between the whole read and the chunked
read, never a refusal. A chunk is 2 MiB of whole JSONL records. A chunked read follows the
harness's own order: Claude Code's active branch from its leaf back through `parentUuid`,
Codex and Pi in file order.

Step 1, the overview, ships now (`overviewOnly` on the detail request: no event, no
ledger, no size bound; resume and thread names use it). The chunked readers belong in
Mukade, which today has only whole-session `read` and the two-ends `peek`, and need the
Mukade pin brought forward first.

## Rejected Alternatives

- **Compressing the session before reading it.** Compression saves disk, not reading: to
  show, search or export a session it must be decompressed and parsed, so memory and time
  are the same.
- **Raising the bound.** A larger bound moves the failure to a larger session and spends
  the helper's whole budget on one read.
- **Reading the whole file in the host, outside the helper.** It drops the containment the
  helper exists for (C22).

## Enforceable Invariants

- **INV-001**: no session operation returns a size refusal (`InputTooLarge`) for the
  session's own size once the chunked readers ship; until then only view, search and export
  may, and resume and thread names never do.
- **INV-002**: an overview never parses a record outside the file's head and tail windows.
- **INV-003**: an overview request carries no cursor; a cursor with `overviewOnly` is
  refused as an invalid request.
- **INV-004**: a chunked read holds at most one chunk plus the page it projects.

## Examples

- **EX-001** (nominal): a 30 MiB Claude session is resumed -> the overview confirms id,
  harness and project in constant time; Claude reopens it.
- **EX-002** (nominal): a 100 MiB session is viewed -> the newest 50 events appear from the
  last chunks; scrolling back loads earlier chunks.
- **EX-003** (error): a chunk holds a torn last line -> that line is skipped as today; a
  record that cannot be parsed is reported, never a size refusal.

## Consequences

- Resume and thread names no longer depend on the session's size (step 1, 2026-11-13: the
  real 10.6 MB session's overview takes 132 ms against 946 ms for a full read, same folder
  and title).
- Mukade gains chunked readers per harness; Shinatama's pin moves forward.
- Register SS2 drops "or returns `too large`": larger input pages.

## Confirmation

- `crates/engine/src/session_detail.rs` test
  `real_reader_pages_utf8_without_raw_reasoning_and_refuses_large_input_before_parsing`: a
  30 MiB file is refused for a full read and returns its overview with `overviewOnly`.
- Register SS2 (amended) and CL16 for the chunked readers; FRD-0006 R49.
