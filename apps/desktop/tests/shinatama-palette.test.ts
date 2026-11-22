// Taken from Imago's tag v0.1.4 with the skins (tests/shinatama-palette.test.ts there).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applyShinatamaPalette, SHINATAMA_PALETTES } from '../src/shinatama-palette.ts';

function styledRoot() {
  const properties = new Map<string, { value: string; priority: string }>();
  const style = {
    setProperty(name: string, value: string, priority = '') { properties.set(name, { value, priority }); },
    getPropertyValue(name: string) { return properties.get(name)?.value ?? ''; },
    getPropertyPriority(name: string) { return properties.get(name)?.priority ?? ''; },
    removeProperty(name: string) { properties.delete(name); },
  };
  return { root: { style } as unknown as HTMLElement, properties, style };
}

test('five saved palette identifiers map controls, text and feedback to their own colours', () => {
  assert.deepEqual(Object.keys(SHINATAMA_PALETTES), ['shinatama', 'neutral', 'section9', 'tctf', 'sumi']);
  const { root, style } = styledRoot();
  for (const [id, { tokens }] of Object.entries(SHINATAMA_PALETTES)) {
    applyShinatamaPalette(root, id, false);
    for (const [role, expected] of Object.entries({
      '--im-ground': tokens.stage, '--im-sheet-solid': tokens.surface, '--im-sheet': tokens.surface,
      '--im-button': tokens['surface-2'], '--im-text-hi': tokens.text, '--im-chip-text': tokens['text-2'],
      '--im-hover': tokens.selection + '44', '--im-selected': tokens.selection, '--im-accent': tokens.accent,
      '--im-danger': tokens.danger, '--im-scrim': tokens.scrim,
    })) assert.equal(style.getPropertyValue(role), expected, id + ' ' + role);
    applyShinatamaPalette(root, id, true);
    assert.equal(style.getPropertyValue('--im-sheet'), tokens.surface + 'cc');
    assert.equal(style.getPropertyValue('--im-sheet-solid'), tokens.surface);
    assert.equal(style.getPropertyValue('--im-terminal'), tokens.terminal + 'b3');
  }
});

test('leaving the palette restores the original cascade on exactly the affected root', () => {
  const first = styledRoot(), second = styledRoot();
  first.style.setProperty('--im-ground', 'red', 'important');
  applyShinatamaPalette(first.root, '__proto__', false);
  assert.equal(first.style.getPropertyValue('--im-ground'), SHINATAMA_PALETTES.shinatama.tokens.stage);
  applyShinatamaPalette(first.root, 'sumi', true);
  applyShinatamaPalette(second.root, 'tctf', false);
  applyShinatamaPalette(first.root, null, false);
  assert.equal(first.properties.size, 1);
  assert.equal(first.style.getPropertyValue('--im-ground'), 'red');
  assert.equal(first.style.getPropertyPriority('--im-ground'), 'important');
  assert.equal(second.style.getPropertyValue('--im-ground'), SHINATAMA_PALETTES.tctf.tokens.stage);
});
