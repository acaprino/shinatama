import { test } from 'node:test';
import assert from 'node:assert/strict';
import { afterRefusal, createOpenQueue, createWants, newWant, nextWant, openCard, type OpenOutcome, RETRY_DELAYS_MS, type Want } from '../src/session-opens.ts';

// 2026-11-19, the owner: Resume could not be pressed on a session while another one was opening.
test('opens take their turn one at a time, in the order asked; none is refused for being busy', async () => {
  const seen: string[] = []; const changes: (string | null)[] = [];
  const queue = createOpenQueue(() => changes.push(queue.current()));
  let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; });
  const first = queue.run('t_a', async () => { seen.push('a starts'); await held; seen.push('a ends'); return 'a'; });
  const second = queue.run('t_b', async () => { seen.push('b starts'); return 'b'; });
  assert.equal(queue.busy(), true);
  await Promise.resolve(); await Promise.resolve();
  assert.equal(queue.current(), 't_a');
  assert.deepEqual(seen, ['a starts']);
  release();
  assert.deepEqual(await Promise.all([first, second]), ['a', 'b']);
  assert.deepEqual(seen, ['a starts', 'a ends', 'b starts']);
  assert.equal(queue.current(), null);
  assert.equal(queue.busy(), false);
  assert.deepEqual(changes, ['t_a', null, 't_b', null]);
});
test('an open that fails gives its failure to its caller and the next one still runs', async () => {
  const queue = createOpenQueue();
  const failed = queue.run('t_a', async () => { throw new Error('refused'); });
  const next = queue.run('t_b', async () => 'opened');
  await assert.rejects(failed, /refused/);
  assert.equal(await next, 'opened');
  assert.equal(queue.busy(), false);
});

