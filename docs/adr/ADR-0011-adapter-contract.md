---
adr_id: ADR-0011
version: 1
title: One adapter per agent behind a contract with declared capabilities and a conformance suite
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - adapter-contract
    - capabilities
    - conformance-suite
    - command-line
governs_symbols:
depends_on:
  - ADR-0010
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

# ADR-0011: One adapter per agent behind a contract with declared capabilities and a conformance suite

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. Each hosted agent has its own store, flags, signals and version
drift. The question this record answers is how the engine talks to six different agents without
depending on any of them, and when an agent's integration counts as acceptable.

Origin: "One adapter per agent (decided), so Grok or another agent can be added without touching
the rest" (functional-design 6), and the user's requirement, marked decided: "every supported
agent has an adapter with export, import, resume and handover functions, and the rest of what is
needed" (technical-design 5.5). The contract's design is the builder's and its requirements the
author's (functional-design 6 heading); the operation list is a language-neutral sketch for a
Rust trait. The command names (functional-design 4) and the exit codes (technical-design 5.5.2,
[C]) are the author's calls, and functional-design 8 leaves both to the builder. The conformance
suite is untagged; how adapters are packaged is the builder's call. The full design is in technical-design 5.5, 5.5.2 and 5.5.3; this record
holds only the decision.

## Decision Drivers

- The engine must not depend on a particular agent.
- Capability differences must be visible to the user.
- Adding Grok, Gemini beyond launch-only or a future agent must be cheap.
- Each claim about an agent is tied to fixtures of its version (ADR-0010).

## Decision

The engine depends only on a per-agent adapter contract. Each adapter declares its capabilities,
shown to the user and never hidden, and implements seven groups of operations: A identity and
environment (`placement`, `detect`, `instance_set`, `fixture_gate`, `inherited_markers`,
`auth_status`, `login`, `install_or_update`, `install_daodan`, `system_homes`); B read and export
(`list_sessions`, bounded `read_transcript`, `export`, `export_native`); C import, post-v1 only
(`import_support`, `import`, `verify_import`); D run (`start_new`, `resume`, optional `fork` and
`attach_backend`); E handover (`handover_out`, `handover_in`, `confirm_receipt`, `catch_up`,
`release`); F signals (`state_events`, `queue_message`, `ask_headless`); G injection and
capabilities (`hooks`, `mcp_inject`, `capabilities()`). Anything not implemented is declared
`unsupported`, not faked. The command line exposes the same operations (`sessions`, `find`,
`export`, `import`, `resume`, `hop`, `doctor`) with a stable `--json` schema and exit codes 0 done,
2 refused with a reason, 3 needs the user, 4 failed and nothing changed, 5 failed and recovery
needed. An adapter is acceptable only if it passes a contract suite on recorded fixtures per agent
version, structure-only and synthetic; one that fails for the installed version drops to
`Degraded` or `Unsupported` and the UI says so. Adding an agent means implementing the contract,
recording fixtures and passing the suite. The engine compiles and passes its tests with no
adapter at all and with a fake in-memory adapter. See technical-design 5.5.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the canary hop, a made-up codename stated once in turn 3 of the source, run against every other adapter in both directions -> Expected: the destination answers it without the codename being repeated, for every ordered pair at stage 1 (technical-design 5.5.3, 11.2).
- **EX-002** (boundary case): Input: an adapter whose agent supports import (post-v1) -> Expected: `read_transcript(import(export(x)))` has the same `user_text`, `assistant_text` and `compaction` items in order and the same tool items as digests; what the agent cannot represent is listed in the report (technical-design 5.5.1).
- **EX-003** (rejection or error case): Input: an operation the adapter does not declare -> Expected: refused; every declared capability is exercised by the suite (technical-design 5.5.3).
- **EX-004** (rejection or error case): Input: an agent whose installed version fails its fixtures -> Expected: it drops to launch-only with a visible reason instead of a wrong hop (technical-design 11.10).
- **EX-005** (nominal case): Input: `shinatama find "refresh token" --json | jq -r '.[0].id'`, then `shinatama resume t_8f3a --agent codex` -> Expected: a thread id from the text index, then thread `t_8f3a` resumed in Codex, or hopped there if its agent differs (functional-design 5.3; technical-design 5.5.2).

## Rejected Alternatives

- **RA-001**: faking a capability an agent lacks; it is declared `unsupported` and shown, because "Capability differences per agent are declared and shown to the user, never hidden" (functional-design 6; technical-design 5.5).
- **RA-002**: engine code that depends on a particular agent; one adapter per agent exists "so Grok or another agent can be added without touching the rest" (functional-design 6).

## Enforceable Invariants

- **INV-001**: the engine compiles and passes its tests with no adapter and with a fake in-memory adapter; adding an agent changes neither the engine, the database, the UI nor the roundtable.
- **INV-002**: every capability an adapter does not implement is declared `unsupported` and shown; the suite refuses every undeclared one.
- **INV-003**: `auth_status()` asks the agent and never reads credentials (technical-design I5).
- **INV-004**: `resume` is native and same-agent only; `fork` creates a new native id and never resumes a live session; `release` never touches its native session (technical-design I2).
- **INV-005**: in v1, `handover_in` uses only `native_resume` or `seed_prompt`, and group C is unavailable (technical-design I7).
- **INV-006**: suite fixtures are structure-only and synthetic, never the user's real conversations.
- **INV-007**: `list_sessions` never reads a whole session per row, and unknown metadata stays unknown.
- **INV-008**: `--json` output keeps a stable schema and the exit codes above.

## Consequences

- The first milestone must establish per agent: starting with an initial prompt and its size limit, choosing or learning the session id, the exact resume command, per-launch hooks and MCP, a non-interactive mode with a clear turn end, and an answer-only policy (technical-design 5.5; 10 item 2).
- Readers delegate parsing to Mukade (ADR-0020); `.stx.jsonl` is ADR-0021; placement and launch operations follow ADR-0012 and ADR-0013.
- Post-v1 import differs per agent: Shinatama's own writers for Claude Code, Codex, OpenCode and Pi; Hermes Agent's official import for Claude Code and Codex sources only; none for DeepSeek Harness.
- The degraded outcome is worded three ways: `Degraded` or `Unsupported` (technical-design 5.5.3), "launch-only plus the agent's own export command" (technical-design 5.2), and "launch-only" (technical-design 11.10). The builder must reconcile them.
- Open: register O7, whether the hop dialog, roundtable and palette offer only actions both adapters' `capabilities()` support.

## Confirmation

- Each adapter's contract suite passes on recorded fixtures for its installed version; acceptance 11.2 and 11.10 pass.
- The per-agent first-milestone rows (CL1, CL2, CL4, CX1, CX2, CX4, OC1, PI1, PI2, PI11, HM1, HM2, DS1, DS2) record the facts the adapters need. CX4 passed on 2026-10-27 and PI11 is running (partial); the rest are open. No adapter or suite exists yet.
