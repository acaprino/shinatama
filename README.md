# shinatama

<p align="center">
  <img src="docs/assets/branding/shinatama-logo-xenotron-glow.png" alt="Shinatama from Oni beside the Shinatama wordmark" width="800">
</p>

Shinatama: a local-first desktop workspace, with a Rust engine and a Tauri shell, that hosts Claude Code, Codex, OpenCode, Pi, Hermes Agent and DeepSeek Harness in tiled terminal panes and hops a live chat between them (native tiling decided 2026-10-29; v1 hops are seed-only, decided 2026-10-31).

Status: implementation started. Windows is the only officially supported platform for now, and
Tauri is the definitive V1 shell. The
current build includes contained shells and an explicitly checked Claude TUI
startup path. It remains an incomplete V1; provider/model traffic, other agents,
session search and hop are not certified by opening that TUI.

## Run the prototype

For the compiled local Windows build, double-click `Shinatama.cmd` and follow the
[quick start in Italian](AVVIO.md): project → Session index / Start → Agents /
Claude Code / Check / Open agent. The qualified startup slice uses safe mode and
an empty set; sign-in stays in Claude's own terminal interface.

Implementation is tracked in the [complete resumable plan](docs/implementation/PLAN.md).
Daodan integration is temporarily deferred at the user's request (2026-11-02);
generic checkup, agent launch, sessions, hop and worktrees do not depend on it.
Run `npm run plan:status` to see the next ready increment; see the
[resume protocol](docs/implementation/README.md) for checkpoints and commit boundaries.
The project picker stores multiple roots and an explicit default, filters and
pages their first-level folders, and opens any folder with the native F3 picker.
It remembers the last ten outside folders and restores the last available
selection at startup without launching a terminal. State lives in per-user
SQLite, with verified migration backups and bounded idle maintenance that
preserves saved work. [Project picker evidence](docs/verification/2026-11-02-C04-project-catalog.md)
and [maintenance evidence](docs/verification/2026-11-02-C03M-desktop.md) cover the
current local Windows build; full resource resilience remains a later gate.
In the project picker, **New project** or `n` creates a folder in an explicitly
chosen root, with optional Git initialization on `main`. Invalid or duplicate
names leave existing work untouched; interrupted nonempty preparation is kept
with a recovery path. No terminal starts automatically.

With Rust, Node.js and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) installed:

```sh
npm ci
npm run desktop
```

The borderless window has themed minimize, maximize/restore and close controls.
Drag its title bar to move it; double-click the bar to maximize or restore.
Choose a project folder, then **New terminal**. Split panes in any direction,
move tabs, hide/restore views, maximize a pane, search its retained terminal text
or select a color profile without restarting a process. Stopping a live terminal
or closing the window asks for confirmation. No terminal starts on launch.

The full project workspace tree, proportions, visibility and presentation settings
are saved. Reload reconnects existing live terminal IDs; saved views never replay
commands. Five application skins and per-terminal profiles change presentation.
**Profiles are colors, not agent launchers.** Use Agents for the qualified Claude
startup path. Session search, hop and Learning are still being connected. Windows
is the supported platform.
Native terminal jobs have fixed limits, a raw process census and finite queues;
uncertain cleanup keeps its reservation. Full release qualification remains open.

For the existing local embedded build, double-click `Shinatama.cmd`. It opens the
current compiled application and requires its adjacent terminal helper; it does
not compile, install anything or launch a terminal automatically.
The independent Rust engine now has versioned adapter contracts and an in-memory
test adapter. `cargo run -p shinatama-cli -- doctor --json` inspects local program
locations without starting agents or creating state. Exit 2 reports missing or
unqualified support; [C10 evidence](docs/verification/2026-11-02-C10-adapter-contracts.md)
describes the current limits.

See the [prototype scope, dependencies and limits](docs/implementation/tauri-prototype.md)
and [verification evidence](docs/verification/2026-11-02-tauri-terminal-prototype.md).

## Verify on Windows

```sh
npm run verify
```

