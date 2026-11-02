# Verification register

**Implementation tracking:** [complete plan](implementation/PLAN.md),
[checkpoint](implementation/progress.json). B02 delivers a verified Windows slice
of recent project selection; [its evidence](verification/2026-11-02-B02-project-history.md)
does not close the complete PM or cross-platform S gates.

**2026-11-02:** Tauri is the definitive V1 shell (ADR-0005 v2, ADR-0047 v2). See the
prototype's [verification record](verification/2026-11-02-tauri-terminal-prototype.md)
and [scope note](implementation/tauri-prototype.md). Windows shell and layout checks are
partial evidence; full TL/P gates and M0 are not passed.

**Windows only, 2026-11-02:** Windows is the only officially supported platform for now
(ADR-0047; [deferral](implementation/2026-11-02-macos-linux-deferred.md)). A row that names macOS,
Linux or "3 OS" runs on Windows only; the other systems' parts stay open and deferred, and none is
reported as passed.

**Daodan deferred, 2026-11-02:** R1, D4, D20 and the other rows that need Daodan are out of
the active cycle until the user resumes it
([deferral](implementation/2026-11-02-daodan-deferred.md)). They are not passed.

Date: 2026-10-26. Status: draft. Companion of
[the functional design](functional-design.md) and
[the technical design](technical-design.md).

Every claim in the specs that has not been proved by running something on a real machine
is listed here, once, with how to prove it, what counts as proved, and where the result is
written back. Nothing in this file has been run yet, except where a row says **observed**.

## Native tiling revision — 2026-10-29

These gates cover functional design 5.16 and technical design 3.1. They have not
been executed. The toolkit remains a proposal; native Rust UI is the requirement.

| ID | Priority | Verification and pass criterion | Status |
| --- | --- | --- | --- |
| TL1 | G0 | Shell proof of technical design 3.1, not the terminal-first UI milestone of functional design 5.12 and 5.16 (which has no file navigator). Prove the Tauri shell (ADR-0005) with real agent PTYs, nested splits and Markdown/file panels on Windows in WebView2. No required Node, development server or browser startup; record the WebView2 version and gaps. A failure revises ADR-0005. | open |
| TL2 | G1 | Mouse and keyboard split/resize/tab move/collapse/maximize work, including cancelled drops and small windows. Session identity/output stay intact; final PTY size is correct, hidden output is drained and modal/background input is isolated. Include IME, Unicode and scaling. | open |
| TL3 | G1 | Per-project/window layout survives restart; corrupt/unknown state recovers without lost documents/history, conflicting workspace ownership is rejected and restoration starts no agents, apps or interactive browsers. | open |
| TL4 | G1 | Markdown source/preview and dirty close preserve buffers; concurrent agent writes, save failures and stale render results cannot silently overwrite content. Hostile Markdown, paths/symlinks and remote assets cannot exceed grants. Test visual editor fidelity separately before enabling it. | open |
| TL5 | G1 | Many noisy/hidden agent panes and large previews remain within explicit resource limits without starving UI or other panes; record measured CPU/memory and responsiveness, then set budgets rather than claim them in advance. | open |
| TL6 | G1 | Before DOCX reader, interactive embedded browser or mini-app activation, record format fidelity/capability limits, isolation/grants, lifecycle and target-platform results. Unsupported features remain unavailable; no assumed JanuraCode app compatibility. Browser B-series gates still apply. | open |

Write results to `docs/verification/YYYY-MM-DD-TL<n>-<topic>.md` under the rules
below, then update the corresponding spec. No pass is claimed by adding these rows.

## 0. Rules for running and reporting

**Priority.** `G0` gate: if it fails, the design changes; nothing is built on it before it
passes (the first milestone). `G1` blocks one feature, not the architecture. `G2` tunes a
default, a target or a number.

**Status** (the column that gets updated): `open`, `running`, `passed`, `failed`,
`changed` (passed with the design adjusted), `dropped` (no longer needed, with the reason).

**One record per item**, written when it is run: `docs/verification/YYYY-MM-DD-<ID>-<topic>.md`
holding: the item id; date; machine and OS version; the versions of every agent and tool
involved (`--version` output); the exact commands or the script path; the raw result
(structure only when the data is private, see below); pass or fail against the stated
criterion; what the spec said before; what it says now. Scripts live in `verify/<ID>/` in
the repository. Reruns depend on the recorded machine, agent versions and prerequisites;
check each script and record before running. Raw output under `verify/*/out/` is ignored by
Git, and some records point to machine-local paths, so not all evidence is portable with
the repository. External records are linked as such rather than stored in this directory.

**Write-back is part of the item.** An item is not `passed` until the spec sentence it
backs has been changed from `[U]` (or `[C]`, `[R]`, `[D]`) to `[V]` with the date and the
record's file name, or has been corrected. A failed item gets the same treatment: the spec
is changed and the failure is recorded, not deleted.

**Fixtures** are captured from synthetic sessions created for the test, with invented
content, a marker string per turn, and the agent version in the file name. Real
conversations of the user are never copied into the repository or into a record; where a
check needs their shape (for example the size of the OpenCode database), only counts and
column names are recorded.

**Scratch homes.** Anything that writes uses a scratch copy of the agent's home so the
user's real sessions, indexes and settings are untouched: `CLAUDE_CONFIG_DIR`,
`CODEX_HOME`, `OPENCODE_DB` plus `XDG_*`, `PI_CODING_AGENT_DIR`. A scratch home starts signed
out. Tests that need a live model therefore need **one login per scratch home, done by the
user**; credential files are never copied into the repository, a record or a scratch home by
the test code. Each live-model test uses tiny prompts and a stated cost cap, and the cost
is written in the record. Which accounts are used is the user's decision (open question O1).

**Reading from the real homes** (listing, structure) is read-only, with shared read access,
and never `immutable` on a database an agent may be writing.

**Version drift.** Every [V] and [R] fact carries the version it was observed on. When an
agent's version changes, `fixture_gate` (engine 5.5) compares the fixtures and drops the
adapter to degraded until its suite passes; this register's agent rows are rerun on the
new version before the release notes claim support.

**Security-sensitive items** (marked `SEC`) are also reviewed by a second reader and their
records state the negative result as well as the positive.

**Size.** 266 rows in the native tiling table and sections 2 to 19a (7a, 7b and 14a included): 107 `G0`, 144 `G1`, 15 `G2`; 24 are marked `SEC` (counted by script on 2026-10-31, after HM1 to HM14, DS1 to DS15 and D20 were added for Hermes Agent and DeepSeek Harness; the 2026-10-28 count of 213 preceded TL1 to TL6, B1 to B10, SS1 to SS6 and M15). `G0` means the
gate of its own component, not of the whole first milestone. The **first-milestone set** is
the subset that decides the foundations: P1, P2, P3, P4, P9, P12, P14, P16, P17, S1, S2,
S9, CL1, CL2, CL4, CX1, CX2, CX4, OC1, PI1, PI2, PI11, HM1, HM2, DS1, DS2, M1, U3, X3, TL1 (30 rows; CX3 left the set on 2026-10-31 because native import is post-v1, TL1 entered with the native tiling revision, HM1, HM2, DS1 and DS2 with the two agents added the same day). The
remaining `G0` rows gate their own feature (the hop, the messaging layer, the widget, the
sidecar port) and run when that feature is built. The engine design's section 10 is the
short form of this set; this register is the complete one and wins where they differ.

## 1. Spec statements that this list found questionable (to fix, or to prove)

| # | Statement in the specs | Why it is suspect | Row |
|---|---|---|---|
| Q1 | Shinatama is the "companion marketplace" in `D:\Projects\shinatama`; the product reuses the name | Daodan's README says the 24 plugins "returned" to Daodan in 42.0.0 (commit "Reunify Shinatama into Daodan as one marketplace"); the other repo's last commit is 2026-10-25. The role of `acaprino/shinatama` today is unclear, and the product name may collide with it. **Settled 2026-10-27:** the user decided only Daodan is the plugin marketplace, and the remote `acaprino/shinatama` was deleted after a bundle backup | D1 |
| Q2 | Pi can run a backend that the real TUI attaches to (engine 5.7, item 4: "Pi (rpc)") | Pi's `--mode rpc` is a long-lived protocol; nothing found says the Pi TUI can attach to it, unlike Codex `--remote` and OpenCode `--server` | PI5 |
| Q3 | Two instances changing the same file in one worktree are detected from `git status` (engine 8.5) | `git status` says a file changed, not which instance changed it; attribution needs tool-call targets from events or transcripts. **Settled 2026-11-01:** the user decided collisions are attributed from tool-call targets (events and transcripts); an unattributable change is warned as "possible collision" without naming an instance; generated files are excluded (engine 8.5). I4 still tests the method | I4 |
| Q4 | Global keys `^H`, `^K`, `^F`, `^A`, `^T`, `^D` (mockups, main spec) | `^H` is Backspace on many terminals, `^D` is end-of-file, `^K` `^A` `^F` are line-editing keys the agents use | U4 |
| Q5 | The titler and seed pass through the sidecar like ordinary traffic | The sidecar can rewrite the human message (prompt rewriter, `promptRewrite.mode: auto`) and externalize large results (CCR); a seed or a bootstrap must not be altered | SC9 |
| Q7 | "Each instance's traffic goes through the proxy" (engine 9) | Whether the sidecar proxy is one per instance (per launch) or one shared by all instances of a window is not decided, and the memory and recovery design depends on it | SC5 |
| Q8 | `event`, `summary_log` and `message_event` grow without bound | No pruning policy is written | I8 |
| Q9 | A seed is a safe user message | Text from an old transcript can contain lines starting with `/` (slash commands), `!` (shell escape in Claude), `@path` (file inclusion) or `#`; pasted or passed as a prompt they may be interpreted by the destination | H7 |
| Q10 | Claude hooks "never fail the agent" because `shinatama hook` exits 0 | Claude documents exit-code semantics for hooks (a specific nonzero code blocks the action); the tested behavior on 2.1.310 is not recorded | M11 |
| Q11 | OpenCode SSE emits `session.status` (busy/idle) and `session.idle` is deprecated (engine 5.7) | OpenChamber's v2 translator, shipped with opencode 2.1.4 (MIT, `lib/event-stream/translate-v2.js`), states "v2 emits no `session.status` and no `session.idle` of its own" and synthesizes status from `session.execution.*`, `session.retry.scheduled` and `session.step.*` | OC5 |

