import { uiPart, uiButton, uiInput, uiSelect, uiOption, uiTraits } from '@acaprino/imago/ui';
import { closeControl } from './controls.ts';
import { openModal } from './modal.ts';

export type SessionRootSelector = { storeRootId: string; configurationRevision: number };
export type SessionCursor = { offset: number; fingerprint: string };
export type SessionLibraryRoot = SessionRootSelector & {
  harness: string; kind: 'box' | 'system'; access: 'read_only' | 'read_write'; available: boolean; label?: string;
};
export type SessionMetadataRow = {
  nativeId: string; title: string | null; workingDirectory: string | null; updatedAt: string | null;
  createdAt: string | null; turns: number | null; bytes: number | null; parentNativeId: string | null;
  archived: boolean | null; metadataTruncated: boolean;
};
export type SessionListPage = {
  root: SessionRootSelector; harness: string; fingerprint: string; total: number;
  rows: SessionMetadataRow[]; next: SessionCursor | null;
};
export type SessionListRequest = { root: SessionRootSelector; cursor: SessionCursor | null; limit: 50; includeChildren: true };
export type SessionArchiveAddRequest = { harness: 'claude_code' | 'codex' | 'pi'; path: string };
type Options = {
  roots: () => Promise<readonly SessionLibraryRoot[]>;
  list: (request: SessionListRequest) => Promise<SessionListPage>;
  addArchive?: (request: SessionArchiveAddRequest) => Promise<SessionLibraryRoot>;
  openDetail?: (root: SessionRootSelector, nativeId: string) => void;
  reportError: (message: string) => void;
  restoreFocus?: () => void;
};
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, cap: number): value is string => typeof value === 'string' && value.length <= cap && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const fingerprint = (value: unknown): value is string => typeof value === 'string' && /^[0-9a-f]{64}$/i.test(value);
const nullableText = (value: unknown, cap: number) => value === null || text(value, cap);
const nullableInteger = (value: unknown) => value === null || integer(value);
function selector(value: unknown): value is SessionRootSelector & Record<string, unknown> {
  return record(value) && text(value.storeRootId, 256) && value.storeRootId.length > 0
    && integer(value.configurationRevision) && value.configurationRevision > 0;
}
const same = (a: SessionRootSelector, b: SessionRootSelector) => a.storeRootId === b.storeRootId && a.configurationRevision === b.configurationRevision;
/** "14 Nov 10:22", or "10:22" today; a value that is not a date is shown as recorded. */
export function when(value: string | null | undefined): string {
  if (!value) return 'Unknown';
  const at = new Date(value); if (Number.isNaN(at.getTime())) return value;
  const today = at.toDateString() === new Date().toDateString();
  const time = at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  return today ? time : `${at.toLocaleDateString([], { day: '2-digit', month: 'short', year: at.getFullYear() === new Date().getFullYear() ? undefined : 'numeric' })} ${time}`;
}
export function rootsValue(value: unknown): SessionLibraryRoot[] | null {
  if (!Array.isArray(value) || value.length > 100) return null;
  const result: SessionLibraryRoot[] = [];
  for (const item of value) {
    if (!selector(item) || !record(item) || !text(item.harness, 64) || !/^[a-z0-9_]+$/.test(item.harness)
      || typeof item.kind !== 'string' || !['box', 'system'].includes(item.kind)
      || typeof item.access !== 'string' || !['read_only', 'read_write'].includes(item.access)
      || typeof item.available !== 'boolean' || (item.label !== undefined && !text(item.label, 128))
      || result.some(root => same(root, item))) return null;
    result.push({ storeRootId: item.storeRootId, configurationRevision: item.configurationRevision, harness: item.harness,
      kind: item.kind as 'box' | 'system', access: item.access as 'read_only' | 'read_write', available: item.available,
      ...(item.label !== undefined ? { label: item.label as string } : {}) });
  }
  return result;
}
export function pageValue(value: unknown, root: SessionLibraryRoot, requested: SessionCursor | null): SessionListPage | null {
  if (!record(value) || !selector(value.root) || !same(value.root, root) || value.harness !== root.harness
    || !fingerprint(value.fingerprint) || !integer(value.total) || !Array.isArray(value.rows) || value.rows.length > 50
    || (requested !== null && requested.fingerprint !== value.fingerprint)) return null;
  const rows: SessionMetadataRow[] = [];
  for (const item of value.rows) {
    if (!record(item) || !text(item.nativeId, 256) || !item.nativeId || rows.some(row => row.nativeId === item.nativeId)
      || !nullableText(item.title, 512) || !nullableText(item.workingDirectory, 1024) || !nullableText(item.updatedAt, 64)
      || !nullableText(item.createdAt, 64) || !nullableText(item.parentNativeId, 256) || !nullableInteger(item.turns)
      || !nullableInteger(item.bytes) || (item.archived !== null && typeof item.archived !== 'boolean') || typeof item.metadataTruncated !== 'boolean') return null;
    rows.push({ nativeId: item.nativeId, title: item.title as string | null, workingDirectory: item.workingDirectory as string | null,
      updatedAt: item.updatedAt as string | null, createdAt: item.createdAt as string | null, parentNativeId: item.parentNativeId as string | null,
      turns: item.turns as number | null, bytes: item.bytes as number | null, archived: item.archived as boolean | null, metadataTruncated: item.metadataTruncated });
  }
  const offset = requested?.offset ?? 0;
  if (offset + rows.length > value.total || (rows.length === 0 && value.total > offset)) return null;
  let next: SessionCursor | null = null;
  if (value.next !== null) {
    if (!record(value.next) || !integer(value.next.offset) || value.next.offset !== offset + rows.length
      || value.next.offset <= offset || value.next.offset >= value.total || value.next.fingerprint !== value.fingerprint) return null;
    next = { offset: value.next.offset, fingerprint: value.fingerprint };
  }
  if (offset + rows.length < value.total && !next) return null;
  return { root: { storeRootId: root.storeRootId, configurationRevision: root.configurationRevision }, harness: root.harness,
    fingerprint: value.fingerprint, total: value.total, rows, next };
}
const FAILURES: Record<string, string> = {
  root_unavailable: 'The selected archive is unavailable. No other archive was used.', root_changed: 'The archive configuration changed. Reload archives and select it explicitly.',
  source_changed: 'Session metadata changed while paging. Reload sessions to start again.', unsupported_harness: 'This agent has no reader in the reviewed Mukade version.',
  unsupported_root_shape: 'This archive shape is not supported by the reviewed reader.', unsafe_root: 'The archive failed read-only path verification.',
  metadata_too_large: 'Session metadata exceeds the fixed limits.', timed_out: 'The metadata reader reached its time limit.',
  output_limit: 'The metadata reader reached its output limit.', cleanup_unconfirmed: 'Reader cleanup is unconfirmed. Its resources remain reserved. Reload archives or sessions to retry only that cleanup.',
  invalid_request: 'Choose a supported agent and an existing absolute archive folder.',
  read_unavailable: 'Archive storage is busy or unavailable. Retry explicitly.', worker_unavailable: 'The contained metadata reader is unavailable. Retry explicitly when resources are ready.',
};
export function failureText(value: unknown) {
  const code = typeof value === 'string' ? value : record(value) ? value.issue ?? value.code : null;
  return typeof code === 'string' && Object.hasOwn(FAILURES, code) ? FAILURES[code] : 'Session information could not be loaded. Retry explicitly.';
}

