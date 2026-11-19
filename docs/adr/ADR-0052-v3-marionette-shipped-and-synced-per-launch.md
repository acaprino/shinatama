---
adr_id: ADR-0052
version: 3
title: Marionette, shipped in the package and synced from its latest GitHub release at every start, is the one plugin every launch of Claude Code, Codex, Pi and OpenCode gets
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - crates/engine/src/marionette.rs
    - crates/engine/src/claude_launch.rs
    - crates/engine/src/claude_mods.rs
    - crates/engine/src/codex_launch.rs
    - crates/engine/src/pi_launch.rs
    - crates/engine/src/opencode_launch.rs
    - crates/adapter/src/pi/sets.rs
    - apps/desktop/src-tauri/src/marionette.rs
    - apps/desktop/src-tauri/src/agents.rs
    - apps/desktop/src-tauri/src/sidecar.rs
    - scripts/package-local.ps1
    - scripts/marionette-pin.json
    - scripts/fetch-release.ps1
    - build-release.bat
  modules:
  domains:
    - bundled-plugins
    - per-launch-injection
    - plugin-sync
    - install-distribution
governs_symbols:
depends_on:
  - ADR-0013
  - ADR-0048
created_from_task: marionette-shipped-2026-11-19
proposed_on: 2026-11-19
proposed_at_sha: 9e44ef2
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
  - ADR-0052 v2
superseded_by: null
changes:
  - "2026-11-12: Ponytail and i-have-adhd are no longer shipped or wired natively; Marionette replaces them and is downloaded, not packaged."
  - "2026-11-12: The release source is Marionette's own repository, acaprino/marionette; Muro's releases are Muro itself (user: \"muro nn deve avere release di marionette\", \"marionette è un esempio di utilizzo di muro\", \"è muro ad avere release al più\")."
  - "2026-11-12: Muro's Release workflow publishes Marionette as `marionette-v<version>` releases of acaprino/muro beside `muro-v<version>`; the sync follows the highest of them (user, linking marionette-v0.2.0: \"deve seguire gli update automatici all'avvio di marionette last release\")."
  - "2026-11-18: The bounds of one release are four times what they were (zip 512 MiB, tree 2 GiB, 65,536 files, lock 16 MiB). Marionette carries its sources whole since Muro's ADR-0026: its zip went from 16 MB to 188 MiB, over the 128 MiB allowed, so the sync refused it and kept the copy before it (user, choosing between this and a smaller release: \"1\"). Measured with that zip: install 191 to 203 s against 47 s, a Claude Code launch 2.9 s against 0.3 s, and 28 s against 6 s for the first launch that reads a copy written by the one before."
  - "2026-11-19: The package ships the pinned Marionette release (scripts/marionette-pin.json) and the app installs it before the sync when the store has no intact copy at least as new (user: \"deve arrivare con marionette già bundled\")."
---

# ADR-0052 v3: Marionette, shipped in the package and synced from its latest GitHub release at every start, is the one plugin every launch gets

<!-- adr-header:start -->
Version 3 · Proposed 2026-11-19 at 9e44ef2 · proposed 2026-11-19 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Sources, kept apart:

- Decided by the user, 2026-11-12: "elimina il support nativo di i-have-adhd e ponytail in
  quanto portiamo dentro un solo plugin: marionette, by muro", then "con sync e update
  upstream direttamente da github a ogni avvio". Asked where the build lives (Muro's
  repository was private and had no published release): "make muro public"; asked which
  version to follow: the latest release.
- Found while building it: Marionette is the package Muro builds
  (`examples/marionette/sources.json` in github.com/acaprino/muro). It already embeds
  Ponytail v5.3.3 and i-have-adhd v0.4.1 as a synthesized session policy, so carrying them
  natively as well would load the same policy twice. A release holds one native adapter per
  host under `marionette/adapters/` (`claude-code`, `codex`, `opencode`, `pi`, `hermes`).
- The repository was renamed from `acaprino/acaprinoprivate` to `acaprino/muro` and made
  public on 2026-11-12; GitHub redirects the old name.
- Decided by the user, 2026-11-12: "marionette è un esempio di utilizzo di muro"; then,
  linking `github.com/acaprino/muro/releases/tag/marionette-v0.2.0`: "deve seguire gli
  update automatici all'avvio di marionette last release".
- Found while building it: Muro's `Release` workflow (`.github/workflows/release.yml`)
  publishes the Marionette package as `marionette-v<version>` releases of `acaprino/muro`,
  with assets `marionette-<version>.zip` and its `.sha256`, beside Muro's own
  `muro-v<version>` releases. GitHub marks whichever was published last as latest, so
  `releases/latest` can name Muro. `acaprino/marionette` holds only `v0.1.0` and is not
  followed.

The question: how does one plugin that changes upstream reach every hosted agent at its
newest release, without a manual step and without editing the user's own agent
configuration (ADR-0013)?

## Decision

