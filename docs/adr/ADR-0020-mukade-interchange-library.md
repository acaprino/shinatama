---
adr_id: ADR-0020
version: 1
title: Mukade, Shinatama's own interchange library, linked in-process at a reviewed pin; adapters delegate parsing; no third-party converter
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/mukade-handover.md
    - docs/functional-design.md
  modules:
  domains:
    - mukade
    - session-readers
    - interchange-library
    - dependency-pin
    - store-discovery
governs_symbols:
depends_on:
  - ADR-0011
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

# ADR-0020: Mukade, Shinatama's own interchange library, linked in-process at a reviewed pin; adapters delegate parsing; no third-party converter

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet (`Cargo.toml` workspace with no members). Search, the sessions surface,
export and hops all need to read each harness's native session store, and post-v1 native import
needs to write them. The landscape research and the JanuraCode survey found no cross-harness
session translator; the UniSessions trial (UNI1) found one that does not work for OpenCode 2.1.4.
The question this record answers is who reads and writes native sessions and how Shinatama
depends on it.

Origin: "No third-party session converter is used (the user's decision of 2026-10-27; it
replaces the earlier plan to link an external converter)" (technical-design 5.4.1; register H27).
Delegating the readers with no second parse is an author's call ("[C]", technical-design 5,
2026-10-31); the reviewed Git pin and the ownership split are the proposed shape of
`docs/mukade-handover.md` ("not implemented or approved"); reuse of Mukade's catalog and
installation scan is "my call" (functional-design 5.14, 5.16). Technical-design 5 and 5.4.1 and
mukade-handover hold the full design; this record holds only the decision.

## Decision Drivers

- An own library: "own library" is the user's stated reason (register H27).
- One parser and one recognition rule set per harness, not two.
- A dependency whose behaviour is fixed and reviewed, not a moving branch.

## Decision

Native session reading and, post-v1, writing for the hosted harnesses are delegated to `mukade`
(Rust, MIT, `acaprino/mukade`), Shinatama's own library, linked in-process. It is declared as a
Git dependency in `[workspace.dependencies]` when the first runtime crate exists, pinned to a
reviewed commit with the lockfile committed (mukade-handover 1 gives `4124b1f` as the example).
The declared pin, not a local checkout, defines what Shinatama can do; later revisions count only
once the pin is advanced and mukade-handover sections 1 to 7 are rechecked. In v1 only Mukade's
readers and read-side APIs are used, through the adapters; `hop()` and the writers are post-v1.

The adapter owns store resolution and capabilities and delegates parsing: `list_sessions` and
`read_transcript` call Mukade and project its `Session` onto the canonical transcript; adapters
do not parse native stores a second time. The one exception is a bounded incremental reader,
which Shinatama may own until a reviewed Mukade revision exposes one, and must then replace.
Discovery may use Mukade's harness catalog and installation scan, one recognition rule set;
which root becomes a box or a read-only system root is Shinatama's decision, recorded in its
catalog. The engine owns the hop state machine, close/fork and source activation; the adapter
owns capabilities, instance configuration and launch; Mukade owns conversion and the new
destination session (mukade-handover 2). Hermes Agent and DeepSeek Harness have no Mukade reader
yet: until a reviewed pin carries one, their adapters declare `list_sessions`, `read_transcript`
and `export` unsupported; they resume natively and receive seeds, but are not searched and are
not hop sources.

## Verified Interview Examples

- **EX-001** (rejection or error case): Input: UniSessions' `claude-to-opencode --write` output imported into OpenCode 2.1.4 -> Expected (observed): 400 "Missing key at [\"info\"][\"cost\"]", then `messages[0]` refused; a v2 export read back as nearly empty; two `--write` runs give two different `ses_` ids (UNI1 record). Not adopted as the converter.
- **EX-002** (boundary case): Input: a hop from Hermes Agent before a reviewed pin carries its reader -> Expected: not offered as a hop source and not searched; Hermes still resumes natively and receives seeds; its acceptance pairs count from the day Mukade reads it (technical-design 5, 11.2).
- **EX-003** (nominal case): Input: a synthetic Claude Code session with each feature, read through the adapter at the declared pin -> Expected: every item maps to the canonical form; unknown types counted (register CL6, run against Mukade's fixtures at the pin).
- **EX-004** (nominal case): Input: the sessions tree over boxed and read-only system roots -> Expected: correct list and provenance from the declared pin; no write, ambient fallback or invented metadata (register SS1).

## Rejected Alternatives

- **RA-001**: linking an external converter, the earlier plan and the converter used as a probe in H27; replaced by the user's decision of 2026-10-27 for an own library. The documents give no reason beyond "own library".
- **RA-002**: UniSessions as the converter; its OpenCode legs target the v1 export shape (2.1.4 rejects them) and each `--write` mints a new id, so re-runs duplicate. Kept as a cross-check (technical-design 5.4; UNI1).
- **RA-003**: a second, Shinatama-side parser or recognition rule set; "one recognition rule set and no second parser" (functional-design 5.16), except the temporary bounded reader.
- **RA-004**: tracking a moving branch; the pin must be a reviewed commit, not "silently tracking a moving branch" (mukade-handover 1).

## Enforceable Invariants

- **INV-001**: no Shinatama code parses a native store format that the declared Mukade pin reads, except a bounded incremental reader until a reviewed Mukade revision exposes one.
- **INV-002**: the Mukade dependency is one reviewed commit with a committed lockfile; capabilities are those of that pin.
- **INV-003**: in v1 Shinatama calls no Mukade write API (technical-design I7).
- **INV-004**: an adapter whose harness the pin does not read declares `list_sessions`, `read_transcript` and `export` unsupported.
- **INV-005**: no third-party session converter is a build or runtime dependency.

## Consequences

- Reader rows CL6, CX10, OC3, PI3, HM3, DS3 and H15 run against Mukade's fixtures at the pin.
- The pin gives listing and whole-session reads, not bounded paging: until then the adapter must
  apply a pre-read size cap and return `too large` (functional-design 5.16).
- Hermes Agent and DeepSeek Harness readers are Mukade work (mukade-handover 9); the order is
  Mukade's call, acceptance is per harness.
- Updating the pin requires rechecking the API, harness versions and acceptance evidence. Post-pin
  revisions (installation scan, `hop_into`) wait on that (register 20, next run 8).
- `verify_import`, interactive resume, the seed fallback, the nonce and the state machine stay
  Shinatama's (technical-design 5.4.1). No Shinatama runtime integration exists yet.

## Confirmation

- Register H27 (status changed: a probe with an external converter, 36/36 written and resumed;
  it does not validate the in-process integration).
- Register CL6, CX10, OC3, PI3, HM3, DS3, H15 and SS1 pass at the declared pin.
- Nothing has been run against a Shinatama integration.
