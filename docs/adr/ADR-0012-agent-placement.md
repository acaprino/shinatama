---
adr_id: ADR-0012
version: 1
title: Program system|own and home box|system per agent, per-instance sets; never own program with system home
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - agent-placement
    - box-home
    - own-program
    - per-instance-sets
    - home-isolation
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

# ADR-0012: Program system|own and home box|system per agent, per-instance sets; never own program with system home

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. Where each agent's program and store live decides whether Shinatama
can disturb the user's own agent setup. The question this record answers is which binary runs,
which home it writes, and how one tab gets its own plugins and skills.

Origin: decided by the user on 2026-10-28: "di base è con istanze private, ma poi c'è l'opzione
per integrarlo con quello di sistema", then, to "si possono avviare anche se installate nel
sistema, in una sorta di box per cui hanno le skill e i plugin solo per quella istanza?", "ok"
(functional-design 5.14). The shape is the author's ("shape is mine"). The defaults (program
`system` when installed, home `box`), Codex in a box with its leak shown, and the per-agent set
mechanisms are author's calls listed in functional-design 7; owner-only box permissions are also
the author's call. The single `private` mode was split into a program choice and a home choice on
2026-10-28, after the D15 runs (record D15). The full design is in functional-design 5.14 and
technical-design 3; this record holds only the decision.

## Decision Drivers

- By default the user's sessions, settings, plugins and login stay untouched.
- An option to integrate with the system installation.
- Two versions of one agent must never write one store.
- Per-tab plugins and skills without splitting login and sessions.

## Decision

Three independent choices per agent. Program, `agents.<agent>.program = system | own`, default
`system` when the agent is installed: a `system` program is never installed, updated or replaced
by Shinatama, which only reports an update with its owner's command, and a version outside the
proved range runs degraded with the reason shown; an `own` program is Shinatama's copy under
`<data>/agents/<agent>/versions/<version>/`, installed with npm `--prefix` side by side, with the
previous version kept for rollback when `fixture_gate` fails and the agent's self-update and update
command blocked. Home, `agents.<agent>.home = box | system`, default `box`: a box is
`<data>/agents/<agent>/home/` on a short path, selected by the agent's home variable plus
`TEMP`/`TMP`, `NODE_COMPILE_CACHE` and the per-agent extras of functional-design 5.14, created with
owner-only permissions, and the user's home is never written; a `system` home is the user's own,
and Daodan goes into its native plugin store. The combination `own` program with `system` home is
forbidden. Per-instance set: each instance (tab) can carry its own plugins, skills and MCP servers
without an agent home of its own, so login and sessions stay shared, and a resumed session takes
the set of the instance that resumes it. Home and write destination are per agent, not per
instance; switching any choice moves nothing; from a box, the user's system-home sessions are
still found through `system_homes()`, read-only. Codex runs in a box with its unelevated Windows
sandbox leak (`~\.sbx-denybin`) shown, not blocked, and its elevated sandbox is not offered in a
box. See functional-design 5.14 and technical-design 3.

## Verified Interview Examples

- **EX-001** (nominal case): Input: two instances of one agent with sets A and B on one box, run at once against a mock model -> Expected: each first model request holds exactly its own set; B resuming A's session carries A's earlier turn and only B's set: "the set follows the launch, not the session" (record D19, Pi 1.4.0 and Claude Code 2.1.312, Windows).
- **EX-002** (rejection or error case): Input: Codex 0.164.0 in a box with the unelevated Windows sandbox, during a tool turn -> Expected: it wrote `~\.sbx-denybin` (four stub files) in the real profile, ignoring `USERPROFILE`; the leak is recorded and shown to the user (record D15; functional-design 5.14).
- **EX-003** (boundary case): Input: a box `CODEX_HOME` whose daemon socket path was 163 characters -> Expected: the private daemon fails ("path must be shorter than SUN_LEN"), so the box path must stay short (record D15).
- **EX-004** (rejection or error case): Input: a placement of `own` program with `system` home -> Expected: not allowed; `placement()` never returns it (functional-design 5.14; technical-design 5.5 A).
- **EX-005** (boundary case): Input: a checkup and an update notice for a `system` program -> Expected: the user's binary checksum is unchanged; a version outside the proved range runs degraded with the reason shown; switching mode moves nothing (register D18; not yet run on Shinatama).

## Rejected Alternatives

- **RA-001**: `own` program with `system` home; two versions of one agent would write the same store, and a newer version can migrate it so the older one no longer reads it, as OpenCode 2.x does once with its v1 tables (functional-design 5.14).
- **RA-002**: a box made by the home variable alone; D15 found that every agent then leaks outside it (temp files, the Node compile cache, user-level skill reads, Codex's sandbox) (functional-design 5.14; record D15).
- **RA-003**: Hermes Agent profiles (`-p`) for per-instance sets; a profile "brings its own session store, so it would split the sessions" (functional-design 5.14).
- **RA-004**: Codex's elevated Windows sandbox in a box; it "creates system users and firewall rules and is not box-safe" (functional-design 5.14).
- **RA-005**: blocking Codex from a box because of `~\.sbx-denybin`; the leak is shown instead, since a system Codex already writes it (functional-design 5.14, 7).

## Enforceable Invariants

- **INV-001**: `placement()` never returns `own` program with `system` home.
- **INV-002**: with a box, nothing is written to the user's home; a found leak is recorded and shown, or the agent is marked `system`-only (register D15).
- **INV-003**: Shinatama never writes, updates or replaces a `system` program's binary.
- **INV-004**: additional system roots are `read_only` and never a box's write destination (register SS5; technical-design 4.2).
- **INV-005**: switching program or home moves no session; the other home's sessions stay readable and importable.
- **INV-006**: a box is created owner-only: 0700 on macOS and Linux, a current-user ACL on Windows.
- **INV-007**: an instance's launch loads exactly its set and excludes the home's others, user-level skill folders included (register D19).
- **INV-008**: a box migrated by a newer program that an older one cannot open is shown, never repaired (inference, not tested).

## Consequences

- Costs: one login per agent per box the first time; an `own` copy takes hundreds of MB to over a GB per kept version (D16).
- Per agent: Codex needs `CODEX_HOME` on a short path; OpenCode needs `OPENCODE_DB`, the four `XDG_*`, a plugin that restores the user's `XDG_*` for its children (D17) and `OPENCODE_DISABLE_CLAUDE_CODE=1`; Shinatama starts and stops OpenCode's service and Hermes Agent's gateway per box (ADR-0008). Claude still writes MCP logs to `%LOCALAPPDATA%\claude-cli-nodejs\Cache`.
- Codex sets are pre-installed into the home turned off, a home write (author's call); Pi's MCP through the instance's extension is untested; Hermes Agent keeps its box's set until HM13; DeepSeek Harness sets are untested (DS13). `own` Codex waits for D16.
- Store roots and their access are recorded in the catalog (ADR-0009, ADR-0023); hooks, MCP and sets reach the agent per launch (ADR-0013). The register's scratch homes stay a separate, test-only device.

## Confirmation

- Register D15 and D16 (G0), D17, D18 and D19 (G1), HM9, HM10, HM13, DS9, DS13 and SS5 pass with records.
- Today D15, D16, D18 and D19 are running (partial) and D17 is changed, all on Windows only (2026-10-28). Unproved: login persistence in a box (waits on O1), interactive mode with sets, the one-week self-update observation, OpenCode `own` side by side, macOS and Linux; the Hermes Agent, DeepSeek Harness and SS5 rows have not run.
