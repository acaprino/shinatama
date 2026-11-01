# FRD-0002: Agent placement, homes and per-instance sets

| | |
|---|---|
| Version | 1 (Markdown record; JSON record version 1 at `c84d28e`) |
| Status | Accepted 2026-11-01 (this Markdown text; the JSON evidence record, where one exists, stays Proposed until `frd_tool.py promote`) |
| Date | 2026-11-01 |
| Sources | functional-design 3 (isolation), 5.14, 7; technical-design 3, 5.5 A; records D15 to D19, P20 |
| Governing ADRs | ADR-0012 v2, ADR-0013, ADR-0014 |
| Register sections | D15 to D19, P20, HM9 to HM11, HM13, DS9 to DS11, DS13, SS5, M1, CX12 |
| Related records | FRD-0003 (login in a box), FRD-0006 (store roots), FRD-0008 (Daodan into the home), FRD-0016 (TokenEconomizer in the set) |

## 1. Purpose and scope

Where each hosted agent lives decides whether Shinatama can disturb the user's own agent setup.
Three independent choices exist per agent: the **program** that runs, the **home** it writes,
and the **per-instance set** of plugins, skills and MCP servers one tab carries. This record
also holds the launch-time isolation rules: per-launch injection of hooks and MCP, instance
identity, and the removal of inherited agent markers.

## 2. Requirements

### 2.1 The three choices

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Placement is three independent choices per agent (program, home, per-instance set), so the user can, for example, keep Claude Code's home on `system` and the other agents in a box. | U 2026-10-28 ("private by default, option to integrate with the system"); A (three-way split) | X | D15, D18, D19 | 0012 |
| R02 | Switching any choice moves nothing: the other home's sessions stay readable and importable. | A | X | D15, D18 | 0012 |
| R03 | Home and write destination are configured per agent, not per instance. | A | X | SS5 | 0012, 0023 |

### 2.2 Program: `agents.<agent>.program = system | own`

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R04 | Default `system` when the agent is installed, `own` otherwise. | A | X | D18 | 0012 |
| R05 | With `system`, Shinatama runs the binary the user installed, discovered by reusing Mukade's harness catalog and installation scan where they cover the case (one recognition rule set); it never installs, updates or replaces that binary, and only reports an available update with its owner's command (`claude update`, `codex update`, `npm install -g <package>@latest` with the package read from the npm shim). | A | X | D18 | 0012, 0020 |
| R06 | A `system` program whose version is outside the proved range runs `Degraded`: each capability whose fixtures fail is `unsupported`, the agent stays launch-only plus its own export command, hops to or from it go by seed or are refused, and the user is told why. | U 2026-11-01 | X | E1, E5 | 0011, 0012 |
| R07 | With `own`, Shinatama installs its copy under `<data>/agents/<agent>/versions/<version>/` with npm `--prefix` (DeepSeek Harness is an npm package too); an update installs next to the running version, open instances keep the old one, new instances start on the new one, and the previous version is kept for rollback when `fixture_gate` fails. | A | V1 | D16, DS10 | 0012 |
| R08 | With `own`, the agent's self-update and its update command are both blocked (Claude `DISABLE_AUTOUPDATER=1` and `DISABLE_UPDATES=1`; OpenCode `OPENCODE_DISABLE_AUTOUPDATE=1`; Pi `PI_OFFLINE=1` or `PI_SKIP_VERSION_CHECK` and `PI_TELEMETRY=0`, run as `node … cli.js`, Node 22.19 or later). `own` Codex is not offered until its three update paths are controllable (D16). | A | V1 | D16 | 0012 |
| R09 | Hermes Agent's `own` recipe is its own installer with a bundled uv and Python (HM10); if HM10 yields no recipe, `own` is refused for Hermes Agent. | A | V1 | HM10 | 0012 |
| R10 | The combination `own` program with `system` home is forbidden for every agent: two versions of one agent would write one store, and a newer version can migrate it so the older one no longer reads it. `placement()` never returns it. | A | X | — | 0012 |
| R11 | A box migrated by a newer program that an older one cannot open is shown to the user and never repaired. | A | X | — | 0012 |

