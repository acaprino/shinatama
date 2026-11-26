import test from 'node:test';
import assert from 'node:assert/strict';
import {
  decodeDetail, decodeFind, decodeRaw, detailFailure, findFailure, ledgerFacts, positionText, rawFailure, rawRequest, searchableDetail, validQuery,
} from '../src/session-detail-model.ts';

const root = { storeRootId: 'box-1', configurationRevision: 1 };
const FP = 'a'.repeat(64);
const request = (offset = 0) => ({ root, nativeId: 'one', cursor: offset ? { offset, fingerprint: FP } : null, limit: 50 as const });
const event = (ordinal: number, extra: Record<string, unknown> = {}) => ({ ordinal, entryOrdinal: ordinal + 1, sourceEntryId: null, timestamp: null, kind: 'user_text', text: `t${ordinal}`, truncated: false, payloadOmitted: false, ...extra });
const lineage = { nativeId: 'one', state: 'resolved', selectedState: 'resolved', ancestorState: 'unknown_parent', childrenState: 'resolved', ancestors: [], children: [{ nativeId: 'kid', state: 'resolved', canSelect: true }], totalDirectChildren: 1 };
const reply = (events: unknown[], extra: Record<string, unknown> = {}, total = events.length) => ({
  root, nativeId: 'one', issue: null,
  page: { root, nativeId: 'one', harness: 'claude_code', fingerprint: FP, totalEvents: total, overview: { title: 'T', workingDirectory: null, createdAt: null, updatedAt: null, parentNativeId: null, harnessVersion: null }, events, next: null, notices: [], lineage, ledger: null, ...extra },
});
const code = (run: () => unknown) => { try { run(); } catch (error) { return (error as Error).message; } return null; };

test('a detail answers the session and the page that was asked', () => {
  const page = decodeDetail(reply([event(0), event(1)]), request());
  assert.equal(page.events.length, 2);
  assert.equal(page.lineage.children[0].canSelect, true);
  // The second page starts where the first ended.
  assert.equal(decodeDetail(reply([event(2)], {}, 3), request(2)).events[0].ordinal, 2);
  assert.equal(code(() => decodeDetail(reply([event(5)], {}, 6), request(2))), 'read_unavailable', 'an event out of order');
  assert.equal(code(() => decodeDetail({ ...reply([]), nativeId: 'other' }, request())), 'read_unavailable', 'another session');
  assert.equal(code(() => decodeDetail(reply([event(2)], { fingerprint: 'b'.repeat(64) }, 3), request(2))), 'source_changed');
  assert.equal(code(() => decodeDetail({ ...reply([]), issue: 'session_not_found', page: null }, request())), 'session_not_found');
  assert.equal(code(() => decodeDetail({ ...reply([]), issue: 'something new', page: null }, request())), 'read_unavailable');
  assert.equal(code(() => decodeDetail(reply([event(0, { kind: 'telepathy' })]), request())), 'read_unavailable', 'an event kind that is not known');
  assert.equal(code(() => decodeDetail(reply([event(0)], { next: { offset: 9, fingerprint: FP } }, 20), request())), 'read_unavailable', 'a cursor that skips events');
});

test('a lineage node can be opened only when it is resolved', () => {
  const bad = { ...lineage, children: [{ nativeId: 'kid', state: 'cycle', canSelect: true }] };
  assert.equal(code(() => decodeDetail(reply([], { lineage: bad }), request())), 'read_unavailable');
});

test('the words of a refusal, and of a position', () => {
  assert.match(detailFailure(new Error('source_changed')), /Reload the detail/);
  assert.match(detailFailure('nonsense'), /could not be loaded/);
  assert.equal(positionText(0, 5, 41, 'events'), '1–5 of 41 events');
  assert.equal(positionText(0, 0, 0, 'matches'), '0–0 of 0 matches');
});

test('the model usage says what the agent recorded, and nothing it did not', () => {
  assert.deepEqual(ledgerFacts(null).map(fact => fact.name), ['Token usage']);
  assert.match(ledgerFacts(null)[0].value, /No model requests/);
  const none = { turns: 0, requests: 0, total: { input: 0, cacheRead: 0, cacheWrite: 0, output: 0 }, hitRatePermille: null, compactions: 0, toolCalls: 0, rerunsAfterCompaction: 0, recordsWithoutUsage: 4 };
  assert.match(ledgerFacts(none)[0].value, /did not record usage/);
  const some = { ...none, requests: 31, turns: 12, total: { input: 1200, cacheRead: 98000, cacheWrite: 4000, output: 5200 }, hitRatePermille: 940 };
  const facts = Object.fromEntries(ledgerFacts(some).map(fact => [fact.name, fact.value]));
  assert.match(facts['Prompt tokens'], /^103,200 /);
  assert.equal(facts['Cache hit rate'], '94.0 %');
});

