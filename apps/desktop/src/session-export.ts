import { uiPart, uiButton, uiInput } from '@acaprino/imago/ui';
import { sidePanelOpen, sideSurface } from './side-panel.ts';
import type { SessionRootSelector } from './session-library.ts';
import { createProgressLine, type ProgressBackend } from './session-progress.ts';

export type ExportFormat = 'stx' | 'mukade' | 'md';
export type ExportMode = 'full' | 'portable';
export type ExportSelection = { root: SessionRootSelector; nativeId: string; fingerprint: string; harness: string };
export type ExportUiRequest = { root: SessionRootSelector; nativeId: string; fingerprint: string;
  format: ExportFormat; mode: ExportMode; withAttachments: boolean };
type Report = { items: number; redactions: Record<string, number>; omitted: { reasoning: number; meta: number; structured_results: number };
  attachmentsCopied: number; attachmentsNotCopied: number; attachmentsReferenced: number;
  declaration: { label: string; losses: string[]; lossless_backup: boolean } };
export type ExportOutcome = { state: 'saved'; path: string; bytes: number; sha256: string; attachments: number; report: Report }
  | { state: 'cancelled' } | { state: 'refused'; issue: string };
type Options = { save: (request: ExportUiRequest, requestId: string) => Promise<ExportOutcome>; reportError: (message: string) => void; restoreFocus?: () => void;
  progress?: ProgressBackend };

/** Declarations shown before writing (FRD-0006 R31). Native export is not offered until proven. */
const FORMATS: { value: ExportFormat; label: string; detail: string }[] = [
  { value: 'stx', label: 'Shinatama STX (.stx.jsonl)', detail: 'Shinatama export v1: one item per line with provenance. Unknown fields are kept by readers; a newer version is refused.' },
  { value: 'mukade', label: 'Portable Mukade JSON', detail: 'Interchange document for other tools. Not a lossless backup; attachment content is not copied.' },
  { value: 'md', label: 'Markdown', detail: 'Readable transcript for people. Not machine-readable; provenance per item is not shown.' },
];
const ISSUES: Record<string, string> = {
  format_unavailable: 'This format is not available for this agent.', output_limit: 'The export exceeded its fixed size limit.',
  invalid_request: 'This export request is invalid.', source_changed: 'The session changed. Reload its detail and export again.',
  root_changed: 'The archive revision changed. Reload its catalog.', root_unavailable: 'The archive is unavailable. No other archive was used.',
  input_too_large: 'This archive holds more than 65,536 files.', session_not_found: 'The exact session is no longer listed.',
  unsupported_harness: 'This agent has no qualified session reader.', unsupported_export_context: 'This agent needs a separately qualified exporter.',
  cleanup_unconfirmed: 'Reader cleanup remains unconfirmed; its resources stay reserved.', read_unavailable: 'Export is unavailable. Retry explicitly.',
};

