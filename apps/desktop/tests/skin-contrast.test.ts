import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const css = readFileSync(new URL('../src/styles.css', import.meta.url), 'utf8');
const rootBlock = css.match(/:root\s*\{([\s\S]*?)\}/)?.[1] ?? '';
const tokens = Object.fromEntries([...rootBlock.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)].map(match => [match[1], match[2]]));

function luminance(hex: string) {
  const channels = [1, 3, 5].map(index => parseInt(hex.slice(index, index + 2), 16) / 255)
    .map(value => value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}
function contrast(foreground: string, background: string) {
  const [lighter, darker] = [luminance(tokens[foreground]), luminance(tokens[background])].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

const surfaces = ['stage', 'chrome', 'chrome-2', 'surface', 'surface-2', 'terminal'];

test('every text role reaches 4.5:1 on every chrome surface (ADR-0039 INV-006)', () => {
  for (const text of ['text', 'text-2', 'muted', 'ice', 'accent', 'live', 'warning', 'error']) {
    for (const surface of surfaces) {
      assert.ok(contrast(text, surface) >= 4.5, `${text} on ${surface}: ${contrast(text, surface).toFixed(2)}`);
    }
  }
  assert.ok(contrast('accent-ink', 'accent') >= 4.5);
  assert.ok(contrast('accent-ink', 'accent-soft') >= 4.5);
  assert.ok(contrast('on-danger', 'danger') >= 4.5);
});

test('control borders reach 3:1 on the surfaces they sit on', () => {
  for (const surface of ['chrome', 'chrome-2', 'terminal']) {
    assert.ok(contrast('line-strong', surface) >= 3, `line-strong on ${surface}: ${contrast('line-strong', surface).toFixed(2)}`);
  }
});

test('chrome colors come only from the skin tokens (ADR-0039 INV-001)', () => {
  const outsideRoot = css.replace(/:root\s*\{[\s\S]*?\}/, '');
  const declarations = [...outsideRoot.matchAll(/\{([^{}]*)\}/g)].map(block => block[1]).join('\n');
  assert.deepEqual(declarations.match(/#[0-9a-fA-F]{3,8}\b/g) ?? [], []);
  assert.doesNotMatch(declarations, /\b(?:rgb|hsl)a?\(/);
});

test('no glow and no rounded chrome in the default skin', () => {
  assert.doesNotMatch(css, /text-shadow/);
  assert.doesNotMatch(css, /border-radius:\s*[1-9]/);
  for (const shadow of css.matchAll(/box-shadow:\s*([^;}]+)/g)) {
    assert.match(shadow[1], /var\(--shadow\)|^none$/, `only dark drop shadows are allowed: ${shadow[1]}`);
  }
});
