# C26 session export and CLI (2026-11-09)

Windows 11 x64, HEAD `1e5e080`. Proofs run with `TEMP`/`TMP` set to `C:\shinatama-tmp`
(outside any Git repository; inside one the storage check refuses with "La cartella
appartiene a un progetto Git").

## What exists

- `crates/export` (`shinatama-export`): canonical export items with provenance and no
  reasoning variant; linear secret redaction; `.stx.jsonl` v1 read/write; the four format
  declarations of FRD-0006 R31; Markdown rendering; atomic output (`8abfa5f`).
- Session worker export: the contained helper reads the pinned session through the detail
  path, redacts once and renders STX, portable Mukade JSON or Markdown. The trusted parent
  writes. Export replies have their own 64 MiB bound and are validated against the pinned
  fingerprint, format, mode and attachment policy (`c2a65a4`).
- Desktop: **Export…** in session detail opens the declarations, then the native Save dialog;
  the dialog's overwrite prompt is the consent to replace. A report lists items, redaction
  counts, omissions, attachments, losses and the SHA-256 of the written file (`c2a65a4`).
- CLI: `sessions`, `find`, `export`, `forget` with stable JSON and R37 exit codes; `resume`
  and `hop` refused as desktop-only; no `import` (`1e5e080`).

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-export` | 21 passed |
| `cargo test -p shinatama-engine --features session-reader --lib session_` | 12 passed |
| `cargo test -p shinatama-engine --features session-reader --test session_export -- --ignored` | passed: real contained helper; GitHub token redacted; thinking absent; source bytes unchanged; reservation released |
| `cargo test -p shinatama-cli` | 9 unit + 2 native tests passed |
| `cargo test -p shinatama-cli --test sessions -- --ignored` | passed: first run exit 3 with nothing created; list; portable STX export; refusal on existing file (exit 2, file unchanged); Markdown to stdout; native, resume and missing session exit 2; source unchanged |
| `cargo build -p shinatama-desktop`, `npm run build` | passed |
| `shinatama sessions --json` on the real profile | exit 0, 0 sessions (the Claude box home has no `projects/` yet) |

## Limits

- Native export is offered nowhere: no agent's own export is proven (R39); the UI and CLI say so.
- Attachments are copied only when the record carries inline data (base64 images). Path-only
  attachments stay hashed references; files outside the archive are never opened.
- OpenCode detail/export stays refused (`unsupported_export_context`): its pinned reader
  launches the agent's exporter, which is a separately qualified path.
- The CLI admits its worker on the shared reservation ledger but does not sample host headroom
  like the desktop governor; the worker's job-object limits still apply.
- Not run: an app-level smoke clicking Export (the native Save dialog is not automated);
  the STX round-trip law per adapter (R34) needs an import path, which no version has.
