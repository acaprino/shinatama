# D21 — Bundled plugins activate in every harness launch

Item: D21 (G1): the package ships Ponytail and i-have-adhd and every real session of Claude Code, Codex, Pi and OpenCode loads both, per launch, with no manual step.
Status: **running (partial)**. Windows only.
Date: 2026-11-11.
Runner: Claude Code session (Sonnet 5.5). No provider request from Shinatama; CLIs run only for listing.
Machine: Windows 11 Home build 26200; PowerShell 7.

**Unit evidence** (`cargo test -p shinatama-adapter -p shinatama-engine -p shinatama-desktop`, `TEMP` outside any Git project): all pass, including
`bundled_plugins` (pinned files verify, exact set, tampered file refused, unlisted file refused, Codex cache idempotent and `-c` text),
`claude_launch::bundled_plugins_become_plugin_dir_pairs`, `pi::sets::bundled_plugins_wire_extensions_and_skills_in_fixed_order`,
`opencode_launch::bundled_shim_is_a_file_url_reexport`.

**Live evidence, the CLIs against the staged package files:**

| Harness | Result |
|---|---|
| Claude Code | With the launcher's own flags (`--setting-sources project,local --settings --mcp-config --strict-mcp-config`) plus two `--plugin-dir`, the `stream-json` init lists `ponytail:*` (6 skills), `i-have-adhd:i-have-adhd` and both plugins. With `CLAUDE_CODE_SAFE_MODE=1` no plugin skill loads, so activation is limited to tracked sessions. `claude plugin validate` passes for both. |
| Codex | In a fresh `CODEX_HOME`, `-c marketplaces...` and `-c plugins=...` alone: `plugin list` says "not installed". With the files also placed in `plugins/cache/shinatama-bundled/<plugin>/<version>`: "installed, enabled" (ponytail 5.3.3, i-have-adhd 0.4.1). `codex exec` could not reach the provider here, so no in-session skill listing. |
| Pi | `-e` for both extensions and `--skill` for both skill directories: Ponytail's six skills listed; `/skill:i-have-adhd` answers in the ADHD style. The extension command `/i-have-adhd` in `-p` mode is read as a path. |
| OpenCode | Not runnable here: the app reports `Location plugins did not finish activating` in an empty directory and `ECONNRESET` with `--standalone`. Plugin files only pass `node --check` and import. |
| Gemini | No launch adapter. The `gemini.toml` command parses; the CLI is not authenticated. |

**Open:** a session started from the packaged desktop app per harness (needs the GUI), Codex hook trust for Ponytail, OpenCode in a working environment, `node` availability for the plugins' hooks.
