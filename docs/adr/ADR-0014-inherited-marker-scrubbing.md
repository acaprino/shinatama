---
adr_id: ADR-0014
version: 1
title: Remove the union of inherited agent markers by exact name before every launch
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - inherited-markers
    - launch-environment
    - agent-launch
    - adapter-identity
governs_symbols:
depends_on:
  - ADR-0011
  - ADR-0013
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

# ADR-0014: Remove the union of inherited agent markers by exact name before every launch

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama can itself be started from inside an agent: a terminal opened by Claude Code, an
agent's shell tool, or the command line mode called by a hosted agent. It then inherits that
agent's environment markers and would pass them to every agent it launches. The P20 run of
2026-10-28 showed that an interactive Claude Code 2.1.312 that inherits
`CLAUDE_CODE_CHILD_SESSION` saves no transcript and no history.

Origin: technical-design 3 tags the rule "[C; decided by the user 2026-10-28]", and the
harness/cli survey note (`docs/notes/2026-10-28-harness-cli-survey.md`, section 4 item 4) says
"Adopted 2026-10-28". Functional-design 7 lists it among the "calls made to settle the rest
(check them)". The per-agent name lists come from the P20 record,
which is still running (partial). The full rule and evidence are in technical-design 3 and the
P20 record; this record holds only the decision.

## Decision Drivers

- A hosted Claude that inherits the marker cannot be resumed and is invisible to search (P20).
- A hosted Claude that inherited the markers "would take itself for the outer Claude's child and
  could reach its socket" (technical-design 3).
- Codex 0.164.0, OpenCode 2.1.4 and Pi 1.4.0 pass every inherited marker on to their children;
  OpenCode connects its TUI to an inherited `CLAUDE_CODE_SSE_PORT` (read from strings, not run).
- The user's own settings, such as `CLAUDE_CONFIG_DIR` and `CLAUDE_CODE_USE_BEDROCK`, must pass
  through.

## Decision

Before launching any agent, Shinatama removes from the child's environment the **union** of
every adapter's marker list, so the agent starts as it would from a fresh terminal. The
`SHINATAMA_*` variables are set after the removal. Each adapter declares its markers through
`inherited_markers()` (technical-design 5.5 A) as **exact names, never prefixes**. Two names are
conditional: `GIT_EDITOR` is removed only when its value is Claude's helper, and Codex's generic
sandbox names (`NPM_CONFIG_OFFLINE`, `PIP_NO_INDEX` and the others) only when `SBX_NONET_ACTIVE`
is present. Values are not read otherwise. The current lists per adapter are the table in the
P20 record; Hermes Agent and DeepSeek Harness add theirs through HM11 and DS11. Pi's
`<agentDir>\bin` prepended to `PATH` is not covered by names. See technical-design 3.

## Verified Interview Examples

- **EX-001** (rejection or error case): Input: an interactive Claude Code 2.1.312 started with an inherited `CLAUDE_CODE_CHILD_SESSION` and no removal -> Expected (observed): the turn reaches the model, but no transcript and no `history.jsonl` are written ("Transcript saving is off — inherited CLAUDE_CODE_CHILD_SESSION marker"); the clean run writes both. This is the failure the rule prevents (P20 record).
- **EX-002** (boundary case): Input: the environment carries `CLAUDE_CONFIG_DIR` or `CLAUDE_CODE_USE_BEDROCK` -> Expected: both reach the agent unchanged, because removal is by exact name and never by the `CLAUDE_CODE_*` prefix (technical-design 3; harness/cli survey, section 4 item 4).
- **EX-003** (boundary case): Input: `GIT_EDITOR` is set -> Expected: removed only when its value is Claude's own editor helper, kept when the user set it (P20 record, Criterion).
- **EX-004** (boundary case): Input: `NPM_CONFIG_OFFLINE` or `PIP_NO_INDEX` is present -> Expected: removed only when `SBX_NONET_ACTIVE` shows they came from a Codex sandbox, kept otherwise (P20 record, Criterion).

## Rejected Alternatives

- **RA-001**: removal by prefix (`CLAUDE_CODE_*`); it "would also delete user settings such as `CLAUDE_CODE_USE_BEDROCK`" (harness/cli survey, section 4 item 4).

## Enforceable Invariants

- **INV-001**: before every agent launch, no name in the union of all adapters' `inherited_markers()` lists is present in the child's environment, except the two conditional cases of INV-003.
- **INV-002**: removal matches whole names only; no variable is removed because of a prefix, and no user setting is removed.
- **INV-003**: `GIT_EDITOR` is removed only when its value is Claude's helper; Codex's generic sandbox names only when `SBX_NONET_ACTIVE` is present.
- **INV-004**: the `SHINATAMA_*` variables are set after the removal, so they are never removed.
- **INV-005**: every adapter, including one added later, contributes its list to the union (HM11 and DS11: "List added to the union").

## Consequences

- Adapter operation `inherited_markers() -> [exact env names]` in technical-design 5.5 A; its
  lists are maintained per agent version from the P20 table.
- Not covered: Pi's `<agentDir>\bin` prepended to `PATH` "cannot be removed by name". No rule for
  it is recorded.
- Post-v1 native handover must confirm that Mukade-launched children receive the same marker
  removal before that path is enabled (mukade-handover 3; ADR-0022).
- Open: P20 ran on Windows only, with a mock model in print or exec mode. A real nested
  interactive start for Codex, OpenCode and Pi, and macOS and Linux, are not run (register 20,
  next run 1). HM11 and DS11 are open.
- Functional-design 7 and technical-design 3 label the origin differently; approving this record
  settles it.

## Confirmation

- Register P20 passes: "Exact list per agent written into its adapter; with the removal,
  behaviour equals a fresh terminal; no user setting removed." Status today: running (partial).
- Register HM11 and DS11 pass for Hermes Agent and DeepSeek Harness.
- Nothing has run against Shinatama itself; there is no code yet.
