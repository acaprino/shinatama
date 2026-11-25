import test from 'node:test';
import assert from 'node:assert/strict';
import { controlStrip } from '../src/browser-tab.ts';

test('an agent page offers to take control', () => {
  assert.deepEqual(controlStrip({ controller: 'agent:i_a', openedBy: 'i_a' }), { text: "L'agente sta usando questa pagina", action: 'take' });
});

test('an agent page the user took offers to hand it back', () => {
  assert.deepEqual(controlStrip({ controller: 'user', openedBy: 'i_a' }), { text: "Hai il controllo della pagina dell'agente", action: 'give' });
});

test("the user's own page shows no strip", () => {
  assert.equal(controlStrip({ controller: 'user', openedBy: null }), null);
});
