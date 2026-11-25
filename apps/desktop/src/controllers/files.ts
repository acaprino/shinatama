// The files of the project the window works in: browse, search, read, edit and save, in the inspector (a tree and the preview of the
// file). Rust reads a folder at a time and a search can be cancelled by its request id. The inspector's files panel owns the rest.
import { filesApi } from '../api/files.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';

let sequence = 0;
/** An id for a read or a search that can be cancelled. */
export const requestId = (): string => `files-${Date.now().toString(36)}-${++sequence}`;

export class FilesController extends Store {
  /** The file chosen in the tree, relative to the project. */
  selected: string | null = null;

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  /** The project the files are of: the folder new sessions start in. */
  get project(): string | null {
    return this.app.projects.path || null;
  }

  open(): void {
    this.app.ui.openPanel({ kind: 'files' });
  }

  select(path: string | null): void {
    this.selected = path;
    this.changed();
  }

  list(path: string, id = requestId()): Promise<unknown> {
    return this.project ? filesApi.list(this.project, path, id) : Promise.reject('Choose a project first.');
  }

  search(query: string, id = requestId()): Promise<unknown> {
    return this.project ? filesApi.search(this.project, query, id) : Promise.reject('Choose a project first.');
  }

  cancel(id: string): Promise<void> {
    return filesApi.cancel(id);
  }
}
