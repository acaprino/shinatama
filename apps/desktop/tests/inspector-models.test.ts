// The pure decisions of the inspector's panels: export and hop, the running instances, the index of session text, the files, a thread's settings.
import test from 'node:test';
import assert from 'node:assert/strict';
import { absolutePath, ancestorsOf, dirty, entryNode, fileStatus, formatSize, isImage, isMarkdown, isNote, isPage, listingNodes, searchText, shortFolder } from '../src/files-model.ts';
import { collisionText, doingText, gitText, matchesFilter, stateTone, summaryLine, usageText, type InstanceRow } from '../src/instances-model.ts';
import { destinations, exportFacts, exportFailure, exportRefusal, hopFacts, redactedText, transferable, type HopPreview } from '../src/session-export-model.ts';
import { indexButtons, indexLines, indexReason, indexSummary, indexValue, isRunning } from '../src/session-index-model.ts';
import { AGENT_CHOICES } from '../src/session-library-model.ts';
import { cleanName, operationFailure } from '../src/thread-settings-model.ts';

const entry = (name: string, kind: 'file' | 'directory' | 'link', extra: Record<string, unknown> = {}) => ({ name, path: `src/${name}`, kind, size: 1536, linkInside: null, ...extra });

test('a folder\'s rows load when the folder opens, a link out of the project is never opened, and a note says when a folder is empty or cut', () => {
  assert.equal(entryNode(entry('lib', 'directory')).hasChildren, true);
  assert.equal(entryNode(entry('a.ts', 'file')).hasChildren, undefined);
  assert.equal(entryNode(entry('a.ts', 'file')).meta, '1.5 KB');
  assert.equal(entryNode(entry('out', 'link', { linkInside: false })).disabled, true);
  assert.equal(entryNode(entry('in', 'link', { linkInside: true })).hasChildren, true);
  assert.equal(entryNode(entry('a.ts', 'file'), true).meta, 'src', 'a search hit says where it is');
  const empty = listingNodes({ path: 'src', entries: [], truncated: false });
  assert.equal(empty.length, 1);
  assert.equal(isNote(empty[0].id), true);
  assert.equal(empty[0].name, 'Empty folder');
  assert.match(listingNodes({ path: '', entries: [entry('a', 'file')], truncated: true }).at(-1)!.name, /More than 2,000/);
  assert.equal(listingNodes({ path: '', entries: [entry('a', 'file')], truncated: false }).length, 1);
});

test('sizes, folders of a hit, and what a path is', () => {
  assert.equal(formatSize(null), '');
  assert.equal(formatSize(12), '12 B');
  assert.equal(formatSize(12 * 1024), '12 KB');
  assert.equal(formatSize(1536), '1.5 KB');
  assert.equal(shortFolder(''), '/');
  assert.equal(shortFolder('src'), 'src');
  assert.equal(shortFolder('a/b/c/d'), '…/c/d');
  assert.equal(isMarkdown('docs/Plan.MD'), true);
  assert.equal(isMarkdown('a.mdx'), false);
  assert.equal(isImage('x/logo.PNG'), true);
  assert.equal(isImage('x/spec.docx'), true, 'a document is shown as a picture of it');
  assert.equal(isPage('index.html'), true);
  assert.equal(absolutePath('C:\\work\\p\\', 'src/a b.html'), 'C:\\work\\p\\src\\a b.html');
  assert.deepEqual(ancestorsOf('a/b/c.ts'), ['a', 'a/b']);
  assert.deepEqual(ancestorsOf('c.ts'), []);
});

test('a buffer is dirty when its text is not the text that was read, and the status says what can be done', () => {
  const buffer = { project: 'p', path: 'a.ts', baseText: 'x', baseSha256: 's', text: 'x', editable: true };
  assert.equal(dirty(buffer), false);
  assert.equal(dirty({ ...buffer, text: 'y' }), true);
  assert.match(fileStatus(buffer, false), /^1 line · edit and press Save/);
  assert.match(fileStatus({ ...buffer, text: 'a\nb', editable: false }, false), /^2 lines · read-only/);
  assert.match(fileStatus(buffer, true), /larger than 4 MiB/);
  assert.equal(searchText({ query: 'ret', hits: [], truncated: false, cancelled: false, timedOut: false }), '0 matches for “ret”.');
  assert.match(searchText({ query: 'a', hits: [], truncated: true, cancelled: false, timedOut: false }), /Too many matches/);
  assert.match(searchText({ query: 'a', hits: [], truncated: false, cancelled: false, timedOut: true }), /partial/);
});

