import { hasModal, uiPart, uiButton, uiInput, uiTraits } from '@acaprino/imago/ui';
import { sidePanelOpen, sideSurface } from './side-panel.ts';
import type { DetailRequest } from './session-detail.ts';
import type { SessionRootSelector } from './session-library.ts';
import { createProgressLine, type ProgressBackend } from './session-progress.ts';

export type FindCursor = { offset: number; fingerprint: string };
export type FindRequest = { detail: DetailRequest; query: string; cursor: FindCursor | null };
export type FindHit = { entryOrdinal: number; eventOrdinal: number; matchOrdinal: number; snippet: string; truncated: boolean };
export type FindPage = { root: SessionRootSelector; nativeId: string; sourceFingerprint: string; query: string;
  totalHits: number; hits: FindHit[]; next: FindCursor | null };
export type FindIssue = { code: 'invalid_query' | 'fingerprint_required' | 'output_limit' } | { code: 'detail'; detail: string };
export type FindReply = { root: SessionRootSelector; nativeId: string; page: FindPage | null; issue: FindIssue | null };
export type FindSelection = { root: SessionRootSelector; nativeId: string; entryOrdinal: number; eventOrdinal: number; fingerprint: string };
type Options = { read: (request: FindRequest, requestId: string) => Promise<FindReply>; reportError: (message: string) => void;
  onSelectHit: (selection: FindSelection) => void; restoreFocus?: () => void; progress?: ProgressBackend };
const encoder = new TextEncoder();
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const digest = (value: unknown): value is string => typeof value === 'string' && /^[a-f0-9]{64}$/i.test(value);
const nativeId = (value: unknown): value is string => typeof value === 'string' && /^[a-z0-9_-]{1,256}$/i.test(value);
const rootValue = (value: unknown): value is SessionRootSelector => record(value) && typeof value.storeRootId === 'string'
  && value.storeRootId.length > 0 && value.storeRootId.length <= 256 && !/[\u0000-\u001f\u007f]/.test(value.storeRootId)
  && integer(value.configurationRevision) && value.configurationRevision > 0;
const sameRoot = (left: SessionRootSelector, right: SessionRootSelector) => left.storeRootId === right.storeRootId
  && left.configurationRevision === right.configurationRevision;
const validQuery = (query: string) => !!query.trim() && encoder.encode(query).length <= 128 && !/[\u0000-\u001f\u007f-\u009f]/.test(query);
const ERRORS: Record<string, string> = {
  invalid_query: 'Enter a non-empty literal query of at most 128 UTF-8 bytes.', fingerprint_required: 'Reload the session detail before searching.',
  output_limit: 'The search response exceeded its fixed limit.', invalid_request: 'This search selector is invalid.',
  source_changed: 'The session changed. Close search and reload its detail.', root_changed: 'The archive revision changed. Reload its catalog.',
  root_unavailable: 'The selected archive is unavailable. No other archive was searched.', unsafe_namespace: 'The archive failed read-only path verification.',
  input_too_large: 'This archive holds more than 65,536 files. It was refused before reading.',
  unsupported_harness: 'This agent has no qualified session reader.', unsupported_export_context: 'This native export context is not qualified.',
  session_not_found: 'The exact native session is unavailable.', ambiguous_session: 'This exact session cannot be distinguished safely.',
  read_unavailable: 'Session search is unavailable. Retry explicitly.', cleanup_unconfirmed: 'Reader cleanup remains unconfirmed; its resources stay reserved.',
};
function decode(value: unknown, request: FindRequest): FindPage {
  if (!record(value) || !rootValue(value.root) || !sameRoot(value.root, request.detail.root) || value.nativeId !== request.detail.nativeId) throw new Error('read_unavailable');
  if (value.issue !== null) {
    if (!record(value.issue) || value.page !== null) throw new Error('read_unavailable');
    const code = value.issue.code === 'detail' ? value.issue.detail : value.issue.code;
    throw new Error(typeof code === 'string' && Object.hasOwn(ERRORS, code) ? code : 'read_unavailable');
  }
  const page = value.page;
  if (!record(page) || !rootValue(page.root) || !sameRoot(page.root, request.detail.root) || page.nativeId !== request.detail.nativeId
    || page.sourceFingerprint !== request.detail.cursor?.fingerprint || !digest(page.sourceFingerprint) || page.query !== request.query
    || !integer(page.totalHits) || !Array.isArray(page.hits) || page.hits.length > 50) throw new Error('read_unavailable');
  const offset = request.cursor?.offset ?? 0;
  const hits: FindHit[] = [];
  for (const hit of page.hits) {
    const prior = hits.at(-1);
    if (!record(hit) || !integer(hit.entryOrdinal) || !integer(hit.eventOrdinal) || hit.matchOrdinal !== offset + hits.length
      || typeof hit.snippet !== 'string' || hit.snippet.length > 512 || encoder.encode(hit.snippet).length > 512
      || typeof hit.truncated !== 'boolean' || (prior && (hit.entryOrdinal < prior.entryOrdinal || hit.eventOrdinal < prior.eventOrdinal))) throw new Error('read_unavailable');
    hits.push({ entryOrdinal: hit.entryOrdinal, eventOrdinal: hit.eventOrdinal, matchOrdinal: hit.matchOrdinal as number,
      snippet: hit.snippet, truncated: hit.truncated });
  }
  const end = offset + hits.length;
  if (end > page.totalHits || (!hits.length && end < page.totalHits)) throw new Error('read_unavailable');
  let next: FindCursor | null = null;
  if (page.next !== null) {
    if (!record(page.next) || page.next.offset !== end || !integer(page.next.offset) || page.next.offset <= offset
      || page.next.offset >= page.totalHits || page.next.fingerprint !== page.sourceFingerprint) throw new Error('read_unavailable');
    next = { offset: end, fingerprint: page.sourceFingerprint };
  }
  if (end < page.totalHits && !next) throw new Error('read_unavailable');
  return { root: { ...request.detail.root }, nativeId: request.detail.nativeId, sourceFingerprint: page.sourceFingerprint,
    query: request.query, totalHits: page.totalHits, hits, next };
}

