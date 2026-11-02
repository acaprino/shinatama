# Functional requirements documents

Date of this set: 2026-11-01, written at commit `620bcf9` from every design document in the
repository (`functional-design.md`, `technical-design.md`, `browser-integration.md`,
`mukade-handover.md`, `daodan-native-support.md`, `harness-tools.md`, `specs/companion-explainer.md`,
`specs/token-savings.md`, the two notes of 2026-11-01 and the verification register). Status of
every Markdown record: **Accepted** by the user on 2026-11-01. The JSON evidence records stay
Proposed until `frd_tool.py promote` runs; their spans into `functional-design.md`,
`technical-design.md`, `companion-explainer.md` and `token-savings.md` are stale after the
coherence fixes of that day and need a refresh with the tool.

## 1. Two representations, one set

Each area has two files with the same number:

- `FRD-NNNN-<slug>.md`: the readable requirements document. This is what a person reviews,
  approves and builds from. It groups the requirements by behaviour, says who decided each one,
  in which delivery slice it lands, which register row or acceptance test proves it, and which
  decision records govern it.
- `FRD-NNNN-<slug>.json` (FRD-0001 to FRD-0013 only): the evidence record of Daodan's
  `frd-method` (`frd_tool.py`). Every requirement there carries exact spans of the design
  documents, bound to the file's SHA-256 and code-point offsets. The JSON is the machine proof
  that a requirement comes from the specification; the Markdown is the specification of the
  requirements.

When the two disagree, the disagreement is a defect to fix in one of them, not a precedence
rule. The Markdown consolidates the 866 extracted sentences of the JSON set into fewer, complete
statements and adds four areas (FRD-0014 to FRD-0017) that the extraction had left inside other
records or had not reached.

## 2. Conventions used in every record

**Requirement id.** `Rnn` inside a document; outside it, `FRD-0005/R12`. Ids are stable: a
withdrawn requirement keeps its number and is marked withdrawn, never reused.

**Statement.** Present tense, Shinatama as the subject, one behaviour per row. "Never" and
"only" are deliberate: they are the words a test can falsify.

**Origin**, who decided it:

| Code | Meaning |
|---|---|
| **U** | Decided by the user, with the date when the documents give one. |
| **A** | The author's call to fill a gap; the user has not confirmed it. |
| **P** | Proposed in a specification that is not yet approved as a whole (browser, Daodan native support, companion, token savings). |
| **S** | Stated in a design document without an origin mark. |

**Slice**, when it is delivered (functional-design 5.12, ADR-0040):

| Code | Meaning |
|---|---|
| **M0** | Terminal-first UI milestone: project selector, terminal tiles, tab groups, terminal profiles. |
| **S1** | First usable slice: read-only session browsing and search, same-agent native resume. |
| **S2** | Live seed hop with preview, consent, journal and recovery. |
| **V1** | In v1, after its own gates, not blocking S1 or S2. |
| **P1** | Post-v1, gated per agent and version. |
| **EV** | An evaluation, not a delivery. |
| **X** | Cross-cutting: holds from the first line of code. |

**Proof.** The register rows (`docs/verification-register.md`) or acceptance tests that prove
the requirement: `P1`, `H7`, `B4` are register rows; `AT-n` is acceptance test n of
technical-design 11; `CS-n` is row n of the companion specification's 10.2; `TE-n`, `MR-n`
are the checks of the two notes of 2026-11-01. A requirement with no proof row is marked `—`
and is a gap the register has to close.

**ADR.** The decision records that govern the requirement (`docs/adr/`).

Open questions are not requirements. Each record lists them at the end, with the register's
`O` ids where they exist.

## 3. Index of the readable records

