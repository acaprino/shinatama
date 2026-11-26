// What the attention widget shows: the notices of `attention.rs` as the page reads them, the words of each kind (Italian, as they have been),
// and which of the portrait's expressions comes next. Pure: no React, no Tauri.

export type AttentionKind = 'input' | 'complete' | 'error' | 'update' | 'memory';

/** A notice as Rust sends it (`attention::Notice`, camel case). */
export interface AttentionNotice {
  readonly id: string;
  readonly kind: AttentionKind;
  readonly title: string;
  readonly body: string;
  /** The session the notice is about, when it is about one: "Open" brings its tab forward. */
  readonly sessionId: string | null;
  readonly createdAt: number;
}

/** What `attention_current` answers (`attention::Snapshot`). */
export interface AttentionSnapshot {
  readonly enabled: boolean;
  readonly muted: boolean;
  readonly current: AttentionNotice | null;
}

export type AttentionTone = 'success' | 'warning' | 'danger' | 'info';

const KINDS: readonly AttentionKind[] = ['input', 'complete', 'error', 'update', 'memory'];

/** The words and the tone of each kind. An unknown kind (a newer Rust) is read as an update. */
export const KINDS_SAY: Readonly<Record<AttentionKind, { readonly label: string; readonly tone: AttentionTone }>> = {
  input: { label: 'Serve una tua risposta', tone: 'warning' },
  complete: { label: 'Processo terminato', tone: 'success' },
  error: { label: 'Richiede attenzione', tone: 'danger' },
  update: { label: 'Aggiornamento', tone: 'info' },
  memory: { label: 'Memoria del PC in esaurimento', tone: 'warning' },
};

export const WIDGET_WORDS = {
  name: 'Shinatama',
  window: 'Avviso di Shinatama',
  dismiss: 'Chiudi avviso',
  openSession: 'Apri sessione',
  openApp: 'Apri Shinatama',
  stale: 'Avviso non aggiornato.',
  openFailed: 'Apri la sessione in Shinatama.',
  dismissFailed: 'Riprova a chiudere l’avviso.',
} as const;

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value);

function noticeOf(value: unknown): AttentionNotice | null {
  if (!isRecord(value) || typeof value.id !== 'string' || typeof value.title !== 'string' || typeof value.body !== 'string') return null;
  const kind = KINDS.find(one => one === value.kind) ?? 'update';
  return {
    id: value.id, kind, title: value.title, body: value.body,
    sessionId: typeof value.sessionId === 'string' ? value.sessionId : null,
    createdAt: typeof value.createdAt === 'number' ? value.createdAt : 0,
  };
}

/** What Rust answered, checked: anything else is "no notice". */
export function parseSnapshot(value: unknown): AttentionSnapshot {
  if (!isRecord(value)) return { enabled: false, muted: false, current: null };
  return { enabled: value.enabled === true, muted: value.muted === true, current: noticeOf(value.current) };
}

/** The notice to draw: none while the alerts are off. */
export const noticeShown = (snapshot: AttentionSnapshot): AttentionNotice | null => (snapshot.enabled ? snapshot.current : null);

/** The label of the button that answers a notice: a session's, or the app's. */
export const openLabel = (notice: Pick<AttentionNotice, 'sessionId'>): string => (notice.sessionId ? WIDGET_WORDS.openSession : WIDGET_WORDS.openApp);

export const EXPRESSIONS = ['smile', 'happy', 'curious'] as const;
export type Expression = (typeof EXPRESSIONS)[number];

/** The expression of a new notice: the first one a page shows is the smile, then one of the others, never the same twice running. */
export function nextExpression(current: Expression, first: boolean, random: () => number): Expression {
  if (first) return 'smile';
  const others = EXPRESSIONS.filter(one => one !== current);
  return others[Math.min(others.length - 1, Math.floor(random() * others.length))]!;
}
