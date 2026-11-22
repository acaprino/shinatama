---
version: alpha
name: Shinatama Desktop
description: Glass desktop workspace for agents that write code. Smooth look in dark and light, ONI MODE as the classic look in dark (with its skins) and light, one pink accent.
x-zsd:
  profile: "0.1"
  kind: project
  source:
    method: hybrid
    captured: 2026-11-19
    paths:
      - apps/desktop/src/presentation.ts
      - apps/desktop/src/styles.css
      - apps/desktop/src/smooth.css
      - apps/desktop/src/workspace-renderer.css
      - apps/desktop/src/toolbar-simple.css
      - apps/desktop/src/thread-sidebar.css
      - apps/desktop/index.html
      - apps/desktop/src-tauri/tauri.conf.json
      - apps/desktop/src-tauri/src/presentation.rs
      - apps/desktop/src-tauri/src/native_frame.rs
      - docs/verification/2026-11-11-U9-glass-on-screen.md
  review:
    status: draft
  intent:
    audience: Developers who run several coding agents at once in one Windows desktop window.
    primary-task: Watch and steer agent terminals, threads and panels without a second window.
    character: Glass over the Windows acrylic material, quiet chrome, hairlines, one pink accent.
    density: compact
    theme: either look in dark, light or the Windows theme (ONI with the Windows theme is dark); a skin paints ONI dark only.
    references:
      - SmoothUI, the model for the Smooth look (outlined buttons, underlined tabs, sentence case).
      - Windows 11 acrylic material (DWM system backdrop type 3, transient window).
      - Shinatama brand, the pink accent and the Bahnschrift display face.
  preferences:
    must-use:
      - A transparent window over the Windows acrylic material, with the window theme set from the look.
      - One pink accent per look. Only an ONI skin changes it.
      - Hairline boundaries. Depth comes from glass, blur and shadows.
      - Sentence case in Smooth. Capitals with letter spacing only in ONI MODE.
      - Local Windows fonts only.
    must-avoid:
      - A second accent color.
      - An opaque fill on the frame bars over the glass.
      - Light glass over a dark acrylic material, or dark glass over a light one.
      - Browser behavior in the shell, such as page reload keys, the page context menu and selectable chrome.
      - Status shown by color only.
    preserve:
      - Pane geometry is the same in both looks, so a terminal keeps its rows when the look changes.
      - ONI MODE stays as it is. Smooth is a layer scoped to the root attribute data-look="smooth".
  implementation:
    token-source: apps/desktop/src/presentation.ts
    component-sources:
      - apps/desktop/index.html
      - apps/desktop/src/window-controls.ts
      - apps/desktop/src/modal.ts
      - apps/desktop/src/toolbar-simple.ts
      - apps/desktop/src/workspace-renderer.ts
      - apps/desktop/src-tauri/src/native_frame.rs
    style-sources:
      - apps/desktop/src/styles.css
      - apps/desktop/src/smooth.css
      - apps/desktop/src/workspace-renderer.css
      - apps/desktop/src/toolbar-simple.css
    font-assets: []
    image-assets: []
    icon-assets:
      - apps/desktop/index.html
  unresolved:
    - The source sets the line height of headline-md and title-sm to normal. The value 1.2 is an estimate.
    - Smooth light is measured on screen for the frame only, not with terminal text on it.
    - The four ONI skins are not measured on screen over the acrylic material.
    - Hover, pressed and disabled states of menu items have no tokens of their own.
    - High contrast mode, battery saver and Windows 10 are not checked with the glass.
    - The bars are not checked at text zoom.
    - The team map grid color stays in presentation.ts only; it is not part of this shared system.
    - In Smooth light, button-danger-hover puts on-primary on danger-line at about 2.2 to 1 (styles.css, button.danger:hover).
  layout:
    viewport-min: 500px
    drawer-width-min: 400px
    drawer-width-max: 620px
    menu-width-max: 420px
  breakpoints:
    narrow: 620px
    compact: 850px
    drawer: 1000px
  border:
    width-default: 1px
    color-default: "{colors.outline-variant}"
  elevation:
    glass-blur-radius: 18px
    glass-saturate: 1.5
    shadow-menu-y: 18px
    shadow-menu-blur: 50px
    shadow-dialog-y: 24px
    shadow-dialog-blur: 80px
    shadow-color-smooth-dark: "#00000066"
    shadow-color-smooth-light: "#15274A26"
    shadow-color-oni: "#000000AA"
  motion:
    duration-fast: 120ms
    duration-base: 180ms
    duration-slow: 320ms
    easing-out: cubic-bezier(0.16, 1, 0.3, 1)
    easing-out-oni: cubic-bezier(0.2, 0.8, 0.2, 1)
    easing-in: cubic-bezier(0.4, 0, 1, 1)
  accessibility:
    target-min: "{spacing.control-height}"
    focus-width: 2px
    focus-offset: 2px
    focus-color: "{colors.primary-text}"
  modes:
    smooth-light:
      colors:
        primary-text: "#B0286A"
        background: "#F6F7F9"
        surface-container-low: "#F6F7F9"
        surface-container: "#F0F2F5"
        surface: "#FFFFFF"
        surface-container-high: "#ECEEF2"
        terminal: "#FFFFFF"
        on-surface: "#111827"
        on-surface-variant: "#3D4656"
        muted: "#566070"
        outline-variant: "#E3E6EA"
        outline: "#7B8494"
        tertiary: "#2B4A80"
        live: "#0F7566"
        warning: "#8A5A00"
        error: "#B3261E"
        danger: "#B3261E"
        danger-line: "#8F1D17"
        primary-container: "#F9D7E6"
        wash: "#1118270A"
        wash-hover: "#11182717"
        glass-ground: "#FFFFFFE0"
        glass-edge: "#1118271C"
        glass-sheet: "#DDE0E6A8"
        terminal-glass: "#FFFFFF94"
        scrim: "#0F183055"
    oni:
      colors:
        background: "#04070F"
        surface-container-low: "#070C18"
        surface-container: "#0A1120"
        surface: "#0E1729"
        surface-container-high: "#13203A"
        terminal: "#050912"
        on-surface: "#E6F0FF"
        on-surface-variant: "#B4C8E8"
        muted: "#8FA6C9"
        outline-variant: "#1C3563"
        outline: "#4A78E6"
        tertiary: "#A9C8FF"
        primary-container: "#2A4A8F"
        wash: "#00000000"
        wash-hover: "#13203A"
        glass-ground: "#04070FDB"
        glass-edge: "#4A78E666"
        glass-sheet: "#142240A8"
        terminal-glass: "#05091294"
        scrim: "#02050BCC"
    oni-neutral:
      colors:
        primary: "#B8C9E3"
        primary-soft: "#DBE7FA"
        on-primary: "#0A1220"
        primary-text: "#B8C9E3"
        background: "#080B10"
        surface-container-low: "#0D1219"
        surface-container: "#121A24"
        surface: "#17212E"
        surface-container-high: "#1D2A3A"
        terminal: "#090E15"
        on-surface: "#E6F0FF"
        on-surface-variant: "#B4C8E8"
        muted: "#8FA6C9"
        outline-variant: "#334256"
        outline: "#899BB6"
        tertiary: "#D5E1F4"
        primary-container: "#2A4A8F"
        wash: "#00000000"
        wash-hover: "#1D2A3A"
        glass-ground: "#080B10DB"
        glass-edge: "#899BB666"
        terminal-glass: "#090E1594"
        scrim: "#02050BCC"
    oni-section9:
      colors:
        primary: "#78E2DC"
        primary-soft: "#B4FFF8"
        on-primary: "#032021"
        primary-text: "#78E2DC"
        background: "#030B0C"
        surface-container-low: "#061214"
        surface-container: "#091B1D"
        surface: "#0C2225"
        surface-container-high: "#103035"
        terminal: "#041012"
        on-surface: "#E4F8F6"
        on-surface-variant: "#BEDEDB"
        muted: "#A4C9C6"
        outline-variant: "#24545A"
        outline: "#62AEB6"
        tertiary: "#A8EDF2"
        primary-container: "#235E62"
        wash: "#00000000"
        wash-hover: "#103035"
        glass-ground: "#030B0CDB"
        glass-edge: "#62AEB666"
        terminal-glass: "#04101294"
        scrim: "#02050BCC"
    oni-tctf:
      colors:
        primary: "#FFC072"
        primary-soft: "#FFDFAD"
        on-primary: "#241405"
        primary-text: "#FFC072"
        background: "#090B0E"
        surface-container-low: "#10151B"
        surface-container: "#171E26"
        surface: "#1B2631"
        surface-container-high: "#24313E"
        terminal: "#0A1016"
        on-surface: "#F5ECE0"
        on-surface-variant: "#D8CAB8"
        muted: "#C7BAA7"
        outline-variant: "#455567"
        outline: "#B59B78"
        tertiary: "#F6D9B3"
        primary-container: "#635035"
        wash: "#00000000"
        wash-hover: "#24313E"
        glass-ground: "#090B0EDB"
        glass-edge: "#B59B7866"
        terminal-glass: "#0A101694"
        scrim: "#02050BCC"
    oni-sumi:
      colors:
        primary: "#F49C8B"
        primary-soft: "#FFD2C5"
        on-primary: "#2A0C08"
        primary-text: "#F49C8B"
        background: "#080707"
        surface-container-low: "#100C0C"
        surface-container: "#171111"
        surface: "#211919"
        surface-container-high: "#2B2020"
        terminal: "#0A0808"
        on-surface: "#F7E9E3"
        on-surface-variant: "#DCC7C0"
        muted: "#CBB5AE"
        outline-variant: "#573A37"
        outline: "#C88B81"
        tertiary: "#F3D5CD"
        primary-container: "#653C35"
        wash: "#00000000"
        wash-hover: "#2B2020"
        glass-ground: "#080707DB"
        glass-edge: "#C88B8166"
        terminal-glass: "#0A080894"
        scrim: "#02050BCC"