1. **Shipped, pinned.** `resources/agent-plugins/` (Ponytail, i-have-adhd, their lock and
   marketplace) stays deleted. `build-release.bat` puts the `marionette*.zip` of the release
   that `scripts/marionette-pin.json` names into `marionette\` beside the executable, checked
   against the pin's SHA-256 and size (`scripts/fetch-release.ps1`), with the pin beside it.
   At start, before the sync and on the same thread, `seed` installs that zip into the store
   when the store has no intact copy at least as new, through the same checks a download
   gets (size, SHA-256, tree, lock, every adapter) before `current.json` switches. A fresh
   install has Marionette from its first start, offline too. The pin moves by hand, like the
   Xiox and Mukade pins. The Claude Code mods in `resources/claude-mods` stay packaged and keep
   their own lock.
2. **Synced at every start.** When the app starts, a background thread lists
   `api.github.com/repos/acaprino/muro/releases?per_page=100` and takes the highest
   `marionette-v<version>` release in numeric version order; drafts, prereleases, Muro's own
   `muro-v*` releases and any other tag are ignored. The release must carry exactly one
   `marionette*.zip` asset under `github.com/acaprino/muro/releases/download/<tag>/` with a
   GitHub `sha256:` digest. When the tag
   differs from the current copy, or the current copy fails its check, the zip is downloaded,
   its size and SHA-256 are checked against the release, it is extracted into a staging
   folder, and the tree is checked: ordinary files only, no links, fixed bounds on count,
   size and depth, all four adapters present. Shinatama then writes `lock.json` (size and
   SHA-256 of every file) and its Codex marketplace, renames the folder to
   `releases/<tag>`, and switches `current.json` in one rename. The new copy and the one
   before it are kept, older ones removed. Downloads use System32 `curl.exe` (HTTPS only,
   proxies from the environment) and extraction System32 `tar.exe`: no network library in
   the app. The window never waits for the sync; a session opened before it finishes uses the
   copy that was current until then.
3. **Where it lives.** `<SHINATAMA_HOME>/marionette/` (`current.json`, `releases/<tag>/`).
   Offline, rate-limited, or with a bad release, the last good copy stays current. With no
   good copy, sessions start without Marionette and are never blocked.
4. **One default set.** Real sessions start with the set `marionette` (the one plugin,
   revision 1). A launcher accepts only the empty set or this exact set.
5. **Verified at every launch, per host.** The launcher checks every file of its own
   adapter against `lock.json` and refuses unlisted files; a damaged adapter drops
   Marionette for that host only, and the next sync repairs it.
6. **Per-launch injection, nothing global touched** (ADR-0013):
   - Claude Code: `--plugin-dir` of a private copy of `adapters/claude-code` (Claude writes
     into loaded plugin folders), tracked sessions only. Its policy is a `prompt.compose`
     mod, which needs Claude Code mods. The display-only mods keep their own root.
   - Codex: `adapters/codex` mirrored into
     `<CODEX_HOME>/plugins/cache/shinatama/marionette/<version>`, then `-c marketplaces...`
     and `-c plugins={"marionette@shinatama"={enabled=true}}`. Tracked sessions only.
   - Pi: `-e adapters/pi/extensions/policy.js --skill adapters/pi/skills`, read in place.
   - OpenCode: one shim `shn-marionette.js` in the private plugin folder that imports
     `adapters/opencode/index.js` where it was verified.
7. **Opt-out.** `SHINATAMA_MARIONETTE=off` skips the sync and gives every session the empty
   set; it also turns off the Claude Code mods, which ride on Marionette.
   `SHINATAMA_BUNDLED_PLUGINS` is retired.
8. **Out of scope.** Hermes and Gemini have no launch adapter. Codex hook trust for the
   adapter's `SessionStart`/`SubagentStart` hooks is Codex's own review. The adapters' helpers
   use the user's `python` and `node` on `PATH`. Building Marionette and publishing it as a
   `marionette-v<version>` release of `acaprino/muro` is Muro's Release workflow, outside
   Shinatama.

## Consequences

- Updates reach every session without a new Shinatama package; a broken Muro release can
  reach every user at their next start. The digest proves the bytes are the release's, not
  that the release is good.
- First start needs no network: the package carries the pinned copy. The package grows by
  that zip (197,024,353 bytes for `marionette-v0.4.0`), and a newer release still arrives
  through the sync.
- FRD-0017 R23 and FRD-0002 R42 describe Marionette; `AVVIO.md` and `README.md` drop the
  Ponytail and i-have-adhd commands.
- Each start makes one unauthenticated GitHub API request (60 per hour per address).
- The lock names its repository: a copy synced from `acaprino/marionette` fails its check
  and is replaced at the next online start.

## Verification

Unit tests in `crates/engine/src/marionette.rs` (seed from the shipped zip, an intact newer
copy kept, a shipped zip that differs from its pin refused, sync, up to date, new release, offline,
digest mismatch, tampered and unlisted files, Claude copy and Codex cache mirroring, asset
rules, highest `marionette-v*` release chosen over Muro's own) and the launchers' argument
tests. Manual, 2026-11-12: `real_github_sync` (ignored by default) followed
`marionette-v0.2.0` from `acaprino/muro` through `SystemFetch`, four adapters verified. Manual: a Muro build zipped with `tar.exe` and
installed through `SystemFetch::extract` (`real_zip_installs_through_system_tar`, ignored by
default). A session per harness from the desktop against a published release is open.
