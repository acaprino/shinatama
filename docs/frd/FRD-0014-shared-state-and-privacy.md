# FRD-0014: Shared state, privacy and data retention

| | |
|---|---|
| Version | 1 (new Markdown record; no JSON record yet) |
| Status | Accepted 2026-11-01 (this Markdown text; the JSON evidence record, where one exists, stays Proposed until `frd_tool.py promote`) |
| Date | 2026-11-01 |
| Sources | functional-design 3, 5.1, 8; technical-design 1 (invariants), 3, 4.1 to 4.7, 8.3; companion spec 9.7 (new tables) |
| Governing ADRs | ADR-0008, ADR-0009 |
| Register sections | S1 to S11, I8, X4, X5 |
| Related records | FRD-0005 (hop tables and journal), FRD-0006 (index tables, `forget`), FRD-0010 (event feed), FRD-0012 (companion record and ledger) |

## 1. Purpose and scope

Every Shinatama process (windows, widget, command line, `shinatama mcp`, `shinatama hook`)
coordinates through one SQLite database per user. This record states the process model that
needs it, where the database lives, what it holds and which tables are authoritative, the
constraints that enforce the engine's invariants, how processes wake, what is sensitive and how
the user removes it. The JSON set spread these rules over FRD-0005, FRD-0006 and FRD-0010.

## 2. Requirements

### 2.1 Process model

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Shinatama is one owning runtime process per native application window; each owns the PTYs of the instances it hosts, and only that process writes into them; browser operations are owned the same way. | U 2026-10-28 (confirmed 2026-11-01) | X | P14 | 0008 |
| R02 | There is no resident service and no daemon: windows, the widget, the command line, each agent's MCP server and hooks are separate processes that open the one database directly and coordinate through it; no process brokers between them. An optional local socket or named pipe only wakes a sleeping process sooner. | U 2026-10-28 | X | S1, S3 | 0008 |
| R03 | Any process may run an operation that does not touch a PTY it does not own (hop, send message, summarize); operations that need a foreign PTY are queued in the database and executed by the owning process. | S | X | — | 0008 |
| R04 | No Shinatama process, and no agent background process Shinatama started (OpenCode's service, Hermes Agent's gateway), keeps executing work after the owning window closes. | S (anti-goal) | X | P14, HM2 | 0008 |

### 2.2 Location, pragmas and lifecycle of the database

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R05 | One SQLite file per user in the platform data directory (`%LOCALAPPDATA%\shinatama\shinatama.db`, `~/.local/share/shinatama/shinatama.db`, `~/Library/Application Support/shinatama/shinatama.db`), overridable with `SHINATAMA_HOME`. The storage choice is the user's; exact tables and columns are the builder's. | U (SQLite per user, confirmed in principle) | S1 | S1 | 0009 |
| R06 | Never inside a project folder or on a synchronized or network volume (Google Drive, OneDrive, SMB, NFS, Dropbox): at open Shinatama detects such a location and refuses with an explanation, without false alarms on local disks. | S | S1 | S2, E4 | 0009 |
| R07 | Bundled SQLite with FTS5; `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`; several writers and readers across processes on Windows, macOS and Linux without corruption. | S | S1 | S1 | 0009 |
| R08 | Migrations: `PRAGMA user_version` with an embedded, ordered, idempotent list; a backup copy before each migration; a database newer than the binary opens read-only with a message. | S | S1 | S5 | 0009 |
| R09 | File mode 0600; on Windows an ACL for the current user only. | S | S1 | S11 | 0009 |
| R10 | Write rule: short `BEGIN IMMEDIATE` transactions; no network or agent call inside a transaction. | S | S1 | S4 | 0009 |
| R11 | `shinatama doctor` runs `PRAGMA integrity_check` and can rebuild the operational index tables from the agents' own stores; it never regenerates authoritative tables. | S | S1 | — | 0009 |
| R12 | Crash safety: a kill mid-transaction, including in the middle of a hop's state change, leaves `integrity_check` clean and the hop state recoverable. | S | S2 | S4, AT-1 | 0009 |

### 2.3 Tables and their authority

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R13 | Authoritative tables (lost = lost work history): `project`, `store_root`, `thread`, `segment`, `activation`, `hop`, `hop_event`, `snapshot`, `message`, `message_event`, `roundtable`, `roundtable_participant`, `setting`; plus, for the Daodan support and the companion, `thread_task` (derived, droppable), the companion record and `learning_memory`. | S; P (added tables) | S1 | — | 0009 |
| R14 | Operational tables, rebuildable or discardable: `instance`, `event`, `summary_log`, `session_index`, `session_text` (with its FTS5 virtual table). | S | S1 | — | 0009 |
| R15 | Time is integer Unix milliseconds UTC; ids are a prefix plus 12 random base32 characters (`t_`, `s_`, `h_`, `i_`, `r_`, `m_`, `bs_`). | S | S1 | — | 0009 |
| R16 | Every native session reference is scoped by `store_root_id`; agent plus native id alone is not an identity. A root's canonical location, origin and configuration revision are recorded once; changing or reconnecting a root creates a new revision rather than reassigning sessions; migrations preserve the old mapping; removing a root marks its records unavailable instead of deleting thread history; additional system roots are `read_only`. | S | S1 | SS5 (SEC) | 0009, 0023 |
| R17 | `thread.kind` is `work`, `roundtable` or `companion`; a companion thread is linked to its worker by its own column, not by `parent_thread_id`, which is the hop lineage. | S; P (companion) | S1 | — | 0009, 0002 v2 |
| R18 | Layout is a versioned JSON workspace record keyed by project and workspace, with a revision and one owning window (FRD-0001 R28). | A | M0 | TL3 | 0005 |

