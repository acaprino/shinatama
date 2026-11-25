// The search of a terminal's retained output (src/terminal-find.ts).
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { findLines, findStatus, MAX_HITS, SEARCH_LINES, stepHit } from '../src/terminal-find.ts';

const buffer = ['PS C:\\work> git status', 'On branch develop', 'nothing to commit', 'PS C:\\work> git log', 'Commit abc'];

test('the lines that hold the text are found, ignoring case', () => {
  assert.deepEqual(findLines(index => buffer[index], buffer.length, 'GIT').rows, [0, 3]);
  assert.deepEqual(findLines(index => buffer[index], buffer.length, 'commit').rows, [2, 4]);
  assert.deepEqual(findLines(index => buffer[index], buffer.length, '').rows, []);
  assert.deepEqual(findLines(index => buffer[index], buffer.length, 'zzz'), { rows: [], overflow: false });
});

test('only the last lines are searched, and only so many are kept', () => {
  const lines = Array.from({ length: SEARCH_LINES + 100 }, (_, index) => index < 50 ? 'old hit' : 'hit');
  const found = findLines(index => lines[index], lines.length, 'hit');
  assert.equal(found.rows.length, MAX_HITS);
  assert.equal(found.rows[0], 100, 'the oldest 100 lines are out of reach');
  assert.equal(found.overflow, true);
  const exact = findLines(index => lines[index], lines.length, 'old');
  assert.deepEqual(exact.rows, [], 'the old lines are beyond the search');
});

test('next and previous go round the hits, and start at the first or the last when none is chosen', () => {
  assert.equal(stepHit(0, -1, 1), -1);
  assert.equal(stepHit(3, -1, 1), 0);
  assert.equal(stepHit(3, -1, -1), 2);
  assert.equal(stepHit(3, 2, 1), 0);
  assert.equal(stepHit(3, 0, -1), 2);
});

test('the dialog says where the person is in the hits', () => {
  assert.match(findStatus('', { rows: [], overflow: false }, -1), /Search the terminal/);
  assert.match(findStatus('x', { rows: [], overflow: false }, -1), /No matching lines/);
  assert.equal(findStatus('x', { rows: [3, 5], overflow: false }, 1), '2 of 2 matching lines');
  assert.equal(findStatus('x', { rows: [3, 5], overflow: true }, 0), '1 of 2+ matching lines');
});
