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

test('PowerShell gets single quotes, so $ and the backtick stay literal', () => {
  assert.equal(formatPaths(['C:\\$Recycle.Bin\\x'], 'powershell'), "'C:\\$Recycle.Bin\\x' ");
  assert.equal(formatPaths(['D:\\a`b\\c'], 'powershell'), "'D:\\a`b\\c' ");
  assert.equal(formatPaths(["D:\\it's here"], 'powershell'), "'D:\\it''s here' ");
  assert.equal(formatPaths(['D:\\repo'], 'powershell'), 'D:\\repo ');
});
