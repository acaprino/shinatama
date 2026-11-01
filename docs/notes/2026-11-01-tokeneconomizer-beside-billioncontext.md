# TokenEconomizer beside BillionContext — integration study

Date: 2026-11-01
Status: study, not a spec. It feeds `docs/specs/token-savings.md` (7.2, "Evaluated") and
ADR-0045; nothing is built from it until those are approved. Its companion is
`docs/notes/2026-11-01-model-routing-decision-model.md` (automatic model routing, ADR-0044); the
two were one study until the user split them (2026-11-01): "integrazione di TokenEconomizer
affiancato a BillionContext" on one side, routing on the other. julia1, a jev-like selector
model, serves both (section 8).

## 1. The question

The user asked (2026-11-01) for a study of integrating and adopting
`microsoft/TokenEconomizer` in Shinatama, next to BillionContext and next to the jev tool chooser
of token-savings 6.5, which exists only as a design and whose seam is the Claude Code function
hooks (the user noted that the OpenCode side is probably not enabled yet).

## 2. Answer in short

| Question | Verdict | Why, in one line |
| --- | --- | --- |
| Adopt TokenEconomizer? | **Yes, as an optional per-instance module, beside BillionContext, with two of its three mechanisms and one adaptation** | It attacks waste no Shinatama layer attacks (cross-session re-learning, never-used tools, stuck turns), acts through hooks, not a proxy, and is cache-aware by design; but it writes repo files that leak out of the box, covers four of the six hosted harnesses, and is tested on older versions than the installed ones (section 4) |
| Does it collide with BillionContext? | **No shared layer; four contact points to verify** | BillionContext reduces what is inside one session on the wire; TokenEconomizer learns across sessions through hooks and the transcript on disk (section 5) |
| Which mechanisms | Tool pruning (**propose** mode, applied by Shinatama at launch), loop guard (on), micro-skills only as `envfacts` | Section 6 |
| TokenEconomizer and the jev chooser | Complementary: TE removes tools never used across sessions; the chooser preloads likely deferred tools per turn | Different time scale, same layer (section 7) |
| julia1's part | The loop guard's judge, as a `noul` question, once TokenEconomizer offers an optimizer endpoint | Section 8 |

## 3. TokenEconomizer: what it is

Read, not run: `github.com/microsoft/TokenEconomizer` at commit `c53a4a6` (2026-10-31),
Apache-2.0, standard-library Python 3.10+. Read: `README.md`, `docs/ARCHITECTURE.md`,
`docs/PLUGIN.md` (sections 1, 2, 4, 5), `tokeneconomizer/plugin/sidecall.py`, the header of
`plugin/opencode/tokeneconomizer.ts`, and a search of the package for home variables. Paper:
Li et al., "TokenEconomizer: Experience-Driven Black-Box Cost Control for Coding Agents",
arXiv 2610.22732 (UC Berkeley, Microsoft Research, Microsoft Security). Clone kept outside this
repository.

It learns from the finished sessions of one repository and cuts waste in the next ones, without
touching the agent. Three mechanisms:

1. **Micro-skills.** Facts about the repository (the command spelling that works, a build step
   that must run first, an environment variable) learned from failures repeated across sessions
   and, every *k* = 3 sessions, from a model that reviews recent sessions. A fact is injected at
   session start once three sessions confirm it, dropped when later ones contradict it. Off by
   default. `microskills=envfacts` learns only from repeated failures, with no model call.
2. **Tool pruning.** A tool unused over *p* = 10 prompts across *m* = 2 sessions, or over
   *s* = 5 sessions, is denied through the agent's own settings from the **next** session. The
   deny list is written only after the repository has been idle *q* = 60 minutes, so a resumed
   conversation does not rewrite a still-live prompt cache. `toolprune=propose` writes nothing
   until `toolprune apply`. One command gives tools back.
3. **Loop guard.** After *e* = 5 turns that ended green, a turn showing repeated failures goes
   to a judge model; at confidence *c* ≥ 0.7 the agent gets one redirect (*r* = 1) as context on
   the tool result; still stuck, the person gets a one-line notice. It never stops a turn.

Its own figures (vendor-reported, not measured here): 18.4–30.9% lower mean cost on 277
SWE-Bench tasks across five agent–model pairs, Pass@1 up in 4 of 5; 16–21% on Web-Bench; 6–20%
on two security benchmarks where every task is a new repository. The plugin's live loop guard
ran only on provoked loops, and whether a redirect changes a looping model's course "is not
established" (`docs/PLUGIN.md` 5).

