# FRD-0012: Companion pane, Aided Decision cards and web grounding

| | |
|---|---|
| Version | 1 |
| Status | Accepted 2026-11-01 (this Markdown text) |
| Date | 2026-11-01 |
| Sources | specs/companion-explainer (revision 4.1 draft) 1 to 10; harness-tools 1 to 3; technical-design 5.5 (`ask_headless`), 7.6, 8.1, 8.3, 8.4 |
| Governing ADRs | ADR-0002 v3, ADR-0003 v2, ADR-0041, ADR-0042, ADR-0043 |
| Register sections | companion spec 10.2 rows 1 to 19 (`CS-n`; ids still to be assigned in the register), CL11, CX9, OC7, PI8, HM7, DS7, R2, U3 |
| Related records | FRD-0001 (a second native chat surface), FRD-0002 (scratch home, per-instance set), FRD-0006 (incremental reader), FRD-0010 (waiting events with payload, overview rows), FRD-0014 (companion record and ledger tables) |

## 1. Purpose and scope

A **companion** is a read-only session opened beside a working agent instance (the **worker**)
that explains what the worker is doing in the project, or teaches it, and prepares the user to
answer the worker's proposals. The worker keeps working untouched. This record states the feed,
the two card flows (Learning and Aided Decision), the preset-only input, the per-person ledger,
the card block, the three read-only mechanisms, harness and cost, lifecycle, web grounding and
the release order.

Vocabulary (spec 3): **card** (one companion answer), **preset** (a fixed action: basics, why
here, alternatives, example, simpler, deeper, expand, sources, Explain this), **ledger** (what
the companion explained, per person), **request chip** (a control for a pending request of the
worker), **web turn** (the one kind of turn that may search), **brief** (the fixed instructions
sent first).

## 2. Requirements

### 2.1 What the companion is

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | A companion is a separate, read-only session beside a worker, fed by Shinatama from the worker's canonical transcript; it is neither a native fork of the worker nor an output style inside the worker. Its harness need not be the worker's. | U 2026-11-01 ("ok a") | V1 (R1) | CS-1 | 0002 v3 |
| R02 | Attaching a companion leaves the worker's bytes, context and behaviour as without it: on the same recorded session the worker's input tokens per turn are equal with and without a companion, Aided Decision turns included, and the worker's transcript holds no text the feature added. | S | V1 (R1) | CS-6 | 0002 v3, 0003 v2 |
| R03 | The companion opens beside any agent tab in a split of the native tiled workspace, from a key (default `^E`, placeholder until U3) or the tab menu, with a header showing harness, model, the read-only badge, the feed state (live, partial, paused, stateless) and today's cost. It is a second native chat surface, which functional-design 5.1 and 5.16 must declare. | A | V1 (R1) | U3 | 0002 v3 |
| R04 | Two flows on one machinery: **Learning** cards, where the tutor explains to the user and to nobody else, and **Aided Decision** cards, which prepare the user to answer the worker. Which kind a turn gets is decided by its origin (a pending request, the worker's final message, the user's expand preset), not by a mode switch. | U 2026-11-01 (the two flows, from the pasted design); A (mechanics) | V1 | CS-14 | 0041 |
| R05 | The expansion of a worker's proposal, problem, finding or request is always written in the companion pane, never in the main chat; the user answers the worker in the main chat, where the worker waits. The worker is never asked to expand its own messages. | U 2026-11-01 | V1 | CS-6 | 0003 v2 |
| R06 | A companion explains only sessions running in Shinatama, not a closed thread or an old session from the session browser; it does not replace the overview's one-line titles; a card appears whole, not streamed; it never writes code into the project and exercises live in the pane only. | S; A (no streaming) | V1 | — | 0002 v3 |

