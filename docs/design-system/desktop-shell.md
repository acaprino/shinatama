# Desktop shell: glass, window and native-feeling components

How Shinatama makes a Tauri 2 window on Windows 11 look and behave like a desktop
application with a glass material, and how another app of the package (Muro, Mukade, Xiox,
Ethel) reuses it. The design values are in [DESIGN.md](DESIGN.md) and in `tokens/`; this file
holds the techniques, the reasons and the measurements behind them.

Checked against: Tauri 2.12.5, tauri-runtime-wry 2.12.3, tao 0.37.2, wry 0.57.1,
window-vibrancy 0.8.1, WebView2 on Windows 11 Home 10.0.26200 (2026-11-19).

## 1. The glass technique

### 1.1 What Windows paints

The main window is transparent and asks for the acrylic effect
(`apps/desktop/src-tauri/tauri.conf.json`):

```json
{ "label": "main", "create": false, "decorations": false, "transparent": true,
  "backgroundColor": "#00000000", "windowEffects": { "effects": ["acrylic"] } }
```

- On Windows 11 22H2 and later, `window-vibrancy::apply_acrylic` sets the DWM system backdrop
  `DWMSBT_TRANSIENTWINDOW`. That material is the system's own: **it ignores the tint colour of
  `windowEffects`** and follows only the window's light or dark theme. The tint reaches only
  Windows 10, where the legacy `SetWindowCompositionAttribute` path is used (and is slow to
  drag).
- The acrylic is a blur of what is behind the window with a nearly fixed luminosity. Measured
  from the screen (0-255): light acrylic about **143 over a dark window and 226 over a white
  one**; dark acrylic about **84 over a dark window**
  ([U9 note](../verification/2026-11-11-U9-glass-on-screen.md)). A light acrylic over dark
  content is therefore a mid grey, and a dark one over bright content a lighter grey.
- Windows drops the material (solid fallback) when the window is inactive, in battery saver,
  with transparency off in Settings, and in high contrast. The page cannot detect most of these
  states, so every layer the page paints must keep its text readable on its own.

### 1.2 The window theme follows the look

The theme of the window decides the acrylic. A dark acrylic under light surfaces is exactly
what turned Smooth light grey before 2026-11-19. The rule:

| Look | Window theme |
|---|---|
| Smooth, theme Light | `light` |
| Smooth, theme Dark | `dark` |
| Smooth, theme Windows | `system` (no theme of its own: the window follows Windows) |
| ONI MODE (Classic) | `dark` (always) |

Implementation in Shinatama:

- `presentation.ts`: `windowThemeFor(look, theme)` gives the value; `applyWindowTheme()` runs in
  `apply()` and sends it only when it changes:
  `invoke('presentation_window_theme', { theme })`.
- `presentation.rs`: the command `presentation_window_theme` calls
  `WebviewWindow::set_theme(Some(Light|Dark) | None)` and writes the word to
  `<data home>/window-theme`.
- `webview_recovery.rs`: `build()` reads that file (`saved_window_theme`) and puts the theme in
  the main window's config before building it, so the **first frame** already has the right
  acrylic; a rebuilt window after a WebView2 crash gets it too.
- `tauri.conf.json` declares no theme: with no saved file (a fresh install) the window follows
  Windows, which matches Smooth's default theme.

`set_theme` reaches DWM through tao (`DWMWA_USE_IMMERSIVE_DARK_MODE`), and tauri-runtime-wry
passes the new theme to WebView2's `PreferredColorScheme` on `ThemeChanged`. A custom app
command needs no extra capability; the JavaScript API `getCurrentWindow().setTheme()` would
need `core:window:allow-set-theme` and cannot persist the choice for the first frame.

### 1.3 The layers

From the back to the front, each look's values (tokens in DESIGN.md):

