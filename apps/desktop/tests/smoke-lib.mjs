// What the smokes of the page share: the browser, the dev server with the mocked commands, and what a page is allowed to say.
//
// The browser is Playwright's own Chromium on a developer's machine, and Edge on the Windows runner (`SHINATAMA_BROWSER_CHANNEL=msedge`,
// set by scripts/verify.ps1): the runner has Edge, and no step of the CI downloads a browser.
//
// A page may say nothing: an uncaught error, a console error and a console warning are failures (but Chromium's own noise, below).
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { createServer } from 'vite';

const here = fileURLToPath(new URL('.', import.meta.url));

/** The directory of the tests, with the trailing slash. */
export const testsDir = here;

/** A browser. `options` are Playwright's (`args`...); the channel comes from the environment. */
export function launch(options = {}) {
  const channel = process.env['SHINATAMA_BROWSER_CHANNEL'];
  return chromium.launch({ ...options, ...(channel ? { channel } : {}) });
}

/** The dev server of the app (the page with the mocked commands), with no watcher (an edit must not reload a page in the middle of a check)
 *  and no echo of the page's console in the terminal (the smokes read it themselves). Returns the server and its address. */
export async function serve(port) {
  const server = await createServer({
    configFile: `${here}../vite.config.ts`,
    server: { port, strictPort: false, watch: null, forwardConsole: false },
    logLevel: 'error',
  });
  await server.listen();
  return { server, url: server.resolvedUrls.local[0] };
}

/** Chromium's own warning (not the page's) when its scheduler is late on a loaded machine; it says nothing of the app. */
export const BROWSER_NOISE = /^task queue exceeded allotted deadline/;

/** What a message of the console says, its `%s` filled: Chromium hands the format and the values apart. */
async function said(message) {
  const values = await Promise.all(message.args().map((arg) => arg.jsonValue().catch(() => '?')));
  const [format, ...rest] = values;
  return typeof format === 'string' ? format.replace(/%[sdifoO]/g, () => String(rest.shift())) : message.text();
}

/** Whether a message of the console is a failure: every error and warning is, but the browser's own noise. */
export function isFailure(type, text) {
  return (type === 'error' || type === 'warning') && !BROWSER_NOISE.test(text);
}

/** Counts what a page says as failures of the smoke, named by `name`. */
export function watch(page, name, failures) {
  page.on('pageerror', (error) => failures.push(`${name}: ${error}`));
  page.on('console', async (message) => {
    if (message.type() !== 'error' && message.type() !== 'warning') return;
    const text = await said(message);
    if (isFailure(message.type(), text)) failures.push(`${name}: console ${message.type()}: ${text}`);
  });
}
