// The pages of the browser tab (ADR-0056): a page Rust drives through the browser engine, which the person watches and takes over,
// and which an agent's browser tools may open on its own. Each page is a tab of a pane (`browser-<page>`, kind `browser`), one pane
// per window at first. The frames of a page and what the person types into it go through `browserApi`, from the screen of the tab.
import { listen, type Unlisten } from '../api/core.ts';
import { browserApi } from '../api/files.ts';
import type { Shinatama } from '../app.ts';
import { Store } from '../store.ts';

/** What Rust says about a page. */
export interface PageInfo { readonly id: string; readonly url: string; readonly title: string; readonly controller: string; readonly live: boolean; readonly openedBy: string | null }
export type BrowserTarget = { url: string } | { file: string } | { page: string };

export const browserViewId = (pageId: string): string => `browser-${pageId}`;

export class BrowserController extends Store {
  /** The pages that are open, by their view id. */
  readonly pages = new Map<string, PageInfo>();
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
  }

  viewOf(pageId: string): string | null {
    return [...this.pages].find(([, page]) => page.id === pageId)?.[0] ?? null;
  }

  /** Opens a page, a local HTML document or a page an agent opened, in a tab of its own. */
  async open(target: BrowserTarget): Promise<string | null> {
    try {
      const info = ('page' in target ? await browserApi.info(target.page) : await browserApi.open(target)) as PageInfo;
      const id = browserViewId(info.id);
      this.pages.set(id, info);
      this.app.layout.open({ id, resource: { kind: 'browser', resourceId: id }, hidden: false }, other => other.resource.kind === 'browser');
      this.changed();
      return id;
    } catch (error) { this.app.ui.error(error); return null; }
  }

  /** Closes the tab and the page. */
  async close(viewId: string): Promise<void> {
    const page = this.pages.get(viewId);
    this.forget(viewId);
    if (page) await browserApi.close(page.id).catch(() => {});
  }

  private forget(viewId: string): void {
    this.pages.delete(viewId);
    this.app.layout.close(viewId);
    this.changed();
  }
}
