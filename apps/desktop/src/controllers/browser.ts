// The pages of the browser tab (ADR-0056): a page Rust drives through the browser engine, which the person watches and takes over,
// and which an agent's browser tools may open on its own. Each page is a tab of a pane (`browser-<page>`, kind `browser`), one pane
// per window at first. This controller holds the pages, what each tab sends to Rust (the person's input in batches, its size, whether
// it is in sight) and the frames Rust sends back, which the screen of the tab (screens/browser) draws on its surface.
import { listen, type Unlisten } from '../api/core.ts';
import { browserApi } from '../api/browser.ts';
import type { Shinatama } from '../app.ts';
import { coalesce, type BrowserInputEvent } from '../browser-input.ts';
import {
  addressUrl, browserViewId, pageLabel, parseSaved, reopenTarget, type BrowserTarget, type NavigateAction, type PageInfo, type SavedPage,
} from '../browser-model.ts';
import { Store } from '../store.ts';

export { browserViewId } from '../browser-model.ts';
export type { BrowserTarget, PageInfo } from '../browser-model.ts';

const SAVED_KEY = 'shinatama.browser.tabs';
/** How often a tab in sight asks Rust about its page again (the title and the address change by themselves). */
export const POLL_MS = 1500;

/** What a tab that is on screen holds: its input waiting to go, whether it is in sight, and where its frames are drawn. */
interface Connection {
  queue: BrowserInputEvent[];
  sending: boolean;
  visible: boolean;
  sink: ((bytes: ArrayBuffer) => void) | null;
  poll: ReturnType<typeof setInterval> | null;
}

export class BrowserController extends Store {
  /** The pages that are open, by their view id. */
  readonly pages = new Map<string, PageInfo>();
  /** When a batch of input goes: the next turn, so that a burst of moves is one call. A test makes it its own. */
  schedule: (run: () => void) => void = run => { setTimeout(run, 0); };

