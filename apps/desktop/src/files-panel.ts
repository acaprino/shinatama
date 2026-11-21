import { breadcrumbs, componentElement, fileWorkspace, inputHtml, segmentedControl, selectHtml, selectOptionsHtml, uiButton, uiInput, uiTextarea, uiPart } from '@acaprino/imago/ui';
import { openModal } from './modal.ts';
import { createFilesTree, type Entry, type Listing } from './files-tree.ts';
import type { CodeView } from './code-view.ts';
import './files-panel.css';

type Search = { query: string; hits: Entry[]; truncated: boolean; cancelled: boolean; timedOut: boolean };
type TextFile = { path: string; text: string | null; binary: boolean; truncated: boolean; bytes: number; sha256: string };
type SaveOutcome = { state: 'saved'; sha256: string } | { state: 'conflict'; disk_sha256: string } | { state: 'failed'; message: string };
type VisualBlock = { kind: 'heading' | 'paragraph' | 'code' | 'blank' | 'preserved'; prefix: string; text: string; suffix: string; level: number; editable: boolean };
type VisualDocument = { blocks: VisualBlock[]; exact: boolean; editableBlocks: number; preservedBlocks: number };
type Buffer = { visual?: { flush(): Promise<boolean> }; project: string; path: string; baseText: string; baseSha256: string; text: string; editable: boolean };
type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
type Options = { invoke: Invoke; project: () => string | null; reportError: (message: string) => void;
  decide: (title: string, message: string, accept: string) => Promise<boolean>; restoreFocus?: () => void;
  /** Opens a local HTML document in a browser tab (ADR-0056); absent outside the desktop. */
  openInBrowser?: (path: string) => void };

const id = () => crypto.randomUUID();
const message = (error: unknown) => typeof error === 'string' ? error : error instanceof Error ? error.message : 'The file operation failed.';
const MODES = [['source', 'Source'], ['split', 'Split'], ['preview', 'Preview'], ['visual', 'Visual']] as const;

