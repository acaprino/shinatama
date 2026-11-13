---
adr_id: ADR-0054
version: 1
title: Hermes Agent and DeepSeek Harness sessions are read through Mukade from read-only system roots; the pin protects identity, not size, for Hermes's live SQLite store; every claim waits for its register row
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - crates/engine/src/session_roots.rs
    - crates/engine/src/session_detail.rs
    - crates/engine/src/session_find.rs
    - crates/engine/src/session_export.rs
    - apps/desktop/src-tauri/src/sessions.rs
    - apps/desktop/src-tauri/src/sessions/search.rs
    - apps/desktop/src/main.ts
    - docs/frd/FRD-0006-search-sessions-and-resume.md
    - docs/verification-register.md
    - Cargo.toml
  modules:
    - mukade
  domains:
    - session-reading
    - store-roots
    - dependency-pin
governs_symbols:
depends_on:
  - ADR-0020
  - ADR-0023
  - ADR-0053
supersedes:
created_from_task: hermes-deepseek-session-design-2026-11-13
proposed_on: 2026-11-13
proposed_at_sha: 20b7ed4a0f7ec89dc3eb3b43dabcc00f6b31012e
---

# ADR-0054: Hermes Agent and DeepSeek Harness sessions are read through Mukade from read-only system roots; the pin protects identity, not size, for Hermes's live SQLite store; every claim waits for its register row

Line numbers below are those of 2026-11-13 and drift: other work is editing the same files.

## Context

Hermes Agent and DeepSeek Harness (DSH) are refused today as `UnsupportedHarness`:
`crates/engine/src/session_detail.rs:103-106`, `crates/engine/src/session_roots.rs:663-669`
(`harness()`) and `:933` (the namespace subtrees). FRD-0006 R03 says they stay refused "until a
reviewed pin carries their readers"; FRD-0001 R02 says a capability is claimed only once its
register row passes (HM3, DS3 are the reader rows, `docs/verification-register.md` 7a, 7b).
The Mukade revision Shinatama is moving to carries both readers (`../mukade-chunked/src/reader/hermes.rs`,
`.../deepseek_harness.rs`), which is the first half of R03's condition. Reading what they do:

1. **Neither reader has the ADR-0053 v2 operations.** `Reader::overview`, `read_clipped` and
   `read_streamed` have defaults (`src/reader/mod.rs:76-101`): list everything and pick the row,
   read whole and clip, read whole and hand over. Claude Code, Codex and Pi override them
   (`claude_code.rs:1627`, `codex.rs:1288`, `pi.rs:876`); Hermes (`hermes.rs:985-1002`) and DSH
   (`deepseek_harness.rs:1049-1080`) implement only `list`, `list_all` and `read`.
2. **Hermes's `Reader::read` never sets `meta.source_sha256`.** Only the export-file path does
   (`hermes.rs:129`); `read_session` (`hermes.rs:783-835`) leaves it `None`.
   `session_fingerprint` refuses a read without a 64-hex digest as `ReadUnavailable`
   (`crates/engine/src/session_detail.rs:459-468`). Every view, search, export and RAW read of a
   Hermes session would fail for that reason alone, before size is an issue.
3. **DSH's listing decodes every log whole.** `listed` reads and decompresses the entire file to
   find the header, the last `session/title` and the first prompt (`deepseek_harness.rs:873-915`,
   `peek` at `:849-871`); `read_file` does `std::fs::read` and joins every frame into one `String`
   (`:164-167`, `:150-152`). A listing costs the archive, an overview costs the archive, and the
   view's lineage catalog (`session_detail.rs:395`, `list_all`) costs the archive again. The
   listing writes a cache to `mukade-data/cache` beside the executable
   (`src/portable.rs:42-57`, `src/cache.rs:33-50`) unless the host selects `MUKADE_DATA_DIR`.
