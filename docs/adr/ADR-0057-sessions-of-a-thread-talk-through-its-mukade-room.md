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
    - crates/mukade
    - build-release.bat
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
   (session start, prompt, stop). Codex: prompt and stop, defined exactly as Mukade defines them
   (`& '<…>/mukade/mukade.exe' chat hook codex <event>`, timeouts 30 and 3630 s), next to
   Shinatama's session-start hook in one `hooks={state=…,SessionStart=…,…}` override so the argv
   stays within the terminal host's 32-argument bound. Without a reachable service or bundled
   program the session opens as an ordinary one.
3a. **Codex trusts Mukade's hooks through Mukade.** Before such a launch Shinatama runs
   `mukade chat trust codex` against the session's private Codex home: Codex's own app-server
   lists the hooks with their keys and hashes, and the approval is written to that home's
   `config.toml`, as `/hooks` would (Mukade's live record, run 8, Codex 0.167.2 on Windows). It is
   accepted only when Codex then reports all of Mukade's hooks trusted, and is checked once per
   run of Shinatama. The approval holds because the program's path never changes. Codex reads hook
   trust from the user layer and the session-flags layer and merges it key by key
   (`codex-rs/hooks/src/config_rules.rs`), so Shinatama's override for its own hook does not hide
   it; Shinatama computes no hash for Mukade's hooks, which would override the one Codex recorded.
   Nothing outside the private home is written; no trust is bypassed. A failure is logged and the
   session opens without the room.
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

8. **Mukade is embedded.** The program that runs the service, the hooks and the window's
   client is built in Shinatama's workspace (`crates/mukade`, the `chat` feature) from the Mukade
   revision the workspace pins, and shipped as `mukade\mukade.exe`. The version Shinatama runs is
   the version it was built and tested against; no Mukade checkout beside Shinatama is used.
   A test keeps the dependency, `scripts/mukade-pin.json` and `MUKADE_REVISION` on one revision.

9. **What counts as "in the room".** A session is a member through its hooks only when its
   launch in this run composed them (a launch record kept by the engine) and the adapter joined;
   the adapter's file alone outlives the run that wrote it. A session that joined by hand counts
   while it runs. If Codex's approval fails, the hooks are not composed at all. The bootstrap of a
   session opened by an agent tells it to look for the `[mukade chat]` note and to join by hand
   when there is none, since whether the hooks loaded is known only after the text is written.
10. **Reads stay reads; sends happen once.** `shinatama_thread_peers` starts no service and
   creates no room. A room's id is kept for the run once opened. The panel's first read takes the
   newest events (a widening window ending now). The unread mark counts only messages and
   signals after the event the user saw. The user's message is resent only when Mukade refused
   the handle (`stale_member`); a timeout is reported as uncertain, not repeated.

11. **OpenCode and Pi through Mukade's plugin kit.** A new OpenCode or Pi session of a talking
   thread loads Mukade's own kit, prepared by the embedded program's `shinatama-kit` command,
   which calls Mukade's `chat_launch` and starts nothing. OpenCode: the plugin and the MCP entry
   merged by Mukade into the inline configuration Shinatama already passes (`--standalone` is
   already Shinatama's). Pi: `-e <Mukade's extension>` and `-e builtin:mcp`, which Shinatama's
   `--no-extensions` otherwise leaves off. The child's environment carries the room and the
   service's credential, as Mukade's launcher sets them (Mukade's live record, run 10: OpenCode
   2.2.5 and Pi 1.4.0 on Windows join and wake). Shinatama records no native session for them, so
   their member is listed in the room but not tied to their tab: the panel says "launched with
   the room's kit", the tab " · chat". Neither has resume yet, so only new sessions join. Hermes
   is not launched by Shinatama and Mukade has no chat kit for it: it does not take part.

12. **Nothing mixes with another installation, and Marionette stays.** Shinatama only ever uses
   its own Mukade service and data folder; an inherited MUKADE_DATA_DIR is ignored. The Mukade
   processes it runs (service, chat commands, Codex's approval, kits) see none of the system's
   `MUKADE_*` or `CODEX_*` variables nor the harnesses' store variables, so they read and write no
   other installation's sessions or configuration; sessions get no system `MUKADE_*` variable.
   Kits, approvals and adapter state live in the session's private home and Shinatama's data
   folder. The chat only adds to a launch: Marionette's plugin, overrides and extension stay
   (tests in `codex_launch` and `pi_launch`).

## Rejected alternatives

- **RA-001: keep manual consumption only.** An idle session never answers; the user relays by hand.
- **RA-002: Shinatama's own wake hook.** Re-implements Mukade's adapter, its batches and receipts.
- **RA-003: link Mukade's `chat` feature into the desktop process.** Pulls an async runtime, an
  HTTP server and client into the desktop itself (ADR-0047), and ties a service other sessions
  share to the window's lifetime. The feature is linked into the separate `mukade` program instead.
- **RA-004: Codex hooks as separate `-c` pairs.** Exceeds the 32-argument bound at the fullest
  launch.
- **RA-006: Shinatama computes the trust hash of Mukade's hooks itself.** Its identity rules
  are Codex's internals; an error would silently leave the hooks untrusted, and a session-flags
  hash overrides the correct one Codex recorded. Codex computes it instead, through Mukade.
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
- OpenCode and Pi take part through Mukade's plugin kit (new sessions only, member not tied to the tab); Hermes does not take part.
- Mukade's adapter names members itself (`claude-1`, `codex-1`); `shinatama_thread_peers` reports
  those names when the hooks joined.

## Confirmation

Not yet run on Windows with real agents. To confirm: in a thread that is on, a Claude Code and a
Codex session; Claude sends Codex a task; Codex, idle after its turn, wakes and answers; the panel
shows both messages; a message from the panel to Claude wakes it; Codex's `/hooks` lists the two
Mukade hooks as trusted; a thread that is off launches with the previous argv.
