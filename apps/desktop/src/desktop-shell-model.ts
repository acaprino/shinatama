// Decisions of the desktop shell (desktop-shell.ts) that need no window: the zoom levels, where
// the engine's keys, menu and Select All stay. Kept free of Tauri imports for tests; the keys and
// the menu themselves are Imago's guardDesktop and zoom controller.

/** Zoom levels of the interface; 200% is the WCAG minimum the app must reach on its own. */
/** The interface zoom levels Settings offers: Imago's, 50 to 200% in steps of 10 (a level saved before is rounded to one). */
export const ZOOM_STEPS = Array.from({ length: 16 }, (_, step) => (5 + step) / 10);

/** Terminals, the browser tab and the editor run their own keys and menus: guardDesktop leaves them alone there. */
export const ENGINE_SURFACES = '.xterm, .browser-canvas, .cm-editor';

/** Fields, editors, terminals and the browser tab keep their own keys and menus. */
export const OWN_INPUT = `input, textarea, select, [contenteditable=""], [contenteditable="true"], ${ENGINE_SURFACES}`;

export interface ElementLike { closest(selector: string): unknown }

/** Ctrl+A selects inside a field or terminal; outside, at most the open panel's text. */
export function keepsSelectAll(target: ElementLike | null): boolean {
  return !!target && target.closest(OWN_INPUT) !== null;
}

export interface EnterLike { readonly key: string; readonly isComposing: boolean; readonly keyCode: number }

/** Enter that confirms a field, not the Enter that picks an IME candidate (keyCode 229). */
export function confirmsField(event: EnterLike): boolean {
  return event.key === 'Enter' && !event.isComposing && event.keyCode !== 229;
}
