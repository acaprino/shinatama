---
adr_id: ADR-0024
version: 1
title: Peer messages are quoted untrusted data with default-deny scope, at-least-once delivery, flood limits and a four-tool MCP server
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - inter-instance-messaging
    - message-delivery
    - mcp-server
    - prompt-injection
    - instance-identity
governs_symbols:
depends_on:
  - ADR-0008
  - ADR-0009
  - ADR-0013
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0024: Peer messages are quoted untrusted data with default-deny scope, at-least-once delivery, flood limits and a four-tool MCP server

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user asked that different harness instances and models talk to
each other and compare on a topic (functional-design goal 10, decided as a wish) and for an
overview of the communication flows and exchanges between instances (goal 12). The shape of the
messaging is the author's, except its boundary, which the user decided on 2026-11-01: the line
between these messages and the non-goal of an addressed mailbox is persistence. The messages
stay; each expires after one hour by default and never later, and stays within one project
folder or roundtable, so no mailbox outlives the sessions or the expiry (functional-design 2;
technical-design 7.1, 7.2). Technical-design 7.2 and 7.5 are tagged [C], the rest of section 7
is untagged author's design, and technical-design 12 lists the limits on what agents can do
among the author's defaults, with the project-or-roundtable scope marked as the user's and the
per-project setting that broadens it as the author's. The PTY-write rule of register M5 and the server-side
depth limit of register M7 are written into technical-design 7.3 and 7.4 as the author's calls
[C]. The question is how one hosted agent reaches another without either one steering the other or
acting with the user's authority. The full design is in technical-design 7.1 to 7.7; this record
holds only the decision.

## Decision Drivers

- A message from another agent must never act as an instruction or an authorization (technical-design I6).
- No silent loss and no runaway loop between two agents.
- No daemon and no edit of the user's global configuration.
- Tool definitions cost context in every agent.
- Delivery states must be honest: Shinatama cannot know that a model read a message.

## Decision

A message has a `kind` (`note`, `ask`, `reply`, `catch_up`, `roundtable_prompt`,
`roundtable_answer`, `alert`), a body of at most 16 KB (longer content travels as a file path and
a hash), a `depth` that counts agent-to-agent replies and an `expires_at` that defaults to one
hour and can only be shorter: a message not delivered by then expires, so there is no mailbox
that outlives the sessions or the expiry (user, 2026-11-01). It is addressed to an instance id,
`agent@project` or a roundtable group. Scope is **default deny across projects**: only the same
project folder or the same roundtable (user, 2026-11-01), unless a per-project setting
broadens it.

Delivery tries, per recipient capability: the agent's native queue, then hook injection through
`shinatama hook`, then MCP pull through `shinatama_inbox`, and last a PTY bracketed paste, only
when the instance is idle and the user has typed nothing in it for 10 s, and never while a
permission dialog, a selection, a search box or a draft is active, so that a PTY write can never
answer a permission prompt; otherwise that channel is removed. States are `queued`,
`delivering`, `delivered`, `acked`, `failed` and `expired`; `delivered` is never shown as read.
Delivery is at least once (at most 5 retries, backoff 5 s ×2), receivers are idempotent by
message id, and nothing promises exactly once. Limits: depth 6, enforced by the server even when
the agents do not carry `in_reply_to`; 6 messages per minute per sender-recipient pair; 40 per
hour per thread. A tripped limit pauses agent-to-agent
auto-replies and raises an `alert`; nothing is dropped silently.

`shinatama mcp` is a stdio subcommand of the same binary, injected per launch, with four tools:
`shinatama_instances`, `shinatama_send`, `shinatama_inbox` (quoted data, oldest first, capped at
8 KB) and `shinatama_ack`. It identifies the caller by `SHINATAMA_INSTANCE` plus a per-launch
token stored hashed, and refuses a mismatch. Messages reach the model wrapped as
`<peer-message from="…" id="…" untrusted="true">`; they never answer an approval request or count
as the user's reply, and secrets are redacted before storage and delivery. Agents cannot hop,
start instances, start roundtables, read another instance's transcript or change settings. See
technical-design 7.1 to 7.7.

## Verified Interview Examples

