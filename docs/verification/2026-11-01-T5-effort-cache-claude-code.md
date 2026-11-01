# T5 — does changing the effort field lower prompt-cache reads? (Anthropic, via Claude Code)

Item: T5 (verification register, section 18). Date: 2026-11-01. Status: running (partial).
Machine: Windows 11 Home 10.0.26200. Claude Code 2.1.316. BillionContext sidecar 0.1.207
(Musashi Terminal build), listener 127.0.0.1:18794, MITM via `HTTPS_PROXY`. Account: the user's
logged-in Claude Code; no API key. Models as resolved by the aliases `sonnet`, `opus`, `haiku`,
`fable`: `claude-sonnet-5-5`, `claude-opus-5-5`, `claude-haiku-5-5`, `claude-fable-5-1`.

## What was asked

Before turning on the output-steering effort lever for any harness, verify per provider that
changing the effort field between two requests of one session does not cost the cached prefix
(user, 2026-11-01). The lever lowers `output_config.effort` (and `thinking.budget_tokens`) on
tool-result turns by editing the top-level request field.

## What the documentation said (read 2026-11-01)

`platform.claude.com/docs/en/build-with-claude/prompt-caching`, "What invalidates the cache":
changing `output_config.effort` "always invalidates message blocks", with a model-specific effect
on the tool and system caches. The same page and the effort page say a per-message effort (a
`role: "system"` message carrying `output_config`) keeps the cache on Fable 5.1, Opus 5.5, Opus 5,
Sonnet 5.5 and Haiku 5.5.

## Method

Scripts: `verify/T5/probe-controlled.py`, `verify/T5/probe-forwarder.py`,
`verify/T5/wire-flip-forwarder.mjs`. Raw output: `verify/T5/out/` (git-ignored).
One trial is one fresh session driven by `claude -p ... --resume`, minimal prefix
(`--strict-mcp-config --mcp-config {"mcpServers":{}} --disable-slash-commands --tools Read
--setting-sources project --exclude-dynamic-system-prompt-sections`, about 5K tokens of system
and tools) plus a first user message of about 15K tokens of filler, so the message cache is
large enough to see. Calls A, B warm the cache; C is the control (same effort, cache warm); D is
the flip. Read `cache_read_input_tokens` and `cache_creation_input_tokens` from the CLI JSON
`usage`. Paths: `direct` (no proxy), `bili` (through the sidecar), and a local forwarder
(`ANTHROPIC_BASE_URL`) that logs the request fields and, in `strip` mode, removes Claude Code's
own per-message effort entry and rewrites only the top-level `output_config.effort` from the
4th request on, which is what the sidecar's lever does.

Wire fact seen through the forwarder: Claude Code 2.1.316 sends effort twice, as the top-level
`output_config.effort` and as a `{"role":"system","output_config":{"effort":...}}` message at
index 1 of `messages`.

## Results (flip read / control read; the prefix is about 20.7K tokens)

| Case | Trials | Flip read vs control read | Verdict |
| --- | --- | --- | --- |
| Sonnet 5.5, `--effort high` to `low`, direct | 2 | 20716 vs 20702; 20744 vs 20730 | preserved |
| Sonnet 5.5, `--effort low` to `high`, direct | 2 | 20792 vs 20778; 20769 vs 20755 | preserved |
| Sonnet 5.5, same flips, through the sidecar | 2 | 20720 vs 20706; 20738 vs 20724 | preserved |
| Sonnet 5.5, top-level only changed, per-message entry kept | 2 | 20773, 20770 vs 20771 | preserved |
| Sonnet 5.5, top-level only changed, per-message entry removed | 3 | 20267, 20274, 20334 vs 20319, 20364 | preserved |
| Opus 5.5, top-level only changed, per-message removed | 1 | 20361, create 249 | preserved |
| Fable 5.1, top-level only changed, per-message removed | 1 | 22043, create 294 | preserved |
| **Haiku 5.5**, top-level only changed, per-message removed | 2 | **3081** read, **18309 and 18300** created | **message cache invalidated; tools and system kept** |

