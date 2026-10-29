# D15 — `private` mode: no writes outside the private home, login, Daodan

Item: D15 (G0): "`private` mode (main 5.14): each agent, started with only its home variable pointing at a private home, writes nothing to the user's home; its login persists across restarts in that home; Daodan installs and enables there." Pass when: "Per agent: zero writes outside the private home; login persists; Daodan enabled; or the leak recorded and the agent marked `system`-only."
Status: **running (partial)**. Windows only. Login persistence needs the user's sign-in in each private home (O1).
Date: 2026-10-28.
Runner: Claude Code session (Opus 5.5), one worker per agent. Mock model only, no login, no credential content read, cost 0.
Machine: Windows 11 Home build 26200; PowerShell 7; Node v24.14.1.

**Method (decoy home).** `USERPROFILE`, `HOME`, `APPDATA` and `LOCALAPPDATA` point at an empty decoy folder. The agent's home variable points at a private home inside it (`decoy\localappdata\Shinatama\agents\<agent>\home`). The mock model and a refusing proxy or egress sink come from the H27 bench. Each run has a plain turn, a tool turn ("H27-TOOLS") and a resume turn. Afterwards every file under the decoy is listed (relative path and size), and the real home is checked. Inherited Claude markers are removed from every child.

## Codex 0.164.0 (`verify/D15/codex/run.ps1 [-Sandbox unelevated]`, `daodan.ps1`, `lib.ps1`)

