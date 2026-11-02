import { getCurrentWindow } from '@tauri-apps/api/window';
import { initializeResourceTelemetry } from './resource-telemetry.ts';

/** Keep window operations separate from terminal/view operations. */
export async function initializeWindowControls(reportError: (error: unknown) => void) {
  const appWindow = getCurrentWindow();
  const minimize = document.querySelector<HTMLButtonElement>('#window-minimize')!;
  const maximize = document.querySelector<HTMLButtonElement>('#window-maximize')!;
  const close = document.querySelector<HTMLButtonElement>('#window-close')!;
  let revision = 0;

  async function syncMaximized() {
    const request = ++revision;
    const maximized = await appWindow.isMaximized();
    if (request !== revision) return;
    maximize.setAttribute('aria-label', maximized ? 'Restore window' : 'Maximize window');
    maximize.title = maximized ? 'Restore' : 'Maximize';
    maximize.querySelector('.maximize-icon')!.toggleAttribute('hidden', maximized);
    maximize.querySelector('.restore-icon')!.toggleAttribute('hidden', !maximized);
  }

  minimize.onclick = () => { void appWindow.minimize().catch(reportError); };
  maximize.onclick = () => { void appWindow.toggleMaximize().then(syncMaximized).catch(reportError); };
  // The existing Rust CloseRequested handler retains live-terminal confirmation.
  close.onclick = () => { void appWindow.close().catch(reportError); };
  await appWindow.onResized(() => { void syncMaximized().catch(reportError); });
  await syncMaximized();
  for (const button of [minimize, maximize, close]) button.disabled = false;
  initializeResourceTelemetry();
}
