import { test } from 'node:test';
import assert from 'node:assert/strict';
import { newSessionProject } from '../src/session-project.ts';

const jupiter = 'D:\\Projects\\jupiter'; const artcraft = 'D:\\Projects\\artcraft-web';

test('New session beside the projects starts in the project shown there, not in the open thread', () => {
  assert.equal(newSessionProject('project', { openThread: jupiter, sidebar: jupiter, selected: jupiter, current: artcraft }), artcraft);
});

test('a pane\'s + starts in the open thread\'s project', () => {
  assert.equal(newSessionProject('thread', { openThread: jupiter, sidebar: artcraft, selected: artcraft, current: artcraft }), jupiter);
});

test('an explicit folder (Other folder…, a restart) is used as given', () => {
  assert.equal(newSessionProject('thread', { explicit: artcraft, openThread: jupiter, current: jupiter }), artcraft);
  assert.equal(newSessionProject('project', { explicit: artcraft, openThread: jupiter, current: jupiter }), artcraft);
});

test('missing projects fall through in order, and nothing known gives null', () => {
  assert.equal(newSessionProject('thread', { sidebar: jupiter, selected: artcraft, current: artcraft }), jupiter);
  assert.equal(newSessionProject('project', { selected: artcraft, openThread: jupiter, current: '' }), artcraft);
  assert.equal(newSessionProject('project', { openThread: jupiter, current: '  ' }), jupiter);
  assert.equal(newSessionProject('thread', { current: '' }), null);
});
