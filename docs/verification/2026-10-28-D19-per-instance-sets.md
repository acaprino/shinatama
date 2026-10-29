# D19 — Per-instance set of plugins, skills and MCP servers

Item: D19 (G1): "Per-instance set (main 5.14): an instance launched with the switches of its agent sees exactly its plugins, skills and MCP servers and none of the home's others (user-level skills folders included), while the login and sessions of the home stay shared; two instances of one agent with different sets run at once." Pass when: "Each instance's request contains exactly its set; no switch edits a file in the home; or the gap recorded per agent."
Status: **running (partial)**. Windows only.
Date: 2026-10-28.
Runner: Claude Code session (Opus 5.5), one worker per agent. Mock model, synthetic fixtures, no login, no network, cost 0.
Machine: Windows 11 Home build 26200; PowerShell 7; Node v24.14.1.

**Method.** Each agent gets synthetic fixtures with unique names, so any appearance in a request can be traced:
- **In the home:** a plugin or extension `d19-home-*`, a skill, an MCP server, and user-level skills in the profile folders the agent reads.
- **Two sets, A and B:** each has its own plugin, skill and MCP server.

Each variant (no switches, set A, set B, and each switch combination tried) runs one turn against the mock. The **first model request** is read for `d19-*` names. Two instances with different sets run at once on one home. Every home file is hashed before and after each run, except session files.

## Pi 1.4.0 (`verify/D19/pi/run.ps1 -Mode box|system|both`, `inspect.mjs`, `mcp-stub.mjs`)

