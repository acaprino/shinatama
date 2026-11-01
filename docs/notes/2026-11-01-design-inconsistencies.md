# Design inconsistencies — worklist (2026-11-01)

Found while writing ADR-0004 to ADR-0040 and extracting FRD-0001 to FRD-0013 from the
documents at commit `da4b83e`. Each item names where the documents disagree. Nothing here is
fixed yet: this is the list to work through.

FD = functional-design, TD = technical-design, DN = daodan-native-support, BI =
browser-integration.

Kind:
- **E** editorial: the documents already settle it (a later or governing text wins, or a
  reference or label is stale); the fix is a wording change.
- **U** user: only the user can settle it (who decided, or which behaviour is wanted).
- **G** gap: a rule exists only in the register or only in one place; it has to be written
  into the governing section, or dropped.

Fixing a design document changes its hash, so the FRD evidence pointing into it must be
refreshed afterwards (`frd_tool.py merge` with a fresh extraction, or a new proposal while
the FRD is still Proposed).

## 1. Who decided (origin labels)

| # | Kind | Item | Where | FRD / ADR |
|---|---|---|---|---|
| 1 | U | Inherited-marker removal: "[C; decided by the user 2026-10-28]" (TD 3), "adopted" (harness/cli note), listed as a call to check (FD 7) | TD 3; FD 7; notes/harness-cli 4 | FRD-0002; ADR-0014 |
| 2 | U | Proxy supervision (one supervisor, busy is not dead, no diagnostics on agent terminals): under the "decided" heading of FD 5.6, [C] in TD 9 | FD 5.6; TD 9 | FRD-0009; ADR-0030 |
| 3 | U | Sessions surface: heading "decided 2026-10-30; revised 2026-10-31" vs listed among the calls to check | FD 5.16; FD 7 | FRD-0006; ADR-0023 |
| 4 | U | Roundtable shape: "open" (goal 10), "my calls" (5.10), the free-form variant in the calls list but "open" in 5.10 | FD 2, 5.10, 7 | FRD-0007; ADR-0026 |
| 5 | U | Default home `box`: the 5.14 heading quotes the user ("di base è con istanze private"), FD 7 lists it as an author default | FD 5.14; FD 7 | FRD-0002; ADR-0012 |
| 6 | U | Daodan native support: goal 5 marks reading, linking, carrying a task and read-only as decided; 5.5 says "shape is mine"; daodan-native 1 says "my reading" | FD 2, 5.5; DN 1 | FRD-0008; ADR-0033 |
| 7 | E | Adapter requirements: FD 6 says "requirements mine", TD 5.5 says "The user's requirement (decided)" | FD 6; TD 5.5 | FRD-0001; ADR-0011 |
| 8 | U | Terminal-first scope (2026-10-30) and delivery baseline (2026-10-31) are dated but not attributed | FD 5.12, 5.16 | FRD-0001; ADR-0040 |
| 9 | U | Companion spec marks default harness/model, the read-only layers, `summaries_excluded` and the decision cards as "my call", while ADR-0002 and ADR-0003 (Accepted) record them; the spec header still says "draft, not yet approved" | specs/companion-explainer 0, 9 | FRD-0012 |
| 10 | E | Token-savings decisions (goals 2, 5, 8.2) carry no date | specs/token-savings | FRD-0013 |

## 2. Contradictions in behaviour