/** Read-only browsing plus explicit, version-checked saves. Opening a file never runs it. */
export function createFilesPanel(options: Options) {
  const dialog = uiPart('dialog'); dialog.id = 'files-dialog'; dialog.classList.add('im-file-dialog'); dialog.setAttribute('aria-labelledby', 'files-title');
  const heading = uiPart('header');
  const title = uiPart('title', 'Project files'); title.id = 'files-title';
  const closeButton = uiButton('×', { attrs: { 'aria-label': 'Close files' } });
  const chooser = componentElement<HTMLSelectElement>(selectHtml({ name: 'files-project', label: 'Project', options: [], value: '', attrs: { class: 'im-select files-project' } }));
  heading.append(title, chooser, closeButton);
  const SIDE_KEY = 'shinatama.files.side';
  let initialWidth = 300;
  try { initialWidth = Number(localStorage.getItem(SIDE_KEY)) || initialWidth; } catch { /* optional preference */ }
  const workspace = fileWorkspace({
    resizeLabel: 'Resize the folder tree', toolbarLabel: 'File actions', initialWidth,
    onWidthCommit(width) { try { localStorage.setItem(SIDE_KEY, String(width)); } catch { /* optional preference */ } },
  });
  const { navigation: side, toolbar: bar, location: crumbs, panes, status } = workspace;
  const search = componentElement<HTMLInputElement>(inputHtml({ name: 'file-search', label: 'Find a file by name', value: '',
    attrs: { type: 'search', class: 'im-input files-search', placeholder: 'Find a file by name' } }));
  const tree = createFilesTree({
    list: async path => {
      const project = projectPath(); if (!project) throw 'Choose a project first.';
      return options.invoke<Listing>('files_list', { project, path, requestId: id() });
    },
    open: entry => requestOpen(entry.path),
    failed: text => setStatus(text, true),
  });
  side.append(search, tree.element);
  const language = uiPart('hint'); language.classList.add('files-language');
  const modeChoice = segmentedControl(document, {
    label: 'View', choices: MODES.map(([value, label]) => ({ value, label })), value: 'source',
    async onChange(value) {
      const buffer = current;
      if (buffer?.visual) {
        const ready = await buffer.visual.flush();
        if (current !== buffer) return;
        if (!ready) { render(); return; }
      }
      view = value as typeof view;
      render();
      if (view === 'visual') void renderVisual(); else schedulePreview();
    },
  });
  const modes = modeChoice.element;
  const save = uiButton('Save', { primary: true });
  const inBrowser = uiButton('Browser'); inBrowser.hidden = true;
  inBrowser.title = 'Open this page in a browser tab, with its own folder only';
  inBrowser.onclick = () => {
    const project = projectPath(); if (!current || !project) return;
    options.openInBrowser?.([project.replace(/[\\/]+$/, ''), ...current.path.split('/').filter(Boolean)].join('\\'));
    dialog.close();
  };
  bar.append(language, modes, inBrowser, save);
  const code = document.createElement('div'); code.className = 'files-code'; code.setAttribute('aria-label', 'File source');
  const preview = document.createElement('div'); preview.className = 'files-preview'; preview.setAttribute('aria-label', 'Rendered Markdown');
  const picture = document.createElement('div'); picture.className = 'files-picture'; picture.hidden = true;
  const visual = document.createElement('div'); visual.className = 'files-visual'; visual.hidden = true; visual.setAttribute('aria-label', 'Visual editor');
  const empty = uiPart('hint', 'Choose a file in the tree, or find one by name.', 'im-empty-message');
  panes.append(code, preview, picture, visual, empty);
  status.classList.add('files-status');
  dialog.append(heading, workspace.element);
  (document.querySelector('#app') ?? document.body).append(dialog);

  const buffers = new Map<string, Buffer>();
  const saving = new WeakSet<Buffer>();
  let activeProject: string | null = null;
  let opening = 0;
  const dirty = (buffer: Buffer) => !!buffer.visual || buffer.text !== buffer.baseText;
  let current: Buffer | null = null; let showingImage = false; let view: 'source' | 'preview' | 'split' | 'visual' = 'source';
  let visualAvailable = false; let visualGeneration = 0;
  let pending: string | null = null; let renderGeneration = 0; let renderTimer = 0;
  let codeView: CodeView | null = null; let codeLoading: Promise<CodeView> | null = null;
  const markdown = (path: string) => /\.(md|markdown)$/i.test(path);
  const setStatus = (text: string, error = false) => { status.textContent = text; status.classList.toggle('im-text-err', error); };
  const projectPath = () => activeProject;
  /** CodeMirror loads with the first file opened, not with the app. */
  function editor(): Promise<CodeView> {
    if (codeView) return Promise.resolve(codeView);
    codeLoading ??= import('./code-view.ts').then(module => {
      codeView = module.createCodeView(code, (path, text) => {
        const buffer = buffers.get(path); if (!buffer?.editable || buffer.text === text) return;
        delete buffer.visual;
        buffer.text = text; if (buffer === current) { render(); schedulePreview(); }
      });
      return codeView;
    });
    return codeLoading;
  }

  function paintCrumbs() {
    const path = current?.path ?? (showingImage ? crumbs.dataset.path : '') ?? '';
    const unsaved = !!current && dirty(current);
    if (crumbs.dataset.rendered === path && crumbs.dataset.dirty === String(unsaved)) return;
    crumbs.dataset.rendered = path;
    crumbs.dataset.dirty = String(unsaved);
    const parts = path.split('/').filter(Boolean);
    const trail = breadcrumbs(document, 'Open file path',
      parts.map((label, i) => ({ label, id: parts.slice(0, i + 1).join('/') })),
      id => { resetSearch(); void tree.reveal(id); });
    crumbs.replaceChildren(parts.length ? trail : uiPart('hint', 'No file open'));
    if (unsaved) crumbs.append(uiPart('hint', 'Unsaved changes'));
  }
  function render() {
    const md = !!current && markdown(current.path);
    modes.hidden = !md; if (!md) view = 'source';
    if (view === 'visual' && !visualAvailable) view = 'source';
    for (const [mode, button] of modeChoice.inputs) {
      button.checked = mode === view;
      if (mode === 'visual') { button.disabled = !visualAvailable; button.title = visualAvailable ? 'Edit headings, paragraphs and code; other Markdown is kept as source' : 'Not available: this document does not round-trip exactly'; }
    }
    code.hidden = !current || view === 'preview' || view === 'visual'; preview.hidden = !current || view === 'source' || view === 'visual'; visual.hidden = !current || view !== 'visual';
    empty.hidden = !!current || showingImage;
    panes.dataset.view = view;
    inBrowser.hidden = !current || !options.openInBrowser || !/\.html?$/i.test(current.path);
    save.hidden = !current; save.disabled = !current?.editable || !dirty(current) || saving.has(current);
    paintCrumbs();
  }
  function schedulePreview() {
    if (!current || !markdown(current.path) || view === 'source') return;
    clearTimeout(renderTimer);
    const at = ++renderGeneration; const buffer = current; const text = buffer.text;
    renderTimer = window.setTimeout(async () => {
      try {
        const result = await options.invoke<{ html: string; truncated: boolean }>('files_markdown', { text });
        if (at !== renderGeneration || current !== buffer) return; // a newer render won; discard this one
        preview.innerHTML = result.html; // produced by the Rust renderer: escaped, fixed tags, no URLs
        if (result.truncated) setStatus('The preview shows the first 2 MiB only.');
      } catch (error) { if (at === renderGeneration && current === buffer) setStatus(message(error), true); }
    }, 150);
  }
  async function cancelPending() { if (pending) { const old = pending; pending = null; await options.invoke('files_cancel', { requestId: old }).catch(() => {}); } }
  async function openImage(project: string, path: string, ticket: number) {
    const outcome = await options.invoke<{ state: 'ready'; image: { path: string; width: number; height: number; bytes: number; dataUrl: string } } | { state: 'refused'; message: string }>('files_image', { project, path });
    if (ticket !== opening) return;
    current = null; showingImage = true; crumbs.dataset.path = path; language.textContent = 'Image'; preview.replaceChildren(); render();
    picture.replaceChildren(); picture.hidden = false;
    if (outcome.state === 'refused') { setStatus(outcome.message, true); return; }
    const img = document.createElement('img'); img.alt = outcome.image.path; img.src = outcome.image.dataUrl; picture.append(img);
    setStatus(`${outcome.image.width} × ${outcome.image.height} pixels, ${outcome.image.bytes.toLocaleString()} bytes. Read-only.`);
  }
  function requestOpen(path: string) {
    const read = openFile(path);
    const ticket = opening;
    void read.catch(error => { if (ticket === opening) setStatus(message(error), true); });
  }
  async function openFile(path: string) {
    const project = projectPath(); if (!project) throw new Error('Choose a project first.');
    const ticket = ++opening;
    ++visualGeneration;
    picture.hidden = true; picture.replaceChildren(); showingImage = false;
    if (/\.(png|jpe?g|gif|webp|docx?)$/i.test(path)) { await openImage(project, path, ticket); return; }
    let buffer = buffers.get(path); let note = '';
    if (!buffer) {
      const file = await options.invoke<TextFile>('files_read', { project, path });
      if (ticket !== opening) return;
      if (file.binary || file.text === null) { setStatus(`${file.path} is not text (${file.bytes.toLocaleString()} bytes); it is not opened or run.`); return; }
      buffer = { project, path: file.path, baseText: file.text, baseSha256: file.sha256, text: file.text, editable: !file.truncated };
      buffers.set(file.path, buffer);
      if (file.truncated) note = 'This file is larger than 4 MiB: shown read-only, first part only.';
    }
    const view_ = await editor();
    if (ticket !== opening) return;
    if (buffer.visual) await buffer.visual.flush();
    if (ticket !== opening) return;
    await view_.open(buffer.path, buffer.text, buffer.editable);
    const { languageOf } = await import('./code-view.ts');
    if (ticket !== opening) return;
    current = buffer; preview.replaceChildren();
    language.textContent = languageOf(buffer.path);
    tree.select(buffer.path);
    visualAvailable = false;
    if (markdown(buffer.path) && buffer.editable) {
      const doc = await options.invoke<VisualDocument>('files_visual_parse', { text: buffer.text }).catch(() => null);
      if (ticket !== opening) return;
      visualAvailable = !!doc && doc.exact && doc.editableBlocks > 0;
    }
    if (markdown(buffer.path) && view === 'source') view = 'split';
    const lines = buffer.text.split('\n').length;
    setStatus(note || `${lines.toLocaleString()} line${lines === 1 ? '' : 's'} · ${buffer.editable ? 'edit and press Save to write it' : 'read-only'}`);
    render(); schedulePreview();
    if (view === 'visual') void renderVisual();
  }
  async function renderVisual() {
    if (!current || view !== 'visual') return;
    const buffer = current;
    const at = ++visualGeneration;
    if (buffer.visual && !await buffer.visual.flush()) return;
    if (buffer !== current || view !== 'visual' || at !== visualGeneration) return;
    const source = buffer.text;
    const doc = await options.invoke<VisualDocument>('files_visual_parse', { text: source }).catch(() => null);
    if (at !== visualGeneration || buffer !== current || buffer.text !== source || view !== 'visual') return;
    if (!doc || !doc.exact) { visualAvailable = false; view = 'source'; render(); setStatus('Visual mode is not available for this document; use Source.', true); return; }
    visual.replaceChildren();
    // Block indices refer to this exact parse. Send all changes to that source together.
    const edits = new Map<number, { index: number; text: string }>();
    const fields = new Map<number, HTMLInputElement | HTMLTextAreaElement>();
    let revision = 0;
    let timer = 0;
    let applied = source;
    let pendingApply: Promise<boolean> | undefined;
    const draft = { flush };
    function flush(): Promise<boolean> {
      clearTimeout(timer);
      pendingApply ??= applyEdits().finally(() => { pendingApply = undefined; });
      return pendingApply;
    }
    async function applyEdits(): Promise<boolean> {
      while (buffer.visual === draft && buffers.get(buffer.path) === buffer && buffer.text === applied) {
        const version = revision;
        try {
          const text = await options.invoke<string>('files_visual_apply', { text: source, edits: [...edits.values()] });
          if (version !== revision) continue;
          if (buffers.get(buffer.path) !== buffer || buffer.visual !== draft || buffer.text !== applied) return false;
          applied = buffer.text = text;
          delete buffer.visual;
          for (const field of fields.values()) field.removeAttribute('aria-invalid');
          if (buffer === current) {
            codeView?.replace(buffer.path, text);
            setStatus('Edited visually; Save writes the file.'); render(); schedulePreview();
          }
          return true;
        } catch (error) {
          if (version !== revision) continue;
          if (buffer === current && buffer.visual === draft) {
            for (const index of edits.keys()) fields.get(index)?.setAttribute('aria-invalid', 'true');
            setStatus(message(error), true);
          }
          return false;
        }
      }
      return false;
    }
    doc.blocks.forEach((block, index) => {
      if (block.kind === 'blank') return;
      if (!block.editable) {
        const kept = uiPart('pre'); kept.classList.add('files-visual-kept'); kept.textContent = block.prefix + block.text + block.suffix;
        kept.title = 'Kept as source: not part of the visual subset'; visual.append(kept); return;
      }
      const field = block.kind === 'heading' ? uiInput() : uiTextarea();
      field.classList.add(`files-visual-${block.kind}`);
      if (block.kind === 'heading') field.classList.add(`level-${block.level}`);
      field.value = block.kind === 'code' ? block.text.replace(/\r?\n$/, '') : block.text;
      field.setAttribute('aria-label', `${block.kind} ${index}`);
      if (field instanceof HTMLTextAreaElement) field.rows = Math.min(12, Math.max(2, field.value.split('\n').length));
      fields.set(index, field);
      field.addEventListener('input', () => {
        edits.set(index, { index, text: field.value });
        revision++;
        buffer.visual = draft;
        render();
        clearTimeout(timer);
        timer = window.setTimeout(() => { void flush(); }, 300);
      });
      visual.append(field);
    });
  }
  preview.addEventListener('click', event => {
    const link = (event.target as HTMLElement).closest('a[data-path]');
    if (!link) return; event.preventDefault();
    const base = current?.path.split('/').slice(0, -1).join('/') ?? '';
    const target = link.getAttribute('data-path') ?? '';
    requestOpen(base ? `${base}/${target}` : target);
  });
  async function saveCurrent() {
    const buffer = current;
    if (!buffer?.editable || saving.has(buffer)) return;
    saving.add(buffer); render();
    const report = (text: string, error = false) => { if (current === buffer) setStatus(text, error); };
    try {
      if (buffer.visual && !await buffer.visual.flush()) return;
      if (buffers.get(buffer.path) !== buffer || buffer.text === buffer.baseText) return;
      const request = { project: buffer.project, path: buffer.path, text: buffer.text, baseSha256: buffer.baseSha256, overwrite: false };
      let outcome = await options.invoke<SaveOutcome>('files_save', { request });
      if (outcome.state === 'conflict') {
        if (buffers.get(buffer.path) !== buffer) return;
        const replace = await options.decide('The file changed on disk', `${buffer.path} was changed by someone else (an agent?) after you opened it. Overwrite it with your version? Cancel keeps your text here unsaved.`, 'Overwrite');
        if (!replace) { report('Not saved: the file on disk is newer. Your text is still here.', true); return; }
        if (buffers.get(buffer.path) !== buffer) return;
        if (buffer.text !== request.text || buffer.visual) { report('The draft changed during confirmation. Save again to write it.'); return; }
        // Authorize replacement of the reviewed disk version only.
        outcome = await options.invoke<SaveOutcome>('files_save', { request: { ...request, baseSha256: outcome.disk_sha256 } });
      }
      if (outcome.state === 'saved') {
        buffer.baseSha256 = outcome.sha256;
        buffer.baseText = request.text;
        report(dirty(buffer) ? 'Saved. Newer edits are still unsaved.' : 'Saved.');
      } else report(outcome.state === 'failed' ? outcome.message : 'The file changed again. Save to review it.', true);
    } catch (error) { report(message(error), true); }
    finally { saving.delete(buffer); if (current === buffer) render(); }
  }
  save.onclick = () => { void saveCurrent(); };
  // Ctrl+S saves while the panel has the keyboard.
  dialog.addEventListener('keydown', event => {
    if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 's') { event.preventDefault(); if (!save.disabled && !save.hidden) save.click(); }
  });
  let searchTimer = 0;
  let searchRevision = 0;
  function resetSearch() {
    ++searchRevision;
    clearTimeout(searchTimer);
    void cancelPending();
    search.value = '';
    tree.results(null);
  }
  search.addEventListener('input', () => {
    const revision = ++searchRevision;
    clearTimeout(searchTimer);
    searchTimer = window.setTimeout(async () => {
      const query = search.value.trim(); const project = projectPath();
      if (!query) { await cancelPending(); if (revision !== searchRevision) return; tree.results(null); setStatus(current ? status.textContent ?? '' : 'Files open for reading; nothing is run.'); return; }
      if (!project) return;
      await cancelPending(); if (revision !== searchRevision) return;
      const request = id(); pending = request;
      setStatus('Searching…');
      try {
        const result = await options.invoke<Search>('files_search', { project, query, requestId: request });
        if (pending !== request || revision !== searchRevision) return; pending = null;
        tree.results(result.hits);
        setStatus(result.cancelled ? 'Search cancelled.' : result.timedOut ? 'Search stopped after 5 seconds; results are partial.'
          : result.truncated ? 'Too many matches; refine the name.' : `${result.hits.length} match${result.hits.length === 1 ? '' : 'es'} for “${result.query}”.`);
      } catch (error) { if (pending === request && revision === searchRevision) { pending = null; setStatus(message(error), true); } }
    }, 250);
  });
  search.addEventListener('keydown', event => {
    if (event.key === 'ArrowDown') { event.preventDefault(); tree.focus(); }
    else if (event.key === 'Escape' && search.value) { event.preventDefault(); event.stopPropagation(); search.value = ''; search.dispatchEvent(new Event('input')); }
  });
  closeButton.onclick = async () => {
    const unsaved = [...buffers.values()].filter(dirty);
    if (unsaved.length && !await options.decide('Discard unsaved changes?', `${unsaved.map(b => b.path).join(', ')} ${unsaved.length === 1 ? 'has' : 'have'} unsaved changes. Closing discards them; Cancel keeps the files open.`, 'Discard')) return;
    dialog.close();
  };
  dialog.addEventListener('cancel', event => { event.preventDefault(); closeButton.click(); });
  /** Everything the panel holds goes when it closes: buffers, editor states, the tree. */
  function forgetAll() { opening++; visualGeneration++; renderGeneration++; buffers.clear(); current = null; showingImage = false; codeView?.clear(); preview.replaceChildren(); picture.replaceChildren(); visual.replaceChildren(); language.textContent = ''; render(); }
  dialog.addEventListener('close', () => { resetSearch(); forgetAll(); tree.clear(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  chooser.onchange = async () => {
    const requested = chooser.value;
    chooser.value = activeProject ?? '';
    chooser.disabled = true;
    try {
      if ([...buffers.values()].some(dirty) && !await options.decide('Discard unsaved changes?', 'Switching project discards unsaved changes in this panel.', 'Discard')) return;
    } finally { chooser.disabled = false; }
    if (!dialog.open) return;
    activeProject = requested;
    chooser.value = requested;
    resetSearch(); forgetAll();
    try { await tree.load(); } catch (error) { setStatus(message(error), true); }
  };
  async function show() {
    if (dialog.open) { search.focus(); return; }
    const projects = await options.invoke<{ path: string; name: string; available: boolean }[]>('project_recent').catch(() => []);
    const usable = projects.filter(p => p.available);
    if (!usable.length) { options.reportError('Open a project first; only your projects can be browsed.'); return; }
    const preferred = options.project()?.replace(/[\\/]+$/, '').toLowerCase();
    chooser.innerHTML = selectOptionsHtml(usable.map(p => [p.path, p.name || p.path]), chooser.value);
    chooser.value = usable.find(p => p.path.replace(/[\\/]+$/, '').toLowerCase() === preferred)?.path ?? usable[0].path;
    activeProject = chooser.value;
    if (!dialog.open) openModal(dialog);
    render(); setStatus('Files open for reading; nothing is run.');
    try { await tree.load(); } catch (error) { setStatus(message(error), true); }
    search.focus();
  }
  render();
  return { show };
}