Its model calls (the proposer, the judge, the redirect writer) are **side calls through the
agent's own CLI**: `claude -p --output-format json --max-turns 1 --tools "" --strict-mcp-config
--mcp-config <empty file> --model haiku --no-session-persistence`, `codex exec --json --disable hooks
--ephemeral -s read-only`, `pi --print --no-extensions --offline`, `opencode run --format json`, each with
`TE_SIDECALL=1` so its hooks do not recurse (`sidecall.py`). The seam is one callable,
`runner(prompt, model) -> (text, effective_tokens)` (`runner_for`).

State lives under `$TOKENECONOMIZER_HOME` (default `~/.tokeneconomizer`), per repository (keyed
by its `origin` remote) and per host, files `0600` on POSIX (a no-op on Windows: `chmod` does not
set NTFS ACLs, `te_bootstrap.py:151`, `paths.py:33-47`).

## 4. Fit with Shinatama, per harness

| Harness | Installed here (2026-11-01) | TE tested range | Tool pruning unit, file | Loop guard seam | Notes |
| --- | --- | --- | --- | --- | --- |
| Claude Code | 2.1.316 | 2.1.300–2.1.310 | single tools, `.claude/settings.local.json` | `PostToolUse`, `PostToolUseFailure` hooks | **Newer than tested**; plugin loads per instance with `--plugin-dir` (D19) |
| Codex | 0.164.0 | 0.153.1–0.164.0 | capability packages, `.codex/config.toml` | hooks (Codex 0.141+) | In range; hooks need the "trust all" review once per home; `codex exec` prints no hook messages |
| OpenCode | 2.2.5 | 1.19.5 and 2.2.0, experimental | none yet | `tool.execute.after`; on 2.x the turn closes on `session.execution.(succeeded\|failed\|interrupted)` (`plugin/opencode/tokeneconomizer.ts:481-489`); `session.idle` is the 1.x path only | **Newer than tested**. If 2.2.5 does not emit `session.execution.*`, `Stop` never fires, no envelope forms and the loop guard is silently inert (review H3). The 10-minute fold (`STALE_S = 600`, `fold.py:32`) folds un-ended sessions for learning; it never closes a turn |
| Pi | 1.4.0 | 1.2.0–1.3.0 | none (Pi loads four tools) | extension events | **Newer than tested** |
| Hermes Agent | not installed here | not supported | — | — | No integration |
| DeepSeek Harness | not installed here | not supported | — | — | No integration |

Windows: TE says Windows beyond Claude Code is untested; its last commits fix Windows tests and
the OpenCode console on Windows. Shinatama runs on Windows first.

### 4.1 Admission against token-savings 7.2

| Test | Result |
| --- | --- |
| 1. Attacks a content kind no layer attacks | **Passes.** Cross-session waste (re-learning, tools never used, stuck turns) is attacked by no layer of the 7.2 table; BillionContext acts within one session's context |
| 2. Not a second proxy on the wire | **Passes.** Hooks and settings only. Its side calls ride the box's CLI and reach BillionContext's path through the environment on Claude Code and Codex; on Pi `--no-extensions` drops BillionContext's extension (review M3); on OpenCode `OPENCODE_CONFIG_CONTENT` against `OPENCODE_CONFIG` is unverified |
| 3. Does not rewrite the cached prefix on its own schedule | **Passes by design.** Facts at session start; deny list effective from the next session, after the idle wait; redirects appended at the tail. To verify per harness (section 9) |
| 4. What it removes comes back through a mechanism the model knows | **Fails for tool pruning**: a denied tool comes back only through a person's `toolprune restore`, not through `ToolSearch` or `acp_retrieve`. Acceptable only with two guards: the protected set (questions, plan mode, sub-agents, web) is never pruned, **but no MCP tool is protected by default** (`adapter.py:37-48`, `toolprune/tools.py:41`), so BillionContext's own recovery tools (`mcp__bili__acp_retrieve`, `decompress`, `acp_read_original_image`) are the likeliest proposals (review H1): Shinatama sets `toolprune_protect=mcp__bili__*` (`config.py:274`) and never applies a `mcp__bili__*` or `image_media` entry whatever TE proposes; and it shows the denied list at launch (4.1.4 of token-savings) |
| 5. Every hosted harness, or optional, off by default, per instance | **Optional route.** Four of six harnesses; tool pruning on two |

## 5. Beside BillionContext

BillionContext owns the request path and every reversible reduction of what is already in one
session's context (token-savings 7.2). TokenEconomizer never touches the wire: it reads hook
payloads and the harness's own transcript on disk, and writes settings, context at session start
and short redirects. So they do not share a layer:

| | BillionContext | TokenEconomizer |
| --- | --- | --- |
| Unit | one request, one session | one repository, across sessions |
| Acts on | the wire: stored results (CCR), absorb, tiered compress, prompt rewrite, output steering, image fold | hooks and settings: facts at session start, tool deny lists for the next session, redirects on a stuck turn |
| Reads | the requests as they pass | the hook payloads and the transcript the harness writes |
| Model calls | none of its own for these layers | side calls through the agent's own CLI (the judge, the proposer) |
| Undo | `acp_retrieve`, `decompress` | `toolprune restore`, facts dropped when contradicted |

What follows from it, each point either settled by reading or a check of section 9:

1. **TokenEconomizer learns from the raw sessions.** BillionContext rewrites the wire, not the
   transcript the harness writes, so the facts, the tool evidence and the loop guard's envelope are
   computed on what really happened. Its cost ledger reads the usage the provider reported, so it
   already counts BillionContext's savings (inference from `trace` and `ledger`; TE7 checks it).
2. **Its side calls ride BillionContext's path, on Claude Code and Codex.** A judge call inherits
   the box's environment, hence the sidecar's base URL (`sidecall.py:57-58`). For BillionContext it
   is a new header-less session under the anonymous profile (`session-id.ts:7-20,82`,
   `host-profiles.ts:88-91`): wire tools and the standing prompt are injected into a request made
   with `--tools "" --max-turns 1` (review M5; TE8 measures whether the judge's JSON answer rate
   suffers). On Pi the side call passes `--no-extensions --offline`, so BillionContext's extension
   is not loaded and only the `HTTPS_PROXY` half may apply (M3). `--no-session-persistence` keeps it
   out of the harness's history on Claude Code. **It also loads the box, not the instance** (review
   H5): `claude -p` gets none of the instance's `--settings` / `--setting-sources` / `--plugin-dir`
   flags, so it runs on the box's user `settings.json` with every plugin enabled there, their hooks
   and skills; `--strict-mcp-config` drops MCP servers only. **BillionContext's results feed the
   loop guard as failures** (review H6): an MCP result has no exit code, so error text in it counts
   (`loopguard/state.py:197-206`), and one repeated failure signature alone opens the judge gate
   (`rules.py:86-90`). With the live threshold (`minToolTokens` 4000) an `acp_retrieve` returns a
   file pointer without error text, not the body (`ccr.ts:53,413-427`), so the realistic sources
   are BillionContext's own receipts: `[Compression FAILED: …]` and `[acp_retrieve FAILED: …]`
   (`stream.ts:698,836`, `store.ts:190,204`), and the retrieved body itself if the threshold is ever
   lowered below 4000. TE11 measures it; the fix is an upstream exemption of a tool-name pattern
   from failure accounting.
3. **On OpenCode the redirect disappears into a stored result** (review H2, settled by reading).
   TokenEconomizer appends the redirect to the end of the tool result
   (`plugin/opencode/tokeneconomizer.ts:300-302,437-443`); CCR replaces a result of
   `compress.ccr.minToolTokens` (4000 live) or more with a placeholder that keeps only the head
   (`kernel/src/ccr.ts:477,527-558`, `maxHeadChars` 96), so the tail with the redirect never
   reaches the model, and TE counts the one redirect per turn as spent. On Claude Code the redirect
   travels as `additionalContext` beside the result and on Pi as a steering message: there it
   holds. The loop guard stays **off on OpenCode** until the redirect travels outside the result
   (upstream request; `ctx.session.synthetic` is the channel BillionContext uses for notices).
   TE9 measures the threshold crossing, not the mechanism.
4. **Facts injected at session start meet `promptRewrite`.** BillionContext's `promptRewrite`
   (`mode = "auto"` in the baseline) appends a note after the human message and never replaces it
   (`src/musashi-terminal/rewrite-wire.ts:60-84`), but the text it feeds the rewriter is the join of
   every text part of the last user message (`rewrite-wire.ts:13-20,40-41`), and Claude Code
   delivers `SessionStart` context inside the first user message. TE10 narrows to: does the note
   restate the facts. Micro-skills is off by default and used only as `envfacts`
   (section 6), so this matters only once facts exist.
5. **One owner of compaction is untouched.** TokenEconomizer does not compact; decision
   compaction and BillionContext's tiers stay the only reducers of history (ADR-0038 INV-001).
6. **The prompt cache.** BillionContext keeps the prefix stable per launch; TokenEconomizer
   changes the tool list only between sessions, after the repository has been idle longer than a
   cache lives, and adds facts only at session start. The two rules agree; TE2 measures it.
7. **Savings do not add up.** Fewer tool schemas also shrink what BillionContext caches, and a
   shorter loop leaves less for CCR to store: the two savings overlap. Only Shinatama's ledger
   (token-savings 4.4.4), with each module on and off, can tell what each one adds; neither
   project's own figure is used.

## 6. How Shinatama would run it

1. **Per-instance module, off by default**, chosen in the instance's set (functional-design
   5.14), packaged from `scripts/build_plugin.py`'s self-contained bundle and installed per box
   like a Daodan plugin: `--plugin-dir` on Claude Code; on Codex installed into the home and
   switched per launch with `-c` (unverified: BillionContext's overlay links `plugins/` so the
   cache stays visible, `launcher.ts:2975-2994`); on OpenCode through the `plugin` array of the
   instance's `$OPENCODE_CONFIG`, as BillionContext does (`launcher.ts:3390-3410`;
   `OPENCODE_CONFIG_DIR/plugins` is used by neither project), with the 2.x background service
   restarted so it sees `TOKENECONOMIZER_HOME`; `-e` on Pi. TE5 (2026-11-01) found the
   `plugin` array route stalling the standalone server once with a `file:///` URL, while a
   re-export wrapper in `<OPENCODE_CONFIG_DIR>/plugins/` loaded; Shinatama uses the latter
   until the stall is explained. Pinned to a commit; updated next to
   the running version, as the `own` program rule does. **Version gate is Shinatama's**: TE has
   none (flags by `--help`, `README.md:196-203`); the launcher refuses the module outside the range
   its checks passed on and says why.
