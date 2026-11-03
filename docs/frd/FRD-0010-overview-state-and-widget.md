# FRD-0010: Overview, instance state and desktop widget

| | |
|---|---|
| Version | 1 (Markdown record; JSON record version 1 at `c84d28e`) |
| Status | Accepted 2026-11-01 (this Markdown text; the JSON evidence record, where one exists, stays Proposed until `frd_tool.py promote`) |
| Date | 2026-11-01 |
| Sources | functional-design goals 9 and 12, 5.8, 5.13, 7; technical-design 4.2 (`instance`, `event`, `summary_log`), 4.6, 8.1 to 8.5, 12; specs/token-savings 4.4.3, 7 (shared runtime) |
| Governing ADRs | ADR-0027, ADR-0028 |
| Register sections | I1 to I10, W1 to W10, Q3, Q8, O2, O3 |
| Related records | FRD-0003 (no credentials for titles), FRD-0008 (task alerts), FRD-0009 (auxiliary call path), FRD-0012 (companion rows and badges), FRD-0013 (shared local runtime), FRD-0014 (event feed) |

## 1. Purpose and scope

One tab shows every instance started from or inside Shinatama, what each is doing, how they
collaborate and what work touches which files; a desktop widget shows Shinatama as an assistant
that alerts and takes quick answers. Both are clients of one event feed. This record states the
instance states and their trust order, the hook command, the event feed, the titles by a
low-cost model, branch and collision facts, and the widget's rules.

## 2. Requirements

### 2.1 Instance state

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | An instance is in one of `starting`, `idle` (waiting for the user), `working`, `waiting_permission`, `waiting_question`, `error`, `exited`. | S | V1 | I1 | 0027 |
| R02 | The state comes from the most trusted source available, recorded in `state_source`: the agent's native hook or event stream (Claude hooks through `--settings`; Codex TUI notifications in the PTY stream, legacy `notify` and app-server status; OpenCode's SSE feed; Pi's `-e` extension; Hermes's shell hooks in the box's `config.yaml`; DeepSeek's ACP notifications), then the structured transcript tail, then PTY heuristics (8 s of silence after activity means probably `idle`), always labelled `unsure`. Each native signal is a candidate until its fixture passes. | S | V1 | I1, CL8, CX7, OC5, PI4, PI6, HM4, DS4 | 0027 |
| R03 | `waiting_permission` is never inferred from screen text in a way that lets the widget send keystrokes. Pi never reaches `waiting_permission` (it has no permission prompts) but can be `waiting_question`. | S | V1 | I1, PI6 | 0027 |
| R04 | Where a harness reports the pending request natively, the `state` event for `waiting_permission` and `waiting_question` carries it (tool and arguments, or the question and its options), redacted; this is unverified per harness and is what Aided Decision cards need (FRD-0012). | P | V1 | CS-10 | 0027, 0003 v2 |
| R05 | Liveness: every owning process updates `instance.last_seen` every 5 s; an instance is dead when `last_seen` is older than 20 s and its `owner_pid` with `owner_start_time` is gone; a reaper, run by any process at start and every 30 s, marks dead instances `exited` and closes their activations with `end_reason = crashed`. Laptop sleep causes no false "dead". | S | V1 | S8 | 0008 |

