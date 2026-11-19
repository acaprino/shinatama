# Tauri terminal prototype — Windows evidence

Date: 2026-11-02. Scope: partial TL1/TL2/TL3/TL5, P1/P2/P5/P9/P14 evidence,
not closure of any of those gates. The user authorized the Tauri experiment;
see [scope and dependency decision](../implementation/tauri-prototype.md).

Environment: Windows NT 10.0.26200.0, x86_64-pc-windows-msvc; rustc 1.99.1;
Node 24.14.1; npm 11.11.0; Tauri 2.12.5, portable-pty 0.9.0, xterm 6.0.0,
Playwright 1.64.1. No agent, provider call or real conversation was used.

## Executed

| Command / record | Result |
| --- | --- |
| `npm run build` | Type check and frontend bundle pass |
| `npm test` | 3 tests pass: malformed/versioned layout recovery, resource fields stripped, split bounds |
| `cargo test -p shinatama-terminal` | 3 tests pass: size limits, missing folder without launch, unknown-session rejection |
| `cargo run -p shinatama-terminal --example smoke` | Real PowerShell/ConPTY starts; cursor query answered; synthetic output received; resize, normal exit and cleanup pass |
| `cargo build -p shinatama-desktop --features custom-protocol` | Embedded Windows debug executable builds |
| `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --features custom-protocol -- -D warnings` | Formatting and static checks pass |
| `npm run test:desktop` | Real WebView2 + Rust + ConPTY integration passes, no frontend exceptions |

The desktop script verifies no initial sessions, keyboard input and real shell
output, process IDs unchanged across move/hide/restore/resize, maximization, layout
direction, profiles, two simultaneous shells, invalid-path form validation,
stop/cancel confirmation, output flooding in a hidden terminal while the other
responds, normal exit, and window-close/cancel/confirm behavior. “IDs” here are
Shinatama's stable session IDs, not operating-system PIDs.

The final desktop run also passed with the Vite server stopped and asserted the
embedded `tauri.localhost` origin. The executable runs independently of Node/Vite.

Reproducible scripts: `crates/terminal/examples/smoke.rs` and
`apps/desktop/tests/desktop-smoke.mjs`. Generated screenshots are local, ignored
artifacts under `.prototype-evidence/desktop-terminals.png` and
`.prototype-evidence/browser-empty.png`.

## Findings incorporated

ConPTY requests a cursor-position report before shell startup. A headless probe
must answer it; the real emulator must send protocol replies even from hidden
views. User input remains controlled by DOM focus/modal focus.

The Windows console keeps its output pipe open after shell exit. Close the console
off the UI thread after observing process exit while continuing to drain its tail.
Drop the output receiver before the master on stop, so a full bounded output queue
cannot deadlock console destruction. Disable PSReadLine history persistence for
the prototype shell; use ordinary canonical Windows paths rather than verbatim
paths in PowerShell's prompt.

The initial integration script passed its assertions but reported a closed-page
error when awaiting a close command after the WebView disappeared. The corrected
test waits for the close event and additionally checks confirmation/cancellation;
its complete run exits successfully.

## Borderless frame follow-up — 2026-11-02

At the user's request, native decorations are disabled. The themed header supplies
minimize, maximize/restore and close controls, plus a native drag region. The updated
desktop test passes against the embedded executable: it measures the client-area
inset (no native title strip), checks actual native minimization and restoration,
checks maximization through Tauri and the changing button label, double-clicks the
titlebar, and closes through the custom button with live-terminal cancel/confirm.
The existing two-shell and output-flood assertions also pass with no frontend errors.

The test-only Windows probe follows the original HWND throughout minimization:
`MainWindowHandle` can otherwise select a helper window. Windows style bits alone
do not prove visible decorations because Tao retains some bits for OS behavior.
Manual drag/snap and macOS/Linux frame behavior are not certified by this run.

## Still open

No real agent TUI, adapter, sidecar or box-home isolation was exercised. Nested
split trees, SQLite ownership/persistence, restored screen state, crash-at-spawn,
complete descendant cleanup/crash tests, adversarial escapes, IME, high DPI,
resource measurements, macOS/Linux and the combined Markdown/file TL1 prototype
are not proved. All full gates remain open; this record proves only this slice.
