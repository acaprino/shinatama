// The sessions controller and its stores, on a window with no Rust (fake-host.ts): the library read archive by archive, the text search,
// the running sessions, one session's conversation, and what is done with it (search in it, a raw entry, export, hop).
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { fakeHost, tick } from './fake-host.ts';

afterEach(() => overrideInvoke(null));

const FP = 'a'.repeat(64);
const root = (id: string, harness: string, extra: Record<string, unknown> = {}) => ({ storeRootId: id, configurationRevision: 1, harness, kind: 'box', access: 'read_only', available: true, ...extra });
const row = (nativeId: string, extra: Record<string, unknown> = {}) => ({
  nativeId, title: nativeId, workingDirectory: 'C:\\w', updatedAt: '2026-11-14T10:00:00Z', createdAt: null, turns: 1, bytes: 1, parentNativeId: null, archived: false, metadataTruncated: false, ...extra,
});
/** The answers of an archive that lists `all` two rows at a time. */
const lister = (archives: Record<string, string[]>, size = 2) => (args: Record<string, unknown> | undefined) => {
  const request = args?.request as { root: { storeRootId: string; configurationRevision: number }; cursor: { offset: number } | null };
  const all = archives[request.root.storeRootId] ?? [];
  const offset = request.cursor?.offset ?? 0;
  const rows = all.slice(offset, offset + size).map(id => row(id));
  const end = offset + rows.length;
  return { root: request.root, harness: request.root.storeRootId === 'cx' ? 'codex' : 'claude_code', fingerprint: FP, total: all.length, rows, next: end < all.length ? { offset: end, fingerprint: FP } : null };
};

test('the library reads every archive that is there, a page at a time, and says which could not be read', async () => {
  const { app } = fakeHost({
    session_roots: [root('cc', 'claude_code'), root('cx', 'codex'), root('gone', 'pi', { available: false })],
    session_list: lister({ cc: ['a', 'b', 'c'], cx: ['d'] }),
  });
  const sessions = app.sessions;
  await sessions.ensureSaved();
  assert.equal(sessions.loadState, 'ready');
  assert.deepEqual(sessions.saved.map(item => item.row.nativeId).sort(), ['a', 'b', 'd']);
  assert.equal(sessions.more, true, 'the first archive has a page more');
  assert.deepEqual(sessions.failures.map(failure => [failure.root.storeRootId, failure.failure]), [['gone', 'This archive is unavailable.']]);
  await sessions.ensureSaved();
  assert.equal(sessions.archives.length, 3, 'a list that is read is not read again');
  await sessions.loadOlder();
  assert.deepEqual(sessions.saved.map(item => item.row.nativeId).sort(), ['a', 'b', 'c', 'd']);
  assert.equal(sessions.more, false);
  assert.deepEqual(sessions.agents, [{ agent: 'claude_code', count: 3 }, { agent: 'codex', count: 1 }, { agent: 'pi', count: 0 }]);
});

test('an archive that answers in a shape that was not asked for is not trusted, and one that refuses does not stop the others', async () => {
  const { app } = fakeHost({
    session_roots: [root('cc', 'claude_code'), root('cx', 'codex')],
    session_list: (args) => {
      const request = args?.request as { root: { storeRootId: string } };
      if (request.root.storeRootId === 'cc') return { surprise: true };
      throw { issue: 'worker_unavailable' };
    },
  });
  await app.sessions.loadSaved();
  assert.equal(app.sessions.loadState, 'ready');
  assert.equal(app.sessions.failures.length, 2);
  assert.match(app.sessions.failures[0].failure, /unexpected shape/);
  assert.match(app.sessions.failures[1].failure, /contained metadata reader is unavailable/);
  assert.equal(app.sessions.more, false);
});

