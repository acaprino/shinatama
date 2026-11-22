# F34: the desktop composed with Imago (units F34.1 to F34.5)

Status: F34.1 to F34.4 verified; F34.5 open until Imago and imago-shell are pinned to the same release tag, which
the owner publishes by merging Imago's `develop` into `main`.

Every check below ran either on this machine (the Node, Chromium and Imago gallery checks, which build no Rust) or
on GitHub Actions "Verify Windows" (everything that needs the desktop executable: Rust is built and tested there
only). The CI runs are named by commit; their reports are the `verification-windows-<sha>` artifacts.

Imago and imago-shell are pinned to the same commit of Imago's `develop`, `de75226` (v0.1.4 plus the drawer, the
DOM roles, the document panes and the overview strip). Imago's own CI ("Verify", which also runs `cargo test -p
imago-shell` and the whole gallery) passed on every one of those commits.

## F34.1 Projects, roots and the folder choice

| What | Result |
|---|---|
| The two path fields of "Project roots" are Imago `pathField`s with the `commit` option (Imago `acf40d0`), commit `2044e2d` | done |
| `projects-smoke`, `new-project-smoke` and `desktop-smoke` on Verify Windows, commit `bff1131` (run 44925040941, 27/27) | passed |

## F34.2 Commands, sidebar and panels

| What | Result |
|---|---|
| Every drawer is a layer of Imago's one stack (`openDialogDrawer`): a menu or a sheet over a drawer closes first, a dialog makes it inert, a drawer opened from inside another covers it, its control closes it from the pointer or the keyboard; Escape in a terminal stays the agent's. Commit `c054e52` | done |
| A drawer sits beside the panes: from 900px the work area gives it its width. Commit `09f5041` | done |
| No screen writes an Imago class: roles, tones and emphases by name (`uiTraits`, `uiTone`, `uiEmphasis`). Commits `07a444a`, `c0d2c88`; the ratchet's rule in `apps/desktop/tests/imago-ratchet.test.ts` | done |
| `imago-desktop-smoke` (production bundle, simulated native boundary): the Settings drawer is a drawer layer, the panes stay live and end where it starts, Escape in and out of the terminal, one action for one press; the attention widget's actions | passed locally |
| Imago gallery `surfaces.mjs` (drawers under both looks and both themes) and `tests/layers.test.ts` | passed |
| All the WebView2 smokes on Verify Windows, commit `e1fee0e` (run 44936833101): 26/27, the one failure the `new-project-smoke` race described under Limits; commit `f62c741` (run 44942028896), with the race fixed: every check of F34.1 to F34.4 passed | passed |

## F34.3 Workspace with live resources

| What | Result |
|---|---|
| The tiling workspace is Imago's `createTilingWorkspace`, with stable bindings by view ID. Commit `b4181c1` | done |
| `desktop-smoke`: three terminals dragged into new groups keep their IDs and keep running; `thread-sets-smoke`: tabs, closing and bringing back a session, a restart | passed (Verify Windows) |
| `pane-drag-smoke`: the three live DOM bindings survive moves, parking and reactivation; the three-way split | passed |

## F34.4 Terminal, files and browser

| What | Result |
|---|---|
| The files tool shows its document in Imago's `documentPane`s (source, rendered, picture, visual editor). Commit `6e42bac` | done |
| The terminal's minimap is Imago's `overviewStrip`; the archived lines open in an Imago overlay. Commit `6543a37` | done |
| `specialist-layout.browser.mjs` and `files-tree.browser.mjs` (the real modules and styles in Edge) | passed |
| `files-smoke`, `minimap-smoke` on Verify Windows (added to `scripts/verify.ps1` by `9ace60f`), commit `f62c741` (run 44942028896, 29/30): the minimap follows a drawer that narrows the terminal | passed |

## F34.5 Team map and closing

| What | Result |
|---|---|
| The team map takes the graph editor's roles by name (Imago `e368e5c`). Commit `c0d2c88` | done |
| `team-map-smoke` on Verify Windows, commit `7fe3859` (run 44948332289, 30/30). On the way it found a product defect: after a restart the window takes its layout over and loads it a second time, and that load replaced a tab opened meanwhile (the other thread's team map, opened while the layout was claiming); `9b5099d` keeps it. The smoke also closes a box that lies over a card before pressing it, and measures the arrow port on screen | passed |
| Imago and imago-shell pinned to the same release tag: both at `8418cca`, the commit of Imago's tag `v0.2.0` (`e0d43ce`) | done |

## Imago v0.2.0's canon (2026-11-22)

Branch `feat/imago-canon`, merged into `develop` as `77c1b48`.

| What | Result |
|---|---|
| The window is the canon's `im-shell`: title bar with the sidebar toggle, the thread sidebar as Imago's sidebar region (`im-row` options of one listbox, the rail, the fold, `sidebarResize`), the content region (toolbar as its tools row, the tiling workspace as its body), the inspector as the one side panel (Model proxy, the conversation, Leftovers), the footer. `mountApp` attaches the layers, F6, the registry, Ctrl+K and Ctrl+F. Commits `e0d43ce`, `e2a1ce9`, `12d4f5f`, `d55b642` | done |
| A focused terminal sends Ctrl+K and Ctrl+F to its program; F6 leaves it; the footer's keys come from the registry. Commit `85c2010` | done |
| Notices are Imago's (`notify`), the open in progress is said in the footer. Commit `af2fc0e` | done |
| Every destructive confirmation names its verb and object; deleting a closed session, forgetting the index, stopping the Model proxy and restarting ask in Imago's `confirm`, Cancel first; removing a root is at once, with Undo. Commit `03d43bc` | done |
| ONI has a light theme; the look and the theme are the five Appearance commands; a skin paints ONI dark only. Commit `61c4c30` | done |
| Icon-only controls are `im-icon-btn` with a tooltip; no hand cursor; the executables' icons are the package's (`shinatama.exe` through `crates/cli/build.rs`). Commits `4e7ce8a`, `d8470f2`, `3020606` | done |
| The thread sidebar's states are Imago's panel states (empty with Choose projects, loading after 300ms, error with Retry). Commit `f7a347d` | done |
| A preference save refused as busy by a load still returning is sent again instead of blocking saving (it broke `desktop-smoke`'s skin step). Commit `2c593f9` | done |
| `imago-check`: no findings (two exceptions with their reasons); it runs in `scripts/verify.ps1`. Commit `b64ccbe` | passed |
| `npm test` (221), `npm run build`, `npm run test:imago`, `sidebar-groups-smoke` (18 cases), `pane-drag-smoke` on Windows 11 | passed |
| Verify Windows run 44965147062 on `d4285d3` | passed, 31/31 |


- `new-project-smoke` failed about one run in three on the runner, on "Reading project roots…" for good while the same
  command answered in 4 ms. The cause: the browser fires a dialog's close event after it has closed, and under load
  after the next opening (the next key press handled first); the handler then advanced the dialog's generation and
  dropped the new opening's read without a repaint. Commit `f62c741` (its message names only its smoke changes) guards
  every close handler that resets its screen against an event that finds the dialog open again;
  `new-project.browser.mjs` reproduces the race in Edge and `dialog-close.test.ts` keeps the handlers guarded. An early
  experiment that ruled this out was wrong: it opened and closed the dialog within one task, where Chromium drops the
  event.
- `browser-smoke` needs `shinatama-moli.exe`, which Verify Windows does not build: the browser tab's controls are
  checked by `specialist-layout.browser.mjs` only.
- The Claude Code part of `desktop-smoke` is skipped on the runner, where Claude Code is not installed.
- `native-picker-smoke` (the Windows folder picker) does not run on the runner.
- The canon's remaining parts are written on branch `imago-rest` (2026-11-22) and wait for Verify Windows: the panel
  states of the other panels (Overview, Leftovers, Sessions' saved list, the conversation's first read, a session's
  first read; the archive browser and the search index are forms with status lines and keep theirs), the toolbar's
  panels and the session views on the side panel (a stack: a view opened from inside covers its opener), Files as a tab
  of the tiling workspace, Settings as the last view tab of the title bar. Nothing of it was run on the machine that
  wrote it (the owner's rule); `imago-check` was. Settings keeps its Save button for the sections that wait for it
  (the design file's "Settings save themselves" is not done). The design file has the app pick the look and the theme
  with the palette's commands only, where the plan's amendment allowed controls in Settings too: Settings has none. The
  session helper (a bin of `shinatama-engine`) has no icon.
- `launch-preflight-smoke` raced the window's diagnostic log (`shinatama.log`, written when the start ends); since
  `d4285d3` the folder is compared without it. `rust-tests` (the contained terminal's synthetic memory fault ended in a
  stack overflow before its marker), `new-project-smoke` (CDP port occupied at start) and `maintenance-smoke` (no idle
  tick before its deadline) failed once each on run 44962045846 and passed on the next run, with no change to them.
