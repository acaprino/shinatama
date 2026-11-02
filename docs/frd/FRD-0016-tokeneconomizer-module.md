# FRD-0016: TokenEconomizer as an optional module

| | |
|---|---|
| Version | 1 (new Markdown record; no JSON record yet) |
| Status | Accepted 2026-11-01 (this Markdown text; the JSON evidence record, where one exists, stays Proposed until `frd_tool.py promote`) |
| Date | 2026-11-01 |
| Sources | docs/notes/2026-11-01-tokeneconomizer-beside-billioncontext.md 1 to 10; specs/token-savings 7.2; records TE5, TE12, TE13 |
| Governing ADRs | ADR-0045 v3 |
| Register sections | TE1 to TE13 (TE5, TE12, TE13 in the register; the others proposed in the note) |
| Related records | FRD-0002 (installed per box like a Daodan plugin, state in the box), FRD-0009 (BillionContext owns the wire), FRD-0013 (admission tests, ledger), FRD-0015 (evidence as the chooser's prior) |

## 1. Purpose and scope

`microsoft/TokenEconomizer` (Apache-2.0, commit `c53a4a6`, read not run) learns across the
sessions of one repository and cuts waste no other Shinatama layer attacks: facts the sessions
keep rediscovering (micro-skills), tools never used (tool pruning) and stuck turns (loop guard).
It acts through the agent's hooks and settings, not a proxy. This record states how Shinatama
admits it beside BillionContext: optional, per instance, with two of its three mechanisms, one
adaptation, and the checks that precede enabling it on each harness.

## 2. Requirements

### 2.1 Admission and scope

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | TokenEconomizer is admitted beside BillionContext as an optional module: off by default, chosen per instance in the instance's set, because it covers Claude Code, Codex, OpenCode (experimental) and Pi, not Hermes Agent or DeepSeek Harness, and is tested on older versions than the installed ones (Claude Code 2.1.300 to 2.1.310 against 2.1.316; OpenCode 1.19.5 and 2.2.0 against 2.2.5; Pi 1.2.0 to 1.3.0 against 1.4.0; Codex 0.164.0 in range). | U 2026-11-01 ("TokenEconomizer beside BillionContext"); A (optional, per instance) | V1 | TE1, TE6 | 0045 v3 |
| R02 | It passes the admission tests of FRD-0013 R30 except test 4 for tool pruning (a denied tool comes back only through a person's `restore`), which is accepted only with the guards of R10 and R11. | A | V1 | — | 0045 v3 |
| R03 | It is never offered in the set of a Hermes Agent or DeepSeek Harness instance. | A | V1 | — | 0045 v3 |
| R04 | It adds no proxy and no compaction: BillionContext stays the only owner of the wire, and decision compaction and BillionContext's tiers the only reducers of history. | A | V1 | — | 0045 v3, 0038 |

### 2.2 Installation, state and gate

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R05 | Pinned to a commit and installed per box like a Daodan plugin, from its self-contained bundle: `--plugin-dir` on Claude Code; installed into the home and switched per launch with `-c` on Codex (in a box only, as every Codex set); on OpenCode loaded from `<OPENCODE_CONFIG_DIR>/plugins/` through a re-export wrapper, because a `plugins` array entry must be a directory and skips a file (TE13); `-e` on Pi. Updated next to the running version as the `own` program rule does. | A; S (TE5, TE13) | V1 | TE1, TE5, TE6, TE13 | 0045 v3 |
| R06 | Its state lives under a `TOKENECONOMIZER_HOME` inside the box, shared by the box's instances so evidence accumulates; the default `~/.tokeneconomizer` would write the user's home, which a box never does. | A | V1 | TE1 | 0045 v3, 0012 |
| R07 | Shinatama's launcher owns the version gate, since TokenEconomizer has none: the module is loaded only on a harness version whose checks have passed, and the instance says why when it is not. | A | V1 | TE1, TE6 | 0045 v3 |
| R08 | TokenEconomizer runs Python 3.10 or later at every hook, including every tool call: Shinatama provides the interpreter (Hermes Agent's `own` recipe already brings a bundled uv and Python) and measures the per-call hook latency on Windows with four agents running before enabling it. | A | V1 | TE3 | 0045 v3 |

### 2.3 Tool pruning: it proposes, Shinatama applies

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R09 | `toolprune=propose` is written explicitly (the default `auto` writes `.claude/settings.local.json` or `.codex/config.toml` in the repository, which the user's own runs outside the box also read). Shinatama reads the proposal from `<TOKENECONOMIZER_HOME>/repos/<key>/<host>/untool.json`, waits for the repository's idle time itself (60 minutes, the rule that exists only under `auto`), and adds the deny entries to that instance's launch only (`--disallowed-tools` or the equivalent). | A | V1 | TE1, TE2 | 0045 v3 |
| R10 | Never applied from a proposal: any entry matching `mcp__bili__*` (BillionContext's recovery tools, which no MCP tool protection covers by default) or Codex's `image_media`; `toolprune_protect=mcp__bili__*` is set; the denied list is shown at launch. | A | V1 | TE1 | 0045 v3 |
| R11 | The `apply` path is closed to the instance: the `toolprune` slash command is kept out of its set and the session-start notice that invites `toolprune apply` is not relayed. The residual path (a person running `te toolprune apply` from a terminal) is documented, not prevented. | A | V1 | TE1 | 0045 v3 |
| R12 | The tool list of a running session never changes because of TokenEconomizer: a deny list formed during a session takes effect at the next launch, after the repository's idle wait, so a resumed session's prompt cache is not rewritten. | A | V1 | TE2 | 0045 v3 |
| R13 | On OpenCode TokenEconomizer skips its pruning stage before reading anything (`fold.py:115`, `denylist.py:55,76`; verified TE12), so no proposal and no evidence come from it. Shinatama computes the evidence itself (route B) from the per-session exports TokenEconomizer writes (`opencode-sessions/<id>.json`), with TokenEconomizer's own rule (`enough_evidence`: unused over 10 prompts across at least 2 sessions, or over 5 sessions) and a protected set in OpenCode's spelling (`read`, `write`, `patch`, `task`, `question`, `skill`, `todowrite`, `webfetch`, `websearch`, `bili_*`), counting a use made through Code Mode (`state.metadata.toolCalls` of `execute`) and taking the catalog from the instance's set, since Code Mode keeps MCP tools out of the request's tool list, and writes the result as `permissions` deny rules in that instance's `OPENCODE_CONFIG`. An upstream request asks for `opencode` as a propose-only host; when it lands, its proposal replaces the computation. | U 2026-11-01 (route B now, route A upstream); S (TE12, TE13) | V1 | TE12, TE13 | 0045 v3 |

### 2.4 Loop guard

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R14 | The loop guard is on for Claude Code and Pi: after 5 turns that ended green, a turn with repeated failures goes to a judge; at confidence 0.7 the agent gets one redirect as context on the tool result (`additionalContext` on Claude Code, a steering message on Pi); still stuck, the person gets a one-line notice; it never stops a turn. | A | V1 | TE6 | 0045 v3 |
| R15 | Off on OpenCode until the redirect travels outside the tool result (an upstream request): TokenEconomizer appends it to the end of the result and CCR replaces a result of 4000 tokens or more with a placeholder that keeps only the head, so the redirect never reaches the model. | A (settled by reading) | V1 | TE9 | 0045 v3 |
| R16 | Off on Codex, and on OpenCode too, until its judge runs on julia1 through a `noul` question, once TokenEconomizer offers an optimizer endpoint beside `runner_for` (an upstream request, not a fork, as with fast-jev-compaction); until then the judge is a side call through the agent's own CLI, which on Codex carries every MCP server of the home plus `bili`. | A | V1 | TE4 | 0045 v3 |
| R17 | On Claude Code the judge's side call runs on the box's user settings, not the instance's set, so it carries the box's whole plugin inventory (Daodan preinstalled); its cost is measured in a real box (TE4) and a threshold set before the judge is enabled there. The redirect text comes from a fixed template or stays on the side call; julia1 generates no text. | A | V1 | TE4 | 0045 v3 |
| R18 | BillionContext's own receipts (`[Compression FAILED: …]`, `[acp_retrieve FAILED: …]`) and a retrieved body can count as failures for the loop guard; TE11 measures whether a retrieve raises the repeated-failure signature, and if it does the exemption of a tool-name pattern is requested upstream before the guard is enabled. | A | V1 | TE11 | 0045 v3 |

### 2.5 Micro-skills

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R19 | Micro-skills run only as `envfacts` (fixed templates from repeated failures, no model call); the proposer stays off, because its facts are open-vocabulary text derived from command output the repository controls, injected into every session, and TokenEconomizer itself says to leave it off on untrusted repositories. Overlap with Daodan's memory and the project instructions is measured before the proposer is ever turned on. | A | V1 | TE10 | 0045 v3 |
| R20 | Facts injected at session start meet `promptRewrite` (which appends a note after the human message): TE10 checks that the facts reach the model unchanged and the note does not restate them. | A | V1 | TE10 | 0045 v3 |

### 2.6 Contact points with BillionContext and the ledger

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R21 | TokenEconomizer learns from the raw sessions the harness writes, not the rewritten wire, so its facts, tool evidence and loop envelope reflect what happened; its cost ledger reads the provider's reported usage and already counts BillionContext's savings (TE7 checks). | A | V1 | TE7 | 0045 v3 |
| R22 | Its side calls ride BillionContext's path on Claude Code and Codex and appear to the proxy as a new header-less session under the anonymous profile; TE8 records whether a judge call shows as a session of its own, and if so it is labelled, not counted as user work. On Pi `--no-extensions` drops BillionContext's extension; on OpenCode `OPENCODE_CONFIG_CONTENT` against `OPENCODE_CONFIG` is unverified. | A | V1 | TE8 | 0045 v3 |
| R23 | The prompt cache: BillionContext keeps the prefix stable per launch and TokenEconomizer changes the tool list only between sessions and adds facts only at session start; TE2 measures that the cache hit rate inside a session is unchanged with the deny list on. | A | V1 | TE2 | 0045 v3 |
| R24 | Savings do not add up (fewer schemas shrink what BillionContext caches; a shorter loop leaves less for CCR): only Shinatama's ledger, with the module on and off, tells what each adds; no saving is stated for TokenEconomizer before that, and neither project's own figure (18.4 to 30.9 % on SWE-Bench, vendor-reported) is used. | A | V1 | TE7 | 0045 v3, 0038 |

## 3. Open questions

1. Route A (upstream propose-only host for OpenCode) against route B (Shinatama computes the evidence): route B now, route A as the upstream request, per the user's decision of 2026-11-01; the record is ADR-0045 v3, accepted.

## 4. Acceptance

TE1 to TE13 of the study; today TE5 and TE13 passed (one trial per case) and TE12 failed as designed
with its cause settled and replaced by TE13; the rest is open.

## 5. Relation to the JSON records

No JSON record covers this area; FRD-0013.json predates the study. A JSON record should be
produced with the tool from the note and ADR-0045 v3 when this record is reviewed.
