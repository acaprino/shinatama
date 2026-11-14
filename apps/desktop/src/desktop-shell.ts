// What the main window does as a desktop application rather than a web page (the native side
// is native_frame.rs): the interface zooms on Ctrl+= / Ctrl+- / Ctrl+0, the engine's page menu
// and Select All stay out of the chrome, the mouse's back and forward buttons do not navigate,
// and "Reload interface" in the window menu saves the layout before the page reloads.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { keepsContextMenu, keepsSelectAll, nextZoom, savedZoom, zoomCommand } from './desktop-shell-model.ts';

const ZOOM_KEY = 'shinatama.zoom';

export interface DesktopShellOptions {
  readonly notify: (message: string, tone: 'info') => void;
  /** Saves what the page holds; a reload follows. */
  readonly beforeReload: () => Promise<unknown>;
}

function readZoom(): number {
  try { return savedZoom(localStorage.getItem(ZOOM_KEY)); } catch { return 1; }
}

export async function initializeDesktopShell(options: DesktopShellOptions): Promise<void> {
  const webview = getCurrentWebview();
  let zoom = readZoom();
  if (zoom !== 1) await webview.setZoom(zoom).catch(() => { zoom = 1; });

  // Capture: a terminal or a field never sees these keys, as in Windows Terminal.
  document.addEventListener('keydown', event => {
    const target = event.target instanceof Element ? event.target : null;
    const command = target?.closest('.browser-canvas') ? null : zoomCommand(event);
    if (command) {
      event.preventDefault(); event.stopPropagation();
      if (event.repeat && command === 'reset') return;
      // The level moves at once, so presses faster than the engine count every step.
      const previous = zoom; const next = nextZoom(zoom, command);
      if (next === zoom) { options.notify(`Zoom ${Math.round(zoom * 100)}%`, 'info'); return; }
      zoom = next;
      void webview.setZoom(next).then(() => {
        try { localStorage.setItem(ZOOM_KEY, String(next)); } catch { /* the level lasts until restart */ }
        options.notify(`Zoom ${Math.round(next * 100)}%`, 'info');
      }).catch(() => { if (zoom === next) zoom = previous; options.notify('This window cannot change its zoom.', 'info'); });
      return;
    }
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
