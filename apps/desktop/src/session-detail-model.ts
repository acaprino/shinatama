// Reading one session: what the worker answers for its detail, for a search in it and for one raw entry, checked against the request
// that asked (a reply for another session, another archive or another page is refused), and the words of its refusals. Pure.
import {
  digest, integer, nativeIdValue, record, rootValue, sameRoot, text, type SessionRootSelector,
} from './session-library-model.ts';

// --- the detail --------------------------------------------------------------------------------------------------------------

export type DetailCursor = { offset: number; fingerprint: string };
export type DetailRequest = { root: SessionRootSelector; nativeId: string; cursor: DetailCursor | null; limit: 50 };
export type EventKind = 'user_text' | 'assistant_text' | 'tool_call' | 'tool_result' | 'compaction' | 'meta' | 'attachment_omitted' | 'reasoning_omitted';
export type LineageState = 'unknown_parent' | 'unknown_links' | 'unresolved_in_selected_archive' | 'cycle' | 'ambiguous' | 'truncated' | 'resolved';
export type LineageNode = { nativeId: string; state: LineageState; canSelect: boolean };
export type Lineage = {
  nativeId: string; state: LineageState; selectedState: LineageState; ancestorState: LineageState; childrenState: LineageState;
  ancestors: LineageNode[]; children: LineageNode[]; totalDirectChildren: number | null;
};
export type DetailEvent = {
  ordinal: number; entryOrdinal: number; sourceEntryId: string | null; timestamp: string | null;
  kind: EventKind; text: string; truncated: boolean; payloadOmitted: boolean;
};
type Usage = { input: number; cacheRead: number; cacheWrite: number; output: number };
export type Ledger = {
  turns: number; requests: number; total: Usage; hitRatePermille: number | null; compactions: number; toolCalls: number;
  rerunsAfterCompaction: number; recordsWithoutUsage: number;
};
export type DetailPage = {
  root: SessionRootSelector; nativeId: string; harness: string; fingerprint: string; totalEvents: number;
  overview: { title: string | null; workingDirectory: string | null; createdAt: string | null; updatedAt: string | null; parentNativeId: string | null; harnessVersion: string | null };
  events: DetailEvent[]; next: DetailCursor | null; notices: string[]; lineage: Lineage; ledger: Ledger | null;
};

export const EVENT_KINDS: Record<EventKind, string> = {
  user_text: 'User', assistant_text: 'Assistant', tool_call: 'Tool call', tool_result: 'Tool result', compaction: 'Compaction summary',
  meta: 'Metadata', attachment_omitted: 'Attachment omitted', reasoning_omitted: 'Reasoning omitted',
};
export const DETAIL_ERRORS: Record<string, string> = {
  input_too_large: 'This archive holds more than 65,536 files. It was refused before reading.',
  invalid_request: 'This session request is invalid or unsupported.', root_unavailable: 'The selected archive is unavailable. No other archive was used.',
  root_changed: 'The archive revision changed. Reload the session catalog.', unsafe_namespace: 'The source failed read-only path verification.',
  unsupported_harness: 'This agent has no qualified detail reader.', unsupported_export_context: 'This detail needs a separately contained exporter that is not qualified.',
  session_not_found: 'The exact session ID is not listed in this archive.', ambiguous_session: 'The native reader cannot distinguish this exact session safely.',
  source_changed: 'The source changed. Reload the detail to start again.', read_unavailable: 'Session detail is unavailable.',
  output_limit: 'The detail response exceeded its fixed output limit.', cleanup_unconfirmed: 'Reader cleanup remains unconfirmed; its resources are still reserved.',
};
export const LINEAGE_STATES: Record<LineageState, string> = {
  unknown_parent: 'Unknown parent', unknown_links: 'Relationships unknown', unresolved_in_selected_archive: 'Not resolved in this archive',
  cycle: 'Cycle detected', ambiguous: 'Ambiguous reference', truncated: 'Fixed limit reached', resolved: 'Recorded in this archive',
};

const encoder = new TextEncoder();
const scalar = (value: unknown, cap: number): value is string => text(value, cap);
const nullable = (value: unknown, cap: number) => value === null || scalar(value, cap);
const fail = (code: string): never => { throw new Error(code); };

