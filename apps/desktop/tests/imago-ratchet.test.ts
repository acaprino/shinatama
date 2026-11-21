// What the screens build themselves (Imago migration study, section 7). Three rules hold in every file of src/:
// - no Imago class is written by hand: a part takes Imago's roles by name (uiTraits, uiTone, uiEmphasis);
// - no element is made with document.createElement but the mount points of the engines, which are content, not
//   controls (the canvas a Moli frame is drawn in, the host xterm.js renders into, a CodeMirror gutter marker);
//   a button, a field or a panel made by hand fails here, in the Moli file as anywhere else;
// - markup is written into the page only where the listed producer made it (the Rust document renderer, an
//   Imago renderer).
// The DOM parts a screen composes a stateful view with (uiPart and its siblings, the API Imago gives for views
// that keep focus, selection or a live engine) are counted per file: no file may add any, and a file that loses
// some lowers its line in imago-ratchet.json, so the gain stays.
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
/** An element the app makes itself. */
const made = /\bcreateElement(?:NS)?\s*\(\s*(?:'[^']*',\s*)?'([a-z0-9-]+)'/g;
/** Markup written into the page. */
const written = /\b([A-Za-z_$][\w$]*)\.(?:innerHTML|outerHTML)\s*=|\binsertAdjacentHTML\s*\(/g;

/** The engines' mount points, by file and element: content an engine draws, not a control. */
const ENGINE_MOUNTS: Record<string, Record<string, number>> = {
  'browser-tab.ts': { canvas: 1 }, // the Moli frame is drawn in it
  'code-view.ts': { span: 1 }, // CodeMirror's fold gutter asks for its marker as an element
  'terminal-history-view.ts': { div: 1 }, // the host xterm.js renders the archived lines into
  'terminal-minimap.ts': { canvas: 1, div: 3 }, // the trace of the output; the strip, band and frame go to Imago next
  'files-panel.ts': { div: 4, img: 1 }, // the document panes go to Imago next; the picture is the document
};
/** Where markup is written into the page, and who made it. */
const WRITERS: Record<string, string[]> = {
  'files-panel.ts': ['preview', 'chooser'], // the Rust renderer's escaped document; Imago's selectOptionsHtml
};

export function count(text: string): Counts {
  const code = text.split(/\r?\n/).filter(line => !/^\s*import\b/.test(line)).join('\n');
  return { parts: code.match(parts)?.length ?? 0, classes: code.match(classes)?.length ?? 0 };
}
export function elements(text: string): Record<string, number> {
  const found: Record<string, number> = {};
  for (const [, tag] of text.matchAll(made)) found[tag] = (found[tag] ?? 0) + 1;
  return found;
}
export function writers(text: string): string[] {
  return [...text.matchAll(written)].map(match => match[1] ?? 'insertAdjacentHTML');
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

test('the counters see parts, hand-set Imago classes, made elements and written markup', () => {
  const sample = [
    "import { uiPart } from '@acaprino/imago/ui';",
    "const row = uiPart('row'); row.append(uiButton('Save'));",
    "const input = componentElement<HTMLInputElement>(inputHtml({ name: 'q', label: 'Find', value: '' }));",
    "row.className = 'im-panel im-panel--docked'; row.dataset.imLook = 'oni';",
    "document.documentElement.setAttribute('data-im-look', 'oni'); row.style.setProperty('--im-gap', '4px');",
    "row.querySelector('.im-panel'); const reconnect = document.createElement('button');",
    "const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); row.innerHTML = '<b>x</b>';",
  ].join('\n');
  assert.deepEqual(count(sample), { parts: 3, classes: 2 });
  assert.deepEqual(elements(sample), { button: 1, svg: 1 });
  assert.deepEqual(writers(sample), ['row']);
});

test('no screen writes an Imago class', () => {
  const problems = files().map(file => [file, count(readFileSync(source + file, 'utf8')).classes] as const)
    .filter(([, found]) => found > 0).map(([file, found]) => `${file}: ${found} Imago classes written by hand; give the part its role with uiTraits, uiTone or uiEmphasis`);
  assert.deepEqual(problems, []);
});

test('no screen makes an element but the engines\' mount points', () => {
  const problems: string[] = [];
  for (const file of files()) {
    const found = elements(readFileSync(source + file, 'utf8'));
    const allowed = ENGINE_MOUNTS[file] ?? {};
    for (const tag of new Set([...Object.keys(found), ...Object.keys(allowed)])) {
      const is = found[tag] ?? 0; const may = allowed[tag] ?? 0;
      if (is > may) problems.push(`${file}: ${is} <${tag}> made by hand, ${may} allowed; compose an Imago part or control instead`);
      if (is < may) problems.push(`${file}: ${is} <${tag}> made by hand, fewer than the ${may} allowed; lower ENGINE_MOUNTS`);
    }
  }
  assert.deepEqual(problems, []);
});

test('markup is written into the page only where its producer is listed', () => {
  const problems: string[] = [];
  for (const file of files()) {
    const found = writers(readFileSync(source + file, 'utf8'));
    const allowed = WRITERS[file] ?? [];
    if (JSON.stringify(found) !== JSON.stringify(allowed)) problems.push(`${file}: markup written into ${JSON.stringify(found)}, listed ${JSON.stringify(allowed)}`);
  }
  assert.deepEqual(problems, []);
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
