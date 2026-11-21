// Migration ratchet (Imago migration study, section 7.3): controls the app still composes itself from
// Imago's DOM parts, and Imago classes it sets by hand, counted per file. No file may add any; a file that
// loses some must lower its line in imago-ratchet.json, so the gain stays. The goal is zero everywhere:
// screens compose Imago's controls (openFormDialog, pathField, stackListHtml, inspector, ...) instead.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const source = fileURLToPath(new URL('../src/', import.meta.url));
const baselineFile = fileURLToPath(new URL('./imago-ratchet.json', import.meta.url));
type Counts = { parts: number; classes: number };

/** `uiPart(...)` and its siblings build one element at a time: the screen, not Imago, composes the control. */
const parts = /\b(?:uiPart|uiButton|uiInput|uiSelect|uiOption|uiTextarea|componentElement)\s*(?:<[^<>()]*>)?\s*\(/g;
/** An `im-…` class written in the app: Imago's own controls set their classes themselves. */
const classes = /(?<![\w/@.-])im-[a-z][a-z0-9-]*/g;

export function count(text: string): Counts {
  const code = text.split(/\r?\n/).filter(line => !/^\s*import\b/.test(line)).join('\n');
  return { parts: code.match(parts)?.length ?? 0, classes: code.match(classes)?.length ?? 0 };
}

function current(): Record<string, Counts> {
  const counts: Record<string, Counts> = {};
  for (const file of readdirSync(source).filter(name => name.endsWith('.ts')).sort()) {
    const found = count(readFileSync(source + file, 'utf8'));
    if (found.parts || found.classes) counts[file] = found;
  }
  return counts;
}

test('the counter sees parts and hand-set Imago classes, not imports, CSS variables or data attributes', () => {
  assert.deepEqual(count([
    "import { uiPart } from '@acaprino/imago/ui';",
    "const row = uiPart('row'); row.append(uiButton('Save'));",
    "const input = componentElement<HTMLInputElement>(inputHtml({ name: 'q', label: 'Find', value: '' }));",
    "row.className = 'im-panel im-panel--docked'; row.dataset.imLook = 'oni';",
    "document.documentElement.setAttribute('data-im-look', 'oni'); row.style.setProperty('--im-gap', '4px');",
    "row.querySelector('.im-panel');",
  ].join('\n')), { parts: 3, classes: 2 });
});

test('no screen adds hand-composed controls; a screen that loses some records it', () => {
  const now = current();
  if (process.env.IMAGO_RATCHET_WRITE === '1') writeFileSync(baselineFile, JSON.stringify(now, null, 2) + '\n');
  const baseline: Record<string, Counts> = JSON.parse(readFileSync(baselineFile, 'utf8'));
  const problems: string[] = [];
  for (const file of new Set([...Object.keys(now), ...Object.keys(baseline)])) {
    const was = baseline[file] ?? { parts: 0, classes: 0 };
    const is = now[file] ?? { parts: 0, classes: 0 };
    for (const kind of ['parts', 'classes'] as const) {
      if (is[kind] > was[kind]) problems.push(`${file}: ${is[kind]} ${kind}, more than the ${was[kind]} recorded; compose Imago's controls instead`);
      if (is[kind] < was[kind]) problems.push(`${file}: ${is[kind]} ${kind}, fewer than the ${was[kind]} recorded; lower its line in imago-ratchet.json (or run with IMAGO_RATCHET_WRITE=1)`);
    }
  }
  assert.deepEqual(problems, []);
});
