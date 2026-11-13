# Plan: Hermes Agent and DeepSeek Harness in the session library

Date: 2026-11-13. Status: proposal, follows ADR-0054 (Proposed). Nothing here is claimed until its
register row passes (FRD-0001 R02). Line numbers are those of 2026-11-13 and drift; search by symbol.

Scope: list, overview, view, search, export and hop source for both harnesses. Resume stays refused
(needs the launch adapters: HM1, HM6, DS2). Mukade owns reading; Shinatama opens roots, pins, asks.

## Order and what each phase claims

| Phase | Delivers | Needs Mukade | Claims (register) | Hours |
|---|---|---|---|---|
| P0 probes | Facts that decide the pin: WAL join with a live gateway, sidecars, `session.lock` share mode, real id charset | nothing | none | 5 |
| P1 Hermes list and overview | System root, database pin, tree, thread names | M1 | list, overview (HM3 partial, SS1) | 17 |
| P2 Hermes view, search, export | Detail, find, export, RAW, hop source | M2, M6, M7 (then M3) | read_transcript, export (HM3, SS2, SS3, SS6; HM15, HM16) | 14 + 10 Mukade |
| P3 DSH list and overview | System root, `sessions` pin with raised caps, tree | M5 | list, overview (DS3 partial, SS1) | 14 |
| P4 DSH view, search, export | Detail, find, export, RAW | M4 | read_transcript, export (DS3, SS2, SS3, SS6; DS16) | 7 + 12 Mukade |
| P5 write-back | FRD-0006, register, handover | none | none | 5 |

Hermes first: its Mukade work is small and its pin is one file. P3 may start in parallel with P2.
Total: Shinatama about 62 h, Mukade about 35 h, probes and write-back included.

## Mukade changes (`D:\Projects\mukade-chunked`, owned by Mukade; Shinatama never edits them)

| ID | What | File | Tests in Mukade | h |
|---|---|---|---|---|
| M1 | `Hermes::overview`: one row by whole id, parent's `end_reason`, `first_prompt` for that id only; no store listing | `src/reader/hermes.rs` (Reader impl) | overview equals the matching `list_all` row on `tests/hermes_reader.rs` stores; `NotFound`, `Ambiguous`; no `messages` read except one `LIMIT 1` | 3 |
| M2 | `read_session` sets `meta.source_sha256` = SHA-256 of the rows read (chain `sessions` rows, `messages` in `id` order); same digest from `read`, `read_clipped`, `read_streamed` | `src/reader/hermes.rs:783-835` | digest stable across reads; changes when one message is appended, not when another session is | 2 |
| M6 | One deferred read transaction per `read_session`, `listed`, `store_counts` | `src/reader/hermes.rs:645`, `:783`, `:853` | a second connection commits between statements: the read still sees one snapshot (WAL fixture) | 2 |
| M7 | Open mode from the sidecars (ADR-0054 section 4): both present read-only; both absent `immutable=1` plus the after-read check and one read-only rerun; one alone `ReadUnavailable` | `src/reader/hermes.rs:6`, `:607-635` | each case creates no file; a gateway checkpointing during an `immutable` read triggers the rerun | 4 |
| M3 | `read_clipped`, `read_streamed_with_meta` by row iteration, clip at the cell, lookahead for `Builder::push` (`:333`), digest as M2 | `src/reader/hermes.rs` | clipped and streamed equal the full read's entries (ADR-0053 INV-005); 100 MiB session, peak memory recorded | 10 |
| M5 | `DeepSeekHarness::overview`; head/tail `listed`: header frame 1, first prompt from first frames, latest `session/title` from a bounded tail window | `src/reader/deepseek_harness.rs:849`, `:873` | overview equals the old listing's row on the fixtures; a 100 MiB log lists in constant time; title outside both windows recorded | 6 |
| M4 | `read_clipped`, `read_streamed_with_meta` frame by frame via `zstd_frames::scan` (`src/reader/zstd_frames.rs:46`); digest in chunks (replaces `std::fs::read` at `:164`); two passes for `replace` | `src/reader/deepseek_harness.rs:150-175` | equal entries; torn tail still `Unfinished`; 100 MiB log peak memory | 12 |

Each ships with a revision Shinatama can pin. Shinatama's pin advances when a phase needs it
(`Cargo.toml:59`, the `[patch]` at `:67-68` removed at the pin), `MUKADE_REVISION` moves with it.

## Shinatama, file by file

### P0 probes (5 h; scratch homes outside git, per the verification notes)

1. Hermes WAL (2 h): a scratch `HERMES_HOME`, a gateway writing, `sqlite3 -readonly` and a Rust
   read-only connection reading repeatedly: busy rate, WAL growth during a 60 s read, whether
   `state.db` bytes and mtime change. Then M7's modes (ADR-0054 section 4): on a closed store an
   `immutable=1` read creates no sidecar; a gateway that starts and checkpoints during an
   `immutable` read always changes what the after-read check compares (identity, size,
   last-write time, header bytes 24-27); the files present when one sidecar is left alone after a
   killed gateway. Answers open question 1, the rest of question 4 and ADR-0054 INV-003.
