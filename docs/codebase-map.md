# Codebase map

Date: 2026-11-01, at commit `b667db5`. Status: descriptive, not a design document; it changes
nothing in the specs. Regenerate it when documents are added, renamed or retired.

Implementation update, 2026-11-02: the workspace now contains product code:
`crates/terminal` owns real PTYs; `crates/projects` persists project-picker settings;
`apps/desktop` contains the borderless Tauri/TypeScript/xterm shell and native bridge.
`docs/implementation/PLAN.md` is the complete delivery map; `progress.json` records
verified increments and the next ready step. `scripts/implementation.py` checks
coverage and dependency/checkpoint consistency. The document inventory below retains
its original 2026-11-01 snapshot where not updated; it is not a current implementation claim.

Built by hand from a full read of the documents. Daodan's `codebase-map` plugin was not run
for this map and no `.codebase-map/` directory was created. Daodan's onboarding check
(`daodan-native-support.md` 4.7) uses that directory as a marker of Daodan state, so a
hand-made copy would give a false answer.

## 1. Layout

```
shinatama/
├── README.md                     entry point: one-paragraph product statement, links, naming, D1 history
├── Cargo.toml                    Rust workspace (edition 2024, resolver 3); the allowed dependency list
├── crates/terminal/              real PTY ownership, bounded I/O and explicit process stop
├── crates/projects/              validated MRU project settings, atomic writes and cooperative writer locking
├── apps/desktop/                 borderless Tauri shell, terminal/picker UI and real-app tests
├── scripts/                      implementation plan validation, rendering and checkpoint tests
├── .gitignore                    .jcode/, .daodan/, target/, verify/*/out/
├── docs/
│   ├── functional-design.md      WHAT and WHY: goals, non-goals, surfaces, the parts (5.1–5.16), adapter requirements
│   ├── technical-design.md       HOW: invariants I1–I7, process model, SQLite, adapters, hop, messaging, state, sidecar
│   ├── browser-integration.md    Moli fast path and explicit Patchright UI (proposed 2026-10-29)
│   ├── mukade-handover.md        integration guide for the Mukade library, pinned at 4124b1f
│   ├── daodan-native-support.md  Daodan task layer inside Shinatama (proposed 2026-10-27)
│   ├── harness-tools.md          web mode and read-only tool allowlists of the six harnesses, for the companion
│   ├── verification-register.md  every unproved claim, one row each (295 rows), with gates G0/G1/G2
│   ├── codebase-map.md           this file
│   ├── specs/
│   │   ├── companion-explainer.md  read-only companion pane (decisions: ADR-0002, ADR-0003)
│   │   └── token-savings.md        decision compaction and the local julia1 executor
│   ├── adr/                      architecture decision records (section 4)
│   ├── frd/                      functional requirements documents, evidence-bound JSON (section 4a)
│   ├── verification/             18 run records, YYYY-MM-DD-<ID>-<topic>.md (17 rows + UNI1 trial)
│   ├── notes/                    two surveys, and the inconsistency worklist of 2026-11-01
│   ├── mockups/                  workspace.html, workspace-lite.html and their 1440 px PNGs
│   └── assets/branding/          logo, its generation notes, and the Oni reference image
└── verify/                       scripts behind the register rows, one folder per row id
```

## 2. Documents and their authority

| Document | Role | Authority | Status |
|---|---|---|---|
| `README.md` | Product statement and index | none of its own; points to the documents | current |
| `docs/functional-design.md` | Intent, goals, the parts, the adapter requirements, open questions (7), builder choices (8) | Functional authority. Marks every statement **Decided** (user), **My call** (author) or **Open** | draft, not approved |
| `docs/technical-design.md` | Engine: invariants (1), process model (3), tiling host (3.1), SQLite (4), adapters (5), hop (6), messages and MCP (7), state (8), sidecar (9), first milestone (10), acceptance (11) | Wins over the functional summary where they differ (functional-design 5.2). Evidence tags `[V] [D] [R] [S] [U] [C]` (section 0) | draft, not approved |
| `docs/browser-integration.md` | Browser routing, contract, lifecycle, security, gates B1–B10 | Detail for functional-design 5.15 and technical-design 7.9 | requirements decided 2026-10-29; shape proposed |
| `docs/mukade-handover.md` | How Shinatama calls Mukade, at pin `4124b1f`; revisions after the pin (8); Hermes and DeepSeek (9) | Detail for technical-design 5.4.1 | proposal, not implemented |
| `docs/daodan-native-support.md` | Agenda reading, thread-task link, task across a hop, onboarding | Detail for functional-design 5.5 | proposed, not approved |
| `docs/harness-tools.md` | Web tools and read-only allowlists per harness, with a probe for every unproved fact | Detail for the companion spec 5.9, 6 A and 10.2; the tool inventories themselves are Mukade's | draft, nothing run but `--help` |
| `docs/specs/companion-explainer.md` | Companion pane, decision cards (revision 2) | Its decisions are ADR-0002 and ADR-0003 (Accepted) | spec |
| `docs/specs/token-savings.md` | Token-savings techniques, decision compaction, julia1 | Own spec | draft, not approved |
| `docs/verification-register.md` | Every claim not yet proved, how to prove it, where the result is written back | Wins over technical-design 10 where they differ | living |
| `docs/notes/*.md` | Surveys; inputs, not decisions | none | reference |
| `docs/mockups/*` | Interaction references | Not binding (functional-design 5.16) | reference |

