# Unified model settings

User request: one panel and one shared store for provider connections, API keys and models, reused by OpenCode, Pi and Hermes. Shared configuration confirmed on 2026-11-16.

## Design

Add Models to the existing Settings drawer. A connection has a stable id, a name, HTTP(S) base URL, an API protocol, an encrypted optional API key, and model ids with explicit context/output limits and reasoning/image capabilities. Each harness selects a connection and model, or keeps its native configuration. Provider presets fill the endpoint and protocol, never restrict model ids or harness versions.

One `model-settings.json` in Shinatama's data home is the canonical store. Windows DPAPI protects keys; load responses contain only key presence. Keep/set/clear edits preserve keys without returning them. Validate before an atomic save; unreadable data must never be silently replaced.

OpenCode receives a generated provider and model through its existing inline configuration. Pi receives a private per-launch provider extension and explicit provider/model arguments. Keys reach only the selected harness via a launcher-owned environment variable; generated files and arguments contain no key. Both use a snapshot at launch. Qualification trials remain credential-free. Native config and session roots stay untouched.

Hermes has no launch adapter in this repository. Store its selection, with an explicit UI message that it is saved only and cannot yet apply to a running Hermes session. Implementing a Hermes launcher is outside this change.

Considered writing each harness's global config: rejected because it duplicates secrets and overwrites independent settings. A shared connection store with per-launch translation fits the existing private-home launchers.

## Plan and progress

- [x] Shared schema, validation, encrypted store, local-only commands and persistence tests.
- [x] OpenCode/Pi launch translation, secret isolation and configuration tests.
- [x] Models panel, presets, model editor, assignments and form tests.
- [x] Build, focused tests, native UI smoke and independent review.

## Constraints and verification

No new dependencies; no harness version allow-list; no copies of installed executables. Test key keep/replace/clear, corrupted store preservation, invalid/dangling selections, duplicate ids, hostile endpoint/input values, generated config escaping, native fallback and trial isolation. Native UI smoke uses a disposable Shinatama data home and synthetic keys; no paid provider requests.

Implementation takes place on `feat/unified-model-settings` from a clean checkout. No deployment requested.

## Verification, 2026-11-16

- `npm run build`: TypeScript and Vite passed; existing large-chunk warning remains.
- `npm test`: 126 passed.
- `cargo test --locked -p shinatama-engine --lib model_config -j 2`: 5 passed.
- `cargo test --locked -p shinatama-desktop model_settings --bin shinatama-desktop -j 2`: 3 passed.
- `cargo test --locked -p shinatama-engine --lib launch -j 2`: 69 passed, 1 pre-existing explicit native bridge proof ignored.
- `npm run test:models`: real desktop/WebView, disposable `SHINATAMA_HOME`; shared assignments, encrypted file/redacted IPC, reopening, URL validation, partial-save feedback, explicit native fallback and key cleanup passed. Screenshot: `target/model-settings-panel.png` (build artifact, not committed).
- `npm run test:model-providers`: generated translations from the engine exercised against the installed Pi and OpenCode and a loopback HTTP provider. Both selected an arbitrary model id containing `/` and `:`, delivered the exact synthetic key containing `!`, `$` and `${HOME}`, and completed a response. No external model calls. Pi's stale native credential was overridden while its `auth.json` remained unchanged.
- `cargo build --locked -p shinatama-desktop --features custom-protocol -j 2`: local desktop executable. No installed executable copied or backed up.

The smoke scripts accept `SHINATAMA_SMOKE_TEMP` (UI fixture directory), `SHINATAMA_PI_SMOKE_CLI` (installed JavaScript entry) and optional `SHINATAMA_OPENCODE_SMOKE_EXE` (installed executable). The UI fixture must be outside a Git project. This machine's inherited TEMP was inside another repository; C: also lacked the existing database migration's 1 GiB reserve, so the successful UI run used `D:/Temp`.

## Review decisions and compatibility

The independent review found and verified fixes for Pi interpreting literal keys as commands/templates, Anthropic URL differences and partial-save feedback. Anthropic connection URLs accept a host/prefix or terminal `/v1`; translation strips that suffix for Pi's SDK and appends it for OpenCode's SDK.

Pi uses a fixed environment reference plus an environment alias for earlier whole-string resolvers. On runtimes exposing native Providers, a public `session_start` provider override makes shared authentication authoritative without rewriting native credentials. Runtimes exposing the earlier runtime-key API use that instead. If neither capability exists, launching remains allowed and Pi explains that a native `/login` may take precedence and `/logout` restores shared authentication. No version allow-list was introduced.

Hermes selection is stored only, clearly marked in the panel, because Shinatama still has no Hermes launcher. OAuth login and model catalog discovery remain native harness functions; this panel manages manually entered API connections and model metadata.

Protocol references: [Pi custom providers](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/custom-provider.md), [Pi extensions](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/extensions.md), [OpenCode providers](https://opencode.ai/docs/providers/). Installed package source and loopback tests were used to resolve differences from current online documentation.