## 2. Platform, terminal hosting, security of the terminal layer (P)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| P1 | G0 | A pseudo-terminal crate hosts each agent interactively on Windows 11 (ConPTY), macOS and Linux: start, type, resize, Ctrl-C, exit code, close without hang | Spike binary spawning each agent in a PTY and a vt parser; scripted keystrokes | Echo correct; resize reaches the agent; exit code propagated; closing never hangs; no orphan processes | main 5.1; main 3 |
| P2 | G0 | Emulator fidelity against the real agents | Record the raw byte stream of a scripted session per agent through the PTY; replay into candidate emulators; diff the final grid cell by cell against a reference terminal's grid of the same bytes | Text and attributes identical except a written list of exceptions | main 5.1 |
| P3 | G0 | Wide characters, emoji, CJK, combining marks, zero-width joiners, box drawing render with correct widths | Fixture strings through P2 | No column drift on any row in any agent's screen | main 5.1 |
| P4 | G0 | Alternate screen enter and exit, scroll regions, insert/delete line, scrollback in the pane | Replay corpus from each agent plus a terminal conformance suite | Screen restored exactly on exit; user can scroll the pane's history | main 5.1 |
| P5 | G1 | Mouse reporting, wheel, selection and copy in the pane; bracketed paste both directions | Manual script plus recorded input events per OS | Mouse reaches agents that request it; selection works; paste not mangled | main 5.1 |
| P6 | G0 | Advanced keys (kitty keyboard protocol, modifyOtherKeys, Shift+Enter, Alt combinations, Italian keyboard AltGr) pass from the native window through Shinatama to the agent | Key matrix per OS and per agent | Every key the agent documents arrives unchanged | main 5.1; U4 |
| P7 | G1 | No flicker; CPU and latency with 4 instances streaming | Synthetic streaming load; measure frame time, CPU %, keystroke-to-echo latency | Targets written from the measurement (my starting target: echo under 20 ms at the 95th percentile) | main 3 |
| P8 | G1 | Images (kitty graphics, sixel, iTerm2 inline) from an agent are drawn in its pane in the native window, or are cleanly dropped | Which agents emit images; test on 3 OS | Documented per agent and OS; no corrupted screen | main 5.1; main 5.13 icons |
| P9 | G0 | Escape-sequence policy: which sequences Shinatama's emulator acts on and which it swallows (OSC 0/2 title, OSC 7 cwd, OSC 8 links, OSC 9/777 notifications, OSC 52 clipboard, DECRQSS and other query answerbacks, window-manipulation CSI t) | Corpus of known terminal escape attacks plus each agent's real stream | A hosted agent cannot write the clipboard, read the screen, retitle the window or inject input without a decision; each decision is documented | main 5.1; SEC |
| P10 | G1 | Truecolor passthrough; 16-colour remap onto each skin keeps agent output legible | Contrast measured for every ANSI colour on every skin background; screenshots | Every remapped colour meets 4.5:1 against its background or is exempted in writing | main 5.7; U1 |
| P11 | G1 | Smallest usable pane width per agent; behavior when a side panel narrows the agent | Resize each agent from 120 down to 40 columns | Each agent's minimum documented; side panels collapse before the agent breaks | main 5.1 |
| P12 | G0 | Overlay compositing: a window drawn over the pane then removed restores the agent screen, cursor, SGR state, wide characters and alternate screen | Automated screen-grid comparison before and after, mid-stream output included | Grids identical; no lost output while the overlay is open (output keeps being parsed) | main 5.1 |
| P13 | G1 | Native-window matrix: Windows, macOS and Linux; launched from the desktop and from PowerShell, cmd, Git Bash, zsh, bash | Run P1-P12 subsets in each | Supported list and known problems written | README; main 3 |
| P14 | G0 | Process lifecycle: closing an instance kills its whole tree (Windows job object, Unix process group); a Shinatama crash leaves no orphan agents or MCP servers | Kill -9 and window-close tests | No surviving process after 10 s, or the survivors are documented | engine 3 |
| P15 | G1 | Environment given to the child: `SHINATAMA_*`, `TERM`, `COLORTERM`; child-only | Env dump from inside each agent's shell tool | Values as specified; nothing leaks to the parent or siblings | engine 3 |
| P16 | G0 | Spawning without a shell on Windows with paths containing spaces, parentheses, ampersands, non-ASCII, over 260 characters; `.cmd` shims; PATHEXT; Pi as `node.exe cli.js` | Spawn matrix per agent, including the known cases (EINVAL on `.cmd`, PATHEXT) | Every case starts the right program with the right arguments | engine 5.6; engine 6.8 |
| P17 | G0 | The Rust crates and npm packages in use exist, are maintained, license-compatible and work on Windows (PTY, xterm.js terminal emulation, Tauri, SQLite) | Read crates' metadata and repositories; build the spike | A shortlist with versions and licences; "mature crates exist" restated with evidence | main 3 |
| P18 | G2 | Startup time, idle memory, per-instance memory, binary size | Measure on 3 OS | Numbers written; targets set from them | main 3 |
| P19 | G1 | Terminal capability fallbacks: 16-colour only, no Unicode box drawing, no Nerd Font, `NO_COLOR`, screen readers | Force each fallback in the native window | Usable and legible; keyboard-only operation complete | main 5.7 |
| P20 | G1 | Inherited agent markers: the exact variables each agent (Claude Code, Codex, OpenCode, Pi; Hermes Agent and DeepSeek Harness in HM11 and DS11) sets for its children; what a hosted agent does differently when it inherits them (Claude: `CLAUDECODE`, `CLAUDE_CODE_CHILD_SESSION`, `CLAUDE_CODE_MESSAGING_SOCKET`); that removing exactly those names gives fresh-terminal behaviour and removes none of the user's own settings | Env dump (names only) from each agent's shell tool in a scratch home; start each agent from inside another agent with and without the removal; compare startup, session listing, hooks, messaging socket use | Exact list per agent written into its adapter; with the removal, behaviour equals a fresh terminal; no user setting removed | engine 3; engine 5.5 A |

## 3. Shared state: SQLite (S)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| S1 | G0 | Bundled SQLite with FTS5 and WAL works with several processes on Windows, macOS, Linux | 4 writers and 4 readers for 10 minutes; kill tests | No corruption; busy errors within the stated budget | engine 4.1 |
| S2 | G0 | Refuse a network or synchronized location (Google Drive `G:` `H:`, OneDrive, SMB, NFS, Dropbox) without false alarms on local disks | Detection per OS tested on those volumes, using a throwaway folder only | Refused on every synced or network volume; accepted on local; the message is useful | engine 4.1 |
| S3 | G1 | `PRAGMA data_version` polling at 200 ms: latency and CPU; works across processes | Two processes, measure | Latency and CPU recorded; the interval chosen from data | engine 4.6 |
| S4 | G0 | Crash safety: kill -9 mid-transaction, in the middle of a hop's state change | Fault injection | `integrity_check` ok; hop state recoverable | engine 4; engine 11.1 |
| S5 | G1 | Migrations: forward migration, backup before, refusal of a newer database (opened read-only) | Versioned fixture databases | Each path behaves as written | engine 4.1 |
| S6 | G1 | Index size and speed on a realistic corpus (counts only: Claude project folders, Codex rollouts, OpenCode 232 MB database) | Index synthetic data at the real corpus's size | Indexing time, database growth and search latency recorded; targets set (starting target: search 150 ms at the 95th percentile) | engine 4.2 |
| S7 | G1 | Incremental indexing: detect appended JSONL, rotated or deleted files (Claude retention), OpenCode changes | mtime plus size plus content hash; watch cost | No full re-read of unchanged files; deleted sessions keep their text and are marked | engine 4.2; engine 5.7 |
| S8 | G1 | Liveness: pid plus start time per OS; thresholds 5 s and 20 s; laptop sleep and resume | Kill, suspend, resume tests | No false "dead" after sleep; dead found within the stated time | engine 4.4 |
| S9 | G0 | Path canonicalization: `\\?\` prefix, junctions, drive-letter case, 8.3 names, symlinks, Google Drive paths; one project recognized across the different path encodings of every hosted agent (DeepSeek Harness adds its `--<normalized cwd>--` folders) | Path corpus including the real shapes seen: Claude flattened names, Codex `\\?\`, Pi `--…--` | The same folder always maps to the same project id | engine 4.5 |
| S10 | G0 | The unique indexes enforce "one live segment per thread" under concurrent hops | Acceptance 11.4 | Exactly one wins | engine 4.3 |
| S11 | G1 | File permissions: mode 0600, Windows ACL for the current user only | Inspect after creation on 3 OS | Other users cannot read | engine 4.1 |

## 4. Claude Code adapter (CL), version 2.1.310 (installed 2026-10-27: 2.1.311 — rows to re-run first, [E1 record](verification/2026-10-27-E1-agent-cli-recheck.md))

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| CL1 | G0 | `claude --session-id <uuid> "<bootstrap>"` starts interactive, uses that id, writes `<id>.jsonl` at the expected path; the prompt is a user turn | Scratch config dir, one login | File name and first user entry as predicted | engine 5.7 |
| CL2 | G0 | Seed by argument: 2 KB, 30 KB, 100 KB; special characters, newlines, leading `-`, non-ASCII; the Windows 32,767 limit | Spawn matrix, `--` separator | Delivered intact up to a stated size; the failing size and error recorded | engine 5.6 |
| CL3 | G1 | `--append-system-prompt-file` as a large-text channel (it is system prompt, not a user turn) | Use and inspect the transcript | Behaviour recorded; adopted or rejected | engine 5.6 |
| CL4 | G0 | `--resume <id>` in the same cwd; in a different cwd; `--resume <absolute path>`; `--fork-session` | Run each | Documented outcome per case; the cwd rule confirmed | engine 5.7 |
| CL5 | G1 | The CLI accepts a hand-built transcript (minimal valid) in a scratch config dir and resumes it | Build from fixtures; try | Accepted or not; the minimum fields written | engine 5.4 |
| CL6 | G0 | Reader against real records: block names (`text`, `thinking`, `tool_use`, `tool_result`), `isMeta`, `isSidechain`, leaf selection by `last-prompt.leafUuid`, compaction, attachments, hook-output records | Synthetic session with each feature; parse | Every item maps to the canonical form; unknown types counted | engine 5.3 |
| CL7 | G1 | Retention: `cleanupPeriodDays` default 30, when deletion happens, what is deleted | Scratch dir, shortened setting, aged files | Behaviour recorded; the claim "Claude deletes after 30 days" confirmed | engine 5.7 |
| CL8 | G0 | Hooks via `--settings`: merge with the user's hooks; `Notification` types, `Stop`, `SessionStart` (resume, fork) fire; command latency; quoting of the command in JSON on Windows | Launch with a hook that logs; scripted turns | Events arrive with the documented fields; user hooks still run | engine 5.7; engine 8.1 |
| CL9 | G0 | Per-launch MCP: `--mcp-config` adds `shinatama mcp` next to the user's servers; the server child sees `SHINATAMA_INSTANCE`; survives `/resume`, `/clear`, compaction | Launch, run each, call the tool | Tool works; identity stable (the bili `/resume` bug does not recur) | engine 7.5; engine 3 |
| CL10 | G1 | Pre-approval of the four tools (`--allowedTools` or settings) | Call them without a prompt | No permission prompt for the four; none for anything else | engine 7.5 |
| CL11 | G1 | Headless answer-only: `claude -p` with tools disabled (`--allowedTools`/`--disallowedTools`, `--permission-mode`), `--no-session-persistence`; stream-json ends with `result`; cost field | Ask it to run a command and write a canary file | No file written, no command run; result and cost parsed | engine 7.6; R2 |
| CL12 | G0 | First-run folder-trust prompt in an unfamiliar directory: what shows, how long, which state a hook reports | Fresh directory, scratch dir | Surfaced to the user in time; never stuck hidden | engine 6.7 |
| CL13 | G1 | `claude auth status` reports sign-in without exposing credentials; its output format; `auth login` runs in a PTY | Run both, signed out and in | Parseable; no secret in output | engine 5.5; D5 |
| CL14 | G1 | `AGENTS.md` versus `CLAUDE.md` reading rule (2.1.277 and later) | Projects with each combination | Matches the documented rule; the seed header states it correctly | engine 5.7 |
| CL15 | G1 | Resuming a session already open in another process: interleaving, locks | Two processes | Outcome documented; Shinatama's prevention (4.3) is sufficient or extended | engine 4.3 |
| CL16 | G2 | Large transcripts (over 64 MiB): backward streaming read time and memory | Synthetic 100 MiB file | Within a stated budget | engine 6.8 |
| CL17 | G1 | `claude import` really imports configuration only | `--dry-run` in a scratch dir | Confirmed or corrected | engine 5.4 |

## 5. Codex adapter (CX), version 0.164.0

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| CX1 | G0 | `codex -C <cwd> "<bootstrap>"` starts interactive; the new thread id is learned reliably (new rollout file, `session_meta.id`, notify) with two instances starting at once | Scratch `CODEX_HOME`; start several | Correct id for each instance, no mix-up | engine 5.7; engine 6.3 |
| CX2 | G0 | `codex resume <id>` for a native session, an archived session, an imported one; `--last` never used | Run each | Documented outcome | engine 5.7 |
| CX3 | G0 | `thread/inject_items` persists to the rollout and survives `codex resume` | App server in a scratch home: inject, stop, resume, read | Items visible after resume, in the rollout file | engine 5.4 |
| CX4 | G0 | `externalAgentConfig/detect` and `/import`: request and response shape (from `codex app-server generate-json-schema`); import of one named synthetic Claude session; resumable; tool items kept; the 30-day window; a session still being appended | Scratch home, synthetic fixtures | Each criterion recorded; Claude → Codex strategy chosen from the result | engine 5.4; engine 6.2 |
| CX5 | G1 | `codex queue --thread` reaches a running interactive instance, and when | Queue while idle and while working | Delivery timing recorded; honest wording for the UI | engine 7.3 |
| CX6 | G1 | Backend attach: `codex app-server` plus `codex --remote ws://` or `unix://` (also `--remote-auth-token-env`): TUI behaves like stand-alone; `unix://` on Windows; the labelled "experimental and unsupported" `ws://`; the local app-server daemon (`codex app-server daemon`) already on this machine | Run both modes side by side | Behaviour parity or a list of differences; decision attach versus stand-alone | engine 5.7 item 4 |
| CX7 | G0 | TUI notifications emitted into the PTY: `[tui] notifications`, `osc9`/`bel`, `notification_condition = always`, set per launch with `-c`; which events fire; no false positives | Launch with the setting, scripted turns, read the byte stream | Reliable turn-end and approval signals, or the signal is dropped | engine 5.7; engine 8.1 |
| CX8 | G1 | Hooks: trust gate; whether `-c` can inject them; confirm Shinatama cannot rely on hooks without `--dangerously-bypass-hook-trust` (never used) | Try | Recorded; legacy `notify` evaluated as the alternative | engine 5.7 |
| CX9 | G1 | Headless answer-only: `codex exec --ephemeral --ignore-user-config --json` with a read-only sandbox and no approvals | Ask it to write a canary file | No file written; events parsed; end of turn clear | engine 7.6; R2 |
| CX10 | G0 | Reader against real rollout lines: `response_item` payload types (message, agent_message, reasoning, tool items), `compacted`, `ordinal` as cursor, turn boundaries; `history_mode: paginated` and `codex migrate-rollouts` do not break it | Synthetic session in both modes | Every item maps; paginated detected and handled or the adapter degrades | engine 5.3 |
| CX11 | G1 | `codex login status` output; `codex login` in a PTY | Signed out and in | Parseable; no secret in output | engine 5.5; D5 |
| CX12 | G1 | `CODEX_HOME` handling agrees with the sidecar's private overlay (sessions still land in the usual home) | Launch through the sidecar | Native chats, history and auth stay where Codex expects | engine 9 |
| CX13 | G1 | A session open in the Codex Desktop app and resumed from the terminal | Two processes | Outcome documented | engine 4.3 |
| CX14 | G1 | `AGENTS.md` reading rules and `project_doc_max_bytes` against a seed header; `CLAUDE.md` fallback | Test | The header's statement is true | engine 5.7 |