export function createSessionExport(options: Options) {
  const view = sideSurface('session-export-dialog', 'Export session', 'Close export', 'session-export-title');
  const source = uiPart('hint');
  const formatSet = uiPart('fieldset'); const formatLegend = uiPart('legend', 'Format'); formatSet.append(formatLegend);
  const radios = new Map<ExportFormat, HTMLInputElement>();
  for (const format of FORMATS) {
    const label = uiPart('label');
    const input = uiInput('radio'); input.name = 'session-export-format'; input.value = format.value; input.checked = format.value === 'stx';
    const text = uiPart('text'); const strong = uiPart('strong', format.label);
    const small = uiPart('hint', format.detail); text.append(strong, small); label.append(input, text);
    formatSet.append(label); radios.set(format.value, input); input.onchange = render;
  }
  const native = uiPart('hint', "The agent's own export is not offered yet: no native export has been proven for this agent.");
  formatSet.append(native);
  const modeSet = uiPart('fieldset'); const modeLegend = uiPart('legend', 'Privacy'); modeSet.append(modeLegend);
  const modes = new Map<ExportMode, HTMLInputElement>();
  for (const [value, label, detail] of [['full', 'Full backup', 'For you: keeps paths and addresses. Secrets are still redacted.'],
    ['portable', 'Portable', 'For sharing: also replaces your home folder with ~ and removes e-mail addresses.']] as [ExportMode, string, string][]) {
    const row = uiPart('label');
    const input = uiInput('radio'); input.name = 'session-export-mode'; input.value = value; input.checked = value === 'full';
    const text = uiPart('text'); const strong = uiPart('strong', label);
    const small = uiPart('hint', detail); text.append(strong, small); row.append(input, text); modeSet.append(row); modes.set(value, input);
  }
  const attachRow = uiPart('label');
  const attach = uiInput('checkbox');
  const attachText = uiPart('text', 'Copy attachment content beside the file (inline images only). Otherwise attachments are hashed references.');
  attachRow.append(attach, attachText);
  const always = uiPart('hint', 'Reasoning is never exported. The file is written to a temporary name and renamed when complete.');
  const actions = uiPart('actions');
  const save = uiButton('Save as…', { primary: true }); actions.append(save);
  const status = uiPart('status');
  const progress = createProgressLine('export', options.progress);
  const report = uiPart('descriptionList');
  view.body.append(source, formatSet, modeSet, attachRow, always, actions, status, progress.element, report);
  let selection: ExportSelection | null = null; let busy = false; let generation = 0;
  const chosenFormat = (): ExportFormat => [...radios].find(([, input]) => input.checked)?.[0] ?? 'stx';
  const chosenMode = (): ExportMode => [...modes].find(([, input]) => input.checked)?.[0] ?? 'full';
  function render() {
    save.disabled = busy || !selection; attach.disabled = busy || chosenFormat() === 'mukade';
    if (chosenFormat() === 'mukade') attach.checked = false;
    for (const input of [...radios.values(), ...modes.values()]) input.disabled = busy;
    source.textContent = selection ? `${selection.harness} · session ${selection.nativeId}` : '';
    view.root.setAttribute('aria-busy', String(busy));
  }
  function fact(label: string, value: string) {
    const key = uiPart('term', label); const item = uiPart('definition', value); report.append(key, item);
  }
  save.onclick = async () => {
    if (busy || !selection) return;
    const at = ++generation; const requestId = progress.begin(); busy = true; report.replaceChildren(); status.dataset.state = 'ready'; status.textContent = 'Reading and redacting the session…'; render();
    try {
      const outcome = await options.save({ root: { ...selection.root }, nativeId: selection.nativeId, fingerprint: selection.fingerprint,
        format: chosenFormat(), mode: chosenMode(), withAttachments: attach.checked }, requestId);
      if (at !== generation || !view.isOpen()) return;
      if (outcome.state === 'cancelled') { status.textContent = progress.cancelled() ? 'Export cancelled. Nothing was written.' : 'Nothing was written.'; return; }
      if (outcome.state === 'refused') {
        const code = outcome.issue.startsWith('write_failed:') ? outcome.issue.slice(13) : ISSUES[outcome.issue] ?? ISSUES.read_unavailable;
        status.dataset.state = 'error'; status.textContent = code; options.reportError(code); return;
      }
      const r = outcome.report; const redacted = Object.entries(r.redactions).map(([k, v]) => `${k} ${v}`).join(', ');
      status.textContent = `Saved ${outcome.bytes.toLocaleString()} bytes.`;
      fact('File', outcome.path); fact('Items', String(r.items)); fact('Redacted', redacted || 'Nothing matched a secret pattern');
      fact('Left out', `${r.omitted.reasoning} reasoning, ${r.omitted.meta} metadata records`);
      fact('Attachments', `${r.attachmentsReferenced} referenced, ${outcome.attachments} copied`);
      fact('Losses', r.declaration.losses.join(' ')); fact('SHA-256', outcome.sha256);
    } catch (error) {
      if (at !== generation || !view.isOpen()) return;
      const code = typeof error === 'string' ? error : (error as { issue?: string })?.issue ?? '';
      status.dataset.state = 'error'; status.textContent = ISSUES[code] ?? ISSUES.read_unavailable; options.reportError(status.textContent);
    } finally { progress.end(requestId); if (at === generation) { busy = false; render(); } }
  };
  view.onClose(() => { progress.cancel(); generation++; busy = false; selection = null; report.replaceChildren(); status.textContent = ''; render(); if (!sidePanelOpen()) options.restoreFocus?.(); });
  function show(value: ExportSelection) {
    if (!/^[a-f0-9]{64}$/i.test(value.fingerprint) || !/^[a-z0-9_-]{1,256}$/i.test(value.nativeId)) { options.reportError('This session cannot be exported safely.'); return; }
    selection = { root: { ...value.root }, nativeId: value.nativeId, fingerprint: value.fingerprint, harness: value.harness };
    report.replaceChildren(); status.textContent = 'Choose a format, then Save as…'; status.dataset.state = 'ready'; render();
    if (!view.isOpen()) view.open(); save.focus();
  }
  render();
  return { show, close: () => view.close() };
}