The accepted Markdown FRDs and the user's 2026-11-02 instruction to proceed govern
implementation. The overall design still contains draft details and open gates;
the code's existence does not settle or pass them.

### Cross-references

```
README ──► functional-design ◄──► technical-design ──► mukade-handover ──► github.com/acaprino/mukade
   │              │    │                 │   ▲
   │              │    └► daodan-native  │   └ verification/ (CX4, OC2, PI5, E1, UNI1)
   │              └► browser-integration ◄┘
   └► verification-register ◄── every design doc; ──► verification/*.md (all 18 records)
specs/companion-explainer, specs/token-savings ──► technical-design sections they change
docs/adr/ ──► the sections listed in each record's scope.paths
docs/frd/ ──► exact spans of the design documents (path, sha256, code-point offsets)
```

## 3. Domains and where each is specified

| Domain | Functional | Technical | Other | Register rows | ADR |
|---|---|---|---|---|---|
| Real agents in PTYs | 1, 2, 5.1 | 3.1 | — | P, U | ADR-0004 |
| Native tiling workspace, layout | 5.16 | 3.1 | mockups | TL1–TL6 | ADR-0005 |
| Hosted agents, Gemini, Grok | 2, 5.9 | 5.7 | mukade-handover 9 | CL, CX, OC, PI, HM, DS | ADR-0006 |
| DeepSeek Harness ACP pane | 5.1, 5.16 | 5.7 | — | DS | ADR-0007 |
| Process model, no daemon | 3 | 3, 4.6 | — | P, S | ADR-0008 |
| Shared state (SQLite) | 5.1, 8 | 4 | — | S | ADR-0009 |
| Evidence and verification | 3, 8a | 0, 10, 11 | register 0 | all | ADR-0010 |
| Adapter contract | 6 | 5.5 | — | CL, CX, OC, PI, HM, DS | ADR-0011 |
| Agent placement (program, home, sets) | 5.14 | 3 | — | D15–D19, SS5 | ADR-0012 |
| Per-launch injection, isolation | 3 | 3, 7.5 | — | M1, SC5, CX12 | ADR-0013 |
| Inherited markers | 7 (calls) | 3 | notes/harness-cli 4 | P20 | ADR-0014 |
| Hop | 5.2 | 6 | mukade-handover 2 | H | ADR-0015 – ADR-0019 |
| Session interchange (Mukade) | 5.2, 5.16 | 5.1–5.4.1, 5.5.1 | mukade-handover | H27, SS | ADR-0020 – ADR-0022 |
| Sessions surface, search | 5.3, 5.16 | 3.1, 6.2 | — | SS | ADR-0023 |
| Messages and MCP | 5.10 | 7.1–7.7 | — | M | ADR-0024, ADR-0025 |
| Roundtable | 5.10 | 7.8 | — | R | ADR-0026 |
| Instance state, widget | 5.8, 5.13 | 8.1–8.3 | — | I, W | ADR-0027 |
| Overview and titles | 5.13 | 8.4, 8.5 | — | I | ADR-0028 |
| BillionContext sidecar | 5.6 | 9 | — | SC | ADR-0029, ADR-0030 |
| Login | 5.11 | 5.5 A | — | CL13, CX11, OC8, PI9 | ADR-0031 |
| Daodan | 5.5 | — | daodan-native-support | D | ADR-0032 – ADR-0034 |
| Browser | 5.15 | 7.9 | browser-integration | B | ADR-0035, ADR-0036 |
| Token savings | — | 7.5a | specs/token-savings | T | ADR-0037, ADR-0038 |
| Skins, terminal profiles | 5.7, 5.16 | 3.1 | — | U | ADR-0039 |
| Order of work, first milestone | 5.12, 5.16 | 10 | — | TL1, G0 rows | ADR-0040 |
| Companion pane, decision cards, web grounding | 5.1, 5.16 (to be declared) | 4.2, 5.5, 7.6 | specs/companion-explainer, harness-tools | spec 10.2 | ADR-0002, ADR-0003, ADR-0041 – ADR-0043 |

