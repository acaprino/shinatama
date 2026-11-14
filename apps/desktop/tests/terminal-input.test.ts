import test from 'node:test';
import assert from 'node:assert/strict';
import { formatPaths } from '../src/terminal-input.ts';

test('paths are pasted space-separated, quoted only when they need it', () => {
  assert.equal(formatPaths(['C:\\shot.png']), 'C:\\shot.png ');
  assert.equal(formatPaths(['C:\\My Files\\a.png', 'D:\\repo']), '"C:\\My Files\\a.png" D:\\repo ');
  assert.equal(formatPaths(['C:\\a&b']), '"C:\\a&b" ');
  assert.equal(formatPaths([]), '');
  assert.equal(formatPaths(['']), '');
});
