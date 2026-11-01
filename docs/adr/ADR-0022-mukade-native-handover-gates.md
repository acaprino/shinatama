---
adr_id: ADR-0022
version: 1
title: Post-v1 native handover; journal before write, receipt is not acceptance, no automatic retry, reasoning dropped, per-instance store roots
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/mukade-handover.md
    - docs/technical-design.md
    - docs/functional-design.md
  modules:
  domains:
    - native-handover
    - mukade-hop
    - write-recovery
    - store-roots
    - handover-privacy
governs_symbols:
depends_on:
  - ADR-0016
  - ADR-0018
  - ADR-0020
created_from_task: adr-backfill-2026-11-01
proposed_on: 2026-11-01
proposed_at_sha: b667db5c3d7df4618ccb73b73165e8f3290ae151
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0022: Post-v1 native handover; journal before write, receipt is not acceptance, no automatic retry, reasoning dropped, per-instance store roots

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

Shinatama has no runtime crate yet. After v1, a hop may write a real native session in the
destination through `mukade::hop` (ADR-0016, ADR-0020). `hop()` writes a new native session and
returns a receipt and manifest; "it does not implement Shinatama's verification, journal or
switch" (technical-design 5.4), and it has no host idempotency key. The question this record answers is which rules Shinatama follows around
that write before the route is enabled.

Origin: the user decided on 2026-10-31 that native writes are post-v1, gated per agent and
version (functional-design 5.2). The rules below are the proposed shape of
`docs/mukade-handover.md`, a "technical integration proposal, not implemented or approved",
written against Mukade `4124b1f`. `ReasoningPolicy::Drop` is the author's call, following
technical-design 5.1 and 6.4. Functional-design 5.16 repeats the write-path rules, unmarked.
Mukade-handover 2 to 7 and technical-design 6.3 and 6.7 hold the full design; this record holds
only the decision.

## Decision Drivers

- Each successful `hop()` creates another session, so a blind retry duplicates.
- A database transaction cannot make the native write atomic with the journal.
- A write reported by Mukade does not prove the destination works or understood the task.
- A boxed instance must never write into the user's own store by accident.

## Decision

Post-v1 only, and only for a pair and version whose gates pass. The sequence is: (1) plan and
preview; (2) after confirmation, journal `hop.id`, source reference and cursor, settings, target
store and intended effect, and reserve the destination id or path where Mukade supports it,
before any external work; (3) recheck the source fingerprint, then call `mukade::hop` with
`resume: None`, and persist the destination id, location, resume command and manifest at once;
(4) the adapter verifies the written session in the intended store, starts the interactive
instance, checks content against the manifest and obtains the separate nonce acknowledgement;
(5) switch or recover through technical-design 6.3. A Mukade receipt "proves a write was
reported"; it is not the nonce receipt, a list-and-resume check, or a successful launch.

After an uncertain outcome Shinatama never calls `hop()` again automatically: `SourceChanged`
discards the preview and requires a new confirmation; `WriteUncertain` reconciles the reserved
id and location first; `ResumeFailed` persists the embedded receipt and recovers that
destination. Absence is never inferred from missing host state, and native sessions, indexes or
leftovers are never deleted automatically. Hops select `ReasoningPolicy::Drop`, not Mukade's
`DropForeign` default. `StoreRoots` is built from the resolved instances, not ambient defaults;
OpenCode's database and `XDG_*` are resolved explicitly for a boxed instance; process-wide
environment variables are not changed for concurrent hops; Mukade's children must get
Shinatama's isolation, marker removal and credential configuration. The route stays closed until
the gates of mukade-handover 5 are met: an exact staged preview with separate consent before a
source-model summary, a redaction boundary over the complete data, a stable source cursor with
serialized target writes, and isolation of children, caches, hooks and temporary files.

## Verified Interview Examples

- **EX-001** (rejection or error case): Input: `SourceChanged { expected, actual }` from a staged call -> Expected: discard the preview, recapture the source and require a new confirmation; never write from the stale selection (mukade-handover 6).
- **EX-002** (rejection or error case): Input: `WriteUncertain { reason, dest_id, location }` -> Expected: reconcile the returned or reserved id and location before any retry; never create another destination automatically (mukade-handover 6).
- **EX-003** (rejection or error case): Input: `ResumeFailed { reason, receipt }` -> Expected: persist the receipt and manifest, keep the source live and recover or resume that destination; never blindly repeat the import (technical-design 6.7; mukade-handover 6).
- **EX-004** (boundary case): Input: a double-click, a cross-process retry and fault injection at every write and receipt point -> Expected: no unconfirmed or duplicate write, and every uncertain destination reconciled (register SS4, SEC).
- **EX-005** (boundary case): Input: a second OpenCode import of an existing `ses_` id -> Expected (observed on 2.1.4): 409 `ConflictError`, "Session already exists" (OC2 record).

## Rejected Alternatives

- **RA-001**: Mukade's `DropForeign` default, preserving compatible signed reasoning, or rendering it as labelled text; Shinatama's canonical transcript and seed exclude reasoning, and "using either would require a separate Shinatama policy decision" (mukade-handover 4).
- **RA-002**: calling `hop()` again automatically after an uncertain crash; "each successful call creates another session and Mukade has no host `hop.id` idempotency key" (mukade-handover 2).
- **RA-003**: `StoreRoots::from_env()` or ambient defaults for an instance; for OpenCode `None` means its configured store, so a boxed instance could target the user's store (mukade-handover 3).
- **RA-004**: substituting a weaker preview for the exact one; "until then this route is gated, not silently substituted" (mukade-handover 5).

## Enforceable Invariants

- **INV-001**: no native write starts before the journal holds `hop.id`, source reference and cursor, settings, target store and intended effect (technical-design I3).
- **INV-002**: no automatic second `hop()` or import after an uncertain or failed outcome; reconciliation comes first.
- **INV-003**: a Mukade receipt never satisfies READY; READY needs the adapter's check against the manifest and the nonce acknowledgement.
- **INV-004**: the receipt and manifest are persisted even when launch or resume fails.
- **INV-005**: hops request `ReasoningPolicy::Drop`; signed or encrypted payloads are never altered.
- **INV-006**: every write is a new session; source records are never edited or deleted (technical-design I2); no headless resume and interactive writer run on one session at once.
- **INV-007**: `StoreRoots` comes from per-instance resolution; system roots are never a box's write destination; a same-harness two-home transfer is refused until SS5 verifies it.

## Consequences

- None of this is reachable in v1; the `native_import` branches of technical-design 6.3 stay
  closed (ADR-0016).
- Before enabling: every allowed pair and version in scratch stores, roots and isolation, losses,
  resume and nonce, close/fork and catch-up, preview and redaction, summary-cost consent, failure
  before and after write, crash recovery, concurrent hops and provider rejection
  (mukade-handover 7). Real providers only with approved accounts and limits (register O1).
- The manifest and all intermediate material are sensitive; they are not promised to disappear
  after a crash.
- A source-model summary costs money and discloses content to the source's provider; it runs
  only after consent. `DestinationNative` is `Unsupported` in Mukade at the pin.
- Mukade's CLI codes 2, 4 and 5 are not Shinatama's contract; its journal and exit codes are.
- Revisions after the pin (`hop_into`) count only after the pin advances (register 20, next run 8).

## Confirmation

- Register SS4, SS5 and SS6 pass for each pair before it is enabled; acceptance 11.1 passes with
  the native branches.
- H27 is a probe with an external converter; it does not validate this integration.
- "No such host tests have been run" (mukade-handover 7).
