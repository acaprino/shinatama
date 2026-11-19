# F09 companion card protocol (2026-11-10)

Windows 11 x64. `crates/companion` at `682cecf`.

## Decisions

- Renderer for Tauri: cards render through Shinatama's own Rust Markdown renderer
  (`shinatama-files::markdown`) into a fixed tag subset with every character escaped. The
  WebView shows that HTML; no script, remote request or image is produced. This replaces the
  legacy "native, no JavaScript runtime" wording: there is a WebView, but nothing executable
  reaches it. `mermaid` blocks stay visible source; no diagram renderer is claimed.
- Normalization: a compatibility fold (fullwidth ASCII to ASCII, Unicode spaces to one space,
  trim, collapse) instead of full NFKC, because a normalization-table crate is not in the
  locked graph. Other compatibility characters are not folded; the allowed-character rule
  rejects most of them.

## Checks run

`cargo test -p shinatama-companion`: 13 passed. Hostile operand cannot close its wrapper or
open a tag; credentials redacted and the last message keeps its end; learner context stays
valid JSON with escaped strings; R34 levels; IT/EN labels with English fallback; valid block
parsed and removed; invalid items dropped individually while prose survives; malformed,
oversized, misplaced and duplicate blocks; requested kind wins and Aided Decision teaches
nothing; hostile card prose renders inert.

## Limits

- No companion pane yet (F10): the protocol is a library with tests, not visible in the app.
- Full NFKC not applied (see above).