test('a search in a session takes a literal of at most 128 bytes and no control characters', () => {
  assert.equal(validQuery('retry'), true);
  assert.equal(validQuery('   '), false);
  assert.equal(validQuery('a'.repeat(129)), false);
  assert.equal(validQuery('é'.repeat(65)), false, 'bytes, not characters');
  assert.equal(validQuery('tab\there'), false);
  assert.equal(searchableDetail({ root, nativeId: 'one', cursor: { offset: 0, fingerprint: FP }, limit: 50 }), true);
  assert.equal(searchableDetail({ root, nativeId: 'one', cursor: { offset: 3, fingerprint: FP }, limit: 50 }), false, 'only the first page of a verified detail');
});

const findRequest = (cursor: { offset: number; fingerprint: string } | null = null) => ({ detail: { root, nativeId: 'one', cursor: { offset: 0, fingerprint: FP }, limit: 50 as const }, query: 'retry', cursor });
const hit = (matchOrdinal: number, entryOrdinal = matchOrdinal, eventOrdinal = matchOrdinal) => ({ entryOrdinal, eventOrdinal, matchOrdinal, snippet: 's', truncated: false });
const found = (hits: unknown[], total = hits.length, extra: Record<string, unknown> = {}) => ({ root, nativeId: 'one', issue: null, page: { root, nativeId: 'one', sourceFingerprint: FP, query: 'retry', totalHits: total, hits, next: null, ...extra } });

test('a search answers the query and the version of the session that was asked', () => {
  assert.equal(decodeFind(found([hit(0), hit(1)]), findRequest()).hits.length, 2);
  assert.equal(code(() => decodeFind(found([hit(0)], 1, { query: 'other' }), findRequest())), 'read_unavailable');
  assert.equal(code(() => decodeFind(found([hit(0)], 1, { sourceFingerprint: 'b'.repeat(64) }), findRequest())), 'read_unavailable');
  assert.equal(code(() => decodeFind(found([hit(1)], 2), findRequest())), 'read_unavailable', 'the first hit is the first match');
  assert.equal(code(() => decodeFind(found([hit(0, 5), hit(1, 4)], 2), findRequest())), 'read_unavailable', 'hits in order');
  assert.equal(code(() => decodeFind({ ...found([]), issue: { code: 'detail', detail: 'source_changed' }, page: null }, findRequest())), 'source_changed');
  assert.match(findFailure(new Error('fingerprint_required')), /Reload the session detail/);
  assert.match(findFailure(new Error('')), /Session search is unavailable/);
});

const selection = { root, nativeId: 'one', entryOrdinal: 3, fingerprint: FP };
const raw = (extra: Record<string, unknown> = {}) => ({ root, nativeId: 'one', entryOrdinal: 3, sourceFingerprint: FP, rawJson: '{"a":1}', issue: null, ...extra });

test('a raw entry is for the entry and the version that was asked, and at most 64 KiB', () => {
  assert.equal(decodeRaw(raw(), selection), '{"a":1}');
  assert.equal(code(() => decodeRaw(raw({ entryOrdinal: 4 }), selection)), 'read_unavailable');
  assert.equal(code(() => decodeRaw(raw({ sourceFingerprint: 'b'.repeat(64) }), selection)), 'source_changed');
  assert.equal(code(() => decodeRaw(raw({ rawJson: 'x'.repeat(65 * 1024) }), selection)), 'output_limit');
  assert.equal(code(() => decodeRaw(raw({ rawJson: null, issue: { code: 'raw_unavailable' } }), selection)), 'raw_unavailable');
  assert.match(rawFailure(new Error('confirmation_required')), /Confirm the privacy warning/);
  const asked = rawRequest(selection);
  assert.equal(asked.confirmedRaw, true);
  assert.deepEqual(asked.detail.cursor, { offset: 0, fingerprint: FP });
});
