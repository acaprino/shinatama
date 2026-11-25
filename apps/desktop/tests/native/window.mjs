// The window itself, on the built exe, Windows only: what the browser smokes cannot prove because they run without a window of Shinatama's own.
// The caption buttons, the zoom the shell controls with the keys and the picker of Settings, the glass (a command of the palette and a switch of
// Settings that only a window of its own has), the scheme kept across a reload, and the close that ends the program with code 0.
// The look and the frame are Imago's and are proved by its own verification (docs/verification.md of @acaprino/imago): Snap Layouts on the maximize
// button, WebView2 recovery, the first frame. This probe checks that Shinatama wired them: the buttons are there, the commands exist.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/window.mjs
// It stands for legacy native-frame-smoke, desktop-smoke (frame and zoom), storage-smoke (window controls) and glass-capture.
import { check, closeWindow, command, finish, fixtureFolder, goTo, openApp, until } from './lib.mjs';

const fixture = await fixtureFolder('window');
let handle;
try {
  handle = await openApp(fixture);
  const { page } = handle;
  await page.evaluate(() => { window.__probeNotReloaded = true; });
  const bar = page.locator('[data-slot="title-bar"]');

  // The three caption buttons of a window of its own: a browser has none.
  for (const name of ['Minimize', 'Maximize']) check(`the title bar has ${name}`, await bar.getByRole('button', { name, exact: true }).isVisible());
  check('and Close', await bar.getByRole('button', { name: 'Close', exact: true }).isVisible());
  await bar.getByRole('button', { name: 'Maximize', exact: true }).click();
  check('Maximize becomes Restore', !!await until(() => bar.getByRole('button', { name: 'Restore', exact: true }).isVisible(), 5000, 'no Restore').catch(() => null));
  await bar.getByRole('button', { name: 'Restore', exact: true }).click();
  check('and Restore gives it back', !!await until(() => bar.getByRole('button', { name: 'Maximize', exact: true }).isVisible(), 5000, 'no Maximize').catch(() => null));

  // The title of the window says the view.
  await goTo(page, 'Sessions');
  check('the title says the view', !!await until(() => page.title().then((title) => title === 'Sessions - Shinatama'), 5000, 'title').catch(() => null), await page.title());
  await goTo(page, 'Settings');
  await page.getByRole('region', { name: 'Appearance' }).waitFor();

  // Glass is the person's choice, in a window of its own: a command of the palette and a switch of Settings, the same choice, at once, with no reload.
  const glass = page.getByRole('switch', { name: 'Glass window' });
  check('Settings draws the glass switch in a window of its own', await glass.isVisible());
  const was = await glass.isChecked();
  await glass.click();
  check('turning glass on or off changes the switch', !!await until(async () => await glass.isChecked() !== was, 5000, 'no change').catch(() => null));
  check('and does not reload the page', await page.evaluate(() => window.__probeNotReloaded === true));
  await page.getByRole('button', { name: /Commands/ }).first().click();
  check('the palette offers Glass, checked as the switch is', await page.getByRole('option', { name: /^Glass/ }).count() === 1);
  await page.keyboard.press('Escape');
  await glass.click();

  // The zoom is the shell's: the picker is enabled in a window and follows the keys.
  const zoom = page.getByRole('combobox', { name: 'Interface zoom' });
  check('the zoom picker is enabled', await zoom.isEnabled());
  await page.keyboard.press('Control+=');
  check('Ctrl+= raises the zoom to 110%', !!await until(async () => (await zoom.textContent())?.trim() === '110%', 5000, 'no zoom').catch(() => null), (await zoom.textContent())?.trim());
  await page.keyboard.press('Control+0');
  check('Ctrl+0 gives 100% back', !!await until(async () => (await zoom.textContent())?.trim() === '100%', 5000, 'no reset').catch(() => null));

  // The scheme is kept: the switch, then a reload.
  await page.getByRole('button', { name: /^Switch to (light|dark)$/ }).click();
  const scheme = await page.evaluate(() => document.documentElement.dataset['scheme']);
  await page.reload();
  await page.getByText(/^Local runtime/).waitFor({ timeout: 30000 });
  check('the scheme chosen is the one drawn after a reload', await page.evaluate(() => document.documentElement.dataset['scheme']) === scheme, scheme);
  await command(page, 'Dark theme');

  check('the page said nothing wrong', handle.errors.length === 0, handle.errors.join(' | '));
  const code = await closeWindow(handle);
  check('Close ends the program with code 0', code === 0, `code ${code}`);
} catch (error) {
  check('the probe ran to its end', false, String(error.message ?? error));
} finally {
  await handle?.stop();
  await fixture.remove();
}
finish();
