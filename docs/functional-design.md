# Shinatama — design

Date: 2026-10-26; native tiling revision: 2026-10-29
Status: draft, not yet approved. Nothing is built from this document until it is.

Shinatama is a local-first native desktop workspace, written in Rust, that hosts the real
coding agents (Claude Code, Codex, OpenCode, Pi, Hermes Agent, DeepSeek Harness) in tiled terminal panes and lets the user hop
a live chat from one agent to another without starting over. This document lives in
the project `shinatama`.

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
found by searching and resumed in the same or another agent; and every launch keeps
the full resilience, safety and compression of the BillionContext sidecar (5.6).

## 2. Goals, non-goals, anti-goals

### Goals (decided unless marked)

1. Host Claude Code, Codex, OpenCode, Pi, Hermes Agent and DeepSeek Harness as real
   processes in a terminal multiplexed inside Shinatama (the six decided 2026-10-31,
   user: "deve supportare opencode, codex, claude code, pi, hermes e deep seek code").
   "Deep seek code" is read as DeepSeek Harness, DeepSeek's own open-source agent; no
   DeepSeek product is called DeepSeek Code (my call, section 7). Hermes Agent and
   DeepSeek Harness join with every adapter capability declared `unsupported` until
   its register row passes (engine 5.5 and 5.7; register sections 7a and 7b): nothing
   is claimed for them from documentation alone.
2. Seamless hop of a live thread between those agents (see 5.2).
3. Search all local sessions of all supported agents; resume from history in the
   same agent or in a different one. Interactive terminal flow plus a scriptable
   command line mode.
4. An integrated project manager (5.4), with a default root directory and a shortcut
   to create a new project.
5. Daodan arrives preinstalled and enabled in every agent (5.5), and Shinatama is a
   **native support for it** (decided 2026-10-27, user): it reads Daodan's tasks, links
   threads to them and carries a task across a hop. Read-only; Daodan stays the only
   writer of its state. See [daodan-native-support.md](daodan-native-support.md).
6. The BillionContext sidecar is integrated with its full resilience, security and
   compression (5.6).
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
13. Browser access for agents and the user (decided 2026-10-29): Moli by default
    for fast operations, local HTML, previews and MCP web interfaces; Patchright
    UI only as an explicit graphical path, with no Chrome resident and no
    automatic fallback or session migration (5.15).

14. Native tiling, draggable tabs, resizable splits and collapsible side panes
    (decided 2026-10-29; 5.16), with a Rust UI, not a web frontend.

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
- A web-built application shell: no HTML/CSS/JavaScript frontend, Tauri or Electron
  for the main UI (decided 2026-10-29). Native desktop tiling supersedes the earlier
  terminal-only decision. Agents still run in real PTYs, not simulated chats. A
  future isolated browser pane would not make the shell a web frontend.
- Automatic browser fallback or migration (5.15): no switching from Moli to
  Patchright after an error, no shared cookies/login and no resident Chrome or
  Patchright companion. Persistent `moli serve` and interactive Moli previews are
  outside the first integration until their security gates pass.

### Anti-goals (would be a failure even if it technically works)

- A wrapper that replays or simulates the conversation instead of running the agent.
- Promising a lossless transfer. The UI must always say what carries over and what
  does not. The words "native resume" are used only for resuming within the *same*
  agent.
- A second scheduler or a resident service that keeps executing work after the
  owning application window closes.
- A weakened sidecar integration: any loss of private root, child-only environment,
  child-only certificate trust or proxy recovery.

## 3. Constraints

- Rust. macOS and Linux are required (the original request); Windows is assumed
  required too (section 7, item 1).
- Why Rust: the user's original request named Rust; my reasons agree. Shinatama
  *hosts* agents: it needs a pseudo-terminal on all three systems and a terminal
  emulator, for which mature Rust crates exist (pseudo-terminal, escape-sequence
  parser, native UI); it runs many concurrent tasks (several terminals, sidecar, event
  stream, summaries); one toolchain builds and cross-builds for macOS, Linux and
  Windows. Not proved yet: that a Rust pseudo-terminal plus
  emulator hosts the six agents faithfully on Windows with ConPTY — the first
  milestone must prove it before the rest is built on it.
- Local-first: no account, no hosted service. Threads and settings live on disk.
- Every agent launch goes through the checkup: there is one launch point and it runs
  the checkup (my call; the user did not object).
- Agent behaviour that Shinatama depends on (resume flags, session stores, hooks,
  HTTP APIs) must be verified against the installed version of each agent with real
  captured fixtures. Documentation alone is not proof.
- The BillionContext sidecar source in `sidecar/source-build` stays the single
  source; Shinatama does not fork it, and does not install the precompiled npm
  package (docs/sidecar.md). The sidecar documents named in these designs
  (`docs/sidecar.md`, `docs/token-efficiency.md`, `docs/prompt-rewrite.md`) are in the
  sidecar's own source tree, not in this repository.
- No measured saving percentage may be claimed for compression
  (docs/token-efficiency.md says none has been measured).
- **Isolation, at a glance.** Nothing system-level is touched: the sidecar keeps a
  private root and applies its environment and its certificate authority to the launched
  child only; hooks and the MCP server are injected per launch, never by editing global
  configuration. Where each agent lives is set per agent (5.14): by default the user's
  installed program runs in a **box**, a home in Shinatama's folder, so the user's
  sessions, settings and plugins are untouched, Daodan included; with the `system`
  home it uses the user's own home, and then Daodan is installed into that agent's
  native plugin store, so the user's own agent runs see it too (the user's disabled
  plugins stay disabled). Each instance can also carry its own set of plugins and
  skills. Unproved parts: per-launch MCP injection, one proxy per instance, the
  `CODEX_HOME` overlay (register rows M1, SC5, CX12) and the box itself (D15 to D19).