2. **State inside the box.** `TOKENECONOMIZER_HOME` set by Shinatama to a folder per box (all
   instances of the box share it, so evidence accumulates across them). The default
   `~/.tokeneconomizer` would write the user's home, which a box never does.
3. **Tool pruning in `propose` mode; Shinatama applies the list at launch.** The default is
   `auto` (`config.py:152`), which writes `.claude/settings.local.json` or `.codex/config.toml`
   **in the repository**, and the user's own runs outside Shinatama read those files too: the box
   boundary leaks. Shinatama writes `toolprune=propose` explicitly, reads the proposal from
   `<TOKENECONOMIZER_HOME>/repos/<key>/<host>/untool.json` (`proposal`, internal schema,
   `fold.py:168-191`) and adds the deny entries to that launch only: the experiments' own channel
   (`--disallowed-tools` at launch, `docs/PLUGIN.md` 2). The idle rule (*q* = 60) exists only
   under `auto` (`fold.py:312-341`), so Shinatama implements the wait itself: a resumed session
   gets the new set only after its cache would have expired anyway. Three guards close the
   `apply` path (review H4): the `toolprune` slash command is not in the instance's set, the
   session-start notice that invites `toolprune apply` is not relayed, and `mcp__bili__*` and
   Codex's `image_media` are never applied.
