# Interactive memory verification — 2026-11-12

Scope: Windows interactive terminal memory policy, startup admission, widget
warnings, and migration of an empty legacy resource ledger.

## Passing checks

- Final focused rerun after startup-support and ledger-migration integration:
  `cargo test --locked -p shinatama-desktop memory -j1 -- --test-threads=1`,
  **13/13 passed** (runtime/startup and widget lifecycle/ranking).
- `npm test`: 19/19, including advisory vs enforced memory display and malformed
  telemetry rejection.
- `npm run build`: TypeScript and production frontend build succeeded.
- `cargo build --locked -p shinatama-desktop -p shinatama-terminal --bins --features custom-protocol -j1`:
  succeeded; updated the local debug desktop and terminal helper used by
  `Shinatama.cmd`. No running app was restarted and the portable package was
  not replaced.
- Desktop suite: 40 passed, including all seven startup/runtime memory checks
  and six widget memory tests. Six unrelated filesystem-backed fixtures failed
  at their existing ACL setup; one sound audition is intentionally ignored.
- Terminal library: 14/14 unit tests passed.
- Engine resource unit tests: 44 passed; one filesystem-backed admission fixture
  failed at the same ACL setup.
- `cargo test --locked -p shinatama-engine --test resource_faults -j1 -- --test-threads=1`:
  3/3 passed. A real child allocated 32 MiB above advisory thresholds of 16 MiB
  per job / 8 MiB per process and continued answering three pings. Existing hard
  background containment tests still passed.
- `cargo test --locked -p shinatama-state --lib policy_replacement_tests -j1 -- --test-threads=1`:
  7/7 passed against actual SQLite transactions in memory. Covers complete-policy
  and revision comparisons, all outstanding reservation states, rollback,
  invalid state/schema and idempotence.

The desktop regressions first reproduced memory-triggered stop and excessive
startup headroom requirements, then passed with the replacement policy. Native
Job Object tests read back the actual absence of Windows memory-limit flags;
CPU/process-count/cleanup settings remain present.

## Environment limitations

These fixtures failed before their tested operation, when `Database::open`
rejected the sandbox ACL on `schema.lock`:

- `agents::thread_lifecycle::tests::claude_keeps_its_sealed_uuid_and_requires_the_same_durable_cleanup_barrier`
- `agents::thread_lifecycle::tests::codex_native_id_is_observed_after_start_and_session_end_cannot_finalize_ownership`
- `agents::thread_lifecycle::tests::codex_new_session_rejects_resume_source_and_keeps_pending_owner_until_cleanup_receipt`
- `agents::thread_lifecycle::tests::codex_resume_rejects_wrong_source_or_uuid_and_retains_the_sealed_owner`
- `agents::thread_lifecycle::tests::codex_resume_seals_the_saved_uuid_and_confirms_with_source_resume`
- `new_project::tests::roots_changed_during_preparation_refuse_the_publication_permit`
- `real_contained_terminal_faults_keep_a_fresh_sibling_responsive_and_saved_work_intact`
- `resources::admission::tests::native_empty_job_reservation_is_held_until_all_queue_credits_return`

Changing the temporary directory did not resolve the admission fixture failure.
No production permission check was weakened. The complete desktop/terminal test
command therefore did not pass, despite the focused memory regressions passing.

The headless Edge widget layout check could not launch in this sandbox
(`0x80070005`, access denied). No visual or end-to-end desktop pass is claimed.
No user terminal was closed or restarted during this work.