2. DSH lock (2 h): hold `session.lock` the way `dsh` does while opening it with the pin's share
   mode (`FILE_SHARE_READ | FILE_SHARE_WRITE`, access `FILE_READ_ATTRIBUTES | 1`,
   `session_roots.rs:878`). Answers open question 2.
3. Ids (1 h): run `tests/hermes_live.rs` (ignored, read-only) and a DSH census; check every id
   against `native_id()` (`session_detail.rs:189`). Answers question 3.
Result file: `docs/verification/2026-11-13-HM3-DS3-reader-probes.md` (the register's own form).

### `crates/engine/src/session_roots.rs` (about 17 h)

- `harness()` `:663-669`: accept Hermes and DSH (the harness name already matches the record,
  `:688-693`). 0.5 h.
- `verify_root` `:688-723`: a Hermes root must hold a regular file `state.db`, a DSH root a
  `sessions` directory; otherwise `UnsupportedRootShape`. Use Mukade's recognition rule if it is
  public (`src/detect.rs`), else the two checks inline. 1 h.
- `capture` `:911-990`: remove the refusal at `:933`; Hermes: pin ancestors plus `state.db` with the
  database policy; DSH: subtree `sessions` with raised caps (`MAX_ENTRIES` `:816`, `MAX_DIRECTORIES`
  `:817`). `Pin` gains a policy field. 4 h.
- `inspect`/`verify_current` `:841`, `:974-990`: the database policy compares volume, file id,
  attributes and link count only; the re-capture comparison counts the one file. 2 h.
- `file_size` `:995`: refuse a database pin, so `list_in_worker` `:1109` gives `bytes: None`.
  0.5 h.
- `row()` `:1044`: `archived` also for Hermes (its `archived`/`auto_archived`, `hermes.rs:~900`).
  0.5 h.
- `session_file_bytes` `:747` (wall for search and export): DSH: the folder whose name holds the id,
  its largest regular file; Hermes: the size of `state.db`, capped by `WORKER_WALL_MAX` (`:36`).
  2 h.
- Helper environment `:1460-1470`: add `MUKADE_DATA_DIR` = the private working directory. 1 h.
- Tests (same file, test module near `:1700`): Hermes root lists with a synthetic `state.db` built
  by a test-only `rusqlite` connection (`dev-dependency`, workspace version `Cargo.toml:38`, bundled;
  check `dependency-policy.md` and ADR-0047: a dev-dependency of the engine crate, already built for
  `state`); DSH root lists with a synthetic zstd log from `mukade::reader::zstd_frames`; replacing
  `state.db` while pinned fails; a root that is `state.db` itself is `UnsupportedRootShape`; no
  `mukade-data` appears beside the helper. 6 h.

### `crates/engine/src/session_detail.rs` (about 7 h)

- Delete the refusal `:103-106`; keep OpenCode `:107-110`. 0.5 h.
- `open` `:306-319`: `MAX_INPUT_FILES` (`:21`) per harness (DSH 16,384); `Opened.bytes` becomes
  `Option<u64>`; the overview digest `:428` hashes its absence. 1.5 h.
- Lineage `:401`, `:417`: `ParentMetadata::Complete` for Hermes and DSH only, from
  `mukade::catalog::lineage_recorded` (`src/catalog.rs:221`) intersected with the two harnesses.
  1.5 h.
- Tests: a Hermes session and a DSH session view page by page; `SourceChanged` when a row is added
  between pages; refusal text for schema 31 (`hermes.rs:665-672`) is `ReadUnavailable` with the
  reason Mukade gave, not a size issue; the helper spawns no child (job process count stays 1).
  3.5 h.
- `session_find.rs`, `session_export.rs`, `session_raw.rs`: no change expected; confirm by the P2/P4
  tests (export of each as STX and portable JSON). 1 h inside the tests above.

### `apps/desktop/src-tauri/src/sessions.rs` and `sessions/search.rs` (about 9 h with the UI)

- `add_archive` `:403`: accept `hermes` and `deepseek_harness`; one table maps adapter ids
  (`hermes_agent`, `deepseek_harness`, `crates/adapter/src/catalog.rs:44-49`) to Mukade ids. 1 h.
- A proposal command: for each of the two, `mukade::catalog::store_default` plus the recognition
  rule; returns candidates without reading any session; the user confirms; registration with
  `RootOrigin::Catalog`. 4 h.
- `apps/desktop/src/main.ts` (`addArchive` `:727` and the archive dialog) and the tree labels: the
  two harness names, the proposal list inline (no popup, per the UI notes), Hermes rows show size
  `unknown`. 3 h.
- `search.rs` `:318`: nothing to exclude; check the signature `updated_at` + `bytes.unwrap_or(0)`
  (`:258-261`) re-indexes a Hermes session when `updated_at` changes (it uses `last_activity_at`). 1 h.
- `sessions/hop.rs:17`: `DESTINATIONS` spells `"deepseek"`, the Mukade and adapter id is
  `deepseek_harness`. Not a source gate; flag for the hop owner. 0 h here.

### Documents (P5, 5 h)

- `docs/frd/FRD-0006-search-sessions-and-resume.md`: R03 (lifted per operation, with the table of
  ADR-0054), R06 done 2026-11-13 (Hermes SQLite primary, export only as a file the user hands over), R17 unchanged, R14 (proposal
  list), R23 (native children for Hermes and DSH). 1.5 h.
- `docs/verification-register.md`: HM3 and DS3 "How" gain "through the contained helper at the pin";
  new rows HM15, HM16, DS16 below; write back to SS1, SS2, SS3, SS5, SS6. 1.5 h.
- `docs/mukade-handover.md:362` ("Hermes first or both at once"): record the order and M1 to M7. 0.5 h.
- `docs/frd/FRD-0001-workspace-and-agent-hosting.md` R02: unchanged; add the capability list that
  stays `unsupported` (resume). 0.5 h.
- ADR-0054 to Accepted after P0 (the user decided R06 and R17 on 2026-11-13). 0.5 h.

## Register rows that must pass, and the test that proves each

| Row | Proof | Phase |
|---|---|---|
| HM3 | Mukade fixtures at the pin (`tests/hermes_reader.rs`: schema 32 and 33, in-place compaction, `_delegate_from`, tools, export equality) **plus** a Shinatama helper test with a second WAL connection appending while the helper pages a session: every page ok or `SourceChanged`, never torn, never `Busy` twice; schema 31 refused with its number | P2 |
| DS3 | Mukade fixtures (`tests/deepseek_reader.rs`: torn frame, header lineage, compaction, sub-agent) **plus** a helper test that holds `session.lock` with the share mode P0 found and reads | P4 |
| SS1 | the tree lists both harnesses from a user-added system root, children included, `unknown` where the store gives none | P1, P3 |
| SS2 | paged view, in-session search and large input through the helper for both (100 MiB), memory and time recorded | P2, P4 |
| SS3 | native parent and children from the stores, cycles and missing parents marked | P2, P4 |
| SS5 | the record: harness `hermes` / `deepseek_harness`, kind System, scope Additional, access ReadOnly, native key includes `store_root_id`; no box root | P1, P3 |
| SS6 | export as STX and portable JSON declares its losses for both; the redacted RAW holds | P2, P4 |
| SS4 | before any hop source from these harnesses: source fingerprint rechecked | after P2/P4 |
| **HM15** (new, G0, SEC) | A Hermes read creates, writes or deletes no file in the root, closed store or live: file set, `state.db` bytes and last-write time equal; no process launched (job count); a second read after the first does not fail its pin | P1, P2 |
| **HM16** (new, G1) | A 100 MiB Hermes session views, searches and exports through the helper within the memory budget, clipped equals full | P2 (needs M3) |
| **DS16** (new, G1) | 2,000 sessions in `sessions/`: pin time and `verify_current` time recorded, list and overview within the wall; a 100 MiB log views, searches and exports | P3, P4 (needs M4, M5) |

## Verification protocol

Build the helper in release (ADR-0053 INV-006), run Mukade's tests in `mukade-chunked`, then
Shinatama: `cargo test -p shinatama-engine --features session-reader` for the engine tests and the
UI check through the built exe over CDP with `SHINATAMA_HOME` outside git, as in the notes on UI
verification. Live runs use scratch homes; the user's real `%LOCALAPPDATA%\hermes` and `~\.dsh` are
only ever read, and only after the user adds them as roots.

## Risks, in the order they bite

1. M3 and M4 are the large Mukade work; without them view, search and export are not claimed
   (ADR-0054 INV-007). P2 can ship with M2 and M6 only if the user accepts "claimed up to the
   size HM16 measures" as an interim row result, which ADR-0053 INV-001 does not allow; the plan
   assumes it does not, so P2's claim waits for M3.
2. DSH pin cost: one directory and several files per session. The raised caps are a guess until
   DS16 measures; the fallback is to pin only the located session for view, find, export and RAW
   (Mukade's overview names the exact file) and keep the full walk for the list.
3. The live Hermes gateway's locking behaviour (open question 1) can make P1 impossible; P0 runs
   first for this reason.
4. `session.lock` sharing (question 2) can make the pin refuse every DSH root while a `dsh` runs.
5. M7's after-read check is only as good as what a checkpoint changes: if P0 finds a checkpoint that
   leaves identity, size, last-write time and header bytes 24-27 equal, `immutable` is dropped and a
   closed store is refused as `ReadUnavailable` until a gateway opens it (R17 kept either way).
