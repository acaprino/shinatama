---
adr_id: ADR-0012
version: 1
title: Program system|own and home box|system per agent, per-instance sets; never own program with system home
status: Accepted
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
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0012: Program system|own and home box|system per agent, per-instance sets; never own program with system home

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. Where each agent's program and store live decides whether Shinatama
can disturb the user's own agent setup. The question this record answers is which binary runs,
which home it writes, and how one tab gets its own plugins and skills.

Origin: decided by the user on 2026-10-28: "di base è con istanze private, ma poi c'è l'opzione
per integrarlo con quello di sistema", then, to "si possono avviare anche se installate nel
sistema, in una sorta di box per cui hanno le skill e i plugin solo per quella istanza?", "ok"
(functional-design 5.14). The shape is the author's ("shape is mine"), except the default home
`box`, which is the user's decision of 2026-10-28, confirmed on 2026-11-01 (functional-design 7).
The default program (`system` when installed) and the per-agent set mechanisms are author's
calls listed in functional-design 7; owner-only box permissions and refusing `own` for Hermes
Agent when HM10 finds no recipe are also the author's calls. On 2026-11-01 the user decided that
a known, harmless leak such as Codex's is shown and the agent stays boxable, only a leak that
touches credentials, sessions or the user's configuration making it `system`-only (register
D15), and that Codex has per-instance sets only in a `box`, because installing a set writes the
home (functional-design 5.14 and 7). The same day the user decided Claude's MCP-log leak (record
D19): Shinatama tries `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES` (untested); if it works there is no
leak, and if not, the leak is shown like Codex's and Claude stays boxable (functional-design
5.14; register D15). The single `private` mode was split into a program choice and a home choice on
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
proved range runs `Degraded` (ADR-0011) with the reason shown; an `own` program is Shinatama's copy under
`<data>/agents/<agent>/versions/<version>/`, installed with npm `--prefix` side by side, with the
previous version kept for rollback when `fixture_gate` fails and the agent's self-update and update
command blocked. Home, `agents.<agent>.home = box | system`, default `box`: a box is
`<data>/agents/<agent>/home/` on a short path, selected by the agent's home variable
(technical-design 3 lists all six: `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` + `XDG_*`,
`PI_CODING_AGENT_DIR`, `HERMES_HOME`, `DSH_HOME` + `DSH_AGENTS_HOME`) plus `TEMP`/`TMP`,
`NODE_COMPILE_CACHE` and the per-agent extras of functional-design 5.14, created with
owner-only permissions, and the user's home is never written; a `system` home is the user's own,
and Daodan goes into its native plugin store. The combination `own` program with `system` home is
forbidden. Per-instance set: each instance (tab) can carry its own plugins, skills and MCP servers
without an agent home of its own, so login and sessions stay shared, and a resumed session takes
the set of the instance that resumes it; for Codex this holds only in a `box`, and with the
`system` home Codex instances use the set already in the user's home. Home and write destination are per agent, not per
instance; switching any choice moves nothing; from a box, the user's system-home sessions are
still found through `system_homes()`, read-only. Codex runs in a box with its unelevated Windows
sandbox leak (`~\.sbx-denybin`) shown, not blocked, and its elevated sandbox is not offered in a
box. Claude runs in a box with `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES` tried against its MCP
server logs in `%LOCALAPPDATA%\claude-cli-nodejs\Cache`; if they are still written, that leak is
shown in the same way and Claude stays boxable (user, 2026-11-01). See functional-design 5.14 and
technical-design 3.

## Verified Interview Examples

- **EX-001** (nominal case): Input: two instances of one agent with sets A and B on one box, run at once against a mock model -> Expected: each first model request holds exactly its own set; B resuming A's session carries A's earlier turn and only B's set: "the set follows the launch, not the session" (record D19, Pi 1.4.0 and Claude Code 2.1.312, Windows).
- **EX-002** (rejection or error case): Input: Codex 0.164.0 in a box with the unelevated Windows sandbox, during a tool turn -> Expected: it wrote `~\.sbx-denybin` (four stub files) in the real profile, ignoring `USERPROFILE`; the leak is recorded and shown to the user (record D15; functional-design 5.14).
- **EX-003** (boundary case): Input: a box `CODEX_HOME` whose daemon socket path was 163 characters -> Expected: the private daemon fails ("path must be shorter than SUN_LEN"), so the box path must stay short (record D15).
- **EX-004** (rejection or error case): Input: a placement of `own` program with `system` home -> Expected: not allowed; `placement()` never returns it (functional-design 5.14; technical-design 5.5 A).
- **EX-005** (boundary case): Input: a checkup and an update notice for a `system` program -> Expected: the user's binary checksum is unchanged; a version outside the proved range runs `Degraded` (ADR-0011) with the reason shown; switching mode moves nothing (register D18; not yet run on Shinatama).

