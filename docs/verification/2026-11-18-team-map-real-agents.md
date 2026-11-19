# Team map with real agents (plan tasks 9, 15 step 5, 16 step 6)

Status: NOT RUN. Everything below needs the owner's installation (their Claude Code, Codex, OpenCode and Pi
sign-ins, and the model `janura-qwen`), which the agent that wrote the code could not use. The code was
tested with unit tests, the native smoke test against a real window with the team commands stubbed, and the
real programs' command-line arguments (see `2026-11-17-opencode-pi-session-id.md`). Mukade's `host-credential`,
`join --host`, `set-role`, `retire` and `rejoin --host` have been exercised only by Mukade's own tests, never
by Shinatama's announcer against a live service.

Fill the right-hand column as it happened, including what failed. Do not edit the left.

## 0. Setup

| Step | Result |
|---|---|
| Build and copy the executables over `D:\Software\Shinatama` in place (no backup; close Shinatama first) | |
| Run ADR-0057's confirmation (a thread that is on, one session of each program joins its room) | |
| Mukade release in the pin: `crates/mukade/tests/pin.rs` passes; `mukade chat rejoin --host` exists | |

## 1. Task 9: one Claude Code and one OpenCode session

A thread whose "Sessions talk to each other" switch is on; a Claude Code session and an OpenCode session
whose model is `janura-qwen`, both opened by hand.

| Step | Result |
|---|---|
| Open the Team map; put both sessions on it | |
| Claude gives work to OpenCode, "at every step"; Apply | |
| Text each session received (paste it); the heading says `From the user, through shinatama:` | |
| Tell Claude only "have the implementer run docs/plans/<a small plan>"; it reaches OpenCode | |
| Wake counts before and after (Conversation panel) | |
| Add a Codex and a Pi session; Apply again; each was told once | |
| A third session not concerned stays asleep on a later change (wake count unchanged) | |
| Draw `observes` from Codex to Claude; Apply: Codex is refused `chat_send`; `chat_observe` of Claude works | |
| Make OpenCode "only done / blocked": it answers with a signal; Claude's role in the room is `lead` | |
| Restart Shinatama and resume both: neither is told again. Change one word, Apply: both ends told once | |
| An agent joins the room by hand under the name `shinatama` before the first Apply: the host takes another name; the map says so | |
| An agent calls `chat_rejoin` on the member `shinatama`: it is refused (`not_host`) | |

## 2. Task 15 step 5: Apply opens sessions

In an empty thread of another project, use a template whose agents are Claude Code (own model) and OpenCode on
`janura-qwen`; Apply.

| Step | Result |
|---|---|
| Both tabs open | |
| OpenCode lists and uses `janura-qwen` although the Models panel assigns it something else | |
| Each session states its role unprompted after its first message ("You are …" starts the first message: OpenCode accepts it) | |
| The Conversation panel ties each member to its tab | |
| The same with a Pi and a Codex agent | |
| Stop the Mukade service, Apply: nothing is opened, the card says why | |
| Force a launch failure (a model removed from the Models panel): the opening stops, the card says why, "Open 1 session and apply" shows again | |
| Restart Shinatama between "Apply" and the first message of a session | |

## 3. Task 16 step 6: an agent opens OpenCode and Pi

In a talking thread, tell a Claude Code session "open an OpenCode session and have it run <a small plan>".

| Step | Result |
|---|---|
| The OpenCode session opens in the thread (not on the map) | |
| READY arrives; the task is delivered; RESULT comes back | |
| The same for Pi | |
| In a thread whose switch is off, the request fails with the sentence about turning it on | |

## 4. What failed

(as it failed)

## 5. Decisions

ADR-0058 is `Proposed`. Set it `Accepted` only if the owner approves its text. The handle of the member
`shinatama` goes to `mukade chat` on the command line (ledger ruling 13): accept it, or ask for a Mukade
change (handle on standard input or in the environment) and a new release.
