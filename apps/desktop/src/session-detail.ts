import { openModal } from './modal.ts';
import type { SessionRootSelector } from './session-library.ts';
import './session-detail.css';

export type DetailCursor = { offset: number; fingerprint: string };
export type DetailRequest = { root: SessionRootSelector; nativeId: string; cursor: DetailCursor | null; limit: 50 };
type EventKind = 'user_text' | 'assistant_text' | 'tool_call' | 'tool_result' | 'compaction' | 'meta' | 'attachment_omitted' | 'reasoning_omitted';
export type DetailEvent = { ordinal: number; entryOrdinal: number; sourceEntryId: string | null; timestamp: string | null;
  kind: EventKind; text: string; truncated: boolean; payloadOmitted: boolean };
export type DetailPage = { root: SessionRootSelector; nativeId: string; harness: string; fingerprint: string; totalEvents: number;
  overview: { title: string | null; workingDirectory: string | null; createdAt: string | null; updatedAt: string | null; parentNativeId: string | null; harnessVersion: string | null };
  events: DetailEvent[]; next: DetailCursor | null; notices: string[] };
export type DetailReply = { root: SessionRootSelector; nativeId: string; page: DetailPage | null; issue: string | null };
type Options = { read: (request: DetailRequest) => Promise<DetailReply>; reportError: (message: string) => void; restoreFocus?: () => void };
const KINDS: Record<EventKind, string> = { user_text: 'User', assistant_text: 'Assistant', tool_call: 'Tool call', tool_result: 'Tool result',
  compaction: 'Compaction summary', meta: 'Metadata', attachment_omitted: 'Attachment omitted', reasoning_omitted: 'Reasoning omitted' };
const ERRORS: Record<string, string> = {
  input_too_large: 'This archive exceeds the 16 MiB detail input bound. It was refused before parsing.',
  invalid_request: 'This session request is invalid or unsupported.', root_unavailable: 'The selected archive is unavailable. No other archive was used.',
  root_changed: 'The archive revision changed. Reload the session catalog.', unsafe_namespace: 'The source failed read-only path verification.',
  unsupported_harness: 'This agent has no qualified detail reader.', unsupported_export_context: 'This detail needs a separately contained exporter that is not qualified.',
  session_not_found: 'The exact session ID is not listed in this archive.', ambiguous_session: 'The native reader cannot distinguish this exact session safely.',
  source_changed: 'The source changed. Reload the detail to start again.', read_unavailable: 'Session detail is unavailable.',
  output_limit: 'The detail response exceeded its fixed output limit.', cleanup_unconfirmed: 'Reader cleanup remains unconfirmed; its resources are still reserved.',
};
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const scalar = (value: unknown, cap: number): value is string => typeof value === 'string' && value.length <= cap && !/[\u0000-\u001f\u007f]/.test(value);
const nullable = (value: unknown, cap: number) => value === null || scalar(value, cap);
const nativeId = (value: unknown): value is string => typeof value === 'string' && /^[a-z0-9_-]{1,256}$/i.test(value);
const digest = (value: unknown): value is string => typeof value === 'string' && /^[a-f0-9]{64}$/i.test(value);
const rootValue = (value: unknown): value is SessionRootSelector => record(value) && scalar(value.storeRootId, 256) && value.storeRootId.length > 0
  && integer(value.configurationRevision) && value.configurationRevision > 0;