- **EX-001** (nominal case): Input: a message to an offline recipient -> Expected: queued, then delivered when the recipient starts within the expiry; a recipient that has not started by `expires_at` never receives it, and the message is `expired` (technical-design 7.1, 11.7).
- **EX-002** (boundary case): Input: the same message sent twice -> Expected: one delivery (technical-design 11.7; register M10).
- **EX-003** (boundary case): Input: a two-agent ping-pong, also when the agents do not carry `in_reply_to` -> Expected: it stops at depth 6 with an alert (technical-design 11.7; register M7).
- **EX-004** (rejection or error case): Input: a send to an instance of another project, with the default scope -> Expected: refused (technical-design 11.7; register M13).
- **EX-005** (rejection or error case): Input: a hosted agent's shell tool sets a peer's `SHINATAMA_INSTANCE` and calls `shinatama mcp` or `shinatama hook` without the peer's token -> Expected: refused and logged; the agent's own calls still work; the token never appears in clear in the database, in logs or in a transcript (register M15).

## Rejected Alternatives

- **RA-001**: offering `shinatama_roundtable` to agents; only the user starts a roundtable, because it spends money.
- **RA-002**: writing into the PTY as a regular channel; it can collide with a draft, so it is the last resort, and the channel is removed if a write could ever answer a permission prompt (register M5).

## Enforceable Invariants

- **INV-001**: a peer message reaches a model only inside the `untrusted="true"` wrapper, never answers an approval request and never counts as the user's reply (technical-design I6).
- **INV-002**: with default settings an instance can message only instances of its own project folder or its own roundtable (decided by the user 2026-11-01; technical-design 7.2).
- **INV-003**: `delivered` is never displayed as "read".
- **INV-004**: when a depth, pair or thread limit trips, agent-to-agent auto-replies pause and an `alert` is raised; no message is dropped silently; messages between two agents never trigger a tool call or an approval by themselves.
- **INV-005**: `message.dedupe_key` is unique and receivers are idempotent by message id.
- **INV-006**: `shinatama mcp` and `shinatama hook` refuse a caller whose token does not match the instance's stored token hash.
- **INV-007**: an agent's only Shinatama tools are the four; it cannot hop, start instances or roundtables, read another instance's transcript or change settings.
- **INV-008**: a PTY write happens only when the recipient is idle and the user has typed nothing in it for 10 s, and never while a permission dialog, a selection, a search box or a draft is active, so that a PTY write never answers a permission prompt; where that cannot be guaranteed the channel is removed (author's call; technical-design 7.3; register M5).
- **INV-009**: secrets are redacted from message bodies before storage and delivery (technical-design I5).
- **INV-010**: the depth limit is enforced by the server, so it holds even when the agents do not carry `in_reply_to` (author's call; technical-design 7.4; register M7).
- **INV-011**: no message's `expires_at` is more than one hour after it was sent; a message not delivered by then becomes `expired` and is never delivered afterwards, so no mailbox outlives the expiry (decided by the user 2026-11-01; functional-design 2; technical-design 7.1).

## Consequences

- No channel is proved in practice for any agent: technical-design 7.3 tags native queues and hook injection [U] or [R-S] (`codex queue` is [V] only for existing), what reaches an idle agent is register M4 and milestone item 6, and per-launch MCP injection is a G0 gate; an agent where it fails is marked "no MCP" (register M1).
- Hermes Agent has no per-launch switch: its hook and MCP entries live in the box's `config.yaml`, so a Hermes on the `system` home runs without them; DeepSeek Harness needs another way to receive the token (technical-design 3, 5.7; register DS5).
- The token limits impersonation between instances; it is not isolation from an agent with a shell (technical-design 7.5).
- Message bodies are stored in the database, which is sensitive; `shinatama forget` deletes them (technical-design 4.7).
- ADR-0002's companion reuses the untrusted wrapper and asks technical-design 7.6 to add escaping of the wrapper's content, which this design does not state yet.
- ADR-0025 keeps these four tools plain in v1; ADR-0026 builds the roundtable on the same quoting rules.
- Open: the measured injection rate per agent (register M6); if it is not zero, tools stay denied for unattended delivery. A `shinatama_handover_request` is possible later and is not in v1.

## Confirmation

- Register M1 to M15 pass; all are open.
- Acceptance test 11.7 passes: offline recipient, duplicate send, ping-pong and cross-project send.
- Nothing has been run yet.
