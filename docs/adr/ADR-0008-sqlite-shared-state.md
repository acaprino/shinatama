---
adr_id: ADR-0008
version: 1
title: One SQLite database per user on local disk; authoritative vs operational tables; store-root scoping
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - shared-state
    - sqlite
    - schema
    - store-roots
    - data-privacy
governs_symbols:
depends_on:
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

# ADR-0008: One SQLite database per user on local disk; authoritative vs operational tables; store-root scoping

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. A thread must outlive the agent and the terminal ("Closing Shinatama
does not lose a thread", functional-design 5.1), and several processes must share state without a
daemon (ADR-0007). The question this record answers is where that state lives, which parts are
history the user cannot lose, and how a native session is identified when the same agent has
several stores.

Origin: technical-design 4 is marked "[C, confirmed in principle by the user]", with no date;
functional-design 8 says "the storage choice, SQLite per user, is decided" and leaves the exact
tables and columns to the builder. Functional-design 5.1 still says the thread's "format and
location are the builder's call". The tables are a sketch (technical-design 0); the path quirks of
4.5 are observed [V]; FTS5 and WAL on three systems are unverified [U]. The full design is in
technical-design 4; this record holds only the decision.

## Decision Drivers

- Several processes, no daemon.
- "SQLite WAL needs shared memory on a local disk" [D], and some of the user's projects live on Google Drive.
- Work history the user cannot lose, apart from indexes that can be rebuilt.
- One native session id can exist in several stores: a box, the system home, a portable root.
- The database holds sensitive text.

## Decision

One SQLite file per user in the platform data directory (`%LOCALAPPDATA%\shinatama\shinatama.db`,
`~/.local/share/shinatama/shinatama.db`, `~/Library/Application Support/shinatama/shinatama.db`),
overridable with `SHINATAMA_HOME`; never inside a project folder or a synchronized or network
location, which is detected at open and refused with an explanation. Bundled SQLite with FTS5,
`journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5000`; ordered
idempotent migrations through `PRAGMA user_version` with a backup before each; a database newer
than the binary opens read-only with a message; file mode 0600 or a current-user ACL. Writes are
short `BEGIN IMMEDIATE` transactions with no network or agent call inside. Authoritative tables
("lost = lost work history"): `project`, `store_root`, `thread`, `segment`, `activation`, `hop`,
`hop_event`, `snapshot`, `message`, `message_event`, `roundtable`, `roundtable_participant`,
`setting`. Operational tables, which can be rebuilt or discarded: `instance`, `event`,
`summary_log`, `session_index`, `session_text`; `shinatama doctor` runs `integrity_check` and
rebuilds only the index tables. Every native session reference is scoped by `store_root_id`;
agent plus native id alone is not an identity; changing or reconnecting a root creates a new
revision, removing one marks its records unavailable, and additional system roots are
`read_only`. Unique indexes enforce one live segment per thread, message dedupe and instance
exclusivity. Every path is canonicalized before it is stored or compared. The user can
`shinatama forget <thread>`, mark a thread `summaries_excluded` and switch indexing off per
project. See technical-design 4.1 to 4.7.

## Verified Interview Examples

- **EX-001** (rejection or error case): Input: two processes try to hop the same thread -> Expected: exactly one wins; the loser gets a clear message (technical-design 11.4; register S10).
- **EX-002** (rejection or error case): Input: the database placed on Google Drive (`G:`, `H:`), OneDrive, SMB, NFS or Dropbox, and on local disks -> Expected: refused on every synchronized or network volume with a useful message; accepted on local disks without false alarms (register S2).
- **EX-003** (boundary case): Input: a database newer than the binary, and a forward migration -> Expected: the newer one opens read-only; the migration takes a backup first (technical-design 4.1; register S5).
- **EX-004** (boundary case): Input: `C:\Users\alfio\.claude\double-shot-latte`, stored by Claude Code as the folder `C--Users-alfio--claude-double-shot-latte`, and Codex paths with the `\\?\` prefix -> Expected: the folder name is not inverted, each record's `cwd` is used, and the same folder always maps to the same project id (technical-design 4.5; register S9).
- **EX-005** (rejection or error case): Input: duplicate native ids across two roots, hostile roots, and a root migrated or reconnected -> Expected: no collision, no silent reassignment and no unauthorized destination (register SS5).

## Rejected Alternatives

- **RA-001**: the database inside a project folder or a synchronized or network location; WAL needs shared memory on a local disk, and some projects live on Google Drive (technical-design 4.1).
- **RA-002**: agent plus native session id as a session's identity; every native session id is scoped by its store root (technical-design 2, 4.2).
- **RA-003**: recovering a project path from Claude Code's flattened folder name; the `.` and the separators both become `-`, so "the name cannot be inverted" (technical-design 4.5).

## Enforceable Invariants

- **INV-001**: technical-design I1 holds in the database: a partial `UNIQUE(thread_id)` over non-terminal hop states, a unique open activation per segment checked against the whole thread in the same transaction, and a unique live `(agent, segment_id)` instance.
- **INV-002**: the database is never opened inside a project folder or on a synchronized or network volume.
- **INV-003**: no network or agent call runs inside a transaction.
- **INV-004**: technical-design I5: no credentials in the database, snapshots, messages or summaries.
- **INV-005**: every `segment`, `session_index` and `session_text` row carries `store_root_id`; a `read_only` root is never a write destination.
- **INV-006**: `doctor` regenerates operational index tables only, never authoritative ones.
- **INV-007**: no data leaves the machine except the title calls of technical-design 8.4 and the agents' own traffic through the sidecar.

## Consequences

- The layout, hop journal, messages, events and store catalog all live here (ADR-0004, ADR-0007, ADR-0017, ADR-0022, ADR-0023).
- Technical-design 4.7 states what Shinatama does with the data, not isolation from hosted agents: an agent with a shell runs as the same user and can open the file (technical-design 7.9).
- `event`, `summary_log` and `message_event` have no pruning policy yet (register Q8, I8).
- Shared-memory SQLite with FTS5 under two processes on three systems is unproved (technical-design 10 item 1).

## Confirmation

- Register S1, S2 and S9 (G0, first-milestone set), S4 and S10 (G0), S3, S5 to S8 and S11 (G1), and SS5 pass with records.
- Acceptance 11.1, 11.4 and 11.6 pass.
- Nothing has been run.
