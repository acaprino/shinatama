---
adr_id: ADR-0059
version: 1
title: Muro, launched by Shinatama on a registry of Shinatama's own, keeps the plugins of Shinatama's agents in sync
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - apps/desktop/src-tauri/src/muro.rs
    - apps/desktop/src-tauri/src/main.rs
    - apps/desktop/src/settings-panel.ts
    - build-release.bat
    - scripts/fetch-release.ps1
    - docs/specs/muro-plugin-sync.md
  modules:
  domains:
    - plugin-sync
    - agent-placement
    - install-distribution
governs_symbols:
depends_on:
  - ADR-0012
  - ADR-0052
created_from_task: muro-plugin-sync-2026-11-19
proposed_on: 2026-11-19
proposed_at_sha: cdd346f
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - "2026-11-19: Owner: \"da shinatama deve essere possibile gestire / avviare muro come plugin sync e gestione\", \"affinché shinatama tenga in sync plugin ecc delle harness lanciate dentro shinatama\"."
  - "2026-11-19: Owner choices: a list just for Shinatama; Muro shipped in the package; sync at start plus Sync now; Muro's background engine, launched by Shinatama, does the sync; Shinatama's Muro quits with Shinatama."
---

# ADR-0059: Muro, launched by Shinatama on a registry of Shinatama's own, keeps the plugins of Shinatama's agents in sync

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-19 at cdd346f · proposed 2026-11-19 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama launches Claude Code, Codex, OpenCode and Pi each in a box, a private home in
Shinatama's data folder (ADR-0012, FRD-0002 R12). Muro (github.com/acaprino/muro) manages the
plugins of the harness homes it is given, from a registry of targets and plugins, and since its
ADR-0028 can run in the background and enforce that registry by itself. Plugins the user manages
with Muro reach the harnesses' usual homes, never Shinatama's boxes.

Sources, kept apart:

- Decided by the owner, 2026-11-19: Shinatama must be able to start and manage Muro for plugin
  sync, so that the harnesses launched in Shinatama keep their plugins in sync.
- Decided by the owner, the same day, choosing among options: a Muro list just for Shinatama
  (not the user's own); Muro shipped in Shinatama's package; a sync at every start plus Sync now
  and Open Muro in Settings; the sync run by Muro's own background engine, launched by Shinatama,
  not by Shinatama; Shinatama's Muro quits with Shinatama.
- Found while designing it: Muro's targets need an existing absolute configuration folder per
  harness; Muro runs Claude Code, Codex and Pi with their home variable set to that folder and
  configures OpenCode through the `opencode.json` of its folder. The latest Muro release, 0.3.3,
  has no background mode; Muro's `develop` has it (ADR-0028 plan 1).

## Decision

1. **A registry of Shinatama's own.** `<data home>/muro` is Shinatama's Muro registry. Every
   Muro call names it. The user's own Muro, its registry and the harnesses' usual homes are
   never touched.
2. **Shinatama's boxes are its targets.** One target per agent box that exists
   (`shinatama-<harness>`): the box home for Claude Code, Codex and Pi, the OpenCode folder of
   the box's XDG configuration for OpenCode; the executable from Muro's own resolver; enabled,
   never exclusive, so Marionette's copies (ADR-0052 v3) and anything added by hand stay.
   Shinatama creates or updates targets, never removes them and never edits the plugin list.
3. **Muro does the sync.** At start, on a thread of its own, Shinatama registers the targets,
   turns automatic sync on in its registry, and starts its Muro in the background on it. Muro
   resolves, plans, applies and records the state. Sync now sends Muro's `sync_now`; Open Muro
   shows the window of Shinatama's Muro. Its Start-with-Windows name is its own and stays off.
4. **Shipped from a pinned release.** The package carries Muro unpacked in `muro\` beside the
   program, from the `muro-v<version>` release `scripts/muro-pin.json` names (`muro-v0.4.0`, the
   first with the background mode), checked against its SHA-256 and size like Mukade and Xiox.
   `SHINATAMA_MURO` names another Muro folder.
5. **Opt-out.** `SHINATAMA_MURO=off` turns the integration off.

## Consequences

- One plugin list for Shinatama's agents, managed with all of Muro's features; a second Muro
  icon in the notification area while Shinatama runs.
- Shinatama's Muro ends with Shinatama. Muro offers no quit request yet, so Shinatama takes the
  registry's operation lock, which waits for an apply that runs and keeps the next one from
  starting, then ends the Muro it started; past a bounded wait it leaves it running.
- Muro's start crashes when another request holds its registry at that moment: Shinatama sends
  nothing while its Muro starts and starts it once more after such a crash.
- The package grows by Muro's portable folder once a release is pinned.

## Verification

Unit tests of the desktop crate through one fake Muro runner (targets, settings, status, busy
retries, Muro missing, the switch); a real run of the debug app with a local build of Muro's
`develop`.
