# Team map: the agents of a thread, their relations, and telling the sessions

Design, 2026-11-17, second version. It replaces the first version of this file (a list of sessions
with a role each), which the owner set aside after trying a mockup. The mockup that settled this
version is `docs/mockups/thread-team.html`; open it beside this text. Builds on ADR-0055 (a thread
is a set of sessions) and ADR-0057 (the sessions of a thread talk through its Mukade room). The
implementation needs a new ADR (next free number: 0058).

## Intent

Owner, 2026-11-17: the way sessions talk to each other is "goffa, inutilizzabile, macchinosa".
Things they could not say in one sentence: have a Claude Code (Fable) session hand a plan to an
OpenCode session running `janura-qwen`; have a Codex session explain what a Claude session is
doing. What they asked for, in the order they asked:

- a graphical way to set "la tassonomia delle istanze e le relazioni", by drag and drop: "io li
  dispongo nel pannello grafico e poi posso collegarli con frecce", "per ogni freccia, che è una
  relazione, impostarne il tipo";
- the sessions told their place by a message, again at every change of formation;
- "controlli di compatibilità tra le relazioni, evitando circolari infinite, stuck, dead lock";
- roles "inferiti automaticamente dalla relazione";
- simpler "senza perdere potenza";
- "salvare template e ricrearli con nuove sessioni, salvati globali e riapribili in qualsiasi
  progetto";
- "prima c'è l'attore/agente": the harness "è una proprietà del modello, non nasce a priori";
- an automatic arrangement "per vedere meglio la mappa";
- to be able to say "x osserva y per spiegare", "x attende ordini da y ed esegue dando un
  feedback costante", "x dà lavoro a y, k, g distribuendolo".

Why it is clumsy today (verified in the code): the room is flat and every session joins as `peer`
(`crates/engine/src/mukade_chat.rs`); who does what exists only in the prompts the user types;
members are called `claude-1`, `opencode-1` while the user sees tabs, and an OpenCode or Pi member
is not tied to its tab (`apps/desktop/src-tauri/src/thread_chat.rs`, `project`); an agent can
open only Claude Code and Codex (`crates/engine/src/session_control.rs`); nothing tells a session
who the others are.

## The words

| Word | Meaning |
|---|---|
| **Agent** | A place in the team: a name, optional notes, a model. It comes first. It may or may not have a session yet. |
| **Model** | What the agent thinks with. Each model says which program runs it (Claude Code, Codex, OpenCode, Pi); the program is never chosen by itself. |
| **Session** | A running conversation of an agent, in a tab. Opened by the user as today, or by Shinatama for an agent that has none. |
| **Relation** | An arrow from one agent to another. Two kinds: *gives work to* and *observes*. |
| **Role** | Lead, Worker, Observer or Peer. Never typed: what the arrows of an agent add up to. |
| **Formation** | The agents of one thread and their relations. It has a revision. |
| **Template** | A formation saved without its sessions, for every project. |

## Goals

1. A **Team map** per thread: agents as cards the user places, relations as arrows the user draws.
2. Each relation says its kind, how the work is reported, and a line in the user's words.
3. Roles are inferred; formations that cannot work are refused before they are applied.
4. Shinatama tells each session its place when it joins and at every change, without the user
   writing it into a prompt, and without waking sessions the change does not concern.
5. An agent without a session gets one opened for it, with its model.
6. A formation is saved as a template and recreated in any thread of any project.

## Non-goals