const instance = (extra: Partial<InstanceRow> = {}): InstanceRow => ({ id: 'i_aaaaaaaaaaaa', agent: 'claude_code', cwd: 'C:\\w', state: 'quiet', stateSource: 'process', certain: true, git: { state: 'not_repository' }, usage: '', ...extra });

test('the running sessions are summed up most urgent first, and filtered by what a person would type', () => {
  assert.equal(summaryLine([]), '');
  assert.equal(summaryLine([{ state: 'quiet' }, { state: 'attention_requested' }, { state: 'quiet' }]), '1 asking for attention · 2 quiet');
  assert.equal(stateTone('attention_requested'), 'warning');
  assert.equal(stateTone('quiet'), undefined);
  assert.equal(gitText({ state: 'known', branch: 'main', dirty: 2, untracked: 1 }), 'main · 2 changed, 1 untracked');
  assert.equal(gitText({ state: 'known', branch: null, dirty: 0, untracked: 0 }), 'detached · clean');
  assert.equal(gitText({ state: 'unknown', reason: 'x' }), 'Git unknown');
  assert.equal(usageText({ requests: 3, promptTokens: 1200, outputTokens: 40, hitRatePermille: 940 }), '1,200 in · 40 out · 3 req · cache 94 %');
  assert.equal(doingText(undefined), null);
  assert.equal(doingText({ instanceId: 'x', threadId: null, title: null, reason: 'no tool call yet', touched: 0, usage: null }), '(no tool call yet)');
  assert.equal(matchesFilter(instance(), undefined, 'claude'), true);
  assert.equal(matchesFilter(instance(), undefined, 'codex'), false);
  assert.equal(matchesFilter(instance(), { instanceId: 'x', threadId: null, title: 'git push', reason: null, touched: 0, usage: null }, 'PUSH'), true);
  assert.match(collisionText({ kind: 'file_collision', worktree: 'w', path: 'a.ts', instances: ['i_aaaaaaaaaaaa', 'gone'] }, [instance()]), /Both Claude Code \(aaaa\) and gone changed a\.ts/);
  assert.match(collisionText({ kind: 'possible_collision', worktree: 'w', path: 'a.ts' }, []), /possible collision/);
});

const preview = (extra: Partial<HopPreview> = {}): HopPreview => ({
  hop_id: 'h', nonce: 'n', destination: 'codex', channel: 'file', seed: 'SEED', seed_sha256: 'c'.repeat(64), bootstrap: 'b', bootstrap_sha256: 'd'.repeat(64),
  seed_path: 's.md', full_path: 'f.md', files_in_repository: true, report: { turns_total: 4, verbatim_turns: 3, digest_turns: 1, omitted_turns: 0, clipped_outputs: 0, bytes: 2000, estimated_tokens: 500, budget: null },
  redactions: { token: 2 }, nested_seeds_dropped: 0, injected_context_dropped: 0, carried: [], lost: [], ...extra,
});

test('what an export or a hop says before and after it writes or sends', () => {
  assert.equal(transferable({ nativeId: 'abc_1', fingerprint: 'a'.repeat(64) }), true);
  assert.equal(transferable({ nativeId: 'abc_1', fingerprint: 'nope' }), false);
  assert.equal(transferable({ nativeId: 'no way', fingerprint: 'a'.repeat(64) }), false);
  assert.equal(exportRefusal('write_failed:Access is denied.'), 'Access is denied.');
  assert.match(exportRefusal('format_unavailable'), /not available for this agent/);
  assert.match(exportRefusal('whatever'), /Export is unavailable/);
  assert.match(exportFailure({ issue: 'source_changed' }), /Reload its detail/);
  assert.equal(redactedText({}), 'Nothing matched a secret pattern');
  assert.equal(redactedText({ token: 2, key: 1 }), 'token 2, key 1');
  const facts = exportFacts({ state: 'saved', path: 'C:\\x.stx.jsonl', bytes: 5, sha256: 'e'.repeat(64), attachments: 1, report: { items: 41, redactions: {}, omitted: { reasoning: 3, meta: 2, structured_results: 0 }, attachmentsCopied: 1, attachmentsNotCopied: 0, attachmentsReferenced: 2, declaration: { label: 'STX', losses: ['No reasoning.'], lossless_backup: false } } });
  assert.deepEqual(facts.map(fact => fact.name), ['File', 'Items', 'Redacted', 'Left out', 'Attachments', 'Losses', 'SHA-256']);
  assert.equal(facts.find(fact => fact.name === 'Attachments')!.value, '2 referenced, 1 copied');
  assert.deepEqual(destinations('codex', AGENT_CHOICES).map(([id]) => id), ['claude_code', 'opencode', 'pi', 'hermes', 'deepseek']);
  const said = Object.fromEntries(hopFacts(preview()).map(fact => [fact.name, fact.value]));
  assert.match(said.Seed, /2,000 bytes, about 500 tokens of \? allowed \(window unknown: 24,000 cap\)/);
  assert.equal(said.Turns, '4 total: 3 verbatim, 1 as digests, 0 omitted');
  assert.equal(said.Redacted, 'token 2');
  assert.match(said.Files, /inside \.git, never tracked/);
});

