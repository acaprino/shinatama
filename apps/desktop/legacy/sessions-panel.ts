import { LOADING_DELAY_MS, showAfter, uiPart, uiButton, uiInput, uiSelect, uiOption } from '@acaprino/imago/ui';
import { stateLayer, type SectionState } from './panel-states.ts';
/** Sessions: one panel for every session of an agent, whatever its state. Running sessions come
 * first, with what they are doing now; saved ones follow, read from the agents' own archives.
 * The search field filters both as you type and, on Enter, searches the text of the indexed
 * conversations. A session that ends moves from the first list to the second: "running" and
 * "saved" are states of one thing, not two panels. Reading never starts or resumes an agent.
 * The panel is a view of the window's side panel. */
import { sidePanelOpen, sideSurface } from './side-panel.ts';
import { agentName, createInstancesSection, summaryLine } from './instances-overview.ts';
import { failureText, pageValue, rootsValue, when, type SessionCursor, type SessionLibraryRoot, type SessionListPage,
  type SessionListRequest, type SessionMetadataRow, type SessionRootSelector } from './session-library.ts';
import { renderSnippet, type SearchHit, type SearchRequest } from './session-search.ts';
import { byRecency, folderName, inProject } from './sessions-model.ts';
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
  /** Settings at its Sessions section: activity titles and search indexing are switched there. */
  openSettings: () => void;
  /** The open project's folder, or null. */
  project: () => string | null;
  restoreFocus?: () => void;
};
type Saved = { root: SessionLibraryRoot; row: SessionMetadataRow };
type Archive = { root: SessionLibraryRoot; rows: SessionMetadataRow[]; next: SessionCursor | null; failure: string | null };
const REFRESH_MS = 2000;

const size = (bytes: number | null) => bytes === null ? 'Unknown' : bytes < 1024 ? `${bytes} B` : bytes < 1048576 ? `${(bytes / 1024).toFixed(0)} KB` : `${(bytes / 1048576).toFixed(1)} MB`;

