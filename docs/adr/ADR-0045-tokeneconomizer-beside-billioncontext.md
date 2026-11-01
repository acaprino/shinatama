---
adr_id: ADR-0045
version: 1
title: TokenEconomizer beside BillionContext as an optional per-instance module
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/specs/token-savings.md
    - docs/functional-design.md
    - docs/notes/2026-11-01-tokeneconomizer-beside-billioncontext.md
  modules:
  domains:
    - token-savings
    - tool-pruning
    - loop-guard
    - billioncontext
    - per-instance-set
governs_symbols:
depends_on:
created_from_task: tokeneconomizer-routing-study-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0045: TokenEconomizer beside BillionContext as an optional per-instance module

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

The user asked on 2026-11-01 for a study of integrating `microsoft/TokenEconomizer` in Shinatama,
then framed it as "integrazione di TokenEconomizer affiancato a BillionContext". TokenEconomizer
(Apache-2.0, commit `c53a4a6`, read and not run) learns across the sessions of one repository and
cuts three kinds of waste through the agent's own hooks and settings: facts the sessions keep
rediscovering (micro-skills), tools never used (tool pruning) and stuck turns (loop guard). It
supports Claude Code, Codex, OpenCode (experimental) and Pi, not Hermes Agent or DeepSeek Harness,
and is tested on Claude Code 2.1.300-2.1.310, Codex 0.153.1-0.164.0, OpenCode 1.19.5 and 2.2.0 and
Pi 1.2.0-1.3.0; this machine runs 2.1.316, 0.164.0, 2.2.5 and 1.4.0. BillionContext owns the
request path and every reversible reduction inside one session (token-savings 7.2). The study is
`docs/notes/2026-11-01-tokeneconomizer-beside-billioncontext.md`.

## Decision Drivers

- Attack waste no layer attacks, without a second owner of the wire or of compaction.
- Keep the prompt cache: no tool-list change inside a session.
- A box never writes outside itself, and the user's own runs outside Shinatama must not change.
- Text injected into every session must not come from content a repository controls.
- Nothing is claimed as a saving before the ledger measures it (ADR-0038 INV-005).

## Decision

TokenEconomizer is admitted beside BillionContext as an optional module, off by default and chosen
per instance in the instance's set, pinned to a commit and installed per box like a Daodan plugin
(`--plugin-dir` on Claude Code; installed into the home and switched per launch with `-c` on
Codex; through the `plugin` array of the instance's `$OPENCODE_CONFIG` on OpenCode; `-e` on Pi).
Its state lives under a `TOKENECONOMIZER_HOME` inside the box, shared by the box's instances.
Shinatama's launcher owns the version gate, since TokenEconomizer has none. Tool pruning runs in
`propose` mode, written explicitly because the default is `auto`: Shinatama reads the proposal from
TokenEconomizer's state, waits for the repository's idle time itself, and adds the deny entries to
that instance's launch only; it never applies an entry matching `mcp__bili__*` (BillionContext's
recovery tools) or Codex's `image_media`, sets `toolprune_protect=mcp__bili__*`, keeps the
`toolprune` slash command out of the instance's set and does not relay the notice that invites
`toolprune apply`, so the repository's `.claude/settings.local.json` and `.codex/config.toml` are
not written by the module. On OpenCode, where TokenEconomizer has neither a catalog nor a writer,
Shinatama writes the instance's tool catalog into TokenEconomizer's state and turns the proposal
into `permissions` deny rules in that instance's `OPENCODE_CONFIG`, with OpenCode's work surface
and BillionContext's tools (`bili_*`) protected. The loop guard is on for Claude Code and Pi, off on OpenCode until the
redirect travels outside the tool result (an upstream request), and off on Codex and OpenCode
until its judge runs on julia1, a jev-like selector model on the local CPU, through a `noul`
question once TokenEconomizer offers an optimizer endpoint (an upstream request, not a fork);
until then the judge stays a side call through the agent's own CLI, measured first (TE4) because
it loads the box's user settings and not the instance's set. Micro-skills runs only as `envfacts`.
TokenEconomizer never sits on the wire and never compacts; BillionContext keeps both. Before it is
enabled on a harness, that harness passes the study's checks, including the contact points with
BillionContext: its ledger against billed usage (TE7), its side calls as BillionContext sees them
(TE8), redirects against CCR's threshold (TE9), session-start facts under `promptRewrite` (TE10)
and BillionContext's retrieved results against the loop guard's failure accounting (TE11).

