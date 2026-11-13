import { invoke } from '@tauri-apps/api/core';
import { buttons, driftLine, parseStatus, phaseLabel, startLabel, stopConsequence, trafficLine, type ProxyStatus } from './sidecar-status.ts';

// A stop answered for a proxy that was already down says nothing true about it; the next poll does.
const DOWN = ['stopped', 'failed', 'retired', 'unavailable'];
const RUNNING = ['starting', 'ready', 'busy', 'draining', 'degraded', 'recovering'];
const POLL_MS = 1500;

/** The Model proxy side panel. An `aside`, never a dialog: terminals stay live beside it and the
 * confirmation before a costly stop is inline, inside the panel. */
export function createSidecarControl(desktop: boolean, notify: (message: string) => void) {
  const el = <K extends keyof HTMLElementTagNameMap>(tag: K, props: Partial<HTMLElementTagNameMap[K]> = {}) => Object.assign(document.createElement(tag), props);
  const button = el('button', { id: 'model-proxy', type: 'button', textContent: 'Model proxy', disabled: !desktop });
  button.setAttribute('aria-controls', 'model-proxy-panel'); button.setAttribute('aria-expanded', 'false');
  document.querySelector('.toolbar')?.append(button);

  const panel = el('aside', { id: 'model-proxy-panel', hidden: true });
  panel.setAttribute('aria-labelledby', 'model-proxy-title');
  const close = el('button', { type: 'button', textContent: '×' }); close.setAttribute('aria-label', 'Close Model proxy');
  const heading = el('div', { className: 'dialog-heading' });
  heading.append(el('h1', { id: 'model-proxy-title', textContent: 'Model proxy' }), close);
  const description = el('p', { textContent: 'Every agent reaches its model through this local service. Recovery never resends an agent request.' });
  const state = el('p', { id: 'model-proxy-state', tabIndex: -1 }); state.setAttribute('role', 'status');
  const traffic = el('p', { className: 'hint' });
  const message = el('p', { className: 'hint' });
  const drift = el('p', { className: 'hint' });
  const confirm = el('div', { id: 'model-proxy-confirm', hidden: true }); confirm.setAttribute('role', 'group'); confirm.setAttribute('aria-label', 'Confirm stop');
  const consequence = el('p');
  const keep = el('button', { type: 'button', textContent: 'Keep running', className: 'primary' });
  const stopAnyway = el('button', { type: 'button', textContent: 'Stop anyway' });
  const choices = el('div', { className: 'proxy-actions' }); choices.append(stopAnyway, keep);
  confirm.append(consequence, choices);
  const actions = el('div', { className: 'proxy-actions' });
  const start = el('button', { type: 'button', className: 'primary' });
  const stop = el('button', { type: 'button', textContent: 'Stop' });
  const retry = el('button', { type: 'button', textContent: 'Retry recovery' });
  actions.append(stop, retry, start);
  panel.append(heading, description, state, traffic, message, drift, confirm, actions);
  (document.querySelector('#app') ?? document.body).append(panel);

  let current: ProxyStatus | null = null; let outstanding = false; let destroyed = false; let confirming = false;

  function render() {
    const phase = current?.phase ?? 'unavailable';
    button.textContent = `Model proxy · ${phaseLabel(phase)}`;
    state.textContent = phaseLabel(phase);
    const running = current !== null && RUNNING.includes(phase);
    traffic.hidden = !running; traffic.textContent = running ? trafficLine(current!) : '';
    message.textContent = current?.message ?? (current?.healthy ? 'Local service is responding.' : 'Saved sessions are preserved.');
    const note = current ? driftLine(current) : null;
    const values = current?.resolved ? Object.entries(current.resolved).map(([key, value]) => `${key} ${value}`).join(' · ') : '';
    drift.hidden = note === null; drift.textContent = note === null ? '' : values ? `${note} Running with: ${values}` : note;
    const enabled = buttons(current, outstanding);
    start.disabled = !enabled.start; stop.disabled = !enabled.stop; retry.disabled = !enabled.retry;
    start.textContent = startLabel(current);
    const cost = current ? stopConsequence(current) : null;
    if (confirming && (cost === null || !buttons(current, false).stop)) confirming = false; // nothing left to confirm
    confirm.hidden = !confirming; consequence.textContent = confirming && cost ? cost : '';
    panel.setAttribute('aria-busy', String(outstanding));
  }
  // A poll never disables the buttons or outlives a command: a reply that started before a command is stale.
  let generation = 0; let polling = false;
  async function request(command: string, report = false, args?: Record<string, unknown>) {
    const poll = command === 'sidecar_status';
    if (destroyed || !desktop || outstanding || (poll && polling)) return;
    const started = generation;
    if (poll) polling = true; else { outstanding = true; generation++; render(); }
    const before = current?.phase; let again = false;
    try {
      const received = parseStatus(await invoke<unknown>(command, args));
      if (!received) throw new Error('Invalid status');
      if (poll && started !== generation) return;
      if (command === 'sidecar_stop' && (before === undefined || DOWN.includes(before))) again = true;
      else {
        current = received;
        if (received.refused) confirming = true; // the server saw agents the panel had not
      }
    } catch { if (!poll || started === generation) current = null; if (report) notify('Model proxy action did not complete. Check its status before retrying.'); }
    finally {
      if (poll) polling = false; else outstanding = false;
      if (!destroyed) { render(); if (!confirm.hidden && command === 'sidecar_stop') keep.focus(); }
    }
    if (again) void request('sidecar_status');
  }
  function open() { panel.hidden = false; button.setAttribute('aria-expanded', 'true'); render(); close.focus(); void request('sidecar_status'); }
  function shut(returnFocus: boolean) {
    if (panel.hidden) return;
    panel.hidden = true; confirming = false; button.setAttribute('aria-expanded', 'false'); render();
    if (returnFocus) button.focus();
  }
  button.onclick = () => { if (panel.hidden) open(); else shut(true); };
  close.onclick = () => shut(true);
  const onKey = (event: KeyboardEvent) => {
    if (event.key !== 'Escape' || panel.hidden || document.querySelector('dialog[open]')) return;
    // Escape belongs to a terminal agent unless focus is on this panel or its button.
    if (panel.contains(document.activeElement) || document.activeElement === button) { event.preventDefault(); shut(true); }
  };
  document.addEventListener('keydown', onKey);
  start.onclick = () => { void request('sidecar_start', true); };
  retry.onclick = () => { void request('sidecar_retry', true); };
  stop.onclick = () => {
    if (current && stopConsequence(current) !== null) { confirming = true; render(); keep.focus(); return; }
    void request('sidecar_stop', true, { force: false });
  };
  keep.onclick = () => { confirming = false; render(); stop.focus(); };
  stopAnyway.onclick = () => { confirming = false; render(); state.focus(); void request('sidecar_stop', true, { force: true }); };
  render();
  if (desktop) void request('sidecar_status');
  const timer = desktop ? window.setInterval(() => { if (!panel.hidden || current?.phase !== 'stopped') void request('sidecar_status'); }, POLL_MS) : 0;
  return { destroy() { destroyed = true; window.clearInterval(timer); document.removeEventListener('keydown', onKey); button.remove(); panel.remove(); } };
}
