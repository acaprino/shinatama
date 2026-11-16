// ADR-0057: what the conversation panel accepts from a room and how it draws it.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { eventLine, laterEvent, mergeEvents, parseSnapshot, type ChatEvent } from '../src/thread-chat-model.ts';

const event = (eventId: string, extra: Partial<ChatEvent> = {}): ChatEvent => ({
  eventId, timestamp: '2026-11-16T10:00:00.000Z', kind: 'message', actor: 'claude-1', recipients: [], text: 'hi', signal: null, reason: null, ...extra,
});

test('event ids compare as 64-bit numbers, not as text or floats', () => {
  assert.ok(laterEvent('10', '9'));
  assert.ok(!laterEvent('9', '10'));
  assert.ok(laterEvent('18446744073709551615', '18446744073709551614'));
  assert.ok(!laterEvent('7', '7'));
});

test('a later read adds only new events, in order, within the bound', () => {
  const merged = mergeEvents([event('1'), event('3')], [event('3'), event('2'), event('10')]);
  assert.deepEqual(merged.map(item => item.eventId), ['1', '2', '3', '10']);
  const many = Array.from({ length: 450 }, (_, index) => event(String(index + 1)));
  const bounded = mergeEvents([], many);
  assert.equal(bounded.length, 400);
  assert.equal(bounded[0].eventId, '51');
});

test('a snapshot is checked before it is drawn', () => {
  assert.equal(parseSnapshot(null), null);
  assert.deepEqual(parseSnapshot({ enabled: false }), { enabled: false, members: [], sessions: [], events: [] });
  assert.equal(parseSnapshot({ enabled: true, members: [] }), null);
  const parsed = parseSnapshot({
    enabled: true,
    members: [{ memberId: 'm1', name: 'codex-1', harness: 'codex', role: 'peer', active: true, sessionId: 't_aaaaaaaaaaaa', user: false }, { name: 42 }],
    sessions: [{ sessionId: 't_aaaaaaaaaaaa', harness: 'codex', title: null, state: 'running', memberName: 'codex-1', connection: 'in_room', wakes: 2 },
      { sessionId: 't_bbbbbbbbbbbb', harness: 'pi', title: null, state: 'stopped', memberName: null, connection: 'bogus', wakes: null }],
    events: [{ eventId: '5', timestamp: 't', kind: 'message', actor: 'user', recipients: ['codex-1', 7], text: 'x' }, { eventId: '1.5', kind: 'message', timestamp: 't' }],
  })!;
  assert.equal(parsed.members.length, 1);
  assert.equal(parsed.sessions[1].connection, 'not_connected');
  const launched = parseSnapshot({ enabled: true, members: [], events: [],
    sessions: [{ sessionId: 't_pppppppppppp', harness: 'pi', title: null, state: 'running', memberName: null, connection: 'launched', wakes: null }] })!;
  assert.equal(launched.sessions[0].connection, 'launched');
  assert.equal(parsed.events.length, 1);
  assert.deepEqual(parsed.events[0].recipients, ['codex-1']);
});

test('lines say who wrote to whom, and presence apart from messages', () => {
  assert.deepEqual(eventLine(event('1', { actor: 'user', recipients: ['codex-1'], text: 'Review the diff' })),
    { who: 'user → codex-1', what: 'Review the diff', kind: 'message' });
  assert.equal(eventLine(event('2')).who, 'claude-1 → room');
  assert.deepEqual(eventLine(event('3', { kind: 'join', text: null })), { who: 'claude-1', what: 'joined the room', kind: 'presence' });
  assert.equal(eventLine(event('4', { kind: 'signal', signal: 'blocked', reason: 'needs a key' })).what, 'signalled blocked: needs a key');
});