| Layer | Smooth dark | Smooth light | ONI MODE |
|---|---|---|---|
| Acrylic material | dark | light | dark |
| Ground (`#app`): `glass-ground` | glass tint `#0C111D` at 80% | white at 88% | stage at 86% |
| Frame bars (title bar, toolbar, sidebar, status bar) | nothing painted | nothing painted | `--glass-bar` (chrome + line-strong, 66%) |
| Terminal panes: `terminal-glass` | terminal colour at 58% | white at 58% | profile colour at 58% |
| Menus, dialogs, notices: `glass-sheet` | tint at 66% + blur 18px, saturate 1.5 | same | same |
| Hairlines and window outline: `glass-edge` | text at 16% (light lines) | text at 11% (dark lines) | line-strong at 40% (cobalt) |

Code: `styles.css` (the `.native-glass` block, ONI MODE) and the last block of `smooth.css`
(Smooth). `main.ts` adds `native-glass` to the root in the desktop app only; the browser
preview keeps opaque colours.

Why these values:

- **The frame is one sheet of glass.** Bars with their own fill over a dense ground made the
  title bar and toolbar one opaque band: in Smooth dark it measured 32,35,41, the colour of
  Windows 11's own dark title bar (32,32,32), which is why it read as a native frame.
- **The dark tint is darker and bluer than the stage.** The dark acrylic is neutral grey; mixed
  into a neutral slate it gives Windows grey. A deep cool slate keeps the frame at about
  23,26,37 over a dark window: glass in Shinatama's colours, not the system caption.
- **Light frost is white and denser.** Smooth's grey ground over a light acrylic over a dark
  window gave 228-230 (grey). White at 88% gives about 241 there and about 251 over a white
  window.
- **Terminals stay dense.** Text contrast on terminals must not depend on the desktop behind.

### 1.4 Rules

- Never put light glass on a dark acrylic material, or dark glass on a light one: set the window
  theme from the look before the first frame.
- Never paint an opaque fill on the frame bars over the glass; a band of one colour along the
  top reads as the system's title bar.
- Keep a dark ground away from Windows' caption grey (32,32,32): tint it.
- Keep hover and pressed fills translucent on glass (`wash`, `wash-hover`).
- Check contrast on the composited colour, not on the token alone.

### 1.5 How to check it on screen

A webview screenshot draws the acrylic as white: it cannot judge the glass. Photograph the real
window from the screen:

- `node apps/desktop/tests/glass-capture.mjs` starts the debug app with an isolated data folder,
  opens a PowerShell session and copies the window from the screen
  (`apps/desktop/tests/glass-capture.ps1`).
- Raise the window once (topmost) and keep it there for every capture of a run; a window
  lowered between captures ends up behind the window in use.
- Discard a capture whose title bar pixel is not the app's: it photographed the window in
  front. Switch looks through `#presentation-theme` (a `change` event), not by reloading.

Measured on 2026-11-19 over a dark terminal window: Smooth dark frame 23,26,37 (before
27-37 neutral grey), Smooth light frame 241,241,242 (before 230,231,235), ONI MODE 12,15,30
(unchanged).

## 2. The window: Tauri techniques

| Behaviour | How | Source |
|---|---|---|
| No white flash | Transparent window, `backgroundColor: #00000000`; wry gives WebView2 a transparent default background | `tauri.conf.json` |
| Windows built after the app state | `"create": false`, built in `setup` once every service is managed | `main.rs`, `webview_recovery.rs` |
| HTML title bar with Snap Layouts | `decorations: false`; a child window that never paints lies over the HTML maximize button and answers `HTMAXBUTTON`; the page reports the button's rectangle | `native_frame.rs`, `window-controls.ts` |
| Dragging | `data-tauri-drag-region` on `.titlebar-drag`; its children ignore the pointer | `index.html`, `styles.css` |
| Snap zones | `minWidth: 500` | `tauri.conf.json` |
| Not a web page | Navigation limited to the app origin; new windows refused; browser accelerators off in release; reload keys refused outside terminals and editors | `native_frame.rs`, `desktop-shell.ts` |
| Reload when hung | "Reload interface" in the window menu (Alt+Space) | `native_frame.rs` |
| WebView2 crash | Windows destroyed and built again in place; a crashed renderer reloaded at most three times a minute | `webview_recovery.rs` |
| Chrome versus content | Chrome is not selectable and keeps the arrow cursor; fields, panels, notices and code are selectable | `styles.css` |
| Zoom | Ctrl+= / Ctrl+- / Ctrl+0, 50-200%, saved per machine | `desktop-shell.ts` |
| Theme | Window theme from the look, saved for the first frame (section 1.2) | `presentation.rs` |

