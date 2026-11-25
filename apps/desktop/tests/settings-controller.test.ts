// The Settings page and the services behind it, on a window with no Rust (fake-host.ts) and the stand-in of src/mock/settings.ts: everything
// is read when the page opens, what waits for Save says so, Save sends the models, the launch settings and the memory in turn and stops at the
// first thing that is wrong, a secret is never kept after the page closes, and a stop of the proxy that would cut agents off asks first.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { SettingsController } from '../src/controllers/settings.ts';
import { answers } from '../src/mock/settings.ts';
import { fakeHost, tick, type Answers } from './fake-host.ts';

const open: { stop(): void }[] = [];
afterEach(() => { for (const one of open.splice(0)) one.stop(); overrideInvoke(null); });

function host(extra: Answers = {}) {
  const made = fakeHost({ ...answers, session_index_status: { index: { enabled: false } }, ...extra });
  // The page reads the proxy every 1.5 s while it is open: a test that opens it stops that when it ends.
  open.push({ stop: () => { made.app.services.stopWatching(); made.app.settings.dispose(); } });
  return made;
}

test('the page reads the models, the launch settings, the memory and the switches when it opens, and says what new sessions start with', async () => {
  const { app } = host();
  await app.settings.reload();
  assert.equal(app.settings.modelsLoaded, true);
  assert.equal(app.settings.models.connections.length, 1);
  assert.equal(app.settings.launch.githubUsername, 'acme-dev');
  assert.equal(app.settings.launch.rows.length, 2);
  assert.equal(app.settings.memoryMargin, '1024');
  assert.match(app.settings.line.text, /New sessions start with: full access · GitHub sign-in · 2 variables · memory check with 1024 MiB kept free/);
  await tick(10);
  assert.equal(app.settings.titles, true);
  assert.equal(app.settings.indexing, false);
});

test('a change that waits for Save says so, and Save sends the models, then the launch settings, then the memory', async () => {
  const { app, calls } = host();
  await app.settings.reload();
  app.settings.setFullAccess(false);
  assert.match(app.settings.line.text, /^Not saved yet: agents keep their own protections/);
  app.settings.assign('pi', { connectionId: 'acme', modelId: 'qwen3-coder-30b' });
  app.settings.setMemoryMargin('512');
  const before = calls.length;
  assert.equal(await app.settings.save(), true);
  assert.deepEqual(calls.slice(before).filter(call => /_save$/.test(call.command)).map(call => call.command), ['model_settings_save', 'launch_settings_save', 'memory_settings_save']);
  const launch = calls.find(call => call.command === 'launch_settings_save')!.args!.request as { fullAccess: boolean; githubToken: { op: string }; variables: { value: { op: string } }[] };
  assert.equal(launch.fullAccess, false);
  assert.equal(launch.githubToken.op, 'keep', 'a saved token is kept, never sent back');
  assert.deepEqual(launch.variables.map(variable => variable.value.op), ['keep', 'keep']);
  assert.match(app.settings.line.text, /^Saved\./);
  assert.equal(app.settings.models.assignments.pi?.modelId, 'qwen3-coder-30b');
});

test('Save stops at the first thing that is wrong and sends nothing', async () => {
  const { app, calls } = host();
  await app.settings.reload();
  app.settings.setMemoryMargin('99999');
  assert.match(app.settings.marginProblem, /between 0 and 16384/);
  const before = calls.length;
  assert.equal(await app.settings.save(), false);
  assert.equal(calls.length, before);
  assert.equal(app.settings.line.error, true);
  app.settings.setMemoryMargin('256');
  app.settings.addVariable();
  app.settings.setVariable(2, { name: 'REGION' });
  assert.equal(await app.settings.save(), false);
  assert.match(app.settings.line.text, /REGION needs a value/);
});

