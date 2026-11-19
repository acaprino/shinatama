# TL1 isolated shell proof

This fixture is compiled into a separate proof build using the `tl1-proof` Cargo
feature and a Tauri configuration override. Its frontend assets and read-only
file commands are absent from the normal application build. The synthetic file
workspace is created under ignored `.prototype-evidence/tl1/`. Its private data
home is a newly allocated system temporary directory outside the repository,
so it passes the same C01 location policy as the normal application; it is
removed after the proof process and its handles have closed.

From the repository root:

1. `npm run build:tl1`
2. `npm run desktop:tl1`
3. `node apps/desktop/tests/tl1/proof.mjs`
4. Restore normal embedded frontend assets with `npm run build`.
5. Restore the normal debug binary with `cargo build -p shinatama-desktop --features custom-protocol` before normal smoke tests.

For the qualified local run, the harness was launched through the existing P14
test job wrapper: `verify/P14/target/debug/p14-probe.exe wrap-run node apps/desktop/tests/tl1/proof.mjs`.
It confines the proof application's process family for test cleanup. This is a
test fixture and does not qualify production P14 containment.

The harness types the installed native Claude Code executable into the normal
PowerShell ConPTY. It opens the native interactive TUI and runs the local `/help`
command, resizes nested panes while the same session is live, exits the CLI and
checks that the shell still responds. The harness sends no model prompt, reads
no account or credential files, and fails if interactive login is required. The CLI
uses only project settings, has hooks disabled, and receives an explicit empty
MCP configuration; it is started without any agent adapter.

Both Cargo features are explicit: this workspace inherits its Tauri dependency,
so the CLI cannot infer and enable `custom-protocol` from a local dependency
declaration. Without it, a debug build would try a development URL and would not
qualify the embedded-assets requirement.

Node/Playwright are external test drivers. The application starts from embedded
assets and uses its installed WebView2 runtime. The debug endpoint is supplied
only in the test process environment. A process-name check verifies that the
application has no Node, development-server or external-browser child.

Only sanitized booleans/runtime information and a post-agent screenshot are
written to evidence. Native account presentation and raw agent output are not
saved. The screenshot is taken after clearing the shell screen.

The minimal Markdown renderer intentionally displays all raw HTML and remote
image/link syntax as text. Full Markdown/Mermaid fidelity and Windows without
WebView2 are recorded gaps, not claimed passes of later gates.

## Qualification of failed attempts

An initial harness attempt misclassified Tauri's `ipc.localhost` transport as
an external request; that driver assertion was corrected. A second attempt
passed native Claude startup, local help and live nested resizing but failed
to exit Claude with two Ctrl+C keys. That attempt sent a synthetic PowerShell
marker before confirming the agent process had exited. The marker could have
been interpreted as a model prompt; the absence of a model submission cannot
be established for that failed attempt. Its sanitized evidence is retained as
`failed-unverified-agent-exit.json` under ignored proof output.

The corrected harness uses the local `/exit` command and hard-fails unless the
Claude descendant process is absent **before** sending any PowerShell marker.
`modelPromptSubmitted: false` in a successful result applies to that successful
run only. It does not qualify the earlier failed attempt.