| FRD | Title | Requirements | Slices | Governing ADRs |
|---|---|---|---|---|
| [FRD-0001](FRD-0001-workspace-and-agent-hosting.md) | Workspace and agent hosting | 58 | M0, V1 | 0004, 0005, 0006, 0007, 0039, 0040 |
| [FRD-0002](FRD-0002-agent-placement.md) | Agent placement, homes and per-instance sets | 41 | X, V1 | 0012 v2, 0013, 0014 |
| [FRD-0003](FRD-0003-accounts-and-login.md) | Accounts and login | 20 | X, V1 | 0031 |
| [FRD-0004](FRD-0004-project-manager.md) | Project manager | 25 | M0, V1 | 0046 |
| [FRD-0005](FRD-0005-threads-and-hop.md) | Threads and hop | 62 | S2, P1 | 0015 to 0019, 0022, 0034 |
| [FRD-0006](FRD-0006-search-sessions-and-resume.md) | Search, sessions, resume and the command line | 48 | S1, S2, P1 | 0011, 0020, 0021, 0023, 0049 |
| [FRD-0007](FRD-0007-roundtable-and-messaging.md) | Roundtable and messages between instances | 42 | V1 | 0024, 0025, 0026 |
| [FRD-0008](FRD-0008-daodan.md) | Daodan preinstalled and native task support | 42 | X, V1 | 0032, 0033, 0034 |
| [FRD-0009](FRD-0009-billioncontext-sidecar.md) | BillionContext sidecar | 30 | X | 0029, 0030 |
| [FRD-0010](FRD-0010-overview-state-and-widget.md) | Overview, instance state and desktop widget | 44 | V1 | 0027, 0028 |
| [FRD-0011](FRD-0011-browser.md) | Browser integration | 52 | V1 | 0035, 0036 |
| [FRD-0012](FRD-0012-companion-pane.md) | Companion pane, Aided Decision cards and web grounding | 96 | V1 (R1 to R5) | 0002 v3, 0003 v2, 0041, 0042, 0043 |
| [FRD-0013](FRD-0013-token-savings.md) | Token savings | 40 | V1, EV | 0025, 0037, 0038 |
| [FRD-0014](FRD-0014-shared-state-and-privacy.md) | Shared state, privacy and data retention | 32 | X | 0008, 0009 |
| [FRD-0015](FRD-0015-model-routing-and-tool-chooser.md) | Model routing and the tool chooser | 26 | V1 | 0044, 0031 v2, 0037 v2 |
| [FRD-0016](FRD-0016-tokeneconomizer-module.md) | TokenEconomizer as an optional module | 24 | V1 | 0045 v3 |
| [FRD-0017](FRD-0017-install-distribution-and-release.md) | Installation, distribution, updates and release | 22 | X, V1 | 0047, 0048 |

Cross-cutting rules that every record relies on are stated once and referenced: evidence before
claims (ADR-0010; functional-design 8a), the process model (ADR-0008), the adapter contract
(ADR-0011) and the seven engine invariants I1 to I7 (technical-design 1).

## 4. Relation to the design documents and the register

The design documents keep the full design and stay the place where details change. A record
holds the requirements, not the reasoning: when a statement here needs its "why", the row names
the section that has it. The verification register remains the only place that says whether a
requirement is proved; a record never claims a pass.

Any edit to a cited document changes its hash, so the JSON evidence pointing into it is stale
until the FRD is refreshed with the tool; the Markdown records are refreshed by hand in the same
commit as the change they follow.

<!-- frd-index:start -->

Generated by frd_tool.py. Edit records through the CLI, not this block.

| FRD | Version | Title | Status | Requirements | Open conflicts | Verified at |
|---|---:|---|---|---:|---:|---|
| [FRD-0001](<FRD-0001-workspace-and-agent-hosting.json>) | 1 | Workspace and agent hosting | Proposed | 100 | 0 | `` |
| [FRD-0002](<FRD-0002-agent-placement.json>) | 1 | Agent placement, homes and per-instance sets | Proposed | 54 | 0 | `` |
| [FRD-0003](<FRD-0003-accounts-and-login.json>) | 1 | Accounts and login | Proposed | 26 | 0 | `` |
| [FRD-0004](<FRD-0004-project-manager.json>) | 1 | Project manager | Proposed | 27 | 0 | `` |
| [FRD-0005](<FRD-0005-threads-and-hop.json>) | 1 | Threads and hop | Proposed | 74 | 0 | `` |
| [FRD-0006](<FRD-0006-search-sessions-and-resume.json>) | 1 | Search, sessions and resume | Proposed | 66 | 0 | `` |
| [FRD-0007](<FRD-0007-roundtable-and-messaging.json>) | 1 | Roundtable and messages between instances | Proposed | 51 | 0 | `` |
| [FRD-0008](<FRD-0008-daodan.json>) | 1 | Daodan preinstalled and native task support | Proposed | 57 | 0 | `` |
| [FRD-0009](<FRD-0009-billioncontext-sidecar.json>) | 1 | BillionContext sidecar | Proposed | 38 | 0 | `` |
| [FRD-0010](<FRD-0010-overview-state-and-widget.json>) | 1 | Overview, instance state and desktop widget | Proposed | 53 | 0 | `` |
| [FRD-0011](<FRD-0011-browser.json>) | 1 | Browser integration | Proposed | 72 | 0 | `` |
| [FRD-0012](<FRD-0012-companion-pane.json>) | 1 | Companion pane, decision cards and web grounding | Proposed | 201 | 0 | `` |
| [FRD-0013](<FRD-0013-token-savings.json>) | 1 | Token savings | Proposed | 47 | 0 | `` |

<!-- frd-index:end -->
