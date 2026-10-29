# D18 — `system` mode: finding the user's installed agents (partial, Windows)

Item: D18 (G1): "`system` mode (main 5.14): the user's binary and home are found on 3 OS (PATH, npm prefix, Homebrew, winget, Claude's native install folder). Shinatama never writes the binary. A version outside the proved range runs degraded, with the reason shown. The user's sessions are listed read-only while the agent is `private`. Switching mode moves nothing."
Status: **running (partial)**. The Windows discovery is recorded. macOS, Linux and every behaviour that needs Shinatama itself (never writing the binary, degraded state, switching) are open.
Date: 2026-10-28.
Runner: Claude Code session (Opus 5.5). Read-only, no home file content read, cost 0.
Machine: Windows 11 Home build 26200; PowerShell 7.
Command: `pwsh -NoProfile -File verify/D18/run.ps1` (rerunnable without edits; removes inherited Claude markers before starting any child).
Raw output: `%LOCALAPPDATA%\shinatama-verify\D18\discovery.txt`.

## Result

| Agent | Version | Binary on PATH | Owner | Its own update command | Home (present) |
|---|---|---|---|---|---|
| Claude Code | 2.1.312 | `~\.local\bin\claude.exe` | Claude native installer; it keeps versions side by side in `~\.local\share\claude\versions\` (2.1.310, 2.1.311, 2.1.312) | `claude update` (`install` also exists) | `~\.claude\`, `~\.claude.json` |
| Codex | 0.164.0 | `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin\codex.exe` (plus `codex-code-mode-host.exe`) | Codex native installer | `codex update` | `~\.codex\` |
| OpenCode | 2.1.4 | `%APPDATA%\npm\opencode{,.cmd,.ps1}`, which runs `node_modules\@opencode\cli\bin\opencode.exe` | npm, global prefix | `opencode upgrade` | `~\.local\share\opencode`, `~\.config\opencode`, `~\.cache\opencode`, `~\.local\state\opencode` |
| Pi | 1.4.0 | `%APPDATA%\npm\pi{,.cmd,.ps1}`, which runs `node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js` | npm, global prefix | `pi update self` | `~\.pi\` |

- None of the four has a winget entry.
- Pi is 1.4.0 here. The register and the technical design still quote 1.2.0, so Pi's agent rows need a rerun (register 0, "Version drift").

## Observations that change the design

1. **Four agents, three owners**: two native installers with their own update commands, and npm for the other two. In `system` mode the update notice must name the owner's command. For npm that is `npm install -g <package>@latest` with the package read from the shim target (`@opencode/cli`, `@earendil-works/pi-coding-agent`), not guessed from the agent's name. For the native installers it is `claude update` or `codex update`.
2. **Each agent has its own update command, and the agent itself can run it.** In `private` mode this is a second path to self-update, besides the automatic check. D16 has to cover both:
   - the automatic check turned off;
   - the agent's update command refused, or pointed at Shinatama's private copy.
3. **Codex has a shared local daemon.** Its help lists `codex agents`, "Browse all agent sessions on the shared local app-server daemon", and `remote-control`, "Manage the app-server daemon". A daemon already started with the user's environment could serve a private instance. That would break isolation the same way the H27 bench found for OpenCode's background service ("always pass `--standalone` or `--server`"). D15 must check this for Codex.

## Criterion

Partially met: Windows discovery works from PATH, the npm prefix and the native install folders. Open:
- macOS and Linux (Homebrew, the npm prefix, `~/.local/bin`);
- everything that needs Shinatama: binary checksum unchanged, the degraded state, listing sessions read-only, switching modes.

## Spec before / after

Before: D18 open.
After: unchanged in status. The update notice in `system` mode depends on the owner, as in observation 1. Observations 2 and 3 are handed to D16 and D15.
