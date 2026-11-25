// What the native probes share (Windows only, the built exe, driven over the WebView2 debugging port): the app on a private data folder, the
// page's own `invoke` for what the page cannot show, and the few handles the page offers by id (a terminal's tab is `data-tab-id`, because the
// session ids come from Rust and nothing else names a tab by id). Everything else is found by role and name, as a person and a screen reader
// find it: never a class.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/<probe>.mjs
// None of these has been run in a cloud session (there is no Windows there): they wait for the Windows runner and for a real window.
import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'win32') throw new Error('This probe needs Windows: it drives the built exe.');

export const root = process.cwd();
export { delay };

export const freePort = async () => {
  const reservation = createServer();
  await new Promise((accept, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', accept); });
  const found = reservation.address().port;
  await new Promise((accept, reject) => reservation.close((error) => error ? reject(error) : accept()));
  return found;
};

/** Polls `read` until it gives something, or throws `message`. */
export async function until(read, milliseconds, message, every = 250) {
  for (const deadline = Date.now() + milliseconds; ;) {
    const value = await Promise.resolve().then(read).catch(() => null);
    if (value) return value;
    if (Date.now() >= deadline) throw new Error(message);
    await delay(every);
  }
}

/** The key of a path as Windows compares it: no `\\?\` prefix, no trailing slash, any case. */
export const pathKey = (path) => String(path ?? '').replace(/^\\\\\?\\/, '').replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();

const results = [];
/** Says what was proved; the probe fails at the end when one was not. */
export function check(name, ok, detail = '') {
  results.push(!!ok);
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`);
}
export function finish() {
  const failed = results.filter((ok) => !ok).length;
  console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
  process.exitCode = failed ? 1 : 0;
}

/** A folder of the probe that is removed at the end: the data folder, the web profile, and whatever the probe puts in it. */
export async function fixtureFolder(name) {
  const folder = await mkdtemp(resolve(tmpdir(), `shinatama-${name}-`));
  await mkdir(resolve(folder, 'project'), { recursive: true });
  return { folder, project: resolve(folder, 'project'), data: resolve(folder, 'data'), remove: () => rm(folder, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 }).catch(() => undefined) };
}

/** Starts the exe on the fixture's data folder and returns what a probe drives it with. `env` is added to the process's. */
export async function openApp(fixture, { env = {}, width, height } = {}) {
  const port = await freePort();
  let diagnostics = '';
  const app = spawn(process.env['SHINATAMA_EXE'] ?? resolve(root, 'target/debug/shinatama-desktop.exe'), [], {
    cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
    env: {
      ...process.env, SHINATAMA_HOME: fixture.data, WEBVIEW2_USER_DATA_FOLDER: resolve(fixture.folder, 'webview-profile'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`, ...env,
    },
  });
  app.stderr.on('data', (chunk) => { diagnostics = (diagnostics + chunk).slice(-16384); });
  const browser = await until(() => chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }), 30000, `WebView2 unavailable. ${diagnostics}`);
  // The app also runs the attention widget (widget.html) in a second WebView of the same browser: the main page is the other one.
  const page = await until(() => browser.contexts()[0].pages().find((one) => one.url().includes('tauri.localhost') && !one.url().includes('widget.html')), 30000, 'The main window did not appear.');
  page.setDefaultTimeout(15000);
  if (width && height) await page.setViewportSize({ width, height });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message.slice(0, 1024)));
  await page.getByText(/^Local runtime/).waitFor({ timeout: 30000 });

  /** A command of Rust through the page's own `invoke`; a refusal that says `busy` is a wait, not a failure. */
  const ipc = async (command, args = {}) => {
    for (const deadline = Date.now() + 15000; ;) {
      const result = await page.evaluate(async ({ command, args }) => {
        try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(command, args) }; } catch (issue) { return { ok: false, issue }; }
      }, { command, args });
      if (result.ok) return result.value;
      if (result.issue?.code !== 'busy' || Date.now() > deadline) throw new Error(`${command} refused (${result.issue?.code ?? JSON.stringify(result.issue)}).`);
      await delay(100);
    }
  };
  const exited = () => app.exitCode !== null || app.signalCode !== null
    ? Promise.resolve()
    : new Promise((accept) => app.once('exit', accept));
  const stop = async () => {
    await page.evaluate(() => window.__TAURI_INTERNALS__?.invoke('terminal_shutdown')).catch(() => undefined);
    await browser.close().catch(() => undefined);
    if (app.exitCode === null && app.signalCode === null) { const gone = exited(); app.kill(); await gone; }
  };
  return { app, browser, page, ipc, errors, exited, stop, diagnostics: () => diagnostics };
}

// --- the window, by roles --------------------------------------------------------------------------------------------------------------------

/** The view of the title bar (a button from 860px, a tab in the row under it). */
export const goTo = (page, name) => page.getByRole('button', { name, exact: true }).or(page.getByRole('tab', { name, exact: true })).first().click();

/** Runs a command by its name in the palette (Ctrl+K). */
export async function command(page, name) {
  await page.getByRole('button', { name: /Commands/ }).first().click();
  await page.getByRole('combobox', { name: /command/i }).fill(name);
  await page.keyboard.press('Enter');
  await page.getByRole('combobox', { name: /command/i }).waitFor({ state: 'detached' });
}