## 4. Decision records

`docs/adr/` holds one record per decision, in the format of Daodan's project-delivery
`adr-method` (`adr_tool.py`): frontmatter with scope, status and SHAs, then Context, Drivers,
Decision, Examples, Rejected Alternatives, Invariants, Consequences and Confirmation.
`docs/adr/README.md` has the generated index.

- ADR-0001 (Abandoned), ADR-0002 (Accepted) and ADR-0003 (Accepted, decision cards) cover the
  companion pane.
- ADR-0004 to ADR-0040 were backfilled on 2026-11-01 from decisions already written in the
  documents above. They are **Proposed**. Many record decisions the user already made in
  conversation, and each record's Context separates those from the author's calls. The user
  approves the text of a record before it becomes Accepted.
- A record holds only the decision. The design documents keep the full design and stay the
  place where details change. When a record and a document disagree, the disagreement is a
  defect to fix in one of them, not a precedence rule.
- `adr_tool.py validate` passes on all 45 records (ADR-0041 to ADR-0043 and the v2 records of
  ADR-0002 and ADR-0003 come from the companion work).
- Added 2026-11-01 and **Accepted** the same day with the coherence fixes (ADR-0002 v3, ADR-0012 v2
  and ADR-0045 v2 included; seven earlier versions Superseded; no record left Proposed), by hand and
  not yet validated by the tool: ADR-0046 (project manager), ADR-0047 (Rust, platforms, dependency policy), ADR-0048 (install, distribution,
  updates), ADR-0049 (text index outlives retention), ADR-0050 (the FRD and ADR method), and the
  v2 records of ADR-0031 and ADR-0037 that carry the narrowings ADR-0044 states. The README of
  `docs/adr/` has a reading map by domain and the coherence audit C1 to C12, all resolved.

## 4a. Functional requirements documents

`docs/frd/` holds the FRDs of Daodan's project-delivery `frd-method` (`frd_tool.py`): one JSON
record per area. Every requirement carries at least one evidence span, an exact excerpt
bound to the source file's SHA-256 and code-point offsets. Each requirement text ends with
its origin: decided by the user (with the date when the documents give one), author's call,
proposed, or unmarked in the design. Open questions are not requirements.

| FRD | Area | Requirements |
|---|---|---:|
| FRD-0001 | Workspace and agent hosting | 100 |
| FRD-0002 | Agent placement, homes and per-instance sets | 54 |
| FRD-0003 | Accounts and login | 26 |
| FRD-0004 | Project manager | 27 |
| FRD-0005 | Threads and hop | 74 |
| FRD-0006 | Search, sessions and resume | 66 |
| FRD-0007 | Roundtable and messages between instances | 51 |
| FRD-0008 | Daodan preinstalled and native task support | 57 |
| FRD-0009 | BillionContext sidecar | 38 |
| FRD-0010 | Overview, instance state and desktop widget | 53 |
| FRD-0011 | Browser integration | 72 |
| FRD-0012 | Companion pane, decision cards and web grounding (spec revision 4) | 201 |
| FRD-0013 | Token savings | 47 |

Since 2026-11-01 each area also has a readable Markdown record beside the JSON
(`FRD-NNNN-<slug>.md`), and four areas the extraction had left inside other records have their own:
FRD-0014 (shared state and privacy), FRD-0015 (model routing and the tool chooser), FRD-0016
(TokenEconomizer) and FRD-0017 (install and release). The Markdown is the text a person reviews;
the JSON is the evidence binding; `docs/frd/README.md` states the conventions (origin, slice, proof).

All 13 JSON records are **Proposed**, extracted on 2026-11-01 and last refreshed at `c84d28e`, after the
inconsistency fixes and the user's three rounds of answers that day (866 requirements); items are `extracted`, not
reviewed. `frd_tool.py validate` passes. An FRD becomes Accepted only through `promote`
after the user approves its exact text. Because evidence is bound to file hashes, any edit
to a cited document makes the spans in it stale until the FRD is refreshed.

