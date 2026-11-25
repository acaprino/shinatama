// The files panel decides, without the page: how a listing becomes the rows of Sigrid's tree, what a path is (markdown, an image, a page
// that a browser opens), the words of a size and of a search, and when a buffer has unsaved changes. Pure.
import type { TreeNode } from '@acaprino/sigrid/assemblies';

export type Entry = { name: string; path: string; kind: 'file' | 'directory' | 'link'; size: number | null; linkInside: boolean | null };
export type Listing = { path: string; entries: Entry[]; truncated: boolean };
export type SearchResult = { query: string; hits: Entry[]; truncated: boolean; cancelled: boolean; timedOut: boolean };
export type TextFile = { path: string; text: string | null; binary: boolean; truncated: boolean; bytes: number; sha256: string };
export type ImageFile =
  | { state: 'ready'; image: { path: string; width: number; height: number; bytes: number; dataUrl: string } }
  | { state: 'refused'; message: string };
export type SaveOutcome = { state: 'saved'; sha256: string } | { state: 'conflict'; disk_sha256: string } | { state: 'failed'; message: string };

/** A file the panel holds: what it was when read (`base*`), what it is now (`text`), and whether it can be written back. */
export type Buffer = { project: string; path: string; baseText: string; baseSha256: string; text: string; editable: boolean };
export const dirty = (buffer: Buffer): boolean => buffer.text !== buffer.baseText;

/** "12 KB": what a size column needs, at a glance. */
export function formatSize(bytes: number | null): string {
  if (bytes === null) return '';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** A search hit's folder, short enough for its column: the last two folders, "…/" before them. */
export function shortFolder(path: string): string {
  if (!path) return '/';
  const parts = path.split('/');
  return parts.length <= 2 && path.length <= 32 ? path : `…/${parts.slice(-2).join('/')}`;
}

const NOTE = '\0note:';
const outside = (file: Entry) => file.kind === 'link' && file.linkInside !== true;

/** The row of one entry. A folder (or a link that stays inside the project) holds rows that are read when it is first opened; a link that
 *  leaves the project is there but is never opened. In a search the rows are flat and say where the file is. */
export function entryNode(file: Entry, result = false): TreeNode {
  const out = outside(file);
  return {
    id: file.path,
    name: file.name,
    icon: file.kind === 'directory' ? 'folder' : file.kind === 'link' ? 'external' : 'file',
    ...(file.kind === 'directory' ? { openIcon: 'folderOpen' as const } : {}),
    meta: result ? shortFolder(file.path.split('/').slice(0, -1).join('/')) : formatSize(file.kind === 'file' ? file.size : null),
    ...(!result && (file.kind === 'directory' || (file.kind === 'link' && !out)) ? { hasChildren: true } : {}),
    ...(out ? { disabled: true } : {}),
  };
}

/** The rows of a folder's listing, with a last row that says when the folder is empty or cut. */
export function listingNodes(list: Listing): TreeNode[] {
  const nodes = list.entries.map(file => entryNode(file));
  const note = list.truncated ? 'More than 2,000 entries; only the first are shown.' : !nodes.length ? 'Empty folder' : '';
  if (note) nodes.push({ id: NOTE + list.path, name: note, disabled: true });
  return nodes;
}

/** The kind of row a node is: a note is a line of words, not a thing to open. */
export const isNote = (id: string): boolean => id.startsWith(NOTE);

/** The path of a file under a project, as a Windows path (what a browser tab opens). */
export function absolutePath(project: string, path: string): string {
  return [project.replace(/[\\/]+$/, ''), ...path.split('/').filter(Boolean)].join('\\');
}

export const isMarkdown = (path: string): boolean => /\.(md|markdown)$/i.test(path);
export const isImage = (path: string): boolean => /\.(png|jpe?g|gif|webp|docx?)$/i.test(path);
export const isPage = (path: string): boolean => /\.html?$/i.test(path);

/** The sentence a finished search says. */
export function searchText(result: SearchResult): string {
  return result.cancelled ? 'Search cancelled.'
    : result.timedOut ? 'Search stopped after 5 seconds; results are partial.'
    : result.truncated ? 'Too many matches; refine the name.'
    : `${result.hits.length} match${result.hits.length === 1 ? '' : 'es'} for “${result.query}”.`;
}

/** What the status line says about the file that is open. */
export function fileStatus(buffer: Buffer, truncated: boolean): string {
  if (truncated) return 'This file is larger than 4 MiB: shown read-only, first part only.';
  const lines = buffer.text.split('\n').length;
  return `${lines.toLocaleString()} line${lines === 1 ? '' : 's'} · ${buffer.editable ? 'edit and press Save to write it' : 'read-only'}`;
}

/** The ancestors of a path, from the first folder to the one that holds it: the branches to open to reveal it. */
export function ancestorsOf(path: string): string[] {
  const parts = path.split('/').filter(Boolean);
  return parts.slice(0, -1).map((_, index) => parts.slice(0, index + 1).join('/'));
}

// --- a file in a pane ------------------------------------------------------------------------------------------------------------------

/** The view of a file open in a pane: its id may hold no path (a view id is letters, digits and a few signs), so it is a hash of the file. */
export function fileViewId(project: string, path: string): string {
  let hash = 0x811c9dc5;
  for (const unit of `${project.toLowerCase()}\0${path}`) hash = Math.imul(hash ^ unit.codePointAt(0)!, 0x01000193) >>> 0;
  return `file-${hash.toString(16).padStart(8, '0')}`;
}

/** Where a file open in a pane lives: kept so that a tab saved in the layout can read its file again after a restart. */
export type SavedFile = { project: string; path: string };

/** The files that were open in panes as storage kept them: anything that is not a file is left out. */
export function parseSavedFiles(text: string | null): Record<string, SavedFile> {
  try {
    const value: unknown = JSON.parse(text ?? '{}');
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
    const saved: Record<string, SavedFile> = {};
    for (const [view, file] of Object.entries(value)) {
      const { project, path } = (file ?? {}) as Record<string, unknown>;
      if (typeof project === 'string' && typeof path === 'string') saved[view] = { project, path };
    }
    return saved;
  } catch { return {}; }
}

/** The name of a tab: the file's own name, a dot after it while it has changes that are not saved. */
export function fileTabLabel(path: string, unsaved: boolean): string {
  return `${path.split('/').filter(Boolean).at(-1) ?? path}${unsaved ? ' •' : ''}`;
}
