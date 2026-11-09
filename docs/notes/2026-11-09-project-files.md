# C41/C42 project files, buffers and Markdown (2026-11-09)

Windows 11 x64. Proofs run with `TEMP`/`TMP` at `C:\shinatama-tmp`.

## What exists

- `crates/files` (`2361934`): navigator (root containment incl. links/junctions, bounded and
  cancellable list/search, read for display), shared buffers with undo/redo and dirty close,
  version-checked saves with temp-file swap, inert Markdown renderer.
- Desktop **Files** panel (`d016dc2`): project chooser limited to recorded projects, tree,
  name search, source / preview / split for Markdown, Save with a conflict dialog.

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-files` | 11 passed |
| `node apps/desktop/tests/files-smoke.mjs` | PASS, 6 checks (see commit `d016dc2`) |

## Limits

- Files open in a panel dialog, not as workspace tabs; the layout never stores file text.
- The frontend keeps one buffer per path while the panel is open; the Rust `Buffers` type
  (shared views, undo/redo) is tested but the panel uses the textarea's own undo.
- Mermaid stays a code block; no diagram renderer exists.
- Startup flake: twice the first app start after a rebuild exited before the WebView page
  loaded; the immediate rerun passed. Not diagnosed yet.
