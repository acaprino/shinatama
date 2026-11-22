import { uiPart, uiButton, uiTraits } from '@acaprino/imago/ui';
import { closeControl } from './controls.ts';
import { openModal, topModal } from './modal.ts';
import type { SessionRootSelector } from './session-library.ts';
import type { RawSelection } from './session-raw.ts';
import { nextCapabilityLines, parseCapabilities, type CapabilityState } from './sidecar-status.ts';

export type DetailCursor = { offset: number; fingerprint: string };
export type DetailRequest = { root: SessionRootSelector; nativeId: string; cursor: DetailCursor | null; limit: 50 };
type EventKind = 'user_text' | 'assistant_text' | 'tool_call' | 'tool_result' | 'compaction' | 'meta' | 'attachment_omitted' | 'reasoning_omitted';
type LineageState = 'unknown_parent' | 'unknown_links' | 'unresolved_in_selected_archive' | 'cycle' | 'ambiguous' | 'truncated' | 'resolved';
type LineageNode = {nativeId:string;state:LineageState;canSelect:boolean};
type Lineage = {nativeId:string;state:LineageState;selectedState:LineageState;ancestorState:LineageState;childrenState:LineageState;ancestors:LineageNode[];children:LineageNode[];totalDirectChildren:number|null};
export type DetailEvent = { ordinal: number; entryOrdinal: number; sourceEntryId: string | null; timestamp: string | null;
  kind: EventKind; text: string; truncated: boolean; payloadOmitted: boolean };
export type DetailPage = { root: SessionRootSelector; nativeId: string; harness: string; fingerprint: string; totalEvents: number;
  overview: { title: string | null; workingDirectory: string | null; createdAt: string | null; updatedAt: string | null; parentNativeId: string | null; harnessVersion: string | null };
  events: DetailEvent[]; next: DetailCursor | null; notices: string[]; lineage: Lineage; ledger: Ledger | null };
type Usage = { input: number; cacheRead: number; cacheWrite: number; output: number };
type Ledger = { turns: number; requests: number; total: Usage; hitRatePermille: number | null; compactions: number; toolCalls: number;
  rerunsAfterCompaction: number; recordsWithoutUsage: number };
export type DetailReply = { root: SessionRootSelector; nativeId: string; page: DetailPage | null; issue: string | null };
type Options = { read: (request: DetailRequest) => Promise<DetailReply>; capabilities?: (nativeId: string) => Promise<unknown>; openRaw?: (selection:RawSelection)=>void;
  openFind?: (request:DetailRequest)=>void; openExport?: (selection:{root:SessionRootSelector;nativeId:string;fingerprint:string;harness:string})=>void;
  openHop?: (selection:{root:SessionRootSelector;nativeId:string;fingerprint:string;harness:string})=>void;
  /** Where the conversation was opened from; a way back is shown when given. */
  back?: { label: string; open: () => void };
  reportError: (message: string) => void; restoreFocus?: () => void };
const KINDS: Record<EventKind, string> = { user_text: 'User', assistant_text: 'Assistant', tool_call: 'Tool call', tool_result: 'Tool result',
  compaction: 'Compaction summary', meta: 'Metadata', attachment_omitted: 'Attachment omitted', reasoning_omitted: 'Reasoning omitted' };
