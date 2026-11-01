---
adr_id: ADR-0031
version: 2
title: Login detected and performed through the agent itself; Shinatama never holds an agent's credentials; the one key it may hold is the router's, in the OS vault only
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
    - docs/notes/2026-11-01-model-routing-decision-model.md
  modules:
  domains:
    - login
    - auth-status
    - credentials
    - adapter-contract
    - router-key
governs_symbols:
depends_on:
  - ADR-0044
created_from_task: frd-adr-consolidation-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: 620bcf9a66edeb57d0528b3c522f3018928b3706
accepted_on: 2026-11-01
accepted_at_sha: 202a34e2675cfe7e8d7ba1d5e7ecf88bf2299182
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
  - Decision
  - INV-001
  - INV-007
  - EX-006
  - RA-003
  - INV-008
---

# ADR-0031: Login detected and performed through the agent itself; Shinatama never holds an agent's credentials; the one key it may hold is the router's, in the OS vault only

<!-- adr-header:start -->
Version 2 · Accepted 2026-11-01 at 202a34e · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Version 1 (Accepted 2026-11-01) says Shinatama never holds credentials. On the same day the user
asked for an automatic model router reached with one key of the user's, free or paid, and ADR-0044
(Accepted) records that the router key lives only in the operating system's vault and states that
it "narrows ADR-0031 to agent credentials". An accepted record cannot be edited, so this version
carries the narrowing; everything else of version 1 stands unchanged. Sources: the user's request
for a router key (2026-11-01; routing note 1 and 7); the vault shape is the author's call (routing
note 7; ADR-0044 INV-004). The verification records CL13, CX11, OC8 and PI9 of 2026-10-27 observed
the status commands of four agents with no login performed; none has passed. The full design is in
functional-design 5.11, technical-design 5.5 A and 5.7 and the routing note; this record holds only
the decision.

## Decision Drivers

- Shinatama must not become a credential store that must be protected.
- No credentials in the database, snapshots, messages, summaries or the sidecar's launch manifest (technical-design I5).
- The user asked for a router reached with one key of their own; a key Shinatama does not hold cannot be used.
- Each agent's authentication methods differ and change by version.

## Decision

Unchanged from version 1: at first run and in `shinatama doctor` each agent shows whether it is
signed in; `auth_status()` returns `SignedIn`, `SignedOut` or `Unknown` by asking the agent itself;
Shinatama does not read, copy, move or store an **agent's** credentials and puts none in the
sidecar's launch manifest; `login()` runs the agent's own flow in a PTY and Shinatama observes the
outcome; each adapter declares what it detects and what it only launches; login state appears in
the setup screen, the project manager and the hop dialog; Hermes Agent and DeepSeek Harness follow
the rules of version 1 (DeepSeek `unknown` until DS9, login through `dsh web` on the home the agent
uses); a box needs one first login per agent; commands that print secrets are never called.

Narrowed: the rule covers the credentials of the hosted agents and of their providers. The one
credential Shinatama itself may store is the **router key** of ADR-0044, and only in the operating
system's vault (Windows Credential Manager, the macOS Keychain, the Secret Service on Linux),
referenced from the configuration by name (`router.key`) and read only by the router sidecar; it
never appears in Shinatama's database, snapshots, messages, configuration files, launch manifests
or logs, and it is never given to a hosted agent. Holding provider keys once and sharing them
across agents remains outside v1.

Clarified for the companion (ADR-0002 v3, coherence audit C3): a companion's harness reaches its
credential files only through a read-only binding of their original path that the operating system
enforces (a read rule or a read-only bind mount); Shinatama never opens, copies, moves or stores
them, so "detect, never collect" holds. A platform that cannot bind runs the companion as the
opt-in preview of ADR-0002.

## Verified Interview Examples

