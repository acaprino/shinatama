# TE5 — OpenCode 2.2.5 closes a TokenEconomizer turn

Date: 2026-11-01 · Status: **passed** (one trial per case) · Owner row: register TE5 ·
Source: `docs/notes/2026-11-01-tokeneconomizer-beside-billioncontext.md` section 9; review
finding H3 (`.daodan/runs/review-te-bc-2026-11-01/review-te-bc-2026-11-01.md`).

## Question

TokenEconomizer's OpenCode plugin (2.x path, `setupV2`) closes a turn on
`session.execution.(succeeded|failed|interrupted)` (`plugin/opencode/tokeneconomizer.ts:481-489`).
If OpenCode 2.2.5 did not emit those events, `Stop` would never fire, no envelope would form and
the loop guard would be silently inert (review H3). Does the event fire on 2.2.5?

## Setup

- OpenCode 2.2.5 (`opencode --version`), Windows 11, TokenEconomizer commit `c53a4a6` at
  `D:\tmp-te\te`, Python 3.13.14 as `TE_PYTHON=python`.
- Isolated from the user's configuration: `OPENCODE_CONFIG_DIR=D:\tmp-te\oc-test\config2`
  holding an `opencode.json` with the user's provider entry (one model,
  `tri-llm-gateway/gemini-3.5-flash-lite`) and no plugin list; the plugin loaded from
  `config2/plugins/tokeneconomizer.ts`, a one-line re-export of
  `D:/tmp-te/te/plugin/opencode/tokeneconomizer.ts` (so the plugin keeps its sibling `scripts/`
  and `lib/`); `TOKENECONOMIZER_HOME=D:\tmp-te\oc-test\te-home`; cwd a throwaway git
  repository `D:\tmp-te\oc-test\repo` with one commit.
- Command: `opencode run --standalone --format json -m tri-llm-gateway/gemini-3.5-flash-lite "<prompt>"`.

## Runs

| Run | Prompt | Exit | Tool calls | TE turn record |
| --- | --- | --- | --- | --- |
| control (no plugin, `config-noplugin`) | "Reply with the single word ok" | 0 | 0 | none (no plugin) |
| 1 (plugin through the `plugins` array of `opencode.json`, `file:///D:/tmp-te/te/plugin/opencode/tokeneconomizer.ts`) | same | **124** (timeout 150 s, no output; the standalone server logged "cli starting" and nothing more) | — | none |
| 2 (plugin from `config2/plugins/`) | same | 0 | 0 | `turns: [{tool_calls: 0, resolved_proxy: false}]` |
| 3 (same as 2) | "Use the bash tool to run exactly: git status --short. Then reply with the single word ok." | 0 | 1 (`shell`) | `turns: [{tool_calls: 1, resolved_proxy: true, verifications: 0}]`; `envelope.json` gains `resolved_turns: [1]` |

Records read: `<TOKENECONOMIZER_HOME>/repos/repo-5c9c9367b1/opencode/sessions/<id>.json`,
`envelope.json`, `events.jsonl`; `errors.log` absent.

## Reading

- `resolved_proxy` is true only when the turn was closed with `stopped_normally=True`
  (`loopguard/turn.py:262`), which happens on `Stop` (`plugin/controller.py:273`); `SessionEnd`
  and a later prompt close with `False` (`controller.py:264,307,369`). Run 3 shows `True`, so
  `Stop` fired: **OpenCode 2.2.5 emits `session.execution.succeeded`** and the plugin closes the
  turn on it. Review H3's trigger does not occur on this version.
- Run 2's `False` is not a failure of the event: a turn with zero tool calls is never green
  (`turn.py:260`, `cum_turns == 0`). Such turns do not feed the envelope.
- Run 1: loading the same file through the `plugins` array with a `file:///` URL stalled the
  standalone server before it served the request (one trial, not diagnosed). The route TE
  documents, a file under `<config dir>/plugins/`, works. ADR-0045 rule 1 names the `plugin`
  array route for OpenCode; until the stall is understood, Shinatama uses the `plugins/`
  directory of the instance's `OPENCODE_CONFIG_DIR` with a re-export wrapper, as here.

## Correction (TE13, 2026-11-02)

Run 1 was not stalled by the `plugins` array. `opencode run` hangs on an inherited stdin, with the
same log ending at `cli starting`; with stdin closed the same array exits 0 three times out of three
and skips the file (`configured plugin path must be a directory`). The `plugins/` directory route
remains the one that loads the plugin, for that reason and not for a stall. "Isolated" above covers
configuration and TokenEconomizer's state only: these runs wrote to the user's OpenCode database.

## Not covered

One model, one provider, headless `run` only (the TUI path was not exercised); `failed` and
`interrupted` variants not provoked; the hang of run 1 not reproduced a second time.
