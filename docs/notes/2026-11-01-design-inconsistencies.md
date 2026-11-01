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
| 53 | U | Proxy topology undecided (one per instance or shared; Q7, SC5) while TD 9 says each instance's traffic goes through "the proxy" | TD 9; register Q7, SC5 | FRD-0009 |

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

## 6a. Found on 2026-11-01 after the third round

| # | Kind | Item | Where | FRD |
|---|---|---|---|---|
| 106 | E | token-savings 4.4.3 still puts "summaries" on julia1, while activity titles stay on the user's chosen backend (TD 8.4, FD 5.13): which summaries are meant | specs/token-savings 4.4.3 | FRD-0013 |
| 107 | E | The summary fallback fires at task boundaries (4.3.4), but the library falls back inside the same compaction at `compactAtPercent` when the reduction is too small or the scorer fails (5.1.6) | specs/token-savings 4.3.4, 5.1.6 | FRD-0013 |
| 108 | E | token-savings 7.1 says BillionContext sits behind "one shared proxy core" while the topology is open (#53, SC5) | specs/token-savings 7.1 | FRD-0013 |
| 109 | G | harness-tools 3 says items 1–3 and 7 "are applied in revision 4.1 of the companion spec"; the spec at HEAD is revision 4 and contains none of them | harness-tools 3; companion spec | FRD-0012 |
| 110 | U | Codex has no read tool (harness-tools 3.1): the companion's Codex route reads files only through its shell, against the "no shell commands" policy of spec 6 A | harness-tools 3.1; spec 6 A | FRD-0012 |
| 111 | U | Codex's hosted search does not report result URLs (harness-tools 3.2), so the `seen` link check of 5.9 would mark every link unseen | harness-tools 3.2; spec 5.9 | FRD-0012 |
| 112 | E | "Every query is shown on the card" (spec 5.9) cannot be literal: Claude Code only counts its internal searches and DeepSeek hides its native ones (harness-tools 3.3) | spec 5.9; harness-tools 3.3 | FRD-0012 |
| 113 | E | Spec 6 A still says the design documents are silent on the web tools; harness-tools.md now covers them | spec 6 A | FRD-0012 |
| 114 | E | Spec 7 speaks of "the local model shared with the titler and julia1"; token-savings 7 now says they share the runtime and are one model only when one model serves both | spec 7; specs/token-savings 7 | FRD-0012, FRD-0013 |

Items 109–114 belong to the companion work, like section 6.

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
| 14 | U | User decision 2026-11-01: TD 5.4 allows stage 3 also when the agent's own import loses what Mukade's writer keeps (Codex, CX4), as 5.4.1. |
| 15 | U | User decision 2026-11-01: Codex per-instance sets exist only in a `box`; with the `system` home Codex instances use the home's own set and Shinatama writes nothing there (FD 5.14, FD 7, TD 3). Claude's `pluginUsage` rewrite is the agent's own bookkeeping, and Daodan in the native store follows the user's explicit `system` choice. |
| 16 | U | User decision 2026-11-01: the restriction to Daodan alone is lifted; any marketplace may be registered, including Daodan's dependency marketplaces (FD 5.5, DN 4.6, README). |
| 104 | U | User decision 2026-11-01: a known, harmless leak is shown and the agent stays boxable; only a leak touching credentials, sessions or the user's configuration makes it `system`-only (register D15). |
| — | E | FD 5.12 no longer labels the UI milestone "TL1"; register TL1 is the toolkit prototype gate only. |
| 30 | U | User decision 2026-11-01: the line is persistence; messages live at most 1 h within a project or roundtable; no mailbox outlives the sessions or the expiry (FD 2, TD 7). |
| 32 | U | User decision 2026-11-01 (O2): offer the local model at first run, else none; a provider model stays a choice (FD 5.13, TD 8.4, register O2). |
| 34 | U | User decision 2026-11-01: the overview keeps focus, hop and pause; a new instance in its own worktree is an option of the new-instance command (FD 5.13). |
| 35 | U | User decision 2026-11-01: attribute from tool-call targets; otherwise "possible collision" without naming an instance; generated files excluded (FD 5.13, TD 8.5, register Q3, I4). |
| 38 | U | User decision 2026-11-01: the gate defers the fast path, it does not withdraw Moli as the default (BI 11, FD 5.15). |
| 40 | U | User decision 2026-11-01: with `browser.patchright.enabled = false`, `open-ui` returns `unsupported` and says how to enable it (BI 10, FD 5.15). |
| 46 | U | User decision 2026-11-01: the code-mode gateway is evaluated after T1 and built only past the user's T3 threshold; not in v1 (token-savings, TD 7.5a). |
| 47 | U | User decision 2026-11-01: the title backend stays the user's choice; julia1 and the local title model share one runtime, one download if one model serves both (token-savings, TD 8.4). |
| 48 | E | token-savings states which trigger applies where (library threshold vs the fallback at task boundaries). |
| 49 | U | User decision 2026-11-01: request-path behaviour goes into the sidecar's single source (Musashi Terminal); no fork or patch inside Shinatama; token-savings 8.2 says "no second proxy". |
| 50 | E | token-savings goal 2: replaced as the primary method; summary compaction stays the fallback (4.3.4). |
| 51 | U | User decision 2026-11-01: state unknown until DS9; login opens `dsh web` on the home the agent uses, box or system (FD 5.11, TD 5.7). |
| 80 | U | User decision 2026-11-01: sorted by last use in Shinatama; non-standard = outside the project roots; filter on `/`, `n` creates; the new-project dialog's Daodan option replaces the one-time offer for created projects (FD 5.4, DN 4.7). |
| 81 | U | User decision 2026-11-01: try `CLAUDE_CODE_SKIP_MCP_DEBUG_LOG_FILES`; if it fails, the leak is disclosed and Claude stays boxable (FD 5.14, register). |
| 82 | U | User decision 2026-11-01: MCP web interfaces stay in the first integration if B3 demonstrates them within one-shot `fetch`, otherwise explicitly deferred (FD 5.15, BI). |
| 105 | U | User decision 2026-11-01: the bypass marker goes into the sidecar's single source, inherited by Shinatama (TD 9, register SC9). |