colors:
  primary: "#F48FB8"
  primary-soft: "#FFB3D0"
  on-primary: "#1C0811"
  primary-text: "#F48FB8"
  background: "#12161E"
  surface-container-low: "#12161E"
  surface-container: "#161B25"
  surface: "#1A202C"
  surface-container-high: "#222A38"
  terminal: "#0E1118"
  on-surface: "#E8ECF3"
  on-surface-variant: "#C0C8D6"
  muted: "#9AA5B8"
  outline-variant: "#2A3240"
  outline: "#6B7488"
  tertiary: "#C9D4E6"
  live: "#6FE3D2"
  warning: "#E8BB72"
  error: "#FF8495"
  danger: "#A0294E"
  danger-line: "#E35F87"
  on-danger: "#FFFFFF"
  primary-container: "#4A2C40"
  clear: "#00000000"
  wash: "#E8ECF30A"
  wash-hover: "#E8ECF317"
  glass-ground: "#0C111DCC"
  glass-edge: "#E8ECF329"
  glass-sheet: "#242935A8"
  terminal-glass: "#0E111894"
  scrim: "#0A0D12CC"
typography:
  headline-md:
    fontFamily: "Bahnschrift, 'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: 18px
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: 0px
  title-sm:
    fontFamily: "Bahnschrift, 'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: 13px
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: 0px
  body-md:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif"
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: 0px
  label-md:
    fontFamily: "Bahnschrift, 'Segoe UI Variable Display', 'Segoe UI', sans-serif"
    fontSize: 12px
    fontWeight: 600
    lineHeight: 1
    letterSpacing: 0px
  code-md:
    fontFamily: "'Cascadia Mono', 'Cascadia Code', Consolas, monospace"
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.15
    letterSpacing: 0px
