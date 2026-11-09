# C28 hop coordinator and journal (2026-11-09)

Windows 11 x64, HEAD `619dcee`. Proofs run with `TEMP`/`TMP` at `C:\shinatama-tmp`.

## Decision

The hop journal has its own SQLite file, `hops.sqlite`, next to the authoritative database
(same pattern as the search index). Reason: every arrow must be one transaction with its
event (R35) and the unique-unfinished-hop rule (I1) must hold across processes; a separate
file gives both without an authoritative schema migration and its backup step. Thread ids are
the authoritative database's own.

## What exists

- `crates/state/src/hops.rs` (`d7962a5`): `hop` and `hop_event`, compare-and-set transitions,
  partial unique index per thread, `record_effect` before external effects, BUSY/LOCKED retry
  then refusal, FULL refusal (R43), `recover()` per R42.
- `crates/hop/src/coordinator.rs` (`e5beb1f`): consent before effects, snapshot re-check,
  spawn journaled, evidence + receipt within 90 s (R38), failure handling per R49, idle-source
  switch and close/fork difference (R11-R12, R39-R40), against `Destination`/`Source` traits.
- Desktop (`619dcee`): **Hop** in the preview dialog runs a hop from the session library as a
  fork from history: opens the destination through the qualified Open path, pastes the exact
  previewed bootstrap, waits for `ready <nonce>`, commits. `hop_status` shows the journal.

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-state --lib hops` | 4 passed: happy path one event per arrow; invalid/stale transitions change nothing; 8 concurrent connections on one file -> exactly 1 winner, others `ThreadBusy` with its id; recovery per state |
| `cargo test -p shinatama-hop` | 21 passed, incl. 5 fake-adapter suites: no spawn before CONFIRMED; close ends the source activation at SWITCHING and closes it last; fork never touches the source; spawn failure, seed mismatch and missing session fail with only the started instance stopped; missing receipt waits; busy source blocks the switch; draft needs confirmation; changed snapshot aborts before any spawn |
| `node apps/desktop/tests/hop-preview-smoke.mjs` | PASS, 7 checks, incl. a hop to an unavailable destination: FAILED after `spawn_attempted`, delivery refused, the failed hop frees the session, source bytes unchanged |

## Limits

- Not run: a hop into a real signed-in agent (it spends the user's tokens). Try it in the app:
  session detail -> Hop… -> Codex -> Prepare preview -> Hop.
- Paste channel only: the destination's first stored entry is not hashed (R38 technical
  evidence is the receipt alone); argv/file/protocol channels need qualified launch recipes.
- Hops start only from the session library (no live source yet), so close mode, the source
  idle check and draft confirmation are proven against fake adapters, not live panes.
- UNDO (R41), catch-up (R45, R50) and warm standby (R34) are C29.
- The READY timeout on a frozen destination does not show the pane's last 40 lines in the
  dialog; the pane itself stays open.