### 2.2 The feed

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R07 | Shinatama builds the feed; the companion never reads the worker's native store. Sources: the canonical transcript through the Mukade reader at the declared pin, the event feed, the git state of the worker's worktree, and the linked Daodan task and phase where one exists. | A | V1 (R2) | CS-3 | 0002 v3 |
| R08 | One delta per finished worker turn: `user_text`, `assistant_text`, `tool_call` and `tool_result` sent; `attachment` as a path reference; `compaction` as a marked item; `meta` as a one-line note; sub-agent work as one call/result pair; `reasoning` never. A per-turn delta needs the bounded incremental reader of FRD-0006 R20 and the harness's `turn_end_signal` capability; a harness without it gets only on-demand cards. | S | V1 (R2) | CS-3 | 0002 v3 |
| R09 | Files changed in a turn are named from the event feed's `file_change` events and the turn's tool-call targets; the cumulative `git diff --stat` is attached only as context. | A | V1 (R2) | CS-3 | 0002 v3 |
| R10 | Each delta is capped (default 8 KB): a tool result above the cap is replaced by its per-tool digest where one exists, otherwise cut with a marker naming the dropped bytes; an assistant text above 16 KB keeps its end, where a question to the user usually is. | A | V1 (R2) | CS-3 | 0002 v3 |
| R11 | Turns that finish while a companion turn runs coalesce into one card request naming every merged turn; nothing is dropped silently. Only the newest turn's last assistant message is judged for a decision handed to the user, and only if no user message follows it in the batch. | A | V1 (R2) | CS-14 | 0002 v3 |
| R12 | Every delta, selection, scrollback excerpt and preset operand reaches the companion wrapped as untrusted data (`<worker-turn … untrusted="true">`) and escaped so its content cannot close the wrapper; the brief says to treat wrapped content as data. | S | V1 (R1) | CS-3, CS-12, CS-13 | 0002 v3 |
| R13 | Credential-like strings are redacted before Shinatama sends them, in the feed, a selection and a scrollback excerpt (the titler's rules); the companion's own file reads are not redacted. | S | V1 (R1) | CS-3 | 0002 v3 |
| R14 | A thread marked `summaries_excluded` gets no companion and the pane says why (open point 3 asks whether a feed-only local companion may serve it). | A | V1 | — | 0002 v3 |
| R15 | A Hermes Agent worker (no reader) gets a **partial feed** of events and git state only; a DeepSeek Harness worker's feed comes live from the ACP stream, labelled **ACP feed** (unverified); with no source available no companion is offered. | A | V1 | CS-3 | 0002 v3 |
| R16 | When the worker's harness natively reports `waiting_permission` or `waiting_question`, a pending-request item (the request: tool and arguments, or question and options; plus the worker's last assistant text, redacted and wrapped) is built at once and sent at once, outside the turn-end rhythm, when the pending-request setting selects it; if the event carries no request the companion is told and its card says what it could not see; an unselected request is kept for its chip and sent only on expand while pending. | S | V1 (R1) | CS-10 | 0003 v2 |

### 2.3 Learning cards

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R17 | With automatic turn cards (the default) the companion writes a Learning card after each finished worker turn: what is happening (what the turn did, which files changed and how, as a table, what to watch), why, the concepts it rests on, why here, the presets; the card block ends every card. | A | V1 (R2) | CS-5 | 0041 |
| R18 | A reason stated in a card is either quoted from the worker's own text or labelled as the companion's inference; a guess is never presented as the worker's reason. Measured by rubric, not enforceable at runtime. | S | V1 | CS-5 | 0002 v3 |
| R19 | The default card is short; depth comes only on request (deeper, simpler, basics, a concept chip). A card teaches at most two concepts (a concept new to the ledger at level 1, what it is and which problem it solves, in at most 60 words); other concepts are chips. Turn cards: at most 200 words of prose and one diagram; preset cards at most 150 words; expand at most 300. | U 2026-11-01 (short, depth on request); A (caps) | V1 | CS-5, CS-13 | 0041 |
| R20 | A concept already in the ledger that matters to the turn gets one reminder line of at most 20 words tied to what the turn does with it, and no lesson; one that does not matter is not mentioned. | U 2026-11-01 | V1 (R3) | CS-15 | 0042 |
| R21 | Learning cards can be switched to on demand: deltas accumulate and are sent with the next preset action; then no finished turn is judged for a decision, so no Aided Decision card comes from a finished turn. | A | V1 | — | 0041 |
| R22 | Quiz first is an opt-in for Learning cards and for Aided Decision cards whose risk is not only "changes only the worktree": one headless turn; the question with up to four options plus "not sure" is shown first and the card hidden until the pick or a skip; the pick counts as no turn; Shinatama then states which option the user picked and which the companion expected, computed without a second turn. | S | V1 (R3) | CS-17 | 0041 |

