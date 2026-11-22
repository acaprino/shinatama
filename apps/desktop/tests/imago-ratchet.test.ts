// What the screens compose themselves (Imago migration study, section 7). imago-boundary.test.ts keeps the elements
// a screen makes, the markup it writes and its stylesheets to the engines' mount points, and imago-check holds the app
// to the names of Imago's canon (v0.2.0 asks an app to write some classes itself, an im-row in a sidebar). This file
// counts the DOM parts a screen composes a stateful view with (uiPart and its siblings, the API Imago gives for views
// that keep focus, selection or a live engine) per file: no file may add any, and a file that loses some lowers its
// line in imago-ratchet.json, so the gain stays.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const source = fileURLToPath(new URL('../src/', import.meta.url));
const baselineFile = fileURLToPath(new URL('./imago-ratchet.json', import.meta.url));
type Counts = { parts: number; classes: number };

/** `uiPart(...)` and its siblings build one element at a time: the screen composes the view from Imago's parts. */
const parts = /\b(?:uiPart|uiButton|uiInput|uiSelect|uiOption|uiTextarea|componentElement)\s*(?:<[^<>()]*>)?\s*\(/g;
/** An `im-…` class written in the app: Imago's own controls and roles set their classes themselves. */
const classes = /(?<![\w/@.-])im-[a-z][a-z0-9-]*/g;

export function count(text: string): Counts {
  const code = text.split(/\r?\n/).filter(line => !/^\s*import\b/.test(line)).join('\n');
  return { parts: code.match(parts)?.length ?? 0, classes: code.match(classes)?.length ?? 0 };
}

const files = () => readdirSync(source).filter(name => name.endsWith('.ts')).sort();

function current(): Record<string, { parts: number }> {
  const counts: Record<string, { parts: number }> = {};
  for (const file of files()) {
    const found = count(readFileSync(source + file, 'utf8'));
    if (found.parts) counts[file] = { parts: found.parts };
  }
  return counts;
}

test('the counter sees parts and hand-set Imago classes, not imports, CSS variables or data attributes', () => {
  assert.deepEqual(count([
    "import { uiPart } from '@acaprino/imago/ui';",
    "const row = uiPart('row'); row.append(uiButton('Save'));",
    "const input = componentElement<HTMLInputElement>(inputHtml({ name: 'q', label: 'Find', value: '' }));",
    "row.className = 'im-panel im-sidebar'; row.dataset.imLook = 'oni';",
    "document.documentElement.setAttribute('data-im-look', 'oni'); row.style.setProperty('--im-gap', '4px');",
    "row.querySelector('.im-panel');",
  ].join('\n')), { parts: 3, classes: 2 });
});

test('no screen adds DOM parts; a screen that loses some records it', () => {
  const now = current();
  if (process.env.IMAGO_RATCHET_WRITE === '1') writeFileSync(baselineFile, JSON.stringify(now, null, 2) + '\n');
  const baseline: Record<string, { parts: number }> = JSON.parse(readFileSync(baselineFile, 'utf8'));
  const problems: string[] = [];
  for (const file of new Set([...Object.keys(now), ...Object.keys(baseline)])) {
    const was = baseline[file]?.parts ?? 0;
    const is = now[file]?.parts ?? 0;
    if (is > was) problems.push(`${file}: ${is} parts, more than the ${was} recorded; compose Imago's controls instead`);
    if (is < was) problems.push(`${file}: ${is} parts, fewer than the ${was} recorded; lower its line in imago-ratchet.json (or run with IMAGO_RATCHET_WRITE=1)`);
  }
  assert.deepEqual(problems, []);
});
