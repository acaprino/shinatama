# Tauri terminal prototype — 2026-11-02

## Authorization

After the previous native-only ADR-0005 decision was identified, the user selected
**“Prototipo Tauri + motore Rust (consigliato)”** on 2026-11-02. This authorizes an
executable experiment, not a claim that native Rust cannot reproduce the style or
that the entire product has passed its gates. Later that day the user made Tauri the
definitive V1 shell (ADR-0005 v2, ADR-0047 v2), which replaces the scoped exception this
note first recorded. ADR-0040's terminal-first sequence still applies.

## Implemented

- `crates/terminal`: independent Rust PTY ownership, binary output, queued input,
  resize, explicit stop and process containment. No session reader, credentials,
  model calls or agent adapter.
- `apps/desktop`: Tauri + TypeScript + xterm, using the Lite mockup's colors and
  proportions. Two panes, tab groups, drag/move between groups, hide/restore,
  maximize, keyboard/pointer splitter, rows/columns and five color profiles.
- Windows launches system PowerShell without the user's profile or saved
  PSReadLine history. Unix uses `$SHELL` or `/bin/sh`. Exact inherited agent identity
  markers are removed; this partial list is not full P20 adapter conformance.
- No process starts automatically. WebView local storage saves only version, axis
  and split proportion. No runtime ID, path, output or process handle is persisted.
  Unknown or corrupt preferences fall back safely.
- The browser preview is labelled and never simulates a working shell. The original
  geometric SVG icon uses no Oni artwork or remote fonts. At the user's request,
  the window is borderless, with themed minimize, maximize/restore and close controls
  and a draggable title area (double-click toggles maximization). Close retains the
  live-terminal confirmation. The later-feature workspace and Git Diff are not introduced.

## Dependency decision

Experiment-specific implementation choices. Exact versions are locked; every direct
Rust dependency is centralized in `Cargo.toml`. This is not the release-wide X3 audit.

| Dependency | Resolved version | License | Purpose |
| --- | --- | --- | --- |
| tauri / tauri-build | 2.12.5 / 2.7.4 | MIT OR Apache-2.0 | Window, local IPC, WebView, build |
| portable-pty | 0.9.0 | MIT | ConPTY and Unix PTYs |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | Kill-on-close process jobs |
| libc | 0.2.191 | MIT OR Apache-2.0 | Unix process-group stop |
| @tauri-apps/api / cli | 2.12.4 | MIT OR Apache-2.0 | Frontend bridge and tooling |
| @xterm/xterm / addon-fit | 6.0.0 / 0.11.0 | MIT | Terminal emulation and sizing |
| Vite / TypeScript | 8.3.5 / 7.0.2 | MIT / Apache-2.0 | Build and type checks |
| @playwright/test | 1.64.1 | Apache-2.0 | Real WebView2 integration test |

serde, anyhow and uuid reuse existing allowances. `shinatama-terminal` is the local
workspace dependency. Node is needed to build, not to run the embedded executable.

## Boundaries and limits

The main window has event-listening, close, minimize, maximize/state and titlebar-drag
permissions and explicit application
commands. No filesystem, shell, HTTP, clipboard or opener plugin is installed. CSP
excludes frames and remote resources. Terminal bytes go to xterm, never HTML. OSC
title/cwd/link/notification/clipboard and window-manipulation sequences are blocked;
terminal protocol replies still work. No external content is embedded.

Limits: eight shells; 512 KiB queued output per shell; 64 KiB drained per poll;
32 input messages of at most 64 KiB each; 5,000 scrollback rows per emulator. The
frontend waits for parsing before polling again. Hidden terminals drain while
retaining their usable size. These are bounds, not production performance claims.

Windows jobs kill on close. Assignment follows spawn and is not atomic before child
execution; crash-at-spawn and containment escape are unverified. Unix explicit stop
signals the process group, but crash-proof supervision is not implemented. P14 stays
open. Programs typed manually use the normal user environment: box homes and the
BillionContext sidecar are not active. Profiles do not imply supported agent adapters.

This is a two-pane experiment, not an arbitrary split tree. Preferences are global
to the prototype profile, not per-project SQLite records. A frontend reload reattaches
processes but loses earlier screen contents. Restart starts no processes. There is
no session recovery, production installer, signing or updater. macOS/Linux, IME,
high DPI and adversarial escape corpora remain unverified.

## Run and verify

```sh
npm ci
npm run desktop
npm run build
npm test
cargo test -p shinatama-terminal
cargo run -p shinatama-terminal --example smoke
```

Build an embedded debug executable without a development server:

```sh
npm run build
cargo build -p shinatama-desktop --features custom-protocol
```

On Windows, run `target/debug/shinatama-desktop.exe` with system WebView2 installed.
`npm run test:desktop` exercises this executable with an isolated test profile,
a loopback-only debugging port and synthetic shell commands, then closes it. Normal
app startup enables no debugging port. The test script is Windows-only.

The user authorized complete staged implementation after validating this prototype.
The [complete plan](PLAN.md) and [checkpoint](progress.json) now govern delivery.
B02 adds persistent recent projects in platform configuration, not the authoritative
session database. First supported agent launch follows storage/location gates,
environment isolation, sidecar and generic checkup rather than bypassing those prerequisites.
The user subsequently deferred Daodan integration: its packages/tasks/seams do not
block supported agent launch or the active session/hop milestones.

References: [Tauri architecture](https://v2.tauri.app/concept/architecture/),
[capabilities](https://v2.tauri.app/security/capabilities/),
[portable-pty](https://docs.rs/portable-pty/0.9.0/portable_pty/).