## Verified Interview Examples

- **EX-001** (nominal case): Input: a Claude Code instance with TokenEconomizer in its set, whose repository's evidence proposes denying a tool unused over 10 prompts across 2 sessions -> Expected: after the repository has been idle 60 minutes, the next launch of that instance carries the deny entry; no file in the repository changes; a session launched outside Shinatama still sees the tool.
- **EX-002** (boundary case): Input: a running session when the deny list forms -> Expected: its tool list does not change; the next session gets it.
- **EX-003** (boundary case): Input: on Claude Code, in a repository whose envelope already holds five turns that ended green, a turn repeating the same failure, judged stuck at confidence 0.82 -> Expected: one redirect reaches the model as `additionalContext`; a second stuck segment gives the person a one-line notice, which on Claude Code the model is asked to repeat; the turn is never stopped by the module.
- **EX-004** (rejection or error case): Input: a Hermes Agent or DeepSeek Harness instance -> Expected: TokenEconomizer is not offered in its set.
- **EX-005** (rejection or error case): Input: a harness version outside the range its checks passed on -> Expected: Shinatama's launcher does not load the module for that instance and the instance says why (TokenEconomizer itself would run on any version).
- **EX-006** (rejection or error case): Input: TokenEconomizer's proposal contains `mcp__bili__acp_retrieve` -> Expected: Shinatama never adds it to a launch; the recovery of stored results and folded images stays available.
- **EX-007** (rejection or error case): Input: an OpenCode instance with the module -> Expected: tool pruning evidence accrues against the catalog Shinatama wrote, the proposal becomes deny rules in the next launch's `OPENCODE_CONFIG` only, the loop guard is off, and no redirect is appended to a tool result.

## Rejected Alternatives

- **RA-001**: tool pruning in `auto` mode; it writes the repository's settings files, which the user's own runs outside the box also read.
- **RA-002**: micro-skills with the proposer on by default; its facts are open-vocabulary text derived from command output the repository controls, injected into every session.
- **RA-003**: forking or vendoring TokenEconomizer to change its judge; one optimizer seam beside `runner_for` is an upstream request, as with fast-jev-compaction (token-savings 8.1).
- **RA-004**: TokenEconomizer on by default for every instance; it covers four of six harnesses and is untested on three installed versions.
- **RA-005**: using TokenEconomizer's own cost figures or ledger as the measure of a saving; only Shinatama's ledger, with each module on and off, separates its saving from BillionContext's.

## Enforceable Invariants

- **INV-001**: Shinatama launches TokenEconomizer so that it writes no file in the user's repository or home: `propose` mode written explicitly, the `apply` command and its notice kept out of the instance, state under the box's `TOKENECONOMIZER_HOME`; the residual path (a person running `te toolprune apply` from a terminal) is documented, not prevented.
- **INV-002**: the tool list of a running session never changes because of TokenEconomizer.
- **INV-003**: TokenEconomizer adds no proxy and no compaction; BillionContext stays the only owner of the wire, decision compaction and BillionContext's tiers the only reducers of history.
- **INV-004**: Shinatama's launcher loads the module only on a harness version whose checks have passed; the gate is Shinatama's, since TokenEconomizer has none.
- **INV-006**: no entry matching `mcp__bili__*`, and no Codex `image_media` capability, is ever applied from a TokenEconomizer proposal.
- **INV-005**: no saving is stated for TokenEconomizer before the ledger has measured it with the module on and off.
