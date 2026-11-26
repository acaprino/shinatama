import test from 'node:test';
import assert from 'node:assert/strict';
import { byRecency, folderName, inProject } from '../src/sessions-model.ts';

const project = String.raw`D:\Projects\shinatama`;

test('a session belongs to a project when its folder is the project or inside it', () => {
  assert.equal(inProject(project, project), true);
  assert.equal(inProject('d:/projects/Shinatama/.worktrees/x/', `${project}\\`), true);
  assert.equal(inProject(String.raw`\\?\D:\Projects\shinatama\apps`, project), true);
  assert.equal(inProject(String.raw`D:\Projects\shinatama-old`, project), false, 'a sibling with the same prefix is another project');
  assert.equal(inProject(null, project), false);
  assert.equal(inProject(null, null), true, 'no project chosen shows everything');
});

test('saved sessions are listed newest first, undated ones last', () => {
  const rows = [{ row: { updatedAt: '2026-11-10T10:00:00Z' } }, { row: { updatedAt: null } }, { row: { updatedAt: '2026-11-14T09:00:00Z' } }, { row: { updatedAt: 'not a date' } }];
  assert.deepEqual(byRecency(rows).map(item => item.row.updatedAt), ['2026-11-14T09:00:00Z', '2026-11-10T10:00:00Z', null, 'not a date']);
});

test('a folder is named by its last part', () => {
  assert.equal(folderName(`${project}\\`), 'shinatama');
  assert.equal(folderName('/home/me/work'), 'work');
  assert.equal(folderName(null), 'Unknown folder');
});

// --- the library of the Sessions view ------------------------------------------------------------------------------------------

import { agentCounts, rootId, runningRows, savedRows, tableRows, RUNNING_GROUP, SAVED_GROUP, type Archive } from '../src/sessions-model.ts';

const rootOf = (id: string, harness: string) => ({ storeRootId: id, configurationRevision: 1, harness, kind: 'box' as const, access: 'read_only' as const, available: true });
const saved = (nativeId: string, title: string, folder: string | null, updatedAt: string | null) => ({
  nativeId, title, workingDirectory: folder, updatedAt, createdAt: null, turns: 1, bytes: 1, parentNativeId: null, archived: false, metadataTruncated: false,
});
const archives: Archive[] = [
  { root: rootOf('cc', 'claude_code'), rows: [saved('a', 'Retry backoff', project, '2026-11-14T10:00:00Z'), saved('b', 'Other project', String.raw`D:\Projects\xiox`, '2026-11-15T10:00:00Z')], next: null, failure: null },
  { root: rootOf('cx', 'codex'), rows: [saved('c', 'Idempotency', `${project}\\apps`, '2026-11-16T10:00:00Z')], next: { offset: 1, fingerprint: 'f' }, failure: null },
];
const none = { project: null, archive: null, agent: null, text: '' };
const ids = (rows: { row: { nativeId: string } }[]) => rows.map(item => item.row.nativeId);

test('the saved list is narrowed by project, archive, agent and text, and keeps the newest first', () => {
  assert.deepEqual(ids(savedRows(archives, none)), ['c', 'b', 'a']);
  assert.deepEqual(ids(savedRows(archives, { ...none, project })), ['c', 'a'], 'a project holds what is inside its folder');
  assert.deepEqual(ids(savedRows(archives, { ...none, archive: { storeRootId: 'cc', configurationRevision: 1 } })), ['b', 'a']);
  assert.deepEqual(ids(savedRows(archives, { ...none, agent: 'codex' })), ['c']);
  assert.deepEqual(ids(savedRows(archives, { ...none, text: ' RETRY ' })), ['a'], 'the text is matched without case or edges');
  assert.deepEqual(ids(savedRows(archives, { ...none, text: 'codex' })), ['c'], 'the agent\'s name is searched too');
  assert.deepEqual(ids(savedRows(archives, { ...none, text: 'xiox' })), ['b'], 'and the folder');
});

const instance = (id: string, agent: string, state = 'quiet') => ({ id, agent, cwd: 'C:\\w', state, stateSource: 'process', certain: true, git: { state: 'not_repository' as const }, usage: '' });

test('the running list answers the agent and the text, and the counts add running and saved', () => {
  const running = [instance('i_a', 'claude_code'), instance('i_b', 'codex')];
  assert.deepEqual(runningRows(running, null, { agent: 'codex', text: '' }).map(row => row.id), ['i_b']);
  assert.deepEqual(runningRows(running, null, { agent: null, text: 'claude' }).map(row => row.id), ['i_a']);
  assert.deepEqual(agentCounts(archives, running), [{ agent: 'claude_code', count: 3 }, { agent: 'codex', count: 2 }]);
  assert.equal(rootId({ storeRootId: 'cc', configurationRevision: 2 }), 'cc\u00002');
});

test('the table holds a heading for each group that has rows, running first', () => {
  const running = [instance('i_a', 'claude_code')];
  const rows = tableRows(running, savedRows(archives, none));
  assert.deepEqual(rows.map(row => row.kind), ['group', 'running', 'group', 'saved', 'saved', 'saved']);
  assert.equal(rows[0].id, RUNNING_GROUP);
  assert.equal(rows[2].id, SAVED_GROUP);
  assert.deepEqual(tableRows([], []), []);
  assert.deepEqual(tableRows(running, []).map(row => row.kind), ['group', 'running'], 'no heading over nothing');
  assert.equal(new Set(rows.map(row => row.id)).size, rows.length, 'every row has an id of its own');
});
