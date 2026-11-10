import { openModal } from './modal.ts';
import './files-panel.css';

type Entry = { name: string; path: string; kind: 'file' | 'directory' | 'link'; size: number | null; linkInside: boolean | null };
type Listing = { path: string; entries: Entry[]; truncated: boolean };
type Search = { query: string; hits: Entry[]; truncated: boolean; cancelled: boolean; timedOut: boolean };
type TextFile = { path: string; text: string | null; binary: boolean; truncated: boolean; bytes: number; sha256: string };
type SaveOutcome = { state: 'saved'; sha256: string } | { state: 'conflict'; disk_sha256: string } | { state: 'failed'; message: string };
type Buffer = { path: string; baseText: string; baseSha256: string; text: string; editable: boolean };
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
type Options = { invoke: Invoke; project: () => string | null; reportError: (message: string) => void;
  decide: (title: string, message: string, accept: string) => Promise<boolean>; restoreFocus?: () => void };

const id = () => crypto.randomUUID();
const message = (error: unknown) => typeof error === 'string' ? error : error instanceof Error ? error.message : 'The file operation failed.';

/** Read-only browsing plus explicit, version-checked saves. Opening a file never runs it. */
export function createFilesPanel(options: Options) {
  const dialog = document.createElement('dialog'); dialog.id = 'files-dialog'; dialog.setAttribute('aria-labelledby', 'files-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'files-title'; title.textContent = 'Project files';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close files');
  const chooser = document.createElement('select'); chooser.className = 'files-project'; chooser.setAttribute('aria-label', 'Project');
  heading.append(title, chooser, closeButton);
  const body = document.createElement('div'); body.className = 'files-body';
  const side = document.createElement('div'); side.className = 'files-side';
  const search = document.createElement('input'); search.type = 'search'; search.placeholder = 'Find a file by name'; search.setAttribute('aria-label', 'Find a file by name');
  const crumbs = document.createElement('div'); crumbs.className = 'files-crumbs';
  const list = document.createElement('ul'); list.className = 'files-list'; list.setAttribute('role', 'listbox');
  side.append(search, crumbs, list);
  const main = document.createElement('div'); main.className = 'files-main';
  const bar = document.createElement('div'); bar.className = 'files-bar';
  const name = document.createElement('span'); name.className = 'files-name';
  const modes = document.createElement('div'); modes.className = 'files-modes';
  const modeButtons = new Map<string, HTMLButtonElement>();
  for (const mode of ['source', 'preview', 'split']) {
    const button = document.createElement('button'); button.type = 'button'; button.textContent = mode[0].toUpperCase() + mode.slice(1);
    button.onclick = () => { view = mode as typeof view; render(); }; modes.append(button); modeButtons.set(mode, button);
  }
  const save = document.createElement('button'); save.type = 'button'; save.className = 'primary'; save.textContent = 'Save';
  bar.append(name, modes, save);
  const panes = document.createElement('div'); panes.className = 'files-panes';
  const editor = document.createElement('textarea'); editor.className = 'files-editor'; editor.spellcheck = false; editor.setAttribute('aria-label', 'File source');
  const preview = document.createElement('div'); preview.className = 'files-preview'; preview.setAttribute('aria-label', 'Rendered Markdown');
  const picture = document.createElement('div'); picture.className = 'files-picture'; picture.hidden = true;
  panes.append(editor, preview, picture);
  const status = document.createElement('p'); status.className = 'files-status'; status.setAttribute('role', 'status');
  main.append(bar, panes, status);
  body.append(side, main);
  dialog.append(heading, body);
  (document.querySelector('#app') ?? document.body).append(dialog);

  const buffers = new Map<string, Buffer>();
  let current: Buffer | null = null; let folder = ''; let view: 'source' | 'preview' | 'split' = 'source';
  let pending: string | null = null; let renderGeneration = 0; let renderTimer = 0;
  const markdown = (path: string) => /\.(md|markdown)$/i.test(path);
  const setStatus = (text: string, error = false) => { status.textContent = text; status.dataset.state = error ? 'error' : 'ready'; };
  const projectPath = () => chooser.value || null;

  function render() {
    const md = !!current && markdown(current.path);
    modes.hidden = !md; if (!md) view = 'source';
    for (const [mode, button] of modeButtons) button.setAttribute('aria-pressed', String(mode === view));
    editor.hidden = !current || view === 'preview'; preview.hidden = !current || view === 'source';
    save.disabled = !current?.editable || current.text === current.baseText;
    name.textContent = current ? `${current.path}${current.text !== current.baseText ? ' •' : ''}` : 'No file open';
    editor.readOnly = !current?.editable;
  }
  function schedulePreview() {
    if (!current || !markdown(current.path) || view === 'source') return;
    clearTimeout(renderTimer);
    const at = ++renderGeneration; const text = current.text;
    renderTimer = window.setTimeout(async () => {
      try {
        const result = await options.invoke<{ html: string; truncated: boolean }>('files_markdown', { text });
        if (at !== renderGeneration) return; // a newer render won; discard this one
        preview.innerHTML = result.html; // produced by the Rust renderer: escaped, fixed tags, no URLs
        if (result.truncated) setStatus('The preview shows the first 2 MiB only.');
      } catch (error) { if (at === renderGeneration) setStatus(message(error), true); }
    }, 150);
  }
  async function cancelPending() { if (pending) { const old = pending; pending = null; await options.invoke('files_cancel', { requestId: old }).catch(() => {}); } }
  function entryRow(entry: Entry) {
    const item = document.createElement('li');
    const button = document.createElement('button'); button.type = 'button';
    const icon = entry.kind === 'directory' ? '▸ ' : entry.kind === 'link' ? '↪ ' : '';
    button.textContent = `${icon}${entry.name}`;
    button.title = entry.kind === 'link' ? (entry.linkInside ? 'Link inside the project' : 'Link outside the project: not opened') : entry.path;
    button.disabled = entry.kind === 'link' && !entry.linkInside;
    button.onclick = () => { void (entry.kind === 'directory' || entry.kind === 'link' ? openFolder(entry.path).catch(() => openFile(entry.path)) : openFile(entry.path)); };
    item.append(button); return item;
  }
  async function openFolder(path: string) {
    const project = projectPath(); if (!project) throw new Error('Choose a project first.');
    await cancelPending(); const request = id(); pending = request;
    const listing = await options.invoke<Listing>('files_list', { project, path, requestId: request });
    if (pending !== request) return; pending = null;
    folder = listing.path; list.replaceChildren();
    crumbs.textContent = `/${folder}`;
    if (folder) {
      const up = document.createElement('li'); const button = document.createElement('button'); button.type = 'button'; button.textContent = '..';
      button.onclick = () => { void openFolder(folder.split('/').slice(0, -1).join('/')).catch(error => setStatus(message(error), true)); };
      up.append(button); list.append(up);
    }
    for (const entry of listing.entries) list.append(entryRow(entry));
    if (listing.truncated) setStatus('This folder has more than 2,000 entries; only the first are shown.');
  }
  async function confirmLeave(): Promise<boolean> {
    if (!current || current.text === current.baseText) return true;
    return options.decide('Unsaved changes', `${current.path} has unsaved changes. They stay in this window while it is open; nothing is written until you press Save.`, 'Keep editing elsewhere');
  }
  async function openImage(path: string) {
    const project = projectPath(); if (!project) return;
    const outcome = await options.invoke<{ state: 'ready'; image: { path: string; width: number; height: number; bytes: number; dataUrl: string } } | { state: 'refused'; message: string }>('files_image', { project, path });
    current = null; editor.value = ''; preview.replaceChildren(); render(); editor.hidden = true; preview.hidden = true;
    picture.replaceChildren(); picture.hidden = false;
    if (outcome.state === 'refused') { name.textContent = path; setStatus(outcome.message, true); return; }
    const img = document.createElement('img'); img.alt = outcome.image.path; img.src = outcome.image.dataUrl; picture.append(img);
    name.textContent = outcome.image.path; setStatus(`${outcome.image.width} x ${outcome.image.height} pixels, ${outcome.image.bytes.toLocaleString()} bytes. Read-only.`);
  }
  async function openFile(path: string) {
    const project = projectPath(); if (!project) throw new Error('Choose a project first.');
    picture.hidden = true; picture.replaceChildren();
    if (/\.(png|jpe?g|gif|webp|docx?)$/i.test(path)) { await openImage(path); return; }
    let buffer = buffers.get(path);
    if (!buffer) {
      const file = await options.invoke<TextFile>('files_read', { project, path });
      if (file.binary || file.text === null) { setStatus(`${file.path} is not text (${file.bytes} bytes); it is not opened or run.`); return; }
      buffer = { path: file.path, baseText: file.text, baseSha256: file.sha256, text: file.text, editable: !file.truncated };
      buffers.set(file.path, buffer);
      if (file.truncated) setStatus('This file is larger than 4 MiB: shown read-only, first part only.');
    }
    current = buffer; editor.value = buffer.text; preview.replaceChildren();
    if (markdown(buffer.path) && view === 'source') view = 'split';
    setStatus(buffer.editable ? 'Opened for reading. Edit and press Save to write it.' : status.textContent ?? ''); render(); schedulePreview();
  }
  editor.addEventListener('input', () => { if (current?.editable) { current.text = editor.value; render(); schedulePreview(); } });
  preview.addEventListener('click', event => {
    const link = (event.target as HTMLElement).closest('a[data-path]');
    if (!link) return; event.preventDefault();
    const base = current?.path.split('/').slice(0, -1).join('/') ?? '';
    const target = link.getAttribute('data-path') ?? '';
    void openFile(base ? `${base}/${target}` : target).catch(error => setStatus(message(error), true));
  });
  async function saveCurrent(overwrite = false): Promise<boolean> {
    const project = projectPath(); const buffer = current;
    if (!project || !buffer?.editable) return false;
    const outcome = await options.invoke<SaveOutcome>('files_save', { request: { project, path: buffer.path, text: buffer.text, baseSha256: buffer.baseSha256, overwrite } });
    if (outcome.state === 'saved') { buffer.baseSha256 = outcome.sha256; buffer.baseText = buffer.text; setStatus('Saved.'); render(); return true; }
    if (outcome.state === 'conflict') {
      const replace = await options.decide('The file changed on disk', `${buffer.path} was changed by someone else (an agent?) after you opened it. Overwrite it with your version? Cancel keeps your text here unsaved.`, 'Overwrite');
      return replace ? saveCurrent(true) : (setStatus('Not saved: the file on disk is newer. Your text is still here.', true), false);
    }
    setStatus(outcome.message, true); return false;
  }
  save.onclick = () => { void saveCurrent().catch(error => setStatus(message(error), true)); };
  let searchTimer = 0;
  search.addEventListener('input', () => {
    clearTimeout(searchTimer);
    searchTimer = window.setTimeout(async () => {
      const query = search.value.trim(); const project = projectPath();
      if (!query) { void openFolder(folder).catch(() => {}); return; }
      if (!project) return;
      await cancelPending(); const request = id(); pending = request;
      setStatus('Searching…');
      try {
        const result = await options.invoke<Search>('files_search', { project, query, requestId: request });
        if (pending !== request) return; pending = null;
        list.replaceChildren(...result.hits.map(entryRow));
        crumbs.textContent = `Results for “${result.query}”`;
        setStatus(result.cancelled ? 'Search cancelled.' : result.timedOut ? 'Search stopped after 5 seconds; results are partial.'
          : result.truncated ? 'Too many matches; refine the name.' : `${result.hits.length} match${result.hits.length === 1 ? '' : 'es'}.`);
      } catch (error) { if (pending === request) { pending = null; setStatus(message(error), true); } }
    }, 250);
  });
  closeButton.onclick = async () => {
    const dirty = [...buffers.values()].filter(b => b.text !== b.baseText);
    if (dirty.length && !await options.decide('Discard unsaved changes?', `${dirty.map(b => b.path).join(', ')} ${dirty.length === 1 ? 'has' : 'have'} unsaved changes. Closing discards them; Cancel keeps the files open.`, 'Discard')) return;
    dialog.close();
  };
  dialog.addEventListener('cancel', event => { event.preventDefault(); closeButton.click(); });
  dialog.addEventListener('close', () => { void cancelPending(); buffers.clear(); current = null; editor.value = ''; preview.replaceChildren(); render(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  chooser.onchange = async () => {
    const dirty = [...buffers.values()].some(b => b.text !== b.baseText);
    if (dirty && !await options.decide('Discard unsaved changes?', 'Switching project discards unsaved changes in this panel.', 'Discard')) return;
    buffers.clear(); current = null; editor.value = ''; preview.replaceChildren(); render();
    try { await openFolder(''); } catch (error) { setStatus(message(error), true); }
  };
  async function show() {
    const projects = await options.invoke<{ path: string; name: string; available: boolean }[]>('project_recent').catch(() => []);
    const usable = projects.filter(p => p.available);
    if (!usable.length) { options.reportError('Open a project first; only your projects can be browsed.'); return; }
    const preferred = options.project()?.replace(/[\\/]+$/, '').toLowerCase();
    chooser.replaceChildren(...usable.map(p => { const o = document.createElement('option'); o.value = p.path; o.textContent = p.name || p.path; return o; }));
    chooser.value = usable.find(p => p.path.replace(/[\\/]+$/, '').toLowerCase() === preferred)?.path ?? usable[0].path;
    if (!dialog.open) openModal(dialog);
    render(); setStatus('Files open for reading; nothing is run.');
    try { await openFolder(''); } catch (error) { setStatus(message(error), true); }
    search.focus();
  }
  void confirmLeave; render();
  return { show };
}
