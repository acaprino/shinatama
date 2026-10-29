# D17 — `XDG_*` for a private OpenCode: what its children see

Item: D17 (G1): "`XDG_*` for a private OpenCode: what the programs it starts read when `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the others point at the private home; whether a narrower variable suffices." Pass when: "The user's git and tool configuration still found, or the variables narrowed until it is."
Status: **changed** (passed with the design adjusted). Windows only.
Date: 2026-10-28.
Runner: Claude Code session (Opus 5.5), one worker. Mock model, git configuration *names and origins* only, cost 0.
Machine: Windows 11 Home build 26200; PowerShell 7; Node v24.14.1; npm 11.11.0. Agent: opencode 2.1.4 (npm `@opencode/cli`, a Bun-built binary of 246,919,560 bytes). Source read at upstream tag v2.1.4. The `v2.1.4\source` folder that H27 cites has been modified by JanuraCode (`JCODE_*` names); the installed binary matches upstream.
Scripts: `verify/D17/opencode/run.ps1`, `server.shell.mjs` (a copy of the H27 mock with a `SHN-SHELL <command>` trigger), `dump-env.mjs`, `shn-restore-env.js`. Raw output: `%LOCALAPPDATA%\shinatama-verify\D17\opencode\report.txt`.

## Result

1. **A narrower variable set does not isolate OpenCode.**
   - `OPENCODE_DB` + `OPENCODE_CONFIG_DIR` + `OPENCODE_TEST_HOME` fails: data, log, cache, bin and state go to `%USERPROFILE%\.local\share|state\opencode` and `.cache\opencode`. `OPENCODE_DB` alone fails the same way.
   - The source has no other override for those folders. The working set is `OPENCODE_DB` + all four `XDG_*`, plus `OPENCODE_TEST_HOME` and `OPENCODE_CONFIG` (D15).
2. **OpenCode passes `XDG_*` unchanged to its shell tool's children.** The child environment is `{...launch environment, TERM, OPENCODE_TERMINAL}` plus OpenCode's markers (P20).
3. **Effect on git.**
   - On this machine git finds the same configuration origins as in a fresh terminal: `C:/Program Files/Git/etc/gitconfig` (13 keys) and `~/.gitconfig` (3 keys). That holds only because the user keeps the configuration in `~/.gitconfig`.
   - With a decoy home holding `~/.config/git/config`, the private `XDG_CONFIG_HOME` **hides that file**: the probe key `shn.xdgprobe` disappears.
   - npm is unaffected: it reads `~/.npmrc` and its own cache path.
4. **The fix that works.** Keep `XDG_*` for OpenCode itself and add a small private OpenCode plugin. The plugin hooks the shell tool's `create.before` and restores the user's original `XDG_*` for the child (`verify/D17/opencode/shn-restore-env.js`). Observed: OpenCode stays private, children get no Shinatama `XDG_*`, and git's origins match the fresh terminal.

## Criterion

Met with a design change. The user's git and tool configuration is found when the restoring plugin is installed in the private home. Without it, a user who keeps git, or any other XDG-aware tool, under `~/.config` would lose that configuration inside OpenCode's shell tool.

## Spec before / after

Before: main 5.14 assumed the `XDG_*` effect on children was unknown; register D17 open.
After: main 5.14 and engine 3 say that a private OpenCode needs `XDG_*` (no narrower set exists in 2.1.4), plus Shinatama's restoring plugin, which gives children back the user's own values.