4. **Hermes's reader opens SQLite read-only and never `immutable`, and says why**
   (`hermes.rs:6`, `:607-615`): a gateway may be writing. Its own documentation records that a
   read-only open of a WAL database can create `-wal` and `-shm` sidecars
   (`docs/harnesses/hermes-agent.md:43`). It reads a session with several separate statements and
   no explicit read transaction (`hermes.rs:734-835`), and retries once on `SQLITE_BUSY` or
   `SQLITE_IOERR` after 200 ms (`:619-635`).
5. **The namespace pin was built for append-only files.** It holds an open handle per file and
   directory with `FILE_SHARE_READ | FILE_SHARE_WRITE` (`session_roots.rs:878`), compares identity,
   size and last-write time on every `verify_current` (`:974-978`) and re-captures the namespace
   (`:979-989`). The comment at `:832-836` states the limit: it rejects observed changes and is not
   isolation from a concurrent writer. For `state.db` in WAL mode that comparison is the wrong
   detector: the main file changes only at checkpoints, the content changes in `-wal`, and our own
   read may create `-wal` and `-shm`.
6. **Hermes has not OpenCode's problem.** OpenCode is refused for detail because its pinned
   `Reader.read` launches the harness exporter (`session_detail.rs:107-110`,
   `UnsupportedExportContext`; FRD-0006 R07). Neither Hermes's nor DSH's reader spawns a process
   (no `Command` in `hermes.rs` or `deepseek_harness.rs`); they read files and a SQLite database.
7. **FRD-0006 R06 contradicts what Mukade built.** R06 says Hermes's own export is the primary
   source and SQLite a read-only fallback. Mukade reads `state.db` as primary
   (`hermes.rs:985-1002`) and offers the export only as `read_file` over a file someone hands it
   (`hermes.rs:102`). Running `hermes sessions export` means launching a Python harness from the
   contained helper, which R07 forbids without the adapter's full isolated launch context.
8. **Shinatama has no way today to register these roots.** Box roots exist only where an agent is
   launched in a private home (`apps/desktop/src-tauri/src/agents.rs:466-475`); Hermes and DSH
   have no launch adapter (`apps/desktop/src/agent-setup.ts:28-29`, `checkable: false`). The only
   other route is the user-added system root, which accepts `claude_code`, `codex` and `pi` and
   nothing else (`apps/desktop/src-tauri/src/sessions.rs:403`). The root record's `harness` must
   equal Mukade's `HarnessId::as_str()` (`session_roots.rs:688-693`): `hermes` and
   `deepseek_harness`, while the adapter catalog says `hermes_agent` (`crates/adapter/src/catalog.rs:44`).

## Decision

### 0. The user's decisions, 2026-11-13

1. **FRD-0006 R06: for Hermes the SQLite store is the primary source.** Hermes's own export
   would mean launching a process from the contained helper, the reason OpenCode is refused
   (point 7 of the Context). The export stays readable as a file the user hands over.
2. **A Hermes read creates no file in the user's store; FRD-0006 R17 stands as written.** Mukade
   chooses how to open `state.db` from its sidecars (section 4, M7): both present, read-only,
   joining the WAL; both absent, `immutable=1`, validated after the read; anything else, no read.
   The P0 probes confirm SQLite's behaviour before this ADR is accepted.

This ADR stays Proposed until the P0 probes; the work order is: the ADR-0053 v2 export first,
then commit, pin and install, then P0, then the user's go or no-go on the remaining phases.

### 1. Store roots

- Both are registered as **read-only system roots**: `RootKind::System`, `RootScope::Additional`,
  `RootAccess::ReadOnly` (`sessions.rs:426-436`), with `harness` = `hermes` or `deepseek_harness`.
  There is no box root until the launch adapters pass their rows (HM1, HM9, DS1, DS9).
- The root path is the **home directory**, not `state.db` (Mukade's `store_is_file` is false for
  both; the check is `session_roots.rs:709`): Hermes `%LOCALAPPDATA%\hermes` by default, moved by
  `HERMES_HOME` (`docs/harnesses/hermes-agent.md:68-69`); a named profile is another home folder
  and another root. DSH `%USERPROFILE%\.dsh` by default, moved by `DSH_HOME`; its sessions are
  under `<home>\sessions`.
