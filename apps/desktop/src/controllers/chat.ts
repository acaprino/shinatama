// ADR-0057: the threads whose sessions talk to each other through their room (Mukade), the marks that say so on rows and tabs, and
// what the person has seen of each room. The conversation itself is a tab of a pane (`chat-<thread>`, kind `chat`), drawn by the
// workspace screens; this controller reads the room's overview, which is cheap, and keeps the person's place in each room.
import { errorText, hasHost } from '../api/core.ts';
import { chatApi, type ChatOverview, type ChatSeen, type WorksetScope } from '../api/threads.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';
import { laterEvent, MAX_MESSAGE_BYTES, mergeEvents, parseSnapshot, type ChatEvent, type ChatSnapshot, type RoomFilter } from '../thread-chat-model.ts';
import { chatViewId } from '../view-ids.ts';
import type { TerminalSession } from './terminals.ts';

const SEEN_KEY = 'shinatama.threadChat.seen.v2';
const OVERVIEW_MS = 10_000;
/** How often the conversation that is on screen reads its room again. */
export const ROOM_POLL_MS = 3000;
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

/** What the conversation panel of one thread shows: its room as last read, and what the person is doing in it. */
export interface Room {
  snapshot: ChatSnapshot | null;
  /** The lines of the room, oldest first, the ones of every read merged. */
  events: ChatEvent[];
  /** What the panel says under its title: that the room is being read, that a message went, what failed. */
  status: { text: string; error: boolean };
  /** The room could not be read and there is nothing to show yet: the panel says so, with Retry. */
  failure: string | null;
  /** The first read is under way and nothing is shown yet. */
  loading: boolean;
  sending: boolean;
  /** The recipient the person chose; null until they do (the first of the members is shown meanwhile). */
  to: string | null;
  filter: RoomFilter;
}

const newRoom = (): Room => ({ snapshot: null, events: [], status: { text: '', error: false }, failure: null, loading: false, sending: false, to: null, filter: 'all' });

interface Watcher { count: number; timer: ReturnType<typeof setInterval>; generation: number; reading: boolean; seenEvent: string | null }

