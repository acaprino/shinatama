// The answers of the browser for the commands of this area (see mock/index.ts): the projects' threads (worksets), the sessions in them,
// the state of the thread sidebar and the conversation of a thread whose sessions talk. A small in-memory store, so that renaming,
// archiving, deleting and closing a session in the browser behave as they do with Rust. At the first load it also leaves in the page's
// storage the threads that were on and the sessions that ran (what a reload of the page keeps), so the workspace starts populated.
import type { Answers } from './index.ts';
import { RUNNING } from './terminals.ts';

type Session = { id: string; harness: string | null; title: string | null; status: 'unbound' | 'inactive' | 'recorded_open' | 'archived'; native: boolean; at: number };
type Thread = { id: string; projectId: string; title: string | null; archived: boolean; revision: number; usedAt: number; sessions: Session[] };

const NOW = Date.now();
const MINUTE = 60_000;
const store: Thread[] = [
  { id: 'w_aaaaaaaaaaaa', projectId: 'p_aaaaaaaaaaaa', title: 'Migrate to Imago', archived: false, revision: 1, usedAt: NOW - 2 * MINUTE, sessions: [
    { id: 't_aaaaaaaaaaaa', harness: 'claude_code', title: 'Claude Code', status: 'recorded_open', native: true, at: NOW - 40 * MINUTE },
    { id: 't_aaaaaaaaaaab', harness: 'codex', title: 'Codex', status: 'recorded_open', native: true, at: NOW - 30 * MINUTE },
    { id: 't_aaaaaaaaaaac', harness: 'powershell', title: 'PowerShell', status: 'recorded_open', native: false, at: NOW - 20 * MINUTE },
    { id: 't_aaaaaaaaaaad', harness: 'claude_code', title: 'Reviewer', status: 'archived', native: true, at: NOW - 3 * 60 * MINUTE },
    { id: 't_aaaaaaaaaaae', harness: 'opencode', title: 'OpenCode', status: 'inactive', native: true, at: NOW - 25 * MINUTE },
  ] },
  { id: 'w_cccccccccccc', projectId: 'p_aaaaaaaaaaaa', title: 'Terminal history', archived: false, revision: 1, usedAt: NOW - 3 * 60 * MINUTE, sessions: [
    { id: 't_cccccccccccc', harness: 'claude_code', title: 'Claude Code', status: 'inactive', native: true, at: NOW - 3 * 60 * MINUTE },
    { id: 't_ccccccccccca', harness: 'codex', title: 'Codex', status: 'inactive', native: true, at: NOW - 4 * 60 * MINUTE },
  ] },
  { id: 'w_dddddddddddd', projectId: 'p_aaaaaaaaaaaa', title: 'Release notes', archived: false, revision: 1, usedAt: NOW - 26 * 60 * MINUTE, sessions: [] },
  { id: 'w_eeeeeeeeeeee', projectId: 'p_aaaaaaaaaaaa', title: 'Old spike', archived: true, revision: 1, usedAt: NOW - 9 * 24 * 60 * MINUTE, sessions: [
    { id: 't_eeeeeeeeeeee', harness: 'pi', title: 'Pi', status: 'inactive', native: true, at: NOW - 9 * 24 * 60 * MINUTE },
  ] },
  { id: 'w_bbbbbbbbbbbb', projectId: 'p_bbbbbbbbbbbb', title: 'Port the cleaner', archived: false, revision: 1, usedAt: NOW - 8 * MINUTE, sessions: [
    { id: 't_bbbbbbbbbbba', harness: 'opencode', title: 'OpenCode', status: 'recorded_open', native: true, at: NOW - 50 * MINUTE },
  ] },
  { id: 'w_ffffffffffff', projectId: 'p_bbbbbbbbbbbb', title: null, archived: false, revision: 1, usedAt: NOW - 90 * MINUTE, sessions: [
    { id: 't_ffffffffffff', harness: 'claude_code', title: 'Fix the flaky checkout test', status: 'inactive', native: true, at: NOW - 90 * MINUTE },
  ] },
];

const ON = [{ id: 'w_aaaaaaaaaaaa', projectId: 'p_aaaaaaaaaaaa' }, { id: 'w_bbbbbbbbbbbb', projectId: 'p_bbbbbbbbbbbb' }];
const BOUND: Record<string, { threadId: string; workset: { id: string; projectId: string } }> = {
  i_aaaaaaaaaaaa: { threadId: 't_aaaaaaaaaaaa', workset: ON[0] },
  i_aaaaaaaaaaab: { threadId: 't_aaaaaaaaaaab', workset: ON[0] },
  i_aaaaaaaaaaac: { threadId: 't_aaaaaaaaaaac', workset: ON[0] },
  i_bbbbbbbbbbba: { threadId: 't_bbbbbbbbbbba', workset: ON[1] },
};

