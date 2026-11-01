# Token savings — spec

Date: 2026-10-31
Status: draft, not yet approved. Nothing is built from this document until it is.

This document specifies how Shinatama cuts the tokens that the hosted harnesses (Claude Code,
Codex, OpenCode, Pi, Hermes Agent, DeepSeek Harness) send to their frontier models, without lowering the quality of the work.
The bookkeeping that makes the saving possible (scoring, choosing, digesting) runs on a small
fast model on the local CPU. Working concept name for that model: **julia1**. Its analyses and
benchmarks run in another project; here it is a contract and a set of tasks, not a model
choice to debate.

Who decided what is marked throughout, as in `docs/functional-design.md`:

- **Decided** — said by the user.
- **My call** — a choice made to fill a gap; the user has not confirmed it.
- **Open** — only the user can answer it, and the answer changes what gets built.

Related: functional-design 5.6 (BillionContext sidecar), 5.13 and technical-design 8.4 (the
local tiny-model backend for titles), 5.14 (program, home, per-instance set), the Mukade
handover document (compaction policies of a hop).

## 1. Intent and why

The user runs several agents all day. Every turn resends the whole context, so one large tool
result is paid again on every later turn of the session. The user wants every token-saving
technique that exists today applied in Shinatama, as long as none of them lowers quality, and
wants the bookkeeping done by a local small model rather than by paying a frontier model for
it (decided, 2026-10-31).

Success looks like this: fewer input tokens per turn on the same tasks, the same work
finished, no constraint or decision lost after a compaction, no credential and no transcript
leaving the machine for the bookkeeping, and the same behaviour on every harness Shinatama
hosts.

## 2. Goals, non-goals, anti-goals

### Goals

1. Apply the technique catalog of section 4 in the order of its returns: prevent tokens from
   entering, then remove them reversibly, then cut lossily, never the reverse (my call).
2. Replace summary compaction with **decision compaction**: tool calls and results are kept
   verbatim or removed, user and assistant text is never rewritten (decided, by pointing at
   `tamaratran/fast-jev-compaction`).
3. Run every scoring, ranking and digest step on julia1, offline, with no credentials; what
   julia1 is and how it is trained is out of scope (8.4) (decided).
4. One shared core and one adapter per supported harness, each built on that harness's own
   seam, the way BillionContext does it (decided, 2026-10-31). The behaviour is uniform; the
   integration is not.
5. Measure before claiming: no saving percentage is stated anywhere until it is measured on
   the user's own sessions (existing constraint of functional-design 3; decided).

### Non-goals

- Picking, training, calibrating or benchmarking the model behind julia1. That runs
  elsewhere (8.4).
- Changing Daodan. Daodan ships compiled per harness; Shinatama installs the build.
- Rewriting what the user or the assistant said. Text is never a candidate.
- A hosted service. Everything here runs on the user's machine.

### Anti-goals (would be a failure even if it technically works)

- A compaction that drops the file path, exact error, command or constraint the next turn
  needed. Re-running a tool is acceptable; losing a decision is not.
- Breaking the prompt cache by rewriting early messages or reordering tools per turn. A
  saving that invalidates the cached prefix costs more than it saves.
- Stacking lossy techniques (sliding window + lossy summary + sub-agent summary) so that
  their losses add up.
- Putting julia1 on the critical path of the frontier model's turn. Scoring is incremental
  and asynchronous; when compaction fires, decisions already exist.

## 3. Where tokens go

Every request carries four kinds of content. The techniques below are grouped by which one
they attack, because each has a different lever and a different quality risk.

| Content | Behaviour | Lever |
| --- | --- | --- |
| Static prefix: system prompt, tool schemas, skill listings | Same every turn; cacheable | Keep it stable and small; load schemas on demand |
| Tool results | Largest share; paid on every later turn | Keep them out of context, or make them removable |
| History: user and assistant text | Grows; holds the decisions | Compress reversibly; externalize durable state |
| Model output: reasoning and text | Paid once; sets the next prefix | Effort per step; no rereads; no narration |

## 4. Technique catalog

Each entry names the layer, what it saves, the quality risk and where it runs. "Harness"
means the hosted agent already does it; "Shinatama" means this project builds or configures
it; "julia1" means the local model does the judging.

### 4.1 Static prefix

