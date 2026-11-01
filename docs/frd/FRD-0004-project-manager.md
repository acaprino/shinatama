# FRD-0004: Project manager

| | |
|---|---|
| Version | 1 (Markdown record; JSON record version 1 at `c84d28e`) |
| Status | Proposed |
| Date | 2026-11-01 |
| Sources | functional-design goal 4, 5.4, 5.13 (worktree option), 7; technical-design 4.2 (`project`); daodan-native-support 4.1, 4.7 |
| Governing ADRs | ADR-0046 (proposed), ADR-0033 (Daodan rows and offers) |
| Register sections | PM1 to PM8, U3, D14 |
| Related records | FRD-0006 (threads per project), FRD-0008 (Daodan tasks and offers), FRD-0010 (worktree collisions) |

## 1. Purpose and scope

The project manager is the first surface the user sees: it lists projects, remembers which
were used, creates new ones and starts instances in them. This record states its listing,
sorting, filter, history, creation and robustness rules, and the two places where Daodan
appears in it.

## 2. Requirements

### 2.1 Listing, sorting and filtering

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Shinatama has an integrated project manager as a native surface; the project selector is part of the terminal-first milestone. | U | M0 | TL1 | 0046, 0040 |
| R02 | Projects are the first-level folders of each project root; several roots may be configured. | S (first level); A (several roots) | M0 | PM1 | 0046 |
| R03 | Projects are sorted by last use in Shinatama (last open or last thread activity, `project.last_opened`); projects never opened in Shinatama sort by folder modification time. | U 2026-11-01 | M0 | PM1 | 0046 |
| R04 | An incremental filter opens with `/` or by focusing the search field. | U 2026-11-01 | M0 | U3 | 0046 |
| R05 | Any folder can be opened as a project through F3 (a native folder picker on each OS, with drive letters and no terminal-escape traps). | S | M0 | PM6 | 0046 |
| R06 | A history of the last 10 non-standard folders is kept: folders opened with F3 that are not under a project root (Desktop, shared drives, folders that are not git repositories). | U 2026-11-01 | M0 | — | 0046 |
| R07 | The user can pin projects. | A | V1 | — | 0046 |
| R08 | Each project row shows its git branch and dirty-file count, the last agent and last thread, and its threads beside it; one key resumes the last thread. Git facts for 30 projects are gathered in parallel with timeouts and a cache. | A | V1 | PM2, PM3 | 0046 |
| R09 | Each project row shows its open Daodan tasks beside its threads (objective, phase, `sealed/total` waves, unanswered-question count, holder), as FRD-0008 specifies. | U 2026-11-01 | V1 | D9 to D12 | 0033 |
| R10 | Login state is visible here: an agent that cannot start shows why (FRD-0003). | A | M0 | D5 | 0031 |

### 2.2 Creating a project

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R11 | A default root, where new projects are created, is chosen at first run and changeable later. | U | M0 | PM8 | 0046 |
| R12 | The `n` shortcut creates a new project and works outside the incremental filter; the final key is set with U3. | U 2026-11-01 | M0 | U3 | 0046 |
| R13 | The new-project dialog offers an optional `git init`. | A | M0 | PM4 | 0046 |
| R14 | The new-project dialog offers to type Daodan's native init command into the chosen agent's prompt; it never runs it. For a project Shinatama creates this is the only Daodan offer: declining it counts as the "Not now" of daodan-native-support 4.7 and no second offer follows. | A (option); U 2026-11-01 (only offer, "Not now") | V1 | D14 | 0033, 0046 |
| R15 | Creating a project never leaves a half-created folder; names are validated per OS (reserved names, trailing dots, case-insensitive duplicates), the `git init` default branch and permissions are set, and a root on a synced drive is handled. | A | M0 | PM4 | 0046 |

### 2.3 Starting instances

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R16 | The new-instance command, from the project manager or a tab, can start the instance in a new linked worktree so that two agents on the same project do not collide. It is an option of that command, not an overview action. | U 2026-11-01 | V1 | H20, I3 | 0046, 0028 |
| R17 | A thread linked to a Daodan task opens in that task's worktree; a task with no linked thread offers "start a thread for this task" in the agent of the user's choice, in that worktree, with the task's native continue command typed, not sent (FRD-0008). | P | V1 | D10 | 0033 |
| R18 | Every launch from the project manager goes through the single launch point and its checkup (FRD-0001 R08). | A | X | D7 | 0013 |

### 2.4 Robustness

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R19 | Listing and sorting a root never blocks the UI, even with hundreds of folders or a Google Drive path. | A | M0 | PM1 | 0046 |
| R20 | A missing root (a removed drive) is reported, not ignored. | A | M0 | PM7 | 0046 |
| R21 | Projects are matched across agents' different path encodings through the canonicalization of FRD-0014, so one folder is one project whichever agent recorded it. | S | S1 | S9, PM3 | 0009 |
| R22 | The configuration (roots, default root, pins, history) lives in the per-user database or the platform configuration directory, never in a project folder. | A | M0 | PM8 | 0009, 0046 |

### 2.5 Onboarding offer for opened projects

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R23 | When a project is opened, or a thread first started in it, and neither `.codebase-map` nor `.daodan/runs` exists as a directory in it or its main checkout, Shinatama offers once: Onboard, Initialize or Not now; a choice types the command, "Not now" is remembered per project, and the offer ends when a state directory appears (FRD-0008). | U 2026-11-01 (offer); P (test) | V1 | D14 | 0033 |
| R24 | The onboarding check is reimplemented in Shinatama (two directory tests) rather than calling Daodan's script; D14 checks parity. | A | V1 | D14 | 0033 |
| R25 | Shinatama neither asks Daodan's first-use questions nor passes answers to them. | P | V1 | D14 | 0033 |

## 3. Open questions

1. The final key bindings for the filter, `n` and F3 (register U3).
2. Whether the project manager should offer only the actions both adapters support (register O7).

## 4. Acceptance

PM1 to PM8 pass; D14 for the onboarding parity; U3 for the keys. Nothing has run.

## 5. Relation to the JSON record

FRD-0004.json holds 27 extracted sentences; this record covers them all and adds R10, R18,
R21 and R22, which the JSON set kept in other records.
