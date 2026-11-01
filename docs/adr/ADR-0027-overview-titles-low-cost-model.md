---
adr_id: ADR-0027
version: 1
title: Overview titles from a pluggable low-cost backend that Shinatama calls, capped; facts from git only
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - global-overview
    - activity-titles
    - low-cost-model
    - worktree-collisions
    - summary-privacy
governs_symbols:
depends_on:
  - ADR-0026
  - ADR-0028
  - ADR-0030
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

# ADR-0027: Overview titles from a pluggable low-cost backend that Shinatama calls, capped; facts from git only

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user asked for "a global view tab with all the instances, how
they collaborate, what they are doing, a low-cost model titling their activity, the
communication flows, the exchanges and the work" (functional-design goal 12 and 5.13, decided as
a wish). The user also decided that the table shows each instance's git branch and worktree and
that every tab shows its agent's icon. The rest is the author's call: that Shinatama, not the
agents, calls the model; the three backends (technical-design 8.4, tagged [C]); and the $0.50
daily cap, listed in technical-design 12 among the author's defaults. The default backend is the
author's recommendation, to be chosen by a quality gate in the first milestone and by the user
(register O2). The full design is in functional-design 5.13 and technical-design 8.4 and 8.5;
this record holds only the decision.

## Decision Drivers

- Agents cannot be made to title their own activity reliably.
- Shinatama holds no credentials (ADR-0030).
- Summaries send parts of a transcript to a provider.
- The cost must stay small and capped.
- The view shows facts, never guesses about intent.

## Decision

The overview tab shows every instance started from or inside Shinatama: agent and model, project
and thread, state, what it is doing, tokens and cost, and context fill, beside the flows between
instances, a timeline of exchanges and changes, and the work per shared folder. It reads; its
only actions are focus a session, hop and pause an instance's summaries.

What an instance is doing is a title that Shinatama asks a low-cost model for, on a state change
and every 2 minutes while `working`: an input of at most 6 KB from the transcript tail, redacted,
and an answer of one line of at most 12 words, stored in `instance.doing_title`, with tokens and
cost logged in `summary_log`. The titler is an interface with three backends, one active and
chosen by the user: a small local GGUF model run through llama.cpp (the idea of `kouhxp/fftext`,
not its code), a one-time opt-in download of about 500 MB with its licence shown; a provider
low-cost model chosen from the catalog of a provider the user is signed in with, by capability
and price and never by a name in the code, called through the sidecar's auxiliary-call path or
the agent's own non-interactive mode; or none. A daily cap (default $0.50 for paid backends) and
an off switch exist; with no backend, at the cap or off, the title is the last tool or message
verbatim. A thread marked `summaries_excluded` is never sent. The recommended default offers the
local backend at first run and otherwise none; the quality gate decides.

Branch and worktree are read from git (`rev-parse`, `status --porcelain=v1 -z`) at start and at
each `idle` transition; non-git folders show none. "How they collaborate" is built only from known
facts (hops, roundtable groups, shared project folders) and infers no intent. A `file_collision`
is raised only when two instances share a worktree root and their modified paths intersect. See
functional-design 5.13 and technical-design 8.4 and 8.5.

## Verified Interview Examples

- **EX-001** (boundary case): Input: two instances modify the same path in two different worktrees -> Expected: no collision; equal worktree roots with intersecting modified paths raise `file_collision` (functional-design 5.13; technical-design 8.5).
- **EX-002** (boundary case): Input: a paid backend reaches the daily cap -> Expected: the cap holds and `doing_title` falls back to the verbatim last tool or message (technical-design 8.4; register I7).
- **EX-003** (rejection or error case): Input: a thread marked `summaries_excluded`, and a transcript tail with a planted fake key and bearer token, under network capture -> Expected: no excluded or secret text leaves the machine (register I9; technical-design 11.6).
- **EX-004** (nominal case): Input: backends 1 and 2 on about 50 recorded activity snapshots, scored by a written rubric (names the right file or action, no invented fact, at most 12 words) -> Expected: the default backend chosen from the result (technical-design 8.4; register I5).

## Rejected Alternatives

- **RA-001**: asking the agents to title their own activity; they cannot be made to do it reliably.
- **RA-002**: using fftext's code; it is Python, a CLI and no library, so Shinatama takes only the idea of a small local model.
- **RA-003**: choosing the provider model by a name written in the code; the docs choose by capability and price from the catalog and give no further reason.

## Enforceable Invariants

- **INV-001**: titles come only from the backend Shinatama calls; no agent is asked for one.
- **INV-002**: a thread marked `summaries_excluded` is never sent to any backend; every title input is at most 6 KB and redacted before sending.
- **INV-003**: paid title calls stop at the daily cap; at the cap, switched off or with no backend, the title is verbatim.
- **INV-004**: a title is one line of at most 12 words.
- **INV-005**: Shinatama holds no credentials for title calls (technical-design I5).
- **INV-006**: branch and worktree come from git, never guessed; `file_collision` requires equal worktree roots and intersecting modified paths.
- **INV-007**: the overview's only actions are focus a session, hop and pause an instance's summaries.

## Consequences

- The default backend waits for the quality gate (technical-design 10 item 9; register I5) and the user's choice (O2). The local model's CPU and latency with four agents running, its licence and the fftext README claims are unmeasured (I6); whether a 0.8B model writes useful titles is not assumed.
- Whether the sidecar's auxiliary-call path can carry title calls without handling credentials is open; otherwise the titler uses another route (register SC10).
- Summary calls and the agents' own traffic are the only data that leaves the machine (technical-design 4.7); first-use disclosure and per-thread exclusion are required.
- How a changed file is attributed to an instance is open (register I4); the cost of `git status` at each idle on large repositories needs bounding (I3); token, cost and context-fill figures need checking per agent (I2).
- The token-savings spec routes titles to julia1, which shares the local runtime and the one-time download with backend 1 (token-savings 4.4, 7; ADR-0036).
- Icons have a text or letter badge fallback and accessible names independent of colour (register I10).

## Confirmation

- Register I2 to I7, I9, I10 and SC10 pass; all are open.
- Acceptance test 11.6 passes, including summary inputs.
- Nothing has been run yet.
