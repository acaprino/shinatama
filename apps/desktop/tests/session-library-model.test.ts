import test from 'node:test';
import assert from 'node:assert/strict';
import {
  agentName, archiveName, failureText, hitsValue, pageValue, refusalCode, rootsValue, searchFailureText, sizeText, snippetParts, when,
} from '../src/session-library-model.ts';

const root = { storeRootId: 'box-1', configurationRevision: 1, harness: 'claude_code', kind: 'box', access: 'read_only', available: true, label: 'Claude Code' };
const row = (nativeId: string, extra: Record<string, unknown> = {}) => ({
  nativeId, title: 'T', workingDirectory: 'C:\\w', updatedAt: null, createdAt: null, turns: 1, bytes: 10, parentNativeId: null, archived: false, metadataTruncated: false, ...extra,
});
const FP = 'a'.repeat(64);
const page = (rows: unknown[], extra: Record<string, unknown> = {}) => ({ root: { storeRootId: 'box-1', configurationRevision: 1 }, harness: 'claude_code', fingerprint: FP, total: rows.length, rows, next: null, ...extra });
const parsedRoot = rootsValue([root])![0];

test('the archives the worker lists are accepted only in the shape asked for', () => {
  assert.deepEqual(rootsValue([root]), [root]);
  assert.equal(rootsValue('nope'), null);
  assert.equal(rootsValue([{ ...root, harness: 'Claude Code' }]), null, 'a harness id is lower case with underscores');
  assert.equal(rootsValue([{ ...root, kind: 'cloud' }]), null);
  assert.equal(rootsValue([{ ...root, configurationRevision: 0 }]), null, 'a revision starts at one');
  assert.equal(rootsValue([root, root]), null, 'the same archive twice is no list');
  assert.equal(rootsValue([{ ...root, label: 'bad\u0000label' }]), null, 'a control character is no label');
});

test('a page of sessions must answer the archive and the cursor that asked, and add up', () => {
  const ok = pageValue(page([row('a'), row('b')]), parsedRoot, null);
  assert.deepEqual(ok?.rows.map(item => item.nativeId), ['a', 'b']);
  assert.equal(pageValue(page([row('a')], { harness: 'codex' }), parsedRoot, null), null, 'another agent');
  assert.equal(pageValue(page([row('a')], { root: { storeRootId: 'other', configurationRevision: 1 } }), parsedRoot, null), null, 'another archive');
  assert.equal(pageValue(page([row('a'), row('a')]), parsedRoot, null), null, 'a session listed twice');
  assert.equal(pageValue(page([row('a')], { fingerprint: 'x' }), parsedRoot, null), null, 'a fingerprint is a digest');
  // A next page: the cursor must start where this page ends and carry the same fingerprint.
  assert.equal(pageValue(page([row('a')], { total: 3, next: { offset: 1, fingerprint: FP } }), parsedRoot, null)?.next?.offset, 1);
  assert.equal(pageValue(page([row('a')], { total: 3, next: { offset: 2, fingerprint: FP } }), parsedRoot, null), null, 'a cursor that skips a row');
  assert.equal(pageValue(page([row('a')], { total: 3, next: null }), parsedRoot, null), null, 'rows missing and no way to the rest');
  assert.equal(pageValue(page([row('b')], { total: 3 }), parsedRoot, { offset: 2, fingerprint: 'b'.repeat(64) }), null, 'the source changed while paging');
});

test('the hits of a text search are read, and a malformed one refuses the whole list', () => {
  const hit = { storeRootId: 'box-1', configurationRevision: 1, nativeId: 'a', harness: 'codex', title: null, workingDirectory: null, updatedAt: null, snippet: 'x «y» z', sourceDeleted: false };
  assert.equal(hitsValue([hit])?.length, 1);
  assert.equal(hitsValue([{ ...hit, sourceDeleted: 'no' }]), null);
  assert.equal(hitsValue({}), null);
});

test('a refusal says what to do, and an unknown one says only that it failed', () => {
  assert.match(failureText({ issue: 'worker_unavailable' }), /contained metadata reader is unavailable/);
  assert.match(failureText('root_changed'), /Reload archives/);
  assert.match(failureText({ code: 'surprise' }), /could not be loaded/);
  assert.equal(refusalCode(new Error('source_changed')), 'source_changed');
  assert.match(searchFailureText({ issue: 'invalid_request' }), /Turn indexing on/);
  assert.match(searchFailureText('anything else'), /busy or unavailable/);
});

test('a time is said short today, with its day otherwise, and as recorded when it is not a date', () => {
  const now = new Date('2026-11-25T12:00:00');
  assert.equal(when(null), 'Unknown');
  assert.equal(when('not a date'), 'not a date');
  assert.doesNotMatch(when('2026-11-25T09:30:00', now), /Nov/);
  assert.match(when('2026-11-20T09:30:00', now), /20/);
  assert.match(when('2025-11-20T09:30:00', now), /2025/, 'another year says the year');
});

test('sizes, agent names and archive names', () => {
  assert.equal(sizeText(null), 'Unknown');
  assert.equal(sizeText(512), '512 B');
  assert.equal(sizeText(2048), '2 KB');
  assert.equal(sizeText(3 * 1048576), '3.0 MB');
  assert.equal(agentName('claude_code'), 'Claude Code');
  assert.equal(agentName('unknown_one'), 'unknown_one');
  assert.equal(archiveName({ ...parsedRoot, available: false }), 'Claude Code · Shinatama · unavailable');
});

test('a snippet is cut at its markers and never read as markup', () => {
  assert.deepEqual(snippetParts('the «idempotency» key'), [{ text: 'the ', match: false }, { text: 'idempotency', match: true }, { text: ' key', match: false }]);
  assert.deepEqual(snippetParts('<b>«a»</b>'), [{ text: '<b>', match: false }, { text: 'a', match: true }, { text: '</b>', match: false }]);
  assert.deepEqual(snippetParts('open « but never closed'), [{ text: 'open « but never closed', match: false }]);
  assert.deepEqual(snippetParts(''), []);
});
