---
adr_id: ADR-0052
version: 1
title: Ponytail and i-have-adhd ship in the package and activate in every launch of Claude Code, Codex, Pi and OpenCode
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - crates/engine/src/bundled_plugins.rs
    - crates/engine/src/claude_launch.rs
    - crates/engine/src/codex_launch.rs
    - crates/engine/src/pi_launch.rs
    - crates/engine/src/opencode_launch.rs
    - crates/adapter/src/pi/sets.rs
    - apps/desktop/src-tauri/src/agents.rs
    - resources/agent-plugins/
    - scripts/package-local.ps1
  modules:
  domains:
    - bundled-plugins
    - per-launch-injection
    - install-distribution
governs_symbols:
depends_on:
  - ADR-0013
  - ADR-0048
created_from_task: bundled-plugins-2026-11-11
proposed_on: 2026-11-11
proposed_at_sha: 85fda5ada959d5daedf645b0a8252feaf81c902a
accepted_on: 2026-11-11
accepted_at_sha: 4ab547007a78e8f7d10052e240317788441116bb
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0052: Ponytail and i-have-adhd ship in the package and activate in every launch of Claude Code, Codex, Pi and OpenCode

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-11 at 4ab5470 · proposed 2026-11-11 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Sources, kept apart:

- Decided by the user, 2026-11-11: the package "deve arrivare con embedded
  DietrichGebert/ponytail e ayghri/i-have-adhd", and after the first version only copied
  them: "Shinatama deve auto installare, come gli altri plugin preconfigurati".
- Found while building it: no launch carried any set. The desktop passed an empty set and
  every launcher refused a non-empty one (`UnsupportedSet`), so no plugin was preconfigured.
  Daodan's plugins (ADR-0032) are a separate, still deferred mechanism.

The question: how do two plugins that are not Daodan reach every hosted agent without a
manual step and without editing the user's own agent configuration (ADR-0013)?

## Decision

1. **Shipped files.** `resources/agent-plugins/` holds pinned copies of both plugins, their
   MIT licences and `plugins.lock.json` (upstream repository, commit, version, size and
   SHA-256 of every file). The Windows packager stages the directory and ties the lock to
   its receipt (FRD-0017 R23).
2. **One default set.** Real sessions start with the set `bundled-plugins` (exactly the two
   plugins, revision 1). A launcher accepts only the empty set or this exact set; any other
   non-empty set is still refused.
3. **Verified at every launch.** `BundledPlugins::verify` checks each file against the lock,
   refuses links, unlisted files and oversize trees. If the bundle is missing or altered the
   desktop falls back to the empty set: the session starts without plugins and is never
   blocked.
4. **Per-launch injection, nothing global touched** (ADR-0013):
   - Claude Code: `--plugin-dir <plugin>` per plugin, tracked sessions only, because the
     safe-mode profile (`CLAUDE_CODE_SAFE_MODE=1`) loads no plugin (observed 2026-11-11).
   - Codex: the verified files are placed in
     `<CODEX_HOME>/plugins/cache/shinatama-bundled/<plugin>/<version>` inside the private
     home, then `-c marketplaces...` and `-c plugins=...` enable them for that launch;
     `config.toml` is not written. Tracked sessions only. Without the cache Codex lists the
     plugins as "not installed".
   - Pi: `-e` for each extension and `--skill` for each skill directory, read where verified.
   - OpenCode: a one-line shim per plugin in the private `OPENCODE_CONFIG_DIR/plugins`
     imports the plugin where it was verified (each plugin resolves its hooks and skills
     relative to its own tree); the plugins' command files are copied to `command/`.
5. **Opt-out.** `SHINATAMA_BUNDLED_PLUGINS=off` in the environment of the app gives every
   session the empty set. A persisted setting and a per-plugin choice are not decided.
6. **Bundled by the build, adapted one by one.** The set of bundled plugins is fixed at
   build time: the packager stages `resources/agent-plugins/` and ties the lock to its
   receipt, so a package carries exactly the plugins its build declared. There is no
   generic loader that reads the lock and wires whatever it finds. Each plugin has its own
   adaptation for each harness (hooks, skills, commands and the way each agent loads them
   differ), written and tested by hand. Adding a plugin is one deliberate change: its tree
   and lock entry, its entry in `PLUGINS`, its adaptation for every harness that carries it,
   its tests, and a new revision of this record.
7. **Out of scope.** Gemini has no launch adapter. Ponytail's hooks in Codex stay untrusted
   until the user grants trust in `/hooks`; Codex hook trust for plugins is not computed.
   The plugins' `node` hooks use the user's `node` on `PATH`.

## Consequences

- FRD-0017 R23 changes from "activation is optional" to automatic activation with the
  opt-out above; FRD-0002 R42 states the behaviour; `AVVIO.md` keeps manual commands only
  for CLIs started outside Shinatama.
- Relation to ADR-0032: that record decides how Daodan reaches the agents and `daodan.enable`;
  this one governs only the two bundled plugins. A later Daodan set may reuse the same
  launcher paths.
- The qualified trial stays empty, so no recipe evidence or receipt changes.
- Cost, said once: both plugins add skill descriptions and, for Ponytail, hooks to each
  session's context; the opt-out is the only brake for now.

## Verification

`docs/verification/2026-11-11-D21-bundled-plugins.md` (register row D21).