4. **Loop guard on.** The judge is a `noul` question by nature ("is this turn stuck?", acted on
   at 0.7). Its side call today costs a Haiku call through the box login; under Shinatama it
   carries the box's whole plugin inventory (Daodan is preinstalled, ADR-0032), which TE's
   `--tools ""` and empty MCP config cannot remove, since the call runs on the box's user settings
   and not on the instance's set (review H5, confirmed); on Codex it also carries every MCP
   server of the home plus `bili`, since only Claude and Pi have an MCP switch (H7, Codex arm);
   on OpenCode the side call inherits the instance's set by environment and its single request
   carries no MCP tools yet (D19), so the cost there is the plugins of the set, not the servers.
   So the judge is not enabled on Codex before it runs on julia1, and on Claude Code only after
   TE4 is measured in a real box. A fresh box has no loop guard at all until five turns have ended green
   (`controller.py:290-291`). Target: the judge on julia1
   through the System One `noul` question, the redirect text from a fixed template or kept on
   the side call. That needs one seam in TE (an optimizer endpoint beside `runner_for`); it is
   an upstream request, not a fork, by the same rule as fast-jev-compaction (token-savings 8.1).
5. **Micro-skills as `envfacts` only.** The proposer's facts are open-vocabulary text derived
   from command output the repository controls, injected into every session; TE itself says to
   leave it off on untrusted repositories. `envfacts` fills fixed templates and calls no model.
   Overlap with Daodan's memory and the project instructions is likely; measure before turning
   the proposer on.
