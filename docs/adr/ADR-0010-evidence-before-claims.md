---
adr_id: ADR-0010
version: 1
title: Behaviour is proved against installed versions with real fixtures captured from synthetic sessions; the verification register governs
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/verification-register.md
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - verification
    - evidence-tags
    - fixtures
    - spec-write-back
governs_symbols:
depends_on:
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0010: Behaviour is proved against installed versions with real fixtures captured from synthetic sessions; the verification register governs

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet, and most of what the specs say about the agents comes from
documentation, source or `--help` rather than from running anything: "Nothing here has been run
as a prototype" (technical-design 0). The question this record answers is when a statement about
an outside system counts as proved, and what happens to the spec when it is checked.

Origin: functional-design 3 states, with no decided or my-call marker, that agent behaviour
Shinatama depends on "must be verified against the installed version of each agent with real
captured fixtures. Documentation alone is not proof." Functional-design 8a adds the write-back
rule. The running and reporting rules are section 0 of the verification register, itself a draft
dated 2026-10-26; no user decision or date is recorded for them. The full rules are in the
register, sections 0 and 21; this record holds only the decision.

## Decision Drivers

- Agent behaviour differs per agent and per version and drifts without notice (Claude Code 2.1.310 to 2.1.311, register 4).
- Each spec sentence must say what is proved and what is not.
- The user's real conversations, credentials and homes must stay untouched by tests.
- A failed check must change the design, not disappear.

## Decision

A claim Shinatama depends on counts as proved only when a register row has been run on a real
machine against the installed version and has passed. Every factual claim about an outside system
carries an evidence tag: `[V]` verified on the user's machine, `[D]` documentation or source, `[R]` the
research task (read, not executed), `[S]` another tool's source, `[U]` unverified, `[C]` the
author's call, `[H: …]` read in the named `--help` output with nothing more run; a note after a
tag qualifies it (`[C; register M15]` names the row that tests a call, `[C; decided by the user
<date>]` marks a call the user decided), and a later record states its own date and versions
(technical-design 0). Every unproved claim is listed once in the register with how
to prove it, what counts as proved and where the result is written back. Each run writes one
record, `docs/verification/YYYY-MM-DD-<ID>-<topic>.md`, with machine, versions, commands, raw
result, pass or fail, and the spec text before and after. Write-back is part of the item: a row is
not `passed` until its spec sentence is changed to `[V]` with the date and the record's name, or
corrected; a failed item changes the spec and keeps the failure. Fixtures come from synthetic
sessions created for the test, with invented content, a marker string per turn and the agent
version in the file name. Anything that writes uses a scratch home; real homes are read only.
`G0` rows change the design if they fail, `G1` rows block one feature, `G2` rows tune a value.
An agent whose installed version fails its suite is `Degraded` (each failing capability
`unsupported`, launch-only plus its own export command, hops by seed or refused; ADR-0011) until
the suite passes again. `SEC` rows get a second reader. Where the register and technical-design 10 differ,
the register wins. "A claim is not 'done' until its row is `passed` and the sentence here has been
changed to match" (functional-design 8a).

## Verified Interview Examples

- **EX-001** (nominal case): Input: the spec claim that Pi can run a backend its TUI attaches to (register Q2) -> Expected: PI5, run on 2026-10-27, found no TUI attach; the row reads "passed (spec corrected)" and technical-design 5.7 item 4 carries the dated correction (register 21; record PI5).
- **EX-002** (rejection or error case): Input: D15, each agent started with only its home variable pointing at a private home -> Expected: the leaks found are recorded, functional-design 5.14 is rewritten to list the extra variables per agent, and the row stays "running (partial)" while login persistence waits on O1 (record D15).
- **EX-003** (boundary case): Input: an installed agent version that fails its fixtures -> Expected: the agent is `Degraded` (launch-only plus its own export command, failing capabilities `unsupported`, hops by seed or refused) with a visible reason instead of a wrong hop (technical-design 5.5.3, 11.10); its register rows are rerun on the new version before release notes claim support (register 0).
- **EX-004** (boundary case): Input: a check that needs the shape of the user's real store, such as the size of the OpenCode database -> Expected: only counts and column names are recorded; no real conversation enters the repository or a record (register 0).
- **EX-005** (boundary case): Input: Hermes Agent and DeepSeek Harness, known only from research -> Expected: every adapter capability declared `unsupported` until its row passes; nothing claimed from documentation alone (functional-design 2, goal 1).

## Rejected Alternatives

- **RA-001**: treating documentation, source reading or `--help` output as proof; "Documentation alone is not proof" (functional-design 3), and research facts are "Not the same as [V]" (technical-design 0).
- **RA-002**: copying the user's real conversations, or credential files, into the repository, a record, a fixture or a scratch home; never done (register 0). No further reason is recorded.

## Enforceable Invariants

- **INV-001**: no spec sentence is tagged `[V]` without a dated record in `docs/verification/` that it names.
- **INV-002**: a row is `passed` only after its spec sentence is changed to `[V]` or corrected; a failed or changed row keeps its record and its spec change.
- **INV-003**: fixtures are synthetic; no real conversation of the user is copied into the repository or a record.
- **INV-004**: tests that write use scratch homes; real homes are read with shared read access and never with `immutable` on a database an agent may be writing; test code never copies credential files; logins are done by the user, one per scratch home; every live-model test states a cost cap and records the cost.
- **INV-005**: after an agent version changes, the agent runs `Degraded` (ADR-0011) until its fixtures pass on the new version.
- **INV-006**: no measured saving percentage is claimed for compression until it is measured (functional-design 3).
- **INV-007**: `SEC` rows are reviewed by a second reader, and their records state the negative result as well as the positive.

## Consequences

- The register holds 266 rows (107 `G0`, 144 `G1`, 15 `G2`; 24 `SEC`; counted 2026-10-31); a 30-row first-milestone set decides the foundations.
- Scripts live in `verify/<ID>/`; raw output under `verify/*/out/` is ignored by Git and some records point to machine-local paths, so not all evidence is portable with the repository (register 0).
- The adapter conformance suite and `fixture_gate` carry this rule into the runtime (ADR-0011).
- Functional-design 3 and 8 speak of "real captured fixtures" and "real-fixture tests", while the register and technical-design 5.5.3 require synthetic, structure-only fixtures; read together, fixtures are captured from the real installed agent running synthetic sessions.
- Open: O1 (which accounts and scratch logins tests may use, and the cost cap per run), which also blocks D15's login persistence.

## Confirmation

- Each register row is confirmed by its own pass criterion, its record and its spec write-back.
- Runs so far (register 21): 17 rows with local records from 2026-10-27 and 2026-10-28, all on one Windows 11 machine, plus H27 (changed), whose record was removed from Mukade's docs. CX4 and OC2 passed, PI5 passed with the spec corrected, D17 changed, D1 is done; the other twelve are running, eleven of them partial and OC8 with its criterion not met on the real home. Most rows, including most of the first-milestone set, are open.