  private readonly files = new Map<string, string>();
  private readonly connections = new Map<string, Connection>();
  private readonly stops: Unlisten[] = [];
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    super();
    this.app = app;
  }

  async start(): Promise<void> {
    // A page an agent opened through its browser tools becomes a tab the user can watch and take over.
    this.stops.push(await listen<{ page: string; instance: string }>('browser-agent-page', ({ page }) => {
      if (!this.viewOf(page)) void this.open({ page });
    }));
    this.stops.push(await listen<{ page: string }>('browser-page-closed', ({ page }) => {
      const view = this.viewOf(page);
      if (view) this.forget(view);
    }));
  }

  dispose(): void {
    for (const stop of this.stops.splice(0)) stop();
    for (const connection of this.connections.values()) if (connection.poll !== null) clearInterval(connection.poll);
    this.connections.clear();
  }

  viewOf(pageId: string): string | null {
    return [...this.pages].find(([, page]) => page.id === pageId)?.[0] ?? null;
  }

  isOpen(viewId: string): boolean {
    return this.pages.has(viewId);
  }

  /** The tab's name: its page's title, "closed" after it when the page is gone. */
  label(viewId: string): string {
    const page = this.pages.get(viewId);
    return pageLabel(page ?? this.saved()[viewId] ?? null, page !== undefined);
  }

  /** What a saved tab whose page is not open can offer: its last address. */
  savedPage(viewId: string): SavedPage | null {
    return this.saved()[viewId] ?? null;
  }

  /** Opens a page, a local HTML document or a page an agent opened, in a tab of its own. */
  async open(target: BrowserTarget): Promise<string | null> {
    try {
      const info = 'page' in target ? await browserApi.info(target.page) : await browserApi.open(target);
      const id = browserViewId(info.id);
      this.pages.set(id, info);
      if ('file' in target) this.files.set(id, target.file);
      this.remember(id);
      this.app.layout.open({ id, resource: { kind: 'browser', resourceId: id }, hidden: false }, other => other.resource.kind === 'browser');
      this.changed();
      return id;
    } catch (error) { this.app.ui.error(error); return null; }
  }

  /** A saved tab whose page is closed: opens that page again in a new tab, and the closed one goes. */
  async reopen(viewId: string): Promise<string | null> {
    const saved = this.saved()[viewId];
    if (!saved) return null;
    const fresh = await this.open(reopenTarget(saved));
    if (fresh && fresh !== viewId) this.forget(viewId);
    return fresh;
  }

  /** Closes the tab and the page. */
  async close(viewId: string): Promise<void> {
    const page = this.pages.get(viewId);
    this.forget(viewId);
    if (page) await browserApi.close(page.id).catch(() => {});
  }

  // ---- what a tab on screen does ------------------------------------------------------------------------------------------

  /**
   * A tab is drawn: its frames go to `sink` from now on, and while it is in sight its page is read again every `POLL_MS`. Returns what
   * ends it. Rust keeps the last sink it was given, so a tab drawn again takes the frames over.
   */
  connect(viewId: string, sink: (bytes: ArrayBuffer) => void): () => void {
    const page = this.pages.get(viewId);
    if (!page) return () => {};
    const connection: Connection = this.connections.get(viewId) ?? { queue: [], sending: false, visible: false, sink: null, poll: null };
    this.connections.set(viewId, connection);
    connection.sink = sink;
    void browserApi.frames(page.id, bytes => connection.sink?.(bytes)).catch(error => this.app.ui.error(error));
    this.polling(viewId, connection);
    return () => {
      if (connection.sink === sink) connection.sink = null;
    };
  }

  /** Whether the tab is the one in sight: Rust stops sending frames of a page nobody sees. */
  setVisible(viewId: string, visible: boolean): void {
    const page = this.pages.get(viewId);
    const connection = this.connections.get(viewId);
    if (!page || !connection || connection.visible === visible) return;
    connection.visible = visible;
    void browserApi.visibility(page.id, visible).catch(() => {});
    this.polling(viewId, connection);
    if (visible) void this.refresh(viewId);
  }

  /** The surface has a size (in CSS pixels, with the device's ratio): the page is laid out at it. */
  resize(viewId: string, size: { width: number; height: number; dpr: number }): void {
    const page = this.pages.get(viewId);
    if (!page || size.width < 1 || size.height < 1) return;
    void browserApi.resize(page.id, Math.round(size.width), Math.round(size.height), size.dpr).catch(() => {});
  }

  /** What the person did on the page: sent in batches, the moves of a burst as the last of them, one call at a time. */
  input(viewId: string, event: BrowserInputEvent): void {
    const connection = this.connections.get(viewId);
    if (!connection) return;
    connection.queue.push(event);
    this.schedule(() => { void this.flush(viewId); });
  }

  /** Reads the page again: its address, its title and who drives it. A page that closed says so on the next action. */
  async refresh(viewId: string): Promise<void> {
    const page = this.pages.get(viewId);
    if (!page) return;
    try {
      const next = await browserApi.info(page.id);
      if (this.pages.get(viewId)?.id !== page.id) return;
      this.pages.set(viewId, next);
      this.remember(viewId);
      this.changed();
    } catch { /* the page closed */ }
  }

  /** Back, forward, reload. */
  async navigate(viewId: string, action: Exclude<NavigateAction, 'go'>): Promise<void> {
    await this.step(viewId, action);
  }

  /** What the person typed in the address field. Returns whether it was an address. */
  async go(viewId: string, typed: string): Promise<boolean> {
    const url = addressUrl(typed);
    if (!url) return false;
    await this.step(viewId, 'go', url);
    return true;
  }

  /** Takes the page from the agent that drives it, or gives it back. */
  async control(viewId: string, take: boolean): Promise<void> {
    const page = this.pages.get(viewId);
    if (!page) return;
    try {
      this.pages.set(viewId, await browserApi.control(page.id, take));
      this.remember(viewId);
      this.changed();
    } catch (error) { this.app.ui.error(error); }
  }

  /** Opens the page's address in the system's browser. */
  async external(viewId: string): Promise<void> {
    const page = this.pages.get(viewId);
    if (!page) return;
    try { await browserApi.external(page.id); } catch (error) { this.app.ui.error(error); }
  }

  // ---- inside -------------------------------------------------------------------------------------------------------------

  private async step(viewId: string, action: NavigateAction, url?: string): Promise<void> {
    const page = this.pages.get(viewId);
    if (!page) return;
    try {
      await browserApi.navigate(page.id, action, url);
      await this.refresh(viewId);
    } catch (error) { this.app.ui.error(error); }
  }

  private async flush(viewId: string): Promise<void> {
    const connection = this.connections.get(viewId);
    const page = this.pages.get(viewId);
    if (!connection || !page || connection.sending || !connection.queue.length) return;
    connection.sending = true;
    const events = coalesce(connection.queue.splice(0));
    try { await browserApi.input(page.id, events); }
    catch (error) { this.app.ui.error(error); }
    finally {
      connection.sending = false;
      if (connection.queue.length) void this.flush(viewId);
    }
  }

  private polling(viewId: string, connection: Connection): void {
    if (connection.poll !== null) { clearInterval(connection.poll); connection.poll = null; }
    if (connection.visible) connection.poll = setInterval(() => { void this.refresh(viewId); }, POLL_MS);
  }

  private forget(viewId: string): void {
    const connection = this.connections.get(viewId);
    if (connection?.poll != null) clearInterval(connection.poll);
    this.connections.delete(viewId);
    this.pages.delete(viewId);
    this.files.delete(viewId);
    this.unremember(viewId);
    this.app.layout.close(viewId);
    this.changed();
  }

  // The pages that were open, kept so that a tab saved in the layout can offer its page again after a restart. A convenience only: a
  // storage that cannot be read or written keeps nothing and the tab says it is closed.
  private saved(): Record<string, SavedPage> {
    try { return parseSaved(localStorage.getItem(SAVED_KEY)); } catch { return {}; }
  }

  private remember(viewId: string): void {
    const page = this.pages.get(viewId);
    if (!page) return;
    const file = this.files.get(viewId);
    const saved = this.saved();
    saved[viewId] = { url: page.url, title: page.title, ...(file ? { file } : {}) };
    try { localStorage.setItem(SAVED_KEY, JSON.stringify(saved)); } catch { /* a convenience only */ }
  }

  /** A tab that is closed is forgotten, so that a restart does not offer it again. */
  private unremember(viewId: string): void {
    const saved = this.saved();
    delete saved[viewId];
    try { localStorage.setItem(SAVED_KEY, JSON.stringify(saved)); } catch { /* a convenience only */ }
  }
}