spacing:
  xs: 4px
  sm: 8px
  md: 10px
  lg: 14px
  xl: 24px
  control-height: 28px
  field-height: 36px
  titlebar-height: 34px
  toolbar-height: 44px
  statusbar-height: 28px
  pane-header-height: 30px
rounded:
  none: 0px
  sm: 6px
  md: 8px
  lg: 12px
  full: 999px
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.label-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.md}"
    height: "{spacing.control-height}"
  button-primary-hover:
    backgroundColor: "{colors.primary-soft}"
    textColor: "{colors.on-primary}"
    typography: "{typography.label-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.md}"
    height: "{spacing.control-height}"
  button-secondary:
    backgroundColor: "{colors.wash}"
    typography: "{typography.label-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.md}"
    height: "{spacing.control-height}"
  button-secondary-hover:
    backgroundColor: "{colors.wash-hover}"
    typography: "{typography.label-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.md}"
    height: "{spacing.control-height}"
  button-danger:
    backgroundColor: "{colors.danger}"
    textColor: "{colors.on-danger}"
    typography: "{typography.label-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.md}"
    height: "{spacing.control-height}"
  button-danger-hover:
    backgroundColor: "{colors.danger-line}"
    textColor: "{colors.on-primary}"
    typography: "{typography.label-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.md}"
    height: "{spacing.control-height}"
  input:
    backgroundColor: "{colors.terminal}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body-md}"
    rounded: "{rounded.sm}"
    padding: "{spacing.md}"
    height: "{spacing.field-height}"
  caption:
    backgroundColor: "{colors.clear}"
    textColor: "{colors.muted}"
    typography: "{typography.label-md}"
    rounded: "{rounded.none}"
    height: "{spacing.titlebar-height}"
  caption-hover:
    backgroundColor: "{colors.surface-container-high}"
    textColor: "{colors.on-surface}"
    typography: "{typography.label-md}"
    rounded: "{rounded.none}"
    height: "{spacing.titlebar-height}"
  caption-close-hover:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.label-md}"
    rounded: "{rounded.none}"
    height: "{spacing.titlebar-height}"
  frame:
    backgroundColor: "{colors.glass-ground}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body-md}"
    rounded: "{rounded.none}"
  frame-edge:
    backgroundColor: "{colors.glass-edge}"
  brand-mark:
    textColor: "{colors.primary-text}"
  menu:
    backgroundColor: "{colors.glass-sheet}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body-md}"
    rounded: "{rounded.md}"
    padding: "{spacing.sm}"
  dialog:
    backgroundColor: "{colors.glass-sheet}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body-md}"
    rounded: "{rounded.lg}"
    padding: "{spacing.xl}"
  dialog-scrim:
    backgroundColor: "{colors.scrim}"
  pane:
    backgroundColor: "{colors.terminal-glass}"
    textColor: "{colors.on-surface}"
    typography: "{typography.code-md}"
    rounded: "{rounded.md}"
  pane-tab-active:
    backgroundColor: "{colors.clear}"
    textColor: "{colors.on-surface}"
    typography: "{typography.label-md}"
    height: "{spacing.pane-header-height}"
  notice:
    backgroundColor: "{colors.glass-sheet}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body-md}"
    rounded: "{rounded.md}"
    padding: "{spacing.md}"
  status-live:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.live}"
    typography: "{typography.label-md}"
  status-warning:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.warning}"
    typography: "{typography.label-md}"
