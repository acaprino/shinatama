// The threads of the sidebar on the built exe, Windows only (ADR-0055): a session opened in a project lives in a thread of its own; the thread is
// renamed in its panel and the name is the one Rust keeps; an empty thread is archived, listed under "Show archived threads" and unarchived; a
// thread with a running session asks before it is turned off, a no keeps it on, a yes stops the session and leaves its tab "not running", and
// the thread is on again after a reload without starting anything. Driven by roles and names, with the page's own `invoke` for what Rust keeps.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/threads.mjs
// It stands for legacy threads-smoke, thread-rename-smoke and thread-sets-smoke (the sidebar's width and collapse are Imago's now, kept under the
// app's own storage key); thread-recovery-smoke is stood for by the unit tests of the opens and by crates/state (recovery), see docs/desktop-ui.md.
import { check, clearNotices, finish, fixtureFolder, openApp, pathKey, startSession, tabOf, until } from './lib.mjs';

const fixture = await fixtureFolder('threads');
let handle;
try {
  handle = await openApp(fixture);
  const { page, ipc } = handle;
  const threads = page.getByRole('navigation', { name: 'Threads' });
  const worksets = async (projectId, archived = false) => (await ipc('workset_list', { request: { projectId, archived } })).rows;

  // A shell in the project: a thread of its own is on, with the session in a tab.
  await (await startSession(handle, { project: fixture.project, harness: 'powershell' })).done(30000);
  const project = (await ipc('project_recent')).find((item) => pathKey(item.path) === pathKey(fixture.project));
  check('the project is recorded', !!project);
  const [session] = await ipc('terminal_list');
  check('one terminal runs, and its tab is in a pane', !!session && await tabOf(page, session.id).isVisible());
  const [thread] = await until(async () => { const rows = await worksets(project.id); return rows.length === 1 ? rows : null; }, 15000, 'The thread was not listed.');
  check('the thread has one session and is open', thread.sessions === 1 && thread.open >= 1, JSON.stringify({ sessions: thread.sessions, open: thread.open }));

  // Renamed in its panel: Rust keeps the name and the sidebar shows it.
  await clearNotices(page);
  await page.getByRole('button', { name: /^Settings of / }).first().click();
  const name = page.getByRole('form', { name: 'Name of the thread' });
  await name.getByRole('textbox', { name: 'Name' }).fill('Renamed by the probe');
  await name.getByRole('button', { name: 'Save' }).click();
  check('the name is the one Rust keeps', !!await until(async () => (await worksets(project.id))[0]?.title === 'Renamed by the probe', 10000, 'not renamed').catch(() => null));
  check('and the sidebar shows it', !!await until(() => threads.getByRole('button', { name: /^Renamed by the probe/ }).first().isVisible(), 10000, 'not shown').catch(() => null));
  await page.getByRole('complementary', { name: 'Thread settings' }).getByRole('button', { name: 'Close', exact: true }).click();

  // An empty thread is archived and comes back.
  await threads.getByRole('button', { name: /^New thread in / }).click();
  await until(async () => (await worksets(project.id)).length === 2, 15000, 'The new thread was not made.');
  const empty = threads.getByRole('button', { name: /^New thread/ }).first();
  await empty.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Archive' }).click();
  check('an archived thread leaves the list', !!await until(async () => (await worksets(project.id)).length === 1 && (await worksets(project.id, true)).length === 1, 10000, 'not archived').catch(() => null));
  await threads.getByRole('checkbox', { name: 'Show archived threads' }).check();
  check('the archived list shows it, as archived', !!await until(async () => /Archived/.test(await threads.getByRole('button', { name: /^New thread/ }).first().textContent()), 10000, 'not shown').catch(() => null));
  await threads.getByRole('button', { name: /^New thread/ }).first().click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Unarchive' }).click();
  await threads.getByRole('checkbox', { name: 'Show archived threads' }).uncheck();
  check('Unarchive brings it back', !!await until(async () => (await worksets(project.id, true)).length === 0, 10000, 'still archived').catch(() => null));

  // A thread with a running session asks before it is turned off.
  const row = threads.getByRole('button', { name: /^Renamed by the probe/ }).first();
  await row.click();
  const ask = page.getByRole('alertdialog').or(page.getByRole('dialog')).filter({ hasText: /Turn the thread off\?/ });
  await ask.waitFor();
  await ask.getByRole('button', { name: 'Cancel' }).click();
  check('a no leaves the session running', (await ipc('terminal_list')).length === 1 && await tabOf(page, session.id).isVisible());
  await row.click();
  await ask.getByRole('button', { name: /^Turn the thread off/ }).click();
  check('a yes stops the session', !!await until(async () => (await ipc('terminal_list')).length === 0, 20000, 'still running').catch(() => null));
  await row.click();
  check('on again, its tab says it does not run', !!await until(() => page.getByRole('tab', { name: /not running/ }).first().isVisible(), 15000, 'no tab').catch(() => null));
  check('and nothing was started', (await ipc('terminal_list')).length === 0);

  // The thread that was on is on after a reload, and still starts nothing.
  await page.reload();
  await page.getByText(/^Local runtime/).waitFor({ timeout: 30000 });
  check('after a reload the thread is on', !!await until(async () => (await threads.getByRole('button', { name: /^Renamed by the probe/ }).first().getAttribute('aria-pressed')) === 'true', 15000, 'not on').catch(() => null));
  check('and no terminal was started by it', (await ipc('terminal_list')).length === 0);
  check('the page said nothing wrong', handle.errors.length === 0, handle.errors.join(' | '));
} catch (error) {
  check('the probe ran to its end', false, String(error.message ?? error));
} finally {
  await handle?.stop();
  await fixture.remove();
}
finish();
