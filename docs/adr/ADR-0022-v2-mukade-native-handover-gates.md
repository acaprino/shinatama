---
adr_id: ADR-0022
version: 2
title: The native handover route is closed; its rules survive only as the re-entry preconditions of a future decision
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/mukade-handover.md
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - native-handover
    - mukade-hop
    - write-recovery
    - store-roots
    - handover-privacy
governs_symbols:
depends_on:
  - ADR-0016
  - ADR-0018
  - ADR-0020
created_from_task: user-decision-2026-11-03
proposed_on: 2026-11-03
proposed_at_sha: ca72385451d6196c0e0c0968640f1e9d94e3e0ff
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - Decision (route closed)
  - INV-001
  - INV-008
---

# ADR-0022: The native handover route is closed; its rules survive only as the re-entry preconditions of a future decision

<!-- adr-header:start -->
Version 2 · Proposed 2026-11-03 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Version 1 set the rules Shinatama would follow around a post-v1 `mukade::hop` write: journal
before write, receipt is not acceptance, no automatic retry, `ReasoningPolicy::Drop`,
per-instance `StoreRoots`, and the mukade-handover 5 gates. ADR-0016 v2 (2026-11-03, user
decision) removes the route itself: handoff always carries the seed document and Shinatama
never writes an agent's native store. The question this version answers is what remains of the
v1 rules now that nothing they govern can happen.

## Decision Drivers

- A closed route still needs its rules preserved: a future decision may reopen native handover,
  and the failure modes v1 analysed (duplicate writes, uncertain outcomes, ambient store
  roots, reasoning payloads) do not disappear with the route.
- mukade-handover.md describes the write path in detail; deleting or rewriting it would lose
  the reviewed design, while leaving it unmarked would advertise a route that does not exist.

## Decision

- The native handover route is closed in every version by ADR-0016 v2: no `mukade::hop` /
  `hop_into` / writer call, no native store write, nothing of version 1's sequence is
  reachable.
- The rules of ADR-0022 v1 — plan and preview, journal `hop.id` and intended effect before any
  external work, fingerprint recheck, receipt-is-not-acceptance with the separate nonce,
  no automatic second `hop()` after an uncertain outcome, `SourceChanged`/`WriteUncertain`/
  `ResumeFailed` handling, `ReasoningPolicy::Drop`, per-instance `StoreRoots` (never ambient
  defaults, never process-wide environment changes for concurrent hops), child isolation, and
  the mukade-handover 5 gates — survive **only** as mandatory preconditions for any future
  decision that reopens the route. They are not an active code path and create no v1 work.
- `docs/mukade-handover.md` is kept as the design record of the closed route, with a status
  header that says so; it is the document a re-entry decision would have to update.

## Verified Interview Examples

- **EX-001** (rejection or error case): Input: a design or feature proposal that calls `mukade::hop` from Shinatama -> Expected: rejected while ADR-0016 v2 stands; the proposal must first supersede that record and satisfy the v1 preconditions preserved here.
- **EX-002** (boundary case): Input: Mukade upstream adds write-side capabilities (e.g. the native compaction mode pending uncommitted on 2026-11-03) -> Expected: no Shinatama obligation arises; read-side pin advances are unaffected (ADR-0020).
- **EX-003** (nominal case): Input: any handoff under this record -> Expected: the ADR-0019 seed document; the v1 sequence (journal, write, verify, switch) never starts.

## Rejected Alternatives

- **RA-001**: retiring (Abandoned) ADR-0022 v1 outright; the failure-mode analysis is expensive to reconstruct and a re-entry decision would need it verbatim.
- **RA-002**: keeping v1 Accepted with its "post-v1 only" phrasing; after ADR-0016 v2 that phrasing advertises a route no version contains.
- **RA-003**: deleting mukade-handover.md; it is the reviewed design record the pin evidence and section 8 history refer to.

## Enforceable Invariants

- **INV-001**: while ADR-0016 v2 stands, no native write starts at all — the v1 precondition
  "journal before write" is unreachable because no write exists.
- **INV-002** to **INV-007** of version 1 are preserved unchanged as re-entry preconditions
  (journal content, no automatic retry, receipt versus READY, receipt persistence on failure,
  `ReasoningPolicy::Drop` with unaltered signed payloads, new-session-only writes with no
  concurrent headless resume, per-instance `StoreRoots` with the same-harness two-home refusal).
- **INV-008**: a re-entry decision must state how each preserved precondition is implemented
  and proven before the route opens; capability existence in Mukade is never sufficient.

## Consequences

- Register next-run item 8 (SS5 against `hop_into`) is withdrawn; SS4's post-v1 write part and
  SS5's staged-transfer part close; SS5's catalog-identity part and SS6 stay.
- mukade-handover sections 2 to 7 describe a closed route; its header status records this.
- The pin advance to `14edbf3` (2026-11-03) remains valid and read-side-driven; `hop_into` in
  the pin is capability Shinatama does not use.
- Version 1's "None of this is reachable in v1" becomes "not reachable in any version".

## Confirmation

- Nothing has been run; the route is closed by decision, not by evidence of failure.
- If a future record supersedes ADR-0016 v2, this record's preconditions and register SS4/SS5/SS6
  rows are its starting gates (mukade-handover 7).