1. **Prompt caching with a stable prefix.** Append-only sessions; nothing before the newest
   turn is rewritten between requests; MCP servers are not restarted mid-session (a reorder of
   tool schemas invalidates everything after them). Risk: none. Runs: harness; Shinatama
   keeps the sidecar and MCP set stable per launch.
2. **Deferred tools / tool search.** Schemas load on first use; the prefix holds names and one
   line each. Risk: one extra round-trip when a deferred tool is first needed. Runs: harness.
   Anthropic reports roughly 85% fewer tool-definition tokens on large tool sets; not measured
   here (see 9).
3. **Progressive disclosure of skills.** Name and description in context, body on demand.
   Risk: none. Runs: Daodan already does it, on the four harnesses it builds for; Hermes Agent
   and DeepSeek Harness have no Daodan package yet (functional-design 5.5).
4. **MCP pruning per instance.** Mount only the servers a session needs. Shinatama's
   per-instance plugin and skill set (functional-design 5.14) is the switch. Risk: a missing
   tool; mitigated by a visible list at launch. Runs: Shinatama.

### 4.2 Tool results

1. **Code mode / programmatic tool calling.** The model writes code against a typed API; the
   code runs in a sandbox and calls the tools; only one schema and the final result enter
   context. Saves schemas, intermediate results and round-trips at once. Risk: the model
   cannot see intermediate values it may have needed; keep direct calls available. Runs:
   harness where native; Shinatama can put a code-mode gateway in front of MCP servers,
   which works the same for every hosted harness (section 7). Anthropic reports about 37%
   fewer tokens on chained-call tasks; Cloudflare's Code Mode is the same idea over
   isolates. Neither figure is measured here.
2. **Offload to external engines.** The model sends a specification and receives a status;
   the engine does the work. Mukade (DeckCraft, WordCraft, GridCraft) and codebase-map (an
   index answers "where is X" instead of a grep dump) are this pattern. Risk: none beyond the
   engine's own. Runs: Daodan and Mukade.
3. **Store and placeholder.** A large result is saved and replaced by a recoverable
   placeholder. BillionContext's `acp-stored` does this. Risk: zero when the model recovers
   on need. Runs: sidecar (5.6).
4. **Absorb.** Right after a result is read, the model distils it and the original leaves
   context. Lossy, but the choice happens while the context is fresh. Runs: sidecar.
5. **Query discipline.** Grep and line ranges instead of whole files; no reread after an
   edit; structured outputs over prose. Small per call, large cumulated. Runs: harness
   prompts and Daodan rules.
6. **Per-tool digests** (new, julia1). For results with a known shape (test runners, build
   output, directory listings, diffs) a parser keeps the signal and julia1 extracts the rest
   to a fixed output schema; the verbatim original stays recoverable as in 3. Risk: a digest
   that hides the line that mattered; mitigated by keeping the original recoverable and by
   starting with tools whose output has a stable shape. Runs: Shinatama sidecar,
   asynchronously after `tool.call`.

### 4.3 History

1. **Reversible tiered compression.** A summary replaces a range; the original stays
   recoverable (BillionContext compress / decompress). Best quality-to-saving ratio. It must
   be cache-aware: compressing invalidates the cache downstream, so it pays only past the
   break-even the sidecar measures (`acp_cache`). Runs: sidecar.
2. **Decision compaction** (new, julia1; section 5). Score every non-pinned tool call and
   result; keep verbatim or remove; never rewrite text. Runs: Shinatama, through the harness
   adapter (7.1).
3. **Context editing at the API.** Clear old tool results and thinking past N turns, keep
   user and assistant text. Runs: harness/provider where available.
4. **Monolithic summary compaction** (the harness default). One summary, lossy; loses
   decisions, constraints and the "why". Kept only as the fallback when 2 cannot remove
   enough, with a structured compaction prompt (decisions, open items, files touched) and
   firing at task boundaries rather than at a percentage. Runs: harness.
5. **Externalized state.** Anything that must survive a compaction lives on disk: plan file,
   task ledger, decision records, memory. Work-controller run records and Daodan already do
   this. Runs: Daodan; Shinatama's thread store (5.1) keeps the hop context.
6. **Last N turns only.** Cheapest and most dangerous; acceptable only together with 5 and a
   per-turn reinjection of the state file. Not planned on its own.

### 4.4 Output and architecture

