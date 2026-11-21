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
| `team-map-smoke` on Verify Windows: in run 44942028896 a click on a card the open arrow box covered on the runner's smaller map; the smoke now closes the box first, as a user would, and passes 3 times out of 3 on the executable that run built | awaits the next run |
| Imago and imago-shell pinned to the same release tag | open: needs the owner's Imago release |

## Limits

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