const ERRORS: Record<string, string> = {
  input_too_large: 'This archive holds more than 65,536 files. It was refused before reading.',
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
const CAPABILITY_POLL_MS = 5000;
const CAPABILITY_TIMEOUT_MS = 4000;
const lineageStates: Record<LineageState,string> = {unknown_parent:'Unknown parent',unknown_links:'Relationships unknown',unresolved_in_selected_archive:'Not resolved in this archive',cycle:'Cycle detected',ambiguous:'Ambiguous reference',truncated:'Fixed limit reached',resolved:'Recorded in this archive'};
function lineageValue(value:unknown,id:string):Lineage {
  const state=(v:unknown):v is LineageState=>typeof v==='string'&&Object.hasOwn(lineageStates,v);
  if(!record(value)||value.nativeId!==id||!state(value.state)||!state(value.selectedState)||!state(value.ancestorState)||!state(value.childrenState)
    ||(value.totalDirectChildren!==null&&!integer(value.totalDirectChildren))) throw new Error('read_unavailable');
  const nodes=(items:unknown,cap:number):LineageNode[]=>{
    if(!Array.isArray(items)||items.length>cap) throw new Error('read_unavailable');
    return items.map(item=>{
      if(!record(item)||!scalar(item.nativeId,256)||!item.nativeId||!state(item.state)||typeof item.canSelect!=='boolean'
        ||(item.canSelect&&(item.state!=='resolved'||!nativeId(item.nativeId)))) throw new Error('read_unavailable');
      return {nativeId:item.nativeId,state:item.state,canSelect:item.canSelect};
    });
  };
  return {nativeId:id,state:value.state,selectedState:value.selectedState,ancestorState:value.ancestorState,childrenState:value.childrenState,
    ancestors:nodes(value.ancestors,32),children:nodes(value.children,50),totalDirectChildren:value.totalDirectChildren as number|null};
}
function ledgerValue(value: unknown): Ledger | null {
  if (!record(value) || !record(value.total)) return null;
  const t = value.total; const n = (v: unknown) => integer(v) ? v : 0;
  return { turns: n(value.turns), requests: n(value.requests), total: { input: n(t.input), cacheRead: n(t.cacheRead), cacheWrite: n(t.cacheWrite), output: n(t.output) },
    hitRatePermille: integer(value.hitRatePermille) ? value.hitRatePermille : null, compactions: n(value.compactions), toolCalls: n(value.toolCalls),
    rerunsAfterCompaction: n(value.rerunsAfterCompaction), recordsWithoutUsage: n(value.recordsWithoutUsage) };
}
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
    totalEvents: page.totalEvents, events, next, lineage:lineageValue(page.lineage,request.nativeId), ledger: ledgerValue(page.ledger), notices: [...page.notices] as string[], overview: {
      title: overview.title as string | null, workingDirectory: overview.workingDirectory as string | null,
      createdAt: overview.createdAt as string | null, updatedAt: overview.updatedAt as string | null,
      parentNativeId: overview.parentNativeId as string | null, harnessVersion: overview.harnessVersion as string | null } };
}

