// The answers of the browser for the commands of the sessions: the archives and their sessions, the conversation of one (a page at a
// time), the search in it, a raw entry, an export, the text search and its index, and the preview of a hop. Development only.
import type { Answers } from './index.ts';

const FINGERPRINT = 'a'.repeat(64);
const root = (storeRootId: string, harness: string, extra: Record<string, unknown> = {}) => ({
  storeRootId, configurationRevision: 1, harness, kind: 'box', access: 'read_only', available: true, ...extra,
});
export const ROOTS = [
  root('box-claude', 'claude_code', { label: 'Claude Code' }),
  root('box-codex', 'codex', { label: 'Codex' }),
  root('sys-pi', 'pi', { kind: 'system', label: 'Pi', available: false }),
];

const session = (nativeId: string, title: string | null, folder: string, updatedAt: string, turns: number, extra: Record<string, unknown> = {}) => ({
  nativeId, title, workingDirectory: folder, updatedAt, createdAt: '2026-11-20T10:00:00Z', turns, bytes: 120_000 + turns * 1000, parentNativeId: null, archived: false, metadataTruncated: false, ...extra,
});
const NOW = Date.now();
const ago = (minutes: number) => new Date(NOW - minutes * 60_000).toISOString();
export const SESSIONS: Record<string, ReturnType<typeof session>[]> = {
  'box-claude': [
    session('cc-retry', 'Retry backoff for the charge worker', 'C:\\work\\shinatama', ago(2), 41),
    session('cc-migration', 'Postgres 16 migration plan', 'C:\\work\\shinatama', ago(60 * 24), 18),
    session('cc-writers', 'Map the writers', 'C:\\work\\xiox', ago(60 * 48), 9, { parentNativeId: 'cc-migration' }),
    session('cc-untitled', null, 'C:\\work\\xiox', ago(60 * 150), 3),
  ],
  'box-codex': [
    session('cx-idem', 'Idempotency keys on /v1/charges', 'C:\\work\\shinatama', ago(180), 22),
    session('cx-invoice', 'Invoice PDF page breaks', 'C:\\work\\xiox', ago(60 * 52), 12),
    session('cx-dunning', 'Dunning email copy', 'C:\\work\\shinatama', ago(60 * 96), 7),
    session('cx-webhook', 'Webhook signature check', 'C:\\work\\shinatama', ago(60 * 120), 15),
    session('cx-tax', 'Tax rounding bug', 'C:\\work\\xiox', ago(60 * 130), 5),
  ],
};
/** The mock reads three sessions at a time, so that "Load older sessions" has something to load. */
const PAGE = 3;

const EVENTS = [
  { kind: 'user_text', text: 'Carry the retry policy to the webhook handler too.' },
  { kind: 'assistant_text', text: 'I read `verify.ts` and `retry.ts`. The handler can reuse `withRetry` with the same policy.' },
  { kind: 'tool_call', text: 'Read(src/webhooks/verify.ts)' },
  { kind: 'tool_result', text: '142 lines' },
  { kind: 'assistant_text', text: 'Done: the handler retries with the 30 s cap and keeps one key per delivery.' },
  { kind: 'compaction', text: 'Earlier turns were summarised by the agent.' },
  { kind: 'user_text', text: 'Thanks. Add a test for the idempotency key.' },
];