const sameRoot = (a: SessionRootSelector, b: SessionRootSelector) => a.storeRootId === b.storeRootId && a.configurationRevision === b.configurationRevision;
const encoder = new TextEncoder();
function decode(value: unknown, request: DetailRequest): DetailPage {
  if (!record(value) || !rootValue(value.root) || !sameRoot(value.root, request.root) || value.nativeId !== request.nativeId) throw new Error('read_unavailable');
  if (value.issue !== null) throw new Error(typeof value.issue === 'string' && Object.hasOwn(ERRORS, value.issue) ? value.issue : 'read_unavailable');
  const page = value.page;
  if (!record(page) || !rootValue(page.root) || !sameRoot(page.root, request.root) || page.nativeId !== request.nativeId
    || typeof page.harness !== 'string' || !['claude_code', 'codex', 'pi'].includes(page.harness)
    || !digest(page.fingerprint) || !integer(page.totalEvents) || !record(page.overview) || !Array.isArray(page.events) || page.events.length > 50
    || !Array.isArray(page.notices) || page.notices.length > 6 || !page.notices.every(notice => scalar(notice, 240))) throw new Error('read_unavailable');
  if (request.cursor && page.fingerprint !== request.cursor.fingerprint) throw new Error('source_changed');
  const overview = page.overview;
  if (!nullable(overview.title, 512) || !nullable(overview.workingDirectory, 1024) || !nullable(overview.createdAt, 64)
    || !nullable(overview.updatedAt, 64) || !nullable(overview.parentNativeId, 256) || !nullable(overview.harnessVersion, 64)) throw new Error('read_unavailable');
  const offset = request.cursor?.offset ?? 0;
  const events: DetailEvent[] = [];
  for (const item of page.events) {
    if (!record(item) || item.ordinal !== offset + events.length || !integer(item.entryOrdinal) || !nullable(item.sourceEntryId, 256)
      || !nullable(item.timestamp, 64) || typeof item.kind !== 'string' || !Object.hasOwn(KINDS, item.kind)
      || typeof item.text !== 'string' || item.text.length > 4096 || encoder.encode(item.text).length > 4096
      || typeof item.truncated !== 'boolean' || typeof item.payloadOmitted !== 'boolean') throw new Error('read_unavailable');
    events.push({ ordinal: item.ordinal as number, entryOrdinal: item.entryOrdinal, sourceEntryId: item.sourceEntryId as string | null,
      timestamp: item.timestamp as string | null, kind: item.kind as EventKind, text: item.text, truncated: item.truncated, payloadOmitted: item.payloadOmitted });
  }
  const end = offset + events.length;
  if (end > page.totalEvents || (!events.length && end < page.totalEvents)) throw new Error('read_unavailable');
  let next: DetailCursor | null = null;
  if (page.next !== null) {
    if (!record(page.next) || page.next.offset !== end || !integer(page.next.offset) || page.next.offset <= offset
      || page.next.offset >= page.totalEvents || page.next.fingerprint !== page.fingerprint) throw new Error('read_unavailable');
    next = { offset: end, fingerprint: page.fingerprint };
  }
  if (end < page.totalEvents && !next) throw new Error('read_unavailable');
  return { root: { ...request.root }, nativeId: request.nativeId, harness: page.harness, fingerprint: page.fingerprint,
    totalEvents: page.totalEvents, events, next, notices: [...page.notices] as string[], overview: {
      title: overview.title as string | null, workingDirectory: overview.workingDirectory as string | null,
      createdAt: overview.createdAt as string | null, updatedAt: overview.updatedAt as string | null,
      parentNativeId: overview.parentNativeId as string | null, harnessVersion: overview.harnessVersion as string | null } };
}

