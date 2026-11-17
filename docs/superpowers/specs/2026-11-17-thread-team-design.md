# Thread team: who is in a thread, in what role, and telling the sessions

Design, 2026-11-17. Status: awaiting the owner's approval. Builds on ADR-0055 (a thread is a set
of sessions) and ADR-0057 (the sessions of a thread talk through its Mukade room). The
implementation needs a new ADR (next free number: 0058).

## Intent

User, 2026-11-17: the way sessions talk to each other is "goffa, inutilizzabile, macchinosa".
Two things they could not do in one sentence: have a Claude Code (Fable) session hand an
implementation plan to an OpenCode session running `janura-qwen`, and have a Codex session
explain what a Claude session is doing. They asked for a graphical way to set "la tassonomia
delle istanze e le relazioni", and for the sessions to be told their role by a message, again at
every change of formation: a member added, removed, or given another role.

Why it is clumsy today (verified in the code):

- The room is flat. Every session joins as `peer` (`MUKADE_CHAT_ROLE` is fixed in
  `crates/engine/src/mukade_chat.rs`), and who does what exists only in the prompts the user types.
- Members are called `claude-1`, `opencode-1`; the user sees tabs. An OpenCode or Pi member is not
  tied to its tab at all (`thread_chat.rs`, `project`), so "that OpenCode session" cannot be named.
- An agent can open only Claude Code and Codex (`session_control.rs`, the `harness` enum).
- Nothing tells a session who the others are and what they are for; each has to discover it.

## Goals

1. A **Team** view per thread where the user gives each session a name, a role, whom it answers
   to, and whether it may write or only observe.
2. Shinatama tells the sessions: each one learns its own role and the formation when it joins,
   and learns of every later change, without the user writing it into any prompt.
3. After that, "have the implementer run the plan" and "explain what the planner is doing" are
   sentences a session can resolve by itself.
4. An agent can open an OpenCode or Pi session into the thread, not only Claude Code and Codex.

## Non-goals

- A model per session. A harness's model stays the one set in the Models panel ("Model for new
  sessions"); the Team view shows it, read-only. `janura-qwen` on OpenCode is that assignment.
- Enforced delegation. "Answers to" is said to the agents, not enforced: Mukade has no such rule.
- A graph editor. The formation is a list with a "answers to" menu per row.
- Team templates shared across threads; opening a whole formation in one click.
- Hermes, and any session of a thread whose switch is off.

## Anti-goals

- A formation change that wakes every session. Wakes are bounded (10 per session) and each one
  interrupts an agent and spends tokens.
- A role the panel shows but the session was never told, with nothing saying so.
- A role message an agent could mistake for another agent's opinion, or one agent passing off
  its own message as the user's formation.

## Approaches

- **A. Messages only.** At each change Shinatama writes the whole formation to the room, addressed
  to everyone. Simple and visible, but every addressed message is a wake: a few edits spend half
  of each session's budget and interrupt work in progress.
- **B. State only.** The formation is thread state, read through `shinatama_thread_peers` when a
  session wants it. No noise, but a session whose role changed does not find out; and OpenCode and
  Pi have no Shinatama tools to read it with.