const detailFor = (request: { root: { storeRootId: string; configurationRevision: number }; nativeId: string; cursor: { offset: number; fingerprint: string } | null }) => {
  const offset = request.cursor?.offset ?? 0;
  const events = EVENTS.slice(offset, offset + 5).map((event, index) => ({
    ordinal: offset + index, entryOrdinal: offset + index + 10, sourceEntryId: `e${offset + index}`, timestamp: '2026-11-25T13:02:00Z',
    kind: event.kind, text: event.text, truncated: false, payloadOmitted: false,
  }));
  const end = offset + events.length;
  return {
    root: request.root, nativeId: request.nativeId, issue: null,
    page: {
      root: request.root, nativeId: request.nativeId, harness: request.root.storeRootId.includes('codex') ? 'codex' : 'claude_code', fingerprint: FINGERPRINT, totalEvents: EVENTS.length,
      overview: { title: 'Retry backoff for the charge worker', workingDirectory: 'C:\\work\\shinatama', createdAt: '2026-11-25T13:02:00Z', updatedAt: ago(2), parentNativeId: null, harnessVersion: '2.1.4' },
      events, next: end < EVENTS.length ? { offset: end, fingerprint: FINGERPRINT } : null, notices: [],
      lineage: { nativeId: request.nativeId, state: 'resolved', selectedState: 'resolved', ancestorState: 'unknown_parent', childrenState: 'resolved', ancestors: [], children: [{ nativeId: 'cc-writers', state: 'resolved', canSelect: true }], totalDirectChildren: 1 },
      ledger: { turns: 12, requests: 31, total: { input: 1200, cacheRead: 98000, cacheWrite: 4000, output: 5200 }, hitRatePermille: 940, compactions: 1, toolCalls: 18, rerunsAfterCompaction: 2, recordsWithoutUsage: 0 },
    },
  };
};

let enabled = true;
let documents = 1284;
let indexing: { phase: string } = { phase: 'idle' };

