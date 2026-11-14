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

type Node = { entry: Entry; item: HTMLLIElement; row: HTMLDivElement; group: HTMLUListElement | null; loaded: boolean; level: number };
const SVG = 'http://www.w3.org/2000/svg';
function icon(name: string, className = 'icon') {
  const svg = document.createElementNS(SVG, 'svg'); svg.setAttribute('class', className); svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS(SVG, 'use'); use.setAttribute('href', `#${name}`); svg.append(use); return svg;
}
const folderLike = (entry: Entry) => entry.kind === 'directory' || (entry.kind === 'link' && entry.linkInside === true);

export function createFilesTree(options: {
  /** One folder's entries; '' is the project root. */
  list(path: string): Promise<Listing>;
  /** A file was chosen. */
  open(entry: Entry): void;
  /** A folder could not be read. */
  failed(message: string): void;
}) {
  const element = document.createElement('div'); element.className = 'files-tree-host';
  const tree = document.createElement('ul'); tree.className = 'files-tree'; tree.setAttribute('role', 'tree'); tree.setAttribute('aria-label', 'Project folders');
  const results = document.createElement('ul'); results.className = 'files-tree files-results'; results.setAttribute('role', 'listbox'); results.setAttribute('aria-label', 'Matching files'); results.hidden = true;
  element.append(tree, results);
  const nodes = new Map<string, Node>(); let selected: string | null = null; let typed = ''; let typedAt = 0; let generation = 0;

  function row(entry: Entry, level: number, resultParent?: string): { item: HTMLLIElement; row: HTMLDivElement } {
    const item = document.createElement('li'); item.dataset.path = entry.path; item.tabIndex = -1;
    const line = document.createElement('div'); line.className = 'files-row'; line.style.setProperty('--level', String(level));
    line.dataset.kind = entry.kind;
    const twist = document.createElement('span'); twist.className = 'files-twist';
    if (folderLike(entry) && !resultParent) twist.append(icon('i-caret', 'icon files-caret'));
    const glyph = icon(folderLike(entry) ? 'i-folder' : entry.kind === 'link' ? 'i-link' : 'i-file', 'icon files-glyph');
    const label = document.createElement('span'); label.className = 'files-label'; label.textContent = entry.name;
    line.append(twist, glyph, label);
    if (resultParent !== undefined) {
      const where = document.createElement('span'); where.className = 'files-where'; where.textContent = shortFolder(resultParent); where.title = resultParent || '/';
      line.append(where);
    }
    else { const size = document.createElement('span'); size.className = 'files-size'; size.textContent = entry.kind === 'file' ? formatSize(entry.size) : ''; line.append(size); }
    const outside = entry.kind === 'link' && !entry.linkInside;
    item.title = outside ? 'Link outside the project: not opened' : entry.path;
    if (outside) item.setAttribute('aria-disabled', 'true');
    item.append(line); return { item, row: line };
  }

  function fill(group: HTMLUListElement, listing: Listing, level: number) {
    group.replaceChildren();
    for (const entry of listing.entries) {
      const made = row(entry, level); made.item.setAttribute('role', 'treeitem'); made.item.setAttribute('aria-level', String(level + 1));
      made.item.setAttribute('aria-selected', String(entry.path === selected));
      const node: Node = { entry, item: made.item, row: made.row, group: null, loaded: false, level };
      if (folderLike(entry)) { made.item.setAttribute('aria-expanded', 'false'); }
      nodes.set(entry.path, node); group.append(made.item);
    }
    if (!listing.entries.length) group.append(note('Empty folder', level));
    if (listing.truncated) group.append(note('More than 2,000 entries; only the first are shown.', level));
  }
  function note(text: string, level: number) {
    const item = document.createElement('li'); item.className = 'files-note'; item.setAttribute('role', 'none');
    item.style.setProperty('--level', String(level)); item.textContent = text; return item;
  }

  async function expand(node: Node, open = true) {
    if (!folderLike(node.entry)) return;
    if (!open) { node.item.setAttribute('aria-expanded', 'false'); if (node.group) node.group.hidden = true; return; }
    node.item.setAttribute('aria-expanded', 'true');
    if (!node.group) { node.group = document.createElement('ul'); node.group.setAttribute('role', 'group'); node.item.append(node.group); }
    node.group.hidden = false;
    if (node.loaded) return;
    node.item.setAttribute('aria-busy', 'true');
    try {
      const listing = await options.list(node.entry.path);
      fill(node.group, listing, node.level + 1); node.loaded = true;
    } catch (error) {
      // A link that is not a folder opens as a file, as before.
      if (node.entry.kind === 'link') { node.item.setAttribute('aria-expanded', 'false'); node.group.hidden = true; options.open(node.entry); }
      else { node.item.setAttribute('aria-expanded', 'false'); node.group.hidden = true; options.failed(typeof error === 'string' ? error : 'This folder could not be read.'); }
    } finally { node.item.removeAttribute('aria-busy'); }
  }

  const visible = (): HTMLLIElement[] => {
    const list = results.hidden ? tree : results;
    return [...list.querySelectorAll<HTMLLIElement>('li[data-path]')].filter(item => item.offsetParent !== null || item.getClientRects().length > 0);
  };
  function focusItem(item: HTMLLIElement | undefined) {
    if (!item) return;
    for (const other of element.querySelectorAll<HTMLLIElement>('li[tabindex="0"]')) other.tabIndex = -1;
    item.tabIndex = 0; item.focus(); item.scrollIntoView({ block: 'nearest' });
  }
  function choose(item: HTMLLIElement) {
    if (item.getAttribute('aria-disabled') === 'true') return;
    const path = item.dataset.path!; const node = nodes.get(path);
    const entry = node?.entry ?? resultEntries.get(path); if (!entry) return;
    if (!results.hidden || !folderLike(entry)) { select(path); options.open(entry); return; }
    if (node) void expand(node, node.item.getAttribute('aria-expanded') !== 'true');
  }
  function select(path: string | null) {
    selected = path;
    for (const item of element.querySelectorAll<HTMLLIElement>('li[aria-selected]')) item.setAttribute('aria-selected', String(item.dataset.path === path));
  }

  element.addEventListener('click', event => {
    const item = (event.target as Element).closest<HTMLLIElement>('li[data-path]'); if (!item) return;
    focusItem(item); choose(item);
  });
  element.addEventListener('keydown', event => {
    const items = visible(); const current = (document.activeElement as HTMLElement | null)?.closest<HTMLLIElement>('li[data-path]');
    const index = current ? items.indexOf(current) : -1; const node = current ? nodes.get(current.dataset.path!) : undefined;
    const key = event.key;
    if (key === 'ArrowDown') focusItem(items[Math.min(items.length - 1, index + 1)]);
    else if (key === 'ArrowUp') focusItem(items[Math.max(0, index - 1)]);
    else if (key === 'Home') focusItem(items[0]);
    else if (key === 'End') focusItem(items.at(-1));
    else if (key === 'Enter' || key === ' ') { if (current) choose(current); }
    else if (key === 'ArrowRight' && node && results.hidden && folderLike(node.entry)) {
      if (node.item.getAttribute('aria-expanded') !== 'true') void expand(node);
      else focusItem(node.group?.querySelector<HTMLLIElement>('li[data-path]') ?? undefined);
    } else if (key === 'ArrowLeft' && current && results.hidden) {
      if (node && node.item.getAttribute('aria-expanded') === 'true') void expand(node, false);
      else focusItem(current.parentElement?.closest<HTMLLIElement>('li[data-path]') ?? undefined);
    } else if (key.length === 1 && /\S/.test(key) && !event.ctrlKey && !event.altKey && !event.metaKey) {
      const now = Date.now(); typed = now - typedAt > 700 ? key.toLowerCase() : typed + key.toLowerCase(); typedAt = now;
      const after = [...items.slice(index + 1), ...items.slice(0, index + 1)];
      focusItem(after.find(item => (item.querySelector('.files-label')?.textContent ?? '').toLowerCase().startsWith(typed)));
    } else return;
    event.preventDefault();
  });

  const resultEntries = new Map<string, Entry>();
  return {
    element,
    /** Shows a project's root; everything opened before is forgotten. */
    async load(): Promise<void> {
      const mine = ++generation; nodes.clear(); selected = null;
      const listing = await options.list('');
      if (mine !== generation) return;
      fill(tree, listing, 0);
      tree.querySelector<HTMLLIElement>('li[data-path]')?.setAttribute('tabindex', '0');
    },
    /** Search results in place of the tree; null brings the tree back as it was. */
    results(hits: Entry[] | null) {
      resultEntries.clear(); results.replaceChildren();
      results.hidden = hits === null; tree.hidden = hits !== null;
      if (!hits) return;
      for (const hit of hits) {
        resultEntries.set(hit.path, hit);
        const parent = hit.path.split('/').slice(0, -1).join('/');
        const made = row(hit, 0, parent); made.item.setAttribute('role', 'option'); made.item.setAttribute('aria-selected', String(hit.path === selected));
        results.append(made.item);
      }
      results.querySelector<HTMLLIElement>('li[data-path]')?.setAttribute('tabindex', '0');
    },
    /** Opens the folders down to `path` and selects it. */
    async reveal(path: string) {
      results.hidden = true; tree.hidden = false;
      const parts = path.split('/').filter(Boolean);
      for (let depth = 1; depth < parts.length; depth++) {
        const node = nodes.get(parts.slice(0, depth).join('/')); if (!node) break;
        await expand(node);
      }
      select(path); const target = nodes.get(path)?.item;
      if (target) { focusItem(target); if (folderLike(nodes.get(path)!.entry)) void expand(nodes.get(path)!); }
    },
    select,
    focus() { focusItem(element.querySelector<HTMLLIElement>('li[tabindex="0"]') ?? visible()[0]); },
    clear() { generation++; nodes.clear(); tree.replaceChildren(); results.replaceChildren(); resultEntries.clear(); selected = null; },
  };
}
