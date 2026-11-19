# C36 project overview, pins, git facts and worktrees (2026-11-10)

Windows 11 x64. Proofs with `TEMP`/`TMP` at `D:\shinatama-tmp`.

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-projects --lib git::tests` | 4 passed; 30 synthetic projects scanned in 181 ms (8 parallel, 3 s deadline each); missing folder = Unknown; cache hit < 50 ms; worktree plan/create/refusals |
| `cargo test -p shinatama-state --test projects pinned` | 1 passed: pins persist across reopen, newest first, unknown id refused |
| `node apps/desktop/tests/projects-overview-smoke.mjs` | PASS: git facts, pinned first, plan creates nothing, mismatched confirmation refused, confirmed worktree on `shinatama/task-1`, duplicate refused, UI lists and unpins |

Limits: no Git diff UI (by design). "Resume last" reuses the qualified thread resume path; it
is not separately smoke-tested here. Worktree removal is left to the user's Git.