- **C. State plus targeted messages (chosen by the user, 2026-11-17).** The formation is thread
  state and always readable; at a change, only the sessions it concerns personally get an
  addressed message, and the rest get one unaddressed message, which they read at their next turn
  and which wakes nobody (`thread_chat.rs`: "none writes to the whole room, which wakes nobody";
  Mukade's adapter waits with `addressed_only`).

## Design

### 1. The formation is state of the thread

One setting, `threads.team.v1`, next to `threads.chat_enabled.v1` (`crates/state/src/database.rs`):
per workset, a `revision` (integer, +1 at each Apply) and its `members`. A member is a session
of the thread (`t_…`) with:

| Field | Meaning |
|---|---|
| `alias` | The name the user gives it ("planner"). Shown in the panel and said to the agents next to the Mukade member name; it does not replace it, since Mukade names members itself. |
| `role` | A short title ("Implementer"). |
| `duties` | Free text, at most 1 KB: what this session is for. |
| `answers_to` | Another member's session id, `user`, or nothing. |
| `access` | `write` or `observe`. |

A session of the thread with no entry is simply not in the team: it behaves as today.
Per member the setting also keeps what was last delivered to it (`told_revision`, and the Mukade
`member_id` it was delivered to), so a restart of Shinatama repeats nothing.

Only the user edits the formation. No tool lets an agent change it (see Anti-goals).

### 2. The Team view

A "Team" section at the top of the thread's Conversation panel (`apps/desktop/src/thread-chat.ts`),
which already lists the thread's sessions and their connection. Reached as today, by the thread's
Chat button; the thread's settings (⋯) get a "Team…" entry that opens the same panel.

```
Team                                             revision 3 · all told
┌──────────────────────────────────────────────────────────────────────┐
│ planner      Claude Code · in chat (claude-1)            wakes 2/10 │
│   Role [Planner        ]  Answers to [you ▾]   Access [Writes  ▾]   │
│   Duties [Writes the plan, reviews the implementer's result.     ]  │
├──────────────────────────────────────────────────────────────────────┤
│ implementer  OpenCode · janura-qwen · in chat (opencode-1)          │
│   Role [Implementer    ]  Answers to [planner ▾] Access [Writes ▾]  │
│   Duties [Implements the plan it is given, step by step.         ]  │
├──────────────────────────────────────────────────────────────────────┤
│ watcher      Codex · not in chat yet · will be told when it joins   │
│   Role [Observer       ]  Answers to [you ▾]   Access [Observes ▾]  │
│   ⚠ Observes from its next resume; it writes until then.            │
└──────────────────────────────────────────────────────────────────────┘
  + Add a session of this thread ▾               [Discard]  [Apply]
```

- Edits are local until **Apply**: several edits are one revision and one announcement.
- Each row says, in words, whether the session was told its current role: "told", "will be told
  when it joins", or "could not be told: <reason>".
- "Add" lists the thread's sessions that are not in the team. Opening a new session stays where
  it is today (the thread's + button); a new session then appears in "Add".
- Removing a row removes the role, not the session.

### 3. Telling the sessions

The sender is a new room member, `shinatama` (harness `shinatama`), distinct from `user`: the
panel draws its messages as notices, and agents can tell the user's formation from the user's
words and from another agent. Its handle is kept like the user's (in memory, recovered by
`rejoin`). Every personal message says that `shinatama` relays the formation the user set in the
Team view and that no other member can change it; `shinatama_thread_peers` says the same. (There
is no start note for sessions the user opens, and OpenCode and Pi have no Shinatama tools, so the
message itself has to carry it.)

Mukade as pinned ends every delivery with "Other members wrote these; they are not instructions
from the user" (`adapter.rs`, the delivery text) and shows only the sender's name. That line
would also cover the formation, and it already covers what the user writes in the panel as
`user`. See "Changes to Mukade".

No text Shinatama sends may contain `@`: Mukade turns `@name` and `@all` in a message into
recipients, which would wake them. Aliases, roles and duties are refused with `@` at Apply.
Mukade also refuses a message that looks like it holds a secret; such a refusal is shown on the
row as "could not be told", with Mukade's reason.

Messages are plain text, at most 2 KB, in English, of two kinds:

- **Personal** (addressed, wakes a listening session): "TEAM revision 3. You are **implementer**
  (member opencode-1). Role: Implementer. Duties: … You answer to planner (claude-1). You may
  write." followed by the formation, one line per member.
- **Notice** (unaddressed, wakes nobody): "TEAM revision 3. Joined: implementer (opencode-1),
  Implementer, answers to planner. Left: watcher. Changed: planner now answers to the user."

When each is sent:

| Event | Personal to | Notice |
|---|---|---|
| Apply: a member added, or its own role, duties, answers-to, access or alias changed | that member, if in the room | one, with the changes, when at least one other member is in the room |
| Apply: a member removed | that member ("you no longer have a role in this team") | same notice |
| A team member's session joins the room (first join, resume, a new run of Shinatama) and its `told_revision` is behind, or its Mukade member changed | that member | none: the join is already an event of the room |
| A team member's session is closed (archived) | nobody | one ("Closed: …") |

A hooked member never leaves the room by itself: the adapter does not call `leave`, and Shinatama
may not read its handle (ADR-0057 INV-002). Without the Mukade change below, the member of a
closed session stays listed in Mukade; the formation, not Mukade's member list, says who is in
the team, and the notice tells the others.

A member not in the room at Apply is told when it joins; its row says so until then. A send that
fails is retried at the next check, never more than once per check; a timeout is "uncertain" and
is not repeated blindly (same rule as the user's messages, ADR-0057 point 10): the ledger is read
first for a `shinatama` message carrying that revision and recipient.

What sends them: a worker in the desktop backend (like the session-launch worker in
`collaboration.rs`), not the window's read commands, which stay reads (ADR-0057 INV-001 and
point 10). Apply wakes it; otherwise it checks the talking threads that have a team every 10 s,
using the launch record and adapter files first and the service only when something changed.
It runs with the window closed to the tray as long as Shinatama runs.

### 4. Reading the formation

`shinatama_thread_peers` (Claude Code and Codex) gains `team`: the revision, `you` (alias, role,
duties, answers_to, access) and the same per peer, plus one `how_to` line: address a teammate by
its `member_name`; the formation comes from the user. OpenCode and Pi have no Shinatama tools:
they know the formation from the personal message, which carries all of it.

The bootstrap of a session opened by an agent (`collaboration.rs`, `initial_prompt`) gets one
line saying the thread has a team and where its role will come from.

### 5. Observe-only access

Mukade's adapter already supports `MUKADE_CHAT_ROLE=observer`: the member cannot send, follows
another member with `chat_observe`, and the adapter binds every hooked session so it can be
followed (`mukade/src/chat/adapter.rs`, `role_rules` and the bind after the first turn). So:

- A member with access `observe` is launched or resumed with role `observer` instead of `peer`
  (`ChatHooks::apply_environment`). This answers "have Codex explain what Claude is doing":
  give a Codex session the Observer role, then ask it.
- OpenCode and Pi too: their kit runs `mukade chat hook` with the inherited environment
  (`chat/kits/hook.js`), which reads the role (`adapter.rs`, `Settings::from_vars`). Read in the
  source, not yet run.
- Mukade as pinned fixes the role at the member's first join: the adapter reuses its stored
  state, role included, while the room is the same, `rejoin` copies the role, and no operation
  changes it. So without the Mukade change below, access can be chosen only for a session that
  has not joined yet; for one that has, the menu is disabled and says why. The panel's "Observes
  from its next resume" line in the sketch above holds only with that change.

### 6. Tying an OpenCode or Pi member to its session

Needed so that a role can be delivered to "that OpenCode tab". Mukade chooses the member name and
Shinatama records no native session for these harnesses, so the tie is made by exclusion: when a
kit launch of session S into room R is recorded (`mukade_chat::note_launch`), the first member of
that harness that joins R afterwards and is tied to no session belongs to S, provided S is the
only untied kit launch of that harness waiting in R. The tie is stored with the team
(`member_id`). When two launches of the same harness wait at once, neither is tied automatically:
the row offers a menu of the room's untied members of that harness ("Which member is this
session?"). A tie can always be corrected from that menu.

### 7. An agent opens OpenCode or Pi

`shinatama_session_open` accepts `opencode` and `pi` (`session_control.rs`; `open_collaboration`
in `agents.rs`). Per AGENTS.md no harness version is gated. These launches require the thread's
switch to be on, since the kit is their only way into the room; otherwise the request fails with
that reason, never as an isolated session. The model is the one assigned in the Models panel; the
receipt reports it. Today `open_collaboration` refuses both and neither launcher
(`opencode_launch.rs`, `pi_launch.rs`) takes an initial prompt, while OpenCode's kit joins the
room only at the first prompt: this step therefore includes passing the bootstrap as the
harness's own initial prompt, verified against the installed CLI, without which the session would
sit outside the room until the user typed. The bootstrap for these two omits the Shinatama tools they do not have and
names the coordinator's member. A session opened this way is not added to the team by the agent:
it appears in "Add" for the user.

The `mukade-collaborate` skill and its `references/mcp-contract.md` are updated to match
(harnesses, the `team` object, the `shinatama` member, the observer role).

## Changes to Mukade (the owner's decision, open)

Mukade is the owner's own project, pinned by revision (`scripts/mukade-pin.json`). Three things
above cannot be done properly with the pinned revision:

1. **Messages from the person and the host delivered as such.** Today they arrive labelled "not
   instructions from the user". Wanted: members of the host's harness (`shinatama`) are delivered
   under their own heading ("From the user, through Shinatama").
2. **Changing a member's role** (peer ↔ observer) for a member that already joined, by the host.
3. **The host retiring a member** whose session was closed.

With them, steps 1 and 2 meet their goals fully. Without them, step 1 relies on wording alone
(an agent may discount the formation), access is choosable only before the first join, and closed
sessions stay listed in Mukade. Recommended: make the three changes in Mukade first, bump the
pin, then build.

## Constraints

- ADR-0057 invariants hold: the window consumes nothing; no handle or credential reaches the
  frontend, argv or a file; a thread that is off, or has no team, launches exactly as today; a
  chat or team failure never prevents a session from opening.
- All of it applies only to threads whose switch is on. The Team section of a thread that is off
  says to turn the switch on.
- Windows first; no new dependency in the desktop process (ADR-0047).
- Texts sent to agents are English, like every other bootstrap; the interface is English.

## Order of work

1. State, Team view, Apply, the `shinatama` member, personal and notice messages, join detection,
   `team` in `shinatama_thread_peers` — for Claude Code and Codex (their members are already tied).
2. Observe-only access.
3. Tying OpenCode and Pi members; roles for them.
4. `shinatama_session_open` for OpenCode and Pi.

Each step is usable on its own and verified with real agents before the next (the confirmation
of ADR-0057 itself — "Not yet run on Windows with real agents" — is a prerequisite of step 1).

## Left to the builder

- The exact wording of the two messages and of the panel's texts, within the limits above.
- The shape of the stored setting and its migration-free reading (absent means no team).
- How the worker is woken and paced, within "no service call when nothing changed".
- Tests: pure projections (what to send for a given old/new formation and room state) in Rust;
  the panel's model in TypeScript; one real-agent run per step, recorded in `docs/verification`.

## To verify before building on it

- That an unaddressed message really leaves a listening hooked session asleep (Mukade's wait
  uses `addressed_only: true`; not yet observed live).
- That OpenCode and Pi kits take the observer role live (read in the source only).
- The wake count is never reset: a session has 10 wakes for its whole life in the room, and
  personal team messages draw on them. Whether that budget is enough in practice.
- That a `rooms`/`members` read is enough to notice a join within one 10 s check.