/** The notices of the window, put away so that none sits over what a step presses. */
export async function clearNotices(page) {
  const close = page.getByRole('status', { name: 'Notices' }).getByRole('button');
  for (let n = await close.count(), tries = 0; n > 0 && tries < 8; n = await close.count(), tries += 1) { await close.first().click({ timeout: 1500 }).catch(() => {}); await delay(100); }
}

/** The text of the newest error notice, or null. An error stays until it is closed (Sigrid marks the notice of an error with its icon). */
export async function lastError(page) {
  const errors = page.locator('[data-scope="toast"]').filter({ has: page.locator('[data-slot="toast-error"]') });
  const text = await errors.last().textContent().catch(() => null);
  return text?.replace(/\s+/g, ' ').trim() || null;
}

/** The close of the window, by its button in the title bar: the page goes, and the program ends with code 0. */
export async function closeWindow(handle) {
  const closing = Promise.all([handle.page.waitForEvent('close', { timeout: 15000 }), handle.exited()]);
  void closing.catch(() => {});
  await handle.page.locator('[data-slot="title-bar"]').getByRole('button', { name: 'Close', exact: true }).click();
  await closing;
  return handle.app.exitCode;
}

// --- a session, a terminal and a thread ------------------------------------------------------------------------------------------------------

/** The tab of a session by the id Rust gave it, and the panel of that tab. */
export const tabOf = (page, id) => page.locator(`[role="tab"][data-tab-id="${id}"]`);
export const paneOf = (page, id) => page.locator(`[role="tabpanel"][aria-labelledby$="-tab-${id}"]`);

/** The text a terminal shows (what a screen reader reads of its rows), after bringing its tab forward. */
export async function screenOf(page, id) {
  await tabOf(page, id).click();
  return paneOf(page, id).textContent();
}

/** Types into a terminal as a person does (key events, not an insertion) and presses `key`. */
export async function typeInto(page, id, text, key) {
  await tabOf(page, id).click();
  const field = paneOf(page, id).getByRole('textbox', { name: /^Terminal of / });
  await field.focus();
  if (text) await field.pressSequentially(text, { delay: 5 });
  if (key) await page.keyboard.press(key);
}

/** Whether the process of a terminal has ended: its tab says so ("Exited"), and offers a new session. */
export const hasExited = async (page, id) => (await paneOf(page, id).getByText(/^Exited/).count()) > 0;

/** Stops a session by its tab's close, answering the question that asks first. */
export async function stopSession(page, id) {
  await tabOf(page, id).click();
  await tabOf(page, id).locator('xpath=..').getByRole('button', { name: /^Close / }).click();
  await page.getByRole('alertdialog').or(page.getByRole('dialog')).getByRole('button', { name: /^(Stop terminal|Close session)$/ }).click();
}

/**
 * Starts a session as a person does: the command New session, the agent, the folder, Open. Returns once Open is pressed, without waiting for
 * the session: `done()` waits for it (and throws the product's words if it was refused), `refusal()` says the error notice, `pending()` whether
 * it is still opening. A session is told by the terminals Rust holds.
 */
export async function startSession(handle, { project, harness, label }) {
  const { page, ipc } = handle;
  const before = (await ipc('terminal_list')).length;
  await clearNotices(page);
  await command(page, 'New session');
  const dialog = page.getByRole('dialog', { name: 'New session' });
  await dialog.waitFor();
  await dialog.getByRole('button', { name: label ?? harnessLabel(harness), exact: true }).click();
  await dialog.getByRole('textbox', { name: 'Project folder' }).fill(project);
  await dialog.getByRole('button', { name: 'Open', exact: true }).click();
  const flow = {
    refusal: () => lastError(page),
    pending: async () => await dialog.isVisible() || ((await ipc('terminal_list')).length <= before && !await lastError(page)),
    done: (milliseconds = 90000) => until(async () => {
      const refused = await flow.refusal();
      if (refused) throw new Error(`New session was refused: ${refused.slice(0, 300)}`);
      return !await flow.pending();
    }, milliseconds, 'New session did not finish opening before its deadline.'),
  };
  return flow;
}
const harnessLabel = (id) => ({ claude_code: 'Claude Code', codex: 'Codex', opencode: 'OpenCode', pi: 'Pi', powershell: 'PowerShell' })[id] ?? id;

/** The row of a thread in the sidebar, by the title it shows. */
export const threadRow = (page, title) => page.getByRole('navigation', { name: 'Threads' }).getByRole('button', { name: new RegExp(`^${title.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`) }).first();

/** The card of a session that does not run (its tab is `saved-<thread>`): Resume, or Recover session. */
export const savedCard = (page, threadId) => paneOf(page, `saved-${threadId}`);

/**
 * The Resume (or Reopen) button of a closed session: a session that was closed waits under Closed sessions in the panel of its thread (the
 * thread's gear in the sidebar). The proofs have one project and one thread. Returns the button, or throws when it does not appear.
 */
export async function resumeClosed(page, { wait = 10000 } = {}) {
  await page.getByRole('button', { name: /^Settings of / }).first().click();
  const button = page.getByRole('button', { name: /^(Resume|Reopen) / }).first();
  await button.waitFor({ timeout: wait });
  return button;
}