test('when the models were saved and the rest was not, the page says so; nothing typed in a secret field outlives the page', async () => {
  const { app } = host({ launch_settings_save: () => { throw 'The store is read-only.'; } });
  await app.settings.reload();
  app.settings.setToken('ghp_secret');
  app.settings.editConnection('acme', { keyInput: 'sk-secret' });
  assert.equal(await app.settings.save(), false);
  assert.match(app.settings.line.text, /Models were saved; the remaining settings were not all saved\. The store is read-only\./);
  app.settings.setToken('ghp_secret');
  app.settings.editConnection('acme', { keyInput: 'sk-secret' });
  app.settings.leave();
  assert.equal(app.settings.launch.tokenInput, '');
  assert.equal(app.settings.connection('acme')?.keyInput, '');
});

test('the terminal presentation takes a size only in range, and is saved a moment after the last change', async () => {
  const { app, calls } = host({ presentation_load: { revision: 1, settings: { version: 1, skin: 'shinatama', followSkin: false, fontFamily: 'profile', fontSize: 13, lineHeight: 115 } } });
  await app.settings.start();
  app.settings.setFontSize('99');
  assert.equal(app.settings.presentation.fontSize, 13);
  assert.match(SettingsController.sizeProblem('99'), /from 8 to 32/);
  assert.equal(SettingsController.sizeProblem('16'), '');
  app.settings.setFontSize('16');
  app.settings.setLineHeight('130');
  assert.equal(app.settings.presentation.fontSize, 16);
  assert.equal(await app.settings.flush(), true);
  const saved = calls.filter(call => call.command === 'presentation_save');
  assert.equal(saved.length, 1, 'two changes, one save');
  assert.equal((saved[0].args!.request as { settings: { fontSize: number; lineHeight: number } }).settings.lineHeight, 130);
});

test('the check of an agent is read strictly, and an answer that is not a checkup is a plain failure to retry', async () => {
  const { app } = host();
  await app.settings.checkAgent('claude_code');
  const found = app.settings.checkups.get('claude_code');
  assert.ok(found && !('error' in found) && found.program.version === '2.1.4');
  await app.settings.checkAgent('codex');
  assert.equal(app.settings.checkups.has('codex'), false, 'an agent that is not qualified is not asked');
  const bad = host({ agent_check: { nonsense: true } });
  await bad.app.settings.checkAgent('claude_code');
  assert.deepEqual(bad.app.settings.checkups.get('claude_code'), { error: 'Agent check did not complete. Retry explicitly.' });
});

test('a stop of the proxy that would cut agents off asks with what it costs, and a no leaves it running', async () => {
  const { app, calls } = host();
  await app.services.refreshProxy();
  assert.equal(app.services.proxy?.phase, 'ready');
  const stopping = app.services.stopProxy();
  await tick(5);
  assert.equal(app.ui.dialog?.kind, 'confirm');
  if (app.ui.dialog?.kind === 'confirm') assert.match(app.ui.dialog.description, /2 agent sessions will lose the connection/);
  app.ui.answer(false);
  await stopping;
  assert.equal(calls.some(call => call.command === 'sidecar_stop'), false);
  const again = app.services.stopProxy();
  await tick(5);
  app.ui.answer(true);
  await again;
  assert.deepEqual(calls.find(call => call.command === 'sidecar_stop')!.args, { force: true });
  assert.equal(app.services.proxy?.phase, 'stopped');
});

test('the switches of Sessions apply at once through their commands, and a refused one goes back and says why', async () => {
  const { app } = host({ session_index_enable: () => { throw 'Indexing is busy.'; } });
  await app.settings.readSwitches();
  await app.settings.setTitles(false);
  assert.equal(app.settings.titles, false);
  assert.equal(app.settings.switchesLine.text, 'Activity titles off.');
  await app.settings.setIndexing(true);
  assert.equal(app.settings.indexing, false);
  assert.deepEqual(app.settings.switchesLine, { text: 'Indexing is busy.', error: true });
});