- **EX-001** (nominal case): Input: `claude auth status` (claude 2.1.311) on a signed-in home, then on a scratch `CLAUDE_CONFIG_DIR` -> Expected: exit 0 and one JSON object with `loggedIn` true, then exit 1 with `loggedIn` false; no credential material in either (CL13 record).
- **EX-002** (boundary case): Input: `codex login status` (codex-cli 0.164.0) -> Expected: stdout empty; stderr `Logged in using ChatGPT` with exit 0, or `Not logged in` with exit 1; the adapter reads stderr and the exit code (CX11 record).
- **EX-003** (rejection or error case): Input: `opencode auth list` (opencode 2.1.4) on the real home, which failed five times in a row after two successes -> Expected: not adopted as the detector as it is; the HTTP route or retries are preferred, and `opencode auth export`, which prints secrets, is never called (OC8 record).
- **EX-004** (boundary case): Input: `pi auth check --json --no-refresh --provider anthropic` (pi 1.2.0) -> Expected: exit 1 and `{"status":"not_ready", …}`; `--no-refresh` avoids writing credentials; `--credentials` and the `print-*` commands are never used (PI9 record).
- **EX-005** (boundary case): Input: DeepSeek Harness at setup -> Expected: sign-in shown as `unknown`; login opens `dsh web` on the home the agent uses (functional-design 5.11; register DS9).
- **EX-006** (nominal case): Input: the user enters a TypeSafe Jev key in the router settings -> Expected: the key is written to the OS vault under a Shinatama entry; `settings` holds only the entry's name; a dump of the database, the configuration files, the launch manifests and the logs contains no byte of the key; the router sidecar reads it; no hosted agent's environment contains it.

## Rejected Alternatives

- **RA-001**: Shinatama holding provider keys once and sharing them across agents; not in v1 (author's call), because it would make Shinatama a credential store that must be protected (functional-design 5.11, 7).
- **RA-002**: reading credential files to infer sign-in or calling commands that print credentials; the adapter asks the agent and never reads credentials (technical-design 5.5 A; register D5, PI9).
- **RA-003**: storing the router key in Shinatama's configuration, database or a launch manifest, or giving it to a hosted agent; the vault is the only place (ADR-0044 RA-006).

## Enforceable Invariants

- **INV-001**: no agent or provider credential is stored in the database, snapshots, messages, summaries, configuration files or the sidecar's launch manifest (technical-design I5); the router key is the one exception and exists only in the OS vault.
- **INV-002**: `auth_status()` only runs the agent's own status command or check and reads no credential file; an agent with no secret-free check reports `Unknown`.
- **INV-003**: `login()` runs the agent's own flow in a PTY; Shinatama observes only the outcome.
- **INV-004**: commands that print stored credentials are never called.
- **INV-005**: a hop to a destination that is not signed in offers login first; if that cannot be fixed, the strategy is `refuse` (technical-design 6.2, 6.7).
- **INV-006**: DeepSeek Harness's `auth_status()` is `Unknown` until register DS9 finds a check that reads no secret, and its `login()` opens `dsh web` on the home the instance uses, never on another (decided by the user 2026-11-01; technical-design 5.7).
- **INV-007**: the router key is read only by the router sidecar, referenced by name from the configuration, never written to the database, snapshots, messages, configuration files, launch manifests or logs, and never present in a hosted agent's environment (ADR-0044 INV-004).
- **INV-008**: a companion's credentials are exposed only as a read-only OS binding of the original files; Shinatama's process never opens, copies or stores them (ADR-0002 v3 INV-008).

## Consequences

- The detectors stay per agent and per version as in version 1 (CL13, CX11, OC8, PI9); login in a PTY and login persistence in a box wait for test accounts (register O1, D15).
- The router sidecar is the one process with vault access; its launch and supervision follow the local-sidecar rules of ADR-0037.
- Title calls (ADR-0028) and the sidecar manifest (ADR-0029) are unchanged: Shinatama holds no credentials for them.
- FRD-0003 and FRD-0015 carry the requirement.

## Confirmation

- Register D5, CL13, CX11, OC8, PI9 and DS9 pass; a vault check for the router key is added to the register with the routing rows (MR1 to MR4).
- Nothing about login has passed yet; the 2026-10-27 runs performed no login and made no model calls.
