import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import type { Terminal } from '@xterm/xterm';
// These pure token/profile cases do not open a modal. Allow its static module
// to register the browser listener while importing under Node, then restore it.
const previousDocument = Reflect.get(globalThis, 'document');
Reflect.set(globalThis, 'document', { addEventListener() {} });
const { DEFAULT_PRESENTATION, PROFILES, ACCENT_TEXT_LIGHT, SKINS, SMOOTH, SMOOTH_LIGHT, applyTerminalProfile, optionsForProfile, storedLook, storedTheme, windowThemeFor } = await import('../src/presentation.ts');
if (previousDocument === undefined) Reflect.deleteProperty(globalThis, 'document');
else Reflect.set(globalThis, 'document', previousDocument);

function luminance(hex: string) {
  const values = [1, 3, 5].map(start => Number.parseInt(hex.slice(start, start + 2), 16) / 255)
    .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722;
}
function contrast(a: string, b: string) { const values = [luminance(a), luminance(b)].sort((a, b) => b - a); return (values[0] + .05) / (values[1] + .05); }
test('every selectable frame skin preserves text and primary-action contrast', () => {
  // Smooth's light theme: the pink of the fills carries its dark ink; as text it is ACCENT_TEXT_LIGHT.
  const lightText = { ...SMOOTH_LIGHT, accent: ACCENT_TEXT_LIGHT };
  for (const surface of ['stage', 'chrome', 'chrome-2', 'surface', 'surface-2', 'terminal', 'glass-tint'] as const) {
    for (const text of ['text', 'text-2', 'muted', 'ice', 'accent', 'live', 'warning', 'error'] as const) assert.ok(contrast(lightText[text], SMOOTH_LIGHT[surface]) >= 4.5, `smoothLight: ${text}/${surface}`);
  }
  assert.ok(contrast(SMOOTH_LIGHT['accent-ink'], SMOOTH_LIGHT.accent) >= 4.5);
  assert.ok(contrast(SMOOTH_LIGHT['accent-ink'], SMOOTH_LIGHT['accent-soft']) >= 4.5);
  assert.ok(contrast(SMOOTH_LIGHT['on-danger'], SMOOTH_LIGHT.danger) >= 4.5);
  assert.ok(contrast(SMOOTH_LIGHT['line-strong'], SMOOTH_LIGHT.chrome) >= 3);
  for (const [id, skin] of Object.entries({ ...SKINS, smooth: { tokens: SMOOTH } })) {
    for (const surface of ['stage', 'chrome', 'chrome-2', 'surface', 'surface-2', 'terminal', 'glass-tint'] as const) {
      for (const text of ['text', 'text-2', 'muted', 'ice', 'accent', 'live', 'warning', 'error'] as const) {
        assert.ok(contrast(skin.tokens[text], skin.tokens[surface]) >= 4.5, `${id}: ${text}/${surface}`);
      }
    }
    assert.ok(contrast(skin.tokens['accent-ink'], skin.tokens.accent) >= 4.5, id);
    assert.ok(contrast(skin.tokens['accent-ink'], skin.tokens['accent-soft']) >= 4.5, id);
    assert.ok(contrast(skin.tokens['on-danger'], skin.tokens.danger) >= 4.5, id);
    assert.ok(contrast(skin.tokens['line-strong'], skin.tokens.chrome) >= 3, id);
  }
});
test('profile fallback and follow-skin change presentation without writing or restarting a PTY', () => {
  assert.equal(DEFAULT_PRESENTATION.followSkin, false);
  assert.equal(PROFILES.hermes.version, 1);
  const themeA = optionsForProfile('codex', DEFAULT_PRESENTATION).theme;
  const themeB = optionsForProfile('codex', { ...DEFAULT_PRESENTATION, skin: 'sumi' }).theme;
  assert.deepEqual(themeA, themeB, 'frame-only change must preserve the terminal profile');
  const fake = { options: {}, write() { assert.fail('PTY bytes must not be rewritten'); }, reset() { assert.fail('terminal must not restart'); }, resize() { assert.fail('owner schedules one final resize'); } };
  const first = applyTerminalProfile(fake as unknown as Terminal, 'missing-profile', DEFAULT_PRESENTATION);
  assert.equal(first.id, 'neutral'); assert.equal(first.fallback, true); assert.equal(first.metricsChanged, true);
  assert.equal(applyTerminalProfile(fake as unknown as Terminal, 'neutral', DEFAULT_PRESENTATION).metricsChanged, false);
  const follow = optionsForProfile('codex', { ...DEFAULT_PRESENTATION, skin: 'sumi', followSkin: true });
  assert.equal(follow.theme?.background, SKINS.sumi.tokens.terminal);
  assert.match(String(follow.fontFamily), /monospace/);
});
test('every skin paints a dark terminal, so followed harnesses start on their dark ANSI themes', () => {
  // presentation.rs maps "follow skin" to the dark variants; a light skin must change it there.
  // shortcut: Smooth's light theme is not in this list, a followed harness starts on its dark ANSI theme over a light terminal; teach presentation.rs the theme if that reads badly.
  for (const [id, skin] of Object.entries({ ...SKINS, smooth: { tokens: SMOOTH } })) assert.ok(luminance(skin.tokens.terminal) < 0.179, id);
});
test('the window opens in Smooth unless ONI MODE was chosen, and followed terminals take the palette of the look', () => {
  assert.equal(storedLook(null), 'smooth');
  assert.equal(storedLook('ONI'), 'smooth', 'only the exact stored word');
  assert.equal(storedLook('oni'), 'oni');
  const followed = { ...DEFAULT_PRESENTATION, skin: 'sumi', followSkin: true };
  assert.equal(optionsForProfile('codex', followed, 'smooth').theme?.background, SMOOTH.terminal);
  assert.equal(optionsForProfile('codex', followed, 'oni').theme?.background, SKINS.sumi.tokens.terminal);
  assert.match(readFileSync(new URL('../index.html', import.meta.url), 'utf8'), /<html[^>]*data-look="smooth"/);
});
test('every Smooth rule is scoped to the Smooth root, so ONI MODE sees none of them', () => {
  const css = readFileSync(new URL('../src/smooth.css', import.meta.url), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
  const tops: string[] = []; let depth = 0; let start = 0;
  for (let index = 0; index < css.length; index++) {
    if (css[index] === '{') { if (depth++ === 0) tops.push(css.slice(start, index).trim()); }
    else if (css[index] === '}') { if (--depth === 0) start = index + 1; }
  }
  assert.ok(tops.length > 0);
  for (const selector of tops) assert.match(selector, /^(:where\()?:root\[data-look="smooth"\]\)?$/, `unscoped: ${selector}`);
  assert.doesNotMatch(css, /#[0-9a-fA-F]{3,8}\b|\b(?:rgb|hsl)a?\(/, 'Smooth colours are its tokens (presentation.ts)');
});
test('Smooth follows Windows unless light or dark was chosen, and its light theme paints followed terminals light', () => {
  assert.equal(storedTheme(null), 'system');
  assert.equal(storedTheme('Light'), 'system', 'only the exact stored word');
  assert.equal(storedTheme('light'), 'light');
  assert.equal(storedTheme('dark'), 'dark');
  const followed = { ...DEFAULT_PRESENTATION, followSkin: true };
  assert.equal(optionsForProfile('codex', followed, 'smooth', true).theme?.background, SMOOTH_LIGHT.terminal);
  assert.equal(optionsForProfile('codex', followed, 'oni', true).theme?.background, SKINS.shinatama.tokens.terminal, 'ONI MODE is always dark');
});
test('the window takes the theme of the look, so light glass never sits on a dark acrylic', () => {
  assert.equal(windowThemeFor('oni', 'light'), 'dark', 'ONI MODE is always dark');
  assert.equal(windowThemeFor('oni', 'system'), 'dark');
  for (const theme of ['light', 'dark', 'system'] as const) assert.equal(windowThemeFor('smooth', theme), theme);
});
