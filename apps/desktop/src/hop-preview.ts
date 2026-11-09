import { openModal } from './modal.ts';
import type { SessionRootSelector } from './session-library.ts';
import './hop-preview.css';

export type HopSelection = { root: SessionRootSelector; nativeId: string; fingerprint: string; harness: string };
type Report = { turns_total: number; verbatim_turns: number; digest_turns: number; omitted_turns: number; clipped_outputs: number;
  bytes: number; estimated_tokens: number; budget: { window_tokens: number | null; seed_tokens: number; seed_bytes: number } | null };
export type HopPreview = { hop_id: string; nonce: string; destination: string; channel: string; seed: string; seed_sha256: string;
  bootstrap: string; bootstrap_sha256: string; seed_path: string; full_path: string; files_in_repository: boolean; report: Report;
  redactions: Record<string, number>; nested_seeds_dropped: number; injected_context_dropped: number; carried: string[]; lost: string[] };
export type HopPreviewOutcome = { state: 'ready'; preview: HopPreview } | { state: 'refused'; issue: string; message: string };
type Options = {
  prepare: (request: { root: SessionRootSelector; nativeId: string; fingerprint: string; destination: string }) => Promise<HopPreviewOutcome>;
  send?: (preview: HopPreview, progress: (message: string) => void, release: () => void) => Promise<string>;
  reportError: (message: string) => void; restoreFocus?: () => void };

const AGENTS: [string, string][] = [['claude_code', 'Claude Code'], ['codex', 'Codex'], ['opencode', 'OpenCode'], ['pi', 'Pi'], ['hermes', 'Hermes Agent'], ['deepseek', 'DeepSeek Harness']];