1. **Effort per step.** Low for mechanical steps, high for decisions (Daodan's model-routing
   skill). Runs: Daodan.
2. **Isolated sub-agents.** Their dumps never enter the main context; only the report comes
   back. Cost: duplicated prefix and the hand-off loss. Forks inherit the context with a cache
   hit and keep their output out. Runs: harness.
3. **Model routing for bookkeeping.** Summaries, titles (8.4), digests and scoring on julia1;
   the frontier model only on the work. Runs: Shinatama. Routing of the working model itself, at
   session start and sub-agent spawn, delegated to a configurable jev-like decision model:
   ADR-0044 and `docs/notes/2026-11-01-model-routing-decision-model.md`.
4. **Measurement.** A ledger per session of input tokens per turn, cache hit rate, compaction
   count and tool re-runs after compaction. Without it no technique can be said to save
   anything (section 9).

### 4.5 Code mode against direct MCP

| Situation | Wins |
| --- | --- |
| Many tools, chained calls, large intermediates (Office, mail, drive, data pipelines) | Code mode |
| One call, small result, the model must see it to decide (reading a file it will edit) | Direct MCP |
| Both in one session | Tool search + programmatic calling, both kept |

## 5. Decision compaction: what fast-jev-compaction does

Read from `github.com/tamaratran/fast-jev-compaction`, commit `e3f262a` (2026-09-17),
package version 0.2.0, MIT. It is both an npm library (`src/`) and a Claude Code
function-hook plugin (`hooks/`). What follows is from its README and source; it was not run.

### 5.1 Mechanism

1. Every `tool_use` is paired with its `tool_result` by `tool_use_id`. Calls in the first
   message and in the newest `preserveRecentMessages` messages (default 6) are pinned.
2. The **state** sent to the scorer is the whole conversation, oldest first, every result
   replaced by a short note (`ok, 4213 chars (omitted)`); inputs and texts included; nothing
   summarized. It is fitted under `maxStateTokens` (default 25,000, estimated without a
   tokenizer) in stages: inputs truncated to 1000, 200, 60 characters; long texts abridged to
   head and tail; old messages collapsed to a size note; old calls reduced to one line
   (`t12 Read file_path=src/a.ts → ok 480ch`); old call-only runs folded. If it still does not
   fit, compaction throws.
3. For every non-pinned call two `noul` (yes/no probability) questions: does the **call**
   still matter (knowing it was made, with its input), and does the **result** still have to
   stay verbatim (re-running the tool would not do).
4. Questions are batched under `maxRequestTokens` (30,000); the full state is resent with
   every batch; batches run concurrently.
5. Decisions against `keepThreshold` 0.5: keep call and result; else keep the call and
   truncate the result to `truncateHeadChars` (300) plus a note; else remove both. No result
   is ever left without its call; untouched messages are returned as the same objects.
6. The Claude Code adapter registers on `session.compact` (function hooks, early access,
   Claude Code 2.1.274+, flag `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1`) and on `turn.complete`,
   where it triggers compaction at `compactAtPercent` (60) of the window. It falls back to the
   built-in summary when the reduction is under `minReductionRatio` (0.25) or the scorer
   fails.

### 5.2 Why it is safe with a weak scorer

The only possible error is removing a result that was still needed; the assistant then
re-runs the tool. Constraints in text always survive. Pinned messages protect the current
work. This is the property that makes a small local model acceptable here, where it would
not be acceptable as a summarizer.

### 5.3 The seam

`JevAsker.ask(state, questions) → { answers: { <name>: { noul: p } } }`. Everything else
(fitting, batching, deciding, rebuilding) stays as in the library. Question types in the
wire format: `noul` (probability), `choice` (label with confidence and probabilities),
`score`.

Two facts to act on:

- The library accepts `baseUrl`, but `hooks/fast-jev.ts` does not pass it to
  `buildJevRequest`, so the plugin is wired to `https://api.typesafe.ai/v1/systemone`. One
  line and one plugin option separate the plugin from a local scorer.
- The hook contract (`types/claude-code.d.ts`) has no event for offering tools per turn.
  `tool.describe` rewrites descriptions, `prompt.submit` can preload deferred tools,
  `$.tool.list()` reads the set; `$.session.usage()` gives `context.percent`.

## 6. julia1: the local executor

### 6.1 Contract

