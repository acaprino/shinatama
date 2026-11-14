/** What the Sessions panel decides without the page: which saved sessions belong to a project and in what order. */
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
