# C24 session search: ledger entry and recheck (2026-11-10)

C24 was implemented in `ad04532` (2026-11-08, "persistent incremental session search") and
marked done in PLAN-SEMPLIFICATO, but `progress.json` had no entry. Rechecked on 2026-11-10 at
HEAD after C43: `node apps/desktop/tests/session-search-smoke.mjs` PASS, 11 checks (empty before
indexing, all indexed, snippet and literal query, tool and reasoning text never indexed,
agent/project/date filters, unchanged sessions not reread, only changed session reindexed,
deleted source searchable and flagged, UI highlight/badge/open detail, disable and forget touch
only the index, rebuild from scratch).