test('a list of archives that is not a list fails the read and says so, and can be read again', async () => {
  let answer: unknown = 'nonsense';
  const { app } = fakeHost({ session_roots: () => answer, session_list: lister({}) });
  await app.sessions.loadSaved();
  assert.equal(app.sessions.loadState, 'failed');
  assert.match(app.sessions.problem ?? '', /busy or unavailable/);
  answer = [];
  await app.sessions.ensureSaved();
  assert.equal(app.sessions.loadState, 'ready');
  assert.equal(app.sessions.problem, null);
});

test('typing filters the list; Enter searches the text of conversations; typing again leaves the matches', async () => {
  const hit = { storeRootId: 'cc', configurationRevision: 1, nativeId: 'a', harness: 'claude_code', title: 'A', workingDirectory: null, updatedAt: null, snippet: 'the «key»', sourceDeleted: false };
  const requests: unknown[] = [];
  const { app } = fakeHost({
    session_roots: [root('cc', 'claude_code')], session_list: lister({ cc: ['a', 'b'] }),
    session_search: (args) => { requests.push(args?.request); return [hit]; },
  });
  const sessions = app.sessions;
  await sessions.loadSaved();
  sessions.setQuery('b');
  assert.deepEqual(sessions.saved.map(item => item.row.nativeId), ['b']);
  assert.equal(sessions.hits, null);
  sessions.setQuery('key');
  sessions.setAgent('codex');
  await sessions.runSearch();
  assert.deepEqual(requests, [{ query: 'key', harness: 'codex' }]);
  assert.equal(sessions.hits?.length, 1);
  assert.equal(sessions.searched, 'key');
  assert.equal(sessions.saved.length, 0, 'the agent filter still holds');
  sessions.setQuery('keys');
  assert.equal(sessions.hits, null, 'a changed text leaves the matches');
  sessions.clearSearch();
  assert.equal(sessions.query, '');
});

test('a search that fails says why and leaves the list as it was; a reply that is no list of hits fails too', async () => {
  let answer: unknown = () => { throw { issue: 'invalid_request' }; };
  const { app } = fakeHost({ session_roots: [], session_search: () => (typeof answer === 'function' ? (answer as () => unknown)() : answer) });
  app.sessions.setQuery('x');
  await app.sessions.runSearch();
  assert.match(app.sessions.searchProblem ?? '', /Turn indexing on/);
  assert.equal(app.sessions.searching, false);
  answer = { not: 'a list' };
  await app.sessions.runSearch();
  assert.match(app.sessions.searchProblem ?? '', /busy or unavailable/);
  assert.equal(app.sessions.hits, null);
});

test('a search that was answered after the text changed is not shown', async () => {
  let release: (hits: unknown[]) => void = () => {};
  const { app } = fakeHost({ session_roots: [], session_search: () => new Promise(resolve => { release = resolve as typeof release; }) });
  app.sessions.setQuery('old');
  const pending = app.sessions.runSearch();
  app.sessions.setQuery('new');
  release([]);
  await pending;
  assert.equal(app.sessions.hits, null);
});

test('the running sessions are read with their titles and the hops, and only a change is told', async () => {
  const rows = [{ id: 'i_aaaaaaaaaaaa', agent: 'codex', cwd: 'C:\\w', state: 'quiet', stateSource: 'process', certain: true, git: { state: 'not_repository' }, usage: '' }];
  let titles: unknown = { enabled: true, excluded: [], collisions: [], rows: [] };
  const { app, calls } = fakeHost({ instances_overview: () => rows, instance_titles: () => titles, hops_recent: [] });
  let told = 0;
  app.sessions.subscribe(() => { told += 1; });
  await app.sessions.refreshRunning();
  assert.equal(app.sessions.running.length, 1);
  assert.equal(app.sessions.runningRead, true);
  assert.equal(app.sessions.shown.length, 1);
  const first = told;
  await app.sessions.refreshRunning();
  assert.equal(told, first, 'the same picture is not drawn again');
  titles = { enabled: true, excluded: [], collisions: [], rows: [{ instanceId: 'i_aaaaaaaaaaaa', threadId: null, title: 'Bash: ls', reason: null, touched: 1, usage: null }] };
  await app.sessions.refreshRunning();
  assert.equal(told, first + 1);
  assert.deepEqual(calls.filter(call => call.command === 'instance_titles').at(-1)?.args, { requests: [{ instanceId: 'i_aaaaaaaaaaaa', threadId: null, state: 'quiet' }] });
  app.sessions.setAgent('claude_code');
  assert.deepEqual(app.sessions.shown, [], 'another agent is not shown');
});