Working launch, both for a box (`PI_CODING_AGENT_DIR`) and a system home (a decoy profile's `.pi\agent`):

```
node …\cli.js -p … --offline -ne -ns -np
  -e <set>\d19-A.ts  --skill <set>\skills\d19-a-skill
  -e builtin:mcp  -e <set>\d19-a-mcp.ts          (registers the instance's MCP server)
  --exclude-tools "mcp__<home server>__*"          (or an allow-list: --tools read,bash,edit,write,d19_a_tool,mcp__d19_a_mcp__*)
```

`-e <package folder>` loads a package's extension **and** its skills, even with `-ns`.

| Variant | Skills | Extension tools | MCP |
|---|---|---|---|
| no switches | home, profile, settings and user skills | all 3 home tools | home server tool; a second home server named in the prompt |
| `-ne -ns -np` | none | none | none |
| A, no exclusion | `d19-a-skill` | `d19_a_tool` | A's server **plus the home's** |
| A or B with exclusion or allow-list | own only | own only | own only, **plus the home's `codemode` server named in the prompt** |

- **Exact extensions, packages and skills: PASS.**
- **User-level skills excluded: PASS.** `-ns` removes the agent-folder skills, `HOME\.agents\skills`, the skills listed in `settings.json` and package skills.
- **MCP exact: PARTIAL.**
  - Pi has no per-launch MCP config. Servers come only from `<agentDir>\mcp.json` (or a trusted project's `.pi\mcp.json`), and `pi mcp add` writes to the home.
  - `-ne` also turns off Pi's built-in MCP extension; `-e builtin:mcp` brings it back together with the whole home `mcp.json`.
  - Excluding tools hides a home server's tools, but every home server is still **started**. A home server at Pi's default `codemode` exposure stays named in the system prompt, although it cannot be called.
- **Two at once: PASS** (box). Instance A, in RPC mode, was mid-turn on a hanging request while B, with a different set, ran to completion. A then went on. Each request held only its own set.
- **Sessions shared: PASS.** B, launched with set B, resumed A's session. The request carried A's earlier turn and only B's set: **the set follows the launch, not the session.**
- **Login shared: PASS** (inference for a real sign-in): every instance used the home's provider file.
- **Home unchanged: PASS.** Only session files changed; `settings.json` was unchanged; the real `~\.pi\agent` was untouched.
- The home's `AGENTS.md` reaches every variant unless `-nc` is passed, and `-nc` also drops the project's `AGENTS.md`.
- (inference, untested) In a box, keeping the box's `mcp.json` empty and giving each instance its servers through its own extension makes MCP exact. In a system home, an exact MCP set needs `--no-mcp` or a Shinatama extension that replaces Pi's built-in MCP.

## Claude Code 2.1.312 (`verify/D19/claude_code/run.ps1`, `analyze.mjs`, `d19-mcp.mjs`)

Working launch, in a box and in a second home standing for the system home:

```
claude -p … --setting-sources project,local
  --settings <copy of the home's user settings.json without enabledPlugins/extraKnownMarketplaces>
  --plugin-dir <each plugin of the set>
  --mcp-config <the set's servers + each set plugin's own servers, keyed plugin_<plugin>_<server>>
  --strict-mcp-config
```

The minimal form, `--setting-sources project,local --plugin-dir A --mcp-config a.json --strict-mcp-config`, also gives the exact set.

| Variant | `d19-*` names in the first request |
|---|---|
| no switches | the home's plugin, skill, agent and plugin MCP; user skill, agent and command; user-scope and local-scope MCP |
| A | `d19-a-plugin`, `d19-a-skill`, `d19-a-mcp` (+ `d19-aplug-mcp` when passed in `--mcp-config`) |
| B | `d19-b-plugin`, `d19-b-skill`, `d19-b-mcp` |

- **Exact set: PASS.** `--strict-mcp-config` also drops the set's *own* plugin MCP server. Passing it in `--mcp-config` under the key `plugin_<plugin>_<server>` restores it with the usual tool name, so skills that call plugin tools keep working.
- **User-level skills excluded: PASS.** Leaving out the `user` source removes `<config>\skills`, `agents` and `commands`. The decoy `HOME\.claude\skills` never appeared.
- **MCP exact: PASS, with `--strict-mcp-config` only.** Without it, local-scope servers stored in the home's `.claude.json` under `projects[cwd]` still load. Without it, each run also made one `CONNECT api.anthropic.com:443`. (inference) That is an account-level lookup, such as claude.ai connectors, which with a real login could add servers. That is a second reason to always use strict.
- **Two at once: PASS.** A hung at the mock while B finished; each request carried only its own set.
- **Sessions shared: PASS.** Both transcripts are in the one home. A, resumed with B's set, kept its session id; the earlier turn's skill list stays in history, but the current tools were B's only.
- **Login shared: NOT RUN.** Every instance uses the same `CLAUDE_CONFIG_DIR`, so the same `<home>\.credentials.json` (D15).
- **Home unchanged: PARTIAL.** `settings.json`, `installed_plugins.json`, `known_marketplaces.json` and the MCP entries of `.claude.json` never changed. But every run, with or without switches, rewrites `plugins\store\cc-plugin-agents-md_builtin-*.json` and the `.claude.json` key `pluginUsage`, which now lists `d19-a-plugin@inline` and `d19-b-plugin@inline`. This is bookkeeping, not configuration.
- **Leaving out the `user` source drops all user settings**, for example `includeCoAuthoredBy:false`. Passing a copy back with `--settings` restores them.
- **Switches that did not give the set:**
  - `--plugin-dir` + `--mcp-config` alone: home and set together.
  - `--settings` that disables the home's plugins: user skills, agents and commands stay.
  - `--bare`: the plugin's skill is not listed, and OAuth is never read, so a subscription login would not work.
  - `--safe-mode`: drops the set too.
  - `--disable-slash-commands`: drops the set's skills.
  - `--restricted`: refused together with bypassed permissions.
- `--add-dir <dir>` loads loose skills from `<dir>\.claude\skills` even with the user source left out.
- **Box leak not seen in D15:** MCP server logs go to `%LOCALAPPDATA%\claude-cli-nodejs\Cache\<cwd>\mcp-logs-*`, outside the box. The binary contains `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES` (untested). The 1-byte `fswatch-probe` in `%TEMP%` appeared again.

## Codex 0.164.0 (`verify/D19/codex/run.ps1`, `setup.ps1`, `variants.ps1`, `sets.ps1`, `lib.ps1`, `mcp-stub.mjs`, `inspect.mjs`)

**Codex cannot load a plugin for one launch only.** The set's plugins must already be installed in the home and turned off there. This is a one-time write into the home, done by Shinatama (`codex plugin add …`, then `enabled = false` in `config.toml`). Each instance then switches its own plugins on, and everything else off, with `-c` (run through `exec`, sandbox off):

```
codex exec …
  -c plugins.<home plugin>.enabled=false
  -c mcp_servers.<home server>.enabled=false
  -c "skills.config=[{path='<home>\skills\<skill>\SKILL.md',enabled=false}, …]"
  -c skills.bundled.enabled=false
  -c plugins.d19-a@d19.enabled=true
  -c "mcp_servers.d19-a-mcp.command='…node.exe'" -c "mcp_servers.d19-a-mcp.args=[…]"
```

Plugin keys must be written without quotes (`plugins.d19-a@d19`); the quoted form matched nothing.

| Variant | `d19-*` names in the first request |
|---|---|
| no switches | home plugin skill, user skill, home MCP tool |
| A | `d19-a:d19-a-skill`, `mcp__d19_a_mcp/…` only |
| B | `d19-b:d19-b-skill`, `mcp__d19_b_mcp/…` only |
| A without the switch-offs | A plus all of the home's |

- **Exact set: PASS**, given the install step.
- **User-level skills excluded: PASS.** `skills.config … enabled=false` removes `<CODEX_HOME>\skills`, and `skills.bundled.enabled=false` removes Codex's built-in skills. Whether Codex reads `~\.agents\skills` from the real profile is undetermined: the decoy skill never appeared, and the user's real folder is empty.
- **MCP exact: PASS.** Servers can be defined per launch with `-c`.
- **Two at once: PASS.** A hung while B finished; `thread/list` saw both threads.
- **Sessions shared: PASS, with a catch.** Both rollouts are in the one home. A resumed thread carries the earlier set's skill list in its history, next to the current one.
- **Login shared: NOT RUN.** (inference) One `CODEX_HOME` holds one auth store.
- **Home unchanged: PASS.** No configuration file changed in 19 runs plus the concurrent run, `config.toml` included. Only sessions, SQLite WAL/SHM, the sandbox log and locks changed.
- **Did not work:**
  - enabling a plugin that is not installed;
  - a marketplace defined with `-c`;
  - `skills.config` to *add* a skill (it only switches skills off);
  - `-c plugins={}`, `mcp_servers={}` and the like (the override merges);
  - `--disable plugins` (not selective);
  - `skills.include_instructions=false` (removes all skills);
  - `-p <absolute path>` (only plain profile names).
- **Alternatives that work:**
  - `-p <set>` with a `<set>.config.toml` written into the home, one file per set;
  - `exec --ignore-user-config` with the provider supplied by `-c`. The flag exists only on `exec`, not on the interactive `codex`.
- (inference) The interactive `codex` takes the same `-c`. Not run.

## OpenCode 2.1.4 (`verify/D19/opencode/run.ps1`, `lib.ps1`, `mcp-server.mjs`, `db-sessions.mjs`)

**Working launch, box only.** Start from the box environment of D15: `OPENCODE_DB`, the four `XDG_*` variables, `TEMP`/`TMP` and `OPENCODE_DISABLE_CLAUDE_CODE=1`. Then add, per instance:
- `OPENCODE_CONFIG_DIR=<set dir>`, holding `opencode.json` (only `mcp.servers`), `plugins/<name>/index.js` and `skills/<name>/SKILL.md`;
- `OPENCODE_CONFIG=<Shinatama settings file>`, holding provider and model only;
- `OPENCODE_TEST_HOME=<an empty home of the instance's own>`.

The command is `opencode run --standalone …`.

| Variant | `d19-*` names seen |
|---|---|
| no switches, box standard | home plugin, home skill, home MCP, `d19-user-agents-skill` |
| `OPENCODE_CONFIG` or `OPENCODE_CONFIG_CONTENT` with set A | all of the above **plus** A |
| `OPENCODE_CONFIG_DIR` alone | exit 1, `Model unavailable` (the home's provider settings are gone) |
| `OPENCODE_CONFIG_DIR` + settings file | `d19-user-agents-skill` + A |
| with the home's own `opencode.json` as the settings file | as above **plus the home MCP** again |
| **working launch, A** | **exactly A** |
| **working launch, B** | **exactly B** |

- **Merge or replace:**
  - `OPENCODE_CONFIG` and `OPENCODE_CONFIG_CONTENT` **merge**: they add a layer and the home's items stay.
  - `OPENCODE_CONFIG_DIR` **replaces** the global configuration folder (`packages/cli/src/server-process.ts:38`, `packages/util/src/global.ts:79`), so the provider and model have to be supplied again.
  - MCP servers are added together from every layer, and none can remove another's. The settings file must therefore never be the home's own `opencode.json`.
- **User-level skills excluded: PASS, but only with a home of the instance's own.**
  - `OPENCODE_DISABLE_CLAUDE_CODE` shuts off `~/.claude`.
  - Nothing turns off `~/.agents/skills`, which is read from `OPENCODE_TEST_HOME` and otherwise from the OS home.
  - (from source, untested) The instance's home also moves where `~` resolves in skill, instruction and plugin paths, the plan folder `~/.opencode/plan`, and the AWS and gcloud provider settings.
  - (inference) A Shinatama plugin using `ctx.skill.transform` could remove out-of-set skills instead.
- **MCP exact: PASS, from the second request on.**
  - The servers connect 1.6–1.8 s after start, so the first request of a session has **no** MCP tools at all.
  - Each server also needs `codemode:false`; otherwise its tools hide behind the single `execute` tool.
- **Two at once: PASS.** A hung while B finished with exactly its set; there were no database lock errors.
- **Sessions shared: PASS.** All 12 sessions are in the one box database. The login is the `credential` table of that same database (inference, not exercised).
- **Home unchanged: PASS for configuration, plugins and skills.** The only new files were shell-tool output under `data\opencode\shell\…`, which appears with or without switches.
- A `plugins:` entry must be a folder with `index.js`; a single file is rejected.
- **System home: NOT RUN** (it needs the real stores).
- **Not caused by this run:** during it, 585 new lines appeared in the real `~\.local\share\opencode` log. None mention the box: they come from an OpenCode run of another session, in a scratch folder under `D:\Projects\mukade`.

## Criterion

Met on Windows for all four agents in a box, each with its own conditions. A real login and the system home (for OpenCode) remain open, as does the interactive mode of each agent (all runs were print/exec).

| Agent | Exact set | Condition |
|---|---|---|
| Claude Code | yes | `--setting-sources project,local` + `--settings <user settings without plugins>` + `--plugin-dir` + `--mcp-config` + `--strict-mcp-config`; a set plugin's own MCP server must be passed again in `--mcp-config` |
| Codex | yes | set plugins installed in the home and turned off there once; per launch `-c` switches the set on and the rest off |
| OpenCode | yes | `OPENCODE_CONFIG_DIR` (set) + `OPENCODE_CONFIG` (provider and model only) + `OPENCODE_TEST_HOME` (an empty home per instance); MCP tools arrive from the second request on |
| Pi | yes, except MCP | `-ne -ns -np -e … --skill …`; home MCP servers still start, so a box keeps `mcp.json` empty and each instance brings its servers through its own extension (untested) |

In all four, two instances with different sets ran at once on one home, sharing its sessions; no switch changed a configuration file. For Claude, two bookkeeping files changed. **A resumed session takes the set of the instance that resumes it.** Codex and Claude keep the earlier skill list in the history.

## Spec before / after

Before: main 5.14 listed the per-instance switches as "in `--help`, not yet run".
After: main 5.14 names the working combination per agent and its conditions: Codex installs set plugins into the home turned off; OpenCode needs a per-instance home; Pi's MCP goes through the instance's extension. Claude's MCP logs and OpenCode's late MCP are recorded as known limits.
