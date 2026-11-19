# F20 per-session token ledger (2026-11-10)

`crates/token-economy` + detail page ledger. The ledger reads only what the agent recorded:
Claude `message.usage` (input excludes cache reads/writes), Codex `token_count`
`last_token_usage` (input includes `cached_input_tokens`, normalized), Pi camelCase usage.

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-token-economy` | 4 passed |
| `cargo test -p shinatama-engine --features session-reader --lib ledger_tests` | 1 passed |
| `node apps/desktop/tests/hop-preview-smoke.mjs` | PASS incl. ledger from recorded usage |

Limits and deferred parts:
- No baseline measured yet: ten real sessions per harness are required before any claim (R38);
  none were run (they spend the user's tokens). The UI says no saving is claimed.
- No aggregation across sessions, no tool-definition, verbosity-cache or image-weight
  measurements linked yet (T-rows stay open).
- Daodan-related baseline parts stay deferred with Daodan (FRD-0013 R08-R12, R29).

## F21 rule-only decision planner (2026-11-10)

`crates/token-economy/src/decision.rs`: pairing, pinning, deterministic rules with recorded
stages, apply with pairs kept together and text untouched, structured-fallback signal under a
25 % reduction. 3 tests. Not connected to any agent: no endpoint, no interception of Claude's
native compaction yet.