/** No startup read, HTML, images, URL navigation, transcript cache or launch action. */
export function createSessionDetail(options: Options) {
  const dialog = document.createElement('dialog'); dialog.id = 'session-detail-dialog'; dialog.setAttribute('aria-labelledby', 'session-detail-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'session-detail-title'; title.textContent = 'Session detail';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close session detail'); heading.append(title, closeButton);
  const source = document.createElement('p'); source.className = 'hint session-detail-source';
  const status = document.createElement('p'); status.id = 'session-detail-status'; status.setAttribute('role', 'status');
  const tools = document.createElement('div'); tools.className = 'session-detail-tools';
  const reload = document.createElement('button'); reload.type = 'button'; reload.textContent = 'Reload detail'; tools.append(reload);
  const panels = new Map<string, HTMLElement>(); let activeTab = 'transcript';
  for (const name of ['overview', 'transcript', 'lineage']) {
    const button = document.createElement('button'); button.type = 'button'; button.textContent = name[0].toUpperCase() + name.slice(1);
    button.onclick = () => { activeTab = name; render(); }; tools.append(button);
    const panel = document.createElement('section'); panel.setAttribute('aria-label', button.textContent); panels.set(name, panel);
  }
  const overview = document.createElement('dl'); overview.className = 'session-detail-overview'; panels.get('overview')!.append(overview);
  const events = document.createElement('div'); events.className = 'session-detail-events'; panels.get('transcript')!.append(events);
  const lineage = document.createElement('p'); lineage.className = 'hint'; panels.get('lineage')!.append(lineage);
  const pager = document.createElement('div'); pager.className = 'session-detail-pagination';
  const previous = document.createElement('button'); previous.type = 'button'; previous.textContent = 'Previous';
  const position = document.createElement('span'); const next = document.createElement('button'); next.type = 'button'; next.textContent = 'Next'; pager.append(previous, position, next);
  const notices = document.createElement('ul'); notices.className = 'session-detail-notices';
  const unavailable = document.createElement('p'); unavailable.className = 'hint'; unavailable.textContent = 'Raw view, export, resume and hop are separate and unavailable here. Attachments and remote content are never opened.';
  dialog.append(heading, source, status, tools, ...panels.values(), pager, notices, unavailable); (document.querySelector('#app') ?? document.body).append(dialog);
  let current: { root: SessionRootSelector; nativeId: string } | null = null;
  let page: DetailPage | null = null; let cursor: DetailCursor | null = null; let history: (DetailCursor | null)[] = [];
  let busy = false; let generation = 0; let destroyed = false; let failed = false; let message = 'Choose a session explicitly.';
  function field(label: string, value: string | number | null) {
    const key = document.createElement('dt'); key.textContent = label; const item = document.createElement('dd'); item.textContent = value === null || value === '' ? 'Unknown' : String(value); overview.append(key, item);
  }
  function render() {
    if (destroyed) return; dialog.setAttribute('aria-busy', String(busy));
    reload.disabled = busy || !current; previous.disabled = busy || !history.length; next.disabled = busy || !page?.next;
    source.textContent = current ? `Session ${current.nativeId} · Archive ${current.root.storeRootId} · revision ${current.root.configurationRevision}` : '';
    status.textContent = busy ? 'Reading bounded session detail…' : message; status.dataset.state = failed ? 'error' : 'ready';
    for (const [name, panel] of panels) panel.hidden = name !== activeTab;
    overview.replaceChildren(); events.replaceChildren(); notices.replaceChildren();
    if (page) {
      field('Harness', page.harness); field('Harness version', page.overview.harnessVersion); field('Title', page.overview.title);
      field('Working folder', page.overview.workingDirectory); field('Created', page.overview.createdAt); field('Updated', page.overview.updatedAt);
      for (const event of page.events) {
        const article = document.createElement('article'); const label = document.createElement('h2'); label.textContent = KINDS[event.kind];
        const provenance = document.createElement('small'); provenance.textContent = `Entry ${event.entryOrdinal} · ID ${event.sourceEntryId ?? 'Unknown'} · time ${event.timestamp ?? 'Unknown'}`;
        const body = document.createElement('pre'); body.textContent = event.text || '(No visible text recorded)'; article.append(label, provenance, body);
        if (event.truncated || event.payloadOmitted) { const note = document.createElement('p'); note.className = 'hint'; note.textContent = event.truncated ? 'Displayed text is truncated at its fixed limit.' : 'Sensitive or raw payload is omitted.'; article.append(note); }
        events.append(article);
      }
      for (const notice of page.notices) { const item = document.createElement('li'); item.textContent = notice; notices.append(item); }
    }
    lineage.textContent = `Parent session ID: ${page?.overview.parentNativeId ?? 'Unknown'}. Only the recorded parent is shown; lineage navigation is not qualified.`;
    position.textContent = page ? `${page.totalEvents ? (cursor?.offset ?? 0) + 1 : 0}–${(cursor?.offset ?? 0) + page.events.length} of ${page.totalEvents} events${!history.length && (cursor?.offset ?? 0) > 0 ? ' · Reload for first page' : ''}` : 'No detail loaded';
  }
  async function load(requested: DetailCursor | null, direction: 'reload' | 'next' | 'previous') {
    if (busy || !current) return;
    const at = generation; const request: DetailRequest = { root: { ...current.root }, nativeId: current.nativeId, cursor: requested, limit: 50 };
    busy = true; failed = false; render();
    try {
      const received = await options.read(request);
      if (destroyed || at !== generation || !dialog.open) return;
      const value = decode(received, request);
      if (direction === 'reload') history = []; else if (direction === 'next') { history.push(cursor); if (history.length > 64) history.shift(); } else history.pop();
      page = value; cursor = requested; message = page.totalEvents ? 'Read-only projection. Raw and omitted payloads are not requested.' : 'No canonical events were returned.';
    } catch (error) {
      if (!destroyed && at === generation && dialog.open) { page = null; cursor = null; history = []; failed = true;
        const code = error instanceof Error ? error.message : typeof error === 'string' ? error : record(error) ? error.issue ?? error.code : null;
        message = typeof code === 'string' && Object.hasOwn(ERRORS, code) ? ERRORS[code] : 'Session detail could not be loaded. Retry explicitly.'; options.reportError(message); }
    } finally { busy = false; if (!destroyed && at !== generation && dialog.open) message = 'The previous read finished. Reload the selected session explicitly.'; render(); }
  }
  function close() { generation++; current = null; page = null; cursor = null; history = []; render(); if (dialog.open) dialog.close(); }
  function show(root: SessionRootSelector, id: string) {
    if (destroyed || !rootValue(root) || !nativeId(id)) { if (!destroyed) options.reportError('This session selector cannot be read safely.'); return false; }
    const other = [...document.querySelectorAll<HTMLDialogElement>('dialog[open]')].at(-1);
    if (!dialog.open && other && other.id !== 'session-library-dialog') return false;
    generation++; current = { root: { storeRootId: root.storeRootId, configurationRevision: root.configurationRevision }, nativeId: id };
    page = null; cursor = null; history = []; failed = false; activeTab = 'transcript'; message = busy ? 'The previous read is still returning; no replacement was started.' : 'Reading the selected session…';
    render(); if (!dialog.open) openModal(dialog); closeButton.focus(); if (!busy) void load(null, 'reload'); return true;
  }
  reload.onclick = () => { void load(null, 'reload'); }; previous.onclick = () => { if (history.length) void load(history.at(-1) ?? null, 'previous'); };
  next.onclick = () => { if (page?.next) void load(page.next, 'next'); }; closeButton.onclick = close;
  dialog.addEventListener('close', () => { if (!dialog.open) { generation++; current = null; page = null; cursor = null; history = []; render(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); } });
  render(); return { show, close, destroy: () => { destroyed = true; close(); dialog.remove(); } };
}