## 5. The verification bench (`verify/`)

One folder per register row that has been run. Scripts are PowerShell 5.1 plus Node (`.mjs`)
helpers, written for the Windows 11 machine of the first runs. Raw output goes to
`verify/*/out/`, which Git ignores.

| Folder | Row | What it probes | Record |
|---|---|---|---|
| `E1/` | E1 | `--help` surface of the four agents | 2026-10-27-E1 |
| `CL13/`, `CX11/`, `OC8/`, `PI9/` | same | sign-in status per agent | 2026-10-27-* |
| `CX4/` | CX4 | Claude → Codex native import through Codex | 2026-10-27-CX4 (passed) |
| `OC2/` | OC2 | OpenCode session export/import round trip | 2026-10-27-OC2 (passed) |
| `PI5/`, `PI10/`, `PI11/` | same | Pi TUI attach, package update, startup timing | 2026-10-27-PI* |
| `D1/` | D1 | marketplace name collision | 2026-10-27-D1 (done) |
| `D15/` – `D19/` | same | private homes, install and update, OpenCode XDG children, system discovery, per-instance sets | 2026-10-28-D1x |
| `P20/` | P20 | inherited agent markers | 2026-10-28-P20 |
| `H27/` | H27 | per-harness native session write and resume against a local mock model | record moved to Mukade, then removed there |

`verify/H27/` is the largest probe: a mock model server (`mock/server.mjs`, speaks Anthropic
Messages, OpenAI Responses and Chat Completions, captures every request), per-harness
scripts, Python analysis (`analyze.py`, `strict.py`, `tool-census.py`, `transform.py`), and
two separate Rust workspaces. `driver/` calls the external `txcript` crate at a fixed commit.
That converter was **not adopted** (user, 2026-10-27; ADR-0020). `shim/` wraps `opencode` to
add `--standalone`. Neither crate is part of the product.

Register rows by area (prefix: count): TL 6, Q 10, P 21, S 11, CL 18, CX 16, OC 13, PI 15,
HM 14, DS 15, H 28, M 15, I 10, SC 14, D 25, PM 7, U 8, SS 6, W 10, R 7, X 6, T 4, E 6, B 10.
By gate: G0 116, G1 153, G2 16. The register's section 21 is the status view. It lists 18 run
rows: CX4, OC2 and PI5 passed, D1 is done, D17 and H27 are `changed`, and the other 12 are
partial. `docs/verification/` holds 18 records: 17 of those rows (H27's record lived in Mukade)
plus the UNI1 trial, which is not a register row.

## 6. External repositories the design depends on

| Repository | Relation | Where |
|---|---|---|
| `acaprino/mukade` | Shinatama's session-interchange library, linked in-process at a reviewed pin | technical-design 5.4.1, mukade-handover.md, ADR-0020 |
| Daodan (the user's plugin marketplace) | Preinstalled in every hosted agent; task layer read by Shinatama | functional-design 5.5, daodan-native-support.md, ADR-0032 – ADR-0034 |
| BillionContext | Sidecar proxy, unmodified, single source | functional-design 5.6, technical-design 9, ADR-0029 |
| Moli, Patchright | Browser engines | browser-integration.md, ADR-0035 |
| `tamaratran/fast-jev-compaction` | Decision compaction library | specs/token-savings.md, ADR-0037 |

## 7. Open items that block approval

From register section 20: O1 (test accounts and cost cap), O2 (default title backend), O3
(Windows in v1, widget "assistant" scope), O4 (Oni character for distribution), O5 (approval of
both designs), O7 (offer only actions both adapters support), O8 (reduced browser integration
and its performance budget), O9 ("deep seek code" read as DeepSeek Harness in an ACP pane),
O10 (ask Daodan for Hermes and DeepSeek packages). O6 was decided on 2026-10-28.

## 8. Inconsistencies between documents

The numbered worklist is [notes/2026-11-01-design-inconsistencies.md](notes/2026-11-01-design-inconsistencies.md).
On 2026-11-01, 75 items were resolved: editorial fixes, register-only rules written into the
governing sections as the author's calls, and the user's answers in three rounds (who decided
what, the degraded-adapter rule, stage 3, Codex sets, marketplaces, box leaks, messaging,
overview, collisions, titles, browser, token savings, sidecar changes, DeepSeek login, project
manager). Open: 18 + 6 items on the companion spec and harness-tools, left to that work; #53
(decided by the empirical row SC5); seven on the token-savings spec and register references.