## 3. Native-feeling components

In-page components that behave like the operating system's. None uses `showModal()`: that makes
the custom title bar inert and leaves a borderless window that cannot be moved, minimized or
closed (`modal.ts`).

- **Title bar and caption buttons** (`index.html`, `window-controls.ts`): three 44-pixel buttons
  at the right edge, full height, close turns to the accent on hover. The maximize button gets
  Snap Layouts from the native child window, so hover and press states come from Windows events
  (`os-hover`, `os-pressed`).
- **Toolbar menus** (`toolbar-simple.ts`): a popover under its control, glass with blur and a
  shadow; Escape and a press outside close it and give focus back to the control.
- **Drawers** (`modal.ts`, `openModal`): docked to the right edge between the toolbar and the
  status bar; no scrim, nothing inert, terminals keep working beside it; one side panel in view.
  Below 1000 pixels a drawer covers the work area instead of pushing it.
- **Modal dialogs** (`modal.ts`, `MODAL_ONLY`): only for decisions about the whole workspace;
  everything below the title bar becomes inert; containment and Escape are explicit; actions
  stay in a sticky bar at the bottom.
- **Sheets** (`modal.ts`, `openSheet`): docked to the control or pane they belong to (below, pane
  top, pane bottom); a press outside dismisses one the way Escape does; one at a time.
- **Notices** (`#notice`): bottom centre, `role="status"`, never take focus; errors say Error in
  text, not only in colour.
- **Panes and tabs** (`workspace-renderer.ts`): panes split by splitters, tabs with a close
  button, drag to rearrange; the same geometry in every look.
- **Attention widget** (`widget.html`): a separate transparent, always-on-top window without
  effects, that never takes focus.

## 4. Reusing it in another app

1. Take the tokens: `tokens/smooth-dark.css`, `tokens/smooth-light.css` and `tokens/oni.css`
   define `--zsd-*` custom properties (for example `--zsd-colors-primary`); the
   `*.tokens.json` files are the same values in DTCG 2025.10 for any other tool or a terminal
   UI. Map them to the app's own variable names in one place.
2. Declare the window as in section 1.1 (transparent, clear background, acrylic, no fixed theme).
3. Add the theme command of section 1.2 (about 30 lines of Rust: parse, `set_theme`, a file in the
   data folder, read it back before building the window) and call it from the page whenever the
   look or theme changes.
4. Paint the layers of section 1.3 under a root class set only in the desktop app, so a browser
   preview stays opaque.
5. Apply the window behaviours of section 2 that the app needs; the Snap Layouts child window
   only matters with an HTML title bar.
6. Check the result with a screen capture (section 1.5) in each look, over a dark and a bright
   window.

To regenerate the exports after a change to DESIGN.md (zero-slop-design 0.2.1, shipped in
Marionette):

```text
node <zero-slop-design>/tools/zsd.mjs lint docs/design-system/DESIGN.md
node <zero-slop-design>/tools/zsd.mjs export docs/design-system/DESIGN.md --format css [--mode smooth-light|oni] --out docs/design-system/tokens/<look>.css --report docs/design-system/tokens/<look>.css.report.json --force
node <zero-slop-design>/tools/zsd.mjs export docs/design-system/DESIGN.md --format dtcg [--mode smooth-light|oni] --out docs/design-system/tokens/<look>.tokens.json --report docs/design-system/tokens/<look>.dtcg.report.json --force
```

The export refuses a file with any lint finding, and DTCG has no shadow, filter or `clamp()`
type: keep those values split into dimensions, numbers and colours.
