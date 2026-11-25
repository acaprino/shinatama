# The desktop interface

Shinatama's window is React on Sigrid, in Imago's shell (`@acaprino/imago` 1.3.0, `@acaprino/sigrid` 6.11.0). The look, the frame, the
palette, the notices, the theme, the zoom and the glass are the shell's and Sigrid's; the app writes **no class, no style, no stylesheet
and no component that draws** (`npx imago-check` and `npx --no sigrid-check apps/desktop/src`, rule `own-class`, must be clean). The app
is its screens, made of Sigrid's parts, and the controllers that hold its data. The canon is `node_modules/@acaprino/imago/docs/design-file.md`.

## Layout of `apps/desktop`

| Path | What it is |
|---|---|
| `index.html`, `src/main.tsx` | The window's page: `defineApp` (id `shinatama`, the hue of the design file, storage keys `shinatama.theme`, `.zoom`, `.sidebar`, `ownKeysIn` for the terminal and the code view) and `startApp` |
| `widget.html`, `src/widget/` | The attention widget: its own page and its own window (see "The attention widget") |
| `src/store.ts` | `Store`, and the contract every controller and screen follows (read its header) |
| `src/app.ts` | `Shinatama`: the controllers, one field each, and `start()` (what the old `initialize()` did, in the same order) |
| `src/hooks.ts` | `useApp()`, `useController(store)` (a render when the controller changes), `useAfter` |
| `src/api/<area>.ts` | Every Rust command, typed. `core.ts`: `invoke`, `listen`, `errorText`, `appLog`. Controllers call these; screens never name a command |
| `src/controllers/<name>.ts` | What the page remembers, in plain TypeScript with no React and no DOM, so `npm test` runs it with node |
| `src/screens/shinatama.tsx` | The window: `AppWindow` with the views Workspace, Sessions, Settings; a sidebar per view; the status; the foot's keys |
| `src/screens/<area>/index.tsx` | One folder per area; the area's agent owns the folder |
| `src/*-model.ts`, `workspace-model.ts`, `session-*.ts`... | Pure models, with their tests in `tests/` |
| `src/mock/` | What the browser answers instead of Rust (development only), one file per area |
| `legacy/` | The interface of 0.4 (DOM modules, smokes): to be ported and then deleted; excluded from `tsc`, the checks and `npm test` |

## Controllers