/** What a reload of the page keeps: the threads that were on, and which thread each running terminal belongs to. */
function seed(): void {
  try {
    if (localStorage.getItem('shinatama.threads') === null) localStorage.setItem('shinatama.threads', JSON.stringify(ON));
    if (sessionStorage.getItem('shinatama.sessions') === null) {
      sessionStorage.setItem('shinatama.sessions', JSON.stringify(Object.fromEntries(RUNNING.map(session => [session.id, BOUND[session.id] ?? { threadId: null, workset: null }]))));
    }
  } catch { /* no storage: the workspace starts empty */ }
}
seed();

const summary = (thread: Thread) => {
  const open = thread.sessions.filter(session => session.status === 'recorded_open').length;
  const first = thread.sessions.find(session => session.status !== 'archived');
  return {
    id: thread.id, projectId: thread.projectId, title: thread.title, archived: thread.archived, revision: thread.revision, usedAt: thread.usedAt,
    sessions: thread.sessions.filter(session => session.status !== 'archived').length, open,
    lead: first ? { threadId: first.id, title: first.title, harness: first.harness } : null,
  };
};
const rowOf = (thread: Thread, session: Session) => ({
  id: session.id, projectId: thread.projectId, kind: 'work', title: session.title, revision: 1, updatedAt: session.at,
  harness: session.harness, nativeAvailable: session.native, status: session.status,
});
const find = (id: string) => store.find(thread => thread.id === id);
const owner = (sessionId: string) => store.find(thread => thread.sessions.some(session => session.id === sessionId));

type Request<T> = { request: T };
const body = <T>(args: Record<string, unknown> | undefined): T => (args as Request<T>).request;

let sidebar = { version: 1, width: 236, collapsed: false, activeProjects: ['p_aaaaaaaaaaaa', 'p_bbbbbbbbbbbb'] };
let sidebarRevision = 1;

// The room of the thread that talks.
const TALKING = new Set(['w_aaaaaaaaaaaa']);
const clock = (minutes: number) => new Date(NOW - minutes * MINUTE).toISOString();
const events = [
  { eventId: '1', timestamp: clock(52), kind: 'join', actor: 'claude-1', actorHost: false, recipients: [], text: null, signal: null, reason: null },
  { eventId: '2', timestamp: clock(51), kind: 'join', actor: 'codex-1', actorHost: false, recipients: [], text: null, signal: null, reason: null },
  { eventId: '3', timestamp: clock(49), kind: 'message', actor: 'shinatama', actorHost: true, recipients: ['claude-1', 'codex-1'], text: 'You work in this thread: claude-1 writes the screens, codex-1 reviews them.', signal: null, reason: null },
  { eventId: '4', timestamp: clock(36), kind: 'message', actor: 'claude-1', actorHost: false, recipients: ['codex-1'], text: 'The workspace tab renderer is ready. Please review `screens/workspace/terminal-tab.tsx`.', signal: null, reason: null },
  { eventId: '5', timestamp: clock(31), kind: 'message', actor: 'codex-1', actorHost: false, recipients: ['claude-1'], text: 'Reviewed it. Also reject payloads older than 5 minutes: the header carries the timestamp.', signal: null, reason: null },
  { eventId: '6', timestamp: clock(30), kind: 'signal', actor: 'codex-1', actorHost: false, recipients: [], text: null, signal: 'review_done', reason: 'two comments' },
];
let nextEvent = 7;

