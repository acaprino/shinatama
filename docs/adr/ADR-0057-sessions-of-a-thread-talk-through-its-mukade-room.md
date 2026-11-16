---
adr_id: ADR-0057
version: 1
title: The sessions of a thread talk to each other through the thread's Mukade room, joined and woken by Mukade's own hooks, and the window shows and joins that conversation
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - crates/engine/src/mukade_chat.rs
    - crates/engine/src/session_control.rs
    - crates/engine/src/claude_launch.rs
    - crates/engine/src/codex_launch.rs
    - crates/state/src/database.rs
    - crates/state/src/worksets.rs
    - apps/desktop/src-tauri/src/thread_chat.rs
    - apps/desktop/src-tauri/src/thread_peers.rs
    - apps/desktop/src-tauri/src/collaboration.rs
    - apps/desktop/src-tauri/src/agents/service.rs
    - apps/desktop/src/thread-chat.ts
    - apps/desktop/src/thread-chat-model.ts
    - apps/desktop/src/workset-sidebar.ts
    - apps/desktop/src/main.ts
    - .claude/skills/mukade-collaborate
  modules:
  domains:
    - threads
    - mukade-chat
    - per-launch-injection
    - sidebar
governs_symbols:
depends_on:
  - ADR-0013
  - ADR-0020
  - ADR-0055
supersedes:
created_from_task: user-request-2026-11-16-sessions-of-a-thread-communicate-through-mukade
proposed_on: 2026-11-16
proposed_at_sha: 3d3e42318071e8ba8ca8c4883a4d3307001c9610
accepted_on:
accepted_at_sha:
---

# ADR-0057: The sessions of a thread talk to each other through the thread's Mukade room, joined and woken by Mukade's own hooks, and the window shows and joins that conversation

## Context

On 2026-11-16 the user asked that several sessions of the same thread (a workset, ADR-0055) be
able to communicate through Mukade, then asked how that is used from the interface, and chose all
three parts of the proposal: a per-thread switch with Mukade's hooks, a conversation panel with
marks, and writing as the user.

Before this record the sessions of a thread could reach each other only by hand: the agent called
`shinatama_thread_peers`, opened the room by name and joined with `chat_join`; a session read only
when it called `chat_read` or `chat_wait`, so an idle one never answered by itself, and nothing in
the window showed the conversation.

Mukade already has the missing half: its adapter (`mukade chat hook <harness> <event>`) joins a
room at the first turn, hands the model what others wrote at each turn and, after a turn, waits for
a message addressed to the member and continues the session with it (bounded by
`MUKADE_CHAT_LISTEN` and `MUKADE_CHAT_MAX_WAKES`). Its command line runs every chat operation
(`mukade chat <operation> … --json`), so the desktop needs no MCP client of its own.

## Decision

1. **A switch per thread.** "Sessions talk to each other" in the thread's settings (⋯) stores the
   thread in the `threads.chat_enabled.v1` setting. Off by default. Turning it on opens the room at
   once so a problem shows then; it changes no running session.
2. **One room per thread, by name.** `shinatama-thread-<suffix of the workset id>`. Mukade's
   `chat_open` returns the existing room for a known name, so Shinatama keeps no room registry.
3. **Mukade's hooks, composed per launch.** A Claude Code or Codex session created in, or resumed
   from, a thread that is on gets Mukade's hooks next to Shinatama's tracking hook, and the room in
   its environment (`MUKADE_CHAT_ROOM_ID`, role `peer`, listen 300 s, at most 10 wakes,
   `MUKADE_DATA_DIR`). Claude Code: three entries appended to the per-instance `settings.json`
   (session start, prompt, stop). Codex: prompt and stop, trusted the way Shinatama's own hook is
   (session-flags state key and identity hash), all in one `hooks={state=…,SessionStart=…,…}`
   override so the argv stays within the terminal host's 32-argument bound. Nothing global is
   written; no trust is bypassed. Without a reachable service or bundled program the session opens
   as an ordinary one.
4. **Sessions are created in their thread.** `agent_open` takes the thread, so the launch knows
   it; a session opened by another one (`shinatama_session_open`) is created in the caller's thread.
5. **The window reads without consuming.** It uses `rooms`, `members` and `ledger` through the
   bundled program; never `read`, `wait` or `ack`, which belong to a member. A member is tied to a
   session through Mukade's adapter record of that session's native conversation (read without its
   handle), or by the stable name a session that joined by hand uses.
6. **The user is a member.** Messages from the panel go out as the member `user` (harness
   `shinatama`). Its handle stays in memory; after a restart the member is recovered with `rejoin`.
7. **Marks.** A "Chat" button on the row of a talking thread, with a dot when its room has events
   the user has not seen (the last one seen is kept per window); "· in chat" or "· not in chat yet"
   on the tabs of its Claude Code and Codex sessions. One `rooms` call every 10 s covers all of them.

## Rejected alternatives

- **RA-001: keep manual consumption only.** An idle session never answers; the user relays by hand.
- **RA-002: Shinatama's own wake hook.** Re-implements Mukade's adapter, its batches and receipts.
- **RA-003: link Mukade's `chat` feature into Shinatama.** Pulls an async runtime, an HTTP server
  and client into the desktop's dependency set (ADR-0047) for calls the bundled program already
  makes.
- **RA-004: Codex hooks as separate `-c` pairs.** Exceeds the 32-argument bound at the fullest
  launch.
- **RA-005: write the room into Mukade's adapter state before launch.** Writes another program's
  private records and needs a native id Codex chooses itself.

## Invariants

- **INV-001**: the window never calls a consuming chat operation (`read`, `wait`, `ack`).
- **INV-002**: no member handle or service credential reaches the frontend, argv or a file Shinatama
  writes; the adapter's handle is never read.
- **INV-003**: a thread that is off launches sessions exactly as before this record.
- **INV-004**: a chat failure never prevents an ordinary session from opening.
- **INV-005**: the Codex launch with chat on uses no more arguments than without it.

## Consequences

- After each turn a listening session shows its stop hook running for up to 5 minutes; typing or
  Escape ends the wait. This is Mukade's behaviour and the price of being woken.
- Sessions running when the switch is turned on join only when resumed.
- OpenCode and Pi sessions are listed but cannot take part: Shinatama gives them no Mukade tools.
- Mukade's adapter names members itself (`claude-1`, `codex-1`); `shinatama_thread_peers` reports
  those names when the hooks joined.

## Confirmation

Not yet run on Windows with real agents. To confirm: in a thread that is on, a Claude Code and a
Codex session; Claude sends Codex a task; Codex, idle after its turn, wakes and answers; the panel
shows both messages; a message from the panel to Claude wakes it; Codex's `/hooks` lists the two
Mukade hooks as trusted; a thread that is off launches with the previous argv.
