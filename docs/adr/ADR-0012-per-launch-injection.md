---
adr_id: ADR-0012
version: 1
title: Nothing system-level is touched; hooks, MCP and sets injected per launch; launch identity via SHINATAMA_* variables
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - per-launch-injection
    - instance-identity
    - mcp-injection
    - hooks
    - system-isolation
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

# ADR-0012: Nothing system-level is touched; hooks, MCP and sets injected per launch; launch identity via SHINATAMA_* variables

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. It must give every agent it starts its hooks, its MCP server, the
instance's set of plugins and skills, and an identity, without changing the user's configuration
or system state. The question this record answers is how those reach the agent and what
identifies a running instance.

Origin: functional-design 3 states, with no marker, "Nothing system-level is touched ... hooks
and the MCP server are injected per launch, never by editing global configuration";
technical-design 3 marks the isolation rule as the author's call [C]. The identity variables are
"[V for the failure; C for the fix]" (technical-design 3); the per-instance token is the author's
call (technical-design 7.5, register M15). "There is one launch point and it runs the checkup" is
the author's call ("the user did not object", functional-design 3). Per-launch MCP injection, one
proxy per instance, the `CODEX_HOME` overlay and the box are unproved (register M1, SC5, CX12,
D15 to D19). The full design is in technical-design 3 and 7.5; this record holds only the
decision.

## Decision Drivers

- The user's global configuration and system state stay untouched.
- An instance identity that survives `/resume`.
- Instances must not be able to act as each other.
- One place where every launch is prepared and checked.

## Decision

Every agent launch goes through one launch point, which runs the checkup. Nothing system-level is
touched: the sidecar keeps a private root and applies its environment and certificate authority
to the launched child only (ADR-0028), and Shinatama's hooks, its MCP server `shinatama mcp` and
the instance's set of plugins, skills and MCP servers arrive through per-launch switches:
`--settings`, `-c`, `OPENCODE_CONFIG*`, `-e`, DeepSeek Harness's `--patch` or its ACP session
(adapter operations `hooks()`, `mcp_inject()` and `instance_set()`). Hermes Agent has no
per-launch switch: its hook and MCP entries live in the box's own `config.yaml`, a file Shinatama
owns, and a Hermes Agent on the `system` home runs without them. At launch, after inherited agent
markers are removed (ADR-0013), the agent gets `SHINATAMA_INSTANCE`, `SHINATAMA_THREAD`,
`SHINATAMA_DB` and `SHINATAMA_INSTANCE_TOKEN`, a per-launch random token stored hashed in
`instance`. These are the stable identity of the instance; the agent's own session id is not used
for identity. `shinatama mcp` and `shinatama hook` refuse an unknown or ended instance and a caller
whose token does not match. See technical-design 3 and 7.5.

## Verified Interview Examples

- **EX-001** (nominal case): Input: per-launch MCP injection for every agent (Hermes Agent through the box's config, DeepSeek Harness through `--patch` or ACP) -> Expected: works on each without editing global configuration, coexists with the user's servers, and the server sees `SHINATAMA_INSTANCE`; otherwise the agent is marked "no MCP" (register M1).
- **EX-002** (rejection or error case): Input: a hosted agent's shell tool sets a peer's `SHINATAMA_INSTANCE`, with and without the token -> Expected: refused and logged; the agent's own calls still work; the token never appears in clear in the database, logs or a transcript (register M15).
- **EX-003** (boundary case): Input: an instance after `/resume`, `/clear` and compaction -> Expected: its identity is stable (register M9).
- **EX-004** (boundary case): Input: an environment dump from inside each agent's shell tool -> Expected: `SHINATAMA_*`, `TERM` and `COLORTERM` as specified; nothing leaks to the parent or siblings (register P15).
- **EX-005** (boundary case): Input: several instances starting at once, and an agent update while an instance runs -> Expected: a written checkup policy; a running agent is never updated (register D7).

## Rejected Alternatives

- **RA-001**: adding Shinatama's hooks or MCP server by editing the user's global configuration; never done, so that system state stays untouched (functional-design 3; technical-design 3, 7.5).
- **RA-002**: the agent's own session id as the instance's identity; it "changes on `/resume`, which already broke the bili MCP shim" (technical-design 3).
- **RA-003**: a Hermes Agent profile or a per-launch `HERMES_HOME` to carry the entries; "the only levers are `HERMES_HOME` and the profile, and both also switch the session store" (technical-design 5.7).

## Enforceable Invariants

- **INV-001**: no launch edits the user's global agent configuration to add Shinatama's hooks, MCP server or an instance set.
- **INV-002**: every agent launch passes through the single launch point and its checkup.
- **INV-003**: the four `SHINATAMA_*` variables are set for the child only, after inherited markers are removed.
- **INV-004**: instance identity never derives from the agent's own session id.
- **INV-005**: `SHINATAMA_INSTANCE_TOKEN` is stored only hashed and never appears in clear in the database, logs or transcripts (technical-design I5; register M15).
- **INV-006**: `shinatama mcp` and `shinatama hook` refuse an unknown or ended instance and a mismatched token.
- **INV-007**: a Hermes Agent on the `system` home gets no Shinatama hook or MCP entry.

## Consequences

- The token limits impersonation between instances; it is not isolation from an agent with a shell (technical-design 7.5, 7.9).
- DeepSeek Harness drops variables matching `TOKEN` from its stdio MCP servers' environment, so the token must reach `shinatama mcp` another way (register DS5).
- Some launches still write to an agent home: Codex set plugins are pre-installed there turned off (a home write, author's call); Claude Code rewrites bookkeeping such as `pluginUsage` in `.claude.json` (record D19); with the `system` home, Daodan is installed into the agent's native plugin store, so the user's own runs see it (functional-design 3; ADR-0011).
- The sidecar's session id stays separate from `SHINATAMA_INSTANCE`; the bili MCP shim and `shinatama mcp` coexist in one launch (technical-design 9).
- Adapter operations `hooks()`, `mcp_inject()` and `instance_set()` (ADR-0010); inherited-marker removal is ADR-0013; messaging over `shinatama mcp` is ADR-0023.

## Confirmation

- Register M1 (G0, first-milestone set), M15 (G0), M9, P15, D7, HM5, DS5, SC5 and CX12 pass with records, with technical-design 10 items 5 and 10 (per-launch hooks and MCP for each agent); D19 passes in interactive mode.
- Only D19 has run: per-instance sets in print or exec mode on Windows, 2026-10-28, running (partial). Nothing else here has run.