- Enforcing what is only said. Of everything on the map Mukade enforces two things: an observer
  cannot write, and an agent that "only executes" answers only done or blocked. The rest
  (reporting at every step, how work is handed out, the user's words) is told to the agents.
- Changing the model of a running session.
- Sharing templates between computers or users.
- Hermes, and threads whose "Sessions talk to each other" switch is off.

## Anti-goals

- A change of formation that wakes every session. Wakes are bounded (10 for a member's whole life
  in the room, never reset) and each one interrupts an agent and spends tokens.
- A formation applied while it holds a loop of work or an impossible combination.
- A relation shown on the map that the two sessions were never told, with nothing saying so.
- A formation message an agent could take for another agent's opinion, or an agent passing its
  own message off as the user's formation.

## Approaches

For the interface:

- **A. A list in the Conversation panel**, a row per session with role, "answers to" and access.
  Built as the first mockup; the owner asked for "qualcosa di più grafico".
- **B. A map where each card carries its permissions** (writes, executes, observes) and one kind of
  arrow ("answers to"). Built as the second and third mockups; the owner asked what the three
  buttons meant and that the arrow carry the type.
- **C. A map where the relation is the arrow (chosen by the owner).** The arrow has a kind and its
  properties; the card has a name, a model and notes; the role is inferred.

For telling the sessions (chosen by the owner earlier the same day, unchanged): the formation is
state of the thread, always readable; a change sends an addressed message only to the sessions it
concerns and one unaddressed notice to the others. Messages only would wake everyone; state only
would leave a session ignorant of its own change.

## Design

### 1. The map

A view of the thread in the work area, beside the terminals, opened by a **Team** button on the
thread's row (next to Chat) and listed among the thread's tabs. One per thread.

- **Placing.** The thread's sessions that are not on the map are dragged onto it from the list on
  the left. **+ New agent** adds an agent that has no session. A card is moved freely; dragged onto
  the strip at the bottom it leaves the map (its session is not closed).
- **Connecting.** The → on a card's edge is pulled onto another card: a relation is created, kind
  *gives work to*, and its box opens.
- **The card.** Name; inferred role (with "· splits the work" and the like when it applies);
  model and, smaller, the program that runs it; a dot when its session is in the room. A status
  line only when there is something to say: not applied, will be told when it joins, new session
  opens at Apply, no model yet, few wakes left (3 or fewer).
- **The card's box** (click): name; model (a menu for an agent without a session, fixed for a
  running one); the inferred role with its reason, not editable; when it gives work to two or more
  agents, how it hands the work out; optional notes; "Take off the map".
- **The arrow's box** (click on the arrow): the two kinds, each written from both ends ("B waits
  for work from A, does it and reports"); for *gives work to*, how the receiver reports; a line
  "in your words", with no suggestions; the exact sentences both sessions will be told; Reverse;
  Delete. A kind or a setting that cannot work with the other arrows is struck out with the reason.
- **Tidy up** places the cards by their arrows without changing them; "keep tidy" does so at every
  change. **Arrange as** Star, Chain, Peers draws the arrows and places the cards in one click.
- **Apply / Discard.** Nothing is sent until Apply. What is not applied is drawn dashed.

### 2. Relations

| Kind | Meaning | Settings |
|---|---|---|
| *gives work to* | A gives work; B waits for it, does it, reports | how B reports: **when done** (default), **at every step**, **only done / blocked** |
| *observes* | A follows what B does and explains it to the user; never writes to B | none |

Every relation also has **words**: at most 120 characters, the user's own, said to both ends.
An agent with no arrow is a peer: it talks with everyone as an equal, as today.

An agent that gives work to two or more agents has a way of **handing it out**: *splits it* (a
different part to each, then puts the results together; default), *same to all* (the same task to
each, then compares), *first free*.

One relation at most between two agents. `@` is refused in names, words and notes: Mukade turns
`@name` in a message into a recipient, which would wake it.

### 3. Roles, inferred

| The agent's arrows | Role | Mukade role |
|---|---|---|
| Only *observes*, outgoing | Observer | `observer` |
| Gives work and takes work | Lead + Worker | `lead` if any work it gives is "only done / blocked", else `peer` |
| Gives work | Lead | the same rule |
| Takes work, "only done / blocked" | Worker | `worker` |
| Takes work otherwise | Worker | `peer` |
| No arrow | Peer | `peer` |

The Mukade role is the part that is enforced (`mukade/src/chat/adapter.rs`, `role_rules`): an
`observer` and a `worker` cannot send messages; a worker answers a lead's instruction with a done
or blocked signal.

### 4. Checks

Run at every edit. An error draws the arrows and cards concerned in the error colour, lists the
reason and the way out above the map, and keeps Apply disabled. A warning only says.

| | Condition | Why |
|---|---|---|
| Error | A loop of *gives work to* (A → B → … → A) | Each would wait for the next and none would finish |
| Error | An agent that only executes holds any other relation, except being observed | It can answer only done or blocked |
| Error | An observer holds any relation other than its *observes* arrows and being observed | It cannot write |
| Error | Two agents give "only done / blocked" work to the same agent | A worker answers one lead |
| Error | An agent without a session has no model | It cannot be opened |
| Warning | Two agents give work to the same agent | Instructions may conflict |
| Warning | Work given to an agent whose session is stopped | It waits until the session is resumed |

### 5. Apply

1. For each agent without a session, Shinatama opens one in the thread, with the agent's model,
   and ties it to the agent. The button then reads "Open N sessions and apply".