6. **Python.** TE runs `python` (3.10+) at every hook, including every tool call. Shinatama
   provides the interpreter (Hermes Agent's `own` recipe already brings a bundled uv and Python,
   functional-design 5.14) and measures the per-call hook latency on Windows before enabling it.
7. **One ledger.** TE prices sessions from the transcript with cache pricing; token-savings 4.4.4
   specifies Shinatama's own ledger. TE's `ledger.jsonl` is read as one more source, never as
   the authority for a saving figure.

## 7. TokenEconomizer and the jev tool chooser

| | TE tool pruning | jev tool chooser (token-savings 6.5) |
| --- | --- | --- |
| Time scale | across sessions | per turn |
| Acts | removes tools never used in this repository | preloads deferred tools likely needed now; shortens descriptions of unlikely ones |
| Cache | changes the tool list only at a session boundary after the idle wait | changes nothing in the list; preloading is a per-turn addition |
| Executor | rules over evidence, no model | julia1 or jev `choice` |
| Harnesses | Claude Code, Codex (Copilot CLI, not hosted) | Claude Code with function hooks (`prompt.submit`, `tool.describe`), early access; elsewhere unknown |

Order: TE narrows the set at launch, the chooser ranks inside what is left. TE's per-repository
usage evidence (`tool_usage`) is the chooser's prior; since 2026-11-01 the chooser is the same
configurable selector as the model router (routing note, section 5.1).

### 7.1 Tool pruning on OpenCode (decided by the user, 2026-11-01)

TE prunes nothing on OpenCode: its evidence stage runs there, but it has no catalog and no
writer. Shinatama supplies both, keeping TE as the evidence source (my calls on the shape):

1. **The catalog.** TE's proposal is "catalog minus used minus protected" (`adapter.py:126-141`,
   `fold.py:165-185`); on OpenCode the catalog is only `hosts/opencode/catalog.json` under the
   state root (`adapter.py:92-100`, written by a `refresh-catalog` probe that exists for Claude and
   Copilot only) plus the tools the transcript used, so without that file nothing is ever
   "never used". Shinatama writes that file per box from what it already knows: the instance
   set's built-in tools and the tools of its MCP servers, as OpenCode names them
   (`bash`, `read`, `edit`, `write`, `patch`, `glob`, `grep`, `list`, `task`, `question`, `skill`,
   `todowrite`, `webfetch`, `websearch`, `<server>_<tool>`). The format is `{tool: description}`;
   it is TE's internal file, so the write is pinned to the commit and re-checked on update.
2. **The protected set, in OpenCode's spelling.** `PROTECTED_TOOLS` and `INTERACTIVE_PROTECTED`
   hold Claude, Codex and Copilot names; OpenCode's `read`, `write`, `patch`, `task`, `question`,
   `skill`, `todowrite` are not among them. Shinatama passes them in `toolprune_protect`, with
   `webfetch`, `websearch` and BillionContext's tools under the name OpenCode gives them
   (`<server>_<tool>`, so `bili_*`, to be verified against the server name the plugin registers).
3. **The writer.** TE's proposal (`untool.json`, `proposal`) becomes, for that instance only, a
   `permissions` deny rule per tool in the instance's `OPENCODE_CONFIG`: a deny removes the tool
   from the request, MCP tools included (harness-tools.md, OpenCode section: deny-all removes
   `shell`, `edit`, `subagent`, `skill`, `execute` and `<server>_<tool>`). Nothing is written in
   the repository or the box's config dir; the rule lives in the per-launch file Shinatama
   already writes (functional-design 5.14). Idle wait, `propose` mode and the never-applied
   patterns (`bili_*`) are the same as on Claude Code (section 6.3).
4. **Evidence quality.** TE's OpenCode reader counts tool calls from the plugin's own export
   (`hosts.py:355-392`, `trace/opencode.py`); on 2.x the export is built from events, so the
   prompt count and the usage depend on the events of 2.2.5 firing as on 2.2.0 (TE5 covers the
   turn-closing event; TE12 covers the export: after two sessions, `untool.json` holds a
   non-empty `used_tools` and the catalog Shinatama wrote).

