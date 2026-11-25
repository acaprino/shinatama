import { uiPart, uiButton } from '@acaprino/imago/ui';
import { SHOW_AFTER_MS, acceptsEvent, describeProgress, type ProgressEvent } from './session-progress-core.ts';

const TICK_MS = 1000;

export type { ProgressEvent };
export type ProgressBackend = {
  subscribe: (handler: (event: ProgressEvent) => void) => () => void;
  cancel: (requestId: string) => Promise<unknown>;
};
export type ProgressLine = {
  /** Called when the work starts; returns the id the backend tags its events and its cancel with. */
  begin: () => string;
  /** Called when the work `requestId` ends, however it ends. A request already replaced by a newer one is left alone. */
  end: (requestId: string) => void;
  /** Stops the running work (the Cancel control, or the surface closing). */
  cancel: () => void;
  /** Whether the work that just ended was stopped by `cancel`. Valid until the next `begin`. */
  cancelled: () => boolean;
  element: HTMLElement;
  destroy: () => void;
};

function newId(): string {
  const random = globalThis.crypto?.randomUUID?.() ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
  return `r-${random}`.slice(0, 64);
}

/**
 * The inline progress line of a long search or export: hidden until the worker has reported and
 * `SHOW_AFTER_MS` have passed, then "Exporting… 12,400 entries · 38 MB written · 14 s" with a Cancel
 * control. It lives inside the surface that asked for the work, never in a popup. An event of any
 * other request (a closed or replaced one) is ignored.
 */
export function createProgressLine(kind: 'export' | 'search', backend?: ProgressBackend, now: () => number = () => performance.now()): ProgressLine {
  const element = uiPart('status');
  element.hidden = true; element.dataset.kind = kind; element.setAttribute('role', 'status');
  const text = uiPart('text');
  const stop = uiButton('Cancel');
  element.append(text, stop);
  let id: string | null = null; let startedAt: number | null = null; let last: ProgressEvent | null = null;
  let showTimer: ReturnType<typeof setTimeout> | null = null; let tick: ReturnType<typeof setInterval> | null = null;
  let stopped = false; let visible = false;
  const paint = () => {
    if (!visible || startedAt === null) return;
    text.textContent = stopped ? 'Cancelling…' : describeProgress(kind, last, now() - startedAt);
    stop.disabled = stopped;
  };
  const reveal = () => { showTimer = null; if (id === null) return; visible = true; element.hidden = false; paint(); };
  const clear = () => {
    if (showTimer !== null) clearTimeout(showTimer);
    if (tick !== null) clearInterval(tick);
    showTimer = null; tick = null; visible = false; element.hidden = true; id = null; startedAt = null; last = null;
  };
  const unsubscribe = backend?.subscribe(event => {
    if (!acceptsEvent(id, kind, event)) return;
    last = event;
    if (startedAt === null) {
      startedAt = now();
      showTimer = setTimeout(reveal, SHOW_AFTER_MS);
      tick = setInterval(paint, TICK_MS);
    }
    paint();
  });
  const cancel = () => {
    if (id === null || stopped) return;
    stopped = true; paint();
    void backend?.cancel(id).catch(() => {});
  };
  stop.onclick = cancel;
  return {
    element,
    begin() { clear(); stopped = false; id = newId(); return id; },
    end(requestId) { if (id === requestId) clear(); },
    cancel,
    cancelled: () => stopped,
    destroy() { clear(); unsubscribe?.(); element.remove(); },
  };
}
