# F03H hop transactions, uniqueness and crash recovery (2026-11-10)

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-hop --test cross_process` | 3 consecutive passes: 6 real processes race one journal, exactly 1 wins; 2 concurrent recovery processes apply the R42 abort exactly once; integrity ok |
| `cargo test -p shinatama-state --lib hops` | 5 passed: 8 connections race, one event per arrow, a kill inside an open transaction leaves the previous state |
| `cargo test -p shinatama-hop` (recovery) | 13-point kill matrix: expected state, kills, reopens, integrity, second pass inert |
| `node apps/desktop/tests/hop-preview-smoke.mjs` | source native session bytes unchanged after preview and failed hops |

Every external effect is journaled before it runs (`spawn_attempted`, `seed_delivery`,
`source_activation_end`, `source_close`), verified in the coordinator tests.

Limit: no OS-level kill of the desktop process during a real hop to a signed-in agent.
