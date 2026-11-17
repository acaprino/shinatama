import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildModelSave, connectionHost, firstModelId, fromView, parseModelView, readChoice, suggestedName, usedBy, writeChoice, type View } from '../src/model-settings-model.ts';
const view = (): View => ({ revision: 4, connections: [{ connection: { id: 'shared', name: 'Local', api: 'openai-completions', baseUrl: 'http://localhost:1234/v1', models: [{ id: 'org/model:v2', contextWindow: 32000, maxTokens: 4000, reasoning: false, vision: false }] }, hasApiKey: true }], assignments: { opencode: { connectionId: 'shared', modelId: 'org/model:v2' }, pi: { connectionId: 'shared', modelId: 'org/model:v2' }, hermes: null } });
test('one connection is reused without needing the saved key', () => {
  const original = view(); const form = fromView(original); const built = buildModelSave(form);
  assert.ok('request' in built); assert.equal(built.request.connections.length, 1);
  assert.deepEqual(built.request.connections[0].apiKey, { op: 'keep' });
  form.connections[0].connection.models[0].id = 'edited'; assert.equal(original.connections[0].connection.models[0].id, 'org/model:v2');
  assert.deepEqual(parseModelView(original), original);
});
test('keys can be replaced and cleared without keeping input after reload', () => {
  const form = fromView(view()); form.connections[0].keyInput = 'new-key';
  let built = buildModelSave(form); assert.ok('request' in built); assert.deepEqual(built.request.connections[0].apiKey, { op: 'set', value: 'new-key' });
  form.connections[0].keyInput = ''; form.connections[0].removeKey = true;
  built = buildModelSave(form); assert.ok('request' in built); assert.deepEqual(built.request.connections[0].apiKey, { op: 'clear' });
});
test('removing an assigned connection or model requires a new selection', () => {
  const form = fromView(view()); form.connections = [];
  assert.ok('problem' in buildModelSave(form));
  form.assignments.opencode = form.assignments.pi = null; assert.ok('request' in buildModelSave(form));
});
test('rejects credential URLs, interpolation, duplicate models and invalid token limits', () => {
  for (const baseUrl of ['https://user:secret@example.com', 'https://example.com?key=secret', 'file:///x', 'https://{env:KEY}', 'https://example.com/#fragment']) {
    const form = fromView(view()); form.connections[0].connection.baseUrl = baseUrl; assert.ok('problem' in buildModelSave(form), baseUrl);
  }
  const form = fromView(view()); const c = form.connections[0].connection;
  c.models[0].maxTokens = 32001; assert.ok('problem' in buildModelSave(form));
  c.models[0].maxTokens = 4000; c.models.push({ ...c.models[0] }); assert.ok('problem' in buildModelSave(form));
  c.models.pop(); c.models[0].id = '{env:KEY}'; assert.ok('problem' in buildModelSave(form));
});
test('a connection reports which agents use it, so an unused one can be flagged', () => {
  const form = fromView(view());
  assert.deepEqual(usedBy(form, 'shared'), ['opencode', 'pi']);
  form.assignments.opencode = form.assignments.pi = null;
  assert.deepEqual(usedBy(form, 'shared'), []);
  assert.deepEqual(usedBy(form, 'other'), []);
});
test('the first usable model is the one a quick assignment picks', () => {
  const form = fromView(view()); const c = form.connections[0].connection;
  c.models.unshift({ ...c.models[0], id: '   ' });
  assert.equal(firstModelId(c), 'org/model:v2');
  c.models = [{ ...c.models[0], id: '' }]; assert.equal(firstModelId(c), null);
});
test('a connection is named after its provider host until the user names it', () => {
  assert.equal(suggestedName('https://llm.janura.com/v1'), 'Janura');
  assert.equal(suggestedName('https://api.example.co.uk/v1'), 'Example');
  assert.equal(suggestedName('http://localhost:1234/v1'), 'localhost:1234');
  assert.equal(suggestedName('http://192.168.1.20:8080/v1'), '192.168.1.20:8080');
  assert.equal(suggestedName('not a url'), null);
  assert.equal(connectionHost('https://llm.janura.com/v1'), 'llm.janura.com');
  assert.equal(connectionHost(''), '');
});
test('one model menu value round-trips a connection and model, including colons and slashes', () => {
  const choice = { connectionId: 'shared', modelId: 'org/model:v2' };
  assert.deepEqual(readChoice(writeChoice(choice)), choice);
  assert.equal(readChoice(''), null);
  assert.equal(readChoice('garbage'), null);
});
test('malformed responses cannot reset the form to defaults', () => {
  assert.equal(parseModelView({ revision: 0, connections: [], assignments: {} }), null);
  assert.equal(parseModelView(null), null);
  const invalid = view(); invalid.assignments.pi = { connectionId: 'missing', modelId: 'x' }; assert.equal(parseModelView(invalid), null);
});