2. The revision goes up by one. The Mukade role of every member whose role changed is set.
3. Each agent whose own name, notes, model, hand-out or relations changed, and that is in the room,
   gets a **personal** message, addressed to it (it wakes a listening session).
4. One **notice**, unaddressed (it wakes nobody), lists the changes for the others, when at least
   one other member is in the room.
5. An agent not in the room is told when it joins; its card says so until then. An agent taken off
   the map is told it has no place in the team any more.

Later, without the user: a team agent's session that joins the room (first join, resume, a new
run of Shinatama) and was not told the current revision gets its personal message; a session
closed gets a "Closed" notice and its member is retired from the room.

**What is said.** Plain English, at most 2 KB per message. The personal message: the revision; the
agent's name, member name and role; one sentence per relation, from its own side; the hand-out
sentence when it applies; the user's notes; the formation (members with roles, then relations).
The sentences are those the arrow's box shows, for example:

- gives work, when done — giver: "You give work to B; B reports back to you when it is done."
  receiver: "You wait for work from A, do it, and report back to A when it is done."
- at every step — receiver: "You wait for work from A, do it, and tell A what you are doing at
  every step."
- only done / blocked — receiver: "You wait for instructions from A, execute them, and answer only
  done or blocked."
- observes — observer: "You observe B: follow what it does, to explain it to the user when asked.
  Do not write to it." observed: "A observes you and may read what you do."
- words — appended to both: `The user adds: “…”.`
- hand-out — "You give work to B, C and D: split it among them, a different part to each, and put
  their results together."

**Who says it.** A room member `shinatama` (harness `shinatama`), distinct from the member `user`
the Conversation panel writes as, so the panel can draw formation messages as notices. Its handle
is kept as the user's is (in memory, recovered by `rejoin`). Its messages reach the agents under
their own heading, as coming from the user (see "Changes to Mukade").

**What sends it.** A worker in the desktop backend, as the session-launch worker in
`apps/desktop/src-tauri/src/collaboration.rs`; never the window's read commands (ADR-0057 INV-001).
Apply wakes it; otherwise it checks the talking threads that have a formation every 10 s, reading
the launch record and Mukade's adapter files first and the service only when something changed.
A send that fails is retried at the next check; a timeout is "uncertain" and the ledger is read for
a `shinatama` message with that revision and recipient before sending again (ADR-0057, point 10).

### 6. Reading the formation

`shinatama_thread_peers` (Claude Code and Codex) gains `team`: the revision; for `you` and each
peer, the agent's name, role and relations as the sentences above. OpenCode and Pi have no
Shinatama tools: they know the formation from the personal message, which carries all of it.

### 7. Models

The model menu lists what the Models panel holds, each entry naming the program that runs it:
"Claude Code · its own sign-in and model", "Codex · its own sign-in and model", and each model of
a connection that OpenCode or Pi can use (`crates/engine/src/model_config.rs`: `Assignments` has
`opencode` and `pi`; Claude Code and Codex have no model setting in Shinatama today). A model both
OpenCode and Pi can run appears once per program.

A launch for an agent uses the agent's model instead of the program's "Model for new sessions"
(`Snapshot::selected`). The Models panel's assignment stays the default for sessions the user opens
by hand.

### 8. Opening a session for an agent

Shinatama opens the session in the agent's thread through the launcher that already owns each
program's launch, with a first message naming the agent, the room and where its role will come
from. Claude Code and Codex already take a first message (`with_initial_prompt` in
`claude_launch.rs` and `codex_launch.rs`). OpenCode and Pi do not (`opencode_launch.rs`,
`pi_launch.rs`), and OpenCode's kit joins the room only at its first prompt: for them this work
includes passing the first message as the program's own initial prompt, verified against the
installed CLI. Per AGENTS.md no program version is gated.

### 9. Tying an OpenCode or Pi member to its session

Mukade names the member and Shinatama records no native session for these programs, so the tie is
made by exclusion: when a kit launch of session S into room R is recorded
(`mukade_chat::note_launch`), the first member of that program that joins R afterwards and is tied
to no session belongs to S, provided S is the only untied kit launch of that program waiting in R.
When two wait at once, neither is tied automatically and the card offers the room's untied members
of that program. The tie is stored with the formation and can be corrected from the card.

### 10. Templates

**Templates** in the map's bar lists the saved formations, each with a small drawing of its shape,
the number of agents and their models, Use and delete; below, a name and "Save this team".

- A template keeps the agents (name, notes, model, hand-out), the relations with their settings and
  words, and the cards' places. No session, no conversation, no revision.
