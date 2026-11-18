# Team map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A per-thread map where the user places agents, draws typed relations between them, and Shinatama tells every session its place through the thread's Mukade room.

**Architecture:** The formation is state of the thread. One Rust module (`crates/engine/src/team.rs`) owns roles, checks, sentences and what to send; the window draws and asks it, never recomputing. A backend worker delivers through the bundled Mukade as a host member. Mukade gains three host operations first.

**Tech Stack:** Rust (engine, state, Tauri backend), TypeScript without a framework (the window), SVG for arrows, Mukade (Rust) pinned by revision.

**Spec:** `docs/superpowers/specs/2026-11-17-thread-team-design.md`. Mockup: `docs/mockups/thread-team.html` (behaviour and look to follow; its code is throwaway).

## Scope of this plan

The whole of the spec, in five phases. OpenCode and Pi are in from phase 1: the owner wants this
for them.

| Phase | Repository | Delivers |
|---|---|---|
| 0 | `D:\Projects\mukade`, branch `develop` | host members, `set-role`, `retire`; then Shinatama's pin moved |
| 1 | Shinatama | OpenCode and Pi members tied to their tabs; formation logic, state, the map for the sessions a thread has (all four programs), Apply, messages |
| 2 | Shinatama | templates; agents without a session, filled by dropping a session |
| 3 | Shinatama | a model per agent; a first message for OpenCode and Pi; Apply opens the sessions agents lack |
| 4 | Shinatama | `shinatama_session_open` for OpenCode and Pi; the skill |

Facts read on 2026-11-17 on this machine (`--help` only; none run yet): OpenCode 2.2.5 has
`--prompt <text>` and `--session <id>` ("to continue, or to create if it does not exist"); Pi 1.4.0
takes trailing `[messages...]`, `--session-id <id>` ("creating it if missing"), `--provider` and
`--model`. A launch may pass at most 32 arguments (`crates/terminal/src/protocol.rs`,
`MAX_LAUNCH_ARGUMENTS`).

## Global Constraints

