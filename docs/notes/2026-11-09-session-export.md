# C26 session export and CLI (2026-11-09)

Windows 11 x64, HEAD `1e5e080`. Proofs run with `TEMP`/`TMP` set to `C:\shinatama-tmp`
(outside any Git repository; inside one the storage check refuses with "La cartella
appartiene a un progetto Git").

## What exists

- `crates/export` (`shinatama-export`): canonical export items with provenance and no
  reasoning variant; linear secret redaction; `.stx.jsonl` v1 read/write; the four format
  declarations of FRD-0006 R31; Markdown rendering; atomic output (`8abfa5f`).
- Session worker export: the contained helper streams the pinned session through Mukade
  (`Reader::read_streamed`, one entry at a time), redacts and renders each item as it arrives
  (STX or Markdown) and writes the body to its pipe in frames of at most 1 MiB
  (`crates/engine/src/export_frames.rs`: `tag | u32 length | payload`), then one final frame:
  the report, the bytes that go in front of the body, the attachments, or the refusal. There
  is no total bound on a session or on the output (ADR-0053 v2); the parent holds one frame
  at a time. The final frame is validated against the pinned fingerprint, format, mode,
  attachment policy and the number of body bytes that arrived. Every other worker reply stays
  one bounded JSON document. The worker never writes a file.
- Why the header comes last: Markdown's title and facts and STX's header line (item count,
  body checksum, redaction counts) are known only after the last item. The parent stages the
  body in a file beside the destination (`output::stage`), and on the final frame writes
  header + body into a second temporary file and renames it into place (`Staged::commit`),
  with the existing overwrite and attachment rules. Peak disk use is twice the output while
  committing. A failure, a timeout or a cancel drops the stage and removes its file; nothing
  is replaced. The in-memory render (`render_at`) uses the same item writers, so a streamed
  file is the in-memory render byte for byte.
- The `mukade` format streams too: Mukade's `wire::DocumentWriter` writes the interchange
  document one message at a time (compact JSON), opening with the meta that
  `read_streamed_with_meta` hands over first; each message loses reasoning and opaque fields and
  is redacted as it is written. No file export has a content bound. The hop preview keeps an in-memory export bound of 24 MiB: it
  holds the whole transcript in memory by design.
- Attachment bounds (8 MiB each, 12 MiB in all) bound one attachment, not the session: the
  final frame carries them base64-encoded (at most 24 MiB), never the session's text.
- Wall time is the size-scaled wall of search and export (10 s plus 1 s per 32 MiB of the
  session file, at most 120 s); frames received do not extend it.
- Desktop: **Export…** in session detail opens the declarations, then the native Save dialog
  first (the staged file lives beside the destination, so the destination is known before the
  worker starts; the suggested name is the session's id); the dialog's overwrite prompt is the
  consent to replace. A report lists items, redaction counts, omissions, attachments, losses
  and the SHA-256 of the written file (`c2a65a4`). The CLI without `-o` stages the body in the
  system temp folder, then copies header and body to stdout.
- CLI: `sessions`, `find`, `export`, `forget` with stable JSON and R37 exit codes; `resume`
  and `hop` refused as desktop-only; no `import` (`1e5e080`).

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p shinatama-export` | 21 passed |
| `cargo test -p shinatama-engine --features session-reader --lib session_` | 12 passed |
| `cargo test -p shinatama-engine --features session-reader --test session_export -- --ignored` | passed (2): real contained helper over framed pipe; GitHub token redacted; thinking absent; source bytes unchanged; reservation released; a 27 MiB Markdown output past the old 24 MiB bound, staged and committed, no staged file left |
| `cargo test -p shinatama-engine --features session-reader --lib -- session_export export_frames` | passed: Markdown and STX of a session past 64 MiB (66 MiB of output) streamed through frames, staged and committed, byte-identical to the in-memory render; a broken pipe and a refusing final frame leave no staged file and keep the old output |
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
