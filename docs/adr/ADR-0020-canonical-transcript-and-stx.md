---
adr_id: ADR-0020
version: 1
title: Canonical transcript, reader rules, and `.stx.jsonl` as Shinatama's own export format
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - canonical-transcript
    - reader-rules
    - stx-format
    - session-export
governs_symbols:
depends_on:
  - ADR-0019
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

# ADR-0020: Canonical transcript, reader rules, and `.stx.jsonl` as Shinatama's own export format

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. Six agents keep sessions in six different shapes, while the seed, the
index, the companion feed and export need one form. Export must also let a thread be backed up,
kept after the agent deletes it (Claude Code does after 30 days) and moved between machines. The
question this record answers is which intermediate form Shinatama uses, how readers behave, and
what Shinatama's own export file is.

Origin: the item kinds of technical-design 5.1 and most reader rules of 5.2 are untagged
proposed shape; branch selection is "[C]"; the `.stx.jsonl` format of 5.5.1 is "[C]" (author's
call, not confirmed). The reading rule for OpenCode is documented and was run in OC2; for Hermes
Agent it is from research ("[R]"). The export choices STX, portable Mukade JSON and native belong
to the sessions surface (functional-design 5.16, "decided 2026-10-30; revised 2026-10-31").
Technical-design 5.1 to 5.3 and 5.5.1 to 5.5.3 hold the full design; this record holds only the
decision.

## Decision Drivers

- One form for every consumer, with provenance back to the native record.
- Readers must never disturb a running agent.
- A new agent version must degrade visibly, not produce a wrong hop.
- An export file that survives the agent and can be shared without secrets.

## Decision

A transcript is a list of turns; a turn is a list of items. Kinds and transferability:
`user_text` yes; `assistant_text` yes; `reasoning` **never**; `tool_call` and `tool_result` as
text; `attachment` as a path reference; `compaction` yes, marked; `meta` dropped or noted. Every
item keeps its provenance: agent, native session id, native entry id or ordinal, timestamp, and
byte range or row (technical-design 5.1). Adapters produce it by projecting Mukade's `Session`
(ADR-0019).

Reader rules (5.2): read-only and never take a lock the agent could wait on; SQLite `mode=ro`,
never `immutable` while the agent may write; an incomplete last JSONL line is ignored until the
next scan; unknown record types are skipped and counted, and `doctor` shows the count; a new
major version that fails its fixtures degrades the agent to launch-only plus its own export
command, and says so; trees are followed from the root to one leaf (the one the agent names,
else the most recent), other branches indexed but not carried; sub-agent entries become one
`tool_call`/`tool_result` pair. For OpenCode and Hermes Agent the agent's own export is the
primary source and SQLite only a read-only fallback for listing and search (5.3).

`.stx.jsonl` version 1 (5.5.1) is Shinatama's export file: a header line, then one line per item
with provenance. Readers ignore and keep unknown fields; a higher `version` is refused with a
message. Two modes, `full` and `portable`; secrets are redacted in both. `reasoning` is never
exported; attachments are hashed references, copied only with `--with-attachments`. It is
distinct from Mukade's interchange JSON: neither is parsed into the other, and neither parses
native stores. `shinatama export --format stx|mukade|native|md` offers all four (5.5.2).

## Verified Interview Examples

- **EX-001** (nominal case): Input: `export` then `import` then `read_transcript` of a transcript `x`, per adapter -> Expected: the same `user_text`, `assistant_text` and `compaction` items in order and the same tool items as digests; what the agent cannot represent is listed in the report (technical-design 5.5.1, round-trip law).
- **EX-002** (rejection or error case): Input: an `.stx.jsonl` header with a higher `version` than the reader knows -> Expected: refused with a message, not guessed at (technical-design 5.5.1).
- **EX-003** (boundary case): Input: a JSONL session whose last line is incomplete because the agent is writing -> Expected: the line is ignored until the next scan (technical-design 5.2); for DeepSeek Harness a torn last Zstandard frame is ignored the same way (register DS3).
- **EX-004** (boundary case): Input: synthetic branched sessions with rewinds and forks -> Expected: the reader follows the same path as the agent's own resume (register H15).
- **EX-005** (rejection or error case): Input: an agent whose installed version fails its fixtures -> Expected: it drops to launch-only with a visible reason instead of a wrong hop (technical-design 11.10).

## Rejected Alternatives

- **RA-001**: OpenCode's or Hermes Agent's SQLite file as the primary read source; OpenCode's "schema changes between versions and the legacy and current tables disagree", and Hermes's "schema is versioned and moves" (technical-design 5.3).
- **RA-002**: guessing at an `.stx.jsonl` version higher than the reader knows (technical-design 5.5.1).
- **RA-003**: treating `.stx.jsonl` and Mukade's interchange JSON as one format; "no direct compatibility or automatic redaction between them is assumed" (mukade-handover 1; technical-design 5.5.1).
- **RA-004**: DeepSeek Harness's web UI `/export` zip as a read source; "not used" (technical-design 5.3), with no further reason given.

## Enforceable Invariants

- **INV-001**: no `reasoning` item is transferred, seeded or exported.
- **INV-002**: every canonical item carries its provenance.
- **INV-003**: a reader takes no lock the agent could wait on, opens SQLite `mode=ro` and never `immutable` while the agent may write, and never writes a native store (technical-design I7).
- **INV-004**: unknown record types are counted, not silently dropped; `doctor` reports the count.
- **INV-005**: both STX modes redact secrets before writing (technical-design I5).
- **INV-006**: STX and Mukade interchange JSON are never parsed into each other.

## Consequences

- Block names and several record shapes in technical-design 5.3 are "[U]"; the reader rows (CL6,
  CX10, OC3, PI3, HM3, DS3) capture them first.
- Importing an STX file into an agent writes a native session, so it is post-v1 (technical-design
  5.5 C; ADR-0015).
- The conformance suite of technical-design 5.5.3 checks `read → export → read` stability and the
  round-trip law where import is supported.
- The sessions surface must declare format version, losses and redaction for each export choice;
  portable Mukade JSON is not presented as a lossless backup (functional-design 5.16).
- The seed's hand-off marker is read back as a `meta` item (ADR-0018).

## Confirmation

- The 5.5.3 conformance suite passes per adapter and agent version.
- Register CL6, CX10, OC3, PI3, HM3, DS3, H15, H16 and SS6 pass; acceptance 11.10 passes.
- OC2 (passed) showed OpenCode's own export and import round trip losslessly for the kinds tested;
  no STX round trip has been run, and there is no code yet.
