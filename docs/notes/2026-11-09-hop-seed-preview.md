# C27 hop seed and preview (2026-11-09)

Windows 11 x64, HEAD `0985476`. Proofs run with `TEMP`/`TMP` at `C:\shinatama-tmp`.

## What exists

- `crates/hop`: normalized seed history (turns, tool digests, nested seeds and injected
  context dropped, system reminders stripped), seed redaction rules with `[redacted:<kind>]`
  and a high-entropy rule, the FRD-0005 R22 budget, R23 shedding with `[N turns omitted]`,
  R19 refusal with numbers, R20 header, R28 escaping of hostile line starts, `full.md`,
  private hand-off files (R24) and per-agent delivery channels (R31): `e14871e`.
- Desktop **Hop…** in session detail: destination choice, exact seed, numbers, redaction
  counts, carried and lost lists, SHA-256, and **Copy hand-off prompt** (`0985476`).
- The frozen preview is kept in memory for the coordinator (C28), which must send it byte
  for byte (`preview::verify_send`).

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-hop` | 16 passed (budget formula, shedding within budget, refusals, hostile starts, full file, Git common dir for linked worktrees, files written once, preview tamper detection) |
| `cargo test -p shinatama-export` | 22 passed (seed corpus: recall on tokens and high-entropy strings; hex digests, UUIDs, identifiers and paths untouched) |
| `node apps/desktop/tests/hop-preview-smoke.mjs` | PASS on the real app: synthetic Claude session in a Git repo, preview to Codex, token redacted, thinking absent, no hostile line start, bootstrap <= 2 KB, `seed.md` bytes equal the preview SHA-256, owner-only ACL (`icacls`), working tree and native source unchanged; same-agent refused |

## Limits

- The destination window comes from no qualified provider catalog yet, so the unknown-window
  cap (24,000 tokens) always applies (R22).
- The sidecar bypass marker (R33, SC9) is not implemented or proven here.
- Sending is C28: until then the user copies the hand-off prompt into the destination.
- Attachments appear as path, type, size and hash; their content is not carried.