julia1 is reached through one local HTTP endpoint speaking the System One wire format of
5.3 (my call: reuse the format so the plugin needs no change beyond `baseUrl`). It answers
`noul`, `choice` and `score`. Model, quantization, runtime and thresholds are owned by the
other project; this spec only fixes the tasks and the acceptance in section 9.

### 6.2 What Shinatama guarantees the executor

The model, its runtime, its latency budget and how it meets it are the other project's
concern. Shinatama's side of the contract is to make the job as small and as regular as it
can (my calls):

1. **Append-only, incremental state.** The state is the same prefix plus a delta at every
   call; Shinatama scores the new calls after each turn instead of the whole history at
   compaction time, so an executor that caches its prefix only ever reads the delta.
2. **Tunable state budget.** `maxStateTokens` is a setting, not a constant; the library's
   fitting stages degrade in a controlled way when it is lowered.
3. **Features precomputed.** The deterministic signals of 6.3 travel with the question, so
   the executor is never the only source of judgement.
4. **Off the critical path.** No request to the executor is awaited by the frontier model's
   turn (section 7).

### 6.3 Deterministic features

Computed by Shinatama from the transcript, no model involved; they carry a large part of the
signal and make every decision explainable:

1. **Cited later.** Paths, identifiers, line numbers or error strings that appear only in
   this result and reappear in later text or tool inputs.
2. **File edited after the Read.** The result is stale by definition.
3. **Same call repeated later.** The earlier one is superseded.
4. **Error then resolved.** An `isError` result followed by a similar successful call.
5. **Prior per tool.** Glob, Grep and listings re-run at no cost; a test run is valid until
   the next one; a Read of a file the task names stays.

Pipeline (my call): rules decide first where they are conclusive (a Read of a file edited
afterwards, a call repeated later); the executor is asked only about the rest. Each decision
records which stage produced it.

### 6.4 Threshold

The executor returns a probability; the library turns it into keep or drop through
`keepThreshold`. Shinatama exposes it as a setting and takes its value from the model's
delivery, because what a probability means depends on the model (6.1, 8.4). Shinatama's own
part is to record in the ledger the rate of tool re-runs after compaction, which only the
harness side can observe (section 9).

### 6.5 Tool chooser

The tool list cannot change per turn without breaking the prefix cache, and the harness
exposes no per-turn offer hook. Two cache-safe moves, same `ask` contract:

1. On `prompt.submit`, rank deferred tools (embedding or `choice` question on the prompt and
   the recent window) and preload the likely ones, so the model skips the search round-trip.
2. Through `tool.describe`, shorten descriptions of tools the session is unlikely to use.

The chooser is the same configurable selector as the model router (decided by the user,
2026-11-01): one `choice` question over a closed label set, on the endpoint and with the settings
of ADR-0044; TokenEconomizer's per-repository usage evidence is its prior, and BillionContext's
recovery tools are never candidates. Shape in
`docs/notes/2026-11-01-model-routing-decision-model.md`, section 5.1.

### 6.6 Digests

Per-tool parsers for stable output shapes first; julia1 with a fixed output schema for the
remainder; the original always recoverable (4.2.6). Runs asynchronously after `tool.call`,
never on the model's turn.

## 7. Where it runs in Shinatama

| Component | Contract | Local executor | When |
| --- | --- | --- | --- |
| Call/result scoring | `ask(state, noul[])` | rules where conclusive, julia1 for the rest | incrementally on `turn.complete` |
| Compaction | `applyDecisions` | no model | at threshold; decisions already made |
| Tool chooser | `ask(state, choice)` | julia1 | `prompt.submit` |
| Digests | parser, then schema extraction | julia1 | async after `tool.call` |

- **One sidecar per box** (functional-design 5.14): an HTTP process on localhost with the
  System One wire format, launched and supervised like the BillionContext sidecar (5.6),
  private root, no credentials, no system change. julia1 and the title backend of 8.4 share
  the runtime (llama.cpp through a Rust binding or a bundled `llama-server`, builder's
  choice) and the one-time opt-in model download.
- **One adapter per harness** (decided, 2026-10-31). The core is shared: the scoring
  library of section 5, the features of 6.3, the julia1 endpoint, the ledger. What touches
  the harness is an adapter written for that harness, on the seam it offers, and verified on
  its installed version (functional-design 3). There is no single proxy for all of them.
  See 7.1.
