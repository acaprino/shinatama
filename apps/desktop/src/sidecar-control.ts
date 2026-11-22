import { confirm, uiPart, uiButton } from '@acaprino/imago/ui';
import { invoke } from '@tauri-apps/api/core';
import { closeSidePanel, openSidePanel, sidePanelShows, sideView } from './side-panel.ts';
import { adoptStopReply, archiveLine, buttons, confirmStillNeeded, diskLine, driftLine, exitLine, faultLine, indicator, messageLine, parseStatus, phaseLabel, restartOffered, showsTraffic, stallLine, startLabel, stopConsequence, trafficLine, type ProxyStatus } from './sidecar-status.ts';

const POLL_MS = 1500;

/** The Model proxy side panel, a view of the window's side panel, never a dialog itself: terminals stay live beside it.
 * A stop that would cut agents off and a restart cannot be undone, so each asks in a dialog that says what it costs, its
 * Cancel first. `restart` closes the workspace and starts Shinatama again: the exit that works when the supervisor no
 * longer answers. */
export function createSidecarControl(desktop: boolean, notify: (message: string) => void, restart: () => Promise<void>) {
  // The toolbar shows a traffic light, not a labelled button: the phase is in its accessible name
  // and tooltip, and a click still opens the panel.
  const button = Object.assign(uiButton(''), { id: 'model-proxy', disabled: !desktop });
  button.setAttribute('aria-controls', 'model-proxy-panel'); button.setAttribute('aria-expanded', 'false');
  // The canon's dot in the state's tone; the state's word is the button's accessible name and tooltip.
  const lamp = uiPart('text', undefined, 'im-dot'); lamp.setAttribute('aria-hidden', 'true'); button.replaceChildren(lamp);
  document.querySelector('.toolbar')?.append(button);

  const view = sideView('model-proxy-panel', 'Model proxy', 'Close Model proxy', 'model-proxy-title');
  const panel = view.root; const close = view.close;
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
  const actions = uiPart('actions');
  const start = uiButton('', { primary: true });
  const stop = uiButton('Stop');
  const retry = uiButton('Retry recovery');
  const restartOffer = Object.assign(uiButton('Restart Shinatama', { cls: 'danger', danger: true }), { hidden: true });
  actions.append(stop, retry, restartOffer, start);
  view.body.append(description, state, traffic, message, stall, fault, disk, archive, interruption, drift);
  view.foot.append(actions);
  const shownNow = () => sidePanelShows(panel);

  let current: ProxyStatus | null = null; let outstanding = false; let destroyed = false;
  // Status reads that failed in a row: three mean the panel can no longer see the proxy's owner.
  // asking: a stop or restart dialog is open, so a second press or a refused stop does not open another.
  let failedPolls = 0; let asking = false; let restarting = false;

  function render() {
    const phase = current?.phase ?? 'unavailable';
    const shown = indicator(current);
    const tone = { green: 'ok', amber: 'warn', red: 'err', off: '' }[shown.tone];
    if (tone) lamp.dataset.imTone = tone; else delete lamp.dataset.imTone;
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
    restartOffer.hidden = !offered; restartOffer.disabled = !offered || restarting || asking;
    const note = current ? driftLine(current) : null;
    drift.hidden = note === null; drift.textContent = note ?? '';
    const ended = current ? exitLine(current) : null;
    interruption.hidden = ended === null; interruption.textContent = ended ?? '';
    const enabled = buttons(current, outstanding);
    start.disabled = !enabled.start; stop.disabled = !enabled.stop; retry.disabled = !enabled.retry;
    start.textContent = startLabel(current);
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
        if (received.refused) void askStop(); // the server saw agents the panel had not
      }
    } catch {
      if (poll) failedPolls += 1;
      if (started === generation) current = null;
      if (report) notify('Model proxy action did not complete. Check its status before retrying.');
    }
    finally {
      if (poll) polling = false; else outstanding = false;
      if (!destroyed) render();
    }
    if (again) void request('sidecar_status');
  }
  // One side panel for the window: opening this view replaces the one shown, and Escape, its ×, the button or another
  // view take it away (left), which ends what it showed.
  let open = false;
  function left() {
    if (!open) return;
    open = false; generation++; button.setAttribute('aria-expanded', 'false'); render();
  }
  function show() { open = true; openSidePanel(panel, { opener: button, onLeave: left }); button.setAttribute('aria-expanded', 'true'); render(); close.focus(); void request('sidecar_status'); }
  function shut(returnFocus: boolean) {
    if (!open) return;
    closeSidePanel(panel); left();
    if (returnFocus) button.focus();
  }
  button.onclick = () => { if (shownNow()) shut(true); else show(); };
  close.onclick = () => shut(true);
  start.onclick = () => { void request('sidecar_start', true); };
  retry.onclick = () => { void request('sidecar_retry', true); };
  const shell = () => document.querySelector<HTMLElement>('#app') ?? document.body;
  /** A stop that would cut running agents off asks first, with what it costs; Keep running is its Cancel. */
  async function askStop() {
    const cost = current ? stopConsequence(current) : null;
    if (asking || destroyed || cost === null) return;
    asking = true; render();
    try {
      const yes = await confirm(shell(), { title: 'Stop the model proxy?', body: cost, confirm: 'Stop model proxy', cancel: 'Keep running', danger: true });
      // Agents that ended meanwhile need no forced stop, and one already stopped needs none at all.
      if (yes && !destroyed) void request('sidecar_stop', true, { force: confirmStillNeeded(current) });
    } finally { asking = false; if (!destroyed) render(); }
  }
  stop.onclick = () => {
    if (current && stopConsequence(current) !== null) { void askStop(); return; }
    void request('sidecar_stop', true, { force: false });
  };
  restartOffer.onclick = async () => {
    if (asking || restarting) return;
    asking = true; render();
    let yes = false;
    try {
      yes = await confirm(shell(), {
        title: 'Restart Shinatama?',
        body: 'Shinatama will close and start again. Running terminals stop; their sessions are restored at the next start.',
        confirm: 'Restart Shinatama', cancel: 'Keep waiting', danger: true,
      });
    } finally { asking = false; if (!destroyed) render(); }
    if (!yes || destroyed) return;
    restarting = true; render();
    // A refused restart closed nothing: say why and keep the panel as it was.
    try { await restart(); } catch (error) { notify(error instanceof Error ? error.message : String(error)); }
    finally { restarting = false; if (!destroyed) render(); }
  };
  render();
  if (desktop) void request('sidecar_status');
  const timer = desktop ? window.setInterval(() => { if (shownNow() || current?.phase !== 'stopped') void request('sidecar_status'); }, POLL_MS) : 0;
  return { destroy() { destroyed = true; window.clearInterval(timer); shut(false); button.remove(); panel.remove(); } };
}