export const answers: Answers = {
  workset_list: args => {
    const { projectId, archived } = body<{ projectId: string; archived: boolean }>(args);
    return { rows: store.filter(thread => thread.projectId === projectId && thread.archived === archived).sort((a, b) => b.usedAt - a.usedAt).map(summary), more: false };
  },
  workset_threads: args => {
    const thread = find(body<{ worksetId: string }>(args).worksetId);
    if (!thread) throw { code: 'conflict' };
    return thread.sessions.map(session => rowOf(thread, session));
  },
  workset_of: args => {
    const thread = owner(body<{ threadId: string }>(args).threadId);
    return thread ? { id: thread.id, projectId: thread.projectId } : null;
  },
  workset_create: args => {
    const { projectId } = body<{ projectId: string }>(args);
    const id = `w_${'abcdefghijklmnopqrstuvwxyz234567'.charAt(store.length % 32).repeat(12)}`;
    store.push({ id, projectId, title: null, archived: false, revision: 1, usedAt: Date.now(), sessions: [] });
    return { id, revision: 1 };
  },
  workset_update: args => {
    const { worksetId, title, archived } = body<{ worksetId: string; title?: string; archived?: boolean }>(args);
    const thread = find(worksetId);
    if (!thread) throw { code: 'conflict' };
    if (title !== undefined) thread.title = title || null;
    if (archived !== undefined) thread.archived = archived;
    thread.revision += 1;
    return null;
  },
  workset_delete: args => {
    const at = store.findIndex(thread => thread.id === body<{ worksetId: string }>(args).worksetId);
    if (at >= 0) store.splice(at, 1);
    return null;
  },
  workset_adopt: args => {
    const { worksetId, threadId } = body<{ worksetId: string; threadId: string }>(args);
    const thread = find(worksetId);
    if (!thread || thread.sessions.some(session => session.id === threadId)) return null;
    thread.sessions.push({ id: threadId, harness: 'claude_code', title: null, status: 'recorded_open', native: true, at: Date.now() });
    return null;
  },
  thread_rename: args => {
    const { threadId, title } = body<{ threadId: string; title: string }>(args);
    const thread = owner(threadId);
    const session = thread?.sessions.find(one => one.id === threadId);
    if (session) session.title = title;
    return null;
  },
  thread_archive: args => {
    const { threadId, archived } = body<{ threadId: string; archived: boolean }>(args);
    const session = owner(threadId)?.sessions.find(one => one.id === threadId);
    if (session) session.status = archived ? 'archived' : 'inactive';
    return null;
  },
  thread_forget: args => {
    const { threadId } = body<{ threadId: string }>(args);
    const thread = owner(threadId);
    if (thread) thread.sessions = thread.sessions.filter(session => session.id !== threadId);
    return null;
  },
  thread_recover: null,
  thread_sidebar_load: () => ({ revision: sidebarRevision, settings: sidebar }),
  thread_sidebar_save: args => {
    sidebar = body<{ settings: typeof sidebar }>(args).settings;
    return { revision: ++sidebarRevision, settings: sidebar };
  },
  thread_names: [],
  thread_summaries_pause: [],
  titles_enabled: false,
  thread_chat_list: () => [...TALKING],
  thread_chat_set: args => {
    const { worksetId, enabled } = body<{ worksetId: string; enabled: boolean }>(args);
    if (enabled) TALKING.add(worksetId); else TALKING.delete(worksetId);
    return { enabled: [...TALKING], warning: null };
  },
  thread_chat_overview: args => {
    const { worksetIds, seen } = body<{ worksetIds: string[]; seen: Record<string, { eventId: string }> }>(args);
    const latest = events.at(-1)!;
    return {
      service: 'ready',
      rooms: Object.fromEntries(worksetIds.filter(id => TALKING.has(id)).map(id => [id, { latest: latest.eventId, lastActivity: latest.timestamp, unread: seen[id] !== undefined && BigInt(seen[id].eventId) < BigInt(latest.eventId) }])),
      connected: { 'w_aaaaaaaaaaaa': ['t_aaaaaaaaaaaa', 't_aaaaaaaaaaab'] },
      launched: {},
    };
  },
  thread_chat_snapshot: args => {
    const { worksetId } = body<{ worksetId: string }>(args);
    if (!TALKING.has(worksetId)) return { enabled: false };
    return {
      enabled: true,
      members: [
        { memberId: 'm1', name: 'claude-1', harness: 'claude_code', role: 'worker', active: true, sessionId: 't_aaaaaaaaaaaa', actorHost: false },
        { memberId: 'm2', name: 'codex-1', harness: 'codex', role: 'worker', active: true, sessionId: 't_aaaaaaaaaaab', actorHost: false },
        { memberId: 'm3', name: 'user', harness: 'user', role: 'user', active: true, sessionId: null, user: true },
      ],
      sessions: [
        { sessionId: 't_aaaaaaaaaaaa', harness: 'claude_code', title: 'Claude Code', state: 'running', memberName: 'claude-1', connection: 'in_room', wakes: 2 },
        { sessionId: 't_aaaaaaaaaaab', harness: 'codex', title: 'Codex', state: 'running', memberName: 'codex-1', connection: 'in_room', wakes: 1 },
        { sessionId: 't_aaaaaaaaaaae', harness: 'opencode', title: 'OpenCode', state: 'not_running', memberName: null, connection: 'not_connected', wakes: null },
      ],
      events,
    };
  },
  thread_chat_send: args => {
    const { recipients, text } = body<{ recipients: string[]; text: string }>(args);
    events.push({ eventId: String(nextEvent++), timestamp: new Date().toISOString(), kind: 'message', actor: 'user', actorHost: false, recipients, text, signal: null, reason: null });
    return null;
  },
};