// shinatama.log 2026-11-18: at a start the reopen of a session was refused while the model proxy was
// still starting, and the session was left "not running" for good.
test('a refusal that can pass is tried again later; one that cannot, or the last try, gives the want up', () => {
  let want: Want | null = newWant(true);
  for (const [index, delay] of RETRY_DELAYS_MS.entries()) {
    want = afterRefusal(want!, true, 'The model proxy is not ready.', 1000);
    assert.deepEqual(want, { attempts: index + 1, notBefore: 1000 + delay, reason: 'The model proxy is not ready.', restored: true });
  }
  assert.equal(afterRefusal(want!, true, 'again', 1000), null);
  assert.equal(afterRefusal(newWant(false), false, 'No saved conversation yet.', 1000), null);
});
test('the sessions the user asked for open first, then the restored ones in the order of their tabs', () => {
  const wants = new Map<string, Want>([['t_c', newWant(true)], ['t_a', newWant(true)], ['t_user', newWant(false)]]);
  const order = ['t_a', 't_b', 't_c'];
  assert.deepEqual(nextWant(wants, order, 0), { id: 't_user' });
  wants.delete('t_user');
  assert.deepEqual(nextWant(wants, order, 0), { id: 't_a' });
  // One waiting for its next try is passed over; when all wait, the pump is told for how long.
  wants.set('t_a', { ...newWant(true), attempts: 1, notBefore: 5000, reason: 'busy' });
  assert.deepEqual(nextWant(wants, order, 0), { id: 't_c' });
  wants.set('t_c', { ...newWant(true), attempts: 1, notBefore: 3000, reason: 'busy' });
  assert.deepEqual(nextWant(wants, order, 1000), { waitMs: 2000 });
  assert.deepEqual(nextWant(wants, order, 3000), { id: 't_c' });
  assert.equal(nextWant(new Map(), order, 0), null);
});
/** The wants over a clock the test moves: a wait ends at once and the clock jumps past it. */
function harness(attempt: (id: string, restored: boolean, tries: number) => Promise<OpenOutcome> | OpenOutcome, restoredOrder: string[] = []) {
  let clock = 0; const tried: string[] = []; const settled: [string, string | null][] = []; const waits: number[] = [];
  const opens = createOpenQueue();
  const wants = createWants(opens, {
    attempt: async (id, restored) => { tried.push(id); return attempt(id, restored, tried.filter(other => other === id).length); },
    restoredOrder: () => restoredOrder, settled: (id, refused) => { settled.push([id, refused]); }, changed: () => {},
    now: () => clock, wait: ms => { waits.push(ms); clock += ms; return { done: Promise.resolve(), wake: () => {} }; },
  });
  return { opens, wants, tried, settled, waits };
}
// The start of 2026-11-18 21:30: the first reopens were refused while the proxy started, and stayed "not running".
test('restored sessions open in the order of their tabs; one refused while the proxy starts is tried again until it opens', async () => {
  const run = harness((id, _restored, tries) => id === 't_b' && tries < 3 ? { refused: 'The model proxy is not ready.', retry: true } : { opened: true }, ['t_a', 't_b', 't_c']);
  for (const id of ['t_c', 't_b', 't_a']) run.wants.ask(id, true);
  await run.wants.idle();
  // t_b is passed over while it waits, so the others are not held behind it; then it is tried again.
  assert.deepEqual(run.tried, ['t_a', 't_b', 't_c', 't_b', 't_b']);
  assert.deepEqual(run.settled, [['t_a', null], ['t_c', null], ['t_b', null]]);
  assert.deepEqual(run.waits, [RETRY_DELAYS_MS[0], RETRY_DELAYS_MS[1]]);
  assert.equal(run.wants.all().size, 0);
});
test('a refusal that cannot pass, or the last try, gives the session up once with its reason', async () => {
  const run = harness(id => id === 't_gone' ? { refused: 'No saved conversation yet.', retry: false } : { refused: 'Not enough free memory.', retry: true });
  run.wants.ask('t_gone'); run.wants.ask('t_busy');
  await run.wants.idle();
  assert.equal(run.tried.filter(id => id === 't_gone').length, 1);
  assert.equal(run.tried.filter(id => id === 't_busy').length, RETRY_DELAYS_MS.length + 1);
  assert.deepEqual(run.settled, [['t_gone', 'No saved conversation yet.'], ['t_busy', 'Not enough free memory.']]);
});
test('a try that fails outright is a refusal that is not repeated, and the next session still opens', async () => {
  const run = harness(id => { if (id === 't_a') throw new Error('The window lost its connection.'); return { nothing: true }; });
  run.wants.ask('t_a'); run.wants.ask('t_b');
  await run.wants.idle();
  assert.deepEqual(run.settled, [['t_a', 'The window lost its connection.'], ['t_b', null]]);
});
// 2026-11-19: Resume was greyed out while another session opened. Now it is asked at once and waits its turn.
test('a session asked while another opens waits its turn; cancelled meanwhile, it is not tried', async () => {
  let release!: () => void; const held = new Promise<void>(resolve => { release = resolve; });
  const run = harness(async id => { if (id === 't_a') await held; return { opened: true }; });
  run.wants.ask('t_a'); run.wants.ask('t_b'); run.wants.ask('t_c');
  await Promise.resolve(); await Promise.resolve();
  const state = () => ({ current: run.opens.current(), wants: run.wants.all(), now: 0 });
  assert.deepEqual(['t_a', 't_b', 't_c'].map(id => openCard(id, state())), ['opening', 'queued', 'queued']);
  assert.equal(run.wants.cancel('t_b'), true);
  assert.equal(run.wants.cancel('t_b'), false);
  assert.equal(openCard('t_b', state()), 'offer');
  release(); await run.wants.idle();
  assert.deepEqual(run.tried, ['t_a', 't_c']);
  assert.deepEqual(run.settled, [['t_b', null], ['t_a', null], ['t_c', null]]);
});
test('a session waiting for its next try is tried now when the user asks, and a want asked after the loop ended starts it again', async () => {
  let wake = () => {}; let clock = 0; const tried: string[] = []; let ready = false;
  const wants = createWants(createOpenQueue(), {
    attempt: async id => { tried.push(id); return ready ? { opened: true } : { refused: 'busy', retry: true }; },
    restoredOrder: () => [], settled: () => {}, changed: () => {}, now: () => clock,
    // A wait that only ends when woken: the user's Try now is what moves it.
    wait: () => { let end!: () => void; const done = new Promise<void>(resolve => { end = resolve; }); wake = end; return { done, wake: end }; },
  });
  wants.ask('t_a', true);
  for (let turn = 0; turn < 10; turn++) await Promise.resolve();
  assert.deepEqual(tried, ['t_a']);
  assert.equal(openCard('t_a', { current: null, wants: wants.all(), now: clock }), 'retrying');
  ready = true; wants.ask('t_a');
  await wants.idle();
  assert.deepEqual(tried, ['t_a', 't_a']);
  wants.ask('t_b'); await wants.idle();
  assert.deepEqual(tried, ['t_a', 't_a', 't_b']);
  void wake;
});
test('the card of a session that is not running says where its open stands', () => {
  const wants = new Map<string, Want>([['t_b', newWant(true)], ['t_c', { ...newWant(true), attempts: 1, notBefore: 9000, reason: 'busy' }]]);
  const state = { current: 't_a', wants, now: 1000 };
  assert.equal(openCard('t_a', state), 'opening');
  assert.equal(openCard('t_b', state), 'queued');
  assert.equal(openCard('t_c', state), 'retrying');
  assert.equal(openCard('t_c', { ...state, now: 9000 }), 'queued');
  // Nobody asked for it: the card offers its button, whatever else is opening.
  assert.equal(openCard('t_d', state), 'offer');
});