## Rejected Alternatives

- **RA-001**: `own` program with `system` home; two versions of one agent would write the same store, and a newer version can migrate it so the older one no longer reads it, as OpenCode 2.x does once with its v1 tables (functional-design 5.14).
- **RA-002**: a box made by the home variable alone; D15 found that every agent then leaks outside it (temp files, the Node compile cache, user-level skill reads, Codex's sandbox) (functional-design 5.14; record D15).
- **RA-003**: Hermes Agent profiles (`-p`) for per-instance sets; a profile "brings its own session store, so it would split the sessions" (functional-design 5.14).
- **RA-004**: Codex's elevated Windows sandbox in a box; it "creates system users and firewall rules and is not box-safe" (functional-design 5.14).
- **RA-005**: blocking Codex from a box because of `~\.sbx-denybin`; the leak is shown instead, since a system Codex already writes it (functional-design 5.14, 7).

## Enforceable Invariants

- **INV-001**: `placement()` never returns `own` program with `system` home.
- **INV-002**: with a box, Shinatama writes nothing to the user's home; a leak the agent causes is recorded and shown: a known, harmless one (Codex's `~\.sbx-denybin`) keeps the agent boxable, and only a leak that touches credentials, sessions or the user's configuration makes the agent `system`-only (decided by the user 2026-11-01; register D15).
- **INV-003**: Shinatama never writes, updates or replaces a `system` program's binary.
- **INV-004**: additional system roots are `read_only` and never a box's write destination (register SS5; technical-design 4.2).
- **INV-005**: switching program or home moves no session; the other home's sessions stay readable and importable.
- **INV-006**: a box is created owner-only: 0700 on macOS and Linux, a current-user ACL on Windows.
- **INV-007**: an instance's launch loads exactly its set and excludes the home's others, user-level skill folders included (register D19).
- **INV-008**: a box migrated by a newer program that an older one cannot open is shown, never repaired (inference, not tested).
- **INV-009**: if register HM10 yields no `own` recipe for Hermes Agent (its installer with a bundled uv and Python), `own` is refused for Hermes Agent (author's call; register HM10; functional-design 5.14).
- **INV-010**: with the `system` home Shinatama installs no Codex set into the user's Codex home; Codex instances there use the home's own set (decided by the user 2026-11-01; functional-design 5.14).
- **INV-011**: a boxed Claude Code launch tries `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES`; MCP logs that still reach `%LOCALAPPDATA%\claude-cli-nodejs\Cache` are shown to the user and never make Claude `system`-only (decided by the user 2026-11-01; register D15, D19).

## Consequences

- Costs: one login per agent per box the first time; an `own` copy takes hundreds of MB to over a GB per kept version (D16).
- Per agent: Codex needs `CODEX_HOME` on a short path; OpenCode needs `OPENCODE_DB`, the four `XDG_*`, a plugin that restores the user's `XDG_*` for its children (D17) and `OPENCODE_DISABLE_CLAUDE_CODE=1`; Shinatama starts and stops OpenCode's service and Hermes Agent's gateway per box (ADR-0008). Claude writes MCP logs to `%LOCALAPPDATA%\claude-cli-nodejs\Cache` (D19); whether `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES` stops them is untested and register D15 checks it; if it does not, the leak is shown and Claude stays boxable (user, 2026-11-01).
- Codex sets are pre-installed into the box home turned off, a home write, which is why Codex has per-instance sets only in a `box` (user, 2026-11-01); Pi's MCP through the instance's extension is untested; Hermes Agent keeps its box's set until HM13; DeepSeek Harness sets are untested (DS13). `own` Codex waits for D16.
- Store roots and their access are recorded in the catalog (ADR-0009, ADR-0023); hooks, MCP and sets reach the agent per launch (ADR-0013). The register's scratch homes stay a separate, test-only device.

## Confirmation

- Register D15 and D16 (G0), D17, D18 and D19 (G1), HM9, HM10, HM13, DS9, DS13 and SS5 pass with records.
- Today D15, D16, D18 and D19 are running (partial) and D17 is changed, all on Windows only (2026-10-28). Unproved: login persistence in a box (waits on O1), interactive mode with sets, the one-week self-update observation, OpenCode `own` side by side, macOS and Linux; the Hermes Agent, DeepSeek Harness and SS5 rows have not run.
