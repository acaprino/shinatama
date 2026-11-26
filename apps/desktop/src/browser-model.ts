// The pure part of the browser tab (ADR-0056): what Rust says about a page, the strip that tells who drives it, what an address field
// means, how a tab is named, and the pages that were open (kept to offer them again). No React, no Tauri, no DOM.
export type { BrowserInputEvent } from './browser-input.ts';

/** What Rust says about a page. */
export interface PageInfo { readonly id: string; readonly url: string; readonly title: string; readonly controller: string; readonly live: boolean; readonly openedBy: string | null }
/** What `browser_navigate` does: a step in the history, a reload, or a new address. */
export type NavigateAction = 'back' | 'forward' | 'reload' | 'go';
/** What opens a page: an address, a local document (served from its folder, so it reopens from the file), or a page Rust already opened. */
export type BrowserTarget = { readonly url: string } | { readonly file: string } | { readonly page: string };

export const browserViewId = (pageId: string): string => `browser-${pageId}`;

export const BROWSER_WORDS = {
  agentUsing: "L'agente sta usando questa pagina",
  takeControl: 'Prendi il controllo',
  userHolds: "Hai il controllo della pagina dell'agente",
  giveBack: 'Restituisci',
  reopen: 'Riapri',
  closed: 'Questa pagina è chiusa.',
} as const;

/** What the control strip says for a page, or null when it is hidden. */
export function controlStrip(page: Pick<PageInfo, 'controller' | 'openedBy'>): { readonly text: string; readonly action: 'take' | 'give' } | null {
  if (page.controller.startsWith('agent:')) return { text: BROWSER_WORDS.agentUsing, action: 'take' };
  if (page.openedBy) return { text: BROWSER_WORDS.userHolds, action: 'give' };
  return null;
}

/** What the person typed in the address field, as an address: one with no scheme is https. Empty is nothing. */
export function addressUrl(typed: string): string | null {
  const text = typed.trim();
  if (!text) return null;
  return /^[a-z][a-z0-9+.-]*:/i.test(text) ? text : `https://${text}`;
}

/** The page as it was when the tab was last open: enough to offer it again. */
export interface SavedPage { readonly url: string; readonly title: string; readonly file?: string }

/** The name of a tab: the title, else the address without its scheme; "closed" for a page that is not open. */
export function pageLabel(page: Pick<PageInfo, 'url' | 'title'> | SavedPage | null, open: boolean): string {
  if (!page) return 'Browser';
  const title = page.title || page.url.replace(/^https?:\/\//, '').replace(/\/$/, '');
  return `${title || 'Browser'}${open ? '' : ' · closed'}`.slice(0, 80);
}

/** The saved pages as storage kept them: anything that is not a page is left out. */
export function parseSaved(text: string | null): Record<string, SavedPage> {
  try {
    const value: unknown = JSON.parse(text ?? '{}');
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
    const saved: Record<string, SavedPage> = {};
    for (const [view, page] of Object.entries(value)) {
      if (typeof page !== 'object' || page === null) continue;
      const { url, title, file } = page as Record<string, unknown>;
      if (typeof url !== 'string') continue;
      saved[view] = { url, title: typeof title === 'string' ? title : '', ...(typeof file === 'string' ? { file } : {}) };
    }
    return saved;
  } catch { return {}; }
}

/** The target that opens a saved page again. */
export const reopenTarget = (page: SavedPage): BrowserTarget => (page.file ? { file: page.file } : { url: page.url });