## 4. Surfaces

Native Rust desktop surfaces: project manager, tiled workspace with agent tabs
and side panes (5.16), hop dialog and preview, find (search), Daodan
palette, first-run setup. Command line: `shinatama find`, `resume`, `hop`,
`doctor` and the rest of engine 5.5.2 (`sessions`, `export`, `import`, `forget`; names
are my call). Browser access: `/browse` with `read`, `preview`
and explicit `open-ui` operations (5.15; spelling is my call). Desktop widget:
separate process (5.8).

## 5. The parts

### 5.1 Runtime and threads on disk

Each agent runs in a pseudo-terminal owned by Shinatama; the user types into the
real agent. A **thread** is Shinatama's own record on disk: an id, the project
folder, the chain of agent sessions it has been through (agent, native session id,
time span) and enough to rebuild the hand-over context. The format and location are
the builder's call. Closing Shinatama does not lose a thread.

Shinatama reads each agent's own session store to search and to resume. Those stores
differ per agent and per version; the adapter for each agent owns reading them.

**The hosted agent lives in a terminal pane in the native tiled workspace
(decided; revised 2026-10-29, 5.16).** What Shinatama
draws around and over it:

- **Frame and bars:** border and title, a top bar (thread, folder, branch, cost, the
  agent's icon) and a bottom bar of keys, all coloured by the skin.
- **Side panels:** for example the thread column (what carries over in a hop) beside
  the agent. The agent is given a narrower terminal and resizes itself, so resizing is
  forwarded to it correctly.
- **Windows over the agent:** the hop dialog (`^H`), the Daodan palette (`^K`) and
  find. Native overlays leave the emulator screen intact and restore focus to the
  originating pane when dismissed. Key names in this document (`^H`, `^K`) are
  placeholders until register row U3 settles bindings that no agent or outer terminal
  consumes.
- **Terminal presentation inside the pane:** each view uses its harness's terminal
  profile (font, metrics, ANSI palette and cursor), so the agent keeps its own
  visual identity. Chrome uses the active skin. An explicit follow-skin option
  may remap the default and 16-colour ANSI palette; it never rewrites truecolour.

What stays the agent's: the text it draws (tables, tool calls, reasoning, choices) and
any colour it picks at 24-bit. Not yet verified, to be proved by the first milestone:
fidelity of the emulator against the real Claude Code, Codex, OpenCode, Pi, Hermes
Agent and DeepSeek Harness on macOS, Linux and Windows (ConPTY can alter escape sequences); wide characters and
emoji; alternate screen and scroll regions; mouse and advanced keys; passing agent
images (kitty or sixel) through to the outer terminal; no flicker; and how well the
colour remap reads in each skin.

Formatting of live work stays the agent's own. Where the data is already structured
and read-only — find, the hop preview, the roundtable, the overview — Shinatama draws
its own rich views. Redrawing the live chat itself from each agent's structured stream
is not planned, with one exception: DeepSeek Harness ships no terminal UI, so its live
view is a native ACP pane (5.16).

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

How the history reaches the destination — **v1 is seed-only (decided 2026-10-31,
user).** V1 ships the text hand-off for every pair of agents, with the preview, and
never writes a native session through Shinatama or Mukade. Native import is post-v1:
it is added one destination agent and version at a time, only when its fixtures,
privacy, store and recovery gates pass, because it writes into another agent's
session store. The hybrid remains the end state: where the destination's session
format can be written reliably, import the converted history as a real native
session; otherwise fall back to a readable text export. Native import is done by the
`mukade` interchange library — Shinatama's own reader and writer for the hosted
harnesses (four today; Hermes Agent and DeepSeek Harness are new Mukade work,
[mukade-handover.md](mukade-handover.md) section 9), published as its own repository and linked in-process (technical design
5.4.1). Until that post-v1 gate opens, every hop uses `seed_prompt` or
`native_resume`; a fork also uses a seed, never a native fork or import.

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

Full-text search over the sessions of every agent Mukade reads, filterable by agent,
project and date, with a preview of the thread. Enter resumes in the same agent
(native); another key resumes in a different agent (a hop). The command line mode
mirrors it for scripts, for example:

```
shinatama find "refresh token" --json | jq -r '.[0].id'
shinatama resume t_8f3a --agent codex
```

### 5.4 Project manager

Project roots whose first-level folders are the projects, sorted by last change;
incremental filter while typing; any folder via F3; a history of the last 10
non-standard folders, including folders that are not git repositories (Desktop,
shared drives).

Decided additions: a **default root** (where new projects are created; chosen at
first run, changeable) and a **shortcut (`n`) to create a new project**.

My calls: pinned projects; git branch and dirty-file count;
the last agent and last thread per project; the threads of a project listed
beside it; one key to resume the last thread; several roots. In the new-project
dialog: optional `git init`, and an option to type Daodan's native init command in
the chosen agent (never run automatically).

### 5.5 Daodan preinstalled (decided: option A, downloaded and kept updated)

Daodan is a set of plugins, already compiled for each agent: Shinatama converts
nothing, it only installs. The plugins go into the agent's home (5.14): the box by
default, the user's own home with the `system` home; an instance with its own set loads
them per launch instead.
Shinatama's checkup installs or updates it in every
agent present and enables, by default, **every plugin the Daodan marketplace lists**
(decided 2026-10-27, user: "mi aspetto che attivi tutti i plugin"; read from the
manifest at check time, 48 at marketplace 1.0.0): the entry plugin `daodan`, the
document-driven project management (project-lifecycle, project-delivery with ADR/FRD
under `docs/`, project-knowledge), codebase-map, senior-review, testing, the domain
verticals, `research` and the rest. A setting `daodan.enable = all | core | custom`
(default `all`; `core` is the entry plugin plus its dependency closure) keeps the
lighter choice available. The agents use them
through their own native entries: `/daodan:<workflow>` on Claude Code and OpenCode,
`<workflow>-workflow` on Codex, `/daodan-<workflow>` on Pi (docs/hosts.md of the
Daodan repository).

**Hermes Agent and DeepSeek Harness have no Daodan package (2026-10-31).** Daodan builds for
five hosts: Claude Code, GitHub Copilot, Codex, Pi and OpenCode (its README, "One source,
five hosts", marketplace 1.0.0). On the two new agents the checkup installs nothing and says
"Daodan: no package for this agent"; the palette offers no Daodan command there. Shinatama
never copies another host's build into them: a package Daodan did not compile and check for
that host would claim an execution nobody proved. The task view of the native support still
works for their threads, because that is Shinatama reading `.daodan/agenda.json`. Adding the
two hosts is Daodan's work, in its own adapters and exports (register D20); the day Daodan
ships them, the checkup treats them like the other five.

Daodan's own build for each host already carries its dependencies, so the checkup
installs that build and enables what its manifest lists; nothing is translated. The
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

The same BillionContext code, supervised by Shinatama. Acceptance is each item
below, verified under Shinatama and not assumed:

- private root; Canonical config, data, cache and state paths resolve under it;
- environment applied to the child process only;
- the proxy's certificate authority trusted only by the launched process, no
  system trust-store change;
- the launch manifest holds argv and launch overrides only, never credentials;
- automatic proxy recovery on the original address for active sessions; orderly
  drain on exit;
- one supervisor per proxy: Shinatama alone starts and restarts it, and a recovery
  starts one replacement; each session's process attaches to the proxy and never
  starts a replacement of its own;
- busy is not dead: a proxy whose process is alive and still holds its port while it
  misses health checks (a long compression can hold its event loop for a minute) is
  shown as busy and never duplicated; whether the port is held is checked by binding
  it, not by connecting, because on Windows a held proxy refuses new connections once
  its accept queue is full; recovery starts only when its process has exited or its
  port is free;
- the proxy never shuts itself down because its own event loop was held: the time it
  could not read its owners' heartbeats is not their silence;
- no sidecar diagnostic is written to an agent's terminal, where it would print over
  the agent's TUI: diagnostics go to Shinatama's log and to the proxy status shown
  in the window. The rule covers every line a launcher or proxy can write (owner
  renewal, attach refusals and stale-proxy rotation included) and is verified by
  capturing the agent's terminal while faults are injected, not by auditing messages
  (register SC15);
- no auto-update; the owned source upgrades only through Shinatama;
- compression policy as shipped (adaptive pressure, CCR, Absorb, profiles);
  `--no-context` remains an explicit per-session opt-out.

Agents covered in v1: Claude Code, Codex, OpenCode, Pi, Hermes Agent and DeepSeek
Harness; Gemini is launch-only (5.9). The last two count as covered only once the proxy
carries their provider traffic (register HM12, DS12): Hermes reaches many providers
(OpenRouter, Nous, Anthropic, OpenAI Codex, Gemini, Bedrock and more), DeepSeek Harness
the DeepSeek API plus the providers of its pi-ai layer.
The sidecar needs Node; how Node is provided on macOS and Linux is
the builder's call within the install story. A known interaction: the hop context
passes through the sidecar and counts against the destination's window; and the MCP
shim must follow `/resume` session ids (docs/sidecar.md, 2026-10-26).
The three supervision items above are not what the shipped launcher does when
several sessions share one proxy (observed 2026-10-31 with BillionContext 0.1.207):
each session's launcher watches the proxy itself, counts it dead after two missed
health checks of 1.5 s, starts a replacement that can only lose the port to the busy
proxy, and prints every attempt on its agent's terminal. On that day 170 such
replacements started, and the agents' screens filled with the messages. On 2026-11-01 a
launcher build whose recovery messages had been moved to a log still printed
`bili: watcher registration failed — ... (The operation was aborted due to timeout)` over
Claude Code's screen: the owner-renewal path had been missed, so fixing messages one path
at a time does not close the class. Shinatama therefore does not rely on per-session
supervision or on the launcher's own message discipline (technical design 9; register
SC6, SC12, SC14, SC15). Later that day the agents received `ECONNREFUSED` by two more
paths: the shared proxy shut itself down four times with every owner alive, counting
its own held event loop as their silence, and the launchers' connection checks read the
busy proxy as gone once its accept queue was full, then started replacements that lost
the port. Musashi Terminal's owned source fixes both (technical design 9; register
SC12, SC14, SC16).

