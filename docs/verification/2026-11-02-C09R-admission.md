# C09R.2 — bounded admission and native workload jobs

Windows local checkpoint passed: 102 targeted engine/state/native checks, desktop
build, one real-app observer proof and the existing CLI regression checks.

Jobs have memory/process/CPU limits before a child is created. A pinned startup
binding and suspended-child identity/membership check prevent an uncontained
fallback. One bounded persisted ledger serializes reservations across windows:
two real competing processes admit exactly one budget. Queue credits remain charged
until returned; cleanup releases the ledger last. Explicit retry reuses the same
retained job instead of opening another slot.

The native crash fixture found that a disappeared job name can precede child
termination. That inference is removed: release requires complete lifetime
coverage, signaled process handles, closed admission/job and returned queue credits.
Unknown descendants, missed short-lived children and owner-crash uncertainty retain
the budget as `Retired`. Automatic recovery of that uncertainty is not qualified.
The healthy sibling answers again after the owner crash. Synthetic console-free
helpers use DETACHED_PROCESS; actual console helpers must be counted in real budgets.

The monitor timestamps now share one epoch with its status/caller observations.
No native agent start is enabled by this checkpoint. Adaptive policy and full
fault/terminal integration remain open under C09R.3–.4 and dependent steps.

Essential reproducible checks: engine/state `--lib` tests, engine `job_containment`
and state `resource_reservations` integration tests; desktop build and
`apps/desktop/tests/resources-smoke.mjs`. Detailed local outputs: `.verification/c09r-*`.

Reference: [Windows process creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags).
