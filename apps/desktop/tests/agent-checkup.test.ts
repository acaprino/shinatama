import test from 'node:test';
import assert from 'node:assert/strict';
import { parseCheckup, stepTone } from '../src/agent-checkup.ts';

const good = {
  agentId: 'claude_code', displayName: 'Claude Code', program: { state: 'found', version: '2.1.4' },
  home: { state: 'ready' }, sidecar: { state: 'ready' }, sets: { state: 'missing' }, authentication: 'unknown', canOpen: false, canLogin: true, issues: ['No set chosen'],
};

test('a checkup is read strictly and only for the agent that was asked about', () => {
  assert.deepEqual(parseCheckup(good, 'claude_code')?.program, { state: 'found', version: '2.1.4' });
  assert.equal(parseCheckup(good, 'codex'), null);
  for (const bad of [null, [], { ...good, program: { state: 'weird' } }, { ...good, home: { state: 'ok' } }, { ...good, issues: [1] }, { ...good, canOpen: 'yes' }, { ...good, program: { state: 'found', version: 'a b' } }]) {
    assert.equal(parseCheckup(bad, 'claude_code'), null);
  }
});

test('a step is drawn in the tone of its state, with its word', () => {
  assert.equal(stepTone('ready'), 'success');
  assert.equal(stepTone('missing'), 'danger');
  assert.equal(stepTone('unsupported'), 'warning');
  assert.equal(stepTone('unknown'), undefined);
});