function lineageValue(value: unknown, id: string): Lineage {
  const state = (v: unknown): v is LineageState => typeof v === 'string' && Object.hasOwn(LINEAGE_STATES, v);
  if (!record(value) || value.nativeId !== id || !state(value.state) || !state(value.selectedState) || !state(value.ancestorState) || !state(value.childrenState)
    || (value.totalDirectChildren !== null && !integer(value.totalDirectChildren))) return fail('read_unavailable');
  const nodes = (items: unknown, cap: number): LineageNode[] => {
    if (!Array.isArray(items) || items.length > cap) return fail('read_unavailable');
    return items.map(item => {
      if (!record(item) || !scalar(item.nativeId, 256) || !item.nativeId || !state(item.state) || typeof item.canSelect !== 'boolean'
        || (item.canSelect && (item.state !== 'resolved' || !nativeIdValue(item.nativeId)))) return fail('read_unavailable');
      return { nativeId: item.nativeId, state: item.state, canSelect: item.canSelect };
    });
  };
  return {
    nativeId: id, state: value.state, selectedState: value.selectedState, ancestorState: value.ancestorState, childrenState: value.childrenState,
    ancestors: nodes(value.ancestors, 32), children: nodes(value.children, 50), totalDirectChildren: value.totalDirectChildren as number | null,
  };
}

function ledgerValue(value: unknown): Ledger | null {
  if (!record(value) || !record(value.total)) return null;
  const t = value.total;
  const n = (v: unknown) => integer(v) ? v : 0;
  return {
    turns: n(value.turns), requests: n(value.requests), total: { input: n(t.input), cacheRead: n(t.cacheRead), cacheWrite: n(t.cacheWrite), output: n(t.output) },
    hitRatePermille: integer(value.hitRatePermille) ? value.hitRatePermille : null, compactions: n(value.compactions), toolCalls: n(value.toolCalls),
    rerunsAfterCompaction: n(value.rerunsAfterCompaction), recordsWithoutUsage: n(value.recordsWithoutUsage),
  };
}

/** The page of a detail that answers `request`. Throws an `Error` whose message is the refusal's code. */
export function decodeDetail(value: unknown, request: DetailRequest): DetailPage {
  if (!record(value) || !rootValue(value.root) || !sameRoot(value.root, request.root) || value.nativeId !== request.nativeId) return fail('read_unavailable');
  if (value.issue !== null) return fail(typeof value.issue === 'string' && Object.hasOwn(DETAIL_ERRORS, value.issue) ? value.issue : 'read_unavailable');
  const page = value.page;
  if (!record(page) || !rootValue(page.root) || !sameRoot(page.root, request.root) || page.nativeId !== request.nativeId
    || typeof page.harness !== 'string' || !['claude_code', 'codex', 'pi'].includes(page.harness)
    || !digest(page.fingerprint) || !integer(page.totalEvents) || !record(page.overview) || !Array.isArray(page.events) || page.events.length > 50
    || !Array.isArray(page.notices) || page.notices.length > 6 || !page.notices.every(notice => scalar(notice, 240))) return fail('read_unavailable');
  if (request.cursor && page.fingerprint !== request.cursor.fingerprint) return fail('source_changed');
  const overview = page.overview;
  if (!nullable(overview.title, 512) || !nullable(overview.workingDirectory, 1024) || !nullable(overview.createdAt, 64)
    || !nullable(overview.updatedAt, 64) || !nullable(overview.parentNativeId, 256) || !nullable(overview.harnessVersion, 64)) return fail('read_unavailable');
  const offset = request.cursor?.offset ?? 0;
  const events: DetailEvent[] = [];
  for (const item of page.events) {
    if (!record(item) || item.ordinal !== offset + events.length || !integer(item.entryOrdinal) || !nullable(item.sourceEntryId, 256)
      || !nullable(item.timestamp, 64) || typeof item.kind !== 'string' || !Object.hasOwn(EVENT_KINDS, item.kind)
      || typeof item.text !== 'string' || item.text.length > 4096 || encoder.encode(item.text).length > 4096
      || typeof item.truncated !== 'boolean' || typeof item.payloadOmitted !== 'boolean') return fail('read_unavailable');
    events.push({
      ordinal: item.ordinal as number, entryOrdinal: item.entryOrdinal, sourceEntryId: item.sourceEntryId as string | null,
      timestamp: item.timestamp as string | null, kind: item.kind as EventKind, text: item.text, truncated: item.truncated, payloadOmitted: item.payloadOmitted,
    });
  }
  const end = offset + events.length;
  if (end > page.totalEvents || (!events.length && end < page.totalEvents)) return fail('read_unavailable');
  let next: DetailCursor | null = null;
  if (page.next !== null) {
    if (!record(page.next) || page.next.offset !== end || !integer(page.next.offset) || page.next.offset <= offset
      || page.next.offset >= page.totalEvents || page.next.fingerprint !== page.fingerprint) return fail('read_unavailable');
    next = { offset: end, fingerprint: page.fingerprint };
  }
  if (end < page.totalEvents && !next) return fail('read_unavailable');
  return {
    root: { ...request.root }, nativeId: request.nativeId, harness: page.harness, fingerprint: page.fingerprint, totalEvents: page.totalEvents,
    events, next, lineage: lineageValue(page.lineage, request.nativeId), ledger: ledgerValue(page.ledger), notices: [...page.notices] as string[],
    overview: {
      title: overview.title as string | null, workingDirectory: overview.workingDirectory as string | null, createdAt: overview.createdAt as string | null,
      updatedAt: overview.updatedAt as string | null, parentNativeId: overview.parentNativeId as string | null, harnessVersion: overview.harnessVersion as string | null,
    },
  };
}

