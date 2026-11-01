---
adr_id: ADR-0025
version: 1
title: Keep four plain MCP tools in v1; Code Mode only after measurement
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/specs/token-savings.md
  modules:
  domains:
    - code-mode
    - mcp-server
    - token-economy
    - tool-definitions
governs_symbols:
depends_on:
  - ADR-0024
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0025: Keep four plain MCP tools in v1; Code Mode only after measurement

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user asked to remember Cloudflare's Code Mode to save tokens:
instead of many MCP tools, the model gets one tool that runs code it writes against a typed API.
Technical-design 7.5a records this as "decided as a reminder; scope is mine": the reminder is
the user's, while the reading of what Code Mode means for Shinatama and the scope below are the
author's calls, not confirmed, and the typed-API design is tagged [C]. The question is whether
the agent-facing MCP surface of v1 should be a code-execution tool instead of plain tools, and
how the decision is taken. The full analysis is in technical-design 7.5a; this record holds only
the decision.

## Decision Drivers

- Tool definitions cost context in every agent.
- `shinatama mcp` has only four tools; the machinery that would replace them has its own cost.
- Agent-written code that runs under the influence of a peer message is a new attack surface (technical-design 7.6).
- Cloudflare's post gives no figures; the reported figure is secondhand.
- No saving may be claimed before it is measured on Shinatama's own sessions.

## Decision

v1 keeps the four plain tools of `shinatama mcp` (ADR-0024). The four operations are specified as
a typed API (`instances()`, `send()`, `inbox()`, `ack()`), and the server's tool layer is
separate from them. A second surface, `shinatama_search(query)` for the declarations and
`shinatama_execute(code)` running agent-written script in a sandbox with only these bindings,
can then be added without changing the engine; it would keep the same authorization and rate
limits (technical-design 7.2, 7.4), have no file, network or process access, and run under a
time and memory cap. The embedded script engine is the builder's choice.

A code-mode gateway in front of the MCP servers a Shinatama session already carries (Daodan's
`moe`, `peer-review`, `work-controller`, `codebase-map`, Playwright, the bili tools) is not in v1;
it is an evaluated option. The decision is taken with Shinatama's own numbers: milestone 1
measures, per agent, the tokens that the tool definitions of a typical session cost, and no
saving percentage is claimed before that. See technical-design 7.5a and 10 item 8.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the first request of a typical session, per agent, captured through the sidecar -> Expected: the tokens of tool definitions (Daodan plugins' MCP servers, bili, `shinatama mcp`) recorded per agent (register T1; technical-design 10 item 8).
- **EX-002** (nominal case): Input: the four `shinatama mcp` tool definitions in that first request -> Expected: their token cost recorded per agent (register M3).
- **EX-003** (boundary case): Input: T1 shows a large footprint -> Expected: a prototype gateway in front of the real MCP servers is measured for tokens saved, task success, latency, sandbox security, engine size and licence, and is built only if the saving beats a threshold the user sets and task success does not fall (register T3).
- **EX-004** (rejection or error case): Input: the secondhand report that Cloudflare's server costs about 1,000 tokens instead of more than 1.17 million -> Expected: not used as evidence; the claims are restated with primary citations (register T2; technical-design 7.5a).

## Rejected Alternatives

- **RA-001**: Code Mode for `shinatama mcp` in v1; for four tools the saving is small, and they are cheaper than the machinery that would replace them.
- **RA-002**: a code-mode gateway in v1; it is a large feature with its own security model, kept as an evaluated option.
- **RA-003**: deciding on Cloudflare's numbers; the post gives no figures, the reported figure is secondhand, and only Shinatama's own measurement counts.

## Enforceable Invariants

- **INV-001**: in v1, `shinatama mcp` exposes exactly `shinatama_instances`, `shinatama_send`, `shinatama_inbox` and `shinatama_ack`.
- **INV-002**: the four operations are a typed API independent of the MCP tool layer; adding a second surface changes no engine code.
- **INV-003**: any future `shinatama_execute` enforces the scope and rate limits of technical-design 7.2 and 7.4, has no file, network or process access and runs under a time and memory cap.
- **INV-004**: no token-saving percentage for Code Mode or a gateway is claimed before register T1, and for a gateway T3, has measured it.

## Consequences

- Milestone 1 carries the measurement (technical-design 10 item 8; register T1, M3).
- Code Mode cuts tool definitions and intermediate results; the sidecar's CCR and Absorb cut tool results. The two are complementary, not replacements (technical-design 7.5a).
- The token-savings spec keeps the gateway as a later uniform piece, the last step of its order of work (token-savings 4.2, 7, 8.1; ADR-0038).
- Adding the second surface later needs an embedded script engine in Rust and a security review of agent-written code that may follow a peer message.
- Open: the threshold of T3 is the user's and is not set.

## Confirmation

- Register T1, T2 and M3 pass; they are open. T3 runs only if T1 shows a large footprint.
- Nothing has been run yet.
