import { openModal, topModal } from './modal.ts';
import type { DetailRequest } from './session-detail.ts';
import type { SessionRootSelector } from './session-library.ts';
import './session-raw.css';

export type RawSelection = { root: SessionRootSelector; nativeId: string; entryOrdinal: number; fingerprint: string };
export type RawRequest = { detail: DetailRequest; entryOrdinal: number; confirmedRaw: boolean };
export type RawIssue = { code: 'confirmation_required' | 'fingerprint_required' | 'entry_not_found' | 'raw_unavailable' | 'output_limit' }
  | { code: 'detail'; detail: string };
export type RawReply = { root: SessionRootSelector; nativeId: string; entryOrdinal: number;
  sourceFingerprint: string | null; rawJson: string | null; issue: RawIssue | null };
type Options = { read: (request: RawRequest) => Promise<RawReply>; reportError: (message: string) => void; restoreFocus?: () => void };
const MAX_RAW_BYTES = 64 * 1024;
const encoder = new TextEncoder();
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const digest = (value: unknown): value is string => typeof value === 'string' && /^[a-f0-9]{64}$/i.test(value);
const nativeId = (value: unknown): value is string => typeof value === 'string' && /^[a-z0-9_-]{1,256}$/i.test(value);
const rootValue = (value: unknown): value is SessionRootSelector => record(value) && typeof value.storeRootId === 'string'
  && value.storeRootId.length > 0 && value.storeRootId.length <= 256 && !/[\u0000-\u001f\u007f]/.test(value.storeRootId)
  && integer(value.configurationRevision) && value.configurationRevision > 0;
const sameRoot = (left: SessionRootSelector, right: SessionRootSelector) => left.storeRootId === right.storeRootId
  && left.configurationRevision === right.configurationRevision;
const ERRORS: Record<string, string> = {
  confirmation_required: 'Confirm the privacy warning before loading RAW.', fingerprint_required: 'Reload the session detail before choosing RAW.',
  entry_not_found: 'This exact entry is unavailable. Reload the session detail.', raw_unavailable: 'The native reader has no RAW record for this canonical entry.',
  output_limit: 'The RAW entry exceeds 64 KiB and was refused.', source_changed: 'The source changed. Close RAW and reload the detail.',
  root_changed: 'The archive revision changed. Close RAW and reload the catalog.', root_unavailable: 'The selected archive is unavailable.',
  input_too_large: 'This archive holds more than 4,096 files. It was refused before reading.',
  unsafe_namespace: 'The source failed read-only path verification.', unsupported_export_context: 'This native export context is not qualified.',
  unsupported_harness: 'This agent has no qualified RAW reader.', ambiguous_session: 'This exact session cannot be distinguished safely.',
  session_not_found: 'The exact native session is unavailable in this archive.', read_unavailable: 'RAW could not be read. Retry explicitly.',
  invalid_request: 'This RAW selector is invalid.',
};
function decode(value: unknown, selection: RawSelection): string {
  if (!record(value) || !rootValue(value.root) || !sameRoot(value.root, selection.root) || value.nativeId !== selection.nativeId
    || value.entryOrdinal !== selection.entryOrdinal) throw new Error('read_unavailable');
  if (value.issue !== null) {
    if (!record(value.issue)) throw new Error('read_unavailable');
    const code = value.issue.code === 'detail' ? value.issue.detail : value.issue.code;
    throw new Error(typeof code === 'string' && Object.hasOwn(ERRORS, code) ? code : 'read_unavailable');
  }
  if (value.sourceFingerprint !== selection.fingerprint) throw new Error('source_changed');
  if (typeof value.rawJson !== 'string' || !value.rawJson.length || value.rawJson.length > MAX_RAW_BYTES
    || encoder.encode(value.rawJson).length > MAX_RAW_BYTES) throw new Error('output_limit');
  return value.rawJson;
}

/** Opening never reads RAW. Only a checked warning plus an explicit Load click
 * invokes the injected contained reader. No cache, logging, HTML or navigation. */
