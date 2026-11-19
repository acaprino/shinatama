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
