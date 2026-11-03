import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Terminal } from '@xterm/xterm';
// These pure token/profile cases do not open a modal. Allow its static module
// to register the browser listener while importing under Node, then restore it.
const previousDocument = Reflect.get(globalThis, 'document');
Reflect.set(globalThis, 'document', { addEventListener() {} });
const { DEFAULT_PRESENTATION, PROFILES, SKINS, applyTerminalProfile, optionsForProfile } = await import('../src/presentation.ts');
if (previousDocument === undefined) Reflect.deleteProperty(globalThis, 'document');
else Reflect.set(globalThis, 'document', previousDocument);

function luminance(hex: string) {
  const values = [1, 3, 5].map(start => Number.parseInt(hex.slice(start, start + 2), 16) / 255)
    .map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722;
}
function contrast(a: string, b: string) { const values = [luminance(a), luminance(b)].sort((a, b) => b - a); return (values[0] + .05) / (values[1] + .05); }
test('every selectable frame skin preserves text and primary-action contrast', () => {
  for (const [id, skin] of Object.entries(SKINS)) {
    for (const surface of ['stage', 'chrome', 'chrome-2', 'surface', 'surface-2', 'terminal'] as const) {
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