export function detailFailure(error: unknown): string {
  const code = error instanceof Error ? error.message : typeof error === 'string' ? error : record(error) ? error.issue ?? error.code : null;
  return typeof code === 'string' && Object.hasOwn(DETAIL_ERRORS, code) ? DETAIL_ERRORS[code] : 'Session detail could not be loaded. Retry explicitly.';
}

/** "1–50 of 41 events", or what stands in its place before a page is read. */
export function positionText(offset: number, shown: number, total: number, noun: string): string {
  return `${total ? offset + 1 : 0}–${offset + shown} of ${total} ${noun}`;
}

const num = (value: number) => value.toLocaleString();

/** What the model usage of a session says, as the facts the overview lists. */
export function ledgerFacts(ledger: Ledger | null): { name: string; value: string }[] {
  if (!ledger || !ledger.requests) {
    return [{ name: 'Token usage', value: ledger?.recordsWithoutUsage ? 'This agent did not record usage in the session.' : 'No model requests recorded.' }];
  }
  const prompt = ledger.total.input + ledger.total.cacheRead + ledger.total.cacheWrite;
  return [
    { name: 'Model requests', value: `${num(ledger.requests)} over ${num(ledger.turns)} turns` },
    { name: 'Prompt tokens', value: `${num(prompt)} (uncached ${num(ledger.total.input)}, cache read ${num(ledger.total.cacheRead)}, cache write ${num(ledger.total.cacheWrite)})` },
    { name: 'Output tokens', value: num(ledger.total.output) },
    { name: 'Cache hit rate', value: ledger.hitRatePermille === null ? 'Unknown' : `${(ledger.hitRatePermille / 10).toFixed(1)} %` },
    { name: 'Compactions', value: `${ledger.compactions}; tool calls repeated after a compaction: ${ledger.rerunsAfterCompaction} of ${ledger.toolCalls}` },
    { name: 'Token figures', value: 'As recorded by the agent. No saving is claimed until ten sessions per agent are measured.' },
  ];
}

// --- a search inside one session ---------------------------------------------------------------------------------------------