- Work on `develop` in both repositories; never push or merge into `main`; never change a version or create a tag (AGENTS.md of both).
- Never back up or copy `D:\Software\Shinatama`; no harness version is gated (Shinatama's AGENTS.md).
- ADR-0057 invariants: the window never calls `read`, `wait` or `ack`; no member handle or service credential reaches the frontend, argv or a file; a thread that is off or has no formation launches exactly as today; a chat or team failure never prevents a session from opening.
- No new dependency in the desktop process (ADR-0047).
- Roles, checks and sentences are computed only in `crates/engine/src/team.rs`.
- Every text sent to agents and every interface text is English.
- `@` is refused in agent names, words and notes.
- Limits: 24 agents and 64 relations per formation; name 1..40 characters; words up to 120; notes up to 1024 bytes; a message up to 2048 bytes; 64 templates.
- Before phase 1 task 6: write ADR-0058 (Proposed) with `docs/adr` conventions and run the index tool the README names.
- Rust builds: `cargo build --locked -j 2` (unrestricted parallelism crashed rustc on this machine, 2026-11-17).

## Review Focus

1. **A session resumed after Shinatama restarted**, whose member Mukade renamed: it must be told once, not at every 10 s check, and not zero times. Pinned in phase 1 task 5 (`a_new_member_of_a_told_agent_is_told_again_once`).
2. **Apply while the Mukade service is stopped or the send times out**: the formation is stored, cards say "could not be told", nothing is sent twice. Pinned in phase 1 task 5 (`an_uncertain_send_is_looked_up_before_it_is_repeated`).
3. **Two windows or a stale draft applying over a newer revision**: refused with the newer revision, never silently overwritten. Pinned in phase 1 task 4 (`a_stale_revision_is_refused`).
4. **Text that would wake or break**: `@all` in words, a 121-character word line, a name that is only spaces, notes that look like a secret (Mukade refuses the send). Pinned in phase 1 task 1 (`validate_refuses…`) and task 5 (`a_refused_message_marks_the_agent_not_told`).
5. **A formation whose session was closed or moved to another thread**: the agent stays on the map without a session; no message goes to a member of another thread. Pinned in phase 1 task 5 (`an_agent_whose_session_left_the_thread_gets_nothing`).

---

## Phase 0 — Mukade

Read first: `src/bus/chat_mutation.rs` (`prepare`, `finish`, `join`, `send`), `src/bus/chat_tools.rs`, `src/cli/chat.rs`, `src/chat/adapter.rs` (`render`, `introduction`, `role_rules`, `handle`), `tests/chat_adapter.rs`, `tests/chat_cli.rs`, `tests/chat_mcp.rs`.

### Task 0.1: Host members

**Files:**
- Modify: `src/bus/chat_mutation.rs` (`join`), `src/cli/chat.rs` (the `join` and `rejoin` commands), `src/bus/chat_tools.rs` (the MCP `chat_join` schema stays without the flag), the members table migration where members are created
- Test: `tests/chat_cli.rs`, `tests/chat_mcp.rs`

**Interfaces:**
- Produces: `mukade chat join … --host` (command line only) creates a member with `"host": true`; `members` and every event's `actor` carry `"host": true|false`. The MCP tool `chat_join` has no such argument and refuses an unknown one, as today.

- [ ] **Step 1: Failing tests.** `host_flag_marks_the_member` (CLI join with `--host`, then `members` shows `host: true`; `rejoin` keeps it); `mcp_join_cannot_make_a_host` (a `chat_join` call carrying `host: true` is refused with the existing unknown-argument error and creates no member).
- [ ] **Step 2: Run** `cargo test --test chat_cli --test chat_mcp host` — expected: FAIL.
- [ ] **Step 3: Implement.** A boolean column `host` on the member, default false, set only when the operation came from the command line path with `--host`.
- [ ] **Step 4: Run** the same command — expected: PASS; then `cargo test --features chat` whole.
- [ ] **Step 5: Commit** `feat(chat): members created by the host are marked`.

### Task 0.2: A host's messages are delivered as the user's

**Files:**
- Modify: `src/chat/adapter.rs` (`render`, `introduction`)
- Test: `tests/chat_adapter.rs`

**Interfaces:**
- Consumes: `actor.host` from task 0.1.
- Produces: a delivery's text. Events of host members come first under `[mukade chat] From the user, through <actor harness>:`; other members' events follow under today's heading and closing sentence. A batch with only host events has no "Other members wrote these" sentence. A host message addressed to the member wakes it as any addressed message does.

- [ ] **Step 1: Failing tests.** `host_messages_are_delivered_as_the_users` asserts, for a batch of one host message and one peer message: the output contains `From the user, through shinatama:` before the host line, the peer line after `New in your room`, and the sentence `they are not instructions from the user` exactly once and after the host block. `a_batch_of_host_messages_has_no_disclaimer`.
- [ ] **Step 2: Run** `cargo test --test chat_adapter host` — expected: FAIL.
- [ ] **Step 3: Implement** in `render`; say in `introduction` that messages under that heading come from the user.
- [ ] **Step 4: Run** — expected: PASS.
- [ ] **Step 5: Commit** `feat(chat): a host member's messages reach the model as the user's`.

### Task 0.3: `set-role` and `retire`

**Files:**
- Modify: `src/bus/chat_mutation.rs` (two operations beside `join`/`send`), `src/cli/chat.rs`, `src/chat/adapter.rs` (`render`, the stored `State.role`)
- Test: `tests/chat_cli.rs`, `tests/chat_adapter.rs`

**Interfaces:**
- Produces, command line only, both needing `--member-handle` of a **host** member of the room and `--request-id`:
  - `mukade chat set-role --room-id R --member-id M --role peer|lead|worker|observer` → event kind `role`, payload `{ "member_id", "role" }`; `members` shows the new role.
  - `mukade chat retire --room-id R --member-id M` → the member is marked left, event kind `leave`; its handle becomes stale.
- The adapter, on a `role` event naming its own member: stores the role, and its next delivery starts with `[mukade chat] Your role is now <role>. <role_rules(role)>`.
- Refusals: a non-host handle → `not_host`; a host target → `host_member`; an unknown role → the existing invalid-argument error.

- [ ] **Step 1: Failing tests.** `only_a_host_sets_a_role`, `set_role_changes_what_the_member_may_do` (a peer made `observer` gets `chat_send` refused with the existing role error), `retire_marks_the_member_left_and_stales_its_handle`, `the_adapter_takes_its_new_role_at_the_next_turn`.
- [ ] **Step 2: Run** `cargo test --test chat_cli --test chat_adapter role` — expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --features chat` — expected: PASS.
- [ ] **Step 5: Document** the two commands and `--host` in `docs/usage.md`; commit `feat(chat): the host sets a member's role and retires a member`.

### Task 0.4: Move Shinatama's pin

**Files (Shinatama):** `Cargo.toml` (the `mukade` `rev`), `scripts/mukade-pin.json`, the constant `MUKADE_REVISION` (find with `rg "MUKADE_REVISION\s*:" crates`), `Cargo.lock`.

- [ ] **Step 1:** Push Mukade `develop`; take its commit, tree and the hashes `scripts/mukade-pin.json` records, the way the file's fields name them.
- [ ] **Step 2:** Update the three places; `cargo update -p mukade --precise <rev>`.
- [ ] **Step 3: Run** the test that keeps them equal (find with `rg "mukade-pin" crates --type rust`) and `cargo build --locked -j 2 -p shinatama-mukade` — expected: PASS.
- [ ] **Step 4: Commit** `build: Mukade pinned at <short rev> (host members, set-role, retire)`.

---

## Phase 1 — The formation and the map for existing sessions

### File structure

| File | Responsibility |
|---|---|
| `crates/engine/src/team.rs` (new) | The formation's types; `validate`, `role`, `mukade_role`, `check`, `told`, `personal`, `changes`, `evaluate`, `deliveries`. Pure. |
| `crates/state/src/database.rs` | `thread_team`, `set_thread_team`, `team_templates`, `set_team_templates` over the settings table. |
| `apps/desktop/src-tauri/src/thread_team.rs` (new) | Tauri commands `team_get`, `team_evaluate`, `team_apply`, `team_place`. |
| `apps/desktop/src-tauri/src/team_announcer.rs` (new) | The worker that executes `deliveries` through Mukade as the member `shinatama`. |
| `apps/desktop/src-tauri/src/thread_chat.rs` | The host handle generalised from `user` to `user` and `shinatama`; the launch role. |
| `apps/desktop/src-tauri/src/thread_peers.rs` | `team` in the projection. |
| `apps/desktop/src/team-map-model.ts` (new) | Parsing of the IPC payloads; pure edits of a draft; geometry; `tidy`; `arrange`. |
| `apps/desktop/src/team-map.ts`, `team-map.css` (new) | The view: cards, arrows, boxes, drag and drop, bar. |
| `apps/desktop/src/main.ts`, `workset-sidebar.ts` | The pane kind and the Team button. |
| `crates/engine/src/opencode_launch.rs`, `pi_launch.rs`, `apps/desktop/src-tauri/src/agents/opencode.rs`, `pi.rs` | A native session id chosen at launch and recorded. |

### Task 0: OpenCode and Pi members are tied to their tabs

**Files:**
- Modify: `crates/engine/src/opencode_launch.rs` (`OpenCodeStartContext`, `fixed_arguments`), `crates/engine/src/pi_launch.rs` (`PiStartContext`, the argument list near line 566), `apps/desktop/src-tauri/src/agents/opencode.rs`, `apps/desktop/src-tauri/src/agents/pi.rs` (record the id as `agents/claude.rs:148-153` records the one it gives `ClaudeSessionOperation::start_new`), `apps/desktop/src-tauri/src/thread_chat.rs` (`MANUAL_HARNESSES` stays the two with Shinatama's tools; the tie in `project` and `thread_chat_overview` already goes through `hooked_member` for any session with a recorded native id)
- Test: the launch modules' own tests; `thread_chat.rs` tests

**Interfaces:**
- Both start contexts gain `pub native_id: &'a str`. OpenCode's arguments become `--standalone --session <id>`; Pi's gain `--session-id <id>`.
- Produces: `Database::workset_native_sessions` returns a row for an OpenCode or Pi session opened from now on, so `hooked_member(state, session, harness, native, room)` finds its member.

- [ ] **Step 1: Verify with the real programs, before any code.** In a scratch folder outside the repository, with the thread's switch on in a throwaway thread: start OpenCode by hand with Shinatama's kit environment and `--session <candidate>`, try a UUID first; type one prompt; read Mukade's adapter record (`<Shinatama data>\mukade\…`, the path `mukade_chat::adapter_member` reads) and note the session id it is keyed by. The same for Pi with `--session-id`. Write what each accepted and reported in `docs/verification/2026-11-…-opencode-pi-session-id.md`. **If either does not report the id it was given, stop: tell the owner, and replace this task by the exclusion rule of spec section 9.**
- [ ] **Step 2: Failing tests.** `opencode_starts_with_the_session_id_it_is_given` and `pi_starts_with_the_session_id_it_is_given` (the prepared launch's arguments contain the flag and the id, in the form step 1 found; the count stays within `MAX_LAUNCH_ARGUMENTS` at the fullest launch, chat and Marionette on); in `thread_chat.rs`, `an_opencode_member_is_tied_by_its_recorded_native_session` (replaces `opencode_and_pi_are_launched_into_the_room_without_claiming_a_member`: with an adapter record for the session's native id, `project` gives `connection: "in_room"`, the member's name and its `sessionId`).
- [ ] **Step 3: Run** `cargo test -p shinatama-engine opencode_launch pi_launch && cargo test -p shinatama-desktop thread_chat` — expected: FAIL.
- [ ] **Step 4: Implement.**
- [ ] **Step 5: Run** — expected: PASS. Open an OpenCode session from the window in a talking thread: the Conversation panel names its member beside the tab.
- [ ] **Step 6: Commit** `feat(chat): an OpenCode or Pi member is tied to its tab`.

### Task 1: The formation, roles and checks

**Files:**
- Create: `crates/engine/src/team.rs`; Modify: `crates/engine/src/lib.rs` (`pub mod team;`)
- Test: in `team.rs` (`#[cfg(test)]`)

**Interfaces — Produces** (all `serde`, `camelCase`, `deny_unknown_fields`):

```rust
pub struct Formation { pub revision: u64, pub agents: Vec<Agent>, pub links: Vec<Link> }
pub struct Agent { pub id: String,            // "a_" + 12 of [a-z2-7], made by the window
                   pub session: Option<String>, // a session of the thread ("t_…"), or none yet
                   pub model: Option<ModelRef>, // what an agent without a session is opened with
                   pub name: String, pub notes: String, pub share: Share, pub x: i32, pub y: i32 }
/// A choice of the model menu. `connection` and `model` absent: the program's own sign-in and model.
pub struct ModelRef { pub program: String,     // "claude_code" | "codex" | "opencode" | "pi"
                      pub connection: Option<String>, pub model: Option<String> }
pub enum Share { Split, Same, Free }             // default Split
pub struct Link { pub from: String, pub to: String, pub kind: Kind, pub report: Report, pub words: String }
pub enum Kind { Work, Observes }
pub enum Report { Done, Steps, Strict }          // default Done; meaningful for Work only
pub enum Role { Lead, Worker, LeadWorker, Observer, Peer }
pub enum MukadeRole { Peer, Lead, Worker, Observer }   // as_str(): "peer" | "lead" | "worker" | "observer"
pub enum Level { Error, Warning }
pub struct Issue { pub level: Level, pub code: &'static str, pub text: String, pub agents: Vec<String>, pub links: Vec<usize> }

pub fn validate(f: &Formation) -> Result<(), String>;
pub fn role(f: &Formation, agent: &str) -> Role;
pub fn mukade_role(f: &Formation, agent: &str) -> MukadeRole;
/// `stopped`: agents whose session exists but is not running.
pub fn check(f: &Formation, stopped: &BTreeSet<String>) -> Vec<Issue>;
```

Issue codes: `loop`, `only_executes`, `observer_writes`, `two_leads`, `no_model` (an agent with neither a session nor a model), `two_givers` (warning), `stopped` (warning). Rules and role table: spec sections 3 and 4.

- [ ] **Step 1: Failing tests**, with the spec's values:

```rust
#[test] fn roles_follow_the_arrows() {
    let f = team(&["a","b","c","d"], &[("a","b",Work,Done), ("b","c",Work,Strict), ("d","a",Observes,Done)]);
    assert_eq!(role(&f,"a"), Role::Lead);        assert_eq!(mukade_role(&f,"a"), MukadeRole::Peer);
    assert_eq!(role(&f,"b"), Role::LeadWorker);  assert_eq!(mukade_role(&f,"b"), MukadeRole::Lead);
    assert_eq!(role(&f,"c"), Role::Worker);      assert_eq!(mukade_role(&f,"c"), MukadeRole::Worker);
    assert_eq!(role(&f,"d"), Role::Observer);    assert_eq!(mukade_role(&f,"d"), MukadeRole::Observer);
}
#[test] fn a_loop_of_work_is_an_error_naming_its_arrows() {
    let f = team(&["a","b","c"], &[("a","b",Work,Done), ("b","c",Work,Done), ("c","a",Work,Done)]);
    let issue = &check(&f, &none())[0];
    assert_eq!((issue.level, issue.code), (Level::Error, "loop"));
    assert_eq!(issue.links, vec![0,1,2]);
}
#[test] fn who_only_executes_holds_nothing_else_but_being_observed() { /* a→b Strict + b→c Work → error only_executes; a→b Strict + c observes b → no error */ }
#[test] fn an_observer_does_not_work() { /* a observes b + c→a Work → error observer_writes */ }
#[test] fn two_strict_givers_is_an_error_two_plain_givers_a_warning() {}
#[test] fn work_for_a_stopped_session_is_a_warning() {}
#[test] fn validate_refuses_at_signs_long_words_blank_names_unknown_ends_and_two_arrows_between_two_agents() {
    // "@all" in words; 121 characters of words; a name of spaces; a link to a missing agent;
    // a→b and b→a together; 25 agents; 65 links.
}
```
- [ ] **Step 2: Run** `cargo test -p shinatama-engine team::` — expected: FAIL to compile.
- [ ] **Step 3: Implement.** The loop search follows work arrows from each arrow's end back to its start, bounded by the number of agents.
- [ ] **Step 4: Run** — expected: PASS.
- [ ] **Step 5: Commit** `feat(team): the formation, its inferred roles and its checks`.

### Task 2: What each session is told

**Files:** `crates/engine/src/team.rs` and its tests.

**Interfaces — Produces:**

```rust
/// The sentence for one end of a relation. `giver`: the end the arrow starts from.
pub fn told(f: &Formation, link: usize, giver: bool) -> String;
/// `members`: Mukade member name by agent id, for those in the room.
pub fn personal(f: &Formation, agent: &str, members: &BTreeMap<String, String>) -> String;
pub struct Changes { pub touched: Vec<String>, pub removed: Vec<Agent>, pub lines: Vec<String> }
/// Places (`x`, `y`) are not a change.
pub fn changes(before: Option<&Formation>, after: &Formation) -> Changes;
pub fn notice(revision: u64, lines: &[String]) -> String;
pub struct Evaluation { pub roles: BTreeMap<String, RoleView>, pub issues: Vec<Issue>, pub sentences: Vec<[String; 2]> }
pub struct RoleView { pub role: Role, pub detail: String, pub mukade: MukadeRole, pub hand_out: Option<String> }
pub fn evaluate(f: &Formation, stopped: &BTreeSet<String>) -> Evaluation;
```

- [ ] **Step 1: Failing tests** with the exact sentences of spec section 5 ("What is said"): one test per row (when done, at every step, only done / blocked, observes, words appended to both ends, the three hand-outs with "B, C and D"); `personal_names_the_agent_its_member_its_role_then_the_formation`; `a_personal_message_stays_within_2048_bytes` (24 agents, 64 links, longest fields: the formation list is cut with "… and N more", the agent's own sentences never); `moving_a_card_is_not_a_change`; `a_changed_arrow_touches_both_its_ends_and_no_one_else`.
- [ ] **Step 2: Run** `cargo test -p shinatama-engine team::` — expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — expected: PASS.
- [ ] **Step 5: Commit** `feat(team): the sentences a session is told and what a change concerns`.

### Task 3: Storing the formation

**Files:** `crates/state/src/database.rs` (beside `chat_threads` / `set_thread_chat`); tests in the same file's test module.

**Interfaces — Produces:**

```rust
/// What a thread keeps: the applied formation and, per agent, what it was last told.
pub fn thread_team(&self, workset_id: &str) -> Result<Option<serde_json::Value>>;
pub fn set_thread_team(&self, workset_id: &str, record: Option<&serde_json::Value>) -> Result<()>;
```
Setting key `threads.team.v1`: an object from workset id to the record. The record's shape belongs to the engine: `{ "formation": Formation, "told": { "<agent id>": { "revision": u64, "member": "<member id>" } } }`.

- [ ] **Step 1: Failing tests** `a_thread_s_team_is_kept_and_removed`, `an_invalid_thread_id_is_refused`, `another_thread_s_team_is_untouched`.
- [ ] **Step 2: Run** `cargo test -p shinatama-state team` — expected: FAIL.
- [ ] **Step 3: Implement** with `setting_json` / `set_setting_json`, validating the id as `set_thread_chat` does.
- [ ] **Step 4: Run** — expected: PASS. **Step 5: Commit** `feat(state): a thread keeps its formation`.

### Task 4: The window's commands

**Files:** Create `apps/desktop/src-tauri/src/thread_team.rs`; Modify `apps/desktop/src-tauri/src/main.rs` (register in both handler lists, beside `thread_chat::thread_chat_send`).

**Interfaces — Produces** (requests `camelCase`, `deny_unknown_fields`, guarded by `crate::local(&window)` as `thread_chat` does):

| Command | Request | Answer |
|---|---|---|
| `team_get` | `{ worksetId }` | `{ enabled, formation: Formation \| null, told, sessions: [{ sessionId, harness, title, state, memberName, connection, wakes }], evaluation }` — `sessions` as `thread_chat::project` gives them |
| `team_evaluate` | `{ worksetId, formation }` | `Evaluation` |
| `team_apply` | `{ worksetId, formation, expectedRevision }` | `{ formation }` with `revision = expectedRevision + 1` |
| `team_place` | `{ worksetId, places: { "<agent id>": [x, y] } }` | `{}` — changes only `x`, `y` of the stored formation; no revision |

`team_apply` refuses, in this order: a thread that is off; `validate`; an agent whose `session` is not a session of this thread; a stored revision other than `expectedRevision` (`"The team was changed elsewhere (revision N). Reload it."`); any `Level::Error` from `check`. It then stores and wakes the announcer (`team_announcer::wake()`).

- [ ] **Step 1: Failing tests** on the pure parts, in `thread_team.rs`: `a_stale_revision_is_refused`, `a_formation_with_an_error_is_not_stored`, `a_session_of_another_thread_is_refused`, `placing_changes_no_revision`.
- [ ] **Step 2: Run** `cargo test -p shinatama-desktop thread_team` — expected: FAIL.
- [ ] **Step 3: Implement**, the storing logic as a function over `&Database` so the tests need no window.
- [ ] **Step 4: Run** — expected: PASS. **Step 5: Commit** `feat(desktop): the team commands`.

### Task 5: Telling the sessions

**Files:** `crates/engine/src/team.rs` (`deliveries`); Create `apps/desktop/src-tauri/src/team_announcer.rs`; Modify `thread_chat.rs` (`user_handle` → `host_handle(state, controller, room, name)` for `USER_MEMBER` and a new `TEAM_MEMBER = "shinatama"`, joined with `--host`), `collaboration.rs` (`start` spawns the worker, `stop` ends it), `crates/engine/src/mukade_chat.rs` (`pub const TEAM_MEMBER`).

**Interfaces — Produces:**

```rust
/// One agent as the room sees it now.
pub struct Seat { pub member_id: String, pub member_name: String, pub role: String, pub in_room: bool }
pub struct Told { pub revision: u64, pub member: String }
pub enum Delivery {
    SetRole  { member_id: String, role: MukadeRole },
    Personal { agent: String, member_name: String, text: String },
    Notice   { text: String },
    Retire   { member_id: String },
}
/// What is owed, given what was stored, what was told and who is in the room. Pure and
/// idempotent: with `told` updated from a run's successes, a second call returns nothing.
pub fn deliveries(before: Option<&Formation>, now: &Formation, told: &BTreeMap<String, Told>,
                  seats: &BTreeMap<String, Seat>, closed: &[Seat]) -> Vec<Delivery>;
```
Order within one run: `SetRole` for every seat whose role differs from `mukade_role`; then `Personal` for each agent in the room whose `told` is absent, behind `now.revision`, or names another `member`; then one `Notice` when `changes(before, now).lines` is not empty and a member other than those just told is in the room; then `Retire`.

The worker: for each talking thread with a record, build `seats` from `thread_chat`'s members and session ties, call `deliveries`, execute each through `mukade_chat::call` (`set-role`, `send` with `--recipients`, `send` without, `retire`) with the `shinatama` handle, and store `told` after each success. A refused send (`stale_member`) recovers the handle once, as `thread_chat_send` does. A timeout: read the ledger since the send for a `shinatama` message whose text starts `TEAM revision <n>` addressed to that member; found → told; not found → leave it for the next check. Any other refusal is kept per agent as the reason `team_get` returns.

- [ ] **Step 1: Failing tests** for `deliveries`: `a_new_formation_tells_everyone_in_the_room_and_sends_no_notice_to_nobody`; `one_changed_arrow_tells_its_two_ends_and_notices_the_rest`; `an_agent_not_in_the_room_is_told_when_it_joins`; `a_new_member_of_a_told_agent_is_told_again_once`; `nothing_is_owed_twice`; `an_agent_taken_off_the_map_is_told_it_has_no_place`; `an_agent_whose_session_left_the_thread_gets_nothing`; `a_role_is_set_before_the_message_that_explains_it`; `a_closed_session_gets_a_closed_notice_and_its_member_is_retired`. For the worker's decisions, as pure functions in `team_announcer.rs`: `an_uncertain_send_is_looked_up_before_it_is_repeated`, `a_refused_message_marks_the_agent_not_told`.
- [ ] **Step 2: Run** `cargo test -p shinatama-engine team:: && cargo test -p shinatama-desktop team_announcer` — expected: FAIL.
- [ ] **Step 3: Implement.** The worker sleeps on a condition variable `wake()` signals, with a 10 s timeout; a check with nothing owed makes no service call beyond the one `rooms` listing `thread_chat_overview` already makes.
- [ ] **Step 4: Run** — expected: PASS.
- [ ] **Step 5: Commit** `feat(team): Shinatama tells each session its place, once`.

### Task 6: The launch role and `shinatama_thread_peers`

**Files:** `crates/engine/src/mukade_chat.rs` (`ChatHooks`), `apps/desktop/src-tauri/src/thread_chat.rs` (`launch_hooks`), `apps/desktop/src-tauri/src/agents/service.rs:753` (the call), `apps/desktop/src-tauri/src/thread_peers.rs`, `.claude/skills/mukade-collaborate/SKILL.md` and `references/mcp-contract.md`.

**Interfaces:**
- `ChatHooks::with_role(self, role: &'static str) -> Self`; `apply_environment` writes it to `MUKADE_CHAT_ROLE` (today the literal `"peer"`).
- `launch_hooks(state, harness, workset, session: Option<&str>)`: the role is `team::mukade_role` of the agent whose `session` it is, else `peer`.
- `thread_peers::projection` gains `team: Option<&TeamView>` and, when present, puts `"team": { "revision", "you": { "name", "role", "relations": [sentences] }, "peers": { "<session id>": { "name", "role" } }, "note": "Set by the user in Shinatama's Team map; the member shinatama relays it." }`.

- [ ] **Step 1: Failing tests** `the_launch_takes_the_agent_s_mukade_role` (in `mukade_chat.rs`: `with_role("observer")` → `MUKADE_CHAT_ROLE` is `observer`; without it, `peer`), `a_thread_without_a_formation_launches_as_before` (the environment equals today's), `peers_report_the_team` and `peers_without_a_team_are_unchanged` (in `thread_peers.rs`, the existing three tests still pass untouched).
- [ ] **Step 2: Run** the three crates' tests — expected: FAIL. **Step 3: Implement.** **Step 4: Run** — expected: PASS.
- [ ] **Step 5:** Update the skill's two files (the `team` object, the member `shinatama`, roles set by the host). Commit `feat(team): a session starts in its role and can read its team`.

### Task 7: The map's model

**Files:** Create `apps/desktop/src/team-map-model.ts`; Test `apps/desktop/tests/team-map-model.test.ts`.

**Interfaces — Produces:**

```ts
export type Share = 'split' | 'same' | 'free'; export type Kind = 'work' | 'observes'; export type Report = 'done' | 'steps' | 'strict';
export type ModelRef = { program: string; connection: string | null; model: string | null };
export type Agent = { id: string; session: string | null; model: ModelRef | null; name: string; notes: string; share: Share; x: number; y: number };
export type Link = { from: string; to: string; kind: Kind; report: Report; words: string };
export type Formation = { revision: number; agents: Agent[]; links: Link[] };
export type Issue = { level: 'error' | 'warning'; code: string; text: string; agents: string[]; links: number[] };
export type Evaluation = { roles: Record<string, { role: string; detail: string; mukade: string; handOut: string | null }>; issues: Issue[]; sentences: [string, string][] };
export function parseTeam(value: unknown): TeamSnapshot | null;      // the answer of team_get
export function parseEvaluation(value: unknown): Evaluation | null;
export function newAgentId(): string;                                 // "a_" + 12 of [a-z2-7]
export function addSession(f: Formation, session: string, name: string, at: Point): Formation;
export function removeAgent(f: Formation, agent: string): Formation;  // with its arrows
export function connect(f: Formation, from: string, to: string): Formation;  // replaces the pair's arrow; kind work, report done
export function changed(applied: Formation | null, draft: Formation): boolean;  // places ignored
export function pendingLinks(applied: Formation | null, draft: Formation): Set<number>;
export function borderPoint(card: Rect, towards: Point, gap: number): Point;
export function tidy(f: Formation, roles: Evaluation['roles'], width: number): Record<string, Point>;
export function arrange(f: Formation, shape: 'star' | 'chain' | 'peers', hub: string | null, width: number): Formation;
export const CARD = { width: 190, height: 78 } as const;
```
`tidy` follows spec section 11; `arrange` follows the mockup's `shape` and `arrange` (star: the hub gives work to every other non-observer, corners for up to four spokes, an ellipse beyond; chain: each gives work to the next; peers: work arrows removed; `observes` arrows always kept).

- [ ] **Step 1: Failing tests:** `connect_replaces_the_arrow_between_two_agents`; `removing_an_agent_removes_its_arrows`; `moving_a_card_is_not_a_change`; `tidy_puts_givers_above_takers_and_overlaps_nothing` (the seven-agent case tried on the mockup: a→d, a→b, b→x1, d→x2, c observes b, e alone, width 960: a in the first row, b and d in the second, x1 under b and x2 under d, e in the last row, c right of everything at b's height; no two cards closer than 190 × 78); `tidy_survives_a_loop`; `a_wide_layer_wraps`; `star_keeps_observers_and_points_the_hub_at_everyone`; `parse_refuses_what_is_not_a_team`.
- [ ] **Step 2: Run** `npm test` — expected: FAIL.
- [ ] **Step 3: Implement.** **Step 4: Run** `npm test && npx tsc --noEmit` — expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): the team map's model`.

### Task 8: The map's view

**Files:** Create `apps/desktop/src/team-map.ts`, `apps/desktop/src/team-map.css`; Modify `apps/desktop/src/main.ts` (a pane resource kind `team`, wired where the `browser` kind is: `rg "kind === 'browser'" apps/desktop/src`), `apps/desktop/src/workset-sidebar.ts` (a **Team** button beside Chat on a talking thread's row; sessions of the thread draggable onto the map), `apps/desktop/src/thread-chat.ts` (messages of the member `shinatama` drawn as notices); Test `apps/desktop/tests/team-map-smoke.mjs` and a `test:team` script in `package.json`, modelled on `model-settings-smoke.mjs`.

**Interfaces:**
- Consumes: task 7's functions; the commands of task 4 through the same `invoke` wrapper `thread-chat.ts` uses.
- Produces: `createTeamMap(deps): { open(thread), close(), has(viewId), focus(viewId), setVisible(viewId, visible), label(viewId) }`, the shape `createBrowserTabs` gives the pane host.

Behaviour to build, each line a thing the smoke test drives: the empty map and its hint; a session dragged from the thread's list lands where it is dropped; a card moves and `team_place` is called when nothing else is pending; pulling → onto a card creates a work arrow and opens its box; the box changes kind, report and words, reverses, deletes; a struck-out option shows the reason `team_evaluate` gave for the trial formation; the card's box edits name and notes and shows the role; hand-out appears with two or more work arrows; the problems strip lists `evaluation.issues` and Apply is disabled on an error; Tidy up, keep tidy, Star, Chain, Peers; Apply calls `team_apply` and the cards show "told" states from `team_get`, polled every 10 s while the view is visible; Discard; a card dragged to the bottom strip leaves the map. `team_evaluate` is called at most once per 150 ms while editing.

Accessibility and skin: every action reachable from the keyboard (cards and arrows are focusable; Enter opens the box; the box offers "Connect to…" as a menu, the keyboard's way to draw an arrow); colours only from `styles.css` tokens; no rounded corners except the card's dot and the → handle; error never by colour alone (the ✕ and the text).

- [ ] **Step 1: Write the smoke test** driving the list above against a real window with two Claude Code sessions stubbed as the other smokes stub agents; it fails for the missing view.
- [ ] **Step 2: Run** `npm run test:team` — expected: FAIL.
- [ ] **Step 3: Implement the view.**
- [ ] **Step 4: Run** `npx tsc --noEmit && npm test && npm run build && npm run test:team` — expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): the team map`.

### Task 9: With real agents

- [ ] **Step 1:** Build and copy the executables over `D:\Software\Shinatama` in place (no backup).
- [ ] **Step 2:** Run ADR-0057's confirmation, then: a thread that is on, one Claude Code session and one OpenCode session whose model is `janura-qwen`, both opened by hand; put both on the map; Claude gives work to OpenCode, at every step; Apply. Then say to Claude only "have the implementer run docs/plans/<a small plan>" and watch it reach OpenCode. Repeat the Apply with a Codex and a Pi session added. Record in `docs/verification/2026-11-…-team-map.md` the text each session received, that the heading says it comes from the user, the wake counts before and after, and that a third session not concerned stayed asleep on a later change.
- [ ] **Step 3:** Draw `observes` from Codex to Claude, Apply: Codex is refused `chat_send`, and `chat_observe` of Claude works. Make OpenCode "only done / blocked": it answers with a signal, and Claude's role in the room is `lead`.
- [ ] **Step 4:** Restart Shinatama, resume both: neither is told again; change one word, Apply: both ends are told once.
- [ ] **Step 5:** Record what failed as it failed. Set ADR-0058 Accepted only if the owner approves its text.

---

## Phase 2 — Templates and agents without a session

### Task 10: Templates in the engine and the state

**Files:** `crates/engine/src/team.rs`, `crates/state/src/database.rs`, `apps/desktop/src-tauri/src/thread_team.rs`.

**Interfaces — Produces:**

```rust
pub struct Template { pub id: String, pub name: String, pub agents: Vec<Agent>, pub links: Vec<Link> }  // every agent.session is None
pub fn template_of(id: String, name: &str, f: &Formation) -> Result<Template, String>;  // refuses a formation with an error
pub fn team_templates(&self) -> Result<Vec<serde_json::Value>>;            // Database; key "team.templates.v1"
pub fn set_team_templates(&self, list: &[serde_json::Value]) -> Result<()>;
```
Commands: `team_templates` → `[Template]`; `team_template_save { name, formation }` → the list; `team_template_delete { id }` → the list. Name 1..60 characters, unique without regard to case; 64 at most.

- [ ] **Step 1: Failing tests** `a_template_keeps_no_session_and_no_revision`, `a_formation_with_a_loop_is_not_saved`, `a_second_template_of_the_same_name_is_refused`, `the_sixty_fifth_template_is_refused`, `templates_outlive_a_thread` (state).
- [ ] **Step 2–4:** run, implement, run (`cargo test -p shinatama-engine team:: -p shinatama-state team -p shinatama-desktop thread_team`).
- [ ] **Step 5: Commit** `feat(team): templates, kept for every project`.

### Task 11: Templates and empty agents on the map

**Files:** `apps/desktop/src/team-map-model.ts`, `team-map.ts`, `team-map.css`, their tests.

**Interfaces — Produces:**

```ts
export function fromTemplate(t: Template, width: number, id: () => string): Formation;  // new agent ids, places scaled to the width
export function fillAgent(f: Formation, agent: string, session: string): Formation;       // the session takes the agent's place, name and arrows
export function miniature(t: Template): { cards: Point[]; lines: { a: Point; b: Point; kind: Kind }[] };  // in a 64 × 40 box
```
The view: a Templates menu (list with miniature, agent count, Use, delete; a name field and "Save this team", disabled on an empty map or an error); **+ New agent**; an agent without a session drawn hatched with "no session yet"; a session dropped on it fills it. In this phase Apply is disabled while an agent has no session, with the reason "Drop a session on <name>, or take it off the map" (phase 3 lifts this).

- [ ] **Step 1: Failing tests** `a_template_gets_new_ids_and_fits_the_width`, `a_session_dropped_on_an_agent_takes_its_arrows`, `a_miniature_stays_in_its_box`; the smoke test gains: save, see it listed after a restart of the window, use it in another thread, fill by dropping.
- [ ] **Step 2–4:** run, implement, run. **Step 5: Commit** `feat(desktop): team templates on the map`.

---

## Phase 3 — A model per agent, and Apply opens the sessions

### Task 12: A launch with the agent's model

**Files:**
- Modify: `crates/engine/src/model_config.rs`, `crates/engine/src/opencode_launch.rs` (near line 641), `crates/engine/src/pi_launch.rs` (near line 547), their start contexts
- Test: the three modules' own tests

**Interfaces — Produces:**

```rust
impl Snapshot {
    /// The same configuration with one program's selection replaced, for one launch. `None`:
    /// the program's own sign-in and model, whatever the Models panel assigns.
    pub fn with_selection(self, harness: Harness, selection: Option<Selection>) -> Result<Snapshot, String>;
}
```
It fails when the connection or the model is no longer in the configuration ("The model <id> is no longer in the Models panel."). `OpenCodeStartContext` and `PiStartContext` gain `pub model: Option<Option<&'a Selection>>` (outer `None`: the panel's assignment, as today).

- [ ] **Step 1: Failing tests** `one_launch_takes_another_selection_and_the_panel_keeps_its_own`, `a_removed_model_is_refused_by_name`, `pi_passes_the_agent_s_model` (the prepared arguments hold `--provider <alias>` `--model <id>` of the override), `opencode_s_configuration_holds_the_agent_s_model` (the inline configuration built by `Snapshot::opencode`), `no_override_launches_as_before` (arguments and configuration equal today's).
- [ ] **Step 2: Run** `cargo test -p shinatama-engine model_config opencode_launch pi_launch` — expected: FAIL. **Step 3: Implement.** **Step 4: Run** — expected: PASS.
- [ ] **Step 5: Commit** `feat(models): one launch can take its own model`.

### Task 13: A first message for OpenCode and Pi

**Files:** `crates/engine/src/opencode_launch.rs`, `crates/engine/src/pi_launch.rs`, their contexts and tests.

**Interfaces:** both start contexts gain `pub initial_prompt: Option<&'a str>`, at most 8192 bytes, no NUL, as `collaboration.rs` bounds it. OpenCode: `--prompt <text>`. Pi: `--` then the text as the last argument, after every option.

- [ ] **Step 1: Verify with the real programs first:** by hand, `opencode --standalone --prompt "say only: ready"` and `pi -- "say only: ready"` open the interactive interface and run that turn. Add what happened to the verification note of task 0. **If one does not, stop and tell the owner.**
- [ ] **Step 2: Failing tests** `opencode_takes_its_first_message`, `pi_takes_its_first_message_last` (the text is the final argument and follows `--`), `a_first_message_with_a_nul_or_over_8192_bytes_is_refused`, `the_fullest_launch_stays_within_32_arguments` (chat on, Marionette on, model override, session id, first message; both programs).
- [ ] **Step 3: Run** — expected: FAIL. **Step 4: Implement.** **Step 5: Run** — expected: PASS.
- [ ] **Step 6: Commit** `feat(launch): OpenCode and Pi start with a first message`.

### Task 14: The model menu and its checks

**Files:** `crates/engine/src/team.rs`, `apps/desktop/src-tauri/src/thread_team.rs`, `apps/desktop/src/team-map-model.ts`, `team-map.ts`.

**Interfaces:**
- `team_get` and `team_evaluate` take the configured models from `model_config::current()` and report `no_model` (error) for an agent without a session and without a model, and `model_gone` (error) for a model the panel no longer holds; `check` gains the parameter `models: &BTreeSet<(String, Option<String>, Option<String>)>` (program, connection, model) for that.
- `team_get` answers `models: [{ program, connection, model, label }]`: the four "its own sign-in and model" entries first, then each model of each connection OpenCode or Pi can use, once per program; `label` is `"<model> · runs in <Program>"`.
- The card's box: a menu of those for an agent without a session; for a running one the model and program, fixed. The card shows `<model> · via <Program>`.
- `fromTemplate` keeps a template's model only if `models` holds it; otherwise the agent has none.

- [ ] **Step 1: Failing tests** `an_agent_without_a_session_needs_a_model`, `a_model_no_longer_configured_is_an_error_naming_it`, `a_running_agent_s_model_is_not_checked`, `a_template_s_missing_model_leaves_the_agent_without_one` (TypeScript).
- [ ] **Step 2–4:** run, implement, run (`cargo test -p shinatama-engine team:: && npm test`). **Step 5: Commit** `feat(team): an agent's model, chosen from the Models panel`.

### Task 15: Apply opens the sessions

**Files:** `apps/desktop/src-tauri/src/agents.rs` (`open_collaboration` refuses everything but Claude Code and Codex at line 496: a new `open_agent` beside it), `apps/desktop/src-tauri/src/agents/service.rs`, `agents/opencode.rs`, `agents/pi.rs` (pass `model` and `initial_prompt` into the start contexts), `apps/desktop/src-tauri/src/thread_team.rs` (`team_apply`), `crates/engine/src/team.rs`, `team-map.ts`.

**Interfaces — Produces:**

```rust
/// Opens a session of `program` in `workset`, with the agent's model and first message.
pub async fn open_agent(self: &Arc<Self>, program: &str, host: Arc<TerminalHost>, project: PathBuf,
    model: Option<Selection>, own_model: bool, initial_prompt: String, workset: String) -> Result<AgentStarted, &'static str>;
/// "You are <name> in this thread's team. The thread's Mukade room is <room>. Your role and
/// relations arrive there from the user, through the member shinatama. Wait for them before acting."
pub fn opening(agent_name: &str, room_name: &str) -> String;   // in team.rs
```
- `team_apply` opens, one after another, a session for each agent without one, sets `agent.session` to the returned thread id, emits `collaboration-session-opened` as `collaboration.rs` does, then stores and wakes the announcer.
- A launch that fails stops the opening, stores the formation with the sessions opened so far, and answers `{ formation, failed: { agent, reason } }`: the map shows the reason on that card and Apply reads "Open 1 session and apply" again.
- The thread's switch must be on (it is, or the map would not open); the Mukade service must be reachable before the first launch, else nothing is opened.
- The window's Apply button reads "Open N sessions and apply" when N agents have none.

- [ ] **Step 1: Failing tests** on the pure sequencing, with the opener as a closure: `sessions_are_opened_before_the_formation_is_stored`, `a_failed_opening_keeps_what_was_opened_and_names_the_agent`, `an_agent_opened_once_is_not_opened_again_on_retry`, `nothing_is_opened_while_the_service_is_unreachable`.
- [ ] **Step 2–4:** run, implement, run. The smoke test gains: a template used in an empty thread, Apply, the tabs appear.
- [ ] **Step 5: Verify with real agents.** In an empty thread of another project, use a template whose agents are Claude Code (its own model) and OpenCode on `janura-qwen`; Apply. Record: both tabs open; OpenCode lists and uses `janura-qwen` although the Models panel assigns OpenCode something else; each session states its role unprompted after its first message; the Conversation panel ties each member to its tab. Then the same with a Pi and a Codex agent. Record what failed as it failed.
- [ ] **Step 6: Commit** `feat(team): Apply opens the sessions an agent lacks`.

---

## Phase 4 — An agent opens OpenCode or Pi

### Task 16: `shinatama_session_open` for all four programs

**Files:** `crates/engine/src/session_control.rs` (the check at line 405, the schema at line 442, the tool's description), `apps/desktop/src-tauri/src/collaboration.rs` (`open`, `initial_prompt`), `.claude/skills/mukade-collaborate/SKILL.md`, `references/mcp-contract.md`.

**Interfaces:**
- `harness` accepts `claude_code`, `codex`, `opencode`, `pi`; the default stays `claude_code`; an explicit harness is never replaced by another.
- For `opencode` and `pi`: the caller's thread must have its switch on and `room_id` must be that thread's room, else `failed` with "An OpenCode or Pi session joins a room only in a thread whose sessions talk to each other. Turn it on in the thread's settings."; the launch goes through `open_agent` with the program's own or assigned model.
- Their bootstrap leaves out `shinatama_thread_peers` and the manual `chat_join` path (they have neither), and says: the kit has joined you; send READY to the coordinator named here.
- The session is created in the caller's thread and is not put on the map.

- [ ] **Step 1: Failing tests** `opencode_and_pi_are_accepted_and_an_unknown_harness_is_not` (extends `defaults_to_claude_but_never_substitutes_explicit_codex`), `a_kit_session_is_refused_in_a_thread_that_does_not_talk`, `the_kit_bootstrap_names_no_tool_the_session_lacks` (the text holds neither `shinatama_thread_peers` nor `chat_join`).
- [ ] **Step 2–4:** run (`cargo test -p shinatama-engine session_control && cargo test -p shinatama-desktop collaboration`), implement, run.
- [ ] **Step 5:** Update the skill and its contract: the four harnesses, the `team` object, the member `shinatama`, roles set by the host, OpenCode and Pi tied to their tabs. Remove "un peer con `chat: \"none\"` (OpenCode, Pi) non ha strumenti Mukade".
- [ ] **Step 6: Verify with real agents:** in a talking thread, tell a Claude Code session "open an OpenCode session and have it run <a small plan>"; it opens, READY arrives, the task is delivered, RESULT comes back. Record it. Commit `feat(collaboration): an agent can open OpenCode and Pi sessions`.

---

## Not in this plan

Changing the model of a running session; resuming an OpenCode or Pi session (the id chosen at
launch makes it possible, but it is its own piece of work); sharing templates between computers.