### 2.4 Aided Decision cards

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R23 | An Aided Decision card is raised by one of three origins: (a) a pending permission or question the harness reports natively; (b) a finished turn whose last assistant message, read in the structured transcript, asks the user to decide, proposes a decision or reports a problem or finding (the companion's judgement, declared in the card block, never an inference from screen text); (c) the user's expand preset on a card, a request chip, an Explain this result or the worker's last message. | U 2026-11-01 (need); A (origins) | V1 (R1 for a and c; R2 for b) | CS-10, CS-14 | 0003 v2 |
| R24 | The card contains, in order: what the proposal really says in plain words for the user's level and what it took for granted; what a yes and a no would literally do (files, commands, ports, services, accounts, money) and whether it can be undone; one realistic example in this project; the options; why the worker asks; what could make it a bad idea and what the worker did not mention; at most three things to check first; a risk marker from a fixed set (changes only the worktree; changes things outside it; needs or exposes credentials; costs money; opens a port or service; hard to undo). Each part at most 40 words, the whole at most 300 with the example and "why" collapsed. | A | V1 | CS-11 | 0003 v2 |
| R25 | The card is neutral: it never recommends approving, declining or any option, never ranks or pre-selects, never argues for the worker's choice; its template has no verdict field; the options list names the answers the user can give in the main chat, approve and decline among them, each with one line of consequence. | S | V1 | CS-11 | 0003 v2 |
| R26 | Every claim about a tool or concept carries a source label: **from the repository** with `path:line`, **from the web (read)** with the URL of a page read in that turn, **from the web (search result only)** with URL and snippet, or **general knowledge, not checked**; a version-specific claim the companion cannot ground says so. | S | V1 | CS-11 | 0003 v2, 0043 |
| R27 | An Aided Decision card carries at least one line on what each tool or concept it names is, even when the concept is in the ledger; it teaches nothing and writes nothing to the ledger (its `taught` list is empty; its concepts are chips). | U 2026-11-01 | V1 | CS-15 | 0042 |
| R28 | The companion cannot answer, hold, delay or alter a worker's request; no state of the companion (running, queued, capped, paused, crashed) delays or changes the worker's approval prompt, which is answerable the moment it appears with the same latency as without a companion. | S | V1 | CS-10 | 0003 v2 |
| R29 | A new card never takes keyboard focus from, scrolls or resizes the worker's pane; it shows as a badge (**explanation ready**) in the pane header, on the worker's tab and in the overview. The pane has no button that sends text to the worker or replies for the user. | S | V1 | CS-13 | 0003 v2 |
| R30 | The pending-request setting is smart (default: every request that is not a read inside the worktree, and every question offering options), all, or none. A harness with no native waiting event shows **approvals not detected** and raises no card from a request; origins (b) and (c), presets and Explain this still work. | A | V1 | CS-10 | 0003 v2 |
| R31 | While a request is pending the pane shows a request chip (the request in one line, an expand button) whatever the setting and whether or not a card exists; it disappears when the request is answered. | S | V1 | CS-13 | 0003 v2 |
| R32 | A misjudged finished turn is recoverable with expand; a wrong-kind card costs the same as the right one; origin (b) accuracy is measured on about 30 recorded last messages and the target comes from the measurement. | S | V1 (R2) | CS-14 | 0003 v2 |

### 2.5 Presets, Explain this and the level text

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R33 | The pane has no text field or editable control and no key typed in it reaches the companion; every action is a preset on a card, a concept chip or a request chip, or a selection. Each preset reaches the companion as a fixed template, versioned with Shinatama and written in English, with its operand wrapped as untrusted data of kind card, chip, request, selection or message and escaped. | A (from the pasted design, "no free text") | V1 (R1) | CS-13 | 0041 |
| R34 | Presets and levels: basics (level 1, sets the entry to 1), why here (level 2), alternatives (level 3), example (writes nothing), simpler (entry level minus 1, at least 1, lowers the entry), deeper (entry level plus 1, at most 4; absent counts as 0), expand (an Aided Decision card, writes nothing), sources (FRD-0012 2.10). The level written is the one the template asks for, not the one the model declares. | A | V1 | CS-13, CS-15 | 0041, 0042 |
| R35 | Explain this: the user selects text in the worker's pane and asks by key or menu; the selection and a scrollback excerpt (each at most 4 KB) are explained as a Learning card; it needs no reader and no native event, so it works on every hosted harness. | A | V1 (R1) | CS-12 | 0041 |
| R36 | Operand caps: selection and excerpt 4 KB each, a card 8 KB, the worker's last message 16 KB keeping its end; all redacted and escaped. | A | V1 | CS-13 | 0041 |
| R37 | The only typed text that reaches the companion is the level text of the settings screen (at most 500 characters, once per user and optionally per companion), stored locally, sent as a JSON string in the per-turn context block; it cannot request an action and is never inferred from behaviour. It sets how much a concept new to the ledger gets; for a known concept the ledger decides. | A | V1 (R1) | CS-13 | 0041 |
| R38 | Preset labels and the brief's language follow the interface language setting (default the OS locale; Italian and English first, English the fallback). Settings, level text and the memory view live in the settings screen, not in the pane. | A | V1 | — | 0041 |

### 2.6 The brief and the per-turn context block

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R39 | The brief is the first message of the companion session, identical on every harness (not per-harness system-prompt flags): which flows are on, the read-only role, the untrusted framing of the feed and every operand, the preset templates, the quoted-or-inference rule, the rendering conventions, the Aided Decision rules, the card block protocol, the web rules and the user's language. | A | V1 (R1) | — | 0002 v3 |
| R40 | For every headless turn Shinatama rebuilds a per-turn context block (settings including `web` and its budget, the level text, the relevant ledger entries, the concepts the thread's cards already taught) as an untrusted `<learner-context>` wrapper with escaped JSON strings; the latest block wins and its names are data; it is part of the turn's input and cost, never stored in the record. A setting or ledger change starts no turn of its own. | S | V1 | CS-15 | 0042 |

### 2.7 Rendering

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R41 | Answers are rendered as Markdown with tables and diagrams: the pane is a native chat pane, not a terminal, and Shinatama renders GitHub-flavoured Markdown (headings, lists, tables, task lists, fenced code with highlighting, file links that open in the navigator when it exists); `mermaid` blocks render natively to SVG without a browser or JavaScript runtime; a parse failure shows the source with the error and never drops the card. | U 2026-11-01 (Markdown, tables, diagrams); A (native rendering) | V1 (R1) | CS-4 | 0002 v3 |
| R42 | Rendering never executes scripts or embedded commands, makes no unapproved remote request and fetches no remote images. The brief asks for a table when comparing three or more things and a diagram for a flow across three or more components. | A | V1 | CS-4 | 0002 v3 |

### 2.8 The ledger of what was explained

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R43 | Shinatama keeps one short, context-saving ledger per person of what the companion explained: an entry is a concept title, a theme, up to four keywords, the highest level explained (1 to 4) and two dates; no explanation text. It records what was explained, not what was understood: no "got it" marks, no inference from behaviour. | U 2026-11-01 (two pastes) | V1 (R3) | CS-15 | 0042 |
| R44 | The companion names what a card taught in its card block; only Shinatama's code validates and stores it, once the card has been in view (pane visible, card at least partly on screen for 2 seconds, or a preset pressed on it); a card produced while the pane is hidden writes nothing until then. The model has no write channel to the store, and a turn card never lowers an entry. | U 2026-11-01 (written as it explains); A (in-view rule) | V1 (R3) | CS-15 | 0042 |
| R45 | The ledger is per person, not per project: stored in the per-user database outside every repository, shared across projects, companions and harnesses; project-specific concepts are not stored. | U 2026-11-01 (per person is the user's hypothesis) | V1 (R3) | CS-15 | 0042 |
| R46 | The memory view lists entries with dates and lets the user lower, delete or forget them; a deleted concept is taught as new; a setting turns the ledger off (nothing written or sent, no notice, every concept new). Changes take effect with the next turn. | U 2026-11-01 | V1 (R3) | CS-15 | 0042 |
| R47 | The ledger is sent only to the companion's provider, in the per-turn context block, grouped by theme in one compact line per theme, relevance first (entries matching a word of the delta or operand, case-folded, up to 30, then the most recent, at most 40 entries and 2 KB, with a count of what was left out); never in a message to the worker, a repository or an export. | U 2026-11-01 | V1 (R3) | CS-15, CS-16 | 0042 |
| R48 | Before the first entry is written a one-time notice says the tutor keeps a short list on this computer and sends the relevant entries to the companion's provider, with View and Turn off; later entries show none. | U 2026-11-01 | V1 (R3) | CS-15 | 0042 |
| R49 | A read-only database (newer than Shinatama) lets the companion open but writes nothing (no entry, record, cost log or setting), turns automatic cards off and says so; an unreadable ledger table is treated as empty with a notice and never overwritten. | U 2026-11-01 | V1 (R3) | CS-15 | 0042 |
| R50 | Ledger entries do not age in this revision; the memory view shows the dates (open point 7). After two weeks the user checks five random entries to inform ageing and the reminder rule (a user check, not a feature). | S | V1 (R3) | CS-18 | 0042 |

### 2.9 The card block

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R51 | Every answer ends with one fenced `shinatama-card` block holding one JSON object that Shinatama parses and removes before rendering, ignoring unknown keys; when several fences exist only the last counts, and only as the last element, the others shown as code. | A | V1 (R1) | CS-16 | 0041, 0042 |
| R52 | Limits: block at most 4 KB; at most 2 `taught` items and 4 chips; a name at most 60 characters, a theme 30, up to 4 keywords of 24, measured after Unicode NFKC and trimming and made of letters, digits, spaces and `. - _ / + #`; a level 1 to 4; risk a subset of the six markers; a quiz with a question of 200 characters, at most 4 options of 120 and an expected index within the options; `sources` at most 3 items, each an `https` URL of at most 300 characters, a title of 100 and a read flag. An item outside the limits is dropped, not the block; invalid JSON or no block means no chips, no entries, no quiz, the kind Shinatama asked for (Learning for a finished turn), and the card still renders. | A | V1 | CS-16 | 0042 |
| R53 | A concept's identity is its name case-folded with spaces collapsed; no aliases. Shinatama sets the kind of a card it asked for; only for a finished-turn card does the companion declare Learning or Aided Decision. Every name, theme and keyword the model writes is stored and written back only as a JSON string inside a data wrapper, never as raw text, from the first release. | A | V1 (R1) | CS-16 | 0042 |

### 2.10 Web grounding

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R54 | When the user allows it, the companion may search the web through its own harness's web search tool to ground a definition, check a tool's documentation and link further reading, in Learning and Aided Decision cards. Shinatama builds no search or fetch of its own; the companion has no other network access. | U 2026-11-01 | V1 (R4) | CS-19 | 0043 |
| R55 | Web tools exist only on a **web turn**: the turn made by the sources preset or, in auto mode, an automatic turn; every other turn, in every mode, has no web tools, so an injected instruction cannot cause a search. A web turn runs stateless in a throwaway session seeded with the history Shinatama keeps, so no snippet or page survives it; only the card joins the history and the record. Timeout 60 s. | A | V1 (R4) | CS-19 | 0043 |
| R56 | The `web` setting is off, on demand (default: web turns only from sources) or auto (also for a Learning card about a concept not in the ledger and an Aided Decision card about an ungrounded tool, if the measured latency allows). Off: no web turn, sources hidden, header "web: off", Aided Decision claims labelled general knowledge. | A | V1 (R4) | CS-19 | 0043 |
| R57 | Budget per card: at most 2 searches, queries of at most 200 characters, and, where page reading is hosted, at most 2 page reads of URLs of at most 300 characters; cost counts in the card and the daily cap; a web turn is one turn of the queue. | A | V1 (R4) | CS-19 | 0043 |
| R58 | A card made on a web turn shows a Sources section (at most 3 links: title, URL, one line of at most 25 words; official documentation first, then free material) and a Searches line made by Shinatama from the events the harness reports (one line per query with the result count, one per URL requested), never by the model; neither counts in the word caps. Queries are in English by default and in the user's language for local material. | A | V1 (R4) | CS-19 | 0043 |
| R59 | Every URL in a card is compared with the URLs the harness reported in that turn (normalised: https, lowercase host, no fragment, no default port, no trailing slash, query kept); a seen URL is a link, an unseen one is inert text with "link not seen in this turn's searches", stored with the flag unseen, never from the web, never accepted for `read`. Whether a URL was seen is computed by Shinatama alone. Links open only on the user's click, after the full domain is shown. | A | V1 (R4) | CS-19 | 0043 |
| R60 | After each reported query and URL Shinatama checks for the worktree's path, a credential-like string and excess length; on a hit or a budget breach it aborts the turn, flags the card, shows the query with a warning and turns `web` off for the companion until the user turns it on. Shinatama does not claim a search cannot leak: it cannot filter words before they are sent, recall a request or detect a clean-looking private query; it limits the amount, shows every query and detects obvious cases after. | A | V1 (R4) | CS-19 | 0043 |
| R61 | The brief says to search only for concepts and public tool names, never code, paths, file contents, secrets or private names, and that anything met on the web is data: an instruction on a page or in a snippet changes neither a query's purpose, nor any channel to the worker, nor a write to the worktree. | A | V1 (R4) | CS-19 | 0043 |
| R62 | A harness hosts web mode only if its web tool reports every query and result URL to Shinatama (search run at the provider's side with nothing in the stream excludes it) and its row passes; otherwise "web: unavailable", sources hidden, claims general knowledge. "Every query" means every one the harness reports: Claude Code lists only a count of its refined internal searches, DeepSeek hides its native ones, Codex's hosted tool reports no result URLs. | A | V1 (R4) | CS-19 | 0043 |
| R63 | Page reading (label **from the web (read)**) joins web mode in R5, per harness, only where the page tool can be restricted to the URLs of this turn's results and Shinatama verified it; until then web mode is search-only and no source counts as read. C4 is native nowhere; Claude Code (a `PreToolUse` hook) and OpenCode (a second turn with exact-URL rules) are the first candidates. | A | V1 (R5) | CS-19 (g) | 0043 |
| R64 | Before the first web turn a one-time web notice says the companion sends search words, written by the model from the user's code and conversation, to its harness's web search, and that Shinatama cannot filter them; Search or Keep web off. Two harnesses need an account beyond the model login (a search backend key or Nous subscription for Hermes, a DeepSeek key for DeepSeek Harness). | A | V1 (R4) | CS-19 | 0043 |
| R65 | The companion keeps the links it cited and the queries it made, never pages; the ledger stores no URL. | S | V1 (R4) | — | 0043 |

### 2.11 Read-only, by independent mechanisms

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R66 | No byte of the worker's worktree, ignored paths and `.git` included, nor the project-local state of the companion's own harness, is created, changed or deleted by the companion or its harness. Three independent mechanisms enforce it: the harness's own policy (A), OS confinement (B), and Shinatama withholding every channel to the worker (C). | U 2026-10-31 ("session read only"); A (layering) | V1 | CS-1, CS-2 | 0002 v3 |
| R67 | A: each turn runs through the adapter's headless route under a read-only policy: reads and searches inside the worktree allowed; writes, shell commands, MCP servers that can write and every network access except the harness's web tool on a web turn denied. The one exception is Codex, which has no read tool and reads through its shell: the read-only sandbox, backed by B, keeps it from writing. Candidates per harness are in spec 6 A and harness-tools; the existing answer-only rows prove something else, so new rows are needed. | S | V1 (R1) | CS-1 | 0002 v3 |
| R68 | When a harness cannot resume the companion's session under the policy, Shinatama sends the history with each turn (stateless mode, more tokens, shown in the header). | S | V1 | CS-1 | 0002 v3 |
| R69 | B: the companion's whole process tree can write only its scratch directory, which is also its harness home; it can read but not write the worktree, `.git` and ignored paths, the harness's program and runtime, the harness's credential files only as a read-only binding of their original path enforced by the OS (Shinatama never opens, copies or stores them; a platform that cannot bind runs the companion as the opt-in preview), and read-only copies of the skills of the companion's set; it cannot read the worker's agent home, the system home, other projects, Shinatama's data directory or anything else. Mechanisms are per OS (Landlock or bubblewrap, a macOS sandbox profile, a restricted token) and the allow-list is recorded by row 2. Outside a web turn every other outbound connection is closed where the platform allows it (unverified). | S | V1 (per platform) | CS-2 | 0002 v3 |
| R70 | A harness that must write to its credentials to refresh them cannot host a companion until its row shows otherwise; repository project-scope hooks and settings are switched off for the companion's harness wherever it has a switch, and its set carries no hook or skill that writes state. | S | V1 | CS-2 | 0002 v3 |
| R71 | Where confinement is not implemented for a platform the companion is an opt-in preview with the badge **read-only: harness policy only**, whose first-use notice says its reads are not confined; **read-only (confined)** is reserved for the confined case. A harness is offered as a host only after its row for A passes; the plain badge also needs the row for B on that platform. | S | V1 | CS-1, CS-2 | 0002 v3 |
| R72 | C: the companion has no PTY input path to the worker, no approval channel, no `shinatama mcp` server at all (none of the four tools) and no MCP server that writes; its harness home is its own scratch directory, never the worker's agent home, with a per-instance set of read-only tools and skills; nothing it produces reaches the worker's PTY, inbox, approval prompts or context, or changes the worker's home, hooks, settings or session store. | S | V1 (R1) | CS-9 | 0002 v3 |
| R73 | A file read while the worker writes can be stale; the companion says which version it read when it matters. | S | V1 | — | 0002 v3 |

### 2.12 Harness, model and cost

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R74 | Default: the worker's harness with the lowest-cost model of the same provider that passes a quality gate, chosen from the catalog by capability and price, never by a name in the code. Any other installed harness and model may be chosen per companion. | A (default); U 2026-11-01 (choice) | V1 (R1) | CS-5 | 0002 v3 |
| R75 | Before any feed goes to a companion whose provider differs from the worker's, a first-use warning (once per project and provider) says it can read any worktree file unredacted, receives the relevant ledger entries and has its answers produced by that provider; Continue or Use the worker's provider; declining opens nothing. | A | V1 | — | 0002 v3 |
| R76 | A feed-only companion on the local model shared with the titler and julia1 (no tools, no native session, no file reads) may come later, only after a row shows its context holds the brief, a delta and enough history and its quality passes the rubric; it does not serve excluded threads. | A | V1 (R3) | CS-5 | 0002 v3 |
| R77 | Tokens and cost of each card are logged in the companion's own record, not `summary_log`, and shown in the header beside the worker's cost for the same period (dollars where reported, else tokens); a close-mode hop keeps the record so cost continues. | S | V1 (R1) | — | 0002 v3 |
| R78 | A daily cost cap per user (default $2.00, open point 1; separate from the titler's) pauses every automatic card; presets still work and show their cost; a pending request keeps its chip reading "explanation paused: cap reached. Explain anyway" with expand; a pause key stops the feed. | S | V1 | — | 0002 v3 |
| R79 | Whether a headless companion turn goes through the BillionContext sidecar, and what its history costs after 40 cards, is a register row; until then no compression of the companion's history is claimed. | S | V1 | CS-7 | 0002 v3 |

### 2.13 Lifecycle and queue

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R80 | At most one headless turn is in flight per companion session, across windows too. A preset pressed during a card turn is queued and shown as queued; order: pending request, then queued preset, then queued automatic card; no automatic turn card starts while a request is pending; a running turn is not cancelled by a pending request (the added latency is measured by row 10). | S | V1 (R1) | CS-8, CS-10 | 0041, 0002 v3 |
| R81 | A companion attaches to the worker's thread, not its process: after a close-mode hop it stays open on its own harness and its feed continues from the new worker's transcript with a card marking the hop (header **partial feed** if the new harness has no reader); after a fork-mode hop it stays with the source worker and a card offers a companion for the new worker; never two workers. Those cards are Learning cards with an empty `taught` list, written by Shinatama, costing nothing. | S | V1 (R2) | — | 0002 v3 |
| R82 | Closing the pane ends the session and deletes the native session the harness kept in the scratch home. Shinatama keeps the record (cards and preset actions with their queries and sources, not deltas or context blocks; harness and model; concepts explained; tokens and cost per card) as a thread of kind `companion`, linked to the worker thread by its own column, not the hop lineage; searchable through Shinatama's own search; never a hop source; never shown as a hop. | S | V1 (R1) | — | 0002 v3 |
| R83 | One companion per worker thread; opening again focuses it; after Close a new session loads from the record only the list of concepts already explained for that thread. The overview shows a companion as a child row of its worker, never a collaborator or collision source, with cost from its record; it has no `instance` row of its own. | A | V1 | — | 0002 v3 |

### 2.14 Release order and the harness tools

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R84 | The companion pane comes after the terminal-first milestone and the Markdown panel. R1: one harness end to end (Claude Code first) with policy A, the feed, presets, Explain this, the card block, Aided Decision cards from origins (a) and (c) with the request chip, settings, level text and the per-turn context block; as the opt-in preview until confinement exists. R2: automatic Learning cards, origin (b) and coalescing, behind the turn-end signal and the incremental reader. R3: the ledger with view, setting and notice, quiz first, the feed-only backend, export. R4: web grounding, search-only, harness by harness. R5: page reading per harness. Confinement per platform and the other harnesses one at a time, each behind its rows. | A | V1 | — | 0040 |
| R85 | Per harness, web mode needs five conditions (harness-tools 1): C1 the search tool on for one turn and absent from the others; C2 every query and result URL in the headless stream; C3 a turn limited to read tools plus search; C4 (R5) page reading limited to the turn's results; C5 read-only with resume or stateless. No harness is eligible today without a probe; Pi has no built-in web tool and is not eligible now. | A | V1 (R4) | CS-19 | 0043 |
| R86 | Codex's hosted search reports no result URLs, so with it every link would show unseen: either its standalone `web.run` tool is used once stable or Codex hosts no web mode. | A | V1 (R4) | CS-19 | 0043 |
| R87 | The per-harness facts of technical-design 5.7 and the answer-only rows were observed on older versions than the installed ones (Claude Code 2.1.316 against 2.1.311, OpenCode 2.2.5 against 2.1.4, Pi 1.4.0 against 1.2.0) and are re-run on the installed versions before any support claim. | S | X | E1 | 0010 |

### 2.15 Changes this feature requires elsewhere

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R88 | technical-design 4.2: thread kind `companion` with a `companion_of_thread_id` column; a table for the companion's record; search and lineage views that exclude companions as hops. | P | V1 (R1) | — | 0009 |
| R89 | technical-design 4.1, 4.2: a `learning_memory` table in the per-user database (key, name, theme, keywords, level, explained-at, updated-at) with read, lower, delete and forget-all; the read-only and unreadable behaviours of R49; settings and the notice flag in `setting(key, value)`. | P | V1 (R3) | — | 0009, 0042 |
| R90 | technical-design 4.7: the privacy statement extended to a companion on another provider, the ledger's titles, themes, keywords and levels, and the search words; `shinatama forget <thread>` also removes the companion record, and the ledger has its own forget-all. | P | V1 | — | 0009 |
| R91 | technical-design 5.5: `ask_headless` gains a read-only-with-reads policy value, a web policy value (none, search only, search and reading), the stateless mode, and a return of the tool events the harness reports (queries, result URLs, URLs requested). | P | V1 | — | 0011 |
| R92 | technical-design 7.6: escaping of the wrapper's content. technical-design 8.1 and 8.3: the waiting events carry the pending request where the harness reports it. | P | V1 | CS-10 | 0024, 0027 |
| R93 | functional-design 5.1 and 5.16: the companion pane as a second native chat surface, the Explain this context action, a pane with no text input that takes no focus; functional-design 5.13: the **explanation ready** badge. | P | V1 | — | 0004, 0005 |
| R94 | Resolved 2026-11-01: the scratch home is a Shinatama-owned store per companion, outside the box/system choice and outside invariant I7 (ADR-0012 v2); credentials reach it only as read-only OS bindings (ADR-0031 v2); the companion gets no `shinatama mcp` server and its overview row comes from its own record, not `instance` (ADR-0002 v3). | U 2026-11-01 | V1 | — | 0002 v3, 0012 v2, 0031 v2 |

### 2.16 Verification rows of the specification

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R95 | The nineteen checks of spec 10.2 (policy A per harness; confinement B per platform; the feed; rendering; the quality gate on about 30 recorded turns; context neutrality; sidecar coverage; concurrency; channel isolation; pending-request feed and latency; the Aided Decision rubric on about 20 requests, 8 on unfamiliar infrastructure and 5 plausible but wrong; Explain this; presets, caps and attention; origin (b) classification; the ledger; the card block parser; quiz first; the ledger field check; web grounding (a) to (g)) are given register ids and run before the corresponding release. | A | V1 | CS-1 to CS-19 | 0010 |
| R96 | Every claim in the specification is unverified until its row runs: nothing of the companion has been built or run. | S | — | — | 0010 |

## 3. Open questions (spec 9.5)

1. The daily cost cap (default proposed $2.00).
2. Learning cards outside a running worker (an old thread from the session browser).
3. A feed-only local companion on a `summaries_excluded` thread (would amend technical-design 8.4).
4. Whether Shinatama should require that the card was read before the widget's quick-approve accepts a high-risk request (not planned).
5. Whether Shinatama should later run the web search itself (reopened only if row 19 shows a harness whose queries cannot be seen, or a leak).
6. A typed question box as an opt-in setting later.
7. Whether ledger entries should age.
8. A per-project memory beside the per-person one.

## 4. Acceptance

Spec 9.3 examples EX-001 to EX-033 and the invariants INV-001 to INV-016 of 9.4; the rows of
10.2 by release as 9.3 lists them. Nothing has run.
