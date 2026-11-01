---
adr_id: ADR-0042
version: 1
title: A per-person ledger of what the companion explained, written by Shinatama from the companion's card block
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/specs/companion-explainer.md
    - docs/technical-design.md
  modules:
  domains:
    - companion
    - learning-memory
governs_symbols:
depends_on:
  - ADR-0002
created_from_task: init-abfe6369faa4489a915c5f057a84e467
proposed_on: 2026-11-01
proposed_at_sha: 6539cf1d5bf6564247a6d8b5d47ccc114f9fdc58
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0042: A per-person ledger of what the companion explained, written by Shinatama from the companion's card block

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

The user wants the tutor not to restart from the basics and not to explain the same things again
(pasted design, 2026-11-01). The first design of this record was a memory of what the user marked
as understood, set by a "got it" press. The user replaced it in two later pastes: the memory is
"qualcosa che viene scritto dall'agente man mano che dà risposte o spiegazioni, sotto forma di
lista di conoscenze già spiegate, con keyword, titoli, temi, molto sintetica e context saving";
what was already explained is explained again "meno approfondito e solo per ciò che serve in quel
momento", and the user can always ask for more depth or a simpler version. The user's hypothesis
is that it is per person and not per project ("Docker lo sai anche nel progetto dopo"). A ledger
of what was explained is not a record of what was understood: people rate their understanding
higher than it is (specification 10.1, Rozenblit and Keil). The question this record answers is
what the ledger holds, who writes it, how it is used and where it may go. The design is in
`docs/specs/companion-explainer.md`, sections 5.7 and 5.8.

## Decision Drivers

- The tutor should not repeat what it already explained, and should say less about it.
- The ledger must be short and cheap to send (context saving).
- The companion is read-only and cannot write the database.
- A skipped explanation must stay visible and recoverable.
- It is personal data about what the user has seen: local, except for the companion's provider.

## Decision

Shinatama keeps one ledger per person, in the per-user database, outside every repository. An
entry is a concept title, a theme, up to four keywords, the highest level explained (1 to 4) and
two dates; it holds no explanation text and records what was explained, not what was understood.
The companion names what a card taught in the card block of the card, and **Shinatama's code
validates and stores it, only once the card has been in view**; the model never writes the
store, and the user presses nothing. A concept not in the ledger is taught from level 1. A
concept in the ledger that matters to the current turn gets only a reminder line of at most 20
words tied to what the turn does with it, and one that does not matter is not mentioned. Depth
comes only on request (deeper, simpler, basics, a concept chip); simpler and basics lower the
entry. An Aided Decision card always carries one line on every concept it names. The memory view
lists, lowers, deletes and forgets entries, and a setting turns the ledger off. The ledger is
sent only to the companion's provider, in the per-turn context block, as titles, themes,
keywords and levels, relevance first, at most 40 entries and 2 KB, never stored in the
companion's record. Names, themes and keywords written by the model are bounded and carried only
as JSON strings. The first time an entry is about to be written a one-time notice says where the
ledger goes. Concepts specific to one project are not stored.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the ledger holds Coolify at level 1 and reverse proxy at level 2 and the next turn touches both -> Expected: one reminder line of at most 20 words for each, tied to what the turn does with it, no basics; their chips remain; deeper on the Coolify chip explains level 2 (spec EX-015).
- **EX-002** (boundary case): Input: a Learning card is produced while the pane is collapsed -> Expected: the ledger gets no entry until the card has been in view (spec EX-025).
- **EX-003** (boundary case): Input: the worker proposes something involving a concept already in the ledger -> Expected: the Aided Decision card still carries one line on it (spec EX-016).
- **EX-004** (boundary case): Input: the ledger holds Docker at level 3 and the user presses basics on its chip -> Expected: Docker is explained from level 1 and the entry is set to 1; the memory view can delete it (spec EX-020).
- **EX-005** (rejection or error case): Input: the ledger table cannot be read, or the database opened read-only -> Expected: an unreadable table is treated as empty with a notice and is not overwritten; a read-only database writes nothing and turns automatic cards off (spec EX-021).
- **EX-006** (boundary case): Input: the first entry is about to be written -> Expected: a one-time notice says the relevant entries go to the companion's provider, with View and Turn off (spec EX-022).

## Rejected Alternatives

- **RA-001**: a memory set by the user's "got it" marks, which this record proposed first; rejected by the user (2026-11-01) in favour of a ledger the companion writes as it explains (spec RA-009).
- **RA-002**: a memory inferred from the user's behaviour (time spent, no follow-up questions); an inference silently removes explanations and cannot be seen or undone by the user (spec RA-009).
- **RA-003**: a memory per project; the user's hypothesis is per person, and a per-project one would explain the same basics again in every project.
- **RA-004**: letting the companion write the store itself; it would need a write channel and break the read-only session (ADR-0002, spec 6).

## Enforceable Invariants

- **INV-001**: a ledger entry is written only by Shinatama's code, from the validated card block of a card that has been in view, and is lowered only by an explicit user preset or the memory view; the model never writes the store and nothing infers what the user understood (spec INV-013).
- **INV-002**: the ledger is sent only to the companion's provider, in the per-turn context block, within 2 KB of relevance-first entries; never in a message to the worker, in a repository or in an export (spec INV-014).
- **INV-003**: every name, theme and keyword written by the model is bounded (name 60, theme 30, keyword 24 characters, 4 keywords) and carried only as a JSON string in a data wrapper (spec 5.8, INV-014).

## Consequences

- technical-design 4.1, 4.2 and 4.7 gain a ledger table in the per-user database, its read-only and unreadable behaviour, `forget` of the ledger and the privacy statement.
- The companion needs a per-turn context block rebuilt for every headless turn, not a stored brief update (spec 5.4).
- The ledger is out of the first release (spec 9.6, R3): until it exists every concept is new and cards carry chips.
- A user who did not retain what was explained is not warned by the ledger itself; the safeguards are the reminder line, the chips, basics, the memory view and the field check of spec 10.2 row 18; ageing is an open point (spec 9.5, point 7).

## Confirmation

- Row 15 of specification 10.2: write-on-view, preset levels, reminder lines, relevance-first selection, view, setting, notice and failure behaviour.
- Row 16: the card-block parser fixtures, including hostile names.
- Row 18: the user's own two-week field check.
