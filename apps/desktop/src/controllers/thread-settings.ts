// The settings of one thread, in the inspector: its name, whether its sessions talk to each other, the sessions it holds (open ones with
// Show and Rename, closed ones with Resume or Reopen and Delete), archiving it and deleting it. One operation at a time; the sessions
// are read again after each change. The threads themselves are the threads controller's (`app.threads`), the room is the chat's.
import type { Shinatama } from '../app.ts';
import { cleanName, operationFailure } from '../thread-settings-model.ts';
import type { WorksetSummary } from '../api/threads.ts';
import type { ThreadSession } from '../session-tabs.ts';
import { Store } from '../store.ts';

export class ThreadSettingsController extends Store {
  threadId: string | null = null;
  /** The sessions of the thread (closed ones included), null until they are read. */
  sessions: ThreadSession[] | null = null;
  busy = false;
  failed = false;
  message = '';
  private reading = 0;
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  /** The thread's record, as the sidebar lists it. */
  get thread(): WorksetSummary | undefined {
    return this.threadId ? this.app.threads.summary(this.threadId) : undefined;
  }

  get title(): string {
    return (this.threadId ? this.app.threads.title(this.threadId) : null) ?? 'Thread';
  }

  get talks(): boolean {
    return !!this.threadId && this.app.chat.talks(this.threadId);
  }

  /** The panel shows this thread: its sessions are read. */
  show(threadId: string): void {
    if (this.threadId === threadId) { void this.read(); return; }
    this.threadId = threadId; this.sessions = null; this.failed = false; this.message = '';
    this.changed();
    void this.read();
  }

  forget(): void {
    this.reading++;
    this.threadId = null; this.sessions = null; this.failed = false; this.message = '';
    this.changed();
  }

  /** Reads the sessions of the thread; only the latest reply is kept. */
  async read(): Promise<void> {
    const id = this.threadId;
    if (!id) return;
    const ask = ++this.reading;
    try {
      const found = await this.app.threads.sessionsOf(id);
      if (this.threadId === id && ask === this.reading) { this.sessions = found; this.changed(); }
    } catch { /* the list stays as it was */ }
  }

  /** One operation at a time; the sessions are read again after every change. */
  private async operate(doing: string, work: () => Promise<void>, what: 'thread' | 'session' = 'thread'): Promise<boolean> {
    if (this.busy) return false;
    this.busy = true; this.failed = false; this.message = ''; this.changed();
    let done = true;
    try { await work(); }
    catch (error) { done = false; this.failed = true; this.message = operationFailure(error, doing, what); this.app.ui.report(this.message, { error: true }); }
    this.busy = false;
    this.changed();
    await this.read();
    return done;
  }

  async rename(value: string): Promise<boolean> {
    const thread = this.thread;
    const title = cleanName(value);
    if (!thread || !title) return false;
    return this.operate('renamed', () => this.app.threads.update(thread, { title }));
  }

  async setTalking(on: boolean): Promise<void> {
    const thread = this.thread;
    if (!thread) return;
    await this.operate('changed', () => this.app.chat.setTalking({ id: thread.id, projectId: thread.projectId }, on));
  }

  async toggleArchived(): Promise<void> {
    const thread = this.thread;
    if (!thread) return;
    const archive = !thread.archived;
    // The thread leaves the list it was in (archived ones are listed apart): its settings go with it, and a notice says where it went.
    if (await this.operate('changed', () => this.app.threads.update(thread, { archived: archive }))) {
      this.app.ui.closePanels();
      this.app.ui.info(archive ? `${this.title} is archived. It is listed under Show archived threads.` : `${this.title} is back in the list.`);
    }
  }

  /** Deleting a thread cannot be undone: a dialog names it and what goes with it. True when the thread is gone. */
  async remove(): Promise<boolean> {
    const thread = this.thread;
    if (!thread || this.busy) return false;
    const sessions = `${thread.sessions} session${thread.sessions === 1 ? '' : 's'}`;
    const yes = await this.app.ui.ask({
      title: `Delete the thread ${this.title}?`,
      description: `It is deleted from Shinatama with its ${sessions}, closed ones included. The agents' own files stay. This cannot be undone.`,
      confirm: `Delete thread ${this.title}`,
    });
    const now = this.thread;
    if (!yes || !now) return false;
    const gone = await this.operate('deleted', () => this.app.threads.forgetWorkset(now));
    if (gone) { this.app.ui.closePanels(); this.forget(); }
    return gone;
  }

  async showSession(id: string): Promise<void> {
    try { await this.app.threads.focusThreadSession(id); this.app.ui.leaveToWorkspace(); }
    catch { this.app.ui.error('That session could not be shown.'); }
  }

  renameSession(id: string, value: string): Promise<boolean> {
    const title = cleanName(value);
    return title ? this.operate('renamed', () => this.app.threads.renameSession(id, title), 'session') : Promise.resolve(false);
  }

  restore(id: string): Promise<boolean> {
    return this.operate('reopened', () => this.app.threads.restoreSession(id));
  }

  /** Removes a session with nothing saved in it, or deletes a closed one (asked first: it cannot be undone). */
  async forgetSession(session: ThreadSession): Promise<void> {
    if (this.busy) return;
    if (session.state === 'closed') {
      const yes = await this.app.ui.ask({
        title: `Delete the session ${session.name}?`,
        description: "It is deleted from Shinatama with its place in this thread. The agent's own session files stay. This cannot be undone.",
        confirm: `Delete session ${session.name}`,
      });
      if (!yes) return;
    }
    await this.operate(session.state === 'closed' ? 'deleted' : 'removed', () => this.app.threads.forgetSession(session.id), 'session');
  }
}