/** Metadata only; one callback outstanding, including across close/root changes. */
export function createSessionLibrary(options: Options) {
  const button = uiButton(''); button.id = 'sessions'; button.type = 'button'; button.textContent = 'Archive browser'; button.title = 'Page through one session archive, or add a read-only archive';
  document.querySelector('.toolbar')?.append(button);
  const dialog = uiPart('dialog'); dialog.id = 'session-library-dialog'; dialog.setAttribute('aria-labelledby', 'session-library-title');
  const heading = uiPart('header'); heading.classList.add('dialog-heading');
  const title = uiPart('title'); title.id = 'session-library-title'; title.textContent = 'Archive browser';
  const closeButton = closeControl('Close sessions'); heading.append(title, closeButton);
  const description = uiPart('paragraph', 'Browse the metadata of an explicitly selected archive. Browsing never starts or resumes an agent.');
  const tools = uiPart('toolbar');
  const loadRoots = uiButton('Load archives');
  const rootLabel = uiPart('label'); rootLabel.htmlFor = 'session-library-root'; rootLabel.textContent = 'Archive';
  const rootSelect = uiSelect(); rootSelect.id = 'session-library-root';
  const reload = uiButton('Reload sessions'); tools.append(loadRoots, rootLabel, rootSelect, reload);
  const archive = uiPart('details'); archive.hidden = !options.addArchive;
  const archiveHeading = uiPart('summary', 'Add a read-only archive');
  const archiveHelp = uiPart('hint', 'Choose an agent and its existing archive folder. Files stay in place and browsing does not change them.');
  const archiveForm = uiPart('form');
  const harnessLabel = uiPart('label'); harnessLabel.htmlFor = 'session-archive-agent'; harnessLabel.textContent = 'Agent';
  const harness = uiSelect(); harness.id = 'session-archive-agent'; harness.required = true;
  for (const [value, label] of [['', 'Choose agent'], ['claude_code', 'Claude Code'], ['codex', 'Codex'], ['pi', 'Pi']]) {
    const option = uiOption(value, label); harness.append(option);
  }
  const archivePathLabel = uiPart('label'); archivePathLabel.htmlFor = 'session-archive-path'; archivePathLabel.textContent = 'Archive folder';
  const archivePath = uiInput(); archivePath.id = 'session-archive-path'; archivePath.type = 'text'; archivePath.maxLength = 4096;
  archivePath.autocomplete = 'off'; archivePath.spellcheck = false; archivePath.required = true;
  const addArchive = uiButton(''); addArchive.type = 'submit'; addArchive.textContent = 'Add archive';
  archiveForm.append(harnessLabel, harness, archivePathLabel, archivePath, addArchive); archive.append(archiveHeading, archiveHelp, archiveForm);
  const source = uiPart('hint');
  const status = uiPart('status'); status.id = 'session-library-status'; status.setAttribute('role', 'status');
  const filterLabel = uiPart('label'); filterLabel.htmlFor = 'session-library-filter'; filterLabel.textContent = 'Filter this page';
  const filter = uiInput(); filter.id = 'session-library-filter'; filter.type = 'search'; filter.maxLength = 128; filter.autocomplete = 'off';
  const views = uiPart('stack'); views.setAttribute('role', 'group'); views.setAttribute('aria-label', 'Current page view');
  const listView = uiButton('List'); listView.setAttribute('aria-controls', 'session-library-list');
  const projectView = uiButton('Projects / sessions'); projectView.setAttribute('aria-controls', 'session-library-projects');
  views.append(listView, projectView);
  const viewScope = uiPart('hint');
  const scroll = uiTraits(uiPart('scroll'), 'bounded');
  const table = uiPart('table'); table.id = 'session-library-list'; table.setAttribute('aria-label', 'Session metadata');
  const head = uiPart('tableHead'); const header = uiPart('tableRow');
  for (const label of ['Session', 'Working folder', 'Updated']) { const cell = uiPart('columnHeader'); cell.scope = 'col'; cell.textContent = label; header.append(cell); }
  head.append(header); const body = uiPart('tableBody'); table.append(head, body); scroll.append(table);
  const projects = uiPart('stack'); projects.id = 'session-library-projects'; projects.setAttribute('aria-label', 'Projects and sessions on this page'); scroll.append(projects);
  const pagination = uiPart('actions'); pagination.classList.add('session-library-pagination');
  const previous = uiButton('Previous');
  const position = uiPart('text'); const next = uiButton('Next'); pagination.append(previous, position, next);
  const metadata = uiPart('descriptionList'); metadata.classList.add('session-library-metadata'); metadata.setAttribute('aria-label', 'Selected session metadata');
  const unavailable = uiPart('hint', 'Resume and hop require separately qualified native operations.');
  const actions = uiPart('actions');
  const detail = uiButton(options.openDetail ? 'Open detail' : 'Detail unavailable'); detail.disabled = true; actions.append(detail);
  for (const label of ['Resume unavailable', 'Hop unavailable']) { const action = uiButton(''); action.disabled = true; action.textContent = label; actions.append(action); }
  dialog.append(heading, description, tools, archive, source, status, filterLabel, filter, views, viewScope, scroll, pagination, metadata, unavailable, actions);
  (document.querySelector('#app') ?? document.body).append(dialog);
  let roots: SessionLibraryRoot[] = []; let page: SessionListPage | null = null; let cursor: SessionCursor | null = null;
  let history: (SessionCursor | null)[] = []; let selected: SessionMetadataRow | null = null;
  let view: 'list' | 'projects' = 'list'; const expandedProjects = new Set<string | null>();
  const renderedProjects = new Map<HTMLDetailsElement, { folder: string | null; owner: SessionListPage }>();
  let busy: 'roots' | 'list' | 'archive' | null = null; let generation = 0; let destroyed = false;
  let message = 'Load archives to choose an explicit store.'; let failed = false;
  const currentRoot = () => rootSelect.value === '' ? null : roots[Number(rootSelect.value)] ?? null;
  function metadataRow(label: string, value: string | number | boolean | null) {
    const term = uiPart('term', label); const detail = uiPart('definition', value === null || value === '' ? 'Unknown' : String(value)); metadata.append(term, detail);
  }
  function chooseSession(row: SessionMetadataRow, owner: SessionListPage) {
    // The title is what a person recognises; the ID stays visible beneath it, shortened, in full on hover.
    const choose = uiButton('');
    const name = uiPart('text', row.title || 'Untitled session');
    const native = uiPart('text', row.nativeId.length > 18 ? `${row.nativeId.slice(0, 8)}…${row.nativeId.slice(-4)}` : row.nativeId);
    choose.append(name, native); choose.title = row.nativeId; choose.setAttribute('aria-label', `${row.title || 'Untitled session'}, session ${row.nativeId}`);
    choose.dataset.nativeId = row.nativeId; choose.disabled = busy !== null;
    choose.setAttribute('aria-pressed', String(selected === row));
    choose.onclick = () => {
      const root = currentRoot();
      if (busy || !dialog.open || page !== owner || !root || !same(root, owner.root) || !owner.rows.includes(row)) return;
      selected = row; render();
    };
    return choose;
  }
  function render() {
    if (destroyed) return;
    const root = currentRoot(); loadRoots.disabled = busy !== null; rootSelect.disabled = busy === 'roots' || busy === 'archive' || !roots.length;
    detail.disabled = busy !== null || !selected || !root?.available || !options.openDetail;
    harness.disabled = busy !== null; archivePath.disabled = busy !== null; addArchive.disabled = busy !== null || !options.addArchive;
    reload.disabled = busy !== null || !root?.available; previous.disabled = busy !== null || history.length === 0; next.disabled = busy !== null || !page?.next;
    filter.disabled = page === null; dialog.setAttribute('aria-busy', String(busy !== null));
    source.textContent = root ? `Archive: ${root.storeRootId} · revision ${root.configurationRevision} · ${root.kind} · ${root.access === 'read_only' ? 'read only' : 'metadata reads only'}` : 'No archive selected.';
    status.textContent = busy ? busy === 'roots' ? 'Loading archives…' : busy === 'archive' ? 'Registering read-only archive…' : 'Reading bounded session metadata…' : message;
    status.dataset.state = failed ? 'error' : 'ready';
    listView.setAttribute('aria-pressed', String(view === 'list')); projectView.setAttribute('aria-pressed', String(view === 'projects'));
    table.hidden = view !== 'list'; projects.hidden = view !== 'projects';
    viewScope.textContent = view === 'projects'
      ? `Current page only: ${page?.rows.length ?? 0} sessions, grouped by the exact recorded working folder. Unknown project means the reader supplied no working folder. These groups are not linked to saved projects.`
      : 'List of session metadata on the current page.';
    // Native toggle events are queued; capture the actual open attributes before
    // rebuilding so immediate selection/filtering cannot lose an expansion.
    for (const [group, state] of renderedProjects) {
      if (group.isConnected && state.owner === page) {
        if (group.open) expandedProjects.add(state.folder); else expandedProjects.delete(state.folder);
      }
    }
    renderedProjects.clear();
    const query = filter.value.toLocaleLowerCase(); body.replaceChildren(); projects.replaceChildren();
    const visible = page?.rows.filter(row => [row.nativeId, row.title, row.workingDirectory, row.updatedAt].some(value => value?.toLocaleLowerCase().includes(query))) ?? [];
    if (page && view === 'list') {
      for (const row of visible) {
        const line = uiPart('tableRow'); line.setAttribute('aria-selected', String(selected === row));
        const id = uiPart('cell'); id.append(chooseSession(row, page)); line.append(id);
        const folder = uiPart('cell', row.workingDirectory || 'Unknown'); folder.title = row.workingDirectory || '';
        const updated = uiPart('cell', when(row.updatedAt)); updated.title = row.updatedAt || '';
        line.append(folder, updated); body.append(line);
      }
    } else if (page) {
      const owner = page; const groups = new Map<string | null, SessionMetadataRow[]>();
      for (const row of visible) {
        // No path resolution, case folding or relationship to saved projects.
        const folder = row.workingDirectory?.trim() ? row.workingDirectory : null;
        const members = groups.get(folder) ?? []; members.push(row); groups.set(folder, members);
      }
      for (const [folder, members] of groups) {
        const group = uiPart('details'); group.classList.add('session-project-group'); group.open = expandedProjects.has(folder);
        const summary = uiPart('summary');
        const name = uiPart('text', folder ?? 'Unknown project');
        const count = uiPart('text', `${members.length} on this page`); summary.append(name, count);
        const items = uiPart('list');
        for (const row of members) {
          const item = uiPart('item'); const choose = chooseSession(row, owner);
          const fields = uiPart('stack');
          const rowTitle = uiPart('text', row.title || 'Unknown title');
          const updated = uiPart('text', row.updatedAt || 'Unknown update time');
          fields.append(rowTitle, updated); item.append(choose, fields); items.append(item);
        }
        group.append(summary, items);
        group.ontoggle = () => {
          if (!group.isConnected || page !== owner) return;
          if (group.open) expandedProjects.add(folder); else expandedProjects.delete(folder);
        };
        renderedProjects.set(group, { folder, owner });
        projects.append(group);
      }
    }
    position.textContent = page ? `${page.total ? (cursor?.offset ?? 0) + 1 : 0}–${(cursor?.offset ?? 0) + page.rows.length} of ${page.total} · ${visible.length} shown${!history.length && (cursor?.offset ?? 0) > 0 ? ' · Reload for first page' : ''}` : 'No page loaded';
    metadata.replaceChildren();
    if (selected) {
      metadataRow('Native ID', selected.nativeId); metadataRow('Title', selected.title); metadataRow('Created', selected.createdAt);
      metadataRow('Updated', selected.updatedAt); metadataRow('Working folder', selected.workingDirectory); metadataRow('Parent ID', selected.parentNativeId);
      metadataRow('Turns', selected.turns); metadataRow('Bytes', selected.bytes); metadataRow('Archived', selected.archived);
      if (selected.metadataTruncated) metadataRow('Some metadata', 'Unavailable because its fixed field limits were exceeded');
    }
  }
  function reset() { page = null; cursor = null; history = []; selected = null; expandedProjects.clear(); renderedProjects.clear(); filter.value = ''; failed = false; }
  function replaceRoots(received: SessionLibraryRoot[], selectedRoot: SessionRootSelector | null) {
    roots = received; reset(); rootSelect.replaceChildren();
    const placeholder = uiOption('', 'Choose an archive explicitly'); rootSelect.append(placeholder);
    roots.forEach((root, index) => { const option = uiOption(String(index), `${root.label || root.harness} · ${root.kind} · revision ${root.configurationRevision}${root.available ? '' : ' · Unavailable'}`); rootSelect.append(option); });
    const retained = selectedRoot ? roots.findIndex(root => same(root, selectedRoot)) : -1; rootSelect.value = retained >= 0 ? String(retained) : '';
    loadRoots.textContent = 'Reload archives';
  }
  async function readRoots() {
    if (busy) return; const at = generation; const before = currentRoot(); busy = 'roots'; failed = false; render();
    try {
      const received = rootsValue(await options.roots()); if (!received) throw new Error('Invalid archive projection');
      if (destroyed || at !== generation || !dialog.open) return;
      replaceRoots(received, before); message = roots.length ? 'Choose an archive to read its metadata.' : 'No registered archives. No ambient home was searched.';
    } catch (error) {
      if (!destroyed && at === generation && dialog.open) { reset(); failed = true; message = failureText(error); options.reportError(message); }
    } finally { busy = null; render(); }
  }
  async function registerArchive() {
    if (busy || !options.addArchive) return;
    const agent = harness.value; const path = archivePath.value.trim();
    if (!['claude_code', 'codex', 'pi'].includes(agent) || !path || !text(path, 4096)) {
      failed = true; message = FAILURES.invalid_request; render(); options.reportError(message); return;
    }
    const at = generation; busy = 'archive'; failed = false; render(); let registered = false;
    try {
      const added = rootsValue([await options.addArchive({ harness: agent as SessionArchiveAddRequest['harness'], path })]);
      if (!added || added[0].harness !== agent) throw new Error('Invalid archive projection');
      registered = true;
      if (destroyed || at !== generation || !dialog.open) return;
      const received = rootsValue(await options.roots()); if (!received) throw new Error('Invalid archive projection');
      if (destroyed || at !== generation || !dialog.open) return;
      replaceRoots(received, added[0]); archivePath.value = ''; archive.open = false;
      message = added[0].available ? 'Archive registered. Use Reload sessions to browse metadata; the archive files remain unchanged.'
        : 'This archive is already registered but unavailable. Reconnect it explicitly; its history was preserved.';
    } catch (error) {
      if (!destroyed && at === generation && dialog.open) {
        failed = true; message = registered ? 'The archive was registered, but its list could not be refreshed. Load archives explicitly.' : failureText(error);
        options.reportError(message);
      }
    } finally { busy = null; render(); }
  }
  async function readPage(requested: SessionCursor | null, direction: 'reload' | 'next' | 'previous') {
    const root = currentRoot(); if (busy || !root?.available) return; const at = generation; busy = 'list'; failed = false; render();
    try {
      const received = pageValue(await options.list({ root: { storeRootId: root.storeRootId, configurationRevision: root.configurationRevision }, cursor: requested, limit: 50, includeChildren: true }), root, requested);
      if (!received) throw new Error('Invalid metadata projection');
      if (destroyed || at !== generation || !dialog.open || !currentRoot() || !same(root, currentRoot()!)) return;
      if (direction === 'reload') history = [];
      else if (direction === 'next') { history.push(cursor); if (history.length > 64) history.shift(); }
      else history.pop();
      page = received; cursor = requested; selected = null; expandedProjects.clear();
      message = page.total ? 'Metadata only. Select a session ID to inspect the listed fields.' : 'This archive contains no listed sessions.';
    } catch (error) {
      if (!destroyed && at === generation && dialog.open) { reset(); failed = true; message = failureText(error); options.reportError(message); }
    } finally { busy = null; render(); }
  }
  function close() { generation++; if (dialog.open) dialog.close(); }
  function show() {
    if (destroyed || dialog.open || document.querySelector('dialog[open][aria-modal="true"]')) return;
    generation++; reset(); rootSelect.value = ''; message = busy ? 'The previous read is still returning; no replacement was started.' : 'Load archives to choose an explicit store.'; render(); openModal(dialog); closeButton.focus();
  }
  rootSelect.onchange = () => { generation++; reset(); const root = currentRoot(); message = root ? root.available ? 'Ready to read this explicit archive.' : 'This archive is unavailable. No other archive was used.' : 'Choose an archive explicitly.'; render(); if (!busy && root?.available) void readPage(null, 'reload'); };
  loadRoots.onclick = () => { void readRoots(); }; reload.onclick = () => { void readPage(null, 'reload'); };
  archiveForm.onsubmit = event => { event.preventDefault(); void registerArchive(); };
  previous.onclick = () => { if (history.length) void readPage(history.at(-1) ?? null, 'previous'); };
  next.onclick = () => { if (page?.next) void readPage(page.next, 'next'); }; filter.oninput = render;
  listView.onclick = () => { view = 'list'; render(); }; projectView.onclick = () => { view = 'projects'; render(); };
  button.onclick = show; closeButton.onclick = close;
  detail.onclick = () => { const root = currentRoot(); if (!busy && root?.available && selected) options.openDetail?.({storeRootId:root.storeRootId,configurationRevision:root.configurationRevision},selected.nativeId); };
  dialog.addEventListener('close', () => { if (!dialog.open) { generation++; if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); } });
  render();
  return { show, close, destroy: () => { destroyed = true; close(); button.remove(); dialog.remove(); } };
}
