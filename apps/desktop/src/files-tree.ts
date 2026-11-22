import { createFileTree, uiPart, type FileTreeEntry } from '@acaprino/imago/ui';

/** The Files panel's folder tree: an ARIA tree whose folders load their entries when first opened.
 * Keyboard as in Explorer: arrows move, Right opens or enters a folder, Left closes or goes to the
 * parent, Enter opens a file, Home/End jump, typing jumps to the next name with those letters.
 * A search replaces the tree with a flat list of matches until the query is cleared. */
export type Entry = { name: string; path: string; kind: 'file' | 'directory' | 'link'; size: number | null; linkInside: boolean | null };
export type Listing = { path: string; entries: Entry[]; truncated: boolean };

/** "12 KB": what a size column needs, at a glance. */
export function formatSize(bytes: number | null): string {
  if (bytes === null) return '';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB']; let value = bytes / 1024; let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** A search hit's folder, short enough for its column: the last two folders, "…/" before them. */
export function shortFolder(path: string): string {
  if (!path) return '/';
  const parts = path.split('/');
  return parts.length <= 2 && path.length <= 32 ? path : `…/${parts.slice(-2).join('/')}`;
}


type ProjectRow = FileTreeEntry & { file?: Entry };

export function createFilesTree(options: {
  list(path: string): Promise<Listing>;
  open(entry: Entry): void;
  failed(message: string): void;
}) {
  const element = uiPart('scroll');
  element.classList.add('files-tree-host');
  let generation = 0;
  let selected: string | undefined;
  let tree: ReturnType<typeof createFileTree> | undefined;
  let results: ReturnType<typeof createFileTree> | undefined;

  function row(file: Entry, result = false): ProjectRow {
    const outside = file.kind === 'link' && file.linkInside !== true;
    return {
      id: file.path, name: file.name, file,
      icon: file.kind === 'directory' ? 'folder' : file.kind === 'link' ? 'external' : 'file',
      detail: result ? shortFolder(file.path.split('/').slice(0, -1).join('/')) : formatSize(file.kind === 'file' ? file.size : null),
      title: outside ? 'Link outside the project: not opened' : file.path,
      lazy: !result && (file.kind === 'directory' || (file.kind === 'link' && !outside)),
      disabled: outside,
    };
  }

  function entries(list: Listing): ProjectRow[] {
    const items = list.entries.map(file => row(file));
    const note = list.truncated ? 'More than 2,000 entries; only the first are shown.' : !items.length ? 'Empty folder' : '';
    if (note) items.push({ id: '\0note:' + list.path, name: note, disabled: true });
    return items;
  }

  function mount(items: ProjectRow[], label: string) {
    const control = createFileTree(document, items, {
      label, singleClick: true, selected,
      loadChildren: async item => entries(await options.list(item.id)),
      onLoadError(item, error) {
        const file = (item as ProjectRow).file;
        if (file?.kind === 'link' && file.linkInside) options.open(file);
        else options.failed(typeof error === 'string' ? error : 'This folder could not be read.');
      },
      onOpen(item) {
        const file = (item as ProjectRow).file;
        if (file && !item.disabled) options.open(file);
      },
    });
    element.append(control.element);
    return control;
  }

  return {
    element,
    async load() {
      const mine = ++generation;
      tree?.destroy(); tree = undefined;
      selected = undefined;
      const list = await options.list('');
      if (mine !== generation) return;
      tree = mount(entries(list), 'Project folders');
      tree.element.hidden = !!results;
    },
    results(hits: Entry[] | null) {
      results?.destroy();
      results = hits === null ? undefined : mount(hits.map(file => row(file, true)), 'Matching files');
      if (tree) tree.element.hidden = !!results;
    },
    async reveal(path: string) {
      results?.destroy(); results = undefined;
      if (!tree) return;
      tree.element.hidden = false;
      const parts = path.split('/').filter(Boolean);
      await tree.reveal(parts.map((_, i) => parts.slice(0, i + 1).join('/')), true, true);
    },
    select(path: string | null) {
      selected = path ?? undefined;
      tree?.setSelected(selected);
      results?.setSelected(selected);
    },
    focus() { (results ?? tree)?.element.querySelector<HTMLElement>('[tabindex="0"]')?.focus(); },
    clear() {
      generation++; tree?.destroy(); results?.destroy();
      tree = results = undefined; selected = undefined;
    },
  };
}
