import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import '@acaprino/imago/css/imago.css';
import { portraitNotice } from '@acaprino/imago/ui';
import { applyLook } from '@acaprino/imago/shell';
import shinatamaSmile from '../../../docs/assets/widget/shinatama-smile-v1.png';
import shinatamaHappy from '../../../docs/assets/widget/shinatama-happy-v1.png';
import shinatamaCurious from '../../../docs/assets/widget/shinatama-curious-v1.png';

applyLook(document.documentElement, { app: 'shinatama', look: 'smooth', theme: 'dark', glass: false });
const { root, portrait, kind, title, body, error, open, dismiss } = portraitNotice({
  label: 'Avviso di Shinatama', name: 'Shinatama', dismiss: 'Chiudi avviso', open: 'Apri sessione',
});
root.id = 'attention';
portrait.id = 'shinatama-portrait';
for (const [name, node] of Object.entries({ kind, title, body, error, open, dismiss })) node.id = 'attention-' + name;
document.querySelector('#widget')!.replaceChildren(root);
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

type AttentionKind = 'input' | 'complete' | 'error' | 'update' | 'memory';
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

const kindLabels: Record<AttentionKind, string> = {
  input: 'Serve una tua risposta', complete: 'Processo terminato',
  error: 'Richiede attenzione', update: 'Aggiornamento', memory: 'Memoria del PC in esaurimento',
};
let current: AttentionNotice | null = null;
let refreshRevision = 0;
let actionPending = false;
let arrival: Animation | undefined;

function render(snapshot: AttentionSnapshot) {
  const notice = snapshot.enabled ? snapshot.current : null;
  if (!notice) {
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
    // Reading time belongs to the user. Only an explicit action dismisses a
    // notice; the backend can still replace resolved or superseded facts.
    arrival?.cancel();
    if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      arrival = portrait.animate([{ opacity: 0.65 }, { opacity: 1 }], { duration: 350, easing: 'ease-out' });
    }
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
// imago-check: allow own-keydown — the attention window is not a shell and has no registry: Escape dismisses its one notice.
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') { event.preventDefault(); void act('attention_dismiss'); }
});

// Subscribe before the first snapshot so an event between startup and hydration is retained.
void listen('attention-changed', () => { void refresh(); }).then((unlisten) => {
  window.addEventListener('pagehide', () => {
    unlisten(); arrival?.cancel();
  }, { once: true });
  return refresh();
}).catch(() => { void refresh(); });
