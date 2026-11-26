/** What the Sessions view decides without the page: which saved sessions belong to a project and in what order. */
import { agentName, sameRoot, type SessionCursor, type SessionLibraryRoot, type SessionMetadataRow, type SessionRootSelector } from './session-library-model.ts';
import { matchesFilter, type InstanceRow, type Titles } from './instances-model.ts';
const folderKey = (path: string) => path.replace(/^\\\\\?\\/, '').replace(/\//g, '\\').replace(/\\+$/, '').toLocaleLowerCase();
/** Whether a session's working folder is the project's folder or lies inside it. */
export function inProject(workingDirectory: string | null, project: string | null): boolean {
  if (!project) return true; if (!workingDirectory) return false;
  const folder = folderKey(workingDirectory); const root = folderKey(project);
  return folder === root || folder.startsWith(`${root}\\`);
}
export const folderName = (path: string | null) => path ? path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path : 'Unknown folder';
const stamp = (value: string | null) => { const at = value ? Date.parse(value) : NaN; return Number.isNaN(at) ? 0 : at; };
/** Newest first; a session with no date goes last. */
export function byRecency<T extends { row: { updatedAt: string | null } }>(items: T[]): T[] {
  return [...items].sort((a, b) => stamp(b.row.updatedAt) - stamp(a.row.updatedAt));
}

// --- the library the Sessions view lists ----------------------------------------------------------------------------------------

/** One archive the library reads: what it holds so far, where the next page starts, and why it could not be read. */
export type Archive = { root: SessionLibraryRoot; rows: SessionMetadataRow[]; next: SessionCursor | null; failure: string | null };
export type SavedRow = { root: SessionLibraryRoot; row: SessionMetadataRow };
/** What narrows the library: the project (a folder, or none for all of them), one archive, one agent and the text typed. */
export type Narrowing = { project: string | null; archive: SessionRootSelector | null; agent: string | null; text: string };

const fold = (value: string | null | undefined) => value?.toLocaleLowerCase() ?? '';

/** The saved sessions that answer the narrowing, newest first. `text` is matched against title, folder, id and agent. */
export function savedRows(archives: readonly Archive[], narrowing: Narrowing): SavedRow[] {
  const wanted = narrowing.text.trim().toLocaleLowerCase();
  const only = narrowing.archive ? archives.filter(archive => sameRoot(archive.root, narrowing.archive!)) : archives;
  const all = only.flatMap(archive => archive.rows.map(row => ({ root: archive.root, row })));
  return byRecency(all.filter(({ root, row }) =>
    (!narrowing.agent || root.harness === narrowing.agent)
    && inProject(row.workingDirectory, narrowing.project)
    && (!wanted || [row.title, row.workingDirectory, row.nativeId, agentName(root.harness)].some(value => fold(value).includes(wanted)))));
}

/** The running instances that answer the narrowing (the agent and the text; a project does not narrow what runs). */
export function runningRows(rows: readonly InstanceRow[], titles: Titles | null, narrowing: Pick<Narrowing, 'agent' | 'text'>): InstanceRow[] {
  return rows.filter(row => (!narrowing.agent || row.agent === narrowing.agent)
    && matchesFilter(row, titles?.rows.find(entry => entry.instanceId === row.id), narrowing.text));
}

/** How many sessions each agent has, running and saved together, in the order of the agents' names; `all` first. */
export function agentCounts(archives: readonly Archive[], running: readonly InstanceRow[]): { agent: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const archive of archives) counts.set(archive.root.harness, (counts.get(archive.root.harness) ?? 0) + archive.rows.length);
  for (const row of running) counts.set(row.agent, (counts.get(row.agent) ?? 0) + 1);
  return [...counts].map(([agent, count]) => ({ agent, count })).sort((a, b) => agentName(a.agent).localeCompare(agentName(b.agent)));
}

export const rootId = (root: SessionRootSelector): string => `${root.storeRootId}\u0000${root.configurationRevision}`;

// --- the rows of the table ----------------------------------------------------------------------------------------------------

/** A row of the Sessions table: a heading (Running, Saved sessions), a session that runs now, or a saved one. */
export type TableRow =
  | { kind: 'group'; id: string; label: string; count: number }
  | { kind: 'running'; id: string; row: InstanceRow }
  | { kind: 'saved'; id: string; saved: SavedRow };

export const RUNNING_GROUP = 'group:running';
export const SAVED_GROUP = 'group:saved';

/** Running sessions first under their heading, saved ones after; a heading whose group has nothing is left out. */
export function tableRows(running: readonly InstanceRow[], saved: readonly SavedRow[]): TableRow[] {
  const rows: TableRow[] = [];
  if (running.length) rows.push({ kind: 'group', id: RUNNING_GROUP, label: 'Running', count: running.length }, ...running.map(row => ({ kind: 'running' as const, id: `running:${row.id}`, row })));
  if (saved.length) rows.push({ kind: 'group', id: SAVED_GROUP, label: 'Saved sessions', count: saved.length }, ...saved.map(one => ({ kind: 'saved' as const, id: `saved:${rootId(one.root)}\u0000${one.row.nativeId}`, saved: one })));
  return rows;
}
