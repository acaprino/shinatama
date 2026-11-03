# C23 bounded read-only session detail

The desktop now opens an explicitly selected session through the same contained
session helper as metadata listing. The host resolves the authoritative root and
revision, sends no caller-supplied path, and rechecks the exact record before
accepting the response. A retained uncertain cleanup prevents a replacement read.

The helper calls the actual pinned Mukade reader. It refuses input before parsing
when the complete effective archive namespace exceeds 16 MiB or 4,096 files.
This conservative whole-archive limit is intentional: this pin materializes a
whole session and does not provide bounded input pagination. The per-job memory,
CPU, queue, output and ten-second wall limits from C22 remain unchanged.

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
