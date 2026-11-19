# U10 — smoke tests through the harness menu

Item: U10 (verification register, section 14). Date: 2026-11-11. Status: partial.
Machine: Windows 11 Home 10.0.26200. Build: debug `shinatama-desktop.exe` with
`custom-protocol`, built from the working tree at the time of the run. Codex CLI 0.167.1.
`TEMP` and `TMP` pointed at `%LOCALAPPDATA%\Temp` so that the private data folder of each test
is outside any Git project.

## What changed in the tests

New session no longer opens a dialog (ADR-0039 v2, FRD-0001 R68). Nine smoke tests opened one;
they now call `startSession` in `apps/desktop/tests/session-flow.mjs`, which presses the control
and then does what a user does:

- no project selected: the project finder opens; it chooses the folder through Roots… and
  presses the control again;
- the wanted folder is the selected project: it picks the harness in the menu;
- another folder: it uses Other folder… and the folder sheet.

A refusal is read from the sheet's inline error or from the error notice of the menu route.
The menu now carries the project's full path (`data-project-path`, and the heading's tooltip).

Three steps that were out of date for other reasons were also corrected, because the tests
could not reach their end otherwise:

- `thread-rename-smoke`: "Reload threads" is a visually hidden control since `e0ff1d8`; it is
  activated by keyboard. The stop at the end uses the tab's close control.
- `desktop-smoke`: the theme and the pane's split, move and hide controls sit behind the two
  "⋯" menus since `e0ff1d8`; the project button opens a finder, not a dialog.
- `desktop-smoke`: a Claude Code thread is bound to its native conversation at launch; the
  test expected "No native session linked" and now expects `recorded_open`, as
  `new-session-smoke` already did. This is a product expectation, not an interface one.

## Results

| Test | Result |
|---|---|
| `new-session-smoke` (`SHINATAMA_SMOKE_SESSION_HARNESS=powershell`) | pass: finder with no project, menu, Other folder… |
| `desktop-smoke` | pass, including a real Claude Code start from the menu |
| `attention-smoke` | pass |
| `thread-rename-smoke` | pass |
| `claude-resume-smoke` | pass |
| `claude-resume-race-smoke` | pass |
| `codex-native-smoke` | pass after the tests answer the hook review (see below); failed before with `first-native-binding-deadline` |
| `codex-resume-smoke` | not confirmed: same change, but the open was refused with `Ledger(Capacity)` on a loaded machine |
| `forced-crash-recovery-smoke` | fail `sidecar-not-ready`, at stage `app1-start` |
| `ui-flow-smoke` | fail at its first step: it looks for an "Open terminal" button |

## What the failures are

- The two Codex tests opened their session through the menu and failed later: no thread
  activation within 60 seconds of the first prompt. Cause, seen on the terminal: the first turn
  stops at Codex's screen "Hooks need review. 4 hooks are new or changed", with the choices
  "1. Review hooks", "2. Trust all and continue", "3. Continue without trusting (hooks won't
  run)". The four hooks are those of the bundled plugins, which Codex does not trust (ADR-0052,
  item 7); both tests passed at `4fed71b`, before the plugins were added in `4ab5470`.
- Fix in the tests: they answer 3, as a user who grants nothing. Shinatama's own hook is trusted
  by its hash and still binds the session. `codex-native-smoke` then passes (12 checks, among
  them `pluginHookReviewDeclined`). `codex-resume-smoke` carries the same change but could not
  be run to the end: three attempts were refused at open with `Terminal admission was refused:
  Ledger(Capacity)` while the machine had about 3.7 GiB of commit free (six Codex and five
  Claude processes of other work were open). That refusal was not investigated.
- The product still shows that screen to the user on the first turn of every Codex session.
- `forced-crash-recovery-smoke` fails before it touches the interface. Not investigated.
- `ui-flow-smoke` runs the interface against scripted IPC and was last changed on 2026-11-03;
  it describes "Open terminal", always-visible pane controls and panels that block the page.
  It was not rewritten.

## Not covered

- No test asserts that a terminal stays usable while a drawer or a sheet is open beside it.
- `new-session-smoke` with Claude Code, Codex, OpenCode or Pi as the harness was not run;
  `desktop-smoke` covers the Claude Code start.
