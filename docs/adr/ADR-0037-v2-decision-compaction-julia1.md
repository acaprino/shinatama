---
adr_id: ADR-0037
version: 2
title: Decision compaction over summary compaction, scored by a local offline executor (julia1); compaction and digests never leave the machine, routing and the tool chooser may by opt-in
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/specs/token-savings.md
    - docs/notes/2026-11-01-model-routing-decision-model.md
  modules:
  domains:
    - token-savings
    - compaction
    - local-model
    - julia1
governs_symbols:
depends_on:
  - ADR-0044
created_from_task: frd-adr-consolidation-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: 620bcf9a66edeb57d0528b3c522f3018928b3706
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - Decision
  - INV-003
  - RA-004
  - EX-005
---

# ADR-0037: Decision compaction over summary compaction, scored by a local offline executor (julia1); compaction and digests never leave the machine, routing and the tool chooser may by opt-in

<!-- adr-header:start -->
Version 2 · Proposed 2026-11-01 by Alfio Caprino · supersedes version 1 (Accepted 2026-11-01)
<!-- adr-header:end -->

## Context and Problem Statement

Version 1 (Accepted 2026-11-01) rejects "a hosted scorer or service, such as the plugin's default
`api.typesafe.ai` endpoint" (RA-004) and says no transcript leaves the machine "for scoring, ranking
or digests" (INV-003). On the same day the user asked for a model router reached with one key of
their own, free or paid, and ADR-0044 (Accepted) records that routing alone may send a bounded,
redacted digest to a hosted decision model by the user's opt-in, and that the same selector is the
tool chooser; ADR-0044 states that it narrows version 1 "to compaction and digests". An accepted
record cannot be edited, so this version carries the narrowing. The mechanism of decision
compaction, the julia1 contract and the title rule of version 1 stand unchanged. Sources: the user's
request (2026-11-01; routing note 1, 7); the digest-only shape is the author's call (ADR-0044
INV-005). The full design is in `docs/specs/token-savings.md`, a draft, and the routing note; this
record holds only the decision.

## Decision Drivers

- No decision, constraint, path, exact error or command lost after a compaction.
- Safe with a weak scorer: the only possible error is removing a result still needed.
- No credential and no transcript leaves the machine for bookkeeping.
- The user asked for a hosted jev-like router reached with one key; a bounded digest is not a transcript.

## Decision

Unchanged from version 1: decision compaction replaces summary compaction as the primary method
(pairing, pinning of the first message and the newest `preserveRecentMessages` messages, two
`noul` questions per non-pinned call, keep, truncate or remove against `keepThreshold`, no result
without its call, text never rewritten); it fires at `compactAtPercent` on `turn.complete` or on an
intercepted native compaction, with summary compaction only as the fallback at task boundaries;
scoring, ranking and digests run on julia1, a small model on the local CPU, offline, through one
local HTTP endpoint speaking the System One wire format, so the plugin needs no change beyond
`baseUrl`; Shinatama keeps the job small (append-only incremental state, tunable `maxStateTokens`,
`keepThreshold` from the model's delivery, precomputed features, rules first, each decision
recording its stage); julia1 runs in one sidecar per box sharing the llama.cpp runtime with the
local title model, with one opt-in download when one model serves both; titles go through julia1
only when it is the chosen local model (user, 2026-11-01).

