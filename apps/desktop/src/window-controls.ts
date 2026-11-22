import { mountWindowControls } from '@acaprino/imago/shell';
import { tauriWindowApi } from '@acaprino/imago/tauri';
import { initializeResourceTelemetry } from './resource-telemetry.ts';

export async function initializeWindowControls(reportError: (error: unknown) => void) {
  const titlebar = document.querySelector<HTMLElement>('.im-titlebar')!;
  const fail = (_action: string, error: unknown) => reportError(error);
  const release = mountWindowControls(titlebar, tauriWindowApi(fail), { onError: fail });
  for (const button of titlebar.querySelectorAll<HTMLButtonElement>('[data-im-window]')) button.disabled = false;
  window.addEventListener('pagehide', release, { once: true });
  initializeResourceTelemetry();
}