---

# Shinatama Desktop

## Overview

Shinatama is a Windows workspace for agents that write code. Smooth is the default look, in dark and light. ONI MODE is the classic look, in dark (with Shinatama's skins) and light. Both looks paint translucent layers on the Windows acrylic material. Muro, Mukade, Xiox and Ethel use this file and `tokens/`.

### Implementation

Read `x-zsd.implementation` before a UI change. The code tokens are in `presentation.ts`. The glass procedure is in [desktop-shell.md](desktop-shell.md).

## Colors

The top-level values are Smooth dark. `x-zsd.modes` holds Smooth light, ONI MODE and the ONI skins. Each bullet gives the code variable.

- `primary`: Use for the one primary action and the active tab mark. Code: `--accent`.
- `primary-soft`: Use for the hover state of the primary action. Code: `--accent-soft`.
- `on-primary`: Use for text on `primary` and `primary-soft`. Code: `--accent-ink`.
- `primary-text`: Use for accent text and the focus ring. Code: `--accent-text`.
- `primary-container`: Use for selected text. Code: `--selection`.
- `background`, `surface-container-low`, `surface-container`: Use for the ground, the bars and the toolbar without glass. Code: `--stage`, `--chrome`, `--chrome-2`.
- `surface`, `surface-container-high`: Use for sheets, the selected item and quiet hover states. Code: `--surface`, `--surface-2`.
- `terminal`: Use for terminal panes and fields.
- `on-surface`, `on-surface-variant`, `muted`: Use for main text, secondary text and hints. Code: `--text`, `--text-2`, `--muted`.
- `outline-variant`, `outline`: Use for hairlines and for boundaries that must stay visible. Code: `--line`, `--line-strong`.
- `tertiary`: Use for the wordmark and informational marks. Code: `--ice`.
- `live`, `warning`, `error`: Use for status marks with a text label.
- `danger`, `danger-line`, `on-danger`: Use for destructive actions only.
- `clear`, `wash`, `wash-hover`: Use for controls with no fill, a secondary button and its hover state.
- `glass-ground`, `glass-edge`: Use for the window ground and its hairlines on glass.
- `glass-sheet`, `terminal-glass`: Use for menus, dialogs, notices and terminal panes on glass.
- `scrim`: Use for the modal scrim behind a dialog.

## Typography

- `headline-md`: Use for dialog and drawer titles.
- `title-sm`: Use for sidebar and panel headings.
- `body-md`: Use for text and field values.
- `label-md`: Use for button labels, in sentence case. ONI MODE writes labels in capitals with letter spacing.
- `code-md`: Use for terminals, code and paths. ONI MODE also writes its labels in it.

All fonts are local Windows fonts. The app downloads no font.

## Layout

The window is one grid: title bar, toolbar, work area and status bar. Their heights are `titlebar-height`, `toolbar-height` and `statusbar-height`. The thread sidebar is at the left. Drawers open at the right, at a width between `x-zsd.layout.drawer-width-min` and `drawer-width-max`, and push the work area. Panes have the same size in both looks.

- Use `xs` between Smooth toolbar items. Use `sm` between ONI MODE toolbar items and around the work area.
- Use `md` inside controls, `lg` inside the title bar and `xl` inside dialogs.
- Use `control-height` or more for controls, `field-height` for fields and `pane-header-height` for pane headers.

### Responsive behavior

The minimum width is `x-zsd.layout.viewport-min`. Below `x-zsd.breakpoints.drawer`, drawers cover the work area. Below `x-zsd.breakpoints.compact`, secondary title and status items hide. Below `x-zsd.breakpoints.narrow`, dialog rows wrap. Long names end with an ellipsis and keep a tooltip.

## Elevation & Depth

Depth comes from glass, hairlines and shadows.

- The window is transparent. Windows 11 paints the acrylic material behind it.
- The window theme follows the look: light for Smooth light, the Windows theme for Smooth Windows, dark otherwise. Windows 11 ignores a tint color.
- The frame is one sheet of glass. Its bars paint nothing on `glass-ground`.
- Panes, menus, dialogs and notices are denser sheets on the frame. Sheets blur the content under them with `x-zsd.elevation.glass-blur-radius` and saturate it with `glass-saturate`.
- Menus cast a shadow down by `x-zsd.elevation.shadow-menu-y` with `shadow-menu-blur`. Dialogs use `shadow-dialog-y` and `shadow-dialog-blur`. Each look takes its `shadow-color` entry.
- Hairlines on glass use `glass-edge`. They are light in the dark and dark in the light.

Read [desktop-shell.md](desktop-shell.md) for the procedure and the measured values.

## Shapes

- `none`: Use for the window, the bars, drawers and all ONI MODE corners.
- `sm`: Use for buttons and fields in Smooth.
- `md`: Use for panes, menus and notices in Smooth.
- `lg`: Use for dialogs in Smooth.
- `full`: Use for filter chips and status dots in Smooth.

## Components

### Button

Use `button-primary` for the main action of a group. Use `button-secondary` for other actions, with `on-surface` text. Use `button-danger` only to delete data or stop an agent. Toolbar buttons have no fill until the pointer is on them.

- On hover, use `button-primary-hover`, `button-secondary-hover` or `button-danger-hover`.
- On press, the fill is `outline-variant`. A disabled button has a lower opacity.

### Input

Use `input` with a visible label above it. On focus, the boundary takes `primary-text`.

### Caption

The title bar is HTML. The `caption` buttons minimize, maximize and close the window. On hover they use `caption-hover`, or `wash-hover` on glass. Close uses `caption-close-hover`. A native child window over the maximize button gives Windows 11 Snap Layouts.

### Frame

The `frame` is the title bar, the toolbar, the sidebar and the status bar on glass. Its dividers and hairlines use `frame-edge`. Its text is not selectable.

### Brand

The `brand-mark` in the title bar takes the accent as its text color.

### Menu

Use `menu` for a list that opens from the toolbar. Escape or a click outside closes it.

### Dialog

Use `dialog` for a decision that stops the workspace, over `dialog-scrim`. A sheet docks to its control. A drawer opens at the right. The actions stay at the bottom.

### Pane

Use `pane` for each terminal or view. The active tab uses `pane-tab-active` with a `primary` underline. The focused pane has a `primary` boundary in Smooth.

### Notice

Use `notice` for short status at the bottom center. A notice does not take focus.

### Status

Use `status-live` and `status-warning` for state marks, always with a text label.

## Do's and Don'ts

- Do use the tokens of this file for each color, size and radius.
- Do keep the window transparent and paint its ground with `glass-ground`.
- Do set the window theme from the look before the first frame.
- Do keep chrome text unselectable and content text selectable.
- Do give each status mark a text label.
- Do not paint a fill on the frame bars over the glass.
- Do not put light glass on a dark acrylic material, or dark glass on a light one.
- Do not keep browser behavior in the shell: reload keys, the page context menu, a hand cursor on buttons.
- Do not add a second accent color.

## Motion

Use `x-zsd.motion.duration-fast` for hover and press, and `duration-base` for menus, sheets and drawers. Arrivals use `easing-out`, or `easing-out-oni` in ONI MODE. Departures use `easing-in`. With reduced motion, states change at once.

## Iconography

Icons are inline SVG symbols in `index.html`, with a thin stroke and no fill. An icon-only control has an accessible name and a tooltip.

## Accessibility

Focus uses the `x-zsd.accessibility` values. Each text token has 4.5:1 or more on each surface token in each mode, and `presentation.test.ts` checks this. On glass, check the composited color with `desktop-shell.md`. Each action works from the keyboard. Escape closes the top sheet.
