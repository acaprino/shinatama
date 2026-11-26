import test from 'node:test';
import assert from 'node:assert/strict';
import { gitText, ordered, parsePlan, parseRows, worktreeNameProblem } from '../src/projects-overview-model.ts';

test('the rows are read one by one: a row that is not one is left out, and what is missing has a plain default', () => {
  const rows = parseRows([
    { id: 'p_1', path: 'C:\\a', name: 'a', available: true, pinned: true, lastOpened: 5, git: { state: 'known', branch: 'main', dirty: 1, untracked: 2 }, lastThread: { id: 't', title: 'T', harness: 'codex', revision: 3, updatedAt: 9 } },
    { id: 'p_2', path: 'C:\\b' }, 42, { id: 3 },
  ])!;
  assert.equal(rows.length, 2);
  assert.equal(rows[0].lastThread?.harness, 'codex');
  assert.deepEqual([rows[1].name, rows[1].available, rows[1].pinned, rows[1].git.state], ['', true, false, 'unknown']);
  assert.equal(parseRows({}), null);
});

test('the Git state is a line a person reads', () => {
  assert.equal(gitText({ state: 'known', branch: 'main', dirty: 0, untracked: 0 }), 'main · clean');
  assert.equal(gitText({ state: 'known', branch: null, dirty: 2, untracked: 1 }), 'detached · 2 changed, 1 untracked');
  assert.equal(gitText({ state: 'not_repository' }), 'Not a Git repository');
  assert.match(gitText({ state: 'unknown', reason: 'timeout' }), /unknown \(timeout\)/);
});

test('pinned projects come first, then the last used; a worktree has a short plain name; a plan is read strictly', () => {
  const row = (id: string, pinned: boolean, lastOpened: number) => ({ id, path: id, name: id, available: true, pinned, lastOpened, git: { state: 'not_repository' as const }, lastThread: null });
  assert.deepEqual(ordered([row('a', false, 9), row('b', true, 1), row('c', false, 12)]).map(r => r.id), ['b', 'c', 'a']);
  assert.equal(worktreeNameProblem('task-1'), '');
  for (const bad of ['', 'Task', 'a b', 'x'.repeat(41)]) assert.notEqual(worktreeNameProblem(bad), '');
  assert.deepEqual(parsePlan({ project: 'p', path: 'C:\\p-x', branch: 'x' }), { project: 'p', path: 'C:\\p-x', branch: 'x' });
  assert.equal(parsePlan({ project: 'p' }), null);
});
