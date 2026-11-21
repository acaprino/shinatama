import { componentElement, inputHtml, uiButton, uiPart } from '@acaprino/imago/ui';
// Browser tabs (ADR-0056): each page is a workspace view drawn from the Moli
// host's JPEG frames on a canvas. Page content never becomes DOM here; the
// address bar shows the page's real URL. Tabs tile like terminal tabs.
import { coalesce, keyEvent, mouseEvent, settle, wheelEvent, type BrowserInputEvent } from './browser-input.ts';
import { confirmsField } from './desktop-shell-model.ts';

export interface PageInfo { readonly id: string; readonly url: string; readonly title: string; readonly controller: string; readonly live: boolean; readonly openedBy: string | null }

/** What the control strip says for a page, or null when it is hidden. */
export function controlStrip(page: Pick<PageInfo, 'controller' | 'openedBy'>): { readonly text: string; readonly action: 'take' | 'give' } | null {
  if (page.controller.startsWith('agent:')) return { text: "L'agente sta usando questa pagina", action: 'take' };
  if (page.openedBy) return { text: "Hai il controllo della pagina dell'agente", action: 'give' };
  return null;
}
/** `page`: a page the host already opened (an agent's), shown in a new tab. */
export type BrowserTarget = { readonly url: string } | { readonly file: string } | { readonly page: string };

export interface BrowserTabsDeps {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  /** A Tauri channel whose messages are the raw frame bytes. */
  channel(onFrame: (bytes: ArrayBuffer) => void): unknown;
  notify(message: string): void;
}

interface Tab {
  readonly viewId: string;
  readonly element: HTMLElement;
  readonly canvas: HTMLCanvasElement;
  readonly address: HTMLInputElement;
  readonly strip: HTMLElement;
  page: PageInfo | null;
  /** The local document a tab was opened on: served from its folder, so it reopens from the file. */
  file: string | null;
  visible: boolean;
  queue: BrowserInputEvent[];
  sending: boolean;
  poll: number;
  dispose(): void;
}

const STORE = 'shinatama.browser.tabs';
type Saved = Record<string, { url: string; title: string; file?: string }>;
function readSaved(): Saved { try { return JSON.parse(localStorage.getItem(STORE) ?? '{}') as Saved; } catch { return {}; } }
function writeSaved(saved: Saved) { try { localStorage.setItem(STORE, JSON.stringify(saved)); } catch { /* a convenience only */ } }

function button(label: string, title: string, run: () => void): HTMLButtonElement {
  const b = uiButton(label, { sm: true, attrs: { title, 'aria-label': title } });
  b.addEventListener('click', run);
  return b;
}

export function newBrowserViewId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  return `browser-${[...bytes].map(b => b.toString(16).padStart(2, '0')).join('')}`;
}