export function createSessionsPanel(options: Options) {
  const view = sideSurface('sessions-dialog', 'Sessions', 'Close sessions', 'sessions-title');

  const tools = uiPart('toolbar'); tools.classList.add('sessions-tools');
  const search = uiInput('search'); search.maxLength = 128; search.autocomplete = 'off'; search.spellcheck = false;
  search.placeholder = 'Filter · Enter searches text'; search.setAttribute('aria-label', 'Filter sessions, or press Enter to search the text of conversations');
  const scope = uiSelect(); scope.setAttribute('aria-label', 'Which sessions to show');
  tools.append(search, scope);
  const status = uiPart('status');

  const runningHead = uiPart('heading'); const runningCount = uiPart('text'); const runningSummary = uiPart('text');
  runningHead.classList.add('sessions-heading'); runningCount.classList.add('sessions-count'); runningSummary.classList.add('sessions-note');
  runningHead.append('Running', runningCount, runningSummary);
  const runningEmpty = uiPart('paragraph', 'No session is running.');
  const instances = createInstancesSection({
    invoke: options.invoke, focus: options.focus, threadOf: options.threadOf,
    focused: () => view.close(), notice: text => say(text, 'info'),
  });

  const savedHead = uiPart('heading'); const savedCount = uiPart('text'); const savedNote = uiPart('text');
  const reload = uiButton('Reload'); reload.classList.add('sessions-quiet'); reload.title = 'Read the archives again';
  const savedLabel = document.createTextNode('Saved');
  savedHead.classList.add('sessions-heading'); savedCount.classList.add('sessions-count'); savedNote.classList.add('sessions-note');
  savedHead.append(savedLabel, savedCount, savedNote, reload);
  const savedList = uiPart('list'); savedList.classList.add('sessions-list'); savedList.setAttribute('aria-label', 'Saved sessions');
  const savedEmpty = uiPart('paragraph');
  // The saved list's loading, empty and error states (Imago's layer), written where the list is: the search field and
  // the running sessions stay usable beside it.
  const savedState = uiPart('stack'); savedState.id = 'sessions-saved-state';
  let savedLoadingShown = false; let cancelSavedLoading = () => {};
  function paintSavedState(state: SectionState | null) { savedState.replaceChildren(...(state ? [stateLayer(state)] : [])); }
  const more = uiButton('Load older sessions'); more.classList.add('sessions-more'); more.hidden = true;

  const advanced = uiPart('details'); advanced.classList.add('sessions-options');
  const advancedSummary = uiPart('summary', 'Options and archives');
  const advancedTools = uiPart('toolbar'); advancedTools.classList.add('sessions-options-tools');
  const archivesButton = uiButton('Archive browser…'); archivesButton.title = 'Page through one archive, group it by folder, or add a read-only archive';
  const indexButton = uiButton('Search index…'); indexButton.title = 'Index your sessions, filter a search by agent or date, or forget the indexed text';
  const settingsButton = uiButton('Settings…'); settingsButton.title = 'Turn activity titles or search indexing on or off';
  advancedTools.append(archivesButton, indexButton, settingsButton);
  advanced.append(advancedSummary, advancedTools, instances.legend);

  view.body.append(tools, status, runningHead, runningEmpty, instances.element, savedHead, savedList, savedState, savedEmpty, more, advanced);

  let timer = 0; let notice = 0; let generation = 0;
  let archives: Archive[] = []; let loadState: 'idle' | 'loading' | 'ready' | 'failed' = 'idle'; let savedFailure = '';
  let hits: SearchHit[] | null = null; let searched = ''; let searching = false; let expanded: string | null = null;
  let chosenScope = 'project';

  function say(text: string, tone: 'info' | 'error') {
    status.textContent = text; status.dataset.tone = tone; status.hidden = false; delete status.dataset.source;
    clearTimeout(notice); if (tone === 'info') notice = window.setTimeout(() => { status.hidden = true; }, 5000);
  }
  const quiet = () => { if (status.dataset.tone !== 'info') status.hidden = true; };

  function paintScope() {
    const project = options.project(); const wanted = chosenScope; scope.replaceChildren();
    const option = (value: string, label: string, parent: HTMLElement = scope) => { const item = uiOption('', label); item.value = value; parent.append(item); };
    if (project) option('project', `This project · ${folderName(project)}`);
    option('all', 'All projects');
    if (archives.length) {
      const group = uiPart('optionGroup'); group.label = 'One archive';
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
    const item = uiPart('item'); item.classList.add('sessions-row'); item.dataset.nativeId = row.nativeId;
    const head = uiButton(''); head.classList.add('sessions-row-head'); head.type = 'button'; head.setAttribute('aria-expanded', String(open)); head.dataset.key = id;
    const name = uiPart('text', row.title || 'Untitled session'); name.classList.add('sessions-row-title');
    const meta = uiPart('text', `${agentName(root.harness)} · ${folderName(row.workingDirectory)}`); meta.classList.add('sessions-row-meta'); meta.title = row.workingDirectory ?? '';
    const time = uiPart('text', when(row.updatedAt)); time.classList.add('sessions-row-time'); time.title = row.updatedAt ?? '';
    head.replaceChildren(name, time, meta);
    head.onclick = () => { expanded = open ? null : id; paintSaved(); savedList.querySelector<HTMLElement>(`[data-key="${CSS.escape(id)}"]`)?.focus(); };
    item.append(head);
    if (open) {
      const body = uiPart('row'); body.classList.add('sessions-row-body');
      const facts = uiPart('descriptionList');
      const fact = (label: string, value: string) => { const detail = uiPart('definition', value); detail.title = value; facts.append(uiPart('term', label), detail); };
      fact('ID', row.nativeId); fact('Folder', row.workingDirectory ?? 'Unknown');
      fact('Created', row.createdAt ? when(row.createdAt) : 'Unknown'); fact('Turns', row.turns === null ? 'Unknown' : row.turns.toLocaleString());
      fact('Size', size(row.bytes)); if (row.parentNativeId) fact('Parent', row.parentNativeId);
      fact('Archive', `${root.label ?? agentName(root.harness)} · ${root.access === 'read_only' ? 'read only' : 'metadata reads only'}`);
      const actions = uiPart('actions');
      const openButton = uiButton('Open conversation', { primary: true }); openButton.type = 'button'; openButton.disabled = !root.available;
      openButton.title = 'Read the conversation, search in it, export it or hop it to another agent';
      openButton.onclick = () => options.openDetail({ storeRootId: root.storeRootId, configurationRevision: root.configurationRevision }, row.nativeId);
      actions.append(openButton);
      body.append(facts, actions); item.append(body);
    }
    return item;
  }
  function hitRow(hit: SearchHit): HTMLLIElement {
    const item = uiPart('item'); item.classList.add('sessions-row');
    const head = uiButton(''); head.classList.add('sessions-row-head'); head.type = 'button';
    head.disabled = hit.configurationRevision === 0;
    head.title = hit.configurationRevision === 0 ? 'This archive was removed; the text is still searchable.' : 'Open the conversation';
    const name = uiPart('text', hit.title || hit.nativeId); name.classList.add('sessions-row-title');
    const meta = uiPart('text', `${agentName(hit.harness)} · ${folderName(hit.workingDirectory)}`); meta.classList.add('sessions-row-meta'); meta.title = hit.workingDirectory ?? '';
    const time = uiPart('text', when(hit.updatedAt)); time.classList.add('sessions-row-time');
    const snippet = uiPart('text'); snippet.classList.add('sessions-row-snippet'); renderSnippet(snippet, hit.snippet);
    head.replaceChildren(name, time, meta, snippet);
    if (hit.sourceDeleted) { const note = uiPart('text', 'Source deleted: still searchable, no longer resumable.'); note.classList.add('sessions-row-snippet'); head.append(note); }
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
      paintSavedState(null); savedEmpty.hidden = hits.length > 0;
      savedEmpty.textContent = `No indexed conversation contains “${searched}”. If the index is empty, open Options and archives, then Search index.`;
      return;
    }
    const rows = visibleSaved(); const loaded = archives.reduce((sum, archive) => sum + archive.rows.length, 0);
    savedLabel.textContent = 'Saved'; savedCount.textContent = loadState === 'ready' || rows.length ? String(rows.length) : '';
    const failed = archives.filter(archive => archive.failure);
    savedNote.textContent = loadState === 'loading' ? 'reading archives…' : failed.length ? `${failed.length} archive${failed.length === 1 ? '' : 's'} could not be read` : '';
    savedNote.title = failed.map(archive => `${archive.root.label ?? agentName(archive.root.harness)}: ${archive.failure}`).join('\n');
    reload.hidden = false; reload.disabled = loadState === 'loading';
    savedList.replaceChildren(...rows.map(savedRow)); savedList.hidden = !rows.length;
    more.hidden = loadState !== 'ready' || !archives.some(archive => archive.next); more.disabled = false;
    const filtered = search.value.trim() !== '';
    const scoped = !filtered && loaded > 0 && chosenScope === 'project';
    // Nothing to list: the first read waits (bars, once it lasts past 300 ms), fails (the sentence and Retry) or finds none.
    paintSavedState(loadState === 'loading' && !rows.length && savedLoadingShown ? { kind: 'loading', rows: 3 }
      : loadState === 'failed' ? { kind: 'error', text: savedFailure, retry: { label: 'Retry', run: () => { void loadSaved(); } } }
      : loadState === 'ready' && !rows.length && !filtered && !scoped ? { kind: 'empty', text: 'No saved session yet.', action: { label: 'Read the archives again', run: () => { void loadSaved(); } } }
      : null);
    savedEmpty.hidden = rows.length > 0 || loadState === 'idle' || loadState === 'loading' || loadState === 'failed' || (loadState === 'ready' && !filtered && !scoped);
    savedEmpty.textContent = filtered ? 'No saved session matches the filter. Press Enter to search inside conversations.'
      : 'No saved session of this project among those read. Choose All projects, or load older sessions.';
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
    const mine = ++generation; loadState = 'loading'; archives = []; expanded = null; paintScope();
    cancelSavedLoading(); savedLoadingShown = false;
    cancelSavedLoading = showAfter(LOADING_DELAY_MS, () => { if (mine === generation && loadState === 'loading') { savedLoadingShown = true; paintSaved(); } });
    paintSaved();
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
      loadState = 'ready';
    } catch (error) { if (mine !== generation) return; loadState = 'failed'; savedFailure = failureText(error); }
    cancelSavedLoading(); savedLoadingShown = false;
    paintSaved();
  }
  more.onclick = async () => {
    const mine = generation; more.disabled = true; loadState = 'loading'; paintSaved();
    for (const archive of archives) { if (archive.next) await readPage(archive, archive.next, mine); if (mine !== generation) return; }
    loadState = 'ready'; paintSaved();
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
    else if (event.key === 'ArrowDown') { event.preventDefault(); view.root.querySelector<HTMLElement>('.instances-item:not([hidden]) button, .sessions-row-head')?.focus(); }
  });
  scope.onchange = () => { chosenScope = scope.value; expanded = null; if (hits) { hits = null; void runSearch(); } paint(); };

  async function refreshRunning(force = false) {
    try { await instances.load(force); if (status.dataset.tone === 'error' && status.dataset.source === 'running') status.hidden = true; paintRunning(); }
    catch (error) { say(typeof error === 'string' ? error : 'The running sessions could not be read.', 'error'); status.dataset.source = 'running'; }
  }
  archivesButton.onclick = () => options.openArchives();
  indexButton.onclick = () => options.openIndex();
  settingsButton.onclick = () => options.openSettings();
  view.onClose(() => { cancelSavedLoading(); clearInterval(timer); clearTimeout(notice); instances.reset(); if (!sidePanelOpen()) options.restoreFocus?.(); });

  /** Opens the panel as it was left: the saved list is read once and kept until Reload. */
  async function show() {
    if (!view.isOpen()) { status.hidden = true; delete status.dataset.source; paintScope(); paint(); view.open(); search.focus(); }
    clearInterval(timer); timer = window.setInterval(() => { if (view.isOpen()) void refreshRunning(); }, REFRESH_MS);
    const running = refreshRunning(true);
    if (loadState === 'idle' || loadState === 'failed') void loadSaved();
    await running;
  }
  paintScope(); paint();
  return { show };
}