test('a read of the running sessions that fails says so, and a read that follows clears it', async () => {
  let fail = true;
  const { app } = fakeHost({ instances_overview: () => { if (fail) throw 'The backend is busy.'; return []; }, instance_titles: null, hops_recent: [] });
  await app.sessions.refreshRunning();
  assert.equal(app.sessions.runningProblem, 'The backend is busy.');
  fail = false;
  await app.sessions.refreshRunning(true);
  assert.equal(app.sessions.runningProblem, null);
});

test('watching the running sessions reads at once and stops when the last screen lets go', async () => {
  const { app, calls } = fakeHost({ instances_overview: [], instance_titles: null, hops_recent: [] });
  const stopOne = app.sessions.watchRunning();
  const stopTwo = app.sessions.watchRunning();
  await tick(5);
  assert.ok(calls.filter(call => call.command === 'instances_overview').length >= 1);
  stopOne(); stopOne();
  stopTwo();
  app.sessions.dispose();
});

// --- one session's conversation -------------------------------------------------------------------------------------------------

const rootSel = { storeRootId: 'cc', configurationRevision: 1 };
const events = (n: number) => Array.from({ length: n }, (_, ordinal) => ({ ordinal, entryOrdinal: ordinal + 10, sourceEntryId: null, timestamp: null, kind: ordinal % 2 ? 'assistant_text' : 'user_text', text: `t${ordinal}`, truncated: false, payloadOmitted: false }));
const detailAnswer = (total: number, size = 2) => (args: Record<string, unknown> | undefined) => {
  const request = args?.request as { root: typeof rootSel; nativeId: string; cursor: { offset: number; fingerprint: string } | null };
  const offset = request.cursor?.offset ?? 0;
  const shown = events(total).slice(offset, offset + size);
  const end = offset + shown.length;
  return {
    root: request.root, nativeId: request.nativeId, issue: null,
    page: {
      root: request.root, nativeId: request.nativeId, harness: 'claude_code', fingerprint: FP, totalEvents: total,
      overview: { title: 'T', workingDirectory: null, createdAt: null, updatedAt: null, parentNativeId: null, harnessVersion: null }, events: shown,
      next: end < total ? { offset: end, fingerprint: FP } : null, notices: [],
      lineage: { nativeId: request.nativeId, state: 'resolved', selectedState: 'resolved', ancestorState: 'unknown_parent', childrenState: 'resolved', ancestors: [], children: [], totalDirectChildren: 0 }, ledger: null,
    },
  };
};

test('a conversation is read a page at a time, and the way back goes through the pages that were read', async () => {
  const { app } = fakeHost({ session_detail: detailAnswer(5) });
  const { detail } = app.sessions;
  assert.equal(detail.show(rootSel, 'one'), true);
  await tick(5);
  assert.deepEqual(detail.page?.events.map(event => event.ordinal), [0, 1]);
  assert.equal(detail.request?.cursor?.fingerprint, FP, 'what is done with it names the exact version that was read');
  await detail.next();
  await detail.next();
  assert.deepEqual(detail.page?.events.map(event => event.ordinal), [4]);
  assert.equal(detail.page?.next, null);
  await detail.previous();
  assert.deepEqual(detail.page?.events.map(event => event.ordinal), [2, 3]);
  await detail.reload();
  assert.deepEqual(detail.history, []);
  assert.equal(detail.cursor, null);
});

