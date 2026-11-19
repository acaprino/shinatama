# TE12 — Tool pruning evidence on OpenCode

Date: 2026-11-01 · Status: **failed as designed; cause settled** · Owner row: register TE12 ·
Source: `docs/notes/2026-11-01-tokeneconomizer-beside-billioncontext.md` section 7.1;
ADR-0045 rule 4.

## Question

The study assumed that on OpenCode TokenEconomizer's pruning **evidence** accrues (used tools,
unused streaks) and that only the catalog and the writer are missing, so that Shinatama could
supply a catalog file and apply the proposal as `permissions` deny rules. Does the evidence form
once a catalog is present?

## What was found before writing any catalog

Same isolated setup as TE5 (`OPENCODE_CONFIG_DIR=D:\tmp-te\oc-test\config2`,
`TOKENECONOMIZER_HOME=D:\tmp-te\oc-test\te-home`, TokenEconomizer `c53a4a6`, OpenCode 2.2.5),
after the two TE5 sessions, one of which made a `shell` tool call:

- `repos/repo-5c9c9367b1/opencode/` holds `envelope.json`, `events.jsonl`, `sessions/`,
  `migrate.log` and **no `untool.json`**.
- `te --host opencode toolprune` (run through `plugin/scripts/te_bootstrap.py cli`, no model
  call) answers: `tool pruning is not available on opencode (it declares only a handful of
  tools)`; `te --host opencode status` reports `toolprune not available on this host`.
- Cause in the source: `_fold_toolprune` returns before any evidence is read or written when
  `denylist.supports(host)` is false (`tokeneconomizer/plugin/fold.py:115-116`), and
  `supports` is membership in `_SETTINGS = {"claude", "codex", "copilot"}`
  (`tokeneconomizer/plugin/denylist.py:55,76-77`). OpenCode is not in it, so the stage never
  runs: no `used_tools`, no streaks, no proposal, whatever catalog file exists under
  `hosts/opencode/catalog.json`.

No catalog was written and no further session was run: the gate sits before the catalog is
consulted, so the planned trial could not change the outcome.

## Consequence

The study's section 7.1 and ADR-0045 rule 4 rest on a false premise. On OpenCode
TokenEconomizer contributes **nothing** to tool pruning today, not even evidence. Two ways to
get it, recorded in the study (7.1) for the user's decision:

1. **Upstream request**: add `opencode` to `denylist._SETTINGS` with a propose-only entry (no
   file written) and read the catalog from `hosts/opencode/catalog.json`; TokenEconomizer then
   produces the proposal and Shinatama applies it as planned.
2. **Shinatama computes the evidence itself** from the per-session exports TokenEconomizer
   already writes (`<TOKENECONOMIZER_HOME>/opencode-sessions/<id>.json`: every `tool` part with
   its `tool` name and `state.status`), with the same rule as TokenEconomizer's
   `enough_evidence` (unused over 10 prompts across at least 2 sessions, or over 5 sessions;
   `fold.py:97-106`) and the same protected set, then writes the deny rules into the instance's
   `OPENCODE_CONFIG`. No dependency on upstream; one small rule duplicated.

## Not covered

Whether OpenCode 2.2.5 honours a `permissions` deny rule for an MCP tool by removing it from the
request (harness-tools.md records it for 2.1.4) was not re-run here.
