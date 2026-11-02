# C03 — verified backups and recoverable migrations

Date: 2026-11-02. Windows 11 x64 build 26200. The current schema is version 2.

Embedded ordered migrations create the minimal v1 schema, then add v2 validation
triggers for JSON settings. No authoritative table is dropped or reconstructed.
The complete expected schema is checked for each supported version, preserving
SQL literals exactly. Future versions continue to open read-only, without running
migrations or recreating data.

Before **each** migration, an owner-only snapshot is made using the
[SQLite Online Backup API](https://www.sqlite.org/backup.html), including committed
WAL data. Its integrity, schema and version are verified and its file is flushed
before a Windows write-through rename publishes the completed copy. Partial copies
remain explicitly named `.incomplete.db`; no automatic rollback replaces live data.

The source's `data_version` is sampled before backup and checked again after
`BEGIN IMMEDIATE`. Any intervening commit defers migration and preserves the new
work. DDL and `user_version` commit together. Another active writer can therefore
require closing other app versions and reopening before an update succeeds.

Backup admission is fixed: **8 completed/incomplete copies, 2 GiB cumulative,
at least 1 GiB free**, directory scan capped at 256 entries. Byte/free-space
headroom is rechecked during incremental copying. Repeated failures cannot
continually add copies after the cap. Existing data and backups are retained;
archiving/recovery requires an explicit action, followed by reopening the app.
The 30-second cooperative deadline covers the copy-step loop, not OS I/O,
integrity/schema verification or flush. Those operations remain outside the UI
thread and are not claimed to have a hard timeout.

Checks passed on the supported Windows host:

```powershell
cargo test --offline -p shinatama-state --lib --test database --test location --test migrations --example check_location -j1 -- --test-threads=1
cargo clippy --offline -p shinatama-state --lib --test database --test location --test migrations --example check_location -j1 -- -D warnings
verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/state-smoke.mjs
```

**43 state tests passed:** 9 unit, 14 database, 13 location and 7 migration tests.
They cover a killed child during uncommitted DDL and successful recovery, a commit
between backup and migration, WAL snapshot contents, finite locked-source behavior,
malformed old settings, future/corrupt/unknown schemas, and eight failed attempts
followed by a ninth refusal without a new file or changed data.

The real embedded-app test passed all three launches with schema 2: legacy import,
unchanged source JSON, stable project IDs/selection after restart, ignored obsolete
corrupt JSON and unchanged read-only schema-999 database after closing. The core
SQLite stress and existing project/terminal checks remain separate evidence.

The new C03M maintenance requirements cover idle scheduling, operational cleanup,
bounded anomaly metadata and wider database/WAL growth control. C03's backup
budget is implemented; C03M runtime jobs and full RES/S gates are not claimed here.
