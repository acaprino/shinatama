import assert from 'node:assert/strict';
import { test } from 'node:test';

import { emptyWorkspace, reduceWorkspaceLayout } from '../src/workspace-model.ts';
import { createWorkspacePersistence, layoutLoadFailureText, transientLayoutFailure } from '../src/workspace-persistence.ts';
import type { WorkspaceLayout } from '../src/workspace-model.ts';
import type { WorkspaceSnapshot, WorkspaceTransport, WorkspaceWrite } from '../src/workspace-persistence.ts';

const scope = { projectId: 'p_abcdefghijkl', workspaceId: 'main' };
function edited(layout: WorkspaceLayout, name: string): WorkspaceLayout {
  const result = reduceWorkspaceLayout(layout, { type: 'add-view', groupId: 'group-main', view: {
    id: `view-${name}`, hidden: false, resource: { kind: 'terminal', instanceId: null, threadId: `thread-${name}`, profileId: 'neutral' },
  } });
  if (!result.ok) throw new Error(result.error); return result.layout;
}
function acknowledged(request: WorkspaceWrite): WorkspaceSnapshot {
  return { ...scope, revision: request.expectedRevision + 1, schemaVersion: 1, layoutJson: request.layoutJson, ownership: 'ours', readOnly: false };
}
function fixture() {
  const writes: { request: WorkspaceWrite; resolve: (value: unknown) => void; reject: (cause: unknown) => void }[] = [];
  let inFlight = 0; let maximum = 0; let releases = 0;
  const transport: WorkspaceTransport = {
    async load() { return null; },
    async claim(request) { return acknowledged(request); },
    save(request) {
      inFlight += 1; maximum = Math.max(maximum, inFlight);
      return new Promise<unknown>((resolve, reject) => writes.push({ request, resolve, reject })).finally(() => { inFlight -= 1; });
    },
    async release() { releases += 1; return null; },
  };
  const client = createWorkspacePersistence({ scope, initialLayout: emptyWorkspace(), transport, debounceMs: 1000 });
  return { client, writes, maximum: () => maximum, releases: () => releases };
}
async function ready(client: ReturnType<typeof createWorkspacePersistence>): Promise<void> {
  assert.equal((await client.load()).ok, true); assert.equal((await client.claim()).ok, true);
}
async function turn(): Promise<void> { await new Promise<void>(resolve => setTimeout(resolve, 0)); }

test('a refused load says the storage reason, and the next load can still succeed', async t => {
  let refusals = 1;
  const transport: WorkspaceTransport = {
    async load() { if (refusals-- > 0) throw { code: 'unavailable', message: 'Workspace storage is busy; retry explicitly.' }; return null; },
    async claim(request) { return acknowledged(request); }, async save(request) { return acknowledged(request); }, async release() { return null; },
  };
  const client = createWorkspacePersistence({ scope, initialLayout: emptyWorkspace(), transport }); t.after(() => client.dispose());
  const refused = await client.load();
  assert.equal(refused.ok, false); if (refused.ok) return;
  assert.equal(refused.error, 'unavailable'); assert.equal(refused.detail, 'Workspace storage is busy; retry explicitly.');
  assert.equal(transientLayoutFailure(refused.error), true);
  const text = layoutLoadFailureText(refused.error, refused.detail);
  assert.match(text, /storage is busy/); assert.match(text, /panes on screen stay/); assert.match(text, /shinatama\.log/);
  assert.equal((await client.load()).ok, true); assert.equal((await client.claim()).ok, true);
});

test('a saved layout that fails its checks is not retried and says so', async t => {
  const transport: WorkspaceTransport = {
    async load() { return { ...scope, revision: 3, schemaVersion: 1, layoutJson: '{"version":1}', ownership: 'ours', readOnly: false }; },
    async claim(request) { return acknowledged(request); }, async save(request) { return acknowledged(request); }, async release() { return null; },
  };
  const client = createWorkspacePersistence({ scope, initialLayout: emptyWorkspace(), transport }); t.after(() => client.dispose());
  const refused = await client.load();
  assert.equal(refused.ok, false); if (refused.ok) return;
  assert.equal(refused.error, 'invalid'); assert.equal(transientLayoutFailure(refused.error), false);
  assert.match(layoutLoadFailureText(refused.error, refused.detail), /did not pass its checks/);
});

test('one in-flight save coalesces intermediate edits and releases only the acknowledged revision', async t => {
  const { client, writes, maximum, releases } = fixture(); t.after(() => client.dispose()); await ready(client);
  const first = edited(client.layout(), 'one'); client.update(first); const flushed = client.flush(1000); await turn();
  assert.equal(writes.length, 1);
  const middle = edited(first, 'two'); const latest = edited(middle, 'three'); client.update(middle); client.update(latest);
  writes[0].resolve(acknowledged(writes[0].request)); await turn();
  assert.equal(writes.length, 2); assert.equal(writes[1].request.expectedRevision, 2);
  assert.deepEqual(JSON.parse(writes[1].request.layoutJson), latest);
  writes[1].resolve(acknowledged(writes[1].request)); assert.equal((await flushed).ok, true);
  assert.equal(maximum(), 1); assert.equal(client.status().revision, 3); assert.equal(client.status().dirty, false);
  assert.equal((await client.release(1000)).ok, true); assert.equal(releases(), 1); assert.equal(client.status().phase, 'released');
});

test('CAS conflict stops automatic writes while keeping newer local layout references intact', async t => {
  const { client, writes, releases } = fixture(); t.after(() => client.dispose()); await ready(client);
  const first = edited(client.layout(), 'one'); client.update(first); const flushed = client.flush(1000); await turn();
  const latest = edited(first, 'two'); client.update(latest);
  writes[0].reject({ code: 'conflict' }); assert.deepEqual(await flushed, { ok: false, error: 'conflict' });
  assert.deepEqual(client.layout(), latest); assert.equal(client.status().phase, 'blocked'); assert.equal(client.status().revision, 1);
  assert.equal(client.status().automaticSaves, false); assert.equal(client.status().dirty, true);
  client.update(edited(latest, 'three')); await turn(); assert.equal(writes.length, 1);
  assert.deepEqual(await client.claim(), { ok: false, error: 'conflict' });
  assert.equal((await client.release(20)).ok, false); assert.equal(releases(), 0);
});

test('flush timeout keeps the real request occupied and a late acknowledgement never re-arms autosave', async t => {
  const { client, writes } = fixture(); t.after(() => client.dispose()); await ready(client);
  const first = edited(client.layout(), 'one'); client.update(first);
  assert.deepEqual(await client.flush(5), { ok: false, error: 'timeout' });
  assert.equal(client.status().inFlight, true); assert.equal(client.status().automaticSaves, false);
  const latest = edited(first, 'two'); client.update(latest); assert.equal(writes.length, 1);
  writes[0].resolve(acknowledged(writes[0].request)); await turn();
  assert.equal(client.status().inFlight, false); assert.equal(client.status().phase, 'blocked');
  assert.equal(client.status().automaticSaves, false); assert.deepEqual(client.layout(), latest); assert.equal(writes.length, 1);
  assert.deepEqual(await client.flush(20), { ok: false, error: 'timeout' });
});
