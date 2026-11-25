// The overview of the projects (dialog «Projects»): one row for each project with its Git state and its last thread. Read strictly from what
// Rust answers; rows that are not rows are left out, so one odd project never empties the list.
export type Git = { state: 'known'; branch: string | null; dirty: number; untracked: number } | { state: 'not_repository' } | { state: 'unknown'; reason: string };
export type LastThread = { id: string; title: string | null; harness: string | null; revision: number; updatedAt: number };
export type ProjectRow = { id: string; path: string; name: string; available: boolean; pinned: boolean; lastOpened: number; git: Git; lastThread: LastThread | null };
export type WorktreePlan = { project: string; path: string; branch: string };

const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const num = (value: unknown): number => typeof value === 'number' && Number.isFinite(value) ? value : 0;

function parseGit(value: unknown): Git {
  if (!record(value)) return { state: 'unknown', reason: 'no answer' };
  if (value.state === 'not_repository') return { state: 'not_repository' };
  if (value.state === 'known') return { state: 'known', branch: typeof value.branch === 'string' ? value.branch : null, dirty: num(value.dirty), untracked: num(value.untracked) };
  return { state: 'unknown', reason: typeof value.reason === 'string' ? value.reason : 'unknown' };
}

export function parseRows(value: unknown): ProjectRow[] | null {
  if (!Array.isArray(value)) return null;
  const rows: ProjectRow[] = [];
  for (const item of value) {
    if (!record(item) || typeof item.id !== 'string' || typeof item.path !== 'string') continue;
    const last = record(item.lastThread) && typeof item.lastThread.id === 'string'
      ? { id: item.lastThread.id, title: typeof item.lastThread.title === 'string' ? item.lastThread.title : null, harness: typeof item.lastThread.harness === 'string' ? item.lastThread.harness : null,
        revision: num(item.lastThread.revision), updatedAt: num(item.lastThread.updatedAt) } : null;
    rows.push({ id: item.id, path: item.path, name: typeof item.name === 'string' ? item.name : '', available: item.available !== false, pinned: item.pinned === true, lastOpened: num(item.lastOpened), git: parseGit(item.git), lastThread: last });
  }
  return rows;
}

export function parsePlan(value: unknown): WorktreePlan | null {
  return record(value) && typeof value.project === 'string' && typeof value.path === 'string' && typeof value.branch === 'string'
    ? { project: value.project, path: value.path, branch: value.branch } : null;
}

/** The Git state in a line. */
export function gitText(git: Git): string {
  if (git.state === 'not_repository') return 'Not a Git repository';
  if (git.state === 'unknown') return `Git: unknown (${git.reason})`;
  const changes = git.dirty + git.untracked;
  return `${git.branch ?? 'detached'} · ${changes ? `${git.dirty} changed, ${git.untracked} untracked` : 'clean'}`;
}

/** What a worktree may be called: lowercase letters, digits and hyphens, 1 to 40. Empty when fine. */
export function worktreeNameProblem(name: string): string {
  return /^[a-z0-9-]{1,40}$/.test(name) ? '' : 'Use 1 to 40 lowercase letters, digits or hyphens.';
}

/** Pinned projects first, then by last use; the order of equals is kept. */
export function ordered(rows: readonly ProjectRow[]): ProjectRow[] {
  return [...rows].sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.lastOpened - a.lastOpened);
}
