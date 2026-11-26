// The attention widget: what it reads from Rust, which notice it shows, the portrait's expression, and its two answers.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { EXPRESSIONS, KINDS_SAY, nextExpression, noticeShown, openLabel, parseSnapshot } from '../src/attention-model.ts';
import { AttentionWidget } from '../src/widget/attention.ts';

afterEach(() => overrideInvoke(null));

const notice = (id: string, extra: Record<string, unknown> = {}) => ({ id, kind: 'input', title: 'Claude Code · billing', body: 'Apri la sessione.', sessionId: 'i_a', createdAt: 1, ...extra });
const snapshot = (current: unknown, enabled = true) => ({ enabled, muted: false, current });

test('a snapshot is read as Rust sends it, and anything else is no notice', () => {
  assert.deepEqual(parseSnapshot(snapshot(notice('n1'))).current, notice('n1'));
  assert.equal(parseSnapshot(null).current, null);
  assert.equal(parseSnapshot('x').enabled, false);
  assert.equal(parseSnapshot(snapshot({ id: 7 })).current, null);
  assert.equal(parseSnapshot(snapshot(notice('n1', { sessionId: undefined }))).current?.sessionId, null);
  // A kind a newer Rust adds is an update, not a crash.
  assert.equal(parseSnapshot(snapshot(notice('n1', { kind: 'novelty' }))).current?.kind, 'update');
});

test('no notice is shown while the alerts are off', () => {
  assert.equal(noticeShown(parseSnapshot(snapshot(notice('n1'), false))), null);
  assert.equal(noticeShown(parseSnapshot(snapshot(notice('n1'))))?.id, 'n1');
});

test('every kind has its words and its tone, and the button says what it opens', () => {
  assert.deepEqual(Object.keys(KINDS_SAY).sort(), ['complete', 'error', 'input', 'memory', 'update']);
  assert.equal(KINDS_SAY.input.label, 'Serve una tua risposta');
  assert.equal(KINDS_SAY.error.tone, 'danger');
  assert.equal(openLabel({ sessionId: 'i_a' }), 'Apri sessione');
  assert.equal(openLabel({ sessionId: null }), 'Apri Shinatama');
});

test('the first portrait is the smile, then one of the others, never the same twice running', () => {
  assert.equal(nextExpression('curious', true, () => 0.99), 'smile');
  for (const current of EXPRESSIONS) {
    for (const roll of [0, 0.3, 0.5, 0.99]) assert.notEqual(nextExpression(current, false, () => roll), current);
  }
});

test('the widget shows the notice Rust holds, keeps the expression through a read of the same one, and changes it for a new one', async () => {
  let current: unknown = notice('n1');
  overrideInvoke(async command => { assert.equal(command, 'attention_current'); return snapshot(current); });
  const widget = new AttentionWidget(() => 0);
  await widget.refresh();
  assert.equal(widget.notice?.id, 'n1');
  assert.equal(widget.expression, 'smile');
  await widget.refresh();
  assert.equal(widget.expression, 'smile');
  current = notice('n2');
  await widget.refresh();
  assert.equal(widget.notice?.id, 'n2');
  assert.notEqual(widget.expression, 'smile');
  current = null;
  await widget.refresh();
  assert.equal(widget.notice, null);
});

test('a read that fails keeps the notice on show and says it may be old; a new notice clears that', async () => {
  let fail = false;
  let current: unknown = notice('n1');
  overrideInvoke(async () => { if (fail) throw 'busy'; return snapshot(current); });
  const widget = new AttentionWidget();
  await widget.refresh();
  fail = true;
  await widget.refresh();
  assert.equal(widget.notice?.id, 'n1');
  assert.equal(widget.problem, 'Avviso non aggiornato.');
  fail = false;
  current = notice('n2');
  await widget.refresh();
  assert.equal(widget.problem, null);
});

test('a read that is overtaken by a newer one is dropped', async () => {
  const answers: ((value: unknown) => void)[] = [];
  overrideInvoke(() => new Promise(resolve => { answers.push(resolve); }));
  const widget = new AttentionWidget();
  const first = widget.refresh();
  const second = widget.refresh();
  answers[1]!(snapshot(notice('new')));
  await second;
  answers[0]!(snapshot(notice('old')));
  await first;
  assert.equal(widget.notice?.id, 'new');
});

test('open and close ask Rust about the notice on show, one at a time, and read again; a refusal is said in words', async () => {
  const calls: string[] = [];
  let current: unknown = notice('n1');
  let refuse = true;
  overrideInvoke(async (command, args) => {
    calls.push(`${command}:${args?.id ?? ''}`);
    if (command === 'attention_open' && refuse) { refuse = false; throw 'no longer current'; }
    if (command !== 'attention_current') current = null;
    return command === 'attention_current' ? snapshot(current) : null;
  });
  const widget = new AttentionWidget();
  await widget.refresh();
  await widget.open();
  assert.equal(widget.problem, 'Apri la sessione in Shinatama.');
  assert.equal(widget.busy, false);
  assert.equal(widget.notice?.id, 'n1');
  const both = Promise.all([widget.open(), widget.dismiss()]);
  await both;
  assert.deepEqual(calls, ['attention_current:', 'attention_open:n1', 'attention_open:n1', 'attention_current:']);
  assert.equal(widget.notice, null);
  // With nothing on show an answer asks nothing.
  await widget.dismiss();
  assert.equal(calls.length, 4);
});
