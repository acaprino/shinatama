---
adr_id: ADR-0039
version: 3
title: "Two looks colour a glass chrome whose window theme follows the look; panels open in place; per-harness terminal profiles; truecolour never rewritten"
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - docs/verification-register.md
    - apps/desktop/src/styles.css
    - apps/desktop/src/smooth.css
    - apps/desktop/src/modal.ts
    - apps/desktop/src/presentation.ts
    - apps/desktop/src-tauri/tauri.conf.json
    - apps/desktop/src-tauri/src/presentation.rs
    - apps/desktop/src-tauri/src/webview_recovery.rs
    - docs/design-system/DESIGN.md
    - docs/design-system/desktop-shell.md
  modules:
  domains:
    - skins
    - terminal-profiles
    - native-ui
    - accessibility
    - glass
    - in-place-surfaces
governs_symbols:
depends_on:
  - ADR-0005
created_from_task: smooth-glass-2026-11-19
proposed_on: 2026-11-19
proposed_at_sha: 9e44ef2
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
  - ADR-0039 v2
superseded_by: null
changes:
  - "2026-11-19: Smooth (e2f0bbe) is the default look beside ONI MODE, with rounded corners and a light theme, scoped to data-look=smooth; this version records it."
  - "2026-11-19: The window theme follows the look instead of being always dark (user: \"il light non deve sembrare grigio con il glass\")."
  - "2026-11-19: Smooth's frame is one sheet of glass in its own tint (user: \"la nuova ui di shinatama non applica l'effetto glass della vecchia\", \"la toolbar è ancora quasi come un frame window nativo\")."
  - "2026-11-22: Imago v0.2.0's canon: ONI has a light theme too, and the look and the theme are the window's five Appearance commands (Ctrl+K). A skin paints ONI dark only; under ONI light the skin control is disabled and says why. An ONI saved before (no shinatama.look-version 2) stays dark. INV-012 reads accordingly."
---

# ADR-0039: Two looks colour a glass chrome whose window theme follows the look; panels open in place; per-harness terminal profiles; truecolour never rewritten

<!-- adr-header:start -->
Version 3 · Proposed 2026-11-19 at 9e44ef2 · proposed 2026-11-19 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama is a skinnable native workspace whose panes host real agent TUIs. Version 1 decided
what a skin may change and what stays the harness's own inside its pane. It said nothing about
the material the chrome is drawn in, nor about how panels and choices are presented; the first
desktop build drew opaque chrome and opened every panel as a centred dialog over the terminals.
Sources, kept apart:

- Decided by the user, unchanged from version 1: a skinnable UI whose default is the pink
  Shinatama theme, other skins selectable; chrome from the skin, agent terminals from separate
  per-harness profiles.
- Decided by the user on 2026-11-11, on the running desktop app: the interface is frosted glass
  over the desktop, on the whole window, menus and dialogs included; the glass keeps the
  distinctive colours of the active skin; the look is square, clean and fluid (the references
  remain *Oni* and *Ghost in the Shell*); there are as few popups as possible; New session
  chooses the harness from a dropdown and starts in the selected project, without a dialog.