export function createBrowserTabs(deps: BrowserTabsDeps) {
  const tabs = new Map<string, Tab>();

  function remember(tab: Tab) {
    if (!tab.page) return;
    const saved = readSaved(); saved[tab.viewId] = { url: tab.page.url, title: tab.page.title, ...(tab.file ? { file: tab.file } : {}) }; writeSaved(saved);
  }
  function forget(viewId: string) { const saved = readSaved(); delete saved[viewId]; writeSaved(saved); }

  function paintInfo(tab: Tab) {
    const page = tab.page; if (!page) return;
    if (document.activeElement !== tab.address) tab.address.value = page.url;
    const agent = page.controller.startsWith('agent:');
    const strip = controlStrip(page);
    tab.strip.hidden = !strip;
    if (strip) {
      tab.strip.querySelector('.browser-strip-text')!.textContent = strip.text;
      tab.strip.querySelector<HTMLButtonElement>('[data-control="take"]')!.hidden = strip.action !== 'take';
      tab.strip.querySelector<HTMLButtonElement>('[data-control="give"]')!.hidden = strip.action !== 'give';
    }
    tab.element.dataset.controller = agent ? 'agent' : 'user';
    tab.canvas.style.cursor = agent ? 'not-allowed' : '';
    remember(tab);
  }

  async function refresh(tab: Tab) {
    if (!tab.page) return;
    try { tab.page = await deps.invoke<PageInfo>('browser_info', { page: tab.page.id }); paintInfo(tab); } catch { /* the page closed; its tab says so on the next action */ }
  }

  async function flush(tab: Tab) {
    if (tab.sending || !tab.page || !tab.queue.length) return;
    tab.sending = true;
    const events = coalesce(tab.queue.splice(0));
    try { await deps.invoke('browser_input', { page: tab.page.id, events }); }
    catch (error) { deps.notify(String(error)); }
    finally { tab.sending = false; if (tab.queue.length) void flush(tab); }
  }
  function send(tab: Tab, event: BrowserInputEvent) { tab.queue.push(event); requestAnimationFrame(() => { void flush(tab); }); }

  function draw(tab: Tab, bytes: ArrayBuffer) {
    void createImageBitmap(new Blob([bytes], { type: 'image/jpeg' })).then(bitmap => {
      const context = tab.canvas.getContext('2d'); if (!context) { bitmap.close(); return; }
      context.drawImage(bitmap, 0, 0, tab.canvas.width, tab.canvas.height); bitmap.close();
    }).catch(() => { /* a broken frame is skipped; the next one repaints */ });
  }

  function build(viewId: string): Tab {
    const element = uiPart('stack'); element.classList.add('browser-tab', 'im-render-view'); element.dataset.viewId = viewId;
    const bar = uiPart('toolbar'); bar.classList.add('browser-bar');
    bar.setAttribute('role', 'group'); bar.setAttribute('aria-label', 'Browser navigation');
    const address = componentElement<HTMLInputElement>(inputHtml({ name: 'address', label: 'Address', value: '',
      attrs: { class: 'im-input browser-address', spellcheck: 'false' } }));
    const strip = uiPart('row'); strip.classList.add('browser-strip'); strip.hidden = true;
    const area = uiPart('viewport'); area.classList.add('browser-page');
    const canvas = document.createElement('canvas'); canvas.className = 'browser-canvas'; canvas.tabIndex = 0; canvas.setAttribute('aria-label', 'Web page');
    area.append(canvas);
    const tab: Tab = { viewId, element, canvas, address, strip, page: null, file: null, visible: false, queue: [], sending: false, poll: 0, dispose: () => {} };
    const nav = (action: string, url?: string) => { if (tab.page) void deps.invoke('browser_navigate', { page: tab.page.id, action, url }).then(() => refresh(tab), error => deps.notify(String(error))); };
    bar.append(
      button('←', 'Back', () => nav('back')),
      button('→', 'Forward', () => nav('forward')),
      button('⟳', 'Reload', () => nav('reload')),
      address,
      button('↗', 'Open in the system browser', () => { if (tab.page) void deps.invoke('browser_external', { page: tab.page.id }).catch(error => deps.notify(String(error))); }),
    );
    address.addEventListener('keydown', event => {
      if (!confirmsField(event)) return;
      let url = address.value.trim(); if (!url) return;
      if (!/^[a-z][a-z0-9+.-]*:/i.test(url)) url = `https://${url}`;
      nav('go', url); canvas.focus();
    });
    const control = (take: boolean) => {
      if (tab.page) void deps.invoke<PageInfo>('browser_control', { page: tab.page.id, take }).then(info => { tab.page = info; paintInfo(tab); }, error => deps.notify(String(error)));
    };
    const take = button('Prendi il controllo', 'Take control of this page from the agent', () => control(true)); take.dataset.control = 'take';
    const give = button('Restituisci', 'Hand this page back to the agent', () => control(false)); give.dataset.control = 'give';
    const stripText = uiPart('text'); stripText.classList.add('browser-strip-text');
    strip.append(stripText, take, give);
    element.append(bar, strip, area);

    const rect = () => canvas.getBoundingClientRect();
    canvas.addEventListener('pointerdown', event => { canvas.focus(); canvas.setPointerCapture(event.pointerId); send(tab, mouseEvent('pressed', event, rect())); event.preventDefault(); });
    canvas.addEventListener('pointerup', event => { send(tab, mouseEvent('released', event, rect())); });
    canvas.addEventListener('pointermove', event => { send(tab, mouseEvent('moved', event, rect())); });
    canvas.addEventListener('wheel', event => { send(tab, wheelEvent(event, rect())); event.preventDefault(); }, { passive: false });
    canvas.addEventListener('contextmenu', event => event.preventDefault());
    canvas.addEventListener('keydown', event => { if (event.key === 'Tab' && event.ctrlKey) return; send(tab, keyEvent('down', event)); event.preventDefault(); });
    canvas.addEventListener('keyup', event => { send(tab, keyEvent('up', event)); event.preventDefault(); });
    canvas.addEventListener('paste', event => { const text = event.clipboardData?.getData('text/plain'); if (text) send(tab, { type: 'text', text }); event.preventDefault(); });

    const resize = settle<{ width: number; height: number }>(150, size => {
      if (!tab.page || size.width < 1 || size.height < 1) return;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(size.width * dpr); canvas.height = Math.round(size.height * dpr);
      void deps.invoke('browser_resize', { page: tab.page.id, width: Math.round(size.width), height: Math.round(size.height), dpr }).catch(() => {});
    });
    const observer = new ResizeObserver(entries => { for (const entry of entries) resize({ width: entry.contentRect.width, height: entry.contentRect.height }); });
    observer.observe(area);
    tab.dispose = () => { observer.disconnect(); clearInterval(tab.poll); };
    return tab;
  }

  async function attach(tab: Tab, target: BrowserTarget) {
    tab.file = 'file' in target ? target.file : null;
    tab.page = 'page' in target ? await deps.invoke<PageInfo>('browser_info', { page: target.page })
      : await deps.invoke<PageInfo>('browser_open', 'url' in target ? { url: target.url } : { file: target.file });
    paintInfo(tab);
    await deps.invoke('browser_frames', { page: tab.page.id, channel: deps.channel(bytes => draw(tab, bytes)) });
    const box = tab.canvas.parentElement?.getBoundingClientRect();
    if (box && box.width > 0) {
      const dpr = window.devicePixelRatio || 1;
      tab.canvas.width = Math.round(box.width * dpr); tab.canvas.height = Math.round(box.height * dpr);
      await deps.invoke('browser_resize', { page: tab.page.id, width: Math.round(box.width), height: Math.round(box.height), dpr });
    }
    if (tab.visible) await deps.invoke('browser_visibility', { page: tab.page.id, visible: true });
    tab.poll = window.setInterval(() => { if (tab.visible) void refresh(tab); }, 1500);
  }

  return {
    has: (viewId: string) => tabs.has(viewId),
    /** The view showing a host page, if any. */
    viewOf: (pageId: string) => [...tabs.values()].find(tab => tab.page?.id === pageId)?.viewId ?? null,
    element: (viewId: string) => tabs.get(viewId)?.element ?? null,

    /** Opens a page in a new tab element; the caller places the view. */
    async open(target: BrowserTarget, viewId = newBrowserViewId()): Promise<{ viewId: string; element: HTMLElement }> {
      const tab = build(viewId); tabs.set(viewId, tab);
      try { await attach(tab, target); }
      catch (error) {
        // A page the host opened before a later step failed must not stay open in Moli.
        if (tab.page) await deps.invoke('browser_close', { page: tab.page.id }).catch(() => undefined);
        tab.dispose(); tabs.delete(viewId); throw error;
      }
      return { viewId, element: tab.element };
    },

    label(viewId: string): string {
      const page = tabs.get(viewId)?.page ?? readSaved()[viewId] ?? null;
      if (!page) return 'Browser';
      const title = page.title || page.url.replace(/^https?:\/\//, '').replace(/\/$/, '');
      return `${title || 'Browser'}${tabs.has(viewId) ? '' : ' · closed'}`.slice(0, 80);
    },

    setVisible(viewId: string, visible: boolean) {
      const tab = tabs.get(viewId); if (!tab || tab.visible === visible) return;
      tab.visible = visible;
      if (tab.page) void deps.invoke('browser_visibility', { page: tab.page.id, visible }).catch(() => {});
    },

    focus(viewId: string) { tabs.get(viewId)?.canvas.focus(); },

    async close(viewId: string) {
      const tab = tabs.get(viewId); forget(viewId);
      if (!tab) return;
      tab.dispose(); tabs.delete(viewId);
      if (tab.page) await deps.invoke('browser_close', { page: tab.page.id }).catch(() => {});
    },

    /** A saved tab whose page is not open: its last address and a button to open it again. */
    placeholder(viewId: string, reopen: (target: BrowserTarget) => void): HTMLElement | null {
      if (tabs.has(viewId)) return null;
      const saved = readSaved()[viewId];
      const card = uiPart('stack'); card.classList.add('browser-closed');
      card.append(uiPart('paragraph', saved?.url ?? 'This page is closed.'));
      if (saved?.url) card.append(button('Riapri', 'Open this page again', () => reopen(saved.file ? { file: saved.file } : { url: saved.url })));
      return card;
    },
  };
}
export type BrowserTabs = ReturnType<typeof createBrowserTabs>;