### 5.7 Skins (decided: default is Shinatama pink; others selectable)

Skins are selectable at runtime and stored in the configuration; the UI colours come
from a skin definition, never hard-coded.

- **Shinatama (default):** midnight navy, cobalt/ice lines as in the repository
  logo, her pink hair as the only warm accent.
- **Section 9:** teal-black with cyan, faint scan lines.
- **TCTF:** steel grey with orange, cut corners, filled active tab.
- **Sumi:** ink black with vermilion.

In the native UI a skin controls the chrome: window, menus, tabs, side panels,
UI typography, borders and panel states. Agent terminals use separate terminal
profiles: each harness has a default monospace font and metrics, ANSI palette,
default foreground/background and cursor. The profile gives the harness a
stable identity inside its pane; it does not replace or reinterpret the TUI.
Hermes Agent gets a profile like the other four. DeepSeek Harness has no terminal, so
its ACP pane (5.16) is drawn with the skin and a harness accent colour instead.
A user may opt into **follow Shinatama skin**, which remaps only the default and
16-colour ANSI palette. Truecolour output chosen by the harness remains
untouched. Effects seen in the HTML mockups (scan lines, clipped corners, glow)
illustrate the mood and are not requirements.

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
- **Participants:** any mix of the supported agents, each with its own model, including
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

