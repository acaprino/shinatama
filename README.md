<h1 align="center">
  <img src="docs/assets/branding/shinatama-logo-xenotron-glow.png" alt="Shinatama" width="800">
</h1>

<p align="center"><strong>A desktop workspace for terminal coding agents.</strong></p>

<p align="center">
  <a href="#quickstart">Quickstart</a> ·
  <a href="#supported-harnesses">Supported harnesses</a> ·
  <a href="#ecosystem">Ecosystem</a> ·
  <a href="#development">Development</a>
</p>

Run Claude Code and Codex side by side, keep a PowerShell pane for your own
commands, and hand off the relevant conversation context when you switch agents.
Shinatama hosts each harness in its native terminal interface and saves your
workspace layout locally.

**Windows x64 · Early development.** The [verification register](docs/verification-register.md)
tracks which launch, resume, and handoff paths have been exercised.

## What you can do

- **Tile your terminals.** Split panes, move tabs, maximize a view, and search
  terminal output. Shinatama restores your layout when you reopen the app.
- **Keep work organized by project.** Pin repositories, see their Git status,
  return to a thread, or start a session in a new Git worktree.
- **Browse and export conversations.** Open local session archives in read-only
  mode and export conversations as Markdown, STX, or Mukade JSON.
- **Hand off a session.** Use **Hop** to preview the context sent to a new session
  in another harness. The current handoff starts the destination with a prepared
  prompt and leaves the source session intact.
- **See what needs attention.** The widget can take you to the relevant terminal.
  When system memory runs low, it shows the highest-usage terminal it measured
  and keeps existing sessions running. New terminals have a small startup
  memory check. [Memory policy](docs/notes/2026-11-12-interactive-memory.md)

## Quickstart

From a Windows x64 checkout, install the Node version in [`.node-version`](.node-version)
and set up Rust with [rustup](https://rustup.rs/). The repository pins the
[toolchain](rust-toolchain.toml). Install the
[Tauri prerequisites for Windows](https://v2.tauri.app/start/prerequisites/#windows),
including the MSVC C++ build tools and WebView2.

```powershell
npm ci
npm run desktop
```

Select a project folder, then open **New session → PowerShell** to try the
workspace. To run agents from a source checkout, follow the
[sidecar setup](sidecar/source-build/README.md) for the pinned proxy and its
Node runtime. With those resources in place, choose an installed harness from
**New session** and complete any sign-in in its terminal. The portable build
includes the sidecar resources.

If you already have the local debug build and its helpers, double-click
[`Shinatama.cmd`](Shinatama.cmd). See the [Italian quickstart](AVVIO.md) for
the desktop controls and portable package details.

## Supported harnesses

| Harness | New session | Resume from Threads | Browse local archives |
| --- | --- | --- | --- |
| <img src="docs/assets/harnesses/claude-code.svg" alt="" width="24" height="24"> [Claude Code](https://github.com/anthropics/claude-code) | Available | Available | Available |
| <img src="docs/assets/harnesses/codex.svg" alt="" width="24" height="24"> [Codex CLI](https://github.com/openai/codex) | Available | Available | Available |
| <picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/harnesses/opencode-dark.svg"><img src="docs/assets/harnesses/opencode-light.svg" alt="" width="24" height="24"></picture> [OpenCode](https://opencode.ai/) | Available | Planned | Planned |
| <img src="docs/assets/harnesses/pi.svg" alt="" width="24" height="24"> [Pi](https://pi.dev/) | Available | Planned | Available |

**Hop** can send context from Claude Code, Codex, or Pi to a different harness
listed above. Resume depends on an eligible saved thread. Hermes Agent and
DeepSeek Harness are planned integrations.

Every session of the four supported harnesses starts with **Marionette**, the
plugin built by [Muro](https://github.com/acaprino/muro). Shinatama downloads
its latest GitHub release at each start and checks its SHA-256. Set
`SHINATAMA_MARIONETTE=off` to disable it. The local build also has an optional
display mod for supported Claude Code versions. [Marionette](AVVIO.md#marionette) ·
[Claude display mod](resources/claude-mods/README.md) ·
[Logo credits](docs/assets/harnesses/NOTICE.md)

## Ecosystem

| Project | Role |
| --- | --- |
| [**Mukade**](https://github.com/acaprino/mukade) | Rust library and CLI for reading, converting, and moving native agent sessions between harnesses. Shinatama uses Mukade to read sessions. The [handover design](docs/mukade-handover.md) covers native session transfer. |
| [**Muro**](https://github.com/acaprino/acaprinoprivate#readme) | Builds packages from skills and plugins. The host agent selects the capabilities it needs for each task. Repository access required. |

## Development

Shinatama uses **Rust** for the terminal engine, harness adapters, and local
SQLite state, with a **Tauri** desktop shell and a **TypeScript/xterm.js** UI.
See the [desktop app](apps/desktop) and [Rust crates](crates).

```powershell
npm run verify
```

The Windows verification runner builds the app and exercises real PTYs, desktop
controls, project restart, and storage. See the [verification guide](docs/implementation/windows-verification.md)
for prerequisites, individual checks, and CI artifacts.

- [Implementation plan](docs/implementation/PLAN.md) and [progress commands](docs/implementation/README.md)
- [Functional design](docs/functional-design.md) and [technical design](docs/technical-design.md)
- [Requirements](docs/frd/README.md) and [architecture decisions](docs/adr/README.md)
- [Browser integration proposal](docs/browser-integration.md) and [deferred Daodan integration](docs/daodan-native-support.md)

For bug reports, include your Windows version, the harness and its version if
relevant, and the steps to reproduce the issue. [GitHub Issues](https://github.com/acaprino/shinatama/issues)

## License

The Shinatama workspace is currently marked `UNLICENSED`.
See the [third-party notices](THIRD_PARTY_NOTICES.md) for dependency licenses.
