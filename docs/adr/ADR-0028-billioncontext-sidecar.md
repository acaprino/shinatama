---
adr_id: ADR-0028
version: 1
title: The unmodified BillionContext sidecar, single source, carries all agent traffic with its full resilience and security
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - billioncontext-sidecar
    - child-only-isolation
    - compression
    - provider-traffic
governs_symbols:
depends_on:
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

# ADR-0028: The unmodified BillionContext sidecar, single source, carries all agent traffic with its full resilience and security

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user decided that the BillionContext sidecar comes integrated
with "same resilience, security, compression" (functional-design goal 6 and 5.6). Functional-design
3 makes the source in `sidecar/source-build` the single source, which Shinatama does not fork,
and rules out the precompiled npm package. How that source, kept today in another repository, is
brought in (a copy with provenance, a submodule or a published package) is the builder's call,
as long as it stays a single source (functional-design 7 item 3). Gemini's launch-only coverage
is the author's call (functional-design 5.9). The sidecar's own documents (`docs/sidecar.md`,
`docs/token-efficiency.md`, `docs/prompt-rewrite.md`) live in its source tree, not here. Who
supervises the proxy is a separate record (ADR-0029). The full design is in functional-design 5.6
and technical-design 9; this record holds only the decision.

## Decision Drivers

- A Shinatama session is the full working setup, with the sidecar's resilience, safety and compression on every launch (functional-design 1).
- Nothing system-level is touched: environment and certificate trust stay with the launched child.
- No credentials in Shinatama's records or the sidecar's launch manifest (technical-design I5).
- One source of sidecar code, so its behaviour is the shipped behaviour.
- A hop must not trigger a compression pass on the destination's first turn.

## Decision

Shinatama runs the same BillionContext code, supervised by Shinatama, from the single source in
`sidecar/source-build`: not forked, not the precompiled npm package, and with no auto-update; the
owned source upgrades only through Shinatama. Each item below is acceptance, verified under
Shinatama and not assumed: a private root under which its configuration, data, cache and state
paths resolve; its environment applied to the child process only; its certificate authority
trusted only by the launched process, with no system trust-store change; a launch manifest with
argv and launch overrides only, never credentials; automatic proxy recovery on the original
address for active sessions and an orderly drain on exit; and the compression policy as shipped
(adaptive pressure, CCR, Absorb, profiles), with `--no-context` as an explicit per-session
opt-out.

Every instance's traffic goes through the proxy, and so do title calls to a provider and the
roundtable's bare-model calls. Seeds and catch-up messages are ordinary user turns for it, and the seed budget
stays well below the sidecar's adaptive threshold (`maxTokens: 160000` in the shipped policy) so a
hop does not trigger compression on the destination's first turn. The bili MCP shim and
`shinatama mcp` coexist in the same launch. v1 covers Claude Code, Codex, OpenCode, Pi, Hermes
Agent and DeepSeek Harness, with Gemini launch-only; Hermes Agent and DeepSeek Harness count as
covered only once the proxy carries their provider traffic. No measured compression saving is
claimed. See functional-design 5.6 and technical-design 9.

## Verified Interview Examples

- **EX-001** (nominal case): Input: an agent launched through the sidecar; its parent, siblings and shell inspected -> Expected: no leakage of the sidecar's environment (register SC2).
- **EX-002** (boundary case): Input: the system trust store listed before and after a launch, per agent runtime (native binary, Rust, Bun, Node) and per OS -> Expected: each agent's TLS to the proxy works and the store is unchanged (register SC3).
- **EX-003** (rejection or error case): Input: the launch manifest on disk -> Expected: argv and overrides only; no credentials found (register SC4).
- **EX-004** (boundary case): Input: a seed, bootstrap and receipt sent through the sidecar -> Expected: the bytes sent and received are identical, not altered by the prompt rewriter or externalized by CCR, and the first destination turn triggers no compression pass; otherwise a bypass marker is added (register SC9).
- **EX-005** (rejection or error case): Input: a Hermes Agent provider whose traffic does not pass through the proxy -> Expected: that provider is marked uncovered (register HM12; DS12 likewise for DeepSeek Harness).

## Rejected Alternatives

- **RA-001**: forking the sidecar; the source must stay a single source (functional-design 3, 7).
- **RA-002**: installing the precompiled npm package; ruled out in functional-design 3 with a reference to the sidecar's `docs/sidecar.md`, with no further reason in this repository.
- **RA-003**: sidecar auto-update; the owned source upgrades only through Shinatama.
- **RA-004**: a weakened integration that loses the private root, child-only environment, child-only certificate trust or proxy recovery; it is an anti-goal, a failure "even if it technically works" (functional-design 2).

## Enforceable Invariants

- **INV-001**: sidecar code comes from one source, unmodified: no fork, no npm package, no auto-update.
- **INV-002**: the sidecar's configuration, data, cache and state paths resolve under its private root.
- **INV-003**: the sidecar's environment and certificate authority reach only the launched child; the system trust store is never changed.
- **INV-004**: the launch manifest holds argv and launch overrides only (technical-design I5).
- **INV-005**: hosted agent traffic, title calls to a provider and roundtable bare-model calls go through the proxy; a provider whose traffic does not is marked uncovered.
- **INV-006**: the seed budget stays below the sidecar's adaptive threshold.
- **INV-007**: no compression saving percentage is claimed (functional-design 3).

## Consequences

- The sidecar needs Node; how it is provided on macOS and Linux is the builder's call (register SC13).
- One proxy per instance or one shared is open, and with it the launch path (register SC5, G0); ADR-0029 decides who supervises the proxy.
- The bili MCP shim must follow `/resume` session ids (register SC8), and the `CODEX_HOME` overlay must leave Codex's history and auth in place (CX12).
- The hop context passes through the sidecar and counts against the destination's window (functional-design 5.6).
- The token-savings adapters for Hermes Agent and DeepSeek Harness ride this path and wait for HM12 and DS12 (token-savings 8.1).
- Gemini's launch-only adapter must keep working through the sidecar (register SC11).

## Confirmation

- Register SC1 to SC4, SC6 to SC11, SC13, CX12, HM12 and DS12 pass; all are open. SC6 is shared with ADR-0029, which also holds SC5, SC12 and SC14.
- Nothing has been run yet under Shinatama.
