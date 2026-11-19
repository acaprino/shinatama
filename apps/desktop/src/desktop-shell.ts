// What the main window does as a desktop application rather than a web page (the native side
// is native_frame.rs): the interface zooms on Ctrl+= / Ctrl+- / Ctrl+0, browser keys (F5,
// Ctrl+R, Ctrl+P, Alt+Left...) do nothing, the engine's page menu and Select All stay out of the
// chrome, the mouse's back and forward buttons do not navigate, and "Reload interface" in the
// window menu saves the layout before the page reloads.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { browserCommand, keepsContextMenu, keepsSelectAll, nextZoom, savedZoom, zoomCommand } from './desktop-shell-model.ts';

const ZOOM_KEY = 'shinatama.zoom';

export interface DesktopShellOptions {
  readonly notify: (message: string, tone: 'info') => void;
  /** Saves what the page holds; a reload follows. */
  readonly beforeReload: () => Promise<unknown>;
}

function readZoom(): number {
  try { return savedZoom(localStorage.getItem(ZOOM_KEY)); } catch { return 1; }
}

// The interface zoom is one preference with two controls: the keys below and the Appearance
// section of Settings. Both move it through changeZoom, so they never disagree.
let zoom = 1;
let zoomWebview: ReturnType<typeof getCurrentWebview> | null = null;
const zoomListeners = new Set<(level: number) => void>();
/** The interface zoom now; 1 before the desktop shell starts or in a browser preview. */
export const interfaceZoom = () => zoom;
/** False until the desktop shell runs: a browser preview cannot zoom the window. */
export const canZoom = () => zoomWebview !== null;
/** Called with the level every time it changes, from either control. */
export function onInterfaceZoom(listener: (level: number) => void): void { zoomListeners.add(listener); }
/** Moves the interface to a level the app offers and saves it. False when the window cannot zoom. */
export function changeZoom(next: number): Promise<boolean> {
  const webview = zoomWebview; const level = savedZoom(String(next));
  if (!webview) return Promise.resolve(false);
  // The level moves at once, so presses faster than the engine count every step.
  const previous = zoom; zoom = level;
  for (const listener of zoomListeners) listener(zoom);
  return webview.setZoom(level).then(() => {
    try { localStorage.setItem(ZOOM_KEY, String(level)); } catch { /* the level lasts until restart */ }
    return true;
  }, () => {
    if (zoom === level) { zoom = previous; for (const listener of zoomListeners) listener(zoom); }
    return false;
  });
}

export async function initializeDesktopShell(options: DesktopShellOptions): Promise<void> {
  const webview = getCurrentWebview();
  zoom = readZoom();
  if (zoom !== 1) await webview.setZoom(zoom).catch(() => { zoom = 1; });
  zoomWebview = webview;
  for (const listener of zoomListeners) listener(zoom);

  // Capture: a terminal or a field never sees these keys, as in Windows Terminal.
  document.addEventListener('keydown', event => {
    const target = event.target instanceof Element ? event.target : null;
    const command = target?.closest('.browser-canvas') ? null : zoomCommand(event);
    if (command) {
      event.preventDefault(); event.stopPropagation();
      if (event.repeat && command === 'reset') return;
      const next = nextZoom(zoom, command);
      if (next === zoom) { options.notify(`Zoom ${Math.round(zoom * 100)}%`, 'info'); return; }
      void changeZoom(next).then(changed => options.notify(changed ? `Zoom ${Math.round(next * 100)}%` : 'This window cannot change its zoom.', 'info'));
      return;
    }
    // Not stopped: the app's own handlers for the same keys (F3, Ctrl+S in the files panel) still run.
    if (browserCommand(event) && !target?.closest('.xterm, .browser-canvas, .cm-editor')) { event.preventDefault(); return; }
    if (event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey && event.key.toLowerCase() === 'a' && !keepsSelectAll(target)) {
      event.preventDefault();
      const panel = document.activeElement?.closest('dialog[open]');
      if (panel) window.getSelection()?.selectAllChildren(panel);
    }
  }, true);

  document.addEventListener('contextmenu', event => {
    const target = event.target instanceof Element ? event.target : null;
    const selection = window.getSelection();
    const selected = !!selection && !selection.isCollapsed && !!target && selection.containsNode(target, true);
    if (!keepsContextMenu(target, selected)) event.preventDefault();
  });

  // The fourth and fifth buttons are Back and Forward to the engine.
  window.addEventListener('mouseup', event => { if (event.button === 3 || event.button === 4) event.preventDefault(); });
  // An image dragged out of the chrome is not a file the user meant to move.
  document.addEventListener('dragstart', event => { if (event.target instanceof HTMLImageElement) event.preventDefault(); });

  const stopReload = await listen<number>('interface-reload-requested', ({ payload }) => {
    void options.beforeReload().catch(() => undefined).finally(() => { void invoke('interface_reload_ready', { token: payload }); });
  });
  window.addEventListener('pagehide', () => { stopReload(); }, { once: true });
}
