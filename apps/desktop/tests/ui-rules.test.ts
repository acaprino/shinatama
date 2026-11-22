// Interface rules that a later edit could undo without anything else failing (UI review, 2026-11-16).
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { test } from 'node:test';

const source = new URL('../src/', import.meta.url);
const sheets = readdirSync(source).filter(name => name.endsWith('.css')).map(name => ({ name, css: readFileSync(new URL(name, source), 'utf8') }));
const styles = readFileSync(new URL(import.meta.resolve('@acaprino/imago/css/base.css')), 'utf8');

test('no text is set under 11px', () => {
  for (const { name, css } of sheets) {
    // In the font shorthand the first length is the size; the line height follows a slash.
    for (const match of css.matchAll(/[\s;{]font(?:-size)?:[^;}]*?(\d+(?:\.\d+)?)px/g)) {
      assert.ok(Number(match[1]) >= 11, `${name}: ${match[0].trim()}`);
    }
  }
});

test('nothing but the "session is opening" pulse moves without end (WCAG 2.2.2)', () => {
  for (const { name, css } of sheets) {
    for (const match of css.matchAll(/animation:[^;}]*infinite[^;}]*/g)) assert.match(match[0], /session-opening/, `${name}: ${match[0]}`);
  }
});

test('reduced motion also stops pseudo-elements', () => {
  const reduced = styles.match(/@media \(prefers-reduced-motion: reduce\)\s*\{([^}]*\})/)?.[1];
  assert.ok(reduced);
  for (const selector of ['*::before', '*::after']) assert.ok(reduced.includes(selector));
  assert.match(reduced, /transition: none !important[\s\S]*animation: none !important/);
});

const scripts = readdirSync(source).filter(name => name.endsWith('.ts')).map(name => ({ name, ts: readFileSync(new URL(name, source), 'utf8') }));

test("an icon-only control is the canon's icon button, its name also its tooltip (no button whose text is a symbol)", () => {
  for (const { name, ts } of scripts) {
    for (const match of ts.matchAll(/uiButton\('([^\p{L}\p{N}'][^']?)'/gu)) assert.fail(`${name}: ${match[0]}`);
  }
});

test('no control has a hand cursor', () => {
  for (const { name, css } of sheets) assert.doesNotMatch(css, /cursor:\s*pointer/, name);
  for (const { name, ts } of scripts) assert.doesNotMatch(ts, /cursor:\s*'pointer'/, name);
});
