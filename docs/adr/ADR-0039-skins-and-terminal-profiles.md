---
adr_id: ADR-0039
version: 1
title: Data-driven skins control only the chrome; per-harness terminal profiles; truecolour never rewritten
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - docs/verification-register.md
  modules:
  domains:
    - skins
    - terminal-profiles
    - native-ui
    - accessibility
governs_symbols:
depends_on:
  - ADR-0005
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0039: Data-driven skins control only the chrome; per-harness terminal profiles; truecolour never rewritten

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama is a skinnable native workspace whose panes host real agent TUIs. The question is
what a skin may change, and what stays the harness's own inside its pane. Sources, kept apart:

- Decided by the user: a skinnable UI whose default is the pink Shinatama theme, with other
  skins selectable (functional-design goal 7; 5.7, "decided: default is Shinatama pink; others
  selectable").
- From the native tiling revision (functional-design 5.16, "decided 2026-10-29") and the
  terminal-first scope the user decided on 2026-10-30, which includes per-harness terminal
  profiles: chrome from the skin, agent terminals from separate per-harness profiles. The doc
  does not quote the user on the profile details.
- Author's call, not confirmed: the DeepSeek Harness ACP pane (2026-10-31), drawn with the skin
  and a harness accent colour.
- Left to the builder: final colour tokens and the skin definition format (functional-design 5.7, 8).

The full design is in functional-design 5.7 and 5.16 and technical-design 3.1; this record
holds only the decision.

## Decision Drivers

- Each agent keeps its own visual identity inside its pane.
- The harness owns its TUI; Shinatama must not redraw or reinterpret it.
- Colour choices made by the harness at 24-bit must survive.
- Readable text in every skin.

## Decision

Skins are selectable at runtime and stored in the configuration; UI colours come from a skin
definition, never hard-coded. The default is Shinatama (midnight navy, cobalt and ice lines,
her pink hair as the only warm accent); Section 9, TCTF and Sumi are the others. A skin
controls only the chrome: window, menus, tabs, side panels, UI typography, borders and panel
states. Each harness has a versioned terminal profile: monospace font and size, cell metrics,
cursor style, the 16-colour ANSI palette and default foreground and background. A profile is
presentation data, not layout or session identity; it does not replace or reinterpret the TUI,
and the harness stays the sole owner of what appears inside the PTY. A missing or invalid
profile falls back to a built-in monospace profile without restarting the PTY. "Follow
Shinatama skin" is an explicit opt-in compatibility option, not the default; it remaps only
the default and 16-colour ANSI palette and never rewrites truecolour. Hermes Agent gets a
profile like the other four; DeepSeek Harness, which has no terminal, is drawn with the skin
and a harness accent colour. Secondary text reaches 4.5:1 contrast in every skin; there are no
glowing shadows. See functional-design 5.7 and 5.16; technical-design 3.1.

## Verified Interview Examples

- **EX-001** (nominal case): Input: every text role on every skin, measured by a script on the final tokens -> Expected: all pass WCAG 4.5:1, or the exemptions are written (register U1).
- **EX-002** (boundary case): Input: a terminal profile whose font is unavailable -> Expected: the terminal falls back to the built-in monospace face without changing the PTY session (functional-design 5.16; technical-design 3.1).
- **EX-003** (boundary case): Input: the user turns on follow-skin while a harness emits truecolour -> Expected: only the default and 16-colour ANSI palette are remapped; the truecolour sequences are preserved (functional-design 5.7; technical-design 3.1).
- **EX-004** (boundary case): Input: a DeepSeek Harness tab -> Expected: its ACP pane is drawn with the active skin and a harness accent colour, not a terminal profile (functional-design 5.7).

## Rejected Alternatives

- **RA-001**: following the skin by default inside agent terminals; the profile gives the harness a stable identity in its pane, so following the skin is an explicit compatibility option (functional-design 5.1, 5.7, 5.16).
- **RA-002**: redrawing the harness's widgets as native controls; the harness owns layout, glyphs, spacing, prompts, boxes and colours inside the PTY (functional-design 5.16).
- **RA-003**: the HTML mockups' effects (scan lines, clipped corners, glow) as requirements; they illustrate the mood only, and where a skin lists scan lines or cut corners they are optional decoration (functional-design 5.7).

## Enforceable Invariants

- **INV-001**: no UI colour is hard-coded; every chrome colour comes from the active skin definition.
- **INV-002**: changing the skin never changes what the harness draws inside the PTY; only follow-skin may remap the default and 16-colour palette.
- **INV-003**: truecolour output chosen by a harness is never rewritten, with or without follow-skin.
- **INV-004**: follow-skin is off by default.
- **INV-005**: a profile change or fallback never restarts the PTY; the terminal is initialised with its profile before rows and columns are derived, and a font or metric change delivers one final resize (technical-design 3.1).
- **INV-006**: secondary text reaches 4.5:1 contrast in every skin.

## Consequences

- Bundled fonts and their licences join the dependency notice (technical-design 3.1).
- The terminal-first milestone ships per-harness profiles (functional-design 5.12, 5.16; ADR-0040).
- Register U2 tests a live skin change on the chrome and the DeepSeek Harness ACP pane, without restart or flicker, with terminal panes keeping their harness profiles unless follow-skin is on.
- The Oni-derived art of the default skin is a distribution decision separate from the code (functional-design 5.12; register O4).

## Confirmation

- Register rows U1, U2 and U8 (G1, G2) must pass; U8 is the design review of the real UI, not the mockups.
- The prototype gate of technical-design 3.1 covers per-profile font fallback and ANSI/truecolour output; TL1 and TL2 cover the toolkit and live panes. None has been run.