test('a selector that is not safe to read is refused with a notice, and a conversation that cannot be read says why', async () => {
  const { app, notices } = fakeHost({ session_detail: () => ({ root: rootSel, nativeId: 'one', page: null, issue: 'session_not_found' }) });
  const { detail } = app.sessions;
  assert.equal(detail.show(rootSel, 'not safe!'), false);
  assert.match(notices[0].text, /cannot be read safely/);
  detail.show(rootSel, 'one');
  await tick(5);
  assert.equal(detail.failed, true);
  assert.equal(detail.page, null);
  assert.match(detail.message, /not listed in this archive/);
  assert.equal(notices.at(-1)?.error, true);
});

test('a conversation that was closed while it was read is not shown when the answer arrives', async () => {
  let release: (value: unknown) => void = () => {};
  const { app } = fakeHost({ session_detail: () => new Promise(resolve => { release = resolve; }) });
  const { detail } = app.sessions;
  detail.show(rootSel, 'one');
  detail.close();
  release(detailAnswer(2)({ request: { root: rootSel, nativeId: 'one', cursor: null } }));
  await tick(5);
  assert.equal(detail.page, null);
  assert.equal(detail.current, null);
});

test('a hit of a search opens the conversation at its event, in the page it names', async () => {
  const { app } = fakeHost({ session_detail: detailAnswer(5) });
  const { detail } = app.sessions;
  detail.show(rootSel, 'one', { offset: 2, fingerprint: FP });
  await tick(5);
  assert.deepEqual(detail.page?.events.map(event => event.ordinal), [2, 3]);
  assert.equal(detail.jump, 2);
});

test('the proxy lines of a session are read while it is watched, and only a change is told', async () => {
  const { app } = fakeHost({ session_detail: detailAnswer(1), sidecar_capabilities: { state: 'not_observed' } });
  const { detail } = app.sessions;
  detail.show(rootSel, 'one');
  const stop = detail.watchProxy();
  await tick(10);
  assert.match(detail.proxyLines?.[0] ?? '', /Not observed yet/);
  stop();
  assert.equal(detail.proxyLines, null, 'nothing of it is kept');
});

// --- search in the session, raw, export, hop ---------------------------------------------------------------------------------------

const verified = { root: rootSel, nativeId: 'one', cursor: { offset: 0, fingerprint: FP }, limit: 50 as const };

test('a search in a session takes a verified detail, pages through its matches and opens the one chosen', async () => {
  const hits = (offset: number) => Array.from({ length: Math.min(2, 3 - offset) }, (_, at) => ({ entryOrdinal: offset + at + 10, eventOrdinal: offset + at, matchOrdinal: offset + at, snippet: 's', truncated: false }));
  const { app } = fakeHost({
    session_find: (args) => {
      const request = args?.request as { cursor: { offset: number } | null; query: string };
      const offset = request.cursor?.offset ?? 0;
      const end = offset + hits(offset).length;
      return { root: rootSel, nativeId: 'one', issue: null, page: { root: rootSel, nativeId: 'one', sourceFingerprint: FP, query: request.query, totalHits: 3, hits: hits(offset), next: end < 3 ? { offset: end, fingerprint: FP } : null } };
    },
  });
  const { find } = app.sessions;
  assert.equal(find.open({ ...verified, cursor: { offset: 4, fingerprint: FP } }), false, 'only the first page of a verified detail');
  assert.equal(find.open(verified), true);
  assert.equal(find.canSearch, false, 'nothing typed');
  find.setQuery('retry');
  assert.equal(find.canSearch, true);
  await find.search();
  assert.equal(find.page?.hits.length, 2);
  await find.next();
  assert.equal(find.page?.hits.length, 1);
  await find.previous();
  assert.equal(find.page?.hits[0].matchOrdinal, 0);
  find.choose(find.page!.hits[1]);
  const panel = app.ui.panel;
  assert.equal(panel?.kind === 'session' && panel.at?.offset, 1);
  find.setQuery('other');
  assert.equal(find.page, null, 'a changed query leaves the matches');
});