export type FindCursor = { offset: number; fingerprint: string };
export type FindRequest = { detail: DetailRequest; query: string; cursor: FindCursor | null };
export type FindHit = { entryOrdinal: number; eventOrdinal: number; matchOrdinal: number; snippet: string; truncated: boolean };
export type FindPage = {
  root: SessionRootSelector; nativeId: string; sourceFingerprint: string; query: string; totalHits: number; hits: FindHit[]; next: FindCursor | null;
};
export type FindSelection = { root: SessionRootSelector; nativeId: string; entryOrdinal: number; eventOrdinal: number; fingerprint: string };

export const FIND_ERRORS: Record<string, string> = {
  invalid_query: 'Enter a non-empty literal query of at most 128 UTF-8 bytes.', fingerprint_required: 'Reload the session detail before searching.',
  output_limit: 'The search response exceeded its fixed limit.', invalid_request: 'This search selector is invalid.',
  source_changed: 'The session changed. Close search and reload its detail.', root_changed: 'The archive revision changed. Reload its catalog.',
  root_unavailable: 'The selected archive is unavailable. No other archive was searched.', unsafe_namespace: 'The archive failed read-only path verification.',
  input_too_large: 'This archive holds more than 65,536 files. It was refused before reading.',
  unsupported_harness: 'This agent has no qualified session reader.', unsupported_export_context: 'This native export context is not qualified.',
  session_not_found: 'The exact native session is unavailable.', ambiguous_session: 'This exact session cannot be distinguished safely.',
  read_unavailable: 'Session search is unavailable. Retry explicitly.', cleanup_unconfirmed: 'Reader cleanup remains unconfirmed; its resources stay reserved.',
};

export const validQuery = (query: string): boolean => !!query.trim() && encoder.encode(query).length <= 128 && !/[\u0000-\u001f\u007f-\u009f]/.test(query);

export function decodeFind(value: unknown, request: FindRequest): FindPage {
  if (!record(value) || !rootValue(value.root) || !sameRoot(value.root, request.detail.root) || value.nativeId !== request.detail.nativeId) return fail('read_unavailable');
  if (value.issue !== null) {
    if (!record(value.issue) || value.page !== null) return fail('read_unavailable');
    const code = value.issue.code === 'detail' ? value.issue.detail : value.issue.code;
    return fail(typeof code === 'string' && Object.hasOwn(FIND_ERRORS, code) ? code : 'read_unavailable');
  }
  const page = value.page;
  if (!record(page) || !rootValue(page.root) || !sameRoot(page.root, request.detail.root) || page.nativeId !== request.detail.nativeId
    || page.sourceFingerprint !== request.detail.cursor?.fingerprint || !digest(page.sourceFingerprint) || page.query !== request.query
    || !integer(page.totalHits) || !Array.isArray(page.hits) || page.hits.length > 50) return fail('read_unavailable');
  const offset = request.cursor?.offset ?? 0;
  const hits: FindHit[] = [];
  for (const hit of page.hits) {
    const prior = hits.at(-1);
    if (!record(hit) || !integer(hit.entryOrdinal) || !integer(hit.eventOrdinal) || hit.matchOrdinal !== offset + hits.length
      || typeof hit.snippet !== 'string' || hit.snippet.length > 512 || encoder.encode(hit.snippet).length > 512
      || typeof hit.truncated !== 'boolean' || (prior && (hit.entryOrdinal < prior.entryOrdinal || hit.eventOrdinal < prior.eventOrdinal))) return fail('read_unavailable');
    hits.push({ entryOrdinal: hit.entryOrdinal, eventOrdinal: hit.eventOrdinal, matchOrdinal: hit.matchOrdinal as number, snippet: hit.snippet, truncated: hit.truncated });
  }
  const end = offset + hits.length;
  if (end > page.totalHits || (!hits.length && end < page.totalHits)) return fail('read_unavailable');
  let next: FindCursor | null = null;
  if (page.next !== null) {
    if (!record(page.next) || page.next.offset !== end || !integer(page.next.offset) || page.next.offset <= offset
      || page.next.offset >= page.totalHits || page.next.fingerprint !== page.sourceFingerprint) return fail('read_unavailable');
    next = { offset: end, fingerprint: page.sourceFingerprint };
  }
  if (end < page.totalHits && !next) return fail('read_unavailable');
  return {
    root: { ...request.detail.root }, nativeId: request.detail.nativeId, sourceFingerprint: page.sourceFingerprint, query: request.query,
    totalHits: page.totalHits, hits, next,
  };
}