- **Code-mode gateway** in front of MCP servers (4.2.1) is a second uniform piece; it is a
  later milestone (8.1).
- **Scoring is incremental**: after each turn julia1 scores only the new calls with a warm
  cache. When compaction fires there is only applying to do. The frontier model never waits
  for julia1.

### 7.1 Adapters per harness

BillionContext already integrates each harness on a different seam, behind one shared
proxy core. The seams it uses are the reference for where each adapter can attach (read from
its source, not run here):

| Harness | Seam BillionContext uses | Adapter for decision compaction |
| --- | --- | --- |
| Claude Code | No in-process extension; installer pins `ANTHROPIC_BASE_URL` to a loopback port and a `SessionStart` hook makes sure the proxy listens (`claude-native-bootstrap.ts`) | The `session.compact` function hook of fast-jev-compaction where the installed version has function hooks; otherwise the base-URL route |
| Codex | Proxy side: Codex recognized by its User-Agent, its compaction handled on the Responses wire (`codex-compact.ts`) | On the wire, inside the request path Codex already takes |
| OpenCode | Plugin (`.server()` on 1.x, `{ id, setup }` on 2.x) with an `http.request` hook that routes model URLs to the proxy (`src/agent/opencode*.ts`) | Inside the OpenCode plugin |
| Pi | Extension that patches `fetch` to route model requests to the proxy (`src/agent/pi-native.ts`, `native-intercept.ts`) | Inside the Pi extension |
| Hermes Agent | No plugin of its own: Hermes runs on its real home and its httpx stack is routed through the proxy by environment, with the proxy's certificate authority passed as `HERMES_CA_BUNDLE`; providers come from `config.yaml`, and with none configured traffic does not reach the proxy (`launcher.ts`, Hermes branch) | On the wire, inside the request path Hermes already takes. Hermes compacts natively at 50% of the window and has a `post_compaction` hook (technical-design 5.7); how to stand its compaction down is not yet known and is verified first |
| DeepSeek Harness | A plugin installed per profile through dsh's own `dsh plugin --profile <p> add` (`dsh-channel.ts`, `dsh-ledger.ts`); remote traffic through the proxy, loopback and `DEEPSEEK_BASE_URL` routes rewritten (`launcher.ts`, dsh branch); dsh's native compaction rows disabled (`dsh-compaction-disable.ts`) | Inside the dsh plugin of the box's profile, the ACP profile Shinatama runs (functional-design 5.16); native compaction stood down the same way |

Rules every adapter follows (my calls):

1. **One owner of compaction.** When the adapter compacts, the harness's native compaction
   is stood down or intercepted, as BillionContext does (`dsh-compaction-disable.ts`, the
   Pi compaction cancel). Two compactions on one history destroy each other's substrate.
2. **One owner of the wire.** Where BillionContext already sits on the request path, the
   adapter acts inside that path, not as a second proxy in front of or behind it.
3. **Cache-safe.** No rewrite inside the cached prefix; compaction happens at the boundary
   the sidecar's break-even indicates.
4. **Verified per version.** Each adapter has a conformance test against the real installed
   harness and a version gate, like the other adapters of functional-design 6.

### 7.2 Layer ownership and the BillionContext baseline

BillionContext owns the request path and every reversible reduction of what is already in
context. Any other token-saving tool is admitted only if it passes all five tests (my call,
from the 2026-11-01 review of `chipfighter/context-attachment-manager`):

1. It attacks a content kind that no layer in the table below already attacks.
2. It is not a second proxy on the wire; it acts inside BillionContext's path or off the path.
3. It does not rewrite the cached prefix on its own schedule.
4. What it removes comes back through the mechanism the model already knows (`acp_retrieve`,
   `decompress`), not through a second one.
5. It works on every hosted harness, or it is optional, off by default and chosen per instance.

