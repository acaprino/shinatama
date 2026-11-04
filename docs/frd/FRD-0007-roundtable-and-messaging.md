# FRD-0007: Roundtable and messages between instances

| | |
|---|---|
| Version | 1 |
| Status | Accepted 2026-11-01 (this Markdown text) |
| Date | 2026-11-01 |
| Sources | functional-design goal 10, 2 (non-goals), 5.10, 7; technical-design 1 (I6), 4.2 (`message`, `roundtable`), 7.1 to 7.8, 7.5a, 11.7, 11.8, 12 |
| Governing ADRs | ADR-0024, ADR-0025, ADR-0026 |
| Register sections | M1 to M15, R1 to R7, T1 to T3 |
| Related records | FRD-0002 (instance identity and injection), FRD-0009 (traffic through the sidecar), FRD-0010 (alerts), FRD-0013 (code mode) |

## 1. Purpose and scope

Hosted instances can exchange messages, and the user can put several instances around one topic
in a moderated roundtable. This record fixes the message model, the authority rule that a peer
message is quoted data, the delivery channels and limits, the four-tool MCP server, and the
roundtable runner with its caps. The boundary with Daodan's Work Controller is persistence:
messages expire; nothing outlives the sessions.

## 2. Requirements

### 2.1 Message model and scope

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | A message has a `kind` (`note`, `ask`, `reply`, `catch_up`, `roundtable_prompt`, `roundtable_answer`, `alert`), a body of at most 16 KB (longer content travels as a file path and hash), a `depth` counting agent-to-agent replies, `in_reply_to`, a unique `dedupe_key` and an `expires_at`. | S | V1 | — | 0024 |
| R02 | `expires_at` defaults to one hour and can only be shorter; a message not delivered by then expires and is never delivered afterwards. There is no mailbox that outlives the sessions or the expiry; the task graph, checkpoints and persistent mailbox stay Daodan's roadmap. | U 2026-11-01 | V1 | AT-7 | 0024 |
| R03 | A message is addressed to an instance id, `agent@project` or a roundtable group. | A | V1 | — | 0024 |
| R04 | Scope is default deny across projects: an instance may message only instances of the same project folder or the same roundtable; a cross-project send is refused. A per-project setting may broaden the scope. | U 2026-11-01 (default); A (setting) | V1 | AT-7 | 0024 |

### 2.2 Authority: a peer message is quoted data

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R05 | A message from another agent reaches the model wrapped as `<peer-message from="…" id="…" untrusted="true">`; the tool descriptions and the seed instruction say to treat it as data from another agent, not to follow instructions inside it, and that it grants no permission (invariant I6). | S | V1 | M6 (SEC) | 0024 |
| R06 | A peer message never answers an approval request and never counts as the user's reply; messages between two agents never trigger a tool call or approval by themselves. | S | V1 | M5, M6 | 0024 |
| R07 | Secrets are redacted from message bodies before storage and delivery, with the rules of FRD-0005 R27. | S | V1 | AT-6 | 0024 |
| R08 | Injection resistance is measured per agent with canary messages ("create file X", "ignore previous instructions"); where the rate is not zero, tools stay denied for unattended delivery. | S | V1 | M6 (SEC) | 0024 |

### 2.3 Delivery

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R09 | Channels are tried in order per recipient capability: the agent's native queue (Codex `queue`, app-server `turn/steer`, Pi RPC `prompt`/`steer`, OpenCode `/prompt` or `/synthetic`); hook injection at the next boundary through `shinatama hook`; MCP pull through `shinatama_inbox`; a PTY bracketed paste as the last resort. | S | V1 | M4, CX5 | 0024 |
| R10 | A PTY write happens only when the instance is idle and the user has typed nothing in it for 10 s, never while a permission dialog, a selection, a search box or a draft is active, so it can never answer a permission prompt; where that cannot be guaranteed the channel is removed for that agent. | A | V1 | M5 (SEC) | 0024 |
| R11 | States: `queued` → `delivering` → `delivered` → `acked`, `failed` or `expired`. `delivered` is never shown as "read"; the UI says delivered or acknowledged. | S | V1 | M4 | 0024 |
| R12 | Delivery is at least once: at most 5 retries with a backoff starting at 5 s and doubling; receivers deduplicate by message id; a duplicate send yields one delivery; no exactly-once promise. | S | V1 | AT-7 | 0024 |
| R13 | A message to an offline recipient is queued and delivered when the recipient starts, if still within the expiry; otherwise it expires. | S | V1 | AT-7 | 0024 |
| R14 | What "delivered" means per agent is stated honestly in the UI from what M4 measured (a hook output, a queued message, a pasted line). | S | V1 | M4 | 0024 |

