---
adr_id: ADR-0032
version: 1
title: Daodan installed as prebuilt per-host packages with every plugin enabled; other marketplaces allowed
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/daodan-native-support.md
    - docs/technical-design.md
    - docs/verification-register.md
  modules:
  domains:
    - daodan
    - checkup
    - plugin-marketplace
    - agent-home
governs_symbols:
depends_on:
  - ADR-0012
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

# ADR-0032: Daodan installed as prebuilt per-host packages with every plugin enabled; other marketplaces allowed

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Daodan is the user's plugin set, already compiled by Daodan for each host it supports. The
question is how Daodan reaches every hosted agent, which plugins are on, and what happens on
agents Daodan does not build for. Sources of the decision, kept apart:

- Decided by the user: "option A, downloaded and kept updated" (functional-design 5.5 heading);
  every plugin enabled, 2026-10-27: "mi aspetto che attivi tutti i plugin di shinatama" (I expect
  it to activate all the plugins; daodan-native-support 4.6); only Daodan is a marketplace,
  2026-10-27: "solo daodan è il repository marketplace plugin" (D1 record), a restriction the
  user lifted on 2026-11-01: any marketplace may be registered, including the ones Daodan
  declares as dependencies (functional-design 5.5; daodan-native-support 4.6).
- Author's call, not confirmed: no Daodan package on Hermes Agent and DeepSeek Harness and
  nothing copied from another host (functional-design 5.5 and 7, dated 2026-10-31).
- Proposed shape: the `daodan.enable` setting and the checkup rules of daodan-native-support 4.6,
  a document that is "proposed, not approved".

The full design is in functional-design 5.5 and daodan-native-support 4.6; this record holds
only the decision.

## Decision Drivers

- Daodan already ships a build per host; Shinatama should install it, not translate it.
- The user expects every plugin to be active.
- The old `acaprino/shinatama` marketplace was redundant and collided with the product name;
  Daodan itself depends on plugins from other marketplaces (record D15).
- A package Daodan did not compile and check for a host would claim an execution nobody proved.
- The context cost of "all" must stay controllable.

## Decision

Daodan is the marketplace Shinatama installs and enables, but not the only one allowed: the
marketplaces Daodan declares as dependencies, and any other, may be registered (user,
2026-11-01). The checkup installs or updates Daodan's own prebuilt build in every agent present,
into the agent's home: the box by default, the agent's native plugin store with the `system`
home, and per launch for an instance with its own set (see ADR-0012). Shinatama converts
nothing and translates nothing. The checkup enables every plugin the Daodan marketplace lists,
read from its manifest at check time and never from a list kept in Shinatama (48 at
marketplace 1.0.0). The setting `daodan.enable = all | core | custom` defaults to `all`; `core`
is the entry plugin plus its dependency closure. Plugins the user disabled stay disabled; a
plugin whose MCP server does not start is reported and never blocks the launch; a plugin added
in a later release is enabled at the next check. On Hermes Agent and DeepSeek Harness, for
which Daodan builds no package, the checkup installs nothing and says "Daodan: no package for
this agent". See functional-design 5.5 and daodan-native-support 4.6.

## Verified Interview Examples

- **EX-001** (nominal case): Input: the checkup on fresh profiles of each host Daodan builds for -> Expected: each host ends with the plugins enabled and no duplicates; Daodan's per-host build installed as is, nothing translated (register D2).
- **EX-002** (boundary case): Input: the user had disabled some Daodan plugins in their own setup -> Expected: they stay disabled (register D3).
- **EX-003** (boundary case): Input: `all` enabled and one plugin's MCP server fails to start -> Expected: the failure is reported by the checkup and the launch is not blocked (register D4; daodan-native-support 4.6).
- **EX-004** (rejection or error case): Input: the checkup on Hermes Agent or DeepSeek Harness while Daodan lists no package for them -> Expected: the checkup says so, installs nothing, the palette types nothing, nothing is copied from another host (register D20).

## Rejected Alternatives

- **RA-001**: keeping `acaprino/shinatama` as a companion marketplace beside Daodan; its 24 plugins all exist in Daodan at an equal or newer version, and its name collided with the product (register Q1, D1 record).
- **RA-002**: copying another host's Daodan build into Hermes Agent or DeepSeek Harness; a package Daodan did not compile and check for that host would claim an execution nobody proved (functional-design 5.5).

## Enforceable Invariants

- **INV-001**: Daodan's marketplace is registered on every host Daodan builds for; another marketplace, such as one Daodan declares as a dependency, may be registered beside it and never changes what `all` enables (user, 2026-11-01).
- **INV-002**: the set of plugins enabled under `all` equals the Daodan manifest read at check time; no plugin list is kept in Shinatama.
- **INV-003**: what is installed is Daodan's build for that host, unchanged; no host receives another host's build.
- **INV-004**: a plugin the user disabled is never re-enabled by the checkup.
- **INV-005**: no Daodan plugin failure blocks an agent launch.

## Consequences

- The cost of `all` is open (register D4): skill listings and tool definitions in each agent's first request, failing MCP servers, launch time. The D15 record saw Pi's system prompt grow from 2,932 to 55,463 characters after a Daodan install. The default stays `all` as asked.
- With the `system` home, the user's own agent runs see Daodan too (functional-design 3).
- The D15 record (2026-10-28) found that on Claude Code `testing@daodan` needs `mattpocock-skills@mattpocock`, and that Daodan depends on 8 plugins from other marketplaces (seven dependency marketplaces at Daodan `9fa47e7`, daodan-native-support 4.6); registering them is allowed since the user lifted the Daodan-only restriction on 2026-11-01. Register D2, which covers per-host install, is still open.
- Daodan's current marketplace is 1.0.0, since Daodan reset its versions on 2026-10-29 (functional-design 5.5; register D2, D4); the 42.0.0 of register Q1 and the 42.7.3 of the D1 record are history.
- The old `acaprino/shinatama` marketplace is gone: its local checkout was deleted on 2026-10-26 and the remote repository on 2026-10-27, after a verified bundle backup (functional-design 7 item 3; D1 record).
- Left behind by D1: prose naming the old repository in Daodan's `README.md` and `AGENTS.md`, and a dead link in Mukade's `README.md` line 8.
- When Daodan ships packages for Hermes Agent or DeepSeek Harness, the checkup treats them like the other five (register D20); asking Daodan for them is open (register O10).

## Confirmation

- D1 is done (record of 2026-10-27). D2 (G0), D3, D4 and D20 (G1) are open and must pass.
- D15 (running, partial) recorded Daodan installs in a box for Codex and Pi; Claude Code was partial and OpenCode was not run.
- Nothing else has been run.
