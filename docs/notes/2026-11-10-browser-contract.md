# F17 browser request contract (2026-11-10)

`crates/browser`: typed, frozen requests; routing before the target loads; identity from the
authenticated caller; Moli default; Patchright only on an explicit user request with the
setting on; agents confined to user-granted scopes; failures never switch engine; a text-only
result never satisfies a preview. `cargo test -p shinatama-browser`: 3 passed.

Open: Moli (F18) and Patchright (F19) execution, the session view (R14), MCP web interfaces
(R15, B3).
