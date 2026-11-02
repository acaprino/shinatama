# C05 — protected project creation

Date: 2026-11-02. **Local Windows qualification passed:** 146 Rust tests,
Clippy with warnings denied, frontend build and the real embedded-app proof
including optional Git initialization. Host: Windows 11 x64 build 26200,
Node 24.14.1, WebView2 156.0.4314.58. The durable machine-readable record is
[C05-project-creation.result.json](2026-11-02-C05-project-creation.result.json).

## Creation and recovery

The picker exposes **New project** and `n` outside input/filter/xterm. It keeps
unsaved root drafts and refuses replacement during an active folder mutation.
The dialog uses the explicitly configured default, allows another configured
root, and defaults Git to off. A completed create updates selection and its
revision without starting a terminal. Windows names are validated without
trimming: reserved devices, forbidden characters, trailing spaces/dots, case
aliases and overlong components are rejected.

The command reserves the shared folder-operation slot through `catalog_begin`.
Current database schema/write admission and the requested root ID/revision/path
are checked before filesystem preparation, after optional Git discovery and
again before publication. The admission check reads the current schema rather
than trusting access cached when the app opened. No SQLite transaction or
foreground service lock spans native filesystem/Git work.

Creation pins the chosen root and its ancestors with no-delete-sharing handles.
Windows volume/file identities protect private-data separation even when path
spellings differ; incomplete identities and reparse points are refused. The
staging directory is created relative to the pinned root using `NtCreateFile`
with `FILE_CREATE`: it cannot reopen or replace an existing entry, and the
returned handle pins the new identity in the same call. Project permissions
inherit the chosen root's access model.

A persistent, verified empty `.shinatama-create.lock` serializes creators in
that root, with a cooperative three-second lock wait. Each scan examines at
most **4,000 root entries**. At **eight retained staging entries**, another
preparation is refused. These are count/admission bounds, not a cumulative
staging-byte quota. Catalog scans exclude preparation names before metadata
lookup, so an unfinished preparation is not listed as a project.

Optional Git runs inside `.shinatama-create-<uuid>.tmp`. Immediately before the
final rename, a commit permit competes atomically with Cancel. Publication is
one `FileRenameInfo` operation with replacement disabled. A destination created
by another process is preserved. Cancellation before this boundary cannot
publish the project; afterward the UI offers **Close; creation continues**
instead of promising an undo.

Failure cleanup can remove only the same verified, still-empty staging
directory, by handle. Nonempty, changed or unconfirmed preparations remain and
produce a typed `recoveryPath`; no recursive deletion removes partial Git work
or a concurrent user's file. A crash likewise preserves staging and releases
the crash-owned root lock. The final filesystem rename and SQLite selection
are separate commits: if saving metadata fails after publication, the complete
project remains and the error explicitly supplies its path. No automatic retry
or rollback replaces existing work.

## Fixed Git helper containment

Git is opt-in and native: no shell or wrapper command is used. The child is
created suspended with `PROC_THREAD_ATTRIBUTE_JOB_LIST` and an explicit inherited
handle list. Membership is verified before resume. Failed job-list setup,
membership checks or resume have no uncontained fallback. Kill-on-job-close
also covers the interval before resume if the host exits.

| Resource | Fixed admission limit |
|---|---|
| Processes and memory | 2 active processes; 128 MiB per process; 256 MiB per job |
| Execution | 5 seconds job user-mode CPU time; cooperative 5-second supervision loop |
| Output | 32 KiB total; drained through a bounded pipe without retaining raw output |
| Teardown | One cleanup attempt, root-process signal and zero active job processes; cooperative 250 ms confirmation budget |

The helper has `Free`, `Active` and `Retired` admission states. If shutdown
cannot be confirmed, its slot becomes Retired for that app run; repeated
requests cannot add another uncertain child. Drop does not repeat the failed
cleanup attempt. Only the responsible private job is stopped.