- Registration is **by the user**, never by an ambient default (FRD-0006 R14, R24):
  (a) the existing "Load archive" action gains the two harnesses; (b) a proposal list built from
  Mukade's catalog (`mukade::catalog::store_default`, `src/catalog.rs:185`) and its recognition
  rule (`state.db` for Hermes, a `sessions/` tree for DSH) shows what exists on this machine; the
  user confirms each, recorded with `RootOrigin::Catalog`. Shinatama never reads its own
  `HERMES_HOME` or `DSH_HOME`: they describe the agent's environment, not Shinatama's.
- The record's `harness` is Mukade's id. One table maps adapter ids (`hermes_agent`) to Mukade ids;
  no other file spells either.

### 2. The namespace pin

- **DSH**: the same policy as Codex's `sessions` tree (`session_roots.rs:931`): `sessions/`
  enumerated and pinned, identity, size and write time compared. DSH appends whole zstd frames, so
  an append is the same observed change as a Claude append. Because a DSH session is a folder with
  several files, not one file, the pin caps are raised for DSH only (entries 4,096 to 16,384,
  directories 1,024 to 4,096, files 4,096 to 16,384: `:816-817`, `session_detail.rs:21`); the
  measured cost decides in the plan whether this holds or pins only the located session.
- **Hermes**: the pin gains a second policy, **database**. It holds the ancestor chain and a
  handle on `state.db` (identity, link count and attributes compared; size and write time not).
  It does not pin `state.db-wal` or `state.db-shm`: they are SQLite's, Shinatama never opens them,
  and a read may create them. The change detector for a Hermes read is not file metadata but
  Mukade's digest of the session's own rows (point 5 below) and a read transaction (point 4).
  `file_size` refuses a database pin, so a Hermes row has `bytes: None` (shown as `unknown`,
  FRD-0006 R18) instead of the size of the whole database repeated on every row
  (`session_roots.rs:1109`).
- Replacement of `state.db` while pinned is still refused: the handle denies `DELETE` sharing
  (`:878`), and a replacement by rename changes the file identity, which `verify_current` compares.

### 3. Operations

| Operation | Hermes | DSH | Waits for |
|---|---|---|---|
| list (tree, children, archived flag) | yes | yes | Hermes: roots and pin. DSH: also M5 |
| overview, thread names | yes | yes | M1 (Hermes), M5 (DSH) |
| view (paged, with lineage) | yes | yes | M2 and M6 (Hermes); M5 (DSH) |
| in-session search, indexed search | yes | yes | the view; index is `sessions/search.rs`, unchanged |
| export (STX, portable JSON), hop source | yes | yes | the view, plus SS4 for hop |
| RAW (redacted by default) | yes | yes | the view |
| resume (open in a terminal pane) | **refused** | **refused** | launch adapters: HM1, HM6, DS2 |

Resume stays `unsupported_harness` (`apps/desktop/src-tauri/src/agents/resume.rs:80`, `:172-174`):
Hermes needs `hermes --resume <id>` from an adapter that starts it inside a box (HM6), and DSH has
no terminal UI at all (`docs/harnesses/deepseek-harness.md:16`); its resume is the ACP pane's
`session/resume` (DS2, ADR-0007). Reading a session never needs either.

### 4. Hermes read isolation

- Mukade holds **one read transaction per operation** (M6), so a session row, its parent chain
  and its messages come from one snapshot, and a listing is one snapshot.
