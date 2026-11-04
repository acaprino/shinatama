---
adr_id: ADR-0050
version: 2
title: Requirements live in FRD records and decisions in ADR records, both Proposed until the user approves the text; design documents keep the design, the register alone says what is proved; requirements records are Markdown only
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/frd/README.md
    - docs/adr/README.md
    - docs/codebase-map.md
    - docs/verification-register.md
  modules:
  domains:
    - documentation-method
    - decision-records
    - requirements-records
    - approval
governs_symbols:
depends_on:
  - ADR-0010
created_from_task: user-decision-2026-11-04
proposed_on: 2026-11-04
proposed_at_sha: 548670982b841e778c7698d213a97c8c6aebd052
accepted_on: 2026-11-04
accepted_at_sha: e6e452d44c9d90a744f5e81e3151ca05cd8af5a3
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - Decision item 3 (JSON evidence records removed)
  - EX-004
  - RA-005
  - Consequences
---

# ADR-0050: Requirements live in FRD records and decisions in ADR records, both Proposed until the user approves the text; design documents keep the design, the register alone says what is proved; requirements records are Markdown only

<!-- adr-header:start -->
Version 2 · Accepted 2026-11-04 at e6e452d · proposed 2026-11-04 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Version 1 kept, beside each readable Markdown requirements record, the JSON evidence record of
Daodan's project-delivery `frd-method` (`frd_tool.py`): every requirement bound to exact spans
of the design documents by SHA-256 and code-point offsets, refreshed and accepted only through
the tool (`validate`, `promote`). In practice the tool lives in Daodan, not in this repository;
the spans went stale on the same day they were extracted (2026-11-01) with every coherence fix,
were never refreshed, and the 14 JSON files (about 1.2 MB) advertised a machine proof nobody
could regenerate. On 2026-11-04 the user decided to remove the JSON evidence mechanism from the
project entirely ("questa cosa dei json … estirpiamola completamente da tutto il progetto"). This
version records that decision; everything else in version 1 stands.

## Decision Drivers

- A second representation nobody can regenerate is not evidence: it misstates its own authority
  the moment any cited document is edited.
- `frd_tool.py` is not in this repository and never ran here; the "refresh with the tool"
  obligation of version 1 was unsatisfiable.
- The Markdown records are what a person reviews, approves and builds from, and they already
  carry origin (U/A/P/S), slice, proof row and governing ADRs per requirement.
- The design documents are edited often (the 2026-11-04 hop rewrite alone touched hundreds of
  lines); hash-bound spans would go stale on every such commit.

## Decision

The four kinds of document of version 1 stand, with item 3 changed:

1. **Design documents** keep the full design and stay the place where details change, with
   their origin marks and evidence tags.
2. **Decision records** (`docs/adr/`) hold only the decision; Proposed when written, Accepted
   when the user approves the text, Superseded by a later version, Abandoned when retired
   unapproved; an accepted text is never edited.
3. **Requirements records** (`docs/frd/`) are the readable Markdown records
   (`FRD-NNNN-<slug>.md`) **only**. The JSON evidence records of `frd_tool.py` are retired and
   removed from the repository; they remain recoverable from Git history. No tool step
   (`extract`, `validate`, `promote`) is part of this project's method; a record is Accepted
   when the user approves its Markdown text. Provenance stays enforceable through the origin
   code, the `Sources` header row and the proof column that names register rows.
4. **The verification register** alone says whether a claim is proved.

Disagreement between documents remains a defect to fix, not a precedence rule; the codebase map
is regenerated or amended when documents are added, renamed or retired.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the user decides in conversation that a fork hop carries no task -> Expected: the design document gains the rule marked "decided by the user" with the date; a decision record (ADR-0034) holds the decision; the requirements record (FRD-0005, FRD-0008) gains the requirement with origin U; the register gains or updates the row that proves it (D13).
- **EX-002** (nominal case): Input: ADR-0044 narrows two accepted records -> Expected: ADR-0031 v2 and ADR-0037 v2 are written as Proposed versions; the v1 texts are not edited; the index lists them as Proposed until approved.
- **EX-003** (rejection or error case): Input: a record says "verified" for a row the register marks open -> Expected: a defect in the record; it is corrected to the register's status.
- **EX-004** (boundary case): Input: a design document is edited -> Expected: the Markdown requirements records that cite it are refreshed by hand in the same commit; there is no JSON evidence to refresh and no tool step.
- **EX-005** (rejection or error case): Input: a requirement found only in a study note -> Expected: not a requirement until a design document or specification states it; the note is cited as its source of reasoning only.

## Rejected Alternatives

- **RA-001**: the design documents as the only authority, with no records; decisions made in conversation were being lost and re-argued, which is why the user chose records (2026-11-01).
- **RA-002**: records that replace the design documents; a record holds a decision, not the design.
- **RA-003**: a precedence rule (records over documents, or the reverse); it hides defects instead of fixing them.
- **RA-004**: machine-readable requirements only (the JSON records); they are evidence, not a text a person reviews and approves.
- **RA-005**: keeping the JSON files as frozen historical snapshots beside the Markdown; a permanently stale second representation invites confusion about which text governs, and Git history already preserves them byte for byte.

## Enforceable Invariants

- **INV-001**: every decision the user makes is written in a design document with its date and in a decision record; a decision found only in a conversation or a study is not a decision of the project.
- **INV-002**: an accepted record's text is never edited; a change is a new version with its own status.
- **INV-003**: no requirements record or decision record states that a row passed; only the register and the `[V]` tags of the design documents do.
- **INV-004**: every requirement names its origin (U, A, P or S), its slice and its proof row or `—`.
- **INV-005**: a disagreement between two documents is recorded in the inconsistency worklist until it is fixed in one of them.
- **INV-006**: no requirements record exists in JSON form; `docs/frd/` contains Markdown records and their README only.

## Consequences

- The 14 JSON files (FRD-0001 to FRD-0014) are deleted; the "Relation to the JSON record"
  sections, the JSON status wording in every record header, the `frd_tool.py` index block in
  `docs/frd/README.md` and the JSON description in `docs/codebase-map.md` 4a are removed with
  them.
- What is lost: the exact machine-checked spans from the 2026-11-01 extraction (866 sentences).
  They were stale since that day and regenerable only with a tool this repository does not
  have; Git history keeps them (last refreshed at `c84d28e`).
- Writing a decision still costs three edits (document, record, requirement) and one register
  row; maintaining the records no longer has an unsatisfiable tool obligation.
- `adr_tool.py` (the ADR index block) is unaffected by this record; it also lives in Daodan and
  its index edits stay provisional hand edits here.

## Confirmation

- The user approves or amends this text; on approval ADR-0050 v1 becomes Superseded and the
  Markdown records are the only requirements records.
- Not a register item: this record is a method, not a claim about an outside system.