/** No startup read, HTML, images, URL navigation, transcript cache or launch action. */
export function createSessionDetail(options: Options) {
  const dialog = uiPart('dialog'); dialog.id = 'session-detail-dialog'; dialog.setAttribute('aria-labelledby', 'session-detail-title');
  const heading = uiPart('header'); heading.classList.add('dialog-heading');
  const title = uiPart('title'); title.id = 'session-detail-title'; title.textContent = 'Session detail';
  const closeButton = closeControl('Close session detail'); heading.append(title, closeButton);
  if (options.back) {
    const back = uiButton(`‹ ${options.back.label}`);
    back.setAttribute('aria-label', `Back to ${options.back.label}`); back.onclick = () => { dialog.close(); options.back!.open(); }; heading.prepend(back);
  }
  const source = uiPart('hint');
  const status = uiPart('status'); status.id = 'session-detail-status'; status.setAttribute('role', 'status');
  const tools = uiPart('toolbar');
  const reload = uiButton('Reload detail'); tools.append(reload);
  const find = uiButton('Find in conversation');
  if (options.openFind) tools.append(find);
  const exportButton = uiButton('Export…');
  if (options.openExport) tools.append(exportButton);
  const hopButton = uiButton('Hop…');
  if (options.openHop) tools.append(hopButton);
  const panels = new Map<string, HTMLElement>(); let activeTab = 'transcript';
  for (const name of ['overview', 'transcript', 'lineage']) {
    const button = uiButton(name[0].toUpperCase() + name.slice(1));
    button.onclick = () => { activeTab = name; render(); }; tools.append(button);
    const panel = uiPart('region'); panel.setAttribute('aria-label', button.textContent); panels.set(name, panel);
  }
  const overview = uiPart('descriptionList'); overview.classList.add('session-detail-overview'); panels.get('overview')!.append(overview);
  const events = uiTraits(uiPart('stack'), 'scroll', 'bounded'); events.classList.add('session-detail-events'); panels.get('transcript')!.append(events);
  const lineage = uiTraits(uiPart('stack'), 'scroll', 'bounded'); panels.get('lineage')!.append(lineage);
  const pager = uiPart('actions'); pager.classList.add('session-detail-pagination');
  const previous = uiButton('Previous');
  const position = uiPart('text'); const next = uiButton('Next'); pager.append(previous, position, next);
  const notices = uiPart('list');
  const unavailable = uiPart('hint', 'RAW requires an explicit privacy confirmation. Export redacts secrets and never includes reasoning. Attachment files stay closed.');
  dialog.append(heading, source, status, tools, ...panels.values(), pager, notices, unavailable); (document.querySelector('#app') ?? document.body).append(dialog);
  let current: { root: SessionRootSelector; nativeId: string } | null = null;
  let page: DetailPage | null = null; let cursor: DetailCursor | null = null; let history: (DetailCursor | null)[] = [];
  let busy = false; let generation = 0; let destroyed = false; let failed = false; let message = 'Choose a session explicitly.';
  // Per-session proxy capabilities: read on open, on becoming visible again and every CAPABILITY_POLL_MS while this detail
  // is open and visible, never while closed. A reply for a session that is no longer the open one, or one that came after
  // CAPABILITY_TIMEOUT_MS, is dropped. A tick never calls render(): it replaces only the "Model proxy" value, and only when
  // the lines changed, so open tool calls, focus and text selection in the transcript are left alone.
  let proxyLines: string[] | null = null; let proxyGeneration = 0; let proxyTimer = 0; let proxyInFlight = -1;
  let proxyKey: HTMLElement | null = null; let proxyItem: HTMLElement | null = null;
  function paintProxy() {
    if (!proxyLines || !page) { proxyKey?.remove(); proxyItem?.remove(); proxyKey = proxyItem = null; return; }
    if (!proxyKey || !proxyItem || !proxyItem.isConnected) {
      proxyKey = uiPart('term', 'Model proxy'); proxyItem = uiPart('definition'); overview.append(proxyKey, proxyItem);
    }
    proxyItem.replaceChildren(...proxyLines.map(text => uiPart('paragraph', text)));
  }
  const onVisible = () => { if (!document.hidden) void readProxy(); };
  function stopProxyReads() {
    proxyGeneration++; proxyInFlight = -1; proxyLines = null; if (proxyTimer) window.clearInterval(proxyTimer); proxyTimer = 0;
    document.removeEventListener('visibilitychange', onVisible); paintProxy();
  }
  async function readProxy() {
    if (destroyed || !options.capabilities || !current || !dialog.open || document.hidden || proxyInFlight === proxyGeneration) return;
    const at = proxyGeneration; const id = current.nativeId; proxyInFlight = at;
    let read: CapabilityState; let timer = 0;
    try {
      const expired = new Promise<unknown>(resolve => { timer = window.setTimeout(() => resolve(null), CAPABILITY_TIMEOUT_MS); });
      read = parseCapabilities(await Promise.race([options.capabilities(id), expired]));
    } catch { read = { state: 'unavailable' }; } finally { window.clearTimeout(timer); }
    if (proxyInFlight === at) proxyInFlight = -1;
    if (destroyed || at !== proxyGeneration || !current || current.nativeId !== id || !dialog.open) return;
    const lines = nextCapabilityLines(proxyLines, read);
    if (lines) { proxyLines = lines; paintProxy(); }
  }
  function startProxyReads() {
    stopProxyReads(); if (!options.capabilities) return;
    document.addEventListener('visibilitychange', onVisible);
    proxyTimer = window.setInterval(() => { void readProxy(); }, CAPABILITY_POLL_MS); void readProxy();
  }
  function field(label: string, value: string | number | null) {
    const key = uiPart('term', label); const item = uiPart('definition', value === null || value === '' ? 'Unknown' : String(value)); overview.append(key, item);
  }
  function render() {
    if (destroyed) return; dialog.setAttribute('aria-busy', String(busy));
    reload.disabled = busy || !current; previous.disabled = busy || !history.length; next.disabled = busy || !page?.next;
    find.disabled = busy || !page; exportButton.disabled = busy || !page; hopButton.disabled = busy || !page;
    source.textContent = current ? `Session ${current.nativeId} · Archive ${current.root.storeRootId} · revision ${current.root.configurationRevision}` : '';
    status.textContent = busy ? 'Reading bounded session detail…' : message; status.dataset.state = failed ? 'error' : 'ready';
    for (const [name, panel] of panels) panel.hidden = name !== activeTab;
    overview.replaceChildren(); events.replaceChildren(); notices.replaceChildren();
    if (page) {
      const shownPage=page;
      field('Harness', page.harness); field('Harness version', page.overview.harnessVersion); field('Title', page.overview.title);
      field('Working folder', page.overview.workingDirectory); field('Created', page.overview.createdAt); field('Updated', page.overview.updatedAt);
      const ledger = page.ledger; const num = (v: number) => v.toLocaleString();
      if (ledger && ledger.requests) {
        const prompt = ledger.total.input + ledger.total.cacheRead + ledger.total.cacheWrite;
        field('Model requests', `${num(ledger.requests)} over ${num(ledger.turns)} turns`);
        field('Prompt tokens', `${num(prompt)} (uncached ${num(ledger.total.input)}, cache read ${num(ledger.total.cacheRead)}, cache write ${num(ledger.total.cacheWrite)})`);
        field('Output tokens', num(ledger.total.output));
        field('Cache hit rate', ledger.hitRatePermille === null ? 'Unknown' : `${(ledger.hitRatePermille / 10).toFixed(1)} %`);
        field('Compactions', `${ledger.compactions}; tool calls repeated after a compaction: ${ledger.rerunsAfterCompaction} of ${ledger.toolCalls}`);
        field('Token figures', 'As recorded by the agent. No saving is claimed until ten sessions per agent are measured.');
      } else field('Token usage', ledger?.recordsWithoutUsage ? 'This agent did not record usage in the session.' : 'No model requests recorded.');
      proxyKey = proxyItem = null; paintProxy();
      for (const event of page.events) {
        const article = uiPart('article'); const label = uiPart('heading', KINDS[event.kind]);
        const provenance = uiPart('hint', `Entry ${event.entryOrdinal} · ID ${event.sourceEntryId ?? 'Unknown'} · time ${event.timestamp ?? 'Unknown'}`);
        const body = uiPart('pre', event.text || '(No visible text recorded)');
        let content: HTMLElement = article;
        if (event.kind === 'tool_call' || event.kind === 'tool_result') {
          const disclosure = uiPart('details'); const summary = uiPart('summary', KINDS[event.kind]);
          disclosure.open = cursor !== null && event.ordinal === cursor.offset;
          disclosure.append(summary); article.append(disclosure); content = disclosure;
        } else content.append(label);
        content.append(provenance, body);
        if(options.openRaw){
          const raw=uiButton('RAW entry'); raw.disabled=busy;
          raw.onclick=()=>{if(!busy&&page===shownPage)options.openRaw?.({root:{...shownPage.root},nativeId:shownPage.nativeId,entryOrdinal:event.entryOrdinal,fingerprint:shownPage.fingerprint});};
          content.append(raw);
        }
        if (event.truncated || event.payloadOmitted) { const note = uiPart('hint', event.truncated ? 'Displayed text is truncated at its fixed limit.' : 'Sensitive or raw payload is omitted.'); content.append(note); }
        events.append(article);
      }
      for (const notice of page.notices) { const item = uiPart('item', notice); notices.append(item); }
    }
    lineage.replaceChildren();
    const parent=uiPart('paragraph'); parent.textContent=`Parent session ID: ${page?.overview.parentNativeId ?? 'Unknown'}`; lineage.append(parent);
    if(page){
      const report=page.lineage;
      const addBranch=(label:string,items:LineageNode[],state:LineageState)=>{
        const heading=uiPart('heading'); heading.textContent=`${label} · ${lineageStates[state]}`;
        const list=uiPart('list');
        for(const node of items){
          const item=uiPart('item'); const button=uiButton(node.nativeId);
          button.disabled=busy||!node.canSelect; button.title=lineageStates[node.state];
          button.onclick=()=>{if(!busy&&current&&page?.lineage===report)show({...current.root},node.nativeId);};
          const note=uiPart('text'); note.textContent=` · ${lineageStates[node.state]}`; item.append(button,note); list.append(item);
        }
        lineage.append(heading,list);
      };
      addBranch('Ancestors',report.ancestors,report.ancestorState); addBranch('Direct children',report.children,report.childrenState);
      const count=uiPart('hint'); count.textContent=`Direct children recorded: ${report.totalDirectChildren??'Unknown'}. Links open read-only detail in this archive.`; lineage.append(count);
    }
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
  function close() { generation++; stopProxyReads(); current = null; page = null; cursor = null; history = []; render(); if (dialog.open) dialog.close(); }
  function show(root: SessionRootSelector, id: string, jump: DetailCursor | null = null) {
    if (destroyed || !rootValue(root) || !nativeId(id) || (jump && (!integer(jump.offset) || !digest(jump.fingerprint)))) { if (!destroyed) options.reportError('This session selector cannot be read safely.'); return false; }
    const other = topModal();
    if (!dialog.open && other && other.id !== 'session-library-dialog') return false;
    generation++; current = { root: { storeRootId: root.storeRootId, configurationRevision: root.configurationRevision }, nativeId: id };
    page = null; cursor = null; history = []; failed = false; activeTab = 'transcript'; message = busy ? 'The previous read is still returning; no replacement was started.' : 'Reading the selected session…';
    render(); if (!dialog.open) openModal(dialog); closeButton.focus(); startProxyReads(); if (!busy) void load(jump, 'reload'); return true;
  }
  find.onclick = () => {
    if (!busy && page) options.openFind?.({ root: { ...page.root }, nativeId: page.nativeId,
      cursor: { offset: 0, fingerprint: page.fingerprint }, limit: 50 });
  };
  exportButton.onclick = () => {
    if (!busy && page) options.openExport?.({ root: { ...page.root }, nativeId: page.nativeId, fingerprint: page.fingerprint, harness: page.harness });
  };
  hopButton.onclick = () => {
    if (!busy && page) options.openHop?.({ root: { ...page.root }, nativeId: page.nativeId, fingerprint: page.fingerprint, harness: page.harness });
  };
  reload.onclick = () => { void load(null, 'reload'); }; previous.onclick = () => { if (history.length) void load(history.at(-1) ?? null, 'previous'); };
  next.onclick = () => { if (page?.next) void load(page.next, 'next'); }; closeButton.onclick = close;
  dialog.addEventListener('close', () => { if (!dialog.open) { generation++; stopProxyReads(); current = null; page = null; cursor = null; history = []; render(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); } });
  render(); return { show, close, destroy: () => { destroyed = true; close(); dialog.remove(); } };
}
