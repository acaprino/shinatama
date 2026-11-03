# C25.1 durable thread metadata and sidebar

Schema 4 adds thread, segment and activation records through the existing
verified-backup migration. The migration preserves project, root, layout and
settings tables. No native archive is rewritten or migrated. Unknown newer
databases retain the existing read-only behavior.

Thread metadata uses canonical IDs and revisions. Segment bindings require an
explicit native session key and exact store revision; the product does not
invent one before the agent reports it. Activation records have unique open
thread/segment/native bindings and exact PID plus creation-time ownership.
These are durable records, not a claim that a native process is alive. There is
no TTL takeover, automatic resume or crash-recovery qualification in this slice.

The desktop sidebar supports explicit project-scoped, bounded lists, manual
**New thread**, metadata selection, keyboard/pointer resizing and collapse.
Width/collapse are saved under one fixed typed preference key with revision
checks. Native activity and unbound sources remain unknown. Selecting a row
never starts a process. Successful native startup now saves its work-thread
record before the contained terminal is started; a failed start may leave an
explicitly unbound draft rather than losing its saved identity.

Essential checks on Windows, 2026-11-03:

1. State test passes: v3-to-v4 upgrade, saved project/layout/settings preserved,
   verified backup, reopen, stale writes, two independent SQLite connections,
   exclusive activation metadata, PID reuse refusal, source unavailable state,
   archive state and sidebar preference conflicts. Runtime 0.16 seconds.
2. Existing migration rollback/retry test passes against schema 4, 0.11 seconds.
3. Frontend and desktop/helper builds pass. The Windows desktop executable now
   uses the GUI subsystem in debug builds too, avoiding a second console window.
4. Native DB-only UI flow passes: manual canonical thread, read-only selection,
   width 264-to-280, collapse saved before reload, preferences and same thread
   restored, missing project and stale preference writes refused, no terminal
   startup and owned-window close.

Receipt: `.prototype-evidence/threads-sidebar.json`. Log:
`.verification/c25-saved-threads-live.log`.

The separate real-Claude integration attempt was deferred by the unchanged
global resource policy before starting the sidecar/agent. Its log is
`.verification/c25-thread-sidebar-live.log`; it is not a passing native-start
qualification for this new persistence hook. The DB-only flow remains usable
while new native workloads are deferred. Full resume, native ID discovery,
authoritative activation recovery and parent C25 acceptance remain open.