| Content | Owner |
| --- | --- |
| Large tool results | BillionContext CCR: stored, replaced by a placeholder, recovered with `acp_retrieve` |
| A result just read | BillionContext Absorb |
| History ranges | BillionContext tiered compress and decompress; decision compaction (section 5) inside the same path |
| The human message | BillionContext `promptRewrite` |
| Model output | BillionContext output steering |
| Images | BillionContext: older media folds by default (#2640) in token space, plus a byte-space shed backstop on the WebSocket lane and a rescale (`egress-shed.ts`, `image-rescale.ts`); originals come back with `decompress({ imageRef })` or `acp_read_original_image` |
| Static prefix | The harness and Daodan |

**BillionContext baseline** (decided by the user, 2026-11-01: CCR must be visibly on, and both
input and output must be compressed):

- CCR: `compress.ccr.enabled = true`; `compress.ccr.minToolTokens` is 4000 in the live config.
- Input: `compress.absorb.enabled`, `compress.tiers`, `compress.adaptivePressure.enabled`,
  `promptRewrite.mode = "auto"`.
- Output: `compress.outputSteering.enabled = true`, with `verbosityLevel` 0 to 4 (default 2) and
  `effortRouting` (default true once enabled). The key lives inside the `compress` block
  (`src/config.ts`, the compress key whitelist), not at the top level of `config.json`. Read on
  2026-11-01: this was **not active**; the live `config.json` has no such key and the kernel
  default is `enabled: false` (`kernel/src/output-steering.ts`). The verbosity lever appends one
  frozen directive to the tail of the system prompt and rewrites no past text, so it does not
  break the goal that text is never rewritten. The effort lever lowers reasoning effort on
  tool-result continuation turns only, so it flips the field at every boundary between a user
  turn and a tool-result turn. **The effort lever is locked off on every harness, provider and
  model** (decided by the user, 2026-11-01): Shinatama writes `effortRouting: false` into the
  `compress.outputSteering` block it manages and offers no switch for it. Reasons, from the T5
  record: on Haiku 5.5 the flip invalidates the message cache (2 of 2 trials); on Sonnet 5.5,
  Opus 5.5 and Fable 5.1 it did not, but its effect on reasoning tokens is weak (8% to 32% in a
  small sample) and was not measured for the sidecar's own top-level edit; for OpenAI, Gemini and
  DeepSeek nothing is measured (OpenAI blocked by quota until 2026-11-04). Cost of being wrong is
  asymmetric: a cache miss rewrites the whole prefix at full price, the saving is a few hundred
  reasoning tokens. The lock is lifted only by a later decision backed by a passed T5 row for that
  provider and model **and** a measured saving that beats the miss risk; BillionContext's
  per-provider merge (`src/compress-settings.ts`) would carry the exception.
**Effort lever and the prompt cache, per provider** (official documentation read 2026-11-01;
nothing here is measured, T5 measures it):

| Provider (wire) | Reached by | What the documentation says | Lever |
| --- | --- | --- | --- |
| Anthropic Messages | Claude Code; OpenCode, Pi, Hermes when pointed at Anthropic | Documentation: changing `output_config.effort` or `thinking.budget_tokens` "always invalidates message blocks". **Measured 2026-11-01 on Claude Code 2.1.316 (T5 record):** a top-level-only change kept the cache on Sonnet 5.5, Opus 5.5 and Fable 5.1 (minimal prefix, 5 trials) and invalidated the whole message cache on Haiku 5.5 (2 of 2). Claude Code itself sends effort as a per-message `role: "system"` entry as well, and its own `--effort` changes kept the cache in 6 of 6 minimal-prefix trials and 2 of 2 clean full-configuration trials; parallel full-configuration runs showed full misses that also occurred with no effort change, so they are not attributed to it. Whether the top-level `low` changes behavior on the three models is unmeasured | **Locked off.** Never for Haiku 5.5 (measured miss) |
| OpenAI Responses and chat | Codex (ChatGPT route, WebSocket); OpenCode, Pi, Hermes on OpenAI | `reasoning.effort` "can change model-side reasoning instructions"; on GPT-6 and later a `configuration_update` item changes effort and keeps the prefix. Cache is exact-prefix | **Locked off** (nothing measured) |
| Gemini | OpenCode, Pi, Hermes on Google | The caching page is silent on `thinkingBudget`; the thinking page is silent on caching; current models use `thinking_level`, while BillionContext edits `thinkingBudget` | **Locked off**, and the lever may not even match the current field |
| DeepSeek | DeepSeek Harness; others pointed at DeepSeek | Disk cache matches an input prefix "fully"; the pages do not say whether `reasoning_effort` or `thinking.type` is part of the match. With `tools`, earlier `reasoning_content` must be sent back or the API returns 400 | **Locked off** (nothing measured) |

Consequence for the baseline: the verbosity lever is cache-neutral by construction (a frozen
block at the tail of the system prompt, identical on every turn), so it is the only output lever
on, and T6 confirms it per harness. The effort lever is locked off (see the baseline above); T5
and T7 stay in the register as the evidence a future decision would need, and the cache-safe
mechanisms of T7 (per-message effort on Anthropic, `configuration_update` on OpenAI) are the only
routes to lift the lock without risking the cache; BillionContext implements neither today.

- Shinatama shows one status line per instance: CCR on or off, input on or off, output level.
  It is read from the effective configuration, not the intended one, and a baseline item that is
  off shows as a warning, never as a silent state.

**Evaluated and not admitted: `chipfighter/context-attachment-manager`** (MIT, v0.9.1; README
and the source headers of `proxy.ts`, `claude-rewrite.ts`, `rewrite.ts`, `claude-thinking.ts`,
`hooks/register.js` and `INSTALL.md` read on 2026-11-01; not run). What it does: a local proxy
that replaces the images of earlier turns with text placeholders and lets the model fetch one
back, plus a feature-line index so the model can find an image by what it showed. It also keeps
Anthropic thinking blocks valid after it changes the history (`claude-thinking.ts`). Why it is
not run beside BillionContext:

- **Overlap.** BillionContext already folds older media by default (#2640), has a byte-space
  backstop and a rescale, and recovers the originals. Its own authors removed the equivalent
  sliding-cutoff strip (`compress.stripImages`) because it broke the prompt cache turn after
  turn on three of four wires (`src/config.ts`, `warnInertStripImages`). CAM's automatic
  selection is that same shape.
- **Second owner of the wire** (section 7, rule 2): both pin `ANTHROPIC_BASE_URL`, and two
  history rewriters in a chain both have to keep the thinking chain valid.
- **Reach.** Claude support applies to the Code tab of Claude Desktop (a function hook and a
  panel in the desktop browser); Codex support to the Codex desktop app. Shinatama hosts the
  CLI harnesses, so it would not engage there, and OpenCode, Pi, Hermes and DeepSeek Harness
  are out of its reach.
- **Scope.** It matches `image` blocks (pasted images, images inside tool results, PDF-page
  screenshots Codex attaches). A PDF sent as a `document` block is not handled.
- **Evidence.** Its figures are request bytes on a nine-frame test, not tokens; not repeated.

Where it is right: documents handled as render-and-review loops (slides, pages, scans) put an
image into context on every iteration, and that is where image weight concentrates (the user,
2026-11-01). Whether BillionContext's default folding fires early enough on such a loop is
**not measured**; a scan of the stored sessions found image blocks in 5 of 5,015 files, which
cannot tell folding from absence (the store may hold folded copies; `index.db` was not
readable). The test is a session of the real workflow with the ledger of 4.4 on: image tokens
per turn and the turn at which they fold. If they fold too late, the fix is to tune or extend
BillionContext's media fold for tool-result images, in its own path, and to borrow CAM's one
good idea (a described, searchable index of images), not to add CAM (my call).

**Evaluated, admissible as an optional module: `microsoft/TokenEconomizer`** (Apache-2.0, commit
`c53a4a6`, 2026-10-31; read, not run; study
`docs/notes/2026-11-01-tokeneconomizer-beside-billioncontext.md`, ADR-0045). It attacks cross-session
waste no layer above attacks (re-learning a repository, tools never used, stuck turns) through
hooks, not a proxy. It passes tests 1, 2 and 3; it fails test 4 for tool pruning (a denied tool
comes back only through a person's `restore`), and it covers Claude Code, Codex, OpenCode
(experimental) and Pi, not Hermes Agent or DeepSeek Harness, so by test 5 it is off by default
and chosen per instance. Adaptations (my calls): state under a per-box `TOKENECONOMIZER_HOME`;
tool pruning in `propose` mode, with Shinatama applying the deny list at launch instead of
letting it write the repository's `.claude/settings.local.json` or `.codex/config.toml`, which
the user's own runs outside the box would also read; loop guard on, its judge moved to julia1
through a `noul` question once TokenEconomizer offers an optimizer endpoint (an upstream request,
not a fork); micro-skills only as `envfacts`. The installed Claude Code 2.1.316, OpenCode 2.2.5
and Pi 1.4.0 are newer than its tested ranges; Codex 0.164.0 is inside. Checks TE1 to TE10 of the
study come first, among them the four contact points with BillionContext (its section 5).

## 8. Order of work and open questions

### 8.1 Milestones (my calls)

1. **Measure.** The ledger of 4.4.4 on the user's current sessions, no technique added:
   tokens per turn, cache hit rate, compaction count, tool re-runs after compaction. This
   is the baseline every later claim is compared with.
2. **Claude Code first.** Fork or vendor nothing: add `baseUrl` to the plugin, point it at a
   local endpoint that answers with the rules of 6.3 alone (no model). Measure.
3. **julia1 behind the endpoint**, delivered by the other project with its threshold.
   Measure against 2.
4. **Adapters for Codex, OpenCode, Pi, Hermes Agent and DeepSeek Harness**, one at a time,
   each on its own seam (7.1), each verified on the installed version. Hermes Agent and
   DeepSeek Harness come after their proxy coverage is verified (register HM12, DS12), since
   their adapters ride that path.
5. **Tool chooser and digests.**
6. **Code-mode gateway** in front of MCP.

### 8.2 Decided

- Decision compaction over summary compaction; text never rewritten.
- Bookkeeping on a local CPU model; model analyses and benchmarks elsewhere.
- No saving percentage claimed before it is measured.
- One adapter per supported harness on its own seam, as BillionContext does; no single
  shared proxy.

### 8.3 Open

None at the moment.

### 8.4 Out of scope here (decided, 2026-10-31)

Training, fine-tuning, calibration, operating threshold, architecture and benchmarks of
julia1 or whichever model is chosen. This document fixes only the wire contract (6.1), what
Shinatama guarantees the executor (6.2), the signals it precomputes (6.3) and how it acts on
the answer (6.4).

## 9. Evidence and verification

### 9.1 What this document rests on

- Read, not run: `tamaratran/fast-jev-compaction` at commit `e3f262a`: README, `src/types.ts`,
  `src/request.ts`, `src/client.ts`, `hooks/fast-jev.ts`, `hooks/hooks.json`, and the hook
  contract `types/claude-code.d.ts` (event list, `session.compact` input and result,
  `ContextCategory`). Clone kept in the session scratchpad, not in this repository.
- Read, not run: the BillionContext source (`sidecar/source-build`), header comments of
  `host-profiles.ts`, `codex-compact.ts`, `claude-native-bootstrap.ts`,
  `src/agent/opencode*.ts`, `src/agent/pi*.ts`, `src/agent/native-intercept.ts`,
  `src/agent/dsh-compaction-disable.ts`, `dsh-channel.ts` and `dsh-ledger.ts`, and the Hermes
  and dsh branches of `launcher.ts`.
- Reused from technical-design 5.7: the Hermes Agent 0.22.5 and DeepSeek Harness
  0.2.1-rc.5 facts (hooks, compaction, profiles), themselves research, not runs.
- Vendor claims, unverified: Anthropic's figures for tool search and programmatic tool
  calling; Cloudflare's Code Mode write-up; Morph's "FlashCompact" and the community
  "Magic Compact" plugin found while looking for the name "fast compact". Third-party
  guides disagree on the auto-compact threshold (83% to 95%), which only shows it changes
  by version.
- Existing constraint reused: functional-design 3 forbids claiming a measured saving for
  compression; 8.4 already specifies a local 0.8B GGUF backend through llama.cpp with its
  unmeasured points listed.
- Science Mode checkpoint: reuse of the evidence above; no new research started here, since
  the model benchmarks run in the other project and the decisions in this document are
  about contracts and task decomposition, which the repository reading settles.

### 9.2 What must be verified before anything is claimed

1. The ledger of 8.1 step 1 on at least ten real sessions per harness.
2. Tool re-run rate after compaction, before and after each milestone.
3. Latency of one incremental scoring pass on the target CPU while four agents run.
4. That the function-hook plugin loads on the installed Claude Code and that `session.compact`
   receives the main conversation and sub-agent transcripts as `types/claude-code.d.ts`
   describes. Documentation alone is not proof (functional-design 3).
5. For each adapter, that its rewrite leaves the cache hit rate unchanged and that the
   harness's native compaction does not run behind it.
6. For Hermes Agent, which setting or hook stands its native compaction down; until that is
   known its adapter is not built.

Verification records go under `docs/verification/` and the register, as for the other
designs.