Full Claude Code configuration (no `--strict-mcp-config`, user plugins, hooks and skills on; about
100 tools, a 172K to 180K token prefix), run sequentially without the sidecar in the path, one
session per trial, flip at step D:

| Case | Control step (no flip) | Flip step | Verdict |
| --- | --- | --- | --- |
| `high` to `low` | C read 0, created 173410 (**a full miss with no flip**) | D read 172053, created 1562 | preserved (the cache had just been rewritten at C) |
| `low` to `high` | C read 171403, created 397 | D read 171608, created 397 | preserved |
| `high` only, no flip, 5 calls | C, D, E read 171246, 171451, 171656 | none | no miss |

Earlier full-configuration runs (parallel, with the sidecar or the forwarder in the path, 107K to
148K prefix) had several full misses of 0 cache read, but they also had two on steps with **no**
effort change (the sixth call of the sidecar run and the control step C of the sequential run
above). A miss with no flip means those zeros cannot be attributed to the effort change. Likely
contributors, not isolated: every `claude -p` call re-runs the session-start hooks of the full
configuration, so part of the prefix may differ between calls (the first two calls read only
82K to 108K of a 172K prefix); parallel trials sharing a prefix; the sidecar reporting "event
loop saturated" during overlapping runs. Some parallel runs died with exit code 3221226505
(0xC0000409) and were discarded.

## What was not shown

- Whether the top-level change has any effect on behavior. Measured only through the CLI's own
  `--effort` (which sets the top-level field and Claude Code's per-message entry), on a task of 25
  sequential modular steps, Sonnet 5.5, thinking tokens per call C, D, E: `high` 642, 257, 238
  (one trial); `low` 675, 177, 206 and 593, 174, 199 (two trials). That is a reduction of 8% to
  32% per call with three or six data points per cell: weak and inside the spread between calls.
  An easier task (one arithmetic line) showed no difference (`low` 192, 156, 181 against `high`
  195, 175, 206). The lever of the sidecar changes only the top-level field and was not measured
  on thinking at all; on the three models where the cache survived it may be a no-op or may work.
- The sidecar's own lever on real tool-result turns (the sidecar was not reconfigured).
- OpenAI (Responses, chat, Codex ChatGPT route, WebSocket), Gemini, DeepSeek, Bedrock, Vertex,
  and Anthropic models older than the five listed above.
- OpenAI through Codex: blocked on 2026-11-01. With the proxy variables inherited from the sidecar
  (`HTTPS_PROXY=http://127.0.0.1:18795`) Codex 0.164.0 failed to reach chatgpt.com (model refresh
  timed out, "workspace routing discovery failed" five times, cause not diagnosed). With the proxy
  variables removed it connected and answered "You've hit your usage limit ... try again at Nov
  4th, 2026 10:38 AM". The ChatGPT account has no quota until then; no request was served, so
  nothing is known about OpenAI's cache and effort.
- Any harness other than Claude Code and Codex.

## Against the criterion

Register criterion: the lever is switched on only where the flip reads the same cached tokens as
the control. Passed for Sonnet 5.5, Opus 5.5 and Fable 5.1 on the minimal configuration (n = 3, 1
and 1 for the top-level-only case); failed for Haiku 5.5 (2 of 2). Full Claude Code configuration: 2 of 2 clean sequential flips preserved the cache; misses seen
in parallel runs also occurred with no flip. Not run for any other provider (OpenAI blocked by quota until 2026-11-04).

## Spec before and after

Before (`docs/specs/token-savings.md` 7.2, written the same day from the documentation alone):
the Anthropic row said the lever is off because every flip rewrites the cache. After: the row
states the measured split by model, keeps the lever off by default, and records that its effect is
unmeasured.
