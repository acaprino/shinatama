---
adr_id: ADR-0053
version: 2
title: No session is too large to resume, view, search or export; Mukade owns how a session of any size is read, and Shinatama only asks for what each operation needs
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - crates/engine/src/session_detail.rs
    - crates/engine/src/session_find.rs
    - crates/engine/src/session_export.rs
    - apps/desktop/src-tauri/src/agents/resume.rs
    - apps/desktop/src-tauri/src/titles.rs
    - scripts/package-local.ps1
    - Cargo.toml
  modules:
    - mukade
  domains:
    - session-reading
    - resume
    - dependency-policy
governs_symbols:
depends_on:
  - ADR-0050
  - ADR-0047
supersedes: ADR-0053 v1
created_from_task: large-session-resume-2026-11-13
proposed_on: 2026-11-13
proposed_at_sha: 20b7ed4a0f7ec89dc3eb3b43dabcc00f6b31012e
accepted_on: 2026-11-13
---

# ADR-0053 v2: No session is too large to resume, view, search or export; Mukade owns how a session of any size is read, and Shinatama only asks for what each operation needs

Version 1 stands where this version does not change it: no operation refuses a session for its
size, and compression is rejected. This version changes who does the reading and how a large
session is read, and adds three findings.

## What version 1 got wrong

1. **The reading was in Shinatama.** Version 1 put the overview and the size handling in
   Shinatama's engine. Reading a native session is Mukade's (FRD-0006 R01, R02); the user,
   2026-11-13: Shinatama is not to carry responsibilities and code that are Mukade's.
2. **Chunks from the end cannot give the same history.** Mukade does not choose Claude Code's
   history by walking `parentUuid` back from a leaf: its reconstruction
   (`src/reader/claude_code/order.rs`, 1,116 lines) recovers summaries, expanded prompts and
   ancestors over the whole file and orders the chosen rows topologically.

## Findings, 2026-11-13

1. **Most of the time is the debug build.** The same 10.6 MB session: 3.3 s through the
   installed debug helper, 0.2 s and 57 MB through a release build of Mukade (`mukade dump`).
2. **Most of the bytes are tool output.** Every string clipped at 4 KiB, the length the view
   shows, keeps 37% of that session's bytes (146 strings clipped) and every structural field.
3. **Mukade's current version brings a chat service** (MCP server and client, HTTP, an async
   runtime) into the library; ADR-0047: no dependency arrives as a side effect.

## Decision

**Mukade** owns reading a session of any size. Its readers gain, for every harness it reads:

1. **An overview of one session**: what its listing reads from the file's two ends (id, working
   directory, title, update time, parent), in constant time, never reading the conversation.
2. **A read that never refuses for size.** Up to a threshold Mukade decides (16 MiB to start),
   the session is read whole, as today. Past it, Mukade decodes it one record at a time from the
   file, never holding its text, and clips every string longer than a length the caller names
   as it decodes, keeping each record's byte offset and the file's digest. The same
   reconstruction runs on the clipped records, so the history, its order and its branches are
   those of the full read.
3. **The full text of the session's records in history order, one record at a time**, read back
   by offset, for search and export.

The size threshold, the clipping and the reading by offset are Mukade's alone and tested in
Mukade against its fixtures and a synthetic session past 100 MiB.

**Mukade's chat service becomes a cargo feature, `chat`,** on by default for Mukade's own
programs.

**Shinatama** keeps no reading logic and no size bound of its own:

- it depends on Mukade with `default-features = false`, so the pin moves forward with the
  readers' small dependency set (anyhow, chrono, rusqlite, ruzstd, serde, serde_json, toml_edit,
  uuid, windows-sys);
- resume and thread names ask Mukade for the overview; the overview code and the 16 MiB check
  now in `crates/engine/src/session_detail.rs` are removed;
- the view asks for the clipped read with 4 KiB, its own display length;
- search and export ask for the full records in history order;
- the contained helper, its budgets and the request between host and helper stay Shinatama's;
  the helper's wall time grows with the file's size for search and export, which show progress;
- the session helper is built in release mode in every package.

## Rejected Alternatives

- **The reading in Shinatama's engine** (version 1): Mukade's responsibility in Shinatama.
- **Chunks from the end along `parentUuid`** (version 1): a different history from the full read.
- **Compression**: saves disk, not reading (version 1).
- **Only the release build**: memory stays about five times the file.
- **Mukade at its current version with the chat service**: dependencies as a side effect.

## Enforceable Invariants

- **INV-001**: no session operation refuses a session for its own size.
- **INV-002**: Shinatama's code holds no session size threshold, no native record parsing and
  no clipping; it calls Mukade.
- **INV-003**: an overview never parses a record outside the file's head and tail windows.
- **INV-004**: past Mukade's threshold no step holds the session's text whole.
- **INV-005**: for the same file, a clipped read and a full read give the same entries in the
  same order; the clipped strings are prefixes of the full ones.
- **INV-006**: the session helper of a package is a release build.

## Examples

- **EX-001** (nominal): a 30 MiB Claude session is resumed -> Mukade's overview confirms id,
  harness and project in constant time; Claude reopens it.
- **EX-002** (nominal): a 100 MiB session is viewed -> Mukade decodes it one record at a time
  with its long texts clipped; the pages equal those of a full read.
- **EX-003** (nominal): a 100 MiB session is exported in full -> Mukade hands its records' full
  texts in order, one at a time; the export shows its progress.
- **EX-004** (error): a record in the middle of the file does not decode -> the error names its
  line, as today; never a size refusal.

## Consequences

- Shinatama's engine loses code: the overview and the size check it gained on 2026-11-13.
- The installed session helper is about fifteen times faster.
- Shinatama's pin moves to the Mukade version that has these reads, without the chat service.

## Confirmation

- In Mukade: a synthetic Claude session past 100 MiB, clipped and full reads compared (INV-005),
  peak memory and time recorded.
- In Shinatama: register SS2 and CL16 through the contained helper; FRD-0006 R49 as written.