## 8. julia1's part

julia1 is a jev-like selector model, of the same kind as gutsy and the OpenJev models, run locally
on the CPU behind one sidecar per box that answers the System One wire format (token-savings 6,
ADR-0037). On this side it has one job: the loop guard's judge, "is
this turn stuck?", a `noul` question acted on at TokenEconomizer's *c* = 0.7. Until
TokenEconomizer offers an optimizer endpoint beside `runner_for` (an upstream request, not a
fork), the judge stays a side call through the agent's own CLI. The redirect text is written by a
fixed template or stays on the side call; julia1 does not generate text. The same model is the
router's default selector in the companion note (its section 6). What julia1 is, and which model
backs it, stays with the other project (token-savings 8.4).

## 9. What must be verified before anything is claimed

| ID | Check | Pass |
| --- | --- | --- |
| TE1 | TE's Claude Code plugin on 2.1.316 in a box, via `--plugin-dir` | `status` works; hooks fire; nothing is written in the repository (in `propose` mode `.git/info/exclude` is never touched either) or outside the box, temp directories apart |
| TE2 | Prompt cache across a session whose launch carries TE's deny list, with BillionContext on | Cache hit rate unchanged inside the session; the change shows only at the next session |
| TE3 | Hook latency per tool call on Windows (Python start) with four agents running | Recorded; threshold set before enabling |
| TE4 | Loop guard side call inside a box: tokens of one judge call with Daodan preinstalled (H5 confirmed: the call runs on the box user settings), `CLAUDE_CONFIG_DIR=<box> claude -p "Reply OK" --output-format stream-json --verbose --tools "" --strict-mcp-config --mcp-config <empty> --max-turns 1 --model haiku`, reading `init` and `usage` | Recorded (TE's 14k figure is unreachable by design in such a box); threshold set before enabling the judge on Claude Code |
| TE5 | OpenCode 2.2.5: does `session.execution.succeeded` fire, so that a turn closes | **Passed 2026-11-01** (record `docs/verification/2026-11-01-TE5-opencode-turn-close.md`): a one-tool-call turn closed with `resolved_proxy: true`, so review H3's trigger is absent on 2.2.5. Side findings: a zero-tool turn is never green (`turn.py:260`); the plugin loaded through the `plugins` array with a `file:///` URL stalled the standalone server once, so Shinatama loads it from `<OPENCODE_CONFIG_DIR>/plugins/` through a re-export wrapper until that is understood |
| TE6 | Codex 0.164.0 and Pi 1.4.0 on Windows | `status` works; loop guard fires on a provoked loop |
| TE7 | TE's ledger with BillionContext on | Its per-session cost matches the provider's billed usage for the same session |
| TE8 | A judge side call as BillionContext sees it | Recorded whether it shows as a session of its own; if so, it is labelled, not counted as user work |
| TE9 | OpenCode: how often a redirect lands on a result at or above CCR's threshold (the loss itself is settled, review H2); Claude Code: the `additionalContext` block is untouched | Rate recorded; on Claude Code the redirect reaches the model in the next request |
| TE10 | Micro-skills facts at session start with `promptRewrite` on | The facts reach the model unchanged and the appended note does not restate them |
| TE12 | OpenCode: two sessions with the catalog Shinatama wrote under `hosts/opencode/catalog.json` | `untool.json` holds `used_tools` and a `proposal` that excludes every protected OpenCode name and `bili_*`; the deny rules in the next launch's `OPENCODE_CONFIG` remove those tools from the request |
| TE11 | Claude Code with CCR on: one failing command with output above 4,000 tokens, then `mcp__bili__acp_retrieve` on it | `turn.st.repeated_fail_sigs` in `sessions/<id>.json` does not rise on the retrieve (review H6); if it does, the exemption is requested upstream before the loop guard is enabled |

## 10. Evidence

- Read, not run: TokenEconomizer at `c53a4a6`, files listed in section 3.
- Repository: token-savings (sections 4-9, especially 7.2 and the BillionContext baseline),
  functional-design 5.14, harness-tools.md, ADR-0037, ADR-0038, verification register Q11.
- Installed versions observed on 2026-11-01 with each CLI's `--version`.
- Science Mode checkpoint: reuse; the verdict rests on repository reading (TokenEconomizer's
  source and documentation, Shinatama's specs); no new web research on this side.
