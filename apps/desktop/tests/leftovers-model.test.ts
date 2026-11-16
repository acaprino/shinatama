import test from 'node:test';
import assert from 'node:assert/strict';
import { countText, folderKey, itemText, leftoversFor, size, summaryText, type LeftoverItem, type Leftovers } from '../src/leftovers-model.ts';

const item = (category: string, extra: Partial<LeftoverItem> = {}): LeftoverItem => ({
  key: `k-${category}`, category, path: `C:\\w\\app-${category}`, label: null, bytes: 2048, bytesPartial: false,
  level: 'caution', blocked: [], reason: 'named like the project', firstSeenMs: 1, ...extra,
});
const picture = (extra: Partial<Leftovers> = {}): Leftovers => ({
  folders: ['c:\\w\\app'], phase: 'watching', root: 'C:\\w\\app', name: 'app', isRepo: true, newItems: [], newItemsOmitted: 0,
  growthBytes: 0, attention: [], stale: false, cleanCommand: null, cleanCommandTruncated: false,
  startedAtMs: null, checkedAtMs: null, message: null, ...extra,
});

test('a row finds the observer of its root or of one of its folders', () => {
  assert.equal(folderKey('C:/W/App/'), 'c:\\w\\app');
  assert.equal(folderKey('C:\\'), 'c:\\');
  const watched = picture({ folders: ['c:\\w\\app', 'c:\\w\\app-spike'] });
  assert.equal(leftoversFor('C:\\W\\App', [watched]), watched);
  assert.equal(leftoversFor('c:/w/app-spike/', [watched]), watched);
  assert.equal(leftoversFor('C:\\w\\other', [watched]), null);
});

test('the summary counts what appeared and says when nothing did', () => {
  assert.equal(summaryText(picture()), 'Leftovers: nothing new.');
  assert.equal(summaryText(picture({ phase: 'starting' })), 'Leftovers: Xiox is taking the first snapshot…');
  assert.equal(summaryText(picture({ phase: 'ended', message: 'error: path not found' })), 'Leftovers: Xiox stopped (error: path not found).');
  const busy = picture({ newItems: [item('worktrees'), item('worktrees'), item('named'), item('copies')], growthBytes: 3 * 1024 * 1024, newItemsOmitted: 2 });
  assert.equal(summaryText(busy), 'Leftovers: 6 new · +3.0 MB · 2 worktrees, 1 folder named like it, 1 copy, 2 more.');
  assert.equal(countText([item('sessions')]), '1 archived session');
});

test('an item line says where, how big, why, and what blocks it', () => {
  assert.equal(size(512), '512 B');
  assert.equal(itemText(item('named')), 'caution · C:\\w\\app-named · 2.0 KB · named like the project');
  assert.equal(
    itemText(item('copies', { bytesPartial: true, blocked: ['uncommitted changes'], reason: 'copy of the repository' })),
    'caution · C:\\w\\app-copies · at least 2.0 KB · copy of the repository · cannot be removed: uncommitted changes',
  );
  assert.equal(itemText(item('sessions', { path: null, label: 'Bench the parser', bytes: null })).split(' · ').slice(1, 3).join(' · '), 'Bench the parser · size unknown');
});
