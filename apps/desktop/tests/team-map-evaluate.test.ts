import test from 'node:test';
import assert from 'node:assert/strict';
import { createEvaluator } from '../src/team-map-evaluate.ts';

const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

test('the engine is asked at most once per gap, however fast the edits come', async () => {
  const starts: number[] = [];
  const asked: string[] = [];
  const evaluator = createEvaluator<string, string>(async (formation) => { starts.push(Date.now()); asked.push(formation); return `answer ${formation}`; }, 60);
  const answers = await Promise.all(['a', 'b', 'c', 'd', 'e'].map((formation) => evaluator.request('draft', formation)));
  // Four of the five were replaced by a later one before they started; only the latest is answered.
  assert.deepEqual(answers, [null, null, null, null, 'answer e']);
  assert.deepEqual(asked, ['e']);
  await evaluator.request('draft', 'f');
  assert.equal(starts.length, 2);
  assert.ok(starts[1] - starts[0] >= 55, `calls ${starts[1] - starts[0]} ms apart`);
});

test('requests of different kinds wait their turn instead of replacing each other', async () => {
  const starts: number[] = [];
  const evaluator = createEvaluator<string, string>(async (formation) => { starts.push(Date.now()); return formation; }, 50);
  const answers = await Promise.all([evaluator.request('draft', 'main'), evaluator.request('trial:kind', 'kind'), evaluator.request('trial:report', 'report')]);
  assert.deepEqual(answers, ['main', 'kind', 'report']);
  for (let i = 1; i < starts.length; i += 1) assert.ok(starts[i] - starts[i - 1] >= 45, `calls ${starts[i] - starts[i - 1]} ms apart`);
});

test('cancel ends what is waiting with null, and a failing call rejects only its own request', async () => {
  let calls = 0;
  const evaluator = createEvaluator<string, string>(async (formation) => { calls += 1; if (formation === 'bad') throw new Error('refused'); return formation; }, 30);
  const first = evaluator.request('a', 'bad');
  await assert.rejects(first, /refused/);
  const waiting = evaluator.request('a', 'late');
  evaluator.cancel();
  assert.equal(await waiting, null);
  await pause(80);
  assert.equal(calls, 1);
  assert.equal(await evaluator.request('a', 'fine'), 'fine');
});
