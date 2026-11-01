# FRD-0013: Token savings

| | |
|---|---|
| Version | 1 (Markdown record; JSON record version 1 at `c84d28e`) |
| Status | Accepted 2026-11-01 (this Markdown text; the JSON evidence record, where one exists, stays Proposed until `frd_tool.py promote`) |
| Date | 2026-11-01 |
| Sources | specs/token-savings 1 to 9; technical-design 7.5a, 8.4, 9; functional-design 3 (no claimed saving); record T5 |
| Governing ADRs | ADR-0025, ADR-0037, ADR-0038 |
| Register sections | T1 to T9, M3 |
| Related records | FRD-0009 (baseline configuration, single source), FRD-0010 (shared local runtime), FRD-0015 (routing and the tool chooser), FRD-0016 (TokenEconomizer), FRD-0007 (code mode) |

## 1. Purpose and scope

Shinatama cuts the tokens the hosted harnesses send to their frontier models without lowering
the quality of the work, and does the bookkeeping (scoring, choosing, digesting) on a small
local CPU model, **julia1**. This record states the technique order, decision compaction and
its executor, the per-harness adapters and their four rules, the layer-ownership test for
other tools, the ledger, and the order of work. The sidecar's baseline configuration is in
FRD-0009 2.6; model routing and the tool chooser are FRD-0015; TokenEconomizer is FRD-0016.

## 2. Requirements

### 2.1 Goals and limits

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Every token-saving technique that exists is applied as long as none lowers quality; the bookkeeping runs on a local small model, not a paid frontier model. | U 2026-10-31 | V1 | T-rows | 0037 |
| R02 | Techniques apply in the order of their returns: prevent tokens from entering, then remove them reversibly, then cut lossily, never the reverse; lossy techniques are never stacked so their losses add up. | A | V1 | — | 0038 |
| R03 | A compaction never drops the file path, exact error, command or constraint the next turn needs; re-running a tool is acceptable, losing a decision is not. | S (anti-goal) | V1 | 9.2 item 2 | 0037 |
| R04 | No technique breaks the prompt cache by rewriting early messages or reordering tools per turn; Shinatama keeps the sidecar and the MCP set stable per launch and never restarts MCP servers mid-session. | S | V1 | T4 | 0038 |
| R05 | No saving percentage is stated anywhere, vendor figures included, before it is measured on the user's own sessions; vendor figures are labelled as not measured. | U | X | 9.2 item 1 | 0038, 0010 |
| R06 | Text of the user or the assistant is never rewritten; it is never a candidate. | U | V1 | — | 0037 |
| R07 | Picking, training, calibrating, benchmarking the model behind julia1 and setting its threshold are out of scope: Shinatama fixes only the wire contract, what it guarantees the executor, the precomputed signals and how it acts on the answer. Daodan is not changed. | U 2026-10-31 | — | — | 0037 |

