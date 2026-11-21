// What the main window does as a desktop application rather than a web page (the native side
// is native_frame.rs): the interface zooms on Ctrl+= / Ctrl+- / Ctrl+0, browser keys (F5,
// Ctrl+R, Ctrl+P, Alt+Left...) do nothing, the engine's page menu and Select All stay out of the
// chrome, the mouse's back and forward buttons do not navigate, and "Reload interface" in the
// window menu saves the layout before the page reloads.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { createZoomController, guardDesktop, type ZoomController } from '@acaprino/imago/shell';
import { ENGINE_SURFACES, keepsSelectAll, ZOOM_STEPS } from './desktop-shell-model.ts';

const ZOOM_KEY = 'shinatama.zoom';

export interface DesktopShellOptions {
  readonly notify: (message: string, tone: 'info') => void;
  /** Saves what the page holds; a reload follows. */
  readonly beforeReload: () => Promise<unknown>;
}

let zoom: ZoomController | null = null;
const zoomListeners = new Set<(level: number) => void>();
export const interfaceZoom = () => zoom?.current() ?? 1;
export const canZoom = () => zoom !== null;
export function onInterfaceZoom(listener: (level: number) => void): void { zoomListeners.add(listener); }
export function changeZoom(level: number): Promise<boolean> {
  return zoom?.set(level) ?? Promise.resolve(false);
}

export async function initializeDesktopShell(options: DesktopShellOptions): Promise<void> {
  const webview = getCurrentWebview();
  const notifyZoom = (level: number) => { for (const listener of zoomListeners) listener(level); };
  const controller = createZoomController({
    storageKey: ZOOM_KEY, applyDefaultOnStart: false,
    apply: level => webview.setZoom(level), onChange: notifyZoom,
  });
  await controller.ready;
  zoom = controller;
  notifyZoom(controller.current());

  const releaseGuard = guardDesktop(document, { allowReloadIn: ENGINE_SURFACES });
  // Capture: a terminal or a field never sees these keys, as in Windows Terminal.
  const onKey = (event: KeyboardEvent): void => {
    const target = event.target instanceof Element ? event.target : null;
    if (!target?.closest('.browser-canvas')) {
      const applied = controller.handleKey(event);
      if (applied) {
        const requested = controller.current();
        void applied.then(changed => options.notify(changed ? `Zoom ${Math.round(requested * 100)}%` : 'This window cannot change its zoom.', 'info'));
        return;
      }
    }
    if (event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey && event.key.toLowerCase() === 'a' && !keepsSelectAll(target)) {
      event.preventDefault();
      const panel = document.activeElement?.closest('dialog[open]');
      if (panel) window.getSelection()?.selectAllChildren(panel);
    }
  };
  document.addEventListener('keydown', onKey, true);

  const stopReload = await listen<number>('interface-reload-requested', ({ payload }) => {
    void options.beforeReload().catch(() => undefined).finally(() => { void invoke('interface_reload_ready', { token: payload }); });
  });
  window.addEventListener('pagehide', () => { releaseGuard(); stopReload(); document.removeEventListener('keydown', onKey, true); }, { once: true });
}
