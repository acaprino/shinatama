import test from 'node:test';
import assert from 'node:assert/strict';
import { holdOutput, splitOutput } from '../src/terminal-output.ts';

const bytes = (text: string) => new TextEncoder().encode(text);
const text = (parts: Uint8Array[]) => parts.map(part => new TextDecoder().decode(part));

test('output is cut after every n-th line, and what is left is the last part', () => {
  assert.deepEqual(text(splitOutput(bytes('a\nb\nc\nd\ne'), 2)), ['a\nb\n', 'c\nd\n', 'e']);
  assert.deepEqual(text(splitOutput(bytes('a\nb\n'), 2)), ['a\nb\n']);
  assert.deepEqual(text(splitOutput(bytes(''))), ['']);
});

test('what a screen that is not there yet holds is bounded, and the newest part always stays', () => {
  let held: Uint8Array[] = [];
  for (const word of ['aaaa', 'bbbb', 'cccc']) held = holdOutput(held, bytes(word), 8);
  assert.deepEqual(text(held), ['bbbb', 'cccc']);
  assert.deepEqual(text(holdOutput([], bytes('longer than the limit'), 4)), ['longer than the limit']);
});
