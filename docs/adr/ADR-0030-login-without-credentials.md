---
adr_id: ADR-0030
version: 1
title: Login detected and performed through the agent itself; Shinatama never holds credentials
status: Proposed
author: Alfio Caprino
scope:
  paths:
    - docs/functional-design.md
    - docs/technical-design.md
  modules:
  domains:
    - login
    - auth-status
    - credentials
    - adapter-contract
governs_symbols:
depends_on:
  - ADR-0010
  - ADR-0011
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: null
accepted_at_sha: null
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0030: Login detected and performed through the agent itself; Shinatama never holds credentials

<!-- adr-header:start -->
Version 1 · Proposed 2026-11-01 at b667db5 · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no code yet. The user asked Shinatama to "recognize automatically or perform the
login for codex, claude, etc." (functional-design goal 11 and 5.11, decided as a wish). The shape
is the author's call: detect, never collect; log in through the agent's own flow; show the state
where it matters. Whether Shinatama should also hold provider keys once and share them across
agents is open, and functional-design 7 records it as not in v1. Four verification records of
2026-10-27 (CL13, CX11, OC8, PI9) observed the status commands of Claude Code, Codex, OpenCode
and Pi on Windows 11, with no login performed; none has passed. The full design is in
functional-design 5.11 and technical-design 5.5 A and 5.7; this record holds only the decision.

## Decision Drivers

- Shinatama must not become a credential store that must be protected.
- No credentials in the database, snapshots, messages, summaries or the sidecar's launch manifest (technical-design I5).
- Each agent's authentication methods differ and change by version.
- A hop destination is prepared in the background, so a signed-out destination must be caught before the hop.
- In a box an agent starts signed out.

## Decision

At first run and in `shinatama doctor` each agent shows whether it is signed in. The adapter's
`auth_status()` returns `SignedIn`, `SignedOut` or `Unknown` by asking the agent itself, through
its own status command or credential check. Shinatama does not read, copy, move or store an
agent's credentials and puts none in the sidecar's launch manifest. `login()` runs the agent's
own flow (browser sign-in, device code or key entry) inside Shinatama's terminal, and Shinatama
only observes the outcome. Each adapter declares what it can detect and what it can only launch.
Login state appears in the setup screen, in the project manager (an agent that cannot start shows
why) and in the hop dialog, where a destination that is not signed in offers login before the
hop.

Hermes Agent reports sign-in without secrets (`hermes auth list`, `hermes auth status
<provider>`) and logs in with `hermes model` or `hermes auth add`. DeepSeek Harness has neither a
login nor a status command: its state shows as `unknown`, and its login opens its own web UI
(`dsh web`, on the box's home) in the user's browser. It also reads a project's `.env`, so a key
there is used even in a box, and the setup screen says so. In a box each agent needs one first
login through its own flow, which then persists in the box and is shared by every instance of
that agent; with the `system` home the user's existing login is used as it is. Commands that
print stored secrets are never called. See functional-design 5.11 and technical-design 5.5 A.

## Verified Interview Examples

- **EX-001** (nominal case): Input: `claude auth status` (claude 2.1.311) on a signed-in home, then on a scratch `CLAUDE_CONFIG_DIR` -> Expected: exit 0 and one JSON object with `loggedIn` true, then exit 1 with `loggedIn` false; no credential material in either (CL13 record).
- **EX-002** (boundary case): Input: `codex login status` (codex-cli 0.164.0) -> Expected: stdout empty; stderr `Logged in using ChatGPT` with exit 0, or `Not logged in` with exit 1; the adapter reads stderr and the exit code (CX11 record).
- **EX-003** (rejection or error case): Input: `opencode auth list` (opencode 2.1.4) on the real home, which failed five times in a row with `Location plugins did not finish activating` (exit 1) after two successes -> Expected: the command is not adopted as the detector as it is; the HTTP route or retries are preferred, and `opencode auth export`, which prints secrets, is never called (OC8 record).
- **EX-004** (boundary case): Input: `pi auth check --json --no-refresh --provider anthropic` (pi 1.2.0) -> Expected: exit 1 and `{"status":"not_ready","provider":"anthropic","reason":"credentials_not_configured"}`; `--no-refresh` avoids refreshing or writing credentials, and `--credentials` and the `print-*` commands are never used (PI9 record).
- **EX-005** (boundary case): Input: DeepSeek Harness at setup -> Expected: sign-in shown as `unknown`; login opens `dsh web` on the box's home in the user's browser (functional-design 5.11; register DS9).

## Rejected Alternatives

- **RA-001**: Shinatama holding provider keys once and sharing them across agents; not in v1 and left open, because it would make Shinatama a credential store that must be protected (functional-design 5.11, 7).
- **RA-002**: reading credential files to infer sign-in (for example Pi's `auth.json`) or calling commands that print credentials (`opencode auth export`, `pi auth print-api-key`, `pi auth print-bearer-token`, `--credentials`); the adapter asks the agent and never reads credentials (technical-design 5.5 A; register D5, PI9).

## Enforceable Invariants

- **INV-001**: no credential is stored in the database, snapshots, messages, summaries or the sidecar's launch manifest (technical-design I5).
- **INV-002**: `auth_status()` only runs the agent's own status command or check and reads no credential file; an agent with no secret-free check reports `Unknown`.
- **INV-003**: `login()` runs the agent's own flow in a PTY; Shinatama observes only the outcome.
- **INV-004**: commands that print stored credentials are never called.
- **INV-005**: a hop to a destination that is not signed in offers login first; if that cannot be fixed, the strategy is `refuse` (technical-design 6.2, 6.7).

## Consequences

- The detectors are per agent and per version: Claude's exit code (CL13), Codex's stderr and exit code on 0.164.0 (CX11), an HTTP route or retries for OpenCode (OC8), and `pi auth check --json --no-refresh --provider <p>` as Pi's candidate (PI9). Hermes Agent and DeepSeek Harness are not installed on the test machine (register 7a, 7b).
- Login in a PTY, the polling cost while signed out and a Pi provider actually signed in are open and wait for test accounts (register O1); so does login persistence in a box (D15).
- A box costs one first login per agent; a key in a project's `.env` reaches DeepSeek Harness even in a box.
- Title calls (ADR-0027) and the sidecar manifest (ADR-0028) follow the same rule: Shinatama holds no credentials.
- The conformance suite covers the signed-out path for each adapter (technical-design 5.5.3).

## Confirmation

- Register D5, CL13, CX11, OC8, PI9 and DS9 pass. CL13, CX11, OC8 and PI9 are running (partial), and OC8's criterion is not met; D5 and DS9 are open.
- Nothing about login has passed yet; the 2026-10-27 runs performed no login and made no model calls.
