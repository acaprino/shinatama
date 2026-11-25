import test from 'node:test';
import assert from 'node:assert/strict';
import { createToneKeeper } from '../src/thread-tone.ts';

const ids = Array.from({ length: 19 }, (_, index) => `w_${String.fromCharCode(97 + index).repeat(12)}`);

test('threads that are on never share a colour while there are colours enough', () => {
  const keeper = createToneKeeper(); const on = ids.slice(0, 18);
  assert.equal(new Set(on.map(id => keeper.tone(on, id))).size, 18, 'the eighteen colours of Sigrid');
});

test('a thread keeps its colour while others are turned on and off', () => {
  const keeper = createToneKeeper();
  const second = keeper.tone(ids.slice(0, 2), ids[1]);
  assert.equal(keeper.tone([ids[1]], ids[1]), second, 'the first thread was turned off');
  assert.equal(keeper.tone([ids[1], ids[2]], ids[1]), second, 'a third thread was turned on');
  assert.notEqual(keeper.tone([ids[1], ids[2]], ids[2]), second);
});

test('a thread that is off has no colour; more threads than colours share the least used one', () => {
  const keeper = createToneKeeper();
  assert.equal(keeper.tone([], ids[0]), null);
  assert.match(keeper.tone(ids, ids[18]) ?? '', /^#[0-9a-f]{6}$/);
});