### 5.9 Gemini (my call)

Gemini keeps a launch-only adapter in v1: it starts through the sidecar, with no hop
and no search. External terminal selection is not offered (section 2, non-goals).
**Open**: Windows (assumed yes, section 7).

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
- Hermes Agent reports sign-in without secrets (`hermes auth list`, `hermes auth status
  <provider>`) and logs in with `hermes model` or `hermes auth add`. DeepSeek Harness has
  neither a login nor a status command: its state shows as **unknown**, and its login opens
  its own web UI (`dsh web`, on the box's home) in the user's browser, where the key or the
  account is entered (register DS9). It also reads a project's `.env`: a key there is used
  even in a box, and the setup screen says so.
- In a box (5.14) an agent's home starts signed out, so the first launch of each agent
  asks for one login through that agent's own flow; it then persists in the box and is
  shared by every instance of that agent. With the `system` home the user's existing
  login is used as it is.

Whether Shinatama should also hold provider keys once and share them across agents
is **open**; it would make Shinatama a credential store that must be protected.

### 5.12 Order of work and audience (my calls, to check)

Suggested order: (1) skin system, PTY host and threads; (2) checkup: agents, Daodan,
login detection; (3) sidecar; (4) project manager and launch; (5) search and
resume within the same agent; (6) hop: seed/text for every pair with the preview;
(7) roundtable; (8) widget (the user generates the animation assets in parallel);
(9) browser fast path, then the optional UI path after its gates (5.15). Native
import is post-v1 and is then enabled one destination agent and version at a time
after its fixtures and privacy, store and recovery gates (5.2). The overview (5.13)
grows alongside: the instance
table right after (1), the summaries and flows once hop (6) and the roundtable (7)
exist.

**Delivery baseline (2026-10-31).** The terminal-first UI milestone of 5.16 (TL1: project
selector, terminal tiles, tab groups, terminal profiles, no sessions surface) comes first.
The first usable release slice adds read-only store browsing/search and same-agent native
resume. The next slice is
the live seed hop. These slices do not drop the other v1 goals, but no downstream surface
is treated as deliverable in the same baseline before its dependencies and G0/G1 gates
are closed. Native import is post-v1; Daodan task handover, browser UI, roundtable and
widget each follow their own gates rather than blocking the first usable slice.

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
  The native UI can render icons directly, with a text label/letter badge fallback
  and accessible names independent of colour. The widget can use the same assets.
  Asset sourcing and licensing remain the builder's responsibility.
- **Own worktree for a new instance** (my idea, optional): from the overview or the
  project manager, start a new instance in a new linked worktree so that two agents on
  the same project do not collide.
- The same event stream feeds the widget (5.8): an alert there is an event here.
- The overview reads. Its only actions: focus a session, hop, pause an instance's
  summaries.

### 5.14 Where the agents live: program, home, per-instance set (decided 2026-10-28, user: "di base è con istanze private, ma poi c'è l'opzione per integrarlo con quello di sistema", then "si possono avviare anche se installate nel sistema, in una sorta di box per cui hanno le skill e i plugin solo per quella istanza?" — "ok"; shape is mine)

Three independent choices per agent, so the user can keep, for example, Claude's home
on `system` (subscription and history already there) and the others in a box.

**1. Program: `agents.<agent>.program = system | own`.** Default `system` when the agent
is installed, `own` otherwise.

- `system`: the binary the user installed (D18 found two native installers and two npm
  installs on this machine). Discovery reuses Mukade's harness catalog and installation
  scan where they cover the case (binary locations, default stores, portable layouts), so
  the recognition rules live once (my call; 5.16). Shinatama never installs, updates or replaces it; it only
  reports that an update exists, with the owner's command (`claude update`, `codex
  update`, or `npm install -g <package>@latest` with the package read from the npm
  shim). If the installed version is outside the proved range, the agent runs degraded
  and the user is told why. Nothing to download and no disk cost.
- `own`: Shinatama's copy, installed with npm `--prefix` (works for all four, D16, and
  DeepSeek Harness is an npm package too; Hermes Agent comes from its own installer with a
  bundled uv and Python, so its `own` recipe is HM10), when
  the agent is not installed or the user wants a pinned version. Updates install
  **next to** the running version: open instances keep the old one, new instances start
  on the new one, the previous version is kept for going back when the new one fails
  its fixtures (`fixture_gate`). The agent's own self-update and its update command
  are both blocked. Cost: hundreds of MB to over a GB per kept version (D16).

**2. Home: `agents.<agent>.home = box | system`.** Default `box`.

- `box`: a home in Shinatama's folder (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB`
  plus `XDG_*`, `PI_CODING_AGENT_DIR`, `HERMES_HOME`, `DSH_HOME` plus `DSH_AGENTS_HOME`).
  Plugins, skills, settings, sessions and login
  live there; the user's own agent runs see none of it, and the user's home is never
  written. Cost: one login per agent the first time (5.11). The user's existing sessions
  in the system home are still found: search and import read them, read-only, and a
  hop or import brings them into the box.
- `system`: the user's own home: same login, sessions and plugins, visible outside
  Shinatama too. Daodan is installed into the user's native plugin store (5.5).

**Forbidden combination: `own` program with `system` home.** Two versions of one agent
would write the same store, and a newer version can migrate it so that the older one no
longer reads it (OpenCode 2.x migrates its v1 tables to v2 once and stamps it,
docs/notes/2026-10-27-januracode-code-survey.md, item 3). The reverse, the user's
program in a box, is safe: only that program ever touches the box. If the user later
downgrades the system program, a box already migrated by the newer one may not open;
Shinatama shows it, never repairs it (inference, not tested).

