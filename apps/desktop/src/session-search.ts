import { openModal } from './modal.ts';
import type { SessionRootSelector } from './session-library.ts';
import './session-search.css';

export type SearchHit = {
  storeRootId: string; configurationRevision: number; nativeId: string; harness: string;
  title: string | null; workingDirectory: string | null; updatedAt: string | null;
  snippet: string; sourceDeleted: boolean;
};
export type SearchRequest = { query: string; harness?: string; project?: string; from?: string; to?: string };
export type IndexProgress = {
  phase: 'idle' | 'running' | 'done' | 'stopped' | 'failed'; rootsTotal: number; rootsDone: number;
  indexed: number; skipped: number; failed: number; unsupportedArchives: number; markedSourceDeleted: number; message: string | null;
};
export type IndexState = { progress: IndexProgress; index: { enabled: boolean; documents: number; sourceDeleted: number; bodyBytes: number } };
type Options = {
  search: (request: SearchRequest) => Promise<SearchHit[]>;
  indexStatus: () => Promise<IndexState>;
  indexStart: () => Promise<IndexProgress>;
  indexCancel: () => Promise<void>;
  indexEnable: (enabled: boolean) => Promise<void>;
  indexForget: () => Promise<number>;
  openDetail: (root: SessionRootSelector, nativeId: string) => void;
  reportError: (message: string) => void;
  restoreFocus?: () => void;
};
const MARK_OPEN = '«'; const MARK_CLOSE = '»';
const reasons: Record<string, string> = {
  invalid_request: 'Check the query and filters, or turn indexing on.',
  read_unavailable: 'The index is busy or unavailable. Try again.',
  cleanup_unconfirmed: 'A reader cleanup is unconfirmed. Restart before indexing again.',
};
const reason = (error: unknown) => {
  const code = typeof error === 'string' ? error : (error as { issue?: string } | null)?.issue;
  return (code && reasons[code]) || 'The operation could not be completed.';
};
const mib = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MiB`;
const isRunning = (state: IndexState | null) => state?.progress.phase === 'running';

/** Renders «match» markers as <mark> without ever parsing the snippet as HTML. */
export function renderSnippet(target: HTMLElement, snippet: string) {
  target.replaceChildren();
  let rest = snippet;
  while (rest.length) {
    const start = rest.indexOf(MARK_OPEN);
    if (start < 0) { target.append(document.createTextNode(rest)); break; }
    const end = rest.indexOf(MARK_CLOSE, start + 1);
    if (end < 0) { target.append(document.createTextNode(rest)); break; }
    if (start > 0) target.append(document.createTextNode(rest.slice(0, start)));
    const mark = document.createElement('mark'); mark.textContent = rest.slice(start + 1, end); target.append(mark);
    rest = rest.slice(end + 1);
  }
}

export function createSessionSearch(options: Options) {
  const button = document.createElement('button'); button.id = 'session-search'; button.type = 'button'; button.textContent = 'Search';
  document.querySelector('.toolbar')?.append(button);
  const dialog = document.createElement('dialog'); dialog.id = 'session-search-dialog'; dialog.setAttribute('aria-labelledby', 'session-search-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'session-search-title'; title.textContent = 'Search all sessions';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close search'); heading.append(title, closeButton);
  const description = document.createElement('p'); description.className = 'hint';
  description.textContent = 'Searches the text you and the agents wrote in indexed sessions. Indexing only reads your session archives; it never starts or resumes an agent.';

  const form = document.createElement('form'); form.className = 'session-search-form';
  const queryLabel = document.createElement('label'); queryLabel.htmlFor = 'session-search-query'; queryLabel.textContent = 'Search for';
  const query = document.createElement('input'); query.id = 'session-search-query'; query.type = 'search'; query.maxLength = 128; query.autocomplete = 'off'; query.required = true;
  const agentLabel = document.createElement('label'); agentLabel.htmlFor = 'session-search-agent'; agentLabel.textContent = 'Agent';
  const agent = document.createElement('select'); agent.id = 'session-search-agent';
  for (const [value, label] of [['', 'Any agent'], ['claude_code', 'Claude Code'], ['codex', 'Codex'], ['pi', 'Pi']]) { const o = document.createElement('option'); o.value = value; o.textContent = label; agent.append(o); }
  const projectLabel = document.createElement('label'); projectLabel.htmlFor = 'session-search-project'; projectLabel.textContent = 'Folder starts with';
  const project = document.createElement('input'); project.id = 'session-search-project'; project.type = 'text'; project.maxLength = 1024; project.autocomplete = 'off'; project.spellcheck = false;
  const fromLabel = document.createElement('label'); fromLabel.htmlFor = 'session-search-from'; fromLabel.textContent = 'From';
  const from = document.createElement('input'); from.id = 'session-search-from'; from.type = 'date';
  const toLabel = document.createElement('label'); toLabel.htmlFor = 'session-search-to'; toLabel.textContent = 'To';
  const to = document.createElement('input'); to.id = 'session-search-to'; to.type = 'date';
  const submit = document.createElement('button'); submit.type = 'submit'; submit.textContent = 'Search';
  form.append(queryLabel, query, agentLabel, agent, projectLabel, project, fromLabel, from, toLabel, to, submit);

  const status = document.createElement('p'); status.id = 'session-search-status'; status.setAttribute('role', 'status');
  const results = document.createElement('ol'); results.className = 'session-search-results'; results.setAttribute('aria-label', 'Search results');

  const indexing = document.createElement('details'); indexing.className = 'session-search-index';
  const indexSummary = document.createElement('summary'); indexSummary.textContent = 'Index';
  const indexInfo = document.createElement('p'); indexInfo.id = 'session-search-index-info'; indexInfo.setAttribute('role', 'status');
  const indexTools = document.createElement('div'); indexTools.className = 'session-search-index-tools';
  const start = document.createElement('button'); start.type = 'button'; start.textContent = 'Index sessions';
  const stop = document.createElement('button'); stop.type = 'button'; stop.textContent = 'Stop';
  const forget = document.createElement('button'); forget.type = 'button'; forget.textContent = 'Forget all indexed text';
  const enabledLabel = document.createElement('label'); enabledLabel.className = 'session-search-enabled';
  const enabled = document.createElement('input'); enabled.type = 'checkbox'; enabled.id = 'session-search-enabled';
  enabledLabel.append(enabled, document.createTextNode(' Indexing allowed'));
  const indexHelp = document.createElement('p'); indexHelp.className = 'hint';
  indexHelp.textContent = 'Only changed sessions are read again. Forgetting removes the indexed text only; your sessions and threads are not touched.';
  indexTools.append(start, stop, forget, enabledLabel); indexing.append(indexSummary, indexInfo, indexTools, indexHelp);

  dialog.append(heading, description, form, status, results, indexing);
  (document.querySelector('#app') ?? document.body).append(dialog);

  let state: IndexState | null = null; let timer: number | undefined; let busy = false; let destroyed = false; let generation = 0;
  const stopPolling = () => { if (timer !== undefined) { window.clearInterval(timer); timer = undefined; } };
  function renderIndex() {
    const idx = state?.index; const progress = state?.progress;
    const running = isRunning(state);
    const parts: string[] = [];
    if (idx) parts.push(`${idx.documents} sessions indexed · ${idx.sourceDeleted} with deleted source · ${mib(idx.bodyBytes)}`);
    if (progress && progress.phase !== 'idle') {
      const verb = { running: 'Indexing', done: 'Finished', stopped: 'Stopped', failed: 'Failed', idle: '' }[progress.phase];
      parts.push(`${verb}: ${progress.rootsDone}/${progress.rootsTotal} archives · ${progress.indexed} read · ${progress.skipped} unchanged`
        + (progress.failed ? ` · ${progress.failed} unreadable` : '') + (progress.unsupportedArchives ? ` · ${progress.unsupportedArchives} archives skipped` : ''));
      if (progress.message) parts.push(progress.message);
    }
    indexInfo.textContent = parts.join('. ') || 'Index not loaded yet.';
    start.disabled = busy || running || !idx?.enabled; stop.disabled = !running; forget.disabled = busy || running || !idx || idx.documents === 0;
    enabled.checked = idx?.enabled ?? true; enabled.disabled = busy;
    submit.disabled = busy; dialog.setAttribute('aria-busy', String(busy));
  }
  async function refresh() {
    const mine = generation;
    try { const next = await options.indexStatus(); if (mine !== generation || destroyed) return; state = next; }
    catch (error) { if (mine === generation && !destroyed) options.reportError(reason(error)); stopPolling(); }
    if (mine === generation && !destroyed) { renderIndex(); if (!isRunning(state)) stopPolling(); }
  }
  function poll() { stopPolling(); timer = window.setInterval(() => { void refresh(); }, 1000); }

  function showResults(hits: SearchHit[], wanted: string) {
    results.replaceChildren();
    status.textContent = hits.length ? `${hits.length} session${hits.length === 1 ? '' : 's'} match “${wanted}”${hits.length >= 50 ? ' (first 50 shown)' : ''}.` : `No indexed session matches “${wanted}”. Index your sessions first if you have not.`;
    for (const hit of hits) {
      const item = document.createElement('li'); item.className = 'session-search-hit';
      const open = document.createElement('button'); open.type = 'button'; open.className = 'session-search-open';
      open.textContent = hit.title || hit.nativeId;
      const root = { storeRootId: hit.storeRootId, configurationRevision: hit.configurationRevision };
      open.disabled = hit.configurationRevision === 0;
      open.title = hit.configurationRevision === 0 ? 'This archive was removed; the text is still searchable.' : 'Open session detail';
      open.onclick = () => { if (open.disabled) return; close(); options.openDetail(root, hit.nativeId); };
      const meta = document.createElement('p'); meta.className = 'session-search-meta';
      meta.textContent = [hit.harness, hit.workingDirectory, hit.updatedAt].filter(Boolean).join(' · ');
      const snippet = document.createElement('p'); snippet.className = 'session-search-snippet'; renderSnippet(snippet, hit.snippet);
      item.append(open, meta, snippet);
      if (hit.sourceDeleted) { const note = document.createElement('p'); note.className = 'hint'; note.textContent = 'Source deleted: still searchable, but it can no longer be resumed natively.'; item.append(note); }
      results.append(item);
    }
  }
  form.onsubmit = event => {
    event.preventDefault(); if (busy || destroyed) return;
    const wanted = query.value.trim(); if (!wanted) { status.textContent = 'Enter something to search for.'; return; }
    const request: SearchRequest = { query: wanted, ...(agent.value ? { harness: agent.value } : {}), ...(project.value.trim() ? { project: project.value.trim() } : {}),
      ...(from.value ? { from: from.value } : {}), ...(to.value ? { to: to.value } : {}) };
    busy = true; const mine = ++generation; status.textContent = 'Searching…'; renderIndex();
    options.search(request).then(hits => { if (mine === generation && !destroyed) showResults(hits, wanted); },
      error => { if (mine === generation && !destroyed) { status.textContent = reason(error); results.replaceChildren(); } })
      .finally(() => { if (mine === generation && !destroyed) { busy = false; renderIndex(); } });
  };
  start.onclick = () => {
    if (busy) return; busy = true; renderIndex();
    options.indexStart().then(() => { busy = false; poll(); void refresh(); }, error => { busy = false; options.reportError(reason(error)); void refresh(); });
  };
  stop.onclick = () => { void options.indexCancel().then(() => refresh(), () => undefined); };
  forget.onclick = () => {
    if (busy || !window.confirm('Forget all indexed session text? Your sessions and threads are not changed.')) return;
    busy = true; renderIndex();
    options.indexForget().then(() => { results.replaceChildren(); status.textContent = 'Indexed text forgotten.'; },
      error => options.reportError(reason(error))).finally(() => { busy = false; void refresh(); });
  };
  enabled.onchange = () => {
    if (busy) { enabled.checked = !enabled.checked; return; }
    busy = true; const wanted = enabled.checked; renderIndex();
    options.indexEnable(wanted).catch(error => options.reportError(reason(error))).finally(() => { busy = false; void refresh(); });
  };
  function show() {
    if (destroyed) return; generation++; busy = false; status.textContent = 'Type a word and press Search.'; renderIndex();
    openModal(dialog); query.focus(); void refresh().then(() => { if (isRunning(state)) poll(); });
  }
  function close() { generation++; stopPolling(); if (dialog.open) dialog.close(); }
  button.onclick = show; closeButton.onclick = close;
  dialog.addEventListener('close', () => { if (!dialog.open) { generation++; stopPolling(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); } });
  renderIndex();
  return { show, close, destroy: () => { destroyed = true; close(); dialog.remove(); button.remove(); } };
}
