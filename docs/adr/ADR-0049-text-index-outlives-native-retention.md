---
adr_id: ADR-0049
version: 1
title: Shinatama's text index outlives an agent's retention; an aged-out session is searchable and can seed, never natively resumed; retention is never edited
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - session-search
    - retention
    - text-index
    - native-resume
governs_symbols:
depends_on:
  - ADR-0009
  - ADR-0023
created_from_task: frd-adr-consolidation-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: 620bcf9a66edeb57d0528b3c522f3018928b3706
accepted_on: 2026-11-01
accepted_at_sha: 202a34e2675cfe7e8d7ba1d5e7ecf88bf2299182
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0049: Shinatama's text index outlives an agent's retention; an aged-out session is searchable and can seed, never natively resumed; retention is never edited

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at 202a34e · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. Claude Code deletes session files older than `cleanupPeriodDays`
(default 30 days; technical-design 5.7, read from documentation, register CL7). Goal 3 promises
that "any old chat, in any supported agent, is found by searching and resumed"; for an aged-out
Claude session that promise cannot be kept as a native resume. Technical-design 5.7 and 4.2 settle
it: Shinatama's own text index keeps the conversation text, so the session can still be found and
can still seed another agent or a new Claude session, but not be natively resumed, and the UI says
which. ADR-0023 covers the sessions surface and ADR-0009 the index tables, but neither records this
rule, which shapes what search may promise. The rule is the author's call (technical-design 5.7),
and raising the retention is the user's setting, never edited by Shinatama. Hermes Agent has no
pruning by default (`sessions.auto_prune`, 90 days when on); DeepSeek Harness deletes nothing. This
record holds only the decision.

## Decision Drivers

- Search must not promise a resume that the agent's own store no longer allows.
- The user's work history should not disappear because an agent cleans its folder.
- Shinatama never edits a `system` agent's settings (ADR-0012).

## Decision

The text index (`session_index`, `session_text` with FTS5; ADR-0009) is incremental and keeps the
indexed text of a session after the agent deleted its file, marking the session as deleted
(register S7). Search returns such a session and shows which of three actions it allows: a native
resume only while the agent's store still holds the session; otherwise a seed into another agent or
into a new session of the same agent, from the indexed text through the canonical transcript. The
hop dialog, the sessions surface and `shinatama resume` say so before acting; a hop back whose
earlier native session is gone falls back to `seed_prompt` and says why (technical-design 6.6).
Shinatama never raises or edits an agent's retention setting; it reports the agent's rule in
`doctor`. The index is operational and rebuildable from the stores for the sessions that still
exist; text kept for deleted sessions is lost when the index is rebuilt, and `doctor` says so
before rebuilding.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a Claude session indexed 40 days ago, with `cleanupPeriodDays` at its default, whose file is gone -> Expected: `shinatama find` still returns it, marked deleted; the actions offered are "seed into …" only; "resume" is absent (register CL7, S7).
- **EX-002** (nominal case): Input: the same session, Enter pressed in find -> Expected: a new session of the same agent starts seeded from the indexed text, labelled as a seed, not a native resume.
- **EX-003** (boundary case): Input: a hop back to an earlier Claude segment whose file retention removed -> Expected: `seed_prompt` with the reason shown (technical-design 6.6 item 4).
- **EX-004** (rejection or error case): Input: `shinatama doctor` is asked to rebuild the index -> Expected: it states how many deleted sessions' text would be lost and asks before proceeding.
- **EX-005** (rejection or error case): Input: a request to raise Claude's retention from Shinatama -> Expected: refused; the setting is the user's, in the agent's own configuration.

## Rejected Alternatives

- **RA-001**: copying aged-out session files into Shinatama's storage to keep them resumable; Shinatama never copies sessions between project folders or stores (technical-design 5.7) and native stores are read-only in v1 (ADR-0016).
- **RA-002**: editing `cleanupPeriodDays` in the user's settings; Shinatama never edits a `system` program's configuration (ADR-0012, ADR-0013).
- **RA-003**: hiding deleted sessions from search; the index exists so that the text is not lost.

## Enforceable Invariants

- **INV-001**: a session whose native file or rows are gone is never offered a native resume; the UI and the command line show seed as the only continuation.
- **INV-002**: the text index keeps a deleted session's text and marks it deleted; a rebuild that would drop it warns first.
- **INV-003**: Shinatama never writes an agent's retention setting.
- **INV-004**: a hop back to a gone session falls back to `seed_prompt` and says why (ADR-0019).

## Consequences

- The index is more than a cache for these sessions; `doctor`'s rebuild needs a warning path.
- The seed of an aged-out session is built from the index, not from the store; the redaction and provenance rules of ADR-0019 and ADR-0021 apply unchanged.
- Hermes Agent's optional pruning and DeepSeek Harness's never-delete rule are recorded per adapter.

## Confirmation

- Register CL7 confirms how retention is applied on the installed version and that the index keeps an aged-out session searchable and usable as a seed; S7 proves incremental indexing of deleted files.
- Nothing has run.
