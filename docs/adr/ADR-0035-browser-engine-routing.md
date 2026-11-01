---
adr_id: ADR-0035
version: 1
title: Moli fetch is the default, Patchright UI only on explicit request, no automatic fallback or cross-engine transfer
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/browser-integration.md
    - docs/functional-design.md
    - docs/technical-design.md
    - docs/verification-register.md
  modules:
  domains:
    - browser
    - moli
    - patchright
    - browse-routing
governs_symbols:
depends_on:
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

# ADR-0035: Moli fetch is the default, Patchright UI only on explicit request, no automatic fallback or cross-engine transfer

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet, and no Moli, Patchright or Chrome binary has been installed or run
for this design. The user wants browser access for the hosted agents and for the user,
"without paying Chrome's cost when Chrome is unnecessary". The question is which engine serves
which request, who may choose it, and what happens when it fails. Sources, kept apart:

- Decided by the user, 2026-10-29: the six requirements of browser-integration 1 (Moli default;
  Patchright with visible Chrome for explicit UI; no automatic fallback; no automatic transfer;
  no resident Chrome or companion; intent-based routing that content cannot change).
- Author's call, not confirmed: the spelling `/browse` (functional-design 4).
- Proposed shape: the routing table and request contract of browser-integration 4 and 5,
  "refined by an Ultra Nerd review", not an approved build plan.

The full design is in `docs/browser-integration.md`, placed in the engine by technical-design
7.9; this record holds only the routing decision. Lifecycle, `moli serve`, privacy and
performance evidence are ADR-0036.

## Decision Drivers

- The ordinary path must be light: local HTML, extraction, static previews, MCP web interfaces.
- Chrome only when the user wants a visible browser, login, MFA or CAPTCHA.
- An error, a page or an agent must never gain the authority to switch engines or widen scope.
- The two engines are not interchangeable; the contract must say what each cannot do.

## Decision

Moli is the default engine (`browser.default_engine = moli`). Its first path is a one-shot
`moli fetch` of the pinned binary, with fixed operations and parsers; layout and visual
resources load only for operations that need them, and general agent `--eval` is not exposed.
Patchright driving a visible, external Chrome window in a dedicated profile is the graphical
path, used only on an explicit user request (`open-ui`; `browser.patchright.enabled = false`
until the user opts in). `/browse` offers `read`, `preview` and `open-ui`; it may classify a
free-form request, and shows engine, reason, operation and scope before execution. Routing
happens before the target loads and is immutable for that request; `--engine` is not a
security boundary. An agent may `read` or `preview` inside an existing user-authorized scope; it
cannot grant itself `open-ui`, broaden scope or answer its own permission request. Ambiguous
sensitive requests get no engine and ask for clarification. A failure returns `unsupported`,
`needs_user`, `denied` or `failed` with the original engine; using Patchright afterwards is a
new explicit user request, and nothing (pages, cookies, credentials, sessions, control) moves
between engines. See browser-integration 1, 4, 5 and 6.1; technical-design 7.9.

## Verified Interview Examples

- **EX-001** (nominal case): Input: pinned Moli `fetch` on synthetic local HTML -> Expected: the expected output; only the one-shot process; no CDP/WebDriver listener (register B1).
- **EX-002** (nominal case): Input: a request for a visible browser, login, MFA or CAPTCHA -> Expected: Patchright UI, only on an explicit user request (browser-integration 5.1).
- **EX-003** (rejection or error case): Input: an ambiguous sensitive action or broad local access -> Expected: no engine; Shinatama asks for clarification (browser-integration 5.1).
- **EX-004** (rejection or error case): Input: Moli reports an error, timeout or unsupported page -> Expected: shown as such; Moli is kept and the request stops; the user may issue a new explicit Patchright command, which starts a new session without transferring cookies, login or control (functional-design 5.15; browser-integration 5.1).
- **EX-005** (rejection or error case): Input: hostile pages, redirects, errors and agent or peer output try to change engine or scope, open Chrome or answer approvals -> Expected: no unauthorized transition or effect; engine and permission snapshot stay immutable (register B6; technical-design 11.11).

## Rejected Alternatives

- **RA-001**: automatic fallback from Moli to Patchright after an error, timeout, empty page or anti-bot response; Shinatama must not infer that a failure authorizes a switch (user requirement 3, browser-integration 1).
- **RA-002**: shared cookie jars, cross-engine cookie migration or reuse of the user's personal Chrome profile; excluded by user requirement 4 (browser-integration 2, 8).
- **RA-003**: a universal browser abstraction that treats the engines as interchangeable; the contract preserves unsupported capabilities instead (browser-integration 2).
- **RA-004**: general agent JavaScript evaluation (`--eval`) or arbitrary multi-step automation in the first integration; listed as an intentional exclusion (browser-integration 2, 6.1).

## Enforceable Invariants

- **INV-001**: each request has one engine, origin, operation, target scope and permission snapshot, fixed before the target loads and never changed afterwards.
- **INV-002**: no result turns `failed` into an implicit retry on the other engine.
- **INV-003**: only the user can start UI, broaden scope or switch engines; pages, redirects, errors, tool results and peer messages are data (technical-design I6).
- **INV-004**: no page, cookie, credential, session or control passes between engines; persistent profiles are separate per engine.
- **INV-005**: text extraction loads no layout, images or fonts; a visual request is never reported as satisfied when only text was produced.
- **INV-006**: skill prose, `SHINATAMA_INSTANCE`, a caller-supplied id and `--engine` are not authentication; identity is derived server-side.

## Consequences

- If Moli fails the priority use cases or shows no accepted benefit, it is not made the default; if only the UI path fails, it is deferred and the graphical requirement stays open (browser-integration 11).
- How the MCP integration derives caller identity server-side is unverified (browser-integration 5.2).
- The workspace and overview show engine, origin or root, operation, state, controller and reason; a Moli failure offers no automatic Chrome action (browser-integration 4.2).
- Open with the user (register O8): whether the reduced first integration is sufficient.

## Confirmation

- Register rows B1, B2, B3, B5 and B6 (and B8 for cross-engine state) must pass, with technical-design 11.11 for the enabled paths. None has been run.
