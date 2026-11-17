# Mukade: session handover integration

**Status: design record of a closed route, not implemented.** On 2026-11-03 the user decided
that native handover is out of scope in every version: a handoff always carries the seed
document (ADR-0019) and Shinatama calls only Mukade's read-side APIs — those are built
(`shinatama-engine` links Mukade behind its `session-reader` feature). Sections 2 to 7 below
describe the native write route as reviewed against the pin; they are kept as the re-entry
design for any future decision that supersedes ADR-0016 v2 / ADR-0022 v2 (both Accepted
2026-11-04), not
as work to do. This document describes the Mukade checkout at
`14edbf3cf71424a48b057e52c5532d6d4ab7d8a4` and the work Shinatama must do around it; it is not
evidence that the two products have been integrated. This revision includes the installation scan,
`StoreRoots::root`/`set_root`, `hop_into`, reader-reported session titles and preserved provider
reasoning proofs, on top of the OpenCode listing, `SourceChanged`, `WriteUncertain` and reservable
destination ids/paths required by the Sessions and recovery design. Updating the pin still
requires rechecking the complete API, harness versions and acceptance evidence. Shinatama uses
only read-side Mukade APIs through its adapters; native writes are out of scope in every
version (ADR-0016 v2, Accepted 2026-11-04). The upstream [README](https://github.com/acaprino/mukade/blob/main/README.md),
[specification](https://github.com/acaprino/mukade/blob/main/docs/session-interchange-spec.md),
[usage guide](https://github.com/acaprino/mukade/blob/main/docs/usage.md) and
[open points](https://github.com/acaprino/mukade/blob/main/next.md) remain authoritative for
later Mukade versions. Shinatama's [engine design](technical-design.md#6-the-hop) owns the user
journey and transaction semantics.

## 1. Boundary and dependency

Mukade reads a Claude Code, Codex CLI, OpenCode or Pi session, converts it to its intermediate
model, plans tool and reasoning conversion, checks the plan and writes a **new** native session
for one of those four harnesses. It returns a destination reference, resume command and manifest.
It does not move a running process, copy credentials or permissions, update Shinatama's database,
manage its instances, or guarantee that the destination understood the task.

The Git dependency is declared in the workspace and used in-process; it is not on crates.io at
the documented revision. Pin a reviewed commit and commit the resulting lockfile rather than
silently tracking a moving branch. The current pin:

```toml
[workspace.dependencies]
mukade = { git = "https://github.com/acaprino/mukade", tag = "v0.1.0", default-features = false }
```

`shinatama-engine` links it behind its `session-reader` feature, and `MUKADE_REVISION`
(`session_roots.rs`) carries the same value into the metadata worker protocol. The reviewed source
is exported offline by `scripts/mukade-pin.ps1` (manifest `scripts/mukade-pin.json`, snapshot
`.verification/mukade-pin-31feb1e`). Updating the pin requires rechecking the API, harness
versions and acceptance evidence. A Shinatama `.stx.jsonl`
export (engine 5.5.1) is a separate proposed format, not Mukade's interchange JSON; no direct
compatibility or automatic redaction between them is assumed.

## 2. Handover sequence and ownership

> **Closed route (decision 2026-11-03, user; ADR-0016 v2, ADR-0022 v2, Accepted 2026-11-04).** Every
> transfer is a host seed operation (ADR-0019); the sequence below is preserved as the re-entry
> design and describes no reachable Shinatama path.

The engine owns the hop state machine, `close`/`fork` semantics and source activation; the
adapter owns harness capabilities, instance configuration and interactive launch. Mukade owns
the conversion and the new destination session. A first transfer to another supported harness
uses `native_import` when the gates below pass; `native_resume` of an earlier segment plus
catch-up and the `seed_prompt` fallback remain host operations, not Mukade operations.

1. **Plan and preview.** Resolve the source native ID, source and destination stores, target
   executable, project directory, destination model/provider and tool capabilities. Select
   compaction and reasoning policies, check permissions and show what will be sent, generated
   or lost. The preview/privacy gates in section 5 must be met before enabling this route.
2. **Confirm and journal.** After user confirmation, record the `hop.id`, source reference and
   cursor, selected settings, target store and intended effect before invoking external work.
   Do not claim that a database transaction makes the native store write atomic with the journal.
3. **Convert and write.** With the destination store safe for writing, call `mukade::hop` with
   `resume: None`. It reads the source, applies compaction, plans and checks the output, writes
   a new session and returns a `Receipt`. Persist destination ID, location, native resume command
   and manifest immediately in the journal and associate them with the new segment. The manifest
   records source fingerprint, options, counts and losses; its exact fields belong to Mukade.
4. **Verify and start.** Shinatama's adapter verifies the written session in the intended store
   and starts its interactive instance with the correct environment and per-instance settings.
   Verify the selected content against the manifest and obtain the separate Shinatama nonce
   acknowledgement. A Mukade receipt proves a write was reported; it is not the nonce receipt,
   a list-and-resume check, or a successful interactive launch. `resume: Some(...)` instead runs
   a noninteractive prompt and may call a model; it is optional and does not replace the host
   acceptance checks. Do not run a headless resume and an interactive writer against the same
   native session concurrently.
5. **Switch or recover.** Compare the source cursor before switching and handle new turns using
   engine 6.3/6.6. Close mode eventually releases the source instance; fork mode creates a new
   Shinatama thread and leaves the source live. Neither mode edits or deletes the source session.
   Preserve the destination receipt even when launch or resume fails. If a crash leaves the write
   outcome uncertain, inspect the journal and store; do not call `hop()` again automatically:
   each successful call creates another session and Mukade has no host `hop.id` idempotency key.

Shinatama's acceptance criterion that a destination answer a history-only question must be tested
separately. The H27 harness probe and Mukade's own tests do not establish this host flow.

## 3. Rust contract and stores

The following illustrates a full-history hop without executing a destination prompt. Paths and
IDs are examples; construct roots from the actual resolved instances, not ambient defaults.

```rust
use std::path::PathBuf;
use mukade::hop::{HopError, HopRequest, SessionRef, WriteDetails};
use mukade::model::HarnessId;
use mukade::policy::ReasoningPolicy;
use mukade::roots::StoreRoots;

fn transfer() -> Result<(), HopError> {
    let roots = StoreRoots {
        claude_config_dir: Some(PathBuf::from(r"C:\scratch\claude")),
        pi_agent_dir: Some(PathBuf::from(r"C:\scratch\pi")),
        ..StoreRoots::default()
    };
    let request = HopRequest {
        source: SessionRef::id(HarnessId::ClaudeCode, "SOURCE_SESSION_ID"),
        dest: HarnessId::Pi,
        cwd: Some(r"C:\scratch\project".into()),
        compaction: Default::default(), // Compaction::None
        resume: None,
        reasoning: ReasoningPolicy::Drop, // Shinatama 5.1 excludes reasoning
        roots,
        write: WriteDetails::default(),
    };
    match mukade::hop(&request) {
        Ok(receipt) => {
            // Persist receipt.dest_id, location, resume_command and manifest before launch.
            println!("{}\n{}", receipt.dest_id, receipt.manifest);
            Ok(())
        }
        Err(HopError::ResumeFailed { reason, receipt }) => {
            // A destination exists: persist *receipt and recover it, never repeat the write.
            Err(HopError::ResumeFailed { reason, receipt })
        }
        Err(error) => Err(error),
    }
}
```

The comments mark required host persistence; the example only prints and does not implement a
journal. The `ResumeFailed` branch matters when a caller sets `resume: Some(ResumePrompt)`.
Other public entry points include `export`, `export_path`, `compact`, `import` and `hop_session`
for a host that needs to inspect or transform a session in stages. They require the caller to
preserve source facts, policy and manifest semantics; selecting them does not automatically
implement Shinatama's preview, redaction or recovery.

`StoreRoots` contains one location per harness: `claude_config_dir` (`CLAUDE_CONFIG_DIR`),
`codex_home` (`CODEX_HOME`), `opencode_db` (`OPENCODE_DB`, a database file) and `pi_agent_dir`
(`PI_CODING_AGENT_DIR`). Claude, Codex and Pi need explicit nonempty roots. For OpenCode, `None`
means its configured store, so a boxed instance must explicitly resolve and validate its database
and related `XDG_*` environment rather than accidentally target a user's store. Mukade's OpenCode
store commands use `--standalone` to avoid a background service with a different environment.
`StoreRoots::from_env()` is convenient for a controlled CLI environment, not a substitute for
per-instance resolution. At the pin `hop()` is `hop_into(req, &req.roots)`, and
`StoreRoots::root`/`set_root` name one harness's store (an existing OpenCode folder means its
`opencode.db`); `hop_into(&request, &destination)` reads and compacts with `request.roots` and
writes with a second `StoreRoots`, so a same-harness transfer between two distinct homes (box to
system, or into a portable JanuraCode) is representable at the library level (section 8). Either
way the transfer stays behind Shinatama's own verification (SS5), which has not been run.

`WriteDetails` supplies destination facts such as protocol, shell, offered tools, provider, model,
project ID, version and executable override. Its `dest_id` and `dest_path` can carry a destination
identity reserved and journaled by the host before the write; use them for reconciliation where the
writer supports them. Fill only facts established for the target instance;
incorrect protocol or tool assumptions can change the conversion. Source summary executable
overrides live in `SummarizeOpts`; OpenCode source selection also uses `MUKADE_OPENCODE_EXE`.
Resolve executables and construct child environments per instance; do not change process-wide
environment variables for concurrent hops. Confirm that Mukade-launched children actually receive
Shinatama's isolation, marker-removal and credential configuration before enabling the path.

## 4. Content and compaction policies

These policies apply only to Mukade's write path, which Shinatama does not call (closed route,
decision 2026-11-03; ADR-0016 v2). The seed document has its own budget and redaction rules
(ADR-0019; engine 5.5.1); the table stays as the re-entry reference.

| Request | Mukade behavior and host implications |
|---|---|
| `Compaction::None` | Transfers the selected full history, subject to conversion losses and provider constraints. It does not guarantee it fits the destination model's context. |
| `Portable { summary: SourceRecord, keep_from: SourceBoundary }` | Uses a readable source-native compaction summary and its retained tail. Refused if there is no usable summary or boundary. The summary is written as a user message, not a destination-native compaction record. |
| `Portable { summary: Supplied(text), keep_from: ... }` | Uses host-supplied summary text and `LastTurns(n)`, `Message(id)` or an applicable source boundary. The host owns its provenance, privacy and quality. |
| `Portable { summary: SourceModel(opts), keep_from: LastTurns(n) }` | Calls the source harness's model once using its credentials and configuration. The call can cost money and disclose conversation content to its provider; it occurs after consent. `SummarizeOpts` chooses model, timeout and source executable; default timeout is 300 seconds. `LastTurns(0)` keeps only the handoff summary. The CLI names this mode `--compaction summary`. |
| `DestinationNative` | The current library returns `Unsupported`. To use the destination's own compaction, first write the full session with `None`, then the host invokes that harness's compaction in its environment. This can incur a model call and requires the full history to fit. |

The source-model summary is a versioned Mukade handoff prompt, **not** the harness's `/compact`.
Claude uses no-session-persistence, Codex an ephemeral fork, Pi a temporary fork and OpenCode a
fork that Mukade attempts to delete with permissions denied. This avoids appending the summary to
the source transcript, but is not a guarantee of zero side effects: Claude hooks can write side
files, Codex's summary run can offer tools within its read-only sandbox, and an OpenCode fork can
remain if deletion fails. Surface the reported leftover and require explicit authorization before
any cleanup. Summary failure is reported before the destination write; preserve diagnostics.

Tool calls are translated where Mukade can preserve their meaning; otherwise they travel as text
and the manifest records the change. The active branch, not abandoned branches, normally travels;
Codex has an `--all-branches` CLI option with restrictions. Attachments, provider/model
compatibility and context limits require per-pair checks; a successful write alone does not prove
that a real provider will accept every payload.

Shinatama's canonical transcript and seed design exclude reasoning (engine 5.1 and 6.4), so this
integration explicitly selects `ReasoningPolicy::Drop`, not Mukade's `DropForeign` default.
Mukade itself can preserve compatible signed/encrypted reasoning or render it as labelled text;
using either would require a separate Shinatama policy decision. Never alter signed or encrypted
payloads. The manifest is sensitive and must be protected along with the session and any summary.

## 5. Preview, privacy and concurrency gates

These are **requirements to implement and prove**, not capabilities established by the current
Mukade call:

- Engine 6.1 requires the user to see exactly what is carried and lost before sending. `hop()`
  returns the manifest after writing, and a source-model summary does not exist before its paid
  call. A preliminary preview cannot be represented as the final manifest. Establish a staged
  preview/confirmation protocol that meets the approved requirement, including a separate consent
  before generating a summary and another confirmation if the exact result changes what is sent;
  until then this route is gated, not silently substituted.
- Engine 6.4 requires secret redaction before storage or transmission. Mukade retains raw native
  records and the native path must not assume the seed's redaction applies to them. Source-model
  summarization can transmit source content before a host can inspect the generated summary.
  Define and test a safe redaction/disclosure boundary for the complete selected data, including
  raw records, tool arguments and attachments; if it cannot be enforced, refuse that path.
- A source can continue while preparation happens in the host design, but this is not proof that
  a live read, native summary fork or target store write is consistent. Establish a stable source
  cursor/snapshot and compare it at switching; serialize writes to the target session/store as
  required by the harness and run the relevant fixtures. A `StoreRoots` value is not a lock.
- Preserve source and destination isolation and enforce the chosen box/system policy across child
  processes, caches, hooks and temporary files. The CLI documents `mukade-data` beside its
  executable, including temporary summary/import files; in-process placement, permissions,
  cleanup on crashes and concurrent runs need verification in the actual host deployment. Treat
  all intermediate material as sensitive; do not promise it disappears after a crash.

## 6. Errors and recovery

| Mukade error | Meaning | Shinatama action |
|---|---|---|
| `NotFound`, `Unsupported` | Lookup, store or option refused; no destination written. | Show the reason; fix configuration or offer an authorized fallback. |
| `SourceChanged { expected, actual }` | A staged caller's reviewed source fingerprint no longer matches; no destination written. | Discard the preview, recapture the source and require a new confirmation; never write from the stale selection. |
| `StrictCheckFailed`, `WriteFailed` | Plan or write failed; API reports no destination write. | Preserve diagnostics and source; investigate before retry. |
| `WriteUncertain { reason, dest_id, location }` | A destination may have committed before the import process failed. | Reconcile the returned/reserved id and location before any retry; never create another destination automatically. |
| `SummaryFailed` | The source-model summary failed; no destination session written. | Report failure, potential model cost and any reported cleanup; no partial handover. |
| `VerifyFailed` | Mukade could not read the source. This is **not** Shinatama `verify_import`. | Keep source active; diagnose the store/read failure. |
| `ResumeFailed { reason, receipt }` | Destination was written but optional prompt/resume failed or timed out. | Persist the embedded receipt and manifest, show the reason, recover that destination; never blindly re-import. |

Mukade maps these to CLI codes 2, 4 or 5; Shinatama's own journal and exit codes remain its
contract. A timeout can leave a written session when it happens after write. A process crash or
interrupted journal can leave the outcome uncertain even if no receipt reached the host: inspect
before retrying, never infer absence from missing host state alone. Do not automatically delete
native sessions, indexes or leftovers.

## 7. Evidence and acceptance gaps

- H27 (its record was removed from Mukade's documentation in the reorganization of commit
  `5ea6bd6`; Mukade's current records are indexed in its
  [docs/README.md](https://github.com/acaprino/mukade/blob/main/docs/README.md#verification-records))
  is a harness probe, including 36/36 reported writes/resumes and four destination-native
  compactions in that scenario. It used an external converter as a probe; it does not validate
  Shinatama's future in-process integration.
- [Summary live record](https://github.com/acaprino/mukade/blob/main/docs/verification/source-model-summary-live.md)
  reports real source-model summary hops from Claude, Pi and OpenCode; Codex used a mock because
  the real account hit its usage limit. Claude hooks did change side files. The result is scoped
  to those versions, sessions and configurations, not all pairs or providers.
- Mukade's [open points](https://github.com/acaprino/mukade/blob/main/next.md) include real
  Codex summary and encrypted-reasoning checks, provider-specific reasoning validation, large
  sessions, native compaction records, Codex multi-page history and subagent linkage. The recorded
  Codex quota reset date is historical, not evidence of a successful subsequent run.
- Before enabling native handover in Shinatama: test all allowed pair/version combinations in
  isolated scratch stores; verify roots and executable/environment isolation, content and losses,
  interactive resume and nonce, close/fork and cursor catch-up, preview/redaction, summary cost
  consent, failure before/after write, crash recovery, concurrent hops and provider rejection.
  Exercise real providers only with approved accounts and spending limits. Validate macOS/Linux
  and actual customer workflows separately; no such host tests have been run by this document.

## 8a. The pin advance to `31feb1e` (2026-11-16)

The workspace dependency (by its release tag `v0.1.0`), `MUKADE_REVISION` and `scripts/mukade-pin.json` now all name
`31feb1e97a05caa268c2cc3b003d05297db14ec3`, Mukade `master` and tag `v0.1.0`. Before this, the
dependency was at `f189a70` while `MUKADE_REVISION` and the pin manifest still named `14edbf3`.
Between `f189a70` and `31feb1e` only documentation changed (no file under `src/` or `Cargo.toml`);
`Cargo.lock` changes only the Mukade source line. The manifest's tree, `Cargo.toml` digest,
source digest and file count (374) were computed from `git archive` of the commit with the
script's rules; the offline snapshot under `.verification/` was not regenerated here (the script
needs PowerShell on Windows). `cargo check --locked` of `shinatama-engine` with `session-reader`,
`shinatama-desktop` and `shinatama-cli` passes for the Windows target; the qualification fields of
the manifest are carried over, not rerun.

The same day Mukade became embedded: `crates/mukade` (package `shinatama-mukade`, binary
`mukade`) builds Mukade's own program from this dependency with the `chat` feature, and
`build-release.bat` ships it as `mukade\mukade.exe` (the terminal browser `mukade-tui.exe` is
the unchanged file of the same release, v0.1.0, downloaded and checked against the SHA-256 in
`scripts/mukade-pin.json`). The program no longer comes from a Mukade checkout beside
Shinatama. `crates/mukade/tests/pin.rs` fails when the dependency, the manifest and
`MUKADE_REVISION` disagree.

## 8. The pin advance to `14edbf3` (2026-11-03)

The pin moved from `4124b1f` to `14edbf3cf71424a48b057e52c5532d6d4ab7d8a4` (Mukade `master` at
the advance; no committed revision is after the pin — the working tree carries pending work
listed at the end of this section). Everything the previous section 8 listed as
after the pin is now in it: the installation scan (`detect::scan_installations(root, max_depth)`,
`locate_installation(path, harness)`; CLI `mukade scan <dir> [--depth n]`, `--root` and
`--dest-root` on `list`, `dump`, `write` and `hop`), `StoreRoots::root`/`set_root` and `hop_into`
(section 3), Mukade's ADR-0001 (one recognition rule set, in the library) and the TUI work (no
Shinatama impact). Shinatama's `detect()` and `system_homes()` (engine 5.5 A) can now delegate
discovery to the scan instead of keeping a second rule set (main 5.14, 5.16). Committed after
that record and also in the pin: reader lists name sessions by their own titles (Pi by display
name or first prompt), reasoning keeps the complete provider proof through conversion, a
destination's model can come from its own installation, and an unrepresentable `exe` deadline
waits for exit instead of panicking.

Already in the previous pin and unchanged: `mukade harnesses` (a text inventory from the embedded
`catalog/harnesses.json`: binary locations, default stores and layouts per OS, the lineage each
harness records), `exe::resolve(harness, override)` (explicit override, then `MUKADE_*_EXE`, then
`PATH`, then well-known locations) and `exe::run_within(command, timeout)`, which stops a harness
child and everything it started at the deadline. `WriteDetails` also carries `dest_agent`,
`metadata`, `tool_map`, `without_tools` and `now` beyond the fields named in section 3.

Recheck performed for the advance (2026-11-03): the API surface Shinatama uses is unchanged or
additive — `reader/mod.rs` (`SessionInfo`, `Reader`, `Lookup`, `for_harness`), `model/kind.rs`
(`Kind`) and `model/tool.rs` (`ResultContent`) are untouched between the two pins,
`catalog::store_is_file` is present, and `StoreRoots` and `hop` gained only new members.
`cargo test -p shinatama-engine --features session-reader` passes at the new pin (110 unit tests,
0 failures); `scripts/mukade-pin.json` and the offline snapshot `.verification/mukade-pin-14edbf3`
were regenerated from the reviewed object (141 files, blob-verified against Git). Harness-version
and acceptance-evidence rechecks beyond that are not implied by this advance: SS5, now unblocked
against `hop_into`, and the reader rows stay open until run.

The OpenCode-specific advance in this pin (commit `7bae1aa`, with Mukade's own record in
`docs/verification/2026-11-02-opaque-reasoning.md`): the reader now preserves the **complete
provider reasoning proof** instead of the first detail only — the whole `native/reasoningDetails`
array from the Gateway as well as OpenRouter, a summary preceding the encrypted entry, several
opaque entries per part, the `data` or `encrypted` key, per-array protocol detection that refuses
mixed formats (`mixed-reasoning-details`) and recognizes `openai-native-responses-v1`.
`Opaque.chat` (`ChatReasoning { provider, details }`) carries the envelope through document I/O;
the scalar fields stay compatibility projections. `payload_fits` now refuses the envelope for a
non-OpenCode destination unless an exact scalar Anthropic conversion exists, the new
`chat_destination_fits` requires a receiving OpenCode provider to match the retained envelope's
provider (never relabel native OpenAI ciphertext as OpenRouter's), and the OpenCode writer writes
the retained array back. For Shinatama's current read-only surface none of this changes
behaviour: the detail projection omits reasoning by construction (`ReasoningOmitted`), find does
not search it, and RAW shows native records whose bytes are untouched. It matters post-v1, when
the section 4 `ReasoningPolicy::Drop` choice meets an OpenCode destination that can now
round-trip the full proof; keeping the envelope instead of dropping it is the separate Shinatama
policy decision section 4 already names.

Pending in Mukade's working tree on 2026-11-03 (**uncommitted**, therefore in no pin; listed so
the next recheck knows what to look at, as earlier revisions of this section did):
`Compaction::Native(NativeBudget)` and CLI `--compaction native` on `document` and `hop`
(Mukade spec §4.4). For an OpenCode destination it writes the whole history plus one
deterministic OpenCode `compaction` record with `metadata.keepID` — no model call anywhere; the
caller supplies the destination facts (`--context-window`, `--max-output-tokens`,
`--text-token-factor`, conservative chars/3 default), the summary is bounded per-user-message
truncation, and the `document` response reports the boundary (`keep_id`, estimated and projected
tokens, summarized counts, budget, `fits`). `dump` and non-OpenCode destinations refuse it. When
it is committed, pushed and pinned: section 4's table gains a `Native` row (the
`DestinationNative` row is unchanged — that variant still means the destination's own compaction,
run by the host), `ImportOpts` gains a `native_budget` field and `Compaction` a variant;
Shinatama constructs neither type today, so the read side is unaffected. Shinatama will not
consume this mode: ADR-0016 v2 (Accepted 2026-11-04) closes the native handover route; the
capability serves hosts that write OpenCode sessions, such as JanuraCode's session bridge.

Hermes Agent and DeepSeek Harness remain at Mukade's ADR-0002 (read-first proposal): the pin has
no reader for either, so section 9 is unchanged. Open in Mukade's own
[`next.md`](https://github.com/acaprino/mukade/blob/main/next.md) and relevant here: Codex
paginated threads (0.165) read from hand-made files only, Codex subagent threads not linked to
their root, native compaction records never written by a portable hop, the 100 MB read not
re-measured after the speed work. CX10 and SS2 inherit them.

## 9. Hermes Agent and DeepSeek Harness (requested 2026-10-31)

Shinatama hosts six agents since 2026-10-31 ([functional design](functional-design.md), goal
1); Mukade reads and writes four. The two new harnesses need Mukade work in its own repository
before Shinatama can search them or hop **from** them. Until a reviewed pin carries it, their
adapters declare `list_sessions`, `read_transcript` and `export` unsupported: they resume
natively within themselves and receive seeds, nothing more ([engine 5](technical-design.md#5-reading-the-agents-adapters-and-the-canonical-transcript)).
The store facts are in engine 5.3 and 5.7, researched from each project's source on
2026-10-31 and not yet observed on this machine.

| | Hermes Agent 0.22.5 | DeepSeek Harness 0.2.1-rc.5 |
|---|---|---|
| Root variable | `HERMES_HOME` (default `~/.hermes`, `%LOCALAPPDATA%\hermes` on Windows); one `state.db` per profile | `DSH_HOME` (default `~/.dsh`), plus `DSH_AGENTS_HOME` |
| Store | SQLite `state.db`, schema 32, WAL: `sessions` and `messages` tables | `sessions/--<cwd>--/<id>/session.v4.jsonl.zstd`: JSONL in checksummed Zstandard frames |
| Agent's own route | `hermes sessions export --format jsonl` (primary source, as OpenCode's export) | none on the command line; ACP `session/list` for listing |
| Lineage | `parent_session_id`; sub-agents marked `_delegate_from` in `model_config` | header `parentSession`, `isSeeded`, `origin: 'subagent'` |
| Compaction | in place: replaced rows kept with `active=0, compacted=1` | `compaction/*` records |
| Writer (post-v1) | Hermes's own `sessions import --from claude\|codex` exists for those two sources; a Mukade writer for the other pairs | no import of any kind; a Mukade writer would be the only route, and the store's single-writer lock must be respected |

Reader requirements are those of section 3 and engine 5.2: read-only, no lock (`session.lock`
included), the torn last record or frame ignored, unknown records counted. Fixtures come
first: register HM3 and DS3. Whether to add both readers at once or Hermes first (its export
route makes it the cheaper one) is Mukade's call; Shinatama's acceptance is per harness.