/** The preview is the exact seed: what is shown here is byte for byte what a hop sends (R06). */
export function createHopPreview(options: Options) {
  const dialog = document.createElement('dialog'); dialog.id = 'hop-preview-dialog'; dialog.setAttribute('aria-labelledby', 'hop-preview-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'hop-preview-title'; title.textContent = 'Hop to another agent';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close hop preview'); heading.append(title, closeButton);
  const source = document.createElement('p'); source.className = 'hint';
  const row = document.createElement('div'); row.className = 'hop-preview-row';
  const label = document.createElement('label'); label.textContent = 'Destination ';
  const select = document.createElement('select'); label.append(select);
  const prepare = document.createElement('button'); prepare.type = 'button'; prepare.className = 'primary'; prepare.textContent = 'Prepare preview';
  row.append(label, prepare);
  const status = document.createElement('p'); status.className = 'hop-preview-status'; status.setAttribute('role', 'status');
  const facts = document.createElement('dl'); facts.className = 'hop-preview-facts';
  const lists = document.createElement('div'); lists.className = 'hop-preview-lists';
  const seed = document.createElement('pre'); seed.className = 'hop-preview-seed'; seed.tabIndex = 0; seed.setAttribute('aria-label', 'Exact seed');
  const actions = document.createElement('div'); actions.className = 'hop-preview-actions';
  const copy = document.createElement('button'); copy.type = 'button'; copy.textContent = 'Copy hand-off prompt';
  const send = document.createElement('button'); send.type = 'button'; send.className = 'primary'; send.textContent = 'Hop';
  actions.append(copy); if (options.send) actions.append(send);
  const hint = document.createElement('p'); hint.className = 'hint';
  hint.textContent = 'Nothing is sent until you confirm. The prompt tells the destination to read the private seed file; reasoning is never included.';
  dialog.append(heading, source, row, status, facts, lists, seed, actions, hint);
  (document.querySelector('#app') ?? document.body).append(dialog);
  let selection: HopSelection | null = null; let current: HopPreview | null = null; let busy = false; let generation = 0;
  function fact(name: string, value: string) { const k = document.createElement('dt'); k.textContent = name; const v = document.createElement('dd'); v.textContent = value; facts.append(k, v); }
  function list(name: string, items: string[]) {
    const section = document.createElement('section'); const h = document.createElement('h2'); h.textContent = name; const ul = document.createElement('ul');
    for (const item of items) { const li = document.createElement('li'); li.textContent = item; ul.append(li); }
    section.append(h, ul); lists.append(section);
  }
  function render() {
    prepare.disabled = busy || !selection || !select.value; select.disabled = busy;
    copy.disabled = busy || !current; send.disabled = busy || !current;
    facts.replaceChildren(); lists.replaceChildren(); seed.textContent = current?.seed ?? '';
    seed.hidden = !current; actions.hidden = !current;
    if (current) {
      const r = current.report; const redacted = Object.entries(current.redactions).map(([k, v]) => `${k} ${v}`).join(', ');
      fact('Seed', `${r.bytes.toLocaleString()} bytes, about ${r.estimated_tokens.toLocaleString()} tokens of ${r.budget?.seed_tokens.toLocaleString() ?? '?'} allowed${r.budget?.window_tokens ? '' : ' (window unknown: 24,000 cap)'}`);
      fact('Turns', `${r.turns_total} total: ${r.verbatim_turns} verbatim, ${r.digest_turns} as digests, ${r.omitted_turns} omitted`);
      fact('Redacted', redacted || 'Nothing matched a secret pattern');
      fact('Files', `${current.seed_path} and ${current.full_path}${current.files_in_repository ? ' (inside .git, never tracked)' : ''}`);
      fact('SHA-256', current.seed_sha256);
      list('Carried', current.carried); list('Lost', current.lost);
    }
  }
  prepare.onclick = async () => {
    if (busy || !selection) return;
    const at = ++generation; busy = true; current = null; status.dataset.state = 'ready'; status.textContent = 'Reading, redacting and budgeting the conversation…'; render();
    try {
      const outcome = await options.prepare({ root: { ...selection.root }, nativeId: selection.nativeId, fingerprint: selection.fingerprint, destination: select.value });
      if (at !== generation || !dialog.open) return;
      if (outcome.state === 'refused') { status.dataset.state = 'error'; status.textContent = outcome.message; return; }
      current = outcome.preview; status.textContent = 'This is exactly what the destination receives.';
    } catch (error) {
      if (at !== generation || !dialog.open) return;
      status.dataset.state = 'error'; status.textContent = 'The preview could not be prepared. Retry explicitly.'; options.reportError(status.textContent);
    } finally { if (at === generation) { busy = false; render(); } }
  };
  copy.onclick = async () => {
    if (!current) return;
    try { await navigator.clipboard.writeText(current.bootstrap); status.textContent = 'Hand-off prompt copied. Paste it into the destination agent; your clipboard may be visible to other applications.'; }
    catch { status.dataset.state = 'error'; status.textContent = 'The clipboard is unavailable.'; }
  };
  send.onclick = async () => {
    if (!current || !options.send || busy) return;
    const at = ++generation; busy = true; render();
    const progress = (message: string) => { if (at === generation && dialog.open) { status.dataset.state = 'ready'; status.textContent = message; } };
    // The destination may need the user (a folder-trust question): free the screen for it.
    const release = () => { if (at === generation && dialog.open) dialog.close(); };
    try { const done = await options.send(current, progress, release); progress(done); if (at === generation) current = null; }
    catch (error) { if (at === generation) { status.dataset.state = 'error'; status.textContent = typeof error === 'string' ? error : 'The hop could not start; nothing changed in the source.'; } }
    finally { if (at === generation) { busy = false; render(); } }
  };
  select.onchange = () => { current = null; status.textContent = ''; render(); };
  closeButton.onclick = () => dialog.close();
  dialog.addEventListener('close', () => { generation++; busy = false; selection = null; current = null; render(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  function show(value: HopSelection) {
    if (!/^[a-f0-9]{64}$/i.test(value.fingerprint)) { options.reportError('This session cannot be handed off safely.'); return; }
    selection = { ...value, root: { ...value.root } }; current = null;
    select.replaceChildren();
    for (const [id, name] of AGENTS) { if (id === value.harness) continue; const option = document.createElement('option'); option.value = id; option.textContent = name; select.append(option); }
    source.textContent = `From ${value.harness} · session ${value.nativeId}`; status.dataset.state = 'ready'; status.textContent = 'Choose a destination, then Prepare preview.';
    render(); if (!dialog.open) openModal(dialog); select.focus();
  }
  render();
  return { show, close: () => dialog.close() };
}