- Rejected by the user on the same day, each after seeing it: rounded corners; a grid drawn
  behind an empty pane; the Mica backdrop (too opaque); opaque bars around a transparent
  terminal (out of tune with the rest); glass in a neutral dark with the skin's lines removed;
  glass over the light system backdrop ("the console is unreadable", "the colours of the UI
  are too washed out"). The same glass over the dark backdrop: "now it is perfect".
- Decided by the user on 2026-11-19, on the running app: Smooth, the new default look, must show
  the glass of the classic look (ONI MODE) ("la nuova ui di shinatama non applica l'effetto glass
  della vecchia"); its toolbar must not read as a native window frame ("la toolbar è ancora quasi
  come un frame window nativo"); the light theme must not look grey with the glass ("il light non
  deve sembrare grigio con il glass"). Measured that day: Smooth dark's title bar and toolbar were
  32,35,41, Windows 11's own dark caption; Smooth light sat on the dark acrylic at 228-230.
- Author's call, not confirmed: the densities of the glass, Smooth's glass tint; which two decisions still take the
  whole page.
- Left to the builder: final colour tokens and the skin definition format (functional-design 5.7, 8).

The full design is in functional-design 5.7 and 5.16 and technical-design 3.1; this record
holds only the decision.

## Decision Drivers

- Each agent keeps its own visual identity inside its pane.
- The harness owns its TUI; Shinatama must not redraw or reinterpret it.
- Colour choices made by the harness at 24-bit must survive.
- Readable text in every skin.
- A skin is recognisable through the glass, not only in its accents.
- Working in a terminal is not interrupted by a panel opened beside it.

## Decision

Skins are selectable at runtime and stored in the configuration; UI colours come from a skin
definition, never hard-coded. The default is Shinatama (midnight navy, cobalt and ice lines,
her pink hair as the only warm accent); Section 9, TCTF and Sumi are the others. A skin
controls only the chrome: window, menus, tabs, side panels, UI typography, borders and panel
states.

**Looks.** Smooth is the default look, in dark and light (or the Windows theme); ONI MODE is the
look this record first described, always dark, with the skins below. Smooth is a layer scoped to
the root's `data-look`, so ONI MODE is unchanged by it.

**Glass.** In the desktop app the window is transparent over the Windows acrylic backdrop, and the
window takes the theme of the look: dark for ONI MODE and Smooth dark, light for Smooth light,
the system's for Smooth's Windows theme. The window opens with the theme its page chose last, so
the backdrop is the look's own from the first frame. Every
chrome surface is one translucent material: bars, side panels, pane headers, menus,
sheets, drawers and notices. Its colour is derived from the active skin, the skin's chrome
colour mixed with a measure of its strong line colour, so the glass is cobalt in Shinatama,
teal in Section 9 and brass in TCTF, never a neutral grey. Terminal panes are the lightest
surface; bars, side panels and floating surfaces are denser, and floating surfaces also blur
what is under them. Seams between surfaces are the skin's line colour, translucent. In ONI MODE corners
are square; nothing glows, and an empty pane has no drawn background. In Smooth the frame (title
bar, toolbar, sidebar and status bar) is one sheet of glass that paints nothing over a ground in
Smooth's glass tint, a deep cool slate in the dark and white in the light; its hairlines are the
text colour, faint; panes, menus and dialogs have rounded corners. The values are in
`docs/design-system/DESIGN.md`, the technique in `docs/design-system/desktop-shell.md`.

**Terminals on glass.** Each harness has a versioned terminal profile: monospace font and size,
cell metrics, cursor style, the 16-colour ANSI palette and default foreground and background.
On glass the pane's default background keeps the profile's colour and is drawn partly
transparent, from the moment the terminal is created; nothing else of the profile changes, and a background the harness paints itself
is drawn as emitted. A profile is presentation data, not layout or session identity; it does
not replace or reinterpret the TUI, and the harness stays the sole owner of what appears
inside the PTY. A missing or invalid profile falls back to a built-in monospace profile
without restarting the PTY. "Follow Shinatama skin" is an explicit opt-in compatibility
option, not the default; it remaps only the default and 16-colour ANSI palette and never
rewrites truecolour. Hermes Agent gets a profile like the other four; DeepSeek Harness, which
has no terminal, is drawn with the skin and a harness accent colour.

**Surfaces open in place.** A panel or a choice is shown in one of three ways, and the first
two never block a terminal they do not cover:

- a *drawer*, docked beside the panes, which give up its width: the toolbar panels (instances,
  projects, files, sessions, search, settings) and their detail views;
- a *sheet*, docked to the pane or control it belongs to and dismissed by a press outside it:
  finding in a terminal, confirming the stop of one terminal, a session in another folder,
  the project roots;
- a *modal*, which takes everything below the window frame: only a decision about the whole
  workspace, namely closing the workspace with live terminals and the hop preview.

New session is a menu of harnesses under the control that was pressed; one choice starts the
session in the selected project. The project is changed from a finder inside the project
button. The window frame is never covered or made inert by any of these.

Secondary text reaches 4.5:1 contrast in every skin, measured on the skin's opaque tokens.
See functional-design 5.7 and 5.16; technical-design 3.1.

## Verified Interview Examples

- **EX-001** (nominal case): Input: every text role on every skin, measured by a script on the final tokens -> Expected: all pass WCAG 4.5:1, or the exemptions are written (register U1).
- **EX-002** (boundary case): Input: a terminal profile whose font is unavailable -> Expected: the terminal falls back to the built-in monospace face without changing the PTY session (functional-design 5.16; technical-design 3.1).
- **EX-003** (boundary case): Input: the user turns on follow-skin while a harness emits truecolour -> Expected: only the default and 16-colour ANSI palette are remapped; the truecolour sequences are preserved (functional-design 5.7; technical-design 3.1).
- **EX-004** (boundary case): Input: a DeepSeek Harness tab -> Expected: its ACP pane is drawn with the active skin and a harness accent colour, not a terminal profile (functional-design 5.7).
- **EX-005** (nominal case): Input: New session is pressed with a project selected -> Expected: a menu of harnesses opens under the control; choosing one starts the session in that project; no dialog opens.
- **EX-006** (nominal case): Input: Files is opened while a terminal is running -> Expected: the panel docks beside the pane, which narrows; typing into the terminal still works.
- **EX-007** (boundary case): Input: Stop is pressed on one terminal while two panes are open -> Expected: the confirmation is a bar along the bottom of that pane; the other pane stays usable.
- **EX-008** (boundary case): Input: the window is closed with live terminals -> Expected: the confirmation takes the page; no keystroke reaches a terminal until it is answered.
- **EX-009** (boundary case): Input: New session is pressed with no project selected -> Expected: the project finder opens and a notice says that sessions start in the selected project; no session starts.

## Rejected Alternatives

- **RA-001**: following the skin by default inside agent terminals; the profile gives the harness a stable identity in its pane, so following the skin is an explicit compatibility option (functional-design 5.1, 5.7, 5.16).
- **RA-002**: redrawing the harness's widgets as native controls; the harness owns layout, glyphs, spacing, prompts, boxes and colours inside the PTY (functional-design 5.16).
- **RA-003**: the HTML mockups' effects (scan lines, clipped corners, glow) as requirements; they illustrate the mood only, and where a skin lists scan lines or cut corners they are optional decoration (functional-design 5.7).
- **RA-004**: the Mica backdrop; over a dark desktop it is nearly opaque, so nothing reads as glass.
- **RA-005**: opaque chrome around transparent terminal panes; the panes read as a hole in the window instead of as part of it.
- **RA-006**: glass in a neutral dark with the seams removed; every skin then looks the same through the backdrop.
- **RA-007**: rounded corners and a grid drawn behind an empty pane; neither belongs to the square, clean look the user asked for.
- **RA-008**: centred dialogs for New session, project selection and the toolbar panels; each one covers the terminals and stops the work under it for a choice that needs one click.
- **RA-009**: letting the backdrop follow the system theme under surfaces of the other theme; with the light theme the acrylic is a light grey under dark surfaces, every skin washes out and terminal text loses its contrast (measured, register U9).
- **RA-010**: a dark window under every look (version 2's INV-012); Smooth light then sits on the dark acrylic and reads grey (its frame measured 228-230 on 2026-11-19, 241 on the light acrylic).
- **RA-011**: opaque frame bars in Smooth over a dense ground; the title bar and toolbar become one band in Windows' own caption colour and read as a native frame.

## Enforceable Invariants

- **INV-001**: no UI colour is hard-coded; every chrome colour, the glass included, comes from the active skin definition. The one written exception is the operating-system backdrop, which is the system's acrylic in the window's theme (INV-012) and carries no skin colour.
- **INV-002**: changing the skin never changes what the harness draws inside the PTY; only follow-skin may remap the default and 16-colour palette.
- **INV-003**: truecolour output chosen by a harness is never rewritten, with or without follow-skin.
- **INV-004**: follow-skin is off by default.
- **INV-005**: a profile change or fallback never restarts the PTY; the terminal is initialised with its profile before rows and columns are derived, and a font or metric change delivers one final resize (technical-design 3.1).
- **INV-006**: secondary text reaches 4.5:1 contrast in every skin, on the skin's opaque tokens.
- **INV-007**: glass changes only the opacity of a terminal's default background; its colour stays the profile's.
- **INV-008**: no glowing shadow anywhere in the chrome; in ONI MODE no rounded corner, on glass as on opaque surfaces.
- **INV-009**: a drawer or a sheet never makes a terminal inert; only a modal does, and only the workspace close confirmation and the hop preview are modals.
- **INV-010**: starting a session in the selected project takes one choice, the harness, and opens no dialog.
- **INV-011**: no surface covers the window frame or makes it inert.
- **INV-012**: the desktop window's theme is the one the page draws: light for a light theme, dark for a dark one, the system's for Smooth's Windows theme and dark for ONI's (Imago's `windowThemeFor`); the window opens with the theme its page chose last. A skin is ONI dark's only.
- **INV-013**: a terminal paints its default background when it is created, not only when it is restored or its profile changes.
- **INV-014**: in Smooth the frame bars paint no fill over the glass ground, and the dark glass tint stays darker and bluer than the Windows caption grey.

## Consequences

- Bundled fonts and their licences join the dependency notice (technical-design 3.1).
- The terminal-first milestone ships per-harness profiles (functional-design 5.12, 5.16; ADR-0040).
- Register U2 tests a live skin change on the chrome and the DeepSeek Harness ACP pane, without restart or flicker, with terminal panes keeping their harness profiles unless follow-skin is on.
- The Oni-derived art of the default skin is a distribution decision separate from the code (functional-design 5.12; register O4).
- On Windows 11 the acrylic is the system backdrop: it follows the window theme and ignores the tint colour in the window configuration, which applies only to Windows 10. Shinatama therefore cannot colour the backdrop; the skin's colour comes from the glass drawn over it.
- A skin with light chrome would need the light backdrop; none exists, and the dark theme is fixed until one does.
- Text sits on glass over whatever is behind the window. Measured over a dark window it passes; over a bright window it has not been measured (register U9).
- Glass is always on in the desktop app; there is no setting to turn it off. The browser preview has no backdrop and draws the same chrome over an opaque stage.
- The desktop smoke tests start sessions through one shared helper, `apps/desktop/tests/session-flow.mjs`, which follows the harness menu, the project finder and Other folder… as a user does. `ui-flow-smoke`, which expects every panel to block the page, no longer describes the product (register U10).

## Confirmation

- Register rows U1, U2 and U8 (G1, G2) must pass; U8 is the design review of the real UI, not the mockups. U9 (text on glass, each skin's hue) and U10 (in-place surfaces) are added by this version.
- Run on 2026-11-11: the type check and the fifteen unit tests, among them the skin test (text roles at 4.5:1 on the tokens, no colour outside the token block, no rounded corner, no glowing shadow); the real desktop app driven with an isolated data folder through the harness menu, the drawers and the in-pane stop confirmation, with no page error.
- Measured on screen on 2026-11-11, Shinatama skin, window over a dark window ([evidence](../verification/2026-11-11-U9-glass-on-screen.md)): in the dark theme the terminal background is 38,40,45 and the skin's text and muted colours reach 12.8:1 and 5.9:1 on it; in the light theme they fell to 8.9:1 and 4.1:1, and to 5.2:1 and 2.4:1 with a white window behind.
- The user confirmed the dark-theme glass by eye on the running app ("now it is perfect"), after rejecting the light-backdrop build and a build whose new terminals painted no background.
- Run on 2026-11-11 for U10 ([evidence](../verification/2026-11-11-U10-new-session-menu.md)): `new-session-smoke` (PowerShell), `desktop-smoke`, `attention-smoke`, `thread-rename-smoke`, `claude-resume-smoke` and `claude-resume-race-smoke` pass on the real app.
- Not passing or not run: the three Codex-backed smoke tests (they fail outside the interface), `ui-flow-smoke` (not updated), the other skins on glass, and U9 over a bright window.
- The prototype gate of technical-design 3.1 covers per-profile font fallback and ANSI/truecolour output; TL1 and TL2 cover the toolkit and live panes. None has been run.
