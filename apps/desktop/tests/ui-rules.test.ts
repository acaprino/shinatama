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
