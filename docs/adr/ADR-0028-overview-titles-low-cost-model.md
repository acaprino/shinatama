---
adr_id: ADR-0028
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
  - ADR-0027
  - ADR-0029
  - ADR-0031
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

# ADR-0028: Overview titles from a pluggable low-cost backend that Shinatama calls, capped; facts from git only

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user asked for "a global view tab with all the instances, how
they collaborate, what they are doing, a low-cost model titling their activity, the
communication flows, the exchanges and the work" (functional-design goal 12 and 5.13, decided as
a wish). The user also decided that the table shows each instance's git branch and worktree and
that every tab shows its agent's icon, and on 2026-11-01: the default title backend is the local
model, offered at first run (about 500 MB, with consent and its licence shown), and none if it is
declined, while a provider low-cost model stays a user choice; the quality gate of the first
milestone confirms that default (register O2, I5); the overview stays read-mostly, and a new
instance in its own linked worktree is an option of the new-instance command, not an overview
action; a changed file is attributed to an instance from tool-call targets, a change that cannot
be attributed is warned as a "possible collision" naming no instance, and generated files are
excluded (register Q3, I4). The rest is the author's call: that Shinatama, not the agents, calls
the model; the three backends (technical-design 8.4, tagged [C]); and the $0.50 daily cap, listed
in technical-design 12 among the author's defaults. The full design is in functional-design 5.13
and technical-design 8.4 and 8.5;
this record holds only the decision.

## Decision Drivers

- Agents cannot be made to title their own activity reliably.
- Shinatama holds no credentials (ADR-0031).
- Summaries send parts of a transcript to a provider.
- The cost must stay small and capped.
- The view shows facts, never guesses about intent.

## Decision

The overview tab shows every instance started from or inside Shinatama: agent and model, project
and thread, state, what it is doing, tokens and cost, and context fill, beside the flows between
instances, a timeline of exchanges and changes, and the work per shared folder. It stays
read-mostly (user, 2026-11-01): its only actions are focus a session, hop and pause an instance's
summaries. A new instance in its own linked worktree is an option of the new-instance command,
from the project manager or a tab (functional-design 5.4), not an overview action.

What an instance is doing is a title that Shinatama asks a low-cost model for, on every state
change and every 2 minutes while `working` (defaults; the frequency is a setting): an input of
at most 6 KB from the transcript tail, redacted,
and an answer of one line of at most 12 words, stored in `instance.doing_title`, with tokens and
cost logged in `summary_log`. The titler is an interface with three backends, one active and
chosen by the user: a small local GGUF model run through llama.cpp (the idea of `kouhxp/fftext`,
not its code), a one-time opt-in download of about 500 MB with its licence shown; a provider
low-cost model chosen from the catalog of a provider the user is signed in with, by capability
and price and never by a name in the code, called through the sidecar's auxiliary-call path or
the agent's own non-interactive mode; or none. A daily cap (default $0.50 for paid backends) and
an off switch exist; with no backend, at the cap or off, the title is the last tool or message
verbatim. A thread marked `summaries_excluded` is never sent. The default, decided by the user on
2026-11-01 (register O2), offers the local backend at first run, after consent and with its
licence shown, and otherwise is none; a provider model is used only when the user chooses it.
The quality gate confirms the default.

Branch and worktree are read from git (`rev-parse`, `status --porcelain=v1 -z`) at start and at
each `idle` transition; non-git folders show none. "How they collaborate" is built only from known
facts (hops, roundtable groups, shared project folders) and infers no intent. `git status` says
that a file changed, not which instance changed it, so each modified path is attributed from the
tool-call targets in the instances' events and transcripts (user, 2026-11-01). A
`file_collision`, naming both instances, is raised only when two instances share a worktree root
and are both attributed a modified path; a modified path that cannot be attributed, while two or
more instances work in that worktree, raises a "possible collision" warning that names no
instance. Generated files are excluded. See functional-design 5.13 and technical-design 8.4 and
8.5.

## Verified Interview Examples

- **EX-001** (boundary case): Input: two instances modify the same path in two different worktrees -> Expected: no collision; in one worktree, a path attributed to both from their tool-call targets raises `file_collision` naming both, a path that cannot be attributed (changed by a shell command) raises "possible collision" naming no instance, and a generated file raises nothing (functional-design 5.13; technical-design 8.5; register I4).
- **EX-002** (boundary case): Input: a paid backend reaches the daily cap -> Expected: the cap holds and `doing_title` falls back to the verbatim last tool or message (technical-design 8.4; register I7).
- **EX-003** (rejection or error case): Input: a thread marked `summaries_excluded`, and a transcript tail with a planted fake key and bearer token, under network capture -> Expected: no excluded or secret text leaves the machine (register I9; technical-design 11.6).
- **EX-004** (nominal case): Input: backends 1 and 2 on about 50 recorded activity snapshots, scored by a written rubric (names the right file or action, no invented fact, at most 12 words) -> Expected: the result confirms the default the user decided, the local model offered at first run (technical-design 8.4; register I5, O2).

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
- **INV-006**: branch and worktree come from git, never guessed; `file_collision` requires equal worktree roots and a modified path attributed to both instances from their tool-call targets; a path that cannot be attributed raises at most a "possible collision" that names no instance; generated files raise neither (decided by the user 2026-11-01; technical-design 8.5).
- **INV-007**: the overview's only actions are focus a session, hop and pause an instance's summaries; starting an instance, in its own worktree or not, is not an overview action (decided by the user 2026-11-01).
- **INV-008**: with default settings no title call leaves the machine: the local model is downloaded only after the user's consent, and while it is declined or not downloaded the backend is none; a provider backend runs only when the user chose it (decided by the user 2026-11-01; register O2).

## Consequences

- The default backend is decided (register O2, user 2026-11-01); the quality gate confirms it (technical-design 10 item 9; register I5). The local model's CPU and latency with four agents running, its licence and the fftext README claims are unmeasured (I6); whether a 0.8B model writes useful titles is not assumed.
- Whether the sidecar's auxiliary-call path can carry title calls without handling credentials is open; otherwise the titler uses another route (register SC10).
- Summary calls and the agents' own traffic are the only data that leaves the machine (technical-design 4.7); first-use disclosure and per-thread exclusion are required.
- The attribution method is decided and register I4 tests it, with changes made by tool calls and by shell commands, and checks that generated files raise nothing; the cost of `git status` at each idle on large repositories needs bounding (I3); token, cost and context-fill figures need checking per agent (I2).
- The title backend stays the user's choice: titles go through julia1 only when it is the chosen local model; julia1 and the local title model share the llama.cpp runtime, and when one model serves both there is one download (user, 2026-11-01; token-savings 4.4.3, 7; ADR-0037).
- Icons have a text or letter badge fallback and accessible names independent of colour (register I10).

## Confirmation

- Register I2 to I7, I9, I10 and SC10 pass; all are open.
- Acceptance test 11.6 passes, including summary inputs.
- Nothing has been run yet.
