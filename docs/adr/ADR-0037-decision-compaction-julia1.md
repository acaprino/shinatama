---
adr_id: ADR-0037
version: 1
title: Decision compaction over summary compaction, scored by a local offline executor (julia1)
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/specs/token-savings.md
  modules:
  domains:
    - token-savings
    - compaction
    - local-model
    - julia1
governs_symbols:
depends_on:
  - ADR-0038
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

# ADR-0037: Decision compaction over summary compaction, scored by a local offline executor (julia1)

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Every turn resends the whole context, so a large tool result is paid again on every later turn.
The harness default, monolithic summary compaction, "loses decisions, constraints and the
'why'". The question is how history is compacted and what does the bookkeeping. Sources, kept
apart:

- Decided by the user: decision compaction, "by pointing at `tamaratran/fast-jev-compaction`"
  (token-savings goal 2, 8.2; no separate date); bookkeeping on a small local CPU model,
  offline, with no credentials (2026-10-31, token-savings 1 and goal 3); the model itself,
  its training, threshold and benchmarks out of scope (2026-10-31, token-savings 8.4).
- Author's call, not confirmed: the System One wire format for julia1 (6.1); what Shinatama
  guarantees the executor and the rules-first pipeline (6.2, 6.3).
- Proposed shape: one julia1 sidecar per box (token-savings 7).

The mechanism was read from fast-jev-compaction at commit `e3f262a`, not run. The full design
is in `docs/specs/token-savings.md`, a draft; this record holds only the decision. The adapter
that applies it on each harness is ADR-0038.

## Decision Drivers

- No decision, constraint, path, exact error or command lost after a compaction.
- Safe with a weak scorer: the only possible error is removing a result still needed, and the
  assistant then re-runs the tool.
- No credential and no transcript leaves the machine for bookkeeping.
- The frontier model never waits for the bookkeeping.

## Decision

Decision compaction replaces summary compaction. Each tool call is paired with its result;
calls in the first message and in the newest `preserveRecentMessages` messages (6) are
pinned. For each non-pinned call the scorer answers two `noul` questions: does the call still
matter, and must the result stay verbatim. Calls and results are kept verbatim, truncated or removed against `keepThreshold`;
user and assistant text is never rewritten, and no result is left without its call. Summary
compaction stays only as the fallback when decision compaction cannot remove enough, with a
structured prompt, firing at task boundaries rather than at a percentage. Scoring, ranking and
digests run on julia1, a small model on the local CPU, offline, reached through one local HTTP
endpoint that speaks the System One wire format (`noul`, `choice`, `score`), so the
fast-jev-compaction plugin needs no change beyond `baseUrl`. Shinatama keeps the job small:
append-only incremental state scored after each turn on `turn.complete`, `maxStateTokens` and
`keepThreshold` as settings (the threshold value comes with the model's delivery), deterministic
features precomputed, rules first where conclusive, and each decision recording its stage.
Compaction then only applies decisions already made. julia1 runs in one sidecar per box,
supervised like the BillionContext sidecar, sharing the llama.cpp runtime and the opt-in model
download with the title backend. See token-savings 4.3, 5, 6 and 7.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a non-pinned call whose score is at or above `keepThreshold` (0.5) -> Expected: call and result are kept verbatim (token-savings 5.1).
- **EX-002** (boundary case): Input: a non-pinned call scored below the threshold -> Expected: the call is kept and its result truncated to `truncateHeadChars` (300) plus a note, or both are removed; no result is left without its call (token-savings 5.1).
- **EX-003** (boundary case): Input: a Read of a file edited afterwards, or a call repeated later -> Expected: decided by the rules with no model call; the decision records which stage produced it (token-savings 6.3).
- **EX-004** (rejection or error case): Input: the reduction is under `minReductionRatio` (0.25), or the scorer fails -> Expected: the built-in summary compaction runs instead (token-savings 5.1).

## Rejected Alternatives

- **RA-001**: monolithic summary compaction as the primary method; one lossy summary loses decisions, constraints and the "why" (token-savings 4.3).
- **RA-002**: a small local model as a summarizer; acceptable as a scorer only because a scorer's error is a re-run tool (token-savings 5.2).
- **RA-003**: "last N turns only"; cheapest and most dangerous, not planned on its own (token-savings 4.3).
- **RA-004**: a hosted scorer or service, such as the plugin's default `api.typesafe.ai` endpoint; everything runs on the user's machine (token-savings non-goals, 5.3).
- **RA-005**: forking or vendoring fast-jev-compaction; one line and one plugin option separate it from a local scorer (token-savings 5.3, 8.1).

## Enforceable Invariants

- **INV-001**: decision compaction never rewrites or removes user or assistant text; only tool calls and results are candidates.
- **INV-002**: no tool result is ever left without its call; calls in the first message and in the newest `preserveRecentMessages` messages are never scored or removed.
- **INV-003**: no credential and no transcript leaves the machine for scoring, ranking or digests.
- **INV-004**: no request to julia1 is awaited by the frontier model's turn.
- **INV-005**: every keep or drop decision records whether a rule or the executor produced it.

## Consequences

- The plugin must pass `baseUrl` (today `hooks/fast-jev.ts` does not) to reach the local endpoint.
- On Claude Code the hook path needs function hooks, an early-access feature (2.1.274+, `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1`), per token-savings 5.1; otherwise the base-URL route (ADR-0038).
- julia1's model, threshold and benchmarks come from another project; this spec fixes only the contract.
- The julia1 sidecar shares its runtime with the local title backend of technical-design 8.4 (see ADR-0028); the title backend default is open (register O2).
- The tool chooser (same `ask` contract) and per-tool digests also run on julia1 and come at a later milestone (ADR-0038).
- Goal 2 says calls and results are "kept verbatim or removed", while the library also truncates a result to 300 characters plus a note (token-savings 5.1); the spec does not say whether Shinatama keeps that third outcome.

## Confirmation

- token-savings 9.2: the ledger on at least ten real sessions per harness; the tool re-run rate after compaction before and after each milestone; the latency of one incremental scoring pass while four agents run; that `session.compact` receives the main and sub-agent transcripts.
- No register row is assigned to these checks yet. Nothing has been run.