### 2.3 Home: `agents.<agent>.home = box | system`

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R12 | Default `box`: a home in Shinatama's folder, `<data>/agents/<agent>/home/` on a short path, selected at launch by the agent's home variable (`CLAUDE_CONFIG_DIR`; `CODEX_HOME`; `OPENCODE_DB` plus the four `XDG_*`; `PI_CODING_AGENT_DIR`; `HERMES_HOME`; `DSH_HOME` plus `DSH_AGENTS_HOME`). Plugins, skills, settings, sessions and login live there; the user's own runs see none of it. | U 2026-10-28 (default box) | X | D15, HM9, DS9 | 0012 |
| R13 | With a box the user's home is never written by Shinatama. Every agent also gets `TEMP`/`TMP` and, for Node-based agents, `NODE_COMPILE_CACHE` inside Shinatama's folder, because the home variable alone is not enough. | A | X | D15 | 0012 |
| R14 | Per-agent box extras: Codex on a short `CODEX_HOME` (its daemon socket must stay under about 108 characters), with its own daemon never meeting the user's; OpenCode with `OPENCODE_DISABLE_CLAUDE_CODE=1` and a small plugin that gives the programs it starts back the user's own `XDG_*`; Pi with `--no-skills` or a child `HOME`; Hermes Agent with an empty `skills.external_dirs` in the box's `config.yaml`; DeepSeek Harness with `DSH_AGENTS_HOME` so it does not read `~/.agents`. | S (D15, D17 records) | X | D15, D17, HM9, DS9 | 0012 |
| R15 | Shinatama starts and stops per box the background processes an agent would leave running: OpenCode's service and Hermes Agent's gateway. | S | X | HM2, D17 | 0008, 0012 |
| R16 | A box is created owner-only: 0700 on macOS and Linux, a current-user ACL on Windows. | A | X | D15, S11 | 0012 |
| R17 | A known, harmless write outside the box is shown to the user and the agent stays boxable; only a leak that touches credentials, sessions or the user's configuration makes the agent `system`-only. Codex's unelevated Windows sandbox writing `~\.sbx-denybin` is shown, not blocked; its elevated sandbox is not offered in a box. | U 2026-11-01 | X | D15 | 0012 |
| R18 | A boxed Claude Code launch tries `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES` against its MCP server logs in `%LOCALAPPDATA%\claude-cli-nodejs\Cache`; if they are still written, the leak is shown like Codex's and Claude stays boxable. | U 2026-11-01 | X | D15, D19 | 0012 |
| R19 | With `system`, the agent uses the user's own home: same login, sessions and plugins, visible outside Shinatama too; Daodan is installed into that agent's native plugin store (FRD-0008). | U 2026-10-28 | X | D18 | 0012 |
| R20 | While an agent is in a box, the user's existing sessions in the system home are still found: search and import read them read-only through `system_homes()`, and a hop or import brings them into the box. Additional system roots are `read_only` and never a box's write destination. | A | S1 | SS5 | 0012, 0023 |

### 2.4 Per-instance set of plugins, skills and MCP servers

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R21 | Each instance (one tab) can carry its own set in either home without an agent home of its own, so login and sessions stay shared; the launch loads exactly the instance's plugins, skills and MCP servers and excludes the home's others, user-level skill folders included. | U 2026-10-28 ("a box so they have the skills and plugins only for that instance"); A (mechanism) | V1 | D19 | 0012 |
| R22 | Two instances of one agent with different sets run at once on one home and share its sessions; a resumed session takes the set of the instance that resumes it. | A | V1 | D19 | 0012 |
| R23 | Claude Code: `--setting-sources project,local`, `--settings` with a copy of the user's settings minus the plugin list, `--plugin-dir` per plugin, `--mcp-config` with the set's servers and always `--strict-mcp-config`. | S (D19) | V1 | D19 | 0012 |
| R24 | Codex: it cannot load a plugin for one launch, so Shinatama installs each set's plugins into the home once, turned off, and each launch switches its set on with `-c`. Because that writes the home, Codex has per-instance sets only in a `box`; with the `system` home Codex instances use the set already in the user's home and Shinatama writes nothing there. | U 2026-11-01 | V1 | D19 | 0012, 0013 |
| R25 | OpenCode: `OPENCODE_CONFIG_DIR` (the set), `OPENCODE_CONFIG` (provider and model only) and an empty per-instance `OPENCODE_TEST_HOME`, because nothing turns off `~/.agents/skills`. | A | V1 | D19 | 0012 |
| R26 | Pi: `-ne -ns -np`, then `-e` and `--skill` per item; the box keeps `mcp.json` empty and each instance brings its MCP servers through its own extension (untested). | A | V1 | D19, PI7 | 0012 |
| R27 | Hermes Agent: no per-launch switch exists and a profile would split the sessions, so a Hermes instance carries its box's set until HM13 finds a way. | S | V1 | HM13 | 0012 |
| R28 | DeepSeek Harness: a `--patch` overlay per launch (plugin and MCP rows) plus the ACP session's `mcpServers`; skills come from the box and the project (untested). | S | V1 | DS13 | 0012 |
| R29 | Only the MCP servers an instance needs are mounted, through its set, and the mounted list is visible at launch. | S | V1 | D19 | 0012, 0038 |