`app.ui` (view, dialog, inspector stack, notices, status line), `app.projects` (current and recent projects), `app.layout` (panes and tabs,
saving, the tree for Sigrid's `Workspace`), `app.terminals` (sessions that run, their output and input, how they end, the window's close),
`app.threads` (threads on and off, their sessions, tab names and colours, the sidebar's lists), `app.chat` (threads that talk, unread marks),
`app.opens` (new sessions, resuming saved ones, the open queue, the agents), `app.hop`, `app.sessions`, `app.files`, `app.team`, `app.browser`,
`app.settings` (terminal presentation), `app.services` (Muro, the model proxy, memory safety, models, alerts).
`app.ui`, `layout`, `terminals`, `threads`, `chat`, `opens`, `projects`, `hop` and `settings` carry the logic of the old `main.ts`; `sessions`,
`files`, `team`, `browser` and `services` hold the state and the calls their screens need and are completed by the area's agent.

## Who owns which file

| Area | Folder and files |
|---|---|
| workspace (panes, terminals, thread sidebar, tabs, thread chat) | `screens/workspace/`, `controllers/{layout,terminals,threads,chat,opens}.ts` (the tab renderer draws `Terminal`, the team map and the chat) |
| inspector | `screens/inspector/` (and its `spec.ts`), `controllers/{files,sessions}.ts` |
| team | `screens/team/`, `controllers/team.ts`, `team-map-*.ts` |
| sessions | `screens/sessions/`, `controllers/sessions.ts` |
| settings | `screens/settings/`, `controllers/{settings,services}.ts` |
| dialogs | `screens/dialogs/` (and its `spec.ts`), `controllers/{ui,hop}.ts` for the dialog state |
| widget | `src/widget/`, `src-tauri/src/{widget_window,attention}.rs` |

A change to a file of another area is a request to its owner. `src/api/<area>.ts` and `src/mock/<area>.ts` belong to the area named.

## The window (Rust)

`imago_shell::plugin()` and `open_main` open the main window (Imago's grounds, glass off until the person chooses it in the palette or in
Settings with `useGlass`, the data folder of the app); `on_run_event` is in the run loop; the capabilities are `main.json`, `shell.json` and
`attention.json`. `reload_gate.rs`: before a reload the person asked for, the page saves its layout (`interface-reload-requested`, answered
with `interface_reload_ready`). The attention widget is built by the app (`widget_window.rs`), hidden, with the main window's browser arguments
and a navigation policy of its own; because the shell rebuilds only the main window when WebView2 loses its browser process, the widget is
destroyed with it and built again the next time an alert needs it.

## Tests

`npm test` runs the model tests and the controller tests (`tests/controllers.test.ts` drives the real controllers on `tests/fake-host.ts`, a
table of answers in place of Rust). The browser and desktop smokes that drove the old DOM are in `legacy/tests/`; each is rewritten against
roles and names (as Xiox's and Mukade's) and moved back to `tests/` by the area that owns the screen. `scripts/verify.ps1` runs what exists.

## The attention widget

`widget.html` is the page of a small window that floats over the desktop (always on top, never focused, transparent, built hidden by
`widget_window.rs` and shown by `attention.rs` when a notice arrives). `src/widget/widget.tsx` is its entry, `src/widget/index.tsx` its one
screen (a Sigrid `Card`: portrait in an `Avatar`, the kind in a `Text` tone, the title, the words, "Apri sessione" or "Apri Shinatama" and
"Chiudi avviso"), `src/widget/attention.ts` its store and `src/attention-model.ts` the pure part (kinds, words, the portrait's expression).
The text stays Italian. It asks Rust for `attention_current`, `attention_open` and `attention_dismiss` through `settingsApi` and listens to
`attention-changed`; Escape closes the notice. Proved by `npm run test:widget` (mock, roles) and `tests/attention-native.mjs` (Windows).

Imago's `startApp` is written for the one main window. In a second window it calls the shell's plugin (`kept_theme`, `kept_glass`,
`ready`), which refuses a window that is not `imago-shell`'s (console warnings), and it paints an opaque ground over the transparent
window. The page follows the scheme the main window keeps because both read `shinatama.theme` (same origin, the `storage` event). The fix is
Imago's `startApp(..., { secondary: true })` (branch `claude/secondary-window`, which needs Sigrid's material `clear`, branch
`claude/clear-material`): when both are released the entry passes the option, and the window's page asks the plugin nothing and paints no ground.

Rust side (`widget_window.rs`, `attention.rs`; Windows is where it is proved): the widget is built hidden at start and again by `ensure` when an alert
needs it and the shell rebuilt the main window; a lock keeps two alerts that arrive together from building it twice; it is destroyed with the main
window and hides instead of closing; it is placed at the bottom right of the work area of the main window's monitor (the primary one when the main
window has none), from its own size, shown with `SWP_NOACTIVATE` so it never takes the focus, and its webview gives its memory back while hidden.

## The browser tab

A page Rust drives through the browser engine (ADR-0056) is a tab of kind `browser` (view id `browser-<page>`). `src/screens/browser/index.tsx`
exports `BrowserTab({ viewId, visible })`, which the workspace's tab renderer mounts: a bar (Back, Forward, Reload, the address, the system's
browser), the strip that says an agent drives the page and offers to take it over, and the page (`surface.tsx`). A tab the layout kept whose
page is not open (after a restart) shows its last address and "Riapri". `app.browser` (`controllers/browser.ts`) holds the pages, sends the
person's input in batches (`browser-input.ts` maps it), the size and whether the tab is in sight, and hands the frames (JPEG, over a Tauri
channel, `api/browser.ts`) to the surface. The tab renderer names the tab with `app.browser.label(viewId)` and closes it with
`app.browser.close(viewId)`; the Files inspector opens a document with `app.browser.open({ file })`.

The page is drawn by Sigrid's `FrameSurface`, which is built (branch `claude/frame-surface` of acaprino/sigrid) and not released: until the app
pins that release `surface.tsx` shows the page's place, with the exact swap in its header. Proved by `npm run test:browser` (mock, roles, in
`tests/harness/browser.html`) and `tests/browser-native.mjs` (Windows).
