import { invoke } from '@tauri-apps/api/core';
import { openModal } from './modal.ts';

type Status = { phase: string; healthy: boolean; message: string | null };
const LABELS: Record<string, string> = {
  stopped: 'Stopped', preparing: 'Preparing…', starting: 'Starting…', ready: 'Ready',
  busy: 'Busy', degraded: 'Temporarily unavailable', recovering: 'Recovering…',
  failed: 'Stopped after a failure', retired: 'Recovery needs attention', unavailable: 'Status unavailable',
};
export function createSidecarControl(desktop: boolean, notify: (message: string) => void) {
  const button = document.createElement('button'); button.id = 'session-index'; button.type = 'button';
  button.textContent = 'Session index'; button.disabled = !desktop;
  document.querySelector('.toolbar')?.append(button);
  const dialog = document.createElement('dialog'); dialog.id = 'session-index-dialog';
  dialog.setAttribute('aria-labelledby', 'session-index-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'session-index-title'; title.textContent = 'Session index';
  const close = document.createElement('button'); close.type = 'button'; close.textContent = '×'; close.setAttribute('aria-label', 'Close session index');
  heading.append(title, close);
  const description = document.createElement('p'); description.textContent = 'Keeps local sessions available while agent terminals run. Recovery never resends an agent request.';
  const state = document.createElement('p'); state.id = 'session-index-state'; state.setAttribute('role', 'status');
  const message = document.createElement('p'); message.className = 'hint';
  const actions = document.createElement('div'); actions.className = 'dialog-actions';
  const start = document.createElement('button'); start.type = 'button'; start.textContent = 'Start'; start.className = 'primary';
  const stop = document.createElement('button'); stop.type = 'button'; stop.textContent = 'Stop';
  const retry = document.createElement('button'); retry.type = 'button'; retry.textContent = 'Retry recovery';
  actions.append(stop, retry, start); dialog.append(heading, description, state, message, actions);
  (document.querySelector('#app') ?? document.body).append(dialog);
  let current: Status | null = null; let outstanding = false; let destroyed = false;
  function render() {
    const phase = current?.phase ?? 'unavailable';
    button.textContent = `Session index · ${LABELS[phase] ?? LABELS.unavailable}`;
    state.textContent = LABELS[phase] ?? LABELS.unavailable;
    message.textContent = current?.message ?? (current?.healthy ? 'Local service is responding.' : 'Saved sessions are preserved.');
    start.disabled = outstanding || !['stopped', 'failed'].includes(phase);
    retry.disabled = outstanding || !['failed', 'retired'].includes(phase);
    stop.disabled = outstanding || ['stopped', 'unavailable'].includes(phase);
    dialog.setAttribute('aria-busy', String(outstanding));
  }
  async function request(command: string, report = false) {
    if (outstanding || destroyed || !desktop) return;
    outstanding = true; render();
    try {
      const received = await invoke<unknown>(command);
      if (received && typeof received === 'object') {
        const value = received as Record<string, unknown>;
        if (typeof value.phase !== 'string' || !Object.hasOwn(LABELS, value.phase) || typeof value.healthy !== 'boolean'
          || !(value.message === null || (typeof value.message === 'string' && value.message.length <= 400))) throw new Error('Invalid status');
        current = { phase: value.phase, healthy: value.healthy, message: value.message as string | null };
      } else throw new Error('Missing status');
    } catch { current = null; if (report) notify('Session index action did not complete. Check its status before retrying.'); }
    finally { outstanding = false; if (!destroyed) render(); }
  }
  button.onclick = () => { if (!document.querySelector('dialog[open]')) { render(); openModal(dialog); close.focus(); void request('sidecar_status'); } };
  close.onclick = () => dialog.close();
  start.onclick = () => { void request('sidecar_start', true); };
  stop.onclick = () => { void request('sidecar_stop', true); };
  retry.onclick = () => { void request('sidecar_retry', true); };
  render();
  if (desktop) void request('sidecar_status');
  const timer = desktop ? window.setInterval(() => { if (dialog.open || current?.phase !== 'stopped') void request('sidecar_status'); }, 1500) : 0;
  return { destroy() { destroyed = true; window.clearInterval(timer); button.remove(); dialog.remove(); } };
}
