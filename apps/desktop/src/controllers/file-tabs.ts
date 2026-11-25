// The files that are open in a pane ("Open in a pane" of the Files panel): each is a tab of the workspace beside the terminals, kind `files`
// (view id `file-<hash>`, see files-model.ts). A tab holds the buffer of its file, which it owns from the moment it is opened: the text,
// what it was when read, the version it was read at. It writes the same way the panel does (`writeBuffer`). The layout keeps the tab; this
// keeps where its file is (a convenience in the page's storage), so that a tab that was saved reads its file again after a restart.
import { errorText } from '../api/core.ts';
import { filesApi } from '../api/files.ts';
import type { Shinatama } from '../app.ts';
import { dirty, fileTabLabel, fileViewId, parseSavedFiles, type Buffer, type SavedFile, type TextFile } from '../files-model.ts';
import { Store } from '../store.ts';
import { writeBuffer } from './files.ts';

const SAVED_KEY = 'shinatama.files.tabs';

/** What a tab holds: its file read (or being read, or not readable), and what the last save said. */
export interface FileTab {
  readonly project: string;
  readonly path: string;
  buffer: Buffer | null;
  state: 'loading' | 'ready' | 'failed';
  /** What was last said about the file: why it could not be read, how a save went. */
  line: string;
  failed: boolean;
  saving: boolean;
}

export class FileTabsController extends Store {
  readonly tabs = new Map<string, FileTab>();
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  /** The tab's name: the file's, with a dot while it has changes that are not saved. */
  label(viewId: string): string {
    const tab = this.tabs.get(viewId);
    const known = tab ?? this.saved()[viewId];
    return known ? fileTabLabel(known.path, !!tab?.buffer && dirty(tab.buffer)) : 'File';
  }

  /** Opens a file in a pane, from the buffer the panel had (the one who opened it hands it over). The same file is one tab. */
  open(buffer: Buffer): string {
    const id = fileViewId(buffer.project, buffer.path);
    if (!this.tabs.has(id)) this.tabs.set(id, { project: buffer.project, path: buffer.path, buffer, state: 'ready', line: '', failed: false, saving: false });
    this.remember(id, { project: buffer.project, path: buffer.path });
    this.app.layout.open({ id, resource: { kind: 'files', resourceId: id }, hidden: false }, other => other.resource.kind === 'files');
    this.changed();
    return id;
  }

  /** A tab the layout kept, drawn: reads its file when this window does not hold it yet (after a restart). */
  async ensure(viewId: string): Promise<void> {
    if (this.tabs.has(viewId)) return;
    const saved = this.saved()[viewId];
    if (!saved) return;
    const tab: FileTab = { ...saved, buffer: null, state: 'loading', line: '', failed: false, saving: false };
    this.tabs.set(viewId, tab);
    this.changed();
    try {
      const file = await filesApi.read(saved.project, saved.path) as TextFile;
      if (file.binary || file.text === null) { tab.state = 'failed'; tab.line = `${file.path} is not text; it is not opened or run.`; tab.failed = true; }
      else {
        tab.buffer = { project: saved.project, path: file.path, baseText: file.text, baseSha256: file.sha256, text: file.text, editable: !file.truncated };
        tab.state = 'ready';
        tab.line = file.truncated ? 'This file is larger than 4 MiB: shown read-only, first part only.' : '';
      }
    } catch (error) {
      tab.state = 'failed'; tab.failed = true; tab.line = errorText(error) || 'The file could not be read.';
    }
    this.changed();
  }

  /** The person typed: the buffer holds the text; nothing is written until Save. */
  edit(viewId: string, text: string): void {
    const buffer = this.tabs.get(viewId)?.buffer;
    if (!buffer?.editable || buffer.text === text) return;
    buffer.text = text;
    this.changed();
  }

  /** Reads a tab's file again, after asking when it has changes that are not saved. */
  async reload(viewId: string): Promise<void> {
    const tab = this.tabs.get(viewId);
    if (!tab) return;
    if (tab.buffer && dirty(tab.buffer) && !await this.app.ui.ask({
      title: 'Discard unsaved changes?', description: `${tab.path}: reading it again discards them.`, confirm: 'Discard changes',
    })) return;
    this.tabs.delete(viewId);
    this.changed();
    await this.ensure(viewId);
  }

  async save(viewId: string): Promise<void> {
    const tab = this.tabs.get(viewId);
    const buffer = tab?.buffer;
    if (!tab || !buffer?.editable || tab.saving) return;
    tab.saving = true; this.changed();
    const report = (text: string, error = false) => { tab.line = text; tab.failed = error; };
    try {
      await writeBuffer(this.app, buffer, report, () => this.tabs.get(viewId) !== tab);
    } catch (error) {
      report(errorText(error) || 'The file operation failed.', true);
    } finally {
      tab.saving = false;
      this.changed();
    }
  }

  /** The close of the tab: with changes that are not saved the person decides first. False when they keep the tab. */
  async close(viewId: string): Promise<boolean> {
    const buffer = this.tabs.get(viewId)?.buffer;
    if (buffer && dirty(buffer) && !await this.app.ui.ask({
      title: 'Discard unsaved changes?', description: `${buffer.path}: closing the tab discards them.`, confirm: `Discard changes to ${buffer.path}`,
    })) return false;
    this.tabs.delete(viewId);
    this.unremember(viewId);
    this.app.layout.close(viewId);
    this.changed();
    return true;
  }

  // The files that were open, kept so that a tab saved in the layout can read its file again. A convenience only: a storage that cannot be
  // read or written keeps nothing and the tab says its file is not known.
  private saved(): Record<string, SavedFile> {
    try { return parseSavedFiles(localStorage.getItem(SAVED_KEY)); } catch { return {}; }
  }

  private remember(viewId: string, file: SavedFile): void {
    try { localStorage.setItem(SAVED_KEY, JSON.stringify({ ...this.saved(), [viewId]: file })); } catch { /* a convenience only */ }
  }

  private unremember(viewId: string): void {
    const saved = this.saved();
    delete saved[viewId];
    try { localStorage.setItem(SAVED_KEY, JSON.stringify(saved)); } catch { /* a convenience only */ }
  }
}
