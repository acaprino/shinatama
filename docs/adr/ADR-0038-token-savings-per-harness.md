---
adr_id: ADR-0038
version: 1
title: One token-savings adapter per harness on its own seam; one owner of compaction and of the wire; measure before claiming
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/specs/token-savings.md
    - docs/functional-design.md
    - docs/verification-register.md
  modules:
  domains:
    - token-savings
    - harness-adapter
    - prompt-cache
    - token-ledger
governs_symbols:
depends_on:
  - ADR-0029
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

# ADR-0038: One token-savings adapter per harness on its own seam; one owner of compaction and of the wire; measure before claiming

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

The user wants every token-saving technique that exists applied, as long as none lowers
quality, with the same behaviour on every hosted harness. The six harnesses offer different
seams, and BillionContext already integrates each of them on its own seam. Sources, kept apart:

- Decided by the user: one shared core and one adapter per harness on its own seam, "the way
  BillionContext does it", with no single shared proxy (2026-10-31, token-savings goal 4 and
  8.2); no saving percentage stated until measured on the user's sessions (goal 5, an existing
  constraint of functional-design 3).
- Author's call, not confirmed: the order prevent, remove reversibly, cut lossily (goal 1); the
  four adapter rules (7.1); the milestones (8.1).

The full design is in `docs/specs/token-savings.md`, a draft; this record holds only the
decision. What the adapters apply is ADR-0037.

## Decision Drivers

- The behaviour is uniform; the integration is not.
- Two compactions on one history destroy each other's substrate.
- A saving that invalidates the cached prefix costs more than it saves.
- No technique can be said to save anything without a per-session ledger.

## Decision

The shared core is the scoring library, the deterministic features, the julia1 endpoint and the
ledger. Each harness gets its own adapter, written for the seam it offers and verified on its
installed version: on Claude Code the `session.compact` function hook where the installed
version has function hooks, otherwise the base-URL route; on Codex and Hermes Agent on the
wire, inside the request path they already take; on OpenCode and Pi inside their plugin or
extension; on DeepSeek Harness inside the dsh plugin of the box's ACP profile. Every adapter
follows four rules: one owner of compaction (the harness's native compaction is stood down or
intercepted); one owner of the wire (inside BillionContext's path, never a second proxy in front
of or behind it); cache-safe (no rewrite inside the cached prefix, compaction at the sidecar's
break-even boundary); verified per version (a conformance test and a version gate). Techniques
apply in the order prevent, remove reversibly, cut lossily, never the reverse, and lossy
techniques are not stacked. A per-session ledger records input tokens per turn, cache hit rate,
compaction count and tool re-runs after compaction. Work order: measure; Claude Code with a
rules-only local endpoint; julia1; the other adapters one at a time (Hermes Agent and DeepSeek
Harness after HM12 and DS12); tool chooser and digests; code-mode gateway. See token-savings
2, 4, 7.1 and 8.1.

## Verified Interview Examples

- **EX-001** (nominal case): Input: Claude Code whose installed version has function hooks -> Expected: the adapter registers on `session.compact`; without function hooks it uses the base-URL route (token-savings 7.1).
- **EX-002** (boundary case): Input: an adapter's rewrite on any harness -> Expected: the cache hit rate is unchanged and the harness's native compaction does not run behind it (token-savings 9.2 item 5).
- **EX-003** (rejection or error case): Input: Hermes Agent, for which no setting or hook is yet known to stand its native compaction down -> Expected: its adapter is not built (token-savings 9.2 item 6).
- **EX-004** (rejection or error case): Input: a vendor figure such as roughly 85% fewer tool-definition tokens or about 37% fewer tokens on chained calls -> Expected: not stated as a saving here until the ledger measures it on at least ten real sessions per harness (token-savings 4.1, 4.2, 9.2 item 1).

## Rejected Alternatives

- **RA-001**: a single proxy shared by all harnesses; each harness is integrated on its own seam, as BillionContext does (token-savings goal 4, 7).
- **RA-002**: letting the harness's native compaction run beside the adapter's; two compactions on one history destroy each other's substrate (token-savings 7.1).
- **RA-003**: rewriting early messages or changing the tool list per turn; it breaks the prompt cache, and a saving that invalidates the cached prefix costs more than it saves (token-savings anti-goals, 6.5).
- **RA-004**: stacking lossy techniques (sliding window, lossy summary, sub-agent summary); their losses add up (token-savings anti-goals).

## Enforceable Invariants

- **INV-001**: on any history, at most one compaction runs: when the adapter compacts, native compaction is stood down or intercepted.
- **INV-002**: where BillionContext sits on the request path, the adapter acts inside it and never adds a second proxy.
- **INV-003**: no adapter rewrites inside the cached prefix or changes the tool list per turn; Shinatama keeps the sidecar and MCP set stable per launch (token-savings 4.1).
- **INV-004**: every adapter has a conformance test against the real installed harness and a version gate.
- **INV-005**: no saving percentage is claimed for Shinatama anywhere before the ledger has measured it on the user's sessions; vendor figures are labelled as not measured.

## Consequences

- The seams are read from BillionContext's source, not run (token-savings 9.1). The Hermes Agent and DeepSeek Harness adapters ride BillionContext's path and wait for its proxy coverage (register HM12, DS12).
- Hermes Agent compacts natively at 50% of the window and has a `post_compaction` hook; how to stand its compaction down is unknown, so its adapter waits.
- The code-mode gateway in front of MCP servers is the last milestone; technical-design 7.5a keeps it out of v1 as an evaluated option, gated by register T1 and T3 (see ADR-0025).
- Daodan is not changed: it ships compiled per harness (token-savings non-goals).
- Verification records go under `docs/verification/` and the register; no register row is assigned to the 9.2 checks yet.

## Confirmation

- token-savings 9.2 items 1, 2, 5 and 6 must hold for each adapter before any saving is claimed; register HM12 and DS12 (G0, `SEC`) gate the Hermes Agent and DeepSeek Harness adapters.
- Nothing has been run.
