# Codebase map

Date: 2026-11-01, at commit `b667db5`. Status: descriptive, not a design document; it changes
nothing in the specs. Regenerate it when documents are added, renamed or retired.

Shinatama is in the pre-design phase: the repository holds design documents, decision records
and a verification bench, and **no product code**. The Cargo workspace has no members
(`Cargo.toml`), and the only Rust in the tree is two throw-away probes under `verify/H27/`.
This map says what each part is, which document has authority over which topic, and where the
decisions live.

Built by hand from a full read of the documents. Daodan's `codebase-map` plugin was not run
for this map and no `.codebase-map/` directory was created. Daodan's onboarding check
(`daodan-native-support.md` 4.7) uses that directory as a marker of Daodan state, so a
hand-made copy would give a false answer.

## 1. Layout

```
shinatama/
├── README.md                     entry point: one-paragraph product statement, links, naming, D1 history
├── Cargo.toml                    empty workspace (edition 2024, resolver 3); the allowed dependency list
├── .gitignore                    .jcode/, .daodan/, target/, verify/*/out/
├── docs/
│   ├── functional-design.md      WHAT and WHY: goals, non-goals, surfaces, the parts (5.1–5.16), adapter requirements
│   ├── technical-design.md       HOW: invariants I1–I7, process model, SQLite, adapters, hop, messaging, state, sidecar
│   ├── browser-integration.md    Moli fast path and explicit Patchright UI (proposed 2026-10-29)
│   ├── mukade-handover.md        integration guide for the Mukade library, pinned at 4124b1f
│   ├── daodan-native-support.md  Daodan task layer inside Shinatama (proposed 2026-10-27)
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
| `docs/specs/companion-explainer.md` | Companion pane, decision cards (revision 2) | Its decisions are ADR-0002 and ADR-0003 (Accepted) | spec |
| `docs/specs/token-savings.md` | Token-savings techniques, decision compaction, julia1 | Own spec | draft, not approved |
| `docs/verification-register.md` | Every claim not yet proved, how to prove it, where the result is written back | Wins over technical-design 10 where they differ | living |
| `docs/notes/*.md` | Surveys; inputs, not decisions | none | reference |
| `docs/mockups/*` | Interaction references | Not binding (functional-design 5.16) | reference |

Overall approval of the design is still open (register O5). Nothing is built from these
documents until it is given.

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
| Companion pane, decision cards | 5.1, 5.16 (to be declared) | 4.2, 5.5, 7.6 | specs/companion-explainer | spec 10.2 | ADR-0002, ADR-0003 |

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
- `adr_tool.py validate` passes on all 40 records.

## 4a. Functional requirements documents

`docs/frd/` holds the FRDs of Daodan's project-delivery `frd-method` (`frd_tool.py`): one JSON
record per area. Every requirement carries at least one evidence span, an exact excerpt
bound to the source file's SHA-256 and code-point offsets. Each requirement text ends with
its origin: decided by the user (with the date when the documents give one), author's call,
proposed, or unmarked in the design. Open questions are not requirements.

| FRD | Area | Requirements |
|---|---|---:|
| FRD-0001 | Workspace and agent hosting | 97 |
| FRD-0002 | Agent placement, homes and per-instance sets | 50 |
| FRD-0003 | Accounts and login | 26 |
| FRD-0004 | Project manager | 19 |
| FRD-0005 | Threads and hop | 72 |
| FRD-0006 | Search, sessions and resume | 66 |
| FRD-0007 | Roundtable and messages between instances | 47 |
| FRD-0008 | Daodan preinstalled and native task support | 55 |
| FRD-0009 | BillionContext sidecar | 30 |
| FRD-0010 | Overview, instance state and desktop widget | 46 |
| FRD-0011 | Browser integration | 68 |
| FRD-0012 | Companion pane and decision cards (spec revision 3) | 168 |
| FRD-0013 | Token savings | 44 |

All 13 are **Proposed**, extracted on 2026-11-01 and refreshed at `e221915` after the merge of
revision 3 of the companion spec and the SC15 terminal-byte rule; items are `extracted`, not
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

## 8. Known inconsistencies found while mapping

The full, numbered list (103 numbers, 96 open items, each marked editorial, needs the user, or register-only)
is [notes/2026-11-01-design-inconsistencies.md](notes/2026-11-01-design-inconsistencies.md).
The items below were the first found.


- technical-design 5.7, consequence 5, says "for the other three" while six agents are listed.
- technical-design 4.3 keys the instance-exclusivity index by `(agent, segment_id)`, while 4.2
  scopes native sessions by `store_root_id`.
- The P20 record cites Pi 1.4.0; technical-design 0 lists Pi 1.2.0. Register section 21 says
  Pi rows recorded on 1.2.0 need a rerun.
- technical-design 0 does not define some tags the body uses (a bare `[H: …]`, mixed forms such
  as "[C; decided by the user 2026-10-28]").
- The companion pane is a second native chat surface, an exception to functional-design 5.16
  that ADR-0002 says must be declared there; it is not declared yet.

Found while writing ADR-0004 to ADR-0040 (each is also noted in the record named):

- Origin labels disagree. Inherited-marker removal is "[C; decided by the user 2026-10-28]"
  (technical-design 3), "adopted" (harness/cli note) and a call to check (functional-design 7)
  (ADR-0014). Proxy supervision sits under a user decision in functional-design 5.6 but is [C]
  in technical-design 9 (ADR-0030). The sessions surface is "decided" in functional-design 5.16
  and a call to check in functional-design 7 (ADR-0023). The roundtable shape is "open" in
  goal 10 and "mine" in 5.10 (ADR-0026).
- functional-design 5.8 describes the widget's feed as a socket or named pipe; technical-design
  4.6 and 8.3 make it the SQLite `event` table, with a socket only as an optional wake-up
  (ADR-0027).
- An agent that fails its fixtures degrades to "`Degraded` or `Unsupported`" (technical-design
  5.5.3), "launch-only plus the agent's own export command" (5.2) or "launch-only" (11.10)
  (ADR-0011).
- functional-design 3 and 8 ask for "real captured fixtures"; the register and technical-design
  5.5.3 ask for synthetic, structure-only fixtures. ADR-0010 reads them together: fixtures
  captured from the real installed agent running synthetic sessions.
- technical-design 5.4 allows a stage-3 write only for an agent with no import of its own, yet
  5.4.1 has Mukade's Codex writer write the rollout although Codex has an import (ADR-0016).
- "Never edit global configuration" (functional-design 3) sits beside three accepted writes: the
  Codex set pre-installed turned off, Claude Code rewriting `pluginUsage` in `.claude.json` (D19),
  and Daodan installed into the native plugin store on a `system` home (ADR-0013).
- Daodan's version is given as marketplace 1.0.0 (functional-design 5.5, register D4), 42.0.0
  (register D2) and 42.7.3 (D1 record). The D15 record found Daodan plugins depending on eight
  plugins from other marketplaces, against "one marketplace per host" (ADR-0032).
- functional-design 9 still says `daodan-flow.html` was dropped because the user rejected a
  thread-task link, which the 2026-10-27 native-support decision reversed (ADR-0033).
- functional-design 5.9 cites a "section 2, non-goals" entry about external terminal selection
  that section 2 does not contain (ADR-0006).
- Register U2 ("changing the skin live applies to the hosted pane") predates the terminal
  profiles and conflicts with the split between chrome and profile (ADR-0039).
- The register's TL1 row prototypes Markdown and file panels, while the TL1 milestone in
  functional-design 5.12 and 5.16 excludes the file navigator (ADR-0040).
- The OC8 record says its criterion is "Not met"; register section 21 lists it as "running
  (partial)" (ADR-0031).
