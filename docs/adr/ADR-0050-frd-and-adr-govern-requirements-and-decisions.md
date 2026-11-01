---
adr_id: ADR-0050
version: 1
title: Requirements live in FRD records and decisions in ADR records, both Proposed until the user approves the text; design documents keep the design, the register alone says what is proved
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

# ADR-0050: Requirements live in FRD records and decisions in ADR records, both Proposed until the user approves the text; design documents keep the design, the register alone says what is proved

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at 202a34e · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

The repository holds two design documents, five specifications, two studies, a verification
register with 295 rows, 45 decision records and 13 requirements records, all written between
2026-10-26 and 2026-11-01. On 2026-11-01 the user decided that decision records are this project's
method from now on (companion spec 9.1), and the backfilled records ADR-0004 to ADR-0040 were
accepted in bulk the same day. What was never recorded is how the four kinds of document relate:
which one a reader trusts for a requirement, for a decision, for the design and for proof; how a
record changes status; and what a disagreement between them means. The codebase map states a rule
("when a record and a document disagree, the disagreement is a defect, not a precedence rule") that
no record holds. This record is the author's proposal of the method the set already follows, so that
it can be approved or changed in one place.

## Decision Drivers

- A reader must know, for any statement, which document is the authority for it.
- A decision made in conversation must survive the conversation.
- Nothing may read as approved or proved before it is.
- The design documents must stay editable without rewriting the records on every change.

## Decision

Four kinds of document, each with one job:

1. **Design documents** (`functional-design.md`, `technical-design.md`, the specifications under
   `docs/` and `docs/specs/`) keep the full design and its reasoning and stay the place where details
   change. They mark every statement with its origin (Decided, My call, Open; the evidence tags of
   technical-design 0).
2. **Decision records** (`docs/adr/`, one per version of a decision) hold only the decision, with
   context, drivers, examples, rejected alternatives, invariants, consequences and confirmation.
   Proposed when written, Accepted when the user approves the text, Superseded by a later version,
   Abandoned when retired unapproved. A change of an accepted decision is a new version, never an
   edit of the accepted text.
3. **Requirements records** (`docs/frd/`, one readable Markdown record per area, with the tool's
   JSON evidence record beside it where one exists) hold the requirements with origin, delivery
   slice, proof row and governing records. Proposed when written, Accepted when the user approves
   the text. A requirement never claims a pass.
4. **The verification register** alone says whether a claim is proved; a `[V]` in a design document
   names the record that proved it (ADR-0010).

When two of them disagree, the disagreement is a defect to fix in one of them, not a precedence
rule; the inconsistency worklist (`docs/notes/2026-11-01-design-inconsistencies.md`) is where such
defects are queued until fixed. The codebase map is regenerated when documents are added, renamed
or retired. Studies under `docs/notes/` are inputs, never authorities.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the user decides in conversation that a fork hop carries no task -> Expected: the design document gains the rule marked "decided by the user" with the date; a decision record (ADR-0034) holds the decision; the requirements record (FRD-0005, FRD-0008) gains the requirement with origin U; the register gains or updates the row that proves it (D13).
- **EX-002** (nominal case): Input: ADR-0044 narrows two accepted records -> Expected: ADR-0031 v2 and ADR-0037 v2 are written as Proposed versions; the v1 texts are not edited; the index lists them as Proposed until approved.
- **EX-003** (rejection or error case): Input: a record says "verified" for a row the register marks open -> Expected: a defect in the record; it is corrected to the register's status.
- **EX-004** (boundary case): Input: a design document is edited -> Expected: the Markdown requirements records that cite it are refreshed in the same commit; the JSON evidence records are refreshed with the tool, since their spans are bound to the file's hash.
- **EX-005** (rejection or error case): Input: a requirement found only in a study note -> Expected: not a requirement until a design document or specification states it; the note is cited as its source of reasoning only.

## Rejected Alternatives

- **RA-001**: the design documents as the only authority, with no records; decisions made in conversation were being lost and re-argued, which is why the user chose records (2026-11-01).
- **RA-002**: records that replace the design documents; a record holds a decision, not the design, and would grow into a second design document.
- **RA-003**: a precedence rule (records over documents, or the reverse); it hides defects instead of fixing them.
- **RA-004**: machine-readable requirements only (the JSON records); they are evidence, not a text a person reviews and approves.

## Enforceable Invariants

- **INV-001**: every decision the user makes is written in a design document with its date and in a decision record; a decision found only in a conversation or a study is not a decision of the project.
- **INV-002**: an accepted record's text is never edited; a change is a new version with its own status.
- **INV-003**: no requirements record or decision record states that a row passed; only the register and the `[V]` tags of the design documents do.
- **INV-004**: every requirement names its origin (U, A, P or S), its slice and its proof row or `—`.
- **INV-005**: a disagreement between two documents is recorded in the inconsistency worklist until it is fixed in one of them.

## Consequences

- Writing a decision costs three edits (document, record, requirement) and one register row; that cost is the method.
- The JSON evidence records go stale on every edit of a cited document and are refreshed with the tool; the Markdown records are refreshed by hand.
- `adr_tool.py` and `frd_tool.py` live in Daodan, not here; their index blocks in the two README files are regenerated when the tools run, and hand edits to those blocks are provisional.

## Confirmation

- The user approves or amends this record; the 50 decision records and the 17 requirements records then carry the status the user gives them.
- Not a register item: this record is a method, not a claim about an outside system.
