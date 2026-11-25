// ADR-0057: the threads whose sessions talk to each other through their room (Mukade), the marks that say so on rows and tabs, and
// what the person has seen of each room. The conversation itself is a tab of a pane (`chat-<thread>`, kind `chat`), drawn by the
// workspace screens; this controller reads the room's overview, which is cheap, and keeps the person's place in each room.
import { inTauri } from '../api/core.ts';
import { chatApi, type ChatOverview, type ChatSeen, type WorksetScope } from '../api/threads.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';
import { laterEvent } from '../thread-chat-model.ts';
import { chatViewId } from '../view-ids.ts';
import type { TerminalSession } from './terminals.ts';

const SEEN_KEY = 'shinatama.threadChat.seen.v2';
const OVERVIEW_MS = 10_000;
const AGENTS = ['Claude Code', 'claude_code', 'Codex', 'codex', 'OpenCode', 'opencode', 'Pi', 'pi'];

/** The last event the user saw per thread, with its time (the backend reads the room from there): a convenience of this window, kept
 *  in its own storage. */
function keptSeen(): Map<string, ChatSeen> {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(SEEN_KEY) ?? '{}');
    if (saved && typeof saved === 'object' && !Array.isArray(saved)) {
      return new Map(Object.entries(saved as Record<string, unknown>).filter((entry): entry is [string, ChatSeen] => {
        const value = entry[1] as Partial<ChatSeen> | null;
        return /^w_[a-z2-7]{12}$/.test(entry[0]) && !!value && typeof value.eventId === 'string' && /^[0-9]{1,19}$/.test(value.eventId)
          && typeof value.timestamp === 'string' && value.timestamp.length <= 64;
      }).slice(0, 200));
    }
  } catch { /* no storage: nothing is marked seen */ }
  return new Map();
}

export class ChatController extends Store {
  /** The threads that talk. */
  threads: ReadonlySet<string> = new Set();
  /** The thread whose conversation is on screen, if any: its news is read as it comes, so it is never unread. */
  reading: string | null = null;
  private readonly unreadIds = new Set<string>();
  private readonly connected = new Map<string, Set<string>>();
  private readonly launched = new Map<string, Set<string>>();
  private readonly seen = keptSeen();
  private reading_ = false;
  private timer: ReturnType<typeof setInterval> | undefined;

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  async start(): Promise<void> {
    if (!inTauri) return;
    // Which threads talk, and the marks of their rooms; read again while the window is shown.
    try { this.threads = new Set(await chatApi.list()); } catch { this.threads = new Set(); }
    void this.readOverview();
    this.timer = setInterval(() => { if (!document.hidden) void this.readOverview(); }, OVERVIEW_MS);
  }

  dispose(): void {
    clearInterval(this.timer);
  }

  /** Whether a thread's sessions talk. */
  talks(threadId: string): boolean {
    return this.threads.has(threadId);
  }

  /** The room of this thread has news the person has not seen. */
  unread(threadId: string): boolean {
    return this.unreadIds.has(threadId) && this.reading !== threadId;
  }

  /** " · in chat" on the tab of a session that is a member of its talking thread's room; " · chat" on an OpenCode or Pi session
   *  launched with the room's kit, whose member Shinatama cannot tie to it. */
  mark(view: Pick<TerminalSession, 'info' | 'threadId' | 'workset'>): string {
    if (!view.workset || !this.threads.has(view.workset.id) || !AGENTS.includes(view.info.shell)) return '';
    if (view.threadId && this.connected.get(view.workset.id)?.has(view.threadId)) return ' · in chat';
    return view.threadId && this.launched.get(view.workset.id)?.has(view.threadId) ? ' · chat' : ' · not in chat yet';
  }

  /** What the person has seen of a room, to ask the backend for news from there. */
  seenOf(threadId: string): ChatSeen | undefined {
    return this.seen.get(threadId);
  }

  /** The person saw the room up to this event. */
  markSeen(id: string, eventId: string, timestamp: string): void {
    const before = this.seen.get(id);
    if (before && !laterEvent(eventId, before.eventId)) return;
    this.seen.set(id, { eventId, timestamp });
    // Seen now: the mark goes at once, not at the next overview.
    this.unreadIds.delete(id);
    try { localStorage.setItem(SEEN_KEY, JSON.stringify(Object.fromEntries([...this.seen].slice(-200)))); } catch { /* kept for this run */ }
    this.changed();
  }

  /** The conversation of this thread is on screen (or not any more). */
  setReading(threadId: string | null): void {
    if (this.reading === threadId) return;
    this.reading = threadId;
    this.changed();
  }

  /** Makes the sessions of a thread talk, or stop. A new session of a thread that talks gets its room at launch. */
  async setTalking(thread: Readonly<WorksetScope>, enabled: boolean): Promise<void> {
    const result = await chatApi.set(thread.id, enabled);
    this.threads = new Set(result.enabled);
    if (result.warning) this.app.ui.error(`The thread will talk, but its room is not ready now: ${result.warning}`);
    else if (enabled) this.app.ui.info('Claude Code, Codex, OpenCode and Pi sessions of this thread opened (or, for Claude Code and Codex, resumed) from now on join its room.');
    if (!enabled) this.app.layout.close(chatViewId(thread.id));
    await this.readOverview();
    this.changed();
  }

  /** Shows the conversation of a thread: a tab beside the terminals, one per thread. */
  open(thread: Readonly<WorksetScope>): void {
    const id = chatViewId(thread.id);
    this.app.ui.leaveToWorkspace();
    this.app.layout.open({ id, resource: { kind: 'chat', resourceId: id }, hidden: false }, other => other.resource.kind === 'chat');
  }

  /** One cheap read for every talking thread: the marks on rows and tabs. */
  async readOverview(): Promise<void> {
    if (!inTauri || this.reading_) return;
    const ids = [...this.threads].slice(0, 64);
    if (!ids.length) { this.unreadIds.clear(); this.connected.clear(); this.launched.clear(); this.changed(); return; }
    this.reading_ = true;
    try {
      const seen = Object.fromEntries(ids.filter(id => this.seen.has(id)).map(id => [id, this.seen.get(id)!]));
      const overview: ChatOverview = await chatApi.overview(ids, seen);
      const marks = () => [this.connected, this.launched].map(map => [...map].map(([id, set]) => `${id}:${[...set].join(',')}`).join(';')).join('|');
      const before = marks();
      const unreadBefore = [...this.unreadIds].join(',');
      this.unreadIds.clear(); this.connected.clear(); this.launched.clear();
      for (const [id, room] of Object.entries(overview.rooms ?? {})) {
        // A thread seen for the first time starts as read: old history is not news.
        if (!this.seen.has(id)) this.markSeen(id, room.latest, room.lastActivity ?? new Date(0).toISOString());
        else if (room.unread) this.unreadIds.add(id);
      }
      for (const [id, sessions] of Object.entries(overview.connected ?? {})) this.connected.set(id, new Set(sessions));
      for (const [id, sessions] of Object.entries(overview.launched ?? {})) this.launched.set(id, new Set(sessions));
      if (before !== marks() || unreadBefore !== [...this.unreadIds].join(',')) { this.changed(); this.app.threads.touch(); }
    } catch { /* the marks keep their last state */ }
    finally { this.reading_ = false; }
  }
}