### 2.5 Per-launch injection and instance identity

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R30 | Nothing system-level is touched by a launch: hooks, `shinatama mcp` and the instance set arrive through per-launch switches (`--settings`, `-c`, `OPENCODE_CONFIG*`, `-e`, `--patch` or the ACP session), never by editing the user's global agent configuration. | S | X | M1, CL8, CL9, OC6, PI4, PI7, HM5, DS5 | 0013 |
| R31 | Hermes Agent has no per-launch switch: its hook and MCP entries live in the box's own `config.yaml`, a file Shinatama owns; a Hermes Agent on the `system` home runs without them and its state comes from the PTY and the gateway. | A | X | HM4, HM5 | 0013 |
| R32 | At launch the agent receives `SHINATAMA_INSTANCE`, `SHINATAMA_THREAD`, `SHINATAMA_DB` and `SHINATAMA_INSTANCE_TOKEN` (a per-launch random token stored hashed). They are the instance's stable identity; the agent's own session id is never used for identity because it changes on `/resume`. | A | X | P15, M9, M15 | 0013 |
| R33 | `shinatama mcp` and `shinatama hook` refuse an unknown or ended instance and a caller whose token does not match, so one hosted agent cannot act as another; the token never appears in clear in the database, logs or transcripts. | A | X | M15 | 0013, 0024 |
| R34 | DeepSeek Harness scrubs `KEY\\|PASSWORD\\|SECRET\\|TOKEN` and `DSH_*` from stdio MCP servers' environments, so the instance token must reach `shinatama mcp` another way, found and verified by DS5. | S | V1 | DS5 (SEC) | 0013 |

### 2.6 Inherited agent markers

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R35 | Before launching any agent, Shinatama removes from the child's environment the union of every adapter's inherited-marker list, so the agent starts as it would from a fresh terminal even when Shinatama was itself started from inside an agent. The `SHINATAMA_*` variables are set after the removal. | U 2026-10-28 | X | P20 | 0014 |
| R36 | Removal matches exact names, never prefixes, so the user's own settings (`CLAUDE_CONFIG_DIR`, `CLAUDE_CODE_USE_BEDROCK` and the like) pass through. | U 2026-10-28 | X | P20 | 0014 |
| R37 | Two names are conditional: `GIT_EDITOR` only when its value is Claude's helper; Codex's generic sandbox names (`NPM_CONFIG_OFFLINE`, `PIP_NO_INDEX`, …) only when `SBX_NONET_ACTIVE` is present. Values are not read otherwise. | U 2026-10-28 | X | P20 | 0014 |
| R38 | Each adapter declares its markers through `inherited_markers()`; Hermes Agent and DeepSeek Harness add theirs through HM11 and DS11. Pi's `<agentDir>\bin` prepended to `PATH` is not covered by names and is handled separately. | S | X | HM11, DS11 | 0014 |

### 2.7 Still to prove

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R39 | The following are unproved and must pass before the feature is claimed: login persistence in a box (needs the user's sign-in, O1), the interactive mode with per-instance sets, the one-week self-update observation, OpenCode `own` side by side, and every row on macOS and Linux. | S | X | D15, D16, D19 | 0010 |
| R40 | Per-launch MCP injection (M1), one proxy per instance versus shared (SC5) and the Codex private overlay leaving history and auth in place (CX12) are unproved parts of the isolation story. | S | X | M1, SC5, CX12 | 0013 |
| R41 | The register's scratch homes are a separate, test-only device and never a product placement. | S | X | register 0 | 0010 |

## 3. Open questions

1. Test accounts and the one sign-in per agent in a box (register O1).
2. Whether Daodan will ship Hermes Agent and DeepSeek Harness packages, which changes what a box installs (register O10).

## 4. Acceptance

D15 to D19 on the three systems (today Windows only, partial), HM9, HM10, HM13, DS9, DS13 and
SS5; P20 with a real nested interactive start for Codex, OpenCode and Pi.

## 5. Relation to the JSON record

FRD-0002.json holds 54 extracted sentences; this record covers all of them in 2.1 to 2.7 and
adds the instance-identity rules (2.5) that the JSON set kept in FRD-0007.