/** No opening-time reads, live keystroke search, persistent index or resume. */
export function createSessionFind(options: Options) {
  const view = sideSurface('session-find-dialog', 'Find in this session', 'Close session search', 'session-find-title');
  const closeButton = view.closeButton;
  const source = uiPart('hint');
  const form = uiPart('form');
  const label = uiPart('label'); label.htmlFor = 'session-find-query'; label.textContent = 'Literal text';
  const query = uiInput(); query.id = 'session-find-query'; query.type = 'search'; query.maxLength = 128;
  query.autocomplete = 'off'; query.spellcheck = false; query.setAttribute('aria-describedby', 'session-find-scope');
  const search = uiButton(''); search.type = 'submit'; search.textContent = 'Search this session'; form.append(label, query, search);
  const status = uiPart('status'); status.classList.add('session-find-status'); status.setAttribute('role', 'status');
  const results = uiTraits(uiPart('orderedList'), 'scroll', 'bounded'); results.setAttribute('aria-label', 'Session search matches');
  const pager = uiPart('actions'); pager.classList.add('session-find-pagination');
  const previous = uiButton('Previous matches');
  const position = uiPart('text'); const next = uiButton('Next matches'); pager.append(previous, position, next);
  const scope = uiPart('hint'); scope.id = 'session-find-scope';
  scope.textContent = 'Searches canonical user, assistant, plain tool-result text and compaction summaries in this session. Case-insensitive literal matching; RAW, reasoning, tool arguments, structured results and attachment content are excluded. At most 50 matches per page. Choosing a match opens its saved transcript; it starts no agent.';
  const progress = createProgressLine('search', options.progress);
  view.body.append(source, form, status, progress.element, results, pager, scope);
  let selected: DetailRequest | null = null; let page: FindPage | null = null; let cursor: FindCursor | null = null;
  let history: (FindCursor | null)[] = []; let generation = 0; let busy = false; let destroyed = false; let failed = false;
  let message = 'Enter text and select Search this session.';
  function render() {
    if (destroyed) return;
    view.root.setAttribute('aria-busy', String(busy)); query.disabled = busy; search.disabled = busy || !selected || !validQuery(query.value);
    previous.disabled = busy || !page || !history.length; next.disabled = busy || !page?.next;
    source.textContent = selected ? `Session ${selected.nativeId} · Archive ${selected.root.storeRootId}` : '';
    status.textContent = busy ? 'Searching this verified session…' : message; status.dataset.state = failed ? 'error' : 'ready';
    results.replaceChildren();
    if (page) {
      position.textContent = page.totalHits ? `${(cursor?.offset ?? 0) + 1}–${(cursor?.offset ?? 0) + page.hits.length} of ${page.totalHits}` : '0 matches';
      for (const hit of page.hits) {
        const item = uiPart('item'); const button = uiButton('');
        button.classList.add('session-find-match'); button.disabled = busy;
        const locator = uiPart('text', `Match ${hit.matchOrdinal + 1} · Entry ${hit.entryOrdinal}`);
        const snippet = uiPart('text', hit.snippet);
        button.append(locator, snippet);
        if (hit.truncated) { const marker = uiPart('text', 'Excerpt limited to 512 bytes'); button.append(marker); }
        const owner = page;
        button.onclick = () => {
          if (busy || !view.isOpen() || page !== owner || !selected || query.value !== owner.query) return;
          const selection: FindSelection = { root: { ...owner.root }, nativeId: owner.nativeId, entryOrdinal: hit.entryOrdinal,
            eventOrdinal: hit.eventOrdinal, fingerprint: owner.sourceFingerprint };
          close(); options.onSelectHit(selection);
        };
        item.append(button); results.append(item);
      }
    } else position.textContent = '';
  }
  function clear() {
    generation++; selected = null; page = null; cursor = null; history = []; query.value = ''; failed = false;
    message = 'Enter text and select Search this session.'; render();
  }
  function close() { progress.cancel(); clear(); if (view.isOpen()) view.close(); }
  async function read(target: FindCursor | null, direction: 'new' | 'next' | 'previous') {
    if (destroyed || busy || !selected || !validQuery(query.value)) return;
    const at = generation; const oldCursor = cursor;
    const request: FindRequest = { detail: { ...selected, root: { ...selected.root }, cursor: { ...selected.cursor! } }, query: query.value, cursor: target };
    const requestId = progress.begin(); busy = true; failed = false; render();
    try {
      const decoded = decode(await options.read(request, requestId), request);
      if (destroyed || at !== generation || !view.isOpen()) return;
      if (direction === 'new') history = [];
      else if (direction === 'next') { if (history.length === 64) history.shift(); history.push(oldCursor); }
      else history.pop();
      page = decoded; cursor = target;
      message = decoded.totalHits ? 'Choose a match to open its saved transcript.' : 'No matching canonical text in this session.';
    } catch (error) {
      if (!destroyed && at === generation && view.isOpen()) {
        page = null; cursor = null; history = [];
        if (progress.cancelled()) { failed = false; message = 'Search cancelled. Enter text and select Search this session.'; }
        else {
          failed = true;
          const code = error instanceof Error ? error.message : '';
          message = Object.hasOwn(ERRORS, code) ? ERRORS[code] : ERRORS.read_unavailable; options.reportError(message);
        }
      }
    } finally {
      progress.end(requestId); busy = false;
      if (!destroyed && at !== generation && view.isOpen()) message = 'The previous search finished. Search again explicitly.';
      render();
    }
  }
  function show(detail: DetailRequest) {
    if (destroyed || !record(detail) || !rootValue(detail.root) || !nativeId(detail.nativeId) || detail.limit !== 50
      || !record(detail.cursor) || detail.cursor.offset !== 0 || !digest(detail.cursor.fingerprint)) {
      if (!destroyed) options.reportError('Reload the session detail before searching.'); return false;
    }
    if (!view.isOpen() && hasModal()) return false;
    clear(); selected = { root: { ...detail.root }, nativeId: detail.nativeId, limit: 50, cursor: { offset: 0, fingerprint: detail.cursor.fingerprint } };
    message = busy ? 'The previous search is still returning; no replacement was started.' : 'Enter text and select Search this session.';
    render(); if (!view.isOpen()) view.open(); if (busy) closeButton.focus(); else query.focus(); return true;
  }
  query.oninput = () => { generation++; page = null; cursor = null; history = []; failed = false; message = validQuery(query.value) ? 'Select Search this session.' : ERRORS.invalid_query; render(); };
  form.onsubmit = event => { event.preventDefault(); void read(null, 'new'); };
  previous.onclick = () => { if (history.length) void read(history.at(-1)!, 'previous'); };
  next.onclick = () => { if (page?.next) void read(page.next, 'next'); };
  closeButton.onclick = close;
  view.onClose(() => { progress.cancel(); clear(); if (!sidePanelOpen()) options.restoreFocus?.(); });
  render(); return { show, close, destroy: () => { close(); destroyed = true; progress.destroy(); view.root.remove(); } };
}
