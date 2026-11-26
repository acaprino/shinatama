// The answers of the browser for the attention widget (widget/attention.ts). A test sets the notice with `setNotice` and says
// `emit('attention-changed')`, as Rust does when its feed changes (smoke: tests/widget.smoke.mjs).
import type { AttentionNotice } from '../attention-model.ts';
import type { Answers } from './index.ts';

export const state: { enabled: boolean; current: AttentionNotice | null; opened: string[]; dismissed: string[]; failNext: 'open' | 'dismiss' | 'current' | null } = {
  enabled: true, current: null, opened: [], dismissed: [], failNext: null,
};

export function setNotice(current: AttentionNotice | null): void {
  state.current = current;
}

function check(kind: 'open' | 'dismiss', id: string): void {
  if (state.failNext === kind) { state.failNext = null; throw 'The alert could not be answered.'; }
  if (state.current?.id !== id) throw 'This alert is no longer current.';
}

export const answers: Answers = {
  attention_current: () => {
    if (state.failNext === 'current') { state.failNext = null; throw 'busy'; }
    return { enabled: state.enabled, muted: false, current: state.current };
  },
  attention_open: args => { const id = String(args?.id); check('open', id); state.opened.push(id); state.current = null; return null; },
  attention_dismiss: args => { const id = String(args?.id); check('dismiss', id); state.dismissed.push(id); state.current = null; return null; },
};