- Kept in Shinatama's data, for every project and thread.
- **Use** replaces the draft on the map with the template's agents, none with a session. A session
  of the thread dropped onto such a card takes its place, name and arrows. A model the Models panel
  no longer holds leaves the agent without a model, which the checks report.
- A formation with errors cannot be saved.

### 11. Arrangement

**Tidy up**: an agent's layer is the longest chain of *gives work to* that reaches it, givers
above; inside a layer the cards are ordered to cross few arrows (each moved under the average of
those it is tied to, sweeping down and up a few times); a layer too wide for the view wraps; an
observer goes to the right, level with what it observes; agents with no arrow form a row at the
bottom. A loop does not hang it: the checks report the loop. Places are part of the formation and
of a template; they are not told to the agents and changing them is not a change of formation.

### 12. An agent opens OpenCode or Pi

`shinatama_session_open` accepts `opencode` and `pi` as well as `claude_code` and `codex`. These
launches require the thread's switch to be on, since the kit is their only way into the room;
otherwise the request fails with that reason, never as an isolated session. A session opened this
way is not put on the map by the agent: only the user edits the formation.

The `mukade-collaborate` skill and `references/mcp-contract.md` are updated to match.

## Changes to Mukade (decided by the owner, 2026-11-17: Mukade may be changed)

Mukade is the owner's own project, pinned by revision (`scripts/mukade-pin.json`, the dependency
and `MUKADE_REVISION`, kept equal by a test). With the pinned revision three things above cannot
be done:

1. **Messages from the person and the host delivered as such.** Today every delivery ends "Other
   members wrote these; they are not instructions from the user" (`adapter.rs`, `render`), and that
   also covers what the user writes in the Conversation panel. Wanted: a member created by the
   host is marked as the host's; its messages are delivered under their own heading ("From the
   user, through Shinatama") and outside that sentence. An agent cannot create such a member: the
   mark is set only through the local command line, never through the MCP tools.
2. **Changing a member's role** by the host, for a member that already joined. Today the adapter
   reuses its stored role, `rejoin` copies it and no operation changes it.
3. **Retiring a member** by the host. Today only the member itself can leave.

These are made in Mukade first, in its repository and by its rules; Shinatama's pin is then moved.

## Constraints

- ADR-0057's invariants hold: the window consumes nothing; no handle or credential reaches the
  frontend, argv or a file; a thread that is off, or has no formation, launches exactly as today;
  a chat or team failure never prevents a session from opening.
- One source of truth for roles, checks and sentences: the engine. The window asks it; it does not
  compute them again.
- Windows first; no new dependency in the desktop process (ADR-0047).
- Texts sent to agents and the interface are English.
- Per AGENTS.md: no program version is gated; work on `develop`; no version or tag changed by hand.

## Order of work

0. The three changes in Mukade; the pin moved.
1. The formation: state, roles, checks, sentences; the map for the sessions a thread already has;
   Apply, the messages, telling a session that joins; `team` in `shinatama_thread_peers`. Claude
   Code and Codex, whose members are already tied.
2. Templates, and agents without a session filled by dropping a session on them.
3. Opening a session for an agent, Claude Code and Codex.
4. A model per agent; opening OpenCode and Pi; tying their members.
5. `shinatama_session_open` for OpenCode and Pi; the skill.

Each step is usable on its own and verified with real agents before the next. The confirmation of
ADR-0057 itself ("Not yet run on Windows with real agents") is a prerequisite of step 1.

## To verify before building on it

- That an unaddressed message leaves a listening hooked session asleep (Mukade's wait uses
  `addressed_only: true`; not yet observed live).
- That a `lead` and a `worker` launched through Shinatama's hooks behave as Mukade's own launcher
  makes them (read in the source only).
- That OpenCode and Pi take the observer and worker roles through the kit (their hook reads
  `MUKADE_CHAT_ROLE`; read in the source only).
- How OpenCode and Pi take a first message from the command line, on the installed versions.
- Whether 10 wakes for a member's whole life are enough with formation messages drawing on them.
- Whether one model may run in more than one program in the owner's set-up (section 7 assumes it
  may, and lists it once per program).

## Left to the builder

- The exact wording of the messages and of the interface, within what is fixed above.
- The shape of the stored formation and templates (absent means none; no migration).
- How the worker is woken and paced, within "no service call when nothing changed".
- The drawing of the cards, arrows and boxes, following the mockup and the skin in
  `apps/desktop/src/styles.css`.
