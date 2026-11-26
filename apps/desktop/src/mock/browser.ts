// The answers of the browser for the browser tab (controllers/browser.ts): pages that live in this file, with a history, a controller
// (the person or an agent) and the input they were sent. A test makes an agent's page with `openAgentPage` and says
// `emit('browser-agent-page', { page, instance })`, as Rust does (smoke: tests/browser.smoke.mjs).
import type { PageInfo } from '../browser-model.ts';
import type { Answers } from './index.ts';

interface MockPage { info: PageInfo; history: string[]; at: number; channel: { onmessage?: (bytes: ArrayBuffer) => void } | null }

export const state: {
  pages: Map<string, MockPage>;
  input: unknown[][];
  sizes: { width: number; height: number; dpr: number }[];
  visibility: boolean[];
  external: string[];
  closed: string[];
  next: number;
} = { pages: new Map(), input: [], sizes: [], visibility: [], external: [], closed: [], next: 1 };

const titleOf = (url: string): string => url.replace(/^https?:\/\//, '').replace(/\/$/, '').split('/')[0] ?? url;

function create(url: string, extra: Partial<PageInfo> = {}): MockPage {
  const id = `p${state.next++}`;
  const page: MockPage = { info: { id, url, title: titleOf(url), controller: 'user', live: true, openedBy: null, ...extra }, history: [url], at: 0, channel: null };
  state.pages.set(id, page);
  return page;
}

/** A page an agent opened through its browser tools: it drives it until the person takes it. */
export function openAgentPage(url: string): string {
  return create(url, { controller: 'agent:i_aaaaaaaaaaaa', openedBy: 'i_aaaaaaaaaaaa' }).info.id;
}

/** Sends one frame to the tab that listens (bytes of an image). */
export function pushFrame(pageId: string, bytes: ArrayBuffer): void {
  state.pages.get(pageId)?.channel?.onmessage?.(bytes);
}

const page = (args: Record<string, unknown> | undefined): MockPage => {
  const found = state.pages.get(String(args?.page));
  if (!found) throw 'The page closed.';
  return found;
};

export const answers: Answers = {
  browser_open: args => {
    const target = args as { url?: string; file?: string } | undefined;
    return create(target?.url ?? `http://127.0.0.1:9000/${String(target?.file ?? '').split(/[\\/]/).pop()}`).info;
  },
  browser_info: args => page(args).info,
  browser_frames: args => { page(args).channel = args?.channel as MockPage['channel']; return null; },
  browser_navigate: args => {
    const one = page(args);
    const action = String(args?.action);
    if (action === 'back') one.at = Math.max(0, one.at - 1);
    else if (action === 'forward') one.at = Math.min(one.history.length - 1, one.at + 1);
    else if (action === 'go') { one.history = [...one.history.slice(0, one.at + 1), String(args?.url)]; one.at = one.history.length - 1; }
    const url = one.history[one.at]!;
    one.info = { ...one.info, url, title: titleOf(url) };
    return null;
  },
  browser_input: args => {
    const one = page(args);
    state.input.push(args?.events as unknown[]);
    // The person acting on an agent's page takes control of it.
    if (one.info.controller !== 'user') one.info = { ...one.info, controller: 'user' };
    return null;
  },
  browser_resize: args => { page(args); state.sizes.push({ width: Number(args?.width), height: Number(args?.height), dpr: Number(args?.dpr) }); return null; },
  browser_visibility: args => { page(args); state.visibility.push(args?.visible === true); return null; },
  browser_control: args => {
    const one = page(args);
    one.info = { ...one.info, controller: args?.take === true ? 'user' : `agent:${one.info.openedBy ?? 'i_aaaaaaaaaaaa'}` };
    return one.info;
  },
  browser_close: args => { const id = String(args?.page); state.pages.delete(id); state.closed.push(id); return null; },
  browser_external: args => { state.external.push(page(args).info.url); return null; },
};