export class ChatController extends Store {
  /** The threads that talk. */
  threads: ReadonlySet<string> = new Set();
  /** The thread whose conversation is on screen, if any: its news is read as it comes, so it is never unread. */
  reading: string | null = null;
  /** The rooms the conversation tabs show, by thread. */
  private readonly rooms = new Map<string, Room>();
  private readonly watchers = new Map<string, Watcher>();
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
    if (!hasHost) return;
    // Which threads talk, and the marks of their rooms; read again while the window is shown.
    try { this.threads = new Set(await chatApi.list()); } catch { this.threads = new Set(); }
    void this.readOverview();
    this.timer = setInterval(() => { if (!document.hidden) void this.readOverview(); }, OVERVIEW_MS);
  }

  dispose(): void {
    clearInterval(this.timer);
    for (const watcher of this.watchers.values()) clearInterval(watcher.timer);
    this.watchers.clear();
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

  // --- the conversation of one thread -----------------------------------------------------------------------

  /** The room of a thread as the conversation shows it (an empty one before the first read). */
  room(threadId: string): Room {
    let room = this.rooms.get(threadId);
    if (!room) { room = newRoom(); this.rooms.set(threadId, room); }
    return room;
  }

  /** The conversation of this thread is on screen: its room is read now and every few seconds until the returned function is called.
   *  It is then read as it comes, so it is never unread. Several screens may watch one room. */
  watch(threadId: string): () => void {
    const room = this.room(threadId);
    const known = this.watchers.get(threadId);
    if (known) known.count += 1;
    else {
      const watcher: Watcher = { count: 1, generation: 0, reading: false, seenEvent: null, timer: setInterval(() => { void this.readRoom(threadId); }, ROOM_POLL_MS) };
      this.watchers.set(threadId, watcher);
      if (!room.snapshot) { room.status = { text: 'Reading the room…', error: false }; room.loading = true; }
      this.setReading(threadId);
      void this.readRoom(threadId);
    }
    let stopped = false;
    return () => {
      if (stopped) return;
      stopped = true;
      const current = this.watchers.get(threadId);
      if (!current || --current.count > 0) return;
      clearInterval(current.timer);
      current.generation += 1;
      this.watchers.delete(threadId);
      if (this.reading === threadId) this.setReading(null);
    };
  }

  /** Reads the room once (the poll, and Retry). One read at a time for each room; a read that is no longer wanted is dropped. */
  async readRoom(threadId: string): Promise<void> {
    const watcher = this.watchers.get(threadId);
    if (!watcher || watcher.reading || (typeof document !== 'undefined' && document.hidden)) return;
    const room = this.room(threadId);
    const generation = watcher.generation;
    watcher.reading = true;
    const stale = () => this.watchers.get(threadId) !== watcher || watcher.generation !== generation;
    try {
      const since = room.events.at(-1)?.timestamp ?? null;
      const snapshot = parseSnapshot(await chatApi.snapshot({ worksetId: threadId, since }));
      if (stale()) return;
      room.loading = false;
      if (!snapshot) { this.roomFailed(room, 'The conversation could not be read.'); return; }
      room.snapshot = snapshot;
      room.failure = null;
      if (!snapshot.enabled) {
        room.events = [];
        room.status = { text: 'This thread\'s sessions do not talk to each other. Turn it on in the thread\'s settings (⋯).', error: false };
        this.changed();
        return;
      }
      room.events = mergeEvents(room.events, snapshot.events);
      room.status = { text: '', error: false };
      this.changed();
      const latest = room.events.at(-1);
      if (latest && latest.eventId !== watcher.seenEvent) {
        watcher.seenEvent = latest.eventId;
        this.markSeen(threadId, latest.eventId, latest.timestamp);
      }
    } catch (error) {
      if (!stale()) { room.loading = false; this.roomFailed(room, errorText(error) || 'The conversation could not be read.'); }
    } finally { watcher.reading = false; }
  }

  /** A read that failed: with nothing on screen the panel offers Retry; with a room on screen it stays a line of status. */
  private roomFailed(room: Room, reason: string): void {
    if (room.snapshot) room.status = { text: reason, error: true };
    else { room.status = { text: '', error: false }; room.failure = reason; }
    this.changed();
  }

  /** Retry of a room that could not be read. */
  retryRoom(threadId: string): void {
    const room = this.room(threadId);
    room.failure = null;
    room.loading = true;
    this.changed();
    void this.readRoom(threadId);
  }

  chooseRecipient(threadId: string, to: string): void {
    this.room(threadId).to = to;
    this.changed();
  }

  setFilter(threadId: string, filter: RoomFilter): void {
    this.room(threadId).filter = filter;
    this.changed();
  }

  /** Writes to the room as the person (the member `user`): to one session's member, or to the whole room (`recipient` empty). Resolves to
   *  whether it went, so the field can be emptied. */
  async send(threadId: string, recipient: string, text: string): Promise<boolean> {
    const room = this.room(threadId);
    if (room.sending || !text.trim()) return false;
    if (new TextEncoder().encode(text).length > MAX_MESSAGE_BYTES) {
      room.status = { text: 'Write a message of at most 16 KB.', error: true };
      this.changed();
      return false;
    }
    room.sending = true;
    this.changed();
    try {
      await chatApi.send({ worksetId: threadId, recipients: recipient ? [recipient] : [], text });
      room.sending = false;
      await this.readRoom(threadId);
      // After the read, which clears the status: the line stays until the next read.
      room.status = { text: recipient ? `Sent to ${recipient}.` : 'Sent to the room.', error: false };
      this.changed();
      return true;
    } catch (error) {
      room.status = { text: errorText(error), error: true };
      return false;
    } finally { room.sending = false; this.changed(); }
  }

  /** One cheap read for every talking thread: the marks on rows and tabs. */
  async readOverview(): Promise<void> {
    if (!hasHost || this.reading_) return;
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
