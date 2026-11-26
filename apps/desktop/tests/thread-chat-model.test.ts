// ADR-0057: what the conversation panel accepts from a room and how it draws it.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { eventLine, laterEvent, mergeEvents, parseSnapshot, type ChatEvent } from '../src/thread-chat-model.ts';

const event = (eventId: string, extra: Partial<ChatEvent> = {}): ChatEvent => ({
  eventId, timestamp: '2026-11-16T10:00:00.000Z', kind: 'message', actor: 'claude-1', host: false, recipients: [], text: 'hi', signal: null, reason: null, ...extra,
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
    events: [{ eventId: '5', timestamp: 't', kind: 'message', actor: 'user', actorHost: true, recipients: ['codex-1', 7], text: 'x' }, { eventId: '1.5', kind: 'message', timestamp: 't' }],
  })!;
  assert.equal(parsed.members.length, 1);
  assert.equal(parsed.sessions[1].connection, 'not_connected');
  const launched = parseSnapshot({ enabled: true, members: [], events: [],
    sessions: [{ sessionId: 't_pppppppppppp', harness: 'pi', title: null, state: 'running', memberName: null, connection: 'launched', wakes: null }] })!;
  assert.equal(launched.sessions[0].connection, 'launched');
  assert.equal(parsed.events.length, 1);
  assert.deepEqual(parsed.events[0].recipients, ['codex-1']);
  assert.equal(parsed.events[0].host, true);
  assert.equal(parsed.members[0].host, false);
});

test('lines say who wrote to whom, and presence apart from messages', () => {
  assert.deepEqual(eventLine(event('1', { actor: 'user', recipients: ['codex-1'], text: 'Review the diff' })),
    { who: 'user → codex-1', what: 'Review the diff', kind: 'message' });
  assert.equal(eventLine(event('2')).who, 'claude-1 → room');
  assert.deepEqual(eventLine(event('3', { kind: 'join', text: null })), { who: 'claude-1', what: 'joined the room', kind: 'presence' });
  assert.equal(eventLine(event('4', { kind: 'signal', signal: 'blocked', reason: 'needs a key' })).what, 'signalled blocked: needs a key');
});

test('what the member shinatama writes is a notice of the team, addressed or not', () => {
  const personal = eventLine(event('5', { actor: 'shinatama', host: true, recipients: ['claude-1'], text: 'TEAM revision 2, set by the user in Shinatama.' }));
  assert.deepEqual(personal, { who: 'shinatama', what: 'TEAM revision 2, set by the user in Shinatama.', kind: 'team', tag: 'to claude-1' });
  assert.deepEqual(eventLine(event('6', { actor: 'shinatama', host: true, recipients: [], text: 'TEAM revision 2.' })).tag, 'notice');
  // An agent called shinatama-ish (not a host), and the user, stay chat lines; the host that took another name is a notice.
  assert.equal(eventLine(event('7', { actor: 'shinatama-2', text: 'hi' })).kind, 'message');
  assert.equal(eventLine(event('8', { actor: 'user', host: true, text: 'hi' })).kind, 'message');
  assert.equal(eventLine(event('9', { actor: 'shinatama', host: false, text: 'hi' })).kind, 'message', 'an agent that took the name is no host');
  assert.equal(eventLine(event('10', { actor: 'shinatama-2', host: true, recipients: ['claude-1'], text: 'TEAM revision 3' })).kind, 'team');
  assert.equal(eventLine(event('11', { actor: 'user-2', host: true, text: 'hi' })).kind, 'message');
  assert.equal(eventLine(event('12', { actor: 'shinatama-x', host: true, text: 'hi' })).kind, 'message');
});

// --- what the conversation panel shows of a room -------------------------------------------------------------------------------------

import { chosenRecipient, clockText, recipientChoices, sessionLine, visibleEvents } from '../src/thread-chat-model.ts';

const room = parseSnapshot({
  enabled: true,
  members: [
    { memberId: 'm1', name: 'claude-1', harness: 'claude_code', role: 'peer', active: true, sessionId: 't_aaaaaaaaaaaa' },
    { memberId: 'm2', name: 'user', harness: 'user', role: 'user', active: true, sessionId: null, user: true },
    { memberId: 'm3', name: 'old-1', harness: 'pi', role: 'peer', active: false, sessionId: null },
    { memberId: 'm4', name: 'extra-1', harness: 'codex', role: 'peer', active: true, sessionId: null },
  ],
  sessions: [{ sessionId: 't_aaaaaaaaaaaa', harness: 'claude_code', title: 'Fix it', state: 'running', memberName: 'claude-1', connection: 'in_room', wakes: 2 }],
  events: [],
})!;
const harness = (id: string) => ({ claude_code: 'Claude Code', codex: 'Codex' } as Record<string, string>)[id] ?? id;

test('a message can go to the members that sessions are, to any other active member that is not the person, or to the whole room', () => {
  const choices = recipientChoices(room, harness);
  assert.deepEqual(choices.map(choice => choice.value), ['claude-1', 'extra-1', '']);
  assert.equal(choices[0].label, 'claude-1 · Claude Code');
  assert.match(choices.at(-1)!.label, /Whole room/);
  assert.deepEqual(recipientChoices(null, harness).map(choice => choice.value), ['']);
});

test('the recipient shown is the person\'s choice while it is still offered, else the first member', () => {
  const choices = recipientChoices(room, harness);
  assert.equal(chosenRecipient(choices, null), 'claude-1');
  assert.equal(chosenRecipient(choices, 'extra-1'), 'extra-1');
  assert.equal(chosenRecipient(choices, ''), '', 'the whole room is a choice too');
  assert.equal(chosenRecipient(choices, 'gone-1'), 'claude-1', 'a member that left is not offered');
});

test('the filter keeps every line, or the messages alone', () => {
  const lines = [event('1', { kind: 'join' }), event('2'), event('3', { kind: 'signal' })];
  assert.equal(visibleEvents(lines, 'all').length, 3);
  assert.deepEqual(visibleEvents(lines, 'messages').map(item => item.eventId), ['2']);
});

test('a session of the thread is told by its name, its member, and how it stands in the room', () => {
  const line = sessionLine(room.sessions[0], harness);
  assert.equal(line.name, 'Fix it · Claude Code');
  assert.equal(line.member, '@claude-1');
  assert.match(line.state, /running · in the room · woken 2×/);
  const bare = sessionLine({ ...room.sessions[0], title: null, memberName: null, harness: null, wakes: null, connection: 'unsupported' }, harness);
  assert.equal(bare.name, 'Session');
  assert.equal(bare.member, '');
  assert.match(bare.state, /cannot take part/);
});

test('the time of a line is read, and empty when it cannot be', () => {
  assert.notEqual(clockText('2026-11-16T10:00:00.000Z'), '');
  assert.equal(clockText('not a time'), '');
});
