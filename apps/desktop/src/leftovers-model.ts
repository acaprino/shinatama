/** Xiox's picture of a project (`project_leftovers`), and the words the overview shows for it. */
export type LeftoverItem = {
  key: string; category: string; path: string | null; label: string | null;
  bytes: number | null; bytesPartial: boolean; level: string; blocked: string[]; reason: string; firstSeenMs: number | null;
};
export type Leftovers = {
  folders: string[]; phase: 'starting' | 'watching' | 'ended'; root: string | null; name: string | null; isRepo: boolean;
  newItems: LeftoverItem[]; newItemsOmitted: number; growthBytes: number; attention: string[]; stale: boolean;
  cleanCommand: string | null; cleanCommandTruncated: boolean; startedAtMs: number | null; checkedAtMs: number | null; message: string | null;
};
export type LeftoverStatus = { available: boolean; reason: string | null; projects: Leftovers[] };

/** The same folder however it is spelled: separators, trailing separator, case (Windows). */
export function folderKey(path: string): string {
  const unified = path.replace(/^\\\\\?\\/, '').replace(/\//g, '\\').toLowerCase();
  const trimmed = unified.replace(/\\+$/, '');
  return trimmed && !trimmed.endsWith(':') ? trimmed : unified;
}

/** The observer covering a project row: its folder is the root Xiox resolved, or one it watches. */
export function leftoversFor(path: string, projects: readonly Leftovers[]): Leftovers | null {
  const key = folderKey(path);
  return projects.find(p => (p.root !== null && folderKey(p.root) === key) || p.folders.some(f => folderKey(f) === key)) ?? null;
}

export function size(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes; let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(1)} ${units[unit]}`;
}

const NAMES: Record<string, [string, string]> = {
  worktrees: ['worktree', 'worktrees'], copies: ['copy', 'copies'], named: ['folder named like it', 'folders named like it'],
  scratch: ['session folder', 'session folders'], builds: ['build folder', 'build folders'], logs: ['log', 'logs'],
  sessions: ['archived session', 'archived sessions'],
};

/** "2 worktrees, 1 copy": the new items by category, in the order they appeared. */
export function countText(items: readonly LeftoverItem[]): string {
  const counts = new Map<string, number>();
  for (const item of items) counts.set(item.category, (counts.get(item.category) ?? 0) + 1);
  return [...counts].map(([category, count]) => {
    const [one, many] = NAMES[category] ?? ['item', 'items'];
    return `${count} ${count === 1 ? one : many}`;
  }).join(', ');
}

const clock = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

/** One line for the project row. */
export function summaryText(picture: Leftovers): string {
  if (picture.phase === 'starting') return 'Leftovers: Xiox is taking the first snapshot…';
  if (picture.phase === 'ended') return `Leftovers: Xiox stopped${picture.message ? ` (${picture.message})` : ''}.`;
  const since = picture.startedAtMs ? ` since ${clock(picture.startedAtMs)}` : '';
  const total = picture.newItems.length + picture.newItemsOmitted;
  const stale = picture.stale ? ' · last check failed, older picture' : '';
  if (total === 0) return `Leftovers: nothing new${since}${stale}.`;
  const growth = picture.growthBytes > 0 ? ` · +${size(picture.growthBytes)}` : '';
  const more = picture.newItemsOmitted ? `, ${picture.newItemsOmitted} more` : '';
  return `Leftovers: ${total} new${since}${growth} · ${countText(picture.newItems)}${more}${stale}.`;
}

/** One line per new item. */
export function itemText(item: LeftoverItem): string {
  const where = item.path ?? item.label ?? item.key;
  const bytes = item.bytes === null ? 'size unknown' : `${item.bytesPartial ? 'at least ' : ''}${size(item.bytes)}`;
  const blocked = item.blocked.length ? ` · cannot be removed: ${item.blocked.join(', ')}` : '';
  return `${item.level} · ${where} · ${bytes} · ${item.reason}${blocked}`;
}