- The contained helper runs as the user with a job object and no filesystem sandbox
  (`session_roots.rs:1460-1470`, a cleared environment and a private working directory), so a
  plain read-only open of a cleanly closed store could create `-shm` and `-wal`. To keep FRD-0006
  R17 as written (the user's decision, section 0), Mukade chooses the open mode from the sidecars
  (M7):
  - **`state.db-wal` and `state.db-shm` both present**: a connection is open, a gateway may be
    writing. Read-only open, joining the WAL; the files already exist, so nothing is created.
  - **Both absent**: no connection is open and every committed transaction is in `state.db`.
    `immutable=1` open, which creates nothing and takes no lock. After the read Mukade checks that
    `state.db`'s identity, size, last-write time and header (file change counter, bytes 24-27)
    are those it saw before the read. If any changed, a gateway started and checkpointed during
    the read: the result is discarded and the read runs once more read-only, the sidecars now
    present. If `state.db` is unchanged, every page read belongs to one state, the one committed
    before the read, even if a gateway has since started writing to a new `-wal`.
  - **One present without the other** (a crash, or the instant a connection opens or closes):
    after one 200 ms retry, `ReadUnavailable` ("Hermes is opening or recovering its store; try
    again"). Neither open mode would be safe: `immutable=1` would ignore a committed `-wal`, and a
    read-only open would create the missing file.
  The main `state.db` is never written: neither mode checkpoints.
- While Hermes writes, a reader joins the WAL through `-shm`, sees a consistent snapshot, and does
  not block the writer. Limits that remain: (a) a read held across a checkpoint prevents the WAL
  from being reset, so it grows for the length of the read (the wall is at most 120 s,
  `session_roots.rs:36`); (b) `SQLITE_BUSY` or `SQLITE_IOERR` during a checkpoint is retried once;
  a second failure is `ReadUnavailable`, shown as "busy, try again", not as a size or source
  error; (c) WAL needs a local volume: `StorageLocation::preflight` already refuses non-local
  drives (`session_roots.rs:695`); (d) a session being written changes between page requests: the
  cursor's fingerprint (a digest of that session's rows) no longer matches and the page is
  `SourceChanged`, exactly as a live Claude session today.
- If the gateway holds the database in exclusive locking mode, no reader can join; that would show
  as `SQLITE_BUSY` every time. HM3 must prove it does not (open question 1).

### 5. Mukade changes (Mukade owns reading; each is a revision to `mukade-chunked`)

| ID | Change | Where |
|---|---|---|
| **M1** | `Hermes::overview`: one `SELECT` of the session row by whole id, its parent's `end_reason` for lineage, `first_prompt` for that id only; `NotFound` and `Ambiguous` as now. Never lists the store. | `src/reader/hermes.rs` (Reader impl, `:985`) |
| **M2** | `read_session` sets `meta.source_sha256`: SHA-256 of the session rows read (the chain's `sessions` rows and every `messages` row in `id` order, as the JSON the reader builds). Not the database file: it holds other sessions and changes at checkpoints. Computed per row so M3 can stream. | `src/reader/hermes.rs:783-835` |
| **M3** | `Hermes::read_clipped` and `read_streamed_with_meta`: iterate `messages` by `id` with a prepared statement, clip strings as the cell is read, keep the lookahead `Builder::push` needs (`hermes.rs:333`), digest as in M2. Memory follows the biggest row. | `src/reader/hermes.rs` |
| **M4** | `DeepSeekHarness::read_clipped` and `read_streamed_with_meta`: scan frames (`zstd_frames::scan`, `src/reader/zstd_frames.rs:46`), decode one frame at a time, digest the file in chunks instead of `std::fs::read` (`deepseek_harness.rs:164`); two passes for `surfaceOp: replace`, as Claude's. | `src/reader/deepseek_harness.rs` |
| **M5** | `DeepSeekHarness::overview` and a head/tail `listed`: header from frame 1, first user prompt from the first frames that hold one, latest `session/title` from the last frames within a bounded tail window; constant cost per log. The cached listing (`list_all_cached`) keeps its stamp. Mukade decides the title rule for a title outside both windows and records the difference. | `src/reader/deepseek_harness.rs:873`, `:849` |
| **M6** | Hermes reads (`read_session`, `listed`, `store_counts`) inside one deferred read transaction. | `src/reader/hermes.rs:645`, `:783`, `:853` |
| **M7** | The open mode from the sidecars (section 4): both present read-only; both absent `immutable=1` with the after-read check of `state.db` (identity, size, last-write time, header bytes 24-27) and one read-only rerun on a change; one alone `ReadUnavailable` after one 200 ms retry. A test for each case, plus a gateway checkpointing during an `immutable` read. | `src/reader/hermes.rs:6`, `:607-635` |

Nothing is needed for `Lookup`, `HarnessId` or the list rows: `parent` and `parent_kind` are set by
both (`hermes.rs:171`, `deepseek_harness.rs:912`).

### 6. Shinatama

- The pin advances to the Mukade revision that carries M1, M2, M6 (Hermes) and M5 (DSH) first,
  M3 and M4 after; `MUKADE_REVISION` in the helper request (`session_roots.rs:1159`) moves with it
  (FRD-0006 R01: a revision counts only once the pin is advanced and rechecked).
- Shinatama gains no parser, no size threshold, no SQL: it opens roots, pins, asks Mukade.
- The helper's environment gains `MUKADE_DATA_DIR` set to its private working directory
  (`session_roots.rs:1460-1470`), so the DSH listing cache lives in Shinatama's private storage
  and never beside the helper's executable.
- Lineage for these two harnesses is `ParentMetadata::Complete` (`session_detail.rs:401`, `:417`),
  because both record the parent link in the store (`mukade::catalog::lineage_recorded`,
  `src/catalog.rs:221`); Claude, Codex and Pi stay `Unavailable` as now.
- FRD-0006 is amended: R03 (lifted per operation, as the table above), R06 (Hermes: SQLite is
  primary because Mukade's reader launches nothing; the export is read only as a file the user
  hands over; amended 2026-11-13), R17 unchanged (M7 creates no sidecar), R23 (native children for Hermes and DSH), R14 (the proposal
  list).

## Rejected Alternatives

- **Hermes's own export as the primary source** (FRD-0006 R06 as written): launches Python from
  the contained helper, which needs R07's full launch context; the helper's job has no such
  context. Mukade's reader already fails closed on a schema it does not know (`hermes.rs:645-681`).
- **`immutable=1` always**: reads a database a gateway may be writing as if it were frozen; wrong
  rows, no error. Mukade refuses it for this reason (`hermes.rs:6`). Adopted only when no sidecar
  exists and with the after-read check (M7).
- **A plain read-only open always** (this ADR's first draft): creates `-wal` and `-shm` in a
  cleanly closed store, a write into the user's store that FRD-0006 R17 forbids; the user kept
  R17 (section 0).
- **A pinned copy of `state.db` and `-wal` in the helper's private directory**: the copy is not
  atomic with a writer (a checkpoint between the two copies gives a database and a WAL that do not
  belong together) and costs the database's size. SQLite's backup API would be consistent but is
  reading logic in Shinatama (ADR-0053 INV-002).
- **Pinning `-wal` and `-shm` by size and time**: a gateway turn changes them; every read of a
  live Hermes would fail `SourceChanged`, and our own read creating them when absent would fail
  its own check.
- **Reading `HERMES_HOME` and `DSH_HOME` from Shinatama's environment**: an ambient default,
  refused by FRD-0006 R14 and R24.
- **A box root for each**: there is no launch adapter to make one (point 8 of the Context).
- **Shinatama computes the Hermes digest itself**: reading logic in Shinatama.
- **Release Hermes and DSH reads together with the existing defaults and a size guard**: a guard
  is a size refusal (ADR-0053 INV-001, INV-002); without it a very large DSH session exhausts the
  helper's 768 MiB process commit (`session_roots.rs:1446`) and shows as an unavailable worker.

## Enforceable Invariants

- **INV-001**: no Hermes or DSH root is registered from an environment variable or a default
  path without the user's confirmation, and its record's `harness` is `hermes` or
  `deepseek_harness`.
- **INV-002**: a capability of these two harnesses is claimed in the adapter and shown in the UI
  only when its register row passes (FRD-0001 R02); until then the operation answers
  `unsupported_harness`.
- **INV-003**: a Hermes read creates, writes or deletes no file in the root: the set of files,
  and `state.db`'s bytes and last-write time, are equal before and after a read that no gateway
  overlapped.
- **INV-004**: no Hermes pin compares the size or write time of `state.db`, `-wal` or `-shm`;
  every Hermes fingerprint is a digest of the session's rows.
- **INV-005**: a Hermes or DSH read never launches a process.
- **INV-006**: no Shinatama code holds a Hermes or DSH size threshold, SQL or frame parsing.
- **INV-007**: past Mukade's threshold no step of a Hermes or DSH read holds the session's text
  whole (ADR-0053 INV-004), proved by M3 and M4 tests; until they land the view, search and export
  of these harnesses are not claimed.
- **INV-008**: the DSH listing cache never lies beside the helper's executable.

## Examples

- **EX-001** (nominal): the user loads `%LOCALAPPDATA%\hermes` as a system root while the gateway
  is running a turn; the list shows its sessions with `bytes` unknown; opening one pages its
  transcript; a new turn in that session makes the next page `SourceChanged` and "Reload" works.
- **EX-002** (nominal, cold): Hermes is closed; opening a session creates two empty sidecars;
  `state.db` is byte-identical afterwards; the next read does not fail its own pin.
- **EX-003** (error): `state.db` is schema 31; Mukade refuses it with its number
  (`hermes.rs:665-672`); the UI says "run a newer Hermes once", not "too large" or "source changed".
- **EX-004** (error): the user points the root at `%LOCALAPPDATA%\hermes\state.db`; the shape
  check refuses it as `UnsupportedRootShape` (a file where a directory is required).
- **EX-005** (nominal): a DSH session with a torn last zstd frame lists, opens and exports with an
  `Unfinished` marker; `session.lock` is held by a running `dsh` and nothing waits for it.
- **EX-006** (refusal): "Resume" on either harness answers `unsupported_harness`; "Copy resume
  command" is not offered.

## Consequences

- The Hermes pin differs from every other pin in what it detects; `verify_current` gains a policy
  per pinned object. Existing Claude, Codex and Pi comparisons are unchanged.
- A Hermes root cannot show a per-session size; a DSH root can.
- Hermes is cheaper than DSH: M1, M2 and M6 are small; M3, M4 and M5 are real work (the plan).
- DSH's per-session folders make the pin walk longer than Claude's per-project folders; the plan
  measures it before the raised caps stand.
- The user decided R06 and the sidecars on 2026-11-13 (section 0): R06 is amended, R17 is not.

## Confirmation

- Register: **HM3** and **DS3** run through the contained helper at the advanced pin, not only in
  Mukade's tests; **SS1** (every harness the pin reads is listed), **SS2** (paged read, in-session
  search, large input), **SS3** (native lineage), **SS5** (roots are read-only system roots with
  identity and revision), **SS6** (export formats and losses); **SS4** before a hop source.
  New rows added with this ADR: **HM15** (sidecars and store untouched, no process launched),
  **HM16** (a 100 MiB Hermes session), **DS16** (pin cost at 2,000 sessions, a 100 MiB DSH
  session). The plan lists the tests.
- A written-back note in FRD-0006 and `docs/mukade-handover.md:362`.

## Open questions

1. Does the Hermes gateway use exclusive locking or a checkpoint mode that stops a read-only
   reader joining? (HM3, live.)
2. Does `dsh` hold `session.lock` with a share mode that lets the pin open it? If not, the pin
   must skip non-log files, which is a rule about DSH's layout. (DS3, live.)
3. Do real Hermes and DSH ids fit `native_id()` (`session_detail.rs:189`: ASCII alphanumeric,
   `-`, `_`, at most 256)? The census test `tests/hermes_live.rs` answers it for Hermes.
4. Resolved 2026-11-13 (section 0): SQLite primary; no sidecar is ever created (M7). Left to P0:
   whether a checkpoint always changes what M7's after-read check compares.
5. Mukade's title rule when the last `session/title` lies outside the tail window (M5).