**3. Per-instance set of plugins and skills.** Each instance (one tab) can carry its own
set, in either home, without an agent home of its own (OpenCode only needs an empty
per-instance `~`), so login and sessions stay shared: the launch
loads exactly the instance's plugins, skills and MCP servers and excludes the rest.
Verified on Windows for all four agents in a box (2026-10-28, record
[D19](verification/2026-10-28-D19-per-instance-sets.md)): two instances with different sets ran at once
on one home and shared its sessions; a resumed session takes the set of the instance
that resumes it.

- Claude Code: `--setting-sources project,local`, `--settings` with a copy of the
  user's settings minus the plugin list (leaving out the user source drops all user
  settings), `--plugin-dir` per plugin, `--mcp-config` with the set's servers (a set
  plugin's own server passed again) and always `--strict-mcp-config`.
- Codex: it cannot load a plugin for one launch, so Shinatama installs each set's
  plugins into the home once, turned off; each launch switches its set on and the rest
  off with `-c` (`plugins.<p>.enabled`, `mcp_servers.<s>.*`, `skills.config`,
  `skills.bundled.enabled=false`).
- OpenCode: `OPENCODE_CONFIG_DIR` (the set), `OPENCODE_CONFIG` (provider and model only)
  and `OPENCODE_TEST_HOME` (an empty home per instance, because nothing turns off
  `~/.agents/skills`). Its MCP tools reach the model from the second request on.
- Pi: `-ne -ns -np`, then `-e` and `--skill` per item; a box keeps `mcp.json` empty and
  each instance brings its MCP servers through its own extension (untested), since Pi
  has no per-launch MCP switch.
- Hermes Agent: no per-launch switch for plugins, skills, hooks or MCP, and a profile (`-p`)
  brings its own session store, so it would split the sessions. Until HM13 finds a way, a
  Hermes instance carries its box's set.
- DeepSeek Harness: a `--patch` overlay per launch (plugin and MCP rows) plus the ACP
  session's `mcpServers`; skills come from the box and the project (untested, DS13).

Switching any choice moves nothing: the other home's sessions stay readable and
importable.

**What a box takes, per agent** (verified on Windows 2026-10-28 with the system
programs, records [D15](verification/2026-10-28-D15-private-home-isolation.md),
[D16](verification/2026-10-28-D16-agent-install-update.md),
[D17](verification/2026-10-28-D17-opencode-xdg-children.md) and
[D18](verification/2026-10-28-D18-system-agent-discovery.md)): the home
variable alone is not enough. Every agent also gets `TEMP`/`TMP` (and, for the
Node-based ones, `NODE_COMPILE_CACHE`) inside Shinatama's folder, plus:

- Claude Code: `CLAUDE_CONFIG_DIR`. On Windows and Linux the login is a plain-text file
  in the box; on macOS a Keychain entry per box. Daodan installs plugin by plugin; the
  full set needs its dependency marketplaces from the network.
- Codex: `CODEX_HOME` on a **short path** (its daemon socket must stay under about 108
  characters); the daemon and its socket follow `CODEX_HOME`, so a boxed Codex never
  meets the user's daemon. **Known leak, shown to the user:** its unelevated Windows
  sandbox writes `~\.sbx-denybin` (four stub files) into the user's profile, as a
  system Codex already does; its elevated sandbox creates system users and firewall
  rules and is not box-safe.
- OpenCode: `OPENCODE_DB` plus the four `XDG_*` (no narrower set exists in 2.1.4) and a
  small Shinatama plugin in the box that gives the programs OpenCode starts back the
  user's own `XDG_*` (otherwise git loses a configuration kept under `~/.config`, D17);
  `OPENCODE_DISABLE_CLAUDE_CODE=1` (otherwise it loads the user's `~/.claude` skills);
  Shinatama starts and stops OpenCode's background service per box. The login is a
  table in its database.
- Pi: `PI_CODING_AGENT_DIR`; `--no-skills` or `HOME` for the child (it reads
  `~\.agents\skills`). Login in `auth.json` in the box.
- Hermes Agent (not yet run, HM9): `HERMES_HOME`; the box's `config.yaml` carries
  Shinatama's hook and MCP entries (engine 5.7) and an empty `skills.external_dirs`, so
  `~/.agents/skills` stays out; Shinatama starts and stops the box's gateway, which
  otherwise keeps running after its terminal closes. Login in `.env` and `auth.json` in the
  box.
- DeepSeek Harness (not yet run, DS9): `DSH_HOME` and `DSH_AGENTS_HOME` (without the second
  it reads `~/.agents/skills` and `~/.agents/AGENTS.md`). Login in `.credentials.yaml` in
  the box.

A box is created with owner-only permissions (0700 on macOS and Linux, a current-user ACL
on Windows, the rule of engine 4.1), because it holds the agent's login and sessions (my
call; D15 records the permissions it finds).

