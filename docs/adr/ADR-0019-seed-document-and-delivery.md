---
adr_id: ADR-0019
version: 1
title: Budgeted seed with full transcript file and redaction, delivered as bootstrap plus file with no shell; warm standby; hop back with catch-up
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - seed-document
    - seed-delivery
    - redaction
    - warm-standby
    - hop-back
governs_symbols:
depends_on:
  - ADR-0016
  - ADR-0018
  - ADR-0021
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0019: Budgeted seed with full transcript file and redaction, delivered as bootstrap plus file with no shell; warm standby; hop back with catch-up

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. In v1 every hop to a new destination is a seed (ADR-0016). The
question this record answers is what the seed contains, how it reaches the agent, how the
destination is prepared, and how a hop back works.

Origin: author's proposed shape, not confirmed by the user. Budget, full-transcript location,
noise filtering, argv bootstrap and warm standby are "[C]"; the redaction rule list is "the
builder's"; markers and cuts are lessons from Agent Deck's source ("[S]"); per-agent channels
are research ("[R]"). Two register rules are written into the governing sections as the
author's calls: lines of the seed or bootstrap that a destination could interpret are never
executed or expanded (technical-design 6.4; register Q9, H7), and the sidecar delivers the seed,
bootstrap and receipt as sent (technical-design 9; register Q5, SC9; ADR-0029); where a bypass
marker is needed for that, the user decided on 2026-11-01 that it goes into the sidecar's single
source and Shinatama inherits it. Functional-design
5.2 marks the whole thread "my call" and the catch-up "a guess to validate". Technical-design 5.6
and 6.4 to 6.6 hold the full design; this record holds only the decision.

## Decision Drivers

- Carry the whole thread without filling the destination's window.
- What the destination receives must be exactly what the user previewed, with no credentials.
- Windows command-line limits (32,767 for `CreateProcessW`, 8,191 via `cmd.exe`) and quoting.
- Booting the destination is the slow part of a hop.

## Decision

**Seed** (technical-design 6.4): an always-kept header (thread, source agent and session, time,
cwd, branch and worktree, uncommitted and untracked counts with the first 20 paths, instruction
files as paths, attachments with sha256, and the instruction to reply `ready <nonce>` and one
line, then wait),
then `[user]`, `[assistant]` and `[tool]` lines. Budget: `seed_tokens = min(0.40 ×
destination_window, 60,000)`, estimated as bytes ÷ 3.5; the window comes from the provider's
model catalog, never from a name in the code, and an unknown window caps the seed at 24,000
tokens. Shedding order: header, first user message, last turns verbatim back to 55 % of the
budget, the middle as one-line digests, then oldest turns dropped with `[N turns omitted]`. Tool
output over 40 lines keeps its first and last 15 lines. `reasoning` is never included. The whole
rendered transcript is written to `<handoffs>/<hop-id>/full.md` and named in the seed;
`<handoffs>` is `<git common dir>/shinatama/handoffs` (never tracked, 0600 or a current-user ACL)
or the data directory. Every seed and catch-up starts with `[shinatama handoff v1]` and the nonce
and is read back as a `meta` marker. Cuts fall on turn and UTF-8 boundaries; noise records are
filtered; tool calls are kept as one-line digests; an empty projection is refused; files over
64 MiB are streamed from the end. Secret-shaped strings become `[redacted:<kind>]` before
anything is stored or sent, and the count is shown.

**Delivery** (technical-design 5.6): through the adapter's `seed_channel`. `argv_prompt` carries
only a bootstrap of at most 2 KB plus the seed file; Pi uses `file_arg`, Hermes Agent
`query_file`, DeepSeek Harness `protocol_prompt`. Agents are spawned with an argument vector and
no shell; Pi is started as `node.exe … cli.js` directly.

**Warm standby** (6.5): for agents the user names, one idle, un-seeded instance per project in
use, discarded after 30 minutes idle or on a version change; off by default (technical-design 12).

**Hop back** (6.6): resume the earlier native session and send a catch-up limited to the missed
turns, same budget and receipt; if that session is gone or cannot be resumed, fall back to
`seed_prompt` and say why.

