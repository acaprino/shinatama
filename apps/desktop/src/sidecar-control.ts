import { uiPart, uiButton, uiTraits } from '@acaprino/imago/ui';
import { invoke } from '@tauri-apps/api/core';
import { sidePanel } from './modal.ts';
import { adoptStopReply, archiveLine, buttons, confirmStillNeeded, diskLine, driftLine, exitLine, faultLine, indicator, messageLine, parseStatus, phaseLabel, restartOffered, showsTraffic, stallLine, startLabel, stopConsequence, trafficLine, type ProxyStatus } from './sidecar-status.ts';

const POLL_MS = 1500;

/** The Model proxy side panel. An `aside`, never a dialog: terminals stay live beside it and the
 * confirmations before a costly stop or a restart are inline, inside the panel. `restart` closes the
 * workspace and starts Shinatama again: the exit that works when the supervisor no longer answers. */
export function createSidecarControl(desktop: boolean, notify: (message: string) => void, restart: () => Promise<void>) {
  // The toolbar shows a traffic light, not a labelled button: the phase is in its accessible name
  // and tooltip, and a click still opens the panel.
  const button = Object.assign(uiButton(''), { id: 'model-proxy', disabled: !desktop });
  button.setAttribute('aria-controls', 'model-proxy-panel'); button.setAttribute('aria-expanded', 'false');
  const lamp = uiTraits(uiPart('text'), 'signal'); button.replaceChildren(lamp);
  document.querySelector('.toolbar')?.append(button);

  const panel = Object.assign(uiTraits(uiPart('aside'), 'dockedInspector'), { id: 'model-proxy-panel', hidden: true });
  panel.setAttribute('aria-labelledby', 'model-proxy-title');
  const close = uiButton('×'); close.setAttribute('aria-label', 'Close Model proxy');
  const heading = uiPart('row', undefined, 'dialog-heading');
  heading.append(Object.assign(uiPart('title', 'Model proxy'), { id: 'model-proxy-title' }), close);
  const description = uiPart('paragraph', 'Every agent reaches its model through this local service. Recovery never resends an agent request.');
  const state = Object.assign(uiPart('status'), { id: 'model-proxy-state', tabIndex: -1 }); state.setAttribute('role', 'status');
  const traffic = uiPart('hint');
  const message = uiPart('hint');
  const stall = uiPart('hint');
  const fault = uiPart('hint');
  const disk = uiPart('hint');
  const archive = uiPart('hint');
  const drift = uiPart('hint');
  const interruption = uiPart('hint');
  const confirm = Object.assign(uiPart('panel'), { id: 'model-proxy-confirm', hidden: true }); confirm.setAttribute('role', 'group'); confirm.setAttribute('aria-label', 'Confirm stop');
  const consequence = uiPart('paragraph');
  const keep = uiButton('Keep running', { primary: true });
  const stopAnyway = uiButton('Stop anyway');
  const choices = uiPart('actions'); choices.append(stopAnyway, keep);
  confirm.append(consequence, choices);
  const restartConfirm = Object.assign(uiPart('panel'), { id: 'model-proxy-restart', hidden: true }); restartConfirm.setAttribute('role', 'group'); restartConfirm.setAttribute('aria-label', 'Confirm restart');
  const restartText = uiPart('paragraph', 'Shinatama will close and start again. Running terminals stop; their sessions are restored at the next start.');
  const restartNow = uiButton('Restart Shinatama', { cls: 'danger', danger: true });
  const keepWaiting = uiButton('Keep waiting', { primary: true });
  const restartChoices = uiPart('actions'); restartChoices.append(restartNow, keepWaiting);
  restartConfirm.append(restartText, restartChoices);
  const actions = uiPart('actions');
  const start = uiButton('', { primary: true });
  const stop = uiButton('Stop');
  const retry = uiButton('Retry recovery');
  const restartOffer = Object.assign(uiButton('Restart Shinatama', { cls: 'danger', danger: true }), { hidden: true });
  actions.append(stop, retry, restartOffer, start);
  panel.append(heading, description, state, traffic, message, stall, fault, disk, archive, interruption, drift, confirm, restartConfirm, actions);
  (document.querySelector('#work-area') ?? document.querySelector('#app') ?? document.body).append(panel);

  let current: ProxyStatus | null = null; let outstanding = false; let destroyed = false; let confirming = false;
  // Status reads that failed in a row: three mean the panel can no longer see the proxy's owner.
  let failedPolls = 0; let confirmingRestart = false; let restarting = false;

  function render() {
    const phase = current?.phase ?? 'unavailable';
    const shown = indicator(current);
    lamp.dataset.imTone = { green: 'ok', amber: 'warn', red: 'err', off: '' }[shown.tone];
    button.dataset.tone = shown.tone; button.setAttribute('aria-label', shown.label); button.title = shown.label;
    state.textContent = current?.stalled ? 'Not reporting' : phaseLabel(phase);
    const running = current !== null && showsTraffic(phase);
    traffic.hidden = !running; traffic.textContent = running ? trafficLine(current!) : '';
    const said = messageLine(current);
    message.hidden = said === null; message.textContent = said ?? '';
    for (const [line, text] of [[stall, current && stallLine(current)], [fault, current && faultLine(current)], [disk, current && diskLine(current)], [archive, current && archiveLine(current)]] as const) {
      line.hidden = !text; line.textContent = text || '';
    }
    const offered = restartOffered(current, failedPolls);
    if (!offered && !restarting) confirmingRestart = false;
    restartOffer.hidden = !offered; restartOffer.disabled = !offered || restarting || confirmingRestart;
    restartConfirm.hidden = !confirmingRestart; restartNow.disabled = restarting; keepWaiting.disabled = restarting;
    const note = current ? driftLine(current) : null;
    drift.hidden = note === null; drift.textContent = note ?? '';
    const ended = current ? exitLine(current) : null;
    interruption.hidden = ended === null; interruption.textContent = ended ?? '';
    const enabled = buttons(current, outstanding);
    start.disabled = !enabled.start; stop.disabled = !enabled.stop; retry.disabled = !enabled.retry;
    start.textContent = startLabel(current);
    const cost = current ? stopConsequence(current) : null;
    if (confirming && !confirmStillNeeded(current)) confirming = false;
    confirm.hidden = !confirming; consequence.textContent = confirming && cost ? cost : '';
    panel.setAttribute('aria-busy', String(outstanding));
  }
  // A reply that began before the panel was closed or a newer command is stale and never adopted.
  let generation = 0; let polling = false;
  async function request(command: string, report = false, args?: Record<string, unknown>) {
    const poll = command === 'sidecar_status';
    if (destroyed || !desktop || outstanding || (poll && polling)) return;
    if (poll) polling = true; else { outstanding = true; generation++; render(); }
    const started = generation; const before = current?.phase; let again = false;
    try {
      const received = parseStatus(await invoke<unknown>(command, args));
      if (!received) throw new Error('Invalid status');
      if (poll) failedPolls = 0;
      if (started !== generation) again = !poll;
      else if (command === 'sidecar_stop' && !adoptStopReply(before)) again = true;
      else {
        current = received;
        if (received.refused) confirming = true; // the server saw agents the panel had not
      }
    } catch {
      if (poll) failedPolls += 1;
      if (started === generation) current = null;
      if (report) notify('Model proxy action did not complete. Check its status before retrying.');
    }
    finally {
      if (poll) polling = false; else outstanding = false;
      if (!destroyed) { render(); if (!confirm.hidden && command === 'sidecar_stop') keep.focus(); }
    }
    if (again) void request('sidecar_status');
  }
  function open() { opening(); panel.hidden = false; button.setAttribute('aria-expanded', 'true'); render(); close.focus(); void request('sidecar_status'); }
  function shut(returnFocus: boolean) {
    if (panel.hidden) return;
    generation++; panel.hidden = true; confirming = false; if (!restarting) confirmingRestart = false; button.setAttribute('aria-expanded', 'false'); render();
    if (returnFocus) button.focus();
  }
  // One side panel at a time: opening this one closes the drawer open, and a drawer closes this one.
  const opening = sidePanel(() => shut(false));
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
  restartOffer.onclick = () => { confirmingRestart = true; render(); keepWaiting.focus(); };
  keepWaiting.onclick = () => { confirmingRestart = false; render(); restartOffer.focus(); };
  restartNow.onclick = async () => {
    restarting = true; render();
    // A refused restart closed nothing: say why and keep the panel as it was.
    try { await restart(); } catch (error) { notify(error instanceof Error ? error.message : String(error)); }
    finally { restarting = false; confirmingRestart = false; if (!destroyed) render(); }
  };
  stopAnyway.onclick = () => { confirming = false; render(); state.focus(); void request('sidecar_stop', true, { force: true }); };
  render();
  if (desktop) void request('sidecar_status');
  const timer = desktop ? window.setInterval(() => { if (!panel.hidden || current?.phase !== 'stopped') void request('sidecar_status'); }, POLL_MS) : 0;
  return { destroy() { destroyed = true; window.clearInterval(timer); document.removeEventListener('keydown', onKey); button.remove(); panel.remove(); } };
}