Narrowed: "nothing leaves the machine" covers **compaction scoring and digests**, which stay local
with no credential and no transcript sent anywhere. The model router and the tool chooser of
ADR-0044 may use a hosted decision model (TypeSafe Jev directly or through OpenRouter, Cloudflare
Clef-flash, or another endpoint speaking System One or the OpenRouter Decisions format) **only by
the user's opt-in**, and that executor receives only the clipped, redacted digest and the candidate
cards, never a transcript, a tool result or an agent credential. The default executor for routing
and the chooser remains julia1 locally, with no key.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a non-pinned call whose score is at or above `keepThreshold` (0.5) -> Expected: call and result are kept verbatim (token-savings 5.1).
- **EX-002** (boundary case): Input: a non-pinned call scored below the threshold -> Expected: the call is kept and its result truncated to `truncateHeadChars` (300) plus a note, or both are removed; no result is left without its call (token-savings 5.1).
- **EX-003** (boundary case): Input: a Read of a file edited afterwards, or a call repeated later -> Expected: decided by the rules with no model call; the decision records which stage produced it (token-savings 6.3).
- **EX-004** (rejection or error case): Input: the reduction is under `minReductionRatio` (0.25), or the scorer fails -> Expected: the built-in summary compaction runs instead (token-savings 5.1).
- **EX-005** (rejection or error case): Input: `router.endpoint` points at a hosted executor and the user has opted in; a compaction pass is due -> Expected: the compaction questions still go to the local julia1 endpoint; a capture of the hosted endpoint's traffic shows only routing and chooser requests, each carrying a digest and cards and no transcript text.

## Rejected Alternatives

- **RA-001**: monolithic summary compaction as the primary method; one lossy summary loses decisions, constraints and the "why" (token-savings 4.3).
- **RA-002**: a small local model as a summarizer; acceptable as a scorer only because a scorer's error is a re-run tool (token-savings 5.2).
- **RA-003**: "last N turns only"; cheapest and most dangerous, not planned on its own (token-savings 4.3).
- **RA-004**: a hosted scorer or service for compaction or digests, such as the plugin's default `api.typesafe.ai` endpoint; compaction and digests run on the user's machine. A hosted executor is admitted for routing and the chooser only, by opt-in and with a digest (ADR-0044).
- **RA-005**: forking or vendoring fast-jev-compaction; one line and one plugin option separate it from a local scorer (token-savings 5.3, 8.1).

## Enforceable Invariants

- **INV-001**: decision compaction never rewrites or removes user or assistant text; only tool calls and results are candidates.
- **INV-002**: no tool result is ever left without its call; calls in the first message and in the newest `preserveRecentMessages` messages are never scored or removed.
- **INV-003**: no credential and no transcript leaves the machine for compaction scoring, ranking or digests; a hosted executor receives only routing and chooser questions, each with a clipped, redacted digest and cards, and only after the user's opt-in (ADR-0044 INV-005).
- **INV-004**: no request to julia1 or to any executor is awaited by the frontier model's turn.
- **INV-005**: every keep or drop decision records whether a rule or the executor produced it.
- **INV-006**: titles are sent to julia1 only when the user chose it as the local title model; otherwise they use the backend the user chose (decided by the user 2026-11-01; technical-design 8.4; token-savings 4.4.3).

## Consequences

- The plugin must pass `baseUrl` (today `hooks/fast-jev.ts` does not) to reach the local endpoint; an upstream request, not a fork.
- The julia1 sidecar serves three question kinds for three callers (compaction, routing, chooser); only the latter two may be redirected to a hosted endpoint by `router.endpoint`.
- The compaction endpoint setting and `router.endpoint` are distinct, so an opt-in for routing never moves compaction traffic.
- The tool chooser (same `ask` contract) and per-tool digests come at a later milestone (ADR-0038); the chooser now follows the router's shape (ADR-0044).
- Goal 2 says calls and results are "kept verbatim or removed", while the library also truncates a result to 300 characters plus a note; whether Shinatama keeps that third outcome is still unstated (worklist).

## Confirmation

- token-savings 9.2: the ledger on at least ten real sessions per harness; the tool re-run rate after compaction before and after each milestone; the latency of one incremental scoring pass while four agents run; that `session.compact` receives the main and sub-agent transcripts; a traffic capture showing compaction requests never reach a hosted endpoint.
- No register row is assigned to these checks yet (worklist #79). Nothing has been run.