test('a search that was cancelled says so, and one that failed says why', async () => {
  const fail = { root: rootSel, nativeId: 'one', page: null, issue: { code: 'output_limit' } };
  const { app, notices } = fakeHost({ session_find: () => fail, session_cancel: true });
  const { find } = app.sessions;
  find.open(verified);
  find.setQuery('x');
  await find.search();
  assert.equal(find.failed, true);
  assert.match(find.message, /exceeded its fixed limit/);
  assert.equal(notices.at(-1)?.error, true);
  find.progress.cancel(); // nothing runs: nothing to cancel
  assert.equal(find.progress.line, null);
  find.dispose();
});

const rawSelection = { root: rootSel, nativeId: 'one', entryOrdinal: 3, fingerprint: FP };

test('a raw entry is read only behind a checked warning, and unchecking it clears what was read', async () => {
  const { app, calls } = fakeHost({
    session_raw: (args) => ({ root: rootSel, nativeId: 'one', entryOrdinal: 3, sourceFingerprint: FP, rawJson: '{"native":true}', issue: null, seen: args }),
  });
  const { raw } = app.sessions;
  assert.equal(raw.open({ ...rawSelection, fingerprint: 'nope' }), false);
  assert.equal(raw.open(rawSelection), true);
  assert.equal(calls.filter(call => call.command === 'session_raw').length, 0, 'opening reads nothing');
  await raw.load();
  assert.equal(calls.filter(call => call.command === 'session_raw').length, 0, 'not without the warning checked');
  raw.confirm(true);
  await raw.load();
  assert.equal(raw.content, '{"native":true}');
  assert.equal((calls.find(call => call.command === 'session_raw')?.args?.request as { confirmedRaw: boolean }).confirmedRaw, true);
  raw.confirm(false);
  assert.equal(raw.content, '');
  assert.match(raw.message, /cleared/);
  raw.confirm(true);
  await raw.load();
  raw.clear();
  assert.equal(raw.content, '');
  assert.equal(raw.confirmed, false, 'closing clears the view, the warning too');
});

test('a raw entry that cannot be read says why', async () => {
  const { app } = fakeHost({ session_raw: () => ({ root: rootSel, nativeId: 'one', entryOrdinal: 3, sourceFingerprint: FP, rawJson: null, issue: { code: 'raw_unavailable' } }) });
  const { raw } = app.sessions;
  raw.open(rawSelection);
  raw.confirm(true);
  await raw.load();
  assert.equal(raw.failed, true);
  assert.match(raw.message, /no RAW record/);
  assert.equal(raw.content, '');
});

const exportSelection = { root: rootSel, nativeId: 'one', fingerprint: FP, harness: 'claude_code' };
const saved = { state: 'saved', path: 'C:\\x.stx.jsonl', bytes: 9, sha256: 'b'.repeat(64), attachments: 0, report: { items: 3, redactions: {}, omitted: { reasoning: 0, meta: 0, structured_results: 0 }, attachmentsCopied: 0, attachmentsNotCopied: 0, attachmentsReferenced: 0, declaration: { label: 'STX', losses: [], lossless_backup: false } } };

test('an export asks for what was chosen, and says what was written', async () => {
  const { app, calls } = fakeHost({ session_export: saved });
  const { exporter } = app.sessions;
  assert.equal(exporter.open({ ...exportSelection, nativeId: 'no way' }), false);
  exporter.open(exportSelection);
  exporter.setMode('portable');
  exporter.setAttachments(true);
  await exporter.save();
  const request = calls.find(call => call.command === 'session_export')?.args?.request;
  assert.deepEqual(request, { root: rootSel, nativeId: 'one', fingerprint: FP, format: 'stx', mode: 'portable', withAttachments: true });
  assert.equal(exporter.saved?.path, 'C:\\x.stx.jsonl');
  assert.equal(exporter.message, 'Saved 9 bytes.');
  exporter.setFormat('mukade');
  assert.equal(exporter.withAttachments, false, 'portable JSON never copies attachment content');
  exporter.close();
  assert.equal(exporter.selected, null);
});

