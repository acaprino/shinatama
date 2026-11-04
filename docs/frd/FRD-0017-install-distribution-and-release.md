# FRD-0017: Installation, distribution, updates and release

| | |
|---|---|
| Version | 1 |
| Status | Accepted 2026-11-01 (this Markdown text) |
| Date | 2026-11-01; R01, R02, R03, R05 and R09 amended 2026-11-02 after the user's platform decision |
| Sources | functional-design goal 8, 3, 5.6 (Node), 5.8 (Oni IP), 5.12 (audience), 7, 8; technical-design 3.1 (toolkit pin), 11.12; Cargo.toml; register 17 (X1 to X6), SC13, W9, O4, O5 |
| Governing ADRs | ADR-0047, ADR-0048, ADR-0040 |
| Register sections | X1 to X6, SC13, P17, P18, W7 to W9 |
| Related records | FRD-0001 (platforms, toolkit), FRD-0002 (`own` agent copies), FRD-0009 (sidecar source and no auto-update), FRD-0010 (widget assets) |

## 1. Purpose and scope

Shinatama is built as if other people will install it: the install instructions of Daodan and
Shinatama already point to public repositories. This record states the platforms and install
mechanisms, release builds with signing and notarization, how Shinatama itself updates without
touching the sidecar's rule, the dependency and licence policy, the outbound-connection rule,
and the legal decisions that stay separate from the code.

## 2. Requirements

### 2.1 Platforms and install

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R01 | Easy install on Windows x64 is a goal; a release binary, an installer or a package manager is the builder's call, and the installer handles the WebView2 runtime. macOS and Linux are deferred. | U 2026-11-02; A (mechanisms) | V1 | X1 | 0048 |
| R02 | A first run on clean Windows 11, from install to the first hop, is timed and its failures listed before release. | S | V1 | X2 | 0048 |
| R03 | The sidecar needs Node; how Node is provided on Windows (system, bundled, downloaded; offline, size, version) is the builder's call within the install story and is measured. | S | V1 | SC13 | 0048, 0029 |
| R04 | Agents are not installed by Shinatama unless the user chooses the `own` program (FRD-0002); the checkup reports what is present and how the user's own installer updates it. | A | X | D16, D18 | 0012 |

### 2.2 Release builds and the code

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R05 | Release builds and Windows code signing are part of the plan. | A (audience) | V1 | X1 | 0048 |
| R06 | The code lives in this repository, `shinatama` (earlier suffixes `-sld` and `-mux` were dropped); the local folder `D:\Projects\shinatama-sld` is renamed to `D:\Projects\shinatama` once JanuraCode releases it. | U 2026-10-26 | X | — | 0048 |
| R07 | The Cargo workspace (edition 2024, resolver 3) has no members until the runtime's first crate exists; `[workspace.dependencies]` is the only list of dependencies the crates may use, and adding one is a decision, not a side effect. The `mukade` crate is declared there as a Git dependency at a reviewed commit with the lockfile committed when the first crate exists. | A | X | P17 | 0047, 0020 |
| R08 | The sidecar's single source (`sidecar/source-build`, today in another repository) is brought in without forking it, as a copy with provenance, a submodule or a published package, the builder's call, but it stays one source. | S (builder's call); U 2026-11-01 (single source) | X | — | 0029 |
| R09 | The Rust crates and npm packages the design relies on (PTY, terminal emulation, Tauri, SQLite with FTS5) exist, are maintained and work on Windows; the shortlist with versions and licences is recorded before the first milestone. | A | M0 | P17 | 0047 |

### 2.3 Updates

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R10 | Shinatama's own updates are specified (how they work, what they check) and never touch the sidecar's no-auto-update rule: the owned sidecar source upgrades only through Shinatama, never by itself. | S | V1 | X6 | 0048, 0029 |
| R11 | Update checks run only when the user enabled them; the same holds for model-catalog calls. | S | V1 | X4 (SEC) | 0048 |
| R12 | An `own` agent copy updates next to the running version with rollback (FRD-0002 R07); the agents' own self-update is blocked for `own` copies and never run by Shinatama for `system` programs. | A | V1 | D16 | 0012 |
| R13 | A pinned dependency (Mukade, Moli, Patchright, the local model) advances only with a recheck of its API, versions and acceptance evidence; versions, checksums, licences and update behaviour are recorded. | S | X | — | 0020, 0036, 0048 |

### 2.4 Licences and legal

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R14 | The licences of every dependency and bundled asset (crates, SQLite, llama.cpp, the small model, BillionContext, Mukade, fonts, icons) are compatible with MIT and listed in a dependency notice. | S | V1 | X3 | 0047, 0048 |
| R15 | The Oni-derived art (the default skin's character, the widget) is a distribution decision separate from the code: fine for personal use; the terms for a distributed build are a legal decision (W9, O4). | S | V1 | W9, O4 | 0048 |
| R16 | The local title model's licence is shown before its one-time download and checked for distribution (FRD-0010 R19). | A | V1 | I6, X3 | 0028 |

### 2.5 Outbound connections and telemetry

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R17 | No unintended outbound connection: only agent traffic through the sidecar, title and companion calls the user enabled, the router's opt-in digest, model-catalog calls and update checks the user enabled. There is no telemetry of Shinatama's own. | S | V1 | X4 (SEC) | 0048, 0009 |
| R18 | Hosted agents' own telemetry is not changed except where an `own` copy requires a switch (`PI_TELEMETRY=0`); Shinatama never edits a `system` program's settings. | A | V1 | D16 | 0012 |

### 2.6 Approval and documentation

| ID | Requirement | Origin | Slice | Proof | ADR |
|---|---|---|---|---|---|
| R19 | Nothing is built from the design documents until the user approves them (register O5); the first milestone's `G0` rows decide whether the design stands. | S | X | O5 | 0040, 0010 |
| R20 | Every decision lives in a decision record and every requirement in a requirements record, Proposed until the user approves its text; the design documents keep the full design; the register alone says what is proved (ADR-0050). | U 2026-11-01 (decision records are the method) | X | — | 0050 |
| R21 | The README is the entry point and index; the codebase map is regenerated when documents are added, renamed or retired. | S | X | — | 0050 |
| R22 | Interactive mockups are interaction references, not binding; the superseded `daodan-flow.html` is not revived. | S | — | — | 0005, 0033 |

## 3. Open questions

1. Windows in v1 (register O3).
2. The Oni character for distribution (register O4).
3. Approval of the functional and technical designs (register O5).

## 4. Acceptance

X1 to X6, SC13, P17, P18, W7 to W9; AT-12 (portability: tests 1 to 11 pass on the three
systems or the failing system is named in the release notes).
