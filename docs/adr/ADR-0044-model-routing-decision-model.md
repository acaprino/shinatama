---
adr_id: ADR-0044
version: 1
title: Automatic model routing delegated to a configurable decision model, at session start and sub-agent spawn
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/specs/token-savings.md
    - docs/functional-design.md
    - docs/notes/2026-11-01-model-routing-decision-model.md
  modules:
  domains:
    - model-routing
    - router-key
    - credentials
    - token-savings
governs_symbols:
depends_on:
created_from_task: tokeneconomizer-routing-study-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0044: Automatic model routing delegated to a configurable decision model, at session start and sub-agent spawn

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

The user asked on 2026-11-01 for automatic model routing "per sessioni e agenti", system wide,
through a jev model or a similar router reached with one key of the user's, free or paid, and then
asked whether the router can "demandare la scelta a un jev-like decision model configurabile".
Shinatama has no code yet. Daodan's `model-routing` skill routes Daodan's own jobs with a generic
classifier (the cheapest model of the harness, asked for a JSON object) and a deterministic
selector, and asks a human to confirm each node. Provider prompt caches are per model, so a model
change inside one conversation resends the whole prefix at full price. The System One wire format
(TypeSafe Jev, `POST /v1/systemone`) has a `choice` question over up to 255 described labels that
returns a label, its confidence and the probabilities of the others, and its documentation lists
routing as a use case; Cloudflare's Clef models are documented as compatible with it. ADR-0031
(Proposed) says Shinatama holds no credentials; ADR-0037 (Proposed) rejects a hosted scorer and
keeps every transcript on the machine for scoring, ranking and digests. The study is
`docs/notes/2026-11-01-model-routing-decision-model.md`, sections 3 to 8.

## Decision Drivers

- Cost per finished task, without lowering the quality of the work.
- The prompt cache is per model: a routing decision is free only before a conversation has a cache.
- The choice is delegated to a decision model the user configures, not to a generic chat model.
- No credential of an agent is read or stored; the router key is the user's own and is protected.
- Routers often fail to beat "always the strong model" on agent work, so nothing is claimed before the ledger measures it (ADR-0038 INV-005).

## Decision