## 6. OpenCode adapter (OC), version 2.1.4

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| OC1 | G0 | `opencode <dir> --standalone --session ses_<id> --prompt "<bootstrap>"`: the chosen id is used (`^ses` format); `--standalone` avoids the shared background service and its inherited environment (the known proxy-poisoning problem) | Scratch DB and XDG; start; inspect | Id as chosen; environment is that of the launching process | engine 5.7 |
| OC2 | G0 | `session import <file> --directory D`: the exact interchange format, round trip with `session export`, tool items preserved or not, the 409 on an existing id | Export a synthetic session, import into a second scratch DB | Round-trip law of 5.5.1 holds or the losses are listed | engine 5.4; 5.5.1 |
| OC3 | G0 | `session export` as the primary reader: speed and size on large sessions; read-only SQLite fallback: legacy `session` and `message` tables versus `session_v2` and `session_message`; reading while OpenCode writes (WAL) | Synthetic and the real DB (counts only) | Reader correct on both generations; no lock trouble | engine 5.3 |
| OC4 | G0 | The HTTP API of a standalone instance: finding the address and auth, `--server URL`, `GET /api/event` (SSE), the 4,096-frame queue and the slow-consumer drop, reconnection | Connect, stress, disconnect | No missed state change after a reconnect; behaviour of a dropped consumer handled | engine 5.7 |
| OC5 | G0 | Signals: `session.status` busy and idle, `permission.asked`, `form.created`, `session.execution.*`; latency | Scripted turns | Mapped to the states of 8.1 | engine 8.1 |
| OC6 | G1 | Per-launch config: `OPENCODE_CONFIG`, `OPENCODE_CONFIG_CONTENT`, `OPENCODE_CONFIG_DIR` merge with the user's config; MCP and plugin injection | Inject, list tools | `shinatama mcp` present; the user's config intact | engine 7.5 |
| OC7 | G1 | Headless answer-only: `opencode run --format json` with restricted permissions | Canary file | No write; events parsed | engine 7.6; R2 |
| OC8 | G1 | Sign-in detection via `opencode auth` subcommands (the mockup's "no provider configured") | Signed out and in | Parseable; no secret in output | engine 5.5; D5 |
| OC9 | G1 | `AGENTS.md` only; seed header statement | Test | True | engine 5.7 |
| OC10 | G1 | Which of `session` and `session_v2` is live on 2.1.4; migration versions; the effect of an upgrade | Read schema, compare to a newer version | Adapter handles both or is gated by version | engine 5.3 |
| OC11 | G2 | OpenCode's own `project` and `worktree` tables versus git worktrees, for the overview | Compare | Mapping documented | main 5.13 |

## 7. Pi adapter (PI), version 1.2.0

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| PI1 | G0 | Starting Pi by `node.exe …/cli.js` on Windows (finding the entry in the npm layout) and by `pi` on macOS and Linux; `pi @file "<bootstrap>"` delivers the file as the first user message; size limits; images | Scratch `PI_CODING_AGENT_DIR` | Whole seed arrives; limits recorded | engine 5.6; 5.7 |
| PI2 | G0 | `--session-id <id>` creates a session with that id; the resulting path; combining with `@file`; `--session-dir` | Run | File at the predicted path | engine 5.7 |
| PI3 | G0 | Reader: tree JSONL, leaf selection, `compaction` entries, `custom_message`, tool-call content shape, `usage` entries | Synthetic session | Every item maps | engine 5.3 |
| PI4 | G0 | A per-launch extension (`-e`) can report `agent_settled`, `ui_prompt_start` to `shinatama hook`; startup cost; coexistence with the user's extensions | Write a minimal extension, run | Events arrive; startup cost recorded | engine 5.7; engine 8.1 |
| PI5 | G1 | Whether the Pi TUI can attach to a backend at all (spec Q2); otherwise delete Pi from the attach list | Read the docs and try | Spec corrected | engine 5.7 item 4 |
| PI6 | G1 | No permission prompts: states `waiting_permission` never occur; `ui_prompt` kinds map to `waiting_question` | Scripted prompts | Mapping confirmed | engine 8.1 |
| PI7 | G1 | MCP per launch through `pi-mcp-adapter`: which file is read (`~/.config/mcp/mcp.json` versus `~/.pi/agent/mcp.json`, per the earlier memory), whether a per-launch override exists | Try | `shinatama mcp` reachable without editing the user's config, or Pi is marked "no MCP" | engine 7.5 |
| PI8 | G1 | Headless answer-only: `--mode json` with tools disabled | Canary file | No write | engine 7.6; R2 |
| PI9 | G1 | Sign-in detection without reading `auth.json`; login flow in a PTY | Try | Parseable status or marked "unknown" | engine 5.5; D5 |
| PI10 | G2 | The correct package name and update path (`@earendil-works/pi-coding-agent` installed; docs earlier named `@mariozechner/…`) | `npm ls -g`, registry | Checkup uses the right one | engine 5.7 |
| PI11 | G0 | **Observed:** `pi --help` printed nothing within 25 s on this machine today. The earlier memory note says Pi is unresponsive for about 60 s after launch because of a synchronous WMI query in `pi-subagents`. This would break a 90 s hop receipt and `fixture_gate` | Time `pi --version` and a bare launch with and without `pi-subagents` | Start time recorded; the hop timeouts and `doctor` timeouts set above it, or a workaround is used | engine 6.3; engine 6.7 |

## 7a. Hermes Agent adapter (HM), version 0.22.5 (added 2026-10-31; not installed on this machine)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| HM1 | G0 | Start and seed: install in a scratch `HERMES_HOME`; on Windows, whether `%LOCALAPPDATA%\hermes\bin\hermes` is a native executable or a shim with a shorter command line; `hermes chat --query-file <seed>` on a PTY delivers the whole file as the first turn and stays open; size limit; the id learned from the `session_id:` line before the first answer | Scratch home, run | Whole seed arrives; id learned; limits recorded | engine 5.6; 5.7 |
| HM2 | G0 | Gateway lifecycle: one gateway per box, started and stopped by Shinatama; closing the PTY only detaches, so `release()` (Ctrl+C, then the gateway with the box's last instance) leaves no running turn and no process; two boxes never share a gateway; restart after a crash; whether Shinatama can read state from the gateway while the classic CLI renders | Start, detach, close, kill | Nothing of Hermes runs after Shinatama's window closes; no gateway shared across homes | engine 5.7; main 2 (anti-goals) |
| HM3 | G0 | Reader (Mukade): `state.db` schema 32 read-only while the gateway writes (WAL); `sessions export --format jsonl` against the SQLite rows; in-place compaction (`active`, `compacted`), sub-agent rows (`_delegate_from`), tool rows, profiles | Synthetic sessions in a scratch home | Every item maps (engine 5.1) or the loss is listed; no lock trouble | engine 5.3; mukade-handover 9 |
| HM4 | G0 | Signals: shell hooks in the box's `config.yaml` reach `shinatama hook` with `session_id`; the consent step (`--accept-hooks`, `HERMES_ACCEPT_HOOKS=1`, `shell-hooks-allowlist.json`) stays inside the box; the mapping to the states of 8.1, approvals included | Scripted turns | States mapped; nothing written outside the box | engine 8.1 |
| HM5 | G1 | MCP: `shinatama mcp` in the box's `mcp_servers:`; how each instance is told apart when the entry is per box (`SHINATAMA_INSTANCE` through the environment); ACP `session/new` with `mcpServers` as the alternative | Inject, list tools | Reachable and per instance, or Hermes marked "no MCP" | engine 7.5 |
| HM6 | G1 | Resume: `--resume <id>` in the session's own directory; the effect of `--no-restore-cwd`; one session resumed in two terminals | Run | Resumed; never twice at once | engine 5.7 |
| HM7 | G1 | Headless answer-only: `chat --oneshot -q --format stream-json --ignore-user-config --ignore-rules` with a toolset that removes tools and writes | Canary file | No write; `result` parsed | engine 7.6; R2 |
| HM8 | G1 | Context files: only the first project file type found is loaded; the seed header names every instruction file | Projects with each combination | Seed statement true | engine 5.7 |
| HM9 | G0 | Box: `HERMES_HOME` alone keeps sessions, settings, skills, hooks allowlist, login and gateway inside the box; `TEMP`/`TMP`; `~/.agents/skills` excluded; owner-only permissions | Scratch box, file watch | Nothing written outside the box, or each leak recorded | main 5.14 |
| HM10 | G1 | `own` program: Hermes's installer into Shinatama's folder (Python and a bundled uv), side by side, self-update blocked, rollback | Install two versions | Recipe written, or `own` refused for Hermes | main 5.14 |
| HM11 | G1 | Inherited markers: the variables Hermes sets for its children (`HERMES_SESSION_ID` and others) and what a hosted Hermes does when it inherits them | As P20 | List added to the union | engine 3 |
| HM12 | G0 | **SEC** Sidecar: Hermes's provider traffic through the BillionContext proxy for each provider the user picks; child-only trust of the proxy's certificate authority in Hermes's Python TLS stack; credentials untouched | Per provider used | Traffic through the proxy, or that provider marked uncovered | main 5.6 |
| HM13 | G1 | Per-instance set without splitting the sessions (a profile brings its own store) | Try | A way found, or Hermes keeps the box's set | main 5.14 |
| HM14 | G0 | **SEC** A seed through `--query-file` containing `@file:`, `@folder:`, `@diff`, `@git:N` or `@url:` is not expanded, fetched or read (H7) | Harmless marker files and a local URL | Nothing expanded or fetched | engine 6.4; H7 |

## 7b. DeepSeek Harness adapter (DS), version 0.2.1-rc.5 (added 2026-10-31; not installed on this machine)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| DS1 | G0 | Install and gate: `npm --prefix` install of the exact version in a scratch `DSH_HOME`; `dsh --profile acp` starts on Windows, macOS and Linux with Node `^22.19.0 \|\| >=24`; `fixture_gate` pins the exact version (developer preview, breaking changes announced) | Install, start | Starts on each OS, or each gap recorded | engine 5.7 |
| DS2 | G0 | ACP pane: `session/new`, `session/prompt` with a realistic seed, streamed updates drawn, cancel, `session/resume`, `session/list`; the id known before the first prompt and equal to the store's | Scripted ACP client | Whole seed arrives; resume works; ids match | engine 5.6; 5.7; main 5.16 |
| DS3 | G0 | Reader (Mukade): Zstandard-framed `session.v4.jsonl.zstd`, a torn last frame, header lineage (`parentSession`, `isSeeded`, `origin`), compaction and sub-agent records; reading while `session.lock` is held | Synthetic sessions | Every item maps or the loss is listed; no lock taken | engine 5.3; mukade-handover 9 |
| DS4 | G0 | Permissions: the `session/request_permission` round trip in the pane; the default `workspace-write` mode; states `waiting_permission` and turn end | Scripted tool calls | States mapped; no tool runs without the user's answer | engine 8.1 |
| DS5 | G1 | **SEC** MCP: `shinatama mcp` per launch through `--patch` and per session through ACP; with the scrubbed environment (`KEY\|PASSWORD\|SECRET\|TOKEN` and `DSH_*` dropped), how the instance identity and `SHINATAMA_INSTANCE_TOKEN` reach the server | Inject, list tools | Reachable and per instance; no secret travels in a variable the filter drops | engine 7.5; M15 |
| DS6 | G1 | Hop: the seed recognized when read back (H11); `--session-id` refusing another directory, forks and sub-agent sessions | Run | Matches 6.3 | engine 6 |
| DS7 | G1 | Headless answer-only: `dsh headless --json` with `DSH_PERMISSION_MODE=read-only` | Canary file | No write; `final` parsed | engine 7.6; R2 |
| DS8 | G1 | Context files (`AGENTS.md`, `CLAUDE.md`, the `.local` variants, `DSH_HOME/AGENTS.md`, `~/.agents/AGENTS.md`) and the 64 KiB cap; seed header statement | Test | True | engine 5.7 |
| DS9 | G0 | Box and login: `DSH_HOME` plus `DSH_AGENTS_HOME` keep everything in the box; login through `dsh web` on the home the agent uses (the box's, or the user's with `system`; user, 2026-11-01); a check of the configured provider that reads no secret; the effect of a key in the project's `.env` | Scratch box | Nothing written outside the box; sign-in detectable or "unknown" | main 5.11; 5.14 |
| DS10 | G1 | Update path of an `own` copy: npm `latest` against `alpha`; any self-update; side by side and rollback | Observe | Recipe written | main 5.14 |
| DS11 | G1 | Inherited markers: the variables DeepSeek Harness sets for its children and the effect of inheriting them | As P20 | List added to the union | engine 3 |
| DS12 | G0 | **SEC** Sidecar: DeepSeek API and pi-ai provider traffic through the proxy; child-only trust of the proxy's certificate authority in Node (`NODE_EXTRA_CA_CERTS` or equivalent) | Per provider used | Traffic through the proxy, or that provider marked uncovered | main 5.6 |
| DS13 | G1 | Per-instance set through `--patch` and the ACP session | Two instances on one box | Each has exactly its set; sessions shared | main 5.14 |
| DS14 | G1 | `dsh web` on request: a loopback port per box, its browser-trust check, opened in the user's browser, stopped when the window closes, never embedded | Open, close | No process left after close | main 5.15; 5.16 |
| DS15 | G0 | **SEC** A seed sent through ACP `session/prompt` starting with `/` (for example `/compact`) or carrying file references is not executed or expanded (H7) | Harmless markers | Nothing executed or expanded | engine 6.4; H7 |

## 8. The hop (H)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| H1 | G0 | Canary hop for every ordered pair among the six hosted agents at stage 1 (a pair whose source is Hermes Agent or DeepSeek Harness counts once Mukade reads that source), small thread and a 400-turn thread, 10 runs each | Acceptance 11.2 with live models, tiny cost | 10 of 10 on the small thread; the 400-turn rate recorded and a floor set | engine 11.2 |
| H2 | G0 | The receipt: each destination replies `ready <nonce>` plus one line as told; failure rate; its token cost; how fast it can be read back from the destination's store | Instrument | Rate and cost recorded; the design keeps or drops the receipt | engine 6.3 |
| H3 | G1 | Budget: the bytes-per-token estimate (3.5) against each provider's reported usage; where each agent's window comes from (never a name in the code) | Compare on real prompts | Error range recorded; the constant adjusted | engine 6.4 |
| H4 | G0 | The full-transcript file is readable by the destination: Claude outside-cwd read permission, Codex sandbox roots, OpenCode external-directory permission, Pi none; whether each reads it when told | Per agent | Location chosen per agent; fallback works | engine 6.4 |
| H5 | G0 | First-run and trust prompts in an unfamiliar directory for each destination (Claude trust, Codex trust, OpenCode, Pi `trust.json`) | Fresh directories | Surfaced to the user quickly; never hidden | engine 6.7 |
| H6 | G1 | Warm standby: cold start to input-ready per agent; idle memory; zero token cost while idle; seed delivery after minutes idle; the targets of 8 s and 25 s | Time and measure | Targets confirmed or revised | engine 6.1; 6.5 |
| H7 | G0 | **SEC** A seed or bootstrap containing a line starting with `/`, `!`, `@path`, `#`, or `-` is not executed or expanded by any destination | Harmless marker commands and files | Nothing runs or expands; otherwise the seed is escaped or sent by a safe channel | engine 6.4 |
| H8 | G1 | Bracketed-paste channel per agent: large paste collapsing, Enter submission, input-ready detection | Try | Documented per agent | engine 5.6 |
| H9 | G0 | Idle detection before switching: turn-end signal latency per agent; interrupting a running turn (keys and `turn/interrupt`) and what is lost | Scripted | Behaviour documented; the "wait or interrupt" dialog tells the truth | engine 6.3 |
| H10 | G0 | Hop back: the earlier session resumes natively and absorbs the catch-up message; a fact from the other segment is known afterward | Canary variant | Pass; cost recorded | engine 6.6; engine 11.3 |
| H11 | G0 | Hand-off marker: the seed is recognized when read back from each agent's store (no nesting across A to B to C) | Read back | Recognized in every agent; otherwise a different marker | engine 6.4 |
| H12 | G0 | Evidence of a spawn per agent (id known in advance for Claude, OpenCode, Pi, and DeepSeek Harness at `session/new`; learned for Codex and Hermes); uncertain-spawn recovery never relaunches | Fault injection | Matches 6.3 | engine 6.3; 11.1 |
| H13 | G1 | Undo: resuming the source natively while the destination still exists | Try | No duplicate live session; documented | engine 6.3 |
| H14 | G1 | Redaction: recall on a synthetic secret corpus (AWS, GitHub, OpenAI, JWT, private keys, `.env` lines, URLs with credentials); false positives on code, hashes, UUIDs; speed on 10 MB | Corpus | Recall and false-positive rates recorded; the rules tuned | engine 6.4; engine 11.6 |
| H15 | G1 | Leaf and branch selection against what each agent itself resumes (rewinds and forks) | Synthetic branched sessions | Same path as the agent's | engine 5.2 |
| H16 | G1 | Tool-call digests: names and targets extracted for each agent's tool vocabulary | Fixtures | Accurate on the fixtures | engine 5.1; 6.4 |
| H17 | G1 | Attachments: where each agent keeps pasted images and files; paths still exist after the hop; destinations can read images by path; size limits | Try | Documented per agent | engine 6.4 |
| H18 | G1 | "Instruction files linked" (CLAUDE.md and AGENTS.md): what exactly is done, with no file created in the user's repository without consent | Decide and test | Behaviour written and tested | main 5.2 |
| H19 | G2 | Git state collection cost on very large repos, submodules, worktrees; timeouts | Large repo | Within a stated budget | engine 6.4 |
| H20 | G1 | Hopping between worktrees of one project (the cwd changes; Claude keys sessions by cwd) | Try | Rules written | engine 6.2 |
| H21 | G1 | Elapsed time from Enter to READY, per pair, 95th percentile | Instrument H1 | Targets in 6.1 confirmed or revised | engine 6.1 |
| H22 | G1 | The measured cost in tokens and money of a hop (seed, receipt), per pair | Instrument | Shown in the dialog only if accurate | engine 6.4 |
| H23 | G1 | Windows: non-ASCII and long cwd in the handoff file path and in argv (UTF-16 conversion) | Try | Correct | engine 5.6 |
| H24 | G2 | Journal and crash recovery at every state (fault injection) | Acceptance 11.1 | Pass | engine 11.1 |
| H25 | G2 | Concurrency of two hops of one thread | Acceptance 11.4 | Pass | engine 11.4 |
| H26 | G1 | Fork hop: the destination is a new thread with a new native session id; the source instance, thread, activation and native session stay live and unmodified; no `native_resume` of a live session; the default setting and the per-hop override both work | Canary fork for same-agent and cross-agent pairs; compare the source session file before/after | Both threads usable; the source file unchanged; close stays the default | engine 6.2; 6.3; main 5.2 |
| H27 | G0 | **Session conversion between the four agents with Shinatama's own library** (originally: txcript as the converter; not adopted, user decision 2026-10-27): for every ordered pair of Claude Code, Codex, OpenCode 2.1.4 and Pi, convert a synthetic session (text, tool call with result, two parallel tool calls, thinking, a compaction, a rewind, an interrupted last turn) and read it back natively; list what is lost or changed per pair | Build Mukade (`cargo build --release` in its repository; `cargo test` runs its conformance suite), convert in scratch dirs, import with the agent's own path (OpenCode `session import`), `resume` where the agent allows; compare with the source | Per pair: a table of kept/changed/lost; parallel results and compaction behave as stated in engine 5.4 or the gap is written; parallel-call and compaction cases are in the pin since the 2026-10-27 merge, so they are tested, not assumed | engine 5.4 |

## 9. Messages, MCP, hooks (M)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| M1 | G0 | Per-launch MCP injection for every agent without editing global configuration (Hermes: the box's config only, HM5; DeepSeek Harness: `--patch` or ACP, DS5); coexists with the user's servers; start latency; the server sees `SHINATAMA_INSTANCE` | CL9, CX, OC6, PI7, HM5, DS5 together | Works on each, or the agent is marked "no MCP" | engine 7.5 |
| M2 | G1 | Pre-approval of the four tools per agent | CL10 and equivalents | No prompts | engine 7.5 |
| M3 | G1 | The token cost of the four tool definitions per agent | Capture the first request through the sidecar | Recorded | engine 7.5a; T1 |
| M4 | G0 | What reaches an idle agent: Claude hook output, `codex queue`, Pi `queued`/`steer`, OpenCode `/prompt` and `/synthetic`; honest statement of what "delivered" means | Per agent | UI wording matches the behaviour | engine 7.3 |
| M5 | G0 | **SEC** PTY write safety: text is never written while a permission dialog, a selection, a search box or a draft is active; a write can never answer a permission prompt | Provoke each situation | Never; otherwise the channel is removed | engine 7.3 |
| M6 | G0 | **SEC** Injection resistance: a peer message that says "create file X" or "ignore previous instructions" is not obeyed by an agent with tools; measured rate per agent | Canary messages | Rate documented honestly; tools stay denied for unattended delivery if not zero | engine 7.6 |
| M7 | G1 | Loop control: ping-pong stops at depth 6 with an alert even if agents do not carry `in_reply_to` | Acceptance 11.7 | Enforced by the server | engine 7.4 |
| M8 | G1 | `shinatama mcp` startup time, stdio framing on Windows, no stdout noise, exits when the parent dies | Measure | Within 200 ms (my target); no orphans | engine 7.5 |
| M9 | G1 | Identity survives `/resume`, `/clear`, compaction (CL9) | Per agent | Stable | engine 3 |
| M10 | G1 | At-least-once, dedupe and expiry behave as written | Fault injection | Matches 7.3 | engine 7.3 |
| M11 | G0 | Hook command can never block or fail an agent: Claude's exit-code semantics, timeouts, a locked database, a crash | Simulate each | The agent continues in all cases | engine 8.2 |
| M12 | G2 | Claude `http` hook handler versus the command handler | Compare | One chosen | engine 8.2 |
| M13 | G1 | Cross-project sends are refused | Acceptance 11.7 | Refused | engine 7.2 |
| M14 | G1 | Latency from send to delivery across two windows; the owning process dies with messages queued | Measure | Queued messages shown, delivered when an owner returns | engine 7.3 |
| M15 | G0 | **SEC** Instance identity: `shinatama mcp` and `shinatama hook` refuse a caller whose `SHINATAMA_INSTANCE_TOKEN` does not match the instance's stored token hash; a hosted agent that sets a peer's `SHINATAMA_INSTANCE` cannot send, acknowledge or report state as that peer | Impersonation attempts from a hosted agent's shell tool, with and without the token | Refused and logged; the agent's own calls still work; the token never appears in the database in clear, in logs or in a transcript | engine 7.5; engine 8.2; engine 3 |

## 10. Instance state, overview, titles (I)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| I1 | G0 | State accuracy per agent against scripted sessions (idle, working, waiting_permission, waiting_question) and the false-positive rate of the PTY heuristic | Acceptance 11.9 | Precision and recall recorded; "unsure" labels used honestly | engine 8.1 |
| I2 | G1 | Tokens, cost and context fill per agent (Claude `cost-state`, Codex `token_count`, OpenCode session columns, Pi `usage`) and the window per model | Compare with provider usage | Accurate within a stated margin | main 5.13 |
| I3 | G1 | Branch and worktree: detached HEAD, linked worktrees, submodules, bare, non-git; cost of `git status` per idle on large repos | Matrix | Correct; cost bounded by caching | engine 8.5 |
| I4 | G0 | Collisions: attributing a changed file to an instance from tool-call targets, the method decided by the user 2026-11-01 (spec Q3); false positives from generated files | Use tool-call targets from events and transcripts, with two instances changing files in one worktree by tool calls and by shell commands | Method tested: attributed collisions name the right instances; an unattributable change gives "possible collision" naming none; generated files raise nothing | engine 8.5 |
| I5 | G0 | The title backends: the quality gate on about 50 real activity snapshots with a written rubric (names the right action, no invented fact, 12 words at most) | Rubric scoring of local and provider models | The default decided by the user (O2) confirmed by the result | engine 8.4 |
| I6 | G1 | The local small model: CPU, memory and latency with four agents running; the Rust binding or a bundled `llama-server` on 3 OS; model licence; download size and hash; the fftext README claims (Qwen 0.8B, 4,096 context, about 500 MB, Apache-2.0 for the tool) | Run, and read the model card and licence | Numbers recorded; licence compatible; claims confirmed | engine 8.4 |
| I7 | G1 | A provider low-cost model: how it is chosen from the catalog without names in code; cost accounting; the cap works | Run | Cap enforced; fallback to verbatim text works | engine 8.4 |
| I8 | G1 | Pruning of `event`, `summary_log`, `message_event` (spec Q8) | Decide and test growth | Policy written; size bounded | engine 4.2 |
| I9 | G1 | Redaction before a summary is sent; `summaries_excluded` really stops sending | Network capture | No excluded or secret text leaves | engine 8.4 |
| I10 | G1 | Icons: each agent's native icon in the tab, the overview and the project manager; text label/letter-badge fallback when an icon is missing; accessible names independent of colour | Matrix: 3 OS, display scaling, missing icon asset, screen reader | Legible everywhere; every agent identifiable without its icon or colour | main 5.13 |

## 11. Sidecar (SC)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| SC1 | G0 | Preparing the private root on Windows, macOS, Linux (Node version, locked build, no native dependencies, time) | Fresh machines | `0.1.207` printed; time recorded | main 5.6 |
| SC2 | G0 | **SEC** Environment applied to the child only | Inspect parent, siblings, shell | No leakage | main 5.6 |
| SC3 | G0 | **SEC** The certificate authority is trusted only by the launched process, per agent runtime (native binary, Rust, Bun, Node) and per OS; the system trust store is unchanged | Before and after store listing; each agent's TLS to the proxy | Works in each; store unchanged | main 5.6 |
| SC4 | G0 | **SEC** The launch manifest holds argv and overrides only, no credentials | Inspect on disk | None found | main 5.6 |
| SC5 | G0 | One proxy per instance versus one shared (spec Q7): memory, recovery, isolation; and the launch path (the shipped per-session launcher with its supervision off, or a direct launch with the proxy environment) that keeps Shinatama the only supervisor | Prototype both | Decision written | engine 9 |
| SC6 | G0 | Proxy recovery on the original address while the agent keeps running; orderly drain when an instance (not the window) exits | Kill the proxy mid-turn with 5 instances attached | Exactly one replacement starts; every agent recovers; no lost final flush; nothing printed in any agent's terminal | main 5.6; engine 9 |
| SC7 | G0 | Compression behaviour as shipped: the sidecar's own fixtures against Shinatama-launched sessions; the `--no-context` opt-out | Run the sidecar's fixtures | Results match the fixtures | main 5.6 |
| SC8 | G1 | The bili MCP shim follows `/resume` under Shinatama and coexists with `shinatama mcp` | Resume test | Correct session id | main 5.6 |
| SC9 | G0 | **SEC** The seed, bootstrap and receipt are not altered by the prompt rewriter or externalized by CCR; a first destination turn does not trigger a compression pass | Compare sent and received bytes | Identical; or a bypass marker is added in the single source (Musashi Terminal, `sidecar/source-build`) and inherited, never patched inside Shinatama (user, 2026-11-01) | engine 9 |
| SC10 | G1 | Reusing the sidecar's auxiliary-call path for titles and roundtable calls without handling credentials (the rewriter is the precedent) | Read and try | A path exists, or the titler uses another route | engine 8.4 |
| SC11 | G1 | Gemini launch-only keeps working through the sidecar | Launch | Starts and is compressed through the sidecar | main 5.9 |
| SC12 | G1 | The Windows proxy watcher (2 s) with Shinatama as the long-lived owner, including after the proxy's own event loop was blocked for longer than the watcher's interval. **Observed 2026-11-01** with the shipped 0.1.207 watchdog: after a ~36 s hold it dropped six live heartbeat owners on its first tick and shut the proxy down with 324 connections open (four times that day). The owned source credits the hold back from Musashi Terminal `ac7d57a`, unit-tested there in `test/watchdog-stall.test.mjs` | Test; block the proxy's loop for 60 s while every owner keeps its 5 s heartbeat | No premature or late shutdown; no owner is dropped | main 5.6 |
| SC13 | G1 | Providing Node on macOS and Linux (system, bundled, downloaded): offline, size, version | Try the options | One chosen | main 5.6 |
| SC14 | G0 | A busy proxy is not replaced: its event loop is held while its process is alive and holds its port, including once its accept queue is full and new connections are refused. **Observed 2026-11-01** on Windows (Node 24.14.1): a held listener refuses connections after 232 queued, the same with a backlog of 2048, while a bind on its port fails with `EADDRINUSE`; after its death the bind succeeds | With 5 instances attached, hold the proxy's loop for 60 s (debugger pause or a synthetic long compression) and open connections until they are refused; count the supervisor's probe connections | No replacement process starts; the supervisor opens no probe connection while the bind decides; no line appears in any agent's terminal; the window shows the proxy busy, then normal; the agents' next turns go through | main 5.6; engine 9 |
| SC15 | G0 | **Observed 2026-11-01:** a launcher build whose recovery messages had been moved to a log still printed `bili: watcher registration failed — ... (The operation was aborted due to timeout)` over Claude Code's screen, from the owner-renewal path. The bytes on an agent's terminal come only from the agent, whatever launch path SC5 picks | With 5 instances attached, capture each agent's PTY from launch to exit while injecting one fault at a time: hold the proxy's loop for 60 s; make the owner-renewal request time out (2 s) and then be refused; kill the proxy; start an older proxy build on the port (stale rotation); attach to an unarmed daemon proxy. Repeat after each sidecar upgrade | Each capture holds only bytes the agent wrote: no line starting `bili:` and no launcher or proxy text; each fault appears once in Shinatama's log; the window shows the matching proxy state | main 5.6; engine 9; ADR-0030 INV-006 |
| SC16 | G0 | Bounded local transport latency while saving history, and preservation of persisted data. **Observed 2026-11-02 in Musashi:** synchronous CCR replacement and six unindexed reference lookups caused a 13.57 s local CONNECT stall; schema v9 plus incremental CCR synchronization reduced it to 59–92 ms. This identifies one persistence cause, not every 15–71 s hold observed on 2026-11-01. [Evidence and procedure](notes/2026-11-02-sidecar-storage-latency.md); qualification under Shinatama remains open | Run the inherited storage-latency suite against the Shinatama-prepared source, including v8 migration, repeat migration, shared references and rollback. In its launch path, measure real local CONNECT plus echo and health during a 600-entry CCR replacement beside 200,000 unrelated messages. With 5 instances on long histories, record event-loop delay (`monitorEventLoopDelay`), local transport and refused agent connections for one hour; profile the longest holds. Repeat on each supported OS and sidecar upgrade | The controlled fixture keeps local CONNECT plus echo and health below 750 ms; all six reference checks use indexes, with no full-table scans; unchanged/reordered/renamed CCR content causes no blob mutations; counts and content survive migration, shared references and rollback. The one-hour run refuses no agent connection and records the longest hold and its cause for SC5. A timeout increase, proxy bypass or busy indicator is not a pass | main 5.6; engine 9; FRD-0009/R24 |

## 12. Daodan, checkup, login (D)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| D1 | G0 | The role of `acaprino/shinatama` after the 42.0.0 reunification (spec Q1); name collision with the product; install instructions | Read both repositories' READMEs and history | Statements in the specs and README corrected | main; README |
| D2 | G0 | Per-host install, update and "enabled" commands for the current Daodan marketplace, 1.0.0 since Daodan reset its versions on 2026-10-29 (Claude marketplace update, Codex, OpenCode V2 package path, Pi git package); non-interactive; Daodan's per-host build installed as is, nothing translated | Fresh profiles | Each host ends with the plugins enabled and no duplicates | main 5.5 |
| D3 | G1 | The user's own disabled plugins stay disabled | Test | Respected | main 5.5 |
| D4 | G1 | Enabling all Daodan plugins by default (48 at marketplace 1.0.0): tokens added to each agent's first request (skill listings, tool definitions), MCP servers that fail to start, launch time; a failing plugin must not block the launch | Capture the first request per agent through the sidecar, all versus `core` | Numbers per agent; failures reported by the checkup; decision on the default recorded | main 5.5; daodan-native 4.6 |
| D5 | G1 | Sign-in detection and login in a PTY for every agent: `claude auth status`/`login`, `codex login status`, `opencode auth`, Pi, `hermes auth list`, DeepSeek Harness (DS9); polling cost; cancel; no secret read | Per agent | Works, or marked "unknown" | main 5.11 |
| D7 | G1 | Checkup time per launch; when it runs when several instances start; updating an agent while an instance runs (locked executables on Windows) | Measure | Policy written; never update a running agent | main 3 |
| D8 | G1 | The Daodan palette lists workflows from the installed manifest, not a hard-coded list; the native entry per host | Compare to hosts doc | Correct on every host Daodan builds for; nothing offered on Hermes Agent and DeepSeek Harness (D20) | main 5.5 |
| D9 | G0 | Does `holder.session` in `.daodan/agenda.json` equal each agent's native session id (Claude, Codex, OpenCode, Pi)? How Daodan gets `--session ID` on each host | Run a Daodan task per host, compare the row with the agent's session store | Per agent: equal, derivable, or unrelated; the link is exact or by place accordingly | daodan-native 4.2 |
| D10 | G0 | The native "continue this task" and "show status" entries per host (`continue-task`, `show-status`): spelling, whether they accept a run id or branch, what they print | Hosts doc plus a run on each host | One typed command per host, or "no argument" recorded | daodan-native 4.5 |
| D11 | G1 | Reading `agenda.json` while a Daodan script rewrites it (`atomic_write`): torn or missing reads on Windows, macOS, Linux and Drive paths | Hammer the reader while the writer loops | No torn read, or the fallback (MCP or `run_state.py agenda`) chosen | daodan-native 7.1 |
| D12 | G1 | Resolving the main checkout from a linked worktree through the Git common directory on Windows paths; non-Git projects with the `.daodan-root` sentinel | Test with worktrees under `.daodan/worktrees/` and a plain folder | Same agenda found from every worktree | daodan-native 3 |
| D13 | G0 | The Daodan task handover: the preliminary seed never claims the source is closed; takeover is enabled only by a separate message after COMMITTED and confirmed source closure, with one user confirmation total; in fork mode the destination reads without writing and leaves the holder | Hop a task thread in each mode between two agents; inject closure refusal, crash before COMMITTED and an already active destination | Close: no early takeover, one confirmation total, holder changes only after confirmed closure. Failure/fork: holder unchanged | daodan-native 4.3 |
| D14 | G1 | Onboarding trigger: Shinatama's "no Daodan state" check (neither `.codebase-map` nor `.daodan/runs` as a directory in the project or the main checkout; a bare `.daodan/` does not count) gives the same answer as `run_state.py first-use` for a plain folder, a Git repo, a linked worktree and a project with only `.codebase-map`; the offer shows once and "Not now" sticks | Run both on a fixture set | Same result on every case; no repeated offer | daodan-native 4.7 |
| D15 | G0 | `box` home (main 5.14; recorded 2026-10-28 as "`private` mode"): each agent, started with only its home variable pointing at a private home (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `OPENCODE_DB` + `XDG_*`, `PI_CODING_AGENT_DIR`), writes nothing to the user's home; its login persists across restarts in that home; Daodan installs and enables there | Fresh private home per agent; file-system diff of the user's home before and after a full session (names and sizes only); restart and `auth status`; for Claude, a session with an MCP server and `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES` set, then look for `%LOCALAPPDATA%\claude-cli-nodejs\Cache\<cwd>\mcp-logs-*` (the leak D19 found) | Per agent: zero writes outside the private home; login persists; Daodan enabled; or the leak recorded: a known, harmless leak is shown to the user and the agent stays boxable (Codex's `~\.sbx-denybin`, main 5.14; Claude's MCP logs if the switch does not stop them, user 2026-11-01), and only a leak that touches credentials, sessions or the user's configuration marks the agent `system`-only (user, 2026-11-01) | main 5.14; main 5.11; engine 3 |
| D16 | G0 | `own` program (main 5.14): obtaining each agent into Shinatama's folder and keeping it there: source per agent (Claude native installer or npm, Codex release binary or npm, OpenCode binary, Pi npm package, which needs Node); turning off the agent's own self-update (Claude `DISABLE_AUTOUPDATER` and the others' equivalents, unverified); a second version installed side by side while an instance runs (locked executables on Windows); going back to the previous version | Per agent on 3 OS | One install and update recipe per agent; no self-update observed in a week; side-by-side and rollback work with an instance running | main 5.14; engine 5.5 A |
| D17 | G1 | `XDG_*` for a private OpenCode: what the programs it starts (git, package managers, other tools in its shell tool) read when `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and the others point at the private home; whether a narrower variable (`OPENCODE_CONFIG_DIR`, `OPENCODE_DB` alone) suffices | Run git and npm inside a private OpenCode's shell tool; compare `git config --show-origin` with a fresh terminal | The user's git and tool configuration still found, or the variables narrowed until it is | main 5.14; engine 3 |
| D18 | G1 | `system` program and `system` home (main 5.14): the user's binary and home are found on 3 OS (PATH, npm prefix, Homebrew, winget, Claude's native install folder); Shinatama never writes the binary; a version outside the proved range runs degraded with the reason shown; the user's sessions are listed read-only while the agent is in a box; switching mode moves nothing | Per agent; checksum of the user's binary before and after a checkup and an update notice | Found everywhere or "not found" with where it looked; binary unchanged; degraded state and switching behave as written | main 5.14; engine 5.5 A |
| D19 | G1 | Per-instance set (main 5.14): an instance launched with the switches of its agent (Claude `--setting-sources` + `--plugin-dir` + `--strict-mcp-config`; Codex `-c`/`--profile`/`--enable`/`--disable`; OpenCode `OPENCODE_CONFIG`/`OPENCODE_CONFIG_DIR`; Pi `--no-extensions --no-skills -e --skill`) sees exactly its plugins, skills and MCP servers and none of the home's others (user-level skills folders included), while the login and sessions of the home stay shared; two instances of one agent with different sets run at once | Per agent, in a box and in the system home: list the skills, plugins, tools and MCP servers in the first model request (mock) for an instance with set A, one with set B and one with none | Each instance's request contains exactly its set; no switch edits a file in the home; or the gap recorded per agent | main 5.14; engine 5.5 A |
| D20 | G1 | Daodan on Hermes Agent and DeepSeek Harness: with no Daodan host package, the checkup says so, installs nothing and the palette types nothing; when Daodan's marketplace lists a package for either host, it is installed like the other five | Checkup on both agents | Nothing copied from another host; the package used once it exists | main 5.5 |

## 13. Project manager (PM)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| PM1 | G1 | Listing and sorting roots with hundreds of folders, including Google Drive paths, without blocking the UI | Measure | Under a stated time, async | main 5.4 |
| PM2 | G1 | Git branch and dirty count for 30 projects: parallel, timeouts, cache | Measure | Bounded | main 5.4 |
| PM3 | G1 | Last agent and thread per project, matching paths across agents (S9), worktree versus main tree | Test | Correct | main 5.4 |
| PM4 | G1 | New project: name validation per OS (reserved names, trailing dots, case-insensitive duplicates), `git init` default branch, permissions, a root on a synced drive | Test | No half-created folder | main 5.4 |
| PM6 | G2 | A folder picker for any folder (F3) on each OS, drive letters, no terminal-escape traps | Test | Works | main 5.4 |
| PM7 | G1 | A missing root (removed drive) is reported, not ignored | Test | Reported | main 5.4 |
| PM8 | G1 | Where the configuration lives on each OS | Decide and test | Documented | main 5.4 |

## 14. Skins and UI (U)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| U1 | G1 | Contrast of every text role on every skin, measured by a script on the final tokens | WCAG 4.5:1 | All pass or exemptions written | main 5.7 |
| U2 | G1 | Changing the skin live applies to the chrome (and the DeepSeek Harness ACP pane) without restart or flicker; terminal panes keep their harness profiles unless the user opted into follow-skin, which remaps only the default and 16-colour palette | Test | Works | main 5.7 |
| U3 | G0 | Global key bindings (spec Q4): which keys each agent consumes and which each OS takes before the native window gets them; choose a leader key or prefix | Key matrix | A binding set with no conflicts, written | main 5.1; mockups |
| U4 | G1 | Mouse support, focus, tab clicks; keyboard-only completeness | Test | Complete | main 5.1 |
| U5 | G1 | Layout at narrow widths; overlays; side panel collapse | Resize | No overlap or loss | main 5.1 |
| U6 | G1 | Rendering four panes: background panes are parsed but not drawn; CPU | Measure | Within budget | main 3 |
| U7 | G1 | Italian keyboard (AltGr), IME, CJK input pass through | Test | Correct | main 5.1 |
| U8 | G2 | No glow effects; the design review of the real UI (not the HTML mockups) | Review | Findings closed | main 5.7 |

## 14a. Sessions surface (SS)

These gates cover functional design 5.16 (sessions surface) and technical design
3.1. They have not been executed.

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| SS1 | G1 | Tree and detail use the declared Mukade pin and list every harness the pin reads, including native children where supported, from explicit box and read-only system roots; metadata unavailable from `list` remains `unknown`; filter, focus and rescan work; child-process readers leave stores and side files untouched | Synthetic stores, boxed instances and filesystem/environment/process-tree diffs | Correct list and provenance; no write, ambient fallback or invented metadata | main 5.14, 5.16; engine 3.1, 5.5 |
| SS2 | G1 | Paged transcript reads bound bytes, decoded records and retained raw end-to-end; Overview, tool calls and in-session search are correct; a 30 MB session meets the recorded total-process memory and responsiveness protocol, larger input pages or returns `too large`; cancelled/stale reads cannot update another panel | Synthetic large sessions, raw-dense fixtures, cancellation and panel/store revision races | Correct rendering within the recorded budget; no unbounded full-file read or stale update | main 5.16; engine 3.1, 5.5 |
| SS3 | G1 | Lineage distinguishes Shinatama thread chain, native parent and native children; Pi/OpenCode relations use the declared pin, Claude/Codex do not invent edges; missing/external/ambiguous parents and cycles are marked; navigation opens without resuming | Forked Pi/OpenCode sessions, missing-parent and cyclic fixtures, Claude/Codex sessions | Correct graph and provenance; no resume or inferred relation | main 5.16 |
| SS4 | G0 | **SEC** Live seed hop and historical handoff have distinct source models; historical handoff has no close/idle/live catch-up and rechecks its source fingerprint. Post-v1 native write: exact staged preview and consent, reserved/journaled destination, crash recovery before/after write and receipt, no duplicate on double-click or cross-process retry, destination launched through the adapter | Live and historical flows, scratch stores, fingerprint changes, double-click, two processes and fault injection at every write/receipt point | Correct v1 seed-only behaviour; post-v1 no unconfirmed or duplicate write and every uncertain destination reconciled | main 5.2, 5.16; engine 6.2, 6.3, 11; mukade-handover 2, 5, 6 |
| SS5 | G0 | **SEC** Store catalog persists root identity, origin, revision and access; native keys include `store_root_id`; system roots are read-only and cannot become a box destination; same-harness two-home transfer is refused without a verified staged path; root removal/reconnection preserves history; executable overrides resolve per agent | Boxed and system agents, duplicate native ids across roots, hostile roots, migration and reconnect tests | Isolation and identity preserved; no collision, silent reassignment or unauthorized destination | main 5.14, 5.16; engine 3.1, 4.2; mukade-handover 3 |
| SS6 | G1 | Raw display/copy defaults to redaction with explicit override and no UI logging/persistence; planted secrets do not leak by default. STX, portable Mukade JSON and native export declare their format/losses and use atomic explicit output; headless resume is separate and cost-warned; cancel leaves no undeclared effect | Secret-laden raw/tool/attachment fixtures, malformed/oversized exports, overwrite/cancel and headless-cost runs | No default secret disclosure, ambiguous format, partial output or implicit model call | main 5.16; engine 5.5.1 |

## 15. Widget and animation (W)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| W1 | G0 | A graphical toolkit for a transparent, borderless, always-on-top, click-through window on Windows 11, macOS, X11, Wayland (GNOME, KDE, sway): idle CPU and memory, startup, 6 to 8 fps sprite animation | Prototype per toolkit candidate | One toolkit chosen with numbers | main 5.8 |
| W2 | G1 | Wayland always-on-top and placement per compositor; the fallback (notification-only mode) | Test | Documented | main 5.8 |
| W3 | G0 | **SEC** The widget never takes keyboard focus and never steals keystrokes when an alert appears | Type continuously while alerts fire, on each OS | No lost or redirected keystroke | main 5.8 |
| W4 | G1 | Event path: DB event to bubble in under 1 s; widget with no Shinatama running; autostart; several windows | Measure | Within target | main 5.8 |
| W5 | G0 | **SEC** Quick answers: allow or deny goes only to the right instance and prompt; the bubble shows the real command from the native event; a prompt that changed between event and click is refused | Race tests | No stale approval | main 5.8 |
| W6 | G1 | Bubble text: source (titles or last assistant text), redaction, length, language | Test | Correct; no secret shown | main 5.8 |
| W7 | G0 | GPT Image 2.5: the model name, variants and parameters from OpenAI's current documentation (the spec's claims come from secondary articles); transparent output with no halo; identity consistency across frames by a rubric; pilot of three frames (base, a mouth edit, an arm edit) before generating all | Generate the pilot | Rubric passed; cost recorded | main 5.8 |
| W8 | G1 | The sprite pipeline: atlas, premultiplied alpha, 1x and 2x, size budget, seamless idle loop | Build | Within budget | main 5.8 |
| W9 | G1 | Terms for using Bungie's character in a distributed build (legal decision) | Read terms; user decides | Decision written | main 5.8; README |
| W10 | G2 | Reduced-motion setting honoured; a mute and disable switch | Test | Works | main 5.8 |

## 16. Roundtable (R)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| R1 | G0 | How Shinatama reaches `moe_ask` (an MCP server) without holding credentials: as an MCP client, or through an agent; where `moe` gets model access | Read Daodan's moe and peer-review, try | One route written | engine 7.8 |
| R2 | G0 | **SEC** Answer-only per agent: no file written, no command run in each headless mode (CL11, CX9, OC7, PI8) | Canary prompts | None written or run | engine 7.6 |
| R3 | G1 | Parallel participants: rate limits on one account; per-participant cost; a cap that stops a round mid-way (interrupt granularity) | Run | Caps hold within a stated overshoot | engine 7.8 |
| R4 | G0 | **SEC** Injection between participants: an answer that tells another to write a file | Canary | Not obeyed in answer-only | engine 7.6 |
| R5 | G1 | Convergence, synthesis quality and real cost of three topics; calibrate the default caps ($2, 40k tokens) | Run | Defaults set from data | main 5.10 |
| R6 | G1 | A roundtable thread is searchable and can seed a work thread | Test | Works | engine 7.8 |
| R7 | G1 | Choosing a model per participant per launch, including a local OpenAI-compatible endpoint on the LAN, without editing global config; model lists from catalogs | Try per agent | Works, or the agent is excluded as a participant | main 5.10 |

## 17. Install, distribution, legal, telemetry (X)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| X1 | G1 | Install on macOS (arm64, x86_64) and Linux (glibc, musl; x86_64, aarch64): release binaries, installer script, package managers; signing and notarization; Windows signing and SmartScreen; checksums | Clean machines | Documented one-command install per OS | README |
| X2 | G1 | First run on clean Windows 11, macOS and Ubuntu: install to first hop, with a stopwatch and a list of failures | Clean VMs | Time and failures recorded | README |
| X3 | G0 | Licences of every dependency and bundled asset are compatible with MIT (crates, SQLite, llama.cpp, the small model, BillionContext, fonts, icons) | `cargo deny` and a manual list | A generated notice file; no conflict | README |
| X4 | G1 | **SEC** No unintended outbound connections: only agent traffic through the sidecar, model-catalog calls the user enabled, update checks the user enabled | Capture traffic during a full session | List matches the document | README; engine 4.7 |
| X5 | G0 | **SEC** A threat model reviewed by a second reader: untrusted agent output into the emulator (P9), the database, MCP and hooks, handoff files, local sockets, the widget quick answers (W5) | Review | Findings closed | new `docs/threat-model.md` |
| X6 | G2 | Shinatama's own updates: how they work and that they do not touch the sidecar's no-auto-update rule | Decide and test | Documented | README |

## 18. Token economy and Code Mode (T)

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| T1 | G1 | Baseline: tokens of tool definitions per agent in a typical session (Daodan plugins' MCP servers, bili, `shinatama mcp`) | Capture the first request per agent through the sidecar | Numbers per agent | engine 7.5a |
| T2 | G1 | Cloudflare Code Mode claims against the primary sources (the April 2026 report's 1.17 million to about 1,000 tokens figure is secondhand) | Read Cloudflare's own posts and documentation | Claims restated with primary citations | engine 7.5a |
| T3 | G2 | A prototype code-mode gateway in front of the real MCP servers: tokens saved, task success, latency, sandbox security, engine size and licence | Evaluated only after T1 (not in v1, not scheduled; user, 2026-11-01); prototype only if T1 shows a large footprint | Built only if the saving beats a threshold the user sets and success does not fall | engine 7.5a |
| T4 | G1 | Prompt-cache effect of a seed on the destination: cost in money per hop | Measure | Included in H22 | engine 6.4 |
| T5 | G0 | **Output steering, effort lever, per wire.** Whether a change of the effort field between two requests of one session lowers `cache_read_input_tokens` (or the provider's equivalent) on the second request. Documented, not measured: Anthropic says a change to `output_config.effort` or `thinking.budget_tokens` "always invalidates message blocks"; OpenAI says `reasoning.effort` "can change model-side reasoning instructions"; Gemini and DeepSeek pages do not say | Per provider the user actually uses (Anthropic, OpenAI Responses incl. the Codex ChatGPT route, Gemini, DeepSeek, each provider reached through OpenCode, Pi, Hermes): three requests on a fixed prefix of at least the provider's minimum cacheable length: A at effort X (writes), B same prefix plus one tool-result turn at effort X (control), C same as B at effort Y. Read the cache-hit field of B and C | The effort lever is locked off on every wire (decision 2026-11-01); this row is the evidence a later decision to lift the lock for one provider and model would need: C reads the same cached tokens as B (within the provider's stated granularity) and a measured saving beats the miss risk. Where C drops, the lock stays | token-savings 7.2 |
| T6 | G0 | **Output steering, verbosity lever, per wire.** The appended system-prompt directive keeps the cache stable after its first turn, on a harness that already places its own cache breakpoint in the system prompt (Claude Code does) | Enable on one session; compare cache-hit tokens of turns 2 to 5 with the same session recorded without it | Hit rate from turn 3 within noise of the control; one miss at the turn it is enabled is expected and recorded | token-savings 7.2 |
| T7 | G1 | **Cache-safe effort change.** The mechanisms the providers document for changing effort without rewriting the prefix: Anthropic per-message `output_config` in a `role: "system"` message (Fable 5.1, Opus 5.5, Opus 5, Sonnet 5.5, Haiku 5.5; not Opus 5 on Bedrock), OpenAI `configuration_update` (GPT-6 and later). BillionContext edits the top-level field, so it implements neither | Same three-request probe as T5, using the documented mechanism for C; then whether BillionContext can emit it per wire | C reads the same cached tokens as B; effort actually lowered (usage shows fewer reasoning tokens) | token-savings 7.2 |
| T8 | G1 | **Image weight in document work.** Image tokens per turn in a render-and-review session (slides, pages, scans) on each hosted harness, and the turn at which BillionContext folds them | One real session per harness with the measurement ledger of token-savings 4.4; read the stored session for folded media and `decompress({ imageRef })` recovery | Number of image tokens per turn and fold turn recorded. If images stay in context for more than a few turns, extend BillionContext's media fold; CAM is not added | token-savings 7.2 |
| TE5 | G0 | **TokenEconomizer on OpenCode 2.x: the turn closes.** The plugin closes a turn on `session.execution.(succeeded\|failed\|interrupted)`; does 2.2.5 emit it | One headless `opencode run --standalone` with the plugin from an isolated `OPENCODE_CONFIG_DIR/plugins/` and a throwaway `TOKENECONOMIZER_HOME`, one prompt that makes a tool call | TE's session record shows the turn with `resolved_proxy: true` and `envelope.json` gains a resolved turn | ADR-0045; TE study section 9 |
| TE12 | G0 | **TokenEconomizer tool-pruning evidence on OpenCode.** Does the evidence form once a catalog is present | Same isolated setup as TE5; read `untool.json` and `te --host opencode toolprune` | Evidence accrues against the catalog, or the cause of its absence is settled | ADR-0045 rule 4; TE study 7.1 |
| TE13 | G0 | **Shinatama's own OpenCode pruning evidence** (route B of the TE study 7.1): proposal computed from TokenEconomizer's session exports with the rule of `enough_evidence` and the protected set, applied as `permissions` deny rules | Exports reaching the `enough_evidence` threshold against a catalog of the instance's set; one launch with the rules, MCP servers direct and under Code Mode | Proposal excludes protected names and `bili_*`; the denied tools are absent from the request on 2.2.5 | ADR-0045 rule 4 v2 |
| T9 | G2 | Effective configuration, not intended: Shinatama's per-instance status line reads CCR, input compression and output steering from what the running sidecar resolved (including provider and model overrides) | Start the sidecar with each lever on and off; compare the line with the sidecar log | The line matches the log in all combinations | token-savings 7.2 |

Token-savings 9.2 (added 2026-11-01; ADR-0037 v2, ADR-0038):

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| T10 | G0 | Baseline ledger: input tokens per turn, cache hit rate, compaction count and tool re-runs after compaction, with no technique added | The ledger on at least ten real sessions per harness | Baseline recorded per harness; every later claim compares with it | token-savings 4.4.4, 8.1 |
| T11 | G0 | Tool re-run rate after compaction before and after each milestone of token-savings 8.1 | Same sessions as T10, per milestone | Rate recorded; a rise is a regression and blocks the milestone | token-savings 6.4 |
| T12 | G1 | Latency of one incremental scoring pass on the target CPU while four agents run | Instrument the julia1 sidecar | Recorded; the frontier model never waits for it | token-savings 6.2 |
| T13 | G0 | The fast-jev-compaction function-hook plugin loads on the installed Claude Code and `session.compact` receives the main conversation and sub-agent transcripts as `types/claude-code.d.ts` describes | Scratch home, function hooks enabled | Hook fires with the documented input, or the base-URL route is used | token-savings 5.1, 7.1 |
| T14 | G0 | Per adapter: its rewrite leaves the cache hit rate unchanged and the harness's native compaction does not run behind it | Per harness, compare with T10 | Hit rate within noise; no native compaction event | token-savings 7.1 |
| T15 | G1 | Hermes Agent: the setting or hook that stands its native compaction down | Read and run in a scratch `HERMES_HOME` | Found, or the Hermes adapter is not built | token-savings 7.1 |
| T16 | G1 | With a hosted router endpoint opted in, compaction and digest requests still reach only the local julia1 endpoint | Traffic capture of both endpoints | No compaction or digest request reaches the hosted endpoint | ADR-0037 v2 |

## 19. Evidence the specs quote that nobody ran

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| E1 | G0 | Every `[R]` line in engine 5.7 (read from docs, source or `--help`, not run) is confirmed on the installed versions by the matching fixture above | The CL, CX, OC and PI rows | Each line becomes `[V]` with a date, or is corrected | engine 5.7 |
| E2 | G2 | The Agent Deck claims in engine 6.8 (`[S]`: read at the tag, not run) are checked by a second reader against the tag | Re-read the files cited | Table confirmed or corrected | engine 6.8 |
| E3 | G1 | The tool facts quoted from Windows documentation: the 32,767 and 8,191 limits | Spawn test (P16, CL2) | Confirmed | engine 5.6 |
| E4 | G1 | "SQLite WAL needs a local disk" and what happens on Google Drive, with a throwaway folder only | S2 | Confirmed | engine 4.1 |
| E5 | G2 | The structure notes marked `[V]` in engine 5.3 are re-captured when an agent's version changes | The fixture suite | Versions in the spec are current | engine 5.3 |

## 19a. Browser integration: Moli and Patchright UI (B)

The requirements and boundaries are in `docs/browser-integration.md`. No browser
binary has been installed or run for this proposal. Source reading is not a pass.

| ID | Prio | What must be proved | How | Pass when | Writes back to |
|---|---|---|---|---|---|
| B1 | G0 | Pinned Moli `fetch` on synthetic local HTML: correct text/structured extraction, bounded output/time, no optional resources for text, no `serve` listener | Run versioned fixtures; capture argv, output and sockets | Expected output; only the one-shot process; no CDP/WebDriver listener | browser spec 6.1 |
| B2 | G1 | Static preview: layout/resources only when requested; correct screenshot; private artifact; honest native preview pane display/fallback | Render visual fixtures in a native preview pane and in the private-artifact-plus-text fallback | Visual output matches the fixture; text-only mode is not called visual success | browser spec 4.2, 6.1; U4/U5 |
| B3 | G0 | Representative MCP web UI within the one-shot boundary, without silently enabling `serve` or general agent `--eval` | Probe a local synthetic MCP UI with fixed operations | The required workflow works within scope, or unsupported interactive work is explicitly deferred | browser spec 2, 6.1 |
| B4 | G0 | **SEC** `moli serve` and Patchright transports: authentication/authorization, listener inventory, proxy/redirect behavior and untrusted-page attempts to reach automation endpoints | Source review plus local hostile-page and socket tests; no real credentials | No unauthenticated/reachable control path; otherwise `serve` or the failing UI transport stays disabled | browser spec 6.2, 6.3, 8 |
| B5 | G0 | Patchright/Chrome is absent at startup, lazy-loaded and started only by an explicit UI request; missing optional component does not auto-install | Inventory processes before/after startup and commands; test component absent | No pre-request browser/companion; explicit UI works or returns clear `unsupported` | browser spec 6.3, 10 |
| B6 | G0 | **SEC** Routing and authority: hostile pages, redirects, errors and agent/peer output cannot change engine/scope, open Chrome or answer approvals; post-error switch is a new user request | Canary content and state-machine tests across both engines | No unauthorized transition or effect; engine and permission snapshot remain immutable | browser spec 5, 8; engine 7.9 |
| B7 | G0 | Whole-tree lifecycle on Windows, macOS and Linux: normal close, cancellation, startup failure, owner crash, PID reuse; no wrong-process kill | Instrument process groups/jobs, descendants and sockets through each path | No managed descendant/listener remains; cleanup state is truthful; unrelated processes survive | browser spec 7; engine 7.9 |
| B8 | G0 | **SEC** Profile, file/network scope and artifacts: symlinks, subresources, redirects, local services, proxies, temporary/persistent profiles, cookies and screenshots | Canary secrets and out-of-scope targets; inspect stores, logs and network | No out-of-scope read, personal-profile use, cross-engine state or secret leakage | browser spec 8; X4/X5 |
| B9 | G1 | UI concurrency/control: second session, user takeover, agent resume, stale queued action after close/control change | Race tests with fake external effects | One controller; no focus/profile theft or stale/duplicate effect | browser spec 9 |
| B10 | G1 | Reproducible whole-tree startup, latency and memory, companion included, on equivalent cold/warm workloads | Freeze versions/hardware/fixtures; publish raw measurements and baseline | Reproducible report supports a user-set budget; no upstream percentage is reused as evidence | browser spec 10; X1/X3 |

## 19b. Rows defined in other records

The implementation plan links these families. Their checks are defined where shown; the step
that runs one adds its result here. All are open.

| Family | Defined in | Status |
|---|---|---|
| AT-1 to AT-12 | technical-design 11 | open |
| CS-1 to CS-19 | FRD-0012, the nineteen checks of spec 10.2 | open |
| MR-1 to MR-4 | FRD-0015 | open |
| TE1 to TE4, TE6 to TE11 | FRD-0016 | open |

## 20. Not empirical: waiting for the user

These are decisions, not experiments; they are listed so nothing is lost.

- O1 Which accounts and scratch logins may the tests use, and the cost cap per test run. Since 2026-10-28 it also blocks D15's login persistence: one sign-in per agent in a box.
- ~~O2~~ Decided 2026-11-01 (user): the default title backend is the small local model, offered at first run (about 500 MB, downloaded after consent, licence shown); if declined, none. The provider low-cost model stays a user choice. The quality gate (I5) confirms it (engine 8.4).
- O3 Windows in v1: answered 2026-11-02, yes, and only Windows for now. The exact scope of the "assistant" in the widget.
- O4 Whether to run the Oni character's use as a distribution decision now (W9).
- O5 Approval of the design and the engine design, so that the plan can be written.
- ~~O6~~ Decided 2026-10-28 (user: yes): the former launcher is out of scope. Its mentions were removed from the README, both designs and this register; the rows that used it as their reference (Q6, D6, PM5, PM9) were removed; P16, SC7, SC11, D2, PM7, PM8 and Q7 were rewritten to stand alone; the first-milestone set is unchanged.
- O7 Should the hop dialog, roundtable and palette offer only the actions both adapters' `capabilities()` support, showing what each choice loses, instead of offering everything and failing? This is a harness/cli idea, `docs/notes/2026-10-28-harness-cli-survey.md`.
- O8 Is the reduced first browser integration (`moli fetch` plus explicit Patchright UI, without `serve` or interactive Moli previews) sufficient? After B10, what performance budget should decide the default? Overall approval remains O5.
- O9 Confirm the two readings of 2026-10-31: "deep seek code" is DeepSeek Harness (`dsh`), and DeepSeek Harness, which has no terminal UI, is hosted as a native ACP pane rather than through its web UI (main 2, goal 1; main 5.16).
- O10 Whether to ask Daodan for Hermes Agent and DeepSeek Harness host packages (work in the Daodan repository; D20).

Next runs, no decision needed. These are listed in priority order:
1. P20 with a real nested interactive start for Codex, OpenCode and Pi.
2. D19 in interactive mode, and Pi's MCP brought through the instance's own extension.
3. OpenCode's D16 rerun (side by side and rollback; it needs about 1 GB free on C:).
4. Codex's update channels for an `own` copy (D16).
5. The Pi rows recorded on 1.2.0, rerun on 1.4.0.
6. The untested switches: `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES`, `CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL` and `CLAUDE_CODE_TMPDIR`.
7. Everything on macOS and Linux (deferred 2026-11-02).
8. SS5 against Mukade's `hop_into` once the pin is advanced past `4124b1f` (mukade-handover section 8).
9. Install Hermes Agent 0.22.5 and DeepSeek Harness 0.2.1-rc.5 in scratch homes (this machine has neither), then HM1, HM2, HM14, DS1, DS2 and DS15.

## 21. Runs so far

Local records: [verification/](verification/); scripts: `verify/<ID>/` (see the rerun and portability limitations in section 0). H27's record lives in Mukade (removed there on its documentation reorganization; section 21). First run 2026-10-27 on Windows 11 Home build 26200 (claude 2.1.311, codex-cli 0.164.0, opencode v2.1.4, pi 1.2.0); no login and no model calls were used.

| Item | Prio | Status | Record | Note |
|---|---|---|---|---|
| E1 | G0 | running (partial) | [2026-10-27-E1-agent-cli-recheck.md](verification/2026-10-27-E1-agent-cli-recheck.md) | `--help` surface of all four agents re-checked; Claude drift 2.1.310 → 2.1.311; runtime rows open |
| P14 | G0 | running (partial) | [2026-11-02-P14-containment-probe.md](verification/2026-11-02-P14-containment-probe.md) | Windows, synthetic children: job and console behaviour measured; real agents, crash paths other than TerminateProcess and sidecar open |
| CX4 | G0 | passed | [2026-10-27-CX4-codex-claude-import.md](verification/2026-10-27-CX4-codex-claude-import.md) | scratch detect/import/resume proven; tool calls arrive as text markers; age window is the detector's filter |
| PI5 | G1 | passed (spec corrected) | [2026-10-27-PI5-pi-tui-attach.md](verification/2026-10-27-PI5-pi-tui-attach.md) | Pi has no TUI attach; 5.7 item 4 corrected |
| PI10 | G2 | running (partial) | [2026-10-27-PI10-pi-package-update.md](verification/2026-10-27-PI10-pi-package-update.md) | `@earendil-works/pi-coding-agent` installed 1.2.0; registry latest 1.4.0 |
| PI11 | G0 | running (partial) | [2026-10-27-PI11-pi-startup-timing.md](verification/2026-10-27-PI11-pi-startup-timing.md) | timings recorded; bare launch open |
| D1 | G0 | done | [2026-10-27-D1-marketplace-name-collision.md](verification/2026-10-27-D1-marketplace-name-collision.md) | remote repo found alive, 24 plugins all in Daodan at equal or newer versions; user decided; deleted 2026-10-27 with bundle backup |
| CL13 | G1 | running (partial) | [2026-10-27-CL13-claude-auth-status.md](verification/2026-10-27-CL13-claude-auth-status.md) | both states recorded: real exit 0 (identity redacted), scratch exit 1 loggedIn false; PTY login open |
| CX11 | G1 | running (partial) | [2026-10-27-CX11-codex-login-status.md](verification/2026-10-27-CX11-codex-login-status.md) | both states recorded: text on **stderr**, exit 0/1; PTY login open |
| T5 | G0 | running (partial) | [2026-11-01-T5-effort-cache-claude-code.md](verification/2026-11-01-T5-effort-cache-claude-code.md) | Anthropic via Claude Code 2.1.316 only: top-level effort change kept the cache on Sonnet 5.5, Opus 5.5, Fable 5.1 and invalidated it on Haiku 5.5; full-configuration runs inconsistent; effect on thinking unmeasured; OpenAI, Gemini, DeepSeek, other harnesses not run |
| TE5 | G0 | passed (one trial per case) | [2026-11-01-TE5-opencode-turn-close.md](verification/2026-11-01-TE5-opencode-turn-close.md) | OpenCode 2.2.5 emits `session.execution.succeeded`: a one-tool-call turn closed with `resolved_proxy: true`; a zero-tool turn is never green by design; the `plugins` array skips a `file:///` URL to a file, the `plugins/` directory route works; the stall first attributed to the array was an inherited stdin (TE13) |
| TE12 | G0 | failed as designed, cause settled | [2026-11-01-TE12-opencode-tool-pruning.md](verification/2026-11-01-TE12-opencode-tool-pruning.md) | TokenEconomizer skips the pruning stage on OpenCode (`fold.py:115`, `denylist.py:55,76`); no evidence forms whatever catalog exists; replaced by TE13 |
| TE13 | G0 | passed (one trial per case; requests from a scripted provider, gateway down) | [2026-11-02-TE13-opencode-pruning-evidence.md](verification/2026-11-02-TE13-opencode-pruning-evidence.md) | Proposal `acme_lookup`, `experimental_image_generate`; protected names and `bili_*` kept; deny removes the tools from the request on 2.2.5, directly and under Code Mode. A Code Mode use shows only in `state.metadata.toolCalls`. `opencode run` hangs on an inherited stdin; BillionContext's real server name still open |
| T6 | G0 | passed for Anthropic through Claude Code on a small prefix; other wires and the full configuration not run | [2026-11-02-T6-output-steering-cache.md](verification/2026-11-02-T6-output-steering-cache.md) | Four models, 24 sessions of six turns, 22 usable. Steered and enabled arms match the control from turn 4 (lowest hit rate 0.9976 against 0.9958 to 0.9977). Enabling costs one miss of the whole message history. One Fable control session excluded for safeguard errors on turns 3 to 5, with no steering in it. The saving itself is not measured |
| OC8 | G1 | running (criterion not met on the real home) | [2026-10-27-OC8-opencode-auth-status.md](verification/2026-10-27-OC8-opencode-auth-status.md) | real home flaky (2 successes then 5x "Location plugins did not finish activating"); scratch answers; HTTP route candidate |
| PI9 | G1 | running (partial) | [2026-10-27-PI9-pi-auth-status.md](verification/2026-10-27-PI9-pi-auth-status.md) | check recorded (`pi auth check --json --no-refresh`, exit 1 not_ready for anthropic); signed-in provider + PTY login open |
| OC2 | G0 | passed | [2026-10-27-OC2-opencode-session-interchange.md](verification/2026-10-27-OC2-opencode-session-interchange.md) | interchange `{info,messages[]}`; round trip lossless for user/assistant+tool/compaction/system/synthetic/shell; 409 ConflictError; `/synthetic resume:false` stays in the inbox |
| H27 | G0 | changed | original record removed from Mukade's docs on its reorganization (commit `5ea6bd6`); current records: [Mukade docs/README.md](https://github.com/acaprino/mukade/blob/master/docs/README.md#verification-records) | external converter not adopted (user, 2026-10-27: own library); probe run kept: 36/36 written and resumed, 4/4 destination-native compactions; requirements L1–L14 in `session-interchange-spec.md` — the library and its docs are now the `mukade` repository (github.com/acaprino/mukade) |
| P20 | G1 | running (partial) | [2026-10-28-P20-inherited-agent-markers.md](verification/2026-10-28-P20-inherited-agent-markers.md) | marker lists for all four agents; Claude with an inherited `CLAUDE_CODE_CHILD_SESSION` saves no transcript; the others pass markers through; union removal rule written to engine 3; nested interactive start for Codex/OpenCode/Pi open |
| D15 | G0 | running (partial) | [2026-10-28-D15-private-home-isolation.md](verification/2026-10-28-D15-private-home-isolation.md) | no agent private with its home variable alone: temp, Node caches, skill reads; Codex's Windows sandbox writes the real profile; login persistence pending user |
| D16 | G0 | running (partial) | [2026-10-28-D16-agent-install-update.md](verification/2026-10-28-D16-agent-install-update.md) | npm `--prefix` recipe for all four; self-update off proved for Claude, OpenCode, Pi; Codex not controllable yet; OpenCode side by side not finished (disk full) |
| D17 | G1 | changed | [2026-10-28-D17-opencode-xdg-children.md](verification/2026-10-28-D17-opencode-xdg-children.md) | no narrower set than `XDG_*`; children inherit it and lose `~/.config/git`; fixed by a private plugin restoring the user's `XDG_*` |
| D18 | G1 | running (partial) | [2026-10-28-D18-system-agent-discovery.md](verification/2026-10-28-D18-system-agent-discovery.md) | Windows discovery: two native installers, two npm installs, no winget; each agent has its own update command |
| D19 | G1 | running (partial) | [2026-10-28-D19-per-instance-sets.md](verification/2026-10-28-D19-per-instance-sets.md) | exact per-instance set works for all four in a box (Codex: set plugins pre-installed turned off; OpenCode: per-instance home; Pi: MCP via the instance's extension, untested); interactive mode, real login and OpenCode system home open |

H27 is evidence from a harness probe, not a test of the Shinatama runtime integration. Mukade's
subsequent source-model summary live checks and the remaining host acceptance gates are linked
from [the handover integration guide](mukade-handover.md#7-evidence-and-acceptance-gaps).
Shinatama's Mukade integration has not been implemented or verified.

Second run 2026-10-28, same machine (claude 2.1.312, codex-cli 0.164.0, opencode v2.1.4, **pi 1.4.0**: Pi rows recorded on 1.2.0 need a rerun). Runner incidents that day: drive C: filled up with test installs (cleaned); one cleanup killed another run's processes (that run was repeated); three "cli starting" lines went into the real OpenCode log through npm's `postinstall`; Codex's sandbox created `~\.sbx-denybin` in the real profile (removed).

Status values follow section 0; this index is the status view until a Status column is added to the tables.

Reuse survey (JanuraCode on this machine): `docs/notes/2026-10-27-januracode-code-survey.md` — official OpenCode import/export client and protocol schema, managed-OpenCode hosting, V1→V2 session migration, sign-in reference; **no cross-harness session translator found**.

External tool trial (not a register row): [UNI1 record](verification/2026-10-27-UNI1-unisessions-trial.md) — **UniSessions** (vibheksoni/session-export, MIT, PyPI 0.5.0): Claude/Codex/Pi-style conversions work in scratch; **the OpenCode legs are incompatible with 2.1.4** (its v1-shaped export is rejected by the import: 400 `info.cost` then `messages[0]`; v2 exports are read as nearly empty).
