import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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
  // The mouse reaches a native window over this button (native_frame.rs), which gives Windows 11
  // its Snap Layouts and maximizes on click; this handler serves the keyboard.
  maximize.onclick = () => { void appWindow.toggleMaximize().then(syncMaximized).catch(reportError); };
  await listen<{ hover: boolean; pressed: boolean }>('caption-maximize', ({ payload }) => {
    maximize.classList.toggle('os-hover', payload.hover);
    maximize.classList.toggle('os-pressed', payload.pressed);
  });
  let reported = '';
  let frame = 0;
  const reportMaximize = () => {
    frame = 0;
    const box = maximize.getBoundingClientRect(); const ratio = window.devicePixelRatio;
    const rect = { x: Math.round(box.left * ratio), y: Math.round(box.top * ratio), width: Math.round(box.width * ratio), height: Math.round(box.height * ratio) };
    const key = JSON.stringify(rect);
    if (key === reported) return;
    reported = key;
    void invoke('window_caption_maximize', rect).catch(() => { reported = ''; });
  };
  const scheduleReport = () => { if (!frame) frame = requestAnimationFrame(reportMaximize); };
  new ResizeObserver(scheduleReport).observe(maximize);
  window.addEventListener('resize', scheduleReport);
  const watchRatio = () => matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`).addEventListener('change', () => { scheduleReport(); watchRatio(); }, { once: true });
  watchRatio();
  scheduleReport();
  // The existing Rust CloseRequested handler retains live-terminal confirmation.
  close.onclick = () => { void appWindow.close().catch(reportError); };
  // Maximized, the native button also covers the resize border: placed again on every resize.
  await appWindow.onResized(() => { reported = ''; scheduleReport(); void syncMaximized().catch(reportError); });
  await syncMaximized();
  for (const button of [minimize, maximize, close]) button.disabled = false;
  initializeResourceTelemetry();
}
