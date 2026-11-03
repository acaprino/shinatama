import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './attention-widget.css';
import shinatamaSmile from '../../../docs/assets/widget/shinatama-smile-v1.png';
import shinatamaHappy from '../../../docs/assets/widget/shinatama-happy-v1.png';
import shinatamaCurious from '../../../docs/assets/widget/shinatama-curious-v1.png';

const portrait = document.querySelector<HTMLImageElement>('#shinatama-portrait')!;
const expressions = [
  { name: 'smile', src: shinatamaSmile },
  { name: 'happy', src: shinatamaHappy },
  { name: 'curious', src: shinatamaCurious },
] as const;
let expressionIndex = 0;
let portraitNoticeId: string | null = null;
portrait.src = expressions[expressionIndex].src;
portrait.dataset.expression = expressions[expressionIndex].name;

function selectExpression(noticeId: string) {
  if (noticeId === portraitNoticeId) return;
  // Start with the smile, then choose uniformly from the other expressions.
  if (portraitNoticeId !== null) {
    const other = Math.floor(Math.random() * (expressions.length - 1));
    expressionIndex = other >= expressionIndex ? other + 1 : other;
  }
  portraitNoticeId = noticeId;
  portrait.src = expressions[expressionIndex].src;
  portrait.dataset.expression = expressions[expressionIndex].name;
}

type AttentionKind = 'input' | 'complete' | 'error' | 'update';
interface AttentionNotice {
  id: string;
  kind: AttentionKind;
  title: string;
  body: string;
  sessionId: string | null;
  createdAt: number;
}
interface AttentionSnapshot {
  enabled: boolean;
  muted: boolean;
  current: AttentionNotice | null;
}

const root = document.querySelector<HTMLElement>('#attention')!;
const kind = document.querySelector<HTMLElement>('#attention-kind')!;
const title = document.querySelector<HTMLElement>('#attention-title')!;
const body = document.querySelector<HTMLElement>('#attention-body')!;
const error = document.querySelector<HTMLElement>('#attention-error')!;
const open = document.querySelector<HTMLButtonElement>('#attention-open')!;
const dismiss = document.querySelector<HTMLButtonElement>('#attention-dismiss')!;
const kindLabels: Record<AttentionKind, string> = {
  input: 'Serve una tua risposta', complete: 'Processo terminato',
  error: 'Richiede attenzione', update: 'Aggiornamento',
};
let current: AttentionNotice | null = null;
let refreshRevision = 0;
let actionPending = false;
let animationTimer: ReturnType<typeof setTimeout> | undefined;
let dismissTimer: ReturnType<typeof setTimeout> | undefined;

function render(snapshot: AttentionSnapshot) {
  const notice = snapshot.enabled ? snapshot.current : null;
  if (!notice) {
    clearTimeout(dismissTimer);
    current = null;
    root.hidden = true;
    return;
  }
  const changed = notice.id !== current?.id;
  current = notice;
  root.dataset.kind = notice.kind;
  kind.textContent = kindLabels[notice.kind] || kindLabels.update;
  title.textContent = notice.title;
  body.textContent = notice.body;
  title.title = notice.title;
  body.title = notice.body;
  open.firstChild!.textContent = notice.sessionId ? 'Apri sessione ' : 'Apri Shinatama ';
  if (changed) {
    selectExpression(notice.id);
    error.hidden = true;
    clearTimeout(dismissTimer);
    if (notice.kind === 'complete' || notice.kind === 'update') {
      dismissTimer = setTimeout(() => {
        if (current?.id === notice.id) void act('attention_dismiss');
      }, 15_000);
    }
    root.classList.remove('arriving');
    clearTimeout(animationTimer);
    // Restart only the brief expression when a different notice arrives.
    requestAnimationFrame(() => {
      if (current?.id !== notice.id) return;
      root.classList.add('arriving');
      animationTimer = setTimeout(() => root.classList.remove('arriving'), 1800);
    });
  }
  root.hidden = false;
}

async function refresh() {
  const revision = ++refreshRevision;
  try {
    const snapshot = await invoke<AttentionSnapshot>('attention_current');
    if (revision === refreshRevision) render(snapshot);
  } catch {
    // Keep an existing notice readable if the main process is briefly busy.
    if (current) {
      error.textContent = 'Avviso non aggiornato.';
      error.hidden = false;
    }
  }
}

async function act(command: 'attention_open' | 'attention_dismiss') {
  if (!current || actionPending) return;
  const id = current.id;
  actionPending = true;
  open.disabled = true;
  dismiss.disabled = true;
  error.hidden = true;
  try {
    await invoke(command, { id });
    await refresh();
  } catch {
    if (current?.id === id) {
      error.textContent = command === 'attention_open' ? 'Apri la sessione in Shinatama.' : 'Riprova a chiudere l’avviso.';
      error.hidden = false;
    }
  } finally {
    actionPending = false;
    open.disabled = false;
    dismiss.disabled = false;
  }
}

open.addEventListener('click', () => { void act('attention_open'); });
dismiss.addEventListener('click', () => { void act('attention_dismiss'); });
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') { event.preventDefault(); void act('attention_dismiss'); }
});

// Subscribe before the first snapshot so an event between startup and hydration is retained.
void listen('attention-changed', () => { void refresh(); }).then((unlisten) => {
  window.addEventListener('pagehide', () => {
    unlisten(); clearTimeout(animationTimer); clearTimeout(dismissTimer);
  }, { once: true });
  return refresh();
}).catch(() => { void refresh(); });
