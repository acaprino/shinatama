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
| `tests/` | The tests: the models and the controllers (`*.test.ts`, run by `npm test`), the smokes of the page on the mocked commands (`*.smoke.mjs`, one per area and `app.smoke.mjs`, with `smoke-lib.mjs`), and `native/`, the probes of the built exe (Windows only) |

## Controllers

`app.ui` (view, dialog, inspector stack, notices, status line), `app.projects` (current and recent projects), `app.layout` (panes and tabs,
saving, the tree for Sigrid's `Workspace`), `app.terminals` (sessions that run, their output and input, how they end, the window's close),
`app.threads` (threads on and off, their sessions, tab names and colours, the sidebar's lists), `app.chat` (threads that talk, unread marks),
`app.opens` (new sessions, resuming saved ones, the open queue, the agents), `app.hop` (runs the hop the person confirmed in the inspector), `app.sessions`, `app.files`, `app.fileTabs` (the files open in a pane), `app.team`, `app.browser`,
`app.settings` (terminal presentation and the Settings page: what waits for Save, the switches that apply at once), `app.services` (Muro, the model proxy, memory safety, alerts), `app.catalog` (the project picker, the roots, the creation of a project: folder operations Rust can cancel, `project-mutation.ts`), `app.overview` (the projects at a glance, worktrees).
`app.ui`, `layout`, `terminals`, `threads`, `chat`, `opens`, `projects`, `hop` and `settings` carry the logic of the old `main.ts`; `sessions`,
`files`, `browser` and `services` hold the state and the calls their screens need and are completed by the area's agent. `team` opens the team map of a thread and keeps one `TeamMap` (`controllers/team-map.ts`) for each: the formation being edited, what the engine says of it, Apply and Discard, the templates. The map that is on screen (`team.shown`) is what the window's inspector (`screens/team/inspector.tsx`: an agent, an arrow: both are chosen on the map, an arrow by a press on it or on its words, or by the keyboard (Tab to the arrows, the arrows between them, Enter; `selectedEdge` and `onSelectEdge` of Sigrid's `GraphCanvas`), as well as from the lists; one choice at a time, `TeamMap.chooseAgent` and `chooseArrow`) and its actions row (Apply, Discard) show; Settings has the actions row too (Reload, Save).

The inspector's controllers: `app.sessions` holds the library (the running sessions, the archives read one after the other, the text search,
the index of session text) and, as small stores of their own (`controllers/session-*.ts`), `detail` (one conversation, a page at a time),
`find`, `raw`, `exporter`, `hopPreview` and `archiveBrowser`; `app.files` the project files (tree, search by name, buffers, saving against the
version read); `app.leftovers` what agents left on disk; `app.threadSettings` the settings of one thread. Their pure parts are
`session-library-model.ts`, `session-detail-model.ts`, `session-export-model.ts`, `session-index-model.ts`, `instances-model.ts`, `files-model.ts`,
`thread-settings-model.ts` and `sessions-model.ts`.

## The inspector

`ui.panels` is a stack of `PanelSpec` (`screens/inspector/spec.ts`) and the panel on top is drawn by `screens/inspector/index.tsx` in Sigrid's
`Inspector` through `Frame`, which gives the title, the actions at the foot and a way back to the panel it covers. A panel opened from inside
another (`covers` in the spec: a conversation from its facts, a search in it, a raw entry, an export, a hop) covers it and the way back gives
it back as it was; any other panel opened while one is shown (a toolbar button, a command) takes the inspector, and another view starts
without panels. A panel that holds work (the files) asks before it is closed.

## Who owns which file

| Area | Folder and files |
|---|---|
| workspace (panes, terminals, thread sidebar, tabs, thread chat) | `screens/workspace/`, `controllers/{layout,terminals,threads,chat,opens}.ts` (the tab renderer draws `Terminal`, the team map and the chat) |
| inspector | `screens/inspector/` (and its `spec.ts`), `controllers/{files,sessions}.ts` |
| team | `screens/team/`, `controllers/team.ts`, `team-map-*.ts` |
| sessions | `screens/sessions/`, `controllers/sessions.ts` |
| settings | `screens/settings/`, `controllers/{settings,services}.ts` |
| dialogs | `screens/dialogs/` (and its `spec.ts`), `controllers/ui.ts` for the dialog state (the hop is a panel of the inspector, `screens/inspector/hop.tsx`, run by `controllers/hop.ts`) |
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

- `npm test` runs the model tests and the controller tests (`tests/controllers.test.ts` and the others drive the real controllers on `tests/fake-host.ts`, a
  table of answers in place of Rust).
- The smokes of the page (`npm run test:workspace`, `:inspector`, `:sessions`, `:team`, `:settings`, `:dialogs`, `:widget`, `:browser` and `:app`) run the window's
  page on Vite with `src/mock/` answering for Rust and drive it by roles and names, never a class. `tests/smoke-lib.mjs` is what they share: the browser (Playwright's
  Chromium here; Edge on the Windows runner, by `SHINATAMA_BROWSER_CHANNEL=msedge`, so that no step downloads one), the dev server, and what a page may say: every
  console error and warning is a failure (but Chromium's own scheduler noise). `app.smoke.mjs` is the one between the areas: the
  three views (title, status strip, Ctrl+F), the palette, a new session from the palette to its terminal, the terminals alive across the views, a file opened in a pane,
  the theme, the zoom and the glass (which only a window of its own has), and the frame at 340, 620, 1000 and 1440 in both schemes.
- `tests/native/` holds the probes of the built exe (Windows only, driven over the WebView2 debugging port; `native/lib.mjs` starts the app on a private data folder):
  `window`, `threads`, `files`, `projects` and `settings` run in `scripts/verify.ps1` inside the contained test job (`workspace` is left out: the terminal fills its pane now and it runs, but on the runner a press on a printed path opens no browser tab and the PowerShell tab of a `shell_open` shell is not back after a kill; whether the app or the probe is wrong is not known); `sessions`, `picker`, `browser`,
  `attention`, `interactive-limits`, `model-provider` and the agent proofs (`claude-resume*`, `codex-*`, `forced-crash-recovery`, which start a real agent against a
  synthetic provider) are run by hand. They were written against roles in a cloud session without Windows and have not been run there.
- `scripts/verify.ps1` runs all of it, after `imago-check` and `sigrid-check`.

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
Imago's `startApp(..., { secondary: true })` (Imago 1.4.0, with Sigrid's material `clear`, 6.12.0): the entry passes the option, and the window's page asks the plugin nothing and paints no ground.

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

The page is drawn by Sigrid's `FrameSurface` (6.13.0), which `surface.tsx` wires to `app.browser`. Proved by `npm run test:browser` (mock, roles, in
`tests/harness/browser.html`) and `tests/browser-native.mjs` (Windows).

## The workspace

`src/screens/workspace/` draws the Workspace view's panes and everything in them; `controllers/{layout,terminals,threads,chat,opens}.ts` keep what
they show.

| File | What it is |
|---|---|
| `index.tsx` | Sigrid's `Workspace` over `app.layout.tree(...)`: the tab renderer (a session, the thread's conversation, its team map, a browser page), the tab's menu (Find in terminal, Thread settings), the close of a tab (a running session stops after asking and moves to Closed sessions), the drop of files on the window, and one pane at a time in a window under 620px (`maximizedId` is the pane the person works in, with a row of "Pane n" buttons) |
| `terminal-tab.tsx` | A session's tab: Sigrid's `Terminal` wired to `app.terminals` (output, input, size, links, the window's own chords through `appKeys`, paste and drop of files and images), its `OverviewStrip` with the archive (and a tick on each line that holds what Find in terminal searches, while the search is open), a foot (state, memory, size, folder) and, when the process ended, "New session" and "Close terminal". A tab whose session does not run is a card: Resume, Recover session, Remove, or where its open stands |
| `terminal-history.tsx`, `src/terminal-history.ts` | What a terminal no longer holds: the archive (lines that fall out of xterm's 1000 are kept as text with colours, in Blobs) and its view, a read-only `Terminal` in a drawer, a window of 2000 lines with Earlier and Later |
| `terminal-find.tsx`, `src/terminal-find.ts` | Find in terminal (Ctrl+F in the Workspace, Ctrl+Shift+F in a terminal, the tab's menu): a dialog with a search field over the last 5500 retained lines |
| `terminal-io.ts` | The clipboard and the window's drop (`tauri://drag-drop`): paths of files and images, quoted for the reader (`src/terminal-input.ts`), pasted as one paste |
| `thread-chat.tsx` | The conversation of a thread that talks (ADR-0057): `Conversation`, `Message`, `PromptInput` and the recipients' `Select`; the room is read by `app.chat.watch` while the tab is shown |
| `sidebar.tsx` | The thread sidebar: a group per project, a row per thread (its colour dot, its state line, + and ⋯, a menu on the row), the filter (Sigrid's `Sidebar search`), Show archived threads |
| `file-tab.tsx`, `controllers/file-tabs.ts` | A file open in a pane ("Open in a pane" of the Files panel; view `file-<hash>`, kind `files`): its text in `CodeView`, edited in place and written by Save against the version read (`writeBuffer`, shared with the panel), a dot on the tab while it has changes, a question before it is closed, read again after a restart |
| `tools.tsx`, `telemetry.ts` | The tools of the title bar (New session, Files, Leftovers) and the window's report of its own measures, once a second, for the resources badge of the status strip |

A terminal is in `ownKeysIn` (`main.tsx`): the shell runs no key of a command in it. The window's own chords (`src/keys.ts`) are run by the terminal's
`appKeys` (`commandOfKey`), and F6 and the zoom are left to the shell (`terminalLeavesKey`).

In development the browser shows a populated workspace: `src/mock/workspace.ts` is the saved layout (three panes), `mock/terminals.ts` four terminals
with output (typing `flood` in the shell prints 2500 lines, for the history), `mock/threads.ts` the projects' threads, their sessions and a room that
talks. The page storage starts with the threads that were on. Proved by `npm test` (`workspace.test.ts`, `thread-sidebar.test.ts`,
`terminal-*.test.ts`, `workspace-keys.test.ts`, `thread-chat-model.test.ts`) and `npm run test:workspace` (roles, both schemes, the widths where the
frame changes). What only a real window proves: the native drop of files, WebView2's paste of images, Snap Layouts, the first frame.

`Terminal` fills its box and `Cluster` takes `fill` (Sigrid 6.13.0).

Also pending in Sigrid: Settings keeps the terminal's font family and "Follow the interface theme in terminals" and Rust uses them (`presentation.rs` gives the agents their ANSI
themes), but Sigrid's `Terminal` takes no font family and no palette, so the page draws the size and the line height only. Where the shell's keys and strip stand: Ctrl+F is the
search of the view (the terminal's in the Workspace, the filter in Sessions, none in Settings, where the shell says so); the strip of Sessions says whether the archives were
read, how many sessions there are and whether the search index is on.
