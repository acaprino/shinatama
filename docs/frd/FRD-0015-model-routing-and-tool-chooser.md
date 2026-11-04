# FRD-0015: Model routing and the tool chooser

| | |
|---|---|
| Version | 1 |
| Status | Accepted 2026-11-01 (this Markdown text) |
| Date | 2026-11-01 |
| Sources | docs/notes/2026-11-01-model-routing-decision-model.md 1 to 10; specs/token-savings 4.4.3, 6.5; harness-tools (seams) |
| Governing ADRs | ADR-0044, ADR-0031 v2, ADR-0037 v2 |
| Register sections | MR1 to MR4 (proposed in the note; ids to be assigned in the register) |
| Related records | FRD-0003 (the router key), FRD-0013 (julia1, the ledger), FRD-0016 (TokenEconomizer's evidence as the chooser's prior), FRD-0014 (what leaves the machine) |

## 1. Purpose and scope

The user asked for an automatic router that picks the model per session and per agent, system
wide, whose choice is delegated to a configurable jev-like decision model reached with one key
of the user's, and then asked that the same selector be the tool chooser. This record states
where a choice is free, how the decision is delegated, the executors, the key, the chooser, and
what must be measured before any saving is claimed.

Vocabulary (note 1): **jev** is the System One question API (`noul`, `choice`, `score`);
**julia1** is a jev-like selector model run locally on the CPU answering that wire format; the
**digest** is the clipped, redacted task description the decision model receives; a **card** is
the short description of one candidate label.

## 2. Requirements

