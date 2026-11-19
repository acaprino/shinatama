# shn-render — LaTeX and Mermaid as text in Claude Code

Date: 2026-11-11. Status: partial — Linux only; Windows not run.

## What was asked

The user asked for rendered math and diagrams in the harnesses, on condition that nothing
that works today changes ("deve funzionare senza portare la minima regressione"). Codex
0.167.1, OpenCode 2.2.7 and Pi 1.5.2 already draw tables, math and Mermaid as text in an
xterm.js pane; Claude Code 2.1.322 shows them as source. The image-based plugin
cc-intelligent-ui draws nothing in xterm.js (it needs the kitty graphics protocol), so a
display-only Claude Code mod drawing Unicode text was built instead: `resources/claude-mods/shn-render`.

## Isolation from what exists

- Own folder and lock (`resources/claude-mods/mods.lock.json`), verified by
  `crates/engine/src/claude_mods.rs`. A missing, altered or extra file drops the mod alone;
  the bundled plugins keep their own verification, unchanged.
- Only the Claude launcher adds it (`claude_launch.rs`, after the bundled plugins, only when
  the bundled set is selected). Codex, Pi and OpenCode launch code and the bundled set
  (`bundled_set()`, `PLUGINS`, `plugins.lock.json`) are untouched.
- Only on Claude Code versions it was run with (`TRIED_CLAUDE_VERSIONS`, today 2.1.322):
  Claude's mod interface is early access. Off: `SHINATAMA_CLAUDE_MODS=off`.
- Not in the portable package: `scripts/package-local.ps1` is untouched.

## Results on Linux (Claude Code 2.1.322, a pane built with the desktop's xterm.js options)

| Check | Result |
|---|---|
| 32 Node tests of the rewrite (`resources/claude-mods/tests`), one per case found in four review rounds | pass |
| `claude_mods.rs` compiled in a scratch crate with the workspace's toolchain: 4 tests, clippy `-D warnings` | pass |
| `cargo fmt --all --check`: findings before and after the change | identical (the change adds none; master already has findings elsewhere) |
| Claude with Ponytail + i-have-adhd, with and without the mod: assistant text in the stored session | byte-identical |
| Same runs: screen text | differs only in the converted formulas and the drawn diagram |
| Answer with code in lists, `$$` as PID, `\(` in prose, prices, unsafe diagrams | only the formula line differs |

## Open

- Not compiled or run on Windows: `claude_launch.rs` is `cfg(windows)` and the engine does
  not build on Linux. `npm run verify` and one Claude session in the desktop app remain.
- Visible changes by design: copying a formula from the pane copies its Unicode form, and
  the pane's text search finds the Unicode form (`ctrl+o` shows the original).
- Diagrams only for the shapes the text layout draws unambiguously; everything else stays code.