### 2.2 Where tokens go and the catalog

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R08 | Static prefix: a stable, append-only prefix (harness); deferred tools and tool search (harness); progressive disclosure of skills (Daodan, on the hosts it builds for); MCP pruning per instance through the per-instance set, with the mounted list visible at launch (Shinatama). | S | V1 | D19 | 0038 |
| R09 | Tool results: code mode where native (harness); offload to engines (Daodan, Mukade); store and placeholder (BillionContext CCR); Absorb (sidecar); query discipline (harness prompts, Daodan rules); per-tool digests (Shinatama, julia1). | S | V1 | — | 0038 |
| R10 | Per-tool digests: for results with a known shape (test runners, build output, listings, diffs) a parser keeps the signal and julia1 extracts the rest to a fixed output schema; the verbatim original stays recoverable; they run asynchronously after `tool.call`, never on the model's turn, starting with tools whose output has a stable shape. | S | V1 (milestone 5) | 9.2 item 3 | 0037 |
| R11 | History: reversible tiered compression, cache-aware (sidecar); decision compaction (Shinatama through the adapter); context editing at the API where available (harness); monolithic summary compaction only as fallback; externalized state (Daodan, the thread store); "last N turns only" never on its own. | S | V1 | — | 0037, 0038 |
| R12 | Output and architecture: effort per step (Daodan's routing skill); isolated sub-agents (harness); bookkeeping on julia1 with titles on the user's chosen backend (FRD-0010); a per-session ledger. | S; U 2026-11-01 (titles) | V1 | — | 0037 |

### 2.3 Decision compaction

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R13 | Decision compaction replaces summary compaction as the primary method, as `tamaratran/fast-jev-compaction` (commit `e3f262a`, read not run) does: every `tool_use` is paired with its `tool_result`; calls in the first message and the newest `preserveRecentMessages` (6) are pinned; for each non-pinned call two `noul` questions (does the call still matter; must the result stay verbatim); against `keepThreshold` the pair is kept verbatim, the result truncated to its head plus a note, or both removed; no result is left without its call; all three outcomes are kept. | U (by pointing at the library; truncate kept 2026-11-01) | V1 | T11, T13 | 0037 |
| R14 | It fires at `compactAtPercent` (60) of the window on `turn.complete`, or on an intercepted native compaction, and only applies decisions already made; the percentage trigger belongs to decision compaction. Summary compaction stays as the fallback when decision compaction cannot remove enough (and inside the library when the reduction is under `minReductionRatio` 0.25 or the scorer fails), with a structured prompt (decisions, open items, files touched) and firing at task boundaries. | S | V1 | 9.2 item 5 | 0037 |
| R15 | Safety with a weak scorer: the only possible error is removing a result still needed, and the assistant re-runs the tool; constraints in text always survive; this is what makes a small local model acceptable as a scorer where it would not be as a summarizer. | S | V1 | 9.2 item 2 | 0037 |
| R16 | The seam is `JevAsker.ask(state, questions) → { answers }` in the System One wire format (`noul`, `choice`, `score`); the plugin needs no change beyond `baseUrl`, which `hooks/fast-jev.ts` does not pass today, so an upstream request or plugin option is needed, never a fork. | A | V1 | 9.2 item 4 | 0037 |

### 2.4 julia1, the local executor

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R17 | julia1 is reached through one local HTTP endpoint speaking the System One wire format; it is a jev-like selector model of the kind of gutsy and the OpenJev models; model, quantization, runtime and thresholds are the other project's. | A (wire); U (local executor) | V1 | 9.2 item 3 | 0037 |
| R18 | Shinatama guarantees the executor an append-only incremental state (the same prefix plus a delta, scored after each turn, not the whole history at compaction time), a tunable `maxStateTokens`, precomputed features travelling with each question, and no request awaited by the frontier model's turn. | A | V1 | 9.2 item 3 | 0037 |
| R19 | Deterministic features computed without a model: cited later; file edited after the read; same call repeated later; error then resolved; per-tool prior. Rules decide first where conclusive; the executor is asked only about the rest; each decision records which stage produced it. | A | V1 | — | 0037 |
| R20 | `keepThreshold` is a setting whose value comes with the model's delivery; the ledger records the rate of tool re-runs after compaction, which only the harness side can observe. | S | V1 | 9.2 item 2 | 0037 |
| R21 | One julia1 sidecar per box: an HTTP process on localhost launched and supervised like the BillionContext sidecar, private root, no credentials, no system change; it shares the llama.cpp runtime with the local title model, with one opt-in download when one model serves both. | S; U 2026-11-01 (shared runtime) | V1 | I6 | 0037 |
| R22 | The tool chooser of the original design (rank deferred tools on `prompt.submit`, shorten descriptions of unlikely tools through `tool.describe`) is now the same configurable selector as the model router (FRD-0015); BillionContext's recovery tools are never candidates. | U 2026-11-01 | V1 | MR-4 | 0044 |

### 2.5 One adapter per harness

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R23 | The core is shared (scoring library, features, julia1 endpoint, ledger); each harness has its own adapter on the seam it offers and verified on its installed version: Claude Code the `session.compact` function hook where the installed version has function hooks (early access, `CLAUDE_CODE_ENABLE_FUNCTION_HOOKS=1`, 2.1.274+), else the base-URL route; Codex and Hermes Agent on the wire inside the request path they already take; OpenCode inside its plugin; Pi inside its extension; DeepSeek Harness inside the dsh plugin of the box's ACP profile. | U 2026-10-31 | V1 | 9.2 items 4, 5 | 0038 |
| R24 | Rule 1, one owner of compaction: when the adapter compacts, the harness's native compaction is stood down or intercepted; two compactions on one history destroy each other's substrate. The Hermes Agent adapter is not built until the setting or hook that stands its native compaction down is known. | A | V1 | 9.2 items 5, 6 | 0038 |
| R25 | Rule 2, one owner of the wire: where BillionContext sits on the request path the adapter acts inside that path, never as a second proxy; its code goes into the sidecar's single source and Shinatama inherits it, with no fork or patch inside Shinatama. | U 2026-11-01 | V1 | — | 0038, 0029 |
| R26 | Rule 3, cache-safe: no rewrite inside the cached prefix; compaction at the boundary the sidecar's break-even indicates; the cache hit rate is unchanged. | A | V1 | 9.2 item 5 | 0038 |
| R27 | Rule 4, verified per version: a conformance test against the real installed harness and a version gate, like the other adapters. | A | V1 | — | 0038 |
| R28 | Hermes Agent and DeepSeek Harness adapters come only after their proxy coverage is verified, since they ride that path. | A | V1 | HM12, DS12 | 0038 |

### 2.6 Layer ownership and admission of other tools

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R29 | BillionContext owns the request path and every reversible reduction of what is already in one session's context: large tool results (CCR), a result just read (Absorb), history ranges (tiers, with decision compaction inside the same path), the human message (`promptRewrite`), model output (output steering), images (media fold, byte-space shed, rescale, recovery); the static prefix belongs to the harness and Daodan. | A | X | — | 0038 |
| R30 | Another token-saving tool is admitted only if it passes five tests: it attacks a content kind no layer attacks; it is not a second proxy; it does not rewrite the cached prefix on its own schedule; what it removes comes back through a mechanism the model already knows (`acp_retrieve`, `decompress`); it works on every hosted harness or is optional, off by default, per instance. | A | X | — | 0038, 0045 |
| R31 | `chipfighter/context-attachment-manager` was evaluated and not admitted (overlap with BillionContext's media fold, a second owner of the wire, reach limited to desktop apps, scope limited to image blocks, byte figures not tokens). Where it is right (image weight in render-and-review loops) the fix is tuning BillionContext's fold in its own path and borrowing the idea of a searchable image index; whether the fold fires early enough is measured by T8. | A | EV | T8 | 0038 |
| R32 | `microsoft/TokenEconomizer` was evaluated and admitted as an optional module under FRD-0016. | U 2026-11-01 | V1 | TE-rows | 0045 |

### 2.7 Output steering and the effort lever

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R33 | The verbosity lever is cache-neutral by construction (one frozen directive at the tail of the system prompt) and is the only output lever on; T6 confirms it per harness. The effort lever is locked off on every harness, provider and model (FRD-0009 R28); T5 and T7 remain the evidence a future decision would need, and the cache-safe mechanisms of T7 (per-message effort on Anthropic, `configuration_update` on OpenAI) are the only routes to lift the lock. | U 2026-11-01 | V1 | T5, T6, T7 | 0038 |
| R34 | Measured on Claude Code 2.1.316 (T5): a top-level effort change kept the cache on Sonnet 5.5, Opus 5.5 and Fable 5.1 and invalidated the whole message cache on Haiku 5.5 (2 of 2); for OpenAI, Gemini and DeepSeek nothing is measured. The asymmetry (a miss rewrites the prefix at full price, the saving is a few hundred reasoning tokens) is the reason for the lock. | S | — | T5 | 0038 |

### 2.8 Ledger and order of work

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R35 | A ledger per session records input tokens per turn, cache hit rate, compaction count and tool re-runs after compaction; without it no technique can be said to save anything. TokenEconomizer's `ledger.jsonl` and any vendor figure are read as one more source, never as the authority. | S | V1 (milestone 1) | 9.2 item 1 | 0038 |
| R36 | Order: measure the baseline on the user's current sessions with no technique added; Claude Code with a local endpoint answering with the rules alone, measured; julia1 behind the endpoint, measured against the previous step; the other adapters one at a time; tool chooser and digests; then the code-mode gateway as an evaluation only. | A | V1 | 9.2 | 0038 |
| R37 | A code-mode gateway in front of MCP servers is not in v1: evaluated after T1 (tool-definition tokens per agent) and built only past the threshold the user sets with no fall in task success (T3). Where code mode is used direct calls stay available, and a session that needs both keeps tool search and programmatic calling. | U 2026-11-01 | EV | T1, T2, T3 | 0025 |
| R38 | Before anything is claimed: the ledger on at least ten real sessions per harness; the re-run rate before and after each milestone; the latency of one incremental scoring pass on the target CPU while four agents run; that the function-hook plugin loads on the installed Claude Code and `session.compact` receives main and sub-agent transcripts; for each adapter that the cache hit rate is unchanged and native compaction does not run behind it; for Hermes Agent, what stands its compaction down. Register rows T10 to T16 (added 2026-11-01). | S | V1 | T10 to T16 | 0010, 0038 |
| R39 | Verification records go under `docs/verification/` and the register, as for the other designs. | S | X | — | 0010 |
| R40 | Open editorial items of the inconsistency worklist on this spec (#10, #84, #106, #107, #108, #114) are closed before the record is promoted. | S | — | — | 0050 |

## 3. Open questions

None in the specification (8.3). The proxy topology (Q7) and the register rows for 9.2 (#79)
are gaps that bear on it.

## 4. Acceptance

The six checks of token-savings 9.2, T1 to T9, TE5 and TE12 (passed and failed as designed).
Nothing of the design has run beyond T5, TE5 and TE12.

## 5. Relation to the JSON record

FRD-0013.json holds 47 extracted sentences; this record covers them all, moves the sidecar
baseline to FRD-0009 2.6, and refers routing, the tool chooser and TokenEconomizer to
FRD-0015 and FRD-0016, which the JSON set does not have.