### 2.1 Where the model is chosen

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Shinatama routes the model automatically, system wide, at three points only: session start (a new instance, a hop destination, a roundtable participant, a task the project manager starts), sub-agent spawn (not a fork, which inherits the parent's cache), and bookkeeping jobs (already on julia1). | U 2026-11-01 (system-wide automatic routing); A (the three points) | V1 | MR-3 | 0044 |
| R02 | The router never changes the model between the turns of one conversation, because provider prompt caches are per model and a switch rewrites the whole prefix at full price. | A | V1 | — | 0044 |
| R03 | When the first prompt is typed into the harness's own TUI, two routes are candidates: an optional task line in the instance dialog, routed before launch; or BillionContext rewriting the `model` field of the session's first request only, inside one provider family, kept as a later separately verified option. | A | V1 | MR-1, MR-2 | 0044 |

### 2.2 The decision, delegated to a configurable decision model

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R04 | The candidate set is closed: the labels are the models the harness actually exposes in this box (the `MODELS.json` inventory format Daodan's router reads), minus those a deterministic gate rejects (a capability the request needs, such as tool calls or image or PDF input; a context window smaller than the request; a provider not signed in). The decision model can answer only one of these labels. | U 2026-11-01 (delegation); A (closed set and gate) | V1 | — | 0044 |
| R05 | One System One `choice` question per routing: state = the digest (task line, first prompt or sub-agent brief, plus the project kind), clipped and redacted with the engine's redaction rules; one card per label (price per million tokens in and out, context window, catalog tier and rank, which label is the instance's default, and the user's own ledger record for that model per kind of work). Answer: one label, its confidence and the probabilities of the others. | A | V1 | MR-3 | 0044 |
| R06 | The label is taken only when its confidence reaches `router.min_confidence` and, where the executor returns one, its `reject` probability stays below `router.max_reject`; otherwise, and on any failure (no answer, a label outside the set, a timeout), the instance keeps its default model, never a cheaper one. | A | V1 | — | 0044 |
| R07 | An optional guard asks a `noul` question ("irreversible or production risk?") in the same call; above its threshold the labels below the catalog's tier 3 are removed before the choice, as Daodan's risk floor does. | A | V1 | — | 0044 |
| R08 | `router.mode` is `choice` (default: the decision model picks among the gated labels), `signals` (Daodan's ten bounded signals asked as System One questions, one `choice` per enum and one `noul` per boolean, resolved by the installed `task_router.py`) or `off` (the instance's default model). | A (default `choice`, because it is the request) | V1 | — | 0044 |
| R09 | Delegation is compatible with Daodan's rule that "the LLM never picks the model": that rule answers a measured failure of free-text selection; a `choice` over a closed, gated set cannot invent a model or a number. Making Daodan's own classifier use the same endpoint is an upstream request; Shinatama routes only its own seams and does not change Daodan. | A | V1 | — | 0044 |

### 2.3 Executors and configuration

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R10 | Settings: `router.endpoint` (a URL speaking System One, `POST /v1/systemone`, or the OpenRouter Decisions format, `POST /v1/decisions`; `router.wire` says which), `router.key` (a reference to the vault entry, never the key), `router.model` when the endpoint hosts several, `router.timeout_ms`, `router.min_confidence`, `router.max_reject`, `router.mode`. | A | V1 | — | 0044 |
| R11 | Default executor: julia1 locally, no key, on the same sidecar as decision compaction. Also without a key: open jev-like models served locally (gutsy on the CPU speaks both wires; the OpenJev models mostly want a GPU). With the user's key, opt-in: TypeSafe Jev directly or through OpenRouter, Cloudflare Clef-flash on Workers AI. A router that speaks neither wire (Arch-Router, NotDiamond, OpenRouter `auto`) goes through one adapter `decide(digest, candidates) → {label, confidence, probabilities}`; a router that returns no probability cannot apply `min_confidence`. | U 2026-11-01 (a key, free or paid; open jev-like models); A (default local) | V1 | MR-4 | 0044 |
| R12 | Services that route and serve the request themselves (OpenRouter `auto` and Jev Router, Requesty, Azure model router) never take the request path, which is BillionContext's; they fit only as decision sources behind the adapter. Per-turn switching inside such a service is excluded by R02. | A | V1 | — | 0044, 0038 |
| R13 | The router key lives only in the operating system's vault (Windows Credential Manager, the Keychain, the Secret Service) and is read only by the router sidecar; it never appears in Shinatama's database, snapshots, messages, configuration files, launch manifests or logs. This narrows ADR-0031 to agent credentials. | A (vault); U 2026-11-01 (a key of the user's) | V1 | — | 0044, 0031 v2 |
| R14 | A hosted decision model receives only the clipped, redacted digest and the cards, never a transcript or an agent credential, and only after the user's opt-in per user. This narrows ADR-0037's "nothing leaves the machine" to compaction and digests; routing and the tool chooser follow the router's opt-in. | U 2026-11-01 (a hosted key); A (digest only) | V1 | X4 | 0044, 0037 v2 |
| R15 | Routing happens while the session starts, not on the first turn's critical path; the digest and cards stay short because jev-like models on CPU are slow on long states (gutsy: median 4.3 s, 95th percentile 27 s); one state per routing with all questions on it exploits the executors' state cache. | A | V1 | MR-4 | 0044 |

### 2.4 Binding on each harness's seam

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R16 | Session start: Claude Code `--model`; Codex `-m` or `-c model=`; OpenCode `OPENCODE_CONFIG` (where Shinatama already writes provider and model per instance) or `--model provider/model`; Pi `--provider` and `--model`; Hermes Agent its `config.yaml` model; DeepSeek Harness per session (`model/selection`). | A | V1 | MR-2 | 0044 |
| R17 | Sub-agent spawn: Claude Code `CLAUDE_CODE_SUBAGENT_MODEL` for all sub-agents, or per call a `PreToolUse` hook on the `Agent` tool returning `updatedInput.model` (`CLAUDE_CODE_SUBAGENT_MODEL_FORCE` is left unset because it would override a per-call choice); Codex `spawn_agent`, OpenCode `agent.<name>.model`, Hermes `delegate_task` and DeepSeek sub-agent sessions are unknown seams, found or recorded as absent. Pi has no built-in sub-agents. | A | V1 | MR-1, MR-2 | 0044 |
| R18 | Each seam is verified on the installed version before use; the strings exist in the Claude Code 2.1.316 binary, the per-call hook's behaviour is unverified. | S | V1 | MR-1 | 0044, 0010 |

### 2.5 The tool chooser

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R19 | The same selector is the tool chooser, in place of the jev chooser of token-savings 6.5: on each `prompt.submit` one `choice` question over the deferred tools the instance exposes (the set's MCP servers and deferred built-ins), minus those TokenEconomizer's per-repository evidence denied for this launch, and never BillionContext's recovery tools (`mcp__bili__*`), which are always preloaded. | U 2026-11-01 | V1 | MR-4 | 0044 |
| R20 | State = the clipped, redacted prompt and the recent window; one card per label = name, one-line description and the user's usage record for that tool in this repository (TokenEconomizer's `tool_usage` evidence as the prior). Answer: the labels to preload with their probabilities; `router.min_confidence` and `router.max_reject` apply; below them nothing is preloaded and the model searches as today. | A | V1 | MR-4 | 0044 |
| R21 | Same executor, settings, key reference and timeout as the router; a routing question runs once per conversation, a chooser question once per prompt, so the chooser's latency budget is the binding one. | A | V1 | MR-4 | 0044 |
| R22 | Cache-safe: preloading adds tool schemas at the tail of the turn's request and never changes the tool list of the cached prefix; shortening descriptions of unlikely tools through `tool.describe` is the second move. The seam is Claude Code's function hooks (`prompt.submit`, `tool.describe`), early access; on the other harnesses the chooser waits for a seam. | A | V1 | — | 0044, 0038 |
| R23 | Order of the three layers on tools: TokenEconomizer removes what the repository never uses (between sessions); the selector preloads what this prompt will likely need (per turn); BillionContext's tools are outside both. | A | V1 | — | 0044, 0045 |

### 2.6 Measurement before claims

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R24 | The ledger records per routed conversation the labels, cards, answer, executor, cost and outcome; "always the instance's default model" is the baseline every routed configuration must beat on cost per finished task and re-work together; no routing saving is stated before that measurement on ten real sessions per harness. | A | V1 | MR-3 | 0044 |
| R25 | Routing latency at session start with julia1 or gutsy on the CPU while four agents run is recorded, and a threshold set before enabling; MR1 (the Claude Code per-call hook) and MR2 (the other sub-agent seams) are found or recorded as absent. | A | V1 | MR-1, MR-2, MR-4 | 0044 |
| R26 | An escalation checkpoint (a cheap model runs a few turns, a judge decides whether a stronger model restarts from the original task, the shape of SWE-Router and OpenRouter's Jev-verified cascade) is compatible with R02 and is a later option, not part of the first build. | A | EV | — | 0044 |

## 3. Open questions

1. The user's explicit approval of the one point that reverses an earlier decision: routing alone may use a hosted executor with a bounded redacted digest (ADR-0037 RA-004, INV-003); recorded as ADR-0031 v2 and ADR-0037 v2, Proposed.
2. Which model backs julia1 stays with the other project (token-savings 8.4).

## 4. Acceptance

MR1 to MR4 of the routing note, to be given register ids. Nothing has run; the installed Claude
Code binary was only checked for the relevant strings.
