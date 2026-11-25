// Every session Shinatama can read, in every store: the library the Sessions view lists, their details, the search and its index, the
// archives the person added. A session the window runs is the terminals' and threads'; these are the ones on disk, read by a worker
// of their own that refuses a read when it is busy. The Sessions view and the inspector's session panels own the rest of this file.
import { errorText } from '../api/core.ts';
import { sessionsApi } from '../api/sessions.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';

export class SessionsController extends Store {
  /** The stores sessions are read from (each agent's folder, the archives added), as the worker answers. */
  roots: unknown = null;
  /** What the last read of the roots said when it failed. */
  problem: string | null = null;

  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  async loadRoots(): Promise<void> {
    try { this.roots = await sessionsApi.roots(); this.problem = null; }
    catch (error) { this.problem = errorText(error); }
    this.changed();
  }

  /** Shows one session's details in the inspector. */
  show(root: string, nativeId: string, at?: { offset: number; fingerprint: string }): void {
    this.app.ui.openPanel(at ? { kind: 'session', root, nativeId, at } : { kind: 'session', root, nativeId });
  }
}
