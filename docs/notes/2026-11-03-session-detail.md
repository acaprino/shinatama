# C23 bounded read-only session detail

The desktop now opens an explicitly selected session through the same contained
session helper as metadata listing. The host resolves the authoritative root and
revision, sends no caller-supplied path, and rechecks the exact record before
accepting the response. A retained uncertain cleanup prevents a replacement read.

The helper calls the actual pinned Mukade reader and keeps no size bound of its
own (ADR-0053 v2): no session is refused for its size. Mukade owns how a session
of any size is read; the helper only asks for what each operation needs. The one
bound left is on the archive: more than 4,096 files in the pinned namespace is
refused before any read, since it bounds the archive, not a session. The
per-job memory, CPU, queue and output limits from C22 remain unchanged; the
ten-second wall remains for list, detail and overview, and a search or an export
gets one more second per 32 MiB of the session's file, at most 120 s (the parent
takes the file's size from a bounded walk by file name; a miss costs the base
wall and never changes what is read).

An overview-only request (`overviewOnly`) calls `Reader::overview`: Mukade reads
the session file's two ends (title, working directory, update time, parent) and
never the conversation, so the call costs the same whatever the session's size.
The reply has no event, ledger or activity; its lineage is that of the one row
(the parent is unresolved, the children unknown). Resume verifies the native
session with it (exists, same harness, same project folder) and thread names come
from it. A change detector built from the file's size (the pinned namespace) and
the overview's fields stands where the page's fingerprint would be; it is not a
cursor.

Viewing calls `Reader::read_clipped` with every string cut at 4,100 bytes (the 4,096
the view shows plus the longest UTF-8 character, so a text over 4,096 is still
known to be over it). Mukade reads a file past 16 MiB one record at a time and
never holds its text; the session it returns has the full read's entries, ids,
kinds and order. Search and export call `Reader::read_streamed` (every entry with
its full text, one at a time; an export leaves the worker in bounded frames, see
`2026-11-09-session-export.md`); RAW calls `read_clipped` at its own 64 KiB bound.
The fingerprint is Mukade's digest of the whole file, the same for all three.

Responses contain at most 50 canonical events and 256 KiB. Each text projection
is UTF-8 bounded at 4,096 bytes. Cursors bind root, revision, native ID, reader pin
and source digest. Unknown metadata stays unknown. Raw records, reasoning, tool
arguments and attachment contents are omitted. No remote content or native agent
is opened. Source namespace pins and their documented concurrent-writer limitation
are the same as C22.

The interface provides Overview, Transcript and the recorded parent ID. Full
lineage navigation, raw view/copy, export, resume and hop remain unqualified.
Closing a detail returns to the session list. Stale results do not populate a
different selection; one request remains outstanding across close/reopen.

Essential checks passed on Windows on 2026-11-03:

1. One real-reader fixture: canonical pagination, UTF-8 clipping, raw/reasoning
   omission, unchanged input, path-selector refusal and a sparse malformed
   30 MiB file refused before parsing. Runtime 0.05 seconds.
2. Frontend build and contained helper/desktop builds.
3. Native app flow with 53 synthetic sessions and one 53-event conversation:
   metadata/detail pages 50/3/50, literal HTML remains inert, unknown overview and
   parent remain unknown, detail returns to the library, source bytes unchanged
   after close, and no agent/terminal starts.

Log: `.verification/c23-detail-live.log`. Receipt:
`.prototype-evidence/sessions-library.json`. Detail screenshot:
`.prototype-evidence/sessions-detail.png`.

This is a working C23 slice, not completion of all harness conformance, full
lineage/privacy actions or the parent C23 requirement.
