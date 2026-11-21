import { clampZoom, stepZoom, zoomDirection } from '@acaprino/imago/shell';

// Decisions of the desktop shell (desktop-shell.ts) that need no window: which keys zoom,
// where the engine's context menu and Select All stay. Kept free of Tauri imports for tests.

/** Zoom levels of the interface; 200% is the WCAG minimum the app must reach on its own. */
export const ZOOM_STEPS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;

export type ZoomCommand = 'in' | 'out' | 'reset';

export interface KeyLike { readonly key: string; readonly code: string; readonly ctrlKey: boolean; readonly altKey: boolean; readonly metaKey: boolean; readonly shiftKey: boolean }

/** Ctrl+= / Ctrl++ / Ctrl+- / Ctrl+0, by character or by physical key, on any layout. */
export function zoomCommand(event: KeyLike): ZoomCommand | null {
  const direction = zoomDirection(event);
  return direction === null ? null : direction === 0 ? 'reset' : direction === 1 ? 'in' : 'out';
}

/** The next level from any current one, including a level saved by an older step list. */
export function nextZoom(current: number, command: ZoomCommand): number {
  return stepZoom(current, command === 'in' ? 1 : command === 'out' ? -1 : 0, ZOOM_STEPS);
}

/** A saved level, or 100% when it is missing or not one the app offers. */
export function savedZoom(stored: string | null): number {
  return clampZoom(Number(stored), ZOOM_STEPS);
}

/** Fields, editors, terminals and the browser tab keep their own keys and menus. */
export const OWN_INPUT = 'input, textarea, select, [contenteditable=""], [contenteditable="true"], .cm-editor, .xterm, .browser-canvas';

export interface ElementLike { closest(selector: string): unknown }

/** The engine's menu offers editing in fields and Copy over selected text; elsewhere it would
 *  offer Back, Refresh, Print and Save as, which do not belong to the app. */
export function keepsContextMenu(target: ElementLike | null, selectionContainsTarget: boolean): boolean {
  return !!target && (target.closest(OWN_INPUT) !== null || selectionContainsTarget);
}

/** Ctrl+A selects inside a field or terminal; outside, at most the open panel's text. */
export function keepsSelectAll(target: ElementLike | null): boolean {
  return !!target && target.closest(OWN_INPUT) !== null;
}

export interface EnterLike { readonly key: string; readonly isComposing: boolean; readonly keyCode: number }

/** Enter that confirms a field, not the Enter that picks an IME candidate (keyCode 229). */
export function confirmsField(event: EnterLike): boolean {
  return event.key === 'Enter' && !event.isComposing && event.keyCode !== 229;
}

/** Keys the engine maps to browser features (reload, print, find, history, source, caret
 *  browsing, DevTools). WebView2's browser accelerators are off in a release build; this
 *  guard also covers the engine's handling of keys the page leaves unhandled. Terminals and
 *  the browser tab consume their own keys before the engine sees them. */
export function browserCommand(event: KeyLike): boolean {
  const { key, ctrlKey: ctrl, shiftKey: shift, altKey: alt, metaKey: meta } = event;
  if (meta) return false;
  const lower = key.length === 1 ? key.toLowerCase() : key;
  if (key === 'F5' || key === 'F7' || key === 'F12' || key === 'BrowserBack' || key === 'BrowserForward' || key === 'BrowserRefresh') return !alt;
  if (alt && !ctrl && (key === 'ArrowLeft' || key === 'ArrowRight' || key === 'Home')) return true;
  if (!ctrl || alt) return false;
  if (shift) return ['r', 'g', 'i', 'j', 'c'].includes(lower);
  return ['r', 'p', 'u', 'f', 'g', 's', 'h', 'j', 'd', 'o', 'n'].includes(lower);
}
