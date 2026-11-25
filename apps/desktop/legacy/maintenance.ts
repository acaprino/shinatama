import { invoke } from '@tauri-apps/api/core';

type Status = { phase: string; completed_at: number | null; message: string | null };

/** Activity hints contain no keys, terminal text, paths or document contents. */
export function initializeMaintenance(notify: (message: string) => void) {
  let lastHint = -Infinity;
  let lastNotice: string | null = null;
  let checking = false;
  const activity = () => {
    const now = performance.now();
    if (now - lastHint < 1000) return;
    lastHint = now;
    void invoke('maintenance_activity').catch(() => {});
  };
  for (const name of ['keydown', 'pointerdown', 'pointermove', 'wheel']) {
    document.addEventListener(name, activity, { capture: true, passive: true });
  }
  window.addEventListener('focus', activity);
  activity();
  window.setInterval(async () => {
    if (checking || document.hidden) return;
    checking = true;
    try {
      const status = await invoke<Status>('maintenance_status');
      const message = status.message || 'The saved archive needs attention. Your saved work has been kept.';
      if (status.phase === 'attention' && message !== lastNotice) {
        lastNotice = message;
        notify(message);
      }
      if (status.phase === 'idle') lastNotice = null;
    } catch { /* A closed/unavailable runtime does not start retries or block controls. */ }
    finally { checking = false; }
  }, 60_000);
}
