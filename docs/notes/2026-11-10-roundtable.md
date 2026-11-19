# C33 headless roundtable coordinator (2026-11-10)

`crates/roundtable` at `bdec7b9`. `cargo test -p shinatama-roundtable`: 6 passed (moderated
rounds + synthesis; token, cost and round caps with partial synthesis; user stop between
rounds and interjection delivered once, escaped; timed-out participant recorded; injected
"write the file" instruction reaches others only as escaped quoted data; secrets redacted;
no answers means no synthesis). A defect found by the tests was fixed: a note given before
round 1 reached nobody.

Open:
- Real `ask_headless` per agent (`claude -p`, `codex exec --ephemeral`, `opencode run`, Pi RPC)
  with deny-tools flags and the canary file proof (R32, R2-R4) is not implemented: it spends the
  user's tokens and must be qualified per agent version.
- No desktop UI, no cost warning dialog, no storage as a `roundtable` thread yet; sidecar
  routing of participant traffic (R34, SC10) not wired.
- Defaults ($2, 40k tokens) are not calibrated on three real topics (R33, R37).