### 2.4 Constraints that enforce the invariants

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R19 | One live segment per thread (I1): a partial unique index `UNIQUE(thread_id) WHERE state IN (non-terminal hop states)` on `hop`, a unique open activation per segment, and a check in the same transaction that no other segment of the thread has an open activation. Exactly one of two concurrent hops wins. | S | S2 | S10, AT-4 | 0009, 0018 |
| R20 | Duplicate messages: `message.dedupe_key UNIQUE`. Instance exclusivity: a native session is never attached to two live instances (unique `(agent, segment_id) WHERE ended_at IS NULL`). | S | V1 | AT-7, CL15 | 0009 |
| R21 | Every hop step is journaled before its effect and idempotent after it (I3); what the destination receives is stored verbatim after redaction (I4); source records are never modified (I2); native stores are read-only in v1 (I7). | S | S2 | AT-1 | 0018, 0016 |

### 2.5 Liveness and waking

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R22 | Every owning process updates `instance.last_seen` every 5 s; an instance is dead only when `last_seen` is older than 20 s and its `owner_pid` with `owner_start_time` is gone (`GetProcessTimes`, `/proc/<pid>/stat`, `proc_pidinfo`); a reaper, run by any process at start and every 30 s, marks dead instances ended with `end_reason = crashed`. Laptop sleep and resume cause no false "dead". | S | V1 | S8 | 0008 |
| R23 | A process polls `PRAGMA data_version` every 200 ms and reads the `event` table after its last `seq`; the interval is chosen from measured latency and CPU; a one-byte "look now" ping over a named pipe or Unix socket is an optional later optimization. | A | V1 | S3 | 0008 |
| R24 | The `event`, `summary_log` and `message_event` tables get a pruning policy before release. | S (open Q8) | V1 | I8 | 0009 |

### 2.6 Path identity

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R25 | Every path is canonicalized before storing or comparing: strip `\\?\`, resolve junctions and symlinks, normalize drive-letter case, handle 8.3 names and Google Drive paths, compare case-insensitively on Windows and default macOS volumes; the same folder always maps to the same project id across Claude's flattened names, Codex's extended-length prefixes and the `--cwd--` folders of Pi and DeepSeek Harness. Claude's folder names are not invertible, so the `cwd` field inside each record is used. | S | S1 | S9 | 0009 |

### 2.7 Privacy of the database

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R26 | The database holds sensitive text: snapshots, message bodies, one-line summaries, the text index of the agents' sessions, the companion record and the ledger. No credential is ever stored in it, in snapshots, messages, summaries, launch manifests or logs (I5). | S | X | AT-6 | 0009, 0031 |
| R27 | `shinatama forget <thread>` deletes a thread's snapshots, messages, summaries and index rows, and its companion record; the ledger has its own forget-all. A thread can be marked `summaries_excluded`; indexing of a project can be switched off. | S; P (companion) | S1 | I9 | 0009 |
| R28 | No data leaves the machine except: the title calls of FRD-0010 (after consent, redacted, only when a provider backend is chosen), the companion's calls to its provider and its search words (FRD-0012), the router's redacted digest when the user opts into a hosted decision model (FRD-0015), and the agents' own traffic through the sidecar. Everything else, scoring and digests included, stays local. | S; U 2026-11-01 (routing opt-in) | X | X4 (SEC) | 0009, 0037, 0044 |
| R29 | A hosted agent with a shell runs as the same user and can open the database; this record states what Shinatama does with the data, not isolation from the agents it hosts. The browser broker makes the same statement (FRD-0011 R32). | S | X | X5 (SEC) | 0009 |
| R30 | Raw session content shown or copied in the UI is never logged or persisted (FRD-0006 R22); handoff files are 0600 or a current-user ACL and never tracked (FRD-0005 R24); browser screenshots and extracted pages are stored privately with bounded retention (FRD-0011 R30). | U 2026-10-30; A; P | S1 | SS6, H4, B8 | 0023, 0019, 0036 |
| R31 | Boxes are created owner-only because they hold logins and sessions (FRD-0002 R16); the sidecar's private root and manifest hold no credentials (FRD-0009). | A; U | X | D15, SC4 | 0012, 0029 |
| R32 | A threat model reviewed by a second reader covers untrusted agent output into the emulator, the database, MCP and hooks, handoff files, local sockets, the widget's quick answers and the browser broker; `SEC` rows get a second reader and state the negative result too. | S | V1 | X5 (SEC) | 0010 |

## 3. Open questions

1. Pruning policy for the event tables (register Q8).
2. Whether a feed-only local companion may serve a `summaries_excluded` thread (companion spec open point 3), which would amend R28.

## 4. Acceptance

S1 to S11 on the three systems; AT-1, AT-4, AT-6; X4 and X5.

## 5. Relation to the JSON records

This area has no JSON record of its own. Its sentences were extracted into FRD-0005 (journal,
I1 to I7), FRD-0006 (index, `forget`, privacy) and FRD-0010 (event feed, liveness). A JSON
record should be produced with the tool when this Markdown record is reviewed.
