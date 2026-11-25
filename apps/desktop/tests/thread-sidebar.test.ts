// What a row of the thread sidebar says (src/thread-sidebar.ts): the name of a thread, its second line, how long ago, and what a refusal says.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { agoText, initials, threadFailureText, threadStateLine, threadTitle, type ThreadRow } from '../src/thread-sidebar.ts';

const label = (id: string | null) => ({ claude_code: 'Claude Code', codex: 'Codex' } as Record<string, string>)[id ?? ''] ?? id ?? 'Session';
const row = (extra: Partial<ThreadRow> = {}): ThreadRow => ({ id: 'w_aaaaaaaaaaaa', title: null, archived: false, usedAt: 0, sessions: 2, lead: null, ...extra });

test('a thread is named by the person, else by its first session, else by the name its agent gave that session', () => {
  assert.equal(threadTitle(row({ title: 'Billing' }), 'ignored', label), 'Billing');
  assert.equal(threadTitle(row(), undefined, label), null, 'no name and no session: the row says "New thread"');
  const lead = (title: string | null) => ({ threadId: 't_aaaaaaaaaaaa', title, harness: 'claude_code' });
  assert.equal(threadTitle(row({ lead: lead('Fix the retry') }), 'agent name', label), 'Fix the retry');
  // A session still named after its agent has no name of its own: the agent's name for it comes first.
  assert.equal(threadTitle(row({ lead: lead('Claude Code') }), 'Refactor billing', label), 'Refactor billing');
  assert.equal(threadTitle(row({ lead: lead('Claude Code') }), undefined, label), 'Claude Code');
  assert.equal(threadTitle(row({ lead: lead(null) }), undefined, label), null);
});

test('the second line of a row counts the sessions, says how many run, and what an empty thread offers', () => {
  assert.equal(threadStateLine({ archived: false, sessions: 3 }, { on: true, running: 0 }), '3 sessions');
  assert.equal(threadStateLine({ archived: false, sessions: 1 }, { on: false, running: 0 }), '1 session');
  assert.equal(threadStateLine({ archived: false, sessions: 4 }, { on: true, running: 3 }), '4 sessions · 3 running');
  assert.equal(threadStateLine({ archived: false, sessions: 4 }, { on: false, running: 2 }), '2 running · tabs hidden');
  assert.equal(threadStateLine({ archived: false, sessions: 0 }, { on: true, running: 0 }), 'Empty · + adds a session');
  assert.equal(threadStateLine({ archived: false, sessions: 0 }, { on: false, running: 0 }), 'Empty');
  assert.equal(threadStateLine({ archived: true, sessions: 2 }, { on: false, running: 0 }), 'Archived · 2 sessions');
  // A session that runs here counts even when the store has not listed it yet.
  assert.equal(threadStateLine({ archived: false, sessions: 0 }, { on: true, running: 1 }), '1 session · 1 running');
});

test('how long ago a thread was used', () => {
  const now = Date.UTC(2026, 10, 25, 12, 0, 0);
  assert.equal(agoText(now - 20_000, now), 'now');
  assert.equal(agoText(now - 5 * 60_000, now), '5 min ago');
  assert.equal(agoText(now - 3 * 3_600_000, now), '3 h ago');
  assert.notEqual(agoText(now - 9 * 86_400_000, now), '');
  assert.equal(agoText(Number.NaN, now), '');
});

test('the mark of a thread in the rail is its first two letters', () => {
  assert.equal(initials('Billing rewrite'), 'BR');
  assert.equal(initials('migrate'), 'MI');
  assert.equal(initials('…'), '·');
});

test('a refused operation says what the store refused, in the person\'s words', () => {
  assert.match(threadFailureText({ code: 'owned_elsewhere' }, 'deleted'), /running\. Stop it first/);
  assert.match(threadFailureText('conflict', 'renamed', 'session'), /This session changed/);
  assert.match(threadFailureText({ code: 'open_unverified' }, 'deleted'), /no longer running/);
  assert.match(threadFailureText({ issue: 'busy' }, 'deleted'), /busy/);
  assert.equal(threadFailureText(new Error('x'), 'archived'), 'The thread could not be archived. Nothing was changed.');
});