## Verified Interview Examples

- **EX-001** (boundary case): Input: a 400-turn thread hops to a destination with a small window -> Expected: the seed is within budget, keeps header, first message and last turns, marks omissions, and the full transcript file is complete (technical-design 11.5).
- **EX-002** (rejection or error case): Input: a planted fake key and a fake bearer token in the thread -> Expected: neither appears in the snapshot, the seed, the full transcript file, a message body or a summary input (technical-design 11.6).
- **EX-003** (nominal case): Input: hop A → B, two turns in B, hop back to A -> Expected: A's native session is resumed with its old id and the catch-up names B's two turns (technical-design 11.3; register H10).
- **EX-004** (rejection or error case): Input: a seed or bootstrap with a line starting with `/`, `!`, `@path`, `#` or `-` -> Expected: nothing runs or expands in any destination; otherwise the seed is escaped or sent by a safe channel (register H7, SEC).
- **EX-005** (rejection or error case): Input: the budget is smaller than header + task + last turn -> Expected: the hop is refused with the numbers (technical-design 6.7).

## Rejected Alternatives

- **RA-001**: the last 32,000 bytes of text with no tool activity, cut mid-turn (Agent Deck); replaced by the whole thread as a budgeted seed plus the full transcript file (technical-design 6.8).
- **RA-002**: dropping tool calls; "the work done is half of the context" (technical-design 6.4).
- **RA-003**: cutting at the next newline, which can split a turn or a character; refusing big files; nesting wrappers across A → B → C (Agent Deck behaviours, technical-design 6.4, 6.8).
- **RA-004**: `system_prompt_file` as the default channel; "history is not system policy" (technical-design 5.6).
- **RA-005**: spawning through a shell (tmux and POSIX `sh` in Agent Deck; Pi's `pi.cmd` shim caps the line at 8,191) (technical-design 5.6, 6.8).
- **RA-006**: hopping back with a fresh export, as Agent Deck does; replaced by a native resume plus catch-up (technical-design 6.8).

## Enforceable Invariants

- **INV-001**: what the destination receives is exactly what the user previewed, stored verbatim after redaction (technical-design I4).
- **INV-002**: no credentials in snapshots, seeds, catch-ups or the full transcript file (technical-design I5, 11.6); redaction runs before anything is stored or sent.
- **INV-003**: no `reasoning` item is ever part of a seed or a catch-up.
- **INV-004**: the seed stays within `min(0.40 × window, 60,000)` tokens, or 24,000 when the window is unknown.
- **INV-005**: argv carries at most a 2 KB bootstrap; no agent is spawned through a shell.
- **INV-006**: cuts never fall inside a turn or a multi-byte character; an empty projection is never sent.
- **INV-007**: a hand-off message read back from a store is a `meta` marker, never a user turn.
- **INV-008**: no destination executes or expands a line of the seed or the bootstrap that starts with `/`, `!`, `@path`, `#` or `-`; where a channel would, the seed is escaped or sent by a safe channel (author's call; technical-design 6.4; register Q9, H7).

## Consequences

- The budget stays well below the sidecar's `maxTokens: 160000`, so a hop does not trigger
  compression on the first turn (technical-design 9); the seed, bootstrap and receipt reach the
  agent as sent, with a bypass marker where the prompt rewriter or CCR would alter them, added in
  the sidecar's single source and never patched inside Shinatama (ADR-0029; register Q5, SC9).
  Each hop costs one short receipt turn; a standby costs memory, not tokens.
- Whether each agent's sandbox can read the handoff file is unverified per agent; the data
  directory is the fallback (register H4).
- The defaults of technical-design 12 (40 %, 60,000 tokens, standby off) are open for the user.

## Confirmation

- Acceptance 11.3, 11.5 and 11.6 pass.
- Register H3, H4, H6, H7, H8, H10, H11, H14 and H23 pass, and technical-design 10 item 4 (2 KB
  bootstrap plus file, and a pasted 30 KB seed, including a path with spaces and parentheses).
- Nothing has been run yet.
