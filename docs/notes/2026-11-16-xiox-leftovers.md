# What sessions leave behind: Xiox's observer in Shinatama

Date: 2026-11-16. Status: implemented. The user chose this approach: Shinatama observes and
shows the results, and agents get no MCP server.

## The problem

Agent sessions leave folders behind: git worktrees, copies of the repository on another disk,
`<project>-bench` and `<project>-backup` folders, build folders that grow by gigabytes. Nobody
remembers them, and nothing in Shinatama showed them.

[Xiox](https://github.com/acaprino/xiox) already knows how to find them for one project, and
its `xiox observe <project>` (guide: `docs/xiox-observe.md` in Xiox) reports what appeared
since it started, one JSON line at a time, without ever removing anything.

## How it works

- **Program.** `crates/xiox` (`shinatama-xiox`) looks for `SHINATAMA_XIOX` (a path, or `off`),
  then `xiox\xiox.exe` beside Shinatama (or one folder up, for `cli\`), then `PATH`.
  `build-release.bat` downloads `xiox.exe` from the Xiox release pinned in
  `scripts/xiox-pin.json` (tag, commit, SHA-256; `scripts/fetch-release.ps1`, through `gh`)
  into `release\xiox\`; no Xiox source is built. The observer is given Shinatama's own
  `mukade\mukade.exe` as `XIOX_MUKADE_EXE`, so a package holds one Mukade. Without Xiox,
  Shinatama works as before and Projects says why nothing is observed.
- **Schema.** Shinatama reads `observe` schema 1 (Xiox `v0.1.0`). Xiox's `feat/step4` moves to
  schema 2, whose lines Shinatama ignores: a later pin needs the reader updated first.
- **When it starts.** `terminal_read` already sees every batch of every terminal (as
  `attention` does). `apps/desktop/src-tauri/src/leftovers.rs` uses that: the first batch of a
  new agent terminal (Claude Code, Codex, OpenCode, Pi; not PowerShell) starts one observer for
  its folder, and the end of that terminal asks the observer for a fresh `report`. The
  harness launches are unchanged.
- **One observer per project.** Xiox resolves a worktree to the main working tree. When a
  second observer reports the same root as one that is already running, it is stopped and its
  folder is added to the first one.
- **How long, how often.** Observers run until Shinatama exits: what a session left is still
  listed after the session ended. Xiox's own interval is 600 seconds. While an agent terminal
  of the project is open, Shinatama asks for a fresh check every 60 seconds (`report` on
  stdin). The end of an agent terminal asks for one at once. Observers run at below-normal
  priority with no console window and end when their stdin closes, so they cannot outlive
  Shinatama. There are at most 16 projects.
- **What is kept.** `shinatama_xiox::Picture` keeps only the items with `new: true` (the first
  snapshot is the project as the person left it), at most 200. It also keeps the sessions that
  Xiox's `provenance` names (at most 3 per item), growth, thresholds, the last error and the
  `cleanCommand`. A `report` replaces what the events said between reports.
- **Who made it.** The panel shows Xiox's evidence from the session histories first ("Created
  by Codex session …"). Without evidence it shows the project's agent terminals that were open
  when the item was first seen (90 s of slack, since a check comes after the folder is made).
  That says "could have", never "did".
- **What the person sees.**
  - The **Leftovers** side panel (`leftovers-panel.ts`), right of the workspace, shown by
    default and toggled from the toolbar button. The button carries the count of new items
    across projects.
  - Per project: open sessions, last check, the counts and the growth, then every item, newest
    first. Each item has its kind, size, path, age, reason, author, blocks, **Copy path** and
    **Show in Explorer** (`leftovers_reveal` accepts only a path that a current picture lists,
    and only selects it). **Go to** jumps to the terminal. **Check now** and **Copy clean
    command** act on the whole project. Archived agent sessions are only counted.
  - Projects → **Leftovers** line: the same summary.
  - The widget (in Italian, like its other notices) speaks as soon as a worktree, copy, named
    folder or session folder appears. When one terminal alone could have made it, the notice
    opens that terminal. Builds and logs are told when a session ends; each item is told once.
    Crossed thresholds are told too (growth over 2 GB, more than 3 new worktrees).

## Never

- Shinatama never runs `xiox clean` and has no path to it. The command is copied, and
  `xiox clean` shows its summary and asks before it removes anything.
- No harness version or Xiox version is checked or required. An unknown event or schema
  is ignored, not refused.

## Checks

- `cargo test -p shinatama-xiox`: event folding, program lookup, the child process (a stand-in
  script), and, with `XIOX_TEST_PROGRAM=<xiox>` (and `XIOX_TEST_MUKADE=<mukade>`), the real
  program on a scratch repository where a worktree and a `-bench` folder are created while it
  runs. Run it with `TEMP`/`TMP` outside any git repository: with the scratch folder inside
  another repository (an agent session's TEMP under another project), Xiox did not report the
  `-bench` folder and the test failed.
  2026-11-16: 9/9 against the `v0.1.0` release with Shinatama's `mukade.exe`.
- `node --experimental-strip-types --test apps/desktop/tests/leftovers-model.test.ts`: the
  texts, ordering, authorship and folder matching of the panel and the overview.
- Not verified yet on Windows: the desktop app with a real session (start, the one-minute
  checks, the widget notice that opens the terminal, Show in Explorer).
