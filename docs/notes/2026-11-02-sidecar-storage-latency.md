# Sidecar storage latency: inherited correction (2026-11-02)

Status: required specification amendment requested by the user on 2026-11-02.
Implementation and live verification exist in Musashi Terminal. **SC16 is still open
under Shinatama.** FRD-0009/R24 and implementation step C13 own its acceptance.
ADR-0029's single-source rule and ADR-0030's supervision rules are unchanged.

## Cause and correction to inherit

In the shared proxy process, synchronous SQLite persistence blocked HTTP and CONNECT
handling. Each session save deleted and recreated every CCR entry. Releasing content
then scanned six referencing tables without indexes; about 198,000 unrelated messages
made a trivial proxied request wait tens of seconds. Supervision prevents duplicate
proxies and false owner loss, but does not repair this persistence cost.

The owned Musashi source, `D:/Projects/horness-launcher/packages/billion-context`, fixes
both parts. Shinatama consumes the prepared source through the existing single-source
route; it must not carry a separate copy of the patch.

| Schema v9 index | Referencing column |
| --- | --- |
| `idx_messages_content_hash` | `messages(content_hash)` |
| `idx_blocks_summary_hash` | `blocks(summary_hash)` |
| `idx_blocks_one_hash` | `blocks(one_hash)` |
| `idx_checkpoints_state_hash` | `checkpoints(state_hash)` |
| `idx_ccr_entries_content_hash` | `ccr_entries(content_hash)` |
| `idx_message_media_hash` | `message_media(media_hash)` |

Migration adds the indexes and advances `user_version` to 9 without rewriting stored
rows. `UnifiedStore.syncCcrEntries` retains identical CCR rows and computes net changes
in blob references inside the existing ingestion transaction. Reordering, renaming and
swapping entries must not delete shared blobs; failed ingestion rolls back completely.

## Observed source evidence

The source report is
[`docs/verification/2026-11-02-storage-proxy-latency.md`](../../../horness-launcher/docs/verification/2026-11-02-storage-proxy-latency.md).
Canonical changed files are `src/storage/schema.ts`, `src/storage/store.ts` and the
repository's `test/storage-latency.test.mjs`. The deployed entry fingerprint is
`523a63a10a579e0ccd3fc268fdf1702d46657ff0d9bc21aca3a3c83ebe62301b`.
This is a SHA-256 of the deployed entry, not a release number or a Git commit.
The unchanged version label `0.1.207` alone cannot identify the correction.

| Observation | Before | After |
| --- | ---: | ---: |
| Save: 600 CCR replacements beside 200,000 unrelated messages | 13,525 ms | 38.8–56.9 ms |
| Real local CONNECT plus echo during that save | 13,574.6 ms | 58.6–92.3 ms |
| Live Google request through proxy | up to 30.70 s | 89–138 ms |
| Live local health response | 22.63 s | 2–15 ms |

The seven focused storage tests passed; the combined transport regression run passed
17 tests with one optional installed-Claude check skipped. Source type checking and
the runtime build passed. The activated database reported schema 9 and
`PRAGMA quick_check = ok`. An unauthenticated HTTPS probe to Google's IAP service
completed through the proxy in 224 ms; no VM SSH session or Vaultwarden upgrade was
performed by these probes.

These observations do not prove the cause of all earlier 15–71 s holds, remove every
CPU-bound task from the event loop, or qualify Shinatama's future launch topology.

## SC16 acceptance procedure

1. Record the owned source revision/content fingerprint and prepared artifact identity.
   Verify the six indexes and migration from schema v8 on a disposable populated copy,
   then repeat migration. Preserve every existing row and validate foreign keys and
   content reference counts. An actual user database upgrade requires a consistent,
   verified backup; do not copy a live SQLite file without its transactional state.
2. Run the inherited storage regression suite against that prepared build. Prove no
   full-table scan for any of the six content-reference checks, no blob mutation for
   unchanged/reordered/renamed CCR content, correct shared-session references and
   rollback after rejected writes, including repeated edits.
3. In Shinatama's actual launch path, replace 600 CCR entries beside 200,000 unrelated
   messages while a separate process probes the real local CONNECT echo target and
   local health endpoint. Each local response must complete in less than 750 ms.
   The existing source regression measures CONNECT plus echo; Shinatama qualification
   also instruments health. Use loopback targets to exclude internet variability and
   paid provider calls. Keep the proxy enabled and its ordinary timeouts unchanged.
4. With five instances working on long histories, measure event-loop delay, local
   transport latency and refused connections for one hour. Record and profile the
   longest holds, require no refused agent connection, and feed the result into SC5.
   Repeat the applicable checks on Windows, macOS and Linux and after sidecar upgrades.

The 750 ms limit is the controlled persistence fixture's regression threshold, not an
internet SLA. Record hardware, Node version, source identity and measured results.
Passing source tests alone does not close SC16; a busy status, longer timeout or proxy
bypass cannot replace these checks. Broader latency and topology decisions remain open.

## Janura Code applicability

Janura Code's distributed Billion component uses `packages/billion-flow` and a separate
compute process. The legacy `packages/billion-context/src/storage` code is absent from
both the compiled frontend and daemon inputs. The same SQLite migration is therefore
not needed there. See the [installed-artifact verification](../../../janura-code/docs/verification/2026-11-02-storage-latency-applicability.md).

## Requirement record boundary

The readable requirement is [FRD-0009/R24](../frd/FRD-0009-billioncontext-sidecar.md),
with [SC16](../verification-register.md) as its gate and C13 as its planned work.
The pre-existing Proposed FRD JSON records already have stale source spans, documented
in `docs/frd/README.md`; they were not re-extracted or promoted by this amendment.
No runtime or implementation checkpoint is marked complete by this specification edit.
