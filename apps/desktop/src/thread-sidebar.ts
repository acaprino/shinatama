// Thread types shared by the thread sidebar (screens/workspace/sidebar.tsx), the session tabs and the threads controller, and what a row
// of the sidebar says (the name of a thread, its second line), as pure functions.
export type CurrentThreadProject = { projectId: string; label?: string };
export type ThreadCursor = { updatedAt: number; threadId: string };
export type ThreadSummary = {
  id: string; projectId: string; kind: 'work' | 'roundtable' | 'companion'; title: string | null;
  revision: number; updatedAt: number; harness: string | null; nativeAvailable: boolean | null;
  status: 'unbound' | 'inactive' | 'recorded_open' | 'archived';
};
export type ThreadListPage = { rows: ThreadSummary[]; next: ThreadCursor | null };
export type ThreadListRequest = { projectId: string; cursor: ThreadCursor | null; limit: 50; archived: boolean };
export type ThreadArchiveRequest = { projectId: string; threadId: string; expectedRevision: number; archived: boolean };
export type ThreadForgetRequest = { projectId: string; threadId: string; expectedRevision: number };
export type ThreadRenameRequest = { projectId: string; threadId: string; expectedRevision: number; title: string };
export type ThreadSidebarState = { version: 1; width: number; collapsed: boolean; activeProjects?: string[]; hidden?: boolean };

// --- what a row of the thread sidebar says (pure: the sidebar draws it, the tests read it) ---------------------------------------------

/** What a thread needs to be named and told: its record as the store gave it. */
export interface ThreadRow {
  id: string; title: string | null; archived: boolean; usedAt: number; sessions: number;
  lead: { threadId: string; title: string | null; harness: string | null } | null;
}

/** "now", "5 min ago", "2 h ago", or the date: when a thread was used. */
export function agoText(timestamp: number, now: number = Date.now()): string {
  const minutes = Math.floor((now - timestamp) / 60000);
  if (minutes < 1) return 'now';
  if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)} h ago`;
  const date = new Date(timestamp);
  return Number.isNaN(date.valueOf()) ? '' : date.toLocaleDateString();
}

/** The name of a thread: the one the person gave it, else its first session's (the one the person gave that, else the one its agent
 *  gave it), else null (the row then says "New thread"). A session still named after its agent has no name of its own. */
export function threadTitle(row: ThreadRow, agentName: string | undefined, harnessLabel: (harness: string | null) => string): string | null {
  if (row.title) return row.title;
  const lead = row.lead;
  if (!lead) return null;
  const own = lead.title && lead.title !== harnessLabel(lead.harness) ? lead.title : null;
  return own ?? agentName ?? lead.title ?? null;
}

/** The second line of a row: how many sessions, how many run, and what the person can do with an empty thread. */
export function threadStateLine(row: Pick<ThreadRow, 'archived' | 'sessions'>, state: { on: boolean; running: number }): string {
  const total = Math.max(row.sessions, state.running);
  const sessions = `${total} session${total === 1 ? '' : 's'}`;
  if (row.archived) return `Archived · ${sessions}`;
  if (state.running) return state.on ? `${sessions} · ${state.running} running` : `${state.running} running · tabs hidden`;
  if (!total) return state.on ? 'Empty · + adds a session' : 'Empty';
  return sessions;
}

/** Two letters for a thread's mark in the rail: the first of its first two words, or the first two of one word. */
export function initials(name: string): string {
  const words = name.match(/[\p{L}\p{N}]+/gu) ?? [];
  const [first = '', second] = words;
  const letters = second ? first.slice(0, 1) + second.slice(0, 1) : first.slice(0, 2);
  return (letters || '·').toUpperCase();
}

/** What a refused thread or session operation says to the person, from the code the store refused it with. */
export function threadFailureText(error: unknown, doing: string, what: 'thread' | 'session' = 'thread'): string {
  const record = error && typeof error === 'object' ? error as { code?: unknown; issue?: unknown } : null;
  const code = typeof error === 'string' ? error : record?.code ?? record?.issue;
  if (code === 'project_missing') return 'The project is unavailable. Choose it again.';
  if (code === 'busy') return 'Thread storage is busy. Try again.';
  if (code === 'conflict') return `This ${what} changed. Reload and try again.`;
  if (code === 'owned_elsewhere') return 'A session of this thread is running. Stop it first.';
  // Nothing runs, but a session is still marked open by a Shinatama that closed badly, and its end could not be proven.
  if (code === 'open_unverified') return 'A session of this thread is still marked open by a Shinatama that is no longer running, and that could not be verified. The thread was kept.';
  return `The ${what} could not be ${doing}. Nothing was changed.`;
}