test('an export that was cancelled writes nothing, and one that was refused says why', async () => {
  let answer: unknown = { state: 'cancelled' };
  const { app, notices } = fakeHost({ session_export: () => answer });
  const { exporter } = app.sessions;
  exporter.open(exportSelection);
  await exporter.save();
  assert.equal(exporter.message, 'Nothing was written.');
  assert.equal(exporter.saved, null);
  answer = { state: 'refused', issue: 'write_failed:Access is denied.' };
  await exporter.save();
  assert.equal(exporter.failed, true);
  assert.equal(exporter.message, 'Access is denied.');
  assert.equal(notices.at(-1)?.error, true);
  answer = Promise.reject({ issue: 'source_changed' });
  await exporter.save();
  assert.match(exporter.message, /Reload its detail/);
  exporter.dispose();
});

const previewAnswer = (destination: string) => ({
  state: 'ready',
  preview: {
    hop_id: 'h', nonce: 'n', destination, channel: 'file', seed: 'SEED', seed_sha256: 'c'.repeat(64), bootstrap: 'b', bootstrap_sha256: 'd'.repeat(64),
    seed_path: 's.md', full_path: 'f.md', files_in_repository: false, report: { turns_total: 1, verbatim_turns: 1, digest_turns: 0, omitted_turns: 0, clipped_outputs: 0, bytes: 5, estimated_tokens: 2, budget: null },
    redactions: {}, nested_seeds_dropped: 0, injected_context_dropped: 0, carried: [], lost: [],
  },
});

test('a hop shows the exact seed first, and runs only what was shown', async () => {
  const { app, calls, notices } = fakeHost({ hop_preview: (args) => previewAnswer((args?.request as { destination: string }).destination) });
  const hopped: unknown[] = [];
  (app.hop as unknown as { run: unknown }).run = async (preview: unknown, progress: (m: string) => void) => { hopped.push(preview); progress('Opening…'); return 'Hop committed.'; };
  const { hopPreview } = app.sessions;
  hopPreview.open(exportSelection, 'codex');
  await hopPreview.send();
  assert.deepEqual(hopped, [], 'nothing is sent before a preview');
  hopPreview.setDestination('pi');
  await hopPreview.prepare();
  assert.equal(hopPreview.preview?.destination, 'pi');
  assert.deepEqual(calls.find(call => call.command === 'hop_preview')?.args?.request, { root: rootSel, nativeId: 'one', fingerprint: FP, destination: 'pi' });
  hopPreview.setDestination('opencode');
  assert.equal(hopPreview.preview, null, 'another destination needs its own preview');
  await hopPreview.prepare();
  await hopPreview.send();
  assert.equal((hopped[0] as { destination: string }).destination, 'opencode');
  assert.equal(hopPreview.preview, null);
  assert.equal(notices.at(-1)?.text, 'Hop committed.');
});

test('a hop that is refused or fails says why, and the panel is given back when the destination needs the person', async () => {
  let outcome: unknown = { state: 'refused', issue: 'x', message: 'The seed is over its budget.' };
  const { app } = fakeHost({ hop_preview: () => outcome });
  const { hopPreview } = app.sessions;
  hopPreview.open(exportSelection, 'codex');
  await hopPreview.prepare();
  assert.equal(hopPreview.failed, true);
  assert.equal(hopPreview.message, 'The seed is over its budget.');
  outcome = previewAnswer('codex');
  await hopPreview.prepare();
  app.ui.openPanel({ kind: 'hop', selection: exportSelection });
  (app.hop as unknown as { run: unknown }).run = async (_: unknown, __: unknown, release: () => void) => { release(); throw 'The destination exited before the seed was sent; the source is untouched.'; };
  await hopPreview.send();
  assert.equal(app.ui.panel, null, 'the screen is free for the destination\'s question');
  assert.equal(hopPreview.failed, true);
  assert.match(hopPreview.message, /exited before the seed was sent/);
});

