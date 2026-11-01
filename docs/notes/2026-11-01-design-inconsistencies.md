# Design inconsistencies — worklist (2026-11-01)

Found while writing ADR-0004 to ADR-0040 and extracting FRD-0001 to FRD-0013 from the
documents at commit `da4b83e`; the companion items (section 6) were redone on revision 3 of the
companion spec, at commit `e221915`. Each item names where the documents disagree. Resolved items move to
section 7 with their resolution.

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
| 10 | E | Token-savings decisions (goals 2, 5, 8.2) carry no date | specs/token-savings | FRD-0013 |

## 2. Contradictions in behaviour

| # | Kind | Item | Where | FRD / ADR |
|---|---|---|---|---|
| 14 | U | Stage-3 writes only for an agent with no import of its own (TD 5.4) vs Mukade's Codex writer writing the rollout although Codex has an import (TD 5.4.1) | TD 5.4, 5.4.1 | ADR-0016, ADR-0020 |
| 15 | U | "Never edit global configuration" vs three accepted writes: Codex set pre-installed turned off (a home write; on the `system` home this is the user's Codex home), Claude Code rewriting `pluginUsage` (D19), Daodan into the native store on a `system` home | FD 3, 5.14; D19 | FRD-0002; ADR-0013 |
| 16 | U | Daodan depends on 8 plugins from other marketplaces (D15) vs "one marketplace per host" | D15 record; FD 5.5; DN 4.6 | FRD-0008; ADR-0032 |
| 30 | U | Non-goal "a persistent addressed mailbox" vs TD 7's addressed, queued messages with a 1-hour expiry: where the line is | FD 2; TD 7 | FRD-0007; ADR-0024 |
| 32 | U | Default title backend: a quality gate (FD 5.13), the author's recommendation (TD 8.4), a user decision (register O2) | FD 5.13; TD 8.4; O2 | FRD-0010; ADR-0028 |
| 34 | U | The overview's only actions are focus, hop, pause summaries, yet "own worktree for a new instance" starts one from it | FD 5.13 | FRD-0010 |
| 35 | U | Collision from `git status` cannot attribute a change to an instance (register Q3, I4) | FD 5.13; TD 8.5 | FRD-0010 |
| 38 | U | BI 11 may withdraw Moli as the default, which requirement 1 (user, 2026-10-29) decided | BI 1, 11 | FRD-0011; ADR-0035 |
| 40 | U | `browser.patchright.enabled = false` vs an explicit `open-ui`: does `open-ui` work with the setting off? | BI 10; FD 5.15 | FRD-0011 |
| 46 | U | Code-mode gateway: a later milestone (token-savings 7, 8.1) vs "not in v1, an evaluated option" (TD 7.5a) vs built only past a user threshold (T3) | specs/token-savings; TD 7.5a | FRD-0013; ADR-0025 |
| 47 | U | Titles through julia1 (token-savings 4.4.3) vs the user's title backend choice (TD 8.4); one model download or two | specs/token-savings; TD 8.4 | FRD-0013; ADR-0028 |
| 48 | E | Compaction trigger: task boundaries (4.3.4) vs the library's `compactAtPercent` 60 vs "at threshold" (7) | specs/token-savings | FRD-0013; ADR-0037 |
| 49 | U | "No single shared proxy" (8.2) vs BillionContext as "one shared proxy core" (7.1), and adapters acting inside an unmodified sidecar's request path | specs/token-savings; FD 3 | FRD-0013; ADR-0038 |
| 50 | E | Goal 2 "replace summary compaction" vs 4.3.4 keeping it as a fallback | specs/token-savings | FRD-0013 |
| 51 | U | DeepSeek sign-in "shows as unknown" (FD 5.11) vs unknown until DS9 (TD 5.7); login with the `system` home not stated | FD 5.11; TD 5.7 | FRD-0003 |
| 53 | U | Proxy topology undecided (one per instance or shared; Q7, SC5) while TD 9 says each instance's traffic goes through "the proxy" | TD 9; register Q7, SC5 | FRD-0009 |
| 104 | U | Register D15's fallback ("the leak recorded and the agent marked `system`-only") contradicts FD 5.14 and FD 7, where Codex's recorded `~\.sbx-denybin` leak is shown to the user and Codex stays boxable | register D15; FD 5.14, 7 | FRD-0002; ADR-0012 |
| 105 | U | TD 9 (from SC9): where the rewriter or CCR would alter the seed, a bypass marker is added; that may need sidecar support, while the sidecar code stays unmodified (ADR-0029) | TD 9; register SC9 | FRD-0009; ADR-0029 |

## 3. Stale text and dangling references

| # | Kind | Item | Where |
|---|---|---|---|
| 84 | E | Register T5 to T9 (added 2026-11-01) cite token-savings 4.4 for output steering and 7.2 for a per-instance status line; the spec has no output-steering text and no section 7.2 | register T5–T9; specs/token-savings |
| 85 | E | Register 21 links `verification/2026-11-01-T5-effort-cache-claude-code.md`, which is not in the repository | register 21 |

## 4. Rules that live only in the register

| # | Kind | Item | Register |
|---|---|---|---|
| 79 | G | Token-savings verification items (9.2) have no register rows; no other document refers to the spec | specs/token-savings 9.2 |

## 5. Undefined in the documents

| # | Kind | Item | Where |
|---|---|---|---|
| 80 | U | Project list "sorted by last change" (of what?), "last 10 non-standard folders" (which?), the `n` shortcut vs the incremental filter, and two Daodan offers for a new project | FD 5.4; DN 4.7 |
| 81 | U | Claude's MCP-log leak in a box (D19) is not disclosed the way Codex's is | FD 5.14; D19 |
| 82 | U | Browser MCP web interfaces "can" be used (FD 5.15) but may be deferred (BI 11, B3) | FD 5.15; BI 11 |

## 6. Companion spec, revision 3

Items 9, 42–45, 68 and 83 of the first list referred to revision 2 and are replaced by these.

| # | Kind | Item | Where | FRD |
|---|---|---|---|---|
| 86 | G | The companion is a second native chat pane run headless; FD 5.1 plans no native redraw of a live chat except DeepSeek (spec 9.7 acknowledges it) | FD 5.1; spec 9.7 | FRD-0012 |
| 87 | G | Neither FD 5.16's panel table nor TD 3.1's `PanelDescriptor` kinds list a companion pane or an "Explain this" action; 9.7 names only 5.16 | FD 5.16; TD 3.1 | FRD-0012 |
| 88 | U | Spec 6 B has Shinatama make read-only copies or bindings of the harness's credential files at launch, against "Detect, never collect" (FD 5.11); 9.7 does not mention it | FD 5.11; spec 6 | FRD-0012, FRD-0003 |
| 89 | U | Each companion gets its own scratch home, a third kind of home beside box and system (FD 5.14, 5.16); 9.7 lists no change to 5.14 | FD 5.14; spec 6 | FRD-0012, FRD-0002 |
| 90 | U | On close Shinatama deletes the native session in the companion's scratch home, against TD invariant 7 (native stores read-only in v1); not acknowledged | TD 1; spec 8 | FRD-0012 |
| 91 | G | TD 4.2 has no thread kind `companion`, no link column, no record table, no `learning_memory` table (acknowledged) | TD 4.2; spec 9.7 | FRD-0012 |
| 92 | G | TD 4.7: companion feed and ledger leave the machine to the companion's provider; sidecar coverage unknown; `forget` does not cover the companion record (acknowledged) | TD 4.7; spec 7 | FRD-0012 |
| 93 | G | TD 5.5 lists `ask_headless` for roundtables and titles only, answer-only with no tools; no read-only-with-reads policy, no stateless mode; TD 7.6 has no wrapper escaping (acknowledged) | TD 5.5, 7.6 | FRD-0012 |
| 94 | G | TD 8.1/8.3 waiting events carry no request payload, which decision cards need (acknowledged; unverified per harness) | TD 8.1, 8.3 | FRD-0012 |
| 95 | U | The spec withholds `shinatama_send` and `shinatama_inbox` from the companion and says nothing of `shinatama_instances` and `shinatama_ack` (TD 7.7) | TD 7.7; spec 6 C | FRD-0012 |
| 96 | U | The companion has no `instance` row, yet the overview shows it as a child row with an "explanation ready" badge; the overview is built from `instance` and event rows | FD 5.13; TD 8.3; spec 8 | FRD-0012, FRD-0010 |
| 97 | U | The two flows are marked decided (goal 2, 9.1) while preset-only input is "my call until the user approves this revision" (goal 7, 5.6, 9.1), though both come from the user's pasted design | spec 2, 5.6, 9.1 | FRD-0012 |
| 98 | U | The 5.7 "decided" heading covers mechanics the quoted words do not (levels 1–4, the 30/40/2 KB selection, notice wording, failure handling) and clashes with 5.6 and 5.8 "my call" on the same level rules | spec 5.6–5.8 | FRD-0012 |
| 99 | E | Section 8's header shows "mode", but revision 3 has no modes | spec 8 | FRD-0012 |
| 100 | E | 9.6 puts "the feed" and expand-on-last-message in R1, which lacks the turn-end signal and incremental reader a per-turn delta needs (section 4) | spec 4, 9.6 | FRD-0012 |
| 101 | E | Turn-card length: 200 words and one diagram in EX-001 and row 5; no cap in 5.1; preset cards capped at 150 words in 5.6 | spec 5.1, 5.6, 10.2 | FRD-0012 |
| 102 | E | Expand takes the worker's last message as an operand, but no pane control for it is described | spec 5.6 | FRD-0012 |
| 103 | U | The daily cap amount ($2.00) is open point 1 | spec 9.5 | FRD-0012 |

## 7. Resolved on 2026-11-01

| # | Kind | Resolution |
|---|---|---|
| 1 | U | User decision (confirmed 2026-11-01): TD 3 now "decided by the user 2026-10-28"; FD 7 marks it (user). |
| 2 | U | User decision (confirmed 2026-11-01): TD 9 proxy supervision no longer [C]. |
| 3 | U | User decision: FD 7 marks the sessions-surface sequencing (user; revised 2026-10-31). |
| 4 | U | User decision (confirmed 2026-11-01): roundtable shape decided in goal 10, 5.10 and FD 7; scheduling and the free-form variant stay open. |
| 5 | U | User decision: FD 7 marks home `box` (user, 2026-10-28). |
| 6 | U | User decision (confirmed 2026-11-01): FD 5.5 and DN 1 record the shape as confirmed; DN 7 calls still to check. |
| 7 | E | FD 6 heading: the requirement is the user's, decided (engine 5.5). |
| 8 | U | User decision: FD 5.16 scope (2026-10-30) and FD 5.12 baseline (2026-10-31) marked decided by the user. |
| 11 | E | FD 5.8: the widget reads the SQLite `event` feed; a socket or pipe is only a wake-up ping. |
| 12 | E | FD 5.13 uses the engine 8.1 state names. |
| 13 | U | User decision 2026-11-01, tiered rule: failing capabilities `unsupported`, the agent `Degraded` (launch-only plus its own export), hops by seed or refused; TD 5.2, 5.5.3, 11.10, FD 5.14 and FD 7 agree. |
| 17 | E | Register D2 names the current marketplace (1.0.0 since the 2026-10-29 reset); Q1 and D1 stay as history. |
| 18 | E | FD 7 item 3: local checkout deleted 2026-10-26, remote 2026-10-27. |
| 19 | E | FD 5.5 and DN goal 3 follow DN 4.3 and D13 (COMMITTED, confirmed closure, separate message). |
| 20 | E | FD 5.2: under 8 s warm, under 25 s cold (engine 6.1 targets). |
| 21 | E | FD 5.2: a fork starts with a seed in v1; the agent's own fork or an import only post-v1. |
| 22 | E | Register H1: every ordered pair among the six agents, as TD 11.2. |
| 23 | E | FD 5.1: one SQLite database per user (decided); tables are the builder's. |
| 24 | E | FD goal 3: every supported agent Mukade reads. |
| 25 | E | FD 4: `forget` is engine 4.7. |
| 26 | E | TD 5.5.2 and FD 4 mark `import` post-v1. |
| 27 | E | FD 8 notes the exit codes proposed in engine 5.5.2 (my call). |
| 28 | E | FD 5.10: headless participants only, no PTY fallback (engine 7.8). |
| 29 | E | FD 5.10: interject between rounds or stop; `max_rounds` and caps (engine 7.8). |
| 31 | E | TD 12 includes the per-project setting of 7.2. |
| 33 | E | FD 5.13: every state change and every 2 min while working (engine 8.4 defaults), a setting. |
| 36 | E | TD 8.3 lists `file_collision`. |
| 37 | E | Register B2 and BI 2: native preview panes. |
| 39 | E | TD 7.9 summary agrees with BI 8. |
| 41 | E | BI 4.1: the spelling is the author's call (FD 4); BI 12 no longer lists command names. |
| 52 | E | FD 5.11: shared provider keys are not in v1 (my call), as FD 7. |
| 54 | E | TD 5.7 consequence 5: "the other five". |
| 55 | E | TD 0: later records state their own versions; Pi rows on 1.2.0 to rerun. |
| 56 | E | TD 0 defines `[H: …]` and the tag-with-note forms. |
| 57 | E | FD 5.9: external terminals are not offered because every agent runs in Shinatama's panes (5.1). |
| 58 | E | FD 9: the daodan-flow mockup note follows the 2026-10-27 decision. |
| 59 | E | DN 5: the owning window's life. |
| 60 | E | DN 3 also names the per-project "Not now". |
| 61 | E | FD 5.1 and register P5, P6, P8, P9, P13, P19, U3 describe the native window, not an outer terminal. |
| 62 | E | Register U2: skins change the chrome; terminal profiles stay unless follow-skin. |
| 63 | E | Register I10 tests native icons with text fallback and accessible names. |
| 64 | E | Register TL1 is named the toolkit prototype gate of TD 3.1, distinct from the UI milestone. |
| 65 | E | Register 21 OC8: "running (criterion not met on the real home)". |
| 66 | E | FD 5.7: scan lines and cut corners are optional decoration. |
| 67 | E | TD 3 lists the Hermes Agent and DeepSeek Harness home variables (HM9, DS9). |
| 69 | E | FD 5.1: Windows results only if Windows is in v1 (open question 1). |
| 70 | G | TD 6.4: seed lines starting with `/`, `!`, `@path`, `#`, `-` are never executed or expanded [C; Q9, H7]. |
| 71 | G | TD 9: the rewriter and CCR leave the seed, bootstrap and receipt as sent [C; Q5, SC9] (see 105). |
| 72 | G | FD 5.2: no file created in the user's repository without consent; "linked" still to decide [my call; H18]. |
| 73 | G | TD 7.3 (M5) and TD 7.4 (M7) [C]. |
| 74 | G | TD 7.8: default caps $2 and 40k tokens, to calibrate [C; R5]. |
| 75 | G | FD 5.8: W2, W3, W5, W10 as my calls. |
| 76 | G | FD 5.1: containment and process-tree cleanup as my calls (P9, P14). |
| 77 | G | FD 5.4: PM1, PM4, PM7 as my calls. |
| 78 | G | FD 5.14: `own` refused for Hermes if HM10 finds no recipe (my call). The D15 half is item 104. |
