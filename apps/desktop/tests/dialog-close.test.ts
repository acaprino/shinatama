// The browser fires a dialog's close event after it has closed; under load it can come after the next opening (a key
// press handled first). A screen that resets its state on close must then leave the new opening alone: every
// `dialog.addEventListener('close', ...)` handler in src/ starts by ignoring an event that finds the dialog open.
// (new-project-smoke stayed on "Reading project roots…" for good when such an event dropped the new opening's read.)
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const source = fileURLToPath(new URL('../src/', import.meta.url));
const handler = /\bdialog\.addEventListener\(\s*'close',\s*\(\)\s*=>\s*\{\s*(?:\/\/[^\n]*\n\s*)*([^\n]*)/g;
const guarded = /^if \(dialog\.open\) return;|^if \(!dialog\.open\)/;

export function unguarded(text: string): string[] {
  return [...text.matchAll(handler)].map(match => match[1].trim()).filter(first => !guarded.test(first));
}

test('the check sees an unguarded handler and accepts both guards', () => {
  assert.deepEqual(unguarded([
    "dialog.addEventListener('close', () => { generation++; });",
    "dialog.addEventListener('close', () => { if (dialog.open) return; generation++; });",
    "dialog.addEventListener('close', () => {\n  // why\n  if (!dialog.open) { generation++; }\n});",
  ].join('\n')), ['generation++; });']);
});

test('every dialog close handler ignores a close event that finds the dialog open again', () => {
  const problems: string[] = [];
  for (const file of readdirSync(source).filter(name => name.endsWith('.ts'))) {
    for (const first of unguarded(readFileSync(source + file, 'utf8'))) problems.push(`${file}: ${first.slice(0, 80)}`);
  }
  assert.deepEqual(problems, []);
});
