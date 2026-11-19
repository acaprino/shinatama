# C29 hop recovery, catch-up, hop back and standby (2026-11-09)

Windows 11 x64. Proofs run with `TEMP`/`TMP` at `C:\shinatama-tmp`.

## What exists

- `crates/hop/src/recovery.rs` (C29.1): `reconcile()` runs the journal's R42 recovery and
  then stops only a journaled, alive destination, reopens the source activation for a close
  hop that died while SWITCHING, and offers a retry on FAILED. The desktop runs it once when
  the hop journal opens. An uncertain spawn is observed, never relaunched.
- `crates/hop/src/catchup.rs` + `Coordinator::catch_up_and_switch` (C29.2): native cursor
  (R47), only-the-missed-turns catch-up with its own nonce, at most two rounds, busy source
  Wait / Interrupt / Cancel (R39, R50).
- `crates/hop/src/hop_back.rs` (C29.3): native resume of the earlier segment with catch-up,
  seed fallback with the reason when the native session is gone, no second activation,
  ten-minute UNDO for close hops and ROLLED_BACK marking (R41, R45-R46).
- `crates/hop/src/standby.rs` (C29.4): opt-in pool, off by default, one per project and agent,
  never handed out after any input, expiry after 30 idle minutes, a version change or turning
  it off; latency medians against the R06 targets (8 s warm, 25 s cold).

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-hop` | 36 passed: 13-point kill matrix (state, kills, reopens, integrity, second pass inert), uncertain spawn, catch-up rounds and limit, busy-source decisions, rewritten history, missing catch-up receipt, hop-back strategy and canary, UNDO window, standby rules, latency summary |
| `cargo test -p shinatama-state --lib hops` | 5 passed, incl. a kill inside an open transaction leaving the previous state |

## Limits

- The desktop starts hops only from the session library (fork from history). Catch-up,
  hop back, UNDO and standby are engine logic with fake-adapter proofs; no live-pane wiring
  exists yet because live sources are not hop sources in the app.
- No warm/cold latency was measured on real agents: the summary makes no claim without samples.
- The kill matrix kills between and inside transactions in-process; no OS-level process kill
  of the desktop during a real hop was run.
