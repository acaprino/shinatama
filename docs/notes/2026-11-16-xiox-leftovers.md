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
  `build-release.bat` builds Xiox from `..\xiox` when that folder exists and copies
  `xiox.exe` into `release\xiox\`, with Xiox's own `mukade.exe` beside it. Without Xiox,
  Shinatama works as before and Projects says why nothing is observed.
- **When it starts.** `terminal_read` already sees every batch of every terminal (as
  `attention` does). `apps/desktop/src-tauri/src/leftovers.rs` uses that: the first batch of a
  new agent terminal (Claude Code, Codex, OpenCode, Pi; not PowerShell) starts one observer for
  its folder, and the end of that terminal asks the observer for a fresh `report`. The
  harness launches are unchanged.
- **One observer per project.** Xiox resolves a worktree to the main working tree. When a
  second observer reports the same root as one that is already running, it is stopped and its
  folder is added to the first one.
- **How long.** Observers run until Shinatama exits: what a session left is still listed after
  the session ended. Each one checks every 120 seconds (Xiox's minimum is 60) at below-normal
  priority with no console window. It also ends when its stdin closes, so it cannot outlive
  Shinatama. There are at most 16 projects.
- **What is kept.** `shinatama_xiox::Picture` keeps only the items with `new: true` (the first
  snapshot is the project as the person left it), at most 200, plus growth, thresholds, the
  last error and the `cleanCommand`. A `report` replaces what the events said between reports.
- **What the person sees.** Projects → the **Leftovers** line of each project: how many new
  items by category, the growth, **Details** with each path, size, reason and block, and **Copy
  clean command**. The page updates on the `project-leftovers` event. The widget (in Italian,
  like its other notices) says when a session that ended left something new, and when Xiox
  reports a crossed threshold (growth over 2 GB, more than 3 new worktrees).

## Never

- Shinatama never runs `xiox clean` and has no path to it. The command is copied, and
  `xiox clean` shows its summary and asks before it removes anything.
- No harness version or Xiox version is checked or required. An unknown event or schema
  is ignored, not refused.

## Checks

- `cargo test -p shinatama-xiox`: event folding, program lookup, the child process (a stand-in
  script), and, with `XIOX_TEST_PROGRAM=<xiox>`, the real program on a scratch repository
  where a worktree and a `-bench` folder are created while it runs.
- `node --experimental-strip-types --test apps/desktop/tests/leftovers-model.test.ts`: the
  texts and folder matching of the overview.
- Not verified yet on Windows: the desktop app with a real session (start, end, widget notice).
