/** Sessions: one panel for every session of an agent, whatever its state. Running sessions come
 * first, with what they are doing now; saved ones follow, read from the agents' own archives.
 * The search field filters both as you type and, on Enter, searches the text of the indexed
 * conversations. A session that ends moves from the first list to the second: "running" and
 * "saved" are states of one thing, not two panels. Reading never starts or resumes an agent. */
import { openModal } from './modal.ts';
import { agentName, createInstancesSection, summaryLine } from './instances-overview.ts';
import { failureText, pageValue, rootsValue, when, type SessionCursor, type SessionLibraryRoot, type SessionListPage,
  type SessionListRequest, type SessionMetadataRow, type SessionRootSelector } from './session-library.ts';
import { renderSnippet, type SearchHit, type SearchRequest } from './session-search.ts';
import { byRecency, folderName, inProject } from './sessions-model.ts';
import './sessions-panel.css';
import { confirmsField } from './desktop-shell-model.ts';

type Options = {
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  focus: (id: string) => boolean;
  threadOf: (id: string) => string | null;
  roots: () => Promise<unknown>;
  list: (request: SessionListRequest) => Promise<unknown>;
  search: (request: SearchRequest) => Promise<SearchHit[]>;
  openDetail: (root: SessionRootSelector, nativeId: string) => void;
  /** The explicit archive browser and the search index, for what this panel does not show. */
  openArchives: () => void; openIndex: () => void;
  /** The open project's folder, or null. */
  project: () => string | null;
  restoreFocus?: () => void;
};
type Saved = { root: SessionLibraryRoot; row: SessionMetadataRow };
type Archive = { root: SessionLibraryRoot; rows: SessionMetadataRow[]; next: SessionCursor | null; failure: string | null };
const REFRESH_MS = 2000;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag); if (className) node.className = className; if (text !== undefined) node.textContent = text; return node;
}
const size = (bytes: number | null) => bytes === null ? 'Unknown' : bytes < 1024 ? `${bytes} B` : bytes < 1048576 ? `${(bytes / 1024).toFixed(0)} KB` : `${(bytes / 1048576).toFixed(1)} MB`;

