import { uiPart, uiButton, uiInput, uiSelect, uiOption, uiTraits, uiEmphasis } from '@acaprino/imago/ui';
import { closeControl } from './controls.ts';
import { openModal } from './modal.ts';
import type { SessionRootSelector } from './session-library.ts';

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
  indexForget: () => Promise<number>;
  openDetail: (root: SessionRootSelector, nativeId: string) => void;
  /** Settings at its Sessions section, where indexing is allowed or stopped. */
  openSettings: () => void;
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
const VERBS = { running: 'Indexing', done: 'Finished', stopped: 'Stopped', failed: 'Failed' };

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
    const mark = uiPart('mark', rest.slice(start + 1, end)); target.append(mark);
    rest = rest.slice(end + 1);
  }
}

export function createSessionSearch(options: Options) {
  const button = uiButton(''); button.id = 'session-search'; button.type = 'button'; button.textContent = 'Search index'; button.title = 'Index your sessions and search them by agent, folder or date';
  document.querySelector('.toolbar')?.append(button);
  const dialog = uiPart('dialog'); dialog.id = 'session-search-dialog'; dialog.setAttribute('aria-labelledby', 'session-search-title');
  const heading = uiPart('header'); heading.classList.add('dialog-heading');
  const title = uiPart('title'); title.id = 'session-search-title'; title.textContent = 'Search all sessions';
  const closeButton = closeControl('Close search'); heading.append(title, closeButton);
  const description = uiPart('hint');
  description.textContent = 'Searches the text you and the agents wrote in indexed sessions. Indexing only reads your session archives; it never starts or resumes an agent.';

  const form = uiPart('form');
  const queryLabel = uiPart('label'); queryLabel.htmlFor = 'session-search-query'; queryLabel.textContent = 'Search for';
  const query = uiInput(); query.id = 'session-search-query'; query.type = 'search'; query.maxLength = 128; query.autocomplete = 'off'; query.required = true;
  const agentLabel = uiPart('label'); agentLabel.htmlFor = 'session-search-agent'; agentLabel.textContent = 'Agent';
  const agent = uiSelect(); agent.id = 'session-search-agent';
  for (const [value, label] of [['', 'Any agent'], ['claude_code', 'Claude Code'], ['codex', 'Codex'], ['pi', 'Pi']]) { const o = uiOption(value, label); agent.append(o); }
  const projectLabel = uiPart('label'); projectLabel.htmlFor = 'session-search-project'; projectLabel.textContent = 'Folder starts with';
  const project = uiInput(); project.id = 'session-search-project'; project.type = 'text'; project.maxLength = 1024; project.autocomplete = 'off'; project.spellcheck = false;
  const fromLabel = uiPart('label'); fromLabel.htmlFor = 'session-search-from'; fromLabel.textContent = 'From';
  const from = uiInput(); from.id = 'session-search-from'; from.type = 'date';
  const toLabel = uiPart('label'); toLabel.htmlFor = 'session-search-to'; toLabel.textContent = 'To';
  const to = uiInput(); to.id = 'session-search-to'; to.type = 'date';
  const submit = uiButton(''); submit.type = 'submit'; submit.textContent = 'Search';
  form.append(queryLabel, query, agentLabel, agent, projectLabel, project, fromLabel, from, toLabel, to, submit);

  const status = uiPart('status'); status.id = 'session-search-status'; status.setAttribute('role', 'status');
  const results = uiTraits(uiPart('orderedList'), 'scroll', 'bounded'); results.setAttribute('aria-label', 'Search results');

  const indexing = uiPart('details'); indexing.classList.add('session-search-index');
  const indexSummary = uiPart('summary', 'Index');
  const indexInfo = uiPart('paragraph'); indexInfo.id = 'session-search-index-info'; indexInfo.setAttribute('role', 'status');
  const indexTools = uiPart('toolbar');
  const start = uiButton('Index sessions');
  const stop = uiButton('Stop');
  const forget = uiButton('Forget all indexed text');
  // Allowing indexing is a preference: it is switched in Settings, and this tool says when it is off.
  const allow = uiButton(''); allow.id = 'session-search-allow'; allow.textContent = 'Turn on in Settings'; allow.hidden = true;
  const indexHelp = uiPart('hint');
  indexHelp.textContent = 'Only changed sessions are read again. Forgetting removes the indexed text only; your sessions and threads are not touched.';
  indexTools.append(start, stop, forget, allow); indexing.append(indexSummary, indexInfo, indexTools, indexHelp);

  dialog.append(heading, description, form, status, results, indexing);
  (document.querySelector('#app') ?? document.body).append(dialog);

  let state: IndexState | null = null; let timer: number | undefined; let busy = false; let destroyed = false; let generation = 0;
  const stopPolling = () => { if (timer !== undefined) { window.clearInterval(timer); timer = undefined; } };
  function renderIndex() {
    const idx = state?.index; const progress = state?.progress;
    const running = isRunning(state);
    const parts: string[] = [];
    if (idx) parts.push(`${idx.documents} sessions indexed · ${idx.sourceDeleted} with deleted source · ${mib(idx.bodyBytes)}`);
    // Before the first run the backend reports no phase at all; that is idle too.
    const verb = progress && (VERBS as Record<string, string | undefined>)[progress.phase];
    if (progress && verb) {
      parts.push(`${verb}: ${progress.rootsDone}/${progress.rootsTotal} archives · ${progress.indexed} read · ${progress.skipped} unchanged`
        + (progress.failed ? ` · ${progress.failed} unreadable` : '') + (progress.unsupportedArchives ? ` · ${progress.unsupportedArchives} archives skipped` : ''));
      if (progress.message) parts.push(progress.message);
    }
    if (idx && !idx.enabled) parts.unshift('Indexing is off');
    indexInfo.textContent = parts.join('. ') || 'Index not loaded yet.';
    start.disabled = busy || running || !idx?.enabled; stop.disabled = !running; forget.disabled = busy || running || !idx || idx.documents === 0;
    allow.hidden = !idx || idx.enabled;
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
      const item = uiPart('item'); item.classList.add('session-search-hit');
      const open = uiButton(''); open.classList.add('session-search-open');
      open.textContent = hit.title || hit.nativeId;
      const root = { storeRootId: hit.storeRootId, configurationRevision: hit.configurationRevision };
      open.disabled = hit.configurationRevision === 0;
      open.title = hit.configurationRevision === 0 ? 'This archive was removed; the text is still searchable.' : 'Open session detail';
      open.onclick = () => { if (open.disabled) return; close(); options.openDetail(root, hit.nativeId); };
      const meta = uiPart('paragraph');
      meta.textContent = [hit.harness, hit.workingDirectory, hit.updatedAt].filter(Boolean).join(' · ');
      const snippet = uiPart('paragraph'); renderSnippet(snippet, hit.snippet);
      item.append(open, meta, snippet);
      if (hit.sourceDeleted) { const note = uiPart('hint', 'Source deleted: still searchable, but it can no longer be resumed natively.'); item.append(note); }
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
  // Confirmed in place: the first press arms the button for a few seconds, the second forgets.
  const forgetLabel = forget.textContent; let forgetArmed = 0;
  const disarmForget = () => { clearTimeout(forgetArmed); forgetArmed = 0; forget.textContent = forgetLabel; uiEmphasis(forget, null); forget.removeAttribute('title'); };
  forget.onblur = disarmForget;
  forget.onclick = () => {
    if (busy) return;
    if (!forgetArmed) {
      forget.textContent = 'Confirm forget'; uiEmphasis(forget, 'danger');
      forget.title = 'Forgets all indexed session text. Your sessions and threads are not changed.';
      status.textContent = 'Press Confirm forget to remove all indexed session text. Your sessions and threads are not changed.';
      forgetArmed = window.setTimeout(disarmForget, 5000); return;
    }
    disarmForget();
    busy = true; renderIndex();
    options.indexForget().then(() => { results.replaceChildren(); status.textContent = 'Indexed text forgotten.'; },
      error => options.reportError(reason(error))).finally(() => { busy = false; void refresh(); });
  };
  allow.onclick = () => options.openSettings();
  // Settings covers this drawer while it is open; what was switched there shows when it comes back.
  new MutationObserver(() => {
    if (!dialog.open || dialog.hasAttribute('data-im-covered')) return;
    // Focus came back to the button that opened Settings; once indexing is on it hides, and Index sessions takes its place.
    void refresh().then(() => { if (allow.hidden && (document.activeElement === document.body || document.activeElement === allow)) start.focus(); });
  }).observe(dialog, { attributes: true, attributeFilter: ['data-im-covered'] });
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