### 2.2 Hook command and event feed

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R06 | `shinatama hook <agent> <event>` reads the agent's JSON from stdin, writes an `event` row and the `instance` update in one transaction, prints whatever the agent expects on stdout (pending messages included), exits in under 100 ms and never fails the agent: errors are logged and swallowed, exit 0 unless the agent's contract says otherwise (Claude's documented hook exit codes are M11). It refuses a mismatched instance token. | S | V1 | M11, CL8 | 0027 |
| R07 | One feed, `event(seq, ts, instance_id, kind, payload)`, kinds `state`, `hop`, `message`, `summary`, `alert`, `file_change`, `file_collision`. The overview, the widget and the roundtable view are consumers that remember their last `seq`; a process wakes by polling `PRAGMA data_version` every 200 ms, with a one-byte ping over a named pipe or Unix socket as an optional later optimization. | S; A (polling) | V1 | S3, W4 | 0027, 0008 |
| R08 | `event`, `summary_log` and `message_event` get a pruning policy before release. | S (open Q8) | V1 | I8 | 0009 |

### 2.3 The overview tab

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R09 | One tab shows every instance started from or inside Shinatama: agent and model, project and thread, state, what it is doing, tokens and cost, context fill. Tokens, cost and fill come from each agent's own records (Claude `cost-state`, Codex `token_count`, OpenCode session columns, Pi `usage`) and the window per model. | U (goal 12); A (columns) | V1 | I2 | 0028 |
| R10 | Beside the table: the flows between instances (hops of a thread, roundtable participants, who waits for whom), a timeline of exchanges and changes (hops, rounds, approvals, errors, files changed), and the work per shared folder including files more than one instance touches. | U | V1 | — | 0028 |
| R11 | "How they collaborate" is built only from what Shinatama knows (threads that hopped, roundtable groups, instances sharing a folder) and infers no intent. | A | V1 | — | 0028 |
| R12 | For each instance the table shows the git branch and the worktree (main tree or linked worktree), read from git at start and at each `idle` transition (`rev-parse --abbrev-ref HEAD`, `--show-toplevel`, `--git-dir`, `status --porcelain=v1 -z`), never guessed; non-git folders show none. Cost on large repositories is measured. | U (branch and worktree shown) | V1 | I3 | 0028 |
| R13 | Collisions: a modified path is attributed to an instance from the tool-call targets in its events and transcript; two instances with equal worktree roots both attributed a modified path raise a `file_collision` naming both; a path that cannot be attributed while two or more instances work in that worktree raises a "possible collision" that names no instance; generated files are excluded; equal paths in different worktrees do not collide. | U 2026-11-01 | V1 | I4, Q3 | 0028 |
| R14 | The overview is read-mostly: its only actions are focus a session, hop, and pause an instance's summaries. Starting an instance in its own worktree is an option of the new-instance command (FRD-0004), not an overview action. | U 2026-11-01 | V1 | — | 0028 |
| R15 | Every tab shows its agent's icon and accent colour; the overview and the project manager show the same (FRD-0001 R48). A companion shows as a child row of its worker (FRD-0012). | U (icons) | V1 | I10 | 0028 |
| R16 | The overview grows alongside the milestones: the instance table right after the PTY host, summaries and flows once hop and roundtable exist. | A | V1 | — | 0040 |

### 2.4 Titles by a low-cost model

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R17 | "What it is doing" is a one-line title of at most 12 words written by a low-cost model that Shinatama calls, from the activity it observes (events and the transcript tail); the agents are not asked to title themselves. | U (low-cost model); A (Shinatama calls it) | V1 | I5 | 0028 |
| R18 | Refresh on every state change and every 2 minutes while `working` (defaults; the frequency is a setting). The input is at most 6 KB of redacted transcript tail (tool names and targets, last assistant lines, no full files); tokens and cost are logged in `summary_log`. | A | V1 | I9 | 0028 |
| R19 | The titler is an interface with three backends, one active, chosen by the user: a small local GGUF model through llama.cpp (the idea of `kouhxp/fftext`, not its code; about 500 MB, one-time opt-in download with the licence shown; zero token cost, offline, no credentials); a provider low-cost model chosen from the catalog of a provider the user is signed in with, by capability and price and never by a name in the code, through the sidecar's auxiliary path or the agent's non-interactive mode; or none (the last tool or message verbatim). | A (backends); U 2026-11-01 (three choices) | V1 | I5, I6, I7, SC10 | 0028 |
| R20 | Default: the local model is offered at first run after consent with its licence shown; if declined or not downloaded, none; the provider model stays a user choice. The quality gate (backends 1 and 2 on about 50 recorded activity snapshots with a written rubric: names the right file or action, no invented fact, at most 12 words) confirms the default. | U 2026-11-01 (O2) | V1 | I5 | 0028 |
| R21 | A daily cost cap for paid backends (default $0.50) and an off switch exist; at the cap, off or with no backend, the title is verbatim. | A | V1 | I7 | 0028 |
| R22 | Privacy: at first use the user is told that summaries send parts of a transcript to a provider; credential-like strings are redacted before sending; a thread marked `summaries_excluded` is never sent to any backend; with default settings no title call leaves the machine. | A | V1 | I9 | 0028 |
| R23 | The local title model and julia1 (FRD-0013) share the llama.cpp runtime (a Rust binding or a bundled `llama-server`, the builder's choice); when one model serves both there is one opt-in download; titles go through julia1 only when it is the chosen local model. CPU, memory and latency with four agents running, the model licence, and the download size and hash are measured and recorded. | U 2026-11-01 | V1 | I6 | 0028, 0037 |

### 2.5 Desktop widget

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R24 | V1 includes a desktop widget: a small borderless, transparent, always-on-top window that brings up Shinatama's face like her communications portrait in Oni, with an animated expression and a short bubble identifying the harness/session and its question or update. It alerts when an agent needs input, an operation finishes or something important happens (the context filling up). The full event-feed client is built last within v1; the user brought forward a first Windows preview on 2026-11-03 (C46), in the Tauri process with bounded transient local alerts. The preview does not satisfy R25's separate-process/shared-feed requirement. | U (in v1; portrait, alerts and early preview 2026-11-03); A ("assistant" scope, open O3) | V1 | W1 | 0027 |
| R25 | The widget is a separate process and just another client: it reads the `event` table after its last `seq`, with an optional socket or pipe used only as a wake-up ping and no other channel; it works with no Shinatama window running and with several windows; an alert reaches the bubble in under 1 s. | A | V1 | W4 | 0027, 0008 |
| R26 | Clicking an alert focuses the right session. | A | V1 | W4 | 0027 |
| R27 | Quick-answer buttons (allow, deny), which send keystrokes to the agent, exist only for prompts reported by a native event; a quick answer goes only to the instance and prompt it was offered for, the bubble shows the real command from the native event, and a prompt that changed between the event and the click is refused. | A | V1 | W5 (SEC) | 0027 |
| R28 | The widget never takes keyboard focus and never steals keystrokes when an alert appears. | A | V1 | W3 (SEC) | 0027 |
| R29 | Bubble text comes from titles or the last assistant text, redacted, bounded in length and in the interface language. | S | V1 | W6 | 0027 |
| R30 | A configurable brief attention sound accompanies alerts; mute suppresses sound while keeping visual alerts, and a separate switch disables the widget. The reduced-motion setting is honoured. Input requests, completion and meaningful updates may alert; streamed tokens and duplicate deliveries of an event do not replay the sound. Spoken voice remains later scope. | U 2026-11-03 (attention sound); A (controls and event deduplication) | V1 | W10 | 0027 |
| R31 | Where always-on-top or free placement fails on Wayland, the widget falls back to a notification-only mode; always-on-top and placement are expected to work on macOS and Windows and are verified per compositor. | A | V1 | W1, W2 | 0027 |
| R32 | A task with unanswered Daodan questions is a widget alert (FRD-0008). | U 2026-11-01 | V1 | — | 0033 |

### 2.6 Widget animation assets

C46's early Windows preview now uses three friendly ImageGen portraits based on
the canonical Oni reference, at the user's request on 2026-11-03: smile, happy
and curious. The first notice uses smile; new notices randomly select another
expression without immediate repeats. Refreshing the same notice keeps its
portrait. Assets and exact prompts are in `docs/assets/widget/generation.md`.
These are static expression variants; the animation frames below remain future
work. The removed invented SVG does not count as Shinatama animation assets.

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R33 | The user generates the animation frames with GPT Image 2.5 in manga or anime style, few frames; proposed shape: one neutral base frame edited for mouth, eyes or arm; seven actions (idle 4, talk 3, alert 4, done 4, thinking 4, worried 3, sleep 3), about 25 transparent 512×512 frames at 6 to 8 frames per second. The action list awaits the user's confirmation; the model's current name, variants and transparent output are verified against OpenAI's documentation. | U (generates them); A (proposal) | V1 | W7 | 0027 |
| R34 | The sprite pipeline (atlas, premultiplied alpha, 1x and 2x, size budget, seamless idle loop) is specified and tested. | A | V1 | W8 | 0027 |
| R35 | The character is Bungie's *Oni* IP: fine for personal use; its use in a distributed build is a legal decision taken separately from the code. | S | V1 | W9, O4 | 0048 |

### 2.7 Measurement before claims

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R36 | State accuracy is proved per agent against scripted sessions, including the false-positive rate of the PTY heuristic. | S | V1 | I1, AT-9 | 0027 |
| R37 | A graphical toolkit for a transparent, borderless, always-on-top, click-through window on Windows 11, macOS, X11 and Wayland (GNOME, KDE, sway) is found and its idle CPU, memory and startup measured before the widget is built. | S | V1 | W1 | 0027 |
| R38 | The low-cost backends are compared on real snapshots before the default is confirmed (R20). | A | V1 | I5 | 0028 |
| R39 | A provider low-cost model is chosen from the catalog without names in code, and its cost accounting and cap work. | A | V1 | I7 | 0028 |
| R40 | OpenCode's own `project` and `worktree` tables are compared with git worktrees for the overview's mapping. | S | V1 | OC11 | 0028 |
| R41 | Branch and worktree facts handle detached HEAD, linked worktrees, submodules, bare and non-git folders. | S | V1 | I3 | 0028 |
| R42 | Redaction before a summary is sent, and `summaries_excluded` really stopping the send, are tested. | A | V1 | I9 | 0028 |
| R43 | Agent icons render natively with text fallback and accessible names. | A | V1 | I10 | 0028 |
| R44 | The event path from database to bubble, autostart and several windows are measured. | A | V1 | W4 | 0027 |

## 3. Open questions

1. What "assistant" means for the widget in v1: animated character, bubble, alerts and quick answers now; chat and voice later (functional-design 7.2; register O3).
2. The Oni character in a distributed build (register O4, W9).
3. Pruning of the event tables (Q8).

## 4. Acceptance

AT-9 (state accuracy), I1 to I10, W1 to W10. Nothing has run.

## 5. Relation to the JSON record

FRD-0010.json holds 53 extracted sentences; this record covers them all and adds the liveness
rule (R05), the hook command (R06), the pruning gap (R08) and the measurement rows of 2.7.
