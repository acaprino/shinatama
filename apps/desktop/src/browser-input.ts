// DOM events of a browser tab's canvas, turned into the input events the Moli
// host takes (crates/browser/src/input.rs). Pure: no DOM access here.

export type BrowserInputEvent =
  | { type: 'mouse'; kind: 'pressed' | 'released' | 'moved'; x: number; y: number; button: string; buttons: number; modifiers: number; clickCount: number }
  | { type: 'wheel'; x: number; y: number; dx: number; dy: number; modifiers: number }
  | { type: 'key'; kind: 'down' | 'up'; key: string; code: string; text: string | null; modifiers: number; keyCode: number }
  | { type: 'text'; text: string };

export interface PointerLike { readonly clientX: number; readonly clientY: number; readonly button: number; readonly buttons: number; readonly detail: number; readonly altKey: boolean; readonly ctrlKey: boolean; readonly metaKey: boolean; readonly shiftKey: boolean }
export interface WheelLike extends PointerLike { readonly deltaX: number; readonly deltaY: number; readonly deltaMode: number }
export interface KeyLike { readonly key: string; readonly code: string; readonly keyCode: number; readonly altKey: boolean; readonly ctrlKey: boolean; readonly metaKey: boolean; readonly shiftKey: boolean }
export interface RectLike { readonly left: number; readonly top: number }

/** CDP modifier bits: Alt 1, Ctrl 2, Meta 4, Shift 8. */
export function modifiers(event: { altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }): number {
  return (event.altKey ? 1 : 0) | (event.ctrlKey ? 2 : 0) | (event.metaKey ? 4 : 0) | (event.shiftKey ? 8 : 0);
}
const BUTTONS = ['left', 'middle', 'right', 'back', 'forward'];

/** Page coordinates are CSS pixels of the page viewport, which is the canvas box. */
export function mouseEvent(kind: 'pressed' | 'released' | 'moved', event: PointerLike, rect: RectLike): BrowserInputEvent {
  return {
    type: 'mouse', kind, x: event.clientX - rect.left, y: event.clientY - rect.top,
    button: kind === 'moved' ? (event.buttons & 1 ? 'left' : 'none') : BUTTONS[event.button] ?? 'none',
    buttons: event.buttons, modifiers: modifiers(event), clickCount: kind === 'moved' ? 0 : Math.max(1, Math.min(3, event.detail)),
  };
}

export function wheelEvent(event: WheelLike, rect: RectLike): BrowserInputEvent {
  const scale = event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? 800 : 1;
  return { type: 'wheel', x: event.clientX - rect.left, y: event.clientY - rect.top, dx: event.deltaX * scale, dy: event.deltaY * scale, modifiers: modifiers(event) };
}

/** A printable key carries its text on keydown only; shortcuts keep their modifiers and no text. */
export function keyEvent(kind: 'down' | 'up', event: KeyLike): BrowserInputEvent {
  const printable = event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey;
  const enter = event.key === 'Enter';
  return { type: 'key', kind, key: event.key, code: event.code, text: kind === 'down' && (printable || enter) ? (enter ? '\r' : event.key) : null, modifiers: modifiers(event), keyCode: event.keyCode };
}

/**
 * Collects calls and runs the last one after `quietMs` without a new call:
 * a tile being dragged resizes its page once, at its final size.
 */
export function settle<T>(quietMs: number, run: (value: T) => void, timers: { set: (fn: () => void, ms: number) => unknown; clear: (handle: unknown) => void } = { set: (fn, ms) => setTimeout(fn, ms), clear: handle => clearTimeout(handle as ReturnType<typeof setTimeout>) }): (value: T) => void {
  let handle: unknown = null;
  return value => {
    if (handle !== null) timers.clear(handle);
    handle = timers.set(() => { handle = null; run(value); }, quietMs);
  };
}

/**
 * Mouse moves are many; the host takes batches. Moves coalesce to the last,
 * everything else keeps its order.
 */
export function coalesce(events: readonly BrowserInputEvent[]): BrowserInputEvent[] {
  const out: BrowserInputEvent[] = [];
  for (const event of events) {
    const last = out[out.length - 1];
    if (event.type === 'mouse' && event.kind === 'moved' && last?.type === 'mouse' && last.kind === 'moved') out[out.length - 1] = event;
    else out.push(event);
  }
  return out;
}
