import { openModal } from './modal.ts';
import { createFilesTree, type Entry, type Listing } from './files-tree.ts';
import type { CodeView } from './code-view.ts';
import './files-panel.css';

type Search = { query: string; hits: Entry[]; truncated: boolean; cancelled: boolean; timedOut: boolean };
type TextFile = { path: string; text: string | null; binary: boolean; truncated: boolean; bytes: number; sha256: string };
type SaveOutcome = { state: 'saved'; sha256: string } | { state: 'conflict'; disk_sha256: string } | { state: 'failed'; message: string };
type VisualBlock = { kind: 'heading' | 'paragraph' | 'code' | 'blank' | 'preserved'; prefix: string; text: string; suffix: string; level: number; editable: boolean };
type VisualDocument = { blocks: VisualBlock[]; exact: boolean; editableBlocks: number; preservedBlocks: number };
type Buffer = { path: string; baseText: string; baseSha256: string; text: string; editable: boolean };
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
type Options = { invoke: Invoke; project: () => string | null; reportError: (message: string) => void;
  decide: (title: string, message: string, accept: string) => Promise<boolean>; restoreFocus?: () => void };

const id = () => crypto.randomUUID();
const message = (error: unknown) => typeof error === 'string' ? error : error instanceof Error ? error.message : 'The file operation failed.';
const MODES = [['source', 'Source'], ['split', 'Split'], ['preview', 'Preview'], ['visual', 'Visual']] as const;

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
  const search = document.createElement('input'); search.type = 'search'; search.className = 'files-search'; search.placeholder = 'Find a file by name'; search.setAttribute('aria-label', 'Find a file by name');
  const tree = createFilesTree({
    list: async path => {
      const project = projectPath(); if (!project) throw 'Choose a project first.';
      return options.invoke<Listing>('files_list', { project, path, requestId: id() });
    },
    open: entry => { void openFile(entry.path).catch(error => setStatus(message(error), true)); },
    failed: text => setStatus(text, true),
  });
  side.append(search, tree.element);
  const main = document.createElement('div'); main.className = 'files-main';
  const bar = document.createElement('div'); bar.className = 'files-bar';
  const crumbs = document.createElement('nav'); crumbs.className = 'files-crumbs'; crumbs.setAttribute('aria-label', 'Open file path');
  const language = document.createElement('span'); language.className = 'files-language';
  const modes = document.createElement('div'); modes.className = 'files-modes'; modes.setAttribute('role', 'group'); modes.setAttribute('aria-label', 'View');
  const modeButtons = new Map<string, HTMLButtonElement>();
  for (const [mode, label] of MODES) {
    const button = document.createElement('button'); button.type = 'button'; button.textContent = label;
    button.onclick = () => { view = mode; render(); if (view === 'visual') void renderVisual(); else schedulePreview(); }; modes.append(button); modeButtons.set(mode, button);
  }
  const save = document.createElement('button'); save.type = 'button'; save.className = 'primary'; save.textContent = 'Save';
  bar.append(crumbs, language, modes, save);
  const panes = document.createElement('div'); panes.className = 'files-panes';
  const code = document.createElement('div'); code.className = 'files-code'; code.setAttribute('aria-label', 'File source');
  const preview = document.createElement('div'); preview.className = 'files-preview'; preview.setAttribute('aria-label', 'Rendered Markdown');
  const picture = document.createElement('div'); picture.className = 'files-picture'; picture.hidden = true;
  const visual = document.createElement('div'); visual.className = 'files-visual'; visual.hidden = true; visual.setAttribute('aria-label', 'Visual editor');
  const empty = document.createElement('div'); empty.className = 'files-empty'; empty.textContent = 'Choose a file in the tree, or find one by name.';
  panes.append(code, preview, picture, visual, empty);
  const status = document.createElement('p'); status.className = 'files-status'; status.setAttribute('role', 'status');
  main.append(bar, panes, status);
  const split = document.createElement('div'); split.className = 'files-split'; split.tabIndex = 0;
  split.setAttribute('role', 'separator'); split.setAttribute('aria-orientation', 'vertical'); split.setAttribute('aria-label', 'Resize the folder tree');
  body.append(side, split, main);
  dialog.append(heading, body);
  // The tree's width is the user's: dragged or nudged with the arrows, kept for the next opening.
  const SIDE_KEY = 'shinatama.files.side';
  const sideWidth = (value: number) => Math.round(Math.max(180, Math.min(value, body.clientWidth * 0.6 || 640)));
  function setSide(value: number, keep = false) {
    const width = sideWidth(value); body.style.setProperty('--files-side', `${width}px`);
    split.setAttribute('aria-valuenow', String(width));
    if (keep) try { localStorage.setItem(SIDE_KEY, String(width)); } catch { /* the width is only a convenience */ }
  }
  try { const saved = Number(localStorage.getItem(SIDE_KEY)); if (saved > 0) body.style.setProperty('--files-side', `${saved}px`); } catch { /* default width */ }
  split.addEventListener('pointerdown', event => {
    if (event.button !== 0) return; event.preventDefault();
    split.setPointerCapture(event.pointerId); split.classList.add('dragging'); dialog.classList.add('resizing');
    const left = body.getBoundingClientRect().left;
    const move = (moved: PointerEvent) => setSide(moved.clientX - left);
    const done = (ended: PointerEvent) => { split.removeEventListener('pointermove', move); split.classList.remove('dragging'); dialog.classList.remove('resizing'); setSide(ended.clientX - left, true); };
    split.addEventListener('pointermove', move); split.addEventListener('pointerup', done, { once: true }); split.addEventListener('pointercancel', done, { once: true });
  });
  split.addEventListener('keydown', event => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault(); setSide(side.getBoundingClientRect().width + (event.key === 'ArrowLeft' ? -24 : 24), true);
  });
  (document.querySelector('#app') ?? document.body).append(dialog);

  const buffers = new Map<string, Buffer>();
  let current: Buffer | null = null; let showingImage = false; let view: 'source' | 'preview' | 'split' | 'visual' = 'source';
  let visualAvailable = false; let visualGeneration = 0;
  let pending: string | null = null; let renderGeneration = 0; let renderTimer = 0;
  let codeView: CodeView | null = null; let codeLoading: Promise<CodeView> | null = null;
  const markdown = (path: string) => /\.(md|markdown)$/i.test(path);
  const setStatus = (text: string, error = false) => { status.textContent = text; status.dataset.state = error ? 'error' : 'ready'; };
  const projectPath = () => chooser.value || null;
  /** CodeMirror loads with the first file opened, not with the app. */
  function editor(): Promise<CodeView> {
    if (codeView) return Promise.resolve(codeView);
    codeLoading ??= import('./code-view.ts').then(module => {
      codeView = module.createCodeView(code, (path, text) => {
        const buffer = buffers.get(path); if (!buffer?.editable) return;
        buffer.text = text; if (buffer === current) { render(); schedulePreview(); }
      });
      return codeView;
    });
    return codeLoading;
  }

  function paintCrumbs() {
    if (!current && !showingImage) { crumbs.replaceChildren(Object.assign(document.createElement('span'), { className: 'files-crumb-none', textContent: 'No file open' })); return; }
    const path = current?.path ?? crumbs.dataset.path ?? '';
    const parts = path.split('/').filter(Boolean); const items: Node[] = [];
    parts.forEach((part, index) => {
      if (index) items.push(Object.assign(document.createElement('span'), { className: 'files-crumb-sep', textContent: '/' }));
      if (index === parts.length - 1) {
        const leaf = document.createElement('strong'); leaf.className = 'files-crumb-leaf'; leaf.textContent = part; items.push(leaf);
        if (current && current.text !== current.baseText) { const dirty = document.createElement('span'); dirty.className = 'files-dirty'; dirty.title = 'Unsaved changes'; dirty.setAttribute('aria-label', 'Unsaved changes'); items.push(dirty); }
        return;
      }
      const folder = document.createElement('button'); folder.type = 'button'; folder.className = 'files-crumb'; folder.textContent = part;
      folder.title = `Show ${parts.slice(0, index + 1).join('/')} in the tree`;
      folder.onclick = () => { search.value = ''; void tree.reveal(parts.slice(0, index + 1).join('/')); };
      items.push(folder);
    });
    crumbs.replaceChildren(...items);
  }
  function render() {
    const md = !!current && markdown(current.path);
    modes.hidden = !md; if (!md) view = 'source';
    if (view === 'visual' && !visualAvailable) view = 'source';
    for (const [mode, button] of modeButtons) {
      button.setAttribute('aria-pressed', String(mode === view));
      if (mode === 'visual') { button.disabled = !visualAvailable; button.title = visualAvailable ? 'Edit headings, paragraphs and code; other Markdown is kept as source' : 'Not available: this document does not round-trip exactly'; }
    }
    code.hidden = !current || view === 'preview' || view === 'visual'; preview.hidden = !current || view === 'source' || view === 'visual'; visual.hidden = !current || view !== 'visual';
    empty.hidden = !!current || showingImage;
    panes.dataset.view = view;
    save.hidden = !current; save.disabled = !current?.editable || current.text === current.baseText;
    paintCrumbs();
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
  async function openImage(path: string) {
    const project = projectPath(); if (!project) return;
    const outcome = await options.invoke<{ state: 'ready'; image: { path: string; width: number; height: number; bytes: number; dataUrl: string } } | { state: 'refused'; message: string }>('files_image', { project, path });
    current = null; showingImage = true; crumbs.dataset.path = path; language.textContent = 'Image'; preview.replaceChildren(); render();
    picture.replaceChildren(); picture.hidden = false;
    if (outcome.state === 'refused') { setStatus(outcome.message, true); return; }
    const img = document.createElement('img'); img.alt = outcome.image.path; img.src = outcome.image.dataUrl; picture.append(img);
    setStatus(`${outcome.image.width} × ${outcome.image.height} pixels, ${outcome.image.bytes.toLocaleString()} bytes. Read-only.`);
  }
  async function openFile(path: string) {
    const project = projectPath(); if (!project) throw new Error('Choose a project first.');
    picture.hidden = true; picture.replaceChildren(); showingImage = false;
    if (/\.(png|jpe?g|gif|webp|docx?)$/i.test(path)) { await openImage(path); return; }
    let buffer = buffers.get(path); let note = '';
    if (!buffer) {
      const file = await options.invoke<TextFile>('files_read', { project, path });
      if (file.binary || file.text === null) { setStatus(`${file.path} is not text (${file.bytes.toLocaleString()} bytes); it is not opened or run.`); return; }
      buffer = { path: file.path, baseText: file.text, baseSha256: file.sha256, text: file.text, editable: !file.truncated };
      buffers.set(file.path, buffer);
      if (file.truncated) note = 'This file is larger than 4 MiB: shown read-only, first part only.';
    }
    const view_ = await editor();
    current = buffer; preview.replaceChildren();
    await view_.open(buffer.path, buffer.text, buffer.editable);
    const { languageOf } = await import('./code-view.ts');
    language.textContent = languageOf(buffer.path);
    tree.select(buffer.path);
    visualAvailable = false;
    if (markdown(buffer.path) && buffer.editable) {
      const doc = await options.invoke<VisualDocument>('files_visual_parse', { text: buffer.text }).catch(() => null);
      visualAvailable = !!doc && doc.exact && doc.editableBlocks > 0;
    }
    if (markdown(buffer.path) && view === 'source') view = 'split';
    const lines = buffer.text.split('\n').length;
    setStatus(note || `${lines.toLocaleString()} line${lines === 1 ? '' : 's'} · ${buffer.editable ? 'edit and press Save to write it' : 'read-only'}`);
    render(); schedulePreview();
  }
  async function renderVisual() {
    if (!current || view !== 'visual') return;
    const at = ++visualGeneration;
    const doc = await options.invoke<VisualDocument>('files_visual_parse', { text: current.text }).catch(() => null);
    if (at !== visualGeneration || !current) return;
    if (!doc || !doc.exact) { visualAvailable = false; view = 'source'; render(); setStatus('Visual mode is not available for this document; use Source.', true); return; }
    visual.replaceChildren();
    doc.blocks.forEach((block, index) => {
      if (block.kind === 'blank') return;
      if (!block.editable) {
        const kept = document.createElement('pre'); kept.className = 'files-visual-kept'; kept.textContent = block.prefix + block.text + block.suffix;
        kept.title = 'Kept as source: not part of the visual subset'; visual.append(kept); return;
      }
      const field = block.kind === 'heading' ? document.createElement('input') : document.createElement('textarea');
      field.className = `files-visual-${block.kind}${block.kind === 'heading' ? ` level-${block.level}` : ''}`;
      field.value = block.kind === 'code' ? block.text.replace(/\r?\n$/, '') : block.text;
      field.setAttribute('aria-label', `${block.kind} ${index}`);
      if (field instanceof HTMLTextAreaElement) field.rows = Math.min(12, Math.max(2, field.value.split('\n').length));
      let timer = 0;
      field.addEventListener('input', () => {
        clearTimeout(timer);
        timer = window.setTimeout(async () => {
          if (!current) return;
          try {
            const text = await options.invoke<string>('files_visual_apply', { text: current.text, edits: [{ index, text: field.value }] });
            current.text = text; codeView?.replace(text); field.classList.remove('invalid'); setStatus('Edited visually; Save writes the file.'); render();
          } catch (error) { field.classList.add('invalid'); setStatus(message(error), true); }
        }, 300);
      });
      visual.append(field);
    });
  }
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
  // Ctrl+S saves while the panel has the keyboard.
  dialog.addEventListener('keydown', event => {
    if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 's') { event.preventDefault(); if (!save.disabled && !save.hidden) save.click(); }
  });
  let searchTimer = 0;
  search.addEventListener('input', () => {
    clearTimeout(searchTimer);
    searchTimer = window.setTimeout(async () => {
      const query = search.value.trim(); const project = projectPath();
      if (!query) { await cancelPending(); tree.results(null); setStatus(current ? status.textContent ?? '' : 'Files open for reading; nothing is run.'); return; }
      if (!project) return;
      await cancelPending(); const request = id(); pending = request;
      setStatus('Searching…');
      try {
        const result = await options.invoke<Search>('files_search', { project, query, requestId: request });
        if (pending !== request) return; pending = null;
        tree.results(result.hits);
        setStatus(result.cancelled ? 'Search cancelled.' : result.timedOut ? 'Search stopped after 5 seconds; results are partial.'
          : result.truncated ? 'Too many matches; refine the name.' : `${result.hits.length} match${result.hits.length === 1 ? '' : 'es'} for “${result.query}”.`);
      } catch (error) { if (pending === request) { pending = null; setStatus(message(error), true); } }
    }, 250);
  });
  search.addEventListener('keydown', event => {
    if (event.key === 'ArrowDown') { event.preventDefault(); tree.focus(); }
    else if (event.key === 'Escape' && search.value) { event.preventDefault(); event.stopPropagation(); search.value = ''; search.dispatchEvent(new Event('input')); }
  });
  closeButton.onclick = async () => {
    const dirty = [...buffers.values()].filter(b => b.text !== b.baseText);
    if (dirty.length && !await options.decide('Discard unsaved changes?', `${dirty.map(b => b.path).join(', ')} ${dirty.length === 1 ? 'has' : 'have'} unsaved changes. Closing discards them; Cancel keeps the files open.`, 'Discard')) return;
    dialog.close();
  };
  dialog.addEventListener('cancel', event => { event.preventDefault(); closeButton.click(); });
  /** Everything the panel holds goes when it closes: buffers, editor states, the tree. */
  function forgetAll() { buffers.clear(); current = null; showingImage = false; codeView?.clear(); preview.replaceChildren(); picture.replaceChildren(); visual.replaceChildren(); language.textContent = ''; render(); }
  dialog.addEventListener('close', () => { void cancelPending(); forgetAll(); tree.clear(); search.value = ''; if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  chooser.onchange = async () => {
    const dirty = [...buffers.values()].some(b => b.text !== b.baseText);
    if (dirty && !await options.decide('Discard unsaved changes?', 'Switching project discards unsaved changes in this panel.', 'Discard')) return;
    forgetAll(); search.value = ''; tree.results(null);
    try { await tree.load(); } catch (error) { setStatus(message(error), true); }
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
    try { await tree.load(); } catch (error) { setStatus(message(error), true); }
    search.focus();
  }
  render();
  return { show };
}
