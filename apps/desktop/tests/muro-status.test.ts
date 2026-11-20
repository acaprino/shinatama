import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseMuroStatus, pluginsButtons, pluginsLine, type MuroStatus, type MuroSync } from '../src/muro-status.ts';

const clock = (seconds: number) => `t${seconds}`;
const sync = (over: Partial<MuroSync> = {}): MuroSync => ({
  lastCheck: 100, lastSync: 90, pending: 0, blockers: 0, current: true, targets: 2, plugins: 3, lastRun: null, ...over,
});
const running = (over: Partial<MuroSync> = {}, message: string | null = null): MuroStatus => ({ state: 'running', message, sync: sync(over) });
const line = (status: MuroStatus) => pluginsLine(status, clock);

test('the backend answer is read strictly', () => {
  const parsed = parseMuroStatus({ state: 'running', message: null, sync: { lastCheck: 5, lastSync: null, pending: 1, blockers: 0, current: true, targets: 4, plugins: 2,
    lastRun: { time: 6, failed: false, message: '1 change applied' } } });
  assert.deepEqual(parsed, { state: 'running', message: null, sync: { lastCheck: 5, lastSync: null, pending: 1, blockers: 0, current: true, targets: 4, plugins: 2,
    lastRun: { time: 6, failed: false, message: '1 change applied' } } });
  assert.deepEqual(parseMuroStatus({ state: 'off', message: null, sync: null }), { state: 'off', message: null, sync: null });
  for (const bad of [null, 3, {}, { state: 'gone' }]) assert.equal(parseMuroStatus(bad), null);
});

test('each state that is not running says why in plain words', () => {
  assert.deepEqual(line({ state: 'missing', message: 'C:\\Shinatama\\muro', sync: null }), { text: 'Muro is not in this build (no Muro in C:\\Shinatama\\muro).', tone: 'info' });
  assert.equal(line({ state: 'off', message: null, sync: null }).text, 'Muro is switched off (SHINATAMA_MURO=off).');
  assert.equal(line({ state: 'starting', message: null, sync: null }).text, 'Muro is starting…');
  assert.deepEqual(line({ state: 'failed', message: 'Registry is busy', sync: null }), { text: 'Muro did not start: Registry is busy', tone: 'error' });
});

test('a running Muro tells what the last check and sync found', () => {
  assert.deepEqual(line(running({ lastRun: { time: 95, failed: false, message: '1 change applied' } })),
    { text: 'In sync · checked t100 · last sync t90 (1 change applied) · 3 plugins for 2 agents.', tone: 'info' });
  assert.equal(line(running({ pending: 2, lastSync: null })).text, '2 changes waiting · checked t100 · 3 plugins for 2 agents.');
  assert.deepEqual(line(running({ blockers: 1 })), { text: 'Needs attention: 1 problem in Muro · checked t100 · last sync t90 · 3 plugins for 2 agents.', tone: 'error' });
  assert.equal(line(running({ current: false })).text, 'Not checked yet · last sync t90 · 3 plugins for 2 agents.');
  assert.equal(line(running({ lastCheck: null, lastSync: null, targets: 1, plugins: 1 })).text, 'Not checked yet · 1 plugin for 1 agent.');
});

test('an empty list, a failed sync and a problem left by the start are said', () => {
  assert.equal(line(running({ plugins: 0 })).text, 'No plugins chosen yet for Shinatama\'s 2 agents: open Muro to choose them.');
  assert.deepEqual(line(running({ lastRun: { time: 99, failed: true, message: 'Codex: cannot read' } })),
    { text: 'The last sync at t99 did not finish: Codex: cannot read.', tone: 'error' });
  const problem = line(running({}, 'Pi could not be added to Muro: overlap'));
  assert.equal(problem.tone, 'error');
  assert.match(problem.text, /^In sync .* Pi could not be added to Muro: overlap$/);
  assert.deepEqual(line({ state: 'running', message: 'Muro\'s state could not be read: busy', sync: null }),
    { text: 'Muro runs in the background. Muro\'s state could not be read: busy', tone: 'error' });
});

test('the buttons are disabled with the reason when they cannot run', () => {
  assert.deepEqual(pluginsButtons(running(), true), { sync: null, open: null });
  assert.deepEqual(pluginsButtons({ state: 'failed', message: 'x', sync: null }, true), { sync: null, open: null }, 'Sync now tries the start again');
  assert.deepEqual(pluginsButtons({ state: 'starting', message: null, sync: null }, true), { sync: 'Muro is still starting.', open: null });
  assert.deepEqual(pluginsButtons({ state: 'missing', message: null, sync: null }, true), { sync: 'Muro is not in this build.', open: 'Muro is not in this build.' });
  assert.deepEqual(pluginsButtons({ state: 'off', message: null, sync: null }, true), { sync: 'Muro is switched off.', open: 'Muro is switched off.' });
  assert.deepEqual(pluginsButtons(running(), false), { sync: 'Open the desktop app to use Muro.', open: 'Open the desktop app to use Muro.' });
  assert.equal(pluginsButtons(null, true).sync, 'Muro\'s state could not be read.');
});
