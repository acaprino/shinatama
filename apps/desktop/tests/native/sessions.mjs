// The Sessions view on the built exe, Windows only (C22 to C24, C27, F04): a read-only archive of synthetic Claude sessions is registered, its
// sessions are listed in the table and narrowed by the filter, the facts of one open in the inspector, its conversation is read as text (literal
// HTML never runs, the reasoning and the raw entry never show), the index of session text is built and Enter searches it, a hop shows the exact seed
// it would send and starts nothing, and the running sessions say what the instances overview observes. Nothing here starts an agent.
// The archive is registered with the page's own `invoke` (the form that does it in the Archive browser is proved by the mock smoke).
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/sessions.mjs
// It stands for legacy sessions-smoke, session-search-smoke, hop-preview-smoke and instances-smoke; the reader of the archives, the index and the
// seed are Rust's, proved by crates/cli/tests/sessions.rs and crates/engine/tests/session_reader.rs.
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { check, clearNotices, finish, fixtureFolder, goTo, openApp, startSession, toggle, until } from './lib.mjs';

const fixture = await fixtureFolder('sessions');
const archive = resolve(fixture.folder, 'synthetic-claude');
const folder = resolve(archive, 'projects/fixture-project');
await mkdir(folder, { recursive: true });
const SAFE = 'SHINATAMA_DETAIL_SAFE_TEXT';
const RAW = 'SHINATAMA_DETAIL_RAW_NEVER_RENDER';
const REASONING = 'SHINATAMA_DETAIL_REASONING_NEVER_RENDER';
const HTML = '<img id="detail-untrusted-image" src="data:," onerror="globalThis.__sessionDetailExecuted=true"><script>globalThis.__sessionDetailExecuted=true</script>';
const detail = (id) => {
  const records = []; let parent = null;
  for (let turn = 0; turn < 3; turn++) {
    const suffix = String(turn).padStart(2, '0');
    records.push({ type: 'user', uuid: `entry-user-${suffix}`, parentUuid: parent, sessionId: id, cwd: 'C:\\synthetic-session-project', timestamp: `2026-11-03T00:00:${suffix}Z`, rawOnlySentinel: RAW,
      message: { role: 'user', content: turn === 0 ? `${SAFE} ${HTML}` : `SHINATAMA_DETAIL_USER_${suffix}` } });
    records.push({ type: 'assistant', uuid: `entry-assistant-${suffix}`, parentUuid: `entry-user-${suffix}`, sessionId: id,
      message: { id: `message-${suffix}`, role: 'assistant', model: 'fixture-model', stop_reason: 'end_turn',
        content: [{ type: 'text', text: `SHINATAMA_DETAIL_ASSISTANT_${suffix}` }, ...(turn === 0 ? [{ type: 'thinking', thinking: REASONING, signature: 'opaque' }] : [])] } });
    parent = `entry-assistant-${suffix}`;
  }
  return `${records.map((record) => JSON.stringify(record)).join('\n')}\n`;
};
await writeFile(resolve(folder, 'idsession-00.jsonl'), detail('idsession-00'));
for (const id of ['idsession-01', 'idsession-02']) await writeFile(resolve(folder, `${id}.jsonl`), `${JSON.stringify({ sessionId: id, cwd: 'C:\\synthetic-session-project' })}\n`);

let handle;
try {
  handle = await openApp(fixture);
  const { page, ipc } = handle;
  const root = await ipc('session_archive_add', { request: { harness: 'claude_code', path: archive } });
  check('the archive is registered as read-only', root.access === 'read_only' && root.harness === 'claude_code');
  await ipc('session_index_enable', { enabled: true });
  await clearNotices(page);
  await goTo(page, 'Sessions');
  const table = page.getByRole('treegrid', { name: 'Sessions' });
  await table.waitFor();
  const rows = () => table.getByRole('row').filter({ hasText: /Claude Code/ });
  check('the saved sessions are listed', !!await until(async () => (await rows().count()) === 3, 30000, 'not listed').catch(() => null), String(await rows().count()));
  const filter = page.getByRole('searchbox', { name: /Filter sessions/ });
  await filter.fill('idsession-00');
  check('the filter narrows by the id', !!await until(async () => (await rows().count()) === 1, 10000, 'not narrowed').catch(() => null));
  check('and no agent or terminal was started', (await ipc('terminal_list')).length === 0);

  // The facts, then the conversation as text.
  await rows().first().click();
  const facts = page.getByRole('complementary', { name: 'Session', exact: true });
  await facts.getByRole('button', { name: 'Open conversation' }).click();
  const conversation = page.getByRole('complementary', { name: 'Session detail', exact: true });
  await conversation.getByText(SAFE, { exact: false }).first().waitFor({ timeout: 30000 });
  const shown = await conversation.innerText();
  check('literal HTML is text and never runs', await page.evaluate(() => globalThis.__sessionDetailExecuted !== true && !document.getElementById('detail-untrusted-image')));
  check('the reasoning and the raw entry are not rendered', !shown.includes(REASONING) && !shown.includes(RAW));
  await conversation.getByRole('button', { name: 'Close', exact: true }).click();
  await filter.fill('');

  // The index of session text: built from the archive, searched with Enter.
  await page.getByRole('button', { name: /^Search index/ }).click();
  const index = page.getByRole('complementary', { name: 'Search index', exact: true });
  await index.getByRole('button', { name: 'Index sessions' }).click();
  check('the index reads the archive', !!await until(async () => (await ipc('session_index_status')).index.documents >= 3, 60000, 'not indexed').catch(() => null));
  await index.getByRole('button', { name: 'Close', exact: true }).click();
  await filter.fill('SHINATAMA_DETAIL_USER_01');
  await filter.press('Enter');
  check('Enter searches the text of the conversations', !!await until(() => page.getByRole('list', { name: 'Matches in conversations' }).isVisible(), 30000, 'no matches').catch(() => null));
  await page.getByRole('button', { name: 'Clear search' }).click();
  await filter.fill('idsession-00');
  await rows().first().click();

  // A hop shows the exact seed it would send, and starts nothing.
  await facts.getByRole('button', { name: /^Hop/ }).click();
  const hop = page.getByRole('complementary', { name: 'Hop to another agent', exact: true });
  await toggle(page, hop.getByRole('radio', { name: /Codex/ }));
  await hop.getByRole('button', { name: 'Prepare preview' }).click();
  check('the preview shows the exact seed', !!await until(() => hop.getByLabel('Exact seed').isVisible(), 60000, 'no seed').catch(() => null));
  check('and the hop started no terminal', (await ipc('terminal_list')).length === 0);
  await hop.getByRole('button', { name: 'Close', exact: true }).click();

  // The running sessions: a shell of the app is one instance, with the state the overview observes.
  await (await startSession(handle, { project: fixture.project, harness: 'powershell' })).done(30000);
  await goTo(page, 'Sessions');
  await page.getByRole('button', { name: /^Running sessions/ }).click();
  const running = page.getByRole('complementary', { name: 'Running sessions', exact: true });
  check('the shell is listed with its facts', !!await until(() => running.getByRole('region').first().isVisible(), 30000, 'not listed').catch(() => null));
  check('the page said nothing wrong', handle.errors.length === 0, handle.errors.join(' | '));
} catch (error) {
  check('the probe ran to its end', false, String(error.message ?? error));
} finally {
  await handle?.stop();
  await fixture.remove();
}
finish();