Shinatama routes the model automatically at three points only: at session start (a new instance,
a hop destination, a Roundtable participant, a task the project manager starts), at sub-agent spawn
(not a fork, which inherits the parent's cache), and for bookkeeping jobs, which already run on
julia1. It never changes the model between turns of one conversation. The choice is delegated to a
configurable decision model through one System One `choice` question: the labels are the models
the harness actually exposes in the box, minus those a deterministic gate rejects (a capability the
request needs, a context window too small, a provider not signed in); the state is a clipped,
redacted digest of the task line, first prompt or sub-agent brief, plus one card per label (prices,
context window, catalog tier and rank, the instance's default, and the user's own ledger record for
that model per kind of work). The label is taken when its confidence reaches `router.min_confidence`
and, where the executor returns one, its `reject` probability stays below `router.max_reject`;
otherwise, or on any failure, the instance keeps its default model. The executor is configured by
`router.endpoint`, a vault reference for the key, `router.model`, `router.timeout_ms`,
`router.min_confidence` and `router.mode` (`choice`, the default; `signals`, Daodan's ten bounded
signals asked as System One questions and resolved by its installed `task_router.py`; `off`).
Endpoints speaking System One (`POST /v1/systemone`) or the OpenRouter Decisions format
(`POST /v1/decisions`) need no adapter: julia1, a jev-like selector model, locally, the default, with no key; open jev-like
models served locally, such as gutsy on the CPU; TypeSafe Jev directly or through OpenRouter, or
Cloudflare Clef-flash, with the user's key. Other routers go through one adapter
`decide(digest, candidates) -> {label, confidence, probabilities}`. The binding is applied on the
harness's own seam (launch flags; on Claude Code the sub-agent model through a `PreToolUse` hook on
`Agent` or `CLAUDE_CODE_SUBAGENT_MODEL`), each seam verified on the installed version before use.
The key lives only in the operating system's vault (Windows Credential Manager) and is read only by
the router sidecar. This narrows ADR-0031 to agent credentials and ADR-0037's "nothing leaves the
machine" to compaction, digests and the tool chooser: routing alone may send its digest to a hosted
decision model, by the user's opt-in. The ledger records labels, cards, answer, executor, cost and
outcome per routed conversation; "always the default model" is the baseline it is measured against.
The same selector is the tool chooser, in place of the jev chooser of token-savings 6.5 (decided
by the user, 2026-11-01): on each prompt one `choice` question over the deferred tools the
instance exposes, minus those TokenEconomizer's evidence denied for this launch and never
BillionContext's recovery tools, which are always preloaded; TokenEconomizer's per-repository
usage evidence is the prior on each card; the same endpoint, key, thresholds and failure rule
apply, and below them nothing is preloaded and the model searches as today. Preloading adds
schemas at the tail of the turn's request and never changes the tool list of the cached prefix.
ADR-0037's "nothing leaves the machine" thus narrows to compaction and digests; the tool chooser
follows the router's opt-in. Study: `docs/notes/2026-11-01-model-routing-decision-model.md` 5.1.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a new Claude Code instance whose task line is "rename a local variable in one file", the box exposing four models, the decision model answering `haiku` with confidence 0.82 and `router.min_confidence` 0.6 -> Expected: the instance launches with `--model` set to that model, and the ledger records the labels, the cards, the answer and the binding.
- **EX-002** (boundary case): Input: a request that needs image input, and a decision model answering a label whose model has no image input -> Expected: that label was never offered; the gate removed it before the question.
- **EX-003** (boundary case): Input: a sub-agent spawned as a fork of the parent conversation -> Expected: not routed; it keeps the parent's model and cache.
- **EX-004** (rejection or error case): Input: the endpoint is unreachable, times out, answers a label outside the set, answers with confidence below `router.min_confidence`, or with a `reject` probability above `router.max_reject` -> Expected: the instance keeps its default model, never a cheaper one, and the ledger records why.
- **EX-005** (rejection or error case): Input: the user's second prompt in a running session -> Expected: no routing; the model stays as it was.
- **EX-006** (nominal case): Input: a prompt "run the tests and fix the failing one" on an instance whose deferred tools include a test runner's MCP tools, the selector answering them with confidence 0.9 -> Expected: those tools are preloaded on `prompt.submit`; the tool list of the cached prefix is unchanged; `mcp__bili__*` were preloaded regardless.
- **EX-007** (rejection or error case): Input: the chooser's answer below `router.min_confidence` -> Expected: nothing is preloaded; the model uses tool search as today.

## Rejected Alternatives

- **RA-001**: routing per turn; the prompt cache is per model, so each switch resends the whole prefix at full price (study sections 3 and 8.4).
- **RA-002**: a generic chat model naming the model in free text, as Daodan's classifier would if it picked; it can invent a model or a figure. The decision model answers only over a closed, gated label set.
- **RA-003**: routers that route and serve the request themselves (OpenRouter `auto` and Jev Router, Requesty, Azure model router) on the request path; the wire belongs to BillionContext and a second owner is excluded (token-savings 7.1). They are admitted only as decision sources behind the adapter.
- **RA-004**: a static prompt classifier trained on chat (RouteLLM) as the default; it fell below random on unfamiliar agentic tasks (study 8.4).
- **RA-005**: rewriting the `model` field of requests on the wire; the harness would believe it runs another model and a mid-session rewrite breaks the cache. A rewrite of a session's first request only is kept as a later, separately verified option.
- **RA-006**: storing the router key in Shinatama's configuration, database or launch manifest, or using a hosted router by default.

## Enforceable Invariants

- **INV-001**: the model of a conversation is chosen only before its first request (session start or non-fork sub-agent spawn) and never changed by the router between its turns.
- **INV-002**: the decision model can return only a label of the gated candidate set; any other answer is a failure.
- **INV-003**: every routing failure, every answer below `router.min_confidence` and every `reject` above `router.max_reject` resolves to the instance's default model.
- **INV-004**: the router key exists only in the operating system's vault; it never appears in Shinatama's database, snapshots, messages, configuration files, launch manifests or logs.
- **INV-005**: a hosted decision model receives only the clipped, redacted digest and the cards, never a transcript or an agent credential, and only after the user opts in.
- **INV-006**: no routing saving is stated before the ledger has measured it against the default-model baseline on the user's sessions.
- **INV-007**: the tool chooser never changes the tool list of a cached prefix and never offers a tool TokenEconomizer's evidence denied for the launch; BillionContext's recovery tools are never candidates and always preloaded.