export function findFailure(error: unknown): string {
  const code = error instanceof Error ? error.message : '';
  return Object.hasOwn(FIND_ERRORS, code) ? FIND_ERRORS[code] : FIND_ERRORS.read_unavailable;
}

/** Whether `detail` may be searched: the first page of a verified detail. */
export function searchableDetail(detail: DetailRequest): boolean {
  return rootValue(detail.root) && nativeIdValue(detail.nativeId) && detail.limit === 50
    && record(detail.cursor) && detail.cursor.offset === 0 && digest(detail.cursor.fingerprint);
}

// --- one raw entry -----------------------------------------------------------------------------------------------------------

export type RawSelection = { root: SessionRootSelector; nativeId: string; entryOrdinal: number; fingerprint: string };
export type RawRequest = { detail: DetailRequest; entryOrdinal: number; confirmedRaw: boolean };
export const MAX_RAW_BYTES = 64 * 1024;

export const RAW_ERRORS: Record<string, string> = {
  confirmation_required: 'Confirm the privacy warning before loading RAW.', fingerprint_required: 'Reload the session detail before choosing RAW.',
  entry_not_found: 'This exact entry is unavailable. Reload the session detail.', raw_unavailable: 'The native reader has no RAW record for this canonical entry.',
  output_limit: 'The RAW entry exceeds 64 KiB and was refused.', source_changed: 'The source changed. Close RAW and reload the detail.',
  root_changed: 'The archive revision changed. Close RAW and reload the catalog.', root_unavailable: 'The selected archive is unavailable.',
  input_too_large: 'This archive holds more than 65,536 files. It was refused before reading.',
  unsafe_namespace: 'The source failed read-only path verification.', unsupported_export_context: 'This native export context is not qualified.',
  unsupported_harness: 'This agent has no qualified RAW reader.', ambiguous_session: 'This exact session cannot be distinguished safely.',
  session_not_found: 'The exact native session is unavailable in this archive.', read_unavailable: 'RAW could not be read. Retry explicitly.',
  invalid_request: 'This RAW selector is invalid.',
};

export function rawRequest(selection: RawSelection): RawRequest {
  return {
    detail: { root: { ...selection.root }, nativeId: selection.nativeId, cursor: { offset: 0, fingerprint: selection.fingerprint }, limit: 50 },
    entryOrdinal: selection.entryOrdinal, confirmedRaw: true,
  };
}

/** The raw JSON a reply holds for `selection`. Throws an `Error` whose message is the refusal's code. */
export function decodeRaw(value: unknown, selection: RawSelection): string {
  if (!record(value) || !rootValue(value.root) || !sameRoot(value.root, selection.root) || value.nativeId !== selection.nativeId
    || value.entryOrdinal !== selection.entryOrdinal) return fail('read_unavailable');
  if (value.issue !== null) {
    if (!record(value.issue)) return fail('read_unavailable');
    const code = value.issue.code === 'detail' ? value.issue.detail : value.issue.code;
    return fail(typeof code === 'string' && Object.hasOwn(RAW_ERRORS, code) ? code : 'read_unavailable');
  }
  if (value.sourceFingerprint !== selection.fingerprint) return fail('source_changed');
  if (typeof value.rawJson !== 'string' || !value.rawJson.length || value.rawJson.length > MAX_RAW_BYTES
    || encoder.encode(value.rawJson).length > MAX_RAW_BYTES) return fail('output_limit');
  return value.rawJson;
}

export function rawFailure(error: unknown): string {
  const code = error instanceof Error ? error.message : null;
  return typeof code === 'string' && Object.hasOwn(RAW_ERRORS, code) ? RAW_ERRORS[code] : RAW_ERRORS.read_unavailable;
}

/** Whether a selector may be read as a raw entry. */
export function rawSelectable(selection: RawSelection): boolean {
  return record(selection) && rootValue(selection.root) && nativeIdValue(selection.nativeId) && integer(selection.entryOrdinal) && digest(selection.fingerprint);
}