export function createSessionsPanel(options: Options) {
  const dialog = el('dialog'); dialog.id = 'sessions-dialog'; dialog.setAttribute('aria-labelledby', 'sessions-title');
  const heading = el('div', 'dialog-heading');
  const title = el('h1', '', 'Sessions'); title.id = 'sessions-title';
  const closeButton = el('button', '', '×'); closeButton.type = 'button'; closeButton.setAttribute('aria-label', 'Close sessions');
  heading.append(title, closeButton);

  const tools = el('div', 'sessions-tools');
  const search = el('input', 'sessions-search'); search.type = 'search'; search.maxLength = 128; search.autocomplete = 'off'; search.spellcheck = false;
  search.placeholder = 'Filter · Enter searches text'; search.setAttribute('aria-label', 'Filter sessions, or press Enter to search the text of conversations');
  const scope = el('select', 'sessions-scope'); scope.setAttribute('aria-label', 'Which sessions to show');
  tools.append(search, scope);
  const status = el('p', 'sessions-status'); status.setAttribute('role', 'status');

  const runningHead = el('h2', 'sessions-heading'); const runningCount = el('span', 'sessions-count'); const runningSummary = el('span', 'sessions-heading-note');
  runningHead.append('Running', runningCount, runningSummary);
  const runningEmpty = el('p', 'sessions-empty', 'No session is running.');
  const instances = createInstancesSection({
    invoke: options.invoke, focus: options.focus, threadOf: options.threadOf,
    focused: () => dialog.close(), notice: text => say(text, 'info'),
  });

  const savedHead = el('h2', 'sessions-heading'); const savedCount = el('span', 'sessions-count'); const savedNote = el('span', 'sessions-heading-note');
  const reload = el('button', 'sessions-quiet', 'Reload'); reload.type = 'button'; reload.title = 'Read the archives again';
  const savedLabel = document.createTextNode('Saved');
  savedHead.append(savedLabel, savedCount, savedNote, reload);
  const savedList = el('ul', 'sessions-list'); savedList.setAttribute('aria-label', 'Saved sessions');
  const savedEmpty = el('p', 'sessions-empty');
  const more = el('button', 'sessions-more', 'Load older sessions'); more.type = 'button'; more.hidden = true;

  const advanced = el('details', 'sessions-options');
  const advancedSummary = el('summary', '', 'Options and archives');
  const advancedTools = el('div', 'sessions-options-tools');
  const archivesButton = el('button', '', 'Archive browser…'); archivesButton.type = 'button'; archivesButton.title = 'Page through one archive, group it by folder, or add a read-only archive';
  const indexButton = el('button', '', 'Search index…'); indexButton.type = 'button'; indexButton.title = 'Index your sessions, filter a search by agent or date, or forget the indexed text';
  advancedTools.append(archivesButton, indexButton);
  advanced.append(advancedSummary, advancedTools, instances.settings);

  dialog.append(heading, tools, status, runningHead, runningEmpty, instances.element, savedHead, savedList, savedEmpty, more, advanced);
  (document.querySelector('#app') ?? document.body).append(dialog);

  let timer = 0; let notice = 0; let generation = 0;
  let archives: Archive[] = []; let savedState: 'idle' | 'loading' | 'ready' | 'failed' = 'idle'; let savedFailure = '';
  let hits: SearchHit[] | null = null; let searched = ''; let searching = false; let expanded: string | null = null;
  let chosenScope = 'project';

  function say(text: string, tone: 'info' | 'error') {
    status.textContent = text; status.dataset.tone = tone; status.hidden = false; delete status.dataset.source;
    clearTimeout(notice); if (tone === 'info') notice = window.setTimeout(() => { status.hidden = true; }, 5000);
  }
  const quiet = () => { if (status.dataset.tone !== 'info') status.hidden = true; };

  function paintScope() {
    const project = options.project(); const wanted = chosenScope; scope.replaceChildren();
    const option = (value: string, label: string, parent: HTMLElement = scope) => { const item = el('option', '', label); item.value = value; parent.append(item); };
    if (project) option('project', `This project · ${folderName(project)}`);
    option('all', 'All projects');
    if (archives.length) {
      const group = el('optgroup'); group.label = 'One archive';
      archives.forEach((archive, index) => option(`root:${index}`, `${archive.root.label ?? agentName(archive.root.harness)} · ${archive.root.kind === 'box' ? 'Shinatama' : 'system'}${archive.root.available ? '' : ' · unavailable'}`, group));
      scope.append(group);
    }
    scope.value = [...scope.options].some(item => item.value === wanted) ? wanted : project ? 'project' : 'all';
    chosenScope = scope.value;
  }
  function visibleSaved(): Saved[] {
    const project = chosenScope === 'project' ? options.project() : null;
    const only = chosenScope.startsWith('root:') ? archives[Number(chosenScope.slice(5))] : null;
    const wanted = hits ? '' : search.value.trim().toLocaleLowerCase();
    const all = (only ? [only] : archives).flatMap(archive => archive.rows.map(row => ({ root: archive.root, row })));
    return byRecency(all.filter(({ root, row }) => inProject(row.workingDirectory, project)
      && (!wanted || [row.title, row.workingDirectory, row.nativeId, agentName(root.harness)].some(value => value?.toLocaleLowerCase().includes(wanted)))));
  }
  const key = (root: SessionRootSelector, nativeId: string) => `${root.storeRootId}\u0000${root.configurationRevision}\u0000${nativeId}`;

  function savedRow({ root, row }: Saved): HTMLLIElement {
    const id = key(root, row.nativeId); const open = expanded === id;
    const item = el('li', 'sessions-row'); item.dataset.nativeId = row.nativeId;
    const head = el('button', 'sessions-row-head'); head.type = 'button'; head.setAttribute('aria-expanded', String(open)); head.dataset.key = id;
    const name = el('span', 'sessions-row-title', row.title || 'Untitled session');
    const meta = el('span', 'sessions-row-meta', `${agentName(root.harness)} · ${folderName(row.workingDirectory)}`); meta.title = row.workingDirectory ?? '';
    const time = el('span', 'sessions-row-time', when(row.updatedAt)); time.title = row.updatedAt ?? '';
    head.append(name, time, meta);
    head.onclick = () => { expanded = open ? null : id; paintSaved(); savedList.querySelector<HTMLElement>(`[data-key="${CSS.escape(id)}"]`)?.focus(); };
    item.append(head);
    if (open) {
      const body = el('div', 'sessions-row-body');
      const facts = el('dl', 'instances-facts sessions-facts');
      const fact = (label: string, value: string) => { const detail = el('dd', '', value); detail.title = value; facts.append(el('dt', '', label), detail); };
      fact('ID', row.nativeId); fact('Folder', row.workingDirectory ?? 'Unknown');
      fact('Created', row.createdAt ? when(row.createdAt) : 'Unknown'); fact('Turns', row.turns === null ? 'Unknown' : row.turns.toLocaleString());
      fact('Size', size(row.bytes)); if (row.parentNativeId) fact('Parent', row.parentNativeId);
      fact('Archive', `${root.label ?? agentName(root.harness)} · ${root.access === 'read_only' ? 'read only' : 'metadata reads only'}`);
      const actions = el('div', 'sessions-row-actions');
      const openButton = el('button', 'primary', 'Open conversation'); openButton.type = 'button'; openButton.disabled = !root.available;
      openButton.title = 'Read the conversation, search in it, export it or hop it to another agent';
      openButton.onclick = () => options.openDetail({ storeRootId: root.storeRootId, configurationRevision: root.configurationRevision }, row.nativeId);
      actions.append(openButton);
      body.append(facts, actions); item.append(body);
    }
    return item;
  }
  function hitRow(hit: SearchHit): HTMLLIElement {
    const item = el('li', 'sessions-row sessions-hit');
    const head = el('button', 'sessions-row-head'); head.type = 'button';
    head.disabled = hit.configurationRevision === 0;
    head.title = hit.configurationRevision === 0 ? 'This archive was removed; the text is still searchable.' : 'Open the conversation';
    const name = el('span', 'sessions-row-title', hit.title || hit.nativeId);
    const meta = el('span', 'sessions-row-meta', `${agentName(hit.harness)} · ${folderName(hit.workingDirectory)}`); meta.title = hit.workingDirectory ?? '';
    const time = el('span', 'sessions-row-time', when(hit.updatedAt));
    const snippet = el('span', 'sessions-row-snippet'); renderSnippet(snippet, hit.snippet);
    head.append(name, time, meta, snippet);
    if (hit.sourceDeleted) head.append(el('span', 'sessions-row-note', 'Source deleted: still searchable, no longer resumable.'));
    head.onclick = () => options.openDetail({ storeRootId: hit.storeRootId, configurationRevision: hit.configurationRevision }, hit.nativeId);
    item.append(head); return item;
  }

  function paintRunning() {
    const rows = instances.rows(); const shown = instances.filter(hits ? '' : search.value);
    runningCount.textContent = rows.length ? String(rows.length) : '';
    runningSummary.textContent = summaryLine(rows);
    runningEmpty.hidden = shown > 0;
    runningEmpty.textContent = rows.length ? 'No running session matches the filter.' : 'No session is running.';
  }
  function paintSaved() {
    if (hits) {
      savedLabel.textContent = 'Matches in conversations'; savedCount.textContent = String(hits.length);
      savedNote.textContent = hits.length >= 50 ? 'first 50' : ''; reload.hidden = true; more.hidden = true;
      savedList.replaceChildren(...hits.map(hitRow)); savedList.hidden = !hits.length;
      savedEmpty.hidden = hits.length > 0;
      savedEmpty.textContent = `No indexed conversation contains “${searched}”. If the index is empty, open Options and archives, then Search index.`;
      return;
    }
    const rows = visibleSaved(); const loaded = archives.reduce((sum, archive) => sum + archive.rows.length, 0);
    savedLabel.textContent = 'Saved'; savedCount.textContent = savedState === 'ready' || rows.length ? String(rows.length) : '';
    const failed = archives.filter(archive => archive.failure);
    savedNote.textContent = savedState === 'loading' ? 'reading archives…' : failed.length ? `${failed.length} archive${failed.length === 1 ? '' : 's'} could not be read` : '';
    savedNote.title = failed.map(archive => `${archive.root.label ?? agentName(archive.root.harness)}: ${archive.failure}`).join('\n');
    reload.hidden = false; reload.disabled = savedState === 'loading';
    savedList.replaceChildren(...rows.map(savedRow)); savedList.hidden = !rows.length;
    more.hidden = savedState !== 'ready' || !archives.some(archive => archive.next); more.disabled = false;
    savedEmpty.hidden = rows.length > 0 || savedState === 'loading';
    savedEmpty.textContent = savedState === 'failed' ? savedFailure
      : search.value.trim() ? 'No saved session matches the filter. Press Enter to search inside conversations.'
      : loaded && chosenScope === 'project' ? 'No saved session of this project among those read. Choose All projects, or load older sessions.'
      : 'No saved session yet.';
  }
  const paint = () => { paintRunning(); paintSaved(); };

  async function readPage(archive: Archive, cursor: SessionCursor | null, mine: number) {
    const selector = { storeRootId: archive.root.storeRootId, configurationRevision: archive.root.configurationRevision };
    try {
      const page: SessionListPage | null = pageValue(await options.list({ root: selector, cursor, limit: 50, includeChildren: true }), archive.root, cursor);
      if (mine !== generation) return;
      if (!page) { archive.failure = 'The archive answered in an unexpected shape.'; archive.next = null; return; }
      const known = new Set(archive.rows.map(row => row.nativeId));
      archive.rows.push(...page.rows.filter(row => !known.has(row.nativeId))); archive.next = page.next; archive.failure = null;
    } catch (error) { if (mine === generation) { archive.failure = failureText(error); archive.next = null; } }
  }
  /** Every available archive, one after the other: the reader is a contained worker, never two at once. */
  async function loadSaved() {
    const mine = ++generation; savedState = 'loading'; archives = []; expanded = null; paintScope(); paintSaved();
    try {
      const roots = rootsValue(await options.roots());
      if (mine !== generation) return;
      if (!roots) throw { issue: 'read_unavailable' };
      archives = roots.map(root => ({ root, rows: [], next: null, failure: root.available ? null : 'This archive is unavailable.' }));
      paintScope();
      for (const archive of archives) {
        if (!archive.root.available) continue;
        await readPage(archive, null, mine); if (mine !== generation) return; paintSaved();
      }
      savedState = 'ready';
    } catch (error) { if (mine !== generation) return; savedState = 'failed'; savedFailure = failureText(error); }
    paintSaved();
  }
  more.onclick = async () => {
    const mine = generation; more.disabled = true; savedState = 'loading'; paintSaved();
    for (const archive of archives) { if (archive.next) await readPage(archive, archive.next, mine); if (mine !== generation) return; }
    savedState = 'ready'; paintSaved();
  };
  reload.onclick = () => { void loadSaved(); };

  async function runSearch() {
    const wanted = search.value.trim(); if (!wanted || searching) return;
    searching = true; say('Searching conversations…', 'info');
    const project = chosenScope === 'project' ? options.project() : null;
    try {
      const found = await options.search({ query: wanted, ...(project ? { project } : {}) });
      if (search.value.trim() !== wanted) return;
      hits = found; searched = wanted; status.hidden = true; paint();
    } catch (error) {
      const code = typeof error === 'string' ? error : (error as { issue?: string } | null)?.issue;
      say(code === 'invalid_request' ? 'The search index is off or cannot take this query. Open Options and archives, then Search index.' : 'The search index is busy or unavailable. Try again.', 'error');
    } finally { searching = false; }
  }
  search.addEventListener('input', () => { if (hits) { hits = null; searched = ''; } quiet(); paint(); });
  search.addEventListener('keydown', event => {
    if (confirmsField(event)) { event.preventDefault(); void runSearch(); }
    else if (event.key === 'Escape' && search.value) { event.preventDefault(); event.stopPropagation(); search.value = ''; hits = null; paint(); }
    else if (event.key === 'ArrowDown') { event.preventDefault(); dialog.querySelector<HTMLElement>('.instances-item:not([hidden]) button, .sessions-row-head')?.focus(); }
  });
  scope.onchange = () => { chosenScope = scope.value; expanded = null; if (hits) { hits = null; void runSearch(); } paint(); };

  async function refreshRunning(force = false) {
    try { await instances.load(force); if (status.dataset.tone === 'error' && status.dataset.source === 'running') status.hidden = true; paintRunning(); }
    catch (error) { say(typeof error === 'string' ? error : 'The running sessions could not be read.', 'error'); status.dataset.source = 'running'; }
  }
  archivesButton.onclick = () => options.openArchives();
  indexButton.onclick = () => options.openIndex();
  closeButton.onclick = () => dialog.close();
  dialog.addEventListener('close', () => { clearInterval(timer); clearTimeout(notice); instances.reset(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });

  /** Opens the panel as it was left: the saved list is read once and kept until Reload. */
  async function show() {
    if (!dialog.open) { status.hidden = true; delete status.dataset.source; paintScope(); paint(); openModal(dialog); search.focus(); }
    clearInterval(timer); timer = window.setInterval(() => { if (dialog.open) void refreshRunning(); }, REFRESH_MS);
    const running = refreshRunning(true);
    if (savedState === 'idle' || savedState === 'failed') void loadSaved();
    await running;
  }
  paintScope(); paint();
  return { show };
}
