# D16 — `private` mode: obtaining each agent, self-update off, side by side, rollback

Item: D16 (G0): "Obtaining each agent into the private folder and keeping it there: source per agent; turning off the agent's own self-update; a second version installed side by side while an instance runs (locked executables on Windows); going back to the previous version." Pass when: "One install and update recipe per agent; no self-update observed in a week; side-by-side and rollback work with an instance running."
Status: **running (partial)**. Windows only. The "one week" observation and macOS and Linux are open.
Date: 2026-10-28.
Runner: Claude Code session (Opus 5.5), one worker per agent. Mock model; network only for the npm installs; cost 0.
Machine: Windows 11 Home build 26200; PowerShell 7; Node v24.14.1.

## Codex (`verify/D16/codex/run.ps1`)

- **Install: PASS.** Command: `npm install --prefix …\versions\<ver> --cache …\npm-cache @openai/codex@<ver>`, for 0.164.0 and 0.163.0.
  - Each folder holds the `codex.js` wrapper, the native `codex.exe` and `rg.exe`, `codex-windows-sandbox-setup.exe` and its helpers.
  - Nothing was written outside the folder.
  - The other source is the GitHub release asset `codex-x86_64-pc-windows-msvc.exe(.zip)`. It was listed, not downloaded.
- **Each version runs from its folder: PASS.**
- **Side by side: PASS.** 0.164.0 stayed running on a hung turn while 0.163.0 was installed and ran two turns.
- **Rollback: PASS.** 0.163.0 resumed a thread written by 0.164.0 in the same home, and 0.164.0 ran on it again afterwards.
- **Self-update off: NOT PROVEN, with three channels.**
  1. *Startup check.* The setting is `check_for_update_on_startup` in `config.toml`; the cache is `<home>\version.json`. In 15 s of the interactive UI, no update request was made with the setting either true or false. So the run does not show what the setting changes.
  2. *`codex update`.* It ignores the setting. On an npm install it runs `npm install -g @openai/codex`, which targets the **global** npm prefix, not the private folder. In the test npm was redirected to a scratch prefix and the sink, so nothing was installed. On a standalone install it runs `irm https://chatgpt.com/codex/install.ps1 | iex`; that was not run.
  3. *Daemon update loop.* Each interactive run copied Codex into `<home>\packages\app-server-daemon\releases\0.163.0-…`, with an `auto-update-version` file. The strings show a loop that fetches `install.ps1`, and the settings `auto_update` and `update_interval` in `app-server-daemon\settings.json`. Turning it off is not verified.
- **The user's standalone install already does its own versioning.** `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin` is a junction to `~\.codex\packages\standalone\current\bin`, and `current` points into `releases\<ver>`; 13 versions are kept.

## Pi (`verify/D16/pi/run.ps1 [-KeepInstalls]`, `logproxy.mjs`)

- **Install: PASS.** `npm install --prefix …\versions\<ver> @earendil-works/pi-coding-agent@<ver>`, for 1.4.0 (29 s) and 1.2.0 (22 s): 121 packages and about 15,150 files each. Both declare `engines: node >=22.19.0`.
- **What npm writes outside the folder** (seen through a decoy profile): `npm-cache\_cacache` (486 files), `_logs` (2), `_update-notifier-last-checked`, and `%TEMP%\node-compile-cache` (538 files). (inference, untested) Setting `npm_config_cache`, `npm_config_update_notifier=false` and `NODE_COMPILE_CACHE` would keep them in Shinatama's folder.
- **Runs as `node.exe …\cli.js` with no shim: PASS.**
- **Self-update: PARTIAL.**
  - From source, Pi never updates itself automatically; only an explicit `pi update` does.
  - Interactive mode makes four network calls: a version check (`https://pi.dev/api/latest-version`), a package update check, install telemetry (`https://pi.dev/api/report-install`) and a model-catalog refresh.
  - What turns them off: `PI_OFFLINE=1` or `--offline` (which also sets `PI_SKIP_VERSION_CHECK`), or `PI_SKIP_VERSION_CHECK` alone. Telemetry also stops with `PI_TELEMETRY=0` or `enableInstallTelemetry:false`.
  - Observed: print mode, online and offline, on both versions, made 0 proxy attempts. Interactive mode without a terminal made 0 attempts in 12 s, but that does not show the check was reached.
- **Side by side: PASS.** A 1.4.0 process in RPC mode kept running while 1.2.0 was installed and run, then answered a second prompt. Both folders carry native `pi-tui` `.node` prebuilds, so an update in place could hit locked files. Pi has its own quarantine code and a managed-release layout (`PI_MANAGED_INSTALL_ROOT`, `releases-v1`); both read from source, not tested.
- **Rollback: PASS.** 1.2.0 resumed with `-c` a session written by 1.4.0 (session format v3 in both).