export const answers: Answers = {
  session_roots: ROOTS,
  session_list: args => {
    const request = (args?.request ?? {}) as { root: { storeRootId: string; configurationRevision: number }; cursor: { offset: number } | null };
    const all = SESSIONS[request.root.storeRootId] ?? [];
    const offset = request.cursor?.offset ?? 0;
    const rows = all.slice(offset, offset + PAGE);
    const end = offset + rows.length;
    const harness = ROOTS.find(one => one.storeRootId === request.root.storeRootId)?.harness ?? 'claude_code';
    return { root: request.root, harness, fingerprint: FINGERPRINT, total: all.length, rows, next: end < all.length ? { offset: end, fingerprint: FINGERPRINT } : null };
  },
  session_archive_add: args => ({ ...root('box-added', (args?.request as { harness: string }).harness, { label: 'Added archive' }) }),
  session_detail: args => detailFor(args?.request as Parameters<typeof detailFor>[0]),
  session_find: args => {
    const request = args?.request as { detail: { root: unknown; nativeId: string; cursor: { fingerprint: string } }; query: string; cursor: { offset: number } | null };
    const found = EVENTS.map((event, index) => ({ event, index })).filter(({ event }) => event.text.toLowerCase().includes(request.query.toLowerCase()));
    return {
      root: request.detail.root, nativeId: request.detail.nativeId, issue: null,
      page: {
        root: request.detail.root, nativeId: request.detail.nativeId, sourceFingerprint: request.detail.cursor.fingerprint, query: request.query, totalHits: found.length,
        hits: found.map(({ event, index }, at) => ({ entryOrdinal: index + 10, eventOrdinal: index, matchOrdinal: at, snippet: event.text, truncated: false })), next: null,
      },
    };
  },
  session_raw: args => {
    const request = args?.request as { detail: { root: unknown; nativeId: string; cursor: { fingerprint: string } }; entryOrdinal: number };
    return {
      root: request.detail.root, nativeId: request.detail.nativeId, entryOrdinal: request.entryOrdinal, sourceFingerprint: request.detail.cursor.fingerprint,
      rawJson: JSON.stringify({ type: 'assistant', uuid: `e${request.entryOrdinal}`, message: { role: 'assistant', content: 'native record' } }, null, 2), issue: null,
    };
  },
  session_export: () => ({
    state: 'saved', path: 'C:\\Users\\me\\Documents\\retry-backoff.stx.jsonl', bytes: 48_210, sha256: 'b'.repeat(64), attachments: 0,
    report: { items: 41, redactions: {}, omitted: { reasoning: 3, meta: 2, structured_results: 0 }, attachmentsCopied: 0, attachmentsNotCopied: 0, attachmentsReferenced: 0, declaration: { label: 'Shinatama STX v1', losses: ['Reasoning is not exported.'], lossless_backup: false } },
  }),
  session_cancel: false,
  session_search: args => {
    const query = String((args?.request as { query: string }).query);
    return [
      { storeRootId: 'box-codex', configurationRevision: 1, nativeId: 'cx-idem', harness: 'codex', title: 'Idempotency keys on /v1/charges', workingDirectory: 'C:\\work\\shinatama', updatedAt: ago(180), snippet: `…the «${query}» key is built inside the loop…`, sourceDeleted: false },
      { storeRootId: 'box-claude', configurationRevision: 1, nativeId: 'cc-retry', harness: 'claude_code', title: 'Retry backoff for the charge worker', workingDirectory: 'C:\\work\\shinatama', updatedAt: ago(2), snippet: `…keep the «${query}» between attempts…`, sourceDeleted: true },
    ];
  },
  session_index_status: () => ({ progress: { phase: indexing.phase, rootsTotal: 2, rootsDone: indexing.phase === 'running' ? 1 : 2, indexed: 12, skipped: 1272, failed: 0, unsupportedArchives: 0, markedSourceDeleted: 0, message: null }, index: { enabled, documents, sourceDeleted: 3, bodyBytes: 31_457_280 } }),
  session_index_enable: args => { enabled = args?.enabled === true; return null; },
  session_index_start: () => { indexing = { phase: 'running' }; setTimeout(() => { indexing = { phase: 'done' }; }, 600); return indexing; },
  session_index_cancel: () => { indexing = { phase: 'stopped' }; },
  session_index_forget: () => { const forgotten = documents; documents = 0; return forgotten; },
  sidecar_capabilities: { state: 'not_observed' },
  hop_preview: args => ({
    state: 'ready',
    preview: {
      hop_id: 'hop_demo', nonce: 'n0nce', destination: String((args?.request as { destination: string }).destination), channel: 'file', seed: 'SEED\n\n[user] Carry the retry policy to the webhook handler too.\n[assistant] Done.',
      seed_sha256: 'c'.repeat(64), bootstrap: 'Read the seed file and reply exactly: ready n0nce', bootstrap_sha256: 'd'.repeat(64), seed_path: 'C:\\work\\shinatama\\.git\\shinatama\\seed.md', full_path: 'C:\\work\\shinatama\\.git\\shinatama\\full.md',
      files_in_repository: true, report: { turns_total: 41, verbatim_turns: 38, digest_turns: 3, omitted_turns: 0, clipped_outputs: 2, bytes: 18_400, estimated_tokens: 4600, budget: { window_tokens: 200_000, seed_tokens: 24_000, seed_bytes: 96_000 } },
      redactions: {}, nested_seeds_dropped: 0, injected_context_dropped: 0, carried: ['The conversation, turn by turn', 'Six named files'], lost: ['Reasoning', 'Tool arguments over 4 KiB'],
    },
  }),
  hops_recent: [{ id: 'h1', state: 'committed', mode: 'seed', sourceAgent: 'claude_code', destinationAgent: 'codex', createdAtMs: NOW - 3_600_000, updatedAtMs: NOW - 3_000_000 }],
  instances_overview: [
    { id: 'i_aaaaaaaaaaaa', agent: 'claude_code', cwd: 'C:\\work\\shinatama', state: 'attention_requested', stateSource: 'terminal_signal', certain: false, git: { state: 'known', branch: 'feature/retry-backoff', dirty: 2, untracked: 1 }, usage: '' },
    { id: 'i_bbbbbbbbbbbb', agent: 'codex', cwd: 'C:\\work\\xiox', state: 'producing_output', stateSource: 'process', certain: true, git: { state: 'not_repository' }, usage: '' },
  ],
  instance_titles: { enabled: true, excluded: [], collisions: [{ kind: 'file_collision', worktree: 'C:\\work\\shinatama', path: 'src/retry.ts', instances: ['i_aaaaaaaaaaaa', 'i_bbbbbbbbbbbb'] }], rows: [{ instanceId: 'i_aaaaaaaaaaaa', threadId: null, title: 'Bash: git push origin feature/retry-backoff', reason: null, touched: 3, usage: { requests: 31, promptTokens: 103_200, outputTokens: 5200, hitRatePermille: 940 } }] },
  thread_summaries_pause: [],
};
