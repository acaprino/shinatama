/** Xiox's picture of a project (`project_leftovers`), and the words the overview shows for it. */
export type LeftoverItem = {
  key: string; category: string; path: string | null; label: string | null;
  bytes: number | null; bytesPartial: boolean; level: string; blocked: string[]; reason: string; firstSeenMs: number | null;
  /** Sessions whose history shows they made or wrote it (Xiox's evidence). */
  sessions: { harness: string; id: string; strength: string | null }[];
};
/** An agent terminal that was open in the project when an item appeared. */
export type Suspect = { terminalId: string; harness: string; open: boolean };
export type Leftovers = {
  folders: string[]; liveSessions: number; suspects: Record<string, Suspect[]>; phase: 'starting' | 'watching' | 'ended'; root: string | null; name: string | null; isRepo: boolean;
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

const HARNESSES: Record<string, string> = { claude_code: 'Claude Code', claude: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi' };
export const harnessName = (harness: string) => HARNESSES[harness] ?? harness;
export const terminalLabel = (suspect: Suspect) => `${harnessName(suspect.harness)} #${suspect.terminalId.slice(0, 6)}`;

/** What the panel lists: everything new but archived sessions, which every session leaves. */
export function visibleItems(picture: Leftovers): LeftoverItem[] {
  return picture.newItems.filter(item => item.category !== 'sessions')
    .sort((a, b) => (b.firstSeenMs ?? 0) - (a.firstSeenMs ?? 0));
}

/** Items the toolbar counts, across every observed project. */
export const visibleCount = (projects: readonly Leftovers[]) => projects.reduce((sum, p) => sum + visibleItems(p).length, 0);

export function ago(ms: number | null, now: number): string {
  if (ms === null) return '';
  const minutes = Math.max(0, Math.floor((now - ms) / 60000));
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  return hours < 48 ? `${hours} h ago` : `${Math.round(hours / 24)} days ago`;
}

export const baseName = (path: string) => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;

const STRENGTH: Record<string, string> = { created: 'Created by', written: 'Written by', used: 'Used by' };

/** Who made it: Xiox's evidence when it has some, otherwise who could have (never "did"). */
export function authorText(item: LeftoverItem, suspects: readonly Suspect[]): string | null {
  const evidence = item.sessions[0];
  if (evidence) {
    const more = item.sessions.length > 1 ? ` and ${item.sessions.length - 1} more` : '';
    return `${STRENGTH[evidence.strength ?? ''] ?? 'Named by'} ${harnessName(evidence.harness)} session ${evidence.id.slice(0, 12)}${more} (from its history)`;
  }
  if (suspects.length === 1) return `Only ${terminalLabel(suspects[0])} was open when it appeared`;
  if (suspects.length > 1) return `Open when it appeared: ${suspects.map(terminalLabel).join(', ')}`;
  return null;
}
