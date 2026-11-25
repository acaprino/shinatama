// The project catalog on the built exe, Windows only (C04): roots that hold projects are added in the Projects dialog and saved at once, their
// folders are listed and filtered, a project is chosen and becomes the most recent, and a restart keeps the roots and starts no terminal by itself.
// The native folder chooser (F3) is picker.mjs. Roles and names only; the page's own `invoke` for what Rust keeps.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/projects.mjs
// It stands for legacy catalog-smoke and launch-preflight-smoke: the launch review of the engine is read in the dialog of a new session, and the
// CLI's own answer is proved by crates/cli/tests/launch.rs.
import { mkdir, utimes } from 'node:fs/promises';
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { check, clearNotices, command, finish, fixtureFolder, openApp, pathKey, until } from './lib.mjs';

const fixture = await fixtureFolder('catalog');
const base = realpathSync.native(fixture.folder);
const rootA = resolve(base, 'root-a');
const rootB = resolve(base, 'root-b');
await mkdir(rootA); await mkdir(rootB);
for (let index = 0; index < 12; index++) { const path = resolve(rootA, `alpha-${String(index).padStart(3, '0')}`); await mkdir(path); await utimes(path, 1700000000 + index, 1700000000 + index); }
for (let index = 0; index < 3; index++) { const path = resolve(rootB, `beta-${index}`); await mkdir(path); await utimes(path, 1700001000 + index, 1700001000 + index); }
let handle;
try {
  handle = await openApp(fixture);
  const { page, ipc } = handle;
  const projects = () => page.getByRole('dialog', { name: 'Projects' });
  await clearNotices(page);
  await command(page, 'Choose the project');
  await projects().waitFor();

  // Roots are saved the moment they are added; the first one is the default.
  for (const root of [rootA, rootB]) {
    await projects().getByRole('textbox', { name: 'Root path' }).fill(root);
    await projects().getByRole('button', { name: 'Add root' }).click();
    await projects().getByRole('button', { name: `Remove root ${root}` }).waitFor({ timeout: 20000 });
  }
  check('the second root can be made the default, the first already is', await projects().getByRole('button', { name: `Make default: ${rootB}` }).isVisible() && await projects().getByRole('button', { name: `Make default: ${rootA}` }).count() === 0);

  // The folders of the roots are listed, and the filter narrows them as it is typed.
  await projects().getByText(/matching projects/).first().waitFor({ timeout: 20000 });
  await projects().getByRole('searchbox', { name: 'Filter projects' }).fill('beta-2');
  check('the filter keeps the one project that matches', !!await until(async () => /^1 matching projects/.test((await projects().getByText(/matching projects/).first().textContent()).trim()), 10000, 'no count').catch(() => null));
  await projects().getByRole('button', { name: /^beta-2/ }).click();
  await projects().waitFor({ state: 'detached', timeout: 20000 });
  const recent = await ipc('project_recent');
  check('the project chosen is the most recent', pathKey(recent[0]?.path) === pathKey(resolve(rootB, 'beta-2')), recent[0]?.path);
  check('choosing a project starts no terminal', (await ipc('terminal_list')).length === 0);

  // A restart keeps the roots, and again starts nothing by itself.
  await handle.stop();
  handle = await openApp(fixture);
  await clearNotices(handle.page);
  await command(handle.page, 'Choose the project');
  const again = handle.page.getByRole('dialog', { name: 'Projects' });
  await again.waitFor();
  for (const root of [rootA, rootB]) check(`the root ${root.split('\\').pop()} is still there`, !!await until(() => again.getByRole('button', { name: `Remove root ${root}` }).isVisible(), 15000, 'root gone').catch(() => null));
  check('and no terminal started with the window', (await handle.ipc('terminal_list')).length === 0);

  // The launch review of the engine is read in the dialog of a new session, and starts nothing.
  await handle.page.keyboard.press('Escape');
  await again.waitFor({ state: 'detached' });
  await command(handle.page, 'New session');
  const session = handle.page.getByRole('dialog', { name: 'New session' });
  await session.waitFor();
  await session.getByText('Launch review').click();
  await session.getByRole('button', { name: 'Review the launch' }).click();
  check('the review says what the engine says', !!await until(() => session.getByText(/^The engine (would start this agent|refuses this launch|cannot finish this review yet)\./).first().isVisible(), 30000, 'no review').catch(() => null));
  check('and no terminal was started by it', (await handle.ipc('terminal_list')).length === 0);
  check('the page said nothing wrong', handle.errors.length === 0, handle.errors.join(' | '));
} catch (error) {
  check('the probe ran to its end', false, String(error.message ?? error));
} finally {
  await handle?.stop();
  await fixture.remove();
}
finish();