## Claude Code 2.1.312 and 2.1.311 (`verify/D16/claude_code/install.ps1`, `run.ps1 [-VersionsRoot <dir>]`)

- **Install into any folder: PASS.** `npm install --prefix <dir> @anthropic-ai/claude-code@<ver>` takes 7–8 s. The package pulls its native binary from `@anthropic-ai/claude-code-win32-x64`, and a postinstall step puts it at `bin\claude.exe`. The npm 2.1.312 binary is byte-identical to the native installer's. `claude install [target]` has only `--force`, no folder option. Outside the folder, the install wrote only `%TEMP%\node-compile-cache` (538 files); the npm cache was given its own folder with `--cache`.
- **Self-update off: PASS.** Interactive 2.1.312 ran for 60 s per run, with every connection blocked by the trap proxy.
  - With `DISABLE_AUTOUPDATER=1`, it attempted raw.githubusercontent.com, api.anthropic.com, downloads.claude.ai and github.com, but not the npm registry.
  - Without it, the same plus `registry.npmjs.org` twice. At that moment a Node/npm process wrote 412 files to the decoy `%TEMP%`: Claude started npm to update itself. The redirected npm prefix stayed empty.
  - `DISABLE_UPDATES` also exists; with it, `claude update` refuses.
- **Side by side: PASS.** While 2.1.311 ran, 2.1.312 was installed into a new folder and ran a turn (exit 0). 2.1.311 was still alive afterwards.
- **Installing over a running binary:** npm exits 0 and the folder reports the new version while the old instance keeps running. npm cannot delete the running exe (`EPERM … unlink …\.claude-code-KloD6h6p\bin\claude.exe`) and leaves a stray folder. This confirms that updates go into a new folder, never in place.
- **Rollback: PASS.** 2.1.311 ran a turn afterwards (exit 0).
- **Runner incidents:** drive C: filled during the run, so the version folders were moved to D: and deleted at the end. These results come from a run started after the 20:30 process kills.

## OpenCode 2.1.4 (`verify/D16/opencode/run.ps1`)

- **Install: PASS for 2.1.4.** `npm install --prefix …\versions\2.1.4 @opencode/cli@2.1.4` takes 8.7 s. Outside the folder it writes the npm cache under `%LOCALAPPDATA%\npm-cache` (including `_logs`). Its `postinstall` runs `opencode --version`, so npm must also get the private-home variables (D15).
- **Self-update off: PASS.** The update check runs only in the TUI. With `OPENCODE_DISABLE_AUTOUPDATE=1` the log says `update check skipped reason=disabled`; with the config `"update":"disable"` it says `reason=policy-disabled`.
- **Why it must be off** (inference from the updater source, not run): left on, the updater detects "npm" through `npm list -g`, finds the user's global `@opencode/cli` and runs `npm install --global …`. A private copy would upgrade the user's global install. The version server already offers 2.2.0.
- **Side by side and rollback: NOT FINISHED.** Installing 2.1.3 failed with `ENOSPC`: drive C: was full (199 MB free), partly from these runs' install folders. The script is ready to rerun with about 1 GB free.
- **Runner incident:** a cleanup filter matching `*shinatama-verify\D16*` killed three processes of the Claude Code run. The filter is now narrowed.

## Criterion

Partly met.

| Agent | Install recipe | Self-update off | Side by side | Rollback |
|---|---|---|---|---|
| Claude Code | npm `--prefix` (binary identical to the native installer) | yes: `DISABLE_AUTOUPDATER=1` + `DISABLE_UPDATES=1` | yes | yes |
| Codex | npm `--prefix` or the release asset | **no**: startup check unclear; `codex update` ignores the setting and targets the global npm prefix; the daemon has its own update loop | yes | yes |
| OpenCode | npm `--prefix` | yes: `OPENCODE_DISABLE_AUTOUPDATE=1` (left on, it would update the user's global install) | not finished (disk full) | not finished |
| Pi | npm `--prefix`, run as `node.exe …\cli.js` (Node ≥ 22.19.0) | yes: `PI_OFFLINE=1` / `PI_SKIP_VERSION_CHECK`, `PI_TELEMETRY=0` (it never updates itself automatically) | yes | yes |

Common to all four: npm writes its cache and `%TEMP%\node-compile-cache` outside the install folder unless `npm_config_cache`, `NODE_COMPILE_CACHE` and `TEMP` point inside Shinatama's folder. Installing over a running binary fails or leaves debris, so a version folder is never reused. Open: Codex's three update channels; OpenCode's side-by-side and rollback (rerun with 1 GB free); macOS and Linux; the one-week observation.

## Spec before / after

Before: main 5.14 said "the agent's own self-update is turned off" without saying how.
After: main 5.14 names the switch for each agent. It says that, for an `own` program (the later name of this part of `private` mode), the agent's own update command must be blocked as well. It also says that an `own` Codex waits for D16.
