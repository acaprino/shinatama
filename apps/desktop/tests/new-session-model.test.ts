import test from 'node:test';
import assert from 'node:assert/strict';
import { absoluteFolder, firstHarness, sortedHarnesses, startProblem } from '../src/new-session-model.ts';
import { nameProblem } from '../src/controllers/catalog.ts';

const harnesses = [
  { id: 'powershell', label: 'PowerShell', nativeResume: false }, { id: 'pi', label: 'Pi', nativeResume: false },
  { id: 'codex', label: 'Codex', nativeResume: true }, { id: 'claude_code', label: 'Claude Code', nativeResume: true },
];

test('a folder is a drive path or a share, never a relative path or one with control characters', () => {
  assert.equal(absoluteFolder('C:\\work\\shinatama'), true);
  assert.equal(absoluteFolder('d:/work'), true);
  assert.equal(absoluteFolder('\\\\server\\share\\x'), true);
  for (const bad of ['', 'work', '.\\work', 'C:work', 'C:\\a\u0000b', `C:\\${'a'.repeat(4100)}`]) assert.equal(absoluteFolder(bad), false);
});

test('the agents are offered in order with the shell last, and the first choice is the one asked for, the last used, or Claude Code', () => {
  assert.deepEqual(sortedHarnesses(harnesses).map(h => h.id), ['claude_code', 'codex', 'pi', 'powershell']);
  assert.equal(firstHarness(harnesses, 'pi', ''), 'pi');
  assert.equal(firstHarness(harnesses, 'nope', 'codex'), 'codex');
  assert.equal(firstHarness(harnesses, undefined, ''), 'claude_code');
  assert.equal(firstHarness([harnesses[0]], undefined, ''), 'powershell');
  assert.equal(firstHarness([], undefined, ''), '');
});

test('a session starts only in a folder that is a place and with an agent that exists', () => {
  assert.match(startProblem('work', 'codex', harnesses), /Choose a project/);
  assert.match(startProblem('C:\\work', 'gone', harnesses), /Choose an available agent/);
  assert.equal(startProblem(' C:\\work ', 'codex', harnesses), '');
});

test('a project folder name is one Windows name: no separators, no reserved device names, no trailing dot or space', () => {
  assert.equal(nameProblem('my-project'), '');
  for (const bad of ['', '  ', '.', '..', 'a/b', 'a\\b', 'a:b', 'x.', 'x ', 'CON', 'nul.txt', 'COM1', 'LPT¹']) assert.notEqual(nameProblem(bad), '', bad);
  assert.equal(nameProblem('console'), '');
});