Arguments explicitly request branch `main`, `--shared=false`, no hooks,
disabled fsmonitor and an empty `--template=`. The child receives a minimal
environment, synthetic project-local home/temp paths, disabled prompts/network
protocols and `GIT_CONFIG_GLOBAL=NUL`, `GIT_CONFIG_SYSTEM=NUL`,
`GIT_CONFIG_NOSYSTEM=1`. Host global/system Git configuration and templates are
not read; credentials and raw subprocess output are not collected. These are
fixed helper protections, **not C09R adaptive resource calibration**. Native
filesystem admission and OS calls have no claimed hard-preemption deadline.

## Qualification

The actual Rust qualification passed **146 tests**, including **39 projects**
and **12 desktop** tests. Clippy subsequently passed with warnings denied.

```powershell
cargo test --offline --workspace --all-targets --features custom-protocol -j1 -- --test-threads=1
cargo clippy --offline --workspace --all-targets --features custom-protocol -j1 -- -D warnings
```

The Rust log is `.verification/c05-rust-tests.log`, SHA-256
`0F258128EFA63AEFCD158EC15BC814F5A3E6DE470038B8503A7030B2C2E8AFD1`.
Broader checks also passed: 29 Python tests, 3 frontend tests, formatting,
dependency inventory and the latest normal frontend/desktop builds.

Native and synthetic tests cover name/path corpora, private-data protection,
parallel creators, a destination arriving at commit, Git failure/cancellation,
crashed preparation, eight retained failures followed by refusal, and existing
lock contents/hard links remaining unchanged. Git helper tests cover hangs,
output overflow, cancellation, refused membership/resume, invalid creation-time
job attributes and retirement after deliberately unconfirmed teardown. Stopping
one private job leaves an independent healthy process running.

The real connected UI run used:

```powershell
verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/new-project-smoke.mjs --with-git
```

It passed **nine checks in 9.5 seconds**, wrapper exit 0 and
`wrapper in global job: true`. The checks cover these groups:

1. `n` in chrome/picker, preserved filter/text/xterm input, visible button and
   retained unsaved root drafts.
2. Explicit default, Cancel, Windows invalid names, and a Unicode/spaced project
   created without Git or an automatic terminal.
3. A case-insensitive duplicate preserves an existing marker; stale root
   revision rejects creation; restart restores the selected stable project ID.
4. A second project uses real Git: `.git/HEAD` is exactly
   `ref: refs/heads/main\n`, without hooks or template description, while the
   original terminal session remains unchanged.
5. While the app stays open, only its synthetic database is changed to schema
   999. Create rejects the request into a fresh configured root before any
   directory or lock is created; every `project` row remains unchanged.

The driver refuses an occupied CDP port and verifies its unique synthetic
storage identity before UI changes. For `--with-git`, only the test app's PATH
is replaced, after removing case-insensitive duplicate keys, so the bridge
deterministically selects the recorded local Git executable. No global
environment is changed. All owned app/helper processes were closed afterward.
Without the flag the driver omits real Git initialization; that mode is not
claimed as a separate completed run here.

## Evidence and limits

The successful GUI run completed before **2026-11-02 20:16:30 UTC**. SHA-256:

| Artifact | SHA-256 |
|---|---|
| `target/debug/shinatama-desktop.exe` | `29E47BE1A2CD569DD5B7E6864CED8F507EA13E415BBDAB5483B9516F5D909EB2` |
| `apps/desktop/tests/new-project-smoke.mjs` | `D73CFC1E84BEFC8336A8C7DD2F0F8C035F2A81C36A13328837DCDD4A3BEA8258` |
| `.prototype-evidence/new-project.json` | `EA7E8CD4BDD972EB35B664EB3CB53DC69171E55294D656B66A8C279BB9D04CC0` |
| `C:\Program Files\Git\cmd\git.exe` | `7B7971DD13F0C3A284E538601F2F9770B3A87DFACCB5FB52D68141C67ED22364` |

Local NTFS behavior is exercised. Different drive/UNC spellings of identical
protected directories are tested with **simulated identity records**; no real
SMB share or UNC alias was accessed. Real UNC/shared/synced drives and
Google Drive/DriveFS latency remain unqualified. Git executable/staging
admission currently requires a verified local fixed drive and rejects reparse
paths. The full U3 keyboard/scale matrix and C09R whole-app adaptive budgets,
resource-pressure and subprocess-fault gates remain separate. No model request
or Daodan integration was made by this proof.
