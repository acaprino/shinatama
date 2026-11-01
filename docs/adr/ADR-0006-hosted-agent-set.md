---
adr_id: ADR-0006
version: 1
title: Host six agents; capabilities declared unsupported until their rows pass; Gemini launch-only; Grok out
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - hosted-agents
    - agent-support
    - adapter-capabilities
    - launch-only
governs_symbols:
depends_on:
  - ADR-0010
  - ADR-0011
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

# ADR-0006: Host six agents; capabilities declared unsupported until their rows pass; Gemini launch-only; Grok out

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The set of hosted agents decides which adapters are built, which
register sections run and which hop pairs are tested. The question this record answers is which
agents Shinatama hosts, at what level, and what may be claimed for each before it is proved.

Origin: the six agents were decided by the user on 2026-10-31: "deve supportare opencode, codex,
claude code, pi, hermes e deep seek code" (functional-design 2, goal 1). Reading "deep seek code"
as DeepSeek Harness (`dsh`) and "hermes" as Hermes Agent by Nous Research is the author's call
(functional-design 7); register O9 asks the user to confirm the DeepSeek reading. Grok is out
because "the user said Grok is not supported for now" (functional-design 2, non-goals). Gemini as
launch-only is the author's call (functional-design 5.9). The full design is in functional-design
2 and 5.9, technical-design 2, 5 and 5.7 and register sections 7a and 7b; this record holds only
the decision.

## Decision Drivers

- The agents the user works with every day.
- Nothing is claimed for an agent from documentation alone (ADR-0010).
- Adding an agent later must be cheap (functional-design 2, the Grok non-goal).
- Hermes Agent and DeepSeek Harness are researched from source and docs, not installed on this machine.

## Decision

Shinatama hosts Claude Code, Codex, OpenCode, Pi, Hermes Agent and DeepSeek Harness. Hermes Agent
and DeepSeek Harness join with every adapter capability declared `unsupported` until its register
row passes (sections 7a and 7b): "nothing is claimed for them from documentation alone". Until a
reviewed Mukade pin carries their readers, those two declare `list_sessions`, `read_transcript`
and `export` unsupported: they resume natively within themselves and receive seeds, but are not
searched and are not hop sources (technical-design 5; mukade-handover 9). They count as covered by
the sidecar only once the proxy carries their provider traffic (register HM12, DS12). Gemini keeps
a launch-only adapter in v1: it starts through the sidecar, with no hop and no search, and a hop to
it is refused. Grok is not supported; adding it later, or Gemini beyond launch-only, means
implementing the adapter contract, recording fixtures and passing the suite, with no change to the
engine, the database, the UI or the roundtable (ADR-0011; technical-design 5.5.3). See
functional-design 2 and 5.9 and technical-design 5.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a made-up project codename stated once in turn 3 of the source, then a hop to each other agent and a question for the codename -> Expected: the correct answer for every ordered pair among the six at stage 1; a pair whose source is Hermes Agent or DeepSeek Harness counts from the day Mukade reads that source (technical-design 11.2).
- **EX-002** (boundary case): Input: Gemini launched from Shinatama -> Expected: it starts and is compressed through the sidecar (register SC11), with no hop and no search offered (functional-design 5.9).
- **EX-003** (rejection or error case): Input: a hop whose destination is a launch-only agent -> Expected: strategy `refuse`, with an explanation (technical-design 6.2).
- **EX-004** (rejection or error case): Input: Hermes Agent or DeepSeek Harness used with a provider whose traffic does not go through the proxy -> Expected: that provider is marked uncovered (register HM12, DS12).

## Rejected Alternatives

- **RA-001**: Grok; "Daodan has no Grok adapter and the user said Grok is not supported for now" (functional-design 2).
- **RA-002**: reading "deep seek code" as a product called DeepSeek Code or as a community project; no DeepSeek product has that name, and DeepSeek-TUI, deepcode-cli and a PyPI `deepseek-harness-cli` are other agents (technical-design 5.7; functional-design 2, goal 1).

## Enforceable Invariants

- **INV-001**: the hosted set is exactly the six named agents plus launch-only Gemini; no other agent is offered.
- **INV-002**: a Hermes Agent or DeepSeek Harness capability is shown as `unsupported` until its register row is `passed`.
- **INV-003**: Hermes Agent and DeepSeek Harness sessions are not listed, searched, exported or used as hop sources until a reviewed Mukade pin reads them (register HM3, DS3).
- **INV-004**: Gemini is never a hop source or destination and is not searched.
- **INV-005**: adding an agent changes only its adapter and fixtures, never the engine, the database, the UI or the roundtable.

## Consequences

- Register sections 7a (HM1 to HM14) and 7b (DS1 to DS15) were added on 2026-10-31; HM1, HM2, DS1 and DS2 joined the first-milestone set (technical-design 10 item 16).
- Mukade needs new readers in its own repository before search and hops from the two new agents (mukade-handover 9; ADR-0020).
- DeepSeek Harness has no terminal UI and is hosted in a native ACP pane (ADR-0007). Hermes Agent's gateway must be started and stopped per box (register HM2; ADR-0008).
- Neither new agent gets a Daodan package until Daodan builds one (ADR-0032; register D20, O10).
- Open: O9 (the DeepSeek reading and how it is hosted) and Gemini on Windows (functional-design 5.9). The versions researched are Hermes Agent 0.22.5 and DeepSeek Harness 0.2.1-rc.5.
- External terminal selection is not offered, for Gemini or any other agent, because every agent runs in Shinatama's own panes (functional-design 5.9, 5.1).

## Confirmation

- Register rows HM1 to HM14 and DS1 to DS15 pass, each lifting the matching capability from `unsupported`; HM3 and DS3 gate search and hops from the two new agents.
- SC11 passes for Gemini, and acceptance 11.2 passes for every ordered pair it covers.
- None of these rows has run; neither Hermes Agent nor DeepSeek Harness is installed on this machine (register 7a, 7b; next run 9 in register 20).
