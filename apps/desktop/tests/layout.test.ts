import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseLayout, DEFAULT_LAYOUT, boundedRatio } from '../src/layout.ts';
test('corrupt, unknown and unbounded layouts recover without launching resources', () => {
  for (const raw of [null, '{', '{}', '{"version":2}', '{"version":1,"axis":"rows","ratio":0}', '{"version":1,"axis":"rows","ratio":"0.5"}']) {
    assert.deepEqual(parseLayout(raw), DEFAULT_LAYOUT);
  }
});
test('restoration strips process references and unknown fields', () => {
  assert.deepEqual(parseLayout('{"version":1,"axis":"rows","ratio":0.65,"pid":12,"terminal":"secret","command":"run"}'), { version: 1, axis: 'rows', ratio: 0.65 });
});
test('split resizing keeps both panes reachable', () => {
  assert.equal(boundedRatio(-2), 0.2); assert.equal(boundedRatio(2), 0.8);
});