const state = (phase: string, extra: Record<string, unknown> = {}, index: Record<string, unknown> = {}) => ({
  progress: { phase, rootsTotal: 2, rootsDone: 1, indexed: 5, skipped: 7, failed: 0, unsupportedArchives: 0, markedSourceDeleted: 0, message: null, ...extra },
  index: { enabled: true, documents: 1284, sourceDeleted: 3, bodyBytes: 31_457_280, ...index },
});

test('the index of session text: read as far as it can be trusted, and which buttons can be pressed', () => {
  assert.equal(indexValue(null), null);
  assert.equal(indexValue({ progress: {} }), null, 'no index in it');
  const bare = indexValue({ index: { enabled: true } })!;
  assert.equal(bare.progress.phase, 'idle', 'before the first run the backend reports no phase');
  assert.equal(bare.index.documents, 0);
  const running = indexValue(state('running'))!;
  assert.equal(isRunning(running), true);
  assert.deepEqual(indexButtons(running, false), { start: false, stop: true, forget: false });
  assert.deepEqual(indexButtons(indexValue(state('done'))!, false), { start: true, stop: false, forget: true });
  assert.deepEqual(indexButtons(indexValue(state('done'))!, true), { start: false, stop: false, forget: false }, 'one operation at a time');
  assert.deepEqual(indexButtons(indexValue(state('idle', {}, { enabled: false, documents: 0 }))!, false), { start: false, stop: false, forget: false });
  const lines = indexLines(indexValue(state('running', { failed: 2, message: 'one archive was busy' }))!);
  assert.equal(lines[0], '1,284 sessions indexed · 3 with deleted source · 30.0 MiB');
  assert.equal(lines[1], 'Indexing: 1/2 archives · 5 read · 7 unchanged · 2 unreadable');
  assert.equal(lines[2], 'one archive was busy');
  assert.equal(indexLines(indexValue(state('idle', {}, { enabled: false }))!)[0], 'Indexing is off');
  assert.equal(indexLines(null)[0], 'Index not loaded yet.');
  assert.equal(indexSummary(indexValue(state('done'))!), '1,284 sessions indexed');
  assert.equal(indexSummary(indexValue(state('idle', {}, { enabled: false, documents: 0 }))!), 'Indexing is off.');
  assert.match(indexReason({ issue: 'invalid_request' }), /turn indexing on/);
  assert.match(indexReason('x'), /could not be completed/);
});

test('what a refused operation on a thread says, and which names are kept', () => {
  assert.match(operationFailure({ code: 'owned_elsewhere' }, 'deleted'), /A session of this thread is running/);
  assert.match(operationFailure('open_unverified', 'deleted'), /could not be verified/);
  assert.equal(operationFailure({ code: 'conflict' }, 'renamed', 'session'), 'This session changed. Reload and try again.');
  assert.equal(operationFailure('busy', 'renamed'), 'Thread storage is busy. Try again.');
  assert.equal(operationFailure(new Error('x'), 'renamed', 'session'), 'The session could not be renamed. Nothing was changed.');
  assert.equal(cleanName('  Billing  '), 'Billing');
  assert.equal(cleanName('   '), null);
  assert.equal(cleanName('x'.repeat(257)), null);
  assert.equal(cleanName('bad\u0007name'), null);
});
