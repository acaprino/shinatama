# FRD-0008: Daodan preinstalled and native task support

**Implementation scope override, user 2026-11-02:** ignore Daodan integration
for now. These requirements are retained for resumption, but Daodan installation,
task/onboarding surfaces and dependent seams are deferred. They do not block the
active launch/search/hop plan. See the [scope checkpoint](../implementation/progress.json)
and [implementation plan](../implementation/PLAN.md). Resume only on a new user instruction.

| | |
|---|---|
| Version | 1 (Markdown record; JSON record version 1 at `c84d28e`) |
| Status | Accepted 2026-11-01 (this Markdown text; the JSON evidence record, where one exists, stays Proposed until `frd_tool.py promote`) |
| Date | 2026-11-01 |
| Sources | functional-design goal 5, 2 (non-goals), 5.5, 7; daodan-native-support 1 to 7; README (marketplaces) |
| Governing ADRs | ADR-0032, ADR-0033, ADR-0034 |
| Register sections | D1 to D14, D20, D4 |
| Related records | FRD-0002 (into which home), FRD-0004 (rows and offers), FRD-0005 (task across a hop), FRD-0007 (`moe` for roundtables), FRD-0010 (alerts) |

## 1. Purpose and scope

Daodan, the user's plugin marketplace, arrives preinstalled and enabled in every hosted agent,
and Shinatama is a **native support** for its task layer: it reads the agenda, links threads to
tasks, surfaces what needs the user, and carries a task across a hop. The one rule under all of
it: Shinatama reads and types; Daodan's scripts stay the only writer of Daodan state.

## 2. Requirements