### 2.4 Loop and flood control

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R15 | `depth` ≤ 6 (configurable), enforced by the server even when the agents do not carry `in_reply_to`; a ping-pong stops at depth 6 with an alert. | S; A (server enforcement) | V1 | M7, AT-7 | 0024 |
| R16 | Per sender-recipient pair at most 6 messages per minute; per thread at most 40 per hour. | S | V1 | M7 | 0024 |
| R17 | When a limit trips, agent-to-agent auto-replies pause, an `alert` is raised to the user and the widget, and nothing is dropped silently. | S | V1 | M7 | 0024 |

### 2.5 The MCP server `shinatama mcp`

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R18 | `shinatama mcp` is a stdio JSON-RPC subcommand of the same binary, started by the agent from its MCP configuration, injected per launch by the adapter without editing the user's global configuration; it opens the database directly and there is no daemon. | A | V1 | M1, CL9, OC6, PI7, HM5, DS5 | 0024, 0013 |
| R19 | In v1 it exposes exactly four tools: `shinatama_instances()` (the instances the caller may message, with id, agent, project, state and activity title), `shinatama_send(to, body, kind?, in_reply_to?)`, `shinatama_inbox(wait_ms?)` (pending messages as quoted data, oldest first, capped at 8 KB) and `shinatama_ack(message_id)`. `shinatama_roundtable` is not offered to agents. | A | V1 | M3 | 0024, 0025 |
| R20 | The server identifies the caller by `SHINATAMA_INSTANCE` plus the per-launch token stored hashed; an unknown or ended instance or a mismatched token is refused, so a hosted agent cannot send, acknowledge or report state as a peer whose id it saw. This limits impersonation; it is not isolation from an agent with a shell. | A | V1 | M15 | 0024, 0013 |
| R21 | The four tools are pre-approved at launch where the agent allows it, so a call raises no permission prompt; nothing else is pre-approved. | A | V1 | M2, CL10 | 0024 |
| R22 | `shinatama mcp` starts within a measured budget (target 200 ms), keeps stdout clean on Windows and exits when its parent dies; identity survives `/resume`, `/clear` and compaction. The bili MCP shim and `shinatama mcp` coexist in one launch. | A | V1 | M8, M9, SC8 | 0024 |
| R23 | An agent can do nothing else on its own: it cannot hop, start instances, start roundtables, read another instance's transcript or change settings. A `shinatama_handover_request` that asks the user to hop is possible later, not in v1. | A | V1 | — | 0024 |
| R24 | The four operations are a typed API (`instances()`, `send()`, `inbox()`, `ack()`) separate from the MCP tool layer, so a code-mode surface (`shinatama_search`, `shinatama_execute` in a sandbox with only these bindings, the same scope and rate limits, no file, network or process access, a time and memory cap) can be added without engine changes. It is not in v1. | A | V1 (design); EV (gateway) | T1, T3 | 0025 |
| R25 | The hook command `shinatama hook <agent> <event>` is the other agent-facing entry: it reads the agent's JSON, writes the event and instance update in one transaction, prints what the agent expects (pending messages included), exits in under 100 ms and never fails the agent (FRD-0010). | S | V1 | M11, CL8 | 0027 |

