# Shinatama — design

Date: 2026-10-26
Status: draft, not yet approved. Nothing is built from this document until it is.

Shinatama is a local-first terminal runtime, written in Rust, that hosts the real
coding agents (Claude Code, Codex, OpenCode, Pi) in one terminal, lets the user hop
a live chat from one agent to another without starting over, and replaces
Musashi Terminal (the C++ launcher in `D:\Projects\horness-launcher`). This document
lives in the new project `shinatama`.

Who decided what is marked throughout:

- **Decided** — said by the user in this conversation.
- **My call** — a choice I made to fill a gap; the user has not confirmed it.
- **Open** — only the user can answer it, and the answer changes what gets built.

## 1. Intent and why

The user runs several coding agents every day and loses time re-explaining a task
when a different agent would do better, or when an old chat has to be found and
resumed. The point is a *runtime*, not a wrapper that fakes the conversation:

- the real agent runs in a real terminal session inside Shinatama;
- the thread is tracked on disk, so it survives the agent and the terminal;
- a hop to another agent keeps the chat, tool calls, files and attachments as far as
  that agent's own native resume flow allows;
- Daodan (the user's plugin marketplace) and the BillionContext sidecar come with it,
  so a Shinatama session is already the full working setup, not an empty shell.

Success looks like this: a thread started in one agent continues in another with
one keystroke and nothing re-explained; any old chat, in any supported agent, is
found by searching and resumed in the same or another agent; and a launch is as
resilient, as safe and as well compressed as a launch through Musashi today.

## 2. Goals, non-goals, anti-goals

### Goals (decided unless marked)

1. Host Claude Code, Codex, OpenCode and Pi as real processes in a terminal
   multiplexed inside Shinatama.
2. Seamless hop of a live thread between those agents (see 5.2).
3. Search all local sessions of all supported agents; resume from history in the
   same agent or in a different one. Interactive terminal flow plus a scriptable
   command line mode.
4. An integrated project manager, "as in Horness, but better" (5.4), with a
   default root directory and a shortcut to create a new project.
5. Daodan arrives preinstalled and enabled in every agent (5.5), and Shinatama is a
   **native support for it** (decided 2026-10-27, user): it reads Daodan's tasks, links
   threads to them and carries a task across a hop. Read-only; Daodan stays the only
   writer of its state. See [daodan-native-support.md](daodan-native-support.md).
6. The BillionContext sidecar is integrated "as in Horness, with the same
   resilience, security and compression" (5.6).
7. Skinnable UI whose default is the pink Shinatama theme (5.7).
8. Easy install on macOS and Linux.
9. A desktop widget with Shinatama as an assistant that talks, moves and alerts
   (5.8) — decided for v1; built last within v1, after the core it listens to.
10. A roundtable: several hosted agent instances, each with its own model, converse
    and compare positions on a topic (5.10). Shape and scheduling are **open**.
11. Recognize existing logins automatically, or run the agent's own login, for each
    supported agent (5.11).
12. A global overview tab of every instance started from or inside Shinatama: what
    each is doing (titled periodically by a low-cost model), how they collaborate,
    the flows and exchanges between them, and the work (5.13).

### Non-goals

- Grok. Daodan has no Grok adapter and the user said Grok is not supported for now.
  The agent adapter contract must make adding it later cheap.
- Writing Daodan state. (Reversed 2026-10-27: the earlier non-goal "no thread-to-task
  link, no hand-over of the task holder when hopping, no task agenda" is withdrawn by
  goal 5. What remains: Shinatama only reads `.daodan/agenda.json`; Daodan's scripts
  are the only writer.)
- A universal migration of a live session between agents. No tool does this; the
  hop starts a *new* native session in the destination.
- Work Controller's task-graph features (checkpoints, handover between Daodan tasks,
  a persistent addressed mailbox between agents). They are Daodan's roadmap; Shinatama
  shows the agenda and the holder but does not implement them. The
  roundtable (5.10) is different: started by the user, session-level, no mailbox.
