import assert from 'node:assert/strict';
import { test } from 'node:test';
import { SHOW_AFTER_MS, acceptsEvent, describeProgress, formatBytes, formatElapsed, type ProgressEvent } from '../src/session-progress-core.ts';

const event = (over: Partial<ProgressEvent> = {}): ProgressEvent => ({ requestId: 'r-1', kind: 'export', entries: 12400, bytes: 38 * 1024 * 1024, hits: 0, fileBytes: 230 * 1024 * 1024, ...over });

test('an export line reads as entries, bytes written and seconds, with the file size as context', () => {
  assert.equal(describeProgress('export', event(), 14_200), 'Exporting… 12,400 entries · 38 MB written · 14 s · session file 230 MB');
  assert.equal(describeProgress('export', event({ entries: 1, bytes: 512, fileBytes: 0 }), 600), 'Exporting… 1 entry · 512 B written · 0 s');
});

test('a search line reads as entries, matches so far and seconds', () => {
  assert.equal(describeProgress('search', event({ kind: 'search', entries: 3000, hits: 1, bytes: 0 }), 75_000), 'Searching… 3,000 entries · 1 match · 1 min 15 s · session file 230 MB');
  assert.equal(describeProgress('search', event({ kind: 'search', hits: 40 }), 1000).includes('40 matches'), true);
});

test('before the worker has reported there is only the elapsed time, and there is no percentage ever', () => {
  assert.equal(describeProgress('export', null, 2_000), 'Exporting… 2 s');
  for (const text of [describeProgress('export', event(), 9000), describeProgress('search', event({ kind: 'search' }), 9000)]) assert.equal(text.includes('%'), false);
});

test('sizes and durations are rounded for people', () => {
  assert.deepEqual([formatBytes(0), formatBytes(2048), formatBytes(1536 * 1024), formatBytes(40 * 1024 * 1024), formatBytes(3 * 1024 ** 3)], ['0 B', '2 KB', '1.5 MB', '40 MB', '3.0 GB']);
  assert.deepEqual([formatElapsed(999), formatElapsed(59_999), formatElapsed(60_000), formatElapsed(3_725_000)], ['0 s', '59 s', '1 min 00 s', '62 min 05 s']);
});

test('an event of a closed or replaced request, or of the other kind, never updates the running one', () => {
  assert.equal(acceptsEvent('r-1', 'export', event()), true);
  assert.equal(acceptsEvent('r-2', 'export', event()), false, 'a replaced request');
  assert.equal(acceptsEvent(null, 'export', event()), false, 'nothing is running');
  assert.equal(acceptsEvent('r-1', 'search', event()), false, 'the other surface');
});

test('a short operation does not flash: the line waits half a second after the first report', () => {
  assert.equal(SHOW_AFTER_MS, 500);
});