- **No sandbox: PASS.** 74 files under the decoy, 73 in the private home (SQLite stores, rollout, `.sandbox\sandbox.<date>.log`, system skills). The one outside is `AppData\Local\Microsoft\PowerShell\StartupProfileData-NonInteractive`, written by the PowerShell that the shell tool started (inference). Real `~\.codex`: 0 changes. Egress: 0.
- **Unelevated Windows sandbox: FAIL.** The decoy stayed clean, but during the tool turn Codex created `C:\Users\alfio\.sbx-denybin\{ssh,scp}.{bat,cmd}` (4 × 26 bytes, 20:18:23) in the **real** profile, ignoring `USERPROFILE`. The binary's strings show "no home dir" next to `.sbx-denybin`. The folder did not exist before; it was removed after the run. The stub bodies contain a literal `\r\n` rather than line breaks.
- **Elevated sandbox: NOT RUN.** It needs UAC and changes the machine. From the strings, it creates the OS users `CodexSandboxOffline` and `CodexSandboxOnline`, the group `CodexSandboxUsers`, firewall rules and WFP filters. That is outside any home by design.
- **Login storage** (strings and `codex doctor`, names only): `cli_auth_credentials_store` = `file` | `keyring` | `ephemeral`. The file backend uses `<CODEX_HOME>\auth.json`. The keyring entry is service `Codex Auth`, account `cli|…`; whether the account is keyed per home is an inference. MCP OAuth tokens go under service `Codex MCP Credentials`. In the private home, `login status` prints "Not logged in". Persistence: **pending user**.
- **Daodan: PASS.** `codex plugin marketplace add D:\Projects\daodan`, then `codex plugin add project-delivery@daodan`. 41 files landed in `<home>\plugins\cache\daodan\project-delivery\1.0.1\`, plus `[marketplaces.daodan]` and the enable line in the private `config.toml`. Its skills reached the model request. Nothing was written outside the home, and the checkout was unchanged. The marketplace entry holds the checkout's path, so the checkout must stay where it is.
- **Shared daemon.** The control socket is `<CODEX_HOME>\app-server-control\app-server-control.sock`, with state in `<CODEX_HOME>\app-server-daemon\`. Both are derived from `CODEX_HOME`, so a private Codex cannot attach to the user's daemon (names listed only; the real daemon was not contacted). The private daemon itself failed to start: `path must be shorter than SUN_LEN`. The socket path was 163 characters; Windows Unix sockets allow about 108. **Shinatama's private `CODEX_HOME` must be short** (inference: at most about 60 characters).

## Pi 1.4.0 (`verify/D15/pi/run.ps1 [-ContainCompileCache]`, `bench.ps1`)

- **Session: PASS.** A new session, a tool turn with `--session <id>`, then `-c`, then an online `-c` (the dead proxy still on). All exited 0 and all four turns are in one session file. The mock received 6 requests, all to 127.0.0.1.
- **Default run: FAIL.** 27 files appeared outside the home, all in `%TEMP%\node-compile-cache\v24.14.1-x64-…`. Pi's bundle calls `enableCompileCache()` (`dist/bundle/cli.js`).
- **With `NODE_COMPILE_CACHE` set inside Shinatama's folder: PASS.** 0 leaks. The private home holds `auth.json` (2 bytes), `models-store.json`, `models.json`, `settings.json` and `sessions\…`.
- **Read leak (from source):** `~\.agents\skills` is read from `HOME`/`homedir()` regardless of `PI_CODING_AGENT_DIR`, so the user's own skills would load. The fix is `--no-skills`, or setting `HOME` for the child.
- **Login storage** (source and docs): `<agentDir>/auth.json`, and `<agentDir>/mcp-auth.json` for MCP. Default agent folder: `~/.pi/agent` on all three operating systems. No keychain. Persistence: **pending user**.
- **Daodan: PASS** (local path). `pi install D:\Projects\daodan` exits 0 and adds `"packages":["D:\\Projects\\daodan"]` to the private `settings.json`; nothing is copied. The next system prompt grows from 2,932 to 55,463 characters. The `git:` route needs the network and was skipped.
- Real `~\.pi\agent`: newest file unchanged (`2026-10-28T19:07:48Z`) before and after.

## Claude Code 2.1.312 (`verify/D15/claude_code/run.ps1`)

- **Session: PASS.** `TEMP` was also pointed into the decoy. Four `-p` turns ran on one session id (plain, a tool turn, a Bash turn, `--resume`), all exit 0, plus one 30 s interactive turn in a hidden console.
- **No writes to the user's home or AppData: PASS.** The decoy home, appdata and localappdata stayed empty.
- **No writes outside the private home: FAIL, minor.** `temp\claude\fswatch-probe-*\probe` appeared three times, 1 byte each. (inference, untested) `CLAUDE_CODE_TMPDIR` pointed at the private home would contain it.
- **Private home:** 19 files: `.claude.json`, `backups\`, `history.jsonl`, `settings.json`, the transcript under `projects\<cwd>\`, `sessions\<pid>.json` + `.key`, and `plugins\…`. Nothing appeared in the work folder.
- **Network.** The `-p` turns made 0 attempts. The interactive turn tried `downloads.claude.ai` and `github.com` even with `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`: that is the official marketplace auto-install (`officialMarketplaceAutoInstall*` in `.claude.json`). The binary also contains `CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL` (untested).
- **Login storage** (binary strings):
  - Windows and Linux: `<CLAUDE_CONFIG_DIR>\.credentials.json`, in plain text (the binary warns "Storing credentials in plaintext"). `CLAUDE_SECURESTORAGE_CONFIG_DIR` overrides the folder.
  - macOS: Keychain service `Claude Code<suffix>-credentials`, with `-<first 8 hex of sha256(config dir)>` appended when `CLAUDE_CONFIG_DIR` is set. Each private home therefore has its own entry.
  - Persistence: **pending user**.
- **Daodan: PARTIAL.**
  - Adding the marketplace from `D:\Projects\daodan` worked, recorded only in the private home (`settings.json`, `plugins\known_marketplaces.json`).
  - `install daodan@daodan` failed: `Dependency "mattpocock-skills@mattpocock" (required by testing@daodan) not found. Is the "mattpocock" marketplace added?` Daodan depends on 8 plugins from other GitHub-hosted marketplaces, so a full offline install is impossible.
  - `text-humanizer@daodan` installed and was enabled under the private home's `plugins\cache\daodan\…`; nothing was written to the decoy.
- **Runner incident:** the first run wrote into the H27 scratch home because of a script bug. Those files were removed and the run repeated.

## OpenCode 2.1.4 (`verify/D15/opencode/run.ps1`, `lib.ps1`)

- **Session:** plain turn, a tool turn (H27-FINAL), a resumed turn, and export; all worked. All runs used `run --standalone`.
- **No writes outside the private home: PASS, except temp.** The decoy received only Bun's runtime files in `%TEMP%` (`.bun-*.dll` 5.3 MB, `.bun-*.node` 518 KB). OpenCode's own temp folder is `%TEMP%\opencode`, and none of OpenCode's variables move it. Shinatama must set `TEMP`/`TMP` for the child as well.
- **Isolating variables:** `OPENCODE_DB` + all four `XDG_*` (+ `OPENCODE_TEST_HOME`, `OPENCODE_CONFIG`). No narrower set works (D17).
- **Background service.** Any command other than `run --standalone` (for example `plugin list`, or the TUI) starts a background `serve --service` from the private environment. The service outlives the CLI, writes `service.json` and keeps a 247 MB copy of the binary in the private cache. Shinatama must own it: start it per private home, stop it with the instance, and never let a private launch meet the user's own service. The worker stopped every service its runs had started.
- **Login storage** (source at upstream v2.1.4; Windows path observed): in 2.x the login lives in a `credential` table inside `opencode.db` in the data folder. A v1 `<data>/auth.json` is imported once, by migration `20260805200742_import_legacy_credentials`. The data folder is `$XDG_DATA_HOME/opencode`, otherwise `~/.local/share/opencode`, on all three operating systems. Persistence: **pending user**.
- **Daodan: NOT RUN.** `opencode plugin add D:\Projects\daodan\exports\opencode` is refused: "Plugin target must be an npm registry package or Git package specifier". The documented `github:` target needs the network. (inference) Copying the package into `<config>/plugins/` might work offline.
- **Side effect on the real home** (a runner mistake, not OpenCode in private mode): three "cli starting" lines (19:28:01Z–19:28:28Z) went into the real `%USERPROFILE%\.local\share\opencode\log\opencode.log`. The cause was npm's `postinstall` running `opencode --version` with the normal environment, and one version check of the runner's. The real database was not touched (last change 10:24). The D16 script now passes the private variables to npm and to every version check, and the rerun added nothing.

## Criterion

Not yet met; no agent is `system`-only. Every agent stays out of the user's home **only when Shinatama sets more than the home variable**:

| Agent | Leak with only the home variable | What contains it |
|---|---|---|
| Claude Code | 3 × 1-byte probe files in `%TEMP%`; marketplace auto-install over the network | `TEMP`/`TMP` (or `CLAUDE_CODE_TMPDIR`) to the private folder; `CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL` (untested) |
| Codex | the Windows sandbox writes `~\.sbx-denybin` in the real profile; the elevated sandbox creates OS users and firewall rules; the daemon fails on a long home path | a short private home path; the unelevated sandbox contained (sandbox off, or `USERPROFILE` for the child, untested); the elevated sandbox cannot be private |
| OpenCode | Bun runtime files and `%TEMP%\opencode`; a background service that outlives the CLI; reads `~/.claude` skills | `TEMP`/`TMP`; Shinatama owns the service per private home; `OPENCODE_DISABLE_CLAUDE_CODE=1` |
| Pi | `%TEMP%\node-compile-cache`; reads `~\.agents\skills` | `NODE_COMPILE_CACHE` and `TEMP`/`TMP`; `--no-skills` or `HOME` for the child |

Login persistence is **pending user** for all four. That needs one sign-in per private home, under the accounts decided in O1. Daodan installs into the private home for Codex and Pi (local path). For Claude only single plugins install offline: the full set needs its dependency marketplaces from the network. OpenCode accepts only npm or git targets.

## Spec before / after

Before: main 5.14 said the home variable alone gives a private home.
After: main 5.14 lists the extra variables and flags above per agent, as what a **box** takes. On the same day, after these runs, `private` mode was split into a program choice and a home choice. These runs used the system programs, so they are box runs. The same section shows Codex's sandbox leak to the user and says that its elevated sandbox is not box-safe.