With the `own` program, also: Claude `DISABLE_AUTOUPDATER=1` and `DISABLE_UPDATES=1`;
OpenCode `OPENCODE_DISABLE_AUTOUPDATE=1` (left on, it would update the user's global npm
install); Pi `PI_OFFLINE=1` (or `PI_SKIP_VERSION_CHECK`) and `PI_TELEMETRY=0`, run as
`node …\cli.js`, Node ≥ 22.19.0; Codex's three update paths (startup check, `codex
update`, the daemon's own loop) are not yet controllable, so `own` Codex waits for D16.

Box leaks found with the per-instance sets (D19): Claude writes MCP server logs to
`%LOCALAPPDATA%\claude-cli-nodejs\Cache` (`CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES`
untested).

Still unproved: login persistence in a box (needs the user's sign-in), the interactive
mode with per-instance sets, the one-week self-update observation and OpenCode side by
side for `own`, macOS and Linux.

### 5.15 Browser integration (requirements decided 2026-10-29; shape proposed)

**Moli is the lightweight default; Patchright UI is explicit and on demand.**
The detailed functional and technical contract is
[browser-integration.md](browser-integration.md).

- Agents and the user can read authorized local HTML and web targets, produce
  static previews and use bounded MCP web-interface operations through Moli.
  Text extraction does not enable layout or optional visual resources; a preview
  does. General agent-provided JavaScript evaluation is not exposed initially.
- The first Moli path is one-shot `moli fetch`, not `moli serve`. Interactive Moli
  previews and persistent CDP/WebDriver automation wait for the transport and
  hostile-content security gate (B4).
- `open-ui` starts Patchright with visible Chrome only at the user's explicit
  request. It supports user-driven login/MFA/CAPTCHA and normal graphical use in
  a separate profile. Patchright is a driver, so its companion is lazy-loaded;
  neither it nor Chrome is resident at startup or installed automatically.
- `/browse` or its preinstalled skill chooses from the user's intent and displays
  engine, reason and scope. Agents may use existing bounded read/preview grants;
  they cannot open UI, broaden scope or authorize themselves. Ambiguous sensitive
  requests ask for clarification.
- **No automatic fallback:** a Moli error, timeout or unsupported page is shown
  as such. The user may issue a new explicit Patchright command, but this starts
  a new session without transferring cookies, login or control.
- Static visual output uses native preview panes for supported artifacts, otherwise
  a private artifact plus textual alternative. Text alone is not a visual preview.
  Interactive browsing remains external Chrome in the first integration; tiling
  does not implicitly approve an embedded browser engine.
- Local roots, MCP/local services, redirects and subresources have explicit scope;
  `localhost` is not blanket permission. Temporary profiles are the default and
  screenshots/extracted content follow secret-aware retention rules.
- One-shot operations can be independent within resource limits. An interactive
  UI session has one controller, explicit user/agent handover and verified
  whole-tree cleanup after close, cancellation or crash.

No speed or memory saving is claimed before B10 measures equivalent workloads on
the whole process tree. B1–B10 in the register define the evidence required. The
reduced first scope and the eventual performance budget remain subject to overall
design approval.

### 5.16 Native tiling workspace (decided 2026-10-29)

Shinatama has a native Rust desktop UI, not a web frontend. Tabs and tiling coexist:
project workspaces contain horizontal/vertical splits with tab groups at the
leaves. Side panes are resizable and collapsible. Earlier terminal/HTML mockups
are interaction references, not binding implementation choices. Toolkit selection
and exact shortcuts remain builder proposals.

**Initial terminal-first scope (2026-10-30):** project selector, resizable terminal
tiles, leaf tab groups and per-harness terminal profiles. Git integration, file
navigator, diffs, thread side panes, session browsing, hop and Daodan surfaces are
not part of this first UI. The rest of this section specifies later workspace
surfaces.

**Interactions and lifecycle:**

- Drag splitters to resize; drag tabs to reorder, move between groups or create
  an edge split. Preview valid drop targets; cancelled/invalid drops change nothing.
- Provide keyboard equivalents for focus, resize, tab moves, splitting and side
  pane collapse/restore. Ordinary input goes to the focused agent; workspace
  actions use documented explicit shortcuts or a command palette.
- Temporarily maximize a pane and restore its prior layout. Enforce minimum usable
  sizes; smaller windows must not discard panes or make them unreachable. Save
  and restore the layout per project and window workspace.
- Moving/hiding/collapsing a terminal view never restarts/stops its agent, creates
  a session or changes its thread. Closing a view is distinct from stopping a live
  instance; stopping requires confirmation. Dirty documents offer save/discard/cancel.
- Restore view/resource references, never stale process handles. Saved layout alone
  must not start/resume agents, execute apps or reopen interactive browser sessions.
  Show unavailable resources explicitly.

**Panel contents:**

| Surface | Behaviour / boundary |
| --- | --- |
| Agent terminal | Real PTY/emulator, resize, selection, scrollback and focus isolation; multiple agents visible together. Each view uses its harness's terminal profile (font, cell metrics, ANSI palette); Shinatama chrome keeps the active skin. |
| File navigator | Project tree, search and explicit open in a tab/pane; no implicit execution or arbitrary-root access. |
| Markdown | Rendered reading, source editing and source/preview split. Direct visual editing is requested but gated by editor/syntax fidelity checks before delivery. |
| File/document preview | Supported text/image artifacts. DOCX reading needs a separate renderer and declared fidelity; DOCX editing/full Office support is not implied. |
| Browser / integrated apps | Static previews fit 5.15. Interactive embedded browsing and mini-app hosting need separate design/security gates; JanuraCode app compatibility is not assumed. |

**DeepSeek Harness pane (2026-10-31, my call).** DeepSeek Harness ships no terminal UI:
its interactive surface is a browser page (`dsh web`), and its other modes are ACP and
SDK over stdio (engine 5.7). So its tab is a native **ACP pane**: Shinatama runs `dsh
--profile acp` and is its ACP client, the way an editor is, drawing messages, tool calls
and permission requests with the active skin and answering permissions through ACP. The
agent's loop, tools, models and session store stay its own, so this is not the simulated
chat of the anti-goals (2). The pane has the same tab, split, hop and overview behaviour as a
terminal tab. `dsh web` is offered as an explicit "open in browser" action in the user's
browser for what the pane lacks (the web UI's fork and export), never embedded (5.15).

**Sessions surface (decided 2026-10-30; revised 2026-10-31).** The workspace can
browse the sessions of every harness Mukade reads and open one in a detail panel. It is not
part of the first terminal-first milestone. Read-only browsing is the first slice;
live seed hops follow after their hop gates. Native Import and native handoff are
post-v1 capabilities and remain unavailable until their separate gates pass (5.2).
The supported Mukade revision must provide a listing reader for every harness shown (four
today; Hermes Agent and DeepSeek Harness appear in the tree once Mukade reads them); the declared
dependency pin, not an arbitrary local checkout, defines that capability. Mukade
revisions after the pin (installation scan, two-home hops) count only once the pin is
advanced and rechecked ([mukade-handover.md](mukade-handover.md), section 8).

- **Tree and store selection.** A Projects/Sessions switch in the navigator;
  harness nodes expand to their sessions with one metadata-only incremental filter
  and a manual rescan (`r`) that works only while the navigator has focus. Arrow
  keys move in the tree; they never steal input from a terminal pane. The adapter
  owns store resolution and capabilities and delegates parsing to Mukade. A catalog
  of persisted `store_root` records identifies box and system roots, their harness,
  origin, configuration revision and read/write permission. Discovery of system and
  portable roots may use Mukade's harness catalog and installation scan, one recognition
  rule set and no second parser; which discovered root becomes a box or a read-only
  system root is Shinatama's decision, recorded in the catalog (my call). The tree can read both
  an agent's box and the system roots exposed read-only by `system_homes()` (5.14);
  it never chooses a root from an arbitrary instance or ambient defaults.
- **Detail and metadata.** A `session_detail` panel with Overview, Transcript and
  Lineage tabs, in-session search, collapsible tool calls, a raw-JSON toggle and
  copy of the id, native path and resume command. Opening a session is read-only:
  it never resumes, imports or writes. Tree and Overview display only fields the
  adapter can obtain for that harness and store; unavailable title, cwd, dates,
  turns or byte counts are `unknown`, never inferred by loading every transcript.
  Native children are included where the reader supports them, not only root
  sessions.
- **Read path and resource limits.** Reads run off the UI thread against an
  explicitly selected `store_root_id`; the panel names the store and shows an
  unavailable or stale state when a root or session is missing or changes. The declared
  Mukade pin provides listing and whole-session reads, **not** bounded paging. Until a
  reviewed revision exposes incremental reads and bounded subprocess capture, or the
  adapter owns an equivalent bounded native reader, the adapter must enforce a pre-read
  size cap and return `too large` without loading the session. The paged adapter contract
  is a required new capability, not a feature attributed to the current pin; limiting only
  rendering or cache is never sufficient. Responses carry `(request_id, panel_revision,
  store_root_id, native_session_id)` and late or cancelled results are discarded. Where a reader
  launches a harness command, such as OpenCode export, it inherits the adapter's
  complete isolated launch context, timeout and cleanup contract; a read-only UI
  label alone does not make that child side-effect free.
- **Privacy and export.** Raw records can contain credentials, private paths and
  tool arguments. Raw display and copy are explicit actions with a privacy warning
  and redaction by default; an override reveals the selected records only. Raw and
  copied content is never logged or persisted by the UI. Export is separate from
  browsing and offers explicit **STX**, **portable Mukade JSON** and **native**
  choices where supported. Each declares format version, raw/unknown-field and
  attachment handling, redaction, losses, overwrite and atomic-output behaviour;
  portable Mukade JSON is not represented as a lossless backup.
- **Write paths.** Resume starts a PTY through the adapter. `hop_live` operates on
  a hosted instance/segment and follows close/fork, cursor, idle and catch-up rules
  (5.2). `handoff_history` operates on a historical native session: it creates a
  derived thread and source segment without closing a process, waiting for idle or
  inventing a live activation; its source fingerprint is rechecked before use. In
  v1 both paths use only seed delivery; native Import and native handoff are
  post-v1. Before any post-v1 write, the dialog shows the exact staged payload and
  known losses, obtains consent and journals the reserved destination id/path
  where supported. The receipt is persisted even when launch fails; uncertain
  writes are reconciled against that reservation, never blindly retried. A
  headless resume is a separate, cost-warned action.
- **Stores and executables.** Home and write destination are configured per agent
  (box/system), not per instance (5.14). Additional system roots are read-only and
  cannot become a box's write destination. A same-harness transfer between two homes (box to
  system, or into a portable installation) is not offered until SS5 verifies it; Mukade
  revisions after the pin address two stores of one harness (`hop_into`), so the gate is
  Shinatama's verification, no longer the library's shape. Executable overrides are adapter facts.
  Settings persist in the shared database; unresolved roots, missing executables
  and refused stores are reported, not replaced by ambient defaults.
- **Lineage.** The tab distinguishes the Shinatama thread chain, native parent and
  native children. Native relations are shown only where the reader exposes them
  (Pi and OpenCode in the declared Mukade revision); it does not infer edges for
  Claude or Codex. Missing, external, ambiguous parents and cycles are marked.
  Clicking a node opens that session or thread; it does not resume it.

**Acceptance of the read-only slice:** every harness the declared Mukade pin reads (the four
today, all six once the Hermes Agent and DeepSeek Harness readers land) listed with
its explicitly resolved stores, including native children where
supported; the user's own stores and side files stay untouched, including child
process effects; no native write from browsing, opening, searching or copying;
id, path, store and available metadata correct, with unavailable fields explicit;
with the current whole-session reader, oversized input returns `too large` before the
read; after the incremental-reader gate, a 30 MB transcript meets a recorded
end-to-end memory and responsiveness budget and larger input pages or is refused
without unbounded allocation; stale, cancelled and concurrent reads cannot update
the wrong panel; redaction and export fixtures contain planted secrets; tree and
detail restore without resuming anything; keyboard focus stays with the terminal
unless the navigator or a dialog is focused. Live seed hops add preview, consent, journal and recovery acceptance.
Post-v1 native writes additionally require an exact staged preview, a persisted
reserved destination, reconciliation after every crash point and no duplicate
session on double-click or retry.

**Terminal presentation:** the harness owns what appears inside the PTY: layout,
glyphs, spacing, prompts, boxes and colours. Shinatama does not redraw those
widgets or pretend they are native controls. The terminal profile controls only
what the emulator owns: monospace font and cell metrics, ANSI palette, default
foreground/background and cursor. If a profile font is unavailable, the terminal
falls back to the built-in monospace face without changing the PTY session.
Following the Shinatama skin is an explicit compatibility option, not the
default.

Editing provides undo/redo, visible dirty state and explicit save. Detect external
changes, including agent writes: preserve the buffer and offer reload/compare/
resolution, never silent overwrite. Markdown rendering must not execute scripts,
embedded commands or unapproved remote requests. Visual editing must preserve
unsupported syntax rather than silently discard it.

**Acceptance:** demonstrate nested splits/tab moves without session changes,
keyboard-only layout control, per-project/window restore without process auto-start,
real-agent resize and input isolation, dirty/external-write protection and safe
Markdown. Verify macOS/Linux and assumed Windows, including scaling, IME/Unicode,
small windows and many active panes. Engine 3.1 defines the technical prototype
gate. This revision specifies behaviour, not an implemented or validated feature.

## 6. Agent adapter contract (builder's design, requirements mine)

One adapter per agent (decided), so Grok or another agent can be added without touching
the rest. **Each adapter provides export, import, resume and handover, plus the rest:**
install and update the agent (only Shinatama's `own` copy, never the user's program)
and the Daodan plugins in the home chosen in 5.14; the per-instance launch switches for
plugins and skills (5.14); login and sign-in status;
start it in a PTY in a folder; list and read its sessions; **export** a session to a
portable file (`.stx.jsonl`); **import** such a file as a native session where the
agent allows it (post-v1, engine 5.5 C); **resume** a session natively; **handover**: the adapter's part of a hop,
including seeding a new session with a thread and confirming it arrived; and, where the
agent exposes it, report "needs input" and "turn finished" without screen parsing.
Capability differences per agent are declared and shown to the user, never hidden.
The full contract, the interchange format, the command line mapping and the conformance
suite are in section 5.5 of the [engine spec](technical-design.md).

## 7. Open questions for the user

1. **Windows in v1?** Assumed yes (my call, to confirm): the user works on Windows;
   macOS and Linux stay required. If yes: ConPTY, Windows paths, environment and trust
   handling from the first milestone.
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
   keep `.jcode` open). How the sidecar source
   (`sidecar/source-build`, kept today in another repository) is brought in without forking it
   — a copy with provenance, a submodule or a published package — is the builder's
   call, but it must stay a single source.

### Calls made to settle the rest (check them; each is also marked where it appears)

- Native Rust desktop tiling supersedes the earlier terminal-only decision
  (2026-10-29, 5.16); real agent PTYs and CLI mode remain. Sessions browsing is a
  later surface: read-only first, live seed hops after their gates, native import
  and native handoff post-v1 (revised 2026-10-31, 5.16).
- Hop: v1 is seed/text-only for every pair, with preview (2026-10-31, user);
  native import is post-v1, enabled agent by agent and version by version (5.2).
- Store discovery reuses Mukade's catalog and installation scan; Shinatama decides what
  each root is used for (5.14, 5.16).
- Hop: close or fork the source at each hop, with a user setting for the default
  (default close); a fork continues in a new thread (5.2; engine 6.2, 6.3).
- Gemini launch-only (5.9).
- "Deep seek code" is DeepSeek Harness (`dsh`, deepseek-ai/deepseek-harness), DeepSeek's
  only official agent; "hermes" is Hermes Agent by Nous Research (goal 1, 2026-10-31).
- DeepSeek Harness runs in a native ACP pane, since it has no terminal UI; its web UI only
  on explicit request, in the user's browser (5.16).
- Hermes Agent and DeepSeek Harness get no Daodan package until Daodan builds one; nothing
  is copied from another host (5.5).
- Until Mukade reads them, Hermes Agent and DeepSeek Harness resume natively and receive
  seeds, but are not searched and are not hop sources (engine 5).
- Others may install it: release, signing and notarization planned (5.12).
- Order of work (5.12).
- All Daodan plugins enabled by default, `research` included (5.5, user).
- Login: detect and run each agent's own login, never hold their credentials; sharing
  provider keys across agents is not in v1 (5.11).
- Roundtable: moderated rounds with a free-form two-agent variant, answer-only by
  default with per-participant opt-in to tools (5.10).
- Agent placement defaults (5.14, 2026-10-28): program `system` when the agent is
  installed, home `box`.
- Codex in a box despite its sandbox leak (`~\.sbx-denybin` in the user's profile),
  shown to the user rather than blocked; its elevated sandbox is not offered in a box
  (5.14).
- Per-instance sets (5.14): for Codex, Shinatama installs each set's plugins into the
  home once, turned off (a home write); for OpenCode, each instance gets an empty `~`
  of its own; for Pi, the box keeps `mcp.json` empty and each instance brings its MCP
  servers through its own extension; a resumed session takes the set of the instance
  that resumes it.
- Inherited markers (engine 3): the union of every supported agent's list is removed before
  any launch; `GIT_EDITOR` only when its value is Claude's helper.
- Overview: titles written by a low-cost model that Shinatama calls (not the agents),
  chosen from the catalog of a provider already signed in or a local model, capped
  and switchable, through the sidecar's auxiliary path so Shinatama holds no
  credentials (5.13).

## 8. Left to the builder

Language-level choices (native Rust UI library, PTY and terminal-emulation crates,
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
