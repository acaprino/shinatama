import test from 'node:test';
import assert from 'node:assert/strict';
import { chatViewId, teamViewId, threadOfChatView, threadOfTeamView } from '../src/view-ids.ts';

test('the views of a thread are named by it, and only by it', () => {
  assert.equal(threadOfTeamView(teamViewId('w_abc')), 'w_abc');
  assert.equal(threadOfChatView(chatViewId('w_abc')), 'w_abc');
  assert.equal(threadOfTeamView(chatViewId('w_abc')), null);
  assert.equal(threadOfChatView('saved-w_abc'), null);
});