| # | Kind | Item | Where | FRD / ADR |
|---|---|---|---|---|
| 11 | E | Widget feed: socket or named pipe (FD 5.8) vs the SQLite `event` table with an optional wake-up socket (TD 3, 4.6, 8.3) | FD 5.8; TD 8.3 | FRD-0010; ADR-0027 |
| 12 | E | Instance states: working, waiting for you, idle, done, error (FD 5.13) vs starting, idle, working, waiting_permission, waiting_question, error, exited (TD 8.1) | FD 5.13; TD 8.1 | FRD-0010 |
| 13 | U | An adapter that fails its fixtures or meets an unknown version: "degraded" (FD 5.14), "Degraded or Unsupported" (TD 5.5.3), "launch-only plus the agent's own export" (TD 5.2), "launch-only" (TD 11.10), the pair drops to seed or refuse (TD 6.7) | FD 5.14; TD 5.2, 5.5.3, 6.7, 11.10 | FRD-0002; ADR-0011 |
| 14 | U | Stage-3 writes only for an agent with no import of its own (TD 5.4) vs Mukade's Codex writer writing the rollout although Codex has an import (TD 5.4.1) | TD 5.4, 5.4.1 | ADR-0016, ADR-0020 |
| 15 | U | "Never edit global configuration" vs three accepted writes: Codex set pre-installed turned off (a home write; on the `system` home this is the user's Codex home), Claude Code rewriting `pluginUsage` (D19), Daodan into the native store on a `system` home | FD 3, 5.14; D19 | FRD-0002; ADR-0013 |
| 16 | U | Daodan depends on 8 plugins from other marketplaces (D15) vs "one marketplace per host" | D15 record; FD 5.5; DN 4.6 | FRD-0008; ADR-0032 |
| 17 | E | Daodan's version: marketplace 1.0.0 (FD 5.5, D4) vs 42.0.0 (register D2, Q1) vs 42.7.3 (D1 record); Daodan reset its versions to 1.0.0 on 2026-10-29, so D2 is stale and Q1, D1 are history | FD 5.5; register D2, Q1 | FRD-0008; ADR-0032 |
| 18 | E | Deletion of `acaprino/shinatama`: 2026-10-26 (FD 7 item 3) vs 2026-10-27 (README, DN 4.6, D1, register 21); only the local checkout went on 2026-10-26 | FD 7 | FRD-0008 |
| 19 | E | Takeover on hop: "the hop-dialog confirmation is the 'old session is closed'" (FD 5.5, DN goal 3) vs COMMITTED plus confirmed closure plus a separate message (DN 4.3, D13) | FD 5.5; DN 2, 4.3 | FRD-0008; ADR-0034 |
| 20 | E | Hop speed: "instant" (FD 5.2) vs under 8 s warm, under 25 s cold, to measure (TD 6.1) | FD 5.2; TD 6.1 | FRD-0005 |
| 21 | E | Fork: "the agent's own fork… otherwise an import or a seed" (FD 5.2) vs seed only in v1 (FD 5.2 seed-only paragraph, TD 6.2) | FD 5.2 | FRD-0005; ADR-0017 |
| 22 | E | Canary pairs: "all 12 ordered pairs" (register H1, four agents) vs every ordered pair among six (TD 11.2) | register H1; TD 11.2 | FRD-0005 |
| 23 | E | Thread storage: "format and location are the builder's call" (FD 5.1) vs SQLite per user decided (FD 8, TD 4) | FD 5.1 | FRD-0005; ADR-0009 |
| 24 | E | Search scope: "all supported agents" (goal 3) vs only agents Mukade reads (5.3, calls, TD 5) | FD 2, 5.3 | FRD-0006 |
| 25 | E | `forget` said to be in TD 5.5.2 (FD 4); it is only in TD 4.7 | FD 4 | FRD-0006 |
| 26 | E | `shinatama import` listed in TD 5.5.2 without "post-v1" | TD 5.5.2 | FRD-0006 |
| 27 | E | Exit codes: [C] in TD 5.5.2 vs left to the builder in FD 8 | TD 5.5.2; FD 8 | FRD-0006 |
| 28 | E | Roundtable transport: PTY typing fallback (FD 5.10) vs headless sessions only (TD 7.8) | FD 5.10; TD 7.8 | FRD-0007; ADR-0026 |
| 29 | E | Roundtable control: "interject in any round, stop, or ask for another" (FD 5.10) vs between rounds, stop, `max_rounds` (TD 7.8) | FD 5.10; TD 7.8 | FRD-0007 |
| 30 | U | Non-goal "a persistent addressed mailbox" vs TD 7's addressed, queued messages with a 1-hour expiry: where the line is | FD 2; TD 7 | FRD-0007; ADR-0024 |
| 31 | E | TD 12 restates messaging scope without 7.2's per-project setting | TD 12 | FRD-0007 |
| 32 | U | Default title backend: a quality gate (FD 5.13), the author's recommendation (TD 8.4), a user decision (register O2) | FD 5.13; TD 8.4; O2 | FRD-0010; ADR-0028 |
| 33 | E | Title frequency: a setting (FD 5.13) vs every 2 min and on state change (TD 8.4) | FD 5.13; TD 8.4 | FRD-0010 |
| 34 | U | The overview's only actions are focus, hop, pause summaries, yet "own worktree for a new instance" starts one from it | FD 5.13 | FRD-0010 |
| 35 | U | Collision from `git status` cannot attribute a change to an instance (register Q3, I4) | FD 5.13; TD 8.5 | FRD-0010 |
| 36 | E | TD 8.5 raises `file_collision`; TD 8.3 lists only `file_change` | TD 8.3, 8.5 | FRD-0010 |
| 37 | E | Browser previews: "on supported terminals" (register B2, BI 2) vs native preview panes (FD 5.15, BI revision note) | BI 2; register B2 | FRD-0011 |
| 38 | U | BI 11 may withdraw Moli as the default, which requirement 1 (user, 2026-10-29) decided | BI 1, 11 | FRD-0011; ADR-0035 |
| 39 | E | Browser content in logs: none by default (TD 7.9) vs the narrower BI 8.5, which TD 7.9 calls the complete contract | TD 7.9; BI 8 | FRD-0011 |
| 40 | U | `browser.patchright.enabled = false` vs an explicit `open-ui`: does `open-ui` work with the setting off? | BI 10; FD 5.15 | FRD-0011 |
| 41 | E | `/browse` spelling: "my call" (FD 4) vs "builder's choice" (BI 4.1) | FD 4; BI 4.1 | FRD-0011 |
| 42 | E | Companion default model: "a lower-cost model" (goal 4) vs "the lowest-cost model that passes a quality gate" (7) | specs/companion-explainer | FRD-0012 |
| 43 | U | RA-005 rejects relying on the harness policy alone, yet the opt-in preview "read-only: harness policy only" does that | specs/companion-explainer 6, 9 | FRD-0012 |
| 44 | E | Decision cards are sent "at once", but INV-007 allows one turn at a time; 9.6 step 2 orders work two ways | specs/companion-explainer 5.5, 9.4, 9.6 | FRD-0012 |
| 45 | U | Select-to-ask is defined on a terminal pane; DeepSeek Harness has none | specs/companion-explainer; FD 5.16 | FRD-0012 |
| 46 | U | Code-mode gateway: a later milestone (token-savings 7, 8.1) vs "not in v1, an evaluated option" (TD 7.5a) vs built only past a user threshold (T3) | specs/token-savings; TD 7.5a | FRD-0013; ADR-0025 |
| 47 | U | Titles through julia1 (token-savings 4.4.3) vs the user's title backend choice (TD 8.4); one model download or two | specs/token-savings; TD 8.4 | FRD-0013; ADR-0028 |
| 48 | E | Compaction trigger: task boundaries (4.3.4) vs the library's `compactAtPercent` 60 vs "at threshold" (7) | specs/token-savings | FRD-0013; ADR-0037 |
| 49 | U | "No single shared proxy" (8.2) vs BillionContext as "one shared proxy core" (7.1), and adapters acting inside an unmodified sidecar's request path | specs/token-savings; FD 3 | FRD-0013; ADR-0038 |
| 50 | E | Goal 2 "replace summary compaction" vs 4.3.4 keeping it as a fallback | specs/token-savings | FRD-0013 |
| 51 | U | DeepSeek sign-in "shows as unknown" (FD 5.11) vs unknown until DS9 (TD 5.7); login with the `system` home not stated | FD 5.11; TD 5.7 | FRD-0003 |
| 52 | E | Shared provider keys: "open" (FD 5.11) vs "not in v1" (FD 7) | FD 5.11, 7 | FRD-0003 |
| 53 | U | Proxy topology undecided (one per instance or shared; Q7, SC5) while TD 9 says each instance's traffic goes through "the proxy" | TD 9; register Q7, SC5 | FRD-0009 |

## 3. Stale text and dangling references

| # | Kind | Item | Where |
|---|---|---|---|
| 54 | E | TD 5.7 consequence 5 says "the other three" while six agents are listed | TD 5.7 |
| 55 | E | Pi 1.4.0 in P20 vs Pi 1.2.0 in TD 0 (register 21: Pi rows on 1.2.0 need a rerun) | TD 0; P20 |
| 56 | E | TD 0 does not define the bare `[H: …]` tag or mixed forms such as "[C; decided by the user …]" | TD 0 |
| 57 | E | FD 5.9 cites a "section 2, non-goals" entry on external terminal selection that does not exist | FD 5.9 |
| 58 | E | FD 9 says the user rejected `daodan-flow.html` for linking threads to tasks; goal 5 reversed that | FD 9 |
| 59 | E | DN 5: an agenda poll "beyond the terminal's life" predates the native window | DN 5 |
| 60 | E | DN 3 says SQLite keeps "only" the link and the last row; DN 4.7 also stores "Not now" | DN 3, 4.7 |
| 61 | E | "Outer terminal" in FD 5.1, the U3 note and register P6, P8, P13, U3: terminal-only era | FD 5.1; register |
| 62 | E | Register U2 ("changing the skin live applies to the hosted pane") predates terminal profiles | register U2 |
| 63 | E | Register I10 tests glyphs across terminals and Nerd Fonts; icons are native now | register I10 |
| 64 | E | Register TL1 prototypes Markdown and file panels; the TL1 milestone excludes the file navigator | register TL1; FD 5.12, 5.16 |
| 65 | E | OC8 record says "Not met"; register 21 lists it as "running (partial)" | register 21; OC8 record |
| 66 | E | FD 5.7: the Section 9 and TCTF skins list scan lines and cut corners, which the same section says are not requirements | FD 5.7 |
| 67 | E | TD 3's box home variables and register D15 cover four agents, FD 5.14 six | TD 3; register D15 |
| 68 | G | Companion spec 9.7: FD 5.1 and 5.16 do not declare the companion pane; TD 4.2 lacks thread kind `companion`; TD 4.7 does not allow the companion's provider data flow; TD 7.6 lacks wrapper escaping; TD 8.1/8.3 lack the pending-request payload | FD 5.1, 5.16; TD 4.2, 4.7, 7.6, 8.1, 8.3 |
| 69 | E | Windows is assumed (open question 1, O3) while FD 5.1 and register B7 require Windows results | FD 3, 5.1; register B7 |
| 84 | E | Register T5 to T9 (added 2026-11-01) cite token-savings 4.4 for output steering and 7.2 for a per-instance status line; the spec has no output-steering text and no section 7.2 | register T5–T9; specs/token-savings |
| 85 | E | Register 21 links `verification/2026-11-01-T5-effort-cache-claude-code.md`, which is not in the repository | register 21 |

## 4. Rules that live only in the register

| # | Kind | Item | Register |
|---|---|---|---|
| 70 | G | Seed lines starting with `/`, `!`, `@path`, `#`, `-` are not executed or expanded | Q9, H7 |
| 71 | G | The prompt rewriter and CCR do not alter or externalize the seed, bootstrap and receipt | Q5, SC9 |
| 72 | G | No file is created in the user's repository without consent ("CLAUDE.md ↔ AGENTS.md linked" is undefined) | H18 |
| 73 | G | The depth limit holds without `in_reply_to`; a terminal write never answers a permission prompt | M7, M5 |
| 74 | G | Roundtable default caps ($2, 40k tokens) | R5 |
| 75 | G | Widget: never takes keyboard focus; refuses a prompt that changed before the click; reduced motion and mute; notification-only fallback on Wayland | W3, W5, W10, W2 |
| 76 | G | An agent cannot write the clipboard, read the screen, retitle the window or inject input; closing kills the whole process tree | P9, P14 |
| 77 | G | Project manager: no half-created folder; a missing root is reported; listing never blocks the UI | PM4, PM7, PM1 |
| 78 | G | Box fallbacks: a leak recorded and the agent marked `system`-only (D15); `own` refused for Hermes (HM10) | D15, HM10 |
| 79 | G | Token-savings verification items (9.2) have no register rows; no other document refers to the spec | specs/token-savings 9.2 |

## 5. Undefined in the documents

| # | Kind | Item | Where |
|---|---|---|---|
| 80 | U | Project list "sorted by last change" (of what?), "last 10 non-standard folders" (which?), the `n` shortcut vs the incremental filter, and two Daodan offers for a new project | FD 5.4; DN 4.7 |
| 81 | U | Claude's MCP-log leak in a box (D19) is not disclosed the way Codex's is | FD 5.14; D19 |
| 82 | U | Browser MCP web interfaces "can" be used (FD 5.15) but may be deferred (BI 11, B3) | FD 5.15; BI 11 |
| 83 | U | Companion word limits (200, 250 words) exist only in acceptance material | specs/companion-explainer 10.2 |