- Choosing or configuring an external terminal (Windows Terminal, Warp). Shinatama
  is a text UI that runs in whatever terminal the user opened, like tmux; it is not
  a window of its own (the original request asked for "a terminal app"). **Decided**
  (the user confirmed the frame-and-panels approach of 5.1). The widget (5.8) is the
  only graphical part. Real graphic effects inside the main window (rounded corners,
  gradients, shadows, free images, animation) are out of scope for that reason.

### Anti-goals (would be a failure even if it technically works)

- A wrapper that replays or simulates the conversation instead of running the agent.
- Promising a lossless transfer. The UI must always say what carries over and what
  does not. The words "native resume" are used only for resuming within the *same*
  agent.
- A second scheduler or a resident service that keeps executing work after the
  terminal closes.
- A sidecar integration that is weaker than Musashi's: any loss of private root,
  child-only environment, child-only certificate trust or proxy recovery.

## 3. Constraints

- Rust. macOS and Linux are required (the original request); Windows is assumed
  required too (section 7, item 1).
- Why Rust and not C++ (Musashi's language): the user's original request named Rust;
  my reasons agree. Shinatama *hosts* agents, which Musashi never did: it needs a
  pseudo-terminal on all three systems and a terminal emulator, for which mature Rust
  crates exist (pseudo-terminal, escape-sequence parser, text UI), against assembling
  ConPTY and a VT library by hand in C++; it runs many concurrent tasks (several
  terminals, sidecar, event stream, summaries); one toolchain builds and cross-builds
  for macOS, Linux and Windows. The cost: the checkup (3,256 lines) and the tests
  (7,134 lines) are rewritten; the captured real-output fixtures can be reused if they
  are plain data (not verified). Not proved yet: that a Rust pseudo-terminal plus
  emulator hosts the four agents faithfully on Windows with ConPTY — the first
  milestone must prove it before the rest is built on it.
- Local-first: no account, no hosted service. Threads and settings live on disk.
- Every agent launch goes through the checkup, as in Musashi: there is one launch
  point and it runs the checkup (my call, inherited from Musashi's first
  constraint; the user did not object).
- Agent behaviour that Shinatama depends on (resume flags, session stores, hooks,
  HTTP APIs) must be verified against the installed version of each agent with real
  captured fixtures, as Musashi's tests do. Documentation alone is not proof.
- The BillionContext sidecar source in `sidecar/source-build` stays the single
  source; Shinatama does not fork it, and does not install the precompiled npm
  package (docs/sidecar.md).
- No measured saving percentage may be claimed for compression
  (docs/token-efficiency.md says none has been measured).
- **Isolation, at a glance.** Nothing system-level is touched: the sidecar keeps a
  private root and applies its environment and its certificate authority to the launched
  child only; hooks and the MCP server are injected per launch, never by editing global
  configuration; the agents are the real binaries on the user's own homes and sessions
  (scratch homes are a test-only device). The exception is Daodan: the checkup installs
  it into each agent's native plugin store, so the user's own agent runs see it too (the
  user's disabled plugins stay disabled). Unproved parts: per-launch MCP injection, one
  proxy per instance, the `CODEX_HOME` overlay (register rows M1, SC5, CX12).

## 4. Surfaces

Terminal UI screens, shown in the mockups listed in section 11: project manager,
session view with agent tabs, hop dialog and preview, find (search), Daodan
palette, first-run setup. Command line: `shinatama find`, `resume`, `hop`,
`doctor` (names are my call). Desktop widget: separate process (5.8).

## 5. The parts

### 5.1 Runtime and threads on disk

Each agent runs in a pseudo-terminal owned by Shinatama; the user types into the
real agent. A **thread** is Shinatama's own record on disk: an id, the project
folder, the chain of agent sessions it has been through (agent, native session id,
time span) and enough to rebuild the hand-over context. The format and location are
the builder's call. Closing Shinatama does not lose a thread.

Shinatama reads each agent's own session store to search and to resume. Those stores
differ per agent and per version; the adapter for each agent owns reading them.

**The hosted agent lives in a pane inside Shinatama's tab (decided).** What Shinatama
draws around and over it:

- **Frame and bars:** border and title, a top bar (thread, folder, branch, cost, the
  agent's icon) and a bottom bar of keys, all coloured by the skin.
- **Side panels:** for example the thread column (what carries over in a hop) beside
  the agent. The agent is given a narrower terminal and resizes itself, so resizing is
  forwarded to it correctly.
- **Windows over the agent:** the hop dialog (`^H`), the Daodan palette (`^K`) and
  find. Shinatama owns the emulator's screen, so it composes the overlay and then
  restores the agent's screen intact.
- **Skin colours inside the pane:** the agent's standard 16-colour palette is remapped
  to the skin's palette, so the Shinatama pink reaches the agent's own output.

What stays the agent's: the text it draws (tables, tool calls, reasoning, choices) and
any colour it picks at 24-bit. Not yet verified, to be proved by the first milestone:
fidelity of the emulator against the real Claude Code, Codex, OpenCode and Pi on
macOS, Linux and Windows (ConPTY can alter escape sequences); wide characters and
emoji; alternate screen and scroll regions; mouse and advanced keys; passing agent
images (kitty or sixel) through to the outer terminal; no flicker; and how well the
colour remap reads in each skin.

Formatting of live work stays the agent's own. Where the data is already structured
and read-only — find, the hop preview, the roundtable, the overview — Shinatama draws
its own rich views. Redrawing the live chat itself from each agent's structured stream
is not planned.

### 5.2 Hop (decided: "truly seamless"; built from scratch in Rust)

> **The how is specified in [the engine spec](technical-design.md)**:
> shared SQLite state (section 4), readers per agent and the canonical transcript
> (5), seed and budget (6.4), the hop state machine with crash recovery (6.3), hop back
> and catch-up (6.6), failure matrix (6.7), where it departs from Agent Deck (6.8).
> What follows is the summary; where they differ, the engine spec wins.

What "seamless" can honestly mean — from the user's own interoperability research
(2026-10-26): no tool moves a live session between different agents. What exists is
a **new native session in the destination, started with the thread's content**,
with the loss declared. Resuming *within* the same agent is native and lossless.

So seamless is defined by experience, not by lossless state:

1. **One keystroke**, nothing to re-explain, no copying. (my call)
2. The destination agent is **prepared in the background** while the user keeps
   working, so the switch is instant. (my call; needs verification that starting an
   agent costs nothing until its first turn)
3. The destination receives the **whole thread**, not a fixed small excerpt —
   bounded by the destination model's window and by what the sidecar will let
   through. Agent Deck, the closest existing tool, caps the export at 32 KB by
   default; Shinatama should do better, with the budget computed for the
   destination. (my call)
4. The **source session stays untouched** and, in close mode, is archived only after the
   destination confirms it has the context. Hopping back later is a native resume of the
   earlier session, plus a catch-up of the turns it missed. (my call; the catch-up is a
   guess to validate)
5. A **preview** of exactly what the destination will receive, before sending.

**Close or fork the source (decided 2026-10-27, user).** Every hop offers both: close
the source once the destination is live (the default), or **fork** — the source instance
and thread stay live and usable, and the destination continues the work as a **new
thread** whose parent is the source thread. A user setting (`hop.source_mode`) picks the
default (default: close); the hop dialog offers the other mode for that hop only.
Forking never resumes the source's live native session in the destination: the
destination gets a new native session (the agent's own fork where it exists, otherwise
an import or a seed), so two processes never write one session. After a fork the two
threads diverge on purpose; nothing synchronizes them. Details: engine spec 6.2 and 6.3.

How the history reaches the destination — **my recommendation, hybrid**: where the
destination's session format can be written reliably, import the converted history
as a real native session (fuller fidelity, tool calls as native entries where
possible); otherwise fall back to a readable text export. Import is gated per agent
and version by captured fixtures. **Staged (my call, to check):** v1 ships the text
hand-off for every pair of agents, with the preview; native import is added one
destination agent at a time, only when its fixtures pass, because it writes into
another agent's session store and a bug could corrupt the user's data. The hybrid is
the end state. The user has not confirmed this; the alternative is text only. Native
import is done by the `mukade` interchange library — Shinatama's own reader and writer for the four harnesses, published as its own repository and linked in-process (technical design 5.4.1).

What carries over, as the hop dialog must show it: conversation (text),
tool calls (summarized unless native import preserves them), files (same folder;
git state unchanged), attachments (paths handed over), project instructions (e.g.
CLAUDE.md ↔ AGENTS.md linked). What does not: permissions (the destination asks
again), model state.

Prior art to study, not to wrap: Agent Deck (Go, tmux, MIT; on Windows only via
WSL) for the handoff design; Agent Relay for messaging. Native APIs to use where
they exist: Codex App Server, Claude Agent SDK sessions, OpenCode V2 HTTP (import,
export, fork). All are documented as partly experimental.

### 5.3 Search and resume

Full-text search over the sessions of all four agents, filterable by agent,
project and date, with a preview of the thread. Enter resumes in the same agent
(native); another key resumes in a different agent (a hop). The command line mode
mirrors it for scripts, for example:

```
shinatama find "refresh token" --json | jq -r '.[0].id'
shinatama resume t_8f3a --agent codex
```

### 5.4 Project manager

Starts from what Musashi does today (`src/config.*`, `src/history.*`): project
roots whose first-level folders are the projects, sorted by last change;
incremental filter while typing; any folder via F3; a history of the last 10
non-standard folders, including folders that are not git repositories (Desktop,
shared drives).

Decided additions: a **default root** (where new projects are created; chosen at
first run, changeable) and a **shortcut (`n`) to create a new project**.

My calls ("better than Horness"): pinned projects; git branch and dirty-file count;
the last agent and last thread per project; the threads of a project listed
beside it; one key to resume the last thread; several roots. In the new-project
dialog: optional `git init`, and an option to type Daodan's native init command in
the chosen agent (never run automatically).

### 5.5 Daodan preinstalled (decided: option A, downloaded and kept updated)

Daodan is a set of plugins. Shinatama's checkup installs or updates it in every
agent present and enables, by default, **every plugin the Daodan marketplace lists**
(decided 2026-10-27, user: "mi aspetto che attivi tutti i plugin"; read from the
manifest at check time, 48 at marketplace 42.7.3): the entry plugin `daodan`, the
document-driven project management (project-lifecycle, project-delivery with ADR/FRD
under `docs/`, project-knowledge), codebase-map, senior-review, testing, the domain
verticals, `research` and the rest. A setting `daodan.enable = all | core | custom`
(default `all`; `core` is the entry plugin plus its dependency closure) keeps the
lighter choice available. The agents use them
through their own native entries: `/daodan:<workflow>` on Claude Code and OpenCode,
`<workflow>-workflow` on Codex, `/daodan-<workflow>` on Pi (docs/hosts.md of the
Daodan repository).

Musashi already does this for Claude, Codex, OpenCode and Pi in `src/checkup/*.cpp`:
it reads the required dependencies from the manifest in each host's clone and
translates them with a per-host table. That logic is ported, not reinvented. The
`research` question is settled by "all"; what is open is the cost of "all" (register D4).

A palette (`^K`) lists Daodan's workflows and types the selected agent's native
command into the prompt; the user presses Enter. (my call)

**Native support (decided 2026-10-27, user; shape is mine):** beyond installing,
Shinatama reads Daodan's task agenda, links threads to tasks, shows tasks and
questions in the project manager, overview and widget, adds task commands to the
palette, and carries the task across a hop (the user's confirmation in the hop dialog
is the "old session is closed" that Daodan's take-over needs). On a project with no
Daodan state it offers once to onboard it (the handover dossier) or initialize it (map,
environment, baseline) by typing the matching Daodan command. It never writes Daodan
files. Full design, behaviour and open checks: [daodan-native-support.md](daodan-native-support.md).

### 5.6 BillionContext sidecar (decided: same resilience, security, compression)

Same code, supervised by Rust instead of C++ (`src/sidecar.cpp` is the reference).
Acceptance is behavioural parity with Musashi, each item re-verified under
Shinatama and not assumed:

- private root; Canonical config, data, cache and state paths resolve under it;
- environment applied to the child process only;
- the proxy's certificate authority trusted only by the launched process, no
  system trust-store change;
- the launch manifest holds argv and launch overrides only, never credentials;
- automatic proxy recovery on the original address for active sessions; orderly
  drain on exit;
- no auto-update; the owned source upgrades only through Shinatama;
- compression policy as shipped (adaptive pressure, CCR, Absorb, profiles);
  `--no-context` remains an explicit per-session opt-out.

Agents covered in v1: Claude Code, Codex, OpenCode, Pi; Gemini keeps the routing it
has today (launch-only, 5.9). The sidecar needs Node (as today); how Node is provided on macOS and Linux is
the builder's call within the install story. A known interaction: the hop context
passes through the sidecar and counts against the destination's window; and the MCP
shim must follow `/resume` session ids (docs/sidecar.md, 2026-10-26).

### 5.7 Skins (decided: default is Shinatama pink; others selectable)

Skins are selectable at runtime and stored in the configuration; the UI colours come
from a skin definition, never hard-coded.

- **Shinatama (default):** midnight navy, cobalt/ice lines as in the repository
  logo, her pink hair as the only warm accent.
- **Section 9:** teal-black with cyan, faint scan lines.
- **TCTF:** steel grey with orange, cut corners, filled active tab.
- **Sumi:** ink black with vermilion.

In a text UI a skin is colour, box characters and layout. Effects seen in the HTML
mockups (scan lines, clipped corners, glow) illustrate the mood and are not
requirements.

The references are *Oni* (Bungie, 2001) and *Ghost in the Shell*; the user's wider
Daodan/Shinatama family shares that world. The colour values in the mockups are
illustrative; final tokens are the builder's. Secondary text must reach 4.5:1
contrast in every skin (the design review caught this in the mockups). No glowing
shadows.

### 5.8 Desktop widget (decided, in v1; "assistant" scope is my call)

A small borderless, transparent, always-on-top window showing Shinatama as an
assistant: it speaks in a bubble, animates, and alerts when an agent needs input,
an operation finishes, or something important happens (e.g. the context filling up).

My calls for the shape: the core exposes a local event stream (socket or named
pipe) that the widget subscribes to, so the widget is just another client and can be
built after the core; clicking an alert focuses the right session; quick-answer
buttons (allow/deny) only for prompts recognized with certainty, because they send
keystrokes to the agent. Detecting "needs input" is per agent, preferably through the
agent's native hooks or events rather than parsing the screen; to be verified agent
by agent.

Known platform limits (from my own knowledge, to be verified): always-on-top and
free placement are not guaranteed on Wayland; they work on macOS and Windows.
The character is Bungie's *Oni* IP; fine for personal use, a distribution decision
for later.

**Animation assets.** The user will generate them with GPT Image 2.5 (OpenAI,
September 2026): manga/anime style, always, few frames. My proposal: one neutral base
frame, every other frame an edit of the base changing only the mouth, eyes or arm,
so framing stays identical; seven actions, about 25 frames, transparent 512×512, 6–8
frames per second: idle (4), talk (3 mouth shapes), alert (4), done (4), thinking
(4), worried (3), sleep (3). The action list awaits the user's confirmation.

### 5.10 Roundtable (decided as a wish: "let different harness instances and models talk to each other and compare on a topic"; shape is mine)

The user puts several agent instances around one topic and lets them answer,
critique and converge. Shinatama is the natural broker: it already owns every
agent process and every sidecar session. Messaging, delivery, loop control, the MCP
interface and the roundtable runner are specified in sections 7 of the
[engine spec](technical-design.md).

My calls:

- **Reuse Ultra Nerd, do not reimplement it.** Daodan already has this protocol for
  bare models: the `moe` plugin ("Ultra Nerd", `moe:multi-model-deliberation`), an
  explicit opt-in because it multiplies token use, with an MCP server named `moe`
  (tools `moe_ask`, `moe_models`), run state under `.moe/<run-id>/`, and the
  `peer-review` plugin for challenging a plan with a second model family. Bare-model
  participants go through those tools (Shinatama's Daodan install already enables
  them). The roundtable adds what Ultra Nerd lacks: participants that are full agent
  instances with their own session, tools and context, and the UI around them. A cost
  warning is shown before a roundtable starts.
- **Moderated, structured rounds**, as in Ultra Nerd: independent proposals, then
  reciprocal critique, then a synthesis. The user can interject in any round, stop,
  or ask for another.
  A free-form chat between two agents is the same machinery with one reply per
  round. Whether the free-form variant is wanted is **open**.
- **Participants:** any mix of the four agents, each with its own model, including
  an agent pointed at a local model. The same agent may appear twice with different
  models.
- **Transport:** for roundtable turns use an agent's non-interactive or native API
  route where it has one, so a turn has a clear end and a clean answer. Typing into
  the PTY and reading the screen is the fallback, relying on native turn-finished
  signals. To be verified per agent.
- **Safety** (an anti-goal is agents steering each other): a message from a peer is
  always passed as quoted data from a named participant, never as an instruction or
  an authorization. Roundtable turns run **answer-only**, without tool permissions,
  by default; the user opts in per participant if it should inspect the repository.
  Hard caps per roundtable on rounds, tokens and cost. Each participant's traffic
  goes through the sidecar as usual.
- **Output:** the transcript is stored as a thread of its own, with a synthesis
  (agreements, differences, open questions). The user can hop any participant's
  position into a normal working session ("continue with codex's plan").

### 5.9 Replacing Musashi

Musashi's behaviour (menus, checkup, error policy) is the specification that
Shinatama must at least match, and its tests with real captured fixtures are the
model for the adapters. What goes away: external terminal selection. My calls:
Gemini keeps a launch-only adapter in v1 (Musashi launches it today and the sidecar
supports it; no hop, no search); `cronologia.json` is imported once. The code lives
in the new project `shinatama` (section 7). **Open**: Windows (assumed yes).

### 5.11 Accounts and login (decided as a wish: "recognize automatically or perform the login for codex, claude, etc."; shape is mine)

At first run and in `shinatama doctor`, each agent shows whether it is signed in. My
calls:

- **Detect, never collect.** The adapter asks the agent itself (its own status
  command or its own credential check). Shinatama does not read, copy, move or
  store an agent's credentials, and puts none of them in the sidecar's launch
  manifest.
- **Log in through the agent's own flow**, run inside Shinatama's terminal (browser
  sign-in, device code or key entry, whatever that agent uses). Shinatama only
  observes the outcome.
- Login state is visible where it matters: the setup screen, the project manager
  (an agent that cannot start shows why) and the hop dialog (a destination that is
  not signed in offers login before the hop, since the destination is prepared in
  the background).
- Each agent's authentication methods differ and change by version; the adapter
  declares what it can detect and what it can only launch. To be verified per agent.

Whether Shinatama should also hold provider keys once and share them across agents
is **open**; it would make Shinatama a credential store that must be protected.

### 5.12 Order of work and audience (my calls, to check)

Suggested order: (1) skin system, PTY host and threads; (2) checkup: agents, Daodan,
login detection; (3) sidecar parity; (4) project manager and launch; (5) search and
resume within the same agent; (6) hop: text for every pair with the preview first,
then native import agent by agent; (7) roundtable; (8) widget (the user generates the
animation assets in parallel). The overview (5.13) grows alongside: the instance
table right after (1), the summaries and flows once hop (6) and the roundtable (7)
exist.

Musashi stays the working launcher until items 1–5 reach parity on the user's
machine, each parity item verified; it is retired only then.

Audience: build as if other people will install it (the install instructions of
Daodan and Shinatama already point to public repositories), so release builds,
signing and notarization are part of the plan. The Oni-derived art (the default
skin's character, the widget) is a distribution decision separate from the code.

### 5.13 Global overview (decided as a wish: "a global view tab with all the instances, how they collaborate, what they are doing, a low-cost model titling their activity, the communication flows, the exchanges and the work"; shape is mine)

(States, events, titles and the shared database that feed this view are specified in
section 8 of the [engine spec](technical-design.md).)

One tab shows every agent instance started from or inside Shinatama: its agent and
model, project and thread, state (working, waiting for you, idle, done, error), what it
is doing, tokens and cost, how full its context is. Beside the table: the **flows**
between instances (hops of a thread, roundtable participants, who waits for whom), a
timeline of **exchanges and changes** (hops, rounds, approvals, errors, files
changed), and the **work** per shared folder, including files that more than one
instance is touching.

My calls:

- **What it is doing** is a short title written by a **low-cost model**, refreshed
  periodically and when an instance changes state. Shinatama makes the call, from the
  activity it already observes (events and the tail of the transcript); the agents are
  not asked to do it, since they cannot be made to do it reliably.
- **The model** is a choice among three backends (engine spec 8.4): a **small local
  model** run through llama.cpp at no token cost and with no credentials (the idea from
  the user's pointer `kouhxp/fftext`: a ~0.5 GB Qwen 0.8B GGUF on CPU; one-time opt-in
  download; quality unmeasured), a **provider low-cost model** chosen from the catalog of a
  provider the user is already signed in with, by capability and price, never by a model
  name written in the code, or **none** (the last tool or message verbatim). Frequency, a
  daily cost cap and an off switch are settings. The default is decided by a quality gate
  on real snapshots in the first milestone.
- **Privacy:** summaries send parts of a transcript to a provider. This is shown at
  first use, credential-like strings are redacted before sending (the builder defines
  how), and a thread can be excluded.
- **Credentials:** Shinatama holds none (5.11), so the call goes through the sidecar's
  existing auxiliary-call path (the prompt rewriter in `docs/prompt-rewrite.md` is the
  precedent) or through the agent's own non-interactive mode. The builder chooses and
  verifies.
- **Branch and worktree** (decided): the table shows, for each instance, the git
  branch and the worktree it works in (the main tree or a linked worktree), read from
  git, not guessed. Non-git folders show none.
- **"How they collaborate"** is built only from what Shinatama really knows: threads
  that hopped, roundtable groups, instances sharing a project folder. It warns when
  two instances change the same file in the same worktree; the same path in two
  different worktrees is not a collision. It does not infer intent.
- **Agent icons** (decided: every tab shows the icon of its agent): each agent has an
  icon and an accent colour, shown in the tab, the overview and the project manager.
  In a text UI an icon is a glyph or a Nerd Font symbol, or an inline image where the
  terminal supports an image protocol; there is always a plain-text fallback (letter
  badge), since terminals differ. The widget can show the real logos. How the glyphs
  or images are obtained and licensed is the builder's call.
- **Own worktree for a new instance** (my idea, optional): from the overview or the
  project manager, start a new instance in a new linked worktree so that two agents on
  the same project do not collide.
- The same event stream feeds the widget (5.8): an alert there is an event here.
- The overview reads. Its only actions: focus a session, hop, pause an instance's
  summaries.

## 6. Agent adapter contract (builder's design, requirements mine)

One adapter per agent (decided), so Grok or another agent can be added without touching
the rest. **Each adapter provides export, import, resume and handover, plus the rest:**
install and update the agent and the Daodan plugins in it; login and sign-in status;
start it in a PTY in a folder; list and read its sessions; **export** a session to a
portable file (`.stx.jsonl`); **import** such a file as a native session where the
agent allows it; **resume** a session natively; **handover**: the adapter's part of a hop,
including seeding a new session with a thread and confirming it arrived; and, where the
agent exposes it, report "needs input" and "turn finished" without screen parsing.
Capability differences per agent are declared and shown to the user, never hidden.
The full contract, the interchange format, the command line mapping and the conformance
suite are in section 5.5 of the [engine spec](technical-design.md).

## 7. Open questions for the user

1. **Windows in v1?** Assumed yes (my call, to confirm): the user asked for a project
   manager "as in Horness", which is Windows-only, and Shinatama replaces Musashi on
   the user's own PC; macOS and Linux stay required. If yes: ConPTY, Windows paths,
   environment and trust handling from the first milestone; if no, Musashi cannot be
   retired there.
2. **What does "assistant" mean for the widget?** The widget is in v1 (decided).
   My default for v1: animated character, speech bubble, alerts and quick answers
   (allow/deny); chat with the assistant and spoken voice later. Voice would add
   speech components and a graphical toolkit.
3. ~~Where does the code live?~~ **Decided:** a new project called `shinatama`
   (name decided on 2026-10-26; the earlier suffixes `-sld` and `-mux` were
   dropped), in this repository. The obsolete Daodan companion marketplace
   (acaprino/shinatama) was deleted on 2026-10-26 (the user's decision on register
   row D1); the local folder `D:\Projects\shinatama-sld` is renamed to
   `D:\Projects\shinatama` as soon as JanuraCode is not holding it (its services
   keep `.jcode` open). It is separate from Musashi in `D:\Projects\horness-launcher`,
   which stays the parity reference until retired. How the sidecar source
   (`sidecar/source-build`, owned by Musashi today) is brought in without forking it
   — a copy with provenance, a submodule or a published package — is the builder's
   call, but it must stay a single source.

### Calls made to settle the rest (check them; each is also marked where it appears)

- ~~A text UI inside the user's own terminal~~ — confirmed by the user (5.1, section 2).
- Hop staged: text hand-off for every pair first, native import agent by agent (5.2).
- Hop: close or fork the source at each hop, with a user setting for the default
  (default close); a fork continues in a new thread (5.2; engine 6.2, 6.3).
- Gemini launch-only; `cronologia.json` imported once (5.9).
- Others may install it: release, signing and notarization planned (5.12).
- Order of work, with Musashi retired only after parity (5.12).
- All Daodan plugins enabled by default, `research` included (5.5, user).
- Login: detect and run each agent's own login, never hold their credentials; sharing
  provider keys across agents is not in v1 (5.11).
- Roundtable: moderated rounds with a free-form two-agent variant, answer-only by
  default with per-participant opt-in to tools (5.10).
- Overview: titles written by a low-cost model that Shinatama calls (not the agents),
  chosen from the catalog of a provider already signed in or a local model, capped
  and switchable, through the sidecar's auxiliary path so Shinatama holds no
  credentials (5.13).

## 8. Left to the builder

Language-level choices (terminal UI library, PTY and terminal-emulation crates,
async model), the exact table and column definitions inside the engine spec's sketch
(the storage choice, SQLite per user, is decided), the optional wake-up socket, the
install mechanism on macOS and Linux (script, package manager, release binaries),
how Node is provisioned for the sidecar, the skin definition format, command names
and exit codes, test strategy (real-fixture tests per adapter).

## 8a. Verification

Everything marked unverified in this document and in the engine design is listed in
[the verification register](verification-register.md), with how to prove it, what
counts as proved and where the result is written back. A claim is not "done" until its row
is `passed` and the sentence here has been changed to match.

## 9. Mockups

Interactive HTML mockups, throwaway sample data, in the brainstorming session
folder `sidecar/runs/.../scratchpad/.superpowers/brainstorm/7059-1793031760/content/`:
`shinatama-skins-v2.html` (session, hop, find, CLI), `hop-honest.html` (hop with
honest carry-over and preview), `daodan-native.html` (Daodan setup and palette),
`project-manager-v2.html` (projects, default root, new project), `widget.html`
(widget states), `roundtable.html` (several agents compared on a topic), `setup-login.html`
(sign-in status and login), `overview.html` (the global view). Superseded: `daodan-flow.html` (it linked threads to Daodan
tasks, which the user rejected).