test('a hop begun from the facts of a session first reads the exact version that the preview is made of', async () => {
  const { app } = fakeHost({ session_detail: detailAnswer(2) });
  await app.sessions.hop(rootSel, 'one');
  const panel = app.ui.panel;
  assert.equal(panel?.kind, 'hop');
  assert.deepEqual(panel?.kind === 'hop' && panel.selection, { root: rootSel, nativeId: 'one', fingerprint: FP, harness: 'claude_code' });
  const refused = fakeHost({ session_detail: () => ({ root: rootSel, nativeId: 'one', page: null, issue: 'root_unavailable' }) });
  await refused.app.sessions.hop(rootSel, 'one');
  assert.equal(refused.app.ui.panel, null);
  assert.match(refused.notices[0].text, /archive is unavailable/);
});

// --- the index ------------------------------------------------------------------------------------------------------------------------

test('the index is read, started, stopped and forgotten (after asking), and each leaves the state the backend says', async () => {
  let documents = 10;
  let phase = 'idle';
  const { app, calls } = fakeHost({
    session_index_status: () => ({ progress: { phase }, index: { enabled: true, documents, sourceDeleted: 0, bodyBytes: 0 } }),
    session_index_start: () => { phase = 'running'; return null; },
    session_index_cancel: () => { phase = 'stopped'; return null; },
    session_index_forget: () => { documents = 0; return 10; },
  });
  const { sessions } = app;
  await sessions.refreshIndex();
  assert.equal(sessions.index?.index.documents, 10);
  await sessions.startIndex();
  assert.equal(sessions.index?.progress.phase, 'running');
  await sessions.stopIndex();
  assert.equal(sessions.index?.progress.phase, 'stopped');
  const asking = sessions.forgetIndex();
  assert.equal(app.ui.dialog?.kind, 'confirm');
  assert.match(app.ui.dialog?.kind === 'confirm' ? app.ui.dialog.confirm : '', /Forget indexed text/);
  app.ui.answer(false);
  await asking;
  assert.equal(calls.some(call => call.command === 'session_index_forget'), false, 'a no forgets nothing');
  const again = sessions.forgetIndex();
  app.ui.answer(true);
  await again;
  assert.equal(sessions.index?.index.documents, 0);
  sessions.dispose();
});

// --- the archive browser ---------------------------------------------------------------------------------------------------------------

test('the archive browser pages one archive the person chose, and registers a read-only archive', async () => {
  const roots = [root('cc', 'claude_code'), root('cx', 'codex')];
  const { app, calls } = fakeHost({
    session_roots: () => roots, session_list: lister({ cc: ['a', 'b', 'c'], cx: ['d'], added: ['z'] }),
    session_archive_add: () => { const added = root('added', 'pi'); roots.push(added); return added; },
  });
  const browser = app.sessions.archiveBrowser;
  await browser.loadRoots();
  assert.equal(browser.roots.length, 2);
  assert.equal(browser.page, null, 'an archive is chosen, never assumed');
  browser.choose('0');
  await tick(5);
  assert.equal(browser.position, '1–2 of 3');
  await browser.next();
  assert.equal(browser.position, '3–3 of 3');
  assert.equal(browser.history.length, 1);
  await browser.previous();
  assert.equal(browser.position, '1–2 of 3');
  assert.equal(await browser.add('claude_code', '   '), false);
  assert.equal(browser.failed, true);
  assert.equal(await browser.add('pi', 'C:\\pi-sessions'), true);
  assert.equal(browser.roots.length, 3);
  assert.equal(browser.root?.storeRootId, 'added', 'the archive that was added is the one chosen');
  assert.ok(calls.some(call => call.command === 'session_archive_add'));
  browser.forget();
  assert.deepEqual(browser.roots, []);
});
