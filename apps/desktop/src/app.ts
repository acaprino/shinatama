// Shinatama: the controllers of the window, wired to each other, and the start of the window.
//
// A controller is given the `Shinatama` and reaches the others through it. The order of the fields is the order they start in. See
// store.ts for what a controller is and how a screen reads it; docs/desktop-ui.md for who owns which file.
import { inTauri, listen, type Unlisten } from './api/core.ts';
import { appLog } from './api/core.ts';
import { BrowserController } from './controllers/browser.ts';
import { CatalogController } from './controllers/catalog.ts';
import { ChatController } from './controllers/chat.ts';
import { FileTabsController } from './controllers/file-tabs.ts';
import { FilesController } from './controllers/files.ts';
import { HopController } from './controllers/hop.ts';
import { LayoutController } from './controllers/layout.ts';
import { LeftoversController } from './controllers/leftovers.ts';
import { OpensController } from './controllers/opens.ts';
import { OverviewController } from './controllers/overview.ts';
import { ProjectsController } from './controllers/projects.ts';
import { ServicesController } from './controllers/services.ts';
import { SessionsController } from './controllers/sessions.ts';
import { SettingsController } from './controllers/settings.ts';
import { TeamController } from './controllers/team.ts';
import { TerminalController } from './controllers/terminals.ts';
import { ThreadSettingsController } from './controllers/thread-settings.ts';
import { ThreadsController } from './controllers/threads.ts';
import { UiController } from './controllers/ui.ts';
import type { Store } from './store.ts';

export class Shinatama {
  readonly ui = new UiController();
  readonly settings = new SettingsController(this);
  readonly services = new ServicesController(this);
  readonly projects = new ProjectsController(this);
  readonly catalog = new CatalogController(this);
  readonly overview = new OverviewController(this);
  readonly layout = new LayoutController(this);
  readonly terminals = new TerminalController(this);
  readonly threads = new ThreadsController(this);
  readonly chat = new ChatController(this);
  readonly opens = new OpensController(this);
  readonly hop = new HopController(this);
  readonly sessions = new SessionsController(this);
  readonly files = new FilesController(this);
  readonly fileTabs = new FileTabsController(this);
  readonly leftovers = new LeftoversController(this);
  readonly threadSettings = new ThreadSettingsController(this);
  readonly team = new TeamController(this);
  readonly browser = new BrowserController(this);

  private readonly started: Store[] = [];
  private readonly stops: Unlisten[] = [];

  /** The controllers that listen to something, in the order they start. */
  private get listening(): Store[] {
    return [this.settings, this.services, this.layout, this.terminals, this.chat, this.opens, this.browser, this.leftovers, this.sessions, this.files];
  }

  /** The start of the window, once its first frame is drawn: what the page reads from Rust, the terminals that survived a reload of the
   *  page, the threads that were on, the saved layout and the sessions it held as running. Nothing here starts a process by itself
   *  except those sessions, in the background, at their turn. */
  async start(): Promise<void> {
    const { ui, opens, threads, projects, terminals, layout, settings } = this;
    if (!inTauri && !import.meta.env?.DEV) {
      ui.setRuntime({ text: 'Browser preview · terminals require the desktop app', failed: true });
      return;
    }
    try {
      await opens.loadHarnessCatalog();
      for (const controller of this.listening) { this.started.push(controller); await controller.start(); }
      this.stops.push(await listen('thread-changed', () => {
        // The backend moved or saved a session: the thread sidebar and the tabs read the saved records again.
        void threads.refreshMembers();
        for (const project of threads.activeProjectIds()) void threads.refreshList(project);
      }));
      // An alert of the attention widget names a session: its tab is brought forward.
      this.stops.push(await listen<{ sessionId: string | null }>('attention-focus', ({ sessionId }) => { if (sessionId) void threads.focusTab(sessionId); }));
      // The window closes: terminals and agents stop, after asking.
      this.stops.push(await listen('confirm-close', async () => {
        if ((terminals.sessions.size || ui.modalOpen()) && !await ui.ask({
          title: 'Close workspace?', description: 'All running terminals and their child programs will stop.', confirm: 'Close workspace',
        })) return;
        try { await terminals.closeWorkspace(false); } catch (error) { ui.error(error); }
      }));
      // A development reload of the page reattaches the processes that exist; it never starts a new one.
      const reattached = await terminals.reattach(threads.knownSessions());
      if (reattached) ui.info('Reconnected to running terminals. Earlier screen contents are not retained across a frontend reload.');
      ui.setRuntime({ text: 'Local runtime · experimental', failed: false });
      appLog('page', 'The main window loaded.');
      await settings.readStorage();
      // The sidebar's saved list is read first: the projects it lists beyond the ten recent are named from it.
      await threads.loadSidebar();
      await projects.restore();
      await projects.refreshRecent();
      for (const project of threads.activeProjectIds()) void threads.refreshList(project);
      // Turn on the threads that were on, before the saved layout loads, so their tabs keep their panes.
      await threads.reopenLast();
      await layout.connect();
      // The saved panes are back, and with them the sessions they held as running.
      opens.reopenRestored();
    } catch (error) {
      ui.setRuntime({ text: 'Runtime unavailable', failed: true });
      ui.error(error);
    }
  }

  dispose(): void {
    for (const stop of this.stops.splice(0)) stop();
    for (const controller of this.started.splice(0).reverse()) controller.dispose();
  }
}
