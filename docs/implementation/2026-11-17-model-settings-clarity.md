# Models panel: say which agent uses what

User report, 2026-11-17: a connection to `llm.janura.com` with the model `janura-qwen` was saved, yet OpenCode did not list the model. Its assignment was still "Native settings": OpenCode only receives the connection assigned to it (`Snapshot::opencode`, `crates/engine/src/model_config.rs`). The user found nothing in the panel that said so ("non si capisce da UI come fare") and asked for a clearer, cleaner panel, guided by the UI/UX Pro Max rules.

## What was wrong

- The choice that decides a new session, "Use for new sessions", sat at the bottom, after the add-connection row and a long hint.
- A saved connection never said whether any agent used it.
- "Native settings" did not say it means the agent's own sign-in and model.
- Two menus per agent (connection, then model) for one decision.
- The custom preset named every connection "Custom endpoint".

## Approaches

- A. Reorder and show state (chosen): the model per agent first, one menu each; every connection card says who uses it, and an unused one offers "Use for OpenCode" / "Use for Pi".
- B. Assign from inside each model row, the bottom block reduced to a summary: one place to act, but which model each agent starts with is no longer visible at a glance, and exclusive choices across cards are hard to follow.
- C. A step-by-step wizard: clearest the first time, heavy for later edits and more code.

The user asked for the UI/UX Pro Max rules applied without choosing among them; A was the recommendation and was built.

## Design

UI/UX Pro Max rules applied: `primary-action`, `visual-hierarchy`, `progressive-disclosure`, `empty-states`, `input-helper-text`, `error-clarity`, `color-not-decorative-only`, `consistency`.

- **Model for new sessions** leads the section: one row and one menu per agent. The first option is "<Agent>'s own sign-in and model"; every other option is `model · connection`, so the closed menu still says where the model comes from. A removed model stays selected as "Removed model · choose again", marked invalid with an error line, until the user chooses again (unchanged rule: deleting never silently switches the next model). Hermes keeps its "Saved only" line.
- **Connections**: each card shows name, host, model count and key state, plus a status in words: "Used by OpenCode" or "Not used" (also with a warning edge, never colour alone). An unused card says so and offers one-click assignment for the agents Shinatama can launch (OpenCode, Pi), picking the connection's first model. The form opens with Edit (`aria-expanded`); new connections open, saved ones start closed.
- A new custom connection takes its name from the endpoint (`llm.janura.com` → "Janura") until the user types a name. Saved names are never changed.
- **New connection** row: labelled and set apart, below the cards.
- Same storage, IPC, validation and launch translation. Only the panel, its stylesheet, small pure helpers in `model-settings-model.ts` and the UI smoke changed.

## Verification, 2026-11-17

- `npx tsc --noEmit`: passed.
- `npm test`: 139 passed, 4 new (`usedBy`, `firstModelId`, `suggestedName`/`connectionHost`, menu value round trip).
- `npm run build`: passed; the existing large-chunk warning remains.
- `cargo build --locked -p shinatama-desktop -p shinatama-cli -p shinatama-terminal --bins --features custom-protocol -j 2`: passed. A first unrestricted-parallelism attempt crashed rustc on `brotli` (`STATUS_STACK_BUFFER_OVERRUN`); `-j 2` built.
- `npm run test:models` with `SHINATAMA_SMOKE_TEMP=D:/Temp`: passed on the real desktop/WebView. It now also covers the "Not used" status, one-click "Use for OpenCode", "Used by OpenCode, Pi and Hermes", saved cards starting closed behind Edit, and the invalid marker on a removed model.
- Panel screenshots at 760 px and 560 px (unused, assigned, open, new custom connection, empty) were reviewed in an isolated preview with synthetic data. They were not committed.