### 2.1 Installation and plugins

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | The checkup installs or updates Daodan in every agent present, so a Shinatama session is the full working setup. Daodan is a set of plugins already compiled per host: Shinatama converts and translates nothing and installs Daodan's build for that host unchanged. | U (option A: downloaded and kept updated) | X | D2 | 0032 |
| R02 | The plugins go into the agent's home: the box by default, the user's native plugin store with the `system` home (so the user's own runs see them too), and per launch for an instance with its own set. | U | X | D2, D19 | 0032, 0012 |
| R03 | By default the checkup enables every plugin the Daodan marketplace lists, `research` included, read from the manifest at check time (48 at marketplace 1.0.0) and never from a list kept in Shinatama; a plugin added in a later release is enabled at the next check. | U 2026-10-27 ("I expect it to activate all the plugins") | X | D2, D4 | 0032 |
| R04 | The setting `daodan.enable = all \| core \| custom` defaults to `all`; `core` is the entry plugin plus its dependency closure. The cost of `all` (skill descriptions and tool definitions in each agent's context, MCP servers that fail to start) is measured by D4. | S | X | D4 | 0032 |
| R05 | Plugins the user disabled stay disabled; a plugin whose MCP server does not start is reported and never blocks the launch. | P | X | D3 | 0032 |
| R06 | Daodan is not the only marketplace allowed: the marketplaces Daodan declares as dependencies, and any other, may be registered; "all the plugins" still means Daodan's list. | U 2026-11-01 (lifting the 2026-10-27 restriction) | X | D2 | 0032 |
| R07 | Hermes Agent and DeepSeek Harness have no Daodan package: the checkup installs nothing there and says "Daodan: no package for this agent"; Shinatama never copies another host's build into them; when Daodan ships their packages the checkup treats them like the other hosts. | A | X | D20 | 0032 |
| R08 | The agents use Daodan through their native entries (`/daodan:<workflow>` on Claude Code and OpenCode, `<workflow>-workflow` on Codex, `/daodan-<workflow>` on Pi), with the Codex and Pi spellings taken from the installed manifest. | S | X | D8, D10 | 0032, 0033 |
| R09 | Checkup time per launch, its behaviour when several instances start, and updating an agent while an instance runs (locked executables on Windows) are measured. | S | X | D7 | 0013 |
| R10 | The obsolete `acaprino/shinatama` marketplace repository was deleted on 2026-10-27 after a verified bundle backup; its 24 plugins exist in Daodan at an equal or newer version; the name collision with this product is gone. | U 2026-10-27 | — | D1 (done) | — |

### 2.2 Reading Daodan's task state

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R11 | Shinatama is a native host for Daodan's task layer, not only its installer: it reads, read-only, every known project's tasks with objective, branch, worktree, phase, status, sealed waves, unanswered questions and holder. | U 2026-10-27 ("a native support for Daodan"); shape confirmed 2026-11-01 | V1 | D9 to D12 | 0033 |
| R12 | Shinatama never writes any Daodan file (`agenda.json`, `runs/`, `processes/`, worktrees), never runs a Daodan script, never creates or removes worktrees and never runs `--take-over` or tells an agent to without the user's confirmation in the hop dialog. The agenda row is the lock; a second writer would be the bypass. | U 2026-11-01 (only writer); A (never runs a script) | V1 | D13 | 0033 |
| R13 | The source of truth is `.daodan/agenda.json`, schema `daodan/agenda/v1`, in the main checkout found through the Git common directory; outside Git, or when the common directory is not a plain `.git`, the project folder is its own home. The read is schema-checked; rows are keyed by `run_id`. | P | V1 | D11, D12 | 0033 |
| R14 | Shinatama reads the file directly rather than through an agent or Daodan's MCP, so task state shows when no agent runs; if D11 shows torn reads it switches to the MCP or the `run_state.py agenda` command. The file is watched with a bounded poll, with no resident service beyond the owning window's life. | A | V1 | D11 | 0033 |
| R15 | An agenda with another schema, or one that cannot be read, shows as "Daodan state unreadable (schema X)", never as an empty list. When the file is unreachable (Drive paths) the last-seen row is shown for display only. | P | V1 | D11 | 0033 |
| R16 | Shinatama's SQLite keeps only derived, droppable state: `thread_task(thread_id, run_id, link_kind, last_row_json, seen_at)` and the per-project "Not now"; nothing of it is treated as truth. | P | V1 | — | 0033, 0009 |
| R17 | A completed row has no holder: it is shown as done and "continue" is no longer offered. A holder whose `host` is another machine is shown and never judged stale. | P | V1 | — | 0033 |
| R18 | Shinatama does not implement Work Controller's task graph, checkpoints, handover between Daodan tasks, scheduler or persistent mailbox; it shows the agenda and the holder. | U 2026-11-01 | — | — | 0033 |

### 2.3 Linking threads to tasks

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R19 | A thread is linked to a task when its working directory equals the row's `worktree`, or is the main checkout with the row's `branch` checked out; the link is derived and rebuildable, never authoritative. | P | V1 | D9 | 0033 |
| R20 | The link is *exact* when the agent's native session id equals `holder.session`, otherwise *by place*; whether the ids ever match is D9, and the design works with by-place links alone. | P | V1 | D9 | 0033 |
| R21 | A link whose branch, worktree or session no longer matches disappears and is never repaired by assumption. | P | V1 | — | 0033 |
| R22 | A thread for a task opens in that task's worktree (FRD-0004 R17). | P | V1 | D10 | 0033 |

### 2.4 Surfaces: project manager, palette, overview, widget

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R23 | Each project row shows its open tasks beside its threads; one key resumes the linked thread in the task's worktree; a task with no linked thread offers to start one in the chosen agent with the native continue command typed, not sent (FRD-0004). | U 2026-11-01 (shape); P (details) | V1 | D10 | 0033 |
| R24 | The palette (`^K`) lists Daodan's workflows from the installed manifest and types the selected agent's native command into the prompt; the user presses Enter. For the selected thread's task it adds *Show status* and *Continue this task* (`/daodan:show-status`, `/daodan:continue-task` on Claude Code and OpenCode; Codex and Pi spellings from the manifest). Hermes Agent and DeepSeek Harness get no Daodan entry. | A (palette); U 2026-11-01 (task entries) | V1 | D8, D10, D20 | 0033 |
| R25 | Every Daodan command Shinatama offers is typed and never sent. | A | V1 | D8 | 0033 |
| R26 | A task with `questions > 0` is a user-needed item in the overview and a widget alert. A holder whose `host` is this machine and whose session matches no live Shinatama thread shows as "held by a closed session: take over from the next agent you open there"; never automatic. | U 2026-11-01 (questions surfaced); P (stale holder text) | V1 | — | 0033 |
| R27 | Threads of Hermes Agent and DeepSeek Harness still show their tasks and holders, because that is Shinatama reading the agenda; they get no typed commands. | U 2026-11-01 | V1 | D20 | 0033 |

### 2.5 Task across a hop

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R28 | When the source thread is linked to an open task the hop dialog gains a Daodan task block (`run_id`, objective, phase, branch, worktree, unanswered questions, holder), shown verbatim in the preview; the seed gains an optional Daodan block with its own budget line. | P | V1 | D13 | 0034 |
| R29 | Close mode: the preliminary seed says close was requested but not yet confirmed; the holder stays with the source and the destination may read, not take over. Only after COMMITTED and the source process's confirmed closure does Shinatama send a separate handover message through the destination's normal channel; the destination then continues through Daodan's own continue flow; if closure is refused, crashes or is uncertain, no message is sent. One user confirmation in total. | U 2026-11-01 | V1 | D13 | 0034 |
| R30 | Fork mode: the source keeps the holder; the destination is a new thread without the task, whose seed says the task may be read, not written, until the source closes. Two threads never both hold one task. | A | V1 | D13 | 0034 |
| R31 | A destination with no Daodan package receives the block as read-only context, the dialog says the task cannot be taken over there, and close mode sends no handover message. | P | V1 | D20 | 0034 |
| R32 | The Daodan block the destination receives is exactly what the preview showed (invariant I4). | S | V1 | D13 | 0034 |
| R33 | No hop path writes Daodan state; the holder changes only through Daodan's own flow in the destination agent. | U 2026-11-01 | V1 | D13 | 0034 |

### 2.6 Onboarding a project with no Daodan state

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R34 | When an opened project, or one in which a thread first starts, has neither `.codebase-map` nor `.daodan/runs` as a directory in it or its main checkout (a bare `.daodan/` does not count), Shinatama offers once: Onboard (`/daodan:onboard-project`, the handover dossier), Initialize (`/daodan:init-project`: map, environment, baseline) or Not now. | U 2026-11-01 (offer); P (test) | V1 | D14 | 0033 |
| R35 | A choice types the command; the user presses Enter. Daodan's own first-use questions are asked by Daodan through the agent's question tool; Shinatama neither asks nor answers them. | P | V1 | D14 | 0033 |
| R36 | "Not now" is remembered per project in Shinatama's SQLite, because declining writes nothing in the project; the offer ends when a state directory appears. | P | V1 | D14 | 0033 |
| R37 | For a project Shinatama creates, the new-project dialog's option to type Daodan's init command is the only offer; declining it counts as "Not now" and no onboarding offer follows (FRD-0004). | U 2026-11-01 | V1 | D14 | 0033 |
| R38 | The onboarding check is reimplemented in Shinatama as two directory tests, to keep "Shinatama never runs a Daodan script"; D14 checks parity with Daodan's `first_use_probe`. | A | V1 | D14 | 0033 |

### 2.7 Engine impact and what must be proved

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R39 | The engine gains a `daodan` reader module (resolve main checkout, read and validate the agenda, expose rows, bounded poll) and the `thread_task` table; the adapter contract gains nothing, since task commands are typed text per agent. | P | V1 | — | 0033 |
| R40 | D9 and D10 decide the exactness of the link and the form of the typed commands; D4 decides whether `all` stays the default; none changes the principle that Daodan is the only writer. | P | V1 | D4, D9, D10 | 0033 |
| R41 | The superseded mockup `daodan-flow.html` is not revived; a new mockup of the project-manager row and the hop block comes with approval. | A | — | — | 0033 |
| R42 | Whether to ask Daodan for Hermes Agent and DeepSeek Harness host packages is the user's decision (register O10); Shinatama does not change Daodan. | S | — | O10 | 0032 |

## 3. Open questions

1. Hermes Agent and DeepSeek Harness host packages (register O10).
2. The cost of `all` and whether it stays the default (D4).

## 4. Acceptance

D2 to D14 and D20 pass with records; D1 is done. daodan-native-support 6 lists D9 to D14 and D4
as the gates of the native support.

## 5. Relation to the JSON record

FRD-0008.json holds 57 extracted sentences; this record covers them all and adds R09, R10 and
R42, which the JSON set kept in other records or in the register only.
