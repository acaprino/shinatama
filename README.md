# shinatama

<p align="center">
  <img src="docs/assets/branding/shinatama-logo-xenotron-glow.png" alt="Shinatama from Oni beside the Shinatama wordmark" width="800">
</p>

Shinatama: a local-first native desktop workspace in Rust that hosts Claude Code, Codex, OpenCode, Pi, Hermes Agent and DeepSeek Harness in tiled terminal panes and hops a live chat between them (native tiling decided 2026-10-29; v1 hops are seed-only, decided 2026-10-31).

Status: implementation started. On 2026-11-02 the user authorized a **Tauri + Rust
prototype** to evaluate the mockup's visual style. This is an experimental terminal
workspace, not the completed first milestone or a supported agent launcher.

## Run the prototype

Implementation is tracked in the [complete resumable plan](docs/implementation/PLAN.md).
Daodan integration is temporarily deferred at the user's request (2026-11-02);
generic checkup, agent launch, sessions, hop and worktrees do not depend on it.
Run `npm run plan:status` to see the next ready increment; see the
[resume protocol](docs/implementation/README.md) for checkpoints and commit boundaries.
The project picker now remembers recent folders and restores the last available
selection at startup without launching a terminal.

With Rust, Node.js and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) installed:

```sh
npm ci
npm run desktop
```

The borderless window has themed minimize, maximize/restore and close controls.
Drag its title bar to move it; double-click the bar to maximize or restore.
Choose a project folder, then **New terminal**. Two resizable panes host real
shells. Move tabs, hide/restore views, maximize a pane, change the split direction
or select a color profile without restarting a process. Stopping a live terminal
or closing the window asks for confirmation. No terminal starts on launch.

Only the split direction and proportion are restored. **Profiles are colors, not
agent launchers.** Agent adapters, isolated homes, sidecar routing, search, hop and
Learning are not wired up yet. Windows is the only platform exercised so far.

See the [prototype scope, dependencies and limits](docs/implementation/tauri-prototype.md)
and [verification evidence](docs/verification/2026-11-02-tauri-terminal-prototype.md).

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