The shared local/CI runner restores locked dependencies, validates the plan and
dependency declarations, builds the embedded application and runs unit, real-PTY,
window and project-restart tests. Install PowerShell 7, Python 3.13, the Windows
Rust/MSVC prerequisites and WebView2 first. Node is pinned in `.node-version`;
`rust-toolchain.toml` pins Rust and formatting/lint components.

[Verification details](docs/implementation/windows-verification.md) describe the
bounded runner and the Windows-only CI. Reports are generated under `.verification/`;
only selected sanitized reports, synthetic screenshots and the unsigned debug
prototype are CI artifacts. Local verification is separate from a remote CI run.

[Third-party inventory](THIRD_PARTY_NOTICES.md) is checked against both lockfiles.
`python scripts/dependencies.py generate` refreshes it after a reviewed dependency
change; `check` rejects stale notices, undeclared dependencies or missing license text.
Full texts accompany artifacts as `.verification/THIRD_PARTY_LICENSES.txt`.

## Design records

- Functional design: [docs/functional-design.md](docs/functional-design.md)
- Technical design (threads, hop, exchange between instances, shared state): [docs/technical-design.md](docs/technical-design.md)
- Browser integration (Moli fast path, explicit Patchright UI, routing, security and lifecycle gates; proposed 2026-10-29): [docs/browser-integration.md](docs/browser-integration.md)
- Mukade integration for native session handover (API, host responsibilities and gates; pinned at Mukade `4124b1f`, with the revisions after the pin listed in its section 8): [docs/mukade-handover.md](docs/mukade-handover.md)
- Native support for Daodan (decided 2026-10-27; proposed, not approved): [docs/daodan-native-support.md](docs/daodan-native-support.md)
- Session import/export: the library is now its own repository, [mukade](https://github.com/acaprino/mukade), which ships the spec ([session-interchange-spec.md](https://github.com/acaprino/mukade/blob/master/docs/session-interchange-spec.md)) and the tool tables ([tool-conversion-tables.md](https://github.com/acaprino/mukade/blob/master/docs/tool-conversion-tables.md)); technical design 5.4.1, register row H27 (its original Mukade record was removed in that repository's documentation reorganization; Mukade's current verification records are indexed in its [docs/README.md](https://github.com/acaprino/mukade/blob/master/docs/README.md#verification-records)).
- What still has to be proved by running something: [docs/verification-register.md](docs/verification-register.md)
- Requirements, one readable record per area with origin, delivery slice and proof row (FRD-0001 to FRD-0017, Proposed): [docs/frd/README.md](docs/frd/README.md)
- Decisions, one record per version (ADR-0001 to ADR-0050; reading map and coherence audit in the index): [docs/adr/README.md](docs/adr/README.md)

Naming (decided 2026-10-26): the project is called `shinatama` (earlier suffixes `-sld` and `-mux` were dropped). The obsolete Daodan marketplace checkout (`acaprino/shinatama`, register row D1) was deleted on 2026-10-26; the local folder is renamed to `D:\Projects\shinatama` once JanuraCode is not holding it, because its services keep `.jcode` open.

D1 (closed 2026-10-27, [record](docs/verification/2026-10-27-D1-marketplace-name-collision.md)): the **remote** `acaprino/shinatama` repository turned out to be public and alive (MIT; last push 2026-10-25; 1 011 commits, 17 tags, 24 plugins) although the local checkout was gone. The user decided that only Daodan is the plugin marketplace, so the remote repository was **deleted on 2026-10-27** after a verified bundle backup (`C:\Users\alfio\Backups\acaprino-shinatama-2026-10-27.bundle`, plus a mirror clone beside it). Every one of its 24 plugins exists in Daodan at an equal or newer version. The name collision with this product is gone. Left behind: prose that still names the repository in Daodan's `README.md` and `AGENTS.md` (history of marketplace 36.0.0) and a dead link in Mukade's `README.md` line 8.

Marketplaces (user, 2026-11-01): the restriction to Daodan alone is lifted; any marketplace may be registered, including the ones Daodan declares as dependencies ([daodan-native-support.md](docs/daodan-native-support.md) 4.6).
