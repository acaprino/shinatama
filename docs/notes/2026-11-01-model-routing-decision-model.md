# Automatic model routing through a configurable decision model — study

Date: 2026-11-01
Status: study, not a spec. It feeds ADR-0044 (Proposed); nothing is built from it until that is
approved. Its companion is `docs/notes/2026-11-01-tokeneconomizer-beside-billioncontext.md`; the
two were one study until the user split them (2026-11-01). julia1, a jev-like selector model,
serves both (section 6).

## 1. The question

The user asked (2026-11-01) for an automatic router that picks the model per session and per
agent, **system wide**, through a jev model or a similar router reached with **one key of the
user's**, free or paid ("poco importa"), and then whether the router can "demandare la scelta a un
jev-like decision model configurabile". Later the user pointed to open jev-like models (gutsy,
OpenJev) that ship routing examples.

Words as used here: **jev** is the System One question API (`noul`, `choice`, `score`) that
`tamaratran/fast-jev-compaction` calls (token-savings 5.3); **julia1** is a **jev-like selector
model**, of the same kind as gutsy and the OpenJev models (section 8.2), run locally on the CPU
and answering the same wire format (token-savings 6.1).

## 2. Answer in short

| Question | Verdict | Why, in one line |
| --- | --- | --- |
| Model routing system wide | **Yes, at session start and at sub-agent spawn, never per turn** | The prompt cache is per model: a mid-session switch rewrites the whole prefix at full price (section 3) |
| Router logic | **The choice is delegated to a configurable jev-like decision model**: one `choice` question over a closed set of labels (the harness's real models that pass the gates); low confidence, a high `reject` or any failure keeps the default model | A closed label set cannot invent a model; Daodan's signal selector stays as a second mode (section 5) |
| Router executor | Any endpoint speaking the System One or the OpenRouter Decisions format: julia1 or an open jev-like model such as gutsy locally (no key) by default; Cloudflare Clef-flash (free daily allowance) or TypeSafe Jev, directly or with an OpenRouter key, as an opt-in | Section 6 and 8 |
| julia1's part | A jev-like selector model: the default selector behind `router.endpoint`, for the model router and for the tool chooser, on the same local sidecar as decision compaction | Sections 5.1 and 6 |
| The key | Only in the operating system's vault; a hosted router is opt-in and receives only a redacted digest | Conflicts with ADR-0031 and ADR-0037 as recorded; ADR-0044 narrows them to routing (section 7) |

## 3. Where a model choice is free and where it is not

Provider prompt caches are per model: changing the model mid-session sends the whole prefix
again at full price. So a choice is free in three places and nowhere else:

1. **Session start**: a new instance, a hop destination, a Roundtable participant, a task the
   project manager starts. Before the first request there is no cache.
2. **Sub-agent spawn**: a sub-agent starts its own conversation (a fork inherits the parent's
   cache and is not routed).
3. **Bookkeeping jobs**: titles, digests, scoring (already on julia1, token-savings 4.4.3).

Routing per turn is rejected for the same reason the effort lever is locked off
(token-savings 7.2).

## 4. Seams per harness

| Harness | Session start | Sub-agent spawn | Source |
| --- | --- | --- | --- |
| Claude Code | `--model` on the launch | `CLAUDE_CODE_SUBAGENT_MODEL` (all sub-agents; a `CLAUDE_CODE_SUBAGENT_MODEL_FORCE` also exists and would override a per-call choice, so Shinatama leaves it unset); per call, a `PreToolUse` hook on the `Agent` tool returning `updatedInput.model` (`sonnet`, `opus`, `haiku`, `fable`) | launch: known; the strings are in the 2.1.316 binary; behaviour of the per-call hook: **to verify** (MR1) |
| Codex | `-m` / `-c model=` on the launch | `spawn_agent` of the `multi_agent` feature; whether it takes a model, or a hook can set one: **unknown** | harness-tools.md |
| OpenCode | `OPENCODE_CONFIG` (Shinatama already writes provider and model there per instance) or `--model provider/model` | `agent.<name>.model` in the same config (per agent role, fixed at launch); per call through a plugin hook: **unknown** | functional-design 5.14, harness-tools.md |
| Pi | `--provider` / `--model` | no built-in sub-agents | harness-tools.md |
| Hermes Agent | its `config.yaml` model | `delegate_task`; model setting **unknown** | technical-design 5.7 |
| DeepSeek Harness | per session (`model/selection`) | sub-agents are separate sessions; model setting **unknown** | technical-design |

When the first prompt is typed into the harness's own TUI, Shinatama learns the task only after
the session exists. Two routes, both to verify: the instance dialog carries an optional task
line that is routed before launch; or BillionContext rewrites the `model` field of the
session's **first** request only, inside one provider family, before any cache exists. The
second makes the harness believe it runs another model (context window, compaction threshold,
the model shown in its UI) and is kept as a later option.

## 5. The decision delegated to a configurable decision model

The user asked (2026-11-01) whether a router can **delegate the choice to a jev-like decision
model that is configurable**. It can. Shape (my calls):

1. **Closed candidate set.** Shinatama builds the labels from the models the harness actually
   exposes in this box (the same `MODELS.json` inventory format Daodan's router reads), minus the
   ones a deterministic gate rejects: a capability the request needs (tool calls, image or PDF
   input), a context window smaller than the request, a provider not signed in. The decision
   model can answer only one of these labels; it cannot name a model the harness cannot run.
2. **One `choice` question.** Context: the digest (task line or first prompt, or the sub-agent's
   brief, plus the project kind), clipped and redacted (technical-design's redaction rule), and
   one short card per label: price per million tokens in and out, context window, the catalog's
   tier and benchmark rank, which label is the instance's default, and **the user's own record
   for that model** from the ledger (sessions finished, re-work, cost per finished task, per
   `work_type`). The decision model knows nothing about a model beyond its card; the evidence of
   section 8.4 says that missing information, not the router, is the main limit. Answer: one
   label, its confidence and the probabilities of the others.
3. **A small rule after the answer.** The label is taken when its confidence is at least
   `router.min_confidence` (a setting) and, where the executor returns one, its `reject`
   probability ("none of the options fits, or the state doesn't say", gutsy) stays below
   `router.max_reject`; otherwise, or on any failure (no answer, a label outside the set, a
   timeout), the instance keeps its default model. Optional guard: a `noul` question
   "irreversible or production risk?" asked in the same call; above its threshold the labels
   below the catalog's tier3 are removed before the choice, as Daodan's risk floor does.
4. **Configurable executor.** `router.endpoint` (a URL speaking the System One wire format,
   `POST /v1/systemone`, or the OpenRouter Decisions format, `POST /v1/decisions`; `router.wire`
   says which),
   `router.key` (a reference to the vault entry, never the key itself), `router.model` when the
   endpoint hosts several, `router.timeout_ms`, `router.min_confidence`, `router.mode`. A router
   that does not speak System One is reached through one adapter with a single contract,
   `decide(digest, candidates) -> {label, confidence, probabilities}` (section 8).

`router.mode` takes three values:

| Mode | Who picks | For | Against |
| --- | --- | --- | --- |
| `choice` (what the user asked) | the decision model, among the gated labels | one call; a router model can trade cost against quality with knowledge Shinatama does not have; any jev-like service fits | the quality is the decision model's; the labels and their cards change with the inventory, so a generic model sees a new menu each time; the "why" is the model's |
| `signals` | Daodan's deterministic selector | reproducible and auditable; one policy with Daodan's jobs; risk floors built in | needs the catalog; the signals still need a classifier |
| `off` | nobody | the instance's default model | — |

Default `choice`, because it is the request. `signals` stays available because it already exists
in Daodan's `model-routing` (`scripts/task_router.py`, read 2026-11-01): a cheap classifier fills
ten bounded signals (`work_type`, `scope`, `reasoning_depth`, `ambiguity`, `risk`,
`domain_knowledge`, `output_volume`, `tool_use_required`, `long_context`, `evidence_required`)
and a deterministic module selects the model from the inventory, the catalog's tiers, risk floors
and capability gates. In `signals` mode the ten signals become ten System One questions (one
`choice` per enum, one `noul` per boolean), answered by the same configured endpoint.

Why delegating is compatible with Daodan's rule that "the LLM never picks the model": that rule
answers a measured failure, an LLM writing free text in the selection path (it invented an
optimum it had not computed). A `choice` over a closed, gated label set cannot invent a model or a
number; its output is valid by construction. What remains is the quality of the choice, which
only the ledger measures (MR3, section 9).

What `model-routing` itself does today does not meet the request: its classifier is **a generic
model**, `pick_classifier` takes "the cheapest model that can actually answer the classification
prompt" among the harness's own models and asks it for one JSON object under a long instruction
prompt (`CLASSIFIER_SYSTEM`, `build_prompt`). It is not a decision model, it costs a call to a
frontier provider per node, and its reply carries no confidence. Daodan's coordinators also ask a
human to confirm each node ("an automatic run-wide approval policy is not implemented").
Shinatama routes only its own seams (4), so the user's "automatic" holds there; making Daodan's
classifier use the same configured endpoint, and its confirmation optional, is an upstream
request to Daodan, since Shinatama does not change Daodan.

### 5.1 The same selector as the tool chooser (decided by the user, 2026-11-01)

After reading the review the user asked to use this selector **also as the tool chooser, in place
of the jev chooser** of token-savings 6.5. The two were already the same `ask(state, choice)`
contract on julia1; what changes is that the chooser now takes the router's shape and settings:

1. **Closed candidate set.** The labels are the deferred tools the harness exposes to this
   instance (the set's MCP servers and deferred built-ins), minus the ones TokenEconomizer's
   per-repository evidence has already denied for this launch, and never the recovery tools of
   BillionContext (`mcp__bili__*`), which are always preloaded.
2. **One `choice` question per prompt**, on `prompt.submit`: state = the clipped, redacted prompt
   and the recent window; one card per label = name, one-line description, and the user's own
   usage record for that tool in this repository (TokenEconomizer's `tool_usage` evidence, read
   from its state, is the prior). Answer: the labels to preload, each with its probability;
   `router.min_confidence` and `router.max_reject` apply; below them nothing is preloaded and the
   model searches as today.
3. **Same executor, same settings** (`router.endpoint`, `router.wire`, key reference, timeout):
   julia1 locally by default, gutsy, Jev or Clef-flash by the user's choice. One sidecar answers
   both questions; a routing question runs once per conversation, a chooser question once per
   prompt, so the chooser's latency budget is the binding one (MR4 measures both).
4. **Cache-safe as before.** Preloading adds tool schemas at the tail of the turn's request; the
   tool list of the cached prefix does not change. `tool.describe` shortening of unlikely tools is
   kept as the second move. The seam is still Claude Code's function hooks (`prompt.submit`,
   `tool.describe`), early access; on the other harnesses the chooser waits for a seam.

Order of the three layers on tools, per launch and per turn: TokenEconomizer removes what the
repository never uses (between sessions); the selector preloads what this prompt will likely need
(per turn); BillionContext's tools are outside both.

## 6. Executors

Every executor answers the same `choice` (and, in `signals` mode, `noul`) questions. Those that
speak the System One wire format need only `router.endpoint` and the key; the others go through
the one adapter contract of 5.

| Executor | Wire | Key | Leaves the machine | Notes |
| --- | --- | --- | --- | --- |
| julia1, a jev-like selector model (local) | System One | none | nothing | Default; same sidecar as decision compaction (ADR-0037); routing quality unmeasured |
| TypeSafe Jev (`api.typesafe.ai/v1/systemone`, `jev-latest`) | System One | the user's (`console.typesafe.ai/keys`) | the digest and the cards | The reference implementation of the format; section 8.1 |
| TypeSafe Jev through OpenRouter (`/api/v1/systemone`, `/api/alpha/decisions`) | System One or Decisions | the user's OpenRouter key | the digest and the cards | No waitlist and no TypeSafe account; section 8.1 |
| Cloudflare Clef / Clef-flash on Workers AI | Jev-compatible | the user's Cloudflare token | the digest and the cards | Free daily allowance; open weights; section 8.1 |
| Open jev-like models served locally: gutsy, OpenJev (three projects) | System One and Decisions (gutsy), System One (OpenJev) | none | nothing | gutsy runs on CPU; the OpenJev models mostly want a GPU; section 8.2 |
| Arch-Router-1.5B (local, quantized) | adapter: routes in, route name out | none | nothing | Returns a label without a probability, so `min_confidence` cannot apply; section 8.2 |
| OpenRouter `auto`, Requesty Auto, NotDiamond | adapter | the user's | the digest (NotDiamond: derived metadata) | They route **and** serve, or recommend from their own data; section 8.2 |
| Side call through the agent's own login (TE's route) | adapter | none of Shinatama's | the digest, to the agent's provider | A frontier call per routing; last resort |

## 7. The key and the recorded decisions it touches

- **ADR-0031** (Proposed): Shinatama never holds credentials. A router key is not an agent's
  credential, but it is a credential Shinatama stores. Proposed shape: the key lives only in the
  operating system's vault (Windows Credential Manager), never in the database, snapshots,
  messages or any launch manifest, and is read only by the router sidecar.
- **ADR-0037** RA-004 rejects "a hosted scorer or service, such as the plugin's default
  `api.typesafe.ai` endpoint", and INV-003 says no transcript leaves the machine "for scoring,
  ranking or digests"; token-savings lists "a hosted service" as a non-goal. The user's request
  for a key for a jev model reverses that for routing. Proposed shape: compaction, digests and
  the tool chooser stay local as decided; **routing alone** may use a hosted executor, opt-in
  per user, with a bounded redacted digest and never a transcript. This is the one point that
  needs the user's explicit approval (the Proposed record).

## 8. Router options

Read on 2026-11-01 by a research worker (32 pages; sources in section 10). Prices and limits are
the vendors' own statements, not checked here.

### 8.1 jev-like decision models (System One wire format)

| | TypeSafe Jev | Cloudflare Clef / Clef-flash |
| --- | --- | --- |
| What | TypeSafe's "first public System One Model", early access; typed decisions with calibrated probabilities, no text | Decision models "fully Jev-API compatible"; open weights (Apache-2.0) on frozen Qwen backbones with custom heads |
| Call | `POST https://api.typesafe.ai/v1/systemone`, `Authorization: Bearer <key>`, body `{model: "jev-latest", state, questions}`; also on OpenRouter as `typesafe/jev-1.14` (alias `~typesafe/jev-latest`) with an OpenRouter key, through `POST https://openrouter.ai/api/v1/systemone` (TypeSafe SDK shape) or `POST https://openrouter.ai/api/alpha/decisions` | Workers AI with a Cloudflare token; 64k context |
| `choice` | `criteria` maps each label to a description, up to 255 labels; answer `{choice, confidence, probabilities}`; the docs list **routing** as a use case | same questions |
| Price | $42 per billion input tokens; output "currently free"; free tier and rate limits not stated | 10,000 neurons a day on the free plan; Clef-flash $0.038 per million input (3,455 neurons per million), so about 2.9 million input tokens a day free (the worker's arithmetic) |
| Latency | 0.114 s in TypeSafe's example; 524 ms median measured by Cloudflare (a competitor) | 38.8 ms median (Clef-flash), 209 ms (Clef), Cloudflare's own figures |
| Limits | requests under 32k tokens in fast-jev-compaction's client (older Jev); OpenRouter lists 262,144 tokens of context for Jev 1.14 | not read |
| Data | no training or fine-tuning on input; kept "as long as reasonably necessary"; no zero-retention option; US hosting (policy dated 2025-11-19) | not read |

For the user's request ("una key, anche gratuito") **Clef-flash is the free hosted route and Jev
the reference one, reachable with an OpenRouter key the user may already hold**; all take the same
request, so switching is a change of `router.endpoint` and key. julia1, and the open models of
8.2, answer the same format locally, with no key.

OpenRouter also sells the choice already made: **Jev Router** (`typesafe/jev-router`, read
2026-11-01) "selects a model and reasoning effort for each request"; Jev assesses the conversation,
the router applies a cost policy (`cost_tier` low, medium, high), capabilities and the caller's
`models` / `excluded_models` patterns, and the chosen model serves the request. It is the closest
ready example of the design of 5, with two differences that keep it out of the request path: it
routes **and serves** (every harness would have to send its traffic to OpenRouter, while the wire
is BillionContext's), and "during supported agentic workflows, the router can retain a model,
adjust its effort, or switch models as the task progresses", which is the per-turn switching section 3
rejects. Its split (Jev judges, a policy picks) is the one this study keeps.

OpenRouter's Jev cookbooks include a Jev-verified cascade (a cheap model drafts, Jev checks the
draft against the context, a stronger model runs only when the check fails), the same shape as the
escalation checkpoint of 8.4.

### 8.2 Open jev-like decision models

The user pointed to them (2026-11-01): open models that answer the same questions locally, like
julia1, and that ship routing examples. Which one, if any, backs julia1 stays with the other
project (token-savings 8.4); here they matter as executors behind `router.endpoint`. Read on
2026-11-01 from each project's README or model card; every figure is the authors' own.

| | gutsy (`kouhxp/gutsy`) | OpenJev (`TheoLeeCJ/openjev`) | OpenJev (`SiliconLabAI/OpenJev`) | Open-Jev (`Zefan-Cai/Open-Jev`) |
| --- | --- | --- | --- | --- |
| License | Apache-2.0 | MIT (code; weights keep upstream licenses) | MIT | MIT |
| Model | `gutsy-0.8b` v0.4, a Qwen3.5-0.8B fine-tune | Qwen3.5-4B scorer | `decider-2b` v10 | 2B, 9B, 27B v1.1 (LoRA and a decision head on Qwen) |
| Hardware | CPU, llama.cpp: Q8_0 775 MB, Q4_K_M 505 MB | GPU for 4B BF16; a llama.cpp GGUF backend for CPU | CUDA GPU, about 4 GB | GPU; a CPU deployment for its workbench |
| Wire | `POST /v1/systemone`, `/v1/decisions`, `/api/alpha/decisions` | its own scorer; a routing example with options `access`, `billing` | `POST /v1/systemone` | its own; support-routing documents |
| Routing evidence | JevBench public set 0.732 (Jev 0.866 as reported); hard-tier `routing_hard` 1.00 on a 5-19 item family (noisy); BANKING77 zero-shot with 77 options 0.613; lists of 20-254 options 0.991 | not read | not read | a support-routing pilot: 2B 166/256 (64.8%) |
| Answer | `probabilities`, `confidence`, `margin`, `reject` ("none of these") | not read | not read | not read |
| Weak spots stated | arithmetic, new multi-step rulebooks, world knowledge (CommonsenseQA 0.58 against Jev 0.86); long states on CPU: median 4.3 s, 95th percentile 27 s | — | — | — |

What it means for the router:

- gutsy is a drop-in executor: it speaks both wires on `127.0.0.1:8765`, runs on the CPU, and its
  `reject` probability gives the router a clean "keep the default" signal (5, point 3).
- Its latency on long states is the cost to watch: the digest and the cards must stay short, and
  routing must happen while the session starts, not on the first turn's critical path. The
  state cache it documents (a state encoded once, follow-up questions pay only their own tokens)
  favours one state per routing with all the questions on it.
- World knowledge is its weakest axis, and choosing a model is partly world knowledge (which
  model is good at what). The cards of 5 carry that knowledge in the state, which is what the
  evidence of 8.4 asks for anyway.

### 8.3 Other routers

| Option | Kind | Key / price | Local | Routes on, returns | Fit |
| --- | --- | --- | --- | --- | --- |
| Arch-Router-1.5B (Katanemo) | open weights, Katanemo license (commercial terms not read) | none | yes: quantized variants for llama.cpp, Ollama, LM Studio | route policies (name and description) and the conversation; a route name or `other` | Local alternative to julia1 for the choice; no probability; CPU latency unmeasured |
| RouteLLM (LMSYS) | Apache-2.0 | some routers need an OpenAI key for embeddings | not stated | prompt; strong-model win rate for one strong/weak pair | **Not for agent sessions**: below random on unfamiliar agentic tasks (8.4) |
| OpenRouter `auto` | hosted | one OpenRouter key; no router fee; `:free` models 20 requests a minute, 50 a day | no | about 30 task types, ranked by 7-day market spend share; `allowed_models`, `cost_tier` | Routes per request and serves the call itself: it would have to be pinned per session, and every harness pointed at OpenRouter |
| Requesty Auto (beta) | hosted | Requesty key; one classifier request per conversation | no | the first user message; 50% confidence threshold; choice pinned 60 minutes per `trace_id` | Already session-pinned; serves the call itself |
| NotDiamond | hosted, by application | $0.05 per million tokens routed | no | derived metadata, not the payload; model and effort "for each step in a session" | Per-step routing conflicts with 3 unless restricted to session start |
| Azure model router, LiteLLM auto router, Copilot Auto | hosted or proxy | their own | no | per request (Azure, LiteLLM); Copilot "along natural cache boundaries" | Inside their own stacks; not a seam Shinatama can use across harnesses |

The services that route **and** serve (OpenRouter, Requesty, Azure) put themselves on the wire;
in Shinatama the wire is BillionContext's, and a second owner is excluded (token-savings 7.1,
rule 2). They fit only as decision sources behind the adapter, not as the path the requests take.

### 8.4 What the evidence says about routing agent sessions

- Single-turn routing works in its own benchmarks: FrugalGPT (2023) matches GPT-4 with up to 98%
  lower cost on single-turn tasks; Hybrid LLM (ICLR 2024) makes up to 40% fewer large-model calls
  with no drop in quality.
- It does not carry over to agents by itself. LLMRouterBench (2026-01, 400K instances, 33 models):
  many routers perform alike and several, commercial ones included, "fail to reliably outperform a
  simple baseline". Agent-as-a-Router (2026-06, 176 agentic tasks unlike its training data):
  lightweight trained routers fell to 8.93-21.43%, below random (31.25%); always using the
  frontier model scored 57.14%; giving the router per-dimension performance statistics added 15.3
  points, and the authors name the bottleneck "information deficit". Hence the ledger statistics
  on every card (5, point 2).
- Routing on the task text alone has an error floor. SWE-Router (2026-06, workshop paper): a cheap
  model runs three turns, then a value head decides whether to escalate, and the strong model
  restarts from the original task (Route-AUC 0.780, +15.3 points over no exploration). That
  restart is compatible with 3, since the strong model starts its own conversation; it is a
  later option (an escalation checkpoint), not part of the first build; OpenRouter's Jev-verified
  cascade (8.1) is a ready recipe of the same shape.
- Caches are per model: Claude Code's documentation says a `/model` switch makes the next request
  read "the entire conversation history with no cache hits" and advises choosing model and effort
  "at the top of a session"; Azure says caching applies "only when the same model handles
  consecutive requests". The Anthropic and OpenAI API pages read do not say it in one sentence.

Consequence for acceptance: **"always the instance's default model" is the baseline** every
routed configuration must beat on the ledger (MR3, section 9), cost per finished task and re-work together.

## 9. What must be verified before anything is claimed

| ID | Check | Pass |
| --- | --- | --- |
| MR1 | Claude Code 2.1.316: a `PreToolUse` hook on `Agent` returning `updatedInput.model` changes the sub-agent's model | The sub-agent's requests carry the new model |
| MR2 | Codex, OpenCode, Hermes, DeepSeek Harness: the sub-agent model seam | Found or recorded as absent |
| MR3 | Routing quality: the ledger on ten real sessions per harness with and without routing | Cost per finished task and re-work recorded against the default-model baseline; no saving claimed before |
| MR4 | Routing latency at session start with julia1 or gutsy on the CPU while four agents run | Recorded; the start of a session is not delayed beyond a threshold set before enabling |

## 10. Evidence

- Read, not run: Daodan `model-routing` 1.0.0, `SKILL.md`, `references/task-routing.md`, the
  header and signal schema of `scripts/task_router.py`.
- Repository: token-savings (sections 4–9), functional-design 5.11 and 5.14, harness-tools.md,
  ADR-0031, ADR-0037, ADR-0038, verification register T5.
- Installed versions observed on 2026-11-01 with each CLI's `--version`.
- Web research by a research worker on 2026-11-01 (32 pages read, abstracts only where noted):
  TypeSafe docs (`docs.typesafe.ai/api`, `/introduction`, `/introduction/quickstart`,
  `/primitives/choice`), `typesafe.ai` and its privacy policy; `tamaratran/fast-jev-compaction`
  README, `src/types.ts`, `src/request.ts`, `src/client.ts`; Cloudflare "Introducing Clef"
  (2026-10-01) and Workers AI pricing (2026-10-20); Arch-Router-1.5B model card and arXiv
  2506.16655 (abstract); RouteLLM README; OpenRouter Auto Router and API limits; NotDiamond
  pricing; Azure model router concepts (2025-11-18); Requesty auto routing; LiteLLM auto router
  caching; GitHub Copilot auto model selection; Claude Code, Anthropic and OpenAI prompt-caching
  pages; FrugalGPT (arXiv 2305.05176), Hybrid LLM (2404.14618), RouterBench (2403.12031,
  abstract), RouterArena (2510.00202, abstract), LLMRouterBench (2601.07206, abstract),
  Agent-as-a-Router (2606.22902), SWE-Router (2607.00053). Not found: Jev's free tier and rate
  limits, Martian's pricing. Contradiction kept open: Jev's latency (0.114 s by TypeSafe, 524 ms
  median by Cloudflare).
- Read on 2026-11-01, after the user named them: the gutsy model card
  (`huggingface.co/kouhxp/gutsy`, `README.md`), OpenRouter's Jev hub
  (`openrouter.ai/docs/guides/community/jev.md`) and Jev Router guide
  (`openrouter.ai/docs/guides/routing/routers/jev-router.md`), and the READMEs of
  `TheoLeeCJ/openjev`, `SiliconLabAI/OpenJev` and `Zefan-Cai/Open-Jev` (only their routing,
  hardware, wire and license lines). Nothing was run.
- Static check, not a run: the installed Claude Code 2.1.316 binary contains the strings
  `CLAUDE_CODE_SUBAGENT_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL_FORCE`, `CLAUDE_CODE_SUBAGENT_EFFORT`,
  `CLAUDE_CODE_WORKFLOW_SUBAGENT_MODEL` and `updatedInput`.
- Science Mode checkpoint: new research for the router options (section 8), kept in this note
  (the project has no `docs/research` archive).
