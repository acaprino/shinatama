// Exporting a session and handing it to another agent (a hop): the choices each offers, what the worker answers, and the words that say
// what was written or what will be sent. Pure.
import type { SessionRootSelector } from './session-library-model.ts';
import { digest, nativeIdValue } from './session-library-model.ts';

// --- export ------------------------------------------------------------------------------------------------------------------

export type ExportFormat = 'stx' | 'mukade' | 'md';
export type ExportMode = 'full' | 'portable';
export type ExportSelection = { root: SessionRootSelector; nativeId: string; fingerprint: string; harness: string };
export type ExportUiRequest = {
  root: SessionRootSelector; nativeId: string; fingerprint: string; format: ExportFormat; mode: ExportMode; withAttachments: boolean;
};
export type ExportReport = {
  items: number; redactions: Record<string, number>; omitted: { reasoning: number; meta: number; structured_results: number };
  attachmentsCopied: number; attachmentsNotCopied: number; attachmentsReferenced: number;
  declaration: { label: string; losses: string[]; lossless_backup: boolean };
};
export type ExportOutcome =
  | { state: 'saved'; path: string; bytes: number; sha256: string; attachments: number; report: ExportReport }
  | { state: 'cancelled' }
  | { state: 'refused'; issue: string };

/** What each format keeps and loses, said before anything is written. */
export const EXPORT_FORMATS: readonly { value: ExportFormat; label: string; detail: string }[] = [
  { value: 'stx', label: 'Shinatama STX (.stx.jsonl)', detail: 'Shinatama export v1: one item per line with provenance. Unknown fields are kept by readers; a newer version is refused.' },
  { value: 'mukade', label: 'Portable Mukade JSON', detail: 'Interchange document for other tools. Not a lossless backup; attachment content is not copied.' },
  { value: 'md', label: 'Markdown', detail: 'Readable transcript for people. Not machine-readable; provenance per item is not shown.' },
];
export const EXPORT_MODES: readonly { value: ExportMode; label: string; detail: string }[] = [
  { value: 'full', label: 'Full backup', detail: 'For you: keeps paths and addresses. Secrets are still redacted.' },
  { value: 'portable', label: 'Portable', detail: 'For sharing: also replaces your home folder with ~ and removes e-mail addresses.' },
];
export const EXPORT_ISSUES: Record<string, string> = {
  format_unavailable: 'This format is not available for this agent.', output_limit: 'The export exceeded its fixed size limit.',
  invalid_request: 'This export request is invalid.', source_changed: 'The session changed. Reload its detail and export again.',
  root_changed: 'The archive revision changed. Reload its catalog.', root_unavailable: 'The archive is unavailable. No other archive was used.',
  input_too_large: 'This archive holds more than 65,536 files.', session_not_found: 'The exact session is no longer listed.',
  unsupported_harness: 'This agent has no qualified session reader.', unsupported_export_context: 'This agent needs a separately qualified exporter.',
  cleanup_unconfirmed: 'Reader cleanup remains unconfirmed; its resources stay reserved.', read_unavailable: 'Export is unavailable. Retry explicitly.',
};

/** Whether a session may be exported or handed off: its fingerprint is a digest and its id is a native id. */
export function transferable(selection: Pick<ExportSelection, 'nativeId' | 'fingerprint'>): boolean {
  return digest(selection.fingerprint) && nativeIdValue(selection.nativeId);
}

/** The words of a refusal of an export: Rust's own for a failed write, ours for the rest. */
export function exportRefusal(issue: string): string {
  return issue.startsWith('write_failed:') ? issue.slice(13) : EXPORT_ISSUES[issue] ?? EXPORT_ISSUES.read_unavailable;
}

export function exportFailure(error: unknown): string {
  const code = typeof error === 'string' ? error : (error as { issue?: string } | null)?.issue ?? '';
  return EXPORT_ISSUES[code] ?? EXPORT_ISSUES.read_unavailable;
}

export const redactedText = (redactions: Record<string, number>): string =>
  Object.entries(redactions).map(([kind, count]) => `${kind} ${count}`).join(', ') || 'Nothing matched a secret pattern';

/** The facts of a file that was written. */
export function exportFacts(outcome: Extract<ExportOutcome, { state: 'saved' }>): { name: string; value: string; code?: boolean }[] {
  const { report } = outcome;
  return [
    { name: 'File', value: outcome.path, code: true },
    { name: 'Items', value: String(report.items) },
    { name: 'Redacted', value: redactedText(report.redactions) },
    { name: 'Left out', value: `${report.omitted.reasoning} reasoning, ${report.omitted.meta} metadata records` },
    { name: 'Attachments', value: `${report.attachmentsReferenced} referenced, ${outcome.attachments} copied` },
    { name: 'Losses', value: report.declaration.losses.join(' ') },
    { name: 'SHA-256', value: outcome.sha256, code: true },
  ];
}

// --- the hop -----------------------------------------------------------------------------------------------------------------

export type HopSelection = { root: SessionRootSelector; nativeId: string; fingerprint: string; harness: string };
type HopReport = {
  turns_total: number; verbatim_turns: number; digest_turns: number; omitted_turns: number; clipped_outputs: number;
  bytes: number; estimated_tokens: number; budget: { window_tokens: number | null; seed_tokens: number; seed_bytes: number } | null;
};
/** The exact seed a hop will send, and what is known about it (R06). */
export type HopPreview = {
  hop_id: string; nonce: string; destination: string; channel: string; seed: string; seed_sha256: string;
  bootstrap: string; bootstrap_sha256: string; seed_path: string; full_path: string; files_in_repository: boolean; report: HopReport;
  redactions: Record<string, number>; nested_seeds_dropped: number; injected_context_dropped: number; carried: string[]; lost: string[];
};
export type HopPreviewOutcome = { state: 'ready'; preview: HopPreview } | { state: 'refused'; issue: string; message: string };

/** The agents a session can be handed to, but its own. */
export function destinations(from: string, agents: readonly (readonly [string, string])[]): [string, string][] {
  return agents.filter(([id]) => id !== from).map(([id, label]) => [id, label]);
}

/** The facts of a preview, the ones the person checks before confirming. */
export function hopFacts(preview: HopPreview): { name: string; value: string; code?: boolean }[] {
  const r = preview.report;
  return [
    { name: 'Seed', value: `${r.bytes.toLocaleString()} bytes, about ${r.estimated_tokens.toLocaleString()} tokens of ${r.budget?.seed_tokens.toLocaleString() ?? '?'} allowed${r.budget?.window_tokens ? '' : ' (window unknown: 24,000 cap)'}` },
    { name: 'Turns', value: `${r.turns_total} total: ${r.verbatim_turns} verbatim, ${r.digest_turns} as digests, ${r.omitted_turns} omitted` },
    { name: 'Redacted', value: redactedText(preview.redactions) },
    { name: 'Files', value: `${preview.seed_path} and ${preview.full_path}${preview.files_in_repository ? ' (inside .git, never tracked)' : ''}`, code: true },
    { name: 'SHA-256', value: preview.seed_sha256, code: true },
  ];
}
