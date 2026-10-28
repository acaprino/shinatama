# Daodan native support

Date: 2026-10-27
Status: proposed, not approved. Nothing is built. Amends [the functional design](functional-design.md) (goal 5, two non-goals, section 5.5); the engine spec absorbs the "Engine impact" part after approval.

## 1. Intent

The user, 2026-10-27: "shinatama deve essere un supporto nativo per daodan" (Shinatama must be a native support for Daodan).

Until now Shinatama only *installed* Daodan (5.5) and typed its commands from a palette. Daodan was "a set of plugins and nothing else", and any Daodan state inside Shinatama was a non-goal. That is reversed. **Shinatama becomes a first-class host for Daodan's task layer**: it sees Daodan's tasks, ties them to threads, and carries a task across a hop without losing who holds it. (My reading of "native"; the sentence has one requirement and I took the widest reading that does not make Shinatama a second Daodan.)

Why it matters: Daodan's lock is the task holder in `.daodan/agenda.json`. A hop starts a *new* native session in the destination, so without support the destination agent is refused by the holder rule (the old session still "holds" the task) and the user has to explain and take over by hand. That is exactly the re-explaining Shinatama exists to remove.

## 2. Goals, non-goals, anti-goals

### Goals

1. **Read Daodan's task state**, read-only, for every project Shinatama knows: tasks with objective, branch, worktree, phase, status, sealed waves, unanswered questions, holder.
2. **Link threads to tasks** (derived, rebuildable, never authoritative).
3. **Carry a task across a hop**: the destination knows the task, and the user's confirmation in the hop dialog is the "user confirms the old session is closed" that Daodan's take-over requires.
4. **Start and resume in the task's worktree**: a thread for a task opens with that task's worktree as its directory.
5. **Surface what needs the user**: unanswered questions and stale holders in the project manager, the overview tab (5.13) and the widget (5.8).
6. **Task-aware palette**: the `^K` palette offers the task commands (show status, continue this task) next to the workflows of 5.5.
7. **Every Daodan plugin enabled** in every agent by default (4.6).
8. **Onboarding offer** on a project with no Daodan state: handover or initialization, once (4.7).

### Non-goals

- **Writing any Daodan file** (`agenda.json`, `runs/`, `processes/`, worktrees). Daodan's scripts are the only writer. Reason: the agenda row *is* the lock, and Daodan states that a bound process cannot bypass its grants, deliveries, questions or gates (`AGENTS.md:92-94`). A second writer would be the bypass.
- A task graph, checkpoints, an addressed mailbox, a scheduler: still Daodan's roadmap (functional design, non-goals).
- Creating or removing worktrees (Worktrunk and Daodan own that).
- Cross-machine holder logic: a holder whose `host` is another machine is shown, never judged stale.
- Guessing: an unreadable or unknown-schema agenda shows as "Daodan state unreadable (schema X)", never as an empty list.

### Anti-goals

- Shinatama running `--take-over` itself, or telling an agent to, without the user's confirmation in the dialog.
- A task link that survives a lie: if branch, worktree or session no longer match, the link disappears; it is never repaired by assumption.
- Two live processes both believing they hold one task (fork mode, section 4).

## 3. What Shinatama reads

Source of truth: `.daodan/agenda.json`, schema `daodan/agenda/v1`, in the **main checkout**, found through the Git common directory (`AGENTS.md:96-99`). Outside Git the project folder is its own home (the `.daodan-root` sentinel). Row fields used (`plugins/work-controller/skills/work-record/scripts/run_state.py:1504-1523`):

`run_id`, `operation`, `objective`, `status`, `loop`, `branch`, `worktree`, `head`, `phase`, `waves.{sealed,total}`, `adr`, `review`, `questions` (count of unanswered), `holder.{session,host,since}`, `updated_at`.

Rules:

- **Read-only, schema-checked.** A different `schema` value is a visible error. Rows are keyed by `run_id`.
- **Rebuilt, not cached as truth**: Shinatama's SQLite keeps only the thread-to-`run_id` link and the last-seen row for display when the file is unreachable (Drive paths).
- **A completed row has no holder** (Daodan releases it on completion); Shinatama shows it as done and stops offering "continue".

## 4. Behaviour

### 4.1 Project manager (5.4)

Each project row shows its open Daodan tasks beside its threads: objective, phase, `sealed/total` waves, `questions` as a count, holder. One key resumes the thread linked to the selected task, in the task's `worktree`. A task with no linked thread offers "start a thread for this task" in the agent of the user's choice, in that worktree, with the task's native continue command typed into the prompt (typed, not sent; same rule as the palette).

### 4.2 The link

A thread is linked to a task when its working directory equals the row's `worktree` (or the main checkout and the row's `branch` is checked out there). If the agent's native session id equals `holder.session` the link is marked *exact*; otherwise *by place*. Whether the ids ever match is open (register D9); the design works with *by place* alone.

### 4.3 Hop with a task

When the source thread is linked to an open task, the hop dialog gains a **Daodan task** block: `run_id`, objective, phase, branch, worktree, unanswered questions, current holder. Two modes, matching 5.2:

- **Close source (default).** The seed to the destination carries a Daodan block: the task identity above, plus the sentence that the source session is closed and the user confirmed it. The destination agent then continues through Daodan's own continue flow, which finds the task by branch and takes over on that confirmation. Shinatama itself writes nothing.
- **Fork.** The source stays live and keeps the holder. The destination is a new thread **without** the task: its seed says the task is held by the source session and may be read, not written, until the source closes. The two threads never both hold one task.

The preview (5.2, item 5) shows the Daodan block verbatim.

### 4.4 Overview, widget, alerts

A task with `questions > 0` is a user-needed item, shown in the overview tab and as a widget alert. A holder whose `host` is this machine and whose session matches no live Shinatama thread is shown as "held by a closed session: take over from the next agent you open there". Never automatic.

### 4.5 Palette (`^K`)

Adds, for the selected thread's task: *Show status* and *Continue this task*, typed as the agent's native command (`/daodan:show-status`, `/daodan:continue-task` on Claude Code and OpenCode; the Codex and Pi spellings come from the installed manifest, 5.5 and D8). The user presses Enter.

### 4.6 All plugins enabled

The user, 2026-10-27: "mi aspetto che attivi tutti i plugin di shinatama" (I expect it to activate all the plugins). The checkup (5.5) enables **every plugin the Daodan marketplace lists**, read from its manifest at check time and never from a list kept in Shinatama: 48 at marketplace 42.7.3 (`.claude-plugin/marketplace.json` in the Daodan repository), including the verticals that returned from the old Shinatama marketplace. Dependencies come along because everything is on.

- Plugins the user disabled stay disabled (D3).
- A plugin whose MCP server does not start is reported by the checkup and never blocks the launch.
- A plugin added in a later release is enabled at the next check.
- Setting `daodan.enable = all | core | custom`, default `all`; `core` is the entry plugin plus its dependency closure.
- **Only Daodan is a plugin marketplace** (user, 2026-10-27). The old `acaprino/shinatama` marketplace repository was deleted the same day (D1); its 24 plugins all exist in Daodan at an equal or newer version, so "all the plugins" is exactly Daodan's list. Shinatama registers one marketplace per host and no other.

Cost, said once: "all" puts every plugin's skill descriptions and tool definitions into each agent's context, and this very session already has MCP servers from Daodan plugins (moe, peer-review, playwright, work-controller) that failed to start. The setting and D4 exist for that; the default stays `all` as asked.

### 4.7 A project with no Daodan state

Daodan does the onboarding itself only when one of its workflows runs. `first_use_probe` (`plugins/work-controller/skills/work-record/scripts/run_state.py:102-110`) is called at the start of `/daodan:init-project`, `/daodan:onboard-project`, `/daodan:map-code` and the other workflows, and `references/first-use.md` of `project-delivery` then asks whether to keep decision records and how deep to analyze (none, lite, deep). Nothing opens that by itself when a project is merely opened: the only hooks Daodan declares are the `research` plugin's session-start and prompt hooks, which emit Science Mode routing and nothing else. Shinatama adds the trigger, not the logic.

When a project is opened in the project manager, or a thread is first started in it, Shinatama checks that `.daodan`, `.codebase-map` and `.codebase-xray` are all absent in the project and in the main checkout (the test of `run_state.py:99-110`; `.daodan` lives in the main checkout). If so it offers once:

- **Onboard**: `/daodan:onboard-project`, the handover dossier; changes nothing in the project.
- **Initialize**: `/daodan:init-project`: map, environment, baseline.
- **Not now.**

A choice types the agent's native command; the user presses Enter. The questions that follow (decision records, analysis depth) are Daodan's, asked through the agent's question tool: Shinatama neither asks them nor passes answers. "Not now" is remembered per project in Shinatama's SQLite, because declining writes nothing in the project; the next Daodan workflow still asks its own first-use questions. The offer ends when a state directory appears.

## 5. Engine impact (for the engine spec after approval)

- A `daodan` reader module: resolve main checkout, read and validate `agenda.json`, expose rows; watch the file for changes with a bounded poll (no resident service beyond the terminal's life).
- One SQLite table `thread_task(thread_id, run_id, link_kind, last_row_json, seen_at)`; derived, droppable.
- Hop seed (6.4) gains an optional Daodan block with its own budget line; the preview renders it.
- The adapter contract (section 6) gains nothing: the task commands are typed text per agent.

## 6. To prove before it is built

Register rows D9 to D14 and D4 ([verification-register.md](verification-register.md), section 12). D9 and D10 decide the exactness of the link and the form of the typed commands; D4 decides whether `all` stays the default; none changes the principle that Daodan is the only writer.

## 7. My calls (check them)

1. Read `agenda.json` directly instead of through Daodan's read-only MCP in `work-controller`: no agent needed, works when no agent is running. If D11 shows torn reads, switch to the MCP or to the `run_state.py agenda` command.
2. Shinatama never runs a Daodan script. It only reads and types.
3. Fork mode hands over no task. Safer than sharing the holder; revisit if forks of task threads turn out to be the common case.
4. The superseded mockup `daodan-flow.html` (threads linked to Daodan tasks) is not revived as is; a new mockup of the project manager row and the hop block comes with approval.
5. The onboarding check is reimplemented in Shinatama (three directory tests) instead of calling Daodan's `first-use` script, to keep "Shinatama never runs a Daodan script" (call 2). D14 checks parity.
6. The offer is made once per project and never auto-runs a command, same typed-not-sent rule as the palette.