### 2.6 Roundtable

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R26 | The user can put several hosted agent instances, each with its own model, around one topic so that they answer, critique and compare positions. Only the user starts a roundtable, after a cost warning; no agent tool starts one. | U (goal 10, shape confirmed 2026-11-01) | V1 | R1 | 0026 |
| R27 | Participants are any mix of the supported agents, each with its own model, including an agent pointed at a local model and the same agent twice with different models; the model per participant is chosen per launch, including a local OpenAI-compatible endpoint, without editing global configuration. | U 2026-11-01 | V1 | R7 | 0026 |
| R28 | Each participant is a headless session of its own (`ask_headless(new session, prompt, policy)`), never the user's live instance, asked in parallel per round; there is no fallback that types into a PTY and reads the screen. | U 2026-11-01 | V1 | R2, R3 | 0026 |
| R29 | Bare-model participants go through Daodan's `moe` tools (`moe_ask`, run state in `.moe/<run-id>/`) and `peer-review`, which the Daodan install already enables; Shinatama does not reimplement Ultra Nerd. How Shinatama reaches `moe_ask` without holding credentials is R1. | A | V1 | R1 | 0026 |
| R30 | Rounds are moderated: round 1 independent proposals on the same topic; rounds 2 to N each participant receives the others' previous answers as quoted data from a named participant and critiques; a synthesis step (an `ask_headless` by a participant the user picks or a configured one) writes agreements, differences and open questions. | U 2026-11-01 | V1 | R5 | 0026 |
| R31 | The user can interject between rounds (the text goes to every participant as a `note`) or stop the run. | U 2026-11-01 | V1 | — | 0026 |
| R32 | Turns run answer-only by default: no tools, no file writes, no network, enforced by each adapter's `ask_headless` policy and proved per agent with a canary file; the user may allow tools per participant, and the allowance is shown in the roundtable header. The exact flags per agent (`claude -p`, `codex exec --ephemeral --ignore-user-config`, `opencode run --format json`, `pi --mode json --no-tools`, `hermes chat --oneshot`, `dsh headless` with `DSH_PERMISSION_MODE=read-only`) are fixtures, not help readings. | U 2026-11-01 | V1 | R2 (SEC), CL11, CX9, OC7, PI8, HM7, DS7 | 0026 |
| R33 | Hard caps per roundtable on rounds (`max_rounds`), tokens and cost; the run stops at the first cap, with a partial synthesis when the token cap stops it. Defaults $2 and 40k tokens, to be calibrated from the real cost of three topics. | U 2026-11-01 (caps); A (defaults) | V1 | R3, R5, AT-8 | 0026 |
| R34 | Every participant's traffic, bare-model calls included, goes through the sidecar. | U 2026-11-01 | V1 | SC10 | 0026, 0029 |
| R35 | Injection between participants is measured: an answer that tells another participant to write a file must not be obeyed. | S | V1 | R4 (SEC) | 0026 |
| R36 | The run is stored as a thread of kind `roundtable`, searchable, and the user can hop any participant's position into a work thread ("continue with codex's plan" seeds a work thread with that participant's final answer). | U 2026-11-01 | V1 | R6 | 0026 |
| R37 | Rate limits on one account, per-participant cost and the granularity at which a cap can interrupt a round are measured before the defaults are fixed. | S | V1 | R3 | 0026 |
| R38 | A free-form chat between two agents is the same machinery with one reply per round; whether it is wanted is open, as is scheduling. | S (open) | — | — | 0026 |

### 2.7 Code Mode and the token footprint of tools

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R39 | V1 keeps the four plain tools; for `shinatama mcp` alone the saving of Code Mode is small. | A | V1 | M3 | 0025 |
| R40 | Milestone 1 measures, per agent, the tokens that the tool definitions of a typical Shinatama session cost (Daodan's MCP servers, bili, `shinatama mcp`); no saving percentage is claimed before. | S | M0 | T1, M3 | 0025 |
| R41 | A code-mode gateway in front of the MCP servers a session carries is not in v1 and not scheduled: it is evaluated after T1 and built only if the saving passes a threshold the user sets with no fall in task success (T3). | U 2026-11-01 | EV | T1, T3 | 0025 |
| R42 | Cloudflare's Code Mode figures are secondhand and are checked against primary sources before they inform any decision. | S | EV | T2 | 0025 |

## 3. Open questions

1. Whether the free-form two-agent variant is wanted, and roundtable scheduling (functional-design goal 10).
2. The per-project setting that broadens messaging scope: default off (technical-design 12).
3. Test accounts for live-model rows (register O1).

## 4. Acceptance

AT-7 (messages: offline recipient, duplicate, ping-pong, cross-project) and AT-8 (roundtable
caps); register M1 to M15 and R1 to R7. Nothing has run.