export function createSessionRaw(options: Options) {
  const dialog = document.createElement('dialog'); dialog.id = 'session-raw-dialog'; dialog.setAttribute('aria-labelledby', 'session-raw-title');
  const heading = document.createElement('div'); heading.className = 'dialog-heading';
  const title = document.createElement('h1'); title.id = 'session-raw-title'; title.textContent = 'RAW native entry';
  const closeButton = document.createElement('button'); closeButton.type = 'button'; closeButton.textContent = '×'; closeButton.setAttribute('aria-label', 'Close RAW entry');
  heading.append(title, closeButton);
  const warning = document.createElement('p'); warning.id = 'session-raw-warning'; warning.className = 'session-raw-warning';
  warning.textContent = 'RAW may contain credentials, private reasoning, signed or encrypted data, and other sensitive native fields. Nothing is hidden or redacted in this view. Load only if you intend to inspect this entry.';
  const consent = document.createElement('label'); consent.className = 'session-raw-consent';
  const confirmed = document.createElement('input'); confirmed.type = 'checkbox'; confirmed.id = 'session-raw-confirm'; confirmed.setAttribute('aria-describedby', warning.id);
  const consentText = document.createElement('span'); consentText.textContent = 'I understand and want to load this one RAW entry.'; consent.append(confirmed, consentText);
  const source = document.createElement('p'); source.className = 'hint session-raw-source';
  const actions = document.createElement('div'); actions.className = 'session-raw-actions';
  const load = document.createElement('button'); load.type = 'button'; load.id = 'session-raw-load'; load.textContent = 'Load this RAW entry';
  const copy = document.createElement('button'); copy.type = 'button'; copy.id = 'session-raw-copy'; copy.textContent = 'Copy visible RAW'; actions.append(load, copy);
  const status = document.createElement('p'); status.className = 'session-raw-status'; status.setAttribute('role', 'status');
  const content = document.createElement('pre'); content.id = 'session-raw-content'; content.tabIndex = 0; content.setAttribute('aria-label', 'Visible RAW native record');
  const scope = document.createElement('p'); scope.className = 'hint'; scope.textContent = 'One entry, at most 64 KiB. Attachment files stay closed. Closing clears this view. Copy writes only the displayed RAW to your clipboard.';
  dialog.append(heading, warning, source, consent, actions, status, content, scope); (document.querySelector('#app') ?? document.body).append(dialog);
  let selected: RawSelection | null = null; let generation = 0; let destroyed = false; let busy = false; let copying = false;
  let message = 'RAW has not been requested.'; let failed = false;
  function render() {
    if (destroyed) return;
    dialog.setAttribute('aria-busy', String(busy)); confirmed.disabled = busy;
    load.disabled = busy || !selected || !confirmed.checked; copy.disabled = busy || copying || !confirmed.checked || !content.textContent;
    status.textContent = busy ? 'Reading this confirmed RAW entry…' : message; status.dataset.state = failed ? 'error' : 'ready';
    source.textContent = selected ? `Session ${selected.nativeId} · Entry ${selected.entryOrdinal} · Archive ${selected.root.storeRootId}` : '';
    content.hidden = !content.textContent;
  }
  function clear() {
    generation++; selected = null; confirmed.checked = false; content.textContent = ''; message = 'RAW has not been requested.'; failed = false;
    render();
  }
  function close() { clear(); if (dialog.open) dialog.close(); }
  async function read() {
    if (busy || !selected || !confirmed.checked || destroyed) return;
    const at = generation; const selection = selected;
    const request: RawRequest = { detail: { root: { ...selection.root }, nativeId: selection.nativeId,
      cursor: { offset: 0, fingerprint: selection.fingerprint }, limit: 50 }, entryOrdinal: selection.entryOrdinal, confirmedRaw: true };
    busy = true; content.textContent = ''; failed = false; render();
    try {
      const reply = await options.read(request);
      if (destroyed || at !== generation || !dialog.open || !confirmed.checked) return;
      content.textContent = decode(reply, selection); message = 'Only this native entry is shown. RAW is not saved by this view.';
    } catch (error) {
      if (!destroyed && at === generation && dialog.open) {
        content.textContent = ''; failed = true;
        const code = error instanceof Error ? error.message : null;
        message = typeof code === 'string' && Object.hasOwn(ERRORS, code) ? ERRORS[code] : ERRORS.read_unavailable;
        options.reportError(message);
      }
    } finally {
      busy = false;
      if (!destroyed && at !== generation && dialog.open) message = 'The previous read finished. Confirm and load again explicitly.';
      render();
    }
  }
  async function copyVisible() {
    if (busy || copying || !dialog.open || !confirmed.checked || !content.textContent || destroyed) return;
    const visible = content.textContent; const at = generation; copying = true; render();
    try {
      await navigator.clipboard.writeText(visible);
      if (!destroyed && at === generation && dialog.open) message = 'Visible RAW copied. Your clipboard may be available to other applications.';
    } catch {
      if (!destroyed && at === generation && dialog.open) {
        content.focus(); const range = document.createRange(); range.selectNodeContents(content);
        const selection = window.getSelection(); selection?.removeAllRanges(); selection?.addRange(range);
        message = 'Clipboard access is unavailable. Visible text is selected; press Ctrl+C to copy it yourself.';
      }
    } finally { copying = false; render(); }
  }
  function show(selection: RawSelection) {
    if (destroyed || !record(selection) || !rootValue(selection.root) || !nativeId(selection.nativeId) || !integer(selection.entryOrdinal) || !digest(selection.fingerprint)) {
      if (!destroyed) options.reportError('This RAW selector is invalid. Reload the session detail.'); return false;
    }
    const other = topModal();
    if (!dialog.open && other && other.id !== 'session-detail-dialog') return false;
    clear(); selected = { root: { ...selection.root }, nativeId: selection.nativeId, entryOrdinal: selection.entryOrdinal, fingerprint: selection.fingerprint };
    message = busy ? 'The previous read is still returning; no replacement was started.' : 'Confirm the privacy warning before loading RAW.';
    render(); if (!dialog.open) openModal(dialog); closeButton.focus(); return true;
  }
  confirmed.onchange = () => { if (!confirmed.checked) { generation++; content.textContent = ''; message = 'RAW has been cleared.'; } render(); };
  load.onclick = () => { void read(); }; copy.onclick = () => { void copyVisible(); }; closeButton.onclick = close;
  dialog.addEventListener('close', () => { if (!dialog.open) { clear(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); } });
  render(); return { show, close, destroy: () => { close(); destroyed = true; dialog.remove(); } };
}
